//! A truthful connection observation is distinct from cumulative admission.
use super::*;

#[tokio::test]
async fn active_invitation_observation_checks_known_close_and_original_clock() {
    let local: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let reservation = std::net::UdpSocket::bind(local).unwrap();
    let bind = reservation.local_addr().unwrap();
    let host_key = generate_key();
    let guest_key = generate_key();
    let service = Service {
        service_id: "game".into(),
        protocol: Protocol::Tcp,
        endpoint: "127.0.0.1:43201".parse().unwrap(),
        revision: 1,
        max_datagram_bytes: 1400,
        required_capability: Capability::ReliableStream,
        udp_return_policy: UdpReturnPolicy::ReplyBudget,
    };
    let invitation = SessionInvitation::issue_loopback_for_lifetimes(
        &host_key,
        guest_key.public(),
        "active-observation",
        bind,
        std::slice::from_ref(&service),
        Duration::from_secs(30),
        Duration::from_secs(60),
    )
    .unwrap();
    let wall = Arc::new(AtomicU64::new(unix_millis()));
    let controlled = invitation.clone().with_test_wall_clock(wall.clone());
    drop(reservation);
    let host = Host::start_invited_session(
        HostConfig {
            secret_key: host_key,
            bind_addr: bind,
            session_id: invitation.session_id().into(),
            services: vec![service],
            leases: vec![invitation.lease()],
            limits: Limits::default(),
        },
        &controlled,
    )
    .await
    .unwrap();
    let lease_id = invitation.invitation_id();
    assert!(!host.invitation_active(lease_id));
    assert!(!host.invitation_active("missing"));
    assert!(!host.invitation_consumed(lease_id));
    let guest = Guest::connect_invited_session(guest_key, &invitation, Limits::default())
        .await
        .unwrap();
    assert!(host.invitation_consumed(lease_id));
    assert!(host.invitation_active(lease_id));
    {
        // Hold the real admission mutex so PeerGuard cannot remove its entry.
        // This proves the predicate observes close_reason, not only membership.
        let active = host.state.active_leases.lock().unwrap();
        assert!(active_invitation(&host.state, &active, lease_id));
        let original_wall = wall.load(Ordering::SeqCst);
        wall.store(invitation.session_expires_at_unix_ms(), Ordering::SeqCst);
        assert!(!active_invitation(&host.state, &active, lease_id));
        wall.store(original_wall, Ordering::SeqCst);
        let connection = active.get(lease_id).unwrap();
        assert!(connection.close_reason().is_none());
        connection.close(0u32.into(), b"observation test");
        assert!(connection.close_reason().is_some());
        assert!(active.contains_key(lease_id));
        assert!(!active_invitation(&host.state, &active, lease_id));
    }
    assert!(!host.invitation_active(lease_id));
    assert!(host.invitation_consumed(lease_id));
    guest.shutdown().await;
    host.shutdown().await;
    assert!(!host.invitation_active(lease_id));
    assert!(host.invitation_consumed(lease_id));
    drop((guest, host));
    drop(std::net::UdpSocket::bind(bind).unwrap());
}
