//! Manual, experimental invitations. No cloud identity or bearer authorization.
//! Canonical UTF-8 JSON is signed with Iroh's strict Ed25519 implementation.
//! All keys remain in the caller; invitations contain only public information.
use crate::{
    Capability, EndpointId, Error, HostConfig, Lease, Limits, Protocol, Result, SecretKey, Service,
    UdpReturnPolicy, generate_key, model::valid_id, unix_millis, wire::InvitationProof,
};
use iroh::Signature;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

pub const MAX_INVITATION_BYTES: usize = 4096;
pub const MAX_INVITATION_LIFETIME: Duration = Duration::from_secs(600);
const DOMAIN: &[u8] = b"aurora/connect-lab/invitation/v1\0";

#[derive(Clone, Copy)]
pub(crate) enum InvitationAddressMode {
    Loopback,
    ExternalIpv6,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InvitationProfile {
    Legacy,
    FragmentedV2,
}

#[derive(Clone, Copy)]
struct InvitationPolicy {
    address: InvitationAddressMode,
    profile: InvitationProfile,
}

impl InvitationPolicy {
    const fn legacy(address: InvitationAddressMode) -> Self {
        Self {
            address,
            profile: InvitationProfile::Legacy,
        }
    }

    const fn fragmented(address: InvitationAddressMode) -> Self {
        Self {
            address,
            profile: InvitationProfile::FragmentedV2,
        }
    }
}

/// Conservative classification of an explicitly selected initial address only.
/// No reachability/translation/tunnel detection or restriction of Iroh's other
/// internal paths is implied. This function opens no socket or lookup.
pub fn validate_external_invitation_address(address: SocketAddr) -> Result<()> {
    let SocketAddr::V6(address) = address else {
        return Err(Error::Configuration);
    };
    let s = address.ip().segments();
    if address.port() == 0
        || address.scope_id() != 0
        || address.flowinfo() != 0
        || s[0] & 0xe000 != 0x2000
        || (s[0] == 0x2001 && s[1] & 0xfe00 == 0)
        || (s[0] == 0x2001 && s[1] == 0x0db8)
        || s[0] == 0x2002
        || (s[0] == 0x2620 && s[1] == 0x004f && s[2] == 0x8000)
        || (s[0] == 0x3fff && s[1] & 0xf000 == 0)
        || (matches!(s[4], 0 | 0x0200) && s[5] == 0x5efe)
    {
        return Err(Error::Configuration);
    }
    Ok(())
}

impl InvitationAddressMode {
    pub(crate) fn validate(self, address: SocketAddr) -> Result<()> {
        match self {
            Self::Loopback => {
                if address.port() == 0
                    || !address.ip().is_loopback()
                    || matches!(address, SocketAddr::V6(address) if address.flowinfo() != 0 || address.scope_id() != 0)
                {
                    return Err(Error::Configuration);
                }
                Ok(())
            }
            Self::ExternalIpv6 => validate_external_invitation_address(address),
        }
    }
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
    expires_at_unix_ms: u64,
    generation: u64,
    direct_address: SocketAddr,
    services: Vec<Grant>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Grant {
    service_id: String,
    protocol: Protocol,
    revision: u32,
    required_capability: Capability,
    max_datagram_bytes: usize,
    udp_return_policy: ReturnPolicy,
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

impl Grant {
    fn from_service(service: &Service) -> Result<Self> {
        if !service.endpoint.ip().is_loopback() || service.endpoint.port() == 0 {
            return Err(Error::Configuration);
        }
        Ok(Self {
            service_id: service.service_id.clone(),
            protocol: service.protocol,
            revision: service.revision,
            required_capability: service.required_capability,
            max_datagram_bytes: service.max_datagram_bytes,
            udp_return_policy: match service.udp_return_policy {
                UdpReturnPolicy::ReplyBudget => ReturnPolicy::ReplyBudget,
                UdpReturnPolicy::AuthorizedDownstream {
                    bytes_per_second,
                    datagrams_per_second,
                } => ReturnPolicy::AuthorizedDownstream {
                    bytes_per_second,
                    datagrams_per_second,
                },
            },
        })
    }
    fn validate(&self, profile: InvitationProfile) -> Result<()> {
        let expected = match self.service_id.as_str() {
            "game" => (Protocol::Tcp, Capability::ReliableStream),
            "aux" => (
                Protocol::Udp,
                match profile {
                    InvitationProfile::Legacy => Capability::Datagram,
                    InvitationProfile::FragmentedV2 => Capability::FragmentedDatagram,
                },
            ),
            _ => return Err(Error::Configuration),
        };
        if self.protocol != expected.0
            || self.required_capability != expected.1
            || self.revision != 1
            || !(1..=65507).contains(&self.max_datagram_bytes)
            || (self.required_capability == Capability::FragmentedDatagram
                && self.max_datagram_bytes > crate::fragment::MAX_DATAGRAM)
        {
            return Err(Error::Configuration);
        }
        if let ReturnPolicy::AuthorizedDownstream {
            bytes_per_second,
            datagrams_per_second,
        } = self.udp_return_policy
        {
            let limits = Limits::default();
            if self.protocol != Protocol::Udp
                || bytes_per_second
                    < match profile {
                        InvitationProfile::Legacy => crate::DATAGRAM_HEADER_LEN as u32,
                        InvitationProfile::FragmentedV2 => crate::fragment::HEADER_LEN as u32,
                    }
                || bytes_per_second > limits.udp_bytes_per_second
                || datagrams_per_second == 0
                || datagrams_per_second > limits.datagrams_per_second
            {
                return Err(Error::Configuration);
            }
        }
        Ok(())
    }
}

fn grants(services: &[Service], profile: InvitationProfile) -> Result<Vec<Grant>> {
    if !(1..=2).contains(&services.len()) {
        return Err(Error::Configuration);
    }
    let mut result = services
        .iter()
        .map(Grant::from_service)
        .collect::<Result<Vec<_>>>()?;
    // Canonical order is game, then optional aux, independent of local registry order.
    result.sort_by_key(|grant| grant.service_id != "game");
    validate_grants(&result, profile)?;
    Ok(result)
}
fn validate_grants(grants: &[Grant], profile: InvitationProfile) -> Result<()> {
    if !(1..=2).contains(&grants.len())
        || (profile == InvitationProfile::FragmentedV2 && grants.len() != 2)
        || grants[0].service_id != "game"
        || (grants.len() == 2 && grants[1].service_id != "aux")
    {
        return Err(Error::Configuration);
    }
    for grant in grants {
        grant.validate(profile)?;
    }
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(DIGITS[(byte >> 4) as usize] as char);
        text.push(DIGITS[(byte & 15) as usize] as char);
    }
    text
}
fn decode_hex<const N: usize>(text: &str) -> Result<[u8; N]> {
    if text.len() != N * 2
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Error::Protocol);
    }
    let mut bytes = [0u8; N];
    for (index, pair) in text.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let value = |byte: u8| {
            if byte <= b'9' {
                byte - b'0'
            } else {
                byte - b'a' + 10
            }
        };
        bytes[index] = (value(pair[0]) << 4) | value(pair[1]);
    }
    Ok(bytes)
}
fn signed_bytes(body: &Body) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(body).map_err(|_| Error::Protocol)?;
    let mut bytes = Vec::with_capacity(DOMAIN.len() + json.len());
    bytes.extend_from_slice(DOMAIN);
    bytes.extend_from_slice(&json);
    Ok(bytes)
}
fn encode(envelope: &Envelope) -> Result<String> {
    let encoded = serde_json::to_string(envelope).map_err(|_| Error::Protocol)?;
    if encoded.len() > MAX_INVITATION_BYTES {
        return Err(Error::Limit);
    }
    Ok(encoded)
}

