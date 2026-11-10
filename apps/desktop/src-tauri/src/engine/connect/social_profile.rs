//! Closed desktop API for social admission within the existing Connect attempt.
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

/// Native receipt of one source claim. Never accepted from IPC or serialized.
#[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
#[derive(Clone)]
pub(crate) struct SocialNativeSourceReceipt {
    pub(super) owner: String,
    pub(super) operation: String,
    pub(super) generation: u64,
    pub(super) epoch: u64,
    pub(super) account: String,
    pub(super) session: String,
    pub(super) claim: String,
}
#[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
impl SocialNativeSourceReceipt {
    pub(crate) fn session_id(&self) -> &str {
        &self.session
    }
    pub(crate) fn matches_binding(
        &self,
        binding: &crate::engine::connect_voice::dedicated::NativeVoiceHostBinding,
    ) -> bool {
        binding
            .claim_digest()
            .is_ok_and(|claim| claim == self.claim)
    }
    pub(crate) fn review_scope(&self) -> Result<String, ConnectError> {
        use sha2::{Digest, Sha256};
        let bytes = serde_json::to_vec(&(
            "aurora.connect.native-source-reclaim-receipt/v1",
            &self.owner,
            &self.operation,
            self.generation,
            self.epoch,
            &self.account,
            &self.session,
            &self.claim,
        ))
        .map_err(|_| ConnectError::Internal)?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_invitations: Option<bool>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SocialParticipant {
    pub grant_id: String,
    pub account_id: String,
    pub state: &'static str,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SocialFailureStage {
    Authentication,
    Bootstrap,
    Identity,
    KeyChallenge,
    HostReview,
    HostRegistration,
    GuestAdmission,
    SessionContinuity,
    Relay,
    SessionControl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocialFailureReason {
    Authentication,
    AccountNotRegistered,
    NotAuthorized,
    AuthorityChanged,
    ClockUncertain,
    ClockUnverified,
    ServiceUnavailable,
    RequestRejected,
    RequestTimedOut,
    ContinuityLost,
    Quota,
}
impl SocialFailureReason {
    pub(super) fn code(self) -> &'static str {
        match self {
            Self::Authentication => "connect_social_auth_failed",
            Self::AccountNotRegistered => "connect_social_account_not_registered",
            Self::NotAuthorized => "connect_social_not_authorized",
            Self::AuthorityChanged => "connect_social_authority_changed",
            Self::ClockUncertain => "connect_social_clock_uncertain",
            Self::ClockUnverified => "connect_social_clock_unverified",
            Self::ServiceUnavailable => "connect_social_service_unavailable",
            Self::RequestRejected => "connect_social_request_rejected",
            Self::RequestTimedOut => "connect_social_request_timed_out",
            Self::ContinuityLost => "connect_social_continuity_lost",
            Self::Quota => "connect_social_quota",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SocialFailureGate {
    HostPoll,
    Source,
    RelayTicket,
    Invitation,
    Session,
    HostTransport,
    Other,
}

/// Relative observations only. No authority, identity, endpoint or credential.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialFailureObservation {
    pub gate: SocialFailureGate,
    pub bootstrap_roundtrip_ms: Option<u64>,
    pub bootstrap_uncertainty_ms: Option<u64>,
    pub host_poll_elapsed_ms: Option<u64>,
    pub host_poll_age_ms: Option<u64>,
    pub session_remaining_ms: u64,
}
impl std::fmt::Display for SocialFailureReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready_game_launch_available: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_state: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_player_voice_state: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_player: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_guests: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participants: Option<Vec<SocialParticipant>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_transport_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_stage: Option<SocialFailureStage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_observation: Option<SocialFailureObservation>,
}

/// Supplied by the trusted process launcher or the compiled HTTPS origin.
/// Renderer IPC cannot supply an authority URL, issuer or verification key.
#[cfg(feature = "connect-lab")]
#[derive(Clone)]
pub struct SocialLabAuthority {
    pub(super) pin: aurora_connect_lab::EndpointId,
    pub(super) issuer: String,
    pub(super) online_origin: Option<&'static str>,
    pub(super) private_invitations: bool,
    pub(super) join_policy: String,
    pub(super) session_expiry: u64,
    pub(super) max_players: u8,
}
#[cfg(feature = "connect-lab")]
impl SocialLabAuthority {
    pub(super) fn private_invitations_enabled(&self) -> bool {
        cfg!(debug_assertions) && self.online_origin.is_none() && self.private_invitations
    }
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
            online_origin: None,
            // Captured once from the trusted lab process, never from IPC or refresh.
            private_invitations: cfg!(debug_assertions)
                && std::env::var("AURORA_SOCIAL_PRIVATE_LAB").as_deref() == Ok("1"),
            join_policy: "open".into(),
            session_expiry: 0,
            max_players: 1,
        })
    }
    /// Build-owned HTTPS authority. No IPC or saved user configuration can change it.
    /// The provisional key is unusable: execute() must fetch authenticated bootstrap first.
    pub fn from_compiled_online() -> Result<Self, ConnectError> {
        let origin = option_env!("AURORA_SOCIAL_ORIGIN").ok_or(ConnectError::Disabled)?;
        let url = reqwest::Url::parse(origin).map_err(|_| ConnectError::InvalidRequest)?;
        if url.scheme() != "https"
            || url.origin().ascii_serialization() != origin
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.port().is_some()
            || url
                .host_str()
                .is_none_or(|host| host == "localhost" || host.parse::<std::net::IpAddr>().is_ok())
        {
            return Err(ConnectError::InvalidRequest);
        }
        Ok(Self {
            pin: aurora_connect_lab::generate_key().public(),
            issuer: String::new(),
            online_origin: Some(origin),
            private_invitations: false,
            join_policy: "open".into(),
            session_expiry: 0,
            max_players: 0,
        })
    }
}

