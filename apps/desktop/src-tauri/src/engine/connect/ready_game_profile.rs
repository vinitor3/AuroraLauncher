//! Explicit, default-off launch of an already prepared diagnostic copy.
//! These DTOs carry observations and review input, never a transport permit.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyGameLaunchCapabilities {
    pub schema_version: u8,
    pub instance_scope: &'static str,
    pub max_plan_seconds: u16,
    pub runtimes: Vec<crate::engine::connect_ready_profile::ReadyProfileDescriptor>,
}
impl Default for ReadyGameLaunchCapabilities {
    fn default() -> Self {
        Self {
            schema_version: 2,
            instance_scope: "diagnosticCopy",
            max_plan_seconds: 60,
            runtimes: crate::engine::connect_ready_profile::ReadyProfile::ALL
                .into_iter()
                .map(|profile| profile.descriptor())
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadyGameAppearance {
    pub skin_url: Option<String>,
    pub skin_file: Option<String>,
    pub cape_url: Option<String>,
    pub skin_model: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadyGameLaunchRequest {
    pub operation_id: String,
    pub instance_id: String,
    pub source_instance_id: String,
    pub context_key: String,
    pub java_executable: String,
    pub nickname: String,
    pub appearance: ReadyGameAppearance,
}
impl ReadyGameLaunchRequest {
    #[cfg(feature = "connect-lab")]
    pub(super) fn validate(&self) -> Result<(), ReadyGameLaunchError> {
        let bounded = |value: &str, maximum: usize| {
            !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
        };
        if !bounded(&self.operation_id, 64)
            || !bounded(&self.instance_id, 96)
            || !bounded(&self.source_instance_id, 96)
            || !bounded(&self.context_key, 512)
            || !bounded(&self.java_executable, 4096)
            || !bounded(&self.nickname, 16)
            || self.nickname.len() < 3
            || !self
                .nickname
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || self
                .appearance
                .skin_url
                .as_ref()
                .is_some_and(|v| !bounded(v, 2048))
            || self
                .appearance
                .cape_url
                .as_ref()
                .is_some_and(|v| !bounded(v, 2048))
            || self
                .appearance
                .skin_file
                .as_ref()
                .is_some_and(|v| !bounded(v, 4096))
            || self
                .appearance
                .skin_model
                .as_deref()
                .is_some_and(|v| !matches!(v, "classic" | "slim"))
        {
            return Err(ReadyGameLaunchError::InvalidRequest);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyGameLaunchReview {
    pub operation_id: String,
    pub plan_id: String,
    pub digest: String,
    pub expires_at_ms: u64,
    pub session_expires_at_ms: u64,
    pub instance_id: String,
    pub source_instance_id: String,
    pub context_key: String,
    pub minecraft_version: &'static str,
    pub loader: &'static str,
    pub loader_version: &'static str,
    pub java_executable: String,
    pub java_major: u32,
    pub nickname: String,
    pub appearance: ReadyGameAppearance,
    pub game_address: String,
    pub core_version: &'static str,
    pub core_sha256: String,
    pub skins_version: &'static str,
    pub skins_sha256: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyGameLaunchResult {
    pub run_id: String,
    pub process_id: u32,
    pub instance_id: String,
    pub world_state: &'static str,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyGameLaunchSnapshot {
    pub operation_id: String,
    pub generation: u64,
    pub plan_id: Option<String>,
    pub phase: &'static str,
    pub review: Option<ReadyGameLaunchReview>,
    pub result: Option<ReadyGameLaunchResult>,
    pub error_code: Option<&'static str>,
}
impl ReadyGameLaunchSnapshot {
    #[cfg(feature = "connect-lab")]
    pub(super) fn idle(operation_id: &str) -> Self {
        Self {
            operation_id: operation_id.into(),
            generation: 0,
            plan_id: None,
            phase: "idle",
            review: None,
            result: None,
            error_code: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadyGameLaunchError {
    Disabled,
    InvalidRequest,
    WrongOwner,
    StaleOperation,
    Busy,
    Unavailable,
    Expired,
    ApprovalMismatch,
    Cancelled,
    InstanceNotReady,
    InstanceChanged,
    InstanceBusy,
    RecoveryPending,
    UnsupportedRuntime,
    MissingArtifact,
    ArtifactMismatch,
    JavaUnavailable,
    JavaIncompatible,
    ConflictingArguments,
    LaunchFailed,
    Internal,
}
impl ReadyGameLaunchError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Disabled => "connect_ready_game_disabled",
            Self::InvalidRequest => "connect_ready_game_invalid_request",
            Self::WrongOwner => "connect_ready_game_wrong_owner",
            Self::StaleOperation => "connect_ready_game_stale_operation",
            Self::Busy => "connect_ready_game_busy",
            Self::Unavailable => "connect_ready_game_unavailable",
            Self::Expired => "connect_ready_game_expired",
            Self::ApprovalMismatch => "connect_ready_game_approval_mismatch",
            Self::Cancelled => "connect_ready_game_cancelled",
            Self::InstanceNotReady => "connect_ready_game_instance_not_ready",
            Self::InstanceChanged => "connect_ready_game_instance_changed",
            Self::InstanceBusy => "connect_ready_game_instance_busy",
            Self::RecoveryPending => "connect_ready_game_recovery_pending",
            Self::UnsupportedRuntime => "connect_ready_game_unsupported_runtime",
            Self::MissingArtifact => "connect_ready_game_missing_artifact",
            Self::ArtifactMismatch => "connect_ready_game_artifact_mismatch",
            Self::JavaUnavailable => "connect_ready_game_java_unavailable",
            Self::JavaIncompatible => "connect_ready_game_java_incompatible",
            Self::ConflictingArguments => "connect_ready_game_conflicting_arguments",
            Self::LaunchFailed => "connect_ready_game_launch_failed",
            Self::Internal => "connect_ready_game_internal",
        }
    }
}
