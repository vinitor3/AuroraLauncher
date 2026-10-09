//! Signed V2 admission through real agents, confined to owned loopback sockets.
//! Payloads are synthetic; the main case sends 15_262 UDP application bytes in
//! both directions combined. No test selects a relay, public address or kit.
use aurora_connect_lab::*;
use bytes::Bytes;
use iroh::{
    Endpoint, NetReportConfig, RelayMode,
    endpoint::{
        ConnectOptions, QuicTransportConfig, RecvStream, SendStream, ZeroRttStatus, presets,
    },
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    future::Future,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    task::JoinHandle,
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;

const DEADLINE: Duration = Duration::from_secs(5);
const MIB: usize = 1024 * 1024;
fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn limits() -> Limits {
    Limits {
        handshake_timeout: DEADLINE,
        ..Limits::default()
    }
}
async fn bounded<T>(future: impl Future<Output = T>) -> T {
    timeout(DEADLINE, future)
        .await
        .expect("fragmented_fixture_deadline")
}
fn services(tcp: SocketAddr, udp: SocketAddr, policy: UdpReturnPolicy) -> Vec<Service> {
    vec![
        Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: tcp,
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        },
        Service {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            endpoint: udp,
            revision: 1,
            max_datagram_bytes: 4096,
            required_capability: Capability::FragmentedDatagram,
            udp_return_policy: policy,
        },
    ]
}

