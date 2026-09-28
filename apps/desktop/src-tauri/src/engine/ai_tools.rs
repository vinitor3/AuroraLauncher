//! Local capability broker. Model proposals cannot name paths or execute code.
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use super::content_updates::{ensure_no_symlink_components, metadata_is_link};
use super::{acquire_instance_mutation_lock, Instance};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiToolMod {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub enabled: bool,
    pub managed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiToolSetting {
    pub id: String,
    pub label: String,
    pub value: Value,
    pub kind: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiToolHistoryEntry {
    pub id: String,
    pub kind: String,
    pub created_at_ms: u64,
    pub title: String,
    pub summary: String,
    pub issue_ids: Vec<String>,
    pub action_id: Option<String>,
    pub status: String,
    pub assessment: Option<String>,
    pub undo_available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiToolContext {
    pub schema_version: u32,
    pub context_id: String,
    pub mods: Vec<AiToolMod>,
    pub settings: Vec<AiToolSetting>,
    pub truncated: bool,
    pub history: Vec<AiToolHistoryEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AiToolProposal {
    RemoveMod {
        #[serde(rename = "targetId")]
        target_id: String,
        reason: String,
    },
    SetSetting {
        #[serde(rename = "targetId")]
        target_id: String,
        value: Value,
        reason: String,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiToolAdvice {
    pub summary: String,
    pub proposals: Vec<AiToolProposal>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolChange {
    pub label: String,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolPlan {
    pub plan_id: String,
    pub digest: String,
    pub title: String,
    pub changes: Vec<AiToolChange>,
    pub warnings: Vec<String>,
    pub expires_at_ms: u64,
}

const ERR: &str = "Não foi possível acessar os arquivos autorizados da instância com segurança.";
const STALE: &str = "A instância mudou. Atualize o contexto e revise uma nova proposta.";
const INVALID: &str = "A proposta não corresponde a uma ação permitida neste contexto.";
const EXPIRED: &str = "O plano expirou ou já foi utilizado. Revise uma nova proposta.";
const MAX_JAR: usize = 64 * 1024 * 1024;
const MAX_JSON: usize = 256 * 1024;
const MAX_SCAN: usize = 256 * 1024 * 1024;
const STORAGE: &str = ".aurora/ai-tools";

#[derive(Clone)]
struct Source {
    relative: String,
    hash: String,
    label: String,
    setting: Option<(String, AiToolSetting)>,
    managed: bool,
}

struct Snapshot {
    public: AiToolContext,
    fingerprint: String,
    targets: BTreeMap<String, Source>,
}

struct ContextState {
    instance: Instance,
    window: String,
    expires: u64,
    snapshot: Snapshot,
}

#[derive(Clone)]
struct Prepared {
    public: AiToolPlan,
    instance: Instance,
    window: String,
    fingerprint: String,
    source: Source,
    replacement: Option<Vec<u8>>,
    undo: Option<Journal>,
}

#[derive(Default)]
struct State {
    contexts: BTreeMap<String, ContextState>,
    plans: BTreeMap<String, Prepared>,
}

#[derive(Default)]
pub struct AiToolBroker(Mutex<State>);

pub struct AppliedAction {
    pub receipt: AiToolHistoryEntry,
    pub action_kind: String,
    pub target_name: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    instance_identity: String,
    action_id: String,
    kind: String,
    target: String,
    label: String,
    before_hash: String,
    changes: Vec<AiToolChange>,
    after_hash: Option<String>,
    created_at_ms: u64,
    signature: String,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn opaque() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn safe_root(root: &Path) -> Result<(), String> {
    for path in root.ancestors() {
        let meta = fs::symlink_metadata(path).map_err(|_| ERR)?;
        if !meta.is_dir() || metadata_is_link(&meta) {
            return Err(ERR.into());
        }
    }
    Ok(())
}

fn safe_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    safe_root(root)?;
    if relative.is_empty()
        || relative.contains([':', '\\'])
        || !Path::new(relative)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(ERR.into());
    }
    ensure_no_symlink_components(root, relative).map_err(|_| ERR)?;
    let path = root.join(relative);
    if path.exists()
        && !path
            .canonicalize()
            .map_err(|_| ERR)?
            .starts_with(root.canonicalize().map_err(|_| ERR)?)
    {
        return Err(ERR.into());
    }
    Ok(path)
}

#[cfg(windows)]
fn single_link(file: &File) -> Result<bool, String> {
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
    // BY_HANDLE_FILE_INFORMATION: nNumberOfLinks is DWORD index 10.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(ERR.into());
    }
    Ok(info.values[10] == 1)
}

#[cfg(unix)]
fn single_link(file: &File) -> Result<bool, String> {
    use std::os::unix::fs::MetadataExt;
    Ok(file.metadata().map_err(|_| ERR)?.nlink() == 1)
}

#[cfg(not(any(unix, windows)))]
fn single_link(_file: &File) -> Result<bool, String> {
    Ok(false)
}

fn secure_read(root: &Path, relative: &str, limit: usize) -> Result<Vec<u8>, String> {
    let path = safe_path(root, relative)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000).share_mode(1);
    }
    let mut file = options.open(&path).map_err(|_| ERR)?;
    let before = file.metadata().map_err(|_| ERR)?;
    if !before.is_file()
        || metadata_is_link(&before)
        || before.len() > limit as u64
        || !single_link(&file)?
    {
        return Err(ERR.into());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ERR)?;
    let after = file.metadata().map_err(|_| ERR)?;
    if bytes.len() > limit
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err(STALE.into());
    }
    Ok(bytes)
}

fn write_new(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), String> {
    let path = safe_path(root, relative)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| ERR)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| ERR.into())
}

fn safe_label(value: &str, fallback: &str) -> String {
    let lower = value.to_lowercase();
    if value.is_empty()
        || value.len() > 96
        || value
            .chars()
            .any(|c| c.is_control() || "@:/\\<>={}".contains(c))
        || ["token", "password", "secret", "bearer", "users", "appdata"]
            .iter()
            .any(|word| lower.contains(word))
    {
        fallback.into()
    } else {
        value.to_owned()
    }
}

fn metadata(bytes: &[u8], filename: &str) -> (String, Option<String>, bool) {
    let mut managed = filename.to_ascii_lowercase().contains("aurora");
    let Ok(mut archive) = ZipArchive::new(Cursor::new(bytes)) else {
        return ("Mod sem metadados válidos".into(), None, true);
    };
    if archive.len() > 8192 {
        return ("Mod excede o limite de metadados".into(), None, true);
    }
    managed |= archive
        .file_names()
        .any(|name| name.to_ascii_lowercase().starts_with("com/aurora/"));
    let mut name = "Mod local".to_owned();
    let mut identified = false;
    let mut version = None;
    for entry_name in ["fabric.mod.json", "mcmod.info", "META-INF/mods.toml"] {
        let Ok(entry) = archive.by_name(entry_name) else {
            continue;
        };
        if entry.size() > 64 * 1024 {
            managed = true;
            continue;
        }
        let mut text = String::new();
        if entry.take(64 * 1024 + 1).read_to_string(&mut text).is_err() || text.len() > 64 * 1024 {
            managed = true;
            continue;
        }
        managed |= text.to_ascii_lowercase().contains("aurora_core")
            || text.to_ascii_lowercase().contains("aurora_skins");
        if let Ok(json) = serde_json::from_str::<Value>(&text) {
            let value = if let Some(first) = json.as_array().and_then(|a| a.first()) {
                first
            } else {
                &json
            };
            name = safe_label(value["name"].as_str().unwrap_or(&name), "Mod local");
            identified |= value["name"].as_str().is_some();
            version = value["version"].as_str().map(str::to_owned);
        } else if entry_name.ends_with(".toml") {
            for line in text.lines().take(1024).map(str::trim) {
                if let Some((key, value)) = line.split_once('=') {
                    let value = value.trim().trim_matches('"');
                    if key.trim() == "displayName" {
                        name = safe_label(value, "Mod local");
                        identified = true;
                    }
                    if key.trim() == "version" {
                        version = Some(value.into());
                    }
                }
            }
        }
    }
    version = version.filter(|v| {
        !v.is_empty()
            && v.len() <= 32
            && v.as_bytes()[0].is_ascii_digit()
            && v.chars()
                .all(|c| c.is_ascii_alphanumeric() || ".-+_".contains(c))
    });
    (name, version, managed || !identified)
}

fn settings_key(key: &str) -> Option<(&'static str, Option<(f64, f64)>)> {
    let normal = key.to_ascii_lowercase().replace(['_', '-'], "");
    match normal.as_str() {
        "renderdistance" => Some(("Distância de renderização", Some((2.0, 32.0)))),
        "maxfps" | "frameratelimit" => Some(("Limite de FPS", Some((30.0, 260.0)))),
        "entitydistancescaling" => Some(("Distância de entidades", Some((0.5, 5.0)))),
        "enablefog" | "fog" => Some(("Névoa", None)),
        "fancygraphics" => Some(("Gráficos detalhados", None)),
        "showfps" => Some(("Exibir FPS", None)),
        "entityculling" | "enableentityculling" => Some(("Ocultar entidades fora de visão", None)),
        "blockentityculling" => Some(("Ocultar blocos fora de visão", None)),
        _ => None,
    }
}

fn sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
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
    .any(|word| lower.contains(word))
}

