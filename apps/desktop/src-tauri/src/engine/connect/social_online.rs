//! Online participants share the original social Attempt, source and Host.
use super::*;
use std::collections::{HashMap, HashSet};
use std::future::Future;
#[cfg(all(windows, feature = "connect-voice-lab"))]
#[path = "social_online_voice.rs"]
pub(super) mod voice;

fn maintenance_interval() -> tokio::time::Interval {
    let mut tick = interval(Duration::from_millis(20));
    // A stalled executor owes one current validation, not every historical
    // tick. Replaying that backlog can starve an already available reply.
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tick
}

async fn wait_host_poll(
    run: &Run,
    group: &mut Group,
    host: &Option<Arc<lab::Host>>,
    #[cfg(all(windows, feature = "connect-voice-lab"))] local: &mut voice::LocalPlayer,
    tick: &mut tokio::time::Interval,
    querying: impl Future<Output = Result<Value, ConnectError>>,
    querying_started: Instant,
) -> Result<Option<Value>, ConnectError> {
    tokio::pin!(querying);
    let response = loop {
        tokio::select! { biased;
            _ = run.cancel.cancelled() => return Ok(None),
            result = &mut querying => {
                run.observe_host_poll(querying_started.elapsed(), None);
                break result?;
            },
            _ = tick.tick() => {
                run.observe_host_poll(querying_started.elapsed(), None);
                group.monitor(run, host).await?;
                #[cfg(all(windows, feature = "connect-voice-lab"))]
                local.tick(run).await?;
            },
        }
    };
    // Reply priority never authorizes stale work: all original current/lease
    // checks still run before the caller observes or applies this response.
    group.monitor(run, host).await?;
    Ok(Some(response))
}

struct Participant {
    grant: lab::VerifiedSocialGrant,
    continuity: lab::SocialContinuity,
    invitation: lab::SessionInvitation,
    admitted: bool,
    terminal: bool,
}
struct PendingDelivery {
    grant: lab::VerifiedSocialGrant,
    continuity: lab::SocialContinuity,
}
struct PollInbox<'a> {
    run: &'a Run,
    challenge: &'a lab::SocialKeyChallenge,
    policy: &'a lab::SignedSocialHostPolicy,
    source: SocketAddr,
    maximum: usize,
    pending: HashMap<String, Option<PendingDelivery>>,
}
fn deliveries(response: &Value) -> Result<&[Value], ConnectError> {
    if field(response, "state")? != "group" {
        return Err(ConnectError::InvitationRejected);
    }
    response["deliveries"]
        .as_array()
        .filter(|items| items.len() <= 8)
        .map(Vec::as_slice)
        .ok_or(ConnectError::InvitationRejected)
}
fn refresh_delivery(
    continuity: &mut lab::SocialContinuity,
    value: &Value,
) -> Result<bool, ConnectError> {
    if value["lease"]["body"]["sequence"]
        .as_u64()
        .is_some_and(|sequence| sequence > continuity.sequence())
        && continuity.update(&packet(value, "lease")?).is_err()
    {
        return Ok(false);
    }
    Ok(continuity.check().is_ok())
}
impl PollInbox<'_> {
    fn observe(&mut self, response: &Value) -> Result<(), ConnectError> {
        self.run.current()?;
        let mut seen = HashSet::new();
        for row in deliveries(response)? {
            let id = field(&row["grant"]["body"], "grantId")?;
            if id.len() != 64
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || !seen.insert(id)
            {
                return Err(ConnectError::InvitationRejected);
            }
            if let Some(entry) = self.pending.get_mut(id) {
                if let Some(pending) = entry {
                    if pending.grant.check_admission().is_err()
                        || !refresh_delivery(&mut pending.continuity, row)?
                    {
                        *entry = None;
                    }
                }
                continue;
            }
            if self.pending.len() >= self.maximum {
                return Err(ConnectError::InvitationRejected);
            }
            let grant = packet(row, "grant")?;
            let grant = lab::VerifiedSocialGrant::for_host_with_clock(
                &grant,
                self.run.authority.pin,
                self.challenge,
                self.policy,
                self.source,
                self.run.deadline,
                self.run.clock.as_ref(),
            );
            let pending = if let Ok(grant) = grant {
                let initial = packet(row, "initialLease")?;
                match lab::SocialContinuity::start(&initial, &grant) {
                    Ok(mut continuity) => (grant.check_admission().is_ok()
                        && refresh_delivery(&mut continuity, row)?)
                    .then_some(PendingDelivery { grant, continuity }),
                    _ => None,
                }
            } else {
                None
            };
            self.pending.insert(id.into(), pending);
        }
        for (id, pending) in &mut self.pending {
            if !seen.contains(id.as_str()) {
                *pending = None;
            }
        }
        Ok(())
    }
    fn take(&mut self, id: &str) -> Option<PendingDelivery> {
        self.pending.get_mut(id)?.take()
    }
}

