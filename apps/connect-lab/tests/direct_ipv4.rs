//! Explicit signed IPv4 profiles. Default tests open no socket or Tokio runtime.
//! Opt-ins require an own IPv4, bind it before dialing, and prove only this PC.
use aurora_connect_lab::*;
use std::{
    future::Future,
    net::{Ipv4Addr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    task::{JoinHandle, JoinSet},
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;

const OWN_IPV4: &str = "AURORA_CONNECT_TEST_IPV4";
const WAIT: Duration = Duration::from_secs(5);
#[derive(Clone, Copy)]
enum Profile {
    Legacy,
    Fragmented,
    Session,
    SessionFragmented,
}
const PROFILES: [Profile; 4] = [
    Profile::Legacy,
    Profile::Fragmented,
    Profile::Session,
    Profile::SessionFragmented,
];
impl Profile {
    fn fragmented(self) -> bool {
        matches!(self, Self::Fragmented | Self::SessionFragmented)
    }
    fn services(self, game: SocketAddr, aux: SocketAddr) -> Vec<Service> {
        let mut services = vec![Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: game,
            revision: 1,
            required_capability: Capability::ReliableStream,
            max_datagram_bytes: 1400,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        }];
        if !matches!(self, Self::Session) {
            services.push(Service {
                service_id: "aux".into(),
                protocol: Protocol::Udp,
                endpoint: aux,
                revision: 1,
                required_capability: if self.fragmented() {
                    Capability::FragmentedDatagram
                } else {
                    Capability::Datagram
                },
                max_datagram_bytes: if self.fragmented() { 4096 } else { 1024 },
                udp_return_policy: if self.fragmented() {
                    UdpReturnPolicy::AuthorizedDownstream {
                        bytes_per_second: 16384,
                        datagrams_per_second: 20,
                    }
                } else {
                    UdpReturnPolicy::ReplyBudget
                },
            });
        }
        services
    }
}
#[derive(Clone)]
enum Approved {
    Legacy(Invitation, bool),
    Session(SessionInvitation, bool),
}
impl Approved {
    fn issue(
        profile: Profile,
        host: &SecretKey,
        guest: EndpointId,
        address: SocketAddr,
        services: &[Service],
        admission: u64,
        session: u64,
    ) -> Self {
        match profile {
            Profile::Legacy => Self::Legacy(
                Invitation::issue_direct_ipv4(
                    host,
                    guest,
                    "ipv4-signed",
                    address,
                    services,
                    unix_millis() + session * 1000,
                )
                .unwrap(),
                false,
            ),
            Profile::Fragmented => Self::Legacy(
                Invitation::issue_fragmented_direct_ipv4_for_lifetime(
                    host,
                    guest,
                    "ipv4-signed",
                    address,
                    services,
                    Duration::from_secs(session),
                )
                .unwrap(),
                true,
            ),
            Profile::Session => Self::Session(
                SessionInvitation::issue_direct_ipv4_for_lifetimes(
                    host,
                    guest,
                    "ipv4-signed",
                    address,
                    services,
                    Duration::from_secs(admission),
                    Duration::from_secs(session),
                )
                .unwrap(),
                false,
            ),
            Profile::SessionFragmented => Self::Session(
                SessionInvitation::issue_fragmented_direct_ipv4_for_lifetimes(
                    host,
                    guest,
                    "ipv4-signed",
                    address,
                    services,
                    Duration::from_secs(admission),
                    Duration::from_secs(session),
                )
                .unwrap(),
                true,
            ),
        }
    }
    fn encode(&self) -> String {
        match self {
            Self::Legacy(v, _) => v.encode(),
            Self::Session(v, _) => v.encode(),
        }
    }
    fn digest(&self) -> String {
        match self {
            Self::Legacy(v, _) => v.digest(),
            Self::Session(v, _) => v.digest(),
        }
    }
    fn lease(&self) -> Lease {
        match self {
            Self::Legacy(v, _) => v.lease(),
            Self::Session(v, _) => v.lease(),
        }
    }
    fn address(&self) -> SocketAddr {
        match self {
            Self::Legacy(v, _) => v.direct_addr(),
            Self::Session(v, _) => v.direct_addr(),
        }
    }
    fn verify(&self, encoded: &str, host: EndpointId, guest: EndpointId) -> Result<Self> {
        match self {
            Self::Legacy(_, false) => {
                Invitation::verify_direct_ipv4(encoded, host, guest).map(|v| Self::Legacy(v, false))
            }
            Self::Legacy(_, true) => {
                Invitation::verify_fragmented_direct_ipv4(encoded, host, guest)
                    .map(|v| Self::Legacy(v, true))
            }
            Self::Session(_, false) => SessionInvitation::verify_direct_ipv4(encoded, host, guest)
                .map(|v| Self::Session(v, false)),
            Self::Session(_, true) => {
                SessionInvitation::verify_fragmented_direct_ipv4(encoded, host, guest)
                    .map(|v| Self::Session(v, true))
            }
        }
    }
    fn config(&self, host: &SecretKey, services: Vec<Service>) -> HostConfig {
        HostConfig {
            secret_key: host.clone(),
            bind_addr: self.address(),
            session_id: "ipv4-signed".into(),
            services,
            leases: vec![self.lease()],
            limits: Limits::default(),
        }
    }
    async fn host(&self, config: HostConfig) -> Result<Host> {
        match self {
            Self::Legacy(v, false) => Host::start_invited_direct_ipv4(config, v).await,
            Self::Legacy(v, true) => Host::start_invited_fragmented_direct_ipv4(config, v).await,
            Self::Session(v, false) => Host::start_invited_session_direct_ipv4(config, v).await,
            Self::Session(v, true) => {
                Host::start_invited_fragmented_session_direct_ipv4(config, v).await
            }
        }
    }
    async fn guest(&self, key: SecretKey) -> Result<Guest> {
        match self {
            Self::Legacy(v, false) => {
                Guest::connect_invited_direct_ipv4(key, v, Limits::default()).await
            }
            Self::Legacy(v, true) => {
                Guest::connect_invited_fragmented_direct_ipv4(key, v, Limits::default()).await
            }
            Self::Session(v, false) => {
                Guest::connect_invited_session_direct_ipv4(key, v, Limits::default()).await
            }
            Self::Session(v, true) => {
                Guest::connect_invited_fragmented_session_direct_ipv4(key, v, Limits::default())
                    .await
            }
        }
    }
}
fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn static_address() -> SocketAddr {
    "10.23.4.5:32000".parse().unwrap()
}
fn static_services(profile: Profile) -> Vec<Service> {
    profile.services(
        "127.0.0.1:43001".parse().unwrap(),
        "127.0.0.1:43002".parse().unwrap(),
    )
}
fn ready_error<T>(future: impl Future<Output = Result<T>>, expected: Error) {
    // No runtime exists here: endpoint creation would panic or yield Pending.
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        matches!(future.as_mut().poll(&mut context), Poll::Ready(Err(e)) if e == expected),
        "policy_must_refuse_before_io"
    );
}

