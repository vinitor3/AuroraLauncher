//! Native ownership of one trusted Child. This is lifecycle infrastructure,
//! not a Java launch plan, copy/provenance check or voice readiness claim.
use super::connect_discovery::{
    require_owned_tcp_listener, require_owned_udp_binding, DiscoveryError, OwnedChildProof,
};
#[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
use super::connect_discovery::{require_owned_udp_loopback_binding, UdpLoopbackRepresentation};
#[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
use std::net::SocketAddrV4;
use std::{
    net::SocketAddr,
    process::Command,
    sync::{Arc, Condvar, Mutex, MutexGuard, Weak},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

#[cfg(all(test, feature = "connect-voice-lab"))]
mod dedicated_test;
#[cfg(test)]
mod test_support;
#[cfg(all(test, feature = "connect-voice-lab"))]
pub(crate) use dedicated_test::{
    start_owned_run_dedicated_test, start_owned_voice_bridge_test, DedicatedTestHooks,
    OwnerTestGate,
};
#[cfg(feature = "connect-voice-lab")]
mod spawn_refusal;
#[cfg(test)]
mod tests;
mod worker;
#[cfg(feature = "connect-voice-lab")]
pub(crate) use spawn_refusal::{OwnedStartFailure, SpawnRefusal};

#[cfg(feature = "connect-voice-lab")]
pub(crate) fn start_owned_voice_bridge_receipted(
    command: Command,
    instance_id: &str,
    attempt: uuid::Uuid,
    limits: OwnedRunLimits,
    cancel: CancellationToken,
    #[cfg(test)] hooks: DedicatedTestHooks,
) -> Result<OwnedRun, OwnedStartFailure> {
    #[cfg(test)]
    let hooks = hooks.0;
    #[cfg(not(test))]
    let hooks = Hooks;
    worker::start_receipted(command, instance_id, attempt, limits, cancel, hooks)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct OwnedRunLimits {
    pub(crate) lifetime: Duration,
    pub(crate) stop_grace: Duration,
    pub(crate) output_bytes: u64,
}
impl OwnedRunLimits {
    fn valid(self) -> bool {
        self.valid_with_lifetime(Duration::from_secs(3600))
    }
    #[cfg(feature = "connect-voice-lab")]
    fn valid_voice_bridge(self) -> bool {
        self.valid_with_lifetime(Duration::from_secs(14_400))
    }
    #[cfg(feature = "connect-voice-lab")]
    fn valid_native_server(self) -> bool {
        self.valid_with_caps(Duration::from_secs(14_400), Duration::from_secs(30))
    }
    fn valid_with_lifetime(self, cap: Duration) -> bool {
        self.valid_with_caps(cap, Duration::from_secs(5))
    }
    fn valid_with_caps(self, cap: Duration, stop_cap: Duration) -> bool {
        !self.lifetime.is_zero()
            && self.lifetime <= cap
            && !self.stop_grace.is_zero()
            && self.stop_grace <= stop_cap
            && self.stop_grace <= self.lifetime
            && (1..=16 * 1024 * 1024).contains(&self.output_bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnedRunReason {
    Requested,
    Cancelled,
    Dropped,
    Deadline,
    Exited,
    Failure,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnedRunFailure {
    Cancelled,
    Dropped,
    Deadline,
    EarlyExit,
    NonzeroExit,
    OutputLimit,
    Read,
    Write,
    Proof,
    Thread,
    Initialization,
    Kill,
    Wait,
    Panic,
    Internal,
    Forced,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnedRunError {
    InvalidRequest,
    Cancelled,
    Deadline,
    Closed,
    Busy,
    Spawn,
    Initialization,
    Internal,
    Discovery(DiscoveryError),
}

/// Closed, redacted summary. No serde or raw output/process identifiers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnedRunReport {
    pub(crate) reason: OwnedRunReason,
    pub(crate) failure: Option<OwnedRunFailure>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) forced: bool,
    pub(crate) stdout_bytes: u64,
    pub(crate) stderr_bytes: u64,
    pub(crate) reaped: bool,
    pub(crate) readers_complete: bool,
}

pub(crate) struct OwnedRun {
    shared: Arc<Shared>,
    completion: OwnedRunCompletion,
}
#[derive(Clone)]
pub(crate) struct OwnedRunObserver {
    shared: Weak<Shared>,
}
/// This channel carries only the report. It never retains Shared, a proof,
/// process, JoinHandle or closure which could extend resource ownership.
#[derive(Clone)]
pub(crate) struct OwnedRunCompletion {
    receiver: watch::Receiver<Option<OwnedRunReport>>,
}

// Reserved internal entry points for the future trusted producer. No fake
// caller, public export, PID adoption or manual-UDP shortcut is introduced.
#[allow(dead_code)]
pub(crate) fn start_owned_run(
    command: Command,
    instance_id: &str,
    limits: OwnedRunLimits,
    cancel: CancellationToken,
) -> Result<OwnedRun, OwnedRunError> {
    #[cfg(test)]
    let hooks = Hooks::default();
    #[cfg(not(test))]
    let hooks = Hooks;
    worker::start(command, instance_id, limits, cancel, hooks)
}
/// Separate trusted producer entry for an explicitly reviewed ManifestV2 policy2
/// recipe. Generic83 and previous producer recipes keep their existing caps.
#[cfg(all(feature = "connect-voice-lab", not(test)))]
pub(crate) fn start_owned_voice_bridge(
    command: Command,
    instance_id: &str,
    limits: OwnedRunLimits,
    cancel: CancellationToken,
) -> Result<OwnedRun, OwnedRunError> {
    #[cfg(test)]
    let hooks = Hooks::default();
    #[cfg(not(test))]
    let hooks = Hooks;
    worker::start_voice_bridge(command, instance_id, limits, cancel, hooks)
}
#[allow(dead_code)] // The future producer owns this non-clonable control.
impl OwnedRun {
    pub(crate) fn observer(&self) -> OwnedRunObserver {
        OwnedRunObserver {
            shared: Arc::downgrade(&self.shared),
        }
    }
    pub(crate) fn completion(&self) -> OwnedRunCompletion {
        self.completion.clone()
    }
    /// Seals now, even if the returned completion is never polled.
    pub(crate) fn request_stop(&self) -> OwnedRunCompletion {
        self.shared.request(OwnedRunReason::Requested, None);
        self.completion()
    }
}
impl Drop for OwnedRun {
    fn drop(&mut self) {
        self.shared
            .request(OwnedRunReason::Dropped, Some(OwnedRunFailure::Dropped));
    }
}
#[allow(dead_code)] // Internal read-only entry point; no caller in this increment.
impl OwnedRunObserver {
    pub(crate) fn observe_udp(&self, endpoint: SocketAddr) -> Result<(), OwnedRunError> {
        match self.observe(QueryRequest::Udp(endpoint))? {
            QueryResult::Udp => Ok(()),
            _ => Err(OwnedRunError::Internal),
        }
    }

    pub(crate) fn observe_tcp(&self, endpoint: SocketAddr) -> Result<(), OwnedRunError> {
        match self.observe(QueryRequest::Tcp(endpoint))? {
            QueryResult::Tcp => Ok(()),
            _ => Err(OwnedRunError::Internal),
        }
    }

    #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
    pub(crate) fn observe_udp_loopback_binding(
        &self,
        endpoint: SocketAddrV4,
    ) -> Result<UdpLoopbackRepresentation, OwnedRunError> {
        match self.observe(QueryRequest::UdpLoopbackBinding(endpoint))? {
            QueryResult::UdpLoopbackBinding(representation) => Ok(representation),
            _ => Err(OwnedRunError::Internal),
        }
    }

    fn observe(&self, request: QueryRequest) -> Result<QueryResult, OwnedRunError> {
        let shared = self.shared.upgrade().ok_or(OwnedRunError::Closed)?;
        let job = {
            let mut state = shared.lock();
            shared.refresh(&mut state);
            if state.reason.is_some() {
                return Err(OwnedRunError::Closed);
            }
            if state.query_active {
                return Err(OwnedRunError::Busy);
            }
            let proof = state.proof.clone().ok_or(OwnedRunError::Closed)?;
            state.query_active = true;
            QueryJob {
                proof: Some(proof),
                shared: shared.clone(),
            }
        };
        #[cfg(test)]
        shared.hooks.before_query();
        let proof = job.proof.as_ref().expect("registered proof");
        let result = match request {
            QueryRequest::Tcp(endpoint) => match require_owned_tcp_listener(proof, endpoint) {
                Ok(()) => Ok(QueryResult::Tcp),
                Err(error) => Err(error),
            },
            QueryRequest::Udp(endpoint) => match require_owned_udp_binding(proof, endpoint) {
                Ok(()) => Ok(QueryResult::Udp),
                Err(error) => Err(error),
            },
            #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
            QueryRequest::UdpLoopbackBinding(endpoint) => {
                match require_owned_udp_loopback_binding(proof, endpoint) {
                    Ok(representation) => Ok(QueryResult::UdpLoopbackBinding(representation)),
                    Err(error) => Err(error),
                }
            }
        }
        .map_err(OwnedRunError::Discovery);
        #[cfg(test)]
        shared.hooks.after_query(result.is_ok());
        // The decision, not the later machine instruction returning to the
        // caller, linearizes against stop. The sample grants no authority.
        let mut state = shared.lock();
        shared.refresh(&mut state);
        if state.reason.is_some() {
            return Err(OwnedRunError::Closed);
        }
        result
    }
}

// Explicit, closed dispatches share the same reservation, original proof,
// QueryJob and final stop gate. There is no callback or per-protocol slot.
enum QueryRequest {
    Tcp(SocketAddr),
    Udp(SocketAddr),
    #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
    UdpLoopbackBinding(SocketAddrV4),
}
enum QueryResult {
    Tcp,
    Udp,
    #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
    UdpLoopbackBinding(UdpLoopbackRepresentation),
}
#[allow(dead_code)] // Completion is reserved for the future trusted producer.
impl OwnedRunCompletion {
    pub(crate) async fn wait(&self) -> OwnedRunReport {
        let mut receiver = self.receiver.clone();
        loop {
            if let Some(report) = receiver.borrow_and_update().clone() {
                return report;
            }
            // Sender is held by the resource owner until it publishes. An
            // unexpected close must never manufacture a completed report.
            if receiver.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    }
}

struct State {
    reason: Option<OwnedRunReason>,
    failure: Option<OwnedRunFailure>,
    proof: Option<Arc<OwnedChildProof>>,
    query_active: bool,
    stdout_bytes: u64,
    stderr_bytes: u64,
    completed: bool,
}
impl State {
    fn seal(&mut self, reason: OwnedRunReason, failure: Option<OwnedRunFailure>) {
        self.reason.get_or_insert(reason);
        if self.failure.is_none() {
            self.failure = failure;
        }
    }
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    deadline: Instant,
    limits: OwnedRunLimits,
    cancel: CancellationToken,
    #[cfg(test)]
    hooks: Hooks,
}
impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                let mut state = poison.into_inner();
                state.seal(OwnedRunReason::Failure, Some(OwnedRunFailure::Internal));
                self.changed.notify_all();
                state
            }
        }
    }
    fn refresh(&self, state: &mut State) {
        if state.completed {
            return;
        }
        if self.cancel.is_cancelled() {
            state.seal(OwnedRunReason::Cancelled, Some(OwnedRunFailure::Cancelled));
        }
        if Instant::now() >= self.deadline {
            state.seal(OwnedRunReason::Deadline, Some(OwnedRunFailure::Deadline));
        }
        if state.reason.is_some() {
            self.changed.notify_all();
        }
    }
    fn request(&self, reason: OwnedRunReason, failure: Option<OwnedRunFailure>) {
        let mut state = self.lock();
        self.refresh(&mut state);
        // Repeated stop/Drop never changes an already chosen reason/failure.
        if state.reason.is_none() {
            state.seal(reason, failure);
        }
        self.changed.notify_all();
    }
    fn fail(&self, failure: OwnedRunFailure) {
        self.lock().seal(OwnedRunReason::Failure, Some(failure));
        self.changed.notify_all();
    }
    fn pause(&self, duration: Duration) {
        let state = self.lock();
        if let Err(poison) = self.changed.wait_timeout(state, duration) {
            let (mut state, _) = poison.into_inner();
            state.seal(OwnedRunReason::Failure, Some(OwnedRunFailure::Internal));
            self.changed.notify_all();
        }
    }
}
struct QueryJob {
    proof: Option<Arc<OwnedChildProof>>,
    shared: Arc<Shared>,
}
impl Drop for QueryJob {
    fn drop(&mut self) {
        // Actual handle ownership goes away before the observable count.
        drop(self.proof.take());
        self.shared.lock().query_active = false;
        self.shared.changed.notify_all();
    }
}

#[cfg(test)]
type Hooks = Arc<test_support::TestHooks>;
#[cfg(not(test))]
#[derive(Default)]
struct Hooks;
