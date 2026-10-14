//! Main-window-only configuration caller. Instance paths and authority stay native.
use aurora_launcher_core::engine::connect_voice_desktop::{
    BeginCloseRequest, CloseIntent, CloseRequest, ConfirmCloseRequest, ConfirmRequest,
    InspectRequest, ListReceiptsRequest, OperationRequest, PrepareApplyRequest, ReceiptList,
    ResumeReceiptRequest, VoiceAnnouncementCapabilities, VoiceAnnouncementManager,
    VoiceAnnouncementSnapshot,
};
use tauri::{AppHandle, Manager, State, WebviewWindow};

fn owner_label(label: &str) -> Result<&str, &'static str> {
    if label == "main" {
        Ok(label)
    } else {
        Err("voice_owner_mismatch")
    }
}

#[tauri::command]
pub fn voice_announcement_capabilities(
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
) -> Result<VoiceAnnouncementCapabilities, &'static str> {
    owner_label(window.label())?;
    Ok(manager.capabilities())
}

#[tauri::command]
pub fn voice_announcement_snapshot(
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    manager
        .snapshot(owner_label(window.label())?)
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_inspect(
    app: AppHandle,
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: InspectRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    let owner = owner_label(window.label())?;
    if !manager.capabilities().available {
        return Err("voice_unavailable");
    }
    // Resolving the configured app directory does not create an instance or
    // run recovery. The manager parses the ID and verifies the diagnostic copy.
    let data_dir = app.path().app_data_dir().map_err(|_| "voice_storage")?;
    manager
        .inspect(owner, &data_dir, request)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_prepare_apply(
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: PrepareApplyRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    manager
        .prepare_apply(owner_label(window.label())?, request)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_prepare_apply_durable(
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: PrepareApplyRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    manager
        .prepare_apply_durable(owner_label(window.label())?, request)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_list_receipts(
    app: AppHandle,
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: ListReceiptsRequest,
) -> Result<ReceiptList, &'static str> {
    let owner = owner_label(window.label())?;
    if !manager.capabilities().available {
        return Err("voice_unavailable");
    }
    let data_dir = app.path().app_data_dir().map_err(|_| "voice_storage")?;
    manager
        .list_receipts(owner, &data_dir, request)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_resume_receipt(
    app: AppHandle,
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: ResumeReceiptRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    let owner = owner_label(window.label())?;
    if !manager.capabilities().available {
        return Err("voice_unavailable");
    }
    let data_dir = app.path().app_data_dir().map_err(|_| "voice_storage")?;
    manager
        .resume_receipt(owner, &data_dir, request)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_confirm(
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: ConfirmRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    manager
        .confirm(owner_label(window.label())?, request)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_prepare_retirement(
    app: AppHandle,
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: ResumeReceiptRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    let owner = owner_label(window.label())?;
    if !manager.capabilities().available {
        return Err("voice_unavailable");
    }
    let data_dir = app.path().app_data_dir().map_err(|_| "voice_storage")?;
    manager
        .prepare_retirement(owner, &data_dir, request)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_confirm_retirement(
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: ConfirmRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    manager
        .confirm_retirement(owner_label(window.label())?, request)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_prepare_restore(
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: OperationRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    manager
        .prepare_restore(owner_label(window.label())?, request)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_prepare_restore_reconciled(
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: OperationRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    manager
        .prepare_restore_reconciled(owner_label(window.label())?, request)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_cancel_review(
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: OperationRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    manager
        .cancel_review(owner_label(window.label())?, request)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_begin_close(
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: BeginCloseRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    let owner = owner_label(window.label())?;
    if request.intent == CloseIntent::Exit {
        return Err("voice_invalid_request");
    }
    manager
        .begin_close(owner, request)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_cancel_close(
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: CloseRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    manager
        .cancel_close(owner_label(window.label())?, request)
        .await
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn voice_announcement_confirm_close(
    app: AppHandle,
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: ConfirmCloseRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    let snapshot = manager
        .confirm_close(owner_label(window.label())?, request)
        .await
        .map_err(|error| error.code())?;
    if manager.exit_ready() {
        // Re-enters ExitRequested. That handler still awaits Connect cleanup;
        // this command never grants the global exit bypass itself.
        app.exit(0);
    }
    Ok(snapshot)
}

#[tauri::command]
pub async fn voice_announcement_finish_close(
    window: WebviewWindow,
    manager: State<'_, VoiceAnnouncementManager>,
    request: CloseRequest,
) -> Result<VoiceAnnouncementSnapshot, &'static str> {
    manager
        .finish_close(owner_label(window.label())?, request)
        .await
        .map_err(|error| error.code())
}

#[cfg(test)]
mod tests {
    use super::owner_label;

    #[test]
    fn announcement_authority_is_limited_to_the_exact_main_window() {
        assert_eq!(owner_label("main"), Ok("main"));
        for label in ["", "Main", " main", "main ", "main-child", "settings"] {
            assert_eq!(owner_label(label), Err("voice_owner_mismatch"));
        }
    }
}
