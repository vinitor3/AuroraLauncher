use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use aurora_launcher_core::{
    auth::{offline_uuid_for_nickname, validate_nickname},
    engine::{
        discover_java_executables, ensure_managed_java_with_progress, install_embedded_companion,
        install_embedded_core, install_modrinth_content_with_progress as install_modrinth_project,
        install_modrinth_modpack_with_progress as install_modrinth_archive,
        install_remote_content_with_progress as install_remote_project, required_java_major,
        resolve_launch_spec, resolve_modrinth_content_artwork,
        search_modrinth_content as search_modrinth_catalog_content,
        search_modrinth_modpacks as search_modrinth_catalog, InstallSummary, InstanceId,
        InstanceLayout, IpcEvent, IpcServer, IpcSessionProfile, JavaRuntime, LaunchIdentity,
        LauncherEngine, MinecraftInstaller, ModpackInstallSummary, ModpackProgress,
        ModrinthContent, ModrinthSearchPage, ResolvedContentArtwork, SpeechResult,
        TransferProgress,
    },
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::{
    webview::DownloadEvent, AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
};
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    data_directory: String,
    ready: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSummary {
    id: String,
    path: String,
    has_mods_directory: bool,
    has_installed_version: bool,
    display_name: Option<String>,
    icon_url: Option<String>,
    project_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceContentSummary {
    mods: Vec<InstanceContentFile>,
    shaderpacks: Vec<InstanceContentFile>,
    resourcepacks: Vec<InstanceContentFile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceContentFile {
    name: String,
    enabled: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManualDownloadEvent {
    instance_id: String,
    filename: String,
    status: String,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceLogSummary {
    filename: String,
    lines: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceLaunchProfile {
    version_id: Option<String>,
    minecraft_version: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaRuntimeSummary {
    executable: String,
    version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceImageSummary {
    url: String,
    data_base64: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSummary {
    process_id: u32,
    version_id: String,
    core_installed: bool,
    companion_installed: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningInstanceSummary {
    instance_id: String,
    process_id: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcSessionEvent {
    process_id: u32,
    event: IpcEvent,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressEvent {
    label: String,
    percent: f64,
    total_percent: f64,
    item_percent: f64,
    item_downloaded_bytes: u64,
    item_total_bytes: Option<u64>,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    completed_files: usize,
    total_files: usize,
    active_downloads: usize,
    bytes_per_second: u64,
}

fn emit_download_progress(app: &AppHandle, label: impl Into<String>, percent: f64) {
    emit_download_event(
        app,
        DownloadProgressEvent {
            label: label.into(),
            percent: percent.clamp(0.0, 100.0),
            total_percent: percent.clamp(0.0, 100.0),
            item_percent: 0.0,
            item_downloaded_bytes: 0,
            item_total_bytes: None,
            downloaded_bytes: 0,
            total_bytes: None,
            completed_files: 0,
            total_files: 0,
            active_downloads: 0,
            bytes_per_second: 0,
        },
    );
}

fn emit_transfer_progress(app: &AppHandle, progress: TransferProgress) {
    emit_download_event(
        app,
        DownloadProgressEvent {
            label: progress.label,
            percent: progress.total_percent,
            total_percent: progress.total_percent,
            item_percent: progress.item_percent,
            item_downloaded_bytes: progress.item_downloaded_bytes,
            item_total_bytes: progress.item_total_bytes,
            downloaded_bytes: progress.downloaded_bytes,
            total_bytes: progress.total_bytes,
            completed_files: progress.completed_files,
            total_files: progress.total_files,
            active_downloads: progress.active_downloads,
            bytes_per_second: progress.bytes_per_second,
        },
    );
}

fn emit_modpack_progress(app: &AppHandle, progress: ModpackProgress) {
    emit_download_event(
        app,
        DownloadProgressEvent {
            label: progress.label,
            percent: progress.percent,
            total_percent: progress.percent,
            item_percent: progress.item_percent,
            item_downloaded_bytes: progress.item_downloaded_bytes,
            item_total_bytes: progress.item_total_bytes,
            downloaded_bytes: progress.downloaded_bytes,
            total_bytes: progress.total_bytes,
            completed_files: progress.completed_files,
            total_files: progress.total_files,
            active_downloads: progress.active_downloads,
            bytes_per_second: progress.bytes_per_second,
        },
    );
}

fn emit_download_event(app: &AppHandle, progress: DownloadProgressEvent) {
    let _ = app.emit("aurora-download-progress", progress);
}

#[derive(Default)]
pub struct IpcSessions(Mutex<BTreeMap<u32, IpcServer>>);

struct RunningInstance {
    instance_id: String,
    child: Child,
}

#[derive(Default)]
pub struct RunningInstances(Mutex<BTreeMap<u32, RunningInstance>>);

/// Configuração pública do Firebase, guardada fora do código empacotado.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirebasePublicConfig {
    api_key: String,
    auth_domain: String,
    project_id: String,
    storage_bucket: String,
    messaging_sender_id: String,
    app_id: String,
    worker_url: Option<String>,
}

fn data_directory(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("não foi possível localizar os dados do Aurora: {error}"))
}

fn layout(app: &AppHandle) -> Result<InstanceLayout, String> {
    Ok(InstanceLayout::new(data_directory(app)?))
}

fn instance_presentation(path: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let Ok(contents) = fs::read(path.join("aurora-modpack.json")) else {
        return (None, None, None);
    };
    let Ok(document) = serde_json::from_slice::<serde_json::Value>(&contents) else {
        return (None, None, None);
    };
    let display_name = document["name"].as_str().map(str::to_owned);
    let icon_url = document["iconUrl"]
        .as_str()
        .filter(|url| url.starts_with("https://") && url.len() <= 2_048)
        .map(str::to_owned);
    let project_id = document["projectId"].as_str().map(str::to_owned);
    (display_name, icon_url, project_id)
}

fn summarize_instance(id: String, path: PathBuf) -> InstanceSummary {
    let has_installed_version = path
        .join("versions")
        .read_dir()
        .ok()
        .is_some_and(|entries| {
            entries.filter_map(Result::ok).any(|version| {
                let version_id = version.file_name().to_string_lossy().into_owned();
                version.path().join(format!("{version_id}.json")).is_file()
            })
        });
    let (display_name, icon_url, project_id) = instance_presentation(&path);
    InstanceSummary {
        id,
        has_mods_directory: path.join("mods").is_dir(),
        has_installed_version,
        display_name,
        icon_url,
        project_id,
        path: path.display().to_string(),
    }
}

fn prune_finished_processes(processes: &mut BTreeMap<u32, RunningInstance>) -> Vec<u32> {
    let finished = processes
        .iter_mut()
        .filter_map(|(process_id, process)| match process.child.try_wait() {
            Ok(Some(_)) | Err(_) => Some(*process_id),
            Ok(None) => None,
        })
        .collect::<Vec<_>>();
    for process_id in &finished {
        processes.remove(process_id);
    }
    finished
}

fn instance_is_running(running: &RunningInstances, instance_id: &str) -> Result<bool, String> {
    let mut processes = running
        .0
        .lock()
        .map_err(|_| "não foi possível consultar os jogos em execução".to_owned())?;
    prune_finished_processes(&mut processes);
    Ok(processes
        .values()
        .any(|process| process.instance_id == instance_id))
}

fn firebase_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_directory(app)?.join("firebase-public-config.json"))
}

fn write_instance_profile(
    instance: &aurora_launcher_core::engine::Instance,
    result: &InstallSummary,
    source: &str,
) -> Result<(), String> {
    instance
        .ensure_layout()
        .map_err(|error| error.to_string())?;
    let profile = serde_json::json!({
        "source": source,
        "minecraft": result.minecraft_version,
        "versionId": result.version_id,
    });
    fs::write(
        instance.root().join("aurora-instance.json"),
        serde_json::to_vec_pretty(&profile)
            .map_err(|error| format!("não foi possível salvar o perfil da instância: {error}"))?,
    )
    .map_err(|error| format!("não foi possível salvar o perfil da instância: {error}"))
}

#[tauri::command]
pub fn engine_status(app: AppHandle) -> Result<EngineStatus, String> {
    let data_directory = data_directory(&app)?;
    fs::create_dir_all(&data_directory)
        .map_err(|error| format!("não foi possível criar os dados do Aurora: {error}"))?;

    Ok(EngineStatus {
        data_directory: data_directory.display().to_string(),
        ready: true,
    })
}

#[tauri::command]
pub fn list_instances(app: AppHandle) -> Result<Vec<InstanceSummary>, String> {
    let layout = layout(&app)?;
    let instances_dir = layout.instances_dir();
    if !instances_dir.exists() {
        return Ok(Vec::new());
    }

    let mut instances = Vec::new();
    for entry in fs::read_dir(&instances_dir)
        .map_err(|error| format!("não foi possível ler as instâncias: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("não foi possível ler uma instância: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("não foi possível validar uma instância: {error}"))?;
        if !file_type.is_dir() {
            continue;
        }

        let id = entry.file_name().to_string_lossy().into_owned();
        if InstanceId::parse(id.clone()).is_err() {
            continue;
        }
        instances.push(summarize_instance(id, entry.path()));
    }
    instances.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(instances)
}

#[tauri::command]
pub fn create_instance(app: AppHandle, id: String) -> Result<InstanceSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    instance
        .ensure_layout()
        .map_err(|error| error.to_string())?;

    Ok(summarize_instance(
        instance.id().as_str().to_owned(),
        instance.root().to_path_buf(),
    ))
}

#[tauri::command]
pub fn rename_instance(
    app: AppHandle,
    running: tauri::State<RunningInstances>,
    id: String,
    new_id: String,
) -> Result<InstanceSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let new_id = InstanceId::parse(new_id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de renomear esta instância".to_owned());
    }
    let layout = layout(&app)?;
    let source = layout.path_for(&id);
    if !source.is_dir() {
        return Err("a instância não existe mais".to_owned());
    }
    if id == new_id {
        return Ok(summarize_instance(id.as_str().to_owned(), source));
    }
    let destination = layout.path_for(&new_id);
    if destination.exists() {
        return Err("já existe uma instância com esse nome".to_owned());
    }
    fs::rename(&source, &destination)
        .map_err(|error| format!("não foi possível renomear a instância: {error}"))?;
    Ok(summarize_instance(new_id.as_str().to_owned(), destination))
}

#[tauri::command]
pub fn set_instance_presentation(
    app: AppHandle,
    id: String,
    display_name: String,
    icon_url: Option<String>,
) -> Result<InstanceSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let display_name = display_name.trim();
    if display_name.is_empty()
        || display_name.len() > 160
        || display_name.chars().any(char::is_control)
    {
        return Err("o nome público do modpack é inválido".to_owned());
    }
    let icon_url = icon_url
        .map(|url| url.trim().to_owned())
        .filter(|url| !url.is_empty());
    if icon_url
        .as_deref()
        .is_some_and(|url| !url.starts_with("https://") || url.len() > 2_048)
    {
        return Err("a imagem do modpack precisa usar uma URL HTTPS".to_owned());
    }
    let instance = layout(&app)?.open(id);
    if !instance.root().is_dir() {
        return Err("a instância não existe mais".to_owned());
    }
    let marker_path = instance.root().join("aurora-modpack.json");
    let mut document = fs::read(&marker_path)
        .ok()
        .and_then(|contents| serde_json::from_slice::<serde_json::Value>(&contents).ok())
        .unwrap_or_else(|| serde_json::json!({ "source": "modrinth" }));
    let object = document
        .as_object_mut()
        .ok_or_else(|| "os metadados do modpack estão corrompidos".to_owned())?;
    object.insert("name".to_owned(), serde_json::json!(display_name));
    if let Some(icon_url) = icon_url {
        object.insert("iconUrl".to_owned(), serde_json::json!(icon_url));
    }
    let temporary = marker_path.with_extension("json.aurora-writing");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&document)
            .map_err(|error| format!("não foi possível preparar os metadados: {error}"))?,
    )
    .map_err(|error| format!("não foi possível preparar os metadados: {error}"))?;
    if marker_path.exists() {
        fs::remove_file(&marker_path)
            .map_err(|error| format!("não foi possível atualizar os metadados: {error}"))?;
    }
    fs::rename(&temporary, &marker_path)
        .map_err(|error| format!("não foi possível concluir os metadados: {error}"))?;
    Ok(summarize_instance(
        instance.id().as_str().to_owned(),
        instance.root().to_path_buf(),
    ))
}

#[tauri::command]
pub fn delete_instance(
    app: AppHandle,
    running: tauri::State<RunningInstances>,
    id: String,
) -> Result<(), String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de excluir esta instância".to_owned());
    }
    let instance = layout(&app)?.open(id);
    if !instance.root().exists() {
        return Ok(());
    }
    fs::remove_dir_all(instance.root())
        .map_err(|error| format!("não foi possível excluir a instância: {error}"))
}

/// Baixa arquivos oficiais do Minecraft fora da thread da interface.
#[tauri::command]
pub async fn install_vanilla(
    app: AppHandle,
    id: String,
    minecraft_version: String,
) -> Result<InstallSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    emit_download_progress(
        &app,
        format!("Preparando Minecraft {minecraft_version}"),
        1.0,
    );
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = MinecraftInstaller::new()
            .install_vanilla_with_progress(&instance, &minecraft_version, |progress| {
                emit_transfer_progress(&progress_app, progress);
            })
            .map_err(|error| error.to_string())?;
        write_instance_profile(&instance, &result, "vanilla")?;
        emit_download_progress(&progress_app, "Minecraft instalado", 100.0);
        Ok(result)
    })
    .await
    .map_err(|error| format!("a tarefa de instalação falhou: {error}"))?
}

