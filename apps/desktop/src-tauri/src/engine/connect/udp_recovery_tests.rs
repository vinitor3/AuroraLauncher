//! Real bounded retry on one admitted session. Fixed UDP port requires serial tests.
use super::fragmented_profile::{UdpState, LOCAL_UDP_BIND};
use super::session_fragmented_tests::{datagram, eof, transfer, until, Echo, Pair};
use super::*;
use serde_json::json;
use tokio::{
    net::{TcpStream, UdpSocket},
    time::{sleep, timeout},
};

fn recovery(snapshot: &ConnectSnapshot) -> serde_json::Value {
    serde_json::to_value(snapshot).unwrap()["udpRecovery"].clone()
}
fn arm(pair: &Pair) -> Arc<udp_recovery::TestGate> {
    let gate = Arc::new(udp_recovery::TestGate::default());
    let mut state = pair.guest.live.state.lock().unwrap();
    selected(&mut state, "main", &pair.guest_id)
        .unwrap()
        .recovery_gate = Some(gate.clone());
    gate
}
async fn entered(gate: &udp_recovery::TestGate) {
    timeout(Duration::from_secs(3), gate.entered.notified())
        .await
        .unwrap();
}
fn retry(pair: &Pair, generation: u64) -> ConnectSnapshot {
    pair.guest
        .retry_session_udp("main", &pair.guest_id, generation)
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_recovery_after_admission_preserves_tcp_original_deadline_and_two_sources() {
    let occupied = UdpSocket::bind(LOCAL_UDP_BIND).await.unwrap();
    let echo = Echo::new().await;
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let pair = Pair::new(echo.address, endpoint.local_addr().unwrap(), 2, 20).await;
    let failed = pair.udp(UdpState::Failed).await;
    assert_eq!(
        recovery(&failed),
        json!({"attemptsUsed":0,"maxAttempts":3,"pending":false})
    );
    assert!(serde_json::to_value(pair.host.snapshot("main").unwrap())
        .unwrap()
        .get("udpRecovery")
        .is_none());
    let mut tcp = TcpStream::connect(pair.tcp).await.unwrap();
    transfer(&mut tcp).await;
    drop(occupied);
    pair.after_ingress().await;
    sleep(Duration::from_millis(120)).await;
    assert_eq!(
        recovery(&pair.udp(UdpState::Failed).await)["attemptsUsed"],
        0
    );
    let gate = arm(&pair);
    let starting = retry(&pair, failed.generation);
    assert_eq!(
        recovery(&starting),
        json!({"attemptsUsed":1,"maxAttempts":3,"pending":true})
    );
    assert_eq!(starting.udp.as_ref().unwrap().udp_state, UdpState::Starting);
    entered(&gate).await;
    assert!(
        std::net::UdpSocket::bind(LOCAL_UDP_BIND).is_err(),
        "real successful bind is retained"
    );
    let duplicate = retry(&pair, failed.generation);
    assert_eq!(duplicate.generation, starting.generation);
    // An independent STATUS changes the snapshot generation while setup waits.
    let checked = pair
        .guest
        .probe_status("main", &pair.guest_id)
        .await
        .unwrap();
    assert!(checked.generation > starting.generation);
    assert_eq!(recovery(&checked)["attemptsUsed"], 1);
    gate.release.notify_one();
    let ready = pair.udp(UdpState::Available).await;
    assert_eq!(
        recovery(&ready),
        json!({"attemptsUsed":1,"maxAttempts":3,"pending":false})
    );
    assert_eq!(ready.session_timing.as_ref(), Some(&pair.timing));
    assert_eq!(ready.expires_at_ms, Some(pair.timing.session_expires_at_ms));
    assert_eq!(ready.operation_id.as_deref(), Some(pair.guest_id.as_str()));
    assert_eq!(
        ready.tcp_address.as_deref(),
        Some(pair.tcp.to_string().as_str())
    );
    assert_eq!(retry(&pair, failed.generation).generation, ready.generation);
    assert!(matches!(
        pair.guest
            .retry_session_udp("main", &pair.guest_id, ready.generation),
        Err(ConnectError::StaleOperation)
    ));
    let first = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let second = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let a = datagram(&first, &endpoint, 4096, 1).await;
    let b = datagram(&second, &endpoint, 4096, 2).await;
    assert_ne!(a, b);
    assert!(
        timeout(Duration::from_millis(80), first.recv(&mut [0; 4096]))
            .await
            .is_err(),
        "the second source's response must not also reach the first source"
    );
    transfer(&mut tcp).await;
    pair.stop().await;
    eof(&mut tcp).await;
    assert_eq!(
        recovery(&pair.guest.snapshot("main").unwrap())["pending"],
        false
    );
    drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
    echo.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_additional_attempts_bound_failure_double_click_and_stale_requests() {
    let occupied = UdpSocket::bind(LOCAL_UDP_BIND).await.unwrap();
    let echo = Echo::new().await;
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let pair = Pair::new(echo.address, endpoint.local_addr().unwrap(), 2, 20).await;
    let mut failed = pair.udp(UdpState::Failed).await;
    let mut tcp = TcpStream::connect(pair.tcp).await.unwrap();
    for attempt in 1..=3 {
        let gate = arm(&pair);
        let source_generation = failed.generation;
        let (one, two) = tokio::join!(async { retry(&pair, source_generation) }, async {
            retry(&pair, source_generation)
        },);
        assert_eq!(one.generation, two.generation);
        assert_eq!(recovery(&one)["attemptsUsed"], attempt);
        entered(&gate).await;
        assert!(matches!(
            pair.guest
                .retry_session_udp("main", &pair.guest_id, one.generation),
            Err(ConnectError::Busy)
        ));
        assert!(matches!(
            pair.guest
                .retry_session_udp("other", &pair.guest_id, source_generation),
            Err(ConnectError::OwnerMismatch)
        ));
        assert!(matches!(
            pair.guest
                .retry_session_udp("main", "old-operation", source_generation),
            Err(ConnectError::StaleOperation)
        ));
        assert!(matches!(
            pair.guest
                .retry_session_udp("main", &pair.guest_id, source_generation - 1),
            Err(ConnectError::StaleOperation)
        ));
        gate.release.notify_one();
        failed = pair.udp(UdpState::Failed).await;
        assert_eq!(
            recovery(&failed),
            json!({"attemptsUsed":attempt,"maxAttempts":3,"pending":false})
        );
        transfer(&mut tcp).await;
    }
    assert!(matches!(
        pair.guest
            .retry_session_udp("main", &pair.guest_id, failed.generation),
        Err(ConnectError::Busy)
    ));
    assert!(matches!(
        pair.host.retry_session_udp(
            "main",
            &pair.host_id,
            pair.host.snapshot("main").unwrap().generation
        ),
        Err(ConnectError::StaleOperation)
    ));
    assert!(
        timeout(Duration::from_millis(100), endpoint.recv(&mut [0; 8]))
            .await
            .is_err()
    );
    pair.stop().await;
    assert!(pair
        .guest
        .retry_session_udp("main", &pair.guest_id, failed.generation)
        .is_err());
    let new = pair
        .guest
        .prepare_session_fragmented_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: lab::generate_key().public().to_string(),
            },
        )
        .unwrap();
    assert!(matches!(
        pair.guest
            .retry_session_udp("main", &pair.guest_id, failed.generation),
        Err(ConnectError::StaleOperation)
    ));
    assert!(serde_json::to_value(pair.guest.snapshot("main").unwrap())
        .unwrap()
        .get("udpRecovery")
        .is_none());
    pair.guest.stop("main", &new.operation_id).await.unwrap();
    drop(occupied);
    drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
    echo.stop().await;
}

