use super::*;
use fixture::Bindings;
use std::{
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    sync::atomic::Ordering,
};
use test_support::{start_with_hooks, TestGate, TestHooks};

fn endpoints(bindings: &Bindings) -> [(SocketAddrV4, UdpLoopbackRepresentation); 2] {
    let SocketAddr::V4(literal) = bindings.udp4 else {
        panic!("fixture_literal_v4");
    };
    [
        (literal, UdpLoopbackRepresentation::Ipv4Literal),
        (
            udp_loopback_fixture::target(bindings.udp6),
            // The fixture explicitly constructs an AF_INET6 dual-stack
            // socket at a mapped address. On this Windows, IP Helper reports
            // that socket as an AF_INET literal row; socket.local_addr is not
            // authority for the representation observed by the production API.
            UdpLoopbackRepresentation::Ipv4Literal,
        ),
    ]
}

async fn expect_sample<T: std::fmt::Debug + PartialEq>(
    actual: T,
    expected: T,
    runs: &[&OwnedRun],
    bindings: &[&Bindings],
) {
    if actual != expected {
        // Finish the same original owners before a diagnostic assertion can
        // unwind. All controls are sealed before waiting for any completion.
        let completions: Vec<_> = runs.iter().map(|run| run.request_stop()).collect();
        for completion in completions {
            clean(&report(&completion).await);
        }
        for endpoints in bindings {
            endpoints.assert_released();
        }
    }
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn explicit_dual_stack_socket_has_literal_os_row_bytes_and_same_child_rebind() {
    let mut harness = Harness::with_mapped_udp();
    let run = start(&harness, "normal", CancellationToken::new());
    let bindings = harness.ready();
    let observer = run.observer();
    for (endpoint, representation) in endpoints(&bindings) {
        expect_sample(
            observer.observe_udp_loopback_binding(endpoint),
            Ok(representation),
            &[&run],
            &[&bindings],
        )
        .await;
    }
    bindings.exchange_bytes();
    for (endpoint, representation) in endpoints(&bindings) {
        expect_sample(
            observer.observe_udp_loopback_binding(endpoint),
            Ok(representation),
            &[&run],
            &[&bindings],
        )
        .await;
    }
    // UDP82 still rejects a mapped request. The logical V4 request legitimately
    // passes because the real IP Helper row is literal V4 on this Windows.
    expect_sample(
        observer.observe_udp(bindings.udp6),
        Err(OwnedRunError::Discovery(DiscoveryError::InvalidRequest)),
        &[&run],
        &[&bindings],
    )
    .await;
    expect_sample(
        observer.observe_udp(udp_loopback_fixture::target(bindings.udp6).into()),
        Ok(()),
        &[&run],
        &[&bindings],
    )
    .await;
    expect_sample(
        observer.observe_udp(bindings.udp4),
        Ok(()),
        &[&run],
        &[&bindings],
    )
    .await;
    harness.release_udp();
    for (endpoint, _) in endpoints(&bindings) {
        expect_sample(
            observer.observe_udp_loopback_binding(endpoint),
            Err(OwnedRunError::Discovery(DiscoveryError::ListenerChanged)),
            &[&run],
            &[&bindings],
        )
        .await;
    }
    harness.rebind_udp();
    bindings.exchange_bytes();
    for (endpoint, representation) in endpoints(&bindings) {
        expect_sample(
            observer.observe_udp_loopback_binding(endpoint),
            Ok(representation),
            &[&run],
            &[&bindings],
        )
        .await;
    }
    normal(&report(&run.request_stop()).await);
    for (endpoint, _) in endpoints(&bindings) {
        assert_eq!(
            observer.observe_udp_loopback_binding(endpoint),
            Err(OwnedRunError::Closed)
        );
    }
    bindings.assert_released();
    harness.finish();
}

#[tokio::test]
async fn loopback_binding_rejects_invalid_parent_foreign_and_same_port_ambiguity() {
    let mut first = Harness::with_mapped_udp();
    let run = start(&first, "normal", CancellationToken::new());
    let bindings = first.ready();
    let observer = run.observer();
    let mut second = Harness::with_mapped_udp();
    let foreign = start(&second, "normal", CancellationToken::new());
    let other = second.ready();
    let parent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let SocketAddr::V4(parent_endpoint) = parent.local_addr().unwrap() else {
        panic!("parent_literal_v4");
    };
    expect_sample(
        observer.observe_udp_loopback_binding(parent_endpoint),
        Err(OwnedRunError::Discovery(DiscoveryError::ListenerChanged)),
        &[&run, &foreign],
        &[&bindings, &other],
    )
    .await;
    for endpoint in [
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0),
        SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, bindings.udp4.port()),
        SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 2), bindings.udp4.port()),
        SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 1), bindings.udp4.port()),
    ] {
        expect_sample(
            observer.observe_udp_loopback_binding(endpoint),
            Err(OwnedRunError::Discovery(DiscoveryError::InvalidRequest)),
            &[&run, &foreign],
            &[&bindings, &other],
        )
        .await;
    }
    for (endpoint, representation) in endpoints(&bindings) {
        expect_sample(
            observer.observe_udp_loopback_binding(endpoint),
            Ok(representation),
            &[&run, &foreign],
            &[&bindings, &other],
        )
        .await;
        expect_sample(
            foreign.observer().observe_udp_loopback_binding(endpoint),
            Err(OwnedRunError::Discovery(DiscoveryError::ListenerChanged)),
            &[&run, &foreign],
            &[&bindings, &other],
        )
        .await;
        // A parent row on another local address is still a second row of the
        // requested port. It must make even the favorable child's row fail.
        let conflicting = UdpSocket::bind((Ipv4Addr::new(127, 0, 0, 2), endpoint.port()))
            .expect("real_second_address_same_udp_port");
        let ambiguous = observer.observe_udp_loopback_binding(endpoint);
        drop(conflicting);
        expect_sample(
            ambiguous,
            Err(OwnedRunError::Discovery(DiscoveryError::ListenerChanged)),
            &[&run, &foreign],
            &[&bindings, &other],
        )
        .await;
        expect_sample(
            observer.observe_udp_loopback_binding(endpoint),
            Ok(representation),
            &[&run, &foreign],
            &[&bindings, &other],
        )
        .await;
    }
    for (endpoint, representation) in endpoints(&other) {
        expect_sample(
            foreign.observer().observe_udp_loopback_binding(endpoint),
            Ok(representation),
            &[&run, &foreign],
            &[&bindings, &other],
        )
        .await;
        expect_sample(
            observer.observe_udp_loopback_binding(endpoint),
            Err(OwnedRunError::Discovery(DiscoveryError::ListenerChanged)),
            &[&run, &foreign],
            &[&bindings, &other],
        )
        .await;
    }
    bindings.exchange_bytes();
    other.exchange_bytes();
    normal(&report(&run.request_stop()).await);
    normal(&report(&foreign.request_stop()).await);
    bindings.assert_released();
    other.assert_released();
    drop(parent);
    first.finish();
    second.finish();
}

