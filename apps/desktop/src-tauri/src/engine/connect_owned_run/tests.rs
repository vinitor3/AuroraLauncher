use super::*;
use std::time::Duration;

mod fixture;
mod lifecycle_gates;
mod tcp_queries;
#[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
mod udp_loopback_fixture;
#[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
mod udp_loopback_queries;
use fixture::{Action, Harness, WAIT};

struct ReleaseGate(Arc<test_support::TestGate>);
impl Drop for ReleaseGate {
    fn drop(&mut self) {
        self.0.release();
    }
}

fn limits() -> OwnedRunLimits {
    OwnedRunLimits {
        lifetime: Duration::from_secs(10),
        stop_grace: Duration::from_secs(2),
        output_bytes: 1024 * 1024,
    }
}

fn start(harness: &Harness, mode: &str, cancel: CancellationToken) -> OwnedRun {
    start_owned_run(
        harness.command(mode),
        "owned-run-test-instance",
        limits(),
        cancel,
    )
    .unwrap_or_else(|_| panic!("owned_run_start"))
}

async fn report(completion: &OwnedRunCompletion) -> OwnedRunReport {
    tokio::time::timeout(WAIT, completion.wait())
        .await
        .expect("owned_run_completion_deadline")
}

fn clean(report: &OwnedRunReport) {
    assert!(report.reaped, "owned_run_reaped");
    assert!(report.readers_complete, "owned_run_readers_joined");
}

fn normal(report: &OwnedRunReport) {
    clean(report);
    assert_eq!(report.reason, OwnedRunReason::Requested);
    assert_eq!(report.failure, None);
    assert_eq!(report.exit_code, Some(0));
    assert!(!report.forced);
}

async fn pending(completion: &OwnedRunCompletion) {
    assert!(
        tokio::time::timeout(Duration::from_millis(30), completion.wait())
            .await
            .is_err(),
        "owned_run_cleanup_still_pending"
    );
}

#[tokio::test]
async fn real_tcp_udp_ipv4_ipv6_bytes_same_original_and_idempotent_stop() {
    let mut harness = Harness::new();
    let run = start(&harness, "normal", CancellationToken::new());
    let bindings = harness.ready();
    let observer = run.observer();
    bindings.exchange_bytes();
    for endpoint in bindings.udp() {
        assert_eq!(observer.observe_udp(endpoint), Ok(()));
    }
    // UDP82 is a current observation: closing and rebinding in this same live
    // process is allowed before the run lifecycle is sealed.
    harness.release_udp();
    for endpoint in bindings.udp() {
        assert_eq!(
            observer.observe_udp(endpoint),
            Err(OwnedRunError::Discovery(DiscoveryError::ListenerChanged))
        );
    }
    harness.rebind_udp();
    for endpoint in bindings.udp() {
        assert_eq!(observer.observe_udp(endpoint), Ok(()));
    }
    let first = run.request_stop();
    let second = run.request_stop();
    harness.stop_observed();
    let result = report(&first).await;
    normal(&result);
    assert_eq!(result, report(&second).await);
    assert_eq!(result, report(&run.request_stop()).await);
    for endpoint in bindings.udp() {
        assert_eq!(observer.observe_udp(endpoint), Err(OwnedRunError::Closed));
    }
    bindings.assert_released();
    drop(run);
    assert_eq!(result, report(&first).await);
    harness.finish();
}

#[tokio::test]
async fn foreign_child_and_exact_replacement_never_reactivate_old_observer() {
    let mut first = Harness::new();
    let original = start(&first, "normal", CancellationToken::new());
    let endpoints = first.ready();
    let old = original.observer();
    let mut second = Harness::new();
    let foreign = start(&second, "normal", CancellationToken::new());
    let other = second.ready();
    let other_observer = foreign.observer();
    for endpoint in endpoints.udp() {
        assert_eq!(old.observe_udp(endpoint), Ok(()));
        assert_eq!(
            other_observer.observe_udp(endpoint),
            Err(OwnedRunError::Discovery(DiscoveryError::ListenerChanged))
        );
    }
    for endpoint in other.udp() {
        assert_eq!(other_observer.observe_udp(endpoint), Ok(()));
        assert_eq!(
            old.observe_udp(endpoint),
            Err(OwnedRunError::Discovery(DiscoveryError::ListenerChanged))
        );
    }
    normal(&report(&original.request_stop()).await);
    endpoints.assert_released();
    let mut replacement = Harness::at(endpoints.clone());
    let replacement_run = start(&replacement, "normal", CancellationToken::new());
    let rebound = replacement.ready();
    for endpoint in rebound.udp() {
        assert_eq!(old.observe_udp(endpoint), Err(OwnedRunError::Closed));
        assert_eq!(replacement_run.observer().observe_udp(endpoint), Ok(()));
    }
    other.exchange_bytes();
    normal(&report(&replacement_run.request_stop()).await);
    normal(&report(&foreign.request_stop()).await);
    rebound.assert_released();
    other.assert_released();
    first.finish();
    second.finish();
    replacement.finish();
}