/// A verified immutable public invitation. Cloning preserves the monotonic
/// deadline; callers must not restore old host keys/sessions across executions.
/// The consumed/revoked admission record belongs to the running Host, not this
/// transferable value. There is no durable replay store or renewal API.
#[derive(Clone)]
pub struct Invitation {
    envelope: Envelope,
    encoded: String,
    digest: String,
    host: EndpointId,
    guest: EndpointId,
    deadline: Instant,
}
impl Invitation {
    pub fn issue(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        expires_at_unix_ms: u64,
    ) -> Result<Self> {
        Self::issue_in_mode(
            host,
            guest,
            session_id,
            direct,
            services,
            expires_at_unix_ms,
            InvitationPolicy::legacy(InvitationAddressMode::Loopback),
        )
    }

    /// Explicit external IPv6 mode. All envelope, grant and expiry rules match
    /// the loopback invitation; the address is serialized canonically and signed.
    pub fn issue_external_ipv6(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        expires_at_unix_ms: u64,
    ) -> Result<Self> {
        Self::issue_in_mode(
            host,
            guest,
            session_id,
            direct,
            services,
            expires_at_unix_ms,
            InvitationPolicy::legacy(InvitationAddressMode::ExternalIpv6),
        )
    }

    /// Explicit V2 grant profile, still limited to a loopback initial address.
    /// Legacy issue/verify and legacy Host/Guest entry points reject this profile.
    pub fn issue_fragmented(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        expires_at_unix_ms: u64,
    ) -> Result<Self> {
        Self::issue_in_mode(
            host,
            guest,
            session_id,
            direct,
            services,
            expires_at_unix_ms,
            InvitationPolicy::fragmented(InvitationAddressMode::Loopback),
        )
    }