#[tauri::command]
pub async fn install_fabric(
    app: AppHandle,
    id: String,
    minecraft_version: String,
) -> Result<InstallSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    emit_download_progress(
        &app,
        format!("Preparando Fabric para {minecraft_version}"),
        1.0,
    );
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = MinecraftInstaller::new()
            .install_fabric_loader_with_progress(&instance, &minecraft_version, None, |progress| {
                emit_transfer_progress(&progress_app, progress);
            })
            .map_err(|error| error.to_string())?;
        write_instance_profile(&instance, &result, "fabric")?;
        emit_download_progress(&progress_app, "Fabric instalado", 100.0);
        Ok(result)
    })
    .await
    .map_err(|error| format!("a tarefa de instalação falhou: {error}"))?
}

/// Instala o Forge oficial na versão solicitada, usando o Java compatível.
#[tauri::command]
pub async fn install_forge(
    app: AppHandle,
    id: String,
    minecraft_version: String,
    forge_version: String,
) -> Result<InstallSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    let runtime_data_directory = data_directory(&app)?;
    emit_download_progress(&app, format!("Preparando Forge {forge_version}"), 1.0);
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let java = ensure_managed_java_with_progress(
            &runtime_data_directory,
            required_java_major(&minecraft_version),
            |mut progress| {
                progress.total_percent *= 0.1;
                emit_transfer_progress(&progress_app, progress);
            },
        )
        .map_err(|error| error.to_string())?;
        let result = MinecraftInstaller::new()
            .install_forge_with_java_and_progress(
                &instance,
                &minecraft_version,
                &forge_version,
                Some(java.executable()),
                |mut progress| {
                    progress.total_percent = 10.0 + progress.total_percent * 0.9;
                    emit_transfer_progress(&progress_app, progress);
                },
            )
            .map_err(|error| error.to_string())?;
        write_instance_profile(&instance, &result, "forge")?;
        emit_download_progress(&progress_app, "Forge instalado", 100.0);
        Ok(result)
    })
    .await
    .map_err(|error| format!("a instalação do Forge falhou: {error}"))?
}

