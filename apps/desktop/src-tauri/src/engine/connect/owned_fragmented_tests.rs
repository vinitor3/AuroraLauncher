//! Schema3 composes the real Windows owned-child TCP selection with manual UDP.
//! The fixture child and both authenticated agents are local and test-owned.
use super::fragmented_profile::{UdpState, LOCAL_UDP_BIND, SESSION_PROFILE};
use super::session_fragmented_tests::{datagram, until};
use super::*;
use crate::engine::connect_discovery::{
    tests::{approved, expire_selection, Fixture},
    DiscoveryError, DiscoveryManager, TargetStatus,
};
use tokio::{
    net::{TcpStream, UdpSocket},
    time::{sleep, timeout},
};

fn request(
    bind: SocketAddr,
    udp: SocketAddr,
    admission: u16,
    session: u16,
) -> SessionFragmentedDiscoveredHostRequest {
    SessionFragmentedDiscoveredHostRequest {
        mode: ConnectMode::Loopback,
        bind_address: bind.to_string(),
        udp_target: udp.to_string(),
        admission_seconds: admission,
        session_seconds: session,
    }
}
async fn selection(manager: &DiscoveryManager, fixture: &mut Fixture) -> TargetStatus {
    let inventory = manager.inventory("main", fixture.proof()).unwrap();
    let candidate = inventory
        .candidates
        .iter()
        .find(|candidate| {
            candidate.bind_kind == "loopback" && candidate.target.starts_with("127.0.0.1:")
        })
        .unwrap();
    let review = manager
        .review("main", &inventory.inventory_id, &candidate.candidate_id)
        .unwrap();
    manager
        .confirm_status("main", &review.review_id, &review.digest)
        .await
        .unwrap()
}
async fn source_failed(manager: &ConnectManager) -> ConnectSnapshot {
    timeout(Duration::from_secs(5), async {
        loop {
            let snapshot = manager.snapshot("main").unwrap();
            if snapshot.phase == "failed" {
                return snapshot;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}
async fn no_udp(endpoint: &UdpSocket) {
    assert!(
        timeout(Duration::from_millis(80), endpoint.recv(&mut [0; 16]))
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owned_game_and_manual_udp_continue_after_ingress_then_source_loss_closes_both() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let selected = selection(&discovery, &mut fixture).await;
    assert!(matches!(
        discovery.take_target("other", &selected.selection_id),
        Err(DiscoveryError::Owner)
    ));
    let game: SocketAddr = selected.target.parse().unwrap();
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = held.local_addr().unwrap();
    let host = ConnectManager::default();
    let guest = ConnectManager::default();
    let hp = host
        .reserve_session_fragmented_host_discovered(
            "main",
            request(bind, endpoint.local_addr().unwrap(), 2, 20),
            selected.selection_id.clone(),
            discovery.clone(),
        )
        .unwrap()
        .wait()
        .await
        .unwrap();
    assert!(matches!(
        discovery.take_target("main", &selected.selection_id),
        Err(DiscoveryError::Stale)
    ));
    let gp = guest
        .prepare_session_fragmented_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: hp.public_id,
            },
        )
        .unwrap();
    let review = host
        .review_host("main", &hp.operation_id, &gp.public_id)
        .unwrap();
    assert_eq!(review.target_source, Some("ownedInstance"));
    assert_eq!(review.instance_id.as_deref(), Some("fixture-instance"));
    assert_eq!(review.run_id.as_deref(), Some("fixture-run"));
    assert_eq!(
        review.game_target.as_deref(),
        Some(selected.target.as_str())
    );
    assert_eq!(
        review
            .fragmented_udp
            .as_ref()
            .unwrap()
            .udp_target
            .as_deref(),
        Some(endpoint.local_addr().unwrap().to_string().as_str())
    );
    assert_eq!(
        review.fragmented_udp.as_ref().unwrap().contract,
        FragmentedUdpContract::session()
    );
    let timing = review.session_timing.clone().unwrap();
    assert_eq!(timing.profile, SESSION_PROFILE);
    assert!(matches!(
        host.confirm("main", &hp.operation_id, "wrong").await,
        Err(ConnectError::WrongDigest)
    ));
    no_udp(&endpoint).await;
    drop(held);
    host.confirm("main", &hp.operation_id, &review.digest)
        .await
        .unwrap();
    until(&host, |snapshot| snapshot.phase == "hostListening").await;
    let export = host.export_invitation("main", &hp.operation_id).unwrap();
    let guest_review = guest
        .review_guest("main", &gp.operation_id, &export.invitation)
        .unwrap();
    assert!(guest_review.game_target.is_none());
    assert!(guest_review
        .fragmented_udp
        .as_ref()
        .unwrap()
        .udp_target
        .is_none());
    assert!(guest_review.instance_id.is_none() && guest_review.run_id.is_none());
    guest
        .confirm("main", &gp.operation_id, &guest_review.digest)
        .await
        .unwrap();
    let active = until(&guest, |snapshot| {
        snapshot
            .udp
            .as_ref()
            .is_some_and(|udp| udp.udp_state == UdpState::Available)
    })
    .await;
    let tcp: SocketAddr = active.tcp_address.unwrap().parse().unwrap();
    while lab::unix_millis() <= timing.admission_expires_at_ms {
        sleep(Duration::from_millis(5)).await;
    }
    assert!(host.export_invitation("main", &hp.operation_id).is_err());
    assert_eq!(
        guest
            .probe_status("main", &gp.operation_id)
            .await
            .unwrap()
            .status_state,
        "responded"
    );
    let one = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let two = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let a = datagram(&one, &endpoint, 4096, 0x64).await;
    let b = datagram(&two, &endpoint, 4096, 0x65).await;
    assert_ne!(a, b);
    assert_eq!(
        guest.snapshot("main").unwrap().session_timing.as_ref(),
        Some(&timing)
    );
    // The same address is now held by this different test-owned process.
    // Port presence alone must never replace the original Child proof.
    fixture.stop();
    let replacement = std::net::TcpListener::bind(game).unwrap();
    let failed = source_failed(&host).await;
    assert_eq!(failed.error_code, Some("connect_source_changed"));
    assert_eq!(failed.cleanup_state, "complete");
    let closed = until(&guest, |snapshot| snapshot.phase == "closed").await;
    assert_eq!(closed.udp.as_ref().unwrap().udp_state, UdpState::Closed);
    assert_eq!(closed.session_timing.as_ref(), Some(&timing));
    assert_eq!(closed.expires_at_ms, Some(timing.session_expires_at_ms));
    one.send_to(&[0x64; 32], LOCAL_UDP_BIND).await.unwrap();
    no_udp(&endpoint).await;
    assert!(TcpStream::connect(tcp).await.is_err());
    drop(std::net::UdpSocket::bind(bind).unwrap());
    drop(std::net::TcpListener::bind(tcp).unwrap());
    drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
    host.stop("main", &hp.operation_id).await.unwrap();
    guest.stop("main", &gp.operation_id).await.unwrap();
    drop(replacement);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn expired_selection_and_invalid_manual_udp_refuse_before_transport_or_datagrams() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let held = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let bind = held.local_addr().unwrap();
    let host = ConnectManager::default();
    let target = selection(&discovery, &mut fixture).await;
    expire_selection(&discovery);
    assert!(matches!(
        host.reserve_session_fragmented_host_discovered(
            "main",
            request(bind, endpoint.local_addr().unwrap(), 2, 20),
            target.selection_id,
            discovery.clone(),
        )
        .unwrap()
        .wait()
        .await,
        Err(DiscoveredPreparationError::Discovery(DiscoveryError::Stale))
    ));
    let failed = host.snapshot("main").unwrap();
    assert_eq!(failed.phase, "failed");
    host.stop("main", failed.operation_id.as_deref().unwrap())
        .await
        .unwrap();
    let baseline = host.snapshot("main").unwrap();
    for (udp, admission, session) in [
        ("127.0.0.1:0", 2, 20),
        ("192.0.2.1:43002", 2, 20),
        ("[fe80::1%1]:43002", 2, 20),
        ("127.0.0.1:43002", 0, 20),
        ("127.0.0.1:43002", 601, 3600),
        ("127.0.0.1:43002", 2, 1),
        ("127.0.0.1:43002", 2, 3601),
    ] {
        let target = selection(&discovery, &mut fixture).await;
        let mut changed = request(bind, endpoint.local_addr().unwrap(), admission, session);
        changed.udp_target = udp.into();
        assert!(matches!(
            host.reserve_session_fragmented_host_discovered(
                "main",
                changed,
                target.selection_id,
                discovery.clone()
            ),
            Err(ConnectError::InvalidRequest)
        ));
        let observed = host.snapshot("main").unwrap();
        assert_eq!(observed.operation_id, baseline.operation_id);
        assert_eq!(observed.generation, baseline.generation);
    }
    no_udp(&held).await;
    no_udp(&endpoint).await;
    fixture.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn source_loss_between_review_and_confirm_cannot_start_either_service() {
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let target = selection(&discovery, &mut fixture).await;
    let game: SocketAddr = target.target.parse().unwrap();
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let held = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let host = ConnectManager::default();
    let hp = host
        .reserve_session_fragmented_host_discovered(
            "main",
            request(
                held.local_addr().unwrap(),
                endpoint.local_addr().unwrap(),
                2,
                20,
            ),
            target.selection_id,
            discovery.clone(),
        )
        .unwrap()
        .wait()
        .await
        .unwrap();
    let review = host
        .review_host(
            "main",
            &hp.operation_id,
            &lab::generate_key().public().to_string(),
        )
        .unwrap();
    fixture.stop();
    let replacement = std::net::TcpListener::bind(game).unwrap();
    host.confirm("main", &hp.operation_id, &review.digest)
        .await
        .unwrap();
    let failed = source_failed(&host).await;
    assert_eq!(failed.error_code, Some("connect_source_changed"));
    assert_eq!(failed.cleanup_state, "complete");
    no_udp(&held).await;
    no_udp(&endpoint).await;
    host.stop("main", &hp.operation_id).await.unwrap();
    drop(replacement);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manual_udp_failure_keeps_discovered_game_status_until_explicit_stop() {
    let occupied = UdpSocket::bind(LOCAL_UDP_BIND).await.unwrap();
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let target = selection(&discovery, &mut fixture).await;
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = held.local_addr().unwrap();
    let host = ConnectManager::default();
    let guest = ConnectManager::default();
    let hp = host
        .reserve_session_fragmented_host_discovered(
            "main",
            request(bind, endpoint.local_addr().unwrap(), 1, 20),
            target.selection_id,
            discovery.clone(),
        )
        .unwrap()
        .wait()
        .await
        .unwrap();
    let gp = guest
        .prepare_session_fragmented_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: hp.public_id,
            },
        )
        .unwrap();
    let review = host
        .review_host("main", &hp.operation_id, &gp.public_id)
        .unwrap();
    let timing = review.session_timing.unwrap();
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
    let failed = until(&guest, |snapshot| {
        snapshot
            .udp
            .as_ref()
            .is_some_and(|udp| udp.udp_state == UdpState::Failed)
    })
    .await;
    let tcp: SocketAddr = failed.tcp_address.unwrap().parse().unwrap();
    while lab::unix_millis() <= timing.admission_expires_at_ms {
        sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        guest
            .probe_status("main", &gp.operation_id)
            .await
            .unwrap()
            .status_state,
        "responded"
    );
    // Stop Connect, then the owned child remains discoverable and queryable.
    let stopped = guest.stop("main", &gp.operation_id).await.unwrap();
    assert_eq!(stopped.cleanup_state, "complete");
    host.stop("main", &hp.operation_id).await.unwrap();
    assert!(approved(&discovery, &mut fixture).await.recheck().is_ok());
    no_udp(&endpoint).await;
    drop(std::net::UdpSocket::bind(bind).unwrap());
    drop(std::net::TcpListener::bind(tcp).unwrap());
    drop(occupied);
    drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
    fixture.stop();
}
