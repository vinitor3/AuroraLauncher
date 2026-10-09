//! Local lifecycle observations, not remote-delivery or voice attestations.
//! Three owned loopback pairs exchange 4096 UDP bytes each way: 24576 bytes
//! total across this file. No Java, external path, fixed port or kit is used.
use aurora_connect_lab::*;
use std::{future::Future, net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    time::{sleep, timeout},
};

const DEADLINE: Duration = Duration::from_secs(5);

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    timeout(DEADLINE, future)
        .await
        .expect("udp_lifecycle_deadline")
}

struct Pair {
    host: Host,
    guest: Guest,
    tcp_endpoint: TcpListener,
    udp_endpoint: UdpSocket,
    transport_addresses: Vec<SocketAddr>,
}

impl Pair {
    async fn new() -> Self {
        let tcp_endpoint = TcpListener::bind(local()).await.unwrap();
        let udp_endpoint = UdpSocket::bind(local()).await.unwrap();
        let services = vec![
            Service {
                service_id: "game".into(),
                protocol: Protocol::Tcp,
                endpoint: tcp_endpoint.local_addr().unwrap(),
                revision: 1,
                required_capability: Capability::ReliableStream,
                max_datagram_bytes: 1400,
                udp_return_policy: UdpReturnPolicy::ReplyBudget,
            },
            Service {
                service_id: "aux".into(),
                protocol: Protocol::Udp,
                endpoint: udp_endpoint.local_addr().unwrap(),
                revision: 1,
                required_capability: Capability::FragmentedDatagram,
                max_datagram_bytes: 4096,
                udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                    bytes_per_second: 16384,
                    datagrams_per_second: 20,
                },
            },
        ];
        let reserved = std::net::UdpSocket::bind(local()).unwrap();
        let bind_addr = reserved.local_addr().unwrap();
        let host_key = generate_key();
        let guest_key = generate_key();
        let invitation = Invitation::issue_fragmented_for_lifetime(
            &host_key,
            guest_key.public(),
            "udp-bridge-lifecycle",
            bind_addr,
            &services,
            Duration::from_secs(30),
        )
        .unwrap();
        let verified = Invitation::verify_fragmented(
            &invitation.encode(),
            host_key.public(),
            guest_key.public(),
        )
        .unwrap();
        let limits = Limits {
            handshake_timeout: DEADLINE,
            ..Limits::default()
        };
        drop(reserved);
        let host = bounded(Host::start_invited_fragmented(
            HostConfig {
                secret_key: host_key,
                bind_addr,
                session_id: invitation.session_id().into(),
                services,
                leases: vec![invitation.lease()],
                limits: limits.clone(),
            },
            &invitation,
        ))
        .await
        .unwrap();
        let guest = bounded(Guest::connect_invited_fragmented(
            guest_key, &verified, limits,
        ))
        .await
        .unwrap();
        let mut transport_addresses = host.bound_sockets();
        transport_addresses.extend(guest.bound_sockets());
        Self {
            host,
            guest,
            tcp_endpoint,
            udp_endpoint,
            transport_addresses,
        }
    }

    async fn udp_exchange(&self, bridge: &UdpBridge) {
        let source = UdpSocket::bind(local()).await.unwrap();
        let payload = vec![0x71; 4096];
        let mut destination_bytes = vec![0; 4097];
        let mut returned = vec![0; 4097];
        bounded(async {
            assert_eq!(
                source.send_to(&payload, bridge.local_addr()).await.unwrap(),
                4096
            );
            let (size, association) = self
                .udp_endpoint
                .recv_from(&mut destination_bytes)
                .await
                .unwrap();
            assert_eq!(&destination_bytes[..size], payload.as_slice());
            assert_eq!(
                self.udp_endpoint
                    .send_to(&payload, association)
                    .await
                    .unwrap(),
                4096
            );
            let (size, from) = source.recv_from(&mut returned).await.unwrap();
            assert_eq!(from, bridge.local_addr());
            assert_eq!(&returned[..size], payload.as_slice());
        })
        .await;
    }

    async fn tcp_stream(&self, bridge: &TcpBridge) -> (TcpStream, TcpStream) {
        bounded(async {
            let stream = TcpStream::connect(bridge.local_addr()).await.unwrap();
            let (destination, _) = self.tcp_endpoint.accept().await.unwrap();
            (stream, destination)
        })
        .await
    }

    async fn shutdown(self) {
        bounded(self.guest.shutdown()).await;
        bounded(self.host.shutdown()).await;
        // The transport endpoint owns OS sockets until its last handle drops.
        // Bridge socket release was already asserted with that handle retained.
        let addresses = self.transport_addresses.clone();
        drop(self);
        for address in &addresses {
            drop(std::net::UdpSocket::bind(address).expect("transport_socket_rebound"));
        }
    }
}

