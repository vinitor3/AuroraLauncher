//! Explicit static schema-3 group. Individual signed envelopes and their
//! original clocks stay independent; the shared Host never synthesizes a lease
//! without its invitation authority or extends another participant's lifetime.
use super::*;

#[derive(Clone, Copy)]
enum GroupAddressMode {
    Loopback,
    ExternalIpv6,
    DirectIpv4,
}

impl Host {
    /// Static group of 2..=8 schema-3 participants, also bounded by max_peers.
    /// Existing single-invitation APIs remain strict and retain their wire form.
    pub async fn start_invited_fragmented_session_group(
        config: HostConfig,
        invitations: &[SessionInvitation],
    ) -> Result<Self> {
        Self::start_session_group(config, invitations, GroupAddressMode::Loopback).await
    }

    /// Explicit external IPv6 initial address; no address discovery or relay.
    pub async fn start_invited_fragmented_session_group_external_ipv6(
        config: HostConfig,
        invitations: &[SessionInvitation],
    ) -> Result<Self> {
        Self::start_session_group(config, invitations, GroupAddressMode::ExternalIpv6).await
    }

    /// Explicit direct IPv4 initial address; no NAT inference or port mapping.
    pub async fn start_invited_fragmented_session_group_direct_ipv4(
        config: HostConfig,
        invitations: &[SessionInvitation],
    ) -> Result<Self> {
        Self::start_session_group(config, invitations, GroupAddressMode::DirectIpv4).await
    }

    async fn start_session_group(
        config: HostConfig,
        invitations: &[SessionInvitation],
        mode: GroupAddressMode,
    ) -> Result<Self> {
        let authorities = validate_session_group(&config, invitations, mode)?;
        Self::start_with_authorities(config, authorities, transport::EndpointMode::Direct).await
    }
}

