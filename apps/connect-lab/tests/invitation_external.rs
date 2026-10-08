//! Pure external-address policy tests. The sole network test is ignored and
//! requires a caller-supplied own IPv6 address, bound locally before any dial.
use aurora_connect_lab::*;
use std::{
    future::Future,
    net::{Ipv6Addr, SocketAddr, SocketAddrV6},
    task::{Context, Poll, Waker},
    time::Duration,
};

fn direct() -> SocketAddr {
    "[2003:abcd:1::1234]:42000".parse().unwrap()
}
fn service(id: &str, protocol: Protocol, endpoint: SocketAddr) -> Service {
    Service {
        service_id: id.into(),
        protocol,
        endpoint,
        revision: 1,
        required_capability: if protocol == Protocol::Tcp {
            Capability::ReliableStream
        } else {
            Capability::Datagram
        },
        max_datagram_bytes: 1024,
        udp_return_policy: UdpReturnPolicy::ReplyBudget,
    }
}
fn services() -> Vec<Service> {
    vec![
        service("game", Protocol::Tcp, "127.0.0.1:43001".parse().unwrap()),
        service("aux", Protocol::Udp, "127.0.0.1:43002".parse().unwrap()),
    ]
}
fn fixture() -> (SecretKey, SecretKey, Invitation) {
    let host = generate_key();
    let guest = generate_key();
    let invitation = Invitation::issue_external_ipv6(
        &host,
        guest.public(),
        "external-test",
        direct(),
        &services(),
        unix_millis() + 60_000,
    )
    .unwrap();
    (host, guest, invitation)
}
fn config(host: &SecretKey, invitation: &Invitation) -> HostConfig {
    HostConfig {
        secret_key: host.clone(),
        bind_addr: invitation.direct_addr(),
        session_id: invitation.session_id().into(),
        services: services(),
        leases: vec![invitation.lease()],
        limits: Limits::default(),
    }
}
fn ready_error<F, T>(future: F, expected: Error)
where
    F: Future<Output = Result<T>>,
{
    // No Tokio runtime: these policy failures must be immediate, before sockets.
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        matches!(future.as_mut().poll(&mut context), Poll::Ready(Err(error)) if error == expected),
        "policy must reject before endpoint construction"
    );
}

#[test]
fn external_filter_prefix_boundaries_and_embedded_mechanisms() {
    let cases = [
        ("[1fff:ffff:ffff:ffff:ffff:ffff:ffff:ffff]:1", false),
        ("[2000::1]:1", true),
        ("[3fff:ffff:ffff:ffff:ffff:ffff:ffff:ffff]:65535", true),
        ("[4000::1]:1", false),
        ("[2001::1]:1", false),
        ("[2001:1ff:ffff:ffff:ffff:ffff:ffff:ffff]:1", false),
        ("[2001:200::1]:1", true),
        ("[2001:db7:ffff:ffff:ffff:ffff:ffff:ffff]:1", true),
        ("[2001:db8::1]:1", false),
        ("[2001:db9::1]:1", true),
        ("[2002::1]:1", false),
        ("[2002:ffff:ffff:ffff:ffff:ffff:ffff:ffff]:1", false),
        ("[2003::1]:1", true),
        ("[2620:4f:7fff:ffff:ffff:ffff:ffff:ffff]:1", true),
        ("[2620:4f:8000::1]:1", false),
        ("[2620:4f:8000:ffff:ffff:ffff:ffff:ffff]:1", false),
        ("[2620:4f:8001::1]:1", true),
        ("[3fff::1]:1", false),
        ("[3fff:fff:ffff:ffff:ffff:ffff:ffff:ffff]:1", false),
        ("[3fff:1000::1]:1", true),
        ("[2003:1:2:3:0:5efe:c000:201]:1", false),
        ("[2003:1:2:3:200:5efe:c000:201]:1", false),
        ("[2003:1:2:3:201:5efe:c000:201]:1", true),
        ("[2003:1:2:3:0:5efd:c000:201]:1", true),
        ("[::]:1", false),
        ("[::1]:1", false),
        ("[fe80::1]:1", false),
        ("[fc00::1]:1", false),
        ("[ff02::1]:1", false),
        ("[::ffff:192.0.2.1]:1", false),
        ("127.0.0.1:1", false),
        ("[2003::1]:0", false),
        ("[2003::1%1]:1", false),
    ];
    for (index, (text, allowed)) in cases.into_iter().enumerate() {
        assert!(
            validate_external_invitation_address(text.parse().unwrap()).is_ok() == allowed,
            "classification case {index}"
        );
    }
    let flow = SocketAddr::V6(SocketAddrV6::new("2003::1".parse().unwrap(), 1, 1, 0));
    assert!(matches!(
        validate_external_invitation_address(flow),
        Err(Error::Configuration)
    ));
}

