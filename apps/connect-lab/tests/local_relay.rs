#![cfg(feature = "local-relay-lab")]

use aurora_connect_lab::*;
use std::{
    net::SocketAddr,
    num::NonZeroU32,
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
use tokio_util::sync::CancellationToken;

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn limits() -> Limits {
    Limits {
        handshake_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(3),
        ..Limits::default()
    }
}
fn service(id: &str, protocol: Protocol, address: SocketAddr) -> Service {
    Service {
        service_id: id.into(),
        protocol,
        endpoint: address,
        revision: 1,
        max_datagram_bytes: 1264,
        required_capability: if protocol == Protocol::Tcp {
            Capability::ReliableStream
        } else {
            Capability::Datagram
        },
        udp_return_policy: UdpReturnPolicy::ReplyBudget,
    }
}
fn config(key: SecretKey, peer: EndpointId, services: Vec<Service>, lease_ms: u64) -> HostConfig {
    HostConfig {
        secret_key: key,
        bind_addr: local(),
        session_id: "relay_fixture".into(),
        services,
        leases: vec![Lease {
            lease_id: "relay_lease".into(),
            peer,
            expires_at_unix_ms: unix_millis() + lease_ms,
            services: ["game".into(), "datagrams".into()].into_iter().collect(),
        }],
        limits: limits(),
    }
}
fn guest_config(key: SecretKey, host: EndpointId) -> GuestConfig {
    GuestConfig {
        secret_key: key,
        bind_addr: local(),
        host: EndpointAddr::new(host),
        session_id: "relay_fixture".into(),
        lease_id: "relay_lease".into(),
        capabilities: vec![Capability::ReliableStream, Capability::Datagram],
        limits: limits(),
    }
}
fn relay_path(guest: &Guest) {
    assert!(
        guest.bound_sockets().is_empty(),
        "relay_must_not_bind_ip_transport"
    );
    let paths = guest.connection().paths();
    assert!(!paths.is_empty(), "path_sample_must_be_observed");
    assert!(
        paths.iter().all(|path| path.is_relay() && !path.is_ip()),
        "all_paths_must_be_relay_only"
    );
}
struct Echo {
    tcp: SocketAddr,
    udp: SocketAddr,
    accepts: Arc<AtomicUsize>,
    cancel: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}
impl Echo {
    async fn start() -> Self {
        let tcp = TcpListener::bind(local()).await.unwrap();
        let udp = UdpSocket::bind(local()).await.unwrap();
        let tcp_address = tcp.local_addr().unwrap();
        let udp_address = udp.local_addr().unwrap();
        let cancel = CancellationToken::new();
        let accepts = Arc::new(AtomicUsize::new(0));
        let stop = cancel.clone();
        let counts = accepts.clone();
        let tcp_task = tokio::spawn(async move {
            let mut clients = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = stop.cancelled() => break,
                    accepted = tcp.accept() => {
                        let Ok((mut socket, _)) = accepted else { break };
                        counts.fetch_add(1, Ordering::Relaxed);
                        clients.spawn(async move {
                            let (mut read, mut write) = socket.split();
                            let _ = tokio::io::copy(&mut read, &mut write).await;
                            let _ = write.shutdown().await;
                        });
                    }
                    _ = clients.join_next(), if !clients.is_empty() => {}
                }
            }
            clients.shutdown().await;
        });
        let stop = cancel.clone();
        let udp_task = tokio::spawn(async move {
            let mut buffer = [0u8; 1264];
            loop {
                tokio::select! {
                    _ = stop.cancelled() => break,
                    received = udp.recv_from(&mut buffer) => {
                        let Ok((n, source)) = received else { break };
                        for byte in &mut buffer[..n] { *byte ^= 0xA5; }
                        if udp.send_to(&buffer[..n], source).await.is_err() { break }
                    }
                }
            }
        });
        Self {
            tcp: tcp_address,
            udp: udp_address,
            accepts,
            cancel,
            tasks: vec![tcp_task, udp_task],
        }
    }
    fn services(&self) -> Vec<Service> {
        vec![
            service("game", Protocol::Tcp, self.tcp),
            service("datagrams", Protocol::Udp, self.udp),
        ]
    }
    async fn shutdown(mut self) {
        self.cancel.cancel();
        for task in self.tasks.drain(..) {
            task.await.unwrap();
        }
        drop(TcpListener::bind(self.tcp).await.unwrap());
        drop(UdpSocket::bind(self.udp).await.unwrap());
    }
}
impl Drop for Echo {
    fn drop(&mut self) {
        self.cancel.cancel();
        for task in &self.tasks {
            task.abort();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_relay_tcp_udp_two_origins_authorization_no_direct_fallback_and_cleanup() {
    let echo = Echo::start().await;
    let host_key = generate_key();
    let guest_key = generate_key();
    let relay = LocalRelayFixture::start(
        [host_key.public(), guest_key.public()],
        LocalRelayProfile::Normal,
    )
    .await
    .unwrap();
    let ticket = relay.ticket();
    let host = Host::start_local_relay(
        config(
            host_key.clone(),
            guest_key.public(),
            echo.services(),
            45_000,
        ),
        &ticket,
    )
    .await
    .unwrap();
    assert!(host.bound_sockets().is_empty());
    assert!(
        host.addr()
            .addrs
            .iter()
            .all(|a| matches!(a, iroh::TransportAddr::Relay(_)))
    );
    let mut invalid = guest_config(guest_key.clone(), host.id());
    invalid.session_id = "not_authorized".into();
    let invalid = Guest::connect_local_relay(invalid, &ticket).await;
    assert_eq!(
        invalid.err(),
        Some(Error::Unauthorized),
        "relay_auth_failure counters={:?}",
        relay.counters()
    );
    assert_eq!(echo.accepts.load(Ordering::Relaxed), 0);
    // The legacy API rejects even a valid relay address before any endpoint.
    let mut direct = guest_config(guest_key.clone(), host.id());
    direct.host = host.addr();
    assert!(matches!(
        Guest::connect(direct).await,
        Err(Error::Configuration)
    ));
    let guest = Guest::connect_local_relay(guest_config(guest_key, host.id()), &ticket)
        .await
        .unwrap();
    relay_path(&guest);
    assert!(matches!(
        guest.open_tcp("ungranted", 1).await,
        Err(Error::ServiceDenied)
    ));
    assert!(matches!(
        guest.open_tcp("game", 2).await,
        Err(Error::StaleRevision)
    ));
    assert_eq!(echo.accepts.load(Ordering::Relaxed), 0);
    let tcp = guest.tcp_bridge("game", 1, local()).await.unwrap();
    let udp = guest.udp_bridge("datagrams", 1, local()).await.unwrap();
    let tcp_address = tcp.local_addr();
    let udp_address = udp.local_addr();
    // 256KiB each direction plus two small UDP datagrams stays below1MiB.
    timeout(Duration::from_secs(12), async {
        let mut socket = TcpStream::connect(tcp_address).await.unwrap();
        let payload: Vec<u8> = (0..262144).map(|n| (n % 251) as u8).collect();
        let (mut read, mut write) = socket.split();
        let send = async {
            write.write_all(&payload).await.unwrap();
            write.shutdown().await.unwrap();
        };
        let receive = async {
            let mut received = Vec::new();
            read.read_to_end(&mut received).await.unwrap();
            received
        };
        let (_, received) = tokio::join!(send, receive);
        assert_eq!(received, payload);
    })
    .await
    .unwrap();
    relay_path(&guest);
    let first = UdpSocket::bind(local()).await.unwrap();
    let second = UdpSocket::bind(local()).await.unwrap();
    let source_addresses = [first.local_addr().unwrap(), second.local_addr().unwrap()];
    let payloads = [vec![0x17u8; 512], vec![0x63u8; 640]];
    first.send_to(&payloads[0], udp_address).await.unwrap();
    second.send_to(&payloads[1], udp_address).await.unwrap();
    for (socket, payload) in [(&first, &payloads[0]), (&second, &payloads[1])] {
        let mut received = [0u8; 1264];
        let (n, from) = timeout(Duration::from_secs(3), socket.recv_from(&mut received))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(from, udp_address);
        assert_eq!(
            received[..n],
            payload.iter().map(|b| b ^ 0xA5).collect::<Vec<_>>()
        );
    }
    relay_path(&guest);
    let counters = relay.counters();
    assert!(counters.received_bytes > 0 && counters.sent_bytes > 0);
    assert!(counters.received_packets > 0 && counters.sent_packets > 0);
    assert_eq!(counters.rate_limited_connections, 0);
    // Actual relay loss must not turn into a direct-IP success.
    relay.shutdown().await.unwrap();
    assert!(matches!(
        Guest::connect_local_relay(guest_config(generate_key(), host.id()), &ticket).await,
        Err(Error::Expired)
    ));
    timeout(Duration::from_secs(6), guest.connection().closed())
        .await
        .unwrap();
    assert!(guest.bound_sockets().is_empty());
    assert!(guest.open_tcp("game", 1).await.is_err());
    tcp.shutdown().await;
    udp.shutdown().await;
    guest.shutdown().await;
    host.shutdown().await;
    drop((tcp, udp, guest, host, first, second));
    drop(TcpListener::bind(tcp_address).await.unwrap());
    drop(UdpSocket::bind(udp_address).await.unwrap());
    for address in source_addresses {
        drop(UdpSocket::bind(address).await.unwrap());
    }
    echo.shutdown().await;
    println!(
        "{{\"event\":\"local_relay_passed\",\"tcpBytesEachDirection\":262144,\"udpSources\":2,\"udpPayloadBytesEachDirection\":1152,\"path\":\"relay_only\",\"directFallback\":false,\"cleanup\":true}}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn actual_relay_rate_limit_is_observed_not_inferred_from_timeout() {
    let echo = Echo::start().await;
    let host_key = generate_key();
    let guest_key = generate_key();
    let relay = LocalRelayFixture::start(
        [host_key.public(), guest_key.public()],
        LocalRelayProfile::RateLimited {
            bytes_per_second: NonZeroU32::new(1024).unwrap(),
            burst: NonZeroU32::new(16384).unwrap(),
        },
    )
    .await
    .unwrap();
    let ticket = relay.ticket();
    let host = Host::start_local_relay(
        config(host_key, guest_key.public(), echo.services(), 25_000),
        &ticket,
    )
    .await
    .unwrap();
    let attempt = Guest::connect_local_relay(guest_config(guest_key, host.id()), &ticket).await;
    let mut guest = None;
    if let Ok(connected) = attempt {
        relay_path(&connected);
        if let Ok((mut send, mut receive)) = connected.open_tcp("game", 1).await {
            // Bounded synthetic pressure on the actual relay RX limiter.
            let _ = timeout(Duration::from_secs(5), async {
                let send_bytes = async {
                    let _ = send.write_all(&vec![0x42; 131072]).await;
                    let _ = send.finish();
                };
                let read_bytes = async {
                    let mut buffer = [0u8; 8192];
                    while let Ok(n) = receive.read(&mut buffer).await {
                        if n.is_none() {
                            break;
                        }
                    }
                };
                tokio::join!(send_bytes, read_bytes);
            })
            .await;
        }
        guest = Some(connected);
    }
    timeout(Duration::from_secs(3), async {
        loop {
            let counters = relay.counters();
            if counters.rate_limited_connections > 0 && counters.rate_limited_bytes > 0 {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("actual_rate_limit_counter_required");
    if let Some(guest) = guest {
        guest.shutdown().await;
    }
    host.shutdown().await;
    drop(host);
    relay.shutdown().await.unwrap();
    echo.shutdown().await;
}

#[tokio::test]
async fn relay_ticket_rejects_unapproved_identity_ip_route_bind_and_unbounded_lease() {
    let host_key = generate_key();
    let guest_key = generate_key();
    let relay = LocalRelayFixture::start(
        [host_key.public(), guest_key.public()],
        LocalRelayProfile::Normal,
    )
    .await
    .unwrap();
    let ticket = relay.ticket();
    assert!(matches!(
        Guest::connect_local_relay(guest_config(generate_key(), host_key.public()), &ticket).await,
        Err(Error::Configuration)
    ));
    let mut routed = guest_config(guest_key.clone(), host_key.public());
    routed.host = routed.host.with_ip_addr("127.0.0.1:9".parse().unwrap());
    assert!(matches!(
        Guest::connect_local_relay(routed, &ticket).await,
        Err(Error::Configuration)
    ));
    let mut bound = guest_config(guest_key.clone(), host_key.public());
    bound.bind_addr = "0.0.0.0:0".parse().unwrap();
    assert!(matches!(
        Guest::connect_local_relay(bound, &ticket).await,
        Err(Error::Configuration)
    ));
    let services = vec![service(
        "game",
        Protocol::Tcp,
        "127.0.0.1:9".parse().unwrap(),
    )];
    assert!(matches!(
        Host::start_local_relay(
            config(host_key, guest_key.public(), services, 600_000),
            &ticket
        )
        .await,
        Err(Error::Configuration)
    ));
    assert_eq!(relay.counters().received_bytes, 0);
    assert_eq!(relay.counters().sent_bytes, 0);
    relay.shutdown().await.unwrap();
}
