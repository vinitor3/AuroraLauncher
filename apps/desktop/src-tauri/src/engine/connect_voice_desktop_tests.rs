use super::*;
#[path = "connect_voice_desktop_durable_tests.rs"]
mod durable_tests;
#[path = "connect_voice_desktop_retirement_tests.rs"]
mod retirement_tests;
use crate::engine::{acquire_instance_recovery_lock, InstanceId, InstanceLayout};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Condvar,
    },
};

const SVC: &[u8] =
    b"# Desktop fixture\r\nport=24454\r\nvoice_host=old.example.invalid:24454\r\nmtu_size=1275\r\n";
const PLASMO: &[u8] = b"# Desktop fixture\r\n[host]\r\nip = '127.0.0.1'\r\nport = 25565\r\n[host.public]\r\nip = 'old.example.invalid'\r\nport = 25565\r\n[voice]\r\nmtu_size = 1024\r\n";

struct Fixture {
    root: PathBuf,
    source: Instance,
    copy: Instance,
    voice_mod: VoiceMod,
    original: Vec<u8>,
}
impl Fixture {
    fn new(voice_mod: VoiceMod, pinned: bool) -> Self {
        let original = match voice_mod {
            VoiceMod::SimpleVoiceChat => SVC,
            VoiceMod::PlasmoVoice => PLASMO,
        };
        let root =
            std::env::temp_dir().join(format!("aurora-connect-desktop-voice-{}", Uuid::new_v4()));
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
        if pinned {
            let artifacts = PathBuf::from(
                std::env::var_os("AURORA_CONNECT_ARTIFACT_DIR")
                    .expect("explicit pinned artifacts required; never downloaded"),
            );
            let jar = artifacts.join(match voice_mod {
                VoiceMod::SimpleVoiceChat => "voicechat-fabric-1.21.1-2.6.22.jar",
                VoiceMod::PlasmoVoice => "plasmovoice-fabric-1.21.1-2.1.17.jar",
            });
            let (hash, size) = voice_mod.artifact();
            assert_eq!(recovery::hash_file(&jar).unwrap(), (hash.to_owned(), size));
            fs::copy(jar, source.root().join("mods/voice.jar")).unwrap();
        } else {
            fs::write(
                source.root().join("mods/voice.jar"),
                b"unapproved test artifact",
            )
            .unwrap();
        }
        let guard = acquire_instance_recovery_lock(&source).unwrap();
        let environment = diagnostic_lab::create(&source, &guard, None, |_| {}).unwrap();
        drop(guard);
        Self {
            root,
            source,
            copy: layout.open(InstanceId::parse(environment.id).unwrap()),
            voice_mod,
            original: original.to_vec(),
        }
    }
    fn manager(&self) -> Manager {
        let manager = Manager::default();
        *manager.shared.test_hooks.probe.lock().unwrap() = Some(Arc::new(|| Ok(())));
        manager
    }
    fn bytes(&self) -> Vec<u8> {
        fs::read(self.copy.root().join(self.voice_mod.config())).unwrap()
    }
    fn write(&self, value: &[u8]) {
        fs::write(self.copy.root().join(self.voice_mod.config()), value).unwrap();
    }
    fn source_unchanged(&self) {
        assert_eq!(
            fs::read(self.source.root().join(self.voice_mod.config())).unwrap(),
            self.original
        );
    }
    fn unlocked(&self) {
        let guard = process::acquire_with_probe(&self.copy, Box::new(|| Ok(()))).unwrap();
        drop(guard);
    }
    fn inspect(&self, manager: &Manager) -> dto::VoiceAnnouncementSnapshot {
        tauri::async_runtime::block_on(manager.inspect(
            &self.root,
            dto::InspectRequest {
                instance_id: self.copy.id().as_str().into(),
                voice_mod: to_mod(self.voice_mod),
            },
        ))
        .unwrap()
    }
    fn prepared(&self, manager: &Manager) -> dto::VoiceAnnouncementSnapshot {
        let observed = self.inspect(manager);
        assert!(observed.phase == Phase::Observed);
        let prepared =
            tauri::async_runtime::block_on(manager.prepare_apply(dto::PrepareApplyRequest {
                operation_id: observed.operation_id.unwrap(),
                generation: observed.generation,
            }))
            .unwrap();
        assert!(prepared.phase == Phase::ReviewApply);
        self.unlocked();
        prepared
    }
    fn applied(&self, manager: &Manager) -> dto::VoiceAnnouncementSnapshot {
        let prepared = self.prepared(manager);
        let applied = confirm(manager, &prepared);
        assert!(applied.phase == Phase::Applied);
        assert!(applied.restoration_state == Some(dto::RestorationState::Ready));
        self.unlocked();
        applied
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let (Ok(root), Ok(temp)) = (
            fs::canonicalize(&self.root),
            fs::canonicalize(std::env::temp_dir()),
        ) else {
            return;
        };
        if root.parent() != Some(temp.as_path())
            || root.file_name() != self.root.file_name()
            || !root
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_prefix("aurora-connect-desktop-voice-"))
                .is_some_and(|s| Uuid::parse_str(s).is_ok())
        {
            return;
        }
        fs::remove_dir_all(root).unwrap();
    }
}
fn op(snapshot: &dto::VoiceAnnouncementSnapshot) -> dto::OperationRequest {
    dto::OperationRequest {
        operation_id: snapshot.operation_id.clone().unwrap(),
    }
}
fn approval(snapshot: &dto::VoiceAnnouncementSnapshot) -> dto::ConfirmRequest {
    let review = snapshot.review.as_ref().unwrap();
    dto::ConfirmRequest {
        operation_id: snapshot.operation_id.clone().unwrap(),
        plan_id: review.plan_id.clone(),
        digest: review.digest.clone(),
    }
}
fn confirm(
    manager: &Manager,
    snapshot: &dto::VoiceAnnouncementSnapshot,
) -> dto::VoiceAnnouncementSnapshot {
    tauri::async_runtime::block_on(manager.confirm(approval(snapshot))).unwrap()
}
fn restore(
    manager: &Manager,
    snapshot: &dto::VoiceAnnouncementSnapshot,
) -> dto::VoiceAnnouncementSnapshot {
    let reviewed = tauri::async_runtime::block_on(manager.prepare_restore(op(snapshot))).unwrap();
    assert!(reviewed.phase == Phase::ReviewRestore);
    confirm(manager, &reviewed)
}
fn spin(mut condition: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(20);
    while !condition() {
        assert!(Instant::now() < until, "bounded fixture wait");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[derive(Default)]
struct Gate {
    entered: AtomicUsize,
    released: Mutex<bool>,
    condition: Condvar,
}
impl Gate {
    fn enter(&self) {
        self.entered.fetch_add(1, Ordering::SeqCst);
        let released = self.released.lock().unwrap();
        let (_released, timeout) = self
            .condition
            .wait_timeout_while(released, Duration::from_secs(20), |ready| !*ready)
            .unwrap();
        assert!(!timeout.timed_out(), "bounded fixture gate");
    }
    fn wait(&self) {
        spin(|| self.entered.load(Ordering::SeqCst) != 0);
    }
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.condition.notify_all();
    }
}

#[test]
fn unavailable_artifact_and_ordinary_source_do_not_create_mutation_authority() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, false);
    let manager = f.manager();
    let observed = f.inspect(&manager);
    assert_eq!(
        tauri::async_runtime::block_on(manager.inspect(
            &f.root,
            dto::InspectRequest {
                instance_id: "another-copy".into(),
                voice_mod: dto::VoiceMod::SimpleVoiceChat
            }
        ))
        .err(),
        Some(Error::ContextActive)
    );
    assert!(observed.phase == Phase::Failed);
    assert_eq!(observed.last_error_code, Some(Error::Incompatible));
    assert!(observed.review.is_none() && observed.restoration_state.is_none());
    assert_eq!(f.bytes(), f.original);
    f.source_unchanged();
    f.unlocked();
    let close = tauri::async_runtime::block_on(manager.begin_close(dto::BeginCloseRequest {
        operation_id: observed.operation_id.clone(),
        intent: dto::CloseIntent::Panel,
    }))
    .unwrap();
    tauri::async_runtime::block_on(manager.finish_close(dto::CloseRequest {
        close_id: close.close.unwrap().close_id,
    }))
    .unwrap();
    let source = tauri::async_runtime::block_on(manager.inspect(
        &f.root,
        dto::InspectRequest {
            instance_id: f.source.id().as_str().into(),
            voice_mod: dto::VoiceMod::SimpleVoiceChat,
        },
    ))
    .unwrap();
    assert_eq!(source.last_error_code, Some(Error::UnsafePath));
    assert!(source.restoration_state.is_none());
}

