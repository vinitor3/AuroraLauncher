//! Explicit experimental desktop caller. Public DTOs exist in default builds;
//! transport, private keys and sockets exist only with the connect-lab feature.
use serde::{Deserialize, Serialize};
mod fragmented_profile;
mod group_profile;
mod path_observation;
pub use path_observation::{ConnectPathObservation, ConnectPathState};
mod ready_game_caller;
mod ready_game_profile;
mod social_presence_profile;
mod social_profile;
mod terminal_diagnostic;
pub use ready_game_profile::*;
pub use social_presence_profile::*;
pub use social_profile::*;
pub use terminal_diagnostic::ConnectTerminalDiagnostic;
#[cfg(feature = "connect-lab")]
mod ready_game_backend;
#[cfg(feature = "connect-lab")]
pub use aurora_connect_lab::SocialKeyBindingProof;
pub use fragmented_profile::{
    FragmentedGuestState, FragmentedHostRequest, FragmentedUdpCapabilities, FragmentedUdpContract,
    FragmentedUdpReview, SessionFragmentedDiscoveredHostRequest, SessionFragmentedHostRequest,
    SessionFragmentedUdpCapabilities, UdpRecoveryCapabilities, UdpRecoveryState,
};
pub use group_profile::{
    GroupDiscoveredHostRequest, GroupHostRequest, GroupInvitationExport, GroupParticipant,
    GroupParticipantRequest, GroupPreparation, GroupReview, HostGroupSnapshot, PendingGroupSource,
    SessionFragmentedHostGroupsCapabilities,
};
#[cfg(feature = "connect-lab")]
pub use live::ready_game::ReadyGameSpawnGate;
#[cfg(feature = "connect-lab")]
pub use ready_game_backend::{PreparedReadyGame, ReadyGameBackend};

