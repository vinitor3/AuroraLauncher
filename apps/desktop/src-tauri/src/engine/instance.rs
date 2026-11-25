use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum InstanceError {
    #[error("identificador de instância inválido: {0}")]
    InvalidId(String),
    #[error("erro de E/S: {0}")]
    Io(#[from] std::io::Error),
}

/// Identificador seguro para uma pasta abaixo de `instances`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InstanceId(String);

impl InstanceId {
    pub fn parse(value: impl Into<String>) -> Result<Self, InstanceError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 64
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            && value != "."
            && value != "..";

        if valid {
            Ok(Self(value))
        } else {
            Err(InstanceError::InvalidId(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct InstanceLayout {
    root: PathBuf,
}

impl InstanceLayout {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            root: data_dir.into(),
        }
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.root.join("instances")
    }

    pub fn path_for(&self, id: &InstanceId) -> PathBuf {
        self.instances_dir().join(id.as_str())
    }

    /// Reserve a fresh directory even when another launcher installs the same pack concurrently.
    pub fn reserve_new(&self, preferred: InstanceId) -> Result<Instance, InstanceError> {
        fs::create_dir_all(self.instances_dir())?;
        let base = preferred.as_str();
        for number in 1..=10_000 {
            let id = if number == 1 {
                preferred.clone()
            } else {
                let suffix = format!("-{number}");
                let prefix = &base[..base.len().min(64 - suffix.len())];
                InstanceId::parse(format!("{prefix}{suffix}"))?
            };
            let instance = self.open(id);
            match fs::create_dir(instance.root()) {
                Ok(()) => return Ok(instance),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "Não há um nome disponível para esta instância.",
        )
        .into())
    }

    pub fn open(&self, id: InstanceId) -> Instance {
        let root = self.path_for(&id);
        Instance {
            id,
            root,
            data_dir: self.root.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Instance {
    data_dir: PathBuf,
    id: InstanceId,
    root: PathBuf,
}

impl Instance {
    pub(super) fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn id(&self) -> &InstanceId {
        &self.id
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn mods_dir(&self) -> PathBuf {
        self.root.join("mods")
    }

    pub fn natives_dir(&self) -> PathBuf {
        self.root.join("natives")
    }

    pub fn ensure_layout(&self) -> Result<(), InstanceError> {
        let paths = [
            self.root.clone(),
            self.mods_dir(),
            self.natives_dir(),
            self.root.join("resourcepacks"),
            self.root.join("shaderpacks"),
            self.root.join("logs"),
        ];
        for path in paths {
            fs::create_dir_all(path)?;
        }
        Ok(())
    }

    /// Instala um JAR oficial Aurora com troca atômica e rollback local.
    pub fn install_aurora_artifact_bytes(
        &self,
        filename: &str,
        contents: &[u8],
    ) -> Result<PathBuf, InstanceError> {
        if filename.is_empty() || filename.contains(['/', '\\']) || !filename.ends_with(".jar") {
            return Err(InstanceError::InvalidId(filename.to_owned()));
        }
        self.ensure_layout()?;
        let destination = self.mods_dir().join(filename);
        let temporary = destination.with_extension("aurora-writing");
        let backup = destination.with_extension("aurora-backup");
        // Normal launches already have both packaged JARs. Compare the actual
        // bytes before replacing them, even when a modified JAR is the same size.
        if !temporary.exists()
            && !backup.exists()
            && fs::symlink_metadata(&destination)
                .is_ok_and(|info| info.is_file() && info.len() == contents.len() as u64)
        {
            let mut installed = Vec::with_capacity(contents.len());
            fs::File::open(&destination)?
                .take(contents.len() as u64 + 1)
                .read_to_end(&mut installed)?;
            if installed == contents {
                return Ok(destination);
            }
        }
        fs::write(&temporary, contents)?;
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        if destination.exists() {
            fs::rename(&destination, &backup)?;
        }
        if let Err(error) = fs::rename(&temporary, &destination) {
            if backup.exists() {
                let _ = fs::rename(&backup, &destination);
            }
            return Err(InstanceError::Io(error));
        }
        if backup.exists() {
            fs::remove_file(backup)?;
        }
        Ok(destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal() {
        assert!(InstanceId::parse("../surprise").is_err());
        assert!(InstanceId::parse("pack/name").is_err());
    }

    #[test]
    fn accepts_safe_identifier() {
        let id = InstanceId::parse("all-the-mods_10.2").unwrap();
        assert_eq!(id.as_str(), "all-the-mods_10.2");
    }

    #[test]
    fn atomically_replaces_an_aurora_artifact_without_leaving_work_files() {
        let root = std::env::temp_dir().join(format!(
            "aurora-artifact-replace-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("replace-test").unwrap());
        let installed = instance
            .install_aurora_artifact_bytes("aurora-core-test.jar", b"old")
            .unwrap();
        instance
            .install_aurora_artifact_bytes("aurora-core-test.jar", b"new")
            .unwrap();

        assert_eq!(fs::read(&installed).unwrap(), b"new");
        let probe = fs::File::options().write(true).open(&installed).unwrap();
        let old_time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        probe.set_modified(old_time).unwrap();
        drop(probe);
        instance
            .install_aurora_artifact_bytes("aurora-core-test.jar", b"new")
            .unwrap();
        assert_eq!(
            fs::metadata(&installed).unwrap().modified().unwrap(),
            old_time
        );
        assert!(!installed.with_extension("aurora-writing").exists());
        assert!(!installed.with_extension("aurora-backup").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn simultaneous_installs_never_reuse_an_existing_instance() {
        let scratch = std::env::temp_dir().join(format!("aurora-reserve-{}", uuid::Uuid::new_v4()));
        let layout = InstanceLayout::new(&scratch);
        let id = InstanceId::parse("a".repeat(64)).unwrap();
        let original = layout.reserve_new(id.clone()).unwrap();
        fs::write(original.root().join("keep.txt"), "original").unwrap();
        let jobs: Vec<_> = (0..4)
            .map(|_| {
                let layout = layout.clone();
                let id = id.clone();
                std::thread::spawn(move || layout.reserve_new(id).unwrap().root().to_owned())
            })
            .collect();
        let paths: std::collections::HashSet<_> =
            jobs.into_iter().map(|job| job.join().unwrap()).collect();
        assert_eq!(paths.len(), 4);
        assert!(!paths.contains(original.root()));
        assert_eq!(
            fs::read(original.root().join("keep.txt")).unwrap(),
            b"original"
        );
        fs::remove_dir_all(scratch).unwrap();
    }
}
