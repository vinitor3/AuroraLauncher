//! Closed Windows x64 acquisition for the dedicated voice factory.
use super::*;
use crate::engine::{
    content_updates::metadata_is_link,
    download::{download_many_cancellable, DownloadRequest, ExpectedHash},
};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{self, Write},
    os::windows::fs::OpenOptionsExt,
};

pub(crate) const LAB_NAME: &str = "connect-native-voice-v1";
const JAVA_NAME: &str = "java-temurin-21-0-12-1-v1";
const JAVA_ROOT: &str = "jdk-21.0.12.1+1-jre";
const JAVA_URL: &str = "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.12.1%2B1/OpenJDK21U-jre_x64_windows_hotspot_21.0.12.1_1.zip";
const JAVA_SIZE: u64 = 48_999_141;
const JAVA_SHA256: &str = "d35f31e712f0fcf6ac5a093edc90204fbff22f720ba3950bd09d331d5e621636";
const JAVA_TREE_SHA256: &str = "0660a43c7d257aa665be19e4ea22467fda0d4b95a1c7a909d2a9d86f274c207b";
const JAVA_MAX_EXPANDED: u64 = 160 * 1024 * 1024;
const JAVA_MAX_ENTRIES: usize = 512;
const WORK_NAME: &str = ".resource-build-v1";
const WORK_ARCHIVE: &str = "temurin.zip";
const WORK_JAVA: &str = "java";
const WORK_STAGE: &str = "server";

type Cancel<'a> = dyn Fn() -> bool + Sync + 'a;

pub(in super::super) fn download_server_artifacts(
    target: &Path,
    pins: &[super::super::voice_catalog::VoiceArtifact],
    cancel: &Cancel<'_>,
) -> Result<()> {
    check_cancel(cancel)?;
    let mut directories = recovery::pin_directory(target)?;
    let mut requests = Vec::new();
    for pin in pins {
        recovery::valid_relative(pin.relative)?;
        let path = recovery::path(target, pin.relative)?;
        let mut current = target.to_path_buf();
        let relative = Path::new(pin.relative).parent().ok_or(Error::UnsafePath)?;
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(Error::UnsafePath);
            };
            current.push(name);
            if !current.try_exists()? {
                fs::create_dir(&current)?;
            }
            directories.extend(recovery::pin_directory(&current)?);
        }
        requests.push(DownloadRequest {
            url: pin.url.into(),
            destination: path,
            label: pin.relative.into(),
            expected_hash: Some(ExpectedHash::Sha256(pin.sha256.into())),
            expected_size: Some(pin.bytes),
        });
    }
    download_many_cancellable(&download_client()?, requests, 3, |_| {}, cancel).map_err(
        |error| {
            #[cfg(test)]
            {
                use crate::engine::download::DownloadError;
                let (kind, status) = match &error {
                    DownloadError::Network { source, .. } => {
                        ("network", source.status().map(|s| s.as_u16()))
                    }
                    DownloadError::Io { .. } => ("io", None),
                    DownloadError::Integrity(_) => ("integrity", None),
                    DownloadError::Resume(_) => ("resume", None),
                    DownloadError::ConflictingDestination(_) => ("conflict", None),
                    DownloadError::Cancelled => ("cancelled", None),
                };
                eprintln!("native_server_download_failure kind={kind} status={status:?}");
            }
            #[cfg(not(test))]
            let _ = error;
            if cancel() {
                Error::Cancelled
            } else {
                Error::Resources
            }
        },
    )?;
    for pin in pins {
        check_cancel(cancel)?;
        let (observation, _) = observe_file(&target.join(pin.relative), pin.bytes)?;
        if observation.bytes != pin.bytes || observation.sha256 != pin.sha256 {
            return Err(Error::Resources);
        }
    }
    Ok(())
}

