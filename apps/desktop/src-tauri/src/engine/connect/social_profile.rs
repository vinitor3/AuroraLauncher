//! Closed desktop API for the explicitly provisioned local social experiment.
use super::*;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SocialReserveRequest {
    pub account_epoch: u64,
    pub account_id: String,
    pub session_id: String,
    pub role: String,
    pub game_target: Option<String>,
    pub visibility: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SocialDiscoveredReserveRequest {
    pub account_epoch: u64,
    pub account_id: String,
    pub session_id: String,
    pub visibility: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialReservation {
    pub operation_id: String,
    pub generation: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialCapabilities {
    pub available: bool,
    pub experimental: bool,
    pub account_epoch: u64,
    pub max_session_seconds: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owned_game_tcp_discovery: Option<bool>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialSnapshot {
    pub session_id: String,
    pub role: String,
    pub stage: &'static str,
    pub digest: Option<String>,
    pub game_target: Option<String>,
    pub visibility: Option<String>,
    pub expires_at_ms: Option<u64>,
}

/// Supplied by the trusted local process launcher, never by an IPC command.
/// Only the fixed loopback authority is supported; ordinary builds use None.
#[cfg(feature = "connect-lab")]
#[derive(Clone)]
pub struct SocialLabAuthority {
    pub(super) pin: aurora_connect_lab::EndpointId,
    pub(super) issuer: String,
}
#[cfg(feature = "connect-lab")]
impl SocialLabAuthority {
    pub fn from_local_process(pin: &str, issuer: &str) -> Result<Self, ConnectError> {
        let lower_hex = |text: &str| {
            text.len() == 64
                && text
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if !lower_hex(pin) || !lower_hex(issuer) {
            return Err(ConnectError::InvalidRequest);
        }
        Ok(Self {
            pin: pin.parse().map_err(|_| ConnectError::InvalidRequest)?,
            issuer: issuer.into(),
        })
    }
}

impl ConnectManager {
    #[cfg(feature = "connect-lab")]
    pub fn with_social_lab(authority: SocialLabAuthority) -> Self {
        Self {
            live: live::Manager::with_social_lab(authority),
        }
    }
    pub fn social_capabilities(&self) -> SocialCapabilities {
        #[cfg(feature = "connect-lab")]
        {
            self.live.social_capabilities()
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            SocialCapabilities {
                available: false,
                experimental: true,
                account_epoch: 0,
                max_session_seconds: 60,
                owned_game_tcp_discovery: None,
            }
        }
    }
    pub fn social_reserve(
        &self,
        owner: &str,
        request: SocialReserveRequest,
    ) -> Result<SocialReservation, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.social_reserve(owner, request)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, request);
            Err(ConnectError::Disabled)
        }
    }
    pub fn social_authenticate(
        &self,
        owner: &str,
        operation_id: &str,
        generation: u64,
        id_token: String,
    ) -> Result<ConnectSnapshot, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live
                .social_authenticate(owner, operation_id, generation, id_token)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id, generation, id_token);
            Err(ConnectError::Disabled)
        }
    }
    pub fn social_reserve_discovered(
        &self,
        owner: &str,
        request: SocialDiscoveredReserveRequest,
        selection_id: String,
        discovery: super::super::connect_discovery::DiscoveryManager,
    ) -> Result<SocialReservation, ConnectError> {
        #[cfg(all(windows, feature = "connect-lab"))]
        {
            self.live
                .social_reserve_discovered(owner, request, selection_id, discovery)
        }
        #[cfg(not(all(windows, feature = "connect-lab")))]
        {
            let _ = (owner, request, selection_id, discovery);
            Err(ConnectError::Disabled)
        }
    }
    pub fn social_confirm_host(
        &self,
        owner: &str,
        operation_id: &str,
        generation: u64,
        digest: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live
                .social_confirm_host(owner, operation_id, generation, digest)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id, generation, digest);
            Err(ConnectError::Disabled)
        }
    }
    pub async fn account_transition_stop(
        &self,
        owner: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.account_transition_stop(owner).await
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = owner;
            Ok(ConnectSnapshot::idle())
        }
    }
}
