//! B2 file-only EULA oracles over admitted physical voice fixtures. No test here
//! starts Java, constructs a Child, or treats MayHaveSpawned as proof of start.
use super::voice_fixture::{VoiceFixture, PROFILES};
use super::*;
use crate::engine::connect_voice::dedicated::manifest::StoredVoiceProfileV2;
use probe::Refusal;
use serde::de::IgnoredAny;
use std::sync::Mutex;

const CONFIGURATION_LIMIT: u64 = 256 * 1024;
const PREFIXES: [Step; 13] = [
    Step::Claim,
    Step::Snapshot,
    Step::Stage,
    Step::Intent,
    Step::OriginalMoved,
    Step::OriginalMarked,
    Step::AppliedMoved,
    Step::AppliedMarked,
    Step::RestoreIntent,
    Step::AppliedParked,
    Step::OriginalRestored,
    Step::RestoredMarked,
    Step::MayHaveSpawned,
];

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ReviewedVoiceFiles {
    original: Observation,
    properties: Observation,
    voice_configuration: Observation,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimView {
    schema: u32,
    bundle: BundleId,
    transaction: Uuid,
    manifest_sha256: String,
    review_digest: String,
    original: Observation,
}

fn assert_claim(paths: &TestPaths, id: BundleId, approval: &str, original: &Observation) {
    let raw = bytes(&slot(paths, "claim.json"));
    let claim: ClaimView = serde_json::from_slice(&raw).expect("voice_claim_schema1");
    assert_eq!(claim.schema, 1);
    assert_eq!(claim.bundle, id);
    assert!(!claim.transaction.is_nil());
    assert_eq!(
        claim.manifest_sha256,
        digest(&bytes(&paths.bundle.join("bundle.json")))
    );
    assert_eq!(claim.review_digest, approval);
    assert_eq!(&claim.original, original);
    assert_eq!(eula::roundtrip_claim_for_test(&raw).unwrap(), raw);
}

fn assert_recovery_preimage(fixture: &VoiceFixture, plan: &EulaRecoveryPlan, id: BundleId) {
    let raw = eula::recovery_input_for_test(plan).expect("voice_recovery_production_input");
    let (schema, purpose, review_id, profile, bundle_id, state_digest): (
        u32,
        String,
        Uuid,
        StoredVoiceProfileV2,
        BundleId,
        String,
    ) = serde_json::from_slice(&raw).expect("voice_recovery_exact_tuple");
    assert_eq!(schema, 2);
    assert_eq!(purpose, "native_voice_eula_recovery_v2");
    assert_eq!(review_id, plan.review().review_id);
    assert_eq!(profile, fixture.stored_profile());
    assert_eq!(bundle_id, id);
    assert_eq!(state_digest.len(), 64);
    assert!(state_digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(digest(&raw), plan.review().digest);
}

struct Invariants {
    origin: files::Tree,
    stage: files::Tree,
    jdk: files::Tree,
}
impl Invariants {
    fn capture(fixture: &VoiceFixture, paths: &TestPaths) -> Self {
        Self {
            origin: files::tree(&paths.origin),
            stage: files::tree(&fixture.fixture.stage),
            jdk: files::tree(&fixture.fixture.jdk),
        }
    }

    fn assert_unchanged(&self, fixture: &VoiceFixture, paths: &TestPaths) {
        files::assert_tree(&paths.origin, &self.origin);
        files::assert_tree(&fixture.fixture.stage, &self.stage);
        files::assert_tree(&fixture.fixture.jdk, &self.jdk);
        assert!(!paths.copy.join(".connect-run-temp").exists());
    }
}

#[track_caller]
fn observed(path: &Path) -> (Observation, Vec<u8>) {
    observe_file(path, CONFIGURATION_LIMIT).expect("voice_eula_observation")
}

#[track_caller]
fn no_claim(paths: &TestPaths) {
    assert_eq!(bytes(&target(paths)), EULA_FALSE);
    assert!(!slot(paths, "claim.json").exists());
    assert_eq!(
        fs::read_dir(&paths.transaction)
            .expect("voice_empty_transaction")
            .count(),
        0
    );
    unlocked(paths);
}

#[derive(Clone, Copy)]
enum ConfigurationMutation {
    Rewrite,
    SameBytesReplacement,
    Missing,
}

struct ConfigurationState {
    path: PathBuf,
    current: Option<(Observation, Vec<u8>)>,
    retained: Option<(PathBuf, Observation, Vec<u8>)>,
}
impl ConfigurationState {
    fn assert_unchanged(&self) {
        match &self.current {
            Some(expected) => assert_eq!(&observed(&self.path), expected),
            None => assert_eq!(
                fs::symlink_metadata(&self.path)
                    .expect_err("voice_configuration_stays_absent")
                    .kind(),
                std::io::ErrorKind::NotFound
            ),
        }
        if let Some((path, observation, bytes)) = &self.retained {
            assert_eq!(&observed(path), &(observation.clone(), bytes.clone()));
        }
    }
}

fn mutate_configuration(
    fixture: &VoiceFixture,
    paths: &TestPaths,
    mutation: ConfigurationMutation,
) -> ConfigurationState {
    let path = fixture.configuration_path(paths);
    let original = observed(&path);
    let retained = fixture
        .fixture
        .root
        .path()
        .join("retained-voice-configuration");
    match mutation {
        ConfigurationMutation::Rewrite => {
            fs::write(&path, b"# independent mutable voice configuration\n")
                .expect("voice_configuration_rewrite");
            let changed = observed(&path);
            assert_eq!(changed.0.identity, original.0.identity);
            assert_ne!(changed.1, original.1);
            ConfigurationState {
                path,
                current: Some(changed),
                retained: None,
            }
        }
        ConfigurationMutation::SameBytesReplacement | ConfigurationMutation::Missing => {
            fs::rename(&path, &retained).expect("voice_configuration_retain_original");
            let current = if matches!(mutation, ConfigurationMutation::SameBytesReplacement) {
                fs::write(&path, &original.1).expect("voice_configuration_new_identity");
                let replaced = observed(&path);
                assert_eq!(replaced.1, original.1);
                assert_ne!(replaced.0.identity, original.0.identity);
                Some(replaced)
            } else {
                None
            };
            ConfigurationState {
                path,
                current,
                retained: Some((retained, original.0, original.1)),
            }
        }
    }
}

struct Interrupted {
    id: BundleId,
    paths: TestPaths,
    original: (Observation, Vec<u8>),
    configuration: (Observation, Vec<u8>),
    review_id: Uuid,
    approval: String,
    invariants: Invariants,
}

fn interrupt(fixture: &VoiceFixture, step: Step) -> Interrupted {
    let prepared = fixture.prepared(fixture::hook_fault(step));
    let paths = prepared.test_paths();
    let interrupted = Interrupted {
        id: prepared.bundle_id(),
        original: observe_file(&target(&paths), 4096).expect("voice_original_eula"),
        configuration: observed(&fixture.configuration_path(&paths)),
        review_id: prepared.review().review_id,
        approval: prepared.review().digest.clone(),
        invariants: Invariants::capture(fixture, &paths),
        paths,
    };
    let result = match step {
        Step::RestoreIntent
        | Step::AppliedParked
        | Step::OriginalRestored
        | Step::RestoredMarked => prepared
            .apply_eula(&interrupted.approval)
            .expect("voice_apply_before_restore_fault")
            .restore_before_spawn()
            .map(|_| ()),
        Step::MayHaveSpawned => {
            let mut applied = prepared
                .apply_eula(&interrupted.approval)
                .expect("voice_apply_before_spawn_marker_fault");
            let result = applied.mark_may_have_spawned();
            drop(applied);
            result
        }
        _ => prepared.apply_eula(&interrupted.approval).map(|_| ()),
    };
    assert_eq!(result, Err(Error::Injected));
    // All original plans, pins and guards dropped normally before reopening.
    // The marker branch records only a file-only intent, never a Child/report.
    unlocked(&interrupted.paths);
    interrupted
        .invariants
        .assert_unchanged(fixture, &interrupted.paths);
    interrupted
}

fn expected_recovery(step: Step) -> (Option<EulaOutcome>, usize) {
    match step {
        Step::Snapshot | Step::Stage => (None, 0),
        Step::Claim | Step::Intent => (Some(EulaOutcome::Unchanged), 0),
        Step::OriginalMoved | Step::OriginalMarked => (Some(EulaOutcome::Unchanged), 1),
        Step::AppliedMoved | Step::AppliedMarked | Step::RestoreIntent | Step::MayHaveSpawned => {
            (Some(EulaOutcome::Restored), 2)
        }
        Step::AppliedParked => (Some(EulaOutcome::Restored), 1),
        Step::OriginalRestored | Step::RestoredMarked => (Some(EulaOutcome::Restored), 0),
    }
}

type ExpectedPrefix<'a> = (Option<&'a [u8]>, Vec<(&'a str, &'a [u8])>);

fn assert_prefix(paths: &TestPaths, step: Step) {
    let (target_bytes, expected): ExpectedPrefix<'_> = match step {
        Step::Claim => (Some(EULA_FALSE), vec![]),
        Step::Snapshot => (Some(EULA_FALSE), vec![("snapshot.bin", EULA_FALSE)]),
        Step::Stage | Step::Intent => (
            Some(EULA_FALSE),
            vec![("snapshot.bin", EULA_FALSE), ("stage.bin", EULA_TRUE)],
        ),
        Step::OriginalMoved | Step::OriginalMarked => (
            None,
            vec![
                ("snapshot.bin", EULA_FALSE),
                ("stage.bin", EULA_TRUE),
                ("old.bin", EULA_FALSE),
            ],
        ),
        Step::AppliedMoved | Step::AppliedMarked | Step::RestoreIntent | Step::MayHaveSpawned => (
            Some(EULA_TRUE),
            vec![("snapshot.bin", EULA_FALSE), ("old.bin", EULA_FALSE)],
        ),
        Step::AppliedParked => (
            None,
            vec![
                ("snapshot.bin", EULA_FALSE),
                ("old.bin", EULA_FALSE),
                ("applied.bin", EULA_TRUE),
            ],
        ),
        Step::OriginalRestored | Step::RestoredMarked => (
            Some(EULA_FALSE),
            vec![("snapshot.bin", EULA_FALSE), ("applied.bin", EULA_TRUE)],
        ),
    };
    if let Some(expected) = target_bytes {
        assert_eq!(bytes(&target(paths)), expected);
    } else {
        assert!(!target(paths).exists());
    }
    for name in ["snapshot.bin", "stage.bin", "old.bin", "applied.bin"] {
        match expected.iter().find(|(expected, _)| *expected == name) {
            Some((_, value)) => assert_eq!(bytes(&slot(paths, name)), *value),
            None => assert!(!slot(paths, name).exists()),
        }
    }
    assert_eq!(
        slot(paths, "may-have-spawned.json").exists(),
        step == Step::MayHaveSpawned
    );
}

