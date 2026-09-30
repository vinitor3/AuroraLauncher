use crate::{
    model::{Metrics, valid_id},
    transport::{self, Bucket, Budget},
    wire::{self, Hello, InvitationProof, OpenRequest, Operation, Reply, SequenceWindow},
    *,
};
use bytes::Bytes;
use iroh::{
    Endpoint, EndpointId,
    endpoint::{Connection, RecvStream, SendStream},
};
use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    net::{TcpListener, UdpSocket},
    sync::{Mutex as AsyncMutex, Notify, OwnedSemaphorePermit, Semaphore},
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

struct Inner {
    endpoint: Endpoint,
    connection: Connection,
    limits: Limits,
    metrics: Arc<Metrics>,
    cancel: CancellationToken,
    open_lock: AsyncMutex<u64>,
    associations: GuestAssociations,
    budget: Budget,
    udp: Arc<Semaphore>,
    task: Mutex<Option<JoinHandle<()>>>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.connection.close(0u32.into(), b"guest closed");
    }
}
#[derive(Clone)]
pub struct Guest {
    inner: Arc<Inner>,
}

impl Guest {
    pub async fn connect(config: GuestConfig) -> Result<Self> {
        Self::connect_inner(config, None).await
    }
    /// Derive every peer/session/grant field from an already verified invitation.
    /// Validate our private key's public identity before opening any endpoint.
    pub async fn connect_invited(
        secret_key: SecretKey,
        invitation: &Invitation,
        limits: Limits,
    ) -> Result<Self> {
        if secret_key.public() != invitation.guest_key() {
            return Err(Error::Unauthorized);
        }
        invitation.check_current()?;
        let direct = invitation.direct_addr();
        let bind_addr = if direct.is_ipv4() {
            "127.0.0.1:0"
        } else {
            "[::1]:0"
        }
        .parse()
        .map_err(|_| Error::Configuration)?;
        Self::connect_inner(
            GuestConfig {
                secret_key,
                bind_addr,
                host: EndpointAddr::new(invitation.host_key()).with_ip_addr(direct),
                session_id: invitation.session_id().into(),
                lease_id: invitation.lease().lease_id,
                capabilities: invitation.capabilities(),
                limits,
            },
            Some(invitation.proof()),
        )
        .await
    }
    async fn connect_inner(
        config: GuestConfig,
        invitation: Option<InvitationProof>,
    ) -> Result<Self> {
        config.limits.validate()?;
        if !valid_id(&config.session_id)
            || !valid_id(&config.lease_id)
            || config.capabilities.len() > 2
        {
            return Err(Error::Configuration);
        }
        // Direct-address laboratory: never accept relay or custom transport addresses in tickets.
        if config.host.addrs.is_empty()
            || config
                .host
                .addrs
                .iter()
                .any(|a| !matches!(a, iroh::TransportAddr::Ip(_)))
        {
            return Err(Error::Configuration);
        }
        let endpoint =
            transport::endpoint(config.secret_key, config.bind_addr, &config.limits).await?;
        let attempt = tokio::time::timeout(config.limits.handshake_timeout, async {
            // Iroh verifies the authenticated remote key against EndpointAddr.id during TLS.
            let connection = endpoint
                .connect(config.host, ALPN)
                .await
                .map_err(|_| Error::Transport)?;
            let (mut send, mut recv) = connection.open_bi().await.map_err(|_| Error::Transport)?;
            let hello = Hello {
                schema_version: SCHEMA_VERSION,
                session_id: config.session_id,
                lease_id: config.lease_id,
                capabilities: config.capabilities,
                invitation,
            };
            wire::write_message(&mut send, &hello, config.limits.max_control_bytes).await?;
            send.finish().map_err(|_| Error::Transport)?;
            wire::read_message::<_, Reply>(&mut recv, config.limits.max_control_bytes)
                .await?
                .check()?;
            Ok::<_, Error>(connection)
        })
        .await
        .unwrap_or(Err(Error::Timeout));
        let connection = match attempt {
            Ok(c) => c,
            Err(error) => {
                endpoint.close().await;
                return Err(error);
            }
        };
        let cancel = CancellationToken::new();
        let associations = Arc::new(Mutex::new(HashMap::new()));
        let metrics = Arc::new(Metrics::default());
        let task = tokio::spawn(receive(
            connection.clone(),
            associations.clone(),
            metrics.clone(),
            cancel.clone(),
        ));
        Ok(Self {
            inner: Arc::new(Inner {
                endpoint,
                connection,
                cancel,
                associations,
                metrics,
                open_lock: AsyncMutex::new(0),
                budget: Arc::new(Mutex::new(Bucket::new(
                    config.limits.session_bytes_per_second,
                    65536,
                ))),
                udp: Arc::new(Semaphore::new(config.limits.max_udp_per_peer)),
                limits: config.limits,
                task: Mutex::new(Some(task)),
            }),
        })
    }
    pub fn id(&self) -> EndpointId {
        self.inner.endpoint.id()
    }
    pub fn connection(&self) -> &Connection {
        &self.inner.connection
    }
    pub fn metrics(&self) -> MetricsSnapshot {
        self.inner.metrics.snapshot()
    }
    pub fn max_datagram_payload(&self, service_max: usize) -> Option<usize> {
        transport::payload_limit(&self.inner.connection, service_max)
    }
    pub async fn shutdown(&self) {
        self.inner.cancel.cancel();
        self.inner.connection.close(0u32.into(), b"guest closed");
        let task = self.inner.task.lock().unwrap().take();
        if let Some(task) = task {
            let _ = task.await;
        }
        self.inner.endpoint.close().await;
    }
    async fn request(
        &self,
        service: &str,
        revision: u32,
        protocol: Protocol,
        operation: Operation,
        association_id: u64,
    ) -> Result<(SendStream, RecvStream, Reply)> {
        if !valid_id(service) {
            return Err(Error::Configuration);
        }
        let mut next = self.inner.open_lock.lock().await;
        if self.inner.cancel.is_cancelled() {
            return Err(Error::Closed);
        }
        if *next >= self.inner.limits.max_requests_per_connection {
            return Err(Error::Limit);
        }
        *next += 1;
        let request = OpenRequest {
            schema_version: SCHEMA_VERSION,
            request_id: *next,
            service_id: service.into(),
            service_revision: revision,
            protocol,
            operation,
            association_id,
        };
        tokio::select! {
            _ = self.inner.cancel.cancelled() => Err(Error::Closed),
            response = tokio::time::timeout(self.inner.limits.handshake_timeout, async {
                let (mut send, mut recv) = self.inner.connection.open_bi().await.map_err(|_| Error::Transport)?;
                wire::write_message(&mut send, &request, self.inner.limits.max_control_bytes).await?;
                let response: Reply = wire::read_message(&mut recv, self.inner.limits.max_control_bytes).await?;
                Ok((send, recv, response.check()?))
            }) => response.unwrap_or(Err(Error::Timeout)),
        }
    }
    pub async fn open_tcp(&self, service: &str, revision: u32) -> Result<(SendStream, RecvStream)> {
        let (send, recv, _) = self
            .request(service, revision, Protocol::Tcp, Operation::Open, 0)
            .await?;
        Ok((send, recv))
    }
    pub async fn open_udp(&self, service: &str, revision: u32) -> Result<UdpAssociation> {
        // Reserve before awaiting the control exchange: concurrent opens cannot bypass limits.
        let permit = self
            .inner
            .udp
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Limit)?;
        let (mut send, _recv, response) = self
            .request(service, revision, Protocol::Udp, Operation::Open, 0)
            .await?;
        send.finish().map_err(|_| Error::Transport)?;
        let max_payload = response.max_payload.ok_or(Error::CapabilityUnavailable)?;
        let state = Arc::new(AssociationState {
            service_handle: response.service_handle,
            generation: response.generation,
            max_payload,
            sequence: Mutex::new(SequenceWindow::default()),
            next_sequence: AtomicU32::new(0),
            inbox: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
            cancel: self.inner.cancel.child_token(),
            activity: Mutex::new(Instant::now()),
            packet_budget: Mutex::new(Bucket::new(
                self.inner.limits.datagrams_per_second,
                self.inner.limits.datagram_burst,
            )),
            byte_budget: Mutex::new(Bucket::new(self.inner.limits.udp_bytes_per_second, 65536)),
        });
        self.inner
            .associations
            .lock()
            .unwrap()
            .insert(response.association_id, state.clone());
        Ok(UdpAssociation {
            inner: Arc::new(AssociationHandle {
                guest: self.clone(),
                id: response.association_id,
                service: service.into(),
                revision,
                state,
                permit: Mutex::new(Some(permit)),
            }),
        })
    }
    pub async fn tcp_bridge(
        &self,
        service: &str,
        revision: u32,
        bind: SocketAddr,
    ) -> Result<TcpBridge> {
        if !bind.ip().is_loopback() || !valid_id(service) {
            return Err(Error::Configuration);
        }
        let listener = TcpListener::bind(bind).await.map_err(|_| Error::Io)?;
        let address = listener.local_addr().map_err(|_| Error::Io)?;
        let cancel = self.inner.cancel.child_token();
        let token = cancel.clone();
        let guest = self.clone();
        let service = service.to_owned();
        let task = tokio::spawn(async move {
            let permits = Arc::new(Semaphore::new(guest.inner.limits.max_tcp_per_peer));
            let mut tasks = JoinSet::new();
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = guest.inner.connection.closed() => break,
                    accepted = listener.accept() => {
                        let Ok((socket, _)) = accepted else { break; };
                        let Ok(permit) = permits.clone().try_acquire_owned() else { drop(socket); continue; };
                        let guest = guest.clone(); let service = service.clone(); let token = token.clone();
                        tasks.spawn(async move {
                            let _permit = permit;
                            if let Ok((send, recv)) = guest.open_tcp(&service, revision).await {
                                let _ = transport::bridge_tcp(socket, send, recv, guest.inner.limits.idle_timeout, token, guest.inner.budget.clone(), guest.inner.metrics.clone()).await;
                            }
                        });
                    }
                    _ = tasks.join_next(), if !tasks.is_empty() => (),
                }
            }
            tasks.shutdown().await;
        });
        Ok(TcpBridge {
            address,
            cancel,
            task: Mutex::new(Some(task)),
        })
    }
    pub async fn udp_bridge(
        &self,
        service: &str,
        revision: u32,
        bind: SocketAddr,
    ) -> Result<UdpBridge> {
        if !bind.ip().is_loopback() {
            return Err(Error::Configuration);
        }
        let socket = Arc::new(UdpSocket::bind(bind).await.map_err(|_| Error::Io)?);
        let address = socket.local_addr().map_err(|_| Error::Io)?;
        // Validate permission/capability now; reserve first association for the first local source.
        let first = self.open_udp(service, revision).await?;
        let negotiated_max = first.inner.state.max_payload;
        let cancel = self.inner.cancel.child_token();
        let token = cancel.clone();
        let guest = self.clone();
        let service = service.to_owned();
        let task = tokio::spawn(async move {
            let mut first = Some(first);
            let mut sources: HashMap<SocketAddr, UdpAssociation> = HashMap::new();
            let mut tasks = JoinSet::new();
            let mut buffer = vec![0u8; 65536];
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = guest.inner.connection.closed() => break,
                    received = socket.recv_from(&mut buffer) => {
                        let Ok((size, source)) = received else { break; };
                        if !source.ip().is_loopback() { continue; }
                        if sources.get(&source).is_some_and(|a| a.is_closed()) { sources.remove(&source); }
                        if let std::collections::hash_map::Entry::Vacant(entry) = sources.entry(source) {
                            let association = match first.take() {
                                Some(a) if !a.is_closed() => Ok(a),
                                _ => guest.open_udp(&service, revision).await,
                            };
                            let Ok(association) = association else { guest.inner.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed); continue; };
                            let returns = association.clone(); let socket = socket.clone(); let token = token.clone();
                            tasks.spawn(async move {
                                loop { tokio::select! {
                                    _ = token.cancelled() => break,
                                    data = returns.recv() => match data { Ok(data) => { let _ = socket.send_to(&data, source).await; }, Err(_) => break },
                                } }
                                returns.shutdown().await;
                            });
                            entry.insert(association);
                        }
                        let _ = sources[&source].send(&buffer[..size]).await;
                    }
                    _ = tasks.join_next(), if !tasks.is_empty() => { sources.retain(|_, a| !a.is_closed()); },
                }
            }
            let mut closing = JoinSet::new();
            if let Some(a) = first {
                closing.spawn(async move {
                    a.shutdown().await;
                });
            }
            for (_, a) in sources {
                closing.spawn(async move {
                    a.shutdown().await;
                });
            }
            while closing.join_next().await.is_some() {}
            tasks.shutdown().await;
        });
        Ok(UdpBridge {
            address,
            cancel,
            task: Mutex::new(Some(task)),
            guest: self.clone(),
            negotiated_max,
        })
    }
}

