//! Explicit IPv4 desktop caller. Real transport tests require the operator's
//! own bindable IPv4 and run separately, serially; no LAN/NAT claim is inferred.
use super::fragmented_profile::{UdpState, LOCAL_UDP_BIND};
use super::session_fragmented_tests::{datagram, eof, until};
use super::*;
use serde_json::json;
use std::{
    future::Future,
    net::Ipv4Addr,
    task::{Context, Poll, Waker},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
    time::{sleep, timeout},
};

const PROFILES: [Profile; 4] = [
    Profile::Legacy,
    Profile::Session,
    Profile::Fragmented,
    Profile::SessionFragmented,
];

struct Request {
    mode: ConnectMode,
    bind: String,
    game: String,
    udp: String,
    admission: u16,
    session: u16,
}
impl Request {
    fn pure(mode: ConnectMode) -> Self {
        Self {
            mode,
            bind: match mode {
                ConnectMode::Loopback => "127.0.0.1:32000",
                ConnectMode::ExternalIpv6 => "[2606:4700:4700::1111]:32000",
                ConnectMode::DirectIpv4 => "10.23.4.5:32000",
            }
            .into(),
            game: "127.0.0.1:43001".into(),
            udp: "127.0.0.1:43002".into(),
            admission: 30,
            session: 60,
        }
    }
    fn prepare(
        &self,
        host: &ConnectManager,
        profile: Profile,
    ) -> Result<ConnectPreparation, ConnectError> {
        match profile {
            Profile::Legacy => host.prepare_host(
                "main",
                HostRequest {
                    mode: self.mode,
                    bind_address: self.bind.clone(),
                    game_target: self.game.clone(),
                    lifetime_seconds: self.admission,
                },
            ),
            Profile::Session => host.prepare_session_host(
                "main",
                SessionHostRequest {
                    mode: self.mode,
                    bind_address: self.bind.clone(),
                    game_target: self.game.clone(),
                    admission_seconds: self.admission,
                    session_seconds: self.session,
                },
            ),
            Profile::Fragmented => host.prepare_fragmented_host(
                "main",
                FragmentedHostRequest {
                    mode: self.mode,
                    bind_address: self.bind.clone(),
                    game_target: self.game.clone(),
                    udp_target: self.udp.clone(),
                    lifetime_seconds: self.admission,
                },
            ),
            Profile::SessionFragmented => host.prepare_session_fragmented_host(
                "main",
                SessionFragmentedHostRequest {
                    mode: self.mode,
                    bind_address: self.bind.clone(),
                    game_target: self.game.clone(),
                    udp_target: self.udp.clone(),
                    admission_seconds: self.admission,
                    session_seconds: self.session,
                },
            ),
        }
    }
}
fn prepare_guest(
    manager: &ConnectManager,
    profile: Profile,
    mode: ConnectMode,
    host: &str,
) -> ConnectPreparation {
    let request = GuestRequest {
        mode,
        expected_host_public_id: host.into(),
    };
    match profile {
        Profile::Legacy => manager.prepare_guest("main", request),
        Profile::Session => manager.prepare_session_guest("main", request),
        Profile::Fragmented => manager.prepare_fragmented_guest("main", request),
        Profile::SessionFragmented => manager.prepare_session_fragmented_guest("main", request),
    }
    .unwrap()
}
fn encoded(host: &ConnectManager) -> String {
    host.live
        .state
        .lock()
        .unwrap()
        .attempt
        .as_ref()
        .unwrap()
        .reviewed
        .as_ref()
        .unwrap()
        .invitation
        .encode()
}
fn refused_without_runtime(
    future: impl Future<Output = Result<ConnectSnapshot, ConnectError>>,
    expected: ConnectError,
) {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        matches!(future.as_mut().poll(&mut context), Poll::Ready(Err(error)) if error == expected)
    );
}

