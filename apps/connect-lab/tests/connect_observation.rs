//! Actual owned loopback endpoints. Observation never changes authentication.
use aurora_connect_lab::*;
use iroh::{Endpoint, RelayMode, endpoint::presets};
use serde_json::Value;
use std::{net::SocketAddr, time::Duration};
use tokio::{io::AsyncReadExt, net::TcpListener, time::timeout};

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn limits() -> Limits {
    Limits {
        handshake_timeout: Duration::from_secs(1),
        ..Limits::default()
    }
}
struct Fixture {
    host_key: SecretKey,
    guest_key: SecretKey,
    invitation: Invitation,
    service: Service,
    canary: TcpListener,
    reservation: Option<std::net::UdpSocket>,
}
impl Fixture {
    async fn new() -> Self {
        let canary = TcpListener::bind(local()).await.unwrap();
        let service = Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: canary.local_addr().unwrap(),
            revision: 1,
            max_datagram_bytes: 1024,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        };
        let reservation = std::net::UdpSocket::bind(local()).unwrap();
        let host_key = generate_key();
        let guest_key = generate_key();
        let invitation = Invitation::issue(
            &host_key,
            guest_key.public(),
            "observed-dial",
            reservation.local_addr().unwrap(),
            std::slice::from_ref(&service),
            unix_millis() + 30000,
        )
        .unwrap();
        Self {
            host_key,
            guest_key,
            invitation,
            service,
            canary,
            reservation: Some(reservation),
        }
    }
    async fn host(&mut self) -> Host {
        self.reservation.take();
        Host::start_invited(
            HostConfig {
                secret_key: self.host_key.clone(),
                bind_addr: self.invitation.direct_addr(),
                session_id: self.invitation.session_id().into(),
                services: vec![self.service.clone()],
                leases: vec![self.invitation.lease()],
                limits: limits(),
            },
            &self.invitation,
        )
        .await
        .unwrap()
    }
    async fn no_destination(&self, host: &Host) {
        assert_eq!(host.metrics().destination_tcp_connects, 0);
        assert_eq!(host.metrics().udp_associations_opened, 0);
        assert!(
            timeout(Duration::from_millis(30), self.canary.accept())
                .await
                .is_err()
        );
    }
}
fn schema(observation: GuestConnectObservation, phase: GuestConnectPhase) {
    assert_eq!(observation.phase, phase);
    let value = serde_json::to_value(observation).unwrap();
    let object = value.as_object().unwrap();
    assert_eq!(object.len(), 3);
    assert_eq!(value["schemaVersion"], 1);
    assert!(value["elapsedMs"].is_u64());
    assert!(matches!(
        value["phase"].as_str(),
        Some(
            "preflight"
                | "endpoint_bind"
                | "endpoint_connect"
                | "control_open"
                | "hello_write"
                | "hello_ack"
                | "admitted"
        )
    ));
}
async fn close(host: Host) {
    let addresses = host.bound_sockets();
    host.shutdown().await;
    drop(host);
    for address in addresses {
        drop(std::net::UdpSocket::bind(address).unwrap());
    }
}

