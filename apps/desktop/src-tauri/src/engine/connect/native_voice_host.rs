//! Private86 Host actor. The original84 query, transport and all81 withdrawal
//! owners are distinct and remain attached to the same Attempt until collected.
use super::*;
use crate::engine::connect_voice::dedicated::DedicatedError;
use std::sync::Weak;
use tokio::task::JoinSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UdpDecision {
    Enabled,
    Retiring,
    Retired,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    Voice(NativeError),
    NativeAuthorityClosed,
    GameAuthorityClosed,
    GameBindingChanged,
    GameObservationRejected,
    Integration,
}
pub(super) struct HostOperation {
    host: Weak<lab::Host>,
    published: bool,
    udp: UdpDecision,
    reason: Option<Reason>,
    retirement_pending: usize,
    #[cfg(test)]
    late_ready: Arc<std::sync::atomic::AtomicBool>,
}
#[derive(Clone, Debug)]
pub(crate) struct Bound {
    pub(crate) operation_id: String,
    pub(crate) participants: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostObservation {
    pub(crate) bound: bool,
    pub(crate) udp: UdpDecision,
    pub(crate) reason: Option<Reason>,
    pub(crate) retirement_pending: usize,
}
impl HostObservation {
    pub(crate) fn udp_retired(self) -> bool {
        self.udp == UdpDecision::Retired
    }
    pub(crate) fn udp_retiring(self) -> bool {
        self.udp == UdpDecision::Retiring
    }
}
impl ConnectManager {
    #[cfg(test)]
    pub(crate) fn native_voice_late_ready_for_test(&self, owner: &str, id: &str) -> Result<bool> {
        let mut state = lock(&self.live.state)?;
        let a = selected(&mut state, owner, id)?;
        Ok(a.native_voice
            .as_ref()
            .and_then(|operation| operation.host.as_ref())
            .is_some_and(|host| host.late_ready.load(std::sync::atomic::Ordering::Acquire)))
    }
    #[cfg(all(test, debug_assertions))]
    pub(crate) fn native_voice_service_disabled_for_test(
        &self,
        owner: &str,
        id: &str,
        participant_id: &str,
    ) -> Result<bool> {
        let mut state = lock(&self.live.state)?;
        let a = selected(&mut state, owner, id)?;
        let operation = a
            .native_voice
            .as_ref()
            .ok_or(ConnectError::StaleOperation)?;
        let review = operation
            .review
            .as_ref()
            .ok_or(ConnectError::StaleOperation)?;
        let index = review
            .review
            .data
            .participants
            .iter()
            .position(|p| p.participant_id == participant_id)
            .ok_or(ConnectError::StaleOperation)?;
        let host = operation
            .host
            .as_ref()
            .and_then(|control| control.host.upgrade())
            .ok_or(ConnectError::StaleOperation)?;
        host.native_voice_service_disabled_for_test(review.invitations[index].invitation_id())
            .map_err(|_| ConnectError::Internal.into())
    }
    #[cfg(all(test, debug_assertions))]
    pub(crate) fn native_voice_retirement_gate_for_test(
        &self,
        owner: &str,
        id: &str,
        participant_id: &str,
        stage: lab::NativeVoiceRetirementStage,
    ) -> Result<lab::NativeVoiceRetirementGate> {
        let mut state = lock(&self.live.state)?;
        let a = selected(&mut state, owner, id)?;
        let operation = a
            .native_voice
            .as_ref()
            .ok_or(ConnectError::StaleOperation)?;
        let review = operation
            .review
            .as_ref()
            .ok_or(ConnectError::StaleOperation)?;
        let index = review
            .review
            .data
            .participants
            .iter()
            .position(|p| p.participant_id == participant_id)
            .ok_or(ConnectError::StaleOperation)?;
        let host = operation
            .host
            .as_ref()
            .and_then(|control| control.host.upgrade())
            .ok_or(ConnectError::StaleOperation)?;
        // The return value is only a weakly registered gate. The temporary
        // original Host owner ends here, before any test awaits the checkpoint.
        host.native_voice_retirement_gate_for_test(review.invitations[index].invitation_id(), stage)
            .map_err(|_| ConnectError::Internal.into())
    }
    pub(crate) fn confirm_native_voice_group(
        &self,
        owner: &str,
        id: &str,
        generation: u64,
        digest: &str,
    ) -> Result<Pending<Bound>> {
        self.live
            .confirm_native_voice_group(owner, id, generation, digest)
    }
    pub(crate) fn native_voice_host_observation(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<HostObservation> {
        let mut state = lock(&self.live.state)?;
        let a = selected(&mut state, owner, id)?;
        super::expire_pending(a, lab::unix_millis());
        let host = a
            .native_voice
            .as_ref()
            .and_then(|operation| operation.host.as_ref())
            .ok_or(ConnectError::StaleOperation)?;
        Ok(HostObservation {
            bound: host.published && !a.cancel.is_cancelled(),
            udp: host.udp,
            reason: host.reason,
            retirement_pending: host.retirement_pending,
        })
    }
    pub(crate) fn export_native_voice_invitation(
        &self,
        owner: &str,
        id: &str,
        participant_id: &str,
    ) -> Result<String> {
        let mut state = lock(&self.live.state)?;
        if state.shutting_down {
            return Err(ConnectError::Cancelled.into());
        }
        let a = selected(&mut state, owner, id)?;
        super::expire_pending(a, lab::unix_millis());
        if a.cancel.is_cancelled() || a.snapshot.phase != "nativeBound" {
            return Err(ConnectError::StaleOperation.into());
        }
        let operation = a
            .native_voice
            .as_ref()
            .ok_or(ConnectError::StaleOperation)?;
        operation.binding.check_current()?;
        let host = operation
            .host
            .as_ref()
            .filter(|host| host.published)
            .and_then(|host| host.host.upgrade())
            .ok_or(ConnectError::StaleOperation)?;
        let review = operation
            .review
            .as_ref()
            .ok_or(ConnectError::StaleOperation)?;
        let index = review
            .review
            .data
            .participants
            .iter()
            .position(|p| p.participant_id == participant_id)
            .ok_or(ConnectError::StaleOperation)?;
        let invitation = &review.invitations[index];
        invitation
            .check_admission_current()
            .map_err(|_| ConnectError::PlanExpired)?;
        if host.invitation_consumed(invitation.invitation_id()) {
            return Err(ConnectError::StaleOperation.into());
        }
        // Original object and clocks, including after UDP was retired. No
        // alternate grant, revision or reconnect invitation is constructed.
        Ok(invitation.encode())
    }
}

impl Manager {
    fn confirm_native_voice_group(
        &self,
        owner: &str,
        id: &str,
        generation: u64,
        approval: &str,
    ) -> Result<Pending<Bound>> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| ConnectError::Internal)?;
        let mut state = lock(&self.state)?;
        check(&mut state, owner, id, generation, "nativeReviewed")?;
        let a = selected(&mut state, owner, id)?;
        if !group_source::finished(a) {
            return Err(ConnectError::Busy.into());
        }
        let operation = a
            .native_voice
            .as_mut()
            .ok_or(ConnectError::StaleOperation)?;
        if operation.host.is_some() {
            return Err(ConnectError::StaleOperation.into());
        }
        let retained = operation
            .review
            .as_ref()
            .ok_or(ConnectError::StaleOperation)?;
        let reviewed = &retained.review;
        if approval != reviewed.digest
            || review::digest(owner, &operation.binding, &reviewed.data)? != approval
        {
            return Err(ConnectError::WrongDigest.into());
        }
        if reviewed.data.operation_id != id
            || reviewed.data.generation != generation
            || Instant::now() >= retained.deadline
            || retained
                .invitations
                .iter()
                .any(|i| i.check_admission_current().is_err())
        {
            return Err(ConnectError::PlanExpired.into());
        }
        operation.binding.check_current()?;
        let invitations = retained.invitations.clone();
        let binding = operation.binding.clone();
        let key = operation.key.clone();
        let bind = operation.bind;
        let services = operation.services.clone();
        let review_id = reviewed.data.review_id.clone();
        let review_deadline = retained.deadline;
        #[cfg(test)]
        let late_ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
        operation.host = Some(HostOperation {
            host: Weak::new(),
            published: false,
            udp: UdpDecision::Enabled,
            reason: None,
            retirement_pending: 0,
            #[cfg(test)]
            late_ready: late_ready.clone(),
        });
        a.snapshot.phase = "nativeStarting";
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        a.snapshot.cleanup_state = "pending";
        let generation = a.snapshot.generation;
        let cancel = a.cancel.clone();
        let shared = self.state.clone();
        let worker_id = id.to_owned();
        let owner = owner.to_owned();
        let (send, receive) = tokio::sync::oneshot::channel();
        // Installed under the same mutex before the actor reserves or dispatches
        // any source job. This handle is never transferred to the waiter.
        a.worker = Some(runtime.spawn(async move {
            let mut actor = Actor {
                shared,
                owner,
                id: worker_id,
                review_id,
                generation,
                binding,
                invitations,
                review_deadline,
                cancel,
                host: None,
                retirements: JoinSet::new(),
                udp: UdpDecision::Enabled,
                retirement_failed: false,
                published: false,
                #[cfg(test)]
                late_ready,
            };
            actor.run(key, bind, services, send).await;
        }));
        Ok(Pending {
            operation_id: id.into(),
            result: receive,
        })
    }
}

