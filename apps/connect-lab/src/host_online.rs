//! Dynamic admissions on one native online endpoint. Accepted, expired and
//! revoked entries stay reserved for the lifetime of the original session.
use super::*;
use crate::network_online::OnlineNetworkTicket;

/// Non-serializable one-use route for the explicit Host-player lease. It holds
/// only weak Host/lease references and never keeps a closed endpoint alive.
pub struct HostLocalPlayerTicket {
    state: std::sync::Weak<State>,
    lease: std::sync::Weak<LeaseState>,
    invitation: SessionInvitation,
    address: SocketAddr,
    used: AtomicBool,
}
impl HostLocalPlayerTicket {
    pub(crate) fn begin(
        &self,
        invitation: &SessionInvitation,
        peer: EndpointId,
    ) -> Result<EndpointAddr> {
        self.check(invitation, peer, false)?;
        self.used
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Replay)?;
        Ok(EndpointAddr::new(invitation.host_key()).with_ip_addr(self.address))
    }
    pub(crate) fn check(
        &self,
        invitation: &SessionInvitation,
        peer: EndpointId,
        admitted: bool,
    ) -> Result<()> {
        let state = self.state.upgrade().ok_or(Error::Closed)?;
        let original = self.lease.upgrade().ok_or(Error::Closed)?;
        if state.cancel.is_cancelled() || original.cancel.is_cancelled() {
            return Err(Error::Closed);
        }
        state
            .online
            .as_ref()
            .ok_or(Error::Configuration)?
            .check_current()?;
        invitation.check_same_online_profile(&self.invitation)?;
        if invitation.digest() != self.invitation.digest()
            || invitation.guest_key() != peer
            || original.value.peer != peer
            || state.session_id != invitation.session_id()
            || state
                .lease(invitation.invitation_id())
                .is_none_or(|current| !Arc::ptr_eq(&current, &original))
        {
            return Err(Error::Unauthorized);
        }
        invitation.check_session_current()?;
        if !admitted {
            invitation.check_admission_current()?;
        }
        Ok(())
    }
}

pub(super) struct OnlineAuthority {
    pub(super) first: SessionInvitation,
    ticket: OnlineNetworkTicket,
}

impl OnlineAuthority {
    pub(super) fn check_current(&self) -> Result<()> {
        self.ticket.check_current()?;
        self.first.check_session_current()
    }

    pub(super) fn remaining(&self) -> Duration {
        self.first.session_remaining().unwrap_or_default().min(
            self.ticket
                .deadline()
                .saturating_duration_since(Instant::now()),
        )
    }

    pub(super) async fn wait_expired(&self) {
        while self.check_current().is_ok() {
            tokio::time::sleep(self.remaining().min(Duration::from_millis(100))).await;
        }
    }
}

impl Host {
    pub async fn start_invited_online(
        config: HostConfig,
        invitation: &SessionInvitation,
        ticket: &OnlineNetworkTicket,
    ) -> Result<Self> {
        invitation.check_host_online(&config)?;
        Self::start_online_authorized(config, invitation, ticket).await
    }

    pub async fn start_invited_online_fragmented(
        config: HostConfig,
        invitation: &SessionInvitation,
        ticket: &OnlineNetworkTicket,
    ) -> Result<Self> {
        invitation.check_host_online_fragmented(&config)?;
        Self::start_online_authorized(config, invitation, ticket).await
    }

    pub async fn start_invited_online_native_voice(
        config: HostConfig,
        invitation: &SessionInvitation,
        ticket: &OnlineNetworkTicket,
    ) -> Result<Self> {
        invitation.check_host_online_native_voice(&config)?;
        Self::start_online_authorized(config, invitation, ticket).await
    }

    async fn start_online_authorized(
        config: HostConfig,
        invitation: &SessionInvitation,
        ticket: &OnlineNetworkTicket,
    ) -> Result<Self> {
        ticket.check_host(&config)?;
        if !crate::OnlineClock::same_scope(invitation.online_clock(), ticket.clock()) {
            return Err(Error::Unauthorized);
        }
        let authority = OnlineAuthority {
            first: invitation.clone(),
            ticket: ticket.clone(),
        };
        authority.check_current()?;
        Self::start_with_authorities(
            config,
            HostAuthorities::Online(Box::new(authority)),
            transport::EndpointMode::Online(ticket.clone()),
        )
        .await
    }

