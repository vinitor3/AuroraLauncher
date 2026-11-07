//! Private86 preparation/review and native Host actor. No IPC entry.
//! Every queued observation owns the original reserved84 job and is retained by
//! the existing Attempt cleanup barrier, independently of the result waiter.
#![cfg_attr(not(test), allow(dead_code))]
use super::*;
use crate::engine::connect_voice::dedicated::{
    NativeVoiceHostBinding, ReservedVoiceQuery, VoiceRuntimeError, VoiceRuntimeSample,
};
use std::collections::HashSet;

#[path = "native_voice_host.rs"]
mod host;
#[path = "native_voice_review.rs"]
mod review;
pub(crate) use review::NativeReview;

const WINDOW: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigurationRejection {
    Missing,
    Unsafe,
    Limit,
    Incompatible,
    Io,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeError {
    Connect(ConnectError),
    ObservationUnavailable,
    VoiceSourceClosed,
    VoiceConfigurationRejected(ConfigurationRejection),
    VoiceConfigurationChanged,
    VoiceContextRejected,
    VoiceBindingRejected,
    VoiceSourceUnavailable,
    VoiceSourceRejected,
    VoiceObserverRejected,
    ObservationWindowElapsed,
}
impl From<ConnectError> for NativeError {
    fn from(error: ConnectError) -> Self {
        Self::Connect(error)
    }
}
impl NativeError {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Connect(error) => error.code(),
            Self::ObservationUnavailable => "native_voice_observation_unavailable",
            Self::VoiceSourceClosed => "native_voice_source_closed",
            Self::VoiceConfigurationRejected(_) => "native_voice_configuration_rejected",
            Self::VoiceConfigurationChanged => "native_voice_configuration_changed",
            Self::VoiceContextRejected => "native_voice_context_rejected",
            Self::VoiceBindingRejected => "native_voice_binding_rejected",
            Self::VoiceSourceUnavailable => "native_voice_source_unavailable",
            Self::VoiceSourceRejected => "native_voice_source_rejected",
            Self::VoiceObserverRejected => "native_voice_observer_rejected",
            Self::ObservationWindowElapsed => "native_voice_observation_window_elapsed",
        }
    }
}
impl From<VoiceRuntimeError> for NativeError {
    fn from(error: VoiceRuntimeError) -> Self {
        use crate::engine::{
            connect_discovery::DiscoveryError as D, connect_owned_run::OwnedRunError as O,
        };
        use ConfigurationRejection as C;
        match error {
            VoiceRuntimeError::Busy | VoiceRuntimeError::NotReady => Self::ObservationUnavailable,
            VoiceRuntimeError::Closed => Self::VoiceSourceClosed,
            VoiceRuntimeError::ConfigurationMissing => Self::VoiceConfigurationRejected(C::Missing),
            VoiceRuntimeError::ConfigurationUnsafe => Self::VoiceConfigurationRejected(C::Unsafe),
            VoiceRuntimeError::ConfigurationLimit => Self::VoiceConfigurationRejected(C::Limit),
            VoiceRuntimeError::ConfigurationIncompatible => {
                Self::VoiceConfigurationRejected(C::Incompatible)
            }
            VoiceRuntimeError::ConfigurationIo => Self::VoiceConfigurationRejected(C::Io),
            VoiceRuntimeError::ConfigurationChanged => Self::VoiceConfigurationChanged,
            VoiceRuntimeError::Context(_) => Self::VoiceContextRejected,
            VoiceRuntimeError::UnsupportedBinding => Self::VoiceBindingRejected,
            VoiceRuntimeError::Source(O::Busy | O::Discovery(D::Busy)) => {
                Self::ObservationUnavailable
            }
            VoiceRuntimeError::Source(O::Closed | O::Cancelled | O::Deadline) => {
                Self::VoiceSourceUnavailable
            }
            VoiceRuntimeError::Source(O::Discovery(D::ProcessChanged | D::ListenerChanged)) => {
                Self::VoiceSourceRejected
            }
            VoiceRuntimeError::Source(O::Discovery(D::Unavailable | D::Limit | D::Internal)) => {
                Self::ObservationUnavailable
            }
            VoiceRuntimeError::Source(
                O::InvalidRequest
                | O::Spawn
                | O::Initialization
                | O::Internal
                | O::Discovery(
                    D::InvalidRequest
                    | D::Stale
                    | D::Owner
                    | D::Digest
                    | D::StatusFailed
                    | D::RuntimeUnavailable,
                ),
            ) => Self::VoiceObserverRejected,
        }
    }
}
type Result<T> = std::result::Result<T, NativeError>;