#[test]
fn voice_eula_review_binds_exact_initial_observation_and_separate_v2_preimage() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let hooks = Hooks::default();
        let prepared = fixture.prepared(hooks.clone());
        let paths = prepared.test_paths();
        let invariants = Invariants::capture(&fixture, &paths);
        let original = observe_file(&target(&paths), 4096).unwrap();
        let configuration = observed(&fixture.configuration_path(&paths));
        assert_eq!(configuration.1, fixture.stored_profile().configuration());
        assert_eq!(
            prepared
                .bundle
                .manifest
                .voice_v2()
                .unwrap()
                .initial_voice_configuration
                .observation,
            configuration.0
        );
        let approved = encode(&prepared.approved_files).unwrap();
        let decoded: ReviewedVoiceFiles = serde_json::from_slice(&approved).unwrap();
        assert_eq!(decoded.original, original.0);
        assert_eq!(
            decoded.properties,
            observed(&paths.copy.join("server.properties")).0
        );
        assert_eq!(decoded.voice_configuration, configuration.0);
        assert_eq!(encode(&decoded).unwrap(), approved);
        let input = eula::eula_input_for_test(&prepared).unwrap();
        let (version, purpose, review_id, _, files, after): (
            u32,
            String,
            Uuid,
            IgnoredAny,
            ReviewedVoiceFiles,
            String,
        ) = serde_json::from_slice(&input).unwrap();
        assert_eq!(version, 2);
        assert_eq!(purpose, "native_voice_eula_v2");
        assert_eq!(review_id, prepared.review().review_id);
        assert_eq!(files, decoded);
        assert_eq!(after, digest(EULA_TRUE));
        assert_eq!(digest(&input), prepared.review().digest);
        assert_eq!(prepared.review().purpose, "native_voice_eula_v2");

        let id = prepared.bundle_id();
        let approval = prepared.review().digest.clone();
        let applied = prepared.apply_eula(&approval).expect("voice_eula_apply");
        assert_claim(&paths, id, &approval, &original.0);
        assert_eq!(bytes(&target(&paths)), EULA_TRUE);
        assert_eq!(observed(&fixture.configuration_path(&paths)), configuration);
        assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 2);
        assert_eq!(
            applied.restore_before_spawn().expect("voice_eula_restore"),
            EulaOutcome::Restored
        );
        assert_eq!(observe_file(&target(&paths), 4096).unwrap(), original);
        assert_eq!(observed(&fixture.configuration_path(&paths)), configuration);
        assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 4);
        invariants.assert_unchanged(&fixture, &paths);
        unlocked(&paths);
    }
}

