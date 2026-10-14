//! Retirement changes only indexed selective authority; default Recovery must
//! recognize its history even when no voice caller is compiled.
use super::*;

#[test]
fn retirement_history_is_distinct_and_default_reader_preserves_it() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    f.apply(&guard).unwrap();
    let store = Store::open(&f.instance).unwrap();
    let mut record = load(&store.db, &store.uuid, &f.id).unwrap();
    record.state = VoiceReceiptState::Retired;
    record.generation = 1;
    shape(&record, &store.uuid).unwrap();
    #[derive(Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum OldState {
        Ready,
        Restored,
    }
    #[derive(Deserialize)]
    struct OldRecord {
        #[serde(rename = "state")]
        _state: OldState,
    }
    let bytes = record_bytes(&record).unwrap();
    assert!(serde_json::from_slice::<OldRecord>(&bytes).is_err());
    assert!(String::from_utf8(bytes.clone())
        .unwrap()
        .contains("\"state\":\"retired\""));
    store
        .db
        .execute(
            "UPDATE voice_receipts SET state='retired',data=?1 WHERE id=?2",
            params![bytes, f.id],
        )
        .unwrap();
    drop(store);
    let store = Store::open(&f.instance).unwrap();
    let reopened = load(&store.db, &store.uuid, &f.id).unwrap();
    assert_eq!(reopened.state, VoiceReceiptState::Retired);
    assert_eq!(reopened.generation, 1);
    assert!(!pins_point(&store.db, &store.uuid, &record.before_point).unwrap());
    assert!(!pins_point(&store.db, &store.uuid, &record.applied_point).unwrap());
    record.generation = 0;
    assert!(matches!(
        shape(&record, &store.uuid),
        Err(RecoveryError::Integrity)
    ));
    record.state = VoiceReceiptState::Ready;
    record.generation = 1;
    assert!(matches!(
        shape(&record, &store.uuid),
        Err(RecoveryError::Integrity)
    ));
    assert_eq!(fs::read(f.config()).unwrap(), APPLIED);
}

#[cfg(feature = "connect-voice-lab")]
fn authority(f: &Fixture) -> (VoiceReceiptToken, String) {
    let store = Store::open(&f.instance).unwrap();
    let record = load(&store.db, &store.uuid, &f.id).unwrap();
    (
        VoiceReceiptToken {
            id: f.id.clone(),
            generation: record.generation,
            record_hash: digest(&record_bytes(&record).unwrap()),
        },
        fingerprint(&inventory(&f.instance).unwrap()).unwrap(),
    )
}
#[cfg(feature = "connect-voice-lab")]
fn protected_rows(f: &Fixture) -> Vec<u64> {
    let store = Store::open(&f.instance).unwrap();
    ["points", "refs", "undo", "previews", "journals"]
        .into_iter()
        .map(|table| {
            store
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap()
        })
        .collect()
}

#[cfg(feature = "connect-voice-lab")]
#[test]
fn retirement_commit_boundaries_reopen_without_file_or_snapshot_changes() {
    for fault in [
        Fault::ReceiptRetireBeforeCommit,
        Fault::ReceiptRetireAfterCommit,
    ] {
        let f = Fixture::new();
        let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
        f.apply(&guard).unwrap();
        let (token, fingerprint) = authority(&f);
        let rows = protected_rows(&f);
        FAULT.with(|value| value.set(fault));
        assert!(matches!(
            retire_voice_receipt(&f.instance, &guard, &token, &fingerprint, || Ok(()), || {}),
            Err(RecoveryError::Storage)
        ));
        FAULT.with(|value| value.set(Fault::None));
        drop(guard);
        let store = Store::open(&f.instance).unwrap();
        let record = load(&store.db, &store.uuid, &f.id).unwrap();
        let expected = if fault == Fault::ReceiptRetireAfterCommit {
            VoiceReceiptState::Retired
        } else {
            VoiceReceiptState::Ready
        };
        assert_eq!(record.state, expected);
        assert_eq!(
            record.generation,
            u64::from(expected == VoiceReceiptState::Retired)
        );
        drop(store);
        assert_eq!(fs::read(f.config()).unwrap(), APPLIED);
        assert_eq!(protected_rows(&f), rows);
        assert!(!has_pending(&f.instance).unwrap());
    }
}