pub(crate) struct Pending<T> {
    pub(crate) operation_id: String,
    result: tokio::sync::oneshot::Receiver<Result<T>>,
}
impl<T> Pending<T> {
    pub(crate) async fn wait(self) -> Result<T> {
        self.result
            .await
            .unwrap_or(Err(ConnectError::Internal.into()))
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Preparation {
    pub(crate) operation_id: String,
    pub(crate) generation: u64,
    pub(crate) host_public_id: String,
    pub(crate) expires_at_ms: u64,
}
pub(super) struct Operation {
    binding: NativeVoiceHostBinding,
    key: lab::SecretKey,
    bind: SocketAddr,
    roster: Vec<GroupParticipantRequest>,
    services: Vec<lab::Service>,
    review: Option<review::RetainedReview>,
    host: Option<host::HostOperation>,
    #[cfg(test)]
    dispatched: bool,
}

impl ConnectManager {
    #[cfg(test)]
    pub(crate) fn native_voice_originals_for_test(
        &self,
    ) -> Option<(NativeReview, Vec<lab::SessionInvitation>)> {
        let state = self.live.state.lock().unwrap();
        let review = state
            .attempt
            .as_ref()?
            .native_voice
            .as_ref()?
            .review
            .as_ref()?;
        Some((review.review.clone(), review.invitations.clone()))
    }
    #[cfg(test)]
    pub(crate) fn native_voice_work_finished_for_test(&self) -> bool {
        let state = self.live.state.lock().unwrap();
        state.attempt.as_ref().is_none_or(group_source::finished)
    }
    #[cfg(test)]
    pub(crate) fn native_voice_query_dispatched_for_test(&self) -> bool {
        let state = self.live.state.lock().unwrap();
        state
            .attempt
            .as_ref()
            .and_then(|a| a.native_voice.as_ref())
            .is_some_and(|operation| operation.dispatched)
    }
    /// Native-only: roster is fixed before any observation or invitation.
    pub(crate) fn prepare_native_voice_group(
        &self,
        owner: &str,
        binding: NativeVoiceHostBinding,
        bind: SocketAddr,
        roster: Vec<GroupParticipantRequest>,
    ) -> Result<Pending<Preparation>> {
        self.live
            .prepare_native_voice_group(owner, binding, bind, roster)
    }
    pub(crate) fn review_native_voice_group(
        &self,
        owner: &str,
        id: &str,
        generation: u64,
    ) -> Result<Pending<NativeReview>> {
        self.live.review_native_voice_group(owner, id, generation)
    }
}

impl Manager {
    fn prepare_native_voice_group(
        &self,
        owner: &str,
        binding: NativeVoiceHostBinding,
        bind: SocketAddr,
        roster: Vec<GroupParticipantRequest>,
    ) -> Result<Pending<Preparation>> {
        if owner.is_empty()
            || owner.len() > 128
            || bind.port() == 0
            || bind.ip() != std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
            || !(2..=8.min(lab::Limits::default().max_peers)).contains(&roster.len())
        {
            return Err(ConnectError::InvalidRequest.into());
        }
        let mut recipients = HashSet::new();
        for request in &roster {
            if !recipients.insert(peer(&request.guest_public_id)?)
                || request.admission_seconds == 0
                || request.admission_seconds > 600
                || request.session_seconds < request.admission_seconds
                || request.session_seconds > 3600
            {
                return Err(ConnectError::InvalidRequest.into());
            }
        }
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| ConnectError::Internal)?;
        binding.check_current()?;
        let (game, udp) = binding.targets();
        let services = fragmented_profile::services(game, udp)?;
        let key = lab::generate_key();
        if recipients.contains(&key.public()) {
            return Err(ConnectError::InvalidRequest.into());
        }
        let now = Instant::now();
        let deadline = (now + Duration::from_secs(600)).min(binding.deadline());
        if now >= deadline {
            return Err(ConnectError::PlanExpired.into());
        }
        let expires =
            lab::unix_millis().saturating_add(deadline.duration_since(now).as_millis() as u64);
        let mut state = lock(&self.state)?;
        if state.shutting_down {
            return Err(ConnectError::Cancelled.into());
        }
        if let Some(a) = state.attempt.as_mut() {
            super::expire_pending(a);
            ready_game::refresh_cleanup(a);
            if !terminal(a) {
                return Err(ConnectError::Busy.into());
            }
        }
        state.generation = state
            .generation
            .max(state.attempt.as_ref().map_or(0, |a| a.snapshot.generation))
            .saturating_add(1);
        let id = uuid::Uuid::new_v4().to_string();
        let mut snapshot = ConnectSnapshot::idle();
        snapshot.operation_id = Some(id.clone());
        snapshot.generation = state.generation;
        snapshot.role = Some("host");
        snapshot.phase = "nativePreparing";
        snapshot.cleanup_state = "pending";
        snapshot.expires_at_ms = Some(expires);
        let generation = snapshot.generation;
        state.attempt = Some(Attempt {
            native_voice: Some(Operation {
                binding: binding.clone(),
                key,
                bind,
                roster,
                services,
                review: None,
                host: None,
                #[cfg(test)]
                dispatched: false,
            }),
            social: None,
            owner: owner.into(),
            snapshot,
            draft: None,
            reviewed: None,
            prepared_deadline: deadline,
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
            group_source: Some(group_source::SourceWork::default()),
            host_source: None,
            host_review: None,
            guest_source: None,
            ready_game: None,
            #[cfg(test)]
            recovery_gate: None,
            #[cfg(test)]
            group_start_gate: None,
        });
        // The Attempt work record precedes the SAME real84 reservation. Both
        // precede dispatch. Any failure drops that reservation, not a counter.
        let query = match binding.reserve_voice() {
            Ok(query) => query,
            Err(error) => {
                fail(state.attempt.as_mut().unwrap(), error.into());
                return Err(error.into());
            }
        };
        let (send, receive) = tokio::sync::oneshot::channel();
        let shared = self.state.clone();
        let worker_id = id.clone();
        let owner = owner.to_owned();
        let window = Instant::now() + WINDOW;
        let worker = runtime.spawn(async move {
            let (result, tail) = observe(
                &shared,
                &owner,
                &worker_id,
                generation,
                "nativePreparing",
                query,
                window,
            )
            .await;
            let result =
                publish_preparation(&shared, &owner, &worker_id, generation, window, result);
            let _ = send.send(result);
            if let Some(tail) = tail {
                let _ = tail.await;
            }
        });
        group_source::install_worker(state.attempt.as_mut().unwrap(), worker);
        Ok(Pending {
            operation_id: id,
            result: receive,
        })
    }

    fn review_native_voice_group(
        &self,
        owner: &str,
        id: &str,
        generation: u64,
    ) -> Result<Pending<NativeReview>> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| ConnectError::Internal)?;
        let mut state = lock(&self.state)?;
        check(&mut state, owner, id, generation, "nativePrepared")?;
        let a = selected(&mut state, owner, id)?;
        if !group_source::finished(a) {
            return Err(ConnectError::Busy.into());
        }
        let binding = a
            .native_voice
            .as_ref()
            .ok_or(ConnectError::StaleOperation)?
            .binding
            .clone();
        // SourceWork remains registered while replacing only a finished handle.
        let query = match binding.reserve_voice() {
            Ok(query) => query,
            Err(error) => {
                fail(a, error.into());
                return Err(error.into());
            }
        };
        a.snapshot.phase = "nativeReviewing";
        #[cfg(test)]
        {
            a.native_voice.as_mut().unwrap().dispatched = false;
        }
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        let generation = a.snapshot.generation;
        let window = Instant::now() + WINDOW;
        let shared = self.state.clone();
        let worker_id = id.to_owned();
        let owner = owner.to_owned();
        let (send, receive) = tokio::sync::oneshot::channel();
        let worker = runtime.spawn(async move {
            let (result, tail) = observe(
                &shared,
                &owner,
                &worker_id,
                generation,
                "nativeReviewing",
                query,
                window,
            )
            .await;
            let result = review::publish(&shared, &owner, &worker_id, generation, window, result);
            let _ = send.send(result);
            if let Some(tail) = tail {
                let _ = tail.await;
            }
        });
        group_source::install_worker(a, worker);
        Ok(Pending {
            operation_id: id.into(),
            result: receive,
        })
    }
}

