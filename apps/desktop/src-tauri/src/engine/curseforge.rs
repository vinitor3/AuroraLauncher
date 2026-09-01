//! Instalação de modpacks CurseForge a partir de manifestos resolvidos pelo Worker Aurora.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use reqwest::blocking::Client;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use zip::ZipArchive;

use super::download::{download_many, download_one_with_progress, DownloadRequest, ExpectedHash};
use super::{
    InstallSummary, Instance, MinecraftInstaller, ModpackError, ModpackInstallSummary,
    ModpackProgress, TransferProgress, DEFAULT_DOWNLOAD_CONCURRENCY,
};

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_MANIFEST_FILES: usize = 2048;
const MAX_OVERRIDE_FILES: usize = 10_000;
const MAX_OVERRIDE_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_OVERRIDE_TOTAL_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeManifestFile {
    pub project_id: u64,
    pub file_id: u64,
    pub required: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeManifestSummary {
    pub name: String,
    pub version_name: String,
    pub minecraft_version: String,
    pub loader: String,
    pub loader_version: String,
    pub files: Vec<CurseForgeManifestFile>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeResolvedFile {
    pub project_id: u64,
    pub file_id: u64,
    pub file_name: String,
    pub download_url: Option<String>,
    pub sha1: String,
    pub file_size: Option<u64>,
}

pub fn curseforge_archive_path(instance: &Instance, project_id: u64, file_id: u64) -> PathBuf {
    instance
        .root()
        .join(".aurora")
        .join("curseforge")
        .join(format!("{project_id}-{file_id}.zip"))
}

pub fn read_cached_curseforge_manifest(
    instance: &Instance,
    project_id: u64,
    file_id: u64,
) -> Result<CurseForgeManifestSummary, ModpackError> {
    read_manifest(&curseforge_archive_path(instance, project_id, file_id))
}

#[allow(clippy::too_many_arguments)]
pub fn prepare_curseforge_modpack_with_progress<F>(
    instance: &Instance,
    project_id: u64,
    file_id: u64,
    download_url: Option<&str>,
    file_name: &str,
    expected_sha1: &str,
    mut progress: F,
) -> Result<CurseForgeManifestSummary, ModpackError>
where
    F: FnMut(TransferProgress),
{
    validate_pack_filename(file_name)?;
    validate_sha1(expected_sha1)?;
    instance
        .ensure_layout()
        .map_err(|error| ModpackError::Io(io::Error::other(error)))?;
    let archive_path = curseforge_archive_path(instance, project_id, file_id);
    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(download_url) = download_url {
        let parsed = Url::parse(download_url).map_err(|_| ModpackError::MissingDownload)?;
        if parsed.scheme() != "https" {
            return Err(ModpackError::MissingDownload);
        }
        download_one_with_progress(
            &client(),
            DownloadRequest {
                url: parsed.to_string(),
                destination: archive_path.clone(),
                label: file_name.to_owned(),
                expected_hash: Some(ExpectedHash::Sha1(expected_sha1.to_owned())),
                expected_size: None,
            },
            &mut progress,
        )?;
    } else {
        verify_sha1(&archive_path, expected_sha1)?;
    }
    read_manifest(&archive_path)
}

#[allow(clippy::too_many_arguments)]
pub fn install_curseforge_modpack_with_progress<F>(
    instance: &Instance,
    project_id: u64,
    file_id: u64,
    resolved_files: &[CurseForgeResolvedFile],
    forge_java: Option<&Path>,
    icon_url: Option<&str>,
    mut progress: F,
) -> Result<ModpackInstallSummary, ModpackError>
where
    F: FnMut(ModpackProgress),
{
    let archive_path = curseforge_archive_path(instance, project_id, file_id);
    let manifest = read_manifest(&archive_path)?;
    let required = manifest
        .files
        .iter()
        .filter(|file| file.required)
        .map(|file| (file.project_id, file.file_id))
        .collect::<HashSet<_>>();
    let mut resolved = HashMap::new();
    for file in resolved_files {
        let key = (file.project_id, file.file_id);
        if !required.contains(&key) || resolved.insert(key, file).is_some() {
            return Err(ModpackError::InvalidResponse);
        }
    }
    if resolved.len() != required.len() {
        return Err(ModpackError::InvalidResponse);
    }

    progress(ModpackProgress::stage(
        format!(
            "Preparando Minecraft {} com {}",
            manifest.minecraft_version, manifest.loader
        ),
        3.0,
    ));
    let minecraft = install_loader(instance, &manifest, forge_java, &mut progress)?;

    let mut requests = Vec::new();
    let mut browser_files = 0usize;
    for key in &required {
        let file = resolved.get(key).ok_or(ModpackError::InvalidResponse)?;
        validate_mod_filename(&file.file_name)?;
        validate_sha1(&file.sha1)?;
        let destination = instance.root().join("mods").join(&file.file_name);
        if let Some(download_url) = file.download_url.as_deref() {
            let parsed = Url::parse(download_url).map_err(|_| ModpackError::MissingDownload)?;
            if parsed.scheme() != "https" {
                return Err(ModpackError::MissingDownload);
            }
            requests.push(DownloadRequest {
                url: parsed.to_string(),
                destination,
                label: file.file_name.clone(),
                expected_hash: Some(ExpectedHash::Sha1(file.sha1.clone())),
                expected_size: file.file_size,
            });
        } else {
            verify_sha1(&destination, &file.sha1)?;
            browser_files += 1;
        }
    }

    let batch = download_many(&client(), requests, DEFAULT_DOWNLOAD_CONCURRENCY, |item| {
        progress(ModpackProgress::transfer(item.map_total(35.0, 94.0)))
    })?;
    progress(ModpackProgress::stage(
        "Aplicando configurações do modpack",
        96.0,
    ));
    let override_files = extract_overrides(&archive_path, instance.root())?;
    let marker = serde_json::json!({
        "source": "curseforge",
        "projectId": project_id,
        "fileId": file_id,
        "name": manifest.name,
        "versionName": manifest.version_name,
        "minecraft": manifest.minecraft_version,
        "loader": manifest.loader,
        "versionId": minecraft.version_id,
        "iconUrl": icon_url.filter(|url| url.starts_with("https://")),
    });
    fs::write(
        instance.root().join("aurora-modpack.json"),
        serde_json::to_vec_pretty(&marker).map_err(|_| ModpackError::InvalidResponse)?,
    )?;
    let summary = ModpackInstallSummary {
        project_id: project_id.to_string(),
        name: manifest.name,
        version_name: manifest.version_name,
        minecraft_version: manifest.minecraft_version,
        loader: manifest.loader,
        downloaded_files: batch.downloaded_files + batch.cached_files + browser_files,
        override_files,
        minecraft,
    };
    let _ = fs::remove_file(archive_path);
    progress(ModpackProgress::stage("Instalação concluída", 100.0));
    Ok(summary)
}

fn install_loader<F>(
    instance: &Instance,
    manifest: &CurseForgeManifestSummary,
    forge_java: Option<&Path>,
    progress: &mut F,
) -> Result<InstallSummary, ModpackError>
where
    F: FnMut(ModpackProgress),
{
    let mut loader_progress = |item: TransferProgress| {
        progress(ModpackProgress::transfer(item.map_total(3.0, 35.0)));
    };
    match manifest.loader.as_str() {
        "fabric" => Ok(
            MinecraftInstaller::new().install_fabric_loader_with_progress(
                instance,
                &manifest.minecraft_version,
                Some(&manifest.loader_version),
                &mut loader_progress,
            )?,
        ),
        "forge" => Ok(
            MinecraftInstaller::new().install_forge_with_java_and_progress(
                instance,
                &manifest.minecraft_version,
                &manifest.loader_version,
                forge_java,
                &mut loader_progress,
            )?,
        ),
        loader => Err(ModpackError::UnsupportedLoader(loader.to_owned())),
    }
}

fn read_manifest(path: &Path) -> Result<CurseForgeManifestSummary, ModpackError> {
    let mut archive = ZipArchive::new(File::open(path)?)?;
    let mut entry = archive
        .by_name("manifest.json")
        .map_err(|_| ModpackError::InvalidResponse)?;
    if entry.size() == 0 || entry.size() > MAX_MANIFEST_BYTES {
        return Err(ModpackError::InvalidResponse);
    }
    let mut contents = String::new();
    entry
        .by_ref()
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_string(&mut contents)?;
    if contents.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(ModpackError::InvalidResponse);
    }
    let manifest: Value =
        serde_json::from_str(&contents).map_err(|_| ModpackError::InvalidResponse)?;
    let minecraft_version = manifest
        .pointer("/minecraft/version")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 32)
        .ok_or(ModpackError::InvalidResponse)?
        .to_owned();
    let mod_loaders = manifest
        .pointer("/minecraft/modLoaders")
        .and_then(Value::as_array)
        .ok_or(ModpackError::InvalidResponse)?;
    let selected_loader = mod_loaders
        .iter()
        .filter_map(|loader| {
            let id = loader["id"].as_str()?;
            parse_loader(id).map(|parsed| (loader["primary"].as_bool().unwrap_or(false), parsed))
        })
        .max_by_key(|(primary, _)| *primary)
        .ok_or_else(|| ModpackError::UnsupportedLoader("desconhecido".to_owned()))?;
    let files = manifest["files"]
        .as_array()
        .ok_or(ModpackError::InvalidResponse)?;
    if files.len() > MAX_MANIFEST_FILES {
        return Err(ModpackError::InvalidResponse);
    }
    let files = files
        .iter()
        .map(|file| {
            let project_id = file["projectID"]
                .as_u64()
                .filter(|id| *id > 0)
                .ok_or(ModpackError::InvalidResponse)?;
            let file_id = file["fileID"]
                .as_u64()
                .filter(|id| *id > 0)
                .ok_or(ModpackError::InvalidResponse)?;
            Ok(CurseForgeManifestFile {
                project_id,
                file_id,
                required: file["required"].as_bool().unwrap_or(true),
            })
        })
        .collect::<Result<Vec<_>, ModpackError>>()?;
    Ok(CurseForgeManifestSummary {
        name: bounded_manifest_text(&manifest, "name", "Modpack CurseForge"),
        version_name: bounded_manifest_text(&manifest, "version", "versão do modpack"),
        minecraft_version,
        loader: selected_loader.1 .0,
        loader_version: selected_loader.1 .1,
        files,
    })
}

