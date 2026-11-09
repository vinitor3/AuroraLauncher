//! Private child of EULA: this owner alone invokes supervisor83 and retains
//! its original completion. No report/boolean/caller completion grants restore.
use super::super::producer::*;
use super::*;
#[cfg(not(test))]
use crate::engine::connect_owned_run::{start_owned_run, start_owned_voice_bridge};
use crate::engine::connect_owned_run::{
    OwnedRun, OwnedRunCompletion, OwnedRunError, OwnedRunLimits, OwnedRunReport,
};
use aurora_connect_lab::{probe_minecraft_status, MinecraftStatusError, MinecraftStatusRequest};
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
};
use tokio::{runtime::Runtime, sync::watch};

enum Invocation {
    NeverCalled,
    MayHaveSpawned,
    Owned {
        control: Arc<OwnedRun>,
        completion: OwnedRunCompletion,
        runtime: Arc<Runtime>,
    },
    Completed(OwnedRunReport),
}

struct Worker {
    prepared: Option<factory::PreparedStatus>,
    applied: Option<AppliedEula>,
    approval: ExecutionApproval,
    invocation: Invocation,
    runtime: Option<Arc<Runtime>>,
    temp_pins: Vec<File>,
    shared: Arc<Shared>,
    hooks: ProducerHooks,
    sender: watch::Sender<Option<DedicatedReport>>,
    disposition: EulaDisposition,
    released: bool,
}

pub(in super::super) fn run(
    plan: DedicatedPlan,
    shared: Arc<Shared>,
    sender: watch::Sender<Option<DedicatedReport>>,
) {
    let (prepared, approval, hooks) = plan.into_parts();
    // Ownership is installed before runtime construction, mutation or launch.
    // It remains outside both catch scopes, including a failed initialization.
    let mut worker = Worker {
        prepared: Some(prepared),
        applied: None,
        approval,
        invocation: Invocation::NeverCalled,
        runtime: None,
        temp_pins: Vec::new(),
        shared,
        hooks,
        sender,
        disposition: EulaDisposition::Unchanged,
        released: false,
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| worker.drive()));
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(failure)) => worker.shared.fail(failure),
        Err(_) => worker.shared.fail(DedicatedFailure::Panic),
    }
    let cleanup = catch_unwind(AssertUnwindSafe(|| worker.finish()));
    if cleanup.is_err() {
        worker.shared.fail(DedicatedFailure::Panic);
        // No hooks and no further EULA writes after an interrupted finalizer.
        // Original process and query ownership are still collected first.
        worker.collect_original();
        worker.shared.wait_queries();
        if worker.applied.is_some() || !matches!(worker.invocation, Invocation::NeverCalled) {
            worker.disposition = EulaDisposition::NeedsRecovery;
        }
        worker.release();
        worker.publish();
    }
}