#[tauri::command]
pub async fn search_modrinth_modpacks(
    query: String,
    offset: u32,
    limit: u32,
) -> Result<ModrinthSearchPage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        search_modrinth_catalog(&query, offset, limit).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("a busca de modpacks falhou: {error}"))?
}

#[tauri::command]
pub async fn search_modrinth_content(
    query: String,
    content_type: String,
    minecraft_version: Option<String>,
    loader: Option<String>,
    sort: String,
) -> Result<Vec<ModrinthContent>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        search_modrinth_catalog_content(
            &query,
            &content_type,
            minecraft_version.as_deref(),
            loader.as_deref(),
            &sort,
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("a busca no Modrinth falhou: {error}"))?
}

#[tauri::command]
pub async fn install_remote_content(
    app: AppHandle,
    id: String,
    url: String,
    filename: String,
    content_type: String,
    sha1: Option<String>,
) -> Result<String, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    emit_download_progress(&app, format!("Baixando {filename}"), 1.0);
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = install_remote_project(
            &instance,
            &url,
            &filename,
            &content_type,
            sha1.as_deref(),
            |progress| emit_transfer_progress(&progress_app, progress),
        )
        .map_err(|error| error.to_string())?;
        emit_download_progress(&progress_app, format!("Instalado: {result}"), 100.0);
        Ok(result)
    })
    .await
    .map_err(|error| format!("a instalação do conteúdo falhou: {error}"))?
}

/// Abre a página oficial de um arquivo bloqueado em uma janela do Aurora. A
/// própria página cumpre a contagem regressiva; o WebView apenas captura o
/// download, valida o SHA-1 publicado e o instala na instância.
#[tauri::command]
pub async fn open_manual_content_download(
    app: AppHandle,
    id: String,
    page_url: String,
    filename: String,
    content_type: String,
    sha1: String,
) -> Result<(), String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance_id = id.as_str().to_owned();
    let instance = layout(&app)?.open(id);
    instance
        .ensure_layout()
        .map_err(|error| error.to_string())?;
    let folder = content_folder(&content_type)?;
    validate_content_filename(&filename, &content_type)?;
    if sha1.len() != 40 || !sha1.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err("o CurseForge não forneceu um SHA-1 seguro para este arquivo".to_owned());
    }
    let parsed_url = reqwest::Url::parse(page_url.trim())
        .map_err(|_| "a página de download do CurseForge é inválida".to_owned())?;
    if parsed_url.scheme() != "https" || !is_curseforge_host(parsed_url.host_str()) {
        return Err("somente páginas HTTPS oficiais do CurseForge podem ser abertas".to_owned());
    }

    let destination = instance.root().join(folder).join(&filename);
    let temporary = destination.with_file_name(format!(
        ".{}.{}.aurora-manual-download",
        Uuid::new_v4().simple(),
        filename
    ));
    let label = format!("curseforge-download-{}", Uuid::new_v4().simple());
    let download_started = Arc::new(AtomicBool::new(false));
    let handler_started = Arc::clone(&download_started);
    let handler_app = app.clone();
    let handler_label = label.clone();
    let handler_instance_id = instance_id.clone();
    let handler_filename = filename.clone();
    let handler_destination = destination.clone();
    let handler_temporary = temporary.clone();
    let handler_sha1 = sha1.clone();

    WebviewWindowBuilder::new(&app, label, WebviewUrl::External(parsed_url))
        .title(format!("Aurora · Baixando {filename}"))
        .inner_size(980.0, 720.0)
        .min_inner_size(720.0, 520.0)
        .center()
        .on_navigation(|url| is_curseforge_host(url.host_str()))
        .on_download(move |_webview, event| match event {
            DownloadEvent::Requested { destination, .. } => {
                if handler_started.swap(true, Ordering::SeqCst) {
                    return false;
                }
                *destination = handler_temporary.clone();
                let _ = handler_app.emit(
                    "aurora-manual-download",
                    ManualDownloadEvent {
                        instance_id: handler_instance_id.clone(),
                        filename: handler_filename.clone(),
                        status: "downloading".to_owned(),
                        error: None,
                    },
                );
                true
            }
            DownloadEvent::Finished { path, success, .. } => {
                let result = if success {
                    finalize_manual_download(
                        path.as_deref(),
                        &handler_temporary,
                        &handler_destination,
                        &handler_sha1,
                    )
                } else {
                    Err("o CurseForge não concluiu o download".to_owned())
                };
                if result.is_err() {
                    let _ = fs::remove_file(&handler_temporary);
                }
                let _ = handler_app.emit(
                    "aurora-manual-download",
                    ManualDownloadEvent {
                        instance_id: handler_instance_id.clone(),
                        filename: handler_filename.clone(),
                        status: if result.is_ok() {
                            "completed"
                        } else {
                            "failed"
                        }
                        .to_owned(),
                        error: result.err(),
                    },
                );
                if let Some(window) = handler_app.get_webview_window(&handler_label) {
                    let _ = window.close();
                }
                true
            }
            _ => true,
        })
        .build()
        .map_err(|error| format!("não foi possível abrir o download dentro do Aurora: {error}"))?;

    Ok(())
}