#[test]
fn ipv4_policy_conservative_block_boundaries_are_pure_and_explicit() {
    let denied = [
        ("0.0.0.0", "0.255.255.255"),
        ("127.0.0.0", "127.255.255.255"),
        ("100.64.0.0", "100.127.255.255"),
        ("169.254.0.0", "169.254.255.255"),
        ("192.0.0.0", "192.0.0.255"),
        ("192.0.2.0", "192.0.2.255"),
        ("192.31.196.0", "192.31.196.255"),
        ("192.52.193.0", "192.52.193.255"),
        ("192.88.99.0", "192.88.99.255"),
        ("192.175.48.0", "192.175.48.255"),
        ("198.18.0.0", "198.19.255.255"),
        ("198.51.100.0", "198.51.100.255"),
        ("203.0.113.0", "203.0.113.255"),
        ("224.0.0.0", "255.255.255.255"),
    ];
    for (start, end) in denied {
        for ip in [start, end] {
            let address: SocketAddr = format!("{ip}:1").parse().unwrap();
            assert_eq!(
                validate_direct_ipv4_invitation_address(address),
                Err(Error::Configuration)
            );
        }
        let first = u32::from(start.parse::<Ipv4Addr>().unwrap());
        let last = u32::from(end.parse::<Ipv4Addr>().unwrap());
        if let Some(before) = first.checked_sub(1) {
            assert!(
                validate_direct_ipv4_invitation_address((Ipv4Addr::from(before), 1).into()).is_ok()
            );
        }
        if let Some(after) = last.checked_add(1) {
            assert!(
                validate_direct_ipv4_invitation_address((Ipv4Addr::from(after), 65535).into())
                    .is_ok()
            );
        }
    }
    for ip in [
        "10.0.0.1",
        "172.16.0.1",
        "172.31.255.254",
        "192.168.1.1",
        "8.8.8.8",
        "192.0.1.1",
    ] {
        assert!(
            validate_direct_ipv4_invitation_address(format!("{ip}:42000").parse().unwrap()).is_ok()
        );
    }
    for text in [
        "10.0.0.1:0",
        "[::]:42000",
        "[::1]:42000",
        "[::ffff:10.0.0.1]:42000",
        "[2003::1]:42000",
    ] {
        assert_eq!(
            validate_direct_ipv4_invitation_address(text.parse().unwrap()),
            Err(Error::Configuration)
        );
    }
}

