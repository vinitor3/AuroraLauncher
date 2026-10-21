use super::*;
use std::sync::{atomic::Ordering, Arc};

#[path = "tests/actual_producer.rs"]
mod actual_producer;
#[path = "tests/actual_resources.rs"]
mod actual_resources;
#[path = "tests/files.rs"]
mod files;
#[path = "tests/fixture.rs"]
mod fixture;
#[path = "tests/hardening.rs"]
mod hardening;
#[path = "tests/probe.rs"]
mod probe;
#[path = "tests/producer_boot_bounds.rs"]
mod producer_boot_bounds;
#[path = "tests/producer_fixture.rs"]
mod producer_fixture;
#[path = "tests/producer_hardening.rs"]
mod producer_hardening;
#[path = "tests/producer_lifecycle.rs"]
mod producer_lifecycle;
#[path = "tests/producer_pre_spawn.rs"]
mod producer_pre_spawn;
#[path = "tests/recovery_guards.rs"]
mod recovery_guards;
#[path = "tests/transactions.rs"]
mod transactions;
#[path = "tests/v1_upgrade_capture.rs"]
mod v1_upgrade_capture;

use fixture::{bytes, locked, slot, target, unlocked, Fixture};

#[test]
fn real_diagnostic_factory_and_eula_roundtrip_preserve_source_and_resources() {
    let fixture = Fixture::new();
    let resources_before = files::tree(&fixture.stage);
    let jdk_before = files::tree(&fixture.jdk);
    let hooks = Hooks::default();
    let prepared = fixture.prepared(hooks.clone());
    let paths = prepared.test_paths();
    let id = prepared.bundle_id();
    let source_before = files::tree(&paths.origin);
    assert!(diagnostic_lab::is_lab_candidate(&fixture::copy_instance(
        &paths
    )));
    locked(&paths);
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);

    let approval = prepared.review().digest.clone();
    let applied = prepared.apply_eula(&approval).expect("eula_apply");
    assert_eq!(applied.test_paths().copy, paths.copy);
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    assert_eq!(bytes(&slot(&paths, "snapshot.bin")), EULA_FALSE);
    assert_eq!(bytes(&slot(&paths, "old.bin")), EULA_FALSE);
    assert!(!slot(&paths, "stage.bin").exists());
    assert!(slot(&paths, "claim.json").is_file());
    assert!(slot(&paths, "intent.json").is_file());
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 2);
    locked(&paths);

    assert_eq!(
        applied.restore_before_spawn().expect("eula_restore"),
        EulaOutcome::Restored
    );
    unlocked(&paths);
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    assert_eq!(bytes(&slot(&paths, "applied.bin")), EULA_TRUE);
    assert!(!slot(&paths, "old.bin").exists());
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 4);
    let terminal = files::tree(&paths.transaction);
    let probe = probe::Probe::default();
    let plan = fixture
        .recover(id, &probe, hooks.clone())
        .expect("reopen_restored");
    assert_eq!(plan.test_paths().transaction, paths.transaction);
    let approval = plan.review().digest.clone();
    assert_eq!(
        plan.confirm(&approval).expect("verify_restored"),
        EulaOutcome::Restored
    );
    assert!(probe.calls() >= 3, "recovery_must_recheck_real_guard");
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 4);
    files::assert_tree(&paths.transaction, &terminal);
    assert!(slot(&paths, "claim.json").is_file());
    files::assert_tree(&paths.origin, &source_before);
    files::assert_tree(&fixture.stage, &resources_before);
    files::assert_tree(&fixture.jdk, &jdk_before);
}

#[test]
fn absent_consent_and_wrong_or_expired_factory_approval_create_nothing() {
    let fixture = Fixture::new();
    let before = files::tree(fixture.root.path());
    assert!(matches!(
        fixture_plan(
            &fixture.lab,
            &fixture.stage,
            &fixture.jdk,
            false,
            Hooks::default()
        ),
        Err(Error::Consent)
    ));
    files::assert_tree(fixture.root.path(), &before);
    let plan = fixture.plan(Hooks::default());
    assert!(matches!(plan.create(&"0".repeat(64)), Err(Error::Digest)));
    files::assert_tree(fixture.root.path(), &before);
    let mut plan = fixture.plan(Hooks::default());
    let approval = plan.review().digest.clone();
    plan.expire_for_test();
    assert!(matches!(plan.create(&approval), Err(Error::Expired)));
    files::assert_tree(fixture.root.path(), &before);

    // A valid approval through the same factory creates an actual diagnostic
    // copy, so the preceding no-creation oracle is not a disconnected counter.
    let prepared = fixture.prepared(Hooks::default());
    let paths = prepared.test_paths();
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    assert_ne!(files::tree(fixture.root.path()), before);
    drop(prepared);
    unlocked(&paths);
}

