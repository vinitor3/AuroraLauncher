//! Three authenticated agents, a native owned child, and causal query gates.
use super::*;
use crate::engine::connect_discovery::{
    tests::{selection, Fixture},
    DiscoveryManager,
};
use group_source::{SourceGate, SourceHooks};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
    time::{sleep, timeout},
};

async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    timeout(Duration::from_secs(8), future)
        .await
        .expect("group actor fixture deadline")
}
async fn until(
    manager: &ConnectManager,
    predicate: impl Fn(&ConnectSnapshot) -> bool,
) -> ConnectSnapshot {
    bounded(async {
        loop {
            let snapshot = manager.snapshot("main").unwrap();
            if predicate(&snapshot) {
                break snapshot;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
}
async fn rebind(bind: SocketAddr) -> std::net::UdpSocket {
    bounded(async {
        loop {
            if let Ok(socket) = std::net::UdpSocket::bind(bind) {
                break socket;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
}
fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
struct Case {
    fixture: Fixture,
    manager: Arc<ConnectManager>,
    preparation: GroupPreparation,
    review: GroupReview,
    keys: [lab::SecretKey; 2],
    bind: SocketAddr,
    held: Option<std::net::UdpSocket>,
    game: SocketAddr,
    aux: SocketAddr,
    sources: Arc<Mutex<BTreeSet<SocketAddr>>>,
    packets: Arc<AtomicUsize>,
    cancel: CancellationToken,
    echo: Option<JoinHandle<()>>,
    guests: Vec<lab::Guest>,
    tcp: Vec<lab::TcpBridge>,
    udp: Vec<lab::UdpBridge>,
    origins: Vec<UdpSocket>,
}
impl Drop for Case {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
impl Case {
    async fn new(hooks: SourceHooks, durations: [(u16, u16); 2]) -> Self {
        let mut fixture = Fixture::new();
        let discovery = DiscoveryManager::default();
        let selected = selection(&discovery, &mut fixture).await;
        let game = selected.target.parse().unwrap();
        let held = std::net::UdpSocket::bind(local()).unwrap();
        let bind = held.local_addr().unwrap();
        let endpoint = UdpSocket::bind(local()).await.unwrap();
        let aux = endpoint.local_addr().unwrap();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let sources = Arc::new(Mutex::new(BTreeSet::new()));
        let observed = sources.clone();
        let packets = Arc::new(AtomicUsize::new(0));
        let counted = packets.clone();
        let echo = tokio::spawn(async move {
            let mut bytes = [0; 4097];
            loop {
                let (size, source) = tokio::select! { _=token.cancelled()=>break, value=endpoint.recv_from(&mut bytes)=>value.unwrap() };
                assert!(size <= 4096);
                observed.lock().unwrap().insert(source);
                counted.fetch_add(1, Ordering::AcqRel);
                endpoint.send_to(&bytes[..size], source).await.unwrap();
            }
        });
        let manager = Arc::new(ConnectManager::default());
        manager
            .live
            .prepare_group_source(
                "main",
                GroupDiscoveredHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: bind.to_string(),
                    udp_target: aux.to_string(),
                },
                selected.selection_id,
                discovery,
                hooks,
            )
            .unwrap();
        let preparation = until(&manager, |s| s.phase == "hostPrepared")
            .await
            .host_group
            .unwrap()
            .preparation
            .unwrap();
        let keys = [lab::generate_key(), lab::generate_key()];
        let review = manager
            .review_session_group_host(
                "main",
                &preparation.operation_id,
                keys.iter()
                    .zip(durations)
                    .map(
                        |(key, (admission_seconds, session_seconds))| GroupParticipantRequest {
                            guest_public_id: key.public().to_string(),
                            admission_seconds,
                            session_seconds,
                        },
                    )
                    .collect(),
            )
            .unwrap();
        Self {
            fixture,
            manager,
            preparation,
            review,
            keys,
            bind,
            held: Some(held),
            game,
            aux,
            sources,
            packets,
            cancel,
            echo: Some(echo),
            guests: Vec::new(),
            tcp: Vec::new(),
            udp: Vec::new(),
            origins: Vec::new(),
        }
    }
    fn id(&self) -> &str {
        &self.preparation.operation_id
    }
    async fn start(&mut self) {
        drop(self.held.take());
        self.manager
            .confirm_session_group_host("main", self.id(), &self.review.digest)
            .await
            .unwrap();
    }
    async fn admit(&mut self) {
        for (index, key) in self.keys.iter().enumerate() {
            let export = self
                .manager
                .export_session_group_invitation(
                    "main",
                    self.id(),
                    &self.review.participants[index].participant_id,
                )
                .unwrap();
            let invitation = lab::SessionInvitation::verify_fragmented_loopback(
                &export.invitation,
                self.preparation.public_id.parse().unwrap(),
                key.public(),
            )
            .unwrap();
            let guest = bounded(lab::Guest::connect_invited_fragmented_session(
                key.clone(),
                &invitation,
                lab::Limits::default(),
            ))
            .await
            .unwrap();
            self.tcp
                .push(bounded(guest.tcp_bridge("game", 1, local())).await.unwrap());
            self.udp.push(
                bounded(guest.udp_bridge_fragmented("aux", 1, local()))
                    .await
                    .unwrap(),
            );
            self.guests.push(guest);
        }
        for _ in 0..4 {
            self.origins.push(UdpSocket::bind(local()).await.unwrap());
        }
    }
    async fn status(&self, index: usize) {
        let reply = bounded(lab::probe_minecraft_status(lab::MinecraftStatusRequest {
            endpoint: self.tcp[index].local_addr(),
            handshake_host: "localhost".into(),
            handshake_port: self.game.port(),
            protocol: 767,
            timeout: Duration::from_secs(5),
        }))
        .await
        .unwrap();
        assert!(reply.protocol_matches_requested);
    }
    async fn datagram(&self, origin: usize, tag: u8) {
        let payload = vec![tag; 4096];
        let bridge = &self.udp[origin / 2];
        let socket = &self.origins[origin];
        assert_eq!(
            socket.send_to(&payload, bridge.local_addr()).await.unwrap(),
            4096
        );
        let mut got = [0; 4097];
        let (size, from) = bounded(socket.recv_from(&mut got)).await.unwrap();
        assert_eq!(from, bridge.local_addr());
        assert_eq!(size, 4096);
        assert_eq!(&got[..size], payload.as_slice());
    }
    fn stop_waiter(&self) -> JoinHandle<Result<ConnectSnapshot, ConnectError>> {
        let manager = self.manager.clone();
        let id = self.id().to_owned();
        tokio::spawn(async move { manager.stop("main", &id).await })
    }
    async fn cleanup(mut self) {
        assert_eq!(
            bounded(self.manager.stop("main", self.id()))
                .await
                .unwrap()
                .cleanup_state,
            "complete"
        );
        for bridge in &self.tcp {
            bounded(bridge.shutdown()).await;
        }
        for bridge in &self.udp {
            bounded(bridge.shutdown()).await;
        }
        for guest in &self.guests {
            bounded(guest.shutdown()).await;
        }
        self.cancel.cancel();
        bounded(self.echo.take().unwrap()).await.unwrap();
        self.fixture.stop();
    }
}
// An acknowledged STATUS packet leaves the same real TCP stream awaiting ping.
// Closing must be observed before the fixture's unchanged five-second timeout.
async fn open_status_stream(bridge: &lab::TcpBridge, port: u16) -> TcpStream {
    bounded(async {
        let mut stream = TcpStream::connect(bridge.local_addr()).await.unwrap();
        let mut body = vec![0, 0xff, 5, 9];
        body.extend(b"localhost");
        body.extend(port.to_be_bytes());
        body.push(1);
        stream.write_all(&[body.len() as u8]).await.unwrap();
        stream.write_all(&body).await.unwrap();
        stream.write_all(&[1, 0]).await.unwrap();
        let mut size = 0usize;
        for shift in [0, 7, 14, 21, 28] {
            let byte = stream.read_u8().await.unwrap();
            size |= usize::from(byte & 127) << shift;
            if byte & 128 == 0 {
                break;
            }
        }
        assert!((1..1024).contains(&size));
        let mut response = vec![0; size];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response[0], 0);
        stream
    })
    .await
}
async fn closed_stream(stream: &mut TcpStream) {
    match timeout(Duration::from_secs(2), stream.read(&mut [0]))
        .await
        .expect("active stream did not close before fixture timeout")
    {
        Ok(0) => {}
        Err(error) => assert!(matches!(
            error.kind(),
            std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::NotConnected
        )),
        other => panic!("active TCP remained usable: {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn group_actor_monitor_stop_closes_two_guests_and_active_tcp_udp_before_query_release() {
    let gate = Arc::new(SourceGate::default());
    let mut case = Case::new(
        SourceHooks {
            monitor: Some(gate.clone()),
            ..Default::default()
        },
        [(30, 60); 2],
    )
    .await;
    case.start().await;
    bounded(gate.entered.notified()).await;
    case.admit().await;
    case.status(0).await;
    case.status(1).await;
    for origin in 0..4 {
        case.datagram(origin, 17 + origin as u8).await;
    }
    assert_eq!(case.sources.lock().unwrap().len(), 4);
    assert_eq!(case.packets.load(Ordering::Acquire), 4);
    let mut stream = open_status_stream(&case.tcp[0], case.game.port()).await;
    let waiter = case.stop_waiter();
    // Start the two-second oracle immediately after the acknowledged stream;
    // waiting for a snapshot first could hide the fixture's own five-second EOF.
    closed_stream(&mut stream).await;
    assert_eq!(
        until(&case.manager, |s| s.phase == "closing")
            .await
            .cleanup_state,
        "pending"
    );
    let _port = rebind(case.bind).await;
    for guest in &case.guests {
        bounded(guest.connection().closed()).await;
    }
    bounded(async {
        while case.tcp.iter().any(|b| !b.is_closed()) || case.udp.iter().any(|b| !b.is_closed()) {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    for bridge in &case.tcp {
        assert!(TcpStream::connect(bridge.local_addr()).await.is_err());
    }
    let before = case.packets.load(Ordering::Acquire);
    for (index, origin) in case.origins.iter().enumerate() {
        origin
            .send_to(&[99; 32], case.udp[index / 2].local_addr())
            .await
            .unwrap();
    }
    sleep(Duration::from_millis(80)).await;
    assert_eq!(case.packets.load(Ordering::Acquire), before);
    assert!(!waiter.is_finished());
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    let pending = case.manager.snapshot("main").unwrap();
    assert_eq!(pending.cleanup_state, "pending");
    assert!(pending
        .host_group
        .unwrap()
        .participants
        .iter()
        .all(|p| p.consumed && !p.revoked && !p.transport_active && !p.session_valid));
    assert!(matches!(
        case.manager.prepare_session_group_host(
            "main",
            GroupHostRequest {
                mode: ConnectMode::Loopback,
                bind_address: case.bind.to_string(),
                game_target: case.game.to_string(),
                udp_target: case.aux.to_string()
            }
        ),
        Err(ConnectError::Busy)
    ));
    assert!(DiscoveryManager::default()
        .inventory("main", case.fixture.proof())
        .is_ok());
    gate.release();
    case.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn group_actor_pre_and_post_bind_stop_or_original_ingress_expiry_keep_query_owned() {
    for after_bind in [false, true] {
        for expiry in [false, true] {
            let gate = Arc::new(SourceGate::default());
            let hooks = if after_bind {
                SourceHooks {
                    after_bind: Some(gate.clone()),
                    ..Default::default()
                }
            } else {
                SourceHooks {
                    before_bind: Some(gate.clone()),
                    ..Default::default()
                }
            };
            let mut case = Case::new(
                hooks,
                if expiry {
                    [(2, 20), (30, 60)]
                } else {
                    [(30, 60); 2]
                },
            )
            .await;
            case.start().await;
            bounded(gate.entered.notified()).await;
            assert_eq!(case.manager.snapshot("main").unwrap().phase, "starting");
            if after_bind {
                assert!(std::net::UdpSocket::bind(case.bind).is_err());
            }
            let waiter = (!expiry).then(|| case.stop_waiter());
            assert_eq!(
                until(&case.manager, |s| s.phase == "closing")
                    .await
                    .cleanup_state,
                "pending"
            );
            let _port = rebind(case.bind).await;
            if let Some(waiter) = waiter {
                assert!(!waiter.is_finished());
                waiter.abort();
                assert!(waiter.await.unwrap_err().is_cancelled());
            }
            let closed = case.manager.snapshot("main").unwrap();
            assert!(closed
                .host_group
                .unwrap()
                .participants
                .iter()
                .all(|p| !p.consumed && !p.revoked));
            gate.release();
            case.cleanup().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn group_actor_source_error_or_panic_before_publication_releases_resources() {
    for after_bind in [false, true] {
        for panic_query in [false, true] {
            let gate = Arc::new(SourceGate::default());
            let hooks = if after_bind {
                SourceHooks {
                    after_bind: Some(gate.clone()),
                    panic_after_bind: panic_query,
                    ..Default::default()
                }
            } else {
                SourceHooks {
                    before_bind: Some(gate.clone()),
                    panic_before_bind: panic_query,
                    ..Default::default()
                }
            };
            let mut case = Case::new(hooks, [(30, 60); 2]).await;
            case.start().await;
            bounded(gate.entered.notified()).await;
            let _replacement = if !panic_query {
                case.fixture.stop();
                Some(std::net::TcpListener::bind(case.game).unwrap())
            } else {
                None
            };
            gate.release();
            let failed = until(&case.manager, |s| s.phase == "failed").await;
            assert_eq!(
                failed.error_code,
                Some(if panic_query {
                    "connect_internal"
                } else {
                    "connect_source_changed"
                })
            );
            assert!(failed
                .host_group
                .unwrap()
                .participants
                .iter()
                .all(|p| !p.consumed && !p.revoked));
            let _port = rebind(case.bind).await;
            case.cleanup().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn group_actor_monitor_preserves_consumed_sessions_and_individual_expiry_until_all_finish() {
    let gate = Arc::new(SourceGate::default());
    let mut case = Case::new(
        SourceHooks {
            monitor: Some(gate.clone()),
            ..Default::default()
        },
        [(2, 4), (2, 6)],
    )
    .await;
    case.start().await;
    bounded(gate.entered.notified()).await;
    case.admit().await;
    case.status(0).await;
    case.status(1).await;
    case.datagram(0, 41).await;
    case.datagram(2, 42).await;
    let admitted = case.manager.snapshot("main").unwrap();
    let original = admitted.host_group.as_ref().unwrap().participants.clone();
    assert!(original.iter().all(|p| p.consumed && p.session_valid));

    // Poll the real, originally issued deadlines. No clock replacement and no
    // projection mutation can make an unconsumed invitation become admitted.
    let ingress_finished = until(&case.manager, |s| {
        s.host_group
            .as_ref()
            .unwrap()
            .participants
            .iter()
            .all(|p| !p.admission_valid)
    })
    .await;
    assert_eq!(ingress_finished.phase, "hostListening");
    assert!(ingress_finished
        .host_group
        .unwrap()
        .participants
        .iter()
        .all(|p| p.consumed && p.session_valid && p.transport_active && !p.revoked));
    // Both actual services remain usable after ingress is no longer valid.
    case.status(0).await;
    case.status(1).await;
    case.datagram(0, 43).await;
    case.datagram(2, 44).await;
    assert!(std::net::UdpSocket::bind(case.bind).is_err());

    let first_expired = until(&case.manager, |s| {
        !s.host_group.as_ref().unwrap().participants[0].session_valid
    })
    .await;
    assert_eq!(first_expired.phase, "hostListening");
    let participants = first_expired.host_group.unwrap().participants;
    assert!(participants[0].consumed && !participants[0].revoked);
    assert!(!participants[0].transport_active);
    assert!(participants[1].consumed && participants[1].session_valid);
    assert!(participants[1].transport_active && !participants[1].revoked);
    for (participant, original) in participants.iter().zip(&original) {
        assert_eq!(participant.session_timing, original.session_timing);
        assert_eq!(participant.invitation_digest, original.invitation_digest);
    }
    bounded(case.guests[0].connection().closed()).await;
    case.status(1).await;
    case.datagram(2, 45).await;
    assert!(std::net::UdpSocket::bind(case.bind).is_err());

    let finished = until(&case.manager, |s| s.phase == "closing").await;
    assert_eq!(finished.cleanup_state, "pending");
    assert!(finished
        .host_group
        .unwrap()
        .participants
        .iter()
        .all(|p| p.consumed && !p.revoked && !p.session_valid && !p.transport_active));
    // The group actor must release the Host, even though its source query
    // remains held and the second guest has also reached its own deadline.
    let _port = rebind(case.bind).await;
    let waiter = case.stop_waiter();
    tokio::task::yield_now().await;
    assert!(!waiter.is_finished());
    gate.release();
    assert_eq!(
        bounded(waiter).await.unwrap().unwrap().cleanup_state,
        "complete"
    );
    assert!(DiscoveryManager::default()
        .inventory("main", case.fixture.proof())
        .is_ok());
    case.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn group_actor_monitor_individual_revocation_keeps_other_guest_and_explicit_history() {
    let gate = Arc::new(SourceGate::default());
    let mut case = Case::new(
        SourceHooks {
            monitor: Some(gate.clone()),
            ..Default::default()
        },
        [(30, 60); 2],
    )
    .await;
    case.start().await;
    bounded(gate.entered.notified()).await;
    case.admit().await;
    case.status(0).await;
    case.status(1).await;
    case.datagram(0, 51).await;
    case.datagram(2, 52).await;
    let before = case.manager.snapshot("main").unwrap();
    let original = before.host_group.as_ref().unwrap().participants.clone();
    let mut stream = open_status_stream(&case.tcp[0], case.game.port()).await;
    let revoked = case
        .manager
        .revoke_session_group_participant(
            "main",
            case.id(),
            &original[0].participant_id,
            before.generation,
        )
        .unwrap();
    closed_stream(&mut stream).await;
    bounded(case.guests[0].connection().closed()).await;
    bounded(async {
        while !case.tcp[0].is_closed() || !case.udp[0].is_closed() {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    let participants = revoked.host_group.unwrap().participants;
    assert!(participants[0].revoked && participants[0].consumed);
    assert!(!participants[0].transport_active && !participants[0].session_valid);
    assert!(participants[1].consumed && participants[1].session_valid);
    assert!(participants[1].transport_active && !participants[1].revoked);
    assert_eq!(participants[1].session_timing, original[1].session_timing);
    assert_eq!(
        participants[1].invitation_digest,
        original[1].invitation_digest
    );
    assert_eq!(revoked.phase, "hostListening");
    case.status(1).await;
    case.datagram(2, 53).await;
    assert!(std::net::UdpSocket::bind(case.bind).is_err());
    let packets = case.packets.load(Ordering::Acquire);
    case.origins[0]
        .send_to(&[54; 32], case.udp[0].local_addr())
        .await
        .unwrap();
    sleep(Duration::from_millis(80)).await;
    assert_eq!(case.packets.load(Ordering::Acquire), packets);
    // Global stop must preserve that only A was explicitly revoked.
    let waiter = case.stop_waiter();
    let stopped = until(&case.manager, |s| s.phase == "closing").await;
    let _port = rebind(case.bind).await;
    assert!(!waiter.is_finished());
    let history = stopped.host_group.unwrap().participants;
    assert!(history.iter().all(|p| p.consumed && !p.transport_active));
    assert!(history[0].revoked);
    assert!(!history[1].revoked);
    gate.release();
    let complete = bounded(waiter).await.unwrap().unwrap();
    let history = complete.host_group.unwrap().participants;
    assert!(history[0].revoked);
    assert!(!history[1].revoked);
    case.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn group_actor_monitor_source_error_or_panic_closes_active_guests_and_releases_host() {
    for panic_query in [false, true] {
        let gate = Arc::new(SourceGate::default());
        let mut case = Case::new(
            SourceHooks {
                monitor: Some(gate.clone()),
                panic_monitor: panic_query,
                ..Default::default()
            },
            [(30, 60); 2],
        )
        .await;
        case.start().await;
        bounded(gate.entered.notified()).await;
        case.admit().await;
        case.status(0).await;
        case.status(1).await;
        case.datagram(0, 61).await;
        case.datagram(2, 62).await;
        let mut stream = open_status_stream(&case.tcp[0], case.game.port()).await;
        let _replacement = if !panic_query {
            case.fixture.stop();
            Some(std::net::TcpListener::bind(case.game).unwrap())
        } else {
            None
        };
        gate.release();
        // Start immediately, before polling the failed projection. In the
        // panic case the fixture remains alive and cannot cause the early EOF.
        closed_stream(&mut stream).await;
        let failed = until(&case.manager, |s| s.phase == "failed").await;
        assert_eq!(
            failed.error_code,
            Some(if panic_query {
                "connect_internal"
            } else {
                "connect_source_changed"
            })
        );
        assert!(failed
            .host_group
            .unwrap()
            .participants
            .iter()
            .all(|p| p.consumed && !p.revoked && !p.transport_active && !p.session_valid));
        let _port = rebind(case.bind).await;
        for guest in &case.guests {
            bounded(guest.connection().closed()).await;
        }
        if panic_query {
            assert!(DiscoveryManager::default()
                .inventory("main", case.fixture.proof())
                .is_ok());
        }
        case.cleanup().await;
    }
}
