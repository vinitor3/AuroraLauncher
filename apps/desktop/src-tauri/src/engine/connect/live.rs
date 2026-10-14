use super::fragmented_profile::{self, FragmentedGuestState, UdpFailure};
use super::session_authority::{Authority, HostDraftRequest, Profile};
use super::*;
#[cfg(test)]
#[path = "direct_ipv4_tests.rs"]
mod direct_ipv4_tests;
use aurora_connect_lab as lab;
use sha2::{Digest, Sha256};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
#[cfg(all(test, windows))]
#[path = "owned_fragmented_tests.rs"]
mod owned_fragmented_tests;
#[path = "udp_recovery.rs"]
mod udp_recovery;
#[cfg(test)]
#[path = "udp_recovery_tests.rs"]
mod udp_recovery_tests;

#[cfg(test)]
#[path = "fragmented_desktop.rs"]
mod fragmented_desktop;
#[cfg(test)]
#[path = "session_fragmented_tests.rs"]
mod session_fragmented_tests;
#[cfg(test)]
#[path = "session_tests.rs"]
mod session_tests;
#[cfg(test)]
#[path = "stop_completion_tests.rs"]
mod stop_completion_tests;

enum TargetGuard {
    Manual,
    #[cfg(windows)]
    Owned(Arc<super::super::connect_discovery::ActiveOwnedTarget>),
}
struct TargetProjection {
    kind: &'static str,
    instance_id: Option<String>,
    run_id: Option<String>,
    fingerprint: Option<(u32, u64, String)>,
}
impl TargetGuard {
    fn recheck(&self) -> Result<Option<SocketAddr>, ConnectError> {
        match self {
            Self::Manual => Ok(None),
            #[cfg(windows)]
            Self::Owned(target) => target
                .recheck_owned()
                .map(Some)
                .map_err(|_| ConnectError::SourceChanged),
        }
    }
    fn projection(&self) -> TargetProjection {
        match self {
            Self::Manual => TargetProjection {
                kind: "manual",
                instance_id: None,
                run_id: None,
                fingerprint: None,
            },
            #[cfg(windows)]
            Self::Owned(target) => TargetProjection {
                kind: "ownedInstance",
                instance_id: Some(target.instance_id().into()),
                run_id: Some(target.run_id().into()),
                fingerprint: Some(target.fingerprint()),
            },
        }
    }
}

