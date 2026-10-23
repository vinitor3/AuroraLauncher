//! Causal approval/launch barriers over admitted physical voice preparations.
//! The shared83 counter is incremented by before_spawn immediately before the
//! real Command::spawn. Every zero-spawn batch also runs a positive Child with
//! that SAME counter. No Java, raw PID or externally supplied completion.
use super::super::producer::{
    self, DedicatedError, DedicatedFailure, DedicatedReason, DedicatedReport, EulaDisposition,
    FixtureMode, ProducerPoint, TestHooks, TestRecipe,
};
use super::voice_fixture::{VoiceFixture, PROFILES};
use super::voice_producer_fixture::Harness;
use super::voice_runtime::{completed, limits, ready, Running};
use super::*;
use crate::engine::connect_owned_run::{DedicatedTestHooks, OwnedRunReason};
use manifest::Manifest;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{atomic::AtomicUsize, Mutex},
};
use tokio_util::sync::CancellationToken;

const CONFIGURATION_LIMIT: u64 = 256 * 1024;

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
#[derive(Clone, Copy, Debug)]
enum SubjectKind {
    VoiceConfiguration,
    Properties,
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
            .expect("voice_pre_subject_lock")
            .clone()
            .expect("voice_pre_subject_installed");
        let expected = match change {
            Change::Contents => {
                // Remains syntactically valid for either fixed config and for
                // properties. The launch gate must compare the exact original
                // approval, even when the additional comment is harmless.
                let mut changed = subject.approved_bytes.clone();
                changed.extend_from_slice(b"# independent edit after execution review\n");
                fs::write(&subject.path, &changed)?;
                changed
            }
            Change::SameBytesNewIdentity => {
                fs::rename(&subject.path, &subject.retained)?;
                recovery::write_new(&subject.path, &subject.approved_bytes)?;
                subject.approved_bytes.clone()
            }
        };
        let (observed, bytes) = observe_file(&subject.path, CONFIGURATION_LIMIT)?;
        if bytes != expected
            || (matches!(change, Change::SameBytesNewIdentity)
                && observed.identity == subject.approved.identity)
        {
            return Err(Error::Changed);
        }
        *self.changed.lock().expect("voice_pre_changed_lock") = Some((observed, expected));
        Ok(())
    }
    fn preserved(&self, change: Change) -> bool {
        let subject = self.subject.lock().expect("voice_pre_subject_lock");
        let subject = subject.as_ref().expect("voice_pre_subject_installed");
        let changed = self.changed.lock().expect("voice_pre_changed_lock");
        let Some((expected, bytes)) = changed.as_ref() else {
            return false;
        };
        observe_file(&subject.path, CONFIGURATION_LIMIT)
            .is_ok_and(|actual| actual == (expected.clone(), bytes.clone()))
            && match change {
                Change::Contents => expected.sha256 != subject.approved.sha256,
                Change::SameBytesNewIdentity => {
                    expected.identity != subject.approved.identity
                        && bytes == &subject.approved_bytes
                        && observe_file(&subject.retained, CONFIGURATION_LIMIT).is_ok_and(
                            |actual| {
                                actual == (subject.approved.clone(), subject.approved_bytes.clone())
                            },
                        )
                }
            }
    }
}

fn execution(
    prepared: PreparedStatus,
    harness: &Harness,
    hooks: Arc<TestHooks>,
) -> producer::DedicatedPlan {
    producer::plan_voice_for_test(
        prepared,
        harness.control_addr(),
        harness.nonce(),
        false,
        limits(),
        hooks,
    )
    .expect("voice_pre_execution_review")
}

struct Facts {
    report: DedicatedReport,
    spawns: usize,
    changed_once: bool,
    edit_preserved: bool,
    marker: bool,
    eula_before_recovery: Vec<u8>,
    explicit_recovery: bool,
}

