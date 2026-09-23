use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::Duration;

use crate::doctor_runs::{self, DoctorRuns};
use aurora_launcher_core::engine::{
    analyze_instance as analyze_doctor, canonical_launch_failure, snapshot_logs, DoctorContext,
    DoctorReport, DoctorRun,
};
use aurora_launcher_core::{
    auth::{offline_uuid_for_nickname, validate_nickname},
    engine::{
        acquire_instance_mutation_lock, apply_content_updates as apply_verified_content_updates,
        apply_modrinth_modpack_update_with_progress as apply_verified_modpack_update,
        apply_remote_content_updates as apply_verified_remote_content_updates,
        available_java_releases, companion_migration_required, discover_java_executables,
        discover_managed_java_executables, ensure_managed_java_with_progress,
        install_curseforge_modpack_with_progress, install_embedded_core, install_embedded_skins,
        install_modrinth_content_with_progress as install_modrinth_project,
        install_modrinth_modpack_with_progress as install_modrinth_archive,
        install_remote_content_with_progress as install_remote_project,
        list_modrinth_categories as list_modrinth_catalog_categories, migrate_managed_companion,
        prepare_curseforge_modpack_with_progress, read_cached_curseforge_manifest,
        required_java_major, required_java_major_for_instance, resolve_launch_spec,
        resolve_modrinth_content_artwork,
        rollback_last_content_update as rollback_verified_content_update,
        scan_content_updates as scan_verified_content_updates, scan_modrinth_modpack_update,
        search_modrinth_content as search_modrinth_catalog_content,
        search_modrinth_modpacks as search_modrinth_catalog, supported_managed_java_major,
        ContentUpdateApplySummary, ContentUpdateRollbackSummary, ContentUpdateScan,
        CurseForgeManifestSummary, CurseForgeResolvedFile, InstallSummary, InstanceId,
        InstanceLayout, InstanceMutationGuard, IpcEvent, IpcServer, IpcSessionProfile,
        JavaReleaseCatalog, JavaRuntime, LaunchIdentity, LauncherEngine, MinecraftInstaller,
        ModpackInstallSummary, ModpackProgress, ModpackUpdateApplySummary, ModpackUpdateScan,
        ModrinthCategory, ModrinthContent, ModrinthSearchPage, RemoteContentUpdate,
        ResolvedContentArtwork, SpeechResult, TransferProgress,
    },
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::{AppHandle, Emitter, Manager};

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
    run_id: String,
    process_id: u32,
    version_id: String,
    core_installed: bool,
    skins_installed: bool,
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

pub(crate) fn emit_transfer_progress(app: &AppHandle, progress: TransferProgress) {
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
    doctor: Option<(AppHandle, String)>,
    instance_id: String,
    child: Child,
    // The OS lock also excludes updates from another launcher process.
    _mutation_lock: InstanceMutationGuard,
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
        .filter_map(|(process_id, process)| {
            let (outcome, code) = match process.child.try_wait() {
                Ok(Some(status)) => (
                    doctor_runs::outcome_for_exit(status.code(), status.success()),
                    status.code(),
                ),
                Err(_) => ("unknown", None),
                Ok(None) => return None,
            };
            if let Some((app, run_id)) = &process.doctor {
                if let Ok(directory) = doctor_directory(app) {
                    let records = app.state::<DoctorRuns>();
                    if records
                        .finish(
                            &directory,
                            &process.instance_id,
                            run_id,
                            outcome,
                            code,
                            None,
                        )
                        .is_ok()
                    {
                        emit_doctor_run(app, &process.instance_id, run_id, outcome);
                    }
                }
            }
            Some(*process_id)
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

fn acquire_idle_instance(
    instance: &aurora_launcher_core::engine::Instance,
) -> Result<InstanceMutationGuard, String> {
    acquire_instance_mutation_lock(instance).map_err(|error| match error {
        aurora_launcher_core::engine::ContentUpdateError::Busy =>
            "Esta instância está em uso em outro jogo ou launcher. Aguarde a operação terminar ou use outra instância para jogar com duas contas.".to_owned(),
        error => error.to_string(),
    })
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

fn update_modpack_launch_profile(
    instance: &aurora_launcher_core::engine::Instance,
    result: &InstallSummary,
    loader: &str,
) -> Result<(), String> {
    let marker_path = instance.root().join("aurora-modpack.json");
    if !marker_path.is_file() {
        return Ok(());
    }
    let mut document = serde_json::from_slice::<serde_json::Value>(
        &fs::read(&marker_path)
            .map_err(|error| format!("não foi possível ler os metadados do modpack: {error}"))?,
    )
    .map_err(|error| format!("os metadados do modpack estão corrompidos: {error}"))?;
    let object = document
        .as_object_mut()
        .ok_or_else(|| "os metadados do modpack estão corrompidos".to_owned())?;
    object.insert("loader".to_owned(), serde_json::json!(loader));
    object.insert(
        "minecraft".to_owned(),
        serde_json::json!(result.minecraft_version),
    );
    object.insert("versionId".to_owned(), serde_json::json!(result.version_id));
    fs::write(
        marker_path,
        serde_json::to_vec_pretty(&document)
            .map_err(|error| format!("não foi possível preparar os metadados: {error}"))?,
    )
    .map_err(|error| format!("não foi possível atualizar os metadados: {error}"))
}

fn validate_instance_update_context(
    instance: &aurora_launcher_core::engine::Instance,
    minecraft_version: &str,
    loader: &str,
) -> Result<(), String> {
    if !matches!(loader, "fabric" | "forge") {
        return Err("somente Forge e Fabric podem receber atualizações".to_owned());
    }
    for filename in ["aurora-modpack.json", "aurora-instance.json"] {
        let path = instance.root().join(filename);
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let document = serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|_| format!("o perfil da instância em {filename} está corrompido"))?;
        let stored_minecraft = document["minecraft"].as_str();
        let stored_loader = document["loader"]
            .as_str()
            .or_else(|| {
                document["source"]
                    .as_str()
                    .filter(|value| matches!(*value, "fabric" | "forge"))
            })
            .or_else(|| document["versionId"].as_str().and_then(loader_for_profile));
        if let Some(stored_minecraft) = stored_minecraft {
            if stored_minecraft != minecraft_version {
                return Err(format!(
                    "a instância usa Minecraft {stored_minecraft}, não {minecraft_version}"
                ));
            }
        }
        if let Some(stored_loader) = stored_loader {
            if stored_loader != loader {
                return Err(format!("a instância usa {stored_loader}, não {loader}"));
            }
        }
        if stored_minecraft.is_some() && stored_loader.is_some() {
            return Ok(());
        }
    }
    Err("não foi possível confirmar a versão e o loader desta instância".to_owned())
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
    let _mutation_lock = acquire_idle_instance(&layout.open(id))?;
    #[cfg(windows)]
    ensure_no_external_instance_game(&source)?;
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
    let _mutation_lock = acquire_idle_instance(&instance)?;
    #[cfg(windows)]
    ensure_no_external_instance_game(instance.root())?;
    fs::remove_dir_all(instance.root())
        .map_err(|error| format!("não foi possível excluir a instância: {error}"))
}

/// Baixa arquivos oficiais do Minecraft fora da thread da interface.
#[tauri::command]
pub async fn install_vanilla(
    app: AppHandle,
    running: tauri::State<'_, RunningInstances>,
    id: String,
    minecraft_version: String,
) -> Result<InstallSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de alterar esta instância".to_owned());
    }
    let instance = layout(&app)?.open(id);
    emit_download_progress(
        &app,
        format!("Preparando Minecraft {minecraft_version}"),
        1.0,
    );
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _mutation_lock =
            acquire_instance_mutation_lock(&instance).map_err(|error| error.to_string())?;
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
    running: tauri::State<'_, RunningInstances>,
    id: String,
    minecraft_version: String,
) -> Result<InstallSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de alterar esta instância".to_owned());
    }
    let instance = layout(&app)?.open(id);
    emit_download_progress(
        &app,
        format!("Preparando Fabric para {minecraft_version}"),
        1.0,
    );
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _mutation_lock =
            acquire_instance_mutation_lock(&instance).map_err(|error| error.to_string())?;
        let result = MinecraftInstaller::new()
            .install_fabric_loader_with_progress(&instance, &minecraft_version, None, |progress| {
                emit_transfer_progress(&progress_app, progress);
            })
            .map_err(|error| error.to_string())?;
        write_instance_profile(&instance, &result, "fabric")?;
        update_modpack_launch_profile(&instance, &result, "fabric")?;
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
    running: tauri::State<'_, RunningInstances>,
    id: String,
    minecraft_version: String,
    forge_version: String,
) -> Result<InstallSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de alterar esta instância".to_owned());
    }
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
        let _mutation_lock =
            acquire_instance_mutation_lock(&instance).map_err(|error| error.to_string())?;
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
    minecraft_version: Option<String>,
    loader: Option<String>,
    category: Option<String>,
    sort: String,
    offset: u32,
    limit: u32,
) -> Result<ModrinthSearchPage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        search_modrinth_catalog(
            &query,
            minecraft_version.as_deref(),
            loader.as_deref(),
            category.as_deref(),
            &sort,
            offset,
            limit,
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("a busca de modpacks falhou: {error}"))?
}

