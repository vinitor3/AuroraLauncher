//! Trusted per-instance launch preferences. Themes and remote manifests cannot write these.
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::content_updates::{ensure_no_symlink_components, metadata_is_link};
use super::{recommended_memory_mb, Instance, InstanceMutationGuard};

const SETTINGS_PATH: &str = ".aurora/launch-settings.json";
const MAX_BYTES: u64 = 32 * 1024;
pub const MIN_MEMORY_MB: u32 = 512;
pub const MAX_MEMORY_MB: u32 = 262_144;

#[derive(Debug, Error)]
pub enum LaunchSettingsError {
    #[error("Use entre 512 e 262144 MB; a memória inicial não pode exceder a máxima.")]
    Memory,
    #[error("A memória configurada excede a RAM física deste computador.")]
    MemoryExceedsSystem,
    #[error("Não foi possível calcular a RAM automática. Use o modo manual com pelo menos 512 MB disponíveis para o jogo.")]
    MemoryUnavailable,
    #[error("Argumento JVM não permitido. Use somente as opções de desempenho documentadas, um argumento por linha; configure a RAM nos campos próprios.")]
    Argument,
    #[error("Selecione apenas um coletor de lixo; use o modo padrão ao definir um coletor nos argumentos JVM.")]
    CollectorConflict,
    #[error("As configurações de inicialização são inválidas ou usam uma versão não suportada. Revise ou restaure os padrões no painel Java & engine.")]
    InvalidSettings,
    #[error("Não foi possível acessar as configurações de inicialização. Verifique espaço, permissões e caminhos; a configuração anterior foi preservada.")]
    Storage,
    #[error("Feche o jogo e aguarde outras operações antes de salvar a configuração.")]
    WrongLock,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MemoryMode {
    #[default]
    Manual,
    Automatic,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GcMode {
    #[default]
    Runtime,
    Recommended,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchSettings {
    pub schema_version: u32,
    pub memory_mode: MemoryMode,
    pub min_memory_mb: Option<u32>,
    pub max_memory_mb: u32,
    pub gc_mode: GcMode,
    pub jvm_args: Vec<String>,
}

impl Default for LaunchSettings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            memory_mode: MemoryMode::Manual,
            min_memory_mb: None,
            max_memory_mb: 4096,
            gc_mode: GcMode::Runtime,
            jvm_args: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EffectiveLaunchSettings {
    pub min_memory_mb: Option<u32>,
    pub max_memory_mb: u32,
    pub jvm_args: Vec<String>,
}

impl EffectiveLaunchSettings {
    pub fn validate(&self) -> Result<(), LaunchSettingsError> {
        validate_memory(self.min_memory_mb, self.max_memory_mb)?;
        validate_arguments(&self.jvm_args)
    }
}

fn validate_memory(min: Option<u32>, max: u32) -> Result<(), LaunchSettingsError> {
    if !(MIN_MEMORY_MB..=MAX_MEMORY_MB).contains(&max)
        || min.is_some_and(|min| min < MIN_MEMORY_MB || min > max)
    {
        return Err(LaunchSettingsError::Memory);
    }
    Ok(())
}

fn is_collector(arg: &str) -> bool {
    matches!(
        arg,
        "-XX:+UseG1GC" | "-XX:+UseParallelGC" | "-XX:+UseSerialGC" | "-XX:+UseZGC"
    )
}

fn permitted_argument(arg: &str) -> bool {
    if arg.is_empty()
        || !arg.is_ascii()
        || arg.len() > 256
        || arg.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return false;
    }
    if is_collector(arg)
        || matches!(
            arg,
            "-ea"
                | "-da"
                | "-XX:+ParallelRefProcEnabled"
                | "-XX:-ParallelRefProcEnabled"
                | "-XX:+UseStringDeduplication"
                | "-XX:-UseStringDeduplication"
                | "-XX:+AlwaysPreTouch"
                | "-XX:-AlwaysPreTouch"
                | "-Dfile.encoding=UTF-8"
        )
    {
        return true;
    }
    if let Some(language) = arg.strip_prefix("-Duser.language=") {
        return (2..=3).contains(&language.len())
            && language.bytes().all(|c| c.is_ascii_lowercase());
    }
    if let Some(country) = arg.strip_prefix("-Duser.country=") {
        return country.len() == 2 && country.bytes().all(|c| c.is_ascii_uppercase());
    }
    if let Some(stack) = arg.strip_prefix("-Xss") {
        let (number, unit) = stack.split_at(stack.len().saturating_sub(1));
        return number.bytes().all(|c| c.is_ascii_digit())
            && number.parse::<u32>().ok().is_some_and(|n| match unit {
                "k" | "K" => (256..=16_384).contains(&n),
                "m" | "M" => (1..=16).contains(&n),
                _ => false,
            });
    }
    let Some((key, number)) = arg.strip_prefix("-XX:").and_then(|arg| arg.split_once('=')) else {
        return false;
    };
    let Some(value) = number
        .parse::<u32>()
        .ok()
        .filter(|_| number.bytes().all(|c| c.is_ascii_digit()))
    else {
        return false;
    };
    match key {
        "MaxGCPauseMillis" => (1..=10_000).contains(&value),
        "G1ReservePercent" => (0..=50).contains(&value),
        "InitiatingHeapOccupancyPercent" => (1..=100).contains(&value),
        "ParallelGCThreads" | "ConcGCThreads" => (1..=256).contains(&value),
        _ => false,
    }
}

fn validate_arguments(args: &[String]) -> Result<(), LaunchSettingsError> {
    if args.len() > 64 || args.iter().any(|arg| !permitted_argument(arg)) {
        return Err(LaunchSettingsError::Argument);
    }
    if args.iter().filter(|arg| is_collector(arg)).count() > 1 {
        return Err(LaunchSettingsError::CollectorConflict);
    }
    Ok(())
}

impl LaunchSettings {
    pub fn validate(&self) -> Result<(), LaunchSettingsError> {
        if self.schema_version != 1 {
            return Err(LaunchSettingsError::InvalidSettings);
        }
        validate_memory(self.min_memory_mb, self.max_memory_mb)?;
        if self.memory_mode == MemoryMode::Automatic && self.min_memory_mb.is_some() {
            return Err(LaunchSettingsError::Memory);
        }
        validate_arguments(&self.jvm_args)?;
        if self.gc_mode == GcMode::Recommended && self.jvm_args.len() >= 64 {
            return Err(LaunchSettingsError::Argument);
        }
        if self.gc_mode == GcMode::Recommended && self.jvm_args.iter().any(|arg| is_collector(arg))
        {
            return Err(LaunchSettingsError::CollectorConflict);
        }
        Ok(())
    }

    pub fn resolve(
        &self,
        total_memory_mb: Option<u32>,
        mod_count: u32,
        java_major: u32,
    ) -> Result<EffectiveLaunchSettings, LaunchSettingsError> {
        self.validate()?;
        let max_memory_mb = match self.memory_mode {
            MemoryMode::Manual => self.max_memory_mb,
            MemoryMode::Automatic => recommendation(total_memory_mb, mod_count)
                .ok_or(LaunchSettingsError::MemoryUnavailable)?,
        };
        if total_memory_mb.is_some_and(|total| max_memory_mb > total) {
            return Err(LaunchSettingsError::MemoryExceedsSystem);
        }
        let mut jvm_args = self.jvm_args.clone();
        // Conservative, explicit policy: leave legacy Java on its own defaults.
        // No claim that a GC flag improves a particular modpack without runtime evidence.
        if self.gc_mode == GcMode::Recommended && java_major >= 17 {
            jvm_args.push("-XX:+UseG1GC".into());
        }
        let effective = EffectiveLaunchSettings {
            min_memory_mb: self.min_memory_mb,
            max_memory_mb,
            jvm_args,
        };
        effective.validate()?;
        Ok(effective)
    }
}

pub fn recommendation(total_memory_mb: Option<u32>, mod_count: u32) -> Option<u32> {
    total_memory_mb
        .map(|total| recommended_memory_mb(total, mod_count).min(MAX_MEMORY_MB))
        .filter(|memory| *memory >= MIN_MEMORY_MB)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSettingsOverview {
    pub settings: LaunchSettings,
    pub total_memory_mb: Option<u32>,
    pub mod_count: u32,
    pub recommended_memory_mb: Option<u32>,
}

pub fn overview(
    instance: &Instance,
    settings: LaunchSettings,
) -> Result<LaunchSettingsOverview, LaunchSettingsError> {
    let total_memory_mb = physical_memory_mb();
    let mod_count = enabled_mod_count(instance)?;
    Ok(LaunchSettingsOverview {
        recommended_memory_mb: recommendation(total_memory_mb, mod_count),
        settings,
        total_memory_mb,
        mod_count,
    })
}

/// Broken launch preferences must block launch, but must not disable read-only diagnosis.
/// The fallback is explicitly labelled; it is never used to start a JVM.
pub fn diagnostic_memory(
    instance: &Instance,
    previous_memory_mb: Option<u32>,
) -> (u32, Option<&'static str>) {
    let resolved = read(instance).and_then(|settings| {
        settings.resolve(physical_memory_mb(), enabled_mod_count(instance)?, 0)
    });
    match resolved {
        Ok(effective) => (effective.max_memory_mb, None),
        Err(_) => (
            previous_memory_mb.filter(|value| (MIN_MEMORY_MB..=MAX_MEMORY_MB).contains(value)).unwrap_or(4096),
            Some("A configuração atual de RAM/JVM não pôde ser validada. Revise Java & engine antes de iniciar. A memória exibida usa a última tentativa registrada ou o padrão de 4096 MB e não representa a configuração atual."),
        ),
    }
}

fn checked_path(instance: &Instance) -> Result<PathBuf, LaunchSettingsError> {
    for ancestor in instance.root().ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(|_| LaunchSettingsError::Storage)?;
        if !metadata.is_dir() || metadata_is_link(&metadata) {
            return Err(LaunchSettingsError::Storage);
        }
    }
    ensure_no_symlink_components(instance.root(), SETTINGS_PATH)
        .map_err(|_| LaunchSettingsError::Storage)?;
    Ok(instance.root().join(SETTINGS_PATH))
}

pub fn read(instance: &Instance) -> Result<LaunchSettings, LaunchSettingsError> {
    let path = checked_path(instance)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = match options.open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LaunchSettings::default())
        }
        Err(_) => return Err(LaunchSettingsError::Storage),
    };
    let metadata = file.metadata().map_err(|_| LaunchSettingsError::Storage)?;
    if !metadata.is_file() || metadata_is_link(&metadata) || metadata.len() > MAX_BYTES {
        return Err(LaunchSettingsError::Storage);
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| LaunchSettingsError::Storage)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(LaunchSettingsError::Storage);
    }
    let settings: LaunchSettings =
        serde_json::from_slice(&bytes).map_err(|_| LaunchSettingsError::InvalidSettings)?;
    settings.validate()?;
    Ok(settings)
}

