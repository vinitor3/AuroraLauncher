//! Narrow local-window caller. Private transport keys never cross this boundary.
use aurora_launcher_core::engine::connect::{
    ConnectCapabilities, ConnectManager, ConnectPreparation, ConnectReview, ConnectSnapshot,
    FragmentedHostRequest, GroupDiscoveredHostRequest, GroupHostRequest, GroupInvitationExport,
    GroupParticipantRequest, GroupPreparation, GroupReview, GuestRequest, HostRequest,
    InvitationExport, SessionFragmentedDiscoveredHostRequest, SessionFragmentedHostRequest,
    SessionHostRequest,
};
use aurora_launcher_core::engine::connect_discovery::{
    DiscoveryManager, Inventory, TargetReview, TargetStatus,
};
use tauri::{Manager as _, State, WebviewWindow};

fn owner(window: &WebviewWindow) -> Result<&str, &'static str> {
    if window.label() != "main" {
        return Err("connect_owner_mismatch");
    }
    Ok(window.label())
}

#[tauri::command]
pub fn connect_social_capabilities(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
) -> Result<aurora_launcher_core::engine::connect::SocialCapabilities, &'static str> {
    owner(&window)?;
    Ok(manager.social_capabilities())
}
#[tauri::command]
pub async fn connect_social_observe_sessions(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    request: aurora_launcher_core::engine::connect::SocialPresenceRequest,
    id_token: String,
) -> Result<Vec<aurora_launcher_core::engine::connect::SocialSessionAvailability>, &'static str> {
    manager
        .social_observe_sessions(owner(&window)?, request, id_token)
        .await
        .map_err(|error| error.code())
}
#[tauri::command]
pub fn connect_social_reserve(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    request: aurora_launcher_core::engine::connect::SocialReserveRequest,
) -> Result<aurora_launcher_core::engine::connect::SocialReservation, &'static str> {
    manager
        .social_reserve(owner(&window)?, request)
        .map_err(|error| error.code())
}
#[tauri::command]
pub async fn connect_social_authenticate(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    generation: u64,
    id_token: String,
) -> Result<ConnectSnapshot, &'static str> {
    manager
        .social_authenticate(owner(&window)?, &operation_id, generation, id_token)
        .map_err(|error| error.code())
}
#[tauri::command]
pub fn connect_social_reserve_discovered(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    discovery: State<'_, DiscoveryManager>,
    request: aurora_launcher_core::engine::connect::SocialDiscoveredReserveRequest,
    selection_id: String,
) -> Result<aurora_launcher_core::engine::connect::SocialReservation, &'static str> {
    manager
        .social_reserve_discovered(
            owner(&window)?,
            request,
            selection_id,
            discovery.inner().clone(),
        )
        .map_err(|error| error.code())
}
#[tauri::command]
pub fn connect_social_confirm_host(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    generation: u64,
    digest: String,
) -> Result<ConnectSnapshot, &'static str> {
    manager
        .social_confirm_host(owner(&window)?, &operation_id, generation, &digest)
        .map_err(|error| error.code())
}
#[tauri::command]
pub fn connect_social_confirm_guest(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    generation: u64,
    digest: String,
) -> Result<ConnectSnapshot, &'static str> {
    manager
        .social_confirm_guest(owner(&window)?, &operation_id, generation, &digest)
        .map_err(|error| error.code())
}
#[tauri::command]
pub async fn connect_account_transition_stop(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    native_voice: State<
        '_,
        aurora_launcher_core::engine::connect_native_voice::NativeVoiceSessionManager,
    >,
) -> Result<ConnectSnapshot, &'static str> {
    let owner = owner(&window)?;
    #[cfg(all(windows, feature = "connect-lab"))]
    let _ready_copy_transition = crate::commands::ready_game::CopyInstallAccountTransition::begin();
    native_voice.cancel();
    let result = manager
        .account_transition_stop(owner)
        .await
        .map_err(|error| error.code());
    native_voice.shutdown_all().await;
    #[cfg(all(windows, feature = "connect-lab"))]
    crate::commands::ready_game::wait_ready_copy_install().await;
    result
}

#[tauri::command]
pub fn connect_social_refresh_authentication(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    account_epoch: u64,
    id_token: String,
) -> Result<(), &'static str> {
    manager
        .social_refresh_authentication(owner(&window)?, &operation_id, account_epoch, id_token)
        .map_err(|error| error.code())
}

