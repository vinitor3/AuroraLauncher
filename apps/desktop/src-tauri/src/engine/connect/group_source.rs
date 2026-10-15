//! Source work belongs to the Attempt even before a Host identity exists.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct SourceWork {
    worker: Option<JoinHandle<()>>,
    review_active: Arc<AtomicBool>,
    #[cfg(test)]
    pub(super) hooks: SourceHooks,
}
impl Default for SourceWork {
    fn default() -> Self {
        Self {
            worker: None,
            review_active: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            hooks: SourceHooks::default(),
        }
    }
}
pub(super) fn finished(a: &Attempt) -> bool {
    a.group_source.as_ref().is_none_or(|work| {
        work.worker.as_ref().is_none_or(JoinHandle::is_finished)
            && !work.review_active.load(Ordering::Acquire)
    })
}
pub(super) fn expire_pending(a: &mut Attempt, now: u64) {
    if a.snapshot.phase == "hostPreparing"
        && (Instant::now() >= a.prepared_deadline
            || a.snapshot
                .expires_at_ms
                .is_none_or(|expires| now >= expires))
    {
        a.cancel.cancel();
        a.snapshot.phase = "closed";
        a.snapshot.transport_state = "closed";
        a.snapshot.cleanup_state = if finished(a) { "complete" } else { "pending" };
        a.snapshot.error_code = Some(ConnectError::PlanExpired.code());
        a.snapshot.pending_group_source = None;
        a.snapshot.expires_at_ms = None;
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
    }
}

/// Held by the thread doing the actual blocking query, never just an IPC waiter.
pub(super) struct ReviewGuard(Arc<AtomicBool>);
impl Drop for ReviewGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
pub(super) fn begin_review(a: &Attempt) -> Result<Option<ReviewGuard>, ConnectError> {
    let Some(work) = &a.group_source else {
        return Ok(None);
    };
    work.review_active
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| ConnectError::Busy)?;
    Ok(Some(ReviewGuard(work.review_active.clone())))
}
pub(super) async fn recheck(
    source: TargetGuard,
    #[cfg(test)] gate: Option<Arc<SourceGate>>,
) -> Result<(), ConnectError> {
    if matches!(source, TargetGuard::Manual) {
        return Ok(());
    }
    tokio::task::spawn_blocking(move || {
        #[cfg(test)]
        if let Some(gate) = gate {
            gate.wait();
        }
        source.recheck().map(|_| ())
    })
    .await
    .map_err(|_| ConnectError::Internal)?
}

#[cfg(test)]
#[derive(Clone, Default)]
pub(super) struct SourceHooks {
    pub(super) preparing: Option<Arc<SourceGate>>,
    pub(super) published: Option<Arc<SourceGate>>,
    pub(super) review: Option<Arc<SourceGate>>,
    pub(super) monitor: Option<Arc<SourceGate>>,
}
#[cfg(test)]
#[derive(Default)]
pub(super) struct SourceGate {
    pub(super) entered: tokio::sync::Notify,
    released: Mutex<bool>,
    changed: std::sync::Condvar,
}
#[cfg(test)]
impl SourceGate {
    pub(super) fn wait(&self) {
        self.entered.notify_one();
        let guard = self.released.lock().unwrap();
        let (guard, timed) = self
            .changed
            .wait_timeout_while(guard, Duration::from_secs(10), |v| !*v)
            .unwrap();
        drop(guard);
        assert!(!timed.timed_out(), "owned source fixture gate deadline");
    }
    pub(super) fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

#[cfg(windows)]
impl Manager {
    pub(in crate::engine::connect) fn prepare_session_group_host_discovered(
        &self,
        owner: &str,
        request: GroupDiscoveredHostRequest,
        selection_id: String,
        discovery: crate::engine::connect_discovery::DiscoveryManager,
    ) -> Result<ConnectSnapshot, ConnectError> {
        self.prepare_group_source(
            owner,
            request,
            selection_id,
            discovery,
            #[cfg(test)]
            SourceHooks::default(),
        )
    }

