//! Individual discovered preparation owns its blocking work before any source IO.
use super::*;

pub(super) struct SourceWork {
    worker: Option<JoinHandle<()>>,
}
pub(super) fn finished(a: &Attempt) -> bool {
    a.host_source
        .as_ref()
        .is_none_or(|work| work.worker.as_ref().is_none_or(JoinHandle::is_finished))
}
pub(super) fn closing(a: &mut Attempt) {
    if a.snapshot.pending_host_source.take().is_some() {
        a.snapshot.expires_at_ms = None;
    }
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
        closing(a);
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
    }
}

#[cfg(windows)]
mod enabled {
    use super::*;
    use crate::engine::connect_discovery::{DiscoveryError, DiscoveryManager};
    use DiscoveredPreparationError::{Connect, Discovery};

    pub(in crate::engine::connect::live) enum SourceRequest {
        Known(HostDraftRequest),
        Fragmented(SessionFragmentedDiscoveredHostRequest),
    }
    impl SourceRequest {
        fn profile(&self) -> Profile {
            match self {
                Self::Known(request) => request.profile,
                Self::Fragmented(_) => Profile::SessionFragmented,
            }
        }
        fn mode(&self) -> ConnectMode {
            match self {
                Self::Known(request) => request.mode,
                Self::Fragmented(request) => request.mode,
            }
        }
        fn validate(&self) -> Result<SocketAddr, ConnectError> {
            let (mode, bind, udp) = match self {
                Self::Known(request) => {
                    request.validate()?;
                    policy(ConnectMode::Loopback, canonical(&request.game_target)?)?;
                    (request.mode, &request.bind_address, None)
                }
                Self::Fragmented(request) => {
                    if request.admission_seconds == 0
                        || request.admission_seconds > 600
                        || request.session_seconds < request.admission_seconds
                        || request.session_seconds > 3600
                    {
                        return Err(ConnectError::InvalidRequest);
                    }
                    (
                        request.mode,
                        &request.bind_address,
                        Some(&request.udp_target),
                    )
                }
            };
            let bind = canonical(bind)?;
            policy(mode, bind)?;
            if let Some(udp) = udp {
                policy(ConnectMode::Loopback, canonical(udp)?)?;
            }
            Ok(bind)
        }
        fn resolve(self, observed: SocketAddr) -> Result<HostDraftRequest, ConnectError> {
            match self {
                Self::Known(request) => {
                    if canonical(&request.game_target)? != observed {
                        return Err(ConnectError::InvalidRequest);
                    }
                    Ok(request)
                }
                Self::Fragmented(request) => Ok(SessionFragmentedHostRequest {
                    mode: request.mode,
                    bind_address: request.bind_address,
                    game_target: observed.to_string(),
                    udp_target: request.udp_target,
                    admission_seconds: request.admission_seconds,
                    session_seconds: request.session_seconds,
                }
                .into()),
            }
        }
    }

    #[cfg(test)]
    #[derive(Clone, Default)]
    pub(in crate::engine::connect::live) struct SourceHooks {
        pub(in crate::engine::connect::live) preparing: Option<Arc<group_source::SourceGate>>,
        pub(in crate::engine::connect::live) activating: Option<Arc<group_source::SourceGate>>,
        pub(in crate::engine::connect::live) validating: Option<Arc<group_source::SourceGate>>,
        pub(in crate::engine::connect::live) published: Option<Arc<group_source::SourceGate>>,
        pub(in crate::engine::connect::live) panic_query: bool,
    }