pub(super) fn refresh(a: &mut Attempt) {
    let Some(operation) = a.native_voice.as_ref() else {
        return;
    };
    let Some(control) = operation.host.as_ref() else {
        return;
    };
    let Some(review) = operation.review.as_ref() else {
        return;
    };
    let authority_expired = if control.published {
        let host = control.host.upgrade();
        review.invitations.iter().all(|invitation| {
            invitation.check_session_current().is_err()
                || (invitation.check_admission_current().is_err()
                    && host
                        .as_ref()
                        .is_none_or(|host| !host.invitation_consumed(invitation.invitation_id())))
        })
    } else {
        Instant::now() >= review.deadline
            || Instant::now() >= a.prepared_deadline
            || review
                .invitations
                .iter()
                .any(|i| i.check_admission_current().is_err())
    };
    if operation.binding.check_current().is_err() || authority_expired {
        // The actor owns transport and cleanup. A snapshot can seal authority,
        // but cannot declare that those owners or a retained query are gone.
        a.cancel.cancel();
    }
}

struct Actor {
    shared: Arc<Mutex<State>>,
    owner: String,
    id: String,
    review_id: String,
    generation: u64,
    binding: NativeVoiceHostBinding,
    invitations: Vec<lab::SessionInvitation>,
    review_deadline: Instant,
    cancel: CancellationToken,
    host: Option<Arc<lab::Host>>,
    retirements: JoinSet<std::result::Result<(), lab::Error>>,
    udp: UdpDecision,
    retirement_failed: bool,
    published: bool,
    #[cfg(test)]
    late_ready: Arc<std::sync::atomic::AtomicBool>,
}

