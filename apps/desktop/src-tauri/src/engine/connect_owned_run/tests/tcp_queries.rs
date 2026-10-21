use super::*;
use std::{
    net::{Ipv6Addr, SocketAddrV6, TcpListener},
    sync::atomic::Ordering,
};
use test_support::{start_with_hooks, TestGate, TestHooks};

async fn bounded_sample(
    observer: &OwnedRunObserver,
    endpoint: SocketAddr,
    tcp: bool,
) -> Result<(), OwnedRunError> {
    let observer = observer.clone();
    tokio::time::timeout(
        WAIT,
        tokio::task::spawn_blocking(move || {
            if tcp {
                observer.observe_tcp(endpoint)
            } else {
                observer.observe_udp(endpoint)
            }
        }),
    )
    .await
    .expect("bounded_query_deadline")
    .expect("bounded_query_worker")
}

#[tokio::test]
async fn exact_original_tcp_samples_bracket_real_bytes_and_close_with_run() {
    let mut harness = Harness::new();
    let run = start(&harness, "normal", CancellationToken::new());
    let endpoints = harness.ready();
    let observer = run.observer();
    for endpoint in [endpoints.tcp4, endpoints.tcp6] {
        assert_eq!(observer.observe_tcp(endpoint), Ok(()));
    }
    endpoints.exchange_bytes();
    for endpoint in [endpoints.tcp4, endpoints.tcp6] {
        assert_eq!(observer.observe_tcp(endpoint), Ok(()));
    }
    for endpoint in endpoints.udp() {
        assert_eq!(observer.observe_udp(endpoint), Ok(()));
    }
    normal(&report(&run.request_stop()).await);
    for endpoint in [endpoints.tcp4, endpoints.tcp6] {
        assert_eq!(observer.observe_tcp(endpoint), Err(OwnedRunError::Closed));
    }
    endpoints.assert_released();
    harness.finish();
}

#[tokio::test]
async fn tcp_queries_reject_nonliteral_loopback_and_preserve_exact_port() {
    let mut harness = Harness::new();
    let run = start(&harness, "normal", CancellationToken::new());
    let endpoints = harness.ready();
    let observer = run.observer();
    let scoped = SocketAddr::V6(SocketAddrV6::new(
        Ipv6Addr::LOCALHOST,
        endpoints.tcp6.port(),
        0,
        1,
    ));
    let flow = SocketAddr::V6(SocketAddrV6::new(
        Ipv6Addr::LOCALHOST,
        endpoints.tcp6.port(),
        1,
        0,
    ));
    for endpoint in [
        "127.0.0.1:0".parse().unwrap(),
        "[::1]:0".parse().unwrap(),
        "0.0.0.0:43001".parse().unwrap(),
        "[::]:43001".parse().unwrap(),
        "192.0.2.1:43001".parse().unwrap(),
        "[::ffff:127.0.0.1]:43001".parse().unwrap(),
        scoped,
        flow,
    ] {
        assert_eq!(
            observer.observe_tcp(endpoint),
            Err(OwnedRunError::Discovery(DiscoveryError::InvalidRequest))
        );
    }
    // A real parent's listener is an exact positive existence control, but it
    // cannot satisfy this child's original process proof.
    let foreign = TcpListener::bind("127.0.0.1:0").unwrap();
    assert_eq!(
        observer.observe_tcp(foreign.local_addr().unwrap()),
        Err(OwnedRunError::Discovery(DiscoveryError::ListenerChanged))
    );
    assert_eq!(observer.observe_tcp(endpoints.tcp4), Ok(()));
    normal(&report(&run.request_stop()).await);
    endpoints.assert_released();
    harness.finish();
}