pub(in super::super) fn prepare_server_java(
    lab: &Path,
    runtime: crate::engine::connect_ready_profile::ReadyProfile,
    cancel: &Cancel<'_>,
) -> Result<(PathBuf, ManagedJava)> {
    let major = runtime.descriptor().java_major;
    let (root, url, size, hash) = match major {
        8 => ("jdk8u512-b09-jre", "https://github.com/adoptium/temurin8-binaries/releases/download/jdk8u512-b09/OpenJDK8U-jre_x64_windows_hotspot_8u512b09.zip", 40_124_908, "ae1ae12f817d7735477c6555107eece73854da0654da69077309ef9bff0b78bf"),
        17 => ("jdk-17.0.21+9-jre", "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.21%2B9/OpenJDK17U-jre_x64_windows_hotspot_17.0.21_9.zip", 43_860_024, "fe4656bd78fb09812dc38c49da777f79cc33e7ee435a254be82f9e7f40d9d117"),
        21 => (JAVA_ROOT, JAVA_URL, JAVA_SIZE, JAVA_SHA256),
        _ => return Err(Error::Resources),
    };
    let expected_tree = super::super::server_preparation::java_tree(runtime);
    let (_lab, _lock) = open_cache(lab)?;
    let java = lab.join(format!("java-server-v3-{major}"));
    let verify = |path: &Path| -> Result<ManagedJava> {
        let (hash, pins) = observe_tree(path, cancel)?;
        if format!("{hash:x}") != expected_tree {
            return Err(Error::Resources);
        }
        Ok(pins)
    };
    if verified_directory(&java, || verify(&java).map(drop))? {
        return Ok((java.clone(), verify(&java)?));
    }
    let work = Staging::create(
        lab,
        lab.join(format!("java-server-build-v3-{}", Uuid::new_v4())),
    )?;
    let output = work.path.join("java");
    fs::create_dir(&output)?;
    let archive = work.path.join("java.zip");
    download_many_cancellable(
        &download_client()?,
        vec![DownloadRequest {
            url: url.into(),
            destination: archive.clone(),
            label: format!("Java {major}"),
            expected_hash: Some(ExpectedHash::Sha256(hash.into())),
            expected_size: Some(size),
        }],
        1,
        |_| {},
        cancel,
    )
    .map_err(|_| {
        if cancel() {
            Error::Cancelled
        } else {
            Error::Resources
        }
    })?;
    extract_java(recovery::secure_open(&archive)?, &output, root, cancel)?;
    drop(verify(&output)?);
    check_cancel(cancel)?;
    replace_directory(lab, &output, &java)?;
    Ok((java.clone(), verify(&java)?))
}

fn check_cancel(cancel: &Cancel<'_>) -> Result<()> {
    if cancel() {
        return Err(Error::Cancelled);
    }
    Ok(())
}

