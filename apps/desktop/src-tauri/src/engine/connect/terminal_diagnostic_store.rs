//! Private bounded storage, using the existing Recovery filesystem guards.
use super::ConnectTerminalDiagnostic;
use crate::engine::recovery;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
const MAX_BYTES: usize = 4096;
fn validate_bytes(bytes: &[u8]) -> Result<(), ()> {
    if bytes.len() > MAX_BYTES {
        return Err(());
    }
    let value = serde_json::from_slice(bytes).map_err(|_| ())?;
    ConnectTerminalDiagnostic::valid_stored(&value)
        .then_some(())
        .ok_or(())
}
fn check_existing(path: &Path) -> Result<(), ()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(()),
        Ok(_) => {}
    }
    let file = recovery::secure_open(path).map_err(|_| ())?;
    if file.metadata().map_err(|_| ())?.len() > MAX_BYTES as u64 {
        return Err(());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    validate_bytes(&bytes)
}
fn create_pinned_directory(directory: &Path) -> Result<Vec<File>, ()> {
    let mut missing = Vec::new();
    let mut current = directory;
    let mut pins = loop {
        match fs::symlink_metadata(current) {
            Ok(_) => break recovery::pin_directory(current).map_err(|_| ())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(current);
                current = current.parent().ok_or(())?;
            }
            Err(_) => return Err(()),
        }
    };
    for component in missing.into_iter().rev() {
        match fs::create_dir(component) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(()),
        }
        // If another writer supplied a junction at this component, reject it
        // before ever creating the next child. Never recurse through it.
        pins.extend(recovery::pin_directory(component).map_err(|_| ())?);
    }
    Ok(pins)
}
pub(super) fn persist(directory: &Path, bytes: &[u8]) -> Result<(), ()> {
    validate_bytes(bytes)?;
    let _pins = create_pinned_directory(directory)?;
    let destination = directory.join("last-terminal.json");
    check_existing(&destination)?;
    let temporary = directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x0020_0000).share_mode(1);
        }
        let mut file = options.open(&temporary).map_err(|_| ())?;
        file.write_all(bytes).map_err(|_| ())?;
        file.sync_all().map_err(|_| ())?;
        drop(file);
        check_existing(&destination)?;
        fs::rename(&temporary, &destination).map_err(|_| ())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
#[path = "terminal_diagnostic_store_tests.rs"]
mod tests;
