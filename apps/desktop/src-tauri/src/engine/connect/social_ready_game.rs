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
    expiry: u64,
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
            expiry: run.expiry,
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
            || lab::unix_millis() >= self.expiry
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
