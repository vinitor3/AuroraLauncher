use super::*;
use crate::engine::acquire_instance_recovery_lock;
use probe::Refusal;

fn interrupted(fixture: &Fixture) -> (BundleId, TestPaths) {
    let prepared = fixture.prepared(fixture::hook_fault(Step::AppliedMarked));
    let paths = prepared.test_paths();
    let id = prepared.bundle_id();
    let approval = prepared.review().digest.clone();
    assert_eq!(prepared.apply_eula(&approval).err(), Some(Error::Injected));
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    unlocked(&paths);
    (id, paths)
}

#[test]
fn process_running_and_unavailable_before_or_after_lock_preserve_the_transaction() {
    for refusal in [Refusal::Running, Refusal::Unavailable] {
        for call in [1, 2] {
            let fixture = Fixture::new();
            let (id, paths) = interrupted(&fixture);
            let before = files::tree(&paths.bundle);
            let probe = probe::Probe::default();
            probe.refuse(call, refusal);
            let hooks = Hooks::default();
            let result = fixture.recover(id, &probe, hooks.clone());
            assert_eq!(
                result.err(),
                Some(match refusal {
                    Refusal::Running => Error::JavaRunning,
                    Refusal::Unavailable => Error::ProbeUnavailable,
                })
            );
            assert_eq!(probe.calls(), call);
            files::assert_tree(&paths.bundle, &before);
            assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
            unlocked(&paths);
        }
    }
}

#[test]
fn busy_copy_cannot_bypass_the_shared_lock_or_reach_the_second_probe() {
    let fixture = Fixture::new();
    let (id, paths) = interrupted(&fixture);
    let owner = acquire_instance_recovery_lock(&fixture::copy_instance(&paths))
        .expect("fixture_hold_shared_lock");
    let before = files::tree(&paths.bundle);
    let probe = probe::Probe::default();
    let hooks = Hooks::default();
    assert_eq!(
        fixture.recover(id, &probe, hooks.clone()).err(),
        Some(Error::Busy)
    );
    assert_eq!(probe.calls(), 1);
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
    files::assert_tree(&paths.bundle, &before);
    drop(owner);
    let plan = fixture
        .recover(id, &probe, hooks.clone())
        .expect("fixture_retry_after_unlock");
    let approval = plan.review().digest.clone();
    plan.confirm(&approval)
        .expect("fixture_recover_after_unlock");
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 2);
    unlocked(&paths);
}

#[test]
fn recovery_wrong_or_expired_approval_never_renames_and_releases_its_guard() {
    for expired in [false, true] {
        let fixture = Fixture::new();
        let (id, paths) = interrupted(&fixture);
        let probe = probe::Probe::default();
        let hooks = Hooks::default();
        let mut plan = fixture
            .recover(id, &probe, hooks.clone())
            .expect("recovery_plan");
        let before = files::tree(&paths.bundle);
        let approval = if expired {
            let digest = plan.review().digest.clone();
            plan.expire_for_test();
            digest
        } else {
            "0".repeat(64)
        };
        assert_eq!(
            plan.confirm(&approval).err(),
            Some(if expired {
                Error::Expired
            } else {
                Error::Digest
            })
        );
        files::assert_tree(&paths.bundle, &before);
        assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
        unlocked(&paths);
        let plan = fixture
            .recover(id, &probe, hooks.clone())
            .expect("fresh_recovery_review");
        let approval = plan.review().digest.clone();
        plan.confirm(&approval)
            .expect("fresh_recovery_confirmation");
        assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 2);
        assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    }
}

#[test]
fn every_guard_recheck_failure_stops_further_progress_and_is_recoverable() {
    let baseline = Fixture::new();
    let (id, paths) = interrupted(&baseline);
    let probe = probe::Probe::default();
    let plan = baseline
        .recover(id, &probe, Hooks::default())
        .expect("guard_baseline_plan");
    let prepare_calls = probe.calls();
    let approval = plan.review().digest.clone();
    plan.confirm(&approval).expect("guard_baseline_confirm");
    let all_calls = probe.calls();
    assert!(
        prepare_calls >= 3 && all_calls > prepare_calls,
        "guard_rechecks_required"
    );
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);

    for refused_call in 3..=all_calls {
        let fixture = Fixture::new();
        let (id, paths) = interrupted(&fixture);
        let probe = probe::Probe::default();
        probe.refuse(refused_call, Refusal::Running);
        let result = match fixture.recover(id, &probe, Hooks::default()) {
            Ok(plan) => {
                let approval = plan.review().digest.clone();
                plan.confirm(&approval)
            }
            Err(error) => Err(error),
        };
        assert_eq!(result.err(), Some(Error::JavaRunning));
        assert_eq!(
            probe.calls(),
            refused_call,
            "failed_probe_must_not_be_retried_as_absence"
        );
        unlocked(&paths);
        assert_eq!(bytes(&slot(&paths, "snapshot.bin")), EULA_FALSE);
        assert!(slot(&paths, "claim.json").is_file());
        let fresh = probe::Probe::default();
        let plan = fixture
            .recover(id, &fresh, Hooks::default())
            .expect("reopen_after_guard_failure");
        let approval = plan.review().digest.clone();
        plan.confirm(&approval)
            .expect("recover_after_guard_failure");
        assert_eq!(bytes(&target(&paths)), EULA_FALSE);
        unlocked(&paths);
    }
}

#[test]
fn target_drift_between_recovery_review_and_confirmation_is_preserved() {
    let fixture = Fixture::new();
    let (id, paths) = interrupted(&fixture);
    let probe = probe::Probe::default();
    let hooks = Hooks::default();
    let plan = fixture
        .recover(id, &probe, hooks.clone())
        .expect("recovery_before_drift");
    let approval = plan.review().digest.clone();
    let changed = b"# edited after recovery review\neula=true\n";
    fs::write(target(&paths), changed).expect("fixture_recovery_drift");
    let before = files::tree(&paths.bundle);
    assert!(plan.confirm(&approval).is_err());
    files::assert_tree(&paths.bundle, &before);
    assert_eq!(bytes(&target(&paths)), changed);
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
    unlocked(&paths);
}
