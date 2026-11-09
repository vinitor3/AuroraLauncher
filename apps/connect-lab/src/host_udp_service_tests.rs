//! Causal private gates around the real Host/Guest transport. No external
//! endpoint, process clock, Java or voice payload is used by these fixtures.
use super::*;
use std::{collections::BTreeSet, future::Future};
use tokio::{net::TcpListener, time::timeout};
use udp_service::{Gate, SendGate, Stage};

const WAIT: Duration = Duration::from_secs(5);
const PENDING: Duration = Duration::from_millis(40);

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    timeout(WAIT, future)
        .await
        .expect("udp_service_private_deadline")
}

async fn until(mut predicate: impl FnMut() -> bool) {
    bounded(async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await;
}

struct Fixture {
    host: Arc<Host>,
    guest: Guest,
    tcp: TcpListener,
    destination: UdpSocket,
    control: Arc<udp_service::Control>,
    addresses: BTreeSet<SocketAddr>,
}

impl Fixture {
    async fn new() -> Self {
        let tcp = TcpListener::bind(local()).await.unwrap();
        let destination = UdpSocket::bind(local()).await.unwrap();
        let key = generate_key();
        let limits = Limits {
            handshake_timeout: WAIT,
            ..Limits::default()
        };
        let services = vec![
            Service {
                service_id: "game".into(),
                protocol: Protocol::Tcp,
                endpoint: tcp.local_addr().unwrap(),
                revision: 1,
                max_datagram_bytes: 1400,
                required_capability: Capability::ReliableStream,
                udp_return_policy: UdpReturnPolicy::ReplyBudget,
            },
            Service {
                service_id: "aux".into(),
                protocol: Protocol::Udp,
                endpoint: destination.local_addr().unwrap(),
                revision: 1,
                max_datagram_bytes: 4096,
                required_capability: Capability::FragmentedDatagram,
                udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                    bytes_per_second: 16384,
                    datagrams_per_second: 20,
                },
            },
            Service {
                service_id: "legacy".into(),
                protocol: Protocol::Udp,
                endpoint: destination.local_addr().unwrap(),
                revision: 1,
                max_datagram_bytes: 1000,
                required_capability: Capability::Datagram,
                udp_return_policy: UdpReturnPolicy::ReplyBudget,
            },
        ];
        let host = Arc::new(
            bounded(Host::start(HostConfig {
                secret_key: generate_key(),
                bind_addr: local(),
                session_id: "udp-private".into(),
                services,
                leases: vec![Lease {
                    lease_id: "p".into(),
                    peer: key.public(),
                    expires_at_unix_ms: unix_millis() + 30_000,
                    services: BTreeSet::from(["game".into(), "aux".into(), "legacy".into()]),
                }],
                limits: limits.clone(),
            }))
            .await
            .unwrap(),
        );
        let guest = bounded(Guest::connect(GuestConfig {
            secret_key: key,
            bind_addr: local(),
            host: host.addr(),
            session_id: "udp-private".into(),
            lease_id: "p".into(),
            capabilities: vec![
                Capability::ReliableStream,
                Capability::Datagram,
                Capability::FragmentedDatagram,
            ],
            limits,
        }))
        .await
        .unwrap();
        let handle = host.state.services["aux"].0;
        let control = host.state.lease("p").unwrap().udp_services[&handle].clone();
        let mut addresses: BTreeSet<_> = host.bound_sockets().into_iter().collect();
        addresses.extend(guest.bound_sockets());
        addresses.insert(destination.local_addr().unwrap());
        Self {
            host,
            guest,
            tcp,
            destination,
            control,
            addresses,
        }
    }

    fn disable(&self) -> JoinHandle<Result<()>> {
        let host = self.host.clone();
        tokio::spawn(async move { host.disable_udp_service("p", "aux", 1).await })
    }

    fn control_for(&self, service: &str) -> Arc<udp_service::Control> {
        let handle = self.host.state.services[service].0;
        self.host.state.lease("p").unwrap().udp_services[&handle].clone()
    }

    async fn assert_game_alive(&self) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut send, mut recv) = bounded(self.guest.open_tcp("game", 1)).await.unwrap();
        let (mut socket, _) = bounded(self.tcp.accept()).await.unwrap();
        send.write_all(b"after-disable").await.unwrap();
        let mut bytes = [0; 13];
        bounded(socket.read_exact(&mut bytes)).await.unwrap();
        assert_eq!(&bytes, b"after-disable");
        socket.write_all(b"game-alive").await.unwrap();
        let mut response = [0; 10];
        bounded(recv.read_exact(&mut response)).await.unwrap();
        assert_eq!(&response, b"game-alive");
        send.finish().unwrap();
    }

    async fn finish(self) {
        bounded(self.guest.shutdown()).await;
        bounded(self.host.shutdown()).await;
        let tcp_address = self.tcp.local_addr().unwrap();
        let addresses = self.addresses;
        drop((
            self.guest,
            self.host,
            self.tcp,
            self.destination,
            self.control,
        ));
        assert!(
            std::net::TcpListener::bind(tcp_address).is_ok(),
            "tcp_fixture_released"
        );
        for address in addresses {
            assert!(
                std::net::UdpSocket::bind(address).is_ok(),
                "udp_fixture_released"
            );
        }
    }
}