#[tauri::command]
pub fn connect_social_revoke_participant(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    account_epoch: u64,
    grant_id: String,
) -> Result<ConnectSnapshot, &'static str> {
    manager
        .social_revoke_participant(owner(&window)?, &operation_id, account_epoch, &grant_id)
        .map_err(|error| error.code())
}

/// Typed possession proof only; no approval, receipt, or transport side effect.
#[tauri::command]
pub fn connect_prove_social_key_binding(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    challenge_json: String,
) -> Result<serde_json::Value, &'static str> {
    let owner = owner(&window)?;
    #[cfg(feature = "connect-lab")]
    {
        let proof = manager
            .prove_social_key_binding(owner, &operation_id, &challenge_json)
            .map_err(|error| error.code())?;
        serde_json::to_value(proof).map_err(|_| "connect_internal")
    }
    #[cfg(not(feature = "connect-lab"))]
    {
        let _ = (owner, manager, operation_id, challenge_json);
        Err("connect_disabled")
    }
}

#[tauri::command]
pub async fn connect_prepare_ready_game_launch(
    window: WebviewWindow,
    app: tauri::AppHandle,
    manager: State<'_, ConnectManager>,
    request: aurora_launcher_core::engine::connect::ReadyGameLaunchRequest,
) -> Result<aurora_launcher_core::engine::connect::ReadyGameLaunchSnapshot, &'static str> {
    let owner = owner(&window)?;
    #[cfg(all(windows, feature = "connect-lab"))]
    {
        manager
            .prepare_ready_game_launch(
                owner,
                request,
                std::sync::Arc::new(crate::commands::ready_game::DesktopReadyGameBackend::new(
                    app,
                )),
            )
            .map_err(|e| e.code())
    }
    #[cfg(not(all(windows, feature = "connect-lab")))]
    {
        let _ = (owner, app, manager, request);
        Err("connect_ready_game_disabled")
    }
}
#[tauri::command]
pub fn connect_ready_game_launch_snapshot(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
) -> Result<aurora_launcher_core::engine::connect::ReadyGameLaunchSnapshot, &'static str> {
    manager
        .ready_game_launch_snapshot(owner(&window)?, &operation_id)
        .map_err(|e| e.code())
}
#[tauri::command]
pub fn connect_confirm_ready_game_launch(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    plan_id: String,
    digest: String,
) -> Result<aurora_launcher_core::engine::connect::ReadyGameLaunchSnapshot, &'static str> {
    manager
        .confirm_ready_game_launch(owner(&window)?, &operation_id, &plan_id, &digest)
        .map_err(|e| e.code())
}
#[tauri::command]
pub async fn connect_cancel_ready_game_launch(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    plan_id: String,
) -> Result<aurora_launcher_core::engine::connect::ReadyGameLaunchSnapshot, &'static str> {
    manager
        .cancel_ready_game_launch(owner(&window)?, &operation_id, &plan_id)
        .await
        .map_err(|e| e.code())
}

#[tauri::command]
pub fn connect_prepare_session_group_host(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    request: GroupHostRequest,
) -> Result<GroupPreparation, &'static str> {
    manager
        .prepare_session_group_host(owner(&window)?, request)
        .map_err(|error| error.code())
}
#[tauri::command]
pub async fn connect_prepare_session_group_host_discovered(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    discovery: State<'_, DiscoveryManager>,
    selection_id: String,
    request: GroupDiscoveredHostRequest,
) -> Result<ConnectSnapshot, &'static str> {
    manager
        .prepare_session_group_host_discovered(
            owner(&window)?,
            request,
            selection_id,
            discovery.inner().clone(),
        )
        .map_err(|error| error.code())
}
#[tauri::command]
pub async fn connect_review_session_group_host(
    window: WebviewWindow,
    app: tauri::AppHandle,
    operation_id: String,
    participants: Vec<GroupParticipantRequest>,
) -> Result<GroupReview, &'static str> {
    let owner = owner(&window)?.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<ConnectManager>()
            .review_session_group_host(&owner, &operation_id, participants)
            .map_err(|error| error.code())
    })
    .await
    .map_err(|_| "connect_internal")?
}
#[tauri::command]
pub async fn connect_confirm_session_group_host(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    digest: String,
) -> Result<ConnectSnapshot, &'static str> {
    manager
        .confirm_session_group_host(owner(&window)?, &operation_id, &digest)
        .await
        .map_err(|error| error.code())
}
#[tauri::command]
pub fn connect_export_session_group_invitation(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    participant_id: String,
) -> Result<GroupInvitationExport, &'static str> {
    manager
        .export_session_group_invitation(owner(&window)?, &operation_id, &participant_id)
        .map_err(|error| error.code())
}
#[tauri::command]
pub fn connect_revoke_session_group_participant(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    participant_id: String,
    expected_generation: u64,
) -> Result<ConnectSnapshot, &'static str> {
    manager
        .revoke_session_group_participant(
            owner(&window)?,
            &operation_id,
            &participant_id,
            expected_generation,
        )
        .map_err(|error| error.code())
}