fn parse_loader(id: &str) -> Option<(String, String)> {
    let normalized = id.trim().to_ascii_lowercase();
    for loader in ["fabric", "forge"] {
        if let Some(version) = normalized.strip_prefix(&format!("{loader}-")) {
            if !version.is_empty()
                && version.len() <= 64
                && version.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+' | b'_')
                })
            {
                return Some((loader.to_owned(), version.to_owned()));
            }
        }
    }
    None
}

fn bounded_manifest_text(manifest: &Value, field: &str, fallback: &str) -> String {
    manifest[field]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(160).collect())
        .unwrap_or_else(|| fallback.to_owned())
}

fn extract_overrides(archive_path: &Path, instance_root: &Path) -> Result<usize, ModpackError> {
    let mut archive = ZipArchive::new(File::open(archive_path)?)?;
    let override_folder = {
        let mut manifest_entry = archive
            .by_name("manifest.json")
            .map_err(|_| ModpackError::InvalidResponse)?;
        let mut contents = String::new();
        manifest_entry
            .by_ref()
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_string(&mut contents)?;
        let manifest: Value =
            serde_json::from_str(&contents).map_err(|_| ModpackError::InvalidResponse)?;
        manifest["overrides"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("overrides")
            .replace('\\', "/")
    };
    let safe_folder = safe_relative_path(&override_folder)?;
    if safe_folder.components().count() != 1 {
        return Err(ModpackError::UnsafePath(override_folder));
    }
    let prefix = format!("{}/", safe_folder.display().to_string().replace('\\', "/"));
    let mut extracted = 0usize;
    let mut extracted_bytes = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().replace('\\', "/");
        let Some(relative) = name.strip_prefix(&prefix) else {
            continue;
        };
        if relative.is_empty() || entry.is_dir() {
            continue;
        }
        if extracted >= MAX_OVERRIDE_FILES
            || entry.size() > MAX_OVERRIDE_FILE_BYTES
            || extracted_bytes.saturating_add(entry.size()) > MAX_OVERRIDE_TOTAL_BYTES
        {
            return Err(ModpackError::InvalidResponse);
        }
        let entry_size = entry.size();
        let relative = safe_relative_path(relative)?;
        let destination = instance_root.join(relative);
        let parent = destination
            .parent()
            .ok_or_else(|| ModpackError::UnsafePath(name.clone()))?;
        fs::create_dir_all(parent)?;
        let mut output = File::create(destination)?;
        io::copy(&mut entry, &mut output)?;
        extracted += 1;
        extracted_bytes = extracted_bytes.saturating_add(entry_size);
    }
    Ok(extracted)
}

