use crate::{
    model::{ActiveGuard, ActiveKind, Metrics, valid_id},
    transport::{self, Bucket, Budget},
    wire::{self, Hello, OpenRequest, Operation, Reply, SequenceWindow},
    *,
};
use iroh::{
    Endpoint, EndpointAddr, EndpointId,
    endpoint::{Connection, SendStream},
};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    net::{TcpStream, UdpSocket},
    sync::{OwnedSemaphorePermit, Semaphore},
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

struct LeaseState {
    value: Lease,
    cancel: CancellationToken,
}
struct State {
    session_id: String,
    services: HashMap<String, (u32, Service)>,
    leases: HashMap<String, LeaseState>,
    limits: Limits,
    metrics: Arc<Metrics>,
    tcp: Arc<Semaphore>,
    udp: Arc<Semaphore>,
    active_leases: Mutex<HashSet<String>>,
    budget: Budget,
    cancel: CancellationToken,
}

pub struct Host {
    endpoint: Endpoint,
    state: Arc<State>,
    task: Mutex<Option<JoinHandle<()>>>,
}
impl Host {
    pub async fn start(config: HostConfig) -> Result<Self> {
        config.limits.validate()?;
        if !valid_id(&config.session_id)
            || config.services.is_empty()
            || config.services.len() > 8
            || config.leases.is_empty()
            || config.leases.len() > config.limits.max_peers
        {
            return Err(Error::Configuration);
        }
        let mut services = HashMap::new();
        for (index, service) in config.services.into_iter().enumerate() {
            if !valid_id(&service.service_id)
                || !service.endpoint.ip().is_loopback()
                || service.endpoint.port() == 0
                || service.revision == 0
                || service.max_datagram_bytes > 65507
                || (service.protocol == Protocol::Tcp
                    && service.required_capability != Capability::ReliableStream)
                || (service.protocol == Protocol::Udp
                    && service.required_capability != Capability::Datagram)
                || services.contains_key(&service.service_id)
            {
                return Err(Error::Configuration);
            }
            services.insert(service.service_id.clone(), ((index + 1) as u32, service));
        }
        let now = unix_millis();
        let mut leases = HashMap::new();
        let mut peers = HashSet::new();
        for lease in config.leases {
            if !valid_id(&lease.lease_id)
                || lease.expires_at_unix_ms > now.saturating_add(3_600_000)
                || lease.services.is_empty()
                || lease.services.iter().any(|id| !services.contains_key(id))
                || leases.contains_key(&lease.lease_id)
                || !peers.insert(lease.peer)
            {
                return Err(Error::Configuration);
            }
            leases.insert(
                lease.lease_id.clone(),
                LeaseState {
                    value: lease,
                    cancel: CancellationToken::new(),
                },
            );
        }
        let endpoint =
            transport::endpoint(config.secret_key, config.bind_addr, &config.limits).await?;
        let state = Arc::new(State {
            session_id: config.session_id,
            services,
            leases,
            tcp: Arc::new(Semaphore::new(config.limits.max_tcp_total)),
            udp: Arc::new(Semaphore::new(config.limits.max_udp_total)),
            budget: Arc::new(Mutex::new(Bucket::new(
                config.limits.session_bytes_per_second,
                65536,
            ))),
            limits: config.limits,
            metrics: Arc::new(Metrics::default()),
            active_leases: Mutex::new(HashSet::new()),
            cancel: CancellationToken::new(),
        });
        let task = tokio::spawn(serve(endpoint.clone(), state.clone()));
        Ok(Self {
            endpoint,
            state,
            task: Mutex::new(Some(task)),
        })
    }
    pub fn addr(&self) -> EndpointAddr {
        self.endpoint.addr()
    }
    pub fn id(&self) -> EndpointId {
        self.endpoint.id()
    }
    pub fn bound_sockets(&self) -> Vec<SocketAddr> {
        self.endpoint.bound_sockets()
    }
    pub fn metrics(&self) -> MetricsSnapshot {
        self.state.metrics.snapshot()
    }
    pub fn revoke(&self, lease_id: &str) -> bool {
        if let Some(lease) = self.state.leases.get(lease_id) {
            lease.cancel.cancel();
            true
        } else {
            false
        }
    }
    pub async fn shutdown(&self) {
        self.state.cancel.cancel();
        let task = self.task.lock().unwrap().take();
        if let Some(task) = task {
            let _ = task.await;
        }
        self.endpoint.close().await;
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.state.cancel.cancel();
    }
}