#[test]
fn explicit_ipv4_dto_capability_and_old_modes_are_preserved() {
    let manager = ConnectManager::default();
    let caps = serde_json::to_value(manager.capabilities()).unwrap();
    assert_eq!(
        caps["modes"],
        json!(["loopback", "externalIpv6", "directIpv4"])
    );
    assert_eq!(caps["available"], true);
    assert_eq!(caps["voiceEnabled"], false);
    assert_eq!(caps["relayEnabled"], false);
    assert_eq!(
        caps["sessionFragmentedUdpInvitations"]["maxSessionSeconds"],
        3600
    );
    for mode in ["loopback", "externalIpv6", "directIpv4"] {
        let parsed: ConnectMode = serde_json::from_value(json!(mode)).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), mode);
    }
    for mode in ["auto", "ipv4", "DirectIpv4", "directIpv6", "relay"] {
        assert!(serde_json::from_value::<ConnectMode>(json!(mode)).is_err());
    }
    let request = json!({"mode":"directIpv4","bindAddress":"10.23.4.5:32000","gameTarget":"127.0.0.1:43001","udpTarget":"127.0.0.1:43002","admissionSeconds":30,"sessionSeconds":60});
    assert!(serde_json::from_value::<SessionFragmentedHostRequest>(request.clone()).is_ok());
    let mut override_request = request;
    override_request["publicAddress"] = json!("8.8.8.8:32000");
    assert!(serde_json::from_value::<SessionFragmentedHostRequest>(override_request).is_err());
}

#[test]
fn four_profiles_reject_invalid_ipv4_and_nonloopback_services_before_io() {
    for profile in PROFILES {
        for bind in [
            "localhost:32000",
            "10.23.4.5:0",
            "10.23.4.5:032000",
            "010.23.4.5:32000",
            " 10.23.4.5:32000",
            "0.1.2.3:32000",
            "127.0.0.1:32000",
            "100.64.0.1:32000",
            "169.254.1.1:32000",
            "224.0.0.1:32000",
            "255.255.255.255:32000",
            "192.0.0.9:32000",
            "192.0.2.1:32000",
            "192.31.196.1:32000",
            "192.52.193.1:32000",
            "192.88.99.1:32000",
            "192.175.48.1:32000",
            "198.18.0.1:32000",
            "198.51.100.1:32000",
            "203.0.113.1:32000",
            "[::ffff:10.23.4.5]:32000",
            "[2606:4700:4700::1111]:32000",
        ] {
            let manager = ConnectManager::default();
            let mut request = Request::pure(ConnectMode::DirectIpv4);
            request.bind = bind.into();
            assert!(matches!(
                request.prepare(&manager, profile),
                Err(ConnectError::InvalidRequest)
            ));
            assert_eq!(manager.snapshot("main").unwrap().phase, "idle");
        }
        for endpoint in [
            "0.0.0.0:43001",
            "10.23.4.5:43001",
            "localhost:43001",
            "127.0.0.1:0",
        ] {
            let manager = ConnectManager::default();
            let mut request = Request::pure(ConnectMode::DirectIpv4);
            request.game = endpoint.into();
            assert!(matches!(
                request.prepare(&manager, profile),
                Err(ConnectError::InvalidRequest)
            ));
            if profile.is_fragmented() {
                request.game = "127.0.0.1:43001".into();
                request.udp = endpoint.into();
                assert!(matches!(
                    request.prepare(&manager, profile),
                    Err(ConnectError::InvalidRequest)
                ));
            }
            assert_eq!(manager.snapshot("main").unwrap().phase, "idle");
        }
        for bind in [
            "10.23.4.5:32000",
            "172.16.0.1:32000",
            "192.168.1.1:32000",
            "8.8.8.8:32000",
        ] {
            let manager = ConnectManager::default();
            let mut request = Request::pure(ConnectMode::DirectIpv4);
            request.bind = bind.into();
            assert!(request.prepare(&manager, profile).is_ok());
            let state = manager.live.state.lock().unwrap();
            let attempt = state.attempt.as_ref().unwrap();
            assert!(
                attempt.worker.is_none() && attempt.host.is_none() && attempt.authority.is_none()
            );
        }
    }
}

