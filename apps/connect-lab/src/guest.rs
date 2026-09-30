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

#[cfg(test)]
mod mtu_observation_tests {
    use super::*;
    use serde_json::{Value, json};
    use std::collections::BTreeSet;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{sleep, timeout};

    const TCP_BYTES: usize = 1024 * 1024;
    const SERVICE_PAYLOAD: usize = 1400;
    type TestResult<T> = std::result::Result<T, &'static str>;

    #[derive(Default)]
    struct OwnedLab {
        host: Option<Host>,
        guest: Option<Guest>,
        old: Option<UdpAssociation>,
        fresh: Option<UdpAssociation>,
        tcp_echo: Option<JoinHandle<TestResult<()>>>,
        udp_echo: Option<JoinHandle<TestResult<()>>>,
        stop: CancellationToken,
        tcp_addr: Option<SocketAddr>,
        udp_addrs: Vec<SocketAddr>,
    }

    impl Drop for OwnedLab {
        fn drop(&mut self) {
            self.stop.cancel();
            // Abort only these test-owned tasks if a deadline/panic interrupted cleanup.
            for task in [&self.tcp_echo, &self.udp_echo].into_iter().flatten() {
                task.abort();
            }
        }
    }

    impl OwnedLab {
        async fn cleanup(&mut self) -> TestResult<()> {
            self.stop.cancel();
            let close_old = async {
                if let Some(old) = &self.old {
                    old.shutdown().await;
                }
            };
            let close_fresh = async {
                if let Some(fresh) = &self.fresh {
                    fresh.shutdown().await;
                }
            };
            tokio::join!(close_old, close_fresh);
            tokio::join!(
                async {
                    if let Some(guest) = &self.guest {
                        guest.shutdown().await;
                    }
                },
                async {
                    if let Some(host) = &self.host {
                        host.shutdown().await;
                    }
                }
            );
            let mut failure = None;
            for task in [&mut self.tcp_echo, &mut self.udp_echo] {
                if let Some(handle) = task.as_mut() {
                    match handle.await {
                        Ok(Ok(())) => (),
                        Ok(Err(code)) => {
                            failure.get_or_insert(code);
                        }
                        Err(_) => {
                            failure.get_or_insert("echo_join_failed");
                        }
                    }
                    task.take();
                }
            }
            // Endpoint::close closes QUIC but retained handles can still own the
            // UDP socket. Release every test-owned association/endpoint handle
            // before testing that Windows can bind its address again.
            self.old.take();
            self.fresh.take();
            self.guest.take();
            self.host.take();
            // Numeric summaries never expose endpoint addresses; rebind stays local.
            if let Some(addr) = self.tcp_addr
                && TcpListener::bind(addr).await.is_err()
            {
                failure.get_or_insert("tcp_cleanup_rebind_failed");
            }
            for &addr in &self.udp_addrs {
                if UdpSocket::bind(addr).await.is_err() {
                    failure.get_or_insert("udp_cleanup_rebind_failed");
                }
            }
            failure.map_or(Ok(()), Err)
        }
    }

