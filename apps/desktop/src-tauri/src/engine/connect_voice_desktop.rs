//! Local, opt-in announcement workflow. Presentation never carries mutation authority.
use serde::{Deserialize, Serialize};
use std::path::Path;
#[path = "connect_voice_desktop_durable_types.rs"]
mod durable_types;
pub use durable_types::*;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMod {
    SimpleVoiceChat,
    PlasmoVoice,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloseIntent {
    Panel,
    Logout,
    Exit,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VoiceAnnouncementPhase {
    Idle,
    Inspecting,
    ResumingReceipt,
    PreparingRetirement,
    ReviewRetirement,
    Retiring,
    Retired,
    Observed,
    PreparingApply,
    ReviewApply,
    Applying,
    Applied,
    PreparingRestore,
    ReviewRestore,
    Restoring,
    Restored,
    NeedsRecovery,
    Failed,
    Closing,
    CloseReview,
    Closed,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestorationState {
    Ready,
    NeedsRecovery,
    Restored,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewAction {
    SetAnnouncement,
    RestoreOriginal,
    RestoreReconciled,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CloseState {
    Waiting,
    ReviewRequired,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum VoiceAnnouncementError {
    #[serde(rename = "voice_unavailable")]
    Unavailable,
    #[serde(rename = "voice_owner_mismatch")]
    OwnerMismatch,
    #[serde(rename = "voice_invalid_request")]
    InvalidRequest,
    #[serde(rename = "voice_busy")]
    Busy,
    #[serde(rename = "voice_context_active")]
    ContextActive,
    #[serde(rename = "voice_context_changed")]
    ContextChanged,
    #[serde(rename = "voice_stale_review")]
    StaleReview,
    #[serde(rename = "voice_java_running")]
    JavaRunning,
    #[serde(rename = "voice_process_unavailable")]
    ProcessUnavailable,
    #[serde(rename = "voice_instance_busy")]
    InstanceBusy,
    #[serde(rename = "voice_incompatible")]
    Incompatible,
    #[serde(rename = "voice_unsafe_path")]
    UnsafePath,
    #[serde(rename = "voice_integrity")]
    Integrity,
    #[serde(rename = "voice_limit")]
    Limit,
    #[serde(rename = "voice_storage")]
    Storage,
    #[serde(rename = "voice_needs_recovery")]
    NeedsRecovery,
    #[serde(rename = "voice_close_pending")]
    ClosePending,
    #[serde(rename = "voice_stale_close")]
    StaleClose,
    #[serde(rename = "voice_internal")]
    Internal,
}
impl VoiceAnnouncementError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "voice_unavailable",
            Self::OwnerMismatch => "voice_owner_mismatch",
            Self::InvalidRequest => "voice_invalid_request",
            Self::Busy => "voice_busy",
            Self::ContextActive => "voice_context_active",
            Self::ContextChanged => "voice_context_changed",
            Self::StaleReview => "voice_stale_review",
            Self::JavaRunning => "voice_java_running",
            Self::ProcessUnavailable => "voice_process_unavailable",
            Self::InstanceBusy => "voice_instance_busy",
            Self::Incompatible => "voice_incompatible",
            Self::UnsafePath => "voice_unsafe_path",
            Self::Integrity => "voice_integrity",
            Self::Limit => "voice_limit",
            Self::Storage => "voice_storage",
            Self::NeedsRecovery => "voice_needs_recovery",
            Self::ClosePending => "voice_close_pending",
            Self::StaleClose => "voice_stale_close",
            Self::Internal => "voice_internal",
        }
    }
}
pub type VoiceAnnouncementResult<T> = Result<T, VoiceAnnouncementError>;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceAnnouncementCapabilities {
    pub available: bool,
    pub experimental: bool,
    pub diagnostic_copy_only: bool,
    pub mode: &'static str,
    pub guest_address: &'static str,
    pub mods: Vec<VoiceMod>,
    pub restoration: &'static str,
    pub audio_state: &'static str,
    pub transport_state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durable_receipts: Option<DurableReceiptCapabilities>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ObservedAnnouncement {
    SimpleVoiceChat {
        #[serde(rename = "voiceHost")]
        voice_host: String,
    },
    PlasmoAbsent,
    PlasmoPublic {
        ip: String,
        port: u16,
    },
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementInspection {
    pub voice_mod: VoiceMod,
    pub artifact_version: &'static str,
    pub artifact_sha256: &'static str,
    pub artifact_bytes: u64,
    pub config_path: &'static str,
    pub source_sha256: String,
    pub source_bytes: u64,
    pub announcement: ObservedAnnouncement,
    pub limitations: Vec<&'static str>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceAnnouncementReview {
    pub action: ReviewAction,
    pub voice_mod: VoiceMod,
    pub plan_id: String,
    pub digest: String,
    pub config_path: String,
    pub before: String,
    pub after: String,
    pub source_sha256: String,
    pub replacement_sha256: String,
    pub expires_at_ms: u64,
    pub warning: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySummary {
    pub restored_point_id: String,
    pub undo_available: bool,
    pub recovered_interrupted: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseReview {
    pub digest: String,
    pub expires_at_ms: u64,
    pub instance_id: String,
    pub voice_mod: VoiceMod,
    pub state: VoiceAnnouncementPhase,
    pub selective_restoration_lost: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durable_receipt: Option<DurableReceiptProjection>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceAnnouncementClose {
    pub close_id: String,
    pub intent: CloseIntent,
    pub state: CloseState,
    pub review: Option<CloseReview>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceAnnouncementSnapshot {
    pub generation: u64,
    pub operation_id: Option<String>,
    pub instance_id: Option<String>,
    pub voice_mod: Option<VoiceMod>,
    pub phase: VoiceAnnouncementPhase,
    pub busy: bool,
    pub observation: Option<AnnouncementInspection>,
    pub review: Option<VoiceAnnouncementReview>,
    pub restoration_state: Option<RestorationState>,
    pub last_recovery_result: Option<RecoverySummary>,
    pub last_error_code: Option<VoiceAnnouncementError>,
    pub close: Option<VoiceAnnouncementClose>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub durable_receipt: Option<DurableReceiptProjection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retirement_review: Option<ReceiptRetirementReview>,
}
impl Default for VoiceAnnouncementSnapshot {
    fn default() -> Self {
        Self {
            generation: 0,
            operation_id: None,
            instance_id: None,
            voice_mod: None,
            phase: VoiceAnnouncementPhase::Idle,
            busy: false,
            observation: None,
            review: None,
            restoration_state: None,
            last_recovery_result: None,
            last_error_code: None,
            close: None,
            durable_receipt: None,
            retirement_review: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectRequest {
    pub instance_id: String,
    pub voice_mod: VoiceMod,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareApplyRequest {
    pub operation_id: String,
    pub generation: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationRequest {
    pub operation_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmRequest {
    pub operation_id: String,
    pub plan_id: String,
    pub digest: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BeginCloseRequest {
    pub operation_id: Option<String>,
    pub intent: CloseIntent,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseRequest {
    pub close_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmCloseRequest {
    pub close_id: String,
    pub digest: String,
}

#[derive(Default)]
pub struct VoiceAnnouncementManager {
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    inner: super::connect_voice::DesktopManager,
}
fn owner(value: &str) -> VoiceAnnouncementResult<()> {
    if value == "main" {
        Ok(())
    } else {
        Err(VoiceAnnouncementError::OwnerMismatch)
    }
}
macro_rules! forward {
    ($name:ident, $request:ty) => {
        pub async fn $name(
            &self,
            owner_label: &str,
            request: $request,
        ) -> VoiceAnnouncementResult<VoiceAnnouncementSnapshot> {
            owner(owner_label)?;
            #[cfg(all(windows, feature = "connect-voice-lab"))]
            {
                self.inner.$name(request).await
            }
            #[cfg(not(all(windows, feature = "connect-voice-lab")))]
            {
                let _ = request;
                Err(VoiceAnnouncementError::Unavailable)
            }
        }
    };
}
impl VoiceAnnouncementManager {
    pub fn capabilities(&self) -> VoiceAnnouncementCapabilities {
        let available = cfg!(all(windows, feature = "connect-voice-lab"));
        VoiceAnnouncementCapabilities {
            available,
            experimental: true,
            diagnostic_copy_only: true,
            mode: "dedicatedAuroraOnly",
            guest_address: "127.0.0.1:35554",
            mods: if available {
                vec![VoiceMod::SimpleVoiceChat, VoiceMod::PlasmoVoice]
            } else {
                vec![]
            },
            restoration: "strictAndExplicitReconciled",
            audio_state: "notVerified",
            transport_state: "notObserved",
            durable_receipts: available.then_some(DurableReceiptCapabilities {
                schema_version: 1,
                max_receipts: 64,
                retirement: Some(ReceiptRetirementCapabilities { schema_version: 1 }),
            }),
        }
    }
    pub fn snapshot(
        &self,
        owner_label: &str,
    ) -> VoiceAnnouncementResult<VoiceAnnouncementSnapshot> {
        owner(owner_label)?;
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        {
            self.inner.snapshot()
        }
        #[cfg(not(all(windows, feature = "connect-voice-lab")))]
        {
            Ok(VoiceAnnouncementSnapshot::default())
        }
    }
    pub async fn inspect(
        &self,
        owner_label: &str,
        data_dir: &Path,
        request: InspectRequest,
    ) -> VoiceAnnouncementResult<VoiceAnnouncementSnapshot> {
        owner(owner_label)?;
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        {
            self.inner.inspect(data_dir, request).await
        }
        #[cfg(not(all(windows, feature = "connect-voice-lab")))]
        {
            let _ = (data_dir, request);
            Err(VoiceAnnouncementError::Unavailable)
        }
    }
    forward!(prepare_apply, PrepareApplyRequest);
    forward!(prepare_apply_durable, PrepareApplyRequest);
    forward!(confirm_retirement, ConfirmRequest);
    forward!(confirm, ConfirmRequest);
    forward!(prepare_restore, OperationRequest);
    forward!(prepare_restore_reconciled, OperationRequest);
    forward!(cancel_review, OperationRequest);
    forward!(begin_close, BeginCloseRequest);
    forward!(cancel_close, CloseRequest);
    forward!(confirm_close, ConfirmCloseRequest);
    forward!(finish_close, CloseRequest);
    pub async fn list_receipts(
        &self,
        owner_label: &str,
        data_dir: &Path,
        request: ListReceiptsRequest,
    ) -> VoiceAnnouncementResult<ReceiptList> {
        owner(owner_label)?;
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        {
            self.inner.list_receipts(data_dir, request).await
        }
        #[cfg(not(all(windows, feature = "connect-voice-lab")))]
        {
            let _ = (data_dir, request);
            Err(VoiceAnnouncementError::Unavailable)
        }
    }
    pub async fn resume_receipt(
        &self,
        owner_label: &str,
        data_dir: &Path,
        request: ResumeReceiptRequest,
    ) -> VoiceAnnouncementResult<VoiceAnnouncementSnapshot> {
        owner(owner_label)?;
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        {
            self.inner.resume_receipt(data_dir, request).await
        }
        #[cfg(not(all(windows, feature = "connect-voice-lab")))]
        {
            let _ = (data_dir, request);
            Err(VoiceAnnouncementError::Unavailable)
        }
    }
    pub async fn prepare_retirement(
        &self,
        owner_label: &str,
        data_dir: &Path,
        request: ResumeReceiptRequest,
    ) -> VoiceAnnouncementResult<VoiceAnnouncementSnapshot> {
        owner(owner_label)?;
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        {
            self.inner.prepare_retirement(data_dir, request).await
        }
        #[cfg(not(all(windows, feature = "connect-voice-lab")))]
        {
            let _ = (data_dir, request);
            Err(VoiceAnnouncementError::Unavailable)
        }
    }
    pub async fn begin_exit(&self) -> VoiceAnnouncementResult<VoiceAnnouncementSnapshot> {
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        {
            self.inner.begin_exit().await
        }
        #[cfg(not(all(windows, feature = "connect-voice-lab")))]
        {
            Ok(VoiceAnnouncementSnapshot {
                phase: VoiceAnnouncementPhase::Closed,
                ..Default::default()
            })
        }
    }
    pub fn exit_ready(&self) -> bool {
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        {
            self.inner.exit_ready()
        }
        #[cfg(not(all(windows, feature = "connect-voice-lab")))]
        {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_requests_reject_unknown_missing_fractional_and_untrusted_authority_fields() {
        let id = uuid::Uuid::new_v4().to_string();
        for value in [
            serde_json::json!({"instanceId":"copy","voiceMod":"simple_voice_chat","path":"C:/other"}),
            serde_json::json!({"instanceId":"copy","voiceMod":"unknown"}),
            serde_json::json!({"instanceId":"copy"}),
        ] {
            assert!(serde_json::from_value::<InspectRequest>(value).is_err());
        }
        for value in [
            serde_json::json!({"operationId":id,"generation":1.5}),
            serde_json::json!({"operationId":id,"generation":-1}),
            serde_json::json!({"operationId":id,"generation":1,"sourceSha256":"a".repeat(64)}),
            serde_json::json!({"operationId":id}),
        ] {
            assert!(serde_json::from_value::<PrepareApplyRequest>(value).is_err());
        }
        assert!(serde_json::from_value::<ConfirmRequest>(serde_json::json!({"operationId":id,"planId":id,"digest":"a".repeat(64),"action":"restoreOriginal"})).is_err());
        assert!(serde_json::from_value::<BeginCloseRequest>(
            serde_json::json!({"operationId":null,"intent":"shutdown"})
        )
        .is_err());
        assert!(serde_json::from_value::<CloseRequest>(
            serde_json::json!({"closeId":id,"skipReview":true})
        )
        .is_err());
    }

    #[test]
    fn main_window_ownership_precedes_every_operation() {
        let manager = VoiceAnnouncementManager::default();
        assert_eq!(
            manager.snapshot("other").err(),
            Some(VoiceAnnouncementError::OwnerMismatch)
        );
        let result = tauri::async_runtime::block_on(manager.inspect(
            "other",
            Path::new("not-used"),
            InspectRequest {
                instance_id: "copy".into(),
                voice_mod: VoiceMod::SimpleVoiceChat,
            },
        ));
        assert_eq!(result.err(), Some(VoiceAnnouncementError::OwnerMismatch));
        assert_eq!(
            tauri::async_runtime::block_on(manager.begin_close(
                "other",
                BeginCloseRequest {
                    operation_id: None,
                    intent: CloseIntent::Panel
                }
            ))
            .err(),
            Some(VoiceAnnouncementError::OwnerMismatch)
        );
        assert_eq!(
            tauri::async_runtime::block_on(manager.confirm_close(
                "other",
                ConfirmCloseRequest {
                    close_id: "not-used".into(),
                    digest: String::new()
                }
            ))
            .err(),
            Some(VoiceAnnouncementError::OwnerMismatch)
        );
    }

    #[cfg(not(all(windows, feature = "connect-voice-lab")))]
    #[test]
    fn default_build_has_no_authority_and_all_work_commands_are_unavailable() {
        let manager = VoiceAnnouncementManager::default();
        let capabilities = manager.capabilities();
        assert!(!capabilities.available && capabilities.mods.is_empty());
        assert!(manager.snapshot("main").unwrap().phase == VoiceAnnouncementPhase::Idle);
        let id = uuid::Uuid::new_v4().to_string();
        tauri::async_runtime::block_on(async {
            assert_eq!(
                manager
                    .inspect(
                        "main",
                        Path::new("not-used"),
                        InspectRequest {
                            instance_id: "copy".into(),
                            voice_mod: VoiceMod::SimpleVoiceChat
                        }
                    )
                    .await
                    .err(),
                Some(VoiceAnnouncementError::Unavailable)
            );
            assert_eq!(
                manager
                    .prepare_apply(
                        "main",
                        PrepareApplyRequest {
                            operation_id: id.clone(),
                            generation: 0
                        }
                    )
                    .await
                    .err(),
                Some(VoiceAnnouncementError::Unavailable)
            );
            assert_eq!(
                manager
                    .confirm(
                        "main",
                        ConfirmRequest {
                            operation_id: id.clone(),
                            plan_id: id.clone(),
                            digest: "0".repeat(64)
                        }
                    )
                    .await
                    .err(),
                Some(VoiceAnnouncementError::Unavailable)
            );
            assert_eq!(
                manager
                    .prepare_restore(
                        "main",
                        OperationRequest {
                            operation_id: id.clone()
                        }
                    )
                    .await
                    .err(),
                Some(VoiceAnnouncementError::Unavailable)
            );
            assert_eq!(
                manager
                    .prepare_restore_reconciled(
                        "main",
                        OperationRequest {
                            operation_id: id.clone()
                        }
                    )
                    .await
                    .err(),
                Some(VoiceAnnouncementError::Unavailable)
            );
            assert_eq!(
                manager
                    .cancel_review(
                        "main",
                        OperationRequest {
                            operation_id: id.clone()
                        }
                    )
                    .await
                    .err(),
                Some(VoiceAnnouncementError::Unavailable)
            );
            assert_eq!(
                manager
                    .begin_close(
                        "main",
                        BeginCloseRequest {
                            operation_id: None,
                            intent: CloseIntent::Panel
                        }
                    )
                    .await
                    .err(),
                Some(VoiceAnnouncementError::Unavailable)
            );
            assert_eq!(
                manager
                    .cancel_close(
                        "main",
                        CloseRequest {
                            close_id: id.clone()
                        }
                    )
                    .await
                    .err(),
                Some(VoiceAnnouncementError::Unavailable)
            );
            assert_eq!(
                manager
                    .confirm_close(
                        "main",
                        ConfirmCloseRequest {
                            close_id: id.clone(),
                            digest: "0".repeat(64)
                        }
                    )
                    .await
                    .err(),
                Some(VoiceAnnouncementError::Unavailable)
            );
            assert_eq!(
                manager
                    .finish_close("main", CloseRequest { close_id: id })
                    .await
                    .err(),
                Some(VoiceAnnouncementError::Unavailable)
            );
            assert!(
                manager.begin_exit().await.unwrap().phase == VoiceAnnouncementPhase::Closed
                    && manager.exit_ready()
            );
        });
    }
}
