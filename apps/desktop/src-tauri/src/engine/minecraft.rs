//! Resolução e instalação dos arquivos oficiais do Minecraft Java.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use zip::ZipArchive;

use super::download::{
    download_many, download_one_with_progress, DownloadError, DownloadRequest, ExpectedHash,
};
use super::{Instance, TransferProgress, DEFAULT_DOWNLOAD_CONCURRENCY};

const VERSION_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const ASSET_HOST: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("falha de rede: {0}")]
    Network(#[from] reqwest::Error),
    #[error("falha ao baixar arquivo: {0}")]
    Download(#[from] DownloadError),
    #[error("falha de E/S: {0}")]
    Io(#[from] io::Error),
    #[error("metadado inválido: {0}")]
    Metadata(String),
    #[error("a versão Minecraft não foi encontrada: {0}")]
    UnknownVersion(String),
    #[error("integridade inválida para {0}")]
    Integrity(PathBuf),
    #[error("arquivo compactado inválido: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("não há Java {required} instalado para preparar esta versão do Forge")]
    JavaRequired { required: u32 },
    #[error("o instalador oficial do Forge falhou: {0}")]
    ForgeInstaller(String),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSummary {
    pub minecraft_version: String,
    pub version_id: String,
    pub client_jar: String,
    pub library_count: usize,
    pub asset_count: usize,
}

#[derive(Clone)]
pub struct MinecraftInstaller {
    client: Client,
}

impl Default for MinecraftInstaller {
    fn default() -> Self {
        Self::new()
    }
}

impl MinecraftInstaller {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("Aurora-Smart-Launcher/0.1")
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(10 * 60))
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .pool_max_idle_per_host(DEFAULT_DOWNLOAD_CONCURRENCY)
            .build()
            .expect("cliente HTTP do Aurora deve ser inicializável");
        Self { client }
    }

    /// Instala o cliente vanilla, assets, bibliotecas e natives Windows na instância.
    pub fn install_vanilla(
        &self,
        instance: &Instance,
        minecraft_version: &str,
    ) -> Result<InstallSummary, InstallError> {
        self.install_vanilla_with_progress(instance, minecraft_version, |_| {})
    }

    pub fn install_vanilla_with_progress<F>(
        &self,
        instance: &Instance,
        minecraft_version: &str,
        mut progress: F,
    ) -> Result<InstallSummary, InstallError>
    where
        F: FnMut(TransferProgress),
    {
        self.install_vanilla_in_range(instance, minecraft_version, 0.0, 100.0, &mut progress)
    }

    fn install_vanilla_in_range<F>(
        &self,
        instance: &Instance,
        minecraft_version: &str,
        start: f64,
        end: f64,
        progress: &mut F,
    ) -> Result<InstallSummary, InstallError>
    where
        F: FnMut(TransferProgress),
    {
        progress(stage_progress(
            format!("Consultando Minecraft {minecraft_version}"),
            start,
        ));
        instance
            .ensure_layout()
            .map_err(|error| InstallError::Io(io::Error::other(error)))?;
        let manifest: Value = self.get_json(VERSION_MANIFEST)?;
        let version_url = manifest["versions"]
            .as_array()
            .and_then(|versions| {
                versions
                    .iter()
                    .find(|entry| entry["id"].as_str() == Some(minecraft_version))
            })
            .and_then(|entry| entry["url"].as_str())
            .ok_or_else(|| InstallError::UnknownVersion(minecraft_version.to_owned()))?;
        let metadata: Value = self.get_json(version_url)?;
        let version_id = metadata["id"]
            .as_str()
            .unwrap_or(minecraft_version)
            .to_owned();
        let version_dir = instance.root().join("versions").join(&version_id);
        fs::create_dir_all(&version_dir)?;
        fs::write(
            version_dir.join(format!("{version_id}.json")),
            serde_json::to_vec_pretty(&metadata).map_err(metadata_error)?,
        )?;

        let client_download = &metadata["downloads"]["client"];
        let client_jar = version_dir.join(format!("{version_id}.jar"));
        let client_request = descriptor_request(
            client_download,
            &client_jar,
            format!("Minecraft {version_id}.jar"),
        )?;
        download_one_with_progress(&self.client, client_request, |item| {
            progress(item.map_total(scale(start, end, 1.0), scale(start, end, 7.0)))
        })?;

        let mut libraries = 0usize;
        let mut library_downloads = Vec::new();
        let mut native_archives = Vec::new();
        for library in metadata["libraries"]
            .as_array()
            .ok_or_else(|| InstallError::Metadata("libraries ausente".into()))?
        {
            if !rules_allow_windows(library) {
                continue;
            }
            if let Some(artifact) = library.pointer("/downloads/artifact") {
                let path = artifact["path"]
                    .as_str()
                    .ok_or_else(|| InstallError::Metadata("path de biblioteca ausente".into()))?;
                library_downloads.push(descriptor_request(
                    artifact,
                    &instance.root().join("libraries").join(path),
                    path.rsplit('/').next().unwrap_or(path).to_owned(),
                )?);
                libraries += 1;
            }
            if let Some(classifier) = native_classifier(library) {
                if let Some(native) =
                    library.pointer(&format!("/downloads/classifiers/{classifier}"))
                {
                    let path = native["path"]
                        .as_str()
                        .ok_or_else(|| InstallError::Metadata("path de native ausente".into()))?;
                    let native_path = instance.root().join("libraries").join(path);
                    library_downloads.push(descriptor_request(
                        native,
                        &native_path,
                        path.rsplit('/').next().unwrap_or(path).to_owned(),
                    )?);
                    native_archives
                        .push((native_path, library.pointer("/extract/exclude").cloned()));
                }
            }
        }
        download_many(
            &self.client,
            library_downloads,
            DEFAULT_DOWNLOAD_CONCURRENCY,
            |item| progress(item.map_total(scale(start, end, 7.0), scale(start, end, 45.0))),
        )?;
        for (native_path, excludes) in native_archives {
            extract_native(&native_path, &instance.natives_dir(), excludes.as_ref())?;
        }

        let asset_index = &metadata["assetIndex"];
        let asset_id = asset_index["id"]
            .as_str()
            .ok_or_else(|| InstallError::Metadata("assetIndex ausente".into()))?;
        let asset_path = instance
            .root()
            .join("assets")
            .join("indexes")
            .join(format!("{asset_id}.json"));
        let asset_index_request = descriptor_request(
            asset_index,
            &asset_path,
            format!("Índice de assets {asset_id}"),
        )?;
        download_one_with_progress(&self.client, asset_index_request, |item| {
            progress(item.map_total(scale(start, end, 45.0), scale(start, end, 49.0)))
        })?;
        let assets: Value =
            serde_json::from_reader(File::open(&asset_path)?).map_err(metadata_error)?;
        let objects = assets["objects"]
            .as_object()
            .ok_or_else(|| InstallError::Metadata("objetos de assets ausentes".into()))?;
        let mut asset_downloads = Vec::with_capacity(objects.len());
        for (asset_name, object) in objects {
            let hash = object["hash"]
                .as_str()
                .ok_or_else(|| InstallError::Metadata("hash de asset ausente".into()))?;
            let relative = format!("{}/{}", &hash[..2], hash);
            asset_downloads.push(DownloadRequest {
                url: format!("{ASSET_HOST}/{relative}"),
                destination: instance
                    .root()
                    .join("assets")
                    .join("objects")
                    .join(relative),
                label: asset_name.clone(),
                expected_hash: Some(ExpectedHash::Sha1(hash.to_owned())),
                expected_size: object["size"].as_u64(),
            });
        }
        download_many(
            &self.client,
            asset_downloads,
            DEFAULT_DOWNLOAD_CONCURRENCY,
            |item| progress(item.map_total(scale(start, end, 49.0), scale(start, end, 99.0))),
        )?;

        progress(stage_progress(
            format!("Minecraft {minecraft_version} preparado"),
            end,
        ));

        Ok(InstallSummary {
            minecraft_version: minecraft_version.to_owned(),
            version_id,
            client_jar: client_jar.display().to_string(),
            library_count: libraries,
            asset_count: objects.len(),
        })
    }

    /// Instala o perfil oficial do Fabric sobre a instalação vanilla da instância.
    /// O perfil é o formato consumido pelo launcher oficial e inclui as bibliotecas
    /// adicionais do Loader; a versão do Loader é consultada no Fabric Meta.
    pub fn install_fabric(
        &self,
        instance: &Instance,
        minecraft_version: &str,
    ) -> Result<InstallSummary, InstallError> {
        self.install_fabric_loader(instance, minecraft_version, None)
    }

    /// Instala a versão exata do Fabric Loader pedida pelo modpack. Sem uma
    /// versão explícita, usa a estável mais recente publicada pelo Fabric Meta.
    pub fn install_fabric_loader(
        &self,
        instance: &Instance,
        minecraft_version: &str,
        requested_loader_version: Option<&str>,
    ) -> Result<InstallSummary, InstallError> {
        self.install_fabric_loader_with_progress(
            instance,
            minecraft_version,
            requested_loader_version,
            |_| {},
        )
    }

    pub fn install_fabric_loader_with_progress<F>(
        &self,
        instance: &Instance,
        minecraft_version: &str,
        requested_loader_version: Option<&str>,
        mut progress: F,
    ) -> Result<InstallSummary, InstallError>
    where
        F: FnMut(TransferProgress),
    {
        let mut summary =
            self.install_vanilla_in_range(instance, minecraft_version, 0.0, 88.0, &mut progress)?;
        let loader_version = match requested_loader_version {
            Some(version) if !version.trim().is_empty() => version.to_owned(),
            _ => {
                let loaders: Value = self.get_json(&format!(
                    "https://meta.fabricmc.net/v2/versions/loader/{minecraft_version}/stable"
                ))?;
                loaders
                    .as_array()
                    .and_then(|entries| entries.first())
                    .and_then(|entry| entry.pointer("/loader/version"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        InstallError::Metadata("Fabric Loader indisponível para esta versão".into())
                    })?
                    .to_owned()
            }
        };
        let profile: Value = self.get_json(&format!("https://meta.fabricmc.net/v2/versions/loader/{minecraft_version}/{loader_version}/profile/json"))?;
        let version_id = profile["id"]
            .as_str()
            .ok_or_else(|| InstallError::Metadata("id do perfil Fabric ausente".into()))?;
        let version_dir = instance.root().join("versions").join(version_id);
        fs::create_dir_all(&version_dir)?;
        fs::write(
            version_dir.join(format!("{version_id}.json")),
            serde_json::to_vec_pretty(&profile).map_err(metadata_error)?,
        )?;
        let mut fabric_downloads = Vec::new();
        if let Some(libraries) = profile["libraries"].as_array() {
            for library in libraries {
                if !rules_allow_windows(library) {
                    continue;
                }
                if let Some(artifact) = library_artifact_descriptor(library)? {
                    let path = artifact["path"].as_str().ok_or_else(|| {
                        InstallError::Metadata("path de biblioteca Fabric ausente".into())
                    })?;
                    fabric_downloads.push(descriptor_request(
                        &artifact,
                        &instance.root().join("libraries").join(path),
                        path.rsplit('/').next().unwrap_or(path).to_owned(),
                    )?);
                    summary.library_count += 1;
                }
            }
        }
        download_many(
            &self.client,
            fabric_downloads,
            DEFAULT_DOWNLOAD_CONCURRENCY,
            |item| progress(item.map_total(88.0, 99.0)),
        )?;
        summary.version_id = version_id.to_owned();
        progress(stage_progress("Fabric preparado", 100.0));
        Ok(summary)
    }

    /// Executa o instalador oficial do Forge dentro da instância. O binário do
    /// Forge não é redistribuído pelo Aurora: ele é baixado do Maven oficial,
    /// validado pelo checksum publicado e iniciado com Java separado.
    pub fn install_forge(
        &self,
        instance: &Instance,
        minecraft_version: &str,
        forge_version: &str,
    ) -> Result<InstallSummary, InstallError> {
        self.install_forge_with_java(instance, minecraft_version, forge_version, None)
    }

    pub fn install_forge_with_java(
        &self,
        instance: &Instance,
        minecraft_version: &str,
        forge_version: &str,
        java_override: Option<&Path>,
    ) -> Result<InstallSummary, InstallError> {
        self.install_forge_with_java_and_progress(
            instance,
            minecraft_version,
            forge_version,
            java_override,
            |_| {},
        )
    }

    pub fn install_forge_with_java_and_progress<F>(
        &self,
        instance: &Instance,
        minecraft_version: &str,
        forge_version: &str,
        java_override: Option<&Path>,
        mut progress: F,
    ) -> Result<InstallSummary, InstallError>
    where
        F: FnMut(TransferProgress),
    {
        let mut summary =
            self.install_vanilla_in_range(instance, minecraft_version, 0.0, 84.0, &mut progress)?;
        let required_java = required_java_major(minecraft_version);
        let java = java_override
            .map(Path::to_path_buf)
            .or_else(|| find_java(required_java))
            .ok_or(InstallError::JavaRequired {
                required: required_java,
            })?;
        let forge_id = format!("{minecraft_version}-{forge_version}");
        let base = format!(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/{forge_id}/forge-{forge_id}-installer.jar"
        );
        let checksum = self
            .client
            .get(format!("{base}.sha1"))
            .send()?
            .error_for_status()?
            .text()?;
        let checksum = checksum
            .split_whitespace()
            .next()
            .ok_or_else(|| InstallError::Metadata("checksum do Forge ausente".to_owned()))?;
        let installer = instance
            .root()
            .join("installers")
            .join(format!("forge-{forge_id}-installer.jar"));
        let request = DownloadRequest {
            url: base,
            destination: installer.clone(),
            label: installer
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Forge installer")
                .to_owned(),
            expected_hash: Some(ExpectedHash::Sha1(checksum.to_owned())),
            expected_size: None,
        };
        download_one_with_progress(&self.client, request, |item| {
            progress(item.map_total(84.0, 93.0))
        })?;

        // Instaladores Forge antigos recusam diretórios que ainda não tenham
        // sido inicializados pelo launcher oficial. O Aurora é o launcher,
        // portanto fornece somente o marcador mínimo esperado pelo instalador.
        let launcher_profiles = instance.root().join("launcher_profiles.json");
        if !launcher_profiles.exists() {
            fs::write(&launcher_profiles, r#"{"profiles":{}}"#)?;
        }

        progress(stage_progress("Executando instalador do Forge", 95.0));
        let mut command = Command::new(java);
        command.arg("-jar").arg(&installer).arg("--installClient");
        if minecraft_version == "1.12.2" {
            command.arg(instance.root());
        } else {
            command.arg("--installPath").arg(instance.root());
        }
        let output = command.output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let details = stderr
                .lines()
                .chain(stdout.lines())
                .rfind(|line| !line.trim().is_empty())
                .unwrap_or("sem detalhes");
            return Err(InstallError::ForgeInstaller(details.to_owned()));
        }
        summary.version_id = format!("{minecraft_version}-forge-{forge_version}");
        progress(stage_progress("Forge preparado", 100.0));
        Ok(summary)
    }

    fn get_json(&self, url: &str) -> Result<Value, InstallError> {
        Ok(self.client.get(url).send()?.error_for_status()?.json()?)
    }
}

fn descriptor_request(
    descriptor: &Value,
    destination: &Path,
    label: String,
) -> Result<DownloadRequest, InstallError> {
    let url = descriptor["url"]
        .as_str()
        .ok_or_else(|| InstallError::Metadata("URL de download ausente".into()))?;
    Ok(DownloadRequest {
        url: url.to_owned(),
        destination: destination.to_path_buf(),
        label,
        expected_hash: descriptor["sha1"]
            .as_str()
            .map(|hash| ExpectedHash::Sha1(hash.to_owned())),
        expected_size: descriptor["size"].as_u64(),
    })
}

fn scale(start: f64, end: f64, local_percent: f64) -> f64 {
    start + (end - start) * local_percent / 100.0
}

fn stage_progress(label: impl Into<String>, total_percent: f64) -> TransferProgress {
    TransferProgress {
        label: label.into(),
        total_percent: total_percent.clamp(0.0, 100.0),
        item_percent: 100.0,
        ..TransferProgress::default()
    }
}

/// Perfis de loaders frequentemente usam coordenadas Maven (name + url), em
/// vez do objeto downloads adotado pelos manifests do Mojang.
fn library_artifact_descriptor(library: &Value) -> Result<Option<Value>, InstallError> {
    if let Some(artifact) = library.pointer("/downloads/artifact") {
        return Ok(Some(artifact.clone()));
    }
    let Some(name) = library["name"].as_str() else {
        return Ok(None);
    };
    let (coordinate, extension) = name.split_once('@').unwrap_or((name, "jar"));
    let pieces: Vec<&str> = coordinate.split(':').collect();
    if pieces.len() < 3 {
        return Err(InstallError::Metadata(format!(
            "coordenada Maven inválida: {name}"
        )));
    }
    let group = pieces[0].replace('.', "/");
    let artifact = pieces[1];
    let version = pieces[2];
    let classifier = pieces.get(3).copied().filter(|value| !value.is_empty());
    let filename = match classifier {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{extension}"),
        None => format!("{artifact}-{version}.{extension}"),
    };
    let path = format!("{group}/{artifact}/{version}/{filename}");
    let repository = library["url"]
        .as_str()
        .unwrap_or("https://maven.fabricmc.net/");
    let url = format!("{}/{}", repository.trim_end_matches('/'), path);
    Ok(Some(serde_json::json!({ "path": path, "url": url })))
}

fn metadata_error(error: serde_json::Error) -> InstallError {
    InstallError::Metadata(error.to_string())
}

pub fn required_java_major(minecraft_version: &str) -> u32 {
    if minecraft_version.starts_with("1.21") {
        21
    } else if minecraft_version.starts_with("1.18")
        || minecraft_version.starts_with("1.19")
        || minecraft_version.starts_with("1.20")
    {
        17
    } else {
        8
    }
}

fn find_java(required_major: u32) -> Option<PathBuf> {
    super::discover_java_executables().into_iter().find(|path| {
        Command::new(path)
            .arg("-version")
            .output()
            .ok()
            .and_then(|output| parse_java_major(&String::from_utf8_lossy(&output.stderr)))
            == Some(required_major)
    })
}

fn parse_java_major(version_text: &str) -> Option<u32> {
    let quoted = version_text.split('"').nth(1)?;
    let mut numbers = quoted.split('.');
    let first = numbers.next()?.parse::<u32>().ok()?;
    if first == 1 {
        numbers.next()?.parse::<u32>().ok()
    } else {
        Some(first)
    }
}

fn rules_allow_windows(library: &Value) -> bool {
    let Some(rules) = library["rules"].as_array() else {
        return true;
    };
    let mut allowed = false;
    for rule in rules {
        let applies = rule
            .pointer("/os/name")
            .and_then(Value::as_str)
            .map(|name| name == "windows")
            .unwrap_or(true);
        if applies {
            allowed = rule["action"].as_str() == Some("allow");
        }
    }
    allowed
}

fn native_classifier(library: &Value) -> Option<String> {
    let natives = library["natives"].as_object()?;
    let classifier = natives
        .get("windows")
        .or_else(|| natives.get("windows-64"))?
        .as_str()?;
    Some(classifier.replace("${arch}", "64"))
}

fn extract_native(
    path: &Path,
    target: &Path,
    excludes: Option<&Value>,
) -> Result<(), InstallError> {
    fs::create_dir_all(target)?;
    let excluded: Vec<&str> = excludes
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mut archive = ZipArchive::new(File::open(path)?)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().replace('\\', "/");
        if entry.is_dir()
            || name.starts_with("META-INF/")
            || excluded.iter().any(|prefix| name.starts_with(prefix))
        {
            continue;
        }
        let Some(filename) = Path::new(&name).file_name() else {
            continue;
        };
        let destination = target.join(filename);
        let mut output = File::create(destination)?;
        io::copy(&mut entry, &mut output)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_java_major, required_java_major};

    #[test]
    fn recognizes_legacy_and_modern_java_versions() {
        assert_eq!(parse_java_major("java version \"1.8.0_452\""), Some(8));
        assert_eq!(parse_java_major("openjdk version \"21.0.6\""), Some(21));
    }

    #[test]
    fn selects_java_generation_for_forge() {
        assert_eq!(required_java_major("1.12.2"), 8);
        assert_eq!(required_java_major("1.20.1"), 17);
        assert_eq!(required_java_major("1.21.1"), 21);
    }
}