#[tauri::command]
pub async fn list_modrinth_categories(
    content_type: String,
) -> Result<Vec<ModrinthCategory>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        list_modrinth_catalog_categories(&content_type).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("a consulta de categorias do Modrinth falhou: {error}"))?
}

#[tauri::command]
pub async fn search_modrinth_content(
    query: String,
    content_type: String,
    minecraft_version: Option<String>,
    loader: Option<String>,
    category: Option<String>,
    sort: String,
) -> Result<Vec<ModrinthContent>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        search_modrinth_catalog_content(
            &query,
            &content_type,
            minecraft_version.as_deref(),
            loader.as_deref(),
            category.as_deref(),
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
    running: tauri::State<'_, RunningInstances>,
    id: String,
    url: String,
    filename: String,
    content_type: String,
    sha1: Option<String>,
) -> Result<String, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de alterar o conteúdo".to_owned());
    }
    let instance = layout(&app)?.open(id);
    emit_download_progress(&app, format!("Baixando {filename}"), 1.0);
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _mutation_lock =
            acquire_instance_mutation_lock(&instance).map_err(|error| error.to_string())?;
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

/// Reutiliza o arquivo verificado, procura uma cópia oficial com SHA-1 idêntico
/// ou captura o download autorizado pela própria página do CurseForge.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn open_manual_content_download(
    app: AppHandle,
    running: tauri::State<'_, RunningInstances>,
    request_id: String,
    id: String,
    page_url: String,
    filename: String,
    content_type: String,
    sha1: String,
    project_id: Option<u64>,
    file_id: Option<u64>,
) -> Result<(), String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de alterar o conteúdo".to_owned());
    }
    let instance_id = id.as_str().to_owned();
    let instance = layout(&app)?.open(id);
    let mutation_lock =
        acquire_instance_mutation_lock(&instance).map_err(|error| error.to_string())?;
    instance
        .ensure_layout()
        .map_err(|error| error.to_string())?;
    validate_content_filename(&filename, &content_type)?;
    if sha1.len() != 40 || !sha1.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err("o CurseForge não forneceu um SHA-1 seguro para este arquivo".to_owned());
    }
    let parsed_url = reqwest::Url::parse(page_url.trim())
        .map_err(|_| "a página de download do CurseForge é inválida".to_owned())?;
    if parsed_url.scheme() != "https" || !is_curseforge_host(parsed_url.host_str()) {
        return Err("somente páginas HTTPS oficiais do CurseForge podem ser abertas".to_owned());
    }

    let destination = if content_type == "modpack" {
        let project_id = project_id.ok_or_else(|| "projeto CurseForge inválido".to_owned())?;
        let file_id = file_id.ok_or_else(|| "arquivo CurseForge inválido".to_owned())?;
        aurora_launcher_core::engine::curseforge_archive_path(&instance, project_id, file_id)
    } else {
        instance
            .root()
            .join(content_folder(&content_type)?)
            .join(&filename)
    };
    crate::manual_downloads::start(
        app,
        crate::manual_downloads::Request {
            request_id,
            instance_id,
            filename,
            content_type,
            sha1,
            page_url: parsed_url,
            root: instance.root().to_path_buf(),
            destination,
        },
        mutation_lock,
    )
    .await
}

