//! Opt-in, owned loopback relay fixture, not a public server/provider integration.
//! Compiled only by the explicitly selected local-relay-lab feature.
//! HTTP has no TLS only on 127.0.0.1; end-to-end QUIC identity/TLS is untouched.
use crate::{EndpointId, Error, GuestConfig, HostConfig, Result, unix_millis};
use iroh::{Endpoint, RelayUrl};
use iroh_relay::{
    RelayConfig as ClientConfig, RelayMap,
    server::{
        self, Access, AccessControl, ClientRequest, ConnectionId, RelayConfig, Server, ServerConfig,
    },
};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    net::{Ipv4Addr, SocketAddr},
    num::NonZeroU32,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{sync::Notify, task::JoinHandle};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy)]
pub enum LocalRelayProfile {
    Normal,
    RateLimited {
        bytes_per_second: NonZeroU32,
        burst: NonZeroU32,
    },
}
#[derive(Debug, thiserror::Error)]
pub enum LocalRelayError {
    #[error("local_relay_proxy_scope_unverified")]
    Scope,
    #[error("local_relay_invalid_peers")]
    Peers,
    #[error("local_relay_start_failed")]
    Start,
    #[error("local_relay_cleanup_failed")]
    Cleanup,
}
const PROXY_ENV: [&str; 6] = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
];

#[cfg(any(windows, test))]
fn disabled_proxy_switch(value: std::result::Result<u32, u32>) -> bool {
    // Only absence and a confirmed zero are disabled. Access denied, a value of
    // the wrong registry type and any other query failure remain unverified.
    matches!(value, Ok(0) | Err(0x80070002) | Err(0x80070003))
}