#[cfg(feature = "connect-lab")]
mod live;
#[cfg(feature = "connect-lab")]
mod session_authority;
#[cfg(all(test, feature = "connect-lab"))]
mod tests;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConnectMode {
    Loopback,
    ExternalIpv6,
    DirectIpv4,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostRequest {
    pub mode: ConnectMode,
    pub bind_address: String,
    pub game_target: String,
    pub lifetime_seconds: u16,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionHostRequest {
    pub mode: ConnectMode,
    pub bind_address: String,
    pub game_target: String,
    pub admission_seconds: u16,
    pub session_seconds: u16,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInvitationCapabilities {
    pub profile: &'static str,
    pub schema_version: u8,
    pub max_admission_seconds: u16,
    pub max_session_seconds: u16,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionTiming {
    pub profile: &'static str,
    pub schema_version: u8,
    pub admission_seconds: u16,
    pub session_seconds: u16,
    pub admission_expires_at_ms: u64,
    pub session_expires_at_ms: u64,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuestRequest {
    pub mode: ConnectMode,
    pub expected_host_public_id: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectCapabilities {
    pub available: bool,
    pub experimental: bool,
    pub max_lifetime_seconds: u16,
    pub modes: Vec<ConnectMode>,
    pub services: Vec<&'static str>,
    pub voice_enabled: bool,
    pub relay_enabled: bool,
    pub status_protocol: i32,
    pub status_handshake_host: &'static str,
    pub status_handshake_port: u16,
    pub owned_instance_discovery: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_invitations: Option<SessionInvitationCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragmented_udp_invitations: Option<FragmentedUdpCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_fragmented_udp_invitations: Option<SessionFragmentedUdpCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_fragmented_host_groups: Option<SessionFragmentedHostGroupsCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready_game_launch: Option<ReadyGameLaunchCapabilities>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectPreparation {
    pub operation_id: String,
    pub generation: u64,
    pub public_id: String,
    pub expires_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitation_profile: Option<&'static str>,
}
/// Native-only result receipt. The manager retains any source-query worker.
#[derive(Debug)]
pub struct PendingHostReview {
    #[cfg(feature = "connect-lab")]
    result:
        Result<ConnectReview, tokio::sync::oneshot::Receiver<Result<ConnectReview, ConnectError>>>,
}
impl PendingHostReview {
    pub async fn wait(self) -> Result<ConnectReview, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            match self.result {
                Ok(review) => Ok(review),
                Err(receiver) => receiver.await.unwrap_or(Err(ConnectError::Internal)),
            }
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            Err(ConnectError::Disabled)
        }
    }
}
/// Observation while an owned game source is checked, before an identity exists.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingHostSource {
    pub kind: &'static str,
    pub mode: ConnectMode,
    pub profile: &'static str,
}
#[cfg(all(windows, feature = "connect-lab"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveredPreparationError {
    Connect(ConnectError),
    Discovery(super::connect_discovery::DiscoveryError),
}
#[cfg(all(windows, feature = "connect-lab"))]
impl DiscoveredPreparationError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Connect(error) => error.code(),
            Self::Discovery(error) => error.code(),
        }
    }
}
/// Only the result receiver leaves the manager. Dropping it cannot cancel or
/// detach the source work retained in the reserved Attempt.
#[cfg(all(windows, feature = "connect-lab"))]
#[derive(Debug)]
pub struct PendingHostPreparation {
    result: tokio::sync::oneshot::Receiver<Result<ConnectPreparation, DiscoveredPreparationError>>,
}
#[cfg(all(windows, feature = "connect-lab"))]
impl PendingHostPreparation {
    pub async fn wait(self) -> Result<ConnectPreparation, DiscoveredPreparationError> {
        self.result
            .await
            .unwrap_or(Err(DiscoveredPreparationError::Connect(
                ConnectError::Internal,
            )))
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReview {
    pub service_id: &'static str,
    pub protocol: &'static str,
    pub revision: u32,
    pub required_capability: &'static str,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectReview {
    pub operation_id: String,
    pub generation: u64,
    pub digest: String,
    pub invitation_digest: String,
    pub role: &'static str,
    pub mode: ConnectMode,
    pub host_public_id: String,
    pub guest_public_id: String,
    pub direct_address: String,
    pub game_target: Option<String>,
    pub local_tcp_bind: Option<String>,
    pub services: Vec<ServiceReview>,
    pub expires_at_ms: u64,
    pub invitation_expires_at_ms: u64,
    pub lifetime_seconds: u16,
    pub status_protocol: i32,
    pub status_handshake_host: &'static str,
    pub status_handshake_port: u16,
    pub target_source: Option<&'static str>,
    pub instance_id: Option<String>,
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_timing: Option<SessionTiming>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragmented_udp: Option<FragmentedUdpReview>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationExport {
    pub invitation: String,
    pub host_public_id: String,
    pub expires_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_timing: Option<SessionTiming>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragmented_udp: Option<FragmentedUdpContract>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusSummary {
    pub requested_protocol: i32,
    pub reported_protocol: i32,
    pub protocol_matches_requested: bool,
    pub reported_players_online: Option<u32>,
    pub reported_players_max: Option<u32>,
    pub connect_micros: u64,
    pub status_micros: u64,
    pub ping_micros: u64,
    pub total_micros: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub social: Option<SocialSnapshot>,
    pub available: bool,
    pub operation_id: Option<String>,
    pub generation: u64,
    pub role: Option<&'static str>,
    pub phase: &'static str,
    pub transport_state: &'static str,
    pub status_state: &'static str,
    pub tcp_address: Option<String>,
    pub expires_at_ms: Option<u64>,
    pub status_summary: Option<StatusSummary>,
    pub error_code: Option<&'static str>,
    pub world_state: &'static str,
    pub simple_voice_chat_state: &'static str,
    pub plasmo_voice_state: &'static str,
    pub relay_enabled: bool,
    pub path_state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_observation: Option<ConnectPathObservation>,
    pub cleanup_state: &'static str,
    pub preparation: Option<ConnectPreparation>,
    pub review: Option<ConnectReview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitation_profile: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_timing: Option<SessionTiming>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragmented_udp: Option<FragmentedUdpContract>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub udp: Option<FragmentedGuestState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub udp_recovery: Option<UdpRecoveryState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_group: Option<HostGroupSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_group_source: Option<PendingGroupSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_host_source: Option<PendingHostSource>,
}
impl ConnectSnapshot {
    fn idle() -> Self {
        Self {
            social: None,
            available: cfg!(feature = "connect-lab"),
            operation_id: None,
            generation: 0,
            role: None,
            phase: "idle",
            transport_state: "notStarted",
            status_state: "notChecked",
            tcp_address: None,
            expires_at_ms: None,
            status_summary: None,
            error_code: None,
            world_state: "notVerified",
            simple_voice_chat_state: "disabled",
            plasmo_voice_state: "disabled",
            relay_enabled: false,
            path_state: "notVerified",
            path_observation: None,
            cleanup_state: "notStarted",
            preparation: None,
            review: None,
            invitation_profile: None,
            session_timing: None,
            fragmented_udp: None,
            udp: None,
            udp_recovery: None,
            host_group: None,
            pending_group_source: None,
            pending_host_source: None,
        }
    }
}
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConnectError {
    #[error("connect_disabled")]
    Disabled,
    #[error("connect_busy")]
    Busy,
    #[error("connect_invalid_request")]
    InvalidRequest,
    #[error("connect_stale_operation")]
    StaleOperation,
    #[error("connect_owner_mismatch")]
    OwnerMismatch,
    #[error("connect_plan_expired")]
    PlanExpired,
    #[error("connect_wrong_digest")]
    WrongDigest,
    #[error("connect_invitation_rejected")]
    InvitationRejected,
    #[error("connect_transport_failed")]
    TransportFailed,
    #[error("connect_port_unavailable")]
    PortUnavailable,
    #[error("connect_status_failed")]
    StatusFailed,
    #[error("connect_cancelled")]
    Cancelled,
    #[error("connect_internal")]
    Internal,
    #[error("connect_source_changed")]
    SourceChanged,
    #[error("connect_cleanup_failed")]
    CleanupFailed,
    #[error("connect_social_quota")]
    SocialQuota,
    #[error("connect_social_unavailable")]
    SocialUnavailable,
    #[error("{1}")]
    SocialFailure(SocialFailureStage, SocialFailureReason),
}
impl ConnectError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Disabled => "connect_disabled",
            Self::Busy => "connect_busy",
            Self::InvalidRequest => "connect_invalid_request",
            Self::StaleOperation => "connect_stale_operation",
            Self::OwnerMismatch => "connect_owner_mismatch",
            Self::PlanExpired => "connect_plan_expired",
            Self::WrongDigest => "connect_wrong_digest",
            Self::InvitationRejected => "connect_invitation_rejected",
            Self::TransportFailed => "connect_transport_failed",
            Self::PortUnavailable => "connect_port_unavailable",
            Self::StatusFailed => "connect_status_failed",
            Self::Cancelled => "connect_cancelled",
            Self::Internal => "connect_internal",
            Self::SourceChanged => "connect_source_changed",
            Self::CleanupFailed => "connect_cleanup_failed",
            Self::SocialQuota => "connect_social_quota",
            Self::SocialUnavailable => "connect_social_unavailable",
            Self::SocialFailure(_, reason) => reason.code(),
        }
    }
}
#[derive(Default)]
pub struct ConnectManager {
    #[cfg(feature = "connect-lab")]
    live: live::Manager,
}
impl ConnectManager {
    /// Possession proof only. It neither accepts a social admission nor starts transport.
    #[cfg(feature = "connect-lab")]
    pub fn prove_social_key_binding(
        &self,
        owner: &str,
        operation_id: &str,
        challenge_json: &str,
    ) -> Result<SocialKeyBindingProof, ConnectError> {
        self.live
            .prove_social_key_binding(owner, operation_id, challenge_json)
    }
    pub fn retry_session_udp(
        &self,
        owner: &str,
        operation_id: &str,
        expected_generation: u64,
    ) -> Result<ConnectSnapshot, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live
                .retry_session_udp(owner, operation_id, expected_generation)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id, expected_generation);
            Err(ConnectError::Disabled)
        }
    }
    pub fn capabilities(&self) -> ConnectCapabilities {
        ConnectCapabilities {
            available: cfg!(feature = "connect-lab"),
            experimental: true,
            max_lifetime_seconds: 600,
            modes: if cfg!(feature = "connect-lab") {
                vec![
                    ConnectMode::Loopback,
                    ConnectMode::ExternalIpv6,
                    ConnectMode::DirectIpv4,
                ]
            } else {
                vec![ConnectMode::Loopback, ConnectMode::ExternalIpv6]
            },
            services: vec!["game"],
            voice_enabled: false,
            relay_enabled: false,
            status_protocol: 767,
            status_handshake_host: "localhost",
            status_handshake_port: 43001,
            owned_instance_discovery: cfg!(all(windows, feature = "connect-lab")),
            session_invitations: cfg!(feature = "connect-lab").then_some(
                SessionInvitationCapabilities {
                    profile: "sessionGameTcp",
                    schema_version: 2,
                    max_admission_seconds: 600,
                    max_session_seconds: 3600,
                },
            ),
            fragmented_udp_invitations: cfg!(feature = "connect-lab")
                .then(FragmentedUdpCapabilities::default),
            session_fragmented_udp_invitations: cfg!(feature = "connect-lab")
                .then(SessionFragmentedUdpCapabilities::default),
            session_fragmented_host_groups: cfg!(feature = "connect-lab")
                .then(SessionFragmentedHostGroupsCapabilities::default),
            ready_game_launch: cfg!(all(windows, feature = "connect-lab"))
                .then(ReadyGameLaunchCapabilities::default),
        }
    }
    pub fn snapshot(&self, owner: &str) -> Result<ConnectSnapshot, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.snapshot(owner)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = owner;
            Ok(ConnectSnapshot::idle())
        }
    }
    pub fn prepare_host(
        &self,
        owner: &str,
        request: HostRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.prepare_host(owner, request)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, request);
            Err(ConnectError::Disabled)
        }
    }
    pub fn prepare_session_group_host(
        &self,
        owner: &str,
        request: GroupHostRequest,
    ) -> Result<GroupPreparation, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.prepare_session_group_host(owner, request)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, request);
            Err(ConnectError::Disabled)
        }
    }
    pub fn prepare_session_group_host_discovered(
        &self,
        owner: &str,
        request: GroupDiscoveredHostRequest,
        selection_id: String,
        discovery: super::connect_discovery::DiscoveryManager,
    ) -> Result<ConnectSnapshot, ConnectError> {
        #[cfg(all(windows, feature = "connect-lab"))]
        {
            self.live
                .prepare_session_group_host_discovered(owner, request, selection_id, discovery)
        }
        #[cfg(not(all(windows, feature = "connect-lab")))]
        {
            let _ = (owner, request, selection_id, discovery);
            Err(ConnectError::Disabled)
        }
    }
    pub fn review_session_group_host(
        &self,
        owner: &str,
        operation_id: &str,
        participants: Vec<GroupParticipantRequest>,
    ) -> Result<GroupReview, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live
                .review_session_group_host(owner, operation_id, participants)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id, participants);
            Err(ConnectError::Disabled)
        }
    }
    pub async fn confirm_session_group_host(
        &self,
        owner: &str,
        operation_id: &str,
        digest: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live
                .confirm_session_group_host(owner, operation_id, digest)
                .await
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id, digest);
            Err(ConnectError::Disabled)
        }
    }
    pub fn export_session_group_invitation(
        &self,
        owner: &str,
        operation_id: &str,
        participant_id: &str,
    ) -> Result<GroupInvitationExport, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live
                .export_session_group_invitation(owner, operation_id, participant_id)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id, participant_id);
            Err(ConnectError::Disabled)
        }
    }
    pub fn revoke_session_group_participant(
        &self,
        owner: &str,
        operation_id: &str,
        participant_id: &str,
        expected_generation: u64,
    ) -> Result<ConnectSnapshot, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.revoke_session_group_participant(
                owner,
                operation_id,
                participant_id,
                expected_generation,
            )
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id, participant_id, expected_generation);
            Err(ConnectError::Disabled)
        }
    }
    pub fn prepare_fragmented_host(
        &self,
        owner: &str,
        request: FragmentedHostRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.prepare_fragmented_host(owner, request)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, request);
            Err(ConnectError::Disabled)
        }
    }
    pub fn prepare_fragmented_guest(
        &self,
        owner: &str,
        request: GuestRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.prepare_fragmented_guest(owner, request)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, request);
            Err(ConnectError::Disabled)
        }
    }
    pub fn prepare_session_fragmented_host(
        &self,
        owner: &str,
        request: SessionFragmentedHostRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.prepare_session_fragmented_host(owner, request)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, request);
            Err(ConnectError::Disabled)
        }
    }
    pub fn prepare_session_fragmented_guest(
        &self,
        owner: &str,
        request: GuestRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.prepare_session_fragmented_guest(owner, request)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, request);
            Err(ConnectError::Disabled)
        }
    }
    pub fn prepare_session_host(
        &self,
        owner: &str,
        request: SessionHostRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.prepare_session_host(owner, request)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, request);
            Err(ConnectError::Disabled)
        }
    }
    pub fn prepare_session_guest(
        &self,
        owner: &str,
        request: GuestRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.prepare_session_guest(owner, request)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, request);
            Err(ConnectError::Disabled)
        }
    }
    #[cfg(all(windows, feature = "connect-lab"))]
    pub fn reserve_session_fragmented_host_discovered(
        &self,
        owner: &str,
        request: SessionFragmentedDiscoveredHostRequest,
        selection_id: String,
        discovery: super::connect_discovery::DiscoveryManager,
    ) -> Result<PendingHostPreparation, ConnectError> {
        self.live.reserve_session_fragmented_host_discovered(
            owner,
            request,
            selection_id,
            discovery,
        )
    }
    #[cfg(all(windows, feature = "connect-lab"))]
    pub fn reserve_session_host_discovered(
        &self,
        owner: &str,
        request: SessionHostRequest,
        selection_id: String,
        discovery: super::connect_discovery::DiscoveryManager,
    ) -> Result<PendingHostPreparation, ConnectError> {
        self.live
            .reserve_session_host_discovered(owner, request, selection_id, discovery)
    }
    #[cfg(all(windows, feature = "connect-lab"))]
    pub fn reserve_host_discovered(
        &self,
        owner: &str,
        request: HostRequest,
        selection_id: String,
        discovery: super::connect_discovery::DiscoveryManager,
    ) -> Result<PendingHostPreparation, ConnectError> {
        self.live
            .reserve_host_discovered(owner, request, selection_id, discovery)
    }
    pub fn reserve_host_review(
        &self,
        owner: &str,
        operation_id: &str,
        guest_public_id: &str,
    ) -> Result<PendingHostReview, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live
                .reserve_host_review(owner, operation_id, guest_public_id)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id, guest_public_id);
            Err(ConnectError::Disabled)
        }
    }
    pub fn prepare_guest(
        &self,
        owner: &str,
        request: GuestRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.prepare_guest(owner, request)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, request);
            Err(ConnectError::Disabled)
        }
    }
    pub fn review_guest(
        &self,
        owner: &str,
        operation_id: &str,
        invitation: &str,
    ) -> Result<ConnectReview, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.review_guest(owner, operation_id, invitation)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id, invitation);
            Err(ConnectError::Disabled)
        }
    }
    pub async fn confirm(
        &self,
        owner: &str,
        operation_id: &str,
        digest: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.confirm(owner, operation_id, digest).await
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id, digest);
            Err(ConnectError::Disabled)
        }
    }
    pub fn export_invitation(
        &self,
        owner: &str,
        operation_id: &str,
    ) -> Result<InvitationExport, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.export_invitation(owner, operation_id)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id);
            Err(ConnectError::Disabled)
        }
    }
    pub async fn probe_status(
        &self,
        owner: &str,
        operation_id: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.probe_status(owner, operation_id).await
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id);
            Err(ConnectError::Disabled)
        }
    }
    pub async fn stop(
        &self,
        owner: &str,
        operation_id: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.stop(owner, operation_id).await
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id);
            Err(ConnectError::Disabled)
        }
    }
    pub async fn shutdown_all(&self) {
        #[cfg(feature = "connect-lab")]
        self.live.shutdown_all().await;
    }
}

