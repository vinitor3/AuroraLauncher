//! Native-only terminal observation. Never an authority or a renderer payload.
use super::*;
#[cfg(feature = "connect-lab")]
#[path = "terminal_diagnostic_store.rs"]
mod store;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectTerminalDiagnostic {
    schema: u8,
    observed_at_ms: u64,
    role: Option<&'static str>,
    phase: &'static str,
    error_code: Option<&'static str>,
    failure_stage: Option<SocialFailureStage>,
    observation: Option<SocialFailureObservation>,
    #[serde(skip)]
    sequence: u64,
}
impl ConnectTerminalDiagnostic {
    #[cfg(feature = "connect-lab")]
    pub(super) fn capture(
        sequence: u64,
        role: Option<&'static str>,
        error: Option<ConnectError>,
        observation: Option<SocialFailureObservation>,
    ) -> Self {
        Self {
            schema: 1,
            observed_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u64::MAX as u128) as u64,
            role: role.filter(|role| matches!(*role, "host" | "guest")),
            phase: if error.is_some() { "failed" } else { "closed" },
            error_code: error.map(ConnectError::code),
            failure_stage: match error {
                Some(ConnectError::SocialFailure(stage, _)) => Some(stage),
                _ => None,
            },
            observation: error.and(observation),
            sequence,
        }
    }
    /// Native setup supplies the launcher-owned Doctor directory. This bounded
    /// write is best effort and must run off the actor/async executor.
    #[cfg(feature = "connect-lab")]
    pub fn persist_latest(&self, directory: &std::path::Path) -> bool {
        serde_json::to_vec(self).is_ok_and(|bytes| store::persist(directory, &bytes).is_ok())
    }

    /// Ordering stays in RAM and never identifies an operation in the journal.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Existing journals are untrusted. Refuse unknown fields/codes rather than
    /// overwrite another file that happens to occupy the diagnostic pathname.
    pub fn valid_stored(value: &serde_json::Value) -> bool {
        use serde_json::Value;
        fn exact(v: &Value, keys: &[&str]) -> bool {
            v.as_object()
                .is_some_and(|o| o.len() == keys.len() && keys.iter().all(|k| o.contains_key(*k)))
        }
        fn optional(v: &Value, allowed: &[&str]) -> bool {
            v.is_null() || v.as_str().is_some_and(|s| allowed.contains(&s))
        }
        if !exact(
            value,
            &[
                "schema",
                "observedAtMs",
                "role",
                "phase",
                "errorCode",
                "failureStage",
                "observation",
            ],
        ) || value["schema"] != 1
            || value["observedAtMs"].as_u64().is_none()
            || !optional(&value["role"], &["host", "guest"])
            || !matches!(value["phase"].as_str(), Some("closed" | "failed"))
            || !optional(
                &value["errorCode"],
                &[
                    "connect_disabled",
                    "connect_busy",
                    "connect_invalid_request",
                    "connect_stale_operation",
                    "connect_owner_mismatch",
                    "connect_plan_expired",
                    "connect_wrong_digest",
                    "connect_invitation_rejected",
                    "connect_transport_failed",
                    "connect_port_unavailable",
                    "connect_status_failed",
                    "connect_cancelled",
                    "connect_internal",
                    "connect_source_changed",
                    "connect_cleanup_failed",
                    "connect_social_quota",
                    "connect_social_unavailable",
                    "connect_social_auth_failed",
                    "connect_social_account_not_registered",
                    "connect_social_not_authorized",
                    "connect_social_authority_changed",
                    "connect_social_clock_uncertain",
                    "connect_social_clock_unverified",
                    "connect_social_service_unavailable",
                    "connect_social_request_rejected",
                    "connect_social_request_timed_out",
                    "connect_social_continuity_lost",
                ],
            )
            || !optional(
                &value["failureStage"],
                &[
                    "authentication",
                    "bootstrap",
                    "identity",
                    "keyChallenge",
                    "hostReview",
                    "hostRegistration",
                    "guestAdmission",
                    "sessionContinuity",
                    "relay",
                    "sessionControl",
                ],
            )
        {
            return false;
        }
        if value["phase"] == "closed"
            && (!value["errorCode"].is_null()
                || !value["failureStage"].is_null()
                || !value["observation"].is_null())
        {
            return false;
        }
        if value["phase"] == "failed" && value["errorCode"].is_null() {
            return false;
        }
        let observation = &value["observation"];
        if observation.is_null() {
            return true;
        }
        exact(
            observation,
            &[
                "gate",
                "bootstrapRoundtripMs",
                "bootstrapUncertaintyMs",
                "hostPollElapsedMs",
                "hostPollAgeMs",
                "sessionRemainingMs",
            ],
        ) && matches!(
            observation["gate"].as_str(),
            Some(
                "hostPoll"
                    | "source"
                    | "relayTicket"
                    | "invitation"
                    | "session"
                    | "hostTransport"
                    | "other"
            )
        ) && [
            "bootstrapRoundtripMs",
            "bootstrapUncertaintyMs",
            "hostPollElapsedMs",
            "hostPollAgeMs",
        ]
        .iter()
        .all(|key| {
            observation[key].is_null() || observation[key].as_u64().is_some_and(|v| v <= 14_400_000)
        }) && observation["sessionRemainingMs"]
            .as_u64()
            .is_some_and(|v| v <= 14_400_000)
    }
}

#[cfg(feature = "connect-lab")]
pub(super) type TerminalRecorder = std::sync::Arc<dyn Fn(ConnectTerminalDiagnostic) + Send + Sync>;

impl ConnectManager {
    /// Set only by native application setup. No IPC accepts a recorder or path.
    #[cfg(feature = "connect-lab")]
    pub fn with_terminal_recorder(
        self,
        recorder: impl Fn(ConnectTerminalDiagnostic) + Send + Sync + 'static,
    ) -> Self {
        Self {
            live: self
                .live
                .with_terminal_recorder(std::sync::Arc::new(recorder)),
        }
    }
}
