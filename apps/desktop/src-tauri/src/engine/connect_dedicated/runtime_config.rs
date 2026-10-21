//! Bounded runtime reads for the two compiled configuration paths. This module
//! has no run/source, mutation guard, database, inventory or restore authority.
// Experimental default-off foundation; the native source consumer arrives in
// C1. Keep test lints enabled and reconsider this scope when that is integrated.
#![cfg_attr(not(test), allow(dead_code))]

use super::{manifest::StoredVoiceProfileV2, recovery, FileIdentity};
use crate::engine::recovery::RecoveryError;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Component, Path, PathBuf, Prefix},
};

const MAX_BYTES: usize = 256 * 1024;
const REPARSE_POINT: u32 = 0x400;
const DIRECTORY: u32 = 0x10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RuntimeConfigError {
    Missing,
    UnsafePath,
    Limit,
    Incompatible,
    Changed,
    Io,
}
type Result<T> = std::result::Result<T, RuntimeConfigError>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileStamp {
    identity: FileIdentity,
    bytes: u64,
    created: u64,
    written: u64,
    attributes: u32,
}
impl FileStamp {
    fn capture(file: &File) -> Result<Self> {
        #[repr(C)]
        struct Info {
            values: [u32; 13],
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut Info) -> i32;
        }
        let mut info = Info { values: [0; 13] };
        // SAFETY: BY_HANDLE_FILE_INFORMATION is exactly thirteen DWORDs. The
        // borrowed File owns this handle and the initialized output lives here.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(RuntimeConfigError::Io);
        }
        let values = info.values;
        if values[0] & (REPARSE_POINT | DIRECTORY) != 0
            || values[10] != 1
            || !file.metadata().map_err(io_error)?.is_file()
        {
            return Err(RuntimeConfigError::UnsafePath);
        }
        let pair = |low: u32, high: u32| u64::from(low) | (u64::from(high) << 32);
        Ok(Self {
            identity: FileIdentity {
                volume: values[7],
                index: pair(values[12], values[11]),
            },
            bytes: pair(values[9], values[8]),
            created: pair(values[1], values[2]),
            written: pair(values[5], values[6]),
            attributes: values[0],
        })
    }

    fn recheck(file: &File) -> Result<Self> {
        // Only for the same handle after a successful initial capture. A
        // transition to an unsafe state (including delete-pending link count)
        // is a change, never authority to read/retry a replacement pathname.
        Self::capture(file).map_err(|error| match error {
            RuntimeConfigError::UnsafePath => RuntimeConfigError::Changed,
            other => other,
        })
    }
}

/// Data only. No serde, bytes, path, handles, pins or authority can escape a read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RuntimeConfigSample {
    profile: StoredVoiceProfileV2,
    stamp: FileStamp,
    sha256: [u8; 32],
}
impl RuntimeConfigSample {
    pub(super) fn require_same_revision(&self, other: &Self) -> Result<()> {
        if self == other {
            Ok(())
        } else {
            Err(RuntimeConfigError::Changed)
        }
    }
}

pub(super) struct PinnedRuntimeConfiguration {
    path: PathBuf,
    profile: StoredVoiceProfileV2,
    _parents: Vec<File>,
}
impl PinnedRuntimeConfiguration {
    pub(super) fn pin(
        copy_root: &Path,
        expected_root_identity: &FileIdentity,
        profile: StoredVoiceProfileV2,
    ) -> Result<Self> {
        internal_copy_path(copy_root)?;
        let mut parents = recovery::pin_directory(copy_root).map_err(path_error)?;
        let root = parents.first().ok_or(RuntimeConfigError::UnsafePath)?;
        if super::file_identity(root).map_err(|_| RuntimeConfigError::Io)?
            != *expected_root_identity
        {
            return Err(RuntimeConfigError::Changed);
        }
        // pin_directory is used ONLY as an existing read-only path/OS-handle
        // primitive. No Recovery observation, inventory, guard or Store runs.
        // Pin each fixed parent before descending through the next component.
        let directories: &[&str] = match profile {
            StoredVoiceProfileV2::Svc => &["config", "config/voicechat"],
            StoredVoiceProfileV2::Plasmo => {
                &["config", "config/plasmovoice", "config/plasmovoice/server"]
            }
        };
        for directory in directories {
            use std::os::windows::fs::MetadataExt;
            let parent = copy_root.join(directory);
            let metadata = fs::symlink_metadata(&parent).map_err(io_error)?;
            if !metadata.is_dir() || metadata.file_attributes() & REPARSE_POINT != 0 {
                return Err(RuntimeConfigError::UnsafePath);
            }
            parents.extend(recovery::pin_directory(&parent).map_err(path_error)?);
        }
        let path = copy_root.join(profile.configuration_name());
        Ok(Self {
            path,
            profile,
            _parents: parents,
        })
    }

    pub(super) fn read_sample(self) -> Result<RuntimeConfigSample> {
        #[cfg(test)]
        {
            self.read_inner(ReadHooks::default())
        }
        #[cfg(not(test))]
        {
            self.read_inner()
        }
    }