#[cfg(feature = "connect-voice-lab")]
#[test]
fn retirement_rechecks_config_and_callback_before_atomic_publication() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    f.apply(&guard).unwrap();
    let (token, fingerprint) = authority(&f);
    let entered = std::cell::Cell::new(false);
    let config = f.config();
    assert!(matches!(
        retire_voice_receipt(
            &f.instance,
            &guard,
            &token,
            &fingerprint,
            || {
                fs::write(&config, ORIGINAL)?;
                Ok(())
            },
            || entered.set(true)
        ),
        Err(RecoveryError::StalePreview)
    ));
    assert!(!entered.get());
    assert_eq!(authority(&f).0.record_hash, token.record_hash);
    let fingerprint =
        crate::engine::recovery::fingerprint(&inventory(&f.instance).unwrap()).unwrap();
    assert!(matches!(
        retire_voice_receipt(
            &f.instance,
            &guard,
            &token,
            &fingerprint,
            || Err(RecoveryError::StalePreview),
            || entered.set(true)
        ),
        Err(RecoveryError::StalePreview)
    ));
    assert!(!entered.get());
    assert_eq!(authority(&f).0.record_hash, token.record_hash);
    assert_eq!(fs::read(f.config()).unwrap(), ORIGINAL);
}

#[cfg(feature = "connect-voice-lab")]
#[test]
fn retirement_refuses_pending_tampered_cas_and_stale_tokens() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    f.apply(&guard).unwrap();
    let (token, fingerprint) = authority(&f);
    let store = Store::open(&f.instance).unwrap();
    store
        .db
        .execute(
            "INSERT INTO journals VALUES(?1,?2)",
            params![store.uuid, b"blocked".as_slice()],
        )
        .unwrap();
    drop(store);
    assert!(matches!(
        retire_voice_receipt(&f.instance, &guard, &token, &fingerprint, || Ok(()), || {}),
        Err(RecoveryError::Pending)
    ));
    let store = Store::open(&f.instance).unwrap();
    store
        .db
        .execute("DELETE FROM journals WHERE instance_uuid=?1", [&store.uuid])
        .unwrap();
    let record = load(&store.db, &store.uuid, &f.id).unwrap();
    let cas = blob(&store.root, &record.original).unwrap();
    let saved = fs::read(&cas).unwrap();
    fs::write(&cas, b"corrupt owned fixture").unwrap();
    drop(store);
    assert!(matches!(
        retire_voice_receipt(&f.instance, &guard, &token, &fingerprint, || Ok(()), || {}),
        Err(RecoveryError::Integrity)
    ));
    fs::write(cas, saved).unwrap();
    let stale = VoiceReceiptToken {
        id: token.id.clone(),
        generation: 1,
        record_hash: token.record_hash.clone(),
    };
    assert!(matches!(
        retire_voice_receipt(&f.instance, &guard, &stale, &fingerprint, || Ok(()), || {}),
        Err(RecoveryError::StalePreview)
    ));
    assert_eq!(authority(&f).0.record_hash, token.record_hash);
    assert_eq!(fs::read(f.config()).unwrap(), APPLIED);
}

#[cfg(feature = "connect-voice-lab")]
#[test]
fn retirement_after_generic_undo_unlocks_new_receipt_and_keeps_history() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    f.apply(&guard).unwrap();
    let store = Store::open(&f.instance).unwrap();
    let old = load(&store.db, &store.uuid, &f.id).unwrap();
    drop(store);
    let undo = preview_undo(&f.instance, &guard).unwrap();
    restore(&f.instance, &guard, &undo.id).unwrap();
    assert_eq!(fs::read(f.config()).unwrap(), ORIGINAL);
    let (token, fingerprint) = authority(&f);
    let rows = protected_rows(&f);
    retire_voice_receipt(&f.instance, &guard, &token, &fingerprint, || Ok(()), || {}).unwrap();
    assert_eq!(protected_rows(&f), rows);
    assert!(matches!(
        retire_voice_receipt(&f.instance, &guard, &token, &fingerprint, || Ok(()), || {}),
        Err(RecoveryError::StalePreview)
    ));
    assert!(matches!(
        readonly::load_voice_receipt(&f.instance, &guard, &f.id),
        Err(RecoveryError::StalePreview)
    ));
    delete_point(&f.instance, &guard, &old.applied_point).unwrap();
    let preview = f.preview(&guard, APPLIED);
    let next_id = Uuid::new_v4().to_string();
    restore_attached(&f.instance, &guard, &preview.id, None, |store, journal| {
        let (Some(Transition::Apply { mut record }), ()) = f.attachment(store, journal)? else {
            panic!()
        };
        record.id = next_id.clone();
        Ok((Some(Transition::Apply { record }), ()))
    })
    .unwrap();
    let records = readonly::list_voice_receipts(&f.instance, &guard).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(
        records.iter().find(|r| r.id == f.id).unwrap().state,
        VoiceReceiptState::Retired
    );
    assert_eq!(
        records.iter().find(|r| r.id == next_id).unwrap().state,
        VoiceReceiptState::Ready
    );
}
