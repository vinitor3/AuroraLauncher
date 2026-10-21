//! The synthetic executable uses the real factory, original Child supervisor,
//! Win32 TCP observation, STATUS codec and EULA transaction.
use super::super::producer::{
    self, DedicatedCompletion, DedicatedError, DedicatedFailure, DedicatedObserver, DedicatedPhase,
    DedicatedPlan, DedicatedReason, DedicatedReport, EulaDisposition, FixtureMode, ProducerPoint,
    TestHooks, TestLimits, TestRecipe,
};
use super::producer_fixture::{Action, Harness, WAIT};
use super::*;
use crate::engine::connect_owned_run::{
    DedicatedTestHooks, OwnedRunError, OwnedRunFailure, OwnedRunReason, OwnerTestGate,
};
use aurora_connect_lab::MinecraftStatusError;
use std::sync::{atomic::AtomicBool, Condvar, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Gate {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}

impl Gate {
    fn reach(&self) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.0 = true;
        self.changed.notify_all();
        let (state, timed) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(20), |s| !s.1)
            .unwrap_or_else(|p| p.into_inner());
        assert!(!timed.timed_out() || state.1, "producer_test_gate_fallback");
    }
    fn reached(&self) {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let (state, _) = self
            .changed
            .wait_timeout_while(state, WAIT, |s| !s.0)
            .unwrap_or_else(|p| p.into_inner());
        assert!(state.0, "producer_test_gate_reached");
    }
    fn release(&self) {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).1 = true;
        self.changed.notify_all();
    }
}

struct Release(Arc<Gate>);
impl Drop for Release {
    fn drop(&mut self) {
        self.0.release();
    }
}

fn held(point: ProducerPoint) -> (Arc<TestHooks>, Arc<Gate>, Release) {
    let gate = Arc::new(Gate::default());
    let called = gate.clone();
    let hooks = Arc::new(TestHooks {
        point: Some(Arc::new(move |actual| {
            if actual == point {
                called.reach();
            }
        })),
        ..TestHooks::default()
    });
    (hooks, gate.clone(), Release(gate))
}

fn limits() -> TestLimits {
    TestLimits {
        lifetime_ms: 15_000,
        readiness_ms: 8_000,
        stop_ms: 750,
        output_bytes: 4 * 1024 * 1024,
        attempts: 8,
    }
}

fn prepare(
    fixture: &Fixture,
    harness: &Harness,
    mode: FixtureMode,
    hooks: Arc<TestHooks>,
    limits: TestLimits,
) -> (DedicatedPlan, TestPaths, BundleId) {
    let prepared = fixture.prepared(Hooks::default());
    let paths = prepared.test_paths();
    let id = prepared.bundle_id();
    let recipe = TestRecipe {
        control: harness.control_addr(),
        nonce: Uuid::parse_str(harness.nonce()).expect("producer_fixture_nonce"),
        mode,
    };
    let plan = producer::plan_for_test(prepared, recipe, limits, hooks)
        .expect("producer_fixture_execution_plan");
    assert_eq!(plan.review().purpose, "native_dedicated_status_execution");
    (plan, paths, id)
}