#[tokio::test]
async fn ipv4_reviews_preserve_authority_and_reject_other_modes_and_profiles_without_io() {
    for source in PROFILES {
        for selected in PROFILES {
            for mode in [
                ConnectMode::Loopback,
                ConnectMode::ExternalIpv6,
                ConnectMode::DirectIpv4,
            ] {
                let host = ConnectManager::default();
                let guest = ConnectManager::default();
                let request = Request::pure(ConnectMode::DirectIpv4);
                let hp = request.prepare(&host, source).unwrap();
                let gp = prepare_guest(&guest, selected, mode, &hp.public_id);
                let review = host
                    .reserve_host_review("main", &hp.operation_id, &gp.public_id)
                    .unwrap()
                    .wait()
                    .await
                    .unwrap();
                assert_eq!(review.mode, ConnectMode::DirectIpv4);
                assert_eq!(review.direct_address, request.bind);
                let value = encoded(&host);
                if source != selected || mode != ConnectMode::DirectIpv4 {
                    assert!(matches!(
                        guest.review_guest("main", &gp.operation_id, &value),
                        Err(ConnectError::InvitationRejected)
                    ));
                    assert_eq!(guest.snapshot("main").unwrap().phase, "guestPrepared");
                } else {
                    let reviewed = guest
                        .review_guest("main", &gp.operation_id, &value)
                        .unwrap();
                    assert_eq!(reviewed.mode, ConnectMode::DirectIpv4);
                    assert_eq!(reviewed.invitation_digest, review.invitation_digest);
                    assert_eq!(reviewed.session_timing, review.session_timing);
                    assert!(reviewed.game_target.is_none());
                    assert!(reviewed
                        .fragmented_udp
                        .as_ref()
                        .is_none_or(|udp| udp.udp_target.is_none()));
                    assert_eq!(
                        reviewed.services.len(),
                        if selected.is_fragmented() { 2 } else { 1 }
                    );
                    refused_without_runtime(
                        guest.confirm("main", &gp.operation_id, "wrong"),
                        ConnectError::WrongDigest,
                    );
                    refused_without_runtime(
                        guest.confirm("other", &gp.operation_id, &reviewed.digest),
                        ConnectError::OwnerMismatch,
                    );
                    refused_without_runtime(
                        guest.confirm("main", "stale", &reviewed.digest),
                        ConnectError::StaleOperation,
                    );
                    assert_eq!(guest.snapshot("main").unwrap().phase, "reviewed");
                }
                assert!(host.export_invitation("main", &hp.operation_id).is_err());
                assert!(host
                    .live
                    .state
                    .lock()
                    .unwrap()
                    .attempt
                    .as_ref()
                    .unwrap()
                    .worker
                    .is_none());
                assert!(guest
                    .live
                    .state
                    .lock()
                    .unwrap()
                    .attempt
                    .as_ref()
                    .unwrap()
                    .worker
                    .is_none());
            }
        }
    }
    // The converse also matters: explicitly selecting IPv4 cannot accept an
    // already valid loopback or IPv6 invitation, even for the correct profile.
    for profile in PROFILES {
        for mode in [ConnectMode::Loopback, ConnectMode::ExternalIpv6] {
            let host = ConnectManager::default();
            let guest = ConnectManager::default();
            let hp = Request::pure(mode).prepare(&host, profile).unwrap();
            let gp = prepare_guest(&guest, profile, ConnectMode::DirectIpv4, &hp.public_id);
            host.reserve_host_review("main", &hp.operation_id, &gp.public_id)
                .unwrap()
                .wait()
                .await
                .unwrap();
            assert!(matches!(
                guest.review_guest("main", &gp.operation_id, &encoded(&host)),
                Err(ConnectError::InvitationRejected)
            ));
            assert!(guest
                .live
                .state
                .lock()
                .unwrap()
                .attempt
                .as_ref()
                .unwrap()
                .worker
                .is_none());
        }
    }
}

