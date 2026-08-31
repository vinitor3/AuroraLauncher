use std::fs;
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

    pub fn open(&self, id: InstanceId) -> Instance {
        let root = self.path_for(&id);
        Instance { id, root }
    }
}

#[derive(Clone, Debug)]
pub struct Instance {
    id: InstanceId,
    root: PathBuf,
}

impl Instance {
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

    /// Copia o Companion para a instância sem tocar em outros mods.
    pub fn install_companion(&self, companion_jar: &Path) -> Result<PathBuf, InstanceError> {
        self.ensure_layout()?;
        let filename = companion_jar
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| InstanceError::InvalidId(companion_jar.display().to_string()))?;
        let destination = self.mods_dir().join(filename);
        fs::copy(companion_jar, &destination)?;
        Ok(destination)
    }

    /// Instala um artefato Companion que já foi validado e empacotado pelo
    /// Aurora. A troca atômica evita deixar um JAR parcialmente gravado.
    pub fn install_companion_bytes(
        &self,
        filename: &str,
        contents: &[u8],
    ) -> Result<PathBuf, InstanceError> {
        self.install_aurora_artifact_bytes(filename, contents)
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
        assert!(!installed.with_extension("aurora-writing").exists());
        assert!(!installed.with_extension("aurora-backup").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
