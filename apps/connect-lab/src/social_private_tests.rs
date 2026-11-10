// Private recipient grants keep existing protocol and lifecycle authority.
use super::*;

#[test]
fn social_private_typescript_vectors_match_native_bytes_and_signatures() {
    let vectors: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/social-private-vectors.json"
    ))
    .unwrap();
    let rows = vectors["vectors"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        let policy: SocialHostPolicy =
            serde_json::from_value(row["policy"]["body"].clone()).unwrap();
        let grant: GrantBody = serde_json::from_value(row["grant"]["body"].clone()).unwrap();
        let context: JoinContextBody =
            serde_json::from_value(row["context"]["body"].clone()).unwrap();
        policy.validate().unwrap();
        grant.validate().unwrap();
        context.validate().unwrap();
        for (name, bytes) in [
            ("policy", policy.bytes()),
            ("grant", grant.bytes()),
            ("context", context.bytes()),
        ] {
            assert_eq!(hex(&bytes), row[name]["preimageHex"].as_str().unwrap());
            verify_signature(
                row[name]["signerPublicKey"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap(),
                &bytes,
                row[name]["signature"].as_str().unwrap(),
            )
            .unwrap();
        }
        assert_eq!(context.authorization_id, grant.authorization_id);
        assert_eq!(context.authorization_revision, grant.authorization_revision);
        assert_eq!(grant.policy_digest, policy.digest());
        assert_eq!(context.policy_digest, policy.digest());
    }
}

fn fixture(voice: bool, source: SocketAddr) -> Fixture {
    let mut f = Fixture::new();
    let review = if voice {
        SocialHostPolicy::review_online_voice(
            &f.hc,
            "friends",
            source,
            "open",
            f.grant.session_expires_at_unix_ms,
            "svc",
            &"77".repeat(32),
            false,
        )
        .unwrap()
        .with_runtime_profile("fabric-loader-0.16.14-1.21.1")
        .unwrap()
        .with_native_voice_transport()
        .unwrap()
    } else {
        SocialHostPolicy::review_online(
            &f.hc,
            "friends",
            source,
            "open",
            f.grant.session_expires_at_unix_ms,
        )
        .unwrap()
    }
    .with_private_recipients()
    .unwrap();
    f.policy = review.approve(&f.host, &review.digest(), f.cap).unwrap();
    f.grant.schema_version = if voice { 7 } else { 6 };
    f.grant.policy_digest = review.digest();
    f.grant.voice_profile = review.voice_profile.clone();
    f.grant.voice_source_digest = review.voice_source_digest.clone();
    f.grant.host_player = review.host_player;
    f.grant.runtime_profile = review.runtime_profile.clone();
    f.grant.voice_transport_profile = review.voice_transport_profile.clone();
    f.grant.authorization_id = Some("ab".repeat(32));
    f.grant.authorization_revision = Some(1);
    f.expected.policy_digest = review.digest();
    f
}
fn context(f: &Fixture) -> VerifiedSocialJoinContext {
    let mut body = f.context(&f.expected).body;
    body.schema_version = 2;
    body.authorization_id = f.grant.authorization_id.clone();
    body.authorization_revision = f.grant.authorization_revision;
    VerifiedSocialJoinContext::verify(
        &f.encoded_context(&body),
        f.authority.public(),
        f.gc.account_id(),
        f.gc.session_id(),
        f.cap,
    )
    .unwrap()
}
fn guest(f: &Fixture, context: &VerifiedSocialJoinContext) -> Result<VerifiedSocialGrant> {
    VerifiedSocialGrant::for_guest_online(
        &f.encoded(&f.grant),
        f.authority.public(),
        &f.gc,
        context,
        f.cap,
    )
}

