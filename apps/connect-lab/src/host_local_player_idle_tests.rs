//! Local-player transport lifetime, without application traffic or Java.
use super::*;
use std::time::Instant;

const IDLE: Duration = Duration::from_secs(7);

struct Fixture {
    relay: LocalRelayFixture,
    host: Host,
    player: Guest,
    other: Guest,
    invitation: SessionInvitation,
    other_invitation: SessionInvitation,
    local_ticket: HostLocalPlayerTicket,
    unused_ticket: HostLocalPlayerTicket,
    tcp: TcpListener,
    udp: UdpSocket,
}
impl Fixture {
    async fn start(session_ms: u64) -> Self {
        assert_eq!(Limits::default().idle_timeout, Duration::from_secs(120));
        let host_key = generate_key();
        let player_key = generate_key();
        let other_key = generate_key();
        let authority = generate_key();
        let relay = LocalRelayFixture::start(
            [host_key.public(), player_key.public()],
            LocalRelayProfile::Normal,
        )
        .await
        .unwrap();
        let origin = relay
            .ticket()
            .url()
            .as_str()
            .trim_end_matches('/')
            .to_owned();
        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
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
            NativeVoiceBudget::service(udp.local_addr().unwrap()),
        ];
        let expiry = unix_millis() + session_ms;
        let invitation = SessionInvitation::issue_online_native_voice(
            &host_key,
            player_key.public(),
            "online-voice-test",
            &services,
            SessionDeadlines {
                admission_expires_at_unix_ms: expiry.min(unix_millis() + 6_000),
                session_expires_at_unix_ms: expiry,
            },
        )
        .unwrap();
        let other_invitation = SessionInvitation::issue_online_native_voice(
            &host_key,
            other_key.public(),
            "online-voice-test",
            &services,
            SessionDeadlines {
                admission_expires_at_unix_ms: invitation.admission_expires_at_unix_ms(),
                session_expires_at_unix_ms: expiry,
            },
        )
        .unwrap();
        let network = ticket(
            &authority,
            host_key.public(),
            host_key.public(),
            &origin,
            expiry,
        );
        let mut limits = NativeVoiceBudget::limits();
        limits.idle_timeout = IDLE;
        let host = bounded(Host::start_invited_online_native_voice(
            HostConfig {
                secret_key: host_key,
                bind_addr: "0.0.0.0:0".parse().unwrap(),
                session_id: "online-voice-test".into(),
                services,
                leases: vec![invitation.lease()],
                limits: limits.clone(),
            },
            &invitation,
            &network,
        ))
        .await
        .unwrap();
        host.admit_online(&other_invitation).unwrap();
        let local_ticket = host.local_player_ticket(&invitation, &player_key).unwrap();
        let unused_ticket = host.local_player_ticket(&invitation, &player_key).unwrap();
        assert_eq!(
            local_ticket.refresh_idle_activity(&invitation),
            Err(Error::Unauthorized)
        );
        let player = bounded(Guest::connect_local_authorized(
            player_key,
            &invitation,
            limits.clone(),
            &local_ticket,
        ))
        .await
        .unwrap();
        assert_eq!(
            unused_ticket.refresh_idle_activity(&invitation),
            Err(Error::Unauthorized)
        );
        let other_ticket = host
            .local_player_ticket(&other_invitation, &other_key)
            .unwrap();
        let other = bounded(Guest::connect_local_authorized(
            other_key,
            &other_invitation,
            limits,
            &other_ticket,
        ))
        .await
        .unwrap();
        assert!(host.invitation_active(invitation.invitation_id()));
        Self {
            relay,
            host,
            player,
            other,
            invitation,
            other_invitation,
            local_ticket,
            unused_ticket,
            tcp,
            udp,
        }
    }
    async fn no_application_traffic(&self) {
        assert_eq!(self.host.metrics().destination_tcp_connects, 0);
        assert_eq!(self.host.metrics().udp_associations_opened, 0);
        assert!(
            tokio::time::timeout(Duration::from_millis(30), self.tcp.accept())
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(30), self.udp.recv_from(&mut [0; 1]))
                .await
                .is_err()
        );
    }
    async fn stop(self) {
        let tcp_address = self.tcp.local_addr().unwrap();
        let udp_address = self.udp.local_addr().unwrap();
        self.player.shutdown().await;
        self.other.shutdown().await;
        self.host.shutdown().await;
        assert!(
            self.local_ticket
                .refresh_idle_activity(&self.invitation)
                .is_err()
        );
        drop((
            self.player,
            self.other,
            self.host,
            self.local_ticket,
            self.unused_ticket,
            self.tcp,
            self.udp,
        ));
        self.relay.shutdown().await.unwrap();
        drop(std::net::TcpListener::bind(tcp_address).unwrap());
        drop(std::net::UdpSocket::bind(udp_address).unwrap());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_local_player_idle_activity_is_scoped_and_never_renews_authority() {
    let fixture = Fixture::start(40_000).await;
    let expiry = fixture.invitation.session_expires_at_unix_ms();
    let admission = fixture.invitation.admission_expires_at_unix_ms();
    let until = Instant::now() + IDLE * 2 + Duration::from_millis(500);
    while Instant::now() < until {
        assert!(
            fixture
                .local_ticket
                .refresh_idle_activity(&fixture.other_invitation)
                .is_err()
        );
        assert!(
            fixture
                .unused_ticket
                .refresh_idle_activity(&fixture.invitation)
                .is_err()
        );
        fixture
            .local_ticket
            .refresh_idle_activity(&fixture.invitation)
            .unwrap();
        assert!(
            fixture
                .host
                .invitation_active(fixture.invitation.invitation_id())
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(fixture.invitation.check_admission_current().is_err());
    assert!(fixture.invitation.check_session_current().is_ok());
    assert_eq!(fixture.invitation.session_expires_at_unix_ms(), expiry);
    assert_eq!(fixture.invitation.admission_expires_at_unix_ms(), admission);
    assert!(
        !fixture
            .host
            .invitation_active(fixture.other_invitation.invitation_id())
    );
    assert!(fixture.other.connection().close_reason().is_some());
    fixture.no_application_traffic().await;
    // Activity stops: the unchanged idle timer still retires the same lease.
    let stopped = Instant::now();
    let reason = bounded(fixture.player.connection().closed()).await;
    assert!(matches!(
        reason,
        iroh::endpoint::ConnectionError::ApplicationClosed(_)
    ));
    assert!(stopped.elapsed() >= IDLE - Duration::from_millis(400));
    assert!(fixture.invitation.check_session_current().is_ok());
    assert!(
        fixture
            .local_ticket
            .refresh_idle_activity(&fixture.invitation)
            .is_err()
    );
    fixture.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_local_player_idle_activity_cannot_override_revocation_or_session_expiry() {
    for expire in [false, true] {
        let fixture = Fixture::start(if expire { 8_500 } else { 30_000 }).await;
        fixture
            .local_ticket
            .refresh_idle_activity(&fixture.invitation)
            .unwrap();
        if expire {
            let limit = Instant::now() + Duration::from_secs(10);
            while fixture.invitation.check_session_current().is_ok() {
                assert!(
                    Instant::now() < limit,
                    "activity must not extend the session"
                );
                if fixture
                    .local_ticket
                    .refresh_idle_activity(&fixture.invitation)
                    .is_err()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        } else {
            assert!(fixture.host.revoke(fixture.invitation.invitation_id()));
        }
        bounded(fixture.player.connection().closed()).await;
        assert!(
            fixture
                .local_ticket
                .refresh_idle_activity(&fixture.invitation)
                .is_err()
        );
        assert!(
            !fixture
                .host
                .invitation_active(fixture.invitation.invitation_id())
        );
        if expire {
            assert!(fixture.invitation.check_session_current().is_err());
        } else {
            assert!(fixture.invitation.check_session_current().is_ok());
        }
        fixture.no_application_traffic().await;
        fixture.stop().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_local_player_idle_without_native_activity_closes_before_session_expiry() {
    let fixture = Fixture::start(30_000).await;
    let start = Instant::now();
    let reason = bounded(fixture.player.connection().closed()).await;
    assert!(
        matches!(
            reason,
            iroh::endpoint::ConnectionError::ApplicationClosed(_)
        ),
        "expected application idle closure"
    );
    assert!(start.elapsed() >= IDLE - Duration::from_millis(300));
    assert!(fixture.invitation.check_session_current().is_ok());
    assert!(
        !fixture
            .host
            .invitation_active(fixture.invitation.invitation_id())
    );
    fixture.no_application_traffic().await;
    fixture.stop().await;
}