async fn serve(endpoint: Endpoint, state: Arc<State>) {
    let admission = Arc::new(Semaphore::new(state.limits.max_peers + 8));
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            _ = state.cancel.cancelled() => break,
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break; };
                let Ok(permit) = admission.clone().try_acquire_owned() else { incoming.refuse(); continue; };
                let state = state.clone();
                tasks.spawn(async move {
                    let _permit = permit;
                    tokio::select! {
                        _ = state.cancel.cancelled() => (),
                        result = tokio::time::timeout(state.limits.handshake_timeout, incoming) => {
                            if let Ok(Ok(connection)) = result { serve_peer(connection, state).await; }
                        }
                    }
                });
            }
            _ = tasks.join_next(), if !tasks.is_empty() => (),
        }
    }
    while tasks.join_next().await.is_some() {}
}

struct PeerGuard {
    state: Arc<State>,
    lease_id: String,
    _active: ActiveGuard,
}
impl Drop for PeerGuard {
    fn drop(&mut self) {
        self.state
            .active_leases
            .lock()
            .unwrap()
            .remove(&self.lease_id);
    }
}

fn authorize<'a>(state: &'a State, lease_id: &str, peer: EndpointId) -> Result<&'a LeaseState> {
    let lease = state.leases.get(lease_id).ok_or(Error::Unauthorized)?;
    if lease.value.peer != peer {
        return Err(Error::Unauthorized);
    }
    if lease.cancel.is_cancelled() || unix_millis() >= lease.value.expires_at_unix_ms {
        return Err(Error::Expired);
    }
    Ok(lease)
}

async fn handshake(connection: &Connection, state: &Arc<State>) -> Result<(Hello, PeerGuard)> {
    let (mut send, mut recv) = connection.accept_bi().await.map_err(|_| Error::Transport)?;
    let hello: Hello = wire::read_message(&mut recv, state.limits.max_control_bytes).await?;
    let decision = (|| {
        if hello.schema_version != SCHEMA_VERSION
            || hello.session_id != state.session_id
            || !valid_id(&hello.lease_id)
            || hello.capabilities.len() > 2
        {
            return Err(Error::Unauthorized);
        }
        authorize(state, &hello.lease_id, connection.remote_id())?;
        let mut active = state.active_leases.lock().unwrap();
        if active.len() >= state.limits.max_peers || !active.insert(hello.lease_id.clone()) {
            return Err(Error::Limit);
        }
        Ok(PeerGuard {
            state: state.clone(),
            lease_id: hello.lease_id.clone(),
            _active: ActiveGuard::new(state.metrics.clone(), ActiveKind::Peer),
        })
    })();
    match decision {
        Ok(guard) => {
            wire::write_message(&mut send, &Reply::default(), state.limits.max_control_bytes)
                .await?;
            send.finish().map_err(|_| Error::Transport)?;
            Ok((hello, guard))
        }
        Err(error) => {
            state.metrics.denied.fetch_add(1, Ordering::Relaxed);
            let _ = wire::write_message(
                &mut send,
                &Reply::error(error),
                state.limits.max_control_bytes,
            )
            .await;
            let _ = send.finish();
            let _ = tokio::time::timeout(state.limits.handshake_timeout, send.stopped()).await;
            Err(error)
        }
    }
}

struct Association {
    socket: Arc<UdpSocket>,
    service_handle: u32,
    generation: u32,
    max_payload: usize,
    cancel: CancellationToken,
    activity: Mutex<Instant>,
    sequence: Mutex<SequenceWindow>,
    credit: AtomicU64,
    limiter: Arc<Mutex<UdpLimiter>>,
}
struct UdpLimiter {
    packets: Bucket,
    bytes: Bucket,
}
impl UdpLimiter {
    fn new(limits: &Limits) -> Self {
        Self {
            packets: Bucket::new(limits.datagrams_per_second, limits.datagram_burst),
            bytes: Bucket::new(limits.udp_bytes_per_second, 65536),
        }
    }
    fn take(&mut self, bytes: usize) -> bool {
        self.packets.take(1) && self.bytes.take(bytes)
    }
}
type Associations = Arc<Mutex<HashMap<u64, Arc<Association>>>>;

