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
    policy: super::super::manifest::ObservationPolicyV2,
    deadline: Instant,
    runtime: crate::engine::connect_ready_profile::ReadyProfile,
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
            policy: context.policy(),
            deadline: self.shared.deadline,
            runtime: context.runtime()?,
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
        let runtime = self.runtime.descriptor();
        sample.profile == expected.0
            && sample.artifact_version == expected.1
            && sample.minecraft == runtime.minecraft_version
            && sample.loader == runtime.loader
            && sample.loader_version == runtime.loader_version
            && sample.binding == VoiceRuntimeBinding::Ipv4Literal
            && sample.announcement
                == match self.policy {
                    super::super::manifest::ObservationPolicyV2::LiteralOnlyFirstPersisted => {
                        VoiceRuntimeAnnouncement::DefaultUnchanged
                    }
                    super::super::manifest::ObservationPolicyV2::LiteralLoopback35554Durable => {
                        VoiceRuntimeAnnouncement::Loopback35554Durable
                    }
                }
    }

    /// Signed social policy material derived only from the same native run.
    /// A hash is descriptive authorization context, never a reconstructible binding.
    pub(crate) fn online_voice_policy(
        &self,
    ) -> std::result::Result<(&'static str, String), VoiceRuntimeError> {
        self.check_current()?;
        if self.policy != super::super::manifest::ObservationPolicyV2::LiteralLoopback35554Durable {
            return Err(VoiceRuntimeError::Context(Error::Identity));
        }
        let profile = self.identity_preimage().1;
        let shared = self.current().map_err(VoiceRuntimeError::from)?;
        let context = shared
            .voice_context
            .as_ref()
            .ok_or(VoiceRuntimeError::Closed)?;
        if context.is_v3() {
            let bytes = serde_json::to_vec(&(
                "aurora.connect.native-online-voice-source/v2",
                self.native_nonce,
                profile,
                self.runtime.version_id(),
                self.runtime.descriptor(),
                context
                    .source_manifest_digest()
                    .map_err(VoiceRuntimeError::Context)?,
                self.targets(),
                self.policy,
                self.policy.configuration_sha256(self.profile),
                super::super::digest(super::super::resources::BRIDGE_PROPERTIES),
                "127.0.0.1:35554",
                4096_u16,
                1264_u16,
                16384_u32,
                20_u32,
            ))
            .map_err(|_| VoiceRuntimeError::Context(Error::Identity))?;
            return Ok((profile, super::super::digest(&bytes)));
        }
        let bytes = serde_json::to_vec(&(
            "aurora.connect.native-online-voice-source/v1",
            self.native_nonce,
            profile,
            self.targets(),
            self.policy,
            self.policy.configuration_sha256(self.profile),
            super::super::digest(super::super::resources::BRIDGE_PROPERTIES),
            "127.0.0.1:35554",
            4096_u16,
            1264_u16,
            16384_u32,
            20_u32,
        ))
        .map_err(|_| VoiceRuntimeError::Context(Error::Identity))?;
        Ok((profile, super::super::digest(&bytes)))
    }

    pub(crate) fn online_runtime_profile(
        &self,
    ) -> std::result::Result<crate::engine::connect_ready_profile::ReadyProfile, VoiceRuntimeError>
    {
        self.check_current()?;
        Ok(self.runtime)
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
                    context.native_nonce() == self.native_nonce
                        && context.profile() == self.profile
                        && context.policy() == self.policy
                        && context
                            .runtime()
                            .is_ok_and(|runtime| runtime == self.runtime)
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

    /// Final native Host-player spawn gate. Keep this exact dedicated lifecycle
    /// locked across child registration; an observation cannot be substituted.
    pub(crate) fn with_current<T>(
        &self,
        work: impl FnOnce() -> T,
    ) -> std::result::Result<T, VoiceRuntimeError> {
        let shared = self.current().map_err(VoiceRuntimeError::from)?;
        let mut state = shared.lock();
        shared.refresh(&mut state);
        if state.reason.is_some() || state.phase != DedicatedPhase::Ready {
            return Err(VoiceRuntimeError::from(DedicatedError::Closed));
        }
        Ok(work())
    }

    pub(crate) fn reserve_voice(
        &self,
    ) -> std::result::Result<ReservedVoiceQuery, VoiceRuntimeError> {
        let shared = self.current().map_err(VoiceRuntimeError::from)?;
        voice::reserve_voice(&shared)
    }
}
