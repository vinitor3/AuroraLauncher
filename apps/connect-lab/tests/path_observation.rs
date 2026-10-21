//! Real owned transports. An observation never opens an application service.
use aurora_connect_lab::*;
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    time::{sleep, timeout},
};

const WAIT: Duration = Duration::from_secs(5);
const PROFILES: [Profile; 4] = [
    Profile::Legacy,
    Profile::Fragmented,
    Profile::Session,
    Profile::SessionFragmented,
];
fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    timeout(WAIT, future).await.expect("path_fixture_deadline")
}
async fn after(deadline: u64) {
    sleep(Duration::from_millis(
        deadline.saturating_sub(unix_millis()) + 30,
    ))
    .await;
}
async fn observe_until(mut sample: impl FnMut() -> Option<PathObservation>, state: PathState) {
    bounded(async {
        loop {
            if sample().is_some_and(|observation| observation.state == state) {
                break;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
}
#[derive(Clone, Copy)]
enum Profile {
    Legacy,
    Fragmented,
    Session,
    SessionFragmented,
}
impl Profile {
    fn fragmented(self) -> bool {
        matches!(self, Self::Fragmented | Self::SessionFragmented)
    }
}
fn services(profile: Profile, game: SocketAddr, aux: SocketAddr) -> Vec<Service> {
    let mut services = vec![Service {
        service_id: "game".into(),
        protocol: Protocol::Tcp,
        endpoint: game,
        revision: 1,
        max_datagram_bytes: 1400,
        required_capability: Capability::ReliableStream,
        udp_return_policy: UdpReturnPolicy::ReplyBudget,
    }];
    if profile.fragmented() {
        services.push(Service {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            endpoint: aux,
            revision: 1,
            max_datagram_bytes: 4096,
            required_capability: Capability::FragmentedDatagram,
            udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                bytes_per_second: 16384,
                datagrams_per_second: 20,
            },
        });
    }
    services
}
enum Approved {
    Legacy(Invitation, bool),
    Session(SessionInvitation, bool),
}
impl Approved {
    fn issue(
        profile: Profile,
        host: &SecretKey,
        guest: EndpointId,
        bind: SocketAddr,
        services: &[Service],
        direct_ipv4: bool,
        lifetimes: (Duration, Duration),
    ) -> Self {
        let (admission, session) = lifetimes;
        match profile {
            Profile::Legacy => {
                let issue = if direct_ipv4 {
                    Invitation::issue_direct_ipv4
                } else {
                    Invitation::issue
                };
                Self::Legacy(
                    issue(
                        host,
                        guest,
                        "path-observation",
                        bind,
                        services,
                        unix_millis() + session.as_millis() as u64,
                    )
                    .unwrap(),
                    false,
                )
            }
            Profile::Fragmented => {
                let issue = if direct_ipv4 {
                    Invitation::issue_fragmented_direct_ipv4_for_lifetime
                } else {
                    Invitation::issue_fragmented_for_lifetime
                };
                Self::Legacy(
                    issue(host, guest, "path-observation", bind, services, session).unwrap(),
                    true,
                )
            }
            Profile::Session | Profile::SessionFragmented => {
                let issue = match (profile.fragmented(), direct_ipv4) {
                    (false, false) => SessionInvitation::issue_loopback_for_lifetimes,
                    (false, true) => SessionInvitation::issue_direct_ipv4_for_lifetimes,
                    (true, false) => SessionInvitation::issue_fragmented_loopback_for_lifetimes,
                    (true, true) => SessionInvitation::issue_fragmented_direct_ipv4_for_lifetimes,
                };
                Self::Session(
                    issue(
                        host,
                        guest,
                        "path-observation",
                        bind,
                        services,
                        admission,
                        session,
                    )
                    .unwrap(),
                    profile.fragmented(),
                )
            }
        }
    }
    fn lease(&self) -> Lease {
        match self {
            Self::Legacy(value, _) => value.lease(),
            Self::Session(value, _) => value.lease(),
        }
    }
    fn expiry(&self) -> u64 {
        self.lease().expires_at_unix_ms
    }
    async fn start(&self, config: HostConfig, direct_ipv4: bool) -> Host {
        bounded(async {
            match (self, direct_ipv4) {
                (Self::Legacy(value, false), false) => Host::start_invited(config, value).await,
                (Self::Legacy(value, false), true) => {
                    Host::start_invited_direct_ipv4(config, value).await
                }
                (Self::Legacy(value, true), false) => {
                    Host::start_invited_fragmented(config, value).await
                }
                (Self::Legacy(value, true), true) => {
                    Host::start_invited_fragmented_direct_ipv4(config, value).await
                }
                (Self::Session(value, false), false) => {
                    Host::start_invited_session(config, value).await
                }
                (Self::Session(value, false), true) => {
                    Host::start_invited_session_direct_ipv4(config, value).await
                }
                (Self::Session(value, true), false) => {
                    Host::start_invited_fragmented_session(config, value).await
                }
                (Self::Session(value, true), true) => {
                    Host::start_invited_fragmented_session_direct_ipv4(config, value).await
                }
            }
        })
        .await
        .unwrap()
    }
    async fn connect(&self, key: SecretKey, direct_ipv4: bool) -> Result<Guest> {
        bounded(async {
            match (self, direct_ipv4) {
                (Self::Legacy(value, false), false) => {
                    Guest::connect_invited(key, value, Limits::default()).await
                }
                (Self::Legacy(value, false), true) => {
                    Guest::connect_invited_direct_ipv4(key, value, Limits::default()).await
                }
                (Self::Legacy(value, true), false) => {
                    Guest::connect_invited_fragmented(key, value, Limits::default()).await
                }
                (Self::Legacy(value, true), true) => {
                    Guest::connect_invited_fragmented_direct_ipv4(key, value, Limits::default())
                        .await
                }
                (Self::Session(value, false), false) => {
                    Guest::connect_invited_session(key, value, Limits::default()).await
                }
                (Self::Session(value, false), true) => {
                    Guest::connect_invited_session_direct_ipv4(key, value, Limits::default()).await
                }
                (Self::Session(value, true), false) => {
                    Guest::connect_invited_fragmented_session(key, value, Limits::default()).await
                }
                (Self::Session(value, true), true) => {
                    Guest::connect_invited_fragmented_session_direct_ipv4(
                        key,
                        value,
                        Limits::default(),
                    )
                    .await
                }
            }
        })
        .await
    }
}
struct Fixture {
    host: Host,
    key: SecretKey,
    approved: Approved,
    listener: TcpListener,
    aux: UdpSocket,
    bind: SocketAddr,
}
impl Fixture {
    async fn new(profile: Profile, own_ipv4: Option<std::net::Ipv4Addr>, short: bool) -> Self {
        let listener = TcpListener::bind(local()).await.unwrap();
        let aux = UdpSocket::bind(local()).await.unwrap();
        let services = services(
            profile,
            listener.local_addr().unwrap(),
            aux.local_addr().unwrap(),
        );
        let reservation = std::net::UdpSocket::bind(
            own_ipv4.map_or(local(), |address| SocketAddr::from((address, 0))),
        )
        .unwrap();
        let bind = reservation.local_addr().unwrap();
        let host_key = generate_key();
        let key = generate_key();
        let approved = Approved::issue(
            profile,
            &host_key,
            key.public(),
            bind,
            &services,
            own_ipv4.is_some(),
            if short {
                (Duration::from_secs(1), Duration::from_secs(3))
            } else {
                (Duration::from_secs(30), Duration::from_secs(60))
            },
        );
        drop(reservation);
        let host = approved
            .start(
                HostConfig {
                    secret_key: host_key,
                    bind_addr: bind,
                    session_id: "path-observation".into(),
                    services,
                    leases: vec![approved.lease()],
                    limits: Limits::default(),
                },
                own_ipv4.is_some(),
            )
            .await;
        Self {
            host,
            key,
            approved,
            listener,
            aux,
            bind,
        }
    }
    async fn finish(self) {
        let game = self.listener.local_addr().unwrap();
        let aux = self.aux.local_addr().unwrap();
        bounded(self.host.shutdown()).await;
        let bind = self.bind;
        drop(self);
        drop(std::net::UdpSocket::bind(bind).unwrap());
        drop(std::net::TcpListener::bind(game).unwrap());
        drop(std::net::UdpSocket::bind(aux).unwrap());
    }
}
async fn exchange(listener: &TcpListener, guest: &Guest, tag: u8) {
    let bridge = bounded(guest.tcp_bridge("game", 1, local())).await.unwrap();
    let address = bridge.local_addr();
    bounded(async {
        let mut app = TcpStream::connect(address).await.unwrap();
        let (mut destination, _) = listener.accept().await.unwrap();
        let payload = [tag; 1024];
        app.write_all(&payload).await.unwrap();
        let mut received = [0; 1024];
        destination.read_exact(&mut received).await.unwrap();
        assert_eq!(received, payload);
        destination.write_all(&received).await.unwrap();
        app.read_exact(&mut received).await.unwrap();
        assert_eq!(received, payload);
    })
    .await;
    bounded(bridge.shutdown()).await;
    drop(bridge);
    drop(std::net::TcpListener::bind(address).unwrap());
}
async fn finish_guest(guest: Guest) {
    let sockets = guest.bound_sockets();
    bounded(guest.shutdown()).await;
    drop(guest);
    for address in sockets {
        drop(std::net::UdpSocket::bind(address).unwrap());
    }
}
async fn direct_case(profile: Profile, own_ipv4: Option<std::net::Ipv4Addr>) {
    let fixture = Fixture::new(profile, own_ipv4, false).await;
    let lease = fixture.approved.lease().lease_id;
    assert!(fixture.host.invitation_path_observation(&lease).is_none());
    assert!(
        fixture
            .host
            .invitation_path_observation("missing")
            .is_none()
    );
    let guest = fixture
        .approved
        .connect(fixture.key.clone(), own_ipv4.is_some())
        .await
        .unwrap();
    observe_until(|| Some(guest.path_observation()), PathState::Direct).await;
    observe_until(
        || fixture.host.invitation_path_observation(&lease),
        PathState::Direct,
    )
    .await;
    for _ in 0..8 {
        assert_eq!(guest.path_observation().state, PathState::Direct);
        assert_eq!(
            fixture
                .host
                .invitation_path_observation(&lease)
                .unwrap()
                .state,
            PathState::Direct
        );
    }
    assert_eq!(fixture.host.metrics().destination_tcp_connects, 0);
    assert_eq!(guest.metrics().destination_tcp_connects, 0);
    exchange(&fixture.listener, &guest, 37).await;
    assert_eq!(fixture.host.metrics().destination_tcp_connects, 1);
    let retained = guest.clone();
    bounded(guest.shutdown()).await;
    assert_eq!(retained.path_observation().state, PathState::Closed);
    observe_until(
        || fixture.host.invitation_path_observation(&lease),
        PathState::Closed,
    )
    .await;
    assert!(fixture.host.invitation_consumed(&lease));
    assert!(
        fixture
            .approved
            .connect(fixture.key.clone(), own_ipv4.is_some())
            .await
            .is_err()
    );
    assert_eq!(
        fixture
            .host
            .invitation_path_observation(&lease)
            .unwrap()
            .state,
        PathState::Closed
    );
    drop(retained);
    finish_guest(guest).await;
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn path_observation_real_direct_all_profiles_close_replay_and_no_service_io() {
    for profile in PROFILES {
        direct_case(profile, None).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn path_observation_original_expiry_and_consumed_ingress_are_independent() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile, None, true).await;
        let lease = fixture.approved.lease().lease_id;
        let guest = fixture
            .approved
            .connect(fixture.key.clone(), false)
            .await
            .unwrap();
        observe_until(|| Some(guest.path_observation()), PathState::Direct).await;
        if let Approved::Session(invitation, _) = &fixture.approved {
            after(invitation.admission_expires_at_unix_ms()).await;
            assert!(invitation.check_admission_current().is_err());
            assert!(invitation.check_session_current().is_ok());
            assert_eq!(guest.path_observation().state, PathState::Direct);
            assert_eq!(
                fixture
                    .host
                    .invitation_path_observation(&lease)
                    .unwrap()
                    .state,
                PathState::Direct
            );
            exchange(&fixture.listener, &guest, 41).await;
        }
        let retained = guest.clone();
        after(fixture.approved.expiry()).await;
        // This real-expiry case checks final behavior. A separate current-thread
        // unit test isolates the synchronous clock gate from the close watcher.
        assert_eq!(guest.path_observation().state, PathState::Closed);
        assert_eq!(retained.path_observation().state, PathState::Closed);
        assert!(fixture.host.invitation_path_observation(&lease).is_none());
        assert!(fixture.host.invitation_consumed(&lease));
        assert!(
            fixture
                .approved
                .connect(fixture.key.clone(), false)
                .await
                .is_err()
        );
        drop(retained);
        finish_guest(guest).await;
        fixture.finish().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn path_observation_manual_host_never_exposes_an_invitation_observation() {
    let listener = TcpListener::bind(local()).await.unwrap();
    let host_key = generate_key();
    let key = generate_key();
    let host = Host::start(HostConfig {
        secret_key: host_key,
        bind_addr: local(),
        session_id: "path-manual".into(),
        services: services(Profile::Legacy, listener.local_addr().unwrap(), local()),
        leases: vec![Lease {
            lease_id: "manual".into(),
            peer: key.public(),
            expires_at_unix_ms: unix_millis() + 60_000,
            services: ["game".into()].into_iter().collect(),
        }],
        limits: Limits::default(),
    })
    .await
    .unwrap();
    let sockets = host.bound_sockets();
    let guest = Guest::connect(GuestConfig {
        secret_key: key,
        bind_addr: local(),
        host: host.addr(),
        session_id: "path-manual".into(),
        lease_id: "manual".into(),
        capabilities: vec![Capability::ReliableStream],
        limits: Limits::default(),
    })
    .await
    .unwrap();
    observe_until(|| Some(guest.path_observation()), PathState::Direct).await;
    assert!(host.invitation_path_observation("manual").is_none());
    assert!(host.invitation_path_observation("missing").is_none());
    exchange(&listener, &guest, 43).await;
    finish_guest(guest).await;
    bounded(host.shutdown()).await;
    assert!(host.invitation_path_observation("manual").is_none());
    drop(host);
    for address in sockets {
        drop(std::net::UdpSocket::bind(address).unwrap());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn path_observation_group_revocation_is_individual_and_cannot_reveal_other_lease() {
    let listener = TcpListener::bind(local()).await.unwrap();
    let aux = UdpSocket::bind(local()).await.unwrap();
    let services = services(
        Profile::SessionFragmented,
        listener.local_addr().unwrap(),
        aux.local_addr().unwrap(),
    );
    let reservation = std::net::UdpSocket::bind(local()).unwrap();
    let bind = reservation.local_addr().unwrap();
    let host_key = generate_key();
    let keys = [generate_key(), generate_key()];
    let invitations: Vec<_> = keys
        .iter()
        .map(|key| {
            SessionInvitation::issue_fragmented_loopback_for_lifetimes(
                &host_key,
                key.public(),
                "path-group",
                bind,
                &services,
                Duration::from_secs(30),
                Duration::from_secs(60),
            )
            .unwrap()
        })
        .collect();
    drop(reservation);
    let host = Host::start_invited_fragmented_session_group(
        HostConfig {
            secret_key: host_key,
            bind_addr: bind,
            session_id: "path-group".into(),
            services,
            leases: invitations
                .iter()
                .rev()
                .map(SessionInvitation::lease)
                .collect(),
            limits: Limits::default(),
        },
        &invitations,
    )
    .await
    .unwrap();
    let a = Guest::connect_invited_fragmented_session(
        keys[0].clone(),
        &invitations[0],
        Limits::default(),
    )
    .await
    .unwrap();
    assert!(
        host.invitation_path_observation(invitations[1].invitation_id())
            .is_none()
    );
    let b = Guest::connect_invited_fragmented_session(
        keys[1].clone(),
        &invitations[1],
        Limits::default(),
    )
    .await
    .unwrap();
    for invitation in &invitations {
        observe_until(
            || host.invitation_path_observation(invitation.invitation_id()),
            PathState::Direct,
        )
        .await;
    }
    assert!(host.revoke(invitations[0].invitation_id()));
    assert!(
        host.invitation_path_observation(invitations[0].invitation_id())
            .is_none()
    );
    observe_until(|| Some(a.path_observation()), PathState::Closed).await;
    assert_eq!(
        host.invitation_path_observation(invitations[1].invitation_id())
            .unwrap()
            .state,
        PathState::Direct
    );
    assert_eq!(b.path_observation().state, PathState::Direct);
    exchange(&listener, &b, 47).await;
    finish_guest(a).await;
    finish_guest(b).await;
    bounded(host.shutdown()).await;
    for invitation in &invitations {
        assert!(
            host.invitation_path_observation(invitation.invitation_id())
                .is_none()
        );
        assert!(host.invitation_consumed(invitation.invitation_id()));
    }
    drop(host);
    drop(std::net::UdpSocket::bind(bind).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit own IPv4 from AURORA_CONNECT_TEST_IPV4; only this PC"]
async fn path_observation_explicit_own_ipv4_all_four_profiles() {
    let address = std::env::var("AURORA_CONNECT_TEST_IPV4")
        .expect("explicit own IPv4 required")
        .parse::<std::net::Ipv4Addr>()
        .expect("own IPv4 must be numeric");
    validate_direct_ipv4_invitation_address(SocketAddr::from((address, 42000))).unwrap();
    for profile in PROFILES {
        direct_case(profile, Some(address)).await;
    }
}

#[cfg(feature = "local-relay-lab")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn path_observation_real_owned_local_relay_has_no_direct_fallback() {
    let listener = TcpListener::bind(local()).await.unwrap();
    let host_key = generate_key();
    let key = generate_key();
    let relay =
        LocalRelayFixture::start([host_key.public(), key.public()], LocalRelayProfile::Normal)
            .await
            .unwrap();
    let ticket = relay.ticket();
    let host = Host::start_local_relay(
        HostConfig {
            secret_key: host_key,
            bind_addr: local(),
            session_id: "path-relay".into(),
            services: services(Profile::Legacy, listener.local_addr().unwrap(), local()),
            leases: vec![Lease {
                lease_id: "manual-relay".into(),
                peer: key.public(),
                // The fixture ticket started its own 60s lifetime before the
                // relay listener was ready. Keep this lease inside that bound.
                expires_at_unix_ms: unix_millis() + 45_000,
                services: ["game".into()].into_iter().collect(),
            }],
            limits: Limits::default(),
        },
        &ticket,
    )
    .await
    .unwrap();
    let guest = Guest::connect_local_relay(
        GuestConfig {
            secret_key: key,
            bind_addr: local(),
            host: EndpointAddr::new(host.id()),
            session_id: "path-relay".into(),
            lease_id: "manual-relay".into(),
            capabilities: vec![Capability::ReliableStream],
            limits: Limits::default(),
        },
        &ticket,
    )
    .await
    .unwrap();
    assert!(host.bound_sockets().is_empty());
    assert!(guest.bound_sockets().is_empty());
    observe_until(|| Some(guest.path_observation()), PathState::Relay).await;
    assert!(host.invitation_path_observation("manual-relay").is_none());
    assert_eq!(host.metrics().destination_tcp_connects, 0);
    exchange(&listener, &guest, 53).await;
    bounded(guest.shutdown()).await;
    assert_eq!(guest.path_observation().state, PathState::Closed);
    drop(guest);
    bounded(host.shutdown()).await;
    drop(host);
    bounded(relay.shutdown()).await.unwrap();
}
