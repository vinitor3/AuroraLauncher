use super::super::test_support::{start_with_hooks, InitStage, TestGate, TestHooks};
use super::*;
use std::sync::atomic::Ordering;

fn hook_cleanup(hooks: &TestHooks) {
    assert_eq!(hooks.spawn_attempts.load(Ordering::SeqCst), 1);
    assert!(
        hooks.child_reaped.load(Ordering::SeqCst),
        "actual_child_reaped"
    );
    assert!(
        hooks.readers_complete.load(Ordering::SeqCst),
        "actual_readers_joined"
    );
}

async fn await_hook_cleanup(hooks: &TestHooks) {
    tokio::time::timeout(WAIT, async {
        while !hooks.child_reaped.load(Ordering::SeqCst)
            || !hooks.readers_complete.load(Ordering::SeqCst)
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("process_cleanup_before_query_release");
}

#[tokio::test]
async fn invalid_inputs_and_precancel_have_causal_zero_spawn_attempts() {
    let valid = limits();
    let invalid = [
        OwnedRunLimits {
            lifetime: Duration::ZERO,
            ..valid
        },
        OwnedRunLimits {
            lifetime: Duration::from_secs(3601),
            ..valid
        },
        OwnedRunLimits {
            stop_grace: Duration::ZERO,
            ..valid
        },
        OwnedRunLimits {
            stop_grace: Duration::from_secs(6),
            ..valid
        },
        OwnedRunLimits {
            lifetime: Duration::from_secs(1),
            stop_grace: Duration::from_secs(2),
            ..valid
        },
        OwnedRunLimits {
            output_bytes: 0,
            ..valid
        },
        OwnedRunLimits {
            output_bytes: 16 * 1024 * 1024 + 1,
            ..valid
        },
    ];
    for bounded in invalid {
        let harness = Harness::new();
        let hooks = Arc::new(TestHooks::default());
        let result = start_with_hooks(
            harness.command("normal"),
            "valid-instance",
            bounded,
            CancellationToken::new(),
            hooks.clone(),
        );
        assert!(matches!(result, Err(OwnedRunError::InvalidRequest)));
        assert_eq!(hooks.spawn_attempts.load(Ordering::SeqCst), 0);
        harness.assert_no_child_connection();
        harness.finish();
    }
    for instance in [String::new(), "x".repeat(129), "á".repeat(65)] {
        let harness = Harness::new();
        let hooks = Arc::new(TestHooks::default());
        let result = start_with_hooks(
            harness.command("normal"),
            &instance,
            valid,
            CancellationToken::new(),
            hooks.clone(),
        );
        assert!(matches!(result, Err(OwnedRunError::InvalidRequest)));
        assert_eq!(hooks.spawn_attempts.load(Ordering::SeqCst), 0);
        harness.assert_no_child_connection();
        harness.finish();
    }
    let mut harness = Harness::new();
    let hooks = Arc::new(TestHooks::default());
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        start_with_hooks(
            harness.command("normal"),
            "valid-instance",
            valid,
            cancelled,
            hooks.clone()
        ),
        Err(OwnedRunError::Cancelled)
    ));
    assert_eq!(hooks.spawn_attempts.load(Ordering::SeqCst), 0);
    harness.assert_no_child_connection();
    // The same hook and executable must count a real, admitted spawn. This
    // anchors the negative count to the actual Command::spawn boundary.
    let run = start_with_hooks(
        harness.command("normal"),
        &"x".repeat(128),
        valid,
        CancellationToken::new(),
        hooks.clone(),
    )
    .unwrap_or_else(|_| panic!("validation_positive_control"));
    let bindings = harness.ready();
    assert_eq!(hooks.spawn_attempts.load(Ordering::SeqCst), 1);
    bindings.exchange_bytes();
    normal(&report(&run.request_stop()).await);
    hook_cleanup(&hooks);
    bindings.assert_released();
    harness.finish();
}

