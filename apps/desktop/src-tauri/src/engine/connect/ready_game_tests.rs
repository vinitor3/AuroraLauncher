//! Real local Host/Guest authorization with a controlled native child, never Java.
use super::*;
use crate::engine::connect::{
    PreparedReadyGame, ReadyGameAppearance, ReadyGameBackend, ReadyGameLaunchError as Error,
    ReadyGameLaunchRequest, ReadyGameLaunchResult, ReadyGameLaunchSnapshot, ReadyGameSpawnGate,
};
use std::{
    future::Future,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Condvar,
    },
    task::{Context, Poll, Waker},
};
use tokio::time::{sleep, timeout};

#[derive(Default)]
struct Gate {
    entered: AtomicBool,
    released: Mutex<bool>,
    release: Condvar,
}
impl Gate {
    fn wait(&self) {
        self.entered.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut open = self.released.lock().unwrap();
        while !*open {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "owned fixture gate was not released");
            let (next, _) = self.release.wait_timeout(open, remaining).unwrap();
            open = next;
        }
    }
    fn open(&self) {
        *self.released.lock().unwrap() = true;
        self.release.notify_all();
    }
    async fn arrived(&self) {
        timeout(Duration::from_secs(5), async {
            while !self.entered.load(Ordering::SeqCst) {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
}
#[derive(Default)]
struct FixtureBackend {
    profile: Option<crate::engine::connect_ready_profile::ReadyProfile>,
    before_prepare: Option<Arc<Gate>>,
    before_spawn: Option<Arc<Gate>>,
    after_spawn: Option<Arc<Gate>>,
    spawns: AtomicUsize,
    child: Mutex<Option<Child>>,
    panic_prepare: bool,
    panic_after_spawn: bool,
    prepare_wait_cancel: bool,
}
fn fixture_backend(change: impl FnOnce(&mut FixtureBackend)) -> Arc<FixtureBackend> {
    let mut backend = FixtureBackend::default();
    change(&mut backend);
    Arc::new(backend)
}
struct FixturePrepared {
    backend: Arc<FixtureBackend>,
    instance: String,
}
impl ReadyGameBackend for Arc<FixtureBackend> {
    fn prepare(
        &self,
        request: &ReadyGameLaunchRequest,
        _: SocketAddr,
        cancel: &CancellationToken,
    ) -> Result<Box<dyn PreparedReadyGame>, Error> {
        if let Some(gate) = &self.before_prepare {
            gate.wait();
        }
        if self.prepare_wait_cancel {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !cancel.is_cancelled() {
                assert!(
                    Instant::now() < deadline,
                    "owned watchdog must signal cancelled backend"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
            return Err(Error::Cancelled);
        }
        assert!(!self.panic_prepare, "intentional_owned_fixture_failure");
        Ok(Box::new(FixturePrepared {
            backend: self.clone(),
            instance: request.instance_id.clone(),
        }))
    }
}
impl PreparedReadyGame for FixturePrepared {
    fn profile(&self) -> crate::engine::connect_ready_profile::ReadyProfile {
        self.backend
            .profile
            .unwrap_or(crate::engine::connect_ready_profile::ReadyProfile::Fabric1211)
    }
    fn fingerprint(&self) -> &str {
        "fixture-readiness-proof"
    }
    fn core_sha256(&self) -> &str {
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }
    fn skins_sha256(&self) -> &str {
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    }
    fn launch(
        self: Box<Self>,
        gate: &ReadyGameSpawnGate,
        _: &CancellationToken,
    ) -> Result<ReadyGameLaunchResult, Error> {
        if let Some(wait) = &self.backend.before_spawn {
            wait.wait();
        }
        let mut tracked = self.backend.child.lock().unwrap();
        let result = gate.with_current(|| {
            let child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "engine::connect::live::ready_game_tests::ready_game_controlled_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("AURORA_READY_GAME_CHILD", "owned-native-fixture")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| Error::LaunchFailed)?;
            let pid = child.id();
            *tracked = Some(child);
            self.backend.spawns.fetch_add(1, Ordering::SeqCst);
            Ok(ReadyGameLaunchResult {
                run_id: uuid::Uuid::new_v4().to_string(),
                process_id: pid,
                instance_id: self.instance.clone(),
                world_state: "notVerified",
            })
        });
        drop(tracked);
        assert!(
            !self.backend.panic_after_spawn,
            "intentional_post_spawn_failure"
        );
        if result.is_ok() {
            if let Some(wait) = &self.backend.after_spawn {
                wait.wait();
            }
        }
        result
    }
}
impl Drop for FixtureBackend {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.get_mut().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
#[test]
#[ignore = "owned fixture child invoked only by the parent test"]
fn ready_game_controlled_child() {
    assert_eq!(
        std::env::var("AURORA_READY_GAME_CHILD").unwrap(),
        "owned-native-fixture"
    );
    std::thread::sleep(Duration::from_secs(10));
}
struct Pair {
    host: ConnectManager,
    guest: Arc<ConnectManager>,
    host_id: String,
    guest_id: String,
    host_public_id: String,
    tcp: SocketAddr,
    backend: tokio::task::JoinHandle<()>,
    _aux: Option<std::net::UdpSocket>,
}
impl Pair {
    async fn new() -> Self {
        Self::with_duration(120).await
    }
    async fn with_duration(session_seconds: u16) -> Self {
        Self::with_profile(Profile::Session, session_seconds).await
    }
    async fn with_profile(profile: Profile, session_seconds: u16) -> Self {
        let (target, backend) = crate::engine::connect::tests::status_fixture().await;
        let hold = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let aux = profile
            .is_fragmented()
            .then(|| std::net::UdpSocket::bind("127.0.0.1:0").unwrap());
        let host = ConnectManager::default();
        let guest = Arc::new(ConnectManager::default());
        let hp = match profile {
            Profile::Session => host.prepare_session_host(
                "main",
                SessionHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: hold.local_addr().unwrap().to_string(),
                    game_target: target.to_string(),
                    admission_seconds: 60.min(session_seconds),
                    session_seconds,
                },
            ),
            Profile::Legacy => host.prepare_host(
                "main",
                HostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: hold.local_addr().unwrap().to_string(),
                    game_target: target.to_string(),
                    lifetime_seconds: session_seconds,
                },
            ),
            Profile::Fragmented => host.prepare_fragmented_host(
                "main",
                FragmentedHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: hold.local_addr().unwrap().to_string(),
                    game_target: target.to_string(),
                    udp_target: aux.as_ref().unwrap().local_addr().unwrap().to_string(),
                    lifetime_seconds: session_seconds,
                },
            ),
            Profile::SessionFragmented => host.prepare_session_fragmented_host(
                "main",
                SessionFragmentedHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: hold.local_addr().unwrap().to_string(),
                    game_target: target.to_string(),
                    udp_target: aux.as_ref().unwrap().local_addr().unwrap().to_string(),
                    admission_seconds: 60.min(session_seconds),
                    session_seconds,
                },
            ),
        }
        .unwrap();
        let request = GuestRequest {
            mode: ConnectMode::Loopback,
            expected_host_public_id: hp.public_id.clone(),
        };
        let gp = match profile {
            Profile::Session => guest.prepare_session_guest("main", request),
            Profile::Legacy => guest.prepare_guest("main", request),
            Profile::Fragmented => guest.prepare_fragmented_guest("main", request),
            Profile::SessionFragmented => guest.prepare_session_fragmented_guest("main", request),
        }
        .unwrap();
        let review = host
            .reserve_host_review("main", &hp.operation_id, &gp.public_id)
            .unwrap()
            .wait()
            .await
            .unwrap();
        drop(hold);
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
        let tcp = phase(&guest, "guestConnected")
            .await
            .tcp_address
            .unwrap()
            .parse()
            .unwrap();
        Self {
            host,
            guest,
            host_id: hp.operation_id,
            guest_id: gp.operation_id,
            host_public_id: hp.public_id,
            tcp,
            backend,
            _aux: aux,
        }
    }
    fn transport_ports(&self) -> Vec<SocketAddr> {
        let state = self.guest.live.state.lock().unwrap();
        state
            .attempt
            .as_ref()
            .unwrap()
            .guest_source
            .as_ref()
            .unwrap()
            .bound_sockets_for_test()
    }
    fn request(&self) -> ReadyGameLaunchRequest {
        ReadyGameLaunchRequest {
            operation_id: self.guest_id.clone(),
            instance_id: format!("aurora-test-{}", uuid::Uuid::new_v4()),
            source_instance_id: "source-fixture".into(),
            context_key: "current-ui-owner".into(),
            java_executable: "C:/fixture/java.exe".into(),
            nickname: "FixturePlayer".into(),
            appearance: ReadyGameAppearance {
                skin_url: None,
                skin_file: None,
                cape_url: None,
                skin_model: None,
            },
        }
    }
    fn prepare(&self, backend: Arc<FixtureBackend>) -> ReadyGameLaunchSnapshot {
        self.guest
            .prepare_ready_game_launch("main", self.request(), Arc::new(backend))
            .unwrap()
    }
    async fn close(self) {
        timeout(
            Duration::from_secs(5),
            self.guest.stop("main", &self.guest_id),
        )
        .await
        .unwrap()
        .unwrap();
        timeout(
            Duration::from_secs(5),
            self.host.stop("main", &self.host_id),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(tokio::net::TcpListener::bind(self.tcp).await.is_ok());
        self.backend.abort();
        let _ = self.backend.await;
    }
}
async fn phase(manager: &ConnectManager, expected: &str) -> ConnectSnapshot {
    timeout(Duration::from_secs(10), async {
        loop {
            let s = manager.snapshot("main").unwrap();
            if s.phase == expected {
                return s;
            }
            assert_ne!(s.phase, "failed");
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}
async fn launch_phase(pair: &Pair, expected: &str) -> ReadyGameLaunchSnapshot {
    timeout(Duration::from_secs(5), async {
        loop {
            let s = pair
                .guest
                .ready_game_launch_snapshot("main", &pair.guest_id)
                .unwrap();
            if s.phase == expected {
                return s;
            }
            assert!(
                !matches!(s.phase, "failed" | "cancelled" | "expired"),
                "unexpected launch state: {}",
                s.phase
            );
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}
fn pending<T>(future: std::pin::Pin<&mut impl Future<Output = T>>) {
    assert!(matches!(
        future.poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_review_projects_the_verified_fabric1201_runtime() {
    let pair = Pair::new().await;
    let backend = fixture_backend(|backend| {
        backend.profile = Some(crate::engine::connect_ready_profile::ReadyProfile::Fabric1201)
    });
    pair.prepare(backend.clone());
    let review = launch_phase(&pair, "reviewed").await.review.unwrap();
    assert_eq!(review.minecraft_version, "1.20.1");
    assert_eq!(review.loader, "fabric");
    assert_eq!(review.loader_version, "0.16.14");
    assert_eq!(review.java_major, 17);
    assert_eq!(review.core_version, "1.1.5");
    assert_eq!(review.skins_version, "0.3.1");
    assert_eq!(backend.spawns.load(Ordering::SeqCst), 0);
    pair.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_real_guest_explicit_digest_starts_one_owned_child_and_stop_preserves_game() {
    let pair = Pair::new().await;
    let backend = Arc::new(FixtureBackend::default());
    let initial = pair.prepare(backend.clone());
    assert_eq!(initial.phase, "preparing");
    let review = launch_phase(&pair, "reviewed").await.review.unwrap();
    assert_eq!(review.game_address, pair.tcp.to_string());
    assert_eq!(backend.spawns.load(Ordering::SeqCst), 0);
    assert_eq!(
        pair.guest
            .confirm_ready_game_launch("other", &pair.guest_id, &review.plan_id, &review.digest)
            .unwrap_err(),
        Error::WrongOwner
    );
    assert_eq!(
        pair.guest
            .confirm_ready_game_launch("main", &pair.guest_id, &review.plan_id, "wrong")
            .unwrap_err(),
        Error::ApprovalMismatch
    );
    assert!(pair
        .guest
        .prepare_ready_game_launch("main", pair.request(), Arc::new(backend.clone()))
        .is_err());
    pair.guest
        .confirm_ready_game_launch("main", &pair.guest_id, &review.plan_id, &review.digest)
        .unwrap();
    let started = launch_phase(&pair, "started").await;
    for _ in 0..3 {
        pair.guest
            .confirm_ready_game_launch("main", &pair.guest_id, &review.plan_id, &review.digest)
            .unwrap();
    }
    assert_eq!(started.result.unwrap().world_state, "notVerified");
    assert_eq!(backend.spawns.load(Ordering::SeqCst), 1);
    let result = pair
        .guest
        .cancel_ready_game_launch("main", &pair.guest_id, &review.plan_id)
        .await
        .unwrap();
    assert_eq!(result.phase, "started");
    pair.guest.stop("main", &pair.guest_id).await.unwrap();
    assert!(backend
        .child
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .try_wait()
        .unwrap()
        .is_none());
    pair.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_lost_prepare_and_stop_waiter_do_not_detach_pending_backend() {
    let pair = Pair::new().await;
    let gate = Arc::new(Gate::default());
    let backend = fixture_backend(|backend| backend.before_prepare = Some(gate.clone()));
    drop(pair.prepare(backend.clone()));
    gate.arrived().await;
    let mut first = Box::pin(pair.guest.stop("main", &pair.guest_id));
    pending(first.as_mut());
    drop(first);
    let mut logout = Box::pin(pair.guest.shutdown_all());
    pending(logout.as_mut());
    assert_eq!(backend.spawns.load(Ordering::SeqCst), 0);
    gate.open();
    timeout(Duration::from_secs(5), logout).await.unwrap();
    assert_eq!(
        pair.guest
            .ready_game_launch_snapshot("main", &pair.guest_id)
            .unwrap()
            .phase,
        "cancelled"
    );
    pair.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_stop_between_confirmation_and_gate_forbids_late_spawn() {
    let pair = Pair::new().await;
    let gate = Arc::new(Gate::default());
    let backend = fixture_backend(|backend| backend.before_spawn = Some(gate.clone()));
    pair.prepare(backend.clone());
    let review = launch_phase(&pair, "reviewed").await.review.unwrap();
    pair.guest
        .confirm_ready_game_launch("main", &pair.guest_id, &review.plan_id, &review.digest)
        .unwrap();
    gate.arrived().await;
    let mut stop = Box::pin(pair.guest.stop("main", &pair.guest_id));
    pending(stop.as_mut());
    gate.open();
    timeout(Duration::from_secs(5), stop)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(backend.spawns.load(Ordering::SeqCst), 0);
    pair.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_completed_child_publication_still_retains_worker_tail_for_logout() {
    let pair = Pair::new().await;
    let gate = Arc::new(Gate::default());
    let backend = fixture_backend(|backend| backend.after_spawn = Some(gate.clone()));
    pair.prepare(backend.clone());
    let review = launch_phase(&pair, "reviewed").await.review.unwrap();
    pair.guest
        .confirm_ready_game_launch("main", &pair.guest_id, &review.plan_id, &review.digest)
        .unwrap();
    gate.arrived().await;
    assert_eq!(
        pair.guest
            .ready_game_launch_snapshot("main", &pair.guest_id)
            .unwrap()
            .phase,
        "started"
    );
    let mut logout = Box::pin(pair.guest.shutdown_all());
    pending(logout.as_mut());
    gate.open();
    timeout(Duration::from_secs(5), logout).await.unwrap();
    assert!(backend
        .child
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .try_wait()
        .unwrap()
        .is_none());
    pair.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_closed_transport_with_pending_worker_tail_stops_without_losing_publication() {
    let pair = Pair::new().await;
    let gate = Arc::new(Gate::default());
    let backend = fixture_backend(|backend| backend.after_spawn = Some(gate.clone()));
    pair.prepare(backend.clone());
    let review = launch_phase(&pair, "reviewed").await.review.unwrap();
    pair.guest
        .confirm_ready_game_launch("main", &pair.guest_id, &review.plan_id, &review.digest)
        .unwrap();
    gate.arrived().await;
    timeout(
        Duration::from_secs(5),
        pair.host.stop("main", &pair.host_id),
    )
    .await
    .unwrap()
    .unwrap();
    let closed = phase(&pair.guest, "closed").await;
    assert_eq!(closed.cleanup_state, "pending");
    let mut stop = Box::pin(pair.guest.stop("main", &pair.guest_id));
    pending(stop.as_mut());
    let still_closed = pair.guest.snapshot("main").unwrap();
    assert_eq!(still_closed.phase, "closed");
    assert_eq!(still_closed.cleanup_state, "pending");
    gate.open();
    let stopped = timeout(Duration::from_secs(5), stop)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stopped.phase, "closed");
    assert_eq!(stopped.cleanup_state, "complete");
    assert_eq!(backend.spawns.load(Ordering::SeqCst), 1);
    assert!(backend
        .child
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .try_wait()
        .unwrap()
        .is_none());
    pair.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_original_bridge_closed_without_port_replacement_cannot_launch() {
    let pair = Pair::new().await;
    let gate = Arc::new(Gate::default());
    let backend = fixture_backend(|backend| backend.before_spawn = Some(gate.clone()));
    pair.prepare(backend.clone());
    let review = launch_phase(&pair, "reviewed").await.review.unwrap();
    pair.guest
        .confirm_ready_game_launch("main", &pair.guest_id, &review.plan_id, &review.digest)
        .unwrap();
    gate.arrived().await;
    let bridge = {
        let state = pair.guest.live.state.lock().unwrap();
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
    bridge.shutdown().await;
    let replacement = tokio::net::TcpListener::bind(pair.tcp).await.unwrap();
    gate.open();
    timeout(Duration::from_secs(5), async {
        loop {
            let s = pair
                .guest
                .ready_game_launch_snapshot("main", &pair.guest_id)
                .unwrap();
            if s.phase == "failed" {
                assert_eq!(s.error_code, Some(Error::Unavailable.code()));
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(backend.spawns.load(Ordering::SeqCst), 0);
    drop(replacement);
    pair.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_preparation_panic_is_recorded_without_poisoning_transport() {
    let pair = Pair::new().await;
    let backend = fixture_backend(|backend| backend.panic_prepare = true);
    pair.prepare(backend.clone());
    timeout(Duration::from_secs(5), async {
        loop {
            let s = pair
                .guest
                .ready_game_launch_snapshot("main", &pair.guest_id)
                .unwrap();
            if s.phase == "failed" {
                assert_eq!(s.error_code, Some(Error::Internal.code()));
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(pair.guest.snapshot("main").unwrap().phase, "guestConnected");
    assert_eq!(backend.spawns.load(Ordering::SeqCst), 0);
    pair.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_post_spawn_panic_preserves_tracked_child_result_and_never_relaunches() {
    let pair = Pair::new().await;
    let backend = fixture_backend(|backend| backend.panic_after_spawn = true);
    pair.prepare(backend.clone());
    let review = launch_phase(&pair, "reviewed").await.review.unwrap();
    pair.guest
        .confirm_ready_game_launch("main", &pair.guest_id, &review.plan_id, &review.digest)
        .unwrap();
    let result = launch_phase(&pair, "started").await.result.unwrap();
    pair.guest
        .cancel_ready_game_launch("main", &pair.guest_id, &review.plan_id)
        .await
        .unwrap();
    let retained = pair
        .guest
        .ready_game_launch_snapshot("main", &pair.guest_id)
        .unwrap();
    assert_eq!(retained.phase, "started");
    assert_eq!(retained.result.unwrap().process_id, result.process_id);
    pair.guest
        .confirm_ready_game_launch("main", &pair.guest_id, &review.plan_id, &review.digest)
        .unwrap();
    assert_eq!(backend.spawns.load(Ordering::SeqCst), 1);
    pair.guest.stop("main", &pair.guest_id).await.unwrap();
    assert!(backend
        .child
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .try_wait()
        .unwrap()
        .is_none());
    pair.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_original_session_deadline_expires_before_spawn_without_renewal() {
    let pair = Pair::with_duration(5).await;
    let gate = Arc::new(Gate::default());
    let backend = fixture_backend(|backend| backend.before_spawn = Some(gate.clone()));
    pair.prepare(backend.clone());
    let review = launch_phase(&pair, "reviewed").await.review.unwrap();
    assert!(review.expires_at_ms <= review.session_expires_at_ms);
    assert!(review.session_expires_at_ms - review.expires_at_ms < 100);
    pair.guest
        .confirm_ready_game_launch("main", &pair.guest_id, &review.plan_id, &review.digest)
        .unwrap();
    gate.arrived().await;
    sleep(Duration::from_millis(
        review
            .session_expires_at_ms
            .saturating_sub(lab::unix_millis())
            + 100,
    ))
    .await;
    gate.open();
    let expired = launch_phase(&pair, "expired").await;
    assert_eq!(expired.error_code, Some(Error::Expired.code()));
    assert_eq!(backend.spawns.load(Ordering::SeqCst), 0);
    pair.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_owned_watchdog_cancels_blocking_preparation_and_preserves_expired_outcome() {
    let pair = Pair::new().await;
    let backend = fixture_backend(|backend| backend.prepare_wait_cancel = true);
    pair.prepare(backend.clone());
    {
        let mut state = pair.guest.live.state.lock().unwrap();
        ready_game::expire_preparation_for_test(state.attempt.as_mut().unwrap());
    }
    let expired = launch_phase(&pair, "expired").await;
    assert_eq!(expired.error_code, Some(Error::Expired.code()));
    assert_eq!(backend.spawns.load(Ordering::SeqCst), 0);
    assert_eq!(pair.guest.snapshot("main").unwrap().phase, "guestConnected");
    pair.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_guest_source_stop_releases_transport_with_manager_alive_without_new_preparation(
) {
    let pair = Pair::new().await;
    let status = pair
        .guest
        .probe_status("main", &pair.guest_id)
        .await
        .unwrap();
    assert_eq!(status.status_state, "responded");
    // Only addresses leave this lock. Retaining a Guest or GuestSource here
    // would manufacture the ownership failure this test is meant to expose.
    let ports = {
        let state = pair.guest.live.state.lock().unwrap();
        let attempt = state.attempt.as_ref().unwrap();
        assert!(attempt.ready_game.is_none());
        attempt
            .guest_source
            .as_ref()
            .unwrap()
            .bound_sockets_for_test()
    };
    assert!(!ports.is_empty());
    let stopped = timeout(
        Duration::from_secs(5),
        pair.guest.stop("main", &pair.guest_id),
    )
    .await
    .unwrap()
    .unwrap();
    let rebound = timeout(Duration::from_secs(2), async {
        loop {
            let sockets: std::io::Result<Vec<_>> =
                ports.iter().map(std::net::UdpSocket::bind).collect();
            if let Ok(sockets) = sockets {
                break sockets;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    // Observe the OS while this exact manager and terminal Attempt remain
    // alive. No new preparation or manager drop may release the old Endpoint.
    let udp_rebound_while_manager_alive = rebound.is_ok();
    drop(rebound);
    let tcp_rebound_while_manager_alive = std::net::TcpListener::bind(pair.tcp).is_ok();
    let source_removed = pair
        .guest
        .live
        .state
        .lock()
        .unwrap()
        .attempt
        .as_ref()
        .unwrap()
        .guest_source
        .is_none();
    // The red baseline must still reap every fixture before its expected
    // assertion fails. Pair::close keeps the current manager until cleanup.
    pair.close().await;
    assert_eq!(stopped.cleanup_state, "complete");
    assert!(tcp_rebound_while_manager_alive);
    assert!(
        udp_rebound_while_manager_alive,
        "Guest transport UDP remained bound after cleanup complete with manager alive"
    );
    assert!(source_removed);
}

async fn rebound_guest_sockets(ports: &[SocketAddr]) -> Option<Vec<std::net::UdpSocket>> {
    timeout(Duration::from_secs(2), async {
        loop {
            let sockets: std::io::Result<Vec<_>> =
                ports.iter().map(std::net::UdpSocket::bind).collect();
            if let Ok(sockets) = sockets {
                break sockets;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_guest_source_retained_workers_do_not_retain_transport_or_detach_on_lost_waiter()
{
    for stage in 0..3 {
        let pair = Pair::new().await;
        let ports = pair.transport_ports();
        assert!(!ports.is_empty());
        assert_eq!(
            pair.guest
                .probe_status("main", &pair.guest_id)
                .await
                .unwrap()
                .status_state,
            "responded"
        );
        let gate = Arc::new(Gate::default());
        let backend = fixture_backend(|backend| match stage {
            0 => backend.before_prepare = Some(gate.clone()),
            1 => backend.before_spawn = Some(gate.clone()),
            2 => backend.after_spawn = Some(gate.clone()),
            _ => unreachable!(),
        });
        pair.prepare(backend.clone());
        if stage != 0 {
            let review = launch_phase(&pair, "reviewed").await.review.unwrap();
            pair.guest
                .confirm_ready_game_launch("main", &pair.guest_id, &review.plan_id, &review.digest)
                .unwrap();
        }
        gate.arrived().await;
        let manager = pair.guest.clone();
        let id = pair.guest_id.clone();
        let waiter = tokio::spawn(async move { manager.stop("main", &id).await });
        let closed = phase(&pair.guest, "closed").await;
        // These are observations while the exact blocking worker is still
        // held. Its source/gate must not keep a strong Guest alive.
        let rebound = rebound_guest_sockets(&ports).await;
        let udp_released_before_worker = rebound.is_some();
        drop(rebound);
        let tcp_released_before_worker = std::net::TcpListener::bind(pair.tcp).is_ok();
        let pending_waiter = !waiter.is_finished();
        waiter.abort();
        let lost_waiter = matches!(waiter.await, Err(error) if error.is_cancelled());
        let pending = pair.guest.snapshot("main").unwrap();
        let source_removed = pair
            .guest
            .live
            .state
            .lock()
            .unwrap()
            .attempt
            .as_ref()
            .unwrap()
            .guest_source
            .is_none();
        let new_preparation_refused = matches!(
            pair.guest.prepare_session_guest(
                "main",
                GuestRequest {
                    mode: ConnectMode::Loopback,
                    expected_host_public_id: pair.host_public_id.clone(),
                }
            ),
            Err(ConnectError::Busy)
        );
        let child_alive_while_worker_held = stage != 2
            || backend
                .child
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .try_wait()
                .unwrap()
                .is_none();
        gate.open();
        let complete = timeout(
            Duration::from_secs(5),
            pair.guest.stop("main", &pair.guest_id),
        )
        .await
        .unwrap()
        .unwrap();
        let outcome = pair
            .guest
            .ready_game_launch_snapshot("main", &pair.guest_id)
            .unwrap();
        let child_alive_after_stop = stage != 2
            || backend
                .child
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .try_wait()
                .unwrap()
                .is_none();
        pair.close().await;
        assert_eq!(closed.cleanup_state, "pending");
        assert_eq!(pending.cleanup_state, "pending");
        assert!(udp_released_before_worker && tcp_released_before_worker);
        assert!(pending_waiter && lost_waiter && source_removed && new_preparation_refused);
        assert_eq!(complete.cleanup_state, "complete");
        assert_eq!(
            backend.spawns.load(Ordering::SeqCst),
            usize::from(stage == 2)
        );
        assert_eq!(
            outcome.phase,
            if stage == 2 { "started" } else { "cancelled" }
        );
        assert!(child_alive_while_worker_held && child_alive_after_stop);
        assert_eq!(outcome.result.is_some(), stage == 2);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_guest_source_expiry_or_remote_revocation_releases_transport_without_new_attempt(
) {
    for remote_revocation in [false, true] {
        let pair = Pair::with_duration(if remote_revocation { 30 } else { 2 }).await;
        let ports = pair.transport_ports();
        assert!(!ports.is_empty());
        assert_eq!(
            pair.guest
                .probe_status("main", &pair.guest_id)
                .await
                .unwrap()
                .status_state,
            "responded"
        );
        let before = pair.guest.snapshot("main").unwrap();
        if remote_revocation {
            // Revoke the actual original lease through its Host. No cancel
            // token or snapshot is used to manufacture the remote close.
            let state = pair.host.live.state.lock().unwrap();
            let (host, lease) = state.attempt.as_ref().unwrap().host.as_ref().unwrap();
            assert!(host.revoke(lease));
        }
        let closed = phase(&pair.guest, "closed").await;
        let rebound = rebound_guest_sockets(&ports).await;
        let udp_released = rebound.is_some();
        drop(rebound);
        let tcp_released = std::net::TcpListener::bind(pair.tcp).is_ok();
        let after = pair.guest.snapshot("main").unwrap();
        let source_removed = pair
            .guest
            .live
            .state
            .lock()
            .unwrap()
            .attempt
            .as_ref()
            .unwrap()
            .guest_source
            .is_none();
        pair.close().await;
        assert_eq!(closed.phase, "closed");
        assert_eq!(after.operation_id, before.operation_id);
        assert_eq!(after.session_timing, before.session_timing);
        assert!(udp_released && tcp_released && source_removed);
        if !remote_revocation {
            assert!(lab::unix_millis() >= before.session_timing.unwrap().session_expires_at_ms);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ready_game_guest_source_shared_actor_releases_legacy_and_fragmented_profiles() {
    for profile in [
        Profile::Legacy,
        Profile::Fragmented,
        Profile::SessionFragmented,
    ] {
        let pair = Pair::with_profile(profile, 30).await;
        let mut ports = pair.transport_ports();
        assert!(!ports.is_empty());
        if profile.is_fragmented() {
            let local_udp = timeout(Duration::from_secs(5), async {
                loop {
                    let snapshot = pair.guest.snapshot("main").unwrap();
                    if let Some(udp) = snapshot.udp {
                        if udp.udp_state == fragmented_profile::UdpState::Available {
                            break udp.udp_address.unwrap().parse().unwrap();
                        }
                    }
                    sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            ports.push(local_udp);
        }
        assert_eq!(
            pair.guest
                .probe_status("main", &pair.guest_id)
                .await
                .unwrap()
                .status_state,
            "responded"
        );
        let stopped = timeout(
            Duration::from_secs(5),
            pair.guest.stop("main", &pair.guest_id),
        )
        .await
        .unwrap()
        .unwrap();
        let rebound = rebound_guest_sockets(&ports).await;
        let udp_released = rebound.is_some();
        drop(rebound);
        let tcp_released = std::net::TcpListener::bind(pair.tcp).is_ok();
        pair.close().await;
        assert_eq!(stopped.cleanup_state, "complete");
        assert!(udp_released && tcp_released);
    }
}