#[test]
fn external_invitation_keeps_envelope_grants_signature_and_expiry() {
    let (host, guest, invitation) = fixture();
    let verified =
        Invitation::verify_external_ipv6(&invitation.encode(), host.public(), guest.public())
            .unwrap();
    assert!(verified.digest() == invitation.digest());
    let body: serde_json::Value = serde_json::from_str(&invitation.encode()).unwrap();
    assert_eq!(body["body"]["schemaVersion"], 1);
    assert_eq!(body["body"]["generation"], 1);
    assert_eq!(body["body"].as_object().unwrap().len(), 10);
    assert!(matches!(
        Invitation::verify(&invitation.encode(), host.public(), guest.public()),
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Invitation::issue(
            &host,
            guest.public(),
            "old",
            direct(),
            &services(),
            unix_millis() + 60_000
        ),
        Err(Error::Configuration)
    ));
    assert!(matches!(
        Invitation::issue_external_ipv6(
            &host,
            guest.public(),
            "new",
            "127.0.0.1:1".parse().unwrap(),
            &services(),
            unix_millis() + 60_000
        ),
        Err(Error::Configuration)
    ));
    for deadline in [unix_millis(), unix_millis() + 601_000] {
        assert!(matches!(
            Invitation::issue_external_ipv6(
                &host,
                guest.public(),
                "new",
                direct(),
                &services(),
                deadline
            ),
            Err(Error::Expired)
        ));
    }
    let mut invalid = services();
    invalid[0].endpoint = direct();
    assert!(matches!(
        Invitation::issue_external_ipv6(
            &host,
            guest.public(),
            "new",
            direct(),
            &invalid,
            unix_millis() + 60_000
        ),
        Err(Error::Configuration)
    ));
}

#[test]
fn canonical_external_address_spellings_are_required() {
    let (host, guest, invitation) = fixture();
    let encoded = invitation.encode();
    for alternative in [
        "[2003:ABCD:1::1234]:42000",
        "[2003:abcd:0001:0:0:0:0:1234]:42000",
        "[2003:abcd:1::1234%0]:42000",
        "[2003:abcd:1::1234]:042000",
    ] {
        let changed = encoded.replace("[2003:abcd:1::1234]:42000", alternative);
        assert!(changed != encoded);
        assert!(
            matches!(
                Invitation::verify_external_ipv6(&changed, host.public(), guest.public()),
                Err(Error::Protocol)
            ),
            "noncanonical representation must not verify"
        );
    }
}

#[test]
fn external_mode_cannot_bypass_signed_body_or_expected_participants() {
    let (host, guest, invitation) = fixture();
    for (before, after) in [
        ("[2003:abcd:1::1234]:42000", "[2003:abcd:1::1235]:42000"),
        ("external-test", "different-test"),
        ("\"maxDatagramBytes\":1024", "\"maxDatagramBytes\":1023"),
        ("\"revision\":1", "\"revision\":2"),
        ("\"generation\":1", "\"generation\":2"),
    ] {
        let changed = invitation.encode().replace(before, after);
        assert!(
            Invitation::verify_external_ipv6(&changed, host.public(), guest.public()).is_err(),
            "tampered signed field must fail"
        );
    }
    assert!(matches!(
        Invitation::verify_external_ipv6(
            &invitation.encode(),
            generate_key().public(),
            guest.public()
        ),
        Err(Error::Unauthorized)
    ));
    assert!(matches!(
        Invitation::verify_external_ipv6(
            &invitation.encode(),
            host.public(),
            generate_key().public()
        ),
        Err(Error::Unauthorized)
    ));
}