fn content_folder(content_type: &str) -> Result<&'static str, String> {
    match content_type {
        "mod" => Ok("mods"),
        "shader" => Ok("shaderpacks"),
        "resourcepack" => Ok("resourcepacks"),
        _ => Err("tipo de conteúdo inválido".to_owned()),
    }
}

fn validate_content_filename(filename: &str, content_type: &str) -> Result<(), String> {
    if filename.is_empty()
        || filename.len() > 255
        || filename.contains(['/', '\\'])
        || filename == "."
        || filename == ".."
    {
        return Err("nome de arquivo de conteúdo inválido".to_owned());
    }
    let expected_extension = if content_type == "mod" {
        ".jar"
    } else {
        ".zip"
    };
    if !filename.to_ascii_lowercase().ends_with(expected_extension) {
        return Err(format!(
            "o arquivo precisa terminar em {expected_extension}"
        ));
    }
    Ok(())
}

fn is_curseforge_host(host: Option<&str>) -> bool {
    host.is_some_and(|host| host == "curseforge.com" || host.ends_with(".curseforge.com"))
}

fn finalize_manual_download(
    reported_path: Option<&std::path::Path>,
    temporary: &std::path::Path,
    destination: &std::path::Path,
    expected_sha1: &str,
) -> Result<(), String> {
    let downloaded = reported_path.unwrap_or(temporary);
    if downloaded != temporary || !temporary.is_file() {
        return Err("o navegador interno não entregou o arquivo esperado".to_owned());
    }
    let metadata = fs::metadata(temporary)
        .map_err(|error| format!("não foi possível verificar o arquivo: {error}"))?;
    if metadata.len() == 0 || metadata.len() > 2 * 1024 * 1024 * 1024 {
        return Err("o arquivo baixado tem um tamanho inválido".to_owned());
    }
    let mut file = fs::File::open(temporary)
        .map_err(|error| format!("não foi possível verificar o arquivo: {error}"))?;
    let mut digest = Sha1::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let size = file
            .read(&mut buffer)
            .map_err(|error| format!("não foi possível verificar o arquivo: {error}"))?;
        if size == 0 {
            break;
        }
        digest.update(&buffer[..size]);
    }
    if !format!("{:x}", digest.finalize()).eq_ignore_ascii_case(expected_sha1) {
        return Err("o SHA-1 do arquivo baixado não corresponde ao publicado".to_owned());
    }
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|error| format!("não foi possível substituir o conteúdo anterior: {error}"))?;
    }
    fs::rename(temporary, destination)
        .map_err(|error| format!("não foi possível instalar o conteúdo: {error}"))
}

#[tauri::command]
pub async fn resolve_content_artwork(
    app: AppHandle,
    id: String,
    content_type: String,
) -> Result<Vec<ResolvedContentArtwork>, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    tauri::async_runtime::spawn_blocking(move || {
        resolve_modrinth_content_artwork(&instance, &content_type)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("a identificação do conteúdo falhou: {error}"))?
}

#[tauri::command]
pub async fn install_modrinth_modpack(
    app: AppHandle,
    id: String,
    project_id: String,
    minecraft_version: String,
) -> Result<ModpackInstallSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    let runtime_data_directory = data_directory(&app)?;
    emit_download_progress(
        &app,
        format!("Preparando modpack para {minecraft_version}"),
        1.0,
    );
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let java = ensure_managed_java_with_progress(
            &runtime_data_directory,
            required_java_major(&minecraft_version),
            |mut progress| {
                progress.total_percent *= 0.1;
                emit_transfer_progress(&progress_app, progress);
            },
        )
        .map_err(|error| error.to_string())?;
        install_modrinth_archive(
            &instance,
            &project_id,
            &minecraft_version,
            Some(java.executable()),
            |mut progress: ModpackProgress| {
                progress.percent = 10.0 + progress.percent * 0.9;
                emit_modpack_progress(&progress_app, progress);
            },
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("a instalação do modpack falhou: {error}"))?
}

#[tauri::command]
pub async fn install_modrinth_content(
    app: AppHandle,
    id: String,
    project_id: String,
    minecraft_version: String,
    content_type: String,
    loader: Option<String>,
) -> Result<String, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    emit_download_progress(&app, format!("Baixando conteúdo {project_id}"), 1.0);
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = install_modrinth_project(
            &instance,
            &project_id,
            &minecraft_version,
            &content_type,
            loader.as_deref(),
            |progress| emit_transfer_progress(&progress_app, progress),
        )
        .map_err(|error| error.to_string())?;
        emit_download_progress(&progress_app, format!("Instalado: {result}"), 100.0);
        Ok(result)
    })
    .await
    .map_err(|error| format!("a instalação do conteúdo falhou: {error}"))?
}

#[tauri::command]
pub fn list_instance_content(app: AppHandle, id: String) -> Result<InstanceContentSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    let list = |folder: &str| -> Result<Vec<InstanceContentFile>, String> {
        let directory = instance.root().join(folder);
        if !directory.is_dir() {
            return Ok(Vec::new());
        }
        let mut entries = fs::read_dir(directory)
            .map_err(|error| format!("não foi possível ler {folder}: {error}"))?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry
                    .file_type()
                    .ok()
                    .filter(|kind| kind.is_file())
                    .map(|_| {
                        let filename = entry.file_name().to_string_lossy().into_owned();
                        let enabled = !filename.ends_with(".disabled");
                        InstanceContentFile {
                            name: filename
                                .strip_suffix(".disabled")
                                .unwrap_or(&filename)
                                .to_owned(),
                            enabled,
                        }
                    })
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(entries)
    };
    Ok(InstanceContentSummary {
        mods: list("mods")?,
        shaderpacks: list("shaderpacks")?,
        resourcepacks: list("resourcepacks")?,
    })
}

fn instance_content_folder(content_type: &str) -> Result<&'static str, String> {
    match content_type {
        "mod" => Ok("mods"),
        "shader" => Ok("shaderpacks"),
        "resourcepack" => Ok("resourcepacks"),
        _ => Err("tipo de conteúdo não suportado".to_owned()),
    }
}

