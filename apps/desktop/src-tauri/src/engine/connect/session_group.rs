//! A static group occupies the existing Attempt slot and cleanup barrier.
//! Individual envelopes are retained once; no polling, export or retry issues
//! replacement authority or derives a participant from another participant.
use super::*;
use crate::engine::connect::group_profile::GROUP_KIND;
use std::collections::HashSet;
#[path = "group_actor_source.rs"]
mod actor_source;
#[cfg(all(test, windows))]
#[path = "group_actor_source_tests.rs"]
mod actor_source_tests;

struct GroupDraft {
    request: GroupHostRequest,
    key: lab::SecretKey,
    bind: SocketAddr,
    services: Vec<lab::Service>,
    source: TargetGuard,
}
#[derive(Clone)]
struct Member {
    participant_id: String,
    invitation: lab::SessionInvitation,
    timing: SessionTiming,
    revoked: bool,
}
pub(super) struct GroupOperation {
    draft: Option<GroupDraft>,
    members: Vec<Member>,
    review_deadline: Option<Instant>,
    host: Option<Arc<lab::Host>>,
}
#[cfg(test)]
pub(super) fn metrics(a: &Attempt) -> Option<lab::MetricsSnapshot> {
    a.group.as_ref()?.host.as_ref().map(|host| host.metrics())
}
#[cfg(windows)]
pub(super) fn publish_discovered(
    a: &mut Attempt,
    source: TargetGuard,
    request: GroupHostRequest,
    bind: SocketAddr,
    services: Vec<lab::Service>,
) {
    let key = lab::generate_key();
    let public_id = key.public().to_string();
    a.snapshot.generation = a.snapshot.generation.saturating_add(1);
    let preparation = GroupPreparation {
        operation_id: a.snapshot.operation_id.clone().expect("reserved operation"),
        generation: a.snapshot.generation,
        public_id: public_id.clone(),
        expires_at_ms: a
            .snapshot
            .expires_at_ms
            .expect("original preparation deadline"),
        kind: GROUP_KIND,
    };
    a.snapshot.host_group = Some(HostGroupSnapshot {
        kind: GROUP_KIND,
        profile: fragmented_profile::SESSION_PROFILE,
        invitation_schema_version: 3,
        mode: request.mode,
        host_public_id: public_id,
        preparation: Some(preparation),
        review: None,
        participants: Vec::new(),
    });
    a.snapshot.phase = "hostPrepared";
    a.snapshot.pending_group_source = None;
    a.group = Some(GroupOperation {
        draft: Some(GroupDraft {
            request,
            key,
            bind,
            services,
            source,
        }),
        members: Vec::new(),
        review_deadline: None,
        host: None,
    });
}
#[cfg(test)]
#[derive(Default)]
pub(super) struct GroupTestGate {
    pub(super) entered: tokio::sync::Notify,
    pub(super) release: tokio::sync::Notify,
}

fn projection(member: &Member, host: Option<&lab::Host>) -> GroupParticipant {
    let session_valid = !member.revoked && member.invitation.check_session_current().is_ok();
    let admission_valid = session_valid && member.invitation.check_admission_current().is_ok();
    // Consumption is sampled after the clock, so an admission immediately
    // before ingress expiry cannot be mistaken for a never-admitted invite.
    let lease_id = member.invitation.invitation_id();
    // Active precedes monotonic consumption: authentication between separate
    // reads must not project an active connection with consumed=false.
    let transport_active =
        session_valid && host.is_some_and(|host| host.invitation_active(lease_id));
    let consumed = host.is_some_and(|host| host.invitation_consumed(lease_id));
    GroupParticipant {
        participant_id: member.participant_id.clone(),
        guest_public_id: member.invitation.guest_key().to_string(),
        invitation_digest: member.invitation.digest(),
        session_timing: member.timing.clone(),
        consumed,
        revoked: member.revoked,
        admission_valid,
        session_valid,
        transport_active: transport_active && consumed,
    }
}