async fn ready(observer: &DedicatedObserver) {
    let deadline = Instant::now() + WAIT;
    loop {
        let snapshot = observer.snapshot().expect("producer_snapshot_owned");
        if snapshot.phase == DedicatedPhase::Ready {
            return;
        }
        assert!(
            snapshot.reason.is_none(),
            "producer_ready_failed: {snapshot:?}"
        );
        assert!(Instant::now() < deadline, "producer_ready_timeout");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn completed(completion: &DedicatedCompletion) -> DedicatedReport {
    tokio::time::timeout(WAIT, completion.wait())
        .await
        .expect("producer_completion_deadline")
}

async fn weak_observer_released(observer: &DedicatedObserver) {
    let deadline = Instant::now() + WAIT;
    while let Some(snapshot) = observer.snapshot() {
        // Sending completion may wake its waiter just before the worker's
        // final Arc drops. Resource release must already have been proved.
        assert_eq!(snapshot.phase, DedicatedPhase::Complete);
        assert!(Instant::now() < deadline, "producer_worker_weak_release");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn restored(paths: &TestPaths, report: &DedicatedReport) {
    assert_eq!(report.eula, EulaDisposition::Restored);
    assert_eq!(bytes(&target(paths)), EULA_FALSE);
    assert_eq!(bytes(&slot(paths, "snapshot.bin")), EULA_FALSE);
    assert_eq!(bytes(&slot(paths, "applied.bin")), EULA_TRUE);
    assert!(slot(paths, "claim.json").is_file());
    assert!(slot(paths, "may-have-spawned.json").is_file());
    assert!(!slot(paths, "old.bin").exists());
    let process = report
        .process
        .as_ref()
        .expect("producer_original_process_report");
    assert!(process.reaped && process.readers_complete);
    unlocked(paths);
    Harness::assert_status_port_released();
}

#[tokio::test]
async fn native_synthetic_status_and_normal_stop_restore_after_original_completion() {
    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let (plan, paths, _) = prepare(
        &fixture,
        &harness,
        FixtureMode::Normal,
        Arc::new(TestHooks::default()),
        limits(),
    );
    let source_before = files::tree(&paths.origin);
    let stage_before = files::tree(&fixture.stage);
    let jdk_before = files::tree(&fixture.jdk);
    let approval = plan.review().digest.clone();
    let run = plan
        .start(&approval, CancellationToken::new())
        .expect("producer_start");
    let observer = run.observer();
    let source = run.source();
    harness.ready();
    ready(&observer).await;
    harness.status_observed(true);
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    locked(&paths);
    assert_eq!(source.observe_tcp(), Ok(()));
    let completion = run.request_stop();
    assert_eq!(source.observe_tcp(), Err(DedicatedError::Closed));
    harness.stop_observed();
    let report = completed(&completion).await;
    assert_eq!(report.reason, DedicatedReason::Requested);
    assert_eq!(report.failure, None);
    assert!(report.reached_ready);
    let process = report.process.as_ref().expect("producer_original_report");
    assert_eq!(process.reason, OwnedRunReason::Requested);
    assert_eq!(process.failure, None);
    assert!(!process.forced);
    restored(&paths, &report);
    assert_eq!(completed(&run.request_stop()).await, report);
    files::assert_tree(&paths.origin, &source_before);
    files::assert_tree(&fixture.stage, &stage_before);
    files::assert_tree(&fixture.jdk, &jdk_before);
    drop(run);
    assert_eq!(source.observe_tcp(), Err(DedicatedError::Closed));
    weak_observer_released(&observer).await;
    harness.finish();
}

#[tokio::test]
async fn lost_waiter_does_not_restore_while_the_same_child_is_held() {
    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let (plan, paths, _) = prepare(
        &fixture,
        &harness,
        FixtureMode::HoldStop,
        Arc::new(TestHooks::default()),
        limits(),
    );
    let approval = plan.review().digest.clone();
    let run = plan
        .start(&approval, CancellationToken::new())
        .expect("producer_start");
    harness.ready();
    ready(&run.observer()).await;
    harness.status_observed(true);
    let completion = run.request_stop();
    let first = completion.clone();
    let waiter = tokio::spawn(async move { first.wait().await });
    harness.stop_observed();
    waiter.abort();
    assert!(waiter
        .await
        .expect_err("producer_lost_waiter")
        .is_cancelled());
    assert!(
        tokio::time::timeout(Duration::from_millis(40), completion.wait())
            .await
            .is_err()
    );
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    assert_eq!(bytes(&slot(&paths, "old.bin")), EULA_FALSE);
    assert!(!slot(&paths, "restored.json").exists());
    locked(&paths);
    assert!(
        std::net::TcpListener::bind(super::producer_fixture::ENDPOINT).is_err(),
        "producer_original_listener_still_owned"
    );
    harness.send(Action::ReleaseStop);
    let report = completed(&run.request_stop()).await;
    assert_eq!(report.reason, DedicatedReason::Requested);
    assert_eq!(report.failure, None);
    restored(&paths, &report);
    assert_eq!(completed(&completion).await, report);
    drop(run);
    harness.finish();
}

#[tokio::test]
async fn dropping_the_control_after_requested_stop_preserves_the_normal_outcome() {
    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let (plan, paths, _) = prepare(
        &fixture,
        &harness,
        FixtureMode::HoldStop,
        Arc::new(TestHooks::default()),
        limits(),
    );
    let approval = plan.review().digest.clone();
    let run = plan
        .start(&approval, CancellationToken::new())
        .expect("producer_start");
    let observer = run.observer();
    harness.ready();
    ready(&observer).await;
    harness.status_observed(true);
    let completion = run.request_stop();
    harness.stop_observed();
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    locked(&paths);
    assert!(
        std::net::TcpListener::bind(super::producer_fixture::ENDPOINT).is_err(),
        "producer_child_is_still_owned_before_control_drop"
    );
    drop(run);
    let sealed = observer
        .snapshot()
        .expect("producer_worker_survives_control_drop");
    assert_eq!(sealed.phase, DedicatedPhase::Stopping);
    assert_eq!(sealed.reason, Some(DedicatedReason::Requested));
    assert_eq!(sealed.failure, None);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), completion.wait())
            .await
            .is_err()
    );
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    assert!(!slot(&paths, "restored.json").exists());
    harness.send(Action::ReleaseStop);
    let report = completed(&completion).await;
    assert_eq!(report.reason, DedicatedReason::Requested);
    assert_eq!(report.failure, None);
    let process = report.process.as_ref().expect("producer_original_report");
    assert_eq!(process.reason, OwnedRunReason::Requested);
    assert_eq!(process.failure, None);
    assert!(!process.forced);
    restored(&paths, &report);
    weak_observer_released(&observer).await;
    assert_eq!(completed(&completion).await, report);
    harness.finish();
}

#[tokio::test]
async fn stop_after_the_spawn_intent_requires_recovery_even_without_a_child_report() {
    let fixture = Fixture::new();
    let harness = Harness::new();
    let gate = Arc::new(Gate::default());
    let _release = Release(gate.clone());
    let intercept = gate.clone();
    let prepared = fixture.prepared(Hooks {
        after_step: Some(Arc::new(move |step| {
            if step == Step::MayHaveSpawned {
                intercept.reach();
            }
            Ok(())
        })),
        ..Hooks::default()
    });
    let paths = prepared.test_paths();
    let id = prepared.bundle_id();
    let hooks = Arc::new(TestHooks::default());
    let recipe = TestRecipe {
        control: harness.control_addr(),
        nonce: Uuid::parse_str(harness.nonce()).expect("producer_fixture_nonce"),
        mode: FixtureMode::Normal,
    };
    let plan = producer::plan_for_test(prepared, recipe, limits(), hooks.clone())
        .expect("producer_execution_review");
    let approval = plan.review().digest.clone();
    let run = plan
        .start(&approval, CancellationToken::new())
        .expect("producer_worker_started");
    gate.reached();
    assert!(slot(&paths, "may-have-spawned.json").is_file());
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    assert_eq!(hooks.owned.spawn_attempts(), 0);
    harness.assert_no_child();
    let journal_before = files::tree(&paths.transaction);
    let completion = run.request_stop();
    gate.release();
    let report = completed(&completion).await;
    assert_eq!(report.reason, DedicatedReason::Requested);
    assert_eq!(
        report.failure,
        Some(DedicatedFailure::Preparation(Error::NeedsRecovery))
    );
    assert_eq!(report.eula, EulaDisposition::NeedsRecovery);
    assert_eq!(report.process, None);
    assert!(!report.reached_ready);
    assert_eq!(hooks.owned.spawn_attempts(), 0);
    harness.assert_no_child();
    files::assert_tree(&paths.transaction, &journal_before);
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    unlocked(&paths);
    Harness::assert_status_port_released();
    // A test counter is not restore authority. Recovery still needs a new
    // review and the real mutation guard through its existing private probe.
    let probe = probe::Probe::default();
    let recovery = fixture
        .recover(id, &probe, Hooks::default())
        .expect("producer_explicit_recovery_plan");
    let recovery_approval = recovery.review().digest.clone();
    assert_eq!(
        recovery.confirm(&recovery_approval),
        Ok(EulaOutcome::Restored)
    );
    assert!(probe.calls() >= 3);
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    assert!(slot(&paths, "claim.json").is_file());
    assert!(slot(&paths, "may-have-spawned.json").is_file());
    drop(run);
    harness.finish();
}

#[tokio::test]
async fn a_late_real_status_never_publishes_ready_after_stop_cancel_or_drop() {
    for reason in [
        DedicatedReason::Requested,
        DedicatedReason::Cancelled,
        DedicatedReason::Dropped,
    ] {
        let fixture = Fixture::new();
        let mut harness = Harness::new();
        let (hooks, gate, _release) = held(ProducerPoint::BeforeReady);
        let (plan, paths, _) = prepare(&fixture, &harness, FixtureMode::Normal, hooks, limits());
        let approval = plan.review().digest.clone();
        let token = CancellationToken::new();
        let mut run = Some(
            plan.start(&approval, token.clone())
                .expect("producer_start"),
        );
        let observer = run.as_ref().expect("producer_control").observer();
        let completion = run.as_ref().expect("producer_control").completion();
        harness.ready();
        gate.reached();
        harness.status_observed(true);
        assert_eq!(
            observer.snapshot().expect("producer_snapshot").phase,
            DedicatedPhase::Starting
        );
        match reason {
            DedicatedReason::Requested => {
                run.as_ref().expect("producer_control").request_stop();
            }
            DedicatedReason::Cancelled => token.cancel(),
            DedicatedReason::Dropped => drop(run.take()),
            _ => unreachable!(),
        }
        let sealed = observer
            .snapshot()
            .expect("producer_worker_still_owns_state");
        assert_eq!(sealed.phase, DedicatedPhase::Stopping);
        assert_eq!(sealed.reason, Some(reason));
        assert_eq!(bytes(&target(&paths)), EULA_TRUE);
        assert!(
            tokio::time::timeout(Duration::from_millis(30), completion.wait())
                .await
                .is_err()
        );
        gate.release();
        let report = completed(&completion).await;
        assert_eq!(report.reason, reason);
        assert!(!report.reached_ready);
        assert_eq!(
            report.failure,
            match reason {
                DedicatedReason::Requested => None,
                DedicatedReason::Cancelled => Some(DedicatedFailure::Cancelled),
                DedicatedReason::Dropped => Some(DedicatedFailure::Dropped),
                _ => unreachable!(),
            }
        );
        restored(&paths, &report);
        drop(run);
        harness.finish();
    }
}

#[tokio::test]
async fn malformed_status_or_pong_never_becomes_readiness_and_still_cleans_up() {
    for (mode, failure) in [
        (FixtureMode::BadProtocol, DedicatedFailure::Protocol),
        (
            FixtureMode::BadPong,
            DedicatedFailure::Status(MinecraftStatusError::PongMismatch),
        ),
        (FixtureMode::StatusTimeout, DedicatedFailure::Readiness),
    ] {
        let fixture = Fixture::new();
        let mut harness = Harness::new();
        let bounded = TestLimits {
            attempts: 2,
            ..limits()
        };
        let (plan, paths, _) = prepare(
            &fixture,
            &harness,
            mode,
            Arc::new(TestHooks::default()),
            bounded,
        );
        let approval = plan.review().digest.clone();
        let run = plan
            .start(&approval, CancellationToken::new())
            .expect("producer_start");
        harness.ready();
        harness.status_observed(false);
        let report = completed(&run.completion()).await;
        assert!(!report.reached_ready);
        assert_eq!(report.failure, Some(failure));
        restored(&paths, &report);
        drop(run);
        harness.finish();
    }
}

#[tokio::test]
async fn first_truncated_status_retries_the_same_child_before_validated_readiness() {
    use std::sync::atomic::AtomicUsize;

    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let second_query = Arc::new(Gate::default());
    let before_ready = Arc::new(Gate::default());
    let _release_second = Release(second_query.clone());
    let _release_ready = Release(before_ready.clone());
    let query_calls = Arc::new(AtomicUsize::new(0));
    let second_reached = Arc::new(AtomicBool::new(false));
    let ready_reached = Arc::new(AtomicBool::new(false));
    let held_second = second_query.clone();
    let held_ready = before_ready.clone();
    let counted_queries = query_calls.clone();
    let reached_second = second_reached.clone();
    let reached_ready = ready_reached.clone();
    let original = DedicatedTestHooks::default();
    let hooks = Arc::new(TestHooks {
        owned: original.clone(),
        point: Some(Arc::new(move |point| match point {
            // Count only a successful original Win32 observation. Startup
            // attempts before the listener exists cannot advance this gate.
            ProducerPoint::AfterQuery => {
                if counted_queries.fetch_add(1, Ordering::SeqCst) == 1 {
                    reached_second.store(true, Ordering::Release);
                    held_second.reach();
                }
            }
            ProducerPoint::BeforeReady => {
                reached_ready.store(true, Ordering::Release);
                held_ready.reach();
            }
            _ => {}
        })),
        ..TestHooks::default()
    });
    let (plan, paths, _) = prepare(
        &fixture,
        &harness,
        FixtureMode::TruncateFirstStatus,
        hooks,
        limits(),
    );
    let source_before = files::tree(&paths.origin);
    let stage_before = files::tree(&fixture.stage);
    let jdk_before = files::tree(&fixture.jdk);
    let digest = plan.review().digest.clone();
    let run = plan
        .start(&digest, CancellationToken::new())
        .expect("producer_start");
    let observer = run.observer();
    let completion = run.completion();
    harness.ready();
    harness.status_observed(false);
    let first_counts = harness.status_counts();

    // The old policy seals with Status(Truncated) here. Collect that original
    // completion before making the RED assertion, rather than panicking while
    // its Child/transaction may still be owned by the worker.
    let deadline = Instant::now() + WAIT;
    while !second_reached.load(Ordering::Acquire)
        && observer
            .snapshot()
            .is_some_and(|state| state.reason.is_none())
        && Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let second_was_reached = second_reached.load(Ordering::Acquire);
    let before_second = observer.snapshot();
    let eula_pending_before_second = fs::read(target(&paths)).is_ok_and(|bytes| bytes == EULA_TRUE);
    let copy_busy_before_second = matches!(
        crate::engine::acquire_instance_recovery_lock(&fixture::copy_instance(&paths)),
        Err(crate::engine::ContentUpdateError::Busy)
    );
    let first_counts_before_retry = harness.status_counts();
    second_query.release();

    if second_was_reached {
        let deadline = Instant::now() + WAIT;
        while !ready_reached.load(Ordering::Acquire)
            && observer
                .snapshot()
                .is_some_and(|state| state.reason.is_none())
            && Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    let validated_before_ready = ready_reached.load(Ordering::Acquire);
    if validated_before_ready {
        harness.status_observed(true);
    }
    let before_publication = observer.snapshot();
    let final_counts = harness.status_counts();
    before_ready.release();

    let deadline = Instant::now() + WAIT;
    let became_ready = loop {
        match observer.snapshot() {
            Some(state) if state.phase == DedicatedPhase::Ready => break true,
            Some(state) if state.reason.is_none() && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            _ => break false,
        }
    };
    run.request_stop();
    let report = completed(&completion).await;
    // Cleanup is checked even on the expected old-policy RED. No test helper
    // restores EULA or fabricates process completion from flags/counters.
    restored(&paths, &report);
    files::assert_tree(&paths.origin, &source_before);
    files::assert_tree(&fixture.stage, &stage_before);
    files::assert_tree(&fixture.jdk, &jdk_before);
    let attempts = original.spawn_attempts();
    drop(run);
    harness.finish();

    assert_eq!(report.failure, None, "same_child_truncated_retry_report");
    assert_eq!(report.reason, DedicatedReason::Requested);
    assert!(became_ready && report.reached_ready);
    assert_eq!(attempts, 1, "truncated_retry_must_not_spawn_another_child");
    assert_eq!(first_counts, (1, 0));
    assert_eq!(first_counts_before_retry, (1, 0));
    assert!(second_was_reached && eula_pending_before_second && copy_busy_before_second);
    assert!(before_second.is_some_and(|state| {
        state.phase == DedicatedPhase::Starting && state.reason.is_none()
    }));
    assert!(validated_before_ready);
    assert!(before_publication.is_some_and(|state| {
        state.phase == DedicatedPhase::Starting && state.reason.is_none()
    }));
    assert_eq!(final_counts, (2, 1));
    assert_eq!(query_calls.load(Ordering::SeqCst), 3);
    let process = report.process.expect("producer_original_process_report");
    assert_eq!(process.reason, OwnedRunReason::Requested);
    assert_eq!(process.failure, None);
    assert_eq!(process.exit_code, Some(0));
    assert!(!process.forced);
}

#[tokio::test]
async fn an_eula_conflict_after_ready_remains_pending_without_overwrite() {
    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let (plan, paths, id) = prepare(
        &fixture,
        &harness,
        FixtureMode::Normal,
        Arc::new(TestHooks::default()),
        limits(),
    );
    let approval = plan.review().digest.clone();
    let run = plan
        .start(&approval, CancellationToken::new())
        .expect("producer_start");
    harness.ready();
    ready(&run.observer()).await;
    harness.status_observed(true);
    let independent = b"# independent writer while the dedicated child is live\neula=true\n";
    fs::write(target(&paths), independent).expect("producer_independent_edit");
    let report = completed(&run.request_stop()).await;
    assert_eq!(report.eula, EulaDisposition::NeedsRecovery);
    assert_eq!(bytes(&target(&paths)), independent);
    assert_eq!(bytes(&slot(&paths, "old.bin")), EULA_FALSE);
    assert_eq!(bytes(&slot(&paths, "snapshot.bin")), EULA_FALSE);
    assert!(report
        .process
        .as_ref()
        .is_some_and(|p| p.reaped && p.readers_complete));
    unlocked(&paths);
    Harness::assert_status_port_released();
    let before = files::tree(&paths.bundle);
    assert!(fixture
        .recover(id, &probe::Probe::default(), Hooks::default())
        .is_err());
    files::assert_tree(&paths.bundle, &before);
    drop(run);
    harness.finish();
}

#[tokio::test]
async fn external_cancel_stops_the_child_during_status_io_without_a_snapshot() {
    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let (plan, paths, _) = prepare(
        &fixture,
        &harness,
        FixtureMode::StatusTimeout,
        Arc::new(TestHooks::default()),
        limits(),
    );
    let approval = plan.review().digest.clone();
    let cancel = CancellationToken::new();
    let run = plan
        .start(&approval, cancel.clone())
        .expect("producer_start");
    let completion = run.completion();
    harness.ready();
    harness.status_observed(false);
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    cancel.cancel();
    // STATUS has a five-second I/O timeout here. Releasing the actual listener
    // earlier proves cancellation reaches the child independently of snapshot
    // refresh, request_stop or completion polling.
    let earlier_than_status_timeout = Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(listener) = std::net::TcpListener::bind(super::producer_fixture::ENDPOINT) {
            drop(listener);
            break;
        }
        assert!(
            Instant::now() < earlier_than_status_timeout,
            "producer_cancel_must_not_wait_for_status_timeout"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let report = completed(&completion).await;
    assert_eq!(report.reason, DedicatedReason::Cancelled);
    assert_eq!(report.failure, Some(DedicatedFailure::Cancelled));
    assert!(!report.reached_ready);
    restored(&paths, &report);
    drop(run);
    harness.finish();
}

#[tokio::test]
async fn rejected_execution_approval_cannot_apply_eula_or_dispatch_a_child() {
    for choice in [
        "digest",
        "file-only-review",
        "expired",
        "cancelled",
        "drift",
    ] {
        let fixture = Fixture::new();
        let mut harness = Harness::new();
        let hooks = Arc::new(TestHooks::default());
        let prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        let file_only_approval = prepared.review().digest.clone();
        let recipe = TestRecipe {
            control: harness.control_addr(),
            nonce: Uuid::parse_str(harness.nonce()).expect("producer_fixture_nonce"),
            mode: FixtureMode::Normal,
        };
        let mut plan = producer::plan_for_test(prepared, recipe, limits(), hooks.clone())
            .expect("producer_execution_review");
        assert_ne!(plan.review().digest, file_only_approval);
        let cancel = CancellationToken::new();
        let approval = match choice {
            "digest" => "0".repeat(64),
            "file-only-review" => file_only_approval,
            "expired" => {
                plan.expire_for_test();
                plan.review().digest.clone()
            }
            "cancelled" => {
                cancel.cancel();
                plan.review().digest.clone()
            }
            "drift" => {
                fs::write(
                    paths.copy.join("server.properties"),
                    b"independent configuration\n",
                )
                .expect("producer_approval_drift");
                plan.review().digest.clone()
            }
            _ => unreachable!(),
        };
        let before = files::tree(&paths.bundle);
        let error = plan
            .start(&approval, cancel)
            .err()
            .expect("producer_start_must_refuse");
        assert_eq!(
            error,
            match choice {
                "digest" | "file-only-review" => DedicatedError::Preparation(Error::Digest),
                "expired" => DedicatedError::Preparation(Error::Expired),
                "cancelled" => DedicatedError::Cancelled,
                "drift" => DedicatedError::Preparation(Error::Changed),
                _ => unreachable!(),
            }
        );
        files::assert_tree(&paths.bundle, &before);
        assert_eq!(bytes(&target(&paths)), EULA_FALSE);
        assert!(!slot(&paths, "claim.json").exists());
        assert_eq!(hooks.owned.spawn_attempts(), 0);
        harness.assert_no_child();
        unlocked(&paths);

        // Calibrate the exact same dispatch counter and control listener with
        // a fresh, genuinely approved bundle through the actual core path.
        let (valid, valid_paths, _) = prepare(
            &fixture,
            &harness,
            FixtureMode::Normal,
            hooks.clone(),
            limits(),
        );
        let valid_approval = valid.review().digest.clone();
        let run = valid
            .start(&valid_approval, CancellationToken::new())
            .expect("producer_positive_start");
        harness.ready();
        ready(&run.observer()).await;
        harness.status_observed(true);
        assert_eq!(hooks.owned.spawn_attempts(), 1);
        let report = completed(&run.request_stop()).await;
        assert_eq!(report.failure, None);
        restored(&valid_paths, &report);
        assert_eq!(hooks.owned.spawn_attempts(), 1);
        drop(run);
        harness.finish();
    }
}

#[tokio::test]
async fn initialization_before_any_eula_or_child_effect_releases_its_preparation() {
    for fail_thread in [true, false] {
        let fixture = Fixture::new();
        let harness = Harness::new();
        let hooks = Arc::new(TestHooks {
            fail_thread,
            fail_runtime: !fail_thread,
            ..TestHooks::default()
        });
        let (plan, paths, _) = prepare(
            &fixture,
            &harness,
            FixtureMode::Normal,
            hooks.clone(),
            limits(),
        );
        let before = files::tree(&paths.bundle);
        let approval = plan.review().digest.clone();
        if fail_thread {
            assert_eq!(
                plan.start(&approval, CancellationToken::new()).err(),
                Some(DedicatedError::Thread)
            );
        } else {
            let run = plan
                .start(&approval, CancellationToken::new())
                .expect("producer_worker_owned");
            let report = completed(&run.completion()).await;
            assert_eq!(report.failure, Some(DedicatedFailure::Runtime));
            assert_eq!(report.eula, EulaDisposition::Unchanged);
            assert!(!report.reached_ready);
            assert_eq!(report.process, None);
            drop(run);
        }
        assert_eq!(hooks.owned.spawn_attempts(), 0);
        files::assert_tree(&paths.bundle, &before);
        assert!(!slot(&paths, "claim.json").exists());
        assert_eq!(bytes(&target(&paths)), EULA_FALSE);
        harness.assert_no_child();
        unlocked(&paths);
        harness.finish();
    }
}

#[tokio::test]
async fn stop_between_open_check_and_query_reservation_remains_a_normal_request() {
    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let (hooks, gate, _release) = held(ProducerPoint::BeforeReserve);
    let (plan, paths, _) = prepare(&fixture, &harness, FixtureMode::Normal, hooks, limits());
    let approval = plan.review().digest.clone();
    let run = plan
        .start(&approval, CancellationToken::new())
        .expect("producer_start");
    harness.ready();
    gate.reached();
    let completion = run.request_stop();
    harness.stop_observed();
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), completion.wait())
            .await
            .is_err()
    );
    gate.release();
    let report = completed(&completion).await;
    assert_eq!(report.reason, DedicatedReason::Requested);
    assert_eq!(report.failure, None);
    assert!(!report.reached_ready);
    restored(&paths, &report);
    drop(run);
    harness.finish();
}

