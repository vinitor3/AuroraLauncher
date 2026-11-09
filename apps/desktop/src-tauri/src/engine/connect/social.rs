//! Social admission is one worker of the existing Attempt, not a UI sidecar.
use super::*;
use serde_json::{json, Value};
use tokio::sync::oneshot;

#[path = "social_online.rs"]
mod online;
#[cfg(test)]
#[path = "social_ready_game_fixture.rs"]
mod ready_fixture;
#[path = "social_ready_game.rs"]
pub(super) mod ready_source;

#[derive(Clone)]
struct DiscoverySelection {
    id: String,
    manager: crate::engine::connect_discovery::DiscoveryManager,
}

const ORIGIN: &str = "http://127.0.0.1:42888";
const MAX_HTTP_BYTES: usize = 32768;
pub(super) struct Control {
    request: SocialReserveRequest,
    key: Option<lab::SecretKey>,
    reservation_generation: u64,
    approval: Option<oneshot::Sender<String>>,
    selection: Option<DiscoverySelection>,
    token: Option<Arc<Mutex<String>>>,
    revocations: std::collections::HashSet<String>,
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    native_voice_source: Option<crate::engine::connect_voice::dedicated::NativeVoiceHostBinding>,
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    native_voice_local_key: Option<lab::SecretKey>,

    #[cfg(test)]
    guard_probe: Option<Arc<ready_source::fixture::Pause>>,
}
pub(super) fn clear(a: &mut Attempt) {
    if let Some(social) = &mut a.snapshot.social {
        social.ready_game_launch_available = None;
    }
    if let Some(control) = &mut a.social {
        control.key = None;
        control.approval = None;
        control.selection = None;
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        {
            control.native_voice_source = None;
            control.native_voice_local_key = None;
        }
        if let Some(token) = control.token.take() {
            if let Ok(mut token) = token.lock() {
                token.clear();
            }
        }
        #[cfg(test)]
        if let Some(probe) = control.guard_probe.take() {
            probe.release();
        }
    }
}
pub(super) fn expire(a: &mut Attempt) {
    if a.worker.is_none()
        && a.social.is_some()
        && (Instant::now() >= a.prepared_deadline
            || a.snapshot
                .social
                .as_ref()
                .and_then(|social| social.expires_at_ms)
                .is_none_or(|expiry| lab::unix_millis() >= expiry))
        && !matches!(a.snapshot.phase, "closed" | "failed")
    {
        clear(a);
        a.cancel.cancel();
        a.snapshot.phase = "closed";
        a.snapshot.cleanup_state = "complete";
        a.snapshot.error_code = Some(ConnectError::PlanExpired.code());
        if let Some(social) = &mut a.snapshot.social {
            social.stage = "closed";
            social.digest = None;
        }
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
    }
}
fn valid_uid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
// Destination confinement only, never authentication. The local authority still
// checks the account with Auth Emulator and applies Firestore rules on each call.
fn local_emulator_token(token: &str, expected_account: &str) -> bool {
    use base64::Engine as _;
    let segments: Vec<_> = token.split('.').collect();
    if segments.len() != 3 || !segments[2].is_empty() || token.len() > 8192 {
        return false;
    }
    let decode = |part: &str| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(part)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    };
    let (Some(header), Some(claims)) = (decode(segments[0]), decode(segments[1])) else {
        return false;
    };
    let now = lab::unix_millis() / 1000;
    header["alg"] == "none"
        && claims["aud"] == "demo-aurora-social"
        && claims["iss"] == "https://securetoken.google.com/demo-aurora-social"
        && claims["sub"] == expected_account
        && claims
            .get("user_id")
            .is_none_or(|id| id == expected_account)
        && claims["exp"].as_u64().is_some_and(|end| end > now)
        && claims["iat"].as_u64().is_some_and(|at| at <= now)
        && claims["auth_time"].as_u64().is_some_and(|at| at <= now)
}
/// Destination confinement; the online Worker, not decoded client claims, verifies RS256.
fn online_token_destination(token: &str, expected_account: &str) -> bool {
    use base64::Engine as _;
    let parts: Vec<_> = token.split('.').collect();
    if token.len() > 8192 || parts.len() != 3 || parts[2].is_empty() {
        return false;
    }
    let decode = |part: &str| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(part)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    };
    let (Some(header), Some(claims)) = (decode(parts[0]), decode(parts[1])) else {
        return false;
    };
    let now = lab::unix_millis() / 1000;
    header["alg"] == "RS256"
        && claims["aud"] == "auroralauncher"
        && claims["iss"] == "https://securetoken.google.com/auroralauncher"
        && claims["sub"] == expected_account
        && claims["exp"].as_u64().is_some_and(|expiry| expiry > now)
}
impl Manager {
    pub(in crate::engine::connect) fn social_revoke_participant(
        &self,
        owner: &str,
        id: &str,
        account_epoch: u64,
        grant_id: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        if !self
            .social_authority
            .as_ref()
            .is_some_and(|authority| authority.online_origin.is_some())
        {
            return Err(ConnectError::Disabled);
        }
        if grant_id.len() != 64
            || !grant_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ConnectError::InvalidRequest);
        }
        let mut state = lock(&self.state)?;
        if state.social_epoch != account_epoch {
            return Err(ConnectError::StaleOperation);
        }
        let attempt = selected(&mut state, owner, id)?;
        if attempt.cancel.is_cancelled()
            || attempt.worker.is_none()
            || attempt.snapshot.phase != "hostListening"
        {
            return Err(ConnectError::StaleOperation);
        }
        let control = attempt
            .social
            .as_mut()
            .ok_or(ConnectError::InvalidRequest)?;
        if control.request.account_epoch != account_epoch || control.request.role != "host" {
            return Err(ConnectError::InvalidRequest);
        }
        if control.revocations.contains(grant_id) {
            return Ok(attempt.snapshot.clone());
        }
        let participant = attempt
            .snapshot
            .social
            .as_mut()
            .and_then(|social| social.participants.as_mut())
            .and_then(|participants| {
                participants
                    .iter_mut()
                    .find(|participant| participant.grant_id == grant_id)
            })
            .ok_or(ConnectError::StaleOperation)?;
        if control.revocations.len() >= 128 {
            return Err(ConnectError::Busy);
        }
        control.revocations.insert(grant_id.into());
        participant.state = "removing";
        attempt.snapshot.generation = attempt.snapshot.generation.saturating_add(1);
        Ok(attempt.snapshot.clone())
    }
    pub(in crate::engine::connect) fn with_social_lab(authority: SocialLabAuthority) -> Self {
        Self {
            state: Default::default(),
            social_authority: Some(authority),
        }
    }
    pub(in crate::engine::connect) fn social_capabilities(&self) -> SocialCapabilities {
        SocialCapabilities {
            available: self.social_authority.is_some(),
            experimental: true,
            account_epoch: self.state.lock().map_or(0, |s| s.social_epoch),
            max_session_seconds: if self
                .social_authority
                .as_ref()
                .is_some_and(|a| a.online_origin.is_some())
            {
                14_400
            } else {
                60
            },
            owned_game_tcp_discovery: cfg!(windows).then_some(self.social_authority.is_some()),
        }
    }
    pub(in crate::engine::connect) fn social_reserve(
        &self,
        owner: &str,
        request: SocialReserveRequest,
    ) -> Result<SocialReservation, ConnectError> {
        self.social_reserve_inner(owner, request, None, false)
    }
    #[cfg(windows)]
    pub(in crate::engine::connect) fn social_reserve_discovered(
        &self,
        owner: &str,
        request: SocialDiscoveredReserveRequest,
        selection_id: String,
        discovery: crate::engine::connect_discovery::DiscoveryManager,
    ) -> Result<SocialReservation, ConnectError> {
        if !uuid::Uuid::parse_str(&selection_id)
            .is_ok_and(|id| id.get_version_num() == 4 && id.to_string() == selection_id)
        {
            return Err(ConnectError::InvalidRequest);
        }
        self.social_reserve_inner(
            owner,
            SocialReserveRequest {
                account_epoch: request.account_epoch,
                account_id: request.account_id,
                session_id: request.session_id,
                role: "host".into(),
                game_target: None,
                visibility: Some(request.visibility),
            },
            Some(DiscoverySelection {
                id: selection_id,
                manager: discovery,
            }),
            false,
        )
    }
    fn social_reserve_inner(
        &self,
        owner: &str,
        request: SocialReserveRequest,
        selection: Option<DiscoverySelection>,
        trusted_native_source: bool,
    ) -> Result<SocialReservation, ConnectError> {
        if self.social_authority.is_none() {
            return Err(ConnectError::Disabled);
        }
        let valid_session = uuid::Uuid::parse_str(&request.session_id)
            .is_ok_and(|id| id.get_version_num() == 4 && id.to_string() == request.session_id);
        let role = match request.role.as_str() {
            "host"
                if ((selection.is_none()
                    && (trusted_native_source
                        || self
                            .social_authority
                            .as_ref()
                            .is_some_and(|authority| authority.online_origin.is_none()))
                    && request.game_target.as_deref() == Some("127.0.0.1:43001"))
                    || (selection.is_some() && request.game_target.is_none()))
                    && matches!(request.visibility.as_deref(), Some("public" | "friends")) =>
            {
                "host"
            }
            "guest"
                if selection.is_none()
                    && request.game_target.is_none()
                    && request.visibility.is_none() =>
            {
                "guest"
            }
            _ => return Err(ConnectError::InvalidRequest),
        };
        if owner != "main" || !valid_uid(&request.account_id) || !valid_session {
            return Err(ConnectError::InvalidRequest);
        }
        let mut state = lock(&self.state)?;
        if state.shutting_down || state.social_epoch != request.account_epoch {
            return Err(ConnectError::StaleOperation);
        }
        if let Some(a) = &mut state.attempt {
            expire_pending(a);
            ready_game::refresh_cleanup(a);
            if !terminal(a) {
                return Err(ConnectError::Busy);
            }
        }
        state.generation = state
            .generation
            .max(state.attempt.as_ref().map_or(0, |a| a.snapshot.generation))
            .saturating_add(1);
        let reservation = SocialReservation {
            operation_id: uuid::Uuid::new_v4().to_string(),
            generation: state.generation,
        };
        let mut snapshot = ConnectSnapshot::idle();
        snapshot.operation_id = Some(reservation.operation_id.clone());
        snapshot.generation = reservation.generation;
        snapshot.role = Some(role);
        snapshot.phase = "starting";
        snapshot.social = Some(SocialSnapshot {
            session_id: request.session_id.clone(),
            role: role.into(),
            stage: "reserved",
            digest: None,
            game_target: request.game_target.clone(),
            visibility: request.visibility.clone(),
            expires_at_ms: Some(lab::unix_millis() + 60_000),
            ready_game_launch_available: None,
            voice_profile: None,
            voice_state: None,
            host_player_voice_state: None,
            host_player: None,
            max_guests: None,
            participants: None,
            runtime_profile: None,
        });
        state.attempt = Some(Attempt {
            #[cfg(all(windows, feature = "connect-voice-lab"))]
            native_voice: None,
            owner: owner.into(),
            snapshot,
            social: Some(Control {
                request,
                key: Some(lab::generate_key()),
                reservation_generation: reservation.generation,
                approval: None,
                selection,
                token: None,
                revocations: std::collections::HashSet::new(),
                #[cfg(all(windows, feature = "connect-voice-lab"))]
                native_voice_source: None,
                #[cfg(all(windows, feature = "connect-voice-lab"))]
                native_voice_local_key: None,
                #[cfg(test)]
                guard_probe: None,
            }),
            draft: None,
            reviewed: None,
            prepared_deadline: Instant::now() + Duration::from_secs(60),
            social_key_proof_emitted: false,
            cancel: CancellationToken::new(),
            worker: None,
            export: None,
            host: None,
            probe_active: false,
            probes: 0,
            last_probe: None,
            session_deadline: None,
            admission_deadline: None,
            authority: None,
            recovery: None,
            group: None,
            group_source: None,
            host_source: None,
            host_review: None,
            guest_source: None,
            ready_game: None,
            #[cfg(test)]
            recovery_gate: None,
            #[cfg(test)]
            group_start_gate: None,
        });
        Ok(reservation)
    }
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    pub(in crate::engine::connect) fn social_reserve_native_voice(
        &self,
        owner: &str,
        request: SocialDiscoveredReserveRequest,
        binding: crate::engine::connect_voice::dedicated::NativeVoiceHostBinding,
        host_player: bool,
    ) -> Result<SocialReservation, ConnectError> {
        if !self
            .social_authority
            .as_ref()
            .is_some_and(|a| a.online_origin.is_some())
        {
            return Err(ConnectError::Disabled);
        }
        binding
            .online_voice_policy()
            .map_err(|_| ConnectError::SourceChanged)?;
        let reservation = self.social_reserve_inner(
            owner,
            SocialReserveRequest {
                account_epoch: request.account_epoch,
                account_id: request.account_id,
                session_id: request.session_id,
                role: "host".into(),
                game_target: Some(binding.targets().0.to_string()),
                visibility: Some(request.visibility),
            },
            None,
            true,
        )?;
        let mut state = lock(&self.state)?;
        let attempt = selected(&mut state, owner, &reservation.operation_id)?;
        let control = attempt
            .social
            .as_mut()
            .ok_or(ConnectError::InvalidRequest)?;
        control.native_voice_source = Some(binding);
        control.native_voice_local_key = host_player.then(lab::generate_key);
        Ok(reservation)
    }
    pub(in crate::engine::connect) fn social_authenticate(
        &self,
        owner: &str,
        id: &str,
        generation: u64,
        id_token: String,
    ) -> Result<ConnectSnapshot, ConnectError> {
        let authority = self
            .social_authority
            .clone()
            .ok_or(ConnectError::Disabled)?;
        if id_token.is_empty()
            || id_token.len() > 8192
            || !id_token.is_ascii()
            || id_token.bytes().any(|b| b <= 32 || b == 127)
        {
            return Err(ConnectError::InvalidRequest);
        }
        let mut state = lock(&self.state)?;
        let epoch = state.social_epoch;
        let a = selected(&mut state, owner, id)?;
        expire(a);
        if a.cancel.is_cancelled() || a.worker.is_some() {
            return Err(ConnectError::StaleOperation);
        }
        let control = a.social.as_mut().ok_or(ConnectError::InvalidRequest)?;
        if generation != control.reservation_generation || control.request.account_epoch != epoch {
            return Err(ConnectError::StaleOperation);
        }
        let key = control.key.take().ok_or(ConnectError::StaleOperation)?;
        let request = control.request.clone();
        let selection = control.selection.take();
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        let native_voice_source = control.native_voice_source.take();
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        let native_voice_local_key = control.native_voice_local_key.take();
        #[cfg(test)]
        let guard_probe = control.guard_probe.clone();
        let deadline = a.prepared_deadline;
        let expiry = a
            .snapshot
            .social
            .as_ref()
            .and_then(|s| s.expires_at_ms)
            .ok_or(ConnectError::Internal)?;
        let cancel = a.cancel.clone();
        if let Some(social) = &mut a.snapshot.social {
            social.stage = "authenticating";
        }
        a.snapshot.cleanup_state = "pending";
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        let worker_state = self.state.clone();
        let operation = id.to_owned();
        a.worker = Some(tokio::spawn(async move {
            let run = Run {
                state: worker_state.clone(),
                id: operation.clone(),
                generation,
                request,
                selection,
                #[cfg(all(windows, feature = "connect-voice-lab"))]
                native_voice_source,
                #[cfg(all(windows, feature = "connect-voice-lab"))]
                native_voice_local_key,
                key,
                authority,
                deadline,
                expiry,
                cancel: cancel.clone(),
                #[cfg(all(test, windows))]
                source_gate: None,
                #[cfg(test)]
                guard_probe,
            };
            let result = execute(run, id_token).await;
            close_operation(&worker_state, &operation, &cancel, execution_error(result)).await;
        }));
        Ok(a.snapshot.clone())
    }
    pub(in crate::engine::connect) fn social_refresh_authentication(
        &self,
        owner: &str,
        id: &str,
        account_epoch: u64,
        id_token: String,
    ) -> Result<(), ConnectError> {
        if !self
            .social_authority
            .as_ref()
            .is_some_and(|a| a.online_origin.is_some())
        {
            return Err(ConnectError::Disabled);
        }
        let mut state = lock(&self.state)?;
        if state.social_epoch != account_epoch {
            return Err(ConnectError::StaleOperation);
        }
        let attempt = selected(&mut state, owner, id)?;
        if attempt.cancel.is_cancelled() || attempt.worker.is_none() {
            return Err(ConnectError::StaleOperation);
        }
        let control = attempt
            .social
            .as_ref()
            .ok_or(ConnectError::InvalidRequest)?;
        if control.request.account_epoch != account_epoch
            || !online_token_destination(&id_token, &control.request.account_id)
        {
            return Err(ConnectError::InvitationRejected);
        }
        let token = control.token.as_ref().ok_or(ConnectError::StaleOperation)?;
        *token.lock().map_err(|_| ConnectError::Internal)? = id_token;
        Ok(())
    }
    pub(in crate::engine::connect) fn social_confirm_host(
        &self,
        owner: &str,
        id: &str,
        generation: u64,
        digest: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        let mut state = lock(&self.state)?;
        let a = selected(&mut state, owner, id)?;
        if a.snapshot.generation != generation {
            return Err(ConnectError::StaleOperation);
        }
        if a.cancel.is_cancelled() || Instant::now() >= a.prepared_deadline {
            return Err(ConnectError::PlanExpired);
        }
        let social = a
            .snapshot
            .social
            .as_ref()
            .ok_or(ConnectError::InvalidRequest)?;
        if social.stage != "hostReview" || social.digest.as_deref() != Some(digest) {
            return Err(ConnectError::WrongDigest);
        }
        a.social
            .as_mut()
            .and_then(|s| s.approval.take())
            .ok_or(ConnectError::StaleOperation)?
            .send(digest.into())
            .map_err(|_| ConnectError::StaleOperation)?;
        if let Some(social) = &mut a.snapshot.social {
            social.digest = None;
            social.stage = "connecting";
        }
        a.snapshot.phase = "starting";
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        Ok(a.snapshot.clone())
    }
    pub(in crate::engine::connect) async fn account_transition_stop(
        &self,
        owner: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        let operation = {
            let mut state = lock(&self.state)?;
            if let Some(a) = &state.attempt {
                if a.owner != owner {
                    return Err(ConnectError::OwnerMismatch);
                }
            }
            state.social_epoch = state
                .social_epoch
                .checked_add(1)
                .ok_or(ConnectError::Internal)?;
            state
                .attempt
                .as_ref()
                .and_then(|a| a.snapshot.operation_id.clone())
        };
        match operation {
            Some(id) => self.stop(owner, &id).await,
            None => Ok(ConnectSnapshot::idle()),
        }
    }
}

