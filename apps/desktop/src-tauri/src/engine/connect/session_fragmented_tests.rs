//! Real local managers for the explicitly selected schema-3 desktop profile.
//! Fixed guest UDP port means these tests run with --test-threads=1.
use super::fragmented_profile::{UdpState, LOCAL_UDP_BIND, SESSION_PROFILE};
use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    time::{sleep, timeout},
};

fn request(
    bind: SocketAddr,
    game: SocketAddr,
    udp: SocketAddr,
    admission: u16,
    session: u16,
) -> SessionFragmentedHostRequest {
    SessionFragmentedHostRequest {
        mode: ConnectMode::Loopback,
        bind_address: bind.to_string(),
        game_target: game.to_string(),
        udp_target: udp.to_string(),
        admission_seconds: admission,
        session_seconds: session,
    }
}
pub(super) async fn until(
    manager: &ConnectManager,
    accept: impl Fn(&ConnectSnapshot) -> bool,
) -> ConnectSnapshot {
    timeout(Duration::from_secs(10), async {
        loop {
            let snapshot = manager.snapshot("main").unwrap();
            if accept(&snapshot) {
                return snapshot;
            }
            assert_ne!(snapshot.phase, "failed", "owned manager failed");
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}
pub(super) async fn eof(stream: &mut TcpStream) {
    assert!(matches!(
        timeout(Duration::from_secs(3), stream.read(&mut [0]))
            .await
            .unwrap(),
        Ok(0) | Err(_)
    ));
}
pub(super) async fn transfer(stream: &mut TcpStream) {
    timeout(Duration::from_secs(3), async {
        let data = vec![0x46; 65536];
        stream.write_all(&data).await.unwrap();
        let mut received = vec![0; data.len()];
        stream.read_exact(&mut received).await.unwrap();
        assert_eq!(received, data);
    })
    .await
    .unwrap();
}
pub(super) struct Echo {
    pub(super) address: SocketAddr,
    closed: Arc<AtomicUsize>,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}
impl Echo {
    pub(super) async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let closed = Arc::new(AtomicUsize::new(0));
        let count = closed.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let task = tokio::spawn(async move {
            let mut children = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _=token.cancelled()=>break,
                    next=listener.accept()=>{
                        let (mut socket,_)=next.unwrap();let count=count.clone();
                        children.spawn(async move {
                            let (mut read,mut write)=socket.split();
                            let _=tokio::io::copy(&mut read,&mut write).await;
                            let _=write.shutdown().await;count.fetch_add(1,Ordering::SeqCst);
                        });
                    },
                    _=children.join_next(),if !children.is_empty()=>{},
                }
            }
            children.shutdown().await;
        });
        Self {
            address,
            closed,
            cancel,
            task,
        }
    }
    async fn closed(&self, expected: usize) {
        timeout(Duration::from_secs(3), async {
            while self.closed.load(Ordering::SeqCst) < expected {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    pub(super) async fn stop(self) {
        self.cancel.cancel();
        self.task.await.unwrap();
        drop(std::net::TcpListener::bind(self.address).unwrap());
    }
}
pub(super) struct Pair {
    pub(super) host: ConnectManager,
    pub(super) guest: ConnectManager,
    pub(super) host_id: String,
    pub(super) guest_id: String,
    pub(super) bind: SocketAddr,
    pub(super) tcp: SocketAddr,
    pub(super) timing: SessionTiming,
}
impl Pair {
    pub(super) async fn new(
        game: SocketAddr,
        udp: SocketAddr,
        admission: u16,
        session: u16,
    ) -> Self {
        let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let bind = held.local_addr().unwrap();
        let host = ConnectManager::default();
        let guest = ConnectManager::default();
        let hp = host
            .prepare_session_fragmented_host("main", request(bind, game, udp, admission, session))
            .unwrap();
        let gp = guest
            .prepare_session_fragmented_guest(
                "main",
                GuestRequest {
                    mode: ConnectMode::Loopback,
                    expected_host_public_id: hp.public_id.clone(),
                },
            )
            .unwrap();
        assert_eq!(hp.invitation_profile, Some(SESSION_PROFILE));
        assert_eq!(gp.invitation_profile, Some(SESSION_PROFILE));
        let prepared = guest.snapshot("main").unwrap();
        assert!(prepared.fragmented_udp.is_none() && prepared.session_timing.is_none());
        assert_eq!(prepared.udp.unwrap().udp_state, UdpState::NotStarted);
        let review = host
            .review_host("main", &hp.operation_id, &gp.public_id)
            .unwrap();
        let timing = review.session_timing.clone().unwrap();
        assert_eq!(timing.profile, SESSION_PROFILE);
        assert_eq!(timing.schema_version, 3);
        assert_eq!(
            (timing.admission_seconds, timing.session_seconds),
            (admission, session)
        );
        assert_eq!(
            review.invitation_expires_at_ms,
            timing.admission_expires_at_ms
        );
        assert!(review.expires_at_ms <= timing.admission_expires_at_ms);
        assert_eq!(review.lifetime_seconds, admission);
        let contract = review.fragmented_udp.as_ref().unwrap().contract.clone();
        assert_eq!(contract, FragmentedUdpContract::session());
        assert_eq!(
            review
                .fragmented_udp
                .as_ref()
                .unwrap()
                .udp_target
                .as_deref(),
            Some(udp.to_string().as_str())
        );
        assert_eq!(review.services.len(), 2);
        assert!(matches!(
            host.confirm("main", &hp.operation_id, "wrong").await,
            Err(ConnectError::WrongDigest)
        ));
        assert!(matches!(
            host.confirm("other", &hp.operation_id, &review.digest)
                .await,
            Err(ConnectError::OwnerMismatch)
        ));
        drop(held);
        host.confirm("main", &hp.operation_id, &review.digest)
            .await
            .unwrap();
        let listening = until(&host, |s| s.phase == "hostListening").await;
        assert_eq!(listening.expires_at_ms, Some(timing.session_expires_at_ms));
        let export = host.export_invitation("main", &hp.operation_id).unwrap();
        assert_eq!(export.expires_at_ms, timing.admission_expires_at_ms);
        assert_eq!(export.session_timing.as_ref(), Some(&timing));
        assert_eq!(export.fragmented_udp.as_ref(), Some(&contract));
        let review = guest
            .review_guest("main", &gp.operation_id, &export.invitation)
            .unwrap();
        assert_eq!(review.session_timing.as_ref(), Some(&timing));
        assert_eq!(review.fragmented_udp.as_ref().unwrap().contract, contract);
        assert!(review.fragmented_udp.as_ref().unwrap().udp_target.is_none());
        guest
            .confirm("main", &gp.operation_id, &review.digest)
            .await
            .unwrap();
        let active = until(&guest, |s| s.phase == "guestConnected").await;
        assert_eq!(active.expires_at_ms, Some(timing.session_expires_at_ms));
        assert_eq!(active.session_timing.as_ref(), Some(&timing));
        Self {
            host,
            guest,
            host_id: hp.operation_id,
            guest_id: gp.operation_id,
            bind,
            tcp: active.tcp_address.unwrap().parse().unwrap(),
            timing,
        }
    }
    pub(super) async fn udp(&self, expected: UdpState) -> ConnectSnapshot {
        until(&self.guest, |s| {
            s.udp.as_ref().is_some_and(|u| u.udp_state == expected)
        })
        .await
    }
    pub(super) async fn after_ingress(&self) {
        while lab::unix_millis() <= self.timing.admission_expires_at_ms {
            sleep(Duration::from_millis(5)).await;
        }
    }
    pub(super) async fn stop(&self) {
        let (h, g) = tokio::join!(
            self.host.stop("main", &self.host_id),
            self.guest.stop("main", &self.guest_id)
        );
        assert_eq!(h.unwrap().cleanup_state, "complete");
        let g = g.unwrap();
        assert_eq!(g.cleanup_state, "complete");
        assert_eq!(g.udp.unwrap().udp_state, UdpState::Closed);
        drop(std::net::TcpListener::bind(self.tcp).unwrap());
        drop(std::net::UdpSocket::bind(self.bind).unwrap());
    }
}
pub(super) async fn datagram(
    source: &UdpSocket,
    endpoint: &UdpSocket,
    size: usize,
    tag: u8,
) -> SocketAddr {
    let payload = vec![tag; size];
    source.send_to(&payload, LOCAL_UDP_BIND).await.unwrap();
    let mut received = vec![0; 65536];
    let (n, remote) = timeout(Duration::from_secs(3), endpoint.recv_from(&mut received))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&received[..n], payload.as_slice());
    let reply = vec![tag ^ 0x55; size];
    endpoint.send_to(&reply, remote).await.unwrap();
    let n = timeout(Duration::from_secs(3), source.recv(&mut received))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&received[..n], reply.as_slice());
    remote
}

#[test]
fn profile_capability_and_request_are_explicit_without_changing_previous_contracts() {
    let caps = serde_json::to_value(ConnectManager::default().capabilities()).unwrap();
    assert_eq!(caps["maxLifetimeSeconds"], 600);
    assert_eq!(caps["sessionInvitations"]["schemaVersion"], 2);
    assert_eq!(caps["fragmentedUdpInvitations"]["maxLifetimeSeconds"], 120);
    assert_eq!(
        caps["sessionFragmentedUdpInvitations"],
        json!({
            "profile":"sessionFragmentedGameUdp","invitationSchemaVersion":3,"udpEnvelopeVersion":2,
            "approvedOriginalDatagramBytes":4096,"maxFragmentFrameBytes":1264,"localUdpBind":"127.0.0.1:35554",
            "udpReturnPolicy":{"type":"authorizedDownstream","bytesPerSecond":16384,"datagramsPerSecond":20},
            "maxAdmissionSeconds":600,"maxSessionSeconds":3600,"udpRecovery":{"maxAttempts":3},"ownedGameTcpDiscovery":cfg!(windows),
        })
    );
    let valid = json!({"mode":"loopback","bindAddress":"127.0.0.1:42000","gameTarget":"127.0.0.1:43001","udpTarget":"127.0.0.1:43002","admissionSeconds":600,"sessionSeconds":3600});
    assert!(serde_json::from_value::<SessionFragmentedHostRequest>(valid.clone()).is_ok());
    assert!(serde_json::from_value::<FragmentedHostRequest>(valid.clone()).is_err());
    assert!(serde_json::from_value::<SessionHostRequest>(valid.clone()).is_err());
    for field in ["udpTarget", "admissionSeconds", "sessionSeconds"] {
        let mut changed = valid.clone();
        changed.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<SessionFragmentedHostRequest>(changed).is_err());
    }
    for field in ["admissionSeconds", "sessionSeconds"] {
        for bad in [json!(1.5), json!(-1), json!(65536)] {
            let mut changed = valid.clone();
            changed[field] = bad;
            assert!(serde_json::from_value::<SessionFragmentedHostRequest>(changed).is_err());
        }
    }
    let mut changed = valid.clone();
    changed["discoverySelectionId"] = json!("not_allowed");
    assert!(serde_json::from_value::<SessionFragmentedHostRequest>(changed).is_err());
    for (admission, session) in [(0, 1), (601, 3600), (5, 4), (600, 3601)] {
        let mut changed = valid.clone();
        changed["admissionSeconds"] = json!(admission);
        changed["sessionSeconds"] = json!(session);
        let request = serde_json::from_value::<SessionFragmentedHostRequest>(changed).unwrap();
        assert!(matches!(
            ConnectManager::default().prepare_session_fragmented_host("main", request),
            Err(ConnectError::InvalidRequest)
        ));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tcp_and_two_udp_sources_continue_after_admission_then_revoke_with_cleanup() {
    let echo = Echo::new().await;
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let pair = Pair::new(echo.address, endpoint.local_addr().unwrap(), 2, 12).await;
    let ready = pair.udp(UdpState::Available).await;
    assert_eq!(ready.udp.unwrap().udp_original_capacity, Some(4096));
    assert!(ready.simple_voice_chat_state == "disabled" && ready.plasmo_voice_state == "disabled");
    let mut first = TcpStream::connect(pair.tcp).await.unwrap();
    transfer(&mut first).await;
    let sources = [
        UdpSocket::bind("127.0.0.1:0").await.unwrap(),
        UdpSocket::bind("127.0.0.1:0").await.unwrap(),
    ];
    let a = datagram(&sources[0], &endpoint, 4096, 0x31).await;
    transfer(&mut first).await;
    let b = datagram(&sources[1], &endpoint, 4096, 0x32).await;
    assert_ne!(a, b);
    pair.after_ingress().await;
    assert!(pair.host.export_invitation("main", &pair.host_id).is_err());
    assert_eq!(pair.guest.snapshot("main").unwrap().phase, "guestConnected");
    transfer(&mut first).await;
    let mut second = TcpStream::connect(pair.tcp).await.unwrap();
    transfer(&mut second).await;
    assert_eq!(datagram(&sources[0], &endpoint, 4096, 0x41).await, a);
    assert_eq!(datagram(&sources[1], &endpoint, 4096, 0x42).await, b);
    // Four complete requests and replies: exactly32768 bytes of application UDP.
    for source in &sources {
        assert!(
            timeout(Duration::from_millis(100), source.recv(&mut [0; 8192]))
                .await
                .is_err()
        );
    }
    pair.host.stop("main", &pair.host_id).await.unwrap();
    eof(&mut first).await;
    eof(&mut second).await;
    echo.closed(2).await;
    until(&pair.guest, |s| s.phase == "closed").await;
    pair.stop().await;
    drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
    echo.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn occupied_udp_port_keeps_tcp_alive_after_admission_without_retry() {
    let occupied = UdpSocket::bind(LOCAL_UDP_BIND).await.unwrap();
    let echo = Echo::new().await;
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let pair = Pair::new(echo.address, endpoint.local_addr().unwrap(), 1, 12).await;
    let failed = pair.udp(UdpState::Failed).await;
    assert_eq!(failed.phase, "guestConnected");
    assert_eq!(
        failed.udp.unwrap().udp_error_code,
        Some(UdpFailure::SetupFailed)
    );
    pair.after_ingress().await;
    let mut stream = TcpStream::connect(pair.tcp).await.unwrap();
    transfer(&mut stream).await;
    drop(occupied);
    sleep(Duration::from_millis(200)).await;
    assert_eq!(
        pair.guest.snapshot("main").unwrap().udp.unwrap().udp_state,
        UdpState::Failed
    );
    let held = UdpSocket::bind(LOCAL_UDP_BIND).await.unwrap();
    transfer(&mut stream).await;
    assert!(
        timeout(Duration::from_millis(100), endpoint.recv(&mut [0; 64]))
            .await
            .is_err()
    );
    pair.stop().await;
    eof(&mut stream).await;
    echo.closed(1).await;
    drop(held);
    drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
    echo.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn original_session_deadline_closes_active_tcp_and_udp_without_renewal() {
    let echo = Echo::new().await;
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let pair = Pair::new(echo.address, endpoint.local_addr().unwrap(), 1, 3).await;
    pair.udp(UdpState::Available).await;
    let mut stream = TcpStream::connect(pair.tcp).await.unwrap();
    transfer(&mut stream).await;
    let source = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let remote = datagram(&source, &endpoint, 64, 0x51).await;
    pair.after_ingress().await;
    transfer(&mut stream).await;
    assert_eq!(datagram(&source, &endpoint, 64, 0x52).await, remote);
    until(&pair.guest, |s| s.phase == "closed").await;
    assert!(lab::unix_millis() >= pair.timing.session_expires_at_ms);
    eof(&mut stream).await;
    echo.closed(1).await;
    endpoint.send_to(&[1, 2, 3], remote).await.unwrap();
    assert!(
        timeout(Duration::from_millis(100), source.recv(&mut [0; 64]))
            .await
            .is_err()
    );
    pair.stop().await;
    drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
    echo.stop().await;
}

#[tokio::test]
async fn four_explicit_profiles_cannot_accept_each_others_invitations_before_network() {
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    held.set_nonblocking(true).unwrap();
    let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let services =
        fragmented_profile::services(tcp.local_addr().unwrap(), udp.local_addr().unwrap()).unwrap();
    let key = lab::generate_key();
    for selected in 0..4 {
        let guest = ConnectManager::default();
        let req = GuestRequest {
            mode: ConnectMode::Loopback,
            expected_host_public_id: key.public().to_string(),
        };
        let p = match selected {
            0 => guest.prepare_guest("main", req),
            1 => guest.prepare_session_guest("main", req),
            2 => guest.prepare_fragmented_guest("main", req),
            _ => guest.prepare_session_fragmented_guest("main", req),
        }
        .unwrap();
        let peer = p.public_id.parse().unwrap();
        let bind = held.local_addr().unwrap();
        let encoded = [
            lab::Invitation::issue(
                &key,
                peer,
                "profile_test",
                bind,
                &services[..1],
                lab::unix_millis() + 30000,
            )
            .unwrap()
            .encode(),
            lab::SessionInvitation::issue_loopback_for_lifetimes(
                &key,
                peer,
                "profile_test",
                bind,
                &services[..1],
                Duration::from_secs(30),
                Duration::from_secs(60),
            )
            .unwrap()
            .encode(),
            lab::Invitation::issue_fragmented_for_lifetime(
                &key,
                peer,
                "profile_test",
                bind,
                &services,
                Duration::from_secs(30),
            )
            .unwrap()
            .encode(),
            lab::SessionInvitation::issue_fragmented_loopback_for_lifetimes(
                &key,
                peer,
                "profile_test",
                bind,
                &services,
                Duration::from_secs(30),
                Duration::from_secs(60),
            )
            .unwrap()
            .encode(),
        ];
        for (i, invitation) in encoded.iter().enumerate() {
            if i != selected {
                assert!(matches!(
                    guest.review_guest("main", &p.operation_id, invitation),
                    Err(ConnectError::InvitationRejected)
                ));
            }
        }
        if selected == 3 {
            for (admission, session) in [(1500, 3000), (2000, 3500)] {
                let value = lab::SessionInvitation::issue_fragmented_loopback_for_lifetimes(
                    &key,
                    peer,
                    "fractional",
                    bind,
                    &services,
                    Duration::from_millis(admission),
                    Duration::from_millis(session),
                )
                .unwrap();
                assert!(matches!(
                    guest.review_guest("main", &p.operation_id, &value.encode()),
                    Err(ConnectError::InvitationRejected)
                ));
            }
        }
        let reviewed = guest
            .review_guest("main", &p.operation_id, &encoded[selected])
            .unwrap();
        assert!(matches!(
            guest.confirm("main", &p.operation_id, "wrong").await,
            Err(ConnectError::WrongDigest)
        ));
        assert!(matches!(
            guest
                .confirm("other", &p.operation_id, &reviewed.digest)
                .await,
            Err(ConnectError::OwnerMismatch)
        ));
        guest.stop("main", &p.operation_id).await.unwrap();
    }
    assert_eq!(
        held.recv(&mut [0; 2048]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(timeout(Duration::from_millis(100), tcp.accept())
        .await
        .is_err());
    assert!(
        timeout(Duration::from_millis(100), udp.recv(&mut [0; 2048]))
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unconsumed_host_closes_at_admission_with_no_service_access() {
    let game = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = held.local_addr().unwrap();
    let host = ConnectManager::default();
    let p = host
        .prepare_session_fragmented_host(
            "main",
            request(
                bind,
                game.local_addr().unwrap(),
                udp.local_addr().unwrap(),
                1,
                30,
            ),
        )
        .unwrap();
    let r = host
        .review_host(
            "main",
            &p.operation_id,
            &lab::generate_key().public().to_string(),
        )
        .unwrap();
    drop(held);
    host.confirm("main", &p.operation_id, &r.digest)
        .await
        .unwrap();
    until(&host, |s| s.phase == "hostListening").await;
    let closed = until(&host, |s| s.phase == "closed").await;
    assert_eq!(closed.cleanup_state, "complete");
    assert!(lab::unix_millis() < r.session_timing.unwrap().session_expires_at_ms);
    assert!(host.export_invitation("main", &p.operation_id).is_err());
    assert!(timeout(Duration::from_millis(100), game.accept())
        .await
        .is_err());
    assert!(
        timeout(Duration::from_millis(100), udp.recv(&mut [0; 2048]))
            .await
            .is_err()
    );
    drop(std::net::UdpSocket::bind(bind).unwrap());
}
