//! Desktop authority over real disposable Recovery storage. Opt-ins require
//! previously pinned public JARs; no Java, download or personal copy is used.
use super::*;

impl Fixture {
    fn durable_prepared(&self, manager: &Manager) -> dto::VoiceAnnouncementSnapshot {
        let observed = self.inspect(manager);
        assert!(observed.phase == Phase::Observed);
        let prepared = tauri::async_runtime::block_on(manager.prepare_apply_durable(
            dto::PrepareApplyRequest {
                operation_id: observed.operation_id.unwrap(),
                generation: observed.generation,
            },
        ))
        .unwrap();
        assert!(prepared.phase == Phase::ReviewApply);
        let receipt = prepared.durable_receipt.as_ref().unwrap();
        assert_eq!(receipt.state, dto::DurableReceiptState::Planned);
        assert!(receipt.receipt_id.is_none() && receipt.generation.is_none());
        assert_eq!(self.bytes(), self.original);
        self.unlocked();
        prepared
    }
    fn durable_applied(&self, manager: &Manager) -> dto::VoiceAnnouncementSnapshot {
        let applied = confirm(manager, &self.durable_prepared(manager));
        assert!(applied.phase == Phase::Applied);
        let receipt = applied.durable_receipt.as_ref().unwrap();
        assert_eq!(receipt.state, dto::DurableReceiptState::Saved);
        assert_eq!(receipt.generation, Some(0));
        assert!(uuid(receipt.receipt_id.as_ref().unwrap()));
        self.unlocked();
        applied
    }
    fn list(&self, manager: &Manager) -> Outcome<dto::ReceiptList> {
        tauri::async_runtime::block_on(manager.list_receipts(
            &self.root,
            dto::ListReceiptsRequest {
                instance_id: self.copy.id().as_str().into(),
            },
        ))
    }
    fn resume(
        &self,
        manager: &Manager,
        id: &str,
        generation: u64,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        tauri::async_runtime::block_on(manager.resume_receipt(
            &self.root,
            dto::ResumeReceiptRequest {
                instance_id: self.copy.id().as_str().into(),
                receipt_id: id.into(),
                expected_generation: generation,
            },
        ))
    }
}

fn close_saved(manager: &Manager, snapshot: &dto::VoiceAnnouncementSnapshot) {
    let closing = tauri::async_runtime::block_on(manager.begin_close(dto::BeginCloseRequest {
        operation_id: snapshot.operation_id.clone(),
        intent: dto::CloseIntent::Panel,
    }))
    .unwrap();
    let close = closing.close.unwrap();
    let review = close.review.unwrap();
    assert!(!review.selective_restoration_lost);
    assert_eq!(
        review.durable_receipt.as_ref().unwrap().state,
        dto::DurableReceiptState::Saved
    );
    let closed = tauri::async_runtime::block_on(manager.confirm_close(dto::ConfirmCloseRequest {
        close_id: close.close_id.clone(),
        digest: review.digest,
    }))
    .unwrap();
    assert!(closed.phase == Phase::Closed && closed.restoration_state.is_none());
    assert_eq!(
        closed.durable_receipt.unwrap().state,
        dto::DurableReceiptState::Saved
    );
    let idle = tauri::async_runtime::block_on(manager.finish_close(dto::CloseRequest {
        close_id: close.close_id,
    }))
    .unwrap();
    assert!(idle.phase == Phase::Idle && idle.durable_receipt.is_none());
}

