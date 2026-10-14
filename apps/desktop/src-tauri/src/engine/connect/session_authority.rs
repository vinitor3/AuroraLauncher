//! Private desktop authority. Clones retain the library's original deadlines;
//! projections never reparse bytes to construct a replacement authorization.
use super::*;
use aurora_connect_lab as lab;
use std::{net::SocketAddr, time::Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Profile {
    Legacy,
    Session,
    Fragmented,
    SessionFragmented,
}
impl Profile {
    pub(super) fn name(self) -> Option<&'static str> {
        match self {
            Self::Legacy => None,
            Self::Session => Some("sessionGameTcp"),
            Self::Fragmented => Some(fragmented_profile::PROFILE),
            Self::SessionFragmented => Some(fragmented_profile::SESSION_PROFILE),
        }
    }
    pub(super) fn is_fragmented(self) -> bool {
        matches!(self, Self::Fragmented | Self::SessionFragmented)
    }
}
pub(super) struct HostDraftRequest {
    pub mode: ConnectMode,
    pub bind_address: String,
    pub game_target: String,
    pub udp_target: Option<String>,
    pub admission_seconds: u16,
    pub session_seconds: u16,
    pub profile: Profile,
}
impl From<HostRequest> for HostDraftRequest {
    fn from(value: HostRequest) -> Self {
        Self {
            mode: value.mode,
            bind_address: value.bind_address,
            game_target: value.game_target,
            udp_target: None,
            admission_seconds: value.lifetime_seconds,
            session_seconds: value.lifetime_seconds,
            profile: Profile::Legacy,
        }
    }
}
impl From<SessionHostRequest> for HostDraftRequest {
    fn from(value: SessionHostRequest) -> Self {
        Self {
            mode: value.mode,
            bind_address: value.bind_address,
            game_target: value.game_target,
            udp_target: None,
            admission_seconds: value.admission_seconds,
            session_seconds: value.session_seconds,
            profile: Profile::Session,
        }
    }
}
impl HostDraftRequest {
    pub(super) fn validate(&self) -> Result<(), ConnectError> {
        if self.profile == Profile::Fragmented {
            fragmented_profile::validate_lifetime(self.admission_seconds)?;
            if self.udp_target.is_none() || self.admission_seconds != self.session_seconds {
                return Err(ConnectError::InvalidRequest);
            }
        } else if self.profile == Profile::SessionFragmented {
            if self.udp_target.is_none() {
                return Err(ConnectError::InvalidRequest);
            }
        } else if self.udp_target.is_some() {
            return Err(ConnectError::InvalidRequest);
        }
        if self.admission_seconds == 0
            || self.admission_seconds > 600
            || self.session_seconds < self.admission_seconds
            || self.session_seconds > 3600
            || (self.profile == Profile::Legacy && self.session_seconds != self.admission_seconds)
        {
            return Err(ConnectError::InvalidRequest);
        }
        Ok(())
    }
}
impl From<FragmentedHostRequest> for HostDraftRequest {
    fn from(value: FragmentedHostRequest) -> Self {
        Self {
            mode: value.mode,
            bind_address: value.bind_address,
            game_target: value.game_target,
            udp_target: Some(value.udp_target),
            admission_seconds: value.lifetime_seconds,
            session_seconds: value.lifetime_seconds,
            profile: Profile::Fragmented,
        }
    }
}
impl From<SessionFragmentedHostRequest> for HostDraftRequest {
    fn from(value: SessionFragmentedHostRequest) -> Self {
        Self {
            mode: value.mode,
            bind_address: value.bind_address,
            game_target: value.game_target,
            udp_target: Some(value.udp_target),
            admission_seconds: value.admission_seconds,
            session_seconds: value.session_seconds,
            profile: Profile::SessionFragmented,
        }
    }
}