#[test]
fn default_host_and_guest_reject_external_verified_objects_before_endpoints() {
    let (host, guest, invitation) = fixture();
    let verified =
        Invitation::verify_external_ipv6(&invitation.encode(), host.public(), guest.public())
            .unwrap();
    ready_error(
        Host::start_invited(config(&host, &verified), &verified),
        Error::Configuration,
    );
    ready_error(
        Guest::connect_invited(guest.clone(), &verified, Limits::default()),
        Error::Configuration,
    );
    ready_error(
        Guest::connect_invited_external_ipv6(generate_key(), &verified, Limits::default()),
        Error::Unauthorized,
    );
    let mut mismatch = config(&host, &verified);
    mismatch.session_id = "different".into();
    ready_error(
        Host::start_invited_external_ipv6(mismatch, &verified),
        Error::Configuration,
    );
    let local = Invitation::issue(
        &host,
        guest.public(),
        "local",
        "[::1]:43000".parse().unwrap(),
        &services(),
        unix_millis() + 60_000,
    )
    .unwrap();
    ready_error(
        Host::start_invited_external_ipv6(config(&host, &local), &local),
        Error::Configuration,
    );
    ready_error(
        Guest::connect_invited_external_ipv6(guest, &local, Limits::default()),
        Error::Configuration,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicitly supplied own IPv6; bind locally before dial; not an internet test"]
async fn own_global_ipv6_invitation_roundtrip_revocation_expiration_cleanup() {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream, UdpSocket},
        time::timeout,
    };
    use tokio_util::sync::CancellationToken;
    let text = std::env::var("AURORA_CONNECT_OWN_EXTERNAL_IPV6_TEST_ADDR")
        .expect("explicit own IPv6 required");
    let ip: Ipv6Addr = text.parse().expect("literal own IPv6 required");
    assert!(
        ip.to_string() == text,
        "own IPv6 spelling must be canonical"
    );
    validate_external_invitation_address(SocketAddr::V6(SocketAddrV6::new(ip, 1, 0, 0))).unwrap();
    // Bind proves local ownership before generating an invitation or dialing.
    let reservation = std::net::UdpSocket::bind(SocketAddr::V6(SocketAddrV6::new(ip, 0, 0, 0)))
        .expect("own IPv6 must bind locally");
    let address = reservation.local_addr().unwrap();
    let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let tcp_address = tcp.local_addr().unwrap();
    let udp_address = udp.local_addr().unwrap();
    let registry = vec![
        service("game", Protocol::Tcp, tcp_address),
        service("aux", Protocol::Udp, udp_address),
    ];
    let cancel = CancellationToken::new();
    let tcp_cancel = cancel.clone();
    let udp_cancel = cancel.clone();
    let tcp_task = tokio::spawn(async move {
        let mut tasks = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _=tcp_cancel.cancelled()=>break,
                accepted=tcp.accept()=> { if let Ok((mut socket,_))=accepted { tasks.spawn(async move { let (mut read,mut write)=socket.split(); let _=tokio::io::copy(&mut read,&mut write).await; let _=write.shutdown().await; }); } else { break; } },
                _=tasks.join_next(),if !tasks.is_empty()=>{}
            }
        }
        tasks.shutdown().await;
    });
    let udp_task = tokio::spawn(async move {
        let mut bytes = [0; 1024];
        loop {
            tokio::select! {
                _=udp_cancel.cancelled()=>break,
                packet=udp.recv_from(&mut bytes)=>{ if let Ok((count,source))=packet { let _=udp.send_to(&bytes[..count],source).await; } else {break;} }
            }
        }
    });
    let host_key = generate_key();
    let guest_key = generate_key();
    let invitation = Invitation::issue_external_ipv6(
        &host_key,
        guest_key.public(),
        "own-global",
        address,
        &registry,
        unix_millis() + 30_000,
    )
    .unwrap();
    drop(reservation);
    let host = Host::start_invited_external_ipv6(
        HostConfig {
            secret_key: host_key,
            bind_addr: address,
            session_id: invitation.session_id().into(),
            services: registry.clone(),
            leases: vec![invitation.lease()],
            limits: Limits::default(),
        },
        &invitation,
    )
    .await
    .unwrap();
    let guest =
        Guest::connect_invited_external_ipv6(guest_key.clone(), &invitation, Limits::default())
            .await
            .unwrap();
    let guest_bound = guest.bound_sockets();
    assert!(guest.bound_sockets().iter().any(|address| matches!(address, SocketAddr::V6(v6) if v6.ip().is_unspecified() && v6.port() != 0)), "explicit external guest binds wildcard IPv6");
    let tcp_bridge = guest
        .tcp_bridge("game", 1, "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let udp_bridge = guest
        .udp_bridge("aux", 1, "127.0.0.1:0".parse().unwrap())
        .await
        .unwrap();
    let bridge_tcp = tcp_bridge.local_addr();
    let bridge_udp = udp_bridge.local_addr();
    let operation = timeout(Duration::from_secs(15), async {
        let socket = TcpStream::connect(bridge_tcp).await?;
        let (mut read, mut write) = socket.into_split();
        let payload = vec![0x45; 1024 * 1024];
        let mut reply = vec![0; payload.len()];
        tokio::try_join!(
            async {
                write.write_all(&payload).await?;
                write.shutdown().await
            },
            async {
                read.read_exact(&mut reply).await?;
                std::io::Result::Ok(())
            }
        )?;
        assert!(reply == payload, "TCP byte-for-byte both directions");
        let left = UdpSocket::bind("127.0.0.1:0").await?;
        let right = UdpSocket::bind("127.0.0.1:0").await?;
        assert!(
            left.local_addr()? != right.local_addr()?,
            "distinct simultaneous UDP sources"
        );
        left.send_to(&[0x45; 512], bridge_udp).await?;
        right.send_to(&[0x46; 512], bridge_udp).await?;
        let mut left_reply = [0; 513];
        let mut right_reply = [0; 513];
        let ((left_count, left_source), (right_count, right_source)) = tokio::try_join!(
            left.recv_from(&mut left_reply),
            right.recv_from(&mut right_reply)
        )?;
        assert!(
            left_count == 512
                && left_reply[..left_count] == [0x45; 512]
                && left_source == bridge_udp,
            "exact UDP reply isolated to left source"
        );
        assert!(
            right_count == 512
                && right_reply[..right_count] == [0x46; 512]
                && right_source == bridge_udp,
            "exact UDP reply isolated to right source"
        );
        std::io::Result::Ok(())
    })
    .await;
    host.revoke(&invitation.lease().lease_id);
    let revoked = timeout(Duration::from_secs(3), guest.connection().closed())
        .await
        .is_ok();
    guest.shutdown().await;
    tcp_bridge.shutdown().await;
    udp_bridge.shutdown().await;
    drop(tcp_bridge);
    drop(udp_bridge);
    host.shutdown().await;
    drop(guest);
    drop(host);
    for socket in &guest_bound {
        let rebound = std::net::UdpSocket::bind(socket);
        assert!(rebound.is_ok(), "owned guest transport socket released");
    }
    let rebound = std::net::UdpSocket::bind(address);
    assert!(rebound.is_ok(), "owned global host socket released");
    drop(rebound);
    let fresh_host = generate_key();
    let expiry_host_id = fresh_host.public();
    let fresh_guest = generate_key();
    let expiring = Invitation::issue_external_ipv6(
        &fresh_host,
        fresh_guest.public(),
        "own-global-expiry",
        address,
        &registry,
        unix_millis() + 2_000,
    )
    .unwrap();
    let host = Host::start_invited_external_ipv6(
        HostConfig {
            secret_key: fresh_host,
            bind_addr: address,
            session_id: expiring.session_id().into(),
            services: registry,
            leases: vec![expiring.lease()],
            limits: Limits::default(),
        },
        &expiring,
    )
    .await
    .unwrap();
    let admitted =
        Guest::connect_invited_external_ipv6(fresh_guest.clone(), &expiring, Limits::default())
            .await;
    let mut expiry_guest_bound = Vec::new();
    let expired = if let Ok(guest) = admitted {
        expiry_guest_bound = guest.bound_sockets();
        let closed = timeout(Duration::from_secs(4), guest.connection().closed())
            .await
            .is_ok();
        let deadline_elapsed = unix_millis() >= expiring.expires_at_unix_ms();
        guest.shutdown().await;
        drop(guest);
        closed && deadline_elapsed
    } else {
        false
    };
    host.shutdown().await;
    drop(host);
    let expired_verification = matches!(
        Invitation::verify_external_ipv6(&expiring.encode(), expiry_host_id, fresh_guest.public()),
        Err(Error::Expired)
    );
    let expired_guest_result = if expired {
        Some(Guest::connect_invited_external_ipv6(fresh_guest, &expiring, Limits::default()).await)
    } else {
        None
    };
    let expired_guest_rejected = matches!(expired_guest_result, Some(Err(Error::Expired)));
    if let Some(Ok(unexpected_guest)) = expired_guest_result {
        unexpected_guest.shutdown().await;
    }
    cancel.cancel();
    let _ = tcp_task.await;
    let _ = udp_task.await;
    let host_rebind = std::net::UdpSocket::bind(address);
    let tcp_rebind = std::net::TcpListener::bind(bridge_tcp);
    let udp_rebind = std::net::UdpSocket::bind(bridge_udp);
    let echo_tcp_rebind = std::net::TcpListener::bind(tcp_address);
    let echo_udp_rebind = std::net::UdpSocket::bind(udp_address);
    for socket in expiry_guest_bound {
        let rebound = std::net::UdpSocket::bind(socket);
        assert!(
            rebound.is_ok(),
            "owned expiry guest transport socket released"
        );
    }
    assert!(
        host_rebind.is_ok()
            && tcp_rebind.is_ok()
            && udp_rebind.is_ok()
            && echo_tcp_rebind.is_ok()
            && echo_udp_rebind.is_ok(),
        "owned host bridge and echo sockets released"
    );
    assert!(matches!(operation, Ok(Ok(()))), "owned roundtrip completed");
    assert!(
        revoked && expired && expired_verification && expired_guest_rejected,
        "revocation and whole-session expiry enforced; independent replay is covered by unchanged local regressions"
    );
}
