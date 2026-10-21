use super::*;

const APPLY_STEPS: &[Step] = &[
    Step::Claim,
    Step::Snapshot,
    Step::Stage,
    Step::Intent,
    Step::OriginalMoved,
    Step::OriginalMarked,
    Step::AppliedMoved,
    Step::AppliedMarked,
];
const RESTORE_STEPS: &[Step] = &[
    Step::RestoreIntent,
    Step::AppliedParked,
    Step::OriginalRestored,
    Step::RestoredMarked,
];

fn assert_slots(paths: &TestPaths, target_bytes: Option<&[u8]>, expected: &[(&str, &[u8])]) {
    let target_path = target(paths);
    if let Some(expected) = target_bytes {
        assert_eq!(bytes(&target_path), expected);
    } else {
        assert!(!target_path.exists(), "target_must_be_really_absent");
    }
    for name in ["snapshot.bin", "stage.bin", "old.bin", "applied.bin"] {
        if let Some((_, value)) = expected.iter().find(|(candidate, _)| *candidate == name) {
            assert_eq!(bytes(&slot(paths, name)), *value);
        } else {
            assert!(!slot(paths, name).exists(), "unexpected_eula_slot");
        }
    }
}

fn crash(fixture: &Fixture, step: Step) -> (BundleId, TestPaths, files::Tree) {
    let prepared = fixture.prepared(fixture::hook_fault(step));
    let paths = prepared.test_paths();
    let id = prepared.bundle_id();
    let source = files::tree(&paths.origin);
    let approval = prepared.review().digest.clone();
    let result = if APPLY_STEPS.contains(&step) {
        prepared.apply_eula(&approval).map(|_| ())
    } else {
        prepared
            .apply_eula(&approval)
            .expect("fixture_apply_before_restore_fault")
            .restore_before_spawn()
            .map(|_| ())
    };
    assert_eq!(result, Err(Error::Injected));
    // No retained plan/transaction owns a lock here: the next operation must
    // reopen the actual persisted state, including the two absent-target gaps.
    unlocked(&paths);
    files::assert_tree(&paths.origin, &source);
    (id, paths, source)
}

#[test]
fn every_apply_and_restore_move_interval_reopens_its_real_disk_state() {
    for step in APPLY_STEPS.iter().chain(RESTORE_STEPS).copied() {
        let fixture = Fixture::new();
        let (id, paths, source) = crash(&fixture, step);
        assert!(slot(&paths, "claim.json").is_file());
        match step {
            Step::Claim => assert_slots(&paths, Some(EULA_FALSE), &[]),
            Step::Snapshot => {
                assert_slots(&paths, Some(EULA_FALSE), &[("snapshot.bin", EULA_FALSE)])
            }
            Step::Stage | Step::Intent => assert_slots(
                &paths,
                Some(EULA_FALSE),
                &[("snapshot.bin", EULA_FALSE), ("stage.bin", EULA_TRUE)],
            ),
            Step::OriginalMoved | Step::OriginalMarked => assert_slots(
                &paths,
                None,
                &[
                    ("snapshot.bin", EULA_FALSE),
                    ("stage.bin", EULA_TRUE),
                    ("old.bin", EULA_FALSE),
                ],
            ),
            Step::AppliedMoved | Step::AppliedMarked | Step::RestoreIntent => assert_slots(
                &paths,
                Some(EULA_TRUE),
                &[("snapshot.bin", EULA_FALSE), ("old.bin", EULA_FALSE)],
            ),
            Step::AppliedParked => assert_slots(
                &paths,
                None,
                &[
                    ("snapshot.bin", EULA_FALSE),
                    ("old.bin", EULA_FALSE),
                    ("applied.bin", EULA_TRUE),
                ],
            ),
            Step::OriginalRestored | Step::RestoredMarked => assert_slots(
                &paths,
                Some(EULA_FALSE),
                &[("snapshot.bin", EULA_FALSE), ("applied.bin", EULA_TRUE)],
            ),
            Step::MayHaveSpawned => unreachable!(),
        }

        let probe = probe::Probe::default();
        let hooks = Hooks::default();
        let before = files::tree(&paths.bundle);
        let plan = fixture.recover(id, &probe, hooks.clone());
        if matches!(step, Step::Snapshot | Step::Stage) {
            // An unpublished snapshot/stage identity is deliberately not
            // inferred from matching bytes or manufactured into an old slot.
            assert_eq!(plan.err(), Some(Error::NeedsRecovery));
            files::assert_tree(&paths.bundle, &before);
            assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
        } else {
            let plan = plan.expect("recovery_recognized_prefix");
            locked(&paths);
            let approval = plan.review().digest.clone();
            plan.confirm(&approval).expect("recover_recognized_prefix");
            assert_eq!(bytes(&target(&paths)), EULA_FALSE);
            assert!(slot(&paths, "claim.json").is_file());
            let expected_renames = match step {
                Step::Claim | Step::Intent | Step::OriginalRestored | Step::RestoredMarked => 0,
                Step::OriginalMoved | Step::OriginalMarked | Step::AppliedParked => 1,
                Step::AppliedMoved | Step::AppliedMarked | Step::RestoreIntent => 2,
                _ => unreachable!(),
            };
            assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), expected_renames);
            if matches!(
                step,
                Step::Intent | Step::OriginalMoved | Step::OriginalMarked
            ) {
                assert_eq!(bytes(&slot(&paths, "stage.bin")), EULA_TRUE);
            }
            let terminal = files::tree(&paths.bundle);
            let plan = fixture
                .recover(id, &probe::Probe::default(), hooks.clone())
                .expect("reopen_recovered_terminal");
            let approval = plan.review().digest.clone();
            plan.confirm(&approval).expect("verify_recovered_terminal");
            assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), expected_renames);
            files::assert_tree(&paths.bundle, &terminal);
        }
        assert!(probe.calls() >= 2, "recovery_must_acquire_real_guard");
        unlocked(&paths);
        files::assert_tree(&paths.origin, &source);
    }
}