#[tauri::command]
pub fn connect_retry_session_udp(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    expected_generation: u64,
) -> Result<ConnectSnapshot, &'static str> {
    manager
        .retry_session_udp(owner(&window)?, &operation_id, expected_generation)
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn connect_prepare_session_fragmented_host_discovered(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    discovery: State<'_, DiscoveryManager>,
    selection_id: String,
    request: SessionFragmentedDiscoveredHostRequest,
) -> Result<ConnectPreparation, &'static str> {
    let owner = owner(&window)?;
    #[cfg(all(windows, feature = "connect-lab"))]
    {
        let pending = manager
            .reserve_session_fragmented_host_discovered(
                owner,
                request,
                selection_id,
                discovery.inner().clone(),
            )
            .map_err(|error| error.code())?;
        pending.wait().await.map_err(|error| error.code())
    }
    #[cfg(not(all(windows, feature = "connect-lab")))]
    {
        let _ = (owner, manager, discovery, selection_id, request);
        Err("discovery_unavailable")
    }
}

#[tauri::command]
pub fn connect_prepare_session_fragmented_host(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    request: SessionFragmentedHostRequest,
) -> Result<ConnectPreparation, &'static str> {
    manager
        .prepare_session_fragmented_host(owner(&window)?, request)
        .map_err(|error| error.code())
}

#[tauri::command]
pub fn connect_prepare_session_fragmented_guest(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    request: GuestRequest,
) -> Result<ConnectPreparation, &'static str> {
    manager
        .prepare_session_fragmented_guest(owner(&window)?, request)
        .map_err(|error| error.code())
}

#[tauri::command]
pub fn connect_prepare_fragmented_host(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    request: FragmentedHostRequest,
) -> Result<ConnectPreparation, &'static str> {
    manager
        .prepare_fragmented_host(owner(&window)?, request)
        .map_err(|error| error.code())
}
#[tauri::command]
pub fn connect_prepare_fragmented_guest(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    request: GuestRequest,
) -> Result<ConnectPreparation, &'static str> {
    manager
        .prepare_fragmented_guest(owner(&window)?, request)
        .map_err(|error| error.code())
}
#[tauri::command]
pub fn connect_prepare_session_host(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    request: SessionHostRequest,
) -> Result<ConnectPreparation, &'static str> {
    manager
        .prepare_session_host(owner(&window)?, request)
        .map_err(|error| error.code())
}

#[tauri::command]
pub fn connect_prepare_session_guest(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    request: GuestRequest,
) -> Result<ConnectPreparation, &'static str> {
    manager
        .prepare_session_guest(owner(&window)?, request)
        .map_err(|error| error.code())
}

#[tauri::command]
pub async fn connect_prepare_session_host_discovered(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    discovery: State<'_, DiscoveryManager>,
    selection_id: String,
    request: SessionHostRequest,
) -> Result<ConnectPreparation, &'static str> {
    let owner = owner(&window)?;
    #[cfg(all(windows, feature = "connect-lab"))]
    {
        let pending = manager
            .reserve_session_host_discovered(
                owner,
                request,
                selection_id,
                discovery.inner().clone(),
            )
            .map_err(|error| error.code())?;
        pending.wait().await.map_err(|error| error.code())
    }
    #[cfg(not(all(windows, feature = "connect-lab")))]
    {
        let _ = (owner, manager, discovery, selection_id, request);
        Err("discovery_unavailable")
    }
}

#[tauri::command]
pub fn connect_discover_instance(
    window: WebviewWindow,
    running: State<'_, crate::commands::RunningInstances>,
    discovery: State<'_, DiscoveryManager>,
    instance_id: String,
) -> Result<Inventory, &'static str> {
    let owner = owner(&window)?;
    #[cfg(all(windows, feature = "connect-lab"))]
    {
        let (proof, runtime) = running
            .connect_owned_child(&instance_id)
            .map_err(|error| error.code())?;
        discovery
            .inventory_for_runtime(owner, proof, runtime)
            .map_err(|error| error.code())
    }
    #[cfg(not(all(windows, feature = "connect-lab")))]
    {
        let _ = (owner, running, discovery, instance_id);
        Err("discovery_unavailable")
    }
}