pub(crate) fn prepare_managed_plan(
    lab: &Path,
    profile: NativeVoiceProfile,
    cancel: &Cancel<'_>,
) -> Result<FactoryPlan> {
    check_cancel(cancel)?;
    let (_lab, _lock) = open_cache(lab)?;
    let work = lab.join(WORK_NAME);
    remove_owned_directory(lab, &work)?;
    let stage = lab.join(stage_name(profile));
    let java = lab.join(JAVA_NAME);
    let pins = voice_pins(profile);
    let needs_stage = !verified_directory(&stage, || verify_stage(&stage, &pins, cancel))?;
    let needs_java = !verified_directory(&java, || verify_java_tree(&java, cancel).map(drop))?;
    if needs_stage || needs_java {
        let staging = Staging::create(lab, work)?;
        let stage_work = staging.path.join(WORK_STAGE);
        let java_work = staging.path.join(WORK_JAVA);
        let archive = staging.path.join(WORK_ARCHIVE);
        let mut downloads = Vec::new();
        if needs_stage {
            fs::create_dir(&stage_work)?;
            fs::create_dir(stage_work.join("libraries"))?;
            fs::create_dir(stage_work.join("mods"))?;
            for pin in &pins {
                downloads.push(artifact_request(pin, &stage_work)?);
            }
        }
        if needs_java {
            fs::create_dir(&java_work)?;
            downloads.push(DownloadRequest {
                url: JAVA_URL.into(),
                destination: archive.clone(),
                label: "Java 21".into(),
                expected_hash: Some(ExpectedHash::Sha256(JAVA_SHA256.into())),
                expected_size: Some(JAVA_SIZE),
            });
        }
        {
            let _work = recovery::pin_directory(&staging.path)?;
            let _stage_directories = if needs_stage {
                let mut directories = recovery::pin_directory(&stage_work.join("libraries"))?;
                directories.extend(recovery::pin_directory(&stage_work.join("mods"))?);
                directories
            } else {
                Vec::new()
            };
            let client = download_client()?;
            download_many_cancellable(&client, downloads, 3, |_| {}, cancel).map_err(|_| {
                if cancel() {
                    Error::Cancelled
                } else {
                    Error::Resources
                }
            })?;
            if needs_stage {
                verify_stage(&stage_work, &pins, cancel)?;
            }
            if needs_java {
                let file = recovery::secure_open(&archive)?;
                extract_java(file, &java_work, JAVA_ROOT, cancel)?;
                verify_java_tree(&java_work, cancel)?;
            }
        }
        check_cancel(cancel)?;
        if needs_stage {
            replace_directory(lab, &stage_work, &stage)?;
        }
        if needs_java {
            replace_directory(lab, &java_work, &java)?;
        }
    }
    check_cancel(cancel)?;
    let plan = factory::plan_voice_bridge_factory_at(lab, &stage, &java, profile)?;
    check_cancel(cancel)?;
    Ok(plan)
}

fn stage_name(profile: NativeVoiceProfile) -> &'static str {
    match profile {
        NativeVoiceProfile::SimpleVoiceChat => "prepared-aurora-svc-v1",
        NativeVoiceProfile::PlasmoVoice => "prepared-aurora-plasmo-v1",
    }
}

fn open_cache(lab: &Path) -> Result<(Vec<File>, File)> {
    explicit_path(lab)?;
    if lab.file_name().and_then(|name| name.to_str()) != Some(LAB_NAME) {
        return Err(Error::UnsafePath);
    }
    let data = lab.parent().ok_or(Error::UnsafePath)?;
    let _parents = recovery::pin_directory(data.parent().ok_or(Error::UnsafePath)?)?;
    create_directory(data)?;
    let _data = recovery::pin_directory(data)?;
    create_directory(lab)?;
    let directories = recovery::pin_directory(lab)?;
    let lock = lock_cache(lab)?;
    Ok((directories, lock))
}

/// Acquire only missing client mod artifacts. The reviewed copy is untouched.
pub(crate) fn acquire_client_mods(
    data: &Path,
    profile: NativeVoiceProfile,
    missing: &[&str],
    cancel: &Cancel<'_>,
) -> Result<Vec<Vec<u8>>> {
    acquire_client_mods_for(
        data,
        crate::engine::connect_ready_profile::ReadyProfile::Fabric1211,
        profile,
        missing,
        cancel,
    )
}

