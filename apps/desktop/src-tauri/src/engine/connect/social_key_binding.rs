//! A bounded proof from the original pending key; never a transport approval.
use super::*;

impl Manager {
    pub(in crate::engine::connect) fn prove_social_key_binding(
        &self,
        owner: &str,
        operation_id: &str,
        challenge_json: &str,
    ) -> Result<lab::SocialKeyBindingProof, ConnectError> {
        let mut state = lock(&self.state)?;
        let attempt = selected(&mut state, owner, operation_id)?;
        expire_pending(attempt);
        if attempt.cancel.is_cancelled()
            || attempt.social_key_proof_emitted
            || attempt.reviewed.is_some()
            || !matches!(attempt.snapshot.phase, "hostPrepared" | "guestPrepared")
        {
            return Err(ConnectError::StaleOperation);
        }
        let challenge = lab::SocialKeyChallenge::parse(challenge_json)
            .map_err(|_| ConnectError::InvalidRequest)?;
        let preparation = attempt
            .snapshot
            .preparation
            .as_ref()
            .ok_or(ConnectError::StaleOperation)?;
        if challenge.operation_id() != operation_id
            || challenge.operation_id() != preparation.operation_id
            || challenge.generation() != preparation.generation
            || challenge.generation() != attempt.snapshot.generation
            || challenge.public_key() != preparation.public_id
            || challenge.expires_at_unix_ms() > preparation.expires_at_ms
            || Some(challenge.role().as_str()) != attempt.snapshot.role
        {
            return Err(ConnectError::InvalidRequest);
        }
        let key = match attempt.draft.as_ref() {
            Some(Draft::Host {
                request,
                key,
                source: TargetGuard::Manual,
                ..
            }) if request.profile == Profile::Session && request.mode == ConnectMode::Loopback => {
                key
            }
            Some(Draft::Guest {
                request,
                key,
                profile: Profile::Session,
                ..
            }) if request.mode == ConnectMode::Loopback => key,
            _ => return Err(ConnectError::InvalidRequest),
        };
        // The lock serializes signing with stop/logout and review. A proof cannot
        // outlive the original preparation budget because this never reanchors it.
        let proof = challenge
            .sign(key)
            .map_err(|_| ConnectError::InvalidRequest)?;
        if Instant::now() >= attempt.prepared_deadline
            || lab::unix_millis() >= preparation.expires_at_ms
        {
            expire_pending(attempt);
            return Err(ConnectError::PlanExpired);
        }
        challenge
            .validate()
            .map_err(|_| ConnectError::PlanExpired)?;
        attempt.social_key_proof_emitted = true;
        Ok(proof)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn host(manager: &Manager) -> ConnectPreparation {
        manager
            .prepare_session_host(
                "main",
                SessionHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: "127.0.0.1:42000".into(),
                    game_target: "127.0.0.1:43001".into(),
                    admission_seconds: 30,
                    session_seconds: 60,
                },
            )
            .unwrap()
    }
    fn challenge(preparation: &ConnectPreparation, role: &str) -> Value {
        json!({"schemaVersion":1,"purpose":"accountKeyBinding","issuerInstance":"11".repeat(32),
            "challengeId":"22".repeat(32),"accountId":"TestAccount",
            "operationId":preparation.operation_id,"generation":preparation.generation,
            "role":role,"publicKey":preparation.public_id,
            "sessionId":"a9e1bccc-3768-43bf-ab1c-36fc48e78e5a","sessionDigest":"33".repeat(32),
            "issuedAtUnixMs":lab::unix_millis(),"expiresAtUnixMs":preparation.expires_at_ms-1})
    }
    #[test]
    fn social_key_proof_preserves_prepared_phase_without_transport_and_is_one_shot() {
        for role in ["host", "guest"] {
            let manager = Manager::default();
            let preparation = if role == "host" {
                host(&manager)
            } else {
                manager
                    .prepare_session_guest(
                        "main",
                        GuestRequest {
                            mode: ConnectMode::Loopback,
                            expected_host_public_id: lab::generate_key().public().to_string(),
                        },
                    )
                    .unwrap()
            };
            let before = manager.snapshot("main").unwrap();
            let request = challenge(&preparation, role).to_string();
            let proof = manager
                .prove_social_key_binding("main", &preparation.operation_id, &request)
                .unwrap();
            proof.verify().unwrap();
            let after = manager.snapshot("main").unwrap();
            assert_eq!(before.phase, after.phase);
            assert_eq!(before.generation, after.generation);
            assert_eq!(after.transport_state, "notStarted");
            assert!(after.tcp_address.is_none());
            assert!(after.review.is_none());
            let state = lock(&manager.state).unwrap();
            let attempt = state.attempt.as_ref().unwrap();
            assert!(
                attempt.worker.is_none() && attempt.host.is_none() && attempt.authority.is_none()
            );
            drop(state);
            assert_eq!(
                manager.prove_social_key_binding("main", &preparation.operation_id, &request),
                Err(ConnectError::StaleOperation)
            );
        }
    }
    #[test]
    fn social_key_context_failures_do_not_consume_the_key_or_preparation() {
        let manager = Manager::default();
        let preparation = host(&manager);
        let original = challenge(&preparation, "host");
        assert_eq!(
            manager.prove_social_key_binding(
                "other",
                &preparation.operation_id,
                &original.to_string()
            ),
            Err(ConnectError::OwnerMismatch)
        );
        assert_eq!(
            manager.prove_social_key_binding("main", "other", &original.to_string()),
            Err(ConnectError::StaleOperation)
        );
        for (field, value) in [
            ("operationId", json!("8b414333-65f8-4524-a01a-d609a5168463")),
            ("generation", json!(preparation.generation + 1)),
            ("role", json!("guest")),
            ("publicKey", json!(lab::generate_key().public().to_string())),
            ("expiresAtUnixMs", json!(preparation.expires_at_ms + 1)),
        ] {
            let mut changed = original.clone();
            changed[field] = value;
            assert_eq!(
                manager.prove_social_key_binding(
                    "main",
                    &preparation.operation_id,
                    &changed.to_string()
                ),
                Err(ConnectError::InvalidRequest),
                "{field}"
            );
        }
        manager
            .prove_social_key_binding("main", &preparation.operation_id, &original.to_string())
            .unwrap();
    }
    #[tokio::test]
    async fn social_key_stop_and_original_monotonic_expiry_close_the_key() {
        let manager = Manager::default();
        let preparation = host(&manager);
        let original = challenge(&preparation, "host").to_string();
        manager
            .stop("main", &preparation.operation_id)
            .await
            .unwrap();
        assert_eq!(
            manager.prove_social_key_binding("main", &preparation.operation_id, &original),
            Err(ConnectError::StaleOperation)
        );
        let preparation = host(&manager);
        let original = challenge(&preparation, "host").to_string();
        {
            let mut state = lock(&manager.state).unwrap();
            state.attempt.as_mut().unwrap().prepared_deadline =
                Instant::now() - Duration::from_millis(1);
        }
        assert_eq!(
            manager.prove_social_key_binding("main", &preparation.operation_id, &original),
            Err(ConnectError::StaleOperation)
        );
        assert_eq!(manager.snapshot("main").unwrap().cleanup_state, "complete");
    }
    #[test]
    fn social_key_legacy_external_fragmented_and_reviewed_profiles_are_ineligible() {
        for (profile, mode) in [
            (Profile::Legacy, ConnectMode::Loopback),
            (Profile::Session, ConnectMode::ExternalIpv6),
            (Profile::Session, ConnectMode::DirectIpv4),
            (Profile::Fragmented, ConnectMode::Loopback),
            (Profile::SessionFragmented, ConnectMode::Loopback),
        ] {
            let manager = Manager::default();
            let preparation = manager
                .prepare_guest_internal(
                    "main",
                    GuestRequest {
                        mode,
                        expected_host_public_id: lab::generate_key().public().to_string(),
                    },
                    profile,
                )
                .unwrap();
            assert_eq!(
                manager.prove_social_key_binding(
                    "main",
                    &preparation.operation_id,
                    &challenge(&preparation, "guest").to_string()
                ),
                Err(ConnectError::InvalidRequest)
            );
        }
        let manager = Manager::default();
        let preparation = host(&manager);
        let original = challenge(&preparation, "host").to_string();
        // A review is no longer the original fresh preparation, even if its key exists.
        {
            let mut state = lock(&manager.state).unwrap();
            state.attempt.as_mut().unwrap().snapshot.phase = "reviewed";
        }
        assert_eq!(
            manager.prove_social_key_binding("main", &preparation.operation_id, &original),
            Err(ConnectError::StaleOperation)
        );
    }
}
