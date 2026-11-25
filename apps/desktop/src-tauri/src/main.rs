// Packaged Windows builds are GUI apps; development keeps its existing console.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod commands_native_voice;
mod connect_commands;
mod discord;
mod doctor_runs;
mod launcher_sessions;
mod manual_downloads;
mod player_profiles;
mod themes;
mod voice_announcement_commands;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, RunEvent, WindowEvent,
};

const MAIN_TRAY_ID: &str = "aurora-main-tray";

#[cfg(any(feature = "connect-lab", feature = "connect-voice-lab"))]
#[derive(Default)]
struct ExperimentalExitState {
    closing: std::sync::atomic::AtomicBool,
    ready: std::sync::atomic::AtomicBool,
}

fn show_launcher(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// The same exit request as the tray. ExitRequested remains the sole owner of
/// voice approval and Connect shutdown; this command never kills the process.
#[tauri::command]
fn launcher_request_exit(window: tauri::WebviewWindow) -> Result<(), &'static str> {
    if window.label() != "main" {
        return Err("launcher_wrong_window");
    }
    window.app_handle().exit(0);
    Ok(())
}

fn create_launcher_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open-launcher", "Abrir Aurora", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let reset_theme = MenuItem::with_id(
        app,
        "reset-theme",
        "Restaurar aparência padrão",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        "quit-launcher",
        "Fechar o launcher",
        true,
        None::<&str>,
    )?;
    let menu = Menu::with_items(app, &[&open, &reset_theme, &separator, &quit])?;

    TrayIconBuilder::with_id(MAIN_TRAY_ID)
        .icon(tauri::include_image!("icons/32x32.png"))
        .tooltip("Aurora Smart Launcher • Clique para abrir")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open-launcher" => show_launcher(app),
            "reset-theme" => themes::restore_from_tray(app),
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
        .setup(|app| {
            // Only the isolated debug social profile accepts its parent's public
            // authority pin. It is never read from WebView state or an IPC call.
            let connect = {
                #[cfg(all(debug_assertions, feature = "connect-lab"))]
                {
                    use aurora_launcher_core::engine::connect::{
                        ConnectManager, SocialLabAuthority,
                    };
                    if app.config().identifier == "com.aurora.smartlauncher.sociallab" {
                        std::env::var("AURORA_SOCIAL_AUTHORITY_PIN")
                            .ok()
                            .zip(std::env::var("AURORA_SOCIAL_AUTHORITY_ISSUER").ok())
                            .and_then(|(pin, issuer)| {
                                SocialLabAuthority::from_local_process(&pin, &issuer).ok()
                            })
                            .map(ConnectManager::with_social_lab)
                            .unwrap_or_default()
                    } else {
                        SocialLabAuthority::from_compiled_online()
                            .map(ConnectManager::with_social_lab)
                            .unwrap_or_default()
                    }
                }
                #[cfg(all(not(debug_assertions), feature = "connect-lab"))]
                {
                    use aurora_launcher_core::engine::connect::{ConnectManager, SocialLabAuthority};
                    SocialLabAuthority::from_compiled_online()
                        .map(ConnectManager::with_social_lab)
                        .unwrap_or_default()
                }
                #[cfg(not(feature = "connect-lab"))]
                {
                    aurora_launcher_core::engine::connect::ConnectManager::default()
                }
            };
            #[cfg(feature = "connect-lab")]
            let connect = match app.path().app_data_dir() {
                Ok(data) => {
                    let recorder = doctor_runs::ConnectTerminalRecorder::new(data.join("doctor-runs").join("connect"));
                    connect.with_terminal_recorder(move |diagnostic| recorder.submit(diagnostic))
                }
                Err(_) => connect,
            };
            app.manage(connect);
            app.manage(aurora_launcher_core::engine::connect_native_voice::NativeVoiceSessionManager::default());
            #[cfg(any(feature = "connect-lab", feature = "connect-voice-lab"))]
            app.manage(ExperimentalExitState::default());
            themes::start_watchdog(app.handle().clone());
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
        .manage(discord::DiscordState::default())
        .manage(commands::RunningInstances::default())
        .manage(launcher_sessions::LauncherSessionState::default())
        .manage(doctor_runs::DoctorRuns::default())
        .manage(manual_downloads::ManualDownloads::default())
        .manage(themes::ThemeState::default())
        .manage(aurora_launcher_core::engine::ai_tools::AiToolBroker::default())
        .manage(aurora_launcher_core::engine::connect_discovery::DiscoveryManager::default())
        .manage(
            aurora_launcher_core::engine::connect_voice_desktop::VoiceAnnouncementManager::default(
            ),
        )
        .invoke_handler(tauri::generate_handler![
            discord::discord_default_application_id,
            discord::discord_set_presence,
            discord::discord_clear_presence,
            voice_announcement_commands::voice_announcement_capabilities,
            voice_announcement_commands::voice_announcement_snapshot,
            voice_announcement_commands::voice_announcement_inspect,
            voice_announcement_commands::voice_announcement_prepare_apply,
            voice_announcement_commands::voice_announcement_prepare_apply_durable,
            voice_announcement_commands::voice_announcement_list_receipts,
            voice_announcement_commands::voice_announcement_resume_receipt,
            voice_announcement_commands::voice_announcement_prepare_retirement,
            voice_announcement_commands::voice_announcement_confirm_retirement,
            voice_announcement_commands::voice_announcement_confirm,
            voice_announcement_commands::voice_announcement_prepare_restore,
            voice_announcement_commands::voice_announcement_prepare_restore_reconciled,
            voice_announcement_commands::voice_announcement_cancel_review,
            voice_announcement_commands::voice_announcement_begin_close,
            voice_announcement_commands::voice_announcement_cancel_close,
            voice_announcement_commands::voice_announcement_confirm_close,
            voice_announcement_commands::voice_announcement_finish_close,
            connect_commands::connect_capabilities,
            commands_native_voice::native_voice_snapshot,
            commands_native_voice::native_voice_execute,
            commands_native_voice::native_voice_social_reserve,
            connect_commands::connect_social_capabilities,
            connect_commands::connect_social_observe_sessions,
            connect_commands::connect_social_reserve,
            connect_commands::connect_social_reserve_discovered,
            connect_commands::connect_social_authenticate,
            connect_commands::connect_social_refresh_authentication,
            connect_commands::connect_social_revoke_participant,
            connect_commands::connect_social_confirm_host,
            connect_commands::connect_social_confirm_guest,
            connect_commands::connect_account_transition_stop,
            connect_commands::connect_prove_social_key_binding,
            connect_commands::connect_discover_instance,
            connect_commands::connect_review_discovered,
            connect_commands::connect_confirm_discovered_status,
            connect_commands::connect_prepare_discovered_host,
            connect_commands::connect_prepare_session_host,
            connect_commands::connect_prepare_fragmented_host,
            connect_commands::connect_prepare_fragmented_guest,
            connect_commands::connect_prepare_session_fragmented_host,
            connect_commands::connect_prepare_session_fragmented_host_discovered,
            connect_commands::connect_prepare_session_fragmented_guest,
            connect_commands::connect_prepare_session_group_host,
            connect_commands::connect_prepare_session_group_host_discovered,
            connect_commands::connect_review_session_group_host,
            connect_commands::connect_confirm_session_group_host,
            connect_commands::connect_export_session_group_invitation,
            connect_commands::connect_revoke_session_group_participant,
            connect_commands::connect_retry_session_udp,
            connect_commands::connect_prepare_ready_game_launch,
            connect_commands::connect_confirm_ready_game_launch,
            connect_commands::connect_cancel_ready_game_launch,
            connect_commands::connect_ready_game_launch_snapshot,
            connect_commands::connect_install_ready_copy,
            connect_commands::connect_cancel_ready_copy_install,
            connect_commands::connect_prepare_session_guest,
            connect_commands::connect_prepare_session_host_discovered,
            connect_commands::connect_snapshot,
            connect_commands::connect_prepare_host,
            connect_commands::connect_review_host,
            connect_commands::connect_prepare_guest,
            connect_commands::connect_review_guest,
            connect_commands::connect_confirm,
            connect_commands::connect_export_invitation,
            connect_commands::connect_probe_status,
            connect_commands::connect_stop,
            launcher_sessions::launcher_auth_scope,
            launcher_request_exit,
            themes::themes_load,
            themes::themes_save,
            themes::themes_delete,
            themes::themes_preview,
            themes::themes_confirm,
            themes::themes_reset,
            themes::themes_heartbeat,
            themes::themes_set_automatic,
            themes::themes_unbind,
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
            commands::instance_keymap,
            commands::rebind_instance_key,
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
            commands::confirm_ai_tool,
            commands::revoke_ai_tool_plan,
            commands::prepare_ai_tool_undo,
            commands::ai_tool_history,
            commands::list_running_instances,
            commands::poll_ipc_events,
            commands::validate_appearance_url,
            commands::load_appearance_url,
            commands::load_local_appearance,
            commands::save_local_appearance,
            commands::verify_java,
            commands::read_global_memory,
            commands::save_global_memory,
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
        .build(tauri::generate_context!())
        .expect("erro ao preparar o Aurora Smart Launcher")
        .run(|app, event| {
            if matches!(event, RunEvent::Exit) {
                themes::shutdown(app);
            }
            #[cfg(any(feature = "connect-lab", feature = "connect-voice-lab"))]
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                use std::sync::atomic::Ordering;
                use tauri::Emitter;
                type VoiceManager =
                    aurora_launcher_core::engine::connect_voice_desktop::VoiceAnnouncementManager;
                let exit = app.state::<ExperimentalExitState>();
                if exit.ready.load(Ordering::Acquire) {
                    return;
                }
                api.prevent_exit();
                if !exit.closing.swap(true, Ordering::AcqRel) {
                    let app = app.clone();
                    let exit_code = code.unwrap_or(0);
                    tauri::async_runtime::spawn(async move {
                        let voice = app.state::<VoiceManager>();
                        let outcome = voice.begin_exit().await;
                        if outcome.is_ok() && voice.exit_ready() {
                            #[cfg(all(windows, feature = "connect-lab"))]
                            commands::ready_game::shutdown_ready_copy_install().await;
                            // Ready+Exit is sealed: no cancel/finish/new writer
                            // can reopen voice admission while this await runs.
                            app.state::<aurora_launcher_core::engine::connect_native_voice::NativeVoiceSessionManager>()
                                .shutdown_all()
                                .await;
                            app.state::<aurora_launcher_core::engine::connect::ConnectManager>()
                                .shutdown_all()
                                .await;
                            if voice.exit_ready() {
                                app.state::<ExperimentalExitState>()
                                    .ready
                                    .store(true, Ordering::Release);
                                app.exit(exit_code);
                                return;
                            }
                        }
                        app.state::<ExperimentalExitState>()
                            .closing
                            .store(false, Ordering::Release);
                        // Approval may have arrived after the first readiness
                        // check while ExitRequested was already being handled.
                        // Re-enter rather than lose that explicit exit request.
                        if voice.exit_ready() && outcome.is_ok() {
                            app.exit(exit_code);
                            return;
                        }
                        show_launcher(&app);
                        match voice.snapshot("main") {
                            Ok(snapshot) if snapshot.close.is_some() => {
                                if let Some(close) = snapshot.close {
                                    let _ = app.emit_to(
                                        "main",
                                        "aurora-voice-close-attention",
                                        serde_json::json!({"closeId": close.close_id}),
                                    );
                                }
                            }
                            _ => {
                                let code =
                                    outcome.err().map_or("voice_internal", |error| error.code());
                                let _ = app.emit_to(
                                    "main",
                                    "aurora-voice-close-blocked",
                                    serde_json::json!({"code": code}),
                                );
                            }
                        }
                    });
                }
            }
            #[cfg(not(any(feature = "connect-lab", feature = "connect-voice-lab")))]
            let _ = (app, event);
        });
}
