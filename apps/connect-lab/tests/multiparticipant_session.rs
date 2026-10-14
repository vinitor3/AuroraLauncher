//! One Host, two separately invited Guests, and four real application origins.
//! Only loopback is exercised. Every case attempts less than 64 KiB of UDP
//! application payload counting both directions; TCP has a separate fixed bound.
use aurora_connect_lab::*;
use iroh::{Endpoint, NetReportConfig, RelayMode, endpoint::presets};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    future::Future,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::Barrier,
    task::{JoinHandle, JoinSet},
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;

const WAIT: Duration = Duration::from_secs(5);
const OBSERVE: Duration = Duration::from_millis(150);
fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn limits() -> Limits {
    Limits {
        handshake_timeout: WAIT,
        ..Limits::default()
    }
}
async fn bounded<T>(future: impl Future<Output = T>) -> T {
    timeout(WAIT, future).await.expect("group_fixture_deadline")
}
async fn after(deadline: u64) {
    sleep(Duration::from_millis(
        deadline.saturating_sub(unix_millis()) + 50,
    ))
    .await;
}
fn services(game: SocketAddr, aux: SocketAddr) -> Vec<Service> {
    vec![
        Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: game,
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        },
        Service {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            endpoint: aux,
            revision: 1,
            max_datagram_bytes: 4096,
            required_capability: Capability::FragmentedDatagram,
            udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                bytes_per_second: 16384,
                datagrams_per_second: 20,
            },
        },
    ]
}
#[derive(Default)]
struct Observations {
    tcp_accepts: AtomicUsize,
    tcp_closed: AtomicUsize,
    tcp_bytes: AtomicUsize,
    udp_packets: AtomicUsize,
    udp_bytes: AtomicUsize,
    udp_sources: Mutex<BTreeSet<SocketAddr>>,
    tcp_bytes_at_udp_ingress: Mutex<Vec<usize>>,
}
struct Fixture {
    host: Option<Host>,
    invitations: Vec<SessionInvitation>,
    keys: Vec<SecretKey>,
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
    async fn new(lifetimes: [(u64, u64); 2], host_limits: Limits) -> Self {
        let tcp = TcpListener::bind(local()).await.unwrap();
        let udp = Arc::new(UdpSocket::bind(local()).await.unwrap());
        let game = tcp.local_addr().unwrap();
        let aux = udp.local_addr().unwrap();
        let services = services(game, aux);
        let reservation = std::net::UdpSocket::bind(local()).unwrap();
        let bind = reservation.local_addr().unwrap();
        let host_key = generate_key();
        let keys = vec![generate_key(), generate_key()];
        let invitations: Vec<_> = keys
            .iter()
            .zip(lifetimes)
            .map(|(key, (admission, session))| {
                let issued = SessionInvitation::issue_fragmented_loopback_for_lifetimes(
                    &host_key,
                    key.public(),
                    "independent-participants",
                    bind,
                    &services,
                    Duration::from_millis(admission),
                    Duration::from_millis(session),
                )
                .unwrap();
                SessionInvitation::verify_fragmented_loopback(
                    &issued.encode(),
                    host_key.public(),
                    key.public(),
                )
                .unwrap()
            })
            .collect();
        // Reverse lease order deliberately: authority must be matched by ID.
        let leases = invitations
            .iter()
            .rev()
            .map(SessionInvitation::lease)
            .collect();
        drop(reservation);
        let host = bounded(Host::start_invited_fragmented_session_group(
            HostConfig {
                secret_key: host_key,
                bind_addr: bind,
                session_id: "independent-participants".into(),
                services,
                leases,
                limits: host_limits,
            },
            &invitations,
        ))
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
                                let count = match socket.read(&mut bytes).await { Ok(0) | Err(_) => break, Ok(n) => n };
                                seen.tcp_bytes.fetch_add(count, Ordering::SeqCst);
                                if socket.write_all(&bytes[..count]).await.is_err() { break; }
                            }
                            let _ = socket.shutdown().await;
                            seen.tcp_closed.fetch_add(1, Ordering::SeqCst);
                        });
                    }
                    done = children.join_next(), if !children.is_empty() => { done.unwrap().unwrap(); }
                }
            }
            children.shutdown().await;
        });
        let udp_stop = stop.clone();
        let udp_observed = observed.clone();
        let udp_task = tokio::spawn(async move {
            let mut bytes = [0; 4098];
            let mut children = JoinSet::new();
            loop {
                tokio::select! {
                    _ = udp_stop.cancelled() => break,
                    received = udp.recv_from(&mut bytes) => {
                        let (count, source) = received.unwrap();
                        assert!(count <= 4096, "oversize_reached_group_destination");
                        udp_observed.udp_sources.lock().unwrap().insert(source);
                        udp_observed.udp_packets.fetch_add(1, Ordering::SeqCst);
                        udp_observed.udp_bytes.fetch_add(count, Ordering::SeqCst);
                        udp_observed.tcp_bytes_at_udp_ingress.lock().unwrap()
                            .push(udp_observed.tcp_bytes.load(Ordering::SeqCst));
                        let reply = bytes[..count].to_vec();
                        let socket = udp.clone();
                        children.spawn(async move {
                            // Different tags delay returns independently, exercising a real
                            // endpoint response after other participants have sent traffic.
                            sleep(Duration::from_millis(10 + u64::from(reply[0] % 4) * 15)).await;
                            assert_eq!(socket.send_to(&reply, source).await.unwrap(), reply.len());
                        });
                    }
                    done = children.join_next(), if !children.is_empty() => { done.unwrap().unwrap(); }
                }
            }
            children.shutdown().await;
        });
        let mut udp_addresses = vec![aux];
        udp_addresses.extend(host.bound_sockets());
        Self {
            host: Some(host),
            invitations,
            keys,
            guests: vec![],
            observed,
            tcp_addresses: vec![game],
            udp_addresses,
            stop,
            tasks: vec![tcp_task, udp_task],
        }
    }
    fn host(&self) -> &Host {
        self.host.as_ref().unwrap()
    }
    async fn guest(&mut self, index: usize) -> Guest {
        let guest = bounded(Guest::connect_invited_fragmented_session(
            self.keys[index].clone(),
            &self.invitations[index],
            limits(),
        ))
        .await
        .unwrap();
        self.udp_addresses.extend(guest.bound_sockets());
        self.guests.push(guest.clone());
        guest
    }
    async fn bridges(&mut self, guest: &Guest) -> (TcpBridge, UdpBridge) {
        let tcp = bounded(guest.tcp_bridge("game", 1, local())).await.unwrap();
        let udp = bounded(guest.udp_bridge_fragmented("aux", 1, local()))
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
    async fn active(&self, peers: u64, tcp: u64, udp: u64) {
        bounded(async {
            loop {
                let m = self.host().metrics();
                if (m.active_peers, m.active_tcp, m.active_udp) == (peers, tcp, udp) {
                    break;
                }
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
    }
    fn hello(&self, index: usize) -> Value {
        let invite = &self.invitations[index];
        json!({"schemaVersion":1,"sessionId":invite.session_id(),"leaseId":invite.invitation_id(),
            "capabilities":["reliableStream","fragmentedDatagram"],
            "invitation":{"digest":invite.digest(),"generation":invite.generation()}})
    }
    async fn raw_hello(&mut self, key: SecretKey, hello: Value) -> Value {
        let endpoint = Endpoint::builder(presets::Minimal)
            .clear_ip_transports()
            .clear_relay_transports()
            .clear_address_lookup()
            .relay_mode(RelayMode::Disabled)
            .net_report_config(NetReportConfig::minimal())
            .keylog(false)
            .secret_key(key)
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
            let bytes = serde_json::to_vec(&hello).unwrap();
            send.write_u32(bytes.len() as u32).await.unwrap();
            send.write_all(&bytes).await.unwrap();
            send.finish().unwrap();
            let size = recv.read_u32().await.unwrap() as usize;
            assert!((1..=1024).contains(&size));
            let mut bytes = vec![0; size];
            recv.read_exact(&mut bytes).await.unwrap();
            serde_json::from_slice(&bytes).unwrap()
        })
        .await;
        connection.close(0u32.into(), b"fixture complete");
        bounded(endpoint.close()).await;
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
        let tcp = std::mem::take(&mut self.tcp_addresses);
        let udp = std::mem::take(&mut self.udp_addresses);
        drop(self);
        let mut tcp_rebound = vec![];
        let mut udp_rebound = vec![];
        for address in tcp.into_iter().collect::<BTreeSet<_>>() {
            tcp_rebound.push(TcpListener::bind(address).await.expect("group_tcp_rebind"));
        }
        for address in udp.into_iter().collect::<BTreeSet<_>>() {
            udp_rebound.push(UdpSocket::bind(address).await.expect("group_udp_rebind"));
        }
    }
}
async fn tcp_echo(socket: &mut TcpStream, tag: u8, size: usize) {
    bounded(async {
        let payload = vec![tag; size];
        socket.write_all(&payload).await.unwrap();
        let mut received = vec![0; size];
        socket.read_exact(&mut received).await.unwrap();
        assert_eq!(received, payload, "group_tcp_scope_or_bytes");
    })
    .await;
}
async fn paced_tcp(socket: &mut TcpStream, tag: u8, started: &Barrier, returned: &Barrier) {
    bounded(async {
        tcp_echo(socket, tag, 4096).await;
        started.wait().await;
        for _ in 1..31 {
            tcp_echo(socket, tag, 4096).await;
            sleep(Duration::from_millis(4)).await;
        }
        // Do not finish either TCP transfer until all four isolated UDP replies
        // arrived. This is causal overlap, independent of relative scheduling.
        returned.wait().await;
        tcp_echo(socket, tag, 4096).await;
    })
    .await;
}
async fn udp_echo(socket: &UdpSocket, address: SocketAddr, tag: u8, size: usize) {
    bounded(async {
        let payload: Vec<_> = (0..size)
            .map(|i| tag.wrapping_add((i % 251) as u8))
            .collect();
        assert_eq!(socket.send_to(&payload, address).await.unwrap(), size);
        let mut received = [0; 4098];
        let (count, source) = socket.recv_from(&mut received).await.unwrap();
        assert!(
            source == address && count == size && received[..count] == payload,
            "group_udp_cross_return_or_bytes"
        );
    })
    .await;
}
async fn tcp_closed(socket: &mut TcpStream) {
    let mut byte = [0];
    assert!(
        matches!(bounded(socket.read(&mut byte)).await, Ok(0) | Err(_)),
        "revoked_participant_tcp_open"
    );
}
async fn udp_closed(bridge: &UdpBridge) {
    bounded(async {
        while !bridge.is_closed() {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_agents_four_origins_after_admission_and_individual_revocation() {
    let mut f = Fixture::new([(2000, 20000), (2000, 20000)], limits()).await;
    let a = f.guest(0).await;
    let b = f.guest(1).await;
    let (ta, ua) = f.bridges(&a).await;
    let (tb, ub) = f.bridges(&b).await;
    let origins = [
        f.origin().await,
        f.origin().await,
        f.origin().await,
        f.origin().await,
    ];
    let mut sa = bounded(TcpStream::connect(ta.local_addr())).await.unwrap();
    let mut sb = bounded(TcpStream::connect(tb.local_addr())).await.unwrap();
    tcp_echo(&mut sa, 0x91, 256).await;
    tcp_echo(&mut sb, 0x92, 256).await;
    after(
        f.invitations
            .iter()
            .map(SessionInvitation::admission_expires_at_unix_ms)
            .max()
            .unwrap(),
    )
    .await;
    for invite in &f.invitations {
        assert_eq!(invite.check_admission_current(), Err(Error::Expired));
        invite.check_session_current().unwrap();
        assert!(f.host().invitation_consumed(invite.invitation_id()));
    }
    // Four associations are created AFTER both admission windows, using only
    // the original admitted authority, while both distinct TCP streams run.
    let started = Barrier::new(3);
    let returned = Barrier::new(3);
    bounded(async {
        tokio::join!(
            paced_tcp(&mut sa, 0xa1, &started, &returned),
            paced_tcp(&mut sb, 0xb2, &started, &returned),
            async {
                started.wait().await;
                tokio::join!(
                    udp_echo(&origins[0], ua.local_addr(), 0x31, 4096),
                    udp_echo(&origins[1], ua.local_addr(), 0x42, 4096),
                    udp_echo(&origins[2], ub.local_addr(), 0x53, 4096),
                    udp_echo(&origins[3], ub.local_addr(), 0x64, 4096),
                );
                returned.wait().await;
            },
        );
    })
    .await;
    f.active(2, 2, 4).await;
    assert_eq!(f.observed.udp_sources.lock().unwrap().len(), 4);
    assert_eq!(f.observed.udp_bytes.load(Ordering::SeqCst), 4 * 4096);
    assert!(
        f.observed
            .tcp_bytes_at_udp_ingress
            .lock()
            .unwrap()
            .iter()
            .any(|&bytes| bytes > 512 && bytes < 2 * (131072 + 256)),
        "udp_must_arrive_during_real_tcp_forwarding"
    );
    assert_eq!(
        f.observed.tcp_bytes.load(Ordering::SeqCst),
        2 * (131072 + 256)
    );
    for (origin, bridge) in [(&origins[0], &ua), (&origins[2], &ub)] {
        assert_eq!(
            origin
                .send_to(&[0xee; 4097], bridge.local_addr())
                .await
                .unwrap(),
            4097
        );
    }
    sleep(OBSERVE).await;
    assert_eq!(
        f.observed.udp_packets.load(Ordering::SeqCst),
        4,
        "oversize_endpoint_contact"
    );
    let mut unexpected = [0; 4098];
    for origin in &origins {
        assert!(
            timeout(Duration::from_millis(20), origin.recv_from(&mut unexpected))
                .await
                .is_err(),
            "extra_or_cross_return"
        );
    }
    assert!(f.host().revoke(f.invitations[0].invitation_id()));
    bounded(a.connection().closed()).await;
    tcp_closed(&mut sa).await;
    f.active(1, 1, 2).await;
    udp_closed(&ua).await;
    let before = f.observed.udp_packets.load(Ordering::SeqCst);
    origins[0]
        .send_to(b"revoked", ua.local_addr())
        .await
        .unwrap();
    sleep(OBSERVE).await;
    assert_eq!(f.observed.udp_packets.load(Ordering::SeqCst), before);
    let hello = f.hello(0);
    assert_eq!(
        f.raw_hello(f.keys[0].clone(), hello).await["error"],
        "expired"
    );
    assert!(b.connection().close_reason().is_none());
    let fresh = f.origin().await;
    let mut second_b = bounded(TcpStream::connect(tb.local_addr())).await.unwrap();
    tokio::join!(
        tcp_echo(&mut sb, 0x75, 4096),
        tcp_echo(&mut second_b, 0x86, 4096),
        udp_echo(&origins[2], ub.local_addr(), 0x97, 32),
        udp_echo(&fresh, ub.local_addr(), 0xa8, 32)
    );
    f.active(1, 2, 3).await;
    assert_eq!(f.host().metrics().destination_tcp_connects, 3);
    assert_eq!(f.host().metrics().udp_associations_opened, 5);
    let attempted_udp = 2 * 4 * 4096 + 2 * 4097 + 7 + 2 * 2 * 32;
    assert!(attempted_udp < 65536);
    bounded(ta.shutdown()).await;
    bounded(ua.shutdown()).await;
    bounded(tb.shutdown()).await;
    bounded(ub.shutdown()).await;
    drop((sa, sb, second_b, origins, fresh, ta, ua, tb, ub, a, b));
    f.cleanup().await;
}

#[tokio::test]
async fn shorter_session_expires_during_traffic_without_ending_or_renewing_other_participant() {
    let mut f = Fixture::new([(2000, 4500), (2000, 20000)], limits()).await;
    let a = f.guest(0).await;
    let b = f.guest(1).await;
    let (ta, ua) = f.bridges(&a).await;
    let (tb, ub) = f.bridges(&b).await;
    let oa = f.origin().await;
    let ob = f.origin().await;
    let mut sa = bounded(TcpStream::connect(ta.local_addr())).await.unwrap();
    let mut sb = bounded(TcpStream::connect(tb.local_addr())).await.unwrap();
    let mut rounds = 0;
    while f.invitations[0].session_remaining().unwrap() > Duration::from_millis(600) {
        tokio::join!(
            tcp_echo(&mut sa, 0x11, 64),
            tcp_echo(&mut sb, 0x22, 64),
            udp_echo(&oa, ua.local_addr(), 0x33, 8),
            udp_echo(&ob, ub.local_addr(), 0x44, 8)
        );
        rounds += 1;
        assert!(rounds <= 40);
        sleep(Duration::from_millis(100)).await;
    }
    assert!(rounds > 2);
    bounded(a.connection().closed()).await;
    assert_eq!(
        f.invitations[0].check_session_current(),
        Err(Error::Expired)
    );
    f.invitations[1].check_session_current().unwrap();
    tcp_closed(&mut sa).await;
    f.active(1, 1, 1).await;
    udp_closed(&ua).await;
    let count = f.observed.udp_packets.load(Ordering::SeqCst);
    oa.send_to(b"expired", ua.local_addr()).await.unwrap();
    sleep(OBSERVE).await;
    assert_eq!(f.observed.udp_packets.load(Ordering::SeqCst), count);
    assert!(bounded(a.open_tcp("game", 1)).await.is_err());
    assert!(bounded(a.open_udp_fragmented("aux", 1)).await.is_err());
    let extra = f.origin().await;
    let mut second_b = bounded(TcpStream::connect(tb.local_addr())).await.unwrap();
    tokio::join!(
        tcp_echo(&mut sb, 0x55, 2048),
        tcp_echo(&mut second_b, 0x66, 2048),
        udp_echo(&extra, ub.local_addr(), 0x77, 4096)
    );
    f.active(1, 2, 2).await;
    assert_eq!(f.host().metrics().destination_tcp_connects, 3);
    assert_eq!(f.host().metrics().udp_associations_opened, 3);
    assert!(rounds * 32 + 7 + 8192 < 65536);
    bounded(ta.shutdown()).await;
    bounded(ua.shutdown()).await;
    bounded(tb.shutdown()).await;
    bounded(ub.shutdown()).await;
    drop((sa, sb, second_b, oa, ob, extra, ta, ua, tb, ub, a, b));
    f.cleanup().await;
}

#[tokio::test]
async fn late_unused_member_is_refused_by_host_while_other_member_remains_live() {
    let mut f = Fixture::new([(1000, 12000), (12000, 18000)], limits()).await;
    let b = f.guest(1).await;
    let (tb, ub) = f.bridges(&b).await;
    let origin = f.origin().await;
    let mut socket = bounded(TcpStream::connect(tb.local_addr())).await.unwrap();
    tcp_echo(&mut socket, 0x81, 128).await;
    udp_echo(&origin, ub.local_addr(), 0x92, 32).await;
    after(f.invitations[0].admission_expires_at_unix_ms()).await;
    let before = f.host().metrics();
    assert!(matches!(
        bounded(Guest::connect_invited_fragmented_session(
            f.keys[0].clone(),
            &f.invitations[0],
            limits()
        ))
        .await,
        Err(Error::Expired)
    ));
    assert_eq!(
        f.host().metrics().denied,
        before.denied,
        "local_expiry_must_not_dial"
    );
    let hello = f.hello(0);
    assert_eq!(
        f.raw_hello(f.keys[0].clone(), hello).await["error"],
        "expired"
    );
    assert!(
        !f.host()
            .invitation_consumed(f.invitations[0].invitation_id())
    );
    assert!(
        f.host()
            .invitation_consumed(f.invitations[1].invitation_id())
    );
    let after = f.host().metrics();
    assert_eq!(
        after.destination_tcp_connects,
        before.destination_tcp_connects
    );
    assert_eq!(
        after.udp_associations_opened,
        before.udp_associations_opened
    );
    assert_eq!(after.denied, before.denied + 1);
    f.active(1, 1, 1).await;
    let fresh = f.origin().await;
    tokio::join!(
        tcp_echo(&mut socket, 0xa3, 1024),
        udp_echo(&fresh, ub.local_addr(), 0xb4, 4096)
    );
    bounded(tb.shutdown()).await;
    bounded(ub.shutdown()).await;
    drop((tb, ub, socket, origin, fresh, b));
    f.cleanup().await;
}

#[tokio::test]
async fn proof_peer_capability_and_replay_refusals_never_consume_the_other_invitation() {
    let mut f = Fixture::new([(20000, 30000), (20000, 30000)], limits()).await;
    for case in 0..4 {
        let mut hello = f.hello(1);
        let key = if case == 0 {
            f.keys[0].clone()
        } else {
            f.keys[1].clone()
        };
        let expected = match case {
            0 => "unauthorized",
            1 => {
                hello["invitation"]["digest"] = json!(f.invitations[0].digest());
                "unauthorized"
            }
            2 => {
                hello.as_object_mut().unwrap().remove("invitation");
                "unauthorized"
            }
            _ => {
                hello["capabilities"] = json!(["reliableStream", "datagram"]);
                "capability_unavailable"
            }
        };
        assert_eq!(f.raw_hello(key, hello).await["error"], expected);
        for invite in &f.invitations {
            assert!(!f.host().invitation_consumed(invite.invitation_id()));
        }
    }
    let a = f.guest(0).await;
    bounded(a.shutdown()).await;
    f.active(0, 0, 0).await;
    assert!(matches!(
        bounded(Guest::connect_invited_fragmented_session(
            f.keys[0].clone(),
            &f.invitations[0],
            limits()
        ))
        .await,
        Err(Error::Replay)
    ));
    assert!(
        !f.host()
            .invitation_consumed(f.invitations[1].invitation_id())
    );
    let b = f.guest(1).await;
    f.active(1, 0, 0).await;
    let m = f.host().metrics();
    assert_eq!(
        (
            m.destination_tcp_connects,
            m.udp_associations_opened,
            m.udp_forwarded,
            m.udp_returned
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(m.denied, 5);
    assert_eq!(f.observed.tcp_accepts.load(Ordering::SeqCst), 0);
    assert_eq!(f.observed.udp_packets.load(Ordering::SeqCst), 0);
    let (tb, ub) = f.bridges(&b).await;
    let origin = f.origin().await;
    let mut socket = bounded(TcpStream::connect(tb.local_addr())).await.unwrap();
    tokio::join!(
        tcp_echo(&mut socket, 0x35, 4096),
        udp_echo(&origin, ub.local_addr(), 0x46, 4096)
    );
    bounded(tb.shutdown()).await;
    bounded(ub.shutdown()).await;
    drop((tb, ub, socket, origin, a, b));
    f.cleanup().await;
}

#[tokio::test]
async fn per_participant_and_global_tcp_udp_limits_are_not_multiplied_by_group_size() {
    for per_peer in [1, 2] {
        let host_limits = Limits {
            max_tcp_per_peer: per_peer,
            max_tcp_total: 2,
            max_udp_per_peer: per_peer,
            max_udp_total: 2,
            ..limits()
        };
        let mut f = Fixture::new([(20000, 30000), (20000, 30000)], host_limits).await;
        let a = f.guest(0).await;
        let b = f.guest(1).await;
        let (mut sa, mut ra) = bounded(a.open_tcp("game", 1)).await.unwrap();
        let ua = bounded(a.open_udp_fragmented("aux", 1)).await.unwrap();
        if per_peer == 1 {
            assert!(matches!(
                bounded(a.open_tcp("game", 1)).await,
                Err(Error::Limit)
            ));
            assert!(matches!(
                bounded(a.open_udp_fragmented("aux", 1)).await,
                Err(Error::Limit)
            ));
        }
        let (mut sb, mut rb) = bounded(b.open_tcp("game", 1)).await.unwrap();
        let ub = bounded(b.open_udp_fragmented("aux", 1)).await.unwrap();
        assert_eq!(
            ua.association_id(),
            ub.association_id(),
            "colliding_ids_should_still_be_connection_scoped"
        );
        // The first association has the same request ID in each connection;
        // its isolated response must still select the original participant.
        f.active(2, 2, 2).await;
        let before = f.host().metrics();
        for guest in [&a, &b] {
            assert!(matches!(
                bounded(guest.open_tcp("game", 1)).await,
                Err(Error::Limit)
            ));
            assert!(matches!(
                bounded(guest.open_udp_fragmented("aux", 1)).await,
                Err(Error::Limit)
            ));
        }
        assert_eq!(
            f.host().metrics().destination_tcp_connects,
            before.destination_tcp_connects
        );
        assert_eq!(
            f.host().metrics().udp_associations_opened,
            before.udp_associations_opened
        );
        bounded(async {
            sa.write_all(b"A").await.unwrap();
            sb.write_all(b"B").await.unwrap();
            let mut first = [0];
            let mut second = [0];
            ra.read_exact(&mut first).await.unwrap();
            rb.read_exact(&mut second).await.unwrap();
            assert_eq!(first, *b"A");
            assert_eq!(second, *b"B");
            ua.send(b"participant-A").await.unwrap();
            ub.send(b"participant-B").await.unwrap();
            assert_eq!(ua.recv().await.unwrap().as_ref(), b"participant-A");
            assert_eq!(ub.recv().await.unwrap().as_ref(), b"participant-B");
        })
        .await;
        bounded(a.shutdown()).await;
        bounded(ua.shutdown()).await;
        drop((sa, ra, ua));
        f.active(1, 1, 1).await;
        if per_peer == 2 {
            let (s, r) = bounded(b.open_tcp("game", 1)).await.unwrap();
            let u = bounded(b.open_udp_fragmented("aux", 1)).await.unwrap();
            f.active(1, 2, 2).await;
            bounded(u.shutdown()).await;
            drop((s, r, u));
        }
        bounded(ub.shutdown()).await;
        drop((sb, rb, ub, a, b));
        f.cleanup().await;
    }
}

fn ready_refusal<T>(future: impl Future<Output = Result<T>>) {
    // No Tokio runtime exists in this test: endpoint creation would panic or
    // yield Pending. Only a synchronous configuration/profile refusal passes.
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        matches!(
            future.as_mut().poll(&mut context),
            Poll::Ready(Err(Error::Configuration | Error::CapabilityUnavailable))
        ),
        "group_must_refuse_before_io"
    );
}
#[test]
fn malformed_group_and_old_single_entry_points_refuse_before_endpoint_creation() {
    let key = generate_key();
    let direct = "127.0.0.1:43200".parse().unwrap();
    let registry = services(
        "127.0.0.1:43201".parse().unwrap(),
        "127.0.0.1:43202".parse().unwrap(),
    );
    let issue = |guest, session: &str, address, services: &[Service]| {
        SessionInvitation::issue_fragmented_loopback_for_lifetimes(
            &key,
            guest,
            session,
            address,
            services,
            Duration::from_secs(30),
            Duration::from_secs(60),
        )
        .unwrap()
    };
    let originals = vec![
        issue(generate_key().public(), "group-negative", direct, &registry),
        issue(generate_key().public(), "group-negative", direct, &registry),
    ];
    let config = |invitations: &[SessionInvitation]| HostConfig {
        secret_key: key.clone(),
        bind_addr: direct,
        session_id: "group-negative".into(),
        services: registry.clone(),
        leases: invitations.iter().map(SessionInvitation::lease).collect(),
        limits: limits(),
    };
    for case in 0..17 {
        let mut invites = originals.clone();
        let mut c = config(&invites);
        match case {
            0 => {
                invites.clear();
                c.leases.clear();
            }
            1 => {
                invites.truncate(1);
                c.leases.truncate(1);
            }
            2 => {
                invites = (0..9)
                    .map(|_| issue(generate_key().public(), "group-negative", direct, &registry))
                    .collect();
                c = config(&invites);
            }
            3 => {
                c.limits.max_peers = 1;
            }
            4 => {
                c.leases.pop();
            }
            5 => {
                c.leases.push(originals[0].lease());
            }
            6 => {
                invites[1] = invites[0].clone();
                c = config(&invites);
            }
            7 => {
                invites[1] = issue(invites[0].guest_key(), "group-negative", direct, &registry);
                c = config(&invites);
            }
            8 => {
                c.leases[1] = c.leases[0].clone();
            }
            9 => {
                c.leases[1].peer = generate_key().public();
            }
            10 => {
                c.leases[1].expires_at_unix_ms += 1;
            }
            11 => {
                c.leases[1].services.remove("aux");
            }
            12 => {
                c.secret_key = generate_key();
            }
            13 => {
                invites[1] = issue(
                    generate_key().public(),
                    "different-session",
                    direct,
                    &registry,
                );
                c = config(&invites);
            }
            14 => {
                invites[1] = issue(
                    generate_key().public(),
                    "group-negative",
                    "127.0.0.1:43203".parse().unwrap(),
                    &registry,
                );
                c = config(&invites);
            }
            15 => {
                c.services[1].revision += 1;
            }
            _ => {
                invites[1] = SessionInvitation::issue_loopback_for_lifetimes(
                    &key,
                    generate_key().public(),
                    "group-negative",
                    direct,
                    &registry[..1],
                    Duration::from_secs(30),
                    Duration::from_secs(60),
                )
                .unwrap();
                c = config(&invites);
            }
        }
        ready_refusal(Host::start_invited_fragmented_session_group(c, &invites));
    }
    ready_refusal(Host::start_invited_fragmented_session_group_external_ipv6(
        config(&originals),
        &originals,
    ));
    ready_refusal(Host::start_invited_fragmented_session_group_direct_ipv4(
        config(&originals),
        &originals,
    ));
    ready_refusal(Host::start_invited_fragmented_session(
        config(&originals),
        &originals[0],
    ));
    ready_refusal(Host::start_invited_session(
        config(&originals),
        &originals[0],
    ));
}