pub(crate) fn acquire_client_mods_for(
    data: &Path,
    runtime: crate::engine::connect_ready_profile::ReadyProfile,
    profile: NativeVoiceProfile,
    missing: &[&str],
    cancel: &Cancel<'_>,
) -> Result<Vec<Vec<u8>>> {
    let voice = super::super::client_voice_pins_for(runtime, profile).ok_or(Error::Resources)?;
    if missing.is_empty() {
        return Ok(Vec::new());
    }
    check_cancel(cancel)?;
    let lab = data.join(LAB_NAME);
    let (_lab, _lock) = open_cache(&lab)?;
    let work = lab.join(WORK_NAME);
    remove_owned_directory(&lab, &work)?;
    let staging = Staging::create(&lab, work)?;
    fs::create_dir(staging.path.join("mods"))?;
    let _staging = recovery::pin_directory(&staging.path.join("mods"))?;
    let cached = lab.join(stage_name(profile));
    let mut ordered = Vec::new();
    let mut downloads = Vec::new();
    for name in missing {
        let (name, size, sha256) = voice
            .iter()
            .find(|(relative, _, _)| relative == name)
            .ok_or(Error::Resources)?;
        let pin = Pin {
            name,
            size: *size,
            expected: (*sha256).into(),
            sha1: false,
        };
        let bytes = read_client_mod(&cached, &pin).ok();
        if bytes.is_none() {
            downloads.push(artifact_request(&pin, &staging.path)?);
        }
        ordered.push((pin, bytes));
    }
    if !downloads.is_empty() {
        let client = download_client()?;
        download_many_cancellable(&client, downloads, 2, |_| {}, cancel).map_err(|_| {
            if cancel() {
                Error::Cancelled
            } else {
                Error::Resources
            }
        })?;
    }
    ordered
        .into_iter()
        .map(|(pin, bytes)| {
            check_cancel(cancel)?;
            match bytes {
                Some(bytes) => Ok(bytes),
                None => read_client_mod(&staging.path, &pin),
            }
        })
        .collect()
}

fn read_client_mod(directory: &Path, pin: &Pin) -> Result<Vec<u8>> {
    let bytes = recovery::read_bounded(&directory.join(pin.name), pin.size)?;
    if bytes.len() as u64 != pin.size || digest(&bytes) != pin.expected {
        return Err(Error::Resources);
    }
    Ok(bytes)
}

fn create_directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn lock_cache(lab: &Path) -> Result<File> {
    let path = lab.join(".resources.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(0x0020_0000)
        .share_mode(3)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata_is_link(&metadata) || !recovery::single_link(&file)? {
        return Err(Error::UnsafePath);
    }
    fs2::FileExt::try_lock_exclusive(&file).map_err(|_| Error::Busy)?;
    Ok(file)
}

fn verified_directory(path: &Path, verify: impl FnOnce() -> Result<()>) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
        Ok(metadata) if !metadata.is_dir() || metadata_is_link(&metadata) => Err(Error::UnsafePath),
        Ok(_) => {
            let _directory = recovery::pin_directory(path)?;
            match verify() {
                Ok(()) => Ok(true),
                Err(Error::Resources | Error::Changed | Error::Io | Error::Limit) => Ok(false),
                Err(error) => Err(error),
            }
        }
    }
}

struct Staging<'a> {
    lab: &'a Path,
    path: PathBuf,
}
impl<'a> Staging<'a> {
    fn create(lab: &'a Path, path: PathBuf) -> Result<Self> {
        fs::create_dir(&path)?;
        Ok(Self { lab, path })
    }
}
impl Drop for Staging<'_> {
    fn drop(&mut self) {
        let _ = remove_owned_directory(self.lab, &self.path);
    }
}

fn remove_owned_directory(lab: &Path, path: &Path) -> Result<()> {
    if path.parent() != Some(lab) {
        return Err(Error::UnsafePath);
    }
    let _lab = recovery::pin_directory(lab)?;
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(metadata) if !metadata.is_dir() || metadata_is_link(&metadata) => {
            return Err(Error::UnsafePath);
        }
        Ok(_) => {}
    }
    if path.canonicalize()?.parent() != Some(lab.canonicalize()?.as_path()) {
        return Err(Error::UnsafePath);
    }
    // remove_dir_all uses no-follow handles for directory entries on Windows.
    fs::remove_dir_all(path)?;
    Ok(())
}

fn replace_directory(lab: &Path, source: &Path, destination: &Path) -> Result<()> {
    remove_owned_directory(lab, destination)?;
    recovery::durable_rename(source, destination)?;
    Ok(())
}

