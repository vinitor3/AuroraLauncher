// Packaged Windows builds are GUI apps; development keeps its existing console.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod doctor_runs;
mod launcher_sessions;
mod manual_downloads;
mod player_profiles;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

const MAIN_TRAY_ID: &str = "aurora-main-tray";

fn show_launcher(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn create_launcher_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open-launcher", "Abrir Aurora", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(
        app,
        "quit-launcher",
        "Fechar o launcher",
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&open, &separator, &quit])?;

    TrayIconBuilder::with_id(MAIN_TRAY_ID)
        .icon(tauri::include_image!("icons/32x32.png"))
        .tooltip("Aurora Smart Launcher • Clique para abrir")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open-launcher" => show_launcher(app),
            // Exit the launcher explicitly. Minecraft processes are independent
            // and retain their existing lifecycle; no game is terminated here.
            "quit-launcher" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_launcher(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            if create_launcher_tray(app).is_err() {
                // On systems without a tray, retain normal closing behavior
                // instead of hiding the only way to reach the launcher.
                eprintln!(
                    "A bandeja do sistema não está disponível; a janela fechará normalmente."
                );
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    if window.app_handle().tray_by_id(MAIN_TRAY_ID).is_some()
                        && window.hide().is_ok()
                    {
                        api.prevent_close();
                    }
                }
            }
        })
        .manage(commands::IpcSessions::default())
        .manage(commands::RunningInstances::default())
        .manage(launcher_sessions::LauncherSessionState::default())
        .manage(doctor_runs::DoctorRuns::default())
        .manage(manual_downloads::ManualDownloads::default())
        .manage(aurora_launcher_core::engine::ai_tools::AiToolBroker::default())
        .invoke_handler(tauri::generate_handler![
            launcher_sessions::launcher_auth_scope,
            player_profiles::lookup_player_skin,
            commands::engine_status,
            commands::list_instances,
            commands::create_instance,
            commands::rename_instance,
            commands::set_instance_presentation,
            commands::delete_instance,
            commands::recovery_list,
            commands::recovery_create,
            commands::recovery_details,
            commands::recovery_delete,
            commands::recovery_preview,
            commands::recovery_preview_undo,
            commands::recovery_restore,
            commands::doctor_recovery_overview,
            commands::diagnostic_lab_list,
            commands::diagnostic_lab_create,
            commands::diagnostic_lab_worlds,
            commands::diagnostic_lab_details,
            commands::diagnostic_lab_toggle_mod,
            commands::diagnostic_lab_set_setting,
            commands::diagnostic_lab_preview_apply,
            commands::diagnostic_lab_apply,
            commands::diagnostic_lab_delete,
            commands::install_vanilla,
            commands::install_fabric,
            commands::install_forge,
            commands::list_modrinth_categories,
            commands::search_modrinth_modpacks,
            commands::install_modrinth_modpack,
            commands::prepare_curseforge_modpack,
            commands::install_curseforge_modpack,
            commands::search_modrinth_content,
            commands::install_modrinth_content,
            commands::install_remote_content,
            commands::scan_instance_content_updates,
            commands::apply_instance_content_updates,
            commands::apply_remote_content_updates,
            commands::rollback_last_content_update,
            commands::scan_instance_modpack_update,
            commands::apply_instance_modpack_update,
            commands::open_manual_content_download,
            commands::cancel_manual_content_download,
            commands::resolve_content_artwork,
            commands::list_instance_content,
            commands::set_instance_content_enabled,
            commands::set_instance_content_enabled_bulk,
            commands::remove_instance_content,
            commands::open_instance_folder,
            commands::read_instance_log,
            commands::read_instance_launch_profile,
            commands::launch_instance,
            commands::begin_instance_launch,
            commands::fail_instance_launch,
            commands::cancel_instance_launch,
            commands::read_latest_doctor_run,
            commands::analyze_instance,
            commands::ai_tools_context,
            commands::prepare_ai_tool,
            commands::confirm_ai_tool,
            commands::revoke_ai_tool_plan,
            commands::prepare_ai_tool_undo,
            commands::ai_tool_history,
            commands::ai_record_advice,
            commands::list_running_instances,
            commands::poll_ipc_events,
            commands::synthesize_speech,
            commands::validate_appearance_url,
            commands::load_appearance_url,
            commands::load_local_appearance,
            commands::save_local_appearance,
            commands::verify_java,
            commands::read_launch_settings,
            commands::save_launch_settings,
            commands::discover_java,
            commands::required_java_for_instance,
            commands::available_java_versions,
            commands::install_java_major,
            commands::ensure_java,
            commands::offline_uuid,
            commands::load_firebase_config,
            commands::save_firebase_config,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao executar o Aurora Smart Launcher");
}
