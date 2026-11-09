//! Private social lifetime gate. No endpoint, process, IO or renderer input.
//! The actor moves its original continuity here; no latest-lease reconstruction.
use super::*;
use std::sync::Weak;
#[cfg(test)]
#[path = "social_ready_guard_fixture.rs"]
pub(super) mod fixture;

struct Current {
    continuity: lab::SocialContinuity,
    invitation: Option<lab::SessionInvitation>,
    terminal: bool,
}

/// Only the social worker can construct this state. A ReadyGame source keeps a
/// Weak reference, and never substitutes a plain invitation for social policy.
pub(in crate::engine::connect::live) struct LaunchAuthority {
    current: Mutex<Current>,
    grant: lab::VerifiedSocialGrant,
    cancel: CancellationToken,
    deadline: Instant,
    account_epoch: u64,
}

impl LaunchAuthority {
    pub(super) fn new(
        run: &Run,
        grant: lab::VerifiedSocialGrant,
        original_continuity: lab::SocialContinuity,
    ) -> Result<Arc<Self>, ConnectError> {
        run.current()?;
        grant
            .check_session()
            .map_err(|_| ConnectError::PlanExpired)?;
        original_continuity
            .check()
            .map_err(|_| ConnectError::PlanExpired)?;
        Ok(Arc::new(Self {
            current: Mutex::new(Current {
                continuity: original_continuity,
                invitation: None,
                terminal: false,
            }),
            grant,
            cancel: run.cancel.clone(),
            deadline: run.deadline,
            account_epoch: run.request.account_epoch,
        }))
    }

    fn validate(
        &self,
        current: &mut Current,
        require_invitation: bool,
    ) -> Result<(), ConnectError> {
        let result = if current.terminal || self.cancel.is_cancelled() {
            Err(ConnectError::Cancelled)
        } else if Instant::now() >= self.deadline
            || self.grant.check_session().is_err()
            || current.continuity.check().is_err()
        {
            Err(ConnectError::PlanExpired)
        } else if require_invitation
            && current
                .invitation
                .as_ref()
                .is_none_or(|invitation| invitation.check_session_current().is_err())
        {
            Err(ConnectError::InvitationRejected)
        } else {
            Ok(())
        };
        if result.is_err() {
            current.terminal = true;
        }
        result
    }

    pub(super) fn check_continuity(&self) -> Result<(), ConnectError> {
        let mut current = self.current.lock().map_err(|_| ConnectError::Internal)?;
        self.validate(&mut current, false)
    }

    pub(super) fn update(&self, signed_lease: &str) -> Result<(), ConnectError> {
        let mut current = self.current.lock().map_err(|_| ConnectError::Internal)?;
        self.validate(&mut current, false)?;
        if current.continuity.update(signed_lease).is_err() {
            current.terminal = true;
            return Err(ConnectError::InvitationRejected);
        }
        Ok(())
    }

    pub(super) fn sequence(&self) -> Result<u64, ConnectError> {
        let mut current = self.current.lock().map_err(|_| ConnectError::Internal)?;
        self.validate(&mut current, false)?;
        Ok(current.continuity.sequence())
    }

    /// The original verified invitation is installed once, only after the actor
    /// owns its admitted Guest and TCP bridge. The grant supplies expected keys.
    pub(super) fn admit(&self, invitation: lab::SessionInvitation) -> Result<(), ConnectError> {
        let mut current = self.current.lock().map_err(|_| ConnectError::Internal)?;
        self.validate(&mut current, false)?;
        if current.invitation.is_some()
            || invitation.is_online_native_voice()
                != (self.grant.voice_transport_profile() == Some("nativeVoice8V1"))
            || invitation.session_id() != self.grant.session_id()
            || invitation.host_key() != self.grant.host_key()
            || invitation.guest_key() != self.grant.guest_key()
            || invitation.admission_expires_at_unix_ms()
                != self.grant.admission_expires_at_unix_ms()
            || invitation.session_expires_at_unix_ms() != self.grant.session_expires_at_unix_ms()
            || invitation.check_session_current().is_err()
        {
            current.terminal = true;
            return Err(ConnectError::InvitationRejected);
        }
        current.invitation = Some(invitation);
        Ok(())
    }

