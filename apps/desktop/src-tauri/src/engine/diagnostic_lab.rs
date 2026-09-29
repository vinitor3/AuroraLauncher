//! Disposable diagnostic copies. This is file isolation, not an OS sandbox.
//! Only verified mod toggles and existing allowlisted JSON settings can return
//! to the source, through recovery's persisted preview/journal/undo executor.
use super::recovery::{
    self, RecoveryError, RecoveryFile, RecoveryObservation, RecoveryPreview, RecoveryResult,
};
use super::{
    acquire_instance_recovery_lock, Instance, InstanceId, InstanceLayout, InstanceMutationGuard,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

type Result<T> = std::result::Result<T, RecoveryError>;
const PREFIX: &str = "aurora-test-";
const STORAGE: &str = ".aurora/diagnostic-lab";
const MAX_FILES: usize = 150_000;
const MAX_BYTES: u64 = 48 * 1024 * 1024 * 1024;
const MAX_RECORD: u64 = 64 * 1024 * 1024;
#[cfg(test)]
#[derive(Clone, Copy, PartialEq)]
enum Fault {
    None,
    NoSpace,
    DeleteInterrupted,
    BeforeClaim,
    BeforeRecord,
    StageCorruption,
}
#[cfg(test)]
thread_local! { static FAULT: std::cell::Cell<Fault> = const { std::cell::Cell::new(Fault::None) }; }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LabEnvironment {
    pub id: String,
    pub instance_uuid: String,
    pub origin_uuid: String,
    pub origin_instance_id: String,
    pub created_at: u64,
    pub state: String,
    pub logical_bytes: u64,
    pub copied_world: Option<String>,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LabMod {
    pub path: String,
    pub enabled: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LabSetting {
    pub path: String,
    pub pointer: String,
    pub label: String,
    pub value: Value,
    pub kind: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LabChange {
    pub id: String,
    pub path: String,
    pub action: String,
    pub before: Option<String>,
    pub after: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabDetails {
    pub environment: LabEnvironment,
    pub mods: Vec<LabMod>,
    pub settings: Vec<LabSetting>,
    pub changes: Vec<LabChange>,
    pub baseline_fingerprint: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabProgress {
    pub phase: String,
    pub completed_bytes: u64,
    pub total_bytes: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    version: u32,
    environment: LabEnvironment,
    baseline: RecoveryObservation,
    filesystem_identity: String,
    baseline_settings: Vec<LabSetting>,
    copy_files: Vec<RecoveryFile>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplyBinding {
    version: u32,
    preview_id: String,
    source_uuid: String,
    test_uuid: String,
    test_fingerprint: String,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn encode<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(v).map_err(|_| RecoveryError::Integrity)
}
fn read<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T> {
    serde_json::from_slice(&recovery::read_bounded(path, MAX_RECORD)?)
        .map_err(|_| RecoveryError::Integrity)
}
fn data(instance: &Instance) -> Result<&Path> {
    instance
        .root()
        .parent()
        .and_then(Path::parent)
        .ok_or(RecoveryError::UnsafePath)
}
fn uuid_from_id(id: &str) -> Result<&str> {
    let uuid = id.strip_prefix(PREFIX).ok_or(RecoveryError::Identity)?;
    recovery::valid_id(uuid)?;
    Ok(uuid)
}
fn directory(data: &Path, id: &str) -> Result<PathBuf> {
    let uuid = uuid_from_id(id)?;
    recovery::path(data, &format!("{STORAGE}/{uuid}"))
}
fn marker(data: &Path, id: &str, name: &str) -> Result<PathBuf> {
    recovery::path(&directory(data, id)?, name)
}
pub fn is_lab_candidate(instance: &Instance) -> bool {
    instance.id().as_str().starts_with(PREFIX)
}
fn record(instance: &Instance) -> Result<Record> {
    let rec: Record = read(&marker(
        data(instance)?,
        instance.id().as_str(),
        "record.json",
    )?)?;
    if rec.version != 1
        || rec.environment.id != instance.id().as_str()
        || rec.environment.origin_uuid != rec.baseline.instance_uuid
        || rec.copy_files.len() > MAX_FILES
    {
        return Err(RecoveryError::Integrity);
    }
    recovery::valid_id(&rec.environment.instance_uuid)?;
    recovery::valid_id(&rec.environment.origin_uuid)?;
    if rec.environment.instance_uuid == rec.environment.origin_uuid {
        return Err(RecoveryError::Identity);
    }
    if super::ai_tools::instance_identity(instance).map_err(|_| RecoveryError::Identity)?
        != rec.filesystem_identity
    {
        return Err(RecoveryError::Identity);
    }
    let deleting = marker(data(instance)?, instance.id().as_str(), "deleting.json")?.exists();
    let identity = recovery::identity_lookup(instance)?;
    if identity.as_deref() != Some(rec.environment.instance_uuid.as_str())
        && !(deleting && identity.is_none())
    {
        return Err(RecoveryError::Identity);
    }
    Ok(rec)
}
pub fn baseline_observation(test: &Instance) -> Result<RecoveryObservation> {
    ensure_launchable(test)?;
    Ok(record(test)?.baseline)
}

pub fn ensure_launchable(instance: &Instance) -> Result<()> {
    if !is_lab_candidate(instance) {
        return Ok(());
    }
    let rec = record(instance)?;
    let dir = directory(data(instance)?, instance.id().as_str())?;
    if dir.join("deleting.json").exists() || !dir.join("ready.json").exists() {
        return Err(RecoveryError::Pending);
    }
    let ready: String = read(&recovery::path(&dir, "ready.json")?)?;
    if ready != rec.environment.instance_uuid {
        return Err(RecoveryError::Identity);
    }
    validate_launch_metadata(instance)?;
    Ok(())
}
fn validate_launch_metadata(instance: &Instance) -> Result<()> {
    fn walk(root: &Path, rel: &str, depth: usize, count: &mut usize) -> Result<()> {
        if depth > 8 || *count > 4096 {
            return Err(RecoveryError::Limit);
        }
        let directory = recovery::path(root, rel)?;
        if !directory.exists() {
            return Ok(());
        }
        let _pins = recovery::pin_directory(&directory)?;
        for entry in fs::read_dir(directory)? {
            let name = entry?
                .file_name()
                .into_string()
                .map_err(|_| RecoveryError::UnsafePath)?;
            let child = format!("{rel}/{name}");
            let p = recovery::path(root, &child)?;
            if p.is_dir() {
                walk(root, &child, depth + 1, count)?;
            } else if child.to_ascii_lowercase().ends_with(".json") {
                *count += 1;
                validate_runtime_json(&recovery::read_bounded(&p, 16 * 1024 * 1024)?)?;
            }
        }
        Ok(())
    }
    walk(instance.root(), "versions", 0, &mut 0)?;
    for filename in ["aurora-instance.json", "aurora-modpack.json"] {
        let p = recovery::path(instance.root(), filename)?;
        if p.exists() {
            validate_runtime_json(&recovery::read_bounded(&p, 1024 * 1024)?)?;
        }
    }
    Ok(())
}
fn environment(mut rec: Record, dir: &Path, instance: Option<&Instance>) -> Result<LabEnvironment> {
    rec.environment.state = if dir.join("deleting.json").exists() {
        "deleting"
    } else if dir.join("ready.json").exists() {
        "ready"
    } else {
        "failed"
    }
    .into();
    if let Some(instance) = instance {
        if instance.root().exists() {
            let mut files = Vec::new();
            scan_tree(instance.root(), "", &mut files, 0, false)?;
            rec.environment.logical_bytes = files.iter().map(|f| f.size).sum();
        }
    }
    Ok(rec.environment)
}
pub fn list(data: &Path) -> Result<Vec<LabEnvironment>> {
    let root = recovery::path(data, STORAGE)?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let uuid = entry
            .file_name()
            .into_string()
            .map_err(|_| RecoveryError::UnsafePath)?;
        if recovery::valid_id(&uuid).is_err() {
            continue;
        }
        let dir = recovery::path(&root, &uuid)?;
        if recovery::path(&dir, "deleted.json")?.exists() {
            continue;
        }
        let id = format!("{PREFIX}{uuid}");
        let loaded = (|| -> Result<LabEnvironment> {
            let rec: Record = read(&recovery::path(&dir, "record.json")?)?;
            if rec.version != 1 || rec.environment.id != id {
                return Err(RecoveryError::Integrity);
            }
            let mut env = environment(rec, &dir, None)?;
            if env.state == "ready" {
                let test = InstanceLayout::new(data)
                    .open(InstanceId::parse(&id).map_err(|_| RecoveryError::Identity)?);
                if ensure_launchable(&test).is_err() {
                    env.state = "failed".into();
                    env.limitations.push("A identidade ou os metadados da cópia não puderam ser validados. Iniciar e aplicar estão bloqueados.".into());
                }
            }
            Ok(env)
        })();
        let env = match loaded {
            Ok(env) => env,
            Err(_) => {
                let mut env = read::<LabEnvironment>(&recovery::path(&dir, "intent.json")?)
                    .unwrap_or_else(|_| LabEnvironment {
                        id: id.clone(),
                        instance_uuid: String::new(),
                        origin_uuid: String::new(),
                        origin_instance_id: String::new(),
                        created_at: 0,
                        state: "failed".into(),
                        logical_bytes: 0,
                        copied_world: None,
                        limitations: Vec::new(),
                    });
                if env.id != id {
                    env.id = id;
                    env.origin_uuid.clear();
                    env.origin_instance_id.clear();
                }
                env.state = "failed".into();
                env.limitations.push("Criação interrompida ou registro indisponível. Iniciar e aplicar estão bloqueados; a limpeza exige identidade verificável.".into());
                env
            }
        };
        result.push(env);
    }
    result.sort_by_key(|entry| std::cmp::Reverse(entry.created_at));
    Ok(result)
}

pub fn origin_instance(data: &Path, test: &Instance) -> Result<Instance> {
    let rec = record(test)?;
    let layout = InstanceLayout::new(data);
    for entry in fs::read_dir(layout.instances_dir())? {
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| RecoveryError::UnsafePath)?;
        let Ok(id) = InstanceId::parse(name) else {
            continue;
        };
        let candidate = layout.open(id);
        if is_lab_candidate(&candidate) {
            continue;
        }
        if recovery::identity_lookup(&candidate)
            .ok()
            .flatten()
            .as_deref()
            == Some(rec.environment.origin_uuid.as_str())
        {
            return Ok(candidate);
        }
    }
    Err(RecoveryError::Identity)
}
pub fn worlds(source: &Instance, guard: &InstanceMutationGuard) -> Result<Vec<String>> {
    let _pins = recovery::check_guard(source, guard)?;
    let root = recovery::path(source.root(), "saves")?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(root)? {
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| RecoveryError::UnsafePath)?;
        recovery::valid_relative(&name)?;
        if name.contains('/') {
            return Err(RecoveryError::UnsafePath);
        }
        let candidate = recovery::path(source.root(), &format!("saves/{name}"))?;
        if candidate.is_dir() && recovery::path(&candidate, "level.dat")?.is_file() {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

// Files are read with the same no-link/no-hardlink handles as recovery. Paths
// are inventoried twice and each destination is verified after an independent
// byte copy. No runtime/cache tree outside this explicit allowlist is read.
fn scan_tree(
    root: &Path,
    rel: &str,
    out: &mut Vec<RecoveryFile>,
    depth: usize,
    screen_json: bool,
) -> Result<()> {
    if depth > 40 || out.len() > MAX_FILES {
        return Err(RecoveryError::Limit);
    }
    let p = if rel.is_empty() {
        root.to_owned()
    } else {
        recovery::path(root, rel)?
    };
    if !p.exists() {
        return Ok(());
    }
    let _pins = recovery::pin_directory(&p)?;
    for entry in fs::read_dir(&p)? {
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| RecoveryError::UnsafePath)?;
        let child = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        let path = recovery::path(root, &child)?;
        if path.is_dir() {
            scan_tree(root, &child, out, depth + 1, screen_json)?;
        } else {
            let (sha256, size) = recovery::hash_file(&path)?;
            if screen_json && child.to_ascii_lowercase().ends_with(".json") {
                validate_runtime_json(&recovery::read_bounded(&path, 16 * 1024 * 1024)?)?;
            }
            out.push(RecoveryFile {
                path: child,
                size,
                sha256,
            });
            if out.len() > MAX_FILES {
                return Err(RecoveryError::Limit);
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    if out.iter().map(|f| f.size).sum::<u64>() > MAX_BYTES {
        return Err(RecoveryError::Limit);
    }
    Ok(())
}
// Only installed profiles with confined file references can be copied or
// launched as a diagnostic instance. Normal URLs are metadata, never game dirs.
fn validate_runtime_paths(value: &Value) -> Result<()> {
    fn id(value: &str) -> Result<()> {
        if value.is_empty()
            || value == "."
            || value == ".."
            || value.len() > 200
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-+".contains(&b))
        {
            return Err(RecoveryError::UnsafePath);
        }
        Ok(())
    }
    fn maven(value: &str) -> Result<()> {
        let (coordinate, extension) = value.split_once('@').unwrap_or((value, "jar"));
        if !extension.bytes().all(|b| b.is_ascii_alphanumeric()) || extension.is_empty() {
            return Err(RecoveryError::UnsafePath);
        }
        let parts: Vec<_> = coordinate.split(':').collect();
        if !(3..=4).contains(&parts.len()) {
            return Err(RecoveryError::UnsafePath);
        }
        for part in &parts {
            id(part)?;
        }
        if parts[0].split('.').any(str::is_empty) {
            return Err(RecoveryError::UnsafePath);
        }
        Ok(())
    }
    fn argument(value: &str) -> Result<()> {
        let lower = value.to_ascii_lowercase();
        if lower.contains("file:")
            || value.contains('\\')
            || value.contains("../")
            || value.split(['/', '=', ';']).any(|part| part == "..")
            || value.contains("..\\")
            || value.starts_with('/')
            || value.starts_with('@')
        {
            return Err(RecoveryError::UnsafePath);
        }
        if value
            .strip_prefix("-XX:")
            .unwrap_or(value)
            .as_bytes()
            .windows(2)
            .any(|p| p[0].is_ascii_alphabetic() && p[1] == b':')
        {
            // JVM properties with a drive-qualified path, URI or agent path
            // would bypass the independent game directory.
            return Err(RecoveryError::UnsafePath);
        }
        if let Some((flag, path)) = value.split_once('=') {
            if path.starts_with('/') || path.starts_with('~') {
                return Err(RecoveryError::UnsafePath);
            }
            let expected = match flag.to_ascii_lowercase().as_str() {
                "--gamedir" => Some("${game_directory}"),
                "--assetsdir" => Some("${assets_root}"),
                _ => None,
            };
            if expected.is_some_and(|expected| path != expected) {
                return Err(RecoveryError::UnsafePath);
            }
        }
        Ok(())
    }
    fn argument_list(values: &[Value]) -> Result<()> {
        let mut words = Vec::new();
        fn collect(value: &Value, out: &mut Vec<String>) {
            match value {
                Value::String(s) => out.extend(s.split_whitespace().map(str::to_owned)),
                Value::Array(values) => {
                    for v in values {
                        collect(v, out)
                    }
                }
                Value::Object(map) => {
                    if let Some(value) = map.get("value") {
                        collect(value, out)
                    }
                }
                _ => {}
            }
        }
        for value in values {
            collect(value, &mut words);
        }
        for (index, word) in words.iter().enumerate() {
            argument(word)?;
            let expected = match word.to_ascii_lowercase().as_str() {
                "--gamedir" => Some("${game_directory}"),
                "--assetsdir" => Some("${assets_root}"),
                _ => None,
            };
            if expected
                .is_some_and(|expected| words.get(index + 1).is_none_or(|value| value != expected))
            {
                return Err(RecoveryError::UnsafePath);
            }
        }
        Ok(())
    }
    if let Some(map) = value.as_object() {
        for field in ["inheritsFrom", "jar", "versionId"] {
            if let Some(value) = map.get(field).and_then(Value::as_str) {
                id(value)?;
            }
        }
        let profile = map.contains_key("mainClass")
            || map.contains_key("libraries")
            || map.contains_key("inheritsFrom");
        if profile {
            if let Some(value) = map.get("id").and_then(Value::as_str) {
                id(value)?;
            }
            if let Some(value) = value.pointer("/assetIndex/id").and_then(Value::as_str) {
                id(value)?;
            }
            if let Some(libraries) = value.get("libraries").and_then(Value::as_array) {
                for library in libraries {
                    if let Some(name) = library.get("name").and_then(Value::as_str) {
                        maven(name)?;
                    }
                }
            }
        }
        for field in ["game", "jvm"] {
            if let Some(values) = value
                .pointer(&format!("/arguments/{field}"))
                .and_then(Value::as_array)
            {
                argument_list(values)?;
            }
        }
        if let Some(value) = map.get("minecraftArguments") {
            argument_list(std::slice::from_ref(value))?;
        }
        if let Some(value) = map.get("argument") {
            argument_list(std::slice::from_ref(value))?;
        }
        for (key, value) in map {
            if key == "path" {
                if let Some(path) = value.as_str() {
                    recovery::valid_relative(path)?;
                }
            }
            validate_runtime_paths(value)?;
        }
    } else if let Some(values) = value.as_array() {
        for value in values {
            validate_runtime_paths(value)?;
        }
    }
    Ok(())
}

fn validate_runtime_json(bytes: &[u8]) -> Result<()> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| RecoveryError::Integrity)?;
    fn auth_flag(value: &str) -> bool {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "--accesstoken"
                | "--session"
                | "--clientid"
                | "--xuid"
                | "--uuid"
                | "--username"
                | "--usertype"
                | "--userproperties"
                | "--profileproperties"
        )
    }
    fn placeholder(value: &str) -> bool {
        matches!(
            value,
            "${auth_access_token}"
                | "${auth_session}"
                | "${clientid}"
                | "${auth_xuid}"
                | "${auth_uuid}"
                | "${auth_player_name}"
                | "${user_type}"
                | "${user_properties}"
                | "${profile_properties}"
        )
    }
    fn credential_key(value: &str) -> bool {
        let key = value.to_ascii_lowercase().replace(['_', '-'], "");
        [
            "token",
            "password",
            "credential",
            "secret",
            "session",
            "account",
            "authentication",
            "apikey",
            "authorization",
        ]
        .iter()
        .any(|part| key.contains(part))
    }
    fn check_string(value: &str) -> Result<()> {
        let lower = value.to_ascii_lowercase();
        if lower.contains("bearer ") || lower.contains("authorization:") || lower.contains("eyjhb")
        {
            return Err(RecoveryError::UnsafePath);
        }
        let words: Vec<_> = value.split_whitespace().collect();
        for (index, word) in words.iter().enumerate() {
            if let Some((flag, parameter)) = word.split_once('=') {
                if auth_flag(flag) && !placeholder(parameter) {
                    return Err(RecoveryError::UnsafePath);
                }
                if flag.to_ascii_lowercase().starts_with("-d")
                    && credential_key(flag)
                    && !placeholder(parameter)
                {
                    return Err(RecoveryError::UnsafePath);
                }
            }
            if auth_flag(word) && words.get(index + 1).is_some_and(|next| !placeholder(next)) {
                return Err(RecoveryError::UnsafePath);
            }
            if let Ok(url) = reqwest::Url::parse(word) {
                if !url.username().is_empty()
                    || url.password().is_some()
                    || url.query_pairs().any(|(key, _)| credential_key(&key))
                {
                    return Err(RecoveryError::UnsafePath);
                }
            }
        }
        Ok(())
    }
    fn walk(value: &Value, depth: usize) -> Result<()> {
        if depth > 48 {
            return Err(RecoveryError::Limit);
        }
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    if credential_key(key) {
                        return Err(RecoveryError::UnsafePath);
                    }
                    walk(value, depth + 1)?;
                }
            }
            Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    if value.as_str().is_some_and(auth_flag)
                        && !values
                            .get(index + 1)
                            .and_then(Value::as_str)
                            .is_some_and(placeholder)
                    {
                        return Err(RecoveryError::UnsafePath);
                    }
                    walk(value, depth + 1)?;
                }
            }
            Value::String(value) => check_string(value)?,
            _ => {}
        }
        Ok(())
    }
    validate_runtime_paths(&value)?;
    walk(&value, 0)
}

fn copied_files(
    source: &Instance,
    baseline: &RecoveryObservation,
    selected_world: Option<&str>,
) -> Result<Vec<RecoveryFile>> {
    let mut files = baseline.files.clone();
    for root in [
        "versions",
        "libraries",
        "assets",
        "natives",
        "resourcepacks",
        "shaderpacks",
    ] {
        scan_tree(source.root(), root, &mut files, 0, true)?;
    }
    if let Some(world) = selected_world {
        recovery::valid_relative(world)?;
        if world.contains('/')
            || !recovery::path(source.root(), &format!("saves/{world}/level.dat"))?.is_file()
        {
            return Err(RecoveryError::UnsafePath);
        }
        scan_tree(
            source.root(),
            &format!("saves/{world}"),
            &mut files,
            0,
            false,
        )?;
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut seen = BTreeSet::new();
    for file in &files {
        if !seen.insert(file.path.to_lowercase()) {
            return Err(RecoveryError::UnsafePath);
        }
    }
    Ok(files)
}
pub fn create(
    source: &Instance,
    source_guard: &InstanceMutationGuard,
    selected_world: Option<&str>,
    mut progress: impl FnMut(LabProgress),
) -> Result<LabEnvironment> {
    let mut source_pins = recovery::check_guard(source, source_guard)?;
    for root in [
        "versions",
        "libraries",
        "assets",
        "natives",
        "resourcepacks",
        "shaderpacks",
    ] {
        let p = recovery::path(source.root(), root)?;
        if p.exists() {
            recovery::pin_tree(&p, &mut source_pins, 0)?;
        }
    }
    if let Some(world) = selected_world {
        recovery::valid_relative(world)?;
        if world.contains('/') {
            return Err(RecoveryError::UnsafePath);
        }
        let p = recovery::path(source.root(), &format!("saves/{world}"))?;
        recovery::pin_tree(&p, &mut source_pins, 0)?;
    }
    if is_lab_candidate(source) {
        return Err(RecoveryError::Identity);
    }
    let baseline = recovery::capture_observation(source, source_guard)?;
    progress(LabProgress {
        phase: "Verificando arquivos e origem".into(),
        completed_bytes: 0,
        total_bytes: 0,
    });
    let copy_files = copied_files(source, &baseline, selected_world)?;
    let total = copy_files.iter().map(|f| f.size).sum::<u64>();
    #[cfg(test)]
    if FAULT.with(|f| f.get() == Fault::NoSpace) {
        return Err(RecoveryError::NoSpace);
    }
    recovery::ensure_space(data(source)?, total.saturating_add(64 * 1024 * 1024))?;
    let id = format!("{PREFIX}{}", Uuid::new_v4());
    let test = InstanceLayout::new(data(source)?)
        .open(InstanceId::parse(&id).map_err(|_| RecoveryError::Identity)?);
    let destination = recovery::path(data(source)?, &format!("instances/{id}"))?;
    let mut env=LabEnvironment{id:id.clone(),instance_uuid:Uuid::new_v4().to_string(),origin_uuid:baseline.instance_uuid.clone(),origin_instance_id:source.id().as_str().into(),created_at:now(),state:"creating".into(),logical_bytes:total,copied_world:selected_world.map(str::to_owned),limitations:vec!["Cópia de teste com arquivos independentes; não é isolamento de segurança do sistema operacional.".into(),"Credenciais, sessões, históricos e logs da origem não são copiados. Use a autenticação normal do launcher.".into(),if selected_world.is_some(){"Somente o mundo escolhido foi copiado de forma independente; ele não pode ser aplicado à origem."}else{"Mundos da origem não foram copiados. Crie mundos descartáveis no jogo."}.into(),"Uma execução sem o erro anterior é uma observação, não uma garantia de solução.".into(),"Só alternâncias de mods sem alteração do JAR e campos de configuração suportados aparecem na lista aplicável. Mundos e outras mudanças nunca são transferidos.".into(),"Excluir a cópia libera a pasta de jogo; o armazenamento compartilhado de recuperação mantém objetos imutáveis conforme a política atual.".into()]};
    let dir = directory(data(source)?, &id)?;
    fs::create_dir_all(&dir)?;
    let _registry_pins = recovery::pin_directory(&dir)?;
    recovery::write_new(&recovery::path(&dir, "intent.json")?, &encode(&env)?)?;
    fs::create_dir(&destination)?;
    #[cfg(test)]
    if FAULT.with(|f| f.get() == Fault::BeforeClaim) {
        return Err(RecoveryError::Pending);
    }
    let mut destination_pins = recovery::pin_directory(&destination)?;
    recovery::write_new(
        &recovery::path(&dir, "claim.json")?,
        &encode(&recovery::filesystem_identity(&test)?)?,
    )?;
    let test_guard = acquire_instance_recovery_lock(&test).map_err(|_| RecoveryError::WrongLock)?;
    env.instance_uuid = recovery::capture_observation(&test, &test_guard)?.instance_uuid;
    let rec = Record {
        version: 1,
        filesystem_identity: super::ai_tools::instance_identity(&test)
            .map_err(|_| RecoveryError::Identity)?,
        environment: env.clone(),
        baseline_settings: settings(source, &baseline)?,
        baseline,
        copy_files,
    };
    #[cfg(test)]
    if FAULT.with(|f| f.get() == Fault::BeforeRecord) {
        return Err(RecoveryError::Pending);
    }
    recovery::write_new(&recovery::path(&dir, "record.json")?, &encode(&rec)?)?;
    let mut completed = 0;
    let mut pinned_parents = BTreeSet::new();
    for file in &rec.copy_files {
        let dest = recovery::path(test.root(), &file.path)?;
        fs::create_dir_all(dest.parent().ok_or(RecoveryError::UnsafePath)?)?;
        let parent = dest.parent().ok_or(RecoveryError::UnsafePath)?;
        if pinned_parents.insert(parent.to_owned()) {
            destination_pins.extend(recovery::pin_directory(parent)?);
        }
        recovery::stage_copy(&recovery::path(source.root(), &file.path)?, &dest, file)?;
        completed += file.size;
        progress(LabProgress {
            phase: "Copiando arquivos independentes".into(),
            completed_bytes: completed,
            total_bytes: total,
        });
    }
    // Reconstruct the launcher profile from verified harmless fields only.
    for filename in ["aurora-instance.json", "aurora-modpack.json"] {
        let mut object = serde_json::Map::new();
        for field in ["minecraft", "versionId", "loader", "source"] {
            if let Some(v) = rec.baseline.runtime.get(&format!("{filename}:{field}")) {
                object.insert(field.into(), v.clone().into());
            }
        }
        if filename == "aurora-modpack.json" {
            if let Some(Some(name)) = rec
                .baseline
                .profile
                .get(filename)
                .and_then(|p| p.get("name"))
            {
                object.insert("name".into(), name.clone().into());
            }
        }
        if !object.is_empty() {
            recovery::write_new(&recovery::path(test.root(), filename)?, &encode(&object)?)?;
        }
    }
    if recovery::capture_observation(source, source_guard)?.fingerprint != rec.baseline.fingerprint
        || copied_files(source, &rec.baseline, selected_world)? != rec.copy_files
    {
        return Err(RecoveryError::Changed);
    }
    for file in &rec.copy_files {
        recovery::verify(&recovery::path(test.root(), &file.path)?, file)?;
    }
    let ready_observation = recovery::capture_observation(&test, &test_guard)?;
    if copied_files(&test, &ready_observation, selected_world)? != rec.copy_files {
        return Err(RecoveryError::Changed);
    }
    if ready_observation.runtime != rec.baseline.runtime
        || ready_observation.files != rec.baseline.files
        || ready_observation.profile != rec.baseline.profile
    {
        return Err(RecoveryError::Changed);
    }
    recovery::write_new(
        &recovery::path(&dir, "ready.json")?,
        &encode(&env.instance_uuid)?,
    )?;
    progress(LabProgress {
        phase: "Cópia de teste pronta".into(),
        completed_bytes: total,
        total_bytes: total,
    });
    env.state = "ready".into();
    Ok(env)
}

fn setting_key(key: &str) -> Option<(&'static str, Option<(f64, f64)>)> {
    match key.to_ascii_lowercase().replace(['_', '-'], "").as_str() {
        "renderdistance" => Some(("Distância de renderização", Some((2., 32.)))),
        "maxfps" | "frameratelimit" => Some(("Limite de FPS", Some((30., 260.)))),
        "entitydistancescaling" => Some(("Distância de entidades", Some((0.5, 5.)))),
        "enablefog" | "fog" => Some(("Névoa", None)),
        "fancygraphics" => Some(("Gráficos detalhados", None)),
        "showfps" => Some(("Exibir FPS", None)),
        "entityculling" | "enableentityculling" => Some(("Ocultar entidades fora de visão", None)),
        "blockentityculling" => Some(("Ocultar blocos fora de visão", None)),
        _ => None,
    }
}
fn collect_settings(
    value: &Value,
    path: &str,
    pointer: &str,
    out: &mut Vec<LabSetting>,
    depth: usize,
) {
    if depth > 3 || out.len() >= 64 {
        return;
    }
    if let Some(map) = value.as_object() {
        for (key, value) in map.iter().take(128) {
            if key.contains(['/', '~'])
                || [
                    "auth",
                    "token",
                    "secret",
                    "password",
                    "security",
                    "permission",
                    "account",
                    "session",
                    "api",
                    "server",
                    "network",
                    "address",
                    "url",
                    "key",
                    "webhook",
                    "aurora",
                    "privacy",
                    "telemetry",
                    "ssl",
                    "tls",
                    "cert",
                ]
                .iter()
                .any(|s| key.to_ascii_lowercase().contains(s))
            {
                continue;
            }
            let next = format!("{pointer}/{key}");
            if value.is_object() {
                collect_settings(value, path, &next, out, depth + 1);
                continue;
            }
            if let Some((label, range)) = setting_key(key) {
                let valid = match range {
                    Some((min, max)) => value
                        .as_f64()
                        .is_some_and(|v| v.is_finite() && (min..=max).contains(&v)),
                    None => value.is_boolean(),
                };
                if valid {
                    out.push(LabSetting {
                        path: path.into(),
                        pointer: next,
                        label: label.into(),
                        value: value.clone(),
                        kind: if range.is_some() { "number" } else { "boolean" }.into(),
                        min: range.map(|r| r.0),
                        max: range.map(|r| r.1),
                    });
                }
            }
        }
    }
}
fn read_verified_document(
    instance: &Instance,
    observation: &RecoveryObservation,
    path: &str,
) -> Result<Value> {
    let file = observation
        .files
        .iter()
        .find(|f| f.path == path)
        .ok_or(RecoveryError::Integrity)?;
    let bytes = recovery::read_bounded(&recovery::path(instance.root(), path)?, 256 * 1024)?;
    if file.size != bytes.len() as u64 || file.sha256 != format!("{:x}", Sha256::digest(&bytes)) {
        return Err(RecoveryError::Changed);
    }
    serde_json::from_slice(&bytes).map_err(|_| RecoveryError::Integrity)
}

fn settings(instance: &Instance, observation: &RecoveryObservation) -> Result<Vec<LabSetting>> {
    let mut out = Vec::new();
    for file in &observation.files {
        if file.path.starts_with("config/")
            && file.path.ends_with(".json")
            && file.size <= 256 * 1024
        {
            match read_verified_document(instance, observation, &file.path) {
                Ok(value) => collect_settings(&value, &file.path, "", &mut out, 0),
                Err(RecoveryError::Integrity) => {}
                Err(error) => return Err(error),
            }
        }
    }
    Ok(out)
}
fn mod_base(path: &str) -> Option<&str> {
    if !path.starts_with("mods/") {
        return None;
    }
    if path.ends_with(".jar") {
        Some(path)
    } else {
        path.strip_suffix(".disabled")
            .or_else(|| path.strip_suffix(".off"))
            .filter(|p| p.ends_with(".jar"))
    }
}
fn grouped_mods(files: &[RecoveryFile]) -> Result<BTreeMap<String, &RecoveryFile>> {
    let mut out = BTreeMap::new();
    for file in files {
        if let Some(base) = mod_base(&file.path) {
            if out.insert(base.to_owned(), file).is_some() {
                return Err(RecoveryError::Integrity);
            }
        }
    }
    Ok(out)
}
fn changes(
    rec: &Record,
    current: &RecoveryObservation,
    current_settings: &[LabSetting],
) -> Result<Vec<LabChange>> {
    let before = grouped_mods(&rec.baseline.files)?;
    let after = grouped_mods(&current.files)?;
    let mut result = Vec::new();
    for (base, old) in before {
        if let Some(new) = after.get(&base) {
            if old.path != new.path && old.sha256 == new.sha256 {
                result.push(LabChange {
                    id: format!("mod:{base}"),
                    path: base,
                    action: "toggle_mod".into(),
                    before: Some(
                        if old.path.ends_with(".jar") {
                            "Ativado"
                        } else {
                            "Desativado"
                        }
                        .into(),
                    ),
                    after: Some(
                        if new.path.ends_with(".jar") {
                            "Ativado"
                        } else {
                            "Desativado"
                        }
                        .into(),
                    ),
                });
            }
        }
    }
    for old in &rec.baseline_settings {
        if let Some(new) = current_settings
            .iter()
            .find(|s| s.path == old.path && s.pointer == old.pointer)
        {
            if old.value != new.value {
                result.push(LabChange {
                    id: format!("setting:{}#{}", old.path, old.pointer),
                    path: old.path.clone(),
                    action: "set_setting".into(),
                    before: Some(old.value.to_string()),
                    after: Some(new.value.to_string()),
                });
            }
        }
    }
    Ok(result)
}
pub fn inspect(test: &Instance, guard: &InstanceMutationGuard) -> Result<LabDetails> {
    ensure_launchable(test)?;
    let rec = record(test)?;
    let current = recovery::capture_observation(test, guard)?;
    let settings = settings(test, &current)?;
    let changes = changes(&rec, &current, &settings)?;
    let mods = current
        .files
        .iter()
        .filter(|f| mod_base(&f.path).is_some())
        .map(|f| LabMod {
            path: f.path.clone(),
            enabled: f.path.ends_with(".jar"),
        })
        .collect();
    let baseline_fingerprint = rec.baseline.fingerprint.clone();
    let environment = environment(
        rec,
        &directory(data(test)?, test.id().as_str())?,
        Some(test),
    )?;
    Ok(LabDetails {
        environment,
        mods,
        settings,
        changes,
        baseline_fingerprint,
    })
}
pub fn toggle_mod(
    test: &Instance,
    guard: &InstanceMutationGuard,
    path: &str,
    enabled: bool,
) -> Result<LabDetails> {
    ensure_launchable(test)?;
    let current = recovery::capture_observation(test, guard)?;
    let file = current
        .files
        .iter()
        .find(|f| f.path == path)
        .ok_or(RecoveryError::UnsafePath)?;
    let base = mod_base(path).ok_or(RecoveryError::UnsafePath)?;
    let destination = if enabled {
        base.to_owned()
    } else {
        format!("{base}.disabled")
    };
    if destination == path {
        return inspect(test, guard);
    }
    if recovery::path(test.root(), &destination)?.exists() {
        return Err(RecoveryError::Changed);
    }
    let replacements = BTreeMap::from([
        (path.to_owned(), None),
        (
            destination,
            Some(recovery::SelectedFile {
                source: recovery::path(test.root(), &file.path)?,
                sha256: file.sha256.clone(),
                size: file.size,
            }),
        ),
    ]);
    let preview = recovery::preview_selected_files(
        test,
        guard,
        &current.fingerprint,
        &replacements,
        "Alternância de mod na cópia de teste",
    )?;
    recovery::restore(test, guard, &preview.id)?;
    inspect(test, guard)
}
fn setting_value_valid(setting: &LabSetting, value: &Value) -> bool {
    if setting.kind == "boolean" {
        value.is_boolean()
    } else {
        value.as_f64().is_some_and(|v| {
            v.is_finite() && v >= setting.min.unwrap_or(0.) && v <= setting.max.unwrap_or(0.)
        }) && (!(setting.value.is_i64() || setting.value.is_u64())
            || value.is_i64()
            || value.is_u64())
    }
}
fn stage_bytes(instance: &Instance, bytes: &[u8]) -> Result<recovery::SelectedFile> {
    let dir = recovery::path(instance.root(), ".aurora/lab-stage")?;
    fs::create_dir_all(&dir)?;
    let _pins = recovery::pin_directory(&dir)?;
    let p = recovery::path(&dir, &format!("{}.json", Uuid::new_v4()))?;
    recovery::write_new(&p, bytes)?;
    #[cfg(test)]
    if FAULT.with(|f| f.get() == Fault::StageCorruption) {
        fs::write(&p, br#"{"renderDistance":8,"unsupported":99}"#)?;
    }
    Ok(recovery::SelectedFile {
        source: p,
        sha256: format!("{:x}", Sha256::digest(bytes)),
        size: bytes.len() as u64,
    })
}
pub fn set_setting(
    test: &Instance,
    guard: &InstanceMutationGuard,
    path: &str,
    pointer: &str,
    value: Value,
) -> Result<LabDetails> {
    ensure_launchable(test)?;
    let current = recovery::capture_observation(test, guard)?;
    let allowed = settings(test, &current)?;
    let setting = allowed
        .iter()
        .find(|s| s.path == path && s.pointer == pointer)
        .ok_or(RecoveryError::UnsafePath)?;
    if !setting_value_valid(setting, &value) {
        return Err(RecoveryError::Integrity);
    }
    let mut doc = read_verified_document(test, &current, path)?;
    *doc.pointer_mut(pointer).ok_or(RecoveryError::Integrity)? = value;
    let tmp = stage_bytes(test, &encode(&doc)?)?;
    let result = (|| {
        let preview = recovery::preview_selected_files(
            test,
            guard,
            &current.fingerprint,
            &BTreeMap::from([(path.to_owned(), Some(tmp.clone()))]),
            "Configuração na cópia de teste",
        )?;
        recovery::restore(test, guard, &preview.id)
    })();
    let _ = fs::remove_file(tmp.source);
    result?;
    inspect(test, guard)
}
pub fn preview_apply(
    source: &Instance,
    source_guard: &InstanceMutationGuard,
    test: &Instance,
    test_guard: &InstanceMutationGuard,
    selected: &[String],
) -> Result<RecoveryPreview> {
    ensure_launchable(test)?;
    let rec = record(test)?;
    let origin = recovery::capture_observation(source, source_guard)?;
    let current = recovery::capture_observation(test, test_guard)?;
    if origin.instance_uuid != rec.environment.origin_uuid {
        return Err(RecoveryError::Identity);
    }
    if origin.fingerprint != rec.baseline.fingerprint {
        return Err(RecoveryError::OriginChanged);
    }
    let current_settings = settings(test, &current)?;
    let available = changes(&rec, &current, &current_settings)?;
    let chosen: BTreeSet<_> = selected.iter().collect();
    if selected.is_empty()
        || selected.len() != chosen.len()
        || chosen
            .iter()
            .any(|id| !available.iter().any(|c| &c.id == *id))
    {
        return Err(RecoveryError::Integrity);
    }
    let mut replacements = BTreeMap::new();
    let mut documents: BTreeMap<String, Value> = BTreeMap::new();
    let mut temporary = Vec::new();
    for id in chosen {
        if let Some(base) = id.strip_prefix("mod:") {
            let before = grouped_mods(&rec.baseline.files)?;
            let after = grouped_mods(&current.files)?;
            let old = before.get(base).ok_or(RecoveryError::Integrity)?;
            let new = after.get(base).ok_or(RecoveryError::Integrity)?;
            replacements.insert(old.path.clone(), None);
            replacements.insert(
                new.path.clone(),
                Some(recovery::SelectedFile {
                    source: recovery::path(test.root(), &new.path)?,
                    sha256: new.sha256.clone(),
                    size: new.size,
                }),
            );
        } else {
            let target = current_settings
                .iter()
                .find(|s| id == &format!("setting:{}#{}", s.path, s.pointer))
                .ok_or(RecoveryError::Integrity)?;
            if !documents.contains_key(&target.path) {
                documents.insert(
                    target.path.clone(),
                    read_verified_document(source, &origin, &target.path)?,
                );
            }
            let doc = documents
                .get_mut(&target.path)
                .ok_or(RecoveryError::Integrity)?;
            *doc.pointer_mut(&target.pointer)
                .ok_or(RecoveryError::Integrity)? = target.value.clone();
        }
    }
    for (path, doc) in documents {
        let tmp = stage_bytes(test, &encode(&doc)?)?;
        temporary.push(tmp.source.clone());
        replacements.insert(path, Some(tmp));
    }
    let result = recovery::preview_selected_files(
        source,
        source_guard,
        &origin.fingerprint,
        &replacements,
        "Alterações selecionadas da cópia de teste",
    );
    for p in temporary {
        let _ = fs::remove_file(p);
    }
    let preview = result?;
    if recovery::capture_observation(test, test_guard)?.fingerprint != current.fingerprint {
        return Err(RecoveryError::Changed);
    }
    let binding = ApplyBinding {
        version: 1,
        preview_id: preview.id.clone(),
        source_uuid: origin.instance_uuid,
        test_uuid: current.instance_uuid,
        test_fingerprint: current.fingerprint,
    };
    let dir = directory(data(test)?, test.id().as_str())?;
    recovery::write_new(
        &recovery::path(&dir, &format!("apply-{}.json", preview.id))?,
        &encode(&binding)?,
    )?;
    Ok(preview)
}
pub fn apply(
    source: &Instance,
    source_guard: &InstanceMutationGuard,
    test: &Instance,
    test_guard: &InstanceMutationGuard,
    preview_id: &str,
) -> Result<RecoveryResult> {
    ensure_launchable(test)?;
    recovery::valid_id(preview_id)?;
    let rec = record(test)?;
    let p = marker(
        data(test)?,
        test.id().as_str(),
        &format!("apply-{preview_id}.json"),
    )?;
    let binding: ApplyBinding = read(&p)?;
    let current = recovery::capture_observation(test, test_guard)?;
    let origin = recovery::capture_observation(source, source_guard)?;
    if binding.version != 1
        || binding.preview_id != preview_id
        || binding.source_uuid != rec.environment.origin_uuid
        || binding.test_uuid != rec.environment.instance_uuid
        || origin.instance_uuid != binding.source_uuid
    {
        return Err(RecoveryError::Identity);
    }
    if origin.fingerprint != rec.baseline.fingerprint {
        return Err(RecoveryError::OriginChanged);
    }
    if current.fingerprint != binding.test_fingerprint {
        return Err(RecoveryError::StalePreview);
    }
    let result = recovery::restore(source, source_guard, preview_id)?;
    let _ = fs::remove_file(p);
    Ok(result)
}

/// A durable deletion marker makes interrupted removal resumable. The root is
/// derived only from the managed ID; links/hardlinks are refused before any
/// removal. Shared CAS objects and original worlds are never part of this tree.
pub fn delete(test: &Instance, guard: &InstanceMutationGuard) -> Result<()> {
    {
        let _pins = recovery::check_guard(test, guard)?;
    }
    let dir = directory(data(test)?, test.id().as_str())?;
    let record_path = recovery::path(&dir, "record.json")?;
    let uuid = if record_path.exists() {
        record(test)?.environment.instance_uuid
    } else {
        let intent: LabEnvironment = read(&recovery::path(&dir, "intent.json")?)?;
        if intent.id != test.id().as_str() {
            return Err(RecoveryError::Identity);
        }
        let claim = recovery::path(&dir, "claim.json")?;
        if claim.exists() {
            let identity: String = read(&claim)?;
            if identity != recovery::filesystem_identity(test)? {
                return Err(RecoveryError::Identity);
            }
            // No user payload is written before record.json is durable. Only
            // interrupted identity publication may remain in this early phase.
            let mut files = Vec::new();
            scan_tree(test.root(), "", &mut files, 0, false)?;
            if files.iter().any(|f| {
                f.path != ".aurora/recovery-identity.json"
                    && !(f.path.starts_with(".aurora/recovery-identity-")
                        && f.path.ends_with(".pending"))
            }) {
                return Err(RecoveryError::UnsafePath);
            }
        } else if fs::read_dir(test.root())?.next().is_some() {
            return Err(RecoveryError::Identity);
        }
        intent.instance_uuid
    };
    let deleting = recovery::path(&dir, "deleting.json")?;
    if !deleting.exists() {
        recovery::write_new(&deleting, &encode(&uuid)?)?;
    }
    let mut delete_pins = Vec::new();
    recovery::pin_tree(test.root(), &mut delete_pins, 0)?;
    let mut files = Vec::new();
    scan_tree(test.root(), "", &mut files, 0, false)?;
    files.sort_by_key(|f| f.path == ".aurora/recovery-identity.json");
    for file in files {
        let p = recovery::path(test.root(), &file.path)?;
        recovery::verify(&p, &file)?;
        let _parent_pins = recovery::pin_directory(p.parent().ok_or(RecoveryError::UnsafePath)?)?;
        fs::remove_file(p)?;
        #[cfg(test)]
        if FAULT.with(|f| f.get() == Fault::DeleteInterrupted) {
            return Err(RecoveryError::Pending);
        }
    }
    drop(delete_pins);
    fn remove_empty(root: &Path) -> Result<()> {
        let pins = recovery::pin_directory(root)?;
        for entry in fs::read_dir(root)? {
            let name = entry?
                .file_name()
                .into_string()
                .map_err(|_| RecoveryError::UnsafePath)?;
            let p = recovery::path(root, &name)?;
            if !p.is_dir() {
                return Err(RecoveryError::Changed);
            }
            remove_empty(&p)?;
        }
        drop(pins);
        // Parent stays pinned while removing this child. No recursive shell or
        // recursive unlink primitive can follow a replacement link.
        let _parent_pins =
            recovery::pin_directory(root.parent().ok_or(RecoveryError::UnsafePath)?)?;
        fs::remove_dir(root)?;
        Ok(())
    }
    remove_empty(test.root())?;
    recovery::write_new(&recovery::path(&dir, "deleted.json")?, &encode(&uuid)?)?;
    Ok(())
}

/// Completes only registry tombstones when an interrupted create/delete left no
/// instance root. Never creates, removes or follows an instance directory.
pub fn finish_deleted(data: &Path, id: &str) -> Result<bool> {
    let dir = directory(data, id)?;
    let root = recovery::path(data, &format!("instances/{id}"))?;
    if root.exists() {
        return Ok(false);
    }
    let record_path = recovery::path(&dir, "record.json")?;
    let env = if record_path.exists() {
        let rec: Record = read(&record_path)?;
        if rec.version != 1 {
            return Err(RecoveryError::Integrity);
        }
        rec.environment
    } else {
        read::<LabEnvironment>(&recovery::path(&dir, "intent.json")?)?
    };
    if env.id != id {
        return Err(RecoveryError::Identity);
    }
    recovery::valid_id(&env.instance_uuid)?;
    let deleting = recovery::path(&dir, "deleting.json")?;
    if !deleting.exists() {
        recovery::write_new(&deleting, &encode(&env.instance_uuid)?)?;
    }
    let tombstone: String = read(&deleting)?;
    if tombstone != env.instance_uuid {
        return Err(RecoveryError::Identity);
    }
    let p = recovery::path(&dir, "deleted.json")?;
    if !p.exists() {
        recovery::write_new(&p, &encode(&env.instance_uuid)?)?;
    }
    Ok(true)
}

#[cfg(test)]
#[path = "diagnostic_lab_tests.rs"]
mod tests;