    impl Manager {
        pub(in crate::engine::connect) fn reserve_host_discovered(
            &self,
            owner: &str,
            request: HostRequest,
            selection_id: String,
            discovery: DiscoveryManager,
        ) -> Result<PendingHostPreparation, ConnectError> {
            self.reserve_host_source(
                owner,
                SourceRequest::Known(request.into()),
                selection_id,
                discovery,
                #[cfg(test)]
                SourceHooks::default(),
            )
        }
        pub(in crate::engine::connect) fn reserve_session_host_discovered(
            &self,
            owner: &str,
            request: SessionHostRequest,
            selection_id: String,
            discovery: DiscoveryManager,
        ) -> Result<PendingHostPreparation, ConnectError> {
            self.reserve_host_source(
                owner,
                SourceRequest::Known(request.into()),
                selection_id,
                discovery,
                #[cfg(test)]
                SourceHooks::default(),
            )
        }
        pub(in crate::engine::connect) fn reserve_session_fragmented_host_discovered(
            &self,
            owner: &str,
            request: SessionFragmentedDiscoveredHostRequest,
            selection_id: String,
            discovery: DiscoveryManager,
        ) -> Result<PendingHostPreparation, ConnectError> {
            self.reserve_host_source(
                owner,
                SourceRequest::Fragmented(request),
                selection_id,
                discovery,
                #[cfg(test)]
                SourceHooks::default(),
            )
        }
        pub(in crate::engine::connect::live) fn reserve_host_source(
            &self,
            owner: &str,
            request: SourceRequest,
            selection_id: String,
            discovery: DiscoveryManager,
            #[cfg(test)] hooks: SourceHooks,
        ) -> Result<PendingHostPreparation, ConnectError> {
            // Only pure validation precedes reservation; even take_target may do IO.
            if owner.is_empty()
                || owner.len() > 128
                || uuid::Uuid::parse_str(&selection_id).is_err()
            {
                return Err(ConnectError::InvalidRequest);
            }
            let bind = request.validate()?;
            let runtime =
                tokio::runtime::Handle::try_current().map_err(|_| ConnectError::Internal)?;
            let mut state = lock(&self.state)?;
            if state.shutting_down {
                return Err(ConnectError::Cancelled);
            }
            if let Some(a) = state.attempt.as_mut() {
                super::super::expire_pending(a);
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
            snapshot.expires_at_ms = Some(lab::unix_millis().saturating_add(60_000));
            snapshot.invitation_profile = request.profile().name();
            snapshot.pending_host_source = Some(PendingHostSource {
                kind: "ownedGameTcp",
                mode: request.mode(),
                profile: request.profile().name().unwrap_or("legacy"),
            });
            state.attempt = Some(Attempt {
                owner: owner.into(),
                snapshot,
                draft: None,
                reviewed: None,
                prepared_deadline: at + Duration::from_secs(60),
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
                host_source: Some(SourceWork { worker: None }),
                host_review: None,
                guest_source: None,
                ready_game: None,
                #[cfg(test)]
                recovery_gate: None,
                #[cfg(test)]
                group_start_gate: None,
            });
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let shared = self.state.clone();
            let owner = owner.to_owned();
            // Reservation and handle installation share this lock. No queued
            // closure can reserve later, after an IPC error or account barrier.
            let worker = runtime.spawn_blocking(move || {
                let queried = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    #[cfg(test)]
                    {
                        if let Some(gate) = &hooks.preparing {
                            gate.wait();
                        }
                        assert!(!hooks.panic_query, "injected owned source query panic");
                    }
                    check_preparing(&shared, &id).map_err(Connect)?;
                    let target = discovery
                        .take_target(&owner, &selection_id)
                        .map_err(Discovery)?;
                    #[cfg(test)]
                    if let Some(gate) = &hooks.activating {
                        gate.wait();
                    }
                    check_preparing(&shared, &id).map_err(Connect)?;
                    let target = target.activate().map_err(|error| {
                        Connect(if error == DiscoveryError::Stale {
                            ConnectError::PlanExpired
                        } else {
                            ConnectError::SourceChanged
                        })
                    })?;
                    #[cfg(test)]
                    if let Some(gate) = &hooks.validating {
                        gate.wait();
                    }
                    check_preparing(&shared, &id).map_err(Connect)?;
                    let observed = target
                        .recheck_owned()
                        .map_err(|_| Connect(ConnectError::SourceChanged))?;
                    check_preparing(&shared, &id).map_err(Connect)?;
                    let request = request.resolve(observed).map_err(Connect)?;
                    let services = if let Some(udp) = &request.udp_target {
                        fragmented_profile::services(observed, canonical(udp).map_err(Connect)?)
                            .map_err(Connect)?
                    } else {
                        vec![game(observed)]
                    };
                    Ok((request, TargetGuard::Owned(Arc::new(target)), services))
                }))
                .unwrap_or(Err(Connect(ConnectError::Internal)));
                let result = publish(&shared, &id, bind, queried);
                // Publishing to the waiter does not release the owned worker.
                let _ = sender.send(result);
                #[cfg(test)]
                if let Some(gate) = &hooks.published {
                    gate.wait();
                }
            });
            state
                .attempt
                .as_mut()
                .unwrap()
                .host_source
                .as_mut()
                .unwrap()
                .worker = Some(worker);
            Ok(PendingHostPreparation { result: receiver })
        }
    }

    fn check_attempt(state: &State, id: &str) -> Result<(), ConnectError> {
        let a = state.attempt.as_ref().ok_or(ConnectError::StaleOperation)?;
        if a.snapshot.operation_id.as_deref() != Some(id) {
            return Err(ConnectError::StaleOperation);
        }
        // Preserve expiry as the cause even when observation already cancelled it.
        if a.snapshot.error_code == Some(ConnectError::PlanExpired.code())
            || Instant::now() >= a.prepared_deadline
            || (a.snapshot.phase == "hostPreparing"
                && a.snapshot
                    .expires_at_ms
                    .is_none_or(|v| lab::unix_millis() >= v))
        {
            return Err(ConnectError::PlanExpired);
        }
        if state.shutting_down || a.cancel.is_cancelled() || a.snapshot.phase != "hostPreparing" {
            return Err(ConnectError::Cancelled);
        }
        Ok(())
    }
    fn check_preparing(state: &Mutex<State>, id: &str) -> Result<(), ConnectError> {
        let state = lock(state)?;
        check_attempt(&state, id)
    }
    type QueryResult =
        Result<(HostDraftRequest, TargetGuard, Vec<lab::Service>), DiscoveredPreparationError>;
    fn publish(
        state: &Mutex<State>,
        id: &str,
        bind: SocketAddr,
        queried: QueryResult,
    ) -> Result<ConnectPreparation, DiscoveredPreparationError> {
        let mut state = lock(state).map_err(Connect)?;
        let result = check_attempt(&state, id).map_err(Connect).and(queried);
        let a = state
            .attempt
            .as_mut()
            .ok_or(Connect(ConnectError::StaleOperation))?;
        if a.snapshot.operation_id.as_deref() != Some(id) {
            return Err(Connect(ConnectError::StaleOperation));
        }
        match result {
            Ok((request, source, services)) => {
                let key = lab::generate_key();
                let preparation = ConnectPreparation {
                    operation_id: id.into(),
                    generation: a.snapshot.generation,
                    public_id: key.public().to_string(),
                    expires_at_ms: a
                        .snapshot
                        .expires_at_ms
                        .ok_or(Connect(ConnectError::Internal))?,
                    invitation_profile: request.profile.name(),
                };
                a.draft = Some(Draft::Host {
                    request,
                    key,
                    bind,
                    services,
                    source,
                });
                a.snapshot.phase = "hostPrepared";
                a.snapshot.preparation = Some(preparation.clone());
                a.snapshot.pending_host_source = None;
                a.snapshot.expires_at_ms = None;
                // Like the previous hostPrepared shape, no transport exists.
                a.snapshot.cleanup_state = "notStarted";
                Ok(preparation)
            }
            Err(error) => {
                if a.snapshot.phase == "hostPreparing" {
                    let projection = match error {
                        Connect(error) => error,
                        Discovery(DiscoveryError::Stale) => ConnectError::PlanExpired,
                        Discovery(DiscoveryError::Owner) => ConnectError::OwnerMismatch,
                        Discovery(DiscoveryError::Busy) => ConnectError::Busy,
                        Discovery(DiscoveryError::Internal) => ConnectError::Internal,
                        Discovery(_) => ConnectError::SourceChanged,
                    };
                    a.cancel.cancel();
                    a.snapshot.phase = if projection == ConnectError::Cancelled {
                        "closed"
                    } else {
                        "failed"
                    };
                    a.snapshot.transport_state = "closed";
                    a.snapshot.error_code =
                        (projection != ConnectError::Cancelled).then_some(projection.code());
                    closing(a);
                    a.snapshot.generation = a.snapshot.generation.saturating_add(1);
                }
                Err(error)
            }
        }
    }
}
#[cfg(all(windows, test))]
pub(super) use enabled::{SourceHooks, SourceRequest};
