//! Public-API observations of per-lease UDP disable on owned loopback sockets.
//! Each case attempts less than 64 KiB of UDP application bytes, both ways.
//! TCP transfers have fixed sizes. No relay, Java, audio or remote notice is tested.
use aurora_connect_lab::*;
use std::{
    collections::BTreeSet,
    future::Future,
    net::SocketAddr,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::Barrier,
    time::{sleep, timeout},
};

const WAIT: Duration = Duration::from_secs(5);
const OBSERVE: Duration = Duration::from_millis(150);

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

fn limits() -> Limits {
    Limits {
        handshake_timeout: WAIT,
        ..Limits::default()
    }
}

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    timeout(WAIT, future)
        .await
        .expect("udp_service_disable_deadline")
}

fn payload(tag: u8, size: usize) -> Vec<u8> {
    (0..size)
        .map(|i| tag.wrapping_add((i % 251) as u8))
        .collect()
}

fn game_service(endpoint: SocketAddr) -> Service {
    Service {
        service_id: "game".into(),
        protocol: Protocol::Tcp,
        endpoint,
        revision: 1,
        required_capability: Capability::ReliableStream,
        max_datagram_bytes: 1400,
        udp_return_policy: UdpReturnPolicy::ReplyBudget,
    }
}

fn udp_service(id: &str, endpoint: SocketAddr, fragmented: bool) -> Service {
    Service {
        service_id: id.into(),
        protocol: Protocol::Udp,
        endpoint,
        revision: 1,
        required_capability: if fragmented {
            Capability::FragmentedDatagram
        } else {
            Capability::Datagram
        },
        max_datagram_bytes: if fragmented { 4096 } else { 1000 },
        udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
            bytes_per_second: 16384,
            datagrams_per_second: 20,
        },
    }
}

// No detached echo workers: the fixture owns its endpoints and each test drives
// destination I/O explicitly. On failure, Host/Guest/bridge Drop cancels their
// existing library workers; on success cleanup awaits them before port checks.
struct Fixture {
    host: Host,
    tcp: TcpListener,
    udp: UdpSocket,
    keys: Vec<SecretKey>,
    invitations: Vec<SessionInvitation>,
    guests: Vec<Guest>,
    tcp_addresses: Mutex<BTreeSet<SocketAddr>>,
    udp_addresses: Mutex<BTreeSet<SocketAddr>>,
}

impl Fixture {
    async fn group(lifetimes: [(u64, u64); 2], host_limits: Limits) -> Self {
        let tcp = TcpListener::bind(local()).await.unwrap();
        let udp = UdpSocket::bind(local()).await.unwrap();
        let services = vec![
            game_service(tcp.local_addr().unwrap()),
            udp_service("aux", udp.local_addr().unwrap(), true),
        ];
        let reserved = std::net::UdpSocket::bind(local()).unwrap();
        let bind_addr = reserved.local_addr().unwrap();
        let host_key = generate_key();
        let keys = vec![generate_key(), generate_key()];
        let invitations: Vec<_> = keys
            .iter()
            .zip(lifetimes)
            .map(|(key, (admission, session))| {
                let issued = SessionInvitation::issue_fragmented_loopback_for_lifetimes(
                    &host_key,
                    key.public(),
                    "udp-service-disable-group",
                    bind_addr,
                    &services,
                    Duration::from_millis(admission),
                    Duration::from_millis(session),
                )
                .unwrap();
                SessionInvitation::verify_fragmented_loopback(
                    &issued.encode(),
                    host_key.public(),
                    key.public(),
                )
                .unwrap()
            })
            .collect();
        let leases = invitations
            .iter()
            .rev()
            .map(SessionInvitation::lease)
            .collect();
        drop(reserved);
        let host = bounded(Host::start_invited_fragmented_session_group(
            HostConfig {
                secret_key: host_key,
                bind_addr,
                session_id: "udp-service-disable-group".into(),
                services,
                leases,
                limits: host_limits,
            },
            &invitations,
        ))
        .await
        .unwrap();
        Self::owned(host, tcp, udp, keys, invitations)
    }