#[test]
fn voice_configuration_edit_or_identical_swap_after_review_or_at_preclaim_is_not_recaptured() {
    for profile in PROFILES {
        for mutation in [
            ConfigurationMutation::Rewrite,
            ConfigurationMutation::SameBytesReplacement,
        ] {
            for at_preclaim in [false, true] {
                let fixture = VoiceFixture::new(profile);
                let selected = Arc::new(Mutex::new(None::<TestPaths>));
                let intercepted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let retained = fixture.fixture.root.path().join("retained-preclaim-voice");
                let selected_for_hook = selected.clone();
                let calls_for_hook = intercepted.clone();
                let retained_for_hook = retained.clone();
                let configuration_name = fixture.stored_profile().configuration_name();
                let hooks = Hooks {
                    before_claim: at_preclaim.then(|| {
                        Arc::new(move || {
                            let paths = selected_for_hook
                                .lock()
                                .expect("voice_selected_lock")
                                .clone()
                                .expect("voice_selected_path");
                            let path = paths.copy.join(configuration_name);
                            match mutation {
                                ConfigurationMutation::Rewrite => {
                                    fs::write(path, b"# independent preclaim voice edit\n")?;
                                }
                                ConfigurationMutation::SameBytesReplacement => {
                                    fs::rename(&path, &retained_for_hook)?;
                                    fs::write(&path, fs::read(&retained_for_hook)?)?;
                                }
                                ConfigurationMutation::Missing => unreachable!(),
                            }
                            calls_for_hook.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        }) as Arc<dyn Fn() -> Result<()> + Send + Sync>
                    }),
                    ..Hooks::default()
                };
                let prepared = fixture.prepared(hooks.clone());
                let paths = prepared.test_paths();
                let invariants = Invariants::capture(&fixture, &paths);
                let configuration = fixture.configuration_path(&paths);
                let reviewed = observed(&configuration);
                let approved = encode(&prepared.approved_files).unwrap();
                let approval = prepared.review().digest.clone();
                *selected.lock().unwrap() = Some(paths.clone());
                let changed =
                    (!at_preclaim).then(|| mutate_configuration(&fixture, &paths, mutation));
                let before = (!at_preclaim).then(|| files::tree(fixture.fixture.root.path()));
                assert_eq!(prepared.apply_eula(&approval).err(), Some(Error::Changed));
                if let Some(before) = before {
                    files::assert_tree(fixture.fixture.root.path(), &before);
                }
                if let Some(changed) = changed {
                    changed.assert_unchanged();
                }
                let actual = observed(&configuration);
                match mutation {
                    ConfigurationMutation::Rewrite => {
                        assert_eq!(actual.0.identity, reviewed.0.identity);
                        assert_ne!(actual.1, reviewed.1);
                    }
                    ConfigurationMutation::SameBytesReplacement => {
                        assert_ne!(actual.0.identity, reviewed.0.identity);
                        assert_eq!(actual.1, reviewed.1);
                        if at_preclaim {
                            assert_eq!(observed(&retained), reviewed);
                        }
                    }
                    ConfigurationMutation::Missing => unreachable!(),
                }
                let original_approved: ReviewedVoiceFiles =
                    serde_json::from_slice(&approved).unwrap();
                assert_eq!(original_approved.voice_configuration, reviewed.0);
                assert_eq!(intercepted.load(Ordering::SeqCst), usize::from(at_preclaim));
                assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
                no_claim(&paths);
                invariants.assert_unchanged(&fixture, &paths);
            }
        }
    }
}

