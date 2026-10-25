//! Opt-in local social admission. These typed signatures are separate from key
//! possession, manual invitations and the transport's existing TLS authority.
use crate::{EndpointId, SecretKey, SocialKeyChallenge, SocialKeyRole, unix_millis};
use iroh::Signature;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

pub const MAX_SOCIAL_ADMISSION_BYTES: usize = 8192;
const SAFE: u64 = 9_007_199_254_740_991;
const POLICY_DOMAIN: &str = "aurora/connect/social-host-policy/v1\0";
const GRANT_DOMAIN: &str = "aurora/connect/social-admission/v1\0";
const LEASE_DOMAIN: &str = "aurora/connect/social-continuity/v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SocialAdmissionError {
    #[error("social_admission_invalid")]
    Invalid,
    #[error("social_admission_context")]
    Context,
    #[error("social_admission_signature")]
    Signature,
    #[error("social_admission_expired")]
    Expired,
    #[error("social_admission_approval")]
    Approval,
    #[error("social_admission_sequence")]
    Sequence,
}
type Result<T> = std::result::Result<T, SocialAdmissionError>;
fn hex(value: &[u8]) -> String {
    value.iter().map(|v| format!("{v:02x}")).collect()
}
fn is_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn uid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || v == b'_' || v == b'-')
}
fn integer(value: u64) -> bool {
    value > 0 && value <= SAFE
}
fn uuid(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 36
        && b[14] == b'4'
        && matches!(b[19], b'8' | b'9' | b'a' | b'b')
        && b.iter().enumerate().all(|(i, &v)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                v == b'-'
            } else {
                v.is_ascii_digit() || (b'a'..=b'f').contains(&v)
            }
        })
}
fn public(value: &str) -> bool {
    is_hex(value) && value.parse::<EndpointId>().is_ok()
}
fn preimage(domain: &str, fields: &[String]) -> Vec<u8> {
    let mut out = domain.as_bytes().to_vec();
    for field in fields {
        out.extend_from_slice(&(field.len() as u16).to_be_bytes());
        out.extend_from_slice(field.as_bytes());
    }
    out
}
fn parse<T: for<'a> Deserialize<'a>>(text: &str) -> Result<T> {
    if text.is_empty() || text.len() > MAX_SOCIAL_ADMISSION_BYTES {
        return Err(SocialAdmissionError::Invalid);
    }
    serde_json::from_str(text).map_err(|_| SocialAdmissionError::Invalid)
}
fn verify_signature(pin: EndpointId, bytes: &[u8], signature: &str) -> Result<()> {
    if signature.len() != 128
        || !signature
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return Err(SocialAdmissionError::Signature);
    }
    let mut decoded = [0u8; 64];
    for (i, pair) in signature.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let nibble = |b| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
        decoded[i] = nibble(pair[0]) << 4 | nibble(pair[1]);
    }
    pin.verify(bytes, &Signature::from_bytes(&decoded))
        .map_err(|_| SocialAdmissionError::Signature)
}
fn current(issued: u64, expires: u64, deadline: Instant) -> Result<()> {
    let now = unix_millis();
    if now < issued || now >= expires || Instant::now() >= deadline {
        Err(SocialAdmissionError::Expired)
    } else {
        Ok(())
    }
}
fn deadline(at: Instant, now: u64, expiry: u64, cap: Instant) -> Result<Instant> {
    Ok(at
        .checked_add(Duration::from_millis(
            expiry
                .checked_sub(now)
                .ok_or(SocialAdmissionError::Expired)?,
        ))
        .ok_or(SocialAdmissionError::Invalid)?
        .min(cap))
}
pub fn social_source_digest(source: SocketAddr) -> Result<String> {
    if !matches!(source,SocketAddr::V4(v4) if v4.ip().octets()==[127,0,0,1] && v4.port()!=0) {
        return Err(SocialAdmissionError::Invalid);
    }
    Ok(hex(&Sha256::digest(format!(
        "aurora/connect/social-source/loopback/v1\0{source}"
    ))))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SocialHostPolicy {
    schema_version: u8,
    purpose: String,
    issuer_instance: String,
    policy_id: String,
    account_id: String,
    operation_id: String,
    generation: u64,
    public_key: String,
    session_id: String,
    session_digest: String,
    visibility: String,
    join_policy: String,
    service_id: String,
    protocol: String,
    source_digest: String,
    max_guests: u8,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
}
impl SocialHostPolicy {
    pub fn review(
        challenge: &SocialKeyChallenge,
        visibility: &str,
        source: SocketAddr,
    ) -> Result<Self> {
        challenge
            .validate()
            .map_err(|_| SocialAdmissionError::Expired)?;
        if challenge.role() != SocialKeyRole::Host {
            return Err(SocialAdmissionError::Context);
        }
        let value = Self {
            schema_version: 1,
            purpose: "hostSocialSession".into(),
            issuer_instance: challenge.issuer_instance().into(),
            policy_id: challenge.challenge_id().into(),
            account_id: challenge.account_id().into(),
            operation_id: challenge.operation_id().into(),
            generation: challenge.generation(),
            public_key: challenge.public_key().into(),
            session_id: challenge.session_id().into(),
            session_digest: challenge.session_digest().into(),
            visibility: visibility.into(),
            join_policy: "open".into(),
            service_id: "game".into(),
            protocol: "tcp".into(),
            source_digest: social_source_digest(source)?,
            max_guests: 1,
            issued_at_unix_ms: challenge.issued_at_unix_ms(),
            expires_at_unix_ms: challenge.expires_at_unix_ms(),
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.purpose != "hostSocialSession"
            || !is_hex(&self.issuer_instance)
            || !is_hex(&self.policy_id)
            || !uid(&self.account_id)
            || !uuid(&self.operation_id)
            || !integer(self.generation)
            || !public(&self.public_key)
            || !uuid(&self.session_id)
            || !is_hex(&self.session_digest)
            || !matches!(self.visibility.as_str(), "public" | "friends")
            || self.join_policy != "open"
            || self.service_id != "game"
            || self.protocol != "tcp"
            || !is_hex(&self.source_digest)
            || self.max_guests != 1
            || !integer(self.issued_at_unix_ms)
            || !integer(self.expires_at_unix_ms)
            || self.expires_at_unix_ms <= self.issued_at_unix_ms
            || self.expires_at_unix_ms - self.issued_at_unix_ms > 60_000
        {
            return Err(SocialAdmissionError::Invalid);
        }
        Ok(())
    }
    fn bytes(&self) -> Vec<u8> {
        preimage(
            POLICY_DOMAIN,
            &[
                self.schema_version.to_string(),
                self.purpose.clone(),
                self.issuer_instance.clone(),
                self.policy_id.clone(),
                self.account_id.clone(),
                self.operation_id.clone(),
                self.generation.to_string(),
                self.public_key.clone(),
                self.session_id.clone(),
                self.session_digest.clone(),
                self.visibility.clone(),
                self.join_policy.clone(),
                self.service_id.clone(),
                self.protocol.clone(),
                self.source_digest.clone(),
                self.max_guests.to_string(),
                self.issued_at_unix_ms.to_string(),
                self.expires_at_unix_ms.to_string(),
            ],
        )
    }
    pub fn digest(&self) -> String {
        hex(&Sha256::digest(self.bytes()))
    }
    pub fn approve(
        &self,
        key: &SecretKey,
        digest: &str,
        prepared_deadline: Instant,
    ) -> Result<SignedSocialHostPolicy> {
        self.validate()?;
        current(
            self.issued_at_unix_ms,
            self.expires_at_unix_ms,
            prepared_deadline,
        )?;
        if digest != self.digest() {
            return Err(SocialAdmissionError::Approval);
        }
        if key.public().to_string() != self.public_key {
            return Err(SocialAdmissionError::Context);
        }
        Ok(SignedSocialHostPolicy {
            body: self.clone(),
            signature: hex(&key.sign(&self.bytes()).to_bytes()),
        })
    }
    pub fn policy_id(&self) -> &str {
        &self.policy_id
    }
    pub fn session_digest(&self) -> &str {
        &self.session_digest
    }
    pub fn expires_at_unix_ms(&self) -> u64 {
        self.expires_at_unix_ms
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedSocialHostPolicy {
    body: SocialHostPolicy,
    signature: String,
}
impl SignedSocialHostPolicy {
    pub fn parse(text: &str) -> Result<Self> {
        let v: Self = parse(text)?;
        v.verify()?;
        Ok(v)
    }
    pub fn verify(&self) -> Result<()> {
        self.body.validate()?;
        verify_signature(
            self.body
                .public_key
                .parse()
                .map_err(|_| SocialAdmissionError::Invalid)?,
            &self.body.bytes(),
            &self.signature,
        )
    }
    pub fn body(&self) -> &SocialHostPolicy {
        &self.body
    }
    pub fn encode(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|_| SocialAdmissionError::Invalid)
    }
}

/// This expectation is supplied by the trusted runner before any grant, never
/// learned from an admission response or replaced during the operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SocialGuestExpectation {
    host_account_id: String,
    host_public_key: String,
    host_operation_id: String,
    host_generation: u64,
    policy_id: String,
    policy_digest: String,
    session_digest: String,
}
impl SocialGuestExpectation {
    pub fn parse(text: &str) -> Result<Self> {
        let v: Self = parse(text)?;
        v.validate()?;
        Ok(v)
    }
    fn validate(&self) -> Result<()> {
        if !uid(&self.host_account_id)
            || !public(&self.host_public_key)
            || !uuid(&self.host_operation_id)
            || !integer(self.host_generation)
            || !is_hex(&self.policy_id)
            || !is_hex(&self.policy_digest)
            || !is_hex(&self.session_digest)
        {
            return Err(SocialAdmissionError::Invalid);
        }
        Ok(())
    }
    pub fn session_digest(&self) -> &str {
        &self.session_digest
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GrantBody {
    schema_version: u8,
    purpose: String,
    issuer_instance: String,
    grant_id: String,
    policy_id: String,
    policy_digest: String,
    session_id: String,
    session_digest: String,
    host_account_id: String,
    host_public_key: String,
    host_operation_id: String,
    host_generation: u64,
    guest_account_id: String,
    guest_public_key: String,
    guest_operation_id: String,
    guest_generation: u64,
    issued_at_unix_ms: u64,
    admission_expires_at_unix_ms: u64,
    session_expires_at_unix_ms: u64,
}
impl GrantBody {
    fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.purpose != "socialAdmission"
            || !is_hex(&self.issuer_instance)
            || !is_hex(&self.grant_id)
            || !is_hex(&self.policy_id)
            || !is_hex(&self.policy_digest)
            || !uuid(&self.session_id)
            || !is_hex(&self.session_digest)
            || !uid(&self.host_account_id)
            || !public(&self.host_public_key)
            || !uuid(&self.host_operation_id)
            || !integer(self.host_generation)
            || !uid(&self.guest_account_id)
            || !public(&self.guest_public_key)
            || !uuid(&self.guest_operation_id)
            || !integer(self.guest_generation)
            || self.host_account_id == self.guest_account_id
            || self.host_public_key == self.guest_public_key
            || !integer(self.issued_at_unix_ms)
            || !integer(self.admission_expires_at_unix_ms)
            || !integer(self.session_expires_at_unix_ms)
            || self.admission_expires_at_unix_ms <= self.issued_at_unix_ms
            || self.admission_expires_at_unix_ms - self.issued_at_unix_ms > 10_000
            || self.session_expires_at_unix_ms < self.admission_expires_at_unix_ms
            || self.session_expires_at_unix_ms - self.issued_at_unix_ms > 60_000
        {
            return Err(SocialAdmissionError::Invalid);
        }
        Ok(())
    }
    fn bytes(&self) -> Vec<u8> {
        preimage(
            GRANT_DOMAIN,
            &[
                self.schema_version.to_string(),
                self.purpose.clone(),
                self.issuer_instance.clone(),
                self.grant_id.clone(),
                self.policy_id.clone(),
                self.policy_digest.clone(),
                self.session_id.clone(),
                self.session_digest.clone(),
                self.host_account_id.clone(),
                self.host_public_key.clone(),
                self.host_operation_id.clone(),
                self.host_generation.to_string(),
                self.guest_account_id.clone(),
                self.guest_public_key.clone(),
                self.guest_operation_id.clone(),
                self.guest_generation.to_string(),
                self.issued_at_unix_ms.to_string(),
                self.admission_expires_at_unix_ms.to_string(),
                self.session_expires_at_unix_ms.to_string(),
            ],
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantEnvelope {
    body: GrantBody,
    signature: String,
}
/// Clone preserves the original monotonic caps. Callers consume this once and
/// must not reconstruct authority by repeatedly parsing the signed bytes.
#[derive(Debug, Clone)]
pub struct VerifiedSocialGrant {
    body: GrantBody,
    pin: EndpointId,
    admission_deadline: Instant,
    session_deadline: Instant,
}
impl VerifiedSocialGrant {
    fn verified(
        text: &str,
        pin: EndpointId,
        challenge: &SocialKeyChallenge,
        prepared_deadline: Instant,
    ) -> Result<Self> {
        let at = Instant::now();
        let now = unix_millis();
        challenge
            .validate()
            .map_err(|_| SocialAdmissionError::Expired)?;
        let envelope: GrantEnvelope = parse(text)?;
        let b = envelope.body;
        b.validate()?;
        verify_signature(pin, &b.bytes(), &envelope.signature)?;
        if b.issuer_instance != challenge.issuer_instance()
            || b.session_id != challenge.session_id()
            || b.session_digest != challenge.session_digest()
            || b.issued_at_unix_ms < challenge.issued_at_unix_ms()
            || b.session_expires_at_unix_ms > challenge.expires_at_unix_ms()
        {
            return Err(SocialAdmissionError::Context);
        }
        let value = Self {
            admission_deadline: deadline(
                at,
                now,
                b.admission_expires_at_unix_ms,
                prepared_deadline,
            )?,
            session_deadline: deadline(at, now, b.session_expires_at_unix_ms, prepared_deadline)?,
            body: b,
            pin,
        };
        value.check_admission()?;
        Ok(value)
    }
    pub fn for_host(
        text: &str,
        pin: EndpointId,
        challenge: &SocialKeyChallenge,
        policy: &SignedSocialHostPolicy,
        source: SocketAddr,
        prepared_deadline: Instant,
    ) -> Result<Self> {
        policy.verify()?;
        let value = Self::verified(text, pin, challenge, prepared_deadline)?;
        let b = &value.body;
        let p = &policy.body;
        if challenge.role() != SocialKeyRole::Host
            || p.policy_id != challenge.challenge_id()
            || p.public_key != challenge.public_key()
            || p.account_id != challenge.account_id()
            || p.operation_id != challenge.operation_id()
            || p.generation != challenge.generation()
            || p.issuer_instance != challenge.issuer_instance()
            || p.session_id != challenge.session_id()
            || p.session_digest != challenge.session_digest()
            || p.issued_at_unix_ms != challenge.issued_at_unix_ms()
            || p.expires_at_unix_ms != challenge.expires_at_unix_ms()
            || p.source_digest != social_source_digest(source)?
            || b.policy_id != p.policy_id
            || b.policy_digest != p.digest()
            || b.host_account_id != p.account_id
            || b.host_public_key != p.public_key
            || b.host_operation_id != p.operation_id
            || b.host_generation != p.generation
            || b.session_expires_at_unix_ms > p.expires_at_unix_ms
        {
            return Err(SocialAdmissionError::Context);
        }
        Ok(value)
    }
    pub fn for_guest(
        text: &str,
        pin: EndpointId,
        challenge: &SocialKeyChallenge,
        expected: &SocialGuestExpectation,
        prepared_deadline: Instant,
    ) -> Result<Self> {
        expected.validate()?;
        let value = Self::verified(text, pin, challenge, prepared_deadline)?;
        let b = &value.body;
        if challenge.role() != SocialKeyRole::Guest
            || b.guest_account_id != challenge.account_id()
            || b.guest_public_key != challenge.public_key()
            || b.guest_operation_id != challenge.operation_id()
            || b.guest_generation != challenge.generation()
            || b.host_account_id != expected.host_account_id
            || b.host_public_key != expected.host_public_key
            || b.host_operation_id != expected.host_operation_id
            || b.host_generation != expected.host_generation
            || b.policy_id != expected.policy_id
            || b.policy_digest != expected.policy_digest
            || b.session_digest != expected.session_digest
        {
            return Err(SocialAdmissionError::Context);
        }
        Ok(value)
    }
    pub fn check_admission(&self) -> Result<()> {
        current(
            self.body.issued_at_unix_ms,
            self.body.admission_expires_at_unix_ms,
            self.admission_deadline,
        )
    }
    pub fn check_session(&self) -> Result<()> {
        current(
            self.body.issued_at_unix_ms,
            self.body.session_expires_at_unix_ms,
            self.session_deadline,
        )
    }
    pub fn grant_id(&self) -> &str {
        &self.body.grant_id
    }
    pub fn session_id(&self) -> &str {
        &self.body.session_id
    }
    pub fn host_key(&self) -> EndpointId {
        self.body
            .host_public_key
            .parse()
            .expect("validated host public key")
    }
    pub fn guest_key(&self) -> EndpointId {
        self.body
            .guest_public_key
            .parse()
            .expect("validated guest public key")
    }
    pub fn admission_expires_at_unix_ms(&self) -> u64 {
        self.body.admission_expires_at_unix_ms
    }
    pub fn session_expires_at_unix_ms(&self) -> u64 {
        self.body.session_expires_at_unix_ms
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LeaseBody {
    schema_version: u8,
    purpose: String,
    issuer_instance: String,
    grant_id: String,
    policy_digest: String,
    sequence: u64,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
}
impl LeaseBody {
    fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.purpose != "socialContinuity"
            || !is_hex(&self.issuer_instance)
            || !is_hex(&self.grant_id)
            || !is_hex(&self.policy_digest)
            || !integer(self.sequence)
            || !integer(self.issued_at_unix_ms)
            || !integer(self.expires_at_unix_ms)
            || self.expires_at_unix_ms <= self.issued_at_unix_ms
            || self.expires_at_unix_ms - self.issued_at_unix_ms > 3_000
        {
            return Err(SocialAdmissionError::Invalid);
        }
        Ok(())
    }
    fn bytes(&self) -> Vec<u8> {
        preimage(
            LEASE_DOMAIN,
            &[
                self.schema_version.to_string(),
                self.purpose.clone(),
                self.issuer_instance.clone(),
                self.grant_id.clone(),
                self.policy_digest.clone(),
                self.sequence.to_string(),
                self.issued_at_unix_ms.to_string(),
                self.expires_at_unix_ms.to_string(),
            ],
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeaseEnvelope {
    body: LeaseBody,
    signature: String,
}
pub struct SocialContinuity {
    grant: VerifiedSocialGrant,
    lease: LeaseBody,
    deadline: Instant,
    terminal: bool,
}
impl SocialContinuity {
    fn verify(text: &str, grant: &VerifiedSocialGrant) -> Result<(LeaseBody, Instant)> {
        let at = Instant::now();
        let now = unix_millis();
        grant.check_session()?;
        let envelope: LeaseEnvelope = parse(text)?;
        let b = envelope.body;
        b.validate()?;
        verify_signature(grant.pin, &b.bytes(), &envelope.signature)?;
        if b.issuer_instance != grant.body.issuer_instance
            || b.grant_id != grant.body.grant_id
            || b.policy_digest != grant.body.policy_digest
            || b.issued_at_unix_ms < grant.body.issued_at_unix_ms
            || b.expires_at_unix_ms > grant.body.session_expires_at_unix_ms
        {
            return Err(SocialAdmissionError::Context);
        }
        let end = deadline(at, now, b.expires_at_unix_ms, grant.session_deadline)?;
        current(b.issued_at_unix_ms, b.expires_at_unix_ms, end)?;
        Ok((b, end))
    }
    pub fn start(text: &str, grant: &VerifiedSocialGrant) -> Result<Self> {
        let (lease, deadline) = Self::verify(text, grant)?;
        if lease.sequence != 1 {
            return Err(SocialAdmissionError::Sequence);
        }
        Ok(Self {
            grant: grant.clone(),
            lease,
            deadline,
            terminal: false,
        })
    }
    pub fn update(&mut self, text: &str) -> Result<()> {
        let result = (|| {
            self.check()?;
            let (lease, deadline) = Self::verify(text, &self.grant)?;
            if lease.sequence <= self.lease.sequence {
                return Err(SocialAdmissionError::Sequence);
            }
            Ok((lease, deadline))
        })();
        match result {
            Ok((lease, deadline)) => {
                self.lease = lease;
                self.deadline = deadline;
                Ok(())
            }
            Err(error) => {
                self.terminal = true;
                Err(error)
            }
        }
    }
    pub fn check(&self) -> Result<()> {
        if self.terminal {
            return Err(SocialAdmissionError::Expired);
        }
        self.grant.check_session()?;
        current(
            self.lease.issued_at_unix_ms,
            self.lease.expires_at_unix_ms,
            self.deadline,
        )
    }
    pub fn sequence(&self) -> u64 {
        self.lease.sequence
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    struct Fixture {
        authority: SecretKey,
        host: SecretKey,
        guest: SecretKey,
        hc: SocialKeyChallenge,
        gc: SocialKeyChallenge,
        policy: SignedSocialHostPolicy,
        grant: GrantBody,
        expected: SocialGuestExpectation,
        cap: Instant,
    }
    fn challenge(key: &SecretKey, role: &str, now: u64) -> SocialKeyChallenge {
        let account = if role == "host" {
            "local_host"
        } else {
            "local_guest"
        };
        let id = if role == "host" {
            "11".repeat(32)
        } else {
            "22".repeat(32)
        };
        SocialKeyChallenge::parse(&json!({"schemaVersion":1,"purpose":"accountKeyBinding","issuerInstance":"33".repeat(32),
            "challengeId":id,"accountId":account,"operationId":"ed35a732-2839-4a18-9740-73ed5ecff11a","generation":1,"role":role,
            "publicKey":key.public().to_string(),"sessionId":"ed35a732-2839-4a18-9740-73ed5ecff11b","sessionDigest":"44".repeat(32),
            "issuedAtUnixMs":now,"expiresAtUnixMs":now+50_000}).to_string()).unwrap()
    }
    impl Fixture {
        fn new() -> Self {
            let authority = crate::generate_key();
            let host = crate::generate_key();
            let guest = crate::generate_key();
            let now = unix_millis();
            let cap = Instant::now() + Duration::from_secs(50);
            let hc = challenge(&host, "host", now);
            let gc = challenge(&guest, "guest", now);
            let review =
                SocialHostPolicy::review(&hc, "friends", "127.0.0.1:43001".parse().unwrap())
                    .unwrap();
            let policy = review.approve(&host, &review.digest(), cap).unwrap();
            let grant = GrantBody {
                schema_version: 1,
                purpose: "socialAdmission".into(),
                issuer_instance: hc.issuer_instance().into(),
                grant_id: "55".repeat(32),
                policy_id: hc.challenge_id().into(),
                policy_digest: review.digest(),
                session_id: hc.session_id().into(),
                session_digest: hc.session_digest().into(),
                host_account_id: hc.account_id().into(),
                host_public_key: host.public().to_string(),
                host_operation_id: hc.operation_id().into(),
                host_generation: 1,
                guest_account_id: gc.account_id().into(),
                guest_public_key: guest.public().to_string(),
                guest_operation_id: gc.operation_id().into(),
                guest_generation: 1,
                issued_at_unix_ms: now,
                admission_expires_at_unix_ms: now + 9_000,
                session_expires_at_unix_ms: now + 40_000,
            };
            let expected = SocialGuestExpectation {
                host_account_id: hc.account_id().into(),
                host_public_key: host.public().to_string(),
                host_operation_id: hc.operation_id().into(),
                host_generation: 1,
                policy_id: hc.challenge_id().into(),
                policy_digest: review.digest(),
                session_digest: hc.session_digest().into(),
            };
            Self {
                authority,
                host,
                guest,
                hc,
                gc,
                policy,
                grant,
                expected,
                cap,
            }
        }
        fn encoded(&self, body: &GrantBody) -> String {
            serde_json::to_string(&GrantEnvelope {
                body: body.clone(),
                signature: hex(&self.authority.sign(&body.bytes()).to_bytes()),
            })
            .unwrap()
        }
        fn verify_host(&self, encoded: &str) -> Result<VerifiedSocialGrant> {
            VerifiedSocialGrant::for_host(
                encoded,
                self.authority.public(),
                &self.hc,
                &self.policy,
                "127.0.0.1:43001".parse().unwrap(),
                self.cap,
            )
        }
        fn verified(&self) -> VerifiedSocialGrant {
            self.verify_host(&self.encoded(&self.grant)).unwrap()
        }
        fn lease(&self, sequence: u64, expiry: u64) -> String {
            let body = LeaseBody {
                schema_version: 1,
                purpose: "socialContinuity".into(),
                issuer_instance: self.grant.issuer_instance.clone(),
                grant_id: self.grant.grant_id.clone(),
                policy_digest: self.grant.policy_digest.clone(),
                sequence,
                issued_at_unix_ms: unix_millis(),
                expires_at_unix_ms: expiry,
            };
            serde_json::to_string(&LeaseEnvelope {
                signature: hex(&self.authority.sign(&body.bytes()).to_bytes()),
                body,
            })
            .unwrap()
        }
    }
    #[test]
    fn social_admission_host_requires_exact_approval_source_and_original_key() {
        let f = Fixture::new();
        let review = f.policy.body();
        assert_eq!(
            review.approve(&f.host, "wrong", f.cap).unwrap_err(),
            SocialAdmissionError::Approval
        );
        assert_eq!(
            review
                .approve(&f.guest, &review.digest(), f.cap)
                .unwrap_err(),
            SocialAdmissionError::Context
        );
        assert_eq!(
            review
                .approve(&f.host, &review.digest(), Instant::now())
                .unwrap_err(),
            SocialAdmissionError::Expired
        );
        assert!(
            SocialHostPolicy::review(&f.hc, "private", "127.0.0.1:43001".parse().unwrap()).is_err()
        );
        assert!(
            SocialHostPolicy::review(&f.gc, "friends", "127.0.0.1:43001".parse().unwrap()).is_err()
        );
        for source in [
            "127.0.0.2:43001",
            "[::1]:43001",
            "127.0.0.1:0",
            "192.168.1.1:43001",
        ] {
            assert!(social_source_digest(source.parse().unwrap()).is_err());
        }
        f.policy.verify().unwrap();
        assert_eq!(
            SignedSocialHostPolicy::parse(&f.policy.encode().unwrap())
                .unwrap()
                .body()
                .digest(),
            review.digest()
        );
        assert!(
            VerifiedSocialGrant::for_host(
                &f.encoded(&f.grant),
                f.authority.public(),
                &f.hc,
                &f.policy,
                "127.0.0.1:43002".parse().unwrap(),
                f.cap
            )
            .is_err()
        );
    }
    #[test]
    fn social_admission_binds_both_clients_and_independent_guest_expectation() {
        let f = Fixture::new();
        let encoded = f.encoded(&f.grant);
        let host = f.verify_host(&encoded).unwrap();
        let guest = VerifiedSocialGrant::for_guest(
            &encoded,
            f.authority.public(),
            &f.gc,
            &f.expected,
            f.cap,
        )
        .unwrap();
        assert_eq!(host.host_key(), f.host.public());
        assert_eq!(guest.guest_key(), f.guest.public());
        for (field, replacement) in [
            ("hostAccountId", json!("other")),
            (
                "hostPublicKey",
                json!(crate::generate_key().public().to_string()),
            ),
            (
                "hostOperationId",
                json!("ed35a732-2839-4a18-9740-73ed5ecff11c"),
            ),
            ("hostGeneration", json!(2)),
            ("policyId", json!("66".repeat(32))),
            ("policyDigest", json!("77".repeat(32))),
            ("sessionDigest", json!("88".repeat(32))),
        ] {
            let mut value = serde_json::to_value(&f.expected).unwrap();
            value[field] = replacement;
            let expected = SocialGuestExpectation::parse(&value.to_string()).unwrap();
            assert!(
                VerifiedSocialGrant::for_guest(
                    &encoded,
                    f.authority.public(),
                    &f.gc,
                    &expected,
                    f.cap
                )
                .is_err(),
                "{field}"
            );
        }
        assert!(
            VerifiedSocialGrant::for_guest(
                &encoded,
                f.authority.public(),
                &f.hc,
                &f.expected,
                f.cap
            )
            .is_err()
        );
        assert!(
            VerifiedSocialGrant::for_host(
                &encoded,
                crate::generate_key().public(),
                &f.hc,
                &f.policy,
                "127.0.0.1:43001".parse().unwrap(),
                f.cap
            )
            .is_err()
        );
    }
    #[test]
    fn social_admission_all_signed_fields_and_trusted_context_changes_are_rejected() {
        let f = Fixture::new();
        let signed: Value = serde_json::from_str(&f.encoded(&f.grant)).unwrap();
        for (field, replacement) in [
            ("issuerInstance", json!("99".repeat(32))),
            ("grantId", json!("99".repeat(32))),
            ("policyId", json!("99".repeat(32))),
            ("policyDigest", json!("99".repeat(32))),
            ("sessionId", json!("ed35a732-2839-4a18-9740-73ed5ecff11c")),
            ("sessionDigest", json!("99".repeat(32))),
            ("hostAccountId", json!("other")),
            (
                "hostPublicKey",
                json!(crate::generate_key().public().to_string()),
            ),
            (
                "hostOperationId",
                json!("ed35a732-2839-4a18-9740-73ed5ecff11c"),
            ),
            ("hostGeneration", json!(2)),
            ("guestAccountId", json!("other")),
            (
                "guestPublicKey",
                json!(crate::generate_key().public().to_string()),
            ),
            (
                "guestOperationId",
                json!("ed35a732-2839-4a18-9740-73ed5ecff11c"),
            ),
            ("guestGeneration", json!(2)),
            ("issuedAtUnixMs", json!(f.grant.issued_at_unix_ms - 1)),
            (
                "admissionExpiresAtUnixMs",
                json!(f.grant.admission_expires_at_unix_ms - 1),
            ),
            (
                "sessionExpiresAtUnixMs",
                json!(f.grant.session_expires_at_unix_ms - 1),
            ),
        ] {
            let mut changed = signed.clone();
            changed["body"][field] = replacement;
            assert!(f.verify_host(&changed.to_string()).is_err(), "{field}");
        }
        let mut wrong = f.grant.clone();
        wrong.host_generation = 2;
        assert!(f.verify_host(&f.encoded(&wrong)).is_err());
        let mut wrong = f.grant.clone();
        wrong.guest_account_id = "other".into();
        assert!(
            VerifiedSocialGrant::for_guest(
                &f.encoded(&wrong),
                f.authority.public(),
                &f.gc,
                &f.expected,
                f.cap
            )
            .is_err()
        );
        let mut wrong = signed;
        wrong["signature"] = json!(hex(&f
            .authority
            .sign(b"aurora/connect/social-key-proof/v1\0")
            .to_bytes()));
        assert!(f.verify_host(&wrong.to_string()).is_err());
    }
    #[test]
    fn social_admission_monotonic_preparation_caps_clones_and_closed_schema() {
        let f = Fixture::new();
        let encoded = f.encoded(&f.grant);
        assert!(
            VerifiedSocialGrant::for_host(
                &encoded,
                f.authority.public(),
                &f.hc,
                &f.policy,
                "127.0.0.1:43001".parse().unwrap(),
                Instant::now()
            )
            .is_err()
        );
        let mut grant = f.verified();
        grant.session_deadline = Instant::now();
        assert_eq!(
            grant.clone().check_session(),
            Err(SocialAdmissionError::Expired)
        );
        let mut raw: Value = serde_json::from_str(&encoded).unwrap();
        raw["extra"] = json!(true);
        assert!(f.verify_host(&raw.to_string()).is_err());
        for (field, replacement) in [
            ("schemaVersion", json!(2)),
            ("purpose", json!("keyBinding")),
            ("hostGeneration", json!(0)),
            ("guestGeneration", json!(SAFE + 1)),
            ("issuedAtUnixMs", json!(0)),
            (
                "admissionExpiresAtUnixMs",
                json!(f.grant.issued_at_unix_ms + 10_001),
            ),
        ] {
            let mut raw = serde_json::to_value(&f.grant).unwrap();
            raw[field] = replacement;
            let wrong: GrantBody = serde_json::from_value(raw).unwrap();
            assert!(f.verify_host(&f.encoded(&wrong)).is_err(), "{field}");
        }
        assert!(
            f.verify_host(&" ".repeat(MAX_SOCIAL_ADMISSION_BYTES + 1))
                .is_err()
        );
    }
    #[test]
    fn social_admission_continuity_increases_sequence_without_renewing_session() {
        let f = Fixture::new();
        let grant = f.verified();
        let session_end = grant.session_deadline;
        let mut owner = SocialContinuity::start(&f.lease(1, unix_millis() + 2000), &grant).unwrap();
        owner.update(&f.lease(2, unix_millis() + 2500)).unwrap();
        owner.check().unwrap();
        assert_eq!(owner.grant.session_deadline, session_end);
        assert_eq!(owner.sequence(), 2);
        assert_eq!(
            owner.update(&f.lease(2, unix_millis() + 2500)),
            Err(SocialAdmissionError::Sequence)
        );
        assert!(owner.update(&f.lease(3, unix_millis() + 2500)).is_err());
        assert!(owner.check().is_err());
        assert!(SocialContinuity::start(&f.lease(2, unix_millis() + 2000), &grant).is_err());
    }
    #[test]
    fn social_admission_continuity_expiry_context_signature_and_bound_are_terminal() {
        let f = Fixture::new();
        let grant = f.verified();
        for mutate in [0, 1, 2, 3, 4] {
            let mut owner =
                SocialContinuity::start(&f.lease(1, unix_millis() + 2000), &grant).unwrap();
            let mut raw: LeaseEnvelope =
                serde_json::from_str(&f.lease(2, unix_millis() + 2000)).unwrap();
            match mutate {
                0 => raw.body.grant_id = "99".repeat(32),
                1 => raw.body.policy_digest = "99".repeat(32),
                2 => raw.body.issuer_instance = "99".repeat(32),
                3 => raw.body.expires_at_unix_ms = raw.body.issued_at_unix_ms + 3001,
                _ => raw.body.expires_at_unix_ms = raw.body.issued_at_unix_ms,
            }
            raw.signature = hex(&f.authority.sign(&raw.body.bytes()).to_bytes());
            assert!(owner.update(&serde_json::to_string(&raw).unwrap()).is_err());
            assert!(owner.update(&f.lease(3, unix_millis() + 2000)).is_err());
        }
        let mut owner = SocialContinuity::start(&f.lease(1, unix_millis() + 2000), &grant).unwrap();
        owner.deadline = Instant::now();
        assert!(owner.update(&f.lease(2, unix_millis() + 2000)).is_err());
        let mut short = grant.clone();
        short.body.session_expires_at_unix_ms = unix_millis() + 500;
        assert!(SocialContinuity::start(&f.lease(1, unix_millis() + 1000), &short).is_err());
    }
}
