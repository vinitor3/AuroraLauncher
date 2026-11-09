//! Immutable civil time bounds from one signed, fresh HTTPS response.
//! The upper bound expires received authority early. The lower bound is used
//! when issuing deadlines that must fit within the server's remaining time.
use crate::{EndpointId, Error, Result};
use iroh::Signature;
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const DOMAIN: &[u8] = b"aurora/connect/social-clock/v1\0";
const MAX_UNCERTAINTY: Duration = Duration::from_secs(2);
const SAMPLE_RESOLUTION: Duration = Duration::from_millis(1);
const MAX_SESSION_MS: u64 = 14_400_000;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub struct OnlineClockBinding<'a> {
    pub issuer: &'a str,
    pub account: &'a str,
    pub session: &'a str,
    pub operation: &'a str,
    pub generation: u64,
    pub nonce: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OnlineClockBootstrapError {
    #[error("{0}")]
    Rejected(Error),
    #[error("expired_clock_session")]
    Expired,
    /// A valid signed response for a live session whose round trip leaves
    /// too much unobserved time. Its opaque restriction cannot serve as a clock.
    #[error("uncertain_clock_sample")]
    Uncertain {
        session_expiry: u64,
        restriction: OnlineClockBootstrapRestriction,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Body {
    schema_version: u8,
    issuer_instance: String,
    account_id: String,
    session_id: String,
    operation_id: String,
    generation: u64,
    nonce: String,
    server_received_at_unix_ms: u64,
    server_now_unix_ms: u64,
    session_expires_at_unix_ms: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    body: Body,
    signature: String,
}

#[derive(Debug)]
struct Anchor {
    body: Body,
    pin: EndpointId,
    sent: Instant,
    received: Instant,
    deadline: Instant,
    consistency: BootstrapConsistency,
}

/// Intersection of every authenticated sample seen before publication. Keep
/// nanosecond precision when rebasing so repeated selections cannot widen it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct BootstrapConsistency {
    observed: Instant,
    lower: Duration,
    upper: Duration,
}

/// Authenticated evidence that can only narrow another sample before it is
/// published. No public constructor, clock projection, or authority conversion.
#[derive(Clone, PartialEq, Eq)]
pub struct OnlineClockBootstrapRestriction(Box<BootstrapRestriction>);
impl std::fmt::Debug for OnlineClockBootstrapRestriction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthenticatedBootstrapRestriction")
            .finish_non_exhaustive()
    }
}
#[derive(Clone, PartialEq, Eq)]
struct BootstrapRestriction {
    body: Body,
    pin: EndpointId,
    sent: Instant,
    deadline: Instant,
    consistency: BootstrapConsistency,
}
impl OnlineClockBootstrapRestriction {
    pub fn check_current(&self) -> Result<()> {
        if Instant::now() >= self.0.deadline {
            Err(Error::Expired)
        } else {
            Ok(())
        }
    }

    pub fn merge(self, mut next: Self) -> Result<Self> {
        let now = Instant::now();
        if now >= self.0.deadline || now >= next.0.deadline {
            return Err(Error::Expired);
        }
        if !same_bootstrap_scope(&self.0.body, self.0.pin, &next.0.body, next.0.pin)
            || self.0.body.nonce == next.0.body.nonce
            || next.0.sent < self.0.sent
        {
            return Err(Error::Unauthorized);
        }
        next.0.consistency = self.0.consistency.intersect(&next.0.consistency, now)?;
        next.0.deadline = self.0.deadline.min(next.0.deadline);
        Ok(next)
    }

    pub fn constrain_unpublished(self, clock: OnlineClock) -> Result<OnlineClock> {
        clock.check_current()?;
        let now = Instant::now();
        if now >= self.0.deadline {
            return Err(Error::Expired);
        }
        if !same_bootstrap_scope(&self.0.body, self.0.pin, &clock.0.body, clock.0.pin)
            || self.0.body.nonce == clock.0.body.nonce
        {
            return Err(Error::Unauthorized);
        }
        let consistency = self.0.consistency.intersect(&clock.0.consistency, now)?;
        let mut anchor = Arc::try_unwrap(clock.0).map_err(|_| Error::Unauthorized)?;
        anchor.consistency = consistency;
        anchor.deadline = anchor.deadline.min(self.0.deadline);
        let clock = OnlineClock(Arc::new(anchor));
        clock.check_current()?;
        Ok(clock)
    }
}

fn same_bootstrap_scope(
    left: &Body,
    left_pin: EndpointId,
    right: &Body,
    right_pin: EndpointId,
) -> bool {
    left_pin == right_pin
        && left.issuer_instance == right.issuer_instance
        && left.account_id == right.account_id
        && left.session_id == right.session_id
        && left.operation_id == right.operation_id
        && left.generation == right.generation
        && left.session_expires_at_unix_ms == right.session_expires_at_unix_ms
}
impl BootstrapConsistency {
    fn intersect(&self, next: &Self, observed: Instant) -> Result<Self> {
        let elapsed = observed.saturating_duration_since(self.observed);
        let next_elapsed = observed.saturating_duration_since(next.observed);
        let lower = self
            .lower
            .saturating_add(elapsed)
            .max(next.lower.saturating_add(next_elapsed));
        let upper = self
            .upper
            .saturating_add(elapsed)
            .min(next.upper.saturating_add(next_elapsed));
        if lower > upper {
            return Err(Error::Unauthorized);
        }
        Ok(Self {
            observed,
            lower,
            upper,
        })
    }
}