fn safe_content_filename(filename: &str) -> Result<&str, String> {
    if filename.is_empty()
        || filename == "."
        || filename == ".."
        || std::path::Path::new(filename)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(filename)
    {
        return Err("nome de arquivo de conteúdo inválido".to_owned());
    }
    Ok(filename)
}

#[tauri::command]
pub fn set_instance_content_enabled(
    app: AppHandle,
    id: String,
    content_type: String,
    filename: String,
    enabled: bool,
) -> Result<(), String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let folder = instance_content_folder(&content_type)?;
    let filename = safe_content_filename(&filename)?;
    let directory = layout(&app)?.open(id).root().join(folder);
    let active = directory.join(filename);
    let inactive = directory.join(format!("{filename}.disabled"));
    let (source, destination) = if enabled {
        (inactive, active)
    } else {
        (active, inactive)
    };
    if !source.is_file() {
        return Err("arquivo de conteúdo não foi encontrado".to_owned());
    }
    if destination.exists() {
        return Err("já existe um arquivo com esse nome no destino".to_owned());
    }
    fs::rename(source, destination)
        .map_err(|error| format!("não foi possível atualizar o conteúdo da instância: {error}"))
}

#[tauri::command]
pub fn set_instance_content_enabled_bulk(
    app: AppHandle,
    id: String,
    content_type: String,
    filenames: Vec<String>,
    enabled: bool,
) -> Result<usize, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let folder = instance_content_folder(&content_type)?;
    let filenames = unique_content_filenames(filenames)?;
    let directory = layout(&app)?.open(id).root().join(folder);
    let mut changes = Vec::with_capacity(filenames.len());
    for filename in filenames {
        let active = directory.join(&filename);
        let inactive = directory.join(format!("{filename}.disabled"));
        let (source, destination) = if enabled {
            (inactive, active)
        } else {
            (active, inactive)
        };
        if !source.is_file() {
            return Err(format!(
                "o arquivo “{filename}” não foi encontrado no estado esperado"
            ));
        }
        if destination.exists() {
            return Err(format!(
                "já existe um arquivo chamado “{filename}” no destino"
            ));
        }
        changes.push((source, destination));
    }
    for (source, destination) in &changes {
        fs::rename(source, destination)
            .map_err(|error| format!("não foi possível atualizar o conteúdo: {error}"))?;
    }
    Ok(changes.len())
}

#[tauri::command]
pub fn remove_instance_content(
    app: AppHandle,
    id: String,
    content_type: String,
    filenames: Vec<String>,
) -> Result<usize, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let folder = instance_content_folder(&content_type)?;
    let filenames = unique_content_filenames(filenames)?;
    let directory = layout(&app)?.open(id).root().join(folder);
    let mut targets = Vec::with_capacity(filenames.len());
    for filename in filenames {
        let active = directory.join(&filename);
        let inactive = directory.join(format!("{filename}.disabled"));
        let target = if active.is_file() {
            active
        } else if inactive.is_file() {
            inactive
        } else {
            return Err(format!("o arquivo “{filename}” não foi encontrado"));
        };
        targets.push(target);
    }
    for target in &targets {
        fs::remove_file(target)
            .map_err(|error| format!("não foi possível desinstalar o conteúdo: {error}"))?;
    }
    Ok(targets.len())
}

fn unique_content_filenames(filenames: Vec<String>) -> Result<Vec<String>, String> {
    if filenames.is_empty() || filenames.len() > 500 {
        return Err("selecione entre 1 e 500 arquivos".to_owned());
    }
    let mut seen = HashSet::new();
    let mut unique = Vec::with_capacity(filenames.len());
    for filename in filenames {
        let filename = safe_content_filename(&filename)?.to_owned();
        if seen.insert(filename.clone()) {
            unique.push(filename);
        }
    }
    Ok(unique)
}

#[tauri::command]
pub fn open_instance_folder(app: AppHandle, id: String) -> Result<(), String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    instance
        .ensure_layout()
        .map_err(|error| error.to_string())?;
    Command::new("explorer.exe")
        .arg(instance.root())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("não foi possível abrir a pasta da instância: {error}"))
}

