//! Explicit, bounded requests to the one actor that owns Guest and its bridges.
//! No identity, invitation, endpoint or clock is recreated by a recovery.
use super::fragmented_profile::{UdpState, MAX_RECOVERY_ATTEMPTS};
use super::*;
use tokio::sync::mpsc;

pub(super) struct RecoveryControl {
    sender: mpsc::Sender<u8>,
    accepted_generation: Option<u64>,
    pending: Option<u8>,
    connection_closed: Box<dyn Fn() -> bool + Send + Sync>,
}
impl RecoveryControl {
    pub(super) fn new(sender: mpsc::Sender<u8>, guest: &lab::Guest) -> Self {
        let connection = guest.connection().clone();
        Self {
            sender,
            accepted_generation: None,
            pending: None,
            connection_closed: Box::new(move || connection.close_reason().is_some()),
        }
    }
}

pub(super) fn clear(a: &mut Attempt) {
    a.recovery = None;
    if let Some(recovery) = &mut a.snapshot.udp_recovery {
        recovery.pending = false;
    }
}

fn valid(a: &Attempt) -> bool {
    a.snapshot.role == Some("guest")
        && a.snapshot.phase == "guestConnected"
        && !a.cancel.is_cancelled()
        && matches!(a.authority, Some(Authority::SessionFragmented(_)))
        && a.session_deadline
            .is_some_and(|deadline| Instant::now() < deadline)
        && a.authority
            .as_ref()
            .is_some_and(|authority| authority.check_session().is_ok())
}

impl Manager {
    pub(in crate::engine::connect) fn retry_session_udp(
        &self,
        owner: &str,
        id: &str,
        expected_generation: u64,
    ) -> Result<ConnectSnapshot, ConnectError> {
        let mut state = lock(&self.state)?;
        let shutting_down = state.shutting_down;
        let a = selected(&mut state, owner, id)?;
        if shutting_down || a.cancel.is_cancelled() {
            return Err(ConnectError::Cancelled);
        }
        if a.snapshot.role != Some("guest")
            || a.snapshot.phase != "guestConnected"
            || !matches!(a.authority, Some(Authority::SessionFragmented(_)))
        {
            return Err(ConnectError::StaleOperation);
        }
        if !valid(a) {
            return Err(ConnectError::PlanExpired);
        }
        let control = a.recovery.as_mut().ok_or(ConnectError::StaleOperation)?;
        if (control.connection_closed)() {
            return Err(ConnectError::StaleOperation);
        }
        // A lost IPC reply/double click does not spend another attempt. This
        // applies only after checking the original live session above.
        if control.accepted_generation == Some(expected_generation) {
            return Ok(path_projection::snapshot(a));
        }
        if a.snapshot.generation != expected_generation {
            return Err(ConnectError::StaleOperation);
        }
        let projection = a
            .snapshot
            .udp_recovery
            .as_mut()
            .ok_or(ConnectError::Internal)?;
        if control.pending.is_some()
            || projection.pending
            || projection.attempts_used >= MAX_RECOVERY_ATTEMPTS
        {
            return Err(ConnectError::Busy);
        }
        if a.snapshot
            .udp
            .as_ref()
            .is_none_or(|udp| udp.udp_state != UdpState::Failed)
        {
            return Err(ConnectError::StaleOperation);
        }
        // Reserve before changing the snapshot; a closed/full channel does not
        // consume budget or create a starting state without an actor request.
        let permit = control
            .sender
            .clone()
            .try_reserve_owned()
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => ConnectError::Busy,
                mpsc::error::TrySendError::Closed(_) => ConnectError::StaleOperation,
            })?;
        let sequence = projection.attempts_used + 1;
        projection.attempts_used = sequence;
        projection.pending = true;
        control.pending = Some(sequence);
        control.accepted_generation = Some(expected_generation);
        a.snapshot.udp = Some(FragmentedGuestState::starting());
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
        permit.send(sequence);
        Ok(path_projection::snapshot(a))
    }
}

pub(super) fn current(state: &Mutex<State>, id: &str, sequence: u8) -> bool {
    state.lock().ok().is_some_and(|state| {
        !state.shutting_down
            && state.attempt.as_ref().is_some_and(|a| {
                a.snapshot.operation_id.as_deref() == Some(id)
                    && valid(a)
                    && a.recovery.as_ref().is_some_and(|control| {
                        control.pending == Some(sequence) && !(control.connection_closed)()
                    })
            })
    })
}

pub(super) fn complete(
    state: &Mutex<State>,
    id: &str,
    sequence: u8,
    projection: FragmentedGuestState,
) -> bool {
    let Ok(mut state) = state.lock() else {
        return false;
    };
    if state.shutting_down {
        return false;
    }
    let Some(a) = state.attempt.as_mut() else {
        return false;
    };
    if a.snapshot.operation_id.as_deref() != Some(id)
        || !valid(a)
        || a.recovery.as_ref().is_none_or(|control| {
            control.pending != Some(sequence) || (control.connection_closed)()
        })
    {
        return false;
    }
    // STATUS may have changed snapshot.generation while setup was pending.
    // The operation + pending sequence, not that unrelated generation, own it.
    a.recovery.as_mut().expect("checked").pending = None;
    a.snapshot
        .udp_recovery
        .as_mut()
        .expect("recovery projection")
        .pending = false;
    a.snapshot.udp = Some(projection);
    a.snapshot.generation = a.snapshot.generation.saturating_add(1);
    true
}

// Deterministic lifecycle tests retain the result of a real UDP setup before
// publication. Neither these fields nor this barrier exist in production.
#[cfg(test)]
#[derive(Default)]
pub(super) struct TestGate {
    pub(super) entered: tokio::sync::Notify,
    pub(super) release: tokio::sync::Notify,
}