struct Run {
    state: Arc<Mutex<State>>,
    id: String,
    generation: u64,
    request: SocialReserveRequest,
    selection: Option<DiscoverySelection>,
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    native_voice_source: Option<crate::engine::connect_voice::dedicated::NativeVoiceHostBinding>,
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    native_voice_local_key: Option<lab::SecretKey>,
    key: lab::SecretKey,
    authority: SocialLabAuthority,
    deadline: Instant,
    expiry: u64,
    cancel: CancellationToken,
    #[cfg(all(test, windows))]
    source_gate: Option<Arc<group_source::SourceGate>>,
    #[cfg(test)]
    guard_probe: Option<Arc<ready_source::fixture::Pause>>,
}
impl Run {
    fn requested_revocations(&self) -> Result<std::collections::HashSet<String>, ConnectError> {
        Ok(lock(&self.state)?
            .attempt
            .as_ref()
            .filter(|attempt| attempt.snapshot.operation_id.as_deref() == Some(self.id.as_str()))
            .and_then(|attempt| attempt.social.as_ref())
            .map(|control| control.revocations.clone())
            .unwrap_or_default())
    }
    fn current(&self) -> Result<(), ConnectError> {
        if self.cancel.is_cancelled() {
            Err(ConnectError::Cancelled)
        } else if Instant::now() >= self.deadline || lab::unix_millis() >= self.expiry {
            Err(ConnectError::PlanExpired)
        } else {
            Ok(())
        }
    }
    fn stage(&self, stage: &'static str, phase: &'static str) {
        project_stage(&self.state, &self.id, stage, phase);
    }
}
fn execution_error(result: Result<(), ConnectError>) -> Option<ConnectError> {
    result
        .err()
        .filter(|error| *error != ConnectError::Cancelled)
}
fn project_stage(state: &Mutex<State>, id: &str, stage: &'static str, phase: &'static str) {
    let Ok(mut state) = state.lock() else {
        return;
    };
    let Some(a) = state.attempt.as_mut() else {
        return;
    };
    if a.snapshot.operation_id.as_deref() != Some(id) || a.cancel.is_cancelled() {
        return;
    }
    let Some(social) = a.snapshot.social.as_mut() else {
        return;
    };
    let admitted = stage == "connected" && a.snapshot.role == Some("host");
    if a.snapshot.phase == phase
        && social.stage == stage
        && (!admitted || a.snapshot.transport_state == "admitted")
    {
        return;
    }
    a.snapshot.phase = phase;
    social.stage = stage;
    if admitted {
        a.snapshot.transport_state = "admitted";
    }
    if !matches!(phase, "hostListening" | "guestConnected") {
        path_projection::clear(a);
    }
    a.snapshot.generation = a.snapshot.generation.saturating_add(1);
}
fn interval(duration: Duration) -> tokio::time::Interval {
    let mut interval = tokio::time::interval(duration);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval
}
#[derive(Clone)]
struct Http {
    client: reqwest::Client,
    token: Arc<Mutex<String>>,
    cancel: CancellationToken,
    deadline: Instant,
    origin: &'static str,
    session_id: String,
}
impl Http {
    fn new(token: String, run: &Run) -> Result<Self, ConnectError> {
        if !(if run.authority.online_origin.is_some() {
            online_token_destination(&token, &run.request.account_id)
        } else {
            local_emulator_token(&token, &run.request.account_id)
        }) {
            return Err(ConnectError::InvitationRejected);
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(0)
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|_| ConnectError::Internal)?;
        let token = Arc::new(Mutex::new(token));
        update(&run.state, &run.id, |a| {
            if let Some(control) = &mut a.social {
                control.token = Some(token.clone());
            }
        });
        Ok(Self {
            client,
            token,
            cancel: run.cancel.clone(),
            deadline: run.deadline,
            origin: run.authority.online_origin.unwrap_or(ORIGIN),
            session_id: run.request.session_id.clone(),
        })
    }
    async fn post(&self, path: &'static str, body: Value) -> Result<Value, ConnectError> {
        let token = self
            .token
            .lock()
            .map_err(|_| ConnectError::Internal)?
            .clone();
        let work = async {
            let mut response = self
                .client
                .post(format!("{}{path}", self.origin))
                .header("X-Aurora-Session", &self.session_id)
                .bearer_auth(&token)
                .json(&body)
                .send()
                .await
                .map_err(|_| ConnectError::TransportFailed)?;
            if !response.status().is_success() {
                if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    return Err(ConnectError::SocialQuota);
                }
                if response.status().is_server_error() {
                    return Err(ConnectError::SocialUnavailable);
                }
                return Err(ConnectError::InvitationRejected);
            }
            if response
                .content_length()
                .is_some_and(|n| n > MAX_HTTP_BYTES as u64)
            {
                return Err(ConnectError::InvitationRejected);
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| ConnectError::TransportFailed)?
            {
                if bytes.len().saturating_add(chunk.len()) > MAX_HTTP_BYTES {
                    return Err(ConnectError::InvitationRejected);
                }
                bytes.extend_from_slice(&chunk);
            }
            serde_json::from_slice(&bytes).map_err(|_| ConnectError::InvitationRejected)
        };
        tokio::select! { biased;
            _=self.cancel.cancelled()=>Err(ConnectError::Cancelled),
            _=tokio::time::sleep_until(self.deadline.into())=>Err(ConnectError::PlanExpired),
            result=work=>result,
        }
    }
    async fn stop(&self, path: &'static str, body: Value) {
        let stopping = Self {
            cancel: CancellationToken::new(),
            deadline: Instant::now() + Duration::from_secs(2),
            ..self.clone()
        };
        let _ = stopping.post(path, body).await;
    }
}
fn field<'a>(value: &'a Value, name: &str) -> Result<&'a str, ConnectError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .ok_or(ConnectError::InvitationRejected)
}
fn packet(value: &Value, name: &str) -> Result<String, ConnectError> {
    let v = value.get(name).ok_or(ConnectError::InvitationRejected)?;
    let text = v.to_string();
    if text.len() > lab::MAX_SOCIAL_ADMISSION_BYTES {
        Err(ConnectError::InvitationRejected)
    } else {
        Ok(text)
    }
}
async fn pause(run: &Run, duration: Duration) -> Result<(), ConnectError> {
    tokio::select! {biased; _=run.cancel.cancelled()=>Err(ConnectError::Cancelled),
    _=tokio::time::sleep_until(run.deadline.into())=>Err(ConnectError::PlanExpired),
    _=tokio::time::sleep(duration)=>run.current(), }
}
async fn challenge(
    run: &Run,
    http: &Http,
    expected: Option<&lab::VerifiedSocialJoinContext>,
) -> Result<(lab::SocialKeyChallenge, lab::SocialKeyBindingProof), ConnectError> {
    run.current()?;
    let value = http.post("/v1/social/key/challenge", json!({"schemaVersion":1,"operationId":run.id,"generation":run.generation,
        "role":run.request.role,"publicKey":run.key.public().to_string(),"sessionId":run.request.session_id,
        "preparedExpiresAtUnixMs":run.expiry})).await?;
    let challenge = lab::SocialKeyChallenge::parse(&value.to_string())
        .map_err(|_| ConnectError::InvitationRejected)?;
    if challenge.account_id() != run.request.account_id
        || challenge.session_id() != run.request.session_id
        || challenge.operation_id() != run.id
        || challenge.generation() != run.generation
        || challenge.role().as_str() != run.request.role
        || challenge.public_key() != run.key.public().to_string()
        || challenge.issuer_instance() != run.authority.issuer
        || challenge.expires_at_unix_ms() > run.expiry
    {
        return Err(ConnectError::InvitationRejected);
    }
    run.current()?;
    if let Some(expected) = expected {
        expected
            .check_current()
            .map_err(|_| ConnectError::PlanExpired)?;
        expected
            .check_challenge(&challenge)
            .map_err(|_| ConnectError::InvitationRejected)?;
    }
    let proof = challenge
        .sign(&run.key)
        .map_err(|_| ConnectError::InvitationRejected)?;
    run.current()?;
    if let Some(expected) = expected {
        expected
            .check_current()
            .map_err(|_| ConnectError::PlanExpired)?;
    }
    challenge
        .validate()
        .map_err(|_| ConnectError::PlanExpired)?;
    Ok((challenge, proof))
}
async fn execute(mut run: Run, token: String) -> Result<(), ConnectError> {
    let http = Http::new(token, &run)?;
    if run.authority.online_origin.is_some() {
        let bootstrap = http.post("/v1/social/bootstrap", json!({})).await?;
        if bootstrap["schemaVersion"] != 1
            || field(&bootstrap, "accountId")? != run.request.account_id
            || field(&bootstrap, "sessionId")? != run.request.session_id
            || !bootstrap["expiresAtUnixMs"]
                .as_u64()
                .is_some_and(|expiry| expiry > lab::unix_millis())
        {
            return Err(ConnectError::InvitationRejected);
        }
        let validated = SocialLabAuthority::from_local_process(
            field(&bootstrap, "admissionPublicKey")?,
            field(&bootstrap, "issuerInstance")?,
        )?;
        let policy = field(&bootstrap, "joinPolicy")?;
        if !matches!(policy, "open" | "approval") {
            return Err(ConnectError::InvitationRejected);
        }
        run.authority.pin = validated.pin;
        run.authority.issuer = validated.issuer;
        run.authority.join_policy = policy.into();
        run.authority.session_expiry = bootstrap["expiresAtUnixMs"]
            .as_u64()
            .ok_or(ConnectError::InvitationRejected)?;
        run.authority.max_players = bootstrap["maxPlayers"]
            .as_u64()
            .filter(|maximum| (1..=100).contains(maximum))
            .ok_or(ConnectError::InvitationRejected)? as u8;
        run.current()?;
    }
    let identity = http.post("/v1/social/identity", json!({})).await?;
    if field(&identity, "accountId")? != run.request.account_id
        || field(&identity, "issuerInstance")? != run.authority.issuer
    {
        return Err(ConnectError::InvitationRejected);
    }
    run.current()?;
    if run.request.role == "host" {
        host(run, http).await
    } else {
        guest(run, http).await
    }
}
fn limits() -> lab::Limits {
    lab::Limits {
        handshake_timeout: Duration::from_secs(2),
        ..Default::default()
    }
}