#[tokio::test]
async fn dual_stack_socket_with_literal_os_row_cannot_reactivate_replaced_weak_observer() {
    let mut first = Harness::with_mapped_udp();
    let original = start(&first, "normal", CancellationToken::new());
    let bindings = first.ready();
    let old = original.observer();
    for (endpoint, representation) in endpoints(&bindings) {
        expect_sample(
            old.observe_udp_loopback_binding(endpoint),
            Ok(representation),
            &[&original],
            &[&bindings],
        )
        .await;
    }
    normal(&report(&original.request_stop()).await);
    bindings.assert_released();
    let mut replacement = Harness::at(bindings.clone());
    let next = start(&replacement, "normal", CancellationToken::new());
    let rebound = replacement.ready();
    rebound.exchange_bytes();
    for (endpoint, representation) in endpoints(&rebound) {
        expect_sample(
            next.observer().observe_udp_loopback_binding(endpoint),
            Ok(representation),
            &[&original, &next],
            &[&rebound],
        )
        .await;
        expect_sample(
            old.observe_udp_loopback_binding(endpoint),
            Err(OwnedRunError::Closed),
            &[&original, &next],
            &[&rebound],
        )
        .await;
    }
    normal(&report(&next.request_stop()).await);
    rebound.assert_released();
    first.finish();
    replacement.finish();
}

#[derive(Clone, Copy)]
enum QueryKind {
    Tcp,
    Udp82,
    Loopback85,
}

fn sample(
    observer: &OwnedRunObserver,
    bindings: &Bindings,
    kind: QueryKind,
) -> Result<(), OwnedRunError> {
    match kind {
        QueryKind::Tcp => observer.observe_tcp(bindings.tcp4),
        QueryKind::Udp82 => observer.observe_udp(bindings.udp4),
        QueryKind::Loopback85 => observer
            .observe_udp_loopback_binding(udp_loopback_fixture::target(bindings.udp6))
            .map(|representation| {
                assert_eq!(representation, UdpLoopbackRepresentation::Ipv4Literal);
            }),
    }
}

async fn bounded_sample(
    observer: &OwnedRunObserver,
    bindings: &Bindings,
    kind: QueryKind,
) -> Result<(), OwnedRunError> {
    let observer = observer.clone();
    let bindings = bindings.clone();
    tokio::time::timeout(
        WAIT,
        tokio::task::spawn_blocking(move || sample(&observer, &bindings, kind)),
    )
    .await
    .expect("loopback85_query_deadline")
    .expect("loopback85_query_worker")
}

#[derive(Clone, Copy)]
enum Seal {
    Stop,
    Cancel,
    Deadline,
    Drop,
}