fn require_no_proxy_scope() -> std::result::Result<(), LocalRelayError> {
    // The pinned Iroh net-report client inherits reqwest environment proxies.
    // There is no public setter for its no_proxy(): refuse before opening any
    // fixture or endpoint, without changing the process environment or settings.
    if PROXY_ENV
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
    {
        return Err(LocalRelayError::Scope);
    }
    #[cfg(windows)]
    {
        // hyper-util's Windows system matcher reads only this HKCU switch.
        // Also check it defensively if another consumer unifies system-proxy.
        // Never read ProxyServer, ProxyOverride, credentials or raw values.
        let switch = match windows_registry::CURRENT_USER
            .open("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        {
            Ok(key) => key
                .get_u32("ProxyEnable")
                .map_err(|error| error.code().0 as u32),
            Err(error) => Err(error.code().0 as u32),
        };
        if !disabled_proxy_switch(switch) {
            return Err(LocalRelayError::Scope);
        }
    }
    // The pinned optional system matcher has separate macOS behavior. Do not
    // claim this Windows/Linux scope gate verifies macOS system proxies.
    #[cfg(target_os = "macos")]
    return Err(LocalRelayError::Scope);
    #[cfg(not(target_os = "macos"))]
    Ok(())
}
struct Allowed {
    peers: HashSet<EndpointId>,
    connections: Mutex<HashMap<ConnectionId, EndpointId>>,
    denied: AtomicU64,
}
impl fmt::Debug for Allowed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalRelayAccess")
            .field("allowedPeers", &self.peers.len())
            .finish_non_exhaustive()
    }
}
impl AccessControl for Allowed {
    async fn on_connect(&self, request: &ClientRequest) -> Access {
        if !self.peers.contains(&request.endpoint_id()) {
            self.denied.fetch_add(1, Ordering::Relaxed);
            return Access::Deny {
                reason: Some("local_test_peer_denied".into()),
            };
        }
        let Ok(mut active) = self.connections.lock() else {
            return Access::Deny { reason: None };
        };
        // This bounds admitted relay clients. It is not a claim that upstream
        // bounds unauthenticated HTTP accepts (its accept limit is unimplemented).
        if active.len() >= 4
            || active
                .values()
                .filter(|peer| **peer == request.endpoint_id())
                .count()
                >= 2
        {
            self.denied.fetch_add(1, Ordering::Relaxed);
            return Access::Deny {
                reason: Some("local_test_connection_limit".into()),
            };
        }
        active.insert(request.connection_id(), request.endpoint_id());
        Access::Allow
    }
    fn on_disconnect(&self, _peer: EndpointId, id: ConnectionId) {
        if let Ok(mut active) = self.connections.lock() {
            active.remove(&id);
        }
    }
}
/// A relay URL/map derived only from this fixture's actual owned listening socket.
/// Callers cannot construct a public-provider URL or add credentials/fallback IPs.
#[derive(Clone)]
pub struct LocalRelayTicket {
    url: RelayUrl,
    peers: [EndpointId; 2],
    deadline: Instant,
    expires_at_unix_ms: u64,
    lifetime: Arc<Lifetime>,
}
#[derive(Default)]
struct Endpoints {
    creating: usize,
    live: Vec<Endpoint>,
}
struct Lifetime {
    cancel: CancellationToken,
    endpoints: Mutex<Endpoints>,
    changed: Notify,
}
/// Keep the owned HTTP listener alive until an in-flight endpoint bind has
/// either registered or closed, including cancellation of its caller.
pub(crate) struct EndpointCreation {
    lifetime: Arc<Lifetime>,
}
impl EndpointCreation {
    pub(crate) fn register(&self, endpoint: &Endpoint) -> Result<()> {
        let mut endpoints = self.lifetime.endpoints.lock().map_err(|_| Error::Closed)?;
        endpoints.live.push(endpoint.clone());
        if self.lifetime.cancel.is_cancelled() {
            Err(Error::Expired)
        } else {
            Ok(())
        }
    }
}
impl Drop for EndpointCreation {
    fn drop(&mut self) {
        if let Ok(mut endpoints) = self.lifetime.endpoints.lock() {
            endpoints.creating = endpoints.creating.saturating_sub(1);
        }
        self.lifetime.changed.notify_one();
    }
}
impl LocalRelayTicket {
    pub(crate) fn online_deadline(&self, timeout: Duration) -> Result<Instant> {
        self.check_current()?;
        let timeout = Instant::now()
            .checked_add(timeout)
            .ok_or(Error::Configuration)?;
        Ok(self.deadline.min(timeout))
    }
    pub(crate) async fn cancelled(&self) {
        self.lifetime.cancel.cancelled().await;
    }
    pub(crate) fn check_current(&self) -> Result<()> {
        if self.lifetime.cancel.is_cancelled()
            || Instant::now() >= self.deadline
            || unix_millis() >= self.expires_at_unix_ms
        {
            Err(Error::Expired)
        } else {
            require_no_proxy_scope().map_err(|_| Error::Configuration)
        }
    }
    pub(crate) fn begin_endpoint(&self) -> Result<EndpointCreation> {
        let mut endpoints = self.lifetime.endpoints.lock().map_err(|_| Error::Closed)?;
        self.check_current()?;
        endpoints.live.retain(|endpoint| !endpoint.is_closed());
        if endpoints.live.len() + endpoints.creating >= 4 {
            return Err(Error::Limit);
        }
        endpoints.creating += 1;
        Ok(EndpointCreation {
            lifetime: self.lifetime.clone(),
        })
    }
    fn check_identity_pair(&self, local: EndpointId, remote: EndpointId) -> Result<()> {
        self.check_current()?;
        if local == remote || !self.peers.contains(&local) || !self.peers.contains(&remote) {
            Err(Error::Configuration)
        } else {
            Ok(())
        }
    }
    pub(crate) fn check_host(&self, config: &HostConfig) -> Result<()> {
        if !config.bind_addr.ip().is_loopback()
            || config.bind_addr.port() != 0
            || config.leases.len() != 1
        {
            return Err(Error::Configuration);
        }
        let lease = &config.leases[0];
        self.check_identity_pair(config.secret_key.public(), lease.peer)?;
        if lease.expires_at_unix_ms <= unix_millis()
            || lease.expires_at_unix_ms > self.expires_at_unix_ms
        {
            return Err(Error::Configuration);
        }
        Ok(())
    }
    pub(crate) fn check_guest(&self, config: &GuestConfig) -> Result<()> {
        self.check_identity_pair(config.secret_key.public(), config.host.id)?;
        if !config.bind_addr.ip().is_loopback()
            || config.bind_addr.port() != 0
            || config.host.addrs.iter().any(
                |address| !matches!(address, iroh::TransportAddr::Relay(url) if url == &self.url),
            )
        {
            return Err(Error::Configuration);
        }
        Ok(())
    }
    pub(crate) fn map(&self) -> RelayMap {
        ClientConfig::new(self.url.clone(), None).into()
    }
    pub(crate) fn url(&self) -> RelayUrl {
        self.url.clone()
    }
}
#[derive(Clone, Debug)]
pub struct LocalRelayCounters {
    pub received_bytes: u64,
    pub sent_bytes: u64,
    pub received_packets: u64,
    pub sent_packets: u64,
    pub rate_limited_bytes: u64,
    pub rate_limited_connections: u64,
    pub denied_connections: u64,
    pub accepted_connections: u64,
    pub http_connections: u64,
}
pub struct LocalRelayFixture {
    ticket: LocalRelayTicket,
    address: SocketAddr,
    metrics: server::RelayMetrics,
    cancel: CancellationToken,
    worker: Option<JoinHandle<std::result::Result<(), LocalRelayError>>>,
    access: Arc<Allowed>,
}
impl LocalRelayFixture {
    pub async fn start(
        peers: [EndpointId; 2],
        profile: LocalRelayProfile,
    ) -> std::result::Result<Self, LocalRelayError> {
        Self::start_inner(peers, profile, None).await
    }
    /// Native desktop test only: one original Host and two independently keyed
    /// remote Guests. The existing relay-only ticket still binds its original pair.
    #[cfg(all(windows, debug_assertions, feature = "native-voice-test-support"))]
    pub async fn start_native_group(
        peers: [EndpointId; 3],
        profile: LocalRelayProfile,
    ) -> std::result::Result<Self, LocalRelayError> {
        Self::start_inner([peers[0], peers[1]], profile, Some(peers[2])).await
    }
    #[cfg(all(windows, debug_assertions, feature = "native-voice-test-support"))]
    pub fn bound_addr(&self) -> SocketAddr {
        self.address
    }
    async fn start_inner(
        peers: [EndpointId; 2],
        profile: LocalRelayProfile,
        extra: Option<EndpointId>,
    ) -> std::result::Result<Self, LocalRelayError> {
        require_no_proxy_scope()?;
        if peers[0] == peers[1] || extra.is_some_and(|peer| peers.contains(&peer)) {
            return Err(LocalRelayError::Peers);
        }
        let started = Instant::now();
        let lifetime = Arc::new(Lifetime {
            cancel: CancellationToken::new(),
            endpoints: Mutex::new(Endpoints::default()),
            changed: Notify::new(),
        });
        let expires_at_unix_ms = unix_millis().saturating_add(60_000);
        let access = Arc::new(Allowed {
            peers: peers.into_iter().chain(extra).collect(),
            connections: Mutex::new(HashMap::new()),
            denied: AtomicU64::new(0),
        });
        let mut relay = RelayConfig::new((Ipv4Addr::LOCALHOST, 0));
        relay.access = access.clone();
        relay.key_cache_capacity = Some(32);
        if let LocalRelayProfile::RateLimited {
            bytes_per_second,
            burst,
        } = profile
        {
            let mut limit = server::ClientRateLimit::new(bytes_per_second);
            limit.max_burst_bytes = Some(burst);
            relay.limits.client_rx = Some(limit);
        }
        let mut config = ServerConfig::default();
        config.relay = Some(relay);
        config.quic = None;
        config.metrics_addr = None;
        // TLS/ACME are disabled in RelayConfig::new; QUIC discovery and the
        // metrics HTTP listener are also disabled explicitly. No tracing setup.
        let mut server = Server::spawn(config)
            .await
            .map_err(|_| LocalRelayError::Start)?;
        let Some(address) = server.http_addr() else {
            let _ = server.shutdown().await;
            return Err(LocalRelayError::Start);
        };
        if address.ip() != Ipv4Addr::LOCALHOST
            || address.port() == 0
            || server.https_addr().is_some()
            || server.quic_addr().is_some()
        {
            let _ = server.shutdown().await;
            return Err(LocalRelayError::Start);
        }
        let url: RelayUrl = match format!("http://127.0.0.1:{}/", address.port()).parse() {
            Ok(url) => url,
            Err(_) => {
                let _ = server.shutdown().await;
                return Err(LocalRelayError::Start);
            }
        };
        let ticket = LocalRelayTicket {
            url,
            peers,
            deadline: started + Duration::from_secs(60),
            expires_at_unix_ms,
            lifetime: lifetime.clone(),
        };
        let metrics = server.metrics().clone();
        let cancel = lifetime.cancel.clone();
        let shutdown = cancel.clone();
        let worker = tokio::spawn(async move {
            let unexpected_exit = tokio::select! {
                _ = shutdown.cancelled() => false,
                _ = tokio::time::sleep_until((started+Duration::from_secs(60)).into()) => false,
                _ = server.join() => true,
            };
            // Invalidate all ticket clones and close relay-only endpoints before
            // releasing the HTTP socket, preventing retries to a reoccupied port.
            shutdown.cancel();
            loop {
                let changed = lifetime.changed.notified();
                let (endpoints, creating) = {
                    let mut tracked = lifetime
                        .endpoints
                        .lock()
                        .map_err(|_| LocalRelayError::Cleanup)?;
                    (std::mem::take(&mut tracked.live), tracked.creating)
                };
                for endpoint in endpoints {
                    endpoint.close().await;
                }
                if creating == 0 {
                    break;
                }
                changed.await;
            }
            // join already awaited the supervisor in this branch; do not poll
            // its JoinHandle twice. The SDK has closed its child tasks itself.
            if unexpected_exit {
                return Err(LocalRelayError::Cleanup);
            }
            server
                .shutdown()
                .await
                .map_err(|_| LocalRelayError::Cleanup)
        });
        Ok(Self {
            ticket,
            address,
            metrics,
            cancel,
            worker: Some(worker),
            access,
        })
    }
    pub fn ticket(&self) -> LocalRelayTicket {
        self.ticket.clone()
    }
    pub fn counters(&self) -> LocalRelayCounters {
        let m = &self.metrics.server;
        LocalRelayCounters {
            received_bytes: m.bytes_recv.get(),
            sent_bytes: m.bytes_sent.get(),
            received_packets: m.send_packets_recv.get(),
            sent_packets: m.send_packets_sent.get(),
            rate_limited_bytes: m.bytes_rx_ratelimited_total.get(),
            rate_limited_connections: m.conns_rx_ratelimited_total.get(),
            denied_connections: self.access.denied.load(Ordering::Relaxed),
            accepted_connections: m.accepts.get(),
            http_connections: m.http_connections.get(),
        }
    }
    pub async fn shutdown(mut self) -> std::result::Result<(), LocalRelayError> {
        self.cancel.cancel();
        if let Some(worker) = self.worker.take() {
            worker.await.map_err(|_| LocalRelayError::Cleanup)??;
        }
        let listener =
            std::net::TcpListener::bind(self.address).map_err(|_| LocalRelayError::Cleanup)?;
        drop(listener);
        Ok(())
    }
}
impl Drop for LocalRelayFixture {
    fn drop(&mut self) {
        self.cancel.cancel(); /* no task abort; the server owner closes asynchronously */
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn actual_owned_relay_binds_only_loopback_and_shuts_down() {
        let left = crate::generate_key();
        let right = crate::generate_key();
        let relay =
            LocalRelayFixture::start([left.public(), right.public()], LocalRelayProfile::Normal)
                .await
                .unwrap();
        assert!(relay.address.ip().is_loopback() && relay.address.port() != 0);
        let ticket = relay.ticket();
        assert!(
            ticket
                .map()
                .get(&ticket.url())
                .is_some_and(|config| config.quic.is_none() && config.auth_token.is_none())
        );
        let m = relay.counters();
        assert_eq!(m.received_bytes, 0);
        assert_eq!(m.sent_bytes, 0);
        relay.shutdown().await.unwrap();
        assert!(matches!(ticket.check_current(), Err(Error::Expired)));
    }

    #[tokio::test]
    async fn drop_invalidates_retained_ticket_and_reaps_owned_socket() {
        let left = crate::generate_key();
        let right = crate::generate_key();
        let relay =
            LocalRelayFixture::start([left.public(), right.public()], LocalRelayProfile::Normal)
                .await
                .unwrap();
        let address = relay.address;
        let ticket = relay.ticket();
        drop(relay);
        assert!(matches!(ticket.check_current(), Err(Error::Expired)));
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Ok(listener) = std::net::TcpListener::bind(address) {
                    drop(listener);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let config = GuestConfig {
            secret_key: right,
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            host: crate::EndpointAddr::new(left.public()),
            session_id: "dropped".into(),
            lease_id: "dropped".into(),
            capabilities: vec![crate::Capability::ReliableStream],
            limits: crate::Limits::default(),
        };
        assert!(matches!(
            crate::Guest::connect_local_relay(config, &ticket).await,
            Err(Error::Expired)
        ));
    }

    #[tokio::test]
    async fn relay_acl_rejects_an_actual_third_identity_before_relaying_payload() {
        let left = crate::generate_key();
        let right = crate::generate_key();
        let relay =
            LocalRelayFixture::start([left.public(), right.public()], LocalRelayProfile::Normal)
                .await
                .unwrap();
        let ticket = relay.ticket();
        // This private transport-only fixture deliberately bypasses the public
        // identity gate to exercise the real authenticated relay AccessControl.
        let limits = crate::Limits {
            handshake_timeout: Duration::from_millis(400),
            ..crate::Limits::default()
        };
        let result = crate::transport::endpoint_in_mode(
            crate::generate_key(),
            "127.0.0.1:0".parse().unwrap(),
            &limits,
            crate::transport::EndpointMode::LocalRelay(ticket),
        )
        .await;
        assert!(matches!(result, Err(Error::Timeout)));
        let counters = relay.counters();
        assert!(
            counters.denied_connections > 0,
            "authenticated_relay_acl_must_deny"
        );
        assert_eq!(counters.accepted_connections, 0);
        assert_eq!(counters.received_bytes, 0);
        assert_eq!(counters.sent_bytes, 0);
        relay.shutdown().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_relay_interrupts_pending_online_and_releases_creation_guard() {
        let left = crate::generate_key();
        let right = crate::generate_key();
        let relay =
            LocalRelayFixture::start([left.public(), right.public()], LocalRelayProfile::Normal)
                .await
                .unwrap();
        let ticket = relay.ticket();
        assert!(ticket.online_deadline(Duration::from_secs(60)).unwrap() <= ticket.deadline);
        let limits = crate::Limits {
            handshake_timeout: Duration::from_secs(60),
            ..crate::Limits::default()
        };
        // The owned relay really denies this third identity. Thus online cannot
        // complete before we cancel; a timeout-only implementation would wait60s.
        let pending_ticket = ticket.clone();
        let pending = tokio::spawn(async move {
            crate::transport::endpoint_in_mode(
                crate::generate_key(),
                "127.0.0.1:0".parse().unwrap(),
                &limits,
                crate::transport::EndpointMode::LocalRelay(pending_ticket),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            while relay.counters().denied_connections == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            !pending.is_finished(),
            "online_must_really_be_pending_before_cancel"
        );
        tokio::time::timeout(Duration::from_secs(3), async {
            relay.shutdown().await.unwrap();
            assert!(matches!(pending.await.unwrap(), Err(Error::Expired)));
        })
        .await
        .expect("cancel_must_close_and_rebind_without_waiting60s");
        assert!(matches!(ticket.check_current(), Err(Error::Expired)));
    }

    #[tokio::test]
    async fn duplicate_relay_identities_are_rejected_before_start() {
        let key = crate::generate_key();
        assert!(matches!(
            LocalRelayFixture::start([key.public(), key.public()], LocalRelayProfile::Normal).await,
            Err(LocalRelayError::Peers)
        ));
    }

    #[test]
    fn windows_proxy_gate_rejects_enabled_and_ambiguous_values() {
        assert!(disabled_proxy_switch(Ok(0)));
        assert!(disabled_proxy_switch(Err(0x80070002)));
        assert!(disabled_proxy_switch(Err(0x80070003)));
        assert!(!disabled_proxy_switch(Ok(1)));
        assert!(!disabled_proxy_switch(Ok(u32::MAX)));
        assert!(!disabled_proxy_switch(Err(0x80070005)));
        assert!(!disabled_proxy_switch(Err(0x8007000D)));
    }

    #[tokio::test]
    #[ignore = "internal exact child fixture; the parent sets proxy env before process/runtime start"]
    async fn inherited_proxy_child_fixture() {
        let left = crate::generate_key();
        let right = crate::generate_key();
        assert!(matches!(
            LocalRelayFixture::start([left.public(), right.public()], LocalRelayProfile::Normal)
                .await,
            Err(LocalRelayError::Scope)
        ));
    }

    #[test]
    fn inherited_proxy_is_rejected_in_owned_child_without_contacting_proxy() {
        let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        proxy.set_nonblocking(true).unwrap();
        let address = proxy.local_addr().unwrap();
        for name in PROXY_ENV {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child.args([
                "--exact",
                "local_relay::tests::inherited_proxy_child_fixture",
                "--ignored",
            ]);
            for clear in PROXY_ENV {
                child.env_remove(clear);
            }
            child.env(name, format!("http://127.0.0.1:{}/", address.port()));
            let mut child = child
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let exit = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("proxy_child_deadline")
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            assert!(exit.success(), "proxy_gate_child_failed");
            assert!(
                matches!(proxy.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
                "must_not_contact_even_owned_proxy"
            );
        }
    }
}