fn collect_settings(
    value: &Value,
    pointer: &str,
    depth: usize,
    relative: &str,
    digest: &str,
    index: usize,
    snapshot: &mut Snapshot,
) {
    if depth > 3 || snapshot.public.settings.len() >= 64 {
        snapshot.public.truncated = true;
        return;
    }
    let Some(object) = value.as_object() else {
        return;
    };
    for (key, value) in object.iter().take(128) {
        if snapshot.public.settings.len() >= 64 {
            snapshot.public.truncated = true;
            break;
        }
        if sensitive_key(key) || key.contains(['/', '~']) {
            continue;
        }
        let next = format!("{pointer}/{key}");
        if value.is_object() {
            collect_settings(value, &next, depth + 1, relative, digest, index, snapshot);
            continue;
        }
        let Some((label, range)) = settings_key(key) else {
            continue;
        };
        let valid = match range {
            None => value.is_boolean(),
            Some((min, max)) => value
                .as_f64()
                .is_some_and(|v| v.is_finite() && (min..=max).contains(&v)),
        };
        if !valid {
            continue;
        }
        let id = opaque();
        let setting = AiToolSetting {
            id: id.clone(),
            label: format!("{label} · configuração {index}"),
            value: value.clone(),
            kind: if range.is_some() { "number" } else { "boolean" }.into(),
            min: range.map(|v| v.0),
            max: range.map(|v| v.1),
        };
        snapshot.targets.insert(
            id,
            Source {
                relative: relative.into(),
                hash: digest.into(),
                label: setting.label.clone(),
                setting: Some((next, setting.clone())),
                managed: false,
            },
        );
        snapshot.public.settings.push(setting);
    }
}

