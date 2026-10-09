//! Two real direct QUIC peers and owned loopback TCP canaries. No Minecraft,
//! voice, public relay, personal configuration or external address is used.
use aurora_connect_lab::*;
use iroh::{
    Endpoint, RelayMode,
    endpoint::{ConnectOptions, Connection, ZeroRttStatus, presets},
};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::{JoinHandle, JoinSet},
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn limits() -> Limits {
    Limits {
        handshake_timeout: Duration::from_secs(3),
        ..Limits::default()
    }
}
struct Fixture {
    host_key: SecretKey,
    guest_key: SecretKey,
    invitation: SessionInvitation,
    service: Service,
    reservation: Option<std::net::UdpSocket>,
    accepts: Arc<AtomicUsize>,
    closed: Arc<AtomicUsize>,
    cancel: CancellationToken,
    echo: Option<JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(task) = &self.echo {
            task.abort();
        }
    }
}
impl Fixture {
    async fn new(admission_ms: u64, session_ms: u64) -> Self {
        let listener = TcpListener::bind(local()).await.unwrap();
        let service = Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: listener.local_addr().unwrap(),
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        };
        let reservation = std::net::UdpSocket::bind(local()).unwrap();
        let host_key = generate_key();
        let guest_key = generate_key();
        let now = unix_millis();
        let invitation = SessionInvitation::issue_loopback(
            &host_key,
            guest_key.public(),
            "session-lifetime",
            reservation.local_addr().unwrap(),
            std::slice::from_ref(&service),
            SessionDeadlines {
                admission_expires_at_unix_ms: now + admission_ms,
                session_expires_at_unix_ms: now + session_ms,
            },
        )
        .unwrap();
        let accepts = Arc::new(AtomicUsize::new(0));
        let count = accepts.clone();
        let closed = Arc::new(AtomicUsize::new(0));
        let closed_count = closed.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let echo = tokio::spawn(async move {
            let mut children = JoinSet::new();
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    accepted = listener.accept() => {
                    let (mut stream, _) = accepted.unwrap(); count.fetch_add(1, Ordering::SeqCst);
                    let closed_count = closed_count.clone();
                    children.spawn(async move { let (mut read, mut write) = stream.split(); let _ = tokio::io::copy(&mut read, &mut write).await; let _ = write.shutdown().await; closed_count.fetch_add(1, Ordering::SeqCst); });
                    }
                    _ = children.join_next(), if !children.is_empty() => (),
                }
            }
            children.shutdown().await;
        });
        Self {
            host_key,
            guest_key,
            invitation,
            service,
            reservation: Some(reservation),
            accepts,
            closed,
            cancel,
            echo: Some(echo),
        }
    }
    fn config(&self) -> HostConfig {
        HostConfig {
            secret_key: self.host_key.clone(),
            bind_addr: self.invitation.direct_addr(),
            session_id: self.invitation.session_id().into(),
            services: vec![self.service.clone()],
            leases: vec![self.invitation.lease()],
            limits: limits(),
        }
    }
    async fn host(&mut self) -> Host {
        self.reservation.take();
        Host::start_invited_session(self.config(), &self.invitation)
            .await
            .unwrap()
    }
    async fn guest(&self) -> Guest {
        Guest::connect_invited_session(self.guest_key.clone(), &self.invitation, limits())
            .await
            .unwrap()
    }
    fn hello(&self) -> Value {
        json!({"schemaVersion":1,"sessionId":self.invitation.session_id(),"leaseId":self.invitation.invitation_id(),"capabilities":["reliableStream"],"invitation":{"digest":self.invitation.digest(),"generation":1}})
    }
    async fn stop(&mut self, host: Host) {
        let sockets = host.bound_sockets();
        host.shutdown().await;
        drop(host);
        self.cancel.cancel();
        self.echo.take().unwrap().await.unwrap();
        for address in sockets {
            drop(std::net::UdpSocket::bind(address).unwrap());
        }
        drop(std::net::TcpListener::bind(self.service.endpoint).unwrap());
    }
    fn no_destination(&self, host: &Host) {
        assert_eq!(self.accepts.load(Ordering::SeqCst), 0);
        assert_eq!(host.metrics().destination_tcp_connects, 0);
        assert_eq!(host.metrics().udp_associations_opened, 0);
    }
    async fn destinations_closed(&self, expected: usize) {
        timeout(Duration::from_secs(3), async {
            while self.closed.load(Ordering::SeqCst) < expected {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(self.closed.load(Ordering::SeqCst), expected);
    }
}
async fn raw_endpoint(key: SecretKey) -> Endpoint {
    Endpoint::builder(presets::Minimal)
        .clear_ip_transports()
        .clear_relay_transports()
        .clear_address_lookup()
        .relay_mode(RelayMode::Disabled)
        .keylog(false)
        .secret_key(key)
        .bind_addr(local())
        .unwrap()
        .bind()
        .await
        .unwrap()
}
async fn send_value(send: &mut iroh::endpoint::SendStream, value: &Value) {
    let bytes = serde_json::to_vec(value).unwrap();
    send.write_u32(bytes.len() as u32).await.unwrap();
    send.write_all(&bytes).await.unwrap();
    send.finish().unwrap();
}
async fn read_value(recv: &mut iroh::endpoint::RecvStream) -> Value {
    let n = recv.read_u32().await.unwrap();
    assert!(n < 16384);
    let mut bytes = vec![0; n as usize];
    recv.read_exact(&mut bytes).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
async fn hello_on(connection: &Connection, hello: &Value) -> Value {
    timeout(Duration::from_secs(3), async {
        let (mut send, mut recv) = connection.open_bi().await.unwrap();
        send_value(&mut send, hello).await;
        read_value(&mut recv).await
    })
    .await
    .unwrap()
}
async fn raw_hello(f: &Fixture, key: SecretKey, hello: &Value) -> Value {
    let endpoint = raw_endpoint(key).await;
    let connection = timeout(
        Duration::from_secs(3),
        endpoint.connect(
            EndpointAddr::new(f.host_key.public()).with_ip_addr(f.invitation.direct_addr()),
            ALPN,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let reply = hello_on(&connection, hello).await;
    connection.close(0u32.into(), b"fixture done");
    endpoint.close().await;
    reply
}
async fn idle(host: &Host) {
    timeout(Duration::from_secs(3), async {
        while host.metrics().active_peers != 0 || host.metrics().active_tcp != 0 {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
async fn wait_after(unix_ms: u64) {
    sleep(Duration::from_millis(
        unix_ms.saturating_sub(unix_millis()) + 40,
    ))
    .await;
}
async fn echo(stream: &mut TcpStream, byte: u8, n: usize) {
    timeout(Duration::from_secs(3), async {
        let bytes = vec![byte; n];
        stream.write_all(&bytes).await.unwrap();
        let mut received = vec![0; n];
        stream.read_exact(&mut received).await.unwrap();
        assert_eq!(received, bytes);
    })
    .await
    .unwrap();
}
async fn closed(stream: &mut TcpStream) {
    let mut b = [0; 1];
    let result = timeout(Duration::from_secs(3), stream.read(&mut b))
        .await
        .unwrap();
    assert!(matches!(result, Ok(0) | Err(_)));
}
async fn cleanup_guest(guest: Guest, bridge: TcpBridge) {
    let sockets = guest.bound_sockets();
    let address = bridge.local_addr();
    bridge.shutdown().await;
    guest.shutdown().await;
    drop(bridge);
    drop(guest);
    drop(std::net::TcpListener::bind(address).unwrap());
    for address in sockets {
        drop(std::net::UdpSocket::bind(address).unwrap());
    }
}

#[tokio::test]
async fn admission_expiry_preserves_tcp_and_new_streams_until_revocation() {
    let mut f = Fixture::new(2000, 12000).await;
    let host = f.host().await;
    let guest = f.guest().await;
    assert!(host.invitation_consumed(f.invitation.invitation_id()));
    let replay = raw_hello(&f, f.guest_key.clone(), &f.hello()).await;
    assert_eq!(replay["error"], "replay");
    let bridge = guest.tcp_bridge("game", 1, local()).await.unwrap();
    let mut first = TcpStream::connect(bridge.local_addr()).await.unwrap();
    echo(&mut first, 1, 65536).await;
    wait_after(f.invitation.admission_expires_at_unix_ms()).await;
    assert_eq!(f.invitation.check_admission_current(), Err(Error::Expired));
    assert!(f.invitation.check_session_current().is_ok());
    assert!(host.invitation_consumed(f.invitation.invitation_id()));
    echo(&mut first, 2, 65536).await;
    let mut second = TcpStream::connect(bridge.local_addr()).await.unwrap();
    echo(&mut second, 3, 65536).await;
    assert_eq!(host.metrics().destination_tcp_connects, 2);
    assert!(host.revoke(f.invitation.invitation_id()));
    closed(&mut first).await;
    closed(&mut second).await;
    idle(&host).await;
    assert_eq!(
        raw_hello(&f, f.guest_key.clone(), &f.hello()).await["error"],
        "expired"
    );
    assert_eq!(host.metrics().destination_tcp_connects, 2);
    assert!(host.invitation_consumed(f.invitation.invitation_id()));
    f.destinations_closed(2).await;
    drop((first, second));
    cleanup_guest(guest, bridge).await;
    f.stop(host).await;
}

#[tokio::test]
async fn late_hello_is_rejected_by_host_without_consuming_or_opening_destination() {
    let mut f = Fixture::new(900, 12000).await;
    let host = f.host().await;
    wait_after(f.invitation.admission_expires_at_unix_ms()).await;
    assert!(matches!(
        Guest::connect_invited_session(f.guest_key.clone(), &f.invitation, limits()).await,
        Err(Error::Expired)
    ));
    assert_eq!(
        raw_hello(&f, f.guest_key.clone(), &f.hello()).await["error"],
        "expired"
    );
    assert_eq!(host.metrics().denied, 1);
    assert!(!host.invitation_consumed(f.invitation.invitation_id()));
    f.no_destination(&host);
    f.stop(host).await;
}

#[tokio::test]
async fn absolute_session_expiry_closes_active_application_without_activity_renewal() {
    let mut f = Fixture::new(1500, 3000).await;
    let host = f.host().await;
    let guest = f.guest().await;
    let bridge = guest.tcp_bridge("game", 1, local()).await.unwrap();
    let mut stream = TcpStream::connect(bridge.local_addr()).await.unwrap();
    echo(&mut stream, 4, 1024).await;
    wait_after(f.invitation.admission_expires_at_unix_ms()).await;
    echo(&mut stream, 5, 1024).await;
    closed(&mut stream).await;
    idle(&host).await;
    assert_eq!(f.invitation.check_session_current(), Err(Error::Expired));
    assert!(guest.open_tcp("game", 1).await.is_err());
    assert_eq!(host.metrics().destination_tcp_connects, 1);
    f.destinations_closed(1).await;
    drop(stream);
    cleanup_guest(guest, bridge).await;
    f.stop(host).await;
}

#[tokio::test]
async fn wrong_pin_schema_proof_capability_and_uninvited_peer_never_reach_game() {
    let mut f = Fixture::new(20000, 30000).await;
    let host = f.host().await;
    let legacy = Invitation::issue(
        &f.host_key,
        f.guest_key.public(),
        f.invitation.session_id(),
        f.invitation.direct_addr(),
        std::slice::from_ref(&f.service),
        unix_millis() + 20000,
    )
    .unwrap();
    assert!(
        Invitation::verify(
            &f.invitation.encode(),
            f.host_key.public(),
            f.guest_key.public()
        )
        .is_err()
    );
    assert!(
        SessionInvitation::verify_loopback(
            &legacy.encode(),
            f.host_key.public(),
            f.guest_key.public()
        )
        .is_err()
    );
    let mut hello = f.hello();
    hello["invitation"]["digest"] = json!(legacy.digest());
    assert_eq!(
        raw_hello(&f, f.guest_key.clone(), &hello).await["error"],
        "unauthorized"
    );
    let mut hello = f.hello();
    hello["capabilities"] = json!(["reliableStream", "datagram"]);
    assert_eq!(
        raw_hello(&f, f.guest_key.clone(), &hello).await["error"],
        "capability_unavailable"
    );
    assert_eq!(
        raw_hello(&f, generate_key(), &f.hello()).await["error"],
        "unauthorized"
    );
    let mut hello = f.hello();
    hello["invitation"]["generation"] = json!(2);
    assert_eq!(
        raw_hello(&f, f.guest_key.clone(), &hello).await["error"],
        "unauthorized"
    );
    assert!(!host.invitation_consumed(f.invitation.invitation_id()));
    f.no_destination(&host);
    let guest = f.guest().await;
    assert!(guest.open_udp("game", 1).await.is_err());
    assert!(guest.open_tcp("game", 2).await.is_err());
    assert!(guest.open_tcp("aux", 1).await.is_err());
    f.no_destination(&host);
    guest.shutdown().await;
    drop(guest);
    host.shutdown().await;
    drop(host);
    // Reverse direction: an old Host has the same pins/address/session, but
    // must reject the schema-2 digest even with its correct legacy lease ID.
    let mut config = f.config();
    config.leases = vec![legacy.lease()];
    let host = Host::start_invited(config, &legacy).await.unwrap();
    let mut hello = f.hello();
    hello["leaseId"] = json!(legacy.lease().lease_id);
    assert_eq!(
        raw_hello(&f, f.guest_key.clone(), &hello).await["error"],
        "unauthorized"
    );
    assert!(!host.invitation_consumed(&legacy.lease().lease_id));
    f.no_destination(&host);
    f.stop(host).await;
}

#[tokio::test]
async fn guest_accepts_delayed_ack_after_valid_admission_without_renewing_session() {
    let canary = TcpListener::bind(local()).await.unwrap();
    let reservation = std::net::UdpSocket::bind(local()).unwrap();
    let address = reservation.local_addr().unwrap();
    let host_key = generate_key();
    let guest_key = generate_key();
    let now = unix_millis();
    let invitation = SessionInvitation::issue_loopback(
        &host_key,
        guest_key.public(),
        "delayed-ack",
        address,
        &[Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: canary.local_addr().unwrap(),
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        }],
        SessionDeadlines {
            admission_expires_at_unix_ms: now + 1500,
            session_expires_at_unix_ms: now + 5000,
        },
    )
    .unwrap();
    drop(reservation);
    let server = Endpoint::builder(presets::Minimal)
        .clear_ip_transports()
        .clear_relay_transports()
        .clear_address_lookup()
        .relay_mode(RelayMode::Disabled)
        .keylog(false)
        .secret_key(host_key)
        .alpns(vec![ALPN.to_vec()])
        .bind_addr(address)
        .unwrap()
        .bind()
        .await
        .unwrap();
    let expected = invitation.clone();
    let serving = server.clone();
    let task = tokio::spawn(async move {
        let connection = serving.accept().await.unwrap().await.unwrap();
        let (mut send, mut recv) = connection.accept_bi().await.unwrap();
        let hello = read_value(&mut recv).await;
        assert_eq!(hello["invitation"]["digest"], expected.digest());
        assert_eq!(connection.remote_id(), expected.guest_key());
        assert!(expected.check_admission_current().is_ok());
        wait_after(expected.admission_expires_at_unix_ms()).await;
        send_value(&mut send, &json!({"error":null,"serviceHandle":0,"associationId":0,"generation":0,"maxPayload":null})).await;
        connection
    });
    let guest = Guest::connect_invited_session(guest_key, &invitation, limits())
        .await
        .unwrap();
    let connection = task.await.unwrap();
    assert_eq!(invitation.check_admission_current(), Err(Error::Expired));
    assert!(invitation.check_session_current().is_ok());
    assert_eq!(invitation.session_expires_at_unix_ms(), now + 5000);
    assert!(
        timeout(Duration::from_millis(50), canary.accept())
            .await
            .is_err()
    );
    guest.shutdown().await;
    server.close().await;
    drop((guest, connection, server, canary));
    drop(std::net::UdpSocket::bind(address).unwrap());
}

#[tokio::test]
async fn explicit_modes_and_host_lease_deadline_are_checked_before_bind() {
    let mut f = Fixture::new(20000, 30000).await;
    // Reservation deliberately stays bound: Configuration proves the error
    // comes from the gate rather than an attempted bind of the occupied port.
    let mut bad = f.config();
    bad.leases[0].expires_at_unix_ms = f.invitation.admission_expires_at_unix_ms();
    assert!(matches!(
        Host::start_invited_session(bad, &f.invitation).await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Host::start_invited_session_external_ipv6(f.config(), &f.invitation).await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Guest::connect_invited_session_external_ipv6(f.guest_key.clone(), &f.invitation, limits())
            .await,
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Guest::connect_invited_session(generate_key(), &f.invitation, limits()).await,
        Err(Error::Unauthorized)
    ));
    let host = f.host().await;
    f.no_destination(&host);
    f.stop(host).await;
}

#[tokio::test]
async fn accepted_zero_rtt_replay_is_denied_before_queued_tcp_open() {
    let mut f = Fixture::new(20000, 30000).await;
    let host = f.host().await;
    let endpoint = raw_endpoint(f.guest_key.clone()).await;
    let connection = endpoint.connect(host.addr(), ALPN).await.unwrap();
    assert!(hello_on(&connection, &f.hello()).await["error"].is_null());
    sleep(Duration::from_millis(150)).await;
    connection.close(0u32.into(), b"get ticket");
    connection.closed().await;
    idle(&host).await;
    drop(connection);
    let connecting = endpoint
        .connect_with_opts(host.addr(), ALPN, ConnectOptions::new())
        .await
        .unwrap();
    let early = match connecting.into_0rtt() {
        Ok(value) => value,
        Err(_) => panic!("session_ticket_missing"),
    };
    let (mut send, mut recv) = early.open_bi().await.unwrap();
    assert!(recv.is_0rtt());
    send_value(&mut send, &f.hello()).await;
    let (mut open, pending) = early.open_bi().await.unwrap();
    send_value(&mut open,&json!({"schemaVersion":1,"requestId":1,"serviceId":"game","serviceRevision":1,"protocol":"tcp","operation":"open","associationId":0})).await;
    let connection = match early.handshake_completed().await.unwrap() {
        ZeroRttStatus::Accepted(value) => value,
        ZeroRttStatus::Rejected(_) => panic!("session_early_data_not_accepted"),
    };
    assert_eq!(
        timeout(Duration::from_secs(3), read_value(&mut recv))
            .await
            .unwrap()["error"],
        "replay"
    );
    connection.closed().await;
    drop((pending, connection));
    endpoint.close().await;
    idle(&host).await;
    f.no_destination(&host);
    f.stop(host).await;
}
