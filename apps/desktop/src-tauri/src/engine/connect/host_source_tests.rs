//! Causal ownership tests use only a native child and loopback sockets.
use super::*;
use crate::engine::connect_discovery::{
    tests::{expire_selection, selection, Fixture},
    DiscoveryError, DiscoveryManager, TargetStatus,
};
use group_source::SourceGate;
use host_source::{SourceHooks, SourceRequest};
use tokio::time::{sleep, timeout};

async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    timeout(Duration::from_secs(8), future)
        .await
        .expect("owned individual fixture deadline")
}
async fn phase(manager: &ConnectManager, phase: &str) -> ConnectSnapshot {
    bounded(async {
        loop {
            let observed = manager.snapshot("main").unwrap();
            if observed.phase == phase {
                return observed;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
}
async fn stopped(manager: &ConnectManager, id: &str) {
    assert_eq!(
        bounded(manager.stop("main", id))
            .await
            .unwrap()
            .cleanup_state,
        "complete"
    );
}
fn request(profile: usize, target: &str, bind: SocketAddr) -> SourceRequest {
    match profile {
        0 => SourceRequest::Known(
            HostRequest {
                mode: ConnectMode::Loopback,
                bind_address: bind.to_string(),
                game_target: target.into(),
                lifetime_seconds: 30,
            }
            .into(),
        ),
        1 => SourceRequest::Known(
            SessionHostRequest {
                mode: ConnectMode::Loopback,
                bind_address: bind.to_string(),
                game_target: target.into(),
                admission_seconds: 30,
                session_seconds: 60,
            }
            .into(),
        ),
        2 => SourceRequest::Fragmented(SessionFragmentedDiscoveredHostRequest {
            mode: ConnectMode::Loopback,
            bind_address: bind.to_string(),
            udp_target: "127.0.0.1:43002".into(),
            admission_seconds: 30,
            session_seconds: 60,
        }),
        _ => panic!("fixture profile"),
    }
}
fn reserve(
    manager: &ConnectManager,
    discovery: &DiscoveryManager,
    selected: &TargetStatus,
    bind: SocketAddr,
    profile: usize,
    hooks: SourceHooks,
) -> PendingHostPreparation {
    manager
        .live
        .reserve_host_source(
            "main",
            request(profile, &selected.target, bind),
            selected.selection_id.clone(),
            discovery.clone(),
            hooks,
        )
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_host_all_profiles_reserve_before_query_and_lost_receivers_keep_cleanup_owned() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let manager = Arc::new(ConnectManager::default());
    for (profile, name) in ["legacy", "sessionGameTcp", "sessionFragmentedGameUdp"]
        .iter()
        .enumerate()
    {
        let selected = selection(&discovery, &mut fixture).await;
        let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let bind = held.local_addr().unwrap();
        let gate = Arc::new(SourceGate::default());
        let before_reservation = lab::unix_millis();
        let pending = reserve(
            &manager,
            &discovery,
            &selected,
            bind,
            profile,
            SourceHooks {
                preparing: Some(gate.clone()),
                ..Default::default()
            },
        );
        let after_reservation = lab::unix_millis();
        bounded(gate.entered.notified()).await;
        let observed = manager.snapshot("main").unwrap();
        let id = observed.operation_id.clone().unwrap();
        assert_eq!(observed.phase, "hostPreparing");
        assert_eq!(observed.cleanup_state, "pending");
        assert_eq!(observed.transport_state, "notStarted");
        assert!(observed.preparation.is_none() && observed.review.is_none());
        assert!(observed.host_group.is_none() && observed.pending_group_source.is_none());
        assert!(
            observed.session_timing.is_none()
                && observed.fragmented_udp.is_none()
                && observed.udp.is_none()
        );
        assert_eq!(
            observed.pending_host_source.as_ref().unwrap().profile,
            *name
        );
        assert_eq!(observed.invitation_profile, (profile != 0).then_some(*name));
        assert!((before_reservation + 60_000..=after_reservation + 60_000)
            .contains(&observed.expires_at_ms.unwrap()));
        let json = serde_json::to_value(&observed).unwrap();
        assert_eq!(json["pendingHostSource"].as_object().unwrap().len(), 3);
        assert!(json.get("hostGroup").is_none() && json.get("fragmentedUdp").is_none());
        // Dropping the result receiver never transfers or cancels work.
        drop(pending);
        assert!(matches!(
            manager.live.reserve_host_source(
                "main",
                request(profile, &selected.target, bind),
                selected.selection_id.clone(),
                discovery.clone(),
                SourceHooks::default()
            ),
            Err(ConnectError::Busy)
        ));
        let closing_manager = manager.clone();
        let closing_id = id.clone();
        let waiter = tokio::spawn(async move { closing_manager.stop("main", &closing_id).await });
        phase(&manager, "closed").await;
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        let closing = manager.snapshot("main").unwrap();
        assert_eq!(closing.cleanup_state, "pending");
        assert!(closing.pending_host_source.is_none() && closing.expires_at_ms.is_none());
        assert_eq!(closing.transport_state, "closed");
        gate.release();
        stopped(&manager, &id).await;
        assert!(discovery
            .take_target("main", &selected.selection_id)
            .is_ok());
        assert!(manager.snapshot("main").unwrap().preparation.is_none());
    }
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_host_queries_after_consumption_remain_cancellable_without_restoring_selection() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    for after_activation in [false, true] {
        let manager = Arc::new(ConnectManager::default());
        let selected = selection(&discovery, &mut fixture).await;
        let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let gate = Arc::new(SourceGate::default());
        let hooks = if after_activation {
            SourceHooks {
                validating: Some(gate.clone()),
                ..Default::default()
            }
        } else {
            SourceHooks {
                activating: Some(gate.clone()),
                ..Default::default()
            }
        };
        let pending = reserve(
            &manager,
            &discovery,
            &selected,
            held.local_addr().unwrap(),
            1,
            hooks,
        );
        bounded(gate.entered.notified()).await;
        let id = manager.snapshot("main").unwrap().operation_id.unwrap();
        assert!(matches!(
            discovery.take_target("main", &selected.selection_id),
            Err(DiscoveryError::Stale)
        ));
        let closing_manager = manager.clone();
        let closing_id = id.clone();
        let waiter = tokio::spawn(async move { closing_manager.stop("main", &closing_id).await });
        assert_eq!(phase(&manager, "closed").await.cleanup_state, "pending");
        assert!(!waiter.is_finished());
        gate.release();
        assert!(matches!(
            bounded(pending.wait()).await,
            Err(DiscoveredPreparationError::Connect(ConnectError::Cancelled))
        ));
        assert_eq!(
            bounded(waiter).await.unwrap().unwrap().cleanup_state,
            "complete"
        );
        assert!(manager.snapshot("main").unwrap().preparation.is_none());
    }
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_host_shutdown_waits_reserved_query_and_never_publishes_an_identity() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let selected = selection(&discovery, &mut fixture).await;
    let manager = Arc::new(ConnectManager::default());
    let gate = Arc::new(SourceGate::default());
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = held.local_addr().unwrap();
    let pending = reserve(
        &manager,
        &discovery,
        &selected,
        bind,
        2,
        SourceHooks {
            preparing: Some(gate.clone()),
            ..Default::default()
        },
    );
    bounded(gate.entered.notified()).await;
    let closing = manager.clone();
    let waiter = tokio::spawn(async move { closing.shutdown_all().await });
    let observed = phase(&manager, "closed").await;
    assert_eq!(observed.cleanup_state, "pending");
    assert!(!waiter.is_finished());
    gate.release();
    assert!(matches!(
        bounded(pending.wait()).await,
        Err(DiscoveredPreparationError::Connect(ConnectError::Cancelled))
    ));
    bounded(waiter).await.unwrap();
    assert_eq!(manager.snapshot("main").unwrap().cleanup_state, "complete");
    assert!(matches!(
        manager.reserve_host_discovered(
            "main",
            HostRequest {
                mode: ConnectMode::Loopback,
                bind_address: bind.to_string(),
                game_target: selected.target.clone(),
                lifetime_seconds: 30,
            },
            selected.selection_id.clone(),
            discovery.clone()
        ),
        Err(ConnectError::Cancelled)
    ));
    assert!(discovery
        .take_target("main", &selected.selection_id)
        .is_ok());
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_host_query_panic_and_post_reservation_failure_preserve_observable_attempt() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let selected = selection(&discovery, &mut fixture).await;
    let manager = ConnectManager::default();
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = held.local_addr().unwrap();
    let baseline = manager.snapshot("main").unwrap();
    // A definitive pure rejection cannot queue a future reservation.
    assert!(matches!(
        manager.live.reserve_host_source(
            "main",
            request(0, &selected.target, bind),
            "invalid-id".into(),
            discovery.clone(),
            SourceHooks::default()
        ),
        Err(ConnectError::InvalidRequest)
    ));
    let unchanged = manager.snapshot("main").unwrap();
    assert_eq!(unchanged.operation_id, baseline.operation_id);
    assert_eq!(unchanged.generation, baseline.generation);
    let error_tail = Arc::new(SourceGate::default());
    let pending = reserve(
        &manager,
        &discovery,
        &selected,
        bind,
        0,
        SourceHooks {
            panic_query: true,
            published: Some(error_tail.clone()),
            ..Default::default()
        },
    );
    assert!(matches!(
        bounded(pending.wait()).await,
        Err(DiscoveredPreparationError::Connect(ConnectError::Internal))
    ));
    bounded(error_tail.entered.notified()).await;
    let failed = manager.snapshot("main").unwrap();
    assert_ne!(failed.operation_id, baseline.operation_id);
    assert!(failed.generation > baseline.generation);
    assert_eq!(failed.phase, "failed");
    assert_eq!(failed.error_code, Some("connect_internal"));
    assert_eq!(failed.cleanup_state, "pending");
    assert!(failed.preparation.is_none() && failed.pending_host_source.is_none());
    assert!(matches!(
        manager.live.reserve_host_source(
            "main",
            request(0, &selected.target, bind),
            selected.selection_id.clone(),
            discovery.clone(),
            SourceHooks::default()
        ),
        Err(ConnectError::Busy)
    ));
    error_tail.release();
    stopped(&manager, failed.operation_id.as_deref().unwrap()).await;
    let terminal = manager.snapshot("main").unwrap();
    assert!(matches!(
        manager.live.reserve_host_source(
            "main",
            request(0, &selected.target, bind),
            "invalid-id".into(),
            discovery.clone(),
            SourceHooks::default()
        ),
        Err(ConnectError::InvalidRequest)
    ));
    let unchanged = manager.snapshot("main").unwrap();
    assert_eq!(unchanged.operation_id, terminal.operation_id);
    assert_eq!(unchanged.generation, terminal.generation);
    assert!(discovery
        .take_target("main", &selected.selection_id)
        .is_ok());
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_host_all_profiles_publication_tail_survives_fast_confirmation_and_stop() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    for profile in 0..3 {
        let manager = Arc::new(ConnectManager::default());
        let selected = selection(&discovery, &mut fixture).await;
        let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let bind = held.local_addr().unwrap();
        let prepare_gate = Arc::new(SourceGate::default());
        let tail_gate = Arc::new(SourceGate::default());
        let pending = reserve(
            &manager,
            &discovery,
            &selected,
            bind,
            profile,
            SourceHooks {
                preparing: Some(prepare_gate.clone()),
                published: Some(tail_gate.clone()),
                ..Default::default()
            },
        );
        bounded(prepare_gate.entered.notified()).await;
        let reserved = manager.snapshot("main").unwrap();
        prepare_gate.release();
        let prepared = bounded(pending.wait()).await.unwrap();
        bounded(tail_gate.entered.notified()).await;
        assert_eq!(Some(prepared.operation_id.clone()), reserved.operation_id);
        assert_eq!(prepared.generation, reserved.generation);
        assert_eq!(Some(prepared.expires_at_ms), reserved.expires_at_ms);
        let observed = manager.snapshot("main").unwrap();
        assert_eq!(observed.phase, "hostPrepared");
        assert!(observed.pending_host_source.is_none() && observed.expires_at_ms.is_none());
        let snapshot_json = serde_json::to_value(&observed).unwrap();
        assert!(snapshot_json.get("pendingHostSource").is_none());
        assert!(snapshot_json.get("pendingGroupSource").is_none());
        let json = serde_json::to_value(&prepared).unwrap();
        assert_eq!(
            json.as_object().unwrap().len(),
            if profile == 0 { 4 } else { 5 }
        );
        let review = manager
            .reserve_host_review(
                "main",
                &prepared.operation_id,
                &lab::generate_key().public().to_string(),
            )
            .unwrap()
            .wait()
            .await
            .unwrap();
        assert_eq!(review.target_source, Some("ownedInstance"));
        drop(held);
        manager
            .confirm("main", &prepared.operation_id, &review.digest)
            .await
            .unwrap();
        phase(&manager, "hostListening").await;
        let closing_manager = manager.clone();
        let id = prepared.operation_id.clone();
        let waiter = tokio::spawn(async move { closing_manager.stop("main", &id).await });
        assert_eq!(phase(&manager, "closed").await.cleanup_state, "pending");
        assert!(!waiter.is_finished());
        tail_gate.release();
        assert_eq!(
            bounded(waiter).await.unwrap().unwrap().cleanup_state,
            "complete"
        );
        assert!(std::net::UdpSocket::bind(bind).is_ok());
    }
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_host_original_preparation_clocks_expire_during_query_without_releasing_work() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    for civil in [false, true] {
        let manager = ConnectManager::default();
        let selected = selection(&discovery, &mut fixture).await;
        let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let gate = Arc::new(SourceGate::default());
        let pending = reserve(
            &manager,
            &discovery,
            &selected,
            held.local_addr().unwrap(),
            2,
            SourceHooks {
                preparing: Some(gate.clone()),
                ..Default::default()
            },
        );
        bounded(gate.entered.notified()).await;
        {
            let mut state = lock(&manager.live.state).unwrap();
            let a = state.attempt.as_mut().unwrap();
            if civil {
                a.snapshot.expires_at_ms = Some(lab::unix_millis() - 1);
            } else {
                a.prepared_deadline = Instant::now() - Duration::from_secs(1);
            }
        }
        let expired = manager.snapshot("main").unwrap();
        assert_eq!(expired.phase, "closed");
        assert_eq!(expired.error_code, Some("connect_plan_expired"));
        assert_eq!(expired.cleanup_state, "pending");
        gate.release();
        assert!(matches!(
            bounded(pending.wait()).await,
            Err(DiscoveredPreparationError::Connect(
                ConnectError::PlanExpired
            ))
        ));
        stopped(&manager, expired.operation_id.as_deref().unwrap()).await;
        assert!(discovery
            .take_target("main", &selected.selection_id)
            .is_ok());
    }
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_host_prepared_expiry_keeps_publication_tail_and_cannot_restore_authority() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let selected = selection(&discovery, &mut fixture).await;
    let manager = ConnectManager::default();
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let gate = Arc::new(SourceGate::default());
    let pending = reserve(
        &manager,
        &discovery,
        &selected,
        held.local_addr().unwrap(),
        1,
        SourceHooks {
            published: Some(gate.clone()),
            ..Default::default()
        },
    );
    let prepared = bounded(pending.wait()).await.unwrap();
    bounded(gate.entered.notified()).await;
    lock(&manager.live.state)
        .unwrap()
        .attempt
        .as_mut()
        .unwrap()
        .prepared_deadline = Instant::now();
    let expired = manager.snapshot("main").unwrap();
    assert_eq!(expired.phase, "closed");
    assert_eq!(expired.cleanup_state, "pending");
    assert!(expired.preparation.is_none() && expired.review.is_none());
    assert!(manager
        .reserve_host_review(
            "main",
            &prepared.operation_id,
            &lab::generate_key().public().to_string()
        )
        .is_err());
    gate.release();
    stopped(&manager, &prepared.operation_id).await;
    assert!(manager.snapshot("main").unwrap().preparation.is_none());
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_host_selection_owner_expiry_replay_and_equality_fail_inside_reserved_attempts() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let manager = ConnectManager::default();
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = held.local_addr().unwrap();
    for failure in 0..5 {
        let selected = selection(&discovery, &mut fixture).await;
        if failure == 0 {
            expire_selection(&discovery);
        }
        if failure == 1 {
            let _ = discovery
                .take_target("main", &selected.selection_id)
                .unwrap();
        }
        let owner = if failure == 2 { "other" } else { "main" };
        let profile = if failure == 4 { 1 } else { 0 };
        let target = if failure >= 3 {
            "127.0.0.1:1"
        } else {
            &selected.target
        };
        let pending = manager
            .live
            .reserve_host_source(
                owner,
                request(profile, target, bind),
                selected.selection_id,
                discovery.clone(),
                SourceHooks::default(),
            )
            .unwrap();
        let error = bounded(pending.wait()).await.unwrap_err();
        assert_eq!(
            error.code(),
            match failure {
                0 | 1 => "discovery_stale_review",
                2 => "discovery_owner_mismatch",
                _ => "connect_invalid_request",
            }
        );
        let failed = manager.snapshot(owner).unwrap();
        assert_eq!(failed.phase, "failed");
        assert!(failed.preparation.is_none() && failed.pending_host_source.is_none());
        assert_eq!(failed.transport_state, "closed");
        assert_eq!(
            bounded(manager.stop(owner, failed.operation_id.as_deref().unwrap()))
                .await
                .unwrap()
                .cleanup_state,
            "complete"
        );
    }
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_host_source_replacement_during_validation_refuses_identity_and_transport() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let selected = selection(&discovery, &mut fixture).await;
    let game: SocketAddr = selected.target.parse().unwrap();
    let manager = ConnectManager::default();
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let gate = Arc::new(SourceGate::default());
    let pending = reserve(
        &manager,
        &discovery,
        &selected,
        held.local_addr().unwrap(),
        2,
        SourceHooks {
            validating: Some(gate.clone()),
            ..Default::default()
        },
    );
    bounded(gate.entered.notified()).await;
    fixture.stop();
    let replacement = std::net::TcpListener::bind(game).unwrap();
    gate.release();
    assert!(matches!(
        bounded(pending.wait()).await,
        Err(DiscoveredPreparationError::Connect(
            ConnectError::SourceChanged
        ))
    ));
    let failed = manager.snapshot("main").unwrap();
    assert!(failed.preparation.is_none() && failed.pending_host_source.is_none());
    assert_eq!(failed.transport_state, "closed");
    stopped(&manager, failed.operation_id.as_deref().unwrap()).await;
    drop(replacement);
}
