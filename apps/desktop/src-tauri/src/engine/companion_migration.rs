//! Retirement of launcher-distributed Companion JARs. No Companion bytes are
//! embedded in production: the catalog contains hashes of immutable releases.
//! A write-ahead journal and the original bytes remain outside Minecraft's mods
//! directory. An interrupted migration resumes before the next launch.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;
use zip::ZipArchive;

use super::content_updates::{
    ensure_instance_root_secure, ensure_no_symlink_components, metadata_is_link,
};
use super::{ContentUpdateError, Instance, InstanceMutationGuard};

const STORAGE: &str = ".aurora/removed-companion";
const MAX_METADATA: u64 = 128 * 1024;
const MAX_JAR: u64 = 64 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum CompanionMigrationError {
    #[error("encerre o Minecraft desta instância antes de retirar o mod Companion")]
    GameRunning,
    #[error("migração Companion sem bloqueio exclusivo desta instância")]
    WrongLock,
    #[error("caminho inseguro na migração Companion: {0}")]
    UnsafePath(String),
    #[error("Companion de origem ou integridade incerta: {0}; arquivo preservado, revise-o antes de iniciar")]
    Uncertain(String),
    #[error("migração Companion interrompida; o backup e o journal foram preservados: {0}")]
    Io(#[from] io::Error),
    #[error("journal ou catálogo da migração Companion inválido")]
    InvalidJournal,
    #[error(transparent)]
    Security(#[from] ContentUpdateError),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    schema_version: u32,
    identity: String,
    artifacts: Vec<KnownArtifact>,
}

#[derive(Deserialize)]
struct KnownArtifact {
    sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    filename: String,
    sha256: String,
}

/// The caller checks tracked/external game processes before acquiring this
/// guard, then keeps the guard until the launched JVM exits. Other installers,
/// updates and launch attempts use the same cross-process file lock.
pub fn migrate_managed_companion(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    game_running: bool,
) -> Result<usize, CompanionMigrationError> {
    migrate(instance, guard, game_running, false)
}

/// Read-only preflight: avoid probing unrelated JVMs on ordinary launches once
/// there is no retired mod or unfinished migration in this managed instance.
pub fn companion_migration_required(instance: &Instance) -> Result<bool, CompanionMigrationError> {
    secure_root(instance)?;
    let storage = secure_path(instance, STORAGE)?;
    if storage.exists() {
        for entry in fs::read_dir(&storage)? {
            let entry = entry?;
            let relative = relative_path(instance, &entry.path())?;
            secure_path(instance, &relative)?;
            let journal = secure_path(instance, &format!("{relative}/journal.json"))?;
            let complete = secure_path(instance, &format!("{relative}/complete"))?;
            if journal.exists() && !complete.exists() {
                return Ok(true);
            }
        }
    }
    let mods = secure_path(instance, "mods")?;
    if mods.exists() {
        for entry in fs::read_dir(&mods)? {
            let entry = entry?;
            let relative = relative_path(instance, &entry.path())?;
            let path = secure_path(instance, &relative)?;
            if path.is_file()
                && is_candidate_filename(&entry.file_name().to_string_lossy())
                && companion_identity(&path)?
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn is_candidate_filename(filename: &str) -> bool {
    let lower = filename.to_ascii_lowercase();
    lower.ends_with(".jar") || lower.ends_with(".disabled") || lower.ends_with(".jar.off")
}

fn migrate(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    game_running: bool,
    interrupt_after_move: bool,
) -> Result<usize, CompanionMigrationError> {
    if game_running {
        return Err(CompanionMigrationError::GameRunning);
    }
    secure_root(instance)?;
    if guard.root != instance.root().canonicalize()? {
        return Err(CompanionMigrationError::WrongLock);
    }
    let catalog: Catalog = serde_json::from_str(include_str!("known-companion-artifacts.json"))
        .map_err(|_| CompanionMigrationError::InvalidJournal)?;
    if catalog.schema_version != 1 || catalog.identity != "aurora_companion" {
        return Err(CompanionMigrationError::InvalidJournal);
    }
    let mods = secure_path(instance, "mods")?;
    let storage = secure_path(instance, STORAGE)?;
    let mut removed = 0;
    if storage.exists() {
        for entry in fs::read_dir(&storage)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if Uuid::parse_str(&name).is_err() {
                return Err(CompanionMigrationError::InvalidJournal);
            }
            let relative = format!("{STORAGE}/{name}");
            let transaction = secure_path(instance, &relative)?;
            let journal_path = secure_path(instance, &format!("{relative}/journal.json"))?;
            // A crash before journal publication cannot have moved a JAR.
            if !journal_path.exists() {
                continue;
            }
            if fs::metadata(&journal_path)?.len() > MAX_METADATA {
                return Err(CompanionMigrationError::InvalidJournal);
            }
            let journal: Journal = serde_json::from_reader(File::open(&journal_path)?)
                .map_err(|_| CompanionMigrationError::InvalidJournal)?;
            validate_journal(&journal, &catalog)?;
            let complete = secure_path(instance, &format!("{relative}/complete"))?;
            if !complete.exists() {
                finish_transaction(instance, &transaction, &journal, false)?;
                removed += 1;
            }
        }
    }
    if !mods.exists() {
        return Ok(removed);
    }
    // Scan before modifying anything so an ambiguous identity leaves the full
    // batch untouched. Filenames never authorize removal.
    let mut candidates = Vec::new();
    for entry in fs::read_dir(&mods)? {
        let entry = entry?;
        let filename = entry
            .file_name()
            .into_string()
            .map_err(|_| CompanionMigrationError::UnsafePath("nome não UTF-8".into()))?;
        let path = secure_path(instance, &format!("mods/{filename}"))?;
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() {
            continue;
        }
        if !is_candidate_filename(&filename) {
            continue;
        }
        let identity = companion_identity(&path)?;
        if metadata.len() > MAX_JAR {
            if identity {
                return Err(CompanionMigrationError::Uncertain(filename));
            }
            continue;
        }
        let hash = sha256(&path)?;
        if known_hash(&catalog, &hash) {
            if !identity {
                return Err(CompanionMigrationError::Uncertain(filename));
            }
            candidates.push(Journal {
                schema_version: 1,
                filename,
                sha256: hash,
            });
        } else if identity {
            return Err(CompanionMigrationError::Uncertain(filename));
        }
    }
    for journal in candidates {
        validate_journal(&journal, &catalog)?;
        let transaction = secure_path(instance, &format!("{STORAGE}/{}", Uuid::new_v4()))?;
        fs::create_dir_all(&transaction)?;
        secure_root(instance)?;
        secure_path(instance, &relative_path(instance, &transaction)?)?;
        let bytes = serde_json::to_vec_pretty(&journal)
            .map_err(|_| CompanionMigrationError::InvalidJournal)?;
        // Publish a fully synced journal before moving the only source copy.
        let temporary = transaction.join("journal.pending");
        write_new_synced(&temporary, &bytes)?;
        fs::rename(temporary, transaction.join("journal.json"))?;
        finish_transaction(instance, &transaction, &journal, interrupt_after_move)?;
        removed += 1;
    }
    Ok(removed)
}

fn finish_transaction(
    instance: &Instance,
    transaction: &Path,
    journal: &Journal,
    interrupt_after_move: bool,
) -> Result<(), CompanionMigrationError> {
    let source = secure_path(instance, &format!("mods/{}", journal.filename))?;
    let relative = relative_path(instance, transaction)?;
    let backup = secure_path(instance, &format!("{relative}/artifact.jar.backup"))?;
    match (source.exists(), backup.exists()) {
        (true, false) => {
            if sha256(&source)? != journal.sha256 {
                return Err(CompanionMigrationError::Uncertain(journal.filename.clone()));
            }
            fs::rename(&source, &backup)?;
            OpenOptions::new().write(true).open(&backup)?.sync_all()?;
        }
        (false, true) => {}
        _ => return Err(CompanionMigrationError::Uncertain(journal.filename.clone())),
    }
    if sha256(&backup)? != journal.sha256 {
        return Err(CompanionMigrationError::Uncertain(journal.filename.clone()));
    }
    if interrupt_after_move {
        return Err(io::Error::other("interrupção simulada após backup").into());
    }
    let complete = secure_path(instance, &format!("{relative}/complete"))?;
    write_new_synced(&complete, b"1\n")?;
    Ok(())
}

fn validate_journal(journal: &Journal, catalog: &Catalog) -> Result<(), CompanionMigrationError> {
    let filename = &journal.filename;
    if journal.schema_version != 1
        || filename.is_empty()
        || filename.contains(['/', '\\', ':'])
        || filename == "."
        || filename == ".."
        || !known_hash(catalog, &journal.sha256)
    {
        return Err(CompanionMigrationError::InvalidJournal);
    }
    Ok(())
}

fn known_hash(catalog: &Catalog, hash: &str) -> bool {
    catalog
        .artifacts
        .iter()
        .any(|artifact| artifact.sha256 == hash)
}

fn secure_root(instance: &Instance) -> Result<(), CompanionMigrationError> {
    ensure_instance_root_secure(instance)?;
    for ancestor in instance.root().ancestors() {
        if metadata_is_link(&fs::symlink_metadata(ancestor)?) {
            return Err(CompanionMigrationError::UnsafePath(
                ancestor.display().to_string(),
            ));
        }
    }
    Ok(())
}

fn secure_path(instance: &Instance, relative: &str) -> Result<PathBuf, CompanionMigrationError> {
    if relative.contains(':')
        || Path::new(relative)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(CompanionMigrationError::UnsafePath(relative.into()));
    }
    ensure_no_symlink_components(instance.root(), relative)?;
    Ok(instance.root().join(relative))
}

fn relative_path(instance: &Instance, path: &Path) -> Result<String, CompanionMigrationError> {
    path.strip_prefix(instance.root())
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|_| CompanionMigrationError::UnsafePath(path.display().to_string()))
}

fn write_new_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn sha256(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    io::copy(&mut file, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

fn companion_identity(path: &Path) -> Result<bool, CompanionMigrationError> {
    let Ok(mut archive) = ZipArchive::new(File::open(path)?) else {
        return Ok(false);
    };
    for name in ["fabric.mod.json", "mcmod.info", "META-INF/mods.toml"] {
        let Ok(entry) = archive.by_name(name) else {
            continue;
        };
        if entry.size() > MAX_METADATA {
            return Err(CompanionMigrationError::Uncertain(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ));
        }
        let mut text = String::new();
        entry.take(MAX_METADATA + 1).read_to_string(&mut text)?;
        if text.len() as u64 > MAX_METADATA {
            return Err(CompanionMigrationError::InvalidJournal);
        }
        if name.ends_with(".toml") {
            let mut in_mod = false;
            for line in text.lines().map(str::trim) {
                if line.starts_with('[') {
                    in_mod = line == "[[mods]]";
                }
                if in_mod {
                    if let Some((key, value)) = line.split_once('=') {
                        if key.trim() == "modId"
                            && value
                                .split('#')
                                .next()
                                .unwrap_or_default()
                                .trim()
                                .trim_matches(['\'', '"'])
                                == "aurora_companion"
                        {
                            return Ok(true);
                        }
                    }
                }
            }
        } else if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if json_has_companion_identity(&value) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn json_has_companion_identity(value: &serde_json::Value) -> bool {
    if let Some(values) = value.as_array() {
        return values.iter().any(json_has_companion_identity);
    }
    value
        .get("id")
        .or_else(|| value.get("modid"))
        .and_then(|v| v.as_str())
        == Some("aurora_companion")
        || value
            .get("modList")
            .is_some_and(json_has_companion_identity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{acquire_instance_mutation_lock, InstanceId, InstanceLayout};
    use zip::write::SimpleFileOptions;

    struct Fixture {
        root: PathBuf,
        instance: Instance,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("aurora-retirement-{}", Uuid::new_v4()));
            let instance = InstanceLayout::new(&root).open(InstanceId::parse("qa").unwrap());
            instance.ensure_layout().unwrap();
            Self { root, instance }
        }
        fn known(&self, name: &str) -> PathBuf {
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../releases/companion/1.20.1/fabric/aurora-companion-fabric-1.20.1-0.2.3.jar");
            let path = self.instance.mods_dir().join(name);
            fs::copy(source, &path).unwrap();
            path
        }
        fn jar(&self, name: &str, metadata: &str) -> PathBuf {
            let path = self.instance.mods_dir().join(name);
            let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
            zip.start_file("fabric.mod.json", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(metadata.as_bytes()).unwrap();
            zip.finish().unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    #[test]
    fn retires_known_stable_versioned_and_disabled_files_idempotently() {
        let fixture = Fixture::new();
        assert!(!companion_migration_required(&fixture.instance).unwrap());
        for name in [
            "aurora-companion-fabric-1.20.1.jar",
            "aurora-companion-fabric-1.20.1-0.2.3.jar",
            "renamed.jar.disabled",
        ] {
            fixture.known(name);
        }
        let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
        assert!(companion_migration_required(&fixture.instance).unwrap());
        assert_eq!(
            migrate_managed_companion(&fixture.instance, &guard, false).unwrap(),
            3
        );
        assert_eq!(
            migrate_managed_companion(&fixture.instance, &guard, false).unwrap(),
            0
        );
        assert!(!companion_migration_required(&fixture.instance).unwrap());
        assert_eq!(
            fs::read_dir(fixture.instance.mods_dir()).unwrap().count(),
            0
        );
        let transactions: Vec<_> = fs::read_dir(fixture.instance.root().join(STORAGE))
            .unwrap()
            .collect();
        assert_eq!(transactions.len(), 3);
        for entry in transactions {
            let transaction = entry.unwrap().path();
            let journal: Journal =
                serde_json::from_reader(File::open(transaction.join("journal.json")).unwrap())
                    .unwrap();
            assert_eq!(
                sha256(&transaction.join("artifact.jar.backup")).unwrap(),
                journal.sha256
            );
            assert!(transaction.join("complete").exists());
        }
    }

    #[test]
    fn all_historical_releases_match_catalog_and_metadata() {
        let fixture = Fixture::new();
        let value: serde_json::Value =
            serde_json::from_str(include_str!("known-companion-artifacts.json")).unwrap();
        for (index, artifact) in value["artifacts"].as_array().unwrap().iter().enumerate() {
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join(artifact["source"].as_str().unwrap());
            assert_eq!(
                sha256(&source).unwrap(),
                artifact["sha256"].as_str().unwrap()
            );
            assert!(companion_identity(&source).unwrap());
            fs::copy(
                source,
                fixture
                    .instance
                    .mods_dir()
                    .join(format!("release-{index}.jar.disabled")),
            )
            .unwrap();
        }
        let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
        assert_eq!(
            migrate_managed_companion(&fixture.instance, &guard, false).unwrap(),
            value["artifacts"].as_array().unwrap().len()
        );
    }

    #[test]
    fn preserves_similarly_named_third_party_mods_core_skins_and_desktop_data() {
        let fixture = Fixture::new();
        for name in [
            "aurora-companion-helper.jar",
            "aurora-core.jar",
            "aurora-skins.jar",
        ] {
            fixture.jar(
                name,
                r#"{"id":"third_party","depends":{"aurora_companion":"*"}}"#,
            );
        }
        let desktop = fixture.root.join("assistant-conversation.json");
        fs::write(&desktop, "preserved history and voice preferences").unwrap();
        fixture.known("retired.jar");
        let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
        assert_eq!(
            migrate_managed_companion(&fixture.instance, &guard, false).unwrap(),
            1
        );
        assert_eq!(
            fs::read_dir(fixture.instance.mods_dir()).unwrap().count(),
            3
        );
        assert_eq!(
            fs::read_to_string(desktop).unwrap(),
            "preserved history and voice preferences"
        );
    }

    #[test]
    fn unknown_companion_identity_is_reported_without_mutation() {
        let fixture = Fixture::new();
        let unknown = fixture.jar("custom-build.jar", r#"{"id":"aurora_companion"}"#);
        let known = fixture.known("known.jar");
        let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
        assert!(matches!(
            migrate_managed_companion(&fixture.instance, &guard, false),
            Err(CompanionMigrationError::Uncertain(_))
        ));
        assert!(unknown.exists() && known.exists());
    }

    #[test]
    fn running_game_and_wrong_instance_guard_block_migration() {
        let fixture = Fixture::new();
        let other = Fixture::new();
        let path = fixture.known("known.jar");
        let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
        let other_guard = acquire_instance_mutation_lock(&other.instance).unwrap();
        assert!(matches!(
            migrate_managed_companion(&fixture.instance, &guard, true),
            Err(CompanionMigrationError::GameRunning)
        ));
        assert!(matches!(
            migrate_managed_companion(&fixture.instance, &other_guard, false),
            Err(CompanionMigrationError::WrongLock)
        ));
        assert!(matches!(
            acquire_instance_mutation_lock(&fixture.instance),
            Err(ContentUpdateError::Busy)
        ));
        assert!(path.exists());
    }

    #[test]
    fn recovers_interruption_after_move_from_durable_journal() {
        let fixture = Fixture::new();
        let path = fixture.known("known.jar");
        let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
        assert!(migrate(&fixture.instance, &guard, false, true).is_err());
        assert!(!path.exists());
        assert_eq!(
            migrate_managed_companion(&fixture.instance, &guard, false).unwrap(),
            1
        );
        assert_eq!(
            migrate_managed_companion(&fixture.instance, &guard, false).unwrap(),
            0
        );
    }

    #[test]
    fn recovers_before_move_and_refuses_modified_source() {
        let fixture = Fixture::new();
        let source = fixture.known("known.jar");
        let transaction = fixture
            .instance
            .root()
            .join(STORAGE)
            .join(Uuid::new_v4().to_string());
        fs::create_dir_all(&transaction).unwrap();
        let journal = Journal {
            schema_version: 1,
            filename: "known.jar".into(),
            sha256: sha256(&source).unwrap(),
        };
        write_new_synced(
            &transaction.join("journal.json"),
            &serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
        fs::write(&source, b"changed by another program").unwrap();
        assert!(matches!(
            migrate_managed_companion(&fixture.instance, &guard, false),
            Err(CompanionMigrationError::Uncertain(_))
        ));
        fixture.known("known.jar");
        assert_eq!(
            migrate_managed_companion(&fixture.instance, &guard, false).unwrap(),
            1
        );
    }

    #[test]
    fn refuses_traversal_journal_without_touching_source() {
        let fixture = Fixture::new();
        let source = fixture.known("known.jar");
        let transaction = fixture
            .instance
            .root()
            .join(STORAGE)
            .join(Uuid::new_v4().to_string());
        fs::create_dir_all(&transaction).unwrap();
        let journal = Journal {
            schema_version: 1,
            filename: "../../outside.jar".into(),
            sha256: sha256(&source).unwrap(),
        };
        write_new_synced(
            &transaction.join("journal.json"),
            &serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
        assert!(matches!(
            migrate_managed_companion(&fixture.instance, &guard, false),
            Err(CompanionMigrationError::InvalidJournal)
        ));
        assert!(source.exists());
    }

    #[cfg(windows)]
    #[test]
    fn refuses_junction_in_mods_or_backup_path() {
        use crate::engine::process::background_command;
        for relative in ["mods", STORAGE] {
            let fixture = Fixture::new();
            let outside = fixture.root.join("outside");
            fs::create_dir(&outside).unwrap();
            let link = fixture.instance.root().join(relative);
            if link.exists() {
                fs::remove_dir(&link).unwrap();
            }
            fs::create_dir_all(link.parent().unwrap()).unwrap();
            let output = background_command("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(link.to_string_lossy().replace('/', "\\"))
                .arg(outside.to_string_lossy().replace('/', "\\"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
            assert!(migrate_managed_companion(&fixture.instance, &guard, false).is_err());
            // Remove the junction itself before fixture cleanup, never its target.
            fs::remove_dir(link).unwrap();
            assert!(outside.exists());
        }
    }
}
