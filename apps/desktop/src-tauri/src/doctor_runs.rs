//! Launcher-owned execution records. Analysis never writes into an instance.
#[cfg(feature = "connect-lab")]
#[path = "doctor_connect_terminal.rs"]
mod connect_terminal;
#[cfg(feature = "connect-lab")]
pub use connect_terminal::ConnectTerminalRecorder;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use aurora_launcher_core::engine::{
    canonical_launch_failure, recovery, DoctorRun, InstanceId, InstanceLayout,
};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

const STORAGE_ERROR: &str = "Não foi possível acessar o registro local do diagnóstico.";

#[derive(Clone, Deserialize, Serialize)]
struct StoredRun {
    owner: String,
    instance_created_at_ns: Option<u64>,
    #[serde(default)]
    filesystem_identity: Option<String>,
    run: DoctorRun,
}

pub struct DoctorRuns {
    owner: String,
    records: Mutex<BTreeMap<String, StoredRun>>,
    memory_only: Mutex<BTreeSet<String>>,
}

impl Default for DoctorRuns {
    fn default() -> Self {
        Self {
            owner: uuid::Uuid::new_v4().to_string(),
            records: Mutex::new(BTreeMap::new()),
            memory_only: Mutex::new(BTreeSet::new()),
        }
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// A link in any component could redirect our private execution journal.
pub fn check_path(path: &Path) -> Result<(), String> {
    for component in path.ancestors() {
        match fs::symlink_metadata(component) {
            Ok(metadata) => {
                #[cfg(windows)]
                let linked = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let linked = metadata.file_type().is_symlink();
                if linked {
                    return Err(STORAGE_ERROR.into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(STORAGE_ERROR.into()),
        }
    }
    Ok(())
}

fn instance_creation(directory: &Path, instance_id: &str) -> Result<Option<u64>, String> {
    let root = directory
        .parent()
        .ok_or(STORAGE_ERROR)?
        .join("instances")
        .join(instance_id);
    check_path(&root)?;
    Ok(fs::metadata(root)
        .ok()
        .and_then(|metadata| metadata.created().ok())
        .and_then(|created| created.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_nanos()).ok()))
}

fn verified_identity(directory: &Path, instance_id: &str) -> Result<Option<String>, String> {
    let id = InstanceId::parse(instance_id).map_err(|_| STORAGE_ERROR.to_owned())?;
    let data = directory.parent().ok_or(STORAGE_ERROR)?;
    let instance = InstanceLayout::new(data).open(id);
    if !instance.root().exists() {
        return Ok(None);
    }
    recovery::filesystem_identity(&instance)
        .map(Some)
        .map_err(|_| STORAGE_ERROR.to_owned())
}

fn record_key(instance_id: &str, identity: &Option<String>) -> String {
    identity
        .as_ref()
        .map(|id| format!("identity:{id}"))
        .unwrap_or_else(|| instance_id.to_owned())
}

fn record_path(directory: &Path, instance_id: &str) -> PathBuf {
    // Opaque filename: instance names never become filesystem instructions.
    directory.join(format!("{:x}.json", Sha1::digest(instance_id.as_bytes())))
}

fn secure_open(path: &Path) -> Result<File, String> {
    check_path(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    let file = options.open(path).map_err(|_| STORAGE_ERROR.to_owned())?;
    let metadata = file.metadata().map_err(|_| STORAGE_ERROR.to_owned())?;
    if !metadata.is_file() || metadata.len() > 256 * 1024 {
        return Err(STORAGE_ERROR.into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(STORAGE_ERROR.into());
        }
    }
    Ok(file)
}

impl DoctorRuns {
    fn read_disk(directory: &Path, instance_id: &str) -> Result<Option<StoredRun>, String> {
        let path = record_path(directory, instance_id);
        check_path(&path)?;
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(STORAGE_ERROR.into()),
            Ok(_) => {}
        }
        let mut bytes = Vec::new();
        secure_open(&path)?
            .take(256 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| STORAGE_ERROR.to_owned())?;
        if bytes.len() > 256 * 1024 {
            return Err(STORAGE_ERROR.into());
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| STORAGE_ERROR.to_owned())
    }

    pub fn read(&self, directory: &Path, instance_id: &str) -> Result<Option<DoctorRun>, String> {
        let identity = verified_identity(directory, instance_id)?;
        let key = record_key(instance_id, &identity);
        let created = instance_creation(directory, instance_id)?;
        let mut records = self.records.lock().map_err(|_| STORAGE_ERROR.to_owned())?;
        if !records.contains_key(&key) {
            let mut memory_only = self
                .memory_only
                .lock()
                .map_err(|_| STORAGE_ERROR.to_owned())?;
            if memory_only.contains(&key) {
                return Ok(None);
            }
            let candidate = (|| {
                if let Some(stored) = Self::read_disk(directory, &key)? {
                    return Ok(Some(stored));
                }
                // Legacy diagnosis records remain readable when their original
                // directory creation stamp matches. They do not create an audit
                // snapshot or a recovery-point association.
                if identity.is_some() {
                    if let Some(stored) = records.get(instance_id) {
                        return Ok(Some(stored.clone()));
                    }
                    return Self::read_disk(directory, instance_id);
                }
                Ok(None)
            })();
            match candidate {
                Ok(Some(mut stored)) => {
                    if stored.filesystem_identity.is_none() && identity.is_some() {
                        if created.is_none() || stored.instance_created_at_ns != created {
                            return Ok(None);
                        }
                        stored.filesystem_identity = identity.clone();
                    }
                    records.insert(key.clone(), stored);
                }
                Ok(None) => return Ok(None),
                Err(_) => {
                    // An unsafe/corrupt journal is never overwritten. Continue
                    // tracking in memory without blocking a healthy launch.
                    memory_only.insert(key.clone());
                    return Ok(None);
                }
            }
        }
        if records.get(&key).is_some_and(|stored| {
            stored.filesystem_identity != identity
                || (identity.is_none() && stored.instance_created_at_ns != created)
        }) {
            records.remove(&key);
            return Ok(None);
        }
        Ok(records.get(&key).map(|stored| {
            let mut run = stored.run.clone();
            if stored.owner != self.owner && matches!(run.outcome.as_str(), "preparing" | "running")
            {
                run.outcome = "unknown".into();
            }
            run
        }))
    }

    pub fn start(&self, directory: &Path, instance_id: &str, run: DoctorRun) -> Result<(), String> {
        // Check existing storage even for direct callers before any write.
        self.read(directory, instance_id)?;
        self.save_record(directory, instance_id, run, true)
    }

    pub fn save(&self, directory: &Path, instance_id: &str, run: DoctorRun) -> Result<(), String> {
        self.save_record(directory, instance_id, run, false)
    }

    fn save_record(
        &self,
        directory: &Path,
        instance_id: &str,
        run: DoctorRun,
        starting: bool,
    ) -> Result<(), String> {
        let identity = verified_identity(directory, instance_id)?;
        let key = record_key(instance_id, &identity);
        let mut records = self.records.lock().map_err(|_| STORAGE_ERROR.to_owned())?;
        if !starting
            && records
                .get(&key)
                .is_none_or(|current| current.run.run_id != run.run_id)
        {
            return Err("A tentativa de inicialização já foi substituída.".into());
        }
        if let Some(current) = records.get(&key) {
            if current.owner == self.owner {
                if starting && matches!(current.run.outcome.as_str(), "preparing" | "running") {
                    return Err("Uma inicialização desta instância já está em andamento.".into());
                }
                if !starting && !matches!(current.run.outcome.as_str(), "preparing" | "running") {
                    // A very short JVM can exit before launch_instance returns.
                    return Ok(());
                }
            }
        }
        let stored = StoredRun {
            owner: self.owner.clone(),
            instance_created_at_ns: instance_creation(directory, instance_id)?,
            filesystem_identity: identity,
            run,
        };
        self.persist(directory, &key, &stored)?;
        records.insert(key, stored);
        Ok(())
    }

    fn persist(
        &self,
        directory: &Path,
        instance_id: &str,
        stored: &StoredRun,
    ) -> Result<(), String> {
        let mut memory_only = self
            .memory_only
            .lock()
            .map_err(|_| STORAGE_ERROR.to_owned())?;
        if memory_only.contains(instance_id) {
            return Ok(());
        }
        if Self::persist_to_disk(directory, instance_id, stored).is_err() {
            // Write failures disable only journal persistence, not the in-memory
            // execution identity, concurrent-start guard or terminal outcome.
            memory_only.insert(instance_id.to_owned());
        }
        Ok(())
    }

    fn persist_to_disk(
        directory: &Path,
        instance_id: &str,
        stored: &StoredRun,
    ) -> Result<(), String> {
        check_path(directory)?;
        fs::create_dir_all(directory).map_err(|_| STORAGE_ERROR.to_owned())?;
        check_path(directory)?;
        let destination = record_path(directory, instance_id);
        check_path(&destination)?;
        let temporary = directory.join(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let bytes = serde_json::to_vec(stored).map_err(|_| STORAGE_ERROR.to_owned())?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|_| STORAGE_ERROR.to_owned())?;
            file.write_all(&bytes)
                .map_err(|_| STORAGE_ERROR.to_owned())?;
            file.sync_all().map_err(|_| STORAGE_ERROR.to_owned())?;
            drop(file);
            check_path(&destination)?;
            fs::rename(&temporary, &destination).map_err(|_| STORAGE_ERROR.to_owned())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }

    pub fn finish(
        &self,
        directory: &Path,
        instance_id: &str,
        run_id: &str,
        outcome: &str,
        exit_code: Option<i32>,
        failure: Option<&str>,
    ) -> Result<DoctorRun, String> {
        // Populate from disk before taking the mutation lock; recheck under it.
        self.read(directory, instance_id)?;
        let identity = verified_identity(directory, instance_id)?;
        let key = record_key(instance_id, &identity);
        let mut records = self.records.lock().map_err(|_| STORAGE_ERROR.to_owned())?;
        let stored = records
            .get_mut(&key)
            .ok_or("Execução não encontrada para diagnóstico.")?;
        if stored.run.run_id != run_id {
            return Err(
                "A instância já tem uma execução mais recente; analise a execução atual.".into(),
            );
        }
        if !matches!(stored.run.outcome.as_str(), "preparing" | "running") {
            return Ok(stored.run.clone());
        }
        let mut run = stored.run.clone();
        run.outcome = outcome.into();
        run.ended_at_ms = Some(now_ms());
        run.exit_code = exit_code;
        run.failure = failure.map(canonical_launch_failure);
        let next = StoredRun {
            owner: self.owner.clone(),
            instance_created_at_ns: stored.instance_created_at_ns,
            filesystem_identity: stored.filesystem_identity.clone(),
            run: run.clone(),
        };
        // Keep a observed outcome available in memory even if persistence fails.
        *stored = next;
        self.persist(directory, &key, stored)?;
        Ok(run)
    }
}

pub fn outcome_for_exit(code: Option<i32>, success: bool) -> &'static str {
    if success {
        "success"
    }
    // Windows Ctrl+C/console close and conventional SIGINT/SIGTERM exit codes.
    else if matches!(code, Some(-1073741510 | 130 | 143)) {
        "cancelled"
    } else {
        "failed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run() -> DoctorRun {
        DoctorRun {
            run_id: uuid::Uuid::new_v4().to_string(),
            started_at_ms: now_ms(),
            ended_at_ms: None,
            outcome: "running".into(),
            exit_code: None,
            version_id: "1.20.1".into(),
            minecraft_version: "1.20.1".into(),
            java_executable: String::new(),
            java_major: Some(17),
            max_memory_mb: 4096,
            failure: None,
            logs_before: Vec::new(),
        }
    }

    #[test]
    fn normal_and_voluntary_exits_are_not_crashes() {
        assert_eq!(outcome_for_exit(Some(0), true), "success");
        for code in [-1073741510, 130, 143] {
            assert_eq!(outcome_for_exit(Some(code), false), "cancelled");
        }
        assert_eq!(outcome_for_exit(Some(1), false), "failed");
    }

    #[test]
    fn durable_latest_run_and_lost_observer_are_conservative() {
        let directory =
            std::env::temp_dir().join(format!("aurora-doctor-journal-{}", uuid::Uuid::new_v4()));
        let registry = DoctorRuns::default();
        let first = run();
        registry
            .start(&directory, "fixture", first.clone())
            .unwrap();
        assert_eq!(
            registry
                .read(&directory, "fixture")
                .unwrap()
                .unwrap()
                .outcome,
            "running"
        );
        assert_eq!(
            DoctorRuns::default()
                .read(&directory, "fixture")
                .unwrap()
                .unwrap()
                .outcome,
            "unknown"
        );
        registry
            .finish(
                &directory,
                "fixture",
                &first.run_id,
                "success",
                Some(0),
                None,
            )
            .unwrap();
        let second = run();
        registry
            .start(&directory, "fixture", second.clone())
            .unwrap();
        assert!(registry
            .finish(
                &directory,
                "fixture",
                &first.run_id,
                "failed",
                Some(1),
                None
            )
            .is_err());
        registry
            .finish(
                &directory,
                "fixture",
                &second.run_id,
                "success",
                Some(0),
                None,
            )
            .unwrap();
        registry.save(&directory, "fixture", second).unwrap();
        assert!(registry.save(&directory, "fixture", first).is_err());
        assert_eq!(
            registry
                .read(&directory, "fixture")
                .unwrap()
                .unwrap()
                .outcome,
            "success"
        );
        assert_eq!(
            DoctorRuns::default()
                .read(&directory, "fixture")
                .unwrap()
                .unwrap()
                .outcome,
            "success"
        );
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn records_only_sanitized_preparation_failures() {
        let directory =
            std::env::temp_dir().join(format!("aurora-doctor-privacy-{}", uuid::Uuid::new_v4()));
        let registry = DoctorRuns::default();
        let attempt = run();
        registry
            .start(&directory, "fixture", attempt.clone())
            .unwrap();
        registry.finish(&directory, "fixture", &attempt.run_id, "failed", None,
            Some("Failed to download https://private.invalid/?token=synthetic-secret C:/Users/private-player/file")).unwrap();
        let bytes = fs::read_to_string(record_path(&directory, "fixture")).unwrap();
        for private in ["private.invalid", "synthetic-secret", "private-player"] {
            assert!(!bytes.contains(private));
        }
        assert!(registry
            .read(&directory, "fixture")
            .unwrap()
            .unwrap()
            .failure
            .unwrap()
            .contains("download"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn concurrent_begins_keep_one_authoritative_run() {
        let directory = std::env::temp_dir().join(format!(
            "aurora-doctor-concurrency-{}",
            uuid::Uuid::new_v4()
        ));
        let registry = std::sync::Arc::new(DoctorRuns::default());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut threads = Vec::new();
        for _ in 0..2 {
            let registry = registry.clone();
            let barrier = barrier.clone();
            let directory = directory.clone();
            threads.push(std::thread::spawn(move || {
                let attempt = run();
                barrier.wait();
                registry.start(&directory, "fixture", attempt).is_ok()
            }));
        }
        barrier.wait();
        let successes = threads
            .into_iter()
            .map(|thread| usize::from(thread.join().unwrap()))
            .sum::<usize>();
        assert_eq!(successes, 1);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn recreated_instance_does_not_inherit_previous_execution() {
        let data =
            std::env::temp_dir().join(format!("aurora-doctor-identity-{}", uuid::Uuid::new_v4()));
        let instance = data.join("instances/fixture");
        fs::create_dir_all(&instance).unwrap();
        let directory = data.join("doctor-runs");
        let registry = DoctorRuns::default();
        registry.start(&directory, "fixture", run()).unwrap();
        fs::rename(&instance, data.join("previous-instance")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::create_dir(&instance).unwrap();
        assert!(registry.read(&directory, "fixture").unwrap().is_none());
        assert!(DoctorRuns::default()
            .read(&directory, "fixture")
            .unwrap()
            .is_none());
        fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn verified_run_survives_rename_and_recovery_storage_failure() {
        use aurora_launcher_core::engine::{acquire_instance_mutation_lock, recovery};
        let data =
            std::env::temp_dir().join(format!("aurora-doctor-identity-{}", uuid::Uuid::new_v4()));
        let instance = InstanceLayout::new(&data).open(InstanceId::parse("fixture").unwrap());
        instance.ensure_layout().unwrap();
        let directory = data.join("doctor-runs");
        let registry = DoctorRuns::default();
        let attempt = run();
        registry
            .start(&directory, "fixture", attempt.clone())
            .unwrap();
        let guard = acquire_instance_mutation_lock(&instance).unwrap();
        recovery::capture_observation(&instance, &guard).unwrap();
        drop(guard);
        fs::write(
            data.join(".aurora/recovery/index.sqlite3"),
            b"corrupt recovery storage",
        )
        .unwrap();
        registry
            .finish(
                &directory,
                "fixture",
                &attempt.run_id,
                "failed",
                Some(1),
                None,
            )
            .unwrap();
        fs::rename(instance.root(), data.join("instances/renamed")).unwrap();
        let restored = DoctorRuns::default()
            .read(&directory, "renamed")
            .unwrap()
            .unwrap();
        assert_eq!(restored.run_id, attempt.run_id);
        assert_eq!(restored.outcome, "failed");
        assert!(registry.read(&directory, "fixture").unwrap().is_none());
        assert_eq!(data.parent(), Some(std::env::temp_dir().as_path()));
        fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn legacy_diagnosis_stays_readable_when_recovery_identity_is_initialized() {
        use aurora_launcher_core::engine::{acquire_instance_mutation_lock, recovery};
        let data =
            std::env::temp_dir().join(format!("aurora-doctor-identity-{}", uuid::Uuid::new_v4()));
        let instance = InstanceLayout::new(&data).open(InstanceId::parse("fixture").unwrap());
        instance.ensure_layout().unwrap();
        let directory = data.join("doctor-runs");
        let mut attempt = run();
        attempt.outcome = "failed".into();
        let stored = StoredRun {
            owner: uuid::Uuid::new_v4().to_string(),
            instance_created_at_ns: instance_creation(&directory, "fixture").unwrap(),
            filesystem_identity: None,
            run: attempt.clone(),
        };
        DoctorRuns::persist_to_disk(&directory, "fixture", &stored).unwrap();
        let original = fs::read(record_path(&directory, "fixture")).unwrap();
        let guard = acquire_instance_mutation_lock(&instance).unwrap();
        recovery::capture_observation(&instance, &guard).unwrap();
        drop(guard);
        let registry = DoctorRuns::default();
        assert_eq!(
            registry
                .read(&directory, "fixture")
                .unwrap()
                .unwrap()
                .run_id,
            attempt.run_id
        );
        assert_eq!(
            fs::read(record_path(&directory, "fixture")).unwrap(),
            original,
            "reading preserves the existing diagnosis journal"
        );
        fs::rename(instance.root(), data.join("old-instance")).unwrap();
        instance.ensure_layout().unwrap();
        assert!(registry.read(&directory, "fixture").unwrap().is_none());
        assert_eq!(data.parent(), Some(std::env::temp_dir().as_path()));
        fs::remove_dir_all(data).unwrap();
    }

    fn cleanup_fallback_fixture(directory: &Path) {
        assert_eq!(directory.parent(), Some(std::env::temp_dir().as_path()));
        assert!(directory
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("aurora-doctor-fallback-"));
        check_path(directory).unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupt_journal_does_not_block_launch_tracking_or_get_overwritten() {
        let directory =
            std::env::temp_dir().join(format!("aurora-doctor-fallback-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let path = record_path(&directory, "fixture");
        let original = b"{corrupt or unknown journal";
        fs::write(&path, original).unwrap();
        let registry = DoctorRuns::default();
        let first = run();
        assert!(registry.read(&directory, "fixture").unwrap().is_none());
        registry
            .start(&directory, "fixture", first.clone())
            .unwrap();
        registry.save(&directory, "fixture", first.clone()).unwrap();
        assert_eq!(
            registry
                .read(&directory, "fixture")
                .unwrap()
                .unwrap()
                .run_id,
            first.run_id
        );
        assert!(
            registry.start(&directory, "fixture", run()).is_err(),
            "memory fallback keeps the concurrent-launch guard"
        );
        registry
            .finish(
                &directory,
                "fixture",
                &first.run_id,
                "success",
                Some(0),
                None,
            )
            .unwrap();
        registry.save(&directory, "fixture", first.clone()).unwrap();
        assert_eq!(
            registry
                .read(&directory, "fixture")
                .unwrap()
                .unwrap()
                .outcome,
            "success",
            "a delayed launch result cannot overwrite a terminal outcome"
        );
        let second = run();
        registry
            .start(&directory, "fixture", second.clone())
            .unwrap();
        assert!(registry
            .finish(
                &directory,
                "fixture",
                &first.run_id,
                "failed",
                Some(1),
                None
            )
            .is_err());
        assert_eq!(
            registry
                .read(&directory, "fixture")
                .unwrap()
                .unwrap()
                .run_id,
            second.run_id
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(
            DoctorRuns::default()
                .read(&directory, "fixture")
                .unwrap()
                .is_none(),
            "memory-only history does not invent a durable record"
        );
        cleanup_fallback_fixture(&directory);
    }

    #[test]
    fn write_failure_after_begin_keeps_the_run_and_exit_result_available() {
        let directory =
            std::env::temp_dir().join(format!("aurora-doctor-fallback-{}", uuid::Uuid::new_v4()));
        let registry = DoctorRuns::default();
        let mut attempt = run();
        attempt.outcome = "preparing".into();
        registry
            .start(&directory, "fixture", attempt.clone())
            .unwrap();
        let destination = record_path(&directory, "fixture");
        fs::remove_file(&destination).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("sentinel"), "preserve").unwrap();
        attempt.outcome = "running".into();
        registry
            .save(&directory, "fixture", attempt.clone())
            .unwrap();
        assert_eq!(
            registry
                .read(&directory, "fixture")
                .unwrap()
                .unwrap()
                .outcome,
            "running"
        );
        registry
            .finish(
                &directory,
                "fixture",
                &attempt.run_id,
                "failed",
                Some(1),
                Some("failed to download"),
            )
            .unwrap();
        assert_eq!(
            registry
                .read(&directory, "fixture")
                .unwrap()
                .unwrap()
                .outcome,
            "failed"
        );
        assert_eq!(
            fs::read_to_string(destination.join("sentinel")).unwrap(),
            "preserve"
        );
        assert_eq!(
            fs::read_dir(&directory).unwrap().count(),
            1,
            "failed atomic writes leave no temporary journal behind"
        );
        cleanup_fallback_fixture(&directory);
    }

    #[test]
    fn concurrent_memory_only_starts_still_keep_one_authoritative_run() {
        let directory =
            std::env::temp_dir().join(format!("aurora-doctor-fallback-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        fs::write(record_path(&directory, "fixture"), b"invalid JSON").unwrap();
        let registry = std::sync::Arc::new(DoctorRuns::default());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let threads = (0..2)
            .map(|_| {
                let registry = registry.clone();
                let barrier = barrier.clone();
                let directory = directory.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    registry.start(&directory, "fixture", run()).is_ok()
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        assert_eq!(
            threads
                .into_iter()
                .map(|thread| usize::from(thread.join().unwrap()))
                .sum::<usize>(),
            1
        );
        assert_eq!(
            fs::read(record_path(&directory, "fixture")).unwrap(),
            b"invalid JSON"
        );
        cleanup_fallback_fixture(&directory);
    }

    #[cfg(windows)]
    #[test]
    fn linked_journal_storage_uses_memory_without_writing_through_the_link() {
        use std::os::windows::process::CommandExt;
        let directory =
            std::env::temp_dir().join(format!("aurora-doctor-fallback-{}", uuid::Uuid::new_v4()));
        let external = directory.join("external");
        let journal = directory.join("doctor-runs");
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("sentinel"), "preserve").unwrap();
        let result = std::process::Command::new("powershell.exe")
            .creation_flags(0x08000000)
            .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:AURORA_DOCTOR_QA_LINK -Target $env:AURORA_DOCTOR_QA_TARGET | Out-Null"])
            .env("AURORA_DOCTOR_QA_LINK", &journal).env("AURORA_DOCTOR_QA_TARGET", &external)
            .output().unwrap();
        assert!(
            result.status.success(),
            "synthetic junction creation failed"
        );
        let registry = DoctorRuns::default();
        let attempt = run();
        registry
            .start(&journal, "fixture", attempt.clone())
            .unwrap();
        registry
            .finish(
                &journal,
                "fixture",
                &attempt.run_id,
                "success",
                Some(0),
                None,
            )
            .unwrap();
        assert_eq!(
            registry.read(&journal, "fixture").unwrap().unwrap().outcome,
            "success"
        );
        assert_eq!(fs::read_dir(&external).unwrap().count(), 1);
        assert_eq!(
            fs::read_to_string(external.join("sentinel")).unwrap(),
            "preserve"
        );
        fs::remove_dir(&journal).expect("unlink only the test junction");
        cleanup_fallback_fixture(&directory);
    }
}