#[derive(Clone)]
pub(super) enum Authority {
    Legacy(lab::Invitation),
    Session(lab::SessionInvitation),
    Fragmented(lab::Invitation),
    SessionFragmented(lab::SessionInvitation),
}
impl Authority {
    pub(super) fn deadlines(&self, legacy: (Instant, Instant)) -> lab::Result<(Instant, Instant)> {
        match self {
            Self::Legacy(_) | Self::Fragmented(_) => Ok(legacy),
            Self::Session(value) | Self::SessionFragmented(value) => {
                // Take the anchor BEFORE asking the original object for its
                // remaining budgets. Nothing is reconstructed from a maximum.
                let at = Instant::now();
                Ok((
                    at + value.admission_remaining()?,
                    at + value.session_remaining()?,
                ))
            }
        }
    }
    pub(super) fn check_admission(&self) -> lab::Result<()> {
        match self {
            // Legacy keeps the caller's captured monotonic deadline and the
            // library's private checks. Do not reverify its bytes to get a new
            // deadline merely to observe the public absolute expiry here.
            Self::Legacy(v) | Self::Fragmented(v)
                if lab::unix_millis() >= v.expires_at_unix_ms() =>
            {
                Err(lab::Error::Expired)
            }
            Self::Legacy(_) | Self::Fragmented(_) => Ok(()),
            Self::Session(v) | Self::SessionFragmented(v) => v.check_admission_current(),
        }
    }
    pub(super) fn check_session(&self) -> lab::Result<()> {
        match self {
            Self::Legacy(_) | Self::Fragmented(_) => self.check_admission(),
            Self::Session(v) | Self::SessionFragmented(v) => v.check_session_current(),
        }
    }
    pub(super) fn lease(&self) -> lab::Lease {
        match self {
            Self::Legacy(v) | Self::Fragmented(v) => v.lease(),
            Self::Session(v) | Self::SessionFragmented(v) => v.lease(),
        }
    }
    pub(super) fn session_id(&self) -> &str {
        match self {
            Self::Legacy(v) | Self::Fragmented(v) => v.session_id(),
            Self::Session(v) | Self::SessionFragmented(v) => v.session_id(),
        }
    }
    pub(super) fn direct_addr(&self) -> SocketAddr {
        match self {
            Self::Legacy(v) | Self::Fragmented(v) => v.direct_addr(),
            Self::Session(v) | Self::SessionFragmented(v) => v.direct_addr(),
        }
    }
    pub(super) fn digest(&self) -> String {
        match self {
            Self::Legacy(v) | Self::Fragmented(v) => v.digest(),
            Self::Session(v) | Self::SessionFragmented(v) => v.digest(),
        }
    }
    pub(super) fn encode(&self) -> String {
        match self {
            Self::Legacy(v) | Self::Fragmented(v) => v.encode(),
            Self::Session(v) | Self::SessionFragmented(v) => v.encode(),
        }
    }
    pub(super) fn admission_expires(&self) -> u64 {
        match self {
            Self::Legacy(v) | Self::Fragmented(v) => v.expires_at_unix_ms(),
            Self::Session(v) | Self::SessionFragmented(v) => v.admission_expires_at_unix_ms(),
        }
    }
    pub(super) fn session_expires(&self) -> u64 {
        match self {
            Self::Legacy(v) | Self::Fragmented(v) => v.expires_at_unix_ms(),
            Self::Session(v) | Self::SessionFragmented(v) => v.session_expires_at_unix_ms(),
        }
    }
    pub(super) fn preview(&self) -> serde_json::Value {
        match self {
            Self::Legacy(v) | Self::Fragmented(v) => v.preview(),
            Self::Session(v) | Self::SessionFragmented(v) => v.preview(),
        }
    }
    pub(super) fn timing(&self) -> Result<Option<SessionTiming>, ConnectError> {
        let (value, profile, schema_version) = match self {
            Self::Session(value) => (value, "sessionGameTcp", 2),
            Self::SessionFragmented(value) => (value, fragmented_profile::SESSION_PROFILE, 3),
            _ => return Ok(None),
        };
        let preview = value.preview();
        if preview["schemaVersion"] != schema_version || preview["profile"] != profile {
            return Err(ConnectError::InvitationRejected);
        }
        let issued = preview["issuedAtUnixMs"]
            .as_u64()
            .ok_or(ConnectError::InvitationRejected)?;
        let admission = value.admission_expires_at_unix_ms();
        let session = value.session_expires_at_unix_ms();
        let max_safe_integer = (1u64 << 53) - 1;
        if issued == 0
            || issued > max_safe_integer
            || admission > max_safe_integer
            || session > max_safe_integer
            || admission <= issued
            || session < admission
        {
            return Err(ConnectError::InvitationRejected);
        }
        if (admission - issued) % 1000 != 0 || (session - issued) % 1000 != 0 {
            return Err(ConnectError::InvitationRejected);
        }
        let admission_seconds = (admission - issued) / 1000;
        let session_seconds = (session - issued) / 1000;
        // UI fields use integral seconds from signed issuance, never remaining
        // time. No rounding of an absolute expiry is allowed to fit the DTO.
        if admission_seconds > 600
            || session_seconds > 3600
            || session - admission != (session_seconds - admission_seconds) * 1000
        {
            return Err(ConnectError::InvitationRejected);
        }
        Ok(Some(SessionTiming {
            profile,
            schema_version,
            admission_seconds: admission_seconds as u16,
            session_seconds: session_seconds as u16,
            admission_expires_at_ms: admission,
            session_expires_at_ms: session,
        }))
    }
    pub(super) fn is_fragmented(&self) -> bool {
        matches!(self, Self::Fragmented(_) | Self::SessionFragmented(_))
    }
    pub(super) fn fragmented_contract(&self) -> Option<FragmentedUdpContract> {
        match self {
            Self::Fragmented(_) => Some(FragmentedUdpContract::default()),
            Self::SessionFragmented(_) => Some(FragmentedUdpContract::session()),
            _ => None,
        }
    }
    pub(super) fn validate_desktop(&self) -> Result<(), ConnectError> {
        if let Self::Fragmented(value) = self {
            fragmented_profile::validate_verified_invitation(value)
        } else {
            self.timing().map(|_| ())
        }
    }
    pub(super) fn fragmented_seconds(&self) -> Option<u16> {
        if let Self::Fragmented(value) = self {
            let issued = value.preview()["issuedAtUnixMs"].as_u64()?;
            Some(((value.expires_at_unix_ms() - issued) / 1000) as u16)
        } else {
            None
        }
    }
    pub(super) async fn start_host(
        &self,
        mode: ConnectMode,
        config: lab::HostConfig,
    ) -> lab::Result<lab::Host> {
        match (self, mode) {
            (Self::Legacy(v), ConnectMode::Loopback) => lab::Host::start_invited(config, v).await,
            (Self::Legacy(v), ConnectMode::ExternalIpv6) => {
                lab::Host::start_invited_external_ipv6(config, v).await
            }
            (Self::Fragmented(v), ConnectMode::Loopback) => {
                lab::Host::start_invited_fragmented(config, v).await
            }
            (Self::Fragmented(v), ConnectMode::ExternalIpv6) => {
                lab::Host::start_invited_fragmented_external_ipv6(config, v).await
            }
            (Self::Session(v), ConnectMode::Loopback) => {
                lab::Host::start_invited_session(config, v).await
            }
            (Self::Session(v), ConnectMode::ExternalIpv6) => {
                lab::Host::start_invited_session_external_ipv6(config, v).await
            }
            (Self::SessionFragmented(v), ConnectMode::Loopback) => {
                lab::Host::start_invited_fragmented_session(config, v).await
            }
            (Self::SessionFragmented(v), ConnectMode::ExternalIpv6) => {
                lab::Host::start_invited_fragmented_session_external_ipv6(config, v).await
            }
        }
    }
    pub(super) async fn connect_guest(
        &self,
        mode: ConnectMode,
        key: lab::SecretKey,
    ) -> lab::Result<lab::Guest> {
        match (self, mode) {
            (Self::Legacy(v), ConnectMode::Loopback) => {
                lab::Guest::connect_invited(key, v, lab::Limits::default()).await
            }
            (Self::Legacy(v), ConnectMode::ExternalIpv6) => {
                lab::Guest::connect_invited_external_ipv6(key, v, lab::Limits::default()).await
            }
            (Self::Fragmented(v), ConnectMode::Loopback) => {
                lab::Guest::connect_invited_fragmented(key, v, lab::Limits::default()).await
            }
            (Self::Fragmented(v), ConnectMode::ExternalIpv6) => {
                lab::Guest::connect_invited_fragmented_external_ipv6(key, v, lab::Limits::default())
                    .await
            }
            (Self::Session(v), ConnectMode::Loopback) => {
                lab::Guest::connect_invited_session(key, v, lab::Limits::default()).await
            }
            (Self::Session(v), ConnectMode::ExternalIpv6) => {
                lab::Guest::connect_invited_session_external_ipv6(key, v, lab::Limits::default())
                    .await
            }
            (Self::SessionFragmented(v), ConnectMode::Loopback) => {
                lab::Guest::connect_invited_fragmented_session(key, v, lab::Limits::default()).await
            }
            (Self::SessionFragmented(v), ConnectMode::ExternalIpv6) => {
                lab::Guest::connect_invited_fragmented_session_external_ipv6(
                    key,
                    v,
                    lab::Limits::default(),
                )
                .await
            }
        }
    }
}
