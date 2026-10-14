//! These tests also run with voice disabled: Recovery must understand attached
//! transactions and retain authority through interruptions independently of UI.
use super::*;
use crate::engine::{acquire_instance_recovery_lock, InstanceId, InstanceLayout};

const ORIGINAL: &[u8] = b"voice_host=\nport=24454\n";
const APPLIED: &[u8] = b"voice_host=127.0.0.1:35554\nport=24454\n";
struct Fixture {
    root: PathBuf,
    instance: Instance,
    id: String,
}
impl Fixture {
    fn new() -> Self {
        FAULT.with(|v| v.set(Fault::None));
        let root = std::env::temp_dir().join(format!("aurora-recovery-voice-{}", Uuid::new_v4()));
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("fixture").unwrap());
        instance.ensure_layout().unwrap();
        let dest = instance
            .root()
            .join(VoiceReceiptMod::SimpleVoiceChat.config());
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(dest, ORIGINAL).unwrap();
        Self {
            root,
            instance,
            id: Uuid::new_v4().to_string(),
        }
    }
    fn config(&self) -> PathBuf {
        self.instance
            .root()
            .join(VoiceReceiptMod::SimpleVoiceChat.config())
    }
    fn preview(&self, guard: &InstanceMutationGuard, desired: &[u8]) -> RecoveryPreview {
        let current = fs::read(self.config()).unwrap();
        fs::write(self.config(), desired).unwrap();
        let point = create_point(&self.instance, guard, None).unwrap();
        fs::write(self.config(), current).unwrap();
        preview_restore(&self.instance, guard, &point.id).unwrap()
    }
    fn attachment(&self, store: &Store, journal: &Journal) -> Result<(Option<Transition>, ())> {
        let before = store.load(&journal.undo_id)?;
        let after = store.load(&journal.point_id)?;
        let config = VoiceReceiptMod::SimpleVoiceChat.config();
        let record = Record {
            schema_version: 1,
            id: self.id.clone(),
            instance_uuid: store.uuid.clone(),
            voice_mod: VoiceReceiptMod::SimpleVoiceChat,
            application_transaction_id: journal.transaction_id.clone(),
            generation: 0,
            state: VoiceReceiptState::Ready,
            before_point: journal.undo_id.clone(),
            before_manifest_hash: digest(&json(&before)?),
            applied_point: journal.point_id.clone(),
            applied_manifest_hash: digest(&json(&after)?),
            original: before
                .point
                .files
                .iter()
                .find(|f| f.path == config)
                .unwrap()
                .clone(),
            applied: after
                .point
                .files
                .iter()
                .find(|f| f.path == config)
                .unwrap()
                .clone(),
        };
        Ok((
            Some(Transition::Apply {
                record: Box::new(record),
            }),
            (),
        ))
    }
    fn apply(&self, guard: &InstanceMutationGuard) -> Result<(RecoveryResult, ())> {
        let preview = self.preview(guard, APPLIED);
        restore_attached(&self.instance, guard, &preview.id, None, |store, j| {
            self.attachment(store, j)
        })
    }
    fn count(&self) -> u64 {
        let store = Store::open(&self.instance).unwrap();
        store
            .db
            .query_row("SELECT count(*) FROM voice_receipts", [], |row| row.get(0))
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        FAULT.with(|v| v.set(Fault::None));
        assert!(
            self.root.starts_with(std::env::temp_dir())
                && self
                    .root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("aurora-recovery-voice-")
        );
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn durable_receipt_before_journal_has_no_authority_or_configuration_mutation() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    FAULT.with(|v| v.set(Fault::ReceiptBeforeJournal));
    assert!(matches!(f.apply(&guard), Err(RecoveryError::Pending)));
    FAULT.with(|v| v.set(Fault::None));
    drop(guard);
    assert_eq!(fs::read(f.config()).unwrap(), ORIGINAL);
    assert_eq!(f.count(), 0);
    assert!(!has_pending(&f.instance).unwrap());
}