#[tokio::test]
async fn tcp_foreign_child_and_same_endpoint_replacement_do_not_reactivate_original() {
    let mut first = Harness::new();
    let original = start(&first, "normal", CancellationToken::new());
    let endpoints = first.ready();
    let old = original.observer();
    let mut second = Harness::new();
    let other_run = start(&second, "normal", CancellationToken::new());
    let other = second.ready();
    for endpoint in [endpoints.tcp4, endpoints.tcp6] {
        assert_eq!(old.observe_tcp(endpoint), Ok(()));
        assert_eq!(
            other_run.observer().observe_tcp(endpoint),
            Err(OwnedRunError::Discovery(DiscoveryError::ListenerChanged))
        );
    }
    normal(&report(&original.request_stop()).await);
    endpoints.assert_released();
    let mut replacement = Harness::at(endpoints.clone());
    let next = start(&replacement, "normal", CancellationToken::new());
    let rebound = replacement.ready();
    for endpoint in [rebound.tcp4, rebound.tcp6] {
        assert_eq!(next.observer().observe_tcp(endpoint), Ok(()));
        assert_eq!(old.observe_tcp(endpoint), Err(OwnedRunError::Closed));
    }
    normal(&report(&next.request_stop()).await);
    normal(&report(&other_run.request_stop()).await);
    rebound.assert_released();
    other.assert_released();
    first.finish();
    second.finish();
    replacement.finish();
}

async fn shared_slot_and_reap_before_query(tcp_first: bool, after_positive: bool) {
    let gate = Arc::new(TestGate::default());
    let _release_on_unwind = ReleaseGate(gate.clone());
    let hooks = Arc::new(TestHooks {
        before_query_gate: (!after_positive).then(|| gate.clone()),
        after_positive_query_gate: after_positive.then(|| gate.clone()),
        ..Default::default()
    });
    let mut harness = Harness::new();
    let run = start_with_hooks(
        harness.command("normal"),
        "tcp-query-original-instance",
        limits(),
        CancellationToken::new(),
        hooks.clone(),
    )
    .unwrap();
    let endpoints = harness.ready();
    let observer = run.observer();
    let query_observer = observer.clone();
    let query_endpoint = if tcp_first {
        endpoints.tcp4
    } else {
        endpoints.udp4
    };
    let query = tokio::task::spawn_blocking(move || {
        if tcp_first {
            query_observer.observe_tcp(query_endpoint)
        } else {
            query_observer.observe_udp(query_endpoint)
        }
    });
    assert!(gate.wait_reached(WAIT));
    assert_eq!(
        bounded_sample(&observer, endpoints.tcp6, true).await,
        Err(OwnedRunError::Busy)
    );
    assert_eq!(
        bounded_sample(&observer, endpoints.udp6, false).await,
        Err(OwnedRunError::Busy)
    );
    let completion = run.request_stop();
    harness.stop_observed();
    tokio::time::timeout(WAIT, async {
        while !hooks.child_reaped.load(Ordering::SeqCst)
            || !hooks.readers_complete.load(Ordering::SeqCst)
        {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    // This real rebind occurs with the same child proof query deliberately
    // retained. Completion remains pending until that registered query returns.
    endpoints.assert_released();
    pending(&completion).await;
    assert_eq!(
        bounded_sample(&observer, endpoints.tcp4, true).await,
        Err(OwnedRunError::Closed)
    );
    gate.release();
    assert_eq!(
        tokio::time::timeout(WAIT, query)
            .await
            .expect("held_query_release_deadline")
            .expect("held_query_worker"),
        Err(OwnedRunError::Closed)
    );
    normal(&report(&completion).await);
    assert_eq!(hooks.query_calls.load(Ordering::SeqCst), 1);
    harness.finish();
}

#[tokio::test]
async fn held_tcp_query_excludes_udp_and_reap_precedes_query_completion() {
    shared_slot_and_reap_before_query(true, false).await;
}

#[tokio::test]
async fn held_udp_query_excludes_tcp_in_the_same_original_run_slot() {
    shared_slot_and_reap_before_query(false, false).await;
}

#[tokio::test]
async fn positive_real_tcp_query_cannot_return_ready_after_stop_seals() {
    shared_slot_and_reap_before_query(true, true).await;
}
