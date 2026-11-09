//! Account-fenced, bounded native observations of the compiled HTTPS authority.
use super::*;
use std::collections::HashSet;

static IN_FLIGHT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
static NEXT_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Observation {
    schema_version: u8,
    account_id: String,
    session_id: String,
    state: String,
    server_now_unix_ms: u64,
    #[serde(deserialize_with = "Option::deserialize")]
    checked_at_unix_ms: Option<u64>,
    #[serde(deserialize_with = "Option::deserialize")]
    expires_at_unix_ms: Option<u64>,
    #[serde(deserialize_with = "Option::deserialize")]
    admitted_guests: Option<u8>,
    #[serde(deserialize_with = "Option::deserialize")]
    connected_guests: Option<u8>,
    #[serde(deserialize_with = "Option::deserialize")]
    host_player_connected: Option<bool>,
    #[serde(deserialize_with = "Option::deserialize")]
    max_guests: Option<u8>,
}
fn decode(bytes: &[u8]) -> Result<Observation, ConnectError> {
    // Keep original tokens for serde's duplicate-field checks. Positional struct
    // arrays are never a wire format for this native HTTPS observation.
    if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
        return Err(ConnectError::InvitationRejected);
    }
    serde_json::from_slice(bytes).map_err(|_| ConnectError::InvitationRejected)
}
fn project(
    value: Observation,
    account: &str,
    session: &str,
    elapsed: Duration,
) -> Result<SocialSessionAvailability, ConnectError> {
    if value.schema_version != 1
        || value.account_id != account
        || value.session_id != session
        || value.server_now_unix_ms == 0
    {
        return Err(ConnectError::InvitationRejected);
    }
    let unknown = |state| SocialSessionAvailability {
        session_id: session.into(),
        state,
        valid_for_ms: 0,
        admitted_guests: None,
        connected_guests: None,
        host_player_connected: None,
        max_guests: None,
    };
    match value.state.as_str() {
        "unavailable" | "unknown" => {
            if value.checked_at_unix_ms.is_some()
                || value.expires_at_unix_ms.is_some()
                || value.admitted_guests.is_some()
                || value.connected_guests.is_some()
                || value.host_player_connected.is_some()
                || value.max_guests.is_some()
            {
                return Err(ConnectError::InvitationRejected);
            }
            let mut result = unknown(if value.state == "unavailable" {
                "unavailable"
            } else {
                "unknown"
            });
            if result.state == "unavailable" {
                result.valid_for_ms =
                    3000_u64.saturating_sub(elapsed.as_millis().min(u64::MAX as u128) as u64);
            }
            Ok(result)
        }
        "ready" => {
            let (
                Some(checked),
                Some(expires),
                Some(admitted),
                Some(connected),
                Some(local),
                Some(maximum),
            ) = (
                value.checked_at_unix_ms,
                value.expires_at_unix_ms,
                value.admitted_guests,
                value.connected_guests,
                value.host_player_connected,
                value.max_guests,
            )
            else {
                return Err(ConnectError::InvitationRejected);
            };
            if checked > value.server_now_unix_ms
                || expires <= value.server_now_unix_ms
                || expires <= checked
                || expires - checked > 3000
                || !(1..=8).contains(&maximum)
                || admitted > maximum
                || connected > admitted
                || (local && maximum > 7)
            {
                return Err(ConnectError::InvitationRejected);
            }
            let remaining = (expires - value.server_now_unix_ms)
                .saturating_sub(elapsed.as_millis().min(u64::MAX as u128) as u64);
            if remaining == 0 {
                return Ok(unknown("unknown"));
            }
            Ok(SocialSessionAvailability {
                session_id: session.into(),
                state: "ready",
                valid_for_ms: remaining,
                admitted_guests: Some(admitted),
                connected_guests: Some(connected),
                host_player_connected: Some(local),
                max_guests: Some(maximum),
            })
        }
        _ => Err(ConnectError::InvitationRejected),
    }
}
impl Manager {
    pub(in crate::engine::connect) async fn social_observe_sessions(
        &self,
        owner: &str,
        request: SocialPresenceRequest,
        token: String,
    ) -> Result<Vec<SocialSessionAvailability>, ConnectError> {
        if owner != "main"
            || request.account_id.is_empty()
            || request.account_id.len() > 128
            || !request
                .account_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || token.is_empty()
            || token.len() > 8192
            || !super::social::online_token_destination(&token, &request.account_id)
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            || request.session_ids.is_empty()
            || request.session_ids.len() > 8
        {
            return Err(ConnectError::InvalidRequest);
        }
        let mut unique = HashSet::new();
        for id in &request.session_ids {
            if !unique.insert(id)
                || uuid::Uuid::parse_str(id)
                    .ok()
                    .is_none_or(|parsed| parsed.get_version_num() != 4 || parsed.to_string() != *id)
            {
                return Err(ConnectError::InvalidRequest);
            }
        }
        let current = || {
            let state = lock(&self.state)?;
            if state.shutting_down || state.social_epoch != request.account_epoch {
                Err(ConnectError::StaleOperation)
            } else {
                Ok(())
            }
        };
        current()?;
        let origin = self
            .social_authority
            .as_ref()
            .and_then(|value| value.online_origin)
            .ok_or(ConnectError::Disabled)?;
        let _permit = IN_FLIGHT.try_acquire().map_err(|_| ConnectError::Busy)?;
        {
            let mut next = NEXT_REQUEST.lock().map_err(|_| ConnectError::Internal)?;
            if next.is_some_and(|at| Instant::now() < at) {
                return Err(ConnectError::SocialQuota);
            }
            *next = Some(Instant::now() + Duration::from_secs(2));
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(0)
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|_| ConnectError::Internal)?;
        let mut tasks = tokio::task::JoinSet::new();
        for session in request.session_ids {
            let client = client.clone();
            let token = token.clone();
            let account = request.account_id.clone();
            tasks.spawn(async move {
                let started = Instant::now();
                let mut response = client
                    .post(format!("{origin}/v1/social/presence"))
                    .header("X-Aurora-Session", &session)
                    .bearer_auth(token)
                    .json(&serde_json::json!({}))
                    .send()
                    .await
                    .map_err(|_| ConnectError::SocialUnavailable)?;
                if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    return Err(ConnectError::SocialQuota);
                }
                if !response.status().is_success() {
                    return Err(ConnectError::SocialUnavailable);
                }
                if response
                    .content_length()
                    .is_some_and(|length| length > 2048)
                {
                    return Err(ConnectError::InvitationRejected);
                }
                let mut bytes = Vec::new();
                while let Some(chunk) = response
                    .chunk()
                    .await
                    .map_err(|_| ConnectError::SocialUnavailable)?
                {
                    if bytes.len() + chunk.len() > 2048 {
                        return Err(ConnectError::InvitationRejected);
                    }
                    bytes.extend_from_slice(&chunk);
                }
                let value = decode(&bytes)?;
                project(value, &account, &session, started.elapsed())
                    .map(|value| (value, Instant::now()))
            });
        }
        let mut observed = Vec::new();
        while let Some(result) = tasks.join_next().await {
            current()?;
            observed.push(result.map_err(|_| ConnectError::Internal)??);
        }
        current()?;
        let mut observed = observed
            .into_iter()
            .map(|(mut value, completed)| {
                value.valid_for_ms = value
                    .valid_for_ms
                    .saturating_sub(completed.elapsed().as_millis().min(u64::MAX as u128) as u64);
                if value.state == "ready" && value.valid_for_ms == 0 {
                    value.state = "unknown";
                    value.admitted_guests = None;
                    value.connected_guests = None;
                    value.host_player_connected = None;
                    value.max_guests = None;
                }
                value
            })
            .collect::<Vec<_>>();
        observed.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        Ok(observed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Observation {
        Observation {
            schema_version: 1,
            account_id: "alice".into(),
            session_id: "session".into(),
            state: "ready".into(),
            server_now_unix_ms: 10100,
            checked_at_unix_ms: Some(10000),
            expires_at_unix_ms: Some(13000),
            admitted_guests: Some(2),
            connected_guests: Some(1),
            host_player_connected: Some(true),
            max_guests: Some(7),
        }
    }
    #[test]
    fn catalog_observation_subtracts_rtt_and_never_uses_local_wall_clock() {
        let value = project(sample(), "alice", "session", Duration::from_millis(600)).unwrap();
        assert_eq!(value.valid_for_ms, 2300);
        assert_eq!(value.connected_guests, Some(1));
        assert_eq!(value.admitted_guests, Some(2));
        assert_eq!(
            project(sample(), "alice", "session", Duration::from_secs(3))
                .unwrap()
                .state,
            "unknown"
        );
    }
    #[test]
    fn catalog_observation_rejects_account_mismatch_invented_counts_and_extended_freshness() {
        assert!(project(sample(), "bob", "session", Duration::ZERO).is_err());
        let mut value = sample();
        value.connected_guests = Some(3);
        assert!(project(value, "alice", "session", Duration::ZERO).is_err());
        let mut value = sample();
        value.expires_at_unix_ms = Some(13001);
        assert!(project(value, "alice", "session", Duration::ZERO).is_err());
    }

    #[test]
    fn catalog_observation_accepts_only_complete_original_json_objects() {
        let valid = serde_json::json!({
            "schemaVersion":1,"accountId":"alice","sessionId":"session","state":"unavailable",
            "serverNowUnixMs":10000,"checkedAtUnixMs":null,"expiresAtUnixMs":null,
            "admittedGuests":null,"connectedGuests":null,"hostPlayerConnected":null,"maxGuests":null
        });
        let encoded = serde_json::to_vec(&valid).unwrap();
        assert!(decode(&encoded).is_ok());
        for field in valid.as_object().unwrap().keys() {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                decode(&serde_json::to_vec(&missing).unwrap()).is_err(),
                "{field}"
            );
        }
        let mut extra = valid.clone();
        extra["worldPlayers"] = serde_json::json!(7);
        assert!(decode(&serde_json::to_vec(&extra).unwrap()).is_err());
        let duplicate =
            String::from_utf8(encoded)
                .unwrap()
                .replacen('{', "{\"schemaVersion\":1,", 1);
        assert!(decode(duplicate.as_bytes()).is_err());
        assert!(decode(
            br#"[1,"alice","session","unavailable",10000,null,null,null,null,null,null]"#
        )
        .is_err());
        for nonobject in [b"null".as_slice(), b"[]", b"true", b"\"text\"", b""] {
            assert!(decode(nonobject).is_err());
        }
    }