#[test]
fn wrong_and_expired_execution_approval_never_touch_target_or_transaction() {
    for expired in [false, true] {
        let fixture = Fixture::new();
        let hooks = Hooks::default();
        let mut prepared = fixture.prepared(hooks.clone());
        let paths = prepared.test_paths();
        let before = files::tree(&paths.bundle);
        let approval = if expired {
            let approval = prepared.review().digest.clone();
            prepared.expire_for_test();
            approval
        } else {
            "0".repeat(64)
        };
        let result = prepared.apply_eula(&approval);
        assert_eq!(
            result.err(),
            Some(if expired {
                Error::Expired
            } else {
                Error::Digest
            })
        );
        files::assert_tree(&paths.bundle, &before);
        assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
        assert_eq!(bytes(&target(&paths)), EULA_FALSE);
        unlocked(&paths);

        let valid = fixture.prepared(hooks.clone());
        let valid_paths = valid.test_paths();
        let approval = valid.review().digest.clone();
        let applied = valid.apply_eula(&approval).expect("positive_eula_apply");
        assert_eq!(bytes(&target(&valid_paths)), EULA_TRUE);
        applied
            .restore_before_spawn()
            .expect("positive_eula_restore");
        assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 4);
        assert_eq!(bytes(&target(&valid_paths)), EULA_FALSE);
    }
}

#[test]
fn independent_eula_edit_after_review_is_preserved_without_claim_or_rename() {
    let fixture = Fixture::new();
    let hooks = Hooks::default();
    let prepared = fixture.prepared(hooks.clone());
    let paths = prepared.test_paths();
    let approval = prepared.review().digest.clone();
    let independent = b"# independent fixture edit\neula=false\n";
    fs::write(target(&paths), independent).expect("independent_eula_edit");
    let before = files::tree(&paths.bundle);
    assert!(prepared.apply_eula(&approval).is_err());
    files::assert_tree(&paths.bundle, &before);
    assert_eq!(bytes(&target(&paths)), independent);
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
    assert!(!slot(&paths, "claim.json").exists());
    unlocked(&paths);
}

#[test]
fn same_bytes_replaced_after_approval_cannot_be_recaptured_as_reviewed_identity() {
    for relative in ["eula.txt", "server.properties"] {
        let fixture = Fixture::new();
        let selected = Arc::new(std::sync::Mutex::new(None::<PathBuf>));
        let intercepted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let retained = fixture.root.path().join("retained-reviewed-file.bin");
        let hook_selected = selected.clone();
        let hook_intercepted = intercepted.clone();
        let hook_retained = retained.clone();
        let hooks = Hooks {
            before_claim: Some(Arc::new(move || {
                let path = hook_selected
                    .lock()
                    .expect("fixture_selected_lock")
                    .clone()
                    .expect("fixture_selected_path");
                fs::rename(&path, &hook_retained)?;
                fs::write(&path, fs::read(&hook_retained)?)?;
                hook_intercepted.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })),
            ..Hooks::default()
        };
        let prepared = fixture.prepared(hooks.clone());
        let paths = prepared.test_paths();
        let selected_path = paths.copy.join(relative);
        let reviewed_bytes = bytes(&selected_path);
        *selected.lock().expect("fixture_set_selected") = Some(selected_path.clone());
        let approval = prepared.review().digest.clone();
        assert_eq!(prepared.apply_eula(&approval).err(), Some(Error::Changed));
        assert_eq!(intercepted.load(Ordering::SeqCst), 1);
        assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
        assert_eq!(bytes(&selected_path), reviewed_bytes);
        assert_eq!(bytes(&retained), reviewed_bytes);
        assert_eq!(bytes(&target(&paths)), EULA_FALSE);
        assert!(!slot(&paths, "claim.json").exists());
        assert_eq!(
            fs::read_dir(&paths.transaction)
                .expect("empty_transaction")
                .count(),
            0
        );
        unlocked(&paths);
    }
}
