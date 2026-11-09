//! Manual restore points: immutable SHA-256 objects, SQLite FULL transactions,
//! per-instance locking, persisted previews and a durable rename journal.
#[cfg(test)]
#[path = "recovery_journal_tests.rs"]
mod journal_tests;
use super::content_updates::{ensure_no_symlink_components, metadata_is_link};
use super::{Instance, InstanceMutationGuard};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;
use zip::ZipArchive;

#[path = "recovery_readonly.rs"]
mod readonly;
#[path = "recovery_voice_receipts.rs"]
mod voice_receipts;
#[cfg(any(feature = "connect-lab", feature = "connect-voice-lab"))]
pub(crate) use readonly::with_existing_observation;
#[cfg(feature = "connect-voice-lab")]
pub(crate) use readonly::{list_voice_receipts, load_voice_receipt};
#[cfg(feature = "connect-voice-lab")]
pub(crate) use voice_receipts::{
    retire_voice_receipt, VoiceReceiptIntent, VoiceReceiptLoaded, VoiceReceiptMod,
    VoiceReceiptState, VoiceReceiptSummary, VoiceReceiptToken,
};

const VERSION: u32 = 1;
const MAX_FILES: usize = 30_000;
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_JSON: u64 = 32 * 1024 * 1024;
const IDENTITY: &str = ".aurora/recovery-identity.json";
const ROOTS: &[&str] = &["mods", "config", "defaultconfigs"];
const OPTIONS: &[&str] = &["options.txt", "optionsof.txt", "optionsshaders.txt"];
const PROFILE: &str = "aurora-modpack.json";
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    IdentityPublish,
    None,
    NoSpace,
    CaptureChange,
    StageWrite,
    BackupInterrupt,
    ReceiptBeforeJournal,
    ReceiptBeforeCommit,
    ReceiptAfterCommit,
    ReceiptRollbackInterrupt,
    #[cfg(feature = "connect-voice-lab")]
    ReceiptRetireBeforeCommit,
    #[cfg(feature = "connect-voice-lab")]
    ReceiptRetireAfterCommit,
}
#[cfg(test)]
thread_local! { static FAULT: std::cell::Cell<Fault> = const { std::cell::Cell::new(Fault::None) }; }
type Profile = BTreeMap<String, BTreeMap<String, Option<String>>>;
type Result<T> = std::result::Result<T, RecoveryError>;

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("Não foi possível gravar ou ler a recuperação. Verifique o espaço livre e as permissões; o estado anterior foi preservado ou aguarda recuperação segura.")]
    Io,
    #[error("O armazenamento da recuperação está indisponível ou corrompido. Nenhum ponto foi aplicado.")]
    Storage,
    #[error("O ponto contém dados ausentes, corrompidos ou um manifesto inválido. A restauração foi bloqueada.")]
    Integrity,
    #[error(
        "Um caminho, link ou arquivo não é seguro para recuperação. A operação foi bloqueada."
    )]
    UnsafePath,
    #[error("Os arquivos mudaram durante a operação. Feche programas que alteram a instância e tente novamente.")]
    Changed,
    #[error("A prévia ficou desatualizada. Gere uma nova prévia antes de confirmar.")]
    StalePreview,
    #[error("A instância original mudou desde a criação desta cópia. Crie uma nova cópia para testar o estado atual.")]
    OriginChanged,
    #[error("O ponto não pertence a esta instância. Instâncias recriadas não reutilizam pontos antigos.")]
    Identity,
    #[error("A versão ou o loader da instância mudou. Este ponto não pode restaurar mods para outro ambiente.")]
    Compatibility,
    #[error("Não há espaço suficiente para proteger o estado anterior e preparar a restauração. Nenhum arquivo foi alterado.")]
    NoSpace,
    #[error("Não existe um ponto disponível para esta operação.")]
    NotFound,
    #[error("A recuperação requer o bloqueio exclusivo desta instância com o jogo fechado.")]
    WrongLock,
    #[cfg(feature = "connect-voice-lab")]
    #[error("O laboratório de voz exige fechar todos os processos Java antes de alterar a configuração. Nenhum processo foi encerrado pelo Aurora.")]
    VoiceProcessRunning,
    #[cfg(feature = "connect-voice-lab")]
    #[error("Não foi possível verificar a ausência de processos Java. A alteração de voz foi bloqueada; nenhuma configuração foi aplicada por esta tentativa.")]
    VoiceProcessUnavailable,
    #[error(
        "A instância ultrapassa o limite de recuperação: 30 mil arquivos ou 8 GiB protegidos."
    )]
    Limit,
    #[error("Há uma recuperação interrompida. Abra Recuperação com o jogo fechado antes de alterar ou iniciar esta instância.")]
    Pending,
}
impl From<io::Error> for RecoveryError {
    fn from(_: io::Error) -> Self {
        Self::Io
    }
}
impl From<rusqlite::Error> for RecoveryError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryPoint {
    pub id: String,
    pub created_at: u64,
    pub description: String,
    pub scope: Vec<String>,
    pub logical_bytes: u64,
    pub file_count: usize,
    pub kind: String,
    pub excluded: Vec<String>,
    pub profile: Profile,
    pub files: Vec<RecoveryFile>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryOverview {
    pub points: Vec<RecoveryPoint>,
    pub unavailable_points: Vec<RecoveryUnavailablePoint>,
    pub undo_point: Option<RecoveryPoint>,
    pub undo_unavailable: bool,
    pub physical_bytes: u64,
    pub scope: Vec<String>,
    pub excluded: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryUnavailablePoint {
    pub id: String,
    pub created_at: u64,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryChange {
    pub path: String,
    pub action: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryProfileChange {
    pub file: String,
    pub field: String,
    pub before: Option<String>,
    pub after: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryPreview {
    pub id: String,
    pub point: RecoveryPoint,
    pub changes: Vec<RecoveryChange>,
    pub profile_changes: Vec<RecoveryProfileChange>,
    pub excluded: Vec<String>,
    pub current_fingerprint: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryResult {
    pub restored_point_id: String,
    pub undo_available: bool,
    pub recovered_interrupted: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    instance_uuid: String,
    runtime: BTreeMap<String, String>,
    profile_files: BTreeMap<String, Option<RecoveryFile>>,
    excluded_paths: BTreeSet<String>,
    point: RecoveryPoint,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Identity {
    schema_version: u32,
    uuid: String,
    filesystem_identity: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Inventory {
    files: Vec<RecoveryFile>,
    profile: Profile,
    runtime: BTreeMap<String, String>,
    profile_files: BTreeMap<String, Option<RecoveryFile>>,
    excluded: BTreeMap<String, String>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SavedPreview {
    schema_version: u32,
    instance_uuid: String,
    point_id: String,
    fingerprint: String,
    manifest_hash: String,
    undo: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Operation {
    path: String,
    before: Option<RecoveryFile>,
    after: Option<RecoveryFile>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    instance_uuid: String,
    transaction_id: String,
    undo_id: String,
    point_id: String,
    operations: Vec<Operation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    voice_receipt: Option<voice_receipts::Transition>,
}
struct Store {
    root: PathBuf,
    db: Connection,
    uuid: String,
    _directories: Vec<File>,
    _database_file: File,
}

fn scope() -> Vec<String> {
    [
        "mods/**/*.jar, *.jar.disabled e *.jar.off (exceto módulos Aurora)",
        "config/** e defaultconfigs/** (texto local sem dados sensíveis)",
        "options.txt, optionsof.txt e optionsshaders.txt",
        "aurora-modpack.json: campo name",
    ]
    .iter()
    .map(|s| (*s).into())
    .collect()
}
fn exclusions() -> Vec<String> {
    ["Mundos (saves) NÃO são incluídos nem restaurados.", "Screenshots, logs, caches, Java, bibliotecas e instalações compartilhadas.", "Credenciais, sessões, tokens, dados globais de conta e caminhos externos.", "Core, Skins e Companion: artefatos administrados pelo Aurora permanecem fora do ponto; a matriz ativa continua responsável por Core/Skins. Companion nunca é reinstalado.", "Versão, loader, identidade da instância e vínculos internos do launcher permanecem intactos."].iter().map(|s| (*s).into()).collect()
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn json<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(v).map_err(|_| RecoveryError::Integrity)
}
fn parse<T: for<'de> Deserialize<'de>>(b: &[u8]) -> Result<T> {
    serde_json::from_slice(b).map_err(|_| RecoveryError::Integrity)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn valid_id(s: &str) -> Result<()> {
    if Uuid::parse_str(s).is_ok_and(|id| id.to_string() == s) {
        Ok(())
    } else {
        Err(RecoveryError::Integrity)
    }
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn valid_relative(s: &str) -> Result<()> {
    if s.is_empty() || s.len() > 1024 || s.contains(['\\', ':', '\0']) {
        return Err(RecoveryError::UnsafePath);
    }
    for part in s.split('/') {
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with([' ', '.'])
            || part.chars().any(char::is_control)
            || part.contains(['<', '>', '"', '|', '?', '*'])
            || [
                "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
                "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
                "LPT9",
            ]
            .contains(&stem.as_str())
        {
            return Err(RecoveryError::UnsafePath);
        }
    }
    Ok(())
}
fn secure_root(root: &Path) -> Result<()> {
    for ancestor in root.ancestors() {
        let meta = fs::symlink_metadata(ancestor)?;
        if metadata_is_link(&meta) || !meta.is_dir() {
            return Err(RecoveryError::UnsafePath);
        }
    }
    Ok(())
}
pub(crate) fn path(root: &Path, rel: &str) -> Result<PathBuf> {
    valid_relative(rel)?;
    secure_root(root)?;
    ensure_no_symlink_components(root, rel).map_err(|_| RecoveryError::UnsafePath)?;
    Ok(root.join(rel))
}
pub(crate) fn check_guard(instance: &Instance, guard: &InstanceMutationGuard) -> Result<Vec<File>> {
    secure_root(instance.root())?;
    if guard.root != instance.root().canonicalize()? {
        return Err(RecoveryError::WrongLock);
    }
    let mut handles = pin_directory(instance.root())?;
    for root in ROOTS.iter().copied().chain([".aurora"]) {
        let directory = path(instance.root(), root)?;
        if directory.exists() {
            pin_tree(&directory, &mut handles, 0)?;
        }
    }
    Ok(handles)
}

/// Hold every existing directory involved in the operation against replacement
/// on Windows. New directories must be pinned by the caller after creation.
pub(crate) fn pin_directory(root: &Path) -> Result<Vec<File>> {
    secure_root(root)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let mut handles = Vec::new();
        for directory in root.ancestors() {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(0x0220_0000)
                .share_mode(3)
                .open(directory)?;
            let metadata = file.metadata()?;
            if !metadata.is_dir() || metadata_is_link(&metadata) {
                return Err(RecoveryError::UnsafePath);
            }
            handles.push(file);
        }
        Ok(handles)
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

pub(crate) fn pin_tree(root: &Path, handles: &mut Vec<File>, depth: usize) -> Result<()> {
    if depth > 35 || handles.len() > MAX_FILES.saturating_mul(40) {
        return Err(RecoveryError::Limit);
    }
    handles.extend(pin_directory(root)?);
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata_is_link(&metadata) {
            return Err(RecoveryError::UnsafePath);
        }
        if metadata.is_dir() {
            pin_tree(&entry.path(), handles, depth + 1)?;
        }
    }
    Ok(())
}

pub(crate) fn single_link(file: &File) -> Result<bool> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[repr(C)]
        struct Info {
            values: [u32; 13],
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut Info) -> i32;
        }
        let mut info = Info { values: [0; 13] };
        // SAFETY: info has the exact DWORD layout of BY_HANDLE_FILE_INFORMATION;
        // the handle belongs to a live File and the buffer lives through the call.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(RecoveryError::Io);
        }
        Ok(info.values[10] == 1)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(file.metadata()?.nlink() == 1)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = file;
        Ok(false)
    }
}

pub(crate) struct SecureFile {
    file: File,
    _directories: Vec<File>,
}
impl std::ops::Deref for SecureFile {
    type Target = File;
    fn deref(&self) -> &Self::Target {
        &self.file
    }
}
impl Read for SecureFile {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.file.read(buffer)
    }
}
impl Seek for SecureFile {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.file.seek(position)
    }
}
pub(crate) fn secure_open(p: &Path) -> Result<SecureFile> {
    let directories = pin_directory(p.parent().ok_or(RecoveryError::UnsafePath)?)?;
    let metadata = fs::symlink_metadata(p)?;
    if !metadata.is_file() || metadata_is_link(&metadata) {
        return Err(RecoveryError::UnsafePath);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000).share_mode(1);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0o400000); // O_NOFOLLOW
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x100); // O_NOFOLLOW
    }
    let file = options.open(p)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata_is_link(&metadata) || !single_link(&file)? {
        return Err(RecoveryError::UnsafePath);
    }
    Ok(SecureFile {
        file,
        _directories: directories,
    })
}

pub(crate) fn shared_file(p: &Path) -> Result<File> {
    let metadata = fs::symlink_metadata(p)?;
    if !metadata.is_file() || metadata_is_link(&metadata) {
        return Err(RecoveryError::UnsafePath);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000).share_mode(3);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0o400000);
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x100);
    }
    let file = options.open(p)?;
    if !file.metadata()?.is_file() || metadata_is_link(&file.metadata()?) || !single_link(&file)? {
        return Err(RecoveryError::UnsafePath);
    }
    Ok(file)
}
pub(crate) fn read_bounded(p: &Path, max: u64) -> Result<Vec<u8>> {
    let mut f = secure_open(p)?;
    if f.metadata()?.len() > max {
        return Err(RecoveryError::Limit);
    }
    let mut b = Vec::new();
    (&mut f).take(max + 1).read_to_end(&mut b)?;
    if b.len() as u64 > max {
        return Err(RecoveryError::Limit);
    }
    Ok(b)
}
pub(crate) fn write_new(p: &Path, b: &[u8]) -> Result<()> {
    let _directories = pin_directory(p.parent().ok_or(RecoveryError::UnsafePath)?)?;
    let mut f = OpenOptions::new().write(true).create_new(true).open(p)?;
    f.write_all(b)?;
    f.sync_all()?;
    sync_parent(p)?;
    Ok(())
}
fn sync_parent(p: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(p.parent().ok_or(RecoveryError::UnsafePath)?)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = p;
    Ok(())
}
pub(crate) fn durable_rename(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
        }
        let from: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: both paths are validated, NUL-terminated live UTF-16 buffers.
        // WRITE_THROUGH persists each same-volume rename before the journal is
        // completed. No REPLACE_EXISTING: unexpected destinations fail closed.
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0x8) } == 0 {
            return Err(RecoveryError::Io);
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, destination)?;
        sync_parent(source)?;
        sync_parent(destination)?;
    }
    Ok(())
}
pub(crate) fn hash_file(p: &Path) -> Result<(String, u64)> {
    let mut f = secure_open(p)?;
    if f.metadata()?.len() > MAX_BYTES {
        return Err(RecoveryError::Limit);
    }
    let mut h = Sha256::new();
    let n = io::copy(&mut (&mut f).take(MAX_BYTES + 1), &mut h)?;
    if n > MAX_BYTES {
        return Err(RecoveryError::Limit);
    }
    Ok((format!("{:x}", h.finalize()), n))
}
pub(crate) fn verify(p: &Path, file: &RecoveryFile) -> Result<()> {
    if hash_file(p)? != (file.sha256.clone(), file.size) {
        return Err(RecoveryError::Integrity);
    }
    Ok(())
}
fn identity(instance: &Instance, create: bool) -> Result<Option<Identity>> {
    let p = path(instance.root(), IDENTITY)?;
    let filesystem_identity =
        super::ai_tools::instance_identity(instance).map_err(|_| RecoveryError::Identity)?;
    if p.exists() {
        let id: Identity = parse(&read_bounded(&p, 4096)?)?;
        valid_id(&id.uuid)?;
        if id.schema_version != VERSION || id.filesystem_identity != filesystem_identity {
            return Err(RecoveryError::Identity);
        }
        return Ok(Some(id));
    }
    if !create {
        return Ok(None);
    }
    fs::create_dir_all(path(instance.root(), ".aurora")?)?;
    let id = Identity {
        schema_version: VERSION,
        uuid: Uuid::new_v4().to_string(),
        filesystem_identity,
    };
    let temporary = path(
        instance.root(),
        &format!(".aurora/recovery-identity-{}.pending", id.uuid),
    )?;
    let _directories = pin_directory(p.parent().ok_or(RecoveryError::UnsafePath)?)?;
    write_new(&temporary, &json(&id)?)?;
    #[cfg(test)]
    if FAULT.with(|fault| fault.get() == Fault::IdentityPublish) {
        return Err(RecoveryError::Io);
    }
    // A failed write or interrupted first capture must not leave a truncated
    // identity marker that subsequently blocks a healthy instance's launch.
    durable_rename(&temporary, &p)?;
    Ok(Some(id))
}
impl Store {
    fn open(instance: &Instance) -> Result<Self> {
        let id = identity(instance, true)?.ok_or(RecoveryError::Identity)?;
        let data = instance
            .root()
            .parent()
            .and_then(Path::parent)
            .ok_or(RecoveryError::UnsafePath)?;
        let root = path(data, ".aurora/recovery")?;
        fs::create_dir_all(&root)?;
        let mut directories = pin_directory(&root)?;
        for rel in ["blobs/sha256", "manifests"] {
            fs::create_dir_all(path(&root, rel)?)?;
        }
        pin_tree(&root, &mut directories, 0)?;
        for rel in [
            "index.sqlite3",
            "index.sqlite3-journal",
            "index.sqlite3-wal",
            "index.sqlite3-shm",
        ] {
            let file = path(&root, rel)?;
            if file.exists() {
                drop(shared_file(&file)?);
            }
        }
        let database_path = path(&root, "index.sqlite3")?;
        let db = Connection::open_with_flags(
            &database_path,
            rusqlite::OpenFlags::default() | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        // Allow SQLite to write, while denying replacement of the DB leaf on
        // Windows. The independent handle validates that it is not a hardlink.
        let database_file = shared_file(&database_path)?;
        db.busy_timeout(std::time::Duration::from_secs(10))?;
        let schema: u32 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if schema != 0 && schema != VERSION {
            return Err(RecoveryError::Integrity);
        }
        db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS points(id TEXT PRIMARY KEY, instance_uuid TEXT NOT NULL, manifest_hash TEXT NOT NULL, kind TEXT NOT NULL, created_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS refs(point_id TEXT NOT NULL REFERENCES points(id) ON DELETE CASCADE, hash TEXT NOT NULL, size INTEGER NOT NULL, PRIMARY KEY(point_id,hash)); CREATE TABLE IF NOT EXISTS previews(id TEXT PRIMARY KEY, instance_uuid TEXT NOT NULL, data BLOB NOT NULL); CREATE TABLE IF NOT EXISTS journals(instance_uuid TEXT PRIMARY KEY, data BLOB NOT NULL); CREATE TABLE IF NOT EXISTS undo(instance_uuid TEXT PRIMARY KEY, point_id TEXT NOT NULL); CREATE TABLE IF NOT EXISTS instances(instance_uuid TEXT PRIMARY KEY, filesystem_identity TEXT NOT NULL UNIQUE); PRAGMA user_version=1;")?;
        voice_receipts::initialize(&db)?;
        let owner: Option<String> = db
            .query_row(
                "SELECT filesystem_identity FROM instances WHERE instance_uuid=?1",
                [&id.uuid],
                |row| row.get(0),
            )
            .optional()?;
        if owner
            .as_ref()
            .is_some_and(|owner| owner != &id.filesystem_identity)
        {
            return Err(RecoveryError::Identity);
        }
        let recorded_uuid: Option<String> = db
            .query_row(
                "SELECT instance_uuid FROM instances WHERE filesystem_identity=?1",
                [&id.filesystem_identity],
                |row| row.get(0),
            )
            .optional()?;
        if recorded_uuid.as_ref().is_some_and(|uuid| uuid != &id.uuid) {
            return Err(RecoveryError::Identity);
        }
        db.execute(
            "INSERT OR IGNORE INTO instances VALUES(?1,?2)",
            params![id.uuid, id.filesystem_identity],
        )?;
        Ok(Self {
            root,
            db,
            uuid: id.uuid,
            _directories: directories,
            _database_file: database_file,
        })
    }
    fn blob(&self, sha: &str) -> Result<PathBuf> {
        if !valid_hash(sha) {
            return Err(RecoveryError::Integrity);
        }
        path(&self.root, &format!("blobs/sha256/{}/{sha}", &sha[..2]))
    }
    fn put(&self, source: &Path, record: &RecoveryFile) -> Result<()> {
        let dest = self.blob(&record.sha256)?;
        if dest.exists() {
            return verify(&dest, record);
        }
        fs::create_dir_all(dest.parent().ok_or(RecoveryError::UnsafePath)?)?;
        let _prefix_pins = pin_directory(dest.parent().ok_or(RecoveryError::UnsafePath)?)?;
        let tmp = path(
            &self.root,
            &format!(
                "blobs/sha256/{}/{}.pending",
                &record.sha256[..2],
                Uuid::new_v4()
            ),
        )?;
        let mut input = secure_open(source)?;
        let mut out = OpenOptions::new().create_new(true).write(true).open(&tmp)?;
        io::copy(&mut input, &mut out)?;
        out.sync_all()?;
        drop(out);
        if let Err(e) = verify(&tmp, record) {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        // Serialize object publication across instances; live files are copied,
        // never hardlinked. Existing objects are never replaced.
        let lock_path = path(&self.root, "publish.lock")?;
        if !lock_path.exists() {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(file) => file.sync_all()?,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        let lock = shared_file(&lock_path)?;
        lock.lock()?;
        if dest.exists() {
            verify(&dest, record)?;
            fs::remove_file(&tmp)?;
        } else {
            durable_rename(&tmp, &dest)?;
        }
        sync_parent(&dest)?;
        Ok(())
    }
    fn load(&self, id: &str) -> Result<Manifest> {
        valid_id(id)?;
        let row: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT instance_uuid,manifest_hash FROM points WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (owner, hash) = row.ok_or(RecoveryError::NotFound)?;
        if owner != self.uuid {
            return Err(RecoveryError::Identity);
        }
        let bytes = read_bounded(
            &path(&self.root, &format!("manifests/{id}.json"))?,
            MAX_JSON,
        )
        .map_err(|_| RecoveryError::Integrity)?;
        if digest(&bytes) != hash {
            return Err(RecoveryError::Integrity);
        }
        let manifest: Manifest = parse(&bytes)?;
        validate_manifest(&manifest, &self.uuid)?;
        if manifest.point.id != id {
            return Err(RecoveryError::Integrity);
        }
        for file in &manifest.point.files {
            let blob = self.blob(&file.sha256)?;
            verify(&blob, file).map_err(|_| RecoveryError::Integrity)?;
            if file.path.starts_with("mods/") && managed_mod(&blob, &file.path)? {
                return Err(RecoveryError::Integrity);
            }
            if !file.path.starts_with("mods/") && sensitive_config(&blob, &file.path)? {
                return Err(RecoveryError::Integrity);
            }
        }
        Ok(manifest)
    }
    fn publish(&mut self, manifest: &Manifest) -> Result<()> {
        validate_manifest(manifest, &self.uuid)?;
        let bytes = json(manifest)?;
        let destination = path(&self.root, &format!("manifests/{}.json", manifest.point.id))?;
        write_new(&destination, &bytes)?;
        if digest(&read_bounded(&destination, MAX_JSON)?) != digest(&bytes) {
            return Err(RecoveryError::Integrity);
        }
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT INTO points VALUES(?1,?2,?3,?4,?5)",
            params![
                manifest.point.id,
                self.uuid,
                digest(&bytes),
                manifest.point.kind,
                manifest.point.created_at
            ],
        )?;
        for f in &manifest.point.files {
            tx.execute(
                "INSERT OR IGNORE INTO refs VALUES(?1,?2,?3)",
                params![manifest.point.id, f.sha256, f.size],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}
fn protected(rel: &str) -> bool {
    (rel.starts_with("mods/") && mod_candidate(rel))
        || ["config", "defaultconfigs"]
            .iter()
            .any(|root| rel.starts_with(&format!("{root}/")))
        || OPTIONS.contains(&rel)
}
fn mod_candidate(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    [".jar", ".jar.disabled", ".jar.off"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}
fn validate_manifest(m: &Manifest, uuid: &str) -> Result<()> {
    if m.schema_version != VERSION || m.instance_uuid != uuid {
        return Err(RecoveryError::Identity);
    }
    valid_id(&m.point.id)?;
    if !matches!(m.point.kind.as_str(), "manual" | "undo")
        || m.point.scope != scope()
        || m.point.description.len() > 1000
        || m.point.files.len() > MAX_FILES
        || m.point.file_count != m.point.files.len()
    {
        return Err(RecoveryError::Integrity);
    }
    if m.runtime.iter().any(|(k, v)| {
        !["aurora-instance.json", PROFILE].iter().any(|file| {
            ["minecraft", "versionId", "loader", "source"]
                .iter()
                .any(|field| k == &format!("{file}:{field}"))
        }) || v.is_empty()
            || v.len() > 200
            || !v
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-+".contains(&c))
    }) {
        return Err(RecoveryError::Integrity);
    }
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for file in &m.point.files {
        valid_relative(&file.path)?;
        if !protected(&file.path)
            || !valid_hash(&file.sha256)
            || !names.insert(file.path.to_lowercase())
        {
            return Err(RecoveryError::Integrity);
        }
        total = total.checked_add(file.size).ok_or(RecoveryError::Limit)?;
    }
    if total > MAX_BYTES || total != m.point.logical_bytes {
        return Err(RecoveryError::Integrity);
    }
    if m.excluded_paths.len() > MAX_FILES
        || m.excluded_paths.iter().any(|p| {
            valid_relative(p).is_err()
                || !(ROOTS.iter().any(|r| p.starts_with(&format!("{r}/")))
                    || OPTIONS.contains(&p.as_str()))
                || m.point.files.iter().any(|f| f.path.eq_ignore_ascii_case(p))
        })
    {
        return Err(RecoveryError::Integrity);
    }
    for (p, file) in &m.profile_files {
        if !["aurora-instance.json", PROFILE].contains(&p.as_str())
            || file
                .as_ref()
                .is_some_and(|f| f.path != *p || f.size > 1024 * 1024 || !valid_hash(&f.sha256))
        {
            return Err(RecoveryError::Integrity);
        }
    }
    for (file, fields) in &m.point.profile {
        if file != PROFILE
            || fields.keys().any(|k| k != "name")
            || fields
                .values()
                .flatten()
                .any(|v| v.len() > 256 || v.chars().any(char::is_control))
        {
            return Err(RecoveryError::Integrity);
        }
    }
    Ok(())
}
pub(crate) fn managed_mod(p: &Path, rel: &str) -> Result<bool> {
    let lower = rel.to_lowercase();
    if [
        "aurora-core",
        "aurora_core",
        "aurora-skins",
        "aurora_skins",
        "aurora-companion",
        "aurora_companion",
    ]
    .iter()
    .any(|s| lower.contains(s))
    {
        return Ok(true);
    }
    let Ok(mut zip) = ZipArchive::new(secure_open(p)?) else {
        return Ok(false);
    };
    if zip.len() > 100_000 {
        return Err(RecoveryError::Limit);
    }
    if zip
        .file_names()
        .any(|n| n.starts_with("com/aurora/") || n.starts_with("org/aurora/"))
    {
        return Ok(true);
    }
    for name in [
        "fabric.mod.json",
        "mcmod.info",
        "META-INF/mods.toml",
        "META-INF/neoforge.mods.toml",
    ] {
        if let Ok(entry) = zip.by_name(name) {
            if entry.size() > 128 * 1024 {
                return Err(RecoveryError::Integrity);
            }
            let mut text = String::new();
            entry
                .take(128 * 1024 + 1)
                .read_to_string(&mut text)
                .map_err(|_| RecoveryError::Integrity)?;
            let text = text.to_lowercase();
            if [
                "aurora_core",
                "aurora_skins",
                "aurora_companion",
                "aurora-core",
                "aurora-skins",
                "aurora-companion",
            ]
            .iter()
            .any(|s| text.contains(s))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
pub(crate) fn sensitive_config(p: &Path, rel: &str) -> Result<bool> {
    let lower = rel.to_lowercase();
    if [
        "token",
        "session",
        "account",
        "auth",
        "credential",
        "password",
        "secret",
        "aurora",
    ]
    .iter()
    .any(|s| lower.contains(s))
    {
        return Ok(true);
    }
    if fs::metadata(p)?.len() > 4 * 1024 * 1024 {
        return Ok(true);
    }
    let bytes = read_bounded(p, 4 * 1024 * 1024)?;
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Ok(true);
    };
    if text.contains('\0') {
        return Ok(true);
    }
    let lower = text.to_lowercase();
    Ok([
        "token",
        "password",
        "credential",
        "secret",
        "sessionid",
        "session_id",
        "apikey",
        "api_key",
        "accesskey",
        "privatekey",
        "authorization",
        "clientsecret",
        ":\\",
        ":/",
        "file://",
        "\\\\",
        "\"/",
        "=/",
        "= /",
        ": /",
        "'/",
        "../",
        "..\\",
        "~/",
        "~\\",
        "${",
        "$home",
        "%appdata%",
        "%userprofile%",
        "%localappdata%",
        "%homedrive%",
        "%homepath%",
    ]
    .iter()
    .any(|s| lower.contains(s)))
}
fn inventory(instance: &Instance) -> Result<Inventory> {
    let mut inv = Inventory {
        files: vec![],
        profile: BTreeMap::new(),
        runtime: BTreeMap::new(),
        profile_files: BTreeMap::new(),
        excluded: BTreeMap::new(),
    };
    let mut visited = BTreeSet::new();
    for root in ROOTS {
        let p = path(instance.root(), root)?;
        if p.exists() {
            walk(instance, root, &mut inv, &mut visited, 0)?;
        }
    }
    for rel in OPTIONS {
        if path(instance.root(), rel)?.exists() {
            scan_file(instance, rel, &mut inv, &mut visited)?;
        }
    }
    for rel in ["aurora-instance.json", PROFILE] {
        let p = path(instance.root(), rel)?;
        if !p.exists() {
            inv.profile_files.insert(rel.into(), None);
            continue;
        }
        let bytes = read_bounded(&p, 1024 * 1024)?;
        inv.profile_files.insert(
            rel.into(),
            Some(RecoveryFile {
                path: rel.into(),
                sha256: digest(&bytes),
                size: bytes.len() as u64,
            }),
        );
        let doc: serde_json::Value = parse(&bytes)?;
        if !doc.is_object() {
            return Err(RecoveryError::Integrity);
        }
        for field in ["minecraft", "versionId", "loader", "source"] {
            if let Some(s) = doc.get(field).and_then(|v| v.as_str()) {
                if s.len() > 200
                    || s.is_empty()
                    || !s
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"._-+".contains(&c))
                {
                    return Err(RecoveryError::Integrity);
                }
                inv.runtime.insert(format!("{rel}:{field}"), s.to_owned());
            }
        }
        if rel == PROFILE {
            let name = doc
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|v| v.len() <= 256 && !v.chars().any(char::is_control))
                .map(str::to_owned);
            if doc.get("name").is_some_and(|v| !v.is_null()) && name.is_none() {
                return Err(RecoveryError::UnsafePath);
            }
            inv.profile
                .insert(rel.into(), BTreeMap::from([("name".into(), name)]));
        }
    }
    inv.profile
        .entry(PROFILE.into())
        .or_insert_with(|| BTreeMap::from([("name".into(), None)]));
    inv.files.sort_by(|a, b| a.path.cmp(&b.path));
    if inv.files.iter().map(|f| f.size).sum::<u64>() > MAX_BYTES {
        return Err(RecoveryError::Limit);
    }
    Ok(inv)
}
fn walk(
    instance: &Instance,
    rel: &str,
    inv: &mut Inventory,
    visited: &mut BTreeSet<String>,
    depth: usize,
) -> Result<()> {
    if depth > 32 {
        return Err(RecoveryError::Limit);
    }
    let p = path(instance.root(), rel)?;
    if !p.is_dir() {
        return Err(RecoveryError::UnsafePath);
    }
    for entry in fs::read_dir(&p)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| RecoveryError::UnsafePath)?;
        let relative = format!("{rel}/{name}");
        let child = path(instance.root(), &relative)?;
        let metadata = fs::symlink_metadata(&child)?;
        if metadata.is_dir() {
            walk(instance, &relative, inv, visited, depth + 1)?;
        } else if metadata.is_file() {
            scan_file(instance, &relative, inv, visited)?;
        } else {
            return Err(RecoveryError::UnsafePath);
        }
    }
    Ok(())
}
fn scan_file(
    instance: &Instance,
    rel: &str,
    inv: &mut Inventory,
    visited: &mut BTreeSet<String>,
) -> Result<()> {
    valid_relative(rel)?;
    if !visited.insert(rel.to_lowercase()) {
        return Err(RecoveryError::UnsafePath);
    }
    if inv.files.len() + inv.excluded.len() >= MAX_FILES {
        return Err(RecoveryError::Limit);
    }
    let p = path(instance.root(), rel)?;
    if !p.is_file() {
        return Err(RecoveryError::UnsafePath);
    }
    let (sha256, size) = hash_file(&p)?;
    if size > MAX_BYTES {
        return Err(RecoveryError::Limit);
    }
    let excluded = if rel.starts_with("mods/") {
        !mod_candidate(rel) || managed_mod(&p, rel)?
    } else {
        sensitive_config(&p, rel)?
    };
    if excluded {
        inv.excluded.insert(rel.into(), sha256);
    } else {
        inv.files.push(RecoveryFile {
            path: rel.into(),
            size,
            sha256,
        });
    }
    Ok(())
}
fn fingerprint(inv: &Inventory) -> Result<String> {
    Ok(digest(&json(inv)?))
}
fn capture(
    instance: &Instance,
    store: &mut Store,
    description: String,
    kind: &str,
) -> Result<Manifest> {
    let before = inventory(instance)?;
    ensure_space(
        &store.root,
        before
            .files
            .iter()
            .map(|f| f.size)
            .sum::<u64>()
            .saturating_add(1024 * 1024),
    )?;
    for file in &before.files {
        store.put(&path(instance.root(), &file.path)?, file)?;
    }
    #[cfg(test)]
    if FAULT.with(|f| f.get() == Fault::CaptureChange) {
        fs::write(
            path(instance.root(), "options.txt")?,
            b"changed during capture",
        )?;
    }
    let after = inventory(instance)?;
    if fingerprint(&before)? != fingerprint(&after)? {
        return Err(RecoveryError::Changed);
    }
    let mut excluded = exclusions();
    excluded.extend(
        before
            .excluded
            .keys()
            .map(|p| format!("{p} (módulo administrado ou configuração sensível/não local)")),
    );
    let m = Manifest {
        schema_version: VERSION,
        instance_uuid: store.uuid.clone(),
        runtime: before.runtime,
        profile_files: before.profile_files,
        excluded_paths: before.excluded.keys().cloned().collect(),
        point: RecoveryPoint {
            id: Uuid::new_v4().to_string(),
            created_at: now(),
            description,
            scope: scope(),
            logical_bytes: before.files.iter().map(|f| f.size).sum(),
            file_count: before.files.len(),
            kind: kind.into(),
            excluded,
            profile: before.profile,
            files: before.files,
        },
    };
    store.publish(&m)?;
    Ok(m)
}
pub(crate) fn ensure_space(root: &Path, required: u64) -> Result<()> {
    #[cfg(test)]
    if FAULT.with(|f| f.get() == Fault::NoSpace) {
        return Err(RecoveryError::NoSpace);
    }
    if fs2::available_space(root)? < required {
        return Err(RecoveryError::NoSpace);
    }
    Ok(())
}

/// Verified local state shared by Doctor and disposable diagnostic copies.
/// It records hashes only; credentials and excluded file contents never enter it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryObservation {
    pub instance_uuid: String,
    pub fingerprint: String,
    pub files: Vec<RecoveryFile>,
    pub profile: BTreeMap<String, BTreeMap<String, Option<String>>>,
    pub runtime: BTreeMap<String, String>,
}

pub fn filesystem_identity(instance: &Instance) -> Result<String> {
    super::ai_tools::instance_identity(instance).map_err(|_| RecoveryError::Identity)
}

pub fn identity_lookup(instance: &Instance) -> Result<Option<String>> {
    readonly::identity_lookup(instance)
}

pub fn capture_observation(
    instance: &Instance,
    guard: &InstanceMutationGuard,
) -> Result<RecoveryObservation> {
    let _directories = check_guard(instance, guard)?;
    let store = Store::open(instance)?;
    reject_pending(&store)?;
    let before = inventory(instance)?;
    let hash = fingerprint(&before)?;
    if fingerprint(&inventory(instance)?)? != hash {
        return Err(RecoveryError::Changed);
    }
    Ok(RecoveryObservation {
        instance_uuid: store.uuid,
        fingerprint: hash,
        files: before.files,
        profile: before.profile,
        runtime: before.runtime,
    })
}

/// Builds a narrowly selected target in the existing CAS, then uses the same
/// persisted preview, protection capture, transaction journal and undo executor.
/// Callers must additionally validate the supported operation (mod toggle or
/// allowlisted setting) before supplying these replacement paths.
#[derive(Clone)]
pub(crate) struct SelectedFile {
    pub source: PathBuf,
    pub sha256: String,
    pub size: u64,
}

pub(crate) fn preview_selected_files(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    expected_fingerprint: &str,
    replacements: &BTreeMap<String, Option<SelectedFile>>,
    description: &str,
) -> Result<RecoveryPreview> {
    let _directories = check_guard(instance, guard)?;
    let mut store = Store::open(instance)?;
    reject_pending(&store)?;
    let current = inventory(instance)?;
    if fingerprint(&current)? != expected_fingerprint {
        return Err(RecoveryError::StalePreview);
    }
    if replacements.is_empty() || replacements.len() > MAX_FILES {
        return Err(RecoveryError::Integrity);
    }
    let mut files: BTreeMap<_, _> = current
        .files
        .iter()
        .cloned()
        .map(|f| (f.path.clone(), f))
        .collect();
    for (rel, replacement) in replacements {
        valid_relative(rel)?;
        if !protected(rel) || current.excluded.keys().any(|p| p.eq_ignore_ascii_case(rel)) {
            return Err(RecoveryError::UnsafePath);
        }
        if let Some(replacement) = replacement {
            let source = &replacement.source;
            let sha256 = replacement.sha256.clone();
            let size = replacement.size;
            let expected = RecoveryFile {
                path: rel.clone(),
                sha256: sha256.clone(),
                size,
            };
            verify(source, &expected)?;
            if (rel.starts_with("mods/") && managed_mod(source, rel)?)
                || (!rel.starts_with("mods/") && sensitive_config(source, rel)?)
            {
                return Err(RecoveryError::UnsafePath);
            }
            let file = RecoveryFile {
                path: rel.clone(),
                sha256,
                size,
            };
            ensure_space(&store.root, size.saturating_add(1024 * 1024))?;
            store.put(source, &file)?;
            files.insert(rel.clone(), file);
        } else {
            files.remove(rel);
        }
    }
    // Existing unchanged files belong to the target too; persist their verified
    // contents so restoring this preview can never silently omit protection.
    for file in &current.files {
        store.put(&path(instance.root(), &file.path)?, file)?;
    }
    let files: Vec<_> = files.into_values().collect();
    let manifest = Manifest {
        schema_version: VERSION,
        instance_uuid: store.uuid.clone(),
        runtime: current.runtime.clone(),
        profile_files: current.profile_files.clone(),
        excluded_paths: current.excluded.keys().cloned().collect(),
        point: RecoveryPoint {
            id: Uuid::new_v4().to_string(),
            created_at: now(),
            description: description.into(),
            scope: scope(),
            logical_bytes: files.iter().map(|f| f.size).sum(),
            file_count: files.len(),
            kind: "manual".into(),
            excluded: exclusions(),
            profile: current.profile.clone(),
            files,
        },
    };
    compatible(&manifest, &current)?;
    if fingerprint(&inventory(instance)?)? != expected_fingerprint {
        return Err(RecoveryError::Changed);
    }
    store.publish(&manifest)?;
    preview_restore(instance, guard, &manifest.point.id)
}

pub fn create_point(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    description: Option<String>,
) -> Result<RecoveryPoint> {
    let _directories = check_guard(instance, guard)?;
    let mut store = Store::open(instance)?;
    reject_pending(&store)?;
    let description = description
        .unwrap_or_default()
        .trim()
        .chars()
        .take(240)
        .filter(|c| !c.is_control())
        .collect();
    Ok(capture(instance, &mut store, description, "manual")?.point)
}
pub fn point_details(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    point_id: &str,
) -> Result<RecoveryPoint> {
    let _directories = check_guard(instance, guard)?;
    let store = Store::open(instance)?;
    Ok(store.load(point_id)?.point)
}
pub fn list_points(instance: &Instance, guard: &InstanceMutationGuard) -> Result<RecoveryOverview> {
    let _directories = check_guard(instance, guard)?;
    let store = Store::open(instance)?;
    reject_pending(&store)?;
    let mut query = store.db.prepare("SELECT id,created_at FROM points WHERE instance_uuid=?1 AND kind='manual' ORDER BY created_at DESC,id")?;
    let indexed_points: Vec<(String, u64)> = query
        .query_map([&store.uuid], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let mut points = Vec::new();
    let mut unavailable_points = Vec::new();
    for (id, created_at) in indexed_points {
        match store.load(&id) {
            Ok(manifest) => points.push(manifest.point),
            Err(_) => unavailable_points.push(RecoveryUnavailablePoint {
                id,
                created_at,
                reason: "Os dados deste ponto estão ausentes, corrompidos ou não puderam ser validados. A restauração está indisponível.".into(),
            }),
        }
    }
    let undo_id: Option<String> = store
        .db
        .query_row(
            "SELECT point_id FROM undo WHERE instance_uuid=?1",
            [&store.uuid],
            |r| r.get(0),
        )
        .optional()?;
    let (undo_point, undo_unavailable) = match undo_id {
        Some(id) => match store.load(&id) {
            Ok(manifest) => (Some(manifest.point), false),
            Err(_) => (None, true),
        },
        None => (None, false),
    };
    let physical_bytes = physical_size(&path(&store.root, "blobs/sha256")?)?;
    Ok(RecoveryOverview {
        points,
        unavailable_points,
        undo_point,
        undo_unavailable,
        physical_bytes,
        scope: scope(),
        excluded: exclusions(),
    })
}
fn physical_size(root: &Path) -> Result<u64> {
    let mut n = 0u64;
    for e in fs::read_dir(root)? {
        let e = e?;
        let m = fs::symlink_metadata(e.path())?;
        if metadata_is_link(&m) {
            return Err(RecoveryError::UnsafePath);
        }
        n = n.saturating_add(if m.is_dir() {
            physical_size(&e.path())?
        } else {
            m.len()
        });
    }
    Ok(n)
}
pub fn delete_point(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    point_id: &str,
) -> Result<()> {
    let _directories = check_guard(instance, guard)?;
    valid_id(point_id)?;
    let mut store = Store::open(instance)?;
    reject_pending(&store)?;
    let tx = store.db.transaction()?;
    let active: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM undo WHERE instance_uuid=?1 AND point_id=?2)",
        params![store.uuid, point_id],
        |r| r.get(0),
    )?;
    if active || voice_receipts::pins_point(&tx, &store.uuid, point_id)? {
        return Err(RecoveryError::Pending);
    }
    if tx.execute(
        "DELETE FROM points WHERE id=?1 AND instance_uuid=?2 AND kind='manual'",
        params![point_id, store.uuid],
    )? != 1
    {
        return Err(RecoveryError::NotFound);
    }
    tx.execute("DELETE FROM previews WHERE instance_uuid=?1", [&store.uuid])?;
    tx.commit()?;
    // Retain immutable manifests/unreferenced blobs. Safe GC is a later increment.
    Ok(())
}
pub fn preview_restore(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    point_id: &str,
) -> Result<RecoveryPreview> {
    preview(instance, guard, point_id, false)
}
pub fn preview_undo(instance: &Instance, guard: &InstanceMutationGuard) -> Result<RecoveryPreview> {
    let _directories = check_guard(instance, guard)?;
    let store = Store::open(instance)?;
    let id: String = store
        .db
        .query_row(
            "SELECT point_id FROM undo WHERE instance_uuid=?1",
            [&store.uuid],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(RecoveryError::NotFound)?;
    preview(instance, guard, &id, true)
}
fn preview(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    point_id: &str,
    undo: bool,
) -> Result<RecoveryPreview> {
    let _directories = check_guard(instance, guard)?;
    let store = Store::open(instance)?;
    reject_pending(&store)?;
    let target = store.load(point_id)?;
    preflight_shape(instance, &target)?;
    let inv = inventory(instance)?;
    compatible(&target, &inv)?;
    let changes = restoration_diff(&inv.files, &target);
    let profile_changes = profile_diff(&inv.profile, &target.point.profile);
    let current_fingerprint = fingerprint(&inv)?;
    let id = Uuid::new_v4().to_string();
    let saved = SavedPreview {
        schema_version: VERSION,
        instance_uuid: store.uuid.clone(),
        point_id: point_id.into(),
        fingerprint: current_fingerprint.clone(),
        manifest_hash: digest(&json(&target)?),
        undo,
    };
    store.db.execute(
        "INSERT INTO previews VALUES(?1,?2,?3)",
        params![id, store.uuid, json(&saved)?],
    )?;
    let mut excluded = target.point.excluded.clone();
    excluded.extend(
        inv.excluded
            .keys()
            .map(|p| format!("{p} (preservado fora do escopo)")),
    );
    excluded.sort();
    excluded.dedup();
    Ok(RecoveryPreview {
        id,
        point: target.point,
        changes,
        profile_changes,
        excluded,
        current_fingerprint,
    })
}
fn compatible(target: &Manifest, inv: &Inventory) -> Result<()> {
    if target.runtime != inv.runtime {
        return Err(RecoveryError::Compatibility);
    }
    let excluded: BTreeSet<String> = inv.excluded.keys().map(|p| p.to_lowercase()).collect();
    if target
        .point
        .files
        .iter()
        .any(|f| excluded.contains(&f.path.to_lowercase()))
    {
        return Err(RecoveryError::UnsafePath);
    }
    // Changing a file into a directory or only its case needs a separate,
    // directory-aware transaction format. Reject it before confirmation.
    let current_paths: BTreeMap<String, &str> = inv
        .files
        .iter()
        .map(|f| (f.path.to_lowercase(), f.path.as_str()))
        .collect();
    for target_file in &target.point.files {
        let lower = target_file.path.to_lowercase();
        let prefix = format!("{lower}/");
        if current_paths
            .get(&lower)
            .is_some_and(|p| *p != target_file.path)
            || current_paths
                .range(prefix.clone()..)
                .next()
                .is_some_and(|(p, _)| p.starts_with(&prefix))
        {
            return Err(RecoveryError::UnsafePath);
        }
        for (index, _) in lower.match_indices('/') {
            if current_paths.contains_key(&lower[..index]) {
                return Err(RecoveryError::UnsafePath);
            }
        }
    }
    Ok(())
}
fn preflight_shape(instance: &Instance, target: &Manifest) -> Result<()> {
    for file in &target.point.files {
        if path(instance.root(), &file.path)?.is_dir() {
            return Err(RecoveryError::UnsafePath);
        }
    }
    Ok(())
}
fn diff(before: &[RecoveryFile], after: &[RecoveryFile]) -> Vec<RecoveryChange> {
    let a: BTreeMap<_, _> = before.iter().map(|f| (&f.path, f)).collect();
    let b: BTreeMap<_, _> = after.iter().map(|f| (&f.path, f)).collect();
    let paths: BTreeSet<_> = a.keys().chain(b.keys()).copied().collect();
    paths
        .into_iter()
        .filter_map(|p| {
            let action = match (a.get(p), b.get(p)) {
                (None, Some(_)) => "add",
                (Some(_), None) => "remove",
                (Some(x), Some(y)) if x.sha256 != y.sha256 => "replace",
                _ => return None,
            };
            Some(RecoveryChange {
                path: p.clone(),
                action: action.into(),
            })
        })
        .collect()
}
fn restoration_diff(before: &[RecoveryFile], target: &Manifest) -> Vec<RecoveryChange> {
    let excluded: BTreeSet<String> = target
        .excluded_paths
        .iter()
        .map(|p| p.to_lowercase())
        .collect();
    let effective: Vec<RecoveryFile> = before
        .iter()
        .filter(|f| !excluded.contains(&f.path.to_lowercase()))
        .cloned()
        .collect();
    diff(&effective, &target.point.files)
}
fn profile_diff(before: &Profile, after: &Profile) -> Vec<RecoveryProfileChange> {
    let mut result = vec![];
    for (file, fields) in after {
        for (field, value) in fields {
            let current = before
                .get(file)
                .and_then(|f| f.get(field))
                .cloned()
                .flatten();
            if &current != value {
                result.push(RecoveryProfileChange {
                    file: file.clone(),
                    field: field.clone(),
                    before: current,
                    after: value.clone(),
                });
            }
        }
    }
    result
}
fn reject_pending(store: &Store) -> Result<()> {
    if store.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM journals WHERE instance_uuid=?1)",
        [&store.uuid],
        |r| r.get::<_, bool>(0),
    )? {
        Err(RecoveryError::Pending)
    } else {
        Ok(())
    }
}
pub fn has_pending(instance: &Instance) -> Result<bool> {
    if super::content_updates::has_pending_voice_install(instance)
        .map_err(|_| RecoveryError::Pending)?
    {
        return Ok(true);
    }
    if identity(instance, false)?.is_none() {
        return Ok(false);
    }
    let store = Store::open(instance)?;
    match reject_pending(&store) {
        Ok(()) => Ok(false),
        Err(RecoveryError::Pending) => Ok(true),
        Err(e) => Err(e),
    }
}
pub fn restore(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    preview_id: &str,
) -> Result<RecoveryResult> {
    restore_impl(instance, guard, preview_id, None)
}
fn restore_impl(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    preview_id: &str,
    interrupt: Option<usize>,
) -> Result<RecoveryResult> {
    restore_attached(instance, guard, preview_id, interrupt, |_, _| {
        Ok((None, ()))
    })
    .map(|(result, ())| result)
}

#[cfg(feature = "connect-voice-lab")]
pub(crate) fn restore_voice_receipt(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    preview_id: &str,
    intent: VoiceReceiptIntent,
) -> Result<(RecoveryResult, VoiceReceiptToken)> {
    restore_attached(instance, guard, preview_id, None, |store, journal| {
        let transition = voice_receipts::prepare(store, journal, intent)?;
        let token = voice_receipts::token(store, &transition)?;
        Ok((Some(transition), token))
    })
}

fn restore_attached<T>(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    preview_id: &str,
    interrupt: Option<usize>,
    attachment: impl FnOnce(&Store, &Journal) -> Result<(Option<voice_receipts::Transition>, T)>,
) -> Result<(RecoveryResult, T)> {
    let _directories = check_guard(instance, guard)?;
    valid_id(preview_id)?;
    let mut store = Store::open(instance)?;
    reject_pending(&store)?;
    let bytes: Vec<u8> = store
        .db
        .query_row(
            "SELECT data FROM previews WHERE id=?1 AND instance_uuid=?2",
            params![preview_id, store.uuid],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(RecoveryError::StalePreview)?;
    let saved: SavedPreview = parse(&bytes)?;
    if saved.schema_version != VERSION || saved.instance_uuid != store.uuid {
        return Err(RecoveryError::Integrity);
    }
    let target = store.load(&saved.point_id)?;
    preflight_shape(instance, &target)?;
    if digest(&json(&target)?) != saved.manifest_hash {
        return Err(RecoveryError::StalePreview);
    }
    let current = inventory(instance)?;
    compatible(&target, &current)?;
    if fingerprint(&current)? != saved.fingerprint {
        return Err(RecoveryError::StalePreview);
    }
    if saved.undo {
        let current_undo: Option<String> = store
            .db
            .query_row(
                "SELECT point_id FROM undo WHERE instance_uuid=?1",
                [&store.uuid],
                |r| r.get(0),
            )
            .optional()?;
        if current_undo.as_deref() != Some(&saved.point_id) {
            return Err(RecoveryError::StalePreview);
        }
    }
    let required = current
        .files
        .iter()
        .map(|f| f.size)
        .sum::<u64>()
        .saturating_add(target.point.logical_bytes)
        .saturating_mul(2)
        .saturating_add(16 * 1024 * 1024);
    ensure_space(instance.root(), required)?;
    ensure_space(&store.root, required)?;
    let before = capture(
        instance,
        &mut store,
        "Estado anterior à restauração".into(),
        "undo",
    )?;
    if fingerprint(&inventory(instance)?)? != saved.fingerprint {
        return Err(RecoveryError::StalePreview);
    }
    let txid = Uuid::new_v4().to_string();
    let stage_rel = format!(".aurora/recovery-stage/{txid}");
    let stage = path(instance.root(), &stage_rel)?;
    fs::create_dir_all(path(instance.root(), &format!("{stage_rel}/new"))?)?;
    fs::create_dir_all(path(instance.root(), &format!("{stage_rel}/old"))?)?;
    let _stage_new_pins = pin_directory(&stage.join("new"))?;
    let _stage_old_pins = pin_directory(&stage.join("old"))?;
    let mut operations = Vec::new();
    for change in restoration_diff(&current.files, &target) {
        let old = current
            .files
            .iter()
            .find(|f| f.path == change.path)
            .cloned();
        let new = target
            .point
            .files
            .iter()
            .find(|f| f.path == change.path)
            .cloned();
        if let Some(file) = &new {
            stage_copy(
                &store.blob(&file.sha256)?,
                &stage.join("new").join(operations.len().to_string()),
                file,
            )?;
        }
        operations.push(Operation {
            path: change.path,
            before: old,
            after: new,
        });
    }
    for change in profile_diff(&current.profile, &target.point.profile) {
        let dest = path(instance.root(), &change.file)?;
        let original = if dest.exists() {
            Some(read_bounded(&dest, 1024 * 1024)?)
        } else {
            None
        };
        let mut doc: serde_json::Value = original
            .as_ref()
            .map(|b| parse(b))
            .transpose()?
            .unwrap_or_else(|| serde_json::json!({}));
        let obj = doc.as_object_mut().ok_or(RecoveryError::Integrity)?;
        match change.after {
            Some(value) => {
                obj.insert(change.field.clone(), value.into());
            }
            None => {
                obj.remove(&change.field);
            }
        }
        let bytes = json(&doc)?;
        let new = RecoveryFile {
            path: change.file.clone(),
            size: bytes.len() as u64,
            sha256: digest(&bytes),
        };
        write_new(
            &stage.join("new").join(operations.len().to_string()),
            &bytes,
        )?;
        let old = original.map(|b| RecoveryFile {
            path: change.file.clone(),
            size: b.len() as u64,
            sha256: digest(&b),
        });
        operations.push(Operation {
            path: change.file,
            before: old,
            after: Some(new),
        });
    }
    if fingerprint(&inventory(instance)?)? != saved.fingerprint {
        return Err(RecoveryError::StalePreview);
    }
    let mut journal = Journal {
        schema_version: VERSION,
        instance_uuid: store.uuid.clone(),
        transaction_id: txid,
        undo_id: before.point.id,
        point_id: target.point.id.clone(),
        operations,
        voice_receipt: None,
    };
    let (receipt_transition, committed_metadata) = attachment(&store, &journal)?;
    if let Some(transition) = receipt_transition {
        journal.schema_version = 2;
        journal.voice_receipt = Some(transition);
    }
    validate_journal(instance, &journal, &store)?;
    #[cfg(test)]
    if FAULT.with(|f| f.get() == Fault::ReceiptBeforeJournal) {
        return Err(RecoveryError::Pending);
    }
    store.db.execute(
        "INSERT INTO journals VALUES(?1,?2)",
        params![store.uuid, json(&journal)?],
    )?;
    let result = (|| -> Result<()> {
        for (index, operation) in journal.operations.iter().enumerate() {
            apply_one(instance, &stage, index, operation)?;
            if interrupt == Some(index + 1) {
                return Err(RecoveryError::Pending);
            }
        }
        let completed = inventory(instance)?;
        if !restoration_diff(&completed.files, &target).is_empty()
            || completed.profile != target.point.profile
            || completed.runtime != current.runtime
            || completed.excluded != current.excluded
        {
            return Err(RecoveryError::Changed);
        }
        for operation in &journal.operations {
            if let Some(after) = &operation.after {
                verify(&path(instance.root(), &operation.path)?, after)?;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        if matches!(error, RecoveryError::Pending) {
            return Err(error);
        }
        rollback(instance, &store, &journal)?;
        return Err(error);
    }
    #[cfg(test)]
    if FAULT.with(|f| f.get() == Fault::ReceiptBeforeCommit) {
        return Err(RecoveryError::Pending);
    }
    // Undo publication and completion are one FULL transaction. A crash before
    // this commit rolls back even when all file renames already succeeded.
    let tx = store.db.transaction()?;
    tx.execute("INSERT INTO undo VALUES(?1,?2) ON CONFLICT(instance_uuid) DO UPDATE SET point_id=excluded.point_id",params![store.uuid,journal.undo_id])?;
    if let Some(transition) = &journal.voice_receipt {
        voice_receipts::commit(&tx, &store.uuid, transition)?;
    }
    tx.execute("DELETE FROM journals WHERE instance_uuid=?1", [&store.uuid])?;
    tx.execute("DELETE FROM previews WHERE instance_uuid=?1", [&store.uuid])?;
    tx.commit()?;
    cleanup_stage(&stage, &journal);
    #[cfg(test)]
    if FAULT.with(|f| f.get() == Fault::ReceiptAfterCommit) {
        return Err(RecoveryError::Pending);
    }
    Ok((
        RecoveryResult {
            restored_point_id: target.point.id,
            undo_available: true,
            recovered_interrupted: false,
        },
        committed_metadata,
    ))
}
fn cleanup_stage(stage: &Path, journal: &Journal) {
    // Delete only enumerated verified temporary files; never recursively erase
    // a computed path. Immutable undo content remains in the CAS.
    for (index, op) in journal.operations.iter().enumerate() {
        for (directory, expected) in [("old", &op.before), ("new", &op.after)] {
            if let (Ok(p), Some(file)) = (path(stage, &format!("{directory}/{index}")), expected) {
                if p.exists() && verify(&p, file).is_ok() {
                    let _ = fs::remove_file(p);
                }
            }
        }
    }
}
pub(crate) fn stage_copy(source: &Path, dest: &Path, file: &RecoveryFile) -> Result<()> {
    #[cfg(test)]
    if FAULT.with(|f| f.get() == Fault::StageWrite) {
        return Err(RecoveryError::Io);
    }
    let mut input = secure_open(source)?;
    let _directories = pin_directory(dest.parent().ok_or(RecoveryError::UnsafePath)?)?;
    let mut out = OpenOptions::new().write(true).create_new(true).open(dest)?;
    io::copy(&mut input, &mut out)?;
    out.sync_all()?;
    drop(out);
    verify(dest, file)
}
fn validate_journal(instance: &Instance, j: &Journal, store: &Store) -> Result<()> {
    if !matches!(
        (j.schema_version, j.voice_receipt.is_some()),
        (1, false) | (2, true)
    ) || j.instance_uuid != store.uuid
        || j.operations.len() > MAX_FILES + 1
    {
        return Err(RecoveryError::Integrity);
    }
    valid_id(&j.transaction_id)?;
    valid_id(&j.undo_id)?;
    valid_id(&j.point_id)?;
    // A journal is untrusted data. Its allowlisted paths alone are insufficient:
    // bind the exact operation set and both byte inventories to verified points.
    let before = store.load(&j.undo_id)?;
    let target = store.load(&j.point_id)?;
    if before.point.kind != "undo" || before.runtime != target.runtime {
        return Err(RecoveryError::Integrity);
    }
    let expected_files = restoration_diff(&before.point.files, &target);
    let expected_profile = profile_diff(&before.point.profile, &target.point.profile);
    if j.operations.len() != expected_files.len() + expected_profile.len() {
        return Err(RecoveryError::Integrity);
    }
    let mut names = BTreeSet::new();
    for (index, op) in j.operations.iter().enumerate() {
        valid_relative(&op.path)?;
        if !(protected(&op.path) || op.path == PROFILE) || !names.insert(op.path.to_lowercase()) {
            return Err(RecoveryError::Integrity);
        }
        for f in [&op.before, &op.after].into_iter().flatten() {
            if f.path != op.path || !valid_hash(&f.sha256) || f.size > MAX_BYTES {
                return Err(RecoveryError::Integrity);
            }
        }
        if op.path == PROFILE {
            if !expected_profile
                .iter()
                .any(|change| change.file == op.path && change.field == "name")
                || op.before != before.profile_files.get(PROFILE).cloned().flatten()
            {
                return Err(RecoveryError::Integrity);
            }
            validate_profile_operation(instance, j, index, op, &target)?;
        } else if !expected_files.iter().any(|change| change.path == op.path)
            || op.before.as_ref() != before.point.files.iter().find(|file| file.path == op.path)
            || op.after.as_ref() != target.point.files.iter().find(|file| file.path == op.path)
        {
            return Err(RecoveryError::Integrity);
        }
    }
    if let Some(transition) = &j.voice_receipt {
        voice_receipts::validate(store, j, transition)?;
    }
    Ok(())
}

fn validate_profile_operation(
    instance: &Instance,
    journal: &Journal,
    index: usize,
    operation: &Operation,
    target: &Manifest,
) -> Result<()> {
    let old = path(
        instance.root(),
        &format!(
            ".aurora/recovery-stage/{}/old/{index}",
            journal.transaction_id
        ),
    )?;
    let dest = path(instance.root(), PROFILE)?;
    let mut document = if let Some(before) = &operation.before {
        if before.size > 1024 * 1024 {
            return Err(RecoveryError::Integrity);
        }
        // Before application, and after a completed rollback step, the original
        // is at dest. Between renames the original lives in the journal's old/.
        let original = if old.exists() { &old } else { &dest };
        verify(original, before)?;
        parse::<serde_json::Value>(&read_bounded(original, 1024 * 1024)?)?
    } else {
        if old.exists() {
            return Err(RecoveryError::Integrity);
        }
        serde_json::json!({})
    };
    let value = target
        .point
        .profile
        .get(PROFILE)
        .and_then(|fields| fields.get("name"))
        .ok_or(RecoveryError::Integrity)?;
    let object = document.as_object_mut().ok_or(RecoveryError::Integrity)?;
    match value {
        Some(name) => {
            object.insert("name".into(), name.clone().into());
        }
        None => {
            object.remove("name");
        }
    }
    let expected = json(&document)?;
    let after = operation.after.as_ref().ok_or(RecoveryError::Integrity)?;
    if after.size != expected.len() as u64 || after.sha256 != digest(&expected) {
        return Err(RecoveryError::Integrity);
    }
    Ok(())
}
fn apply_one(instance: &Instance, stage: &Path, index: usize, op: &Operation) -> Result<()> {
    let dest = path(instance.root(), &op.path)?;
    let old = stage.join("old").join(index.to_string());
    let new = stage.join("new").join(index.to_string());
    if let Some(before) = &op.before {
        verify(&dest, before).map_err(|_| RecoveryError::Changed)?;
    } else if dest.exists() {
        return Err(RecoveryError::Changed);
    }
    if let Some(after) = &op.after {
        verify(&new, after)?;
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let _destination_pins = pin_directory(dest.parent().ok_or(RecoveryError::UnsafePath)?)?;
    path(instance.root(), &op.path)?;
    if op.before.is_some() {
        durable_rename(&dest, &old)?;
        sync_parent(&old)?;
        #[cfg(test)]
        if FAULT.with(|f| f.get() == Fault::BackupInterrupt) {
            return Err(RecoveryError::Pending);
        }
    }
    if op.after.is_some() {
        durable_rename(&new, &dest)?;
        sync_parent(&dest)?;
    }
    Ok(())
}
fn rollback(instance: &Instance, store: &Store, j: &Journal) -> Result<()> {
    validate_journal(instance, j, store)?;
    let stage = path(
        instance.root(),
        &format!(".aurora/recovery-stage/{}", j.transaction_id),
    )?;
    let mut directories = pin_directory(&stage)?;
    directories.extend(pin_directory(&stage.join("old"))?);
    // Verify every remaining preimage and destination before the first rollback
    // mutation. A changed external file must never be overwritten to finish a
    // recovery; keep the journal and fail closed instead.
    for (index, op) in j.operations.iter().enumerate() {
        let dest = path(instance.root(), &op.path)?;
        let old = path(&stage, &format!("old/{index}"))?;
        if let Some(parent) = dest.parent() {
            if parent.exists() {
                directories.extend(pin_directory(parent)?);
            }
        }
        if old.exists() {
            verify(&old, op.before.as_ref().ok_or(RecoveryError::Integrity)?)?;
            if dest.exists() {
                verify(&dest, op.after.as_ref().ok_or(RecoveryError::Changed)?)
                    .map_err(|_| RecoveryError::Changed)?;
            }
        } else if let Some(before) = &op.before {
            verify(&dest, before).map_err(|_| RecoveryError::Changed)?;
        } else if dest.exists() {
            verify(&dest, op.after.as_ref().ok_or(RecoveryError::Integrity)?)
                .map_err(|_| RecoveryError::Changed)?;
        }
    }
    for (index, op) in j.operations.iter().enumerate().rev() {
        let dest = path(instance.root(), &op.path)?;
        let old = path(&stage, &format!("old/{index}"))?;
        if old.exists() {
            let expected = op.before.as_ref().ok_or(RecoveryError::Integrity)?;
            verify(&old, expected)?;
            if dest.exists() {
                let after = op.after.as_ref().ok_or(RecoveryError::Changed)?;
                verify(&dest, after).map_err(|_| RecoveryError::Changed)?;
                fs::remove_file(&dest)?;
            }
            durable_rename(&old, &dest)?;
            sync_parent(&dest)?;
        } else if let Some(before) = &op.before {
            verify(&dest, before).map_err(|_| RecoveryError::Changed)?;
        } else if dest.exists() {
            let after = op.after.as_ref().ok_or(RecoveryError::Integrity)?;
            verify(&dest, after).map_err(|_| RecoveryError::Changed)?;
            fs::remove_file(&dest)?;
            sync_parent(&dest)?;
        }
        #[cfg(test)]
        if FAULT.with(|f| f.get() == Fault::ReceiptRollbackInterrupt) {
            return Err(RecoveryError::Pending);
        }
    }
    store
        .db
        .execute("DELETE FROM journals WHERE instance_uuid=?1", [&store.uuid])?;
    cleanup_stage(&stage, j);
    Ok(())
}
pub fn recover_pending(instance: &Instance, guard: &InstanceMutationGuard) -> Result<bool> {
    let _directories = check_guard(instance, guard)?;
    let store = Store::open(instance)?;
    let bytes: Option<Vec<u8>> = store
        .db
        .query_row(
            "SELECT data FROM journals WHERE instance_uuid=?1",
            [&store.uuid],
            |r| r.get(0),
        )
        .optional()?;
    let regular = bytes.is_some();
    if let Some(bytes) = bytes {
        let j: Journal = parse(&bytes)?;
        rollback(instance, &store, &j)?;
    }
    drop(_directories);
    let voice = super::content_updates::recover_pending_voice_installs(instance)
        .map_err(|_| RecoveryError::Pending)?;
    Ok(regular || voice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{
        acquire_instance_mutation_lock, acquire_instance_recovery_lock, InstanceId, InstanceLayout,
    };

    struct Fixture {
        root: PathBuf,
        instance: Instance,
    }
    impl Fixture {
        fn new() -> Self {
            FAULT.with(|f| f.set(Fault::None));
            let root =
                std::env::temp_dir().join(format!("aurora-recovery-engine-{}", Uuid::new_v4()));
            let instance = InstanceLayout::new(&root).open(InstanceId::parse("qa").unwrap());
            instance.ensure_layout().unwrap();
            fs::create_dir_all(instance.root().join("config")).unwrap();
            fs::write(instance.root().join("config/test.toml"), b"distance=12").unwrap();
            fs::write(instance.root().join("options.txt"), b"gamma:1").unwrap();
            Self { root, instance }
        }
        fn write(&self, rel: &str, bytes: &[u8]) {
            fs::write(self.instance.root().join(rel), bytes).unwrap();
        }
        fn bytes(&self, rel: &str) -> Vec<u8> {
            fs::read(self.instance.root().join(rel)).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            FAULT.with(|f| f.set(Fault::None));
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn interrupted_restore_reopens_from_disk_and_blocks_ordinary_mutations() {
        let f = Fixture::new();
        let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
        let point = create_point(&f.instance, &guard, None).unwrap();
        f.write("config/test.toml", b"distance=24");
        f.write("options.txt", b"gamma:2");
        let preview = preview_restore(&f.instance, &guard, &point.id).unwrap();
        assert!(matches!(
            restore_impl(&f.instance, &guard, &preview.id, Some(1)),
            Err(RecoveryError::Pending)
        ));
        assert!(has_pending(&f.instance).unwrap());
        drop(guard);
        assert!(acquire_instance_mutation_lock(&f.instance).is_err());
        let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
        assert!(recover_pending(&f.instance, &guard).unwrap());
        assert_eq!(f.bytes("config/test.toml"), b"distance=24");
        assert_eq!(f.bytes("options.txt"), b"gamma:2");
        assert!(!recover_pending(&f.instance, &guard).unwrap());
        drop(guard);
        assert!(acquire_instance_mutation_lock(&f.instance).is_ok());
    }
    #[test]
    fn interruption_between_original_rename_and_publish_recovers() {
        let f = Fixture::new();
        let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
        let point = create_point(&f.instance, &guard, None).unwrap();
        f.write("config/test.toml", b"distance=24");
        let preview = preview_restore(&f.instance, &guard, &point.id).unwrap();
        FAULT.with(|v| v.set(Fault::BackupInterrupt));
        assert!(restore(&f.instance, &guard, &preview.id).is_err());
        FAULT.with(|v| v.set(Fault::None));
        assert!(recover_pending(&f.instance, &guard).unwrap());
        assert_eq!(f.bytes("config/test.toml"), b"distance=24");
    }
    #[test]
    fn capture_aborts_if_external_writer_changes_inventory_before_publish() {
        let f = Fixture::new();
        let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
        FAULT.with(|v| v.set(Fault::CaptureChange));
        assert!(matches!(
            create_point(&f.instance, &guard, None),
            Err(RecoveryError::Changed)
        ));
        FAULT.with(|v| v.set(Fault::None));
        assert!(list_points(&f.instance, &guard).unwrap().points.is_empty());
    }
    #[test]
    fn no_space_and_staging_write_failure_leave_protected_bytes_unchanged() {
        let f = Fixture::new();
        let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
        let point = create_point(&f.instance, &guard, None).unwrap();
        f.write("config/test.toml", b"distance=24");
        let preview = preview_restore(&f.instance, &guard, &point.id).unwrap();
        FAULT.with(|v| v.set(Fault::NoSpace));
        assert!(matches!(
            restore(&f.instance, &guard, &preview.id),
            Err(RecoveryError::NoSpace)
        ));
        assert_eq!(f.bytes("config/test.toml"), b"distance=24");
        FAULT.with(|v| v.set(Fault::StageWrite));
        assert!(matches!(
            restore(&f.instance, &guard, &preview.id),
            Err(RecoveryError::Io)
        ));
        FAULT.with(|v| v.set(Fault::None));
        assert_eq!(f.bytes("config/test.toml"), b"distance=24");
        assert!(!has_pending(&f.instance).unwrap());
    }
    #[test]
    fn missing_and_corrupt_blobs_never_reach_commit() {
        for missing in [false, true] {
            let f = Fixture::new();
            let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
            let point = create_point(&f.instance, &guard, None).unwrap();
            f.write("config/test.toml", b"distance=24");
            let preview = preview_restore(&f.instance, &guard, &point.id).unwrap();
            let store = Store::open(&f.instance).unwrap();
            let blob = store.blob(&point.files[0].sha256).unwrap();
            if missing {
                fs::remove_file(blob).unwrap();
            } else {
                fs::write(blob, b"corrupt").unwrap();
            }
            assert!(matches!(
                restore(&f.instance, &guard, &preview.id),
                Err(RecoveryError::Integrity)
            ));
            assert_eq!(f.bytes("config/test.toml"), b"distance=24");
        }
    }
    #[test]
    fn stored_objects_are_independent_copies_and_identical_points_share_bytes() {
        let f = Fixture::new();
        let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
        let a = create_point(&f.instance, &guard, None).unwrap();
        let size = list_points(&f.instance, &guard).unwrap().physical_bytes;
        let b = create_point(&f.instance, &guard, None).unwrap();
        assert_eq!(
            list_points(&f.instance, &guard).unwrap().physical_bytes,
            size
        );
        f.write("config/test.toml", b"a different live file");
        let store = Store::open(&f.instance).unwrap();
        for file in &a.files {
            verify(&store.blob(&file.sha256).unwrap(), file).unwrap();
        }
        drop(store);
        delete_point(&f.instance, &guard, &a.id).unwrap();
        assert!(point_details(&f.instance, &guard, &b.id).is_ok());
    }
    #[test]
    fn preview_binds_manifest_and_unknown_profile_fields() {
        let f = Fixture::new();
        f.write(PROFILE, br#"{"name":"old","projectId":"keep"}"#);
        let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
        let point = create_point(&f.instance, &guard, None).unwrap();
        let preview = preview_restore(&f.instance, &guard, &point.id).unwrap();
        f.write(PROFILE, br#"{"name":"old","projectId":"changed"}"#);
        assert!(matches!(
            restore(&f.instance, &guard, &preview.id),
            Err(RecoveryError::StalePreview)
        ));
        let preview = preview_restore(&f.instance, &guard, &point.id).unwrap();
        let store = Store::open(&f.instance).unwrap();
        let mut m = store.load(&point.id).unwrap();
        m.point.description = "changed after review".into();
        let bytes = json(&m).unwrap();
        fs::write(
            store.root.join(format!("manifests/{}.json", point.id)),
            &bytes,
        )
        .unwrap();
        store
            .db
            .execute(
                "UPDATE points SET manifest_hash=?1 WHERE id=?2",
                params![digest(&bytes), point.id],
            )
            .unwrap();
        drop(store);
        assert!(matches!(
            restore(&f.instance, &guard, &preview.id),
            Err(RecoveryError::StalePreview)
        ));
    }
    #[test]
    fn excluded_at_capture_stays_excluded_even_if_file_later_looks_safe() {
        let f = Fixture::new();
        f.write("config/test.toml", b"token=secret");
        let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
        let point = create_point(&f.instance, &guard, None).unwrap();
        f.write("config/test.toml", b"distance=24");
        let preview = preview_restore(&f.instance, &guard, &point.id).unwrap();
        assert!(!preview.changes.iter().any(|c| c.path == "config/test.toml"));
        restore(&f.instance, &guard, &preview.id).unwrap();
        assert_eq!(f.bytes("config/test.toml"), b"distance=24");
    }
    #[test]
    fn display_name_with_punctuation_restores_without_interpreting_it_as_path() {
        let f = Fixture::new();
        f.write(
            PROFILE,
            br#"{"name":"RPG: magic / survival","projectId":"keep"}"#,
        );
        let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
        let point = create_point(&f.instance, &guard, None).unwrap();
        f.write(PROFILE, br#"{"name":"renamed","projectId":"keep"}"#);
        let preview = preview_restore(&f.instance, &guard, &point.id).unwrap();
        restore(&f.instance, &guard, &preview.id).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&f.bytes(PROFILE)).unwrap();
        assert_eq!(value["name"], "RPG: magic / survival");
        assert_eq!(value["projectId"], "keep");
    }
}