async fn tcp_exchange(source: &mut TcpStream, destination: &mut TcpStream, byte: u8) {
    bounded(async {
        let expected = [byte; 256];
        let mut received = [0; 256];
        source.write_all(&expected).await.unwrap();
        destination.read_exact(&mut received).await.unwrap();
        assert_eq!(received, expected);
        destination.write_all(&expected).await.unwrap();
        source.read_exact(&mut received).await.unwrap();
        assert_eq!(received, expected);
    })
    .await;
}

async fn application_closed(stream: &mut TcpStream) {
    assert!(matches!(
        bounded(stream.read(&mut [0])).await,
        Ok(0) | Err(_)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutting_only_udp_retains_handle_reports_closed_and_preserves_real_tcp() {
    let pair = Pair::new().await;
    let udp = bounded(pair.guest.udp_bridge_fragmented("aux", 1, local()))
        .await
        .unwrap();
    let tcp = bounded(pair.guest.tcp_bridge("game", 1, local()))
        .await
        .unwrap();
    assert!(!udp.is_closed());
    assert_eq!(udp.max_payload(), Some(4096));
    pair.udp_exchange(&udp).await;
    let (mut source, mut destination) = pair.tcp_stream(&tcp).await;
    tcp_exchange(&mut source, &mut destination, 0x41).await;

    bounded(udp.shutdown()).await;
    assert!(udp.is_closed());
    // Keep the bridge handle alive: dropping it must not provide the cleanup.
    drop(std::net::UdpSocket::bind(udp.local_addr()).expect("udp_bridge_rebound"));
    assert!(pair.guest.connection().close_reason().is_none());
    tcp_exchange(&mut source, &mut destination, 0x42).await;
    assert!(udp.is_closed());

    bounded(pair.guest.shutdown()).await;
    application_closed(&mut source).await;
    application_closed(&mut destination).await;
    bounded(tcp.shutdown()).await;
    drop(std::net::TcpListener::bind(tcp.local_addr()).expect("tcp_bridge_rebound"));
    drop(tcp);
    drop(udp);
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_or_guest_closure_is_observable_without_dropping_udp_bridge_handle() {
    for close_host in [true, false] {
        let pair = Pair::new().await;
        let udp = bounded(pair.guest.udp_bridge_fragmented("aux", 1, local()))
            .await
            .unwrap();
        assert!(!udp.is_closed());
        pair.udp_exchange(&udp).await;
        if close_host {
            bounded(pair.host.shutdown()).await;
        } else {
            bounded(pair.guest.shutdown()).await;
        }
        bounded(async {
            while !udp.is_closed() {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        // is_closed describes cancellation/completion. Await shutdown separately
        // before claiming that the operating-system socket has been released.
        bounded(udp.shutdown()).await;
        assert!(udp.is_closed());
        drop(std::net::UdpSocket::bind(udp.local_addr()).expect("udp_bridge_rebound"));
        drop(udp);
        pair.shutdown().await;
    }
}