#[tokio::test]
async fn ipv4_review_digest_binds_mode_and_original_deadlines_are_not_extended() {
    for profile in PROFILES {
        let host = ConnectManager::default();
        let guest = ConnectManager::default();
        let hp = Request::pure(ConnectMode::DirectIpv4)
            .prepare(&host, profile)
            .unwrap();
        let gp = prepare_guest(&guest, profile, ConnectMode::DirectIpv4, &hp.public_id);
        let review = host
            .reserve_host_review("main", &hp.operation_id, &gp.public_id)
            .unwrap()
            .wait()
            .await
            .unwrap();
        let stored = host.snapshot("main").unwrap().review.unwrap();
        assert_eq!(
            serde_json::to_value(&review).unwrap(),
            serde_json::to_value(stored).unwrap()
        );
        let mut changed = review.clone();
        changed.digest.clear();
        let digest = |value: &ConnectReview| {
            Sha256::digest(serde_json::to_vec(&(Option::<String>::None, value)).unwrap())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        assert_eq!(digest(&changed), review.digest);
        changed.mode = ConnectMode::Loopback;
        let other_mode_digest = digest(&changed);
        assert_ne!(other_mode_digest, review.digest);
        refused_without_runtime(
            host.confirm("main", &hp.operation_id, &other_mode_digest),
            ConnectError::WrongDigest,
        );
        let guest_review = guest
            .review_guest("main", &gp.operation_id, &encoded(&host))
            .unwrap();
        let mut state = guest.live.state.lock().unwrap();
        let attempt = state.attempt.as_mut().unwrap();
        let original = attempt.reviewed.as_ref().unwrap().deadline;
        assert!(original > Instant::now());
        attempt.reviewed.as_mut().unwrap().review_deadline =
            Instant::now() - Duration::from_millis(1);
        drop(state);
        refused_without_runtime(
            guest.confirm("main", &gp.operation_id, &guest_review.digest),
            ConnectError::PlanExpired,
        );
        let state = guest.live.state.lock().unwrap();
        let attempt = state.attempt.as_ref().unwrap();
        assert!(attempt.worker.is_none() && attempt.authority.is_none());
        assert_ne!(attempt.snapshot.phase, "starting");
    }
}

fn own_ipv4() -> Ipv4Addr {
    let text = std::env::var("AURORA_CONNECT_TEST_IPV4")
        .expect("explicit own-IPv4 opt-in requires AURORA_CONNECT_TEST_IPV4");
    let ip: Ipv4Addr = text.parse().expect("own IPv4 literal required");
    assert!(ip.to_string() == text, "own IPv4 must be canonical");
    lab::validate_direct_ipv4_invitation_address(SocketAddr::from((ip, 1))).unwrap();
    ip
}
struct Pair {
    host: ConnectManager,
    guest: ConnectManager,
    host_id: String,
    guest_id: String,
    bind: SocketAddr,
    tcp: SocketAddr,
    profile: Profile,
    timing: Option<SessionTiming>,
}
impl Pair {
    async fn new(
        profile: Profile,
        game: SocketAddr,
        udp: SocketAddr,
        admission: u16,
        session: u16,
    ) -> Self {
        // Bind first proves the operator-selected address belongs to this PC.
        let held = std::net::UdpSocket::bind(SocketAddr::from((own_ipv4(), 0)))
            .expect("selected own IPv4 must be bindable on this PC");
        let bind = held.local_addr().unwrap();
        let host = ConnectManager::default();
        let guest = ConnectManager::default();
        let request = Request {
            mode: ConnectMode::DirectIpv4,
            bind: bind.to_string(),
            game: game.to_string(),
            udp: udp.to_string(),
            admission,
            session,
        };
        let hp = request.prepare(&host, profile).unwrap();
        let gp = prepare_guest(&guest, profile, ConnectMode::DirectIpv4, &hp.public_id);
        let review = host
            .reserve_host_review("main", &hp.operation_id, &gp.public_id)
            .unwrap()
            .wait()
            .await
            .unwrap();
        assert_eq!(review.mode, ConnectMode::DirectIpv4);
        assert!(review.direct_address == bind.to_string());
        assert_eq!(review.target_source, Some("manual"));
        let timing = review.session_timing.clone();
        assert!(matches!(
            host.confirm("main", &hp.operation_id, "wrong").await,
            Err(ConnectError::WrongDigest)
        ));
        drop(held);
        host.confirm("main", &hp.operation_id, &review.digest)
            .await
            .unwrap();
        until(&host, |s| s.phase == "hostListening").await;
        let export = host.export_invitation("main", &hp.operation_id).unwrap();
        assert_eq!(export.session_timing, timing);
        let guest_review = guest
            .review_guest("main", &gp.operation_id, &export.invitation)
            .unwrap();
        assert_eq!(guest_review.mode, ConnectMode::DirectIpv4);
        assert_eq!(guest_review.session_timing, timing);
        assert_eq!(guest_review.invitation_digest, review.invitation_digest);
        assert!(guest_review.game_target.is_none());
        guest
            .confirm("main", &gp.operation_id, &guest_review.digest)
            .await
            .unwrap();
        let active = until(&guest, |s| {
            s.phase == "guestConnected"
                && (!profile.is_fragmented()
                    || s.udp.as_ref().is_some_and(|udp| {
                        matches!(udp.udp_state, UdpState::Available | UdpState::Failed)
                    }))
        })
        .await;
        assert_eq!(active.session_timing, timing);
        assert_eq!(active.world_state, "notVerified");
        assert_eq!(active.simple_voice_chat_state, "disabled");
        assert_eq!(active.plasmo_voice_state, "disabled");
        assert!(!active.relay_enabled);
        Self {
            host,
            guest,
            host_id: hp.operation_id,
            guest_id: gp.operation_id,
            bind,
            tcp: active.tcp_address.unwrap().parse().unwrap(),
            profile,
            timing,
        }
    }
    async fn after_ingress(&self) {
        let timing = self.timing.as_ref().unwrap();
        while lab::unix_millis() <= timing.admission_expires_at_ms {
            sleep(Duration::from_millis(5)).await;
        }
        assert!(self.host.export_invitation("main", &self.host_id).is_err());
        assert_eq!(
            self.guest.snapshot("main").unwrap().session_timing,
            self.timing
        );
    }
    async fn stop(&self) {
        let (host, guest) = tokio::join!(
            self.host.stop("main", &self.host_id),
            self.guest.stop("main", &self.guest_id)
        );
        assert_eq!(host.unwrap().cleanup_state, "complete");
        let guest = guest.unwrap();
        assert_eq!(guest.cleanup_state, "complete");
        assert_eq!(guest.session_timing, self.timing);
        assert!(guest.tcp_address.is_none());
        if self.profile.is_fragmented() {
            assert_eq!(guest.udp.unwrap().udp_state, UdpState::Closed);
        }
        drop(std::net::UdpSocket::bind(self.bind).unwrap());
        drop(std::net::TcpListener::bind(self.tcp).unwrap());
    }
}
async fn transfer(address: SocketAddr) {
    timeout(Duration::from_secs(5), async {
        let mut socket = TcpStream::connect(address).await.unwrap();
        let bytes = vec![0x45; 65536];
        socket.write_all(&bytes).await.unwrap();
        let mut reply = vec![0; bytes.len()];
        socket.read_exact(&mut reply).await.unwrap();
        assert_eq!(bytes, reply);
        socket.shutdown().await.unwrap();
    })
    .await
    .unwrap();
}
async fn sources(endpoint: &UdpSocket) {
    let one = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let two = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let first = datagram(&one, endpoint, 4096, 0x66).await;
    let second = datagram(&two, endpoint, 4096, 0x76).await;
    assert_ne!(first, second);
    for source in [&one, &two] {
        assert!(
            timeout(Duration::from_millis(80), source.recv(&mut [0; 4096]))
                .await
                .is_err()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires explicit own IPv4 AURORA_CONNECT_TEST_IPV4; serial; not a LAN/internet test"]
async fn own_ipv4_four_profiles_tcp_status_udp_and_cleanup() {
    let (game, fixture) = super::super::tests::status_fixture().await;
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    for profile in PROFILES {
        let admission = if matches!(profile, Profile::Session | Profile::SessionFragmented) {
            2
        } else {
            20
        };
        let pair = Pair::new(profile, game, endpoint.local_addr().unwrap(), admission, 20).await;
        if pair.timing.is_some() {
            pair.after_ingress().await;
        }
        if profile.is_fragmented() {
            assert_eq!(
                pair.guest.snapshot("main").unwrap().udp.unwrap().udp_state,
                UdpState::Available
            );
            tokio::join!(transfer(pair.tcp), sources(&endpoint));
        } else {
            transfer(pair.tcp).await;
        }
        let snapshot = pair
            .guest
            .probe_status("main", &pair.guest_id)
            .await
            .unwrap();
        assert_eq!(snapshot.status_state, "responded");
        assert_eq!(snapshot.status_summary.unwrap().reported_protocol, 767);
        assert_eq!(snapshot.world_state, "notVerified");
        assert_eq!(snapshot.session_timing, pair.timing);
        pair.stop().await;
        if profile.is_fragmented() {
            drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
        }
    }
    fixture.abort();
    let _ = fixture.await;
    drop(std::net::TcpListener::bind(game).unwrap());
    println!("{{\"event\":\"desktop_ipv4_profiles_passed\",\"scope\":\"same_pc_own_ipv4\",\"profiles\":4,\"relay\":false,\"worldJoined\":false,\"udpApplicationBytesEachDirection\":16384,\"cleanupPassed\":true}}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires explicit own IPv4 AURORA_CONNECT_TEST_IPV4; serial; not a LAN/internet test"]
async fn own_ipv4_schema3_expiry_closes_active_tcp_udp_at_original_deadline() {
    let (game, fixture) = super::super::tests::status_fixture().await;
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let pair = Pair::new(
        Profile::SessionFragmented,
        game,
        endpoint.local_addr().unwrap(),
        2,
        6,
    )
    .await;
    let mut tcp = TcpStream::connect(pair.tcp).await.unwrap();
    tcp.write_all(&[0x45]).await.unwrap();
    assert_eq!(tcp.read_u8().await.unwrap(), 0x45);
    pair.after_ingress().await;
    tokio::join!(transfer(pair.tcp), sources(&endpoint));
    let source = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    source.send_to(&[0x46; 4097], LOCAL_UDP_BIND).await.unwrap();
    assert!(
        timeout(Duration::from_millis(100), endpoint.recv(&mut [0; 8192]))
            .await
            .is_err()
    );
    let closed = until(&pair.guest, |s| s.phase == "closed").await;
    assert_eq!(closed.session_timing, pair.timing);
    assert_eq!(closed.udp.unwrap().udp_state, UdpState::Closed);
    eof(&mut tcp).await;
    assert!(TcpStream::connect(pair.tcp).await.is_err());
    source.send_to(&[0x47; 32], LOCAL_UDP_BIND).await.unwrap();
    assert!(
        timeout(Duration::from_millis(80), endpoint.recv(&mut [0; 8192]))
            .await
            .is_err()
    );
    pair.stop().await;
    drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
    fixture.abort();
    let _ = fixture.await;
    drop(std::net::TcpListener::bind(game).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires explicit own IPv4 AURORA_CONNECT_TEST_IPV4; serial; not a LAN/internet test"]
async fn own_ipv4_schema3_udp_failure_and_manual_retry_preserve_tcp_authority() {
    let occupied = UdpSocket::bind(LOCAL_UDP_BIND).await.unwrap();
    let (game, fixture) = super::super::tests::status_fixture().await;
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let pair = Pair::new(
        Profile::SessionFragmented,
        game,
        endpoint.local_addr().unwrap(),
        2,
        20,
    )
    .await;
    let failed = pair.guest.snapshot("main").unwrap();
    assert_eq!(failed.udp.unwrap().udp_state, UdpState::Failed);
    assert_eq!(failed.udp_recovery.unwrap().attempts_used, 0);
    pair.after_ingress().await;
    transfer(pair.tcp).await;
    assert_eq!(
        pair.guest
            .probe_status("main", &pair.guest_id)
            .await
            .unwrap()
            .status_state,
        "responded"
    );
    drop(occupied);
    let before = pair.guest.snapshot("main").unwrap();
    let pending = pair
        .guest
        .retry_session_udp("main", &pair.guest_id, before.generation)
        .unwrap();
    assert!(pending.udp_recovery.unwrap().pending);
    assert_eq!(pending.session_timing, pair.timing);
    let ready = until(&pair.guest, |s| {
        s.udp
            .as_ref()
            .is_some_and(|udp| udp.udp_state == UdpState::Available)
    })
    .await;
    assert_eq!(ready.udp_recovery.unwrap().attempts_used, 1);
    assert_eq!(ready.session_timing, pair.timing);
    assert!(ready.tcp_address == Some(pair.tcp.to_string()));
    tokio::join!(transfer(pair.tcp), sources(&endpoint));
    pair.host.stop("main", &pair.host_id).await.unwrap();
    until(&pair.guest, |s| s.phase == "closed").await;
    assert!(TcpStream::connect(pair.tcp).await.is_err());
    pair.stop().await;
    drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
    fixture.abort();
    let _ = fixture.await;
    drop(std::net::TcpListener::bind(game).unwrap());
}