    /// Native caller must have explicitly included this player in the reviewed
    /// roster. No address argument exists: only this Host's bound IPv4 socket
    /// can supply the route, and the already-registered online voice lease is retained.
    pub fn local_player_ticket(
        &self,
        invitation: &SessionInvitation,
        local_key: &SecretKey,
    ) -> Result<HostLocalPlayerTicket> {
        invitation.check_online_local_player()?;
        let online = self.state.online.as_ref().ok_or(Error::Configuration)?;
        online.check_current()?;
        if invitation.host_key() != self.id() || invitation.guest_key() != local_key.public() {
            return Err(Error::Unauthorized);
        }
        let lease = self
            .state
            .lease(invitation.invitation_id())
            .ok_or(Error::Unauthorized)?;
        if lease.cancel.is_cancelled()
            || lease.consumed.load(Ordering::Acquire)
            || lease
                .invitation
                .as_ref()
                .is_none_or(|original| original.proof() != invitation.proof())
        {
            return Err(Error::Replay);
        }
        let port = self
            .bound_sockets()
            .into_iter()
            .find(|address| address.is_ipv4() && address.port() != 0)
            .ok_or(Error::Configuration)?
            .port();
        let ticket = HostLocalPlayerTicket {
            state: Arc::downgrade(&self.state),
            lease: Arc::downgrade(&lease),
            invitation: invitation.clone(),
            address: SocketAddr::from(([127, 0, 0, 1], port)),
            used: AtomicBool::new(false),
        };
        ticket.check(invitation, local_key.public(), false)?;
        Ok(ticket)
    }

    /// Add one independently signed participant to the original online Host.
    /// No socket, source, ticket or deadline is replaced by admission.
    pub fn admit_online(&self, invitation: &SessionInvitation) -> Result<()> {
        let online = self.state.online.as_ref().ok_or(Error::Configuration)?;
        let mut leases = self.state.leases.lock().map_err(|_| Error::Closed)?;
        if self.state.cancel.is_cancelled() {
            return Err(Error::Closed);
        }
        online.check_current()?;
        insert_admission(
            &mut leases,
            &online.first,
            invitation,
            &self.state.services,
            &self.state.limits,
        )
    }
}