#[tauri::command]
pub fn read_instance_log(app: AppHandle, id: String) -> Result<InstanceLogSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let directory = layout(&app)?.open(id).root().join("logs");
    if !directory.is_dir() {
        return Ok(InstanceLogSummary {
            filename: "latest.log".to_owned(),
            lines: Vec::new(),
        });
    }
    let newest = fs::read_dir(directory)
        .map_err(|error| format!("não foi possível ler os logs: {error}"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata
                .is_file()
                .then_some((metadata.modified().ok()?, entry.path()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path);
    let Some(path) = newest else {
        return Ok(InstanceLogSummary {
            filename: "latest.log".to_owned(),
            lines: Vec::new(),
        });
    };
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("não foi possível abrir o log: {error}"))?;
    let mut lines = contents
        .lines()
        .rev()
        .take(500)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    lines.reverse();
    Ok(InstanceLogSummary {
        filename: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("latest.log")
            .to_owned(),
        lines,
    })
}

#[tauri::command]
pub fn read_instance_launch_profile(
    app: AppHandle,
    id: String,
) -> Result<InstanceLaunchProfile, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    for filename in ["aurora-modpack.json", "aurora-instance.json"] {
        let path = instance.root().join(filename);
        let Ok(contents) = fs::read(path) else {
            continue;
        };
        let Ok(document) = serde_json::from_slice::<serde_json::Value>(&contents) else {
            continue;
        };
        let version_id = document["versionId"].as_str().map(str::to_owned);
        let minecraft_version = document["minecraft"].as_str().map(str::to_owned);
        if version_id.is_some() || minecraft_version.is_some() {
            return Ok(InstanceLaunchProfile {
                version_id,
                minecraft_version,
            });
        }
    }
    let versions_dir = instance.root().join("versions");
    if let Ok(entries) = fs::read_dir(versions_dir) {
        let mut candidates = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| {
                let version_id = entry.file_name().to_string_lossy().into_owned();
                let profile_path = entry.path().join(format!("{version_id}.json"));
                let document =
                    serde_json::from_slice::<serde_json::Value>(&fs::read(profile_path).ok()?)
                        .ok()?;
                let minecraft_version = document["inheritsFrom"]
                    .as_str()
                    .or_else(|| {
                        document["id"].as_str().filter(|id| {
                            id.chars()
                                .next()
                                .is_some_and(|character| character.is_ascii_digit())
                        })
                    })
                    .map(str::to_owned);
                let loader_priority = if version_id.to_ascii_lowercase().contains("fabric")
                    || version_id.to_ascii_lowercase().contains("forge")
                {
                    0
                } else {
                    1
                };
                Some((loader_priority, version_id, minecraft_version))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| right.1.cmp(&left.1)));
        if let Some((_, version_id, minecraft_version)) = candidates.into_iter().next() {
            return Ok(InstanceLaunchProfile {
                version_id: Some(version_id),
                minecraft_version,
            });
        }
    }
    Ok(InstanceLaunchProfile {
        version_id: None,
        minecraft_version: None,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn launch_instance(
    app: AppHandle,
    sessions: tauri::State<IpcSessions>,
    running: tauri::State<RunningInstances>,
    id: String,
    version_id: String,
    minecraft_version: String,
    java_executable: String,
    nickname: String,
    skin_url: Option<String>,
    skin_file: Option<String>,
    cape_url: Option<String>,
    skin_model: Option<String>,
) -> Result<LaunchSummary, String> {
    let nickname = nickname.trim().to_owned();
    validate_nickname(&nickname).map_err(|error| error.to_string())?;
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("esta instância já está rodando".to_owned());
    }
    let instance_id = id.as_str().to_owned();
    let instance = layout(&app)?.open(id);
    let java = JavaRuntime::from_executable(java_executable).map_err(|error| error.to_string())?;
    let uuid = offline_uuid_for_nickname(&nickname).map_err(|error| error.to_string())?;
    let mut spec =
        resolve_launch_spec(&instance, &version_id).map_err(|error| error.to_string())?;
    spec.apply_offline_compatibility(&minecraft_version);
    let loader = loader_for_profile(&version_id);
    let mut ipc_server = None;
    let mut core_installed = false;
    let mut companion_installed = false;
    if let Some(loader) = loader {
        install_embedded_core(&instance, &minecraft_version, loader)
            .map_err(|error| error.to_string())?;
        core_installed = true;
        install_embedded_companion(&instance, &minecraft_version, loader)
            .map_err(|error| error.to_string())?;
        let server =
            IpcServer::start_with_session(Some(IpcSessionProfile::offline(uuid, nickname.clone())))
                .map_err(|error| error.to_string())?;
        spec.aurora_ipc_port = Some(server.endpoint().port);
        spec.aurora_session_nonce = Some(server.endpoint().nonce.clone());
        spec.jvm_args.push(format!("-Daurora.loader={loader}"));
        if let Some(url) = public_profile_url(skin_url.as_deref()) {
            spec.jvm_args
                .push(format!("-Daurora.profile.skinUrl={url}"));
        }
        if let Some(path) = local_appearance_path(&app, skin_file.as_deref(), "skin") {
            spec.jvm_args
                .push(format!("-Daurora.profile.skinFile={}", path.display()));
        }
        if let Some(url) = public_profile_url(cape_url.as_deref()) {
            spec.jvm_args
                .push(format!("-Daurora.profile.capeUrl={url}"));
        }
        if let Some(model) = skin_model
            .as_deref()
            .filter(|model| matches!(*model, "classic" | "slim"))
        {
            spec.jvm_args
                .push(format!("-Daurora.profile.skinModel={model}"));
        }
        ipc_server = Some(server);
        companion_installed = true;
    }
    let identity = LaunchIdentity {
        username: nickname,
        uuid,
        access_token: "aurora_session_authenticated".to_owned(),
        user_type: "mojang".to_owned(),
    };
    let prepared = LauncherEngine
        .prepare_launch(&instance, &java, &identity, &spec, 4_096)
        .map_err(|error| error.to_string())?;
    let mut child = prepared.spawn().map_err(|error| error.to_string())?;
    let process_id = child.id();
    if let Some(server) = ipc_server {
        let Ok(mut ipc_sessions) = sessions.0.lock() else {
            let _ = child.kill();
            return Err("não foi possível preservar a sessão IPC".to_owned());
        };
        ipc_sessions.insert(process_id, server);
    }
    let Ok(mut processes) = running.0.lock() else {
        let _ = child.kill();
        if let Ok(mut ipc_sessions) = sessions.0.lock() {
            ipc_sessions.remove(&process_id);
        }
        return Err("não foi possível acompanhar o jogo iniciado".to_owned());
    };
    prune_finished_processes(&mut processes);
    if processes
        .values()
        .any(|process| process.instance_id == instance_id)
    {
        let _ = child.kill();
        if let Ok(mut ipc_sessions) = sessions.0.lock() {
            ipc_sessions.remove(&process_id);
        }
        return Err("esta instância já está rodando".to_owned());
    }
    processes.insert(process_id, RunningInstance { instance_id, child });
    Ok(LaunchSummary {
        process_id,
        version_id,
        core_installed,
        companion_installed,
    })
}

#[tauri::command]
pub fn list_running_instances(
    running: tauri::State<RunningInstances>,
    sessions: tauri::State<IpcSessions>,
) -> Result<Vec<RunningInstanceSummary>, String> {
    let mut processes = running
        .0
        .lock()
        .map_err(|_| "não foi possível consultar os jogos em execução".to_owned())?;
    let finished = prune_finished_processes(&mut processes);
    if !finished.is_empty() {
        let mut ipc_sessions = sessions
            .0
            .lock()
            .map_err(|_| "não foi possível atualizar as sessões encerradas".to_owned())?;
        for process_id in finished {
            ipc_sessions.remove(&process_id);
        }
    }
    Ok(processes
        .iter()
        .map(|(process_id, process)| RunningInstanceSummary {
            instance_id: process.instance_id.clone(),
            process_id: *process_id,
        })
        .collect())
}

#[tauri::command]
pub fn poll_ipc_events(sessions: tauri::State<IpcSessions>) -> Vec<IpcSessionEvent> {
    let Ok(sessions) = sessions.0.lock() else {
        return Vec::new();
    };
    sessions
        .iter()
        .flat_map(|(process_id, server)| {
            std::iter::from_fn(|| server.try_recv().ok()).map(|event| IpcSessionEvent {
                process_id: *process_id,
                event,
            })
        })
        .collect()
}

#[tauri::command]
pub fn toggle_ipc_assistant(sessions: tauri::State<IpcSessions>) -> Result<bool, String> {
    let sessions = sessions
        .0
        .lock()
        .map_err(|_| "não foi possível acessar a sessão do jogo".to_owned())?;
    let mut delivered = false;
    for server in sessions.values().filter(|server| server.is_connected()) {
        server
            .send_json(&serde_json::json!({ "kind": "toggleAssistant" }))
            .map_err(|error| error.to_string())?;
        delivered = true;
    }
    Ok(delivered)
}

#[tauri::command]
pub fn send_ipc_assistant_response(
    sessions: tauri::State<IpcSessions>,
    process_id: u32,
    request_id: String,
    text: Option<String>,
    error: Option<String>,
) -> Result<(), String> {
    if request_id.is_empty() || request_id.len() > 64 {
        return Err("identificador de resposta inválido".to_owned());
    }
    let text = text.map(|value| value.chars().take(8_000).collect::<String>());
    let error = error.map(|value| value.chars().take(500).collect::<String>());
    let sessions = sessions
        .0
        .lock()
        .map_err(|_| "não foi possível acessar a sessão do jogo".to_owned())?;
    let server = sessions
        .get(&process_id)
        .ok_or_else(|| "a sessão do jogo não está mais conectada".to_owned())?;
    server
        .send_json(&serde_json::json!({
            "kind": "assistantResponse",
            "requestId": request_id,
            "text": text,
            "error": error,
        }))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn send_ipc_caption(
    sessions: tauri::State<IpcSessions>,
    process_id: u32,
    request_id: String,
    caption: String,
) -> Result<(), String> {
    let sessions = sessions
        .0
        .lock()
        .map_err(|_| "não foi possível acessar a sessão do jogo".to_owned())?;
    let server = sessions
        .get(&process_id)
        .ok_or_else(|| "a sessão do jogo não está mais conectada".to_owned())?;
    server
        .send_json(&serde_json::json!({
            "kind": "assistantCaption",
            "requestId": request_id,
            "caption": caption.chars().take(500).collect::<String>(),
        }))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn send_ipc_transcript(
    sessions: tauri::State<IpcSessions>,
    process_id: u32,
    request_id: String,
    text: Option<String>,
    error: Option<String>,
) -> Result<(), String> {
    if request_id.is_empty() || request_id.len() > 64 {
        return Err("identificador de transcrição inválido".to_owned());
    }
    let sessions = sessions
        .0
        .lock()
        .map_err(|_| "não foi possível acessar a sessão do jogo".to_owned())?;
    let server = sessions
        .get(&process_id)
        .ok_or_else(|| "a sessão do jogo não está mais conectada".to_owned())?;
    server
        .send_json(&serde_json::json!({
            "kind": "assistantTranscript",
            "requestId": request_id,
            "text": text.map(|value| value.chars().take(2_000).collect::<String>()),
            "error": error.map(|value| value.chars().take(300).collect::<String>()),
        }))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn synthesize_speech(text: String) -> Result<SpeechResult, String> {
    aurora_launcher_core::engine::synthesize_speech(&text)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn validate_appearance_url(url: String, kind: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        download_appearance_url_blocking(&url, &kind).map(|(url, _)| url)
    })
    .await
    .map_err(|error| format!("não foi possível validar a imagem: {error}"))?
}

#[tauri::command]
pub async fn load_appearance_url(
    url: String,
    kind: String,
) -> Result<AppearanceImageSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (url, bytes) = download_appearance_url_blocking(&url, &kind)?;
        Ok(AppearanceImageSummary {
            url,
            data_base64: format!("data:image/png;base64,{}", STANDARD.encode(bytes)),
        })
    })
    .await
    .map_err(|error| format!("não foi possível carregar a imagem: {error}"))?
}

