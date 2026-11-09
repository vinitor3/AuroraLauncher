//! The new signed voice profile consumes fresh discovery only while verifying
//! its grant. Its review then retains that original grant, attempt and deadline.
use super::*;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
#[path = "social_guest_admission_tests.rs"]
mod tests;

pub(super) struct NativeVoiceAdmission {
    grant: lab::VerifiedSocialGrant,
    grant_digest: String,
    deadline: Instant,
    state: Arc<Mutex<State>>,
    operation: String,
    epoch: u64,
    cancel: CancellationToken,
    admitted: AtomicBool,
}

pub(super) fn verify_guest_admission(
    run: &Run,
    context: &lab::VerifiedSocialJoinContext,
    challenge: &lab::SocialKeyChallenge,
    encoded: &str,
) -> Result<(lab::VerifiedSocialGrant, Option<NativeVoiceAdmission>), ConnectError> {
    run.current()?;
    let verify = if run.authority.online_origin.is_some() {
        lab::VerifiedSocialGrant::for_guest_online
    } else {
        lab::VerifiedSocialGrant::for_guest
    };
    // This verifier checks the original context both before and after signature
    // verification, including its policy/session digests and every participant.
    let grant = verify(encoded, run.authority.pin, challenge, context, run.deadline)
        .map_err(|_| ConnectError::InvitationRejected)?;
    let native = match grant.voice_transport_profile() {
        None => None,
        Some("nativeVoice8V1") if run.authority.online_origin.is_some() => {
            let deadline = run
                .clock()?
                .deadline_for(grant.admission_expires_at_unix_ms())
                .map_err(|_| ConnectError::PlanExpired)?
                .min(grant.session_deadline())
                .min(run.deadline);
            let native = NativeVoiceAdmission {
                grant: grant.clone(),
                grant_digest: format!("{:x}", Sha256::digest(encoded.as_bytes())),
                deadline,
                state: run.state.clone(),
                operation: run.id.clone(),
                epoch: run.request.account_epoch,
                cancel: run.cancel.clone(),
                admitted: AtomicBool::new(false),
            };
            native.check()?;
            Some(native)
        }
        _ => return Err(ConnectError::InvitationRejected),
    };
    Ok((grant, native))
}

impl NativeVoiceAdmission {
    fn check(&self) -> Result<(), ConnectError> {
        if self.cancel.is_cancelled() {
            return Err(ConnectError::Cancelled);
        }
        {
            let state = lock(&self.state)?;
            if state.social_epoch != self.epoch
                || state.attempt.as_ref().is_none_or(|attempt| {
                    attempt.cancel.is_cancelled()
                        || attempt.snapshot.operation_id.as_deref() != Some(self.operation.as_str())
                })
            {
                return Err(ConnectError::StaleOperation);
            }
        }
        self.grant
            .check_session()
            .map_err(|_| ConnectError::PlanExpired)?;
        if !self.admitted.load(Ordering::Acquire) {
            if Instant::now() >= self.deadline {
                return Err(ConnectError::PlanExpired);
            }
            self.grant
                .check_admission()
                .map_err(|_| ConnectError::PlanExpired)?;
        }
        Ok(())
    }
    fn admit(&self, guest: &lab::Guest) -> Result<(), ConnectError> {
        self.check()?;
        if guest.id() != self.grant.guest_key()
            || guest.connection().remote_id() != self.grant.host_key()
            || guest.connection().close_reason().is_some()
        {
            return Err(ConnectError::InvitationRejected);
        }
        self.admitted
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ConnectError::StaleOperation)?;
        self.check()
    }
}