    pub(in crate::engine::connect::live) fn account_epoch(&self) -> u64 {
        self.account_epoch
    }
    pub(in crate::engine::connect::live) fn voice_profile(
        &self,
    ) -> Result<Option<crate::engine::connect_native_voice::NativeVoiceProfile>, ReadyGameLaunchError>
    {
        self.with_current(|| native_profile(self.grant.voice_profile()))
    }
    pub(in crate::engine::connect::live) fn runtime_profile(
        &self,
    ) -> Result<Option<crate::engine::connect_ready_profile::ReadyProfile>, ReadyGameLaunchError>
    {
        self.with_current(|| {
            native_runtime_profile(self.grant.runtime_profile(), self.grant.voice_profile())
        })
    }

    /// Connect is already locked at final spawn. No caller may acquire Connect
    /// while holding this guard from an update/invalidation path.
    pub(in crate::engine::connect::live) fn with_current<T>(
        &self,
        work: impl FnOnce() -> Result<T, ReadyGameLaunchError>,
    ) -> Result<T, ReadyGameLaunchError> {
        let mut current = self
            .current
            .lock()
            .map_err(|_| ReadyGameLaunchError::Unavailable)?;
        self.validate(&mut current, true)
            .map_err(|_| ReadyGameLaunchError::Unavailable)?;
        work()
    }

    pub(super) fn close(&self) {
        if let Ok(mut current) = self.current.lock() {
            current.terminal = true;
        }
    }

    pub(super) fn heartbeat_guard(self: &Arc<Self>) -> HeartbeatGuard {
        HeartbeatGuard(Arc::downgrade(self))
    }
}

/// Producer completion (including error/panic) invalidates the same guarded
/// authority immediately, independently of when the transport actor next polls.
pub(super) struct HeartbeatGuard(Weak<LaunchAuthority>);
impl Drop for HeartbeatGuard {
    fn drop(&mut self) {
        if let Some(authority) = self.0.upgrade() {
            authority.close();
        }
    }
}

fn native_runtime_profile(
    runtime: Option<&str>,
    voice: Option<&str>,
) -> Result<Option<crate::engine::connect_ready_profile::ReadyProfile>, ReadyGameLaunchError> {
    use crate::engine::connect_ready_profile::ReadyProfile;
    match (runtime, voice) {
        (Some(id), Some(_)) => ReadyProfile::from_version_id(id)
            .map(Some)
            .ok_or(ReadyGameLaunchError::UnsupportedRuntime),
        // Schema three explicitly supported only the original Fabric 1.21.1 source.
        (None, Some(_)) => Ok(Some(ReadyProfile::Fabric1211)),
        (None, None) => Ok(None),
        _ => Err(ReadyGameLaunchError::UnsupportedRuntime),
    }
}
fn native_profile(
    profile: Option<&str>,
) -> Result<Option<crate::engine::connect_native_voice::NativeVoiceProfile>, ReadyGameLaunchError> {
    use crate::engine::connect_native_voice::NativeVoiceProfile;
    match profile {
        None => Ok(None),
        Some("svc") => Ok(Some(NativeVoiceProfile::SimpleVoiceChat)),
        Some("plasmo") => Ok(Some(NativeVoiceProfile::PlasmoVoice)),
        Some(_) => Err(ReadyGameLaunchError::Unavailable),
    }
}

#[cfg(all(windows, feature = "connect-voice-lab"))]
struct HostPlayerCurrent {
    terminal: bool,
    host_poll: Option<(u64, Instant)>,
}