async fn changed_case(
    profile: NativeVoiceProfile,
    kind: SubjectKind,
    boundary: Boundary,
    change: Change,
    original: &DedicatedTestHooks,
) -> Facts {
    let fixture = VoiceFixture::new(profile);
    let harness = Harness::new(fixture.stored_profile());
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
    let subject = match kind {
        SubjectKind::VoiceConfiguration => fixture.configuration_path(&paths),
        SubjectKind::Properties => paths.copy.join("server.properties"),
    };
    let (approved, approved_bytes) =
        observe_file(&subject, CONFIGURATION_LIMIT).expect("voice_pre_same_baseline");
    *mutation.subject.lock().expect("voice_pre_install_subject") = Some(Subject {
        path: subject,
        retained: fixture
            .fixture
            .root
            .path()
            .join("independent-original-launch-file.bin"),
        approved,
        approved_bytes,
    });
    let origin = files::tree(&paths.origin);
    let stage = files::tree(&fixture.fixture.stage);
    let jdk = files::tree(&fixture.fixture.jdk);
    let cancel = CancellationToken::new();
    let fallback = cancel.clone();
    let at_point = mutation.clone();
    let hooks = Arc::new(TestHooks {
        owned: original.clone(),
        point: Some(Arc::new(move |point| {
            if matches!(
                (boundary, point),
                (Boundary::AfterApply, ProducerPoint::AfterApply)
                    | (Boundary::BeforeStart, ProducerPoint::BeforeStart)
            ) {
                at_point.apply(change).expect("voice_pre_independent_edit");
            }
            if point == ProducerPoint::AfterStart {
                // A regression may actually spawn. Stop only that original
                // invocation and collect its real completion before RED.
                fallback.cancel();
            }
        })),
        ..TestHooks::default()
    });
    let plan = execution(prepared, &harness, hooks);
    let execution_digest = plan.review().digest.clone();
    let before_spawns = original.spawn_attempts();
    let running = Running::start(plan, cancel);
    let report = completed(&running.completion).await;
    if let Some(process) = &report.process {
        assert!(
            process.reaped && process.readers_complete,
            "voice_pre_regressed_child_collected"
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
    harness.ports_released();
    let mut explicit_recovery = false;
    if report.eula == EulaDisposition::NeedsRecovery {
        let journal = files::tree(&paths.transaction);
        let blocked = probe::Probe::default();
        blocked.refuse(1, probe::Refusal::Running);
        assert_eq!(
            fixture.recover(id, &blocked, Hooks::default()).err(),
            Some(Error::JavaRunning)
        );
        assert_eq!(blocked.calls(), 1);
        files::assert_tree(&paths.transaction, &journal);
        let fresh = probe::Probe::default();
        let wrong = fixture
            .recover(id, &fresh, Hooks::default())
            .expect("voice_pre_recovery_review");
        assert_ne!(wrong.review().digest, execution_digest);
        assert_eq!(wrong.confirm(&execution_digest).err(), Some(Error::Digest));
        files::assert_tree(&paths.transaction, &journal);
        let recovery = fixture
            .recover(id, &fresh, Hooks::default())
            .expect("voice_pre_fresh_recovery");
        let approval = recovery.review().digest.clone();
        assert_eq!(recovery.confirm(&approval), Ok(EulaOutcome::Restored));
        assert!(fresh.calls() >= 6, "voice_pre_reacquired_recovery_guard");
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
    files::assert_tree(&paths.origin, &origin);
    files::assert_tree(&fixture.fixture.stage, &stage);
    files::assert_tree(&fixture.fixture.jdk, &jdk);
    let changed_once = mutation.calls.load(Ordering::SeqCst) == 1;
    drop(running);
    harness.finish();
    Facts {
        report,
        spawns,
        changed_once,
        edit_preserved,
        marker,
        eula_before_recovery,
        explicit_recovery,
    }
}

async fn positive(profile: NativeVoiceProfile, original: &DedicatedTestHooks) {
    let fixture = VoiceFixture::new(profile);
    let mut harness = Harness::new(fixture.stored_profile());
    let prepared = fixture.prepared(Hooks::default());
    let paths = prepared.test_paths();
    let properties = observe_file(&paths.copy.join("server.properties"), 4096).unwrap();
    let plan = execution(
        prepared,
        &harness,
        Arc::new(TestHooks {
            owned: original.clone(),
            ..TestHooks::default()
        }),
    );
    let before = original.spawn_attempts();
    let running = Running::start(plan, CancellationToken::new());
    harness.ready();
    ready(running.control()).await;
    harness.status_observed();
    harness.udp_bytes();
    assert_eq!(
        running
            .control()
            .voice_source()
            .unwrap()
            .observe()
            .unwrap()
            .profile,
        profile
    );
    let report = running.finish().await;
    harness.ports_released();
    unlocked(&paths);
    harness.finish();
    assert_eq!(
        original.spawn_attempts() - before,
        1,
        "voice_pre_same_counter_real_spawn"
    );
    assert!(report.reached_ready);
    assert_eq!(report.reason, DedicatedReason::Requested);
    assert_eq!(report.failure, None);
    assert_eq!(report.eula, EulaDisposition::Restored);
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    assert_eq!(
        observe_file(&paths.copy.join("server.properties"), 4096).unwrap(),
        properties
    );
    let process = report.process.expect("voice_pre_positive_original_report");
    assert_eq!(process.reason, OwnedRunReason::Requested);
    assert_eq!(process.failure, None);
    assert_eq!(process.exit_code, Some(0));
    assert!(!process.forced && process.reaped && process.readers_complete);
}

async fn exercise(kind: SubjectKind, boundaries: &[Boundary]) {
    for profile in PROFILES {
        let original = DedicatedTestHooks::default();
        let mut cases = Vec::new();
        for &boundary in boundaries {
            for change in [Change::Contents, Change::SameBytesNewIdentity] {
                cases.push((
                    boundary,
                    change,
                    changed_case(profile, kind, boundary, change, &original).await,
                ));
            }
        }
        // The actual same supervisor hook must count a successful Command::spawn.
        positive(profile, &original).await;
        for (boundary, change, facts) in cases {
            assert_eq!(
                facts.spawns, 0,
                "voice_pre_unapproved_spawn: {profile:?}/{kind:?}/{boundary:?}/{change:?}"
            );
            assert!(facts.changed_once && facts.edit_preserved);
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
}

#[tokio::test]
async fn voice_configuration_drift_after_apply_cannot_reach_original_supervisor_spawn() {
    exercise(SubjectKind::VoiceConfiguration, &[Boundary::AfterApply]).await;
}
#[tokio::test]
async fn voice_configuration_drift_before_start_cannot_replace_the_approved_observation() {
    exercise(SubjectKind::VoiceConfiguration, &[Boundary::BeforeStart]).await;
}
#[tokio::test]
async fn voice_configuration_drift_after_spawn_marker_requires_new_recovery_without_spawn() {
    exercise(SubjectKind::VoiceConfiguration, &[Boundary::MayHaveSpawned]).await;
}
#[tokio::test]
async fn properties_drift_keeps_both_voice_profiles_on_the_same_pre_and_post_marker_barriers() {
    exercise(
        SubjectKind::Properties,
        &[
            Boundary::AfterApply,
            Boundary::BeforeStart,
            Boundary::MayHaveSpawned,
        ],
    )
    .await;
}

#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct RecipeView {
    executable: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
    environment: BTreeMap<String, String>,
    temp: PathBuf,
    endpoint: SocketAddr,
    protocol: i32,
    limits: LimitsView,
    synthetic: bool,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct LimitsView {
    lifetime_ms: u64,
    readiness_ms: u64,
    stop_ms: u64,
    output_bytes: u64,
    attempts: u32,
}

#[test]
fn execution_review_binds_exact_tuple2_profile_manifest_files_and_unmodified_compiled_recipe() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        let file_digest = prepared.review().digest.clone();
        let deadline = prepared.approval_deadline();
        let before = files::tree(&paths.bundle);
        // This is the production builder. It plans verified fixture resources
        // without executing their java.exe or replacing its argv with a Child.
        let plan = producer::plan_voice(prepared).expect("voice_pre_production_plan");
        assert_eq!(plan.review().purpose, "native_voice_execution_v2");
        assert_ne!(plan.review().digest, file_digest);
        let (prepared, approval, _) = plan.into_parts();
        assert_eq!(prepared.approval_deadline(), deadline);
        assert!(matches!(&prepared.bundle.manifest, Manifest::VoiceV2(_)));
        assert_eq!(
            prepared.bundle.manifest.voice_v2().unwrap().profile,
            fixture.stored_profile()
        );
        let expected_input = encode(&(
            2,
            "native_voice_execution_v2",
            approval.review.review_id,
            &prepared.bundle.manifest,
            &file_digest,
            &approval.recipe,
        ))
        .expect("voice_pre_expected_exact_preimage");
        // Agreement with the real review digest checks its actual production
        // preimage. No raw-payload hash or parallel legacy encoder is used.
        assert_eq!(digest(&expected_input), approval.review.digest);
        let wrong_version = encode(&(
            1,
            "native_dedicated_status_execution",
            approval.review.review_id,
            &prepared.bundle.manifest,
            &file_digest,
            &approval.recipe,
        ))
        .unwrap();
        assert_ne!(digest(&wrong_version), approval.review.digest);
        let recipe: RecipeView =
            serde_json::from_slice(&encode(&approval.recipe).unwrap()).unwrap();
        // Prove that omitting only this argument changes the production
        // review. Verify the test view's byte order before changing it.
        let mut without_bundler: RecipeView =
            serde_json::from_slice(&encode(&approval.recipe).unwrap()).unwrap();
        assert_eq!(
            encode(&without_bundler).unwrap(),
            encode(&approval.recipe).unwrap()
        );
        assert_eq!(
            without_bundler.args.remove(5),
            "-DbundlerRepoDir=.connect-run-temp"
        );
        let missing_bundler_input = encode(&(
            2,
            "native_voice_execution_v2",
            approval.review.review_id,
            &prepared.bundle.manifest,
            &file_digest,
            &without_bundler,
        ))
        .unwrap();
        assert_ne!(digest(&missing_bundler_input), approval.review.digest);
        assert_eq!(recipe.executable, fixture.fixture.jdk.join("bin/java.exe"));
        assert_eq!(recipe.cwd, paths.copy);
        assert_eq!(recipe.temp, recipe.cwd.join(".connect-run-temp"));
        assert_eq!(recipe.endpoint, producer::ENDPOINT);
        assert_eq!(recipe.protocol, 767);
        assert!(!recipe.synthetic);
        assert_eq!(
            recipe.args,
            vec![
                "-Djava.io.tmpdir=.connect-run-temp".to_owned(),
                "-Duser.home=.connect-run-temp".into(),
                "-XX:-UsePerfData".into(),
                "-Xms256M".into(),
                "-Xmx1536M".into(),
                "-DbundlerRepoDir=.connect-run-temp".into(),
                "-Dfabric.gameJarPath=server.jar".into(),
                "-cp".into(),
                resources::status_classpath(),
                "net.fabricmc.loader.impl.launch.knot.KnotServer".into(),
                "nogui".into(),
            ]
        );
        assert_eq!(
            (
                recipe.limits.lifetime_ms,
                recipe.limits.readiness_ms,
                recipe.limits.stop_ms
            ),
            (300_000, 180_000, 5_000)
        );
        assert_eq!(
            (recipe.limits.output_bytes, recipe.limits.attempts),
            (4 * 1024 * 1024, 180)
        );
        assert_eq!(
            recipe
                .environment
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["SystemRoot", "TEMP", "TMP", "USERPROFILE", "windir"]
        );
        assert_eq!(
            recipe.environment["SystemRoot"],
            recipe.environment["windir"]
        );
        for key in ["TEMP", "TMP", "USERPROFILE"] {
            assert_eq!(Path::new(&recipe.environment[key]), recipe.temp);
        }
        assert!(approval
            .review
            .details
            .iter()
            .any(|detail| detail.contains(fixture.stored_profile().configuration_name())));
        approval
            .check(&prepared)
            .expect("voice_pre_own_approval_valid");
        files::assert_tree(&paths.bundle, &before);
        assert!(!slot(&paths, "claim.json").exists());
        drop((approval, prepared));
        unlocked(&paths);
    }
}

#[tokio::test]
async fn file_review_wrong_digest_original_expiry_and_cancel_never_apply_or_spawn() {
    for profile in PROFILES {
        let original = DedicatedTestHooks::default();
        for refusal in ["file-review", "expired", "cancelled"] {
            let fixture = VoiceFixture::new(profile);
            let harness = Harness::new(fixture.stored_profile());
            let mut prepared = fixture.prepared(Hooks::default());
            if refusal == "expired" {
                prepared.shorten_approval_for_test(Duration::from_secs(3));
            }
            let deadline = prepared.approval_deadline();
            let file_digest = prepared.review().digest.clone();
            let paths = prepared.test_paths();
            let before = files::tree(&paths.bundle);
            let plan = execution(
                prepared,
                &harness,
                Arc::new(TestHooks {
                    owned: original.clone(),
                    ..TestHooks::default()
                }),
            );
            let approved = plan.review().digest.clone();
            assert_ne!(approved, file_digest);
            let cancel = CancellationToken::new();
            let expected = match refusal {
                "file-review" => DedicatedError::Preparation(Error::Digest),
                "expired" => {
                    assert!(
                        Instant::now() < deadline,
                        "voice_pre_expiry_initially_valid"
                    );
                    tokio::time::sleep(
                        deadline.saturating_duration_since(Instant::now())
                            + Duration::from_millis(25),
                    )
                    .await;
                    DedicatedError::Preparation(Error::Expired)
                }
                "cancelled" => {
                    cancel.cancel();
                    DedicatedError::Cancelled
                }
                _ => unreachable!(),
            };
            let supplied = if refusal == "file-review" {
                &file_digest
            } else {
                &approved
            };
            assert_eq!(plan.start(supplied, cancel).err(), Some(expected));
            assert_eq!(original.spawn_attempts(), 0);
            assert!(!slot(&paths, "claim.json").exists());
            assert!(!paths.copy.join(".connect-run-temp").exists());
            assert_eq!(bytes(&target(&paths)), EULA_FALSE);
            files::assert_tree(&paths.bundle, &before);
            unlocked(&paths);
            harness.assert_no_child();
            harness.ports_released();
            harness.finish();
        }
        positive(profile, &original).await;
        assert_eq!(
            original.spawn_attempts(),
            1,
            "voice_pre_refusal_counter_connected_to_spawn"
        );
    }
}

#[test]
fn crossed_status_and_voice_approvals_and_different_voice_profiles_fail_kind_before_other_checks() {
    let status_fixture = Fixture::new();
    let status = producer::plan(status_fixture.prepared(Hooks::default()))
        .expect("voice_pre_status_approval")
        .into_parts();
    let svc_fixture = VoiceFixture::new(NativeVoiceProfile::SimpleVoiceChat);
    let svc = producer::plan_voice(svc_fixture.prepared(Hooks::default()))
        .expect("voice_pre_svc_approval")
        .into_parts();
    let plasmo_fixture = VoiceFixture::new(NativeVoiceProfile::PlasmoVoice);
    let plasmo = producer::plan_voice(plasmo_fixture.prepared(Hooks::default()))
        .expect("voice_pre_plasmo_approval")
        .into_parts();
    let subjects = [&status, &svc, &plasmo];
    let before: Vec<_> = subjects
        .iter()
        .map(|subject| files::tree(&subject.0.test_paths().bundle))
        .collect();
    for (index, subject) in subjects.iter().enumerate() {
        subject
            .1
            .check(&subject.0)
            .expect("voice_pre_matching_approval_positive");
        for (other_index, other) in subjects.iter().enumerate() {
            if index != other_index {
                // Both approvals/preparations were independently produced by
                // their real APIs. Nothing is cloned, relabelled or fabricated.
                assert_eq!(subject.1.check(&other.0), Err(Error::Identity));
            }
        }
        subject
            .1
            .check(&subject.0)
            .expect("voice_pre_original_approval_still_valid");
        let paths = subject.0.test_paths();
        files::assert_tree(&paths.bundle, &before[index]);
        assert!(!slot(&paths, "claim.json").exists());
        assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    }
}

#[tokio::test]
async fn voice_and_status_kind_gates_precede_recipe_io_and_both_own_child_paths_are_positive() {
    let original = DedicatedTestHooks::default();
    // A real STATUS recipe reaches the already-existing-temp check. Its voice
    // counterpart must reject kind first, with Identity instead of Consumed.
    for voice_api in [false, true] {
        let fixture = Fixture::new();
        let prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        fs::create_dir(paths.copy.join(".connect-run-temp")).unwrap();
        let before = files::tree(&paths.bundle);
        let error = if voice_api {
            producer::plan_voice(prepared).err()
        } else {
            producer::plan(prepared).err()
        };
        assert_eq!(
            error,
            Some(DedicatedError::Preparation(if voice_api {
                Error::Identity
            } else {
                Error::Consumed
            }))
        );
        files::assert_tree(&paths.bundle, &before);
        assert!(!slot(&paths, "claim.json").exists());
        unlocked(&paths);
    }
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        fs::create_dir(paths.copy.join(".connect-run-temp")).unwrap();
        let before = files::tree(&paths.bundle);
        assert_eq!(
            producer::plan(prepared).err(),
            Some(DedicatedError::Preparation(Error::Identity))
        );
        files::assert_tree(&paths.bundle, &before);
        unlocked(&paths);
        positive(profile, &original).await;
    }
    let fixture = Fixture::new();
    let mut harness = super::producer_fixture::Harness::new();
    let prepared = fixture.prepared(Hooks::default());
    let paths = prepared.test_paths();
    let plan = producer::plan_for_test(
        prepared,
        TestRecipe {
            control: harness.control_addr(),
            nonce: Uuid::parse_str(harness.nonce()).unwrap(),
            mode: FixtureMode::Normal,
        },
        limits(),
        Arc::new(TestHooks {
            owned: original.clone(),
            ..TestHooks::default()
        }),
    )
    .expect("voice_pre_v1_actual_plan");
    let before = original.spawn_attempts();
    let running = Running::start(plan, CancellationToken::new());
    harness.ready();
    ready(running.control()).await;
    harness.status_observed(true);
    assert!(running.control().voice_source().is_none());
    assert_eq!(running.control().source().observe_tcp(), Ok(()));
    let report = running.finish().await;
    assert_eq!(original.spawn_attempts() - before, 1);
    assert_eq!(
        original.spawn_attempts(),
        3,
        "voice_pre_v1_and_two_v2_original_spawns"
    );
    assert_eq!(report.eula, EulaDisposition::Restored);
    assert_eq!(report.reason, DedicatedReason::Requested);
    assert_eq!(report.failure, None);
    assert!(report.reached_ready);
    let process = report.process.expect("voice_pre_status_original_report");
    assert!(process.reaped && process.readers_complete && !process.forced);
    assert_eq!(process.exit_code, Some(0));
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    unlocked(&paths);
    super::producer_fixture::Harness::assert_status_port_released();
    harness.finish();
}