/// A blocking source query belongs to the social actor. It captures no endpoint;
/// cancellation releases transport before awaiting the exact retained query.
async fn source_query<T: Send + 'static>(
    run: &Run,
    host: &mut Option<lab::Host>,
    lease: Option<&str>,
    permitted: impl Fn() -> Result<(), ConnectError>,
    query: impl FnOnce() -> Result<T, ConnectError> + Send + 'static,
) -> Result<T, ConnectError> {
    run.current()?;
    permitted()?;
    let mut query = tokio::task::spawn_blocking(query);
    let mut tick = interval(Duration::from_millis(20));
    let failure = loop {
        tokio::select! { biased;
            _ = run.cancel.cancelled() => break ConnectError::Cancelled,
            _ = tick.tick() => {
                if let Err(error) = run.current().and_then(|_| permitted()) { break error; }
            },
            result = &mut query => {
                let result = result.unwrap_or(Err(ConnectError::Internal));
                return run.current().and_then(|_| permitted()).and(result);
            },
        }
    };
    if let Some(host) = host.take() {
        if let Some(lease) = lease {
            host.revoke(lease);
        }
        host.shutdown().await;
        drop(host);
    }
    let _ = query.await;
    Err(failure)
}

async fn discovered_source(run: &Run) -> Result<(TargetGuard, SocketAddr), ConnectError> {
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    if let Some(binding) = &run.native_voice_source {
        online::voice::probe(run, binding, None).await?;
        return Ok((TargetGuard::Manual, binding.targets().0));
    }
    let Some(selection) = run.selection.clone() else {
        return Ok((
            TargetGuard::Manual,
            "127.0.0.1:43001".parse().expect("fixed local source"),
        ));
    };
    #[cfg(windows)]
    {
        run.stage("sourceChecking", "starting");
        #[cfg(test)]
        let gate = run.source_gate.clone();
        source_query(
            run,
            &mut None,
            None,
            || Ok(()),
            move || {
                #[cfg(test)]
                if let Some(gate) = gate {
                    gate.wait();
                }
                let active = selection
                    .manager
                    .take_target("main", &selection.id)
                    .and_then(|target| target.activate())
                    .map_err(|_| ConnectError::SourceChanged)?;
                let target = active
                    .recheck_owned()
                    .map_err(|_| ConnectError::SourceChanged)?;
                // Existing social policy permits only the exact IPv4 loopback.
                lab::social_source_digest(target).map_err(|_| ConnectError::InvalidRequest)?;
                Ok((TargetGuard::Owned(Arc::new(active)), target))
            },
        )
        .await
    }
    #[cfg(not(windows))]
    {
        let _ = selection;
        Err(ConnectError::Disabled)
    }
}

async fn check_source(
    run: &Run,
    source: &TargetGuard,
    host: &mut Option<lab::Host>,
    lease: Option<&str>,
    permitted: impl Fn() -> Result<(), ConnectError>,
) -> Result<(), ConnectError> {
    if matches!(source, TargetGuard::Manual) {
        return run.current().and_then(|_| permitted());
    }
    let source = source.clone();
    source_query(run, host, lease, permitted, move || {
        source.recheck().map(|_| ())
    })
    .await
}

