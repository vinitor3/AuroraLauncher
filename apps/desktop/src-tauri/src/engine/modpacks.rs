//! Catálogo e instalação segura de modpacks públicos do Modrinth.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use reqwest::blocking::Client;
use reqwest::Url;
use serde::Serialize;
use serde_json::Value;
use sha1::{Digest as Sha1Digest, Sha1};
use thiserror::Error;
use zip::ZipArchive;

use super::download::{
    download_many, download_one_with_progress, DownloadError, DownloadRequest, ExpectedHash,
};
use super::{
    InstallError, InstallSummary, Instance, MinecraftInstaller, TransferProgress,
    DEFAULT_DOWNLOAD_CONCURRENCY,
};

const MODRINTH_API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Error)]
pub enum ModpackError {
    #[error("falha ao consultar Modrinth: {0}")]
    Network(#[from] reqwest::Error),
    #[error("falha ao baixar arquivo: {0}")]
    Download(#[from] DownloadError),
    #[error("resposta Modrinth inválida")]
    InvalidResponse,
    #[error("o modpack não possui uma versão compatível com Minecraft {0}")]
    NoCompatibleVersion(String),
    #[error("o modpack usa o loader {0}, que ainda não pode ser instalado automaticamente")]
    UnsupportedLoader(String),
    #[error("o arquivo do modpack não possui download válido")]
    MissingDownload,
    #[error("caminho inválido no modpack: {0}")]
    UnsafePath(String),
    #[error("falha de E/S: {0}")]
    Io(#[from] io::Error),
    #[error("arquivo compactado inválido: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("falha ao instalar Minecraft: {0}")]
    Minecraft(#[from] InstallError),
    #[error("integridade inválida para {0}")]
    Integrity(PathBuf),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthPack {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub versions: Vec<String>,
    pub loaders: Vec<String>,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub follows: u64,
    pub author: String,
    pub date_modified: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthSearchPage {
    pub items: Vec<ModrinthPack>,
    pub total_hits: u64,
    pub offset: u32,
    pub limit: u32,
}

/// Resultado público reutilizado pelas três abas do editor da instância.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthContent {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub versions: Vec<String>,
    pub loaders: Vec<String>,
    pub downloads: u64,
    pub author: String,
    pub date_modified: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedContentArtwork {
    pub filename: String,
    pub project_id: String,
    pub title: String,
    pub icon_url: Option<String>,
}

pub fn search_modrinth_modpacks(
    query: &str,
    offset: u32,
    limit: u32,
) -> Result<ModrinthSearchPage, ModpackError> {
    let limit = limit.clamp(1, 50);
    let mut url = Url::parse(&format!("{MODRINTH_API}/search")).expect("URL Modrinth válida");
    url.query_pairs_mut()
        .append_pair("query", query.trim())
        .append_pair("limit", &limit.to_string())
        .append_pair("offset", &offset.to_string())
        .append_pair("facets", r#"[["project_type:modpack"]]"#);
    let document: Value = Client::builder()
        .user_agent("Aurora-Smart-Launcher/0.1")
        .build()
        .expect("cliente HTTP deve inicializar")
        .get(url)
        .send()?
        .error_for_status()?
        .json()?;
    let hits = document["hits"]
        .as_array()
        .ok_or(ModpackError::InvalidResponse)?;
    let items = hits
        .iter()
        .filter_map(|hit| {
            Some(ModrinthPack {
                project_id: hit["project_id"].as_str()?.to_owned(),
                slug: hit["slug"].as_str().unwrap_or_default().to_owned(),
                title: hit["title"].as_str()?.to_owned(),
                description: hit["description"].as_str().unwrap_or_default().to_owned(),
                versions: hit["versions"]
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                loaders: hit["categories"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter(|category| {
                        matches!(*category, "fabric" | "forge" | "neoforge" | "quilt")
                    })
                    .map(str::to_owned)
                    .collect(),
                icon_url: hit["icon_url"].as_str().map(str::to_owned),
                downloads: hit["downloads"].as_u64().unwrap_or_default(),
                follows: hit["follows"].as_u64().unwrap_or_default(),
                author: hit["author"].as_str().unwrap_or_default().to_owned(),
                date_modified: hit["date_modified"].as_str().unwrap_or_default().to_owned(),
            })
        })
        .filter(|pack| {
            pack.loaders
                .iter()
                .any(|loader| loader == "fabric" || loader == "forge")
        })
        .collect();
    Ok(ModrinthSearchPage {
        items,
        total_hits: document["total_hits"].as_u64().unwrap_or_default(),
        offset,
        limit,
    })
}

pub fn search_modrinth_content(
    query: &str,
    content_type: &str,
    minecraft_version: Option<&str>,
    loader: Option<&str>,
    sort: &str,
) -> Result<Vec<ModrinthContent>, ModpackError> {
    let project_type = match content_type {
        "mod" | "shader" | "resourcepack" => content_type,
        _ => return Err(ModpackError::InvalidResponse),
    };
    let mut url = Url::parse(&format!("{MODRINTH_API}/search")).expect("URL Modrinth válida");
    let mut facets = vec![vec![format!("project_type:{project_type}")]];
    if let Some(version) = minecraft_version.filter(|value| !value.trim().is_empty()) {
        facets.push(vec![format!("versions:{}", version.trim())]);
    }
    if content_type == "mod" {
        if let Some(loader) = loader.filter(|value| matches!(*value, "fabric" | "forge")) {
            facets.push(vec![format!("categories:{loader}")]);
        }
    }
    let facets = serde_json::to_string(&facets).map_err(|_| ModpackError::InvalidResponse)?;
    let index = match sort {
        "relevance" if !query.trim().is_empty() => "relevance",
        "updated" => "updated",
        _ => "downloads",
    };
    {
        let mut query_pairs = url.query_pairs_mut();
        if !query.trim().is_empty() {
            query_pairs.append_pair("query", query.trim());
        }
        query_pairs
            .append_pair("limit", "24")
            .append_pair("index", index)
            .append_pair("facets", &facets);
    }
    let document: Value = Client::builder()
        .user_agent("Aurora-Smart-Launcher/0.1")
        .build()
        .expect("cliente HTTP deve inicializar")
        .get(url)
        .send()?
        .error_for_status()?
        .json()?;
    let hits = document["hits"]
        .as_array()
        .ok_or(ModpackError::InvalidResponse)?;
    Ok(hits
        .iter()
        .filter_map(|hit| {
            Some(ModrinthContent {
                project_id: hit["project_id"].as_str()?.to_owned(),
                slug: hit["slug"].as_str().unwrap_or_default().to_owned(),
                title: hit["title"].as_str()?.to_owned(),
                description: hit["description"].as_str().unwrap_or_default().to_owned(),
                icon_url: hit["icon_url"].as_str().map(str::to_owned),
                versions: hit["versions"]
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                loaders: hit["categories"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter(|category| matches!(*category, "fabric" | "forge"))
                    .map(str::to_owned)
                    .collect(),
                downloads: hit["downloads"].as_u64().unwrap_or_default(),
                author: hit["author"].as_str().unwrap_or_default().to_owned(),
                date_modified: hit["date_modified"].as_str().unwrap_or_default().to_owned(),
            })
        })
        .collect())
}

/// Instala um arquivo resolvido por um catálogo protegido (por exemplo, CurseForge).
/// O nome, protocolo e hash são validados antes que o arquivo seja gravado.
pub fn install_remote_content(
    instance: &Instance,
    url: &str,
    filename: &str,
    content_type: &str,
    expected_sha1: Option<&str>,
) -> Result<String, ModpackError> {
    install_remote_content_with_progress(
        instance,
        url,
        filename,
        content_type,
        expected_sha1,
        |_| {},
    )
}

pub fn install_remote_content_with_progress<F>(
    instance: &Instance,
    url: &str,
    filename: &str,
    content_type: &str,
    expected_sha1: Option<&str>,
    progress: F,
) -> Result<String, ModpackError>
where
    F: FnMut(TransferProgress),
{
    let folder = match content_type {
        "mod" => "mods",
        "shader" => "shaderpacks",
        "resourcepack" => "resourcepacks",
        _ => return Err(ModpackError::InvalidResponse),
    };
    let parsed_url = Url::parse(url).map_err(|_| ModpackError::MissingDownload)?;
    if parsed_url.scheme() != "https" {
        return Err(ModpackError::MissingDownload);
    }
    if filename.is_empty() || filename.contains(['/', '\\']) {
        return Err(ModpackError::UnsafePath(filename.to_owned()));
    }
    if content_type == "mod" && !filename.to_ascii_lowercase().ends_with(".jar") {
        return Err(ModpackError::MissingDownload);
    }
    if content_type != "mod" && !filename.to_ascii_lowercase().ends_with(".zip") {
        return Err(ModpackError::MissingDownload);
    }
    instance
        .ensure_layout()
        .map_err(|error| ModpackError::Io(io::Error::other(error)))?;
    let destination = instance.root().join(folder).join(filename);
    let request = content_download_request(
        parsed_url.as_str(),
        &destination,
        filename,
        expected_sha1,
        None,
        None,
    )?;
    download_one_with_progress(&client(), request, progress)?;
    Ok(filename.to_owned())
}

/// Reconhece arquivos já instalados usando somente o SHA-1 local e a API pública
/// em lote do Modrinth. O conteúdo do arquivo nunca sai do computador.
pub fn resolve_modrinth_content_artwork(
    instance: &Instance,
    content_type: &str,
) -> Result<Vec<ResolvedContentArtwork>, ModpackError> {
    let folder = match content_type {
        "mod" => "mods",
        "shader" => "shaderpacks",
        "resourcepack" => "resourcepacks",
        _ => return Err(ModpackError::InvalidResponse),
    };
    let directory = instance.root().join(folder);
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut hash_to_filename = std::collections::HashMap::new();
    for entry in fs::read_dir(directory)?.filter_map(Result::ok) {
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let disk_name = entry.file_name().to_string_lossy().into_owned();
        let filename = disk_name
            .strip_suffix(".disabled")
            .unwrap_or(&disk_name)
            .to_owned();
        let expected_extension = if content_type == "mod" {
            ".jar"
        } else {
            ".zip"
        };
        if !filename.to_ascii_lowercase().ends_with(expected_extension) {
            continue;
        }
        if let Some(hash) = sha1_file(&entry.path()) {
            hash_to_filename.insert(hash, filename);
        }
    }
    if hash_to_filename.is_empty() {
        return Ok(Vec::new());
    }
    let client = client();
    let mut file_projects = std::collections::HashMap::<String, String>::new();
    let hashes = hash_to_filename.keys().cloned().collect::<Vec<_>>();
    for batch in hashes.chunks(100) {
        let response: Value = client
            .post(format!("{MODRINTH_API}/version_files"))
            .json(&serde_json::json!({ "hashes": batch, "algorithm": "sha1" }))
            .send()?
            .error_for_status()?
            .json()?;
        if let Some(versions) = response.as_object() {
            for (hash, version) in versions {
                if let (Some(filename), Some(project_id)) =
                    (hash_to_filename.get(hash), version["project_id"].as_str())
                {
                    file_projects.insert(filename.clone(), project_id.to_owned());
                }
            }
        }
    }
    let project_ids = file_projects
        .values()
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut project_details = std::collections::HashMap::<String, (String, Option<String>)>::new();
    for batch in project_ids.chunks(100) {
        let mut url = Url::parse(&format!("{MODRINTH_API}/projects")).expect("URL Modrinth válida");
        url.query_pairs_mut().append_pair(
            "ids",
            &serde_json::to_string(batch).map_err(|_| ModpackError::InvalidResponse)?,
        );
        let projects: Value = client.get(url).send()?.error_for_status()?.json()?;
        if let Some(projects) = projects.as_array() {
            for project in projects {
                if let (Some(id), Some(title)) = (project["id"].as_str(), project["title"].as_str())
                {
                    project_details.insert(
                        id.to_owned(),
                        (
                            title.to_owned(),
                            project["icon_url"].as_str().map(str::to_owned),
                        ),
                    );
                }
            }
        }
    }
    Ok(file_projects
        .into_iter()
        .filter_map(|(filename, project_id)| {
            let (title, icon_url) = project_details.get(&project_id)?.clone();
            Some(ResolvedContentArtwork {
                filename,
                project_id,
                title,
                icon_url,
            })
        })
        .collect())
}

/// Baixa um único arquivo público do Modrinth para a pasta correta da instância.
/// Todo arquivo é validado pelo hash publicado antes de substituir o anterior.
pub fn install_modrinth_content(
    instance: &Instance,
    project_id: &str,
    minecraft_version: &str,
    content_type: &str,
    loader: Option<&str>,
) -> Result<String, ModpackError> {
    install_modrinth_content_with_progress(
        instance,
        project_id,
        minecraft_version,
        content_type,
        loader,
        |_| {},
    )
}

pub fn install_modrinth_content_with_progress<F>(
    instance: &Instance,
    project_id: &str,
    minecraft_version: &str,
    content_type: &str,
    loader: Option<&str>,
    progress: F,
) -> Result<String, ModpackError>
where
    F: FnMut(TransferProgress),
{
    let folder = match content_type {
        "mod" => "mods",
        "shader" => "shaderpacks",
        "resourcepack" => "resourcepacks",
        _ => return Err(ModpackError::InvalidResponse),
    };
    let client = client();
    let versions: Value = get_json(
        &client,
        &format!("{MODRINTH_API}/project/{project_id}/version"),
    )?;
    let selected = versions
        .as_array()
        .and_then(|items| {
            items.iter().find(|version| {
                let supports_minecraft =
                    version["game_versions"]
                        .as_array()
                        .is_some_and(|game_versions| {
                            game_versions
                                .iter()
                                .any(|item| item.as_str() == Some(minecraft_version))
                        });
                let supports_loader = if content_type == "mod" {
                    loader.is_some_and(|expected| {
                        version["loaders"].as_array().is_some_and(|loaders| {
                            loaders.iter().any(|item| item.as_str() == Some(expected))
                        })
                    })
                } else {
                    true
                };
                supports_minecraft && supports_loader
            })
        })
        .ok_or_else(|| ModpackError::NoCompatibleVersion(minecraft_version.to_owned()))?;
    let file = selected["files"]
        .as_array()
        .and_then(|files| {
            files
                .iter()
                .find(|file| file["primary"].as_bool() == Some(true))
                .or_else(|| files.first())
        })
        .ok_or(ModpackError::MissingDownload)?;
    let filename = file["filename"]
        .as_str()
        .ok_or(ModpackError::MissingDownload)?;
    if filename.is_empty() || filename.contains(['/', '\\']) {
        return Err(ModpackError::UnsafePath(filename.to_owned()));
    }
    if content_type == "mod" && !filename.ends_with(".jar") {
        return Err(ModpackError::MissingDownload);
    }
    let url = file["url"]
        .as_str()
        .filter(|url| url.starts_with("https://"))
        .ok_or(ModpackError::MissingDownload)?;
    instance
        .ensure_layout()
        .map_err(|error| ModpackError::Io(io::Error::other(error)))?;
    let destination = instance.root().join(folder).join(filename);
    let request = content_download_request(
        url,
        &destination,
        filename,
        file.pointer("/hashes/sha1").and_then(Value::as_str),
        file.pointer("/hashes/sha512").and_then(Value::as_str),
        file["size"].as_u64(),
    )?;
    download_one_with_progress(&client, request, progress)?;
    Ok(filename.to_owned())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModpackInstallSummary {
    pub project_id: String,
    pub name: String,
    pub version_name: String,
    pub minecraft_version: String,
    pub loader: String,
    pub downloaded_files: usize,
    pub override_files: usize,
    pub minecraft: InstallSummary,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModpackProgress {
    pub label: String,
    pub percent: f64,
    pub item_percent: f64,
    pub item_downloaded_bytes: u64,
    pub item_total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub completed_files: usize,
    pub total_files: usize,
    pub active_downloads: usize,
    pub bytes_per_second: u64,
}

impl ModpackProgress {
    fn stage(label: impl Into<String>, percent: f64) -> Self {
        Self {
            label: label.into(),
            percent,
            item_percent: 100.0,
            item_downloaded_bytes: 0,
            item_total_bytes: None,
            downloaded_bytes: 0,
            total_bytes: None,
            completed_files: 0,
            total_files: 0,
            active_downloads: 0,
            bytes_per_second: 0,
        }
    }

    fn transfer(progress: TransferProgress) -> Self {
        Self {
            label: progress.label,
            percent: progress.total_percent,
            item_percent: progress.item_percent,
            item_downloaded_bytes: progress.item_downloaded_bytes,
            item_total_bytes: progress.item_total_bytes,
            downloaded_bytes: progress.downloaded_bytes,
            total_bytes: progress.total_bytes,
            completed_files: progress.completed_files,
            total_files: progress.total_files,
            active_downloads: progress.active_downloads,
            bytes_per_second: progress.bytes_per_second,
        }
    }
}

/// Instala uma versão compatível do modpack diretamente na instância.
///
/// O arquivo mrpack é somente um índice. Cada arquivo é baixado do endereço
/// indicado pelo projeto e validado antes de receber o nome final na instância.
pub fn install_modrinth_modpack(
    instance: &Instance,
    project_id: &str,
    minecraft_version: &str,
) -> Result<ModpackInstallSummary, ModpackError> {
    install_modrinth_modpack_with_progress(instance, project_id, minecraft_version, None, |_| {})
}

pub fn install_modrinth_modpack_with_progress<F>(
    instance: &Instance,
    project_id: &str,
    minecraft_version: &str,
    forge_java: Option<&Path>,
    mut progress: F,
) -> Result<ModpackInstallSummary, ModpackError>
where
    F: FnMut(ModpackProgress),
{
    progress(ModpackProgress::stage(
        "Consultando versões do modpack",
        2.0,
    ));
    let client = client();
    let versions: Value = get_json(
        &client,
        &format!("{MODRINTH_API}/project/{project_id}/version"),
    )?;
    let selected = versions
        .as_array()
        .and_then(|versions| {
            versions.iter().find(|version| {
                version["game_versions"]
                    .as_array()
                    .is_some_and(|game_versions| {
                        game_versions
                            .iter()
                            .any(|game_version| game_version.as_str() == Some(minecraft_version))
                    })
                    && supported_project_loader(version).is_some()
            })
        })
        .ok_or_else(|| ModpackError::NoCompatibleVersion(minecraft_version.to_owned()))?;
    let pack_file = selected["files"]
        .as_array()
        .and_then(|files| {
            files
                .iter()
                .find(|file| {
                    file["primary"].as_bool() == Some(true)
                        && file["filename"]
                            .as_str()
                            .is_some_and(|name| name.ends_with(".mrpack"))
                })
                .or_else(|| {
                    files.iter().find(|file| {
                        file["filename"]
                            .as_str()
                            .is_some_and(|name| name.ends_with(".mrpack"))
                    })
                })
        })
        .ok_or(ModpackError::MissingDownload)?;
    let pack_url = pack_file["url"]
        .as_str()
        .ok_or(ModpackError::MissingDownload)?;
    let pack_filename = pack_file["filename"].as_str().unwrap_or("modpack.mrpack");
    let temporary_pack = instance.root().join(".aurora-modpack.mrpack");
    let pack_request = content_download_request(
        pack_url,
        &temporary_pack,
        pack_filename,
        pack_file.pointer("/hashes/sha1").and_then(Value::as_str),
        pack_file.pointer("/hashes/sha512").and_then(Value::as_str),
        pack_file["size"].as_u64(),
    )?;
    download_one_with_progress(&client, pack_request, |item| {
        progress(ModpackProgress::transfer(item.map_total(3.0, 15.0)))
    })?;
    progress(ModpackProgress::stage("Verificando o pacote", 15.0));

    let result = install_pack_archive(
        instance,
        &temporary_pack,
        &client,
        project_id,
        minecraft_version,
        selected,
        forge_java,
        &mut progress,
    )?;
    let _ = fs::remove_file(&temporary_pack);
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn install_pack_archive<F>(
    instance: &Instance,
    pack_path: &Path,
    client: &Client,
    project_id: &str,
    minecraft_version: &str,
    selected_version: &Value,
    forge_java: Option<&Path>,
    progress: &mut F,
) -> Result<ModpackInstallSummary, ModpackError>
where
    F: FnMut(ModpackProgress),
{
    instance
        .ensure_layout()
        .map_err(|error| ModpackError::Io(io::Error::other(error)))?;
    let mut archive = ZipArchive::new(File::open(pack_path)?)?;
    let mut index_contents = String::new();
    archive
        .by_name("modrinth.index.json")?
        .read_to_string(&mut index_contents)?;
    let index: Value =
        serde_json::from_str(&index_contents).map_err(|_| ModpackError::InvalidResponse)?;
    if index["formatVersion"].as_u64() != Some(1) {
        return Err(ModpackError::InvalidResponse);
    }
    let indexed_minecraft = index
        .pointer("/dependencies/minecraft")
        .and_then(Value::as_str)
        .ok_or(ModpackError::InvalidResponse)?;
    if indexed_minecraft != minecraft_version {
        return Err(ModpackError::NoCompatibleVersion(
            minecraft_version.to_owned(),
        ));
    }
    let dependencies = index["dependencies"]
        .as_object()
        .ok_or(ModpackError::InvalidResponse)?;
    let (loader, loader_version) =
        if let Some(version) = dependencies.get("fabric-loader").and_then(Value::as_str) {
            ("fabric", version)
        } else if let Some(version) = dependencies.get("forge").and_then(Value::as_str) {
            ("forge", version)
        } else {
            let unsupported = dependencies
                .keys()
                .find(|key| key.ends_with("loader"))
                .cloned()
                .unwrap_or_else(|| "desconhecido".to_owned());
            return Err(ModpackError::UnsupportedLoader(unsupported));
        };

    progress(ModpackProgress::stage(
        format!("Preparando Minecraft {minecraft_version} com {loader}"),
        18.0,
    ));
    let minecraft = {
        let mut minecraft_progress = |item: TransferProgress| {
            progress(ModpackProgress::transfer(item.map_total(18.0, 35.0)));
        };
        match loader {
            "fabric" => MinecraftInstaller::new().install_fabric_loader_with_progress(
                instance,
                minecraft_version,
                Some(loader_version),
                &mut minecraft_progress,
            )?,
            "forge" => MinecraftInstaller::new().install_forge_with_java_and_progress(
                instance,
                minecraft_version,
                loader_version,
                forge_java,
                &mut minecraft_progress,
            )?,
            other => return Err(ModpackError::UnsupportedLoader(other.to_owned())),
        }
    };
    progress(ModpackProgress::stage(
        "Minecraft e loader preparados",
        35.0,
    ));

    let files = index["files"]
        .as_array()
        .ok_or(ModpackError::InvalidResponse)?;
    let mut pack_downloads = Vec::with_capacity(files.len());
    for file in files {
        let relative =
            safe_relative_path(file["path"].as_str().ok_or(ModpackError::InvalidResponse)?)?;
        let url = file["downloads"]
            .as_array()
            .and_then(|downloads| {
                downloads
                    .iter()
                    .filter_map(Value::as_str)
                    .find(|url| url.starts_with("https://"))
            })
            .ok_or(ModpackError::MissingDownload)?;
        let label = relative.display().to_string();
        pack_downloads.push(content_download_request(
            url,
            &instance.root().join(&relative),
            &label,
            file.pointer("/hashes/sha1").and_then(Value::as_str),
            file.pointer("/hashes/sha512").and_then(Value::as_str),
            file["fileSize"].as_u64(),
        )?);
    }
    let batch = download_many(
        client,
        pack_downloads,
        DEFAULT_DOWNLOAD_CONCURRENCY,
        |item| progress(ModpackProgress::transfer(item.map_total(35.0, 95.0))),
    )?;
    let downloaded_files = batch.downloaded_files + batch.cached_files;

    progress(ModpackProgress::stage(
        "Aplicando configurações do modpack",
        97.0,
    ));
    let override_files = extract_overrides(&mut archive, instance.root())?;
    let name = index["name"]
        .as_str()
        .unwrap_or("Modpack Modrinth")
        .to_owned();
    let version_name = selected_version["name"]
        .as_str()
        .unwrap_or("versão do modpack")
        .to_owned();
    let marker = serde_json::json!({
        "source": "modrinth",
        "projectId": project_id,
        "name": name,
        "versionName": version_name,
        "minecraft": minecraft_version,
        "loader": loader,
        "versionId": minecraft.version_id,
    });
    fs::write(
        instance.root().join("aurora-modpack.json"),
        serde_json::to_vec_pretty(&marker).map_err(|_| ModpackError::InvalidResponse)?,
    )?;

    let summary = ModpackInstallSummary {
        project_id: project_id.to_owned(),
        name,
        version_name,
        minecraft_version: minecraft_version.to_owned(),
        loader: loader.to_owned(),
        downloaded_files,
        override_files,
        minecraft,
    };
    progress(ModpackProgress::stage("Instalação concluída", 100.0));
    Ok(summary)
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

fn get_json(client: &Client, url: &str) -> Result<Value, ModpackError> {
    Ok(client.get(url).send()?.error_for_status()?.json()?)
}

fn supported_project_loader(version: &Value) -> Option<&'static str> {
    let loaders = version["loaders"].as_array()?;
    if loaders
        .iter()
        .any(|loader| loader.as_str() == Some("fabric"))
    {
        Some("fabric")
    } else if loaders
        .iter()
        .any(|loader| loader.as_str() == Some("forge"))
    {
        Some("forge")
    } else {
        None
    }
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

fn content_download_request(
    url: &str,
    destination: &Path,
    label: &str,
    sha1: Option<&str>,
    sha512: Option<&str>,
    expected_size: Option<u64>,
) -> Result<DownloadRequest, ModpackError> {
    let expected_hash = sha512
        .map(|hash| ExpectedHash::Sha512(hash.to_owned()))
        .or_else(|| sha1.map(|hash| ExpectedHash::Sha1(hash.to_owned())))
        .ok_or_else(|| ModpackError::Integrity(destination.to_path_buf()))?;
    Ok(DownloadRequest {
        url: url.to_owned(),
        destination: destination.to_path_buf(),
        label: label.to_owned(),
        expected_hash: Some(expected_hash),
        expected_size,
    })
}

fn sha1_file(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut digest = Sha1::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let size = file.read(&mut buffer).ok()?;
        if size == 0 {
            break;
        }
        digest.update(&buffer[..size]);
    }
    Some(format!("{:x}", digest.finalize()))
}

fn extract_overrides(
    archive: &mut ZipArchive<File>,
    instance_root: &Path,
) -> Result<usize, ModpackError> {
    let mut extracted = 0usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().replace('\\', "/");
        let Some(relative) = name.strip_prefix("overrides/") else {
            continue;
        };
        if relative.is_empty() || entry.is_dir() {
            continue;
        }
        let relative = safe_relative_path(relative)?;
        let destination = instance_root.join(relative);
        let parent = destination
            .parent()
            .ok_or_else(|| ModpackError::UnsafePath(name))?;
        fs::create_dir_all(parent)?;
        let mut output = File::create(destination)?;
        io::copy(&mut entry, &mut output)?;
        extracted += 1;
    }
    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::safe_relative_path;

    #[test]
    fn rejects_modpack_path_traversal() {
        assert!(safe_relative_path("../outside.jar").is_err());
        assert!(safe_relative_path("C:\\outside.jar").is_err());
        assert!(safe_relative_path("mods/example.jar").is_ok());
    }
}
