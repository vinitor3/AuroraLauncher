//! Owned ready-game worker and final native transport gate.
use super::*;
use crate::engine::connect::ready_game_backend::ReadyGameBackend;

use std::sync::{mpsc, Weak};

pub(super) struct GuestSource {
    guest: lab::Guest,
    pub(super) bridge: Weak<lab::TcpBridge>,
    authority: Authority,
    deadline: Instant,
    cancel: CancellationToken,
}
impl GuestSource {
    pub(super) fn new(
        guest: &lab::Guest,
        bridge: &Arc<lab::TcpBridge>,
        authority: &Authority,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Arc<Self> {
        Arc::new(Self {
            guest: guest.clone(),
            bridge: Arc::downgrade(bridge),
            authority: authority.clone(),
            deadline,
            cancel: cancel.clone(),
        })
    }
    fn address(&self) -> Result<SocketAddr, ReadyGameLaunchError> {
        if self.cancel.is_cancelled()
            || Instant::now() >= self.deadline
            || self.authority.check_session().is_err()
            || self.guest.connection().close_reason().is_some()
        {
            return Err(ReadyGameLaunchError::Unavailable);
        }
        let bridge = self
            .bridge
            .upgrade()
            .ok_or(ReadyGameLaunchError::Unavailable)?;
        if bridge.is_closed() {
            return Err(ReadyGameLaunchError::Unavailable);
        }
        Ok(bridge.local_addr())
    }
}
pub(super) struct ReadyWork {
    snapshot: ReadyGameLaunchSnapshot,
    cancel: CancellationToken,
    confirm: mpsc::SyncSender<()>,
    worker: Option<JoinHandle<()>>,
    digest: Option<String>,
    deadline: Instant,
}
pub(super) fn finished(a: &Attempt) -> bool {
    a.ready_game
        .as_ref()
        .is_none_or(|work| work.worker.as_ref().is_none_or(JoinHandle::is_finished))
}
pub(super) fn cancel(a: &Attempt) {
    if let Some(work) = &a.ready_game {
        work.cancel.cancel();
    }
}
#[cfg(test)]
pub(super) fn expire_preparation_for_test(a: &mut Attempt) {
    let work = a.ready_game.as_mut().expect("owned test work");
    assert_eq!(work.snapshot.phase, "preparing");
    work.deadline = Instant::now();
}
pub(super) fn refresh_cleanup(a: &mut Attempt) {
    if finished(a)
        && group_source::finished(a)
        && !a.probe_active
        && a.worker.as_ref().is_none_or(JoinHandle::is_finished)
        && matches!(a.snapshot.phase, "closed" | "failed")
        && a.snapshot.cleanup_state == "pending"
    {
        a.snapshot.cleanup_state = "complete";
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
    }
}

/// Opaque source and operation identity captured only by an admitted Guest actor.
/// Tracking locks must be acquired BEFORE this gate (Running → IPC → Connect).
pub struct ReadyGameSpawnGate {
    state: Arc<Mutex<State>>,
    operation: String,
    plan: String,
    source: Arc<GuestSource>,
    cancel: CancellationToken,
    deadline: Instant,
    expires_at: u64,
}
impl ReadyGameSpawnGate {
    pub fn with_current(
        &self,
        spawn_and_register: impl FnOnce() -> Result<ReadyGameLaunchResult, ReadyGameLaunchError>,
    ) -> Result<ReadyGameLaunchResult, ReadyGameLaunchError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ReadyGameLaunchError::Internal)?;
        let a = state
            .attempt
            .as_ref()
            .ok_or(ReadyGameLaunchError::StaleOperation)?;
        let work = a
            .ready_game
            .as_ref()
            .ok_or(ReadyGameLaunchError::StaleOperation)?;
        if a.snapshot.operation_id.as_deref() != Some(&self.operation)
            || a.snapshot.phase != "guestConnected"
            || work.snapshot.plan_id.as_deref() != Some(&self.plan)
            || work.snapshot.phase != "starting"
            || a.guest_source
                .as_ref()
                .is_none_or(|source| !Arc::ptr_eq(source, &self.source))
            || a.cancel.is_cancelled()
            || self.cancel.is_cancelled()
        {
            return Err(ReadyGameLaunchError::Cancelled);
        }
        if Instant::now() >= self.deadline || lab::unix_millis() >= self.expires_at {
            return Err(ReadyGameLaunchError::Expired);
        }
        self.source.address()?;
        let result = spawn_and_register()?;
        let work = state
            .attempt
            .as_mut()
            .and_then(|a| a.ready_game.as_mut())
            .ok_or(ReadyGameLaunchError::Internal)?;
        work.snapshot.phase = "started";
        work.snapshot.result = Some(result.clone());
        work.snapshot.review = None;
        work.snapshot.error_code = None;
        work.snapshot.generation = work.snapshot.generation.saturating_add(1);
        Ok(result)
    }
}