#[tokio::test]
async fn drop_control_stops_child_while_weak_observers_and_completion_survive() {
    let mut first = Harness::new();
    let run = start(&first, "normal", CancellationToken::new());
    let bindings = first.ready();
    let observer = run.observer();
    let observer_copy = observer.clone();
    let completion = run.completion();
    let completion_copy = completion.clone();
    let mut second = Harness::new();
    let unrelated = start(&second, "normal", CancellationToken::new());
    let other = second.ready();
    drop(run);
    first.stop_observed();
    let result = report(&completion).await;
    clean(&result);
    assert_eq!(result.reason, OwnedRunReason::Dropped);
    assert_eq!(result.failure, Some(OwnedRunFailure::Dropped));
    assert_eq!(result.exit_code, Some(0));
    assert!(!result.forced);
    assert_eq!(result, report(&completion_copy).await);
    assert_eq!(
        observer.observe_udp(bindings.udp4),
        Err(OwnedRunError::Closed)
    );
    assert_eq!(
        observer_copy.observe_udp(bindings.udp6),
        Err(OwnedRunError::Closed)
    );
    bindings.assert_released();
    other.exchange_bytes();
    normal(&report(&unrelated.request_stop()).await);
    other.assert_released();
    first.finish();
    second.finish();
}

#[tokio::test]
async fn external_cancel_seals_observations_and_collects_the_same_child() {
    let cancel = CancellationToken::new();
    let mut harness = Harness::new();
    let run = start(&harness, "normal", cancel.clone());
    let bindings = harness.ready();
    let completion = run.completion();
    cancel.cancel();
    assert_eq!(
        run.observer().observe_udp(bindings.udp4),
        Err(OwnedRunError::Closed)
    );
    let result = report(&completion).await;
    clean(&result);
    assert_eq!(result.reason, OwnedRunReason::Cancelled);
    assert_eq!(result.failure, Some(OwnedRunFailure::Cancelled));
    assert_eq!(result.exit_code, Some(0));
    assert!(!result.forced);
    assert_eq!(result, report(&run.request_stop()).await);
    bindings.assert_released();
    harness.finish();
}

#[tokio::test]
async fn abandoned_first_waiter_and_repeated_stop_keep_original_cleanup() {
    let mut harness = Harness::new();
    let run = start(&harness, "hold-stop", CancellationToken::new());
    let bindings = harness.ready();
    let completion = run.request_stop();
    let waiting = completion.clone();
    let waiter = tokio::spawn(async move { waiting.wait().await });
    // This event is sent only after the real child reads stop and then EOF.
    // Its fixture gate keeps that same process alive until explicitly released.
    harness.stop_observed();
    pending(&completion).await;
    waiter.abort();
    assert!(waiter
        .await
        .expect_err("first_waiter_abandoned")
        .is_cancelled());
    let second = run.request_stop();
    pending(&second).await;
    harness.send(Action::ReleaseStop);
    let result = report(&completion).await;
    normal(&result);
    assert_eq!(result, report(&second).await);
    bindings.assert_released();
    harness.finish();
}

#[tokio::test]
async fn ignored_stop_is_forced_failure_and_reaps_real_resources() {
    let mut harness = Harness::new();
    let mut bounded = limits();
    bounded.stop_grace = Duration::from_millis(100);
    let run = start_owned_run(
        harness.command("ignore-stop"),
        "stubborn-instance",
        bounded,
        CancellationToken::new(),
    )
    .unwrap_or_else(|_| panic!("stubborn_start"));
    let bindings = harness.ready();
    let completion = run.request_stop();
    harness.stop_observed();
    let result = report(&completion).await;
    clean(&result);
    assert_eq!(result.reason, OwnedRunReason::Requested);
    assert_eq!(result.failure, Some(OwnedRunFailure::Forced));
    assert!(result.forced);
    bindings.assert_released();
    harness.finish();
}