#[test]
fn ordinary_source_is_rejected_before_process_probe_or_creation_of_an_instance_lock() {
    let root =
        std::env::temp_dir().join(format!("aurora-connect-desktop-voice-{}", Uuid::new_v4()));
    let source = InstanceLayout::new(&root).open(InstanceId::parse("ordinary-source").unwrap());
    source.ensure_layout().unwrap();
    let config = source.root().join(CONFIG);
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(config, SVC).unwrap();
    let f = Fixture {
        root,
        source: source.clone(),
        copy: source,
        voice_mod: VoiceMod::SimpleVoiceChat,
        original: SVC.to_vec(),
    };
    assert!(!f.source.root().join(".aurora").exists());
    let manager = Manager::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let probe_calls = calls.clone();
    *manager.shared.test_hooks.probe.lock().unwrap() = Some(Arc::new(move || {
        probe_calls.fetch_add(1, Ordering::SeqCst);
        Err(RecoveryError::VoiceProcessUnavailable)
    }));
    let result = f.inspect(&manager);
    assert!(
        result.phase == Phase::Failed
            && result.review.is_none()
            && result.restoration_state.is_none()
    );
    assert_eq!(result.last_error_code, Some(Error::UnsafePath));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!f.source.root().join(".aurora").exists());
    assert!(!f.root.join(".aurora").exists());
    f.source_unchanged();
}