    async fn manual() -> Self {
        let tcp = TcpListener::bind(local()).await.unwrap();
        let udp = UdpSocket::bind(local()).await.unwrap();
        let key = generate_key();
        let host = bounded(Host::start(HostConfig {
            secret_key: generate_key(),
            bind_addr: local(),
            session_id: "udp-service-disable-manual".into(),
            services: vec![
                game_service(tcp.local_addr().unwrap()),
                udp_service("one", udp.local_addr().unwrap(), false),
                udp_service("two", udp.local_addr().unwrap(), false),
                udp_service("ungranted", udp.local_addr().unwrap(), false),
            ],
            leases: vec![Lease {
                lease_id: "p".into(),
                peer: key.public(),
                expires_at_unix_ms: unix_millis() + 30_000,
                services: BTreeSet::from(["game".into(), "one".into(), "two".into()]),
            }],
            limits: limits(),
        }))
        .await
        .unwrap();
        Self::owned(host, tcp, udp, vec![key], vec![])
    }

    fn owned(
        host: Host,
        tcp: TcpListener,
        udp: UdpSocket,
        keys: Vec<SecretKey>,
        invitations: Vec<SessionInvitation>,
    ) -> Self {
        let mut udp_addresses: BTreeSet<_> = host.bound_sockets().into_iter().collect();
        udp_addresses.insert(udp.local_addr().unwrap());
        let tcp_addresses = BTreeSet::from([tcp.local_addr().unwrap()]);
        Self {
            host,
            tcp,
            udp,
            keys,
            invitations,
            guests: vec![],
            tcp_addresses: Mutex::new(tcp_addresses),
            udp_addresses: Mutex::new(udp_addresses),
        }
    }

    async fn guest(&mut self, index: usize) -> Guest {
        let guest = if self.invitations.is_empty() {
            bounded(Guest::connect(GuestConfig {
                secret_key: self.keys[index].clone(),
                bind_addr: local(),
                host: self.host.addr(),
                session_id: "udp-service-disable-manual".into(),
                lease_id: "p".into(),
                capabilities: vec![Capability::ReliableStream, Capability::Datagram],
                limits: limits(),
            }))
            .await
        } else {
            bounded(Guest::connect_invited_fragmented_session(
                self.keys[index].clone(),
                &self.invitations[index],
                limits(),
            ))
            .await
        }
        .unwrap();
        self.udp_addresses
            .lock()
            .unwrap()
            .extend(guest.bound_sockets());
        self.guests.push(guest.clone());
        guest
    }

    async fn tcp_pair(&self, guest: &Guest) -> TcpPair {
        let bridge = bounded(guest.tcp_bridge("game", 1, local())).await.unwrap();
        self.tcp_addresses
            .lock()
            .unwrap()
            .insert(bridge.local_addr());
        let source = bounded(TcpStream::connect(bridge.local_addr()))
            .await
            .unwrap();
        let (destination, _) = bounded(self.tcp.accept()).await.unwrap();
        TcpPair {
            bridge,
            source,
            destination,
        }
    }

    async fn bridge(&self, guest: &Guest) -> UdpBridge {
        let bridge = bounded(guest.udp_bridge_fragmented("aux", 1, local()))
            .await
            .unwrap();
        assert_eq!(bridge.max_payload(), Some(4096));
        self.udp_addresses
            .lock()
            .unwrap()
            .insert(bridge.local_addr());
        bridge
    }

    async fn origin(&self) -> UdpSocket {
        let origin = UdpSocket::bind(local()).await.unwrap();
        self.udp_addresses
            .lock()
            .unwrap()
            .insert(origin.local_addr().unwrap());
        origin
    }

    async fn ingress(&self, expected: &[u8]) -> SocketAddr {
        let mut received = [0; 4097];
        let (size, source) = bounded(self.udp.recv_from(&mut received)).await.unwrap();
        assert_eq!(&received[..size], expected, "destination_scope_or_payload");
        self.udp_addresses.lock().unwrap().insert(source);
        source
    }

    async fn echo_bridge(
        &self,
        origin: &UdpSocket,
        bridge: &UdpBridge,
        tag: u8,
        size: usize,
    ) -> SocketAddr {
        let bytes = payload(tag, size);
        origin.send_to(&bytes, bridge.local_addr()).await.unwrap();
        let source = self.ingress(&bytes).await;
        self.reply_bridge(origin, bridge, source, &bytes).await;
        source
    }