#[test]
fn four_signed_profiles_roundtrip_without_new_schema_fields_or_signature_domain() {
    let host = generate_key();
    let guest = generate_key();
    for profile in PROFILES {
        let invitation = Approved::issue(
            profile,
            &host,
            guest.public(),
            static_address(),
            &static_services(profile),
            5,
            20,
        );
        let verified = invitation
            .verify(&invitation.encode(), host.public(), guest.public())
            .unwrap();
        assert_eq!(verified.digest(), invitation.digest());
        let encoded = invitation.encode();
        let envelope: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        let body = &envelope["body"];
        assert!(body.get("addressMode").is_none());
        assert_eq!(
            body.as_object().unwrap().len(),
            if matches!(profile, Profile::Legacy | Profile::Fragmented) {
                10
            } else {
                11
            }
        );
        let domain: &[u8] = match profile {
            Profile::Legacy | Profile::Fragmented => b"aurora/connect-lab/invitation/v1\0",
            Profile::Session => b"aurora/connect-lab/session-invitation/v2\0",
            Profile::SessionFragmented => b"aurora/connect-lab/session-invitation/v3\0",
        };
        let body_bytes = encoded
            .strip_prefix("{\"body\":")
            .unwrap()
            .rsplit_once(",\"signature\":")
            .unwrap()
            .0
            .as_bytes();
        let signature_hex = envelope["signature"].as_str().unwrap();
        let mut signature = [0; 64];
        for (index, byte) in signature.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&signature_hex[index * 2..index * 2 + 2], 16).unwrap();
        }
        let mut signed = domain.to_vec();
        signed.extend_from_slice(body_bytes);
        assert!(
            host.public()
                .verify(&signed, &iroh::Signature::from_bytes(&signature))
                .is_ok()
        );
        assert_eq!(body["directAddress"], static_address().to_string());
        assert_eq!(
            body["schemaVersion"],
            match profile {
                Profile::Legacy | Profile::Fragmented => 1,
                Profile::Session => 2,
                Profile::SessionFragmented => 3,
            }
        );
        assert!(
            invitation
                .verify(&encoded, generate_key().public(), guest.public())
                .is_err()
        );
        assert!(
            invitation
                .verify(&encoded, host.public(), generate_key().public())
                .is_err()
        );
        let tampered = encoded.replace("10.23.4.5:32000", "10.23.4.6:32000");
        assert!(
            invitation
                .verify(&tampered, host.public(), guest.public())
                .is_err()
        );
        match &invitation {
            Approved::Legacy(v, fragmented) => {
                assert!(Invitation::verify(&encoded, host.public(), guest.public()).is_err());
                assert!(
                    Invitation::verify_fragmented(&encoded, host.public(), guest.public()).is_err()
                );
                assert!(
                    Invitation::verify_external_ipv6(&encoded, host.public(), guest.public())
                        .is_err()
                );
                assert!(
                    Invitation::verify_fragmented_external_ipv6(
                        &encoded,
                        host.public(),
                        guest.public()
                    )
                    .is_err()
                );
                let wrong = if *fragmented {
                    Invitation::verify_direct_ipv4
                } else {
                    Invitation::verify_fragmented_direct_ipv4
                };
                assert!(wrong(&v.encode(), host.public(), guest.public()).is_err());
            }
            Approved::Session(v, fragmented) => {
                assert!(
                    SessionInvitation::verify_loopback(&encoded, host.public(), guest.public())
                        .is_err()
                );
                assert!(
                    SessionInvitation::verify_fragmented_loopback(
                        &encoded,
                        host.public(),
                        guest.public()
                    )
                    .is_err()
                );
                assert!(
                    SessionInvitation::verify_external_ipv6(
                        &encoded,
                        host.public(),
                        guest.public()
                    )
                    .is_err()
                );
                assert!(
                    SessionInvitation::verify_fragmented_external_ipv6(
                        &encoded,
                        host.public(),
                        guest.public()
                    )
                    .is_err()
                );
                let wrong = if *fragmented {
                    SessionInvitation::verify_direct_ipv4
                } else {
                    SessionInvitation::verify_fragmented_direct_ipv4
                };
                assert!(wrong(&v.encode(), host.public(), guest.public()).is_err());
            }
        }
    }
}