#[tokio::test]
async fn native_run_uuid_and_trusted_instance_labels_remain_unchanged() {
    let mut one = Harness::new();
    let first_hooks = Arc::new(TestHooks::default());
    let first = start_with_hooks(
        one.command("normal"),
        "shared-label",
        limits(),
        CancellationToken::new(),
        first_hooks.clone(),
    )
    .unwrap_or_else(|_| panic!("metadata_first_start"));
    let first_bindings = one.ready();
    let before = first_hooks.metadata_snapshot().expect("native_metadata");
    assert_eq!(before.0, "shared-label");
    assert!(uuid::Uuid::parse_str(&before.1).is_ok(), "native_run_uuid");
    let mut two = Harness::new();
    let second_hooks = Arc::new(TestHooks::default());
    let second = start_with_hooks(
        two.command("normal"),
        "shared-label",
        limits(),
        CancellationToken::new(),
        second_hooks.clone(),
    )
    .unwrap_or_else(|_| panic!("metadata_second_start"));
    let second_bindings = two.ready();
    let other = second_hooks
        .metadata_snapshot()
        .expect("second_native_metadata");
    assert_eq!(other.0, before.0);
    assert!(uuid::Uuid::parse_str(&other.1).is_ok());
    assert!(other.1 != before.1, "fresh_native_run_for_each_child");
    let observer = first.observer();
    for endpoint in first_bindings.udp() {
        assert_eq!(observer.clone().observe_udp(endpoint), Ok(()));
    }
    assert!(first_hooks.metadata_snapshot().as_ref() == Some(&before));
    normal(&report(&first.request_stop()).await);
    normal(&report(&second.request_stop()).await);
    assert!(first_hooks.metadata_snapshot().as_ref() == Some(&before));
    first_bindings.assert_released();
    second_bindings.assert_released();
    one.finish();
    two.finish();
}

#[test]
fn every_partial_initialization_failure_and_panic_reaps_before_returning_error() {
    for stage in [
        InitStage::Spawned,
        InitStage::Proof,
        InitStage::FirstReader,
        InitStage::Readers,
    ] {
        for panic in [false, true] {
            let harness = Harness::new();
            let hooks = Arc::new(TestHooks {
                init_failure: (!panic).then_some(stage),
                init_panic: panic.then_some(stage),
                ..TestHooks::default()
            });
            let result = start_with_hooks(
                harness.command("normal"),
                "init-instance",
                limits(),
                CancellationToken::new(),
                hooks.clone(),
            );
            let expected = if panic {
                OwnedRunError::Internal
            } else {
                OwnedRunError::Initialization
            };
            assert!(matches!(result, Err(error) if error == expected));
            // These flags are emitted only after real Child::wait/try_wait and
            // joins of all readers created before this exact fault stage.
            hook_cleanup(&hooks);
            harness.finish();
        }
    }
}

#[tokio::test]
async fn worker_creation_failure_after_real_readiness_still_collects_child_and_pipes() {
    let gate = Arc::new(TestGate::default());
    let _release = ReleaseGate(gate.clone());
    let hooks = Arc::new(TestHooks {
        after_init_gate: Some(gate.clone()),
        fail_worker_spawn: true,
        ..TestHooks::default()
    });
    let mut harness = Harness::new();
    let command = harness.command("normal");
    let work_hooks = hooks.clone();
    let work = tokio::task::spawn_blocking(move || {
        start_with_hooks(
            command,
            "worker-create-instance",
            limits(),
            CancellationToken::new(),
            work_hooks,
        )
    });
    assert!(gate.wait_reached(WAIT));
    let bindings = harness.ready();
    bindings.exchange_bytes();
    gate.release();
    let result = tokio::time::timeout(WAIT, work)
        .await
        .expect("worker_create_cleanup_deadline")
        .expect("worker_create_task");
    assert!(matches!(result, Err(OwnedRunError::Initialization)));
    hook_cleanup(&hooks);
    bindings.assert_released();
    harness.finish();
}

#[tokio::test]
async fn cancellation_and_deadline_during_initialization_are_rechecked_before_start_returns() {
    for deadline_case in [false, true] {
        let gate = Arc::new(TestGate::default());
        let _release = ReleaseGate(gate.clone());
        let hooks = Arc::new(TestHooks {
            after_init_gate: Some(gate.clone()),
            ..TestHooks::default()
        });
        let cancel = CancellationToken::new();
        let work_cancel = cancel.clone();
        let mut bounded = limits();
        if deadline_case {
            bounded.lifetime = Duration::from_millis(500);
            bounded.stop_grace = Duration::from_millis(100);
        }
        let mut harness = Harness::new();
        let command = harness.command("normal");
        let work_hooks = hooks.clone();
        let work = tokio::task::spawn_blocking(move || {
            start_with_hooks(
                command,
                "slow-init-instance",
                bounded,
                work_cancel,
                work_hooks,
            )
        });
        assert!(gate.wait_reached(WAIT));
        let bindings = harness.ready();
        if deadline_case {
            tokio::time::sleep(bounded.lifetime + Duration::from_millis(20)).await;
        } else {
            cancel.cancel();
        }
        gate.release();
        let result = tokio::time::timeout(WAIT, work)
            .await
            .expect("initialization_cleanup_deadline")
            .expect("initialization_task");
        let expected = if deadline_case {
            OwnedRunError::Deadline
        } else {
            OwnedRunError::Cancelled
        };
        assert!(matches!(result, Err(error) if error == expected));
        hook_cleanup(&hooks);
        bindings.assert_released();
        harness.finish();
    }
}

