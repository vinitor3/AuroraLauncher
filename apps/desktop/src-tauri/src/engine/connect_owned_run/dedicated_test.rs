//! Closed test-only seams for the dedicated producer. These retain the real
//! supervisor, Child identity, pipes, query gate and original completion.
use super::*;
use std::sync::atomic::Ordering;

#[derive(Clone, Default)]
pub(crate) struct OwnerTestGate(Arc<test_support::TestGate>);
impl OwnerTestGate {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    pub(crate) fn wait_reached(&self, timeout: Duration) -> bool {
        self.0.wait_reached(timeout)
    }
    pub(crate) fn release(&self) {
        self.0.release();
    }
}

#[derive(Clone, Default)]
pub(crate) struct DedicatedTestHooks(Arc<test_support::TestHooks>);
impl DedicatedTestHooks {
    pub(crate) fn new(
        fail_init_after_spawn: bool,
        reader_final: Option<OwnerTestGate>,
        before_report: Option<OwnerTestGate>,
    ) -> Self {
        Self(Arc::new(test_support::TestHooks {
            init_failure: fail_init_after_spawn.then_some(test_support::InitStage::Spawned),
            reader_final_gate: reader_final.map(|gate| gate.0),
            before_report_gate: before_report.map(|gate| gate.0),
            ..test_support::TestHooks::default()
        }))
    }
    pub(crate) fn spawn_attempts(&self) -> usize {
        self.0.spawn_attempts.load(Ordering::SeqCst)
    }
}

pub(crate) fn start_owned_run_dedicated_test(
    command: Command,
    instance_id: &str,
    limits: OwnedRunLimits,
    cancel: CancellationToken,
    hooks: DedicatedTestHooks,
) -> Result<OwnedRun, OwnedRunError> {
    test_support::start_with_hooks(command, instance_id, limits, cancel, hooks.0)
}