fn assert_socket_held(address: SocketAddr) {
    assert!(
        std::net::UdpSocket::bind(address).is_err(),
        "retained_socket_must_be_owned"
    );
}

fn assert_socket_released(address: SocketAddr) {
    assert!(
        std::net::UdpSocket::bind(address).is_ok(),
        "retired_socket_must_rebind_with_host_alive"
    );
}

async fn setup_phase(stage: Stage) {
    let fixture = Fixture::new().await;
    let gate = Gate::new();
    fixture.control.install(stage, gate.clone());
    let permits = fixture.host.state.udp.available_permits();
    let guest = fixture.guest.clone();
    let opening = tokio::spawn(async move { guest.open_udp_fragmented("aux", 1).await });
    bounded(gate.entered()).await;
    assert_eq!(fixture.control.work_count(), 1);
    assert_eq!(fixture.host.state.udp.available_permits(), permits - 1);
    assert_eq!(
        fixture
            .host
            .state
            .metrics
            .udp_associations_opened
            .load(Ordering::Relaxed),
        0
    );
    if let Some(address) = gate.socket() {
        assert_socket_held(address);
    }

    let mut first = fixture.disable();
    bounded(fixture.control.token().cancelled()).await;
    assert!(
        timeout(PENDING, &mut first).await.is_err(),
        "setup_owner_must_prevent_success"
    );
    first.abort();
    assert!(bounded(first).await.unwrap_err().is_cancelled());
    // Losing the caller cannot lose the service's same registered work.
    let mut second = fixture.disable();
    assert!(
        timeout(PENDING, &mut second).await.is_err(),
        "second_waiter_must_track_original_setup"
    );
    assert_eq!(fixture.control.work_count(), 1);
    assert!(matches!(fixture.control.begin(), Err(Error::ServiceDenied)));
    if let Some(address) = gate.socket() {
        assert_socket_held(address);
    }

    gate.open();
    bounded(second).await.unwrap().unwrap();
    assert_eq!(fixture.control.work_count(), 0);
    assert_eq!(fixture.host.state.udp.available_permits(), permits);
    if let Some(address) = gate.socket() {
        assert_socket_released(address);
    }
    let opened = bounded(opening).await.unwrap();
    if stage == Stage::BeforeReturn {
        // ACK won before the seal; it cannot retain a Host association or
        // authorize a subsequent datagram, even though the Guest knows the id.
        let remote = opened.unwrap();
        remote.send(b"sealed-after-ack").await.unwrap();
        let mut data = [0; 64];
        assert!(
            timeout(PENDING, fixture.destination.recv(&mut data))
                .await
                .is_err()
        );
        bounded(remote.shutdown()).await;
    } else {
        assert!(opened.is_err(), "no_late_setup_publication");
        assert_eq!(
            fixture
                .host
                .state
                .metrics
                .udp_associations_opened
                .load(Ordering::Relaxed),
            0
        );
    }
    assert!(matches!(
        bounded(fixture.guest.open_udp_fragmented("aux", 1)).await,
        Err(Error::ServiceDenied)
    ));
    assert_eq!(fixture.control.work_count(), 0);
    bounded(fixture.host.disable_udp_service("p", "aux", 1))
        .await
        .unwrap();
    fixture.assert_game_alive().await;
    fixture.finish().await;
}