pub(super) enum GuestIngress<'a> {
    Legacy(&'a lab::VerifiedSocialJoinContext),
    Native(&'a NativeVoiceAdmission),
}
impl GuestIngress<'_> {
    pub(super) fn check_current(&self) -> Result<(), ConnectError> {
        match self {
            Self::Legacy(context) => context
                .check_current()
                .map_err(|_| ConnectError::PlanExpired),
            Self::Native(native) => native.check(),
        }
    }
    pub(super) fn ingress_deadline(&self) -> Instant {
        match self {
            Self::Legacy(context) => context.ingress_deadline(),
            Self::Native(native) => native.deadline,
        }
    }
    pub(super) fn admit(&self, guest: &lab::Guest) -> Result<(), ConnectError> {
        match self {
            Self::Legacy(_) => Ok(()),
            Self::Native(native) => native.admit(guest),
        }
    }
    pub(super) fn review_digest(
        &self,
        invitation: &lab::SessionInvitation,
        generation: u64,
    ) -> Result<String, ConnectError> {
        let Self::Native(native) = self else {
            return Err(ConnectError::InvitationRejected);
        };
        native.check()?;
        let bytes = serde_json::to_vec(&(
            "aurora/connect/native-voice-guest-review/v1",
            &native.grant_digest,
            invitation.digest(),
            &native.operation,
            native.epoch,
            generation,
        ))
        .map_err(|_| ConnectError::Internal)?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

pub(super) fn check_before_transport(
    run: &Run,
    ingress: &GuestIngress<'_>,
    grant: &lab::VerifiedSocialGrant,
    invitation: &lab::SessionInvitation,
    authority: &ready_source::LaunchAuthority,
) -> Result<(), ConnectError> {
    run.current()?;
    ingress.check_current()?;
    grant
        .check_admission()
        .map_err(|_| ConnectError::PlanExpired)?;
    invitation
        .check_admission_current()
        .map_err(|_| ConnectError::PlanExpired)?;
    authority.check_continuity()
}

/// The owned connection future can be drained after cancellation before its
/// first poll. Keep this gate inside that future so draining cannot start an
/// endpoint whose original admission has already ended.
pub(super) async fn after_ticket<T>(
    run: &Run,
    ingress: &GuestIngress<'_>,
    grant: &lab::VerifiedSocialGrant,
    invitation: &lab::SessionInvitation,
    authority: &ready_source::LaunchAuthority,
    connection: impl std::future::Future<Output = Result<T, ConnectError>>,
) -> Result<T, ConnectError> {
    check_before_transport(run, ingress, grant, invitation, authority)?;
    connection.await
}

pub(super) fn consume_latest_lease(
    authority: &ready_source::LaunchAuthority,
    leases: &mut tokio::sync::watch::Receiver<String>,
) -> Result<(), ConnectError> {
    let latest = leases.borrow_and_update().clone();
    let sequence = serde_json::from_str::<Value>(&latest)
        .ok()
        .and_then(|value| value["body"]["sequence"].as_u64())
        .ok_or(ConnectError::InvitationRejected)?;
    if sequence > authority.sequence()? {
        authority
            .update(&latest)
            .map_err(|_| ConnectError::InvitationRejected)?;
    }
    Ok(())
}

/// A review sender is useful only to this wait. Dropping the wait, including
/// unwinding, removes the exact pending review before slower network cleanup.
pub(super) struct GuestReviewGuard<'a> {
    pub(super) run: &'a Run,
    pub(super) digest: &'a str,
}
impl Drop for GuestReviewGuard<'_> {
    fn drop(&mut self) {
        update(&self.run.state, &self.run.id, |attempt| {
            let Some(social) = &mut attempt.snapshot.social else {
                return;
            };
            if social.stage != "guestReview" || social.digest.as_deref() != Some(self.digest) {
                return;
            }
            social.digest = None;
            social.stage = "closed";
            if let Some(control) = &mut attempt.social {
                control.approval = None;
            }
            attempt.snapshot.phase = "closing";
            attempt.snapshot.cleanup_state = "pending";
            attempt.snapshot.generation = attempt.snapshot.generation.saturating_add(1);
        });
    }
}
