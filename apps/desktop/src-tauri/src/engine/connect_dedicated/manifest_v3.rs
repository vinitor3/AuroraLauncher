//! Version three is a distinct envelope. Its embedded common voice fields never
//! relax the standalone V2 codec or grant a persisted document execution authority.
use super::super::server_preparation::ServerPreparationProvenance;
use super::*;
use crate::engine::connect_ready_profile::ReadyProfile;

pub(in super::super) const MAX_V3_MANIFEST_BYTES: u64 = 512 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct ServerRuntimeV3 {
    pub runtime_profile: String,
    pub java_major: u32,
    pub recipe_args: Vec<String>,
    #[serde(deserialize_with = "object_only")]
    pub provenance: ServerPreparationProvenance,
}
impl ServerRuntimeV3 {
    pub(in super::super) fn runtime(&self) -> Result<ReadyProfile> {
        ReadyProfile::from_version_id(&self.runtime_profile).ok_or(Error::Identity)
    }
    pub(in super::super) fn runtime_metadata(&self) -> Result<VoiceRuntimeV2> {
        let runtime = self.runtime()?;
        let descriptor = runtime.descriptor();
        Ok(VoiceRuntimeV2 {
            minecraft: descriptor.minecraft_version.into(),
            loader: descriptor.loader.into(),
            loader_version: descriptor.loader_version.into(),
            protocol: crate::engine::connect_status_profile::protocol_for_ready_profile(runtime)
                .ok_or(Error::Identity)?,
        })
    }
    pub(in super::super) fn validate(&self, voice: &VoiceManifestV2) -> Result<()> {
        let runtime = self.runtime()?;
        if self.java_major != runtime.descriptor().java_major
            || voice.runtime != self.runtime_metadata()?
            || self.recipe_args.is_empty()
            || self.recipe_args.len() > 256
            || self
                .recipe_args
                .iter()
                .any(|arg| arg.is_empty() || arg.len() > 8192 || arg.chars().any(char::is_control))
            || self.recipe_args.iter().map(String::len).sum::<usize>() > 32 * 1024
        {
            return Err(Error::Identity);
        }
        let profile = match voice.profile {
            StoredVoiceProfileV2::Svc => super::super::NativeVoiceProfile::SimpleVoiceChat,
            StoredVoiceProfileV2::Plasmo => super::super::NativeVoiceProfile::PlasmoVoice,
        };
        let artifacts = voice
            .resources
            .iter()
            .filter(|record| !record.name.starts_with("jdk/"))
            .cloned()
            .collect::<Vec<_>>();
        self.provenance
            .validate(runtime, profile, &artifacts, &self.recipe_args)
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(in super::super) struct VoiceManifestV3 {
    pub schema: u32,
    pub voice: VoiceManifestV2,
    pub server: ServerRuntimeV3,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    schema: u32,
    voice: VoiceManifestV2,
    #[serde(deserialize_with = "object_only")]
    server: ServerRuntimeV3,
}
impl<'de> Deserialize<'de> for VoiceManifestV3 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value: Wire = object_only(deserializer)?;
        Ok(Self {
            schema: value.schema,
            voice: value.voice,
            server: value.server,
        })
    }
}
impl VoiceManifestV3 {
    pub(in super::super) fn validate_wire(&self) -> Result<()> {
        let voice = &self.voice;
        let profile = voice.profile;
        let initial = &voice.initial_voice_configuration;
        if self.schema != 3
            || voice.schema != 2
            || voice.observation_policy != ObservationPolicyV2::LiteralLoopback35554Durable
            || initial.name != profile.configuration_name()
            || initial.observation.bytes
                != voice.observation_policy.configuration(profile).len() as u64
            || initial.observation.sha256 != voice.observation_policy.configuration_sha256(profile)
            || voice.provenance.len() != 6
            || voice.installed.is_empty()
            || voice.installed.len() > 512
            || voice.resources.len() != voice.installed.len() + 2
        {
            return Err(Error::Identity);
        }
        self.server.validate(voice)
    }
}
