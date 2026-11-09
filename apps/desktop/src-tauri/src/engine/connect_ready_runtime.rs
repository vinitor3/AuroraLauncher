//! Read-only readiness proof for one existing diagnostic copy. Kept distinct
//! from installation and repair. Pins live only while the caller holds its lock.
use super::{
    connect::{ReadyGameLaunchError as Error, ReadyVoiceAction, ReadyVoiceMod, ReadyVoiceReview},
    connect_native_voice::NativeVoiceProfile,
    connect_quick_play, connect_ready_files,
    connect_ready_profile::{self, ReadyProfile},
    diagnostic_lab, recovery, Instance, InstanceMutationGuard, VersionLaunchSpec,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read},
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[path = "connect_ready_runtime_pins.rs"]
mod runtime_pins;
use runtime_pins::{verify_sha1, RuntimePins};

pub struct ReadyVoiceInstall {
    review: ReadyVoiceReview,
    #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
    downloads: Vec<Vec<u8>>,
}
impl ReadyVoiceInstall {
    pub fn review(&self) -> &ReadyVoiceReview {
        &self.review
    }
    pub fn commit(
        &self,
        proof: &mut ReadyRuntime,
        instance: &Instance,
        guard: &InstanceMutationGuard,
        authorized: impl Fn() -> bool,
    ) -> Result<(), Error> {
        #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
        {
            if proof.root != instance.root() || proof.voice_review.as_ref() != Some(&self.review) {
                return Err(Error::InstanceChanged);
            }
            proof.recheck_before_spawn()?;
            let additions: Vec<_> = self
                .review
                .mods
                .iter()
                .filter(|file| file.action == ReadyVoiceAction::Install)
                .zip(&self.downloads)
                .map(|(file, bytes)| (file.relative.as_str(), bytes.as_slice()))
                .collect();
            if additions.is_empty() {
                return Ok(());
            }
            super::content_updates::install_voice_mods(instance, guard, &additions, &authorized)
                .map_err(|error| match error {
                    super::ContentUpdateError::VoiceInstallCancelled => Error::Cancelled,
                    super::ContentUpdateError::RecoveryPending => Error::RecoveryPending,
                    _ => Error::InstanceChanged,
                })?;
            proof.pin_installed_voice_mods(&|| !authorized())?;
            Ok(())
        }
        #[cfg(not(all(windows, feature = "connect-lab", feature = "connect-voice-lab")))]
        {
            let _ = (proof, instance, guard, authorized);
            Err(Error::Disabled)
        }
    }
}

pub struct ReadyRuntime {
    pub profile: ReadyProfile,
    pub fingerprint: String,
    pub core_sha256: String,
    pub skins_sha256: String,
    pub spec: VersionLaunchSpec,
    pub voice_review: Option<ReadyVoiceReview>,
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
    pub fn prepare_voice_install(
        &self,
        app_data: &Path,
        cancelled: impl Fn() -> bool + Sync,
    ) -> Result<Option<ReadyVoiceInstall>, Error> {
        let Some(review) = self.voice_review.as_ref() else {
            return Ok(None);
        };
        #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
        {
            use super::connect_voice::dedicated as voice;
            let missing: Vec<_> = review
                .mods
                .iter()
                .filter(|file| file.action == ReadyVoiceAction::Install)
                .map(|file| file.relative.as_str())
                .collect();
            let downloads = voice::acquire_client_mods(
                app_data,
                dedicated_voice_profile(review.profile),
                &missing,
                &cancelled,
            )
            .map_err(|error| match error {
                voice::Error::Cancelled => Error::Cancelled,
                voice::Error::Busy => Error::Busy,
                _ => Error::VoiceArtifactsUnavailable,
            })?;
            if missing.len() != downloads.len() {
                return Err(Error::Internal);
            }
            Ok(Some(ReadyVoiceInstall {
                review: review.clone(),
                downloads,
            }))
        }
        #[cfg(not(all(windows, feature = "connect-lab", feature = "connect-voice-lab")))]
        {
            let _ = (review, app_data, cancelled);
            Err(Error::Disabled)
        }
    }

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