/// Drive a serial heartbeat inside the original Host future while its endpoint
/// or local bridges initialize. No presence is projected by these polls.
/// If the pulse fails, cancel and collect any work that may own a resource.
async fn poll_while_starting<T>(
    run: &Run,
    http: &Http,
    policy_id: &str,
    inbox: &mut PollInbox<'_>,
    work: impl Future<Output = Result<T, ConnectError>>,
) -> Result<T, ConnectError> {
    let mut ticks = interval(Duration::from_millis(500));
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    tokio::pin!(work);
    loop {
        tokio::select! { biased;
            _ = ticks.tick() => {},
            result = &mut work => {
                let value = result?;
                run.current()?;
                return Ok(value);
            },
        }
        let poll = http.post(SocialRoute::HostPoll, json!({"policyId":policy_id}));
        tokio::pin!(poll);
        let (reply, done) = tokio::select! { biased;
            reply = &mut poll => (reply, None),
            result = &mut work => (poll.await, Some(result?)),
        };
        if let Err(error) = reply.and_then(|value| inbox.observe(&value)) {
            run.cancel.cancel();
            if done.is_none() {
                let _ = work.await;
            }
            return Err(error);
        }
        if let Some(value) = done {
            run.current()?;
            return Ok(value);
        }
    }
}