#[tokio::test]
async fn original_monotonic_deadline_forces_stubborn_child_without_renewal() {
    let mut harness = Harness::new();
    let mut bounded = limits();
    bounded.lifetime = Duration::from_secs(1);
    bounded.stop_grace = Duration::from_millis(100);
    let run = start_owned_run(
        harness.command("ignore-stop"),
        "deadline-instance",
        bounded,
        CancellationToken::new(),
    )
    .unwrap_or_else(|_| panic!("deadline_start"));
    let bindings = harness.ready();
    assert_eq!(run.observer().observe_udp(bindings.udp4), Ok(()));
    let result = report(&run.completion()).await;
    clean(&result);
    assert_eq!(result.reason, OwnedRunReason::Deadline);
    assert_eq!(result.failure, Some(OwnedRunFailure::Deadline));
    assert!(result.forced);
    assert_eq!(
        run.observer().observe_udp(bindings.udp6),
        Err(OwnedRunError::Closed)
    );
    bindings.assert_released();
    harness.finish();
}

#[tokio::test]
async fn spontaneous_zero_and_nonzero_exits_are_execution_failures() {
    for (action, code) in [(Action::ExitZero, 0), (Action::ExitSeven, 7)] {
        let mut harness = Harness::new();
        let run = start(&harness, "normal", CancellationToken::new());
        let bindings = harness.ready();
        harness.send(action);
        let result = report(&run.completion()).await;
        clean(&result);
        assert_eq!(result.reason, OwnedRunReason::Exited);
        assert_eq!(result.failure, Some(OwnedRunFailure::EarlyExit));
        assert_eq!(result.exit_code, Some(code));
        assert!(!result.forced);
        assert_eq!(result, report(&run.request_stop()).await);
        bindings.assert_released();
        harness.finish();
    }
}

#[tokio::test]
async fn stdout_stderr_and_aggregate_output_limits_preserve_failure_after_exit_zero() {
    for (index, action) in [Action::FloodOut, Action::FloodErr, Action::FloodBoth]
        .into_iter()
        .enumerate()
    {
        let mut harness = Harness::new();
        let mut bounded = limits();
        bounded.output_bytes = 64 * 1024;
        let run = start_owned_run(
            harness.command("normal"),
            "output-instance",
            bounded,
            CancellationToken::new(),
        )
        .unwrap_or_else(|_| panic!("output_start"));
        let bindings = harness.ready();
        harness.send(action);
        let result = report(&run.completion()).await;
        clean(&result);
        assert_eq!(result.reason, OwnedRunReason::Failure);
        assert_eq!(result.failure, Some(OwnedRunFailure::OutputLimit));
        assert_eq!(result.exit_code, Some(0));
        assert!(!result.forced);
        assert!(result.stdout_bytes.saturating_add(result.stderr_bytes) > bounded.output_bytes);
        if index == 2 {
            assert!(result.stdout_bytes < bounded.output_bytes);
            assert!(result.stderr_bytes < bounded.output_bytes);
            assert!(result.stdout_bytes >= 40 * 1024 && result.stderr_bytes >= 40 * 1024);
        }
        bindings.assert_released();
        harness.finish();
    }
}

#[test]
fn native_worker_reaps_and_joins_without_a_tokio_runtime() {
    let mut harness = Harness::new();
    let run = start(&harness, "normal", CancellationToken::new());
    let bindings = harness.ready();
    let completion = run.request_stop();
    harness.stop_observed();
    let deadline = Instant::now() + WAIT;
    let result = loop {
        if let Some(result) = completion.receiver.borrow().clone() {
            break result;
        }
        assert!(
            Instant::now() < deadline,
            "native_worker_completion_deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    normal(&result);
    bindings.assert_released();
    harness.finish();
}

#[tokio::test]
async fn exit_reaped_before_requested_stop_is_not_normal_success() {
    use test_support::{start_with_hooks, TestGate, TestHooks};
    let gate = Arc::new(TestGate::default());
    let _release = ReleaseGate(gate.clone());
    let hooks = Arc::new(TestHooks {
        after_exit_gate: Some(gate.clone()),
        ..TestHooks::default()
    });
    let mut harness = Harness::new();
    let run = start_with_hooks(
        harness.command("normal"),
        "exit-before-stop-instance",
        limits(),
        CancellationToken::new(),
        hooks.clone(),
    )
    .unwrap_or_else(|_| panic!("exit_race_start"));
    let bindings = harness.ready();
    harness.send(Action::ExitZero);
    assert!(gate.wait_reached(WAIT), "real_exit_before_classification");
    assert!(hooks.child_reaped.load(std::sync::atomic::Ordering::SeqCst));
    bindings.assert_released();
    // The actual Child has already exited and been reaped. A later Requested
    // reason must not reinterpret that spontaneous zero exit as a normal stop.
    let completion = run.request_stop();
    gate.release();
    let result = report(&completion).await;
    clean(&result);
    assert_eq!(result.reason, OwnedRunReason::Requested);
    assert_eq!(result.failure, Some(OwnedRunFailure::EarlyExit));
    assert_eq!(result.exit_code, Some(0));
    assert!(!result.forced);
    harness.finish();
}