fn artifact_request(pin: &Pin, stage: &Path) -> Result<DownloadRequest> {
    let url = match pin.name {
        "server.jar" => "https://piston-data.mojang.com/v1/objects/59353fb40c36d304f2035d51e7d6e6baa98dc05c/server.jar",
        "libraries/asm-9.8.jar" => "https://maven.fabricmc.net/org/ow2/asm/asm/9.8/asm-9.8.jar",
        "libraries/asm-analysis-9.8.jar" => "https://maven.fabricmc.net/org/ow2/asm/asm-analysis/9.8/asm-analysis-9.8.jar",
        "libraries/asm-commons-9.8.jar" => "https://maven.fabricmc.net/org/ow2/asm/asm-commons/9.8/asm-commons-9.8.jar",
        "libraries/asm-tree-9.8.jar" => "https://maven.fabricmc.net/org/ow2/asm/asm-tree/9.8/asm-tree-9.8.jar",
        "libraries/asm-util-9.8.jar" => "https://maven.fabricmc.net/org/ow2/asm/asm-util/9.8/asm-util-9.8.jar",
        "libraries/sponge-mixin-0.15.5+mixin.0.8.7.jar" => "https://maven.fabricmc.net/net/fabricmc/sponge-mixin/0.15.5+mixin.0.8.7/sponge-mixin-0.15.5+mixin.0.8.7.jar",
        "libraries/intermediary-1.21.1.jar" => "https://maven.fabricmc.net/net/fabricmc/intermediary/1.21.1/intermediary-1.21.1.jar",
        "libraries/fabric-loader-0.16.14.jar" => "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.14/fabric-loader-0.16.14.jar",
        "mods/voicechat-fabric-1.21.1-2.6.22.jar" => "https://cdn.modrinth.com/data/9eGKb6K1/versions/VS5X7jZd/voicechat-fabric-1.21.1-2.6.22.jar",
        "mods/plasmovoice-fabric-1.21.1-2.1.17.jar" => "https://cdn.modrinth.com/data/1bZhdhsH/versions/6Q2QmcUp/plasmovoice-fabric-1.21.1-2.1.17.jar",
        "mods/fabric-api-0.116.10+1.21.1.jar" => "https://cdn.modrinth.com/data/P7dR8mSH/versions/ID8pq1x1/fabric-api-0.116.10%2B1.21.1.jar",
        _ => catalog_url(pin).ok_or(Error::Resources)?,
    };
    let hash = if pin.sha1 {
        ExpectedHash::Sha1(pin.expected.clone())
    } else {
        ExpectedHash::Sha256(pin.expected.clone())
    };
    Ok(DownloadRequest {
        url: url.into(),
        destination: stage.join(pin.name),
        label: pin.name.into(),
        expected_hash: Some(hash),
        expected_size: Some(pin.size),
    })
}

fn catalog_url(pin: &Pin) -> Option<&'static str> {
    for runtime in crate::engine::connect_ready_profile::ReadyProfile::ALL {
        for profile in [
            NativeVoiceProfile::SimpleVoiceChat,
            NativeVoiceProfile::PlasmoVoice,
        ] {
            for artifact in super::super::voice_catalog::artifacts(runtime, profile).unwrap_or(&[])
            {
                if artifact.relative == pin.name
                    && artifact.bytes == pin.size
                    && artifact.sha256 == pin.expected
                    && !pin.sha1
                {
                    return Some(artifact.url);
                }
            }
        }
    }
    None
}

fn download_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent("AuroraLauncher/0.1.1")
        .https_only(true)
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let allowed = attempt.previous().len() == 1
                && [JAVA_URL,
                    "https://github.com/adoptium/temurin8-binaries/releases/download/jdk8u512-b09/OpenJDK8U-jre_x64_windows_hotspot_8u512b09.zip",
                    "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.21%2B9/OpenJDK17U-jre_x64_windows_hotspot_17.0.21_9.zip"].contains(&attempt.previous()[0].as_str())
                && attempt.url().scheme() == "https"
                && attempt.url().host_str() == Some("release-assets.githubusercontent.com")
                && attempt.url().port_or_known_default() == Some(443)
                && attempt.url().username().is_empty()
                && attempt.url().password().is_none();
            if allowed {
                attempt.follow()
            } else {
                attempt.error("unexpected resource redirect")
            }
        }))
        .build()
        .map_err(|_| Error::Resources)
}

