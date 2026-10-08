//! Narrow local-window caller. Private transport keys never cross this boundary.
use aurora_launcher_core::engine::connect::{
    ConnectCapabilities, ConnectManager, ConnectPreparation, ConnectReview, ConnectSnapshot,
    GuestRequest, HostRequest, InvitationExport,
};
use tauri::{State, WebviewWindow};

fn owner(window: &WebviewWindow) -> Result<&str, &'static str> {
    if window.label() != "main" {
        return Err("connect_owner_mismatch");
    }
    Ok(window.label())
}

#[tauri::command]
pub fn connect_capabilities(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
) -> Result<ConnectCapabilities, &'static str> {
    owner(&window)?;
    Ok(manager.capabilities())
}

#[tauri::command]
pub fn connect_snapshot(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
) -> Result<ConnectSnapshot, &'static str> {
    manager
        .snapshot(owner(&window)?)
        .map_err(|error| error.code())
}

#[tauri::command]
pub fn connect_prepare_host(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    request: HostRequest,
) -> Result<ConnectPreparation, &'static str> {
    manager
        .prepare_host(owner(&window)?, request)
        .map_err(|error| error.code())
}

#[tauri::command]
pub fn connect_review_host(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    guest_public_id: String,
) -> Result<ConnectReview, &'static str> {
    manager
        .review_host(owner(&window)?, &operation_id, &guest_public_id)
        .map_err(|error| error.code())
}

#[tauri::command]
pub fn connect_prepare_guest(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    request: GuestRequest,
) -> Result<ConnectPreparation, &'static str> {
    manager
        .prepare_guest(owner(&window)?, request)
        .map_err(|error| error.code())
}

#[tauri::command]
pub fn connect_review_guest(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    encoded_invitation: String,
) -> Result<ConnectReview, &'static str> {
    manager
        .review_guest(owner(&window)?, &operation_id, &encoded_invitation)
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn connect_confirm(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    digest: String,
) -> Result<ConnectSnapshot, &'static str> {
    manager
        .confirm(owner(&window)?, &operation_id, &digest)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub fn connect_export_invitation(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
) -> Result<InvitationExport, &'static str> {
    manager
        .export_invitation(owner(&window)?, &operation_id)
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn connect_probe_status(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
) -> Result<ConnectSnapshot, &'static str> {
    manager
        .probe_status(owner(&window)?, &operation_id)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn connect_stop(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
) -> Result<ConnectSnapshot, &'static str> {
    manager
        .stop(owner(&window)?, &operation_id)
        .await
        .map_err(|error| error.code())
}
