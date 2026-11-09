//! Local process/socket fixtures only; no Firebase credentials or Minecraft.
use super::*;
use crate::engine::connect_discovery::{
    tests::{expire_selection, selection, Fixture},
    DiscoveryManager,
};
use group_source::SourceGate;

async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(8), future)
        .await
        .expect("social source fixture deadline")
}
fn request() -> SocialDiscoveredReserveRequest {
    SocialDiscoveredReserveRequest {
        account_epoch: 0,
        account_id: "localTestAccount".into(),
        session_id: uuid::Uuid::new_v4().to_string(),
        visibility: "public".into(),
    }
}
fn reserve_run(
    manager: &Manager,
    discovery: &DiscoveryManager,
    selection: &str,
) -> (SocialReservation, Run) {
    let reserved = manager
        .social_reserve_discovered("main", request(), selection.into(), discovery.clone())
        .unwrap();
    let mut state = manager.state.lock().unwrap();
    let a = state.attempt.as_mut().unwrap();
    let control = a.social.as_mut().unwrap();
    let run = Run {
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        native_voice_source: None,
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        native_voice_local_key: None,
        state: manager.state.clone(),
        id: reserved.operation_id.clone(),
        generation: reserved.generation,
        request: control.request.clone(),
        selection: control.selection.take(),
        key: control.key.take().unwrap(),
        authority: manager.social_authority.clone().unwrap(),
        clock: None,
        deadline: a.prepared_deadline,
        expiry: a.snapshot.social.as_ref().unwrap().expires_at_ms.unwrap(),
        cancel: a.cancel.clone(),
        source_gate: None,
        guard_probe: None,
    };
    (reserved, run)
}