    #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
    fn pin_installed_voice_mods(&mut self, cancelled: &impl Fn() -> bool) -> Result<(), Error> {
        let review = self.voice_review.as_ref().ok_or(Error::Internal)?;
        let (_, expected_names) = self
            .directory_observations
            .iter_mut()
            .find(|(directory, _)| *directory == "mods")
            .ok_or(Error::Internal)?;
        let mut reader = Reader {
            cancel: cancelled,
            pins: Vec::new(),
            hashes: Vec::new(),
            bytes: 0,
        };
        for file in review
            .mods
            .iter()
            .filter(|file| file.action == ReadyVoiceAction::Install)
        {
            let path =
                recovery::path(&self.root, &file.relative).map_err(|_| Error::InstanceChanged)?;
            let bytes = reader.read(&path, file.relative.clone(), file.bytes)?;
            if bytes.len() as u64 != file.bytes
                || format!("{:x}", Sha256::digest(&bytes)) != file.sha256
            {
                return Err(Error::ArtifactMismatch);
            }
            expected_names.push(
                file.relative
                    .strip_prefix("mods/")
                    .ok_or(Error::Internal)?
                    .into(),
            );
        }
        expected_names.sort();
        self._pins.extend(reader.pins);
        self.recheck_before_spawn()
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
    inspect_ready_runtime_for_voice(
        instance,
        guard,
        source_id,
        java,
        skin_file,
        endpoint,
        observed_java,
        None,
        cancelled,
    )
}

/// Final check for a fresh, still-unpublished runtime copy. The installer
/// already verified the downloaded assets. The normal ReadyGame launch later
/// rechecks the copy, the full asset inventory and its authenticated bridge.
#[cfg(all(windows, feature = "connect-lab"))]
pub struct VerifiedInstalledReadyCopy {
    pub(super) root: PathBuf,
    pub(super) java: PathBuf,
    pub(super) java_sha256: String,
    _pins: Vec<recovery::SecureFile>,
}

#[cfg(all(windows, feature = "connect-lab"))]
pub fn verify_installed_copy_runtime(
    instance: &Instance,
    profile: ReadyProfile,
    java: &Path,
    cancelled: impl Fn() -> bool,
) -> Result<VerifiedInstalledReadyCopy, Error> {
    if !diagnostic_lab::is_lab_candidate(instance)
        || connect_ready_profile::select_ready_profile(instance)? != profile
    {
        return Err(Error::UnsupportedRuntime);
    }
    let runtime = profile.descriptor();
    let mut reader = Reader {
        cancel: &cancelled,
        pins: Vec::new(),
        hashes: Vec::new(),
        bytes: 0,
    };
    let mut read_profile = |version: &str| {
        let bytes = reader.instance(
            instance,
            &format!("versions/{version}/{version}.json"),
            1024 * 1024,
        )?;
        serde_json::from_slice(&bytes).map_err(|_| Error::UnsupportedRuntime)
    };
    let base = read_profile(runtime.minecraft_version)?;
    let child = read_profile(profile.version_id())?;
    connect_quick_play::validate_profile_chain(&base, &child, profile)
        .map_err(|_| Error::UnsupportedRuntime)?;
    let pins = RuntimePins::from_profiles(profile, &base, &child)?;
    let mut spec = super::resolve_launch_spec(instance, profile.version_id())
        .map_err(|_| Error::UnsupportedRuntime)?;
    connect_quick_play::apply_guest_entry(
        instance,
        &mut spec,
        profile,
        runtime.java_major,
        SocketAddr::from(([127, 0, 0, 1], 1)),
    )
    .map_err(|_| Error::ConflictingArguments)?;
    let mut observed = BTreeSet::new();
    for path in spec
        .libraries
        .iter()
        .chain(std::iter::once(&spec.client_jar))
    {
        let relative = path
            .strip_prefix(instance.root())
            .ok()
            .and_then(Path::to_str)
            .ok_or(Error::UnsupportedRuntime)?
            .replace('\\', "/");
        if !observed.insert(relative.clone()) {
            return Err(Error::UnsupportedRuntime);
        }
        let bytes = reader.instance(instance, &relative, 256 * 1024 * 1024)?;
        if let Some(pins) = &pins {
            let expected = if path == &spec.client_jar {
                &pins.client_sha1
            } else {
                pins.library_sha1
                    .get(&relative)
                    .ok_or(Error::UnsupportedRuntime)?
            };
            verify_sha1(&bytes, expected)?;
        }
    }
    if let Some(pins) = &pins {
        for (relative, expected) in &pins.additional_libraries {
            let bytes = reader.instance(instance, relative, 256 * 1024 * 1024)?;
            verify_sha1(&bytes, expected)?;
        }
        for (name, expected) in &pins.extracted_natives {
            let bytes = reader.instance(instance, &format!("natives/{name}"), 64 * 1024 * 1024)?;
            verify_sha1(&bytes, expected)?;
        }
    }
    let asset_index = reader.instance(
        instance,
        &format!("assets/indexes/{}.json", spec.asset_index),
        16 * 1024 * 1024,
    )?;
    if let Some(pins) = &pins {
        verify_sha1(&asset_index, &pins.asset_index_sha1)?;
    }
    let managed =
        connect_ready_files::verify_managed_files_for_profile(instance, profile, &cancelled)
            .map_err(|error| match error {
                connect_ready_files::ReadyFilesError::Cancelled => Error::Cancelled,
                _ => Error::ArtifactMismatch,
            })?;
    for file in managed.fingerprints {
        let bytes = reader.instance(instance, &file.relative, file.bytes)?;
        if bytes.len() as u64 != file.bytes
            || format!("{:x}", Sha256::digest(&bytes)) != file.sha256
        {
            return Err(Error::ArtifactMismatch);
        }
    }
    let java_sha256 = pin_and_measure_java(&mut reader, java, runtime.java_major)?;
    Ok(VerifiedInstalledReadyCopy {
        root: instance.root().to_owned(),
        java: java.to_owned(),
        java_sha256,
        _pins: reader.pins,
    })
}

/// The profile is supplied only by the original native Connect source.
#[allow(clippy::too_many_arguments)]
pub fn inspect_ready_runtime_for_voice(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    source_id: &str,
    java: &Path,
    skin_file: Option<&Path>,
    endpoint: SocketAddr,
    observed_java: u32,
    voice: Option<NativeVoiceProfile>,
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
    let voice_pins = match voice {
        None => &[][..],
        Some(profile_voice) => {
            if profile != ReadyProfile::Fabric1211 {
                return Err(Error::UnsupportedRuntime);
            }
            voice_artifact_pins(profile_voice)?
        }
    };
    let mut read_profile = |version: &str| {
        let bytes = reader.instance(
            instance,
            &format!("versions/{version}/{version}.json"),
            1024 * 1024,
        )?;
        serde_json::from_slice(&bytes).map_err(|_| Error::UnsupportedRuntime)
    };
    let base = read_profile(profile.descriptor().minecraft_version)?;
    let child = read_profile(profile.version_id())?;
    // The general resolver follows arbitrary inheritsFrom strings. Validate the
    // two retained files first, so it cannot read a third or external profile.
    connect_quick_play::validate_profile_chain(&base, &child, profile)
        .map_err(|_| Error::UnsupportedRuntime)?;
    let pins = RuntimePins::from_profiles(profile, &base, &child)?;
    let mut spec = super::resolve_launch_spec(instance, profile.version_id())
        .map_err(|_| Error::UnsupportedRuntime)?;
    connect_quick_play::apply_guest_entry(instance, &mut spec, profile, observed_java, endpoint)
        .map_err(|error| match error {
            connect_quick_play::QuickPlayError::ConflictingArguments => Error::ConflictingArguments,
            _ => Error::UnsupportedRuntime,
        })?;
    let managed = connect_ready_files::verify_managed_files_with_voice(
        instance, profile, voice, voice_pins, &cancelled,
    )
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
    let mut classpath_native_hashes = BTreeMap::new();
    let observed_natives = &directory_observations[1].1;
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
        let bytes = reader.instance(instance, &relative, 256 * 1024 * 1024)?;
        if let Some(pins) = &pins {
            let expected = if path == &spec.client_jar {
                &pins.client_sha1
            } else {
                pins.library_sha1
                    .get(&relative)
                    .ok_or(Error::UnsupportedRuntime)?
            };
            verify_sha1(&bytes, expected)?;
            if !observed_natives.is_empty()
                && pins.extracted_natives.is_empty()
                && relative.ends_with("-natives-windows.jar")
            {
                collect_classpath_native_hashes(&bytes, &mut classpath_native_hashes)?;
            }
        }
    }
    if let Some(pins) = &pins {
        for (relative, expected) in &pins.additional_libraries {
            if !paths.insert(relative.clone()) {
                return Err(Error::Internal);
            }
            let bytes = reader.instance(instance, relative, 256 * 1024 * 1024)?;
            verify_sha1(&bytes, expected)?;
        }
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
    if let Some(pins) = &pins {
        verify_sha1(&index, &pins.asset_index_sha1)?;
    }
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
    if let Some(pins) = &pins {
        if pins
            .extracted_natives
            .keys()
            .any(|name| !observed_natives.contains(name))
        {
            return Err(Error::MissingArtifact);
        }
    }
    for name in observed_natives {
        let bytes = reader.instance(instance, &format!("natives/{name}"), 64 * 1024 * 1024)?;
        if let Some(pins) = &pins {
            let expected = if pins.extracted_natives.is_empty() {
                classpath_native_hashes.get(name)
            } else {
                pins.extracted_natives.get(name)
            }
            .ok_or(Error::ArtifactMismatch)?;
            verify_sha1(&bytes, expected)?;
        }
    }
    let managed_java =
        diagnostic_lab::installed_runtime_java(instance).map_err(|_| Error::JavaUnavailable)?;
    if let Some((expected_path, expected_sha256)) = managed_java {
        if expected_path != java
            || pin_and_measure_java(&mut reader, java, observed_java)? != expected_sha256
        {
            return Err(Error::JavaUnavailable);
        }
    } else {
        pin_java_runtime(&mut reader, java, observed_java)?;
    }
    if let Some(path) = skin_file {
        reader.read(path, "selected-local-skin".into(), 5 * 1024 * 1024)?;
    }
    if cancelled() {
        return Err(Error::Cancelled);
    }
    reader.hashes.sort();
    let voice_review = voice.map(|profile| ReadyVoiceReview {
        profile,
        mods: voice_pins
            .iter()
            .map(|(relative, bytes, sha256)| ReadyVoiceMod {
                relative: (*relative).into(),
                bytes: *bytes,
                sha256: (*sha256).into(),
                action: if managed
                    .fingerprints
                    .iter()
                    .any(|file| file.relative == *relative)
                {
                    ReadyVoiceAction::Present
                } else {
                    ReadyVoiceAction::Install
                },
            })
            .collect(),
    });
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
        voice_review,
        root: instance.root().to_owned(),
        directory_observations,
        deleting_marker,
        _pins: reader.pins,
        _directories: directories,
    };
    proof.recheck_before_spawn()?;
    Ok(proof)
}