fn scan(instance: &Instance) -> Result<Snapshot, String> {
    safe_root(instance.root())?;
    let mut snapshot = Snapshot {
        public: AiToolContext {
            schema_version: 1,
            context_id: opaque(),
            mods: Vec::new(),
            settings: Vec::new(),
            truncated: false,
            history: Vec::new(),
        },
        fingerprint: String::new(),
        targets: BTreeMap::new(),
    };
    let mut state = instance_identity(instance)?.into_bytes();
    let mut remaining = MAX_SCAN;
    for folder in ["mods", "config"] {
        let directory = safe_path(instance.root(), folder)?;
        if !directory.exists() {
            continue;
        }
        let mut entries = Vec::new();
        for (index, entry) in fs::read_dir(directory).map_err(|_| ERR)?.enumerate() {
            if index >= 256 {
                snapshot.public.truncated = true;
                break;
            }
            let entry = entry.map_err(|_| ERR)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains(['/', '\\', ':']) {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            if (folder == "mods" && (lower.ends_with(".jar") || lower.ends_with(".jar.disabled")))
                || (folder == "config" && lower.ends_with(".json") && !sensitive_key(&lower))
            {
                entries.push(name);
            }
        }
        entries.sort();
        for (index, name) in entries.into_iter().enumerate() {
            let relative = format!("{folder}/{name}");
            if (folder == "mods" && snapshot.public.mods.len() >= 128)
                || (folder == "config" && index >= 16)
            {
                snapshot.public.truncated = true;
                break;
            }
            let max = if folder == "mods" { MAX_JAR } else { MAX_JSON };
            let bytes = match secure_read(instance.root(), &relative, max.min(remaining)) {
                Ok(bytes) => bytes,
                Err(_) => {
                    snapshot.public.truncated = true;
                    state.extend_from_slice(relative.as_bytes());
                    state.extend_from_slice(b":unavailable;");
                    continue;
                }
            };
            remaining = remaining.saturating_sub(bytes.len());
            let digest = hash(&bytes);
            state.extend_from_slice(relative.as_bytes());
            state.extend_from_slice(digest.as_bytes());
            state.push(0);
            if folder == "mods" {
                let (label, version, managed) = metadata(&bytes, &name);
                let id = opaque();
                snapshot.public.mods.push(AiToolMod {
                    id: id.clone(),
                    name: label.clone(),
                    version,
                    enabled: !name.ends_with(".disabled"),
                    managed,
                });
                snapshot.targets.insert(
                    id,
                    Source {
                        relative,
                        hash: digest,
                        label,
                        setting: None,
                        managed,
                    },
                );
            } else if let Ok(json) = serde_json::from_slice::<Value>(&bytes) {
                collect_settings(&json, "", 0, &relative, &digest, index + 1, &mut snapshot);
            }
        }
    }
    snapshot.fingerprint = hash(&state);
    Ok(snapshot)
}

fn validate_reason(reason: &str) -> Result<(), String> {
    if reason.is_empty() || reason.chars().count() > 240 || reason.chars().any(char::is_control) {
        return Err(INVALID.into());
    }
    Ok(())
}

impl AiToolBroker {
    pub fn context(
        &self,
        instance: Instance,
        window: &str,
        history: Vec<AiToolHistoryEntry>,
    ) -> Result<AiToolContext, String> {
        let mut snapshot = scan(&instance)?;
        snapshot.public.history = history;
        let public = snapshot.public.clone();
        let mut state = self.0.lock().map_err(|_| ERR)?;
        state.contexts.retain(|_, c| {
            c.expires > now() && !(c.window == window && c.instance.root() == instance.root())
        });
        state.plans.retain(|_, p| p.public.expires_at_ms > now());
        if state.contexts.len() >= 16 {
            return Err("Há muitos contextos ativos. Aguarde alguns minutos.".into());
        }
        state.contexts.insert(
            public.context_id.clone(),
            ContextState {
                instance,
                window: window.into(),
                expires: now() + 10 * 60 * 1000,
                snapshot,
            },
        );
        Ok(public)
    }

    pub fn validate_advice(
        &self,
        instance: &Instance,
        window: &str,
        context_id: &str,
        advice: &AiToolAdvice,
    ) -> Result<Vec<(String, String)>, String> {
        if advice.summary.is_empty()
            || advice.summary.chars().count() > 600
            || advice.summary.chars().any(char::is_control)
            || advice.proposals.len() > 3
        {
            return Err(INVALID.into());
        }
        let state = self.0.lock().map_err(|_| ERR)?;
        let context = self.checked_context(&state, instance, window, context_id)?;
        let mut targets = Vec::new();
        for proposal in &advice.proposals {
            let (source, kind, _) = proposed_change(context, proposal)?;
            targets.push((kind, source.label));
        }
        Ok(targets)
    }

    fn checked_context<'a>(
        &self,
        state: &'a State,
        instance: &Instance,
        window: &str,
        id: &str,
    ) -> Result<&'a ContextState, String> {
        let context = state.contexts.get(id).ok_or(EXPIRED)?;
        if context.expires <= now()
            || context.window != window
            || context.instance.root() != instance.root()
        {
            return Err(EXPIRED.into());
        }
        if scan(instance)?.fingerprint != context.snapshot.fingerprint {
            return Err(STALE.into());
        }
        Ok(context)
    }

    pub fn prepare(
        &self,
        instance: &Instance,
        window: &str,
        context_id: &str,
        proposal: AiToolProposal,
    ) -> Result<AiToolPlan, String> {
        let mut state = self.0.lock().map_err(|_| ERR)?;
        let context = self.checked_context(&state, instance, window, context_id)?;
        let (source, kind, replacement) = proposed_change(context, &proposal)?;
        let before = source
            .setting
            .as_ref()
            .map(|(_, s)| s.value.to_string())
            .unwrap_or_else(|| "Instalado na pasta mods".into());
        let after = match &proposal {
            AiToolProposal::SetSetting { value, .. } => value.to_string(),
            _ => "Movido para quarentena reversível".into(),
        };
        let title = if kind == "remove_mod" {
            "Remover mod da instância"
        } else {
            "Alterar configuração existente"
        }
        .to_owned();
        let mut public = AiToolPlan {
            plan_id: opaque(),
            digest: String::new(),
            title,
            changes: vec![AiToolChange {
                label: format!("{} ({})", source.label, source.relative),
                before,
                after,
            }],
            warnings: vec![
                "O jogo deve permanecer fechado. O conteúdo atual será preservado para desfazer."
                    .into(),
            ],
            expires_at_ms: now() + 5 * 60 * 1000,
        };
        public.digest = hash(&serde_json::to_vec(&public).map_err(|_| ERR)?);
        let prepared = Prepared {
            public: public.clone(),
            instance: instance.clone(),
            window: window.into(),
            fingerprint: context.snapshot.fingerprint.clone(),
            source,
            replacement,
            undo: None,
        };
        if state.plans.len() >= 32 {
            return Err("Há muitos planos ativos. Feche uma revisão antes de continuar.".into());
        }
        state.plans.insert(public.plan_id.clone(), prepared);
        Ok(public)
    }

    pub fn revoke(&self, window: &str, plan_id: &str) -> Result<(), String> {
        let mut state = self.0.lock().map_err(|_| ERR)?;
        if state
            .plans
            .get(plan_id)
            .is_some_and(|plan| plan.window == window)
        {
            state.plans.remove(plan_id);
        }
        Ok(())
    }

    pub fn confirm<F>(
        &self,
        window: &str,
        plan_id: &str,
        digest: &str,
        idle: F,
    ) -> Result<AppliedAction, String>
    where
        F: FnOnce(&Instance) -> Result<(), String>,
    {
        let plan = {
            let mut state = self.0.lock().map_err(|_| ERR)?;
            let plan = state.plans.get(plan_id).ok_or(EXPIRED)?;
            if plan.window != window
                || plan.public.expires_at_ms <= now()
                || plan.public.digest != digest
            {
                return Err(EXPIRED.into());
            }
            state.plans.remove(plan_id).ok_or(EXPIRED)?
        };
        safe_root(plan.instance.root())?;
        let _guard = acquire_instance_mutation_lock(&plan.instance)
            .map_err(|_| "A instância está ocupada. Feche o jogo e aguarde outras operações.")?;
        idle(&plan.instance)?;
        if plan.public.expires_at_ms <= now() {
            return Err(EXPIRED.into());
        }
        if scan(&plan.instance)?.fingerprint != plan.fingerprint {
            return Err(STALE.into());
        }
        let result = if plan.undo.is_some() {
            apply_undo(&plan)
        } else {
            apply_action(&plan)
        }?;
        let mut state = self.0.lock().map_err(|_| ERR)?;
        state
            .contexts
            .retain(|_, context| context.instance.root() != plan.instance.root());
        state
            .plans
            .retain(|_, other| other.instance.root() != plan.instance.root());
        Ok(result)
    }
}

