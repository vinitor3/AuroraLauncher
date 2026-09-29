//! Opaque synthetic UDP packets exercise host-authorized receive-heavy services.
//! These tests do not instantiate, decode or claim support for a voice mod.
use aurora_connect_lab::*;
use std::{collections::BTreeSet, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    net::UdpSocket,
    time::{Instant, sleep, timeout},
};

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn service(endpoint: SocketAddr, policy: UdpReturnPolicy) -> Service {
    Service {
        service_id: "opaque".into(),
        protocol: Protocol::Udp,
        endpoint,
        revision: 1,
        max_datagram_bytes: 1200,
        required_capability: Capability::Datagram,
        udp_return_policy: policy,
    }
}
async fn fixture(
    endpoint: SocketAddr,
    policy: UdpReturnPolicy,
    limits: Limits,
    peers: usize,
) -> (Host, Vec<Guest>) {
    let keys: Vec<_> = (0..peers).map(|_| generate_key()).collect();
    let leases = keys
        .iter()
        .enumerate()
        .map(|(i, k)| Lease {
            lease_id: format!("p{i}"),
            peer: k.public(),
            expires_at_unix_ms: unix_millis() + 60_000,
            services: BTreeSet::from(["opaque".into()]),
        })
        .collect();
    let host = Host::start(HostConfig {
        secret_key: generate_key(),
        bind_addr: local(),
        session_id: "downstream".into(),
        services: vec![service(endpoint, policy)],
        leases,
        limits: limits.clone(),
    })
    .await
    .unwrap();
    let mut guests = Vec::new();
    for (i, key) in keys.into_iter().enumerate() {
        guests.push(
            Guest::connect(GuestConfig {
                secret_key: key,
                bind_addr: local(),
                host: host.addr(),
                session_id: "downstream".into(),
                lease_id: format!("p{i}"),
                capabilities: vec![Capability::Datagram],
                limits: limits.clone(),
            })
            .await
            .unwrap(),
        );
    }
    (host, guests)
}
fn downstream() -> UdpReturnPolicy {
    UdpReturnPolicy::AuthorizedDownstream {
        bytes_per_second: 100_000,
        datagrams_per_second: 100,
    }
}
async fn stop(host: Host, guests: Vec<Guest>) {
    for guest in guests {
        guest.shutdown().await;
    }
    host.shutdown().await;
    assert_eq!(host.metrics().active_udp, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicitly_authorized_downstream_receives_many_larger_packets_after_one_request() {
    let endpoint = UdpSocket::bind(local()).await.unwrap();
    let (host, guests) = fixture(
        endpoint.local_addr().unwrap(),
        downstream(),
        Limits::default(),
        1,
    )
    .await;
    let a = guests[0].open_udp("opaque", 1).await.unwrap();
    let mut input = [0; 8];
    // Opening an association is not a request to the local service or a grant to send payload.
    assert!(
        timeout(Duration::from_millis(80), endpoint.recv_from(&mut input))
            .await
            .is_err()
    );
    assert_eq!(host.metrics().udp_forwarded, 0);
    assert_eq!(host.metrics().udp_returned, 0);
    a.send(&[7]).await.unwrap();
    let (n, address) = timeout(Duration::from_secs(2), endpoint.recv_from(&mut input))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&input[..n], &[7]);
    for marker in 0..8u8 {
        let payload = vec![marker; 512];
        endpoint.send_to(&payload, address).await.unwrap();
        assert_eq!(
            timeout(Duration::from_secs(2), a.recv())
                .await
                .unwrap()
                .unwrap()
                .as_ref(),
            payload
        );
    }
    assert_eq!(host.metrics().udp_forwarded, 1);
    assert_eq!(host.metrics().udp_returned, 8);
    a.shutdown().await;
    stop(host, guests).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn default_reply_budget_rejects_larger_response_without_consuming_valid_echo_credit() {
    let endpoint = UdpSocket::bind(local()).await.unwrap();
    let (host, guests) = fixture(
        endpoint.local_addr().unwrap(),
        UdpReturnPolicy::ReplyBudget,
        Limits::default(),
        1,
    )
    .await;
    let a = guests[0].open_udp("opaque", 1).await.unwrap();
    a.send(&[7]).await.unwrap();
    let mut buffer = [0; 8];
    let (_, address) = endpoint.recv_from(&mut buffer).await.unwrap();
    endpoint.send_to(&[8; 512], address).await.unwrap();
    assert!(timeout(Duration::from_millis(150), a.recv()).await.is_err());
    endpoint.send_to(&[7], address).await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(2), a.recv())
            .await
            .unwrap()
            .unwrap()
            .as_ref(),
        &[7]
    );
    assert!(host.metrics().udp_dropped >= 1);
    a.shutdown().await;
    stop(host, guests).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn downstream_budget_is_shared_between_associations_but_isolated_between_peers() {
    let endpoint = Arc::new(UdpSocket::bind(local()).await.unwrap());
    let policy = UdpReturnPolicy::AuthorizedDownstream {
        bytes_per_second: 10_000,
        datagrams_per_second: 5,
    };
    let (host, guests) =
        fixture(endpoint.local_addr().unwrap(), policy, Limits::default(), 2).await;
    let mut associations = Vec::new();
    let mut addresses = Vec::new();
    for guest in &guests {
        for _ in 0..2 {
            let a = guest.open_udp("opaque", 1).await.unwrap();
            a.send(&[1]).await.unwrap();
            let mut buffer = [0; 8];
            let (_, addr) = endpoint.recv_from(&mut buffer).await.unwrap();
            addresses.push(addr);
            associations.push(a);
        }
    }
    let mut readers = tokio::task::JoinSet::new();
    for (i, a) in associations.iter().enumerate() {
        let a = a.clone();
        readers.spawn(async move {
            let deadline = Instant::now() + Duration::from_millis(250);
            let mut count = 0;
            while let Ok(Ok(data)) = tokio::time::timeout_at(deadline, a.recv()).await {
                assert_eq!(data.as_ref(), &[i as u8; 100]);
                count += 1;
            }
            (i / 2, count)
        });
    }
    for round in 0..12 {
        for (i, addr) in addresses.iter().enumerate() {
            endpoint.send_to(&[i as u8; 100], addr).await.unwrap();
        }
        if round % 3 == 0 {
            sleep(Duration::from_millis(2)).await;
        }
    }
    let mut counts = [0; 2];
    while let Some(result) = readers.join_next().await {
        let (peer, count) = result.unwrap();
        counts[peer] += count;
    }
    // 5 packets of initial burst + at most 2 tokens during 250ms; opening two
    // associations cannot multiply it. Both authenticated peers get their own cap.
    assert!(
        counts.iter().all(|n| *n > 0 && *n <= 7),
        "per-peer combined replies: {counts:?}"
    );
    assert!(host.metrics().udp_dropped > 0);
    for a in associations {
        a.shutdown().await;
    }
    stop(host, guests).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn downstream_does_not_renew_guest_idle_deadline() {
    let endpoint = Arc::new(UdpSocket::bind(local()).await.unwrap());
    let limits = Limits {
        udp_idle_timeout: Duration::from_millis(300),
        ..Limits::default()
    };
    let (host, guests) = fixture(endpoint.local_addr().unwrap(), downstream(), limits, 1).await;
    let a = guests[0].open_udp("opaque", 1).await.unwrap();
    a.send(&[1]).await.unwrap();
    let mut buffer = [0; 8];
    let (_, address) = endpoint.recv_from(&mut buffer).await.unwrap();
    let producer = endpoint.clone();
    let producer = tokio::spawn(async move {
        for _ in 0..30 {
            let _ = producer.send_to(&[1; 200], address).await;
            sleep(Duration::from_millis(20)).await;
        }
    });
    let start = Instant::now();
    let mut received = 0;
    loop {
        match timeout(Duration::from_secs(2), a.recv()).await {
            Ok(Ok(_)) => received += 1,
            Ok(Err(_)) => break,
            Err(_) => panic!("downstream retained idle association"),
        }
    }
    assert!(received > 1);
    assert!(start.elapsed() < Duration::from_secs(1));
    timeout(Duration::from_secs(1), async {
        while host.metrics().active_udp != 0 {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    producer.abort();
    let _ = producer.await;
    a.shutdown().await;
    stop(host, guests).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn revocation_cancels_authorized_downstream() {
    let endpoint = UdpSocket::bind(local()).await.unwrap();
    let (host, guests) = fixture(
        endpoint.local_addr().unwrap(),
        downstream(),
        Limits::default(),
        1,
    )
    .await;
    let a = guests[0].open_udp("opaque", 1).await.unwrap();
    a.send(&[1]).await.unwrap();
    let mut buffer = [0; 8];
    let (_, address) = endpoint.recv_from(&mut buffer).await.unwrap();
    endpoint.send_to(&[2; 200], address).await.unwrap();
    assert!(a.recv().await.is_ok());
    host.revoke("p0");
    timeout(Duration::from_secs(1), async {
        while !a.is_closed() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    endpoint.send_to(&[3; 200], address).await.unwrap();
    assert!(a.recv().await.is_err());
    stop(host, guests).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invalid_downstream_policy_never_opens_an_endpoint() {
    for policy in [
        UdpReturnPolicy::AuthorizedDownstream {
            bytes_per_second: 0,
            datagrams_per_second: 5,
        },
        UdpReturnPolicy::AuthorizedDownstream {
            bytes_per_second: 10,
            datagrams_per_second: 0,
        },
        UdpReturnPolicy::AuthorizedDownstream {
            bytes_per_second: 250_001,
            datagrams_per_second: 5,
        },
        UdpReturnPolicy::AuthorizedDownstream {
            bytes_per_second: 1000,
            datagrams_per_second: 201,
        },
    ] {
        let key = generate_key();
        let result = Host::start(HostConfig {
            secret_key: generate_key(),
            bind_addr: local(),
            session_id: "invalid".into(),
            services: vec![service("127.0.0.1:9".parse().unwrap(), policy)],
            leases: vec![Lease {
                lease_id: "p".into(),
                peer: key.public(),
                expires_at_unix_ms: unix_millis() + 5000,
                services: BTreeSet::from(["opaque".into()]),
            }],
            limits: Limits::default(),
        })
        .await;
        assert!(matches!(result, Err(Error::Configuration)));
    }
}