/// Refresh only observations. The immutable review and its digest never change.
/// Returns true when every participant's original authorization is finished.
pub(super) fn refresh(a: &mut Attempt) -> bool {
    let Some(group) = a.group.as_ref() else {
        return false;
    };
    if a.snapshot.phase != "hostListening" {
        return false;
    }
    let participants: Vec<_> = group
        .members
        .iter()
        .map(|member| projection(member, group.host.as_deref()))
        .collect();
    let finished = participants.iter().all(|member| {
        member.revoked || !member.session_valid || (!member.consumed && !member.admission_valid)
    });
    if let Some(snapshot) = a.snapshot.host_group.as_mut() {
        if snapshot.participants != participants {
            snapshot.participants = participants;
            a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        }
    }
    finished
}

pub(super) fn closing(a: &mut Attempt) {
    if a.snapshot.pending_group_source.take().is_some() {
        a.snapshot.expires_at_ms = None;
    }
    if let Some(group) = a.group.as_mut() {
        group.draft = None;
        group.review_deadline = None;
    }
    if let Some(snapshot) = a.snapshot.host_group.as_mut() {
        snapshot.preparation = None;
        snapshot.review = None;
        for participant in &mut snapshot.participants {
            participant.admission_valid = false;
            participant.session_valid = false;
            participant.transport_active = false;
        }
        a.snapshot.expires_at_ms = None;
    }
}
pub(super) fn finished(a: &mut Attempt) {
    if let (Some(group), Some(snapshot)) = (a.group.as_ref(), a.snapshot.host_group.as_mut()) {
        if let Some(host) = group.host.as_ref() {
            for (member, participant) in group.members.iter().zip(&mut snapshot.participants) {
                participant.consumed |= host.invitation_consumed(member.invitation.invitation_id());
                participant.revoked = member.revoked;
            }
        }
    }
    closing(a);
    a.group = None;
}
pub(super) fn expire_pending(a: &mut Attempt, now: u64) {
    if !matches!(a.snapshot.phase, "hostPrepared" | "reviewed") {
        return;
    }
    let Some(group) = a.group.as_ref() else {
        return;
    };
    let deadline = group
        .review_deadline
        .unwrap_or(a.prepared_deadline)
        .min(a.prepared_deadline);
    let expired = a
        .snapshot
        .expires_at_ms
        .is_none_or(|expires| now >= expires)
        || Instant::now() >= deadline
        || group
            .members
            .iter()
            .any(|member| member.invitation.check_admission_current().is_err());
    if expired {
        finished(a);
        a.snapshot.phase = "closed";
        a.snapshot.transport_state = "closed";
        a.cancel.cancel();
        a.snapshot.cleanup_state = if group_source::finished(a) {
            "complete"
        } else {
            "pending"
        };
        a.snapshot.error_code = Some(ConnectError::PlanExpired.code());
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
    }
}

