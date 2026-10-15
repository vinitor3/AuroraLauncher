//! Real desktop transport tests. No Java, external network or configuration.
use super::fragmented_profile::{UdpState, LOCAL_UDP_BIND};
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    time::{sleep, timeout},
};

async fn snapshot_until(
    manager: &ConnectManager,
    accept: impl Fn(&ConnectSnapshot) -> bool,
) -> ConnectSnapshot {
    timeout(Duration::from_secs(15), async {
        loop {
            let s = manager.snapshot("main").unwrap();
            if accept(&s) {
                return s;
            }
            assert!(s.phase != "failed", "owned manager failed");
            sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap()
}
struct Pair {
    host: ConnectManager,
    guest: ConnectManager,
    host_id: String,
    guest_id: String,
    bind: SocketAddr,
    tcp: SocketAddr,
    expires: u64,
}
impl Pair {
    async fn new(game: SocketAddr, udp: SocketAddr, ttl: u16) -> Self {
        let reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let bind = reservation.local_addr().unwrap();
        let host = ConnectManager::default();
        let guest = ConnectManager::default();
        let hp = host
            .prepare_fragmented_host(
                "main",
                FragmentedHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: bind.to_string(),
                    game_target: game.to_string(),
                    udp_target: udp.to_string(),
                    lifetime_seconds: ttl,
                },
            )
            .unwrap();
        let gp = guest
            .prepare_fragmented_guest(
                "main",
                GuestRequest {
                    mode: ConnectMode::Loopback,
                    expected_host_public_id: hp.public_id,
                },
            )
            .unwrap();
        assert_eq!(
            guest.snapshot("main").unwrap().udp.unwrap().udp_state,
            UdpState::NotStarted
        );
        assert!(serde_json::to_value(host.snapshot("main").unwrap())
            .unwrap()
            .get("udpState")
            .is_none());
        let review = host
            .reserve_host_review("main", &hp.operation_id, &gp.public_id)
            .unwrap()
            .wait()
            .await
            .unwrap();
        assert!(review.session_timing.is_none());
        assert!(
            review
                .fragmented_udp
                .as_ref()
                .unwrap()
                .udp_target
                .as_deref()
                == Some(&udp.to_string())
        );
        assert!(matches!(
            host.confirm("main", &hp.operation_id, "wrong").await,
            Err(ConnectError::WrongDigest)
        ));
        drop(reservation);
        host.confirm("main", &hp.operation_id, &review.digest)
            .await
            .unwrap();
        snapshot_until(&host, |s| s.phase == "hostListening").await;
        let export = host.export_invitation("main", &hp.operation_id).unwrap();
        assert!(export.fragmented_udp.is_some() && export.session_timing.is_none());
        let review = guest
            .review_guest("main", &gp.operation_id, &export.invitation)
            .unwrap();
        assert_eq!(review.lifetime_seconds, ttl);
        assert!(review.fragmented_udp.as_ref().unwrap().udp_target.is_none());
        guest
            .confirm("main", &gp.operation_id, &review.digest)
            .await
            .unwrap();
        let active = snapshot_until(&guest, |s| s.phase == "guestConnected").await;
        Self {
            host,
            guest,
            host_id: hp.operation_id,
            guest_id: gp.operation_id,
            bind,
            tcp: active.tcp_address.unwrap().parse().unwrap(),
            expires: export.expires_at_ms,
        }
    }
    async fn udp(&self, expected: UdpState) -> ConnectSnapshot {
        snapshot_until(&self.guest, |s| {
            s.udp.as_ref().is_some_and(|u| u.udp_state == expected)
        })
        .await
    }
    async fn stop(&self) {
        let (h, g) = tokio::join!(
            self.host.stop("main", &self.host_id),
            self.guest.stop("main", &self.guest_id)
        );
        assert_eq!(h.unwrap().cleanup_state, "complete");
        let g = g.unwrap();
        assert_eq!(g.cleanup_state, "complete");
        assert_eq!(g.udp.unwrap().udp_state, UdpState::Closed);
        drop(std::net::UdpSocket::bind(self.bind).unwrap());
        drop(std::net::TcpListener::bind(self.tcp).unwrap());
    }
}
struct Echo {
    address: SocketAddr,
    bytes: Arc<AtomicUsize>,
    closed: Arc<AtomicUsize>,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}
impl Echo {
    async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let bytes = Arc::new(AtomicUsize::new(0));
        let closed = Arc::new(AtomicUsize::new(0));
        let cancel = CancellationToken::new();
        let count = bytes.clone();
        let done = closed.clone();
        let token = cancel.clone();
        let task = tokio::spawn(async move {
            let mut tasks = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _=token.cancelled()=>break,
                    accepted=listener.accept()=>{let(mut stream,_)=accepted.unwrap();let count=count.clone();let done=done.clone();tasks.spawn(async move {let mut buf=vec![0;32768];loop {match stream.read(&mut buf).await {Ok(0)|Err(_)=>break,Ok(n)=>{count.fetch_add(n,Ordering::SeqCst);if stream.write_all(&buf[..n]).await.is_err(){break;}}}}let _=stream.shutdown().await;done.fetch_add(1,Ordering::SeqCst);});},
                    _=tasks.join_next(),if !tasks.is_empty()=>{}
                }
            }
            tasks.shutdown().await;
        });
        Self {
            address,
            bytes,
            closed,
            cancel,
            task,
        }
    }
    async fn wait_closed(&self) {
        timeout(Duration::from_secs(3), async {
            while self.closed.load(Ordering::SeqCst) == 0 {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    async fn stop(self) {
        self.cancel.cancel();
        self.task.await.unwrap();
        drop(std::net::TcpListener::bind(self.address).unwrap());
    }
}
async fn tcp_small(stream: &mut TcpStream) {
    stream.write_all(&[0x45; 64]).await.unwrap();
    let mut bytes = [0; 64];
    timeout(Duration::from_secs(3), stream.read_exact(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert!(bytes == [0x45; 64]);
}
async fn eof(stream: &mut TcpStream) {
    assert!(matches!(
        timeout(Duration::from_secs(3), stream.read(&mut [0]))
            .await
            .unwrap(),
        Ok(0) | Err(_)
    ));
}
async fn no_udp(socket: &UdpSocket) {
    assert!(
        timeout(Duration::from_millis(300), socket.recv(&mut [0; 8192]))
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_managers_tcp_one_mib_and_two_udp_sources_preserve_boundaries_concurrently() {
    let echo = Echo::new().await;
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let udp_target = endpoint.local_addr().unwrap();
    let pair = Pair::new(echo.address, udp_target, 30).await;
    let ready = pair.udp(UdpState::Available).await;
    let fields = ready.udp.unwrap();
    assert_eq!(fields.udp_original_capacity, Some(4096));
    assert_eq!(fields.udp_capacity_scope, Some("initialAssociation"));
    assert!(ready.simple_voice_chat_state == "disabled" && ready.plasmo_voice_state == "disabled");
    let (release, wait) = tokio::sync::oneshot::channel();
    let tcp = pair.tcp;
    let stream_task = tokio::spawn(async move {
        let mut stream = TcpStream::connect(tcp).await.unwrap();
        let block = vec![0x45; 32768];
        for _ in 0..31 {
            stream.write_all(&block).await.unwrap();
            let mut reply = vec![0; block.len()];
            stream.read_exact(&mut reply).await.unwrap();
            assert!(reply == block);
            sleep(Duration::from_millis(100)).await;
        }
        wait.await.unwrap();
        stream.write_all(&block).await.unwrap();
        stream.shutdown().await.unwrap();
        let mut reply = vec![0; block.len()];
        stream.read_exact(&mut reply).await.unwrap();
        assert!(reply == block);
        assert_eq!(stream.read(&mut [0]).await.unwrap(), 0);
    });
    let sources = [
        UdpSocket::bind("127.0.0.1:0").await.unwrap(),
        UdpSocket::bind("127.0.0.1:0").await.unwrap(),
    ];
    let mut associations = std::collections::BTreeSet::new();
    let mut attempted = 0usize;
    let mut valid_received = 0usize;
    let mut buf = vec![0; 65536];
    let mut received = vec![0; 65536];
    let before = echo.bytes.load(Ordering::SeqCst);
    for (i, source) in sources.iter().enumerate() {
        let payload = vec![0x61 + i as u8; 4096];
        source.send_to(&payload, LOCAL_UDP_BIND).await.unwrap();
        attempted += payload.len();
        let (n, remote) = timeout(Duration::from_secs(3), endpoint.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert!(buf[..n] == payload);
        valid_received += n;
        associations.insert(remote);
        endpoint.send_to(&vec![0xEE; 4097], remote).await.unwrap();
        attempted += 4097;
        let answer = payload.iter().map(|byte| byte ^ 0x55).collect::<Vec<_>>();
        endpoint.send_to(&answer, remote).await.unwrap();
        attempted += answer.len();
        let n = timeout(Duration::from_secs(3), source.recv(&mut received))
            .await
            .unwrap()
            .unwrap();
        assert!(received[..n] == answer);
        no_udp(&sources[1 - i]).await; // no cross-source or oversized return
        source
            .send_to(&vec![0xEE; 4097], LOCAL_UDP_BIND)
            .await
            .unwrap();
        attempted += 4097;
        no_udp(&endpoint).await;
        sleep(Duration::from_millis(250)).await; // quota counts frames, not originals
        let payload = vec![0x71 + i as u8; 64];
        source.send_to(&payload, LOCAL_UDP_BIND).await.unwrap();
        attempted += payload.len();
        let (n, again) = timeout(Duration::from_secs(3), endpoint.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert!(buf[..n] == payload && again == remote);
        valid_received += n;
        endpoint.send_to(&payload, remote).await.unwrap();
        attempted += payload.len();
        let n = timeout(Duration::from_secs(3), source.recv(&mut received))
            .await
            .unwrap()
            .unwrap();
        assert!(received[..n] == payload);
    }
    assert_eq!(associations.len(), 2);
    assert_eq!(valid_received, 8320);
    assert_eq!(attempted, 33028);
    assert!(attempted <= 65536);
    let progress = echo.bytes.load(Ordering::SeqCst);
    assert!(progress > before && progress < 1048576);
    release.send(()).unwrap();
    timeout(Duration::from_secs(6), stream_task)
        .await
        .unwrap()
        .unwrap();
    echo.wait_closed().await;
    assert_eq!(echo.bytes.load(Ordering::SeqCst), 1048576);
    for source in &sources {
        no_udp(source).await;
    }
    pair.stop().await;
    drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
    drop(endpoint);
    drop(std::net::UdpSocket::bind(udp_target).unwrap());
    for source in sources {
        let address = source.local_addr().unwrap();
        drop(source);
        drop(std::net::UdpSocket::bind(address).unwrap());
    }
    echo.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn occupied_udp_port_does_not_close_tcp_or_status_and_never_retries() {
    let occupied = UdpSocket::bind(LOCAL_UDP_BIND).await.unwrap();
    let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let (target, fixture) = super::super::tests::status_fixture().await;
    let pair = Pair::new(target, endpoint.local_addr().unwrap(), 30).await;
    let failed = pair.udp(UdpState::Failed).await;
    assert_eq!(failed.phase, "guestConnected");
    let fields = failed.udp.unwrap();
    assert_eq!(fields.udp_error_code, Some(UdpFailure::SetupFailed));
    assert!(fields.udp_address.is_none() && fields.udp_original_capacity.is_none());
    assert_eq!(
        pair.guest
            .probe_status("main", &pair.guest_id)
            .await
            .unwrap()
            .status_state,
        "responded"
    );
    let mut stream = TcpStream::connect(pair.tcp).await.unwrap();
    tcp_small(&mut stream).await;
    drop(occupied);
    sleep(Duration::from_millis(200)).await;
    assert_eq!(
        pair.guest.snapshot("main").unwrap().udp.unwrap().udp_state,
        UdpState::Failed
    );
    let free = UdpSocket::bind(LOCAL_UDP_BIND).await.unwrap();
    tcp_small(&mut stream).await;
    no_udp(&endpoint).await;
    pair.stop().await;
    eof(&mut stream).await;
    drop(free);
    fixture.abort();
    let _ = fixture.await;
    drop(std::net::TcpListener::bind(target).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revocation_expiry_and_manager_drop_close_tcp_udp_and_release_owned_sockets() {
    for end in [0, 1, 2] {
        let echo = Echo::new().await;
        let endpoint = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let pair = Pair::new(
            echo.address,
            endpoint.local_addr().unwrap(),
            if end == 1 { 2 } else { 30 },
        )
        .await;
        pair.udp(UdpState::Available).await;
        let mut stream = TcpStream::connect(pair.tcp).await.unwrap();
        tcp_small(&mut stream).await;
        let source = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        source.send_to(&[9], LOCAL_UDP_BIND).await.unwrap();
        let (n, remote) = timeout(Duration::from_secs(3), endpoint.recv_from(&mut [0; 16]))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(n, 1);
        endpoint.send_to(&[8, 7], remote).await.unwrap();
        let mut response = [0; 16];
        let n = timeout(Duration::from_secs(3), source.recv(&mut response))
            .await
            .unwrap()
            .unwrap();
        assert!(response[..n] == [8, 7]); // explicit downstream > ingress
        if end == 2 {
            let Pair {
                host,
                guest,
                guest_id,
                bind,
                tcp,
                ..
            } = pair;
            drop(host);
            snapshot_until(&guest, |s| s.phase == "closed").await;
            guest.stop("main", &guest_id).await.unwrap();
            eof(&mut stream).await;
            echo.wait_closed().await;
            drop(std::net::UdpSocket::bind(bind).unwrap());
            drop(std::net::TcpListener::bind(tcp).unwrap());
        } else {
            if end == 0 {
                pair.host.stop("main", &pair.host_id).await.unwrap();
            } else {
                snapshot_until(&pair.guest, |s| s.phase == "closed").await;
                assert!(lab::unix_millis() >= pair.expires);
            }
            eof(&mut stream).await;
            echo.wait_closed().await;
            pair.stop().await;
        }
        drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
        echo.stop().await;
    }
}

#[tokio::test]
async fn signed_profile_grants_lifetime_and_cross_schema_are_rejected_before_network() {
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let key = lab::generate_key();
    let guest = ConnectManager::default();
    let p = guest
        .prepare_fragmented_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: key.public().to_string(),
            },
        )
        .unwrap();
    let services =
        fragmented_profile::services(target.local_addr().unwrap(), udp.local_addr().unwrap())
            .unwrap();
    let peer = p.public_id.parse().unwrap();
    let issue = |values: &[lab::Service], ttl: Duration| {
        lab::Invitation::issue_fragmented_for_lifetime(
            &key,
            peer,
            "desktop_scope",
            held.local_addr().unwrap(),
            values,
            ttl,
        )
        .unwrap()
    };
    let mut invalid = Vec::new();
    invalid.push(issue(&services, Duration::from_secs(121)).encode());
    invalid.push(issue(&services, Duration::from_millis(1500)).encode());
    let mut wrong = services.clone();
    wrong[1].udp_return_policy = lab::UdpReturnPolicy::ReplyBudget;
    invalid.push(issue(&wrong, Duration::from_secs(30)).encode());
    wrong = services.clone();
    wrong[1].max_datagram_bytes = 2048;
    invalid.push(issue(&wrong, Duration::from_secs(30)).encode());
    invalid.push(
        lab::Invitation::issue(
            &key,
            peer,
            "old",
            held.local_addr().unwrap(),
            &services[..1],
            lab::unix_millis() + 30000,
        )
        .unwrap()
        .encode(),
    );
    invalid.push(
        lab::SessionInvitation::issue_loopback_for_lifetimes(
            &key,
            peer,
            "session",
            held.local_addr().unwrap(),
            &services[..1],
            Duration::from_secs(30),
            Duration::from_secs(60),
        )
        .unwrap()
        .encode(),
    );
    for encoded in invalid {
        assert!(matches!(
            guest.review_guest("main", &p.operation_id, &encoded),
            Err(ConnectError::InvitationRejected)
        ));
    }
    let valid = issue(&services, Duration::from_secs(30));
    let mut tampered: serde_json::Value = serde_json::from_str(&valid.encode()).unwrap();
    tampered["signature"] = serde_json::json!("0".repeat(128));
    assert!(guest
        .review_guest("main", &p.operation_id, &tampered.to_string())
        .is_err());
    let review = guest
        .review_guest("main", &p.operation_id, &valid.encode())
        .unwrap();
    assert_eq!(review.lifetime_seconds, 30);
    assert!(matches!(
        guest.confirm("main", &p.operation_id, "wrong").await,
        Err(ConnectError::WrongDigest)
    ));
    held.set_nonblocking(true).unwrap();
    assert_eq!(
        held.recv(&mut [0; 2048]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(timeout(Duration::from_millis(20), target.accept())
        .await
        .is_err());
    no_udp(&udp).await;
    guest.stop("main", &p.operation_id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_during_pending_authenticated_dial_awaits_cleanup_without_udp_publication() {
    let silent = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let key = lab::generate_key();
    let guest = ConnectManager::default();
    let prepared = guest
        .prepare_fragmented_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: key.public().to_string(),
            },
        )
        .unwrap();
    let services = fragmented_profile::services(
        "127.0.0.1:32001".parse().unwrap(),
        "127.0.0.1:32002".parse().unwrap(),
    )
    .unwrap();
    let invitation = lab::Invitation::issue_fragmented_for_lifetime(
        &key,
        prepared.public_id.parse().unwrap(),
        "cancel_fragmented",
        silent.local_addr().unwrap(),
        &services,
        Duration::from_secs(30),
    )
    .unwrap();
    let review = guest
        .review_guest("main", &prepared.operation_id, &invitation.encode())
        .unwrap();
    guest
        .confirm("main", &prepared.operation_id, &review.digest)
        .await
        .unwrap();
    let (_, address) = timeout(Duration::from_secs(5), silent.recv_from(&mut [0; 2048]))
        .await
        .unwrap()
        .unwrap();
    let closed = timeout(
        Duration::from_secs(20),
        guest.stop("main", &prepared.operation_id),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(closed.cleanup_state, "complete");
    assert!(closed.tcp_address.is_none());
    let udp = closed.udp.unwrap();
    assert_eq!(udp.udp_state, UdpState::Closed);
    assert!(udp.udp_address.is_none());
    drop(std::net::UdpSocket::bind(address).unwrap());
    drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_udp_result_keeps_cancel_and_both_clocks_live_until_the_same_future_is_reaped() {
    for cause in ["cancel", "monotonic", "civil"] {
        let echo = Echo::new().await;
        let udp_target = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let reservation = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let bind = reservation.local_addr().unwrap();
        let host_key = lab::generate_key();
        let guest_key = lab::generate_key();
        let services =
            fragmented_profile::services(echo.address, udp_target.local_addr().unwrap()).unwrap();
        let invitation = lab::Invitation::issue_fragmented_for_lifetime(
            &host_key,
            guest_key.public(),
            "pending_desktop_udp",
            bind,
            &services,
            Duration::from_secs(30),
        )
        .unwrap();
        let expires = invitation.expires_at_unix_ms();
        drop(reservation);
        let host = lab::Host::start_invited_fragmented(
            lab::HostConfig {
                secret_key: host_key,
                bind_addr: bind,
                session_id: invitation.session_id().into(),
                services,
                leases: vec![invitation.lease()],
                limits: lab::Limits::default(),
            },
            &invitation,
        )
        .await
        .unwrap();
        let guest = Arc::new(
            lab::Guest::connect_invited_fragmented(guest_key, &invitation, lab::Limits::default())
                .await
                .unwrap(),
        );
        let tcp = guest
            .tcp_bridge("game", 1, "127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let tcp_address = tcp.local_addr();
        let mut stream = TcpStream::connect(tcp_address).await.unwrap();
        tcp_small(&mut stream).await;
        // A real authorized bridge is retained in a pending setup result. This
        // isolates the actor await boundary, not an uncooperative raw TLS ACK.
        let bridge = guest
            .udp_bridge_fragmented("aux", 1, LOCAL_UDP_BIND.parse().unwrap())
            .await
            .unwrap();
        assert!(!bridge.is_closed());
        let token = CancellationToken::new();
        let cancel = token.clone();
        let wall = Arc::new(std::sync::atomic::AtomicU64::new(lab::unix_millis()));
        let clock = wall.clone();
        let deadline = Instant::now()
            + if cause == "monotonic" {
                Duration::from_millis(120)
            } else {
                Duration::from_secs(30)
            };
        let (release, held) = tokio::sync::oneshot::channel();
        let (begun, entered) = tokio::sync::oneshot::channel();
        let completed = Arc::new(AtomicUsize::new(0));
        let observed = completed.clone();
        let worker_guest = guest.clone();
        let task = tokio::spawn(async move {
            await_udp_work(
                &worker_guest,
                &cancel,
                deadline,
                || clock.load(Ordering::SeqCst) < expires,
                async move {
                    begun.send(()).unwrap();
                    held.await.unwrap();
                    observed.fetch_add(1, Ordering::SeqCst);
                    bridge
                },
            )
            .await
        });
        entered.await.unwrap();
        match cause {
            "cancel" => token.cancel(),
            "civil" => wall.store(expires, Ordering::SeqCst),
            _ => {}
        }
        eof(&mut stream).await;
        echo.wait_closed().await;
        assert_eq!(completed.load(Ordering::SeqCst), 0);
        assert!(
            !task.is_finished(),
            "the exact pending future must be retained"
        );
        release.send(()).unwrap();
        let (bridge, interrupted) = timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
        assert!(interrupted && bridge.is_closed());
        assert_eq!(completed.load(Ordering::SeqCst), 1);
        bridge.shutdown().await;
        drop(bridge);
        tcp.shutdown().await;
        drop(tcp);
        guest.shutdown().await;
        drop(guest);
        host.shutdown().await;
        drop(host);
        drop(std::net::TcpListener::bind(tcp_address).unwrap());
        drop(std::net::UdpSocket::bind(LOCAL_UDP_BIND).unwrap());
        drop(std::net::UdpSocket::bind(bind).unwrap());
        echo.stop().await;
    }
}
