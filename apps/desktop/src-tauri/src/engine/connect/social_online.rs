//! Online participants share the original social Attempt, source and Host.
use super::*;
use std::collections::{HashMap, HashSet};
#[cfg(all(windows, feature = "connect-voice-lab"))]
#[path = "social_online_voice.rs"]
pub(super) mod voice;

struct Participant {
    grant: lab::VerifiedSocialGrant,
    continuity: lab::SocialContinuity,
    invitation: lab::SessionInvitation,
    admitted: bool,
    terminal: bool,
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
            } else {
                connected |= participant.admitted && active.invitation_active(id);
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
    if group.voice_enabled() || matches!(source, TargetGuard::Manual) {
        return Ok(());
    }
    let source = source.clone();
    let mut query = tokio::task::spawn_blocking(move || source.recheck().map(|_| ()));
    let mut tick = interval(Duration::from_millis(20));
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
        .post("/v1/social/relay/ticket", json!({"role":role,"id":id}))
        .await?;
    let ticket = lab::OnlineNetworkTicket::verify(
        &value.to_string(),
        run.authority.pin,
        &run.authority.issuer,
        &run.request.account_id,
        &run.request.session_id,
        run.key.public(),
        host_key,
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
        lab::SessionInvitation::issue_online_fragmented(
            &run.key,
            key,
            &run.request.session_id,
            services,
            deadlines,
        )
    } else {
        lab::SessionInvitation::issue_online(
            &run.key,
            key,
            &run.request.session_id,
            services,
            deadlines,
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
        limits: limits(),
    };
    let voice_enabled = group.voice_enabled();
    let starting = async {
        if voice_enabled {
            lab::Host::start_invited_online_fragmented(config, invitation, ticket).await
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
    let mut sockets = Vec::new();
    let mut revoke_job: Option<(String, JoinHandle<Result<Value, ConnectError>>)> = None;
    let revoke_cancel = run.cancel.child_token();
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    let mut local = voice::LocalPlayer::default();
    let result = async {
        let ticket = ticket(run, http, "host", policy_id, run.key.public()).await?;
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        if let Some(key) = &run.native_voice_local_key {
            if !group.voice_enabled() { return Err(ConnectError::InvitationRejected); }
            let expiry = policy.body().expires_at_unix_ms().min(run.expiry);
            let invitation = issue(run, key.public(), &group.services(source)?, lab::SessionDeadlines {
                admission_expires_at_unix_ms: expiry.min(lab::unix_millis() + 60_000), session_expires_at_unix_ms: expiry }, true)?;
            start_host(run, source_guard, source, &mut group, &mut host, &mut sockets, &invitation, &ticket).await?;
            local.start(run, host.as_ref().unwrap(), group.voice.as_mut().unwrap(), key, &invitation, policy, &ticket).await?;
        }
        let mut poll = interval(Duration::from_millis(500));
        let mut tick = interval(Duration::from_millis(20));
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
                _ = tick.tick() => { group.monitor(run, &host).await?; },
                _ = poll.tick() => {
                    group.monitor(run, &host).await?;
                    // Local revoke already happened. Keep one retained backend
                    // decrease in flight, without delaying ordinary host polls.
                    if revoke_job.is_none() {
                        if let Some(id) = run.requested_revocations()?.into_iter().find(|id| !group.backend_revoked.contains(id)) {
                            let mut client = http.clone(); client.cancel = revoke_cancel.clone(); let grant_id = id.clone();
                            revoke_job = Some((id, tokio::spawn(async move { client.post("/v1/social/host/revoke", json!({"grantId":grant_id})).await })));
                        }
                    }
                    if checked_source.elapsed() >= Duration::from_secs(1) {
                        check_source_online(run, source_guard, &mut host, &mut group, || run.current()).await?;
                        checked_source = Instant::now();
                    }
                    let querying = http.post("/v1/social/host/poll", json!({"policyId":policy_id})); tokio::pin!(querying);
                    let response = loop { tokio::select! { biased;
                        _ = run.cancel.cancelled() => return Ok(()),
                        _ = tick.tick() => { group.monitor(run, &host).await?; #[cfg(all(windows, feature = "connect-voice-lab"))] local.tick(run).await?; },
                        result = &mut querying => break result?,
                    }};
                    group.monitor(run, &host).await?;
                    if field(&response, "state")? != "group" { return Err(ConnectError::InvitationRejected); }
                    #[cfg(all(windows, feature = "connect-voice-lab"))]
                    local.refresh_poll(run, &response)?;
                    let deliveries = response["deliveries"].as_array().filter(|items| items.len() <= 8).ok_or(ConnectError::InvitationRejected)?;
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
                            if !existing.terminal && delivery["lease"]["body"]["sequence"].as_u64().is_some_and(|value| value > existing.continuity.sequence())
                                && existing.continuity.update(&packet(delivery, "lease")?).is_err() {
                                if let Some(host) = &host { host.revoke(existing.invitation.invitation_id()); } existing.terminal = true;
                            }
                            continue;
                        }
                        #[allow(unused_mut)]
                        let mut maximum = 128;
                        #[cfg(all(windows, feature = "connect-voice-lab"))]
                        if run.native_voice_local_key.is_some() { maximum = 127; }
                        if group.participants.len() + group.rejected.len() >= maximum { return Err(ConnectError::InvitationRejected); }
                        let grant = match lab::VerifiedSocialGrant::for_host(&packet(delivery, "grant")?, run.authority.pin,
                            challenge, policy, source, run.deadline) { Ok(grant) => grant, Err(_) => { group.rejected.insert(id); continue; } };
                        if group.removed_accounts.contains(grant.guest_account_id()) { group.rejected.insert(id); continue; }
                        let mut continuity = match lab::SocialContinuity::start(&packet(delivery, "initialLease")?, &grant) { Ok(continuity) => continuity, Err(_) => { group.rejected.insert(id); continue; } };
                        if delivery["lease"]["body"]["sequence"].as_u64().is_some_and(|sequence| sequence > continuity.sequence())
                            && continuity.update(&packet(delivery, "lease")?).is_err() { group.rejected.insert(id); continue; }
                        #[cfg(all(windows, feature = "connect-voice-lab"))]
                        if let Some(voice) = &group.voice {
                            let (profile, digest) = voice.source().online_voice_policy().map_err(|_| ConnectError::SourceChanged)?;
                            let runtime = voice.source().online_runtime_profile().map_err(|_| ConnectError::SourceChanged)?;
                            if grant.voice_profile() != Some(profile) || grant.voice_source_digest() != Some(digest.as_str()) || grant.host_player() != run.native_voice_local_key.is_some() {
                                return Err(ConnectError::InvitationRejected);
                            }
                            if grant.runtime_profile().unwrap_or("fabric-loader-0.16.14-1.21.1") != runtime.version_id() { return Err(ConnectError::InvitationRejected); }
                        }
                        if grant.voice_profile().is_some() != group.voice_enabled() { return Err(ConnectError::InvitationRejected); }
                        if grant.check_admission().and_then(|_| continuity.check()).is_err() { group.rejected.insert(id); continue; }
                        let invitation = issue(run, grant.guest_key(), &group.services(source)?, lab::SessionDeadlines {
                            admission_expires_at_unix_ms: grant.admission_expires_at_unix_ms(), session_expires_at_unix_ms: grant.session_expires_at_unix_ms() }, group.voice_enabled())?;
                        if let Some(host) = &host {
                            if host.admit_online(&invitation).is_err() { group.rejected.insert(id); continue; }
                            #[cfg(all(windows, feature = "connect-voice-lab"))]
                            if let Some(voice) = &mut group.voice { voice.register_lease(host, &invitation).await?; }
                        } else {
                            start_host(run, source_guard, source, &mut group, &mut host, &mut sockets, &invitation, &ticket).await?;
                        }
                        group.participants.insert(id.clone(), Participant { grant, continuity, invitation: invitation.clone(), admitted: false, terminal: false });
                        group.monitor(run, &host).await?;
                        if group.current_grant(&id).is_err() { continue; }
                        let publishing = http.post("/v1/social/host/publish", json!({"grantId":id,"invitation":invitation.encode()})); tokio::pin!(publishing);
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
            }
        }
    }.await;
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
