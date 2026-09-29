//! Independent real-loopback checks for the host-authorized opaque UDP policy.
//! No mod, microphone, game, NAT traversal or relay is exercised here.
use aurora_connect_lab::*;
use serde_json::json;
use std::{collections::BTreeSet, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::AsyncReadExt,
    net::UdpSocket,
    time::{Instant, sleep, timeout},
};

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn limits() -> Limits {
    Limits {
        handshake_timeout: Duration::from_secs(2),
        ..Limits::default()
    }
}
fn service(id: &str, endpoint: SocketAddr, packets: u32) -> Service {
    Service {
        service_id: id.into(),
        protocol: Protocol::Udp,
        endpoint,
        revision: 1,
        max_datagram_bytes: 1000,
        required_capability: Capability::Datagram,
        udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
            bytes_per_second: 100_000,
            datagrams_per_second: packets,
        },
    }
}
async fn host(endpoint: SocketAddr, key: &SecretKey, packets: u32, lifetime: Duration) -> Host {
    Host::start(HostConfig {
        secret_key: generate_key(),
        bind_addr: local(),
        session_id: "downstream-qa".into(),
        services: vec![
            service("one", endpoint, packets),
            service("two", endpoint, packets),
        ],
        leases: vec![Lease {
            lease_id: "p".into(),
            peer: key.public(),
            expires_at_unix_ms: unix_millis() + lifetime.as_millis() as u64,
            services: BTreeSet::from(["one".into(), "two".into()]),
        }],
        limits: limits(),
    })
    .await
    .unwrap()
}
async fn guest(host: &Host, key: SecretKey) -> Guest {
    Guest::connect(GuestConfig {
        secret_key: key,
        bind_addr: local(),
        host: host.addr(),
        session_id: "downstream-qa".into(),
        lease_id: "p".into(),
        capabilities: vec![Capability::Datagram],
        limits: limits(),
    })
    .await
    .unwrap()
}
async fn activate(socket: &UdpSocket, association: &UdpAssociation) -> SocketAddr {
    association.send(b"activate").await.unwrap();
    let mut buffer = [0; 32];
    let (n, address) = timeout(Duration::from_secs(1), socket.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&buffer[..n], b"activate");
    address
}
async fn reply(socket: &UdpSocket, address: SocketAddr, association: &UdpAssociation, marker: u8) {
    socket.send_to(&[marker; 100], address).await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(1), association.recv())
            .await
            .unwrap()
            .unwrap()
            .as_ref(),
        &[marker; 100]
    );
}
async fn empty_peers(host: &Host) {
    timeout(Duration::from_secs(1), async {
        while host.metrics().active_peers != 0 {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn downstream_quota_survives_reconnect_of_the_same_lease() {
    let endpoint = UdpSocket::bind(local()).await.unwrap();
    let key = generate_key();
    let host = host(
        endpoint.local_addr().unwrap(),
        &key,
        1,
        Duration::from_secs(30),
    )
    .await;
    let g = guest(&host, key.clone()).await;
    let first = g.open_udp("one", 1).await.unwrap();
    let address = activate(&endpoint, &first).await;
    reply(&endpoint, address, &first, 1).await;
    let consumed = Instant::now();
    g.shutdown().await;
    empty_peers(&host).await;
    let replacement = guest(&host, key).await;
    let second = replacement.open_udp("one", 1).await.unwrap();
    let address = activate(&endpoint, &second).await;
    assert!(
        consumed.elapsed() < Duration::from_millis(700),
        "loopback setup took too long to exercise the unrefilled token"
    );
    endpoint.send_to(&[2; 100], address).await.unwrap();
    assert!(
        timeout(Duration::from_millis(120), second.recv())
            .await
            .is_err(),
        "reconnect reset the per-lease downstream bucket"
    );
    assert_eq!(host.metrics().udp_returned, 1);
    sleep(Duration::from_millis(1100).saturating_sub(consumed.elapsed())).await;
    reply(&endpoint, address, &second, 3).await;
    replacement.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn downstream_quota_is_scoped_per_service_while_origins_stay_connected() {
    let endpoint = UdpSocket::bind(local()).await.unwrap();
    let stranger = UdpSocket::bind(local()).await.unwrap();
    let key = generate_key();
    let host = host(
        endpoint.local_addr().unwrap(),
        &key,
        1,
        Duration::from_secs(30),
    )
    .await;
    let g = guest(&host, key).await;
    let one = g.open_udp("one", 1).await.unwrap();
    let two = g.open_udp("two", 1).await.unwrap();
    let address_one = activate(&endpoint, &one).await;
    let address_two = activate(&endpoint, &two).await;
    assert_ne!(address_one, address_two);
    stranger.send_to(&[99; 100], address_one).await.unwrap();
    assert!(
        timeout(Duration::from_millis(100), one.recv())
            .await
            .is_err(),
        "foreign local UDP origin crossed the connected service socket"
    );
    reply(&endpoint, address_one, &one, 1).await;
    // Exhausting service one must not exhaust service two's approved bucket.
    endpoint.send_to(&[2; 100], address_one).await.unwrap();
    reply(&endpoint, address_two, &two, 3).await;
    assert!(
        timeout(Duration::from_millis(100), one.recv())
            .await
            .is_err()
    );
    assert_eq!(host.metrics().udp_returned, 2);
    g.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn peer_cannot_inject_udp_return_policy_into_open_request() {
    let endpoint = UdpSocket::bind(local()).await.unwrap();
    let key = generate_key();
    let host = host(
        endpoint.local_addr().unwrap(),
        &key,
        100,
        Duration::from_secs(30),
    )
    .await;
    let g = guest(&host, key).await;
    let (mut send, mut recv) = g.connection().open_bi().await.unwrap();
    let body = serde_json::to_vec(&json!({"schemaVersion":1,"requestId":1,"serviceId":"one","serviceRevision":1,
        "protocol":"udp","operation":"open","associationId":0,"udpReturnPolicy":{"authorizedDownstream":{"bytesPerSecond":u32::MAX,"datagramsPerSecond":u32::MAX}}})).unwrap();
    let mut message = (body.len() as u32).to_be_bytes().to_vec();
    message.extend_from_slice(&body);
    send.write_all(&message).await.unwrap();
    send.finish().unwrap();
    let size = timeout(Duration::from_secs(1), recv.read_u32())
        .await
        .unwrap()
        .unwrap() as usize;
    assert!(size < 16384);
    let mut bytes = vec![0; size];
    recv.read_exact(&mut bytes).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(result["error"], "protocol");
    assert_eq!(host.metrics().udp_associations_opened, 0);
    assert_eq!(host.metrics().udp_forwarded, 0);
    g.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lease_deadline_is_absolute_even_with_continuous_authorized_ingress_and_downstream() {
    let endpoint = Arc::new(UdpSocket::bind(local()).await.unwrap());
    let key = generate_key();
    let host = host(
        endpoint.local_addr().unwrap(),
        &key,
        100,
        Duration::from_millis(1500),
    )
    .await;
    let g = guest(&host, key).await;
    let a = g.open_udp("one", 1).await.unwrap();
    let server = endpoint.clone();
    let producer = tokio::spawn(async move {
        let mut b = [0; 32];
        while let Ok((_, source)) = server.recv_from(&mut b).await {
            let _ = server.send_to(&[7; 100], source).await;
        }
    });
    let pumping = a.clone();
    let pumping = tokio::spawn(async move {
        while pumping.send(b"keepalive").await.is_ok() {
            if !matches!(
                timeout(Duration::from_millis(150), pumping.recv()).await,
                Ok(Ok(_))
            ) {
                break;
            }
            sleep(Duration::from_millis(40)).await;
        }
    });
    timeout(Duration::from_secs(2), g.connection().closed())
        .await
        .expect("guest activity renewed an absolute lease");
    pumping.await.unwrap();
    assert!(host.metrics().udp_forwarded > 5);
    assert!(host.metrics().udp_returned > 5);
    assert!(a.send(b"expired").await.is_err());
    g.shutdown().await;
    host.shutdown().await;
    assert_eq!(host.metrics().active_udp, 0);
    producer.abort();
}
