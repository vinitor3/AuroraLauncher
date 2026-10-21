//! Three real loopback agents plus a launcher-owned native STATUS fixture.
//! These tests do not start Minecraft, Java, a personal instance or internet IO.
use super::*;
use crate::engine::connect_discovery::{tests::Fixture, DiscoveryManager, TargetStatus};
use group_source::{SourceGate, SourceHooks};
use std::collections::BTreeSet;
use tokio::{
    net::UdpSocket,
    time::{sleep, timeout},
};

const WAIT: Duration = Duration::from_secs(8);
fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    timeout(WAIT, future)
        .await
        .expect("owned group fixture deadline")
}
async fn selection(
    discovery: &DiscoveryManager,
    fixture: &mut Fixture,
    owner: &str,
) -> TargetStatus {
    let inventory = discovery.inventory(owner, fixture.proof()).unwrap();
    let candidate = inventory
        .candidates
        .iter()
        .find(|row| row.bind_kind == "loopback" && row.target.starts_with("127.0.0.1:"))
        .unwrap();
    let review = discovery
        .review(owner, &inventory.inventory_id, &candidate.candidate_id)
        .unwrap();
    discovery
        .confirm_status(owner, &review.review_id, &review.digest)
        .await
        .unwrap()
}
fn request(bind: SocketAddr, udp: SocketAddr) -> GroupDiscoveredHostRequest {
    GroupDiscoveredHostRequest {
        mode: ConnectMode::Loopback,
        bind_address: bind.to_string(),
        udp_target: udp.to_string(),
    }
}
fn participants(keys: &[lab::SecretKey; 2]) -> Vec<GroupParticipantRequest> {
    keys.iter()
        .map(|key| GroupParticipantRequest {
            guest_public_id: key.public().to_string(),
            admission_seconds: 60,
            session_seconds: 120,
        })
        .collect()
}
async fn phase(manager: &ConnectManager, desired: &str) -> ConnectSnapshot {
    bounded(async {
        loop {
            let snapshot = manager.snapshot("main").unwrap();
            if snapshot.phase == desired {
                return snapshot;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
}
async fn stopped(manager: &ConnectManager, id: &str) -> ConnectSnapshot {
    let snapshot = bounded(manager.stop("main", id)).await.unwrap();
    assert_eq!(snapshot.cleanup_state, "complete");
    snapshot
}
fn prepare(
    manager: &ConnectManager,
    discovery: &DiscoveryManager,
    selection: &str,
    bind: SocketAddr,
    udp: SocketAddr,
    hooks: SourceHooks,
) -> ConnectSnapshot {
    manager
        .live
        .prepare_group_source(
            "main",
            request(bind, udp),
            selection.into(),
            discovery.clone(),
            hooks,
        )
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_group_preparing_reserves_before_query_and_lost_stop_waiter_keeps_slot() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let target = selection(&discovery, &mut fixture, "main").await;
    let manager = Arc::new(ConnectManager::default());
    let gate = Arc::new(SourceGate::default());
    let hold = std::net::UdpSocket::bind(local()).unwrap();
    let bind = hold.local_addr().unwrap();
    let snapshot = prepare(
        &manager,
        &discovery,
        &target.selection_id,
        bind,
        "127.0.0.1:43002".parse().unwrap(),
        SourceHooks {
            preparing: Some(gate.clone()),
            ..Default::default()
        },
    );
    let id = snapshot.operation_id.unwrap();
    assert_eq!(snapshot.phase, "hostPreparing");
    assert!(snapshot.pending_group_source.is_some() && snapshot.host_group.is_none());
    assert_eq!(snapshot.cleanup_state, "pending");
    bounded(gate.entered.notified()).await;
    let first_manager = manager.clone();
    let first_id = id.clone();
    let first = tokio::spawn(async move { first_manager.stop("main", &first_id).await });
    phase(&manager, "closed").await;
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    let pending = manager.snapshot("main").unwrap();
    assert_eq!(pending.cleanup_state, "pending");
    assert!(pending.host_group.is_none() && pending.pending_group_source.is_none());
    assert!(pending.expires_at_ms.is_none());
    assert_eq!(pending.transport_state, "closed");
    assert!(matches!(
        manager.prepare_session_group_host_discovered(
            "main",
            request(bind, "127.0.0.1:43002".parse().unwrap()),
            target.selection_id.clone(),
            discovery.clone()
        ),
        Err(ConnectError::Busy)
    ));
    gate.release();
    stopped(&manager, &id).await;
    // Cancellation happened before consuming the original opaque selection.
    assert!(discovery.take_target("main", &target.selection_id).is_ok());
    assert!(manager.snapshot("main").unwrap().host_group.is_none());
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_group_publication_tail_survives_review_confirmation_and_stop() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let target = selection(&discovery, &mut fixture, "main").await;
    let manager = Arc::new(ConnectManager::default());
    let gate = Arc::new(SourceGate::default());
    let hold = std::net::UdpSocket::bind(local()).unwrap();
    let bind = hold.local_addr().unwrap();
    let pending = prepare(
        &manager,
        &discovery,
        &target.selection_id,
        bind,
        "127.0.0.1:43002".parse().unwrap(),
        SourceHooks {
            published: Some(gate.clone()),
            ..Default::default()
        },
    );
    let id = pending.operation_id.unwrap();
    bounded(gate.entered.notified()).await;
    let prepared = phase(&manager, "hostPrepared").await;
    assert!(prepared.pending_group_source.is_none());
    assert_eq!(prepared.expires_at_ms, pending.expires_at_ms);
    let keys = [lab::generate_key(), lab::generate_key()];
    let review = manager
        .review_session_group_host("main", &id, participants(&keys))
        .unwrap();
    assert_eq!(review.target_source, "ownedInstance");
    assert_eq!(review.instance_id.as_deref(), Some("fixture-instance"));
    assert_eq!(review.run_id.as_deref(), Some("fixture-run"));
    drop(hold);
    manager
        .confirm_session_group_host("main", &id, &review.digest)
        .await
        .unwrap();
    phase(&manager, "hostListening").await;
    let closing_manager = manager.clone();
    let closing_id = id.clone();
    let close = tokio::spawn(async move { closing_manager.stop("main", &closing_id).await });
    phase(&manager, "closed").await;
    assert!(!close.is_finished());
    assert_eq!(manager.snapshot("main").unwrap().cleanup_state, "pending");
    gate.release();
    assert_eq!(
        bounded(close).await.unwrap().unwrap().cleanup_state,
        "complete"
    );
    assert!(std::net::UdpSocket::bind(bind).is_ok());
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_group_review_query_guard_outlives_lost_waiter_and_refuses_duplicate() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let target = selection(&discovery, &mut fixture, "main").await;
    let manager = Arc::new(ConnectManager::default());
    let gate = Arc::new(SourceGate::default());
    let hold = std::net::UdpSocket::bind(local()).unwrap();
    let bind = hold.local_addr().unwrap();
    let pending = prepare(
        &manager,
        &discovery,
        &target.selection_id,
        bind,
        "127.0.0.1:43002".parse().unwrap(),
        SourceHooks {
            review: Some(gate.clone()),
            ..Default::default()
        },
    );
    let id = pending.operation_id.unwrap();
    phase(&manager, "hostPrepared").await;
    let keys = [lab::generate_key(), lab::generate_key()];
    let query_manager = manager.clone();
    let query_id = id.clone();
    let roster = participants(&keys);
    let (send, receive) = tokio::sync::oneshot::channel();
    let waiter = tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            let result = query_manager.review_session_group_host("main", &query_id, roster);
            let _ = send.send(result);
        })
        .await;
        result.unwrap();
    });
    bounded(gate.entered.notified()).await;
    assert!(matches!(
        manager.review_session_group_host("main", &id, participants(&keys)),
        Err(ConnectError::Busy)
    ));
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    let closing_manager = manager.clone();
    let closing_id = id.clone();
    let close = tokio::spawn(async move { closing_manager.stop("main", &closing_id).await });
    phase(&manager, "closed").await;
    assert_eq!(manager.snapshot("main").unwrap().cleanup_state, "pending");
    assert!(!close.is_finished());
    gate.release();
    assert!(matches!(
        bounded(receive).await.unwrap(),
        Err(ConnectError::Cancelled)
    ));
    assert_eq!(
        bounded(close).await.unwrap().unwrap().cleanup_state,
        "complete"
    );
    assert!(manager
        .snapshot("main")
        .unwrap()
        .host_group
        .unwrap()
        .review
        .is_none());
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_group_rejects_unknown_wrong_owner_reused_and_replaced_source() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let held = std::net::UdpSocket::bind(local()).unwrap();
    let bind = held.local_addr().unwrap();
    let udp = "127.0.0.1:43002".parse().unwrap();
    let manager = ConnectManager::default();
    for case in 0..2 {
        let (selection_id, expected) = if case == 0 {
            (uuid::Uuid::new_v4().to_string(), ConnectError::PlanExpired)
        } else {
            (
                selection(&discovery, &mut fixture, "other")
                    .await
                    .selection_id,
                ConnectError::OwnerMismatch,
            )
        };
        let pending = prepare(
            &manager,
            &discovery,
            &selection_id,
            bind,
            udp,
            SourceHooks::default(),
        );
        let failed = phase(&manager, "failed").await;
        assert!(failed.host_group.is_none());
        assert_eq!(failed.error_code, Some(expected.code()));
        stopped(&manager, pending.operation_id.as_deref().unwrap()).await;
    }
    let target = selection(&discovery, &mut fixture, "main").await;
    let pending = prepare(
        &manager,
        &discovery,
        &target.selection_id,
        bind,
        udp,
        SourceHooks::default(),
    );
    phase(&manager, "hostPrepared").await;
    stopped(&manager, pending.operation_id.as_deref().unwrap()).await;
    let pending = prepare(
        &manager,
        &discovery,
        &target.selection_id,
        bind,
        udp,
        SourceHooks::default(),
    );
    assert_eq!(
        phase(&manager, "failed").await.error_code,
        Some(ConnectError::PlanExpired.code())
    );
    stopped(&manager, pending.operation_id.as_deref().unwrap()).await;
    let target = selection(&discovery, &mut fixture, "main").await;
    let pending = prepare(
        &manager,
        &discovery,
        &target.selection_id,
        bind,
        udp,
        SourceHooks::default(),
    );
    phase(&manager, "hostPrepared").await;
    fixture.stop();
    let replacement = std::net::TcpListener::bind(&target.target).unwrap();
    let keys = [lab::generate_key(), lab::generate_key()];
    assert!(matches!(
        manager.review_session_group_host(
            "main",
            pending.operation_id.as_deref().unwrap(),
            participants(&keys)
        ),
        Err(ConnectError::SourceChanged)
    ));
    let failed = stopped(&manager, pending.operation_id.as_deref().unwrap()).await;
    assert_eq!(failed.phase, "failed");
    assert_eq!(failed.transport_state, "closed");
    drop(replacement);
}