    async fn reply_bridge(
        &self,
        origin: &UdpSocket,
        bridge: &UdpBridge,
        source: SocketAddr,
        bytes: &[u8],
    ) {
        self.udp.send_to(bytes, source).await.unwrap();
        let mut received = [0; 4097];
        let (size, from) = bounded(origin.recv_from(&mut received)).await.unwrap();
        assert_eq!(from, bridge.local_addr());
        assert_eq!(&received[..size], bytes, "cross_return_or_payload");
    }

    async fn echo_v1(&self, association: &UdpAssociation, tag: u8) -> SocketAddr {
        let bytes = payload(tag, 128);
        bounded(association.send(&bytes)).await.unwrap();
        let source = self.ingress(&bytes).await;
        self.udp.send_to(&bytes, source).await.unwrap();
        assert_eq!(bounded(association.recv()).await.unwrap().as_ref(), bytes);
        source
    }

    async fn echo_v2(&self, association: &FragmentedUdpAssociation, tag: u8) -> SocketAddr {
        let bytes = payload(tag, 128);
        bounded(association.send(&bytes)).await.unwrap();
        let source = self.ingress(&bytes).await;
        self.udp.send_to(&bytes, source).await.unwrap();
        assert_eq!(bounded(association.recv()).await.unwrap().as_ref(), bytes);
        source
    }