#[tauri::command]
pub async fn cancel_manual_content_download(
    app: AppHandle,
    request_id: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::manual_downloads::cancel(&app, &request_id))
        .await
        .map_err(|_| "não foi possível cancelar o download".to_owned())?
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
    safe_content_filename(filename)?;
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

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn prepare_curseforge_modpack(
    app: AppHandle,
    id: String,
    project_id: u64,
    file_id: u64,
    download_url: Option<String>,
    filename: String,
    sha1: String,
) -> Result<CurseForgeManifestSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    emit_download_progress(&app, format!("Preparando {filename}"), 1.0);
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let manifest = prepare_curseforge_modpack_with_progress(
            &instance,
            project_id,
            file_id,
            download_url.as_deref(),
            &filename,
            &sha1,
            |progress| emit_transfer_progress(&progress_app, progress),
        )
        .map_err(|error| error.to_string())?;
        emit_download_progress(&progress_app, "Manifesto CurseForge validado", 100.0);
        Ok(manifest)
    })
    .await
    .map_err(|error| format!("a preparação do modpack falhou: {error}"))?
}

#[tauri::command]
pub async fn install_curseforge_modpack(
    app: AppHandle,
    running: tauri::State<'_, RunningInstances>,
    id: String,
    project_id: u64,
    file_id: u64,
    files: Vec<CurseForgeResolvedFile>,
    icon_url: Option<String>,
) -> Result<ModpackInstallSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de alterar esta instância".to_owned());
    }
    let instance = layout(&app)?.open(id);
    let manifest = read_cached_curseforge_manifest(&instance, project_id, file_id)
        .map_err(|error| error.to_string())?;
    let runtime_data_directory = data_directory(&app)?;
    emit_download_progress(&app, format!("Instalando {}", manifest.name), 1.0);
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let java = ensure_managed_java_with_progress(
            &runtime_data_directory,
            required_java_major(&manifest.minecraft_version),
            |mut progress| {
                progress.total_percent *= 0.08;
                emit_transfer_progress(&progress_app, progress);
            },
        )
        .map_err(|error| error.to_string())?;
        let _mutation_lock =
            acquire_instance_mutation_lock(&instance).map_err(|error| error.to_string())?;
        install_curseforge_modpack_with_progress(
            &instance,
            project_id,
            file_id,
            &files,
            Some(java.executable()),
            icon_url.as_deref(),
            |mut progress| {
                progress.percent = 8.0 + progress.percent * 0.92;
                emit_modpack_progress(&progress_app, progress);
            },
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("a instalação do modpack falhou: {error}"))?
}

fn is_curseforge_host(host: Option<&str>) -> bool {
    host.is_some_and(|host| host == "curseforge.com" || host.ends_with(".curseforge.com"))
}

pub(crate) fn finalize_manual_download(
    reported_path: Option<&std::path::Path>,
    temporary: &std::path::Path,
    destination: &std::path::Path,
    expected_sha1: &str,
) -> Result<(), String> {
    let downloaded = reported_path.unwrap_or(temporary);
    if downloaded != temporary {
        return Err("o navegador interno não entregou o arquivo esperado".to_owned());
    }
    verify_manual_download_file(temporary, expected_sha1)?;
    // Both paths are siblings: rename replaces atomically without deleting the
    // previous file first. A failed rename therefore preserves that file.
    fs::rename(temporary, destination)
        .map_err(|error| format!("não foi possível instalar o conteúdo: {error}"))
}

pub(crate) fn verify_manual_download_file(path: &Path, expected_sha1: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("não foi possível verificar o arquivo: {error}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("o download não é um arquivo regular".to_owned());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("o download não pode ser um link ou ponto de análise".to_owned());
        }
    }
    if metadata.len() == 0 || metadata.len() > 2 * 1024 * 1024 * 1024 {
        return Err("o arquivo baixado tem um tamanho inválido".to_owned());
    }
    let mut file = fs::File::open(path)
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
    Ok(())
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
    running: tauri::State<'_, RunningInstances>,
    id: String,
    project_id: String,
    minecraft_version: String,
) -> Result<ModpackInstallSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de alterar esta instância".to_owned());
    }
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
        let _mutation_lock =
            acquire_instance_mutation_lock(&instance).map_err(|error| error.to_string())?;
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
    running: tauri::State<'_, RunningInstances>,
    id: String,
    project_id: String,
    minecraft_version: String,
    content_type: String,
    loader: Option<String>,
) -> Result<String, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de alterar o conteúdo".to_owned());
    }
    let instance = layout(&app)?.open(id);
    emit_download_progress(&app, format!("Baixando conteúdo {project_id}"), 1.0);
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _mutation_lock =
            acquire_instance_mutation_lock(&instance).map_err(|error| error.to_string())?;
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
pub async fn scan_instance_content_updates(
    app: AppHandle,
    id: String,
    content_type: String,
    minecraft_version: String,
    loader: String,
) -> Result<ContentUpdateScan, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    if !instance.root().is_dir() {
        return Err("a instância não existe mais".to_owned());
    }
    validate_instance_update_context(&instance, &minecraft_version, &loader)?;
    tauri::async_runtime::spawn_blocking(move || {
        scan_verified_content_updates(&instance, &content_type, &minecraft_version, &loader)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("a verificação de atualizações falhou: {error}"))?
}

#[tauri::command]
pub async fn apply_instance_content_updates(
    app: AppHandle,
    running: tauri::State<'_, RunningInstances>,
    id: String,
    content_type: String,
    filenames: Vec<String>,
    minecraft_version: String,
    loader: String,
) -> Result<ContentUpdateApplySummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de atualizar o conteúdo".to_owned());
    }
    let instance = layout(&app)?.open(id);
    validate_instance_update_context(&instance, &minecraft_version, &loader)?;
    emit_download_progress(&app, "Revalidando atualizações", 2.0);
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let summary = apply_verified_content_updates(
            &instance,
            &content_type,
            &filenames,
            &minecraft_version,
            &loader,
        )
        .map_err(|error| error.to_string())?;
        emit_download_progress(&progress_app, "Atualização concluída", 100.0);
        Ok(summary)
    })
    .await
    .map_err(|error| format!("a atualização de conteúdo falhou: {error}"))?
}

