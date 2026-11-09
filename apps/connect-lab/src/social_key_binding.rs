//! Typed proof of possession for an Aurora account/key binding challenge.
//!
//! This is not a transport invitation, admission, or account authentication.
//! A verifier must independently authenticate the account and consume its own
//! original challenge once. No caller-selected bytes can be signed by this API.
use crate::{EndpointId, OnlineClock, SecretKey, unix_millis};
use iroh::Signature;
use serde::{Deserialize, Serialize};

pub const MAX_SOCIAL_KEY_CHALLENGE_BYTES: usize = 2048;
pub const MAX_SOCIAL_KEY_PROOF_BYTES: usize = 2304;
pub const MAX_SOCIAL_KEY_CHALLENGE_MILLIS: u64 = 60_000;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const DOMAIN: &[u8] = b"aurora/connect/social-key-proof/v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SocialKeyBindingError {
    #[error("social_key_invalid_challenge")]
    InvalidChallenge,
    #[error("social_key_challenge_expired")]
    Expired,
    #[error("social_key_mismatch")]
    KeyMismatch,
    #[error("social_key_invalid_signature")]
    InvalidSignature,
}
type Result<T> = std::result::Result<T, SocialKeyBindingError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SocialKeyRole {
    Host,
    Guest,
}
impl SocialKeyRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Guest => "guest",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SocialKeyChallenge {
    schema_version: u8,
    purpose: String,
    issuer_instance: String,
    challenge_id: String,
    account_id: String,
    operation_id: String,
    generation: u64,
    role: SocialKeyRole,
    public_key: String,
    session_id: String,
    session_digest: String,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
}
impl SocialKeyChallenge {
    pub fn parse(encoded: &str) -> Result<Self> {
        Self::parse_at(encoded, unix_millis())
    }
    pub fn parse_with_clock(encoded: &str, clock: &OnlineClock) -> Result<Self> {
        let challenge = Self::parse_at(encoded, clock.upper_now())?;
        challenge.validate_with_clock(clock)?;
        Ok(challenge)
    }
    fn parse_at(encoded: &str, now: u64) -> Result<Self> {
        if encoded.is_empty() || encoded.len() > MAX_SOCIAL_KEY_CHALLENGE_BYTES {
            return Err(SocialKeyBindingError::InvalidChallenge);
        }
        let value: Self =
            serde_json::from_str(encoded).map_err(|_| SocialKeyBindingError::InvalidChallenge)?;
        value.validate_at(now)?;
        Ok(value)
    }
    /// Rechecks the original absolute timestamps. It never grants extra time.
    pub fn validate(&self) -> Result<()> {
        self.validate_at(unix_millis())
    }
    pub fn validate_with_clock(&self, clock: &OnlineClock) -> Result<()> {
        if !clock.matches_attempt(
            &self.issuer_instance,
            &self.account_id,
            &self.session_id,
            &self.operation_id,
            self.generation,
        ) {
            return Err(SocialKeyBindingError::InvalidChallenge);
        }
        clock
            .deadline_for(self.expires_at_unix_ms)
            .map_err(|_| SocialKeyBindingError::Expired)?;
        self.validate_at(clock.upper_now())
    }
    fn validate_at(&self, now: u64) -> Result<()> {
        if self.schema_version != 1
            || self.purpose != "accountKeyBinding"
            || !lower_hex(&self.issuer_instance, 64)
            || !lower_hex(&self.challenge_id, 64)
            || self.account_id.is_empty()
            || self.account_id.len() > 128
            || !self
                .account_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || !uuid_v4(&self.operation_id)
            || self.generation == 0
            || self.generation > MAX_SAFE_INTEGER
            || !lower_hex(&self.public_key, 64)
            || self.public_key.parse::<EndpointId>().is_err()
            || !uuid_v4(&self.session_id)
            || !lower_hex(&self.session_digest, 64)
            || self.issued_at_unix_ms == 0
            || self.issued_at_unix_ms > MAX_SAFE_INTEGER
            || self.expires_at_unix_ms > MAX_SAFE_INTEGER
            || self.expires_at_unix_ms <= self.issued_at_unix_ms
            || self.expires_at_unix_ms - self.issued_at_unix_ms > MAX_SOCIAL_KEY_CHALLENGE_MILLIS
        {
            return Err(SocialKeyBindingError::InvalidChallenge);
        }
        if now < self.issued_at_unix_ms || now >= self.expires_at_unix_ms {
            return Err(SocialKeyBindingError::Expired);
        }
        Ok(())
    }
    pub fn sign(&self, key: &SecretKey) -> Result<SocialKeyBindingProof> {
        self.sign_at(key, unix_millis())
    }
    pub fn sign_with_clock(
        &self,
        key: &SecretKey,
        clock: &OnlineClock,
    ) -> Result<SocialKeyBindingProof> {
        self.validate_with_clock(clock)?;
        self.sign_at(key, clock.upper_now())
    }
    fn sign_at(&self, key: &SecretKey, now: u64) -> Result<SocialKeyBindingProof> {
        self.validate_at(now)?;
        if key.public().to_string() != self.public_key {
            return Err(SocialKeyBindingError::KeyMismatch);
        }
        Ok(SocialKeyBindingProof {
            schema_version: 1,
            challenge: self.clone(),
            signature: hex(&key.sign(&self.preimage()).to_bytes()),
        })
    }
    // Fixed order and length prefixes prevent ambiguity and cross-protocol use.
    // Only this closed, validated structure can reach SecretKey::sign.
    fn preimage(&self) -> Vec<u8> {
        let mut bytes = DOMAIN.to_vec();
        for value in [
            self.schema_version.to_string(),
            self.purpose.clone(),
            self.issuer_instance.clone(),
            self.challenge_id.clone(),
            self.account_id.clone(),
            self.operation_id.clone(),
            self.generation.to_string(),
            self.role.as_str().into(),
            self.public_key.clone(),
            self.session_id.clone(),
            self.session_digest.clone(),
            self.issued_at_unix_ms.to_string(),
            self.expires_at_unix_ms.to_string(),
        ] {
            bytes.extend_from_slice(&(value.len() as u16).to_be_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }
        bytes
    }
    pub fn schema_version(&self) -> u8 {
        self.schema_version
    }
    pub fn purpose(&self) -> &str {
        &self.purpose
    }
    pub fn issuer_instance(&self) -> &str {
        &self.issuer_instance
    }
    pub fn challenge_id(&self) -> &str {
        &self.challenge_id
    }
    pub fn account_id(&self) -> &str {
        &self.account_id
    }
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn role(&self) -> SocialKeyRole {
        self.role
    }
    pub fn public_key(&self) -> &str {
        &self.public_key
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn session_digest(&self) -> &str {
        &self.session_digest
    }
    pub fn issued_at_unix_ms(&self) -> u64 {
        self.issued_at_unix_ms
    }
    pub fn expires_at_unix_ms(&self) -> u64 {
        self.expires_at_unix_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SocialKeyBindingProof {
    schema_version: u8,
    challenge: SocialKeyChallenge,
    signature: String,
}
impl SocialKeyBindingProof {
    pub fn parse(encoded: &str) -> Result<Self> {
        if encoded.is_empty() || encoded.len() > MAX_SOCIAL_KEY_PROOF_BYTES {
            return Err(SocialKeyBindingError::InvalidChallenge);
        }
        let value: Self =
            serde_json::from_str(encoded).map_err(|_| SocialKeyBindingError::InvalidChallenge)?;
        value.verify()?;
        Ok(value)
    }
    /// Verifies possession only, not the account's identity or eligibility.
    pub fn verify(&self) -> Result<()> {
        self.verify_at(unix_millis())
    }
    fn verify_at(&self, now: u64) -> Result<()> {
        self.challenge.validate_at(now)?;
        if self.schema_version != 1 || !lower_hex(&self.signature, 128) {
            return Err(SocialKeyBindingError::InvalidSignature);
        }
        let key: EndpointId = self
            .challenge
            .public_key
            .parse()
            .map_err(|_| SocialKeyBindingError::InvalidChallenge)?;
        let mut decoded = [0; 64];
        for (i, pair) in self
            .signature
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .enumerate()
        {
            let nibble = |b| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
            decoded[i] = nibble(pair[0]) << 4 | nibble(pair[1]);
        }
        key.verify(&self.challenge.preimage(), &Signature::from_bytes(&decoded))
            .map_err(|_| SocialKeyBindingError::InvalidSignature)
    }
    pub fn challenge(&self) -> &SocialKeyChallenge {
        &self.challenge
    }
    pub fn signature(&self) -> &str {
        &self.signature
    }
    pub fn encode(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|_| SocialKeyBindingError::InvalidChallenge)
    }
}
fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn uuid_v4(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 36
        && b[14] == b'4'
        && matches!(b[19], b'8' | b'9' | b'a' | b'b')
        && b.iter().enumerate().all(|(i, &c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_digit() || (b'a'..=b'f').contains(&c)
            }
        })
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        value.push(DIGITS[(byte >> 4) as usize] as char);
        value.push(DIGITS[(byte & 15) as usize] as char);
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const NOW: u64 = 1_792_000_000_000;
    fn challenge(key: &SecretKey) -> SocialKeyChallenge {
        serde_json::from_value(json!({
            "schemaVersion":1,"purpose":"accountKeyBinding","issuerInstance":"11".repeat(32),
            "challengeId":"22".repeat(32),"accountId":"Aurora_test-1",
            "operationId":"8b414333-65f8-4524-a01a-d609a5168463","generation":3,"role":"host",
            "publicKey":key.public().to_string(),"sessionId":"a9e1bccc-3768-43bf-ab1c-36fc48e78e5a",
            "sessionDigest":"33".repeat(32),"issuedAtUnixMs":NOW,"expiresAtUnixMs":NOW+60_000
        }))
        .unwrap()
    }
    #[test]
    fn social_key_roundtrip_and_original_expiry() {
        let key = crate::generate_key();
        let original = challenge(&key);
        let proof = original.sign_at(&key, NOW).unwrap();
        proof.verify_at(NOW + 59_999).unwrap();
        assert_eq!(
            proof.clone().verify_at(NOW + 60_000),
            Err(SocialKeyBindingError::Expired)
        );
        assert_eq!(
            proof.verify_at(NOW - 1),
            Err(SocialKeyBindingError::Expired)
        );
        assert_eq!(
            original.sign_at(&crate::generate_key(), NOW),
            Err(SocialKeyBindingError::KeyMismatch)
        );
        assert_eq!(proof.challenge(), &original);
        assert_eq!(proof.signature().len(), 128);
        assert!(!proof.encode().unwrap().contains("secret"));
    }
    #[test]
    fn social_key_every_bound_field_and_domain_are_signed() {
        let key = crate::generate_key();
        let mut bound = challenge(&key);
        bound.expires_at_unix_ms = NOW + 59_000;
        let proof = bound.sign_at(&key, NOW).unwrap();
        for (field, replacement) in [
            ("issuerInstance", json!("44".repeat(32))),
            ("challengeId", json!("55".repeat(32))),
            ("accountId", json!("other")),
            ("operationId", json!("8b414333-65f8-4524-a01a-d609a5168464")),
            ("generation", json!(4)),
            ("role", json!("guest")),
            (
                "publicKey",
                json!(crate::generate_key().public().to_string()),
            ),
            ("sessionId", json!("a9e1bccc-3768-43bf-ab1c-36fc48e78e5b")),
            ("sessionDigest", json!("66".repeat(32))),
            ("issuedAtUnixMs", json!(NOW - 1)),
            ("expiresAtUnixMs", json!(NOW + 59_999)),
        ] {
            let mut value = serde_json::to_value(&proof).unwrap();
            value["challenge"][field] = replacement;
            let changed: SocialKeyBindingProof = serde_json::from_value(value).unwrap();
            assert_eq!(
                changed.verify_at(NOW),
                Err(SocialKeyBindingError::InvalidSignature),
                "{field}"
            );
        }
        let mut changed = proof.clone();
        changed.signature = hex(&key
            .sign(b"aurora/connect-lab/session-invitation/v2\0")
            .to_bytes());
        assert_eq!(
            changed.verify_at(NOW),
            Err(SocialKeyBindingError::InvalidSignature)
        );
    }
    #[test]
    fn social_key_closed_bounded_schema_and_safe_numbers() {
        let key = crate::generate_key();
        for (field, replacement) in [
            ("schemaVersion", json!(2)),
            ("purpose", json!("admission")),
            ("issuerInstance", json!("GG".repeat(32))),
            ("challengeId", json!("a")),
            ("accountId", json!("a/b")),
            ("operationId", json!("8b414333-65f8-3524-a01a-d609a5168463")),
            ("sessionId", json!("not-a-uuid")),
            ("generation", json!(0)),
            ("generation", json!(MAX_SAFE_INTEGER + 1)),
            ("expiresAtUnixMs", json!(NOW + 60_001)),
            ("issuedAtUnixMs", json!(0)),
            ("expiresAtUnixMs", json!(NOW)),
            ("sessionDigest", json!("ab")),
        ] {
            let mut value = serde_json::to_value(challenge(&key)).unwrap();
            value[field] = replacement;
            let changed: SocialKeyChallenge = serde_json::from_value(value).unwrap();
            assert_eq!(
                changed.validate_at(NOW),
                Err(SocialKeyBindingError::InvalidChallenge),
                "{field}"
            );
        }
        let mut unknown = serde_json::to_value(challenge(&key)).unwrap();
        unknown["payload"] = json!("not signable");
        assert!(serde_json::from_value::<SocialKeyChallenge>(unknown).is_err());
        assert!(
            SocialKeyChallenge::parse(&" ".repeat(MAX_SOCIAL_KEY_CHALLENGE_BYTES + 1)).is_err()
        );
        let mut fractional = serde_json::to_value(challenge(&key)).unwrap();
        fractional["generation"] = json!(1.5);
        assert!(serde_json::from_value::<SocialKeyChallenge>(fractional).is_err());
    }
    #[test]
    fn social_key_codec_has_fixed_order_and_length_prefixes() {
        let key = crate::generate_key();
        let value = challenge(&key);
        let expected = [
            "1",
            "accountKeyBinding",
            &"11".repeat(32),
            &"22".repeat(32),
            "Aurora_test-1",
            "8b414333-65f8-4524-a01a-d609a5168463",
            "3",
            "host",
            &key.public().to_string(),
            "a9e1bccc-3768-43bf-ab1c-36fc48e78e5a",
            &"33".repeat(32),
            "1792000000000",
            "1792000060000",
        ]
        .iter()
        .fold(DOMAIN.to_vec(), |mut bytes, field| {
            bytes.extend_from_slice(&(field.len() as u16).to_be_bytes());
            bytes.extend_from_slice(field.as_bytes());
            bytes
        });
        assert_eq!(value.preimage(), expected);
    }
}
