//! Opaque native origin for a future private Host caller. Construction performs
//! no IO and grants no transport authority. Only a DedicatedRun can create it.
use super::*;
use crate::engine::connect_voice::dedicated::manifest::StoredVoiceProfileV2;

/// A clone still refers weakly to the same run. These fields cannot be supplied
/// by a caller, reconstructed from a sample or serialized into a new authority.
#[derive(Clone)]
pub(crate) struct NativeVoiceHostBinding {
    shared: Weak<Shared>,
    native_nonce: Uuid,
    profile: StoredVoiceProfileV2,
    deadline: Instant,
}

#[allow(dead_code)] //86A0 has no Host/Attempt/review/IPC caller.
impl DedicatedRun {
    pub(crate) fn native_voice_host_binding(&self) -> RunResult<NativeVoiceHostBinding> {
        let mut state = self.shared.lock();
        self.shared.refresh(&mut state);
        if state.reason.is_some() {
            return Err(DedicatedError::Closed);
        }
        if state.phase != DedicatedPhase::Ready {
            return Err(DedicatedError::NotReady);
        }
        let context = self
            .shared
            .voice_context
            .as_ref()
            .filter(|context| context.native_nonce() == self.shared.native_nonce)
            .ok_or(DedicatedError::Preparation(Error::Identity))?;
        Ok(NativeVoiceHostBinding {
            shared: Arc::downgrade(&self.shared),
            native_nonce: self.shared.native_nonce,
            profile: context.profile(),
            deadline: self.shared.deadline,
        })
    }
}

#[allow(dead_code)] // Observation only; service admission remains a later gate.
impl NativeVoiceHostBinding {
    fn current(&self) -> RunResult<Arc<Shared>> {
        let shared = self.shared.upgrade().ok_or(DedicatedError::Closed)?;
        {
            let mut state = shared.lock();
            shared.refresh(&mut state);
            if state.reason.is_some() {
                return Err(DedicatedError::Closed);
            }
            if state.phase != DedicatedPhase::Ready {
                return Err(DedicatedError::NotReady);
            }
            if shared.native_nonce != self.native_nonce
                || shared.deadline != self.deadline
                || !shared.voice_context.as_ref().is_some_and(|context| {
                    context.native_nonce() == self.native_nonce && context.profile() == self.profile
                })
            {
                return Err(DedicatedError::Preparation(Error::Identity));
            }
        }
        Ok(shared)
    }

    pub(crate) fn reserve_tcp(&self) -> RunResult<ReservedTcpQuery> {
        let shared = self.current()?;
        // The same Shared, same observer and same common reservation gate84.
        // Stop/cancel/deadline racing current() are rechecked by reserve().
        Ok(ReservedTcpQuery {
            job: shared.reserve(QueryKind::Tcp)?,
        })
    }

    pub(crate) fn reserve_voice(
        &self,
    ) -> std::result::Result<ReservedVoiceQuery, VoiceRuntimeError> {
        let shared = self.current().map_err(VoiceRuntimeError::from)?;
        voice::reserve_voice(&shared)
    }
}