#[test]
fn host_guest_mode_profile_identity_and_registry_refusals_precede_any_runtime_or_socket() {
    let host = generate_key();
    let guest = generate_key();
    for profile in PROFILES {
        let services = static_services(profile);
        let v = Approved::issue(
            profile,
            &host,
            guest.public(),
            static_address(),
            &services,
            5,
            20,
        );
        ready_error(v.guest(generate_key()), Error::Unauthorized);
        let mut config = v.config(&host, services.clone());
        config.bind_addr = "0.0.0.0:32000".parse().unwrap();
        ready_error(v.host(config), Error::Configuration);
        let mut config = v.config(&host, services.clone());
        config.services[0].revision += 1;
        ready_error(v.host(config), Error::Configuration);
        match &v {
            Approved::Legacy(v, fragmented) => {
                let config = || HostConfig {
                    secret_key: host.clone(),
                    bind_addr: v.direct_addr(),
                    session_id: v.session_id().into(),
                    services: services.clone(),
                    leases: vec![v.lease()],
                    limits: Limits::default(),
                };
                ready_error(Host::start_invited(config(), v), Error::Configuration);
                ready_error(
                    Host::start_invited_external_ipv6(config(), v),
                    Error::Configuration,
                );
                ready_error(
                    Host::start_invited_fragmented(config(), v),
                    Error::Configuration,
                );
                ready_error(
                    Host::start_invited_fragmented_external_ipv6(config(), v),
                    Error::Configuration,
                );
                ready_error(
                    Guest::connect_invited(guest.clone(), v, Limits::default()),
                    Error::Configuration,
                );
                ready_error(
                    Guest::connect_invited_external_ipv6(guest.clone(), v, Limits::default()),
                    Error::Configuration,
                );
                ready_error(
                    Guest::connect_invited_fragmented(guest.clone(), v, Limits::default()),
                    Error::Configuration,
                );
                ready_error(
                    Guest::connect_invited_fragmented_external_ipv6(
                        guest.clone(),
                        v,
                        Limits::default(),
                    ),
                    Error::Configuration,
                );
                if *fragmented {
                    ready_error(
                        Host::start_invited_direct_ipv4(config(), v),
                        Error::Configuration,
                    );
                    ready_error(
                        Guest::connect_invited_direct_ipv4(guest.clone(), v, Limits::default()),
                        Error::Configuration,
                    );
                } else {
                    ready_error(
                        Host::start_invited_fragmented_direct_ipv4(config(), v),
                        Error::Configuration,
                    );
                    ready_error(
                        Guest::connect_invited_fragmented_direct_ipv4(
                            guest.clone(),
                            v,
                            Limits::default(),
                        ),
                        Error::Configuration,
                    );
                }
            }
            Approved::Session(v, fragmented) => {
                let config = || HostConfig {
                    secret_key: host.clone(),
                    bind_addr: v.direct_addr(),
                    session_id: v.session_id().into(),
                    services: services.clone(),
                    leases: vec![v.lease()],
                    limits: Limits::default(),
                };
                ready_error(
                    Host::start_invited_session(config(), v),
                    Error::Configuration,
                );
                ready_error(
                    Host::start_invited_session_external_ipv6(config(), v),
                    Error::Configuration,
                );
                ready_error(
                    Host::start_invited_fragmented_session(config(), v),
                    Error::Configuration,
                );
                ready_error(
                    Host::start_invited_fragmented_session_external_ipv6(config(), v),
                    Error::Configuration,
                );
                ready_error(
                    Guest::connect_invited_session(guest.clone(), v, Limits::default()),
                    Error::Configuration,
                );
                ready_error(
                    Guest::connect_invited_session_external_ipv6(
                        guest.clone(),
                        v,
                        Limits::default(),
                    ),
                    Error::Configuration,
                );
                ready_error(
                    Guest::connect_invited_fragmented_session(guest.clone(), v, Limits::default()),
                    Error::Configuration,
                );
                ready_error(
                    Guest::connect_invited_fragmented_session_external_ipv6(
                        guest.clone(),
                        v,
                        Limits::default(),
                    ),
                    Error::Configuration,
                );
                if *fragmented {
                    ready_error(
                        Host::start_invited_session_direct_ipv4(config(), v),
                        Error::Configuration,
                    );
                    ready_error(
                        Guest::connect_invited_session_direct_ipv4(
                            guest.clone(),
                            v,
                            Limits::default(),
                        ),
                        Error::Configuration,
                    );
                } else {
                    ready_error(
                        Host::start_invited_fragmented_session_direct_ipv4(config(), v),
                        Error::Configuration,
                    );
                    ready_error(
                        Guest::connect_invited_fragmented_session_direct_ipv4(
                            guest.clone(),
                            v,
                            Limits::default(),
                        ),
                        Error::Configuration,
                    );
                }
            }
        }
    }
}