type VoiceArtifactPin = (&'static str, u64, &'static str);

fn voice_artifact_pins(profile: NativeVoiceProfile) -> Result<&'static [VoiceArtifactPin], Error> {
    #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
    {
        use super::connect_voice::dedicated as voice;
        Ok(voice::client_voice_pins(dedicated_voice_profile(profile)))
    }
    #[cfg(not(all(windows, feature = "connect-lab", feature = "connect-voice-lab")))]
    {
        let _ = profile;
        Err(Error::Disabled)
    }
}

#[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
fn dedicated_voice_profile(
    profile: NativeVoiceProfile,
) -> super::connect_voice::dedicated::NativeVoiceProfile {
    use super::connect_voice::dedicated as voice;
    match profile {
        NativeVoiceProfile::SimpleVoiceChat => voice::NativeVoiceProfile::SimpleVoiceChat,
        NativeVoiceProfile::PlasmoVoice => voice::NativeVoiceProfile::PlasmoVoice,
    }
}

fn collect_classpath_native_hashes(
    bytes: &[u8],
    hashes: &mut BTreeMap<String, String>,
) -> Result<(), Error> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| Error::ArtifactMismatch)?;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| Error::ArtifactMismatch)?;
        let name = entry.name().rsplit('/').next().unwrap_or_default();
        if !name.ends_with(".dll") {
            continue;
        }
        let name = name.to_owned();
        let mut contents = Vec::new();
        entry
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut contents)
            .map_err(|_| Error::ArtifactMismatch)?;
        if contents.len() > 64 * 1024 * 1024 {
            return Err(Error::ArtifactMismatch);
        }
        hashes.insert(name, format!("{:x}", sha1::Sha1::digest(contents)));
    }
    Ok(())
}

