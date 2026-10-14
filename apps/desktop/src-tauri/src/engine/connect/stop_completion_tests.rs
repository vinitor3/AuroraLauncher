//! Deterministic worker-tail regression. `finish` has already published the
//! truthful completed snapshot, but the owned task has not returned yet.
//! No transport, personal configuration, sleep-based race or external service.
use super::*;
use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
    task::{Context, Poll, Waker},
};
use tokio::{sync::Notify, time::timeout};

#[derive(Default)]
struct TailGate {
    begin: Notify,
    entered: Notify,
    release: Notify,
    returned: AtomicBool,
}
fn request() -> HostRequest {
    HostRequest {
        mode: ConnectMode::Loopback,
        bind_address: "127.0.0.1:43000".into(),
        game_target: "127.0.0.1:43001".into(),
        lifetime_seconds: 30,
    }
}
fn pending<T>(future: Pin<&mut impl Future<Output = T>>) {
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        matches!(future.poll(&mut context), Poll::Pending),
        "owned_worker_tail_must_be_awaited"
    );
}
async fn armed(error: Option<ConnectError>) -> (Arc<ConnectManager>, String, Arc<TailGate>) {
    let manager = Arc::new(ConnectManager::default());
    let id = manager
        .prepare_host("main", request())
        .unwrap()
        .operation_id;
    let gate = Arc::new(TailGate::default());
    let worker_gate = gate.clone();
    let state = manager.live.state.clone();
    let worker_id = id.clone();
    let worker = tokio::spawn(async move {
        worker_gate.begin.notified().await;
        finish(&state, &worker_id, error);
        worker_gate.entered.notify_one();
        worker_gate.release.notified().await;
        worker_gate.returned.store(true, Ordering::SeqCst);
    });
    {
        let mut state = manager.live.state.lock().unwrap();
        let attempt = selected(&mut state, "main", &id).unwrap();
        attempt.snapshot.phase = "starting";
        attempt.snapshot.cleanup_state = "pending";
        attempt.worker = Some(worker);
    }
    gate.begin.notify_one();
    timeout(Duration::from_secs(3), gate.entered.notified())
        .await
        .unwrap();
    assert!(!gate.returned.load(Ordering::SeqCst));
    (manager, id, gate)
}
fn assert_complete(manager: &ConnectManager, error: Option<ConnectError>) {
    let snapshot = manager.snapshot("main").unwrap();
    assert_eq!(
        snapshot.phase,
        if error.is_some() { "failed" } else { "closed" }
    );
    assert_eq!(snapshot.cleanup_state, "complete");
    assert_eq!(snapshot.error_code, error.map(ConnectError::code));
}

#[tokio::test]
async fn completed_snapshot_does_not_regress_while_stop_waits_for_owned_worker_tail() {
    for error in [None, Some(ConnectError::TransportFailed)] {
        let (manager, id, gate) = armed(error).await;
        assert_complete(&manager, error);
        let mut stop = Box::pin(manager.stop("main", &id));
        pending(stop.as_mut());
        assert_complete(&manager, error);
        assert!(
            matches!(
                manager.prepare_host("main", request()),
                Err(ConnectError::Busy)
            ),
            "slot_must_not_be_reused_before_worker_return"
        );
        gate.release.notify_one();
        let snapshot = timeout(Duration::from_secs(3), stop)
            .await
            .unwrap()
            .unwrap();
        assert!(gate.returned.load(Ordering::SeqCst));
        assert_eq!(snapshot.cleanup_state, "complete");
        assert_complete(&manager, error);
        // Reuse is allowed only after the same worker's tail has returned.
        let next = manager.prepare_host("main", request()).unwrap();
        assert_ne!(next.operation_id, id);
        manager.stop("main", &next.operation_id).await.unwrap();
    }
}

#[tokio::test]
async fn duplicate_stop_and_logout_survive_lost_waiter_without_skipping_worker_tail() {
    for error in [None, Some(ConnectError::TransportFailed)] {
        let (manager, id, gate) = armed(error).await;
        let mut first = Box::pin(manager.stop("main", &id));
        pending(first.as_mut());
        let mut duplicate = Box::pin(manager.stop("main", &id));
        pending(duplicate.as_mut());
        let mut logout = Box::pin(manager.shutdown_all());
        pending(logout.as_mut());
        assert_complete(&manager, error);
        assert!(!gate.returned.load(Ordering::SeqCst));
        // IPC cancellation cannot transfer or destroy ownership of the worker.
        drop(first);
        pending(duplicate.as_mut());
        pending(logout.as_mut());
        assert_complete(&manager, error);
        gate.release.notify_one();
        let (stopped, ()) = timeout(Duration::from_secs(3), async {
            tokio::join!(duplicate, logout)
        })
        .await
        .unwrap();
        assert_eq!(stopped.unwrap().cleanup_state, "complete");
        assert!(gate.returned.load(Ordering::SeqCst));
        assert_complete(&manager, error);
        assert!(
            matches!(
                manager.prepare_host("main", request()),
                Err(ConnectError::Cancelled)
            ),
            "logout_latch_must_stay_closed"
        );
    }
}