    async fn active(&self, peers: u64, tcp: u64, udp: u64) {
        bounded(async {
            loop {
                let m = self.host.metrics();
                if (m.active_peers, m.active_tcp, m.active_udp) == (peers, tcp, udp) {
                    break;
                }
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
    }

    async fn cleanup(mut self) {
        for guest in &self.guests {
            bounded(guest.shutdown()).await;
        }
        self.guests.clear();
        bounded(self.host.shutdown()).await;
        let m = self.host.metrics();
        assert_eq!((m.active_peers, m.active_tcp, m.active_udp), (0, 0, 0));
        let tcp = self.tcp_addresses.lock().unwrap().clone();
        let udp = self.udp_addresses.lock().unwrap().clone();
        drop(self);
        let mut tcp_rebound = vec![];
        let mut udp_rebound = vec![];
        for address in tcp {
            tcp_rebound.push(
                TcpListener::bind(address)
                    .await
                    .expect("fixture_tcp_rebind"),
            );
        }
        for address in udp {
            udp_rebound.push(UdpSocket::bind(address).await.expect("fixture_udp_rebind"));
        }
    }
}

struct TcpPair {
    bridge: TcpBridge,
    source: TcpStream,
    destination: TcpStream,
}

impl TcpPair {
    async fn exchange(&mut self, tag: u8, size: usize) {
        bounded(async {
            let bytes = payload(tag, size);
            let mut received = vec![0; size];
            self.source.write_all(&bytes).await.unwrap();
            self.destination.read_exact(&mut received).await.unwrap();
            assert_eq!(received, bytes);
            self.destination.write_all(&bytes).await.unwrap();
            self.source.read_exact(&mut received).await.unwrap();
            assert_eq!(received, bytes);
        })
        .await;
    }

    async fn closed(&mut self) {
        for socket in [&mut self.source, &mut self.destination] {
            assert!(matches!(
                bounded(socket.read(&mut [0])).await,
                Ok(0) | Err(_)
            ));
        }
    }

    async fn shutdown(self) {
        bounded(self.bridge.shutdown()).await;
    }
}

async fn no_udp(socket: &UdpSocket) {
    assert!(
        timeout(OBSERVE, socket.recv_from(&mut [0; 4097]))
            .await
            .is_err(),
        "unexpected_new_udp_payload"
    );
}

// Rebinding an observed association address proves actual Host socket release,
// independently of counters, while the Host and Guest handles remain alive.
async fn rebound_association(address: SocketAddr) -> UdpSocket {
    UdpSocket::bind(address)
        .await
        .expect("disabled_host_association_socket_still_owned")
}

struct TransferGates {
    started: Barrier,
    udp_received: Barrier,
    disabled: Barrier,
    finish: Barrier,
    steps: [AtomicUsize; 2],
    finished: AtomicUsize,
}

impl TransferGates {
    fn new() -> Self {
        Self {
            started: Barrier::new(3),
            udp_received: Barrier::new(3),
            disabled: Barrier::new(3),
            finish: Barrier::new(3),
            steps: [AtomicUsize::new(0), AtomicUsize::new(0)],
            finished: AtomicUsize::new(0),
        }
    }

    async fn progressed(&self, steps: usize) {
        bounded(async {
            while self
                .steps
                .iter()
                .any(|value| value.load(Ordering::SeqCst) < steps)
            {
                sleep(Duration::from_millis(1)).await;
            }
        })
        .await;
    }
}

async fn tcp_during_disable(pair: &mut TcpPair, gates: &TransferGates, index: usize) {
    let tag = 0x60 + index as u8;
    pair.exchange(tag, 256).await;
    gates.steps[index].store(1, Ordering::SeqCst);
    gates.started.wait().await;
    gates.udp_received.wait().await;
    pair.exchange(tag, 4096).await;
    gates.steps[index].store(2, Ordering::SeqCst);
    // Neither fixed-size transfer can finish before disable and its assertions.
    gates.disabled.wait().await;
    pair.exchange(tag + 2, 4096).await;
    gates.steps[index].store(3, Ordering::SeqCst);
    gates.finish.wait().await;
    pair.exchange(tag + 4, 256).await;
    gates.finished.fetch_add(1, Ordering::SeqCst);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invalid_targets_fail_closed_without_disabling_valid_services() {
    let mut f = Fixture::manual().await;
    let too_long = "x".repeat(65);
    for bad in ["", "has.dot", "não", too_long.as_str()] {
        assert_eq!(
            bounded(f.host.disable_udp_service(bad, "one", 1)).await,
            Err(Error::Configuration)
        );
        assert_eq!(
            bounded(f.host.disable_udp_service("p", bad, 1)).await,
            Err(Error::Configuration)
        );
    }
    for (lease, service, revision, error) in [
        ("p", "one", 0, Error::Configuration),
        ("missing", "one", 1, Error::ServiceDenied),
        ("p", "missing", 1, Error::ServiceDenied),
        ("p", "ungranted", 1, Error::ServiceDenied),
        ("p", "game", 1, Error::ServiceDenied),
        ("p", "one", 2, Error::StaleRevision),
    ] {
        assert_eq!(
            bounded(f.host.disable_udp_service(lease, service, revision)).await,
            Err(error)
        );
    }
    let m = f.host.metrics();
    assert_eq!(
        (m.destination_tcp_connects, m.udp_associations_opened),
        (0, 0)
    );
    no_udp(&f.udp).await;
    assert!(timeout(OBSERVE, f.tcp.accept()).await.is_err());

    let guest = f.guest(0).await;
    let one = bounded(guest.open_udp("one", 1)).await.unwrap();
    let two = bounded(guest.open_udp("two", 1)).await.unwrap();
    f.echo_v1(&one, 0x11).await;
    f.echo_v1(&two, 0x12).await;
    let mut tcp = f.tcp_pair(&guest).await;
    tcp.exchange(0x13, 256).await;
    bounded(f.host.disable_udp_service("p", "one", 1))
        .await
        .unwrap();
    bounded(f.host.disable_udp_service("p", "one", 1))
        .await
        .unwrap();
    assert_eq!(
        bounded(f.host.disable_udp_service("p", "one", 2)).await,
        Err(Error::StaleRevision),
        "idempotence_must_not_hide_wrong_revision"
    );
    assert_eq!(
        bounded(f.host.disable_udp_service("missing", "one", 1)).await,
        Err(Error::ServiceDenied)
    );
    f.echo_v1(&two, 0x14).await;
    tcp.exchange(0x15, 256).await;
    bounded(one.shutdown()).await;
    bounded(two.shutdown()).await;
    tcp.shutdown().await;
    drop((one, two, guest));
    f.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn group_disable_isolates_udp_and_releases_sockets_with_tcp_in_progress() {
    let mut f = Fixture::group([(20_000, 30_000); 2], limits()).await;
    let a = f.guest(0).await;
    let b = f.guest(1).await;
    let ua = f.bridge(&a).await;
    let ub = f.bridge(&b).await;
    let origins = [
        f.origin().await,
        f.origin().await,
        f.origin().await,
        f.origin().await,
    ];
    let mut ta = f.tcp_pair(&a).await;
    let mut tb = f.tcp_pair(&b).await;
    let lease_a = f.invitations[0].invitation_id().to_owned();
    let gates = TransferGates::new();
    bounded(async {
        tokio::join!(
            tcp_during_disable(&mut ta, &gates, 0),
            tcp_during_disable(&mut tb, &gates, 1),
            async {
                gates.started.wait().await;
                let first = payload(0x21, 4096);
                origins[0].send_to(&first, ua.local_addr()).await.unwrap();
                let a0 = f.ingress(&first).await;
                gates.udp_received.wait().await;
                gates.progressed(2).await;
                // Both TCP streams advanced while this UDP reply was withheld.
                f.reply_bridge(&origins[0], &ua, a0, &first).await;
                let a1 = f.echo_bridge(&origins[1], &ua, 0x22, 4096).await;
                let b0 = f.echo_bridge(&origins[2], &ub, 0x23, 4096).await;
                let b1 = f.echo_bridge(&origins[3], &ub, 0x24, 4096).await;
                assert_eq!(BTreeSet::from([a0, a1, b0, b1]).len(), 4);
                f.active(2, 2, 4).await;
                assert_eq!(gates.finished.load(Ordering::SeqCst), 0);
                bounded(f.host.disable_udp_service(&lease_a, "aux", 1))
                    .await
                    .unwrap();
                f.active(2, 2, 2).await;
                assert!(a.connection().close_reason().is_none());
                assert!(b.connection().close_reason().is_none());
                assert!(f.host.invitation_active(&lease_a));
                assert_eq!(gates.finished.load(Ordering::SeqCst), 0);

                let rebound_a0 = rebound_association(a0).await;
                let rebound_a1 = rebound_association(a1).await;
                gates.disabled.wait().await;
                gates.progressed(3).await;
                let mut extra_tcp = f.tcp_pair(&a).await;
                extra_tcp.exchange(0x25, 256).await;
                extra_tcp.shutdown().await;
                assert!(matches!(
                    bounded(a.open_udp_fragmented("aux", 1)).await,
                    Err(Error::ServiceDenied)
                ));
                // Every earlier echo has been received. These fresh markers
                // cannot be confused with QUIC packets queued before disable.
                origins[0]
                    .send_to(&payload(0xe1, 128), ua.local_addr())
                    .await
                    .unwrap();
                origins[1]
                    .send_to(&payload(0xe2, 128), ua.local_addr())
                    .await
                    .unwrap();
                no_udp(&f.udp).await;
                for (rebound, address, tag) in [(&rebound_a0, a0, 0xe3), (&rebound_a1, a1, 0xe4)] {
                    let bytes = payload(tag, 64);
                    f.udp.send_to(&bytes, address).await.unwrap();
                    let mut received = [0; 64];
                    let (size, from) = bounded(rebound.recv_from(&mut received)).await.unwrap();
                    assert_eq!(from, f.udp.local_addr().unwrap());
                    assert_eq!(&received[..size], bytes);
                }
                tokio::join!(no_udp(&origins[0]), no_udp(&origins[1]));
                assert_eq!(f.echo_bridge(&origins[2], &ub, 0x26, 128).await, b0);
                assert_eq!(f.echo_bridge(&origins[3], &ub, 0x27, 128).await, b1);
                let extra = bounded(b.open_udp_fragmented("aux", 1)).await.unwrap();
                assert_eq!(extra.max_payload(), Some(4096));
                assert_eq!(
                    bounded(extra.send(&[0; 4097])).await,
                    Err(Error::DatagramTooLarge)
                );
                f.echo_v2(&extra, 0x28).await;
                bounded(extra.shutdown()).await;
                bounded(f.host.disable_udp_service(&lease_a, "aux", 1))
                    .await
                    .unwrap();
                assert_eq!(gates.finished.load(Ordering::SeqCst), 0);
                gates.finish.wait().await;
                drop((extra, rebound_a0, rebound_a1));
            }
        );
    })
    .await;
    assert_eq!(gates.finished.load(Ordering::SeqCst), 2);
    ta.shutdown().await;
    tb.shutdown().await;
    bounded(ua.shutdown()).await;
    bounded(ub.shutdown()).await;
    drop((ua, ub, origins, a, b));
    f.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disable_before_admission_keeps_one_use_game_authority_and_other_guest() {
    let mut f = Fixture::group([(20_000, 30_000); 2], limits()).await;
    let lease_a = f.invitations[0].invitation_id().to_owned();
    let lease_b = f.invitations[1].invitation_id().to_owned();
    let original = f.invitations[0].encode();
    bounded(f.host.disable_udp_service(&lease_a, "aux", 1))
        .await
        .unwrap();
    bounded(f.host.disable_udp_service(&lease_a, "aux", 1))
        .await
        .unwrap();
    assert!(!f.host.invitation_consumed(&lease_a));
    assert!(!f.host.invitation_consumed(&lease_b));
    f.active(0, 0, 0).await;
    assert_eq!(f.host.metrics().udp_associations_opened, 0);
    let a = f.guest(0).await;
    assert!(f.host.invitation_consumed(&lease_a));
    assert_eq!(f.invitations[0].encode(), original);
    let mut tcp = f.tcp_pair(&a).await;
    tcp.exchange(0x31, 256).await;
    assert!(matches!(
        bounded(a.open_udp_fragmented("aux", 1)).await,
        Err(Error::ServiceDenied)
    ));
    assert_eq!(f.host.metrics().udp_associations_opened, 0);
    no_udp(&f.udp).await;
    bounded(a.shutdown()).await;
    tcp.closed().await;
    tcp.shutdown().await;
    f.active(0, 0, 0).await;
    assert!(matches!(
        bounded(Guest::connect_invited_fragmented_session(
            f.keys[0].clone(),
            &f.invitations[0],
            limits()
        ))
        .await,
        Err(Error::Replay)
    ));
    assert!(!f.host.invitation_consumed(&lease_b));
    let b = f.guest(1).await;
    let udp = bounded(b.open_udp_fragmented("aux", 1)).await.unwrap();
    f.echo_v2(&udp, 0x32).await;
    bounded(udp.shutdown()).await;
    drop((udp, a, b));
    f.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manual_v1_disable_is_per_service_and_survives_same_lease_reconnect() {
    let mut f = Fixture::manual().await;
    let guest = f.guest(0).await;
    let one = bounded(guest.open_udp("one", 1)).await.unwrap();
    let two = bounded(guest.open_udp("two", 1)).await.unwrap();
    let one_source = f.echo_v1(&one, 0x41).await;
    let two_source = f.echo_v1(&two, 0x42).await;
    assert_ne!(one_source, two_source);
    let mut tcp = f.tcp_pair(&guest).await;
    tcp.exchange(0x43, 256).await;
    bounded(f.host.disable_udp_service("p", "one", 1))
        .await
        .unwrap();
    let rebound = rebound_association(one_source).await;
    f.active(1, 1, 1).await;
    assert!(guest.connection().close_reason().is_none());
    tcp.exchange(0x44, 256).await;
    // Local enqueue is not a remote notice or proof of forwarding.
    let _ = bounded(one.send(&payload(0xf1, 128))).await;
    no_udp(&f.udp).await;
    let fresh_reply = payload(0xf2, 64);
    f.udp.send_to(&fresh_reply, one_source).await.unwrap();
    let mut received = [0; 64];
    let (size, _) = bounded(rebound.recv_from(&mut received)).await.unwrap();
    assert_eq!(&received[..size], fresh_reply);
    assert!(!matches!(timeout(OBSERVE, one.recv()).await, Ok(Ok(_))));
    assert_eq!(f.echo_v1(&two, 0x45).await, two_source);
    assert!(matches!(
        bounded(guest.open_udp("one", 1)).await,
        Err(Error::ServiceDenied)
    ));
    let extra = bounded(guest.open_udp("two", 1)).await.unwrap();
    f.echo_v1(&extra, 0x46).await;
    bounded(extra.shutdown()).await;
    bounded(one.shutdown()).await;
    bounded(two.shutdown()).await;
    bounded(guest.shutdown()).await;
    tcp.closed().await;
    tcp.shutdown().await;
    f.active(0, 0, 0).await;
    drop((one, two, extra, guest));

    let replacement = f.guest(0).await;
    assert!(matches!(
        bounded(replacement.open_udp("one", 1)).await,
        Err(Error::ServiceDenied)
    ));
    let live = bounded(replacement.open_udp("two", 1)).await.unwrap();
    f.echo_v1(&live, 0x47).await;
    let mut tcp = f.tcp_pair(&replacement).await;
    tcp.exchange(0x48, 256).await;
    bounded(f.host.disable_udp_service("p", "one", 1))
        .await
        .unwrap();
    bounded(live.shutdown()).await;
    tcp.shutdown().await;
    drop((live, replacement, rebound));
    f.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disable_recovers_one_capacity_slot_without_expanding_global_limit() {
    let host_limits = Limits {
        // B's third open must hit the global cap, not a matching peer cap.
        max_udp_per_peer: 3,
        max_udp_total: 2,
        ..limits()
    };
    let mut f = Fixture::group([(20_000, 30_000); 2], host_limits).await;
    let a = f.guest(0).await;
    let b = f.guest(1).await;
    let ua = bounded(a.open_udp_fragmented("aux", 1)).await.unwrap();
    let ub = bounded(b.open_udp_fragmented("aux", 1)).await.unwrap();
    let a_source = f.echo_v2(&ua, 0x51).await;
    let b_source = f.echo_v2(&ub, 0x52).await;
    f.active(2, 0, 2).await;
    assert!(matches!(
        bounded(b.open_udp_fragmented("aux", 1)).await,
        Err(Error::Limit)
    ));
    bounded(
        f.host
            .disable_udp_service(f.invitations[0].invitation_id(), "aux", 1),
    )
    .await
    .unwrap();
    let rebound = rebound_association(a_source).await;
    f.active(2, 0, 1).await;
    let extra = bounded(b.open_udp_fragmented("aux", 1)).await.unwrap();
    f.echo_v2(&extra, 0x53).await;
    assert_eq!(f.echo_v2(&ub, 0x54).await, b_source);
    f.active(2, 0, 2).await;
    assert!(matches!(
        bounded(b.open_udp_fragmented("aux", 1)).await,
        Err(Error::Limit)
    ));
    assert!(matches!(
        bounded(a.open_udp_fragmented("aux", 1)).await,
        Err(Error::ServiceDenied)
    ));
    bounded(ua.shutdown()).await;
    bounded(ub.shutdown()).await;
    bounded(extra.shutdown()).await;
    drop((ua, ub, extra, a, b, rebound));
    f.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disable_preserves_original_deadlines_revoke_and_terminal_idempotence() {
    let mut f = Fixture::group([(4000, 6000), (20_000, 30_000)], limits()).await;
    let a = f.guest(0).await;
    let b = f.guest(1).await;
    let lease_a = f.invitations[0].invitation_id().to_owned();
    let lease_b = f.invitations[1].invitation_id().to_owned();
    let deadline = f.invitations[0].session_expires_at_unix_ms();
    let mut ta = f.tcp_pair(&a).await;
    let mut tb = f.tcp_pair(&b).await;
    let ua = bounded(a.open_udp_fragmented("aux", 1)).await.unwrap();
    let ub = bounded(b.open_udp_fragmented("aux", 1)).await.unwrap();
    let a_source = f.echo_v2(&ua, 0x71).await;
    f.echo_v2(&ub, 0x72).await;
    bounded(f.host.disable_udp_service(&lease_a, "aux", 1))
        .await
        .unwrap();
    let rebound = rebound_association(a_source).await;
    ta.exchange(0x73, 256).await;
    tb.exchange(0x74, 256).await;
    assert!(a.connection().close_reason().is_none());
    assert!(f.host.invitation_active(&lease_a));
    sleep(Duration::from_millis(
        deadline.saturating_sub(unix_millis()) + 50,
    ))
    .await;
    ta.closed().await;
    bounded(a.connection().closed()).await;
    bounded(f.host.disable_udp_service(&lease_a, "aux", 1))
        .await
        .unwrap();
    assert_eq!(f.invitations[0].session_expires_at_unix_ms(), deadline);
    tb.exchange(0x75, 256).await;
    f.echo_v2(&ub, 0x76).await;
    assert!(f.host.revoke(&lease_b));
    tb.closed().await;
    bounded(b.connection().closed()).await;
    bounded(f.host.disable_udp_service(&lease_b, "aux", 1))
        .await
        .unwrap();
    f.active(0, 0, 0).await;
    bounded(f.host.shutdown()).await;
    for lease in [&lease_a, &lease_b] {
        bounded(f.host.disable_udp_service(lease, "aux", 1))
            .await
            .unwrap();
        assert_eq!(
            bounded(f.host.disable_udp_service(lease, "aux", 2)).await,
            Err(Error::StaleRevision)
        );
    }
    bounded(ua.shutdown()).await;
    bounded(ub.shutdown()).await;
    ta.shutdown().await;
    tb.shutdown().await;
    drop((ua, ub, a, b, rebound));
    f.cleanup().await;
}