impl Manager {
    pub(in crate::engine::connect) fn prepare_session_group_host(
        &self,
        owner: &str,
        request: GroupHostRequest,
    ) -> Result<GroupPreparation, ConnectError> {
        if owner.is_empty() || owner.len() > 128 {
            return Err(ConnectError::InvalidRequest);
        }
        let bind = canonical(&request.bind_address)?;
        policy(request.mode, bind)?;
        let game_target = canonical(&request.game_target)?;
        policy(ConnectMode::Loopback, game_target)?;
        let services = fragmented_profile::services(game_target, canonical(&request.udp_target)?)?;
        let mut state = lock(&self.state)?;
        if state.shutting_down {
            return Err(ConnectError::Cancelled);
        }
        if let Some(a) = state.attempt.as_mut() {
            super::expire_pending(a);
            if !terminal(a) {
                return Err(ConnectError::Busy);
            }
        }
        let key = lab::generate_key();
        let public_id = key.public().to_string();
        state.generation = state
            .generation
            .max(state.attempt.as_ref().map_or(0, |a| a.snapshot.generation))
            .saturating_add(1);
        let at = Instant::now();
        let preparation = GroupPreparation {
            operation_id: uuid::Uuid::new_v4().to_string(),
            generation: state.generation,
            public_id: public_id.clone(),
            expires_at_ms: lab::unix_millis().saturating_add(600_000),
            kind: GROUP_KIND,
        };
        let mut snapshot = ConnectSnapshot::idle();
        snapshot.operation_id = Some(preparation.operation_id.clone());
        snapshot.generation = preparation.generation;
        snapshot.role = Some("host");
        snapshot.phase = "hostPrepared";
        snapshot.expires_at_ms = Some(preparation.expires_at_ms);
        snapshot.host_group = Some(HostGroupSnapshot {
            kind: GROUP_KIND,
            profile: fragmented_profile::SESSION_PROFILE,
            invitation_schema_version: 3,
            mode: request.mode,
            host_public_id: public_id,
            preparation: Some(preparation.clone()),
            review: None,
            participants: Vec::new(),
        });
        state.attempt = Some(Attempt {
            owner: owner.into(),
            snapshot,
            draft: None,
            reviewed: None,
            prepared_deadline: at + Duration::from_secs(600),
            cancel: CancellationToken::new(),
            worker: None,
            export: None,
            host: None,
            probe_active: false,
            probes: 0,
            last_probe: None,
            session_deadline: None,
            admission_deadline: None,
            authority: None,
            recovery: None,
            guest_source: None,
            ready_game: None,
            group_source: None,
            host_source: None,
            host_review: None,
            group: Some(GroupOperation {
                draft: Some(GroupDraft {
                    request,
                    key,
                    bind,
                    services,
                    source: TargetGuard::Manual,
                }),
                members: Vec::new(),
                review_deadline: None,
                host: None,
            }),
            #[cfg(test)]
            recovery_gate: None,
            #[cfg(test)]
            group_start_gate: None,
        });
        Ok(preparation)
    }

