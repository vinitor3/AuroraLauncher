//! Native producer/source/Win32 observations over the own test executable.
//! These tests do not execute Java or demonstrate voice audio/mod loading.
use super::voice_fixture::{VoiceFixture, PROFILES};
use super::voice_producer_fixture::{Action, Harness, WAIT};
use super::*;
use crate::engine::connect_owned_run::{DedicatedTestHooks, OwnerTestGate};
use producer::{
    DedicatedCompletion, DedicatedPhase, DedicatedReason, DedicatedReport, DedicatedRun,
    EulaDisposition, ProducerPoint, TestHooks, TestLimits, VoiceRuntimeAnnouncement,
    VoiceRuntimeBinding, VoiceRuntimeError,
};
use std::sync::{atomic::AtomicBool, Condvar, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub(super) struct Gate {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}
impl Gate {
    pub(super) fn reach(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.0 = true;
        self.changed.notify_all();
        let (state, timed) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(20), |value| !value.1)
            .unwrap_or_else(|error| error.into_inner());
        assert!(!timed.timed_out() || state.1, "voice_runtime_gate_fallback");
    }
    pub(super) fn reached(&self) {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let (state, _) = self
            .changed
            .wait_timeout_while(state, WAIT, |value| !value.0)
            .unwrap_or_else(|error| error.into_inner());
        assert!(state.0, "voice_runtime_gate_reached");
    }
    pub(super) fn release(&self) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .1 = true;
        self.changed.notify_all();
    }
}
pub(super) struct Release(pub Arc<Gate>);
impl Drop for Release {
    fn drop(&mut self) {
        self.0.release();
    }
}
struct ReleaseOriginal(OwnerTestGate);
impl Drop for ReleaseOriginal {
    fn drop(&mut self) {
        self.0.release();
    }
}