    /// Both V2 and an external IPv6 initial address require this explicit API.
    pub fn issue_fragmented_external_ipv6(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        expires_at_unix_ms: u64,
    ) -> Result<Self> {
        Self::issue_in_mode(
            host,
            guest,
            session_id,
            direct,
            services,
            expires_at_unix_ms,
            InvitationPolicy::fragmented(InvitationAddressMode::ExternalIpv6),
        )
    }

    fn issue_in_mode(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        expires_at_unix_ms: u64,
        policy: InvitationPolicy,
    ) -> Result<Self> {
        let now = unix_millis();
        let body = Body {
            schema_version: 1,
            host_key: host.public().to_string(),
            guest_key: guest.to_string(),
            session_id: session_id.into(),
            invitation_id: generate_key().public().to_string(),
            issued_at_unix_ms: now,
            expires_at_unix_ms,
            generation: 1,
            direct_address: direct,
            services: grants(services, policy.profile)?,
        };
        Self::validate_body(&body, host.public(), guest, now, policy)?;
        let signature = hex(&host.sign(&signed_bytes(&body)?).to_bytes());
        Self::from_envelope(
            Envelope { body, signature },
            host.public(),
            guest,
            now,
            policy,
        )
    }

    /// Verify only after the operator obtained expected_host through an
    /// authorized manual channel. An arbitrary self-signed key is not identity.
    /// Input must be the exact canonical one-line JSON (without a final newline).
    pub fn verify(encoded: &str, expected_host: EndpointId, guest: EndpointId) -> Result<Self> {
        Self::verify_at(encoded, expected_host, guest, unix_millis())
    }

    /// Explicit opt-in; does not allow an old loopback API to accept this value.
    pub fn verify_external_ipv6(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
    ) -> Result<Self> {
        Self::verify_at_in_mode(
            encoded,
            expected_host,
            guest,
            unix_millis(),
            InvitationPolicy::legacy(InvitationAddressMode::ExternalIpv6),
        )
    }

    /// Verify a signed V2 profile only after explicit local transport opt-in.
    pub fn verify_fragmented(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
    ) -> Result<Self> {
        Self::verify_at_in_mode(
            encoded,
            expected_host,
            guest,
            unix_millis(),
            InvitationPolicy::fragmented(InvitationAddressMode::Loopback),
        )
    }

    pub fn verify_fragmented_external_ipv6(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
    ) -> Result<Self> {
        Self::verify_at_in_mode(
            encoded,
            expected_host,
            guest,
            unix_millis(),
            InvitationPolicy::fragmented(InvitationAddressMode::ExternalIpv6),
        )
    }
    fn verify_at(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
        now: u64,
    ) -> Result<Self> {
        Self::verify_at_in_mode(
            encoded,
            expected_host,
            guest,
            now,
            InvitationPolicy::legacy(InvitationAddressMode::Loopback),
        )
    }