type Observation = Result<VoiceRuntimeSample>;
type QueryTail = JoinHandle<Observation>;
async fn observe(
    shared: &Arc<Mutex<State>>,
    owner: &str,
    id: &str,
    generation: u64,
    phase: &'static str,
    query: ReservedVoiceQuery,
    window: Instant,
) -> (Observation, Option<QueryTail>) {
    let permitted = lock(shared)
        .map_err(NativeError::from)
        .and_then(|mut state| check(&mut state, owner, id, generation, phase));
    if let Err(error) = permitted {
        drop(query);
        return (Err(error), None);
    }
    let mut task = tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            query.execute().map_err(NativeError::from)
        }))
        .unwrap_or(Err(ConnectError::Internal.into()))
    });
    #[cfg(test)]
    if let Ok(mut state) = lock(shared) {
        if let Ok(a) = selected(&mut state, owner, id) {
            if let Some(operation) = a.native_voice.as_mut() {
                operation.dispatched = true;
            }
        }
    }
    let mut interval = tokio::time::interval(Duration::from_millis(25));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            result = &mut task => {
                // Deadline wins even when result and timer became ready together.
                let result = if Instant::now() >= window { Err(NativeError::ObservationWindowElapsed) }
                    else { result.unwrap_or(Err(ConnectError::Internal.into())) };
                return (result, None);
            }
            _ = tokio::time::sleep_until(window.into()) => return (Err(NativeError::ObservationWindowElapsed), Some(task)),
            _ = interval.tick() => {
                let current = lock(shared).map_err(NativeError::from)
                    .and_then(|mut state| check(&mut state, owner, id, generation, phase));
                if let Err(error) = current { return (Err(error), Some(task)); }
            }
        }
    }
}

