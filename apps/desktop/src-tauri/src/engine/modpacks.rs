//! Catálogo e instalação segura de modpacks públicos do Modrinth.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::Url;
use serde::{Deserialize, Serialize};
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
const MAX_MRPACK_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_PACK_INDEX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PACK_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_PACK_TOTAL_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_OVERRIDE_FILES: usize = 10_000;
const MAX_OVERRIDE_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_OVERRIDE_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ModpackError {
    #[error("falha ao consultar Modrinth: {0}")]
    Network(#[from] reqwest::Error),
    #[error("falha ao baixar arquivo: {0}")]
    Download(#[from] DownloadError),
    #[error("resposta de catálogo ou manifesto inválida")]
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
    #[error("não foi possível preparar a atualização: {0}")]
    UpdatePreparation(String),
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthCategory {
    pub name: String,
    #[serde(alias = "project_type")]
    pub project_type: String,
    pub header: String,
}

fn validated_modrinth_category(category: Option<&str>) -> Result<Option<&str>, ModpackError> {
    let Some(category) = category.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if category.len() > 64
        || !category
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '+'))
    {
        return Err(ModpackError::InvalidResponse);
    }
    Ok(Some(category))
}

fn push_modrinth_category_facet(
    facets: &mut Vec<Vec<String>>,
    category: Option<&str>,
) -> Result<(), ModpackError> {
    if let Some(category) = validated_modrinth_category(category)? {
        facets.push(vec![format!("categories:{category}")]);
    }
    Ok(())
}

pub fn list_modrinth_categories(content_type: &str) -> Result<Vec<ModrinthCategory>, ModpackError> {
    if !matches!(content_type, "mod" | "modpack" | "resourcepack" | "shader") {
        return Err(ModpackError::InvalidResponse);
    }
    let mut categories = Client::builder()
        .user_agent("Aurora-Smart-Launcher/0.1")
        .timeout(Duration::from_secs(20))
        .build()
        .expect("cliente HTTP deve inicializar")
        .get(format!("{MODRINTH_API}/tag/category"))
        .send()?
        .error_for_status()?
        .json::<Vec<ModrinthCategory>>()?;
    categories.retain(|category| category.project_type == content_type);
    categories.sort_by(|left, right| {
        left.header
            .cmp(&right.header)
            .then_with(|| left.name.cmp(&right.name))
    });
    categories.dedup_by(|left, right| left.name == right.name);
    Ok(categories)
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
    minecraft_version: Option<&str>,
    loader: Option<&str>,
    category: Option<&str>,
    sort: &str,
    offset: u32,
    limit: u32,
) -> Result<ModrinthSearchPage, ModpackError> {
    let limit = limit.clamp(1, 50);
    let mut url = Url::parse(&format!("{MODRINTH_API}/search")).expect("URL Modrinth válida");
    let mut facets = vec![vec!["project_type:modpack".to_owned()]];
    if let Some(version) = minecraft_version.filter(|value| !value.trim().is_empty()) {
        facets.push(vec![format!("versions:{}", version.trim())]);
    }
    push_modrinth_category_facet(&mut facets, category)?;
    if let Some(loader) = loader.filter(|value| matches!(*value, "fabric" | "forge")) {
        facets.push(vec![format!("categories:{loader}")]);
    } else {
        facets.push(vec![
            "categories:fabric".to_owned(),
            "categories:forge".to_owned(),
        ]);
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
            .append_pair("limit", &limit.to_string())
            .append_pair("offset", &offset.to_string())
            .append_pair("index", index)
            .append_pair("facets", &facets);
    }
    let document: Value = Client::builder()
        .user_agent("Aurora-Smart-Launcher/0.1")
        .timeout(Duration::from_secs(20))
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
    category: Option<&str>,
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
    push_modrinth_category_facet(&mut facets, category)?;
    if content_type == "mod" {
        if let Some(loader) = loader.filter(|value| matches!(*value, "fabric" | "forge")) {
            facets.push(vec![format!("categories:{loader}")]);
        } else {
            facets.push(vec![
                "categories:fabric".to_owned(),
                "categories:forge".to_owned(),
            ]);
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
        .timeout(Duration::from_secs(20))
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
    mut progress: F,
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
    instance
        .ensure_layout()
        .map_err(|error| ModpackError::Io(io::Error::other(error)))?;
    let mut queue = VecDeque::from([(selected.clone(), true)]);
    let mut seen_versions = HashSet::new();
    let mut requests = Vec::new();
    let mut root_filename = None;
    while let Some((version, root)) = queue.pop_front() {
        let version_key = version["id"]
            .as_str()
            .or_else(|| version["version_number"].as_str())
            .unwrap_or_default()
            .to_owned();
        if !version_key.is_empty() && !seen_versions.insert(version_key) {
            continue;
        }
        if seen_versions.len() > 128 {
            return Err(ModpackError::InvalidResponse);
        }
        let file = primary_version_file(&version)?;
        let filename = file["filename"]
            .as_str()
            .ok_or(ModpackError::MissingDownload)?;
        if filename.is_empty() || filename.contains(['/', '\\']) {
            return Err(ModpackError::UnsafePath(filename.to_owned()));
        }
        let expected_extension = if root && content_type != "mod" {
            ".zip"
        } else {
            ".jar"
        };
        if !filename.to_ascii_lowercase().ends_with(expected_extension) {
            return Err(ModpackError::MissingDownload);
        }
        let url = file["url"]
            .as_str()
            .filter(|url| url.starts_with("https://"))
            .ok_or(ModpackError::MissingDownload)?;
        let target_folder = if root { folder } else { "mods" };
        requests.push(modrinth_download_request(
            url,
            &instance.root().join(target_folder).join(filename),
            filename,
            file.pointer("/hashes/sha1").and_then(Value::as_str),
            file.pointer("/hashes/sha512").and_then(Value::as_str),
            file["size"].as_u64(),
        )?);
        if root {
            root_filename = Some(filename.to_owned());
        }
        for dependency in version["dependencies"].as_array().into_iter().flatten() {
            if dependency["dependency_type"].as_str() != Some("required") {
                continue;
            }
            let dependency_version = if let Some(version_id) = dependency["version_id"].as_str() {
                get_json(&client, &format!("{MODRINTH_API}/version/{version_id}"))?
            } else if let Some(project_id) = dependency["project_id"].as_str() {
                let candidates = get_json(
                    &client,
                    &format!("{MODRINTH_API}/project/{project_id}/version"),
                )?;
                candidates
                    .as_array()
                    .and_then(|items| {
                        items.iter().find(|candidate| {
                            candidate["game_versions"]
                                .as_array()
                                .is_some_and(|versions| {
                                    versions
                                        .iter()
                                        .any(|value| value.as_str() == Some(minecraft_version))
                                })
                                && loader.is_none_or(|expected| {
                                    candidate["loaders"].as_array().is_some_and(|loaders| {
                                        loaders.iter().any(|value| value.as_str() == Some(expected))
                                    })
                                })
                        })
                    })
                    .cloned()
                    .ok_or_else(|| {
                        ModpackError::NoCompatibleVersion(minecraft_version.to_owned())
                    })?
            } else {
                return Err(ModpackError::InvalidResponse);
            };
            queue.push_back((dependency_version, false));
        }
    }
    download_many(
        &client,
        requests,
        DEFAULT_DOWNLOAD_CONCURRENCY,
        &mut progress,
    )?;
    root_filename.ok_or(ModpackError::MissingDownload)
}

fn primary_version_file(version: &Value) -> Result<&Value, ModpackError> {
    version["files"]
        .as_array()
        .and_then(|files| {
            files
                .iter()
                .find(|file| file["primary"].as_bool() == Some(true))
                .or_else(|| files.first())
        })
        .ok_or(ModpackError::MissingDownload)
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
    pub(crate) fn stage(label: impl Into<String>, percent: f64) -> Self {
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

    pub(crate) fn transfer(progress: TransferProgress) -> Self {
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
    progress: F,
) -> Result<ModpackInstallSummary, ModpackError>
where
    F: FnMut(ModpackProgress),
{
    install_modrinth_modpack_selected_with_progress(
        instance,
        project_id,
        minecraft_version,
        None,
        None,
        forge_java,
        |_| Ok(()),
        progress,
    )
}

/// Instala uma versão exata já revalidada pelo updater. Diferente do fluxo
/// inicial, este caminho nunca escolhe outro loader ou uma versão mais nova
/// entre a revisão do plano e a aplicação.
pub fn install_modrinth_modpack_version_with_progress<F>(
    instance: &Instance,
    project_id: &str,
    minecraft_version: &str,
    loader: &str,
    pack_version_id: &str,
    forge_java: Option<&Path>,
    progress: F,
) -> Result<ModpackInstallSummary, ModpackError>
where
    F: FnMut(ModpackProgress),
{
    install_modrinth_modpack_selected_with_progress(
        instance,
        project_id,
        minecraft_version,
        Some(loader),
        Some(pack_version_id),
        forge_java,
        |_| Ok(()),
        progress,
    )
}

/// Variante usada pelo atualizador transacional. O callback recebe todos os
/// caminhos que o pacote pode alterar depois que o mrpack foi validado, mas
/// antes de qualquer escrita na instância.
#[allow(clippy::too_many_arguments)]
pub fn install_modrinth_modpack_version_transactional_with_progress<F, P>(
    instance: &Instance,
    project_id: &str,
    minecraft_version: &str,
    loader: &str,
    pack_version_id: &str,
    forge_java: Option<&Path>,
    preflight: P,
    progress: F,
) -> Result<ModpackInstallSummary, ModpackError>
where
    F: FnMut(ModpackProgress),
    P: FnMut(&[PathBuf]) -> Result<(), ModpackError>,
{
    install_modrinth_modpack_selected_with_progress(
        instance,
        project_id,
        minecraft_version,
        Some(loader),
        Some(pack_version_id),
        forge_java,
        preflight,
        progress,
    )
}

#[allow(clippy::too_many_arguments)]
fn install_modrinth_modpack_selected_with_progress<F, P>(
    instance: &Instance,
    project_id: &str,
    minecraft_version: &str,
    required_loader: Option<&str>,
    pack_version_id: Option<&str>,
    forge_java: Option<&Path>,
    preflight: P,
    mut progress: F,
) -> Result<ModpackInstallSummary, ModpackError>
where
    F: FnMut(ModpackProgress),
    P: FnMut(&[PathBuf]) -> Result<(), ModpackError>,
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
                pack_version_id.is_none_or(|id| version["id"].as_str() == Some(id))
                    && version["version_type"].as_str() == Some("release")
                    && version["game_versions"]
                        .as_array()
                        .is_some_and(|game_versions| {
                            game_versions.iter().any(|game_version| {
                                game_version.as_str() == Some(minecraft_version)
                            })
                        })
                    && required_loader.map_or_else(
                        || supported_project_loader(version).is_some(),
                        |required| version_supports_loader(version, required),
                    )
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
    let pack_size = pack_file["size"]
        .as_u64()
        .filter(|size| *size > 0 && *size <= MAX_MRPACK_BYTES)
        .ok_or(ModpackError::InvalidResponse)?;
    let temporary_pack = instance.root().join(".aurora-modpack.mrpack");
    let pack_request = modrinth_download_request(
        pack_url,
        &temporary_pack,
        pack_filename,
        pack_file.pointer("/hashes/sha1").and_then(Value::as_str),
        pack_file.pointer("/hashes/sha512").and_then(Value::as_str),
        Some(pack_size),
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
        required_loader,
        forge_java,
        preflight,
        &mut progress,
    );
    let _ = fs::remove_file(&temporary_pack);
    result
}

#[allow(clippy::too_many_arguments)]
fn install_pack_archive<F, P>(
    instance: &Instance,
    pack_path: &Path,
    client: &Client,
    project_id: &str,
    minecraft_version: &str,
    selected_version: &Value,
    required_loader: Option<&str>,
    forge_java: Option<&Path>,
    mut preflight: P,
    progress: &mut F,
) -> Result<ModpackInstallSummary, ModpackError>
where
    F: FnMut(ModpackProgress),
    P: FnMut(&[PathBuf]) -> Result<(), ModpackError>,
{
    let mut archive = ZipArchive::new(File::open(pack_path)?)?;
    let index = read_pack_index(&mut archive)?;
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
    if required_loader.is_some_and(|required| required != loader) {
        return Err(ModpackError::UnsupportedLoader(loader.to_owned()));
    }

    let plan = client_pack_plan(&mut archive, &index, instance.root())?;
    preflight(&plan.managed_paths)?;
    instance
        .ensure_layout()
        .map_err(|error| ModpackError::Io(io::Error::other(error)))?;

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

    let batch = download_many(
        client,
        plan.downloads,
        DEFAULT_DOWNLOAD_CONCURRENCY,
        |item| progress(ModpackProgress::transfer(item.map_total(35.0, 95.0))),
    )?;
    let downloaded_files = batch.downloaded_files + batch.cached_files;

    progress(ModpackProgress::stage(
        "Aplicando configurações do modpack",
        97.0,
    ));
    let override_files = extract_overrides(&mut archive, instance.root(), &plan.overrides)?;
    let name = index["name"]
        .as_str()
        .unwrap_or("Modpack Modrinth")
        .to_owned();
    let version_name = selected_version["name"]
        .as_str()
        .unwrap_or("versão do modpack")
        .to_owned();
    let pack_version_id = selected_version["id"]
        .as_str()
        .ok_or(ModpackError::InvalidResponse)?;
    let version_number = selected_version["version_number"]
        .as_str()
        .ok_or(ModpackError::InvalidResponse)?;
    let owned_files = plan
        .managed_paths
        .iter()
        .filter(|relative| relative.as_path() != Path::new("aurora-modpack.json"))
        .map(|relative| {
            let path = instance.root().join(relative);
            let metadata = fs::metadata(&path)?;
            if !metadata.is_file() {
                return Err(ModpackError::Integrity(path));
            }
            let sha1 = sha1_file(&path).ok_or_else(|| ModpackError::Integrity(path.clone()))?;
            let relative = relative
                .to_str()
                .map(|value| value.replace('\\', "/"))
                .ok_or_else(|| ModpackError::UnsafePath(relative.display().to_string()))?;
            Ok(serde_json::json!({
                "path": relative,
                "sha1": sha1,
                "size": metadata.len(),
            }))
        })
        .collect::<Result<Vec<_>, ModpackError>>()?;
    let marker = serde_json::json!({
        "source": "modrinth",
        "projectId": project_id,
        "packVersionId": pack_version_id,
        "versionNumber": version_number,
        "name": name,
        "versionName": version_name,
        "minecraft": minecraft_version,
        "loader": loader,
        "versionId": minecraft.version_id,
        "ownedFiles": owned_files,
    });
    let marker = serde_json::to_vec_pretty(&marker).map_err(|_| ModpackError::InvalidResponse)?;
    write_pack_file(
        instance.root(),
        Path::new("aurora-modpack.json"),
        &marker[..],
        marker.len() as u64,
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
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !safe_redirect_target(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .expect("cliente HTTP deve inicializar")
}

fn safe_redirect_target(url: &Url) -> bool {
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    url.host_str()
        .map(str::to_ascii_lowercase)
        .is_some_and(|host| {
            host == "cdn.modrinth.com"
                || host.ends_with(".modrinth.com")
                || host == "aurora-api.aurora-edge-proxy.workers.dev"
        })
}

fn modrinth_download_request(
    url: &str,
    destination: &Path,
    label: &str,
    sha1: Option<&str>,
    sha512: Option<&str>,
    expected_size: Option<u64>,
) -> Result<DownloadRequest, ModpackError> {
    let parsed = modrinth_url(url).ok_or(ModpackError::MissingDownload)?;
    content_download_request(
        parsed.as_str(),
        destination,
        label,
        sha1,
        sha512,
        expected_size,
    )
}

fn modrinth_url(value: &str) -> Option<Url> {
    let url = Url::parse(value).ok()?;
    (safe_redirect_target(&url) && url.host_str()?.ends_with(".modrinth.com")).then_some(url)
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

fn version_supports_loader(version: &Value, loader: &str) -> bool {
    matches!(loader, "fabric" | "forge")
        && version["loaders"]
            .as_array()
            .is_some_and(|loaders| loaders.iter().any(|item| item.as_str() == Some(loader)))
}

fn safe_relative_path(value: &str) -> Result<PathBuf, ModpackError> {
    let path = Path::new(value);
    if path.is_absolute()
        || value.is_empty()
        || path.components().any(|component| match component {
            Component::Normal(name) => name
                .to_str()
                .is_none_or(|name| !windows_component_safe(name)),
            _ => true,
        })
    {
        return Err(ModpackError::UnsafePath(value.to_owned()));
    }
    Ok(path.to_path_buf())
}

fn windows_component_safe(value: &str) -> bool {
    if value.is_empty()
        || value.ends_with(['.', ' '])
        || value
            .chars()
            .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
    {
        return false;
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

fn safe_modpack_install_path(value: &str) -> Result<PathBuf, ModpackError> {
    let normalized = value.replace('\\', "/");
    let path: PathBuf = safe_relative_path(&normalized)?.components().collect();
    let first = path
        .components()
        .next()
        .and_then(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .ok_or_else(|| ModpackError::UnsafePath(value.to_owned()))?
        .to_ascii_lowercase();
    if matches!(
        first.as_str(),
        ".aurora"
            | ".aurora-modpack.mrpack"
            | "aurora-modpack.json"
            | "assets"
            | "libraries"
            | "versions"
            | "natives"
            | "saves"
            | "screenshots"
            | "logs"
            | "crash-reports"
            | "backups"
            | "replay_recordings"
    ) {
        return Err(ModpackError::UnsafePath(value.to_owned()));
    }
    Ok(path.components().collect())
}

fn read_pack_index(archive: &mut ZipArchive<File>) -> Result<Value, ModpackError> {
    let entry = archive.by_name("modrinth.index.json")?;
    if entry.size() > MAX_PACK_INDEX_BYTES {
        return Err(ModpackError::InvalidResponse);
    }
    let mut bytes = Vec::new();
    entry
        .take(MAX_PACK_INDEX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PACK_INDEX_BYTES {
        return Err(ModpackError::InvalidResponse);
    }
    let index: Value = serde_json::from_slice(&bytes).map_err(|_| ModpackError::InvalidResponse)?;
    if index["formatVersion"].as_u64() != Some(1) {
        return Err(ModpackError::InvalidResponse);
    }
    Ok(index)
}

struct ClientPackPlan {
    downloads: Vec<DownloadRequest>,
    overrides: Vec<(PathBuf, usize)>,
    managed_paths: Vec<PathBuf>,
}

enum PackSource {
    Download(DownloadRequest),
    CommonOverride(usize),
    ClientOverride(usize),
}

impl PackSource {
    fn layer(&self) -> u8 {
        match self {
            Self::Download(_) => 0,
            Self::CommonOverride(_) => 1,
            Self::ClientOverride(_) => 2,
        }
    }
}

fn client_file_enabled(file: &Value) -> Result<bool, ModpackError> {
    let side = match file.get("env") {
        Some(Value::Object(env)) => env.get("client"),
        None | Some(Value::Null) => None,
        _ => return Err(ModpackError::InvalidResponse),
    };
    match side {
        None => Ok(true),
        // The existing one-click installer includes compatible optional files.
        Some(Value::String(value)) if value == "required" || value == "optional" => Ok(true),
        Some(Value::String(value)) if value == "unsupported" => Ok(false),
        _ => Err(ModpackError::InvalidResponse),
    }
}

fn add_pack_source(
    files: &mut BTreeMap<String, (PathBuf, PackSource)>,
    path: PathBuf,
    source: PackSource,
) -> Result<(), ModpackError> {
    let key = path.to_string_lossy().to_lowercase();
    if let Some((existing_path, existing)) = files.get(&key) {
        if path != *existing_path || source.layer() == existing.layer() {
            return Err(ModpackError::UnsafePath(path.display().to_string()));
        }
        if source.layer() < existing.layer() {
            return Ok(());
        }
    }
    files.insert(key, (path, source));
    Ok(())
}

fn client_pack_plan(
    archive: &mut ZipArchive<File>,
    index: &Value,
    root: &Path,
) -> Result<ClientPackPlan, ModpackError> {
    let mut files = BTreeMap::new();
    let mut total_download_bytes = 0u64;
    for file in index["files"]
        .as_array()
        .ok_or(ModpackError::InvalidResponse)?
    {
        if !client_file_enabled(file)? {
            continue;
        }
        let relative =
            safe_modpack_install_path(file["path"].as_str().ok_or(ModpackError::InvalidResponse)?)?;
        let url = file["downloads"]
            .as_array()
            .and_then(|urls| urls.iter().filter_map(Value::as_str).find_map(modrinth_url))
            .ok_or(ModpackError::MissingDownload)?;
        let size = file["fileSize"]
            .as_u64()
            .filter(|size| *size <= MAX_PACK_FILE_BYTES)
            .ok_or(ModpackError::InvalidResponse)?;
        total_download_bytes = total_download_bytes
            .checked_add(size)
            .filter(|total| *total <= MAX_PACK_TOTAL_BYTES)
            .ok_or(ModpackError::InvalidResponse)?;
        let request = modrinth_download_request(
            url.as_str(),
            &root.join(&relative),
            &relative.to_string_lossy(),
            file.pointer("/hashes/sha1").and_then(Value::as_str),
            file.pointer("/hashes/sha512").and_then(Value::as_str),
            Some(size),
        )?;
        add_pack_source(&mut files, relative, PackSource::Download(request))?;
    }
    let mut override_count = 0;
    let mut override_bytes = 0u64;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().replace('\\', "/");
        let Some((layer, path)) = name.split_once('/') else {
            continue;
        };
        if entry.is_dir() || path.is_empty() {
            continue;
        }
        let source = match layer {
            "overrides" => PackSource::CommonOverride(i),
            "client-overrides" => PackSource::ClientOverride(i),
            _ => continue,
        };
        if entry.size() > MAX_OVERRIDE_FILE_BYTES || override_count >= MAX_OVERRIDE_FILES {
            return Err(ModpackError::InvalidResponse);
        }
        override_count += 1;
        override_bytes = override_bytes
            .checked_add(entry.size())
            .filter(|total| *total <= MAX_OVERRIDE_TOTAL_BYTES)
            .ok_or(ModpackError::InvalidResponse)?;
        add_pack_source(&mut files, safe_modpack_install_path(path)?, source)?;
    }

    let mut plan = ClientPackPlan {
        downloads: Vec::new(),
        overrides: Vec::new(),
        managed_paths: Vec::with_capacity(files.len() + 1),
    };
    for (path, _) in files.values() {
        if path
            .ancestors()
            .skip(1)
            .any(|parent| files.contains_key(&parent.to_string_lossy().to_lowercase()))
        {
            return Err(ModpackError::UnsafePath(path.display().to_string()));
        }
        check_pack_destination(root, path)?;
    }
    for (path, source) in files.into_values() {
        plan.managed_paths.push(path.clone());
        match source {
            PackSource::Download(request) => plan.downloads.push(request),
            PackSource::CommonOverride(i) | PackSource::ClientOverride(i) => {
                plan.overrides.push((path, i))
            }
        }
    }
    let marker = PathBuf::from("aurora-modpack.json");
    check_pack_destination(root, &marker)?;
    plan.managed_paths.push(marker);
    plan.managed_paths.sort();
    Ok(plan)
}

fn check_pack_destination(root: &Path, relative: &Path) -> Result<(), ModpackError> {
    super::content_updates::ensure_no_symlink_components(root, &relative.to_string_lossy())
        .map_err(|error| ModpackError::UpdatePreparation(error.to_string()))
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
    overrides: &[(PathBuf, usize)],
) -> Result<usize, ModpackError> {
    for (relative, index) in overrides {
        let entry = archive.by_index(*index)?;
        let expected = entry.size();
        write_pack_file(instance_root, relative, entry, expected)?;
    }
    Ok(overrides.len())
}

fn write_pack_file(
    root: &Path,
    relative: &Path,
    source: impl Read,
    expected: u64,
) -> Result<(), ModpackError> {
    check_pack_destination(root, relative)?;
    let destination = root.join(relative);
    let parent = destination
        .parent()
        .ok_or_else(|| ModpackError::UnsafePath(relative.display().to_string()))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".aurora-{}.tmp", uuid::Uuid::new_v4()));
    let mut output = File::create_new(&temporary)?;
    let result = (|| {
        if io::copy(&mut source.take(expected + 1), &mut output)? != expected {
            return Err(ModpackError::InvalidResponse);
        }
        output.sync_all()?;
        drop(output);
        check_pack_destination(root, relative)?;
        super::launch_settings::atomic_replace(&temporary, &destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod client_pack_tests;

#[cfg(test)]
mod tests {
    use super::{
        list_modrinth_categories, push_modrinth_category_facet, safe_modpack_install_path,
        safe_relative_path, validated_modrinth_category, ModrinthCategory,
    };

    #[test]
    fn rejects_modpack_path_traversal() {
        assert!(safe_relative_path("../outside.jar").is_err());
        assert!(safe_relative_path("C:\\outside.jar").is_err());
        assert!(safe_relative_path("mods/example.jar").is_ok());
    }

    #[test]
    fn refuses_pack_targets_in_user_or_runtime_directories() {
        for path in [
            "saves/world/level.dat",
            "screenshots/image.png",
            "logs/latest.log",
            "crash-reports/crash.txt",
            "versions/1.20.1/profile.json",
            ".aurora/private.json",
        ] {
            assert!(safe_modpack_install_path(path).is_err(), "{path}");
        }
        assert!(safe_modpack_install_path("mods/example.jar").is_ok());
        assert!(safe_modpack_install_path("config/example.toml").is_ok());
    }

    #[test]
    fn validates_modrinth_category_facets() {
        assert_eq!(
            validated_modrinth_category(Some("adventure")).unwrap(),
            Some("adventure")
        );
        assert_eq!(
            validated_modrinth_category(Some("512x+")).unwrap(),
            Some("512x+")
        );
        assert!(validated_modrinth_category(Some("adventure\"],[\"versions:1.0")).is_err());
        let mut facets = vec![vec!["project_type:modpack".to_owned()]];
        push_modrinth_category_facet(&mut facets, Some("adventure")).unwrap();
        assert_eq!(facets[1], vec!["categories:adventure"]);
    }

    #[test]
    fn reads_modrinth_category_taxonomy_shape() {
        let category: ModrinthCategory = serde_json::from_str(
            r#"{"name":"adventure","project_type":"modpack","header":"categories"}"#,
        )
        .unwrap();
        assert_eq!(category.name, "adventure");
        assert_eq!(category.project_type, "modpack");
    }

    #[test]
    #[ignore = "faz uma chamada real ao catálogo Modrinth"]
    fn reads_live_modrinth_categories() {
        let categories = list_modrinth_categories("modpack").unwrap();
        assert!(categories
            .iter()
            .any(|category| category.name == "adventure"));
    }
}