#[test]
fn voice_wrong_or_expired_eula_approval_cannot_publish_a_claim() {
    for profile in PROFILES {
        for expired in [false, true] {
            let fixture = VoiceFixture::new(profile);
            let hooks = Hooks::default();
            let mut prepared = fixture.prepared(hooks.clone());
            let paths = prepared.test_paths();
            let before = files::tree(fixture.fixture.root.path());
            let approval = if expired {
                let digest = prepared.review().digest.clone();
                prepared.expire_for_test();
                digest
            } else {
                "0".repeat(64)
            };
            assert_eq!(
                prepared.apply_eula(&approval).err(),
                Some(if expired {
                    Error::Expired
                } else {
                    Error::Digest
                })
            );
            files::assert_tree(fixture.fixture.root.path(), &before);
            assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
            no_claim(&paths);
        }
    }
}

#[test]
fn both_voice_profiles_reopen_all_thirteen_real_eula_prefixes_with_new_guard_and_review() {
    for profile in PROFILES {
        for step in PREFIXES {
            let fixture = VoiceFixture::new(profile);
            let crashed = interrupt(&fixture, step);
            let paths = &crashed.paths;
            assert_prefix(paths, step);
            let claim = bytes(&slot(paths, "claim.json"));
            assert_claim(paths, crashed.id, &crashed.approval, &crashed.original.0);
            let before = files::tree(&paths.bundle);
            drop(fixture.reopen(crashed.id).expect("voice_prefix_reopen"));
            files::assert_tree(&paths.bundle, &before);
            let probe = probe::Probe::default();
            let hooks = Hooks::default();
            let recovery = fixture.recover(crashed.id, &probe, hooks.clone());
            let (expected, renames) = expected_recovery(step);
            if let Some(outcome) = expected {
                let plan = recovery.expect("voice_recognized_prefix_recovery");
                assert_recovery_preimage(&fixture, &plan, crashed.id);
                assert_eq!(plan.review().purpose, "native_voice_eula_recovery_v2");
                assert_eq!(plan.review().bundle_id, crashed.id);
                assert_ne!(plan.review().review_id, crashed.review_id);
                assert_ne!(plan.review().digest, crashed.approval);
                let first_review = plan.review().review_id;
                let approval = plan.review().digest.clone();
                locked(paths);
                assert_eq!(plan.confirm(&approval).unwrap(), outcome);
                assert_eq!(
                    observe_file(&target(paths), 4096).unwrap(),
                    crashed.original
                );
                assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), renames);
                let terminal = files::tree(&paths.bundle);
                let fresh_probe = probe::Probe::default();
                let plan = fixture
                    .recover(crashed.id, &fresh_probe, hooks.clone())
                    .expect("voice_terminal_new_guard");
                assert_ne!(plan.review().review_id, first_review);
                let approval = plan.review().digest.clone();
                assert_eq!(plan.confirm(&approval).unwrap(), outcome);
                assert!(fresh_probe.calls() >= 3);
                assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), renames);
                files::assert_tree(&paths.bundle, &terminal);
            } else {
                assert_eq!(recovery.err(), Some(Error::NeedsRecovery));
                files::assert_tree(&paths.bundle, &before);
                assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
            }
            assert!(probe.calls() >= 2);
            assert_eq!(bytes(&slot(paths, "claim.json")), claim);
            assert_eq!(
                observed(&fixture.configuration_path(paths)),
                crashed.configuration
            );
            crashed.invariants.assert_unchanged(&fixture, paths);
            unlocked(paths);
        }
    }
}

