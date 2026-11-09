//! One reviewed server preparation produces one move-only factory capability.
//! Persisted provenance supports comparison/recovery; it cannot launch a process.
use super::*;
use crate::engine::connect_ready_profile::ReadyProfile;
use resources::ResourceRecord;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ServerInstallerProvenance {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ServerPreparationProvenance {
    pub schema: u32,
    pub operation_id: Uuid,
    pub runtime_profile: String,
    pub preparation_review_digest: String,
    #[serde(deserialize_with = "installer_object")]
    pub installer: Option<ServerInstallerProvenance>,
    pub input_digest: String,
    pub output_digest: String,
    pub recipe_digest: String,
    pub java_tree_sha256: String,
}
fn installer_object<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<ServerInstallerProvenance>, D::Error> {
    struct OptionalObject;
    impl<'de> serde::de::Visitor<'de> for OptionalObject {
        type Value = Option<ServerInstallerProvenance>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("an installer object or null")
        }
        fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: serde::Deserializer<'de>>(
            self,
            d: D,
        ) -> std::result::Result<Self::Value, D::Error> {
            manifest::object_only(d).map(Some)
        }
    }
    deserializer.deserialize_option(OptionalObject)
}
fn hash_value(value: &impl Serialize) -> Result<String> {
    Ok(digest(
        &serde_json::to_vec(value).map_err(|_| Error::Identity)?,
    ))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn installer(runtime: ReadyProfile) -> Option<ServerInstallerProvenance> {
    server_catalog::forge_installer(runtime).map(|pin| ServerInstallerProvenance {
        name: pin.relative.into(),
        bytes: pin.bytes,
        sha256: pin.sha256.into(),
    })
}
pub(super) fn java_tree(runtime: ReadyProfile) -> &'static str {
    match runtime.descriptor().java_major {
        8 => "1c592e7daac9213d5ba446f0b63c3bdf3b67309b4b00fbb74994356c61960072",
        17 => "2ce289cadbd989885b42450ff38ab22c46fe428f9c90914cd1cf77d263545d82",
        21 => "0660a43c7d257aa665be19e4ea22467fda0d4b95a1c7a909d2a9d86f274c207b",
        _ => unreachable!("closed native runtime Java"),
    }
}
fn input_digest(runtime: ReadyProfile, profile: NativeVoiceProfile) -> Result<String> {
    let voice = voice_catalog::artifacts(runtime, profile).ok_or(Error::Resources)?;
    hash_value(&(
        "aurora-native-server-inputs/v3",
        runtime.version_id(),
        server_catalog::fabric_artifacts(runtime),
        installer(runtime),
        voice,
        java_tree(runtime),
    ))
}
impl ServerPreparationProvenance {
    pub(super) fn validate(
        &self,
        runtime: ReadyProfile,
        profile: NativeVoiceProfile,
        records: &[ResourceRecord],
        recipe: &[String],
    ) -> Result<()> {
        if self.schema != 1
            || self.operation_id.is_nil()
            || self.runtime_profile != runtime.version_id()
            || !valid_hash(&self.preparation_review_digest)
            || self.installer != installer(runtime)
            || self.input_digest != input_digest(runtime, profile)?
            || self.output_digest != hash_value(&records)?
            || recipe != server_catalog::launch_args(runtime)
            || self.recipe_digest != hash_value(&recipe)?
            || self.java_tree_sha256 != java_tree(runtime)
        {
            return Err(Error::Identity);
        }
        if records.is_empty() || records.len() > 512 {
            return Err(Error::Limit);
        }
        let mut names = BTreeSet::new();
        let mut total = 0u64;
        for record in records {
            recovery::valid_relative(&record.name)?;
            if !artifact_name(&record.name)
                || !names.insert(record.name.to_ascii_lowercase())
                || !valid_hash(&record.observation.sha256)
                || record.observation.bytes == 0
                || record.observation.bytes > 256 * 1024 * 1024
            {
                return Err(Error::Identity);
            }
            total = total
                .checked_add(record.observation.bytes)
                .ok_or(Error::Limit)?;
        }
        if total > 1024 * 1024 * 1024 {
            return Err(Error::Limit);
        }
        if let Some(pins) = server_catalog::fabric_artifacts(runtime) {
            for pin in pins {
                require_pin(records, pin)?;
            }
            if records.len()
                != pins.len()
                    + voice_catalog::artifacts(runtime, profile)
                        .ok_or(Error::Resources)?
                        .len()
            {
                return Err(Error::Identity);
            }
        }
        let voice = voice_catalog::artifacts(runtime, profile).ok_or(Error::Resources)?;
        if records
            .iter()
            .filter(|r| r.name.starts_with("mods/"))
            .count()
            != voice.len()
        {
            return Err(Error::Identity);
        }
        for pin in voice {
            require_pin(records, pin)?;
        }
        for name in server_catalog::required_recipe_artifacts(runtime) {
            if !records.iter().any(|record| record.name == name) {
                return Err(Error::Resources);
            }
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn fixture(
        runtime: ReadyProfile,
        profile: NativeVoiceProfile,
        records: &[ResourceRecord],
        recipe: &[String],
    ) -> Result<Self> {
        Ok(Self {
            schema: 1,
            operation_id: Uuid::new_v4(),
            runtime_profile: runtime.version_id().into(),
            preparation_review_digest: "11".repeat(32),
            installer: installer(runtime),
            input_digest: input_digest(runtime, profile)?,
            output_digest: hash_value(&records)?,
            recipe_digest: hash_value(&recipe)?,
            java_tree_sha256: java_tree(runtime).into(),
        })
    }
}
fn require_pin(records: &[ResourceRecord], pin: &voice_catalog::VoiceArtifact) -> Result<()> {
    if records.iter().any(|record| {
        record.name == pin.relative
            && record.observation.bytes == pin.bytes
            && record.observation.sha256 == pin.sha256
    }) {
        Ok(())
    } else {
        Err(Error::Resources)
    }
}
fn artifact_name(name: &str) -> bool {
    name.len() <= 240
        && name.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-+".contains(&byte))
        })
        && name.ends_with(".jar")
        && (name.starts_with("libraries/") || name.starts_with("mods/") || !name.contains('/'))
}
pub(super) struct PreparationDirectory {
    lab: PathBuf,
    root: PathBuf,
    identity: FileIdentity,
}
impl Drop for PreparationDirectory {
    fn drop(&mut self) {
        // Only this freshly created directory can be removed. Its last owner
        // drops after process completion and after all artifact/path pins.
        if self.root.parent() != Some(self.lab.as_path())
            || directory_identity(&self.root).ok().as_ref() != Some(&self.identity)
        {
            return;
        }
        let Ok(root) = self.root.canonicalize() else {
            return;
        };
        let Ok(lab) = self.lab.canonicalize() else {
            return;
        };
        if root.parent() != Some(lab.as_path()) {
            return;
        }
        // std's Windows traversal does not follow directory reparse points.
        let _ = fs::remove_dir_all(&self.root);
    }
}
struct HeldArtifact {
    path: PathBuf,
    file: recovery::SecureFile,
    expected: Observation,
}
pub(super) struct PreparedServerResources {
    runtime: ReadyProfile,
    profile: NativeVoiceProfile,
    stage: PathBuf,
    java_root: PathBuf,
    recipe: Vec<String>,
    records: Vec<ResourceRecord>,
    provenance: ServerPreparationProvenance,
    held: Vec<HeldArtifact>,
    _directories: Vec<File>,
    #[cfg(feature = "connect-voice-lab")]
    _java: resources::cache::ManagedJava,
    _work: std::sync::Arc<PreparationDirectory>,
}
impl PreparedServerResources {
    pub(super) fn runtime(&self) -> ReadyProfile {
        self.runtime
    }
    pub(super) fn voice_profile(&self) -> NativeVoiceProfile {
        self.profile
    }
    pub(super) fn stage(&self) -> &Path {
        &self.stage
    }
    pub(super) fn java_root(&self) -> &Path {
        &self.java_root
    }
    pub(super) fn recipe_args(&self) -> &[String] {
        &self.recipe
    }
    pub(super) fn artifacts(&self) -> &[ResourceRecord] {
        &self.records
    }
    pub(super) fn provenance(&self) -> &ServerPreparationProvenance {
        &self.provenance
    }
    pub(super) fn work_lifetime(&self) -> std::sync::Arc<PreparationDirectory> {
        self._work.clone()
    }
    pub(super) fn verify(&self) -> Result<()> {
        self.provenance
            .validate(self.runtime, self.profile, &self.records, &self.recipe)?;
        // Every original handle denies writes/deletes for the entire lifetime.
        // Reopen paths to reject replacement/ancestor changes without rehashing
        // immutable bytes already checked under the same retained handles.
        for artifact in &self.held {
            let current = recovery::secure_open(&artifact.path)?;
            if file_identity(&current)? != artifact.expected.identity
                || current.metadata()?.len() != artifact.expected.bytes
                || file_identity(&artifact.file)? != artifact.expected.identity
                || artifact.file.metadata()?.len() != artifact.expected.bytes
            {
                return Err(Error::Changed);
            }
        }
        Ok(())
    }
}

