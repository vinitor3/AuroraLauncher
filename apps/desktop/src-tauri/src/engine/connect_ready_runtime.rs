//! Read-only readiness proof for one existing diagnostic copy. Kept distinct
//! from installation and repair. Pins live only while the caller holds its lock.
use super::{
    connect::ReadyGameLaunchError as Error,
    connect_quick_play, connect_ready_files,
    connect_ready_profile::{self, ReadyProfile},
    diagnostic_lab, recovery, Instance, InstanceMutationGuard, VersionLaunchSpec,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
};

pub struct ReadyRuntime {
    pub profile: ReadyProfile,
    pub fingerprint: String,
    pub core_sha256: String,
    pub skins_sha256: String,
    pub spec: VersionLaunchSpec,
    root: PathBuf,
    directory_observations: Vec<(&'static str, Vec<String>)>,
    deleting_marker: PathBuf,
    _pins: Vec<recovery::SecureFile>,
    _directories: Vec<fs::File>,
}
fn directory_names(root: &Path, relative: &str) -> Result<Vec<String>, Error> {
    let directory = recovery::path(root, relative).map_err(|_| Error::InstanceChanged)?;
    let mut names = Vec::new();
    for entry in fs::read_dir(directory).map_err(|_| Error::InstanceChanged)? {
        let entry = entry.map_err(|_| Error::InstanceChanged)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| Error::InstanceChanged)?;
        let path = recovery::path(root, &format!("{relative}/{name}"))
            .map_err(|_| Error::InstanceChanged)?;
        if names.len() >= 256
            || !fs::symlink_metadata(path)
                .map_err(|_| Error::InstanceChanged)?
                .is_file()
        {
            return Err(Error::InstanceChanged);
        }
        names.push(name);
    }
    names.sort();
    Ok(names)
}
impl ReadyRuntime {
    /// Existing files remain pinned, but a directory pin does not forbid new
    /// children. Recheck its bounded names immediately before the spawn gate.
    /// This is still cooperative file isolation, never an OS sandbox.
    pub fn recheck_before_spawn(&self) -> Result<(), Error> {
        if self
            .deleting_marker
            .try_exists()
            .map_err(|_| Error::InstanceChanged)?
        {
            return Err(Error::InstanceChanged);
        }
        for (relative, expected) in &self.directory_observations {
            if &directory_names(&self.root, relative)? != expected {
                return Err(Error::InstanceChanged);
            }
        }
        Ok(())
    }
}
struct Reader<'a> {
    cancel: &'a dyn Fn() -> bool,
    pins: Vec<recovery::SecureFile>,
    hashes: Vec<(String, u64, String)>,
    bytes: u64,
}
impl Reader<'_> {
    fn read(&mut self, path: &Path, label: String, limit: u64) -> Result<Vec<u8>, Error> {
        if (self.cancel)() {
            return Err(Error::Cancelled);
        }
        if self.pins.len() >= 32_768 {
            return Err(Error::MissingArtifact);
        }
        let mut file = recovery::secure_open(path).map_err(|_| Error::MissingArtifact)?;
        let length = file.metadata().map_err(|_| Error::MissingArtifact)?.len();
        self.bytes = self
            .bytes
            .checked_add(length)
            .ok_or(Error::MissingArtifact)?;
        if length > limit || self.bytes > 4 * 1024 * 1024 * 1024 {
            return Err(Error::MissingArtifact);
        }
        let mut bytes = Vec::with_capacity(length as usize);
        let mut buffer = [0; 64 * 1024];
        loop {
            if (self.cancel)() {
                return Err(Error::Cancelled);
            }
            let count = file.read(&mut buffer).map_err(|_| Error::MissingArtifact)?;
            if count == 0 {
                break;
            }
            if bytes.len() as u64 + count as u64 > length {
                return Err(Error::InstanceChanged);
            }
            bytes.extend_from_slice(&buffer[..count]);
        }
        if bytes.len() as u64 != length {
            return Err(Error::InstanceChanged);
        }
        self.hashes
            .push((label, length, format!("{:x}", Sha256::digest(&bytes))));
        self.pins.push(file);
        Ok(bytes)
    }
    fn instance(
        &mut self,
        instance: &Instance,
        relative: &str,
        limit: u64,
    ) -> Result<Vec<u8>, Error> {
        let path =
            recovery::path(instance.root(), relative).map_err(|_| Error::InstanceNotReady)?;
        self.read(&path, relative.into(), limit)
    }
}

