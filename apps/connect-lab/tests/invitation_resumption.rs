//! Real Iroh 1.3.0 TLS resumption, using only explicit loopback transports.
//! The sender must observe a 0-RTT stream AND ZeroRttStatus::Accepted. A normal
//! connection or rejected early data cannot silently satisfy these tests.
//!
//! This does not dynamically prove a pre-TLS time interval: the handshake runs
//! in the background even before handshake_completed() is awaited. The Host's
//! incoming.await gate is a separate source-level guarantee. No server-side
//! is_0rtt() observation is used as historical evidence of received early data.
//! See iroh-1.3.0/src/endpoint/connection.rs::test_0rtt for the ticket lifecycle.
use aurora_connect_lab::*;
use bytes::Bytes;
use iroh::{
    Endpoint, RelayMode,
    endpoint::{
        ConnectOptions, Connection, QuicTransportConfig, RecvStream, SendStream, ZeroRttStatus,
        presets,
    },
};
use serde_json::{Value, json};
use std::{
    future::Future,
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UdpSocket},
    time::{sleep, timeout},
};

const DEADLINE: Duration = Duration::from_secs(4);

fn checked<T, E>(result: std::result::Result<T, E>, code: &'static str) -> T {
    match result {
        Ok(value) => value,
        Err(_) => panic!("{code}"),
    }
}

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    checked(
        timeout(DEADLINE, future).await,
        "resumption_fixture_deadline",
    )
}

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

fn limits() -> Limits {
    Limits {
        handshake_timeout: DEADLINE,
        ..Limits::default()
    }
}

struct Fixture {
    host: Host,
    invitation: Invitation,
    guest_key: SecretKey,
    tcp: TcpListener,
    udp: UdpSocket,
}

impl Fixture {
    async fn new(ttl: Duration) -> Self {
        let tcp = checked(TcpListener::bind(local()).await, "tcp_canary_bind_failed");
        let udp = checked(UdpSocket::bind(local()).await, "udp_canary_bind_failed");
        let mut services = Vec::new();
        for (id, protocol, address, capability) in [
            (
                "game",
                Protocol::Tcp,
                checked(tcp.local_addr(), "tcp_canary_addr_failed"),
                Capability::ReliableStream,
            ),
            (
                "aux",
                Protocol::Udp,
                checked(udp.local_addr(), "udp_canary_addr_failed"),
                Capability::Datagram,
            ),
        ] {
            services.push(Service {
                service_id: id.into(),
                protocol,
                endpoint: address,
                revision: 1,
                max_datagram_bytes: 1400,
                required_capability: capability,
                udp_return_policy: UdpReturnPolicy::ReplyBudget,
            });
        }
        let reservation = checked(
            std::net::UdpSocket::bind(local()),
            "quic_reservation_failed",
        );
        let bind_addr = checked(reservation.local_addr(), "quic_reservation_addr_failed");
        let host_key = generate_key();
        let guest_key = generate_key();
        let invitation = checked(
            Invitation::issue(
                &host_key,
                guest_key.public(),
                "resumption-session",
                bind_addr,
                &services,
                unix_millis() + ttl.as_millis() as u64,
            ),
            "invitation_fixture_failed",
        );
        drop(reservation);
        let host = checked(
            Host::start_invited(
                HostConfig {
                    secret_key: host_key,
                    bind_addr,
                    session_id: invitation.session_id().into(),
                    services,
                    leases: vec![invitation.lease()],
                    limits: limits(),
                },
                &invitation,
            )
            .await,
            "invited_host_start_failed",
        );
        Self {
            host,
            invitation,
            guest_key,
            tcp,
            udp,
        }
    }

    fn hello(&self) -> Value {
        json!({"schemaVersion":1, "sessionId":self.invitation.session_id(),
            "leaseId":self.invitation.lease().lease_id,
            "capabilities":["reliableStream","datagram"],
            "invitation":{"digest":self.invitation.digest(),"generation":1}})
    }

    fn wrong_proof(&self) -> Value {
        let mut hello = self.hello();
        hello["invitation"]["digest"] = json!("0".repeat(64));
        hello
    }