fn validate_session_group(
    config: &HostConfig,
    invitations: &[SessionInvitation],
    mode: GroupAddressMode,
) -> Result<HostAuthorities> {
    config.limits.validate()?;
    if !(2..=8).contains(&invitations.len())
        || invitations.len() > config.limits.max_peers
        || config.leases.len() != invitations.len()
    {
        return Err(Error::Configuration);
    }
    let mut authorities = HashMap::new();
    let mut peers = HashSet::new();
    let mut lease_ids = HashSet::new();
    for lease in &config.leases {
        if !lease_ids.insert(lease.lease_id.as_str()) {
            return Err(Error::Configuration);
        }
    }
    for invitation in invitations {
        if authorities.contains_key(invitation.invitation_id())
            || !peers.insert(invitation.guest_key())
        {
            return Err(Error::Configuration);
        }
        let lease = config
            .leases
            .iter()
            .find(|lease| lease.lease_id == invitation.invitation_id())
            .ok_or(Error::Configuration)?;
        // Reuse the exact existing validator with one explicitly matched
        // lease, without weakening its single-participant contract. The group
        // check above independently proves complete, unique coverage.
        let single = HostConfig {
            secret_key: config.secret_key.clone(),
            bind_addr: config.bind_addr,
            session_id: config.session_id.clone(),
            services: config.services.clone(),
            leases: vec![lease.clone()],
            limits: config.limits.clone(),
        };
        match mode {
            GroupAddressMode::Loopback => invitation.check_host_fragmented(&single)?,
            GroupAddressMode::ExternalIpv6 => {
                invitation.check_host_fragmented_external_ipv6(&single)?
            }
            GroupAddressMode::DirectIpv4 => {
                invitation.check_host_fragmented_direct_ipv4(&single)?
            }
        }
        invitation.check_admission_current()?;
        authorities.insert(
            invitation.invitation_id().to_owned(),
            InvitationAuthority::Session(invitation.clone()),
        );
    }
    Ok(HostAuthorities::Invited(authorities))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        key: SecretKey,
        invitations: Vec<SessionInvitation>,
        services: Vec<Service>,
    }
    impl Fixture {
        fn new() -> Self {
            let key = generate_key();
            let services = vec![
                Service {
                    service_id: "game".into(),
                    protocol: Protocol::Tcp,
                    endpoint: "127.0.0.1:43201".parse().unwrap(),
                    revision: 1,
                    max_datagram_bytes: 1400,
                    required_capability: Capability::ReliableStream,
                    udp_return_policy: UdpReturnPolicy::ReplyBudget,
                },
                Service {
                    service_id: "aux".into(),
                    protocol: Protocol::Udp,
                    endpoint: "127.0.0.1:43202".parse().unwrap(),
                    revision: 1,
                    max_datagram_bytes: 4096,
                    required_capability: Capability::FragmentedDatagram,
                    udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                        bytes_per_second: 16384,
                        datagrams_per_second: 20,
                    },
                },
            ];
            let invitations = (0..2)
                .map(|_| {
                    SessionInvitation::issue_fragmented_loopback_for_lifetimes(
                        &key,
                        generate_key().public(),
                        "static-group",
                        "127.0.0.1:43200".parse().unwrap(),
                        &services,
                        Duration::from_secs(30),
                        Duration::from_secs(60),
                    )
                    .unwrap()
                })
                .collect();
            Self {
                key,
                invitations,
                services,
            }
        }
        fn config(&self) -> HostConfig {
            HostConfig {
                secret_key: self.key.clone(),
                bind_addr: "127.0.0.1:43200".parse().unwrap(),
                session_id: "static-group".into(),
                services: self.services.clone(),
                leases: self
                    .invitations
                    .iter()
                    .map(SessionInvitation::lease)
                    .collect(),
                limits: Limits::default(),
            }
        }
    }

    #[test]
    fn group_validation_assigns_original_authorities_by_id_not_position() {
        let f = Fixture::new();
        let mut config = f.config();
        config.leases.reverse();
        let HostAuthorities::Invited(authorities) =
            validate_session_group(&config, &f.invitations, GroupAddressMode::Loopback).unwrap()
        else {
            panic!()
        };
        assert_eq!(authorities.len(), 2);
        for invitation in &f.invitations {
            let retained = &authorities[invitation.invitation_id()];
            assert!(retained.proof() == invitation.proof());
            assert!(retained.check_admission().is_ok());
        }
        // New composition never widens an existing single-invitation gate.
        assert!(matches!(
            f.invitations[0].check_host_fragmented(&config),
            Err(Error::Configuration)
        ));
    }

    #[test]
    fn group_retains_each_original_clock_after_reordered_lease_assignment() {
        let f = Fixture::new();
        let mut invitations = f.invitations.clone();
        let wall = Arc::new(AtomicU64::new(unix_millis()));
        invitations[0] = invitations[0].clone().with_test_wall_clock(wall.clone());
        let mut config = f.config();
        config.leases.reverse();
        let HostAuthorities::Invited(authorities) =
            validate_session_group(&config, &invitations, GroupAddressMode::Loopback).unwrap()
        else {
            panic!()
        };
        let a = &authorities[invitations[0].invitation_id()];
        let b = &authorities[invitations[1].invitation_id()];
        assert!(a.check_session().is_ok() && b.check_session().is_ok());
        wall.store(
            invitations[0].session_expires_at_unix_ms(),
            Ordering::SeqCst,
        );
        assert!(a.check_session().is_err());
        assert!(a.check_admission().is_err());
        assert!(b.check_session().is_ok() && b.check_admission().is_ok());
        assert!(a.proof() == invitations[0].proof());
        assert!(b.proof() == invitations[1].proof());
        // The unchanged counterpart using the real clock proves the expiration
        // came from A's retained authority, not elapsed test wall time.
        assert!(f.invitations[0].check_session_current().is_ok());
    }

    #[test]
    fn group_validation_refuses_incomplete_duplicate_or_changed_registry_without_io() {
        let f = Fixture::new();
        for case in 0..10 {
            let mut config = f.config();
            let mut invitations = f.invitations.clone();
            match case {
                0 => {
                    invitations.clear();
                    config.leases.clear();
                }
                1 => {
                    invitations.truncate(1);
                    config.leases.truncate(1);
                }
                2 => {
                    config.limits.max_peers = 1;
                }
                3 => {
                    config.leases.pop();
                }
                4 => {
                    invitations[1] = invitations[0].clone();
                }
                5 => {
                    config.leases[1] = config.leases[0].clone();
                }
                6 => {
                    config.leases[1].peer = generate_key().public();
                }
                7 => {
                    config.leases[1].expires_at_unix_ms += 1;
                }
                8 => {
                    config.session_id = "different-session".into();
                }
                _ => {
                    config.services[1].max_datagram_bytes = 2048;
                }
            }
            assert!(
                validate_session_group(&config, &invitations, GroupAddressMode::Loopback).is_err(),
                "case {case}"
            );
        }
        assert!(
            validate_session_group(&f.config(), &f.invitations, GroupAddressMode::ExternalIpv6)
                .is_err()
        );
        assert!(
            validate_session_group(&f.config(), &f.invitations, GroupAddressMode::DirectIpv4)
                .is_err()
        );
    }

    #[test]
    fn group_validation_refuses_schema_two_and_expired_member_without_retiming() {
        let f = Fixture::new();
        let mut invitations = f.invitations.clone();
        invitations[1] = SessionInvitation::issue_loopback_for_lifetimes(
            &f.key,
            generate_key().public(),
            "static-group",
            "127.0.0.1:43200".parse().unwrap(),
            &f.services[..1],
            Duration::from_secs(30),
            Duration::from_secs(60),
        )
        .unwrap();
        let mut config = f.config();
        config.leases = invitations.iter().map(SessionInvitation::lease).collect();
        assert!(validate_session_group(&config, &invitations, GroupAddressMode::Loopback).is_err());
        let mut invitations = f.invitations.clone();
        let expired = Arc::new(AtomicU64::new(
            invitations[1].admission_expires_at_unix_ms(),
        ));
        invitations[1] = invitations[1].clone().with_test_wall_clock(expired);
        assert!(
            validate_session_group(&f.config(), &invitations, GroupAddressMode::Loopback).is_err()
        );
        assert!(f.invitations[0].check_admission_current().is_ok());
    }
}