#[test]
fn durable_desktop_list_is_read_only_and_invalid_resume_has_no_authority() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, false);
    let manager = f.manager();
    let index = f.root.join(".aurora/recovery/index.sqlite3");
    let before = fs::read(&index).unwrap();
    let list = f.list(&manager).unwrap();
    assert_eq!(list.instance_id, f.copy.id().as_str());
    assert!(list.receipts.is_empty());
    assert_eq!(fs::read(&index).unwrap(), before);
    assert!(manager.snapshot().unwrap().phase == Phase::Idle);
    let unknown = f.resume(&manager, &Uuid::new_v4().to_string(), 0).unwrap();
    assert!(unknown.phase == Phase::Idle && unknown.operation_id.is_none());
    assert!(unknown.last_error_code.is_some() && unknown.durable_receipt.is_none());
    for id in ["../outside", "{00000000-0000-0000-0000-000000000000}"] {
        assert_eq!(f.resume(&manager, id, 0).err(), Some(Error::InvalidRequest));
    }
    assert_eq!(
        f.resume(&manager, &Uuid::new_v4().to_string(), 1).err(),
        Some(Error::InvalidRequest)
    );
    assert_eq!(
        tauri::async_runtime::block_on(manager.list_receipts(
            &f.root,
            dto::ListReceiptsRequest {
                instance_id: f.source.id().as_str().into(),
            }
        ))
        .err(),
        Some(Error::UnsafePath)
    );
    let observed = f.inspect(&manager);
    assert!(observed.phase == Phase::Failed);
    assert_eq!(f.list(&manager).err(), Some(Error::ContextActive));
    assert_eq!(
        f.resume(&manager, &Uuid::new_v4().to_string(), 0).err(),
        Some(Error::ContextActive)
    );
    assert_eq!(fs::read(index).unwrap(), before);
    assert_eq!(f.bytes(), f.original);
    f.source_unchanged();
    f.unlocked();
}

#[test]
fn durable_desktop_abandoned_list_and_resume_hold_the_exit_barrier() {
    for resume in [false, true] {
        let f = Fixture::new(VoiceMod::SimpleVoiceChat, false);
        let manager = f.manager();
        let gate = Arc::new(Gate::default());
        let hook = gate.clone();
        *manager.shared.test_hooks.stage.lock().unwrap() = Some(Arc::new(move |stage| {
            if stage
                == if resume {
                    TestStage::ResumeReceipt
                } else {
                    TestStage::ListReceipts
                }
            {
                hook.enter();
            }
        }));
        let own = manager.clone();
        let root = f.root.clone();
        let id = f.copy.id().as_str().to_owned();
        let task = tauri::async_runtime::spawn(async move {
            if resume {
                own.resume_receipt(
                    &root,
                    dto::ResumeReceiptRequest {
                        instance_id: id,
                        receipt_id: Uuid::new_v4().to_string(),
                        expected_generation: 0,
                    },
                )
                .await
                .map(|_| ())
            } else {
                own.list_receipts(&root, dto::ListReceiptsRequest { instance_id: id })
                    .await
                    .map(|_| ())
            }
        });
        gate.wait();
        let before = manager.snapshot().unwrap();
        if resume {
            assert!(before.phase == Phase::ResumingReceipt && before.busy);
        }
        assert!(
            before.operation_id.is_none()
                && before.instance_id.is_none()
                && before.voice_mod.is_none()
        );
        task.abort();
        assert_eq!(f.list(&manager).err(), Some(Error::Busy));
        let own = manager.clone();
        let exit = tauri::async_runtime::spawn(async move { own.begin_exit().await });
        spin(|| manager.snapshot().unwrap().phase == Phase::Closing);
        assert!(!manager.exit_ready());
        exit.abort();
        gate.release();
        spin(|| manager.exit_ready());
        // Exit publishes Ready while still holding authority; wait for the
        // quiesce task to release it instead of racing those final instructions.
        spin(|| manager.shared.authority.try_lock().is_ok());
        assert!(manager
            .shared
            .authority
            .try_lock()
            .unwrap()
            .operation
            .is_none());
        assert!(manager.snapshot().unwrap().durable_receipt.is_none());
        assert_eq!(f.bytes(), f.original);
        f.unlocked();
        f.source_unchanged();
    }
}