fn selected_ready<'a>(
    state: &'a mut State,
    owner: &str,
    operation: &str,
) -> Result<&'a mut Attempt, ReadyGameLaunchError> {
    selected(state, owner, operation).map_err(|e| match e {
        ConnectError::OwnerMismatch => ReadyGameLaunchError::WrongOwner,
        _ => ReadyGameLaunchError::StaleOperation,
    })
}
fn update_ready(
    state: &Mutex<State>,
    operation: &str,
    plan: &str,
    change: impl FnOnce(&mut ReadyWork),
) {
    if let Ok(mut state) = state.lock() {
        if let Some(a) = state.attempt.as_mut() {
            if a.snapshot.operation_id.as_deref() == Some(operation) {
                if let Some(work) = &mut a.ready_game {
                    if work.snapshot.plan_id.as_deref() == Some(plan) {
                        change(work);
                        work.snapshot.generation = work.snapshot.generation.saturating_add(1);
                    }
                }
            }
        }
    }
}
fn finish_ready(
    state: &Mutex<State>,
    operation: &str,
    plan: &str,
    result: Result<ReadyGameLaunchResult, ReadyGameLaunchError>,
) {
    update_ready(state, operation, plan, |work| {
        if work.snapshot.phase == "started" {
            return;
        }
        let result = if matches!(result, Err(ReadyGameLaunchError::Cancelled))
            && Instant::now() >= work.deadline
        {
            Err(ReadyGameLaunchError::Expired)
        } else {
            result
        };
        work.snapshot.review = None;
        match result {
            Ok(result) => {
                work.snapshot.phase = "started";
                work.snapshot.result = Some(result);
                work.snapshot.error_code = None;
            }
            Err(error) => {
                work.snapshot.phase = match error {
                    ReadyGameLaunchError::Cancelled => "cancelled",
                    ReadyGameLaunchError::Expired => "expired",
                    _ => "failed",
                };
                work.snapshot.result = None;
                work.snapshot.error_code = Some(error.code());
            }
        }
    });
}

