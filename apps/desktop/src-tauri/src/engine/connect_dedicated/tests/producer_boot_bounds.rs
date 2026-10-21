//! A real Child and repeated EOF responses exercise the existing attempt cap.
//! Its closed fixture mode does not change the producer's original limits.
use super::super::producer::{
    self, DedicatedFailure, DedicatedReason, EulaDisposition, FixtureMode, ProducerPoint,
    TestHooks, TestLimits, TestRecipe,
};
use super::producer_fixture::{Harness, WAIT};
use super::*;
use crate::engine::connect_owned_run::DedicatedTestHooks;
use std::sync::{Condvar, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct StartGate {
    released: Mutex<bool>,
    changed: Condvar,
}
impl StartGate {
    fn wait(&self) {
        let released = self
            .released
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let (released, _) = self
            .changed
            .wait_timeout_while(released, Duration::from_secs(20), |released| !*released)
            .unwrap_or_else(|poison| poison.into_inner());
        assert!(*released, "producer_bounds_start_gate_fallback");
    }

    fn release(&self) {
        *self
            .released
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = true;
        self.changed.notify_all();
    }
}

struct ReleaseStart(Arc<StartGate>);
impl Drop for ReleaseStart {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[tokio::test]
async fn repeated_truncated_status_exhausts_two_attempts_without_restarting_the_child() {
    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let start_gate = Arc::new(StartGate::default());
    let _release_start = ReleaseStart(start_gate.clone());
    let after_start = start_gate.clone();
    let original = DedicatedTestHooks::default();
    let prepared = fixture.prepared(Hooks::default());
    let paths = prepared.test_paths();
    let source_before = files::tree(&paths.origin);
    let stage_before = files::tree(&fixture.stage);
    let jdk_before = files::tree(&fixture.jdk);
    let limits = TestLimits {
        lifetime_ms: 15_000,
        readiness_ms: 8_000,
        stop_ms: 750,
        output_bytes: 4 * 1024 * 1024,
        attempts: 2,
    };
    let plan = producer::plan_for_test(
        prepared,
        TestRecipe {
            control: harness.control_addr(),
            nonce: Uuid::parse_str(harness.nonce()).expect("producer_bounds_nonce"),
            mode: FixtureMode::TruncateEveryStatus,
        },
        limits,
        Arc::new(TestHooks {
            owned: original.clone(),
            point: Some(Arc::new(move |point| {
                if point == ProducerPoint::AfterStart {
                    after_start.wait();
                }
            })),
            ..TestHooks::default()
        }),
    )
    .expect("producer_bounds_plan");
    let digest = plan.review().digest.clone();
    let started = Instant::now();
    let run = plan
        .start(&digest, CancellationToken::new())
        .expect("producer_bounds_start");
    let completion = run.completion();
    harness.ready();
    // The original Child has bound its literal listener before attempt one.
    // Missing-listener boot iterations cannot consume this test's budget.
    start_gate.release();
    // Neither polling Ready nor requesting Stop drives this exhaustion. Each
    // real request is acknowledged to the control stream before its socket is
    // closed, and all remaining notifications are consumed after actual reap.
    let report = tokio::time::timeout(WAIT, completion.wait())
        .await
        .expect("producer_bounds_completion");
    let elapsed = started.elapsed();
    let counts = harness.status_counts_after_completion();
    let attempts = original.spawn_attempts();
    let process_complete = report
        .process
        .as_ref()
        .is_some_and(|process| process.reaped && process.readers_complete);
    let eula_restored = bytes(&target(&paths)) == EULA_FALSE;
    let snapshot_preserved = bytes(&slot(&paths, "snapshot.bin")) == EULA_FALSE;
    let applied_preserved = bytes(&slot(&paths, "applied.bin")) == EULA_TRUE;
    let claim_consumed = slot(&paths, "claim.json").is_file();
    let spawn_intent_preserved = slot(&paths, "may-have-spawned.json").is_file();
    let restored_marker = slot(&paths, "restored.json").is_file();
    let original_slot_absent = !slot(&paths, "old.bin").exists();
    unlocked(&paths);
    Harness::assert_status_port_released();
    files::assert_tree(&paths.origin, &source_before);
    files::assert_tree(&fixture.stage, &stage_before);
    files::assert_tree(&fixture.jdk, &jdk_before);
    drop(run);
    harness.finish();

    assert_eq!(report.reason, DedicatedReason::Failure);
    assert_eq!(report.failure, Some(DedicatedFailure::Readiness));
    assert!(!report.reached_ready);
    assert_eq!(counts, (2, 0), "bounded_valid_requests_and_no_pongs");
    assert_eq!(attempts, 1, "retries_keep_the_original_child");
    // Finishing before the original readiness bound distinguishes exhaustion
    // of attempts from merely waiting for that deadline. Starting this clock
    // before start's validation is conservative; no authority clock is reset.
    assert!(
        elapsed < Duration::from_millis(limits.readiness_ms),
        "attempt_cap_must_precede_original_readiness_deadline"
    );
    assert_eq!(report.eula, EulaDisposition::Restored);
    assert!(process_complete);
    assert!(eula_restored && snapshot_preserved && applied_preserved);
    assert!(claim_consumed && spawn_intent_preserved && restored_marker && original_slot_absent);
}
