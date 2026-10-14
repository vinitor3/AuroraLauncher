//! Recovery-owned durable receipt records. This module is also compiled without
//! the voice feature so interrupted transactions never lose their attachment.
use super::*;

const RECORD_VERSION: u32 = 1;
const MAX_RECORD: usize = 16 * 1024;
const MAX_RECEIPTS: u64 = 64;
const MAX_IMAGE: u64 = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VoiceReceiptMod {
    SimpleVoiceChat,
    PlasmoVoice,
}
impl VoiceReceiptMod {
    pub(crate) fn config(self) -> &'static str {
        match self {
            Self::SimpleVoiceChat => "config/voicechat/voicechat-server.properties",
            Self::PlasmoVoice => "config/plasmovoice/server/config.toml",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::SimpleVoiceChat => "simple_voice_chat",
            Self::PlasmoVoice => "plasmo_voice",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VoiceReceiptState {
    Ready,
    Restored,
    Retired,
}
impl VoiceReceiptState {
    fn key(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Restored => "restored",
            Self::Retired => "retired",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Record {
    schema_version: u32,
    id: String,
    instance_uuid: String,
    voice_mod: VoiceReceiptMod,
    application_transaction_id: String,
    generation: u64,
    state: VoiceReceiptState,
    before_point: String,
    before_manifest_hash: String,
    applied_point: String,
    applied_manifest_hash: String,
    original: RecoveryFile,
    applied: RecoveryFile,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Transition {
    Apply {
        record: Box<Record>,
    },
    Restore {
        receipt_id: String,
        generation: u64,
        record_hash: String,
    },
}

/// Constructed only by the voice adapter after its original review/guard checks.
/// No serde and no caller-selected bytes or paths.
#[cfg(feature = "connect-voice-lab")]
pub(crate) enum VoiceReceiptIntent {
    Apply {
        id: String,
        voice_mod: VoiceReceiptMod,
        original_hash: String,
        applied_hash: String,
    },
    Restore(VoiceReceiptToken),
}
#[cfg(feature = "connect-voice-lab")]
pub(crate) struct VoiceReceiptToken {
    pub(crate) id: String,
    pub(crate) generation: u64,
    pub(crate) record_hash: String,
}
#[cfg(feature = "connect-voice-lab")]
pub(crate) struct VoiceReceiptLoaded {
    pub(crate) token: VoiceReceiptToken,
    pub(crate) voice_mod: VoiceReceiptMod,
    pub(crate) instance_uuid: String,
    pub(crate) runtime: BTreeMap<String, String>,
    pub(crate) state: VoiceReceiptState,
    pub(crate) original: Vec<u8>,
    pub(crate) applied: Vec<u8>,
}
#[cfg(feature = "connect-voice-lab")]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VoiceReceiptSummary {
    pub(crate) id: String,
    pub(crate) voice_mod: VoiceReceiptMod,
    pub(crate) state: VoiceReceiptState,
    pub(crate) generation: u64,
}

pub(super) fn initialize(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS voice_receipts(id TEXT PRIMARY KEY, instance_uuid TEXT NOT NULL, voice_mod TEXT NOT NULL, state TEXT NOT NULL, data BLOB NOT NULL); CREATE UNIQUE INDEX IF NOT EXISTS voice_receipts_active ON voice_receipts(instance_uuid,voice_mod) WHERE state='ready';")?;
    Ok(())
}
fn exists(db: &Connection) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='voice_receipts')",
        [],
        |row| row.get(0),
    )?)
}
fn record_bytes(record: &Record) -> Result<Vec<u8>> {
    let bytes = json(record)?;
    if bytes.len() > MAX_RECORD {
        return Err(RecoveryError::Limit);
    }
    Ok(bytes)
}
fn load(db: &Connection, uuid: &str, id: &str) -> Result<Record> {
    valid_id(id)?;
    if !exists(db)? {
        return Err(RecoveryError::NotFound);
    }
    let size: Option<u64> = db
        .query_row(
            "SELECT length(data) FROM voice_receipts WHERE id=?1 AND instance_uuid=?2",
            params![id, uuid],
            |row| row.get(0),
        )
        .optional()?;
    if size.ok_or(RecoveryError::NotFound)? > MAX_RECORD as u64 {
        return Err(RecoveryError::Limit);
    }
    let (kind, state, bytes): (String, String, Vec<u8>) = db.query_row(
        "SELECT voice_mod,state,data FROM voice_receipts WHERE id=?1 AND instance_uuid=?2",
        params![id, uuid],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let record: Record = parse(&bytes)?;
    if record.id != id
        || record.instance_uuid != uuid
        || record.voice_mod.key() != kind
        || record.state.key() != state
    {
        return Err(RecoveryError::Integrity);
    }
    shape(&record, uuid)?;
    Ok(record)
}
fn shape(record: &Record, uuid: &str) -> Result<()> {
    if record.schema_version != RECORD_VERSION
        || record.instance_uuid != uuid
        || !matches!(
            (record.state, record.generation),
            (VoiceReceiptState::Ready, 0)
                | (VoiceReceiptState::Restored, 1)
                | (VoiceReceiptState::Retired, 1)
        )
        || record.original.path != record.voice_mod.config()
        || record.applied.path != record.voice_mod.config()
        || record.original.size == 0
        || record.applied.size == 0
        || record.original.size > MAX_IMAGE
        || record.applied.size > MAX_IMAGE
        || record.original.sha256 == record.applied.sha256
        || [
            &record.original.sha256,
            &record.applied.sha256,
            &record.before_manifest_hash,
            &record.applied_manifest_hash,
        ]
        .iter()
        .any(|hash| !valid_hash(hash))
    {
        return Err(RecoveryError::Integrity);
    }
    for id in [
        &record.id,
        &record.application_transaction_id,
        &record.before_point,
        &record.applied_point,
    ] {
        valid_id(id)?;
    }
    if record.before_point == record.applied_point {
        return Err(RecoveryError::Integrity);
    }
    Ok(())
}
fn blob(root: &Path, file: &RecoveryFile) -> Result<PathBuf> {
    if !valid_hash(&file.sha256) {
        return Err(RecoveryError::Integrity);
    }
    path(
        root,
        &format!("blobs/sha256/{}/{}", &file.sha256[..2], file.sha256),
    )
}
fn manifest(
    root: &Path,
    db: &Connection,
    uuid: &str,
    id: &str,
    expected_hash: &str,
) -> Result<Manifest> {
    let (owner, hash): (String, String) = db
        .query_row(
            "SELECT instance_uuid,manifest_hash FROM points WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(RecoveryError::NotFound)?;
    if owner != uuid || hash != expected_hash {
        return Err(RecoveryError::Integrity);
    }
    let bytes = read_bounded(&path(root, &format!("manifests/{id}.json"))?, MAX_JSON)?;
    if digest(&bytes) != hash {
        return Err(RecoveryError::Integrity);
    }
    let manifest: Manifest = parse(&bytes)?;
    validate_manifest(&manifest, uuid)?;
    if manifest.point.id != id {
        return Err(RecoveryError::Integrity);
    }
    // Receipt authority depends on the exact bounded configuration images, not
    // on availability of an unrelated mod JAR in the live copy after applying.
    Ok(manifest)
}
fn verified(
    root: &Path,
    db: &Connection,
    uuid: &str,
    record: &Record,
) -> Result<BTreeMap<String, String>> {
    shape(record, uuid)?;
    let before = manifest(
        root,
        db,
        uuid,
        &record.before_point,
        &record.before_manifest_hash,
    )?;
    let after = manifest(
        root,
        db,
        uuid,
        &record.applied_point,
        &record.applied_manifest_hash,
    )?;
    if before.runtime != after.runtime
        || before.point.profile != after.point.profile
        || before.profile_files != after.profile_files
        || before.excluded_paths != after.excluded_paths
        || before
            .point
            .files
            .iter()
            .find(|file| file.path == record.voice_mod.config())
            != Some(&record.original)
        || after
            .point
            .files
            .iter()
            .find(|file| file.path == record.voice_mod.config())
            != Some(&record.applied)
        || restoration_diff(&before.point.files, &after).len() != 1
    {
        return Err(RecoveryError::Integrity);
    }
    for file in [&record.original, &record.applied] {
        verify(&blob(root, file)?, file)?;
    }
    Ok(before.runtime)
}

#[cfg(feature = "connect-voice-lab")]
pub(super) fn prepare(
    store: &Store,
    journal: &Journal,
    intent: VoiceReceiptIntent,
) -> Result<Transition> {
    let transition = match intent {
        VoiceReceiptIntent::Apply {
            id,
            voice_mod,
            original_hash,
            applied_hash,
        } => {
            valid_id(&id)?;
            let count: u64 = store.db.query_row(
                "SELECT count(*) FROM voice_receipts WHERE instance_uuid=?1",
                [&store.uuid],
                |row| row.get(0),
            )?;
            if count >= MAX_RECEIPTS {
                return Err(RecoveryError::Limit);
            }
            let active:bool=store.db.query_row("SELECT EXISTS(SELECT 1 FROM voice_receipts WHERE instance_uuid=?1 AND voice_mod=?2 AND state='ready')",params![store.uuid,voice_mod.key()],|row|row.get(0))?;
            if active {
                return Err(RecoveryError::Pending);
            }
            let before = store.load(&journal.undo_id)?;
            let after = store.load(&journal.point_id)?;
            let original = before
                .point
                .files
                .iter()
                .find(|file| file.path == voice_mod.config())
                .cloned()
                .ok_or(RecoveryError::Integrity)?;
            let applied = after
                .point
                .files
                .iter()
                .find(|file| file.path == voice_mod.config())
                .cloned()
                .ok_or(RecoveryError::Integrity)?;
            if original.sha256 != original_hash || applied.sha256 != applied_hash {
                return Err(RecoveryError::Integrity);
            }
            Transition::Apply {
                record: Box::new(Record {
                    schema_version: RECORD_VERSION,
                    id,
                    instance_uuid: store.uuid.clone(),
                    voice_mod,
                    application_transaction_id: journal.transaction_id.clone(),
                    generation: 0,
                    state: VoiceReceiptState::Ready,
                    before_point: journal.undo_id.clone(),
                    before_manifest_hash: digest(&json(&before)?),
                    applied_point: journal.point_id.clone(),
                    applied_manifest_hash: digest(&json(&after)?),
                    original,
                    applied,
                }),
            }
        }
        VoiceReceiptIntent::Restore(token) => Transition::Restore {
            receipt_id: token.id,
            generation: token.generation,
            record_hash: token.record_hash,
        },
    };
    validate(store, journal, &transition)?;
    Ok(transition)
}
pub(super) fn validate(store: &Store, journal: &Journal, transition: &Transition) -> Result<()> {
    let record = match transition {
        Transition::Apply { record } => {
            if record.application_transaction_id != journal.transaction_id
                || record.before_point != journal.undo_id
                || record.applied_point != journal.point_id
                || record.state != VoiceReceiptState::Ready
            {
                return Err(RecoveryError::Integrity);
            }
            let present:bool=store.db.query_row("SELECT EXISTS(SELECT 1 FROM voice_receipts WHERE id=?1 OR (instance_uuid=?2 AND voice_mod=?3 AND state='ready'))",params![record.id,store.uuid,record.voice_mod.key()],|row|row.get(0))?;
            if present {
                return Err(RecoveryError::Integrity);
            }
            record.as_ref().clone()
        }
        Transition::Restore {
            receipt_id,
            generation,
            record_hash,
        } => {
            let record = load(&store.db, &store.uuid, receipt_id)?;
            if record.state != VoiceReceiptState::Ready
                || record.generation != *generation
                || digest(&record_bytes(&record)?) != *record_hash
            {
                return Err(RecoveryError::StalePreview);
            }
            record
        }
    };
    let runtime = verified(&store.root, &store.db, &store.uuid, &record)?;
    let before = store.load(&journal.undo_id)?;
    let after = store.load(&journal.point_id)?;
    if runtime != before.runtime
        || runtime != after.runtime
        || journal.operations.len() != 1
        || journal.operations[0].path != record.voice_mod.config()
        || journal.operations[0].before.is_none()
        || journal.operations[0].after.is_none()
    {
        return Err(RecoveryError::Integrity);
    }
    if let Transition::Apply { .. } = transition {
        if journal.operations[0].before.as_ref() != Some(&record.original)
            || journal.operations[0].after.as_ref() != Some(&record.applied)
        {
            return Err(RecoveryError::Integrity);
        }
    }
    Ok(())
}
/// Executed inside the exact transaction that publishes undo and deletes journal.
pub(super) fn commit(db: &Connection, uuid: &str, transition: &Transition) -> Result<()> {
    match transition {
        Transition::Apply { record } => {
            db.execute(
                "INSERT INTO voice_receipts VALUES(?1,?2,?3,?4,?5)",
                params![
                    record.id,
                    uuid,
                    record.voice_mod.key(),
                    record.state.key(),
                    record_bytes(record)?
                ],
            )?;
        }
        Transition::Restore {
            receipt_id,
            generation,
            record_hash,
        } => {
            let mut record = load(db, uuid, receipt_id)?;
            if record.state != VoiceReceiptState::Ready
                || record.generation != *generation
                || digest(&record_bytes(&record)?) != *record_hash
            {
                return Err(RecoveryError::StalePreview);
            }
            record.state = VoiceReceiptState::Restored;
            record.generation += 1;
            if db.execute("UPDATE voice_receipts SET state='restored',data=?1 WHERE id=?2 AND instance_uuid=?3 AND state='ready'",params![record_bytes(&record)?,receipt_id,uuid])?!=1 {return Err(RecoveryError::StalePreview);}
        }
    }
    Ok(())
}
pub(super) fn pins_point(db: &Connection, uuid: &str, point: &str) -> Result<bool> {
    if !exists(db)? {
        return Ok(false);
    }
    let mut query = db.prepare(
        "SELECT substr(id,1,37) FROM voice_receipts WHERE instance_uuid=?1 AND state='ready' LIMIT 65",
    )?;
    let ids = query
        .query_map([uuid], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if ids.len() > MAX_RECEIPTS as usize {
        return Err(RecoveryError::Limit);
    }
    for id in ids {
        let record = load(db, uuid, &id)?;
        if record.before_point == point || record.applied_point == point {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(feature = "connect-voice-lab")]
pub(super) fn token(store: &Store, transition: &Transition) -> Result<VoiceReceiptToken> {
    match transition {
        Transition::Apply { record } => Ok(VoiceReceiptToken {
            id: record.id.clone(),
            generation: record.generation,
            record_hash: digest(&record_bytes(record)?),
        }),
        Transition::Restore {
            receipt_id,
            generation,
            ..
        } => {
            let mut record = load(&store.db, &store.uuid, receipt_id)?;
            record.state = VoiceReceiptState::Restored;
            record.generation = generation + 1;
            Ok(VoiceReceiptToken {
                id: receipt_id.clone(),
                generation: record.generation,
                record_hash: digest(&record_bytes(&record)?),
            })
        }
    }
}
#[cfg(feature = "connect-voice-lab")]
pub(super) fn list_existing(
    root: &Path,
    db: &Connection,
    uuid: &str,
) -> Result<Vec<VoiceReceiptSummary>> {
    if !exists(db)? {
        return Ok(Vec::new());
    }
    let mut query = db.prepare(
        "SELECT substr(id,1,37) FROM voice_receipts WHERE instance_uuid=?1 ORDER BY id LIMIT 65",
    )?;
    let ids = query
        .query_map([uuid], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if ids.len() > MAX_RECEIPTS as usize {
        return Err(RecoveryError::Limit);
    }
    let mut summaries = Vec::new();
    for id in ids {
        let record = load(db, uuid, &id)?;
        if record.state == VoiceReceiptState::Ready {
            verified(root, db, uuid, &record)?;
        }
        summaries.push(VoiceReceiptSummary {
            id: record.id,
            voice_mod: record.voice_mod,
            state: record.state,
            generation: record.generation,
        });
    }
    Ok(summaries)
}
#[cfg(feature = "connect-voice-lab")]
pub(super) fn load_existing(
    root: &Path,
    db: &Connection,
    uuid: &str,
    id: &str,
) -> Result<VoiceReceiptLoaded> {
    let record = load(db, uuid, id)?;
    if record.state != VoiceReceiptState::Ready {
        return Err(RecoveryError::StalePreview);
    }
    let runtime = verified(root, db, uuid, &record)?;
    let token = VoiceReceiptToken {
        id: record.id.clone(),
        generation: record.generation,
        record_hash: digest(&record_bytes(&record)?),
    };
    let original = read_bounded(&blob(root, &record.original)?, MAX_IMAGE)?;
    let applied = read_bounded(&blob(root, &record.applied)?, MAX_IMAGE)?;
    if digest(&original) != record.original.sha256
        || original.len() as u64 != record.original.size
        || digest(&applied) != record.applied.sha256
        || applied.len() as u64 != record.applied.size
    {
        return Err(RecoveryError::Integrity);
    }
    Ok(VoiceReceiptLoaded {
        token,
        voice_mod: record.voice_mod,
        instance_uuid: record.instance_uuid,
        runtime,
        state: record.state,
        original,
        applied,
    })
}

/// Explicitly relinquish selective authority. This transaction changes no file,
/// CAS reference, point, undo slot, preview or rename journal.
#[cfg(feature = "connect-voice-lab")]
pub(crate) fn retire_voice_receipt(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    token: &VoiceReceiptToken,
    expected_fingerprint: &str,
    before_commit: impl Fn() -> Result<()>,
    on_executor_begin: impl FnOnce(),
) -> Result<()> {
    let _directories = check_guard(instance, guard)?;
    valid_id(&token.id)?;
    if token.generation != 0 || !valid_hash(&token.record_hash) {
        return Err(RecoveryError::StalePreview);
    }
    let store = Store::open(instance)?;
    reject_pending(&store)?;
    let current = inventory(instance)?;
    if fingerprint(&current)? != expected_fingerprint {
        return Err(RecoveryError::StalePreview);
    }
    let mut record = load(&store.db, &store.uuid, &token.id)?;
    if record.state != VoiceReceiptState::Ready
        || record.generation != token.generation
        || digest(&record_bytes(&record)?) != token.record_hash
    {
        return Err(RecoveryError::StalePreview);
    }
    let runtime = verified(&store.root, &store.db, &store.uuid, &record)?;
    if runtime != current.runtime {
        return Err(RecoveryError::Compatibility);
    }
    // Build the full publication before starting the executor. No fallible
    // post-commit metadata read may report a committed state as uncommitted.
    record.state = VoiceReceiptState::Retired;
    record.generation = 1;
    let bytes = record_bytes(&record)?;
    shape(&record, &store.uuid)?;
    let transaction = store.db.unchecked_transaction()?;
    reject_pending(&store)?;
    let latest = load(&transaction, &store.uuid, &token.id)?;
    if latest.state != VoiceReceiptState::Ready
        || latest.generation != token.generation
        || digest(&record_bytes(&latest)?) != token.record_hash
    {
        return Err(RecoveryError::StalePreview);
    }
    // Recheck the original process/deadline guard after historical CAS reads.
    before_commit()?;
    if fingerprint(&inventory(instance)?)? != expected_fingerprint {
        return Err(RecoveryError::StalePreview);
    }
    // Inventory hashing can itself be slow; keep the deadline/process sample
    // on the publication side of that work as well.
    before_commit()?;
    on_executor_begin();
    if transaction.execute(
        "UPDATE voice_receipts SET state='retired',data=?1 WHERE id=?2 AND instance_uuid=?3 AND state='ready'",
        params![bytes, token.id, store.uuid],
    )? != 1 {
        return Err(RecoveryError::StalePreview);
    }
    #[cfg(test)]
    if FAULT.with(|fault| fault.get() == Fault::ReceiptRetireBeforeCommit) {
        return Err(RecoveryError::Storage);
    }
    transaction.commit()?;
    #[cfg(test)]
    if FAULT.with(|fault| fault.get() == Fault::ReceiptRetireAfterCommit) {
        return Err(RecoveryError::Storage);
    }
    Ok(())
}

#[cfg(test)]
#[path = "recovery_voice_receipt_tests.rs"]
mod tests;