#[tokio::test]
async fn silent_udp_reports_endpoint_connect_and_releases_guest_socket() {
    let mut f = Fixture::new().await;
    let sink = f.reservation.take().unwrap();
    sink.set_nonblocking(true).unwrap();
    let sink = tokio::net::UdpSocket::from_std(sink).unwrap();
    let (result, observation) =
        Guest::connect_invited_observed(f.guest_key.clone(), &f.invitation, limits()).await;
    assert!(matches!(result, Err(Error::Timeout)));
    schema(observation, GuestConnectPhase::EndpointConnect);
    assert!(observation.elapsed_ms >= 1000);
    let mut bytes = [0; 2048];
    let (_, source) = timeout(Duration::from_secs(1), sink.recv_from(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    // Source was actually used by the failed dial; awaited cleanup released it.
    drop(std::net::UdpSocket::bind(source).unwrap());
    assert!(
        timeout(Duration::from_millis(30), f.canary.accept())
            .await
            .is_err()
    );
    drop(sink);
    drop(std::net::UdpSocket::bind(f.invitation.direct_addr()).unwrap());
}

#[tokio::test]
async fn authenticated_tls_without_ack_reports_hello_ack_and_closes_connection() {
    let mut f = Fixture::new().await;
    f.reservation.take();
    let server = Endpoint::builder(presets::Minimal)
        .clear_ip_transports()
        .clear_relay_transports()
        .clear_address_lookup()
        .relay_mode(RelayMode::Disabled)
        .keylog(false)
        .secret_key(f.host_key.clone())
        .alpns(vec![ALPN.to_vec()])
        .bind_addr(f.invitation.direct_addr())
        .unwrap()
        .bind()
        .await
        .unwrap();
    let serving = server.clone();
    let expected = f.guest_key.public();
    let task = tokio::spawn(async move {
        let connection = serving.accept().await.unwrap().await.unwrap();
        assert!(
            connection.remote_id() == expected,
            "authenticated fixture peer"
        );
        let (send, mut recv) = connection.accept_bi().await.unwrap();
        let size = recv.read_u32().await.unwrap() as usize;
        assert!(size <= MAX_INVITATION_BYTES);
        let mut data = vec![0; size];
        recv.read_exact(&mut data).await.unwrap();
        let hello: Value = serde_json::from_slice(&data).unwrap();
        assert_eq!(hello["schemaVersion"], 1);
        // Keep the response stream open; no ACK is ever emitted.
        timeout(Duration::from_secs(5), connection.closed())
            .await
            .unwrap();
        drop((send, recv));
    });
    let (result, observation) =
        Guest::connect_invited_observed(f.guest_key.clone(), &f.invitation, limits()).await;
    assert!(matches!(result, Err(Error::Timeout)));
    schema(observation, GuestConnectPhase::HelloAck);
    timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(
        timeout(Duration::from_millis(30), f.canary.accept())
            .await
            .is_err()
    );
    server.close().await;
    drop(server);
    drop(std::net::UdpSocket::bind(f.invitation.direct_addr()).unwrap());
}

#[tokio::test]
async fn admitted_tcp_then_replay_and_revocation_keep_original_errors() {
    let mut f = Fixture::new().await;
    let host = f.host().await;
    let (result, observation) =
        Guest::connect_invited_observed(f.guest_key.clone(), &f.invitation, limits()).await;
    let guest = result.unwrap();
    schema(observation, GuestConnectPhase::Admitted);
    let (mut send, mut recv) = guest.open_tcp("game", 1).await.unwrap();
    let (mut destination, _) = timeout(Duration::from_secs(1), f.canary.accept())
        .await
        .unwrap()
        .unwrap();
    send.write_all(b"observed tcp").await.unwrap();
    let mut bytes = [0; 12];
    destination.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"observed tcp");
    tokio::io::AsyncWriteExt::write_all(&mut destination, b"observed tcp")
        .await
        .unwrap();
    recv.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"observed tcp");
    drop((send, recv));
    let addresses = guest.bound_sockets();
    guest.shutdown().await;
    drop(guest);
    assert!(matches!(
        timeout(Duration::from_secs(2), destination.read(&mut bytes))
            .await
            .unwrap(),
        Ok(0) | Err(_)
    ));
    for address in addresses {
        drop(std::net::UdpSocket::bind(address).unwrap());
    }
    let (replay, observation) =
        Guest::connect_invited_observed(f.guest_key.clone(), &f.invitation, limits()).await;
    assert!(matches!(replay, Err(Error::Replay)));
    schema(observation, GuestConnectPhase::HelloAck);
    assert!(host.revoke(&f.invitation.lease().lease_id));
    let (revoked, observation) =
        Guest::connect_invited_observed(f.guest_key.clone(), &f.invitation, limits()).await;
    assert!(matches!(revoked, Err(Error::Expired)));
    schema(observation, GuestConnectPhase::HelloAck);
    assert_eq!(host.metrics().destination_tcp_connects, 1);
    close(host).await;
}

#[tokio::test]
async fn unknown_signed_lease_is_refused_without_destination_or_consumption() {
    let mut f = Fixture::new().await;
    let host = f.host().await;
    let other = Invitation::issue(
        &f.host_key,
        f.guest_key.public(),
        f.invitation.session_id(),
        f.invitation.direct_addr(),
        std::slice::from_ref(&f.service),
        unix_millis() + 30000,
    )
    .unwrap();
    let (result, observation) =
        Guest::connect_invited_observed(f.guest_key.clone(), &other, limits()).await;
    assert!(matches!(result, Err(Error::Unauthorized)));
    schema(observation, GuestConnectPhase::HelloAck);
    f.no_destination(&host).await;
    // Original default API still admits the untouched original invitation.
    let guest = Guest::connect_invited(f.guest_key.clone(), &f.invitation, limits())
        .await
        .unwrap();
    guest.shutdown().await;
    close(host).await;
}

#[tokio::test]
async fn wrong_key_and_explicit_address_mode_fail_in_preflight() {
    let f = Fixture::new().await;
    let (result, observation) =
        Guest::connect_invited_observed(generate_key(), &f.invitation, limits()).await;
    assert!(matches!(result, Err(Error::Unauthorized)));
    schema(observation, GuestConnectPhase::Preflight);
    let (result, observation) =
        Guest::connect_invited_external_ipv6_observed(f.guest_key.clone(), &f.invitation, limits())
            .await;
    assert!(matches!(result, Err(Error::Configuration)));
    schema(observation, GuestConnectPhase::Preflight);
    let sink = f.reservation.as_ref().unwrap();
    sink.set_nonblocking(true).unwrap();
    assert_eq!(
        sink.recv(&mut [0; 2048]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