fn check(state: &mut State, owner: &str, id: &str, generation: u64, phase: &str) -> Result<()> {
    if state.shutting_down {
        return Err(ConnectError::Cancelled.into());
    }
    let a = selected(state, owner, id)?;
    if a.cancel.is_cancelled() {
        return Err(ConnectError::Cancelled.into());
    }
    if a.snapshot.generation != generation {
        return Err(ConnectError::StaleOperation.into());
    }
    if a.snapshot.phase != phase {
        return Err(ConnectError::PlanExpired.into());
    }
    if Instant::now() >= a.prepared_deadline
        || a.snapshot
            .expires_at_ms
            .is_none_or(|v| lab::unix_millis() >= v)
    {
        return Err(ConnectError::PlanExpired.into());
    }
    a.native_voice
        .as_ref()
        .ok_or(ConnectError::StaleOperation)?
        .binding
        .check_current()?;
    Ok(())
}
fn final_observation(a: &Attempt, window: Instant, result: Observation) -> Observation {
    if Instant::now() >= window {
        return Err(NativeError::ObservationWindowElapsed);
    }
    let sample = result?;
    let binding = &a
        .native_voice
        .as_ref()
        .ok_or(ConnectError::StaleOperation)?
        .binding;
    binding.check_current()?;
    if !binding.accepts(&sample) {
        return Err(NativeError::VoiceBindingRejected);
    }
    Ok(sample)
}
fn publish_preparation(
    shared: &Mutex<State>,
    owner: &str,
    id: &str,
    generation: u64,
    window: Instant,
    result: Observation,
) -> Result<Preparation> {
    let mut state = lock(shared)?;
    let checked = check(&mut state, owner, id, generation, "nativePreparing");
    let a = selected(&mut state, owner, id)?;
    let result = checked.and_then(|()| final_observation(a, window, result));
    match result {
        Ok(_) => {
            a.snapshot.phase = "nativePrepared";
            a.snapshot.generation = a.snapshot.generation.saturating_add(1);
            Ok(Preparation {
                operation_id: id.into(),
                generation: a.snapshot.generation,
                host_public_id: a.native_voice.as_ref().unwrap().key.public().to_string(),
                expires_at_ms: a.snapshot.expires_at_ms.unwrap(),
            })
        }
        Err(error) => {
            if a.snapshot.phase == "nativePreparing" {
                fail(a, error);
            }
            Err(error)
        }
    }
}
fn fail(a: &mut Attempt, error: NativeError) {
    a.cancel.cancel();
    closing(a);
    a.snapshot.phase = if error == ConnectError::Cancelled.into() {
        "closed"
    } else {
        "failed"
    };
    a.snapshot.transport_state = "closed";
    a.snapshot.cleanup_state = "pending";
    a.snapshot.error_code = match error {
        NativeError::Connect(error) => Some(error.code()),
        _ => Some(ConnectError::Internal.code()),
    };
    a.snapshot.expires_at_ms = None;
    a.snapshot.generation = a.snapshot.generation.saturating_add(1);
}
pub(super) fn closing(a: &mut Attempt) {
    a.native_voice = None;
}
pub(super) fn expire_pending(a: &mut Attempt, now: u64) {
    let Some(operation) = &a.native_voice else {
        return;
    };
    if operation.host.is_some() {
        host::refresh(a);
        return;
    }
    let deadline = operation
        .review
        .as_ref()
        .map_or(a.prepared_deadline, |r| r.deadline.min(a.prepared_deadline));
    let invalid = Instant::now() >= deadline
        || a.snapshot.expires_at_ms.is_none_or(|v| now >= v)
        || operation.binding.check_current().is_err()
        || operation.review.as_ref().is_some_and(|r| {
            r.invitations
                .iter()
                .any(|i| i.check_admission_current().is_err())
        });
    if invalid {
        fail(a, ConnectError::PlanExpired.into());
    }
}
