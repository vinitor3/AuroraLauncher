//! Read-only preferences for the selected ready copy, never its source.
use super::*;
use crate::engine::recovery;
use sha2::{Digest, Sha256};

pub struct ReadyLaunchSettings {
    effective: EffectiveLaunchSettings,
    fingerprint: String,
    instance: Instance,
    file: Option<recovery::SecureFile>,
    _directories: Vec<fs::File>,
}

impl ReadyLaunchSettings {
    pub fn effective(&self) -> &EffectiveLaunchSettings {
        &self.effective
    }

    /// The existing ReadyGame digest binds this combined private fingerprint.
    pub fn bind_runtime_fingerprint(&self, runtime: &str) -> String {
        let mut hash = Sha256::new();
        hash.update(b"aurora-ready-game-launch-settings-v1\0");
        hash.update((runtime.len() as u64).to_be_bytes());
        hash.update(runtime.as_bytes());
        hash.update(self.fingerprint.as_bytes());
        format!("{:x}", hash.finalize())
    }

    pub fn recheck_before_spawn(&self) -> Result<(), LaunchSettingsError> {
        let path = checked_path(&self.instance)?;
        match &self.file {
            Some(file) => {
                if !recovery::single_link(file).map_err(|_| LaunchSettingsError::Storage)? {
                    return Err(LaunchSettingsError::Storage);
                }
                // Windows retains a non-writable/non-replaceable file handle.
                // Recheck the path as well; no new preferences are adopted here.
                let metadata =
                    fs::symlink_metadata(path).map_err(|_| LaunchSettingsError::Storage)?;
                if !metadata.is_file() || metadata_is_link(&metadata) {
                    return Err(LaunchSettingsError::Storage);
                }
            }
            None => match fs::symlink_metadata(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err(LaunchSettingsError::Storage),
            },
        }
        Ok(())
    }
}

pub fn inspect_ready_settings(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    java_major: u32,
    additional_mods: u32,
) -> Result<ReadyLaunchSettings, LaunchSettingsError> {
    inspect_with_memory(
        instance,
        guard,
        java_major,
        additional_mods,
        physical_memory_mb(),
    )
}

fn inspect_with_memory(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    java_major: u32,
    additional_mods: u32,
    total_memory_mb: Option<u32>,
) -> Result<ReadyLaunchSettings, LaunchSettingsError> {
    let directories =
        recovery::check_guard(instance, guard).map_err(|_| LaunchSettingsError::WrongLock)?;
    let path = checked_path(instance)?;
    let (settings, bytes, file) = match fs::symlink_metadata(&path) {
        Ok(_) => {
            let mut file =
                recovery::secure_open(&path).map_err(|_| LaunchSettingsError::Storage)?;
            if file
                .metadata()
                .map_err(|_| LaunchSettingsError::Storage)?
                .len()
                > MAX_BYTES
            {
                return Err(LaunchSettingsError::Storage);
            }
            let mut bytes = Vec::new();
            (&mut file)
                .take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| LaunchSettingsError::Storage)?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err(LaunchSettingsError::Storage);
            }
            let settings: LaunchSettings =
                serde_json::from_slice(&bytes).map_err(|_| LaunchSettingsError::InvalidSettings)?;
            (settings, bytes, Some(file))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (LaunchSettings::default(), Vec::new(), None)
        }
        Err(_) => return Err(LaunchSettingsError::Storage),
    };
    let count = enabled_mod_count(instance)?
        .checked_add(additional_mods)
        .filter(|count| *count <= 10_000)
        .ok_or(LaunchSettingsError::Storage)?;
    let effective = settings.resolve(total_memory_mb, count, java_major)?;
    let mut hash = Sha256::new();
    hash.update(b"aurora-ready-copy-preferences-v1\0");
    hash.update([u8::from(file.is_some())]);
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
    hash.update(serde_json::to_vec(&effective).map_err(|_| LaunchSettingsError::InvalidSettings)?);
    let observed = ReadyLaunchSettings {
        effective,
        fingerprint: format!("{:x}", hash.finalize()),
        instance: instance.clone(),
        file,
        _directories: directories,
    };
    observed.recheck_before_spawn()?;
    Ok(observed)
}

#[cfg(test)]
#[path = "launch_settings_ready_tests.rs"]
mod tests;
