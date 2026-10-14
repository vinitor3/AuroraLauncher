//! Standalone retirement reviews exercise the real disposable index; none of
//! these tests starts Java, downloads a JAR, or modifies a personal instance.
use super::*;

fn saved(f: &Fixture) -> (String, Vec<u8>) {
    let manager = f.manager();
    let observed = f.inspect(&manager);
    assert!(observed.phase == Phase::Observed);
    let reviewed =
        tauri::async_runtime::block_on(manager.prepare_apply_durable(dto::PrepareApplyRequest {
            operation_id: observed.operation_id.unwrap(),
            generation: observed.generation,
        }))
        .unwrap();
    let applied = confirm(&manager, &reviewed);
    assert!(applied.phase == Phase::Applied);
    let receipt = applied.durable_receipt.unwrap();
    assert_eq!(receipt.state, dto::DurableReceiptState::Saved);
    (receipt.receipt_id.unwrap(), f.bytes())
}
fn prepare(f: &Fixture, manager: &Manager, id: &str) -> dto::VoiceAnnouncementSnapshot {
    tauri::async_runtime::block_on(manager.prepare_retirement(
        &f.root,
        dto::ResumeReceiptRequest {
            instance_id: f.copy.id().as_str().into(),
            receipt_id: id.into(),
            expected_generation: 0,
        },
    ))
    .unwrap()
}
fn approve(snapshot: &dto::VoiceAnnouncementSnapshot) -> dto::ConfirmRequest {
    let review = snapshot.retirement_review.as_ref().unwrap();
    dto::ConfirmRequest {
        operation_id: snapshot.operation_id.clone().unwrap(),
        plan_id: review.plan_id.clone(),
        digest: review.digest.clone(),
    }
}
fn retire(
    manager: &Manager,
    snapshot: &dto::VoiceAnnouncementSnapshot,
) -> dto::VoiceAnnouncementSnapshot {
    tauri::async_runtime::block_on(manager.confirm_retirement(approve(snapshot))).unwrap()
}
fn list(f: &Fixture) -> dto::ReceiptList {
    tauri::async_runtime::block_on(f.manager().list_receipts(
        &f.root,
        dto::ListReceiptsRequest {
            instance_id: f.copy.id().as_str().into(),
        },
    ))
    .unwrap()
}

#[test]
fn desktop_retirement_invalid_unknown_and_pending_journal_do_not_create_authority() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, false);
    let manager = f.manager();
    let index = f.root.join(".aurora/recovery/index.sqlite3");
    let original_index = fs::read(&index).unwrap();
    for (id, generation) in [("../path".to_owned(), 0), (Uuid::new_v4().to_string(), 1)] {
        assert_eq!(
            tauri::async_runtime::block_on(manager.prepare_retirement(
                &f.root,
                dto::ResumeReceiptRequest {
                    instance_id: f.copy.id().as_str().into(),
                    receipt_id: id,
                    expected_generation: generation,
                }
            ))
            .err(),
            Some(Error::InvalidRequest)
        );
    }
    assert_eq!(
        tauri::async_runtime::block_on(manager.prepare_retirement(
            &f.root,
            dto::ResumeReceiptRequest {
                instance_id: f.source.id().as_str().into(),
                receipt_id: Uuid::new_v4().to_string(),
                expected_generation: 0,
            }
        ))
        .err(),
        Some(Error::UnsafePath)
    );
    let absent = prepare(&f, &manager, &Uuid::new_v4().to_string());
    assert!(
        absent.phase == Phase::Idle
            && absent.operation_id.is_none()
            && absent.retirement_review.is_none()
    );
    assert!(absent.last_error_code.is_some());
    assert_eq!(fs::read(&index).unwrap(), original_index);
    let db = rusqlite::Connection::open(&index).unwrap();
    db.execute(
        "INSERT INTO journals(instance_uuid,data) VALUES(?1,?2)",
        rusqlite::params![
            recovery::identity_lookup(&f.copy).unwrap().unwrap(),
            b"owned pending journal".as_slice()
        ],
    )
    .unwrap();
    drop(db);
    let pending_index = fs::read(&index).unwrap();
    let pending = prepare(&f, &manager, &Uuid::new_v4().to_string());
    assert!(
        pending.phase == Phase::Idle
            && pending.operation_id.is_none()
            && pending.retirement_review.is_none()
    );
    assert_eq!(pending.last_error_code, Some(Error::NeedsRecovery));
    assert_eq!(fs::read(index).unwrap(), pending_index);
    assert_eq!(f.bytes(), f.original);
    f.unlocked();
    f.source_unchanged();
}