async fn status(bridge: &lab::TcpBridge, port: u16) {
    let result = bounded(lab::probe_minecraft_status(lab::MinecraftStatusRequest {
        endpoint: bridge.local_addr(),
        handshake_host: "localhost".into(),
        handshake_port: port,
        protocol: 767,
        timeout: Duration::from_secs(5),
    }))
    .await
    .unwrap();
    assert_eq!(result.reported_protocol, 767);
}
async fn datagram(origin: &UdpSocket, bridge: &lab::UdpBridge, tag: u8) {
    let bytes = vec![tag; 4096];
    assert_eq!(
        origin.send_to(&bytes, bridge.local_addr()).await.unwrap(),
        4096
    );
    let mut got = [0; 4097];
    let (size, address) = bounded(origin.recv_from(&mut got)).await.unwrap();
    assert_eq!(size, 4096);
    assert_eq!(address, bridge.local_addr());
    assert_eq!(&got[..size], bytes.as_slice());
}

async fn two_guests(revoke_first: bool) {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let target = selection(&discovery, &mut fixture, "main").await;
    let port = target.target.parse::<SocketAddr>().unwrap().port();
    let manager = ConnectManager::default();
    let held = std::net::UdpSocket::bind(local()).unwrap();
    let bind = held.local_addr().unwrap();
    let udp = UdpSocket::bind(local()).await.unwrap();
    let aux = udp.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let sources = Arc::new(Mutex::new(BTreeSet::new()));
    let observed = sources.clone();
    let echo = tokio::spawn(async move {
        let mut data = [0; 4097];
        loop {
            let (size, from) = tokio::select! {
                _=token.cancelled()=>break,
                value=udp.recv_from(&mut data)=>value.unwrap(),
            };
            assert!(size <= 4096);
            observed.lock().unwrap().insert(from);
            udp.send_to(&data[..size], from).await.unwrap();
        }
    });
    let pending = prepare(
        &manager,
        &discovery,
        &target.selection_id,
        bind,
        aux,
        SourceHooks::default(),
    );
    let id = pending.operation_id.unwrap();
    let prepared = phase(&manager, "hostPrepared")
        .await
        .host_group
        .unwrap()
        .preparation
        .unwrap();
    let keys = [lab::generate_key(), lab::generate_key()];
    let review = manager
        .review_session_group_host("main", &id, participants(&keys))
        .unwrap();
    drop(held);
    manager
        .confirm_session_group_host("main", &id, &review.digest)
        .await
        .unwrap();
    phase(&manager, "hostListening").await;
    let mut guests = Vec::new();
    let mut tcp = Vec::new();
    let mut bridges = Vec::new();
    for (i, key) in keys.iter().enumerate() {
        let exported = manager
            .export_session_group_invitation("main", &id, &review.participants[i].participant_id)
            .unwrap();
        let invitation = lab::SessionInvitation::verify_fragmented_loopback(
            &exported.invitation,
            prepared.public_id.parse().unwrap(),
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
        tcp.push(bounded(guest.tcp_bridge("game", 1, local())).await.unwrap());
        bridges.push(
            bounded(guest.udp_bridge_fragmented("aux", 1, local()))
                .await
                .unwrap(),
        );
        guests.push(guest);
    }
    let first = UdpSocket::bind(local()).await.unwrap();
    let second = UdpSocket::bind(local()).await.unwrap();
    tokio::join!(
        status(&tcp[0], port),
        status(&tcp[1], port),
        datagram(&first, &bridges[0], 17),
        datagram(&second, &bridges[1], 92)
    );
    assert_eq!(sources.lock().unwrap().len(), 2);
    let current = manager.snapshot("main").unwrap();
    assert!(current
        .host_group
        .as_ref()
        .unwrap()
        .participants
        .iter()
        .all(|row| row.consumed && row.transport_active && !row.revoked));
    if revoke_first {
        manager
            .revoke_session_group_participant(
                "main",
                &id,
                &review.participants[0].participant_id,
                current.generation,
            )
            .unwrap();
        bounded(guests[0].connection().closed()).await;
        tokio::join!(status(&tcp[1], port), datagram(&second, &bridges[1], 44));
    }
    fixture.stop();
    let failed = phase(&manager, "failed").await;
    assert_eq!(failed.error_code, Some(ConnectError::SourceChanged.code()));
    bounded(guests[0].connection().closed()).await;
    bounded(guests[1].connection().closed()).await;
    let closed = stopped(&manager, &id).await;
    let rows = closed.host_group.unwrap().participants;
    assert!(rows.iter().all(|row| row.consumed && !row.transport_active));
    assert_eq!(rows[0].revoked, revoke_first);
    assert!(!rows[1].revoked);
    for bridge in &tcp {
        bounded(bridge.shutdown()).await;
    }
    for bridge in &bridges {
        bounded(bridge.shutdown()).await;
    }
    for guest in &guests {
        bounded(guest.shutdown()).await;
    }
    let tcp_addresses: Vec<_> = tcp.iter().map(lab::TcpBridge::local_addr).collect();
    let udp_addresses: Vec<_> = bridges.iter().map(lab::UdpBridge::local_addr).collect();
    drop((tcp, bridges, guests));
    cancel.cancel();
    bounded(echo).await.unwrap();
    for address in tcp_addresses {
        assert!(std::net::TcpListener::bind(address).is_ok());
    }
    for address in udp_addresses.into_iter().chain([bind, aux]) {
        assert!(std::net::UdpSocket::bind(address).is_ok());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_group_revoke_one_preserves_other_until_source_loss() {
    two_guests(true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_group_source_loss_closes_both_active_guests() {
    two_guests(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_group_shutdown_waits_initial_query_and_never_publishes_identity() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let target = selection(&discovery, &mut fixture, "main").await;
    let manager = Arc::new(ConnectManager::default());
    let gate = Arc::new(SourceGate::default());
    let held = std::net::UdpSocket::bind(local()).unwrap();
    let bind = held.local_addr().unwrap();
    let udp = "127.0.0.1:43002".parse().unwrap();
    prepare(
        &manager,
        &discovery,
        &target.selection_id,
        bind,
        udp,
        SourceHooks {
            preparing: Some(gate.clone()),
            ..Default::default()
        },
    );
    bounded(gate.entered.notified()).await;
    let closing = manager.clone();
    let shutdown = tokio::spawn(async move { closing.shutdown_all().await });
    phase(&manager, "closed").await;
    assert!(!shutdown.is_finished());
    assert_eq!(manager.snapshot("main").unwrap().cleanup_state, "pending");
    assert!(matches!(
        manager.prepare_session_group_host_discovered(
            "main",
            request(bind, udp),
            target.selection_id.clone(),
            discovery.clone()
        ),
        Err(ConnectError::Cancelled)
    ));
    gate.release();
    bounded(shutdown).await.unwrap();
    let closed = manager.snapshot("main").unwrap();
    assert_eq!(closed.cleanup_state, "complete");
    assert!(closed.host_group.is_none() && closed.pending_group_source.is_none());
    assert!(discovery.take_target("main", &target.selection_id).is_ok());
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_group_expiration_during_preparing_or_review_keeps_query_owned() {
    for during_review in [false, true] {
        let mut fixture = Fixture::new();
        let discovery = DiscoveryManager::default();
        let target = selection(&discovery, &mut fixture, "main").await;
        let manager = Arc::new(ConnectManager::default());
        let gate = Arc::new(SourceGate::default());
        let held = std::net::UdpSocket::bind(local()).unwrap();
        let bind = held.local_addr().unwrap();
        let udp = "127.0.0.1:43002".parse().unwrap();
        let hooks = if during_review {
            SourceHooks {
                review: Some(gate.clone()),
                ..Default::default()
            }
        } else {
            SourceHooks {
                preparing: Some(gate.clone()),
                ..Default::default()
            }
        };
        let pending = prepare(&manager, &discovery, &target.selection_id, bind, udp, hooks);
        let id = pending.operation_id.unwrap();
        let query = if during_review {
            phase(&manager, "hostPrepared").await;
            let keys = [lab::generate_key(), lab::generate_key()];
            let roster = participants(&keys);
            let query_manager = manager.clone();
            let query_id = id.clone();
            Some(tokio::task::spawn_blocking(move || {
                query_manager.review_session_group_host("main", &query_id, roster)
            }))
        } else {
            None
        };
        bounded(gate.entered.notified()).await;
        {
            // Private monotonic clock seam; civil expiry and OS time untouched.
            let mut state = lock(&manager.live.state).unwrap();
            selected(&mut state, "main", &id).unwrap().prepared_deadline =
                Instant::now() - Duration::from_millis(1);
        }
        let expired = manager.snapshot("main").unwrap();
        assert_eq!(expired.phase, "closed");
        assert_eq!(expired.cleanup_state, "pending");
        assert_eq!(expired.error_code, Some(ConnectError::PlanExpired.code()));
        assert!(expired
            .host_group
            .as_ref()
            .is_none_or(|group| group.review.is_none() && group.participants.is_empty()));
        assert!(matches!(
            manager.prepare_session_group_host_discovered(
                "main",
                request(bind, udp),
                target.selection_id.clone(),
                discovery.clone()
            ),
            Err(ConnectError::Busy)
        ));
        gate.release();
        if let Some(query) = query {
            assert!(matches!(
                bounded(query).await.unwrap(),
                Err(ConnectError::Cancelled) | Err(ConnectError::PlanExpired)
            ));
        }
        let closed = stopped(&manager, &id).await;
        assert_eq!(closed.error_code, Some(ConnectError::PlanExpired.code()));
        if !during_review {
            assert!(discovery.take_target("main", &target.selection_id).is_ok());
        }
        fixture.stop();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_group_monitor_query_survives_lost_stop_waiter_until_cleanup() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let target = selection(&discovery, &mut fixture, "main").await;
    let manager = Arc::new(ConnectManager::default());
    let gate = Arc::new(SourceGate::default());
    let held = std::net::UdpSocket::bind(local()).unwrap();
    let bind = held.local_addr().unwrap();
    let udp = "127.0.0.1:43002".parse().unwrap();
    let pending = prepare(
        &manager,
        &discovery,
        &target.selection_id,
        bind,
        udp,
        SourceHooks {
            monitor: Some(gate.clone()),
            ..Default::default()
        },
    );
    let id = pending.operation_id.unwrap();
    phase(&manager, "hostPrepared").await;
    let keys = [lab::generate_key(), lab::generate_key()];
    let review = manager
        .review_session_group_host("main", &id, participants(&keys))
        .unwrap();
    drop(held);
    manager
        .confirm_session_group_host("main", &id, &review.digest)
        .await
        .unwrap();
    bounded(gate.entered.notified()).await;
    assert_eq!(manager.snapshot("main").unwrap().phase, "hostListening");
    let stop_manager = manager.clone();
    let stop_id = id.clone();
    let first = tokio::spawn(async move { stop_manager.stop("main", &stop_id).await });
    phase(&manager, "closing").await;
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert_eq!(manager.snapshot("main").unwrap().cleanup_state, "pending");
    //77 keeps the query owned, but now releases transport before it returns.
    let released_port = bounded(async {
        loop {
            if let Ok(socket) = std::net::UdpSocket::bind(bind) {
                break socket;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    assert!(matches!(
        manager.prepare_session_group_host_discovered(
            "main",
            request(bind, udp),
            target.selection_id,
            discovery
        ),
        Err(ConnectError::Busy)
    ));
    gate.release();
    let closed = stopped(&manager, &id).await;
    assert!(closed.error_code.is_none());
    drop(released_port);
    assert!(std::net::UdpSocket::bind(bind).is_ok());
    assert!(DiscoveryManager::default()
        .inventory("main", fixture.proof())
        .is_ok());
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_group_expired_selection_never_creates_host_identity() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let target = selection(&discovery, &mut fixture, "main").await;
    crate::engine::connect_discovery::tests::expire_selection(&discovery);
    let manager = ConnectManager::default();
    let held = std::net::UdpSocket::bind(local()).unwrap();
    let pending = prepare(
        &manager,
        &discovery,
        &target.selection_id,
        held.local_addr().unwrap(),
        "127.0.0.1:43002".parse().unwrap(),
        SourceHooks::default(),
    );
    let failed = phase(&manager, "failed").await;
    assert_eq!(failed.error_code, Some(ConnectError::PlanExpired.code()));
    assert!(failed.host_group.is_none());
    stopped(&manager, pending.operation_id.as_deref().unwrap()).await;
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_group_source_replaced_after_review_refuses_confirm_before_transport() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let target = selection(&discovery, &mut fixture, "main").await;
    let manager = ConnectManager::default();
    let held = std::net::UdpSocket::bind(local()).unwrap();
    let bind = held.local_addr().unwrap();
    let pending = prepare(
        &manager,
        &discovery,
        &target.selection_id,
        bind,
        "127.0.0.1:43002".parse().unwrap(),
        SourceHooks::default(),
    );
    let id = pending.operation_id.unwrap();
    phase(&manager, "hostPrepared").await;
    let keys = [lab::generate_key(), lab::generate_key()];
    let review = manager
        .review_session_group_host("main", &id, participants(&keys))
        .unwrap();
    fixture.stop();
    let replacement = std::net::TcpListener::bind(&target.target).unwrap();
    let starting = manager
        .confirm_session_group_host("main", &id, &review.digest)
        .await
        .unwrap();
    assert_eq!(starting.phase, "starting");
    let failed = phase(&manager, "failed").await;
    assert_eq!(failed.error_code, Some(ConnectError::SourceChanged.code()));
    assert_eq!(failed.transport_state, "closed");
    let final_state = stopped(&manager, &id).await;
    assert!(final_state
        .host_group
        .unwrap()
        .participants
        .iter()
        .all(|row| !row.consumed && !row.revoked && !row.transport_active));
    assert_eq!(held.local_addr().unwrap(), bind);
    drop((held, replacement));
    assert!(std::net::UdpSocket::bind(bind).is_ok());
}