#[tokio::test]
async fn reader_fault_keeps_draining_and_joins_an_independent_real_reader() {
    for stdout_fault in [false, true] {
        let gate = Arc::new(TestGate::default());
        let _release = ReleaseGate(gate.clone());
        let hooks = Arc::new(TestHooks {
            read_error_gate: Some(gate.clone()),
            read_error_stdout: Some(stdout_fault),
            ..TestHooks::default()
        });
        let mut harness = Harness::new();
        let run = start_with_hooks(
            harness.command("normal"),
            "reader-fault-instance",
            limits(),
            CancellationToken::new(),
            hooks.clone(),
        )
        .unwrap_or_else(|_| panic!("reader_fault_start"));
        assert!(gate.wait_reached(WAIT));
        let bindings = harness.ready();
        harness.send(if stdout_fault {
            Action::FloodErr
        } else {
            Action::FloodOut
        });
        tokio::time::timeout(WAIT, async {
            loop {
                let drained = {
                    let state = run.shared.lock();
                    if stdout_fault {
                        state.stderr_bytes
                    } else {
                        state.stdout_bytes
                    }
                };
                if drained >= 80 * 1024 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("independent_reader_drains_real_bytes");
        gate.release();
        let result = report(&run.completion()).await;
        clean(&result);
        assert_eq!(result.reason, OwnedRunReason::Failure);
        assert_eq!(result.failure, Some(OwnedRunFailure::Read));
        assert_eq!(result.exit_code, Some(0));
        assert!(!result.forced);
        hook_cleanup(&hooks);
        bindings.assert_released();
        harness.finish();
    }
}

#[tokio::test]
async fn stop_pipe_error_is_not_hidden_by_later_process_exit() {
    let hooks = Arc::new(TestHooks {
        fail_stop_write: true,
        ..TestHooks::default()
    });
    let mut harness = Harness::new();
    let run = start_with_hooks(
        harness.command("normal"),
        "pipe-error-instance",
        limits(),
        CancellationToken::new(),
        hooks.clone(),
    )
    .unwrap_or_else(|_| panic!("pipe_error_start"));
    let bindings = harness.ready();
    let result = report(&run.request_stop()).await;
    clean(&result);
    assert_eq!(result.reason, OwnedRunReason::Requested);
    assert_eq!(result.failure, Some(OwnedRunFailure::Write));
    hook_cleanup(&hooks);
    bindings.assert_released();
    harness.finish();
}

#[tokio::test]
async fn worker_panic_keeps_cleanup_ownership_of_real_child_and_readers() {
    let gate = Arc::new(TestGate::default());
    let _release = ReleaseGate(gate.clone());
    let hooks = Arc::new(TestHooks {
        worker_gate: Some(gate.clone()),
        worker_panic: true,
        ..TestHooks::default()
    });
    let mut harness = Harness::new();
    let run = start_with_hooks(
        harness.command("normal"),
        "worker-panic-instance",
        limits(),
        CancellationToken::new(),
        hooks.clone(),
    )
    .unwrap_or_else(|_| panic!("worker_panic_start"));
    assert!(gate.wait_reached(WAIT));
    let bindings = harness.ready();
    bindings.exchange_bytes();
    assert_eq!(run.observer().observe_udp(bindings.udp4), Ok(()));
    gate.release();
    let result = report(&run.completion()).await;
    clean(&result);
    assert_eq!(result.reason, OwnedRunReason::Failure);
    assert_eq!(result.failure, Some(OwnedRunFailure::Panic));
    hook_cleanup(&hooks);
    bindings.assert_released();
    harness.finish();
}

async fn retained_query(after_positive: bool) {
    let gate = Arc::new(TestGate::default());
    let _release = ReleaseGate(gate.clone());
    let hooks = Arc::new(TestHooks {
        before_query_gate: (!after_positive).then(|| gate.clone()),
        after_positive_query_gate: after_positive.then(|| gate.clone()),
        ..TestHooks::default()
    });
    let mut harness = Harness::new();
    let run = start_with_hooks(
        harness.command("normal"),
        "query-held-instance",
        limits(),
        CancellationToken::new(),
        hooks.clone(),
    )
    .unwrap_or_else(|_| panic!("query_held_start"));
    let bindings = harness.ready();
    bindings.exchange_bytes();
    let observer = run.observer();
    let query_observer = observer.clone();
    let endpoint = bindings.udp4;
    let query = tokio::task::spawn_blocking(move || query_observer.observe_udp(endpoint));
    assert!(
        gate.wait_reached(WAIT),
        "real_registered_query_reaches_gate"
    );
    let entered = hooks.query_calls.load(Ordering::SeqCst);
    assert_eq!(entered, usize::from(after_positive));
    assert_eq!(
        observer.observe_udp(bindings.udp6),
        Err(OwnedRunError::Busy)
    );
    assert_eq!(hooks.query_calls.load(Ordering::SeqCst), entered);
    let completion = run.request_stop();
    assert_eq!(
        observer.observe_udp(bindings.udp6),
        Err(OwnedRunError::Closed)
    );
    assert_eq!(hooks.query_calls.load(Ordering::SeqCst), entered);
    harness.stop_observed();
    await_hook_cleanup(&hooks).await;
    bindings.assert_released();
    // The original process and real pipes are gone while the SAME query owns
    // its registered work. Completion must still await that work's release.
    pending(&completion).await;
    gate.release();
    assert_eq!(
        tokio::time::timeout(WAIT, query)
            .await
            .expect("query_release_deadline")
            .expect("query_worker"),
        Err(OwnedRunError::Closed)
    );
    normal(&report(&completion).await);
    assert_eq!(hooks.query_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        observer.observe_udp(bindings.udp4),
        Err(OwnedRunError::Closed)
    );
    assert_eq!(hooks.query_calls.load(Ordering::SeqCst), 1);
    bindings.assert_released();
    harness.finish();
}

#[tokio::test]
async fn registered_query_before_win32_does_not_delay_reap_but_holds_completion() {
    retained_query(false).await;
}

#[tokio::test]
async fn positive_real_query_held_before_final_gate_cannot_succeed_after_stop() {
    retained_query(true).await;
}

async fn final_report_freshness(deadline_case: bool) {
    let gate = Arc::new(TestGate::default());
    let _release = ReleaseGate(gate.clone());
    let hooks = Arc::new(TestHooks {
        before_report_gate: Some(gate.clone()),
        ..TestHooks::default()
    });
    let cancel = CancellationToken::new();
    let mut bounded = limits();
    if deadline_case {
        bounded.lifetime = Duration::from_secs(1);
        bounded.stop_grace = Duration::from_millis(200);
    }
    let mut harness = Harness::new();
    let run = start_with_hooks(
        harness.command("normal"),
        "final-report-instance",
        bounded,
        cancel.clone(),
        hooks.clone(),
    )
    .unwrap_or_else(|_| panic!("final_report_start"));
    let bindings = harness.ready();
    let completion = run.request_stop();
    harness.stop_observed();
    assert!(gate.wait_reached(WAIT));
    hook_cleanup(&hooks);
    bindings.assert_released();
    assert_eq!(hooks.query_calls.load(Ordering::SeqCst), 0);
    pending(&completion).await;
    if deadline_case {
        // Merely observe the original monotonic deadline; no private clock or
        // query refresh can install the failure on behalf of final publication.
        while Instant::now() < run.shared.deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    } else {
        cancel.cancel();
    }
    gate.release();
    let result = report(&completion).await;
    clean(&result);
    assert_eq!(result.reason, OwnedRunReason::Requested);
    assert_eq!(
        result.failure,
        Some(if deadline_case {
            OwnedRunFailure::Deadline
        } else {
            OwnedRunFailure::Cancelled
        })
    );
    assert_eq!(result.exit_code, Some(0));
    assert!(!result.forced);
    assert_eq!(hooks.query_calls.load(Ordering::SeqCst), 0);
    bindings.assert_released();
    harness.finish();
}

#[tokio::test]
async fn cancel_after_reap_readers_and_queries_is_rechecked_at_final_publication() {
    final_report_freshness(false).await;
}

#[tokio::test]
async fn deadline_after_reap_readers_and_queries_is_rechecked_at_final_publication() {
    final_report_freshness(true).await;
}
