use aurora_connect_lab::*;
use std::{
    collections::BTreeSet,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    task::JoinHandle,
    time::{sleep, timeout},
};

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn service(id: &str, protocol: Protocol, endpoint: SocketAddr) -> Service {
    Service {
        service_id: id.into(),
        protocol,
        endpoint,
        revision: 1,
        max_datagram_bytes: 1400,
        required_capability: if protocol == Protocol::Tcp {
            Capability::ReliableStream
        } else {
            Capability::Datagram
        },
    }
}
fn lease(id: &str, key: &SecretKey, services: &[&str]) -> Lease {
    Lease {
        lease_id: id.into(),
        peer: key.public(),
        expires_at_unix_ms: unix_millis() + 60_000,
        services: services
            .iter()
            .map(|s| (*s).into())
            .collect::<BTreeSet<_>>(),
    }
}
async fn start_host(
    services: Vec<Service>,
    leases: Vec<Lease>,
    limits: Limits,
    session: &str,
) -> Host {
    Host::start(HostConfig {
        secret_key: generate_key(),
        bind_addr: local(),
        session_id: session.into(),
        services,
        leases,
        limits,
    })
    .await
    .unwrap()
}
async fn connect(
    host: &Host,
    key: SecretKey,
    lease_id: &str,
    session: &str,
    caps: Vec<Capability>,
    limits: Limits,
) -> Result<Guest> {
    Guest::connect(GuestConfig {
        secret_key: key,
        bind_addr: local(),
        host: host.addr(),
        session_id: session.into(),
        lease_id: lease_id.into(),
        capabilities: caps,
        limits,
    })
    .await
}
fn both() -> Vec<Capability> {
    vec![Capability::ReliableStream, Capability::Datagram]
}
struct Echo {
    tcp: SocketAddr,
    udp: SocketAddr,
    accepts: Arc<AtomicUsize>,
    tasks: Vec<JoinHandle<()>>,
}
impl Drop for Echo {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
async fn echo(read_delay: Duration, udp_delay: Duration, xor: u8) -> Echo {
    echo_on(local(), read_delay, udp_delay, xor).await
}
async fn echo_on(bind: SocketAddr, read_delay: Duration, udp_delay: Duration, xor: u8) -> Echo {
    let tcp = TcpListener::bind(bind).await.unwrap();
    let udp = Arc::new(UdpSocket::bind(bind).await.unwrap());
    let tcp_addr = tcp.local_addr().unwrap();
    let udp_addr = udp.local_addr().unwrap();
    let accepts = Arc::new(AtomicUsize::new(0));
    let counts = accepts.clone();
    let t = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                value=tcp.accept()=> { let Ok((mut socket,_))=value else { break }; counts.fetch_add(1,Ordering::SeqCst);
                    connections.spawn(async move { sleep(read_delay).await; let (mut read, mut write)=socket.split(); let _=tokio::io::copy(&mut read,&mut write).await; let _=write.shutdown().await; }); },
                _=connections.join_next(),if !connections.is_empty()=>{},
            }
        }
    });
    let u = tokio::spawn(async move {
        let mut buffer = vec![0; 65536];
        loop {
            let Ok((n, source)) = udp.recv_from(&mut buffer).await else {
                break;
            };
            sleep(udp_delay).await;
            for byte in &mut buffer[..n] {
                *byte ^= xor;
            }
            if udp.send_to(&buffer[..n], source).await.is_err() {
                break;
            }
        }
    });
    Echo {
        tcp: tcp_addr,
        udp: udp_addr,
        accepts,
        tasks: vec![t, u],
    }
}
async fn exchange(address: SocketAddr, payload: Vec<u8>) {
    timeout(Duration::from_secs(20), async {
        let mut s = TcpStream::connect(address).await.unwrap();
        let (mut r, mut w) = s.split();
        let send = async {
            w.write_all(&payload).await.unwrap();
            w.shutdown().await.unwrap();
        };
        let receive = async {
            let mut bytes = Vec::new();
            r.read_to_end(&mut bytes).await.unwrap();
            bytes
        };
        let (_, bytes) = tokio::join!(send, receive);
        assert_eq!(bytes, payload);
    })
    .await
    .unwrap();
}
async fn udp_exchange(socket: &UdpSocket, payload: &[u8], xor: u8) {
    socket.send(payload).await.unwrap();
    let mut buffer = vec![0; 65536];
    let n = timeout(Duration::from_secs(3), socket.recv(&mut buffer))
        .await
        .unwrap_or_else(|_| panic!("udp_echo_deadline payload_bytes={}", payload.len()))
        .unwrap();
    assert_eq!(
        &buffer[..n],
        payload.iter().map(|b| b ^ xor).collect::<Vec<_>>()
    );
}
async fn udp_client(address: SocketAddr) -> UdpSocket {
    let s = UdpSocket::bind(local()).await.unwrap();
    s.connect(address).await.unwrap();
    s
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tcp_exact_large_duplex_half_close_and_independent_connections() {
    let echo = echo(Duration::ZERO, Duration::ZERO, 0).await;
    let key = generate_key();
    let host = start_host(
        vec![service("game", Protocol::Tcp, echo.tcp)],
        vec![lease("p1", &key, &["game"])],
        Limits::default(),
        "session",
    )
    .await;
    let guest = connect(&host, key, "p1", "session", both(), Limits::default())
        .await
        .unwrap();
    let bridge = guest.tcp_bridge("game", 1, local()).await.unwrap();
    let mut tasks = tokio::task::JoinSet::new();
    for marker in 0..4u8 {
        let addr = bridge.local_addr();
        tasks.spawn(exchange(addr, vec![marker; 262144]));
    }
    while let Some(r) = tasks.join_next().await {
        r.unwrap();
    }
    assert_eq!(echo.accepts.load(Ordering::SeqCst), 4);
    bridge.shutdown().await;
    guest.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tcp_slow_reader_applies_backpressure_without_losing_bytes() {
    let echo = echo(Duration::from_millis(350), Duration::ZERO, 0).await;
    let key = generate_key();
    let host = start_host(
        vec![service("game", Protocol::Tcp, echo.tcp)],
        vec![lease("p1", &key, &["game"])],
        Limits::default(),
        "session",
    )
    .await;
    let guest = connect(&host, key, "p1", "session", both(), Limits::default())
        .await
        .unwrap();
    let (mut write, mut read) = guest.open_tcp("game", 1).await.unwrap();
    let payload = vec![0xa5; 8 * 1024 * 1024];
    let send = async {
        write.write_all(&payload).await.unwrap();
        write.finish().unwrap();
    };
    tokio::pin!(send);
    assert!(
        timeout(Duration::from_millis(80), &mut send).await.is_err(),
        "producer unexpectedly buffered the whole load"
    );
    let receive = async {
        let mut bytes = Vec::new();
        read.read_to_end(9 * 1024 * 1024)
            .await
            .map(|v| bytes = v)
            .unwrap();
        bytes
    };
    let (_, bytes) = timeout(Duration::from_secs(20), async {
        tokio::join!(send, receive)
    })
    .await
    .unwrap();
    assert_eq!(bytes, payload);
    guest.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invalid_peer_lease_session_service_revision_and_capability_never_dial_destination() {
    let echo = echo(Duration::ZERO, Duration::ZERO, 0).await;
    let key = generate_key();
    let host = start_host(
        vec![
            service("game", Protocol::Tcp, echo.tcp),
            service("aux", Protocol::Udp, echo.udp),
        ],
        vec![lease("p1", &key, &["game"])],
        Limits::default(),
        "session",
    )
    .await;
    for (k, l, s) in [
        (generate_key(), "p1", "session"),
        (key.clone(), "wrong", "session"),
        (key.clone(), "p1", "wrong"),
    ] {
        assert!(
            connect(&host, k, l, s, both(), Limits::default())
                .await
                .is_err()
        );
    }
    let guest = connect(
        &host,
        key.clone(),
        "p1",
        "session",
        vec![],
        Limits::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        guest.open_tcp("game", 1).await.err(),
        Some(Error::CapabilityUnavailable)
    );
    guest.shutdown().await;
    timeout(Duration::from_secs(2), async {
        while host.metrics().active_peers != 0 {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let guest = connect(&host, key, "p1", "session", both(), Limits::default())
        .await
        .unwrap();
    assert_eq!(
        guest.open_tcp("missing", 1).await.err(),
        Some(Error::ServiceDenied)
    );
    assert_eq!(
        guest.open_tcp("game", 2).await.err(),
        Some(Error::StaleRevision)
    );
    assert!(guest.udp_bridge("aux", 1, local()).await.is_err());
    assert_eq!(host.metrics().destination_tcp_connects, 0);
    assert_eq!(host.metrics().udp_associations_opened, 0);
    assert_eq!(echo.accepts.load(Ordering::SeqCst), 0);
    guest.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_host_pin_is_rejected_before_any_destination_io() {
    let echo = echo(Duration::ZERO, Duration::ZERO, 0).await;
    let key = generate_key();
    let host = start_host(
        vec![service("game", Protocol::Tcp, echo.tcp)],
        vec![lease("p1", &key, &["game"])],
        Limits::default(),
        "session",
    )
    .await;
    let mut address = host.addr();
    address.id = generate_key().public();
    let limits = Limits {
        handshake_timeout: Duration::from_millis(400),
        ..Limits::default()
    };
    assert!(
        Guest::connect(GuestConfig {
            secret_key: key,
            bind_addr: local(),
            host: address,
            session_id: "session".into(),
            lease_id: "p1".into(),
            capabilities: both(),
            limits
        })
        .await
        .is_err()
    );
    assert_eq!(echo.accepts.load(Ordering::SeqCst), 0);
    assert_eq!(host.metrics().destination_tcp_connects, 0);
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn revocation_stops_live_tcp_udp_and_releases_sockets() {
    let echo = echo(Duration::ZERO, Duration::ZERO, 0).await;
    let key = generate_key();
    let host = start_host(
        vec![
            service("game", Protocol::Tcp, echo.tcp),
            service("aux", Protocol::Udp, echo.udp),
        ],
        vec![lease("p1", &key, &["game", "aux"])],
        Limits::default(),
        "session",
    )
    .await;
    let guest = connect(&host, key, "p1", "session", both(), Limits::default())
        .await
        .unwrap();
    let tcp = guest.tcp_bridge("game", 1, local()).await.unwrap();
    let udp = guest.udp_bridge("aux", 1, local()).await.unwrap();
    let tcp_addr = tcp.local_addr();
    let udp_addr = udp.local_addr();
    let mut socket = TcpStream::connect(tcp_addr).await.unwrap();
    socket.write_all(b"before").await.unwrap();
    let mut reply = [0; 6];
    timeout(Duration::from_secs(3), socket.read_exact(&mut reply))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&reply, b"before");
    let u = udp_client(udp_addr).await;
    udp_exchange(&u, b"before", 0).await;
    host.revoke("p1");
    let result = timeout(Duration::from_secs(3), socket.read(&mut reply))
        .await
        .unwrap();
    assert!(result.is_err() || result.unwrap() == 0);
    assert!(guest.open_tcp("game", 1).await.is_err());
    u.send(b"after").await.unwrap();
    let returned = timeout(Duration::from_millis(300), u.recv(&mut reply)).await;
    assert!(
        !matches!(returned, Ok(Ok(_))),
        "revoked session must not return payload"
    );
    tcp.shutdown().await;
    udp.shutdown().await;
    guest.shutdown().await;
    host.shutdown().await;
    assert_eq!(host.metrics().active_tcp, 0);
    assert_eq!(host.metrics().active_udp, 0);
    let _tcp = TcpListener::bind(tcp_addr).await.unwrap();
    let _udp = UdpSocket::bind(udp_addr).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn udp_boundaries_multiple_peers_services_sessions_and_tcp_simultaneously() {
    let echo1 = echo(Duration::ZERO, Duration::ZERO, 0x55).await;
    let echo2 = echo(Duration::ZERO, Duration::ZERO, 0xaa).await;
    let k1 = generate_key();
    let k2 = generate_key();
    let k3 = generate_key();
    let services = vec![
        service("game", Protocol::Tcp, echo1.tcp),
        service("a", Protocol::Udp, echo1.udp),
        service("b", Protocol::Udp, echo2.udp),
    ];
    let host = start_host(
        services.clone(),
        vec![
            lease("p1", &k1, &["game", "a", "b"]),
            lease("p2", &k2, &["a", "b"]),
        ],
        Limits::default(),
        "one",
    )
    .await;
    let other = start_host(
        services,
        vec![lease("p3", &k3, &["a", "b"])],
        Limits::default(),
        "two",
    )
    .await;
    let g1 = connect(&host, k1, "p1", "one", both(), Limits::default())
        .await
        .unwrap();
    let g2 = connect(&host, k2, "p2", "one", both(), Limits::default())
        .await
        .unwrap();
    let g3 = connect(&other, k3, "p3", "two", both(), Limits::default())
        .await
        .unwrap();
    let mut bridges = Vec::new();
    for g in [&g1, &g2, &g3] {
        bridges.push(g.udp_bridge("a", 1, local()).await.unwrap());
        bridges.push(g.udp_bridge("b", 1, local()).await.unwrap());
    }
    let tcp = g1.tcp_bridge("game", 1, local()).await.unwrap();
    let mut tasks = tokio::task::JoinSet::new();
    tasks.spawn(exchange(tcp.local_addr(), vec![4; 1048576]));
    for (i, b) in bridges.iter().enumerate() {
        let addr = b.local_addr();
        tasks.spawn(async move {
            for source in 0..2 {
                let socket = udp_client(addr).await;
                for n in [0, 1, 17, 600, 1000] {
                    udp_exchange(
                        &socket,
                        &vec![(i * 3 + source) as u8; n],
                        if i % 2 == 0 { 0x55 } else { 0xaa },
                    )
                    .await;
                }
            }
        });
    }
    while let Some(result) = tasks.join_next().await {
        assert!(
            result.is_ok(),
            "{result:?}; host={:?}; other={:?}; guests={:?}/{:?}/{:?}",
            host.metrics(),
            other.metrics(),
            g1.metrics(),
            g2.metrics(),
            g3.metrics()
        );
    }
    assert_eq!(host.metrics().udp_associations_opened, 8);
    assert_eq!(other.metrics().udp_associations_opened, 4);
    for b in bridges {
        b.shutdown().await;
    }
    tcp.shutdown().await;
    g1.shutdown().await;
    g2.shutdown().await;
    g3.shutdown().await;
    host.shutdown().await;
    other.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn udp_exact_mtu_oversize_and_local_truncation_are_atomic() {
    let echo = echo(Duration::ZERO, Duration::ZERO, 0).await;
    let key = generate_key();
    let host = start_host(
        vec![service("aux", Protocol::Udp, echo.udp)],
        vec![lease("p1", &key, &["aux"])],
        Limits::default(),
        "session",
    )
    .await;
    let guest = connect(&host, key, "p1", "session", both(), Limits::default())
        .await
        .unwrap();
    let bridge = guest.udp_bridge("aux", 1, local()).await.unwrap();
    let socket = udp_client(bridge.local_addr()).await;
    let n = bridge.max_payload().unwrap();
    assert!(n > 0);
    udp_exchange(&socket, &vec![3; n], 0).await;
    let forwarded = host.metrics().udp_forwarded;
    for size in [n + 1, 65507] {
        socket.send(&vec![9; size]).await.unwrap();
        let mut b = vec![0; 65536];
        assert!(
            timeout(Duration::from_millis(200), socket.recv(&mut b))
                .await
                .is_err()
        );
    }
    assert_eq!(host.metrics().udp_forwarded, forwarded);
    assert!(guest.metrics().udp_oversize >= 2);
    udp_exchange(&socket, b"still-usable", 0).await;
    bridge.shutdown().await;
    guest.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn expired_lease_and_non_loopback_destination_fail_closed() {
    let echo = echo(Duration::ZERO, Duration::ZERO, 0).await;
    let key = generate_key();
    let mut grant = lease("p1", &key, &["game"]);
    grant.expires_at_unix_ms = unix_millis() + 250;
    let host = start_host(
        vec![service("game", Protocol::Tcp, echo.tcp)],
        vec![grant],
        Limits::default(),
        "session",
    )
    .await;
    sleep(Duration::from_millis(350)).await;
    assert!(
        connect(&host, key, "p1", "session", both(), Limits::default())
            .await
            .is_err()
    );
    assert_eq!(echo.accepts.load(Ordering::SeqCst), 0);
    host.shutdown().await;
    assert!(
        Host::start(HostConfig {
            secret_key: generate_key(),
            bind_addr: local(),
            session_id: "x".into(),
            services: vec![service(
                "bad",
                Protocol::Tcp,
                "192.0.2.1:22".parse().unwrap()
            )],
            leases: vec![],
            limits: Limits::default()
        })
        .await
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ipv6_loopback_carries_tcp_and_udp_without_claiming_external_ipv6() {
    let bind: SocketAddr = "[::1]:0".parse().unwrap();
    let echo = echo_on(bind, Duration::ZERO, Duration::ZERO, 0).await;
    let key = generate_key();
    let host = Host::start(HostConfig {
        secret_key: generate_key(),
        bind_addr: bind,
        session_id: "ipv6".into(),
        services: vec![
            service("game", Protocol::Tcp, echo.tcp),
            service("aux", Protocol::Udp, echo.udp),
        ],
        leases: vec![lease("p1", &key, &["game", "aux"])],
        limits: Limits::default(),
    })
    .await
    .unwrap();
    let guest = Guest::connect(GuestConfig {
        secret_key: key,
        bind_addr: bind,
        host: host.addr(),
        session_id: "ipv6".into(),
        lease_id: "p1".into(),
        capabilities: both(),
        limits: Limits::default(),
    })
    .await
    .unwrap();
    let tcp = guest.tcp_bridge("game", 1, bind).await.unwrap();
    let udp = guest.udp_bridge("aux", 1, bind).await.unwrap();
    exchange(tcp.local_addr(), vec![81; 65537]).await;
    let socket = UdpSocket::bind(bind).await.unwrap();
    socket.connect(udp.local_addr()).await.unwrap();
    udp_exchange(&socket, b"ipv6-only-loopback", 0).await;
    guest.shutdown().await;
    tcp.shutdown().await;
    udp.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tcp_session_limit_refuses_before_destination_and_releases_after_half_close() {
    let echo = echo(Duration::ZERO, Duration::ZERO, 0).await;
    let key1 = generate_key();
    let key2 = generate_key();
    let limits = Limits {
        max_tcp_total: 1,
        max_tcp_per_peer: 1,
        ..Limits::default()
    };
    let host = start_host(
        vec![service("game", Protocol::Tcp, echo.tcp)],
        vec![lease("p1", &key1, &["game"]), lease("p2", &key2, &["game"])],
        limits,
        "session",
    )
    .await;
    let g1 = connect(&host, key1, "p1", "session", both(), Limits::default())
        .await
        .unwrap();
    let g2 = connect(&host, key2, "p2", "session", both(), Limits::default())
        .await
        .unwrap();
    let (mut send, mut recv) = g1.open_tcp("game", 1).await.unwrap();
    send.write_all(b"held").await.unwrap();
    let mut bytes = [0; 4];
    recv.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"held");
    assert_eq!(g1.open_tcp("game", 1).await.err(), Some(Error::Limit));
    assert_eq!(g2.open_tcp("game", 1).await.err(), Some(Error::Limit));
    assert_eq!(host.metrics().destination_tcp_connects, 1);
    send.finish().unwrap();
    assert!(recv.read_to_end(1).await.unwrap().is_empty());
    timeout(Duration::from_secs(2), async {
        while host.metrics().active_tcp > 0 {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let (mut send, mut recv) = g2.open_tcp("game", 1).await.unwrap();
    send.write_all(b"after").await.unwrap();
    send.finish().unwrap();
    assert_eq!(recv.read_to_end(8).await.unwrap(), b"after");
    g1.shutdown().await;
    g2.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tcp_silent_destination_times_out_and_cleans_up() {
    let echo = echo(Duration::ZERO, Duration::ZERO, 0).await;
    let key = generate_key();
    let limits = Limits {
        idle_timeout: Duration::from_millis(400),
        ..Limits::default()
    };
    let host = start_host(
        vec![service("game", Protocol::Tcp, echo.tcp)],
        vec![lease("p1", &key, &["game"])],
        limits.clone(),
        "session",
    )
    .await;
    let guest = connect(&host, key, "p1", "session", both(), limits)
        .await
        .unwrap();
    let (_send, mut recv) = guest.open_tcp("game", 1).await.unwrap();
    let result = timeout(Duration::from_secs(3), recv.read_to_end(1))
        .await
        .unwrap();
    assert!(result.is_err() || result.unwrap().is_empty());
    guest.shutdown().await;
    host.shutdown().await;
    assert_eq!(host.metrics().active_tcp, 0);
}