#[test]
fn social_discovered_request_is_closed_and_default_off() {
    let mut value = json!({"accountEpoch":0,"accountId":"localTestAccount","sessionId":uuid::Uuid::new_v4().to_string(),"visibility":"public"});
    assert!(serde_json::from_value::<SocialDiscoveredReserveRequest>(value.clone()).is_ok());
    for name in ["gameTarget", "pid", "authorityPin", "role"] {
        value[name] = json!("untrusted");
        assert!(serde_json::from_value::<SocialDiscoveredReserveRequest>(value.clone()).is_err());
        value.as_object_mut().unwrap().remove(name);
    }
    let manager = Manager::default();
    assert_eq!(
        manager.social_capabilities().owned_game_tcp_discovery,
        Some(false)
    );
    assert_eq!(
        manager
            .social_reserve_discovered(
                "main",
                request(),
                uuid::Uuid::new_v4().to_string(),
                DiscoveryManager::default()
            )
            .unwrap_err(),
        ConnectError::Disabled
    );
    let manager = tests::manager();
    assert!(manager
        .social_reserve_discovered(
            "other",
            request(),
            uuid::Uuid::new_v4().to_string(),
            DiscoveryManager::default()
        )
        .is_err());
    let mut stale = request();
    stale.account_epoch = 1;
    assert_eq!(
        manager
            .social_reserve_discovered(
                "main",
                stale,
                uuid::Uuid::new_v4().to_string(),
                DiscoveryManager::default()
            )
            .unwrap_err(),
        ConnectError::StaleOperation
    );
    assert!(manager.state.lock().unwrap().attempt.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn social_discovered_port_is_native_single_use_and_replacement_is_refused() {
    let manager = tests::manager();
    let discovery = DiscoveryManager::default();
    let mut fixture = Fixture::new();
    let selected = selection(&discovery, &mut fixture).await;
    let (_, run) = reserve_run(&manager, &discovery, &selected.selection_id);
    let (guard, target) = discovered_source(&run).await.unwrap();
    assert_eq!(target.to_string(), selected.target);
    assert!(matches!(guard, TargetGuard::Owned(_)));
    assert!(discovery
        .take_target("main", &selected.selection_id)
        .is_err());
    assert_eq!(guard.recheck().unwrap(), Some(target));
    fixture.stop();
    let replacement = std::net::TcpListener::bind(target).unwrap();
    replacement.set_nonblocking(true).unwrap();
    assert!(matches!(
        check_source(&run, &guard, &mut None, None, || Ok(())).await,
        Err(ConnectError::SourceChanged)
    ));
    assert!(
        matches!(replacement.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn social_discovered_selection_keeps_original_expiry() {
    let manager = tests::manager();
    let discovery = DiscoveryManager::default();
    let mut fixture = Fixture::new();
    let selected = selection(&discovery, &mut fixture).await;
    let (_, run) = reserve_run(&manager, &discovery, &selected.selection_id);
    expire_selection(&discovery);
    assert!(matches!(
        discovered_source(&run).await,
        Err(ConnectError::SourceChanged)
    ));
    assert!(manager
        .snapshot("main")
        .unwrap()
        .social
        .unwrap()
        .game_target
        .is_none());
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn social_discovered_query_is_reserved_and_stop_waits_for_owned_work() {
    let manager = tests::manager();
    let discovery = DiscoveryManager::default();
    let mut fixture = Fixture::new();
    let selected = selection(&discovery, &mut fixture).await;
    let (reserved, mut run) = reserve_run(&manager, &discovery, &selected.selection_id);
    let gate = Arc::new(SourceGate::default());
    run.source_gate = Some(gate.clone());
    let state = manager.state.clone();
    let id = reserved.operation_id.clone();
    let cancel = run.cancel.clone();
    manager
        .state
        .lock()
        .unwrap()
        .attempt
        .as_mut()
        .unwrap()
        .worker = Some(tokio::spawn(async move {
        let result = discovered_source(&run).await.map(|_| ());
        close_operation(&state, &id, &cancel, execution_error(result)).await;
    }));
    bounded(gate.entered.notified()).await;
    let observed = manager.snapshot("main").unwrap();
    assert_eq!(observed.operation_id, Some(reserved.operation_id.clone()));
    assert_eq!(observed.social.unwrap().stage, "sourceChecking");
    assert!(
        observed.preparation.is_none()
            && observed.review.is_none()
            && observed.tcp_address.is_none()
    );
    assert_eq!(
        manager
            .social_reserve_discovered(
                "main",
                request(),
                uuid::Uuid::new_v4().to_string(),
                discovery.clone()
            )
            .unwrap_err(),
        ConnectError::Busy
    );
    assert!(tokio::time::timeout(
        Duration::from_millis(50),
        manager.stop("main", &reserved.operation_id)
    )
    .await
    .is_err());
    let closing = manager.snapshot("main").unwrap();
    assert_eq!(closing.cleanup_state, "pending");
    gate.release();
    let closed = bounded(manager.stop("main", &reserved.operation_id))
        .await
        .unwrap();
    assert_eq!(closed.cleanup_state, "complete");
    assert!(closed.social.unwrap().game_target.is_none());
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn social_source_cancel_releases_real_host_before_blocking_query_finishes() {
    let manager = tests::manager();
    let discovery = DiscoveryManager::default();
    let (_, run) = reserve_run(&manager, &discovery, &uuid::Uuid::new_v4().to_string());
    let cancel = run.cancel.clone();
    let host = lab::Host::start(lab::HostConfig {
        secret_key: lab::generate_key(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        session_id: uuid::Uuid::new_v4().to_string(),
        services: vec![game("127.0.0.1:1".parse().unwrap())],
        leases: vec![lab::Lease {
            lease_id: "social-source-cleanup".into(),
            peer: lab::generate_key().public(),
            expires_at_unix_ms: lab::unix_millis() + 30_000,
            services: ["game".into()].into_iter().collect(),
        }],
        limits: limits(),
    })
    .await
    .unwrap();
    let sockets = host.bound_sockets();
    let gate = Arc::new(SourceGate::default());
    let held = gate.clone();
    let mut worker = tokio::spawn(async move {
        source_query(
            &run,
            &mut Some(host),
            None,
            || Ok(()),
            move || {
                held.wait();
                Ok(())
            },
        )
        .await
    });
    bounded(gate.entered.notified()).await;
    cancel.cancel();
    bounded(async {
        loop {
            if sockets
                .iter()
                .all(|address| std::net::UdpSocket::bind(address).is_ok())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        !worker.is_finished(),
        "source query remains owned after endpoint release"
    );
    gate.release();
    assert_eq!(
        bounded(&mut worker).await.unwrap(),
        Err(ConnectError::Cancelled)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn social_discovered_source_serves_status_through_authenticated_game_bridge() {
    let manager = tests::manager();
    let discovery = DiscoveryManager::default();
    let mut fixture = Fixture::new();
    let selected = selection(&discovery, &mut fixture).await;
    let (_, run) = reserve_run(&manager, &discovery, &selected.selection_id);
    let (guard, target) = discovered_source(&run).await.unwrap();
    let host_key = lab::generate_key();
    let guest_key = lab::generate_key();
    let reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = reservation.local_addr().unwrap();
    let session = uuid::Uuid::new_v4().to_string();
    let service = game(target);
    let now = lab::unix_millis();
    let invitation = lab::SessionInvitation::issue_loopback(
        &host_key,
        guest_key.public(),
        &session,
        bind,
        std::slice::from_ref(&service),
        lab::SessionDeadlines {
            admission_expires_at_unix_ms: now + 10_000,
            session_expires_at_unix_ms: now + 20_000,
        },
    )
    .unwrap();
    check_source(&run, &guard, &mut None, None, || Ok(()))
        .await
        .unwrap();
    drop(reservation);
    let host = lab::Host::start_invited_session(
        lab::HostConfig {
            secret_key: host_key,
            bind_addr: bind,
            session_id: session,
            services: vec![service],
            leases: vec![invitation.lease()],
            limits: limits(),
        },
        &invitation,
    )
    .await
    .unwrap();
    let guest = lab::Guest::connect_invited_session(guest_key, &invitation, limits())
        .await
        .unwrap();
    let bridge = guest
        .tcp_bridge("game", 1, "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let result = lab::probe_minecraft_status(lab::MinecraftStatusRequest {
        endpoint: bridge.local_addr(),
        handshake_host: "localhost".into(),
        handshake_port: target.port(),
        protocol: 767,
        timeout: Duration::from_secs(3),
    })
    .await
    .unwrap();
    assert_eq!(result.reported_protocol, 767);
    assert!(result.protocol_matches_requested);
    guest.shutdown().await;
    bridge.shutdown().await;
    drop(bridge);
    drop(guest);
    host.revoke(invitation.invitation_id());
    let sockets = host.bound_sockets();
    host.shutdown().await;
    drop(host);
    release(&sockets, None).await.unwrap();
    fixture.stop();
}