#[tokio::test]
async fn child_exit_and_aggregate_output_failure_stay_failures_after_eula_restoration() {
    for choice in ["exit-zero", "exit-seven", "stdout", "stderr", "both"] {
        let fixture = Fixture::new();
        let mut harness = Harness::new();
        let (plan, paths, _) = prepare(
            &fixture,
            &harness,
            FixtureMode::Normal,
            Arc::new(TestHooks::default()),
            limits(),
        );
        let approval = plan.review().digest.clone();
        let run = plan
            .start(&approval, CancellationToken::new())
            .expect("producer_start");
        harness.ready();
        ready(&run.observer()).await;
        harness.status_observed(true);
        harness.send(match choice {
            "exit-zero" => Action::ExitZero,
            "exit-seven" => Action::ExitSeven,
            "stdout" => Action::FloodOut,
            "stderr" => Action::FloodErr,
            "both" => Action::FloodBoth,
            _ => unreachable!(),
        });
        let report = completed(&run.completion()).await;
        let expected = if choice.starts_with("exit") {
            OwnedRunFailure::EarlyExit
        } else {
            OwnedRunFailure::OutputLimit
        };
        assert_eq!(report.failure, Some(DedicatedFailure::Child(expected)));
        assert!(report.reached_ready);
        let process = report
            .process
            .as_ref()
            .expect("producer_original_process_report");
        assert_eq!(process.failure, Some(expected));
        if choice == "both" {
            assert!(process.stdout_bytes <= 4 * 1024 * 1024);
            assert!(process.stderr_bytes <= 4 * 1024 * 1024);
            assert!(process.stdout_bytes + process.stderr_bytes > 4 * 1024 * 1024);
        }
        restored(&paths, &report);
        drop(run);
        harness.finish();
    }
}

