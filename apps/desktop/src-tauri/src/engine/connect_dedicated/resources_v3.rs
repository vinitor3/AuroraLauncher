//! Only the original completed installer capability can create executable V3
//! resources. Persisted observations support comparison/recovery, never acquisition.
use super::super::{manifest::VoiceManifestV3, server_preparation::PreparedServerResources};
use super::*;

const MAX_FILES: usize = 512;
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;

fn artifact_name_v3(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 240
        && !name.starts_with("jdk/")
        && name.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with('.')
                && !part.ends_with(' ')
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-+".contains(&byte))
                && ![
                    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6",
                    "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7",
                    "lpt8", "lpt9",
                ]
                .contains(
                    &part
                        .split('.')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .as_str(),
                )
        })
        && name.ends_with(".jar")
        && (name.starts_with("libraries/") || name.starts_with("mods/") || !name.contains('/'))
}
fn records_valid(records: &[ResourceRecord], java: bool) -> Result<()> {
    if records.is_empty() || records.len() > MAX_FILES + if java { 2 } else { 0 } {
        return Err(Error::Limit);
    }
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for record in records {
        let limit = match record.name.as_str() {
            "jdk/release" if java => 64 * 1024,
            "jdk/java.exe" if java => 4 * 1024 * 1024,
            name if artifact_name_v3(name) => MAX_FILE_BYTES,
            _ => return Err(Error::Identity),
        };
        let hash = &record.observation.sha256;
        if !names.insert(record.name.to_ascii_lowercase())
            || record.observation.bytes == 0
            || record.observation.bytes > limit
            || hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Error::Identity);
        }
        total = total
            .checked_add(record.observation.bytes)
            .ok_or(Error::Limit)?;
        if total > MAX_TOTAL_BYTES {
            return Err(Error::Limit);
        }
    }
    Ok(())
}

pub(in super::super) fn v3_record_shape(manifest: &VoiceManifestV3) -> Result<()> {
    manifest.validate_wire()?;
    let voice = &manifest.voice;
    records_valid(&voice.resources, true)?;
    records_valid(&voice.installed, false)?;
    for name in ["jdk/release", "jdk/java.exe"] {
        if !voice.resources.iter().any(|record| record.name == name) {
            return Err(Error::Identity);
        }
    }
    for installed in &voice.installed {
        let original = voice
            .resources
            .iter()
            .find(|record| record.name == installed.name)
            .ok_or(Error::Identity)?;
        if installed.observation.bytes != original.observation.bytes
            || installed.observation.sha256 != original.observation.sha256
        {
            return Err(Error::Identity);
        }
    }
    let profile = match voice.profile {
        StoredVoiceProfileV2::Svc => NativeVoiceProfile::SimpleVoiceChat,
        StoredVoiceProfileV2::Plasmo => NativeVoiceProfile::PlasmoVoice,
    };
    let mods = super::super::voice_catalog::artifacts(manifest.server.runtime()?, profile)
        .ok_or(Error::Resources)?;
    if voice
        .installed
        .iter()
        .filter(|record| record.name.starts_with("mods/"))
        .count()
        != mods.len()
    {
        return Err(Error::Identity);
    }
    for pin in mods {
        if !voice.installed.iter().any(|record| {
            record.name == pin.relative
                && record.observation.bytes == pin.bytes
                && record.observation.sha256 == pin.sha256
        }) {
            return Err(Error::Resources);
        }
    }
    Ok(())
}