struct Group {
    participants: HashMap<String, Participant>,
    rejected: HashSet<String>,
    backend_revoked: HashSet<String>,
    removed_accounts: HashSet<String>,
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    voice: Option<voice::VoiceMonitor>,
}
impl Group {
    fn new(run: &Run) -> Self {
        let _ = run;
        Self {
            participants: HashMap::new(),
            rejected: HashSet::new(),
            backend_revoked: HashSet::new(),
            removed_accounts: HashSet::new(),
            #[cfg(all(windows, feature = "connect-voice-lab"))]
            voice: run
                .native_voice_source
                .clone()
                .map(voice::VoiceMonitor::new),
        }
    }
    fn voice_enabled(&self) -> bool {
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        return self.voice.is_some();
        #[cfg(not(all(windows, feature = "connect-voice-lab")))]
        false
    }
    fn services(&self, source: SocketAddr) -> Result<Vec<lab::Service>, ConnectError> {
        #[allow(unused_mut)]
        let mut services = vec![game(source)];
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        if let Some(voice) = &self.voice {
            if source != voice.source().targets().0 {
                return Err(ConnectError::SourceChanged);
            }
            services.push(voice::service(voice.source()));
        }
        Ok(services)
    }
    async fn monitor(
        &mut self,
        run: &Run,
        host: &Option<Arc<lab::Host>>,
    ) -> Result<(), ConnectError> {
        run.current()?;
        let Some(active) = host else {
            #[cfg(all(windows, feature = "connect-voice-lab"))]
            if let Some(voice) = &self.voice {
                voice
                    .source()
                    .check_current()
                    .map_err(|_| ConnectError::SourceChanged)?;
            }
            return Ok(());
        };
        let mut connected = false;
        let revocations = run.requested_revocations()?;
        for (grant_id, participant) in &self.participants {
            if revocations.contains(grant_id) {
                self.removed_accounts
                    .insert(participant.grant.guest_account_id().into());
            }
        }
        for (grant_id, participant) in &mut self.participants {
            if participant.terminal {
                continue;
            }
            let id = participant.invitation.invitation_id();
            participant.admitted |= active.invitation_consumed(id);
            if revocations.contains(grant_id)
                || self
                    .removed_accounts
                    .contains(participant.grant.guest_account_id())
                || participant.continuity.check().is_err()
                || participant.grant.check_session().is_err()
                || (!participant.admitted && participant.grant.check_admission().is_err())
            {
                active.revoke(id);
                participant.terminal = true;
            } else if participant.admitted && active.invitation_active(id) {
                if active
                    .refresh_social_idle(&participant.invitation, &participant.continuity)
                    .is_err()
                {
                    active.revoke(id);
                    participant.terminal = true;
                } else {
                    connected = true;
                }
            }
        }
        let mut roster: Vec<SocialParticipant> = self
            .participants
            .iter()
            .filter(|(_, participant)| !participant.terminal)
            .map(|(id, participant)| SocialParticipant {
                grant_id: id.clone(),
                account_id: participant.grant.guest_account_id().into(),
                state: if participant.admitted {
                    if active.invitation_active(participant.invitation.invitation_id()) {
                        "connected"
                    } else {
                        "disconnected"
                    }
                } else {
                    "waiting"
                },
            })
            .collect();
        roster.sort_by(|a, b| a.grant_id.cmp(&b.grant_id));
        let changed = lock(&run.state)?
            .attempt
            .as_ref()
            .and_then(|a| a.snapshot.social.as_ref())
            .is_some_and(|social| social.participants.as_ref() != Some(&roster));
        if changed {
            update(&run.state, &run.id, |a| {
                if let Some(social) = &mut a.snapshot.social {
                    social.participants = Some(roster);
                }
            });
        }
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        if let Some(voice) = &mut self.voice {
            voice.tick(run, host).await?;
            let state = voice.state();
            let changed = lock(&run.state)?
                .attempt
                .as_ref()
                .and_then(|a| a.snapshot.social.as_ref())
                .is_some_and(|social| social.voice_state != Some(state));
            if changed {
                update(&run.state, &run.id, |a| {
                    if let Some(social) = &mut a.snapshot.social {
                        social.voice_state = Some(state);
                    }
                });
            }
        }
        run.stage(
            if connected {
                "connected"
            } else {
                "waitingGuest"
            },
            "hostListening",
        );
        Ok(())
    }
    fn current_grant(&self, id: &str) -> Result<(), ConnectError> {
        let participant = self
            .participants
            .get(id)
            .filter(|p| !p.terminal)
            .ok_or(ConnectError::PlanExpired)?;
        participant
            .grant
            .check_admission()
            .and_then(|_| participant.grant.check_session())
            .and_then(|_| participant.continuity.check())
            .map_err(|_| ConnectError::PlanExpired)
    }