impl Manager {
    pub(in crate::engine::connect) fn prepare_ready_game(
        &self,
        owner: &str,
        request: ReadyGameLaunchRequest,
        backend: Arc<dyn ReadyGameBackend>,
    ) -> Result<ReadyGameLaunchSnapshot, ReadyGameLaunchError> {
        request.validate()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| ReadyGameLaunchError::Internal)?;
        if state.shutting_down {
            return Err(ReadyGameLaunchError::Cancelled);
        }
        let a = selected_ready(&mut state, owner, &request.operation_id)?;
        if a.snapshot.phase != "guestConnected" || a.cancel.is_cancelled() {
            return Err(ReadyGameLaunchError::Unavailable);
        }
        if !finished(a) {
            return Err(ReadyGameLaunchError::Busy);
        }
        let source = a
            .guest_source
            .clone()
            .ok_or(ReadyGameLaunchError::Unavailable)?;
        let endpoint = source.address()?;
        let plan = uuid::Uuid::new_v4().to_string();
        let cancel = a.cancel.child_token();
        let deadline = source
            .deadline
            .min(Instant::now() + Duration::from_secs(60));
        let session_expiry = a
            .snapshot
            .expires_at_ms
            .ok_or(ReadyGameLaunchError::Unavailable)?;
        let expires_at = session_expiry.min(
            lab::unix_millis().saturating_add(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis() as u64,
            ),
        );
        let (send, receiver) = mpsc::sync_channel(1);
        let generation = a
            .ready_game
            .as_ref()
            .map_or(1, |work| work.snapshot.generation.saturating_add(1));
        let snapshot = ReadyGameLaunchSnapshot {
            operation_id: request.operation_id.clone(),
            generation,
            plan_id: Some(plan.clone()),
            phase: "preparing",
            review: None,
            result: None,
            error_code: None,
        };
        a.ready_game = Some(ReadyWork {
            snapshot: snapshot.clone(),
            cancel: cancel.clone(),
            confirm: send,
            worker: None,
            digest: None,
            deadline,
        });
        let owned_state = self.state.clone();
        let operation = request.operation_id.clone();
        let owner = owner.to_owned();
        let monitor_state = owned_state.clone();
        let monitor_operation = operation.clone();
        let monitor_plan = plan.clone();
        let monitor_cancel = cancel.clone();
        let worker = tokio::spawn(async move {
            let mut blocking = tokio::task::spawn_blocking(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let check = || {
                        if Instant::now() >= deadline || lab::unix_millis() >= expires_at {
                            return Err(ReadyGameLaunchError::Expired);
                        }
                        if cancel.is_cancelled() {
                            return Err(ReadyGameLaunchError::Cancelled);
                        }
                        source.address().map(|_| ())
                    };
                    check()?;
                    let prepared = backend.prepare(&request, endpoint, &cancel)?;
                    check()?;
                    // Preparation and approval are independently bounded, without
                    // replacing or extending the original session authority.
                    let deadline = source
                        .deadline
                        .min(Instant::now() + Duration::from_secs(60));
                    let expires_at = session_expiry.min(
                        lab::unix_millis().saturating_add(
                            deadline
                                .saturating_duration_since(Instant::now())
                                .as_millis() as u64,
                        ),
                    );
                    let check = || {
                        if Instant::now() >= deadline || lab::unix_millis() >= expires_at {
                            return Err(ReadyGameLaunchError::Expired);
                        }
                        if cancel.is_cancelled() {
                            return Err(ReadyGameLaunchError::Cancelled);
                        }
                        source.address().map(|_| ())
                    };
                    check()?;
                    let material = serde_json::to_vec(&(
                        &owner,
                        &request,
                        &plan,
                        endpoint,
                        expires_at,
                        prepared.fingerprint(),
                    ))
                    .map_err(|_| ReadyGameLaunchError::Internal)?;
                    let mut hash = Sha256::new();
                    hash.update(b"aurora-connect-ready-game-v1\0");
                    hash.update(material);
                    let digest = format!("{:x}", hash.finalize());
                    let review = ReadyGameLaunchReview {
                        operation_id: operation.clone(),
                        plan_id: plan.clone(),
                        digest: digest.clone(),
                        expires_at_ms: expires_at,
                        session_expires_at_ms: session_expiry,
                        instance_id: request.instance_id.clone(),
                        source_instance_id: request.source_instance_id.clone(),
                        context_key: request.context_key.clone(),
                        minecraft_version: "1.21.1",
                        loader: "fabric",
                        loader_version: "0.16.14",
                        java_executable: request.java_executable.clone(),
                        java_major: 21,
                        nickname: request.nickname.clone(),
                        appearance: request.appearance.clone(),
                        game_address: endpoint.to_string(),
                        core_version: "1.1.5",
                        core_sha256: prepared.core_sha256().into(),
                        skins_version: "0.3.1",
                        skins_sha256: prepared.skins_sha256().into(),
                    };
                    update_ready(&owned_state, &operation, &plan, |work| {
                        work.deadline = deadline;
                        work.snapshot.phase = "reviewed";
                        work.snapshot.review = Some(review);
                        work.digest = Some(digest);
                    });
                    loop {
                        check()?;
                        match receiver.recv_timeout(Duration::from_millis(20)) {
                            Ok(()) => break,
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Err(_) => return Err(ReadyGameLaunchError::Cancelled),
                        }
                    }
                    check()?;
                    let gate = ReadyGameSpawnGate {
                        state: owned_state.clone(),
                        operation: operation.clone(),
                        plan: plan.clone(),
                        source: source.clone(),
                        cancel: cancel.clone(),
                        deadline,
                        expires_at,
                    };
                    prepared.launch(&gate, &cancel)
                }))
                .unwrap_or(Err(ReadyGameLaunchError::Internal));
                finish_ready(&owned_state, &operation, &plan, result);
            });
            // The async owner never drops or aborts the blocking task. Deadline
            // and stop only signal its token, then await its registered outcome.
            loop {
                tokio::select! {
                    result = &mut blocking => {
                        if result.is_err() { finish_ready(&monitor_state, &monitor_operation, &monitor_plan, Err(ReadyGameLaunchError::Internal)); }
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {
                        let expired = monitor_state.lock().ok().and_then(|state| state.attempt.as_ref().and_then(|a| a.ready_game.as_ref().map(|work| Instant::now() >= work.deadline))).unwrap_or(true);
                        if expired { monitor_cancel.cancel(); }
                    }
                }
            }
        });
        a.ready_game.as_mut().expect("inserted").worker = Some(worker);
        Ok(snapshot)
    }
    pub(in crate::engine::connect) fn ready_game_snapshot(
        &self,
        owner: &str,
        operation: &str,
    ) -> Result<ReadyGameLaunchSnapshot, ReadyGameLaunchError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ReadyGameLaunchError::Internal)?;
        let a = selected_ready(&mut state, owner, operation)?;
        Ok(a.ready_game.as_ref().map_or_else(
            || ReadyGameLaunchSnapshot::idle(operation),
            |work| work.snapshot.clone(),
        ))
    }
    pub(in crate::engine::connect) fn confirm_ready_game(
        &self,
        owner: &str,
        operation: &str,
        plan: &str,
        digest: &str,
    ) -> Result<ReadyGameLaunchSnapshot, ReadyGameLaunchError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ReadyGameLaunchError::Internal)?;
        let a = selected_ready(&mut state, owner, operation)?;
        let work = a
            .ready_game
            .as_mut()
            .ok_or(ReadyGameLaunchError::StaleOperation)?;
        if work.snapshot.plan_id.as_deref() != Some(plan) {
            return Err(ReadyGameLaunchError::StaleOperation);
        }
        if work.digest.as_deref() != Some(digest) {
            return Err(ReadyGameLaunchError::ApprovalMismatch);
        }
        if matches!(work.snapshot.phase, "starting" | "started") {
            return Ok(work.snapshot.clone());
        }
        if work.snapshot.phase != "reviewed" {
            return Err(ReadyGameLaunchError::Unavailable);
        }
        if work.cancel.is_cancelled() || a.cancel.is_cancelled() {
            return Err(ReadyGameLaunchError::Cancelled);
        }
        if Instant::now() >= work.deadline
            || work
                .snapshot
                .review
                .as_ref()
                .is_none_or(|r| lab::unix_millis() >= r.expires_at_ms)
        {
            work.cancel.cancel();
            return Err(ReadyGameLaunchError::Expired);
        }
        a.guest_source
            .as_ref()
            .ok_or(ReadyGameLaunchError::Unavailable)?
            .address()?;
        work.snapshot.phase = "starting";
        work.snapshot.review = None;
        work.snapshot.generation = work.snapshot.generation.saturating_add(1);
        work.confirm
            .try_send(())
            .map_err(|_| ReadyGameLaunchError::Internal)?;
        Ok(work.snapshot.clone())
    }
    pub(in crate::engine::connect) async fn cancel_ready_game(
        &self,
        owner: &str,
        operation: &str,
        plan: &str,
    ) -> Result<ReadyGameLaunchSnapshot, ReadyGameLaunchError> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| ReadyGameLaunchError::Internal)?;
            let a = selected_ready(&mut state, owner, operation)?;
            let work = a
                .ready_game
                .as_ref()
                .ok_or(ReadyGameLaunchError::StaleOperation)?;
            if work.snapshot.plan_id.as_deref() != Some(plan) {
                return Err(ReadyGameLaunchError::StaleOperation);
            }
            work.cancel.cancel();
        }
        loop {
            {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| ReadyGameLaunchError::Internal)?;
                let a = selected_ready(&mut state, owner, operation)?;
                if a.ready_game
                    .as_ref()
                    .is_none_or(|work| work.snapshot.plan_id.as_deref() != Some(plan))
                {
                    return Err(ReadyGameLaunchError::StaleOperation);
                }
                if finished(a) {
                    return Ok(a.ready_game.as_ref().expect("owned work").snapshot.clone());
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
