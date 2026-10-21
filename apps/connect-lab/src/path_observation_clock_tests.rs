//! Current-thread runtime: no watcher can run between the private clock change
//! and the synchronous observation. The operating-system clock is untouched.
use super::*;
use std::sync::atomic::AtomicU64;
use tokio::{net::TcpListener, time::timeout};

#[tokio::test(flavor = "current_thread")]
async fn path_observation_original_session_clock_gate_precedes_its_close_watcher() {
    let local: SocketAddr = "127.0.0.1:0".parse().unwrap();
    for fragmented in [false, true] {
        for host_clock in [false, true] {
            let listener = TcpListener::bind(local).await.unwrap();
            let aux = UdpSocket::bind(local).await.unwrap();
            let mut services = vec![Service {
                service_id: "game".into(),
                protocol: Protocol::Tcp,
                endpoint: listener.local_addr().unwrap(),
                revision: 1,
                max_datagram_bytes: 1400,
                required_capability: Capability::ReliableStream,
                udp_return_policy: UdpReturnPolicy::ReplyBudget,
            }];
            if fragmented {
                services.push(Service {
                    service_id: "aux".into(),
                    protocol: Protocol::Udp,
                    endpoint: aux.local_addr().unwrap(),
                    revision: 1,
                    max_datagram_bytes: 4096,
                    required_capability: Capability::FragmentedDatagram,
                    udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                        bytes_per_second: 16384,
                        datagrams_per_second: 20,
                    },
                });
            }
            let reservation = std::net::UdpSocket::bind(local).unwrap();
            let bind = reservation.local_addr().unwrap();
            let host_key = generate_key();
            let guest_key = generate_key();
            let issue = if fragmented {
                SessionInvitation::issue_fragmented_loopback_for_lifetimes
            } else {
                SessionInvitation::issue_loopback_for_lifetimes
            };
            let original = issue(
                &host_key,
                guest_key.public(),
                "path-clock",
                bind,
                &services,
                Duration::from_secs(30),
                Duration::from_secs(60),
            )
            .unwrap();
            let wall = Arc::new(AtomicU64::new(unix_millis()));
            let controlled = original.clone().with_test_wall_clock(wall.clone());
            let config = HostConfig {
                secret_key: host_key,
                bind_addr: bind,
                session_id: "path-clock".into(),
                services,
                leases: vec![original.lease()],
                limits: Limits::default(),
            };
            let host_authority = if host_clock { &controlled } else { &original };
            let guest_authority = if host_clock { &original } else { &controlled };
            drop(reservation);
            let host = timeout(Duration::from_secs(5), async {
                if fragmented {
                    Host::start_invited_fragmented_session(config, host_authority).await
                } else {
                    Host::start_invited_session(config, host_authority).await
                }
            })
            .await
            .unwrap()
            .unwrap();
            let guest = timeout(Duration::from_secs(5), async {
                if fragmented {
                    Guest::connect_invited_fragmented_session(
                        guest_key,
                        guest_authority,
                        Limits::default(),
                    )
                    .await
                } else {
                    Guest::connect_invited_session(guest_key, guest_authority, Limits::default())
                        .await
                }
            })
            .await
            .unwrap()
            .unwrap();
            timeout(Duration::from_secs(5), async {
                loop {
                    if guest.path_observation().state == PathState::Direct
                        && host
                            .invitation_path_observation(original.invitation_id())
                            .is_some_and(|observation| observation.state == PathState::Direct)
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            drop(controlled);

            // There is deliberately NO await/yield in this section. Both
            // original clocks were retained by the already-connected objects.
            wall.store(original.session_expires_at_unix_ms(), Ordering::SeqCst);
            assert!(original.check_session_current().is_ok());
            assert!(!guest.inner.cancel.is_cancelled());
            assert!(guest.connection().close_reason().is_none());
            assert!(
                !guest
                    .inner
                    .session_task
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .is_finished()
            );
            if host_clock {
                assert!(
                    host.invitation_path_observation(original.invitation_id())
                        .is_none()
                );
                assert_eq!(guest.path_observation().state, PathState::Direct);
            } else {
                assert_eq!(guest.path_observation().state, PathState::Closed);
                assert_eq!(
                    host.invitation_path_observation(original.invitation_id())
                        .unwrap()
                        .state,
                    PathState::Direct
                );
            }
            // Reading the observation itself does not cancel a connection,
            // send a close message or open an application service.
            assert!(!guest.inner.cancel.is_cancelled());
            assert!(guest.connection().close_reason().is_none());
            assert_eq!(host.metrics().destination_tcp_connects, 0);
            assert!(original.check_session_current().is_ok());

            let sockets = guest.bound_sockets();
            guest.shutdown().await;
            host.shutdown().await;
            drop((guest, host, listener, aux));
            drop(std::net::UdpSocket::bind(bind).unwrap());
            for address in sockets {
                drop(std::net::UdpSocket::bind(address).unwrap());
            }
        }
    }
    eprintln!(
        "{}",
        serde_json::json!({
            "event": "path_clock_gate_observed",
            "profiles": 2,
            "authoritySides": 2,
            "gateBeforeWatcher": true,
            "osClockChanged": false,
            "applicationServiceConnections": 0
        })
    );
}
