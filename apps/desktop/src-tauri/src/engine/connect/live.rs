use super::*;
use aurora_connect_lab as lab;
use sha2::{Digest, Sha256};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

enum Draft {
    Host {
        request: HostRequest,
        key: lab::SecretKey,
        bind: SocketAddr,
        service: lab::Service,
    },
    Guest {
        request: GuestRequest,
        key: lab::SecretKey,
        host: lab::EndpointId,
    },
}
struct Reviewed {
    draft: Draft,
    invitation: lab::Invitation,
    deadline: Instant,
    review_deadline: Instant,
}
struct Attempt {
    owner: String,
    snapshot: ConnectSnapshot,
    draft: Option<Draft>,
    reviewed: Option<Reviewed>,
    prepared_deadline: Instant,
    cancel: CancellationToken,
    worker: Option<JoinHandle<()>>,
    export: Option<InvitationExport>,
    host: Option<(Arc<lab::Host>, String)>,
    probe_active: bool,
    probes: u16,
    last_probe: Option<Instant>,
    session_deadline: Option<Instant>,
}
#[derive(Default)]
struct State {
    attempt: Option<Attempt>,
    generation: u64,
    shutting_down: bool,
}
#[derive(Default)]
pub(super) struct Manager {
    state: Arc<Mutex<State>>,
}
fn lock(state: &Mutex<State>) -> Result<std::sync::MutexGuard<'_, State>, ConnectError> {
    state.lock().map_err(|_| ConnectError::Internal)
}
fn selected<'a>(
    state: &'a mut State,
    owner: &str,
    id: &str,
) -> Result<&'a mut Attempt, ConnectError> {
    let a = state.attempt.as_mut().ok_or(ConnectError::StaleOperation)?;
    if a.owner != owner {
        return Err(ConnectError::OwnerMismatch);
    }
    if a.snapshot.operation_id.as_deref() != Some(id) {
        return Err(ConnectError::StaleOperation);
    }
    Ok(a)
}
fn canonical(text: &str) -> Result<SocketAddr, ConnectError> {
    if text.len() > 80 {
        return Err(ConnectError::InvalidRequest);
    }
    let address: SocketAddr = text.parse().map_err(|_| ConnectError::InvalidRequest)?;
    if address.to_string() != text || address.port() == 0 {
        return Err(ConnectError::InvalidRequest);
    }
    Ok(address)
}
fn policy(mode: ConnectMode, address: SocketAddr) -> Result<(), ConnectError> {
    match mode {
        ConnectMode::ExternalIpv6 => lab::validate_external_invitation_address(address)
            .map_err(|_| ConnectError::InvalidRequest),
        ConnectMode::Loopback
            if address.ip().is_loopback()
                && !matches!(address,SocketAddr::V6(v6) if v6.scope_id()!=0||v6.flowinfo()!=0) =>
        {
            Ok(())
        }
        _ => Err(ConnectError::InvalidRequest),
    }
}
fn peer(text: &str) -> Result<lab::EndpointId, ConnectError> {
    if text.len() != 64
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ConnectError::InvalidRequest);
    }
    text.parse().map_err(|_| ConnectError::InvalidRequest)
}
fn terminal(a: &Attempt) -> bool {
    !a.probe_active
        && matches!(a.snapshot.phase, "closed" | "failed")
        && a.snapshot.cleanup_state == "complete"
        && a.worker.as_ref().is_none_or(|worker| worker.is_finished())
}
fn expire_pending(a: &mut Attempt) {
    if matches!(
        a.snapshot.phase,
        "hostPrepared" | "guestPrepared" | "reviewed"
    ) {
        let deadline = a.reviewed.as_ref().map_or(a.prepared_deadline, |review| {
            review.review_deadline.min(review.deadline)
        });
        if Instant::now() >= deadline {
            a.draft = None;
            a.reviewed = None;
            a.snapshot.preparation = None;
            a.snapshot.review = None;
            a.snapshot.phase = "closed";
            a.snapshot.cleanup_state = "complete";
            a.snapshot.error_code = Some(ConnectError::PlanExpired.code());
            a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        }
    }
}
fn game(endpoint: SocketAddr) -> lab::Service {
    lab::Service {
        service_id: "game".into(),
        protocol: lab::Protocol::Tcp,
        endpoint,
        revision: 1,
        required_capability: lab::Capability::ReliableStream,
        max_datagram_bytes: 1400,
        udp_return_policy: lab::UdpReturnPolicy::ReplyBudget,
    }
}
fn update(state: &Mutex<State>, id: &str, change: impl FnOnce(&mut Attempt)) {
    if let Ok(mut state) = state.lock() {
        if let Some(a) = state.attempt.as_mut() {
            if a.snapshot.operation_id.as_deref() == Some(id) {
                change(a);
                a.snapshot.generation = a.snapshot.generation.saturating_add(1);
            }
        }
    }
}
fn finish(state: &Mutex<State>, id: &str, error: Option<ConnectError>) {
    update(state, id, |a| {
        a.snapshot.phase = if error.is_some() { "failed" } else { "closed" };
        a.snapshot.transport_state = "closed";
        a.snapshot.cleanup_state = "complete";
        a.snapshot.tcp_address = None;
        a.snapshot.error_code = error.map(ConnectError::code);
        a.export = None;
        a.host = None;
        a.snapshot.preparation = None;
        a.snapshot.review = None;
    });
}
struct ProbeGuard {
    state: Arc<Mutex<State>>,
    id: String,
}
impl Drop for ProbeGuard {
    fn drop(&mut self) {
        update(&self.state, &self.id, |a| {
            a.probe_active = false;
            if a.snapshot.status_state == "checking" {
                a.snapshot.status_state = "failed";
                a.snapshot.error_code = Some(ConnectError::Cancelled.code());
            }
        });
    }
}
async fn close_operation(
    state: &Mutex<State>,
    id: &str,
    cancel: &CancellationToken,
    error: Option<ConnectError>,
) {
    update(state, id, |a| {
        a.snapshot.phase = "closing";
        a.snapshot.cleanup_state = "pending";
    });
    cancel.cancel();
    loop {
        let active = state
            .lock()
            .ok()
            .and_then(|state| {
                state
                    .attempt
                    .as_ref()
                    .map(|a| a.snapshot.operation_id.as_deref() == Some(id) && a.probe_active)
            })
            .unwrap_or(false);
        if !active {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    finish(state, id, error);
}
impl Manager {
    pub(super) fn snapshot(&self, owner: &str) -> Result<ConnectSnapshot, ConnectError> {
        let mut state = lock(&self.state)?;
        if let Some(a) = state.attempt.as_mut() {
            if a.owner != owner {
                return Err(ConnectError::OwnerMismatch);
            }
            expire_pending(a);
            return Ok(a.snapshot.clone());
        }
        Ok(ConnectSnapshot::idle())
    }
    fn prepare(
        &self,
        owner: &str,
        draft: Draft,
        role: &'static str,
        public_id: String,
    ) -> Result<ConnectPreparation, ConnectError> {
        if owner.is_empty() || owner.len() > 128 {
            return Err(ConnectError::InvalidRequest);
        }
        let mut state = lock(&self.state)?;
        if state.shutting_down {
            return Err(ConnectError::Cancelled);
        }
        if let Some(a) = state.attempt.as_mut() {
            expire_pending(a);
            if !terminal(a) {
                return Err(ConnectError::Busy);
            }
        }
        state.generation = state
            .generation
            .max(state.attempt.as_ref().map_or(0, |a| a.snapshot.generation))
            .saturating_add(1);
        let preparation = ConnectPreparation {
            operation_id: uuid::Uuid::new_v4().to_string(),
            generation: state.generation,
            public_id,
            expires_at_ms: lab::unix_millis().saturating_add(60_000),
        };
        let mut snapshot = ConnectSnapshot::idle();
        snapshot.operation_id = Some(preparation.operation_id.clone());
        snapshot.generation = preparation.generation;
        snapshot.role = Some(role);
        snapshot.phase = if role == "host" {
            "hostPrepared"
        } else {
            "guestPrepared"
        };
        snapshot.preparation = Some(preparation.clone());
        state.attempt = Some(Attempt {
            owner: owner.into(),
            snapshot,
            draft: Some(draft),
            reviewed: None,
            prepared_deadline: Instant::now() + Duration::from_secs(60),
            cancel: CancellationToken::new(),
            worker: None,
            export: None,
            host: None,
            probe_active: false,
            probes: 0,
            last_probe: None,
            session_deadline: None,
        });
        Ok(preparation)
    }
    pub(super) fn prepare_host(
        &self,
        owner: &str,
        request: HostRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        if request.lifetime_seconds == 0 || request.lifetime_seconds > 600 {
            return Err(ConnectError::InvalidRequest);
        }
        let bind = canonical(&request.bind_address)?;
        policy(request.mode, bind)?;
        let target = canonical(&request.game_target)?;
        policy(ConnectMode::Loopback, target)?;
        let key = lab::generate_key();
        let public = key.public().to_string();
        self.prepare(
            owner,
            Draft::Host {
                request,
                key,
                bind,
                service: game(target),
            },
            "host",
            public,
        )
    }
    pub(super) fn prepare_guest(
        &self,
        owner: &str,
        request: GuestRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        let host = peer(&request.expected_host_public_id)?;
        let key = lab::generate_key();
        let public = key.public().to_string();
        self.prepare(owner, Draft::Guest { request, key, host }, "guest", public)
    }
    fn save_review(
        a: &mut Attempt,
        draft: Draft,
        invitation: lab::Invitation,
        deadline: Instant,
    ) -> Result<ConnectReview, ConnectError> {
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        let (mode, host_public_id, guest_public_id, target, lifetime) = match &draft {
            Draft::Host {
                request,
                key,
                service,
                ..
            } => (
                request.mode,
                key.public().to_string(),
                invitation.lease().peer.to_string(),
                Some(service.endpoint.to_string()),
                request.lifetime_seconds,
            ),
            Draft::Guest { request, key, host } => (
                request.mode,
                host.to_string(),
                key.public().to_string(),
                None,
                invitation
                    .expires_at_unix_ms()
                    .saturating_sub(lab::unix_millis())
                    .div_ceil(1000)
                    .min(600) as u16,
            ),
        };
        let mut review = ConnectReview {
            operation_id: a
                .snapshot
                .operation_id
                .clone()
                .ok_or(ConnectError::Internal)?,
            generation: a.snapshot.generation,
            digest: String::new(),
            invitation_digest: invitation.digest(),
            role: a.snapshot.role.ok_or(ConnectError::Internal)?,
            mode,
            host_public_id,
            guest_public_id,
            direct_address: invitation.direct_addr().to_string(),
            game_target: target,
            local_tcp_bind: if a.snapshot.role == Some("guest") {
                Some("127.0.0.1:0".into())
            } else {
                None
            },
            services: vec![ServiceReview {
                service_id: "game",
                protocol: "tcp",
                revision: 1,
                required_capability: "reliableStream",
            }],
            expires_at_ms: a
                .snapshot
                .preparation
                .as_ref()
                .ok_or(ConnectError::Internal)?
                .expires_at_ms
                .min(invitation.expires_at_unix_ms()),
            invitation_expires_at_ms: invitation.expires_at_unix_ms(),
            lifetime_seconds: lifetime,
            status_protocol: 767,
            status_handshake_host: "localhost",
            status_handshake_port: 43001,
        };
        let bytes = serde_json::to_vec(&review).map_err(|_| ConnectError::Internal)?;
        review.digest = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        a.reviewed = Some(Reviewed {
            draft,
            invitation,
            deadline,
            review_deadline: a.prepared_deadline,
        });
        a.snapshot.phase = "reviewed";
        a.snapshot.review = Some(review.clone());
        a.snapshot.expires_at_ms = Some(review.expires_at_ms);
        Ok(review)
    }
    pub(super) fn review_host(
        &self,
        owner: &str,
        id: &str,
        guest_public_id: &str,
    ) -> Result<ConnectReview, ConnectError> {
        let recipient = peer(guest_public_id)?;
        let mut state = lock(&self.state)?;
        let a = selected(&mut state, owner, id)?;
        expire_pending(a);
        if a.snapshot.phase != "hostPrepared" {
            return Err(ConnectError::PlanExpired);
        }
        let draft = a.draft.take().ok_or(ConnectError::StaleOperation)?;
        let Draft::Host {
            request,
            key,
            bind,
            service,
        } = &draft
        else {
            a.draft = Some(draft);
            return Err(ConnectError::InvalidRequest);
        };
        let deadline = Instant::now() + Duration::from_secs(request.lifetime_seconds.into());
        let expires = lab::unix_millis() + u64::from(request.lifetime_seconds) * 1000;
        let issue = match request.mode {
            ConnectMode::Loopback => lab::Invitation::issue,
            ConnectMode::ExternalIpv6 => lab::Invitation::issue_external_ipv6,
        };
        let invitation = match issue(
            key,
            recipient,
            id,
            *bind,
            std::slice::from_ref(service),
            expires,
        ) {
            Ok(invitation) => invitation,
            Err(_) => {
                a.draft = Some(draft);
                return Err(ConnectError::InvitationRejected);
            }
        };
        Self::save_review(a, draft, invitation, deadline)
    }
    pub(super) fn review_guest(
        &self,
        owner: &str,
        id: &str,
        encoded: &str,
    ) -> Result<ConnectReview, ConnectError> {
        let mut state = lock(&self.state)?;
        let a = selected(&mut state, owner, id)?;
        expire_pending(a);
        if a.snapshot.phase != "guestPrepared" {
            return Err(ConnectError::PlanExpired);
        }
        let draft = a.draft.take().ok_or(ConnectError::StaleOperation)?;
        let Draft::Guest { request, key, host } = &draft else {
            a.draft = Some(draft);
            return Err(ConnectError::InvalidRequest);
        };
        let verify = match request.mode {
            ConnectMode::Loopback => lab::Invitation::verify,
            ConnectMode::ExternalIpv6 => lab::Invitation::verify_external_ipv6,
        };
        let start = Instant::now();
        let now = lab::unix_millis();
        let invitation = match verify(encoded, *host, key.public()) {
            Ok(invitation) => invitation,
            Err(_) => {
                a.draft = Some(draft);
                return Err(ConnectError::InvitationRejected);
            }
        };
        let preview = invitation.preview();
        if preview["services"].as_array().is_none_or(|services| {
            services.len() != 1
                || services[0]["serviceId"] != "game"
                || services[0]["protocol"] != "tcp"
                || services[0]["revision"] != 1
                || services[0]["requiredCapability"] != "reliableStream"
        }) || !invitation.lease().services.contains("game")
        {
            a.draft = Some(draft);
            return Err(ConnectError::InvitationRejected);
        }
        let deadline =
            start + Duration::from_millis(invitation.expires_at_unix_ms().saturating_sub(now));
        Self::save_review(a, draft, invitation, deadline)
    }
    pub(super) async fn confirm(
        &self,
        owner: &str,
        id: &str,
        digest: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        let mut state = lock(&self.state)?;
        let a = selected(&mut state, owner, id)?;
        expire_pending(a);
        if a.snapshot.phase != "reviewed" {
            return Err(ConnectError::PlanExpired);
        }
        if a.snapshot
            .review
            .as_ref()
            .is_none_or(|review| review.digest != digest)
        {
            return Err(ConnectError::WrongDigest);
        }
        let reviewed = a.reviewed.take().ok_or(ConnectError::StaleOperation)?;
        if Instant::now() >= reviewed.deadline || Instant::now() >= reviewed.review_deadline {
            return Err(ConnectError::PlanExpired);
        }
        a.session_deadline = Some(reviewed.deadline);
        a.snapshot.expires_at_ms = Some(reviewed.invitation.expires_at_unix_ms());
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        a.snapshot.phase = "starting";
        a.snapshot.review = None;
        a.snapshot.preparation = None;
        a.snapshot.cleanup_state = "pending";
        let shared = self.state.clone();
        let operation = id.to_owned();
        let cancel = a.cancel.clone();
        a.worker = Some(tokio::spawn(async move {
            run(shared, operation, cancel, reviewed).await;
        }));
        Ok(a.snapshot.clone())
    }
    pub(super) fn export_invitation(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<InvitationExport, ConnectError> {
        let mut state = lock(&self.state)?;
        let a = selected(&mut state, owner, id)?;
        if a.session_deadline
            .is_none_or(|deadline| Instant::now() >= deadline)
        {
            return Err(ConnectError::PlanExpired);
        }
        if a.snapshot.phase != "hostListening"
            || a.host
                .as_ref()
                .is_none_or(|(host, lease)| host.invitation_consumed(lease))
        {
            return Err(ConnectError::StaleOperation);
        }
        a.export.clone().ok_or(ConnectError::StaleOperation)
    }
    pub(super) async fn probe_status(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        let (address, cancel) = {
            let mut state = lock(&self.state)?;
            let a = selected(&mut state, owner, id)?;
            if a.snapshot.phase != "guestConnected" {
                return Err(ConnectError::StaleOperation);
            }
            if a.session_deadline
                .is_none_or(|deadline| Instant::now() >= deadline)
            {
                return Err(ConnectError::PlanExpired);
            }
            if a.probe_active
                || a.probes >= 30
                || a.last_probe
                    .is_some_and(|at| at.elapsed() < Duration::from_secs(20))
            {
                return Err(ConnectError::Busy);
            }
            let address = a
                .snapshot
                .tcp_address
                .as_deref()
                .ok_or(ConnectError::Internal)?
                .parse()
                .map_err(|_| ConnectError::Internal)?;
            a.probe_active = true;
            a.probes += 1;
            a.last_probe = Some(Instant::now());
            a.snapshot.status_state = "checking";
            a.snapshot.generation = a.snapshot.generation.saturating_add(1);
            (address, a.cancel.clone())
        };
        let _probe_guard = ProbeGuard {
            state: self.state.clone(),
            id: id.into(),
        };
        let result = tokio::select! {_=cancel.cancelled()=>Err(ConnectError::Cancelled),result=lab::probe_minecraft_status(lab::MinecraftStatusRequest {endpoint:address,handshake_host:"localhost".into(),handshake_port:43001,protocol:767,timeout:lab::MAX_MINECRAFT_STATUS_TIMEOUT})=>result.map_err(|_|ConnectError::StatusFailed)};
        let mut state = lock(&self.state)?;
        let a = selected(&mut state, owner, id)?;
        a.probe_active = false;
        if a.snapshot.phase != "guestConnected" {
            return Ok(a.snapshot.clone());
        }
        match result {
            Ok(summary) => {
                a.snapshot.status_state = "responded";
                a.snapshot.error_code = None;
                a.snapshot.status_summary = Some(StatusSummary {
                    requested_protocol: summary.requested_protocol,
                    reported_protocol: summary.reported_protocol,
                    protocol_matches_requested: summary.protocol_matches_requested,
                    reported_players_online: summary.reported_players_online,
                    reported_players_max: summary.reported_players_max,
                    connect_micros: summary.connect_micros,
                    status_micros: summary.status_micros,
                    ping_micros: summary.ping_micros,
                    total_micros: summary.total_micros,
                });
            }
            Err(error) => {
                a.snapshot.status_state = "failed";
                a.snapshot.error_code = Some(error.code());
            }
        }
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        Ok(a.snapshot.clone())
    }
    pub(super) async fn stop(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        let (worker, already_closing) = {
            let mut state = lock(&self.state)?;
            let a = selected(&mut state, owner, id)?;
            a.cancel.cancel();
            a.draft = None;
            a.reviewed = None;
            a.export = None;
            a.snapshot.preparation = None;
            a.snapshot.review = None;
            if terminal(a) {
                return Ok(a.snapshot.clone());
            }
            let already_closing = a.snapshot.phase == "closing";
            a.snapshot.phase = "closing";
            a.snapshot.cleanup_state = "pending";
            a.snapshot.generation = a.snapshot.generation.saturating_add(1);
            (a.worker.take(), already_closing)
        };
        if let Some(worker) = worker {
            if worker.await.is_err() {
                update(&self.state, id, |a| {
                    a.snapshot.phase = "failed";
                    a.snapshot.cleanup_state = "failed";
                    a.snapshot.error_code = Some(ConnectError::Internal.code());
                });
            }
        } else if already_closing {
            loop {
                let snapshot = self.snapshot(owner)?;
                if snapshot.operation_id.as_deref() != Some(id) {
                    return Err(ConnectError::StaleOperation);
                }
                if snapshot.cleanup_state == "complete" || snapshot.cleanup_state == "failed" {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        } else {
            finish(&self.state, id, None);
        }
        loop {
            let snapshot = self.snapshot(owner)?;
            if snapshot.operation_id.as_deref() != Some(id) {
                return Err(ConnectError::StaleOperation);
            }
            let active = {
                let mut state = lock(&self.state)?;
                selected(&mut state, owner, id)?.probe_active
            };
            if !active {
                return Ok(snapshot);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    pub(super) async fn shutdown_all(&self) {
        let current = {
            if let Ok(mut state) = self.state.lock() {
                state.shutting_down = true;
                state
                    .attempt
                    .as_ref()
                    .map(|a| (a.owner.clone(), a.snapshot.operation_id.clone()))
            } else {
                None
            }
        };
        if let Some((owner, Some(id))) = current {
            let _ = self.stop(&owner, &id).await;
        }
    }
}
async fn run(state: Arc<Mutex<State>>, id: String, cancel: CancellationToken, reviewed: Reviewed) {
    if cancel.is_cancelled() || Instant::now() >= reviewed.deadline {
        close_operation(&state, &id, &cancel, None).await;
        return;
    }
    let Reviewed {
        draft,
        invitation,
        deadline,
        ..
    } = reviewed;
    match draft {
        Draft::Host {
            request,
            key,
            bind,
            service,
        } => {
            let lease = invitation.lease();
            let lease_id = lease.lease_id.clone();
            let config = lab::HostConfig {
                secret_key: key,
                bind_addr: bind,
                session_id: invitation.session_id().into(),
                services: vec![service],
                leases: vec![lease],
                limits: lab::Limits::default(),
            };
            let result = match request.mode {
                ConnectMode::Loopback => lab::Host::start_invited(config, &invitation).await,
                ConnectMode::ExternalIpv6 => {
                    lab::Host::start_invited_external_ipv6(config, &invitation).await
                }
            };
            let Ok(host) = result else {
                close_operation(
                    &state,
                    &id,
                    &cancel,
                    if cancel.is_cancelled() {
                        None
                    } else {
                        Some(ConnectError::TransportFailed)
                    },
                )
                .await;
                return;
            };
            let host = Arc::new(host);
            if !cancel.is_cancelled() && Instant::now() < deadline {
                update(&state, &id, |a| {
                    if a.snapshot.phase != "closing" {
                        a.snapshot.phase = "hostListening";
                        a.snapshot.transport_state = "listening";
                        a.host = Some((host.clone(), lease_id.clone()));
                        a.export = Some(InvitationExport {
                            invitation: invitation.encode(),
                            host_public_id: host.id().to_string(),
                            expires_at_ms: invitation.expires_at_unix_ms(),
                        });
                    }
                });
                let mut interval = tokio::time::interval(Duration::from_millis(100));
                let mut admitted = false;
                loop {
                    tokio::select! {
                        _=cancel.cancelled()=>break,
                        _=tokio::time::sleep_until(deadline.into())=>break,
                        _ = interval.tick() => {
                            let consumed = host.invitation_consumed(&lease_id);
                            let active = host.metrics().active_peers > 0;
                            if consumed && !admitted {
                                admitted = true;
                                update(&state, &id, |a| {
                                    a.export = None;
                                    if a.snapshot.phase != "closing" {
                                        a.snapshot.transport_state = "admitted";
                                    }
                                });
                            }
                            if consumed && !active { break; }
                        }
                    }
                }
            }
            cancel.cancel();
            update(&state, &id, |a| {
                a.snapshot.phase = "closing";
                a.host = None;
                a.export = None;
            });
            host.revoke(&lease_id);
            host.shutdown().await;
            drop(host);
            close_operation(&state, &id, &cancel, None).await;
        }
        Draft::Guest { request, key, .. } => {
            // Do not select/drop/abort the connect future: it owns an endpoint
            // during dial. Cancellation is recorded immediately, then a late
            // success is shut down before any listener can be published.
            let result = match request.mode {
                ConnectMode::Loopback => {
                    lab::Guest::connect_invited(key, &invitation, lab::Limits::default()).await
                }
                ConnectMode::ExternalIpv6 => {
                    lab::Guest::connect_invited_external_ipv6(
                        key,
                        &invitation,
                        lab::Limits::default(),
                    )
                    .await
                }
            };
            let Ok(guest) = result else {
                close_operation(
                    &state,
                    &id,
                    &cancel,
                    if cancel.is_cancelled() {
                        None
                    } else {
                        Some(ConnectError::TransportFailed)
                    },
                )
                .await;
                return;
            };
            if cancel.is_cancelled() || Instant::now() >= deadline {
                guest.shutdown().await;
                drop(guest);
                close_operation(&state, &id, &cancel, None).await;
                return;
            }
            let bridge = guest
                .tcp_bridge("game", 1, "127.0.0.1:0".parse().expect("fixed loopback"))
                .await;
            let Ok(bridge) = bridge else {
                guest.shutdown().await;
                drop(guest);
                close_operation(&state, &id, &cancel, Some(ConnectError::PortUnavailable)).await;
                return;
            };
            if !cancel.is_cancelled() && Instant::now() < deadline {
                update(&state, &id, |a| {
                    if a.snapshot.phase != "closing" {
                        a.snapshot.phase = "guestConnected";
                        a.snapshot.transport_state = "authenticated";
                        a.snapshot.tcp_address = Some(bridge.local_addr().to_string());
                    }
                });
                tokio::select! {_=cancel.cancelled()=>{},_=tokio::time::sleep_until(deadline.into())=>{},_=guest.connection().closed()=>{}}
            }
            cancel.cancel();
            update(&state, &id, |a| {
                a.snapshot.phase = "closing";
            });
            guest.shutdown().await;
            bridge.shutdown().await;
            drop(bridge);
            drop(guest);
            close_operation(&state, &id, &cancel, None).await;
        }
    }
}