#[test]
fn mutable_voice_configuration_rewrite_new_identity_or_absence_does_not_block_eula_recovery() {
    for profile in PROFILES {
        for mutation in [
            ConfigurationMutation::Rewrite,
            ConfigurationMutation::SameBytesReplacement,
            ConfigurationMutation::Missing,
        ] {
            let fixture = VoiceFixture::new(profile);
            let crashed = interrupt(&fixture, Step::AppliedMarked);
            let changed = mutate_configuration(&fixture, &crashed.paths, mutation);
            let claim = bytes(&slot(&crashed.paths, "claim.json"));
            let before = files::tree(fixture.fixture.root.path());
            drop(
                fixture
                    .reopen(crashed.id)
                    .expect("voice_mutable_configuration_reopen"),
            );
            files::assert_tree(fixture.fixture.root.path(), &before);
            let probe = probe::Probe::default();
            let hooks = Hooks::default();
            let plan = fixture
                .recover(crashed.id, &probe, hooks.clone())
                .expect("voice_mutable_configuration_recovery");
            assert_eq!(plan.review().purpose, "native_voice_eula_recovery_v2");
            let approval = plan.review().digest.clone();
            assert_eq!(plan.confirm(&approval).unwrap(), EulaOutcome::Restored);
            assert!(probe.calls() >= 3);
            assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 2);
            assert_eq!(
                observe_file(&target(&crashed.paths), 4096).unwrap(),
                crashed.original
            );
            assert_eq!(bytes(&slot(&crashed.paths, "claim.json")), claim);
            changed.assert_unchanged();
            crashed
                .invariants
                .assert_unchanged(&fixture, &crashed.paths);
            unlocked(&crashed.paths);
        }
    }
}