    fn read_inner(self, #[cfg(test)] hooks: ReadHooks) -> Result<RuntimeConfigSample> {
        let mut first = open_read(&self.path)?;
        let before = FileStamp::capture(&first)?;
        if before.bytes > MAX_BYTES as u64 {
            return Err(RuntimeConfigError::Limit);
        }
        #[cfg(test)]
        if let Some(gate) = &hooks.after_open {
            gate.arrive();
        }
        #[cfg(test)]
        if hooks.panic_after_open {
            panic!("runtime_config_test_panic_after_open");
        }
        let mut bytes = Vec::new();
        (&mut first)
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        if bytes.len() > MAX_BYTES {
            return Err(RuntimeConfigError::Limit);
        }
        let after = FileStamp::recheck(&first)?;
        if before != after || bytes.len() as u64 != before.bytes {
            return Err(RuntimeConfigError::Changed);
        }
        // The first handle MUST stay alive while the same pathname is opened
        // again. A stable old handle alone cannot detect a real leaf rename.
        let pathname = open_read(&self.path).map_err(|error| {
            if error == RuntimeConfigError::Missing {
                RuntimeConfigError::Changed
            } else {
                error
            }
        })?;
        let current = FileStamp::capture(&pathname)?;
        let first_final = FileStamp::recheck(&first)?;
        if before != current || before != first_final {
            return Err(RuntimeConfigError::Changed);
        }
        drop(pathname);
        drop(first);
        validate_configuration(self.profile, &bytes)?;
        let sample = RuntimeConfigSample {
            profile: self.profile,
            stamp: before,
            sha256: Sha256::digest(&bytes).into(),
        };
        drop(bytes);
        // Consuming self also closes ALL directory pins before data returns.
        drop(self);
        Ok(sample)
    }
}

fn io_error(error: std::io::Error) -> RuntimeConfigError {
    if error.kind() == std::io::ErrorKind::NotFound {
        RuntimeConfigError::Missing
    } else {
        RuntimeConfigError::Io
    }
}
fn path_error(error: RecoveryError) -> RuntimeConfigError {
    match error {
        RecoveryError::UnsafePath => RuntimeConfigError::UnsafePath,
        RecoveryError::Limit => RuntimeConfigError::Limit,
        _ => RuntimeConfigError::Io,
    }
}
fn open_read(path: &Path) -> Result<File> {
    use std::os::windows::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.is_file() || metadata.file_attributes() & REPARSE_POINT != 0 {
        return Err(RuntimeConfigError::UnsafePath);
    }
    OpenOptions::new()
        .read(true)
        .custom_flags(0x0020_0000)
        .share_mode(7)
        .open(path)
        .map_err(io_error)
}
fn internal_copy_path(path: &Path) -> Result<()> {
    if path.as_os_str().len() > 1024 {
        return Err(RuntimeConfigError::Limit);
    }
    let mut parts = path.components();
    if !matches!(parts.next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
        || !matches!(parts.next(), Some(Component::RootDir))
    {
        return Err(RuntimeConfigError::UnsafePath);
    }
    for part in parts {
        let Component::Normal(name) = part else {
            return Err(RuntimeConfigError::UnsafePath);
        };
        let name = name.to_str().ok_or(RuntimeConfigError::UnsafePath)?;
        let stem = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if name.is_empty()
            || name.contains([':', '\0', '<', '>', '"', '|', '?', '*'])
            || name.ends_with(['.', ' '])
            || [
                "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
                "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
                "LPT9",
            ]
            .contains(&stem.as_str())
        {
            return Err(RuntimeConfigError::UnsafePath);
        }
    }
    Ok(())
}
fn validate_configuration(profile: StoredVoiceProfileV2, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_BYTES {
        return Err(RuntimeConfigError::Limit);
    }
    let parsed = match profile {
        StoredVoiceProfileV2::Svc => validate_svc(bytes),
        StoredVoiceProfileV2::Plasmo => super::super::plasmo::validate_runtime_configuration(bytes),
    };
    parsed.map_err(|error| match error {
        RecoveryError::Limit => RuntimeConfigError::Limit,
        _ => RuntimeConfigError::Incompatible,
    })
}
fn validate_svc(bytes: &[u8]) -> std::result::Result<(), RecoveryError> {
    // Preserve 58/59 exactly: the old lexer first validates the complete input.
    // This additional finite pass checks only this runtime profile's fields.
    let announcement = super::super::svc_value_span(bytes)?;
    if !bytes[announcement].is_empty() {
        return Err(RecoveryError::Integrity);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| RecoveryError::Integrity)?;
    let mut port = false;
    let mut bind = false;
    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches('\n').trim_end_matches('\r');
        let trimmed = line.trim_start_matches([' ', '\t']);
        if trimmed.is_empty() || trimmed.starts_with(['#', '!']) {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or(RecoveryError::Integrity)?;
        let value = value.trim_start_matches([' ', '\t']);
        match key.trim_matches([' ', '\t']) {
            "port" => {
                if value != "43002" {
                    return Err(RecoveryError::Integrity);
                }
                port = true;
            }
            "bind_address" => {
                if value != "127.0.0.1" {
                    return Err(RecoveryError::Integrity);
                }
                bind = true;
            }
            _ => {}
        }
    }
    if port && bind {
        Ok(())
    } else {
        Err(RecoveryError::Integrity)
    }
}

#[cfg(test)]
#[derive(Default)]
struct ReadHooks {
    after_open: Option<std::sync::Arc<tests::ReadGate>>,
    panic_after_open: bool,
}

#[cfg(test)]
#[path = "runtime_config/tests.rs"]
mod tests;