/// Verify all inputs before returning a prepared spec and retained read handles.
/// No directories, identity, recovery database, preferences or JARs are created.
#[allow(clippy::too_many_arguments)]
pub fn inspect_ready_runtime(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    source_id: &str,
    java: &Path,
    skin_file: Option<&Path>,
    endpoint: SocketAddr,
    observed_java: u32,
    cancelled: impl Fn() -> bool,
) -> Result<ReadyRuntime, Error> {
    if cancelled() {
        return Err(Error::Cancelled);
    }
    if !diagnostic_lab::is_lab_candidate(instance) {
        return Err(Error::InstanceNotReady);
    }
    diagnostic_lab::ensure_launchable(instance).map_err(|_| Error::InstanceNotReady)?;
    let data = instance
        .root()
        .parent()
        .and_then(Path::parent)
        .ok_or(Error::InstanceNotReady)?;
    if diagnostic_lab::origin_instance(data, instance)
        .map_err(|_| Error::InstanceNotReady)?
        .id()
        .as_str()
        != source_id
    {
        return Err(Error::InstanceNotReady);
    }
    let observation =
        recovery::with_existing_observation(instance, guard, |value| Ok(value.clone())).map_err(
            |error| {
                if matches!(error, recovery::RecoveryError::Pending) {
                    Error::RecoveryPending
                } else {
                    Error::InstanceNotReady
                }
            },
        )?;
    if super::companion_migration_required(instance).map_err(|_| Error::ArtifactMismatch)? {
        return Err(Error::ArtifactMismatch);
    }
    let mut reader = Reader {
        cancel: &cancelled,
        pins: Vec::new(),
        hashes: Vec::new(),
        bytes: 0,
    };
    // Retain provenance and protected configuration observations too. A new
    // recovery transaction cannot replace these files while the proof is held.
    reader.instance(instance, ".aurora/recovery-identity.json", 1024 * 1024)?;
    let copy_uuid = instance
        .id()
        .as_str()
        .strip_prefix("aurora-test-")
        .ok_or(Error::InstanceNotReady)?;
    let deleting_marker = recovery::path(
        data,
        &format!(".aurora/diagnostic-lab/{copy_uuid}/deleting.json"),
    )
    .map_err(|_| Error::InstanceNotReady)?;
    let directory_observations = vec![
        ("mods", directory_names(instance.root(), "mods")?),
        ("natives", directory_names(instance.root(), "natives")?),
    ];
    for marker in ["record.json", "ready.json"] {
        let relative = format!(".aurora/diagnostic-lab/{copy_uuid}/{marker}");
        let path = recovery::path(data, &relative).map_err(|_| Error::InstanceNotReady)?;
        reader.read(&path, format!("provenance/{marker}"), 64 * 1024 * 1024)?;
    }
    let origin =
        diagnostic_lab::origin_instance(data, instance).map_err(|_| Error::InstanceNotReady)?;
    let origin_identity = recovery::path(origin.root(), ".aurora/recovery-identity.json")
        .map_err(|_| Error::InstanceNotReady)?;
    reader.read(
        &origin_identity,
        "origin-recovery-identity".into(),
        1024 * 1024,
    )?;
    for file in &observation.files {
        let bytes = reader.instance(instance, &file.path, 256 * 1024 * 1024)?;
        if bytes.len() as u64 != file.size || format!("{:x}", Sha256::digest(&bytes)) != file.sha256
        {
            return Err(Error::InstanceChanged);
        }
    }
    let mut directories = Vec::new();
    for relative in [
        "mods",
        "natives",
        "resourcepacks",
        "shaderpacks",
        "logs",
        "versions",
        "libraries",
        "assets",
    ] {
        let directory =
            recovery::path(instance.root(), relative).map_err(|_| Error::InstanceNotReady)?;
        if !directory.is_dir() {
            return Err(Error::MissingArtifact);
        }
        directories
            .extend(recovery::pin_directory(&directory).map_err(|_| Error::InstanceNotReady)?);
    }
    let mut root_profiles = Vec::new();
    for relative in ["aurora-instance.json", "aurora-modpack.json"] {
        let path =
            recovery::path(instance.root(), relative).map_err(|_| Error::InstanceNotReady)?;
        if path.exists() {
            let bytes = reader.instance(instance, relative, 1024 * 1024)?;
            let metadata: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| Error::UnsupportedRuntime)?;
            root_profiles.push(metadata);
        }
    }
    let profile = connect_ready_profile::profile_from_metadata(&root_profiles)?;
    for version in [profile.descriptor().minecraft_version, profile.version_id()] {
        reader.instance(
            instance,
            &format!("versions/{version}/{version}.json"),
            1024 * 1024,
        )?;
    }
    let mut spec = super::resolve_launch_spec(instance, profile.version_id())
        .map_err(|_| Error::UnsupportedRuntime)?;
    connect_quick_play::apply_guest_quick_play(instance, &mut spec, observed_java, endpoint)
        .map_err(|error| match error {
            connect_quick_play::QuickPlayError::ConflictingArguments => Error::ConflictingArguments,
            _ => Error::UnsupportedRuntime,
        })?;
    let managed =
        connect_ready_files::verify_managed_files_for_profile(instance, profile, &cancelled)
            .map_err(|e| match e {
                connect_ready_files::ReadyFilesError::Cancelled => Error::Cancelled,
                connect_ready_files::ReadyFilesError::Missing => Error::MissingArtifact,
                _ => Error::ArtifactMismatch,
            })?;
    for file in &managed.fingerprints {
        let bytes = reader.instance(instance, &file.relative, 128 * 1024 * 1024)?;
        if bytes.len() as u64 != file.bytes
            || format!("{:x}", Sha256::digest(&bytes)) != file.sha256
        {
            return Err(Error::InstanceChanged);
        }
    }
    let mut paths = BTreeSet::new();
    for path in spec
        .libraries
        .iter()
        .chain(std::iter::once(&spec.client_jar))
    {
        let relative = path
            .strip_prefix(instance.root())
            .map_err(|_| Error::UnsupportedRuntime)?
            .to_str()
            .ok_or(Error::UnsupportedRuntime)?
            .replace('\\', "/");
        if !paths.insert(relative.clone()) {
            return Err(Error::UnsupportedRuntime);
        }
        reader.instance(instance, &relative, 256 * 1024 * 1024)?;
    }
    if spec.assets_dir != instance.root().join("assets")
        || spec.asset_index.is_empty()
        || spec.asset_index.len() > 64
        || !spec
            .asset_index
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err(Error::UnsupportedRuntime);
    }
    let index = reader.instance(
        instance,
        &format!("assets/indexes/{}.json", spec.asset_index),
        16 * 1024 * 1024,
    )?;
    let index: serde_json::Value =
        serde_json::from_slice(&index).map_err(|_| Error::MissingArtifact)?;
    let objects = index["objects"].as_object().ok_or(Error::MissingArtifact)?;
    if objects.is_empty() || objects.len() > 30_000 {
        return Err(Error::MissingArtifact);
    }
    let mut assets = BTreeSet::new();
    for object in objects.values() {
        let hash = object["hash"]
            .as_str()
            .filter(|h| {
                h.len() == 40
                    && h.bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            })
            .ok_or(Error::MissingArtifact)?;
        let size = object["size"].as_u64().ok_or(Error::MissingArtifact)?;
        if assets.insert(hash.to_owned()) {
            let bytes = reader.instance(
                instance,
                &format!("assets/objects/{}/{hash}", &hash[..2]),
                64 * 1024 * 1024,
            )?;
            if bytes.len() as u64 != size || format!("{:x}", sha1::Sha1::digest(&bytes)) != hash {
                return Err(Error::ArtifactMismatch);
            }
        }
    }
    let natives = instance.natives_dir();
    let mut native_paths = Vec::<PathBuf>::new();
    for entry in fs::read_dir(&natives).map_err(|_| Error::MissingArtifact)? {
        let path = entry.map_err(|_| Error::MissingArtifact)?.path();
        if native_paths.len() >= 256 || !path.is_file() {
            return Err(Error::MissingArtifact);
        }
        native_paths.push(path);
    }
    if native_paths.is_empty() {
        return Err(Error::MissingArtifact);
    }
    native_paths.sort();
    for path in native_paths {
        let relative = path
            .strip_prefix(instance.root())
            .map_err(|_| Error::MissingArtifact)?
            .to_str()
            .ok_or(Error::MissingArtifact)?
            .replace('\\', "/");
        reader.instance(instance, &relative, 64 * 1024 * 1024)?;
    }
    if !java.is_absolute() {
        return Err(Error::JavaUnavailable);
    }
    reader
        .read(java, "selected-java-executable".into(), 64 * 1024 * 1024)
        .map_err(|e| {
            if e == Error::Cancelled {
                e
            } else {
                Error::JavaUnavailable
            }
        })?;
    let java_home = java
        .parent()
        .and_then(Path::parent)
        .ok_or(Error::JavaUnavailable)?;
    for relative in [
        "release",
        "lib/modules",
        "bin/server/jvm.dll",
        "bin/java.dll",
        "bin/jli.dll",
    ] {
        let path = recovery::path(java_home, relative).map_err(|_| Error::JavaUnavailable)?;
        reader
            .read(&path, format!("java-runtime/{relative}"), 512 * 1024 * 1024)
            .map_err(|e| {
                if e == Error::Cancelled {
                    e
                } else {
                    Error::JavaUnavailable
                }
            })?;
    }
    if let Some(path) = skin_file {
        reader.read(path, "selected-local-skin".into(), 5 * 1024 * 1024)?;
    }
    if cancelled() {
        return Err(Error::Cancelled);
    }
    reader.hashes.sort();
    let mut hash = Sha256::new();
    hash.update(b"aurora-connect-ready-runtime-v1\0");
    hash.update(
        serde_json::to_vec(&(
            observation.instance_uuid,
            observation.fingerprint,
            source_id,
            profile.descriptor(),
            reader.hashes,
            &managed,
            &directory_observations,
        ))
        .map_err(|_| Error::Internal)?,
    );
    let proof = ReadyRuntime {
        profile,
        fingerprint: format!("{:x}", hash.finalize()),
        core_sha256: managed.core_sha256,
        skins_sha256: managed.skins_sha256,
        spec,
        root: instance.root().to_owned(),
        directory_observations,
        deleting_marker,
        _pins: reader.pins,
        _directories: directories,
    };
    proof.recheck_before_spawn()?;
    Ok(proof)
}
#[cfg(test)]
#[path = "connect_ready_runtime_tests.rs"]
mod tests;