impl ConnectManager {
    pub fn social_revoke_participant(
        &self,
        owner: &str,
        operation_id: &str,
        account_epoch: u64,
        grant_id: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live
                .social_revoke_participant(owner, operation_id, account_epoch, grant_id)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id, account_epoch, grant_id);
            Err(ConnectError::Disabled)
        }
    }
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
                private_invitations: None,
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
    #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
    pub(crate) fn social_reserve_native_voice(
        &self,
        owner: &str,
        request: SocialDiscoveredReserveRequest,
        binding: crate::engine::connect_voice::dedicated::NativeVoiceHostBinding,
        host_player: bool,
    ) -> Result<SocialReservation, ConnectError> {
        self.live
            .social_reserve_native_voice(owner, request, binding, host_player)
    }
    #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
    pub(crate) fn social_reserve_native_voice_recorded(
        &self,
        owner: &str,
        request: SocialDiscoveredReserveRequest,
        binding: crate::engine::connect_voice::dedicated::NativeVoiceHostBinding,
        host_player: bool,
    ) -> Result<(SocialReservation, SocialNativeSourceReceipt), ConnectError> {
        let claim = binding
            .claim_digest()
            .map_err(|_| ConnectError::SourceChanged)?;
        let epoch = request.account_epoch;
        let account = request.account_id.clone();
        let session = request.session_id.clone();
        let reservation = self.social_reserve_native_voice(owner, request, binding, host_player)?;
        let receipt = SocialNativeSourceReceipt {
            owner: owner.into(),
            operation: reservation.operation_id.clone(),
            generation: reservation.generation,
            epoch,
            account,
            session,
            claim,
        };
        Ok((reservation, receipt))
    }
    #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
    pub(crate) fn can_reclaim_native_source(&self, receipt: &SocialNativeSourceReceipt) -> bool {
        self.live.can_reclaim_native_source(receipt)
    }
    #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
    pub(crate) async fn join_reclaim_native_source(
        &self,
        receipt: &SocialNativeSourceReceipt,
    ) -> Result<(), ConnectError> {
        self.live.join_reclaim_native_source(receipt).await
    }
    #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
    pub(crate) fn with_reclaim_native_source<T>(
        &self,
        receipt: &SocialNativeSourceReceipt,
        work: impl FnOnce() -> Result<T, ConnectError>,
    ) -> Result<T, ConnectError> {
        self.live.with_reclaim_native_source(receipt, work)
    }
    pub fn social_refresh_authentication(
        &self,
        owner: &str,
        operation_id: &str,
        account_epoch: u64,
        id_token: String,
    ) -> Result<(), ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live
                .social_refresh_authentication(owner, operation_id, account_epoch, id_token)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation_id, account_epoch, id_token);
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
    pub fn social_confirm_guest(
        &self,
        owner: &str,
        operation_id: &str,
        generation: u64,
        digest: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live
                .social_confirm_guest(owner, operation_id, generation, digest)
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