#[test]
fn durable_receipt_pending_apply_and_interrupted_rollback_reopen_without_voice_feature() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    FAULT.with(|v| v.set(Fault::ReceiptBeforeCommit));
    assert!(matches!(f.apply(&guard), Err(RecoveryError::Pending)));
    FAULT.with(|v| v.set(Fault::None));
    drop(guard);
    assert_eq!(fs::read(f.config()).unwrap(), APPLIED);
    assert_eq!(f.count(), 0);
    let store = Store::open(&f.instance).unwrap();
    let bytes: Vec<u8> = store
        .db
        .query_row("SELECT data FROM journals", [], |r| r.get(0))
        .unwrap();
    let journal: Journal = parse(&bytes).unwrap();
    assert_eq!(journal.schema_version, 2);
    assert!(journal.voice_receipt.is_some());
    drop(store);
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    FAULT.with(|v| v.set(Fault::ReceiptRollbackInterrupt));
    assert!(matches!(
        recover_pending(&f.instance, &guard),
        Err(RecoveryError::Pending)
    ));
    FAULT.with(|v| v.set(Fault::None));
    drop(guard);
    assert!(has_pending(&f.instance).unwrap());
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    assert!(recover_pending(&f.instance, &guard).unwrap());
    assert_eq!(fs::read(f.config()).unwrap(), ORIGINAL);
    assert_eq!(f.count(), 0);
    assert!(!recover_pending(&f.instance, &guard).unwrap());
}

#[test]
fn durable_receipt_lost_return_after_commit_is_ready_and_pins_historical_points() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    FAULT.with(|v| v.set(Fault::ReceiptAfterCommit));
    assert!(matches!(f.apply(&guard), Err(RecoveryError::Pending)));
    FAULT.with(|v| v.set(Fault::None));
    drop(guard);
    assert_eq!(fs::read(f.config()).unwrap(), APPLIED);
    assert_eq!(f.count(), 1);
    assert!(!has_pending(&f.instance).unwrap());
    let store = Store::open(&f.instance).unwrap();
    let record = load(&store.db, &store.uuid, &f.id).unwrap();
    assert_eq!(record.state, VoiceReceiptState::Ready);
    verified(&store.root, &store.db, &store.uuid, &record).unwrap();
    let point = record.applied_point.clone();
    drop(store);
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    assert!(matches!(
        delete_point(&f.instance, &guard, &point),
        Err(RecoveryError::Pending)
    ));
    assert!(!recover_pending(&f.instance, &guard).unwrap());
}

#[test]
fn durable_receipt_restore_rollback_retains_ready_then_commit_consumes_generation() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    f.apply(&guard).unwrap();
    let make_transition = |store: &Store| {
        let record = load(&store.db, &store.uuid, &f.id).unwrap();
        Transition::Restore {
            receipt_id: f.id.clone(),
            generation: 0,
            record_hash: digest(&record_bytes(&record).unwrap()),
        }
    };
    let preview = f.preview(&guard, ORIGINAL);
    FAULT.with(|v| v.set(Fault::ReceiptBeforeCommit));
    assert!(matches!(
        restore_attached(&f.instance, &guard, &preview.id, None, |store, _| Ok((
            Some(make_transition(store)),
            ()
        ))),
        Err(RecoveryError::Pending)
    ));
    FAULT.with(|v| v.set(Fault::None));
    drop(guard);
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    recover_pending(&f.instance, &guard).unwrap();
    assert_eq!(fs::read(f.config()).unwrap(), APPLIED);
    let store = Store::open(&f.instance).unwrap();
    let old = make_transition(&store);
    assert_eq!(
        load(&store.db, &store.uuid, &f.id).unwrap().state,
        VoiceReceiptState::Ready
    );
    drop(store);
    let preview = f.preview(&guard, ORIGINAL);
    FAULT.with(|v| v.set(Fault::ReceiptAfterCommit));
    assert!(matches!(
        restore_attached(&f.instance, &guard, &preview.id, None, |_, _| {
            Ok((Some(old.clone()), ()))
        }),
        Err(RecoveryError::Pending)
    ));
    FAULT.with(|v| v.set(Fault::None));
    drop(guard);
    let store = Store::open(&f.instance).unwrap();
    let record = load(&store.db, &store.uuid, &f.id).unwrap();
    assert_eq!(record.state, VoiceReceiptState::Restored);
    assert_eq!(record.generation, 1);
    assert!(matches!(
        commit(&store.db, &store.uuid, &old),
        Err(RecoveryError::StalePreview)
    ));
    assert_eq!(fs::read(f.config()).unwrap(), ORIGINAL);
}