struct AssociationState {
    service_handle: u32,
    generation: u32,
    max_payload: usize,
    sequence: Mutex<SequenceWindow>,
    next_sequence: AtomicU32,
    inbox: Mutex<VecDeque<(Instant, Bytes)>>,
    notify: Notify,
    cancel: CancellationToken,
    activity: Mutex<Instant>,
    packet_budget: Mutex<Bucket>,
    byte_budget: Mutex<Bucket>,
}
type GuestAssociations = Arc<Mutex<HashMap<u64, Arc<AssociationState>>>>;
struct AssociationHandle {
    guest: Guest,
    id: u64,
    service: String,
    revision: u32,
    state: Arc<AssociationState>,
    permit: Mutex<Option<OwnedSemaphorePermit>>,
}
impl Drop for AssociationHandle {
    fn drop(&mut self) {
        self.state.cancel.cancel();
        self.guest
            .inner
            .associations
            .lock()
            .unwrap()
            .remove(&self.id);
    }
}
#[derive(Clone)]
pub struct UdpAssociation {
    inner: Arc<AssociationHandle>,
}
impl UdpAssociation {
    pub fn association_id(&self) -> u64 {
        self.inner.id
    }
    pub fn service_handle(&self) -> u32 {
        self.inner.state.service_handle
    }
    pub fn generation(&self) -> u32 {
        self.inner.state.generation
    }
    pub fn max_payload(&self) -> Option<usize> {
        self.inner
            .guest
            .max_datagram_payload(self.inner.state.max_payload)
    }
    pub fn is_closed(&self) -> bool {
        self.inner.state.cancel.is_cancelled()
            || self.inner.guest.inner.connection.close_reason().is_some()
            || self.inner.state.activity.lock().unwrap().elapsed()
                >= self.inner.guest.inner.limits.udp_idle_timeout
    }
    pub async fn send(&self, payload: &[u8]) -> Result<()> {
        let a = &self.inner.state;
        let g = &self.inner.guest.inner;
        if a.cancel.is_cancelled()
            || a.activity.lock().unwrap().elapsed() >= g.limits.udp_idle_timeout
        {
            return Err(Error::Closed);
        }
        let max = self.max_payload().ok_or(Error::CapabilityUnavailable)?;
        if payload.len() > max {
            g.metrics.udp_oversize.fetch_add(1, Ordering::Relaxed);
            return Err(Error::DatagramTooLarge);
        }
        if !a.packet_budget.lock().unwrap().take(1)
            || !a
                .byte_budget
                .lock()
                .unwrap()
                .take(payload.len() + DATAGRAM_HEADER_LEN)
            || !g
                .budget
                .lock()
                .unwrap()
                .take(payload.len() + DATAGRAM_HEADER_LEN)
        {
            g.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
            return Err(Error::Limit);
        }
        let sequence = a
            .next_sequence
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Limit)?;
        let frame = DatagramFrame {
            service_handle: a.service_handle,
            association_id: self.inner.id,
            generation: a.generation,
            sequence,
            payload: Bytes::copy_from_slice(payload),
        };
        let encoded = frame.encode()?;
        // Non-blocking enqueue prioritizes newer datagrams over stale queued data in noq 1.3.
        g.connection
            .send_datagram(encoded)
            .map_err(|_| Error::Transport)?;
        *a.activity.lock().unwrap() = Instant::now();
        g.metrics.udp_forwarded.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    pub async fn recv(&self) -> Result<Bytes> {
        let a = &self.inner.state;
        loop {
            if a.cancel.is_cancelled() {
                return Err(Error::Closed);
            }
            let notified = a.notify.notified();
            {
                let mut queue = a.inbox.lock().unwrap();
                while let Some((at, data)) = queue.pop_front() {
                    if at.elapsed() <= Duration::from_millis(40) {
                        return Ok(data);
                    }
                    self.inner
                        .guest
                        .inner
                        .metrics
                        .udp_dropped
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
            let idle = self.inner.guest.inner.limits.udp_idle_timeout;
            let remaining = idle.saturating_sub(a.activity.lock().unwrap().elapsed());
            if remaining.is_zero() {
                a.cancel.cancel();
                return Err(Error::Timeout);
            }
            tokio::select! {
                _ = a.cancel.cancelled() => return Err(Error::Closed),
                _ = self.inner.guest.inner.connection.closed() => return Err(Error::Closed),
                _ = tokio::time::sleep(remaining) => (),
                _ = notified => (),
            }
        }
    }
    pub async fn shutdown(&self) {
        self.inner.permit.lock().unwrap().take();
        if self.inner.state.cancel.is_cancelled() {
            self.inner
                .guest
                .inner
                .associations
                .lock()
                .unwrap()
                .remove(&self.inner.id);
            return;
        }
        self.inner.state.cancel.cancel();
        let result = tokio::time::timeout(
            Duration::from_millis(500),
            self.inner.guest.request(
                &self.inner.service,
                self.inner.revision,
                Protocol::Udp,
                Operation::CloseUdp,
                self.inner.id,
            ),
        )
        .await;
        if let Ok(Ok((mut send, _, _))) = result {
            let _ = send.finish();
        }
        self.inner
            .guest
            .inner
            .associations
            .lock()
            .unwrap()
            .remove(&self.inner.id);
    }
}

async fn receive(
    connection: Connection,
    associations: GuestAssociations,
    metrics: Arc<Metrics>,
    cancel: CancellationToken,
) {
    loop {
        let data = tokio::select! { _ = cancel.cancelled() => break, d = connection.read_datagram() => match d { Ok(d) => d, Err(_) => break } };
        let frame = match DatagramFrame::decode(data) {
            Ok(f) => f,
            Err(_) => {
                metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };
        let association = associations
            .lock()
            .unwrap()
            .get(&frame.association_id)
            .cloned();
        let Some(a) = association else {
            metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        if a.cancel.is_cancelled()
            || a.service_handle != frame.service_handle
            || a.generation != frame.generation
            || !a.sequence.lock().unwrap().accept(frame.sequence)
        {
            metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        if frame.payload.len() > a.max_payload {
            metrics.udp_oversize.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        let mut queue = a.inbox.lock().unwrap();
        while queue.len() >= 8
            || queue
                .front()
                .is_some_and(|(at, _)| at.elapsed() > Duration::from_millis(40))
        {
            queue.pop_front();
            metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
        }
        queue.push_back((Instant::now(), frame.payload));
        drop(queue);
        metrics.udp_returned.fetch_add(1, Ordering::Relaxed);
        a.notify.notify_one();
    }
    for a in associations.lock().unwrap().values() {
        a.cancel.cancel();
    }
}

pub struct TcpBridge {
    address: SocketAddr,
    cancel: CancellationToken,
    task: Mutex<Option<JoinHandle<()>>>,
}
pub struct UdpBridge {
    address: SocketAddr,
    cancel: CancellationToken,
    task: Mutex<Option<JoinHandle<()>>>,
    guest: Guest,
    negotiated_max: usize,
}
impl UdpBridge {
    /// The initial bidirectional ceiling, reduced immediately if our current path shrinks.
    /// A larger ceiling requires reopening the association; no implicit renegotiation.
    pub fn max_payload(&self) -> Option<usize> {
        self.guest.max_datagram_payload(self.negotiated_max)
    }
}
macro_rules! bridge_handle {
    ($kind:ty) => {
        impl $kind {
            pub fn local_addr(&self) -> SocketAddr {
                self.address
            }
            pub async fn shutdown(&self) {
                self.cancel.cancel();
                let task = self.task.lock().unwrap().take();
                if let Some(task) = task {
                    let _ = task.await;
                }
            }
        }
        impl Drop for $kind {
            fn drop(&mut self) {
                self.cancel.cancel();
            }
        }
    };
}
bridge_handle!(TcpBridge);
bridge_handle!(UdpBridge);
