use super::*;
use crate::engine::{acquire_instance_recovery_lock, InstanceId, InstanceLayout};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

const SVC: &[u8] = b"# Controller fixture\r\nport=24454\r\nvoice_host=old.example.invalid:24454\r\nmtu_size=1275\r\n";
const PLASMO: &[u8] = b"# Controller fixture\r\n[host]\r\nip = '127.0.0.1'\r\nport = 25565\r\n[host.public]\r\nip = 'old.example.invalid'\r\nport = 25565\r\n[voice]\r\nmtu_size = 1024\r\n";
const PLASMO_ABSENT: &[u8] = b"# Controller fixture\r\n[host]\r\nip = '127.0.0.1'\r\nport = 25565\r\n[voice]\r\nmtu_size = 1024\r\n";

struct Fixture {
    root: PathBuf,
    source: Instance,
    copy: Instance,
    voice_mod: VoiceMod,
    original: Vec<u8>,
}
impl Fixture {
    fn new(voice_mod: VoiceMod, original: &[u8]) -> Self {
        let artifacts = PathBuf::from(
            std::env::var_os("AURORA_CONNECT_ARTIFACT_DIR")
                .expect("explicit pinned local artifacts required; never downloaded by tests"),
        );
        let jar = artifacts.join(match voice_mod {
            VoiceMod::SimpleVoiceChat => "voicechat-fabric-1.21.1-2.6.22.jar",
            VoiceMod::PlasmoVoice => "plasmovoice-fabric-1.21.1-2.1.17.jar",
        });
        let (hash, size) = voice_mod.artifact();
        assert_eq!(recovery::hash_file(&jar).unwrap(), (hash.to_owned(), size));
        let root =
            std::env::temp_dir().join(format!("aurora-connect-controller-{}", Uuid::new_v4()));
        let layout = InstanceLayout::new(&root);
        let source = layout.open(InstanceId::parse("fixture-source").unwrap());
        source.ensure_layout().unwrap();
        let config = source.root().join(voice_mod.config());
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(config, original).unwrap();
        fs::write(
            source.root().join("aurora-instance.json"),
            br#"{"minecraft":"1.21.1","loader":"fabric","versionId":"1.21.1"}"#,
        )
        .unwrap();
        fs::copy(jar, source.root().join("mods/voice.jar")).unwrap();
        let guard = acquire_instance_recovery_lock(&source).unwrap();
        let copy = diagnostic_lab::create(&source, &guard, None, |_| {}).unwrap();
        drop(guard);
        Self {
            root,
            source,
            copy: layout.open(InstanceId::parse(copy.id).unwrap()),
            voice_mod,
            original: original.to_vec(),
        }
    }
    fn bytes(&self) -> Vec<u8> {
        fs::read(self.copy.root().join(self.voice_mod.config())).unwrap()
    }
    fn write(&self, value: &[u8]) {
        fs::write(self.copy.root().join(self.voice_mod.config()), value).unwrap();
    }
    fn apply(&self, guard: &VoiceMutationGuard) -> RestorationController {
        let prepared = prepare_announcement(
            &self.copy,
            guard,
            AnnouncementRequest {
                voice_mod: self.voice_mod,
                mode: AnnouncementMode::DedicatedAuroraOnly,
                guest_port: 35554,
                expected_source_sha256: digest(&self.bytes()),
            },
        )
        .unwrap();
        let approval = prepared.review().digest.clone();
        prepared
            .confirm(&self.copy, guard, &approval)
            .unwrap()
            .receipt
            .into_controller()
    }
    fn points(&self, guard: &VoiceMutationGuard) -> Vec<String> {
        recovery::list_points(&self.copy, guard.inner())
            .unwrap()
            .points
            .into_iter()
            .map(|p| p.id)
            .collect()
    }
    fn source_unchanged(&self) {
        assert_eq!(
            fs::read(self.source.root().join(self.voice_mod.config())).unwrap(),
            self.original
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let (Ok(resolved_root), Ok(resolved_temp)) = (
            fs::canonicalize(&self.root),
            fs::canonicalize(std::env::temp_dir()),
        ) else {
            return;
        };
        let Some(expected_name) = self.root.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        let valid_name = expected_name
            .strip_prefix("aurora-connect-controller-")
            .is_some_and(|suffix| Uuid::parse_str(suffix).is_ok());
        if !resolved_root.is_absolute()
            || resolved_root.parent() != Some(resolved_temp.as_path())
            || resolved_root.file_name() != self.root.file_name()
            || !valid_name
        {
            return;
        }
        // Delete only this resolved, directly-owned temporary directory.
        let _ = fs::remove_dir_all(resolved_root);
    }
}
fn guard(instance: &Instance) -> VoiceMutationGuard {
    process::acquire_with_probe(instance, Box::new(|| Ok(()))).unwrap()
}
fn controlled_guard(
    instance: &Instance,
) -> (VoiceMutationGuard, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let refuse_at = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let refuse = refuse_at.clone();
    let guard = process::acquire_with_probe(
        instance,
        Box::new(move || {
            let count = seen.fetch_add(1, Ordering::SeqCst) + 1;
            if count == refuse.load(Ordering::SeqCst) {
                Err(RecoveryError::VoiceProcessRunning)
            } else {
                Ok(())
            }
        }),
    )
    .unwrap();
    (guard, calls, refuse_at)
}
fn mods() -> [(VoiceMod, &'static [u8]); 2] {
    [
        (VoiceMod::SimpleVoiceChat, SVC),
        (VoiceMod::PlasmoVoice, PLASMO),
    ]
}

#[test]
fn controller_has_no_borrowed_guard_and_can_move_to_an_owned_worker() {
    fn assert_send<T: Send + 'static>() {}
    assert_send::<RestorationController>();
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn real_strict_restore_retains_exact_plan_across_guard_handoff_and_rejects_replay() {
    for (voice_mod, original) in mods() {
        let f = Fixture::new(voice_mod, original);
        let initial_guard = guard(&f.copy);
        let mut controller = f.apply(&initial_guard);
        let review = controller.prepare_strict(&f.copy, &initial_guard).unwrap();
        let stored = controller.pending.as_ref().unwrap();
        let deadline = stored.deadline;
        let fingerprint = stored.fingerprint.clone();
        let replacement = stored.replacement.clone();
        let mut display_only = review.clone();
        display_only.after = "untrusted presentation edit".into();
        display_only.digest = "not approval".into();
        assert_ne!(display_only.digest, review.digest);
        assert_eq!(
            controller.pending.as_ref().unwrap().review.digest,
            review.digest
        );
        drop(initial_guard);
        let launch_lock = acquire_instance_recovery_lock(&f.copy).unwrap();
        assert!(process::acquire_with_probe(&f.copy, Box::new(|| Ok(()))).is_err());
        drop(launch_lock);
        let new_guard = guard(&f.copy);
        assert_eq!(controller.pending.as_ref().unwrap().deadline, deadline);
        assert_eq!(
            controller.pending.as_ref().unwrap().fingerprint,
            fingerprint
        );
        assert_eq!(
            controller.pending.as_ref().unwrap().replacement,
            replacement
        );
        assert!(
            controller
                .confirm(&f.copy, &new_guard, &review.digest)
                .unwrap()
                .undo_available
        );
        assert_eq!(controller.state(), RestorationState::Restored);
        assert!(!controller.has_pending_review());
        assert_eq!(f.bytes(), original);
        let points = f.points(&new_guard);
        assert!(controller
            .confirm(&f.copy, &new_guard, &review.digest)
            .is_err());
        assert!(controller.prepare_strict(&f.copy, &new_guard).is_err());
        assert!(controller
            .prepare_reconciled(&f.copy, &new_guard, &digest(original))
            .is_err());
        assert_eq!(f.points(&new_guard), points);
        f.source_unchanged();
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn pending_cancel_wrong_digest_and_expiry_never_publish_a_restore_or_renew_approval() {
    for (voice_mod, original) in mods() {
        let f = Fixture::new(voice_mod, original);
        let (guard, calls, _) = controlled_guard(&f.copy);
        let mut controller = f.apply(&guard);
        let applied = f.bytes();
        let points = f.points(&guard);
        let first = controller.prepare_strict(&f.copy, &guard).unwrap();
        let deadline = controller.pending.as_ref().unwrap().deadline;
        let observed = calls.load(Ordering::SeqCst);
        assert!(controller.prepare_strict(&f.copy, &guard).is_err());
        assert!(controller
            .prepare_reconciled(&f.copy, &guard, &digest(&applied))
            .is_err());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            observed,
            "pending refusal precedes process/file I/O"
        );
        assert_eq!(controller.pending.as_ref().unwrap().deadline, deadline);
        assert_eq!(
            controller.pending.as_ref().unwrap().review.digest,
            first.digest
        );
        assert_eq!(f.points(&guard), points);
        assert!(controller.cancel_review());
        assert!(!controller.cancel_review());
        assert_eq!(controller.state(), RestorationState::Ready);
        assert!(controller.confirm(&f.copy, &guard, &first.digest).is_err());
        let second = controller.prepare_strict(&f.copy, &guard).unwrap();
        assert_ne!(second.digest, first.digest);
        assert!(controller.confirm(&f.copy, &guard, &first.digest).is_err());
        assert!(!controller.has_pending_review());
        assert_eq!(controller.state(), RestorationState::Ready);
        assert!(controller.confirm(&f.copy, &guard, &second.digest).is_err());
        let expired = controller.prepare_strict(&f.copy, &guard).unwrap();
        controller.pending.as_mut().unwrap().deadline = Instant::now() - Duration::from_millis(1);
        assert!(controller.has_pending_review());
        assert!(matches!(
            controller.confirm(&f.copy, &guard, &expired.digest),
            Err(RecoveryError::StalePreview)
        ));
        assert_eq!(controller.state(), RestorationState::Ready);
        assert!(!controller.has_pending_review());
        assert_eq!(f.bytes(), applied);
        assert_eq!(f.points(&guard), points);
        let dropped = controller.prepare_strict(&f.copy, &guard).unwrap();
        assert!(!dropped.digest.is_empty());
        drop(controller);
        assert_eq!(f.bytes(), applied);
        assert_eq!(f.points(&guard), points);
        f.source_unchanged();
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn later_context_or_target_edits_invalidate_the_stored_plan_without_overwriting_them() {
    for (voice_mod, original) in mods() {
        let f = Fixture::new(voice_mod, original);
        let guard = guard(&f.copy);
        let mut controller = f.apply(&guard);
        let applied = f.bytes();
        let options = f.copy.root().join("options.txt");
        fs::write(&options, b"renderDistance:10\n").unwrap();
        let first = controller.prepare_strict(&f.copy, &guard).unwrap();
        fs::write(&options, b"renderDistance:11\n").unwrap();
        assert!(controller.confirm(&f.copy, &guard, &first.digest).is_err());
        assert_eq!(controller.state(), RestorationState::Ready);
        assert_eq!(f.bytes(), applied);
        let next = controller.prepare_strict(&f.copy, &guard).unwrap();
        let edited = [applied.as_slice(), b"# later target edit\r\n"].concat();
        f.write(&edited);
        assert!(controller.confirm(&f.copy, &guard, &next.digest).is_err());
        assert_eq!(f.bytes(), edited);
        assert!(controller.prepare_strict(&f.copy, &guard).is_err());
        assert!(
            !controller.has_pending_review(),
            "strict failure never creates a reconciled plan"
        );
        let revision = controller
            .prepare_reconciled(&f.copy, &guard, &digest(&edited))
            .unwrap();
        controller
            .confirm(&f.copy, &guard, &revision.digest)
            .unwrap();
        assert_eq!(f.bytes(), [original, b"# later target edit\r\n"].concat());
        assert_eq!(fs::read(options).unwrap(), b"renderDistance:11\n");
        f.source_unchanged();
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn reconciliation_uses_a_new_review_and_preserves_non_owned_bytes_for_both_mods() {
    for (voice_mod, original) in [
        (VoiceMod::SimpleVoiceChat, SVC),
        (VoiceMod::PlasmoVoice, PLASMO),
        (VoiceMod::PlasmoVoice, PLASMO_ABSENT),
    ] {
        let f = Fixture::new(voice_mod, original);
        let guard = guard(&f.copy);
        let mut controller = f.apply(&guard);
        let applied = f.bytes();
        let strict = controller.prepare_strict(&f.copy, &guard).unwrap();
        controller.cancel_review();
        let edited = [applied.as_slice(), b"# writer comment preserved\r\n"].concat();
        f.write(&edited);
        let points = f.points(&guard);
        assert!(controller.prepare_strict(&f.copy, &guard).is_err());
        assert!(controller
            .prepare_reconciled(&f.copy, &guard, &digest(&applied))
            .is_err());
        assert!(!controller.has_pending_review());
        let review = controller
            .prepare_reconciled(&f.copy, &guard, &digest(&edited))
            .unwrap();
        assert!(matches!(
            review.action,
            AnnouncementAction::RestoreReconciled { .. }
        ));
        assert_ne!(strict.digest, review.digest);
        assert_eq!(f.points(&guard), points);
        assert!(controller.confirm(&f.copy, &guard, &strict.digest).is_err());
        assert_eq!(f.bytes(), edited);
        let review = controller
            .prepare_reconciled(&f.copy, &guard, &digest(&edited))
            .unwrap();
        controller.confirm(&f.copy, &guard, &review.digest).unwrap();
        assert_eq!(controller.state(), RestorationState::Restored);
        let restored = f.bytes();
        assert_eq!(digest(&restored), review.replacement_sha256);
        assert!(restored.ends_with(b"# writer comment preserved\r\n"));
        assert!(!String::from_utf8_lossy(&restored).contains("35554"));
        if original != PLASMO_ABSENT {
            assert_eq!(
                restored,
                [original, b"# writer comment preserved\r\n"].concat()
            );
        } else {
            assert!(!String::from_utf8_lossy(&restored).contains("[host.public]"));
        }
        f.source_unchanged();
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn wrong_instance_runtime_and_owned_value_never_inherit_receipt_authority() {
    for (voice_mod, original) in mods() {
        let f = Fixture::new(voice_mod, original);
        let g = guard(&f.copy);
        let other = Fixture::new(voice_mod, original);
        let other_guard = guard(&other.copy);
        let mut controller = f.apply(&g);
        let applied = f.bytes();
        assert!(controller
            .prepare_strict(&other.copy, &other_guard)
            .is_err());
        let plan = controller.prepare_strict(&f.copy, &g).unwrap();
        assert!(controller
            .confirm(&other.copy, &other_guard, &plan.digest)
            .is_err());
        assert_eq!(controller.state(), RestorationState::Ready);
        assert_eq!(other.bytes(), original);
        assert_eq!(f.bytes(), applied);
        let runtime = f.copy.root().join("aurora-instance.json");
        let original_runtime = fs::read(&runtime).unwrap();
        fs::write(
            &runtime,
            br#"{"minecraft":"1.21.1","loader":"forge","versionId":"1.21.1"}"#,
        )
        .unwrap();
        assert!(controller.prepare_strict(&f.copy, &g).is_err());
        fs::write(runtime, original_runtime).unwrap();
        let conflict = String::from_utf8(applied.clone())
            .unwrap()
            .replace("35554", "35555")
            .into_bytes();
        f.write(&conflict);
        assert!(controller
            .prepare_reconciled(&f.copy, &g, &digest(&conflict))
            .is_err());
        assert_eq!(f.bytes(), conflict);
        assert!(!controller.has_pending_review());
        f.source_unchanged();
        other.source_unchanged();
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; controlled process probes, no Java"]
fn process_refusals_preserve_ready_before_executor_and_require_recovery_after_it() {
    for (voice_mod, original) in mods() {
        for refusal in [1, 2, 3] {
            let f = Fixture::new(voice_mod, original);
            let (guard, calls, refuse_at) = controlled_guard(&f.copy);
            let mut controller = f.apply(&guard);
            let applied = f.bytes();
            calls.store(0, Ordering::SeqCst);
            refuse_at.store(1, Ordering::SeqCst);
            assert!(controller.prepare_strict(&f.copy, &guard).is_err());
            assert!(!controller.has_pending_review());
            refuse_at.store(0, Ordering::SeqCst);
            let review = controller.prepare_strict(&f.copy, &guard).unwrap();
            calls.store(0, Ordering::SeqCst);
            refuse_at.store(refusal, Ordering::SeqCst);
            assert!(matches!(
                controller.confirm(&f.copy, &guard, &review.digest),
                Err(RecoveryError::VoiceProcessRunning)
            ));
            assert_eq!(
                controller.state(),
                if refusal < 3 {
                    RestorationState::Ready
                } else {
                    RestorationState::NeedsRecovery
                }
            );
            assert!(!controller.has_pending_review());
            assert_eq!(f.bytes(), applied);
            refuse_at.store(0, Ordering::SeqCst);
            if refusal == 3 {
                assert!(controller.prepare_strict(&f.copy, &guard).is_err());
                assert!(controller
                    .prepare_reconciled(&f.copy, &guard, &digest(&applied))
                    .is_err());
                assert!(!controller.cancel_review());
                assert_eq!(controller.state(), RestorationState::NeedsRecovery);
            }
            f.source_unchanged();
        }
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; temporary staging fault, no Java"]
fn staging_failure_consumes_review_and_keeps_recovery_required_without_overwriting_config() {
    for (voice_mod, original) in mods() {
        let f = Fixture::new(voice_mod, original);
        let guard = guard(&f.copy);
        let mut controller = f.apply(&guard);
        let applied = f.bytes();
        let review = controller.prepare_strict(&f.copy, &guard).unwrap();
        let stage = f.copy.root().join(".aurora/connect-voice-stage");
        assert_eq!(fs::read_dir(&stage).unwrap().count(), 0);
        fs::remove_dir(&stage).unwrap();
        fs::write(&stage, b"owned fixture fault").unwrap();
        assert!(controller.confirm(&f.copy, &guard, &review.digest).is_err());
        assert_eq!(controller.state(), RestorationState::NeedsRecovery);
        assert!(!controller.has_pending_review());
        assert_eq!(f.bytes(), applied);
        assert!(controller.confirm(&f.copy, &guard, &review.digest).is_err());
        assert!(controller.prepare_strict(&f.copy, &guard).is_err());
        f.source_unchanged();
    }
}
