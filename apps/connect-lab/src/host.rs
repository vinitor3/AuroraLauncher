use crate::{
    bridge, fragment,
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
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    net::{TcpStream, UdpSocket},
    sync::{Notify, OwnedSemaphorePermit, Semaphore},
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
enum InvitationAuthority {
    Legacy(Invitation),
    Session(SessionInvitation),
}
impl InvitationAuthority {
    fn online_clock(&self) -> Option<&OnlineClock> {
        match self {
            Self::Legacy(_) => None,
            Self::Session(value) => value.online_clock(),
        }
    }
    fn lease_id(&self) -> String {
        match self {
            Self::Legacy(value) => value.lease().lease_id,
            Self::Session(value) => value.invitation_id().to_owned(),
        }
    }
    fn check_admission(&self) -> Result<()> {
        match self {
            Self::Legacy(value) => value.check_current(),
            Self::Session(value) => value.check_admission_current(),
        }
    }
    fn check_session(&self) -> Result<()> {
        match self {
            Self::Legacy(value) => value.check_current(),
            Self::Session(value) => value.check_session_current(),
        }
    }
    fn session_remaining(&self) -> Duration {
        match self {
            Self::Legacy(value) => value.remaining(),
            Self::Session(value) => value.session_remaining().unwrap_or_default(),
        }
    }
    fn proof(&self) -> wire::InvitationProof {
        match self {
            Self::Legacy(value) => value.proof(),
            Self::Session(value) => value.proof(),
        }
    }
    fn capabilities(&self) -> Vec<Capability> {
        match self {
            Self::Legacy(value) => value.capabilities(),
            Self::Session(value) => value.capabilities(),
        }
    }
}

// Manual authorization is an explicit mode. An invited group must assign one
// retained original authority to every lease; absence never becomes manual.
enum HostAuthorities {
    Manual,
    Invited(HashMap<String, InvitationAuthority>),
    Online(Box<online::OnlineAuthority>),
}

#[cfg(test)]
#[path = "host_active_observation_tests.rs"]
mod active_observation_tests;
#[cfg(all(windows, debug_assertions, feature = "native-voice-test-support"))]
#[path = "host_native_voice_test_support.rs"]
mod native_voice_test_support;
#[path = "host_online.rs"]
mod online;
pub use online::HostLocalPlayerTicket;
#[path = "host_session_group.rs"]
mod session_group;
#[path = "host_udp_service.rs"]
mod udp_service;
#[cfg(all(windows, debug_assertions, feature = "native-voice-test-support"))]
pub use native_voice_test_support::{NativeVoiceRetirementGate, NativeVoiceRetirementStage};
#[cfg(test)]
#[path = "host_udp_service_tests.rs"]
mod udp_service_tests;

#[cfg(test)]
mod session_clock_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn civil_jump_closes_active_tcp(host_clock: bool) {
        let local: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = tokio::net::TcpListener::bind(local).await.unwrap();
        let destination_address = listener.local_addr().unwrap();
        let reservation = std::net::UdpSocket::bind(local).unwrap();
        let bind = reservation.local_addr().unwrap();
        let host_key = generate_key();
        let guest_key = generate_key();
        let now = unix_millis();
        let wall = Arc::new(AtomicU64::new(now));
        let service = Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: destination_address,
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        };
        let original = SessionInvitation::issue_loopback(
            &host_key,
            guest_key.public(),
            "clock-session",
            bind,
            std::slice::from_ref(&service),
            SessionDeadlines {
                admission_expires_at_unix_ms: now + 20_000,
                session_expires_at_unix_ms: now + 60_000,
            },
        )
        .unwrap();
        let changed = original.clone().with_test_wall_clock(wall.clone());
        let host_invitation = if host_clock { &changed } else { &original };
        let guest_invitation = if host_clock { &original } else { &changed };
        drop(reservation);
        let host = Host::start_invited_session(
            HostConfig {
                secret_key: host_key,
                bind_addr: bind,
                session_id: original.session_id().into(),
                services: vec![service],
                leases: vec![original.lease()],
                limits: Limits::default(),
            },
            host_invitation,
        )
        .await
        .unwrap();
        let guest = Guest::connect_invited_session(guest_key, guest_invitation, Limits::default())
            .await
            .unwrap();
        let bridge = guest.tcp_bridge("game", 1, local).await.unwrap();
        let bridge_address = bridge.local_addr();
        let mut app = TcpStream::connect(bridge_address).await.unwrap();
        let (mut destination, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
            .await
            .unwrap()
            .unwrap();
        app.write_all(b"x").await.unwrap();
        let mut data = [0; 1];
        tokio::time::timeout(Duration::from_secs(3), destination.read_exact(&mut data))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(data, *b"x");
        // Stall Host's control stream read. A check only in its outer loop
        // cannot observe the civil jump before the 10-second read timeout.
        let control_reads = host.state.control_reads_started.load(Ordering::SeqCst);
        let (mut stalled, receiver) = guest.connection().open_bi().await.unwrap();
        stalled.write_all(&[0, 0]).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while host.state.control_reads_started.load(Ordering::SeqCst) == control_reads {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        let before = Instant::now();
        wall.store(original.session_expires_at_unix_ms(), Ordering::SeqCst);
        let observed = tokio::time::timeout(Duration::from_secs(2), app.read(&mut data))
            .await
            .unwrap();
        assert!(matches!(observed, Ok(0) | Err(_)));
        let destination_closed =
            tokio::time::timeout(Duration::from_secs(2), destination.read(&mut data))
                .await
                .unwrap();
        assert!(matches!(destination_closed, Ok(0) | Err(_)));
        assert!(before.elapsed() < Duration::from_secs(2));
        assert!(
            original.check_session_current().is_ok(),
            "real_session_deadline_must_not_have_elapsed"
        );
        assert_eq!(host.metrics().destination_tcp_connects, 1);
        drop((app, destination, stalled, receiver));
        bridge.shutdown().await;
        guest.shutdown().await;
        host.shutdown().await;
        drop((bridge, guest, host, listener));
        drop(std::net::TcpListener::bind(bridge_address).unwrap());
        drop(std::net::TcpListener::bind(destination_address).unwrap());
        drop(std::net::UdpSocket::bind(bind).unwrap());
    }

    #[tokio::test]
    async fn host_civil_advance_closes_active_tcp_despite_stalled_control() {
        civil_jump_closes_active_tcp(true).await;
    }
    #[tokio::test]
    async fn guest_civil_advance_closes_active_tcp_despite_stalled_host_control() {
        civil_jump_closes_active_tcp(false).await;
    }
}

struct LeaseState {
    value: Lease,
    invitation: Option<InvitationAuthority>,
    consumed: AtomicBool,
    cancel: CancellationToken,
    udp_limiters: Mutex<HashMap<u32, Arc<Mutex<UdpLimiter>>>>,
    downstream_limiters: Mutex<HashMap<u32, Arc<Mutex<UdpLimiter>>>>,
    fragment_ingress: Arc<Mutex<UdpLimiter>>,
    fragment_memory: fragment::PeerBudget,
    udp_services: HashMap<u32, Arc<udp_service::Control>>,
}

impl LeaseState {
    fn new(
        lease: Lease,
        invitation: Option<InvitationAuthority>,
        services: &HashMap<String, (u32, Service)>,
        limits: &Limits,
    ) -> Self {
        Self {
            udp_services: lease
                .services
                .iter()
                .filter_map(|id| {
                    let (handle, service) = &services[id];
                    (service.protocol == Protocol::Udp)
                        .then(|| (*handle, udp_service::Control::new()))
                })
                .collect(),
            fragment_memory: bridge::peer_budget(lease.peer),
            fragment_ingress: Arc::new(Mutex::new(UdpLimiter::new(limits))),
            value: lease,
            invitation,
            consumed: AtomicBool::new(false),
            cancel: CancellationToken::new(),
            udp_limiters: Mutex::new(HashMap::new()),
            downstream_limiters: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
#[path = "host_session_udp_tests.rs"]
mod session_udp_tests;
struct State {
    session_id: String,
    services: HashMap<String, (u32, Service)>,
    leases: Mutex<HashMap<String, Arc<LeaseState>>>,
    online: Option<Box<online::OnlineAuthority>>,
    limits: Limits,
    metrics: Arc<Metrics>,
    tcp: Arc<Semaphore>,
    udp: Arc<Semaphore>,
    active_leases: Mutex<HashMap<String, Connection>>,
    budget: Budget,
    cancel: CancellationToken,
    #[cfg(test)]
    control_reads_started: AtomicU64,
}

impl State {
    fn lease(&self, id: &str) -> Option<Arc<LeaseState>> {
        self.leases.lock().ok()?.get(id).cloned()
    }
}

pub struct Host {
    endpoint: Endpoint,
    state: Arc<State>,
    task: Mutex<Option<JoinHandle<()>>>,
}
impl Host {
    pub async fn start(config: HostConfig) -> Result<Self> {
        Self::start_inner(config, None, transport::EndpointMode::Direct).await
    }
    /// Explicit owned loopback relay fixture. No address lookup, IP transport,
    /// provider account or change to peer/session/service authorization.
    #[cfg(feature = "local-relay-lab")]
    pub async fn start_local_relay(config: HostConfig, ticket: &LocalRelayTicket) -> Result<Self> {
        ticket.check_host(&config)?;
        Self::start_inner(
            config,
            None,
            transport::EndpointMode::LocalRelay(ticket.clone()),
        )
        .await
    }
    /// One explicit invited lease, separate from the reconnectable manual mode.
    /// The caller must create fresh host/session keys each run: replay state is
    /// local to this Host and is neither persisted nor reconstructed from tokens.
    pub async fn start_invited(config: HostConfig, invitation: &Invitation) -> Result<Self> {
        invitation.check_host(&config)?;
        Self::start_inner(
            config,
            Some(InvitationAuthority::Legacy(invitation.clone())),
            transport::EndpointMode::Direct,
        )
        .await
    }
    /// Explicit external IPv6 initial bind only; grants and one-use admission
    /// are identical to the loopback mode. No discovery, relay or renewal.
    pub async fn start_invited_external_ipv6(
        config: HostConfig,
        invitation: &Invitation,
    ) -> Result<Self> {
        invitation.check_host_external_ipv6(&config)?;
        Self::start_inner(
            config,
            Some(InvitationAuthority::Legacy(invitation.clone())),
            transport::EndpointMode::Direct,
        )
        .await
    }

    /// Explicit direct IPv4 opt-in; preserves this profile's grants and original
    /// deadlines. No relay, address discovery, port mapping or route guarantee.
    pub async fn start_invited_direct_ipv4(
        config: HostConfig,
        invitation: &Invitation,
    ) -> Result<Self> {
        invitation.check_host_direct_ipv4(&config)?;
        Self::start_inner(
            config,
            Some(InvitationAuthority::Legacy(invitation.clone())),
            transport::EndpointMode::Direct,
        )
        .await
    }
    /// One-use V2 invitation, requiring the exact signed game/TCP and
    /// aux/fragmented-UDP registry before opening a loopback endpoint.
    pub async fn start_invited_fragmented(
        config: HostConfig,
        invitation: &Invitation,
    ) -> Result<Self> {
        invitation.check_host_fragmented(&config)?;
        Self::start_inner(
            config,
            Some(InvitationAuthority::Legacy(invitation.clone())),
            transport::EndpointMode::Direct,
        )
        .await
    }
    /// Both V2 grants and an external IPv6 initial bind require this explicit
    /// entry point. Authentication, one-use admission and expiry are unchanged.
    pub async fn start_invited_fragmented_external_ipv6(
        config: HostConfig,
        invitation: &Invitation,
    ) -> Result<Self> {
        invitation.check_host_fragmented_external_ipv6(&config)?;
        Self::start_inner(
            config,
            Some(InvitationAuthority::Legacy(invitation.clone())),
            transport::EndpointMode::Direct,
        )
        .await
    }

    /// Explicit direct IPv4 opt-in; preserves this profile's grants and original
    /// deadlines. No relay, address discovery, port mapping or route guarantee.
    pub async fn start_invited_fragmented_direct_ipv4(
        config: HostConfig,
        invitation: &Invitation,
    ) -> Result<Self> {
        invitation.check_host_fragmented_direct_ipv4(&config)?;
        Self::start_inner(
            config,
            Some(InvitationAuthority::Legacy(invitation.clone())),
            transport::EndpointMode::Direct,
        )
        .await
    }
    /// Explicit schema-2 game/TCP session. Closing the admission window does
    /// not close a peer already admitted; the signed session deadline does.
    pub async fn start_invited_session(
        config: HostConfig,
        invitation: &SessionInvitation,
    ) -> Result<Self> {
        invitation.check_host(&config)?;
        Self::start_inner(
            config,
            Some(InvitationAuthority::Session(invitation.clone())),
            transport::EndpointMode::Direct,
        )
        .await
    }
    /// Explicit schema-2 session with an independently approved IPv6 address.
    pub async fn start_invited_session_external_ipv6(
        config: HostConfig,
        invitation: &SessionInvitation,
    ) -> Result<Self> {
        invitation.check_host_external_ipv6(&config)?;
        Self::start_inner(
            config,
            Some(InvitationAuthority::Session(invitation.clone())),
            transport::EndpointMode::Direct,
        )
        .await
    }

    /// Explicit direct IPv4 opt-in; preserves this profile's grants and original
    /// deadlines. No relay, address discovery, port mapping or route guarantee.
    pub async fn start_invited_session_direct_ipv4(
        config: HostConfig,
        invitation: &SessionInvitation,
    ) -> Result<Self> {
        invitation.check_host_direct_ipv4(&config)?;
        Self::start_inner(
            config,
            Some(InvitationAuthority::Session(invitation.clone())),
            transport::EndpointMode::Direct,
        )
        .await
    }
    /// Explicit schema-3 session: signed game/TCP and fragmented aux/UDP,
    /// with independent, immutable admission and session deadlines.
    pub async fn start_invited_fragmented_session(
        config: HostConfig,
        invitation: &SessionInvitation,
    ) -> Result<Self> {
        invitation.check_host_fragmented(&config)?;
        Self::start_inner(
            config,
            Some(InvitationAuthority::Session(invitation.clone())),
            transport::EndpointMode::Direct,
        )
        .await
    }
    /// The external IPv6 initial address remains an explicit caller choice.
    pub async fn start_invited_fragmented_session_external_ipv6(
        config: HostConfig,
        invitation: &SessionInvitation,
    ) -> Result<Self> {
        invitation.check_host_fragmented_external_ipv6(&config)?;
        Self::start_inner(
            config,
            Some(InvitationAuthority::Session(invitation.clone())),
            transport::EndpointMode::Direct,
        )
        .await
    }

    /// Explicit direct IPv4 opt-in; preserves this profile's grants and original
    /// deadlines. No relay, address discovery, port mapping or route guarantee.
    pub async fn start_invited_fragmented_session_direct_ipv4(
        config: HostConfig,
        invitation: &SessionInvitation,
    ) -> Result<Self> {
        invitation.check_host_fragmented_direct_ipv4(&config)?;
        Self::start_inner(
            config,
            Some(InvitationAuthority::Session(invitation.clone())),
            transport::EndpointMode::Direct,
        )
        .await
    }
    async fn start_inner(
        config: HostConfig,
        invitation: Option<InvitationAuthority>,
        mode: transport::EndpointMode,
    ) -> Result<Self> {
        let authorities = match invitation {
            Some(invitation) => {
                HostAuthorities::Invited(HashMap::from([(invitation.lease_id(), invitation)]))
            }
            None => HostAuthorities::Manual,
        };
        Self::start_with_authorities(config, authorities, mode).await
    }
    async fn start_with_authorities(
        config: HostConfig,
        mut authorities: HostAuthorities,
        mode: transport::EndpointMode,
    ) -> Result<Self> {
        config.limits.validate()?;
        let online = matches!(&authorities, HostAuthorities::Online(_));
        let native_voice = matches!(&authorities, HostAuthorities::Online(authority) if authority.first.is_online_native_voice());
        if !valid_id(&config.session_id)
            || config.services.is_empty()
            || config.services.len() > 8
            || config.leases.is_empty()
            || config.leases.len() > config.limits.max_peers
            || (online && config.leases.len() != 1)
        {
            return Err(Error::Configuration);
        }
        let mut services = HashMap::new();
        for (index, service) in config.services.into_iter().enumerate() {
            if let UdpReturnPolicy::AuthorizedDownstream {
                bytes_per_second,
                datagrams_per_second,
            } = service.udp_return_policy
                && (service.protocol != Protocol::Udp
                    || bytes_per_second
                        < (if service.required_capability == Capability::FragmentedDatagram {
                            fragment::HEADER_LEN
                        } else {
                            DATAGRAM_HEADER_LEN
                        }) as u32
                    || bytes_per_second > config.limits.udp_bytes_per_second
                    || datagrams_per_second == 0
                    || datagrams_per_second > config.limits.datagrams_per_second)
            {
                return Err(Error::Configuration);
            }
            if !valid_id(&service.service_id)
                || !service.endpoint.ip().is_loopback()
                || service.endpoint.port() == 0
                || service.revision == 0
                || service.max_datagram_bytes > 65507
                || (service.protocol == Protocol::Tcp
                    && service.required_capability != Capability::ReliableStream)
                || (service.protocol == Protocol::Udp
                    && !matches!(
                        service.required_capability,
                        Capability::Datagram | Capability::FragmentedDatagram
                    ))
                || (service.required_capability == Capability::FragmentedDatagram
                    && service.max_datagram_bytes > fragment::MAX_DATAGRAM)
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
                || (!online && lease.expires_at_unix_ms > now.saturating_add(3_600_000))
                || lease.services.is_empty()
                || lease.services.iter().any(|id| !services.contains_key(id))
                || leases.contains_key(&lease.lease_id)
                || !peers.insert(lease.peer)
            {
                return Err(Error::Configuration);
            }
            let invitation = match &mut authorities {
                HostAuthorities::Manual => None,
                HostAuthorities::Invited(values) => {
                    Some(values.remove(&lease.lease_id).ok_or(Error::Configuration)?)
                }
                HostAuthorities::Online(authority) => {
                    Some(InvitationAuthority::Session(authority.first.clone()))
                }
            };
            leases.insert(
                lease.lease_id.clone(),
                Arc::new(LeaseState::new(
                    lease,
                    invitation,
                    &services,
                    &config.limits,
                )),
            );
        }
        let online = match authorities {
            HostAuthorities::Invited(values) if !values.is_empty() => {
                return Err(Error::Configuration);
            }
            HostAuthorities::Online(authority) => Some(authority),
            _ => None,
        };
        let endpoint =
            transport::endpoint_in_mode(config.secret_key, config.bind_addr, &config.limits, mode)
                .await?;
        for lease in leases.values() {
            if let Some(InvitationAuthority::Session(authority)) = &lease.invitation
                && let Err(error) = authority.check_admission_current()
            {
                endpoint.close().await;
                return Err(error);
            }
        }
        if let Some(online) = &online
            && let Err(error) = online.check_current()
        {
            endpoint.close().await;
            return Err(error);
        }
        let state = Arc::new(State {
            session_id: config.session_id,
            services,
            leases: Mutex::new(leases),
            online,
            tcp: Arc::new(Semaphore::new(config.limits.max_tcp_total)),
            udp: Arc::new(Semaphore::new(config.limits.max_udp_total)),
            budget: Bucket::session(&config.limits, native_voice),
            limits: config.limits,
            metrics: Arc::new(Metrics::default()),
            active_leases: Mutex::new(HashMap::new()),
            cancel: CancellationToken::new(),
            #[cfg(test)]
            control_reads_started: AtomicU64::new(0),
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
    /// Cumulative one-use decision, including an admitted peer that disconnected
    /// before a caller could sample active_peers. Contains no peer/key material.
    pub fn invitation_consumed(&self, lease_id: &str) -> bool {
        let _admission = self.state.active_leases.lock().unwrap();
        self.state.lease(lease_id).is_some_and(|lease| {
            lease.invitation.is_some() && lease.consumed.load(Ordering::Acquire)
        })
    }
    /// Instantaneous authenticated transport observation, not player presence.
    /// Unknown remote failures can become visible after this method returns.
    pub fn invitation_active(&self, lease_id: &str) -> bool {
        let active = self.state.active_leases.lock().unwrap();
        active_invitation(&self.state, &active, lease_id)
    }
    /// Redacted instantaneous path for one already-consumed, current invitation.
    /// Manual, unknown, unconsumed, expired and revoked leases all return None.
    /// The existing admission lock linearizes this read with revoke/admission.
    pub fn invitation_path_observation(&self, lease_id: &str) -> Option<PathObservation> {
        let active = self.state.active_leases.lock().ok()?;
        let lease = self.state.lease(lease_id)?;
        if self.state.cancel.is_cancelled() || lease.invitation.is_none() {
            return None;
        }
        // Check the original clocks before cumulative consumption; expired
        // ingress alone does not invalidate an already-admitted session.
        authorize_session(&self.state, lease_id, lease.value.peer).ok()?;
        if !lease.consumed.load(Ordering::Acquire) {
            return None;
        }
        let connection = active.get(lease_id);
        let mut observation = match connection {
            Some(connection) => {
                if connection.remote_id() != lease.value.peer {
                    return None;
                }
                if connection.close_reason().is_some() {
                    PathObservation::new(PathState::Closed)
                } else {
                    PathObservation::sample(connection)
                }
            }
            None => PathObservation::new(PathState::Closed),
        };
        if self.state.cancel.is_cancelled()
            || authorize_session(&self.state, lease_id, lease.value.peer).is_err()
        {
            return None;
        }
        if connection.is_some_and(|connection| connection.close_reason().is_some()) {
            observation = PathObservation::new(PathState::Closed);
        }
        Some(observation)
    }
    pub fn revoke(&self, lease_id: &str) -> bool {
        // Linearize revocation with admission and its single-use decision.
        let _admission = self.state.active_leases.lock().unwrap();
        if let Some(lease) = self.state.lease(lease_id) {
            lease.cancel.cancel();
            true
        } else {
            false
        }
    }
    /// Permanently retire one original UDP grant, then wait until its Host
    /// setup, sockets, deliveries and return workers have actually released.
    /// TCP and the peer's other services keep their original authorities.
    pub async fn disable_udp_service(
        &self,
        lease_id: &str,
        service_id: &str,
        service_revision: u32,
    ) -> Result<()> {
        if !valid_id(lease_id) || !valid_id(service_id) || service_revision == 0 {
            return Err(Error::Configuration);
        }
        let lease = self.state.lease(lease_id).ok_or(Error::ServiceDenied)?;
        let (handle, service) = self
            .state
            .services
            .get(service_id)
            .ok_or(Error::ServiceDenied)?;
        if service.protocol != Protocol::Udp
            || !lease.value.services.iter().any(|id| id == service_id)
        {
            return Err(Error::ServiceDenied);
        }
        if service_revision != service.revision {
            return Err(Error::StaleRevision);
        }
        let control = lease.udp_services.get(handle).ok_or(Error::ServiceDenied)?;
        // Retirement deliberately does not require current admission/session.
        control.disable().await;
        Ok(())
    }
    /// Wait for owned peer work and close the transport. Iroh's UDP sockets
    /// are released after the last endpoint owner drops.
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
    let online_expired = async {
        match &state.online {
            Some(authority) => authority.wait_expired().await,
            None => std::future::pending::<()>().await,
        }
    };
    tokio::pin!(online_expired);
    let expired = loop {
        tokio::select! {
            biased;
            _ = state.cancel.cancelled() => break false,
            _ = &mut online_expired => {
                state.cancel.cancel();
                break true;
            }
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break false; };
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
    };
    while tasks.join_next().await.is_some() {}
    if expired {
        endpoint.close().await;
    }
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

fn authorize_session(state: &State, lease_id: &str, peer: EndpointId) -> Result<Arc<LeaseState>> {
    let lease = state.lease(lease_id).ok_or(Error::Unauthorized)?;
    if lease.value.peer != peer {
        return Err(Error::Unauthorized);
    }
    let (_, wall) = OnlineClock::observe(
        lease
            .invitation
            .as_ref()
            .and_then(InvitationAuthority::online_clock),
    );
    if lease.cancel.is_cancelled() || wall >= lease.value.expires_at_unix_ms {
        return Err(Error::Expired);
    }
    if let Some(invitation) = &lease.invitation {
        invitation.check_session()?;
    }
    if let Some(online) = &state.online {
        if state.cancel.is_cancelled() {
            return Err(Error::Closed);
        }
        online.check_current()?;
    }
    Ok(lease)
}

fn active_invitation(state: &State, active: &HashMap<String, Connection>, lease_id: &str) -> bool {
    if state.cancel.is_cancelled() {
        return false;
    }
    let Some(connection) = active.get(lease_id) else {
        return false;
    };
    authorize_session(state, lease_id, connection.remote_id()).is_ok_and(|lease| {
        lease.invitation.is_some()
            && lease.consumed.load(Ordering::Acquire)
            && connection.close_reason().is_none()
    })
}

fn authorize_admission(state: &State, lease_id: &str, peer: EndpointId) -> Result<Arc<LeaseState>> {
    let lease = authorize_session(state, lease_id, peer)?;
    if let Some(invitation) = &lease.invitation {
        invitation.check_admission()?;
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
            || hello.capabilities.len() > 3
        {
            return Err(Error::Unauthorized);
        }
        let lease = authorize_admission(state, &hello.lease_id, connection.remote_id())?;
        match (&lease.invitation, &hello.invitation) {
            (None, None) => (),
            (Some(invitation), Some(proof)) if *proof == invitation.proof() => {
                if hello.capabilities != invitation.capabilities() {
                    return Err(Error::CapabilityUnavailable);
                }
            }
            // Missing proof cannot downgrade an invited lease to manual;
            // a manual lease never interprets an unsolicited invitation proof.
            _ => return Err(Error::Unauthorized),
        }
        let mut active = state.active_leases.lock().unwrap();
        authorize_admission(state, &hello.lease_id, connection.remote_id())?;
        if lease.invitation.is_some() && lease.consumed.load(Ordering::Acquire) {
            return Err(Error::Replay);
        }
        if active.len() >= state.limits.max_peers || active.contains_key(&hello.lease_id) {
            return Err(Error::Limit);
        }
        // TLS is fully authenticated before this handler is entered. Consume
        // only after every binding/capability/revocation check, before the ACK.
        // Never undo consumption if sending the ACK fails or the peer drops.
        if lease.invitation.is_some() {
            lease.consumed.store(true, Ordering::Release);
        }
        active.insert(hello.lease_id.clone(), connection.clone());
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
    service_handle: u32,
    generation: u32,
    max_payload: usize,
    cancel: CancellationToken,
    activity: Mutex<Instant>,
    sequence: Mutex<SequenceWindow>,
    credit: AtomicU64,
    limiter: Arc<Mutex<UdpLimiter>>,
    downstream_limiter: Arc<Mutex<UdpLimiter>>,
    return_policy: UdpReturnPolicy,
    first_forwarded: AtomicBool,
    first_forwarded_notify: Notify,
    fragmented: Option<Mutex<fragment::Reassembler>>,
    lease_cancel: CancellationToken,
    lease_unix_deadline: u64,
    lease_deadline: Instant,
    session_authority: Option<SessionInvitation>,
    // Last field: pending fragment reservations drop before socket/work.
    resource: udp_service::Resource,
}
impl Association {
    fn socket(&self) -> &UdpSocket {
        &self.resource.socket
    }
    fn fragment_authorized(&self) -> bool {
        self.resource.work.control().enabled()
            && fragment_authority_valid(
                &self.cancel,
                &self.lease_cancel,
                self.lease_unix_deadline,
                self.lease_deadline,
                self.session_authority
                    .as_ref()
                    .and_then(SessionInvitation::online_clock),
            )
            && self
                .session_authority
                .as_ref()
                .is_none_or(|authority| authority.check_session_current().is_ok())
    }
}
fn fragment_authority_valid(
    cancel: &CancellationToken,
    lease: &CancellationToken,
    unix_deadline: u64,
    monotonic_deadline: Instant,
    clock: Option<&OnlineClock>,
) -> bool {
    let (at, wall) = OnlineClock::observe(clock);
    !cancel.is_cancelled()
        && !lease.is_cancelled()
        && wall < unix_deadline
        && at < monotonic_deadline
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
    fn downstream(policy: UdpReturnPolicy, limits: &Limits) -> Self {
        match policy {
            UdpReturnPolicy::ReplyBudget => Self::new(limits),
            UdpReturnPolicy::AuthorizedDownstream {
                bytes_per_second,
                datagrams_per_second,
            } => Self {
                packets: Bucket::new(
                    datagrams_per_second,
                    datagrams_per_second.min(limits.datagram_burst),
                ),
                bytes: Bucket::new(bytes_per_second, bytes_per_second.min(65536)),
            },
        }
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
    let Some(lease) = state.lease(&hello.lease_id) else {
        connection.close(1u32.into(), b"authorization failed");
        return;
    };
    let (at, wall) = OnlineClock::observe(
        lease
            .invitation
            .as_ref()
            .and_then(InvitationAuthority::online_clock),
    );
    let mut remaining = Duration::from_millis(lease.value.expires_at_unix_ms.saturating_sub(wall));
    if let Some(invitation) = &lease.invitation {
        remaining = remaining.min(invitation.session_remaining());
    }
    if let Some(online) = &state.online {
        remaining = remaining.min(online.remaining());
    }
    let cancel = state.cancel.child_token();
    let lease_deadline = at + remaining;
    let tcp = Arc::new(Semaphore::new(state.limits.max_tcp_per_peer));
    let associations: Associations = Arc::new(Mutex::new(HashMap::new()));
    let mut tasks = JoinSet::new();
    // Revocation/expiry remains responsive even while a peer stalls a control stream.
    let lease_cancel = lease.cancel.clone();
    let watcher_cancel = cancel.clone();
    let watcher_connection = connection.clone();
    let session_authority = lease.invitation.clone();
    tasks.spawn(async move {
        // A separate future rechecks civil time even when a control read is
        // stalled. The original monotonic deadline never moves forward.
        let authority_expired = async {
            if let Some(InvitationAuthority::Session(invitation)) = session_authority {
                invitation.wait_session_expired().await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        tokio::select! {
            _ = watcher_cancel.cancelled() => (),
            _ = lease_cancel.cancelled() => (),
            _ = tokio::time::sleep(remaining) => (),
            _ = authority_expired => (),
        }
        watcher_cancel.cancel();
        watcher_connection.close(0u32.into(), b"lease closed");
    });
    tasks.spawn(receive_datagrams(
        connection.clone(),
        state.clone(),
        associations.clone(),
        cancel.clone(),
        lease.fragment_ingress.clone(),
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
                #[cfg(test)]
                state.control_reads_started.fetch_add(1, Ordering::SeqCst);
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
                                        if authorize_session(&state, &hello.lease_id, connection.remote_id()).is_err() { let _ = reject(&mut send, Error::Expired, &state).await; continue; }
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
                        let service_control = &lease.udp_services[&handle];
                        let work = match service_control.begin() { Ok(work) => work, Err(error) => { let _ = reject(&mut send, error, &state).await; continue; } };
                        let Ok(permit) = state.udp.clone().try_acquire_owned() else { let _ = reject(&mut send, Error::Limit, &state).await; continue; };
                        let fragmented = request.operation == Operation::OpenFragmented;
                        let maximum = if fragmented { bridge::payload_capacity(&connection, service.max_datagram_bytes) } else { transport::payload_limit(&connection, service.max_datagram_bytes) };
                        let Some(max_payload) = maximum else { let _ = reject(&mut send, Error::CapabilityUnavailable, &state).await; continue; };
                        #[cfg(test)]
                        service_control.checkpoint(udp_service::Stage::BeforeBind, None).await;
                        let bind = if service.endpoint.is_ipv4() { "127.0.0.1:0" } else { "[::1]:0" };
                        if !service_control.enabled() { let _ = reject(&mut send, Error::ServiceDenied, &state).await; continue; }
                        let socket = tokio::select! { biased; _ = service_control.token().cancelled() => { continue; }, result = UdpSocket::bind(bind) => result };
                        let socket = match socket { Ok(s) => s, Err(_) => { let _ = reject(&mut send, Error::Destination, &state).await; continue; } };
                        let resource = udp_service::Resource::new(socket, permit, work.clone());
                        #[cfg(test)]
                        service_control.checkpoint(udp_service::Stage::AfterBind, resource.socket.local_addr().ok()).await;
                        if !service_control.enabled() { continue; }
                        if authorize_session(&state, &hello.lease_id, connection.remote_id()).is_err() { let _ = reject(&mut send, Error::Expired, &state).await; continue; }
                        let connected = tokio::select! { biased; _ = service_control.token().cancelled() => { continue; }, result = resource.socket.connect(service.endpoint) => result };
                        if connected.is_err() { let _ = reject(&mut send, Error::Destination, &state).await; continue; }
                        #[cfg(test)]
                        service_control.checkpoint(udp_service::Stage::AfterConnect, resource.socket.local_addr().ok()).await;
                        #[cfg(all(windows, debug_assertions, feature = "native-voice-test-support"))]
                        service_control.native_checkpoint(NativeVoiceRetirementStage::AfterConnect, resource.socket.local_addr().ok()).await;
                        if !service_control.enabled() { continue; }
                        if authorize_session(&state, &hello.lease_id, connection.remote_id()).is_err() { let _ = reject(&mut send, Error::Expired, &state).await; continue; }
                        let association = Arc::new(Association { service_handle: handle,
                            generation: service.revision, max_payload, cancel: cancel.child_token(), activity: Mutex::new(Instant::now()),
                            sequence: Mutex::new(SequenceWindow::default()), credit: AtomicU64::new(0),
                            limiter: lease.udp_limiters.lock().unwrap().entry(handle).or_insert_with(|| Arc::new(Mutex::new(UdpLimiter::new(&state.limits)))).clone(),
                            downstream_limiter: lease.downstream_limiters.lock().unwrap().entry(handle).or_insert_with(|| Arc::new(Mutex::new(UdpLimiter::downstream(service.udp_return_policy, &state.limits)))).clone(),
                            return_policy: service.udp_return_policy, first_forwarded: AtomicBool::new(false), first_forwarded_notify: Notify::new(),
                            fragmented: fragmented.then(|| Mutex::new(fragment::Reassembler::new(fragment::Scope { service_handle: handle, association_id: request.request_id, generation: service.revision }, lease.fragment_memory.clone()))),
                            lease_cancel: lease.cancel.clone(), lease_unix_deadline: lease.value.expires_at_unix_ms, lease_deadline,
                            session_authority: match &lease.invitation { Some(InvitationAuthority::Session(value)) => Some(value.clone()), _ => None },
                            resource,
                        });
                        #[cfg(test)]
                        service_control.checkpoint(udp_service::Stage::BeforePublish, association.socket().local_addr().ok()).await;
                        if let Err(error) = work.publish(&associations, request.request_id, &association) { let _ = reject(&mut send, error, &state).await; continue; }
                        #[cfg(test)]
                        service_control.checkpoint(udp_service::Stage::AfterPublish, association.socket().local_addr().ok()).await;
                        let response = Reply { service_handle: handle, association_id: request.request_id, generation: service.revision, max_payload: Some(max_payload), fragment_version: fragmented.then_some(2), ..Default::default() };
                        let replied = tokio::select! { biased; _ = service_control.token().cancelled() => Err(Error::ServiceDenied), result = reply(&mut send, &response, &state) => result };
                        if replied.is_err() { association.cancel.cancel(); associations.lock().unwrap().remove(&request.request_id); continue; }
                        if !association.fragment_authorized() { association.cancel.cancel(); associations.lock().unwrap().remove(&request.request_id); continue; }
                        let _ = send.finish();
                        #[cfg(test)]
                        service_control.checkpoint(udp_service::Stage::BeforeReturn, association.socket().local_addr().ok()).await;
                        state.metrics.udp_associations_opened.fetch_add(1, Ordering::Relaxed);
                        tasks.spawn(udp_return(connection.clone(), state.clone(), associations.clone(), request.request_id, association, work.clone()));
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
    let lease = authorize_session(state, &hello.lease_id, connection.remote_id())?;
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
    match request.operation {
        Operation::OpenFragmented
            if service.protocol != Protocol::Udp
                || service.required_capability != Capability::FragmentedDatagram =>
        {
            return Err(Error::CapabilityUnavailable);
        }
        Operation::Open if service.required_capability == Capability::FragmentedDatagram => {
            return Err(Error::CapabilityUnavailable);
        }
        _ => (),
    }
    if service.protocol == Protocol::Udp
        && !lease
            .udp_services
            .get(handle)
            .is_some_and(|control| control.enabled())
    {
        return Err(Error::ServiceDenied);
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
    fragment_ingress: Arc<Mutex<UdpLimiter>>,
) {
    let mut expiry = tokio::time::interval(Duration::from_millis(25));
    expiry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let data = tokio::select! {
            _ = cancel.cancelled() => break,
            _ = expiry.tick() => {
                for a in associations.lock().unwrap().values() {
                    if let Some(r) = &a.fragmented { r.lock().unwrap().expire(Instant::now()); }
                }
                continue;
            },
            data = connection.read_datagram() => match data { Ok(d) => d, Err(_) => break },
        };
        if data.first() != Some(&1) {
            // Debit every V2/unknown frame before decoding, including bad scope,
            // replay and malformed framing. The socket is reached only on completion.
            let allowed = fragment_ingress.lock().unwrap().take(data.len());
            let aggregate = state.budget.lock().unwrap().take(data.len());
            if !allowed || !aggregate || data.len() > bridge::MAX_FRAGMENT_FRAME_BYTES {
                state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            receive_fragment(&connection, &state, &associations, &cancel, &data).await;
            continue;
        }
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
            || a.fragmented.is_some()
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
            _ = a.resource.work.control().token().cancelled() => continue,
            result = tokio::time::timeout(Duration::from_millis(40), async {
                a.socket().writable().await.map_err(|_| Error::Io)?;
                a.resource.work.control().send_immediately(|| {
                    if !a.fragment_authorized() || cancel.is_cancelled() { return Err(Error::ServiceDenied); }
                    a.socket().try_send(&frame.payload).map_err(|_| Error::Io)
                })
            }) => result,
        };
        if matches!(delivered, Ok(Ok(n)) if n == frame.payload.len()) {
            a.first_forwarded.store(true, Ordering::Release);
            a.first_forwarded_notify.notify_one();
            state.metrics.udp_forwarded.fetch_add(1, Ordering::Relaxed);
        } else {
            state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

async fn receive_fragment(
    connection: &Connection,
    state: &State,
    associations: &Associations,
    cancel: &CancellationToken,
    data: &[u8],
) {
    let accepted = (|| {
        let frame = fragment::Frame::decode(data).ok()?;
        let a = associations
            .lock()
            .unwrap()
            .get(&frame.scope.association_id)
            .cloned()?;
        if cancel.is_cancelled()
            || !a.fragment_authorized()
            || a.activity.lock().unwrap().elapsed() >= state.limits.udp_idle_timeout
            || frame.scope.service_handle != a.service_handle
            || frame.scope.generation != a.generation
            || frame.total as usize > a.max_payload
            || connection.close_reason().is_some()
        {
            return None;
        }
        let reassembler = a.fragmented.as_ref()?;
        if !a.limiter.lock().unwrap().take(data.len()) {
            return None;
        }
        let output = {
            let mut reassembler = reassembler.lock().unwrap();
            // Reassembler -> ServiceGate. Disable never holds the gate while
            // clearing a reassembler, and the check itself never re-locks it.
            a.resource
                .work
                .control()
                .immediately(|| {
                    if !a.fragment_authorized() || cancel.is_cancelled() {
                        return Err(Error::ServiceDenied);
                    }
                    reassembler
                        .push(frame, Instant::now())
                        .map_err(bridge::error)
                })
                .ok()?
        };
        Some((
            a,
            output,
            frame.total as u64 + frame.count as u64 * fragment::HEADER_LEN as u64,
        ))
    })();
    let Some((a, output, credit)) = accepted else {
        state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
        return;
    };
    let fragment::Progress::Complete(payload) = output else {
        #[cfg(test)]
        a.resource
            .work
            .control()
            .checkpoint(udp_service::Stage::Pending, a.socket().local_addr().ok())
            .await;
        return;
    };
    let delivery = udp_service::Delivery::new(payload, a.resource.work.clone());
    #[cfg(test)]
    a.resource
        .work
        .control()
        .checkpoint(udp_service::Stage::Completed, a.socket().local_addr().ok())
        .await;
    #[cfg(all(windows, debug_assertions, feature = "native-voice-test-support"))]
    a.resource
        .work
        .control()
        .native_checkpoint(
            NativeVoiceRetirementStage::Completed,
            a.socket().local_addr().ok(),
        )
        .await;
    if !a.fragment_authorized() {
        return;
    }
    let delivered = tokio::select! {
        biased;
        _ = cancel.cancelled() => return,
        _ = a.cancel.cancelled() => return,
        _ = a.lease_cancel.cancelled() => return,
        _ = a.resource.work.control().token().cancelled() => return,
        _ = tokio::time::sleep_until(tokio::time::Instant::from_std(a.lease_deadline)) => return,
        result = tokio::time::timeout(Duration::from_millis(40), async {
            a.socket().writable().await.map_err(|_| Error::Io)?;
            a.resource.work.control().send_immediately(|| {
                if !a.fragment_authorized() || cancel.is_cancelled() { return Err(Error::ServiceDenied); }
                a.socket().try_send(delivery.payload.as_ref()).map_err(|_| Error::Io)
            })
        }) => result,
    };
    if matches!(delivered, Ok(Ok(n)) if n == delivery.payload.as_ref().len()) {
        *a.activity.lock().unwrap() = Instant::now();
        let _ = a
            .credit
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(credit).min(65536))
            });
        a.first_forwarded.store(true, Ordering::Release);
        a.first_forwarded_notify.notify_one();
        state.metrics.udp_forwarded.fetch_add(1, Ordering::Relaxed);
    } else {
        state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed);
    }
    // payload's RAII memory reservation stays alive through the awaited UDP send.
}

async fn udp_return(
    connection: Connection,
    state: Arc<State>,
    associations: Associations,
    id: u64,
    a: Arc<Association>,
    _work: udp_service::Work,
) {
    let _active = ActiveGuard::new(state.metrics.clone(), ActiveKind::Udp);
    // A full UDP-family buffer detects oversized payloads without forwarding a truncated prefix.
    let mut buffer = vec![0u8; 65536];
    let mut sequence = 0u32;
    let mut message_ids = fragment::MessageIds::default();
    loop {
        let idle_deadline = tokio::time::Instant::from_std(*a.activity.lock().unwrap())
            + state.limits.udp_idle_timeout;
        tokio::select! {
            biased;
            _ = a.cancel.cancelled() => break,
            _ = a.resource.work.control().token().cancelled() => break,
            _ = tokio::time::sleep_until(idle_deadline) => {
                if a.activity.lock().unwrap().elapsed() >= state.limits.udp_idle_timeout { break; }
            }
            received = a.socket().recv(&mut buffer) => {
                let size = match received { Ok(n) => n, Err(_) => break };
                #[cfg(test)]
                a.resource.work.control().checkpoint(udp_service::Stage::Return, a.socket().local_addr().ok()).await;
                // A fast destination may answer before the sender task stores its success.
                // Wait only for that bounded local send, never enable unsolicited traffic.
                if !a.first_forwarded.load(Ordering::Acquire) {
                    let notified = a.first_forwarded_notify.notified();
                    if !a.first_forwarded.load(Ordering::Acquire) {
                        tokio::select! {
                            biased;
                            _ = a.cancel.cancelled() => break,
                            _ = a.resource.work.control().token().cancelled() => break,
                            _ = tokio::time::timeout(Duration::from_millis(40), notified) => (),
                        }
                    }
                    if !a.first_forwarded.load(Ordering::Acquire) {
                        state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed); continue;
                    }
                }
                if !a.fragment_authorized() || a.activity.lock().unwrap().elapsed() >= state.limits.udp_idle_timeout { break; }
                let Some(max) = (if a.fragmented.is_some() { bridge::payload_capacity(&connection, a.max_payload) } else { transport::payload_limit(&connection, a.max_payload) }) else { break; };
                if size > max { state.metrics.udp_oversize.fetch_add(1, Ordering::Relaxed); continue; }
                if a.fragmented.is_some() {
                    let result = send_fragmented_return(&connection, &state, &a, id, &buffer[..size], &mut message_ids);
                    if result.is_ok() { state.metrics.udp_returned.fetch_add(1, Ordering::Relaxed); }
                    else { state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed); }
                    continue;
                }
                let cost = (size + DATAGRAM_HEADER_LEN) as u64;
                let permitted = match a.return_policy {
                    UdpReturnPolicy::ReplyBudget => a.credit.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(cost)).is_ok()
                        && a.limiter.lock().unwrap().take(size + DATAGRAM_HEADER_LEN),
                    UdpReturnPolicy::AuthorizedDownstream { .. } => a.downstream_limiter.lock().unwrap().take(size + DATAGRAM_HEADER_LEN),
                };
                if !permitted || !state.budget.lock().unwrap().take(size + DATAGRAM_HEADER_LEN) {
                    state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed); continue;
                }
                let frame = DatagramFrame { service_handle: a.service_handle, association_id: id, generation: a.generation,
                    sequence, payload: bytes::Bytes::copy_from_slice(&buffer[..size]) };
                let Some(next) = sequence.checked_add(1) else { break; }; sequence = next;
                if frame.encode().and_then(|bytes| a.resource.work.control().send_immediately(|| {
                    if !a.fragment_authorized() { return Err(Error::ServiceDenied); }
                    connection.send_datagram(bytes).map_err(|_| Error::Transport)
                })).is_ok() {
                    state.metrics.udp_returned.fetch_add(1, Ordering::Relaxed);
                } else { state.metrics.udp_dropped.fetch_add(1, Ordering::Relaxed); }
            }
        }
    }
    a.cancel.cancel();
    if let Some(r) = &a.fragmented {
        r.lock().unwrap().clear();
    }
    associations.lock().unwrap().remove(&id);
}

fn send_fragmented_return(
    connection: &Connection,
    state: &State,
    a: &Association,
    id: u64,
    payload: &[u8],
    ids: &mut fragment::MessageIds,
) -> Result<()> {
    let capacity = bridge::wire_capacity(connection).ok_or(Error::CapabilityUnavailable)?;
    let message = ids.take().map_err(bridge::error)?;
    let scope = fragment::Scope {
        service_handle: a.service_handle,
        association_id: id,
        generation: a.generation,
    };
    let count = payload
        .len()
        .div_ceil(capacity - fragment::HEADER_LEN)
        .max(1);
    let cost = payload.len() + count * fragment::HEADER_LEN;
    // Reserve the whole response budget before emitting any prefix. A failed send
    // does not refund credit and cannot amplify later traffic.
    if matches!(a.return_policy, UdpReturnPolicy::ReplyBudget)
        && a.credit
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                n.checked_sub(cost as u64)
            })
            .is_err()
    {
        return Err(Error::Limit);
    }
    for frame in
        fragment::Fragments::new(scope, message, payload, capacity).map_err(bridge::error)?
    {
        let bytes = frame.payload.len() + fragment::HEADER_LEN;
        let permitted = match a.return_policy {
            UdpReturnPolicy::ReplyBudget => a.limiter.lock().unwrap().take(bytes),
            UdpReturnPolicy::AuthorizedDownstream { .. } => {
                a.downstream_limiter.lock().unwrap().take(bytes)
            }
        };
        let aggregate = state.budget.lock().unwrap().take(bytes);
        if !permitted || !aggregate || !a.fragment_authorized() {
            return Err(Error::Limit);
        }
        a.resource.work.control().send_immediately(|| {
            if !a.fragment_authorized() {
                return Err(Error::ServiceDenied);
            }
            bridge::send_fragment(connection, frame, capacity)
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod fragment_authority_tests {
    use super::*;
    #[test]
    fn original_lease_token_and_both_clocks_are_enforced_before_watcher_runs() {
        let downstream = CancellationToken::new();
        let original = CancellationToken::new();
        let unix = unix_millis() + 60_000;
        let monotonic = Instant::now() + Duration::from_secs(60);
        assert!(fragment_authority_valid(
            &downstream,
            &original,
            unix,
            monotonic,
            None,
        ));
        original.cancel();
        assert!(!downstream.is_cancelled(), "watcher_was_not_run");
        assert!(!fragment_authority_valid(
            &downstream,
            &original,
            unix,
            monotonic,
            None,
        ));
        let original = CancellationToken::new();
        assert!(!fragment_authority_valid(
            &downstream,
            &original,
            unix_millis(),
            monotonic,
            None,
        ));
        assert!(!fragment_authority_valid(
            &downstream,
            &original,
            unix,
            Instant::now(),
            None,
        ));
    }
}