fn safe_relative_path(value: &str) -> Result<PathBuf, ModpackError> {
    let path = Path::new(value);
    if path.is_absolute()
        || value.is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ModpackError::UnsafePath(value.to_owned()));
    }
    Ok(path.to_path_buf())
}

fn validate_pack_filename(file_name: &str) -> Result<(), ModpackError> {
    validate_filename(file_name, ".zip")
}

fn validate_mod_filename(file_name: &str) -> Result<(), ModpackError> {
    validate_filename(file_name, ".jar")
}

fn validate_filename(file_name: &str, extension: &str) -> Result<(), ModpackError> {
    if file_name.is_empty()
        || file_name.len() > 255
        || file_name.contains(['/', '\\'])
        || !file_name.to_ascii_lowercase().ends_with(extension)
    {
        return Err(ModpackError::UnsafePath(file_name.to_owned()));
    }
    Ok(())
}

fn validate_sha1(value: &str) -> Result<(), ModpackError> {
    if value.len() != 40 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(ModpackError::InvalidResponse);
    }
    Ok(())
}

fn verify_sha1(path: &Path, expected: &str) -> Result<(), ModpackError> {
    validate_sha1(expected)?;
    let mut file = File::open(path)?;
    let mut digest = Sha1::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        digest.update(&buffer[..size]);
    }
    if !format!("{:x}", digest.finalize()).eq_ignore_ascii_case(expected) {
        return Err(ModpackError::Integrity(path.to_path_buf()));
    }
    Ok(())
}

