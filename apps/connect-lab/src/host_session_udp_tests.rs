//! Isolate the per-fragment retained session authority from watcher cancellation
//! and from the legacy scalar lease deadlines. No process clock is changed.
use super::*;

#[tokio::test(flavor = "current_thread")]
async fn original_schema_three_authority_rejects_civil_jumps_before_watcher() {
    let local: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let tcp = tokio::net::TcpListener::bind(local).await.unwrap();
    let socket = UdpSocket::bind(local).await.unwrap();
    let reservation = std::net::UdpSocket::bind(local).unwrap();
    let host_key = generate_key();
    let guest_key = generate_key();
    let policy = UdpReturnPolicy::AuthorizedDownstream {
        bytes_per_second: 16384,
        datagrams_per_second: 20,
    };
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
        Service {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            endpoint: socket.local_addr().unwrap(),
            revision: 1,
            max_datagram_bytes: 4096,
            required_capability: Capability::FragmentedDatagram,
            udp_return_policy: policy,
        },
    ];
    let invitation = SessionInvitation::issue_fragmented_loopback_for_lifetimes(
        &host_key,
        guest_key.public(),
        "host-fragment-clock",
        reservation.local_addr().unwrap(),
        &services,
        Duration::from_secs(60),
        Duration::from_secs(120),
    )
    .unwrap();
    let issued = invitation.preview()["issuedAtUnixMs"].as_u64().unwrap();
    let wall = Arc::new(AtomicU64::new(issued));
    let invitation = invitation.with_test_wall_clock(wall.clone());
    let limits = Limits::default();
    let association = Association {
        socket: Arc::new(socket),
        service_handle: 2,
        generation: 1,
        max_payload: 4096,
        cancel: CancellationToken::new(),
        activity: Mutex::new(Instant::now()),
        sequence: Mutex::new(SequenceWindow::default()),
        credit: AtomicU64::new(0),
        limiter: Arc::new(Mutex::new(UdpLimiter::new(&limits))),
        downstream_limiter: Arc::new(Mutex::new(UdpLimiter::downstream(policy, &limits))),
        return_policy: policy,
        first_forwarded: AtomicBool::new(false),
        first_forwarded_notify: Notify::new(),
        fragmented: Some(Mutex::new(fragment::Reassembler::new(
            fragment::Scope {
                service_handle: 2,
                association_id: 1,
                generation: 1,
            },
            bridge::peer_budget(guest_key.public()),
        ))),
        lease_cancel: CancellationToken::new(),
        lease_unix_deadline: invitation.session_expires_at_unix_ms(),
        lease_deadline: Instant::now() + Duration::from_secs(120),
        session_authority: Some(invitation.clone()),
    };
    assert!(association.fragment_authorized());
    for invalid_wall in [issued - 1, invitation.session_expires_at_unix_ms()] {
        wall.store(invalid_wall, Ordering::SeqCst);
        // All older guards are still valid, so only the original retained
        // schema-3 authority can cause this per-fragment rejection.
        assert!(!association.cancel.is_cancelled());
        assert!(!association.lease_cancel.is_cancelled());
        assert!(fragment_authority_valid(
            &association.cancel,
            &association.lease_cancel,
            association.lease_unix_deadline,
            association.lease_deadline,
        ));
        assert!(!association.fragment_authorized());
        wall.store(issued, Ordering::SeqCst);
        assert!(association.fragment_authorized());
    }
    // Revocation still independently dominates the original valid clocks.
    association.lease_cancel.cancel();
    assert!(!association.fragment_authorized());
    drop(association);
    drop(reservation);
    drop(tcp);
}