fn verify_stage(stage: &Path, pins: &[Pin], cancel: &Cancel<'_>) -> Result<()> {
    for pin in pins {
        let mut file = recovery::secure_open(&stage.join(pin.name))?;
        if file.metadata()?.len() != pin.size {
            return Err(Error::Resources);
        }
        let actual = if pin.sha1 {
            format!(
                "{:x}",
                hash_stream::<sha1::Sha1>(&mut file, pin.size, cancel)?
            )
        } else {
            format!("{:x}", hash_stream::<Sha256>(&mut file, pin.size, cancel)?)
        };
        if actual != pin.expected {
            return Err(Error::Resources);
        }
    }
    Ok(())
}

fn hash_stream<D: Digest>(
    input: &mut impl Read,
    bytes: u64,
    cancel: &Cancel<'_>,
) -> Result<sha2::digest::Output<D>> {
    let mut hash = D::new();
    copy_bounded(input, bytes, cancel, |buffer| {
        hash.update(buffer);
        Ok(())
    })?;
    Ok(hash.finalize())
}

fn copy_bounded(
    input: &mut impl Read,
    bytes: u64,
    cancel: &Cancel<'_>,
    mut consume: impl FnMut(&[u8]) -> io::Result<()>,
) -> Result<()> {
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut total = 0;
    loop {
        check_cancel(cancel)?;
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > bytes {
            return Err(Error::Limit);
        }
        consume(&buffer[..read])?;
    }
    if total != bytes {
        return Err(Error::Resources);
    }
    Ok(())
}

fn valid_archive_path(path: &str) -> Result<()> {
    if !path.is_ascii() || path.len() > 200 || path.split('/').count() > 12 {
        return Err(Error::UnsafePath);
    }
    recovery::valid_relative(path)?;
    Ok(())
}

fn extract_java<R: Read + io::Seek>(
    archive: R,
    output: &Path,
    root_name: &str,
    cancel: &Cancel<'_>,
) -> Result<()> {
    let mut zip = zip::ZipArchive::new(archive).map_err(|_| Error::Resources)?;
    if zip.is_empty() || zip.len() > JAVA_MAX_ENTRIES {
        return Err(Error::Limit);
    }
    let _output = recovery::pin_directory(output)?;
    let mut names = BTreeSet::new();
    let mut expanded = 0_u64;
    let mut saw_root = false;
    for index in 0..zip.len() {
        check_cancel(cancel)?;
        let mut entry = zip.by_index(index).map_err(|_| Error::Resources)?;
        let dir = entry.is_dir();
        let mode = entry.unix_mode().map(|mode| mode & 0o170000);
        let expected_mode = if dir { 0o040000 } else { 0o100000 };
        if entry.enclosed_name().is_none()
            || mode.is_some_and(|mode| mode != 0 && mode != expected_mode)
        {
            return Err(Error::UnsafePath);
        }
        let (root, relative) = entry.name().split_once('/').ok_or(Error::UnsafePath)?;
        if root != root_name {
            return Err(Error::UnsafePath);
        }
        let relative = relative.strip_suffix('/').unwrap_or(relative);
        if relative.is_empty() && dir && entry.name() == format!("{root_name}/") {
            if saw_root || entry.size() != 0 {
                return Err(Error::UnsafePath);
            }
            saw_root = true;
            continue;
        }
        valid_archive_path(relative)?;
        if !names.insert(relative.to_ascii_lowercase()) {
            return Err(Error::UnsafePath);
        }
        let destination = recovery::path(output, relative)?;
        if dir {
            if entry.size() != 0 {
                return Err(Error::UnsafePath);
            }
            fs::create_dir_all(&destination)?;
        } else {
            let bytes = entry.size();
            expanded = expanded.checked_add(bytes).ok_or(Error::Limit)?;
            if expanded > JAVA_MAX_EXPANDED {
                return Err(Error::Limit);
            }
            let parent = destination.parent().ok_or(Error::UnsafePath)?;
            fs::create_dir_all(parent)?;
            let _parent = recovery::pin_directory(parent)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)?;
            copy_bounded(&mut entry, bytes, cancel, |buffer| file.write_all(buffer))?;
            file.sync_all()?;
        }
    }
    Ok(())
}

