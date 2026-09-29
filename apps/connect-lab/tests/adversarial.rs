//! Independent negative-path tests. Only ephemeral loopback listeners and synthetic bytes.
use aurora_connect_lab::*;
use bytes::Bytes;
use serde_json::json;
use std::{collections::BTreeSet, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UdpSocket},
    task::JoinHandle,
    time::{sleep, timeout},
};

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn limits() -> Limits {
    Limits {
        handshake_timeout: Duration::from_secs(3),
        ..Limits::default()
    }
}
fn grant(id: &str, key: &SecretKey, expires: u64) -> Lease {
    Lease {
        lease_id: id.into(),
        peer: key.public(),
        expires_at_unix_ms: expires,
        services: BTreeSet::from(["game".into(), "voice".into()]),
    }
}
struct Fixture {
    tcp: TcpListener,
    udp: Arc<UdpSocket>,
}
impl Fixture {
    async fn new() -> Self {
        Self {
            tcp: TcpListener::bind(local()).await.unwrap(),
            udp: Arc::new(UdpSocket::bind(local()).await.unwrap()),
        }
    }
    async fn host(&self, grants: Vec<Lease>, limits: Limits) -> Host {
        Host::start(HostConfig {
            secret_key: generate_key(),
            bind_addr: local(),
            session_id: "adversarial".into(),
            limits,
            leases: grants,
            services: vec![
                Service {
                    service_id: "game".into(),
                    protocol: Protocol::Tcp,
                    endpoint: self.tcp.local_addr().unwrap(),
                    revision: 1,
                    max_datagram_bytes: 1400,
                    required_capability: Capability::ReliableStream,
                },
                Service {
                    service_id: "voice".into(),
                    protocol: Protocol::Udp,
                    endpoint: self.udp.local_addr().unwrap(),
                    revision: 1,
                    max_datagram_bytes: 1400,
                    required_capability: Capability::Datagram,
                },
            ],
        })
        .await
        .unwrap()
    }
    fn echo(&self) -> JoinHandle<()> {
        let socket = self.udp.clone();
        tokio::spawn(async move {
            let mut b = vec![0; 65536];
            while let Ok((n, source)) = socket.recv_from(&mut b).await {
                socket.send_to(&b[..n], source).await.unwrap();
            }
        })
    }
}
async fn guest(host: &Host, key: SecretKey, id: &str, limits: Limits) -> Guest {
    Guest::connect(GuestConfig {
        secret_key: key,
        bind_addr: local(),
        host: host.addr(),
        session_id: "adversarial".into(),
        lease_id: id.into(),
        capabilities: vec![Capability::ReliableStream, Capability::Datagram],
        limits,
    })
    .await
    .unwrap()
}
async fn wait_until(mut condition: impl FnMut() -> bool) {
    timeout(Duration::from_secs(2), async {
        while !condition() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
async fn raw_request(g: &Guest, id: u64) -> serde_json::Value {
    let (mut send, mut recv) = g.connection().open_bi().await.unwrap();
    let bytes = serde_json::to_vec(&json!({"schemaVersion":1,"requestId":id,"serviceId":"voice","serviceRevision":1,"protocol":"udp","operation":"open","associationId":0})).unwrap();
    send.write_u32(bytes.len() as u32).await.unwrap();
    send.write_all(&bytes).await.unwrap();
    send.finish().unwrap();
    let size = timeout(Duration::from_secs(2), recv.read_u32())
        .await
        .unwrap()
        .unwrap() as usize;
    assert!(size <= 16384);
    let mut reply = vec![0; size];
    recv.read_exact(&mut reply).await.unwrap();
    serde_json::from_slice(&reply).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn revocation_interrupts_incomplete_control_header_before_its_deadline() {
    let fixture = Fixture::new().await;
    let key = generate_key();
    let host = fixture
        .host(vec![grant("p", &key, unix_millis() + 30_000)], limits())
        .await;
    let g = guest(&host, key, "p", limits()).await;
    let (mut send, _recv) = g.connection().open_bi().await.unwrap();
    send.write_u32(200).await.unwrap();
    send.write_all(b"{").await.unwrap();
    sleep(Duration::from_millis(30)).await;
    host.revoke("p");
    timeout(Duration::from_millis(600), g.connection().closed())
        .await
        .expect("revocation blocked behind incomplete control header");
    wait_until(|| host.metrics().active_peers == 0).await;
    assert_eq!(host.metrics().destination_tcp_connects, 0);
    assert_eq!(host.metrics().udp_associations_opened, 0);
    g.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lease_expiration_closes_live_tcp_and_udp_without_further_destination_access() {
    let fixture = Fixture::new().await;
    let echo = fixture.echo();
    let key = generate_key();
    let host = fixture
        .host(vec![grant("p", &key, unix_millis() + 2000)], limits())
        .await;
    let g = guest(&host, key, "p", limits()).await;
    let (_tcp_send, mut tcp_recv) = g.open_tcp("game", 1).await.unwrap();
    let (_held_destination, _) = fixture.tcp.accept().await.unwrap();
    let association = g.open_udp("voice", 1).await.unwrap();
    association.send(b"alive").await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(1), association.recv())
            .await
            .unwrap()
            .unwrap(),
        &b"alive"[..]
    );
    timeout(Duration::from_secs(3), g.connection().closed())
        .await
        .unwrap();
    let mut b = [0; 1];
    assert!(
        timeout(Duration::from_millis(300), tcp_recv.read(&mut b))
            .await
            .unwrap()
            .is_err()
    );
    assert!(association.send(b"expired").await.is_err());
    wait_until(|| {
        host.metrics().active_tcp == 0
            && host.metrics().active_udp == 0
            && host.metrics().active_peers == 0
    })
    .await;
    assert_eq!(host.metrics().destination_tcp_connects, 1);
    assert_eq!(host.metrics().udp_forwarded, 1);
    g.shutdown().await;
    host.shutdown().await;
    echo.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn absent_datagram_transport_support_refuses_udp_before_opening_host_socket() {
    let fixture = Fixture::new().await;
    let key = generate_key();
    let host = fixture
        .host(vec![grant("p", &key, unix_millis() + 30_000)], limits())
        .await;
    let g = guest(
        &host,
        key,
        "p",
        Limits {
            datagram_receive_buffer: None,
            ..limits()
        },
    )
    .await;
    assert!(matches!(
        g.open_udp("voice", 1).await,
        Err(Error::CapabilityUnavailable)
    ));
    assert_eq!(host.metrics().udp_associations_opened, 0);
    let (_send, _recv) = g.open_tcp("game", 1).await.unwrap();
    assert_eq!(host.metrics().destination_tcp_connects, 1);
    g.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn udp_cross_peer_hijack_replay_generation_and_service_forgery_do_not_reach_destination() {
    let fixture = Fixture::new().await;
    let echo = fixture.echo();
    let k1 = generate_key();
    let k2 = generate_key();
    let host = fixture
        .host(
            vec![
                grant("p1", &k1, unix_millis() + 30_000),
                grant("p2", &k2, unix_millis() + 30_000),
            ],
            limits(),
        )
        .await;
    let g1 = guest(&host, k1, "p1", limits()).await;
    let g2 = guest(&host, k2, "p2", limits()).await;
    let a = g1.open_udp("voice", 1).await.unwrap();
    let frame = DatagramFrame {
        association_id: a.association_id(),
        service_handle: a.service_handle(),
        generation: a.generation(),
        sequence: 10,
        payload: Bytes::from_static(b"authorized"),
    };
    // Another authenticated connection has no association with these handles.
    g2.connection()
        .send_datagram(frame.encode().unwrap())
        .unwrap();
    wait_until(|| host.metrics().udp_dropped >= 1).await;
    assert_eq!(host.metrics().udp_forwarded, 0);
    for bad in [
        DatagramFrame {
            generation: frame.generation + 1,
            ..frame.clone()
        },
        DatagramFrame {
            service_handle: frame.service_handle + 1,
            ..frame.clone()
        },
    ] {
        g1.connection()
            .send_datagram(bad.encode().unwrap())
            .unwrap();
    }
    wait_until(|| host.metrics().udp_dropped >= 3).await;
    assert_eq!(host.metrics().udp_forwarded, 0);
    g1.connection()
        .send_datagram(frame.encode().unwrap())
        .unwrap();
    assert_eq!(
        timeout(Duration::from_secs(1), a.recv())
            .await
            .unwrap()
            .unwrap(),
        frame.payload
    );
    g1.connection()
        .send_datagram(frame.encode().unwrap())
        .unwrap();
    wait_until(|| host.metrics().udp_dropped >= 4).await;
    assert_eq!(host.metrics().udp_forwarded, 1);
    // Reordering within the replay window remains legal, duplicates remain illegal.
    let older = DatagramFrame {
        sequence: 9,
        payload: Bytes::from_static(b"reordered"),
        ..frame
    };
    g1.connection()
        .send_datagram(older.encode().unwrap())
        .unwrap();
    assert_eq!(
        timeout(Duration::from_secs(1), a.recv())
            .await
            .unwrap()
            .unwrap(),
        older.payload
    );
    assert_eq!(host.metrics().udp_forwarded, 2);
    g1.shutdown().await;
    g2.shutdown().await;
    host.shutdown().await;
    echo.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn repeated_control_request_is_rejected_without_opening_second_association() {
    let fixture = Fixture::new().await;
    let key = generate_key();
    let host = fixture
        .host(vec![grant("p", &key, unix_millis() + 30_000)], limits())
        .await;
    let g = guest(&host, key, "p", limits()).await;
    assert!(raw_request(&g, 40).await["error"].is_null());
    assert_eq!(raw_request(&g, 40).await["error"], "replay");
    assert_eq!(host.metrics().udp_associations_opened, 1);
    g.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_guest_udp_opens_obey_local_limit_even_when_host_limit_is_larger() {
    let fixture = Fixture::new().await;
    let key = generate_key();
    let host = fixture
        .host(vec![grant("p", &key, unix_millis() + 30_000)], limits())
        .await;
    let g = guest(
        &host,
        key,
        "p",
        Limits {
            max_udp_per_peer: 1,
            ..limits()
        },
    )
    .await;
    let mut tasks = tokio::task::JoinSet::new();
    let barrier = Arc::new(tokio::sync::Barrier::new(8));
    for _ in 0..8 {
        let g = g.clone();
        let barrier = barrier.clone();
        tasks.spawn(async move {
            barrier.wait().await;
            g.open_udp("voice", 1).await
        });
    }
    let mut opened = vec![];
    while let Some(value) = tasks.join_next().await {
        match value.unwrap() {
            Ok(a) => opened.push(a),
            Err(e) => assert_eq!(e, Error::Limit),
        }
    }
    let count = opened.len();
    g.shutdown().await;
    host.shutdown().await;
    assert_eq!(
        count, 1,
        "guest admitted concurrent opens above its configured association cap"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unsolicited_udp_replies_do_not_extend_idle_association_lifetime() {
    let fixture = Fixture::new().await;
    let socket = fixture.udp.clone();
    let key = generate_key();
    let flooding = tokio::spawn(async move {
        let mut b = [0; 32];
        let (_, source) = socket.recv_from(&mut b).await.unwrap();
        for _ in 0..80 {
            let _ = socket.send_to(b"unsolicited", source).await;
            sleep(Duration::from_millis(10)).await;
        }
    });
    let host = fixture
        .host(
            vec![grant("p", &key, unix_millis() + 30_000)],
            Limits {
                udp_idle_timeout: Duration::from_millis(100),
                ..limits()
            },
        )
        .await;
    let g = guest(&host, key, "p", limits()).await;
    let a = g.open_udp("voice", 1).await.unwrap();
    a.send(b"start").await.unwrap();
    wait_until(|| host.metrics().udp_forwarded == 1).await;
    sleep(Duration::from_millis(350)).await;
    let active = host.metrics().active_udp;
    g.shutdown().await;
    host.shutdown().await;
    flooding.abort();
    assert_eq!(
        active, 0,
        "discarded endpoint traffic kept an idle association alive"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_peers_udp_flood_and_malformed_frames_do_not_block_other_peer_or_tcp() {
    let fixture = Fixture::new().await;
    let echo = fixture.echo();
    let k1 = generate_key();
    let k2 = generate_key();
    let host = fixture
        .host(
            vec![
                grant("p1", &k1, unix_millis() + 30_000),
                grant("p2", &k2, unix_millis() + 30_000),
            ],
            Limits {
                datagrams_per_second: 1,
                datagram_burst: 2,
                ..limits()
            },
        )
        .await;
    let noisy = guest(&host, k1, "p1", limits()).await;
    let quiet = guest(&host, k2, "p2", limits()).await;
    let a = noisy.open_udp("voice", 1).await.unwrap();
    let b = quiet.open_udp("voice", 1).await.unwrap();
    noisy
        .connection()
        .send_datagram(Bytes::from_static(b"invalid"))
        .unwrap();
    for sequence in 0..40 {
        let frame = DatagramFrame {
            service_handle: a.service_handle(),
            association_id: a.association_id(),
            generation: a.generation(),
            sequence,
            payload: Bytes::from_static(b"noise"),
        };
        noisy
            .connection()
            .send_datagram(frame.encode().unwrap())
            .unwrap();
    }
    wait_until(|| host.metrics().udp_dropped >= 10).await;
    b.send(b"quiet-peer").await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(1), b.recv())
            .await
            .unwrap()
            .unwrap(),
        &b"quiet-peer"[..]
    );
    let (_send, _recv) = quiet.open_tcp("game", 1).await.unwrap();
    assert_eq!(host.metrics().destination_tcp_connects, 1);
    assert!(
        host.metrics().udp_forwarded <= 3,
        "per-peer packet budget was bypassed"
    );
    noisy.shutdown().await;
    quiet.shutdown().await;
    host.shutdown().await;
    echo.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn udp_bridge_local_shutdown_does_not_wait_for_a_stalled_control_stream() {
    let fixture = Fixture::new().await;
    let key = generate_key();
    let host = fixture
        .host(vec![grant("p", &key, unix_millis() + 30_000)], limits())
        .await;
    let g = guest(&host, key, "p", limits()).await;
    let bridge = g.udp_bridge("voice", 1, local()).await.unwrap();
    let local_address = bridge.local_addr();
    let (mut stalled, _recv) = g.connection().open_bi().await.unwrap();
    stalled.write_u32(200).await.unwrap();
    stalled.write_all(b"{").await.unwrap();
    sleep(Duration::from_millis(30)).await;
    let stopped = timeout(Duration::from_millis(800), bridge.shutdown())
        .await
        .is_ok();
    g.shutdown().await;
    host.shutdown().await;
    assert!(
        stopped,
        "local UDP shutdown waited for a remote control header deadline"
    );
    let _rebound = UdpSocket::bind(local_address).await.unwrap();
}
