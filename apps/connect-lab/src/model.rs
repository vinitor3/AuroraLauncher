use iroh::{EndpointAddr, EndpointId, SecretKey};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const ALPN: &[u8] = b"aurora/connect-lab/1";
pub const SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Protocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    ReliableStream,
    Datagram,
}

/// Host-approved behavior for a UDP service; never supplied by a remote peer.
/// Neither policy authenticates a voice mod or changes its advertised address.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum UdpReturnPolicy {
    /// At most the authenticated ingress byte budget, including Aurora framing.
    #[default]
    ReplyBudget,
    /// Opaque downstream after first successful ingress, with per-peer/service bounds.
    /// Inactivity still expires according to authenticated guest ingress, not replies.
    AuthorizedDownstream {
        bytes_per_second: u32,
        datagrams_per_second: u32,
    },
}

#[derive(Debug, Clone)]
pub struct Service {
    pub service_id: String,
    pub protocol: Protocol,
    pub endpoint: SocketAddr,
    pub revision: u32,
    pub max_datagram_bytes: usize,
    pub required_capability: Capability,
    pub udp_return_policy: UdpReturnPolicy,
}

#[derive(Debug, Clone)]
pub struct Lease {
    pub lease_id: String,
    pub peer: EndpointId,
    pub expires_at_unix_ms: u64,
    pub services: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub handshake_timeout: Duration,
    pub idle_timeout: Duration,
    pub udp_idle_timeout: Duration,
    pub max_peers: usize,
    pub max_tcp_per_peer: usize,
    pub max_tcp_total: usize,
    pub max_udp_per_peer: usize,
    pub max_udp_total: usize,
    pub max_control_bytes: usize,
    pub control_per_second: u32,
    pub control_burst: u32,
    pub datagrams_per_second: u32,
    pub datagram_burst: u32,
    pub udp_bytes_per_second: u32,
    pub session_bytes_per_second: u32,
    pub datagram_receive_buffer: Option<usize>,
    pub datagram_send_buffer: usize,
    pub max_requests_per_connection: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            handshake_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(120),
            udp_idle_timeout: Duration::from_secs(30),
            max_peers: 8,
            max_tcp_per_peer: 16,
            max_tcp_total: 64,
            max_udp_per_peer: 16,
            max_udp_total: 128,
            max_control_bytes: 16 * 1024,
            control_per_second: 20,
            control_burst: 40,
            datagrams_per_second: 200,
            datagram_burst: 40,
            udp_bytes_per_second: 250_000,
            session_bytes_per_second: 2_500_000,
            datagram_receive_buffer: Some(16 * 1024),
            datagram_send_buffer: 12 * 1024,
            max_requests_per_connection: 4096,
        }
    }
}

impl Limits {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.handshake_timeout.is_zero()
            || self.handshake_timeout > Duration::from_secs(60)
            || self.idle_timeout.is_zero()
            || self.idle_timeout > Duration::from_secs(600)
            || self.udp_idle_timeout.is_zero()
            || self.udp_idle_timeout > Duration::from_secs(300)
            || !(1..=8).contains(&self.max_peers)
            || !(1..=16).contains(&self.max_tcp_per_peer)
            || !(1..=64).contains(&self.max_tcp_total)
            || !(1..=16).contains(&self.max_udp_per_peer)
            || !(1..=128).contains(&self.max_udp_total)
            || !(128..=16384).contains(&self.max_control_bytes)
            || self.control_per_second == 0
            || self.control_burst == 0
            || self.control_burst > 40
            || self.datagrams_per_second == 0
            || self.datagram_burst == 0
            || self.datagram_burst > 40
            || self.udp_bytes_per_second == 0
            || self.session_bytes_per_second == 0
            || self.max_requests_per_connection == 0
            || self.max_requests_per_connection > 65536
            || self.datagram_send_buffer > 16384
            || self.datagram_receive_buffer.is_some_and(|n| n > 65536)
        {
            return Err(Error::Configuration);
        }
        Ok(())
    }
}