    #[tokio::test]
    async fn catalog_foreign_credentials_fail_before_selecting_or_contacting_an_origin() {
        use base64::Engine as _;
        let manager = Manager::default();
        let valid = serde_json::json!({"aud":"auroralauncher","iss":"https://securetoken.google.com/auroralauncher","sub":"alice","exp":1_800_000_000});
        let token = |claims: &serde_json::Value| {
            format!(
                "{}.{}.AA",
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256"}"#),
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(serde_json::to_vec(claims).unwrap())
            )
        };
        let request = || SocialPresenceRequest {
            account_epoch: 0,
            account_id: "alice".into(),
            session_ids: vec![uuid::Uuid::new_v4().to_string()],
        };
        // This deliberately unsigned test string is only classified, never authenticated.
        // A correct destination reaches the disabled-origin gate without opening I/O.
        assert!(matches!(
            manager
                .social_observe_sessions("main", request(), token(&valid))
                .await,
            Err(ConnectError::Disabled)
        ));
        for (field, value) in [
            ("aud", "minecraft"),
            ("iss", "https://login.microsoftonline.com/"),
            ("sub", "other-account"),
        ] {
            let mut foreign = valid.clone();
            foreign[field] = serde_json::json!(value);
            assert!(matches!(
                manager
                    .social_observe_sessions("main", request(), token(&foreign))
                    .await,
                Err(ConnectError::InvalidRequest)
            ));
        }
        assert!(lock(&manager.state).unwrap().attempt.is_none());
    }
}
