//! Explicit experimental desktop caller. Public DTOs exist in default builds;
//! transport, private keys and sockets exist only with the connect-lab feature.
use serde::{Deserialize, Serialize};

#[cfg(feature = "connect-lab")]
mod live;
#[cfg(all(test, feature = "connect-lab"))]
mod tests;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConnectMode {
    Loopback,
    ExternalIpv6,
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
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectPreparation {
    pub operation_id: String,
    pub generation: u64,
    pub public_id: String,
    pub expires_at_ms: u64,
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
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationExport {
    pub invitation: String,
    pub host_public_id: String,
    pub expires_at_ms: u64,
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
    pub cleanup_state: &'static str,
    pub preparation: Option<ConnectPreparation>,
    pub review: Option<ConnectReview>,
}
impl ConnectSnapshot {
    fn idle() -> Self {
        Self {
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
            cleanup_state: "notStarted",
            preparation: None,
            review: None,
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
        }
    }
}
#[derive(Default)]
pub struct ConnectManager {
    #[cfg(feature = "connect-lab")]
    live: live::Manager,
}
impl ConnectManager {
    pub fn capabilities(&self) -> ConnectCapabilities {
        ConnectCapabilities {
            available: cfg!(feature = "connect-lab"),
            experimental: true,
            max_lifetime_seconds: 600,
            modes: vec![ConnectMode::Loopback, ConnectMode::ExternalIpv6],
            services: vec!["game"],
            voice_enabled: false,
            relay_enabled: false,
            status_protocol: 767,
            status_handshake_host: "localhost",
            status_handshake_port: 43001,
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
    pub fn review_host(
        &self,
        owner: &str,
        operation_id: &str,
        guest_public_id: &str,
    ) -> Result<ConnectReview, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.review_host(owner, operation_id, guest_public_id)
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