/// Check only immutable artifact subtrees. Minecraft's world/config/log files
/// remain outside this inventory and are not promoted into executable inputs.
pub(in super::super) fn v3_inventory(root: &Path, manifest: &VoiceManifestV3) -> Result<()> {
    v3_record_shape(manifest)?;
    let names = manifest
        .voice
        .installed
        .iter()
        .map(|record| record.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut directories = BTreeSet::new();
    for name in &names {
        let mut prefix = String::new();
        let parts = name.split('/').collect::<Vec<_>>();
        for part in &parts[..parts.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            directories.insert(prefix.clone());
        }
    }
    for directory in directories
        .iter()
        .map(String::as_str)
        .chain(std::iter::once("mods"))
    {
        let path = root.join(directory);
        let _pins = recovery::pin_directory(&path)?;
        let prefix = format!("{directory}/");
        let expected = names
            .iter()
            .filter_map(|name| name.strip_prefix(&prefix))
            .filter(|name| !name.contains('/'))
            .map(str::to_owned)
            .chain(
                directories
                    .iter()
                    .filter_map(|name| name.strip_prefix(&prefix))
                    .filter(|name| !name.contains('/'))
                    .map(str::to_owned),
            )
            .collect::<BTreeSet<_>>();
        let mut actual = BTreeSet::new();
        for entry in fs::read_dir(path)? {
            let name = entry?
                .file_name()
                .into_string()
                .map_err(|_| Error::UnsafePath)?;
            if !expected.contains(&name) || !actual.insert(name) || actual.len() > expected.len() {
                return Err(Error::Identity);
            }
        }
        if actual != expected {
            return Err(Error::Identity);
        }
    }
    Ok(())
}
pub(in super::super) fn validate_v3_artifacts(
    root: &Path,
    manifest: &VoiceManifestV3,
) -> Result<()> {
    v3_inventory(root, manifest)?;
    for record in &manifest.voice.installed {
        let path = recovery::path(root, &record.name).map_err(|_| Error::UnsafePath)?;
        if observe_file(&path, record.observation.bytes)?.0 != record.observation {
            return Err(Error::Changed);
        }
    }
    Ok(())
}

impl Resources {
    pub(in super::super) fn inspect_v3(lab: &Path, proof: PreparedServerResources) -> Result<Self> {
        proof.verify()?;
        explicit_path(lab)?;
        let mut directories = recovery::pin_directory(lab)?;
        let stage = fs::canonicalize(proof.stage())?;
        if !stage.starts_with(fs::canonicalize(lab)?) || stage == fs::canonicalize(lab)? {
            return Err(Error::UnsafePath);
        }
        directories.extend(recovery::pin_directory(&stage)?);
        directories.extend(recovery::pin_directory(proof.java_root())?);
        records_valid(proof.artifacts(), false)?;
        let mut result = Self {
            files: Vec::new(),
            records: Vec::new(),
            _directories: directories,
            #[cfg(feature = "connect-voice-lab")]
            _managed_java: None,
            server_proof: None,
        };
        for record in proof.artifacts() {
            let path = recovery::path(&stage, &record.name).map_err(|_| Error::UnsafePath)?;
            result.read_pin(&path, &record.name, record.observation.bytes)?;
            if result
                .records
                .last()
                .is_none_or(|actual| actual.observation != record.observation)
            {
                return Err(Error::Changed);
            }
        }
        result.read_pin(&proof.java_root().join("release"), "jdk/release", 64 * 1024)?;
        let binary = result.read_pin(
            &proof.java_root().join("bin/java.exe"),
            "jdk/java.exe",
            4 * 1024 * 1024,
        )?;
        if !binary.starts_with(b"MZ") {
            return Err(Error::Resources);
        }
        proof.verify()?;
        result.server_proof = Some(Box::new(proof));
        Ok(result)
    }
    pub(super) fn copy_artifacts_v3(&mut self, destination: &Path) -> Result<()> {
        self.recheck_retained()?;
        let mut directories = recovery::pin_directory(destination)?;
        for pin in &mut self.files {
            if pin.name.starts_with("jdk/") {
                continue;
            }
            if !artifact_name_v3(&pin.name) {
                return Err(Error::Identity);
            }
            let parts = pin.name.split('/').collect::<Vec<_>>();
            let mut directory = destination.to_path_buf();
            for part in &parts[..parts.len() - 1] {
                directory.push(part);
                if !directory.try_exists()? {
                    fs::create_dir(&directory)?;
                }
                directories.extend(recovery::pin_directory(&directory)?);
            }
            pin.file.seek(SeekFrom::Start(0))?;
            let mut bytes = Vec::new();
            (&mut pin.file)
                .take(pin.observation.bytes + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 != pin.observation.bytes
                || digest(&bytes) != pin.observation.sha256
            {
                return Err(Error::Changed);
            }
            recovery::write_new(&destination.join(&pin.name), &bytes)?;
        }
        self.recheck_retained()
    }
}
