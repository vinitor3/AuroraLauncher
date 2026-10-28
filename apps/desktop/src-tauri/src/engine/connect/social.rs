//! Social admission is one worker of the existing Attempt, not a UI sidecar.
use super::*;
use serde_json::{json, Value};
use tokio::sync::oneshot;

const ORIGIN: &str = "http://127.0.0.1:42888";
const MAX_HTTP_BYTES: usize = 8192;
pub(super) struct Control {
    request: SocialReserveRequest,
    key: Option<lab::SecretKey>,
    reservation_generation: u64,
    approval: Option<oneshot::Sender<String>>,
}
pub(super) fn clear(a: &mut Attempt) {
    if let Some(control) = &mut a.social {
        control.key = None;
        control.approval = None;
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
impl Manager {
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
            max_session_seconds: 60,
        }
    }
    pub(in crate::engine::connect) fn social_reserve(
        &self,
        owner: &str,
        request: SocialReserveRequest,
    ) -> Result<SocialReservation, ConnectError> {
        if self.social_authority.is_none() {
            return Err(ConnectError::Disabled);
        }
        let valid_session = uuid::Uuid::parse_str(&request.session_id)
            .is_ok_and(|id| id.get_version_num() == 4 && id.to_string() == request.session_id);
        let role = match request.role.as_str() {
            "host"
                if request.game_target.as_deref() == Some("127.0.0.1:43001")
                    && matches!(request.visibility.as_deref(), Some("public" | "friends")) =>
            {
                "host"
            }
            "guest" if request.game_target.is_none() && request.visibility.is_none() => "guest",
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
        });
        state.attempt = Some(Attempt {
            owner: owner.into(),
            snapshot,
            social: Some(Control {
                request,
                key: Some(lab::generate_key()),
                reservation_generation: reservation.generation,
                approval: None,
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
                key,
                authority,
                deadline,
                expiry,
                cancel: cancel.clone(),
            };
            let result = execute(run, id_token).await;
            close_operation(&worker_state, &operation, &cancel, execution_error(result)).await;
        }));
        Ok(a.snapshot.clone())
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
    key: lab::SecretKey,
    authority: SocialLabAuthority,
    deadline: Instant,
    expiry: u64,
    cancel: CancellationToken,
}
impl Run {
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
    token: Arc<String>,
    cancel: CancellationToken,
    deadline: Instant,
}
impl Http {
    fn new(token: String, run: &Run) -> Result<Self, ConnectError> {
        if !local_emulator_token(&token, &run.request.account_id) {
            return Err(ConnectError::InvitationRejected);
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(0)
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|_| ConnectError::Internal)?;
        Ok(Self {
            client,
            token: Arc::new(token),
            cancel: run.cancel.clone(),
            deadline: run.deadline,
        })
    }
    async fn post(&self, path: &'static str, body: Value) -> Result<Value, ConnectError> {
        let work = async {
            let mut response = self
                .client
                .post(format!("{ORIGIN}{path}"))
                .bearer_auth(self.token.as_str())
                .json(&body)
                .send()
                .await
                .map_err(|_| ConnectError::TransportFailed)?;
            if !response.status().is_success() {
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
async fn execute(run: Run, token: String) -> Result<(), ConnectError> {
    let http = Http::new(token, &run)?;
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

async fn host(run: Run, http: Http) -> Result<(), ConnectError> {
    let (challenge, proof) = challenge(&run, &http, None).await?;
    let source: SocketAddr = "127.0.0.1:43001".parse().expect("fixed local source");
    let policy = lab::SocialHostPolicy::review(
        &challenge,
        run.request
            .visibility
            .as_deref()
            .ok_or(ConnectError::InvalidRequest)?,
        source,
    )
    .map_err(|_| ConnectError::InvitationRejected)?;
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
                social.expires_at_ms = Some(policy.expires_at_unix_ms());
            }
            a.snapshot.phase = "reviewed";
        }
    });
    let approved = tokio::select! {biased; _=run.cancel.cancelled()=>return Err(ConnectError::Cancelled),
    _=tokio::time::sleep_until(run.deadline.into())=>return Err(ConnectError::PlanExpired),
    result=receiver=>result.map_err(|_|ConnectError::Cancelled)?, };
    run.current()?;
    let policy = policy
        .approve(&run.key, &approved, run.deadline)
        .map_err(|_| ConnectError::WrongDigest)?;
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
        host_registered(&run, &http, &challenge, &policy, source, &policy_id).await
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
    policy_id: &str,
) -> Result<(), ConnectError> {
    let delivery = loop {
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
    let result = if interrupted || run.current().is_err() || continuity.check().is_err() {
        Err(if run.cancel.is_cancelled() {
            ConnectError::Cancelled
        } else {
            ConnectError::PlanExpired
        })
    } else {
        host_active(
            run,
            http,
            &host,
            &grant,
            &invitation,
            &mut continuity,
            policy_id,
        )
        .await
    };
    host.revoke(invitation.invitation_id());
    let sockets = host.bound_sockets();
    host.shutdown().await;
    drop(host);
    release(&sockets, None).await?;
    result
}
async fn host_active(
    run: &Run,
    http: &Http,
    host: &lab::Host,
    grant: &lab::VerifiedSocialGrant,
    invitation: &lab::SessionInvitation,
    continuity: &mut lab::SocialContinuity,
    policy_id: &str,
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
    loop {
        tokio::select! {biased;
            _=run.cancel.cancelled()=>return Ok(()),
            _=tick.tick()=>{
                run.current()?;continuity.check().map_err(|_|ConnectError::PlanExpired)?;
                admitted|=host.invitation_consumed(invitation.invitation_id());
                if admitted{run.stage("connected","hostListening");}
                if !admitted{grant.check_admission().map_err(|_|ConnectError::PlanExpired)?;}
                if admitted&&!host.invitation_active(invitation.invitation_id()){return Ok(());}
            },
            _=poll.tick()=>{
                let querying=http.post("/v1/social/host/poll",json!({"policyId":policy_id}));tokio::pin!(querying);
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

async fn guest(run: Run, http: Http) -> Result<(), ConnectError> {
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
    let grant = lab::VerifiedSocialGrant::for_guest(
        &packet(&joined, "grant")?,
        run.authority.pin,
        &challenge,
        &expected,
        run.deadline,
    )
    .map_err(|_| ConnectError::InvitationRejected)?;
    let first_lease = packet(&joined, "lease")?;
    let mut continuity = lab::SocialContinuity::start(&first_lease, &grant)
        .map_err(|_| ConnectError::InvitationRejected)?;
    let local_continuity = lab::SocialContinuity::start(&first_lease, &grant)
        .map_err(|_| ConnectError::InvitationRejected)?;
    let grant_id = grant.grant_id().to_owned();
    let heartbeat_cancel = CancellationToken::new();
    let heartbeat_signal = heartbeat_cancel.clone();
    let heartbeat_http = http.clone();
    let heartbeat_id = grant_id.clone();
    let (sender, receiver) = tokio::sync::watch::channel(first_lease);
    let mut heartbeat = tokio::spawn(async move {
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
        local_continuity,
    )
    .await;
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
    mut continuity: lab::SocialContinuity,
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
                _=delivery_tick.tick()=>{run.current()?;expected.check_current().map_err(|_|ConnectError::PlanExpired)?;continuity.check().map_err(|_|ConnectError::PlanExpired)?;if heartbeat.is_finished(){return Err(ConnectError::TransportFailed);}},
                result=leases.changed()=>{result.map_err(|_|ConnectError::TransportFailed)?;let lease=leases.borrow_and_update().clone();continuity.update(&lease).map_err(|_|ConnectError::InvitationRejected)?;},
                result=&mut receiving=>break result?,
            }
        };
        match field(&value, "state")? {
            "ready" => break value,
            "waiting" => pause(run, Duration::from_millis(250)).await?,
            _ => return Err(ConnectError::InvitationRejected),
        }
    };
    let invitation = lab::SessionInvitation::verify_loopback(
        field(&delivery, "invitation")?,
        grant.host_key(),
        run.key.public(),
    )
    .map_err(|_| ConnectError::InvitationRejected)?;
    if invitation.session_id() != grant.session_id()
        || invitation.admission_expires_at_unix_ms() != grant.admission_expires_at_unix_ms()
        || invitation.session_expires_at_unix_ms() != grant.session_expires_at_unix_ms()
        || !matches!(invitation.direct_addr(),SocketAddr::V4(v)if v.ip().octets()==[127,0,0,1])
    {
        return Err(ConnectError::InvitationRejected);
    }
    let preview: Value =
        serde_json::from_str(&invitation.encode()).map_err(|_| ConnectError::InvitationRejected)?;
    let services = preview["body"]["services"]
        .as_array()
        .ok_or(ConnectError::InvitationRejected)?;
    if services.len() != 1
        || services[0]["serviceId"] != "game"
        || services[0]["protocol"] != "tcp"
        || services[0]["revision"] != 1
        || services[0]["requiredCapability"] != "reliableStream"
    {
        return Err(ConnectError::InvitationRejected);
    }
    let latest = leases.borrow_and_update().clone();
    let sequence = serde_json::from_str::<Value>(&latest)
        .ok()
        .and_then(|value| value["body"]["sequence"].as_u64())
        .ok_or(ConnectError::InvitationRejected)?;
    if sequence > continuity.sequence() {
        continuity
            .update(&latest)
            .map_err(|_| ConnectError::InvitationRejected)?;
    }
    expected
        .check_current()
        .map_err(|_| ConnectError::PlanExpired)?;
    grant
        .check_admission()
        .map_err(|_| ConnectError::PlanExpired)?;
    let connecting = lab::Guest::connect_invited_session(run.key.clone(), &invitation, limits());
    tokio::pin!(connecting);
    let mut interrupted = false;
    let mut tick = interval(Duration::from_millis(20));
    let guest=loop{tokio::select!{biased;
        _=run.cancel.cancelled()=>{interrupted=true;break connecting.as_mut().await;},
        _=tick.tick()=>if run.current().is_err()||expected.check_current().is_err()||continuity.check().is_err()||heartbeat.is_finished(){interrupted=true;break connecting.as_mut().await;},
        result=leases.changed()=>{if result.is_err(){interrupted=true;break connecting.as_mut().await;}let lease=leases.borrow_and_update().clone();if continuity.update(&lease).is_err(){interrupted=true;break connecting.as_mut().await;}},
        result=&mut connecting=>break result,
    }}.map_err(|_|if run.cancel.is_cancelled(){ConnectError::Cancelled}else{ConnectError::TransportFailed})?;
    let sockets = guest.bound_sockets();
    let (result, tcp) = if interrupted
        || run.current().is_err()
        || expected.check_current().is_err()
        || continuity.check().is_err()
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
            grant,
            heartbeat,
            &mut continuity,
            &mut leases,
        )
        .await
    };
    guest.shutdown().await;
    drop(guest);
    release(&sockets, tcp).await?;
    result
}
async fn guest_connected(
    run: &Run,
    guest: &lab::Guest,
    expected: &lab::VerifiedSocialJoinContext,
    grant: &lab::VerifiedSocialGrant,
    heartbeat: &mut JoinHandle<Result<(), ConnectError>>,
    continuity: &mut lab::SocialContinuity,
    leases: &mut tokio::sync::watch::Receiver<String>,
) -> (Result<(), ConnectError>, Option<SocketAddr>) {
    let binding = guest.tcp_bridge("game", 1, "127.0.0.1:0".parse().expect("fixed loopback"));
    tokio::pin!(binding);
    let mut interrupted = false;
    let mut bind_tick = interval(Duration::from_millis(20));
    let bound = loop {
        tokio::select! {biased;
            _=run.cancel.cancelled()=>{interrupted=true;guest.shutdown().await;break binding.as_mut().await;},
            _=bind_tick.tick()=>if run.current().is_err()||expected.check_current().is_err()||continuity.check().is_err()||heartbeat.is_finished(){interrupted=true;guest.shutdown().await;break binding.as_mut().await;},
            result=leases.changed()=>{if result.is_err(){interrupted=true;guest.shutdown().await;break binding.as_mut().await;}let lease=leases.borrow_and_update().clone();if continuity.update(&lease).is_err(){interrupted=true;guest.shutdown().await;break binding.as_mut().await;}},
            result=&mut binding=>break result,
        }
    };
    let bridge = match bound {
        Ok(bridge) => bridge,
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
    let result = if interrupted
        || run.current().is_err()
        || expected.check_current().is_err()
        || grant.check_session().is_err()
        || continuity.check().is_err()
    {
        Err(if run.cancel.is_cancelled() {
            ConnectError::Cancelled
        } else {
            ConnectError::PlanExpired
        })
    } else {
        update(&run.state, &run.id, |a| {
            if !a.cancel.is_cancelled() {
                a.snapshot.phase = "guestConnected";
                a.snapshot.transport_state = "authenticated";
                a.snapshot.tcp_address = Some(tcp.to_string());
                a.snapshot.expires_at_ms = Some(grant.session_expires_at_unix_ms());
                a.session_deadline = Some(run.deadline);
                if let Some(social) = &mut a.snapshot.social {
                    social.stage = "connected";
                    social.expires_at_ms = Some(grant.session_expires_at_unix_ms());
                }
            }
        });
        let mut tick = interval(Duration::from_millis(20));
        loop {
            tokio::select! {
                biased;
                _ = run.cancel.cancelled() => break Ok(()),
                _ = guest.connection().closed() => break Ok(()),
                _ = tick.tick() => {
                    if run.current().is_err() || grant.check_session().is_err() {
                        break Ok(());
                    }
                    if continuity.check().is_err() {
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
                    if continuity.update(&lease).is_err() {
                        break Err(ConnectError::InvitationRejected);
                    }
                },
            }
        }
    };
    guest.shutdown().await;
    bridge.shutdown().await;
    drop(bridge);
    (result, Some(tcp))
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
    fn manager() -> Manager {
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
                    manager.shutdown_all().await;
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
        manager.shutdown_all().await;
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