/// A clone keeps the same signed scope, round trip and expiry.
#[derive(Clone, Debug)]
pub struct OnlineClock(Arc<Anchor>);

impl OnlineClock {
    pub const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(6);

    pub fn bootstrap_round_trip(&self) -> Duration {
        self.0.received.duration_since(self.0.sent)
    }

    pub fn bootstrap_uncertainty(&self) -> Duration {
        let processing = Duration::from_millis(
            self.0.body.server_now_unix_ms - self.0.body.server_received_at_unix_ms,
        )
        .saturating_sub(SAMPLE_RESOLUTION);
        self.bootstrap_round_trip().saturating_sub(processing)
    }

    /// Choose only before distributing the clock to policy/grant consumers.
    /// Both samples are consumed exclusively; the retained session cap can only
    /// shorten, even when a warmer sample improves future freshness checks.
    pub fn select_unpublished_bootstrap(self, next: Self) -> Result<Self> {
        self.check_current()?;
        next.check_current()?;
        let previous = &self.0.body;
        let candidate = &next.0.body;
        let now = Instant::now();
        if !same_bootstrap_scope(previous, self.0.pin, candidate, next.0.pin)
            || previous.nonce == candidate.nonce
            || next.0.sent < self.0.sent
        {
            return Err(Error::Unauthorized);
        }
        let consistency = self.0.consistency.intersect(&next.0.consistency, now)?;
        let prefer_next = next.bootstrap_uncertainty() < self.bootstrap_uncertainty();
        let previous = Arc::try_unwrap(self.0).map_err(|_| Error::Unauthorized)?;
        let candidate = Arc::try_unwrap(next.0).map_err(|_| Error::Unauthorized)?;
        let deadline = previous.deadline.min(candidate.deadline);
        let mut selected = if prefer_next { candidate } else { previous };
        selected.deadline = deadline;
        selected.consistency = consistency;
        let clock = Self(Arc::new(selected));
        clock.check_current()?;
        Ok(clock)
    }

    pub fn verify_bootstrap(
        encoded: &str,
        pin: EndpointId,
        binding: OnlineClockBinding<'_>,
        sent: Instant,
        received: Instant,
    ) -> Result<Self> {
        Self::verify_bootstrap_sample(encoded, pin, binding, sent, received).map_err(|error| {
            match error {
                OnlineClockBootstrapError::Rejected(error) => error,
                OnlineClockBootstrapError::Expired
                | OnlineClockBootstrapError::Uncertain { .. } => Error::Expired,
            }
        })
    }

