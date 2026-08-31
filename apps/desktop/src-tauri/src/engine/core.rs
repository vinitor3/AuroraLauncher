//! Aurora Core embedded artifacts selected from one signed compatibility matrix.

use std::collections::BTreeMap;
use std::path::PathBuf;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::{Instance, InstanceError};

const MANIFEST_JSON: &str =
    include_str!("../../../../../apps/aurora-core/compatibility-manifest.json");
const TRUSTED_KEY_ID: &str = "aurora-core-1.0.0-release";
const TRUSTED_PUBLIC_KEY: &str = "Q7cPrNqHdmbgQWVBOCVpVpvsyoaEMJNgg7hj3kcpZ9Q=";

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
    instance
        .install_aurora_artifact_bytes(
            &format!("aurora-core-{loader}-{minecraft_version}.jar"),
            contents,
        )
        .map_err(AuroraCoreError::from)
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

fn embedded_artifact(minecraft: &str, loader: &str) -> Option<&'static [u8]> {
    Some(match (minecraft, loader) {
        ("1.12.2", "forge") => include_bytes!(
            "../../../../../releases/core/1.0.0/1.12.2/forge/aurora-core-forge-1.12.2-1.0.0.jar"
        ),
        ("1.16.5", "fabric") => include_bytes!(
            "../../../../../releases/core/1.0.0/1.16.5/fabric/aurora-core-fabric-1.16.5-1.0.0.jar"
        ),
        ("1.16.5", "forge") => include_bytes!(
            "../../../../../releases/core/1.0.0/1.16.5/forge/aurora-core-forge-1.16.5-1.0.0.jar"
        ),
        ("1.19.2", "fabric") => include_bytes!(
            "../../../../../releases/core/1.0.0/1.19.2/fabric/aurora-core-fabric-1.19.2-1.0.0.jar"
        ),
        ("1.19.2", "forge") => include_bytes!(
            "../../../../../releases/core/1.0.0/1.19.2/forge/aurora-core-forge-1.19.2-1.0.0.jar"
        ),
        ("1.20.1", "fabric") => include_bytes!(
            "../../../../../releases/core/1.0.0/1.20.1/fabric/aurora-core-fabric-1.20.1-1.0.0.jar"
        ),
        ("1.20.1", "forge") => include_bytes!(
            "../../../../../releases/core/1.0.0/1.20.1/forge/aurora-core-forge-1.20.1-1.0.0.jar"
        ),
        ("1.21.1", "fabric") => include_bytes!(
            "../../../../../releases/core/1.0.0/1.21.1/fabric/aurora-core-fabric-1.21.1-1.0.0.jar"
        ),
        ("1.21.1", "forge") => include_bytes!(
            "../../../../../releases/core/1.0.0/1.21.1/forge/aurora-core-forge-1.21.1-1.0.0.jar"
        ),
        _ => return None,
    })
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
        assert_eq!(manifest.builds.len(), 9);
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

            let expected_major = match build.java_runtime {
                8 => 52,
                17 => 61,
                21 => 65,
                other => panic!("Java inesperado no manifesto: {other}"),
            };
            for index in 0..archive.len() {
                let mut entry = archive.by_index(index).expect("entrada do JAR");
                if !entry.name().ends_with(".class") {
                    continue;
                }
                let mut header = [0_u8; 8];
                entry.read_exact(&mut header).expect("cabeçalho de classe");
                assert_eq!(&header[0..4], &[0xCA, 0xFE, 0xBA, 0xBE]);
                let major = u16::from_be_bytes([header[6], header[7]]);
                assert!(
                    major <= expected_major,
                    "{} contém bytecode {major}, acima de Java {}",
                    build.aurora_core_build,
                    build.java_runtime
                );
            }
        }
        fs::remove_dir_all(test_root).expect("limpeza do teste");
    }

    #[test]
    fn rejects_tampered_artifact() {
        let manifest = aurora_core_manifest().unwrap();
        let error = verify_artifact(&manifest, &manifest.builds[0], b"tampered").unwrap_err();
        assert!(matches!(error, AuroraCoreError::Integrity));
    }
}