enum VoicePoll {
    Result(std::result::Result<Observation, tokio::task::JoinError>),
    Window,
    Tick,
}

#[cfg(test)]
async fn observe_ready_before_select<F: std::future::Future>(
    selection: F,
    task: tokio::task::AbortHandle,
    window: Instant,
    observed: &std::sync::atomic::AtomicBool,
) -> F::Output {
    let mut selection = std::pin::pin!(selection);
    std::future::poll_fn(|cx| {
        // Real task completion and the fixed original clock are inspected
        // before polling select on EVERY resume, not inferred from84 slot drop.
        if task.is_finished() && Instant::now() >= window {
            observed.store(true, std::sync::atomic::Ordering::Release);
        }
        selection.as_mut().poll(cx)
    })
    .await
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TcpDecision {
    Observed,
    Unavailable,
    AuthorityClosed,
    Changed,
    Rejected,
    Integration,
}
fn tcp_decision(result: std::result::Result<(), DedicatedError>) -> TcpDecision {
    use crate::engine::{
        connect_discovery::DiscoveryError as D, connect_owned_run::OwnedRunError as O,
    };
    match result {
        Ok(()) => TcpDecision::Observed,
        Err(
            DedicatedError::Busy
            | DedicatedError::NotReady
            | DedicatedError::Source(O::Busy | O::Discovery(D::Busy)),
        ) => TcpDecision::Unavailable,
        Err(
            DedicatedError::Closed
            | DedicatedError::Cancelled
            | DedicatedError::Source(O::Closed | O::Cancelled | O::Deadline),
        ) => TcpDecision::AuthorityClosed,
        Err(DedicatedError::Source(O::Discovery(D::ProcessChanged | D::ListenerChanged))) => {
            TcpDecision::Changed
        }
        Err(DedicatedError::Source(O::Discovery(
            D::Unavailable | D::Limit | D::Internal | D::StatusFailed,
        ))) => TcpDecision::Rejected,
        Err(
            DedicatedError::Preparation(_)
            | DedicatedError::Thread
            | DedicatedError::Source(
                O::InvalidRequest
                | O::Spawn
                | O::Initialization
                | O::Internal
                | O::Discovery(
                    D::InvalidRequest | D::Stale | D::Owner | D::Digest | D::RuntimeUnavailable,
                ),
            ),
        ) => TcpDecision::Integration,
        #[cfg(test)]
        Err(DedicatedError::Internal) => TcpDecision::Integration,
    }
}

impl Actor {
    fn current(&self) -> bool {
        if self.cancel.is_cancelled() || self.binding.check_current().is_err() {
            return false;
        }
        if !self.published
            && (Instant::now() >= self.review_deadline
                || self
                    .invitations
                    .iter()
                    .any(|i| i.check_admission_current().is_err()))
        {
            return false;
        }
        if self.published
            && self.invitations.iter().all(|i| {
                i.check_session_current().is_err()
                    || (i.check_admission_current().is_err()
                        && self
                            .host
                            .as_ref()
                            .is_none_or(|h| !h.invitation_consumed(i.invitation_id())))
            })
        {
            return false;
        }
        let Ok(state) = lock(&self.shared) else {
            return false;
        };
        !state.shutting_down
            && state.attempt.as_ref().is_some_and(|a| {
                a.owner == self.owner
                    && a.snapshot.operation_id.as_deref() == Some(&self.id)
                    && a.snapshot.generation == self.generation
                    && !a.cancel.is_cancelled()
                    && a.native_voice
                        .as_ref()
                        .and_then(|operation| operation.review.as_ref())
                        .is_some_and(|review| review.review.data.review_id == self.review_id)
            })
    }
    fn state(&mut self, reason: Option<Reason>) {
        let Ok(mut state) = lock(&self.shared) else {
            return;
        };
        let Ok(a) = selected(&mut state, &self.owner, &self.id) else {
            return;
        };
        if a.snapshot.generation != self.generation
            || a.cancel.is_cancelled()
            || !a
                .native_voice
                .as_ref()
                .and_then(|operation| operation.review.as_ref())
                .is_some_and(|review| review.review.data.review_id == self.review_id)
        {
            self.cancel.cancel();
            return;
        }
        let Some(control) = a
            .native_voice
            .as_mut()
            .and_then(|operation| operation.host.as_mut())
        else {
            return;
        };
        control.udp = self.udp;
        control.retirement_pending = self.retirements.len();
        if reason.is_some() {
            control.reason = reason;
        }
        // Native lifecycle changes have their own generation. Stop can advance
        // it independently only after cancellation, which always wins current().
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        self.generation = a.snapshot.generation;
    }
    fn retire(&mut self, reason: NativeError) {
        if self.udp != UdpDecision::Enabled {
            return;
        }
        self.udp = UdpDecision::Retiring;
        self.state(Some(Reason::Voice(reason)));
        let Some(host) = &self.host else {
            return;
        };
        // Spawn every original lease's real81 future before awaiting ANY drain.
        // There is no collective atomicity claim; each81 gate seals separately.
        for invitation in &self.invitations {
            let host = host.clone();
            let lease_id = invitation.invitation_id().to_owned();
            self.retirements
                .spawn(async move { host.disable_udp_service(&lease_id, "aux", 1).await });
        }
        self.state(None);
    }
    fn collect_ready_retirements(&mut self) {
        let mut changed = false;
        while let Some(result) = self.retirements.try_join_next() {
            changed = true;
            self.retirement_failed |= !matches!(result, Ok(Ok(())));
        }
        if changed {
            if self.retirements.is_empty() {
                self.udp = if self.retirement_failed {
                    UdpDecision::Failed
                } else {
                    UdpDecision::Retired
                };
            }
            self.state(self.retirement_failed.then_some(Reason::Integration));
        }
    }
    async fn close_transport(&mut self) {
        if let Ok(mut state) = lock(&self.shared) {
            if let Ok(a) = selected(&mut state, &self.owner, &self.id) {
                if let Some(control) = a
                    .native_voice
                    .as_mut()
                    .and_then(|operation| operation.host.as_mut())
                {
                    control.host = Weak::new();
                    control.published = false;
                }
                a.snapshot.transport_state = "closed";
            }
        }
        if let Some(host) = self.host.take() {
            for invitation in &self.invitations {
                host.revoke(invitation.invitation_id());
            }
            host.shutdown().await;
            // A retirement owner can retain the Host too. Recollect these exact
            // tasks before claiming transport owners gone, never before sealing.
            while let Some(result) = self.retirements.join_next().await {
                self.retirement_failed |= !matches!(result, Ok(Ok(())));
            }
            drop(host);
        }
    }
    async fn voice_query(&mut self) -> (Observation, Instant) {
        let window = Instant::now() + WINDOW;
        if !self.current() {
            return (Err(ConnectError::Cancelled.into()), window);
        }
        let query = match self.binding.reserve_voice() {
            Ok(query) => query,
            Err(error) => return (Err(error.into()), window),
        };
        let mut task = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                query.execute().map_err(NativeError::from)
            }))
            .unwrap_or(Err(ConnectError::Internal.into()))
        });
        let mut observation = tokio::time::interval(Duration::from_millis(25));
        observation.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let failure = loop {
            #[cfg(test)]
            let task_completion = task.abort_handle();
            let selection = async {
                tokio::select! {
                    result = &mut task => VoicePoll::Result(result),
                    _ = tokio::time::sleep_until(window.into()) => VoicePoll::Window,
                    _ = observation.tick() => VoicePoll::Tick,
                }
            };
            #[cfg(test)]
            let selected =
                observe_ready_before_select(selection, task_completion, window, &self.late_ready)
                    .await;
            #[cfg(not(test))]
            let selected = selection.await;
            match selected {
                VoicePoll::Result(result) => {
                    let result = if Instant::now() >= window {
                        Err(NativeError::ObservationWindowElapsed)
                    } else if !self.current() {
                        Err(ConnectError::Cancelled.into())
                    } else {
                        result.unwrap_or(Err(ConnectError::Internal.into()))
                    };
                    let result = result.and_then(|sample| {
                        if self.binding.accepts(&sample) {
                            Ok(sample)
                        } else {
                            Err(NativeError::VoiceBindingRejected)
                        }
                    });
                    return (result, window);
                }
                VoicePoll::Window => break NativeError::ObservationWindowElapsed,
                VoicePoll::Tick => {
                    self.collect_ready_retirements();
                    if !self.current() {
                        break ConnectError::Cancelled.into();
                    }
                }
            }
        };
        if self.published && self.current() {
            self.retire(failure);
        } else {
            self.close_transport().await;
        }
        // The window only decides authorization. It never aborts/drops the
        // actual job, and no TCP query can begin until this one has returned.
        loop {
            tokio::select! {
                _ = &mut task => break,
                _ = observation.tick() => {
                    self.collect_ready_retirements();
                    if !self.current() { self.close_transport().await; }
                }
            }
        }
        (Err(failure), window)
    }
    async fn tcp_query(&mut self) -> TcpDecision {
        if !self.current() {
            return TcpDecision::AuthorityClosed;
        }
        let query = match self.binding.reserve_tcp() {
            Ok(query) => query,
            Err(error) => return tcp_decision(Err(error)),
        };
        let mut task = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| query.execute()))
        });
        let mut observation = tokio::time::interval(Duration::from_millis(25));
        observation.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                result = &mut task => return match result { Ok(Ok(result)) => tcp_decision(result), _ => TcpDecision::Integration },
                _ = observation.tick() => {
                    self.collect_ready_retirements();
                    if !self.current() {
                        self.close_transport().await;
                        let _ = task.await;
                        return TcpDecision::AuthorityClosed;
                    }
                }
            }
        }
    }
    fn publish(&mut self, window: Instant) -> Result<Bound> {
        if !self.current() {
            return Err(ConnectError::Cancelled.into());
        }
        let mut state = lock(&self.shared)?;
        if state.shutting_down {
            return Err(ConnectError::Cancelled.into());
        }
        let a = selected(&mut state, &self.owner, &self.id)?;
        if a.cancel.is_cancelled()
            || a.snapshot.generation != self.generation
            || a.snapshot.phase != "nativeStarting"
        {
            return Err(ConnectError::StaleOperation.into());
        }
        self.binding.check_current()?;
        if Instant::now() >= window {
            return Err(NativeError::ObservationWindowElapsed);
        }
        if Instant::now() >= self.review_deadline
            || self
                .invitations
                .iter()
                .any(|i| i.check_admission_current().is_err())
        {
            return Err(ConnectError::PlanExpired.into());
        }
        let control = a
            .native_voice
            .as_mut()
            .and_then(|operation| operation.host.as_mut())
            .ok_or(ConnectError::StaleOperation)?;
        control.host = Arc::downgrade(self.host.as_ref().ok_or(ConnectError::Internal)?);
        control.published = true;
        a.snapshot.phase = "nativeBound";
        a.snapshot.transport_state = "listening";
        a.snapshot.expires_at_ms = None;
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        self.generation = a.snapshot.generation;
        self.published = true;
        Ok(Bound {
            operation_id: self.id.clone(),
            participants: self.invitations.len(),
        })
    }
    async fn run(
        &mut self,
        key: lab::SecretKey,
        bind: SocketAddr,
        services: Vec<lab::Service>,
        send: tokio::sync::oneshot::Sender<Result<Bound>>,
    ) {
        let (before, _) = self.voice_query().await;
        if let Err(error) = before {
            let _ = send.send(Err(error));
            self.close_transport().await;
            finish(&self.shared, &self.id, Some(ConnectError::Internal));
            return;
        }
        if !self.current() {
            let _ = send.send(Err(ConnectError::Cancelled.into()));
            finish(&self.shared, &self.id, None);
            return;
        }
        let config = lab::HostConfig {
            secret_key: key,
            bind_addr: bind,
            session_id: self.id.clone(),
            services,
            leases: self
                .invitations
                .iter()
                .map(lab::SessionInvitation::lease)
                .collect(),
            limits: lab::Limits::default(),
        };
        match lab::Host::start_invited_fragmented_session_group(config, &self.invitations).await {
            Ok(host) => self.host = Some(Arc::new(host)),
            Err(_) => {
                let _ = send.send(Err(ConnectError::Internal.into()));
                finish(&self.shared, &self.id, Some(ConnectError::Internal));
                return;
            }
        }
        let (after, window) = self.voice_query().await;
        let published = after.and_then(|_| self.publish(window));
        let failed = published.is_err();
        let _ = send.send(published);
        if failed {
            self.close_transport().await;
            finish(&self.shared, &self.id, Some(ConnectError::Internal));
            return;
        }
        let mut next = Instant::now() + Duration::from_secs(1);
        let mut housekeeping = tokio::time::interval(Duration::from_millis(25));
        housekeeping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut reason = None;
        while self.current() {
            tokio::select! {
                _ = self.cancel.cancelled() => break,
                _ = housekeeping.tick() => self.collect_ready_retirements(),
                _ = tokio::time::sleep_until(next.into()) => {
                    if self.udp == UdpDecision::Enabled {
                        if let Err(error) = self.voice_query().await.0 {
                            if self.current() { self.retire(error); }
                        }
                    } else {
                        reason = match self.tcp_query().await {
                            TcpDecision::Observed | TcpDecision::Unavailable => None,
                            TcpDecision::AuthorityClosed => Some(Reason::GameAuthorityClosed),
                            TcpDecision::Changed => Some(Reason::GameBindingChanged),
                            TcpDecision::Rejected => Some(Reason::GameObservationRejected),
                            TcpDecision::Integration => Some(Reason::Integration),
                        };
                        if reason.is_some() { break; }
                    }
                    // No catch-up ticks, immediate retry or deadline renewal.
                    next = Instant::now() + Duration::from_secs(1);
                }
            }
        }
        self.state(reason.or(Some(Reason::NativeAuthorityClosed)));
        self.close_transport().await;
        finish(
            &self.shared,
            &self.id,
            reason.map(|_| ConnectError::Internal),
        );
    }
}