#[test]
fn desktop_retirement_abandoned_preparation_keeps_the_exit_barrier_until_read_finishes() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, false);
    let manager = f.manager();
    let gate = Arc::new(Gate::default());
    let hook = gate.clone();
    *manager.shared.test_hooks.stage.lock().unwrap() = Some(Arc::new(move |stage| {
        if stage == TestStage::PrepareRetirement {
            hook.enter();
        }
    }));
    let own = manager.clone();
    let root = f.root.clone();
    let instance_id = f.copy.id().as_str().to_owned();
    let task = tauri::async_runtime::spawn(async move {
        own.prepare_retirement(
            &root,
            dto::ResumeReceiptRequest {
                instance_id,
                receipt_id: Uuid::new_v4().to_string(),
                expected_generation: 0,
            },
        )
        .await
    });
    gate.wait();
    task.abort();
    let pending = manager.snapshot().unwrap();
    assert!(
        pending.phase == Phase::PreparingRetirement
            && pending.busy
            && pending.operation_id.is_none()
    );
    assert!(
        pending.instance_id.is_none()
            && pending.voice_mod.is_none()
            && pending.retirement_review.is_none()
    );
    let own = manager.clone();
    let exit = tauri::async_runtime::spawn(async move { own.begin_exit().await });
    spin(|| manager.snapshot().unwrap().phase == Phase::Closing);
    assert!(!manager.exit_ready());
    gate.release();
    let closed = tauri::async_runtime::block_on(exit).unwrap().unwrap();
    assert!(
        closed.phase == Phase::Closed
            && closed.operation_id.is_none()
            && closed.retirement_review.is_none()
    );
    assert!(manager.exit_ready());
    assert_eq!(f.bytes(), f.original);
    f.unlocked();
    f.source_unchanged();
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn desktop_retirement_both_mods_keep_config_cancel_ready_and_publish_retired_distinctly() {
    for voice_mod in [VoiceMod::SimpleVoiceChat, VoiceMod::PlasmoVoice] {
        let f = Fixture::new(voice_mod, true);
        let (id, mut changed) = saved(&f);
        changed.extend_from_slice(b"# independent annotation\n");
        f.write(&changed);
        fs::remove_file(f.copy.root().join("mods/voice.jar")).unwrap();
        let manager = f.manager();
        let reviewed = prepare(&f, &manager, &id);
        assert!(reviewed.phase == Phase::ReviewRetirement && reviewed.review.is_none());
        assert_eq!(
            reviewed.durable_receipt.as_ref().unwrap().state,
            dto::DurableReceiptState::Saved
        );
        let view = reviewed.retirement_review.as_ref().unwrap();
        assert_eq!(view.receipt_id, id);
        assert!(view.voice_mod == to_mod(voice_mod));
        assert_eq!(view.expected_generation, 0);
        assert_eq!(view.current_config_sha256, Some(digest(&changed)));
        assert_eq!(view.current_config_bytes, Some(changed.len() as u64));
        assert!(!serde_json::to_string(view)
            .unwrap()
            .contains("independent annotation"));
        let old = approve(&reviewed);
        assert_eq!(
            tauri::async_runtime::block_on(manager.confirm(approve(&reviewed))).err(),
            Some(Error::StaleReview)
        );
        assert_eq!(
            tauri::async_runtime::block_on(manager.prepare_restore(op(&reviewed))).err(),
            Some(Error::StaleReview)
        );
        assert_eq!(
            tauri::async_runtime::block_on(manager.prepare_retirement(
                &f.root,
                dto::ResumeReceiptRequest {
                    instance_id: f.copy.id().as_str().into(),
                    receipt_id: id.clone(),
                    expected_generation: 0
                }
            ))
            .err(),
            Some(Error::ContextActive)
        );
        let cancelled =
            tauri::async_runtime::block_on(manager.cancel_review(op(&reviewed))).unwrap();
        assert!(
            cancelled.phase == Phase::Applied
                && cancelled.restoration_state == Some(dto::RestorationState::Ready)
                && cancelled.retirement_review.is_none()
        );
        assert_eq!(
            tauri::async_runtime::block_on(manager.confirm_retirement(old)).err(),
            Some(Error::StaleReview)
        );
        assert_eq!(f.bytes(), changed);
        assert_eq!(list(&f).receipts[0].state, dto::SavedReceiptState::Ready);
        drop(manager);
        let manager = f.manager();
        let reviewed = prepare(&f, &manager, &id);
        let replay = approve(&reviewed);
        let retired = retire(&manager, &reviewed);
        assert!(
            retired.phase == Phase::Retired
                && retired.restoration_state.is_none()
                && retired.retirement_review.is_none()
                && retired.last_recovery_result.is_none()
        );
        let receipt = retired.durable_receipt.as_ref().unwrap();
        assert_eq!(receipt.state, dto::DurableReceiptState::Retired);
        assert_eq!(receipt.generation, Some(1));
        assert_eq!(
            tauri::async_runtime::block_on(manager.confirm_retirement(replay)).err(),
            Some(Error::StaleReview)
        );
        assert_eq!(
            tauri::async_runtime::block_on(manager.prepare_restore(op(&retired))).err(),
            Some(Error::StaleReview)
        );
        let closed = tauri::async_runtime::block_on(manager.begin_close(dto::BeginCloseRequest {
            operation_id: retired.operation_id.clone(),
            intent: dto::CloseIntent::Panel,
        }))
        .unwrap();
        assert!(closed.phase == Phase::Closed && closed.close.as_ref().unwrap().review.is_none());
        assert_eq!(
            closed.durable_receipt.unwrap().state,
            dto::DurableReceiptState::Retired
        );
        let idle = tauri::async_runtime::block_on(manager.finish_close(dto::CloseRequest {
            close_id: closed.close.unwrap().close_id,
        }))
        .unwrap();
        assert!(idle.phase == Phase::Idle && idle.durable_receipt.is_none());
        let records = list(&f);
        assert_eq!(records.receipts[0].state, dto::SavedReceiptState::Retired);
        assert_eq!(records.receipts[0].generation, 1);
        let stale = tauri::async_runtime::block_on(manager.resume_receipt(
            &f.root,
            dto::ResumeReceiptRequest {
                instance_id: f.copy.id().as_str().into(),
                receipt_id: id,
                expected_generation: 0,
            },
        ))
        .unwrap();
        assert!(stale.phase == Phase::Idle && stale.operation_id.is_none());
        assert_eq!(stale.last_error_code, Some(Error::StaleReview));
        assert_eq!(f.bytes(), changed);
        f.unlocked();
        f.source_unchanged();
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn desktop_retirement_wrong_approval_changed_config_and_stale_second_plan_do_not_repeat_commit() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, true);
    let (id, changed) = saved(&f);
    let manager = f.manager();
    let reviewed = prepare(&f, &manager, &id);
    let mut wrong = approve(&reviewed);
    wrong.digest = "0".repeat(64);
    let failed = tauri::async_runtime::block_on(manager.confirm_retirement(wrong)).unwrap();
    assert!(failed.phase == Phase::Applied && failed.retirement_review.is_none());
    assert_eq!(failed.last_error_code, Some(Error::StaleReview));
    assert_eq!(f.bytes(), changed);
    drop(manager);
    let manager = f.manager();
    let reviewed = prepare(&f, &manager, &id);
    let mut edited = changed.clone();
    edited.extend_from_slice(b"# later edit\n");
    f.write(&edited);
    let failed = retire(&manager, &reviewed);
    assert!(failed.phase == Phase::Applied && failed.retirement_review.is_none());
    assert_eq!(failed.last_error_code, Some(Error::StaleReview));
    assert_eq!(list(&f).receipts[0].state, dto::SavedReceiptState::Ready);
    drop(manager);
    let first = f.manager();
    let second = f.manager();
    let a = prepare(&f, &first, &id);
    let b = prepare(&f, &second, &id);
    assert_ne!(
        a.retirement_review.as_ref().unwrap().digest,
        b.retirement_review.as_ref().unwrap().digest
    );
    assert!(retire(&first, &a).phase == Phase::Retired);
    let stale = retire(&second, &b);
    assert!(stale.phase == Phase::Applied && stale.retirement_review.is_none());
    assert_eq!(stale.last_error_code, Some(Error::StaleReview));
    assert_eq!(list(&f).receipts[0].state, dto::SavedReceiptState::Retired);
    assert_eq!(f.bytes(), edited);
    f.unlocked();
    f.source_unchanged();
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn desktop_retirement_lost_response_and_exit_retain_the_actual_committed_result() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, true);
    let (id, changed) = saved(&f);
    let manager = f.manager();
    let reviewed = prepare(&f, &manager, &id);
    let gate = Arc::new(Gate::default());
    let hook = gate.clone();
    *manager.shared.test_hooks.stage.lock().unwrap() = Some(Arc::new(move |stage| {
        if stage == TestStage::OutcomeRetained {
            hook.enter();
        }
    }));
    let own = manager.clone();
    let request = approve(&reviewed);
    let task = tauri::async_runtime::spawn(async move { own.confirm_retirement(request).await });
    gate.wait();
    task.abort();
    let own = manager.clone();
    let exit = tauri::async_runtime::spawn(async move { own.begin_exit().await });
    spin(|| manager.snapshot().unwrap().phase == Phase::Closing);
    assert!(!manager.exit_ready());
    assert!(manager.snapshot().unwrap().retirement_review.is_none());
    gate.release();
    let closed = tauri::async_runtime::block_on(exit).unwrap().unwrap();
    assert!(closed.phase == Phase::Closed && closed.close.as_ref().unwrap().review.is_none());
    assert_eq!(
        closed.durable_receipt.unwrap().state,
        dto::DurableReceiptState::Retired
    );
    assert!(manager.exit_ready());
    assert_eq!(list(&f).receipts[0].state, dto::SavedReceiptState::Retired);
    assert_eq!(f.bytes(), changed);
    f.unlocked();
    f.source_unchanged();
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn desktop_retirement_uncertain_executor_and_cancelled_close_preserve_conservative_state() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, true);
    let (id, changed) = saved(&f);
    let manager = f.manager();
    let reviewed = prepare(&f, &manager, &id);
    // An unwind after executor entry is ambiguous to the desktop. It must not
    // promise Retired or let the cached controller perform another write.
    *manager.shared.test_hooks.stage.lock().unwrap() = Some(Arc::new(|stage| {
        assert!(
            stage != TestStage::ExecutorBegin,
            "owned retirement interruption"
        );
    }));
    let uncertain = retire(&manager, &reviewed);
    assert!(uncertain.phase == Phase::NeedsRecovery && uncertain.retirement_review.is_none());
    assert_eq!(
        uncertain.durable_receipt.as_ref().unwrap().state,
        dto::DurableReceiptState::Uncertain
    );
    assert_eq!(
        uncertain.durable_receipt.as_ref().unwrap().generation,
        Some(0)
    );
    assert_eq!(
        tauri::async_runtime::block_on(manager.prepare_restore(op(&uncertain))).err(),
        Some(Error::StaleReview)
    );
    *manager.shared.test_hooks.stage.lock().unwrap() = None;
    let close = tauri::async_runtime::block_on(manager.begin_close(dto::BeginCloseRequest {
        operation_id: uncertain.operation_id.clone(),
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
    assert!(cancelled.phase == Phase::NeedsRecovery && cancelled.retirement_review.is_none());
    assert_eq!(
        cancelled.durable_receipt.unwrap().state,
        dto::DurableReceiptState::Uncertain
    );
    // The later read-only index observation, never config appearance, resolves
    // this pre-publication fixture as Ready. No retry was scheduled.
    drop(manager);
    assert_eq!(list(&f).receipts[0].state, dto::SavedReceiptState::Ready);
    assert_eq!(f.bytes(), changed);
    f.unlocked();
    f.source_unchanged();
}
