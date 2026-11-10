//! One bounded voice observation of the same original dedicated run. This
//! source contains no Child, proof, resources, guard, pins or restore authority.
use super::*;
use crate::engine::connect_discovery::UdpLoopbackRepresentation;
use crate::engine::connect_voice::dedicated::{
    manifest::StoredVoiceProfileV2,
    runtime_config::{PinnedRuntimeConfiguration, RuntimeConfigError},
    voice_profile::NativeVoiceProfile,
};
use std::net::SocketAddrV4;

#[derive(Clone)]
pub(crate) struct DedicatedVoiceSource {
    shared: Weak<Shared>,
}

/// A consumable reservation of the original shared slot, including its final
/// context identity. No clone, serde or observer-only completion shortcut.
pub(crate) struct ReservedVoiceQuery {
    job: QueryJob,
    expected: Option<(Uuid, StoredVoiceProfileV2)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VoiceRuntimeError {
    Closed,
    Busy,
    NotReady,
    ConfigurationMissing,
    ConfigurationUnsafe,
    ConfigurationLimit,
    ConfigurationIncompatible,
    ConfigurationChanged,
    ConfigurationIo,
    Context(Error),
    Source(OwnedRunError),
    UnsupportedBinding,
}
impl From<RuntimeConfigError> for VoiceRuntimeError {
    fn from(error: RuntimeConfigError) -> Self {
        match error {
            RuntimeConfigError::Missing => Self::ConfigurationMissing,
            RuntimeConfigError::UnsafePath => Self::ConfigurationUnsafe,
            RuntimeConfigError::Limit => Self::ConfigurationLimit,
            RuntimeConfigError::Incompatible => Self::ConfigurationIncompatible,
            RuntimeConfigError::Changed => Self::ConfigurationChanged,
            RuntimeConfigError::Io => Self::ConfigurationIo,
        }
    }
}
impl From<DedicatedError> for VoiceRuntimeError {
    fn from(error: DedicatedError) -> Self {
        match error {
            DedicatedError::Closed | DedicatedError::Cancelled => Self::Closed,
            DedicatedError::Busy => Self::Busy,
            DedicatedError::NotReady => Self::NotReady,
            DedicatedError::Preparation(error) => Self::Context(error),
            DedicatedError::Source(error) => Self::Source(error),
            DedicatedError::Thread => Self::Context(Error::Io),
            #[cfg(test)]
            DedicatedError::Internal => Self::Context(Error::Identity),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VoiceRuntimeBinding {
    Ipv4Literal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VoiceRuntimeAnnouncement {
    DefaultUnchanged,
    Loopback35554Durable,
}
/// Redacted data only. No serde, path, identity, hash, credential or capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VoiceRuntimeSample {
    pub profile: NativeVoiceProfile,
    pub minecraft: &'static str,
    pub loader: &'static str,
    pub loader_version: &'static str,
    pub artifact_version: &'static str,
    pub binding: VoiceRuntimeBinding,
    pub announcement: VoiceRuntimeAnnouncement,
}

#[allow(dead_code)] // No UI/IPC caller or service admission in this checkpoint.
impl DedicatedRun {
    pub(crate) fn voice_source(&self) -> Option<DedicatedVoiceSource> {
        self.shared.voice_context.as_ref()?;
        Some(DedicatedVoiceSource {
            shared: Arc::downgrade(&self.shared),
        })
    }
}
#[allow(dead_code)]
impl DedicatedVoiceSource {
    pub(crate) fn observe(&self) -> std::result::Result<VoiceRuntimeSample, VoiceRuntimeError> {
        self.reserve_voice()?.execute()
    }
    pub(crate) fn reserve_voice(
        &self,
    ) -> std::result::Result<ReservedVoiceQuery, VoiceRuntimeError> {
        let shared = self.shared.upgrade().ok_or(VoiceRuntimeError::Closed)?;
        reserve_voice(&shared)
    }
}

pub(super) fn reserve_voice(
    shared: &Arc<Shared>,
) -> std::result::Result<ReservedVoiceQuery, VoiceRuntimeError> {
    reserve_voice_checked(shared, None)
}

pub(super) fn reserve_voice_for_claim(
    shared: &Arc<Shared>,
    claim: binding::NativeVoiceClaim,
) -> std::result::Result<ReservedVoiceQuery, VoiceRuntimeError> {
    reserve_voice_checked(shared, Some(claim))
}

fn reserve_voice_checked(
    shared: &Arc<Shared>,
    claim: Option<binding::NativeVoiceClaim>,
) -> std::result::Result<ReservedVoiceQuery, VoiceRuntimeError> {
    shared.hooks.at(ProducerPoint::VoiceBeforeReserve);
    // QueryKind::Voice still uses the existing reserve body and QueryJob84.
    let job = match claim {
        Some(claim) => shared.reserve_for_claim(QueryKind::Voice, claim),
        None => shared.reserve(QueryKind::Voice),
    }
    .map_err(VoiceRuntimeError::from)?;
    let expected = shared
        .voice_context
        .as_ref()
        .map(|context| (context.native_nonce(), context.profile()));
    Ok(ReservedVoiceQuery { job, expected })
}

#[allow(dead_code)] // Private reserve/execute API; no caller dispatch in86A0.
impl ReservedVoiceQuery {
    pub(crate) fn execute(self) -> std::result::Result<VoiceRuntimeSample, VoiceRuntimeError> {
        let shared = &self.job.shared;
        // Every reserved outcome, including a configuration/context/source
        // error, reaches the original final gate. The inner owns all temporary
        // context values and C0 reads, and drops them before returning here.
        let result = observe_inner(shared, &self.job.observer);
        shared.hooks.at(ProducerPoint::VoiceBeforeFinalGate);
        let mut state = shared.lock();
        shared.refresh(&mut state);
        let current = shared
            .voice_context
            .as_ref()
            .map(|context| (context.native_nonce(), context.profile()));
        if state.reason.is_some()
            || self.expected.is_none()
            || current != self.expected
            || !current.is_some_and(|(nonce, _)| nonce == shared.native_nonce)
        {
            return Err(VoiceRuntimeError::Closed);
        }
        result
    }
}

fn observe_inner(
    shared: &Shared,
    observer: &OwnedRunObserver,
) -> std::result::Result<VoiceRuntimeSample, VoiceRuntimeError> {
    let context = shared
        .voice_context
        .as_ref()
        .ok_or(VoiceRuntimeError::Context(Error::Identity))?
        .clone();
    context
        .validate_for_query()
        .map_err(VoiceRuntimeError::Context)?;
    observer
        .observe_tcp(ENDPOINT)
        .map_err(VoiceRuntimeError::Source)?;
    let first = PinnedRuntimeConfiguration::pin_with_policy(
        context.configuration_root(),
        context.configuration_root_identity(),
        context.profile(),
        context.policy(),
    )?
    .read_sample()?;
    shared.hooks.at(ProducerPoint::VoiceAfterConfigurationA);
    let port = match context.profile() {
        StoredVoiceProfileV2::Svc => 43002,
        StoredVoiceProfileV2::Plasmo => 43003,
    };
    let binding = observer
        .observe_udp_loopback_binding(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
        .map_err(VoiceRuntimeError::Source)?;
    if binding != UdpLoopbackRepresentation::Ipv4Literal {
        return Err(VoiceRuntimeError::UnsupportedBinding);
    }
    shared.hooks.at(ProducerPoint::VoiceAfterPositiveUdp);
    let second = PinnedRuntimeConfiguration::pin_with_policy(
        context.configuration_root(),
        context.configuration_root_identity(),
        context.profile(),
        context.policy(),
    )?
    .read_sample()?;
    first.require_same_revision(&second)?;
    observer
        .observe_tcp(ENDPOINT)
        .map_err(VoiceRuntimeError::Source)?;
    context
        .validate_for_query()
        .map_err(VoiceRuntimeError::Context)?;
    let (profile, artifact_version) = match context.profile() {
        StoredVoiceProfileV2::Svc => (NativeVoiceProfile::SimpleVoiceChat, "2.6.22"),
        StoredVoiceProfileV2::Plasmo => (NativeVoiceProfile::PlasmoVoice, "2.1.17"),
    };
    let runtime = context
        .runtime()
        .map_err(VoiceRuntimeError::Context)?
        .descriptor();
    Ok(VoiceRuntimeSample {
        profile,
        minecraft: runtime.minecraft_version,
        loader: runtime.loader,
        loader_version: runtime.loader_version,
        artifact_version,
        binding: VoiceRuntimeBinding::Ipv4Literal,
        announcement: match context.policy() {
            super::super::manifest::ObservationPolicyV2::LiteralOnlyFirstPersisted => {
                VoiceRuntimeAnnouncement::DefaultUnchanged
            }
            super::super::manifest::ObservationPolicyV2::LiteralLoopback35554Durable => {
                VoiceRuntimeAnnouncement::Loopback35554Durable
            }
        },
    })
}

/// The V3 producer borrows its already-owned Readiness QueryJob. This does not
/// expose a pre-Ready reservation or manufacture a second observation owner.
pub(in super::super) fn observe_v3_readiness(
    job: &QueryJob,
) -> std::result::Result<VoiceRuntimeSample, VoiceRuntimeError> {
    let shared = &job.shared;
    if !shared
        .voice_context
        .as_ref()
        .is_some_and(|context| context.is_v3() && context.native_nonce() == shared.native_nonce)
    {
        return Err(VoiceRuntimeError::Context(Error::Identity));
    }
    let result = observe_inner(shared, &job.observer);
    let mut state = shared.lock();
    shared.refresh(&mut state);
    if state.reason.is_some() {
        return Err(VoiceRuntimeError::Closed);
    }
    result
}

// This closed fixture plan changes only its OWN argv, never RecipeV1 or the V1
// fixture protocol. The production voice builder/review still run first.
#[cfg(test)]
pub(in super::super) fn plan_voice_for_test(
    prepared: factory::PreparedStatus,
    control: SocketAddr,
    nonce: Uuid,
    hold_stop: bool,
    limits: TestLimits,
    hooks: Arc<TestHooks>,
) -> RunResult<DedicatedPlan> {
    if control.ip() != std::net::IpAddr::V4(Ipv4Addr::LOCALHOST)
        || control.port() == 0
        || !limits.valid()
    {
        return Err(DedicatedError::Preparation(Error::Resources));
    }
    let profile = prepared.bundle.manifest.voice()?.profile;
    let mut recipe = Recipe::voice_production(&prepared)?;
    recipe.executable = std::env::current_exe().map_err(|_| DedicatedError::Internal)?;
    recipe.args = vec![
        "--ignored".into(), "--exact".into(),
        "engine::connect_voice::dedicated::tests::voice_producer_fixture::dedicated_voice_child_fixture".into(),
        "--nocapture".into(), "--test-threads=1".into(),
    ];
    recipe
        .environment
        .insert("AURORA_NATIVE_VOICE_TEST_NONCE".into(), nonce.to_string());
    recipe.environment.insert(
        "AURORA_NATIVE_VOICE_TEST_CONTROL".into(),
        control.to_string(),
    );
    recipe.environment.insert(
        "AURORA_NATIVE_VOICE_TEST_PROFILE".into(),
        match profile {
            StoredVoiceProfileV2::Svc => "svc",
            StoredVoiceProfileV2::Plasmo => "plasmo",
        }
        .into(),
    );
    recipe.environment.insert(
        "AURORA_NATIVE_VOICE_TEST_HOLD_STOP".into(),
        if hold_stop { "1" } else { "0" }.into(),
    );
    recipe.environment.insert(
        "AURORA_NATIVE_VOICE_TEST_ANNOUNCED".into(),
        if prepared.bundle.manifest.voice()?.observation_policy
            == super::super::manifest::ObservationPolicyV2::LiteralLoopback35554Durable
        {
            "1"
        } else {
            "0"
        }
        .into(),
    );
    recipe.synthetic = true;
    recipe.limits = limits;
    make_plan(prepared, recipe, hooks)
}