    /// Only a fully authenticated sample can report retryable uncertainty.
    pub fn verify_bootstrap_sample(
        encoded: &str,
        pin: EndpointId,
        binding: OnlineClockBinding<'_>,
        sent: Instant,
        received: Instant,
    ) -> std::result::Result<Self, OnlineClockBootstrapError> {
        use OnlineClockBootstrapError::{Expired, Rejected, Uncertain};
        let now = Instant::now();
        let round_trip = received
            .checked_duration_since(sent)
            .filter(|duration| *duration <= Self::BOOTSTRAP_TIMEOUT)
            .ok_or(Rejected(Error::Expired))?;
        if encoded.is_empty()
            || encoded.len() > 2048
            || !encoded.is_ascii()
            || received > now
            || now.saturating_duration_since(received) > MAX_UNCERTAINTY
        {
            return Err(Rejected(Error::Expired));
        }
        let shape: serde_json::Value =
            serde_json::from_str(encoded).map_err(|_| Rejected(Error::Protocol))?;
        if !shape.is_object() || !shape["body"].is_object() {
            return Err(Rejected(Error::Protocol));
        }
        // Decode the original text again so duplicate struct fields still fail.
        let envelope: Envelope =
            serde_json::from_str(encoded).map_err(|_| Rejected(Error::Protocol))?;
        let body = envelope.body;
        if body.schema_version != 1
            || !lower_hex(&body.issuer_instance, 64)
            || !lower_hex(&body.nonce, 32)
            || body.issuer_instance != binding.issuer
            || body.account_id != binding.account
            || body.session_id != binding.session
            || body.operation_id != binding.operation
            || body.generation != binding.generation
            || body.generation == 0
            || body.generation > MAX_SAFE_INTEGER
            || body.nonce != binding.nonce
            || body.account_id.is_empty()
            || body.account_id.len() > 128
            || !body
                .account_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || !uuid_v4(&body.session_id)
            || !uuid_v4(&body.operation_id)
            || body.server_received_at_unix_ms == 0
            || body.session_expires_at_unix_ms > MAX_SAFE_INTEGER
        {
            return Err(Rejected(Error::Unauthorized));
        }
        let signature = signature(&envelope.signature).map_err(Rejected)?;
        let mut material = DOMAIN.to_vec();
        material.extend(serde_json::to_vec(&body).map_err(|_| Rejected(Error::Protocol))?);
        pin.verify(&material, &signature)
            .map_err(|_| Rejected(Error::Unauthorized))?;
        if body.session_expires_at_unix_ms <= body.server_now_unix_ms {
            return Err(Expired);
        }
        if body.session_expires_at_unix_ms - body.server_now_unix_ms > MAX_SESSION_MS {
            return Err(Rejected(Error::Expired));
        }
        let server_elapsed = body
            .server_now_unix_ms
            .checked_sub(body.server_received_at_unix_ms)
            .map(Duration::from_millis)
            .filter(|elapsed| *elapsed <= round_trip)
            .ok_or(Rejected(Error::Expired))?;
        let deadline = project_deadline(
            sent,
            body.server_received_at_unix_ms,
            body.session_expires_at_unix_ms,
        )
        .map_err(|_| Expired)?;
        if Instant::now() >= deadline {
            return Err(Expired);
        }
        // The server's signed samples bracket its work in one authority. The
        // unobserved remainder includes both network directions and rounding.
        let consistency = BootstrapConsistency {
            observed: received,
            lower: Duration::from_millis(body.server_now_unix_ms),
            upper: Duration::from_millis(body.server_received_at_unix_ms + 1)
                .saturating_add(round_trip),
        };
        let processing_credit = server_elapsed.saturating_sub(SAMPLE_RESOLUTION);
        if round_trip - processing_credit > MAX_UNCERTAINTY {
            return Err(Uncertain {
                session_expiry: body.session_expires_at_unix_ms,
                restriction: OnlineClockBootstrapRestriction(Box::new(BootstrapRestriction {
                    body,
                    pin,
                    sent,
                    deadline,
                    consistency,
                })),
            });
        }
        let clock = Self(Arc::new(Anchor {
            body,
            pin,
            sent,
            received,
            deadline,
            consistency,
        }));
        clock.check_current().map_err(|_| Expired)?;
        Ok(clock)
    }

    pub fn check_current(&self) -> Result<()> {
        if Instant::now() >= self.0.deadline {
            return Err(Error::Expired);
        }
        Ok(())
    }

    pub fn upper_now(&self) -> u64 {
        self.upper_at(Instant::now())
    }

    pub fn lower_now(&self) -> u64 {
        self.lower_at(Instant::now())
    }

    pub fn session_expiry(&self) -> u64 {
        self.0.body.session_expires_at_unix_ms
    }

    pub fn deadline_for(&self, expiry: u64) -> Result<Instant> {
        if expiry > self.session_expiry() {
            return Err(Error::Unauthorized);
        }
        let deadline =
            project_deadline(self.0.sent, self.0.body.server_received_at_unix_ms, expiry)?
                .min(self.0.deadline);
        if Instant::now() >= deadline {
            return Err(Error::Expired);
        }
        Ok(deadline)
    }

    pub(crate) fn session_matches(&self, session: &str) -> bool {
        self.0.body.session_id == session
    }

    pub(crate) fn matches_recipient(
        &self,
        pin: EndpointId,
        issuer: &str,
        account: &str,
        session: &str,
    ) -> bool {
        self.0.pin == pin
            && self.0.body.issuer_instance == issuer
            && self.0.body.account_id == account
            && self.session_matches(session)
    }

    pub(crate) fn matches_attempt(
        &self,
        issuer: &str,
        account: &str,
        session: &str,
        operation: &str,
        generation: u64,
    ) -> bool {
        self.matches_recipient(self.0.pin, issuer, account, session)
            && self.0.body.operation_id == operation
            && self.0.body.generation == generation
    }

    pub(crate) fn generation(&self) -> u64 {
        self.0.body.generation
    }

    pub(crate) fn same_scope(left: Option<&Self>, right: Option<&Self>) -> bool {
        match (left, right) {
            (Some(left), Some(right)) => Arc::ptr_eq(&left.0, &right.0),
            (None, None) => true,
            _ => false,
        }
    }

    pub(crate) fn observe(clock: Option<&Self>) -> (Instant, u64) {
        let now = Instant::now();
        let wall = clock.map_or_else(crate::unix_millis, |clock| clock.upper_at(now));
        (now, wall)
    }

    fn upper_at(&self, now: Instant) -> u64 {
        // The rounded server receive timestamp cannot predate client send.
        project_time(self.0.body.server_received_at_unix_ms + 1, self.0.sent, now)
    }

    fn lower_at(&self, now: Instant) -> u64 {
        // The signed response timestamp cannot postdate client receipt.
        project_time(self.0.body.server_now_unix_ms, self.0.received, now)
    }
}

fn project_time(base: u64, origin: Instant, now: Instant) -> u64 {
    base.saturating_add(
        now.saturating_duration_since(origin)
            .as_millis()
            .min(u64::MAX as u128) as u64,
    )
}