    pub(super) fn prepare_group_source(
        &self,
        owner: &str,
        request: GroupDiscoveredHostRequest,
        selection_id: String,
        discovery: crate::engine::connect_discovery::DiscoveryManager,
        #[cfg(test)] hooks: SourceHooks,
    ) -> Result<ConnectSnapshot, ConnectError> {
        if owner.is_empty() || owner.len() > 128 || uuid::Uuid::parse_str(&selection_id).is_err() {
            return Err(ConnectError::InvalidRequest);
        }
        let bind = canonical(&request.bind_address)?;
        policy(request.mode, bind)?;
        let udp = canonical(&request.udp_target)?;
        policy(ConnectMode::Loopback, udp)?;
        let mut state = lock(&self.state)?;
        if state.shutting_down {
            return Err(ConnectError::Cancelled);
        }
        if let Some(a) = state.attempt.as_mut() {
            super::expire_pending(a);
            ready_game::refresh_cleanup(a);
            if !terminal(a) {
                return Err(ConnectError::Busy);
            }
        }
        state.generation = state
            .generation
            .max(state.attempt.as_ref().map_or(0, |a| a.snapshot.generation))
            .saturating_add(1);
        let id = uuid::Uuid::new_v4().to_string();
        let at = Instant::now();
        let mut snapshot = ConnectSnapshot::idle();
        snapshot.operation_id = Some(id.clone());
        snapshot.generation = state.generation;
        snapshot.role = Some("host");
        snapshot.phase = "hostPreparing";
        snapshot.cleanup_state = "pending";
        snapshot.expires_at_ms = Some(lab::unix_millis().saturating_add(600_000));
        snapshot.pending_group_source = Some(PendingGroupSource {
            kind: crate::engine::connect::group_profile::GROUP_KIND,
            mode: request.mode,
        });
        let cancel = CancellationToken::new();
        state.attempt = Some(Attempt {
            owner: owner.into(),
            snapshot: snapshot.clone(),
            draft: None,
            reviewed: None,
            prepared_deadline: at + Duration::from_secs(600),
            cancel: cancel.clone(),
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
            guest_source: None,
            ready_game: None,
            host_source: None,
            host_review: None,
            group_source: Some(SourceWork {
                #[cfg(test)]
                hooks: hooks.clone(),
                ..SourceWork::default()
            }),
            #[cfg(test)]
            recovery_gate: None,
            #[cfg(test)]
            group_start_gate: None,
        });
        let shared = self.state.clone();
        let owner = owner.to_owned();
        // No observer can see the reservation without its owned handle: the
        // mutex remains held until installation. Worker checks it before IO.
        let worker = tokio::spawn(async move {
            let query_state = shared.clone();
            let query_id = id.clone();
            let result = tokio::task::spawn_blocking(move || {
                #[cfg(test)]
                if let Some(gate) = &hooks.preparing {
                    gate.wait();
                }
                check_preparing(&query_state, &query_id)?;
                let target = discovery
                    .take_target(&owner, &selection_id)
                    .map_err(discovery_error)?;
                check_preparing(&query_state, &query_id)?;
                let target = target.activate().map_err(discovery_error)?;
                check_preparing(&query_state, &query_id)?;
                let game = target.recheck_owned().map_err(discovery_error)?;
                check_preparing(&query_state, &query_id)?;
                let services = fragmented_profile::services(game, udp)?;
                Ok((
                    TargetGuard::Owned(Arc::new(target)),
                    GroupHostRequest {
                        mode: request.mode,
                        bind_address: request.bind_address,
                        game_target: game.to_string(),
                        udp_target: request.udp_target,
                    },
                    services,
                ))
            })
            .await
            .unwrap_or(Err(ConnectError::Internal));
            if let Ok(mut state) = lock(&shared) {
                let shutdown = state.shutting_down;
                if let Some(a) = state.attempt.as_mut().filter(|a| {
                    a.snapshot.operation_id.as_deref() == Some(&id)
                        && a.snapshot.phase == "hostPreparing"
                }) {
                    let result = if shutdown || a.cancel.is_cancelled() {
                        Err(ConnectError::Cancelled)
                    } else if Instant::now() >= a.prepared_deadline
                        || a.snapshot
                            .expires_at_ms
                            .is_none_or(|v| lab::unix_millis() >= v)
                    {
                        Err(ConnectError::PlanExpired)
                    } else {
                        result
                    };
                    match result {
                        Ok((source, request, services)) => {
                            session_group::publish_discovered(a, source, request, bind, services)
                        }
                        Err(error) => {
                            a.snapshot.phase = if error == ConnectError::Cancelled {
                                "closed"
                            } else {
                                "failed"
                            };
                            a.snapshot.transport_state = "closed";
                            a.snapshot.error_code =
                                (error != ConnectError::Cancelled).then_some(error.code());
                            a.snapshot.pending_group_source = None;
                            a.snapshot.expires_at_ms = None;
                            a.snapshot.generation = a.snapshot.generation.saturating_add(1);
                        }
                    }
                }
            }
            #[cfg(test)]
            {
                let gate = lock(&shared).ok().and_then(|state| {
                    state
                        .attempt
                        .as_ref()
                        .and_then(|a| a.group_source.as_ref())
                        .and_then(|work| work.hooks.published.clone())
                });
                if let Some(gate) = gate {
                    let _ = tokio::task::spawn_blocking(move || gate.wait()).await;
                }
            }
        });
        state
            .attempt
            .as_mut()
            .unwrap()
            .group_source
            .as_mut()
            .unwrap()
            .worker = Some(worker);
        Ok(snapshot)
    }
}
#[cfg(windows)]
fn check_preparing(state: &Mutex<State>, id: &str) -> Result<(), ConnectError> {
    let state = lock(state)?;
    let a = state.attempt.as_ref().ok_or(ConnectError::StaleOperation)?;
    if state.shutting_down
        || a.cancel.is_cancelled()
        || a.snapshot.operation_id.as_deref() != Some(id)
        || a.snapshot.phase != "hostPreparing"
    {
        return Err(ConnectError::Cancelled);
    }
    if Instant::now() >= a.prepared_deadline
        || a.snapshot
            .expires_at_ms
            .is_none_or(|v| lab::unix_millis() >= v)
    {
        return Err(ConnectError::PlanExpired);
    }
    Ok(())
}
#[cfg(windows)]
fn discovery_error(error: crate::engine::connect_discovery::DiscoveryError) -> ConnectError {
    use crate::engine::connect_discovery::DiscoveryError;
    match error {
        DiscoveryError::Stale => ConnectError::PlanExpired,
        DiscoveryError::Owner => ConnectError::OwnerMismatch,
        DiscoveryError::Busy => ConnectError::Busy,
        DiscoveryError::Internal => ConnectError::Internal,
        _ => ConnectError::SourceChanged,
    }
}
