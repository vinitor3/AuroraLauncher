//! Shared heap limit. Replacing all overrides changes a revision in one atomic write.
use super::*;
use fs2::FileExt;

const GLOBAL_PATH: &str = "launcher-memory.json";
const LOCK_PATH: &str = "launcher-memory.lock";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct GlobalMemory {
    pub(super) max_memory_mb: u32,
    revision: Option<uuid::Uuid>,
}

impl Default for GlobalMemory {
    fn default() -> Self {
        Self {
            max_memory_mb: LaunchSettings::default().max_memory_mb,
            revision: None,
        }
    }
}

impl GlobalMemory {
    pub(super) fn apply(&self, mut settings: LaunchSettings) -> LaunchSettings {
        if settings.memory_mode == MemoryMode::Launcher || settings.memory_revision != self.revision
        {
            settings.memory_mode = MemoryMode::Launcher;
            settings.max_memory_mb = self.max_memory_mb;
            settings.min_memory_mb = None;
        }
        settings.memory_revision = self.revision;
        settings
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalMemoryOverview {
    pub memory_revision: Option<uuid::Uuid>,
    pub max_memory_mb: u32,
    pub total_memory_mb: Option<u32>,
}

pub(super) fn load(root: &Path) -> Result<GlobalMemory, LaunchSettingsError> {
    // Before the first instance is created the launcher's data directory can be absent.
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(GlobalMemory::default())
        }
        Err(_) => return Err(LaunchSettingsError::Storage),
        Ok(_) => {}
    }
    let path = checked_settings_path(root, GLOBAL_PATH)?;
    let value: GlobalMemory = read_json(&path)?.unwrap_or_default();
    validate_memory(None, value.max_memory_mb)?;
    Ok(value)
}

pub fn global_memory(root: &Path) -> Result<GlobalMemoryOverview, LaunchSettingsError> {
    let value = load(root)?;
    Ok(GlobalMemoryOverview {
        memory_revision: value.revision,
        max_memory_mb: value.max_memory_mb,
        total_memory_mb: physical_memory_mb(),
    })
}

pub fn save_global_memory(
    root: &Path,
    max_memory_mb: u32,
    replace_overrides: bool,
) -> Result<GlobalMemoryOverview, LaunchSettingsError> {
    validate_memory(None, max_memory_mb)?;
    let total_memory_mb = physical_memory_mb();
    if total_memory_mb.is_some_and(|total| max_memory_mb > total) {
        return Err(LaunchSettingsError::MemoryExceedsSystem);
    }
    // Validate the closest existing ancestor before creating a first-run data directory.
    let ancestor = root
        .ancestors()
        .find(|path| path.exists())
        .ok_or(LaunchSettingsError::Storage)?;
    check_root(ancestor)?;
    fs::create_dir_all(root).map_err(|_| LaunchSettingsError::Storage)?;
    let lock_path = checked_settings_path(root, LOCK_PATH)?;
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    let lock = options
        .open(lock_path)
        .map_err(|_| LaunchSettingsError::Storage)?;
    let metadata = lock.metadata().map_err(|_| LaunchSettingsError::Storage)?;
    if !metadata.is_file() || metadata_is_link(&metadata) {
        return Err(LaunchSettingsError::Storage);
    }
    lock.try_lock_exclusive()
        .map_err(|_| LaunchSettingsError::Storage)?;
    let revision = if replace_overrides {
        Some(uuid::Uuid::new_v4())
    } else {
        load(root)?.revision
    };
    write_json(
        root,
        GLOBAL_PATH,
        &GlobalMemory {
            max_memory_mb,
            revision,
        },
    )?;
    Ok(GlobalMemoryOverview {
        memory_revision: revision,
        max_memory_mb,
        total_memory_mb,
    })
}

pub fn prepare_save(
    instance: &Instance,
    settings: LaunchSettings,
) -> Result<LaunchSettingsOverview, LaunchSettingsError> {
    let global = load(instance.data_dir())?;
    if settings.memory_revision != global.revision {
        return Err(LaunchSettingsError::MemoryRevisionChanged);
    }
    super::overview_with_global(instance, settings, &global)
}