#[test]
fn lost_original_is_never_reconstructed_from_a_valid_snapshot() {
    let fixture = Fixture::new();
    let (id, paths, _) = crash(&fixture, Step::AppliedMarked);
    fs::remove_file(slot(&paths, "old.bin")).expect("fixture_remove_old");
    assert_eq!(bytes(&slot(&paths, "snapshot.bin")), EULA_FALSE);
    let before = files::tree(&paths.bundle);
    let hooks = Hooks::default();
    assert!(fixture
        .recover(id, &probe::Probe::default(), hooks.clone())
        .is_err());
    files::assert_tree(&paths.bundle, &before);
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
    unlocked(&paths);
}

#[test]
fn same_bytes_in_a_replacement_file_do_not_inherit_original_identity() {
    for name in ["snapshot.bin", "old.bin", "target"] {
        let fixture = Fixture::new();
        let (id, paths, _) = crash(&fixture, Step::AppliedMarked);
        let path = if name == "target" {
            target(&paths)
        } else {
            slot(&paths, name)
        };
        let original_bytes = bytes(&path);
        let displaced = fixture.root.path().join("retained-original.bin");
        fs::rename(&path, &displaced).expect("fixture_move_original_identity");
        fs::write(&path, &original_bytes).expect("fixture_same_bytes_new_identity");
        let before = files::tree(fixture.root.path());
        let hooks = Hooks::default();
        assert!(fixture
            .recover(id, &probe::Probe::default(), hooks.clone())
            .is_err());
        files::assert_tree(fixture.root.path(), &before);
        assert_eq!(bytes(&displaced), original_bytes);
        assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
        unlocked(&paths);
    }
}

#[test]
fn edited_target_after_apply_is_preserved_with_snapshot_and_old() {
    let fixture = Fixture::new();
    let hooks = Hooks::default();
    let prepared = fixture.prepared(hooks.clone());
    let paths = prepared.test_paths();
    let id = prepared.bundle_id();
    let approval = prepared.review().digest.clone();
    let applied = prepared.apply_eula(&approval).expect("fixture_apply");
    let independent = b"# concurrent fixture writer\neula=true\n";
    fs::write(target(&paths), independent).expect("fixture_concurrent_edit");
    let claim_before = bytes(&slot(&paths, "claim.json"));
    assert!(applied.restore_before_spawn().is_err());
    assert_eq!(bytes(&target(&paths)), independent);
    assert_eq!(bytes(&slot(&paths, "snapshot.bin")), EULA_FALSE);
    assert_eq!(bytes(&slot(&paths, "old.bin")), EULA_FALSE);
    assert_eq!(bytes(&slot(&paths, "claim.json")), claim_before);
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 2);
    let before = files::tree(&paths.bundle);
    assert!(fixture
        .recover(id, &probe::Probe::default(), Hooks::default())
        .is_err());
    files::assert_tree(&paths.bundle, &before);
    unlocked(&paths);
}

#[test]
fn persisted_spawn_intent_irreversibly_removes_never_spawned_restore_authority() {
    for fault_after_marker in [false, true] {
        let fixture = Fixture::new();
        let hooks = if fault_after_marker {
            fixture::hook_fault(Step::MayHaveSpawned)
        } else {
            Hooks::default()
        };
        let prepared = fixture.prepared(hooks.clone());
        let paths = prepared.test_paths();
        let id = prepared.bundle_id();
        let approval = prepared.review().digest.clone();
        let mut applied = prepared
            .apply_eula(&approval)
            .expect("apply_before_spawn_intent");
        assert_eq!(
            applied.mark_may_have_spawned(),
            if fault_after_marker {
                Err(Error::Injected)
            } else {
                Ok(())
            }
        );
        assert!(slot(&paths, "may-have-spawned.json").is_file());
        assert_eq!(applied.mark_may_have_spawned(), Err(Error::Consumed));
        let before = files::tree(&paths.bundle);
        assert_eq!(applied.restore_before_spawn(), Err(Error::NeedsRecovery));
        files::assert_tree(&paths.bundle, &before);
        assert_eq!(bytes(&target(&paths)), EULA_TRUE);
        assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 2);
        unlocked(&paths);

        // This is the persisted "may have started" state, not evidence that
        // this file-only checkpoint executed a Child or received report83.
        let probe = probe::Probe::default();
        let recovery_hooks = Hooks::default();
        let plan = fixture
            .recover(id, &probe, recovery_hooks.clone())
            .expect("guarded_spawn_intent_recovery");
        let approval = plan.review().digest.clone();
        assert_eq!(
            plan.confirm(&approval).expect("recover_spawn_intent"),
            EulaOutcome::Restored
        );
        assert!(probe.calls() >= 3);
        assert_eq!(recovery_hooks.eula_renames.load(Ordering::SeqCst), 2);
        assert_eq!(bytes(&target(&paths)), EULA_FALSE);
        assert!(slot(&paths, "may-have-spawned.json").is_file());
        assert!(slot(&paths, "claim.json").is_file());
    }
}
