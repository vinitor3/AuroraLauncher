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
pub(crate) struct DedicatedTestHooks(pub(super) Arc<test_support::TestHooks>);
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
    pub(crate) fn java_launcher_jar_unavailable(&self) -> bool {
        self.0.java_launcher_jar_unavailable()
    }
}

#[test]
fn original_stderr_classifier_is_bounded_and_handles_split_pipe_reads() {
    let expected = b"Error: Unable to access jarfile fixed-reviewed.jar\r\n";
    for split in 0..expected.len() {
        let hooks = DedicatedTestHooks::default();
        hooks.0.observe_launcher_stderr(&expected[..split]);
        hooks.0.observe_launcher_stderr(&expected[split..]);
        assert!(hooks.java_launcher_jar_unavailable());
        assert_eq!(
            hooks.0.launcher_stderr.lock().unwrap().0,
            b"Error: Unable to access jarfile ".len()
        );
    }
    for other in [
        b"Error: Unable to access jarfil".as_slice(),
        b"Error: Unable to access jarFILE ",
        b"prefix Error: Unable to access jarfile ",
    ] {
        let hooks = DedicatedTestHooks::default();
        hooks.0.observe_launcher_stderr(other);
        assert!(!hooks.java_launcher_jar_unavailable());
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

pub(crate) fn start_owned_voice_bridge_test(
    command: Command,
    instance_id: &str,
    limits: OwnedRunLimits,
    cancel: CancellationToken,
    hooks: DedicatedTestHooks,
) -> Result<OwnedRun, OwnedRunError> {
    worker::start_voice_bridge(command, instance_id, limits, cancel, hooks.0)
}
#[test]
fn reviewed_voice_lifetime_does_not_expand_original_owner_limits() {
    let limits = OwnedRunLimits {
        lifetime: Duration::from_secs(14_400),
        stop_grace: Duration::from_secs(5),
        output_bytes: 4 * 1024 * 1024,
    };
    assert!(!limits.valid());
    assert!(limits.valid_voice_bridge());
    assert!(!OwnedRunLimits {
        lifetime: Duration::from_secs(14_400) + Duration::from_nanos(1),
        ..limits
    }
    .valid_voice_bridge());
    assert!(!OwnedRunLimits {
        stop_grace: Duration::from_secs(6),
        ..limits
    }
    .valid_voice_bridge());
    assert!(OwnedRunLimits {
        lifetime: Duration::from_secs(3600),
        ..limits
    }
    .valid());
}

#[test]
fn native_v3_stop_grace_does_not_expand_legacy_or_original_lifetime_limits() {
    let reviewed = OwnedRunLimits {
        lifetime: Duration::from_secs(14_400),
        stop_grace: Duration::from_secs(30),
        output_bytes: 4 * 1024 * 1024,
    };
    assert!(reviewed.valid_native_server());
    assert!(!reviewed.valid_voice_bridge());
    assert!(!reviewed.valid());
    for invalid in [
        OwnedRunLimits {
            stop_grace: Duration::ZERO,
            ..reviewed
        },
        OwnedRunLimits {
            stop_grace: Duration::from_secs(30) + Duration::from_nanos(1),
            ..reviewed
        },
        OwnedRunLimits {
            lifetime: Duration::from_secs(14_400) + Duration::from_nanos(1),
            ..reviewed
        },
        OwnedRunLimits {
            lifetime: Duration::from_secs(29),
            ..reviewed
        },
        OwnedRunLimits {
            output_bytes: 0,
            ..reviewed
        },
    ] {
        assert!(!invalid.valid_native_server());
    }
    let original = OwnedRunLimits {
        stop_grace: Duration::from_secs(5),
        ..reviewed
    };
    assert!(original.valid_voice_bridge());
    assert!(!OwnedRunLimits {
        stop_grace: Duration::from_secs(5) + Duration::from_nanos(1),
        ..original
    }
    .valid_voice_bridge());
}