pub struct HostConfig {
    pub secret_key: SecretKey,
    pub bind_addr: SocketAddr,
    pub session_id: String,
    pub services: Vec<Service>,
    pub leases: Vec<Lease>,
    pub limits: Limits,
}
pub struct GuestConfig {
    pub secret_key: SecretKey,
    pub bind_addr: SocketAddr,
    pub host: EndpointAddr,
    pub session_id: String,
    pub lease_id: String,
    pub capabilities: Vec<Capability>,
    pub limits: Limits,
}

#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Error {
    #[error("invalid_configuration")]
    Configuration,
    #[error("transport_failed")]
    Transport,
    #[error("timed_out")]
    Timeout,
    #[error("unauthorized")]
    Unauthorized,
    #[error("expired_or_revoked")]
    Expired,
    #[error("service_not_authorized")]
    ServiceDenied,
    #[error("capability_unavailable")]
    CapabilityUnavailable,
    #[error("stale_service_revision")]
    StaleRevision,
    #[error("invalid_protocol")]
    Protocol,
    #[error("replayed_request")]
    Replay,
    #[error("resource_limit")]
    Limit,
    #[error("destination_unavailable")]
    Destination,
    #[error("datagram_too_large")]
    DatagramTooLarge,
    #[error("closed")]
    Closed,
    #[error("local_io_failed")]
    Io,
}
impl Error {
    pub fn code(&self) -> String {
        self.to_string()
    }
}
pub type Result<T> = std::result::Result<T, Error>;

pub fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
pub(crate) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}

#[derive(Default)]
pub(crate) struct Metrics {
    pub destination_tcp_connects: AtomicU64,
    pub udp_associations_opened: AtomicU64,
    pub active_tcp: AtomicU64,
    pub active_udp: AtomicU64,
    pub active_peers: AtomicU64,
    pub denied: AtomicU64,
    pub udp_forwarded: AtomicU64,
    pub udp_returned: AtomicU64,
    pub udp_dropped: AtomicU64,
    pub udp_oversize: AtomicU64,
    pub tcp_bytes: AtomicU64,
}
#[derive(Debug, Default, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub destination_tcp_connects: u64,
    pub udp_associations_opened: u64,
    pub active_tcp: u64,
    pub active_udp: u64,
    pub active_peers: u64,
    pub denied: u64,
    pub udp_forwarded: u64,
    pub udp_returned: u64,
    pub udp_dropped: u64,
    pub udp_oversize: u64,
    pub tcp_bytes: u64,
}
impl Metrics {
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            destination_tcp_connects: self.destination_tcp_connects.load(Ordering::Relaxed),
            udp_associations_opened: self.udp_associations_opened.load(Ordering::Relaxed),
            active_tcp: self.active_tcp.load(Ordering::Relaxed),
            active_udp: self.active_udp.load(Ordering::Relaxed),
            active_peers: self.active_peers.load(Ordering::Relaxed),
            denied: self.denied.load(Ordering::Relaxed),
            udp_forwarded: self.udp_forwarded.load(Ordering::Relaxed),
            udp_returned: self.udp_returned.load(Ordering::Relaxed),
            udp_dropped: self.udp_dropped.load(Ordering::Relaxed),
            udp_oversize: self.udp_oversize.load(Ordering::Relaxed),
            tcp_bytes: self.tcp_bytes.load(Ordering::Relaxed),
        }
    }
}

pub(crate) struct ActiveGuard(pub Arc<Metrics>, pub ActiveKind);
pub(crate) enum ActiveKind {
    Tcp,
    Udp,
    Peer,
}
impl ActiveGuard {
    pub fn new(m: Arc<Metrics>, kind: ActiveKind) -> Self {
        let counter = match kind {
            ActiveKind::Tcp => &m.active_tcp,
            ActiveKind::Udp => &m.active_udp,
            ActiveKind::Peer => &m.active_peers,
        };
        counter.fetch_add(1, Ordering::Relaxed);
        Self(m, kind)
    }
}
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        let counter = match self.1 {
            ActiveKind::Tcp => &self.0.active_tcp,
            ActiveKind::Udp => &self.0.active_udp,
            ActiveKind::Peer => &self.0.active_peers,
        };
        counter.fetch_sub(1, Ordering::Relaxed);
    }
}