#[test]
fn absolute_and_duration_constructors_keep_lifetime_limits_and_reject_foreign_families() {
    let host = generate_key();
    let guest = generate_key().public();
    let now = unix_millis();
    let game = static_services(Profile::Session);
    let fragmented = static_services(Profile::Fragmented);
    assert!(
        SessionInvitation::issue_direct_ipv4(
            &host,
            guest,
            "absolute",
            static_address(),
            &game,
            SessionDeadlines {
                admission_expires_at_unix_ms: now + 2_000,
                session_expires_at_unix_ms: now + 10_000
            }
        )
        .is_ok()
    );
    assert!(
        Invitation::issue_fragmented_direct_ipv4(
            &host,
            guest,
            "absolute",
            static_address(),
            &fragmented,
            now + 10_000
        )
        .is_ok()
    );
    for address in [
        "127.0.0.1:32000",
        "[2003::1]:32000",
        "100.64.0.1:32000",
        "192.0.2.1:32000",
    ] {
        let address = address.parse().unwrap();
        assert!(
            Invitation::issue_direct_ipv4(&host, guest, "bad", address, &game, now + 10_000)
                .is_err()
        );
        assert!(
            Invitation::issue_fragmented_direct_ipv4_for_lifetime(
                &host,
                guest,
                "bad",
                address,
                &fragmented,
                Duration::from_secs(10)
            )
            .is_err()
        );
        assert!(
            SessionInvitation::issue_direct_ipv4_for_lifetimes(
                &host,
                guest,
                "bad",
                address,
                &game,
                Duration::from_secs(2),
                Duration::from_secs(10)
            )
            .is_err()
        );
        assert!(
            SessionInvitation::issue_fragmented_direct_ipv4_for_lifetimes(
                &host,
                guest,
                "bad",
                address,
                &fragmented,
                Duration::from_secs(2),
                Duration::from_secs(10)
            )
            .is_err()
        );
    }
    for (admission, session) in [(0, 10), (601, 602), (2, 1), (1, 3601)] {
        assert!(
            SessionInvitation::issue_direct_ipv4_for_lifetimes(
                &host,
                guest,
                "bad-clock",
                static_address(),
                &game,
                Duration::from_secs(admission),
                Duration::from_secs(session)
            )
            .is_err()
        );
        assert!(
            SessionInvitation::issue_fragmented_direct_ipv4_for_lifetimes(
                &host,
                guest,
                "bad-clock",
                static_address(),
                &fragmented,
                Duration::from_secs(admission),
                Duration::from_secs(session)
            )
            .is_err()
        );
    }
    assert!(
        Invitation::issue_fragmented_direct_ipv4_for_lifetime(
            &host,
            guest,
            "bad-clock",
            static_address(),
            &fragmented,
            Duration::from_secs(601)
        )
        .is_err()
    );
}

#[test]
fn direct_ipv4_wrappers_refuse_verified_loopback_and_ipv6_values_before_io() {
    let host = generate_key();
    let guest = generate_key();
    for external in [false, true] {
        let address = if external {
            "[2003::1]:32000"
        } else {
            "127.0.0.1:32000"
        }
        .parse()
        .unwrap();
        for profile in PROFILES {
            let services = static_services(profile);
            let approved = match profile {
                Profile::Legacy => {
                    let issue = if external {
                        Invitation::issue_external_ipv6
                    } else {
                        Invitation::issue
                    };
                    Approved::Legacy(
                        issue(
                            &host,
                            guest.public(),
                            "ipv4-signed",
                            address,
                            &services,
                            unix_millis() + 20_000,
                        )
                        .unwrap(),
                        false,
                    )
                }
                Profile::Fragmented => {
                    let issue = if external {
                        Invitation::issue_fragmented_external_ipv6
                    } else {
                        Invitation::issue_fragmented
                    };
                    Approved::Legacy(
                        issue(
                            &host,
                            guest.public(),
                            "ipv4-signed",
                            address,
                            &services,
                            unix_millis() + 20_000,
                        )
                        .unwrap(),
                        true,
                    )
                }
                Profile::Session => {
                    let issue = if external {
                        SessionInvitation::issue_external_ipv6_for_lifetimes
                    } else {
                        SessionInvitation::issue_loopback_for_lifetimes
                    };
                    Approved::Session(
                        issue(
                            &host,
                            guest.public(),
                            "ipv4-signed",
                            address,
                            &services,
                            Duration::from_secs(5),
                            Duration::from_secs(20),
                        )
                        .unwrap(),
                        false,
                    )
                }
                Profile::SessionFragmented => {
                    let issue = if external {
                        SessionInvitation::issue_fragmented_external_ipv6_for_lifetimes
                    } else {
                        SessionInvitation::issue_fragmented_loopback_for_lifetimes
                    };
                    Approved::Session(
                        issue(
                            &host,
                            guest.public(),
                            "ipv4-signed",
                            address,
                            &services,
                            Duration::from_secs(5),
                            Duration::from_secs(20),
                        )
                        .unwrap(),
                        true,
                    )
                }
            };
            assert!(
                approved
                    .verify(&approved.encode(), host.public(), guest.public())
                    .is_err()
            );
            ready_error(
                approved.host(approved.config(&host, services)),
                Error::Configuration,
            );
            ready_error(approved.guest(guest.clone()), Error::Configuration);
        }
    }
}