async fn serve_peer(connection: Connection, state: Arc<State>) {
    let handshake_result = tokio::select! {
        _ = state.cancel.cancelled() => Err(Error::Closed),
        result = tokio::time::timeout(state.limits.handshake_timeout, handshake(&connection, &state)) => result.unwrap_or(Err(Error::Timeout)),
    };
    let Ok((hello, _guard)) = handshake_result else {
        connection.close(1u32.into(), b"authorization failed");
        return;
    };
    let lease = &state.leases[&hello.lease_id];
    let remaining =
        Duration::from_millis(lease.value.expires_at_unix_ms.saturating_sub(unix_millis()));
    let cancel = state.cancel.child_token();
    let tcp = Arc::new(Semaphore::new(state.limits.max_tcp_per_peer));
    let associations: Associations = Arc::new(Mutex::new(HashMap::new()));
    let mut udp_limiters: HashMap<u32, Arc<Mutex<UdpLimiter>>> = HashMap::new();
    let mut tasks = JoinSet::new();
    // Revocation/expiry remains responsive even while a peer stalls a control stream.
    let lease_cancel = lease.cancel.clone();
    let watcher_cancel = cancel.clone();
    let watcher_connection = connection.clone();
    tasks.spawn(async move {
        tokio::select! {
            _ = watcher_cancel.cancelled() => (),
            _ = lease_cancel.cancelled() => (),
            _ = tokio::time::sleep(remaining) => (),
        }
        watcher_cancel.cancel();
        watcher_connection.close(0u32.into(), b"lease closed");
    });
    tasks.spawn(receive_datagrams(
        connection.clone(),
        state.clone(),
        associations.clone(),
        cancel.clone(),
    ));
    let mut last_request = 0;
    let mut control = Bucket::new(state.limits.control_per_second, state.limits.control_burst);
    let mut last_activity = Instant::now();
    let expiry = tokio::time::sleep(remaining);
    tokio::pin!(expiry);
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            _ = state.cancel.cancelled() => break,
            _ = lease.cancel.cancelled() => break,
            _ = &mut expiry => break,
            _ = connection.closed() => break,
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                if tcp.available_permits() == state.limits.max_tcp_per_peer && associations.lock().unwrap().is_empty()
                    && last_activity.elapsed() >= state.limits.idle_timeout { break; }
            }
            stream = connection.accept_bi() => {
                let Ok((mut send, mut recv)) = stream else { break; };
                last_activity = Instant::now();
                if !control.take(1) { let _ = reject(&mut send, Error::Limit, &state).await; continue; }
                let request = tokio::time::timeout(state.limits.handshake_timeout, wire::read_message::<_, OpenRequest>(&mut recv, state.limits.max_control_bytes)).await;
                let request = match request {
                    Ok(Ok(request)) => request,
                    _ => { let _ = reject(&mut send, Error::Protocol, &state).await; continue; }
                };
                let result = validate_open(&state, &hello, &connection, &request, &mut last_request);
                let (handle, service) = match result {
                    Ok(v) => v,
                    Err(error) => { let _ = reject(&mut send, error, &state).await; continue; }
                };
                if request.operation == Operation::CloseUdp {
                    let association = associations.lock().unwrap().get(&request.association_id).cloned();
                    if let Some(association) = association.filter(|a| a.service_handle == handle) {
                        association.cancel.cancel();
                        associations.lock().unwrap().remove(&request.association_id);
                        let _ = reply(&mut send, &Reply::default(), &state).await;
                        let _ = send.finish();
                    } else { let _ = reject(&mut send, Error::ServiceDenied, &state).await; }
                    continue;
                }
                match service.protocol {
                    Protocol::Tcp => {
                        let local = tcp.clone().try_acquire_owned();
                        let global = state.tcp.clone().try_acquire_owned();
                        match (local, global) {
                            (Ok(local), Ok(global)) => {
                                // Authorization and endpoint resolution completed before any destination access.
                                state.metrics.destination_tcp_connects.fetch_add(1, Ordering::Relaxed);
                                let destination = tokio::select! {
                                    _ = cancel.cancelled() => break,
                                    result = tokio::time::timeout(state.limits.handshake_timeout, TcpStream::connect(service.endpoint)) => result,
                                };
                                match destination {
                                    Ok(Ok(socket)) => {
                                        if authorize(&state, &hello.lease_id, connection.remote_id()).is_err() { let _ = reject(&mut send, Error::Expired, &state).await; continue; }
                                        if reply(&mut send, &Reply::default(), &state).await.is_err() { continue; }
                                        let state = state.clone(); let cancel = cancel.clone();
                                        tasks.spawn(async move {
                                            let (_local, _global) = (local, global);
                                            let _active = ActiveGuard::new(state.metrics.clone(), ActiveKind::Tcp);
                                            let _ = transport::bridge_tcp(socket, send, recv, state.limits.idle_timeout, cancel, state.budget.clone(), state.metrics.clone()).await;
                                        });
                                    }
                                    _ => { let _ = reject(&mut send, Error::Destination, &state).await; }
                                }
                            }
                            _ => { let _ = reject(&mut send, Error::Limit, &state).await; }
                        }
                    }
                    Protocol::Udp => {
                        if associations.lock().unwrap().len() >= state.limits.max_udp_per_peer { let _ = reject(&mut send, Error::Limit, &state).await; continue; }
                        let Ok(permit) = state.udp.clone().try_acquire_owned() else { let _ = reject(&mut send, Error::Limit, &state).await; continue; };
                        let Some(max_payload) = transport::payload_limit(&connection, service.max_datagram_bytes) else { let _ = reject(&mut send, Error::CapabilityUnavailable, &state).await; continue; };
                        let bind = if service.endpoint.is_ipv4() { "127.0.0.1:0" } else { "[::1]:0" };
                        let socket = match UdpSocket::bind(bind).await { Ok(s) => s, Err(_) => { let _ = reject(&mut send, Error::Destination, &state).await; continue; } };
                        if socket.connect(service.endpoint).await.is_err() { let _ = reject(&mut send, Error::Destination, &state).await; continue; }
                        let association = Arc::new(Association { socket: Arc::new(socket), service_handle: handle,
                            generation: service.revision, max_payload, cancel: cancel.child_token(), activity: Mutex::new(Instant::now()),
                            sequence: Mutex::new(SequenceWindow::default()), credit: AtomicU64::new(0),
                            limiter: udp_limiters.entry(handle).or_insert_with(|| Arc::new(Mutex::new(UdpLimiter::new(&state.limits)))).clone(),
                        });
                        associations.lock().unwrap().insert(request.request_id, association.clone());
                        let response = Reply { service_handle: handle, association_id: request.request_id, generation: service.revision, max_payload: Some(max_payload), ..Default::default() };
                        if reply(&mut send, &response, &state).await.is_err() { associations.lock().unwrap().remove(&request.request_id); continue; }
                        let _ = send.finish();
                        state.metrics.udp_associations_opened.fetch_add(1, Ordering::Relaxed);
                        tasks.spawn(udp_return(connection.clone(), state.clone(), associations.clone(), request.request_id, association, permit));
                    }
                }
            }
            _ = tasks.join_next(), if tasks.len() > 2 => (),
        }
    }
    cancel.cancel();
    connection.close(0u32.into(), b"session closed");
    tasks.shutdown().await;
    associations.lock().unwrap().clear();
}