async fn retained_query(kind: QueryKind, positive: bool, seal: Seal) {
    let gate = Arc::new(TestGate::default());
    let _release_on_unwind = ReleaseGate(gate.clone());
    let hooks = Arc::new(TestHooks {
        before_query_gate: (!positive).then(|| gate.clone()),
        after_positive_query_gate: positive.then(|| gate.clone()),
        ..Default::default()
    });
    let cancel = CancellationToken::new();
    let mut bounded = limits();
    if matches!(seal, Seal::Deadline) {
        bounded.lifetime = Duration::from_secs(2);
        bounded.stop_grace = Duration::from_millis(100);
    }
    let mut harness = Harness::with_mapped_udp();
    let run = start_with_hooks(
        harness.command("normal"),
        "udp-loopback-query-original-instance",
        bounded,
        cancel.clone(),
        hooks.clone(),
    )
    .unwrap();
    let bindings = harness.ready();
    let observer = run.observer();
    let completion = run.completion();
    let query_observer = observer.clone();
    let query_bindings = bindings.clone();
    let query = tokio::task::spawn_blocking(move || sample(&query_observer, &query_bindings, kind));
    assert!(gate.wait_reached(WAIT), "original_query_retained");
    for candidate in [QueryKind::Tcp, QueryKind::Udp82, QueryKind::Loopback85] {
        assert_eq!(
            bounded_sample(&observer, &bindings, candidate).await,
            Err(OwnedRunError::Busy)
        );
    }
    assert_eq!(
        hooks.query_calls.load(Ordering::SeqCst),
        usize::from(positive)
    );
    assert!(run.shared.lock().query_active, "same_registered_query");
    let mut control = Some(run);
    match seal {
        Seal::Stop => {
            control.as_ref().unwrap().request_stop();
        }
        Seal::Cancel => cancel.cancel(),
        Seal::Deadline => {}
        Seal::Drop => drop(control.take()),
    }
    tokio::time::timeout(WAIT, async {
        while !hooks.child_reaped.load(Ordering::SeqCst)
            || !hooks.readers_complete.load(Ordering::SeqCst)
        {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("reap_and_readers_before_query_release");
    // Every real listener/socket can be rebound before the same query's gate
    // is released. Readers and Child are done, but completion must still wait.
    bindings.assert_released();
    pending(&completion).await;
    let waiting = completion.clone();
    let waiter = tokio::spawn(async move { waiting.wait().await });
    tokio::task::yield_now().await;
    waiter.abort();
    assert!(waiter.await.expect_err("lost_waiter").is_cancelled());
    pending(&completion).await;
    for candidate in [QueryKind::Tcp, QueryKind::Udp82, QueryKind::Loopback85] {
        assert_eq!(
            bounded_sample(&observer, &bindings, candidate).await,
            Err(OwnedRunError::Closed)
        );
    }
    assert!(!query.is_finished(), "same_query_not_discarded");
    gate.release();
    assert_eq!(
        tokio::time::timeout(WAIT, query)
            .await
            .expect("retained_query_release_deadline")
            .expect("retained_query_worker"),
        Err(OwnedRunError::Closed)
    );
    let result = report(&completion).await;
    clean(&result);
    let (reason, failure) = match seal {
        Seal::Stop => (OwnedRunReason::Requested, None),
        Seal::Cancel => (OwnedRunReason::Cancelled, Some(OwnedRunFailure::Cancelled)),
        Seal::Deadline => (OwnedRunReason::Deadline, Some(OwnedRunFailure::Deadline)),
        Seal::Drop => (OwnedRunReason::Dropped, Some(OwnedRunFailure::Dropped)),
    };
    assert_eq!(result.reason, reason);
    assert_eq!(result.failure, failure);
    if matches!(seal, Seal::Stop) {
        normal(&result);
    }
    assert_eq!(hooks.query_calls.load(Ordering::SeqCst), 1);
    if let Some(run) = &control {
        assert_eq!(result, report(&run.request_stop()).await);
    }
    harness.finish();
}

#[tokio::test]
async fn retained_udp85_query_excludes_tcp_udp82_and_reaps_before_same_query_returns() {
    retained_query(QueryKind::Loopback85, false, Seal::Stop).await;
}

#[tokio::test]
async fn retained_tcp_query_excludes_udp85_in_the_same_slot() {
    retained_query(QueryKind::Tcp, false, Seal::Stop).await;
}

#[tokio::test]
async fn retained_udp82_query_excludes_udp85_in_the_same_slot() {
    retained_query(QueryKind::Udp82, false, Seal::Stop).await;
}

#[tokio::test]
async fn positive_real_literal_os_query_returns_closed_after_stop_seals() {
    retained_query(QueryKind::Loopback85, true, Seal::Stop).await;
}

#[tokio::test]
async fn positive_real_literal_os_query_returns_closed_after_cancel_and_reap() {
    retained_query(QueryKind::Loopback85, true, Seal::Cancel).await;
}

#[tokio::test]
async fn positive_real_literal_os_query_returns_closed_after_deadline_and_reap() {
    retained_query(QueryKind::Loopback85, true, Seal::Deadline).await;
}

#[tokio::test]
async fn positive_real_literal_os_query_returns_closed_after_control_drop_and_reap() {
    retained_query(QueryKind::Loopback85, true, Seal::Drop).await;
}