#[tauri::command]
pub async fn apply_remote_content_updates(
    app: AppHandle,
    running: tauri::State<'_, RunningInstances>,
    id: String,
    content_type: String,
    updates: Vec<RemoteContentUpdate>,
    minecraft_version: String,
    loader: String,
) -> Result<ContentUpdateApplySummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de atualizar o conteúdo".to_owned());
    }
    let instance = layout(&app)?.open(id);
    validate_instance_update_context(&instance, &minecraft_version, &loader)?;
    emit_download_progress(&app, "Validando atualizações CurseForge", 2.0);
    let progress_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let summary = apply_verified_remote_content_updates(
            &instance,
            &content_type,
            &updates,
            &minecraft_version,
            &loader,
        )
        .map_err(|error| error.to_string())?;
        emit_download_progress(&progress_app, "Atualização concluída", 100.0);
        Ok(summary)
    })
    .await
    .map_err(|error| format!("a atualização de conteúdo falhou: {error}"))?
}

#[tauri::command]
pub async fn rollback_last_content_update(
    app: AppHandle,
    running: tauri::State<'_, RunningInstances>,
    id: String,
) -> Result<ContentUpdateRollbackSummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de desfazer uma atualização".to_owned());
    }
    let instance = layout(&app)?.open(id);
    tauri::async_runtime::spawn_blocking(move || {
        rollback_verified_content_update(&instance).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("o rollback da atualização falhou: {error}"))?
}

#[tauri::command]
pub async fn scan_instance_modpack_update(
    app: AppHandle,
    id: String,
) -> Result<ModpackUpdateScan, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    if !instance.root().is_dir() {
        return Err("a instância não existe mais".to_owned());
    }
    tauri::async_runtime::spawn_blocking(move || {
        scan_modrinth_modpack_update(&instance).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("a verificação do modpack falhou: {error}"))?
}

#[tauri::command]
pub async fn apply_instance_modpack_update(
    app: AppHandle,
    running: tauri::State<'_, RunningInstances>,
    id: String,
) -> Result<ModpackUpdateApplySummary, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de atualizar o modpack".to_owned());
    }
    let instance = layout(&app)?.open(id);
    if !instance.root().is_dir() {
        return Err("a instância não existe mais".to_owned());
    }
    let marker_path = instance.root().join("aurora-modpack.json");
    let marker = serde_json::from_slice::<serde_json::Value>(
        &fs::read(&marker_path)
            .map_err(|error| format!("não foi possível ler o perfil do modpack: {error}"))?,
    )
    .map_err(|_| "o perfil do modpack está corrompido".to_owned())?;
    if marker["source"].as_str() != Some("modrinth") {
        return Err(
            "a atualização automática de modpack está disponível para instalações Modrinth"
                .to_owned(),
        );
    }
    let minecraft_version = marker["minecraft"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "o perfil não informa a versão do Minecraft".to_owned())?
        .to_owned();
    let runtime_data_directory = data_directory(&app)?;
    emit_download_progress(&app, "Revalidando atualização do modpack", 1.0);
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
        apply_verified_modpack_update(&instance, Some(java.executable()), |mut progress| {
            progress.percent = 10.0 + progress.percent * 0.9;
            emit_modpack_progress(&progress_app, progress);
        })
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("a atualização do modpack falhou: {error}"))?
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
        || filename.len() > 255
        || filename == "."
        || filename == ".."
        || filename.ends_with(['.', ' '])
        || filename
            .chars()
            .any(|character| character.is_control() || "<>:\"/\\|?*".contains(character))
        || std::path::Path::new(filename)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(filename)
    {
        return Err("nome de arquivo de conteúdo inválido".to_owned());
    }
    let stem = filename
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(['.', ' '])
        .to_ascii_lowercase();
    if matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        || ((stem.starts_with("com") || stem.starts_with("lpt"))
            && stem.get(3..).is_some_and(|suffix| {
                suffix.len() == 1
                    && suffix
                        .chars()
                        .next()
                        .is_some_and(|digit| ('1'..='9').contains(&digit))
            }))
    {
        return Err("nome de arquivo de conteúdo inválido".to_owned());
    }
    Ok(filename)
}

