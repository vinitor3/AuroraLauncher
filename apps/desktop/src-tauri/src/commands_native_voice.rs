//! Local-window native voice caller. Destinations and source handles stay native.
use aurora_launcher_core::engine::{
    connect::ConnectManager,
    connect_native_voice::{NativeVoiceRequest, NativeVoiceSessionManager, NativeVoiceSnapshot},
};
use tauri::{Manager, State, WebviewWindow};
fn owner(window: &WebviewWindow) -> Result<&str, &'static str> {
    if window.label() != "main" {
        return Err("connect_owner_mismatch");
    }
    Ok(window.label())
}
#[tauri::command]
pub fn native_voice_snapshot(
    window: WebviewWindow,
    manager: State<'_, NativeVoiceSessionManager>,
) -> Result<NativeVoiceSnapshot, &'static str> {
    manager.snapshot(owner(&window)?)
}
#[tauri::command]
pub async fn native_voice_execute(
    window: WebviewWindow,
    manager: State<'_, NativeVoiceSessionManager>,
    connect: State<'_, ConnectManager>,
    request: NativeVoiceRequest,
) -> Result<NativeVoiceSnapshot, &'static str> {
    let owner = owner(&window)?;
    let data = window
        .path()
        .app_local_data_dir()
        .map_err(|_| "native_voice_preparation_failed")?;
    manager
        .execute(owner, connect.inner(), &data, request)
        .await
}

#[tauri::command]
pub fn native_voice_social_reserve(
    window: WebviewWindow,
    manager: State<'_, NativeVoiceSessionManager>,
    connect: State<'_, ConnectManager>,
    request: aurora_launcher_core::engine::connect::SocialDiscoveredReserveRequest,
    host_player: bool,
) -> Result<aurora_launcher_core::engine::connect::SocialReservation, &'static str> {
    manager.reserve_social(owner(&window)?, connect.inner(), request, host_player)
}