async fn host(mut run: Run, mut http: Http) -> Result<(), ConnectError> {
    let (source_guard, source) = discovered_source(&run).await?;
    let (challenge, proof) = challenge(&run, &http, None).await?;
    check_source(&run, &source_guard, &mut None, None, || Ok(())).await?;
    let visibility = run
        .request
        .visibility
        .as_deref()
        .ok_or(ConnectError::InvalidRequest)?;
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    let voice_policy = if let Some(binding) = &run.native_voice_source {
        let (profile, digest) = binding
            .online_voice_policy()
            .map_err(|_| ConnectError::SourceChanged)?;
        let expiry = run.authority.session_expiry.min(
            lab::unix_millis().saturating_add(
                binding
                    .deadline()
                    .saturating_duration_since(Instant::now())
                    .as_millis() as u64,
            ),
        );
        Some(
            lab::SocialHostPolicy::review_online_voice(
                &challenge,
                visibility,
                source,
                &run.authority.join_policy,
                expiry,
                profile,
                &digest,
                run.native_voice_local_key.is_some(),
            )
            .and_then(|policy| {
                policy.with_runtime_profile(
                    binding
                        .online_runtime_profile()
                        .map_err(|_| lab::SocialAdmissionError::Context)?
                        .version_id(),
                )
            })
            .map_err(|_| ConnectError::InvitationRejected)?,
        )
    } else {
        None
    };
    #[cfg(not(all(windows, feature = "connect-voice-lab")))]
    let voice_policy: Option<lab::SocialHostPolicy> = None;
    let policy = if let Some(policy) = voice_policy {
        Ok(policy)
    } else if run.authority.online_origin.is_some() {
        lab::SocialHostPolicy::review_online(
            &challenge,
            visibility,
            source,
            &run.authority.join_policy,
            run.authority.session_expiry,
        )
    } else {
        lab::SocialHostPolicy::review_with_join_policy(
            &challenge,
            visibility,
            source,
            &run.authority.join_policy,
        )
    }
    .map_err(|_| ConnectError::InvitationRejected)?;
    let policy = if run.authority.online_origin.is_some() {
        let maximum = policy.max_guests().min(
            run.authority
                .max_players
                .saturating_sub(u8::from(policy.host_player())),
        );
        policy
            .with_guest_limit(maximum)
            .map_err(|_| ConnectError::InvalidRequest)?
    } else {
        policy
    };
    let digest = policy.digest();
    let (sender, receiver) = oneshot::channel();
    update(&run.state, &run.id, |a| {
        if !a.cancel.is_cancelled() {
            if let Some(control) = &mut a.social {
                control.approval = Some(sender);
            }
            if let Some(social) = &mut a.snapshot.social {
                social.stage = "hostReview";
                social.digest = Some(digest.clone());
                social.game_target = Some(source.to_string());
                social.expires_at_ms = Some(policy.expires_at_unix_ms());
                social.max_guests = Some(policy.max_guests());
                social.runtime_profile = policy.runtime_profile().map(str::to_owned);
                #[cfg(all(windows, feature = "connect-voice-lab"))]
                if let Some(binding) = &run.native_voice_source {
                    if let Ok((profile, _)) = binding.online_voice_policy() {
                        social.voice_profile = Some(profile.into());
                        social.voice_state = Some("notStarted");
                        social.host_player = Some(run.native_voice_local_key.is_some());
                    }
                }
            }
            a.snapshot.phase = "reviewed";
        }
    });
    let approved = tokio::select! {biased; _=run.cancel.cancelled()=>return Err(ConnectError::Cancelled),
    _=tokio::time::sleep_until(run.deadline.into())=>return Err(ConnectError::PlanExpired),
    result=receiver=>result.map_err(|_|ConnectError::Cancelled)?, };
    run.current()?;
    check_source(&run, &source_guard, &mut None, None, || Ok(())).await?;
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    if let Some(binding) = &run.native_voice_source {
        online::voice::probe(&run, binding, None).await?;
    }
    let policy = policy
        .approve(&run.key, &approved, run.deadline)
        .map_err(|_| ConnectError::WrongDigest)?;
    if run.authority.online_origin.is_some() {
        let expiry = policy.body().expires_at_unix_ms();
        if expiry >= run.expiry {
            run.deadline += Duration::from_millis(expiry - run.expiry);
        } else {
            run.deadline -= Duration::from_millis(run.expiry - expiry);
        }
        run.expiry = expiry;
        http.deadline = run.deadline;
    }
    let policy_id = policy.body().policy_id().to_owned();
    let registration = http
        .post(
            "/v1/social/host/register",
            json!({"proof":proof,"policySigned":policy}),
        )
        .await;
    let result = async {
        let registration = registration?;
        if field(&registration, "policyId")? != policy_id
            || field(&registration, "policyDigest")? != digest
        {
            return Err(ConnectError::InvitationRejected);
        }
        run.stage("waitingGuest", "hostListening");
        if run.authority.online_origin.is_some() {
            online::host_registered(
                &run,
                &http,
                &challenge,
                &policy,
                source,
                &source_guard,
                &policy_id,
            )
            .await
        } else {
            host_registered(
                &run,
                &http,
                &challenge,
                &policy,
                source,
                &source_guard,
                &policy_id,
            )
            .await
        }
    }
    .await;
    http.stop("/v1/social/host/stop", json!({"policyId":policy_id}))
        .await;
    result
}
async fn host_registered(
    run: &Run,
    http: &Http,
    challenge: &lab::SocialKeyChallenge,
    policy: &lab::SignedSocialHostPolicy,
    source: SocketAddr,
    source_guard: &TargetGuard,
    policy_id: &str,
) -> Result<(), ConnectError> {
    let mut source_checked = Instant::now();
    let delivery = loop {
        if source_checked.elapsed() >= Duration::from_secs(1) {
            source_checked = Instant::now();
            check_source(run, source_guard, &mut None, None, || Ok(())).await?;
        }
        let packet = http
            .post("/v1/social/host/poll", json!({"policyId":policy_id}))
            .await?;
        match field(&packet, "state")? {
            "admitted" => break packet,
            "waiting" => pause(run, Duration::from_millis(500)).await?,
            _ => return Err(ConnectError::InvitationRejected),
        }
    };
    let grant = lab::VerifiedSocialGrant::for_host(
        &packet(&delivery, "grant")?,
        run.authority.pin,
        challenge,
        policy,
        source,
        run.deadline,
    )
    .map_err(|_| ConnectError::InvitationRejected)?;
    let mut continuity = lab::SocialContinuity::start(&packet(&delivery, "initialLease")?, &grant)
        .map_err(|_| ConnectError::InvitationRejected)?;
    if delivery["lease"]["body"]["sequence"]
        .as_u64()
        .is_some_and(|sequence| sequence > continuity.sequence())
    {
        continuity
            .update(&packet(&delivery, "lease")?)
            .map_err(|_| ConnectError::InvitationRejected)?;
    }
    let reservation =
        std::net::UdpSocket::bind("127.0.0.1:0").map_err(|_| ConnectError::PortUnavailable)?;
    let bind = reservation
        .local_addr()
        .map_err(|_| ConnectError::PortUnavailable)?;
    let service = game(source);
    let invitation = lab::SessionInvitation::issue_loopback(
        &run.key,
        grant.guest_key(),
        grant.session_id(),
        bind,
        std::slice::from_ref(&service),
        lab::SessionDeadlines {
            admission_expires_at_unix_ms: grant.admission_expires_at_unix_ms(),
            session_expires_at_unix_ms: grant.session_expires_at_unix_ms(),
        },
    )
    .map_err(|_| ConnectError::InvitationRejected)?;
    run.current()?;
    grant
        .check_admission()
        .map_err(|_| ConnectError::PlanExpired)?;
    continuity.check().map_err(|_| ConnectError::PlanExpired)?;
    check_source(run, source_guard, &mut None, None, || {
        grant
            .check_admission()
            .and_then(|_| continuity.check())
            .map_err(|_| ConnectError::PlanExpired)
    })
    .await?;
    drop(reservation);
    // Keep the owned bind future alive through cancellation; late results close.
    let starting = lab::Host::start_invited_session(
        lab::HostConfig {
            secret_key: run.key.clone(),
            bind_addr: bind,
            session_id: grant.session_id().into(),
            services: vec![service],
            leases: vec![invitation.lease()],
            limits: limits(),
        },
        &invitation,
    );
    tokio::pin!(starting);
    let mut interrupted = false;
    let mut tick = interval(Duration::from_millis(20));
    let host=loop {tokio::select!{biased;
        _=run.cancel.cancelled()=>{interrupted=true;break starting.as_mut().await;},
        _=tick.tick()=>if run.current().is_err()||continuity.check().is_err()||grant.check_admission().is_err(){interrupted=true;break starting.as_mut().await;},
        result=&mut starting=>break result,
    }}.map_err(|_|if run.cancel.is_cancelled(){ConnectError::Cancelled}else{ConnectError::TransportFailed})?;
    let sockets = host.bound_sockets();
    let mut host = Some(host);
    let checked = check_source(
        run,
        source_guard,
        &mut host,
        Some(invitation.invitation_id()),
        || {
            grant
                .check_admission()
                .and_then(|_| continuity.check())
                .map_err(|_| ConnectError::PlanExpired)
        },
    )
    .await;
    let result = if let Err(error) = checked {
        Err(error)
    } else if interrupted || run.current().is_err() || continuity.check().is_err() {
        Err(if run.cancel.is_cancelled() {
            ConnectError::Cancelled
        } else {
            ConnectError::PlanExpired
        })
    } else {
        host_active(
            run,
            http,
            &mut host,
            &grant,
            &invitation,
            &mut continuity,
            HostPolicyScope {
                id: policy_id,
                source: source_guard,
            },
        )
        .await
    };
    if let Some(host) = host {
        host.revoke(invitation.invitation_id());
        host.shutdown().await;
        drop(host);
    }
    release(&sockets, None).await?;
    result
}
struct HostPolicyScope<'a> {
    id: &'a str,
    source: &'a TargetGuard,
}
async fn host_active(
    run: &Run,
    http: &Http,
    host: &mut Option<lab::Host>,
    grant: &lab::VerifiedSocialGrant,
    invitation: &lab::SessionInvitation,
    continuity: &mut lab::SocialContinuity,
    policy: HostPolicyScope<'_>,
) -> Result<(), ConnectError> {
    let publishing = http.post(
        "/v1/social/host/publish",
        json!({"grantId":grant.grant_id(),"invitation":invitation.encode()}),
    );
    tokio::pin!(publishing);
    let mut publish_tick = interval(Duration::from_millis(20));
    let published = loop {
        tokio::select! {biased;
            _=run.cancel.cancelled()=>return Ok(()),
            _=publish_tick.tick()=>{run.current()?;continuity.check().map_err(|_|ConnectError::PlanExpired)?;grant.check_admission().map_err(|_|ConnectError::PlanExpired)?;},
            result=&mut publishing=>break result?,
        }
    };
    if published.get("accepted") != Some(&Value::Bool(true)) {
        return Err(ConnectError::InvitationRejected);
    }
    update(&run.state, &run.id, |a| {
        if !a.cancel.is_cancelled() {
            a.snapshot.transport_state = "listening";
            a.snapshot.expires_at_ms = Some(grant.session_expires_at_unix_ms());
        }
    });
    let mut admitted = false;
    let mut tick = interval(Duration::from_millis(20));
    let mut poll = interval(Duration::from_millis(500));
    let mut source_checked = Instant::now();
    loop {
        tokio::select! {biased;
            _=run.cancel.cancelled()=>return Ok(()),
            _=tick.tick()=>{
                run.current()?;continuity.check().map_err(|_|ConnectError::PlanExpired)?;
                let live_host = host.as_ref().ok_or(ConnectError::Cancelled)?;
                admitted|=live_host.invitation_consumed(invitation.invitation_id());
                if admitted{run.stage("connected","hostListening");}
                if !admitted{grant.check_admission().map_err(|_|ConnectError::PlanExpired)?;}
                if admitted&&!live_host.invitation_active(invitation.invitation_id()){return Ok(());}
                if source_checked.elapsed() >= Duration::from_secs(1) {
                    source_checked = Instant::now();
                    check_source(run, policy.source, host, Some(invitation.invitation_id()), || {
                        grant.check_session().and_then(|_| continuity.check()).map_err(|_| ConnectError::PlanExpired)
                    }).await?;
                }
            },
            _=poll.tick()=>{
                let querying=http.post("/v1/social/host/poll",json!({"policyId":policy.id}));tokio::pin!(querying);
                let result=loop{tokio::select!{biased;
                    _=run.cancel.cancelled()=>return Ok(()),
                    _=tick.tick()=>{run.current()?;continuity.check().map_err(|_|ConnectError::PlanExpired)?;},
                    result=&mut querying=>break result?,
                }};
                if field(&result,"state")?!="admitted" {return Err(ConnectError::InvitationRejected);}
                let lease=packet(&result,"lease")?;
                let sequence=result["lease"]["body"]["sequence"].as_u64().ok_or(ConnectError::InvitationRejected)?;
                if sequence>continuity.sequence(){continuity.update(&lease).map_err(|_|ConnectError::InvitationRejected)?;}
            },
        }
    }
}