fn pin_and_measure_java(reader: &mut Reader<'_>, java: &Path, major: u32) -> Result<String, Error> {
    let start = reader.hashes.len();
    pin_java_runtime(reader, java, major)?;
    let material = serde_json::to_vec(&reader.hashes[start..]).map_err(|_| Error::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"aurora-ready-managed-java-v1\0");
    digest.update(material);
    Ok(format!("{:x}", digest.finalize()))
}

fn pin_java_runtime(reader: &mut Reader<'_>, java: &Path, java_major: u32) -> Result<(), Error> {
    fn java_error(error: Error) -> Error {
        match error {
            Error::Cancelled => error,
            _ => Error::JavaUnavailable,
        }
    }
    if !java.is_absolute() {
        return Err(Error::JavaUnavailable);
    }
    reader
        .read(java, "selected-java-executable".into(), 64 * 1024 * 1024)
        .map_err(java_error)?;
    let java_home = java
        .parent()
        .and_then(Path::parent)
        .ok_or(Error::JavaUnavailable)?;
    let mut pin = |relative: &str| -> Result<(), Error> {
        let path = recovery::path(java_home, relative).map_err(|_| Error::JavaUnavailable)?;
        reader
            .read(&path, format!("java-runtime/{relative}"), 512 * 1024 * 1024)
            .map_err(java_error)?;
        Ok(())
    };
    if java_major != 8 {
        for relative in [
            "release",
            "lib/modules",
            "bin/server/jvm.dll",
            "bin/java.dll",
            "bin/jli.dll",
        ] {
            pin(relative)?;
        }
        return Ok(());
    }

    // A selected Java 8 launcher can live in a standalone JRE or at the
    // top of a JDK, whose HotSpot DLLs and bootstrap JARs live under jre/.
    let nested = recovery::path(java_home, "jre/lib/rt.jar").map_err(|_| Error::JavaUnavailable)?;
    let prefix = if nested.try_exists().map_err(|_| Error::JavaUnavailable)? {
        pin("bin/jli.dll")?;
        "jre/"
    } else {
        ""
    };
    let release = recovery::path(java_home, "release").map_err(|_| Error::JavaUnavailable)?;
    if release.try_exists().map_err(|_| Error::JavaUnavailable)? {
        pin("release")?;
    } else if !prefix.is_empty() {
        return Err(Error::JavaUnavailable);
    }
    for file in [
        "bin/java.dll",
        "bin/jli.dll",
        "bin/server/jvm.dll",
        "lib/amd64/jvm.cfg",
        "lib/rt.jar",
        "lib/resources.jar",
        "lib/charsets.jar",
        "lib/jce.jar",
        "lib/jsse.jar",
    ] {
        pin(&format!("{prefix}{file}"))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "connect_ready_runtime_tests.rs"]
mod tests;
