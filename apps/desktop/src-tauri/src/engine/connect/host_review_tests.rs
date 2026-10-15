//! Owned review/actor barriers, with a native fixture and real local transport.
use super::*;
use crate::engine::connect_discovery::{
    tests::{selection, Fixture},
    DiscoveryManager,
};
use group_source::SourceGate;
use host_review::ReviewHooks;
use tokio::time::{sleep, timeout};

async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    timeout(Duration::from_secs(8), future)
        .await
        .expect("owned review fixture deadline")
}
async fn until(
    manager: &ConnectManager,
    predicate: impl Fn(&ConnectSnapshot) -> bool,
) -> ConnectSnapshot {
    bounded(async {
        loop {
            let snapshot = manager.snapshot("main").unwrap();
            if predicate(&snapshot) {
                return snapshot;
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
                return socket;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
}
struct Case {
    manager: Arc<ConnectManager>,
    prepared: ConnectPreparation,
    bind: SocketAddr,
    held: Option<std::net::UdpSocket>,
    game: SocketAddr,
}
impl Case {
    async fn new(fixture: &mut Fixture, profile: usize) -> Self {
        let discovery = DiscoveryManager::default();
        let selected = selection(&discovery, fixture).await;
        let game = selected.target.parse().unwrap();
        let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let bind = held.local_addr().unwrap();
        let manager = Arc::new(ConnectManager::default());
        let pending = match profile {
            0 => manager.reserve_host_discovered(
                "main",
                HostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: bind.to_string(),
                    game_target: selected.target,
                    lifetime_seconds: 30,
                },
                selected.selection_id,
                discovery,
            ),
            1 => manager.reserve_session_host_discovered(
                "main",
                SessionHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: bind.to_string(),
                    game_target: selected.target,
                    admission_seconds: 30,
                    session_seconds: 60,
                },
                selected.selection_id,
                discovery,
            ),
            2 => manager.reserve_session_fragmented_host_discovered(
                "main",
                SessionFragmentedDiscoveredHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: bind.to_string(),
                    udp_target: "127.0.0.1:43002".into(),
                    admission_seconds: 30,
                    session_seconds: 60,
                },
                selected.selection_id,
                discovery,
            ),
            _ => unreachable!(),
        }
        .unwrap();
        let prepared = bounded(pending.wait()).await.unwrap();
        Self {
            manager,
            prepared,
            bind,
            held: Some(held),
            game,
        }
    }
    fn reserve(&self, recipient: &str, hooks: ReviewHooks) -> PendingHostReview {
        self.manager
            .live
            .reserve_review("main", &self.prepared.operation_id, recipient, hooks)
            .unwrap()
    }
    fn reserve_random(&self, hooks: ReviewHooks) -> PendingHostReview {
        self.reserve(&lab::generate_key().public().to_string(), hooks)
    }
    async fn confirm(&mut self, review: &ConnectReview) {
        drop(self.held.take());
        self.manager
            .confirm("main", &self.prepared.operation_id, &review.digest)
            .await
            .unwrap();
    }
    fn stop_waiter(&self) -> JoinHandle<Result<ConnectSnapshot, ConnectError>> {
        let manager = self.manager.clone();
        let id = self.prepared.operation_id.clone();
        tokio::spawn(async move { manager.stop("main", &id).await })
    }
    async fn stopped(&self) {
        assert_eq!(
            bounded(self.manager.stop("main", &self.prepared.operation_id))
                .await
                .unwrap()
                .cleanup_state,
            "complete"
        );
    }
    fn deadlines(&self) -> (Instant, Instant) {
        let state = lock(&self.manager.live.state).unwrap();
        let reviewed = state.attempt.as_ref().unwrap().reviewed.as_ref().unwrap();
        (reviewed.admission_deadline, reviewed.deadline)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_review_all_profiles_pure_rejections_and_lost_receiver_keep_query_owned() {
    let mut fixture = Fixture::new();
    for profile in 0..3 {
        let case = Case::new(&mut fixture, profile).await;
        let baseline = case.manager.snapshot("main").unwrap();
        assert!(matches!(
            case.manager
                .reserve_host_review("main", &case.prepared.operation_id, "invalid"),
            Err(ConnectError::InvalidRequest)
        ));
        assert!(matches!(
            case.manager.reserve_host_review(
                "other",
                &case.prepared.operation_id,
                &lab::generate_key().public().to_string()
            ),
            Err(ConnectError::OwnerMismatch)
        ));
        assert!(matches!(
            case.manager.reserve_host_review(
                "main",
                &case.prepared.operation_id,
                &case.prepared.public_id
            ),
            Err(ConnectError::InvitationRejected)
        ));
        let current = case.manager.snapshot("main").unwrap();
        assert_eq!(current.generation, baseline.generation);
        assert_eq!(current.phase, "hostPrepared");
        {
            let state = lock(&case.manager.live.state).unwrap();
            let a = state.attempt.as_ref().unwrap();
            assert!(a.host_review.is_none() && a.draft.is_some());
        }
        let gate = Arc::new(SourceGate::default());
        let pending = case.reserve_random(ReviewHooks {
            query: Some(gate.clone()),
            ..Default::default()
        });
        bounded(gate.entered.notified()).await;
        drop(pending);
        let current = case.manager.snapshot("main").unwrap();
        assert_eq!(current.generation, baseline.generation);
        assert_eq!(current.phase, "hostPrepared");
        assert!(current.review.is_none());
        assert!(matches!(
            case.manager.reserve_host_review(
                "main",
                &case.prepared.operation_id,
                &lab::generate_key().public().to_string()
            ),
            Err(ConnectError::Busy)
        ));
        let waiter = case.stop_waiter();
        let closing = until(&case.manager, |s| s.phase == "closed").await;
        assert_eq!(closing.cleanup_state, "pending");
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        assert_eq!(
            case.manager.snapshot("main").unwrap().cleanup_state,
            "pending"
        );
        gate.release();
        case.stopped().await;
        let state = lock(&case.manager.live.state).unwrap();
        let a = state.attempt.as_ref().unwrap();
        assert!(a.authority.is_none() && a.reviewed.is_none() && a.export.is_none());
    }
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_review_all_profiles_publication_tail_survives_fast_host_and_stop() {
    let mut fixture = Fixture::new();
    for profile in 0..3 {
        let mut case = Case::new(&mut fixture, profile).await;
        let query = Arc::new(SourceGate::default());
        let tail = Arc::new(SourceGate::default());
        let pending = case.reserve_random(ReviewHooks {
            query: Some(query.clone()),
            published: Some(tail.clone()),
            ..Default::default()
        });
        bounded(query.entered.notified()).await;
        let query_finished_at = lab::unix_millis();
        query.release();
        let review = bounded(pending.wait()).await.unwrap();
        bounded(tail.entered.notified()).await;
        assert_eq!(review.target_source, Some("ownedInstance"));
        assert!(review.expires_at_ms <= case.prepared.expires_at_ms);
        if let Some(timing) = &review.session_timing {
            assert!(timing.admission_expires_at_ms >= query_finished_at + 30_000);
        }
        assert!(matches!(
            case.manager.reserve_host_review(
                "main",
                &case.prepared.operation_id,
                &lab::generate_key().public().to_string()
            ),
            Err(ConnectError::Busy)
        ));
        case.confirm(&review).await;
        until(&case.manager, |s| s.phase == "hostListening").await;
        let waiter = case.stop_waiter();
        assert_eq!(
            until(&case.manager, |s| s.phase == "closed")
                .await
                .cleanup_state,
            "pending"
        );
        let _port = rebind(case.bind).await;
        assert!(!waiter.is_finished());
        tail.release();
        assert_eq!(
            bounded(waiter).await.unwrap().unwrap().cleanup_state,
            "complete"
        );
    }
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_review_panic_reply_keeps_terminal_attempt_and_cleanup_tail() {
    let mut fixture = Fixture::new();
    let case = Case::new(&mut fixture, 1).await;
    let tail = Arc::new(SourceGate::default());
    let pending = case.reserve_random(ReviewHooks {
        panic_query: true,
        published: Some(tail.clone()),
        ..Default::default()
    });
    assert!(matches!(
        bounded(pending.wait()).await,
        Err(ConnectError::Internal)
    ));
    bounded(tail.entered.notified()).await;
    let failed = case.manager.snapshot("main").unwrap();
    assert_eq!(
        failed.operation_id.as_deref(),
        Some(case.prepared.operation_id.as_str())
    );
    assert_eq!(failed.phase, "failed");
    assert_eq!(failed.cleanup_state, "pending");
    assert_eq!(failed.transport_state, "closed");
    assert_eq!(failed.error_code, Some("connect_internal"));
    assert!(failed.preparation.is_none() && failed.review.is_none());
    assert!(matches!(
        case.manager.prepare_host(
            "main",
            HostRequest {
                mode: ConnectMode::Loopback,
                bind_address: case.bind.to_string(),
                game_target: case.game.to_string(),
                lifetime_seconds: 30,
            }
        ),
        Err(ConnectError::Busy)
    ));
    tail.release();
    case.stopped().await;
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_review_replaced_source_fails_without_restoring_draft_or_authority() {
    let mut fixture = Fixture::new();
    let case = Case::new(&mut fixture, 2).await;
    let gate = Arc::new(SourceGate::default());
    let pending = case.reserve_random(ReviewHooks {
        query: Some(gate.clone()),
        ..Default::default()
    });
    bounded(gate.entered.notified()).await;
    fixture.stop();
    let _replacement = std::net::TcpListener::bind(case.game).unwrap();
    gate.release();
    assert!(matches!(
        bounded(pending.wait()).await,
        Err(ConnectError::SourceChanged)
    ));
    let failed = case.manager.snapshot("main").unwrap();
    assert_eq!(failed.phase, "failed");
    assert_eq!(failed.error_code, Some("connect_source_changed"));
    assert!(failed.preparation.is_none() && failed.review.is_none());
    case.stopped().await;
    let state = lock(&case.manager.live.state).unwrap();
    let a = state.attempt.as_ref().unwrap();
    assert!(a.authority.is_none() && a.draft.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_review_original_clocks_and_generation_are_rechecked_after_query() {
    let mut fixture = Fixture::new();
    for change in 0..3 {
        let case = Case::new(&mut fixture, 1).await;
        let gate = Arc::new(SourceGate::default());
        let pending = case.reserve_random(ReviewHooks {
            query: Some(gate.clone()),
            ..Default::default()
        });
        bounded(gate.entered.notified()).await;
        {
            let mut state = lock(&case.manager.live.state).unwrap();
            let a = state.attempt.as_mut().unwrap();
            match change {
                0 => a.prepared_deadline = Instant::now(),
                1 => {
                    a.snapshot.preparation.as_mut().unwrap().expires_at_ms = lab::unix_millis() - 1
                }
                _ => a.snapshot.generation += 1,
            }
        }
        if change < 2 {
            let expired = case.manager.snapshot("main").unwrap();
            assert_eq!(expired.phase, "closed");
            assert_eq!(expired.cleanup_state, "pending");
        }
        gate.release();
        let error = bounded(pending.wait()).await.unwrap_err();
        assert_eq!(
            error,
            if change < 2 {
                ConnectError::PlanExpired
            } else {
                ConnectError::StaleOperation
            }
        );
        case.stopped().await;
        let state = lock(&case.manager.live.state).unwrap();
        assert!(state.attempt.as_ref().unwrap().authority.is_none());
    }
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_review_shutdown_and_expiry_after_signing_cannot_publish_authority() {
    let mut fixture = Fixture::new();
    for shutdown in [false, true] {
        let case = Case::new(&mut fixture, 0).await;
        let gate = Arc::new(SourceGate::default());
        let pending = case.reserve_random(ReviewHooks {
            query: shutdown.then(|| gate.clone()),
            published: (!shutdown).then(|| gate.clone()),
            expire_after_issue: !shutdown,
            ..Default::default()
        });
        if shutdown {
            bounded(gate.entered.notified()).await;
            let manager = case.manager.clone();
            let waiter = tokio::spawn(async move { manager.shutdown_all().await });
            assert_eq!(
                until(&case.manager, |s| s.phase == "closed")
                    .await
                    .cleanup_state,
                "pending"
            );
            assert!(!waiter.is_finished());
            gate.release();
            assert!(matches!(
                bounded(pending.wait()).await,
                Err(ConnectError::Cancelled)
            ));
            bounded(waiter).await.unwrap();
        } else {
            assert!(matches!(
                bounded(pending.wait()).await,
                Err(ConnectError::PlanExpired)
            ));
            bounded(gate.entered.notified()).await;
            let expired = case.manager.snapshot("main").unwrap();
            assert_eq!(expired.phase, "closed");
            assert_eq!(expired.cleanup_state, "pending");
            assert!(expired.review.is_none());
            gate.release();
        }
        case.stopped().await;
        let state = lock(&case.manager.live.state).unwrap();
        assert!(state.attempt.as_ref().unwrap().authority.is_none());
    }
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_actor_before_bind_query_observes_stop_and_original_admission_deadline() {
    let mut fixture = Fixture::new();
    for (profile, expiry) in [(0, false), (0, true), (1, false), (1, true)] {
        let mut case = Case::new(&mut fixture, profile).await;
        let gate = Arc::new(SourceGate::default());
        let clock = Arc::new(Mutex::new(Instant::now()));
        let review = bounded(
            case.reserve_random(ReviewHooks {
                before_bind: Some(gate.clone()),
                monotonic_now: Some(clock.clone()),
                ..Default::default()
            })
            .wait(),
        )
        .await
        .unwrap();
        let (admission, _) = case.deadlines();
        case.confirm(&review).await;
        bounded(gate.entered.notified()).await;
        assert_eq!(case.manager.snapshot("main").unwrap().phase, "starting");
        let _port = std::net::UdpSocket::bind(case.bind).unwrap();
        let waiter = if expiry {
            *clock.lock().unwrap() = admission + Duration::from_millis(1);
            None
        } else {
            Some(case.stop_waiter())
        };
        assert_eq!(
            until(&case.manager, |s| s.phase == "closing")
                .await
                .cleanup_state,
            "pending"
        );
        if let Some(waiter) = waiter {
            waiter.abort();
            assert!(waiter.await.unwrap_err().is_cancelled());
        }
        assert!(case
            .manager
            .export_invitation("main", &case.prepared.operation_id)
            .is_err());
        gate.release();
        case.stopped().await;
        assert_eq!(
            case.manager.snapshot("main").unwrap().transport_state,
            "closed"
        );
    }
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_actor_after_bind_query_closes_socket_before_lost_stop_waiter_or_query_returns() {
    let mut fixture = Fixture::new();
    for expiry in [false, true] {
        let mut case = Case::new(&mut fixture, 1).await;
        let gate = Arc::new(SourceGate::default());
        let clock = Arc::new(Mutex::new(Instant::now()));
        let review = bounded(
            case.reserve_random(ReviewHooks {
                after_bind: Some(gate.clone()),
                monotonic_now: Some(clock.clone()),
                ..Default::default()
            })
            .wait(),
        )
        .await
        .unwrap();
        let (admission, _) = case.deadlines();
        case.confirm(&review).await;
        bounded(gate.entered.notified()).await;
        assert!(std::net::UdpSocket::bind(case.bind).is_err());
        assert!(case
            .manager
            .export_invitation("main", &case.prepared.operation_id)
            .is_err());
        let waiter = if expiry {
            *clock.lock().unwrap() = admission + Duration::from_millis(1);
            None
        } else {
            Some(case.stop_waiter())
        };
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
        assert_eq!(
            case.manager.snapshot("main").unwrap().cleanup_state,
            "pending"
        );
        gate.release();
        case.stopped().await;
    }
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owned_actor_monitor_keeps_consumed_session_then_closes_before_pending_query_returns() {
    let mut fixture = Fixture::new();
    let mut case = Case::new(&mut fixture, 1).await;
    let guest = ConnectManager::default();
    let gp = guest
        .prepare_session_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: case.prepared.public_id.clone(),
            },
        )
        .unwrap();
    let gate = Arc::new(SourceGate::default());
    let clock = Arc::new(Mutex::new(Instant::now()));
    let observation = Arc::new(host_review::MonitorObservation::default());
    let review = bounded(
        case.reserve(
            &gp.public_id,
            ReviewHooks {
                monitor: Some(gate.clone()),
                monotonic_now: Some(clock.clone()),
                monitor_observation: Some(observation.clone()),
                ..Default::default()
            },
        )
        .wait(),
    )
    .await
    .unwrap();
    let (admission, session) = case.deadlines();
    case.confirm(&review).await;
    until(&case.manager, |s| s.phase == "hostListening").await;
    let export = case
        .manager
        .export_invitation("main", &case.prepared.operation_id)
        .unwrap();
    let gr = guest
        .review_guest("main", &gp.operation_id, &export.invitation)
        .unwrap();
    guest
        .confirm("main", &gp.operation_id, &gr.digest)
        .await
        .unwrap();
    let ready = until(&guest, |s| s.phase == "guestConnected").await;
    let tcp: SocketAddr = ready.tcp_address.unwrap().parse().unwrap();
    assert_eq!(
        guest
            .probe_status("main", &gp.operation_id)
            .await
            .unwrap()
            .status_state,
        "responded"
    );
    bounded(gate.entered.notified()).await;
    *clock.lock().unwrap() = admission + Duration::from_millis(1);
    bounded(async {
        while *observation.latest.lock().unwrap()
            != Some((admission + Duration::from_millis(1), true))
        {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    // A real second request while the held query observes expired admission
    // proves that a consumed invitation retains its original session authority.
    let second = bounded(lab::probe_minecraft_status(lab::MinecraftStatusRequest {
        endpoint: tcp,
        handshake_host: "localhost".into(),
        handshake_port: 43001,
        protocol: 767,
        timeout: lab::MAX_MINECRAFT_STATUS_TIMEOUT,
    }))
    .await
    .unwrap();
    assert!(second.protocol_matches_requested);
    assert_eq!(
        case.manager.snapshot("main").unwrap().phase,
        "hostListening"
    );
    *clock.lock().unwrap() = session + Duration::from_millis(1);
    assert_eq!(
        until(&case.manager, |s| s.phase == "closing")
            .await
            .cleanup_state,
        "pending"
    );
    let _port = rebind(case.bind).await;
    until(&guest, |s| s.phase == "closed").await;
    assert!(tokio::net::TcpStream::connect(tcp).await.is_err());
    assert!(case
        .manager
        .export_invitation("main", &case.prepared.operation_id)
        .is_err());
    let waiter = case.stop_waiter();
    tokio::task::yield_now().await;
    assert!(!waiter.is_finished());
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    gate.release();
    case.stopped().await;
    assert_eq!(
        bounded(guest.stop("main", &gp.operation_id))
            .await
            .unwrap()
            .cleanup_state,
        "complete"
    );
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_actor_unconsumed_monitor_closes_on_admission_expiry_while_query_owned() {
    let mut fixture = Fixture::new();
    let mut case = Case::new(&mut fixture, 1).await;
    let gate = Arc::new(SourceGate::default());
    let clock = Arc::new(Mutex::new(Instant::now()));
    let review = bounded(
        case.reserve_random(ReviewHooks {
            monitor: Some(gate.clone()),
            monotonic_now: Some(clock.clone()),
            ..Default::default()
        })
        .wait(),
    )
    .await
    .unwrap();
    let (admission, _) = case.deadlines();
    case.confirm(&review).await;
    until(&case.manager, |s| s.phase == "hostListening").await;
    bounded(gate.entered.notified()).await;
    *clock.lock().unwrap() = admission + Duration::from_millis(1);
    assert_eq!(
        until(&case.manager, |s| s.phase == "closing")
            .await
            .cleanup_state,
        "pending"
    );
    let _port = rebind(case.bind).await;
    assert!(case
        .manager
        .export_invitation("main", &case.prepared.operation_id)
        .is_err());
    gate.release();
    case.stopped().await;
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owned_actor_final_query_is_off_mutex_and_retained_after_transport_closed() {
    let mut fixture = Fixture::new();
    let mut case = Case::new(&mut fixture, 0).await;
    let gate = Arc::new(SourceGate::default());
    let review = bounded(
        case.reserve_random(ReviewHooks {
            final_query: Some(gate.clone()),
            ..Default::default()
        })
        .wait(),
    )
    .await
    .unwrap();
    case.confirm(&review).await;
    until(&case.manager, |s| s.phase == "hostListening").await;
    let waiter = case.stop_waiter();
    bounded(gate.entered.notified()).await;
    let snapshot = case.manager.snapshot("main").unwrap();
    assert_eq!(snapshot.phase, "closing");
    assert_eq!(snapshot.cleanup_state, "pending");
    let _port = rebind(case.bind).await;
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    assert_eq!(
        case.manager.snapshot("main").unwrap().cleanup_state,
        "pending"
    );
    gate.release();
    case.stopped().await;
    fixture.stop();
}
