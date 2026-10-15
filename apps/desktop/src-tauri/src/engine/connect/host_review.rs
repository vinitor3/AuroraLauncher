//! A review and every blocking actor query remain owned until actual completion.
use super::*;

pub(super) struct ReviewWork {
    worker: Option<JoinHandle<()>>,
    #[cfg(test)]
    pub(super) hooks: ReviewHooks,
}
pub(super) fn finished(a: &Attempt) -> bool {
    a.host_review
        .as_ref()
        .is_none_or(|work| work.worker.as_ref().is_none_or(JoinHandle::is_finished))
}
#[cfg(test)]
#[derive(Clone, Default)]
pub(super) struct ReviewHooks {
    pub(super) query: Option<Arc<group_source::SourceGate>>,
    pub(super) published: Option<Arc<group_source::SourceGate>>,
    pub(super) before_bind: Option<Arc<group_source::SourceGate>>,
    pub(super) after_bind: Option<Arc<group_source::SourceGate>>,
    pub(super) monitor: Option<Arc<group_source::SourceGate>>,
    pub(super) final_query: Option<Arc<group_source::SourceGate>>,
    pub(super) panic_query: bool,
    pub(super) expire_after_issue: bool,
    pub(super) monotonic_now: Option<Arc<Mutex<Instant>>>,
    pub(super) monitor_observation: Option<Arc<MonitorObservation>>,
}
#[cfg(test)]
#[derive(Default)]
pub(super) struct MonitorObservation {
    pub(super) latest: Mutex<Option<(Instant, bool)>>,
}
impl Manager {
    pub(in crate::engine::connect) fn reserve_host_review(
        &self,
        owner: &str,
        id: &str,
        guest_public_id: &str,
    ) -> Result<PendingHostReview, ConnectError> {
        self.reserve_review(
            owner,
            id,
            guest_public_id,
            #[cfg(test)]
            ReviewHooks::default(),
        )
    }
    pub(super) fn reserve_review(
        &self,
        owner: &str,
        id: &str,
        guest_public_id: &str,
        #[cfg(test)] hooks: ReviewHooks,
    ) -> Result<PendingHostReview, ConnectError> {
        let recipient = peer(guest_public_id)?;
        let mut state = lock(&self.state)?;
        if state.shutting_down {
            return Err(ConnectError::Cancelled);
        }
        let a = selected(&mut state, owner, id)?;
        expire_pending(a);
        if !finished(a) {
            return Err(ConnectError::Busy);
        }
        if a.snapshot.phase != "hostPrepared" {
            return Err(ConnectError::PlanExpired);
        }
        if a.cancel.is_cancelled() {
            return Err(ConnectError::Cancelled);
        }
        let Draft::Host { source, key, .. } =
            a.draft.as_ref().ok_or(ConnectError::StaleOperation)?
        else {
            return Err(ConnectError::InvalidRequest);
        };
        if recipient == key.public() {
            return Err(ConnectError::InvitationRejected);
        }
        if matches!(source, TargetGuard::Manual) {
            // This path contains no process/listener IO and needs no worker.
            return Self::issue_host_review(a, id, recipient)
                .map(|review| PendingHostReview { result: Ok(review) });
        }
        let source = source.clone();
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| ConnectError::Internal)?;
        let generation = a.snapshot.generation;
        a.host_review = Some(ReviewWork {
            worker: None,
            #[cfg(test)]
            hooks: hooks.clone(),
        });
        let (send, receive) = tokio::sync::oneshot::channel();
        let shared = self.state.clone();
        let id = id.to_owned();
        let owner = owner.to_owned();
        // Install under the reservation lock. A queued worker cannot create or
        // recover authority after a caller has stopped this operation.
        let worker = runtime.spawn_blocking(move || {
            let queried = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                #[cfg(test)]
                {
                    if let Some(gate) = &hooks.query {
                        gate.wait();
                    }
                    assert!(!hooks.panic_query, "injected owned review panic");
                }
                {
                    let mut state = lock(&shared)?;
                    check_review(&mut state, &owner, &id, generation)?;
                }
                source.recheck().map(|_| ())
            }))
            .unwrap_or(Err(ConnectError::Internal));
            let result = publish(&shared, &owner, &id, generation, recipient, queried);
            let _ = send.send(result);
            #[cfg(test)]
            if let Some(gate) = &hooks.published {
                gate.wait();
            }
        });
        state
            .attempt
            .as_mut()
            .unwrap()
            .host_review
            .as_mut()
            .unwrap()
            .worker = Some(worker);
        Ok(PendingHostReview {
            result: Err(receive),
        })
    }
}
fn check_review(
    state: &mut State,
    owner: &str,
    id: &str,
    generation: u64,
) -> Result<(), ConnectError> {
    if state.shutting_down {
        return Err(ConnectError::Cancelled);
    }
    let a = selected(state, owner, id)?;
    expire_pending(a);
    if a.snapshot.error_code == Some(ConnectError::PlanExpired.code()) {
        return Err(ConnectError::PlanExpired);
    }
    if a.cancel.is_cancelled() || a.snapshot.phase != "hostPrepared" {
        return Err(ConnectError::Cancelled);
    }
    if a.snapshot.generation != generation {
        return Err(ConnectError::StaleOperation);
    }
    Ok(())
}
fn publish(
    state: &Mutex<State>,
    owner: &str,
    id: &str,
    generation: u64,
    recipient: lab::EndpointId,
    queried: Result<(), ConnectError>,
) -> Result<ConnectReview, ConnectError> {
    let mut state = lock(state)?;
    let current = check_review(&mut state, owner, id, generation);
    let a = selected(&mut state, owner, id)?;
    let result = current
        .and(queried)
        .and_then(|()| Manager::issue_host_review(a, id, recipient));
    if let Err(error) = result {
        if a.snapshot.phase == "hostPrepared" {
            a.cancel.cancel();
            a.draft = None;
            a.reviewed = None;
            a.export = None;
            a.authority = None;
            a.snapshot.preparation = None;
            a.snapshot.review = None;
            a.snapshot.phase = if error == ConnectError::Cancelled {
                "closed"
            } else {
                "failed"
            };
            a.snapshot.transport_state = "closed";
            a.snapshot.cleanup_state = "pending";
            a.snapshot.expires_at_ms = None;
            a.snapshot.error_code = (error != ConnectError::Cancelled).then_some(error.code());
            a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        }
    }
    result
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum QueryStage {
    BeforeBind,
    AfterBind,
    Monitor,
    Final,
}
pub(super) struct ActorSource<'a> {
    pub(super) state: &'a Mutex<State>,
    pub(super) id: &'a str,
    pub(super) cancel: &'a CancellationToken,
    pub(super) authority: &'a Authority,
    pub(super) admission_deadline: Instant,
    pub(super) deadline: Instant,
    // Owning this handle lets interruption drop the real Endpoint before
    // awaiting a blocked source query. Endpoint::close alone retains sockets.
    pub(super) host: Option<Arc<lab::Host>>,
    pub(super) lease_id: &'a str,
    #[cfg(test)]
    pub(super) monotonic_now: Option<Arc<Mutex<Instant>>>,
    #[cfg(test)]
    pub(super) monitor_observation: Option<Arc<MonitorObservation>>,
}
impl ActorSource<'_> {
    fn now(&self) -> Instant {
        #[cfg(test)]
        if let Some(clock) = &self.monotonic_now {
            return *clock.lock().unwrap();
        }
        Instant::now()
    }
    pub(super) fn permitted(&self, stage: QueryStage) -> bool {
        if stage == QueryStage::Final {
            return true;
        }
        let now = self.now();
        let admission =
            || now < self.admission_deadline && self.authority.check_admission().is_ok();
        let allowed = !self.cancel.is_cancelled()
            && now < self.deadline
            && self.authority.check_session().is_ok()
            && if stage != QueryStage::Monitor {
                admission()
            } else {
                // Read cumulative consumption AFTER checking the clock gate.
                self.host.as_ref().is_some_and(|host| {
                    admission_state(admission, || host.invitation_consumed(self.lease_id)).is_some()
                })
            };
        #[cfg(test)]
        if stage == QueryStage::Monitor {
            if let Some(observation) = &self.monitor_observation {
                *observation.latest.lock().unwrap() = Some((now, allowed));
            }
        }
        allowed
    }
    pub(super) async fn interrupt(&mut self) {
        self.cancel.cancel();
        update(self.state, self.id, |a| {
            a.snapshot.phase = "closing";
            a.snapshot.cleanup_state = "pending";
            a.host = None;
            a.export = None;
        });
        if let Some(host) = self.host.take() {
            host.revoke(self.lease_id);
            host.shutdown().await;
            // The retained query contains only TargetGuard, never this owner.
            drop(host);
        }
    }
    pub(super) async fn recheck(
        &mut self,
        source: &TargetGuard,
        stage: QueryStage,
    ) -> (Result<(), ConnectError>, bool) {
        if !self.permitted(stage) {
            self.interrupt().await;
            return (Ok(()), true);
        }
        if matches!(source, TargetGuard::Manual) {
            return (Ok(()), false);
        }
        let source = source.clone();
        #[cfg(test)]
        let gate = lock(self.state).ok().and_then(|state| {
            state
                .attempt
                .as_ref()
                .filter(|a| a.snapshot.operation_id.as_deref() == Some(self.id))
                .and_then(|a| a.host_review.as_ref())
                .and_then(|work| match stage {
                    QueryStage::BeforeBind => work.hooks.before_bind.clone(),
                    QueryStage::AfterBind => work.hooks.after_bind.clone(),
                    QueryStage::Monitor => work.hooks.monitor.clone(),
                    QueryStage::Final => work.hooks.final_query.clone(),
                })
        });
        let mut query = tokio::task::spawn_blocking(move || {
            #[cfg(test)]
            if let Some(gate) = gate {
                gate.wait();
            }
            source.recheck().map(|_| ())
        });
        if stage == QueryStage::Final {
            return (query.await.unwrap_or(Err(ConnectError::Internal)), false);
        }
        let mut tick = tokio::time::interval(Duration::from_millis(50));
        loop {
            tokio::select! {
                biased;
                _ = self.cancel.cancelled() => break,
                _ = tokio::time::sleep_until(self.deadline.into()) => break,
                _ = tick.tick() => if !self.permitted(stage) { break; },
                result = &mut query => return (result.unwrap_or(Err(ConnectError::Internal)), false),
            }
        }
        // Revoke/close resources while the SAME query is retained and awaited.
        // A lost stop waiter cannot detach this work from its owning Host actor.
        self.interrupt().await;
        (query.await.unwrap_or(Err(ConnectError::Internal)), true)
    }
}