pub(super) fn limits() -> TestLimits {
    TestLimits {
        lifetime_ms: 15_000,
        readiness_ms: 8_000,
        stop_ms: 750,
        output_bytes: 4 * 1024 * 1024,
        attempts: 8,
    }
}
pub(super) fn prepare(
    fixture: &VoiceFixture,
    harness: &Harness,
    hooks: Arc<TestHooks>,
    eula_hooks: Hooks,
    limits: TestLimits,
) -> (producer::DedicatedPlan, TestPaths, BundleId) {
    let prepared = fixture.prepared(eula_hooks);
    let paths = prepared.test_paths();
    let id = prepared.bundle_id();
    let plan = producer::plan_voice_for_test(
        prepared,
        harness.control_addr(),
        harness.nonce(),
        false,
        limits,
        hooks,
    )
    .expect("voice_runtime_actual_plan");
    assert_eq!(plan.review().purpose, "native_voice_execution_v2");
    (plan, paths, id)
}
pub(super) struct Running {
    run: Option<DedicatedRun>,
    pub completion: DedicatedCompletion,
    collected: bool,
}
impl Running {
    pub(super) fn start(plan: producer::DedicatedPlan, cancel: CancellationToken) -> Self {
        let approval = plan.review().digest.clone();
        let run = plan
            .start(&approval, cancel)
            .expect("voice_runtime_actual_start");
        let completion = run.completion();
        Self {
            run: Some(run),
            completion,
            collected: false,
        }
    }
    pub(super) fn control(&self) -> &DedicatedRun {
        self.run.as_ref().unwrap()
    }
    pub(super) fn take_control(&mut self) -> DedicatedRun {
        self.run
            .take()
            .expect("original dedicated control already handed off")
    }
    pub(super) fn restore_control(&mut self, run: DedicatedRun) {
        assert!(self.run.is_none(), "original dedicated control still owned");
        self.run = Some(run);
    }
    pub(super) fn drop_control(&mut self) {
        drop(self.run.take());
    }
    pub(super) async fn finish(self) -> DedicatedReport {
        self.finish_with_timeout(WAIT).await
    }
    pub(super) async fn finish_with_timeout(mut self, timeout: Duration) -> DedicatedReport {
        if let Some(run) = &self.run {
            run.request_stop();
        }
        let report = tokio::time::timeout(timeout, self.completion.wait())
            .await
            .expect("voice_runtime_original_completion_deadline");
        self.collected = true;
        drop(self.run.take());
        report
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        if self.collected {
            return;
        }
        let run = self.run.take();
        if let Some(run) = &run {
            run.request_stop();
        }
        let completion = self.completion.clone();
        // A failed oracle still collects this same original completion. All
        // pending query/gate RAII locals are declared after Running and drop
        // first, so unwinding cannot retain a blocked query through this join.
        let collected = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                tokio::time::timeout(Duration::from_secs(18), completion.wait()).await
            })
        })
        .join();
        drop(run);
        if !std::thread::panicking() {
            assert!(
                matches!(collected, Ok(Ok(_))),
                "voice_runtime_original_cleanup"
            );
        }
    }
}
pub(super) async fn ready(run: &DedicatedRun) {
    let deadline = Instant::now() + WAIT;
    loop {
        let snapshot = run
            .observer()
            .snapshot()
            .expect("voice_runtime_worker_alive");
        if snapshot.phase == DedicatedPhase::Ready {
            return;
        }
        assert!(
            snapshot.reason.is_none(),
            "voice_runtime_failed_before_ready: {snapshot:?}"
        );
        assert!(Instant::now() < deadline, "voice_runtime_ready_deadline");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}
pub(super) async fn completed(completion: &DedicatedCompletion) -> DedicatedReport {
    tokio::time::timeout(WAIT, completion.wait())
        .await
        .expect("voice_runtime_completion_deadline")
}
pub(super) fn restored(paths: &TestPaths, report: &DedicatedReport) {
    assert_eq!(report.eula, EulaDisposition::Restored);
    let original = report
        .process
        .as_ref()
        .expect("voice_runtime_original_report");
    assert!(original.reaped && original.readers_complete);
    assert_eq!(bytes(&target(paths)), EULA_FALSE);
    assert_eq!(bytes(&slot(paths, "applied.bin")), EULA_TRUE);
    assert!(slot(paths, "claim.json").is_file());
    unlocked(paths);
}
struct PendingQuery {
    gate: Arc<Gate>,
    task: Option<
        std::thread::JoinHandle<
            std::result::Result<producer::VoiceRuntimeSample, VoiceRuntimeError>,
        >,
    >,
}
impl PendingQuery {
    fn start(source: producer::DedicatedVoiceSource, gate: Arc<Gate>) -> Self {
        Self {
            gate,
            task: Some(std::thread::spawn(move || source.observe())),
        }
    }
    fn finish(mut self) -> std::result::Result<producer::VoiceRuntimeSample, VoiceRuntimeError> {
        self.gate.release();
        self.task
            .take()
            .unwrap()
            .join()
            .expect("voice_runtime_query_join")
    }
}
impl Drop for PendingQuery {
    fn drop(&mut self) {
        self.gate.release();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}
struct PendingTcp {
    gate: Arc<Gate>,
    task: Option<std::thread::JoinHandle<std::result::Result<(), producer::DedicatedError>>>,
}
impl PendingTcp {
    fn finish(mut self) -> std::result::Result<(), producer::DedicatedError> {
        self.gate.release();
        self.task
            .take()
            .unwrap()
            .join()
            .expect("voice_runtime_tcp_query_join")
    }
}
impl Drop for PendingTcp {
    fn drop(&mut self) {
        self.gate.release();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

fn held(
    point: ProducerPoint,
    original: DedicatedTestHooks,
) -> (Arc<TestHooks>, Arc<Gate>, Release) {
    let gate = Arc::new(Gate::default());
    let called = gate.clone();
    let hooks = Arc::new(TestHooks {
        point: Some(Arc::new(move |actual| {
            if actual == point {
                called.reach();
            }
        })),
        owned: original,
        ..TestHooks::default()
    });
    (hooks, gate.clone(), Release(gate))
}

#[tokio::test]
async fn real_voice_source_is_present_starting_then_observes_each_profile_without_mutable_inventory(
) {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let mut harness = Harness::new(fixture.stored_profile());
        let (hooks, gate, release) = held(ProducerPoint::AfterStart, DedicatedTestHooks::default());
        let (plan, paths, _) = prepare(&fixture, &harness, hooks, Hooks::default(), limits());
        let origin = files::tree(&paths.origin);
        let running = Running::start(plan, CancellationToken::new());
        let _release = release;
        harness.ready();
        gate.reached();
        let source = running
            .control()
            .voice_source()
            .expect("voice_source_supported_before_ready");
        assert_eq!(source.observe(), Err(VoiceRuntimeError::NotReady));
        gate.release();
        ready(running.control()).await;
        harness.status_observed();
        harness.udp_bytes();
        let first = source.observe().expect("voice_source_real_sequence");
        assert_eq!(first.profile, profile);
        assert_eq!(first.binding, VoiceRuntimeBinding::Ipv4Literal);
        assert_eq!(
            first.announcement,
            VoiceRuntimeAnnouncement::DefaultUnchanged
        );
        assert_eq!(
            (first.minecraft, first.loader_version),
            ("1.21.1", "0.16.14")
        );
        harness.command(Action::MutableNoise);
        assert_eq!(source.observe().unwrap(), first);
        assert!(!format!("{first:?}").contains(&paths.copy.display().to_string()));
        let report = running.finish().await;
        restored(&paths, &report);
        harness.ports_released();
        assert_eq!(source.observe(), Err(VoiceRuntimeError::Closed));
        files::assert_tree(&paths.origin, &origin);
        harness.finish();
    }
}

#[tokio::test]
async fn configuration_and_udp_errors_remain_local_while_tcp_and_status_bytes_continue() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let mut harness = Harness::new(fixture.stored_profile());
        let (plan, paths, _) = prepare(
            &fixture,
            &harness,
            Arc::new(TestHooks::default()),
            Hooks::default(),
            limits(),
        );
        let running = Running::start(plan, CancellationToken::new());
        harness.ready();
        ready(running.control()).await;
        harness.status_observed();
        let source = running.control().voice_source().unwrap();
        assert!(source.observe().is_ok());
        harness.udp_bytes();
        for (action, error) in [
            (
                Action::RewriteInvalid,
                VoiceRuntimeError::ConfigurationIncompatible,
            ),
            (
                Action::RemoveConfiguration,
                VoiceRuntimeError::ConfigurationMissing,
            ),
            (
                Action::OversizeConfiguration,
                VoiceRuntimeError::ConfigurationLimit,
            ),
        ] {
            harness.command(action);
            assert_eq!(source.observe(), Err(error));
            assert_eq!(running.control().source().observe_tcp(), Ok(()));
            let result = aurora_connect_lab::probe_minecraft_status(
                aurora_connect_lab::MinecraftStatusRequest {
                    endpoint: producer::ENDPOINT,
                    handshake_host: "127.0.0.1".into(),
                    handshake_port: 43001,
                    protocol: 767,
                    timeout: Duration::from_secs(2),
                },
            )
            .await
            .expect("voice_error_preserves_real_status_pong");
            assert!(result.protocol_matches_requested);
            assert_eq!(
                running.control().observer().snapshot().unwrap().reason,
                None
            );
            harness.command(Action::RestoreValid);
            assert!(source.observe().is_ok());
        }
        harness.command(Action::DropUdp);
        assert!(matches!(
            source.observe(),
            Err(VoiceRuntimeError::Source(_))
        ));
        assert_eq!(running.control().source().observe_tcp(), Ok(()));
        harness.command(Action::BindUdp);
        harness.udp_bytes();
        assert!(source.observe().is_ok());
        let hardlink = fixture
            .fixture
            .root
            .path()
            .join("retained-runtime-hardlink.bin");
        fs::hard_link(fixture.configuration_path(&paths), &hardlink).unwrap();
        assert_eq!(
            source.observe(),
            Err(VoiceRuntimeError::ConfigurationUnsafe)
        );
        assert_eq!(running.control().source().observe_tcp(), Ok(()));
        fs::remove_file(hardlink).unwrap();
        assert!(source.observe().is_ok());
        harness.command(Action::AddForeignMod);
        assert!(matches!(
            source.observe(),
            Err(VoiceRuntimeError::Context(_))
        ));
        harness.command(Action::RemoveForeignMod);
        assert!(source.observe().is_ok());
        let report = running.finish().await;
        restored(&paths, &report);
        harness.ports_released();
        harness.finish();
    }
}

#[tokio::test]
async fn one_real_voice_job_excludes_tcp_and_detects_identical_replacement_between_samples() {
    for profile in PROFILES {
        for action in [Action::RewriteValid, Action::RenameIdentical] {
            let fixture = VoiceFixture::new(profile);
            let mut harness = Harness::new(fixture.stored_profile());
            let (hooks, gate, release) = held(
                ProducerPoint::VoiceAfterConfigurationA,
                DedicatedTestHooks::default(),
            );
            let (plan, paths, _) = prepare(&fixture, &harness, hooks, Hooks::default(), limits());
            let running = Running::start(plan, CancellationToken::new());
            let _release = release;
            harness.ready();
            ready(running.control()).await;
            let source = running.control().voice_source().unwrap();
            let pending = PendingQuery::start(source.clone(), gate.clone());
            gate.reached();
            assert_eq!(source.observe(), Err(VoiceRuntimeError::Busy));
            assert_eq!(
                running.control().source().observe_tcp(),
                Err(producer::DedicatedError::Busy)
            );
            let before = observe_file(&fixture.configuration_path(&paths), 256 * 1024).unwrap();
            harness.command(action);
            let after = observe_file(&fixture.configuration_path(&paths), 256 * 1024).unwrap();
            match action {
                Action::RewriteValid => {
                    assert_eq!(before.0.identity, after.0.identity);
                    assert_ne!(before.1, after.1);
                }
                Action::RenameIdentical => {
                    assert_ne!(before.0.identity, after.0.identity);
                    assert_eq!(before.1, after.1);
                }
                _ => unreachable!(),
            }
            assert_eq!(
                pending.finish(),
                Err(VoiceRuntimeError::ConfigurationChanged)
            );
            assert_eq!(running.control().source().observe_tcp(), Ok(()));
            assert!(source.observe().is_ok());
            let report = running.finish().await;
            restored(&paths, &report);
            harness.ports_released();
            harness.finish();
        }
    }
}

#[tokio::test]
async fn positive_udp_query_retained_across_stop_cancel_deadline_or_drop_delays_only_final_cleanup()
{
    for profile in PROFILES {
        for reason in [
            DedicatedReason::Requested,
            DedicatedReason::Cancelled,
            DedicatedReason::Deadline,
            DedicatedReason::Dropped,
        ] {
            let fixture = VoiceFixture::new(profile);
            let mut harness = Harness::new(fixture.stored_profile());
            let original_gate = OwnerTestGate::new();
            let original = DedicatedTestHooks::new(false, None, Some(original_gate.clone()));
            let (hooks, gate, release) = held(ProducerPoint::VoiceAfterPositiveUdp, original);
            let bounded = if reason == DedicatedReason::Deadline {
                TestLimits {
                    lifetime_ms: 5_000,
                    readiness_ms: 4_000,
                    stop_ms: 500,
                    ..limits()
                }
            } else {
                limits()
            };
            let (plan, paths, _) = prepare(&fixture, &harness, hooks, Hooks::default(), bounded);
            let cancel = CancellationToken::new();
            let mut running = Running::start(plan, cancel.clone());
            let _release = release;
            let _original_release = ReleaseOriginal(original_gate.clone());
            harness.ready();
            ready(running.control()).await;
            let observer = running.control().observer();
            let source = running.control().voice_source().unwrap();
            let pending = PendingQuery::start(source.clone(), gate.clone());
            gate.reached();
            let first = running.completion.clone();
            let (waiting, started) = tokio::sync::oneshot::channel();
            let lost = tokio::spawn(async move {
                let _ = waiting.send(());
                first.wait().await
            });
            // This current-thread test resumes after the task has reached its
            // pending completion wait, rather than aborting an unpolled task.
            tokio::time::timeout(WAIT, started)
                .await
                .expect("voice_lost_waiter_start_deadline")
                .expect("voice_lost_waiter_started");
            lost.abort();
            assert!(lost.await.unwrap_err().is_cancelled());
            match reason {
                DedicatedReason::Requested => {
                    running.control().request_stop();
                }
                DedicatedReason::Cancelled => cancel.cancel(),
                DedicatedReason::Dropped => running.drop_control(),
                DedicatedReason::Deadline => {
                    let until = Instant::now() + WAIT;
                    while observer.snapshot().unwrap().reason.is_none() {
                        assert!(Instant::now() < until);
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
                _ => unreachable!(),
            }
            // At expiry83 may already be at its clipped original deadline:
            // stop_child then closes stdin without writing "stop\n" and may
            // force-reap. The original report gate below proves collection in
            // that case; the other three reasons still require the real line.
            if reason != DedicatedReason::Deadline {
                harness.stop_observed();
            }
            assert!(
                original_gate.wait_reached(WAIT),
                "voice_original_reap_and_readers_before_query_release"
            );
            harness.ports_released();
            original_gate.release();
            assert_eq!(source.observe(), Err(VoiceRuntimeError::Closed));
            assert_eq!(bytes(&target(&paths)), EULA_TRUE);
            locked(&paths);
            assert!(!slot(&paths, "restored.json").exists());
            assert!(
                tokio::time::timeout(Duration::from_millis(40), running.completion.wait())
                    .await
                    .is_err()
            );
            assert_eq!(pending.finish(), Err(VoiceRuntimeError::Closed));
            let report = running.finish().await;
            assert_eq!(report.reason, reason);
            restored(&paths, &report);
            harness.ports_released();
            harness.finish();
        }
    }
}

#[tokio::test]
async fn seal_before_reservation_or_after_configuration_error_still_returns_closed() {
    for profile in PROFILES {
        for point in [
            ProducerPoint::VoiceBeforeReserve,
            ProducerPoint::VoiceBeforeFinalGate,
        ] {
            let fixture = VoiceFixture::new(profile);
            let mut harness = Harness::new(fixture.stored_profile());
            let (hooks, gate, release) = held(point, DedicatedTestHooks::default());
            let (plan, paths, _) = prepare(&fixture, &harness, hooks, Hooks::default(), limits());
            let running = Running::start(plan, CancellationToken::new());
            let _release = release;
            harness.ready();
            ready(running.control()).await;
            let source = running.control().voice_source().unwrap();
            if point == ProducerPoint::VoiceBeforeFinalGate {
                harness.command(Action::RewriteInvalid);
            }
            let pending = PendingQuery::start(source, gate.clone());
            gate.reached();
            running.control().request_stop();
            harness.stop_observed();
            if point == ProducerPoint::VoiceBeforeReserve {
                let report = completed(&running.completion).await;
                restored(&paths, &report);
                harness.ports_released();
            } else {
                assert!(
                    tokio::time::timeout(Duration::from_millis(40), running.completion.wait())
                        .await
                        .is_err()
                );
                assert_eq!(bytes(&target(&paths)), EULA_TRUE);
            }
            assert_eq!(pending.finish(), Err(VoiceRuntimeError::Closed));
            let report = running.finish().await;
            restored(&paths, &report);
            harness.ports_released();
            harness.finish();
        }
    }
}

#[tokio::test]
async fn a_retained_positive_tcp_job_prevents_voice_reservation_in_the_same_slot() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let mut harness = Harness::new(fixture.stored_profile());
        let gate = Arc::new(Gate::default());
        let armed = Arc::new(AtomicBool::new(false));
        let in_hook = gate.clone();
        let armed_hook = armed.clone();
        let hooks = Arc::new(TestHooks {
            point: Some(Arc::new(move |point| {
                if point == ProducerPoint::AfterQuery && armed_hook.load(Ordering::Acquire) {
                    in_hook.reach();
                }
            })),
            ..TestHooks::default()
        });
        let (plan, paths, _) = prepare(&fixture, &harness, hooks, Hooks::default(), limits());
        let running = Running::start(plan, CancellationToken::new());
        let _release = Release(gate.clone());
        harness.ready();
        ready(running.control()).await;
        let voice = running.control().voice_source().unwrap();
        let tcp = running.control().source();
        armed.store(true, Ordering::Release);
        let pending = PendingTcp {
            gate: gate.clone(),
            task: Some(std::thread::spawn(move || tcp.observe_tcp())),
        };
        gate.reached();
        assert_eq!(voice.observe(), Err(VoiceRuntimeError::Busy));
        assert_eq!(pending.finish(), Ok(()));
        assert!(voice.observe().is_ok());
        let report = running.finish().await;
        restored(&paths, &report);
        harness.ports_released();
        harness.finish();
    }
}
