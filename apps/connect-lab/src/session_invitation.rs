//! Explicit schema-2 game/TCP and schema-3 game/fragmented-UDP invitations.
//! Admission and the admitted session have independent absolute deadlines.
//! Host owns admission, consumption and revocation; this value never renews.
use crate::{
    Capability, EndpointId, Error, HostConfig, Lease, NativeVoiceBudget, OnlineClock, Protocol,
    Result, SecretKey, Service, UdpReturnPolicy, generate_key,
    invitation::InvitationAddressMode as AddressMode, model::valid_id, unix_millis,
    wire::InvitationProof,
};
use iroh::Signature;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

pub const MAX_SESSION_INVITATION_BYTES: usize = 4096;
pub const MAX_ADMISSION_LIFETIME: Duration = Duration::from_secs(600);
pub const MAX_SESSION_LIFETIME: Duration = Duration::from_secs(3600);
const DOMAIN: &[u8] = b"aurora/connect-lab/session-invitation/v2\0";
const ONLINE_FRAGMENTED_DOMAIN: &[u8] = b"aurora/connect/session-invitation/v5\0";
const ONLINE_NATIVE_VOICE_DOMAIN: &[u8] = b"aurora/connect/session-invitation/v6\0";
const ONLINE_DOMAIN: &[u8] = b"aurora/connect/session-invitation/v4\0";
const FRAGMENTED_DOMAIN: &[u8] = b"aurora/connect-lab/session-invitation/v3\0";

/// Transport selection is separate from the independently approved address mode.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionInvitationProfile {
    GameTcp,
    OnlineGameTcp,
    OnlineFragmentedGameUdp,
    OnlineNativeVoice8,
    FragmentedGameUdp,
}
impl SessionInvitationProfile {
    fn online(self) -> bool {
        matches!(
            self,
            Self::OnlineGameTcp | Self::OnlineFragmentedGameUdp | Self::OnlineNativeVoice8
        )
    }
    fn fragmented(self) -> bool {
        matches!(
            self,
            Self::FragmentedGameUdp | Self::OnlineFragmentedGameUdp | Self::OnlineNativeVoice8
        )
    }
    fn schema(self) -> u8 {
        match self {
            Self::GameTcp => 2,
            Self::OnlineGameTcp => 4,
            Self::OnlineFragmentedGameUdp => 5,
            Self::OnlineNativeVoice8 => 6,
            Self::FragmentedGameUdp => 3,
        }
    }
    fn grants(self) -> Vec<Grant> {
        match self {
            Self::GameTcp | Self::OnlineGameTcp => vec![Grant::game()],
            Self::FragmentedGameUdp | Self::OnlineFragmentedGameUdp | Self::OnlineNativeVoice8 => {
                vec![Grant::game(), self.aux_grant()]
            }
        }
    }
    fn aux_grant(self) -> Grant {
        let mut grant = Grant::fragmented_aux();
        if self == Self::OnlineNativeVoice8 {
            grant.max_datagram_bytes = Some(NativeVoiceBudget::MAX_DATAGRAM_BYTES);
            grant.udp_return_policy = Some(NativeVoiceBudget::RETURN_POLICY.into());
        }
        grant
    }
    fn validate_grants(self, grants: &[Grant]) -> Result<()> {
        if grants.len() != self.grants().len() {
            return Err(Error::Configuration);
        }
        grants[0].validate()?;
        if self.fragmented() && !grants[1].matches(&self.aux_grant()) {
            return Err(Error::Configuration);
        }
        Ok(())
    }
}