pub(crate) struct ManagedJava {
    _files: Vec<recovery::SecureFile>,
    _directories: Vec<File>,
}

pub(crate) fn pin_managed_java(lab: &Path, java: &Path) -> Result<Option<ManagedJava>> {
    if java == lab.join(JAVA_NAME) {
        return verify_java_tree(java, &|| false).map(Some);
    }
    if lab.file_name().and_then(|name| name.to_str()) == Some(LAB_NAME) {
        return Err(Error::UnsafePath);
    }
    Ok(None)
}

fn verify_java_tree(java: &Path, cancel: &Cancel<'_>) -> Result<ManagedJava> {
    let (hash, pins) = observe_tree(java, cancel)?;
    if format!("{hash:x}") != JAVA_TREE_SHA256 {
        return Err(Error::Resources);
    }
    Ok(pins)
}

fn observe_tree(
    java: &Path,
    cancel: &Cancel<'_>,
) -> Result<(sha2::digest::Output<Sha256>, ManagedJava)> {
    let mut entries = BTreeMap::new();
    let mut pins = ManagedJava {
        _files: Vec::new(),
        _directories: recovery::pin_directory(java)?,
    };
    collect_tree(java, "", &mut entries, &mut pins, cancel)?;
    let mut hash = Sha256::new();
    let mut expanded = 0_u64;
    for (name, is_dir) in entries {
        check_cancel(cancel)?;
        hash.update(if is_dir { b"D" } else { b"F" });
        hash.update(name.as_bytes());
        hash.update([0_u8]);
        if !is_dir {
            let mut file = recovery::secure_open(&java.join(name))?;
            let bytes = file.metadata()?.len();
            expanded = expanded.checked_add(bytes).ok_or(Error::Limit)?;
            if expanded > JAVA_MAX_EXPANDED {
                return Err(Error::Limit);
            }
            hash.update(bytes.to_le_bytes());
            hash.update(hash_stream::<Sha256>(&mut file, bytes, cancel)?);
            pins._files.push(file);
        }
    }
    Ok((hash.finalize(), pins))
}