    fn accept_invitation(
        &mut self,
        id: &str,
        candidate: Result<lab::SessionInvitation, ConnectError>,
    ) -> Result<Option<lab::SessionInvitation>, ConnectError> {
        match candidate {
            Ok(invitation) => Ok(Some(invitation)),
            Err(ConnectError::InvitationRejected) => {
                self.rejected.insert(id.into());
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
}

async fn check_source_online(
    run: &Run,
    source: &TargetGuard,
    host: &mut Option<Arc<lab::Host>>,
    group: &mut Group,
    permitted: impl Fn() -> Result<(), ConnectError>,
) -> Result<(), ConnectError> {
    group.monitor(run, host).await?;
    permitted()?;
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    if host.is_none() {
        if let Some(voice) = &group.voice {
            voice::probe(run, voice.source(), None).await?;
            return permitted();
        }
    }
    if group.voice_enabled() || matches!(source, TargetGuard::Manual) {
        return Ok(());
    }
    let source = source.clone();
    let mut query = tokio::task::spawn_blocking(move || source.recheck().map(|_| ()));
    let mut tick = maintenance_interval();
    let failure = loop {
        tokio::select! { biased;
            _ = run.cancel.cancelled() => break ConnectError::Cancelled,
            _ = tick.tick() => {
                if let Err(error) = group.monitor(run, host).await.and_then(|_| permitted()) { break error; }
            },
            result = &mut query => {
                group.monitor(run, host).await?; permitted()?;
                return result.unwrap_or(Err(ConnectError::Internal));
            },
        }
    };
    if let Some(host) = host.take() {
        host.shutdown().await;
    }
    let _ = query.await;
    Err(failure)
}

pub(super) async fn ticket(
    run: &Run,
    http: &Http,
    role: &str,
    id: &str,
    host_key: lab::EndpointId,
) -> Result<lab::OnlineNetworkTicket, ConnectError> {
    let value = http
        .post(SocialRoute::RelayTicket, json!({"role":role,"id":id}))
        .await?;
    let ticket = lab::OnlineNetworkTicket::verify_with_clock(
        &value.to_string(),
        run.authority.pin,
        &run.authority.issuer,
        &run.request.account_id,
        &run.request.session_id,
        run.key.public(),
        host_key,
        Some(run.clock()?),
    )
    .map_err(|_| ConnectError::InvitationRejected)?;
    if ticket.generation() != run.generation {
        return Err(ConnectError::InvitationRejected);
    }
    run.current()?;
    Ok(ticket)
}

fn issue(
    run: &Run,
    key: lab::EndpointId,
    services: &[lab::Service],
    deadlines: lab::SessionDeadlines,
    voice: bool,
) -> Result<lab::SessionInvitation, ConnectError> {
    if voice {
        lab::SessionInvitation::issue_online_native_voice_with_clock(
            &run.key,
            key,
            &run.request.session_id,
            services,
            deadlines,
            run.clock()?,
        )
    } else {
        lab::SessionInvitation::issue_online_with_clock(
            &run.key,
            key,
            &run.request.session_id,
            services,
            deadlines,
            false,
            run.clock()?,
        )
    }
    .map_err(|_| ConnectError::InvitationRejected)
}
#[allow(clippy::too_many_arguments)] // Mutates only the original actor-owned Host and its socket ledger.
async fn start_host(
    run: &Run,
    source_guard: &TargetGuard,
    source: SocketAddr,
    group: &mut Group,
    host: &mut Option<Arc<lab::Host>>,
    sockets: &mut Vec<SocketAddr>,
    invitation: &lab::SessionInvitation,
    ticket: &lab::OnlineNetworkTicket,
) -> Result<(), ConnectError> {
    run.current()?;
    invitation
        .check_admission_current()
        .map_err(|_| ConnectError::PlanExpired)?;
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    if let Some(voice) = &group.voice {
        voice::probe(run, voice.source(), None).await?;
    }
    if !group.voice_enabled() {
        check_source_online(run, source_guard, host, group, || Ok(())).await?;
    }
    let config = lab::HostConfig {
        secret_key: run.key.clone(),
        bind_addr: "0.0.0.0:0".parse().expect("native online bind"),
        session_id: run.request.session_id.clone(),
        services: group.services(source)?,
        leases: vec![invitation.lease()],
        limits: if group.voice_enabled() {
            native_voice_limits()
        } else {
            limits()
        },
    };
    let voice_enabled = group.voice_enabled();
    let starting = async {
        if voice_enabled {
            lab::Host::start_invited_online_native_voice(config, invitation, ticket).await
        } else {
            lab::Host::start_invited_online(config, invitation, ticket).await
        }
    };
    tokio::pin!(starting);
    let created = tokio::select! { biased;
        _ = run.cancel.cancelled() => starting.as_mut().await,
        result = &mut starting => result,
    }
    .map_err(|_| ConnectError::TransportFailed)?;
    *sockets = created.bound_sockets();
    *host = Some(Arc::new(created));
    run.current()?;
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    if let Some(voice) = &mut group.voice {
        voice::probe(run, voice.source(), host.as_ref()).await?;
        voice
            .register_lease(host.as_ref().unwrap(), invitation)
            .await?;
        return Ok(());
    }
    check_source_online(run, source_guard, host, group, || {
        invitation
            .check_admission_current()
            .map_err(|_| ConnectError::PlanExpired)
    })
    .await
}

pub(super) async fn host_registered(
    run: &Run,
    http: &Http,
    challenge: &lab::SocialKeyChallenge,
    policy: &lab::SignedSocialHostPolicy,
    source: SocketAddr,
    source_guard: &TargetGuard,
    policy_id: &str,
) -> Result<(), ConnectError> {
    let mut host: Option<Arc<lab::Host>> = None;
    let mut group = Group::new(run);
    if group.voice_enabled() != (policy.body().voice_transport_profile() == Some("nativeVoice8V1"))
    {
        return Err(ConnectError::InvitationRejected);
    }
    let mut sockets = Vec::new();
    let mut revoke_job: Option<(String, JoinHandle<Result<Value, ConnectError>>)> = None;
    let revoke_cancel = run.cancel.child_token();
    #[allow(unused_mut)]
    let mut maximum = 128;
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    if run.native_voice_local_key.is_some() {
        maximum = 127;
    }
    let mut inbox = PollInbox {
        run,
        challenge,
        policy,
        source,
        maximum,
        pending: HashMap::new(),
    };
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    let mut local = voice::LocalPlayer::default();
    let result = async {
        let ticket = poll_while_starting(run, http, policy_id, &mut inbox, async {
            let ticket = ticket(run, http, "host", policy_id, run.key.public()).await?;
            #[cfg(all(windows, feature = "connect-voice-lab"))]
            if let Some(key) = &run.native_voice_local_key {
                if !group.voice_enabled() { return Err(ConnectError::InvitationRejected); }
                let expiry = policy.body().expires_at_unix_ms().min(run.expiry);
                let invitation = issue(run, key.public(), &group.services(source)?, lab::SessionDeadlines {
                    admission_expires_at_unix_ms: expiry.min(run.server_issue_time().saturating_add(60_000)), session_expires_at_unix_ms: expiry }, true)?;
                start_host(run, source_guard, source, &mut group, &mut host, &mut sockets, &invitation, &ticket).await?;
                local.start(run, host.as_ref().unwrap(), group.voice.as_mut().unwrap(), key, &invitation, policy, &ticket).await?;
            }
            Ok(ticket)
        }).await?;
        let mut poll = interval(Duration::from_millis(500));
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut tick = maintenance_interval();
        let mut checked_source = Instant::now();
        loop {
            if revoke_job.as_ref().is_some_and(|(_, job)| job.is_finished()) {
                let (id, job) = revoke_job.take().unwrap();
                if job.await.is_ok_and(|result| result.is_ok_and(|value| value["revoked"] == true)) { group.backend_revoked.insert(id); }
            }
            #[cfg(all(windows, feature = "connect-voice-lab"))]
            local.tick(run).await?;
            tokio::select! { biased;
                _ = run.cancel.cancelled() => return Ok(()),
                _ = poll.tick() => {
                    group.monitor(run, &host).await?;
                    // Local revoke already happened. Keep one retained backend
                    // decrease in flight, without delaying ordinary host polls.
                    if revoke_job.is_none() {
                        if let Some(id) = run.requested_revocations()?.into_iter().find(|id| !group.backend_revoked.contains(id)) {
                            let mut client = http.clone(); client.cancel = revoke_cancel.clone(); let grant_id = id.clone();
                            revoke_job = Some((id, tokio::spawn(async move { client.post(SocialRoute::HostRevoke, json!({"grantId":grant_id})).await })));
                        }
                    }
                    if checked_source.elapsed() >= Duration::from_secs(1) {
                        check_source_online(run, source_guard, &mut host, &mut group, || run.current()).await?;
                        checked_source = Instant::now();
                    }
                    let connected = group.participants.iter().filter(|(_, participant)| !participant.terminal
                        && participant.admitted && participant.continuity.check().is_ok() && participant.grant.check_session().is_ok()
                        && host.as_ref().is_some_and(|host| host.invitation_active(participant.invitation.invitation_id())))
                        .map(|(id, _)| id.clone()).collect::<Vec<_>>();
                    #[cfg(all(windows, feature = "connect-voice-lab"))]
                    let host_player_connected = local.transport_connected();
                    #[cfg(not(all(windows, feature = "connect-voice-lab")))]
                    let host_player_connected = false;
                    let querying_started = Instant::now();
                    run.observe_host_poll(Duration::ZERO, None);
                    let querying = http.post(SocialRoute::HostPoll, json!({"policyId":policy_id, "transport": {
                        "observedAtUnixMs":run.server_issue_time(), "connectedGrantIds":connected, "hostPlayerConnected":host_player_connected
                    }}));
                    let Some(response) = wait_host_poll(run, &mut group, &host,
                        #[cfg(all(windows, feature = "connect-voice-lab"))] &mut local,
                        &mut tick, querying, querying_started).await? else { return Ok(()); };
                    inbox.observe(&response)?;
                    #[cfg(all(windows, feature = "connect-voice-lab"))]
                    local.refresh_poll(run, &response)?;
                    run.observe_host_poll(querying_started.elapsed(), response["checkedAtUnixMs"].as_u64());
                    let deliveries = deliveries(&response)?;
                    for (id, participant) in &mut group.participants {
                        if !participant.terminal && !deliveries.iter().any(|item| item["grant"]["body"]["grantId"].as_str() == Some(id.as_str())) {
                            if let Some(host) = &host { host.revoke(participant.invitation.invitation_id()); }
                            participant.terminal = true;
                        }
                    }
                    for delivery in deliveries {
                        group.monitor(run, &host).await?;
                        let id = field(&delivery["grant"]["body"], "grantId")?.to_owned();
                        if group.rejected.contains(&id) { continue; }
                        if let Some(existing) = group.participants.get_mut(&id) {
                            if !existing.terminal && !refresh_delivery(&mut existing.continuity, delivery)? {
                                if let Some(host) = &host { host.revoke(existing.invitation.invitation_id()); } existing.terminal = true;
                            }
                            continue;
                        }
                        let Some(Some(pending)) = inbox.pending.get(&id) else { continue; };
                        let grant = &pending.grant;
                        if group.removed_accounts.contains(grant.guest_account_id())
                            || grant.check_admission().and_then(|_| pending.continuity.check()).is_err() {
                            inbox.take(&id);
                            group.rejected.insert(id);
                            continue;
                        }
                        #[cfg(all(windows, feature = "connect-voice-lab"))]
                        if let Some(voice) = &group.voice {
                            let (profile, digest) = voice.source().online_voice_policy().map_err(|_| ConnectError::SourceChanged)?;
                            let runtime = voice.source().online_runtime_profile().map_err(|_| ConnectError::SourceChanged)?;
                            if grant.voice_profile() != Some(profile) || grant.voice_source_digest() != Some(digest.as_str()) || grant.host_player() != run.native_voice_local_key.is_some() || grant.voice_transport_profile() != Some("nativeVoice8V1") {
                                return Err(ConnectError::InvitationRejected);
                            }
                            if grant.runtime_profile().unwrap_or("fabric-loader-0.16.14-1.21.1") != runtime.version_id() { return Err(ConnectError::InvitationRejected); }
                        }
                        if grant.voice_profile().is_some() != group.voice_enabled() { return Err(ConnectError::InvitationRejected); }
                        let candidate = issue(run, grant.guest_key(), &group.services(source)?, lab::SessionDeadlines {
                            admission_expires_at_unix_ms: grant.admission_expires_at_unix_ms(), session_expires_at_unix_ms: grant.session_expires_at_unix_ms() }, group.voice_enabled());
                        let Some(invitation) = group.accept_invitation(&id, candidate)? else {
                            inbox.take(&id);
                            continue;
                        };
                        if let Some(host) = &host {
                            if host.admit_online(&invitation).is_err() {
                                inbox.take(&id);
                                group.rejected.insert(id);
                                continue;
                            }
                            #[cfg(all(windows, feature = "connect-voice-lab"))]
                            if let Some(voice) = &mut group.voice { voice.register_lease(host, &invitation).await?; }
                        } else {
                            poll_while_starting(run, http, policy_id, &mut inbox,
                                start_host(run, source_guard, source, &mut group, &mut host, &mut sockets, &invitation, &ticket),
                            ).await?;
                        }
                        let Some(PendingDelivery { grant, continuity }) = inbox.take(&id) else {
                            if let Some(host) = &host { host.revoke(invitation.invitation_id()); }
                            continue;
                        };
                        group.participants.insert(id.clone(), Participant { grant, continuity, invitation: invitation.clone(), admitted: false, terminal: false });
                        group.monitor(run, &host).await?;
                        if group.current_grant(&id).is_err() { continue; }
                        let publishing = http.post(SocialRoute::HostPublish, json!({"grantId":id,"invitation":invitation.encode()})); tokio::pin!(publishing);
                        let published = loop { tokio::select! { biased;
                            _ = run.cancel.cancelled() => return Ok(()),
                            _ = tick.tick() => { group.monitor(run, &host).await?; if group.current_grant(&id).is_err() { break None; } #[cfg(all(windows, feature = "connect-voice-lab"))] local.tick(run).await?; },
                            result = &mut publishing => break Some(result),
                        }};
                        group.monitor(run, &host).await?;
                        if group.current_grant(&id).is_err() { continue; }
                        let Some(published) = published else { continue; };
                        if !published.is_ok_and(|value| value["accepted"] == true) {
                            if let Some(participant) = group.participants.get_mut(&id) {
                                if let Some(host) = &host { host.revoke(participant.invitation.invitation_id()); }
                                participant.terminal = true;
                            }
                        }
                    }
                },
                _ = tick.tick() => { group.monitor(run, &host).await?; },
            }
        }
    }.await;
    if let Err(error) = result {
        run.observe_failure(match error {
            ConnectError::PlanExpired => SocialFailureGate::Session,
            ConnectError::SourceChanged => SocialFailureGate::Source,
            ConnectError::SocialFailure(SocialFailureStage::SessionContinuity, _) => {
                SocialFailureGate::HostPoll
            }
            ConnectError::SocialFailure(SocialFailureStage::Relay, _) => {
                SocialFailureGate::RelayTicket
            }
            _ => SocialFailureGate::Other,
        });
    }
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    local.invalidate(run);
    if let Some(host) = host {
        for participant in group.participants.values() {
            host.revoke(participant.invitation.invitation_id());
        }
        host.shutdown().await;
        drop(host);
    }
    revoke_cancel.cancel();
    if let Some((_, job)) = revoke_job {
        let _ = job.await;
    }
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    let local_cleanup = {
        let cleanup = local.shutdown(run).await;
        if let Some(voice) = &mut group.voice {
            voice.finish().await;
        }
        cleanup
    };
    update(&run.state, &run.id, |a| {
        a.snapshot.tcp_address = None;
        if let Some(social) = &mut a.snapshot.social {
            social.participants = Some(Vec::new());
            if social.voice_profile.is_some() {
                social.voice_state = Some("closed");
            }
        }
    });
    release(&sockets, None).await?;
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    local_cleanup?;
    result
}

#[cfg(test)]
#[path = "social_online_tests.rs"]
mod tests;