macro_rules! setup_test {
    ($name:ident, $stage:ident) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn $name() {
            setup_phase(Stage::$stage).await;
        }
    };
}
setup_test!(
    disable_before_bind_owns_setup_across_lost_waiter,
    BeforeBind
);
setup_test!(disable_after_bind_waits_for_exact_socket, AfterBind);
setup_test!(
    disable_after_connect_waits_without_publication,
    AfterConnect
);
setup_test!(
    disable_before_publish_prevents_late_association,
    BeforePublish
);
setup_test!(
    disable_before_ack_cancels_published_association,
    AfterPublish
);
setup_test!(
    disable_before_return_installation_covers_acknowledged_association,
    BeforeReturn
);

async fn fragmented_delivery(stage: Stage) {
    let fixture = Fixture::new().await;
    let association = bounded(fixture.guest.open_udp_fragmented("aux", 1))
        .await
        .unwrap();
    let gate = Gate::new();
    fixture.control.install(stage, gate.clone());
    let wait_gate = Gate::new();
    wait_gate.open();
    fixture
        .control
        .install(Stage::BeforeWait, wait_gate.clone());
    let peer_memory = fixture
        .host
        .state
        .lease("p")
        .unwrap()
        .fragment_memory
        .clone();
    let baseline = peer_memory.used_bytes();
    let payload = vec![0x61; 2048];
    if stage == Stage::Pending {
        let frame = fragment::Fragments::new(
            fragment::Scope {
                service_handle: association.service_handle(),
                association_id: association.association_id(),
                generation: association.generation(),
            },
            1,
            &payload,
            bridge::MAX_FRAGMENT_FRAME_BYTES,
        )
        .unwrap()
        .next()
        .unwrap();
        fixture
            .guest
            .connection()
            .send_datagram(frame.encode().unwrap().into())
            .unwrap();
    } else {
        association.send(&payload).await.unwrap();
    }
    bounded(gate.entered()).await;
    let address = gate.socket().unwrap();
    let weak = fixture
        .control
        .published(association.association_id())
        .unwrap();
    assert_socket_held(address);
    assert_eq!(peer_memory.used_bytes(), baseline + payload.len());
    {
        let real = weak.upgrade().unwrap();
        let reassembler = real.fragmented.as_ref().unwrap().lock().unwrap();
        assert_eq!(reassembler.reserved_bytes(), payload.len());
        assert_eq!(reassembler.reserved_messages(), 1);
        assert_eq!(
            reassembler.pending_messages(),
            usize::from(stage == Stage::Pending)
        );
    }
    let mut disabled = fixture.disable();
    bounded(wait_gate.entered()).await;
    // The wait hook is after cancelling/clearing the published reassembler.
    // A Pending reservation is gone; Completed still owns its real bytes.
    assert_eq!(
        peer_memory.used_bytes(),
        baseline
            + if stage == Stage::Completed {
                payload.len()
            } else {
                0
            }
    );
    assert!(timeout(PENDING, &mut disabled).await.is_err());
    assert_socket_held(address);
    assert!(weak.upgrade().is_some());
    gate.open();
    bounded(disabled).await.unwrap().unwrap();
    assert_eq!(peer_memory.used_bytes(), baseline);
    assert!(weak.upgrade().is_none());
    assert_socket_released(address);
    let mut data = [0; 4096];
    assert!(
        timeout(PENDING, fixture.destination.recv(&mut data))
            .await
            .is_err(),
        "cancelled_payload_cannot_reach_destination"
    );
    // The Pending path injected message 1 directly, without advancing the
    // Guest's counter. Use a new id explicitly so replay cannot explain this
    // negative observation.
    let fresh = fragment::Fragments::new(
        fragment::Scope {
            service_handle: association.service_handle(),
            association_id: association.association_id(),
            generation: association.generation(),
        },
        2,
        b"new-after-disable",
        bridge::MAX_FRAGMENT_FRAME_BYTES,
    )
    .unwrap()
    .next()
    .unwrap();
    fixture
        .guest
        .connection()
        .send_datagram(fresh.encode().unwrap().into())
        .unwrap();
    assert!(
        timeout(PENDING, fixture.destination.recv(&mut data))
            .await
            .is_err()
    );
    assert_eq!(peer_memory.used_bytes(), baseline);
    bounded(association.shutdown()).await;
    drop(association);
    fixture.assert_game_alive().await;
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_fragment_clear_frees_reservation_but_waits_for_real_receiver() {
    fragmented_delivery(Stage::Pending).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn completed_fragment_keeps_reservation_until_delivery_owner_returns() {
    fragmented_delivery(Stage::Completed).await;
}

async fn drive_to_return(fixture: &Fixture) -> (FragmentedUdpAssociation, Arc<Gate>, SocketAddr) {
    let association = bounded(fixture.guest.open_udp_fragmented("aux", 1))
        .await
        .unwrap();
    association.send(b"return-owner-up").await.unwrap();
    let mut data = [0; 64];
    let (count, address) = bounded(fixture.destination.recv_from(&mut data))
        .await
        .unwrap();
    assert_eq!(&data[..count], b"return-owner-up");
    let gate = Gate::new();
    fixture.control.install(Stage::Return, gate.clone());
    fixture
        .destination
        .send_to(b"return-owner-down", address)
        .await
        .unwrap();
    bounded(gate.entered()).await;
    assert_eq!(gate.socket(), Some(address));
    (association, gate, address)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn return_owner_keeps_socket_and_permit_after_map_retirement() {
    let fixture = Fixture::new().await;
    let permits = fixture.host.state.udp.available_permits();
    let (association, gate, address) = drive_to_return(&fixture).await;
    let mut first = fixture.disable();
    bounded(fixture.control.token().cancelled()).await;
    assert!(timeout(PENDING, &mut first).await.is_err());
    first.abort();
    assert!(bounded(first).await.unwrap_err().is_cancelled());
    let mut second = fixture.disable();
    assert!(timeout(PENDING, &mut second).await.is_err());
    assert_eq!(fixture.host.state.udp.available_permits(), permits - 1);
    assert_socket_held(address);
    gate.open();
    bounded(second).await.unwrap().unwrap();
    assert_eq!(fixture.host.state.udp.available_permits(), permits);
    assert_socket_released(address);
    assert!(
        timeout(PENDING, association.recv()).await.is_err(),
        "held_return_must_not_enqueue_after_seal"
    );
    bounded(association.shutdown()).await;
    drop(association);
    fixture.assert_game_alive().await;
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn final_owner_notify_is_not_lost_between_empty_check_and_await() {
    let fixture = Fixture::new().await;
    let (association, return_gate, address) = drive_to_return(&fixture).await;
    let wait_gate = Gate::new();
    fixture
        .control
        .install(Stage::BeforeWait, wait_gate.clone());
    let mut disabled = fixture.disable();
    bounded(wait_gate.entered()).await;
    assert_eq!(fixture.control.work_count(), 1);
    return_gate.open();
    until(|| fixture.control.work_count() == 0).await;
    assert_socket_released(address);
    assert!(timeout(PENDING, &mut disabled).await.is_err());
    // The last Work notified while disable was between its nonempty check
    // and await. Without enable-before-check this is a lost wakeup.
    wait_gate.open();
    bounded(disabled).await.unwrap().unwrap();
    bounded(association.shutdown()).await;
    drop(association);
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn incomplete_control_read_cannot_retain_active_udp_socket() {
    let fixture = Fixture::new().await;
    let association = bounded(fixture.guest.open_udp_fragmented("aux", 1))
        .await
        .unwrap();
    association.send(b"control-stall-up").await.unwrap();
    let mut data = [0; 64];
    let (count, address) = bounded(fixture.destination.recv_from(&mut data))
        .await
        .unwrap();
    assert_eq!(&data[..count], b"control-stall-up");
    fixture
        .destination
        .send_to(b"control-stall-down", address)
        .await
        .unwrap();
    assert_eq!(
        bounded(association.recv()).await.unwrap().as_ref(),
        b"control-stall-down"
    );
    let reads = fixture
        .host
        .state
        .control_reads_started
        .load(Ordering::SeqCst);
    let (mut stalled, receiver) = fixture.guest.connection().open_bi().await.unwrap();
    stalled.write_all(&[0, 0]).await.unwrap();
    until(|| {
        fixture
            .host
            .state
            .control_reads_started
            .load(Ordering::SeqCst)
            > reads
    })
    .await;
    // The Host is known to be reading an incomplete length prefix. Its read
    // timeout is five seconds; service cleanup must finish inside this bound.
    timeout(
        Duration::from_secs(1),
        fixture.host.disable_udp_service("p", "aux", 1),
    )
    .await
    .unwrap()
    .unwrap();
    assert_socket_released(address);
    assert!(fixture.guest.connection().close_reason().is_none());
    assert_eq!(fixture.control.work_count(), 0);
    stalled.reset(0u32.into()).unwrap();
    drop((stalled, receiver));
    bounded(association.shutdown()).await;
    drop(association);
    fixture.assert_game_alive().await;
    fixture.finish().await;
}

#[derive(Clone)]
enum Remote {
    V1(UdpAssociation),
    V2(FragmentedUdpAssociation),
}
impl Remote {
    async fn open(guest: &Guest, fragmented: bool) -> Self {
        if fragmented {
            Self::V2(bounded(guest.open_udp_fragmented("aux", 1)).await.unwrap())
        } else {
            Self::V1(bounded(guest.open_udp("legacy", 1)).await.unwrap())
        }
    }
    fn id(&self) -> u64 {
        match self {
            Self::V1(a) => a.association_id(),
            Self::V2(a) => a.association_id(),
        }
    }
    async fn send(&self, bytes: &[u8]) -> Result<()> {
        match self {
            Self::V1(a) => a.send(bytes).await,
            Self::V2(a) => a.send(bytes).await,
        }
    }
    async fn recv(&self) -> Result<Vec<u8>> {
        match self {
            Self::V1(a) => a.recv().await.map(|bytes| bytes.to_vec()),
            Self::V2(a) => a.recv().await.map(|bytes| bytes.as_ref().to_vec()),
        }
    }
    async fn shutdown(&self) {
        match self {
            Self::V1(a) => a.shutdown().await,
            Self::V2(a) => a.shutdown().await,
        }
    }
}

async fn final_send_gate(fragmented: bool, downstream: bool) {
    let fixture = Fixture::new().await;
    let service = if fragmented { "aux" } else { "legacy" };
    let control = fixture.control_for(service);
    let remote = Remote::open(&fixture.guest, fragmented).await;
    let address = control
        .published(remote.id())
        .unwrap()
        .upgrade()
        .unwrap()
        .socket()
        .local_addr()
        .unwrap();
    let mut data = [0; 64];
    if downstream {
        remote
            .send(b"credit-before-return-with-reserve")
            .await
            .unwrap();
        let (count, source) = bounded(fixture.destination.recv_from(&mut data))
            .await
            .unwrap();
        assert_eq!(source, address);
        assert_eq!(&data[..count], b"credit-before-return-with-reserve");
    }
    let gate = SendGate::new();
    control.install_send(gate.clone());
    let marker = b"authorized-before-seal";
    let receiving = if downstream {
        let receiver = remote.clone();
        Some(tokio::spawn(async move { receiver.recv().await }))
    } else {
        None
    };
    if downstream {
        fixture.destination.send_to(marker, address).await.unwrap();
    } else {
        remote.send(marker).await.unwrap();
    }
    bounded(gate.entered()).await;
    let host = fixture.host.clone();
    let mut disabled = tokio::spawn(async move { host.disable_udp_service("p", service, 1).await });
    bounded(control.seal_attempted()).await;
    // This is a real cross-thread dispute: final IO owns the authorization
    // mutex, so disable cannot yet seal/cancel it. No registry read is made
    // here because it would try to lock the gate that the fixture retains.
    assert!(control.enabled());
    assert!(timeout(PENDING, &mut disabled).await.is_err());
    assert_socket_held(address);
    gate.open();
    bounded(disabled).await.unwrap().unwrap();
    assert!(!control.enabled());
    assert_socket_released(address);
    if let Some(receiving) = receiving {
        assert_eq!(bounded(receiving).await.unwrap().unwrap(), marker);
    } else {
        let (count, source) = bounded(fixture.destination.recv_from(&mut data))
            .await
            .unwrap();
        assert_eq!(source, address);
        assert_eq!(&data[..count], marker);
    }
    // The prior authorized packet is drained above. Only this new marker is
    // the post-seal negative, on the still-open authenticated connection.
    remote.send(b"new-after-seal").await.unwrap();
    assert!(
        timeout(PENDING, fixture.destination.recv_from(&mut data))
            .await
            .is_err()
    );
    let mut called = false;
    assert!(matches!(
        control.send_immediately(|| {
            called = true;
            Ok(())
        }),
        Err(Error::ServiceDenied)
    ));
    assert!(!called);
    bounded(remote.shutdown()).await;
    drop(remote);
    fixture.assert_game_alive().await;
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn v1_destination_send_and_disable_share_the_final_gate() {
    final_send_gate(false, false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn v2_destination_send_and_disable_share_the_final_gate() {
    final_send_gate(true, false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn v1_return_enqueue_and_disable_share_the_final_gate() {
    final_send_gate(false, true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn v2_return_enqueue_and_disable_share_the_final_gate() {
    final_send_gate(true, true).await;
}

fn poison_gate(control: &udp_service::Control) {
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = control.immediately::<()>(|| panic!("fixture_authorization_gate_poison"));
    }));
    assert!(panic.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn poisoned_begin_fails_closed_and_cleanup_releases_real_socket() {
    let fixture = Fixture::new().await;
    let (association, gate, address) = drive_to_return(&fixture).await;
    poison_gate(&fixture.control);
    assert!(matches!(fixture.control.begin(), Err(Error::ServiceDenied)));
    assert!(!fixture.control.enabled());
    let mut disabled = fixture.disable();
    assert!(timeout(PENDING, &mut disabled).await.is_err());
    assert_socket_held(address);
    gate.open();
    bounded(disabled).await.unwrap().unwrap();
    assert_socket_released(address);
    bounded(association.shutdown()).await;
    drop(association);
    fixture.assert_game_alive().await;
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn poisoned_final_send_fails_closed_before_real_destination_io() {
    let fixture = Fixture::new().await;
    let remote = Remote::open(&fixture.guest, false).await;
    let control = fixture.control_for("legacy");
    let address = control
        .published(remote.id())
        .unwrap()
        .upgrade()
        .unwrap()
        .socket()
        .local_addr()
        .unwrap();
    poison_gate(&control);
    remote.send(b"poisoned-final-send").await.unwrap();
    bounded(control.token().cancelled()).await;
    bounded(fixture.host.disable_udp_service("p", "legacy", 1))
        .await
        .unwrap();
    assert_socket_released(address);
    let mut data = [0; 64];
    assert!(
        timeout(PENDING, fixture.destination.recv(&mut data))
            .await
            .is_err()
    );
    assert!(matches!(control.begin(), Err(Error::ServiceDenied)));
    bounded(remote.shutdown()).await;
    drop(remote);
    fixture.assert_game_alive().await;
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn poisoned_publication_fails_closed_without_late_association() {
    let fixture = Fixture::new().await;
    let gate = Gate::new();
    fixture.control.install(Stage::BeforePublish, gate.clone());
    let guest = fixture.guest.clone();
    let opening = tokio::spawn(async move { guest.open_udp_fragmented("aux", 1).await });
    bounded(gate.entered()).await;
    let address = gate.socket().unwrap();
    poison_gate(&fixture.control);
    gate.open();
    assert!(matches!(
        bounded(opening).await.unwrap(),
        Err(Error::ServiceDenied)
    ));
    assert!(!fixture.control.enabled());
    bounded(fixture.host.disable_udp_service("p", "aux", 1))
        .await
        .unwrap();
    assert_socket_released(address);
    assert_eq!(
        fixture
            .host
            .state
            .metrics
            .udp_associations_opened
            .load(Ordering::Relaxed),
        0
    );
    fixture.assert_game_alive().await;
    fixture.finish().await;
}

type BucketSnapshot = (f64, f64, f64, Instant);
fn budgets(fixture: &Fixture) -> [BucketSnapshot; 7] {
    let lease = fixture.host.state.lease("p").unwrap();
    let handle = fixture.host.state.services["aux"].0;
    let ingress = lease.udp_limiters.lock().unwrap()[&handle].clone();
    let downstream = lease.downstream_limiters.lock().unwrap()[&handle].clone();
    let ingress = ingress.lock().unwrap();
    let downstream = downstream.lock().unwrap();
    let fragments = lease.fragment_ingress.lock().unwrap();
    [
        ingress.packets.test_snapshot(),
        ingress.bytes.test_snapshot(),
        downstream.packets.test_snapshot(),
        downstream.bytes.test_snapshot(),
        fragments.packets.test_snapshot(),
        fragments.bytes.test_snapshot(),
        fixture.host.state.budget.lock().unwrap().test_snapshot(),
    ]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disable_repeat_and_disconnect_preserve_debited_buckets_and_credit() {
    let fixture = Fixture::new().await;
    let permits = fixture.host.state.udp.available_permits();
    let association = bounded(fixture.guest.open_udp_fragmented("aux", 1))
        .await
        .unwrap();
    association.send(b"budget-debit-up").await.unwrap();
    let mut data = [0; 64];
    let (count, address) = bounded(fixture.destination.recv_from(&mut data))
        .await
        .unwrap();
    assert_eq!(&data[..count], b"budget-debit-up");
    fixture
        .destination
        .send_to(b"budget-debit-down", address)
        .await
        .unwrap();
    assert_eq!(
        bounded(association.recv()).await.unwrap().as_ref(),
        b"budget-debit-down"
    );
    let gate = Gate::new();
    fixture.control.install(Stage::Return, gate.clone());
    fixture
        .destination
        .send_to(b"held-without-new-debit", address)
        .await
        .unwrap();
    bounded(gate.entered()).await;
    let weak = fixture
        .control
        .published(association.association_id())
        .unwrap();
    let credit = weak.upgrade().unwrap().credit.load(Ordering::Relaxed);
    assert!(credit > 0);
    let original = budgets(&fixture);
    for (tokens, _, burst, _) in original {
        assert!(tokens < burst, "real_traffic_must_debit_each_budget");
    }
    let mut disabled = fixture.disable();
    bounded(fixture.control.token().cancelled()).await;
    assert!(timeout(PENDING, &mut disabled).await.is_err());
    assert_eq!(
        weak.upgrade().unwrap().credit.load(Ordering::Relaxed),
        credit
    );
    assert_eq!(budgets(&fixture), original);
    gate.open();
    bounded(disabled).await.unwrap().unwrap();
    assert_socket_released(address);
    assert_eq!(fixture.host.state.udp.available_permits(), permits);
    assert_eq!(budgets(&fixture), original);
    bounded(fixture.host.disable_udp_service("p", "aux", 1))
        .await
        .unwrap();
    assert_eq!(budgets(&fixture), original);
    bounded(association.shutdown()).await;
    drop(association);
    bounded(fixture.guest.shutdown()).await;
    until(|| {
        !fixture
            .host
            .state
            .active_leases
            .lock()
            .unwrap()
            .contains_key("p")
    })
    .await;
    assert_eq!(budgets(&fixture), original);
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_udp_cleanup_and_disable_wait_for_the_same_return_owner() {
    let fixture = Fixture::new().await;
    let permits = fixture.host.state.udp.available_permits();
    let (association, gate, address) = drive_to_return(&fixture).await;
    let weak = fixture
        .control
        .published(association.association_id())
        .unwrap();
    // CloseUdp is processed by the Host while its real return worker is
    // still held. Local shutdown alone cannot make service cleanup complete;
    // the Host token below proves processing, not receipt of the wire reply.
    bounded(association.shutdown()).await;
    assert!(fixture.control.enabled());
    assert!(weak.upgrade().unwrap().cancel.is_cancelled());
    let mut disabled = fixture.disable();
    bounded(fixture.control.token().cancelled()).await;
    assert!(timeout(PENDING, &mut disabled).await.is_err());
    assert_socket_held(address);
    assert_eq!(fixture.host.state.udp.available_permits(), permits - 1);
    gate.open();
    bounded(disabled).await.unwrap().unwrap();
    assert!(weak.upgrade().is_none());
    assert_socket_released(address);
    assert_eq!(fixture.host.state.udp.available_permits(), permits);
    drop(association);
    fixture.assert_game_alive().await;
    fixture.finish().await;
}

async fn concurrent_terminal(shutdown: bool) {
    let fixture = Fixture::new().await;
    let permits = fixture.host.state.udp.available_permits();
    let original_expiry = fixture
        .host
        .state
        .lease("p")
        .unwrap()
        .value
        .expires_at_unix_ms;
    let (association, gate, address) = drive_to_return(&fixture).await;
    let weak = fixture
        .control
        .published(association.association_id())
        .unwrap();
    let mut disabled = fixture.disable();
    bounded(fixture.control.token().cancelled()).await;
    assert!(timeout(PENDING, &mut disabled).await.is_err());
    assert_socket_held(address);
    let stopping = if shutdown {
        let host = fixture.host.clone();
        Some(tokio::spawn(async move { host.shutdown().await }))
    } else {
        assert!(fixture.host.revoke("p"));
        None
    };
    // Peer teardown aborts and joins its owned workers. The return gate is
    // deliberately NOT released here: bounded success plus exact-port rebind
    // proves actual RAII cleanup, rather than merely observing an abort call.
    bounded(disabled).await.unwrap().unwrap();
    if let Some(stopping) = stopping {
        bounded(stopping).await.unwrap();
    }
    bounded(fixture.guest.connection().closed()).await;
    assert!(weak.upgrade().is_none());
    assert_socket_released(address);
    assert_eq!(fixture.control.work_count(), 0);
    assert_eq!(fixture.host.state.udp.available_permits(), permits);
    assert_eq!(
        fixture
            .host
            .state
            .lease("p")
            .unwrap()
            .value
            .expires_at_unix_ms,
        original_expiry
    );
    bounded(fixture.host.disable_udp_service("p", "aux", 1))
        .await
        .unwrap();
    gate.open();
    bounded(association.shutdown()).await;
    drop(association);
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revocation_converges_with_service_cleanup_and_releases_held_return() {
    concurrent_terminal(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_shutdown_converges_with_service_cleanup_and_releases_held_return() {
    concurrent_terminal(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn original_schema_three_expiry_converges_with_service_cleanup() {
    let tcp = TcpListener::bind(local()).await.unwrap();
    let destination = UdpSocket::bind(local()).await.unwrap();
    let services = vec![
        Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: tcp.local_addr().unwrap(),
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        },
        Service {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            endpoint: destination.local_addr().unwrap(),
            revision: 1,
            max_datagram_bytes: 4096,
            required_capability: Capability::FragmentedDatagram,
            udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                bytes_per_second: 16384,
                datagrams_per_second: 20,
            },
        },
    ];
    let reserved = std::net::UdpSocket::bind(local()).unwrap();
    let bind_addr = reserved.local_addr().unwrap();
    let host_key = generate_key();
    let guest_key = generate_key();
    let invitation = SessionInvitation::issue_fragmented_loopback_for_lifetimes(
        &host_key,
        guest_key.public(),
        "udp-original-expiry",
        bind_addr,
        &services,
        Duration::from_secs(30),
        Duration::from_secs(60),
    )
    .unwrap();
    let lease_id = invitation.invitation_id().to_owned();
    let original_expiry = invitation.session_expires_at_unix_ms();
    let wall = Arc::new(AtomicU64::new(unix_millis()));
    let controlled = invitation.clone().with_test_wall_clock(wall.clone());
    drop(reserved);
    let host = Arc::new(
        bounded(Host::start_invited_fragmented_session(
            HostConfig {
                secret_key: host_key,
                bind_addr,
                session_id: invitation.session_id().into(),
                services,
                leases: vec![invitation.lease()],
                limits: Limits::default(),
            },
            &controlled,
        ))
        .await
        .unwrap(),
    );
    let guest = bounded(Guest::connect_invited_fragmented_session(
        guest_key,
        &invitation,
        Limits::default(),
    ))
    .await
    .unwrap();
    let handle = host.state.services["aux"].0;
    let control = host.state.lease(&lease_id).unwrap().udp_services[&handle].clone();
    let mut addresses: BTreeSet<_> = host.bound_sockets().into_iter().collect();
    addresses.extend(guest.bound_sockets());
    addresses.insert(destination.local_addr().unwrap());
    let fixture = Fixture {
        host,
        guest,
        tcp,
        destination,
        control,
        addresses,
    };
    fixture.assert_game_alive().await;
    let permits = fixture.host.state.udp.available_permits();
    let (association, gate, address) = drive_to_return(&fixture).await;
    let weak = fixture
        .control
        .published(association.association_id())
        .unwrap();
    let host = fixture.host.clone();
    let disable_id = lease_id.clone();
    let mut disabled =
        tokio::spawn(async move { host.disable_udp_service(&disable_id, "aux", 1).await });
    bounded(fixture.control.token().cancelled()).await;
    assert!(timeout(PENDING, &mut disabled).await.is_err());
    assert_socket_held(address);
    assert!(controlled.check_session_current().is_ok());
    // Only the retained invitation's existing private test clock moves. Its
    // signed deadline, scalar lease and the Windows clock stay unchanged.
    wall.store(original_expiry, Ordering::SeqCst);
    bounded(disabled).await.unwrap().unwrap();
    bounded(fixture.guest.connection().closed()).await;
    assert!(weak.upgrade().is_none());
    assert_socket_released(address);
    assert_eq!(fixture.control.work_count(), 0);
    assert_eq!(fixture.host.state.udp.available_permits(), permits);
    assert_eq!(
        fixture
            .host
            .state
            .lease(&lease_id)
            .unwrap()
            .value
            .expires_at_unix_ms,
        original_expiry
    );
    bounded(fixture.host.disable_udp_service(&lease_id, "aux", 1))
        .await
        .unwrap();
    gate.open();
    bounded(association.shutdown()).await;
    drop(association);
    fixture.finish().await;
}
