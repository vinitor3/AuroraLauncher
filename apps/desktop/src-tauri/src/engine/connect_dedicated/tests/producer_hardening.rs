//! Failure boundaries use the real private factory, original Child supervisor,
//! bounded STATUS fixture and EULA journal. No Java is started by these tests.
use super::super::producer::{
    self, DedicatedCompletion, DedicatedError, DedicatedFailure, DedicatedObserver, DedicatedPhase,
    DedicatedPlan, DedicatedReason, DedicatedReport, EulaDisposition, FixtureMode, ProducerPoint,
    TestHooks, TestLimits, TestRecipe,
};
use super::producer_fixture::{Harness, WAIT};
use super::*;
use crate::engine::connect_owned_run::{OwnedRunFailure, OwnedRunReason};
use aurora_connect_lab::MinecraftStatusError;
use std::sync::{atomic::AtomicUsize, Condvar, Mutex};
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
            .wait_timeout_while(state, Duration::from_secs(20), |state| !state.1)
            .unwrap_or_else(|p| p.into_inner());
        assert!(!timed.timed_out() || state.1, "hardening_gate_fallback");
    }

    fn reached(&self) {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let (state, _) = self
            .changed
            .wait_timeout_while(state, WAIT, |state| !state.0)
            .unwrap_or_else(|p| p.into_inner());
        assert!(state.0, "hardening_gate_reached");
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

fn panic_at(
    selected: ProducerPoint,
    gate: Option<Arc<Gate>>,
) -> (Arc<TestHooks>, Arc<AtomicUsize>) {
    let reached = Arc::new(AtomicUsize::new(0));
    let counted = reached.clone();
    let hooks = Arc::new(TestHooks {
        point: Some(Arc::new(move |point| {
            if point == selected {
                counted.fetch_add(1, Ordering::SeqCst);
                if let Some(gate) = &gate {
                    gate.reach();
                }
                panic!("hardening_injected_panic");
            }
        })),
        ..TestHooks::default()
    });
    (hooks, reached)
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

fn plan(
    prepared: PreparedStatus,
    harness: &Harness,
    mode: FixtureMode,
    hooks: Arc<TestHooks>,
    limits: TestLimits,
) -> DedicatedPlan {
    producer::plan_for_test(
        prepared,
        TestRecipe {
            control: harness.control_addr(),
            nonce: Uuid::parse_str(harness.nonce()).expect("hardening_fixture_nonce"),
            mode,
        },
        limits,
        hooks,
    )
    .expect("hardening_execution_review")
}

async fn ready(observer: &DedicatedObserver) {
    let deadline = Instant::now() + WAIT;
    loop {
        let state = observer.snapshot().expect("hardening_owned_snapshot");
        if state.phase == DedicatedPhase::Ready {
            return;
        }
        assert!(state.reason.is_none(), "hardening_ready_failure: {state:?}");
        assert!(Instant::now() < deadline, "hardening_ready_deadline");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn completed(completion: &DedicatedCompletion) -> DedicatedReport {
    tokio::time::timeout(WAIT, completion.wait())
        .await
        .expect("hardening_completion_deadline")
}

fn original_collected(report: &DedicatedReport) {
    let process = report.process.as_ref().expect("hardening_original_report");
    assert!(process.reaped && process.readers_complete);
    Harness::assert_status_port_released();
}

fn restored(paths: &TestPaths) {
    assert_eq!(bytes(&target(paths)), EULA_FALSE);
    assert_eq!(bytes(&slot(paths, "snapshot.bin")), EULA_FALSE);
    assert_eq!(bytes(&slot(paths, "applied.bin")), EULA_TRUE);
    assert!(!slot(paths, "old.bin").exists());
    assert!(slot(paths, "restored.json").is_file());
    unlocked(paths);
}

fn recover_with_fresh_guard(
    fixture: &Fixture,
    id: BundleId,
    paths: &TestPaths,
    execution_digest: &str,
    expected_renames: usize,
) {
    let before = files::tree(&paths.transaction);
    let blocked = probe::Probe::default();
    blocked.refuse(1, probe::Refusal::Running);
    let hooks = Hooks::default();
    assert_eq!(
        fixture.recover(id, &blocked, hooks.clone()).err(),
        Some(Error::JavaRunning)
    );
    assert_eq!(blocked.calls(), 1);
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
    files::assert_tree(&paths.transaction, &before);
    unlocked(paths);

    // The original execution review does not approve recovery after a failed
    // finalizer, even when its original Child has already been collected.
    let probe = probe::Probe::default();
    let wrong = fixture
        .recover(id, &probe, hooks.clone())
        .expect("hardening_recovery_review");
    assert_ne!(wrong.review().digest, execution_digest);
    assert_eq!(wrong.confirm(execution_digest).err(), Some(Error::Digest));
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
    files::assert_tree(&paths.transaction, &before);
    unlocked(paths);

    let recovery = fixture
        .recover(id, &probe, hooks.clone())
        .expect("hardening_fresh_recovery_review");
    let approval = recovery.review().digest.clone();
    assert_eq!(
        recovery.confirm(&approval).expect("hardening_recovery"),
        EulaOutcome::Restored
    );
    assert!(probe.calls() >= 6, "hardening_guard_is_reacquired");
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), expected_renames);
    restored(paths);
}

#[tokio::test]
async fn execution_review_keeps_the_original_preparation_expiry_without_spawn_or_claim() {
    let fixture = Fixture::new();
    let harness = Harness::new();
    let mut prepared = fixture.prepared(Hooks::default());
    // This seam can only shorten A; constructing the execution review must
    // preserve that original deadline instead of renewing the approval.
    prepared.shorten_approval_for_test(Duration::from_secs(3));
    let original_deadline = prepared.approval_deadline();
    let paths = prepared.test_paths();
    let before = files::tree(&paths.bundle);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let hooks = Arc::new(TestHooks::default());
    let execution = plan(
        prepared,
        &harness,
        FixtureMode::Normal,
        hooks.clone(),
        limits(),
    );
    let approval = execution.review().digest.clone();
    assert!(
        Instant::now() < original_deadline,
        "hardening_valid_C_review"
    );
    tokio::time::sleep(
        original_deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(20),
    )
    .await;
    assert_eq!(
        execution.start(&approval, CancellationToken::new()).err(),
        Some(DedicatedError::Preparation(Error::Expired))
    );
    assert_eq!(hooks.owned.spawn_attempts(), 0);
    assert!(!slot(&paths, "claim.json").exists());
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    files::assert_tree(&paths.bundle, &before);
    unlocked(&paths);
    harness.assert_no_child();
    Harness::assert_status_port_released();
    harness.finish();
}

#[tokio::test]
async fn panics_before_and_after_apply_have_distinct_mutation_and_restore_outcomes() {
    for point in [ProducerPoint::BeforeApply, ProducerPoint::AfterApply] {
        let fixture = Fixture::new();
        let harness = Harness::new();
        let prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        let source_before = files::tree(&paths.origin);
        let bundle_before = files::tree(&paths.bundle);
        let (hooks, reached) = panic_at(point, None);
        let execution = plan(
            prepared,
            &harness,
            FixtureMode::Normal,
            hooks.clone(),
            limits(),
        );
        let approval = execution.review().digest.clone();
        let run = execution
            .start(&approval, CancellationToken::new())
            .expect("hardening_worker_started");
        let completion = run.completion();
        let report = completed(&completion).await;
        assert_eq!(reached.load(Ordering::SeqCst), 1);
        assert_eq!(hooks.owned.spawn_attempts(), 0);
        assert_eq!(report.reason, DedicatedReason::Failure);
        assert_eq!(report.failure, Some(DedicatedFailure::Panic));
        assert!(!report.reached_ready);
        assert_eq!(report.process, None);
        assert!(!slot(&paths, "may-have-spawned.json").exists());
        if point == ProducerPoint::BeforeApply {
            assert_eq!(report.eula, EulaDisposition::Unchanged);
            assert!(!slot(&paths, "claim.json").exists());
            files::assert_tree(&paths.bundle, &bundle_before);
        } else {
            assert_eq!(report.eula, EulaDisposition::Restored);
            assert!(slot(&paths, "claim.json").is_file());
            restored(&paths);
        }
        assert_eq!(bytes(&target(&paths)), EULA_FALSE);
        files::assert_tree(&paths.origin, &source_before);
        unlocked(&paths);
        harness.assert_no_child();
        Harness::assert_status_port_released();
        assert_eq!(completed(&completion).await, report);
        drop(run);
        harness.finish();
    }
}

#[tokio::test]
async fn panic_after_start_collects_the_live_original_child_before_restoring_eula() {
    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let prepared = fixture.prepared(Hooks::default());
    let paths = prepared.test_paths();
    let gate = Arc::new(Gate::default());
    let _release = Release(gate.clone());
    let (hooks, reached) = panic_at(ProducerPoint::AfterStart, Some(gate.clone()));
    let execution = plan(
        prepared,
        &harness,
        FixtureMode::Normal,
        hooks.clone(),
        limits(),
    );
    let approval = execution.review().digest.clone();
    let run = execution
        .start(&approval, CancellationToken::new())
        .expect("hardening_start");
    let completion = run.completion();
    gate.reached();
    harness.ready();
    assert_eq!(hooks.owned.spawn_attempts(), 1);
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    locked(&paths);
    assert!(std::net::TcpListener::bind(super::producer_fixture::ENDPOINT).is_err());
    assert!(
        tokio::time::timeout(Duration::from_millis(30), completion.wait())
            .await
            .is_err()
    );
    gate.release();
    harness.stop_observed();
    let report = completed(&completion).await;
    assert_eq!(reached.load(Ordering::SeqCst), 1);
    assert_eq!(report.reason, DedicatedReason::Failure);
    assert_eq!(report.failure, Some(DedicatedFailure::Panic));
    assert!(!report.reached_ready);
    assert_eq!(report.eula, EulaDisposition::Restored);
    original_collected(&report);
    restored(&paths);
    assert!(slot(&paths, "may-have-spawned.json").is_file());
    assert_eq!(completed(&completion).await, report);
    drop(run);
    harness.finish();
}

#[tokio::test]
async fn finalizer_panics_publish_pending_recovery_only_after_original_process_cleanup() {
    for point in [ProducerPoint::BeforeRestore, ProducerPoint::BeforePublish] {
        let fixture = Fixture::new();
        let mut harness = Harness::new();
        let prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        let id = prepared.bundle_id();
        let gate = Arc::new(Gate::default());
        let _release = Release(gate.clone());
        let (hooks, reached) = panic_at(point, Some(gate.clone()));
        let execution = plan(
            prepared,
            &harness,
            FixtureMode::Normal,
            hooks.clone(),
            limits(),
        );
        let approval = execution.review().digest.clone();
        let run = execution
            .start(&approval, CancellationToken::new())
            .expect("hardening_start");
        let source = run.source();
        harness.ready();
        ready(&run.observer()).await;
        harness.status_observed(true);
        let completion = run.request_stop();
        harness.stop_observed();
        gate.reached();
        assert_eq!(hooks.owned.spawn_attempts(), 1);
        assert_eq!(source.observe_tcp(), Err(DedicatedError::Closed));
        Harness::assert_status_port_released();
        locked(&paths);
        assert!(
            tokio::time::timeout(Duration::from_millis(30), completion.wait())
                .await
                .is_err()
        );
        let applied = point == ProducerPoint::BeforeRestore;
        assert_eq!(
            bytes(&target(&paths)),
            if applied { EULA_TRUE } else { EULA_FALSE }
        );
        assert_eq!(slot(&paths, "restored.json").exists(), !applied);
        gate.release();
        let report = completed(&completion).await;
        assert_eq!(reached.load(Ordering::SeqCst), 1);
        assert_eq!(report.reason, DedicatedReason::Requested);
        assert_eq!(report.failure, Some(DedicatedFailure::Panic));
        assert!(report.reached_ready);
        assert_eq!(report.eula, EulaDisposition::NeedsRecovery);
        original_collected(&report);
        let process = report.process.as_ref().expect("hardening_original_report");
        assert_eq!(process.reason, OwnedRunReason::Requested);
        assert_eq!(process.failure, None);
        assert!(!process.forced);
        unlocked(&paths);
        recover_with_fresh_guard(&fixture, id, &paths, &approval, if applied { 2 } else { 0 });
        assert_eq!(completed(&completion).await, report);
        assert_eq!(source.observe_tcp(), Err(DedicatedError::Closed));
        drop(run);
        harness.finish();
    }
}

#[tokio::test]
async fn the_original_status_error_survives_a_later_finalizer_panic_and_recovery() {
    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let prepared = fixture.prepared(Hooks::default());
    let paths = prepared.test_paths();
    let id = prepared.bundle_id();
    let (hooks, reached) = panic_at(ProducerPoint::BeforeRestore, None);
    let execution = plan(
        prepared,
        &harness,
        FixtureMode::BadPong,
        hooks.clone(),
        limits(),
    );
    let approval = execution.review().digest.clone();
    let run = execution
        .start(&approval, CancellationToken::new())
        .expect("hardening_start");
    let completion = run.completion();
    harness.ready();
    // Request receipt is acknowledged before the invalid pong is written.
    // A fast producer cancellation may prevent its later fixture notice.
    harness.status_observed(false);
    harness.stop_observed();
    let report = completed(&completion).await;
    assert_eq!(hooks.owned.spawn_attempts(), 1);
    assert_eq!(reached.load(Ordering::SeqCst), 1);
    assert_eq!(report.reason, DedicatedReason::Failure);
    assert_eq!(
        report.failure,
        Some(DedicatedFailure::Status(MinecraftStatusError::PongMismatch))
    );
    assert!(!report.reached_ready);
    assert_eq!(report.eula, EulaDisposition::NeedsRecovery);
    original_collected(&report);
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    assert!(!slot(&paths, "restored.json").exists());
    unlocked(&paths);
    recover_with_fresh_guard(&fixture, id, &paths, &approval, 2);
    assert_eq!(completed(&completion).await, report);
    drop(run);
    harness.finish();
}

#[tokio::test]
async fn ready_lifetime_expires_without_snapshot_source_or_stop_polling() {
    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let prepared = fixture.prepared(Hooks::default());
    let paths = prepared.test_paths();
    let hooks = Arc::new(TestHooks::default());
    let execution = plan(
        prepared,
        &harness,
        FixtureMode::Normal,
        hooks.clone(),
        TestLimits {
            lifetime_ms: 6_000,
            readiness_ms: 5_000,
            stop_ms: 500,
            ..limits()
        },
    );
    let approval = execution.review().digest.clone();
    let run = execution
        .start(&approval, CancellationToken::new())
        .expect("hardening_start");
    let completion = run.completion();
    harness.ready();
    harness.status_observed(true);
    // Only the completion channel is awaited. It owns no worker or process
    // authority and cannot refresh the run's lifecycle gate.
    let report = completed(&completion).await;
    assert_eq!(hooks.owned.spawn_attempts(), 1);
    assert!(
        report.reached_ready,
        "hardening_ready_was_reached_without_polling"
    );
    assert_eq!(report.reason, DedicatedReason::Deadline);
    assert_eq!(report.failure, Some(DedicatedFailure::Deadline));
    assert_eq!(report.eula, EulaDisposition::Restored);
    original_collected(&report);
    let process = report.process.as_ref().expect("hardening_original_report");
    assert_eq!(process.failure, Some(OwnedRunFailure::Deadline));
    // Deadline exhaustion may require forced termination; it does not grant
    // a fresh graceful-stop interval beyond the original lifetime.
    restored(&paths);
    assert_eq!(completed(&completion).await, report);
    drop(run);
    harness.finish();
}
