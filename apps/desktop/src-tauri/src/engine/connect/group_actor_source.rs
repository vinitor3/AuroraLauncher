//! The group owns its source query and releases transport before awaiting it.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum Stage {
    BeforeBind,
    AfterBind,
    Monitor,
}

pub(super) struct ActorSource<'a> {
    pub(super) state: &'a Mutex<State>,
    pub(super) id: &'a str,
    pub(super) cancel: &'a CancellationToken,
    pub(super) invitations: &'a [lab::SessionInvitation],
    pub(super) host: Option<Arc<lab::Host>>,
}
impl ActorSource<'_> {
    /// Before publication the entire reviewed roster must still be admissible.
    /// This contains no State lock and is also checked under publication's lock.
    pub(super) fn strict_current(&self) -> bool {
        !self.cancel.is_cancelled()
            && self
                .invitations
                .iter()
                .all(|invitation| invitation.check_admission_current().is_ok())
    }
    pub(super) fn monitor_current(&self) -> bool {
        if self.cancel.is_cancelled() {
            return false;
        }
        lock(self.state)
            .map(|mut state| {
                state.attempt.as_mut().is_some_and(|a| {
                    a.snapshot.operation_id.as_deref() == Some(self.id)
                        && a.snapshot.phase == "hostListening"
                        && !refresh(a)
                })
            })
            .unwrap_or(false)
    }
    fn allowed(&self, stage: Stage) -> bool {
        match stage {
            Stage::BeforeBind | Stage::AfterBind => self.strict_current(),
            Stage::Monitor => self.monitor_current(),
        }
    }
    pub(super) async fn interrupt(&mut self) {
        self.cancel.cancel();
        if let Some(host) = &self.host {
            // First linearize against any last admission. The cumulative
            // observation below must not miss an admitted peer racing stop.
            for invitation in self.invitations {
                host.revoke(invitation.invitation_id());
            }
        }
        update(self.state, self.id, |a| {
            // Preserve consumed and EXPLICIT per-member revocation, then drop
            // State's Host clone. Global stop does not mark everyone revoked.
            finished(a);
            a.snapshot.phase = "closing";
            a.snapshot.cleanup_state = "pending";
        });
        if let Some(host) = self.host.take() {
            host.shutdown().await;
            drop(host);
        }
    }
    pub(super) async fn recheck(
        &mut self,
        source: &TargetGuard,
        stage: Stage,
    ) -> (Result<(), ConnectError>, bool) {
        if !self.allowed(stage) {
            self.interrupt().await;
            return (Ok(()), true);
        }
        if matches!(source, TargetGuard::Manual) {
            return (Ok(()), false);
        }
        let source = source.clone();
        #[cfg(test)]
        let (gate, panic_query) = lock(self.state)
            .ok()
            .and_then(|state| {
                state
                    .attempt
                    .as_ref()
                    .filter(|a| a.snapshot.operation_id.as_deref() == Some(self.id))
                    .and_then(|a| a.group_source.as_ref())
                    .map(|work| match stage {
                        Stage::BeforeBind => {
                            (work.hooks.before_bind.clone(), work.hooks.panic_before_bind)
                        }
                        Stage::AfterBind => {
                            (work.hooks.after_bind.clone(), work.hooks.panic_after_bind)
                        }
                        Stage::Monitor => (work.hooks.monitor.clone(), work.hooks.panic_monitor),
                    })
            })
            .unwrap_or_default();
        let mut query = tokio::task::spawn_blocking(move || {
            #[cfg(test)]
            {
                if let Some(gate) = gate {
                    gate.wait();
                }
                assert!(!panic_query, "injected group source query panic");
            }
            source.recheck().map(|_| ())
        });
        let mut tick = tokio::time::interval(Duration::from_millis(50));
        loop {
            tokio::select! {
                biased;
                _ = self.cancel.cancelled() => break,
                _ = tick.tick() => if !self.allowed(stage) { break; },
                result = &mut query => return (result.unwrap_or(Err(ConnectError::Internal)), false),
            }
        }
        // Neither the query nor a future wrapping it is dropped/aborted. The
        // same actor waits after ALL Host/Endpoint owners have been released.
        self.interrupt().await;
        (query.await.unwrap_or(Err(ConnectError::Internal)), true)
    }
}