#[test]
fn durable_receipt_tamper_and_unknown_journal_schema_preserve_journal() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    FAULT.with(|v| v.set(Fault::ReceiptBeforeCommit));
    assert!(f.apply(&guard).is_err());
    FAULT.with(|v| v.set(Fault::None));
    let store = Store::open(&f.instance).unwrap();
    let bytes: Vec<u8> = store
        .db
        .query_row("SELECT data FROM journals", [], |r| r.get(0))
        .unwrap();
    let mut journal: Journal = parse(&bytes).unwrap();
    journal.schema_version = 1;
    store
        .db
        .execute("UPDATE journals SET data=?1", [json(&journal).unwrap()])
        .unwrap();
    drop(store);
    assert!(matches!(
        recover_pending(&f.instance, &guard),
        Err(RecoveryError::Integrity)
    ));
    assert!(has_pending(&f.instance).unwrap());
    let store = Store::open(&f.instance).unwrap();
    store
        .db
        .execute("UPDATE journals SET data=?1", [&bytes])
        .unwrap();
    let journal: Journal = parse(&bytes).unwrap();
    let Transition::Apply { record } = journal.voice_receipt.unwrap() else {
        panic!()
    };
    fs::write(blob(&store.root, &record.original).unwrap(), b"tampered").unwrap();
    drop(store);
    assert!(matches!(
        recover_pending(&f.instance, &guard),
        Err(RecoveryError::Integrity)
    ));
    assert!(has_pending(&f.instance).unwrap());
    assert_eq!(fs::read(f.config()).unwrap(), APPLIED);
}

#[test]
fn durable_receipt_old_journal_serialization_keeps_exact_absence() {
    let j = Journal {
        schema_version: 1,
        instance_uuid: Uuid::new_v4().to_string(),
        transaction_id: Uuid::new_v4().to_string(),
        undo_id: Uuid::new_v4().to_string(),
        point_id: Uuid::new_v4().to_string(),
        operations: Vec::new(),
        voice_receipt: None,
    };
    let v: serde_json::Value = serde_json::from_slice(&json(&j).unwrap()).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 6);
    assert!(v.get("voiceReceipt").is_none());
    let parsed: Journal = parse(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(parsed.voice_receipt.is_none());
}

#[cfg(feature = "connect-voice-lab")]
#[test]
fn durable_receipt_reader_never_initializes_identity_or_repairs_missing_table() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    assert!(readonly::list_voice_receipts(&f.instance, &guard)
        .unwrap()
        .is_empty());
    assert!(!f.instance.root().join(IDENTITY).exists());
    assert!(!f.root.join(".aurora/recovery").exists());
    create_point(&f.instance, &guard, None).unwrap();
    let store = Store::open(&f.instance).unwrap();
    store.db.execute("DROP TABLE voice_receipts", []).unwrap();
    drop(store);
    let database = f.root.join(".aurora/recovery/index.sqlite3");
    let before = fs::read(&database).unwrap();
    assert!(readonly::list_voice_receipts(&f.instance, &guard)
        .unwrap()
        .is_empty());
    assert_eq!(fs::read(database).unwrap(), before);
}

#[test]
fn durable_receipt_backup_interruption_and_external_conflict_never_overwrite() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    FAULT.with(|v| v.set(Fault::BackupInterrupt));
    assert!(matches!(f.apply(&guard), Err(RecoveryError::Pending)));
    FAULT.with(|v| v.set(Fault::None));
    drop(guard);
    assert!(!f.config().exists());
    assert_eq!(f.count(), 0);
    fs::write(f.config(), b"external unrelated edit").unwrap();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    assert!(matches!(
        recover_pending(&f.instance, &guard),
        Err(RecoveryError::Changed)
    ));
    assert_eq!(fs::read(f.config()).unwrap(), b"external unrelated edit");
    assert!(has_pending(&f.instance).unwrap());
    // Explicit fixture repair only; Recovery never overwrites the conflicting edit.
    fs::remove_file(f.config()).unwrap();
    assert!(recover_pending(&f.instance, &guard).unwrap());
    assert_eq!(fs::read(f.config()).unwrap(), ORIGINAL);
    assert_eq!(f.count(), 0);
}