fn proposed_change(
    context: &ContextState,
    proposal: &AiToolProposal,
) -> Result<(Source, String, Option<Vec<u8>>), String> {
    let (id, reason) = match proposal {
        AiToolProposal::RemoveMod { target_id, reason }
        | AiToolProposal::SetSetting {
            target_id, reason, ..
        } => (target_id, reason),
    };
    validate_reason(reason)?;
    let source = context.snapshot.targets.get(id).ok_or(INVALID)?.clone();
    if source.managed {
        return Err(
            "Aurora Core, Aurora Skins e conteúdo protegido não podem ser removidos pela IA."
                .into(),
        );
    }
    match proposal {
        AiToolProposal::RemoveMod { .. } if source.setting.is_none() => {
            Ok((source, "remove_mod".into(), None))
        }
        AiToolProposal::SetSetting { value, .. } => {
            let (pointer, setting) = source.setting.as_ref().ok_or(INVALID)?;
            let valid = if setting.kind == "boolean" {
                value.is_boolean()
            } else {
                value.as_f64().is_some_and(|v| {
                    v.is_finite()
                        && v >= setting.min.unwrap_or(0.0)
                        && v <= setting.max.unwrap_or(0.0)
                })
            };
            if !valid || value == &setting.value {
                return Err(INVALID.into());
            }
            if (setting.value.is_i64() || setting.value.is_u64())
                && !(value.is_i64() || value.is_u64())
            {
                return Err(INVALID.into());
            }
            let bytes = secure_read(context.instance.root(), &source.relative, MAX_JSON)?;
            if hash(&bytes) != source.hash {
                return Err(STALE.into());
            }
            let mut json: Value = serde_json::from_slice(&bytes).map_err(|_| INVALID)?;
            let field = json.pointer_mut(pointer).ok_or(STALE)?;
            if field != &setting.value {
                return Err(STALE.into());
            }
            *field = value.clone();
            let replacement = serde_json::to_vec_pretty(&json).map_err(|_| INVALID)?;
            if replacement.len() > MAX_JSON {
                return Err(INVALID.into());
            }
            Ok((source, "set_setting".into(), Some(replacement)))
        }
        _ => Err(INVALID.into()),
    }
}

