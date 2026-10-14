//! Separate admission/session deadlines through real authenticated loopback peers.
//! UDP budgets count attempted application payload in both directions (not QUIC
//! wire bytes); every case stays below 64 KiB. No Java, relay or external network.
use aurora_connect_lab::*;
use bytes::Bytes;
use iroh::{
    Endpoint, NetReportConfig, RelayMode,
    endpoint::{
        ConnectOptions, QuicTransportConfig, RecvStream, SendStream, ZeroRttStatus, presets,
    },
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    future::Future,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    task::{JoinHandle, JoinSet},
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;

const IO_DEADLINE: Duration = Duration::from_secs(5);
const OBSERVATION: Duration = Duration::from_millis(100);
fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn limits() -> Limits {
    Limits {
        handshake_timeout: IO_DEADLINE,
        ..Limits::default()
    }
}
async fn bounded<T>(future: impl Future<Output = T>) -> T {
    timeout(IO_DEADLINE, future)
        .await
        .expect("fragmented_session_fixture_deadline")
}
async fn wait_after(at: u64) {
    sleep(Duration::from_millis(at.saturating_sub(unix_millis()) + 50)).await;
}
async fn count_reaches(counter: &AtomicUsize, expected: usize) {
    bounded(async {
        while counter.load(Ordering::SeqCst) < expected {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
}

#[derive(Default)]
struct Observations {
    tcp_accepts: AtomicUsize,
    tcp_closed: AtomicUsize,
    tcp_bytes: AtomicUsize,
    udp_packets: AtomicUsize,
    udp_bytes: AtomicUsize,
    udp_sources: Mutex<BTreeSet<SocketAddr>>,
}
struct Fixture {
    host: Option<Host>,
    invitation: SessionInvitation,
    guest_key: SecretKey,
    guests: Vec<Guest>,
    observed: Arc<Observations>,
    tcp_addresses: Vec<SocketAddr>,
    udp_addresses: Vec<SocketAddr>,
    stop: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.cancel();
        for task in &self.tasks {
            task.abort();
        }
    }
}
impl Fixture {
    async fn new(admission_ms: u64, session_ms: u64) -> Self {
        let tcp = TcpListener::bind(local()).await.unwrap();
        let udp = UdpSocket::bind(local()).await.unwrap();
        let tcp_address = tcp.local_addr().unwrap();
        let udp_address = udp.local_addr().unwrap();
        let services = vec![
            Service {
                service_id: "game".into(),
                protocol: Protocol::Tcp,
                endpoint: tcp_address,
                revision: 1,
                max_datagram_bytes: 1400,
                required_capability: Capability::ReliableStream,
                udp_return_policy: UdpReturnPolicy::ReplyBudget,
            },
            Service {
                service_id: "aux".into(),
                protocol: Protocol::Udp,
                endpoint: udp_address,
                revision: 1,
                max_datagram_bytes: 4096,
                required_capability: Capability::FragmentedDatagram,
                udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                    bytes_per_second: 16384,
                    datagrams_per_second: 20,
                },
            },
        ];
        let reservation = std::net::UdpSocket::bind(local()).unwrap();
        let bind = reservation.local_addr().unwrap();
        let host_key = generate_key();
        let guest_key = generate_key();
        let issued = SessionInvitation::issue_fragmented_loopback_for_lifetimes(
            &host_key,
            guest_key.public(),
            "fragmented-session-lifetime",
            bind,
            &services,
            Duration::from_millis(admission_ms),
            Duration::from_millis(session_ms),
        )
        .unwrap();
        let invitation = SessionInvitation::verify_fragmented_loopback(
            &issued.encode(),
            host_key.public(),
            guest_key.public(),
        )
        .unwrap();
        drop(reservation);
        let host = Host::start_invited_fragmented_session(
            HostConfig {
                secret_key: host_key,
                bind_addr: bind,
                session_id: invitation.session_id().into(),
                services,
                leases: vec![invitation.lease()],
                limits: limits(),
            },
            &invitation,
        )
        .await
        .unwrap();
        let observed = Arc::new(Observations::default());
        let stop = CancellationToken::new();
        let tcp_stop = stop.clone();
        let tcp_observed = observed.clone();
        let tcp_task = tokio::spawn(async move {
            let mut children = JoinSet::new();
            loop {
                tokio::select! {
                    _ = tcp_stop.cancelled() => break,
                    accepted = tcp.accept() => {
                        let (mut socket, _) = accepted.unwrap();
                        tcp_observed.tcp_accepts.fetch_add(1, Ordering::SeqCst);
                        let seen = tcp_observed.clone();
                        children.spawn(async move {
                            let mut bytes = [0; 4096];
                            loop {
                                let count = match socket.read(&mut bytes).await {
                                    Ok(0) | Err(_) => break,
                                    Ok(count) => count,
                                };
                                seen.tcp_bytes.fetch_add(count, Ordering::SeqCst);
                                if socket.write_all(&bytes[..count]).await.is_err() {
                                    break;
                                }
                            }
                            let _ = socket.shutdown().await;
                            seen.tcp_closed.fetch_add(1, Ordering::SeqCst);
                        });
                    }
                    completed = children.join_next(), if !children.is_empty() => {
                        completed.unwrap().unwrap();
                    }
                }
            }
            children.shutdown().await;
        });
        let udp_stop = stop.clone();
        let udp_observed = observed.clone();
        let udp_task = tokio::spawn(async move {
            let mut bytes = [0; 4098];
            loop {
                let result = tokio::select! {
                    _ = udp_stop.cancelled() => break,
                    received = udp.recv_from(&mut bytes) => received,
                };
                let (count, source) = result.unwrap();
                assert!(count <= 4096, "oversize_reached_destination");
                udp_observed.udp_sources.lock().unwrap().insert(source);
                udp_observed.udp_bytes.fetch_add(count, Ordering::SeqCst);
                udp_observed.udp_packets.fetch_add(1, Ordering::SeqCst);
                assert_eq!(udp.send_to(&bytes[..count], source).await.unwrap(), count);
            }
        });
        let mut udp_addresses = vec![udp_address];
        udp_addresses.extend(host.bound_sockets());
        Self {
            host: Some(host),
            invitation,
            guest_key,
            guests: Vec::new(),
            observed,
            tcp_addresses: vec![tcp_address],
            udp_addresses,
            stop,
            tasks: vec![tcp_task, udp_task],
        }
    }
    fn host(&self) -> &Host {
        self.host.as_ref().unwrap()
    }
    async fn guest(&mut self) -> Guest {
        let guest = Guest::connect_invited_fragmented_session(
            self.guest_key.clone(),
            &self.invitation,
            limits(),
        )
        .await
        .unwrap();
        self.udp_addresses.extend(guest.bound_sockets());
        self.guests.push(guest.clone());
        guest
    }
    async fn bridges(&mut self, guest: &Guest) -> (TcpBridge, UdpBridge) {
        let tcp = guest.tcp_bridge("game", 1, local()).await.unwrap();
        let udp = guest
            .udp_bridge_fragmented("aux", 1, local())
            .await
            .unwrap();
        assert_eq!(udp.max_payload(), Some(4096));
        self.tcp_addresses.push(tcp.local_addr());
        self.udp_addresses.push(udp.local_addr());
        (tcp, udp)
    }
    async fn origin(&mut self) -> UdpSocket {
        let socket = UdpSocket::bind(local()).await.unwrap();
        self.udp_addresses.push(socket.local_addr().unwrap());
        socket
    }
    async fn idle(&self) {
        bounded(async {
            loop {
                let m = self.host().metrics();
                if (m.active_peers, m.active_tcp, m.active_udp) == (0, 0, 0) {
                    break;
                }
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
    }
    async fn no_destinations(&self) {
        self.idle().await;
        // Echo tasks retain both destination sockets throughout the observation.
        sleep(OBSERVATION).await;
        let m = self.host().metrics();
        assert_eq!(
            (
                m.destination_tcp_connects,
                m.udp_associations_opened,
                m.udp_forwarded,
                m.udp_returned
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(self.observed.tcp_accepts.load(Ordering::SeqCst), 0);
        assert_eq!(self.observed.tcp_bytes.load(Ordering::SeqCst), 0);
        assert_eq!(self.observed.udp_packets.load(Ordering::SeqCst), 0);
        assert_eq!(self.observed.udp_bytes.load(Ordering::SeqCst), 0);
    }
    async fn raw_hello(&mut self) -> Value {
        let endpoint = Endpoint::builder(presets::Minimal)
            .clear_ip_transports()
            .clear_relay_transports()
            .clear_address_lookup()
            .relay_mode(RelayMode::Disabled)
            .net_report_config(NetReportConfig::minimal())
            .keylog(false)
            .secret_key(self.guest_key.clone())
            .bind_addr(local())
            .unwrap()
            .bind()
            .await
            .unwrap();
        self.udp_addresses.extend(endpoint.bound_sockets());
        let connection = bounded(endpoint.connect(self.host().addr(), ALPN))
            .await
            .unwrap();
        let result = bounded(async {
            let (mut send, mut recv) = connection.open_bi().await.unwrap();
            let hello = json!({"schemaVersion":1,"sessionId":self.invitation.session_id(),
                "leaseId":self.invitation.invitation_id(),"capabilities":["reliableStream","fragmentedDatagram"],
                "invitation":{"digest":self.invitation.digest(),"generation":1}});
            let bytes = serde_json::to_vec(&hello).unwrap();
            send.write_u32(bytes.len() as u32).await.unwrap();
            send.write_all(&bytes).await.unwrap();
            send.finish().unwrap();
            let n = recv.read_u32().await.unwrap() as usize;
            assert!((1..=1024).contains(&n));
            let mut reply = vec![0; n];
            recv.read_exact(&mut reply).await.unwrap();
            serde_json::from_slice(&reply).unwrap()
        })
        .await;
        connection.close(0u32.into(), b"fixture complete");
        endpoint.close().await;
        result
    }
    async fn cleanup(mut self) {
        for guest in &self.guests {
            bounded(guest.shutdown()).await;
        }
        self.guests.clear();
        let host = self.host.take().unwrap();
        bounded(host.shutdown()).await;
        let m = host.metrics();
        assert_eq!((m.active_peers, m.active_tcp, m.active_udp), (0, 0, 0));
        drop(host);
        self.stop.cancel();
        for task in self.tasks.drain(..) {
            bounded(task).await.unwrap();
        }
        self.udp_addresses
            .extend(self.observed.udp_sources.lock().unwrap().iter().copied());
        let tcp_addresses = std::mem::take(&mut self.tcp_addresses);
        let udp_addresses = std::mem::take(&mut self.udp_addresses);
        drop(self);
        let mut tcp_rebound = Vec::new();
        let mut udp_rebound = Vec::new();
        for address in tcp_addresses.into_iter().collect::<BTreeSet<_>>() {
            tcp_rebound.push(
                TcpListener::bind(address)
                    .await
                    .expect("tcp_cleanup_rebind"),
            );
        }
        for address in udp_addresses.into_iter().collect::<BTreeSet<_>>() {
            udp_rebound.push(UdpSocket::bind(address).await.expect("udp_cleanup_rebind"));
        }
    }
}

async fn tcp_echo(socket: &mut TcpStream, tag: u8, size: usize) {
    bounded(async {
        let payload = vec![tag; size];
        socket.write_all(&payload).await.unwrap();
        let mut result = vec![0; size];
        socket.read_exact(&mut result).await.unwrap();
        assert_eq!(result, payload, "tcp_changed");
    })
    .await;
}
async fn udp_echo(origin: &UdpSocket, address: SocketAddr, tag: u8, size: usize) {
    bounded(async {
        let payload = vec![tag; size];
        assert_eq!(origin.send_to(&payload, address).await.unwrap(), size);
        let mut bytes = [0; 4098];
        let (count, source) = origin.recv_from(&mut bytes).await.unwrap();
        assert!(
            source == address && count == size && bytes[..count] == payload,
            "udp_scope_or_bytes_changed"
        );
    })
    .await;
}
async fn tcp_closed(socket: &mut TcpStream) {
    let mut byte = [0; 1];
    let result = bounded(socket.read(&mut byte)).await;
    assert!(
        matches!(result, Ok(0) | Err(_)),
        "application_tcp_remained_open"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn admission_expiry_preserves_tcp_and_new_udp_sources_with_exact_isolated_returns() {
    let mut f = Fixture::new(2000, 15000).await;
    let guest = f.guest().await;
    let (tcp, udp) = f.bridges(&guest).await;
    let a = f.origin().await;
    let b = f.origin().await;
    let mut first = TcpStream::connect(tcp.local_addr()).await.unwrap();
    tcp_echo(&mut first, 0x12, 1024).await;
    udp_echo(&a, udp.local_addr(), 0x23, 4).await;
    assert_eq!(f.host().metrics().udp_associations_opened, 1);
    wait_after(f.invitation.admission_expires_at_unix_ms()).await;
    assert_eq!(f.invitation.check_admission_current(), Err(Error::Expired));
    f.invitation.check_session_current().unwrap();
    assert_eq!(
        SessionInvitation::verify_fragmented_loopback(
            &f.invitation.encode(),
            f.host().id(),
            f.guest_key.public()
        )
        .map(|_| ()),
        Err(Error::Expired)
    );
    assert!(f.host().invitation_consumed(f.invitation.invitation_id()));
    tcp_echo(&mut first, 0x34, 1024).await;
    let mut second = TcpStream::connect(tcp.local_addr()).await.unwrap();
    let start = f.observed.tcp_bytes.load(Ordering::SeqCst);
    let before_frames = guest.connection().stats();
    let (_, concurrent_tcp_bytes) = tokio::join!(
        async {
            for _ in 0..32 {
                tcp_echo(&mut second, 0x45, 4096).await;
                sleep(Duration::from_millis(5)).await;
            }
        },
        async {
            count_reaches(&f.observed.tcp_bytes, start + 4096).await;
            let before = f.observed.tcp_bytes.load(Ordering::SeqCst);
            udp_echo(&a, udp.local_addr(), 0x56, 4096).await;
            count_reaches(&f.observed.tcp_bytes, before + 4096).await;
            // B has sent nothing before expiry. This creates its association
            // under the retained session authority, not a fresh admission.
            udp_echo(&b, udp.local_addr(), 0xa7, 4096).await;
            f.observed.tcp_bytes.load(Ordering::SeqCst) - before
        }
    );
    assert!(concurrent_tcp_bytes > 0);
    assert_eq!(f.observed.udp_sources.lock().unwrap().len(), 2);
    assert_eq!(f.host().metrics().udp_associations_opened, 2);
    let after_frames = guest.connection().stats();
    assert!(after_frames.frame_tx.datagram - before_frames.frame_tx.datagram >= 8);
    assert!(after_frames.frame_rx.datagram - before_frames.frame_rx.datagram >= 8);
    let before_packets = f.observed.udp_packets.load(Ordering::SeqCst);
    let before_oversize = guest.metrics().udp_oversize;
    assert_eq!(
        a.send_to(&[0x77; 4097], udp.local_addr()).await.unwrap(),
        4097
    );
    bounded(async {
        while guest.metrics().udp_oversize == before_oversize {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    let mut bytes = [0; 4098];
    assert!(timeout(OBSERVATION, a.recv_from(&mut bytes)).await.is_err());
    assert_eq!(
        f.observed.udp_packets.load(Ordering::SeqCst),
        before_packets
    );
    udp_echo(&a, udp.local_addr(), 0x89, 4).await;
    udp_echo(&b, udp.local_addr(), 0x9a, 4).await;
    assert_eq!(f.observed.udp_packets.load(Ordering::SeqCst), 5);
    assert_eq!(f.observed.udp_bytes.load(Ordering::SeqCst), 8204);
    let udp_attempted = 8204 * 2 + 4097;
    assert!(udp_attempted <= 65536);
    assert_eq!(f.host().metrics().destination_tcp_connects, 2);
    assert!(f.host().revoke(f.invitation.invitation_id()));
    bounded(guest.connection().closed()).await;
    tcp_closed(&mut first).await;
    tcp_closed(&mut second).await;
    count_reaches(&f.observed.tcp_closed, 2).await;
    tcp.shutdown().await;
    udp.shutdown().await;
    assert!(udp.is_closed());
    drop((first, second, tcp, udp, a, b));
    guest.shutdown().await;
    drop(guest);
    f.cleanup().await;
    eprintln!(
        "{}",
        json!({"case":"fragmented_session_after_admission","udpAttemptedBytesBothDirections":udp_attempted,"udpPayloadBytesEachDirection":8204,"udpSources":2,"tcpConcurrentBytes":concurrent_tcp_bytes,"cleanupRebind":true,"internetTested":false,"voiceTested":false})
    );
}

#[tokio::test]
async fn active_fragmented_session_expires_without_activity_renewal_and_releases_bridges() {
    let mut f = Fixture::new(2000, 4000).await;
    let guest = f.guest().await;
    let (tcp, udp) = f.bridges(&guest).await;
    let origin = f.origin().await;
    let mut stream = TcpStream::connect(tcp.local_addr()).await.unwrap();
    let mut packets = 0usize;
    while f.invitation.session_remaining().unwrap() > Duration::from_millis(600) {
        tcp_echo(&mut stream, 0xa1, 128).await;
        udp_echo(&origin, udp.local_addr(), 0xb2, 8).await;
        packets += 1;
        assert!(packets <= 40, "fixture_activity_budget");
        sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(f.invitation.check_admission_current(), Err(Error::Expired));
    f.invitation.check_session_current().unwrap();
    assert!(packets > 2);
    bounded(guest.connection().closed()).await;
    assert_eq!(f.invitation.check_session_current(), Err(Error::Expired));
    assert!(unix_millis() >= f.invitation.session_expires_at_unix_ms());
    tcp_closed(&mut stream).await;
    count_reaches(&f.observed.tcp_closed, 1).await;
    bounded(async {
        while !udp.is_closed() {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    let previous = f.observed.udp_packets.load(Ordering::SeqCst);
    assert_eq!(origin.send_to(b"after", udp.local_addr()).await.unwrap(), 5);
    sleep(OBSERVATION).await;
    assert_eq!(f.observed.udp_packets.load(Ordering::SeqCst), previous);
    assert!(guest.open_tcp("game", 1).await.is_err());
    assert!(guest.open_udp_fragmented("aux", 1).await.is_err());
    assert!(packets * 16 + 5 <= 65536);
    tcp.shutdown().await;
    udp.shutdown().await;
    drop((stream, tcp, udp, origin));
    guest.shutdown().await;
    drop(guest);
    f.cleanup().await;
}

#[tokio::test]
async fn revocation_closes_active_fragmented_session_and_rejects_remote_reentry() {
    let mut f = Fixture::new(15000, 20000).await;
    let guest = f.guest().await;
    let (tcp, udp) = f.bridges(&guest).await;
    let origin = f.origin().await;
    let mut stream = TcpStream::connect(tcp.local_addr()).await.unwrap();
    tcp_echo(&mut stream, 0xc3, 512).await;
    udp_echo(&origin, udp.local_addr(), 0xd4, 8).await;
    assert!(f.host().revoke(f.invitation.invitation_id()));
    bounded(guest.connection().closed()).await;
    tcp_closed(&mut stream).await;
    count_reaches(&f.observed.tcp_closed, 1).await;
    f.invitation.check_admission_current().unwrap();
    assert_eq!(f.raw_hello().await["error"], "expired");
    f.idle().await;
    assert_eq!(f.host().metrics().denied, 1);
    assert_eq!(f.host().metrics().destination_tcp_connects, 1);
    assert_eq!(f.host().metrics().udp_associations_opened, 1);
    assert_eq!(f.observed.udp_packets.load(Ordering::SeqCst), 1);
    tcp.shutdown().await;
    udp.shutdown().await;
    assert!(udp.is_closed());
    drop((stream, tcp, udp, origin));
    guest.shutdown().await;
    drop(guest);
    f.cleanup().await;
}

#[tokio::test]
async fn consumed_fragmented_session_replay_and_revoke_never_open_destinations() {
    let mut f = Fixture::new(15000, 20000).await;
    let guest = f.guest().await;
    guest.shutdown().await;
    drop(guest);
    f.idle().await;
    f.invitation.check_admission_current().unwrap();
    assert!(matches!(
        Guest::connect_invited_fragmented_session(f.guest_key.clone(), &f.invitation, limits())
            .await,
        Err(Error::Replay)
    ));
    assert_eq!(f.host().metrics().denied, 1);
    f.no_destinations().await;
    assert!(f.host().revoke(f.invitation.invitation_id()));
    f.invitation.check_admission_current().unwrap();
    assert!(matches!(
        Guest::connect_invited_fragmented_session(f.guest_key.clone(), &f.invitation, limits())
            .await,
        Err(Error::Expired)
    ));
    assert_eq!(f.host().metrics().denied, 2);
    assert!(f.host().invitation_consumed(f.invitation.invitation_id()));
    f.no_destinations().await;
    f.cleanup().await;
}

#[tokio::test]
async fn late_fragmented_session_is_rejected_locally_and_by_host_without_consumption_or_io() {
    let mut f = Fixture::new(1000, 10000).await;
    wait_after(f.invitation.admission_expires_at_unix_ms()).await;
    f.invitation.check_session_current().unwrap();
    assert!(matches!(
        Guest::connect_invited_fragmented_session(f.guest_key.clone(), &f.invitation, limits())
            .await,
        Err(Error::Expired)
    ));
    assert_eq!(f.host().metrics().denied, 0, "local_refusal_did_not_dial");
    assert_eq!(f.raw_hello().await["error"], "expired");
    assert_eq!(f.host().metrics().denied, 1);
    assert!(!f.host().invitation_consumed(f.invitation.invitation_id()));
    f.no_destinations().await;
    f.cleanup().await;
}

fn early_hello(f: &Fixture) -> Value {
    json!({"schemaVersion":1,"sessionId":f.invitation.session_id(),
        "leaseId":f.invitation.invitation_id(),"capabilities":["reliableStream","fragmentedDatagram"],
        "invitation":{"digest":f.invitation.digest(),"generation":1}})
}
async fn early_send(send: &mut SendStream, value: &Value) {
    let bytes = serde_json::to_vec(value).unwrap();
    assert!(bytes.len() <= 4096);
    send.write_u32(bytes.len() as u32).await.unwrap();
    send.write_all(&bytes).await.unwrap();
    send.finish().unwrap();
}
async fn early_reply(recv: &mut RecvStream) -> Value {
    let size = recv.read_u32().await.unwrap() as usize;
    assert!((1..=1024).contains(&size));
    let mut bytes = vec![0; size];
    recv.read_exact(&mut bytes).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
fn early_datagram() -> Bytes {
    // Structurally valid V2 frame, but neither the early peer nor the claimed
    // service/association has been admitted. Never derive authority from it.
    let mut bytes = vec![2, 0];
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.extend_from_slice(&5u16.to_be_bytes());
    bytes.extend_from_slice(&[0, 1]);
    bytes.extend_from_slice(&5u16.to_be_bytes());
    bytes.extend_from_slice(b"early");
    bytes.into()
}

#[tokio::test]
async fn accepted_zero_rtt_fragmented_session_replay_proof_and_capabilities_never_reach_destinations()
 {
    for (case, expected) in [
        ("replay", "replay"),
        ("proof", "unauthorized"),
        ("capability", "capability_unavailable"),
    ] {
        let mut f = Fixture::new(30000, 40000).await;
        let transport = QuicTransportConfig::builder()
            .send_observed_address_reports(false)
            .receive_observed_address_reports(false)
            .build();
        let endpoint = Endpoint::builder(presets::Minimal)
            .clear_ip_transports()
            .clear_relay_transports()
            .clear_address_lookup()
            .relay_mode(RelayMode::Disabled)
            .net_report_config(NetReportConfig::minimal())
            .secret_key(f.guest_key.clone())
            .keylog(false)
            .transport_config(transport)
            .bind_addr(local())
            .unwrap()
            .bind()
            .await
            .unwrap();
        f.udp_addresses.extend(endpoint.bound_sockets());
        let mut invalid_proof = early_hello(&f);
        invalid_proof["invitation"]["digest"] = json!("0".repeat(64));
        let warm_hello = if case == "replay" {
            early_hello(&f)
        } else {
            invalid_proof.clone()
        };
        bounded(async {
            let connecting = endpoint
                .connect_with_opts(f.host().addr(), ALPN, ConnectOptions::new())
                .await
                .unwrap();
            let connection = match connecting.into_0rtt() {
                Err(connecting) => connecting.await.unwrap(),
                Ok(_) => panic!("fresh_fragmented_session_endpoint_has_ticket"),
            };
            let (mut send, mut recv) = connection.open_bi().await.unwrap();
            early_send(&mut send, &warm_hello).await;
            let reply = early_reply(&mut recv).await;
            if case == "replay" {
                assert!(reply["error"].is_null());
            } else {
                assert_eq!(reply["error"], "unauthorized");
            }
            connection.close(0u32.into(), b"fixture ticket acquired");
            connection.closed().await;
        })
        .await;
        f.no_destinations().await;
        assert_eq!(
            f.host().invitation_consumed(f.invitation.invitation_id()),
            case == "replay"
        );
        let mut hello = if case == "proof" {
            invalid_proof
        } else {
            early_hello(&f)
        };
        if case == "capability" {
            hello["capabilities"] = json!(["reliableStream", "datagram"]);
        }
        bounded(async {
            let connecting = endpoint
                .connect_with_opts(f.host().addr(), ALPN, ConnectOptions::new())
                .await
                .unwrap();
            let early = match connecting.into_0rtt() {
                Ok(early) => early,
                Err(_) => panic!("fragmented_session_resumption_ticket_missing"),
            };
            let (mut send, mut recv) = early.open_bi().await.unwrap();
            assert!(recv.is_0rtt(), "fragmented_session_hello_not_early_data");
            early_send(&mut send, &hello).await;
            let mut pending = Vec::new();
            for (id, service, protocol, operation) in [
                (1, "game", "tcp", "open"),
                (2, "aux", "udp", "openFragmented"),
            ] {
                let (mut send, recv) = early.open_bi().await.unwrap();
                early_send(
                    &mut send,
                    &json!({"schemaVersion":1,"requestId":id,
                    "serviceId":service,"serviceRevision":1,"protocol":protocol,
                    "operation":operation,"associationId":0}),
                )
                .await;
                pending.push(recv);
            }
            early.send_datagram(early_datagram()).unwrap();
            let connection = match early.handshake_completed().await.unwrap() {
                ZeroRttStatus::Accepted(connection) => connection,
                ZeroRttStatus::Rejected(connection) => {
                    connection.close(0u32.into(), b"early data rejected");
                    panic!("fragmented_session_0rtt_rejected_no_evidence");
                }
            };
            assert_eq!(early_reply(&mut recv).await["error"], expected);
            connection.closed().await;
            drop(pending);
        })
        .await;
        f.no_destinations().await;
        assert_eq!(
            f.host().metrics().denied,
            if case == "replay" { 1 } else { 2 }
        );
        endpoint.close().await;
        drop(endpoint);
        if case != "replay" {
            assert!(!f.host().invitation_consumed(f.invitation.invitation_id()));
            let guest = f.guest().await;
            let bridge = guest
                .udp_bridge_fragmented("aux", 1, local())
                .await
                .unwrap();
            f.udp_addresses.push(bridge.local_addr());
            let origin = f.origin().await;
            udp_echo(&origin, bridge.local_addr(), 0xe5, 4).await;
            bridge.shutdown().await;
            drop((bridge, origin));
            guest.shutdown().await;
            drop(guest);
        }
        f.cleanup().await;
    }
    eprintln!(
        "{}",
        json!({"case":"fragmented_session_accepted_0rtt",
        "acceptedEarlyDataCases":3,"udpAttemptedApplicationBytes":31,
        "unauthorizedDestinationAccesses":0,"preTlsIntervalDynamicallyObserved":false,
        "cleanupRebind":true,"internetTested":false})
    );
}

#[tokio::test]
async fn public_profile_mode_key_and_registry_gates_refuse_before_endpoint_or_destination_io() {
    let tcp = TcpListener::bind(local()).await.unwrap();
    let udp = UdpSocket::bind(local()).await.unwrap();
    // Keep the owned reservation bound during every Host configuration gate.
    let transport = std::net::UdpSocket::bind(local()).unwrap();
    transport.set_nonblocking(true).unwrap();
    let bind = transport.local_addr().unwrap();
    let host_key = generate_key();
    let guest_key = generate_key();
    let services = vec![
        Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: tcp.local_addr().unwrap(),
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        },
        Service {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            endpoint: udp.local_addr().unwrap(),
            revision: 1,
            max_datagram_bytes: 4096,
            required_capability: Capability::FragmentedDatagram,
            udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                bytes_per_second: 16384,
                datagrams_per_second: 20,
            },
        },
    ];
    let invitation = SessionInvitation::issue_fragmented_loopback_for_lifetimes(
        &host_key,
        guest_key.public(),
        "fragmented-session-pre-io",
        bind,
        &services,
        Duration::from_secs(20),
        Duration::from_secs(30),
    )
    .unwrap();
    let config = || HostConfig {
        secret_key: host_key.clone(),
        bind_addr: bind,
        session_id: invitation.session_id().into(),
        services: services.clone(),
        leases: vec![invitation.lease()],
        limits: limits(),
    };
    assert!(matches!(
        Host::start_invited_session(config(), &invitation).await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Host::start_invited_session_external_ipv6(config(), &invitation).await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Host::start_invited_fragmented_session_external_ipv6(config(), &invitation).await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Guest::connect_invited_session(guest_key.clone(), &invitation, limits()).await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Guest::connect_invited_session_external_ipv6(guest_key.clone(), &invitation, limits())
            .await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Guest::connect_invited_fragmented_session_external_ipv6(
            guest_key.clone(),
            &invitation,
            limits()
        )
        .await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Guest::connect_invited_fragmented_session(generate_key(), &invitation, limits()).await,
        Err(Error::Unauthorized)
    ));
    for change in 0..10 {
        let mut bad = config();
        match change {
            0 => bad.services[1].revision += 1,
            1 => bad.services[1].required_capability = Capability::Datagram,
            2 => bad.services[1].max_datagram_bytes -= 1,
            3 => bad.services[1].udp_return_policy = UdpReturnPolicy::ReplyBudget,
            4 => {
                bad.services.pop();
            }
            5 => {
                bad.leases[0].services.remove("aux");
            }
            6 => bad.leases[0].expires_at_unix_ms = invitation.admission_expires_at_unix_ms(),
            7 => bad.leases[0].peer = generate_key().public(),
            8 => bad.session_id = "wrong-session".into(),
            // Documentation-only IP: a local pre-I/O gate, never dialled.
            9 => bad.services[1].endpoint = "192.0.2.1:43003".parse().unwrap(),
            _ => unreachable!(),
        }
        assert!(
            matches!(
                Host::start_invited_fragmented_session(bad, &invitation).await,
                Err(Error::Configuration)
            ),
            "fragmented_session_registry_gate_changed"
        );
    }
    let tcp_only = SessionInvitation::issue_loopback_for_lifetimes(
        &host_key,
        guest_key.public(),
        invitation.session_id(),
        bind,
        &services[..1],
        Duration::from_secs(20),
        Duration::from_secs(30),
    )
    .unwrap();
    let mut old_config = config();
    old_config.services.truncate(1);
    old_config.leases = vec![tcp_only.lease()];
    assert!(matches!(
        Host::start_invited_fragmented_session(old_config, &tcp_only).await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Guest::connect_invited_fragmented_session(guest_key, &tcp_only, limits()).await,
        Err(Error::Configuration)
    ));
    let mut bytes = [0; 64];
    assert_eq!(
        transport.recv_from(&mut bytes).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let (tcp_result, udp_result) = tokio::join!(
        timeout(OBSERVATION, tcp.accept()),
        timeout(OBSERVATION, udp.recv_from(&mut bytes)),
    );
    assert!(
        tcp_result.is_err() && udp_result.is_err(),
        "refused_profile_touched_destination"
    );
    let tcp_address = tcp.local_addr().unwrap();
    let udp_address = udp.local_addr().unwrap();
    drop((tcp, udp, transport));
    let _tcp_rebound = TcpListener::bind(tcp_address).await.unwrap();
    let _udp_rebound = UdpSocket::bind(udp_address).await.unwrap();
    let _transport_rebound = UdpSocket::bind(bind).await.unwrap();
}
