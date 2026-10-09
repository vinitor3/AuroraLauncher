//! Explicit schema-2 game/TCP invitations.
//! Admission and the admitted session have independent absolute deadlines.
//! Host owns admission, consumption and revocation; this value never renews.
use crate::{
    Capability, EndpointId, Error, HostConfig, Lease, Protocol, Result, SecretKey, Service,
    UdpReturnPolicy, generate_key, invitation::InvitationAddressMode as AddressMode,
    model::valid_id, unix_millis, wire::InvitationProof,
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
}
impl Grant {
    fn game() -> Self {
        Self {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            revision: 1,
            required_capability: Capability::ReliableStream,
        }
    }
    fn validate(&self) -> Result<()> {
        if self.service_id != "game"
            || self.protocol != Protocol::Tcp
            || self.revision != 1
            || self.required_capability != Capability::ReliableStream
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
    envelope: Envelope,
    encoded: String,
    digest: String,
    host: EndpointId,
    guest: EndpointId,
    admission_deadline: Instant,
    session_deadline: Instant,
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
impl SessionInvitation {
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

    fn issue_in_mode(
        host: &SecretKey,
        guest: EndpointId,
        session_id: &str,
        direct: SocketAddr,
        services: &[Service],
        timing: IssueTiming,
        mode: AddressMode,
    ) -> Result<Self> {
        let clock = Clock::now();
        let deadlines = match timing {
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
        validate_local_services(services)?;
        let body = Body {
            schema_version: 2,
            host_key: host.public().to_string(),
            guest_key: guest.to_string(),
            session_id: session_id.into(),
            invitation_id: generate_key().public().to_string(),
            issued_at_unix_ms: clock.unix_ms,
            admission_expires_at_unix_ms: deadlines.admission_expires_at_unix_ms,
            session_expires_at_unix_ms: deadlines.session_expires_at_unix_ms,
            generation: 1,
            direct_address: direct,
            services: vec![Grant::game()],
        };
        validate_body(&body, host.public(), guest, clock.unix_ms, mode)?;
        let signature = hex(&host.sign(&signed_bytes(&body)?).to_bytes());
        let value = Self::from_envelope(
            Envelope { body, signature },
            host.public(),
            guest,
            clock,
            mode,
        )?;
        value.check_admission_current()?;
        Ok(value)
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

    fn verify_in_mode(
        encoded: &str,
        expected_host: EndpointId,
        guest: EndpointId,
        clock: Clock,
        mode: AddressMode,
    ) -> Result<Self> {
        if encoded.is_empty() || encoded.len() > MAX_SESSION_INVITATION_BYTES {
            return Err(Error::Limit);
        }
        let envelope: Envelope = serde_json::from_str(encoded).map_err(|_| Error::Protocol)?;
        if encode(&envelope)? != encoded {
            return Err(Error::Protocol);
        }
        Self::from_envelope(envelope, expected_host, guest, clock, mode)
    }

    fn from_envelope(
        envelope: Envelope,
        expected_host: EndpointId,
        guest: EndpointId,
        clock: Clock,
        mode: AddressMode,
    ) -> Result<Self> {
        validate_body(&envelope.body, expected_host, guest, clock.unix_ms, mode)?;
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
            admission_deadline: deadline(envelope.body.admission_expires_at_unix_ms)?,
            session_deadline: deadline(envelope.body.session_expires_at_unix_ms)?,
            envelope,
            encoded,
            digest,
            host: expected_host,
            guest,
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
        let clock = Clock::now();
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
            services: ["game".to_owned()].into_iter().collect(),
        }
    }
    pub(crate) fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::ReliableStream]
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
    pub(crate) fn check_host(&self, config: &HostConfig) -> Result<()> {
        self.check_host_at(config, AddressMode::Loopback, Clock::now())
    }
    pub(crate) fn check_host_external_ipv6(&self, config: &HostConfig) -> Result<()> {
        self.check_host_at(config, AddressMode::ExternalIpv6, Clock::now())
    }
    fn check_host_at(&self, config: &HostConfig, mode: AddressMode, clock: Clock) -> Result<()> {
        self.check_admission_at(clock)?;
        self.check_address_mode(mode)?;
        validate_local_services(&config.services)?;
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
        json!({"schemaVersion":2,"experimental":true,"profile":"sessionGameTcp",
            "invitationSha256":self.digest,"hostKey":self.host.to_string(),"guestKey":self.guest.to_string(),
            "sessionId":self.session_id(),"invitationId":self.invitation_id(),"generation":self.generation(),
            "issuedAtUnixMs":self.envelope.body.issued_at_unix_ms,
            "admissionExpiresAtUnixMs":self.admission_expires_at_unix_ms(),
            "sessionExpiresAtUnixMs":self.session_expires_at_unix_ms(),
            "directAddress":self.direct_addr().to_string(),"services":self.envelope.body.services,
            "admissionSingleUse":true,"renewal":false,"relay":false,"accountIdentityVerified":false})
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
    now: u64,
    mode: AddressMode,
) -> Result<()> {
    let host_key =
        EndpointId::from_bytes(&decode_hex::<32>(&body.host_key)?).map_err(|_| Error::Protocol)?;
    let guest_key =
        EndpointId::from_bytes(&decode_hex::<32>(&body.guest_key)?).map_err(|_| Error::Protocol)?;
    let nonce = decode_hex::<32>(&body.invitation_id)?;
    if host_key != expected_host || guest_key != guest || host_key == guest_key {
        return Err(Error::Unauthorized);
    }
    if body.schema_version != 2
        || body.generation != 1
        || !valid_id(&body.session_id)
        || nonce == [0; 32]
        || body.services.len() != 1
    {
        return Err(Error::Configuration);
    }
    mode.validate(body.direct_address)?;
    body.services[0].validate()?;
    let admission_ttl = body
        .admission_expires_at_unix_ms
        .checked_sub(body.issued_at_unix_ms)
        .ok_or(Error::Expired)?;
    let session_ttl = body
        .session_expires_at_unix_ms
        .checked_sub(body.issued_at_unix_ms)
        .ok_or(Error::Expired)?;
    if admission_ttl == 0
        || admission_ttl > MAX_ADMISSION_LIFETIME.as_millis() as u64
        || session_ttl == 0
        || session_ttl > MAX_SESSION_LIFETIME.as_millis() as u64
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
    let mut bytes = Vec::with_capacity(DOMAIN.len() + encoded.len());
    bytes.extend_from_slice(DOMAIN);
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