struct Fixture {
    host: Option<Host>,
    invitation: Invitation,
    guest_key: SecretKey,
    guests: Vec<Guest>,
    tcp: Option<TcpListener>,
    udp: Arc<UdpSocket>,
    tcp_addrs: Vec<SocketAddr>,
    udp_addrs: Vec<SocketAddr>,
    tasks: Vec<JoinHandle<()>>,
    stop: CancellationToken,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.cancel();
        for task in &self.tasks {
            task.abort();
        }
    }
}
impl Fixture {
    async fn new(ttl: Duration, policy: UdpReturnPolicy) -> Self {
        let tcp = TcpListener::bind(local()).await.unwrap();
        let udp = Arc::new(UdpSocket::bind(local()).await.unwrap());
        let tcp_addr = tcp.local_addr().unwrap();
        let udp_addr = udp.local_addr().unwrap();
        let registry = services(tcp_addr, udp_addr, policy);
        let reservation = std::net::UdpSocket::bind(local()).unwrap();
        let bind_addr = reservation.local_addr().unwrap();
        let host_key = generate_key();
        let guest_key = generate_key();
        let issued = Invitation::issue_fragmented(
            &host_key,
            guest_key.public(),
            "fragmented-invitation",
            bind_addr,
            &registry,
            unix_millis() + ttl.as_millis() as u64,
        )
        .unwrap();
        let invitation =
            Invitation::verify_fragmented(&issued.encode(), host_key.public(), guest_key.public())
                .unwrap();
        drop(reservation);
        let host = Host::start_invited_fragmented(
            HostConfig {
                secret_key: host_key,
                bind_addr,
                session_id: invitation.session_id().into(),
                services: registry,
                leases: vec![invitation.lease()],
                limits: limits(),
            },
            &invitation,
        )
        .await
        .unwrap();
        let mut udp_addrs = vec![udp_addr];
        udp_addrs.extend(host.bound_sockets());
        Self {
            host: Some(host),
            invitation,
            guest_key,
            guests: Vec::new(),
            tcp: Some(tcp),
            udp,
            tcp_addrs: vec![tcp_addr],
            udp_addrs,
            tasks: Vec::new(),
            stop: CancellationToken::new(),
        }
    }
    fn host(&self) -> &Host {
        self.host.as_ref().unwrap()
    }
    fn verify_current(&self) -> Result<()> {
        Invitation::verify_fragmented(
            &self.invitation.encode(),
            self.host().id(),
            self.guest_key.public(),
        )
        .map(|_| ())
    }
    async fn guest(&mut self) -> Guest {
        let guest =
            Guest::connect_invited_fragmented(self.guest_key.clone(), &self.invitation, limits())
                .await
                .unwrap();
        self.udp_addrs.extend(guest.bound_sockets());
        self.guests.push(guest.clone());
        guest
    }
    async fn idle(&self) {
        bounded(async {
            while self.host().metrics().active_peers != 0 {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
    }
    async fn no_destinations(&self) {
        self.idle().await;
        let m = self.host().metrics();
        assert_eq!(
            (
                m.destination_tcp_connects,
                m.udp_associations_opened,
                m.udp_forwarded,
                m.udp_returned
            ),
            (0, 0, 0, 0)
        );
        assert_eq!((m.active_tcp, m.active_udp), (0, 0));
        let mut buffer = [0; 64];
        let (tcp, udp) = tokio::join!(
            timeout(
                Duration::from_millis(40),
                self.tcp.as_ref().unwrap().accept()
            ),
            timeout(Duration::from_millis(40), self.udp.recv_from(&mut buffer))
        );
        assert!(tcp.is_err() && udp.is_err(), "unauthorized_destination_io");
    }
    fn hello(&self) -> Value {
        json!({"schemaVersion":1,"sessionId":self.invitation.session_id(),
            "leaseId":self.invitation.lease().lease_id,"capabilities":["reliableStream","fragmentedDatagram"],
            "invitation":{"digest":self.invitation.digest(),"generation":1}})
    }
    fn wrong_proof(&self) -> Value {
        let mut hello = self.hello();
        hello["invitation"]["digest"] = json!("0".repeat(64));
        hello
    }
    async fn raw_endpoint(&mut self) -> Endpoint {
        let transport = QuicTransportConfig::builder()
            .send_observed_address_reports(false)
            .receive_observed_address_reports(false)
            .build();
        let endpoint = Endpoint::builder(presets::Minimal)
            .clear_ip_transports()
            .clear_relay_transports()
            .clear_address_lookup()
            .relay_mode(RelayMode::Disabled)
            .net_report_config(NetReportConfig::minimal())
            .secret_key(self.guest_key.clone())
            .keylog(false)
            .transport_config(transport)
            .bind_addr(local())
            .unwrap()
            .bind()
            .await
            .unwrap();
        self.udp_addrs.extend(endpoint.bound_sockets());
        endpoint
    }
    async fn cleanup(mut self) {
        for guest in &self.guests {
            bounded(guest.shutdown()).await;
        }
        self.guests.clear();
        let host = self.host.take().unwrap();
        bounded(host.shutdown()).await;
        let m = host.metrics();
        assert_eq!((m.active_peers, m.active_tcp, m.active_udp), (0, 0, 0));
        drop(host);
        self.stop.cancel();
        for task in self.tasks.drain(..) {
            bounded(task).await.unwrap();
        }
        self.tcp.take();
        // Move the last fixture reference out before rebinding its original port.
        let placeholder = Arc::new(UdpSocket::bind(local()).await.unwrap());
        self.udp_addrs.push(placeholder.local_addr().unwrap());
        drop(std::mem::replace(&mut self.udp, placeholder));
        let tcp_addrs = std::mem::take(&mut self.tcp_addrs);
        let udp_addrs = std::mem::take(&mut self.udp_addrs);
        drop(self);
        let mut tcp_rebound = Vec::new();
        let mut udp_rebound = Vec::new();
        for addr in tcp_addrs {
            tcp_rebound.push(
                TcpListener::bind(addr)
                    .await
                    .expect("tcp_cleanup_rebind_failed"),
            );
        }
        for addr in udp_addrs.into_iter().collect::<BTreeSet<_>>() {
            udp_rebound.push(
                UdpSocket::bind(addr)
                    .await
                    .expect("udp_cleanup_rebind_failed"),
            );
        }
    }
}

async fn udp_echo(origin: &UdpSocket, bridge: SocketAddr, size: usize, tag: u8) {
    let payload = vec![tag; size];
    assert_eq!(origin.send_to(&payload, bridge).await.unwrap(), size);
    let mut returned = [0u8; 4098];
    let (count, source) = bounded(origin.recv_from(&mut returned)).await.unwrap();
    assert!(
        source == bridge && count == size && returned[..count] == payload,
        "udp_return_scope_or_bytes"
    );
}
async fn wait_bytes(counter: &AtomicUsize, above: usize) {
    bounded(async {
        while counter.load(Ordering::SeqCst) <= above {
            sleep(Duration::from_millis(2)).await;
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn signed_fragmented_bridge_two_origins_and_tcp_progress_with_full_cleanup() {
    let mut f = Fixture::new(Duration::from_secs(30), UdpReturnPolicy::ReplyBudget).await;
    let guest = f.guest().await;
    let bridge = guest
        .udp_bridge_fragmented("aux", 1, local())
        .await
        .unwrap();
    assert_eq!(bridge.max_payload(), Some(4096));
    f.udp_addrs.push(bridge.local_addr());
    let a = UdpSocket::bind(local()).await.unwrap();
    let b = UdpSocket::bind(local()).await.unwrap();
    f.udp_addrs
        .extend([a.local_addr().unwrap(), b.local_addr().unwrap()]);
    let sources = Arc::new(Mutex::new(BTreeSet::new()));
    let seen = sources.clone();
    let udp = f.udp.clone();
    let stop = f.stop.clone();
    f.tasks.push(tokio::spawn(async move {
        let mut bytes = [0u8; 4098];
        for _ in 0..4 {
            let received = tokio::select! { _ = stop.cancelled() => return, value = udp.recv_from(&mut bytes) => value };
            let (count, source) = received.unwrap();
            assert!(count <= 4096, "unexpected_oversize_at_destination");
            seen.lock().unwrap().insert(source);
            assert_eq!(udp.send_to(&bytes[..count], source).await.unwrap(), count);
        }
    }));
    let listener = f.tcp.take().unwrap();
    let progress = Arc::new(AtomicUsize::new(0));
    let count = progress.clone();
    let stop = f.stop.clone();
    f.tasks.push(tokio::spawn(async move {
        let accepted = tokio::select! { _ = stop.cancelled() => return, value = listener.accept() => value };
        let (mut socket, _) = accepted.unwrap();
        let mut bytes = [0; 4096];
        loop {
            let n = tokio::select! { _ = stop.cancelled() => return, value = socket.read(&mut bytes) => value }.unwrap();
            if n == 0 { break; }
            assert!(bytes[..n].iter().all(|b| *b == 0x53), "tcp_upstream_changed");
            count.fetch_add(n, Ordering::SeqCst);
            socket.write_all(&bytes[..n]).await.unwrap();
        }
        socket.shutdown().await.unwrap();
    }));
    let before = guest.connection().stats();
    let (mut send, mut recv) = guest.open_tcp("game", 1).await.unwrap();
    let (tcp, during) = tokio::join!(
        async {
            let ((), result) = tokio::join!(
                async {
                    for _ in 0..256 {
                        send.write_all(&[0x53; 4096]).await.unwrap();
                        sleep(Duration::from_millis(2)).await;
                    }
                    send.finish().unwrap();
                },
                async { bounded(recv.read_to_end(MIB + 1)).await.unwrap() }
            );
            assert!(
                result.len() == MIB && result.iter().all(|b| *b == 0x53),
                "tcp_downstream_changed"
            );
            MIB
        },
        async {
            wait_bytes(&progress, 0).await;
            let first = progress.load(Ordering::SeqCst);
            tokio::join!(
                udp_echo(&a, bridge.local_addr(), 0, 0),
                udp_echo(&b, bridge.local_addr(), 1427, 0x71)
            );
            wait_bytes(&progress, first).await;
            tokio::join!(
                udp_echo(&a, bridge.local_addr(), 2108, 0x19),
                udp_echo(&b, bridge.local_addr(), 4096, 0xa7)
            );
            progress.load(Ordering::SeqCst) - first
        }
    );
    assert_eq!(tcp, MIB);
    assert!(
        during > 0 && sources.lock().unwrap().len() == 2,
        "missing_concurrency_or_source_isolation"
    );
    let after = guest.connection().stats();
    let frames_tx = after.frame_tx.datagram - before.frame_tx.datagram;
    let frames_rx = after.frame_rx.datagram - before.frame_rx.datagram;
    assert!(
        frames_tx >= 9 && frames_rx >= 9,
        "missing_fragmented_wire_frames"
    );
    let metrics = f.host().metrics();
    assert_eq!(
        (
            metrics.destination_tcp_connects,
            metrics.udp_associations_opened,
            metrics.udp_forwarded,
            metrics.udp_returned
        ),
        (1, 2, 4, 4)
    );
    assert!(f.host().invitation_consumed(&f.invitation.lease().lease_id));
    f.udp_addrs.extend(sources.lock().unwrap().iter().copied());
    drop(send);
    drop(recv);
    bridge.shutdown().await;
    drop(bridge);
    drop(a);
    drop(b);
    guest.shutdown().await;
    drop(guest);
    f.cleanup().await;
    eprintln!(
        "{}",
        json!({"case":"fragmented_invited_duplex","tcpBytesEachDirection":MIB,
        "udpBytesBothDirections":15262,"udpOrigins":2,"udpDatagramsEachDirection":4,
        "tcpProgressDuringUdp":during,"quicDatagramFramesTx":frames_tx,"quicDatagramFramesRx":frames_rx,
        "cleanupRebind":true,"internetTested":false})
    );
}

#[tokio::test]
async fn exact_profile_address_key_and_registry_are_checked_before_io() {
    let tcp = TcpListener::bind(local()).await.unwrap();
    let udp = UdpSocket::bind(local()).await.unwrap();
    let reservation = std::net::UdpSocket::bind(local()).unwrap();
    reservation.set_nonblocking(true).unwrap();
    let bind = reservation.local_addr().unwrap();
    let hk = generate_key();
    let gk = generate_key();
    let registry = services(
        tcp.local_addr().unwrap(),
        udp.local_addr().unwrap(),
        UdpReturnPolicy::ReplyBudget,
    );
    let invitation = Invitation::issue_fragmented(
        &hk,
        gk.public(),
        "pre-io",
        bind,
        &registry,
        unix_millis() + 30_000,
    )
    .unwrap();
    let config = || HostConfig {
        secret_key: hk.clone(),
        bind_addr: bind,
        session_id: "pre-io".into(),
        services: registry.clone(),
        leases: vec![invitation.lease()],
        limits: limits(),
    };
    assert!(matches!(
        Host::start_invited(config(), &invitation).await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Host::start_invited_external_ipv6(config(), &invitation).await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Host::start_invited_fragmented_external_ipv6(config(), &invitation).await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Guest::connect_invited(gk.clone(), &invitation, limits()).await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Guest::connect_invited_external_ipv6(gk.clone(), &invitation, limits()).await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Guest::connect_invited_fragmented_external_ipv6(gk.clone(), &invitation, limits()).await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Guest::connect_invited_fragmented(generate_key(), &invitation, limits()).await,
        Err(Error::Unauthorized)
    ));
    for change in 0..9 {
        let mut bad = config();
        match change {
            0 => bad.services[1].revision += 1,
            1 => bad.services[1].required_capability = Capability::Datagram,
            2 => bad.services[1].max_datagram_bytes -= 1,
            3 => {
                bad.services[1].udp_return_policy = UdpReturnPolicy::AuthorizedDownstream {
                    bytes_per_second: 10_000,
                    datagrams_per_second: 20,
                }
            }
            4 => {
                bad.services.pop();
            }
            5 => bad.leases[0].services.pop_first().map(|_| ()).unwrap(),
            6 => bad.leases[0].expires_at_unix_ms -= 1,
            7 => bad.leases[0].peer = generate_key().public(),
            8 => bad.session_id = "different".into(),
            _ => unreachable!(),
        }
        assert!(
            matches!(
                Host::start_invited_fragmented(bad, &invitation).await,
                Err(Error::Configuration)
            ),
            "registry_mismatch_not_rejected_before_bind"
        );
    }
    let mut old = registry.clone();
    old[1].required_capability = Capability::Datagram;
    old[1].max_datagram_bytes = 1264;
    for grants in [&old[..], &old[..1]] {
        let legacy = Invitation::issue(
            &hk,
            gk.public(),
            "pre-io",
            bind,
            grants,
            unix_millis() + 30_000,
        )
        .unwrap();
        let mut cfg = config();
        cfg.services = grants.to_vec();
        cfg.leases = vec![legacy.lease()];
        assert!(matches!(
            Host::start_invited_fragmented(cfg, &legacy).await,
            Err(Error::Configuration)
        ));
        assert!(matches!(
            Guest::connect_invited_fragmented(gk.clone(), &legacy, limits()).await,
            Err(Error::Configuration)
        ));
    }
    let mut bytes = [0; 64];
    assert_eq!(
        reservation.recv_from(&mut bytes).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let (tcp_read, udp_read) = tokio::join!(
        timeout(Duration::from_millis(40), tcp.accept()),
        timeout(Duration::from_millis(40), udp.recv_from(&mut bytes))
    );
    assert!(
        tcp_read.is_err() && udp_read.is_err(),
        "pre_io_guard_touched_destination"
    );
    let tcp_addr = tcp.local_addr().unwrap();
    let udp_addr = udp.local_addr().unwrap();
    drop(tcp);
    drop(udp);
    drop(reservation);
    let _tcp = TcpListener::bind(tcp_addr).await.unwrap();
    let _udp = UdpSocket::bind(udp_addr).await.unwrap();
    let _transport = UdpSocket::bind(bind).await.unwrap();
}

#[tokio::test]
async fn service_revision_v1_downgrade_and_unknown_grants_never_open_destinations() {
    let mut f = Fixture::new(Duration::from_secs(30), UdpReturnPolicy::ReplyBudget).await;
    let g = f.guest().await;
    assert!(matches!(
        g.open_tcp("game", 2).await,
        Err(Error::StaleRevision)
    ));
    assert!(matches!(
        g.open_udp_fragmented("aux", 2).await,
        Err(Error::StaleRevision)
    ));
    assert!(matches!(
        g.open_udp("aux", 1).await,
        Err(Error::CapabilityUnavailable)
    ));
    assert!(matches!(
        g.open_udp_fragmented("other", 1).await,
        Err(Error::ServiceDenied)
    ));
    assert!(matches!(
        g.open_udp_fragmented("game", 1).await,
        Err(Error::Protocol)
    ));
    g.shutdown().await;
    drop(g);
    f.no_destinations().await;
    f.cleanup().await;
}

#[tokio::test]
async fn signed_downstream_policy_and_4097_boundaries_are_enforced() {
    for authorized in [false, true] {
        let policy = if authorized {
            UdpReturnPolicy::AuthorizedDownstream {
                bytes_per_second: 20_000,
                datagrams_per_second: 20,
            }
        } else {
            UdpReturnPolicy::ReplyBudget
        };
        let mut f = Fixture::new(Duration::from_secs(30), policy).await;
        let g = f.guest().await;
        let a = g.open_udp_fragmented("aux", 1).await.unwrap();
        assert_eq!(a.max_payload(), Some(4096));
        let before = g.connection().stats().frame_tx.datagram;
        assert_eq!(a.send(&[0x19; 4097]).await, Err(Error::DatagramTooLarge));
        assert_eq!(g.connection().stats().frame_tx.datagram, before);
        let mut bytes = [0; 4098];
        assert!(
            timeout(Duration::from_millis(40), f.udp.recv_from(&mut bytes))
                .await
                .is_err()
        );
        a.send(b"x").await.unwrap();
        let (count, source) = bounded(f.udp.recv_from(&mut bytes)).await.unwrap();
        assert!(count == 1 && bytes[0] == b'x');
        f.udp_addrs.push(source);
        assert_eq!(f.udp.send_to(&[0x72; 4097], source).await.unwrap(), 4097);
        bounded(async {
            while f.host().metrics().udp_oversize == 0 {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(
            timeout(Duration::from_millis(80), a.recv()).await.is_err(),
            "oversize_prefix_delivered"
        );
        assert_eq!(f.udp.send_to(&[0x31; 2108], source).await.unwrap(), 2108);
        let returned = timeout(Duration::from_millis(150), a.recv()).await;
        if authorized {
            let data = returned.unwrap().unwrap();
            assert!(
                data.as_ref() == [0x31; 2108],
                "authorized_downstream_changed"
            );
        } else {
            assert!(returned.is_err(), "unsigned_downstream_amplification");
        }
        assert_eq!(
            (
                f.host().metrics().udp_forwarded,
                f.host().metrics().udp_returned
            ),
            (1, u64::from(authorized))
        );
        a.shutdown().await;
        drop(a);
        g.shutdown().await;
        drop(g);
        f.cleanup().await;
    }
}

#[tokio::test]
async fn replay_then_revocation_are_distinct_remote_denials_without_new_destinations() {
    let mut f = Fixture::new(Duration::from_secs(30), UdpReturnPolicy::ReplyBudget).await;
    let g = f.guest().await;
    g.shutdown().await;
    drop(g);
    f.idle().await;
    f.verify_current().unwrap();
    assert!(matches!(
        Guest::connect_invited_fragmented(f.guest_key.clone(), &f.invitation, limits()).await,
        Err(Error::Replay)
    ));
    assert_eq!(f.host().metrics().denied, 1);
    f.no_destinations().await;
    assert!(f.host().revoke(&f.invitation.lease().lease_id));
    f.verify_current().unwrap();
    assert!(matches!(
        Guest::connect_invited_fragmented(f.guest_key.clone(), &f.invitation, limits()).await,
        Err(Error::Expired)
    ));
    f.verify_current().unwrap();
    assert_eq!(f.host().metrics().denied, 2);
    f.no_destinations().await;
    f.cleanup().await;
}

#[tokio::test]
async fn active_revocation_and_original_expiry_close_bridges_and_release_all_sockets() {
    for revoke in [true, false] {
        let ttl = if revoke {
            Duration::from_secs(30)
        } else {
            Duration::from_secs(2)
        };
        let mut f = Fixture::new(ttl, UdpReturnPolicy::ReplyBudget).await;
        let g = f.guest().await;
        let tcp = g.tcp_bridge("game", 1, local()).await.unwrap();
        let udp = g.udp_bridge_fragmented("aux", 1, local()).await.unwrap();
        f.tcp_addrs.push(tcp.local_addr());
        f.udp_addrs.push(udp.local_addr());
        let mut tcp_client = bounded(TcpStream::connect(tcp.local_addr())).await.unwrap();
        let (mut tcp_destination, _) = bounded(f.tcp.as_ref().unwrap().accept()).await.unwrap();
        tcp_client.write_all(b"open").await.unwrap();
        let mut tcp_bytes = [0; 4];
        bounded(tcp_destination.read_exact(&mut tcp_bytes))
            .await
            .unwrap();
        assert!(&tcp_bytes == b"open", "active_tcp_payload_changed");
        let origin = UdpSocket::bind(local()).await.unwrap();
        f.udp_addrs.push(origin.local_addr().unwrap());
        origin.send_to(b"live", udp.local_addr()).await.unwrap();
        let mut bytes = [0; 32];
        let (count, source) = bounded(f.udp.recv_from(&mut bytes)).await.unwrap();
        assert!(count == 4 && &bytes[..count] == b"live");
        f.udp_addrs.push(source);
        f.udp.send_to(&bytes[..count], source).await.unwrap();
        bounded(origin.recv_from(&mut bytes)).await.unwrap();
        if revoke {
            assert!(f.host().revoke(&f.invitation.lease().lease_id));
        }
        bounded(g.connection().closed()).await;
        if !revoke {
            assert!(unix_millis() >= f.invitation.expires_at_unix_ms());
            assert_eq!(f.verify_current(), Err(Error::Expired));
        } else {
            f.verify_current().unwrap();
        }
        // Application sockets were still open when the authenticated session
        // ended. Require EOF/reset on both, not only a QUIC closed observation.
        let mut client_byte = [0; 1];
        let (client_end, destination_end) = tokio::join!(
            bounded(tcp_client.read(&mut client_byte)),
            bounded(tcp_destination.read(&mut tcp_bytes)),
        );
        assert!(
            matches!(client_end, Ok(0) | Err(_)),
            "tcp_client_remained_live"
        );
        assert!(
            matches!(destination_end, Ok(0) | Err(_)),
            "tcp_destination_remained_live"
        );
        drop(tcp_client);
        drop(tcp_destination);
        tcp.shutdown().await;
        udp.shutdown().await;
        drop(tcp);
        drop(udp);
        drop(origin);
        g.shutdown().await;
        drop(g);
        f.idle().await;
        f.cleanup().await;
    }
}

async fn send_message(send: &mut SendStream, value: &Value) {
    let bytes = serde_json::to_vec(value).unwrap();
    assert!(bytes.len() <= 4096);
    send.write_u32(bytes.len() as u32).await.unwrap();
    send.write_all(&bytes).await.unwrap();
    send.finish().unwrap();
}
async fn read_reply(recv: &mut RecvStream) -> Value {
    let n = recv.read_u32().await.unwrap() as usize;
    assert!((1..=1024).contains(&n));
    let mut bytes = vec![0; n];
    recv.read_exact(&mut bytes).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
fn early_frame() -> Bytes {
    // Independent 32-byte V2 header with a valid layout, deliberately no admitted association.
    let mut bytes = vec![2, 0];
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.extend_from_slice(&5u16.to_be_bytes());
    bytes.extend_from_slice(&[0, 1]);
    bytes.extend_from_slice(&5u16.to_be_bytes());
    bytes.extend_from_slice(b"early");
    bytes.into()
}

#[tokio::test]
async fn accepted_zero_rtt_rejects_fragmented_replay_proof_and_capability_before_io() {
    let started = Instant::now();
    for (case, expected) in [
        ("replay", "replay"),
        ("proof", "unauthorized"),
        ("capability", "capability_unavailable"),
    ] {
        let mut f = Fixture::new(Duration::from_secs(30), UdpReturnPolicy::ReplyBudget).await;
        let endpoint = f.raw_endpoint().await;
        let warm = if case == "replay" {
            f.hello()
        } else {
            f.wrong_proof()
        };
        bounded(async {
            let connecting = endpoint
                .connect_with_opts(f.host().addr(), ALPN, ConnectOptions::new())
                .await
                .unwrap();
            let connection = match connecting.into_0rtt() {
                Err(c) => c.await.unwrap(),
                Ok(_) => panic!("fresh_endpoint_has_ticket"),
            };
            let (mut send, mut recv) = connection.open_bi().await.unwrap();
            send_message(&mut send, &warm).await;
            let reply = read_reply(&mut recv).await;
            if case == "replay" {
                assert!(reply["error"].is_null());
            } else {
                assert_eq!(reply["error"], "unauthorized");
            }
            connection.close(0u32.into(), b"fixture warm done");
            let _ = connection.closed().await;
        })
        .await;
        f.no_destinations().await;
        let mut hello = if case == "proof" {
            f.wrong_proof()
        } else {
            f.hello()
        };
        if case == "capability" {
            hello["capabilities"] = json!(["reliableStream", "datagram"]);
        }
        bounded(async {
            let connecting = endpoint.connect_with_opts(f.host().addr(), ALPN, ConnectOptions::new()).await.unwrap();
            let early = match connecting.into_0rtt() { Ok(c) => c, Err(_) => panic!("resumption_ticket_missing") };
            let (mut send, mut recv) = early.open_bi().await.unwrap();
            assert!(recv.is_0rtt(), "hello_not_early_data"); send_message(&mut send, &hello).await;
            let mut pending = Vec::new();
            for (id, service, protocol, operation) in [(1,"game","tcp","open"),(2,"aux","udp","openFragmented")] {
                let (mut send, recv) = early.open_bi().await.unwrap();
                send_message(&mut send, &json!({"schemaVersion":1,"requestId":id,"serviceId":service,
                    "serviceRevision":1,"protocol":protocol,"operation":operation,"associationId":0})).await;
                pending.push(recv);
            }
            early.send_datagram(early_frame()).unwrap();
            let connection = match early.handshake_completed().await.unwrap() {
                ZeroRttStatus::Accepted(c) => c,
                ZeroRttStatus::Rejected(c) => { c.close(0u32.into(), b"early data rejected"); panic!("0rtt_rejected_no_evidence"); }
            };
            assert_eq!(read_reply(&mut recv).await["error"], expected);
            let _ = connection.closed().await; drop(pending);
        }).await;
        f.no_destinations().await;
        assert_eq!(
            f.host().metrics().denied,
            if case == "replay" { 1 } else { 2 }
        );
        endpoint.close().await;
        drop(endpoint);
        if case != "replay" {
            assert!(!f.host().invitation_consumed(&f.invitation.lease().lease_id));
            let g = f.guest().await;
            let a = g.open_udp_fragmented("aux", 1).await.unwrap();
            a.send(b"valid").await.unwrap();
            let mut bytes = [0; 8];
            let (count, source) = bounded(f.udp.recv_from(&mut bytes)).await.unwrap();
            assert!(count == 5 && &bytes[..count] == b"valid");
            f.udp_addrs.push(source);
            f.udp.send_to(&bytes[..count], source).await.unwrap();
            assert!(bounded(a.recv()).await.unwrap().as_ref() == b"valid");
            a.shutdown().await;
            drop(a);
            g.shutdown().await;
            drop(g);
        }
        f.cleanup().await;
    }
    eprintln!(
        "{}",
        json!({"case":"fragmented_invited_0rtt","acceptedEarlyDataCases":3,
        "deniedServiceConnections":0,"preTlsIntervalDynamicallyObserved":false,"cleanupRebind":true,
        "elapsedMs":started.elapsed().as_millis()})
    );
}