#[test]
fn voice_configuration_can_change_after_recovery_review_without_becoming_restore_input() {
    for profile in PROFILES {
        for mutation in [
            ConfigurationMutation::Rewrite,
            ConfigurationMutation::SameBytesReplacement,
            ConfigurationMutation::Missing,
        ] {
            let fixture = VoiceFixture::new(profile);
            let crashed = interrupt(&fixture, Step::MayHaveSpawned);
            let probe = probe::Probe::default();
            let hooks = Hooks::default();
            let plan = fixture
                .recover(crashed.id, &probe, hooks.clone())
                .expect("voice_review_before_mutable_edit");
            let approval = plan.review().digest.clone();
            let changed = mutate_configuration(&fixture, &crashed.paths, mutation);
            assert_eq!(plan.confirm(&approval).unwrap(), EulaOutcome::Restored);
            assert!(probe.calls() >= 3);
            assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 2);
            assert_eq!(
                observe_file(&target(&crashed.paths), 4096).unwrap(),
                crashed.original
            );
            changed.assert_unchanged();
            crashed
                .invariants
                .assert_unchanged(&fixture, &crashed.paths);
            unlocked(&crashed.paths);
        }
    }
}

#[test]
fn voice_recovery_requires_fresh_guard_and_its_own_review_after_configuration_drift() {
    for profile in PROFILES {
        for refusal in [Refusal::Running, Refusal::Unavailable] {
            let fixture = VoiceFixture::new(profile);
            let crashed = interrupt(&fixture, Step::AppliedMarked);
            let changed = mutate_configuration(
                &fixture,
                &crashed.paths,
                ConfigurationMutation::SameBytesReplacement,
            );
            let before = files::tree(fixture.fixture.root.path());
            let refused = probe::Probe::default();
            refused.refuse(2, refusal);
            let hooks = Hooks::default();
            assert_eq!(
                fixture.recover(crashed.id, &refused, hooks.clone()).err(),
                Some(match refusal {
                    Refusal::Running => Error::JavaRunning,
                    Refusal::Unavailable => Error::ProbeUnavailable,
                })
            );
            assert_eq!(refused.calls(), 2);
            files::assert_tree(fixture.fixture.root.path(), &before);
            unlocked(&crashed.paths);
            let probe = probe::Probe::default();
            let plan = fixture.recover(crashed.id, &probe, hooks.clone()).unwrap();
            assert_eq!(plan.confirm(&crashed.approval), Err(Error::Digest));
            assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
            files::assert_tree(fixture.fixture.root.path(), &before);
            unlocked(&crashed.paths);
            let fresh = probe::Probe::default();
            let plan = fixture.recover(crashed.id, &fresh, hooks.clone()).unwrap();
            let approval = plan.review().digest.clone();
            assert_eq!(plan.confirm(&approval).unwrap(), EulaOutcome::Restored);
            assert!(fresh.calls() >= 3);
            assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 2);
            assert_eq!(
                observe_file(&target(&crashed.paths), 4096).unwrap(),
                crashed.original
            );
            changed.assert_unchanged();
            crashed
                .invariants
                .assert_unchanged(&fixture, &crashed.paths);
            unlocked(&crashed.paths);
        }
    }
}