#[tauri::command]
pub fn set_instance_content_enabled(
    app: AppHandle,
    running: tauri::State<'_, RunningInstances>,
    id: String,
    content_type: String,
    filename: String,
    enabled: bool,
) -> Result<(), String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de alterar o conteúdo".to_owned());
    }
    let folder = instance_content_folder(&content_type)?;
    let filename = safe_content_filename(&filename)?;
    let instance = layout(&app)?.open(id);
    let _mutation_lock =
        acquire_instance_mutation_lock(&instance).map_err(|error| error.to_string())?;
    let directory = instance.root().join(folder);
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
    running: tauri::State<'_, RunningInstances>,
    id: String,
    content_type: String,
    filenames: Vec<String>,
    enabled: bool,
) -> Result<usize, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de alterar o conteúdo".to_owned());
    }
    let folder = instance_content_folder(&content_type)?;
    let filenames = unique_content_filenames(filenames)?;
    let instance = layout(&app)?.open(id);
    let _mutation_lock =
        acquire_instance_mutation_lock(&instance).map_err(|error| error.to_string())?;
    let directory = instance.root().join(folder);
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
    running: tauri::State<'_, RunningInstances>,
    id: String,
    content_type: String,
    filenames: Vec<String>,
) -> Result<usize, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("feche o Minecraft antes de alterar o conteúdo".to_owned());
    }
    let folder = instance_content_folder(&content_type)?;
    let filenames = unique_content_filenames(filenames)?;
    let instance = layout(&app)?.open(id);
    let _mutation_lock =
        acquire_instance_mutation_lock(&instance).map_err(|error| error.to_string())?;
    let directory = instance.root().join(folder);
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
        if seen.insert(filename.to_lowercase()) {
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
    doctor_runs::check_path(instance.root())?;
    for filename in ["aurora-modpack.json", "aurora-instance.json"] {
        let path = instance.root().join(filename);
        let Ok(contents) = bounded_profile_read(&path) else {
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
    doctor_runs::check_path(&versions_dir)?;
    if let Ok(entries) = fs::read_dir(versions_dir) {
        let mut candidates = entries
            .take(512)
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| {
                let version_id = entry.file_name().to_string_lossy().into_owned();
                let profile_path = entry.path().join(format!("{version_id}.json"));
                let document = serde_json::from_slice::<serde_json::Value>(
                    &bounded_profile_read(&profile_path).ok()?,
                )
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
                let normalized_id = version_id.to_ascii_lowercase();
                let loader_priority = if !normalized_id.contains("neoforge")
                    && (normalized_id.contains("fabric") || normalized_id.contains("forge"))
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

fn bounded_profile_read(path: &Path) -> Result<Vec<u8>, String> {
    doctor_runs::check_path(path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    let file = options
        .open(path)
        .map_err(|_| "Perfil local indisponível.".to_owned())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if file
            .metadata()
            .map_err(|_| "Perfil local indisponível.")?
            .file_attributes()
            & 0x400
            != 0
        {
            return Err("O perfil local contém um caminho não autorizado.".into());
        }
    }
    if !file
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 2 * 1024 * 1024)
    {
        return Err("Perfil local excede o limite de leitura.".into());
    }
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Perfil local indisponível.".to_owned())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Perfil local excede o limite de leitura.".into());
    }
    Ok(bytes)
}

// A JVM can outlive the launcher. Detect it before launch/migration even when
// this process has no RunningInstances entry. Only a status is returned by the
// child process: command lines (which can contain credentials) never enter logs.
#[cfg(windows)]
fn ensure_no_external_instance_game(root: &Path) -> Result<(), String> {
    run_instance_game_probe(INSTANCE_GAME_PROBE, root, None)
}

const INSTANCE_GAME_RUNNING: &str = "Esta instância já está aberta no Minecraft, inclusive se o launcher que a iniciou foi fechado. Use outra instância para jogar com duas contas.";
const INSTANCE_GAME_UNAVAILABLE: &str = "Não foi possível confirmar que esta instância está livre. Feche o Minecraft/Java em execução e tente novamente.";

#[cfg(windows)]
const INSTANCE_GAME_PROBE: &str = r#"$ErrorActionPreference = 'Stop'
$instancePath = [IO.Path]::GetFullPath($env:AURORA_MIGRATION_INSTANCE).TrimEnd([char[]]'\/')
$games = Get-CimInstance Win32_Process -Filter "Name='java.exe' OR Name='javaw.exe'"
foreach ($game in $games) {
  if ([string]::IsNullOrEmpty($game.CommandLine)) { Write-Output 'UNKNOWN'; exit 0 }
  $match = [regex]::Match($game.CommandLine, '(?i)(?:^|\s)--gameDir(?:\s+|=)(?:"([^"]*)"|(\S+))')
  if (!$match.Success) { continue }
  $gamePath = if ($match.Groups[1].Success) { $match.Groups[1].Value } else { $match.Groups[2].Value }
  if (![IO.Path]::IsPathRooted($gamePath)) { Write-Output 'UNKNOWN'; exit 0 }
  $gamePath = [IO.Path]::GetFullPath($gamePath).TrimEnd([char[]]'\/')
  if ($gamePath -ieq $instancePath) { Write-Output 'RUNNING'; exit 0 }
}
Write-Output 'STOPPED'
"#;

#[cfg(windows)]
fn run_instance_game_probe(script: &str, root: &Path, fixture: Option<&str>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("powershell.exe");
    command.creation_flags(0x08000000);
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("AURORA_MIGRATION_INSTANCE", root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(fixture) = fixture {
        command.env("AURORA_QA_GAMES", fixture);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "não foi possível verificar se o Minecraft está encerrado".to_owned())?;
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if child
            .try_wait()
            .map_err(|_| "falha ao verificar processos Minecraft".to_owned())?
            .is_some()
        {
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(
                "tempo esgotado ao verificar se o Minecraft está encerrado; tente novamente".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let output = child
        .wait_with_output()
        .map_err(|_| "falha ao verificar processos Minecraft".to_owned())?;
    if output.status.success() {
        match String::from_utf8_lossy(&output.stdout).trim() {
            "STOPPED" => return Ok(()),
            "RUNNING" => return Err(INSTANCE_GAME_RUNNING.into()),
            _ => {}
        }
    }
    Err(INSTANCE_GAME_UNAVAILABLE.into())
}

#[cfg(not(windows))]
fn ensure_no_external_instance_game(_root: &Path) -> Result<(), String> {
    Err("a retirada automática do Companion requer verificação de processos disponível no Windows; arquivos preservados".into())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorRunEvent {
    instance_id: String,
    run_id: String,
    outcome: String,
}

fn doctor_directory(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_directory(app)?.join("doctor-runs"))
}

fn emit_doctor_run(app: &AppHandle, id: &str, run_id: &str, outcome: &str) {
    let _ = app.emit(
        "aurora-doctor-run",
        DoctorRunEvent {
            instance_id: id.into(),
            run_id: run_id.into(),
            outcome: outcome.into(),
        },
    );
}

#[tauri::command]
pub fn begin_instance_launch(
    app: AppHandle,
    running: tauri::State<RunningInstances>,
    id: String,
) -> Result<DoctorRunEvent, String> {
    let parsed = InstanceId::parse(&id).map_err(|_| "Identificador de instância inválido.")?;
    if instance_is_running(&running, &id)? {
        return Err("Esta instância já está rodando.".into());
    }
    let instance = layout(&app)?.open(parsed);
    // A second launcher must not replace the Doctor record of an active game.
    // Install/launch commands acquire their own guard after this preparation ends.
    let _mutation_lock = acquire_idle_instance(&instance)?;
    // The UI may install a loader after this command, before launch_instance.
    // Reject an orphaned Minecraft before either that install or Doctor writes.
    #[cfg(windows)]
    ensure_no_external_instance_game(instance.root())?;
    let records = app.state::<DoctorRuns>();
    let directory = doctor_directory(&app)?;
    if records
        .read(&directory, &id)?
        .is_some_and(|run| matches!(run.outcome.as_str(), "preparing" | "running"))
    {
        return Err("Uma inicialização desta instância já está em andamento.".into());
    }
    doctor_runs::check_path(instance.root())?;
    let profile = read_instance_launch_profile(app.clone(), id.clone())?;
    let run = DoctorRun {
        run_id: uuid::Uuid::new_v4().to_string(),
        started_at_ms: doctor_runs::now_ms(),
        ended_at_ms: None,
        outcome: "preparing".into(),
        exit_code: None,
        version_id: profile.version_id.unwrap_or_default(),
        minecraft_version: profile.minecraft_version.unwrap_or_default(),
        java_executable: String::new(),
        java_major: None,
        max_memory_mb: 4096,
        failure: None,
        logs_before: snapshot_logs(&instance),
    };
    let event = DoctorRunEvent {
        instance_id: id.clone(),
        run_id: run.run_id.clone(),
        outcome: run.outcome.clone(),
    };
    records.start(&directory, &id, run)?;
    emit_doctor_run(&app, &id, &event.run_id, "preparing");
    Ok(event)
}

#[tauri::command]
pub fn read_latest_doctor_run(
    app: AppHandle,
    running: tauri::State<RunningInstances>,
    id: String,
) -> Result<Option<DoctorRunEvent>, String> {
    InstanceId::parse(&id).map_err(|_| "Identificador de instância inválido.")?;
    let _ = instance_is_running(&running, &id)?;
    Ok(app
        .state::<DoctorRuns>()
        .read(&doctor_directory(&app)?, &id)?
        .map(|run| DoctorRunEvent {
            instance_id: id,
            run_id: run.run_id,
            outcome: run.outcome,
        }))
}

#[tauri::command]
pub fn fail_instance_launch(
    app: AppHandle,
    id: String,
    run_id: String,
    failure: String,
) -> Result<(), String> {
    finish_preparation(&app, &id, &run_id, "failed", Some(&failure))
}

#[tauri::command]
pub fn cancel_instance_launch(app: AppHandle, id: String, run_id: String) -> Result<(), String> {
    finish_preparation(&app, &id, &run_id, "cancelled", None)
}

fn finish_preparation(
    app: &AppHandle,
    id: &str,
    run_id: &str,
    outcome: &str,
    failure: Option<&str>,
) -> Result<(), String> {
    InstanceId::parse(id).map_err(|_| "Identificador de instância inválido.")?;
    let directory = doctor_directory(app)?;
    let records = app.state::<DoctorRuns>();
    let run = records
        .read(&directory, id)?
        .ok_or("Execução não encontrada.")?;
    // Frontend preparation errors must never overwrite an observed JVM result.
    if run.run_id != run_id {
        return Err("A execução solicitada já foi substituída.".into());
    }
    if run.outcome == "preparing" {
        records.finish(&directory, id, run_id, outcome, None, failure)?;
        emit_doctor_run(app, id, run_id, outcome);
    }
    Ok(())
}

#[tauri::command]
pub async fn analyze_instance(
    app: AppHandle,
    id: String,
    java_executable: Option<String>,
    run_id: Option<String>,
) -> Result<DoctorReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let parsed = InstanceId::parse(&id).map_err(|_| "Identificador de instância inválido.")?;
        let instance = layout(&app)?.open(parsed);
        doctor_runs::check_path(instance.root())?;
        let _ = instance_is_running(&app.state::<RunningInstances>(), &id)?;
        let run = app.state::<DoctorRuns>().read(&doctor_directory(&app)?, &id)?;
        if let Some(requested) = &run_id {
            if run.as_ref().is_none_or(|run| &run.run_id != requested) {
                return Err("Essa execução não é mais a última da instância. Atualize o diagnóstico para analisar a execução atual.".into());
            }
        }
        let profile = read_instance_launch_profile(app.clone(), id)?;
        let context = if run_id.is_some() {
            let run = run.as_ref().ok_or("Execução não encontrada.")?;
            DoctorContext { version_id: run.version_id.clone(), minecraft_version: run.minecraft_version.clone(), java_executable: Some(run.java_executable.clone()).filter(|value| !value.is_empty()), max_memory_mb: run.max_memory_mb }
        } else {
            DoctorContext { version_id: profile.version_id.unwrap_or_default(), minecraft_version: profile.minecraft_version.unwrap_or_default(), java_executable: java_executable.filter(|value| !value.trim().is_empty()), max_memory_mb: 4096 }
        };
        let observed_run_id = run.as_ref().map(|run| run.run_id.clone());
        let report = analyze_doctor(&instance, context, run)?;
        let current_run_id = app.state::<DoctorRuns>().read(&doctor_directory(&app)?, instance.id().as_str())?.map(|run| run.run_id);
        if current_run_id != observed_run_id {
            return Err("Uma nova execução começou durante a análise. Analise novamente.".into());
        }
        Ok(report)
    }).await.map_err(|_| "Não foi possível concluir a análise local. Tente novamente.".to_owned())?
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
    run_id: Option<String>,
) -> Result<LaunchSummary, String> {
    let run_id = match run_id {
        Some(run_id) => run_id,
        None => begin_instance_launch(app.clone(), running.clone(), id.clone())?.run_id,
    };
    let directory = doctor_directory(&app)?;
    let records = app.state::<DoctorRuns>();
    let mut run = records
        .read(&directory, &id)?
        .ok_or("Execução não encontrada.")?;
    if run.run_id != run_id || run.outcome != "preparing" {
        return Err("A tentativa de inicialização não está mais disponível.".into());
    }
    run.version_id = version_id.clone();
    run.minecraft_version = minecraft_version.clone();
    run.java_executable = java_executable.clone();
    records.save(&directory, &id, run.clone())?;
    let result = launch_instance_tracked(
        app.clone(),
        sessions,
        running,
        id.clone(),
        version_id,
        minecraft_version,
        java_executable,
        nickname,
        skin_url,
        skin_file,
        cape_url,
        skin_model,
        &mut run,
    );
    match result {
        Ok(summary) => {
            run.outcome = "running".into();
            let _ = records.save(&directory, &id, run);
            if let Ok(Some(observed)) = records.read(&directory, &id) {
                emit_doctor_run(&app, &id, &observed.run_id, &observed.outcome);
            }
            Ok(summary)
        }
        Err(error) => {
            // Persist only recognized, allowlisted evidence; never a raw credential/path.
            let _ = records.save(&directory, &id, run);
            let _ = records.finish(&directory, &id, &run_id, "failed", None, Some(&error));
            emit_doctor_run(&app, &id, &run_id, "failed");
            // These exact native statuses contain no path, command line or
            // credential; preserve the actionable explanation for another game.
            Err(match error.as_str() {
                INSTANCE_GAME_RUNNING | INSTANCE_GAME_UNAVAILABLE => error,
                _ => canonical_launch_failure(&error),
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn launch_instance_tracked(
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
    doctor_run: &mut DoctorRun,
) -> Result<LaunchSummary, String> {
    let nickname = nickname.trim().to_owned();
    validate_nickname(&nickname).map_err(|error| error.to_string())?;
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    if instance_is_running(&running, id.as_str())? {
        return Err("esta instância já está rodando".to_owned());
    }
    let instance_id = id.as_str().to_owned();
    let instance = layout(&app)?.open(id);
    let mutation_lock =
        acquire_instance_mutation_lock(&instance).map_err(|error| error.to_string())?;
    // The mutation lock belongs to the launcher process, but its Minecraft can
    // survive that process. Check the game directory before replacing any JAR.
    #[cfg(windows)]
    ensure_no_external_instance_game(instance.root())?;
    if companion_migration_required(&instance).map_err(|error| error.to_string())? {
        #[cfg(not(windows))]
        ensure_no_external_instance_game(instance.root())?;
        migrate_managed_companion(&instance, &mutation_lock, false)
            .map_err(|error| error.to_string())?;
    }
    let java = JavaRuntime::from_executable(java_executable).map_err(|error| error.to_string())?;
    let uuid = offline_uuid_for_nickname(&nickname).map_err(|error| error.to_string())?;
    let mut spec =
        resolve_launch_spec(&instance, &version_id).map_err(|error| error.to_string())?;
    spec.apply_offline_compatibility(&minecraft_version);
    let loader = loader_for_profile(&version_id);
    let mut ipc_server = None;
    let mut core_installed = false;
    let mut skins_installed = false;
    if let Some(loader) = loader {
        install_embedded_core(&instance, &minecraft_version, loader)
            .map_err(|error| error.to_string())?;
        core_installed = true;
        install_embedded_skins(&instance, &minecraft_version, loader)
            .map_err(|error| error.to_string())?;
        skins_installed = true;
        let required_java = required_java_major_for_instance(&instance, &minecraft_version);
        let selected_java = java.major_version().map_err(|error| error.to_string())?;
        doctor_run.java_major = Some(selected_java);
        let baseline_java = required_java_major(&minecraft_version);
        let compatible = if required_java > baseline_java {
            selected_java >= required_java
        } else {
            selected_java == required_java
        };
        if !compatible {
            return Err(format!(
                "esta instância precisa do Java {required_java}, mas o Java {selected_java} foi selecionado"
            ));
        }
        let server =
            IpcServer::start_with_session(Some(IpcSessionProfile::offline(uuid, nickname.clone())))
                .map_err(|error| error.to_string())?;
        spec.aurora_ipc_port = Some(server.endpoint().port);
        spec.aurora_session_nonce = Some(server.endpoint().nonce.clone());
        spec.jvm_args.push(format!("-Daurora.loader={loader}"));
        let skins_root = data_directory(&app)?.join("skins").join(uuid.to_string());
        spec.jvm_args
            .push(format!("-Daurora.skins.root={}", skins_root.display()));
        if let Some(url) = public_profile_url(skin_url.as_deref()) {
            spec.jvm_args
                .push(format!("-Daurora.profile.skinUrl={url}"));
        }
        if let Some(path) = local_appearance_path(&app, skin_file.as_deref(), "skin") {
            spec.jvm_args.push(format!(
                "-Daurora.profile.skinFile={}",
                java_property_path(&path)
            ));
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
    }
    let identity = LaunchIdentity::offline(nickname, uuid);
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
    processes.insert(
        process_id,
        RunningInstance {
            doctor: Some((app.clone(), doctor_run.run_id.clone())),
            instance_id,
            child,
            _mutation_lock: mutation_lock,
        },
    );
    Ok(LaunchSummary {
        run_id: doctor_run.run_id.clone(),
        process_id,
        version_id,
        core_installed,
        skins_installed,
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

fn java_property_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    #[cfg(target_os = "windows")]
    {
        if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{unc}");
        }
        if let Some(local) = value.strip_prefix(r"\\?\") {
            return local.to_owned();
        }
    }
    value.into_owned()
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
    if version_id.contains("neoforge") {
        None
    } else if version_id.contains("fabric") {
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
pub fn discover_java(app: AppHandle) -> Vec<JavaRuntimeSummary> {
    let mut executables = discover_java_executables()
        .into_iter()
        .collect::<BTreeSet<_>>();
    if let Ok(directory) = data_directory(&app) {
        executables.extend(discover_managed_java_executables(&directory));
    }
    executables
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
pub fn required_java_for_instance(
    app: AppHandle,
    id: String,
    minecraft_version: String,
) -> Result<u32, String> {
    let id = InstanceId::parse(id).map_err(|error| error.to_string())?;
    let instance = layout(&app)?.open(id);
    Ok(required_java_major_for_instance(
        &instance,
        &minecraft_version,
    ))
}

#[tauri::command]
pub async fn available_java_versions() -> Result<JavaReleaseCatalog, String> {
    tauri::async_runtime::spawn_blocking(available_java_releases)
        .await
        .map_err(|error| format!("a consulta de versões Java falhou: {error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn install_java_major(app: AppHandle, major: u32) -> Result<JavaRuntimeSummary, String> {
    if !supported_managed_java_major(major) {
        return Err("escolha uma versão do Java entre 8 e 30".to_owned());
    }
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
    .map_err(|error| format!("a instalação do Java falhou: {error}"))?
}

#[tauri::command]
pub async fn ensure_java(
    app: AppHandle,
    minecraft_version: String,
    required_major: Option<u32>,
) -> Result<JavaRuntimeSummary, String> {
    let major = required_major.unwrap_or_else(|| required_java_major(&minecraft_version));
    if !matches!(major, 8 | 17 | 21) {
        return Err(format!("Java {major} não é suportado pelo Aurora"));
    }
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
    #[cfg(windows)]
    #[test]
    fn process_probe_detects_orphan_game_without_matching_neighbor_instance() {
        let script = super::INSTANCE_GAME_PROBE.replace(
            "Get-CimInstance Win32_Process -Filter \"Name='java.exe' OR Name='javaw.exe'\"",
            "ConvertFrom-Json $env:AURORA_QA_GAMES",
        );
        let root = std::path::Path::new(r"C:\Aurora QA\instances\foo");
        for (line, stopped) in [
            (
                r#"java -cp "C:\Aurora QA\instances\foo\library.jar" --gameDir "C:\Aurora QA\instances\foo-copy""#,
                true,
            ),
            (r#"java --gameDir "C:\Aurora QA\instances\foo""#, false),
            (r#"java --gameDir "c:/aurora qa/instances/foo/""#, false),
            (r#"java --gameDir C:\other\instance"#, true),
        ] {
            let fixture = serde_json::json!([{"CommandLine": line}]).to_string();
            assert_eq!(
                super::run_instance_game_probe(&script, root, Some(&fixture)).is_ok(),
                stopped
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn process_probe_fails_closed_when_a_java_command_line_cannot_be_read() {
        let script = super::INSTANCE_GAME_PROBE.replace(
            "Get-CimInstance Win32_Process -Filter \"Name='java.exe' OR Name='javaw.exe'\"",
            "ConvertFrom-Json $env:AURORA_QA_GAMES",
        );
        let root = std::path::Path::new(r"C:\Aurora QA\instances\foo");
        let fixture = serde_json::json!([{ "CommandLine": null }]).to_string();
        let error = super::run_instance_game_probe(&script, root, Some(&fixture)).unwrap_err();
        assert!(error.contains("Não foi possível confirmar"));
        assert!(!error.contains("migração"));
    }

    use std::fs;

    use super::{
        finalize_manual_download, is_curseforge_host, java_property_path, png_dimensions,
        unique_content_filenames, validate_content_filename,
    };

    #[test]
    fn game_tracking_keeps_mutations_locked_until_process_cleanup() {
        use aurora_launcher_core::engine::{ContentUpdateError, InstanceId, InstanceLayout};
        use std::process::{Command, Stdio};

        let directory =
            std::env::temp_dir().join(format!("aurora-game-lock-test-{}", uuid::Uuid::new_v4()));
        let instance =
            InstanceLayout::new(&directory).open(InstanceId::parse("locked-game").unwrap());
        instance.ensure_layout().unwrap();
        let guard = super::acquire_instance_mutation_lock(&instance).unwrap();
        // Listing the test binary exits without running tests or starting a game.
        let child = Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let process_id = child.id();
        let mut processes = std::collections::BTreeMap::new();
        processes.insert(
            process_id,
            super::RunningInstance {
                doctor: None,
                instance_id: "locked-game".to_owned(),
                child,
                _mutation_lock: guard,
            },
        );
        assert!(matches!(
            super::acquire_instance_mutation_lock(&instance),
            Err(ContentUpdateError::Busy)
        ));
        processes
            .get_mut(&process_id)
            .unwrap()
            .child
            .wait()
            .unwrap();
        assert_eq!(
            super::prune_finished_processes(&mut processes),
            vec![process_id]
        );
        let released = super::acquire_instance_mutation_lock(&instance).unwrap();
        drop(released);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn other_launcher_cannot_prepare_or_modify_a_locked_game_directory() {
        use aurora_launcher_core::engine::{InstanceId, InstanceLayout};
        let directory = std::env::temp_dir().join(format!(
            "aurora-other-launcher-lock-test-{}",
            uuid::Uuid::new_v4()
        ));
        let instance =
            InstanceLayout::new(&directory).open(InstanceId::parse("shared-instance").unwrap());
        instance.ensure_layout().unwrap();
        let game_guard = super::acquire_instance_mutation_lock(&instance).unwrap();
        let error = match super::acquire_idle_instance(&instance) {
            Ok(_) => panic!("another launcher must not acquire an active instance"),
            Err(error) => error,
        };
        assert!(error.contains("outro jogo ou launcher"));
        assert!(error.contains("outra instância"));
        drop(game_guard);

        // Renaming/removing an idle instance still works while its guard is held.
        let idle_guard = super::acquire_idle_instance(&instance).unwrap();
        let renamed = directory.join("instances/renamed-instance");
        fs::rename(instance.root(), &renamed).unwrap();
        fs::remove_dir_all(&renamed).unwrap();
        drop(idle_guard);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn removes_windows_verbatim_prefix_before_passing_paths_to_java() {
        assert_eq!(
            java_property_path(std::path::Path::new(r"\\?\C:\Aurora\skin.png")),
            r"C:\Aurora\skin.png"
        );
        assert_eq!(
            java_property_path(std::path::Path::new(r"\\?\UNC\server\share\skin.png")),
            r"\\server\share\skin.png"
        );
    }

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
        assert!(validate_content_filename("modpack.zip", "modpack").is_ok());
        assert!(validate_content_filename("../example.jar", "mod").is_err());
        assert!(validate_content_filename("example.exe", "mod").is_err());
        assert!(validate_content_filename("bad:stream.jar", "mod").is_err());
        assert!(validate_content_filename("CON.jar", "mod").is_err());
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
        fs::write(&destination, b"previous file").unwrap();
        fs::write(&temporary, b"hello").expect("arquivo temporário");
        assert!(finalize_manual_download(
            Some(&temporary),
            &temporary,
            &destination,
            "0000000000000000000000000000000000000000",
        )
        .is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"previous file");
        assert!(finalize_manual_download(
            Some(&destination),
            &temporary,
            &destination,
            "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d",
        )
        .is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"previous file");
        finalize_manual_download(
            Some(&temporary),
            &temporary,
            &destination,
            "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d",
        )
        .expect("hash correto deve publicar o arquivo");
        assert_eq!(fs::read(&destination).expect("arquivo final"), b"hello");
        assert!(!temporary.exists());
        fs::remove_dir_all(directory).expect("limpeza temporária");
    }

    #[test]
    fn manual_download_failed_publication_does_not_delete_destination() {
        let directory =
            std::env::temp_dir().join(format!("aurora-publish-test-{}", uuid::Uuid::new_v4()));
        let destination = directory.join("existing-folder");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("keep.txt"), b"keep").unwrap();
        let temporary = directory.join("download.part");
        fs::write(&temporary, b"hello").unwrap();
        assert!(finalize_manual_download(
            None,
            &temporary,
            &destination,
            "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"
        )
        .is_err());
        assert_eq!(fs::read(destination.join("keep.txt")).unwrap(), b"keep");
        assert!(temporary.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
