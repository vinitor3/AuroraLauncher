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
    let contents = embedded_skins_artifact(minecraft_version, loader)?;
    instance
        .install_aurora_artifact_bytes(
            &format!("aurora-skins-{loader}-{minecraft_version}.jar"),
            contents,
        )
        .map_err(SkinsError::from)
}

/// Read-only selection of the same immutable bytes used by normal installation.
pub(super) fn embedded_skins_artifact(
    minecraft_version: &str,
    loader: &str,
) -> Result<&'static [u8], SkinsError> {
    super::embedded_modules::skins(minecraft_version, loader).ok_or_else(|| {
        SkinsError::Unsupported {
            minecraft_version: minecraft_version.to_owned(),
            loader: loader.to_owned(),
        }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::io::{Cursor, Read};

    use serde_json::Value;
    use zip::ZipArchive;

    use super::*;
    use crate::engine::{InstanceId, InstanceLayout};

    const MIXINS: &str = "aurora_skins.mixins.json";
    const SERVICE: &str = "META-INF/services/com.aurora.core.api.module.AuroraModule";
    type Files = BTreeMap<String, Vec<u8>>;
    type Contract<T> = Result<T, String>;

    fn json(bytes: &[u8]) -> Contract<Value> {
        serde_json::from_slice(bytes).map_err(|e| format!("JSON inválido: {e}"))
    }

    fn resource<'a>(jars: &'a [Files], name: &str) -> Contract<&'a [u8]> {
        jars.iter()
            .find_map(|jar| jar.get(name).map(Vec::as_slice))
            .ok_or_else(|| format!("recurso ausente: {name}"))
    }

    fn class(jars: &[Files], name: &str) -> Contract<()> {
        resource(jars, &format!("{}.class", name.replace('.', "/"))).map(|_| ())
    }

    // The active product contract is cumulative and independent of the version
    // string. Historical fixtures still use validate() for their original shape.
    fn validate_active_features(jars: &[Files], minecraft: &str) -> Contract<()> {
        let contract = json(include_bytes!(
            "../../../../../scripts/skins-feature-contract.json"
        ))?;
        if contract["schemaVersion"] != 1 {
            return Err("versão inválida do contrato de funcionalidades".into());
        }
        let requirements = contract["requirements"]
            .as_array()
            .filter(|entries| !entries.is_empty())
            .ok_or("contrato de funcionalidades vazio")?;
        let scope = if minecraft == "1.12.2" {
            "legacy"
        } else {
            "modern"
        };
        for requirement in requirements {
            let required_scope = requirement["scope"].as_str().ok_or("escopo ausente")?;
            if !["all", "legacy", "modern"].contains(&required_scope) {
                return Err(format!("escopo inválido: {required_scope}"));
            }
            if required_scope != "all" && required_scope != scope {
                continue;
            }
            let name = requirement["resource"].as_str().ok_or("recurso ausente")?;
            let bytes = resource(jars, name)?;
            // Stable Aurora symbols in the JVM constant pool show that screen,
            // preview, cache and networking implementations retain their links.
            for marker in strings(&requirement["markers"])? {
                if marker.is_empty()
                    || !bytes
                        .windows(marker.len())
                        .any(|part| part == marker.as_bytes())
                {
                    return Err(format!(
                        "ligação de funcionalidade ausente: {name}: {marker}"
                    ));
                }
            }
        }
        Ok(())
    }

    fn strings(value: &Value) -> Contract<Vec<&str>> {
        value
            .as_array()
            .ok_or("array ausente/inválido")?
            .iter()
            .map(|item| item.as_str().ok_or_else(|| "classe inválida".into()))
            .collect()
    }

    // Only nested JARs declared to this loader may satisfy class/resource references.
    fn payload(bytes: &[u8], loader: &str, major: u16) -> Contract<Vec<Files>> {
        let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        // Only these three explicit networking dependencies may contribute Fabric classes.
        // They remain loader-declared nested JARs, never shaded into Aurora's namespace.
        let fabric_api = if loader == "fabric" {
            let mut metadata = Vec::new();
            if let Ok(mut entry) = zip.by_name("fabric.mod.json") {
                entry
                    .read_to_end(&mut metadata)
                    .map_err(|e| e.to_string())?;
            }
            !metadata.is_empty()
                && matches!(
                    json(&metadata)?["id"].as_str(),
                    Some(
                        "fabric-api-base"
                            | "fabric-networking-api-v1"
                            | "fabric-lifecycle-events-v1"
                    )
                )
        } else {
            false
        };
        let mut files = Files::new();
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            if name.ends_with(".json") || name.ends_with(".mcmeta") || name == "mcmod.info" {
                json(&bytes).map_err(|e| format!("{name}: {e}"))?;
            }
            if name.ends_with(".class") {
                // Skins' own code and Architectury's generated bridge only:
                // never shadow Core, Minecraft, loaders, Mixin, SLF4J, Gson, etc.
                if !name.starts_with("com/aurora/skins/")
                    && !(fabric_api && name.starts_with("net/fabricmc/fabric/"))
                    && !(name.starts_with("architectury_inject_")
                        && name.ends_with("/PlatformMethods.class"))
                {
                    return Err(format!("biblioteca sombreada proibida: {name}"));
                }
                if bytes.len() < 8
                    || bytes[..4] != [0xca, 0xfe, 0xba, 0xbe]
                    || !(45..=major).contains(&u16::from_be_bytes([bytes[6], bytes[7]]))
                {
                    return Err(format!("bytecode inválido/incompatível: {name}"));
                }
            }
            if files.insert(name.clone(), bytes).is_some() {
                return Err(format!("entrada duplicada: {name}"));
            }
        }
        let (metadata, key) = if loader == "fabric" {
            ("fabric.mod.json", "file")
        } else {
            ("META-INF/jarjar/metadata.json", "path")
        };
        let mut declared = BTreeSet::new();
        if let Some(bytes) = files.get(metadata) {
            if let Some(jars) = json(bytes)?.get("jars") {
                for jar in jars.as_array().ok_or("nested jars inválidos")? {
                    declared.insert(
                        jar[key]
                            .as_str()
                            .ok_or("caminho nested inválido")?
                            .to_owned(),
                    );
                }
            }
        }
        let physical: BTreeSet<_> = files
            .keys()
            .filter(|name| name.ends_with(".jar"))
            .cloned()
            .collect();
        if declared != physical {
            return Err("nested declarados diferem dos empacotados".into());
        }
        let mut jars = Vec::new();
        for name in declared {
            let child =
                payload(&files[&name], loader, major).map_err(|e| format!("{name}!/{e}"))?;
            if loader == "fabric" {
                resource(&child[..1], "fabric.mod.json")?;
            }
            jars.extend(child);
        }
        jars.insert(0, files);
        Ok(jars)
    }

    fn validate(jars: &[Files], minecraft: &str, loader: &str) -> Contract<()> {
        let legacy = minecraft == "1.12.2";
        let metadata_name = if loader == "fabric" {
            "fabric.mod.json"
        } else if legacy {
            "mcmod.info"
        } else {
            "META-INF/mods.toml"
        };
        let metadata = resource(&jars[..1], metadata_name)?;
        let metadata = std::str::from_utf8(metadata).map_err(|e| e.to_string())?;
        if !metadata.contains("aurora_skins") || !metadata.contains("aurora_core") {
            return Err("identidade/dependência Aurora Core ausente".into());
        }
        let provider = if legacy {
            "com.aurora.skins.legacy.AuroraSkinsLegacyModule"
        } else {
            "com.aurora.skins.minecraft.AuroraSkinsModule"
        };
        let service =
            std::str::from_utf8(resource(&jars[..1], SERVICE)?).map_err(|e| e.to_string())?;
        if !service
            .lines()
            .any(|line| line.split('#').next().unwrap_or_default().trim() == provider)
        {
            return Err("provider AuroraModule não preservado".into());
        }
        for name in [
            provider,
            "com.aurora.skins.catalog.SearchDebouncer",
            "com.aurora.skins.api.PlayerSuggestion",
        ] {
            class(jars, name)?;
        }
        if loader == "forge" {
            let entrypoint = if legacy {
                "com/aurora/skins/legacy/AuroraSkinsForgeLegacy.class"
            } else {
                "com/aurora/skins/minecraft/forge/AuroraSkinsForge.class"
            };
            let bytes = resource(jars, entrypoint)?;
            // Presence markers only; Forge annotation discovery still needs runtime QA.
            for marker in ["Lnet/minecraftforge/fml/common/Mod;", "aurora_skins"] {
                if !bytes
                    .windows(marker.len())
                    .any(|part| part == marker.as_bytes())
                {
                    return Err(format!("marcador @Mod ausente: {entrypoint}: {marker}"));
                }
            }
        }
        let mut configs = BTreeSet::new();
        for jar in jars {
            let manifest = resource(std::slice::from_ref(jar), "META-INF/MANIFEST.MF")?;
            let manifest = std::str::from_utf8(manifest)
                .map_err(|e| e.to_string())?
                .replace("\r\n", "\n")
                .replace("\n ", "");
            for line in manifest.lines().take_while(|line| !line.is_empty()) {
                if let Some((key, value)) = line.split_once(':') {
                    if key.eq_ignore_ascii_case("MixinConfigs") {
                        configs.extend(value.split(',').map(|name| name.trim().to_owned()));
                    }
                }
            }
            if let Some(bytes) = jar.get("fabric.mod.json") {
                let metadata = json(bytes)?;
                if let Some(mixins) = metadata.get("mixins") {
                    for entry in mixins.as_array().ok_or("mixins inválidos")? {
                        configs.insert(
                            entry
                                .as_str()
                                .or_else(|| entry["config"].as_str())
                                .ok_or("config mixin inválida")?
                                .to_owned(),
                        );
                    }
                }
                if metadata["id"] == "aurora_skins"
                    && metadata["entrypoints"]["client"]
                        .as_array()
                        .is_none_or(Vec::is_empty)
                {
                    return Err("entrypoint client ausente".into());
                }
                if let Some(entrypoints) = metadata.get("entrypoints") {
                    for entries in entrypoints
                        .as_object()
                        .ok_or("entrypoints inválidos")?
                        .values()
                    {
                        for entry in entries.as_array().ok_or("entrypoint inválido")? {
                            let value = entry
                                .as_str()
                                .or_else(|| entry["value"].as_str())
                                .ok_or("entrypoint vazio")?;
                            class(jars, value.split("::").next().ok_or("entrypoint vazio")?)?;
                            if let Some(adapter) =
                                entry["adapter"].as_str().filter(|name| *name != "default")
                            {
                                class(
                                    jars,
                                    metadata["languageAdapters"][adapter]
                                        .as_str()
                                        .ok_or("adapter ausente")?,
                                )?;
                            }
                        }
                    }
                }
            }
            for (name, bytes) in jar
                .iter()
                .filter(|(name, _)| name.starts_with("META-INF/services/"))
            {
                let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
                for provider in text
                    .lines()
                    .map(|line| line.split('#').next().unwrap_or_default().trim())
                    .filter(|line| !line.is_empty())
                {
                    class(jars, provider).map_err(|e| format!("{name}: {e}"))?;
                }
            }
        }
        if !legacy && !configs.contains(MIXINS) {
            return Err("declaração mixin ausente".into());
        }
        for name in configs {
            let config = json(resource(jars, &name)?).map_err(|e| format!("{name}: {e}"))?;
            let package = config["package"].as_str().ok_or("package mixin ausente")?;
            let mut count = 0;
            for side in ["mixins", "client", "server"] {
                if let Some(entries) = config.get(side) {
                    for entry in strings(entries)? {
                        class(jars, &format!("{package}.{entry}"))?;
                        count += 1;
                    }
                }
            }
            if count == 0 {
                return Err(format!("config mixin vazia: {name}"));
            }
            if let Some(plugin) = config.get("plugin") {
                class(jars, plugin.as_str().ok_or("plugin inválido")?)?;
            }
            if let Some(refmap) = config.get("refmap") {
                // Its Minecraft targets belong to the game, not to the Skins JAR.
                json(resource(jars, refmap.as_str().ok_or("refmap inválido")?)?)?;
            }
        }
        Ok(())
    }

    #[test]
    fn all_supported_skin_modules_satisfy_packaging_contracts() {
        let root = std::env::temp_dir().join(format!("aurora-skins-test-{}", uuid::Uuid::new_v4()));
        let layout = InstanceLayout::new(&root);
        let mut failures = Vec::new();
        let manifest = crate::engine::aurora_core_manifest().unwrap();
        for target in manifest.builds {
            let minecraft = &target.minecraft;
            let loader = &target.loader;
            let id = InstanceId::parse(format!("skins-{}-{loader}", minecraft.replace('.', "-")))
                .unwrap();
            let path = install_embedded_skins(&layout.open(id), minecraft, loader).unwrap();
            let class_major = (44 + target.java_runtime) as u16;
            let result = payload(&fs::read(path).unwrap(), loader, class_major).and_then(|jars| {
                validate(&jars, minecraft, loader)?;
                validate_active_features(&jars, minecraft)
            });
            if let Err(error) = result {
                failures.push(format!("{minecraft}/{loader}: {error}"));
            }
        }
        fs::remove_dir_all(root).unwrap();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn active_features_reject_each_incomplete_historical_line() {
        let grid_only = payload(
            include_bytes!("../../../../../releases/skins/0.2.5/1.20.1/forge/aurora-skins-forge-1.20.1-0.2.5.jar"),
            "forge", 61,
        ).unwrap();
        assert!(validate_active_features(&grid_only, "1.20.1")
            .unwrap_err()
            .contains("SkinSyncProtocol.class"));
        let multiplayer_only = payload(
            include_bytes!("../../../../../releases/skins/0.3.0/1.20.1/forge/aurora-skins-forge-1.20.1-0.3.0.jar"),
            "forge", 61,
        ).unwrap();
        assert!(validate_active_features(&multiplayer_only, "1.20.1")
            .unwrap_err()
            .contains("CatalogGridState.class"));
    }

    #[test]
    fn active_features_reject_unused_grid_classes_and_removed_preview_cache() {
        let mut jars = payload(
            include_bytes!("../../../../../releases/skins/0.3.1/1.20.1/forge/aurora-skins-forge-1.20.1-0.3.1.jar"),
            "forge", 61,
        ).unwrap();
        validate_active_features(&jars, "1.20.1").unwrap();
        // Keeping GridState/Layout in the JAR must not allow a reverted screen,
        // renderer or provider to pass. Mutations exist only in memory.
        for (name, marker) in [
            (
                "com/aurora/skins/minecraft/ui/AuroraSkinsScreen.class",
                "renderCard",
            ),
            (
                "com/aurora/skins/catalog/AuroraCatalogProvider.class",
                "previewCache",
            ),
            ("aurora_skins.mixins.json", "PlayerInfoMixin"),
        ] {
            let original = jars[0][name].clone();
            let bytes = jars[0].get_mut(name).unwrap();
            let mut matches = 0;
            for offset in 0..=bytes.len() - marker.len() {
                if &bytes[offset..offset + marker.len()] == marker.as_bytes() {
                    bytes[offset..offset + marker.len()].fill(b'_');
                    matches += 1;
                }
            }
            assert!(matches > 0, "fixture marker missing: {marker}");
            let error = validate_active_features(&jars, "1.20.1").unwrap_err();
            assert!(error.contains(name) && error.contains(marker), "{error}");
            jars[0].insert(name.to_owned(), original);
        }
    }

    #[test]
    fn historical_0_2_3_detects_missing_mixin_resource_and_broken_references() {
        // Immutable fixtures; repairs/mutations exist only in memory.
        let fixtures: [(&str, &[u8]); 2] = [
            ("fabric", include_bytes!("../../../../../releases/skins/0.2.3/1.19.2/fabric/aurora-skins-fabric-1.19.2-0.2.3.jar")),
            ("forge", include_bytes!("../../../../../releases/skins/0.2.3/1.19.2/forge/aurora-skins-forge-1.19.2-0.2.3.jar")),
        ];
        for (loader, bytes) in fixtures {
            let mut jars = payload(bytes, loader, 61).unwrap();
            assert_eq!(
                validate(&jars, "1.19.2", loader).unwrap_err(),
                format!("recurso ausente: {MIXINS}")
            );
            jars[0].insert(MIXINS.into(), b"{".to_vec());
            assert!(validate(&jars, "1.19.2", loader)
                .unwrap_err()
                .contains("JSON inválido"));
            jars[0].insert(
                MIXINS.into(),
                serde_json::to_vec(&serde_json::json!({
                    "package": "com.aurora.skins.minecraft.mixin",
                    "client": ["AbstractClientPlayerMixin", "TitleScreenMixin"],
                    "refmap": "aurora-skins-common-refmap.json"
                }))
                .unwrap(),
            );
            validate(&jars, "1.19.2", loader).unwrap();
            let entrypoint = if loader == "fabric" {
                "com/aurora/skins/minecraft/fabric/AuroraSkinsFabric.class"
            } else {
                "com/aurora/skins/minecraft/forge/AuroraSkinsForge.class"
            };
            for name in [
                "com/aurora/skins/minecraft/mixin/TitleScreenMixin.class",
                "aurora-skins-common-refmap.json",
                SERVICE,
                "com/aurora/skins/minecraft/AuroraSkinsModule.class",
                entrypoint,
            ] {
                let bytes = jars[0].remove(name).unwrap();
                assert!(
                    validate(&jars, "1.19.2", loader)
                        .unwrap_err()
                        .contains(name),
                    "{loader}: {name}"
                );
                jars[0].insert(name.to_owned(), bytes);
            }
        }
    }
}