async fn guest(mut run: Run, mut http: Http) -> Result<(), ConnectError> {
    let context = http
        .post(
            "/v1/social/context",
            json!({"sessionId":run.request.session_id}),
        )
        .await?;
    let expected = lab::VerifiedSocialJoinContext::verify(
        &context.to_string(),
        run.authority.pin,
        &run.request.account_id,
        &run.request.session_id,
        run.deadline,
    )
    .map_err(|_| ConnectError::InvitationRejected)?;
    if field(&context["body"], "issuerInstance")? != run.authority.issuer {
        return Err(ConnectError::InvitationRejected);
    }
    let policy_id = field(&context["body"], "policyId")?.to_owned();
    let (challenge, proof) = challenge(&run, &http, Some(&expected)).await?;
    expected
        .check_challenge(&challenge)
        .map_err(|_| ConnectError::InvitationRejected)?;
    expected
        .check_current()
        .map_err(|_| ConnectError::PlanExpired)?;
    let joined = http
        .post(
            "/v1/social/join",
            json!({"proof":proof,"policyId":policy_id}),
        )
        .await?;
    let verify = if run.authority.online_origin.is_some() {
        lab::VerifiedSocialGrant::for_guest_online
    } else {
        lab::VerifiedSocialGrant::for_guest
    };
    let grant = verify(
        &packet(&joined, "grant")?,
        run.authority.pin,
        &challenge,
        &expected,
        run.deadline,
    )
    .map_err(|_| ConnectError::InvitationRejected)?;
    if run.authority.online_origin.is_some() {
        run.deadline = grant.session_deadline();
        run.expiry = grant.session_expires_at_unix_ms();
        http.deadline = run.deadline;
        update(&run.state, &run.id, |a| {
            if let Some(social) = &mut a.snapshot.social {
                social.expires_at_ms = Some(run.expiry);
            }
        });
    }
    let first_lease = packet(&joined, "lease")?;
    let mut continuity = lab::SocialContinuity::start(&first_lease, &grant)
        .map_err(|_| ConnectError::InvitationRejected)?;
    let local_continuity = lab::SocialContinuity::start(&first_lease, &grant)
        .map_err(|_| ConnectError::InvitationRejected)?;
    let authority = ready_source::LaunchAuthority::new(&run, grant.clone(), local_continuity)?;
    let heartbeat_guard = authority.heartbeat_guard();
    let grant_id = grant.grant_id().to_owned();
    let heartbeat_cancel = CancellationToken::new();
    let heartbeat_signal = heartbeat_cancel.clone();
    let heartbeat_http = http.clone();
    let heartbeat_id = grant_id.clone();
    let (sender, receiver) = tokio::sync::watch::channel(first_lease);
    let mut heartbeat = tokio::spawn(async move {
        let _authority_guard = heartbeat_guard;
        loop {
            tokio::select! {biased;_=heartbeat_signal.cancelled()=>return Ok(()),_=tokio::time::sleep(Duration::from_millis(750))=>{}};
            let result = tokio::select! {biased;_=heartbeat_signal.cancelled()=>return Ok(()),result=heartbeat_http.post("/v1/social/continuity",json!({"grantId":heartbeat_id}))=>result?};
            continuity
                .update(&result.to_string())
                .map_err(|_| ConnectError::InvitationRejected)?;
            sender
                .send(result.to_string())
                .map_err(|_| ConnectError::Cancelled)?;
        }
    });
    run.stage("connecting", "starting");
    let result = guest_admitted(
        &run,
        &http,
        &expected,
        &grant,
        receiver,
        &mut heartbeat,
        &authority,
    )
    .await;
    authority.close();
    heartbeat_cancel.cancel();
    let _ = heartbeat.await;
    http.stop("/v1/social/guest/stop", json!({"grantId":grant_id}))
        .await;
    result
}
async fn guest_admitted(
    run: &Run,
    http: &Http,
    expected: &lab::VerifiedSocialJoinContext,
    grant: &lab::VerifiedSocialGrant,
    mut leases: tokio::sync::watch::Receiver<String>,
    heartbeat: &mut JoinHandle<Result<(), ConnectError>>,
    authority: &Arc<ready_source::LaunchAuthority>,
) -> Result<(), ConnectError> {
    let mut delivery_tick = interval(Duration::from_millis(20));
    let delivery = loop {
        expected
            .check_current()
            .map_err(|_| ConnectError::PlanExpired)?;
        if heartbeat.is_finished() {
            return Err(ConnectError::TransportFailed);
        }
        let receiving = http.post(
            "/v1/social/guest/receive",
            json!({"grantId":grant.grant_id()}),
        );
        tokio::pin!(receiving);
        let value = loop {
            tokio::select! {biased;
                _=run.cancel.cancelled()=>return Err(ConnectError::Cancelled),
                _=delivery_tick.tick()=>{run.current()?;expected.check_current().map_err(|_|ConnectError::PlanExpired)?;authority.check_continuity().map_err(|_|ConnectError::PlanExpired)?;if heartbeat.is_finished(){return Err(ConnectError::TransportFailed);}},
                result=leases.changed()=>{result.map_err(|_|ConnectError::TransportFailed)?;let lease=leases.borrow_and_update().clone();authority.update(&lease).map_err(|_|ConnectError::InvitationRejected)?;},
                result=&mut receiving=>break result?,
            }
        };
        match field(&value, "state")? {
            "ready" => break value,
            "waiting" => pause(run, Duration::from_millis(250)).await?,
            _ => return Err(ConnectError::InvitationRejected),
        }
    };
    let voice = grant.voice_profile().is_some();
    update(&run.state, &run.id, |a| {
        if let Some(social) = &mut a.snapshot.social {
            social.runtime_profile = grant.runtime_profile().map(str::to_owned);
            social.voice_profile = grant.voice_profile().map(str::to_owned);
        }
    });
    let verify = if voice {
        if run.authority.online_origin.is_none() {
            return Err(ConnectError::InvitationRejected);
        }
        lab::SessionInvitation::verify_online_fragmented
    } else if run.authority.online_origin.is_some() {
        lab::SessionInvitation::verify_online
    } else {
        lab::SessionInvitation::verify_loopback
    };
    let invitation = verify(
        field(&delivery, "invitation")?,
        grant.host_key(),
        run.key.public(),
    )
    .map_err(|_| ConnectError::InvitationRejected)?;
    if invitation.session_id() != grant.session_id()
        || invitation.admission_expires_at_unix_ms() != grant.admission_expires_at_unix_ms()
        || invitation.session_expires_at_unix_ms() != grant.session_expires_at_unix_ms()
        || (run.authority.online_origin.is_none()
            && !matches!(invitation.direct_addr(),SocketAddr::V4(v)if v.ip().octets()==[127,0,0,1]))
    {
        return Err(ConnectError::InvitationRejected);
    }
    let preview: Value =
        serde_json::from_str(&invitation.encode()).map_err(|_| ConnectError::InvitationRejected)?;
    let services = preview["body"]["services"]
        .as_array()
        .ok_or(ConnectError::InvitationRejected)?;
    if services.len() != if voice { 2 } else { 1 }
        || services[0]["serviceId"] != "game"
        || services[0]["protocol"] != "tcp"
        || services[0]["revision"] != 1
        || services[0]["requiredCapability"] != "reliableStream"
    {
        return Err(ConnectError::InvitationRejected);
    }
    if voice
        && (services[1]["serviceId"] != "aux"
            || services[1]["protocol"] != "udp"
            || services[1]["revision"] != 1
            || services[1]["requiredCapability"] != "fragmentedDatagram")
    {
        return Err(ConnectError::InvitationRejected);
    }
    let latest = leases.borrow_and_update().clone();
    let sequence = serde_json::from_str::<Value>(&latest)
        .ok()
        .and_then(|value| value["body"]["sequence"].as_u64())
        .ok_or(ConnectError::InvitationRejected)?;
    if sequence > authority.sequence()? {
        authority
            .update(&latest)
            .map_err(|_| ConnectError::InvitationRejected)?;
    }
    expected
        .check_current()
        .map_err(|_| ConnectError::PlanExpired)?;
    grant
        .check_admission()
        .map_err(|_| ConnectError::PlanExpired)?;
    let network = if run.authority.online_origin.is_some() {
        Some(online::ticket(run, http, "guest", grant.grant_id(), grant.host_key()).await?)
    } else {
        None
    };
    let connecting = async {
        if let Some(ticket) = &network {
            if voice {
                lab::Guest::connect_invited_online_fragmented(
                    run.key.clone(),
                    &invitation,
                    limits(),
                    ticket,
                )
                .await
            } else {
                lab::Guest::connect_invited_online(run.key.clone(), &invitation, limits(), ticket)
                    .await
            }
        } else {
            lab::Guest::connect_invited_session(run.key.clone(), &invitation, limits()).await
        }
    };
    tokio::pin!(connecting);
    let mut interrupted = false;
    let mut tick = interval(Duration::from_millis(20));
    let guest=loop{tokio::select!{biased;
        _=run.cancel.cancelled()=>{interrupted=true;break connecting.as_mut().await;},
        _=tick.tick()=>if run.current().is_err()||expected.check_current().is_err()||authority.check_continuity().is_err()||heartbeat.is_finished(){interrupted=true;break connecting.as_mut().await;},
        result=leases.changed()=>{if result.is_err(){interrupted=true;break connecting.as_mut().await;}let lease=leases.borrow_and_update().clone();if authority.update(&lease).is_err(){interrupted=true;break connecting.as_mut().await;}},
        result=&mut connecting=>break result,
    }}.map_err(|_|if run.cancel.is_cancelled(){ConnectError::Cancelled}else{ConnectError::TransportFailed})?;
    let guest = Arc::new(guest);
    let sockets = guest.bound_sockets();
    let (result, tcp) = if interrupted
        || run.current().is_err()
        || expected.check_current().is_err()
        || authority.check_continuity().is_err()
    {
        (
            Err(if run.cancel.is_cancelled() {
                ConnectError::Cancelled
            } else {
                ConnectError::PlanExpired
            }),
            None,
        )
    } else {
        guest_connected(
            run,
            &guest,
            expected,
            &invitation,
            heartbeat,
            authority,
            &mut leases,
            grant.voice_profile(),
        )
        .await
    };
    guest.shutdown().await;
    drop(guest);
    release(&sockets, tcp).await?;
    result
}
// Every argument is the original attempt authority or one of its retained owners;
// keeping these borrows explicit prevents a detached UDP or launch lifecycle.
#[allow(clippy::too_many_arguments)]
async fn guest_connected(
    run: &Run,
    guest: &Arc<lab::Guest>,
    expected: &lab::VerifiedSocialJoinContext,
    invitation: &lab::SessionInvitation,
    heartbeat: &mut JoinHandle<Result<(), ConnectError>>,
    authority: &Arc<ready_source::LaunchAuthority>,
    leases: &mut tokio::sync::watch::Receiver<String>,
    voice_profile: Option<&str>,
) -> (Result<(), ConnectError>, Option<SocketAddr>) {
    let binding = guest.tcp_bridge("game", 1, "127.0.0.1:0".parse().expect("fixed loopback"));
    tokio::pin!(binding);
    let mut interrupted = false;
    let mut bind_tick = interval(Duration::from_millis(20));
    let bound = loop {
        tokio::select! {biased;
            _=run.cancel.cancelled()=>{interrupted=true;guest.shutdown().await;break binding.as_mut().await;},
            _=bind_tick.tick()=>if run.current().is_err()||expected.check_current().is_err()||authority.check_continuity().is_err()||heartbeat.is_finished(){interrupted=true;guest.shutdown().await;break binding.as_mut().await;},
            result=leases.changed()=>{if result.is_err(){interrupted=true;guest.shutdown().await;break binding.as_mut().await;}let lease=leases.borrow_and_update().clone();if authority.update(&lease).is_err(){interrupted=true;guest.shutdown().await;break binding.as_mut().await;}},
            result=&mut binding=>break result,
        }
    };
    let bridge = match bound {
        Ok(bridge) => Arc::new(bridge),
        Err(_) => {
            return (
                Err(if run.cancel.is_cancelled() {
                    ConnectError::Cancelled
                } else {
                    ConnectError::PortUnavailable
                }),
                None,
            )
        }
    };
    let tcp = bridge.local_addr();
    let voice_bind = if let Some(profile) = voice_profile {
        update(&run.state, &run.id, |a| {
            if let Some(social) = &mut a.snapshot.social {
                social.voice_profile = Some(profile.into());
                social.voice_state = Some("notStarted");
            }
        });
        guest_voice_bind(run, guest, authority, heartbeat, leases).await
    } else {
        Ok(None)
    };
    let (mut voice, voice_error) = match voice_bind {
        Ok(value) => (value, None),
        Err(error) => (None, Some(error)),
    };
    let voice_address = voice.as_ref().map(lab::UdpBridge::local_addr);
    if voice_profile.is_some() {
        update(&run.state, &run.id, |a| {
            if let Some(social) = &mut a.snapshot.social {
                social.voice_state = Some(if voice.is_some() { "enabled" } else { "failed" });
            }
        });
    }
    let result = if interrupted
        || run.current().is_err()
        || expected.check_current().is_err()
        || authority.check_continuity().is_err()
    {
        Err(if run.cancel.is_cancelled() {
            ConnectError::Cancelled
        } else {
            ConnectError::PlanExpired
        })
    } else if let Some(error) = voice_error {
        Err(error)
    } else if authority.admit(invitation.clone()).is_err() {
        Err(ConnectError::InvitationRejected)
    } else {
        let source =
            ready_game::GuestSource::social(guest, &bridge, authority, run.deadline, &run.cancel);
        update(&run.state, &run.id, |a| {
            if !a.cancel.is_cancelled() && authority.with_current(|| Ok(())).is_ok() {
                a.guest_source = Some(source);
                a.snapshot.phase = "guestConnected";
                a.snapshot.transport_state = "authenticated";
                a.snapshot.tcp_address = Some(tcp.to_string());
                a.snapshot.expires_at_ms = Some(invitation.session_expires_at_unix_ms());
                a.session_deadline = Some(run.deadline);
                if let Some(social) = &mut a.snapshot.social {
                    social.stage = "connected";
                    social.ready_game_launch_available = Some(true);
                    social.expires_at_ms = Some(invitation.session_expires_at_unix_ms());
                }
            }
        });
        #[cfg(test)]
        let pause_error = if let Some(probe) = &run.guard_probe {
            probe
                .hold(authority, leases.clone(), guest, &bridge, &run.cancel)
                .await
                .err()
        } else {
            None
        };
        let mut tick = interval(Duration::from_millis(20));
        loop {
            #[cfg(test)]
            if let Some(error) = pause_error {
                break Err(error);
            }
            tokio::select! {
                biased;
                _ = run.cancel.cancelled() => break Ok(()),
                _ = guest.connection().closed() => break Ok(()),
                _ = tick.tick() => {
                    if voice.as_ref().is_some_and(|bridge| bridge.is_closed() || bridge.max_payload().is_none_or(|size| size < 4096)) {
                        if let Some(bridge) = voice.take() { bridge.shutdown().await; }
                        update(&run.state, &run.id, |a| { if let Some(social) = &mut a.snapshot.social { social.voice_state = Some("retired"); } });
                    }
                    if run.current().is_err() {
                        break Ok(());
                    }
                    if authority.check_continuity().is_err() {
                        break Err(ConnectError::PlanExpired);
                    }
                    if heartbeat.is_finished() {
                        break Err(ConnectError::TransportFailed);
                    }
                },
                result = leases.changed() => {
                    if result.is_err() {
                        break Err(ConnectError::TransportFailed);
                    }
                    let lease = leases.borrow_and_update().clone();
                    if authority.update(&lease).is_err() {
                        break Err(ConnectError::InvitationRejected);
                    }
                },
            }
        }
    };
    authority.close();
    update(&run.state, &run.id, |a| {
        a.guest_source = None;
        ready_game::cancel(a);
        if let Some(social) = &mut a.snapshot.social {
            social.ready_game_launch_available = None;
        }
    });
    guest.shutdown().await;
    if let Some(voice) = voice {
        voice.shutdown().await;
    }
    bridge.shutdown().await;
    drop(bridge);
    update(&run.state, &run.id, |a| {
        if let Some(social) = &mut a.snapshot.social {
            if social.voice_profile.is_some() {
                social.voice_state = Some("closed");
            }
        }
    });
    let cleanup = if let Some(address) = voice_address {
        release(&[address], None).await
    } else {
        Ok(())
    };
    (cleanup.and(result), Some(tcp))
}
/// Keep continuity and cancellation live while the original UDP bind finishes.
/// A failed voice association is terminal for this attempt; TCP remains usable.
async fn guest_voice_bind(
    run: &Run,
    guest: &Arc<lab::Guest>,
    authority: &Arc<ready_source::LaunchAuthority>,
    heartbeat: &JoinHandle<Result<(), ConnectError>>,
    leases: &mut tokio::sync::watch::Receiver<String>,
) -> Result<Option<lab::UdpBridge>, ConnectError> {
    let binding = guest.udp_bridge_fragmented(
        "aux",
        1,
        "127.0.0.1:35554".parse().expect("fixed native bridge"),
    );
    tokio::pin!(binding);
    let mut tick = interval(Duration::from_millis(20));
    let mut interrupted = false;
    let bound = loop {
        tokio::select! { biased;
            _=run.cancel.cancelled()=>{interrupted=true;guest.shutdown().await;break binding.as_mut().await;},
            _=tick.tick()=>if run.current().is_err() || authority.check_continuity().is_err() || heartbeat.is_finished() {
                interrupted=true;guest.shutdown().await;break binding.as_mut().await;
            },
            result=leases.changed()=>{if result.is_err() || authority.update(&leases.borrow_and_update().clone()).is_err() {
                interrupted=true;guest.shutdown().await;break binding.as_mut().await;
            }},
            result=&mut binding=>break result,
        }
    };
    match bound {
        Ok(bridge)
            if !interrupted
                && !bridge.is_closed()
                && bridge.max_payload().is_some_and(|size| size >= 4096) =>
        {
            Ok(Some(bridge))
        }
        Ok(bridge) => {
            bridge.shutdown().await;
            if interrupted {
                Err(ConnectError::PlanExpired)
            } else {
                Ok(None)
            }
        }
        Err(_) if interrupted => Err(ConnectError::PlanExpired),
        Err(_) => Ok(None),
    }
}
async fn release(sockets: &[SocketAddr], tcp: Option<SocketAddr>) -> Result<(), ConnectError> {
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut first = true;
    loop {
        if !first && Instant::now() >= deadline {
            return Err(ConnectError::CleanupFailed);
        }
        first = false;
        let mut udp = Vec::new();
        let mut errors = Vec::new();
        for address in sockets {
            match std::net::UdpSocket::bind(address) {
                Ok(socket) => udp.push(socket),
                Err(error) => errors.push(error.kind()),
            }
        }
        let listener = match tcp.map(std::net::TcpListener::bind) {
            Some(Ok(listener)) => Some(listener),
            Some(Err(error)) => {
                errors.push(error.kind());
                None
            }
            None => None,
        };
        if errors.is_empty() {
            drop(listener);
            drop(udp);
            return Ok(());
        }
        if errors.iter().any(|e| *e != std::io::ErrorKind::AddrInUse) || Instant::now() >= deadline
        {
            return Err(ConnectError::CleanupFailed);
        }
        drop(listener);
        drop(udp);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn manager() -> Manager {
        Manager::with_social_lab(
            SocialLabAuthority::from_local_process(
                &lab::generate_key().public().to_string(),
                &"11".repeat(32),
            )
            .unwrap(),
        )
    }
    fn request(epoch: u64) -> SocialReserveRequest {
        SocialReserveRequest {
            account_epoch: epoch,
            account_id: "localTestAccount".into(),
            session_id: uuid::Uuid::new_v4().to_string(),
            role: "guest".into(),
            game_target: None,
            visibility: None,
        }
    }
    fn cleanup_complete(snapshot: &ConnectSnapshot) -> bool {
        (snapshot.phase == "idle" && matches!(snapshot.cleanup_state, "notStarted" | "complete"))
            || (matches!(snapshot.phase, "closed" | "failed")
                && snapshot.cleanup_state == "complete")
    }
    #[tokio::test]
    async fn social_removal_requires_original_owner_epoch_operation_and_current_roster() {
        let mut manager = manager();
        let mut host_request = request(0);
        host_request.role = "host".into();
        host_request.visibility = Some("public".into());
        host_request.game_target = Some("127.0.0.1:43001".into());
        let reservation = manager.social_reserve("main", host_request).unwrap();
        manager.social_authority.as_mut().unwrap().online_origin =
            Some("https://compiled.test.invalid");
        let grant_id = "ab".repeat(32);
        {
            let mut state = lock(&manager.state).unwrap();
            let attempt = state.attempt.as_mut().unwrap();
            attempt.worker = Some(tokio::spawn(std::future::pending::<()>()));
            attempt.snapshot.phase = "hostListening";
            attempt.snapshot.social.as_mut().unwrap().participants =
                Some(vec![SocialParticipant {
                    grant_id: grant_id.clone(),
                    account_id: "verifiedGuest".into(),
                    state: "connected",
                }]);
        }
        assert!(manager
            .social_revoke_participant("other-window", &reservation.operation_id, 0, &grant_id)
            .is_err());
        assert!(manager
            .social_revoke_participant("main", &reservation.operation_id, 1, &grant_id)
            .is_err());
        assert!(manager
            .social_revoke_participant("main", &uuid::Uuid::new_v4().to_string(), 0, &grant_id)
            .is_err());
        assert!(manager
            .social_revoke_participant("main", &reservation.operation_id, 0, &"cd".repeat(32))
            .is_err());
        let removed = manager
            .social_revoke_participant("main", &reservation.operation_id, 0, &grant_id)
            .unwrap();
        assert_eq!(
            removed.social.unwrap().participants.unwrap()[0].state,
            "removing"
        );
        manager
            .social_revoke_participant("main", &reservation.operation_id, 0, &grant_id)
            .unwrap();
        let job = {
            let mut state = lock(&manager.state).unwrap();
            let attempt = state.attempt.as_mut().unwrap();
            assert_eq!(attempt.social.as_ref().unwrap().revocations.len(), 1);
            attempt.worker.take().unwrap()
        };
        job.abort();
        let _ = job.await;
    }
    #[tokio::test]
    async fn invalid_token_worker_is_reaped_and_never_reopens_from_late_handoff() {
        let manager = manager();
        let reservation = manager.social_reserve("main", request(0)).unwrap();
        let started = manager
            .social_authenticate(
                "main",
                &reservation.operation_id,
                reservation.generation,
                "invalid-local-token".into(),
            )
            .unwrap();
        assert_eq!(started.cleanup_state, "pending");
        assert_eq!(
            manager
                .social_authenticate(
                    "main",
                    &reservation.operation_id,
                    reservation.generation,
                    "invalid-local-token".into()
                )
                .unwrap_err(),
            ConnectError::StaleOperation
        );
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let snapshot = manager.snapshot("main").unwrap();
                if snapshot.phase == "failed" {
                    assert_eq!(
                        snapshot.error_code,
                        Some(ConnectError::InvitationRejected.code())
                    );
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let stopped = manager
            .stop("main", &reservation.operation_id)
            .await
            .unwrap();
        assert!(cleanup_complete(&stopped));
        assert!(manager
            .state
            .lock()
            .unwrap()
            .attempt
            .as_ref()
            .unwrap()
            .worker
            .is_none());
        assert_eq!(
            manager
                .social_authenticate(
                    "main",
                    &reservation.operation_id,
                    reservation.generation,
                    "invalid-local-token".into()
                )
                .unwrap_err(),
            ConnectError::StaleOperation
        );
        assert!(manager.social_reserve("main", request(0)).is_ok());
        manager.shutdown_all().await;
    }
    #[tokio::test]
    async fn stage_projection_changes_once_and_host_admission_is_explicit() {
        let manager = manager();
        let mut request = request(0);
        request.role = "host".into();
        request.game_target = Some("127.0.0.1:43001".into());
        request.visibility = Some("public".into());
        let reservation = manager.social_reserve("main", request).unwrap();
        project_stage(
            &manager.state,
            &reservation.operation_id,
            "waitingGuest",
            "hostListening",
        );
        let waiting = manager.snapshot("main").unwrap();
        assert_eq!(waiting.transport_state, "notStarted");
        project_stage(
            &manager.state,
            &reservation.operation_id,
            "connected",
            "hostListening",
        );
        let admitted = manager.snapshot("main").unwrap();
        assert_eq!(admitted.transport_state, "admitted");
        for _ in 0..10 {
            project_stage(
                &manager.state,
                &reservation.operation_id,
                "connected",
                "hostListening",
            );
        }
        assert_eq!(
            manager.snapshot("main").unwrap().generation,
            admitted.generation
        );
        manager.shutdown_all().await;
    }
    #[tokio::test]
    async fn cleanup_failure_is_never_normalized_or_reported_as_success() {
        assert_eq!(execution_error(Err(ConnectError::Cancelled)), None);
        assert_eq!(
            execution_error(Err(ConnectError::CleanupFailed)),
            Some(ConnectError::CleanupFailed)
        );
        let manager = manager();
        let reservation = manager.social_reserve("main", request(0)).unwrap();
        finish(
            &manager.state,
            &reservation.operation_id,
            Some(ConnectError::CleanupFailed),
        );
        let snapshot = manager
            .stop("main", &reservation.operation_id)
            .await
            .unwrap();
        assert_eq!(snapshot.cleanup_state, "failed");
        assert!(!cleanup_complete(&snapshot));
        assert_eq!(
            manager.social_reserve("main", request(0)).unwrap_err(),
            ConnectError::Busy
        );
    }
    #[tokio::test]
    async fn reserved_deadline_observes_civil_expiry_without_reanchoring_monotonic_time() {
        let manager = manager();
        let reservation = manager.social_reserve("main", request(0)).unwrap();
        {
            let mut state = manager.state.lock().unwrap();
            let attempt = state.attempt.as_mut().unwrap();
            assert!(Instant::now() < attempt.prepared_deadline);
            attempt.snapshot.social.as_mut().unwrap().expires_at_ms =
                Some(lab::unix_millis().saturating_sub(1));
        }
        let snapshot = manager.snapshot("main").unwrap();
        assert_eq!(snapshot.phase, "closed");
        assert!(cleanup_complete(&snapshot));
        assert_eq!(
            manager
                .social_authenticate(
                    "main",
                    &reservation.operation_id,
                    reservation.generation,
                    "invalid-local-token".into()
                )
                .unwrap_err(),
            ConnectError::StaleOperation
        );
        manager.shutdown_all().await;
    }
    #[test]
    fn local_token_confinement_rejects_production_wrong_account_and_expiry() {
        use base64::Engine as _;
        let now = lab::unix_millis() / 1000;
        let claims = json!({"aud":"demo-aurora-social","iss":"https://securetoken.google.com/demo-aurora-social","sub":"localTestAccount","user_id":"localTestAccount","iat":now,"auth_time":now,"exp":now+60});
        let encode = |alg: &str, claims: &Value, signature: &str| {
            format!(
                "{}.{}.{}",
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(json!({"alg":alg}).to_string()),
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string()),
                signature
            )
        };
        assert!(local_emulator_token(
            &encode("none", &claims, ""),
            "localTestAccount"
        ));
        assert!(!local_emulator_token(
            &encode("RS256", &claims, "signature"),
            "localTestAccount"
        ));
        assert!(!local_emulator_token(
            &encode("none", &claims, ""),
            "otherAccount"
        ));
        let mut changed = claims.clone();
        changed["aud"] = json!("production");
        assert!(!local_emulator_token(
            &encode("none", &changed, ""),
            "localTestAccount"
        ));
        changed = claims.clone();
        changed["exp"] = json!(now);
        assert!(!local_emulator_token(
            &encode("none", &changed, ""),
            "localTestAccount"
        ));
        changed = claims;
        changed["user_id"] = json!("otherAccount");
        assert!(!local_emulator_token(
            &encode("none", &changed, ""),
            "localTestAccount"
        ));
        assert!(!local_emulator_token("not-a-token", "localTestAccount"));
    }
    #[tokio::test]
    async fn release_fails_if_any_owned_port_remains_bound() {
        let retained = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let address = retained.local_addr().unwrap();
        assert_eq!(
            release(&[address], None).await,
            Err(ConnectError::CleanupFailed)
        );
        drop(retained);
        assert!(release(&[address], None).await.is_ok());
    }
    #[tokio::test]
    async fn account_fence_denies_late_reserve_and_late_token() {
        let manager = manager();
        let epoch = manager.social_capabilities().account_epoch;
        assert_eq!(
            manager.account_transition_stop("main").await.unwrap().phase,
            "idle"
        );
        assert_eq!(
            manager.social_reserve("main", request(epoch)).unwrap_err(),
            ConnectError::StaleOperation
        );
        let reservation = manager.social_reserve("main", request(epoch + 1)).unwrap();
        let stopped = manager.account_transition_stop("main").await.unwrap();
        assert_eq!(stopped.cleanup_state, "complete");
        assert_eq!(stopped.phase, "closed");
        assert_eq!(
            manager
                .social_authenticate(
                    "main",
                    &reservation.operation_id,
                    reservation.generation,
                    "not-a-token".into()
                )
                .unwrap_err(),
            ConnectError::StaleOperation
        );
        assert!(manager.social_reserve("main", request(epoch + 2)).is_ok());
        manager.shutdown_all().await;
    }
    #[tokio::test]
    async fn reserved_social_uses_existing_slot_and_rejects_manual_actions() {
        let manager = manager();
        let reservation = manager.social_reserve("main", request(0)).unwrap();
        assert_eq!(
            manager.social_reserve("main", request(0)).unwrap_err(),
            ConnectError::Busy
        );
        assert_eq!(
            manager
                .prepare_guest(
                    "main",
                    GuestRequest {
                        mode: ConnectMode::Loopback,
                        expected_host_public_id: lab::generate_key().public().to_string()
                    }
                )
                .unwrap_err(),
            ConnectError::Busy
        );
        assert!(manager
            .confirm("main", &reservation.operation_id, &"11".repeat(32))
            .await
            .is_err());
        assert!(manager
            .export_invitation("main", &reservation.operation_id)
            .is_err());
        assert!(manager
            .social_confirm_host(
                "main",
                &reservation.operation_id,
                reservation.generation,
                &"11".repeat(32)
            )
            .is_err());
        assert!(manager
            .stop("secondary", &reservation.operation_id)
            .await
            .is_err());
        let stopped = manager
            .stop("main", &reservation.operation_id)
            .await
            .unwrap();
        assert_eq!(stopped.cleanup_state, "complete");
        assert_eq!(stopped.social.unwrap().stage, "closed");
    }
    #[test]
    fn bootstrap_and_source_cannot_be_selected_by_renderer() {
        let manager = Manager::default();
        assert!(!manager.social_capabilities().available);
        assert_eq!(
            manager.social_reserve("main", request(0)).unwrap_err(),
            ConnectError::Disabled
        );
        assert!(
            SocialLabAuthority::from_local_process("https://remote.invalid", &"11".repeat(32))
                .is_err()
        );
        let manager = super::tests::manager();
        let mut request = request(0);
        request.role = "host".into();
        request.visibility = Some("public".into());
        request.game_target = Some("127.0.0.1:22".into());
        assert_eq!(
            manager.social_reserve("main", request).unwrap_err(),
            ConnectError::InvalidRequest
        );
    }
    #[tokio::test]
    async fn watch_initial_snapshot_consumes_preexisting_heartbeat_version() {
        let (send, mut receive) = tokio::sync::watch::channel(1_u64);
        send.send(2).unwrap();
        assert_eq!(*receive.borrow_and_update(), 2);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), receive.changed())
                .await
                .is_err()
        );
        send.send(3).unwrap();
        receive.changed().await.unwrap();
        assert_eq!(*receive.borrow_and_update(), 3);
    }

    /// Isolated libtest-only process endpoint; never part of the executable API.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "opt-in real local Auth/Firestore integration process"]
    async fn social_desktop_process() {
        use std::io::{BufRead, Read, Write};
        assert_eq!(
            std::env::var("AURORA_SOCIAL_DESKTOP_FIXTURE").as_deref(),
            Ok("1")
        );
        let pin = std::env::var("AURORA_SOCIAL_LAB_PIN").unwrap();
        let issuer = std::env::var("AURORA_SOCIAL_LAB_ISSUER").unwrap();
        let manager = ConnectManager::with_social_lab(
            SocialLabAuthority::from_local_process(&pin, &issuer).unwrap(),
        );
        fn emit(value: Value) {
            let mut out = std::io::stdout().lock();
            writeln!(out, "SOCIAL89_JSON:{value}").unwrap();
            out.flush().unwrap();
        }
        let (send, mut receive) = tokio::sync::mpsc::channel(4);
        let reader = std::thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            loop {
                let mut bytes = Vec::new();
                let count = input
                    .by_ref()
                    .take(16_385)
                    .read_until(b'\n', &mut bytes)
                    .unwrap_or(0);
                if count == 0 {
                    break;
                }
                if bytes.len() > 16_384 {
                    break;
                }
                let value = serde_json::from_slice::<Value>(&bytes);
                if send.blocking_send(value).is_err() {
                    break;
                }
            }
        });
        emit(json!({"event":"ready","capabilities":manager.social_capabilities()}));
        let fixture_deadline = tokio::time::Instant::now() + Duration::from_secs(180);
        let mut ready_fixture: Option<Arc<ready_fixture::Fixture>> = None;
        let mut guard_probe: Option<Arc<ready_source::fixture::Pause>> = None;
        #[cfg(windows)]
        let discovery = crate::engine::connect_discovery::DiscoveryManager::default();
        #[cfg(windows)]
        let mut owned_fixture: Option<crate::engine::connect_discovery::tests::Fixture> = None;
        loop {
            let line = tokio::select! {_=tokio::time::sleep_until(fixture_deadline)=>break,line=receive.recv()=>line};
            let Some(Ok(value)) = line else {
                break;
            };
            let command = value.get("command").and_then(Value::as_str).unwrap_or("");
            let id = value
                .get("operationId")
                .and_then(Value::as_str)
                .unwrap_or("");
            let generation = value.get("generation").and_then(Value::as_u64).unwrap_or(0);
            let result: Result<Value, ConnectError> = match command {
                "guardProbeArm" => {
                    let mut state = manager.live.state.lock().unwrap();
                    match selected(&mut state, "main", id) {
                        Ok(a)
                            if a.worker.is_none()
                                && a.snapshot.role == Some("guest")
                                && guard_probe.is_none() =>
                        {
                            let probe = Arc::new(ready_source::fixture::Pause::default());
                            a.social.as_mut().unwrap().guard_probe = Some(probe.clone());
                            guard_probe = Some(probe);
                            Ok(json!({"armed":true}))
                        }
                        _ => Err(ConnectError::InvalidRequest),
                    }
                }
                "guardProbeInspect" => match &guard_probe {
                    Some(probe) => probe.inspect_expiry().await,
                    None => Err(ConnectError::InvalidRequest),
                },
                "guardProbeRelease" => {
                    if let Some(probe) = &guard_probe {
                        probe.release();
                    }
                    Ok(json!({"released":true}))
                }
                "readyPrepare" => {
                    if ready_fixture.is_some() {
                        Err(ConnectError::Busy)
                    } else if let Ok(fixture) =
                        ready_fixture::Fixture::new(value["hold"].as_str().unwrap_or("none"))
                    {
                        let result = manager.prepare_ready_game_launch(
                            "main",
                            ready_fixture::Fixture::request(id),
                            Arc::new(fixture.clone()),
                        );
                        ready_fixture = Some(fixture);
                        Ok(match result {
                            Ok(snapshot) => serde_json::to_value(snapshot).unwrap(),
                            Err(error) => json!({"errorCode":error.code()}),
                        })
                    } else {
                        Err(ConnectError::InvalidRequest)
                    }
                }
                "readySnapshot" => Ok(match manager.ready_game_launch_snapshot("main", id) {
                    Ok(snapshot) => serde_json::to_value(snapshot).unwrap(),
                    Err(error) => json!({"errorCode":error.code()}),
                }),
                "readyConfirm" => Ok(
                    match manager.confirm_ready_game_launch(
                        "main",
                        id,
                        value["planId"].as_str().unwrap_or(""),
                        value["digest"].as_str().unwrap_or(""),
                    ) {
                        Ok(snapshot) => serde_json::to_value(snapshot).unwrap(),
                        Err(error) => json!({"errorCode":error.code()}),
                    },
                ),
                "readyRelease" => {
                    if let Some(fixture) = &ready_fixture {
                        fixture.release();
                    }
                    Ok(json!({"released":true}))
                }
                "readyFixture" => Ok(ready_fixture
                    .as_ref()
                    .map_or(json!({"spawns":0,"childAlive":false}), |fixture| {
                        fixture.snapshot()
                    })),
                "readyStopDetached" => {
                    use std::{
                        future::Future,
                        task::{Context, Poll, Waker},
                    };
                    let stopped = manager.stop("main", id);
                    tokio::pin!(stopped);
                    let pending = matches!(
                        stopped
                            .as_mut()
                            .poll(&mut Context::from_waker(Waker::noop())),
                        Poll::Pending
                    );
                    // The real stop waiter is dropped here; its worker remains owned.
                    Ok(json!({"pending":pending}))
                }
                "readyAdvanceEpoch" => {
                    // Exercise the real account transition's first linearized
                    // step without the subsequent cancel masking a gate defect.
                    let mut state = manager.live.state.lock().unwrap();
                    state.social_epoch = state.social_epoch.checked_add(1).unwrap();
                    Ok(
                        json!({"epoch":state.social_epoch,"originalCancelled":state.attempt.as_ref().is_none_or(|a| a.cancel.is_cancelled())}),
                    )
                }
                #[cfg(windows)]
                "discoverFixture" => {
                    if owned_fixture.is_some() {
                        Err(ConnectError::Busy)
                    } else {
                        let mut fixture = crate::engine::connect_discovery::tests::Fixture::new();
                        let selected = crate::engine::connect_discovery::tests::selection(
                            &discovery,
                            &mut fixture,
                        )
                        .await;
                        owned_fixture = Some(fixture);
                        Ok(json!({"selectionId":selected.selection_id,"target":selected.target}))
                    }
                }
                #[cfg(windows)]
                "reserveDiscovered" => serde_json::from_value(value["request"].clone())
                    .map_err(|_| ConnectError::InvalidRequest)
                    .and_then(|request| {
                        manager.social_reserve_discovered(
                            "main",
                            request,
                            value
                                .get("selectionId")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .into(),
                            discovery.clone(),
                        )
                    })
                    .and_then(|result| {
                        serde_json::to_value(result).map_err(|_| ConnectError::Internal)
                    }),
                #[cfg(windows)]
                "stopFixture" => {
                    if let Some(mut fixture) = owned_fixture.take() {
                        fixture.stop();
                    }
                    Ok(json!({"cleanup":true}))
                }
                "probeStatus" => {
                    let snapshot = manager.snapshot("main").unwrap();
                    if snapshot.role != Some("guest") || snapshot.phase != "guestConnected" {
                        Err(ConnectError::InvalidRequest)
                    } else {
                        let endpoint: SocketAddr = snapshot.tcp_address.unwrap().parse().unwrap();
                        lab::probe_minecraft_status(lab::MinecraftStatusRequest { endpoint,
                            handshake_host: "localhost".into(), handshake_port: endpoint.port(), protocol: 767,
                            timeout: Duration::from_secs(3) }).await
                            .map(|summary| json!({"requestedProtocol":summary.requested_protocol,"reportedProtocol":summary.reported_protocol,"protocolMatchesRequested":summary.protocol_matches_requested}))
                            .map_err(|_| ConnectError::StatusFailed)
                    }
                }
                "reserve" => serde_json::from_value(value["request"].clone())
                    .map_err(|_| ConnectError::InvalidRequest)
                    .and_then(|request| manager.social_reserve("main", request))
                    .and_then(|result| {
                        serde_json::to_value(result).map_err(|_| ConnectError::Internal)
                    }),
                "authenticate" => manager
                    .social_authenticate(
                        "main",
                        id,
                        generation,
                        value
                            .get("token")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .into(),
                    )
                    .and_then(|result| {
                        serde_json::to_value(result).map_err(|_| ConnectError::Internal)
                    }),
                "confirm" => manager
                    .social_confirm_host(
                        "main",
                        id,
                        generation,
                        value.get("digest").and_then(Value::as_str).unwrap_or(""),
                    )
                    .and_then(|result| {
                        serde_json::to_value(result).map_err(|_| ConnectError::Internal)
                    }),
                "snapshot" => manager.snapshot("main").and_then(|result| {
                    serde_json::to_value(result).map_err(|_| ConnectError::Internal)
                }),
                "stop" => manager.stop("main", id).await.and_then(|result| {
                    serde_json::to_value(result).map_err(|_| ConnectError::Internal)
                }),
                "accountTransition" => {
                    manager
                        .account_transition_stop("main")
                        .await
                        .and_then(|result| {
                            serde_json::to_value(result).map_err(|_| ConnectError::Internal)
                        })
                }
                "quit" => {
                    if let Some(probe) = &guard_probe {
                        probe.release();
                    }
                    if let Some(fixture) = &ready_fixture {
                        fixture.release();
                    }
                    manager.shutdown_all().await;
                    if let Some(fixture) = ready_fixture.take() {
                        fixture.cleanup();
                    }
                    #[cfg(windows)]
                    if let Some(mut fixture) = owned_fixture.take() {
                        fixture.stop();
                    }
                    let cleanup = cleanup_complete(&manager.snapshot("main").unwrap());
                    emit(json!({"ok":true,"value":{"cleanup":cleanup}}));
                    break;
                }
                _ => Err(ConnectError::InvalidRequest),
            };
            match result {
                Ok(value) => emit(json!({"ok":true,"value":value})),
                Err(error) => emit(json!({"ok":false,"code":error.code()})),
            }
        }
        if let Some(probe) = &guard_probe {
            probe.release();
        }
        if let Some(fixture) = &ready_fixture {
            fixture.release();
        }
        manager.shutdown_all().await;
        if let Some(fixture) = ready_fixture.take() {
            fixture.cleanup();
        }
        #[cfg(windows)]
        if let Some(mut fixture) = owned_fixture.take() {
            fixture.stop();
        }
        assert!(
            cleanup_complete(&manager.snapshot("main").unwrap()),
            "fixture native cleanup incomplete"
        );
        receive.close();
        if reader.is_finished() {
            let _ = reader.join();
        }
    }
}

#[cfg(all(test, windows))]
#[path = "social_source_tests.rs"]
mod source_tests;