    fn local() -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], 0))
    }

    fn service(id: &str, protocol: Protocol, endpoint: SocketAddr) -> Service {
        Service {
            service_id: id.into(),
            protocol,
            endpoint,
            revision: 1,
            max_datagram_bytes: SERVICE_PAYLOAD,
            required_capability: match protocol {
                Protocol::Tcp => Capability::ReliableStream,
                Protocol::Udp => Capability::Datagram,
            },
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        }
    }

    fn snapshot(guest: &Guest, association: &UdpAssociation, start: Instant) -> TestResult<Value> {
        // Bracket the public effective query because PMTUD can change during the calls.
        let before = guest
            .inner
            .connection
            .max_datagram_size()
            .ok_or("datagram_capability_missing")?;
        let effective = association
            .max_payload()
            .ok_or("effective_payload_missing")?;
        let after = guest
            .inner
            .connection
            .max_datagram_size()
            .ok_or("datagram_capability_missing")?;
        let host_reply = association.inner.state.max_payload;
        let lower = before
            .min(after)
            .checked_sub(DATAGRAM_HEADER_LEN)
            .ok_or("invalid_transport_budget")?
            .min(host_reply);
        let upper = before
            .max(after)
            .checked_sub(DATAGRAM_HEADER_LEN)
            .ok_or("invalid_transport_budget")?
            .min(host_reply);
        if effective == 0 || effective < lower || effective > upper || host_reply > SERVICE_PAYLOAD
        {
            return Err("effective_payload_budget_mismatch");
        }
        let paths: Vec<_> = guest
            .inner
            .connection
            .paths()
            .iter()
            .map(|path| {
                let stats = path.stats();
                json!({
                    "currentUdpMtu": stats.current_mtu,
                    "sentPmtuProbes": stats.sent_plpmtud_probes,
                    "lostPmtuProbes": stats.lost_plpmtud_probes,
                    "direct": path.is_ip(),
                    "relay": path.is_relay(),
                    "selected": path.is_selected(),
                })
            })
            .collect();
        if paths.is_empty()
            || paths
                .iter()
                .any(|p| p["relay"] == true || p["direct"] != true)
        {
            return Err("unexpected_transport_path");
        }
        Ok(json!({
            "elapsedMs": start.elapsed().as_millis(),
            "elapsedMicros": start.elapsed().as_micros(),
            "transportDatagramBytesBefore": before,
            "transportDatagramBytesAfter": after,
            "hostReplyPayloadBytes": host_reply,
            "effectivePayloadBytes": effective,
            "paths": paths,
        }))
    }

    async fn udp_exchange(
        association: &UdpAssociation,
        bytes: usize,
        marker: u8,
    ) -> TestResult<()> {
        let payload = vec![marker; bytes];
        association
            .send(&payload)
            .await
            .map_err(|_| "udp_send_failed")?;
        let received = association.recv().await.map_err(|_| "udp_receive_failed")?;
        if received.as_ref() != payload {
            return Err("udp_payload_mismatch");
        }
        Ok(())
    }

    async fn run(lab: &mut OwnedLab) -> TestResult<Value> {
        let started = Instant::now();
        let tcp = TcpListener::bind(local())
            .await
            .map_err(|_| "tcp_echo_bind_failed")?;
        let tcp_addr = tcp.local_addr().map_err(|_| "tcp_echo_address_failed")?;
        lab.tcp_addr = Some(tcp_addr);
        let udp = UdpSocket::bind(local())
            .await
            .map_err(|_| "udp_echo_bind_failed")?;
        let udp_addr = udp.local_addr().map_err(|_| "udp_echo_address_failed")?;
        lab.udp_addrs.push(udp_addr);
        let stop = lab.stop.clone();
        lab.tcp_echo = Some(tokio::spawn(async move {
            let (socket, _) = tokio::select! {
                _ = stop.cancelled() => return Ok(()),
                accepted = tcp.accept() => accepted.map_err(|_| "tcp_echo_accept_failed")?,
            };
            let (mut read, mut write) = socket.into_split();
            let receive = async {
                let mut bytes = vec![0; TCP_BYTES];
                read.read_exact(&mut bytes)
                    .await
                    .map_err(|_| "tcp_echo_read_failed")?;
                if bytes.iter().any(|&b| b != 0x37) {
                    return Err("tcp_upstream_payload_mismatch");
                }
                let mut extra = [0];
                if read
                    .read(&mut extra)
                    .await
                    .map_err(|_| "tcp_echo_eof_failed")?
                    != 0
                {
                    return Err("tcp_upstream_extra_bytes");
                }
                Ok(())
            };
            let send = async {
                write
                    .write_all(&vec![0xa6; TCP_BYTES])
                    .await
                    .map_err(|_| "tcp_echo_write_failed")?;
                write
                    .shutdown()
                    .await
                    .map_err(|_| "tcp_echo_shutdown_failed")
            };
            tokio::select! {
                _ = stop.cancelled() => Ok(()),
                result = async { tokio::try_join!(receive, send).map(|_| ()) } => result,
            }
        }));
        let stop = lab.stop.clone();
        lab.udp_echo = Some(tokio::spawn(async move {
            let mut buffer = vec![0; 65536];
            loop {
                let (size, source) = tokio::select! {
                    _ = stop.cancelled() => return Ok(()),
                    received = udp.recv_from(&mut buffer) => received.map_err(|_| "udp_echo_read_failed")?,
                };
                tokio::select! {
                    _ = stop.cancelled() => return Ok(()),
                    sent = udp.send_to(&buffer[..size], source) => {
                        if sent.map_err(|_| "udp_echo_write_failed")? != size {
                            return Err("udp_echo_short_send");
                        }
                    }
                }
            }
        }));
        let guest_key = generate_key();
        lab.host = Some(
            Host::start(HostConfig {
                secret_key: generate_key(),
                bind_addr: local(),
                session_id: "mtu-observation".into(),
                services: vec![
                    service("game", Protocol::Tcp, tcp_addr),
                    service("aux", Protocol::Udp, udp_addr),
                ],
                leases: vec![Lease {
                    lease_id: "observer".into(),
                    peer: guest_key.public(),
                    expires_at_unix_ms: unix_millis() + 30_000,
                    services: BTreeSet::from(["game".into(), "aux".into()]),
                }],
                limits: Limits::default(),
            })
            .await
            .map_err(|_| "host_start_failed")?,
        );
        let host = lab.host.as_ref().ok_or("host_missing")?;
        lab.udp_addrs.extend(host.bound_sockets());
        lab.guest = Some(
            Guest::connect(GuestConfig {
                secret_key: guest_key,
                bind_addr: local(),
                host: host.addr(),
                session_id: "mtu-observation".into(),
                lease_id: "observer".into(),
                capabilities: vec![Capability::ReliableStream, Capability::Datagram],
                limits: Limits::default(),
            })
            .await
            .map_err(|_| "guest_connect_failed")?,
        );
        let guest = lab.guest.as_ref().ok_or("guest_missing")?;
        lab.udp_addrs.extend(guest.inner.endpoint.bound_sockets());
        lab.old = Some(
            guest
                .open_udp("aux", 1)
                .await
                .map_err(|_| "initial_association_open_failed")?,
        );
        let old = lab.old.as_ref().ok_or("initial_association_missing")?;
        let initial = snapshot(guest, old, started)?;
        udp_exchange(old, 64, 0x19).await?;
        let tcp = async {
            let (mut send, mut recv) = guest
                .open_tcp("game", 1)
                .await
                .map_err(|_| "tcp_open_failed")?;
            let upstream = async {
                send.write_all(&vec![0x37; TCP_BYTES])
                    .await
                    .map_err(|_| "tcp_upstream_write_failed")?;
                send.finish().map_err(|_| "tcp_upstream_finish_failed")
            };
            let downstream = async {
                let bytes = recv
                    .read_to_end(TCP_BYTES + 1)
                    .await
                    .map_err(|_| "tcp_downstream_read_failed")?;
                if bytes.len() != TCP_BYTES || bytes.iter().any(|&b| b != 0xa6) {
                    return Err("tcp_downstream_payload_mismatch");
                }
                Ok(())
            };
            tokio::try_join!(upstream, downstream)?;
            snapshot(guest, old, started)
        };
        let sample = async {
            let mut samples = Vec::with_capacity(20);
            for _ in 0..20 {
                samples.push(snapshot(guest, old, started)?);
                sleep(Duration::from_millis(50)).await;
            }
            Ok::<_, &'static str>(samples)
        };
        let (after_tcp, samples) = tokio::join!(tcp, sample);
        let after_tcp = after_tcp?;
        let samples = samples?;
        // Verify the host endpoint also received every upstream byte before calling it PASS.
        let echo_result = lab.tcp_echo.as_mut().ok_or("tcp_echo_missing")?.await;
        // A completed JoinHandle must never be polled again during cleanup,
        // including when its result is an error. Keep it owned while pending
        // so cancellation can still join it after signalling the stop token.
        lab.tcp_echo.take();
        echo_result.map_err(|_| "tcp_echo_join_failed")??;
        let settled = snapshot(guest, old, started)?;
        if old.inner.state.max_payload
            != initial["hostReplyPayloadBytes"]
                .as_u64()
                .ok_or("initial_summary_invalid")? as usize
        {
            return Err("initial_association_implicitly_renegotiated");
        }
        old.shutdown().await;
        if !old.is_closed() || old.send(b"closed").await != Err(Error::Closed) {
            return Err("initial_association_not_closed");
        }
        lab.fresh = Some(
            guest
                .open_udp("aux", 1)
                .await
                .map_err(|_| "fresh_association_open_failed")?,
        );
        let fresh = lab.fresh.as_ref().ok_or("fresh_association_missing")?;
        if fresh.association_id() == old.association_id() {
            return Err("association_id_reused");
        }
        let reopened = snapshot(guest, fresh, started)?;
        let payload = fresh.max_payload().ok_or("fresh_payload_missing")?;
        udp_exchange(fresh, payload, 0x93).await?;
        if fresh
            .send(&vec![0x41; fresh.inner.state.max_payload + 1])
            .await
            != Err(Error::DatagramTooLarge)
        {
            return Err("fresh_oversize_not_refused");
        }
        udp_exchange(fresh, 64, 0x51).await?;
        let mut observations: Vec<_> = std::iter::once(&initial)
            .chain(samples.iter())
            .chain([&after_tcp, &settled, &reopened])
            .collect();
        // TCP can finish before the sampler. Sort actual observation times rather
        // than treating the report's phase order as chronological evidence.
        observations.sort_by_key(|s| s["elapsedMicros"].as_u64());
        let caps: Vec<_> = observations
            .into_iter()
            .flat_map(|s| {
                [
                    s["transportDatagramBytesBefore"].as_u64(),
                    s["transportDatagramBytesAfter"].as_u64(),
                ]
                .into_iter()
                .flatten()
            })
            .collect();
        let growth = if caps.windows(2).any(|pair| pair[1] > pair[0]) {
            "growth_observed"
        } else {
            "growth_not_observed"
        };
        let host_reply_change = match fresh
            .inner
            .state
            .max_payload
            .cmp(&old.inner.state.max_payload)
        {
            std::cmp::Ordering::Less => "decreased",
            std::cmp::Ordering::Equal => "unchanged",
            std::cmp::Ordering::Greater => "increased",
        };
        Ok(json!({
            "event": "mtu_reopen_observed", "experimental": true, "scope": "loopback",
            "growth": growth, "growthMeasurement": "guest_transport_datagram_capacity",
            "hostOpeningReplyChange": host_reply_change, "hostTransportDirectlyObserved": false,
            "hostReplyIsAnOpeningSnapshot": true, "tcpBytesEachDirection": TCP_BYTES,
            "reopenedUdpExactPayloadBytes": payload, "oversizeRefused": true,
            "initial": initial, "afterTcp": after_tcp, "settledOld": settled,
            "reopened": reopened, "samples": samples,
        }))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn udp_reopening_observes_current_budget_after_real_tcp_without_assuming_growth() {
        let mut lab = OwnedLab::default();
        let result = timeout(Duration::from_secs(7), run(&mut lab)).await;
        let cleanup = timeout(Duration::from_secs(2), lab.cleanup()).await;
        match cleanup {
            Ok(Ok(())) => (),
            Ok(Err(code)) => panic!("{code}"),
            Err(_) => panic!("mtu_observation_cleanup_deadline"),
        }
        match result {
            Ok(Ok(mut observation)) => {
                observation["cleanupRebindPassed"] = json!(true);
                eprintln!("{observation}");
            }
            Ok(Err(code)) => panic!("{code}"),
            Err(_) => panic!("mtu_observation_deadline"),
        }
    }
}