#[test]
fn social_private_schemas_bind_recipient_revision_and_preserve_old_bytes() {
    for voice in [false, true] {
        let f = fixture(voice, "127.0.0.1:43001".parse().unwrap());
        let context = context(&f);
        let grant = guest(&f, &context).unwrap();
        assert_eq!(
            context.private_authorization(),
            Some(("ab".repeat(32).as_str(), 1))
        );
        f.verify_host(&f.encoded(&f.grant)).unwrap();
        assert_eq!(grant.body.schema_version, if voice { 7 } else { 6 });
        assert_eq!(
            grant.voice_transport_profile(),
            voice.then_some("nativeVoice8V1")
        );
        let mut public = f.grant.clone();
        public.schema_version = if voice { 5 } else { 2 };
        public.authorization_id = None;
        public.authorization_revision = None;
        public.validate().unwrap();
        let mut private_base = f.grant.clone();
        private_base.schema_version = public.schema_version;
        // Old schemas have exactly their old field list, even before validation
        // rejects forbidden extra fields. Private lists append the two refs.
        assert_eq!(private_base.bytes(), public.bytes());
        assert!(private_base.validate().is_err());
        assert!(
            f.grant
                .bytes()
                .ends_with(&preimage("", &["ab".repeat(32), "1".into()]))
        );
        assert!(
            context
                .body
                .bytes()
                .ends_with(&preimage("", &["ab".repeat(32), "1".into()]))
        );
        let mut old_context = context.body.clone();
        old_context.schema_version = 1;
        assert!(old_context.validate().is_err());
        assert!(guest(&f, &f.context(&f.expected)).is_err());
        let mut changed = context.clone();
        changed.body.authorization_revision = Some(2);
        assert!(guest(&f, &changed).is_err());
        changed.body.authorization_revision = Some(1);
        changed.body.authorization_id = Some("cd".repeat(32));
        assert!(guest(&f, &changed).is_err());
        let mut wrong_policy = f.policy.clone();
        wrong_policy.body.visibility = "friends".into();
        assert!(wrong_policy.body.validate().is_err());
        wrong_policy.body.visibility = "private".into();
        wrong_policy.body.join_policy = "approval".into();
        assert!(wrong_policy.body.validate().is_err());
    }
}

#[test]
fn social_private_wire_ref_is_required_closed_signed_and_bounded() {
    for voice in [false, true] {
        let f = fixture(voice, "127.0.0.1:43001".parse().unwrap());
        let context = context(&f);
        for (name, replacement) in [
            ("authorizationId", json!(null)),
            ("authorizationId", json!("AB".repeat(32))),
            ("authorizationId", json!("ab")),
            ("authorizationRevision", json!(null)),
            ("authorizationRevision", json!(0)),
            ("authorizationRevision", json!(-1)),
            ("authorizationRevision", json!(4294967296u64)),
            ("authorizationRevision", json!(1.5)),
            ("authorizationRevision", json!("1")),
        ] {
            let mut value = serde_json::to_value(&f.grant).unwrap();
            value[name] = replacement.clone();
            assert!(
                !serde_json::from_value::<GrantBody>(value)
                    .is_ok_and(|value| value.validate().is_ok())
            );
            let mut value = serde_json::to_value(&context.body).unwrap();
            value[name] = replacement;
            assert!(
                !serde_json::from_value::<JoinContextBody>(value)
                    .is_ok_and(|value| value.validate().is_ok())
            );
        }
        for name in ["authorizationId", "authorizationRevision"] {
            let mut value = serde_json::to_value(&f.grant).unwrap();
            value.as_object_mut().unwrap().remove(name);
            assert!(
                serde_json::from_value::<GrantBody>(value)
                    .unwrap()
                    .validate()
                    .is_err()
            );
            let mut wire: Value = serde_json::from_str(&f.encoded(&f.grant)).unwrap();
            wire["body"][name] = if name == "authorizationId" {
                json!("cd".repeat(32))
            } else {
                json!(2)
            };
            assert!(f.verify_host(&wire.to_string()).is_err());
        }
        let mut max = f.grant.clone();
        max.authorization_revision = Some(u32::MAX);
        max.validate().unwrap();
        let mut legacy = f.policy.body.clone();
        legacy.schema_version = if voice { 5 } else { 2 };
        assert!(legacy.validate().is_err());
    }
}