fn client() -> Client {
    Client::builder()
        .user_agent("Aurora-Smart-Launcher/0.1")
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(10 * 60))
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .pool_max_idle_per_host(DEFAULT_DOWNLOAD_CONCURRENCY)
        .build()
        .expect("cliente HTTP deve inicializar")
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::io::Write;

    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    use super::{parse_loader, read_manifest};

    #[test]
    fn accepts_only_supported_curseforge_loaders() {
        assert_eq!(
            parse_loader("fabric-0.14.21"),
            Some(("fabric".to_owned(), "0.14.21".to_owned()))
        );
        assert_eq!(
            parse_loader("forge-36.2.39"),
            Some(("forge".to_owned(), "36.2.39".to_owned()))
        );
        assert!(parse_loader("neoforge-21.1.0").is_none());
        assert!(parse_loader("forge-../../unsafe").is_none());
    }

    #[test]
    fn reads_a_curseforge_manifest_with_required_files() {
        let path = std::env::temp_dir().join(format!(
            "aurora-curseforge-manifest-{}.zip",
            uuid::Uuid::new_v4()
        ));
        let file = File::create(&path).expect("arquivo temporário deve ser criado");
        let mut writer = ZipWriter::new(file);
        writer
            .start_file("manifest.json", SimpleFileOptions::default())
            .expect("manifesto deve iniciar");
        writer
            .write_all(
                br#"{
                  "name":"Pack de teste",
                  "version":"1.2.3",
                  "minecraft":{"version":"1.16.5","modLoaders":[{"id":"fabric-0.14.21","primary":true}]},
                  "files":[{"projectID":238222,"fileID":4019553,"required":true}],
                  "overrides":"overrides"
                }"#,
            )
            .expect("manifesto deve ser escrito");
        writer.finish().expect("zip deve finalizar");
        let manifest = read_manifest(&path).expect("manifesto deve ser aceito");
        assert_eq!(manifest.minecraft_version, "1.16.5");
        assert_eq!(manifest.loader, "fabric");
        assert_eq!(manifest.loader_version, "0.14.21");
        assert_eq!(manifest.files.len(), 1);
        assert!(manifest.files[0].required);
        let _ = fs::remove_file(path);
    }
}