fn own_address() -> Ipv4Addr {
    let text = std::env::var(OWN_IPV4).expect("own_ipv4_explicit_value_required");
    let ip: Ipv4Addr = text.parse().expect("own_ipv4_literal_required");
    assert!(
        ip.to_string() == text,
        "own_ipv4_canonical_literal_required"
    );
    validate_direct_ipv4_invitation_address((ip, 1).into()).expect("own_ipv4_policy_refused");
    ip
}
async fn bounded<T>(future: impl Future<Output = T>) -> T {
    timeout(WAIT, future).await.expect("ipv4_test_deadline")
}
async fn after(at: u64) {
    sleep(Duration::from_millis(at.saturating_sub(unix_millis()) + 50)).await;
}
struct Running {
    host: Host,
    guest: Guest,
    key: SecretKey,
    approved: Approved,
    tcp: TcpBridge,
    udp: Option<UdpBridge>,
    endpoint: UdpSocket,
    address: SocketAddr,
    tcp_target: SocketAddr,
    guest_addresses: Vec<SocketAddr>,
    accepted: Arc<AtomicUsize>,
    cancel: CancellationToken,
    echo: JoinHandle<()>,
}
impl Running {
    async fn start(profile: Profile, admission: u64, session: u64) -> Self {
        // A supplied non-local address cannot get past this OS bind. No fallback,
        // interface change or dial of another address is attempted.
        let reservation =
            std::net::UdpSocket::bind((own_address(), 0)).expect("own_ipv4_local_bind_required");
        let address = reservation.local_addr().unwrap();
        let listener = TcpListener::bind(local()).await.unwrap();
        let tcp_target = listener.local_addr().unwrap();
        let endpoint = UdpSocket::bind(local()).await.unwrap();
        let services = profile.services(tcp_target, endpoint.local_addr().unwrap());
        let host_key = generate_key();
        let key = generate_key();
        let issued = Approved::issue(
            profile,
            &host_key,
            key.public(),
            address,
            &services,
            admission,
            session,
        );
        let approved = issued
            .verify(&issued.encode(), host_key.public(), key.public())
            .unwrap();
        drop(reservation);
        let host = approved
            .host(approved.config(&host_key, services))
            .await
            .expect("own_ipv4_host_start");
        assert!(
            host.bound_sockets() == vec![address],
            "host_must_use_exact_signed_bind"
        );
        let guest = approved
            .guest(key.clone())
            .await
            .expect("own_ipv4_guest_start");
        let guest_addresses = guest.bound_sockets();
        assert!(
            guest_addresses.len() == 1
                && guest_addresses[0].is_ipv4()
                && guest_addresses[0].port() != 0,
            "guest_ipv4_transport_only"
        );
        {
            let paths = guest.connection().paths();
            assert!(!paths.is_empty() && paths.iter().all(|path| matches!(path.remote_addr(), iroh::TransportAddr::Ip(address) if address.is_ipv4()) && !path.is_relay()), "observed_paths_must_be_ipv4_not_relay");
        }
        let tcp = guest.tcp_bridge("game", 1, local()).await.unwrap();
        let udp = if matches!(profile, Profile::Session) {
            None
        } else if profile.fragmented() {
            Some(
                guest
                    .udp_bridge_fragmented("aux", 1, local())
                    .await
                    .unwrap(),
            )
        } else {
            Some(guest.udp_bridge("aux", 1, local()).await.unwrap())
        };
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let accepted = Arc::new(AtomicUsize::new(0));
        let count = accepted.clone();
        let echo = tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            loop {
                tokio::select! {
                    _=token.cancelled()=>break,
                    result=listener.accept()=> { let Ok((mut socket,_))=result else {break}; count.fetch_add(1,Ordering::SeqCst); tasks.spawn(async move { let (mut read,mut write)=socket.split(); let _=tokio::io::copy(&mut read,&mut write).await; }); },
                    _=tasks.join_next(), if !tasks.is_empty()=>(),
                }
            }
            tasks.shutdown().await;
        });
        Self {
            host,
            guest,
            key,
            approved,
            tcp,
            udp,
            endpoint,
            address,
            tcp_target,
            guest_addresses,
            accepted,
            cancel,
            echo,
        }
    }
    async fn tcp_transfer(&self) {
        let mut stream = TcpStream::connect(self.tcp.local_addr()).await.unwrap();
        bounded(async {
            let payload = vec![0x65; 65536];
            stream.write_all(&payload).await.unwrap();
            let mut received = vec![0; payload.len()];
            stream.read_exact(&mut received).await.unwrap();
            assert_eq!(received, payload);
        })
        .await;
    }
    async fn udp_two_sources(&self, size: usize) {
        let bind = self.udp.as_ref().unwrap().local_addr();
        let a = UdpSocket::bind(local()).await.unwrap();
        let b = UdpSocket::bind(local()).await.unwrap();
        let a_addr = a.local_addr().unwrap();
        let b_addr = b.local_addr().unwrap();
        a.send_to(&vec![1; size], bind).await.unwrap();
        b.send_to(&vec![2; size], bind).await.unwrap();
        let mut bytes = vec![0; 65536];
        let mut remotes = Vec::new();
        for _ in 0..2 {
            let (n, remote) = bounded(self.endpoint.recv_from(&mut bytes)).await.unwrap();
            assert_eq!(n, size);
            assert!(bytes[..n].iter().all(|v| *v == bytes[0]));
            assert!(matches!(bytes[0], 1 | 2));
            remotes.push(remote);
            self.endpoint
                .send_to(&vec![bytes[0] ^ 0x55; size], remote)
                .await
                .unwrap();
        }
        assert_ne!(remotes[0], remotes[1]);
        for (source, tag) in [(&a, 1u8), (&b, 2)] {
            let n = bounded(source.recv(&mut bytes)).await.unwrap();
            assert_eq!(n, size);
            assert!(bytes[..n].iter().all(|v| *v == tag ^ 0x55));
            assert!(
                timeout(Duration::from_millis(80), source.recv(&mut bytes))
                    .await
                    .is_err()
            );
        }
        drop(a);
        drop(b);
        drop(std::net::UdpSocket::bind(a_addr).unwrap());
        drop(std::net::UdpSocket::bind(b_addr).unwrap());
    }
    async fn close(self) {
        let tcp_address = self.tcp.local_addr();
        let udp_address = self.udp.as_ref().map(UdpBridge::local_addr);
        self.tcp.shutdown().await;
        if let Some(udp) = &self.udp {
            udp.shutdown().await;
        }
        self.guest.shutdown().await;
        self.host.shutdown().await;
        assert_eq!(self.host.metrics().active_tcp, 0);
        assert_eq!(self.host.metrics().active_udp, 0);
        assert_eq!(self.host.metrics().active_peers, 0);
        self.cancel.cancel();
        bounded(self.echo).await.unwrap();
        let endpoint = self.endpoint.local_addr().unwrap();
        drop(self.endpoint);
        drop(self.tcp);
        drop(self.udp);
        drop(self.guest);
        drop(self.host);
        drop(std::net::TcpListener::bind(tcp_address).unwrap());
        drop(std::net::TcpListener::bind(self.tcp_target).unwrap());
        for address in self
            .guest_addresses
            .into_iter()
            .chain([self.address, endpoint])
            .chain(udp_address)
        {
            drop(std::net::UdpSocket::bind(address).unwrap());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires AURORA_CONNECT_TEST_IPV4 assigned to this PC; no LAN/NAT/internet inference"]
async fn own_ipv4_all_four_signed_profiles_transport_revocation_and_rebind() {
    for profile in PROFILES {
        let running = Running::start(profile, 10, 20).await;
        running.tcp_transfer().await;
        if !matches!(profile, Profile::Session) {
            running
                .udp_two_sources(if profile.fragmented() { 4096 } else { 512 })
                .await;
        }
        let accepted = running.accepted.load(Ordering::SeqCst);
        running.host.revoke(&running.approved.lease().lease_id);
        bounded(running.guest.connection().closed()).await;
        assert!(running.guest.open_tcp("game", 1).await.is_err());
        assert_eq!(running.accepted.load(Ordering::SeqCst), accepted);
        running.close().await;
    }
    println!(
        "{{\"event\":\"signed_ipv4_profiles_passed\",\"profiles\":4,\"scope\":\"same_pc_own_ipv4\",\"relay\":false,\"tcpBytesEachDirection\":262144,\"udpApplicationBytesEachDirection\":17408,\"cleanup\":true}}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires AURORA_CONNECT_TEST_IPV4 assigned to this PC; original session clocks only"]
async fn own_ipv4_fragmented_session_after_admission_expires_at_original_deadline() {
    let running = Running::start(Profile::SessionFragmented, 2, 6).await;
    let Approved::Session(invitation, _) = &running.approved else {
        unreachable!()
    };
    let admitted_until = invitation.admission_expires_at_unix_ms();
    let session_until = invitation.session_expires_at_unix_ms();
    running.tcp_transfer().await;
    after(admitted_until).await;
    assert!(
        SessionInvitation::verify_fragmented_direct_ipv4(
            &invitation.encode(),
            invitation.host_key(),
            invitation.guest_key()
        )
        .is_err()
    );
    ready_error(running.approved.guest(running.key.clone()), Error::Expired);
    assert_eq!(
        running.host.metrics().denied,
        0,
        "late admission must be rejected before dialing"
    );
    let ((), ()) = tokio::join!(running.tcp_transfer(), running.udp_two_sources(4096));
    let mut held_tcp = TcpStream::connect(running.tcp.local_addr()).await.unwrap();
    bounded(async {
        held_tcp.write_all(&[0x65]).await.unwrap();
        let mut byte = [0];
        held_tcp.read_exact(&mut byte).await.unwrap();
        assert_eq!(byte, [0x65]);
    })
    .await;
    let source = UdpSocket::bind(local()).await.unwrap();
    let source_address = source.local_addr().unwrap();
    source
        .send_to(&[0x65; 4097], running.udp.as_ref().unwrap().local_addr())
        .await
        .unwrap();
    assert!(
        timeout(
            Duration::from_millis(150),
            running.endpoint.recv(&mut [0; 8192])
        )
        .await
        .is_err()
    );
    after(session_until).await;
    bounded(running.guest.connection().closed()).await;
    assert!(
        matches!(bounded(held_tcp.read(&mut [0])).await, Ok(0) | Err(_)),
        "session expiry must close the existing TCP stream"
    );
    assert!(running.guest.open_tcp("game", 1).await.is_err());
    assert!(running.udp.as_ref().unwrap().is_closed());
    assert_eq!(invitation.session_expires_at_unix_ms(), session_until);
    drop(source);
    drop(std::net::UdpSocket::bind(source_address).unwrap());
    running.close().await;
    println!(
        "{{\"event\":\"signed_ipv4_session_expiry_passed\",\"scope\":\"same_pc_own_ipv4\",\"udpSources\":2,\"udpPayloadEach\":4096,\"oversizeRejected\":4097,\"renewal\":false,\"cleanup\":true}}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires AURORA_CONNECT_TEST_IPV4 assigned to this PC; replay stays inside this test host"]
async fn own_ipv4_fragmented_session_replay_revocation_and_zero_new_destinations() {
    let running = Running::start(Profile::SessionFragmented, 10, 20).await;
    let initial_associations = running.host.metrics().udp_associations_opened;
    assert_eq!(
        initial_associations, 1,
        "initial successful bridge reserved one association without payload"
    );
    running.guest.shutdown().await;
    bounded(async {
        while running.host.metrics().active_peers != 0 {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    assert!(matches!(
        running.approved.guest(running.key.clone()).await,
        Err(Error::Replay)
    ));
    assert_eq!(running.host.metrics().denied, 1);
    running.host.revoke(&running.approved.lease().lease_id);
    assert!(matches!(
        running.approved.guest(running.key.clone()).await,
        Err(Error::Expired)
    ));
    assert_eq!(running.host.metrics().denied, 2);
    assert_eq!(running.host.metrics().destination_tcp_connects, 0);
    assert_eq!(
        running.host.metrics().udp_associations_opened,
        initial_associations
    );
    assert_eq!(running.host.metrics().udp_forwarded, 0);
    assert_eq!(running.accepted.load(Ordering::SeqCst), 0);
    assert!(
        timeout(
            Duration::from_millis(80),
            running.endpoint.recv(&mut [0; 8])
        )
        .await
        .is_err()
    );
    running.close().await;
    println!(
        "{{\"event\":\"signed_ipv4_replay_revocation_passed\",\"scope\":\"same_pc_own_ipv4\",\"denied\":2,\"newDestinationTcpConnects\":0,\"newUdpAssociations\":0,\"applicationPayloads\":0,\"cleanup\":true}}"
    );
}
