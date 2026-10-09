//! Existing recovery state only. Never initializes identity, SQLite or CAS.
//! The legacy identity lookup keeps its existing shared read-only semantics;
//! the inspection path additionally refuses journal/WAL initialization states.
use super::*;

fn with_existing_identity<T>(
    instance: &Instance,
    strict: bool,
    inspect: impl FnOnce(&Identity, &Connection) -> Result<T>,
) -> Result<Option<T>> {
    let Some(id) = identity(instance, false)? else {
        return Ok(None);
    };
    let data = instance
        .root()
        .parent()
        .and_then(Path::parent)
        .ok_or(RecoveryError::UnsafePath)?;
    let database = path(data, ".aurora/recovery/index.sqlite3")?;
    if !database.exists() {
        return Err(RecoveryError::Identity);
    }
    let _directories = pin_directory(database.parent().ok_or(RecoveryError::UnsafePath)?)?;
    // secure_open denies writers through the retained Windows handle. A shared
    // read handle is deliberately preserved for the old identity_lookup API.
    let mut exclusive = if strict {
        Some(secure_open(&database)?)
    } else {
        None
    };
    let _shared = if strict {
        None
    } else {
        Some(shared_file(&database)?)
    };
    if let Some(file) = exclusive.as_mut() {
        let mut header = [0; 100];
        file.read_exact(&mut header)?;
        if &header[..16] != b"SQLite format 3\0" || header[18] != 1 || header[19] != 1 {
            return Err(RecoveryError::Storage);
        }
        // READ_ONLY by itself does not promise no WAL/SHM initialization. Do
        // not open such a database, clean sidecars, checkpoint or use immutable.
        for suffix in ["-journal", "-wal", "-shm"] {
            if path(data, &format!(".aurora/recovery/index.sqlite3{suffix}"))?.exists() {
                return Err(RecoveryError::Pending);
            }
        }
    }
    let db = Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    let owner: Option<String> = db
        .query_row(
            "SELECT instance_uuid FROM instances WHERE filesystem_identity=?1",
            [&id.filesystem_identity],
            |row| row.get(0),
        )
        .optional()?;
    if owner.as_deref() != Some(id.uuid.as_str()) {
        return Err(RecoveryError::Identity);
    }
    Ok(Some(inspect(&id, &db)?))
}

pub(super) fn identity_lookup(instance: &Instance) -> Result<Option<String>> {
    with_existing_identity(instance, false, |id, _| Ok(id.uuid.clone()))
}

#[cfg(feature = "connect-voice-lab")]
pub(crate) fn with_existing_observation<T>(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    inspect: impl FnOnce(&RecoveryObservation) -> Result<T>,
) -> Result<T> {
    let _pins = check_guard(instance, guard)?;
    with_existing_identity(instance, true, |id, db| {
        let schema: u32 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if schema != VERSION {
            return Err(RecoveryError::Integrity);
        }
        let pending = || -> Result<bool> {
            Ok(db.query_row(
                "SELECT EXISTS(SELECT 1 FROM journals WHERE instance_uuid=?1)",
                [&id.uuid],
                |row| row.get(0),
            )?)
        };
        if pending()? {
            return Err(RecoveryError::Pending);
        }
        let before = inventory(instance)?;
        let hash = fingerprint(&before)?;
        // Keep the deny-write database handle alive during the caller, including
        // diagnostic provenance checks that transitively use identity_lookup.
        let result = inspect(&RecoveryObservation {
            instance_uuid: id.uuid.clone(),
            fingerprint: hash.clone(),
            files: before.files,
            profile: before.profile,
            runtime: before.runtime,
        })?;
        if fingerprint(&inventory(instance)?)? != hash {
            return Err(RecoveryError::Changed);
        }
        if pending()? {
            return Err(RecoveryError::Pending);
        }
        let after = identity(instance, false)?.ok_or(RecoveryError::Identity)?;
        if after.uuid != id.uuid || after.filesystem_identity != id.filesystem_identity {
            return Err(RecoveryError::Identity);
        }
        Ok(result)
    })?
    .ok_or(RecoveryError::Identity)
}
