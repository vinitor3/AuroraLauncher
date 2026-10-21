//! Causal checks of the original approved server.properties across apply and
//! spawn intent. The old implementation is collected before the RED assertion.
use super::super::producer::{
    self, DedicatedFailure, DedicatedPhase, DedicatedReason, DedicatedReport, EulaDisposition,
    FixtureMode, ProducerPoint, TestHooks, TestLimits, TestRecipe,
};
use super::producer_fixture::{Harness, WAIT};
use super::*;
use crate::engine::connect_owned_run::{DedicatedTestHooks, OwnedRunReason};
use std::sync::{atomic::AtomicUsize, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Boundary {
    AfterApply,
    BeforeStart,
    MayHaveSpawned,
}

#[derive(Clone, Copy, Debug)]
enum Change {
    Contents,
    SameBytesNewIdentity,
}

#[derive(Clone)]
struct Subject {
    path: PathBuf,
    retained: PathBuf,
    approved: Observation,
    approved_bytes: Vec<u8>,
}

#[derive(Default)]
struct Mutation {
    subject: Mutex<Option<Subject>>,
    changed: Mutex<Option<(Observation, Vec<u8>)>>,
    calls: AtomicUsize,
}

impl Mutation {
    fn apply(&self, change: Change) -> Result<()> {
        if self.calls.fetch_add(1, Ordering::SeqCst) != 0 {
            return Err(Error::Consumed);
        }
        let subject = self
            .subject
            .lock()
            .expect("pre_spawn_subject_lock")
            .clone()
            .expect("pre_spawn_subject_installed");
        let expected = match change {
            Change::Contents => {
                let original =
                    std::str::from_utf8(&subject.approved_bytes).map_err(|_| Error::Identity)?;
                let changed = original
                    .replace("server-ip=127.0.0.1\n", "server-ip=0.0.0.0\n")
                    .replace("online-mode=false\n", "online-mode=true\n")
                    .into_bytes();
                if changed == subject.approved_bytes {
                    return Err(Error::Changed);
                }
                fs::write(&subject.path, &changed)?;
                changed
            }
            Change::SameBytesNewIdentity => {
                // Keep the original physical file alive outside the bundle so
                // creating the replacement cannot reuse its still-live ID.
                fs::rename(&subject.path, &subject.retained)?;
                recovery::write_new(&subject.path, &subject.approved_bytes)?;
                subject.approved_bytes.clone()
            }
        };
        let (observed, bytes) = observe_file(&subject.path, 4096)?;
        if bytes != expected
            || matches!(change, Change::SameBytesNewIdentity)
                && observed.identity == subject.approved.identity
        {
            return Err(Error::Changed);
        }
        *self.changed.lock().expect("pre_spawn_changed_lock") = Some((observed, expected));
        Ok(())
    }

    fn preserved(&self, change: Change) -> bool {
        let subject = self.subject.lock().expect("pre_spawn_subject_lock");
        let subject = subject.as_ref().expect("pre_spawn_subject_installed");
        let changed = self.changed.lock().expect("pre_spawn_changed_lock");
        let Some((expected, expected_bytes)) = changed.as_ref() else {
            return false;
        };
        let unchanged = observe_file(&subject.path, 4096)
            .is_ok_and(|(actual, bytes)| actual == *expected && bytes == *expected_bytes);
        unchanged
            && match change {
                Change::Contents => expected.sha256 != subject.approved.sha256,
                Change::SameBytesNewIdentity => {
                    expected.identity != subject.approved.identity
                        && expected_bytes == &subject.approved_bytes
                        && observe_file(&subject.retained, 4096).is_ok_and(|(actual, bytes)| {
                            actual == subject.approved && bytes == subject.approved_bytes
                        })
                }
            }
    }
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

fn execution(
    prepared: PreparedStatus,
    harness: &Harness,
    hooks: Arc<TestHooks>,
) -> producer::DedicatedPlan {
    producer::plan_for_test(
        prepared,
        TestRecipe {
            control: harness.control_addr(),
            nonce: Uuid::parse_str(harness.nonce()).expect("pre_spawn_fixture_nonce"),
            mode: FixtureMode::Normal,
        },
        limits(),
        hooks,
    )
    .expect("pre_spawn_execution_review")
}

struct Facts {
    report: DedicatedReport,
    spawns: usize,
    mutated_once: bool,
    edit_preserved: bool,
    marker: bool,
    eula_before_recovery: Vec<u8>,
    explicit_recovery: bool,
}

async fn changed_case(boundary: Boundary, change: Change, original: &DedicatedTestHooks) -> Facts {
    let fixture = Fixture::new();
    let harness = Harness::new();
    let mutation = Arc::new(Mutation::default());
    let at_marker = mutation.clone();
    let prepared = fixture.prepared(Hooks {
        after_step: Some(Arc::new(move |step| {
            if boundary == Boundary::MayHaveSpawned && step == Step::MayHaveSpawned {
                at_marker.apply(change)?;
            }
            Ok(())
        })),
        ..Hooks::default()
    });
    let paths = prepared.test_paths();
    let id = prepared.bundle_id();
    let properties = paths.copy.join("server.properties");
    let (approved, approved_bytes) = observe_file(&properties, 4096).expect("pre_spawn_baseline");
    *mutation.subject.lock().expect("pre_spawn_install_subject") = Some(Subject {
        path: properties,
        retained: fixture
            .root
            .path()
            .join("independent-original-properties.bin"),
        approved,
        approved_bytes,
    });
    let source_before = files::tree(&paths.origin);
    let stage_before = files::tree(&fixture.stage);
    let jdk_before = files::tree(&fixture.jdk);
    let cancel = CancellationToken::new();
    let fallback_cancel = cancel.clone();
    let at_point = mutation.clone();
    let hooks = Arc::new(TestHooks {
        owned: original.clone(),
        point: Some(Arc::new(move |point| {
            if matches!(
                (boundary, point),
                (Boundary::AfterApply, ProducerPoint::AfterApply)
                    | (Boundary::BeforeStart, ProducerPoint::BeforeStart)
            ) {
                at_point.apply(change).expect("pre_spawn_independent_edit");
            }
            if point == ProducerPoint::AfterStart {
                // Old code may wrongly start the synthetic Child. Cancel only
                // this original run and collect it before asserting zero spawn.
                fallback_cancel.cancel();
            }
        })),
        ..TestHooks::default()
    });
    let plan = execution(prepared, &harness, hooks);
    let digest = plan.review().digest.clone();
    let before_spawns = original.spawn_attempts();
    let run = plan.start(&digest, cancel).expect("pre_spawn_control");
    let report = tokio::time::timeout(WAIT, run.completion().wait())
        .await
        .expect("pre_spawn_original_completion");
    // Everything below runs after the actual owner has completed, even when
    // the unpatched code launched a Child. Counters do not authorize cleanup.
    if let Some(process) = &report.process {
        assert!(
            process.reaped && process.readers_complete,
            "pre_spawn_old_child_collected"
        );
    }
    let spawns = original.spawn_attempts() - before_spawns;
    if spawns == 0 {
        harness.assert_no_child();
    }
    let marker = slot(&paths, "may-have-spawned.json").is_file();
    let eula_before_recovery = bytes(&target(&paths));
    let mut edit_preserved = mutation.preserved(change);
    unlocked(&paths);
    Harness::assert_status_port_released();
    let mut explicit_recovery = false;
    if report.eula == EulaDisposition::NeedsRecovery {
        // Recovery is a separate consumed review with a newly acquired real
        // guard using its established private probe seam. It touches only EULA.
        let journal_before = files::tree(&paths.transaction);
        let blocked = probe::Probe::default();
        blocked.refuse(1, probe::Refusal::Running);
        assert_eq!(
            fixture.recover(id, &blocked, Hooks::default()).err(),
            Some(Error::JavaRunning)
        );
        assert_eq!(blocked.calls(), 1);
        files::assert_tree(&paths.transaction, &journal_before);
        let fresh = probe::Probe::default();
        let wrong = fixture
            .recover(id, &fresh, Hooks::default())
            .expect("pre_spawn_recovery_review");
        assert_ne!(wrong.review().digest, digest);
        assert_eq!(wrong.confirm(&digest).err(), Some(Error::Digest));
        files::assert_tree(&paths.transaction, &journal_before);
        let recovery = fixture
            .recover(id, &fresh, Hooks::default())
            .expect("pre_spawn_fresh_recovery");
        let approved = recovery.review().digest.clone();
        assert_eq!(recovery.confirm(&approved), Ok(EulaOutcome::Restored));
        assert!(fresh.calls() >= 6, "pre_spawn_reacquired_guard");
        explicit_recovery = true;
        edit_preserved &= mutation.preserved(change);
    }
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    assert_eq!(bytes(&slot(&paths, "snapshot.bin")), EULA_FALSE);
    assert_eq!(bytes(&slot(&paths, "applied.bin")), EULA_TRUE);
    assert!(slot(&paths, "claim.json").is_file());
    assert!(slot(&paths, "restored.json").is_file());
    assert!(!slot(&paths, "old.bin").exists());
    unlocked(&paths);
    files::assert_tree(&paths.origin, &source_before);
    files::assert_tree(&fixture.stage, &stage_before);
    files::assert_tree(&fixture.jdk, &jdk_before);
    let mutated_once = mutation.calls.load(Ordering::SeqCst) == 1;
    drop(run);
    harness.finish();
    Facts {
        report,
        spawns,
        mutated_once,
        edit_preserved,
        marker,
        eula_before_recovery,
        explicit_recovery,
    }
}

async fn unchanged_positive(original: &DedicatedTestHooks) {
    let fixture = Fixture::new();
    let mut harness = Harness::new();
    let prepared = fixture.prepared(Hooks::default());
    let paths = prepared.test_paths();
    let before = observe_file(&paths.copy.join("server.properties"), 4096)
        .expect("pre_spawn_positive_properties");
    let plan = execution(
        prepared,
        &harness,
        Arc::new(TestHooks {
            owned: original.clone(),
            ..TestHooks::default()
        }),
    );
    let digest = plan.review().digest.clone();
    let before_spawns = original.spawn_attempts();
    let run = plan
        .start(&digest, CancellationToken::new())
        .expect("pre_spawn_positive_start");
    let observer = run.observer();
    harness.ready();
    let deadline = Instant::now() + WAIT;
    let ready = loop {
        match observer.snapshot() {
            Some(state) if state.phase == DedicatedPhase::Ready => break true,
            Some(state) if state.reason.is_none() && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            _ => break false,
        }
    };
    if ready {
        harness.status_observed(true);
    }
    let report = tokio::time::timeout(WAIT, run.request_stop().wait())
        .await
        .expect("pre_spawn_positive_completion");
    let spawns = original.spawn_attempts() - before_spawns;
    let unchanged = observe_file(&paths.copy.join("server.properties"), 4096)
        .is_ok_and(|after| after == before);
    unlocked(&paths);
    Harness::assert_status_port_released();
    drop(run);
    harness.finish();
    assert!(ready && report.reached_ready);
    assert_eq!(spawns, 1, "same_counter_real_spawn_control");
    assert_eq!(report.reason, DedicatedReason::Requested);
    assert_eq!(report.failure, None);
    assert_eq!(report.eula, EulaDisposition::Restored);
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    assert!(unchanged);
    let process = report.process.expect("pre_spawn_positive_original_report");
    assert_eq!(process.reason, OwnedRunReason::Requested);
    assert_eq!(process.failure, None);
    assert_eq!(process.exit_code, Some(0));
    assert!(!process.forced && process.reaped && process.readers_complete);
}

async fn exercise(boundary: Boundary) {
    let original = DedicatedTestHooks::default();
    let mut cases = Vec::new();
    for change in [Change::Contents, Change::SameBytesNewIdentity] {
        cases.push((change, changed_case(boundary, change, &original).await));
    }
    // The very same supervisor counter must observe an unchanged approved
    // execution. A disconnected zero counter cannot satisfy these negatives.
    unchanged_positive(&original).await;
    for (change, facts) in cases {
        assert_eq!(
            facts.spawns, 0,
            "unapproved_properties_spawned: {boundary:?}/{change:?}"
        );
        assert!(facts.mutated_once && facts.edit_preserved);
        assert_eq!(facts.report.reason, DedicatedReason::Failure);
        assert_eq!(
            facts.report.failure,
            Some(DedicatedFailure::Preparation(Error::Changed))
        );
        assert!(facts.report.process.is_none() && !facts.report.reached_ready);
        if boundary == Boundary::MayHaveSpawned {
            assert!(facts.marker && facts.explicit_recovery);
            assert_eq!(facts.report.eula, EulaDisposition::NeedsRecovery);
            assert_eq!(facts.eula_before_recovery, EULA_TRUE);
        } else {
            assert!(!facts.marker && !facts.explicit_recovery);
            assert_eq!(facts.report.eula, EulaDisposition::Restored);
            assert_eq!(facts.eula_before_recovery, EULA_FALSE);
        }
    }
}

#[tokio::test]
async fn properties_changed_after_apply_cannot_reach_supervisor_spawn() {
    exercise(Boundary::AfterApply).await;
}

#[tokio::test]
async fn properties_changed_before_start_cannot_replace_the_approved_observation() {
    exercise(Boundary::BeforeStart).await;
}

#[tokio::test]
async fn properties_changed_after_spawn_intent_require_fresh_eula_recovery_without_spawn() {
    exercise(Boundary::MayHaveSpawned).await;
}
