//! Real local transports. A path observation never grants a game or voice service.
use super::*;
use tokio::time::{sleep, timeout};

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    timeout(Duration::from_secs(8), future)
        .await
        .expect("desktop path fixture deadline")
}
async fn until(
    manager: &ConnectManager,
    accept: impl Fn(&ConnectSnapshot) -> bool,
) -> ConnectSnapshot {
    bounded(async {
        loop {
            let snapshot = manager.snapshot("main").unwrap();
            if accept(&snapshot) {
                break snapshot;
            }
            assert_ne!(snapshot.phase, "failed");
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
}
fn direct(value: Option<ConnectPathObservation>) -> bool {
    value.is_some_and(|value| {
        value.schema_version == 2
            && value.state == ConnectPathState::Direct
            && value.rtt_estimate_micros.is_some_and(|micros| micros > 0)
    })
}
fn absent(snapshot: &ConnectSnapshot) {
    let value = serde_json::to_value(snapshot).unwrap();
    assert!(value.get("pathObservation").is_none());
    if let Some(group) = value.get("hostGroup") {
        for participant in group["participants"].as_array().unwrap() {
            assert!(participant.get("pathObservation").is_none());
        }
    }
    assert_eq!(snapshot.path_state, "notVerified");
    assert!(!snapshot.relay_enabled);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn desktop_path_projection_individual_replies_sample_original_transport_and_stop_releases_it()
{
    let (game, backend) = crate::engine::connect::tests::status_fixture().await;
    let held = std::net::UdpSocket::bind(local()).unwrap();
    let host = ConnectManager::default();
    let guest = ConnectManager::default();
    let hp = host
        .prepare_session_host(
            "main",
            SessionHostRequest {
                mode: ConnectMode::Loopback,
                bind_address: held.local_addr().unwrap().to_string(),
                game_target: game.to_string(),
                admission_seconds: 30,
                session_seconds: 60,
            },
        )
        .unwrap();
    absent(&host.snapshot("main").unwrap());
    let gp = guest
        .prepare_session_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: hp.public_id,
            },
        )
        .unwrap();
    absent(&guest.snapshot("main").unwrap());
    let review = host
        .reserve_host_review("main", &hp.operation_id, &gp.public_id)
        .unwrap()
        .wait()
        .await
        .unwrap();
    let review_bytes = serde_json::to_vec(&review).unwrap();
    assert!(serde_json::to_value(&review)
        .unwrap()
        .get("pathObservation")
        .is_none());
    drop(held);
    absent(
        &host
            .confirm("main", &hp.operation_id, &review.digest)
            .await
            .unwrap(),
    );
    let listening = until(&host, |s| s.phase == "hostListening").await;
    absent(&listening); // An unconsumed invitation has no observation.
    let export = host.export_invitation("main", &hp.operation_id).unwrap();
    assert!(serde_json::to_value(&export)
        .unwrap()
        .get("pathObservation")
        .is_none());
    let reviewed = guest
        .review_guest("main", &gp.operation_id, &export.invitation)
        .unwrap();
    assert!(serde_json::to_value(&reviewed)
        .unwrap()
        .get("pathObservation")
        .is_none());
    absent(
        &guest
            .confirm("main", &gp.operation_id, &reviewed.digest)
            .await
            .unwrap(),
    );
    // Observe phase under the lock without sampling Manager::snapshot first.
    bounded(async {
        loop {
            let connected = guest
                .live
                .state
                .lock()
                .unwrap()
                .attempt
                .as_ref()
                .is_some_and(|a| a.snapshot.phase == "guestConnected");
            if connected {
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    let ports = {
        let state = guest.live.state.lock().unwrap();
        let a = state.attempt.as_ref().unwrap();
        assert!(a.snapshot.path_observation.is_none());
        a.guest_source.as_ref().unwrap().bound_sockets_for_test()
    };
    assert!(!ports.is_empty());
    // The command reply itself must sample; a cached snapshot would omit it.
    let checked = guest.probe_status("main", &gp.operation_id).await.unwrap();
    assert_eq!(checked.status_state, "responded");
    assert!(direct(checked.path_observation));
    let host_observed = until(&host, |s| {
        s.transport_state == "admitted" && direct(s.path_observation)
    })
    .await;
    let same_host = host.snapshot("main").unwrap();
    assert_eq!(same_host.generation, host_observed.generation);
    let first = guest.snapshot("main").unwrap();
    let same = guest.snapshot("main").unwrap();
    assert_eq!(first.generation, same.generation);
    assert!(direct(first.path_observation) && direct(same.path_observation));
    // A new RTT estimate is not a new authorization/review generation.
    {
        let mut state = guest.live.state.lock().unwrap();
        let a = state.attempt.as_mut().unwrap();
        a.snapshot
            .path_observation
            .as_mut()
            .unwrap()
            .rtt_estimate_micros = Some(u32::MAX);
    }
    let estimate_updated = guest.snapshot("main").unwrap();
    assert!(direct(estimate_updated.path_observation));
    assert_ne!(
        estimate_updated
            .path_observation
            .unwrap()
            .rtt_estimate_micros,
        Some(u32::MAX)
    );
    assert_eq!(estimate_updated.generation, same.generation);
    // Closing the application bridge does not revoke the authenticated path.
    // In particular, observation must not call the QuickPlay address() gate.
    let bridge = {
        let state = guest.live.state.lock().unwrap();
        state
            .attempt
            .as_ref()
            .unwrap()
            .guest_source
            .as_ref()
            .unwrap()
            .bridge
            .upgrade()
            .unwrap()
    };
    let tcp = bridge.local_addr();
    bridge.shutdown().await;
    drop(bridge);
    assert!(direct(guest.snapshot("main").unwrap().path_observation));
    let stopped = bounded(guest.stop("main", &gp.operation_id)).await.unwrap();
    absent(&stopped);
    // Repeated sampling must not retain the original Guest/Endpoint in State.
    let rebound = bounded(async {
        loop {
            if let Ok(sockets) = ports
                .iter()
                .map(std::net::UdpSocket::bind)
                .collect::<std::io::Result<Vec<_>>>()
            {
                break sockets;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    assert!(std::net::TcpListener::bind(tcp).is_ok());
    assert_eq!(serde_json::to_vec(&review).unwrap(), review_bytes);
    drop(rebound);
    absent(&bounded(host.stop("main", &hp.operation_id)).await.unwrap());
    backend.abort();
    let _ = backend.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn desktop_path_projection_group_observes_each_original_lease_and_preserves_other_member() {
    let (game, backend) = crate::engine::connect::tests::status_fixture().await;
    let held = std::net::UdpSocket::bind(local()).unwrap();
    let bind = held.local_addr().unwrap();
    let aux = std::net::UdpSocket::bind(local()).unwrap();
    let host = ConnectManager::default();
    let prepared = host
        .prepare_session_group_host(
            "main",
            GroupHostRequest {
                mode: ConnectMode::Loopback,
                bind_address: bind.to_string(),
                game_target: game.to_string(),
                udp_target: aux.local_addr().unwrap().to_string(),
            },
        )
        .unwrap();
    absent(&host.snapshot("main").unwrap());
    let keys = [lab::generate_key(), lab::generate_key()];
    let review = host
        .review_session_group_host(
            "main",
            &prepared.operation_id,
            keys.iter()
                .map(|key| GroupParticipantRequest {
                    guest_public_id: key.public().to_string(),
                    admission_seconds: 2,
                    session_seconds: 6,
                })
                .collect(),
        )
        .unwrap();
    let reviewed_bytes = serde_json::to_vec(&review).unwrap();
    assert!(review
        .participants
        .iter()
        .all(|p| p.path_observation.is_none()));
    absent(&host.snapshot("main").unwrap());
    drop(held);
    absent(
        &host
            .confirm_session_group_host("main", &prepared.operation_id, &review.digest)
            .await
            .unwrap(),
    );
    absent(&until(&host, |s| s.phase == "hostListening").await);
    let mut guests = Vec::new();
    let mut bridges = Vec::new();
    for (index, key) in keys.iter().enumerate() {
        let export = host
            .export_session_group_invitation(
                "main",
                &prepared.operation_id,
                &review.participants[index].participant_id,
            )
            .unwrap();
        assert!(serde_json::to_value(&export)
            .unwrap()
            .get("pathObservation")
            .is_none());
        let invitation = lab::SessionInvitation::verify_fragmented_loopback(
            &export.invitation,
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
        bridges.push(bounded(guest.tcp_bridge("game", 1, local())).await.unwrap());
        guests.push(guest);
    }
    for bridge in &bridges {
        let response = bounded(lab::probe_minecraft_status(lab::MinecraftStatusRequest {
            endpoint: bridge.local_addr(),
            handshake_host: "localhost".into(),
            handshake_port: game.port(),
            protocol: 767,
            timeout: Duration::from_secs(3),
        }))
        .await
        .unwrap();
        assert!(response.protocol_matches_requested);
    }
    let current = until(&host, |s| {
        s.host_group
            .as_ref()
            .unwrap()
            .participants
            .iter()
            .all(|p| direct(p.path_observation) && p.consumed)
    })
    .await;
    assert!(current.path_observation.is_none());
    let stable = host.snapshot("main").unwrap();
    assert_eq!(stable.generation, current.generation);
    {
        let mut state = host.live.state.lock().unwrap();
        let a = state.attempt.as_mut().unwrap();
        for participant in &mut a.snapshot.host_group.as_mut().unwrap().participants {
            participant
                .path_observation
                .as_mut()
                .unwrap()
                .rtt_estimate_micros = Some(u32::MAX);
        }
    }
    let estimates_updated = host.snapshot("main").unwrap();
    assert_eq!(estimates_updated.generation, stable.generation);
    assert!(estimates_updated
        .host_group
        .as_ref()
        .unwrap()
        .participants
        .iter()
        .all(|p| direct(p.path_observation)
            && p.path_observation.unwrap().rtt_estimate_micros != Some(u32::MAX)));
    let expired_ingress = until(&host, |s| {
        s.host_group
            .as_ref()
            .unwrap()
            .participants
            .iter()
            .all(|p| !p.admission_valid)
    })
    .await;
    assert_eq!(expired_ingress.phase, "hostListening");
    assert!(expired_ingress
        .host_group
        .as_ref()
        .unwrap()
        .participants
        .iter()
        .all(|p| p.consumed && p.session_valid && direct(p.path_observation)));
    let revoked = host
        .revoke_session_group_participant(
            "main",
            &prepared.operation_id,
            &review.participants[0].participant_id,
            expired_ingress.generation,
        )
        .unwrap();
    assert!(revoked.path_observation.is_none());
    let rows = revoked.host_group.as_ref().unwrap().participants.clone();
    assert!(rows[0].revoked && rows[0].path_observation.is_none());
    assert!(direct(rows[1].path_observation));
    assert_eq!(
        rows[1].session_timing,
        review.participants[1].session_timing
    );
    assert_eq!(
        rows[1].invitation_digest,
        review.participants[1].invitation_digest
    );
    let duplicate = host
        .revoke_session_group_participant(
            "main",
            &prepared.operation_id,
            &review.participants[0].participant_id,
            0,
        )
        .unwrap();
    assert!(direct(
        duplicate.host_group.as_ref().unwrap().participants[1].path_observation
    ));
    let response = bounded(lab::probe_minecraft_status(lab::MinecraftStatusRequest {
        endpoint: bridges[1].local_addr(),
        handshake_host: "localhost".into(),
        handshake_port: game.port(),
        protocol: 767,
        timeout: Duration::from_secs(3),
    }))
    .await
    .unwrap();
    assert!(response.protocol_matches_requested);
    guests[1].shutdown().await;
    let peer_closed = until(&host, |s| {
        s.host_group.as_ref().unwrap().participants[1]
            .path_observation
            .is_some_and(|p| p.state == ConnectPathState::Closed)
    })
    .await;
    assert!(!peer_closed.host_group.as_ref().unwrap().participants[1].transport_active);
    assert!(peer_closed.host_group.as_ref().unwrap().participants[1]
        .path_observation
        .unwrap()
        .rtt_estimate_micros
        .is_none());
    let stopped = bounded(host.stop("main", &prepared.operation_id))
        .await
        .unwrap();
    absent(&stopped);
    assert_eq!(serde_json::to_vec(&review).unwrap(), reviewed_bytes);
    let _rebound = std::net::UdpSocket::bind(bind).unwrap();
    for bridge in &bridges {
        bounded(bridge.shutdown()).await;
    }
    for guest in &guests {
        bounded(guest.shutdown()).await;
    }
    backend.abort();
    let _ = backend.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn desktop_path_projection_udp_reply_samples_after_generation_gate_and_keeps_idempotence() {
    use super::fragmented_profile::{UdpState, LOCAL_UDP_BIND};
    use super::session_fragmented_tests::{transfer, Echo, Pair};
    use tokio::net::{TcpStream, UdpSocket};
    let occupied = UdpSocket::bind(LOCAL_UDP_BIND).await.unwrap();
    let echo = Echo::new().await;
    let endpoint = UdpSocket::bind(local()).await.unwrap();
    let pair = Pair::new(echo.address, endpoint.local_addr().unwrap(), 30, 60).await;
    let failed = pair.udp(UdpState::Failed).await;
    assert!(direct(failed.path_observation));
    let mut tcp = TcpStream::connect(pair.tcp).await.unwrap();
    transfer(&mut tcp).await;
    drop(occupied);
    let gate = Arc::new(udp_recovery::TestGate::default());
    {
        let mut state = pair.guest.live.state.lock().unwrap();
        let a = state.attempt.as_mut().unwrap();
        a.recovery_gate = Some(gate.clone());
        // Only the output projection is cold. Authorities and generation are
        // unchanged, forcing the command reply to refresh after its input gate.
        a.snapshot.path_observation = None;
    }
    let starting = pair
        .guest
        .retry_session_udp("main", &pair.guest_id, failed.generation)
        .unwrap();
    assert!(direct(starting.path_observation));
    bounded(gate.entered.notified()).await;
    let duplicate = pair
        .guest
        .retry_session_udp("main", &pair.guest_id, failed.generation)
        .unwrap();
    assert_eq!(duplicate.generation, starting.generation);
    assert!(direct(duplicate.path_observation));
    assert_eq!(duplicate.udp_recovery.as_ref().unwrap().attempts_used, 1);
    gate.release.notify_one();
    let ready = pair.udp(UdpState::Available).await;
    assert!(direct(ready.path_observation));
    assert_eq!(ready.session_timing.as_ref(), Some(&pair.timing));
    transfer(&mut tcp).await;
    pair.stop().await;
    absent(&pair.guest.snapshot("main").unwrap());
    absent(&pair.host.snapshot("main").unwrap());
    echo.stop().await;
}

#[tokio::test]
async fn desktop_path_projection_cancelled_group_removes_observation_once_before_actor_runs() {
    // A current-thread runtime is deliberate: while this test holds State
    // without yielding, interrupt() cannot revoke Host leases outside State
    // either. Cancellation therefore precedes both synchronous projections.
    let game = std::net::TcpListener::bind(local()).unwrap();
    let aux = std::net::UdpSocket::bind(local()).unwrap();
    let held = std::net::UdpSocket::bind(local()).unwrap();
    let bind = held.local_addr().unwrap();
    let host = ConnectManager::default();
    let prepared = host
        .prepare_session_group_host(
            "main",
            GroupHostRequest {
                mode: ConnectMode::Loopback,
                bind_address: bind.to_string(),
                game_target: game.local_addr().unwrap().to_string(),
                udp_target: aux.local_addr().unwrap().to_string(),
            },
        )
        .unwrap();
    let keys = [lab::generate_key(), lab::generate_key()];
    let review = host
        .review_session_group_host(
            "main",
            &prepared.operation_id,
            keys.iter()
                .map(|key| GroupParticipantRequest {
                    guest_public_id: key.public().to_string(),
                    admission_seconds: 30,
                    session_seconds: 60,
                })
                .collect(),
        )
        .unwrap();
    drop(held);
    host.confirm_session_group_host("main", &prepared.operation_id, &review.digest)
        .await
        .unwrap();
    until(&host, |s| s.phase == "hostListening").await;
    let mut guests = Vec::new();
    for (index, key) in keys.iter().enumerate() {
        let export = host
            .export_session_group_invitation(
                "main",
                &prepared.operation_id,
                &review.participants[index].participant_id,
            )
            .unwrap();
        let invitation = lab::SessionInvitation::verify_fragmented_loopback(
            &export.invitation,
            prepared.public_id.parse().unwrap(),
            key.public(),
        )
        .unwrap();
        guests.push(
            bounded(lab::Guest::connect_invited_fragmented_session(
                key.clone(),
                &invitation,
                lab::Limits::default(),
            ))
            .await
            .unwrap(),
        );
    }
    let sampled = until(&host, |s| {
        s.host_group
            .as_ref()
            .unwrap()
            .participants
            .iter()
            .all(|p| p.consumed && p.transport_active && direct(p.path_observation))
    })
    .await;
    {
        let mut state = host.live.state.lock().unwrap();
        let a = state.attempt.as_mut().unwrap();
        a.cancel.cancel(); // The actual operation token, not a fabricated grant.
        let first = path_projection::snapshot(a);
        let second = path_projection::snapshot(a);
        absent(&first);
        absent(&second);
        assert_eq!(first.phase, "hostListening");
        assert_eq!(second.phase, "hostListening");
        assert!(first.generation > sampled.generation);
        assert_eq!(second.generation, first.generation);
        assert!(second
            .host_group
            .as_ref()
            .unwrap()
            .participants
            .iter()
            .all(|p| p.consumed && !p.revoked));
    }
    absent(
        &bounded(host.stop("main", &prepared.operation_id))
            .await
            .unwrap(),
    );
    let _rebound = std::net::UdpSocket::bind(bind).unwrap();
    for guest in &guests {
        bounded(guest.shutdown()).await;
    }
}
