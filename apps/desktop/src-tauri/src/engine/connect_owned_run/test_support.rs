//! Per-run deterministic seams. Absent from production builds; these never
//! replace Child identity, Win32 UDP results or process cleanup.
use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InitStage {
    Spawned,
    Proof,
    FirstReader,
    Readers,
}

#[derive(Default)]
pub(super) struct TestGate {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}
impl TestGate {
    pub(super) fn wait_reached(&self, timeout: Duration) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let (state, _) = self
            .changed
            .wait_timeout_while(state, timeout, |s| !s.0)
            .unwrap_or_else(|poison| poison.into_inner());
        state.0
    }
    pub(super) fn release(&self) {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .1 = true;
        self.changed.notify_all();
    }
    fn reach(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.0 = true;
        self.changed.notify_all();
        while !state.1 {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
    }
}

#[derive(Default)]
pub(super) struct TestHooks {
    pub(super) spawn_attempts: AtomicUsize,
    pub(super) query_calls: AtomicUsize,
    pub(super) child_reaped: AtomicBool,
    pub(super) readers_complete: AtomicBool,
    pub(super) before_query_gate: Option<Arc<TestGate>>,
    pub(super) after_positive_query_gate: Option<Arc<TestGate>>,
    pub(super) after_init_gate: Option<Arc<TestGate>>,
    pub(super) worker_gate: Option<Arc<TestGate>>,
    pub(super) before_stop_gate: Option<Arc<TestGate>>,
    pub(super) read_error_gate: Option<Arc<TestGate>>,
    pub(super) before_report_gate: Option<Arc<TestGate>>,
    pub(super) reader_final_gate: Option<Arc<TestGate>>,
    pub(super) after_exit_gate: Option<Arc<TestGate>>,
    pub(super) init_failure: Option<InitStage>,
    pub(super) init_panic: Option<InitStage>,
    pub(super) fail_worker_spawn: bool,
    pub(super) worker_panic: bool,
    pub(super) read_error_stdout: Option<bool>,
    pub(super) fail_stop_write: bool,
    pub(super) metadata: Mutex<Option<(String, String)>>,
}
impl TestHooks {
    pub(super) fn metadata_snapshot(&self) -> Option<(String, String)> {
        self.metadata
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }
    pub(super) fn before_spawn(&self) {
        self.spawn_attempts.fetch_add(1, Ordering::SeqCst);
    }
    pub(super) fn init(&self, stage: InitStage) -> Result<(), OwnedRunFailure> {
        assert_ne!(
            self.init_panic,
            Some(stage),
            "owned-run initialization seam"
        );
        if self.init_failure == Some(stage) {
            return Err(OwnedRunFailure::Initialization);
        }
        Ok(())
    }
    pub(super) fn metadata(&self, instance: &str, run: &str) {
        *self
            .metadata
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some((instance.into(), run.into()));
    }
    pub(super) fn before_query(&self) {
        if let Some(gate) = &self.before_query_gate {
            gate.reach();
        }
        self.query_calls.fetch_add(1, Ordering::SeqCst);
    }
    pub(super) fn after_query(&self, positive: bool) {
        if positive {
            if let Some(gate) = &self.after_positive_query_gate {
                gate.reach();
            }
        }
    }
    pub(super) fn after_init(&self) {
        if let Some(gate) = &self.after_init_gate {
            gate.reach();
        }
    }
    pub(super) fn worker_started(&self) {
        if let Some(gate) = &self.worker_gate {
            gate.reach();
        }
        assert!(!self.worker_panic, "owned-run worker seam");
    }
    pub(super) fn before_stop(&self) {
        if let Some(gate) = &self.before_stop_gate {
            gate.reach();
        }
    }
    pub(super) fn before_read_error(&self) {
        if let Some(gate) = &self.read_error_gate {
            gate.reach();
        }
    }
    pub(super) fn before_report(&self) {
        if let Some(gate) = &self.before_report_gate {
            gate.reach();
        }
    }
    pub(super) fn reader_final(&self) {
        if let Some(gate) = &self.reader_final_gate {
            gate.reach();
        }
    }
    pub(super) fn after_exit(&self) {
        if let Some(gate) = &self.after_exit_gate {
            gate.reach();
        }
    }
    pub(super) fn reaped(&self) {
        self.child_reaped.store(true, Ordering::SeqCst);
    }
    pub(super) fn readers_joined(&self) {
        self.readers_complete.store(true, Ordering::SeqCst);
    }
}

pub(super) fn start_with_hooks(
    command: Command,
    instance_id: &str,
    limits: OwnedRunLimits,
    cancel: CancellationToken,
    hooks: Arc<TestHooks>,
) -> Result<OwnedRun, OwnedRunError> {
    worker::start(command, instance_id, limits, cancel, hooks)
}