#[cfg(feature = "connect-voice-lab")]
#[test]
fn durable_receipt_restored_history_allows_point_deletion_and_new_ready_listing() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    f.apply(&guard).unwrap();
    let store = Store::open(&f.instance).unwrap();
    let record = load(&store.db, &store.uuid, &f.id).unwrap();
    let transition = Transition::Restore {
        receipt_id: f.id.clone(),
        generation: 0,
        record_hash: digest(&record_bytes(&record).unwrap()),
    };
    drop(store);
    let preview = f.preview(&guard, ORIGINAL);
    restore_attached(&f.instance, &guard, &preview.id, None, |_, _| {
        Ok((Some(transition), ()))
    })
    .unwrap();
    delete_point(&f.instance, &guard, &record.applied_point).unwrap();
    assert!(matches!(
        readonly::load_voice_receipt(&f.instance, &guard, &f.id),
        Err(RecoveryError::StalePreview)
    ));
    let next_id = Uuid::new_v4().to_string();
    let preview = f.preview(&guard, APPLIED);
    restore_attached(&f.instance, &guard, &preview.id, None, |store, j| {
        let (Some(Transition::Apply { mut record }), ()) = f.attachment(store, j)? else {
            panic!()
        };
        record.id = next_id.clone();
        Ok((Some(Transition::Apply { record }), ()))
    })
    .unwrap();
    let summaries = readonly::list_voice_receipts(&f.instance, &guard).unwrap();
    assert_eq!(summaries.len(), 2);
    assert_eq!(
        summaries.iter().find(|r| r.id == f.id).unwrap().state,
        VoiceReceiptState::Restored
    );
    assert_eq!(
        summaries.iter().find(|r| r.id == next_id).unwrap().state,
        VoiceReceiptState::Ready
    );
}

#[cfg(feature = "connect-voice-lab")]
#[test]
fn durable_receipt_index_metadata_limits_and_identity_are_checked() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    f.apply(&guard).unwrap();
    let store = Store::open(&f.instance).unwrap();
    let original = load(&store.db, &store.uuid, &f.id).unwrap();
    for mutate in [0, 1, 2, 3] {
        let mut record = original.clone();
        match mutate {
            0 => record.instance_uuid = Uuid::new_v4().to_string(),
            1 => record.generation = 9,
            2 => record.original.path = "config/unrelated.toml".into(),
            _ => record.schema_version = 99,
        };
        store
            .db
            .execute(
                "UPDATE voice_receipts SET data=?1 WHERE id=?2",
                params![record_bytes(&record).unwrap(), f.id],
            )
            .unwrap();
        assert!(matches!(
            load(&store.db, &store.uuid, &f.id),
            Err(RecoveryError::Integrity)
        ));
    }
    store
        .db
        .execute(
            "UPDATE voice_receipts SET data=?1 WHERE id=?2",
            params![record_bytes(&original).unwrap(), f.id],
        )
        .unwrap();
    assert!(matches!(
        load(&store.db, &Uuid::new_v4().to_string(), &f.id),
        Err(RecoveryError::NotFound)
    ));
    for _ in 0..64 {
        let mut record = original.clone();
        record.id = Uuid::new_v4().to_string();
        record.state = VoiceReceiptState::Restored;
        record.generation = 1;
        store
            .db
            .execute(
                "INSERT INTO voice_receipts VALUES(?1,?2,?3,?4,?5)",
                params![
                    record.id,
                    store.uuid,
                    record.voice_mod.key(),
                    record.state.key(),
                    record_bytes(&record).unwrap()
                ],
            )
            .unwrap();
    }
    assert!(matches!(
        list_existing(&store.root, &store.db, &store.uuid),
        Err(RecoveryError::Limit)
    ));
}

#[test]
fn durable_receipt_oversized_index_id_is_rejected_without_accepting_uuid_prefix() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    create_point(&f.instance, &guard, None).unwrap();
    let store = Store::open(&f.instance).unwrap();
    let oversized = format!("{}{}", f.id, "a".repeat(1024 * 1024));
    store
        .db
        .execute(
            "INSERT INTO voice_receipts VALUES(?1,?2,'simple_voice_chat','ready',?3)",
            params![oversized, store.uuid, b"{}".as_slice()],
        )
        .unwrap();
    assert!(matches!(
        pins_point(&store.db, &store.uuid, &Uuid::new_v4().to_string()),
        Err(RecoveryError::Integrity)
    ));
    #[cfg(feature = "connect-voice-lab")]
    assert!(matches!(
        list_existing(&store.root, &store.db, &store.uuid),
        Err(RecoveryError::Integrity)
    ));
    assert_eq!(fs::read(f.config()).unwrap(), ORIGINAL);
}
