//! Signed social admission. These typed signatures are separate from key
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
const CONTEXT_DOMAIN: &str = "aurora/connect/social-join-context/v1\0";

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
fn voice_fields(
    schema: u8,
    profile: &Option<String>,
    digest: &Option<String>,
    host_player: Option<bool>,
) -> bool {
    if matches!(schema, 3 | 4) {
        matches!(profile.as_deref(), Some("svc" | "plasmo"))
            && digest.as_deref().is_some_and(is_hex)
            && host_player.is_some()
    } else {
        profile.is_none() && digest.is_none() && host_player.is_none()
    }
}
fn runtime_fields(schema: u8, profile: Option<&str>, voice: Option<&str>) -> bool {
    if schema != 4 {
        return profile.is_none();
    }
    matches!(
        profile,
        Some(
            "fabric-loader-0.16.14-1.21.1"
                | "fabric-loader-0.16.14-1.20.1"
                | "fabric-loader-0.16.14-1.19.2"
                | "fabric-loader-0.13.3-1.16.5"
                | "1.21.1-forge-52.1.12"
                | "1.20.1-forge-47.4.23"
                | "1.19.2-forge-43.4.16"
                | "1.16.5-forge-36.2.42"
                | "1.12.2-forge-14.23.5.2860"
        )
    ) && !(profile == Some("1.12.2-forge-14.23.5.2860") && voice == Some("plasmo"))
}
fn non_null_bool<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<bool>, D::Error> {
    bool::deserialize(deserializer).map(Some)
}
fn non_null_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null_string"
    )]
    voice_profile: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null_string"
    )]
    voice_source_digest: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null_bool"
    )]
    host_player: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null_string"
    )]
    runtime_profile: Option<String>,
}
impl SocialHostPolicy {
    pub fn review(
        challenge: &SocialKeyChallenge,
        visibility: &str,
        source: SocketAddr,
    ) -> Result<Self> {
        Self::review_with_join_policy(challenge, visibility, source, "open")
    }
    pub fn review_with_join_policy(
        challenge: &SocialKeyChallenge,
        visibility: &str,
        source: SocketAddr,
        join_policy: &str,
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
            join_policy: join_policy.into(),
            service_id: "game".into(),
            protocol: "tcp".into(),
            source_digest: social_source_digest(source)?,
            max_guests: 1,
            issued_at_unix_ms: challenge.issued_at_unix_ms(),
            expires_at_unix_ms: challenge.expires_at_unix_ms(),
            voice_profile: None,
            voice_source_digest: None,
            host_player: None,
            runtime_profile: None,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn review_online(
        challenge: &SocialKeyChallenge,
        visibility: &str,
        source: SocketAddr,
        join_policy: &str,
        session_expiry: u64,
    ) -> Result<Self> {
        let mut policy = Self::review_with_join_policy(challenge, visibility, source, join_policy)?;
        policy.schema_version = 2;
        policy.max_guests = 8;
        policy.expires_at_unix_ms = session_expiry;
        policy.validate()?;
        Ok(policy)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn review_online_voice(
        challenge: &SocialKeyChallenge,
        visibility: &str,
        source: SocketAddr,
        join_policy: &str,
        session_expiry: u64,
        voice_profile: &str,
        voice_source_digest: &str,
        host_player: bool,
    ) -> Result<Self> {
        let mut policy =
            Self::review_online(challenge, visibility, source, join_policy, session_expiry)?;
        policy.schema_version = 3;
        policy.voice_profile = Some(voice_profile.into());
        policy.voice_source_digest = Some(voice_source_digest.into());
        policy.host_player = Some(host_player);
        policy.max_guests = if host_player { 7 } else { 8 };
        policy.validate()?;
        Ok(policy)
    }
    /// Restrict the reviewed online capacity before its digest is approved.
    pub fn with_guest_limit(mut self, maximum: u8) -> Result<Self> {
        if self.schema_version == 1 || maximum == 0 || maximum > self.max_guests {
            return Err(SocialAdmissionError::Context);
        }
        self.max_guests = maximum;
        self.validate()?;
        Ok(self)
    }
    /// Upgrade an original native voice review with its closed, observed runtime.
    /// Existing schema-three bytes remain unchanged until this explicit operation.
    pub fn with_runtime_profile(mut self, runtime: &str) -> Result<Self> {
        if self.schema_version != 3 {
            return Err(SocialAdmissionError::Context);
        }
        self.schema_version = 4;
        self.runtime_profile = Some(runtime.into());
        self.validate()?;
        Ok(self)
    }
    pub fn max_guests(&self) -> u8 {
        self.max_guests
    }
    fn validate(&self) -> Result<()> {
        if !matches!(self.schema_version, 1..=4)
            || !runtime_fields(
                self.schema_version,
                self.runtime_profile.as_deref(),
                self.voice_profile.as_deref(),
            )
            || !voice_fields(
                self.schema_version,
                &self.voice_profile,
                &self.voice_source_digest,
                self.host_player,
            )
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
            || !matches!(self.join_policy.as_str(), "open" | "approval")
            || self.service_id != "game"
            || self.protocol != "tcp"
            || !is_hex(&self.source_digest)
            || if self.schema_version != 1 {
                !(1..=if self.host_player == Some(true) { 7 } else { 8 }).contains(&self.max_guests)
            } else {
                self.max_guests != 1
            }
            || !integer(self.issued_at_unix_ms)
            || !integer(self.expires_at_unix_ms)
            || self.expires_at_unix_ms <= self.issued_at_unix_ms
            || self.expires_at_unix_ms - self.issued_at_unix_ms
                > if self.schema_version != 1 {
                    14_400_000
                } else {
                    60_000
                }
        {
            return Err(SocialAdmissionError::Invalid);
        }
        Ok(())
    }
    fn bytes(&self) -> Vec<u8> {
        let mut fields = vec![
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
        ];
        if matches!(self.schema_version, 3 | 4) {
            fields.push(self.voice_profile.clone().expect("validated voice profile"));
            fields.push(
                self.voice_source_digest
                    .clone()
                    .expect("validated voice source"),
            );
            fields.push(self.host_player.expect("validated host player").to_string());
        }
        if self.schema_version == 4 {
            fields.push(
                self.runtime_profile
                    .clone()
                    .expect("validated runtime profile"),
            );
        }
        preimage(POLICY_DOMAIN, &fields)
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
    pub fn host_player(&self) -> bool {
        self.host_player == Some(true)
    }
    pub fn voice_profile(&self) -> Option<&str> {
        self.voice_profile.as_deref()
    }
    pub fn voice_source_digest(&self) -> Option<&str> {
        self.voice_source_digest.as_deref()
    }
    pub fn runtime_profile(&self) -> Option<&str> {
        self.runtime_profile.as_deref()
    }
    pub fn matches_host(
        &self,
        account: &str,
        operation: &str,
        generation: u64,
        key: EndpointId,
        session: &str,
        source: SocketAddr,
    ) -> bool {
        self.account_id == account
            && self.operation_id == operation
            && self.generation == generation
            && self.public_key == key.to_string()
            && self.session_id == session
            && social_source_digest(source).is_ok_and(|digest| digest == self.source_digest)
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

// Older fixture shape retained only by unit tests. No public raw-expectation
// constructor or transport acceptance API survives the authenticated context.
#[cfg(test)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SocialGuestExpectation {
    host_account_id: String,
    host_public_key: String,
    host_operation_id: String,
    host_generation: u64,
    policy_id: String,
    policy_digest: String,
    session_digest: String,
}
#[cfg(test)]
impl SocialGuestExpectation {
    fn parse(text: &str) -> Result<Self> {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JoinContextBody {
    schema_version: u8,
    purpose: String,
    issuer_instance: String,
    recipient_account_id: String,
    session_id: String,
    session_digest: String,
    host_account_id: String,
    host_public_key: String,
    host_operation_id: String,
    host_generation: u64,
    policy_id: String,
    policy_digest: String,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
}
impl JoinContextBody {
    fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.purpose != "socialJoinContext"
            || !is_hex(&self.issuer_instance)
            || !uid(&self.recipient_account_id)
            || !uuid(&self.session_id)
            || !is_hex(&self.session_digest)
            || !uid(&self.host_account_id)
            || self.recipient_account_id == self.host_account_id
            || !public(&self.host_public_key)
            || !uuid(&self.host_operation_id)
            || !integer(self.host_generation)
            || !is_hex(&self.policy_id)
            || !is_hex(&self.policy_digest)
            || !integer(self.issued_at_unix_ms)
            || !integer(self.expires_at_unix_ms)
            || self.expires_at_unix_ms <= self.issued_at_unix_ms
            || self.expires_at_unix_ms - self.issued_at_unix_ms > 10_000
        {
            return Err(SocialAdmissionError::Invalid);
        }
        Ok(())
    }
    fn bytes(&self) -> Vec<u8> {
        preimage(
            CONTEXT_DOMAIN,
            &[
                self.schema_version.to_string(),
                self.purpose.clone(),
                self.issuer_instance.clone(),
                self.recipient_account_id.clone(),
                self.session_id.clone(),
                self.session_digest.clone(),
                self.host_account_id.clone(),
                self.host_public_key.clone(),
                self.host_operation_id.clone(),
                self.host_generation.to_string(),
                self.policy_id.clone(),
                self.policy_digest.clone(),
                self.issued_at_unix_ms.to_string(),
                self.expires_at_unix_ms.to_string(),
            ],
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JoinContextEnvelope {
    body: JoinContextBody,
    signature: String,
}

/// Authenticated discovery only. Own account/session and the authority pin
/// must originate outside the received object. Clone keeps its original clock.
#[derive(Debug, Clone)]
pub struct VerifiedSocialJoinContext {
    body: JoinContextBody,
    pin: EndpointId,
    deadline: Instant,
}
impl VerifiedSocialJoinContext {
    pub fn verify(
        text: &str,
        pin: EndpointId,
        recipient_account_id: &str,
        session_id: &str,
        prepared_deadline: Instant,
    ) -> Result<Self> {
        let at = Instant::now();
        let now = unix_millis();
        if !uid(recipient_account_id) || !uuid(session_id) {
            return Err(SocialAdmissionError::Context);
        }
        let envelope: JoinContextEnvelope = parse(text)?;
        let body = envelope.body;
        body.validate()?;
        verify_signature(pin, &body.bytes(), &envelope.signature)?;
        if body.recipient_account_id != recipient_account_id || body.session_id != session_id {
            return Err(SocialAdmissionError::Context);
        }
        let deadline = deadline(at, now, body.expires_at_unix_ms, prepared_deadline)?;
        let value = Self {
            body,
            pin,
            deadline,
        };
        value.check_current()?;
        Ok(value)
    }
    pub fn check_current(&self) -> Result<()> {
        self.check_at(unix_millis(), Instant::now())
    }
    fn check_at(&self, wall: u64, monotonic: Instant) -> Result<()> {
        if wall < self.body.issued_at_unix_ms
            || wall >= self.body.expires_at_unix_ms
            || monotonic >= self.deadline
        {
            Err(SocialAdmissionError::Expired)
        } else {
            Ok(())
        }
    }
    pub fn check_challenge(&self, challenge: &SocialKeyChallenge) -> Result<()> {
        self.check_current()?;
        challenge
            .validate()
            .map_err(|_| SocialAdmissionError::Expired)?;
        if challenge.role() != SocialKeyRole::Guest
            || challenge.issuer_instance() != self.body.issuer_instance
            || challenge.account_id() != self.body.recipient_account_id
            || challenge.session_id() != self.body.session_id
            || challenge.session_digest() != self.body.session_digest
        {
            return Err(SocialAdmissionError::Context);
        }
        Ok(())
    }
    pub fn session_digest(&self) -> &str {
        &self.body.session_digest
    }
    /// Only preparation/ingress waits use this cap. It is never a replacement
    /// for the accepted grant's independent session deadline.
    pub fn ingress_deadline(&self) -> Instant {
        self.deadline
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null_string"
    )]
    voice_profile: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null_string"
    )]
    voice_source_digest: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null_bool"
    )]
    host_player: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "non_null_string"
    )]
    runtime_profile: Option<String>,
}
impl GrantBody {
    fn validate(&self) -> Result<()> {
        if !matches!(self.schema_version, 1..=4)
            || !runtime_fields(
                self.schema_version,
                self.runtime_profile.as_deref(),
                self.voice_profile.as_deref(),
            )
            || !voice_fields(
                self.schema_version,
                &self.voice_profile,
                &self.voice_source_digest,
                self.host_player,
            )
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
            || self.session_expires_at_unix_ms - self.issued_at_unix_ms
                > if self.schema_version != 1 {
                    14_400_000
                } else {
                    60_000
                }
        {
            return Err(SocialAdmissionError::Invalid);
        }
        Ok(())
    }
    fn bytes(&self) -> Vec<u8> {
        let mut fields = vec![
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
        ];
        if matches!(self.schema_version, 3 | 4) {
            fields.push(self.voice_profile.clone().expect("validated voice profile"));
            fields.push(
                self.voice_source_digest
                    .clone()
                    .expect("validated voice source"),
            );
            fields.push(self.host_player.expect("validated host player").to_string());
        }
        if self.schema_version == 4 {
            fields.push(
                self.runtime_profile
                    .clone()
                    .expect("validated runtime profile"),
            );
        }
        preimage(GRANT_DOMAIN, &fields)
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
        online: bool,
    ) -> Result<Self> {
        let at = Instant::now();
        let now = unix_millis();
        if !(online && challenge.role() == SocialKeyRole::Host) {
            challenge
                .validate()
                .map_err(|_| SocialAdmissionError::Expired)?;
        }
        let envelope: GrantEnvelope = parse(text)?;
        let b = envelope.body;
        b.validate()?;
        if (b.schema_version != 1) != online {
            return Err(SocialAdmissionError::Context);
        }
        verify_signature(pin, &b.bytes(), &envelope.signature)?;
        if b.issuer_instance != challenge.issuer_instance()
            || b.session_id != challenge.session_id()
            || b.session_digest != challenge.session_digest()
            || b.issued_at_unix_ms < challenge.issued_at_unix_ms()
            || (!online && b.session_expires_at_unix_ms > challenge.expires_at_unix_ms())
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
            session_deadline: deadline(
                at,
                now,
                b.session_expires_at_unix_ms,
                if online {
                    at + Duration::from_secs(14_400)
                } else {
                    prepared_deadline
                },
            )?,
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
        let value = Self::verified(
            text,
            pin,
            challenge,
            prepared_deadline,
            policy.body.schema_version != 1,
        )?;
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
            || (p.schema_version == 1 && p.expires_at_unix_ms != challenge.expires_at_unix_ms())
            || p.source_digest != social_source_digest(source)?
            || b.schema_version != p.schema_version
            || b.voice_profile != p.voice_profile
            || b.voice_source_digest != p.voice_source_digest
            || b.host_player != p.host_player
            || b.runtime_profile != p.runtime_profile
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
        context: &VerifiedSocialJoinContext,
        prepared_deadline: Instant,
    ) -> Result<Self> {
        Self::for_guest_mode(text, pin, challenge, context, prepared_deadline, false)
    }
    pub fn for_guest_online(
        text: &str,
        pin: EndpointId,
        challenge: &SocialKeyChallenge,
        context: &VerifiedSocialJoinContext,
        prepared_deadline: Instant,
    ) -> Result<Self> {
        Self::for_guest_mode(text, pin, challenge, context, prepared_deadline, true)
    }
    fn for_guest_mode(
        text: &str,
        pin: EndpointId,
        challenge: &SocialKeyChallenge,
        context: &VerifiedSocialJoinContext,
        prepared_deadline: Instant,
        online: bool,
    ) -> Result<Self> {
        context.check_challenge(challenge)?;
        if context.pin != pin {
            return Err(SocialAdmissionError::Context);
        }
        let expected = &context.body;
        let value = Self::verified(text, pin, challenge, prepared_deadline, online)?;
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
        context.check_current()?;
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
    pub fn host_player(&self) -> bool {
        self.body.host_player == Some(true)
    }
    pub fn voice_profile(&self) -> Option<&str> {
        self.body.voice_profile.as_deref()
    }
    pub fn voice_source_digest(&self) -> Option<&str> {
        self.body.voice_source_digest.as_deref()
    }
    pub fn runtime_profile(&self) -> Option<&str> {
        self.body.runtime_profile.as_deref()
    }
    pub fn session_deadline(&self) -> Instant {
        self.session_deadline
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
    pub fn guest_account_id(&self) -> &str {
        &self.body.guest_account_id
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
                voice_profile: None,
                voice_source_digest: None,
                host_player: None,
                runtime_profile: None,
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
        fn encoded_context(&self, body: &JoinContextBody) -> String {
            serde_json::to_string(&JoinContextEnvelope {
                body: body.clone(),
                signature: hex(&self.authority.sign(&body.bytes()).to_bytes()),
            })
            .unwrap()
        }
        fn context(&self, expected: &SocialGuestExpectation) -> VerifiedSocialJoinContext {
            let now = unix_millis();
            let body = JoinContextBody {
                schema_version: 1,
                purpose: "socialJoinContext".into(),
                issuer_instance: self.hc.issuer_instance().into(),
                recipient_account_id: self.gc.account_id().into(),
                session_id: self.hc.session_id().into(),
                session_digest: expected.session_digest.clone(),
                host_account_id: expected.host_account_id.clone(),
                host_public_key: expected.host_public_key.clone(),
                host_operation_id: expected.host_operation_id.clone(),
                host_generation: expected.host_generation,
                policy_id: expected.policy_id.clone(),
                policy_digest: expected.policy_digest.clone(),
                issued_at_unix_ms: now,
                expires_at_unix_ms: now + 9_000,
            };
            VerifiedSocialJoinContext::verify(
                &self.encoded_context(&body),
                self.authority.public(),
                self.gc.account_id(),
                self.hc.session_id(),
                self.cap,
            )
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
    fn social_online_voice_scope_is_signed_and_matches_original_host_policy() {
        let mut f = Fixture::new();
        let review = SocialHostPolicy::review_online_voice(
            &f.hc,
            "friends",
            "127.0.0.1:43001".parse().unwrap(),
            "open",
            f.hc.issued_at_unix_ms() + 300_000,
            "svc",
            &"77".repeat(32),
            true,
        )
        .unwrap();
        assert_eq!(review.max_guests, 7);
        assert!(review.bytes().ends_with(&preimage(
            "",
            &["svc".into(), "77".repeat(32), "true".into()]
        )));
        f.policy = review.approve(&f.host, &review.digest(), f.cap).unwrap();
        f.grant.schema_version = 3;
        f.grant.policy_digest = review.digest();
        f.grant.session_expires_at_unix_ms = review.expires_at_unix_ms();
        f.grant.voice_profile = Some("svc".into());
        f.grant.voice_source_digest = Some("77".repeat(32));
        f.grant.host_player = Some(true);
        f.expected.policy_digest = review.digest();
        let encoded = f.encoded(&f.grant);
        let verified = f.verify_host(&encoded).unwrap();
        assert_eq!(verified.voice_profile(), Some("svc"));
        assert!(verified.host_player());
        VerifiedSocialGrant::for_guest_online(
            &encoded,
            f.authority.public(),
            &f.gc,
            &f.context(&f.expected),
            f.cap,
        )
        .unwrap();
        assert!(
            VerifiedSocialGrant::for_guest(
                &encoded,
                f.authority.public(),
                &f.gc,
                &f.context(&f.expected),
                f.cap
            )
            .is_err()
        );
        for kind in 0..3 {
            let mut changed = f.grant.clone();
            match kind {
                0 => changed.voice_profile = Some("plasmo".into()),
                1 => changed.voice_source_digest = Some("88".repeat(32)),
                _ => changed.host_player = Some(false),
            }
            assert!(
                f.verify_host(&f.encoded(&changed)).is_err(),
                "original policy must match even with a valid authority signature"
            );
        }
        let mut legacy = serde_json::to_value(Fixture::new().grant).unwrap();
        legacy["voiceProfile"] = Value::Null;
        assert!(serde_json::from_value::<GrantBody>(legacy).is_err());
    }
    #[test]
    fn social_runtime_policy_matches_typescript_canonical_vector() {
        let policy: SocialHostPolicy = serde_json::from_value(json!({
            "schemaVersion":4,"purpose":"hostSocialSession","issuerInstance":"11".repeat(32),"policyId":"22".repeat(32),"accountId":"host",
            "operationId":"11111111-2222-4333-8444-555555555555","generation":7,"publicKey":"d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
            "sessionId":"22222222-2222-4333-8444-555555555555","sessionDigest":"33".repeat(32),"visibility":"friends","joinPolicy":"approval","serviceId":"game","protocol":"tcp",
            "sourceDigest":"44".repeat(32),"maxGuests":7,"issuedAtUnixMs":1800000000000u64,"expiresAtUnixMs":1800000300000u64,
            "voiceProfile":"svc","voiceSourceDigest":"55".repeat(32),"hostPlayer":true,"runtimeProfile":"1.12.2-forge-14.23.5.2860"
        })).unwrap();
        policy.validate().unwrap();
        assert_eq!(
            policy.digest(),
            "e8a05fd40d0d2eedcb8a5f4249e2ce46e6be7061f02ff1e534786b57ec4aa71b"
        );
    }
    #[test]
    fn social_runtime_profile_has_closed_version_four_codec_and_original_host_congruence() {
        let profiles = [
            "fabric-loader-0.16.14-1.21.1",
            "fabric-loader-0.16.14-1.20.1",
            "fabric-loader-0.16.14-1.19.2",
            "fabric-loader-0.13.3-1.16.5",
            "1.21.1-forge-52.1.12",
            "1.20.1-forge-47.4.23",
            "1.19.2-forge-43.4.16",
            "1.16.5-forge-36.2.42",
            "1.12.2-forge-14.23.5.2860",
        ];
        for runtime in profiles {
            let mut f = Fixture::new();
            let legacy = SocialHostPolicy::review_online_voice(
                &f.hc,
                "friends",
                "127.0.0.1:43001".parse().unwrap(),
                "open",
                f.hc.issued_at_unix_ms() + 300_000,
                "svc",
                &"77".repeat(32),
                false,
            )
            .unwrap();
            assert!(legacy.runtime_profile().is_none());
            let review = legacy.clone().with_runtime_profile(runtime).unwrap();
            assert_ne!(legacy.digest(), review.digest());
            assert_eq!(review.runtime_profile(), Some(runtime));
            assert!(review.bytes().ends_with(&preimage("", &[runtime.into()])));
            f.policy = review.approve(&f.host, &review.digest(), f.cap).unwrap();
            f.grant.schema_version = 4;
            f.grant.policy_digest = review.digest();
            f.expected.policy_digest = review.digest();
            f.grant.session_expires_at_unix_ms = review.expires_at_unix_ms();
            f.grant.voice_profile = Some("svc".into());
            f.grant.voice_source_digest = Some("77".repeat(32));
            f.grant.host_player = Some(false);
            f.grant.runtime_profile = Some(runtime.into());
            let encoded = f.encoded(&f.grant);
            assert_eq!(
                f.verify_host(&encoded).unwrap().runtime_profile(),
                Some(runtime)
            );
            assert_eq!(
                VerifiedSocialGrant::for_guest_online(
                    &encoded,
                    f.authority.public(),
                    &f.gc,
                    &f.context(&f.expected),
                    f.cap
                )
                .unwrap()
                .runtime_profile(),
                Some(runtime)
            );
            let mut changed = f.grant.clone();
            changed.runtime_profile = Some(
                if runtime == profiles[0] {
                    profiles[1]
                } else {
                    profiles[0]
                }
                .into(),
            );
            assert!(f.verify_host(&f.encoded(&changed)).is_err());
            changed.schema_version = 3;
            assert!(changed.validate().is_err());
            assert!(legacy.with_runtime_profile("unknown-runtime").is_err());
        }
        let f = Fixture::new();
        let plasmo = SocialHostPolicy::review_online_voice(
            &f.hc,
            "friends",
            "127.0.0.1:43001".parse().unwrap(),
            "open",
            f.hc.issued_at_unix_ms() + 300_000,
            "plasmo",
            &"77".repeat(32),
            false,
        )
        .unwrap();
        assert!(
            plasmo
                .with_runtime_profile("1.12.2-forge-14.23.5.2860")
                .is_err()
        );
        let mut legacy = serde_json::to_value(f.grant).unwrap();
        legacy["runtimeProfile"] = Value::Null;
        assert!(serde_json::from_value::<GrantBody>(legacy).is_err());
    }
    #[test]
    fn social_online_capacity_can_only_shrink_before_fresh_approval() {
        let f = Fixture::new();
        let original = SocialHostPolicy::review_online(
            &f.hc,
            "friends",
            "127.0.0.1:43001".parse().unwrap(),
            "open",
            f.hc.issued_at_unix_ms() + 300_000,
        )
        .unwrap();
        let limited = original.clone().with_guest_limit(2).unwrap();
        assert_eq!(limited.max_guests(), 2);
        assert_ne!(limited.digest(), original.digest());
        assert_eq!(
            limited
                .approve(&f.host, &original.digest(), f.cap)
                .unwrap_err(),
            SocialAdmissionError::Approval
        );
        assert!(limited.approve(&f.host, &limited.digest(), f.cap).is_ok());
        assert!(limited.with_guest_limit(3).is_err());
        assert!(original.clone().with_guest_limit(0).is_err());
        assert!(original.with_guest_limit(9).is_err());
        assert!(f.policy.body().clone().with_guest_limit(1).is_err());
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
            &f.context(&f.expected),
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
                    &f.context(&expected),
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
                &f.context(&f.expected),
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
                &f.context(&f.expected),
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

    #[test]
    fn social_join_context_requires_independent_pin_recipient_and_selected_session() {
        let f = Fixture::new();
        let context = f.context(&f.expected);
        let encoded = f.encoded_context(&context.body);
        let verify = |pin, account: &str, session: &str| {
            VerifiedSocialJoinContext::verify(&encoded, pin, account, session, f.cap)
        };
        verify(f.authority.public(), f.gc.account_id(), f.gc.session_id()).unwrap();
        assert!(
            verify(
                crate::generate_key().public(),
                f.gc.account_id(),
                f.gc.session_id()
            )
            .is_err()
        );
        assert!(verify(f.authority.public(), "other", f.gc.session_id()).is_err());
        assert!(
            verify(
                f.authority.public(),
                f.gc.account_id(),
                "ed35a732-2839-4a18-9740-73ed5ecff11c"
            )
            .is_err()
        );
        context.check_challenge(&f.gc).unwrap();
        assert!(context.check_challenge(&f.hc).is_err());
        for (field, value) in [
            ("issuerInstance", json!("55".repeat(32))),
            ("sessionDigest", json!("66".repeat(32))),
            ("accountId", json!("other")),
            ("sessionId", json!("ed35a732-2839-4a18-9740-73ed5ecff11c")),
        ] {
            let mut raw = serde_json::to_value(&f.gc).unwrap();
            raw[field] = value;
            let wrong = SocialKeyChallenge::parse(&raw.to_string()).unwrap();
            assert!(context.check_challenge(&wrong).is_err(), "{field}");
        }
    }
    #[test]
    fn social_join_context_all_fields_are_signed_and_schema_is_closed() {
        let f = Fixture::new();
        let context = f.context(&f.expected);
        let original: Value = serde_json::from_str(&f.encoded_context(&context.body)).unwrap();
        for (field, value) in [
            ("schemaVersion", json!(2)),
            ("purpose", json!("socialAdmission")),
            ("issuerInstance", json!("55".repeat(32))),
            ("recipientAccountId", json!("other")),
            ("sessionId", json!("ed35a732-2839-4a18-9740-73ed5ecff11c")),
            ("sessionDigest", json!("55".repeat(32))),
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
            ("policyId", json!("55".repeat(32))),
            ("policyDigest", json!("55".repeat(32))),
            ("issuedAtUnixMs", json!(context.body.issued_at_unix_ms - 1)),
            (
                "expiresAtUnixMs",
                json!(context.body.expires_at_unix_ms - 1),
            ),
        ] {
            let mut changed = original.clone();
            changed["body"][field] = value;
            assert!(
                VerifiedSocialJoinContext::verify(
                    &changed.to_string(),
                    f.authority.public(),
                    f.gc.account_id(),
                    f.gc.session_id(),
                    f.cap
                )
                .is_err(),
                "{field}"
            );
        }
        let mut extra = original.clone();
        extra["body"]["source"] = json!("127.0.0.1:43001");
        assert!(
            VerifiedSocialJoinContext::verify(
                &extra.to_string(),
                f.authority.public(),
                f.gc.account_id(),
                f.gc.session_id(),
                f.cap
            )
            .is_err()
        );
        let mut wrong = original;
        wrong["signature"] = json!(hex(&f
            .authority
            .sign(b"aurora/connect/social-admission/v1\0")
            .to_bytes()));
        assert!(
            VerifiedSocialJoinContext::verify(
                &wrong.to_string(),
                f.authority.public(),
                f.gc.account_id(),
                f.gc.session_id(),
                f.cap
            )
            .is_err()
        );
        assert!(
            VerifiedSocialJoinContext::verify(
                &" ".repeat(MAX_SOCIAL_ADMISSION_BYTES + 1),
                f.authority.public(),
                f.gc.account_id(),
                f.gc.session_id(),
                f.cap
            )
            .is_err()
        );
    }
    #[test]
    fn social_join_context_original_clocks_do_not_renew_or_shorten_an_admitted_session() {
        let f = Fixture::new();
        let context = f.context(&f.expected);
        assert_eq!(
            context.check_at(context.body.issued_at_unix_ms - 1, Instant::now()),
            Err(SocialAdmissionError::Expired)
        );
        assert_eq!(
            context.check_at(context.body.expires_at_unix_ms, Instant::now()),
            Err(SocialAdmissionError::Expired)
        );
        assert_eq!(
            context
                .clone()
                .check_at(context.body.issued_at_unix_ms, context.deadline),
            Err(SocialAdmissionError::Expired)
        );
        assert!(
            VerifiedSocialJoinContext::verify(
                &f.encoded_context(&context.body),
                f.authority.public(),
                f.gc.account_id(),
                f.gc.session_id(),
                Instant::now()
            )
            .is_err()
        );
        let grant = VerifiedSocialGrant::for_guest(
            &f.encoded(&f.grant),
            f.authority.public(),
            &f.gc,
            &context,
            f.cap,
        )
        .unwrap();
        assert!(grant.session_deadline > context.deadline);
        let mut expired = context.clone();
        expired.deadline = Instant::now();
        assert!(expired.check_current().is_err());
        assert!(
            VerifiedSocialGrant::for_guest(
                &f.encoded(&f.grant),
                f.authority.public(),
                &f.gc,
                &expired,
                f.cap
            )
            .is_err()
        );
        grant.check_session().unwrap(); // Discovery expiry never becomes a new session authority.
    }
    #[test]
    fn social_join_context_rejects_future_expired_and_out_of_bounds_signed_objects() {
        let f = Fixture::new();
        let context = f.context(&f.expected);
        for case in 0..6 {
            let mut body = context.body.clone();
            let now = unix_millis();
            match case {
                0 => {
                    body.issued_at_unix_ms = now + 1_000;
                    body.expires_at_unix_ms = now + 2_000;
                }
                1 => {
                    body.issued_at_unix_ms = now - 2_000;
                    body.expires_at_unix_ms = now - 1_000;
                }
                2 => body.expires_at_unix_ms = body.issued_at_unix_ms + 10_001,
                3 => body.host_account_id = body.recipient_account_id.clone(),
                4 => body.host_generation = SAFE + 1,
                _ => body.issued_at_unix_ms = 0,
            }
            assert!(
                VerifiedSocialJoinContext::verify(
                    &f.encoded_context(&body),
                    f.authority.public(),
                    f.gc.account_id(),
                    f.gc.session_id(),
                    f.cap
                )
                .is_err(),
                "{case}"
            );
        }
    }
}