fn directory_handles(path: &Path) -> Result<Vec<File>, String> {
    safe_root(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let mut handles = Vec::new();
        for directory in path.ancestors() {
            let handle = OpenOptions::new()
                .read(true)
                .custom_flags(0x0220_0000)
                .share_mode(3)
                .open(directory)
                .map_err(|_| ERR)?;
            if metadata_is_link(&handle.metadata().map_err(|_| ERR)?) {
                return Err(ERR.into());
            }
            handles.push(handle);
        }
        Ok(handles)
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

fn key(instance: &Instance, create: bool) -> Result<Vec<u8>, String> {
    let data = instance.root().parent().and_then(Path::parent).ok_or(ERR)?;
    let relative = ".aurora/ai-tools-auth-key";
    let path = safe_path(data, relative)?;
    if !path.exists() && create {
        let directory = safe_path(data, ".aurora")?;
        fs::create_dir_all(&directory).map_err(|_| ERR)?;
        let _handles = directory_handles(&directory)?;
        let secret = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        if write_new(data, relative, secret.as_bytes()).is_err() && !path.exists() {
            return Err(ERR.into());
        }
    }
    let bytes = secure_read(data, relative, 64)?;
    if bytes.len() != 64 || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return Err(ERR.into());
    }
    Ok(bytes)
}

fn signature(journal: &Journal, secret: &[u8]) -> Result<String, String> {
    let mut unsigned = journal.clone();
    unsigned.signature.clear();
    let bytes = serde_json::to_vec(&unsigned).map_err(|_| ERR)?;
    let mut inner = [0x36u8; 64];
    let mut outer = [0x5cu8; 64];
    for (index, byte) in secret.iter().enumerate() {
        inner[index] ^= byte;
        outer[index] ^= byte;
    }
    let mut digest = Sha256::new();
    digest.update(inner);
    digest.update(&bytes);
    let middle = digest.finalize();
    let mut digest = Sha256::new();
    digest.update(outer);
    digest.update(middle);
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn instance_identity(instance: &Instance) -> Result<String, String> {
    safe_root(instance.root())?;
    #[cfg(windows)]
    let identity = {
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        #[repr(C)]
        struct Info {
            values: [u32; 13],
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut Info) -> i32;
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(0x0220_0000)
            .open(instance.root())
            .map_err(|_| ERR)?;
        let mut info = Info { values: [0; 13] };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(ERR.into());
        }
        format!(
            "windows:{}:{}:{}:{}:{}",
            info.values[7], info.values[11], info.values[12], info.values[1], info.values[2]
        )
    };
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(instance.root()).map_err(|_| ERR)?;
        format!(
            "unix:{}:{}:{:?}",
            metadata.dev(),
            metadata.ino(),
            metadata.created().ok()
        )
    };
    #[cfg(not(any(windows, unix)))]
    let identity = return Err(ERR.into());
    Ok(hash(identity.as_bytes()))
}