#[cfg(all(test, not(feature = "connect-lab")))]
mod disabled_tests {
    use super::*;
    #[test]
    fn default_build_is_disabled_and_exposes_no_live_resources() {
        let manager = ConnectManager::default();
        assert!(!manager.capabilities().available);
        let caps = serde_json::to_value(manager.capabilities()).unwrap();
        assert_eq!(
            caps["modes"],
            serde_json::json!(["loopback", "externalIpv6"])
        );
        assert!(matches!(
            manager.prepare_guest(
                "main",
                GuestRequest {
                    mode: ConnectMode::DirectIpv4,
                    expected_host_public_id: String::new(),
                }
            ),
            Err(ConnectError::Disabled)
        ));
        assert!(caps.get("sessionInvitations").is_none());
        assert!(caps.get("fragmentedUdpInvitations").is_none());
        assert!(caps.get("sessionFragmentedUdpInvitations").is_none());
        let idle = serde_json::to_value(manager.snapshot("main").unwrap()).unwrap();
        assert!(idle.get("udpRecovery").is_none());
        assert!(idle.get("pendingHostSource").is_none());
        assert!(idle.get("pendingGroupSource").is_none());
        assert!(matches!(
            manager.retry_session_udp("main", "none", 0),
            Err(ConnectError::Disabled)
        ));
        assert!(matches!(
            manager.prepare_session_fragmented_host(
                "main",
                SessionFragmentedHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: "127.0.0.1:32000".into(),
                    game_target: "127.0.0.1:32001".into(),
                    udp_target: "127.0.0.1:32002".into(),
                    admission_seconds: 600,
                    session_seconds: 3600,
                }
            ),
            Err(ConnectError::Disabled)
        ));
        assert!(matches!(
            manager.prepare_session_fragmented_guest(
                "main",
                GuestRequest {
                    mode: ConnectMode::Loopback,
                    expected_host_public_id: String::new(),
                }
            ),
            Err(ConnectError::Disabled)
        ));
        assert!(matches!(
            manager.prepare_fragmented_host(
                "main",
                FragmentedHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: "127.0.0.1:32000".into(),
                    game_target: "127.0.0.1:32001".into(),
                    udp_target: "127.0.0.1:32002".into(),
                    lifetime_seconds: 120
                }
            ),
            Err(ConnectError::Disabled)
        ));
        assert!(matches!(
            manager.prepare_fragmented_guest(
                "main",
                GuestRequest {
                    mode: ConnectMode::Loopback,
                    expected_host_public_id: String::new()
                }
            ),
            Err(ConnectError::Disabled)
        ));
        assert_eq!(caps["maxLifetimeSeconds"], 600);
        assert!(matches!(
            manager.prepare_session_host(
                "main",
                SessionHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: "127.0.0.1:32000".into(),
                    game_target: "127.0.0.1:32001".into(),
                    admission_seconds: 600,
                    session_seconds: 3600
                }
            ),
            Err(ConnectError::Disabled)
        ));
        assert!(matches!(
            manager.prepare_session_guest(
                "main",
                GuestRequest {
                    mode: ConnectMode::Loopback,
                    expected_host_public_id: String::new()
                }
            ),
            Err(ConnectError::Disabled)
        ));
        assert!(manager.snapshot("main").unwrap().operation_id.is_none());
        assert!(matches!(
            manager.prepare_guest(
                "main",
                GuestRequest {
                    mode: ConnectMode::Loopback,
                    expected_host_public_id: String::new()
                }
            ),
            Err(ConnectError::Disabled)
        ));
    }
}