pub fn save(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    settings: &LaunchSettings,
) -> Result<(), LaunchSettingsError> {
    settings.validate()?;
    if instance
        .root()
        .canonicalize()
        .map_err(|_| LaunchSettingsError::Storage)?
        != guard.root
    {
        return Err(LaunchSettingsError::WrongLock);
    }
    let path = checked_path(instance)?;
    let parent = path.parent().ok_or(LaunchSettingsError::Storage)?;
    fs::create_dir_all(parent).map_err(|_| LaunchSettingsError::Storage)?;
    checked_path(instance)?;
    let temporary = parent.join(format!(
        "launch-settings-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| LaunchSettingsError::Storage)?;
        let bytes = serde_json::to_vec_pretty(settings)
            .map_err(|_| LaunchSettingsError::InvalidSettings)?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| LaunchSettingsError::Storage)?;
        drop(file);
        checked_path(instance)?;
        atomic_replace(&temporary, &path).map_err(|_| LaunchSettingsError::Storage)
    })();
    // Only this unique staging file is ours. A failed publication leaves the old file intact.
    let _ = fs::remove_file(&temporary);
    result
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Both NUL-terminated buffers remain alive; paths are local, checked and on the same volume.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0x1 | 0x8) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

pub fn enabled_mod_count(instance: &Instance) -> Result<u32, LaunchSettingsError> {
    checked_path(instance)?;
    ensure_no_symlink_components(instance.root(), "mods")
        .map_err(|_| LaunchSettingsError::Storage)?;
    let entries = match fs::read_dir(instance.mods_dir()) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(_) => return Err(LaunchSettingsError::Storage),
    };
    let mut count = 0;
    for (index, entry) in entries.enumerate() {
        if index >= 10_000 {
            return Err(LaunchSettingsError::Storage);
        }
        let entry = entry.map_err(|_| LaunchSettingsError::Storage)?;
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|_| LaunchSettingsError::Storage)?;
        if metadata.is_file()
            && !metadata_is_link(&metadata)
            && entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
        {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(windows)]
pub fn physical_memory_mb() -> Option<u32> {
    #[repr(C)]
    struct MemoryStatus {
        length: u32,
        load: u32,
        total_physical: u64,
        available_physical: u64,
        total_page: u64,
        available_page: u64,
        total_virtual: u64,
        available_virtual: u64,
        available_extended: u64,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatus) -> i32;
    }
    let mut status = MemoryStatus {
        length: std::mem::size_of::<MemoryStatus>() as u32,
        load: 0,
        total_physical: 0,
        available_physical: 0,
        total_page: 0,
        available_page: 0,
        total_virtual: 0,
        available_virtual: 0,
        available_extended: 0,
    };
    // Windows writes exactly this repr(C) structure after verifying its length.
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return None;
    }
    u32::try_from(status.total_physical / (1024 * 1024))
        .ok()
        .filter(|total| *total > 0)
}

#[cfg(not(windows))]
pub fn physical_memory_mb() -> Option<u32> {
    None
}

#[cfg(test)]
#[path = "launch_settings_tests.rs"]
mod tests;
