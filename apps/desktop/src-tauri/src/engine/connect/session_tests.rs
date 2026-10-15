//! Local schema-2 caller tests. No Java or personal configuration.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{sleep, timeout},
};

fn request(
    bind: SocketAddr,
    target: SocketAddr,
    admission: u16,
    session: u16,
) -> SessionHostRequest {
    SessionHostRequest {
        mode: ConnectMode::Loopback,
        bind_address: bind.to_string(),
        game_target: target.to_string(),
        admission_seconds: admission,
        session_seconds: session,
    }
}
async fn phase(manager: &ConnectManager, expected: &str) -> ConnectSnapshot {
    timeout(Duration::from_secs(15), async {
        loop {
            let value = manager.snapshot("main").unwrap();
            if value.phase == expected {
                return value;
            }
            assert!(value.phase != "failed", "owned manager operation failed");
            sleep(Duration::from_millis(5)).await;
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
    timing: SessionTiming,
}
impl Pair {
    async fn new(target: SocketAddr, admission: u16, session: u16) -> Self {
        let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let bind = held.local_addr().unwrap();
        let host = ConnectManager::default();
        let guest = ConnectManager::default();
        let hp = host
            .prepare_session_host("main", request(bind, target, admission, session))
            .unwrap();
        let gp = guest
            .prepare_session_guest(
                "main",
                GuestRequest {
                    mode: ConnectMode::Loopback,
                    expected_host_public_id: hp.public_id.clone(),
                },
            )
            .unwrap();
        assert_eq!(hp.invitation_profile, Some("sessionGameTcp"));
        assert_eq!(gp.invitation_profile, Some("sessionGameTcp"));
        let review = host
            .reserve_host_review("main", &hp.operation_id, &gp.public_id)
            .unwrap()
            .wait()
            .await
            .unwrap();
        let timing = review.session_timing.clone().unwrap();
        assert_eq!(timing.admission_seconds, admission);
        assert_eq!(timing.session_seconds, session);
        assert_eq!(
            review.invitation_expires_at_ms,
            timing.admission_expires_at_ms
        );
        assert!(review.expires_at_ms <= timing.admission_expires_at_ms);
        assert_eq!(review.lifetime_seconds, admission);
        assert!(matches!(
            host.confirm("main", &hp.operation_id, "wrong").await,
            Err(ConnectError::WrongDigest)
        ));
        drop(held);
        let starting = host
            .confirm("main", &hp.operation_id, &review.digest)
            .await
            .unwrap();
        assert_eq!(starting.session_timing.as_ref(), Some(&timing));
        let active = phase(&host, "hostListening").await;
        assert_eq!(active.expires_at_ms, Some(timing.session_expires_at_ms));
        let export = host.export_invitation("main", &hp.operation_id).unwrap();
        assert_eq!(export.session_timing.as_ref(), Some(&timing));
        assert_eq!(export.expires_at_ms, timing.admission_expires_at_ms);
        let review = guest
            .review_guest("main", &gp.operation_id, &export.invitation)
            .unwrap();
        assert_eq!(review.session_timing.as_ref(), Some(&timing));
        guest
            .confirm("main", &gp.operation_id, &review.digest)
            .await
            .unwrap();
        let active = phase(&guest, "guestConnected").await;
        assert_eq!(active.session_timing.as_ref(), Some(&timing));
        assert_eq!(active.expires_at_ms, Some(timing.session_expires_at_ms));
        let tcp = active.tcp_address.unwrap().parse().unwrap();
        Self {
            host,
            guest,
            host_id: hp.operation_id,
            guest_id: gp.operation_id,
            bind,
            tcp,
            timing,
        }
    }
    async fn after_ingress(&self) {
        while lab::unix_millis() <= self.timing.admission_expires_at_ms {
            sleep(Duration::from_millis(10)).await;
        }
    }
    async fn stop(&self) {
        let (g, h) = tokio::join!(
            self.guest.stop("main", &self.guest_id),
            self.host.stop("main", &self.host_id)
        );
        assert_eq!(g.unwrap().cleanup_state, "complete");
        assert_eq!(h.unwrap().cleanup_state, "complete");
        drop(std::net::TcpListener::bind(self.tcp).unwrap());
        drop(std::net::UdpSocket::bind(self.bind).unwrap());
    }
}
struct Echo {
    address: SocketAddr,
    closed: Arc<AtomicUsize>,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}
impl Echo {
    async fn new() -> Self {
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
                    next=listener.accept()=>{let (mut socket,_)=next.unwrap();let count=count.clone();children.spawn(async move {let(mut read,mut write)=socket.split();let _=tokio::io::copy(&mut read,&mut write).await;let _=write.shutdown().await;count.fetch_add(1,Ordering::SeqCst);});},
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
    async fn closed(&self, count: usize) {
        timeout(Duration::from_secs(3), async {
            while self.closed.load(Ordering::SeqCst) < count {
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
async fn transfer(stream: &mut TcpStream) {
    let data = vec![0x45; 65536];
    timeout(Duration::from_secs(3), async {
        stream.write_all(&data).await.unwrap();
        let mut reply = vec![0; data.len()];
        stream.read_exact(&mut reply).await.unwrap();
        assert!(reply == data);
    })
    .await
    .unwrap();
}

#[test]
fn session_request_requires_its_own_fields_and_preserves_legacy_limit() {
    let new = serde_json::json!({"mode":"loopback","bindAddress":"127.0.0.1:42000","gameTarget":"127.0.0.1:43001","admissionSeconds":600,"sessionSeconds":3600});
    assert!(serde_json::from_value::<SessionHostRequest>(new.clone()).is_ok());
    assert!(serde_json::from_value::<HostRequest>(new.clone()).is_err());
    for field in ["admissionSeconds", "sessionSeconds"] {
        let mut missing = new.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<SessionHostRequest>(missing).is_err());
        for value in [
            serde_json::json!(1.5),
            serde_json::json!(65536),
            serde_json::json!(-1),
            serde_json::json!(u64::MAX),
            serde_json::Value::Null,
        ] {
            let mut malformed = new.clone();
            malformed[field] = value;
            assert!(serde_json::from_value::<SessionHostRequest>(malformed).is_err());
        }
    }
    let mut extra = new;
    extra["lifetimeSeconds"] = serde_json::json!(600);
    assert!(serde_json::from_value::<SessionHostRequest>(extra).is_err());
    let old = serde_json::json!({"mode":"loopback","bindAddress":"127.0.0.1:42000","gameTarget":"127.0.0.1:43001","lifetimeSeconds":600});
    assert!(serde_json::from_value::<HostRequest>(old.clone()).is_ok());
    assert!(serde_json::from_value::<SessionHostRequest>(old).is_err());
    let caps = ConnectManager::default().capabilities();
    assert_eq!(caps.max_lifetime_seconds, 600);
    assert_eq!(caps.session_invitations.unwrap().max_session_seconds, 3600);
}

#[test]
fn admission_consumption_is_sampled_after_the_expiration_gate() {
    let consumed = std::sync::atomic::AtomicBool::new(false);
    // Models Hello consuming valid admission immediately before the clock gate
    // observes expiration. Reading consumption first would incorrectly close it.
    assert_eq!(
        admission_state(
            || {
                consumed.store(true, Ordering::SeqCst);
                false
            },
            || consumed.load(Ordering::SeqCst)
        ),
        Some(true)
    );
    assert_eq!(admission_state(|| false, || false), None);
    assert_eq!(admission_state(|| true, || false), Some(false));
}

#[tokio::test]
async fn preparation_civil_expiry_blocks_confirmation_before_monotonic_or_admission_expiry() {
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let canary = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = ConnectManager::default();
    let p = host
        .prepare_session_host(
            "main",
            request(
                held.local_addr().unwrap(),
                canary.local_addr().unwrap(),
                600,
                3600,
            ),
        )
        .unwrap();
    let review = host
        .reserve_host_review(
            "main",
            &p.operation_id,
            &lab::generate_key().public().to_string(),
        )
        .unwrap()
        .wait()
        .await
        .unwrap();
    {
        let mut state = host.live.state.lock().unwrap();
        let attempt = state.attempt.as_mut().unwrap();
        assert!(Instant::now() < attempt.prepared_deadline);
        assert!(attempt
            .reviewed
            .as_ref()
            .unwrap()
            .invitation
            .check_admission()
            .is_ok());
        expire_pending_at(attempt, p.expires_at_ms);
        assert_eq!(attempt.snapshot.phase, "closed");
    }
    assert!(matches!(
        host.confirm("main", &p.operation_id, &review.digest).await,
        Err(ConnectError::PlanExpired)
    ));
    assert_eq!(host.snapshot("main").unwrap().cleanup_state, "complete");
    held.set_nonblocking(true).unwrap();
    assert_eq!(
        held.recv(&mut [0; 2048]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(timeout(Duration::from_millis(20), canary.accept())
        .await
        .is_err());
}

#[tokio::test]
async fn signed_subsecond_durations_are_rejected_even_with_equal_remainders() {
    let bind = "127.0.0.1:32000".parse().unwrap();
    let service = game("127.0.0.1:32001".parse().unwrap());
    let key = lab::generate_key();
    let guest = ConnectManager::default();
    let p = guest
        .prepare_session_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: key.public().to_string(),
            },
        )
        .unwrap();
    for (admission, session) in [(2500, 4500), (2500, 4000), (2000, 4500)] {
        let value = lab::SessionInvitation::issue_loopback_for_lifetimes(
            &key,
            p.public_id.parse().unwrap(),
            "fractional",
            bind,
            std::slice::from_ref(&service),
            Duration::from_millis(admission),
            Duration::from_millis(session),
        )
        .unwrap();
        assert!(matches!(
            guest.review_guest("main", &p.operation_id, &value.encode()),
            Err(ConnectError::InvitationRejected)
        ));
        assert_eq!(guest.snapshot("main").unwrap().phase, "guestPrepared");
    }
    guest.stop("main", &p.operation_id).await.unwrap();
}

async fn reaped_state(state: &Mutex<State>) {
    timeout(Duration::from_secs(20), async {
        loop {
            if state.lock().unwrap().attempt.as_ref().is_some_and(terminal) {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_active_manager_eventually_closes_both_application_sockets_and_reaps_worker() {
    for drop_host in [true, false] {
        let echo = Echo::new().await;
        let Pair {
            host,
            guest,
            host_id,
            guest_id,
            bind,
            tcp,
            ..
        } = Pair::new(echo.address, 30, 60).await;
        let mut stream = TcpStream::connect(tcp).await.unwrap();
        transfer(&mut stream).await;
        let state = if drop_host {
            host.live.state.clone()
        } else {
            guest.live.state.clone()
        };
        if drop_host {
            drop(host);
            reaped_state(&state).await;
            phase(&guest, "closed").await;
            guest.stop("main", &guest_id).await.unwrap();
        } else {
            drop(guest);
            reaped_state(&state).await;
            phase(&host, "closed").await;
            host.stop("main", &host_id).await.unwrap();
        }
        assert!(matches!(
            timeout(Duration::from_secs(3), stream.read(&mut [0]))
                .await
                .unwrap(),
            Ok(0) | Err(_)
        ));
        echo.closed(1).await;
        drop(std::net::UdpSocket::bind(bind).unwrap());
        drop(std::net::TcpListener::bind(tcp).unwrap());
        echo.stop().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_manager_during_real_silent_udp_dial_awaits_failure_and_releases_endpoint() {
    let silent = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let key = lab::generate_key();
    let guest = ConnectManager::default();
    let p = guest
        .prepare_session_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: key.public().to_string(),
            },
        )
        .unwrap();
    let service = game("127.0.0.1:32001".parse().unwrap());
    let invitation = lab::SessionInvitation::issue_loopback_for_lifetimes(
        &key,
        p.public_id.parse().unwrap(),
        "silent_drop",
        silent.local_addr().unwrap(),
        &[service],
        Duration::from_secs(30),
        Duration::from_secs(60),
    )
    .unwrap();
    let review = guest
        .review_guest("main", &p.operation_id, &invitation.encode())
        .unwrap();
    guest
        .confirm("main", &p.operation_id, &review.digest)
        .await
        .unwrap();
    let (_, source) = timeout(Duration::from_secs(5), silent.recv_from(&mut [0; 2048]))
        .await
        .unwrap()
        .unwrap();
    let state = guest.live.state.clone();
    drop(guest);
    reaped_state(&state).await;
    let state = state.lock().unwrap();
    let attempt = state.attempt.as_ref().unwrap();
    assert_eq!(attempt.snapshot.cleanup_state, "complete");
    assert!(attempt.snapshot.tcp_address.is_none());
    drop(std::net::UdpSocket::bind(source).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admitted_session_keeps_existing_and_new_tcp_after_ingress_and_revokes_both_ends() {
    let echo = Echo::new().await;
    let pair = Pair::new(echo.address, 2, 12).await;
    let mut first = TcpStream::connect(pair.tcp).await.unwrap();
    transfer(&mut first).await;
    pair.after_ingress().await;
    assert_eq!(pair.guest.snapshot("main").unwrap().phase, "guestConnected");
    assert_eq!(
        pair.host.snapshot("main").unwrap().transport_state,
        "admitted"
    );
    assert!(pair.host.export_invitation("main", &pair.host_id).is_err());
    transfer(&mut first).await;
    let mut second = TcpStream::connect(pair.tcp).await.unwrap();
    transfer(&mut second).await;
    pair.host.stop("main", &pair.host_id).await.unwrap();
    for mut stream in [first, second] {
        assert!(matches!(
            timeout(Duration::from_secs(3), stream.read(&mut [0]))
                .await
                .unwrap(),
            Ok(0) | Err(_)
        ));
    }
    echo.closed(2).await;
    phase(&pair.guest, "closed").await;
    pair.stop().await;
    echo.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn absolute_session_expiry_closes_client_and_destination_despite_activity() {
    let echo = Echo::new().await;
    let pair = Pair::new(echo.address, 1, 3).await;
    let mut stream = TcpStream::connect(pair.tcp).await.unwrap();
    transfer(&mut stream).await;
    pair.after_ingress().await;
    transfer(&mut stream).await;
    assert!(matches!(
        timeout(Duration::from_secs(4), stream.read(&mut [0]))
            .await
            .unwrap(),
        Ok(0) | Err(_)
    ));
    assert!(
        lab::unix_millis() >= pair.timing.session_expires_at_ms,
        "not closed at ingress or an earlier caller timer"
    );
    echo.closed(1).await;
    phase(&pair.host, "closed").await;
    phase(&pair.guest, "closed").await;
    pair.stop().await;
    echo.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unconsumed_host_closes_at_admission_without_waiting_for_session_or_destination() {
    let canary = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = held.local_addr().unwrap();
    let host = ConnectManager::default();
    let hp = host
        .prepare_session_host("main", request(bind, canary.local_addr().unwrap(), 1, 30))
        .unwrap();
    let r = host
        .reserve_host_review(
            "main",
            &hp.operation_id,
            &lab::generate_key().public().to_string(),
        )
        .unwrap()
        .wait()
        .await
        .unwrap();
    drop(held);
    host.confirm("main", &hp.operation_id, &r.digest)
        .await
        .unwrap();
    phase(&host, "hostListening").await;
    let closed = timeout(Duration::from_secs(3), phase(&host, "closed"))
        .await
        .unwrap();
    assert_eq!(closed.cleanup_state, "complete");
    assert!(lab::unix_millis() < r.session_timing.unwrap().session_expires_at_ms);
    assert!(host.export_invitation("main", &hp.operation_id).is_err());
    assert!(timeout(Duration::from_millis(30), canary.accept())
        .await
        .is_err());
    drop(std::net::UdpSocket::bind(bind).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_after_ingress_and_shutdown_use_session_not_admission() {
    let (target, fixture) = super::super::tests::status_fixture().await;
    let pair = Pair::new(target, 1, 10).await;
    pair.after_ingress().await;
    let snapshot = pair
        .guest
        .probe_status("main", &pair.guest_id)
        .await
        .unwrap();
    assert_eq!(snapshot.status_state, "responded");
    assert_eq!(snapshot.world_state, "notVerified");
    assert_eq!(snapshot.status_summary.unwrap().reported_protocol, 767);
    pair.guest.shutdown_all().await;
    pair.host.shutdown_all().await;
    assert!(matches!(
        pair.guest.prepare_session_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: lab::generate_key().public().to_string()
            }
        ),
        Err(ConnectError::Cancelled)
    ));
    fixture.abort();
    let _ = fixture.await;
    drop(std::net::TcpListener::bind(pair.tcp).unwrap());
    drop(std::net::UdpSocket::bind(pair.bind).unwrap());
}

#[tokio::test]
async fn session_policy_and_expired_preparation_cannot_confirm_or_open_transport() {
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = held.local_addr().unwrap();
    let canary = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = canary.local_addr().unwrap();
    for (admission, session) in [(0, 30), (601, 3600), (5, 4), (600, 3601)] {
        assert!(matches!(
            ConnectManager::default()
                .prepare_session_host("main", request(bind, target, admission, session)),
            Err(ConnectError::InvalidRequest)
        ));
    }
    let host = ConnectManager::default();
    let p = host
        .prepare_session_host("main", request(bind, target, 600, 3600))
        .unwrap();
    assert!(host.snapshot("other").is_err());
    let review = host
        .reserve_host_review(
            "main",
            &p.operation_id,
            &lab::generate_key().public().to_string(),
        )
        .unwrap()
        .wait()
        .await
        .unwrap();
    assert!(review.expires_at_ms <= p.expires_at_ms);
    {
        let mut state = host.live.state.lock().unwrap();
        state
            .attempt
            .as_mut()
            .unwrap()
            .reviewed
            .as_mut()
            .unwrap()
            .review_deadline = Instant::now() - Duration::from_secs(1);
    }
    assert!(matches!(
        host.confirm("main", &p.operation_id, &review.digest).await,
        Err(ConnectError::PlanExpired)
    ));
    assert_eq!(host.snapshot("main").unwrap().cleanup_state, "complete");
    held.set_nonblocking(true).unwrap();
    assert_eq!(
        held.recv(&mut [0; 2048]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(timeout(Duration::from_millis(20), canary.accept())
        .await
        .is_err());
}

#[tokio::test]
async fn exact_schema_selection_and_signed_seconds_are_preserved_without_deadline_renewal() {
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = held.local_addr().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = game(listener.local_addr().unwrap());
    let key = lab::generate_key();
    for session_profile in [false, true] {
        let guest = ConnectManager::default();
        let req = GuestRequest {
            mode: ConnectMode::Loopback,
            expected_host_public_id: key.public().to_string(),
        };
        let p = if session_profile {
            guest.prepare_session_guest("main", req)
        } else {
            guest.prepare_guest("main", req)
        }
        .unwrap();
        let now = lab::unix_millis();
        let peer = p.public_id.parse().unwrap();
        let old = lab::Invitation::issue(
            &key,
            peer,
            "schema-caller",
            bind,
            std::slice::from_ref(&service),
            now + 30000,
        )
        .unwrap();
        let new = lab::SessionInvitation::issue_loopback_for_lifetimes(
            &key,
            peer,
            "schema-caller",
            bind,
            std::slice::from_ref(&service),
            Duration::from_secs(30),
            Duration::from_secs(60),
        )
        .unwrap();
        let wrong = if session_profile {
            old.encode()
        } else {
            new.encode()
        };
        assert!(matches!(
            guest.review_guest("main", &p.operation_id, &wrong),
            Err(ConnectError::InvitationRejected)
        ));
        sleep(Duration::from_millis(1100)).await;
        let right = if session_profile {
            new.encode()
        } else {
            old.encode()
        };
        let review = guest.review_guest("main", &p.operation_id, &right).unwrap();
        if session_profile {
            let timing = review.session_timing.unwrap();
            assert_eq!(timing.admission_seconds, 30);
            assert_eq!(timing.session_seconds, 60);
            assert_eq!(
                timing.session_expires_at_ms,
                new.session_expires_at_unix_ms()
            );
        } else {
            assert!(review.session_timing.is_none());
        }
        guest.stop("main", &p.operation_id).await.unwrap();
    }
    held.set_nonblocking(true).unwrap();
    assert_eq!(
        held.recv(&mut [0; 2048]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[tokio::test]
async fn legacy_review_bytes_and_digest_keep_their_original_shape() {
    let host = ConnectManager::default();
    let p = host
        .prepare_host(
            "main",
            HostRequest {
                mode: ConnectMode::Loopback,
                bind_address: "127.0.0.1:42000".into(),
                game_target: "127.0.0.1:43001".into(),
                lifetime_seconds: 600,
            },
        )
        .unwrap();
    assert!(serde_json::to_value(&p)
        .unwrap()
        .get("invitationProfile")
        .is_none());
    let mut r = host
        .reserve_host_review(
            "main",
            &p.operation_id,
            &lab::generate_key().public().to_string(),
        )
        .unwrap()
        .wait()
        .await
        .unwrap();
    let expected = r.digest.clone();
    r.digest = String::new();
    let bytes = serde_json::to_vec(&(Option::<(u32, u64, String)>::None, &r)).unwrap();
    assert!(format!("{:x}", Sha256::digest(&bytes)) == expected);
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json[1].as_object().unwrap().len(), 21);
    assert!(json[1].get("sessionTiming").is_none());
    let snapshot = serde_json::to_value(host.snapshot("main").unwrap()).unwrap();
    assert!(snapshot.get("invitationProfile").is_none() && snapshot.get("sessionTiming").is_none());
    assert!(matches!(
        ConnectManager::default().prepare_host(
            "main",
            HostRequest {
                mode: ConnectMode::Loopback,
                bind_address: "127.0.0.1:42000".into(),
                game_target: "127.0.0.1:43001".into(),
                lifetime_seconds: 601
            }
        ),
        Err(ConnectError::InvalidRequest)
    ));
    host.stop("main", &p.operation_id).await.unwrap();
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn discovered_session_retains_owned_process_guard_after_ingress() {
    use crate::engine::connect_discovery::{
        tests::{selection, Fixture},
        DiscoveryManager,
    };
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let target = selection(&discovery, &mut fixture).await;
    let endpoint = target.target.parse().unwrap();
    let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = held.local_addr().unwrap();
    let host = ConnectManager::default();
    let guest = ConnectManager::default();
    let hp = host
        .reserve_session_host_discovered(
            "main",
            request(bind, endpoint, 2, 20),
            target.selection_id,
            discovery.clone(),
        )
        .unwrap()
        .wait()
        .await
        .unwrap();
    let gp = guest
        .prepare_session_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: hp.public_id,
            },
        )
        .unwrap();
    let review = host
        .reserve_host_review("main", &hp.operation_id, &gp.public_id)
        .unwrap()
        .wait()
        .await
        .unwrap();
    assert_eq!(review.target_source, Some("ownedInstance"));
    let timing = review.session_timing.unwrap();
    drop(held);
    host.confirm("main", &hp.operation_id, &review.digest)
        .await
        .unwrap();
    phase(&host, "hostListening").await;
    let export = host.export_invitation("main", &hp.operation_id).unwrap();
    let review = guest
        .review_guest("main", &gp.operation_id, &export.invitation)
        .unwrap();
    guest
        .confirm("main", &gp.operation_id, &review.digest)
        .await
        .unwrap();
    let connected = phase(&guest, "guestConnected").await;
    let tcp: SocketAddr = connected.tcp_address.unwrap().parse().unwrap();
    while lab::unix_millis() <= timing.admission_expires_at_ms {
        sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        guest
            .probe_status("main", &gp.operation_id)
            .await
            .unwrap()
            .status_state,
        "responded"
    );
    fixture.stop();
    let failed = timeout(Duration::from_secs(4), async {
        loop {
            let s = host.snapshot("main").unwrap();
            if s.phase == "failed" {
                break s;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(failed.error_code, Some("connect_source_changed"));
    assert_eq!(failed.cleanup_state, "complete");
    phase(&guest, "closed").await;
    drop(std::net::UdpSocket::bind(bind).unwrap());
    drop(std::net::TcpListener::bind(tcp).unwrap());
}
