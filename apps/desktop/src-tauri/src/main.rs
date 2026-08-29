mod commands;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(commands::IpcSessions::default())
        .invoke_handler(tauri::generate_handler![
            commands::engine_status,
            commands::list_instances,
            commands::create_instance,
            commands::delete_instance,
            commands::install_vanilla,
            commands::install_fabric,
            commands::install_forge,
            commands::search_modrinth_modpacks,
            commands::install_modrinth_modpack,
            commands::search_modrinth_content,
            commands::install_modrinth_content,
            commands::install_remote_content,
            commands::open_manual_content_download,
            commands::resolve_content_artwork,
            commands::list_instance_content,
            commands::set_instance_content_enabled,
            commands::set_instance_content_enabled_bulk,
            commands::remove_instance_content,
            commands::open_instance_folder,
            commands::read_instance_log,
            commands::read_instance_launch_profile,
            commands::launch_instance,
            commands::poll_ipc_events,
            commands::toggle_ipc_assistant,
            commands::send_ipc_assistant_response,
            commands::send_ipc_caption,
            commands::send_ipc_transcript,
            commands::synthesize_speech,
            commands::validate_appearance_url,
            commands::save_local_appearance,
            commands::verify_java,
            commands::discover_java,
            commands::ensure_java,
            commands::offline_uuid,
            commands::load_firebase_config,
            commands::save_firebase_config,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao executar o Aurora Smart Launcher");
}