#[tokio::test]
async fn a_forced_child_stop_is_not_promoted_to_success_by_restored_eula() {
    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let (plan, paths, _) = prepare(
        &fixture,
        &harness,
        FixtureMode::IgnoreStop,
        Arc::new(TestHooks::default()),
        limits(),
    );
    let approval = plan.review().digest.clone();
    let run = plan
        .start(&approval, CancellationToken::new())
        .expect("producer_start");
    harness.ready();
    ready(&run.observer()).await;
    harness.status_observed(true);
    let completion = run.request_stop();
    harness.stop_observed();
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    locked(&paths);
    let report = completed(&completion).await;
    assert_eq!(report.reason, DedicatedReason::Requested);
    assert_eq!(
        report.failure,
        Some(DedicatedFailure::Child(OwnedRunFailure::Forced))
    );
    assert!(
        report
            .process
            .as_ref()
            .expect("producer_original_report")
            .forced
    );
    restored(&paths, &report);
    drop(run);
    harness.finish();
}

struct ReleaseOriginal(OwnerTestGate);
impl Drop for ReleaseOriginal {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[tokio::test]
async fn original_reader_and_original_report_must_finish_before_eula_restore() {
    for reader_final in [true, false] {
        let fixture = Fixture::new();
        let mut harness = Harness::new();
        let gate = OwnerTestGate::new();
        let _release = ReleaseOriginal(gate.clone());
        let hooks = Arc::new(TestHooks {
            owned: DedicatedTestHooks::new(
                false,
                reader_final.then(|| gate.clone()),
                (!reader_final).then(|| gate.clone()),
            ),
            ..TestHooks::default()
        });
        let (plan, paths, _) = prepare(&fixture, &harness, FixtureMode::Normal, hooks, limits());
        let approval = plan.review().digest.clone();
        let run = plan
            .start(&approval, CancellationToken::new())
            .expect("producer_start");
        harness.ready();
        ready(&run.observer()).await;
        harness.status_observed(true);
        let completion = run.request_stop();
        harness.stop_observed();
        assert!(gate.wait_reached(WAIT), "producer_original_cleanup_gate");
        Harness::assert_status_port_released();
        assert_eq!(bytes(&target(&paths)), EULA_TRUE);
        assert_eq!(bytes(&slot(&paths, "old.bin")), EULA_FALSE);
        assert!(!slot(&paths, "restored.json").exists());
        locked(&paths);
        assert!(
            tokio::time::timeout(Duration::from_millis(40), completion.wait())
                .await
                .is_err()
        );
        gate.release();
        let report = completed(&completion).await;
        assert_eq!(report.reason, DedicatedReason::Requested);
        assert_eq!(report.failure, None);
        restored(&paths, &report);
        drop(run);
        harness.finish();
    }
}

struct HeldSource {
    gate: Arc<Gate>,
    worker: Option<std::thread::JoinHandle<std::result::Result<(), DedicatedError>>>,
}
impl HeldSource {
    fn finish(mut self) -> std::result::Result<(), DedicatedError> {
        self.gate.release();
        self.worker
            .take()
            .expect("producer_source_worker")
            .join()
            .expect("producer_source_worker_join")
    }
}
impl Drop for HeldSource {
    fn drop(&mut self) {
        // Release before join even when an earlier oracle panics. This worker
        // owns the real reserved84 query, not a replacement provider result.
        self.gate.release();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[tokio::test]
async fn a_reserved_positive_source_query_blocks_restore_but_not_child_cleanup() {
    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let gate = Arc::new(Gate::default());
    let _release = Release(gate.clone());
    let original_report = OwnerTestGate::new();
    let _release_original = ReleaseOriginal(original_report.clone());
    let armed = Arc::new(AtomicBool::new(false));
    let called = gate.clone();
    let armed_hook = armed.clone();
    let hooks = Arc::new(TestHooks {
        point: Some(Arc::new(move |point| {
            if point == ProducerPoint::AfterQuery && armed_hook.load(Ordering::Acquire) {
                called.reach();
            }
        })),
        owned: DedicatedTestHooks::new(false, None, Some(original_report.clone())),
        ..TestHooks::default()
    });
    let (plan, paths, _) = prepare(&fixture, &harness, FixtureMode::Normal, hooks, limits());
    let approval = plan.review().digest.clone();
    let run = plan
        .start(&approval, CancellationToken::new())
        .expect("producer_start");
    harness.ready();
    ready(&run.observer()).await;
    harness.status_observed(true);
    let source = run.source();
    armed.store(true, Ordering::Release);
    let queried = source.clone();
    let worker = std::thread::spawn(move || queried.observe_tcp());
    let pending = HeldSource {
        gate: gate.clone(),
        worker: Some(worker),
    };
    gate.reached();
    // The gate runs only after the same original Win32 query returned Ok.
    // Its83 job has ended; the enclosing84 source job is still registered.
    assert_eq!(source.observe_tcp(), Err(DedicatedError::Busy));
    let completion = run.request_stop();
    harness.stop_observed();
    assert!(
        original_report.wait_reached(WAIT),
        "producer_reaped_and_readers_joined_before_source_release"
    );
    Harness::assert_status_port_released();
    original_report.release();
    assert_eq!(source.observe_tcp(), Err(DedicatedError::Closed));
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    assert_eq!(bytes(&slot(&paths, "old.bin")), EULA_FALSE);
    assert!(!slot(&paths, "restored.json").exists());
    locked(&paths);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), completion.wait())
            .await
            .is_err()
    );
    assert_eq!(pending.finish(), Err(DedicatedError::Closed));
    let report = completed(&completion).await;
    assert_eq!(report.reason, DedicatedReason::Requested);
    assert_eq!(report.failure, None);
    restored(&paths, &report);
    drop(run);
    harness.finish();
}

