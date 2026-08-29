//! Companion empacotado com o launcher, por versão e loader.

use std::path::PathBuf;

use thiserror::Error;

use super::{Instance, InstanceError};

#[derive(Debug, Error)]
pub enum CompanionError {
    #[error("não há Aurora Companion para {minecraft_version} com {loader}")]
    Unsupported {
        minecraft_version: String,
        loader: String,
    },
    #[error("falha ao instalar o Aurora Companion: {0}")]
    Instance(#[from] InstanceError),
}

pub fn install_embedded_companion(
    instance: &Instance,
    minecraft_version: &str,
    loader: &str,
) -> Result<PathBuf, CompanionError> {
    let contents = match (minecraft_version, loader) {
        ("1.12.2", "forge") => include_bytes!("../../../../../releases/companion/1.12.2/forge/aurora-companion-forge-1.12.2-0.1.0.jar").as_slice(),
        ("1.16.5", "fabric") => include_bytes!("../../../../../releases/companion/1.16.5/fabric/aurora-companion-fabric-1.16.5-0.1.0.jar").as_slice(),
        ("1.16.5", "forge") => include_bytes!("../../../../../releases/companion/1.16.5/forge/aurora-companion-forge-1.16.5-0.1.0.jar").as_slice(),
        ("1.19.2", "fabric") => include_bytes!("../../../../../releases/companion/1.19.2/fabric/aurora-companion-fabric-1.19.2-0.1.0.jar").as_slice(),
        ("1.19.2", "forge") => include_bytes!("../../../../../releases/companion/1.19.2/forge/aurora-companion-forge-1.19.2-0.1.0.jar").as_slice(),
        ("1.20.1", "fabric") => include_bytes!("../../../../../releases/companion/1.20.1/fabric/aurora-companion-fabric-1.20.1-0.1.0.jar").as_slice(),
        ("1.20.1", "forge") => include_bytes!("../../../../../releases/companion/1.20.1/forge/aurora-companion-forge-1.20.1-0.1.0.jar").as_slice(),
        ("1.21.1", "fabric") => include_bytes!("../../../../../releases/companion/1.21.1/fabric/aurora-companion-fabric-1.21.1-0.1.0.jar").as_slice(),
        ("1.21.1", "forge") => include_bytes!("../../../../../releases/companion/1.21.1/forge/aurora-companion-forge-1.21.1-0.1.0.jar").as_slice(),
        _ => {
            return Err(CompanionError::Unsupported {
                minecraft_version: minecraft_version.to_owned(),
                loader: loader.to_owned(),
            });
        }
    };
    instance
        .install_companion_bytes(
            &format!("aurora-companion-{loader}-{minecraft_version}.jar"),
            contents,
        )
        .map_err(CompanionError::from)
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use zip::ZipArchive;

    use super::*;
    use crate::engine::{InstanceId, InstanceLayout};

    #[test]
    fn all_supported_companions_are_valid_embedded_jars() {
        let test_root =
            std::env::temp_dir().join(format!("aurora-companion-test-{}", std::process::id()));
        let layout = InstanceLayout::new(&test_root);
        let combinations = [
            ("1.12.2", "forge"),
            ("1.16.5", "fabric"),
            ("1.16.5", "forge"),
            ("1.19.2", "fabric"),
            ("1.19.2", "forge"),
            ("1.20.1", "fabric"),
            ("1.20.1", "forge"),
            ("1.21.1", "fabric"),
            ("1.21.1", "forge"),
        ];

        for (minecraft, loader) in combinations {
            let id = InstanceId::parse(format!(
                "companion-{}-{loader}",
                minecraft.replace('.', "-")
            ))
            .expect("identificador de teste");
            let instance = layout.open(id);
            let path = install_embedded_companion(&instance, minecraft, loader)
                .expect("Companion deve estar embutido");
            let mut archive = ZipArchive::new(File::open(path).expect("JAR instalado"))
                .expect("Companion deve ser um ZIP/JAR válido");
            assert!(archive.len() > 3, "JAR {minecraft}/{loader} está vazio");
            assert!(
                archive.by_name("META-INF/MANIFEST.MF").is_ok(),
                "manifesto ausente em {minecraft}/{loader}"
            );
        }
        fs::remove_dir_all(test_root).expect("limpeza das instâncias de teste");
    }
}