#[derive(Clone, Copy)]
enum End {
    Stop,
    Revoke,
    Expire,
}
async fn pending_end(end: End) {
    let occupied = UdpSocket::bind(LOCAL_UDP_BIND).await.unwrap();
    let echo = Echo::new().await;
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let session = if matches!(end, End::Expire) { 4 } else { 20 };
    let pair = Arc::new(Pair::new(echo.address, endpoint.local_addr().unwrap(), 2, session).await);
    let failed = pair.udp(UdpState::Failed).await;
    let mut tcp = TcpStream::connect(pair.tcp).await.unwrap();
    transfer(&mut tcp).await;
    drop(occupied);
    let gate = arm(&pair);
    retry(&pair, failed.generation);
    entered(&gate).await;
    let mut stopping = None;
    match end {
        End::Stop => {
            let owned = pair.clone();
            stopping = Some(tokio::spawn(async move {
                owned.guest.stop("main", &owned.guest_id).await.unwrap()
            }));
            let closing = until(&pair.guest, |snapshot| snapshot.phase == "closing").await;
            assert_eq!(recovery(&closing)["pending"], false);
        }
        End::Revoke => {
            pair.host.stop("main", &pair.host_id).await.unwrap();
        }
        End::Expire => {
            while lab::unix_millis() <= pair.timing.session_expires_at_ms {
                sleep(Duration::from_millis(5)).await;
            }
        }
    }
    eof(&mut tcp).await;
    let before_rejected = pair.guest.snapshot("main").unwrap();
    if matches!(end, End::Revoke) {
        // The actor still owns the held setup future. Its watcher cannot yet
        // project closure; only the retained actual Connection says closed.
        let state = pair.guest.live.state.lock().unwrap();
        let attempt = state.attempt.as_ref().unwrap();
        assert_eq!(attempt.snapshot.phase, "guestConnected");
        assert!(!attempt.cancel.is_cancelled());
        assert!(attempt.authority.as_ref().unwrap().check_session().is_ok());
        assert!(Instant::now() < attempt.session_deadline.unwrap());
        assert_eq!(
            recovery(&attempt.snapshot),
            json!({"attemptsUsed":1,"maxAttempts":3,"pending":true})
        );
    }
    let rejected = pair
        .guest
        .retry_session_udp("main", &pair.guest_id, failed.generation);
    assert!(
        rejected.is_err(),
        "duplicate may not outlive lifecycle/authority"
    );
    if matches!(end, End::Revoke) {
        assert!(matches!(rejected, Err(ConnectError::StaleOperation)));
        let after_rejected = pair.guest.snapshot("main").unwrap();
        assert_eq!(after_rejected.generation, before_rejected.generation);
        assert_eq!(recovery(&after_rejected), recovery(&before_rejected));
    }
    if let Some(task) = &stopping {
        assert!(!task.is_finished(), "stop must await pending owned setup");
    }
    assert_ne!(
        pair.guest.snapshot("main").unwrap().cleanup_state,
        "complete"
    );
    gate.release.notify_one();
    if let Some(task) = stopping {
        assert_eq!(task.await.unwrap().cleanup_state, "complete");
    }
    let closed = until(&pair.guest, |snapshot| snapshot.phase == "closed").await;
    assert_eq!(closed.udp.as_ref().unwrap().udp_state, UdpState::Closed);
    assert_eq!(
        recovery(&closed),
        json!({"attemptsUsed":1,"maxAttempts":3,"pending":false})
    );
    assert_eq!(closed.session_timing.as_ref(), Some(&pair.timing));
    pair.stop().await;
    drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
    assert!(
        timeout(Duration::from_millis(80), endpoint.recv(&mut [0; 8]))
            .await
            .is_err()
    );
    echo.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stop_awaits_pending_real_setup_and_never_publishes_late_success() {
    pending_end(End::Stop).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn revoke_awaits_pending_real_setup_and_never_publishes_late_success() {
    pending_end(End::Revoke).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn expiry_awaits_pending_real_setup_and_never_publishes_late_success() {
    pending_end(End::Expire).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn recovered_bridge_and_active_tcp_still_close_at_original_deadline() {
    let occupied = UdpSocket::bind(LOCAL_UDP_BIND).await.unwrap();
    let echo = Echo::new().await;
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let pair = Pair::new(echo.address, endpoint.local_addr().unwrap(), 1, 4).await;
    let failed = pair.udp(UdpState::Failed).await;
    let mut tcp = TcpStream::connect(pair.tcp).await.unwrap();
    drop(occupied);
    pair.after_ingress().await;
    retry(&pair, failed.generation);
    let ready = pair.udp(UdpState::Available).await;
    assert_eq!(ready.session_timing.as_ref(), Some(&pair.timing));
    let source = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    datagram(&source, &endpoint, 4096, 7).await;
    transfer(&mut tcp).await;
    until(&pair.guest, |snapshot| snapshot.phase == "closed").await;
    assert!(lab::unix_millis() >= pair.timing.session_expires_at_ms);
    eof(&mut tcp).await;
    assert!(pair
        .guest
        .retry_session_udp("main", &pair.guest_id, failed.generation)
        .is_err());
    pair.stop().await;
    drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
    echo.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn all_previous_active_profiles_refuse_recovery_and_keep_original_payloads() {
    let occupied = UdpSocket::bind(LOCAL_UDP_BIND).await.unwrap();
    let echo = Echo::new().await;
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    for profile in [Profile::Legacy, Profile::Session, Profile::Fragmented] {
        let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let bind = held.local_addr().unwrap();
        let host = ConnectManager::default();
        let guest = ConnectManager::default();
        let hp = match profile {
            Profile::Legacy => host.prepare_host(
                "main",
                HostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: bind.to_string(),
                    game_target: echo.address.to_string(),
                    lifetime_seconds: 20,
                },
            ),
            Profile::Session => host.prepare_session_host(
                "main",
                SessionHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: bind.to_string(),
                    game_target: echo.address.to_string(),
                    admission_seconds: 5,
                    session_seconds: 20,
                },
            ),
            Profile::Fragmented => host.prepare_fragmented_host(
                "main",
                FragmentedHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: bind.to_string(),
                    game_target: echo.address.to_string(),
                    udp_target: endpoint.local_addr().unwrap().to_string(),
                    lifetime_seconds: 20,
                },
            ),
            _ => unreachable!(),
        }
        .unwrap();
        let request = GuestRequest {
            mode: ConnectMode::Loopback,
            expected_host_public_id: hp.public_id,
        };
        let gp = match profile {
            Profile::Legacy => guest.prepare_guest("main", request),
            Profile::Session => guest.prepare_session_guest("main", request),
            Profile::Fragmented => guest.prepare_fragmented_guest("main", request),
            _ => unreachable!(),
        }
        .unwrap();
        let review = host
            .review_host("main", &hp.operation_id, &gp.public_id)
            .unwrap();
        drop(held);
        host.confirm("main", &hp.operation_id, &review.digest)
            .await
            .unwrap();
        until(&host, |snapshot| snapshot.phase == "hostListening").await;
        let invitation = host.export_invitation("main", &hp.operation_id).unwrap();
        let review = guest
            .review_guest("main", &gp.operation_id, &invitation.invitation)
            .unwrap();
        guest
            .confirm("main", &gp.operation_id, &review.digest)
            .await
            .unwrap();
        let connected = until(&guest, |snapshot| {
            snapshot.phase == "guestConnected"
                && (profile != Profile::Fragmented
                    || snapshot
                        .udp
                        .as_ref()
                        .is_some_and(|udp| udp.udp_state == UdpState::Failed))
        })
        .await;
        assert!(serde_json::to_value(&connected)
            .unwrap()
            .get("udpRecovery")
            .is_none());
        assert!(matches!(
            guest.retry_session_udp("main", &gp.operation_id, connected.generation),
            Err(ConnectError::StaleOperation)
        ));
        assert_eq!(
            guest.snapshot("main").unwrap().generation,
            connected.generation
        );
        let mut tcp = TcpStream::connect(connected.tcp_address.unwrap())
            .await
            .unwrap();
        transfer(&mut tcp).await;
        guest.stop("main", &gp.operation_id).await.unwrap();
        host.stop("main", &hp.operation_id).await.unwrap();
        assert!(serde_json::to_value(guest.snapshot("main").unwrap())
            .unwrap()
            .get("udpRecovery")
            .is_none());
        drop(std::net::UdpSocket::bind(bind).unwrap());
    }
    assert!(
        timeout(Duration::from_millis(80), endpoint.recv(&mut [0; 8]))
            .await
            .is_err()
    );
    drop(occupied);
    echo.stop().await;
}