impl Worker {
    fn drive(&mut self) -> std::result::Result<(), DedicatedFailure> {
        #[cfg(test)]
        if self.hooks.fail_runtime {
            return Err(DedicatedFailure::Runtime);
        }
        self.runtime = Some(Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| DedicatedFailure::Runtime)?,
        ));
        self.hooks.at(ProducerPoint::BeforeApply);
        if !self.shared.open() {
            return Ok(());
        }
        let prepared = self.prepared.as_ref().ok_or(DedicatedFailure::Internal)?;
        self.approval
            .check(prepared)
            .map_err(DedicatedFailure::Preparation)?;
        self.temp_pins = self
            .approval
            .recipe
            .prepare_temp()
            .map_err(DedicatedFailure::Preparation)?;
        if !self.shared.open() {
            return Ok(());
        }
        // The same separate execution review is rechecked immediately before
        // Claim by apply_execution. File-only approval is never auto-promoted.
        let prepared = self.prepared.take().ok_or(DedicatedFailure::Internal)?;
        self.disposition = EulaDisposition::NeedsRecovery;
        self.applied =
            Some(apply_execution(prepared, &self.approval).map_err(DedicatedFailure::Preparation)?);
        self.hooks.at(ProducerPoint::AfterApply);
        if !self.shared.open() {
            return Ok(());
        }

        self.hooks.at(ProducerPoint::BeforeStart);
        if !self.shared.open() {
            return Ok(());
        }
        let applied = self.applied.as_mut().ok_or(DedicatedFailure::Internal)?;
        applied
            .prepared
            .resources
            .recheck()
            .map_err(DedicatedFailure::Preparation)?;
        applied
            .prepared
            .bundle
            .validate(&applied.prepared.guard)
            .map_err(DedicatedFailure::Preparation)?;
        // Time spent in preparation is never added back to supervisor83.
        let remaining = self
            .shared
            .deadline
            .saturating_duration_since(Instant::now());
        let stop_grace = Duration::from_millis(self.approval.recipe.limits.stop_ms);
        if remaining < stop_grace {
            return Err(DedicatedFailure::Deadline);
        }
        let command = self.approval.recipe.command();
        let runtime = self
            .runtime
            .as_ref()
            .cloned()
            .ok_or(DedicatedFailure::Internal)?;
        if !self.shared.open() {
            return Ok(());
        }
        self.approval
            .check_fresh()
            .map_err(DedicatedFailure::Preparation)?;
        applied
            .mark_may_have_spawned()
            .map_err(DedicatedFailure::Preparation)?;
        // The durable marker/checkpoint is another mutation boundary. Recheck
        // the original approval after it, outside the lifecycle mutex. A
        // refusal here keeps NeedsRecovery even though83 was never dispatched.
        applied
            .recheck_approved_launch_files()
            .map_err(DedicatedFailure::Preparation)?;
        // This transition precedes the call. Even an Err returned after83's
        // private cleanup never manufactures a completion or NeverCalled state.
        let limits = {
            let mut state = self.shared.lock();
            self.shared.refresh(&mut state);
            if state.reason.is_some() {
                return Ok(());
            }
            self.approval
                .check_fresh()
                .map_err(DedicatedFailure::Preparation)?;
            let remaining = self
                .shared
                .deadline
                .saturating_duration_since(Instant::now());
            if remaining < stop_grace {
                return Err(DedicatedFailure::Deadline);
            }
            self.invocation = Invocation::MayHaveSpawned;
            OwnedRunLimits {
                lifetime: remaining,
                stop_grace,
                output_bytes: self.approval.recipe.limits.output_bytes,
            }
        };
        let id = applied.prepared.bundle.copy.id().as_str();
        let voice_bridge = applied
            .prepared
            .bundle
            .manifest
            .voice_v2()
            .is_ok_and(|voice| {
                voice.observation_policy
                    == super::super::manifest::ObservationPolicyV2::LiteralLoopback35554Durable
            });
        #[cfg(not(test))]
        let result = if voice_bridge {
            start_owned_voice_bridge(command, id, limits, self.shared.child_cancel.clone())
        } else {
            start_owned_run(command, id, limits, self.shared.child_cancel.clone())
        };
        #[cfg(test)]
        let result = if voice_bridge {
            crate::engine::connect_owned_run::start_owned_voice_bridge_test(
                command,
                id,
                limits,
                self.shared.child_cancel.clone(),
                self.hooks.owned.clone(),
            )
        } else {
            crate::engine::connect_owned_run::start_owned_run_dedicated_test(
                command,
                id,
                limits,
                self.shared.child_cancel.clone(),
                self.hooks.owned.clone(),
            )
        };
        let run = Arc::new(result.map_err(DedicatedFailure::Spawn)?);
        let completion = run.completion();
        {
            let mut state = self.shared.lock();
            self.shared.refresh(&mut state);
            state.run = Arc::downgrade(&run);
            state.observer = Some(run.observer());
        }
        // Install the owner before a test gate or any subsequent fallible work.
        self.invocation = Invocation::Owned {
            control: run,
            completion,
            runtime,
        };
        if !self.shared.open() {
            self.request_original_stop();
        }
        self.hooks.at(ProducerPoint::AfterStart);
        if self.shared.open() {
            self.readiness()?;
        }
        self.wait_until_stopping();
        Ok(())
    }

    fn readiness(&self) -> std::result::Result<(), DedicatedFailure> {
        let runtime = self.runtime.as_ref().ok_or(DedicatedFailure::Internal)?;
        for _ in 0..self.approval.recipe.limits.attempts {
            if !self.shared.open() {
                return Ok(());
            }
            if Instant::now() >= self.shared.readiness_deadline {
                return Err(DedicatedFailure::Readiness);
            }
            self.hooks.at(ProducerPoint::BeforeReserve);
            let job = match self.shared.reserve(QueryKind::Readiness) {
                Ok(job) => job,
                Err(DedicatedError::Closed) if !self.shared.open() => return Ok(()),
                Err(_) => return Err(DedicatedFailure::Source),
            };
            self.hooks.at(ProducerPoint::BeforeQuery);
            let before = job.observer.observe_tcp(ENDPOINT);
            if before.is_ok() {
                self.hooks.at(ProducerPoint::AfterQuery);
            }
            if !self.shared.open() {
                return Ok(());
            }
            if let Err(error) = before {
                drop(job);
                if matches!(
                    error,
                    OwnedRunError::Discovery(
                        crate::engine::connect_discovery::DiscoveryError::ListenerChanged
                    )
                ) {
                    self.pause_attempt(runtime);
                    continue;
                }
                return Err(DedicatedFailure::Source);
            }
            let timeout = self
                .shared
                .readiness_deadline
                .saturating_duration_since(Instant::now())
                .min(
                    self.shared
                        .deadline
                        .saturating_duration_since(Instant::now()),
                )
                .min(Duration::from_secs(5));
            if timeout.is_zero() {
                return Err(DedicatedFailure::Readiness);
            }
            let status = runtime.block_on(async {
                tokio::select! {
                    biased;
                    _ = self.shared.cancel.cancelled() => {
                        self.shared.request(DedicatedReason::Cancelled, Some(DedicatedFailure::Cancelled));
                        None
                    },
                    _ = self.shared.status_cancel.cancelled() => None,
                    result = probe_minecraft_status(MinecraftStatusRequest {
                        endpoint: ENDPOINT,
                        handshake_host: "127.0.0.1".into(),
                        handshake_port: 43001,
                        protocol: PROTOCOL,
                        timeout,
                    }) => Some(result),
                }
            });
            let Some(status) = status else {
                return Ok(());
            };
            match status {
                Ok(summary)
                    if summary.protocol_matches_requested
                        && summary.requested_protocol == PROTOCOL
                        && summary.reported_protocol == PROTOCOL => {}
                Ok(_) => return Err(DedicatedFailure::Protocol),
                Err(MinecraftStatusError::ConnectionFailed | MinecraftStatusError::Truncated) => {
                    drop(job);
                    self.pause_attempt(runtime);
                    continue;
                }
                Err(MinecraftStatusError::Timeout) => return Err(DedicatedFailure::Readiness),
                Err(error) => return Err(DedicatedFailure::Status(error)),
            }
            self.hooks.at(ProducerPoint::AfterStatus);
            if !self.shared.open() {
                return Ok(());
            }
            self.hooks.at(ProducerPoint::BeforeQuery);
            let after = job.observer.observe_tcp(ENDPOINT);
            if after.is_ok() {
                self.hooks.at(ProducerPoint::AfterQuery);
            }
            if !self.shared.open() {
                return Ok(());
            }
            after.map_err(|_| DedicatedFailure::Source)?;
            self.hooks.at(ProducerPoint::BeforeReady);
            let mut state = self.shared.lock();
            self.shared.refresh(&mut state);
            if state.reason.is_some() {
                return Ok(());
            }
            if Instant::now() >= self.shared.readiness_deadline {
                return Err(DedicatedFailure::Readiness);
            }
            state.phase = DedicatedPhase::Ready;
            state.reached_ready = true;
            self.shared.changed.notify_all();
            return Ok(());
        }
        Err(DedicatedFailure::Readiness)
    }

    fn pause_attempt(&self, runtime: &Runtime) {
        runtime.block_on(async {
            tokio::select! {
                _ = self.shared.cancel.cancelled() => {
                    self.shared.request(DedicatedReason::Cancelled, Some(DedicatedFailure::Cancelled));
                },
                _ = self.shared.status_cancel.cancelled() => {},
                _ = tokio::time::sleep(Duration::from_millis(1000)) => {},
            }
        });
    }

    fn wait_until_stopping(&mut self) {
        let Invocation::Owned { completion, .. } = &self.invocation else {
            return;
        };
        let Some(runtime) = self.runtime.as_ref() else {
            return;
        };
        let result = runtime.block_on(async {
            tokio::select! {
                biased;
                _ = self.shared.cancel.cancelled() => {
                    self.shared.request(DedicatedReason::Cancelled, Some(DedicatedFailure::Cancelled));
                    None
                },
                _ = self.shared.status_cancel.cancelled() => None,
                report = completion.wait() => Some(report),
            }
        });
        if let Some(report) = result {
            let mut state = self.shared.lock();
            self.shared.refresh(&mut state);
            state.seal(
                DedicatedReason::Exited,
                report.failure.map(DedicatedFailure::Child),
            );
            self.shared.status_cancel.cancel();
            drop(state);
            self.invocation = Invocation::Completed(report);
        }
    }

    fn request_original_stop(&self) {
        if let Invocation::Owned { control, .. } = &self.invocation {
            control.request_stop();
        }
    }
    fn collect_original(&mut self) {
        self.request_original_stop();
        if let Invocation::Owned {
            completion,
            runtime,
            ..
        } = &self.invocation
        {
            // Runtime ownership is part of the Owned variant, as is the
            // completion obtained from this exact control.
            let report = runtime.block_on(completion.wait());
            if let Some(failure) = report.failure {
                let mut state = self.shared.lock();
                state.seal(
                    DedicatedReason::Failure,
                    Some(DedicatedFailure::Child(failure)),
                );
            }
            self.invocation = Invocation::Completed(report);
        }
    }

    fn finish(&mut self) {
        // Seal before waiting: no new source observations can acquire a job.
        {
            let mut state = self.shared.lock();
            self.shared.refresh(&mut state);
            if state.reason.is_none() {
                state.seal(DedicatedReason::Failure, Some(DedicatedFailure::Internal));
            }
            self.shared.status_cancel.cancel();
        }
        self.collect_original();
        self.shared.wait_queries();
        self.hooks.at(ProducerPoint::BeforeRestore);
        if let Some(applied) = self.applied.as_ref() {
            let state = read_state(&applied.prepared.bundle);
            let original_claim = state.as_ref().is_ok_and(|state| {
                state.claim.transaction == applied.transaction
                    && state.claim.review_digest == self.approval.review.digest
                    && applied.reviewed_digest == self.approval.review.digest
            });
            let authority = match &self.invocation {
                Invocation::NeverCalled if !applied.may_have_spawned => {
                    Some(Authority::NeverSpawned(&applied.prepared.guard))
                }
                Invocation::Completed(report) if report.reaped && report.readers_complete => {
                    Some(Authority::CompletedOriginalRun(&applied.prepared.guard))
                }
                _ => None,
            };
            if let Some(authority) = authority.filter(|_| original_claim) {
                // Private descendant access, after observing the ORIGINAL
                // completion and all84 jobs. No external report enters here.
                match restore(&applied.prepared.bundle, authority, &applied.prepared.hooks) {
                    Ok(EulaOutcome::Restored) => self.disposition = EulaDisposition::Restored,
                    Ok(EulaOutcome::Unchanged) => self.disposition = EulaDisposition::Unchanged,
                    Err(error) => {
                        self.disposition = EulaDisposition::NeedsRecovery;
                        self.shared.fail(DedicatedFailure::Preparation(error));
                    }
                }
            } else {
                self.disposition = EulaDisposition::NeedsRecovery;
                if !original_claim {
                    self.shared
                        .fail(DedicatedFailure::Preparation(Error::Changed));
                } else {
                    self.shared
                        .fail(DedicatedFailure::Preparation(Error::NeedsRecovery));
                }
            }
        }
        self.hooks.at(ProducerPoint::BeforePublish);
        self.release();
        self.publish();
    }

    fn release(&mut self) {
        self.shared.wait_queries();
        {
            let mut state = self.shared.lock();
            state.run = std::sync::Weak::new();
            state.observer = None;
        }
        drop(self.applied.take());
        drop(self.prepared.take());
        self.temp_pins.clear();
        // No STATUS tasks are spawned; dropping the owned runtime closes its
        // driver only after probe futures/sockets and original completion ended.
        drop(self.runtime.take());
        self.released = true;
    }
    fn publish(&self) {
        let mut state = self.shared.lock();
        self.shared.refresh(&mut state);
        let report = DedicatedReport {
            reason: state.reason.unwrap_or(DedicatedReason::Failure),
            failure: state.failure,
            process: match &self.invocation {
                Invocation::Completed(report) => Some(report.clone()),
                _ => None,
            },
            eula: self.disposition,
            reached_ready: state.reached_ready,
        };
        state.phase = DedicatedPhase::Complete;
        self.sender.send_replace(Some(report));
        self.shared.changed.notify_all();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if !self.released {
            self.shared.fail(DedicatedFailure::Panic);
            self.collect_original();
            self.shared.wait_queries();
            // Unwind never writes EULA. Keep its durable recovery evidence.
            self.release();
        }
    }
}