#[test]
fn voice_baseline_is_rechecked_before_spawn_intent_but_never_restored_as_eula() {
    for profile in PROFILES {
        for mutation in [
            ConfigurationMutation::Rewrite,
            ConfigurationMutation::SameBytesReplacement,
        ] {
            let fixture = VoiceFixture::new(profile);
            let hooks = Hooks::default();
            let prepared = fixture.prepared(hooks.clone());
            let paths = prepared.test_paths();
            let invariants = Invariants::capture(&fixture, &paths);
            let original = observe_file(&target(&paths), 4096).unwrap();
            let approval = prepared.review().digest.clone();
            let mut applied = prepared.apply_eula(&approval).unwrap();
            let changed = mutate_configuration(&fixture, &paths, mutation);
            assert_eq!(applied.mark_may_have_spawned(), Err(Error::Changed));
            assert!(!slot(&paths, "may-have-spawned.json").exists());
            assert_eq!(
                applied.restore_before_spawn().unwrap(),
                EulaOutcome::Restored
            );
            assert_eq!(observe_file(&target(&paths), 4096).unwrap(), original);
            assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 4);
            changed.assert_unchanged();
            invariants.assert_unchanged(&fixture, &paths);
            unlocked(&paths);
        }
    }
}

#[test]
fn voice_change_at_spawn_marker_is_caught_by_the_second_launch_files_recheck() {
    for profile in PROFILES {
        for mutation in [
            ConfigurationMutation::Rewrite,
            ConfigurationMutation::SameBytesReplacement,
        ] {
            let fixture = VoiceFixture::new(profile);
            let selected = Arc::new(Mutex::new(None::<PathBuf>));
            let selected_for_hook = selected.clone();
            let retained = fixture
                .fixture
                .root
                .path()
                .join("retained-post-marker-voice");
            let retained_for_hook = retained.clone();
            let hooks = Hooks {
                after_step: Some(Arc::new(move |step| {
                    if step == Step::MayHaveSpawned {
                        let path = selected_for_hook
                            .lock()
                            .expect("voice_post_marker_path_lock")
                            .clone()
                            .expect("voice_post_marker_path");
                        match mutation {
                            ConfigurationMutation::Rewrite => {
                                fs::write(
                                    path,
                                    b"# independent voice write after persisted spawn intent\n",
                                )?;
                            }
                            ConfigurationMutation::SameBytesReplacement => {
                                fs::rename(&path, &retained_for_hook)?;
                                fs::write(&path, fs::read(&retained_for_hook)?)?;
                            }
                            ConfigurationMutation::Missing => unreachable!(),
                        }
                    }
                    Ok(())
                })),
                ..Hooks::default()
            };
            let prepared = fixture.prepared(hooks.clone());
            let paths = prepared.test_paths();
            let id = prepared.bundle_id();
            let invariants = Invariants::capture(&fixture, &paths);
            let original = observe_file(&target(&paths), 4096).unwrap();
            let configuration_path = fixture.configuration_path(&paths);
            let reviewed = observed(&configuration_path);
            *selected.lock().unwrap() = Some(configuration_path.clone());
            let approval = prepared.review().digest.clone();
            let mut applied = prepared.apply_eula(&approval).unwrap();
            assert_eq!(applied.mark_may_have_spawned(), Ok(()));
            assert!(slot(&paths, "may-have-spawned.json").is_file());
            let changed = observed(&configuration_path);
            let retained_state = match mutation {
                ConfigurationMutation::Rewrite => {
                    assert_eq!(changed.0.identity, reviewed.0.identity);
                    assert_ne!(changed.1, reviewed.1);
                    None
                }
                ConfigurationMutation::SameBytesReplacement => {
                    assert_ne!(changed.0.identity, reviewed.0.identity);
                    assert_eq!(changed.1, reviewed.1);
                    assert_eq!(observed(&retained), reviewed);
                    Some((retained, reviewed.0, reviewed.1))
                }
                ConfigurationMutation::Missing => unreachable!(),
            };
            let configuration = ConfigurationState {
                path: configuration_path,
                current: Some(changed),
                retained: retained_state,
            };
            // The worker uses this same second check after the marker hook. This
            // file-only test exercises the gate, never dispatching a process.
            assert_eq!(applied.recheck_approved_launch_files(), Err(Error::Changed));
            assert_eq!(applied.restore_before_spawn(), Err(Error::NeedsRecovery));
            assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 2);
            unlocked(&paths);
            let probe = probe::Probe::default();
            let recovery_hooks = Hooks::default();
            let plan = fixture.recover(id, &probe, recovery_hooks.clone()).unwrap();
            let approval = plan.review().digest.clone();
            assert_eq!(plan.confirm(&approval).unwrap(), EulaOutcome::Restored);
            assert_eq!(observe_file(&target(&paths), 4096).unwrap(), original);
            configuration.assert_unchanged();
            assert!(probe.calls() >= 3);
            assert_eq!(recovery_hooks.eula_renames.load(Ordering::SeqCst), 2);
            invariants.assert_unchanged(&fixture, &paths);
            unlocked(&paths);
        }
    }
}