#[tauri::command]
pub fn connect_review_discovered(
    window: WebviewWindow,
    discovery: State<'_, DiscoveryManager>,
    inventory_id: String,
    candidate_id: String,
) -> Result<TargetReview, &'static str> {
    let owner = owner(&window)?;
    #[cfg(all(windows, feature = "connect-lab"))]
    {
        discovery
            .review(owner, &inventory_id, &candidate_id)
            .map_err(|error| error.code())
    }
    #[cfg(not(all(windows, feature = "connect-lab")))]
    {
        let _ = (owner, discovery, inventory_id, candidate_id);
        Err("discovery_unavailable")
    }
}

#[tauri::command]
pub async fn connect_confirm_discovered_status(
    window: WebviewWindow,
    discovery: State<'_, DiscoveryManager>,
    review_id: String,
    digest: String,
) -> Result<TargetStatus, &'static str> {
    let owner = owner(&window)?;
    #[cfg(all(windows, feature = "connect-lab"))]
    {
        discovery
            .confirm_status(owner, &review_id, &digest)
            .await
            .map_err(|error| error.code())
    }
    #[cfg(not(all(windows, feature = "connect-lab")))]
    {
        let _ = (owner, discovery, review_id, digest);
        Err("discovery_unavailable")
    }
}

#[tauri::command]
pub async fn connect_prepare_discovered_host(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    discovery: State<'_, DiscoveryManager>,
    selection_id: String,
    request: HostRequest,
) -> Result<ConnectPreparation, &'static str> {
    let owner = owner(&window)?;
    #[cfg(all(windows, feature = "connect-lab"))]
    {
        let pending = manager
            .reserve_host_discovered(owner, request, selection_id, discovery.inner().clone())
            .map_err(|error| error.code())?;
        pending.wait().await.map_err(|error| error.code())
    }
    #[cfg(not(all(windows, feature = "connect-lab")))]
    {
        let _ = (owner, manager, discovery, selection_id, request);
        Err("discovery_unavailable")
    }
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
pub async fn connect_review_host(
    window: WebviewWindow,
    manager: State<'_, ConnectManager>,
    operation_id: String,
    guest_public_id: String,
) -> Result<ConnectReview, &'static str> {
    let pending = manager
        .reserve_host_review(owner(&window)?, &operation_id, &guest_public_id)
        .map_err(|error| error.code())?;
    pending.wait().await.map_err(|error| error.code())
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

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReadyCopyInstallRequest {
    pub(crate) operation_id: String,
    pub(crate) connect_operation_id: String,
    pub(crate) source_instance_id: String,
    pub(crate) profile_id: String,
    pub(crate) approved: bool,
}

#[tauri::command]
pub async fn connect_install_ready_copy(
    window: WebviewWindow,
    app: tauri::AppHandle,
    manager: State<'_, ConnectManager>,
    request: ReadyCopyInstallRequest,
) -> Result<aurora_launcher_core::engine::diagnostic_lab::LabEnvironment, &'static str> {
    let owner = owner(&window)?;
    #[cfg(all(windows, feature = "connect-lab"))]
    {
        if manager.capabilities().ready_game_launch.is_none() {
            return Err("connect_ready_game_disabled");
        }
        crate::commands::ready_game::install_ready_copy(app, &manager, owner, request).await
    }
    #[cfg(not(all(windows, feature = "connect-lab")))]
    {
        let _ = (
            app,
            manager,
            owner,
            request.operation_id,
            request.connect_operation_id,
            request.source_instance_id,
            request.profile_id,
            request.approved,
        );
        Err("connect_ready_game_disabled")
    }
}

#[tauri::command]
pub fn connect_cancel_ready_copy_install(
    window: WebviewWindow,
    operation_id: String,
) -> Result<(), &'static str> {
    owner(&window)?;
    #[cfg(all(windows, feature = "connect-lab"))]
    {
        crate::commands::ready_game::cancel_ready_copy_install(Some(&operation_id))
    }
    #[cfg(not(all(windows, feature = "connect-lab")))]
    {
        let _ = operation_id;
        Err("connect_ready_game_disabled")
    }
}