#[test]
fn durable_desktop_pending_journal_is_reported_without_recovery_or_repair() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, false);
    let manager = f.manager();
    let index = f.root.join(".aurora/recovery/index.sqlite3");
    let db = rusqlite::Connection::open(&index).unwrap();
    let instance_uuid = recovery::identity_lookup(&f.copy).unwrap().unwrap();
    db.execute(
        "INSERT INTO journals(instance_uuid,data) VALUES(?1,?2)",
        rusqlite::params![instance_uuid, b"owned malformed pending journal".as_slice()],
    )
    .unwrap();
    drop(db);
    let before = fs::read(&index).unwrap();
    assert_eq!(f.list(&manager).err(), Some(Error::NeedsRecovery));
    let failed = f.resume(&manager, &Uuid::new_v4().to_string(), 0).unwrap();
    assert!(failed.phase == Phase::Idle && failed.operation_id.is_none());
    assert_eq!(failed.last_error_code, Some(Error::NeedsRecovery));
    assert_eq!(fs::read(index).unwrap(), before);
    assert_eq!(f.bytes(), f.original);
    f.source_unchanged();
    f.unlocked();
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn durable_desktop_both_mods_reopen_resume_and_restore_without_jar_or_old_approval() {
    for voice_mod in [VoiceMod::SimpleVoiceChat, VoiceMod::PlasmoVoice] {
        let f = Fixture::new(voice_mod, true);
        let manager = f.manager();
        let applied = f.durable_applied(&manager);
        let id = applied
            .durable_receipt
            .as_ref()
            .unwrap()
            .receipt_id
            .clone()
            .unwrap();
        let original_operation = applied.operation_id.clone();
        assert_ne!(f.bytes(), f.original);
        close_saved(&manager, &applied);
        drop(manager);
        fs::remove_file(f.copy.root().join("mods/voice.jar")).unwrap();
        let manager = f.manager();
        let records = f.list(&manager).unwrap();
        assert_eq!(records.receipts.len(), 1);
        assert_eq!(records.receipts[0].receipt_id, id);
        assert_eq!(records.receipts[0].generation, 0);
        assert!(records.receipts[0].voice_mod == to_mod(voice_mod));
        assert_eq!(records.receipts[0].state, dto::SavedReceiptState::Ready);
        let encoded = serde_json::to_string(&records).unwrap();
        for forbidden in ["sha256", "config/", "127.0.0.1", "before", "token"] {
            assert!(!encoded.contains(forbidden));
        }
        let resumed = f.resume(&manager, &id, 0).unwrap();
        assert!(
            resumed.phase == Phase::Applied
                && resumed.restoration_state == Some(dto::RestorationState::Ready)
        );
        assert_ne!(resumed.operation_id, original_operation);
        assert!(
            resumed.observation.is_none()
                && resumed.review.is_none()
                && resumed.last_recovery_result.is_none()
        );
        assert_eq!(f.resume(&manager, &id, 0).err(), Some(Error::ContextActive));
        let reviewed =
            tauri::async_runtime::block_on(manager.prepare_restore(op(&resumed))).unwrap();
        let old_approval = approval(&reviewed);
        let cancelled =
            tauri::async_runtime::block_on(manager.cancel_review(op(&reviewed))).unwrap();
        assert!(cancelled.phase == Phase::Applied && cancelled.review.is_none());
        assert_eq!(
            tauri::async_runtime::block_on(manager.confirm(old_approval)).err(),
            Some(Error::StaleReview)
        );
        let reviewed =
            tauri::async_runtime::block_on(manager.prepare_restore(op(&cancelled))).unwrap();
        let replay = approval(&reviewed);
        let restored = confirm(&manager, &reviewed);
        assert!(
            restored.phase == Phase::Restored
                && restored.restoration_state == Some(dto::RestorationState::Restored)
        );
        assert_eq!(
            restored.durable_receipt.as_ref().unwrap().generation,
            Some(1)
        );
        assert_eq!(
            tauri::async_runtime::block_on(manager.confirm(replay)).err(),
            Some(Error::StaleReview)
        );
        assert_eq!(f.bytes(), f.original);
        drop(manager);
        let manager = f.manager();
        let records = f.list(&manager).unwrap();
        assert_eq!(records.receipts[0].state, dto::SavedReceiptState::Restored);
        assert_eq!(records.receipts[0].generation, 1);
        let stale = f.resume(&manager, &id, 0).unwrap();
        assert!(stale.phase == Phase::Idle && stale.operation_id.is_none());
        assert_eq!(stale.last_error_code, Some(Error::StaleReview));
        f.source_unchanged();
        f.unlocked();
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn durable_desktop_reconciled_restore_keeps_other_edits_and_rejects_own_conflicts() {
    for voice_mod in [VoiceMod::SimpleVoiceChat, VoiceMod::PlasmoVoice] {
        let f = Fixture::new(voice_mod, true);
        let manager = f.manager();
        let applied = f.durable_applied(&manager);
        let id = applied.durable_receipt.unwrap().receipt_id.unwrap();
        drop(manager);
        let manager = f.manager();
        let resumed = f.resume(&manager, &id, 0).unwrap();
        let mut edited = f.bytes();
        edited.extend_from_slice(b"# preserved external edit\n");
        f.write(&edited);
        let strict = tauri::async_runtime::block_on(manager.prepare_restore(op(&resumed))).unwrap();
        assert!(strict.phase == Phase::Applied && strict.review.is_none());
        assert_eq!(strict.last_error_code, Some(Error::StaleReview));
        let conflict = String::from_utf8(edited.clone())
            .unwrap()
            .replace("35554", "35555")
            .into_bytes();
        assert_ne!(conflict, edited);
        f.write(&conflict);
        let refused =
            tauri::async_runtime::block_on(manager.prepare_restore_reconciled(op(&resumed)))
                .unwrap();
        assert!(refused.review.is_none() && refused.last_error_code.is_some());
        assert_eq!(f.bytes(), conflict);
        f.write(&edited);
        fs::remove_file(f.copy.root().join("mods/voice.jar")).unwrap();
        let reviewed =
            tauri::async_runtime::block_on(manager.prepare_restore_reconciled(op(&resumed)))
                .unwrap();
        assert!(reviewed.review.as_ref().unwrap().action == dto::ReviewAction::RestoreReconciled);
        assert!(confirm(&manager, &reviewed).phase == Phase::Restored);
        let mut expected = f.original.clone();
        expected.extend_from_slice(b"# preserved external edit\n");
        assert_eq!(f.bytes(), expected);
        f.unlocked();
        f.source_unchanged();
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn durable_desktop_policy_precedes_approval_and_ram_shape_remains_unchanged() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, true);
    let manager = f.manager();
    let prepared = f.durable_prepared(&manager);
    let old_digest = prepared.review.as_ref().unwrap().digest.clone();
    let cancelled = tauri::async_runtime::block_on(manager.cancel_review(op(&prepared))).unwrap();
    assert!(cancelled.durable_receipt.is_none() && cancelled.phase == Phase::Observed);
    assert!(f.list(&f.manager()).unwrap().receipts.is_empty());
    let ram = tauri::async_runtime::block_on(manager.prepare_apply(dto::PrepareApplyRequest {
        operation_id: cancelled.operation_id.unwrap(),
        generation: cancelled.generation,
    }))
    .unwrap();
    assert!(ram.durable_receipt.is_none());
    assert!(!serde_json::to_value(&ram)
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("durableReceipt"));
    assert_ne!(ram.review.as_ref().unwrap().digest, old_digest);
    let mut stale = approval(&ram);
    stale.digest = old_digest;
    let failed = tauri::async_runtime::block_on(manager.confirm(stale)).unwrap();
    assert!(
        failed.review.is_none()
            && failed.durable_receipt.is_none()
            && failed.restoration_state.is_none()
    );
    assert_eq!(f.bytes(), f.original);
    let reviewed = f.durable_prepared(&manager);
    let mut wrong = approval(&reviewed);
    wrong.digest = "0".repeat(64);
    let failed = tauri::async_runtime::block_on(manager.confirm(wrong)).unwrap();
    assert!(failed.restoration_state.is_none() && failed.durable_receipt.is_none());
    assert!(f.list(&f.manager()).unwrap().receipts.is_empty());
    assert_eq!(f.bytes(), f.original);
    f.unlocked();
    f.source_unchanged();
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn durable_desktop_uncertain_apply_never_claims_saved_or_safe_selective_close() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, true);
    let manager = f.manager();
    let prepared = f.durable_prepared(&manager);
    let obstacle = f.copy.root().join(".aurora/connect-voice-stage");
    *manager.shared.test_hooks.stage.lock().unwrap() = Some(Arc::new(move |stage| {
        if stage == TestStage::ExecutorBegin {
            fs::write(&obstacle, b"owned fixture obstacle").unwrap();
        }
    }));
    let failed = confirm(&manager, &prepared);
    assert!(failed.phase == Phase::NeedsRecovery);
    let receipt = failed.durable_receipt.as_ref().unwrap();
    assert_eq!(receipt.state, dto::DurableReceiptState::Uncertain);
    assert!(receipt.receipt_id.is_none() && receipt.generation.is_none());
    let close = tauri::async_runtime::block_on(manager.begin_close(dto::BeginCloseRequest {
        operation_id: failed.operation_id.clone(),
        intent: dto::CloseIntent::Panel,
    }))
    .unwrap()
    .close
    .unwrap();
    let review = close.review.unwrap();
    assert!(review.selective_restoration_lost);
    assert_eq!(
        review.durable_receipt.unwrap().state,
        dto::DurableReceiptState::Uncertain
    );
    let cancelled = tauri::async_runtime::block_on(manager.cancel_close(dto::CloseRequest {
        close_id: close.close_id,
    }))
    .unwrap();
    assert!(cancelled.phase == Phase::NeedsRecovery);
    assert_eq!(
        cancelled.durable_receipt.unwrap().state,
        dto::DurableReceiptState::Uncertain
    );
    assert_eq!(f.bytes(), f.original);
    f.unlocked();
    f.source_unchanged();
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn durable_desktop_lost_apply_and_resume_responses_are_retained_before_exit() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, true);
    let manager = f.manager();
    let reviewed = f.durable_prepared(&manager);
    let gate = Arc::new(Gate::default());
    let hook = gate.clone();
    *manager.shared.test_hooks.stage.lock().unwrap() = Some(Arc::new(move |stage| {
        if stage == TestStage::OutcomeRetained {
            hook.enter();
        }
    }));
    let own = manager.clone();
    let request = approval(&reviewed);
    let task = tauri::async_runtime::spawn(async move { own.confirm(request).await });
    gate.wait();
    task.abort();
    let own = manager.clone();
    let exit = tauri::async_runtime::spawn(async move { own.begin_exit().await });
    spin(|| manager.snapshot().unwrap().phase == Phase::Closing);
    assert!(!manager.exit_ready());
    gate.release();
    let close = tauri::async_runtime::block_on(exit).unwrap().unwrap();
    let review = close.close.as_ref().unwrap().review.as_ref().unwrap();
    assert!(!review.selective_restoration_lost);
    assert_eq!(
        review.durable_receipt.as_ref().unwrap().state,
        dto::DurableReceiptState::Saved
    );
    let receipt_id = close.durable_receipt.unwrap().receipt_id.unwrap();
    drop(manager);
    let manager = f.manager();
    let gate = Arc::new(Gate::default());
    let hook = gate.clone();
    *manager.shared.test_hooks.stage.lock().unwrap() = Some(Arc::new(move |stage| {
        if stage == TestStage::ResumeReceipt {
            hook.enter();
        }
    }));
    let own = manager.clone();
    let root = f.root.clone();
    let instance_id = f.copy.id().as_str().to_owned();
    let task = tauri::async_runtime::spawn(async move {
        own.resume_receipt(
            &root,
            dto::ResumeReceiptRequest {
                instance_id,
                receipt_id,
                expected_generation: 0,
            },
        )
        .await
    });
    gate.wait();
    task.abort();
    let own = manager.clone();
    let exit = tauri::async_runtime::spawn(async move { own.begin_exit().await });
    spin(|| manager.snapshot().unwrap().phase == Phase::Closing);
    assert!(!manager.exit_ready());
    gate.release();
    let close = tauri::async_runtime::block_on(exit).unwrap().unwrap();
    assert!(close.operation_id.is_some() && close.review.is_none() && close.observation.is_none());
    let review = close.close.as_ref().unwrap().review.as_ref().unwrap();
    assert!(!review.selective_restoration_lost);
    assert_eq!(
        review.durable_receipt.as_ref().unwrap().state,
        dto::DurableReceiptState::Saved
    );
    assert!(!manager.exit_ready());
    f.unlocked();
    f.source_unchanged();
}
