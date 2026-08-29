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
        if filename.is_empty() || filename.contains(['/', '\\']) || !filename.ends_with(".jar") {
            return Err(InstanceError::InvalidId(filename.to_owned()));
        }
        self.ensure_layout()?;
        let destination = self.mods_dir().join(filename);
        let temporary = destination.with_extension("aurora-writing");
        fs::write(&temporary, contents)?;
        if destination.exists() {
            fs::remove_file(&destination)?;
        }
        fs::rename(&temporary, &destination)?;
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
}
