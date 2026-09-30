//! Real direct QUIC transport with synthetic loopback TCP/UDP fixtures only.
//! No launcher, account, Minecraft, voice mod, external network or relay.
use aurora_connect_lab::*;
use iroh::{
    Endpoint, RelayMode,
    endpoint::{Connection, presets},
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
    net::{TcpListener, TcpStream, UdpSocket},
    task::{JoinHandle, JoinSet},
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
fn service(id: &str, protocol: Protocol, address: SocketAddr) -> Service {
    Service {
        service_id: id.into(),
        protocol,
        endpoint: address,
        revision: 1,
        max_datagram_bytes: 1400,
        required_capability: if protocol == Protocol::Tcp {
            Capability::ReliableStream
        } else {
            Capability::Datagram
        },
        udp_return_policy: UdpReturnPolicy::ReplyBudget,
    }
}
struct Fixture {
    host_key: SecretKey,
    guest_key: SecretKey,
    invitation: Invitation,
    services: Vec<Service>,
    reservation: Option<std::net::UdpSocket>,
    accepts: Arc<AtomicUsize>,
    udp_received: Arc<AtomicUsize>,
    tasks: Vec<JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
impl Fixture {
    async fn new(ttl: Duration) -> Self {
        let listener = TcpListener::bind(local()).await.unwrap();
        let socket = UdpSocket::bind(local()).await.unwrap();
        let services = vec![
            service("game", Protocol::Tcp, listener.local_addr().unwrap()),
            service("aux", Protocol::Udp, socket.local_addr().unwrap()),
        ];
        let reservation = std::net::UdpSocket::bind(local()).unwrap();
        let host_key = generate_key();
        let guest_key = generate_key();
        let invitation = Invitation::issue(
            &host_key,
            guest_key.public(),
            "invited-session",
            reservation.local_addr().unwrap(),
            &services,
            unix_millis() + ttl.as_millis() as u64,
        )
        .unwrap();
        let accepts = Arc::new(AtomicUsize::new(0));
        let tcp_count = accepts.clone();
        let tcp = tokio::spawn(async move {
            let mut children = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((mut stream, _)) = accepted else { break; };
                        tcp_count.fetch_add(1, Ordering::SeqCst);
                        children.spawn(async move { let (mut read, mut write) = stream.split();
                            let _ = tokio::io::copy(&mut read, &mut write).await; let _ = write.shutdown().await; });
                    }
                    _ = children.join_next(), if !children.is_empty() => (),
                }
            }
        });
        let udp_received = Arc::new(AtomicUsize::new(0));
        let udp_count = udp_received.clone();
        let udp = tokio::spawn(async move {
            let mut bytes = [0u8; 65536];
            while let Ok((count, source)) = socket.recv_from(&mut bytes).await {
                udp_count.fetch_add(1, Ordering::SeqCst);
                let _ = socket.send_to(&bytes[..count], source).await;
            }
        });
        Self {
            host_key,
            guest_key,
            invitation,
            services,
            reservation: Some(reservation),
            accepts,
            udp_received,
            tasks: vec![tcp, udp],
        }
    }
    fn config(&self) -> HostConfig {
        HostConfig {
            secret_key: self.host_key.clone(),
            bind_addr: self.invitation.direct_addr(),
            session_id: self.invitation.session_id().into(),
            services: self.services.clone(),
            leases: vec![self.invitation.lease()],
            limits: limits(),
        }
    }
    async fn host(&mut self) -> Host {
        self.reservation.take();
        Host::start_invited(self.config(), &self.invitation)
            .await
            .unwrap()
    }
    async fn guest(&self) -> Guest {
        Guest::connect_invited(self.guest_key.clone(), &self.invitation, limits())
            .await
            .unwrap()
    }
    fn no_destinations(&self, host: &Host) {
        assert_eq!(self.accepts.load(Ordering::SeqCst), 0);
        assert_eq!(self.udp_received.load(Ordering::SeqCst), 0);
        assert_eq!(host.metrics().destination_tcp_connects, 0);
        assert_eq!(host.metrics().udp_associations_opened, 0);
    }
    fn hello(&self) -> Value {
        json!({"schemaVersion":1, "sessionId": self.invitation.session_id(),
            "leaseId":self.invitation.lease().lease_id, "capabilities":["reliableStream","datagram"],
            "invitation":{"digest":self.invitation.digest(),"generation":1}})
    }
}
async fn idle(host: &Host) {
    timeout(Duration::from_secs(3), async {
        while host.metrics().active_peers != 0 {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
async fn raw_endpoint(f: &Fixture, key: SecretKey) -> (Endpoint, Connection) {
    let endpoint = Endpoint::builder(presets::Minimal)
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
        .unwrap();
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
    (endpoint, connection)
}
async fn raw_hello(f: &Fixture, key: SecretKey, hello: &Value) -> Value {
    let (endpoint, connection) = raw_endpoint(f, key).await;
    let result = timeout(Duration::from_secs(3), async {
        let (mut send, mut recv) = connection.open_bi().await.unwrap();
        let encoded = serde_json::to_vec(hello).unwrap();
        send.write_u32(encoded.len() as u32).await.unwrap();
        send.write_all(&encoded).await.unwrap();
        send.finish().unwrap();
        let size = recv.read_u32().await.unwrap();
        assert!(size < 16384);
        let mut bytes = vec![0; size as usize];
        recv.read_exact(&mut bytes).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    })
    .await
    .unwrap();
    connection.close(0u32.into(), b"fixture closed");
    endpoint.close().await;
    result
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
// Preserve the explicitly declared field order when changing a test body.
// This independent helper re-signs bytes, not the production invitation codec.
fn resign(encoded: &str, host: &SecretKey) -> String {
    let body = encoded
        .strip_prefix("{\"body\":")
        .unwrap()
        .split_once(",\"signature\":")
        .unwrap()
        .0;
    let mut data = b"aurora/connect-lab/invitation/v1\0".to_vec();
    data.extend_from_slice(body.as_bytes());
    format!(
        "{{\"body\":{body},\"signature\":\"{}\"}}",
        hex(&host.sign(&data).to_bytes())
    )
}

#[tokio::test]
async fn canonical_signature_roundtrip_and_public_preview_exclude_internal_destinations() {
    let f = Fixture::new(Duration::from_secs(30)).await;
    let encoded = f.invitation.encode();
    let verified = Invitation::verify(&encoded, f.host_key.public(), f.guest_key.public()).unwrap();
    assert!(verified.encode() == encoded, "canonical_roundtrip_mismatch");
    assert!(
        verified.digest() == f.invitation.digest(),
        "invitation_digest_mismatch"
    );
    assert_eq!(verified.session_id(), "invited-session");
    assert!(
        verified.lease().peer == f.guest_key.public(),
        "guest_binding_mismatch"
    );
    assert_eq!(verified.lease().services.len(), 2);
    assert_eq!(verified.preview()["singleUse"], true);
    assert_eq!(verified.preview()["accountIdentityVerified"], false);
    assert!(encoded.len() <= MAX_INVITATION_BYTES);
    for target in &f.services {
        assert!(!encoded.contains(&target.endpoint.to_string()));
    }
    assert!(!encoded.contains(&hex(&f.host_key.to_bytes())));
    assert!(!encoded.contains(&hex(&f.guest_key.to_bytes())));
    let other = Invitation::issue(
        &f.host_key,
        f.guest_key.public(),
        "invited-session",
        f.invitation.direct_addr(),
        &f.services,
        unix_millis() + 30_000,
    )
    .unwrap();
    assert!(
        other.lease().lease_id != f.invitation.lease().lease_id,
        "invitation_id_reused"
    );
    assert_eq!(other.lease().lease_id.len(), 64);
    let mut reordered = f.services.clone();
    reordered.reverse();
    let invite = Invitation::issue(
        &f.host_key,
        f.guest_key.public(),
        "invited-session",
        f.invitation.direct_addr(),
        &reordered,
        unix_millis() + 30_000,
    )
    .unwrap();
    assert_eq!(invite.preview()["services"][0]["serviceId"], "game");
}

#[tokio::test]
async fn signature_binds_host_guest_session_id_address_services_limits_and_domain() {
    let f = Fixture::new(Duration::from_secs(30)).await;
    let encoded = f.invitation.encode();
    for (old, new) in [
        ("invited-session", "other-session"),
        ("\"revision\":1", "\"revision\":2"),
        ("\"maxDatagramBytes\":1400", "\"maxDatagramBytes\":1300"),
        (
            "\"directAddress\":\"127.0.0.1:",
            "\"directAddress\":\"127.0.0.2:",
        ),
    ] {
        let changed = encoded.replace(old, new);
        assert!(changed != encoded, "fixture_did_not_mutate_invitation");
        assert!(Invitation::verify(&changed, f.host_key.public(), f.guest_key.public()).is_err());
    }
    assert!(matches!(
        Invitation::verify(&encoded, generate_key().public(), f.guest_key.public()),
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        Invitation::verify(&encoded, f.host_key.public(), generate_key().public()),
        Err(Error::Unauthorized)
    ));
    let body = encoded
        .strip_prefix("{\"body\":")
        .unwrap()
        .split_once(",\"signature\":")
        .unwrap()
        .0;
    let wrong_domain = format!(
        "{{\"body\":{body},\"signature\":\"{}\"}}",
        hex(&f.host_key.sign(body.as_bytes()).to_bytes())
    );
    assert!(matches!(
        Invitation::verify(&wrong_domain, f.host_key.public(), f.guest_key.public()),
        Err(Error::Unauthorized)
    ));
    let invalid_signature = format!("{{\"body\":{body},\"signature\":\"{}\"}}", "00".repeat(64));
    assert!(matches!(
        Invitation::verify(
            &invalid_signature,
            f.host_key.public(),
            f.guest_key.public()
        ),
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn noncanonical_unknown_duplicate_and_oversized_inputs_are_rejected() {
    let f = Fixture::new(Duration::from_secs(30)).await;
    let encoded = f.invitation.encode();
    let value: Value = serde_json::from_str(&encoded).unwrap();
    for changed in [
        format!(" {encoded}"),
        format!("{encoded}\n"),
        serde_json::to_string_pretty(&value).unwrap(),
        encoded.replace(
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"schemaVersion\":1",
        ),
        encoded.replace(
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"unknown\":true",
        ),
        encoded.replace("invited-session", "invited\\u002dsession"),
        encoded.replace(
            "\"type\":\"replyBudget\"",
            "\"type\":\"replyBudget\",\"extra\":true",
        ),
        "x".repeat(MAX_INVITATION_BYTES + 1),
        String::new(),
        "[".repeat(4096),
    ] {
        assert!(Invitation::verify(&changed, f.host_key.public(), f.guest_key.public()).is_err());
    }
}

#[tokio::test]
async fn valid_signature_does_not_authorize_bad_generation_remote_address_or_expiration() {
    let f = Fixture::new(Duration::from_secs(30)).await;
    let encoded = f.invitation.encode();
    let expiry = f.invitation.expires_at_unix_ms();
    for changed in [
        encoded.replace("\"generation\":1", "\"generation\":2"),
        encoded.replace("127.0.0.1:", "192.0.2.1:"),
        encoded.replace(
            &format!("\"expiresAtUnixMs\":{expiry}"),
            &format!("\"expiresAtUnixMs\":{}", expiry + 600_000),
        ),
        encoded.replace(
            &format!("\"expiresAtUnixMs\":{expiry}"),
            "\"expiresAtUnixMs\":1",
        ),
    ] {
        let signed = resign(&changed, &f.host_key);
        assert!(Invitation::verify(&signed, f.host_key.public(), f.guest_key.public()).is_err());
    }
}

#[tokio::test]
async fn issuance_rejects_invalid_grants_nonloopback_and_excessive_lifetime() {
    let f = Fixture::new(Duration::from_secs(30)).await;
    for address in [
        "0.0.0.0:40000",
        "192.0.2.1:40000",
        "127.0.0.1:0",
        "[::]:40000",
        "[::1%1]:40000",
    ] {
        assert!(
            Invitation::issue(
                &f.host_key,
                f.guest_key.public(),
                "session",
                address.parse().unwrap(),
                &f.services,
                unix_millis() + 30_000
            )
            .is_err()
        );
    }
    for expires in [unix_millis(), 0, unix_millis() + 601_000, u64::MAX] {
        assert!(
            Invitation::issue(
                &f.host_key,
                f.guest_key.public(),
                "session",
                f.invitation.direct_addr(),
                &f.services,
                expires
            )
            .is_err()
        );
    }
    for services in [
        vec![],
        vec![f.services[1].clone()],
        vec![f.services[0].clone(), f.services[0].clone()],
    ] {
        assert!(
            Invitation::issue(
                &f.host_key,
                f.guest_key.public(),
                "session",
                f.invitation.direct_addr(),
                &services,
                unix_millis() + 30_000
            )
            .is_err()
        );
    }
    for mutation in 0..6 {
        let mut services = f.services.clone();
        match mutation {
            0 => services[0].revision = 2,
            1 => services[0].required_capability = Capability::Datagram,
            2 => services[1].service_id = "voice".into(),
            3 => services[1].protocol = Protocol::Tcp,
            4 => services[1].max_datagram_bytes = 0,
            _ => {
                services[0].udp_return_policy = UdpReturnPolicy::AuthorizedDownstream {
                    bytes_per_second: 1024,
                    datagrams_per_second: 10,
                }
            }
        }
        assert!(
            Invitation::issue(
                &f.host_key,
                f.guest_key.public(),
                "session",
                f.invitation.direct_addr(),
                &services,
                unix_millis() + 30_000
            )
            .is_err()
        );
    }
}

#[tokio::test]
async fn host_configuration_must_match_all_public_grants_and_exact_lease_before_binding() {
    let f = Fixture::new(Duration::from_secs(30)).await;
    for mutation in 0..11 {
        let mut config = f.config();
        match mutation {
            0 => config.secret_key = generate_key(),
            1 => config.session_id = "other".into(),
            2 => config.bind_addr = local(),
            3 => config.leases[0].lease_id = "different".into(),
            4 => config.leases[0].peer = generate_key().public(),
            5 => config.leases[0].expires_at_unix_ms += 1,
            6 => {
                config.leases[0].services.remove("aux");
            }
            7 => config.services[0].max_datagram_bytes -= 1,
            8 => {
                config.services.pop();
            }
            9 => {
                config.services[1].udp_return_policy = UdpReturnPolicy::AuthorizedDownstream {
                    bytes_per_second: 1024,
                    datagrams_per_second: 10,
                }
            }
            _ => config.leases.push(f.invitation.lease()),
        }
        assert!(matches!(
            Host::start_invited(config, &f.invitation).await,
            Err(Error::Configuration)
        ));
    }
    // The still-bound reservation ensures an accidental bind would fail with
    // Transport, making this a check of validation ordering too.
    assert!(
        f.reservation.as_ref().unwrap().local_addr().unwrap() == f.invitation.direct_addr(),
        "fixture_reservation_changed"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invited_agents_bridge_real_tcp_half_close_and_bidirectional_udp() {
    let mut f = Fixture::new(Duration::from_secs(30)).await;
    let host = f.host().await;
    let decoded = Invitation::verify(
        &f.invitation.encode(),
        f.host_key.public(),
        f.guest_key.public(),
    )
    .unwrap();
    let guest = Guest::connect_invited(f.guest_key.clone(), &decoded, limits())
        .await
        .unwrap();
    let tcp = guest.tcp_bridge("game", 1, local()).await.unwrap();
    let udp = guest.udp_bridge("aux", 1, local()).await.unwrap();
    let mut socket = TcpStream::connect(tcp.local_addr()).await.unwrap();
    let body = vec![0x56; 128 * 1024];
    let (mut read, mut write) = socket.split();
    let upload = async {
        write.write_all(&body).await.unwrap();
        write.shutdown().await.unwrap();
    };
    let download = async {
        let mut received = Vec::new();
        read.read_to_end(&mut received).await.unwrap();
        assert!(received == body, "tcp_fixture_bytes_changed");
    };
    timeout(Duration::from_secs(5), async {
        tokio::join!(upload, download);
    })
    .await
    .unwrap();
    let socket = UdpSocket::bind(local()).await.unwrap();
    socket.connect(udp.local_addr()).await.unwrap();
    socket.send(b"invited UDP").await.unwrap();
    let mut bytes = [0u8; 32];
    let count = timeout(Duration::from_secs(3), socket.recv(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&bytes[..count], b"invited UDP");
    assert_eq!(f.accepts.load(Ordering::SeqCst), 1);
    guest.shutdown().await;
    tcp.shutdown().await;
    udp.shutdown().await;
    idle(&host).await;
    assert!(matches!(f.guest_attempt().await, Err(Error::Replay)));
    host.shutdown().await;
}
impl Fixture {
    async fn guest_attempt(&self) -> Result<Guest> {
        Guest::connect_invited(self.guest_key.clone(), &self.invitation, limits()).await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_redeemers_have_exactly_one_winner_and_disconnect_does_not_reset() {
    let mut f = Fixture::new(Duration::from_secs(30)).await;
    let host = f.host().await;
    let (first, second) = tokio::join!(f.guest_attempt(), f.guest_attempt());
    let winner = match (first, second) {
        (Ok(g), Err(Error::Replay)) | (Err(Error::Replay), Ok(g)) => g,
        _ => panic!("single_use_admission_did_not_select_exactly_one_peer"),
    };
    f.no_destinations(&host);
    winner.shutdown().await;
    idle(&host).await;
    assert!(matches!(f.guest_attempt().await, Err(Error::Replay)));
    f.no_destinations(&host);
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_peer_missing_proof_digest_generation_and_capabilities_do_not_consume() {
    let mut f = Fixture::new(Duration::from_secs(30)).await;
    let host = f.host().await;
    assert!(matches!(
        Guest::connect_invited(generate_key(), &f.invitation, limits()).await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        raw_hello(&f, generate_key(), &f.hello()).await["error"],
        "unauthorized"
    );
    for mutation in 0..7 {
        let mut hello = f.hello();
        match mutation {
            0 => {
                hello.as_object_mut().unwrap().remove("invitation");
            }
            1 => hello["invitation"]["digest"] = json!("00".repeat(32)),
            2 => hello["invitation"]["generation"] = json!(2),
            3 => hello["capabilities"] = json!(["reliableStream"]),
            4 => hello["capabilities"] = json!(["reliableStream", "reliableStream"]),
            5 => hello["sessionId"] = json!("other"),
            _ => hello["leaseId"] = json!("other"),
        }
        let reply = raw_hello(&f, f.guest_key.clone(), &hello).await;
        assert!(matches!(
            reply["error"].as_str(),
            Some("unauthorized" | "capability_unavailable")
        ));
        f.no_destinations(&host);
    }
    let guest = f.guest().await;
    assert_eq!(host.metrics().active_peers, 1);
    guest.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopped_ack_stream_still_consumes_after_authenticated_admission() {
    let mut f = Fixture::new(Duration::from_secs(30)).await;
    let host = f.host().await;
    let (endpoint, connection) = raw_endpoint(&f, f.guest_key.clone()).await;
    let (mut send, mut recv) = connection.open_bi().await.unwrap();
    let body = serde_json::to_vec(&f.hello()).unwrap();
    // Keep the request incomplete while STOP_SENDING reaches the response
    // direction. Complete Hello only after the reader is already stopped.
    send.write_u32(body.len() as u32).await.unwrap();
    recv.stop(0u32.into()).unwrap();
    sleep(Duration::from_millis(100)).await;
    send.write_all(&body).await.unwrap();
    send.finish().unwrap();
    timeout(Duration::from_secs(3), connection.closed())
        .await
        .unwrap();
    endpoint.close().await;
    idle(&host).await;
    f.no_destinations(&host);
    assert!(matches!(f.guest_attempt().await, Err(Error::Replay)));
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revocation_before_admission_refuses_without_destination_access() {
    let mut f = Fixture::new(Duration::from_secs(30)).await;
    let host = f.host().await;
    assert!(host.revoke(&f.invitation.lease().lease_id));
    assert!(matches!(f.guest_attempt().await, Err(Error::Expired)));
    f.no_destinations(&host);
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revocation_closes_active_connection_and_both_transports() {
    let mut f = Fixture::new(Duration::from_secs(30)).await;
    let host = f.host().await;
    let guest = f.guest().await;
    let (mut send, mut recv) = guest.open_tcp("game", 1).await.unwrap();
    send.write_all(b"one").await.unwrap();
    let mut bytes = [0; 3];
    recv.read_exact(&mut bytes).await.unwrap();
    let udp = guest.open_udp("aux", 1).await.unwrap();
    udp.send(b"two").await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(2), udp.recv())
            .await
            .unwrap()
            .unwrap()
            .as_ref(),
        b"two"
    );
    assert!(host.revoke(&f.invitation.lease().lease_id));
    timeout(Duration::from_secs(2), guest.connection().closed())
        .await
        .unwrap();
    assert!(guest.open_tcp("game", 1).await.is_err());
    assert!(udp.recv().await.is_err());
    guest.shutdown().await;
    idle(&host).await;
    assert_eq!(host.metrics().active_tcp, 0);
    assert_eq!(host.metrics().active_udp, 0);
    assert!(matches!(f.guest_attempt().await, Err(Error::Expired)));
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expiration_refuses_saved_verified_invitation_and_closes_admitted_peer() {
    let mut f = Fixture::new(Duration::from_millis(700)).await;
    let host = f.host().await;
    let guest = f.guest().await;
    timeout(Duration::from_secs(2), guest.connection().closed())
        .await
        .unwrap();
    assert!(matches!(f.guest_attempt().await, Err(Error::Expired)));
    assert!(matches!(
        Invitation::verify(
            &f.invitation.encode(),
            f.host_key.public(),
            f.guest_key.public()
        ),
        Err(Error::Expired)
    ));
    f.no_destinations(&host);
    guest.shutdown().await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manual_mode_rejects_invitation_proof_but_retains_authorized_reconnection() {
    let mut f = Fixture::new(Duration::from_secs(30)).await;
    f.reservation.take();
    let host = Host::start(f.config()).await.unwrap();
    assert_eq!(
        raw_hello(&f, f.guest_key.clone(), &f.hello()).await["error"],
        "unauthorized"
    );
    f.no_destinations(&host);
    for _ in 0..2 {
        let guest = Guest::connect(GuestConfig {
            secret_key: f.guest_key.clone(),
            bind_addr: local(),
            host: host.addr(),
            session_id: f.invitation.session_id().into(),
            lease_id: f.invitation.lease().lease_id,
            capabilities: vec![Capability::ReliableStream, Capability::Datagram],
            limits: limits(),
        })
        .await
        .unwrap();
        guest.shutdown().await;
        idle(&host).await;
    }
    host.shutdown().await;
}

#[tokio::test]
async fn ipv6_loopback_and_game_only_invitation_are_supported_without_aux_permission() {
    let host_key = generate_key();
    let guest_key = generate_key();
    let listener = TcpListener::bind("[::1]:0").await.unwrap();
    let reservation = std::net::UdpSocket::bind("[::1]:0").unwrap();
    let services = vec![service(
        "game",
        Protocol::Tcp,
        listener.local_addr().unwrap(),
    )];
    let invitation = Invitation::issue(
        &host_key,
        guest_key.public(),
        "ipv6",
        reservation.local_addr().unwrap(),
        &services,
        unix_millis() + 30_000,
    )
    .unwrap();
    drop(reservation);
    let host = Host::start_invited(
        HostConfig {
            secret_key: host_key.clone(),
            bind_addr: invitation.direct_addr(),
            session_id: "ipv6".into(),
            services,
            leases: vec![invitation.lease()],
            limits: limits(),
        },
        &invitation,
    )
    .await
    .unwrap();
    let verified =
        Invitation::verify(&invitation.encode(), host_key.public(), guest_key.public()).unwrap();
    let guest = Guest::connect_invited(guest_key, &verified, limits())
        .await
        .unwrap();
    assert!(matches!(
        guest.open_udp("aux", 1).await,
        Err(Error::ServiceDenied)
    ));
    guest.shutdown().await;
    host.shutdown().await;
}