enum Draft {
    Host {
        request: HostDraftRequest,
        key: lab::SecretKey,
        bind: SocketAddr,
        services: Vec<lab::Service>,
        source: TargetGuard,
    },
    Guest {
        request: GuestRequest,
        key: lab::SecretKey,
        host: lab::EndpointId,
        profile: Profile,
    },
}
struct Reviewed {
    draft: Draft,
    invitation: Authority,
    admission_deadline: Instant,
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
    admission_deadline: Option<Instant>,
    authority: Option<Authority>,
    recovery: Option<udp_recovery::RecoveryControl>,
    #[cfg(test)]
    recovery_gate: Option<Arc<udp_recovery::TestGate>>,
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
impl Drop for Manager {
    fn drop(&mut self) {
        if let Ok(state) = self.state.lock() {
            if let Some(attempt) = &state.attempt {
                attempt.cancel.cancel();
            }
        }
    }
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
        ConnectMode::DirectIpv4 => lab::validate_direct_ipv4_invitation_address(address)
            .map_err(|_| ConnectError::InvalidRequest),
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
fn admission_state(
    current: impl FnOnce() -> bool,
    consumed: impl FnOnce() -> bool,
) -> Option<bool> {
    // Read consumption after the clock gate: an admission valid just before
    // ingress expired must not be discarded using an earlier false snapshot.
    let current = current();
    let consumed = consumed();
    (current || consumed).then_some(consumed)
}
fn expire_pending(a: &mut Attempt) {
    expire_pending_at(a, lab::unix_millis());
}
fn expire_pending_at(a: &mut Attempt, now_unix: u64) {
    if matches!(
        a.snapshot.phase,
        "hostPrepared" | "guestPrepared" | "reviewed"
    ) {
        let deadline = a.reviewed.as_ref().map_or(a.prepared_deadline, |review| {
            review.review_deadline.min(review.admission_deadline)
        });
        let admission_invalid = a
            .reviewed
            .as_ref()
            .is_some_and(|review| review.invitation.check_admission().is_err());
        let review_expired = a
            .snapshot
            .preparation
            .as_ref()
            .is_none_or(|p| now_unix >= p.expires_at_ms);
        if Instant::now() >= deadline || admission_invalid || review_expired {
            a.draft = None;
            a.reviewed = None;
            a.snapshot.preparation = None;
            a.snapshot.review = None;
            a.snapshot.phase = "closed";
            a.snapshot.cleanup_state = "complete";
            a.snapshot.error_code = Some(ConnectError::PlanExpired.code());
            if let Some(udp) = &mut a.snapshot.udp {
                udp.close();
            }
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
        udp_recovery::clear(a);
        a.snapshot.phase = if error.is_some() { "failed" } else { "closed" };
        a.snapshot.transport_state = "closed";
        a.snapshot.cleanup_state = "complete";
        a.snapshot.tcp_address = None;
        if let Some(udp) = &mut a.snapshot.udp {
            udp.close();
        }
        a.snapshot.error_code = error.map(ConnectError::code);
        a.export = None;
        a.host = None;
        a.authority = None;
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
        udp_recovery::clear(a);
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

// A pending UDP setup/teardown owns resources and must not be dropped. Keep
// observing session authority while it waits; closing Guest cuts TCP as well
// and unblocks native requests, then the exact same future is reaped.
async fn await_udp_work<T>(
    guest: &lab::Guest,
    cancel: &CancellationToken,
    deadline: Instant,
    current: impl Fn() -> bool,
    work: impl std::future::Future<Output = T>,
) -> (T, bool) {
    tokio::pin!(work);
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            value=&mut work=>return(value,false),
            _=cancel.cancelled()=>break,
            _=tokio::time::sleep_until(deadline.into())=>break,
            _=guest.connection().closed()=>break,
            _=tick.tick()=>if !current(){break;},
        }
    }
    guest.shutdown().await;
    (work.await, true)
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
        let profile = match &draft {
            Draft::Host { request, .. } => request.profile,
            Draft::Guest { profile, .. } => *profile,
        };
        let preparation = ConnectPreparation {
            operation_id: uuid::Uuid::new_v4().to_string(),
            generation: state.generation,
            public_id,
            expires_at_ms: lab::unix_millis().saturating_add(60_000),
            invitation_profile: profile.name(),
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
        snapshot.invitation_profile = profile.name();
        if profile.is_fragmented() && role == "guest" {
            snapshot.udp = Some(FragmentedGuestState::default());
        }
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
            admission_deadline: None,
            authority: None,
            recovery: None,
            #[cfg(test)]
            recovery_gate: None,
        });
        Ok(preparation)
    }
    pub(super) fn prepare_host(
        &self,
        owner: &str,
        request: HostRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        self.prepare_host_internal(owner, request.into(), TargetGuard::Manual)
    }
    #[cfg(windows)]
    pub(super) fn prepare_host_discovered(
        &self,
        owner: &str,
        request: HostRequest,
        target: super::super::connect_discovery::ApprovedTarget,
    ) -> Result<ConnectPreparation, ConnectError> {
        let target = target.activate().map_err(|error| {
            if error == super::super::connect_discovery::DiscoveryError::Stale {
                ConnectError::PlanExpired
            } else {
                ConnectError::SourceChanged
            }
        })?;
        self.prepare_host_internal(owner, request.into(), TargetGuard::Owned(Arc::new(target)))
    }
    pub(super) fn prepare_fragmented_host(
        &self,
        owner: &str,
        request: FragmentedHostRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        self.prepare_host_internal(owner, request.into(), TargetGuard::Manual)
    }
    pub(super) fn prepare_session_host(
        &self,
        owner: &str,
        request: SessionHostRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        self.prepare_host_internal(owner, request.into(), TargetGuard::Manual)
    }
    pub(super) fn prepare_session_fragmented_host(
        &self,
        owner: &str,
        request: SessionFragmentedHostRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        self.prepare_host_internal(owner, request.into(), TargetGuard::Manual)
    }
    #[cfg(windows)]
    pub(super) fn prepare_session_fragmented_host_discovered(
        &self,
        owner: &str,
        request: SessionFragmentedDiscoveredHostRequest,
        target: super::super::connect_discovery::ApprovedTarget,
    ) -> Result<ConnectPreparation, ConnectError> {
        let target = target.activate().map_err(|error| {
            if error == super::super::connect_discovery::DiscoveryError::Stale {
                ConnectError::PlanExpired
            } else {
                ConnectError::SourceChanged
            }
        })?;
        let game_target = target
            .recheck_owned()
            .map_err(|_| ConnectError::SourceChanged)?
            .to_string();
        let request = SessionFragmentedHostRequest {
            mode: request.mode,
            bind_address: request.bind_address,
            game_target,
            udp_target: request.udp_target,
            admission_seconds: request.admission_seconds,
            session_seconds: request.session_seconds,
        };
        self.prepare_host_internal(owner, request.into(), TargetGuard::Owned(Arc::new(target)))
    }
    #[cfg(windows)]
    pub(super) fn prepare_session_host_discovered(
        &self,
        owner: &str,
        request: SessionHostRequest,
        target: super::super::connect_discovery::ApprovedTarget,
    ) -> Result<ConnectPreparation, ConnectError> {
        let target = target.activate().map_err(|error| {
            if error == super::super::connect_discovery::DiscoveryError::Stale {
                ConnectError::PlanExpired
            } else {
                ConnectError::SourceChanged
            }
        })?;
        self.prepare_host_internal(owner, request.into(), TargetGuard::Owned(Arc::new(target)))
    }
    fn prepare_host_internal(
        &self,
        owner: &str,
        request: HostDraftRequest,
        source: TargetGuard,
    ) -> Result<ConnectPreparation, ConnectError> {
        request.validate()?;
        let bind = canonical(&request.bind_address)?;
        policy(request.mode, bind)?;
        let target = canonical(&request.game_target)?;
        policy(ConnectMode::Loopback, target)?;
        if source.recheck()?.is_some_and(|observed| observed != target) {
            return Err(ConnectError::InvalidRequest);
        }
        let services = if let Some(udp) = &request.udp_target {
            fragmented_profile::services(target, canonical(udp)?)?
        } else {
            vec![game(target)]
        };
        let key = lab::generate_key();
        let public = key.public().to_string();
        self.prepare(
            owner,
            Draft::Host {
                request,
                key,
                bind,
                services,
                source,
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
        self.prepare_guest_internal(owner, request, Profile::Legacy)
    }
    pub(super) fn prepare_fragmented_guest(
        &self,
        owner: &str,
        request: GuestRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        self.prepare_guest_internal(owner, request, Profile::Fragmented)
    }
    pub(super) fn prepare_session_guest(
        &self,
        owner: &str,
        request: GuestRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        self.prepare_guest_internal(owner, request, Profile::Session)
    }
    pub(super) fn prepare_session_fragmented_guest(
        &self,
        owner: &str,
        request: GuestRequest,
    ) -> Result<ConnectPreparation, ConnectError> {
        self.prepare_guest_internal(owner, request, Profile::SessionFragmented)
    }
    fn prepare_guest_internal(
        &self,
        owner: &str,
        request: GuestRequest,
        profile: Profile,
    ) -> Result<ConnectPreparation, ConnectError> {
        let host = peer(&request.expected_host_public_id)?;
        let key = lab::generate_key();
        let public = key.public().to_string();
        self.prepare(
            owner,
            Draft::Guest {
                request,
                key,
                host,
                profile,
            },
            "guest",
            public,
        )
    }
    fn save_review(
        a: &mut Attempt,
        draft: Draft,
        invitation: Authority,
        admission_deadline: Instant,
        deadline: Instant,
    ) -> Result<ConnectReview, ConnectError> {
        let (admission_deadline, deadline) = invitation
            .deadlines((admission_deadline, deadline))
            .map_err(|_| ConnectError::PlanExpired)?;
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        let (source, instance_id, run_id, fingerprint) = match &draft {
            Draft::Host { source, .. } => {
                let projection = source.projection();
                (
                    Some(projection.kind),
                    projection.instance_id,
                    projection.run_id,
                    projection.fingerprint,
                )
            }
            Draft::Guest { .. } => (None, None, None, None),
        };
        let (mode, host_public_id, guest_public_id, target, lifetime) = match &draft {
            Draft::Host {
                request,
                key,
                services,
                ..
            } => (
                request.mode,
                key.public().to_string(),
                invitation.lease().peer.to_string(),
                Some(services[0].endpoint.to_string()),
                request.admission_seconds,
            ),
            Draft::Guest {
                request, key, host, ..
            } => (
                request.mode,
                host.to_string(),
                key.public().to_string(),
                None,
                invitation
                    .admission_expires()
                    .saturating_sub(lab::unix_millis())
                    .div_ceil(1000)
                    .min(600) as u16,
            ),
        };
        let session_timing = invitation.timing()?;
        let fragmented_udp = invitation
            .fragmented_contract()
            .map(|contract| FragmentedUdpReview {
                contract,
                udp_target: match &draft {
                    Draft::Host { request, .. } => request.udp_target.clone(),
                    Draft::Guest { .. } => None,
                },
            });
        let lifetime = invitation.fragmented_seconds().unwrap_or(lifetime);
        let lifetime = session_timing
            .as_ref()
            .map_or(lifetime, |timing| timing.admission_seconds);
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
            services: if invitation.is_fragmented() {
                vec![
                    ServiceReview {
                        service_id: "game",
                        protocol: "tcp",
                        revision: 1,
                        required_capability: "reliableStream",
                    },
                    ServiceReview {
                        service_id: "aux",
                        protocol: "udp",
                        revision: 1,
                        required_capability: "fragmentedDatagram",
                    },
                ]
            } else {
                vec![ServiceReview {
                    service_id: "game",
                    protocol: "tcp",
                    revision: 1,
                    required_capability: "reliableStream",
                }]
            },
            expires_at_ms: a
                .snapshot
                .preparation
                .as_ref()
                .ok_or(ConnectError::Internal)?
                .expires_at_ms
                .min(invitation.admission_expires()),
            invitation_expires_at_ms: invitation.admission_expires(),
            lifetime_seconds: lifetime,
            status_protocol: 767,
            status_handshake_host: "localhost",
            status_handshake_port: 43001,
            target_source: source,
            instance_id,
            run_id,
            session_timing: session_timing.clone(),
            fragmented_udp: fragmented_udp.clone(),
        };
        let bytes =
            serde_json::to_vec(&(fingerprint, &review)).map_err(|_| ConnectError::Internal)?;
        review.digest = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        a.reviewed = Some(Reviewed {
            draft,
            invitation,
            admission_deadline,
            deadline,
            review_deadline: a.prepared_deadline,
        });
        a.snapshot.phase = "reviewed";
        a.snapshot.review = Some(review.clone());
        a.snapshot.session_timing = session_timing;
        a.snapshot.fragmented_udp = fragmented_udp.map(|value| value.contract);
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
            services,
            source,
        } = &draft
        else {
            a.draft = Some(draft);
            return Err(ConnectError::InvalidRequest);
        };
        if let Err(error) = source.recheck() {
            a.draft = Some(draft);
            return Err(error);
        }
        let at = Instant::now();
        let now = lab::unix_millis();
        let admission_deadline = at + Duration::from_secs(request.admission_seconds.into());
        let deadline = at + Duration::from_secs(request.session_seconds.into());
        let admission_expires = now + u64::from(request.admission_seconds) * 1000;
        let issued = match request.profile {
            Profile::Legacy => {
                let issue = match request.mode {
                    ConnectMode::Loopback => lab::Invitation::issue,
                    ConnectMode::ExternalIpv6 => lab::Invitation::issue_external_ipv6,
                    ConnectMode::DirectIpv4 => lab::Invitation::issue_direct_ipv4,
                };
                issue(key, recipient, id, *bind, services, admission_expires).map(Authority::Legacy)
            }
            Profile::Fragmented => {
                let issue = match request.mode {
                    ConnectMode::Loopback => lab::Invitation::issue_fragmented_for_lifetime,
                    ConnectMode::DirectIpv4 => {
                        lab::Invitation::issue_fragmented_direct_ipv4_for_lifetime
                    }
                    ConnectMode::ExternalIpv6 => {
                        lab::Invitation::issue_fragmented_external_ipv6_for_lifetime
                    }
                };
                issue(
                    key,
                    recipient,
                    id,
                    *bind,
                    services,
                    Duration::from_secs(request.admission_seconds.into()),
                )
                .map(Authority::Fragmented)
            }
            Profile::Session => {
                let issue = match request.mode {
                    ConnectMode::Loopback => lab::SessionInvitation::issue_loopback_for_lifetimes,
                    ConnectMode::DirectIpv4 => {
                        lab::SessionInvitation::issue_direct_ipv4_for_lifetimes
                    }
                    ConnectMode::ExternalIpv6 => {
                        lab::SessionInvitation::issue_external_ipv6_for_lifetimes
                    }
                };
                issue(
                    key,
                    recipient,
                    id,
                    *bind,
                    services,
                    Duration::from_secs(request.admission_seconds.into()),
                    Duration::from_secs(request.session_seconds.into()),
                )
                .map(Authority::Session)
            }
            Profile::SessionFragmented => {
                let issue = match request.mode {
                    ConnectMode::Loopback => {
                        lab::SessionInvitation::issue_fragmented_loopback_for_lifetimes
                    }
                    ConnectMode::DirectIpv4 => {
                        lab::SessionInvitation::issue_fragmented_direct_ipv4_for_lifetimes
                    }
                    ConnectMode::ExternalIpv6 => {
                        lab::SessionInvitation::issue_fragmented_external_ipv6_for_lifetimes
                    }
                };
                issue(
                    key,
                    recipient,
                    id,
                    *bind,
                    services,
                    Duration::from_secs(request.admission_seconds.into()),
                    Duration::from_secs(request.session_seconds.into()),
                )
                .map(Authority::SessionFragmented)
            }
        };
        let invitation = match issued {
            Ok(invitation) => invitation,
            Err(_) => {
                a.draft = Some(draft);
                return Err(ConnectError::InvitationRejected);
            }
        };
        Self::save_review(a, draft, invitation, admission_deadline, deadline)
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
        let Draft::Guest {
            request,
            key,
            host,
            profile,
        } = &draft
        else {
            a.draft = Some(draft);
            return Err(ConnectError::InvalidRequest);
        };
        let start = Instant::now();
        let now = lab::unix_millis();
        let verified = match profile {
            Profile::Legacy => {
                let verify = match request.mode {
                    ConnectMode::Loopback => lab::Invitation::verify,
                    ConnectMode::ExternalIpv6 => lab::Invitation::verify_external_ipv6,
                    ConnectMode::DirectIpv4 => lab::Invitation::verify_direct_ipv4,
                };
                verify(encoded, *host, key.public()).map(Authority::Legacy)
            }
            Profile::Fragmented => {
                let verify = match request.mode {
                    ConnectMode::Loopback => lab::Invitation::verify_fragmented,
                    ConnectMode::ExternalIpv6 => lab::Invitation::verify_fragmented_external_ipv6,
                    ConnectMode::DirectIpv4 => lab::Invitation::verify_fragmented_direct_ipv4,
                };
                verify(encoded, *host, key.public()).map(Authority::Fragmented)
            }
            Profile::Session => {
                let verify = match request.mode {
                    ConnectMode::Loopback => lab::SessionInvitation::verify_loopback,
                    ConnectMode::ExternalIpv6 => lab::SessionInvitation::verify_external_ipv6,
                    ConnectMode::DirectIpv4 => lab::SessionInvitation::verify_direct_ipv4,
                };
                verify(encoded, *host, key.public()).map(Authority::Session)
            }
            Profile::SessionFragmented => {
                let verify = match request.mode {
                    ConnectMode::Loopback => lab::SessionInvitation::verify_fragmented_loopback,
                    ConnectMode::DirectIpv4 => {
                        lab::SessionInvitation::verify_fragmented_direct_ipv4
                    }
                    ConnectMode::ExternalIpv6 => {
                        lab::SessionInvitation::verify_fragmented_external_ipv6
                    }
                };
                verify(encoded, *host, key.public()).map(Authority::SessionFragmented)
            }
        };
        let invitation = match verified {
            Ok(invitation) if invitation.validate_desktop().is_ok() => invitation,
            _ => {
                a.draft = Some(draft);
                return Err(ConnectError::InvitationRejected);
            }
        };
        let preview = invitation.preview();
        if !invitation.is_fragmented()
            && (preview["services"].as_array().is_none_or(|services| {
                services.len() != 1
                    || services[0]["serviceId"] != "game"
                    || services[0]["protocol"] != "tcp"
                    || services[0]["revision"] != 1
                    || services[0]["requiredCapability"] != "reliableStream"
            }) || !invitation.lease().services.contains("game"))
        {
            a.draft = Some(draft);
            return Err(ConnectError::InvitationRejected);
        }
        let admission_deadline =
            start + Duration::from_millis(invitation.admission_expires().saturating_sub(now));
        let deadline =
            start + Duration::from_millis(invitation.session_expires().saturating_sub(now));
        Self::save_review(a, draft, invitation, admission_deadline, deadline)
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
        // Approval cannot pass a preparation deadline crossed while inspecting
        // the digest. This checks civil time as well as the original Instant.
        expire_pending(a);
        if a.snapshot.phase != "reviewed" {
            return Err(ConnectError::PlanExpired);
        }
        let reviewed = a.reviewed.take().ok_or(ConnectError::StaleOperation)?;
        if Instant::now() >= reviewed.admission_deadline
            || Instant::now() >= reviewed.review_deadline
            || reviewed.invitation.check_admission().is_err()
        {
            return Err(ConnectError::PlanExpired);
        }
        a.session_deadline = Some(reviewed.deadline);
        a.admission_deadline = Some(reviewed.admission_deadline);
        a.authority = Some(reviewed.invitation.clone());
        a.snapshot.expires_at_ms = Some(reviewed.invitation.session_expires());
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
        if a.admission_deadline
            .is_none_or(|deadline| Instant::now() >= deadline)
            || a.authority
                .as_ref()
                .is_none_or(|authority| authority.check_admission().is_err())
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
                || a.authority
                    .as_ref()
                    .is_none_or(|authority| authority.check_session().is_err())
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
        if a.authority
            .as_ref()
            .is_none_or(|value| value.check_session().is_err())
        {
            a.snapshot.status_state = "failed";
            a.snapshot.error_code = Some(ConnectError::PlanExpired.code());
            a.snapshot.generation = a.snapshot.generation.saturating_add(1);
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
        let finish_without_worker = {
            let mut state = lock(&self.state)?;
            let a = selected(&mut state, owner, id)?;
            a.cancel.cancel();
            udp_recovery::clear(a);
            a.draft = None;
            a.reviewed = None;
            a.export = None;
            a.snapshot.preparation = None;
            a.snapshot.review = None;
            let completion_published = matches!(a.snapshot.phase, "closed" | "failed")
                && matches!(a.snapshot.cleanup_state, "complete" | "failed");
            let already_closing = a.snapshot.phase == "closing";
            // The actor publishes cleanup before its task returns. Preserve that
            // publication while retaining the task as the actual lifetime gate.
            if !completion_published && !already_closing {
                a.snapshot.phase = "closing";
                a.snapshot.cleanup_state = "pending";
                a.snapshot.generation = a.snapshot.generation.saturating_add(1);
            }
            a.worker.is_none() && !completion_published && !already_closing
        };
        if finish_without_worker {
            finish(&self.state, id, None);
        }
        loop {
            let finished_worker = {
                let mut state = lock(&self.state)?;
                let a = selected(&mut state, owner, id)?;
                // Never transfer an unfinished task to an IPC waiter: another
                // stop, shutdown or preparation must still see its ownership if
                // that waiter is dropped before the actor finishes.
                if a.worker.as_ref().is_some_and(|worker| worker.is_finished()) {
                    a.worker.take()
                } else {
                    if a.worker.is_none()
                        && !a.probe_active
                        && matches!(a.snapshot.phase, "closed" | "failed")
                        && matches!(a.snapshot.cleanup_state, "complete" | "failed")
                    {
                        return Ok(a.snapshot.clone());
                    }
                    None
                }
            };
            if let Some(worker) = finished_worker {
                if worker.await.is_err() {
                    update(&self.state, id, |a| {
                        a.snapshot.phase = "failed";
                        a.snapshot.cleanup_state = "failed";
                        a.snapshot.error_code = Some(ConnectError::Internal.code());
                    });
                }
                continue;
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
    if cancel.is_cancelled()
        || Instant::now() >= reviewed.admission_deadline
        || reviewed.invitation.check_admission().is_err()
    {
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
            services,
            source,
        } => {
            if source.recheck().is_err() {
                close_operation(&state, &id, &cancel, Some(ConnectError::SourceChanged)).await;
                return;
            }
            let lease = invitation.lease();
            let lease_id = lease.lease_id.clone();
            let config = lab::HostConfig {
                secret_key: key,
                bind_addr: bind,
                session_id: invitation.session_id().into(),
                services,
                leases: vec![lease],
                limits: lab::Limits::default(),
            };
            let result = invitation.start_host(request.mode, config).await;
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
            let mut source_error = source.recheck().err();
            if source_error.is_none()
                && !cancel.is_cancelled()
                && Instant::now() < deadline
                && invitation.check_admission().is_ok()
            {
                update(&state, &id, |a| {
                    if a.snapshot.phase != "closing" {
                        a.snapshot.phase = "hostListening";
                        a.snapshot.transport_state = "listening";
                        a.host = Some((host.clone(), lease_id.clone()));
                        a.export = Some(InvitationExport {
                            invitation: invitation.encode(),
                            host_public_id: host.id().to_string(),
                            expires_at_ms: invitation.admission_expires(),
                            session_timing: a.snapshot.session_timing.clone(),
                            fragmented_udp: a.snapshot.fragmented_udp.clone(),
                        });
                    }
                });
                let mut interval = tokio::time::interval(Duration::from_millis(100));
                let mut admitted = false;
                let mut source_checked = Instant::now();
                loop {
                    tokio::select! {
                        _=cancel.cancelled()=>break,
                        _=tokio::time::sleep_until(deadline.into())=>break,
                        _ = interval.tick() => {
                            if source_checked.elapsed()>=Duration::from_secs(1){source_checked=Instant::now();
                                #[cfg(windows)] if let TargetGuard::Owned(target)=&source {let target=target.clone();if !matches!(tokio::task::spawn_blocking(move||target.recheck_owned()).await,Ok(Ok(_))){source_error=Some(ConnectError::SourceChanged);break;}}
                            }
                            if invitation.check_session().is_err() { break; }
                            let Some(consumed) = admission_state(
                                || invitation.check_admission().is_ok(),
                                || host.invitation_consumed(&lease_id),
                            ) else { break; };
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
                if let Some(error) = source_error {
                    update(&state, &id, |a| a.snapshot.error_code = Some(error.code()));
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
            let source_error = source_error.or_else(|| source.recheck().err());
            close_operation(&state, &id, &cancel, source_error).await;
        }
        Draft::Guest { request, key, .. } => {
            // Do not select/drop/abort the connect future: it owns an endpoint
            // during dial. Cancellation is recorded immediately, then a late
            // success is shut down before any listener can be published.
            let result = invitation.connect_guest(request.mode, key).await;
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
            if cancel.is_cancelled()
                || Instant::now() >= deadline
                || invitation.check_session().is_err()
            {
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
            let mut udp_bridge = None;
            let recoverable = matches!(invitation, Authority::SessionFragmented(_));
            let (recovery_sender, mut recovery_receiver) = tokio::sync::mpsc::channel(1);
            if !cancel.is_cancelled()
                && Instant::now() < deadline
                && invitation.check_session().is_ok()
            {
                update(&state, &id, |a| {
                    if a.snapshot.phase != "closing" {
                        a.snapshot.phase = "guestConnected";
                        a.snapshot.transport_state = "authenticated";
                        a.snapshot.tcp_address = Some(bridge.local_addr().to_string());
                        if invitation.is_fragmented() {
                            a.snapshot.udp = Some(FragmentedGuestState::starting());
                        }
                        if recoverable {
                            a.recovery =
                                Some(udp_recovery::RecoveryControl::new(recovery_sender, &guest));
                            a.snapshot.udp_recovery = Some(UdpRecoveryState::default());
                        }
                    }
                });
                if invitation.is_fragmented() {
                    let ((opened, projection), interrupted) = await_udp_work(
                        &guest,
                        &cancel,
                        deadline,
                        || invitation.check_session().is_ok(),
                        fragmented_profile::open_guest_bridge(&guest),
                    )
                    .await;
                    udp_bridge = opened;
                    if !interrupted
                        && !cancel.is_cancelled()
                        && Instant::now() < deadline
                        && invitation.check_session().is_ok()
                    {
                        update(&state, &id, |a| {
                            if a.snapshot.phase == "guestConnected" {
                                a.snapshot.udp = Some(projection);
                            }
                        });
                    }
                }
                let mut tick = tokio::time::interval(Duration::from_millis(100));
                loop {
                    tokio::select! {
                        _=cancel.cancelled()=>break,
                        _=tokio::time::sleep_until(deadline.into())=>break,
                        _=guest.connection().closed()=>break,
                        request=recovery_receiver.recv(), if recoverable=>{
                            let Some(sequence)=request else { break; };
                            if !udp_recovery::current(&state,&id,sequence) { break; }
                            if let Some(udp)=udp_bridge.take() {
                                let (_,interrupted)=await_udp_work(&guest,&cancel,deadline,||invitation.check_session().is_ok(),udp.shutdown()).await;
                                drop(udp);
                                if interrupted || !udp_recovery::current(&state,&id,sequence) { break; }
                            }
                            #[cfg(test)]
                            let gate=state.lock().ok().and_then(|state|state.attempt.as_ref().and_then(|a|a.recovery_gate.clone()));
                            if guest.connection().close_reason().is_some() || !udp_recovery::current(&state,&id,sequence) { break; }
                            let setup=async {
                                let result=fragmented_profile::open_guest_bridge(&guest).await;
                                #[cfg(test)]
                                if let Some(gate)=gate { gate.entered.notify_one(); gate.release.notified().await; }
                                result
                            };
                            let ((opened,projection),interrupted)=await_udp_work(&guest,&cancel,deadline,||invitation.check_session().is_ok(),setup).await;
                            udp_bridge=opened;
                            if interrupted || !udp_recovery::complete(&state,&id,sequence,projection) { break; }
                        }
                        _=tick.tick()=>{
                            if invitation.check_session().is_err() { break; }
                            let failure=udp_bridge.as_ref().and_then(|udp| {
                                if udp.is_closed() {Some(UdpFailure::Closed)}
                                else if udp.max_payload()!=Some(usize::from(fragmented_profile::ORIGINAL_DATAGRAM_BYTES)) {Some(UdpFailure::CapacityUnavailable)}
                                else {None}
                            });
                            if let Some(code)=failure {
                                if let Some(udp)=udp_bridge.take(){let (_,interrupted)=await_udp_work(&guest,&cancel,deadline,||invitation.check_session().is_ok(),udp.shutdown()).await;drop(udp);if interrupted {break;}}
                                update(&state,&id,|a| {if a.snapshot.phase=="guestConnected" {a.snapshot.udp=Some(FragmentedGuestState::failed(code));}});
                            }
                        }
                    }
                }
            }
            cancel.cancel();
            update(&state, &id, |a| {
                a.snapshot.phase = "closing";
                udp_recovery::clear(a);
            });
            guest.shutdown().await;
            bridge.shutdown().await;
            if let Some(udp) = udp_bridge.take() {
                udp.shutdown().await;
                drop(udp);
            }
            drop(bridge);
            drop(guest);
            close_operation(&state, &id, &cancel, None).await;
        }
    }
}