fn insert_admission(
    leases: &mut HashMap<String, Arc<LeaseState>>,
    first: &SessionInvitation,
    invitation: &SessionInvitation,
    services: &HashMap<String, (u32, Service)>,
    limits: &Limits,
) -> Result<()> {
    invitation.check_same_online_profile(first)?;
    if invitation.host_key() != first.host_key()
        || invitation.session_id() != first.session_id()
        || invitation.direct_addr() != first.direct_addr()
        || invitation.session_expires_at_unix_ms() != first.session_expires_at_unix_ms()
    {
        return Err(Error::Configuration);
    }
    let lease = invitation.lease();
    if leases.contains_key(&lease.lease_id)
        || leases.values().any(|entry| entry.value.peer == lease.peer)
    {
        return Err(Error::Replay);
    }
    let reserved = leases
        .values()
        .filter(|entry| {
            !entry.cancel.is_cancelled()
                && (entry.consumed.load(Ordering::Acquire)
                    || entry
                        .invitation
                        .as_ref()
                        .is_some_and(|authority| authority.check_admission().is_ok()))
        })
        .count();
    if leases.len() >= 128 || reserved >= limits.max_peers {
        return Err(Error::Limit);
    }
    invitation.check_admission_current()?;
    leases.insert(
        lease.lease_id.clone(),
        Arc::new(LeaseState::new(
            lease,
            Some(InvitationAuthority::Session(invitation.clone())),
            services,
            limits,
        )),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        host: SecretKey,
        deadlines: SessionDeadlines,
        services: HashMap<String, (u32, Service)>,
        limits: Limits,
    }
    impl Fixture {
        fn new() -> Self {
            let service = Service {
                service_id: "game".into(),
                protocol: Protocol::Tcp,
                endpoint: "127.0.0.1:41200".parse().unwrap(),
                revision: 1,
                max_datagram_bytes: 1400,
                required_capability: Capability::ReliableStream,
                udp_return_policy: UdpReturnPolicy::ReplyBudget,
            };
            let now = unix_millis();
            Self {
                host: generate_key(),
                deadlines: SessionDeadlines {
                    admission_expires_at_unix_ms: now + 20_000,
                    session_expires_at_unix_ms: now + 7_200_000,
                },
                services: HashMap::from([("game".into(), (1, service))]),
                limits: Limits::default(),
            }
        }
        fn issue(
            &self,
            peer: EndpointId,
            session: &str,
            deadlines: SessionDeadlines,
        ) -> SessionInvitation {
            SessionInvitation::issue_online(
                &self.host,
                peer,
                session,
                std::slice::from_ref(&self.services["game"].1),
                deadlines,
            )
            .unwrap()
        }
        fn admit(
            &self,
            leases: &mut HashMap<String, Arc<LeaseState>>,
            first: &SessionInvitation,
            invitation: &SessionInvitation,
        ) -> Result<()> {
            insert_admission(leases, first, invitation, &self.services, &self.limits)
        }
    }

    #[test]
    fn online_registry_reserves_eight_unique_peers_and_keeps_consumption_and_revocation() {
        let fixture = Fixture::new();
        let first = fixture.issue(generate_key().public(), "online-session", fixture.deadlines);
        let mut leases = HashMap::new();
        fixture.admit(&mut leases, &first, &first).unwrap();
        let original = Arc::clone(&leases[first.invitation_id()]);
        original.consumed.store(true, Ordering::Release);
        original.cancel.cancel();
        assert_eq!(
            fixture.admit(&mut leases, &first, &first),
            Err(Error::Replay)
        );
        let reissued = fixture.issue(first.guest_key(), "online-session", fixture.deadlines);
        assert_eq!(
            fixture.admit(&mut leases, &first, &reissued),
            Err(Error::Replay)
        );
        for _ in 1..8 {
            let invitation =
                fixture.issue(generate_key().public(), "online-session", fixture.deadlines);
            fixture.admit(&mut leases, &first, &invitation).unwrap();
            let added = &leases[invitation.invitation_id()];
            assert!(added.invitation.as_ref().unwrap().proof() == invitation.proof());
            assert_eq!(
                added.value.expires_at_unix_ms,
                first.session_expires_at_unix_ms()
            );
            assert!(!added.consumed.load(Ordering::Acquire));
        }
        let overflow = fixture.issue(generate_key().public(), "online-session", fixture.deadlines);
        fixture.admit(&mut leases, &first, &overflow).unwrap();
        let ninth_active =
            fixture.issue(generate_key().public(), "online-session", fixture.deadlines);
        assert_eq!(
            fixture.admit(&mut leases, &first, &ninth_active),
            Err(Error::Limit)
        );
        assert_eq!(leases.len(), 9);
        assert!(Arc::ptr_eq(&original, &leases[first.invitation_id()]));
        assert!(original.cancel.is_cancelled());
        assert!(original.consumed.load(Ordering::Acquire));
    }

    #[test]
    fn online_registry_keeps_128_tombstones_without_reusing_revoked_authority() {
        let fixture = Fixture::new();
        let first = fixture.issue(generate_key().public(), "online-session", fixture.deadlines);
        let mut leases = HashMap::new();
        for index in 0..128 {
            let invitation = if index == 0 {
                first.clone()
            } else {
                fixture.issue(generate_key().public(), "online-session", fixture.deadlines)
            };
            fixture.admit(&mut leases, &first, &invitation).unwrap();
            leases[invitation.invitation_id()].cancel.cancel();
        }
        let overflow = fixture.issue(generate_key().public(), "online-session", fixture.deadlines);
        assert_eq!(
            fixture.admit(&mut leases, &first, &overflow),
            Err(Error::Limit)
        );
        assert_eq!(
            fixture.admit(&mut leases, &first, &first),
            Err(Error::Replay)
        );
        assert_eq!(leases.len(), 128);
    }

    #[test]
    fn online_registry_refuses_another_host_session_deadline_or_old_invitation_schema() {
        let fixture = Fixture::new();
        let first = fixture.issue(generate_key().public(), "online-session", fixture.deadlines);
        let peer = generate_key().public();
        let mut leases = HashMap::new();
        let other_host = Fixture::new().issue(peer, "online-session", fixture.deadlines);
        let other_session = fixture.issue(peer, "other-session", fixture.deadlines);
        let extended = fixture.issue(
            peer,
            "online-session",
            SessionDeadlines {
                session_expires_at_unix_ms: fixture.deadlines.session_expires_at_unix_ms + 1000,
                ..fixture.deadlines
            },
        );
        let old = SessionInvitation::issue_loopback_for_lifetimes(
            &fixture.host,
            peer,
            "online-session",
            "127.0.0.1:41201".parse().unwrap(),
            std::slice::from_ref(&fixture.services["game"].1),
            Duration::from_secs(20),
            Duration::from_secs(60),
        )
        .unwrap();
        for invitation in [other_host, other_session, extended, old] {
            assert_eq!(
                fixture.admit(&mut leases, &first, &invitation),
                Err(Error::Configuration)
            );
        }
        assert!(leases.is_empty());
    }
}