fn journal_relative(action: &str, file: &str) -> Result<String, String> {
    if uuid::Uuid::parse_str(action).is_err() {
        return Err(INVALID.into());
    }
    Ok(format!("{STORAGE}/{action}/{file}"))
}

fn load_journal(instance: &Instance, action: &str) -> Result<Journal, String> {
    let bytes = secure_read(
        instance.root(),
        &journal_relative(action, "plan.json")?,
        16 * 1024,
    )?;
    let journal: Journal = serde_json::from_slice(&bytes).map_err(|_| ERR)?;
    let expected = signature(&journal, &key(instance, false)?)?;
    if journal.schema_version != 1
        || journal.instance_identity != instance_identity(instance)?
        || journal.action_id != action
        || journal.signature.len() != expected.len()
        || journal
            .signature
            .as_bytes()
            .iter()
            .zip(expected.as_bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            != 0
    {
        return Err("O histórico desta ação não passou na verificação de integridade.".into());
    }
    let target = Path::new(&journal.target);
    if target.components().count() != 2
        || !((journal.kind == "remove_mod"
            && journal.target.starts_with("mods/")
            && (journal.target.ends_with(".jar") || journal.target.ends_with(".jar.disabled")))
            || (journal.kind == "set_setting"
                && journal.target.starts_with("config/")
                && journal.target.ends_with(".json")))
    {
        return Err(ERR.into());
    }
    safe_path(instance.root(), &journal.target)?;
    Ok(journal)
}

fn marker(instance: &Instance, action: &str, name: &str) -> bool {
    journal_relative(action, name)
        .ok()
        .and_then(|relative| secure_read(instance.root(), &relative, 16).ok())
        .is_some_and(|bytes| bytes == b"complete")
}

fn receipt(instance: &Instance, journal: &Journal) -> AiToolHistoryEntry {
    let undone = marker(instance, &journal.action_id, "undone");
    let applied = marker(instance, &journal.action_id, "applied");
    let backup = journal_relative(&journal.action_id, "before.bin")
        .ok()
        .and_then(|relative| secure_read(instance.root(), &relative, MAX_JAR).ok())
        .is_some_and(|bytes| hash(&bytes) == journal.before_hash);
    AiToolHistoryEntry {
        id: journal.action_id.clone(),
        kind: "action".into(),
        created_at_ms: journal.created_at_ms,
        title: if journal.kind == "remove_mod" {
            "Remoção reversível de mod"
        } else {
            "Alteração de configuração"
        }
        .into(),
        summary: format!(
            "{} · {}",
            journal.label,
            if undone {
                "ação desfeita"
            } else if applied {
                "ação aplicada"
            } else {
                "operação pendente; confira os arquivos antes de prosseguir"
            }
        ),
        issue_ids: Vec::new(),
        action_id: Some(journal.action_id.clone()),
        status: if undone {
            "undone"
        } else if applied {
            "applied"
        } else {
            "pending"
        }
        .into(),
        assessment: Some("inconclusive".into()),
        undo_available: backup && !undone,
    }
}

fn ensure_expected(root: &Path, source: &Source) -> Result<(), String> {
    let bytes = secure_read(root, &source.relative, MAX_JAR)?;
    if hash(&bytes) != source.hash {
        return Err(STALE.into());
    }
    Ok(())
}

fn apply_action(plan: &Prepared) -> Result<AppliedAction, String> {
    let instance = &plan.instance;
    let _root_handles = directory_handles(instance.root())?;
    let source = safe_path(instance.root(), &plan.source.relative)?;
    let _source_handles = directory_handles(source.parent().ok_or(ERR)?)?;
    ensure_expected(instance.root(), &plan.source)?;
    if plan.source.managed {
        return Err(INVALID.into());
    }
    let action = opaque();
    let relative = format!("{STORAGE}/{action}");
    let directory = safe_path(instance.root(), &relative)?;
    fs::create_dir_all(&directory).map_err(|_| ERR)?;
    let _journal_handles = directory_handles(&directory)?;
    let secret = key(instance, true)?;
    let mut journal = Journal {
        schema_version: 1,
        instance_identity: instance_identity(instance)?,
        action_id: action.clone(),
        kind: if plan.replacement.is_some() {
            "set_setting"
        } else {
            "remove_mod"
        }
        .into(),
        target: plan.source.relative.clone(),
        label: plan.source.label.clone(),
        changes: plan.public.changes.clone(),
        before_hash: plan.source.hash.clone(),
        after_hash: plan.replacement.as_ref().map(|bytes| hash(bytes)),
        created_at_ms: now(),
        signature: String::new(),
    };
    journal.signature = signature(&journal, &secret)?;
    write_new(
        instance.root(),
        &journal_relative(&action, "plan.json")?,
        &serde_json::to_vec(&journal).map_err(|_| ERR)?,
    )?;
    if let Some(replacement) = &plan.replacement {
        write_new(
            instance.root(),
            &journal_relative(&action, "after.bin")?,
            replacement,
        )?;
    }
    let backup = safe_path(instance.root(), &journal_relative(&action, "before.bin")?)?;
    fs::rename(&source, &backup).map_err(|_| ERR)?;
    let valid = secure_read(
        instance.root(),
        &journal_relative(&action, "before.bin")?,
        MAX_JAR,
    )
    .is_ok_and(|bytes| hash(&bytes) == journal.before_hash);
    if !valid {
        if !source.exists() {
            let _ = fs::rename(&backup, &source);
        }
        return Err(STALE.into());
    }
    if plan.replacement.is_some() {
        let staged = safe_path(instance.root(), &journal_relative(&action, "after.bin")?)?;
        if fs::rename(&staged, &source).is_err() {
            if !source.exists() {
                let _ = fs::rename(&backup, &source);
            }
            return Err("A alteração não pôde ser concluída. Consulte o histórico; o original foi preservado.".into());
        }
    }
    let _ = write_new(
        instance.root(),
        &journal_relative(&action, "applied")?,
        b"complete",
    );
    Ok(AppliedAction {
        receipt: receipt(instance, &journal),
        action_kind: journal.kind.clone(),
        target_name: if journal.kind == "set_setting" {
            format!(
                "{} {} para {}",
                journal
                    .label
                    .split(" · configuração ")
                    .next()
                    .unwrap_or(&journal.label),
                plan.public.changes[0].before,
                plan.public.changes[0].after
            )
        } else {
            journal.label
        },
    })
}

fn apply_undo(plan: &Prepared) -> Result<AppliedAction, String> {
    let instance = &plan.instance;
    let saved = plan.undo.as_ref().ok_or(INVALID)?;
    let journal = load_journal(instance, &saved.action_id)?;
    if journal.signature != saved.signature || marker(instance, &journal.action_id, "undone") {
        return Err(STALE.into());
    }
    let _root_handles = directory_handles(instance.root())?;
    let source = safe_path(instance.root(), &journal.target)?;
    let _source_handles = directory_handles(source.parent().ok_or(ERR)?)?;
    let backup_relative = journal_relative(&journal.action_id, "before.bin")?;
    let backup = safe_path(instance.root(), &backup_relative)?;
    let _journal_handles = directory_handles(backup.parent().ok_or(ERR)?)?;
    if hash(&secure_read(instance.root(), &backup_relative, MAX_JAR)?) != journal.before_hash {
        return Err(STALE.into());
    }
    let displaced = safe_path(
        instance.root(),
        &journal_relative(&journal.action_id, "undo-after.bin")?,
    )?;
    if let Some(expected) = &journal.after_hash {
        if source.exists() || !pending_config(instance, &journal) {
            if hash(&secure_read(instance.root(), &journal.target, MAX_JSON)?) != *expected {
                return Err(STALE.into());
            }
            fs::rename(&source, &displaced).map_err(|_| ERR)?;
        }
    } else if source.exists() {
        return Err(STALE.into());
    }
    if fs::rename(&backup, &source).is_err() {
        if displaced.exists() && !source.exists() {
            let _ = fs::rename(&displaced, &source);
        }
        return Err(
            "Não foi possível restaurar o conteúdo. Os arquivos de recuperação foram preservados."
                .into(),
        );
    }
    let _ = write_new(
        instance.root(),
        &journal_relative(&journal.action_id, "undone")?,
        b"complete",
    );
    Ok(AppliedAction {
        receipt: receipt(instance, &journal),
        action_kind: journal.kind,
        target_name: journal.label,
    })
}

fn pending_config(instance: &Instance, journal: &Journal) -> bool {
    !marker(instance, &journal.action_id, "applied")
        && journal.after_hash.as_ref().is_some_and(|expected| {
            journal_relative(&journal.action_id, "after.bin")
                .ok()
                .and_then(|relative| secure_read(instance.root(), &relative, MAX_JSON).ok())
                .is_some_and(|bytes| hash(&bytes) == *expected)
        })
}

impl AiToolBroker {
    pub fn history(&self, instance: &Instance) -> Result<Vec<AiToolHistoryEntry>, String> {
        let directory = safe_path(instance.root(), STORAGE)?;
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(directory).map_err(|_| ERR)?.take(128) {
            let name = entry
                .map_err(|_| ERR)?
                .file_name()
                .to_string_lossy()
                .into_owned();
            if uuid::Uuid::parse_str(&name).is_err() {
                continue;
            }
            // A damaged or incomplete record must not hide other verified recovery plans.
            if let Ok(journal) = load_journal(instance, &name) {
                entries.push(receipt(instance, &journal));
            }
        }
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.created_at_ms));
        Ok(entries)
    }

    pub fn prepare_undo(
        &self,
        instance: &Instance,
        window: &str,
        action_id: &str,
    ) -> Result<AiToolPlan, String> {
        let journal = load_journal(instance, action_id)?;
        let status = receipt(instance, &journal);
        if !status.undo_available {
            return Err("Esta ação já foi desfeita ou não tem conteúdo recuperável.".into());
        }
        let target = safe_path(instance.root(), &journal.target)?;
        if let Some(expected) = &journal.after_hash {
            if (target.exists() || !pending_config(instance, &journal))
                && hash(&secure_read(instance.root(), &journal.target, MAX_JSON)?) != *expected
            {
                return Err(STALE.into());
            }
        } else if target.exists() {
            return Err(STALE.into());
        }
        let snapshot = scan(instance)?;
        let mut public=AiToolPlan{plan_id:opaque(),digest:String::new(),title:"Desfazer ação confirmada".into(),changes:journal.changes.iter().map(|change|AiToolChange{label:change.label.clone(),before:change.after.clone(),after:change.before.clone()}).collect(),warnings:vec!["O jogo deve estar fechado. Alterações posteriores impedem a restauração automática.".into()],expires_at_ms:now()+5*60*1000};
        public.digest = hash(&serde_json::to_vec(&public).map_err(|_| ERR)?);
        let prepared = Prepared {
            public: public.clone(),
            instance: instance.clone(),
            window: window.into(),
            fingerprint: snapshot.fingerprint,
            source: Source {
                relative: journal.target.clone(),
                hash: journal.before_hash.clone(),
                label: journal.label.clone(),
                setting: None,
                managed: false,
            },
            replacement: None,
            undo: Some(journal),
        };
        let mut state = self.0.lock().map_err(|_| ERR)?;
        state
            .plans
            .retain(|_, plan| plan.public.expires_at_ms > now());
        if state.plans.len() >= 32 {
            return Err("Há muitos planos ativos. Aguarde ou feche revisões anteriores.".into());
        }
        state.plans.insert(public.plan_id.clone(), prepared);
        Ok(public)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{InstanceId, InstanceLayout};

    #[test]
    fn expired_context_and_plan_never_write() {
        let root = std::env::temp_dir().join(format!("aurora-ai-expiry-{}", opaque()));
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("expiry").unwrap());
        instance.ensure_layout().unwrap();
        fs::create_dir(instance.root().join("config")).unwrap();
        let path = instance.root().join("config/render.json");
        let original = b"{\"renderDistance\":16}";
        fs::write(&path, original).unwrap();
        let broker = AiToolBroker::default();
        let context = broker
            .context(instance.clone(), "main", Vec::new())
            .unwrap();
        let proposal = AiToolProposal::SetSetting {
            target_id: context.settings[0].id.clone(),
            value: serde_json::json!(8),
            reason: "Teste sintético de expiração.".into(),
        };
        let plan = broker
            .prepare(&instance, "main", &context.context_id, proposal.clone())
            .unwrap();
        {
            let mut state = broker.0.lock().unwrap();
            state.contexts.get_mut(&context.context_id).unwrap().expires = 0;
            state
                .plans
                .get_mut(&plan.plan_id)
                .unwrap()
                .public
                .expires_at_ms = 0;
        }
        assert!(broker
            .prepare(&instance, "main", &context.context_id, proposal)
            .is_err());
        assert!(broker
            .confirm("main", &plan.plan_id, &plan.digest, |_| panic!(
                "expired plan cannot reach mutation"
            ))
            .is_err());
        assert_eq!(fs::read(path).unwrap(), original);
        assert!(!instance.root().join(STORAGE).exists());
        fs::remove_dir_all(root).unwrap();
    }
}