fn collect_tree(
    root: &Path,
    prefix: &str,
    entries: &mut BTreeMap<String, bool>,
    pins: &mut ManagedJava,
    cancel: &Cancel<'_>,
) -> Result<()> {
    for entry in fs::read_dir(root)? {
        check_cancel(cancel)?;
        let entry = entry?;
        let filename = entry.file_name();
        let filename = filename.to_str().ok_or(Error::UnsafePath)?;
        let relative = if prefix.is_empty() {
            filename.to_owned()
        } else {
            format!("{prefix}/{filename}")
        };
        valid_archive_path(&relative)?;
        if entries.len() >= JAVA_MAX_ENTRIES {
            return Err(Error::Limit);
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata_is_link(&metadata) || !(metadata.is_dir() || metadata.is_file()) {
            return Err(Error::UnsafePath);
        }
        entries.insert(relative.clone(), metadata.is_dir());
        if metadata.is_dir() {
            pins._directories
                .extend(recovery::pin_directory(&entry.path())?);
            collect_tree(&entry.path(), &relative, entries, pins, cancel)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use zip::write::SimpleFileOptions;

    struct TestRoot(PathBuf);
    impl TestRoot {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("aurora-resource-test-{}", Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            remove_owned_directory(self.0.parent().unwrap(), &self.0).unwrap();
        }
    }

    fn archive(files: &[(&str, &[u8])]) -> Cursor<Vec<u8>> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in files {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap()
    }

    #[test]
    fn native_voice_resource_cache_extracts_only_closed_paths() {
        let test = TestRoot::new();
        for (index, files) in [
            vec![
                ("jdk/bin/java.exe", b"MZ".as_slice()),
                ("jdk/release", b"release".as_slice()),
            ],
            vec![("jdk/../escaped", b"escape".as_slice())],
            vec![
                ("jdk/bin/java.exe", b"MZ".as_slice()),
                ("jdk/BIN/JAVA.EXE", b"again".as_slice()),
            ],
            vec![("other/bin/java.exe", b"MZ".as_slice())],
            vec![("jdk/aux.txt", b"device".as_slice())],
        ]
        .into_iter()
        .enumerate()
        {
            let output = test.0.join(index.to_string());
            fs::create_dir(&output).unwrap();
            let result = extract_java(archive(&files), &output, "jdk", &|| false);
            if index == 0 {
                result.unwrap();
                assert_eq!(fs::read(output.join("bin/java.exe")).unwrap(), b"MZ");
            } else {
                assert_eq!(result, Err(Error::UnsafePath));
            }
        }
        assert!(!test.0.join("escaped").exists());
    }

    #[test]
    fn native_voice_resource_cache_rejects_corruption_and_cleans_unpublished_work() {
        let test = TestRoot::new();
        let final_path = test.0.join("verified");
        let work = test.0.join(WORK_NAME);
        let checksum = digest(b"valid");
        let pins = [Pin {
            name: "file",
            size: 5,
            expected: checksum,
            sha1: false,
        }];
        let lock = lock_cache(&test.0).unwrap();
        assert!(matches!(lock_cache(&test.0), Err(Error::Busy)));
        fs::create_dir(&final_path).unwrap();
        fs::write(final_path.join("file"), b"wrong").unwrap();
        let check = || verify_stage(&final_path, &pins, &|| false);
        assert!(!verified_directory(&final_path, check).unwrap());
        {
            let build = Staging::create(&test.0, work.clone()).unwrap();
            let staged = build.path.join(WORK_STAGE);
            fs::create_dir(&staged).unwrap();
            fs::write(staged.join("file"), b"valid").unwrap();
            verify_stage(&staged, &pins, &|| false).unwrap();
            replace_directory(&test.0, &staged, &final_path).unwrap();
        }
        assert!(!work.exists());
        assert!(verified_directory(&final_path, check).unwrap());
        assert!(lock.metadata().is_ok());
    }

    #[test]
    fn native_voice_resource_cache_cancelled_before_writes() {
        let test = TestRoot::new();
        let lab = test.0.join("data").join(LAB_NAME);
        assert!(matches!(
            prepare_managed_plan(&lab, NativeVoiceProfile::SimpleVoiceChat, &|| true),
            Err(Error::Cancelled)
        ));
        assert!(!lab.parent().unwrap().exists());
    }

    #[test]
    #[ignore = "downloads only exact public pins into an isolated temp cache; no Java or EULA execution"]
    fn native_voice_resource_cache_acquires_both_official_profiles() {
        let test = TestRoot::new();
        let lab = test.0.join("data").join(LAB_NAME);
        for profile in [
            NativeVoiceProfile::SimpleVoiceChat,
            NativeVoiceProfile::PlasmoVoice,
        ] {
            let plan = prepare_managed_plan(&lab, profile, &|| false).unwrap();
            assert_eq!(plan.review().purpose, "native_voice_bridge_factory_v2");
            assert!(!lab.join(WORK_NAME).exists());
        }
        let stage = lab.join("prepared-aurora-svc-v1");
        fs::write(stage.join("server.jar"), b"invalid").unwrap();
        let plan =
            prepare_managed_plan(&lab, NativeVoiceProfile::SimpleVoiceChat, &|| false).unwrap();
        assert_eq!(plan.review().purpose, "native_voice_bridge_factory_v2");
        assert!(!lab.join(WORK_NAME).exists());
        assert_eq!(
            fs::metadata(stage.join("server.jar")).unwrap().len(),
            51_627_615
        );
    }
}