#[tauri::command]
pub fn load_local_appearance(
    app: AppHandle,
    user_id: String,
    kind: String,
) -> Result<AppearanceImageSummary, String> {
    if !valid_profile_id(&user_id) || !matches!(kind.as_str(), "skin" | "cape") {
        return Err("aparência local inválida".to_owned());
    }
    let path = data_directory(&app)?
        .join("appearance")
        .join(user_id)
        .join(format!("{kind}.png"));
    let bytes = fs::read(&path)
        .map_err(|error| format!("não foi possível abrir a aparência local: {error}"))?;
    if bytes.is_empty() || bytes.len() > 5 * 1024 * 1024 {
        return Err("a aparência local tem um tamanho inválido".to_owned());
    }
    png_dimensions(&bytes)?;
    Ok(AppearanceImageSummary {
        url: String::new(),
        data_base64: format!("data:image/png;base64,{}", STANDARD.encode(bytes)),
    })
}

#[tauri::command]
pub fn save_local_appearance(
    app: AppHandle,
    user_id: String,
    kind: String,
    data_base64: String,
) -> Result<String, String> {
    const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
    if !matches!(kind.as_str(), "skin" | "cape") {
        return Err("tipo de aparência inválido".to_owned());
    }
    if !valid_profile_id(&user_id) {
        return Err("identificador de perfil inválido".to_owned());
    }
    let encoded = data_base64
        .strip_prefix("data:image/png;base64,")
        .unwrap_or(&data_base64);
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "a aparência não contém um PNG válido".to_owned())?;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return Err("a imagem precisa ter até 5 MB".to_owned());
    }
    let (width, height) = png_dimensions(&bytes)?;
    if kind == "skin" && !(width == 64 && matches!(height, 32 | 64)) {
        return Err(format!(
            "a skin precisa ter 64x64 ou 64x32 pixels; esta imagem tem {width}x{height}"
        ));
    }
    let directory = data_directory(&app)?.join("appearance").join(user_id);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("não foi possível preparar a aparência local: {error}"))?;
    let destination = directory.join(format!("{kind}.png"));
    let temporary = directory.join(format!("{kind}.png.aurora-writing"));
    fs::write(&temporary, bytes)
        .map_err(|error| format!("não foi possível salvar a aparência local: {error}"))?;
    if destination.exists() {
        fs::remove_file(&destination)
            .map_err(|error| format!("não foi possível atualizar a aparência local: {error}"))?;
    }
    fs::rename(&temporary, &destination)
        .map_err(|error| format!("não foi possível concluir a aparência local: {error}"))?;
    Ok(destination.display().to_string())
}

fn valid_profile_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn local_appearance_path(app: &AppHandle, value: Option<&str>, kind: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(value?.trim());
    let root = data_directory(app)
        .ok()?
        .join("appearance")
        .canonicalize()
        .ok()?;
    let candidate = candidate.canonicalize().ok()?;
    (candidate.starts_with(root)
        && candidate.is_file()
        && candidate.file_name()?.to_str()? == format!("{kind}.png"))
    .then_some(candidate)
}