fn project_deadline(sent: Instant, server_received: u64, expiry: u64) -> Result<Instant> {
    let duration = expiry
        .checked_sub(server_received + 1)
        .ok_or(Error::Expired)?;
    sent.checked_add(Duration::from_millis(duration))
        .ok_or(Error::Expired)
}

fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn uuid_v4(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && bytes[14] == b'4'
        && b"89ab".contains(&bytes[19])
        && bytes.iter().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)
            }
        })
}

fn signature(value: &str) -> Result<Signature> {
    if !lower_hex(value, 128) {
        return Err(Error::Unauthorized);
    }
    let mut bytes = [0; 64];
    let nibble = |byte| {
        if byte <= b'9' {
            byte - b'0'
        } else {
            byte - b'a' + 10
        }
    };
    let (pairs, _) = value.as_bytes().as_chunks::<2>();
    for (destination, &[high, low]) in bytes.iter_mut().zip(pairs) {
        *destination = (nibble(high) << 4) | nibble(low);
    }
    Ok(Signature::from_bytes(&bytes))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{SessionDeadlines, SessionInvitation, SocialKeyChallenge, generate_key};
    use serde_json::json;

    const ISSUER: &str = "abababababababababababababababababababababababababababababababab";
    const SESSION: &str = "26bc2f10-2af8-4da6-88d2-4524a0061bc0";
    const OPERATION: &str = "095d4404-8ce1-4b3f-bb12-7881d724bd72";
    const OTHER_OPERATION: &str = "a5f20a41-76a3-4a21-b22e-8af275ac0901";
    const NONCE: &str = "0123456789abcdef0123456789abcdef";
    const ACCOUNT: &str = "test_account";

    fn binding() -> OnlineClockBinding<'static> {
        OnlineClockBinding {
            issuer: ISSUER,
            account: ACCOUNT,
            session: SESSION,
            operation: OPERATION,
            generation: 3,
            nonce: NONCE,
        }
    }

    fn signed(key: &crate::SecretKey, server_now: u64, expires: u64) -> String {
        signed_for(key, &binding(), server_now, server_now, expires)
    }

    pub(crate) fn signed_for(
        key: &crate::SecretKey,
        binding: &OnlineClockBinding<'_>,
        server_received: u64,
        server_now: u64,
        expires: u64,
    ) -> String {
        let body = Body {
            schema_version: 1,
            issuer_instance: binding.issuer.into(),
            account_id: binding.account.into(),
            session_id: binding.session.into(),
            operation_id: binding.operation.into(),
            generation: binding.generation,
            nonce: binding.nonce.into(),
            server_received_at_unix_ms: server_received,
            server_now_unix_ms: server_now,
            session_expires_at_unix_ms: expires,
        };
        let mut material = DOMAIN.to_vec();
        material.extend(serde_json::to_vec(&body).unwrap());
        let signature = key
            .sign(&material)
            .to_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        serde_json::to_string(&Envelope { body, signature }).unwrap()
    }

    #[test]
    fn authenticated_clock_verifies_worker_canonical_golden_signature() {
        let pin = "2152f8d19b791d24453242e15f2eab6cb7cffa7b6a5ed30097960e069881db12"
            .parse()
            .unwrap();
        let signature = concat!(
            "2534fb8a5a76e970d61abf4762cab7f65179df3175cf0fa26a95c290968e1f547",
            "0bff02a31bda3666e1676fe7298fc0d6402cbb8e981ff41144524770216f300",
        );
        let proof = json!({
            "body": {"schemaVersion": 1, "issuerInstance": ISSUER, "accountId": ACCOUNT,
                "sessionId": SESSION, "operationId": OPERATION, "generation": 3,
                "nonce": NONCE, "serverReceivedAtUnixMs": 1_799_999_999_500_u64,
                "serverNowUnixMs": 1_800_000_000_000_u64,
                "sessionExpiresAtUnixMs": 1_800_000_020_000_u64 },
            "signature": signature,
        });
        let now = Instant::now();
        let clock = OnlineClock::verify_bootstrap(
            &proof.to_string(),
            pin,
            binding(),
            now - Duration::from_millis(600),
            now,
        )
        .unwrap();
        assert_eq!(clock.session_expiry(), 1_800_000_020_000);
    }

    #[test]
    fn authenticated_clock_uses_monotonic_bounds_with_wall_skew_in_both_directions() {
        let authority = generate_key();
        let wall = crate::unix_millis();
        for server_now in [wall - 120_000, wall + 120_000] {
            let received = Instant::now();
            let sent = received - Duration::from_millis(300);
            let proof = signed(&authority, server_now, server_now + 20_000);
            let clock = OnlineClock::verify_bootstrap(
                &proof,
                authority.public(),
                binding(),
                sent,
                received,
            )
            .unwrap();
            let later = received + Duration::from_secs(4);
            assert_eq!(clock.upper_at(later), server_now + 4_301);
            assert_eq!(clock.lower_at(later), server_now + 4_000);
            assert_eq!(
                clock.deadline_for(server_now + 3_000).unwrap(),
                sent + Duration::from_millis(2_999)
            );
            assert!(clock.deadline_for(server_now + 20_001).is_err());
            assert!(clock.deadline_for(server_now + 100).is_err());
            assert!(OnlineClock::same_scope(Some(&clock), Some(&clock.clone())));
            let reconstructed = OnlineClock::verify_bootstrap(
                &proof,
                authority.public(),
                binding(),
                sent,
                received,
            )
            .unwrap();
            assert!(!OnlineClock::same_scope(Some(&clock), Some(&reconstructed)));
        }
    }

    #[test]
    fn authenticated_clock_credits_only_bounded_signed_server_processing() {
        let authority = generate_key();
        let received = Instant::now();
        let server_received = crate::unix_millis();
        let expiry = server_received + 30_000;
        let verify = |server_elapsed: i64, round_trip_ms| {
            let server_now = server_received.checked_add_signed(server_elapsed).unwrap();
            let proof = signed_for(&authority, &binding(), server_received, server_now, expiry);
            OnlineClock::verify_bootstrap(
                &proof,
                authority.public(),
                binding(),
                received - Duration::from_millis(round_trip_ms),
                received,
            )
        };
        let clock = verify(2_450, 2_724).unwrap();
        assert_eq!(clock.upper_at(received), server_received + 2_725);
        assert_eq!(clock.lower_at(received), server_received + 2_450);
        assert_eq!(
            clock.deadline_for(expiry).unwrap(),
            received + Duration::from_millis(27_275)
        );
        assert!(verify(2_450, 4_449).is_ok());
        assert!(verify(2_450, 4_450).is_err());
        assert!(verify(2_725, 2_724).is_err());
        assert!(verify(-1, 100).is_err());
        assert!(verify(5_000, 6_001).is_err());
    }

    #[test]
    fn bootstrap_retry_requires_signed_uncertainty_and_unexpired_authority() {
        use OnlineClockBootstrapError::{Expired, Rejected, Uncertain};
        let authority = generate_key();
        let received = Instant::now();
        let sent = received - Duration::from_millis(2_100);
        let server_now = crate::unix_millis();
        let session_expiry = server_now + 20_000;
        let verify = |proof: &str| {
            OnlineClock::verify_bootstrap_sample(
                proof,
                authority.public(),
                binding(),
                sent,
                received,
            )
            .err()
        };
        let proof = signed(&authority, server_now, session_expiry);
        assert!(
            matches!(verify(&proof), Some(Uncertain { session_expiry: expiry, .. }) if expiry == session_expiry)
        );
        let mut damaged: serde_json::Value = serde_json::from_str(&proof).unwrap();
        damaged["signature"] = json!("00".repeat(64));
        assert_eq!(
            verify(&damaged.to_string()),
            Some(Rejected(Error::Unauthorized))
        );
        assert_eq!(
            verify(&signed(&authority, server_now, server_now - 1)),
            Some(Expired)
        );
        assert_eq!(
            verify(&signed(&authority, server_now, server_now + 1_000)),
            Some(Expired)
        );
        assert_eq!(
            verify(&signed_for(
                &authority,
                &binding(),
                server_now + 1,
                server_now,
                session_expiry
            )),
            Some(Rejected(Error::Expired)),
        );
    }

    #[test]
    fn bootstrap_selection_uses_authenticated_uncertainty_and_preserves_session_cap() {
        let authority = generate_key();
        let at = Instant::now();
        let wall = crate::unix_millis();
        let expiry = wall + 20_000;
        let first = OnlineClock::verify_bootstrap(
            &signed_for(&authority, &binding(), wall - 1020, wall - 1000, expiry),
            authority.public(),
            binding(),
            at - Duration::from_millis(1100),
            at - Duration::from_millis(1000),
        )
        .unwrap();
        let original_cap = first.deadline_for(expiry).unwrap();
        let next_scope = OnlineClockBinding {
            nonce: "1123456789abcdef0123456789abcdef",
            ..binding()
        };
        let next = OnlineClock::verify_bootstrap(
            &signed_for(&authority, &next_scope, wall - 880, wall - 20, expiry),
            authority.public(),
            next_scope,
            at - Duration::from_millis(900),
            at,
        )
        .unwrap();
        assert_eq!(first.bootstrap_uncertainty(), Duration::from_millis(81));
        assert_eq!(next.bootstrap_uncertainty(), Duration::from_millis(41));
        assert!(next.bootstrap_round_trip() > first.bootstrap_round_trip());
        assert!(next.deadline_for(expiry).unwrap() > original_cap);
        let selected = first.select_unpublished_bootstrap(next).unwrap();
        assert_eq!(selected.bootstrap_round_trip(), Duration::from_millis(900));
        assert_eq!(selected.bootstrap_uncertainty(), Duration::from_millis(41));
        assert_eq!(selected.deadline_for(expiry).unwrap(), original_cap);
        // Earlier expiries use the better bound; the original session cap still
        // governs every ticket and invitation projected through this clock.
        assert_eq!(
            selected.deadline_for(wall + 3_000).unwrap(),
            at + Duration::from_millis(2979)
        );
        let third_scope = OnlineClockBinding {
            nonce: "2123456789abcdef0123456789abcdef",
            ..binding()
        };
        let third = OnlineClock::verify_bootstrap(
            &signed_for(&authority, &third_scope, wall, wall, expiry),
            authority.public(),
            third_scope,
            at,
            at,
        )
        .unwrap();
        let selected = selected.select_unpublished_bootstrap(third).unwrap();
        assert_eq!(selected.deadline_for(expiry).unwrap(), original_cap);
        let host = generate_key();
        let guest = generate_key();
        let invitation = SessionInvitation::issue_online_with_clock(
            &host,
            guest.public(),
            SESSION,
            &[crate::Service {
                service_id: "game".into(),
                protocol: crate::Protocol::Tcp,
                endpoint: "127.0.0.1:25565".parse().unwrap(),
                revision: 1,
                max_datagram_bytes: 1400,
                required_capability: crate::Capability::ReliableStream,
                udp_return_policy: crate::UdpReturnPolicy::ReplyBudget,
            }],
            SessionDeadlines {
                admission_expires_at_unix_ms: wall + 10_000,
                session_expires_at_unix_ms: expiry,
            },
            false,
            &selected,
        )
        .unwrap();
        let before_remaining = Instant::now();
        assert!(
            invitation.session_remaining().unwrap()
                <= original_cap.saturating_duration_since(before_remaining)
        );
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct TicketBody<'a> {
            schema_version: u8,
            session_id: &'a str,
            issuer_instance: &'a str,
            peer_id: String,
            host_peer_id: String,
            account_id: &'a str,
            generation: u64,
            issued_at_unix_ms: u64,
            expires_at_unix_ms: u64,
            direct_addresses: Vec<String>,
        }
        let body = TicketBody {
            schema_version: 1,
            session_id: SESSION,
            issuer_instance: ISSUER,
            peer_id: host.public().to_string(),
            host_peer_id: host.public().to_string(),
            account_id: ACCOUNT,
            generation: 3,
            issued_at_unix_ms: wall,
            expires_at_unix_ms: expiry,
            direct_addresses: vec![],
        };
        let mut bytes = b"aurora/connect/relay-ticket/v1\0".to_vec();
        bytes.extend(serde_json::to_vec(&body).unwrap());
        let signature = authority
            .sign(&bytes)
            .to_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let ticket = crate::OnlineNetworkTicket::verify_clock_for_test(
            &json!({"body":body,"signature":signature}).to_string(),
            authority.public(),
            ISSUER,
            ACCOUNT,
            SESSION,
            host.public(),
            host.public(),
            &selected,
        )
        .unwrap();
        assert_eq!(ticket.deadline(), original_cap);
    }

    #[test]
    fn bootstrap_selection_rejects_changed_scope_shared_clock_and_disjoint_samples() {
        for mode in 0..10 {
            let authority = generate_key();
            let other = generate_key();
            let at = Instant::now();
            let wall = crate::unix_millis();
            let expiry = wall + 20_000;
            let first = OnlineClock::verify_bootstrap(
                &signed(&authority, wall, expiry),
                authority.public(),
                binding(),
                at - Duration::from_millis(20),
                at,
            )
            .unwrap();
            let retained = (mode == 8).then(|| first.clone());
            let mut scope = OnlineClockBinding {
                nonce: "1123456789abcdef0123456789abcdef",
                ..binding()
            };
            match mode {
                1 => {
                    scope.issuer =
                        "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd"
                }
                2 => scope.account = "another_account",
                3 => scope.session = OTHER_OPERATION,
                4 => scope.operation = OTHER_OPERATION,
                5 => scope.generation += 1,
                7 => scope.nonce = NONCE,
                _ => (),
            }
            let key = if mode == 0 { &other } else { &authority };
            let server_now = if mode == 9 { wall + 1000 } else { wall };
            let proof = signed_for(
                key,
                &scope,
                server_now,
                server_now,
                expiry + u64::from(mode == 6),
            );
            let next = OnlineClock::verify_bootstrap(&proof, key.public(), scope, at, at).unwrap();
            assert!(
                first.select_unpublished_bootstrap(next).is_err(),
                "mode {mode}"
            );
            if let Some(retained) = retained {
                assert!(retained.check_current().is_ok());
            }
        }
    }

    #[test]
    fn bootstrap_selection_keeps_the_intersection_of_discarded_samples() {
        for compatible in [false, true] {
            let authority = generate_key();
            let at = Instant::now();
            let wall = crate::unix_millis();
            let expiry = wall + 20_000;
            let sample = |nonce, server_now, sent_ms, received_ms| {
                let scope = OnlineClockBinding { nonce, ..binding() };
                OnlineClock::verify_bootstrap(
                    &signed_for(&authority, &scope, server_now, server_now, expiry),
                    authority.public(),
                    scope,
                    at - Duration::from_millis(sent_ms),
                    at - Duration::from_millis(received_ms),
                )
                .unwrap()
            };
            // At `at`, A=[wall,wall+10], B=[wall+9,wall+20]. A wins
            // on uncertainty, but the cumulative interval remains [9,10].
            let first = sample(NONCE, wall - 31, 40, 31);
            let second = sample("1123456789abcdef0123456789abcdef", wall - 11, 30, 20);
            let selected = first.select_unpublished_bootstrap(second).unwrap();
            assert_eq!(selected.bootstrap_round_trip(), Duration::from_millis(9));
            assert_eq!(selected.bootstrap_uncertainty(), Duration::from_millis(9));
            // The incompatible C=[wall-5,wall+4] overlaps A and is narrower,
            // but contradicts discarded B. A compatible C still may win.
            let third = if compatible {
                sample("2123456789abcdef0123456789abcdef", wall + 10, 3, 0)
            } else {
                sample("2123456789abcdef0123456789abcdef", wall - 5, 8, 0)
            };
            let result = selected.select_unpublished_bootstrap(third);
            assert_eq!(result.is_ok(), compatible);
            if let Ok(clock) = result {
                assert_eq!(clock.bootstrap_round_trip(), Duration::from_millis(3));
            }
        }
    }

    #[test]
    fn bootstrap_uncertain_restrictions_never_supply_a_clock_or_extend_its_cap() {
        let authority = generate_key();
        let at = Instant::now();
        let wall = crate::unix_millis();
        let expiry = wall + 20_000;
        let uncertain = OnlineClock::verify_bootstrap_sample(
            &signed(&authority, wall - 100, expiry),
            authority.public(),
            binding(),
            at - Duration::from_millis(2200),
            at - Duration::from_millis(100),
        )
        .unwrap_err();
        let rendered = format!("{uncertain:?}");
        for secret in [ACCOUNT, SESSION, OPERATION, NONCE, ISSUER] {
            assert!(!rendered.contains(secret));
        }
        let OnlineClockBootstrapError::Uncertain { restriction, .. } = uncertain else {
            panic!("expected authenticated uncertainty");
        };
        let cap = restriction.0.deadline;
        let scope = OnlineClockBinding {
            nonce: "1123456789abcdef0123456789abcdef",
            ..binding()
        };
        let valid = |server_now| {
            OnlineClock::verify_bootstrap(
                &signed_for(&authority, &scope, server_now, server_now, expiry),
                authority.public(),
                OnlineClockBinding {
                    nonce: scope.nonce,
                    ..binding()
                },
                at - Duration::from_millis(50),
                at,
            )
            .unwrap()
        };
        assert!(
            restriction
                .clone()
                .constrain_unpublished(valid(wall - 200))
                .is_err()
        );
        let selected = restriction.constrain_unpublished(valid(wall)).unwrap();
        assert_eq!(selected.bootstrap_round_trip(), Duration::from_millis(50));
        assert_eq!(selected.deadline_for(expiry).unwrap(), cap);
        assert_eq!(selected.lower_at(at), wall);
        // A later signed but uncertain response must also constrain an earlier
        // usable sample; it cannot be ignored just because it will not win.
        let later_scope = OnlineClockBinding {
            nonce: "2123456789abcdef0123456789abcdef",
            ..binding()
        };
        let later = OnlineClock::verify_bootstrap_sample(
            &signed_for(&authority, &later_scope, wall + 100, wall + 100, expiry),
            authority.public(),
            later_scope,
            at - Duration::from_millis(2100),
            at,
        )
        .unwrap_err();
        let OnlineClockBootstrapError::Uncertain { restriction, .. } = later else {
            panic!("expected authenticated uncertainty");
        };
        assert!(restriction.constrain_unpublished(selected).is_err());
    }

    #[test]
    fn bootstrap_uncertain_restrictions_keep_cumulative_scope_and_intersection() {
        let authority = generate_key();
        let at = Instant::now();
        let wall = crate::unix_millis();
        let expiry = wall + 30_000;
        let uncertain = |nonce, server_now, sent_ms, received_ms| {
            let scope = OnlineClockBinding { nonce, ..binding() };
            let error = OnlineClock::verify_bootstrap_sample(
                &signed_for(&authority, &scope, server_now, server_now, expiry),
                authority.public(),
                scope,
                at - Duration::from_millis(sent_ms),
                at - Duration::from_millis(received_ms),
            )
            .unwrap_err();
            let OnlineClockBootstrapError::Uncertain { restriction, .. } = error else {
                panic!("expected authenticated uncertainty");
            };
            restriction
        };
        // A=[0,2101], B=[2000,4101] relative to wall. No clock exists yet.
        let first = uncertain(NONCE, wall - 100, 2200, 100);
        let second = uncertain("1123456789abcdef0123456789abcdef", wall + 2000, 2100, 0);
        let cap = first.0.deadline.min(second.0.deadline);
        let merged = first.merge(second).unwrap();
        assert_eq!(merged.0.deadline, cap);
        let scope = OnlineClockBinding {
            nonce: "2123456789abcdef0123456789abcdef",
            ..binding()
        };
        let candidate = OnlineClock::verify_bootstrap(
            &signed_for(&authority, &scope, wall + 10, wall + 10, expiry),
            authority.public(),
            scope,
            at - Duration::from_millis(50),
            at,
        )
        .unwrap();
        assert!(merged.constrain_unpublished(candidate).is_err());
    }

    #[test]
    fn authenticated_clock_rejects_slow_stale_expired_and_cross_attempt_proofs() {
        let authority = generate_key();
        let now = Instant::now();
        let server_now = crate::unix_millis();
        let proof = signed(&authority, server_now, server_now + 20_000);
        let packet: serde_json::Value = serde_json::from_str(&proof).unwrap();
        let fields = [
            "schemaVersion",
            "issuerInstance",
            "accountId",
            "sessionId",
            "operationId",
            "generation",
            "nonce",
            "serverReceivedAtUnixMs",
            "serverNowUnixMs",
            "sessionExpiresAtUnixMs",
        ];
        let positional = fields
            .iter()
            .map(|field| packet["body"][field].clone())
            .collect::<Vec<_>>();
        for invalid in [
            json!([packet["body"], packet["signature"]]),
            json!({"body": positional, "signature": packet["signature"]}),
        ] {
            assert!(
                OnlineClock::verify_bootstrap(
                    &invalid.to_string(),
                    authority.public(),
                    binding(),
                    now,
                    now,
                )
                .is_err()
            );
        }
        let reject = |scope, sent, received| {
            assert!(
                OnlineClock::verify_bootstrap(&proof, authority.public(), scope, sent, received)
                    .is_err()
            );
        };
        reject(binding(), now - Duration::from_secs(3), now);
        let stale = now - Duration::from_secs(3);
        reject(binding(), stale, stale);
        for scope in [
            OnlineClockBinding {
                nonce: "0123456789abcdef0123456789abcdee",
                ..binding()
            },
            OnlineClockBinding {
                operation: OTHER_OPERATION,
                ..binding()
            },
            OnlineClockBinding {
                generation: 4,
                ..binding()
            },
            OnlineClockBinding {
                account: "other",
                ..binding()
            },
            OnlineClockBinding {
                session: OTHER_OPERATION,
                ..binding()
            },
            OnlineClockBinding {
                issuer: "other",
                ..binding()
            },
        ] {
            reject(scope, now, now);
        }
        assert!(
            OnlineClock::verify_bootstrap(&proof, generate_key().public(), binding(), now, now)
                .is_err()
        );
        let expired = signed(&authority, server_now, server_now + 100);
        assert!(
            OnlineClock::verify_bootstrap(
                &expired,
                authority.public(),
                binding(),
                now - Duration::from_millis(300),
                now
            )
            .is_err()
        );
    }

    #[test]
    fn authenticated_clock_keeps_challenge_and_invitation_in_the_original_time_scope() {
        let authority = generate_key();
        let host = generate_key();
        let guest = generate_key();
        let now = Instant::now();
        let server_now = crate::unix_millis() + 120_000;
        let clock = OnlineClock::verify_bootstrap(
            &signed(&authority, server_now, server_now + 60_000),
            authority.public(),
            binding(),
            now,
            now,
        )
        .unwrap();
        let challenge = json!({
            "schemaVersion": 1, "purpose": "accountKeyBinding", "issuerInstance": ISSUER,
            "challengeId": "11".repeat(32), "accountId": ACCOUNT, "operationId": OPERATION,
            "generation": 3, "role": "host", "publicKey": host.public().to_string(),
            "sessionId": SESSION, "sessionDigest": "44".repeat(32),
            "issuedAtUnixMs": server_now, "expiresAtUnixMs": server_now + 30_000
        });
        assert!(SocialKeyChallenge::parse(&challenge.to_string()).is_err());
        let challenge =
            SocialKeyChallenge::parse_with_clock(&challenge.to_string(), &clock).unwrap();
        assert!(challenge.sign_with_clock(&host, &clock).is_ok());

        let services = [crate::Service {
            service_id: "game".into(),
            protocol: crate::Protocol::Tcp,
            endpoint: "127.0.0.1:25565".parse().unwrap(),
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: crate::Capability::ReliableStream,
            udp_return_policy: crate::UdpReturnPolicy::ReplyBudget,
        }];
        let invitation = SessionInvitation::issue_online_with_clock(
            &host,
            guest.public(),
            SESSION,
            &services,
            SessionDeadlines {
                admission_expires_at_unix_ms: server_now + 30_000,
                session_expires_at_unix_ms: server_now + 50_000,
            },
            false,
            &clock,
        )
        .unwrap();
        assert!(
            SessionInvitation::verify_online(&invitation.encode(), host.public(), guest.public())
                .is_err()
        );
        let verified = SessionInvitation::verify_online_with_clock(
            &invitation.encode(),
            host.public(),
            guest.public(),
            false,
            &clock,
        )
        .unwrap();
        assert!(verified.check_admission_current().is_ok());
        assert!(verified.check_session_current().is_ok());
    }
}
