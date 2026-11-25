//! Per-instance world history. Only immutable, complete versions are reachable from the index.
use super::{content_updates::metadata_is_link, recovery, Instance, InstanceMutationGuard};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

type Result<T> = std::result::Result<T, BackupError>;
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("Não foi possível ler ou gravar o backup dos mundos. Confira o espaço livre e as permissões da pasta.")]
    Storage,
    #[error("Uma pasta ou arquivo do mundo não pôde ser validado. Confira se o jogo está fechado e se há links na pasta.")]
    Unsafe,
    #[error("O mundo mudou ou ainda está aberto. O backup anterior foi preservado.")]
    Changed,
    #[error("O backup excede o limite de armazenamento ou de arquivos desta instância.")]
    Limit,
    #[error("O backup não passou na verificação de integridade.")]
    Integrity,
}
impl From<std::io::Error> for BackupError {
    fn from(_: std::io::Error) -> Self {
        Self::Storage
    }
}
impl From<rusqlite::Error> for BackupError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
}
impl From<recovery::RecoveryError> for BackupError {
    fn from(_: recovery::RecoveryError) -> Self {
        Self::Unsafe
    }
}
const CHUNK: usize = 1024 * 1024;
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 65_536;
const MAX_WORLDS: usize = 128;
const KEEP: usize = 5;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    pub world: String,
    pub created_at: u64,
    pub bytes: u64,
    pub files: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub global_enabled: bool,
    pub instance_enabled: Option<bool>,
    pub pending: bool,
    pub last_error: Option<String>,
    pub versions: Vec<Version>,
    pub stored_bytes: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: String,
    bytes: u64,
    chunks: Option<Vec<String>>,
}
type Inventory = BTreeMap<String, (bool, u64, SystemTime)>;
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