fn validate_open<'a>(
    state: &'a State,
    hello: &Hello,
    connection: &Connection,
    request: &OpenRequest,
    last: &mut u64,
) -> Result<(u32, &'a Service)> {
    let lease = authorize(state, &hello.lease_id, connection.remote_id())?;
    if request.schema_version != SCHEMA_VERSION || !valid_id(&request.service_id) {
        return Err(Error::Protocol);
    }
    if request.request_id <= *last || request.request_id > state.limits.max_requests_per_connection
    {
        return Err(Error::Replay);
    }
    *last = request.request_id;
    let (handle, service) = state
        .services
        .get(&request.service_id)
        .ok_or(Error::ServiceDenied)?;
    if !lease.value.services.contains(&request.service_id) {
        return Err(Error::ServiceDenied);
    }
    if request.service_revision != service.revision {
        return Err(Error::StaleRevision);
    }
    if request.protocol != service.protocol
        || (request.operation == Operation::CloseUdp && request.protocol != Protocol::Udp)
    {
        return Err(Error::Protocol);
    }
    if !hello.capabilities.contains(&service.required_capability) {
        return Err(Error::CapabilityUnavailable);
    }
    Ok((*handle, service))
}

async fn reply(send: &mut SendStream, response: &Reply, state: &State) -> Result<()> {
    tokio::time::timeout(
        state.limits.handshake_timeout,
        wire::write_message(send, response, state.limits.max_control_bytes),
    )
    .await
    .map_err(|_| Error::Timeout)?
}
async fn reject(send: &mut SendStream, error: Error, state: &State) -> Result<()> {
    state.metrics.denied.fetch_add(1, Ordering::Relaxed);
    reply(send, &Reply::error(error), state).await?;
    send.finish().map_err(|_| Error::Transport)
}

