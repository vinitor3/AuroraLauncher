//! Aurora Skins oficial empacotado por versão e loader.

use std::path::PathBuf;

use thiserror::Error;

use super::{Instance, InstanceError};

#[derive(Debug, Error)]
pub enum SkinsError {
    #[error("não há Aurora Skins para {minecraft_version} com {loader}")]
    Unsupported {
        minecraft_version: String,
        loader: String,
    },
    #[error("falha ao instalar o Aurora Skins: {0}")]
    Instance(#[from] InstanceError),
}

pub fn install_embedded_skins(
    instance: &Instance,
    minecraft_version: &str,
    loader: &str,
) -> Result<PathBuf, SkinsError> {
    let contents = match (minecraft_version, loader) {
        ("1.12.2", "forge") => include_bytes!("../../../../../releases/skins/0.2.3/1.12.2/forge/aurora-skins-forge-1.12.2-0.2.3.jar").as_slice(),
        ("1.16.5", "fabric") => include_bytes!("../../../../../releases/skins/0.2.3/1.16.5/fabric/aurora-skins-fabric-1.16.5-0.2.3.jar").as_slice(),
        ("1.16.5", "forge") => include_bytes!("../../../../../releases/skins/0.2.3/1.16.5/forge/aurora-skins-forge-1.16.5-0.2.3.jar").as_slice(),
        ("1.19.2", "fabric") => include_bytes!("../../../../../releases/skins/0.2.3/1.19.2/fabric/aurora-skins-fabric-1.19.2-0.2.3.jar").as_slice(),
        ("1.19.2", "forge") => include_bytes!("../../../../../releases/skins/0.2.3/1.19.2/forge/aurora-skins-forge-1.19.2-0.2.3.jar").as_slice(),
        ("1.20.1", "fabric") => include_bytes!("../../../../../releases/skins/0.2.3/1.20.1/fabric/aurora-skins-fabric-1.20.1-0.2.3.jar").as_slice(),
        ("1.20.1", "forge") => include_bytes!("../../../../../releases/skins/0.2.3/1.20.1/forge/aurora-skins-forge-1.20.1-0.2.3.jar").as_slice(),
        ("1.21.1", "fabric") => include_bytes!("../../../../../releases/skins/0.2.3/1.21.1/fabric/aurora-skins-fabric-1.21.1-0.2.3.jar").as_slice(),
        ("1.21.1", "forge") => include_bytes!("../../../../../releases/skins/0.2.3/1.21.1/forge/aurora-skins-forge-1.21.1-0.2.3.jar").as_slice(),
        _ => {
            return Err(SkinsError::Unsupported {
                minecraft_version: minecraft_version.to_owned(),
                loader: loader.to_owned(),
            });
        }
    };
    instance
        .install_aurora_artifact_bytes(
            &format!("aurora-skins-{loader}-{minecraft_version}.jar"),
            contents,
        )
        .map_err(SkinsError::from)
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::io::{Cursor, Read, Seek};

    use zip::ZipArchive;

    use super::*;
    use crate::engine::{InstanceId, InstanceLayout};

    fn inspect_skin_payload<R: Read + Seek>(archive: &mut ZipArchive<R>, major: u16) -> bool {
        let mut has_player_search = archive
            .by_name("com/aurora/skins/catalog/SearchDebouncer.class")
            .is_ok()
            && archive
                .by_name("com/aurora/skins/api/PlayerSuggestion.class")
                .is_ok();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).expect("entrada");
            assert!(!entry.name().starts_with("org/slf4j/"));
            assert!(!entry.name().starts_with("net/minecraft/"));
            if entry.name().ends_with(".class") {
                let mut header = [0; 8];
                entry.read_exact(&mut header).expect("header class");
                assert!(u16::from_be_bytes([header[6], header[7]]) <= major);
            } else if entry.name().ends_with(".jar") {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).expect("JAR aninhado");
                let mut nested = ZipArchive::new(Cursor::new(bytes)).expect("Jar-in-Jar válido");
                has_player_search |= inspect_skin_payload(&mut nested, major);
            }
        }
        has_player_search
    }

    #[test]
    fn all_supported_skin_modules_are_embedded_and_depend_on_core() {
        let root = std::env::temp_dir().join(format!("aurora-skins-test-{}", std::process::id()));
        let layout = InstanceLayout::new(&root);
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
            let id = InstanceId::parse(format!("skins-{}-{loader}", minecraft.replace('.', "-")))
                .expect("id de teste");
            let path = install_embedded_skins(&layout.open(id), minecraft, loader)
                .expect("Aurora Skins embutido");
            let mut archive =
                ZipArchive::new(File::open(path).expect("JAR instalado")).expect("JAR válido");
            let metadata_name = if loader == "fabric" {
                "fabric.mod.json"
            } else if minecraft == "1.12.2" {
                "mcmod.info"
            } else {
                "META-INF/mods.toml"
            };
            let mut metadata = String::new();
            archive
                .by_name(metadata_name)
                .expect("metadado")
                .read_to_string(&mut metadata)
                .expect("metadado textual");
            assert!(metadata.contains("aurora_skins"));
            assert!(metadata.contains("aurora_core"));
            assert!(metadata.contains("0.2.3"));
            assert!(archive
                .by_name("META-INF/services/com.aurora.core.api.module.AuroraModule")
                .is_ok());
            let major = match minecraft {
                "1.12.2" | "1.16.5" => 52,
                "1.21.1" => 65,
                _ => 61,
            };
            assert!(
                inspect_skin_payload(&mut archive, major),
                "{minecraft}/{loader}"
            );
        }
        fs::remove_dir_all(root).expect("limpeza");
    }
}
