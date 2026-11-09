use super::*;
use crate::{
    Capability, Guest, Host, Limits, LocalRelayFixture, LocalRelayProfile, OnlineClockBinding,
    Protocol, SecretKey, Service, SessionDeadlines, SessionInvitation, UdpReturnPolicy,
    generate_key, online_clock::tests::signed_for,
};
use tokio::{
    net::{TcpListener, UdpSocket},
    time::{timeout, timeout_at},
};

const SESSION: &str = "26132648-780e-4697-8a82-1697c1aa8430";
const ISSUER: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn clock(
    authority: &SecretKey,
    scope: OnlineClockBinding<'_>,
    server_now: u64,
    expiry: u64,
    received: Instant,
    transit: Duration,
) -> OnlineClock {
    let processing = Duration::from_millis(2_500);
    OnlineClock::verify_bootstrap(
        &signed_for(
            authority,
            &scope,
            server_now - processing.as_millis() as u64,
            server_now,
            expiry,
        ),
        authority.public(),
        scope,
        received - (transit + processing),
        received,
    )
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn ticket(
    authority: &SecretKey,
    origin: &str,
    scope: &OnlineClock,
    account: &str,
    local: EndpointId,
    host: EndpointId,
    issued: u64,
    expiry: u64,
) -> OnlineNetworkTicket {
    let body = Body {
        schema_version: 1,
        session_id: SESSION.into(),
        issuer_instance: ISSUER.into(),
        peer_id: local.to_string(),
        host_peer_id: host.to_string(),
        account_id: account.into(),
        generation: scope.generation(),
        issued_at_unix_ms: issued,
        expires_at_unix_ms: expiry,
        direct_addresses: Vec::new(),
    };
    let signed = super::tests::signed(authority, &body);
    assert!(
        OnlineNetworkTicket::verify_at(
            &signed,
            authority.public(),
            ISSUER,
            account,
            SESSION,
            local,
            host,
            origin,
            true,
            None
        )
        .is_err()
    );
    OnlineNetworkTicket::verify_at(
        &signed,
        authority.public(),
        ISSUER,
        account,
        SESSION,
        local,
        host,
        origin,
        true,
        Some(scope),
    )
    .unwrap()
}

async fn echo_until_original_deadline(skew_ms: i64) {
    let authority = generate_key();
    let host_key = generate_key();
    let guest_key = generate_key();
    let relay = LocalRelayFixture::start(
        [host_key.public(), guest_key.public()],
        LocalRelayProfile::Normal,
    )
    .await
    .unwrap();
    let relay_ticket = relay.ticket();
    let relay_url = relay_ticket.url();
    let origin = relay_url.as_str().trim_end_matches('/');
    let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let services = [
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
    let tcp_echo = tokio::spawn(async move {
        let (mut stream, _) = tcp.accept().await.unwrap();
        let (mut reader, mut writer) = stream.split();
        tokio::io::copy(&mut reader, &mut writer).await
    });
    let udp_echo = tokio::spawn(async move {
        let mut bytes = [0; 4097];
        let (count, sender) = udp.recv_from(&mut bytes).await.unwrap();
        udp.send_to(&bytes[..count], sender).await.unwrap();
        count
    });

    let sample = Instant::now();
    let server_now = unix_millis().checked_add_signed(skew_ms).unwrap();
    let expiry = server_now + 8_000;
    let host_clock = clock(
        &authority,
        OnlineClockBinding {
            issuer: ISSUER,
            account: "clock-host",
            session: SESSION,
            operation: "095d4404-8ce1-4b3f-bb12-7881d724bd72",
            generation: 3,
            nonce: "10101010101010101010101010101010",
        },
        server_now,
        expiry,
        sample,
        Duration::from_millis(45),
    );
    let guest_clock = clock(
        &authority,
        OnlineClockBinding {
            issuer: ISSUER,
            account: "clock-guest",
            session: SESSION,
            operation: "a5f20a41-76a3-4a21-b22e-8af275ac0901",
            generation: 7,
            nonce: "23232323232323232323232323232323",
        },
        server_now,
        expiry,
        sample,
        Duration::from_millis(5),
    );
    let host_ticket = ticket(
        &authority,
        origin,
        &host_clock,
        "clock-host",
        host_key.public(),
        host_key.public(),
        server_now,
        expiry,
    );
    let guest_ticket = ticket(
        &authority,
        origin,
        &guest_clock,
        "clock-guest",
        guest_key.public(),
        host_key.public(),
        server_now,
        expiry,
    );
    let invitation = SessionInvitation::issue_online_with_clock(
        &host_key,
        guest_key.public(),
        SESSION,
        &services,
        SessionDeadlines {
            admission_expires_at_unix_ms: expiry - 1_000,
            session_expires_at_unix_ms: expiry,
        },
        true,
        &host_clock,
    )
    .unwrap();
    assert!(matches!(
        Guest::connect_invited_online_fragmented(
            guest_key.clone(),
            &invitation,
            Limits::default(),
            &guest_ticket,
        )
        .await,
        Err(Error::Unauthorized)
    ));
    let received = SessionInvitation::verify_online_with_clock(
        &invitation.encode(),
        host_key.public(),
        guest_key.public(),
        true,
        &guest_clock,
    )
    .unwrap();
    let host = Host::start_invited_online_fragmented(
        HostConfig {
            secret_key: host_key,
            bind_addr: "0.0.0.0:0".parse().unwrap(),
            session_id: SESSION.into(),
            services: services.to_vec(),
            leases: vec![invitation.lease()],
            limits: Limits::default(),
        },
        &invitation,
        &host_ticket,
    )
    .await
    .unwrap();
    let guest = Guest::connect_invited_online_fragmented(
        guest_key,
        &received,
        Limits::default(),
        &guest_ticket,
    )
    .await
    .unwrap();
    assert!(host.invitation_consumed(invitation.invitation_id()));

    let (mut send, mut recv) = guest.open_tcp("game", 1).await.unwrap();
    let payload = vec![0x42; 4096];
    send.write_all(&payload).await.unwrap();
    let mut bytes = vec![0; 4096];
    recv.read_exact(&mut bytes).await.unwrap();
    assert_eq!(bytes, payload);

    let bridge = guest
        .udp_bridge_fragmented("aux", 1, "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let udp_client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    udp_client
        .send_to(&payload, bridge.local_addr())
        .await
        .unwrap();
    let (count, sender) = udp_client.recv_from(&mut bytes).await.unwrap();
    assert_eq!(sender, bridge.local_addr());
    assert_eq!(&bytes[..count], payload);
    assert_eq!(udp_echo.await.unwrap(), payload.len());

    let deadline = host_ticket.deadline();
    assert!(Instant::now() < deadline);
    timeout_at(
        (deadline + Duration::from_secs(2)).into(),
        guest.connection().closed(),
    )
    .await
    .expect("session reaches its original monotonic expiry");
    assert!(Instant::now() >= deadline);
    assert!(!host.invitation_active(invitation.invitation_id()));
    assert!(guest.open_tcp("game", 1).await.is_err());
    assert!(guest.open_udp_fragmented("aux", 1).await.is_err());
    bridge.shutdown().await;
    guest.shutdown().await;
    host.shutdown().await;
    let _ = tcp_echo.await.unwrap();
    relay.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn authenticated_clock_online_tcp_and_fragmented_udp_expire_at_the_original_deadline() {
    let year_ms = 365 * 24 * 60 * 60 * 1000_i64;
    timeout(Duration::from_secs(16), async {
        tokio::join!(
            echo_until_original_deadline(-year_ms),
            echo_until_original_deadline(year_ms)
        );
    })
    .await
    .expect("both owned relay scenarios complete");
}