    async fn client(&self, key: SecretKey) -> Endpoint {
        let transport = QuicTransportConfig::builder()
            .send_observed_address_reports(false)
            .max_remote_nat_traversal_addresses(0)
            .build();
        checked(
            Endpoint::builder(presets::Minimal)
                .clear_ip_transports()
                .clear_relay_transports()
                .clear_address_lookup()
                .relay_mode(RelayMode::Disabled)
                .secret_key(key)
                .keylog(false)
                .transport_config(transport)
                .bind_addr(local())
                .unwrap()
                .bind()
                .await,
            "raw_client_bind_failed",
        )
    }

    async fn idle(&self) {
        bounded(async {
            while self.host.metrics().active_peers != 0 {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
    }

    async fn no_destinations(&self) {
        self.idle().await;
        let metrics = self.host.metrics();
        assert_eq!(metrics.destination_tcp_connects, 0);
        assert_eq!(metrics.udp_associations_opened, 0);
        assert_eq!(metrics.udp_forwarded, 0);
        assert_eq!(metrics.udp_returned, 0);
        assert_eq!(metrics.active_tcp, 0);
        assert_eq!(metrics.active_udp, 0);
        let mut bytes = [0; 64];
        let (tcp, udp) = tokio::join!(
            timeout(Duration::from_millis(40), self.tcp.accept()),
            timeout(Duration::from_millis(40), self.udp.recv_from(&mut bytes)),
        );
        assert!(tcp.is_err(), "denied_request_reached_tcp_canary");
        assert!(udp.is_err(), "denied_request_reached_udp_canary");
    }

    /// Warm only the TLS cache, with a fully framed reply proving the first
    /// connection completed. Invalid application proof does not consume a lease.
    async fn warm(&self, endpoint: &Endpoint, hello: &Value, expected: Option<&str>) {
        bounded(async {
            let connecting = checked(
                endpoint
                    .connect_with_opts(self.host.addr(), ALPN, ConnectOptions::new())
                    .await,
                "initial_connection_failed",
            );
            let connection = match connecting.into_0rtt() {
                Err(connecting) => checked(connecting.await, "initial_tls_failed"),
                Ok(_) => panic!("unexpected_ticket_in_fresh_endpoint"),
            };
            let (mut send, mut recv) =
                checked(connection.open_bi().await, "initial_hello_open_failed");
            send_message(&mut send, hello).await;
            let reply = read_reply(&mut recv).await;
            expect_error(&reply, expected);
            connection.close(0u32.into(), b"test connection done");
            let _ = connection.closed().await;
        })
        .await;
        self.idle().await;
        self.no_destinations().await;
    }

    async fn resumed_denial(&self, endpoint: &Endpoint, hello: &Value, expected: &str) {
        bounded(async {
            let connecting = checked(
                endpoint
                    .connect_with_opts(self.host.addr(), ALPN, ConnectOptions::new())
                    .await,
                "resumed_connection_failed",
            );
            let early = match connecting.into_0rtt() {
                Ok(early) => early,
                Err(_) => panic!("resumption_ticket_unavailable_no_0rtt_evidence"),
            };
            let (mut send, mut recv) = checked(early.open_bi().await, "early_hello_open_failed");
            assert!(recv.is_0rtt(), "hello_stream_not_opened_in_0rtt");
            send_message(&mut send, hello).await;

            // Real, syntactically valid service opens are queued without waiting
            // for Aurora admission. The Host must reject them with the Hello.
            let mut pending = Vec::new();
            for (id, service, protocol) in [(1, "game", "tcp"), (2, "aux", "udp")] {
                let (mut send, recv) = checked(early.open_bi().await, "early_service_open_failed");
                send_message(
                    &mut send,
                    &json!({"schemaVersion":1, "requestId":id,
                    "serviceId":service,"serviceRevision":1,"protocol":protocol,
                    "operation":"open","associationId":0}),
                )
                .await;
                pending.push(recv);
            }
            // A datagram also cannot acquire an association before admission.
            let datagram = checked(
                DatagramFrame {
                    service_handle: 1,
                    association_id: 2,
                    generation: 1,
                    sequence: 1,
                    payload: Bytes::from_static(b"early-canary"),
                }
                .encode(),
                "early_datagram_encode_failed",
            );
            checked(early.send_datagram(datagram), "early_datagram_queue_failed");

            let completed = checked(early.handshake_completed().await, "resumed_tls_failed");
            let connection: Connection = match completed {
                ZeroRttStatus::Accepted(connection) => connection,
                ZeroRttStatus::Rejected(connection) => {
                    connection.close(0u32.into(), b"early data rejected");
                    panic!("observed_0rtt_rejected_no_accepted_early_data_evidence");
                }
            };
            let reply = read_reply(&mut recv).await;
            expect_error(&reply, Some(expected));
            let _ = connection.closed().await;
            drop(pending);
        })
        .await;
        self.no_destinations().await;
    }

    async fn valid_guest_still_works(&self) {
        bounded(async {
            let guest = checked(
                Guest::connect_invited(self.guest_key.clone(), &self.invitation, limits()).await,
                "legitimate_guest_denied_after_intrusion",
            );
            let (mut send, mut recv) =
                checked(guest.open_tcp("game", 1).await, "legitimate_tcp_denied");
            let (mut destination, _) =
                checked(self.tcp.accept().await, "legitimate_tcp_canary_failed");
            checked(
                send.write_all(b"valid").await,
                "legitimate_tcp_write_failed",
            );
            checked(send.finish(), "legitimate_tcp_finish_failed");
            let mut bytes = [0; 5];
            checked(
                destination.read_exact(&mut bytes).await,
                "legitimate_tcp_read_failed",
            );
            assert!(&bytes == b"valid", "legitimate_tcp_bytes_changed");
            checked(
                destination.write_all(&bytes).await,
                "legitimate_tcp_echo_failed",
            );
            checked(
                destination.shutdown().await,
                "legitimate_tcp_shutdown_failed",
            );
            let echoed = checked(recv.read_to_end(64).await, "legitimate_tcp_return_failed");
            assert!(echoed == b"valid", "legitimate_tcp_echo_changed");
            let association = checked(guest.open_udp("aux", 1).await, "legitimate_udp_denied");
            checked(
                association.send(b"valid").await,
                "legitimate_udp_send_failed",
            );
            let (count, address) = checked(
                self.udp.recv_from(&mut bytes).await,
                "legitimate_udp_canary_failed",
            );
            assert!(
                count == 5 && &bytes == b"valid",
                "legitimate_udp_bytes_changed"
            );
            checked(
                self.udp.send_to(&bytes, address).await,
                "legitimate_udp_echo_failed",
            );
            let echoed = checked(association.recv().await, "legitimate_udp_return_failed");
            assert!(echoed.as_ref() == b"valid", "legitimate_udp_echo_changed");
            guest.shutdown().await;
        })
        .await;
        self.idle().await;
        assert_eq!(self.host.metrics().destination_tcp_connects, 1);
        assert_eq!(self.host.metrics().udp_associations_opened, 1);
    }
}

async fn send_message(send: &mut SendStream, value: &Value) {
    let body = checked(serde_json::to_vec(value), "fixture_message_encode_failed");
    assert!(body.len() <= 4096, "fixture_message_bound");
    checked(
        send.write_u32(body.len() as u32).await,
        "fixture_message_length_write_failed",
    );
    checked(send.write_all(&body).await, "fixture_message_write_failed");
    checked(send.finish(), "fixture_message_finish_failed");
}

async fn read_reply(recv: &mut RecvStream) -> Value {
    let size = checked(recv.read_u32().await, "fixture_reply_length_failed") as usize;
    assert!((1..=1024).contains(&size), "fixture_reply_bound");
    let mut bytes = vec![0; size];
    checked(
        recv.read_exact(&mut bytes).await,
        "fixture_reply_body_failed",
    );
    checked(serde_json::from_slice(&bytes), "fixture_reply_json_failed")
}

fn expect_error(reply: &Value, expected: Option<&str>) {
    match expected {
        Some(code) => assert!(
            reply["error"].as_str() == Some(code),
            "unexpected_admission_error"
        ),
        None => assert!(reply["error"].is_null(), "initial_admission_failed"),
    }
}

fn report(case: &'static str, started: Instant) {
    // Only fixed labels, counts and duration; never invitation, key, ID or address.
    eprintln!(
        "{}",
        json!({"case":case,"zeroRttStatus":"accepted",
        "earlyHelloStream":true,"deniedDestinationTcpConnects":0,
        "deniedUdpAssociations":0,"preTlsIntervalDynamicallyObserved":false,
        "elapsedMs":started.elapsed().as_millis()})
    );
}

#[tokio::test]
async fn consumed_invitation_replay_is_denied_on_accepted_0rtt_connection() {
    let started = Instant::now();
    let f = Fixture::new(Duration::from_secs(30)).await;
    let endpoint = f.client(f.guest_key.clone()).await;
    f.warm(&endpoint, &f.hello(), None).await;
    f.resumed_denial(&endpoint, &f.hello(), "replay").await;
    assert_eq!(f.host.metrics().denied, 1);
    endpoint.close().await;
    f.host.shutdown().await;
    report("consumed_invitation", started);
}

#[tokio::test]
async fn wrong_proof_on_accepted_0rtt_does_not_consume_invitation() {
    let started = Instant::now();
    let f = Fixture::new(Duration::from_secs(30)).await;
    let endpoint = f.client(f.guest_key.clone()).await;
    f.warm(&endpoint, &f.wrong_proof(), Some("unauthorized"))
        .await;
    f.resumed_denial(&endpoint, &f.wrong_proof(), "unauthorized")
        .await;
    assert_eq!(f.host.metrics().denied, 2);
    endpoint.close().await;
    f.valid_guest_still_works().await;
    f.host.shutdown().await;
    report("wrong_proof", started);
}

#[tokio::test]
async fn wrong_tls_peer_on_accepted_0rtt_does_not_consume_invitation() {
    let started = Instant::now();
    let f = Fixture::new(Duration::from_secs(30)).await;
    let endpoint = f.client(generate_key()).await;
    f.warm(&endpoint, &f.hello(), Some("unauthorized")).await;
    f.resumed_denial(&endpoint, &f.hello(), "unauthorized")
        .await;
    assert_eq!(f.host.metrics().denied, 2);
    endpoint.close().await;
    f.valid_guest_still_works().await;
    f.host.shutdown().await;
    report("wrong_tls_peer", started);
}

#[tokio::test]
async fn revoked_invitation_is_denied_on_accepted_0rtt_connection() {
    let started = Instant::now();
    let f = Fixture::new(Duration::from_secs(30)).await;
    let endpoint = f.client(f.guest_key.clone()).await;
    f.warm(&endpoint, &f.wrong_proof(), Some("unauthorized"))
        .await;
    assert!(
        f.host.revoke(&f.invitation.lease().lease_id),
        "fixture_revoke_failed"
    );
    f.resumed_denial(&endpoint, &f.hello(), "expired").await;
    assert_eq!(f.host.metrics().denied, 2);
    endpoint.close().await;
    f.host.shutdown().await;
    report("revoked_invitation", started);
}

#[tokio::test]
async fn expired_invitation_is_denied_on_accepted_0rtt_connection() {
    let started = Instant::now();
    let f = Fixture::new(Duration::from_secs(2)).await;
    let endpoint = f.client(f.guest_key.clone()).await;
    f.warm(&endpoint, &f.wrong_proof(), Some("unauthorized"))
        .await;
    let remaining = f
        .invitation
        .expires_at_unix_ms()
        .saturating_sub(unix_millis());
    sleep(Duration::from_millis(remaining + 20)).await;
    f.resumed_denial(&endpoint, &f.hello(), "expired").await;
    assert_eq!(f.host.metrics().denied, 2);
    endpoint.close().await;
    f.host.shutdown().await;
    report("expired_invitation", started);
}