    pub(in crate::engine::connect) fn review_session_group_host(
        &self,
        owner: &str,
        id: &str,
        participants: Vec<GroupParticipantRequest>,
    ) -> Result<GroupReview, ConnectError> {
        if !(2..=8).contains(&participants.len()) {
            return Err(ConnectError::InvalidRequest);
        }
        // Source IO must outlive a lost caller, yet must not hold Connect's
        // mutex. This synchronous guard lives on the actual query's thread.
        #[cfg(test)]
        let query_gate;
        let (source, observed_generation, _query_guard) = {
            let mut state = lock(&self.state)?;
            let shutting_down = state.shutting_down;
            let a = selected(&mut state, owner, id)?;
            if shutting_down || a.cancel.is_cancelled() {
                return Err(ConnectError::Cancelled);
            }
            super::expire_pending(a);
            if a.snapshot.phase != "hostPrepared" {
                return Err(ConnectError::PlanExpired);
            }
            let source = a
                .group
                .as_ref()
                .and_then(|group| group.draft.as_ref())
                .ok_or(ConnectError::StaleOperation)?
                .source
                .clone();
            let guard = group_source::begin_review(a)?;
            #[cfg(test)]
            {
                query_gate = a
                    .group_source
                    .as_ref()
                    .and_then(|work| work.hooks.review.clone());
            }
            (source, a.snapshot.generation, guard)
        };
        #[cfg(test)]
        if let Some(gate) = query_gate {
            gate.wait();
        }
        if source.recheck().is_err() {
            let mut state = lock(&self.state)?;
            let a = selected(&mut state, owner, id)?;
            if a.snapshot.phase == "hostPrepared" && !a.cancel.is_cancelled() {
                finished(a);
                a.snapshot.phase = "failed";
                a.snapshot.transport_state = "closed";
                a.snapshot.cleanup_state = "pending";
                a.snapshot.error_code = Some(ConnectError::SourceChanged.code());
                a.snapshot.generation = a.snapshot.generation.saturating_add(1);
            }
            return Err(ConnectError::SourceChanged);
        }
        let mut state = lock(&self.state)?;
        let shutting_down = state.shutting_down;
        let a = selected(&mut state, owner, id)?;
        if shutting_down || a.cancel.is_cancelled() {
            return Err(ConnectError::Cancelled);
        }
        if a.snapshot.generation != observed_generation {
            return Err(ConnectError::StaleOperation);
        }
        super::expire_pending(a);
        if a.snapshot.phase != "hostPrepared" {
            return Err(ConnectError::PlanExpired);
        }
        let group = a.group.as_ref().ok_or(ConnectError::StaleOperation)?;
        let draft = group.draft.as_ref().ok_or(ConnectError::StaleOperation)?;
        let mut peers = HashSet::new();
        let recipients = participants
            .iter()
            .map(|participant| {
                let recipient = peer(&participant.guest_public_id)?;
                if recipient == draft.key.public()
                    || !peers.insert(recipient)
                    || participant.admission_seconds == 0
                    || participant.admission_seconds > 600
                    || participant.session_seconds < participant.admission_seconds
                    || participant.session_seconds > 3600
                {
                    return Err(ConnectError::InvalidRequest);
                }
                Ok(recipient)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let started = Instant::now();
        let mut review_deadline = (started + Duration::from_secs(60)).min(a.prepared_deadline);
        let mut expires = a
            .snapshot
            .expires_at_ms
            .ok_or(ConnectError::Internal)?
            .min(lab::unix_millis().saturating_add(60_000));
        let mut members = Vec::new();
        for (participant, recipient) in participants.iter().zip(recipients) {
            let issue = match draft.request.mode {
                ConnectMode::Loopback => {
                    lab::SessionInvitation::issue_fragmented_loopback_for_lifetimes
                }
                ConnectMode::ExternalIpv6 => {
                    lab::SessionInvitation::issue_fragmented_external_ipv6_for_lifetimes
                }
                ConnectMode::DirectIpv4 => {
                    lab::SessionInvitation::issue_fragmented_direct_ipv4_for_lifetimes
                }
            };
            let invitation = issue(
                &draft.key,
                recipient,
                id,
                draft.bind,
                &draft.services,
                Duration::from_secs(participant.admission_seconds.into()),
                Duration::from_secs(participant.session_seconds.into()),
            )
            .map_err(|_| ConnectError::InvitationRejected)?;
            let timing = Authority::SessionFragmented(invitation.clone())
                .timing()?
                .ok_or(ConnectError::Internal)?;
            expires = expires.min(invitation.admission_expires_at_unix_ms());
            let anchor = Instant::now();
            review_deadline = review_deadline.min(
                anchor
                    + invitation
                        .admission_remaining()
                        .map_err(|_| ConnectError::PlanExpired)?,
            );
            members.push(Member {
                participant_id: uuid::Uuid::new_v4().to_string(),
                invitation,
                timing,
                revoked: false,
            });
        }
        if Instant::now() >= review_deadline
            || lab::unix_millis() >= expires
            || members
                .iter()
                .any(|member| member.invitation.check_admission_current().is_err())
        {
            return Err(ConnectError::PlanExpired);
        }
        let generation = a.snapshot.generation.saturating_add(1);
        let origin = draft.source.projection();
        let mut review = GroupReview {
            operation_id: id.into(),
            generation,
            digest: String::new(),
            kind: GROUP_KIND,
            mode: draft.request.mode,
            host_public_id: draft.key.public().to_string(),
            direct_address: draft.bind.to_string(),
            game_target: draft.request.game_target.clone(),
            target_source: origin.kind,
            instance_id: origin.instance_id,
            run_id: origin.run_id,
            fragmented_udp: FragmentedUdpReview {
                contract: FragmentedUdpContract::session(),
                udp_target: Some(draft.request.udp_target.clone()),
            },
            participants: members
                .iter()
                .map(|member| projection(member, None))
                .collect(),
            expires_at_ms: expires,
        };
        let bytes = if origin.kind == "manual" {
            serde_json::to_vec(&("aurora.connect.desktop.static-group/v1", owner, &review))
        } else {
            serde_json::to_vec(&(
                "aurora.connect.desktop.owned-static-group/v1",
                owner,
                &origin.fingerprint,
                &review,
            ))
        }
        .map_err(|_| ConnectError::Internal)?;
        review.digest = format!("{:x}", Sha256::digest(bytes));
        let group = a.group.as_mut().ok_or(ConnectError::Internal)?;
        group.members = members;
        group.review_deadline = Some(review_deadline);
        let snapshot = a
            .snapshot
            .host_group
            .as_mut()
            .ok_or(ConnectError::Internal)?;
        snapshot.preparation = None;
        snapshot.participants = review.participants.clone();
        snapshot.review = Some(review.clone());
        a.snapshot.phase = "reviewed";
        a.snapshot.generation = generation;
        a.snapshot.expires_at_ms = Some(expires);
        Ok(review)
    }

    pub(in crate::engine::connect) async fn confirm_session_group_host(
        &self,
        owner: &str,
        id: &str,
        digest: &str,
    ) -> Result<ConnectSnapshot, ConnectError> {
        let mut state = lock(&self.state)?;
        let shutting_down = state.shutting_down;
        let a = selected(&mut state, owner, id)?;
        if shutting_down || a.cancel.is_cancelled() {
            return Err(ConnectError::Cancelled);
        }
        super::expire_pending(a);
        if a.snapshot.phase != "reviewed" {
            return Err(ConnectError::PlanExpired);
        }
        let review = a
            .snapshot
            .host_group
            .as_ref()
            .and_then(|group| group.review.as_ref())
            .ok_or(ConnectError::StaleOperation)?;
        if review.digest != digest {
            return Err(ConnectError::WrongDigest);
        }
        super::expire_pending(a);
        if a.snapshot.phase != "reviewed" {
            return Err(ConnectError::PlanExpired);
        }
        let group = a.group.as_mut().ok_or(ConnectError::StaleOperation)?;
        let draft = group.draft.take().ok_or(ConnectError::StaleOperation)?;
        let invitations = group
            .members
            .iter()
            .map(|member| member.invitation.clone())
            .collect();
        a.snapshot.phase = "starting";
        a.snapshot.cleanup_state = "pending";
        a.snapshot.expires_at_ms = None;
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        let snapshot = a
            .snapshot
            .host_group
            .as_mut()
            .ok_or(ConnectError::Internal)?;
        snapshot.preparation = None;
        snapshot.review = None;
        let shared = self.state.clone();
        let id = id.to_owned();
        let cancel = a.cancel.clone();
        #[cfg(test)]
        let gate = a.group_start_gate.clone();
        a.worker = Some(tokio::spawn(async move {
            run_group(
                shared,
                id,
                cancel,
                draft,
                invitations,
                #[cfg(test)]
                gate,
            )
            .await;
        }));
        Ok(a.snapshot.clone())
    }

    pub(in crate::engine::connect) fn export_session_group_invitation(
        &self,
        owner: &str,
        id: &str,
        participant_id: &str,
    ) -> Result<GroupInvitationExport, ConnectError> {
        let mut state = lock(&self.state)?;
        let shutting_down = state.shutting_down;
        let a = selected(&mut state, owner, id)?;
        if shutting_down || a.snapshot.phase != "hostListening" || a.cancel.is_cancelled() {
            return Err(ConnectError::StaleOperation);
        }
        refresh(a);
        let group = a.group.as_ref().ok_or(ConnectError::StaleOperation)?;
        let member = group
            .members
            .iter()
            .find(|member| member.participant_id == participant_id)
            .ok_or(ConnectError::StaleOperation)?;
        let host = group.host.as_ref().ok_or(ConnectError::StaleOperation)?;
        let current = projection(member, Some(host));
        if current.revoked || !current.admission_valid || !current.session_valid {
            return Err(ConnectError::PlanExpired);
        }
        if current.consumed {
            return Err(ConnectError::StaleOperation);
        }
        Ok(GroupInvitationExport {
            operation_id: id.into(),
            participant_id: participant_id.into(),
            invitation_digest: member.invitation.digest(),
            invitation: member.invitation.encode(),
            host_public_id: host.id().to_string(),
            expires_at_ms: member.invitation.admission_expires_at_unix_ms(),
            session_timing: member.timing.clone(),
            fragmented_udp: FragmentedUdpContract::session(),
        })
    }

    pub(in crate::engine::connect) fn revoke_session_group_participant(
        &self,
        owner: &str,
        id: &str,
        participant_id: &str,
        expected_generation: u64,
    ) -> Result<ConnectSnapshot, ConnectError> {
        if expected_generation > (1u64 << 53) - 1 {
            return Err(ConnectError::InvalidRequest);
        }
        let mut state = lock(&self.state)?;
        let shutting_down = state.shutting_down;
        let a = selected(&mut state, owner, id)?;
        if shutting_down || a.snapshot.phase != "hostListening" || a.cancel.is_cancelled() {
            return Err(ConnectError::StaleOperation);
        }
        let group = a.group.as_mut().ok_or(ConnectError::StaleOperation)?;
        let member = group
            .members
            .iter_mut()
            .find(|member| member.participant_id == participant_id)
            .ok_or(ConnectError::StaleOperation)?;
        if member.revoked {
            refresh(a);
            return Ok(a.snapshot.clone());
        }
        if a.snapshot.generation != expected_generation {
            return Err(ConnectError::StaleOperation);
        }
        let host = group.host.as_ref().ok_or(ConnectError::StaleOperation)?;
        if !host.revoke(member.invitation.invitation_id()) {
            return Err(ConnectError::StaleOperation);
        }
        member.revoked = true;
        refresh(a);
        Ok(a.snapshot.clone())
    }
}

async fn run_group(
    state: Arc<Mutex<State>>,
    id: String,
    cancel: CancellationToken,
    draft: GroupDraft,
    invitations: Vec<lab::SessionInvitation>,
    #[cfg(test)] gate: Option<Arc<GroupTestGate>>,
) {
    use actor_source::{ActorSource, Stage};
    let mut actor = ActorSource {
        state: &state,
        id: &id,
        cancel: &cancel,
        invitations: &invitations,
        host: None,
    };
    let source = draft.source.clone();
    let (checked, interrupted) = actor.recheck(&source, Stage::BeforeBind).await;
    if checked.is_err() || interrupted || !actor.strict_current() {
        close_operation(&state, &id, &cancel, checked.err()).await;
        return;
    }
    let config = lab::HostConfig {
        secret_key: draft.key,
        bind_addr: draft.bind,
        session_id: id.clone(),
        services: draft.services,
        leases: invitations
            .iter()
            .map(lab::SessionInvitation::lease)
            .collect(),
        limits: lab::Limits::default(),
    };
    let result = match draft.request.mode {
        ConnectMode::Loopback => {
            lab::Host::start_invited_fragmented_session_group(config, &invitations).await
        }
        ConnectMode::ExternalIpv6 => {
            lab::Host::start_invited_fragmented_session_group_external_ipv6(config, &invitations)
                .await
        }
        ConnectMode::DirectIpv4 => {
            lab::Host::start_invited_fragmented_session_group_direct_ipv4(config, &invitations)
                .await
        }
    };
    actor.host = Some(match result {
        Ok(host) => Arc::new(host),
        Err(_) => {
            close_operation(
                &state,
                &id,
                &cancel,
                (!cancel.is_cancelled()).then_some(ConnectError::TransportFailed),
            )
            .await;
            return;
        }
    });
    #[cfg(test)]
    if let Some(gate) = gate {
        gate.entered.notify_one();
        gate.release.notified().await;
    }
    let (checked, interrupted) = actor.recheck(&source, Stage::AfterBind).await;
    let mut final_error = checked.err();
    let mut published = false;
    if final_error.is_none() && !interrupted && actor.strict_current() {
        update(&state, &id, |a| {
            if a.snapshot.phase == "starting" && !a.cancel.is_cancelled() && actor.strict_current()
            {
                if let Some(group) = a.group.as_mut() {
                    group.host = actor.host.clone();
                    a.snapshot.phase = "hostListening";
                    a.snapshot.transport_state = "listening";
                    published = true;
                }
            }
        });
    }
    if published {
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        let mut source_tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _=cancel.cancelled()=>break,
                _=source_tick.tick()=> {
                    let (checked, interrupted) = actor.recheck(&source, Stage::Monitor).await;
                    if let Err(error) = checked { final_error = Some(error); break; }
                    if interrupted { break; }
                }
                _=tick.tick()=> {
                    if !actor.monitor_current() {break;}
                }
            }
        }
    }
    actor.interrupt().await;
    close_operation(&state, &id, &cancel, final_error).await;
}