#[cfg(feature = "connect-voice-lab")]
pub(super) struct ServerPreparationPlan {
    runtime: ReadyProfile,
    profile: NativeVoiceProfile,
    work: PathBuf,
    stage: PathBuf,
    java_root: PathBuf,
    java: resources::cache::ManagedJava,
    review: Review,
    deadline: Instant,
    operation_id: Uuid,
    _pins: Vec<File>,
    work_owner: PreparationDirectory,
}
#[cfg(feature = "connect-voice-lab")]
impl ServerPreparationPlan {
    pub(super) fn prepare(
        lab: &Path,
        runtime: ReadyProfile,
        profile: NativeVoiceProfile,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> Result<Self> {
        let voice = voice_catalog::artifacts(runtime, profile).ok_or(Error::Resources)?;
        let _ = voice;
        if cancelled() {
            return Err(Error::Cancelled);
        }
        // The factory uses these fixed-length native names. Check the full
        // eventual working-directory budget before download, copy or EULA;
        // the final actual path is checked again by the production recipe.
        super::producer::validate_v3_launch_paths(
            &lab.join(format!("native-status-{}", Uuid::nil()))
                .join("native/instances")
                .join(format!("aurora-test-{}", Uuid::nil())),
            runtime,
        )?;
        let (java_root, java) = resources::cache::prepare_server_java(lab, runtime, cancelled)?;
        let operation_id = Uuid::new_v4();
        let work = lab.join(format!("server-prepare-v3-{operation_id}"));
        absent(&work)?;
        fs::create_dir(&work)?;
        let work_owner = PreparationDirectory {
            lab: lab.into(),
            root: work.clone(),
            identity: directory_identity(&work)?,
        };
        let mut pins = recovery::pin_directory(lab)?;
        pins.extend(recovery::pin_directory(&work)?);
        let stage = work.join("server");
        fs::create_dir(&stage)?;
        pins.extend(recovery::pin_directory(&stage)?);
        if let Some(pin) = server_catalog::forge_installer(runtime) {
            resources::cache::download_server_artifacts(&work, &[pin], cancelled)?;
        }
        let kind = if installer(runtime).is_some() {
            "Executar o instalador oficial Forge --installServer em pasta própria; sem EULA ou servidor iniciado"
        } else {
            "Preparar arquivos oficiais Fabric em pasta própria; sem executar Java"
        };
        let descriptor = runtime.descriptor();
        let id = BundleId(operation_id);
        let mut review = Review { review_id: Uuid::new_v4(), digest: String::new(), purpose: "native_voice_server_prepare_v3", bundle_id: id, before: "Nenhum servidor preparado ou processo iniciado".into(), after: kind.into(), details: vec![format!("Minecraft {}; {} {}; Java {}; protocolo {}", descriptor.minecraft_version, descriptor.loader, descriptor.loader_version, descriptor.java_major, server_catalog::protocol(runtime)), format!("Destino próprio {}", stage.display()), format!("Inputs SHA256 {}", input_digest(runtime, profile)?), "Java/instalador limitados a15min; cancelar recolhe o mesmo processo e descendentes antes de liberar a cópia. EULA e execução do servidor exigem outra revisão.".into()] };
        if let Some(pin) = server_catalog::forge_installer(runtime) {
            review.details.push(format!(
                "Installer {}; {} bytes; SHA256 {}",
                pin.relative, pin.bytes, pin.sha256
            ));
        }
        review.digest = hash_value(&(
            "aurora-native-server-prepare-review/v3",
            (
                review.review_id,
                review.purpose,
                review.bundle_id,
                &review.before,
                &review.after,
                &review.details,
            ),
            operation_id,
            runtime.version_id(),
            input_digest(runtime, profile)?,
            &java_root,
            &stage,
            server_catalog::launch_args(runtime),
        ))?;
        Ok(Self {
            runtime,
            profile,
            work,
            stage,
            java_root,
            java,
            review,
            deadline: Instant::now() + REVIEW_TTL,
            operation_id,
            _pins: pins,
            work_owner,
        })
    }
    pub(super) fn review(&self) -> &Review {
        &self.review
    }
    pub(super) fn approval_deadline(&self) -> Instant {
        self.deadline
    }
    pub(super) fn execute(
        self,
        approval: &str,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> Result<PreparedServerResources> {
        approve(&self.review, self.deadline, approval)?;
        if cancelled() {
            return Err(Error::Cancelled);
        }
        if let Some(pin) = server_catalog::forge_installer(self.runtime) {
            let path = self.work.join(pin.relative);
            let held = recovery::secure_open(&path)?;
            let (_, bytes) = observe_file(&path, pin.bytes)?;
            if bytes.len() as u64 != pin.bytes || digest(&bytes) != pin.sha256 {
                return Err(Error::Resources);
            }
            let args = vec![
                std::ffi::OsString::from("-jar"),
                path.into_os_string(),
                std::ffi::OsString::from("--installServer"),
                self.stage.as_os_str().to_owned(),
            ];
            approve(&self.review, self.deadline, approval)?;
            crate::engine::connect_owned_process::run_owned_java_installer(
                &self.java_root.join("bin/java.exe"),
                &args,
                &self.work,
                &self.work.join("installer-home"),
                Duration::from_secs(900),
                cancelled,
            )
            .map_err(|error| {
                if error == crate::engine::connect_owned_process::OwnedInstallerError::Cancelled {
                    Error::Cancelled
                } else {
                    Error::Resources
                }
            })?;
            drop(held);
        } else {
            resources::cache::download_server_artifacts(
                &self.stage,
                server_catalog::fabric_artifacts(self.runtime).ok_or(Error::Resources)?,
                cancelled,
            )
            .map_err(|error| preparation_error("server_inputs", error))?;
        }
        if cancelled() {
            return Err(Error::Cancelled);
        }
        resources::cache::download_server_artifacts(
            &self.stage,
            voice_catalog::artifacts(self.runtime, self.profile).ok_or(Error::Resources)?,
            cancelled,
        )
        .map_err(|error| preparation_error("voice_inputs", error))?;
        let mut capture = Capture {
            records: Vec::new(),
            held: Vec::new(),
            directories: self._pins,
            visited: 0,
            bytes: 0,
        };
        capture_artifacts(&self.stage, "", &mut capture, cancelled)
            .map_err(|error| preparation_error("capture", error))?;
        let Capture {
            mut records,
            held,
            directories,
            ..
        } = capture;
        records.sort_by(|a, b| a.name.cmp(&b.name));
        validate_jar_classpaths(
            &self.stage,
            &records,
            voice_catalog::artifacts(self.runtime, self.profile).ok_or(Error::Resources)?,
            &server_catalog::required_recipe_artifacts(self.runtime),
            cancelled,
        )
        .map_err(|error| preparation_error("jar_classpaths", error))?;
        let recipe = server_catalog::launch_args(self.runtime);
        let provenance = ServerPreparationProvenance {
            schema: 1,
            operation_id: self.operation_id,
            runtime_profile: self.runtime.version_id().into(),
            preparation_review_digest: self.review.digest,
            installer: installer(self.runtime),
            input_digest: input_digest(self.runtime, self.profile)?,
            output_digest: hash_value(&records)?,
            recipe_digest: hash_value(&recipe)?,
            java_tree_sha256: java_tree(self.runtime).into(),
        };
        let prepared = PreparedServerResources {
            runtime: self.runtime,
            profile: self.profile,
            stage: self.stage,
            java_root: self.java_root,
            recipe,
            records,
            provenance,
            held,
            _directories: directories,
            _java: self.java,
            _work: std::sync::Arc::new(self.work_owner),
        };
        prepared
            .verify()
            .map_err(|error| preparation_error("proof", error))?;
        if cancelled() {
            return Err(Error::Cancelled);
        }
        Ok(prepared)
    }
}

#[cfg(all(test, feature = "connect-voice-lab"))]
#[path = "server_preparation_tests.rs"]
mod tests;

#[cfg(feature = "connect-voice-lab")]
fn preparation_error(stage: &'static str, error: Error) -> Error {
    #[cfg(test)]
    eprintln!("native_server_preparation_failure stage={stage} kind={error:?}");
    #[cfg(not(test))]
    let _ = stage;
    error
}

#[cfg(feature = "connect-voice-lab")]
fn jar_manifest_name(
    source: &File,
    central_start: u64,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Option<(String, u64)>> {
    use std::io::{BufReader, Seek, SeekFrom};

    const MANIFEST: &[u8] = b"META-INF/MANIFEST.MF";
    // Java's manifest lookup folds ASCII case. Walk the central names because
    // ZipArchive's public name index has already coalesced exact duplicates.
    let mut input = BufReader::new(source);
    input.seek(SeekFrom::Start(central_start))?;
    let mut found = None;
    loop {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let mut signature = [0; 4];
        input.read_exact(&mut signature)?;
        match &signature {
            b"PK\x01\x02" => (),
            b"PK\x05\x06" | b"PK\x06\x06" | b"PK\x05\x05" => return Ok(found),
            _ => return Err(Error::Resources),
        }
        let mut header = [0; 42];
        input.read_exact(&mut header)?;
        let mut name_bytes = usize::from(u16::from_le_bytes([header[24], header[25]]));
        let extra = usize::from(u16::from_le_bytes([header[26], header[27]]));
        let comment = usize::from(u16::from_le_bytes([header[28], header[29]]));
        if name_bytes == MANIFEST.len() {
            let mut name = [0; MANIFEST.len()];
            input.read_exact(&mut name)?;
            name_bytes = 0;
            if name.eq_ignore_ascii_case(MANIFEST) {
                if found.is_some() {
                    return Err(Error::Resources);
                }
                let start = input
                    .stream_position()?
                    .checked_sub((46 + name.len()) as u64)
                    .ok_or(Error::Resources)?;
                let name = String::from_utf8(name.to_vec()).map_err(|_| Error::Resources)?;
                found = Some((name, start));
            }
        }
        input.seek_relative((name_bytes + extra + comment) as i64)?;
    }
}

#[cfg(feature = "connect-voice-lab")]
const MAX_MAIN_MANIFEST_BYTES: usize = 1024 * 1024;

#[cfg(feature = "connect-voice-lab")]
fn read_main_manifest(source: impl Read) -> Result<Vec<u8>> {
    let mut main = Vec::new();
    let mut line_has_content = false;
    let mut previous_cr = false;
    for (offset, value) in std::io::BufReader::new(source).bytes().enumerate() {
        if offset >= MAX_MAIN_MANIFEST_BYTES {
            return Err(Error::Limit);
        }
        let byte = value?;
        if byte == b'\n' && previous_cr {
            previous_cr = false;
            continue;
        }
        previous_cr = byte == b'\r';
        if matches!(byte, b'\r' | b'\n') {
            if !line_has_content {
                break;
            }
            main.push(b'\n');
            line_has_content = false;
        } else {
            main.push(byte);
            line_has_content = true;
        }
    }
    Ok(main)
}

#[cfg(feature = "connect-voice-lab")]
fn validate_jar_classpaths(
    root: &Path,
    records: &[ResourceRecord],
    mods: &[voice_catalog::VoiceArtifact],
    required: &[String],
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<()> {
    let names: BTreeSet<_> = records.iter().map(|record| record.name.as_str()).collect();
    for record in records {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let pinned_mod = mods.iter().any(|pin| {
            pin.relative == record.name
                && pin.bytes == record.observation.bytes
                && pin.sha256 == record.observation.sha256
        });
        let source = recovery::secure_open(&root.join(&record.name))?;
        let mut archive = zip::ZipArchive::new(&*source).map_err(|_| Error::Resources)?;
        let Some((manifest_name, header_start)) =
            jar_manifest_name(&source, archive.central_directory_start(), cancelled)?
        else {
            continue;
        };
        let manifest = archive
            .by_name(&manifest_name)
            .map_err(|_| Error::Resources)?;
        if manifest.central_header_start() != header_start {
            // A ZIP Unicode path field can alias a decoded name. Inspect
            // only the raw central entry selected under Java's name rules.
            return Err(Error::Resources);
        }
        // Signed Forge JARs carry large per-entry digest sections. The Java
        // classpath uses only the bounded main section before the first blank.
        let bytes = read_main_manifest(manifest)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| Error::Resources)?;
        let mut fields = Vec::<String>::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix(' ') {
                if let Some(previous) = fields.last_mut() {
                    previous.push_str(rest);
                } else {
                    return Err(Error::Resources);
                }
            } else {
                fields.push(line.into());
            }
        }
        let mut seen = false;
        for field in fields {
            let Some((key, value)) = field.split_once(':') else {
                continue;
            };
            if !key.eq_ignore_ascii_case("Class-Path") {
                continue;
            }
            if seen {
                return Err(Error::Resources);
            }
            seen = true;
            for entry in value.split_ascii_whitespace() {
                if entry.contains([':', '\\', '%']) || entry.starts_with('/') {
                    return Err(Error::UnsafePath);
                }
                let mut path = record
                    .name
                    .split('/')
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                path.pop();
                for part in entry.split('/') {
                    match part {
                        "" | "." => {}
                        ".." => {
                            if path.pop().is_none() {
                                return Err(Error::UnsafePath);
                            }
                        }
                        _ => path.push(part.into()),
                    }
                }
                let name = path.join("/");
                if !names.contains(name.as_str()) {
                    // Legacy Forge installs the exact root launch JAR again in
                    // its Maven tree. Its manifest still names dependencies
                    // relative to the root launch JAR. Permit the nonexistent
                    // nested alias only when both retained JARs are identical,
                    // the root JAR is required by our compiled recipe, and the
                    // original root-relative dependency is in the inventory.
                    // The root JAR's own manifest remains strictly validated.
                    let duplicate_root = record.name.starts_with("libraries/")
                        && records.iter().any(|original| {
                            !original.name.contains('/')
                                && required.contains(&original.name)
                                && record.name.rsplit('/').next() == Some(original.name.as_str())
                                && original.observation.bytes == record.observation.bytes
                                && original.observation.sha256 == record.observation.sha256
                        });
                    if duplicate_root
                        && recovery::valid_relative(entry).is_ok()
                        && artifact_name(entry)
                        && names.contains(entry)
                        && !root.join(&name).try_exists()?
                    {
                        continue;
                    }
                    // Official SVC distributions retain development/shaded
                    // dependency names in Class-Path. A missing sibling is
                    // ignored by Java. Permit that only in the exact pinned
                    // mods inventory, never in the bootstrap/recipe graph.
                    if pinned_mod
                        && !entry.contains('/')
                        && artifact_name(&name)
                        && name.starts_with("mods/")
                        && !root.join(&name).try_exists()?
                    {
                        continue;
                    }
                    #[cfg(test)]
                    eprintln!(
                        "native_server_classpath_missing jar={} entry={entry}",
                        record.name
                    );
                    return Err(Error::Resources);
                }
            }
        }
    }
    Ok(())
}
#[cfg(feature = "connect-voice-lab")]
struct Capture {
    records: Vec<ResourceRecord>,
    held: Vec<HeldArtifact>,
    directories: Vec<File>,
    visited: usize,
    bytes: u64,
}
#[cfg(feature = "connect-voice-lab")]
fn capture_artifacts(
    root: &Path,
    relative: &str,
    capture: &mut Capture,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<()> {
    if cancelled() {
        return Err(Error::Cancelled);
    }
    if relative.split('/').count() > 24 {
        return Err(Error::Limit);
    }
    let directory = if relative.is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative)
    };
    capture
        .directories
        .extend(recovery::pin_directory(&directory)?);
    for entry in fs::read_dir(&directory)? {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        capture.visited += 1;
        if capture.visited > 4096 {
            return Err(Error::Limit);
        }
        let entry = entry?;
        let leaf = entry
            .file_name()
            .into_string()
            .map_err(|_| Error::UnsafePath)?;
        let name = if relative.is_empty() {
            leaf
        } else {
            format!("{relative}/{leaf}")
        };
        recovery::valid_relative(&name)?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if crate::engine::content_updates::metadata_is_link(&metadata) {
            return Err(Error::UnsafePath);
        }
        if metadata.is_dir() {
            if name == "libraries"
                || name == "mods"
                || relative.starts_with("libraries/")
                || relative == "libraries"
                || relative.starts_with("mods/")
                || relative == "mods"
            {
                capture_artifacts(root, &name, capture, cancelled)?;
            }
            continue;
        }
        if !metadata.is_file() {
            return Err(Error::UnsafePath);
        }
        if !artifact_name(&name) {
            continue;
        }
        if capture.records.len() >= 512 || metadata.len() == 0 || metadata.len() > 256 * 1024 * 1024
        {
            return Err(Error::Limit);
        }
        capture.bytes = capture
            .bytes
            .checked_add(metadata.len())
            .ok_or(Error::Limit)?;
        if capture.bytes > 1024 * 1024 * 1024 {
            return Err(Error::Limit);
        }
        let file = recovery::secure_open(&entry.path())?;
        if file.metadata()?.len() != metadata.len() {
            return Err(Error::Changed);
        }
        let (observation, _) = observe_file(&entry.path(), metadata.len())?;
        if cancelled() {
            return Err(Error::Cancelled);
        }
        if file_identity(&file)? != observation.identity || observation.bytes != metadata.len() {
            return Err(Error::Changed);
        }
        capture.records.push(ResourceRecord {
            name,
            observation: observation.clone(),
        });
        capture.held.push(HeldArtifact {
            path: entry.path(),
            file,
            expected: observation,
        });
    }
    Ok(())
}