/// Original Host-player authority. It owns no endpoint and cannot authorize a
/// remote guest; launch is guarded by the retained Host, native source and policy.
#[cfg(all(windows, feature = "connect-voice-lab"))]
pub(in crate::engine::connect::live) struct HostPlayerLaunchAuthority {
    current: Mutex<HostPlayerCurrent>,
    policy: lab::SignedSocialHostPolicy,
    binding: crate::engine::connect_voice::dedicated::NativeVoiceHostBinding,
    host: Weak<lab::Host>,
    invitation: lab::SessionInvitation,
    ticket: lab::OnlineNetworkTicket,
    cancel: CancellationToken,
    deadline: Instant,
    expiry: u64,
    clock: Option<lab::OnlineClock>,
    account_epoch: u64,
}
#[cfg(all(windows, feature = "connect-voice-lab"))]
impl HostPlayerLaunchAuthority {
    pub(super) fn new(
        run: &Run,
        policy: &lab::SignedSocialHostPolicy,
        binding: crate::engine::connect_voice::dedicated::NativeVoiceHostBinding,
        host: &Arc<lab::Host>,
        invitation: &lab::SessionInvitation,
        ticket: &lab::OnlineNetworkTicket,
    ) -> Result<Arc<Self>, ConnectError> {
        run.current()?;
        policy
            .verify()
            .map_err(|_| ConnectError::InvitationRejected)?;
        ticket
            .check_current()
            .map_err(|_| ConnectError::PlanExpired)?;
        let (profile, digest) = binding
            .online_voice_policy()
            .map_err(|_| ConnectError::SourceChanged)?;
        let runtime = binding
            .online_runtime_profile()
            .map_err(|_| ConnectError::SourceChanged)?;
        let local = run
            .native_voice_local_key
            .as_ref()
            .ok_or(ConnectError::InvitationRejected)?;
        if run.authority.online_origin.is_none()
            || !policy.body().host_player()
            || !policy.body().matches_host(
                &run.request.account_id,
                &run.id,
                run.generation,
                run.key.public(),
                &run.request.session_id,
                binding.targets().0,
            )
            || policy.body().voice_profile() != Some(profile)
            || policy.body().voice_source_digest() != Some(digest.as_str())
            || native_runtime_profile(
                policy.body().runtime_profile(),
                policy.body().voice_profile(),
            )
            .map_err(|_| ConnectError::InvitationRejected)?
                != Some(runtime)
            || host.id() != run.key.public()
            || invitation.is_online_native_voice()
                != (policy.body().voice_transport_profile() == Some("nativeVoice8V1"))
            || invitation.host_key() != host.id()
            || invitation.guest_key() != local.public()
            || invitation.session_id() != run.request.session_id
            || invitation.session_expires_at_unix_ms() != policy.body().expires_at_unix_ms()
            || ticket.session_id() != run.request.session_id
            || ticket.local_peer() != host.id()
            || ticket.host_peer() != host.id()
            || ticket.generation() != run.generation
            || !host.invitation_active(invitation.invitation_id())
        {
            return Err(ConnectError::InvitationRejected);
        }
        // Re-verify only the invitation's strict profile, while retaining the
        // original invitation and all of its monotonic deadlines below.
        let encoded = invitation.encode();
        match &run.clock {
            Some(clock) if policy.body().voice_transport_profile() == Some("nativeVoice8V1") => {
                lab::SessionInvitation::verify_online_native_voice_with_clock(
                    &encoded,
                    host.id(),
                    local.public(),
                    clock,
                )
            }
            Some(clock) => lab::SessionInvitation::verify_online_with_clock(
                &encoded,
                host.id(),
                local.public(),
                true,
                clock,
            ),
            #[cfg(test)]
            None if policy.body().voice_transport_profile() == Some("nativeVoice8V1") => {
                lab::SessionInvitation::verify_online_native_voice(
                    &encoded,
                    host.id(),
                    local.public(),
                )
            }
            #[cfg(test)]
            None => lab::SessionInvitation::verify_online_fragmented(
                &encoded,
                host.id(),
                local.public(),
            ),
            #[cfg(not(test))]
            None => return Err(ConnectError::InvitationRejected),
        }
        .map_err(|_| ConnectError::InvitationRejected)?;
        Ok(Arc::new(Self {
            current: Mutex::new(HostPlayerCurrent {
                terminal: false,
                host_poll: None,
            }),
            policy: policy.clone(),
            deadline: run.deadline.min(binding.deadline()).min(ticket.deadline()),
            binding,
            host: Arc::downgrade(host),
            invitation: invitation.clone(),
            ticket: ticket.clone(),
            cancel: run.cancel.clone(),
            expiry: run.expiry,
            clock: run.clock.clone(),
            account_epoch: run.request.account_epoch,
        }))
    }
    fn valid(
        &self,
        current: &mut HostPlayerCurrent,
        require_poll: bool,
    ) -> Result<(), ReadyGameLaunchError> {
        let now = self
            .clock
            .as_ref()
            .map_or_else(lab::unix_millis, lab::OnlineClock::upper_now);
        if current.terminal
            || self.cancel.is_cancelled()
            || Instant::now() >= self.deadline
            || now >= self.expiry
            || now >= self.policy.body().expires_at_unix_ms()
            || self.ticket.check_current().is_err()
            || self.invitation.check_session_current().is_err()
            || self
                .host
                .upgrade()
                .is_none_or(|host| !host.invitation_active(self.invitation.invitation_id()))
            || current.host_poll.is_some_and(|(issued, deadline)| {
                now < issued || now >= issued.saturating_add(3000) || Instant::now() >= deadline
            })
        {
            current.terminal = true;
            return Err(ReadyGameLaunchError::Unavailable);
        }
        if require_poll && current.host_poll.is_none() {
            return Err(ReadyGameLaunchError::Unavailable);
        }
        Ok(())
    }
    pub(in crate::engine::connect::live) fn refresh_host_poll(
        &self,
        checked_at_unix_ms: u64,
    ) -> Result<(), ConnectError> {
        let mut current = self.current.lock().map_err(|_| ConnectError::Internal)?;
        self.valid(&mut current, false)
            .map_err(|_| ConnectError::PlanExpired)?;
        let now = self
            .clock
            .as_ref()
            .map_or_else(lab::unix_millis, lab::OnlineClock::upper_now);
        if checked_at_unix_ms > now
            || now.saturating_sub(checked_at_unix_ms) >= 3000
            || current
                .host_poll
                .is_some_and(|(previous, _)| checked_at_unix_ms < previous)
        {
            current.terminal = true;
            return Err(ConnectError::PlanExpired);
        }
        let freshness = checked_at_unix_ms.saturating_add(3000).min(self.expiry);
        let deadline = match &self.clock {
            Some(clock) => clock
                .deadline_for(freshness)
                .map_err(|_| ConnectError::PlanExpired)?,
            None => Instant::now() + Duration::from_millis(freshness.saturating_sub(now)),
        };
        // A cached response keeps the earlier monotonic cap as well as its wall expiry.
        let deadline = current
            .host_poll
            .filter(|(previous, _)| *previous == checked_at_unix_ms)
            .map_or(deadline, |(_, old)| old.min(deadline));
        current.host_poll = Some((checked_at_unix_ms, deadline.min(self.deadline)));
        Ok(())
    }
    pub(in crate::engine::connect::live) fn with_current<T>(
        &self,
        work: impl FnOnce() -> Result<T, ReadyGameLaunchError>,
    ) -> Result<T, ReadyGameLaunchError> {
        let mut current = self
            .current
            .lock()
            .map_err(|_| ReadyGameLaunchError::Unavailable)?;
        self.valid(&mut current, true)?;
        self.binding
            .with_current(work)
            .map_err(|_| ReadyGameLaunchError::Unavailable)?
    }
    pub(in crate::engine::connect::live) fn account_epoch(&self) -> u64 {
        self.account_epoch
    }
    pub(in crate::engine::connect::live) fn voice_profile(
        &self,
    ) -> Result<Option<crate::engine::connect_native_voice::NativeVoiceProfile>, ReadyGameLaunchError>
    {
        self.with_current(|| native_profile(self.policy.body().voice_profile()))
    }
    pub(in crate::engine::connect::live) fn runtime_profile(
        &self,
    ) -> Result<Option<crate::engine::connect_ready_profile::ReadyProfile>, ReadyGameLaunchError>
    {
        self.with_current(|| {
            native_runtime_profile(
                self.policy.body().runtime_profile(),
                self.policy.body().voice_profile(),
            )
        })
    }
    pub(in crate::engine::connect::live) fn close(&self) {
        if let Ok(mut current) = self.current.lock() {
            current.terminal = true;
        }
    }
}