    fn verify_at_in_mode(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
        now: u64,
        policy: InvitationPolicy,
    ) -> Result<Self> {
        if encoded.is_empty() || encoded.len() > MAX_INVITATION_BYTES {
            return Err(Error::Limit);
        }
        let envelope: Envelope = serde_json::from_str(encoded).map_err(|_| Error::Protocol)?;
        if encode(&envelope)? != encoded {
            return Err(Error::Protocol);
        }
        Self::from_envelope(envelope, expected_host, guest, now, policy)
    }
    fn validate_body(
        body: &Body,
        expected_host: EndpointId,
        guest: EndpointId,
        now: u64,
        policy: InvitationPolicy,
    ) -> Result<()> {
        let host_key = EndpointId::from_bytes(&decode_hex::<32>(&body.host_key)?)
            .map_err(|_| Error::Protocol)?;
        let guest_key = EndpointId::from_bytes(&decode_hex::<32>(&body.guest_key)?)
            .map_err(|_| Error::Protocol)?;
        decode_hex::<32>(&body.invitation_id)?;
        if host_key != expected_host || guest_key != guest || host_key == guest_key {
            return Err(Error::Unauthorized);
        }
        if body.schema_version != 1 || body.generation != 1 || !valid_id(&body.session_id) {
            return Err(Error::Configuration);
        }
        policy.address.validate(body.direct_address)?;
        let ttl = body
            .expires_at_unix_ms
            .checked_sub(body.issued_at_unix_ms)
            .ok_or(Error::Expired)?;
        if ttl == 0
            || ttl > MAX_INVITATION_LIFETIME.as_millis() as u64
            || body.issued_at_unix_ms > now
            || body.expires_at_unix_ms <= now
        {
            return Err(Error::Expired);
        }
        validate_grants(&body.services, policy.profile)
    }
    fn from_envelope(
        envelope: Envelope,
        expected_host: EndpointId,
        guest: EndpointId,
        now: u64,
        policy: InvitationPolicy,
    ) -> Result<Self> {
        Self::validate_body(&envelope.body, expected_host, guest, now, policy)?;
        let signature = Signature::from_bytes(&decode_hex::<64>(&envelope.signature)?);
        expected_host
            .verify(&signed_bytes(&envelope.body)?, &signature)
            .map_err(|_| Error::Unauthorized)?;
        let encoded = encode(&envelope)?;
        let digest = hex(&Sha256::digest(encoded.as_bytes()));
        let deadline =
            Instant::now() + Duration::from_millis(envelope.body.expires_at_unix_ms - now);
        Ok(Self {
            envelope,
            encoded,
            digest,
            host: expected_host,
            guest,
            deadline,
        })
    }
    pub fn encode(&self) -> String {
        self.encoded.clone()
    }
    pub fn digest(&self) -> String {
        self.digest.clone()
    }
    pub fn preview(&self) -> Value {
        let mut preview = json!({ "schemaVersion": 1, "experimental": true, "invitationSha256": self.digest,
            "hostKey": self.envelope.body.host_key, "guestKey": self.envelope.body.guest_key,
            "sessionId": self.session_id(), "invitationId": self.envelope.body.invitation_id,
            "issuedAtUnixMs": self.envelope.body.issued_at_unix_ms, "expiresAtUnixMs": self.expires_at_unix_ms(),
            "generation": 1, "directAddress": self.direct_addr().to_string(), "services": self.envelope.body.services,
            "singleUse": true, "relay": false, "accountIdentityVerified": false });
        if self
            .envelope
            .body
            .services
            .iter()
            .any(|grant| grant.required_capability == Capability::FragmentedDatagram)
        {
            preview["profile"] = json!("fragmentedV2");
            preview["udpEnvelopeVersion"] = json!(2);
            preview["maxFragmentFrameBytes"] = json!(crate::MAX_FRAGMENT_FRAME_BYTES);
        }
        preview
    }
    pub fn lease(&self) -> Lease {
        Lease {
            lease_id: self.envelope.body.invitation_id.clone(),
            peer: self.guest,
            expires_at_unix_ms: self.expires_at_unix_ms(),
            services: self
                .envelope
                .body
                .services
                .iter()
                .map(|grant| grant.service_id.clone())
                .collect(),
        }
    }
    pub fn session_id(&self) -> &str {
        &self.envelope.body.session_id
    }
    pub fn direct_addr(&self) -> SocketAddr {
        self.envelope.body.direct_address
    }
    pub fn expires_at_unix_ms(&self) -> u64 {
        self.envelope.body.expires_at_unix_ms
    }
    pub(crate) fn host_key(&self) -> EndpointId {
        self.host
    }
    pub(crate) fn guest_key(&self) -> EndpointId {
        self.guest
    }
    pub(crate) fn capabilities(&self) -> Vec<Capability> {
        self.envelope
            .body
            .services
            .iter()
            .map(|grant| grant.required_capability)
            .collect()
    }
    pub(crate) fn proof(&self) -> InvitationProof {
        InvitationProof {
            digest: self.digest.clone(),
            generation: self.envelope.body.generation,
        }
    }
    pub(crate) fn check_current(&self) -> Result<()> {
        let now = unix_millis();
        if now < self.envelope.body.issued_at_unix_ms
            || now >= self.expires_at_unix_ms()
            || Instant::now() >= self.deadline
        {
            return Err(Error::Expired);
        }
        Ok(())
    }
    pub(crate) fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
    pub(crate) fn check_address_mode(&self, mode: InvitationAddressMode) -> Result<()> {
        mode.validate(self.direct_addr())
    }
    /// Defense in depth when a verified V2 value reaches an old entry point.
    pub(crate) fn check_legacy_profile(&self) -> Result<()> {
        validate_grants(&self.envelope.body.services, InvitationProfile::Legacy)
    }
    pub(crate) fn check_host(&self, config: &HostConfig) -> Result<()> {
        self.check_host_in_mode(config, InvitationAddressMode::Loopback)
    }
    pub(crate) fn check_host_external_ipv6(&self, config: &HostConfig) -> Result<()> {
        self.check_host_in_mode(config, InvitationAddressMode::ExternalIpv6)
    }
    fn check_host_in_mode(&self, config: &HostConfig, mode: InvitationAddressMode) -> Result<()> {
        self.check_address_mode(mode)?;
        self.check_legacy_profile()?;
        self.check_current()?;
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
            || grants(&config.services, InvitationProfile::Legacy)? != self.envelope.body.services
        {
            return Err(Error::Configuration);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_deadline_rejects_even_if_wall_clock_fields_still_allow() {
        let host = generate_key();
        let guest = generate_key();
        let service = Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: "127.0.0.1:40001".parse().unwrap(),
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        };
        let mut invitation = Invitation::issue(
            &host,
            guest.public(),
            "session",
            "127.0.0.1:40002".parse().unwrap(),
            &[service],
            unix_millis() + 60_000,
        )
        .unwrap();
        invitation.deadline = Instant::now() - Duration::from_millis(1);
        assert_eq!(invitation.check_current(), Err(Error::Expired));
        assert_eq!(invitation.clone().remaining(), Duration::ZERO);
    }

    #[test]
    fn verify_rejects_wall_clock_before_issue_and_exact_expiry() {
        let host = generate_key();
        let guest = generate_key();
        let service = Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: "127.0.0.1:40001".parse().unwrap(),
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        };
        let invitation = Invitation::issue(
            &host,
            guest.public(),
            "session",
            "127.0.0.1:40002".parse().unwrap(),
            &[service],
            unix_millis() + 60_000,
        )
        .unwrap();
        for now in [
            invitation.envelope.body.issued_at_unix_ms - 1,
            invitation.expires_at_unix_ms(),
        ] {
            assert!(matches!(
                Invitation::verify_at(&invitation.encode(), host.public(), guest.public(), now),
                Err(Error::Expired)
            ));
        }
    }
}