async fn receive_datagrams(
    connection: Connection,
    state: Arc<State>,
    associations: Associations,
    cancel: CancellationToken,
) {
    loop {
        let data = tokio::select! { _ = cancel.cancelled() => break, data = connection.read_datagram() => match data { Ok(d) => d, Err(_) => break } };
        let frame = match DatagramFrame::decode(data) {
            Ok(f) => f,
            Err(_) => {
                state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };
        let association = associations
            .lock()
            .unwrap()
            .get(&frame.association_id)
            .cloned();
        let Some(a) = association else {
            state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        if cancel.is_cancelled()
            || a.cancel.is_cancelled()
            || a.activity.lock().unwrap().elapsed() >= state.limits.udp_idle_timeout
            || a.service_handle != frame.service_handle
            || a.generation != frame.generation
            || !a.sequence.lock().unwrap().accept(frame.sequence)
        {
            state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        let max = transport::payload_limit(&connection, a.max_payload).unwrap_or(0);
        if frame.payload.len() > max {
            state.metrics.udp_oversize.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        if !a
            .limiter
            .lock()
            .unwrap()
            .take(frame.payload.len() + DATAGRAM_HEADER_LEN)
            || !state
                .budget
                .lock()
                .unwrap()
                .take(frame.payload.len() + DATAGRAM_HEADER_LEN)
        {
            state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        *a.activity.lock().unwrap() = Instant::now();
        let credit = (frame.payload.len() + DATAGRAM_HEADER_LEN) as u64;
        let _ = a
            .credit
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(credit).min(65536))
            });
        let delivered = tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            _ = a.cancel.cancelled() => continue,
            result = tokio::time::timeout(Duration::from_millis(40), a.socket.send(&frame.payload)) => result,
        };
        if matches!(delivered, Ok(Ok(n)) if n == frame.payload.len()) {
            state.metrics.udp_forwarded.fetch_add(1, Ordering::Relaxed);
        } else {
            state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

async fn udp_return(
    connection: Connection,
    state: Arc<State>,
    associations: Associations,
    id: u64,
    a: Arc<Association>,
    _permit: OwnedSemaphorePermit,
) {
    let _active = ActiveGuard::new(state.metrics.clone(), ActiveKind::Udp);
    // A full UDP-family buffer detects oversized payloads without forwarding a truncated prefix.
    let mut buffer = vec![0u8; 65536];
    let mut sequence = 0u32;
    loop {
        let idle_deadline = tokio::time::Instant::from_std(*a.activity.lock().unwrap())
            + state.limits.udp_idle_timeout;
        tokio::select! {
            biased;
            _ = a.cancel.cancelled() => break,
            _ = tokio::time::sleep_until(idle_deadline) => {
                if a.activity.lock().unwrap().elapsed() >= state.limits.udp_idle_timeout { break; }
            }
            received = a.socket.recv(&mut buffer) => {
                let size = match received { Ok(n) => n, Err(_) => break };
                let Some(max) = transport::payload_limit(&connection, a.max_payload) else { break; };
                if size > max { state.metrics.udp_oversize.fetch_add(1, Ordering::Relaxed); continue; }
                let cost = (size + DATAGRAM_HEADER_LEN) as u64;
                if a.credit.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(cost)).is_err()
                    || !a.limiter.lock().unwrap().take(size + DATAGRAM_HEADER_LEN)
                    || !state.budget.lock().unwrap().take(size + DATAGRAM_HEADER_LEN) {
                    state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed); continue;
                }
                let frame = DatagramFrame { service_handle: a.service_handle, association_id: id, generation: a.generation,
                    sequence, payload: bytes::Bytes::copy_from_slice(&buffer[..size]) };
                let Some(next) = sequence.checked_add(1) else { break; }; sequence = next;
                if frame.encode().and_then(|bytes| connection.send_datagram(bytes).map_err(|_| Error::Transport)).is_ok() {
                    state.metrics.udp_returned.fetch_add(1, Ordering::Relaxed);
                } else { state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed); }
            }
        }
    }
    a.cancel.cancel();
    associations.lock().unwrap().remove(&id);
}