/// Both timestamps are absolute. Neither is measured from connection time.
#[derive(Clone, Copy)]
pub struct SessionDeadlines {
    pub admission_expires_at_unix_ms: u64,
    pub session_expires_at_unix_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    body: Body,
    signature: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Body {
    schema_version: u8,
    host_key: String,
    guest_key: String,
    session_id: String,
    invitation_id: String,
    issued_at_unix_ms: u64,
    admission_expires_at_unix_ms: u64,
    session_expires_at_unix_ms: u64,
    generation: u64,
    direct_address: SocketAddr,
    services: Vec<Grant>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Grant {
    service_id: String,
    protocol: Protocol,
    revision: u32,
    required_capability: Capability,
    // Omitted in schema 2 and the TCP grant, preserving its original signed bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_datagram_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    udp_return_policy: Option<ReturnPolicy>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
enum ReturnPolicy {
    ReplyBudget,
    AuthorizedDownstream {
        #[serde(rename = "bytesPerSecond")]
        bytes_per_second: u32,
        #[serde(rename = "datagramsPerSecond")]
        datagrams_per_second: u32,
    },
}
impl From<UdpReturnPolicy> for ReturnPolicy {
    fn from(policy: UdpReturnPolicy) -> Self {
        match policy {
            UdpReturnPolicy::ReplyBudget => Self::ReplyBudget,
            UdpReturnPolicy::AuthorizedDownstream {
                bytes_per_second,
                datagrams_per_second,
            } => Self::AuthorizedDownstream {
                bytes_per_second,
                datagrams_per_second,
            },
        }
    }
}
impl Grant {
    fn game() -> Self {
        Self {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            revision: 1,
            required_capability: Capability::ReliableStream,
            max_datagram_bytes: None,
            udp_return_policy: None,
        }
    }
    fn fragmented_aux() -> Self {
        Self {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            revision: 1,
            required_capability: Capability::FragmentedDatagram,
            max_datagram_bytes: Some(4096),
            udp_return_policy: Some(ReturnPolicy::AuthorizedDownstream {
                bytes_per_second: 16384,
                datagrams_per_second: 20,
            }),
        }
    }
    fn matches(&self, other: &Self) -> bool {
        self.service_id == other.service_id
            && self.protocol == other.protocol
            && self.revision == other.revision
            && self.required_capability == other.required_capability
            && self.max_datagram_bytes == other.max_datagram_bytes
            && self.udp_return_policy == other.udp_return_policy
    }
    fn validate(&self) -> Result<()> {
        if self.service_id != "game"
            || self.protocol != Protocol::Tcp
            || self.revision != 1
            || self.required_capability != Capability::ReliableStream
            || self.max_datagram_bytes.is_some()
            || self.udp_return_policy.is_some()
        {
            return Err(Error::Configuration);
        }
        Ok(())
    }
}

fn validate_local_services(services: &[Service]) -> Result<()> {
    if services.len() != 1 {
        return Err(Error::Configuration);
    }
    let service = &services[0];
    Grant {
        service_id: service.service_id.clone(),
        protocol: service.protocol,
        revision: service.revision,
        required_capability: service.required_capability,
        max_datagram_bytes: None,
        udp_return_policy: None,
    }
    .validate()?;
    AddressMode::Loopback.validate(service.endpoint)?;
    if service.udp_return_policy != UdpReturnPolicy::ReplyBudget
        || !(1..=65507).contains(&service.max_datagram_bytes)
    {
        return Err(Error::Configuration);
    }
    Ok(())
}

fn validate_profile_services(
    services: &[Service],
    profile: SessionInvitationProfile,
) -> Result<()> {
    match profile {
        SessionInvitationProfile::GameTcp | SessionInvitationProfile::OnlineGameTcp => {
            validate_local_services(services)
        }
        SessionInvitationProfile::FragmentedGameUdp
        | SessionInvitationProfile::OnlineFragmentedGameUdp
        | SessionInvitationProfile::OnlineNativeVoice8 => {
            if services.len() != 2 || services[0].max_datagram_bytes != 1400 {
                return Err(Error::Configuration);
            }
            validate_local_services(&services[..1])?;
            let aux = &services[1];
            AddressMode::Loopback.validate(aux.endpoint)?;
            let grant = Grant {
                service_id: aux.service_id.clone(),
                protocol: aux.protocol,
                revision: aux.revision,
                required_capability: aux.required_capability,
                max_datagram_bytes: Some(aux.max_datagram_bytes),
                udp_return_policy: Some(aux.udp_return_policy.into()),
            };
            if !grant.matches(&profile.aux_grant()) {
                return Err(Error::Configuration);
            }
            Ok(())
        }
    }
}

#[derive(Clone, Copy)]
struct Clock {
    unix_ms: u64,
    monotonic: Instant,
}
impl Clock {
    fn now() -> Self {
        // Capture before verification/signing work so that work cannot extend TTL.
        let monotonic = Instant::now();
        Self {
            unix_ms: unix_millis(),
            monotonic,
        }
    }
}

/// Immutable public data. Clone preserves both original monotonic deadlines.
/// The lease projection is a configuration value, not authorization by itself.
/// Host integration must retain this value's original temporal authority and
/// also check TLS identity, one-use admission, generation and revocation.
#[derive(Clone)]
pub struct SessionInvitation {
    profile: SessionInvitationProfile,
    envelope: Envelope,
    encoded: String,
    digest: String,
    host: EndpointId,
    guest: EndpointId,
    admission_deadline: Instant,
    session_deadline: Instant,
    authority_clock: Option<OnlineClock>,
    #[cfg(test)]
    wall_override: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>,
}
enum IssueTiming {
    Absolute(SessionDeadlines),
    Lifetimes {
        admission: Duration,
        session: Duration,
    },
}
struct IssueOptions {
    timing: IssueTiming,
    mode: AddressMode,
    profile: SessionInvitationProfile,
}
#[derive(Clone, Copy)]
struct VerificationOptions {
    clock: Clock,
    mode: AddressMode,
    profile: SessionInvitationProfile,
}
impl SessionInvitation {
    pub fn issue_online_with_clock(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        services: &[Service],
        deadlines: SessionDeadlines,
        fragmented: bool,
        clock: &OnlineClock,
    ) -> Result<Self> {
        let profile = if fragmented {
            SessionInvitationProfile::OnlineFragmentedGameUdp
        } else {
            SessionInvitationProfile::OnlineGameTcp
        };
        Self::issue_online_in_profile(
            host,
            guest,
            session_id,
            services,
            deadlines,
            profile,
            Some(clock),
        )
    }

    fn issue_online_in_profile(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        services: &[Service],
        deadlines: SessionDeadlines,
        profile: SessionInvitationProfile,
        clock: Option<&OnlineClock>,
    ) -> Result<Self> {
        if clock.is_some_and(|clock| !clock.session_matches(session_id)) {
            return Err(Error::Unauthorized);
        }
        Self::issue_using_clock(
            host,
            guest,
            session_id,
            "0.0.0.0:0".parse().expect("route sentinel"),
            services,
            IssueOptions {
                timing: IssueTiming::Absolute(deadlines),
                mode: AddressMode::Loopback,
                profile,
            },
            clock,
        )
    }

    pub fn verify_online_with_clock(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
        fragmented: bool,
        clock: &OnlineClock,
    ) -> Result<Self> {
        let profile = if fragmented {
            SessionInvitationProfile::OnlineFragmentedGameUdp
        } else {
            SessionInvitationProfile::OnlineGameTcp
        };
        Self::verify_online_in_profile(encoded, expected_host, guest, profile, Some(clock))
    }

    fn verify_online_in_profile(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
        profile: SessionInvitationProfile,
        clock: Option<&OnlineClock>,
    ) -> Result<Self> {
        let (monotonic, unix_ms) = OnlineClock::observe(clock);
        let invitation = Self::verify_in_profile(
            encoded,
            expected_host,
            guest,
            VerificationOptions {
                clock: Clock { monotonic, unix_ms },
                mode: AddressMode::Loopback,
                profile,
            },
        )?;
        match clock {
            Some(clock) => invitation.bind_clock(clock),
            None => Ok(invitation),
        }
    }

    fn bind_clock(mut self, clock: &OnlineClock) -> Result<Self> {
        if !clock.session_matches(self.session_id()) {
            return Err(Error::Unauthorized);
        }
        self.admission_deadline = self
            .admission_deadline
            .min(clock.deadline_for(self.admission_expires_at_unix_ms())?);
        self.session_deadline = self
            .session_deadline
            .min(clock.deadline_for(self.session_expires_at_unix_ms())?);
        self.authority_clock = Some(clock.clone());
        self.check_admission_current()?;
        Ok(self)
    }

    pub(crate) fn online_clock(&self) -> Option<&OnlineClock> {
        self.authority_clock.as_ref()
    }

    /// Online routing comes from an independently authenticated native ticket.
    /// The sentinel is never interpreted as a network destination.
    pub fn issue_online(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        services: &[Service],
        deadlines: SessionDeadlines,
    ) -> Result<Self> {
        Self::issue_in_profile(
            host,
            guest,
            session_id,
            "0.0.0.0:0".parse().expect("route sentinel"),
            services,
            IssueOptions {
                timing: IssueTiming::Absolute(deadlines),
                mode: AddressMode::Loopback,
                profile: SessionInvitationProfile::OnlineGameTcp,
            },
        )
    }
    pub fn verify_online(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
    ) -> Result<Self> {
        Self::verify_in_profile(
            encoded,
            expected_host,
            guest,
            VerificationOptions {
                clock: Clock::now(),
                mode: AddressMode::Loopback,
                profile: SessionInvitationProfile::OnlineGameTcp,
            },
        )
    }
    pub(crate) fn check_host_online(&self, config: &HostConfig) -> Result<()> {
        self.check_host_profile_at(
            config,
            VerificationOptions {
                clock: self.current_clock(),
                mode: AddressMode::Loopback,
                profile: SessionInvitationProfile::OnlineGameTcp,
            },
        )
    }
    /// Schema5 approves the same closed fragmented aux profile, while routes
    /// still require a separately verified online ticket. Schema4 stays TCP-only.
    pub fn issue_online_fragmented(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        services: &[Service],
        deadlines: SessionDeadlines,
    ) -> Result<Self> {
        Self::issue_in_profile(
            host,
            guest,
            session_id,
            "0.0.0.0:0".parse().expect("route sentinel"),
            services,
            IssueOptions {
                timing: IssueTiming::Absolute(deadlines),
                mode: AddressMode::Loopback,
                profile: SessionInvitationProfile::OnlineFragmentedGameUdp,
            },
        )
    }
    pub fn verify_online_fragmented(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
    ) -> Result<Self> {
        Self::verify_in_profile(
            encoded,
            expected_host,
            guest,
            VerificationOptions {
                clock: Clock::now(),
                mode: AddressMode::Loopback,
                profile: SessionInvitationProfile::OnlineFragmentedGameUdp,
            },
        )
    }
    pub(crate) fn check_host_online_fragmented(&self, config: &HostConfig) -> Result<()> {
        self.check_host_profile_at(
            config,
            VerificationOptions {
                clock: self.current_clock(),
                mode: AddressMode::Loopback,
                profile: SessionInvitationProfile::OnlineFragmentedGameUdp,
            },
        )
    }
    pub fn issue_online_native_voice(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        services: &[Service],
        deadlines: SessionDeadlines,
    ) -> Result<Self> {
        Self::issue_online_in_profile(
            host,
            guest,
            session_id,
            services,
            deadlines,
            SessionInvitationProfile::OnlineNativeVoice8,
            None,
        )
    }

    pub fn issue_online_native_voice_with_clock(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        services: &[Service],
        deadlines: SessionDeadlines,
        clock: &OnlineClock,
    ) -> Result<Self> {
        Self::issue_online_in_profile(
            host,
            guest,
            session_id,
            services,
            deadlines,
            SessionInvitationProfile::OnlineNativeVoice8,
            Some(clock),
        )
    }

    pub fn verify_online_native_voice(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
    ) -> Result<Self> {
        Self::verify_online_in_profile(
            encoded,
            expected_host,
            guest,
            SessionInvitationProfile::OnlineNativeVoice8,
            None,
        )
    }

    pub fn verify_online_native_voice_with_clock(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
        clock: &OnlineClock,
    ) -> Result<Self> {
        Self::verify_online_in_profile(
            encoded,
            expected_host,
            guest,
            SessionInvitationProfile::OnlineNativeVoice8,
            Some(clock),
        )
    }

    pub fn is_online_native_voice(&self) -> bool {
        self.profile == SessionInvitationProfile::OnlineNativeVoice8
    }

    pub(crate) fn check_host_online_native_voice(&self, config: &HostConfig) -> Result<()> {
        NativeVoiceBudget::check_limits(&config.limits)?;
        self.check_host_profile_at(
            config,
            VerificationOptions {
                clock: self.current_clock(),
                mode: AddressMode::Loopback,
                profile: SessionInvitationProfile::OnlineNativeVoice8,
            },
        )
    }

    pub(crate) fn check_online_local_player(&self) -> Result<()> {
        match self.profile {
            SessionInvitationProfile::OnlineFragmentedGameUdp
            | SessionInvitationProfile::OnlineNativeVoice8 => self.check_profile(self.profile),
            _ => Err(Error::Configuration),
        }
    }
    pub(crate) fn check_same_online_profile(&self, other: &Self) -> Result<()> {
        if !other.profile.online()
            || !OnlineClock::same_scope(self.online_clock(), other.online_clock())
        {
            return Err(Error::Configuration);
        }
        self.check_profile(other.profile)
    }
    pub fn issue_loopback(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        deadlines: SessionDeadlines,
    ) -> Result<Self> {
        Self::issue_in_mode(
            host,
            guest,
            session_id,
            direct,
            services,
            IssueTiming::Absolute(deadlines),
            AddressMode::Loopback,
        )
    }

    pub fn issue_external_ipv6(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        deadlines: SessionDeadlines,
    ) -> Result<Self> {
        Self::issue_in_mode(
            host,
            guest,
            session_id,
            direct,
            services,
            IssueTiming::Absolute(deadlines),
            AddressMode::ExternalIpv6,
        )
    }

    /// Explicit direct IPv4 opt-in; preserves this profile's grants and original
    /// deadlines. No relay, address discovery, port mapping or route guarantee.
    pub fn issue_direct_ipv4(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        deadlines: SessionDeadlines,
    ) -> Result<Self> {
        Self::issue_in_mode(
            host,
            guest,
            session_id,
            direct,
            services,
            IssueTiming::Absolute(deadlines),
            AddressMode::DirectIpv4,
        )
    }

    /// Compute both signed expiries from the same clock as issuedAt. Durations
    /// are exact milliseconds, bounded by the existing admission/session rules.
    pub fn issue_loopback_for_lifetimes(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        admission: Duration,
        session: Duration,
    ) -> Result<Self> {
        Self::issue_in_mode(
            host,
            guest,
            session_id,
            direct,
            services,
            IssueTiming::Lifetimes { admission, session },
            AddressMode::Loopback,
        )
    }
    /// External IPv6 remains an explicit, separate address-policy choice.
    pub fn issue_external_ipv6_for_lifetimes(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        admission: Duration,
        session: Duration,
    ) -> Result<Self> {
        Self::issue_in_mode(
            host,
            guest,
            session_id,
            direct,
            services,
            IssueTiming::Lifetimes { admission, session },
            AddressMode::ExternalIpv6,
        )
    }

    /// Explicit direct IPv4 opt-in; preserves this profile's grants and original
    /// deadlines. No relay, address discovery, port mapping or route guarantee.
    pub fn issue_direct_ipv4_for_lifetimes(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        admission: Duration,
        session: Duration,
    ) -> Result<Self> {
        Self::issue_in_mode(
            host,
            guest,
            session_id,
            direct,
            services,
            IssueTiming::Lifetimes { admission, session },
            AddressMode::DirectIpv4,
        )
    }

    /// Explicit schema-3 TCP/fragmented-UDP profile. Both signed lifetimes are
    /// measured from one original clock sample, never from connection activity.
    pub fn issue_fragmented_loopback_for_lifetimes(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        admission: Duration,
        session: Duration,
    ) -> Result<Self> {
        Self::issue_in_profile(
            host,
            guest,
            session_id,
            direct,
            services,
            IssueOptions {
                timing: IssueTiming::Lifetimes { admission, session },
                mode: AddressMode::Loopback,
                profile: SessionInvitationProfile::FragmentedGameUdp,
            },
        )
    }

    /// The explicit UDP profile does not implicitly approve an external address.
    pub fn issue_fragmented_external_ipv6_for_lifetimes(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        admission: Duration,
        session: Duration,
    ) -> Result<Self> {
        Self::issue_in_profile(
            host,
            guest,
            session_id,
            direct,
            services,
            IssueOptions {
                timing: IssueTiming::Lifetimes { admission, session },
                mode: AddressMode::ExternalIpv6,
                profile: SessionInvitationProfile::FragmentedGameUdp,
            },
        )
    }

    /// Explicit direct IPv4 opt-in; preserves this profile's grants and original
    /// deadlines. No relay, address discovery, port mapping or route guarantee.
    pub fn issue_fragmented_direct_ipv4_for_lifetimes(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        admission: Duration,
        session: Duration,
    ) -> Result<Self> {
        Self::issue_in_profile(
            host,
            guest,
            session_id,
            direct,
            services,
            IssueOptions {
                timing: IssueTiming::Lifetimes { admission, session },
                mode: AddressMode::DirectIpv4,
                profile: SessionInvitationProfile::FragmentedGameUdp,
            },
        )
    }

    fn issue_in_mode(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        timing: IssueTiming,
        mode: AddressMode,
    ) -> Result<Self> {
        Self::issue_in_profile(
            host,
            guest,
            session_id,
            direct,
            services,
            IssueOptions {
                timing,
                mode,
                profile: SessionInvitationProfile::GameTcp,
            },
        )
    }

    fn issue_in_profile(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        options: IssueOptions,
    ) -> Result<Self> {
        Self::issue_using_clock(host, guest, session_id, direct, services, options, None)
    }

    fn issue_using_clock(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        options: IssueOptions,
        authority_clock: Option<&OnlineClock>,
    ) -> Result<Self> {
        let clock = match authority_clock {
            Some(authority) => {
                authority.check_current()?;
                Clock {
                    monotonic: Instant::now(),
                    unix_ms: authority.lower_now(),
                }
            }
            None => Clock::now(),
        };
        let deadlines = match options.timing {
            IssueTiming::Absolute(value) => value,
            IssueTiming::Lifetimes { admission, session } => {
                if admission.is_zero()
                    || admission > MAX_ADMISSION_LIFETIME
                    || session < admission
                    || session > MAX_SESSION_LIFETIME
                    || admission.subsec_nanos() % 1_000_000 != 0
                    || session.subsec_nanos() % 1_000_000 != 0
                {
                    return Err(Error::Configuration);
                }
                SessionDeadlines {
                    admission_expires_at_unix_ms: clock
                        .unix_ms
                        .checked_add(admission.as_millis() as u64)
                        .ok_or(Error::Configuration)?,
                    session_expires_at_unix_ms: clock
                        .unix_ms
                        .checked_add(session.as_millis() as u64)
                        .ok_or(Error::Configuration)?,
                }
            }
        };
        validate_profile_services(services, options.profile)?;
        let body = Body {
            schema_version: options.profile.schema(),
            host_key: host.public().to_string(),
            guest_key: guest.to_string(),
            session_id: session_id.into(),
            invitation_id: generate_key().public().to_string(),
            issued_at_unix_ms: clock.unix_ms,
            admission_expires_at_unix_ms: deadlines.admission_expires_at_unix_ms,
            session_expires_at_unix_ms: deadlines.session_expires_at_unix_ms,
            generation: 1,
            direct_address: direct,
            services: options.profile.grants(),
        };
        let verification = VerificationOptions {
            clock,
            mode: options.mode,
            profile: options.profile,
        };
        validate_body(&body, host.public(), guest, verification)?;
        let signature = hex(&host.sign(&signed_bytes(&body)?).to_bytes());
        let value = Self::from_envelope(
            Envelope { body, signature },
            host.public(),
            guest,
            verification,
        )?;
        match authority_clock {
            Some(clock) => value.bind_clock(clock),
            None => {
                value.check_admission_current()?;
                Ok(value)
            }
        }
    }

    /// expected_host must come from an independently approved public-key pin.
    /// Verification is for NEW admission and rejects an elapsed admission TTL,
    /// even when the signed session deadline has not elapsed yet.
    pub fn verify_loopback(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
    ) -> Result<Self> {
        let value = Self::verify_in_mode(
            encoded,
            expected_host,
            guest,
            Clock::now(),
            AddressMode::Loopback,
        )?;
        value.check_admission_current()?;
        Ok(value)
    }

    pub fn verify_external_ipv6(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
    ) -> Result<Self> {
        let value = Self::verify_in_mode(
            encoded,
            expected_host,
            guest,
            Clock::now(),
            AddressMode::ExternalIpv6,
        )?;
        value.check_admission_current()?;
        Ok(value)
    }

    /// Explicit direct IPv4 opt-in; preserves this profile's grants and original
    /// deadlines. No relay, address discovery, port mapping or route guarantee.
    pub fn verify_direct_ipv4(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
    ) -> Result<Self> {
        let value = Self::verify_in_mode(
            encoded,
            expected_host,
            guest,
            Clock::now(),
            AddressMode::DirectIpv4,
        )?;
        value.check_admission_current()?;
        Ok(value)
    }

    pub fn verify_fragmented_loopback(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
    ) -> Result<Self> {
        let value = Self::verify_in_profile(
            encoded,
            expected_host,
            guest,
            VerificationOptions {
                clock: Clock::now(),
                mode: AddressMode::Loopback,
                profile: SessionInvitationProfile::FragmentedGameUdp,
            },
        )?;
        value.check_admission_current()?;
        Ok(value)
    }

    pub fn verify_fragmented_external_ipv6(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
    ) -> Result<Self> {
        let value = Self::verify_in_profile(
            encoded,
            expected_host,
            guest,
            VerificationOptions {
                clock: Clock::now(),
                mode: AddressMode::ExternalIpv6,
                profile: SessionInvitationProfile::FragmentedGameUdp,
            },
        )?;
        value.check_admission_current()?;
        Ok(value)
    }

    /// Explicit direct IPv4 opt-in; preserves this profile's grants and original
    /// deadlines. No relay, address discovery, port mapping or route guarantee.
    pub fn verify_fragmented_direct_ipv4(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
    ) -> Result<Self> {
        let value = Self::verify_in_profile(
            encoded,
            expected_host,
            guest,
            VerificationOptions {
                clock: Clock::now(),
                mode: AddressMode::DirectIpv4,
                profile: SessionInvitationProfile::FragmentedGameUdp,
            },
        )?;
        value.check_admission_current()?;
        Ok(value)
    }

    fn verify_in_mode(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
        clock: Clock,
        mode: AddressMode,
    ) -> Result<Self> {
        Self::verify_in_profile(
            encoded,
            expected_host,
            guest,
            VerificationOptions {
                clock,
                mode,
                profile: SessionInvitationProfile::GameTcp,
            },
        )
    }
    fn verify_in_profile(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
        options: VerificationOptions,
    ) -> Result<Self> {
        if encoded.is_empty() || encoded.len() > MAX_SESSION_INVITATION_BYTES {
            return Err(Error::Limit);
        }
        let envelope: Envelope = serde_json::from_str(encoded).map_err(|_| Error::Protocol)?;
        if encode(&envelope)? != encoded {
            return Err(Error::Protocol);
        }
        Self::from_envelope(envelope, expected_host, guest, options)
    }

    fn from_envelope(
        envelope: Envelope,
        expected_host: EndpointId,
        guest: EndpointId,
        options: VerificationOptions,
    ) -> Result<Self> {
        validate_body(&envelope.body, expected_host, guest, options)?;
        let clock = options.clock;
        let signature = Signature::from_bytes(&decode_hex::<64>(&envelope.signature)?);
        expected_host
            .verify(&signed_bytes(&envelope.body)?, &signature)
            .map_err(|_| Error::Unauthorized)?;
        let encoded = encode(&envelope)?;
        let digest = hex(&Sha256::digest(encoded.as_bytes()));
        let deadline = |unix_ms| {
            clock
                .monotonic
                .checked_add(Duration::from_millis(unix_ms - clock.unix_ms))
                .ok_or(Error::Expired)
        };
        Ok(Self {
            profile: options.profile,
            admission_deadline: deadline(envelope.body.admission_expires_at_unix_ms)?,
            session_deadline: deadline(envelope.body.session_expires_at_unix_ms)?,
            envelope,
            encoded,
            digest,
            host: expected_host,
            guest,
            authority_clock: None,
            #[cfg(test)]
            wall_override: None,
        })
    }

    pub fn check_admission_current(&self) -> Result<()> {
        self.check_admission_at(self.current_clock())
    }
    pub fn check_session_current(&self) -> Result<()> {
        self.check_session_at(self.current_clock())
    }
    /// Remaining NEW-admission time, bounded by both original clocks.
    /// This temporal check does not imply possession, consumption or authorization.
    pub fn admission_remaining(&self) -> Result<Duration> {
        self.admission_remaining_at(self.current_clock())
    }
    /// Remaining admitted-session time. Passing admission expiry never renews it.
    /// A caller must separately keep and check its original admission/revocation state.
    pub fn session_remaining(&self) -> Result<Duration> {
        self.session_remaining_at(self.current_clock())
    }
    fn current_clock(&self) -> Clock {
        let (monotonic, unix_ms) = OnlineClock::observe(self.authority_clock.as_ref());
        let clock = Clock { monotonic, unix_ms };
        #[cfg(test)]
        if let Some(wall) = &self.wall_override {
            return Clock {
                unix_ms: wall.load(std::sync::atomic::Ordering::SeqCst),
                ..clock
            };
        }
        clock
    }
    #[cfg(test)]
    pub(crate) fn with_test_wall_clock(
        mut self,
        wall: std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        self.wall_override = Some(wall);
        self
    }
    /// Checks the signed civil clock even without application activity. The
    /// shorter original monotonic deadline is always an upper bound; polling
    /// observes civil jumps within 100ms plus executor scheduling latency.
    pub(crate) async fn wait_session_expired(&self) {
        wait_until_expired(|| self.session_remaining()).await;
    }
    fn admission_remaining_at(&self, clock: Clock) -> Result<Duration> {
        self.check_admission_at(clock)?;
        Ok(
            Duration::from_millis(self.admission_expires_at_unix_ms() - clock.unix_ms).min(
                self.admission_deadline
                    .saturating_duration_since(clock.monotonic),
            ),
        )
    }
    fn session_remaining_at(&self, clock: Clock) -> Result<Duration> {
        self.check_session_at(clock)?;
        Ok(
            Duration::from_millis(self.session_expires_at_unix_ms() - clock.unix_ms).min(
                self.session_deadline
                    .saturating_duration_since(clock.monotonic),
            ),
        )
    }
    fn check_admission_at(&self, clock: Clock) -> Result<()> {
        self.check_session_at(clock)?;
        if clock.unix_ms >= self.admission_expires_at_unix_ms()
            || clock.monotonic >= self.admission_deadline
        {
            return Err(Error::Expired);
        }
        Ok(())
    }
    fn check_session_at(&self, clock: Clock) -> Result<()> {
        if clock.unix_ms < self.envelope.body.issued_at_unix_ms
            || clock.unix_ms >= self.session_expires_at_unix_ms()
            || clock.monotonic >= self.session_deadline
        {
            return Err(Error::Expired);
        }
        Ok(())
    }
    pub fn encode(&self) -> String {
        self.encoded.clone()
    }
    pub fn digest(&self) -> String {
        self.digest.clone()
    }
    pub fn host_key(&self) -> EndpointId {
        self.host
    }
    pub fn guest_key(&self) -> EndpointId {
        self.guest
    }
    pub fn session_id(&self) -> &str {
        &self.envelope.body.session_id
    }
    pub fn invitation_id(&self) -> &str {
        &self.envelope.body.invitation_id
    }
    pub fn generation(&self) -> u64 {
        self.envelope.body.generation
    }
    pub fn direct_addr(&self) -> SocketAddr {
        self.envelope.body.direct_address
    }
    pub fn admission_expires_at_unix_ms(&self) -> u64 {
        self.envelope.body.admission_expires_at_unix_ms
    }
    pub fn session_expires_at_unix_ms(&self) -> u64 {
        self.envelope.body.session_expires_at_unix_ms
    }
    /// Local configuration only. Host must retain the original invitation for
    /// the separate admission deadline, monotonic clocks and one-use decision.
    pub fn lease(&self) -> Lease {
        Lease {
            lease_id: self.invitation_id().into(),
            peer: self.guest,
            expires_at_unix_ms: self.session_expires_at_unix_ms(),
            services: self
                .envelope
                .body
                .services
                .iter()
                .map(|grant| grant.service_id.clone())
                .collect(),
        }
    }
    pub(crate) fn capabilities(&self) -> Vec<Capability> {
        match self.profile {
            SessionInvitationProfile::GameTcp | SessionInvitationProfile::OnlineGameTcp => {
                vec![Capability::ReliableStream]
            }
            SessionInvitationProfile::FragmentedGameUdp
            | SessionInvitationProfile::OnlineFragmentedGameUdp
            | SessionInvitationProfile::OnlineNativeVoice8 => {
                vec![Capability::ReliableStream, Capability::FragmentedDatagram]
            }
        }
    }
    pub(crate) fn proof(&self) -> InvitationProof {
        InvitationProof {
            digest: self.digest.clone(),
            generation: self.generation(),
        }
    }
    pub(crate) fn check_address_mode(&self, mode: AddressMode) -> Result<()> {
        mode.validate(self.direct_addr())
    }
    pub(crate) fn check_profile(&self, profile: SessionInvitationProfile) -> Result<()> {
        if self.profile != profile || self.envelope.body.schema_version != profile.schema() {
            return Err(Error::Configuration);
        }
        profile.validate_grants(&self.envelope.body.services)
    }
    pub(crate) fn check_host(&self, config: &HostConfig) -> Result<()> {
        self.check_host_at(config, AddressMode::Loopback, Clock::now())
    }
    pub(crate) fn check_host_external_ipv6(&self, config: &HostConfig) -> Result<()> {
        self.check_host_at(config, AddressMode::ExternalIpv6, Clock::now())
    }

    /// Explicit direct IPv4 opt-in; preserves this profile's grants and original
    /// deadlines. No relay, address discovery, port mapping or route guarantee.
    pub(crate) fn check_host_direct_ipv4(&self, config: &HostConfig) -> Result<()> {
        self.check_host_at(config, AddressMode::DirectIpv4, Clock::now())
    }
    pub(crate) fn check_host_fragmented(&self, config: &HostConfig) -> Result<()> {
        self.check_host_profile_at(
            config,
            VerificationOptions {
                clock: Clock::now(),
                mode: AddressMode::Loopback,
                profile: SessionInvitationProfile::FragmentedGameUdp,
            },
        )
    }
    pub(crate) fn check_host_fragmented_external_ipv6(&self, config: &HostConfig) -> Result<()> {
        self.check_host_profile_at(
            config,
            VerificationOptions {
                clock: Clock::now(),
                mode: AddressMode::ExternalIpv6,
                profile: SessionInvitationProfile::FragmentedGameUdp,
            },
        )
    }

    /// Explicit direct IPv4 opt-in; preserves this profile's grants and original
    /// deadlines. No relay, address discovery, port mapping or route guarantee.
    pub(crate) fn check_host_fragmented_direct_ipv4(&self, config: &HostConfig) -> Result<()> {
        self.check_host_profile_at(
            config,
            VerificationOptions {
                clock: Clock::now(),
                mode: AddressMode::DirectIpv4,
                profile: SessionInvitationProfile::FragmentedGameUdp,
            },
        )
    }
    fn check_host_at(&self, config: &HostConfig, mode: AddressMode, clock: Clock) -> Result<()> {
        self.check_host_profile_at(
            config,
            VerificationOptions {
                clock,
                mode,
                profile: SessionInvitationProfile::GameTcp,
            },
        )
    }
    fn check_host_profile_at(
        &self,
        config: &HostConfig,
        options: VerificationOptions,
    ) -> Result<()> {
        self.check_profile(options.profile)?;
        self.check_admission_at(options.clock)?;
        if !options.profile.online() {
            self.check_address_mode(options.mode)?;
        } else if !config.bind_addr.ip().is_unspecified() || config.bind_addr.port() != 0 {
            return Err(Error::Configuration);
        }
        validate_profile_services(&config.services, options.profile)?;
        let expected = self.lease();
        if config.secret_key.public() != self.host
            || config.bind_addr != self.direct_addr()
            || config.session_id != self.session_id()
            || config.leases.len() != 1
        {
            return Err(Error::Configuration);
        }
        let lease = &config.leases[0];
        if lease.lease_id != expected.lease_id
            || lease.peer != expected.peer
            || lease.expires_at_unix_ms != expected.expires_at_unix_ms
            || lease.services != expected.services
        {
            return Err(Error::Configuration);
        }
        Ok(())
    }
    /// Explicit approval material, not a redacted telemetry event.
    pub fn preview(&self) -> Value {
        let profile = match self.profile {
            SessionInvitationProfile::GameTcp => "sessionGameTcp",
            SessionInvitationProfile::OnlineGameTcp => "onlineGameTcp",
            SessionInvitationProfile::OnlineFragmentedGameUdp => "onlineFragmentedGameUdp",
            SessionInvitationProfile::OnlineNativeVoice8 => "onlineNativeVoice8",
            SessionInvitationProfile::FragmentedGameUdp => "sessionFragmentedGameUdp",
        };
        let mut preview = json!({"schemaVersion":self.profile.schema(),"experimental":true,"profile":profile,
            "invitationSha256":self.digest,"hostKey":self.host.to_string(),"guestKey":self.guest.to_string(),
            "sessionId":self.session_id(),"invitationId":self.invitation_id(),"generation":self.generation(),
            "issuedAtUnixMs":self.envelope.body.issued_at_unix_ms,
            "admissionExpiresAtUnixMs":self.admission_expires_at_unix_ms(),
            "sessionExpiresAtUnixMs":self.session_expires_at_unix_ms(),
            "directAddress":self.direct_addr().to_string(),"services":self.envelope.body.services,
            "admissionSingleUse":true,"renewal":false,"relay":false,"accountIdentityVerified":false});
        if self.profile.fragmented() {
            preview["udpEnvelopeVersion"] = json!(2);
            preview["maxFragmentFrameBytes"] = json!(1264);
        }
        preview
    }
}

async fn wait_until_expired(remaining: impl Fn() -> Result<Duration>) {
    while let Ok(delay) = remaining() {
        tokio::time::sleep(delay.min(Duration::from_millis(100))).await;
    }
}

fn validate_body(
    body: &Body,
    expected_host: EndpointId,
    guest: EndpointId,
    options: VerificationOptions,
) -> Result<()> {
    let now = options.clock.unix_ms;
    let host_key =
        EndpointId::from_bytes(&decode_hex::<32>(&body.host_key)?).map_err(|_| Error::Protocol)?;
    let guest_key =
        EndpointId::from_bytes(&decode_hex::<32>(&body.guest_key)?).map_err(|_| Error::Protocol)?;
    let nonce = decode_hex::<32>(&body.invitation_id)?;
    if host_key != expected_host || guest_key != guest || host_key == guest_key {
        return Err(Error::Unauthorized);
    }
    if body.schema_version != options.profile.schema()
        || body.generation != 1
        || !valid_id(&body.session_id)
        || nonce == [0; 32]
    {
        return Err(Error::Configuration);
    }
    if options.profile.online() {
        if body.direct_address != "0.0.0.0:0".parse::<SocketAddr>().expect("route sentinel") {
            return Err(Error::Configuration);
        }
    } else {
        options.mode.validate(body.direct_address)?;
    }
    options.profile.validate_grants(&body.services)?;
    let admission_ttl = body
        .admission_expires_at_unix_ms
        .checked_sub(body.issued_at_unix_ms)
        .ok_or(Error::Expired)?;
    let session_ttl = body
        .session_expires_at_unix_ms
        .checked_sub(body.issued_at_unix_ms)
        .ok_or(Error::Expired)?;
    if admission_ttl == 0
        || admission_ttl
            > if options.profile.online() {
                60_000
            } else {
                MAX_ADMISSION_LIFETIME.as_millis() as u64
            }
        || session_ttl == 0
        || session_ttl
            > if options.profile.online() {
                14_400_000
            } else {
                MAX_SESSION_LIFETIME.as_millis() as u64
            }
        || body.admission_expires_at_unix_ms > body.session_expires_at_unix_ms
        || body.issued_at_unix_ms > now
        || body.admission_expires_at_unix_ms <= now
        || body.session_expires_at_unix_ms <= now
    {
        return Err(Error::Expired);
    }
    Ok(())
}
fn signed_bytes(body: &Body) -> Result<Vec<u8>> {
    let encoded = serde_json::to_vec(body).map_err(|_| Error::Protocol)?;
    // Every public issue/verify path first checks its explicitly selected schema.
    let domain = if body.schema_version == 6 {
        ONLINE_NATIVE_VOICE_DOMAIN
    } else if body.schema_version == 5 {
        ONLINE_FRAGMENTED_DOMAIN
    } else if body.schema_version == 4 {
        ONLINE_DOMAIN
    } else if body.schema_version == 3 {
        FRAGMENTED_DOMAIN
    } else {
        DOMAIN
    };
    let mut bytes = Vec::with_capacity(domain.len() + encoded.len());
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(&encoded);
    Ok(bytes)
}
fn encode(envelope: &Envelope) -> Result<String> {
    let encoded = serde_json::to_string(envelope).map_err(|_| Error::Protocol)?;
    if encoded.len() > MAX_SESSION_INVITATION_BYTES {
        return Err(Error::Limit);
    }
    Ok(encoded)
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(DIGITS[(byte >> 4) as usize] as char);
        result.push(DIGITS[(byte & 15) as usize] as char);
    }
    result
}
fn decode_hex<const N: usize>(text: &str) -> Result<[u8; N]> {
    if text.len() != N * 2
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Protocol);
    }
    let mut bytes = [0u8; N];
    for (index, pair) in text.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let nibble = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
        bytes[index] = (nibble(pair[0]) << 4) | nibble(pair[1]);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        host: SecretKey,
        guest: SecretKey,
        envelope: Envelope,
        clock: Clock,
    }
    impl Fixture {
        fn new() -> Self {
            let host = generate_key();
            let guest = generate_key();
            let clock = Clock {
                unix_ms: 1_000_000,
                monotonic: Instant::now(),
            };
            let body = Body {
                schema_version: 2,
                host_key: host.public().to_string(),
                guest_key: guest.public().to_string(),
                session_id: "session_test".into(),
                invitation_id: generate_key().public().to_string(),
                issued_at_unix_ms: clock.unix_ms,
                admission_expires_at_unix_ms: clock.unix_ms + 2_000,
                session_expires_at_unix_ms: clock.unix_ms + 12_000,
                generation: 1,
                direct_address: "127.0.0.1:32000".parse().unwrap(),
                services: vec![Grant::game()],
            };
            let signature = hex(&host.sign(&signed_bytes(&body).unwrap()).to_bytes());
            Self {
                host,
                guest,
                envelope: Envelope { body, signature },
                clock,
            }
        }
        fn verify(&self, encoded: &str) -> Result<SessionInvitation> {
            SessionInvitation::verify_in_mode(
                encoded,
                self.host.public(),
                self.guest.public(),
                self.clock,
                AddressMode::Loopback,
            )
        }
        fn valid(&self) -> SessionInvitation {
            self.verify(&encode(&self.envelope).unwrap()).unwrap()
        }
        fn resigned(&self, mutate: impl FnOnce(&mut Body)) -> Result<SessionInvitation> {
            let mut body = self.envelope.body.clone();
            mutate(&mut body);
            let signature = hex(&self.host.sign(&signed_bytes(&body)?).to_bytes());
            self.verify(&encode(&Envelope { body, signature })?)
        }
        fn at(&self, wall_offset: u64, monotonic_offset: u64) -> Clock {
            Clock {
                unix_ms: self.clock.unix_ms + wall_offset,
                monotonic: self.clock.monotonic + Duration::from_millis(monotonic_offset),
            }
        }
        fn fragmented() -> Self {
            let mut fixture = Self::new();
            fixture.envelope.body.schema_version = 3;
            fixture.envelope.body.services = vec![Grant::game(), Grant::fragmented_aux()];
            fixture.envelope.signature = hex(&fixture
                .host
                .sign(&signed_bytes(&fixture.envelope.body).unwrap())
                .to_bytes());
            fixture
        }
        fn verify_fragmented(&self, encoded: &str) -> Result<SessionInvitation> {
            SessionInvitation::verify_in_profile(
                encoded,
                self.host.public(),
                self.guest.public(),
                VerificationOptions {
                    clock: self.clock,
                    mode: AddressMode::Loopback,
                    profile: SessionInvitationProfile::FragmentedGameUdp,
                },
            )
        }
        fn fragmented_value(&self) -> SessionInvitation {
            self.verify_fragmented(&encode(&self.envelope).unwrap())
                .unwrap()
        }
        fn resigned_fragmented(&self, mutate: impl FnOnce(&mut Body)) -> Result<SessionInvitation> {
            let mut envelope = self.envelope.clone();
            mutate(&mut envelope.body);
            envelope.signature = hex(&self.host.sign(&signed_bytes(&envelope.body)?).to_bytes());
            self.verify_fragmented(&encode(&envelope)?)
        }
    }
    fn service() -> Service {
        Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: "127.0.0.1:32001".parse().unwrap(),
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        }
    }
    fn fragmented_services() -> Vec<Service> {
        vec![
            service(),
            Service {
                service_id: "aux".into(),
                protocol: Protocol::Udp,
                endpoint: "127.0.0.1:32003".parse().unwrap(),
                revision: 1,
                max_datagram_bytes: 4096,
                required_capability: Capability::FragmentedDatagram,
                udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                    bytes_per_second: 16384,
                    datagrams_per_second: 20,
                },
            },
        ]
    }

    #[test]
    fn online_profile_keeps_independent_four_hour_session_and_short_admission() {
        let host = generate_key();
        let guest = generate_key();
        let now = unix_millis();
        let invitation = SessionInvitation::issue_online(
            &host,
            guest.public(),
            "online-test",
            &[service()],
            SessionDeadlines {
                admission_expires_at_unix_ms: now + 30_000,
                session_expires_at_unix_ms: now + 14_000_000,
            },
        )
        .unwrap();
        let verified =
            SessionInvitation::verify_online(&invitation.encode(), host.public(), guest.public())
                .unwrap();
        assert_eq!(verified.direct_addr(), "0.0.0.0:0".parse().unwrap());
        assert!(verified.session_remaining().unwrap() > Duration::from_secs(3600));
        assert!(
            SessionInvitation::verify_loopback(&invitation.encode(), host.public(), guest.public())
                .is_err()
        );
        assert!(
            SessionInvitation::issue_online(
                &host,
                guest.public(),
                "online-test",
                &[service()],
                SessionDeadlines {
                    admission_expires_at_unix_ms: now + 60_100,
                    session_expires_at_unix_ms: now + 14_000_000
                }
            )
            .is_err()
        );
    }

    #[test]
    fn online_fragmented_schema5_has_separate_authority_and_exact_aux_grant() {
        let host = generate_key();
        let guest = generate_key();
        let now = unix_millis();
        let deadlines = SessionDeadlines {
            admission_expires_at_unix_ms: now + 30_000,
            session_expires_at_unix_ms: now + 300_000,
        };
        let voice = SessionInvitation::issue_online_fragmented(
            &host,
            guest.public(),
            "online-voice",
            &fragmented_services(),
            deadlines,
        )
        .unwrap();
        let game = SessionInvitation::issue_online(
            &host,
            guest.public(),
            "online-voice",
            &[service()],
            deadlines,
        )
        .unwrap();
        assert_eq!(voice.preview()["schemaVersion"], 5);
        assert_eq!(voice.preview()["profile"], "onlineFragmentedGameUdp");
        assert_eq!(voice.preview()["services"][1]["maxDatagramBytes"], 4096);
        assert!(
            SessionInvitation::verify_online_fragmented(
                &voice.encode(),
                host.public(),
                guest.public()
            )
            .is_ok()
        );
        assert!(
            SessionInvitation::verify_online(&voice.encode(), host.public(), guest.public())
                .is_err()
        );
        assert!(
            SessionInvitation::verify_online_fragmented(
                &game.encode(),
                host.public(),
                guest.public()
            )
            .is_err()
        );
        assert!(
            SessionInvitation::verify_fragmented_loopback(
                &voice.encode(),
                host.public(),
                guest.public()
            )
            .is_err()
        );
        let signed = signed_bytes(&voice.envelope.body).unwrap();
        assert!(signed.starts_with(ONLINE_FRAGMENTED_DOMAIN));
        assert!(!signed.starts_with(ONLINE_DOMAIN));
        let mut wrong = fragmented_services();
        wrong[1].max_datagram_bytes = 4097;
        assert!(
            SessionInvitation::issue_online_fragmented(
                &host,
                guest.public(),
                "online-voice",
                &wrong,
                deadlines
            )
            .is_err()
        );
        assert!(voice.check_same_online_profile(&game).is_err());
    }

    #[test]
    fn schema_two_signed_bytes_and_preview_keep_the_original_shape() {
        let f = Fixture::new();
        let body = &f.envelope.body;
        let expected_body = format!(
            "{{\"schemaVersion\":2,\"hostKey\":\"{}\",\"guestKey\":\"{}\",\"sessionId\":\"session_test\",\"invitationId\":\"{}\",\"issuedAtUnixMs\":1000000,\"admissionExpiresAtUnixMs\":1002000,\"sessionExpiresAtUnixMs\":1012000,\"generation\":1,\"directAddress\":\"127.0.0.1:32000\",\"services\":[{{\"serviceId\":\"game\",\"protocol\":\"tcp\",\"revision\":1,\"requiredCapability\":\"reliableStream\"}}]}}",
            body.host_key, body.guest_key, body.invitation_id,
        );
        let mut expected_signed = b"aurora/connect-lab/session-invitation/v2\0".to_vec();
        expected_signed.extend_from_slice(expected_body.as_bytes());
        assert_eq!(signed_bytes(body).unwrap(), expected_signed);
        let expected_signature = hex(&f.host.sign(&expected_signed).to_bytes());
        assert_eq!(
            f.valid().encode(),
            format!("{{\"body\":{expected_body},\"signature\":\"{expected_signature}\"}}")
        );
        let preview = f.valid().preview();
        assert_eq!(preview["profile"], "sessionGameTcp");
        assert!(preview.get("udpEnvelopeVersion").is_none());
        assert!(preview.get("maxFragmentFrameBytes").is_none());
        assert!(preview["services"][0].get("maxDatagramBytes").is_none());
        assert!(preview["services"][0].get("udpReturnPolicy").is_none());
    }

    #[test]
    fn fragmented_session_roundtrip_binds_grants_deadlines_and_profile() {
        let f = Fixture::fragmented();
        let value = f.fragmented_value();
        let preview = value.preview();
        assert_eq!(preview["schemaVersion"], 3);
        assert_eq!(preview["profile"], "sessionFragmentedGameUdp");
        assert_eq!(preview["udpEnvelopeVersion"], 2);
        assert_eq!(preview["maxFragmentFrameBytes"], 1264);
        assert_eq!(preview["services"][1]["maxDatagramBytes"], 4096);
        assert_eq!(
            preview["services"][1]["udpReturnPolicy"],
            json!({"type":"authorizedDownstream","bytesPerSecond":16384,"datagramsPerSecond":20})
        );
        assert_eq!(value.encode(), encode(&f.envelope).unwrap());
        assert_eq!(
            value.digest(),
            hex(&Sha256::digest(value.encode().as_bytes()))
        );
        assert_eq!(
            value.capabilities(),
            vec![Capability::ReliableStream, Capability::FragmentedDatagram]
        );
        assert_eq!(
            value.lease().services,
            ["aux".to_owned(), "game".to_owned()].into_iter().collect()
        );
        assert_eq!(value.lease().expires_at_unix_ms, 1_012_000);
        assert_eq!(value.proof().digest, value.digest());
        assert_eq!(
            value.check_profile(SessionInvitationProfile::FragmentedGameUdp),
            Ok(())
        );
        assert_eq!(
            value.check_profile(SessionInvitationProfile::GameTcp),
            Err(Error::Configuration)
        );
        assert!(value.encode().len() <= MAX_SESSION_INVITATION_BYTES);
        assert!(!value.encode().contains("32003"));
        assert!(!value.encode().contains(&hex(&f.host.to_bytes())));
        assert!(!value.encode().contains(&hex(&f.guest.to_bytes())));
    }

    #[test]
    fn fragmented_session_requires_explicit_schema_domain_and_address_policy() {
        let old = Fixture::new();
        let f = Fixture::fragmented();
        assert!(matches!(
            old.verify_fragmented(&encode(&old.envelope).unwrap()),
            Err(Error::Configuration)
        ));
        assert!(matches!(
            f.verify(&encode(&f.envelope).unwrap()),
            Err(Error::Configuration)
        ));
        assert_eq!(
            old.valid()
                .check_profile(SessionInvitationProfile::FragmentedGameUdp),
            Err(Error::Configuration)
        );
        for domain in [DOMAIN, b"aurora/connect-lab/invitation/v1\0"] {
            let mut envelope = f.envelope.clone();
            let mut signed = domain.to_vec();
            signed.extend_from_slice(&serde_json::to_vec(&envelope.body).unwrap());
            envelope.signature = hex(&f.host.sign(&signed).to_bytes());
            assert!(matches!(
                f.verify_fragmented(&encode(&envelope).unwrap()),
                Err(Error::Unauthorized)
            ));
        }
        let mut old_envelope = old.envelope.clone();
        let mut signed = FRAGMENTED_DOMAIN.to_vec();
        signed.extend_from_slice(&serde_json::to_vec(&old_envelope.body).unwrap());
        old_envelope.signature = hex(&old.host.sign(&signed).to_bytes());
        assert!(matches!(
            old.verify(&encode(&old_envelope).unwrap()),
            Err(Error::Unauthorized)
        ));
        for profile in [
            SessionInvitationProfile::GameTcp,
            SessionInvitationProfile::FragmentedGameUdp,
        ] {
            let fixture = if profile == SessionInvitationProfile::GameTcp {
                &old
            } else {
                &f
            };
            assert!(matches!(
                SessionInvitation::verify_in_profile(
                    &encode(&fixture.envelope).unwrap(),
                    fixture.host.public(),
                    fixture.guest.public(),
                    VerificationOptions {
                        clock: fixture.clock,
                        mode: AddressMode::ExternalIpv6,
                        profile,
                    }
                ),
                Err(Error::Configuration)
            ));
        }
    }

    #[test]
    fn fragmented_session_factories_keep_exact_lifetimes_and_explicit_modes() {
        let host = generate_key();
        let guest = generate_key().public();
        let services = fragmented_services();
        for (admission, session) in [(1500, 4500), (600_000, 3_600_000)] {
            for external in [false, true] {
                let direct = if external {
                    "[2001:4860::1]:32000"
                } else {
                    "127.0.0.1:32000"
                }
                .parse()
                .unwrap();
                let issue = if external {
                    SessionInvitation::issue_fragmented_external_ipv6_for_lifetimes
                } else {
                    SessionInvitation::issue_fragmented_loopback_for_lifetimes
                };
                let value = issue(
                    &host,
                    guest,
                    "fragmented_session",
                    direct,
                    &services,
                    Duration::from_millis(admission),
                    Duration::from_millis(session),
                )
                .unwrap();
                assert_eq!(
                    value.admission_expires_at_unix_ms() - value.envelope.body.issued_at_unix_ms,
                    admission
                );
                assert_eq!(
                    value.session_expires_at_unix_ms() - value.envelope.body.issued_at_unix_ms,
                    session
                );
                let verify = if external {
                    SessionInvitation::verify_fragmented_external_ipv6
                } else {
                    SessionInvitation::verify_fragmented_loopback
                };
                assert_eq!(
                    verify(&value.encode(), host.public(), guest)
                        .unwrap()
                        .encode(),
                    value.encode()
                );
                let opposite = if external {
                    SessionInvitation::verify_fragmented_loopback
                } else {
                    SessionInvitation::verify_fragmented_external_ipv6
                };
                assert!(opposite(&value.encode(), host.public(), guest).is_err());
                assert!(
                    SessionInvitation::verify_loopback(&value.encode(), host.public(), guest)
                        .is_err()
                );
                assert!(
                    SessionInvitation::verify_external_ipv6(&value.encode(), host.public(), guest)
                        .is_err()
                );
                assert!(
                    SessionInvitation::verify_fragmented_loopback(
                        &value.encode(),
                        generate_key().public(),
                        guest
                    )
                    .is_err()
                );
                assert!(
                    SessionInvitation::verify_fragmented_loopback(
                        &value.encode(),
                        host.public(),
                        generate_key().public()
                    )
                    .is_err()
                );
            }
        }
        for (admission, session) in [
            (Duration::ZERO, Duration::from_secs(1)),
            (Duration::from_secs(601), Duration::from_secs(601)),
            (Duration::from_secs(1), Duration::from_secs(3601)),
            (Duration::from_secs(2), Duration::from_secs(1)),
            (Duration::from_secs(u64::MAX), Duration::from_secs(u64::MAX)),
            (Duration::from_nanos(1_000_001), Duration::from_secs(1)),
            (Duration::from_secs(1), Duration::from_nanos(1_000_000_001)),
        ] {
            assert!(matches!(
                SessionInvitation::issue_fragmented_loopback_for_lifetimes(
                    &host,
                    guest,
                    "fragmented_session",
                    "127.0.0.1:32000".parse().unwrap(),
                    &services,
                    admission,
                    session
                ),
                Err(Error::Configuration)
            ));
        }
    }

    #[test]
    fn fragmented_session_rejects_changed_or_extra_grants_even_when_signed() {
        let f = Fixture::fragmented();
        let changes: [fn(&mut Body); 18] = [
            |b| b.schema_version = 2,
            |b| b.generation = 2,
            |b| b.services.clear(),
            |b| {
                b.services.pop();
            },
            |b| b.services.swap(0, 1),
            |b| b.services.push(Grant::game()),
            |b| b.services[0].max_datagram_bytes = Some(4096),
            |b| b.services[0].udp_return_policy = Some(ReturnPolicy::ReplyBudget),
            |b| b.services[1].service_id = "voice".into(),
            |b| b.services[1].protocol = Protocol::Tcp,
            |b| b.services[1].revision = 2,
            |b| b.services[1].required_capability = Capability::Datagram,
            |b| b.services[1].max_datagram_bytes = None,
            |b| b.services[1].max_datagram_bytes = Some(4095),
            |b| b.services[1].max_datagram_bytes = Some(4097),
            |b| b.services[1].udp_return_policy = None,
            |b| b.services[1].udp_return_policy = Some(ReturnPolicy::ReplyBudget),
            |b| {
                b.services[1].udp_return_policy = Some(ReturnPolicy::AuthorizedDownstream {
                    bytes_per_second: 16385,
                    datagrams_per_second: 20,
                })
            },
        ];
        for mutate in changes {
            assert!(matches!(
                f.resigned_fragmented(mutate),
                Err(Error::Configuration)
            ));
        }
        assert!(
            f.resigned_fragmented(|b| b.services[1].udp_return_policy =
                Some(ReturnPolicy::AuthorizedDownstream {
                    bytes_per_second: 16384,
                    datagrams_per_second: 21
                }))
            .is_err()
        );
        for mutate in [
            (|b: &mut Body| b.admission_expires_at_unix_ms += 1) as fn(&mut Body),
            |b| b.session_expires_at_unix_ms += 1,
            |b| b.direct_address = "127.0.0.1:32005".parse().unwrap(),
        ] {
            let mut envelope = f.envelope.clone();
            mutate(&mut envelope.body);
            assert!(matches!(
                f.verify_fragmented(&encode(&envelope).unwrap()),
                Err(Error::Unauthorized)
            ));
        }
    }

    #[test]
    fn fragmented_session_rejects_noncanonical_unknown_duplicate_and_null_fields() {
        let f = Fixture::fragmented();
        let encoded = encode(&f.envelope).unwrap();
        for changed in [
            encoded.clone() + "\n",
            " ".to_string() + &encoded,
            encoded.replace(
                "\"schemaVersion\":3",
                "\"schemaVersion\":3,\"schemaVersion\":3",
            ),
            encoded.replace(
                "\"maxDatagramBytes\":4096",
                "\"maxDatagramBytes\":4096,\"maxDatagramBytes\":4096",
            ),
            encoded.replace(
                "\"maxDatagramBytes\":4096",
                "\"maxDatagramBytes\":4096,\"unknown\":0",
            ),
            encoded.replace(
                "\"datagramsPerSecond\":20",
                "\"datagramsPerSecond\":20,\"unknown\":0",
            ),
            encoded.replace(
                "\"reliableStream\"",
                "\"reliableStream\",\"maxDatagramBytes\":null",
            ),
            encoded.replace("\"aux\"", "\"a\\u0075x\""),
        ] {
            assert!(matches!(
                f.verify_fragmented(&changed),
                Err(Error::Protocol)
            ));
        }
        assert!(matches!(
            f.verify_fragmented(&"x".repeat(4097)),
            Err(Error::Limit)
        ));
    }

    #[test]
    fn fragmented_session_original_clocks_survive_admission_expiry_without_renewal() {
        let f = Fixture::fragmented();
        let value = f.fragmented_value();
        let copied = value.clone();
        assert_eq!(value.admission_deadline, copied.admission_deadline);
        assert_eq!(value.session_deadline, copied.session_deadline);
        for clock in [f.at(2000, 0), f.at(0, 2000)] {
            assert_eq!(copied.check_admission_at(clock), Err(Error::Expired));
            assert_eq!(copied.check_session_at(clock), Ok(()));
        }
        assert_eq!(
            copied.session_remaining_at(f.at(2000, 2000)),
            Ok(Duration::from_millis(10000))
        );
        assert_eq!(
            copied.session_remaining_at(f.at(1000, 11000)),
            Ok(Duration::from_millis(1000))
        );
        for clock in [
            f.at(12000, 0),
            f.at(0, 12000),
            Clock {
                unix_ms: f.clock.unix_ms - 1,
                ..f.clock
            },
        ] {
            assert_eq!(copied.check_session_at(clock), Err(Error::Expired));
        }
        let options = VerificationOptions {
            clock: f.at(2000, 2000),
            mode: AddressMode::Loopback,
            profile: SessionInvitationProfile::FragmentedGameUdp,
        };
        assert!(matches!(
            SessionInvitation::verify_in_profile(
                &value.encode(),
                f.host.public(),
                f.guest.public(),
                options
            ),
            Err(Error::Expired)
        ));
    }

    #[test]
    fn fragmented_session_host_registry_lease_and_profile_are_exact_before_io() {
        let f = Fixture::fragmented();
        let value = f.fragmented_value();
        let options = VerificationOptions {
            clock: f.clock,
            mode: AddressMode::Loopback,
            profile: SessionInvitationProfile::FragmentedGameUdp,
        };
        let config = || HostConfig {
            secret_key: f.host.clone(),
            bind_addr: value.direct_addr(),
            session_id: value.session_id().into(),
            services: fragmented_services(),
            leases: vec![value.lease()],
            limits: crate::Limits::default(),
        };
        assert_eq!(value.check_host_profile_at(&config(), options), Ok(()));
        assert_eq!(
            value.check_host_at(&config(), AddressMode::Loopback, f.clock),
            Err(Error::Configuration)
        );
        let changes: [fn(&mut HostConfig); 18] = [
            |c| c.secret_key = generate_key(),
            |c| c.bind_addr = "127.0.0.1:32005".parse().unwrap(),
            |c| c.session_id = "other_session".into(),
            |c| c.leases.clear(),
            |c| c.leases.push(c.leases[0].clone()),
            |c| c.leases[0].expires_at_unix_ms = 1_002_000,
            |c| {
                c.leases[0].services.remove("aux");
            },
            |c| {
                c.leases[0].services.insert("voice".into());
            },
            |c| c.services.swap(0, 1),
            |c| c.services[0].max_datagram_bytes = 1401,
            |c| c.services[1].endpoint = "192.0.2.1:32003".parse().unwrap(),
            |c| c.services[1].service_id = "voice".into(),
            |c| c.services[1].protocol = Protocol::Tcp,
            |c| c.services[1].revision = 2,
            |c| c.services[1].required_capability = Capability::Datagram,
            |c| c.services[1].max_datagram_bytes = 4095,
            |c| c.services[1].udp_return_policy = UdpReturnPolicy::ReplyBudget,
            |c| {
                c.services[1].udp_return_policy = UdpReturnPolicy::AuthorizedDownstream {
                    bytes_per_second: 16384,
                    datagrams_per_second: 21,
                }
            },
        ];
        for change in changes {
            let mut changed = config();
            change(&mut changed);
            assert_eq!(
                value.check_host_profile_at(&changed, options),
                Err(Error::Configuration)
            );
        }
        assert!(
            validate_profile_services(&fragmented_services(), SessionInvitationProfile::GameTcp)
                .is_err()
        );
        assert!(
            validate_profile_services(&[service()], SessionInvitationProfile::FragmentedGameUdp)
                .is_err()
        );
        let mut changed = fragmented_services();
        changed[1].max_datagram_bytes = 4097;
        assert!(
            validate_profile_services(&changed, SessionInvitationProfile::FragmentedGameUdp)
                .is_err()
        );
        // The new profile fixes this otherwise inert generic TCP field, while
        // existing schema-2 callers keep their original accepted range.
        for maximum in [1, 1399, 1401, 65507] {
            let mut game = service();
            game.max_datagram_bytes = maximum;
            assert!(
                validate_profile_services(&[game.clone()], SessionInvitationProfile::GameTcp)
                    .is_ok()
            );
            let mut services = fragmented_services();
            services[0] = game;
            assert!(
                validate_profile_services(&services, SessionInvitationProfile::FragmentedGameUdp)
                    .is_err()
            );
        }
    }

    #[test]
    fn duration_factories_use_exact_signed_intervals_and_keep_explicit_address_policy() {
        let host = generate_key();
        let guest = generate_key().public();
        for (admission, session) in [(2500, 4500), (600_000, 3_600_000)] {
            for external in [false, true] {
                let direct = if external {
                    "[2001:4860::1]:32000"
                } else {
                    "127.0.0.1:32000"
                }
                .parse()
                .unwrap();
                let factory = if external {
                    SessionInvitation::issue_external_ipv6_for_lifetimes
                } else {
                    SessionInvitation::issue_loopback_for_lifetimes
                };
                let value = factory(
                    &host,
                    guest,
                    "duration_test",
                    direct,
                    &[service()],
                    Duration::from_millis(admission),
                    Duration::from_millis(session),
                )
                .unwrap();
                let body = &value.envelope.body;
                assert_eq!(
                    body.admission_expires_at_unix_ms - body.issued_at_unix_ms,
                    admission
                );
                assert_eq!(
                    body.session_expires_at_unix_ms - body.issued_at_unix_ms,
                    session
                );
                let verified = if external {
                    SessionInvitation::verify_external_ipv6(&value.encode(), host.public(), guest)
                } else {
                    SessionInvitation::verify_loopback(&value.encode(), host.public(), guest)
                }
                .unwrap();
                assert!(value.encode() == verified.encode());
                let opposite = if external {
                    SessionInvitation::verify_loopback(&value.encode(), host.public(), guest)
                } else {
                    SessionInvitation::verify_external_ipv6(&value.encode(), host.public(), guest)
                };
                assert!(opposite.is_err());
            }
        }
    }

    #[test]
    fn duration_factories_reject_out_of_bounds_and_submillisecond_values_without_truncation() {
        let host = generate_key();
        let guest = generate_key().public();
        for (admission, session) in [
            (Duration::ZERO, Duration::from_secs(1)),
            (Duration::from_secs(601), Duration::from_secs(601)),
            (Duration::from_secs(1), Duration::from_secs(3601)),
            (Duration::from_secs(2), Duration::from_secs(1)),
            (Duration::from_secs(u64::MAX), Duration::from_secs(u64::MAX)),
            (Duration::from_nanos(1_000_001), Duration::from_secs(1)),
            (Duration::from_secs(1), Duration::from_nanos(1_000_000_001)),
        ] {
            assert!(matches!(
                SessionInvitation::issue_loopback_for_lifetimes(
                    &host,
                    guest,
                    "duration_test",
                    "127.0.0.1:32000".parse().unwrap(),
                    &[service()],
                    admission,
                    session
                ),
                Err(Error::Configuration)
            ));
        }
    }

    #[test]
    fn canonical_roundtrip_pins_and_preview_have_separate_deadlines() {
        let f = Fixture::new();
        let value = f.valid();
        assert_eq!(value.encode(), encode(&f.envelope).unwrap());
        assert_eq!(
            value.digest(),
            hex(&Sha256::digest(value.encode().as_bytes()))
        );
        assert_eq!(value.preview()["admissionExpiresAtUnixMs"], 1_002_000);
        assert_eq!(value.preview()["sessionExpiresAtUnixMs"], 1_012_000);
        assert_eq!(value.preview()["schemaVersion"], 2);
        assert_eq!(value.host_key(), f.host.public());
        assert_eq!(value.guest_key(), f.guest.public());
        assert_eq!(value.generation(), 1);
        assert_eq!(value.invitation_id().len(), 64);
        assert!(value.encode().len() <= MAX_SESSION_INVITATION_BYTES);
        for (host, guest) in [
            (generate_key().public(), f.guest.public()),
            (f.host.public(), generate_key().public()),
            (f.host.public(), f.host.public()),
        ] {
            assert!(matches!(
                SessionInvitation::verify_in_mode(
                    &value.encode(),
                    host,
                    guest,
                    f.clock,
                    AddressMode::Loopback
                ),
                Err(Error::Unauthorized)
            ));
        }
    }
    #[test]
    fn signed_deadline_nonce_generation_address_and_session_tampering_is_rejected() {
        let f = Fixture::new();
        let changes: [fn(&mut Body); 6] = [
            |b| b.admission_expires_at_unix_ms += 1,
            |b| b.session_expires_at_unix_ms += 1,
            |b| b.invitation_id = generate_key().public().to_string(),
            |b| b.session_id = "other_session".into(),
            |b| b.direct_address = "127.0.0.1:32002".parse().unwrap(),
            |b| b.issued_at_unix_ms -= 1,
        ];
        for change in changes {
            let mut envelope = f.envelope.clone();
            change(&mut envelope.body);
            assert!(matches!(
                f.verify(&encode(&envelope).unwrap()),
                Err(Error::Unauthorized)
            ));
        }
        let mut envelope = f.envelope.clone();
        envelope.body.generation = 2;
        assert!(f.verify(&encode(&envelope).unwrap()).is_err());
        let mut envelope = f.envelope.clone();
        envelope.signature.replace_range(
            0..2,
            if &f.envelope.signature[..2] == "00" {
                "01"
            } else {
                "00"
            },
        );
        assert!(matches!(
            f.verify(&encode(&envelope).unwrap()),
            Err(Error::Unauthorized)
        ));
    }
    #[test]
    fn signature_domain_is_distinct_from_schema_one() {
        let f = Fixture::new();
        let mut envelope = f.envelope.clone();
        let mut wrong_domain = b"aurora/connect-lab/invitation/v1\0".to_vec();
        wrong_domain.extend_from_slice(&serde_json::to_vec(&envelope.body).unwrap());
        envelope.signature = hex(&f.host.sign(&wrong_domain).to_bytes());
        assert!(matches!(
            f.verify(&encode(&envelope).unwrap()),
            Err(Error::Unauthorized)
        ));
        assert!(
            crate::Invitation::verify(
                &encode(&f.envelope).unwrap(),
                f.host.public(),
                f.guest.public()
            )
            .is_err()
        );
        assert!(f.resigned(|b| b.schema_version = 1).is_err());
    }
    #[test]
    fn ttl_bounds_are_absolute_and_overflow_safe() {
        let f = Fixture::new();
        assert!(
            f.resigned(|b| {
                b.admission_expires_at_unix_ms = b.issued_at_unix_ms + 600_000;
                b.session_expires_at_unix_ms = b.issued_at_unix_ms + 3_600_000;
            })
            .is_ok()
        );
        let changes: [fn(&mut Body); 8] = [
            |b| b.admission_expires_at_unix_ms = b.issued_at_unix_ms,
            |b| b.admission_expires_at_unix_ms = b.issued_at_unix_ms - 1,
            |b| b.admission_expires_at_unix_ms = b.issued_at_unix_ms + 600_001,
            |b| b.session_expires_at_unix_ms = b.issued_at_unix_ms,
            |b| b.session_expires_at_unix_ms = b.issued_at_unix_ms + 3_600_001,
            |b| b.session_expires_at_unix_ms = b.admission_expires_at_unix_ms - 1,
            |b| b.session_expires_at_unix_ms = u64::MAX,
            |b| b.issued_at_unix_ms += 1,
        ];
        for change in changes {
            assert!(matches!(f.resigned(change), Err(Error::Expired)));
        }
    }
    #[test]
    fn admission_expiry_does_not_expire_an_already_verified_session() {
        let f = Fixture::new();
        let value = f.valid();
        assert!(value.check_admission_at(f.at(1_999, 1_999)).is_ok());
        assert_eq!(
            value.check_admission_at(f.at(2_000, 2_000)),
            Err(Error::Expired)
        );
        assert!(value.check_session_at(f.at(2_000, 2_000)).is_ok());
        assert!(value.check_session_at(f.at(11_999, 11_999)).is_ok());
        assert_eq!(
            value.check_session_at(f.at(12_000, 12_000)),
            Err(Error::Expired)
        );
        assert!(matches!(
            SessionInvitation::verify_in_mode(
                &value.encode(),
                f.host.public(),
                f.guest.public(),
                f.at(2_000, 2_000),
                AddressMode::Loopback
            ),
            Err(Error::Expired)
        ));
    }
    #[test]
    fn each_clock_independently_expires_and_clone_never_resets() {
        let f = Fixture::new();
        let value = f.valid();
        let copied = value.clone();
        assert_eq!(value.admission_deadline, copied.admission_deadline);
        assert_eq!(value.session_deadline, copied.session_deadline);
        for clock in [f.at(2_000, 0), f.at(0, 2_000)] {
            assert_eq!(copied.check_admission_at(clock), Err(Error::Expired));
            assert!(copied.check_session_at(clock).is_ok());
        }
        for clock in [f.at(12_000, 0), f.at(0, 12_000)] {
            assert_eq!(copied.check_session_at(clock), Err(Error::Expired));
        }
        let backwards = Clock {
            unix_ms: f.clock.unix_ms - 1,
            ..f.clock
        };
        assert_eq!(copied.check_session_at(backwards), Err(Error::Expired));
        assert_eq!(copied.check_admission_at(backwards), Err(Error::Expired));
        let later = SessionInvitation::verify_in_mode(
            &value.encode(),
            f.host.public(),
            f.guest.public(),
            f.at(500, 500),
            AddressMode::Loopback,
        )
        .unwrap();
        assert_eq!(value.admission_deadline, later.admission_deadline);
        assert_eq!(value.session_deadline, later.session_deadline);
    }
    #[test]
    fn remaining_uses_the_shorter_original_clock_and_never_renews() {
        let f = Fixture::new();
        let value = f.valid();
        assert_eq!(
            value.admission_remaining_at(f.at(500, 700)),
            Ok(Duration::from_millis(1300))
        );
        assert_eq!(
            value.admission_remaining_at(f.at(100, 1200)),
            Ok(Duration::from_millis(800))
        );
        assert_eq!(
            value.session_remaining_at(f.at(9000, 500)),
            Ok(Duration::from_millis(3000))
        );
        assert_eq!(
            value.session_remaining_at(f.at(500, 11000)),
            Ok(Duration::from_millis(1000))
        );
        let copied = value.clone();
        assert_eq!(
            copied.admission_remaining_at(f.at(2000, 2000)),
            Err(Error::Expired)
        );
        assert_eq!(
            copied.session_remaining_at(f.at(2000, 2000)),
            Ok(Duration::from_millis(10000))
        );
        assert_eq!(
            copied.session_remaining_at(f.at(12000, 0)),
            Err(Error::Expired)
        );
        assert_eq!(
            copied.session_remaining_at(f.at(0, 12000)),
            Err(Error::Expired)
        );
    }
    #[test]
    fn schema_generation_nonce_and_game_grant_are_strict_even_when_signed() {
        let f = Fixture::new();
        let changes: [fn(&mut Body); 10] = [
            |b| b.schema_version = 3,
            |b| b.generation = 0,
            |b| b.generation = 2,
            |b| b.invitation_id = "0".repeat(64),
            |b| b.session_id = "../other".into(),
            |b| b.services.clear(),
            |b| b.services.push(Grant::game()),
            |b| b.services[0].protocol = Protocol::Udp,
            |b| b.services[0].required_capability = Capability::FragmentedDatagram,
            |b| b.services[0].revision = 2,
        ];
        for change in changes {
            assert!(matches!(f.resigned(change), Err(Error::Configuration)));
        }
        assert!(f.resigned(|b| b.invitation_id = "z".repeat(64)).is_err());
        assert!(
            f.resigned(|b| b.services[0].service_id = "aux".into())
                .is_err()
        );
    }
    #[test]
    fn noncanonical_duplicate_unknown_oversized_and_trailing_input_are_rejected() {
        let f = Fixture::new();
        let encoded = encode(&f.envelope).unwrap();
        for changed in [
            encoded.clone() + "\n",
            " ".to_string() + &encoded,
            encoded.replace(
                "\"schemaVersion\":2",
                "\"schemaVersion\":2,\"schemaVersion\":2",
            ),
            encoded.replace(
                "\"schemaVersion\":2",
                "\"schemaVersion\":2,\"unexpected\":false",
            ),
            encoded.replace("\"revision\":1", "\"revision\":1,\"unexpected\":false"),
            encoded.replace("\"body\":", "\"extra\":0,\"body\":"),
            encoded.replace("\"game\"", "\"g\\u0061me\""),
        ] {
            assert!(matches!(f.verify(&changed), Err(Error::Protocol)));
        }
        assert!(matches!(f.verify(""), Err(Error::Limit)));
        assert!(matches!(f.verify(&"x".repeat(4097)), Err(Error::Limit)));
        let mut invalid = f.envelope.clone();
        invalid.signature = "A".repeat(128);
        assert!(matches!(
            f.verify(&encode(&invalid).unwrap()),
            Err(Error::Protocol)
        ));
        invalid.signature = "0".repeat(130);
        assert!(matches!(
            f.verify(&encode(&invalid).unwrap()),
            Err(Error::Protocol)
        ));
    }
    #[test]
    fn address_modes_are_explicit_and_do_not_open_any_sockets() {
        let f = Fixture::new();
        let mut envelope = f.envelope.clone();
        envelope.body.direct_address = "[2003::1]:32000".parse().unwrap();
        envelope.signature = hex(&f
            .host
            .sign(&signed_bytes(&envelope.body).unwrap())
            .to_bytes());
        let encoded = encode(&envelope).unwrap();
        assert!(matches!(f.verify(&encoded), Err(Error::Configuration)));
        assert!(
            SessionInvitation::verify_in_mode(
                &encoded,
                f.host.public(),
                f.guest.public(),
                f.clock,
                AddressMode::ExternalIpv6
            )
            .is_ok()
        );
        assert!(
            SessionInvitation::verify_in_mode(
                &encode(&f.envelope).unwrap(),
                f.host.public(),
                f.guest.public(),
                f.clock,
                AddressMode::ExternalIpv6
            )
            .is_err()
        );
        for address in [
            "0.0.0.0:32000",
            "127.0.0.1:0",
            "[::]:32000",
            "[::1%2]:32000",
        ] {
            assert!(
                f.resigned(|b| b.direct_address = address.parse().unwrap())
                    .is_err()
            );
        }
    }
    #[test]
    fn local_game_service_is_exact_and_private_keys_never_enter_the_envelope() {
        let host = generate_key();
        let guest = generate_key();
        let now = unix_millis();
        let deadlines = SessionDeadlines {
            admission_expires_at_unix_ms: now + 60_000,
            session_expires_at_unix_ms: now + 120_000,
        };
        let invitation = SessionInvitation::issue_loopback(
            &host,
            guest.public(),
            "local_session",
            "127.0.0.1:32000".parse().unwrap(),
            &[service()],
            deadlines,
        )
        .unwrap();
        assert!(invitation.check_admission_current().is_ok());
        assert!(invitation.check_session_current().is_ok());
        assert!(
            SessionInvitation::verify_loopback(&invitation.encode(), host.public(), guest.public())
                .is_ok()
        );
        assert!(!invitation.encode().contains("32001"));
        assert!(!invitation.encode().contains(&hex(&host.to_bytes())));
        assert!(!invitation.encode().contains(&hex(&guest.to_bytes())));
        let mut invalid = service();
        invalid.udp_return_policy = UdpReturnPolicy::AuthorizedDownstream {
            bytes_per_second: 1000,
            datagrams_per_second: 1,
        };
        assert!(validate_local_services(&[invalid]).is_err());
        let mut invalid = service();
        invalid.endpoint = "0.0.0.0:32001".parse().unwrap();
        assert!(validate_local_services(&[invalid]).is_err());
        assert!(validate_local_services(&[]).is_err());
        assert!(validate_local_services(&[service(), service()]).is_err());
    }

    #[test]
    fn host_projection_preserves_session_deadline_and_requires_exact_configuration() {
        let f = Fixture::new();
        let invitation = f.valid();
        let config = || HostConfig {
            secret_key: f.host.clone(),
            bind_addr: invitation.direct_addr(),
            session_id: invitation.session_id().into(),
            services: vec![service()],
            leases: vec![invitation.lease()],
            limits: crate::Limits::default(),
        };
        assert!(
            invitation
                .check_host_at(&config(), AddressMode::Loopback, f.clock)
                .is_ok()
        );
        let lease = invitation.lease();
        assert_eq!(lease.lease_id, invitation.invitation_id());
        assert_eq!(lease.peer, f.guest.public());
        assert_eq!(lease.expires_at_unix_ms, 1_012_000);
        assert_eq!(lease.services, ["game".to_owned()].into_iter().collect());
        assert_eq!(invitation.proof().digest, invitation.digest());
        assert_eq!(invitation.proof().generation, 1);
        assert_eq!(invitation.capabilities(), vec![Capability::ReliableStream]);
        let changes: [fn(&mut HostConfig); 14] = [
            |c| c.secret_key = generate_key(),
            |c| c.bind_addr = "127.0.0.1:32002".parse().unwrap(),
            |c| c.session_id = "other_session".into(),
            |c| c.leases.clear(),
            |c| c.leases.push(c.leases[0].clone()),
            |c| c.leases[0].lease_id = generate_key().public().to_string(),
            |c| c.leases[0].peer = generate_key().public(),
            |c| c.leases[0].expires_at_unix_ms = 1_002_000,
            |c| c.leases[0].expires_at_unix_ms += 1,
            |c| {
                c.leases[0].services.insert("aux".into());
            },
            |c| c.services[0].protocol = Protocol::Udp,
            |c| c.services[0].revision = 2,
            |c| c.services[0].required_capability = Capability::FragmentedDatagram,
            |c| c.services[0].endpoint = "0.0.0.0:32001".parse().unwrap(),
        ];
        for change in changes {
            let mut changed = config();
            change(&mut changed);
            assert!(matches!(
                invitation.check_host_at(&changed, AddressMode::Loopback, f.clock),
                Err(Error::Configuration)
            ));
        }
        assert_eq!(
            invitation.check_host_at(&config(), AddressMode::Loopback, f.at(2_000, 2_000)),
            Err(Error::Expired)
        );
        assert!(matches!(
            invitation.check_host_at(&config(), AddressMode::ExternalIpv6, f.clock),
            Err(Error::Configuration)
        ));
        // Projections and clock checks never serialize/verify again or move a
        // deadline. The stored session remains valid after admission expires.
        assert!(invitation.check_session_at(f.at(2_000, 2_000)).is_ok());
        assert_eq!(
            invitation.session_deadline,
            f.clock.monotonic + Duration::from_secs(12)
        );
    }
}
