use super::{
    acquire_idle_instance, ensure_no_external_instance_game, layout, list_running_instances,
    IpcSessions, RunningInstances,
};
use aurora_launcher_core::engine::{
    world_backups as backups, Instance, InstanceId, InstanceMutationGuard,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

fn instance(app: &AppHandle, id: String) -> Result<Instance, String> {
    let instance = layout(app)?.open(InstanceId::parse(id).map_err(|_| "Instância inválida.")?);
    if !instance.root().is_dir() {
        return Err("Instância indisponível.".into());
    }
    Ok(instance)
}
pub(super) fn arm(instance: &Instance) {
    if let Err(e) = backups::arm(instance) {
        eprintln!("Aurora: {e}");
    }
}
pub(super) fn finish(instance: &Instance, guard: &InstanceMutationGuard) {
    // Recheck externally launched games before reading any save. The transferred OS lock
    // also excludes a new Aurora launch until the SQLite transaction finishes.
    if let Err(e) = backups::after_exit_if_idle(instance, guard, || {
        ensure_no_external_instance_game(instance.root()).is_ok()
    }) {
        eprintln!("Aurora: {e}");
    }
}
pub(super) fn catch_up_before_launch(instance: &Instance, guard: &InstanceMutationGuard) {
    if backups::needs_resume(instance).unwrap_or(false) {
        if let Err(e) = backups::after_exit(instance, guard) {
            eprintln!("Aurora: {e}");
        }
    }
}
fn resume(app: &AppHandle) {
    let Ok(root) = super::data_directory(app) else {
        return;
    };
    if !backups::any_pending(&root).unwrap_or(false) {
        return;
    }
    let layout = aurora_launcher_core::engine::InstanceLayout::new(root);
    let Ok(entries) = std::fs::read_dir(layout.instances_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(id) = InstanceId::parse(name) else {
            continue;
        };
        let instance = layout.open(id);
        if !backups::needs_resume(&instance).unwrap_or(false) {
            continue;
        }
        if let Ok(guard) = acquire_idle_instance(&instance) {
            finish(&instance, &guard);
        }
    }
}
pub(crate) fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last_scan = None;
        loop {
            let should_scan =
                last_scan.is_none_or(|t: Instant| t.elapsed() >= Duration::from_secs(60));
            if should_scan {
                last_scan = Some(Instant::now());
            }
            let owner = app.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || {
                let _ = list_running_instances(
                    owner.state::<RunningInstances>(),
                    owner.state::<IpcSessions>(),
                );
                if should_scan {
                    resume(&owner);
                }
            })
            .await;
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}
#[tauri::command]
pub async fn world_backup_overview(
    app: AppHandle,
    id: String,
) -> Result<backups::Overview, String> {
    let instance = instance(&app, id)?;
    tauri::async_runtime::spawn_blocking(move || {
        backups::overview(&instance).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Consulta interrompida.")?
}
#[tauri::command]
pub async fn world_backup_configure(
    app: AppHandle,
    id: String,
    global: bool,
    enabled: Option<bool>,
) -> Result<(), String> {
    let instance = instance(&app, id)?;
    tauri::async_runtime::spawn_blocking(move || {
        backups::configure(&instance, global, enabled).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Configuração interrompida.")?
}
#[tauri::command]
pub async fn world_backup_restore(
    app: AppHandle,
    id: String,
    version: String,
) -> Result<String, String> {
    let instance = instance(&app, id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let guard = acquire_idle_instance(&instance)?;
        ensure_no_external_instance_game(instance.root())?;
        backups::restore_copy(&instance, &guard, &version).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Restauração interrompida.")?
}
#[tauri::command]
pub async fn world_backup_retry(app: AppHandle, id: String) -> Result<(), String> {
    let instance = instance(&app, id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let guard = acquire_idle_instance(&instance)?;
        ensure_no_external_instance_game(instance.root())?;
        backups::after_exit(&instance, &guard).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Backup interrompido.")?
}