#[test]
fn approved_files_from_a_real_status_preparation_and_voice_preparation_cannot_exchange_kinds() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let before_claim = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let before_claim_hook = before_claim.clone();
        let hooks = Hooks {
            before_claim: Some(Arc::new(move || {
                before_claim_hook.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })),
            ..Hooks::default()
        };
        let mut status = fixture.fixture.prepared(hooks.clone());
        let mut voice = fixture.prepared(hooks.clone());
        let status_paths = status.test_paths();
        let voice_paths = voice.test_paths();
        let status_approval = status.review().digest.clone();
        let voice_approval = voice.review().digest.clone();
        assert!(eula::eula_input_for_test(&status).is_ok());
        assert!(eula::eula_input_for_test(&voice).is_ok());
        let before = files::tree(fixture.fixture.root.path());
        std::mem::swap(&mut status.approved_files, &mut voice.approved_files);
        assert_eq!(eula::eula_input_for_test(&status), Err(Error::Identity));
        assert_eq!(eula::eula_input_for_test(&voice), Err(Error::Identity));
        assert_eq!(
            voice.apply_eula(&voice_approval).err(),
            Some(Error::Identity)
        );
        assert_eq!(
            status.apply_eula(&status_approval).err(),
            Some(Error::Identity)
        );
        assert_eq!(before_claim.load(Ordering::SeqCst), 0);
        assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
        files::assert_tree(fixture.fixture.root.path(), &before);
        no_claim(&voice_paths);
        no_claim(&status_paths);
    }
}
