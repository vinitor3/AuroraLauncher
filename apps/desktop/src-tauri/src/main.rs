// Packaged Windows builds are GUI apps; development keeps its existing console.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod manual_downloads;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(commands::IpcSessions::default())
        .manage(commands::RunningInstances::default())
        .manage(manual_downloads::ManualDownloads::default())
        .invoke_handler(tauri::generate_handler![
            commands::engine_status,
            commands::list_instances,
            commands::create_instance,
            commands::rename_instance,
            commands::set_instance_presentation,
            commands::delete_instance,
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
            commands::list_running_instances,
            commands::poll_ipc_events,
            commands::synthesize_speech,
            commands::validate_appearance_url,
            commands::load_appearance_url,
            commands::load_local_appearance,
            commands::save_local_appearance,
            commands::verify_java,
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
