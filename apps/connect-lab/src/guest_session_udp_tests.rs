//! Adversarial schema-3 clock checks with real peers and a real fragmented
//! return already queued. Only the retained Guest authority sees the fake clock.
use super::*;
use std::sync::atomic::AtomicU64;

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

struct OwnedEcho {
    stop: CancellationToken,
    task: Option<JoinHandle<()>>,
}
impl Drop for OwnedEcho {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

async fn queued_return_rejects_original_clock_before_watcher(rollback: bool) {
    let tcp = TcpListener::bind(local()).await.unwrap();
    let udp = UdpSocket::bind(local()).await.unwrap();
    let tcp_address = tcp.local_addr().unwrap();
    let udp_address = udp.local_addr().unwrap();
    let reservation = std::net::UdpSocket::bind(local()).unwrap();
    let host_key = generate_key();
    let guest_key = generate_key();
    let services = vec![
        Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: tcp_address,
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        },
        Service {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            endpoint: udp_address,
            revision: 1,
            max_datagram_bytes: 4096,
            required_capability: Capability::FragmentedDatagram,
            udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                bytes_per_second: 16384,
                datagrams_per_second: 20,
            },
        },
    ];
    let invitation = SessionInvitation::issue_fragmented_loopback_for_lifetimes(
        &host_key,
        guest_key.public(),
        "guest-session-clock",
        reservation.local_addr().unwrap(),
        &services,
        Duration::from_secs(60),
        Duration::from_secs(120),
    )
    .unwrap();
    let issued = invitation.preview()["issuedAtUnixMs"].as_u64().unwrap();
    let wall = Arc::new(AtomicU64::new(issued));
    let guest_authority = invitation.clone().with_test_wall_clock(wall.clone());
    drop(reservation);
    let host = Host::start_invited_fragmented_session(
        HostConfig {
            secret_key: host_key,
            bind_addr: invitation.direct_addr(),
            session_id: invitation.session_id().into(),
            services,
            leases: vec![invitation.lease()],
            limits: Limits::default(),
        },
        &invitation,
    )
    .await
    .unwrap();
    let guest =
        Guest::connect_invited_fragmented_session(guest_key, &guest_authority, Limits::default())
            .await
            .unwrap();
    let stop = CancellationToken::new();
    let token = stop.clone();
    let mut echo = OwnedEcho {
        stop,
        task: Some(tokio::spawn(async move {
            let mut buffer = [0u8; 4097];
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    received = udp.recv_from(&mut buffer) => {
                        let (size, source) = received.unwrap();
                        assert_eq!(size, 2048);
                        udp.send_to(&buffer[..size], source).await.unwrap();
                    }
                }
            }
        })),
    };
    let association = guest.open_udp_fragmented("aux", 1).await.unwrap();
    let bridge = guest
        .udp_bridge_fragmented("aux", 1, local())
        .await
        .unwrap();
    let bridge_address = bridge.local_addr();
    // Keeping this port bound makes an attempted bind observable as Io rather
    // than Expired. The authority must reject before touching that owned port.
    let occupied = UdpSocket::bind(local()).await.unwrap();
    let occupied_address = occupied.local_addr().unwrap();
    association.send(&[0x51; 2048]).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if !association
                .association
                .inner
                .state
                .inbox
                .lock()
                .unwrap()
                .is_empty()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(guest.inner.fragment_memory.used_bytes(), 2048);
    assert_eq!(host.metrics().udp_forwarded, 1);
    assert_eq!(host.metrics().udp_returned, 1);
    let state = &association.association.inner.state;
    {
        let queue = state.inbox.lock().unwrap();
        assert_eq!(queue.len(), 1);
        assert!(matches!(&queue[0].1, ReceivedPayload::V2(_)));
        assert_eq!(queue[0].1.as_ref(), &[0x51; 2048]);
    }
    assert!(association.max_payload().is_some());
    assert!(bridge.max_payload().is_some());
    let opens_before = *guest.inner.open_lock.lock().await;
    assert!(!guest.inner.cancel.is_cancelled());
    assert!(!state.cancel.is_cancelled());
    wall.store(
        if rollback {
            issued - 1
        } else {
            invitation.session_expires_at_unix_ms()
        },
        Ordering::SeqCst,
    );
    // Current-thread runtime and immediately-ready operations below prevent the
    // watcher from running. Cancellation therefore cannot explain any refusal.
    assert!(!guest.inner.cancel.is_cancelled());
    assert!(!state.cancel.is_cancelled());
    assert!(association.is_closed());
    assert!(association.max_payload().is_none());
    assert!(bridge.is_closed());
    assert!(bridge.max_payload().is_none());
    assert!(matches!(
        association.send(&[0x72; 2048]).await,
        Err(Error::Expired)
    ));
    assert!(matches!(association.recv().await, Err(Error::Expired)));
    assert!(matches!(
        guest.open_udp_fragmented("aux", 1).await,
        Err(Error::Expired)
    ));
    assert!(matches!(
        guest
            .udp_bridge_fragmented("aux", 1, occupied_address)
            .await,
        Err(Error::Expired)
    ));
    assert_eq!(*guest.inner.open_lock.lock().await, opens_before);
    assert_eq!(state.inbox.lock().unwrap().len(), 1);
    assert_eq!(guest.inner.fragment_memory.used_bytes(), 2048);
    assert!(
        !guest.inner.cancel.is_cancelled(),
        "watcher_must_not_explain_refusal"
    );
    assert!(
        !state.cancel.is_cancelled(),
        "association_must_still_have_live_token"
    );

    let mut transport_addresses = host.bound_sockets();
    transport_addresses.extend(guest.bound_sockets());
    association.shutdown().await;
    bridge.shutdown().await;
    guest.shutdown().await;
    host.shutdown().await;
    echo.stop.cancel();
    echo.task.take().unwrap().await.unwrap();
    assert_eq!(guest.inner.fragment_memory.used_bytes(), 0);
    drop(association);
    drop(bridge);
    drop(guest);
    drop(host);
    drop(tcp);
    drop(occupied);
    for address in
        transport_addresses
            .into_iter()
            .chain([udp_address, bridge_address, occupied_address])
    {
        drop(std::net::UdpSocket::bind(address).unwrap());
    }
    drop(std::net::TcpListener::bind(tcp_address).unwrap());
}

#[tokio::test(flavor = "current_thread")]
async fn queued_real_fragmented_return_and_new_io_reject_session_expiry_before_watcher() {
    queued_return_rejects_original_clock_before_watcher(false).await;
}

#[tokio::test(flavor = "current_thread")]
async fn queued_real_fragmented_return_and_new_io_reject_clock_rollback_before_watcher() {
    queued_return_rejects_original_clock_before_watcher(true).await;
}