#[test]
fn social_private_reinvite_is_new_authority_and_never_reanchors_old_continuity() {
    let mut f = fixture(false, "127.0.0.1:43001".parse().unwrap());
    let old_context = context(&f);
    let old = guest(&f, &old_context).unwrap();
    let original_deadline = old.session_deadline();
    let lease = f.lease(1, unix_millis() + 3_000);
    let mut old_continuity = SocialContinuity::start(&lease, &old).unwrap();
    let old_next_lease = f.lease(2, unix_millis() + 3_000);
    f.grant.authorization_revision = Some(3); // revoke=2, reinvite=3; ID/expiry unchanged.
    f.grant.grant_id = "ef".repeat(32);
    assert!(guest(&f, &old_context).is_err());
    let new = guest(&f, &context(&f)).unwrap();
    assert_eq!(new.body.authorization_id, old.body.authorization_id);
    assert_eq!(
        new.session_expires_at_unix_ms(),
        old.session_expires_at_unix_ms()
    );
    assert_eq!(old.session_deadline(), original_deadline);
    assert!(
        old_continuity
            .update(&f.lease(2, unix_millis() + 3_000))
            .is_err()
    );
    assert!(old_continuity.update(&old_next_lease).is_err());
    let new_continuity = SocialContinuity::start(&f.lease(1, unix_millis() + 3_000), &new).unwrap();
    assert!(new_continuity.check().is_ok());
    let local = VerifiedSocialGrant::for_guest_private_loopback(
        &f.encoded(&f.grant),
        f.authority.public(),
        &f.gc,
        &context(&f),
        f.cap,
    )
    .unwrap();
    assert!(local.session_deadline() <= f.cap);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn social_private_tcp_two_real_agents_obey_revocation_and_never_contact_unapproved_services()
{
    private_tcp_agents(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn social_private_online_tcp_two_real_agents_keep_signed_network_ticket_and_revocation() {
    private_tcp_agents(true).await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrivateTicketBody {
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

fn private_network_ticket(
    f: &Fixture,
    peer: EndpointId,
    origin: &str,
) -> crate::OnlineNetworkTicket {
    let body = PrivateTicketBody {
        schema_version: 1,
        session_id: f.grant.session_id.clone(),
        issuer_instance: f.grant.issuer_instance.clone(),
        peer_id: peer.to_string(),
        host_peer_id: f.host.public().to_string(),
        account_id: f.grant.host_account_id.clone(),
        generation: f.grant.host_generation,
        issued_at_unix_ms: unix_millis(),
        expires_at_unix_ms: f.grant.session_expires_at_unix_ms,
        direct_addresses: vec![],
    };
    let mut bytes = b"aurora/connect/relay-ticket/v1\0".to_vec();
    bytes.extend(serde_json::to_vec(&body).unwrap());
    let signature = hex(&f.authority.sign(&bytes).to_bytes());
    crate::OnlineNetworkTicket::for_local_test(
        &json!({"body":body,"signature":signature}).to_string(),
        f.authority.public(),
        &f.grant.issuer_instance,
        &f.grant.host_account_id,
        &f.grant.session_id,
        peer,
        f.host.public(),
        origin,
    )
    .unwrap()
}

async fn private_tcp_agents(online: bool) {
    use crate::{
        Capability, Guest, Host, HostConfig, Limits, Protocol, Service, SessionDeadlines,
        SessionInvitation, UdpReturnPolicy,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let source = listener.local_addr().unwrap();
    let f = fixture(false, source);
    let context = context(&f);
    let verify = if online {
        VerifiedSocialGrant::for_guest_online
    } else {
        VerifiedSocialGrant::for_guest_private_loopback
    };
    let grant = verify(
        &f.encoded(&f.grant),
        f.authority.public(),
        &f.gc,
        &context,
        f.cap,
    )
    .unwrap();
    let host_grant = VerifiedSocialGrant::for_host(
        &f.encoded(&f.grant),
        f.authority.public(),
        &f.hc,
        &f.policy,
        source,
        f.cap,
    )
    .unwrap();
    let continuity = SocialContinuity::start(&f.lease(1, unix_millis() + 3_000), &grant).unwrap();
    let service = Service {
        service_id: "game".into(),
        protocol: Protocol::Tcp,
        endpoint: source,
        revision: 1,
        max_datagram_bytes: 1024,
        required_capability: Capability::ReliableStream,
        udp_return_policy: UdpReturnPolicy::ReplyBudget,
    };
    let reserved = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = reserved.local_addr().unwrap();
    drop(reserved);
    let deadlines = SessionDeadlines {
        admission_expires_at_unix_ms: host_grant.admission_expires_at_unix_ms(),
        session_expires_at_unix_ms: host_grant.session_expires_at_unix_ms(),
    };
    let invitation = if online {
        SessionInvitation::issue_online(
            &f.host,
            f.guest.public(),
            host_grant.session_id(),
            std::slice::from_ref(&service),
            deadlines,
        )
    } else {
        SessionInvitation::issue_loopback(
            &f.host,
            f.guest.public(),
            host_grant.session_id(),
            bind,
            std::slice::from_ref(&service),
            deadlines,
        )
    }
    .unwrap();
    let relay = if online {
        Some(
            crate::LocalRelayFixture::start(
                [f.host.public(), f.guest.public()],
                crate::LocalRelayProfile::Normal,
            )
            .await
            .unwrap(),
        )
    } else {
        None
    };
    let origin = relay
        .as_ref()
        .map(|relay| format!("http://{}", relay.bound_addr()));
    let host_ticket = origin
        .as_ref()
        .map(|origin| private_network_ticket(&f, f.host.public(), origin));
    let guest_ticket = origin
        .as_ref()
        .map(|origin| private_network_ticket(&f, f.guest.public(), origin));
    let config = HostConfig {
        secret_key: f.host.clone(),
        bind_addr: invitation.direct_addr(),
        session_id: host_grant.session_id().into(),
        services: vec![service],
        leases: vec![invitation.lease()],
        limits: Limits::default(),
    };
    let host = if let Some(ticket) = &host_ticket {
        Host::start_invited_online(config, &invitation, ticket).await
    } else {
        Host::start_invited_session(config, &invitation).await
    }
    .unwrap();
    let sockets = host.bound_sockets();
    continuity.check().unwrap();
    let guest = if let Some(ticket) = &guest_ticket {
        Guest::connect_invited_online(f.guest.clone(), &invitation, Limits::default(), ticket).await
    } else {
        Guest::connect_invited_session(f.guest.clone(), &invitation, Limits::default()).await
    }
    .unwrap();
    assert_eq!(host.metrics().destination_tcp_connects, 0);
    let bridge = guest
        .tcp_bridge("game", 1, "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut client = TcpStream::connect(bridge.local_addr()).await.unwrap();
        client.write_all(&[17; 1024]).await.unwrap();
        let (mut server, _) = listener.accept().await.unwrap();
        let mut data = [0; 1024];
        server.read_exact(&mut data).await.unwrap();
        assert_eq!(data, [17; 1024]);
        server.write_all(&data).await.unwrap();
        client.read_exact(&mut data).await.unwrap();
        assert_eq!(data, [17; 1024]);
    })
    .await
    .unwrap();
    assert!(host.revoke(invitation.invitation_id()));
    assert!(!host.invitation_active(invitation.invitation_id()));
    let replay = if let Some(ticket) = &guest_ticket {
        Guest::connect_invited_online(f.guest.clone(), &invitation, Limits::default(), ticket).await
    } else {
        Guest::connect_invited_session(f.guest.clone(), &invitation, Limits::default()).await
    };
    assert!(replay.is_err());
    assert_eq!(host.metrics().destination_tcp_connects, 1);
    bridge.shutdown().await;
    guest.shutdown().await;
    host.shutdown().await;
    drop((bridge, guest, host));
    if let Some(relay) = relay {
        relay.shutdown().await.unwrap();
    }
    for socket in sockets {
        assert!(std::net::UdpSocket::bind(socket).is_ok());
    }
}
