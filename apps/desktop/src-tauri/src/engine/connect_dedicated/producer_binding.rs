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
    pub(crate) fn check_current(&self) -> std::result::Result<(), VoiceRuntimeError> {
        self.current().map(|_| ()).map_err(VoiceRuntimeError::from)
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Private preimage only; never a frontend projection or a constructor.
    pub(crate) fn identity_preimage(&self) -> (Uuid, &'static str) {
        (
            self.native_nonce,
            match self.profile {
                StoredVoiceProfileV2::Svc => "svc",
                StoredVoiceProfileV2::Plasmo => "plasmo",
            },
        )
    }

    pub(crate) fn copy_id(&self) -> std::result::Result<String, VoiceRuntimeError> {
        let shared = self.current().map_err(VoiceRuntimeError::from)?;
        shared
            .voice_context
            .as_ref()
            .map(|context| context.copy_id().to_owned())
            .ok_or(VoiceRuntimeError::Closed)
    }

    pub(crate) fn accepts(&self, sample: &VoiceRuntimeSample) -> bool {
        let expected = match self.profile {
            StoredVoiceProfileV2::Svc => {
                (super::super::NativeVoiceProfile::SimpleVoiceChat, "2.6.22")
            }
            StoredVoiceProfileV2::Plasmo => {
                (super::super::NativeVoiceProfile::PlasmoVoice, "2.1.17")
            }
        };
        sample.profile == expected.0
            && sample.artifact_version == expected.1
            && sample.minecraft == "1.21.1"
            && sample.loader == "fabric"
            && sample.loader_version == "0.16.14"
            && sample.binding == VoiceRuntimeBinding::Ipv4Literal
            && sample.announcement == VoiceRuntimeAnnouncement::DefaultUnchanged
    }

    pub(crate) fn targets(&self) -> (SocketAddr, SocketAddr) {
        let port = match self.profile {
            StoredVoiceProfileV2::Svc => 43002,
            StoredVoiceProfileV2::Plasmo => 43003,
        };
        (
            ENDPOINT,
            SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST), port),
        )
    }

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
