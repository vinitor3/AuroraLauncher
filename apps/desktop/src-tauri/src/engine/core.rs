//! Aurora Core embedded artifacts selected from one signed compatibility matrix.

use std::collections::BTreeMap;
use std::path::PathBuf;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::{Instance, InstanceError};

use super::embedded_modules::{
    core as embedded_artifact, CORE_KEY_ID as TRUSTED_KEY_ID, CORE_MANIFEST as MANIFEST_JSON,
    CORE_PUBLIC_KEY as TRUSTED_PUBLIC_KEY,
};

pub fn has_embedded_modules(minecraft: &str, loader: &str) -> bool {
    embedded_artifact(minecraft, loader).is_some()
}

#[derive(Debug, Error)]
pub enum AuroraCoreError {
    #[error("não há Aurora Core para Minecraft {minecraft_version} com {loader}")]
    Unsupported {
        minecraft_version: String,
        loader: String,
    },
    #[error("o manifesto de compatibilidade do Aurora Core é inválido: {0}")]
    Manifest(String),
    #[error("o Aurora Core embutido não passou na verificação de integridade")]
    Integrity,
    #[error("a assinatura do Aurora Core não é confiável")]
    Signature,
    #[error("falha ao instalar o Aurora Core: {0}")]
    Instance(#[from] InstanceError),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuroraCoreManifest {
    pub schema_version: u32,
    pub generated_at: String,
    pub signature_public_keys: BTreeMap<String, String>,
    pub builds: Vec<AuroraCoreBuild>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuroraCoreBuild {
    pub minecraft: String,
    pub loader: String,
    pub loader_version: String,
    pub loader_version_range: String,
    pub java_runtime: u32,
    pub build_jdk: u32,
    pub aurora_core_version: String,
    pub aurora_core_build: String,
    pub download: String,
    pub size: usize,
    pub sha256: String,
    pub signature: ArtifactSignature,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactSignature {
    pub algorithm: String,
    pub key_id: String,
    pub value: String,
}

pub fn aurora_core_manifest() -> Result<AuroraCoreManifest, AuroraCoreError> {
    let manifest: AuroraCoreManifest = serde_json::from_str(MANIFEST_JSON)
        .map_err(|error| AuroraCoreError::Manifest(error.to_string()))?;
    if manifest.schema_version != 1 || manifest.builds.is_empty() {
        return Err(AuroraCoreError::Manifest(
            "schema ou matriz ausente".to_owned(),
        ));
    }
    Ok(manifest)
}

pub fn install_embedded_core(
    instance: &Instance,
    minecraft_version: &str,
    loader: &str,
) -> Result<PathBuf, AuroraCoreError> {
    let contents = verified_embedded_core(minecraft_version, loader)?;
    instance
        .install_aurora_artifact_bytes(
            &format!("aurora-core-{loader}-{minecraft_version}.jar"),
            contents,
        )
        .map_err(AuroraCoreError::from)
}

/// Existing trusted selection and signature validation without installation.
pub(super) fn verified_embedded_core(
    minecraft_version: &str,
    loader: &str,
) -> Result<&'static [u8], AuroraCoreError> {
    let manifest = aurora_core_manifest()?;
    let build = manifest
        .builds
        .iter()
        .find(|build| build.minecraft == minecraft_version && build.loader == loader)
        .ok_or_else(|| AuroraCoreError::Unsupported {
            minecraft_version: minecraft_version.to_owned(),
            loader: loader.to_owned(),
        })?;
    let contents = embedded_artifact(minecraft_version, loader).ok_or_else(|| {
        AuroraCoreError::Unsupported {
            minecraft_version: minecraft_version.to_owned(),
            loader: loader.to_owned(),
        }
    })?;
    verify_artifact(&manifest, build, contents)?;
    Ok(contents)
}

fn verify_artifact(
    manifest: &AuroraCoreManifest,
    build: &AuroraCoreBuild,
    contents: &[u8],
) -> Result<(), AuroraCoreError> {
    if contents.len() != build.size {
        return Err(AuroraCoreError::Integrity);
    }
    let digest = Sha256::digest(contents);
    let actual = format!("{digest:x}");
    if actual != build.sha256 {
        return Err(AuroraCoreError::Integrity);
    }
    if build.signature.algorithm != "Ed25519" {
        return Err(AuroraCoreError::Signature);
    }
    if build.signature.key_id != TRUSTED_KEY_ID
        || manifest
            .signature_public_keys
            .get(TRUSTED_KEY_ID)
            .map(String::as_str)
            != Some(TRUSTED_PUBLIC_KEY)
    {
        return Err(AuroraCoreError::Signature);
    }
    let key_bytes: [u8; 32] = STANDARD
        .decode(TRUSTED_PUBLIC_KEY)
        .map_err(|_| AuroraCoreError::Signature)?
        .try_into()
        .map_err(|_| AuroraCoreError::Signature)?;
    let signature_bytes: [u8; 64] = STANDARD
        .decode(&build.signature.value)
        .map_err(|_| AuroraCoreError::Signature)?
        .try_into()
        .map_err(|_| AuroraCoreError::Signature)?;
    let verifying_key =
        VerifyingKey::from_bytes(&key_bytes).map_err(|_| AuroraCoreError::Signature)?;
    let signature = Signature::from_bytes(&signature_bytes);
    let signed_message = format!("aurora-core:v1:{}", build.sha256);
    verifying_key
        .verify_strict(signed_message.as_bytes(), &signature)
        .map_err(|_| AuroraCoreError::Signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{InstanceId, InstanceLayout};
    use std::fs::{self, File};
    use std::io::Read;
    use zip::ZipArchive;

    #[test]
    fn verifies_and_installs_every_signed_core_build() {
        let test_root =
            std::env::temp_dir().join(format!("aurora-core-test-{}", std::process::id()));
        let layout = InstanceLayout::new(&test_root);
        let manifest = aurora_core_manifest().expect("manifesto válido");
        let matrix: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../scripts/active-mod-matrix.json"
        ))
        .unwrap();
        assert_eq!(
            manifest.builds.len(),
            matrix["targets"].as_array().unwrap().len()
        );
        assert!(!has_embedded_modules("other", "fabric"));
        for build in manifest.builds {
            let id = InstanceId::parse(format!(
                "core-{}-{}",
                build.minecraft.replace('.', "-"),
                build.loader
            ))
            .expect("id de teste");
            let instance = layout.open(id);
            let installed = install_embedded_core(&instance, &build.minecraft, &build.loader)
                .expect("Core assinado deve instalar");
            let mut archive = ZipArchive::new(File::open(installed).expect("JAR instalado"))
                .expect("Core deve ser um JAR válido");
            assert!(archive.len() > 5);
            let metadata = if build.loader == "fabric" {
                "fabric.mod.json"
            } else if build.minecraft == "1.12.2" {
                "mcmod.info"
            } else {
                "META-INF/mods.toml"
            };
            assert!(
                archive.by_name(metadata).is_ok(),
                "metadado ausente: {metadata}"
            );

            let expected_major = 44 + build.java_runtime;
            for index in 0..archive.len() {
                let mut entry = archive.by_index(index).expect("entrada do JAR");
                let entry_name = entry.name().replace('\\', "/");
                assert!(
                    !entry_name.starts_with("org/slf4j/"),
                    "{} contém SLF4J sem relocação: {entry_name}",
                    build.aurora_core_build
                );
                assert!(
                    !entry_name.starts_with("net/minecraft/"),
                    "{} empacota classes do Minecraft: {entry_name}",
                    build.aurora_core_build
                );
                if !entry_name.ends_with(".class") {
                    continue;
                }
                let mut header = [0_u8; 8];
                entry.read_exact(&mut header).expect("cabeçalho de classe");
                assert_eq!(&header[0..4], &[0xCA, 0xFE, 0xBA, 0xBE]);
                let major = u16::from_be_bytes([header[6], header[7]]);
                assert!(
                    u32::from(major) <= expected_major,
                    "{} contém bytecode {major}, acima de Java {}",
                    build.aurora_core_build,
                    build.java_runtime
                );
            }
        }
        fs::remove_dir_all(test_root).expect("limpeza do teste");
    }

    #[test]
    fn legacy_mcmod_version_matches_selected_manifest_build() {
        let manifest = aurora_core_manifest().expect("manifesto válido");
        let build = manifest
            .builds
            .iter()
            .find(|build| build.minecraft == "1.12.2" && build.loader == "forge")
            .expect("Core legado deve constar no manifesto");
        let bytes =
            embedded_artifact(&build.minecraft, &build.loader).expect("artefato legado embutido");
        let mut archive = ZipArchive::new(std::io::Cursor::new(bytes)).expect("JAR válido");
        let metadata: serde_json::Value =
            serde_json::from_reader(archive.by_name("mcmod.info").expect("mcmod.info presente"))
                .expect("mcmod.info deve ser JSON válido");
        let core = metadata
            .as_array()
            .expect("mcmod.info deve conter uma lista de mods")
            .iter()
            .find(|entry| entry["modid"] == "aurora_core")
            .expect("metadados do aurora_core presentes");
        assert_eq!(
            core["version"].as_str(),
            Some(build.aurora_core_version.as_str()),
            "mcmod.info diverge da versão selecionada no manifesto: {}",
            build.aurora_core_build
        );
    }

    #[test]
    fn rejects_tampered_artifact() {
        let manifest = aurora_core_manifest().unwrap();
        let error = verify_artifact(&manifest, &manifest.builds[0], b"tampered").unwrap_err();
        assert!(matches!(error, AuroraCoreError::Integrity));
    }
}
