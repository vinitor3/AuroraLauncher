//! Instantaneous, redacted presentation of existing authenticated transports.
//! No new transport owner, probe, stream, timer, task or authorization exists.
use super::*;

/// All command reply gates must run before this function: observing a path may
/// change the output generation, but must not change a command's input gate.
pub(super) fn snapshot(a: &mut Attempt) -> ConnectSnapshot {
    session_group::refresh(a);
    let observation = if !a.cancel.is_cancelled() && a.snapshot.host_group.is_none() {
        match (a.snapshot.role, a.snapshot.phase) {
            (Some("guest"), "guestConnected") => a
                .guest_source
                .as_ref()
                .and_then(|source| source.path_observation()),
            (Some("host"), "hostListening") => a
                .host
                .as_ref()
                .and_then(|(host, lease)| host.invitation_path_observation(lease))
                .map(Into::into),
            _ => None,
        }
    } else {
        None
    };
    if a.snapshot.path_observation != observation {
        a.snapshot.path_observation = observation;
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
    }
    if !matches!(a.snapshot.phase, "hostListening" | "guestConnected") || a.cancel.is_cancelled() {
        let mut changed = false;
        if let Some(group) = &mut a.snapshot.host_group {
            for participant in &mut group.participants {
                changed |= participant.path_observation.take().is_some();
            }
        }
        if changed {
            a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        }
    }
    a.snapshot.clone()
}

/// Called by state transitions which already advance the output generation.
pub(super) fn clear(a: &mut Attempt) {
    a.snapshot.path_observation = None;
    if let Some(group) = &mut a.snapshot.host_group {
        for participant in &mut group.participants {
            participant.path_observation = None;
        }
    }
}

pub(super) fn participant(
    host: &lab::Host,
    invitation: &lab::SessionInvitation,
    revoked: bool,
    consumed: bool,
    transport_active: bool,
) -> Option<ConnectPathObservation> {
    if revoked || !consumed || invitation.check_session_current().is_err() {
        return None;
    }
    let lease = invitation.invitation_id();
    let observation = host.invitation_path_observation(lease)?;
    // Be conservative when an expiry, close or admission occurred between
    // projection and sampling. Never use a path sample to reactivate a member.
    if invitation.check_session_current().is_err()
        || host.invitation_active(lease) != transport_active
        || (observation.state == lab::PathState::Closed) == transport_active
    {
        return None;
    }
    Some(observation.into())
}