#[test]
fn empty_exit_is_native_only_sealed_and_has_no_configuration_authority() {
    let manager = Manager::default();
    assert_eq!(
        tauri::async_runtime::block_on(manager.begin_close(dto::BeginCloseRequest {
            operation_id: None,
            intent: dto::CloseIntent::Exit
        }))
        .err(),
        Some(Error::InvalidRequest)
    );
    let closed = tauri::async_runtime::block_on(manager.begin_exit()).unwrap();
    assert!(manager.exit_ready());
    let id = closed.close.unwrap().close_id;
    assert_eq!(
        tauri::async_runtime::block_on(manager.cancel_close(dto::CloseRequest {
            close_id: id.clone()
        }))
        .err(),
        Some(Error::StaleClose)
    );
    assert_eq!(
        tauri::async_runtime::block_on(manager.finish_close(dto::CloseRequest { close_id: id }))
            .err(),
        Some(Error::StaleClose)
    );
    assert!(manager.exit_ready());
}

#[test]
fn abandoning_unstarted_or_running_inspection_does_not_leave_reservation_or_lose_cleanup() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, false);
    let manager = f.manager();
    let future = manager.inspect(
        &f.root,
        dto::InspectRequest {
            instance_id: f.copy.id().as_str().into(),
            voice_mod: dto::VoiceMod::SimpleVoiceChat,
        },
    );
    drop(future);
    assert!(manager.snapshot().unwrap().phase == Phase::Idle);
    let gate = Arc::new(Gate::default());
    let hook = gate.clone();
    *manager.shared.test_hooks.stage.lock().unwrap() = Some(Arc::new(move |stage| {
        if stage == TestStage::Inspect {
            hook.enter();
        }
    }));
    let own = manager.clone();
    let data = f.root.clone();
    let id = f.copy.id().as_str().to_owned();
    let task = tauri::async_runtime::spawn(async move {
        own.inspect(
            &data,
            dto::InspectRequest {
                instance_id: id,
                voice_mod: dto::VoiceMod::SimpleVoiceChat,
            },
        )
        .await
    });
    gate.wait();
    assert!(manager.snapshot().unwrap().busy);
    task.abort();
    gate.release();
    spin(|| !manager.snapshot().unwrap().busy);
    assert!(manager.snapshot().unwrap().phase == Phase::Failed);
    f.unlocked();
    f.source_unchanged();
    assert_eq!(f.bytes(), f.original);
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn both_mods_complete_inspection_apply_and_strict_restore_with_owned_original_reviews() {
    for voice_mod in [VoiceMod::SimpleVoiceChat, VoiceMod::PlasmoVoice] {
        let f = Fixture::new(voice_mod, true);
        let manager = f.manager();
        let applied = f.applied(&manager);
        assert_ne!(f.bytes(), f.original);
        f.source_unchanged();
        assert!(
            applied.last_recovery_result.is_some()
                && applied.review.is_none()
                && applied.observation.is_none()
        );
        let restored = restore(&manager, &applied);
        assert!(restored.phase == Phase::Restored);
        assert!(restored.restoration_state == Some(dto::RestorationState::Restored));
        assert_eq!(f.bytes(), f.original);
        f.source_unchanged();
        f.unlocked();
        let json = serde_json::to_value(restored).unwrap();
        assert!(!json.to_string().contains("receipt_id"));
        assert!(!json.to_string().contains("fingerprint"));
        let guard = process::acquire_with_probe(&f.copy, Box::new(|| Ok(()))).unwrap();
        assert!(!recovery::list_points(&f.copy, guard.inner())
            .unwrap()
            .points
            .is_empty());
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn approvals_are_single_use_and_expiry_drift_generation_and_context_fail_closed() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, true);
    let manager = f.manager();
    let observed = f.inspect(&manager);
    assert_eq!(
        tauri::async_runtime::block_on(manager.prepare_apply(dto::PrepareApplyRequest {
            operation_id: observed.operation_id.clone().unwrap(),
            generation: observed.generation + 1
        }))
        .err(),
        Some(Error::StaleReview)
    );
    let reviewed =
        tauri::async_runtime::block_on(manager.prepare_apply(dto::PrepareApplyRequest {
            operation_id: observed.operation_id.unwrap(),
            generation: observed.generation,
        }))
        .unwrap();
    let mut wrong = approval(&reviewed);
    wrong.digest = "0".repeat(64);
    let failed = tauri::async_runtime::block_on(manager.confirm(wrong)).unwrap();
    assert_eq!(failed.last_error_code, Some(Error::StaleReview));
    assert!(failed.phase == Phase::Failed);
    assert_eq!(
        tauri::async_runtime::block_on(manager.confirm(approval(&reviewed))).err(),
        Some(Error::StaleReview)
    );
    assert_eq!(f.bytes(), f.original);
    let reviewed = f.prepared(&manager);
    {
        let mut core = manager.shared.authority.try_lock().unwrap();
        core.operation
            .as_mut()
            .unwrap()
            .pending_apply
            .as_mut()
            .unwrap()
            .deadline = Instant::now() - Duration::from_millis(1);
    }
    let failed = confirm(&manager, &reviewed);
    assert_eq!(failed.last_error_code, Some(Error::StaleReview));
    assert_eq!(f.bytes(), f.original);
    let reviewed = f.prepared(&manager);
    let mut edited = f.original.clone();
    edited.extend_from_slice(b"# other change\n");
    f.write(&edited);
    let failed = confirm(&manager, &reviewed);
    assert_eq!(failed.last_error_code, Some(Error::StaleReview));
    assert_eq!(f.bytes(), edited);
    f.write(&f.original);
    let reviewed = f.prepared(&manager);
    let mut wrong_instance = approval(&reviewed);
    wrong_instance.operation_id = Uuid::new_v4().to_string();
    assert_eq!(
        tauri::async_runtime::block_on(manager.confirm(wrong_instance)).err(),
        Some(Error::ContextChanged)
    );
    assert_eq!(f.bytes(), f.original);
    let changed_runtime = f.copy.root().join("aurora-instance.json");
    fs::write(
        &changed_runtime,
        br#"{"minecraft":"1.20.1","loader":"fabric","versionId":"1.20.1"}"#,
    )
    .unwrap();
    let failed = confirm(&manager, &reviewed);
    assert!(failed.last_error_code.is_some());
    assert_eq!(f.bytes(), f.original);
    fs::write(
        &changed_runtime,
        br#"{"minecraft":"1.21.1","loader":"fabric","versionId":"1.21.1"}"#,
    )
    .unwrap();
    let reviewed = f.prepared(&manager);
    let identity = f.copy.root().join(".aurora/recovery-identity.json");
    let mut marker: serde_json::Value =
        serde_json::from_slice(&fs::read(&identity).unwrap()).unwrap();
    assert!(marker.get("uuid").is_some());
    marker["uuid"] = serde_json::Value::String(Uuid::new_v4().to_string());
    fs::write(identity, serde_json::to_vec(&marker).unwrap()).unwrap();
    let failed = confirm(&manager, &reviewed);
    assert_eq!(failed.last_error_code, Some(Error::ContextChanged));
    assert_eq!(f.bytes(), f.original);
    f.source_unchanged();
    f.unlocked();
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn reconciled_restore_is_explicit_preserves_other_edits_and_does_not_require_installed_jar() {
    for voice_mod in [VoiceMod::SimpleVoiceChat, VoiceMod::PlasmoVoice] {
        let f = Fixture::new(voice_mod, true);
        let manager = f.manager();
        let applied = f.applied(&manager);
        fs::remove_file(f.copy.root().join("mods/voice.jar")).unwrap();
        let mut edited = f.bytes();
        edited.extend_from_slice(b"# writer changed only this comment\n");
        f.write(&edited);
        let strict = tauri::async_runtime::block_on(manager.prepare_restore(op(&applied))).unwrap();
        assert_eq!(strict.last_error_code, Some(Error::StaleReview));
        assert!(strict.phase == Phase::Applied && strict.review.is_none());
        let own_field_changed = String::from_utf8(edited.clone())
            .unwrap()
            .replace("35554", "35555")
            .into_bytes();
        assert_ne!(own_field_changed, edited);
        f.write(&own_field_changed);
        let refused =
            tauri::async_runtime::block_on(manager.prepare_restore_reconciled(op(&applied)))
                .unwrap();
        assert!(
            refused.phase == Phase::Applied
                && refused.review.is_none()
                && refused.last_error_code.is_some()
        );
        assert_eq!(f.bytes(), own_field_changed);
        f.write(&edited);
        assert_eq!(
            tauri::async_runtime::block_on(manager.prepare_restore_reconciled(
                dto::OperationRequest {
                    operation_id: Uuid::new_v4().to_string()
                }
            ))
            .err(),
            Some(Error::ContextChanged)
        );
        let reviewed =
            tauri::async_runtime::block_on(manager.prepare_restore_reconciled(op(&applied)))
                .unwrap();
        assert!(reviewed.review.as_ref().unwrap().action == dto::ReviewAction::RestoreReconciled);
        let mut wrong = approval(&reviewed);
        wrong.digest = "0".repeat(64);
        let refused = tauri::async_runtime::block_on(manager.confirm(wrong)).unwrap();
        assert!(refused.restoration_state == Some(dto::RestorationState::Ready));
        assert_eq!(f.bytes(), edited);
        let next = tauri::async_runtime::block_on(manager.prepare_restore_reconciled(op(&applied)))
            .unwrap();
        assert_ne!(
            next.review.as_ref().unwrap().digest,
            reviewed.review.as_ref().unwrap().digest
        );
        let restored = confirm(&manager, &next);
        assert!(restored.phase == Phase::Restored);
        let mut expected = f.original.clone();
        expected.extend_from_slice(b"# writer changed only this comment\n");
        assert_eq!(f.bytes(), expected);
        f.source_unchanged();
        f.unlocked();
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn restore_failure_after_executor_retains_recovery_state_and_approved_logout_finishes_to_idle() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, true);
    let manager = f.manager();
    let applied = f.applied(&manager);
    let reviewed = tauri::async_runtime::block_on(manager.prepare_restore(op(&applied))).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let probe_calls = calls.clone();
    *manager.shared.test_hooks.probe.lock().unwrap() = Some(Arc::new(move || {
        // Two acquisition checks, two pre-executor checks, then the existing
        // final check after CAS. A late observed process blocks the transaction.
        if probe_calls.fetch_add(1, Ordering::SeqCst) + 1 == 5 {
            Err(RecoveryError::VoiceProcessRunning)
        } else {
            Ok(())
        }
    }));
    let failed = confirm(&manager, &reviewed);
    assert_eq!(calls.load(Ordering::SeqCst), 5);
    assert!(
        failed.phase == Phase::NeedsRecovery
            && failed.restoration_state == Some(dto::RestorationState::NeedsRecovery)
    );
    let changed = f.bytes();
    assert_ne!(changed, f.original);
    let closed = tauri::async_runtime::block_on(manager.begin_close(dto::BeginCloseRequest {
        operation_id: failed.operation_id.clone(),
        intent: dto::CloseIntent::Logout,
    }))
    .unwrap()
    .close
    .unwrap();
    let snapshot =
        tauri::async_runtime::block_on(manager.confirm_close(dto::ConfirmCloseRequest {
            close_id: closed.close_id.clone(),
            digest: closed.review.unwrap().digest,
        }))
        .unwrap();
    assert!(snapshot.phase == Phase::Closed && snapshot.restoration_state.is_none());
    assert_eq!(
        tauri::async_runtime::block_on(manager.cancel_close(dto::CloseRequest {
            close_id: closed.close_id.clone()
        }))
        .err(),
        Some(Error::StaleClose)
    );
    let idle = tauri::async_runtime::block_on(manager.finish_close(dto::CloseRequest {
        close_id: closed.close_id,
    }))
    .unwrap();
    assert!(
        idle.phase == Phase::Idle
            && idle.operation_id.is_none()
            && idle.instance_id.is_none()
            && idle.voice_mod.is_none()
            && idle.last_recovery_result.is_none()
    );
    assert_eq!(f.bytes(), changed);
    f.unlocked();
    f.source_unchanged();
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn pre_executor_error_consumes_review_while_post_executor_error_requires_recovery() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, true);
    let manager = f.manager();
    let reviewed = f.prepared(&manager);
    *manager.shared.test_hooks.probe.lock().unwrap() =
        Some(Arc::new(|| Err(RecoveryError::VoiceProcessRunning)));
    let failed = confirm(&manager, &reviewed);
    assert!(failed.phase == Phase::Failed && failed.restoration_state.is_none());
    assert_eq!(failed.last_error_code, Some(Error::JavaRunning));
    assert_eq!(f.bytes(), f.original);
    *manager.shared.test_hooks.probe.lock().unwrap() = Some(Arc::new(|| Ok(())));
    let reviewed = f.prepared(&manager);
    let blocked_stage = f.copy.root().join(".aurora/connect-voice-stage");
    *manager.shared.test_hooks.stage.lock().unwrap() = Some(Arc::new(move |stage| {
        if stage == TestStage::ExecutorBegin {
            fs::write(&blocked_stage, b"owned fixture obstacle").unwrap();
        }
    }));
    let failed = confirm(&manager, &reviewed);
    assert!(failed.phase == Phase::NeedsRecovery);
    assert!(failed.restoration_state == Some(dto::RestorationState::NeedsRecovery));
    assert!(manager
        .shared
        .authority
        .try_lock()
        .unwrap()
        .operation
        .as_ref()
        .unwrap()
        .controller
        .is_none());
    assert_eq!(
        tauri::async_runtime::block_on(manager.prepare_restore(op(&failed))).err(),
        Some(Error::StaleReview)
    );
    assert_eq!(f.bytes(), f.original);
    f.unlocked();
    f.source_unchanged();
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn abandoned_ipc_and_cancelled_close_wait_for_real_writer_and_retain_receipt() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, true);
    let manager = f.manager();
    let reviewed = f.prepared(&manager);
    let gate = Arc::new(Gate::default());
    let hook = gate.clone();
    let closes = Arc::new(AtomicUsize::new(0));
    let close_count = closes.clone();
    let cancels = Arc::new(AtomicUsize::new(0));
    let cancel_count = cancels.clone();
    *manager.shared.test_hooks.stage.lock().unwrap() = Some(Arc::new(move |stage| {
        if stage == TestStage::OutcomeRetained {
            hook.enter();
        }
        if stage == TestStage::CloseTask {
            close_count.fetch_add(1, Ordering::SeqCst);
        }
        if stage == TestStage::CancelCloseTask {
            cancel_count.fetch_add(1, Ordering::SeqCst);
        }
    }));
    let request = approval(&reviewed);
    let own = manager.clone();
    let task = tauri::async_runtime::spawn(async move { own.confirm(request).await });
    gate.wait();
    assert_ne!(f.bytes(), f.original);
    task.abort();
    let own = manager.clone();
    let id = reviewed.operation_id.clone();
    let close_task = tauri::async_runtime::spawn(async move {
        own.begin_close(dto::BeginCloseRequest {
            operation_id: id,
            intent: dto::CloseIntent::Logout,
        })
        .await
    });
    spin(|| closes.load(Ordering::SeqCst) == 1);
    let waiting = manager.snapshot().unwrap();
    assert!(waiting.phase == Phase::Closing && waiting.review.is_none());
    assert!(waiting.busy);
    assert_eq!(
        tauri::async_runtime::block_on(manager.prepare_restore(op(&reviewed))).err(),
        Some(Error::Busy)
    );
    let close_id = waiting.close.unwrap().close_id;
    let own = manager.clone();
    let cancel = tauri::async_runtime::spawn(async move {
        own.cancel_close(dto::CloseRequest { close_id }).await
    });
    spin(|| cancels.load(Ordering::SeqCst) == 1);
    assert!(manager.snapshot().unwrap().close.is_some());
    cancel.abort();
    close_task.abort();
    gate.release();
    spin(|| manager.snapshot().unwrap().close.is_none() && !manager.snapshot().unwrap().busy);
    let applied = manager.snapshot().unwrap();
    assert!(
        applied.phase == Phase::Applied
            && applied.restoration_state == Some(dto::RestorationState::Ready)
    );
    f.unlocked();
    *manager.shared.test_hooks.stage.lock().unwrap() = None;
    assert!(restore(&manager, &applied).phase == Phase::Restored);
    assert_eq!(f.bytes(), f.original);
    f.source_unchanged();
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn close_reviews_bind_intent_consume_once_keep_receipt_on_error_and_seal_approved_exit() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, true);
    let manager = f.manager();
    let applied = f.applied(&manager);
    let bytes = f.bytes();
    for failure in 0..3 {
        let close = tauri::async_runtime::block_on(manager.begin_close(dto::BeginCloseRequest {
            operation_id: applied.operation_id.clone(),
            intent: dto::CloseIntent::Panel,
        }))
        .unwrap();
        let envelope = close.close.unwrap();
        let review = envelope.review.unwrap();
        let repeat = tauri::async_runtime::block_on(manager.begin_close(dto::BeginCloseRequest {
            operation_id: applied.operation_id.clone(),
            intent: dto::CloseIntent::Panel,
        }))
        .unwrap();
        assert_eq!(repeat.close.unwrap().review.unwrap().digest, review.digest);
        assert_eq!(
            tauri::async_runtime::block_on(manager.begin_close(dto::BeginCloseRequest {
                operation_id: applied.operation_id.clone(),
                intent: dto::CloseIntent::Logout
            }))
            .err(),
            Some(Error::ClosePending)
        );
        let mut digest = review.digest;
        match failure {
            0 => digest = "0".repeat(64),
            1 => manager
                .shared
                .test_hooks
                .civil
                .store(review.expires_at_ms, Ordering::SeqCst),
            _ => {
                manager.shared.view().unwrap().close_deadline =
                    Some(Instant::now() - Duration::from_millis(1))
            }
        }
        let stale =
            tauri::async_runtime::block_on(manager.confirm_close(dto::ConfirmCloseRequest {
                close_id: envelope.close_id.clone(),
                digest,
            }))
            .unwrap();
        assert_eq!(stale.last_error_code, Some(Error::StaleClose));
        assert!(stale.close.unwrap().review.is_none());
        assert!(stale.restoration_state == Some(dto::RestorationState::Ready));
        assert_eq!(f.bytes(), bytes);
        manager.shared.test_hooks.civil.store(0, Ordering::SeqCst);
        let reopened = tauri::async_runtime::block_on(manager.cancel_close(dto::CloseRequest {
            close_id: envelope.close_id,
        }))
        .unwrap();
        assert!(reopened.phase == Phase::Applied);
    }
    manager
        .shared
        .test_hooks
        .civil
        .store(MAX_GENERATION, Ordering::SeqCst);
    let invalid_clock =
        tauri::async_runtime::block_on(manager.begin_close(dto::BeginCloseRequest {
            operation_id: applied.operation_id.clone(),
            intent: dto::CloseIntent::Panel,
        }))
        .unwrap();
    assert!(invalid_clock.phase == Phase::CloseReview && !invalid_clock.busy);
    assert_eq!(invalid_clock.last_error_code, Some(Error::StaleClose));
    let close = invalid_clock.close.unwrap();
    assert!(close.state == dto::CloseState::ReviewRequired && close.review.is_none());
    manager.shared.test_hooks.civil.store(0, Ordering::SeqCst);
    tauri::async_runtime::block_on(manager.cancel_close(dto::CloseRequest {
        close_id: close.close_id,
    }))
    .unwrap();
    let exit = tauri::async_runtime::block_on(manager.begin_exit())
        .unwrap()
        .close
        .unwrap();
    let closed = tauri::async_runtime::block_on(manager.confirm_close(dto::ConfirmCloseRequest {
        close_id: exit.close_id.clone(),
        digest: exit.review.unwrap().digest,
    }))
    .unwrap();
    assert!(closed.phase == Phase::Closed && closed.restoration_state.is_none());
    assert!(manager.exit_ready());
    assert_eq!(
        tauri::async_runtime::block_on(manager.cancel_close(dto::CloseRequest {
            close_id: exit.close_id.clone()
        }))
        .err(),
        Some(Error::StaleClose)
    );
    assert_eq!(
        tauri::async_runtime::block_on(manager.finish_close(dto::CloseRequest {
            close_id: exit.close_id
        }))
        .err(),
        Some(Error::StaleClose)
    );
    assert_eq!(f.bytes(), bytes);
    f.source_unchanged();
    f.unlocked();
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn review_completed_during_close_is_never_published_and_preparation_cancels_without_write() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, true);
    let manager = f.manager();
    let observed = f.inspect(&manager);
    let prepare_gate = Arc::new(Gate::default());
    let prepared_hook = prepare_gate.clone();
    let completion_gate = Arc::new(Gate::default());
    let complete_hook = completion_gate.clone();
    *manager.shared.test_hooks.stage.lock().unwrap() = Some(Arc::new(move |stage| {
        if stage == TestStage::Prepare {
            prepared_hook.enter();
        }
        if stage == TestStage::CompletionPublished {
            complete_hook.enter();
        }
    }));
    let own = manager.clone();
    let operation_id = observed.operation_id.clone().unwrap();
    let prepared = tauri::async_runtime::spawn(async move {
        own.prepare_apply(dto::PrepareApplyRequest {
            operation_id,
            generation: observed.generation,
        })
        .await
    });
    prepare_gate.wait();
    let operation_id = manager.snapshot().unwrap().operation_id;
    let own = manager.clone();
    let closing = tauri::async_runtime::spawn(async move {
        own.begin_close(dto::BeginCloseRequest {
            operation_id,
            intent: dto::CloseIntent::Panel,
        })
        .await
    });
    spin(|| manager.snapshot().unwrap().close.is_some());
    prepare_gate.release();
    completion_gate.wait();
    let held = manager.snapshot().unwrap();
    assert!(held.phase == Phase::Closing && held.busy && held.review.is_none());
    completion_gate.release();
    tauri::async_runtime::block_on(prepared).unwrap().unwrap();
    let closed = tauri::async_runtime::block_on(closing).unwrap().unwrap();
    assert!(closed.phase == Phase::Closed && closed.review.is_none());
    let close_id = closed.close.unwrap().close_id;
    assert_eq!(
        tauri::async_runtime::block_on(manager.cancel_close(dto::CloseRequest {
            close_id: close_id.clone()
        }))
        .err(),
        Some(Error::StaleClose)
    );
    let idle = tauri::async_runtime::block_on(manager.finish_close(dto::CloseRequest { close_id }))
        .unwrap();
    assert!(idle.phase == Phase::Idle && idle.operation_id.is_none());
    assert_eq!(f.bytes(), f.original);
    f.source_unchanged();
    f.unlocked();
}