struct Database {
    connection: Connection,
    _directory: Vec<File>,
    _leaf: File,
}
fn database(data: &Path, name: &str) -> Result<Database> {
    let directory = recovery::path(data, ".aurora/world-backups")?;
    fs::create_dir_all(&directory)?;
    let dirs = recovery::pin_directory(&directory)?;
    let path = recovery::path(&directory, name)?;
    // SQLite journal sidecars must satisfy the same link and single-file rules as the database.
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let file = recovery::path(&directory, &format!("{name}{suffix}"))?;
        if file.try_exists()? {
            let _file = recovery::shared_file(&file)?;
        }
    }
    if !path.try_exists()? && recovery::write_new(&path, &[]).is_err() && !path.try_exists()? {
        return Err(BackupError::Storage);
    }
    let leaf = recovery::shared_file(&path)?;
    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::default() | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    connection.busy_timeout(std::time::Duration::from_secs(3))?;
    connection.execute_batch(
        "PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF; PRAGMA synchronous=FULL;",
    )?;
    Ok(Database {
        connection,
        _directory: dirs,
        _leaf: leaf,
    })
}
pub fn any_pending(data: &Path) -> Result<bool> {
    if !recovery::path(data, ".aurora/world-backups/settings.sqlite3")?.is_file() {
        return Ok(false);
    }
    let db = database(data, "settings.sqlite3")?;
    if !initialized(&db.connection)? {
        return Ok(false);
    }
    Ok(db.connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM preferences WHERE pending=1 AND error IS NULL)",
        [],
        |r| r.get(0),
    )?)
}
fn initialized(db: &Connection) -> Result<bool> {
    Ok(db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))? == 1)
}
fn settings(instance: &Instance) -> Result<Database> {
    let db = database(instance.data_dir(), "settings.sqlite3")?;
    if initialized(&db.connection)? {
        return Ok(db);
    }
    db.connection.execute_batch("CREATE TABLE IF NOT EXISTS preferences(id TEXT PRIMARY KEY, enabled INTEGER, pending INTEGER NOT NULL DEFAULT 0, error TEXT); PRAGMA user_version=1;")?;
    Ok(db)
}
fn archive(instance: &Instance) -> Result<Database> {
    let db = database(instance.root(), "history.sqlite3")?;
    if initialized(&db.connection)? {
        return Ok(db);
    }
    db.connection.execute_batch("PRAGMA auto_vacuum=INCREMENTAL;
        CREATE TABLE IF NOT EXISTS versions(id TEXT PRIMARY KEY, world TEXT NOT NULL, created INTEGER NOT NULL, bytes INTEGER NOT NULL, files INTEGER NOT NULL, fingerprint TEXT NOT NULL, manifest BLOB NOT NULL);
        CREATE INDEX IF NOT EXISTS world_history ON versions(world,created);
        CREATE TABLE IF NOT EXISTS chunks(hash TEXT PRIMARY KEY, data BLOB NOT NULL);
        CREATE TABLE IF NOT EXISTS refs(version TEXT REFERENCES versions(id) ON DELETE CASCADE, hash TEXT REFERENCES chunks(hash), PRIMARY KEY(version,hash));
        CREATE INDEX IF NOT EXISTS chunk_refs ON refs(hash);
        CREATE TABLE IF NOT EXISTS stamps(version TEXT PRIMARY KEY REFERENCES versions(id) ON DELETE CASCADE, metadata BLOB NOT NULL);
        CREATE TABLE IF NOT EXISTS stages(id TEXT PRIMARY KEY); PRAGMA user_version=1;")?;
    Ok(db)
}
fn key(instance: &Instance) -> Result<String> {
    Ok(recovery::filesystem_identity(instance)?)
}
/// Capture the identity before a successful instance removal, then discard its
/// small pending/settings row. The actual world archive lives inside the instance.
pub fn remove_with_preferences(
    instance: &Instance,
    remove: impl FnOnce() -> std::result::Result<(), String>,
) -> std::result::Result<(), String> {
    let id = key(instance).ok();
    remove()?;
    if let Some(id) = id {
        if instance
            .data_dir()
            .join(".aurora/world-backups/settings.sqlite3")
            .is_file()
        {
            if let Ok(db) = settings(instance) {
                let _ = db
                    .connection
                    .execute("DELETE FROM preferences WHERE id=?1", [id]);
            }
        }
    }
    Ok(())
}
struct Preference {
    global: bool,
    local: Option<bool>,
    pending: bool,
    error: Option<String>,
}
fn preference(db: &Connection, key: &str) -> Result<Preference> {
    let global = db
        .query_row("SELECT enabled FROM preferences WHERE id='*'", [], |r| {
            r.get(0)
        })
        .optional()?
        .unwrap_or(true);
    let (local, pending, error) = db
        .query_row(
            "SELECT enabled,pending,error FROM preferences WHERE id=?1",
            [key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .unwrap_or((None, false, None));
    Ok(Preference {
        global,
        local,
        pending,
        error,
    })
}
pub fn configure(instance: &Instance, global: bool, enabled: Option<bool>) -> Result<()> {
    let key = key(instance)?;
    if global && enabled.is_none() {
        return Err(BackupError::Integrity);
    }
    settings(instance)?.connection.execute("INSERT INTO preferences(id,enabled) VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET enabled=excluded.enabled",params![if global{"*"}else{&key}, enabled])?;
    Ok(())
}
pub fn arm(instance: &Instance) -> Result<()> {
    // Capture the final state even for worlds first created during this game.
    let key = key(instance)?;
    settings(instance)?.connection.execute("INSERT INTO preferences(id,pending) VALUES (?1,1) ON CONFLICT(id) DO UPDATE SET pending=1,error=NULL",[key])?;
    Ok(())
}
pub fn needs_resume(instance: &Instance) -> Result<bool> {
    let dbpath = recovery::path(
        instance.data_dir(),
        ".aurora/world-backups/settings.sqlite3",
    )?;
    if !dbpath.is_file() {
        return Ok(false);
    }
    let prefs = preference(&settings(instance)?.connection, &key(instance)?)?;
    Ok(prefs.pending && prefs.error.is_none())
}
fn guard_root(instance: &Instance, guard: &InstanceMutationGuard) -> Result<Vec<File>> {
    if instance.root().canonicalize()? != guard.root {
        return Err(BackupError::Unsafe);
    }
    Ok(recovery::pin_directory(instance.root())?)
}
fn component(name: &str) -> Result<()> {
    if name.contains('/') {
        return Err(BackupError::Unsafe);
    }
    recovery::valid_relative(name)?;
    Ok(())
}
fn inventory(root: &Path) -> Result<Inventory> {
    let mut result = BTreeMap::new();
    let mut normalized = BTreeSet::new();
    let mut todo = vec![String::new()];
    let mut total = 0u64;
    while let Some(dir) = todo.pop() {
        let parent = if dir.is_empty() {
            root.to_owned()
        } else {
            recovery::path(root, &dir)?
        };
        let _pins = recovery::pin_directory(&parent)?;
        for item in fs::read_dir(&parent)? {
            let item = item?;
            let name = item
                .file_name()
                .into_string()
                .map_err(|_| BackupError::Unsafe)?;
            component(&name)?;
            if dir.is_empty() && name.eq_ignore_ascii_case("session.lock") {
                continue;
            }
            let rel = if dir.is_empty() {
                name
            } else {
                format!("{dir}/{name}")
            };
            if rel.len() > 1024 || rel.matches('/').count() > 48 || result.len() >= MAX_ENTRIES {
                return Err(BackupError::Limit);
            }
            if !normalized.insert(rel.to_lowercase()) {
                return Err(BackupError::Unsafe);
            }
            let meta = fs::symlink_metadata(item.path())?;
            if metadata_is_link(&meta) || !(meta.is_file() || meta.is_dir()) {
                return Err(BackupError::Unsafe);
            }
            let size = if meta.is_file() { meta.len() } else { 0 };
            total = total
                .checked_add(size)
                .filter(|v| *v <= MAX_BYTES)
                .ok_or(BackupError::Limit)?;
            if meta.is_dir() {
                todo.push(rel.clone());
            }
            result.insert(rel, (meta.is_dir(), size, meta.modified()?));
        }
    }
    Ok(result)
}
fn world_lock(world: &Path) -> Result<Option<File>> {
    let path = recovery::path(world, "session.lock")?;
    if !path.try_exists()? {
        return Ok(None);
    }
    let f = recovery::shared_file(&path)?;
    fs2::FileExt::try_lock_exclusive(&f).map_err(|_| BackupError::Changed)?;
    Ok(Some(f))
}
fn worlds(instance: &Instance) -> Result<Vec<(String, PathBuf)>> {
    let saves = recovery::path(instance.root(), "saves")?;
    if !saves.try_exists()? {
        return Ok(Vec::new());
    }
    let _pins = recovery::pin_directory(&saves)?;
    let mut list = Vec::new();
    for entry in fs::read_dir(&saves)? {
        let entry = entry?;
        let meta = fs::symlink_metadata(entry.path())?;
        if metadata_is_link(&meta) {
            return Err(BackupError::Unsafe);
        }
        if !meta.is_dir() {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| BackupError::Unsafe)?;
        component(&name)?;
        if recovery::path(&entry.path(), "level.dat")?.is_file()
            || recovery::path(&entry.path(), "level.dat_old")?.is_file()
        {
            list.push((name, entry.path()));
            if list.len() > MAX_WORLDS {
                return Err(BackupError::Limit);
            }
        }
    }
    list.sort();
    Ok(list)
}
fn chunk_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
type Stamps = BTreeMap<String, String>;
#[derive(Default)]
struct Previous {
    id: String,
    fingerprint: String,
    files: BTreeMap<String, Entry>,
    stamps: Stamps,
}
fn previous(db: &Connection, world: &str) -> Result<Previous> {
    let mut stmt=db.prepare("SELECT v.id,v.fingerprint,CASE WHEN length(v.manifest)<=25165824 THEN v.manifest END,CASE WHEN length(s.metadata)<=25165824 THEN s.metadata END FROM versions v LEFT JOIN stamps s ON s.version=v.id WHERE world=?1 ORDER BY v.rowid DESC LIMIT 1")?;
    let mut rows = stmt.query([world])?;
    let Some(row) = rows.next()? else {
        return Ok(Previous::default());
    };
    let encoded: Vec<u8> = row.get(2)?;
    let fingerprint: String = row.get(1)?;
    if chunk_hash(&encoded) != fingerprint {
        return Err(BackupError::Integrity);
    }
    let entries: Vec<Entry> =
        serde_json::from_slice(&encoded).map_err(|_| BackupError::Integrity)?;
    let bytes: Option<Vec<u8>> = row.get(3)?;
    let stamps = bytes
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    Ok(Previous {
        id: row.get(0)?,
        fingerprint,
        files: entries.into_iter().map(|e| (e.path.clone(), e)).collect(),
        stamps,
    })
}
fn save_stamps(db: &Connection, version: &str, stamps: &Stamps) -> Result<()> {
    let bytes = serde_json::to_vec(stamps).map_err(|_| BackupError::Integrity)?;
    db.execute(
        "INSERT OR REPLACE INTO stamps(version,metadata) VALUES(?1,?2)",
        params![version, bytes],
    )?;
    Ok(())
}

// A pinned, single-link handle supplies identity and change time. A filesystem that
// cannot supply trustworthy metadata falls back to hashing the full file.
#[cfg(windows)]
fn file_stamp(file: &File) -> Option<String> {
    use std::{ffi::c_void, os::windows::io::AsRawHandle};
    #[repr(C)]
    #[derive(Default)]
    struct Basic {
        created: i64,
        accessed: i64,
        written: i64,
        changed: i64,
        attributes: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Id {
        volume: u64,
        bytes: [u8; 16],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandleEx(
            h: *mut c_void,
            class: i32,
            output: *mut c_void,
            size: u32,
        ) -> i32;
        fn GetVolumeInformationByHandleW(
            h: *mut c_void,
            name: *mut u16,
            length: u32,
            serial: *mut u32,
            max_component: *mut u32,
            flags: *mut u32,
            system: *mut u16,
            capacity: u32,
        ) -> i32;
    }
    let h = file.as_raw_handle();
    let mut name = [0u16; 16];
    let mut basic = Basic::default();
    let mut id = Id::default();
    // Microsoft FILE_INFO_BY_HANDLE_CLASS: FileBasicInfo=0, FileIdInfo=18.
    unsafe {
        if GetVolumeInformationByHandleW(
            h,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        ) == 0
            || GetFileInformationByHandleEx(
                h,
                0,
                (&raw mut basic).cast(),
                std::mem::size_of::<Basic>() as u32,
            ) == 0
            || GetFileInformationByHandleEx(
                h,
                18,
                (&raw mut id).cast(),
                std::mem::size_of::<Id>() as u32,
            ) == 0
        {
            return None;
        }
    }
    if name[..5] != [78, 84, 70, 83, 0] || basic.changed <= 0 || id.bytes == [0; 16] {
        return None;
    }
    Some(format!(
        "{:x}:{:x}:{:x}:{:x}:{:x}",
        id.volume,
        u128::from_le_bytes(id.bytes),
        basic.created,
        basic.written,
        basic.changed
    ))
}
#[cfg(unix)]
fn file_stamp(file: &File) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let m = file.metadata().ok()?;
    Some(format!(
        "{}:{}:{}:{}:{}:{}",
        m.dev(),
        m.ino(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec()
    ))
}
#[cfg(not(any(windows, unix)))]
fn file_stamp(_: &File) -> Option<String> {
    None
}

#[cfg(test)]
thread_local! { static SOURCE_BYTES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }

fn capture_world(
    db: &Connection,
    world: &str,
    path: &Path,
    budget: &mut (u64, usize),
) -> Result<()> {
    let _pins = recovery::pin_directory(path)?;
    let _lock = world_lock(path)?;
    let before = inventory(path)?;
    budget.0 += before.values().map(|item| item.1).sum::<u64>();
    budget.1 += before.len();
    if budget.0 > MAX_BYTES || budget.1 > MAX_ENTRIES {
        return Err(BackupError::Limit);
    }
    let mut previous = previous(db, world)?;
    let mut stamps = Stamps::new();
    let mut manifest = Vec::with_capacity(before.len());
    let (mut bytes, mut files) = (0u64, 0u64);
    let mut chunk_hashes = BTreeSet::new();
    let mut buffer = vec![0; CHUNK];
    for (rel, (is_dir, size, _)) in &before {
        let chunks = if *is_dir {
            None
        } else {
            let mut source = recovery::secure_open(&recovery::path(path, rel)?)?;
            if let Some(stamp) = file_stamp(&source) {
                let same = previous.stamps.get(rel) == Some(&stamp);
                stamps.insert(rel.clone(), stamp);
                if same {
                    if let Some(entry) = previous
                        .files
                        .remove(rel)
                        .filter(|e| e.bytes == *size && e.chunks.is_some())
                    {
                        chunk_hashes.extend(entry.chunks.iter().flatten().cloned());
                        files += 1;
                        bytes += size;
                        manifest.push(entry);
                        continue;
                    }
                }
            }
            let mut chunks = Vec::new();
            let mut remaining = *size;
            while remaining > 0 {
                let n = remaining.min(CHUNK as u64) as usize;
                source
                    .read_exact(&mut buffer[..n])
                    .map_err(|_| BackupError::Changed)?;
                #[cfg(test)]
                SOURCE_BYTES.with(|b| b.set(b.get() + n as u64));
                let hash = chunk_hash(&buffer[..n]);
                if chunk_hashes.insert(hash.clone()) {
                    if chunk_hashes.len() % 32 == 1
                        && fs2::available_space(path)? < 192 * 1024 * 1024
                    {
                        return Err(BackupError::Storage);
                    }
                    let added = db.execute(
                        "INSERT OR IGNORE INTO chunks(hash,data) VALUES (?1,?2)",
                        params![hash, &buffer[..n]],
                    )?;
                    if added == 0
                        && !db.query_row(
                            "SELECT data=?2 FROM chunks WHERE hash=?1",
                            params![hash, &buffer[..n]],
                            |r| r.get::<_, bool>(0),
                        )?
                    {
                        return Err(BackupError::Integrity);
                    }
                }
                chunks.push(hash);
                remaining -= n as u64;
            }
            if source.read(&mut buffer[..1])? != 0 {
                return Err(BackupError::Changed);
            }
            bytes += size;
            files += 1;
            Some(chunks)
        };
        manifest.push(Entry {
            path: rel.clone(),
            bytes: *size,
            chunks,
        });
    }
    if inventory(path)? != before {
        return Err(BackupError::Changed);
    }
    let encoded = serde_json::to_vec(&manifest).map_err(|_| BackupError::Integrity)?;
    if encoded.len() > 24 * 1024 * 1024 {
        return Err(BackupError::Limit);
    }
    let fingerprint = chunk_hash(&encoded);
    if previous.fingerprint == fingerprint {
        if stamps != previous.stamps {
            save_stamps(db, &previous.id, &stamps)?;
        }
        return Ok(());
    }
    let id = Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO versions VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![id, world, now(), bytes, files, fingerprint, encoded],
    )?;
    save_stamps(db, &id, &stamps)?;
    db.execute("DELETE FROM stamps WHERE version=?1", [previous.id])?;
    let mut insert = db.prepare_cached("INSERT INTO refs(version,hash) VALUES(?1,?2)")?;
    for hash in chunk_hashes {
        insert.execute(params![id, hash])?;
    }
    db.execute("DELETE FROM versions WHERE id IN (SELECT id FROM versions WHERE world=?1 ORDER BY rowid DESC LIMIT -1 OFFSET ?2)",params![world, KEEP])?;
    Ok(())
}
fn stored(db: &Connection) -> Result<u64> {
    Ok(db.query_row(
        "SELECT COALESCE(SUM(length(data)),0) FROM chunks",
        [],
        |r| r.get(0),
    )?)
}
fn retain_history(tx: &Connection, max_versions: usize, max_bytes: u64) -> Result<()> {
    // Only an older version of the same world is eligible. Deleted or rarely
    // visited worlds keep their most recent recovery point.
    let historical="SELECT v.id FROM versions v WHERE EXISTS (SELECT 1 FROM versions n WHERE n.world=v.world AND n.rowid>v.rowid) ORDER BY v.rowid";
    let count: usize = tx.query_row("SELECT count(*) FROM versions", [], |r| r.get(0))?;
    let extra = count.saturating_sub(max_versions);
    if extra > 0
        && tx.execute(
            &format!("DELETE FROM versions WHERE id IN ({historical} LIMIT ?1)"),
            [extra],
        )? != extra
    {
        return Err(BackupError::Limit);
    }
    loop {
        tx.execute(
            "DELETE FROM chunks WHERE NOT EXISTS(SELECT 1 FROM refs WHERE refs.hash=chunks.hash)",
            [],
        )?;
        if stored(tx)? <= max_bytes {
            return Ok(());
        }
        if tx.execute(
            &format!("DELETE FROM versions WHERE id=({historical} LIMIT 1)"),
            [],
        )? == 0
        {
            return Err(BackupError::Limit);
        }
    }
}
fn capture(instance: &Instance, key: &str, guard: &InstanceMutationGuard) -> Result<()> {
    let _guard = guard_root(instance, guard)?;
    let current = worlds(instance)?;
    if current.is_empty() {
        return Ok(());
    }
    let mut db = archive(instance)?;
    if db
        .connection
        .query_row("SELECT EXISTS(SELECT 1 FROM stages)", [], |r| {
            r.get::<_, bool>(0)
        })?
    {
        cleanup_stages(instance, &db.connection)?;
    }
    let tx = db
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut budget = (0, 0);
    for (name, path) in &current {
        capture_world(&tx, name, path, &mut budget)?;
    }
    if worlds(instance)? != current {
        return Err(BackupError::Changed);
    }
    retain_history(&tx, MAX_WORLDS, MAX_BYTES)?;
    let prefs = preference(&settings(instance)?.connection, key)?;
    if prefs.local.unwrap_or(prefs.global) {
        tx.commit()?;
        let _ = db
            .connection
            .execute_batch("PRAGMA incremental_vacuum(256);");
    }
    Ok(())
}
pub fn after_exit(instance: &Instance, guard: &InstanceMutationGuard) -> Result<()> {
    after_exit_if_idle(instance, guard, || true)
}
/// A deferred process probe runs only when an enabled, pending backup needs it.
pub fn after_exit_if_idle(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    idle: impl FnOnce() -> bool,
) -> Result<()> {
    let key = key(instance)?;
    let db = settings(instance)?;
    let prefs = preference(&db.connection, &key)?;
    if !prefs.pending {
        return Ok(());
    }
    let result = if prefs.local.unwrap_or(prefs.global) {
        if !idle() {
            return Ok(());
        }
        capture(instance, &key, guard)
    } else {
        Ok(())
    };
    db.connection.execute(
        "UPDATE preferences SET pending=?2,error=?3 WHERE id=?1",
        params![
            key,
            result.is_err(),
            result.as_ref().err().map(ToString::to_string)
        ],
    )?;
    result
}
pub fn overview(instance: &Instance) -> Result<Overview> {
    let key = key(instance)?;
    let prefs = preference(&settings(instance)?.connection, &key)?;
    let mut out = Overview {
        global_enabled: prefs.global,
        instance_enabled: prefs.local,
        pending: prefs.pending,
        last_error: prefs.error,
        versions: vec![],
        stored_bytes: 0,
    };
    let file = recovery::path(instance.root(), ".aurora/world-backups/history.sqlite3")?;
    if !file.try_exists()? {
        return Ok(out);
    }
    let db = archive(instance)?;
    let mut query = db
        .connection
        .prepare("SELECT id,world,created,bytes,files FROM versions ORDER BY rowid DESC")?;
    out.versions = query
        .query_map([], |r| {
            Ok(Version {
                id: r.get(0)?,
                world: r.get(1)?,
                created_at: r.get(2)?,
                bytes: r.get(3)?,
                files: r.get(4)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    out.stored_bytes = stored(&db.connection)?;
    Ok(out)
}

fn cleanup_stages(instance: &Instance, db: &Connection) -> Result<PathBuf> {
    let parent = recovery::path(instance.root(), ".aurora/world-backup-staging")?;
    fs::create_dir_all(&parent)?;
    let _pins = recovery::pin_directory(&parent)?;
    let mut query = db.prepare("SELECT id FROM stages")?;
    let jobs = query
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(query);
    for id in jobs {
        recovery::valid_id(&id)?;
        let path = recovery::path(&parent, &id)?;
        if path.try_exists()? {
            if !fs::symlink_metadata(&path)?.is_dir() {
                return Err(BackupError::Unsafe);
            }
            fs::remove_dir_all(path)?;
        }
        db.execute("DELETE FROM stages WHERE id=?1", [id])?;
    }
    Ok(parent)
}
/// Restore as a sibling world. The live world is never a destination.
pub fn restore_copy(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    id: &str,
) -> Result<String> {
    recovery::valid_id(id)?;
    let _guard = guard_root(instance, guard)?;
    let db = archive(instance)?;
    let db = &db.connection;
    let (encoded,digest,total):(Vec<u8>,String,u64)=db.query_row(
        "SELECT manifest,fingerprint,bytes FROM versions WHERE id=?1 AND length(manifest)<=25165824",[id],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or(BackupError::Integrity)?;
    if total > MAX_BYTES || chunk_hash(&encoded) != digest {
        return Err(BackupError::Integrity);
    }
    let manifest: Vec<Entry> =
        serde_json::from_slice(&encoded).map_err(|_| BackupError::Integrity)?;
    if manifest.len() > MAX_ENTRIES
        || !manifest
            .iter()
            .any(|e| matches!(e.path.as_str(), "level.dat" | "level.dat_old") && e.chunks.is_some())
    {
        return Err(BackupError::Integrity);
    }
    if fs2::available_space(instance.root())? < total + 128 * 1024 * 1024 {
        return Err(BackupError::Storage);
    }
    let parent = cleanup_stages(instance, db)?;
    let _parent = recovery::pin_directory(&parent)?;
    let job = Uuid::new_v4().to_string();
    let stage = recovery::path(&parent, &job)?;
    let saves = recovery::path(instance.root(), "saves")?;
    fs::create_dir_all(&saves)?;
    let _saves = recovery::pin_directory(&saves)?;
    let name = format!("Aurora-restaurado-{job}");
    let dest = recovery::path(&saves, &name)?;
    fs::create_dir(&stage)?;
    if let Err(error) = db.execute("INSERT INTO stages(id) VALUES (?1)", [&job]) {
        let _ = fs::remove_dir(&stage);
        return Err(error.into());
    }
    let result = (|| {
        let mut count = 0u64;
        let mut seen = BTreeSet::new();
        for entry in manifest {
            recovery::valid_relative(&entry.path)?;
            if entry.path.eq_ignore_ascii_case("session.lock")
                || !seen.insert(entry.path.to_lowercase())
            {
                return Err(BackupError::Integrity);
            }
            let path = recovery::path(&stage, &entry.path)?;
            let _pins = recovery::pin_directory(path.parent().ok_or(BackupError::Unsafe)?)?;
            if let Some(chunks) = entry.chunks {
                count = count
                    .checked_add(entry.bytes)
                    .filter(|v| *v <= total)
                    .ok_or(BackupError::Integrity)?;
                if chunks.len() as u64 != entry.bytes.div_ceil(CHUNK as u64) {
                    return Err(BackupError::Integrity);
                }
                let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
                let mut remaining = entry.bytes;
                for hash in chunks {
                    let expected = remaining.min(CHUNK as u64);
                    let bytes: Vec<u8> = db
                        .query_row(
                            "SELECT data FROM chunks WHERE hash=?1 AND length(data)=?2",
                            params![hash, expected],
                            |r| r.get(0),
                        )
                        .optional()?
                        .ok_or(BackupError::Integrity)?;
                    if chunk_hash(&bytes) != hash {
                        return Err(BackupError::Integrity);
                    }
                    file.write_all(&bytes)?;
                    remaining -= expected;
                }
                file.sync_all()?;
            } else {
                if entry.bytes != 0 {
                    return Err(BackupError::Integrity);
                }
                fs::create_dir(path)?;
            }
        }
        if count != total || dest.try_exists()? {
            return Err(BackupError::Integrity);
        }
        recovery::durable_rename(&stage, &dest)?;
        Ok(name)
    })();
    if result.is_err() && stage.try_exists()? {
        fs::remove_dir_all(stage)?;
    }
    let _ = db.execute("DELETE FROM stages WHERE id=?1", [job]);
    result
}

#[cfg(test)]
#[path = "world_backups_tests.rs"]
mod tests;