#[tokio::test]
async fn supervisor_initialization_error_after_spawn_never_manufactures_restore_authority() {
    let fixture = Fixture::new();
    let harness = Harness::new();
    let hooks = Arc::new(TestHooks {
        owned: DedicatedTestHooks::new(true, None, None),
        ..TestHooks::default()
    });
    let (plan, paths, id) = prepare(
        &fixture,
        &harness,
        FixtureMode::Normal,
        hooks.clone(),
        limits(),
    );
    let approval = plan.review().digest.clone();
    let run = plan
        .start(&approval, CancellationToken::new())
        .expect("producer_worker_owned");
    let report = completed(&run.completion()).await;
    assert_eq!(hooks.owned.spawn_attempts(), 1);
    assert_eq!(report.reason, DedicatedReason::Failure);
    assert_eq!(
        report.failure,
        Some(DedicatedFailure::Spawn(OwnedRunError::Initialization))
    );
    assert_eq!(report.process, None);
    assert_eq!(report.eula, EulaDisposition::NeedsRecovery);
    assert!(!report.reached_ready);
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    assert_eq!(bytes(&slot(&paths, "snapshot.bin")), EULA_FALSE);
    assert_eq!(bytes(&slot(&paths, "old.bin")), EULA_FALSE);
    assert!(slot(&paths, "may-have-spawned.json").is_file());
    assert!(!slot(&paths, "restored.json").exists());
    unlocked(&paths);
    Harness::assert_status_port_released();
    // The failed83 constructor cleaned its Child, but84 received no original
    // completion. Only a new, guarded EULA recovery can restore this journal.
    let probe = probe::Probe::default();
    let recovery = fixture
        .recover(id, &probe, Hooks::default())
        .expect("producer_recovery_review");
    let approval = recovery.review().digest.clone();
    assert_eq!(recovery.confirm(&approval), Ok(EulaOutcome::Restored));
    assert!(probe.calls() >= 3);
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    assert_eq!(hooks.owned.spawn_attempts(), 1);
    assert!(slot(&paths, "claim.json").is_file());
    assert!(slot(&paths, "may-have-spawned.json").is_file());
    drop(run);
    harness.finish();
}