#[cfg(test)]
mod clock_tests {
    use super::*;

    fn prepared(manager: &Manager) -> GroupPreparation {
        manager
            .prepare_session_group_host(
                "main",
                GroupHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: "127.0.0.1:43000".into(),
                    game_target: "127.0.0.1:43001".into(),
                    udp_target: "127.0.0.1:43002".into(),
                },
            )
            .unwrap()
    }
    fn participants() -> Vec<GroupParticipantRequest> {
        (0..2)
            .map(|_| GroupParticipantRequest {
                guest_public_id: lab::generate_key().public().to_string(),
                admission_seconds: 600,
                session_seconds: 3600,
            })
            .collect()
    }

    #[test]
    fn group_preparation_is_600_seconds_and_keeps_monotonic_expiry_without_io() {
        let manager = Manager::default();
        let before = lab::unix_millis();
        let preparation = prepared(&manager);
        let after = lab::unix_millis();
        assert!((before + 600_000..=after + 600_000).contains(&preparation.expires_at_ms));
        {
            let mut state = lock(&manager.state).unwrap();
            let a = selected(&mut state, "main", &preparation.operation_id).unwrap();
            assert!(a.worker.is_none());
            assert!(a.prepared_deadline > Instant::now() + Duration::from_secs(590));
            a.prepared_deadline = Instant::now() - Duration::from_secs(1);
        }
        let snapshot = manager.snapshot("main").unwrap();
        assert_eq!(snapshot.phase, "closed");
        assert_eq!(snapshot.cleanup_state, "complete");
        assert_eq!(snapshot.error_code, Some(ConnectError::PlanExpired.code()));
        assert!(snapshot.host_group.unwrap().participants.is_empty());
    }

    #[test]
    fn group_review_is_60_seconds_or_remaining_original_preparation() {
        for remaining in [600, 5] {
            let manager = Manager::default();
            let preparation = prepared(&manager);
            let at = Instant::now();
            let now = lab::unix_millis();
            {
                let mut state = lock(&manager.state).unwrap();
                let a = selected(&mut state, "main", &preparation.operation_id).unwrap();
                a.prepared_deadline = at + Duration::from_secs(remaining);
                a.snapshot.expires_at_ms = Some(now + remaining * 1000);
                a.snapshot
                    .host_group
                    .as_mut()
                    .unwrap()
                    .preparation
                    .as_mut()
                    .unwrap()
                    .expires_at_ms = now + remaining * 1000;
            }
            let review = manager
                .review_session_group_host("main", &preparation.operation_id, participants())
                .unwrap();
            let limit = remaining.min(60);
            assert!(review.expires_at_ms <= lab::unix_millis() + limit * 1000);
            assert!(review.expires_at_ms > now + (limit - 1) * 1000);
            {
                let mut state = lock(&manager.state).unwrap();
                let a = selected(&mut state, "main", &preparation.operation_id).unwrap();
                assert!(a.worker.is_none());
                let group = a.group.as_mut().unwrap();
                assert!(
                    group.review_deadline.unwrap() <= Instant::now() + Duration::from_secs(limit)
                );
                assert!(group.review_deadline.unwrap() <= a.prepared_deadline);
                // An unchanged civil clock cannot resurrect the review after
                // its independent monotonic deadline has expired.
                group.review_deadline = Some(Instant::now() - Duration::from_secs(1));
            }
            let snapshot = manager.snapshot("main").unwrap();
            assert_eq!(snapshot.phase, "closed");
            assert!(snapshot
                .host_group
                .unwrap()
                .participants
                .iter()
                .all(|member| {
                    !member.admission_valid && !member.session_valid && !member.transport_active
                }));
        }
    }
}
