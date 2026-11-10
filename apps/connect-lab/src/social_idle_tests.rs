//! Real two-agent Social idle lifetime; all listeners are owned ephemeral ports.
use super::*;
use crate::{
    Capability, Guest, Host, HostConfig, Limits, LocalRelayFixture, LocalRelayProfile,
    NativeVoiceBudget, OnlineNetworkTicket, Protocol, Service, SessionDeadlines, SessionInvitation,
    UdpReturnPolicy, generate_key,
};
use tokio::net::{TcpListener, UdpSocket};

const IDLE: Duration = Duration::from_secs(7);
const SESSION: &str = "11111111-2222-4333-8444-555555555555";
const OPERATION: &str = "22222222-2222-4333-8444-555555555555";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TicketBody {
    schema_version: u8,
    session_id: String,
    issuer_instance: String,
    peer_id: String,
    host_peer_id: String,
    account_id: String,
    generation: u64,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
    direct_addresses: Vec<String>,
}
struct Authority {
    key: SecretKey,
    host_key: SecretKey,
    guest_key: SecretKey,
    grant: VerifiedSocialGrant,
    invitation: SessionInvitation,
    services: Vec<Service>,
    challenge: SocialKeyChallenge,
    policy: SignedSocialHostPolicy,
    cap: Instant,
}
impl Authority {
    fn new(source: SocketAddr, udp: SocketAddr, session_ms: u64) -> Self {
        let key = generate_key();
        let host_key = generate_key();
        let guest_key = generate_key();
        let now = unix_millis();
        let challenge = SocialKeyChallenge::parse(
            &serde_json::json!({
                "schemaVersion":1,"purpose":"accountKeyBinding","issuerInstance":"33".repeat(32),
                "challengeId":"44".repeat(32),"accountId":"idle-host","operationId":OPERATION,
                "generation":1,"role":"host","publicKey":host_key.public().to_string(),
                "sessionId":SESSION,"sessionDigest":"55".repeat(32),
                "issuedAtUnixMs":now,"expiresAtUnixMs":now+60_000,
            })
            .to_string(),
        )
        .unwrap();
        let review = SocialHostPolicy::review_online_voice(
            &challenge,
            "public",
            source,
            "open",
            now + session_ms,
            "svc",
            &"66".repeat(32),
            false,
        )
        .unwrap()
        .with_runtime_profile("fabric-loader-0.16.14-1.21.1")
        .unwrap()
        .with_native_voice_transport()
        .unwrap();
        let cap = Instant::now() + Duration::from_secs(60);
        let policy = review.approve(&host_key, &review.digest(), cap).unwrap();
        let body = GrantBody {
            schema_version: 5,
            purpose: "socialAdmission".into(),
            issuer_instance: challenge.issuer_instance().into(),
            grant_id: "77".repeat(32),
            policy_id: challenge.challenge_id().into(),
            policy_digest: review.digest(),
            session_id: SESSION.into(),
            session_digest: challenge.session_digest().into(),
            host_account_id: challenge.account_id().into(),
            host_public_key: host_key.public().to_string(),
            host_operation_id: OPERATION.into(),
            host_generation: 1,
            guest_account_id: "idle-guest".into(),
            guest_public_key: guest_key.public().to_string(),
            guest_operation_id: "33333333-2222-4333-8444-555555555555".into(),
            guest_generation: 1,
            issued_at_unix_ms: now,
            admission_expires_at_unix_ms: now + 6_000,
            session_expires_at_unix_ms: now + session_ms,
            voice_profile: Some("svc".into()),
            voice_source_digest: Some("66".repeat(32)),
            host_player: Some(false),
            runtime_profile: Some("fabric-loader-0.16.14-1.21.1".into()),
            voice_transport_profile: Some("nativeVoice8V1".into()),
        };
        let encoded = serde_json::to_string(&GrantEnvelope {
            signature: hex(&key.sign(&body.bytes()).to_bytes()),
            body,
        })
        .unwrap();
        let grant =
            VerifiedSocialGrant::for_host(&encoded, key.public(), &challenge, &policy, source, cap)
                .unwrap();
        let services = vec![
            Service {
                service_id: "game".into(),
                protocol: Protocol::Tcp,
                endpoint: source,
                revision: 1,
                max_datagram_bytes: 1400,
                required_capability: Capability::ReliableStream,
                udp_return_policy: UdpReturnPolicy::ReplyBudget,
            },
            NativeVoiceBudget::service(udp),
        ];
        let invitation = SessionInvitation::issue_online_native_voice(
            &host_key,
            guest_key.public(),
            SESSION,
            &services,
            SessionDeadlines {
                admission_expires_at_unix_ms: grant.admission_expires_at_unix_ms(),
                session_expires_at_unix_ms: grant.session_expires_at_unix_ms(),
            },
        )
        .unwrap();
        Self {
            key,
            host_key,
            guest_key,
            grant,
            invitation,
            services,
            challenge,
            policy,
            cap,
        }
    }
    fn ticket_encoded(&self, guest: bool) -> String {
        let body = TicketBody {
            schema_version: 1,
            session_id: SESSION.into(),
            issuer_instance: self.grant.body.issuer_instance.clone(),
            peer_id: if guest {
                self.guest_key.public()
            } else {
                self.host_key.public()
            }
            .to_string(),
            host_peer_id: self.host_key.public().to_string(),
            account_id: if guest { "idle-guest" } else { "idle-host" }.into(),
            generation: 1,
            issued_at_unix_ms: self.grant.body.issued_at_unix_ms,
            expires_at_unix_ms: self.grant.session_expires_at_unix_ms(),
            direct_addresses: vec![],
        };
        let mut material = b"aurora/connect/relay-ticket/v1\0".to_vec();
        material.extend(serde_json::to_vec(&body).unwrap());
        serde_json::json!({"signature":hex(&self.key.sign(&material).to_bytes()),"body":body})
            .to_string()
    }
    fn ticket(&self, origin: &str, guest: bool) -> OnlineNetworkTicket {
        OnlineNetworkTicket::for_local_test(
            &self.ticket_encoded(guest),
            self.key.public(),
            &self.grant.body.issuer_instance,
            if guest { "idle-guest" } else { "idle-host" },
            SESSION,
            if guest {
                self.guest_key.public()
            } else {
                self.host_key.public()
            },
            self.host_key.public(),
            origin,
        )
        .unwrap()
    }
    fn clock(&self) -> OnlineClock {
        let binding = crate::OnlineClockBinding {
            issuer: &self.grant.body.issuer_instance,
            account: "idle-host",
            session: SESSION,
            operation: OPERATION,
            generation: 1,
            nonce: &"99".repeat(16),
        };
        let at = Instant::now();
        let now = unix_millis();
        let text = crate::online_clock::tests::signed_for(
            &self.key,
            &binding,
            now,
            now,
            self.grant.session_expires_at_unix_ms(),
        );
        OnlineClock::verify_bootstrap(&text, self.key.public(), binding, at, at).unwrap()
    }
    fn bind_clock(&mut self, clock: &OnlineClock) -> OnlineNetworkTicket {
        let encoded = serde_json::to_string(&GrantEnvelope {
            body: self.grant.body.clone(),
            signature: hex(&self.key.sign(&self.grant.body.bytes()).to_bytes()),
        })
        .unwrap();
        self.grant = VerifiedSocialGrant::for_host_with_clock(
            &encoded,
            self.key.public(),
            &self.challenge,
            &self.policy,
            self.services[0].endpoint,
            self.cap,
            Some(clock),
        )
        .unwrap();
        self.invitation = SessionInvitation::issue_online_native_voice_with_clock(
            &self.host_key,
            self.guest_key.public(),
            SESSION,
            &self.services,
            SessionDeadlines {
                admission_expires_at_unix_ms: self.grant.admission_expires_at_unix_ms(),
                session_expires_at_unix_ms: self.grant.session_expires_at_unix_ms(),
            },
            clock,
        )
        .unwrap();
        OnlineNetworkTicket::verify_clock_for_test(
            &self.ticket_encoded(false),
            self.key.public(),
            &self.grant.body.issuer_instance,
            "idle-host",
            SESSION,
            self.host_key.public(),
            self.host_key.public(),
            clock,
        )
        .unwrap()
    }
    fn lease(&self, sequence: u64) -> String {
        let now = unix_millis();
        let body = LeaseBody {
            schema_version: 1,
            purpose: "socialContinuity".into(),
            issuer_instance: self.grant.body.issuer_instance.clone(),
            grant_id: self.grant.body.grant_id.clone(),
            policy_digest: self.grant.body.policy_digest.clone(),
            sequence,
            issued_at_unix_ms: now,
            expires_at_unix_ms: (now + 3_000).min(self.grant.session_expires_at_unix_ms()),
        };
        serde_json::to_string(&LeaseEnvelope {
            signature: hex(&self.key.sign(&body.bytes()).to_bytes()),
            body,
        })
        .unwrap()
    }
    fn continuity(&self) -> SocialContinuity {
        SocialContinuity::start(&self.lease(1), &self.grant).unwrap()
    }
}
async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(12), future)
        .await
        .expect("social_idle_fixture_deadline")
}
struct Fixture {
    authority: Authority,
    relay: LocalRelayFixture,
    host: Host,
    guest: Guest,
    continuity: SocialContinuity,
    tcp: TcpListener,
    udp: UdpSocket,
}
impl Fixture {
    async fn start(session_ms: u64) -> Self {
        assert_eq!(Limits::default().idle_timeout, Duration::from_secs(120));
        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let authority = Authority::new(
            tcp.local_addr().unwrap(),
            udp.local_addr().unwrap(),
            session_ms,
        );
        let relay = LocalRelayFixture::start(
            [authority.host_key.public(), authority.guest_key.public()],
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
        let mut limits = NativeVoiceBudget::limits();
        limits.idle_timeout = IDLE;
        let host = bounded(Host::start_invited_online_native_voice(
            HostConfig {
                secret_key: authority.host_key.clone(),
                bind_addr: "0.0.0.0:0".parse().unwrap(),
                session_id: SESSION.into(),
                services: authority.services.clone(),
                leases: vec![authority.invitation.lease()],
                limits: limits.clone(),
            },
            &authority.invitation,
            &authority.ticket(&origin, false),
        ))
        .await
        .unwrap();
        let continuity = authority.continuity();
        assert!(
            host.refresh_social_idle(&authority.invitation, &continuity)
                .is_err(),
            "unused invitation is not activity"
        );
        let guest = bounded(Guest::connect_invited_online_native_voice(
            authority.guest_key.clone(),
            &authority.invitation,
            limits,
            &authority.ticket(&origin, true),
        ))
        .await
        .unwrap();
        Self {
            authority,
            relay,
            host,
            guest,
            continuity,
            tcp,
            udp,
        }
    }
    fn refresh(&self) -> crate::Result<()> {
        self.host
            .refresh_social_idle(&self.authority.invitation, &self.continuity)
    }
    fn update(&mut self) {
        let sequence = self.continuity.sequence() + 1;
        self.continuity
            .update(&self.authority.lease(sequence))
            .unwrap();
        self.refresh().unwrap();
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
        self.guest.shutdown().await;
        self.host.shutdown().await;
        assert!(self.refresh().is_err());
        drop((self.guest, self.host, self.tcp, self.udp));
        self.relay.shutdown().await.unwrap();
        drop(std::net::TcpListener::bind(tcp_address).unwrap());
        drop(std::net::UdpSocket::bind(udp_address).unwrap());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn social_idle_generic_online_peer_still_expires_without_witness() {
    let fixture = Fixture::start(30_000).await;
    let reason = bounded(fixture.guest.connection().closed()).await;
    assert!(matches!(
        reason,
        iroh::endpoint::ConnectionError::ApplicationClosed(_)
    ));
    assert!(fixture.authority.invitation.check_session_current().is_ok());
    assert!(fixture.refresh().is_err());
    fixture.no_application_traffic().await;
    fixture.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn social_idle_signed_continuity_keeps_empty_bridges_and_lost_lease_is_terminal() {
    let mut fixture = Fixture::start(40_000).await;
    fixture.refresh().unwrap();
    let expiry = fixture.authority.invitation.session_expires_at_unix_ms();
    let admission = fixture.authority.invitation.admission_expires_at_unix_ms();
    let until = Instant::now() + IDLE * 2 + Duration::from_millis(200);
    while Instant::now() < until {
        fixture.update();
        assert!(
            fixture
                .host
                .invitation_active(fixture.authority.invitation.invitation_id())
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(
        fixture
            .authority
            .invitation
            .check_admission_current()
            .is_err()
    );
    assert_eq!(
        fixture.authority.invitation.session_expires_at_unix_ms(),
        expiry
    );
    assert_eq!(
        fixture.authority.invitation.admission_expires_at_unix_ms(),
        admission
    );
    fixture.no_application_traffic().await;
    // A peer can stall a fresh control stream. Continuity closure must not wait
    // for the 10s control-read timeout, and still must not touch either target.
    let (mut stalled, _receive) = fixture.guest.connection().open_bi().await.unwrap();
    stalled.write_all(&[0]).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    // Repeating the exact sample cannot reset its original 3s signed deadline.
    let original_end = fixture.continuity.deadline;
    while Instant::now() < original_end {
        if fixture.refresh().is_err() {
            break;
        }
        assert_eq!(fixture.continuity.deadline, original_end);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    bounded(fixture.guest.connection().closed()).await;
    assert!(Instant::now().duration_since(original_end) < Duration::from_secs(1));
    assert!(fixture.authority.invitation.check_session_current().is_ok());
    assert!(fixture.refresh().is_err());
    // Even reconstructing a fresh sequence-one continuity cannot revive this lease.
    let reparsed = fixture.authority.continuity();
    assert!(
        fixture
            .host
            .refresh_social_idle(&fixture.authority.invitation, &reparsed)
            .is_err()
    );
    fixture.no_application_traffic().await;
    fixture.stop().await;
}

#[test]
fn social_idle_witness_binds_verified_host_source_account_pin_clock_and_invitation() {
    let mut authority = Authority::new(
        "127.0.0.1:32001".parse().unwrap(),
        "127.0.0.1:32002".parse().unwrap(),
        40_000,
    );
    let clock = authority.clock();
    let ticket = authority.bind_clock(&clock);
    let witness = |continuity: &SocialContinuity, invitation: &SessionInvitation| {
        continuity.host_idle_witness(invitation, &ticket, authority.services[0].endpoint)
    };
    let continuity = authority.continuity();
    witness(&continuity, &authority.invitation).unwrap();
    // These private mutations model mismatched individually verified scopes;
    // no public API can manufacture a VerifiedSocialGrant with raw fields.
    for kind in 0..14 {
        let mut changed = authority.continuity();
        match kind {
            0 => changed.grant.pin = generate_key().public(),
            1 => changed.grant.body.issuer_instance = "88".repeat(32),
            2 => changed.grant.body.host_account_id = "another-account".into(),
            3 => changed.grant.body.host_generation += 1,
            4 => changed.grant.host_source = Some("127.0.0.1:32003".parse().unwrap()),
            5 => changed.grant.host_source = None, // guest verification carries no source approval
            6 => changed.grant.body.host_public_key = generate_key().public().to_string(),
            7 => changed.grant.body.guest_public_key = generate_key().public().to_string(),
            8 => changed.grant.body.session_id = OPERATION.into(),
            9 => changed.grant.body.admission_expires_at_unix_ms += 1,
            10 => changed.grant.body.session_expires_at_unix_ms += 1,
            11 => changed.grant.body.schema_version = 2,
            12 => changed.grant.clock = None,
            _ => changed.grant.clock = Some(authority.clock()),
        }
        assert!(
            witness(&changed, &authority.invitation).is_err(),
            "scope {kind}"
        );
    }
    let other_clock = authority.clock();
    let other_invitation = SessionInvitation::issue_online_native_voice_with_clock(
        &authority.host_key,
        authority.guest_key.public(),
        SESSION,
        &authority.services,
        SessionDeadlines {
            admission_expires_at_unix_ms: authority.grant.admission_expires_at_unix_ms(),
            session_expires_at_unix_ms: authority.grant.session_expires_at_unix_ms(),
        },
        &other_clock,
    )
    .unwrap();
    assert!(witness(&continuity, &other_invitation).is_err());
    assert!(
        continuity
            .host_idle_witness(
                &authority.invitation,
                &authority.ticket("http://127.0.0.1:32004", true),
                authority.services[0].endpoint,
            )
            .is_err()
    );
}

#[test]
fn social_idle_witness_cannot_switch_grant_caps_sequence_or_reanchor_deadline() {
    let mut authority = Authority::new(
        "127.0.0.1:32001".parse().unwrap(),
        "127.0.0.1:32002".parse().unwrap(),
        40_000,
    );
    let clock = authority.clock();
    let ticket = authority.bind_clock(&clock);
    let mut continuity = authority.continuity();
    let witness = |value: &SocialContinuity| {
        value
            .host_idle_witness(
                &authority.invitation,
                &ticket,
                authority.services[0].endpoint,
            )
            .unwrap()
    };
    let mut original = witness(&continuity);
    let deadline = original.deadline;
    original.refresh(witness(&continuity)).unwrap();
    assert_eq!(original.deadline, deadline);
    for kind in 0..9 {
        let mut changed = witness(&continuity);
        match kind {
            0 => changed.grant.body.grant_id = "88".repeat(32),
            1 => changed.grant.body.policy_digest = "88".repeat(32),
            2 => changed.grant.body.host_operation_id = SESSION.into(),
            3 => changed.grant.body.guest_account_id = "different-guest".into(),
            4 => changed.grant.session_deadline += Duration::from_millis(1),
            5 => changed.grant.admission_deadline += Duration::from_millis(1),
            6 => changed.invitation_digest = "88".repeat(32),
            7 => changed.deadline += Duration::from_millis(1),
            _ => changed.lease.issued_at_unix_ms -= 1,
        }
        assert!(original.refresh(changed).is_err(), "immutable scope {kind}");
        assert_eq!(original.deadline, deadline);
    }
    let previous = witness(&continuity);
    continuity.update(&authority.lease(2)).unwrap();
    original.refresh(witness(&continuity)).unwrap();
    assert_eq!(original.lease.sequence, 2);
    assert_eq!(original.deadline, continuity.deadline);
    assert_eq!(
        original.refresh(previous),
        Err(SocialAdmissionError::Sequence)
    );
    original.deadline = Instant::now() - Duration::from_millis(1);
    continuity.update(&authority.lease(3)).unwrap();
    assert_eq!(
        original.refresh(witness(&continuity)),
        Err(SocialAdmissionError::Expired)
    );
    // Failed authority updates are terminal and cannot produce a new witness.
    assert!(continuity.update(&authority.lease(3)).is_err());
    assert!(
        continuity
            .host_idle_witness(
                &authority.invitation,
                &ticket,
                authority.services[0].endpoint
            )
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn social_idle_revocation_and_original_session_win_over_fresh_continuity() {
    for expire in [false, true] {
        let mut fixture = Fixture::start(if expire { 8_500 } else { 30_000 }).await;
        fixture.refresh().unwrap();
        if expire {
            while fixture
                .authority
                .invitation
                .session_remaining()
                .is_ok_and(|left| left > Duration::from_millis(500))
            {
                fixture.update();
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            fixture.update();
        } else {
            assert!(
                fixture
                    .host
                    .revoke(fixture.authority.invitation.invitation_id())
            );
        }
        bounded(fixture.guest.connection().closed()).await;
        assert!(fixture.refresh().is_err());
        if expire {
            assert!(
                fixture
                    .authority
                    .invitation
                    .check_session_current()
                    .is_err()
            );
        }
        fixture.no_application_traffic().await;
        fixture.stop().await;
    }
}