fn download_appearance_url_blocking(url: &str, kind: &str) -> Result<(String, Vec<u8>), String> {
    const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;

    let url = url.trim();
    if url.len() > 2_048 || !url.starts_with("https://") {
        return Err("use uma URL HTTPS direta para a imagem PNG".to_owned());
    }
    if !matches!(kind, "skin" | "cape") {
        return Err("tipo de aparência inválido".to_owned());
    }

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15))
        .user_agent("AuroraLauncher/0.1")
        .build()
        .map_err(|error| format!("falha ao preparar a validação: {error}"))?;
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("não foi possível baixar a imagem: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "o endereço da imagem respondeu com HTTP {}",
            response.status().as_u16()
        ));
    }
    if response.url().scheme() != "https" {
        return Err("o endereço redirecionou para uma conexão não segura".to_owned());
    }
    let final_url = response.url().as_str().to_owned();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_IMAGE_BYTES)
    {
        return Err("a imagem ultrapassa o limite de 5 MB".to_owned());
    }

    let mut bytes = Vec::new();
    response
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("não foi possível ler a imagem: {error}"))?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("a imagem ultrapassa o limite de 5 MB".to_owned());
    }
    let (width, height) = png_dimensions(&bytes)?;
    if kind == "skin" && !(width == 64 && matches!(height, 32 | 64)) {
        return Err(format!(
            "a skin precisa ter 64x64 ou 64x32 pixels; esta imagem tem {width}x{height}"
        ));
    }
    if kind == "cape" && (width == 0 || height == 0 || width > 1_024 || height > 1_024) {
        return Err(format!("dimensões de capa inválidas: {width}x{height}"));
    }

    Ok((final_url, bytes))
}

fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    let png_signature = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != png_signature || &bytes[12..16] != b"IHDR" {
        return Err("o endereço não aponta diretamente para uma imagem PNG".to_owned());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("PNG width slice"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("PNG height slice"));
    Ok((width, height))
}

fn loader_for_profile(version_id: &str) -> Option<&'static str> {
    let version_id = version_id.to_ascii_lowercase();
    if version_id.contains("fabric") {
        Some("fabric")
    } else if version_id.contains("forge") {
        Some("forge")
    } else {
        None
    }
}

fn public_profile_url(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    (value.len() <= 2_048 && value.starts_with("https://")).then_some(value)
}

#[tauri::command]
pub fn verify_java(executable: String) -> Result<String, String> {
    let runtime = JavaRuntime::from_executable(executable).map_err(|error| error.to_string())?;
    runtime.version().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn discover_java() -> Vec<JavaRuntimeSummary> {
    discover_java_executables()
        .into_iter()
        .filter_map(|executable| {
            let runtime = JavaRuntime::from_executable(&executable).ok()?;
            Some(JavaRuntimeSummary {
                executable: executable.display().to_string(),
                version: runtime
                    .version()
                    .unwrap_or_else(|_| "Java detectado".to_owned()),
            })
        })
        .collect()
}

#[tauri::command]
pub async fn ensure_java(
    app: AppHandle,
    minecraft_version: String,
) -> Result<JavaRuntimeSummary, String> {
    let major = required_java_major(&minecraft_version);
    emit_download_progress(&app, format!("Preparando Java {major}"), 5.0);
    let data_directory = data_directory(&app)?;
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = ensure_managed_java_with_progress(&data_directory, major, |progress| {
            emit_transfer_progress(&progress_app, progress);
        })
        .map_err(|error| error.to_string())?;
        let summary = JavaRuntimeSummary {
            executable: runtime.executable().display().to_string(),
            version: runtime.version().map_err(|error| error.to_string())?,
        };
        emit_download_progress(&progress_app, format!("Java {major} pronto"), 100.0);
        Ok(summary)
    })
    .await
    .map_err(|error| format!("a preparação do Java falhou: {error}"))?
}

#[tauri::command]
pub fn offline_uuid(nickname: String) -> Result<String, String> {
    offline_uuid_for_nickname(&nickname)
        .map(|uuid| uuid.to_string())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn load_firebase_config(app: AppHandle) -> Result<Option<FirebasePublicConfig>, String> {
    let path = firebase_config_path(&app)?;
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("não foi possível ler a configuração Firebase: {error}"))?;
    serde_json::from_str(&contents)
        .map(Some)
        .map_err(|error| format!("a configuração Firebase está inválida: {error}"))
}

#[tauri::command]
pub fn save_firebase_config(app: AppHandle, config: FirebasePublicConfig) -> Result<(), String> {
    let values = [
        &config.api_key,
        &config.auth_domain,
        &config.project_id,
        &config.storage_bucket,
        &config.messaging_sender_id,
        &config.app_id,
    ];
    if values.iter().any(|value| value.trim().is_empty()) {
        return Err("preencha todos os campos públicos da configuração Firebase".to_owned());
    }

    let path = firebase_config_path(&app)?;
    let parent = path.parent().ok_or("diretório de dados inválido")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("não foi possível salvar a configuração Firebase: {error}"))?;
    let encoded = serde_json::to_string_pretty(&config)
        .map_err(|error| format!("não foi possível codificar a configuração Firebase: {error}"))?;
    fs::write(path, encoded)
        .map_err(|error| format!("não foi possível salvar a configuração Firebase: {error}"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        finalize_manual_download, is_curseforge_host, png_dimensions, unique_content_filenames,
        validate_content_filename,
    };

    #[test]
    fn reads_png_dimensions_from_ihdr() {
        let mut header = vec![0u8; 24];
        header[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        header[12..16].copy_from_slice(b"IHDR");
        header[16..20].copy_from_slice(&64u32.to_be_bytes());
        header[20..24].copy_from_slice(&32u32.to_be_bytes());
        assert_eq!(png_dimensions(&header), Ok((64, 32)));
    }

    #[test]
    fn refuses_html_disguised_as_image() {
        assert!(png_dimensions(b"<!doctype html><html>").is_err());
    }

    #[test]
    fn bulk_content_actions_reject_unsafe_and_duplicate_names() {
        assert!(unique_content_filenames(vec!["../mod.jar".to_owned()]).is_err());
        let names = unique_content_filenames(vec!["mod.jar".to_owned(), "mod.jar".to_owned()])
            .expect("nomes seguros devem ser aceitos");
        assert_eq!(names, vec!["mod.jar"]);
    }

    #[test]
    fn manual_download_only_accepts_official_pages_and_content_extensions() {
        assert!(is_curseforge_host(Some("www.curseforge.com")));
        assert!(is_curseforge_host(Some("beta.curseforge.com")));
        assert!(!is_curseforge_host(Some("curseforge.com.example.org")));
        assert!(validate_content_filename("example.jar", "mod").is_ok());
        assert!(validate_content_filename("../example.jar", "mod").is_err());
        assert!(validate_content_filename("example.exe", "mod").is_err());
    }

    #[test]
    fn manual_download_is_published_only_after_sha1_validation() {
        let directory = std::env::temp_dir().join(format!(
            "aurora-manual-download-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&directory).expect("diretório temporário");
        let temporary = directory.join("download.part");
        let destination = directory.join("example.jar");
        fs::write(&temporary, b"hello").expect("arquivo temporário");
        finalize_manual_download(
            Some(&temporary),
            &temporary,
            &destination,
            "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d",
        )
        .expect("hash correto deve publicar o arquivo");
        assert_eq!(fs::read(&destination).expect("arquivo final"), b"hello");
        fs::remove_dir_all(directory).expect("limpeza temporária");
    }
}
