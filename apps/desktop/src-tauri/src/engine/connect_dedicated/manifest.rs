//! Closed schema dispatch on the original bounded bytes. Recognizing V2 does
//! not admit its bundle, execution, recovery or resources during checkpointB1.
use super::{
    recovery, resources, voice_profile, BundleId, Error, FileIdentity, Observation, Result,
    MAX_JOURNAL,
};
use serde::{
    de::{self, IgnoredAny, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{fmt, marker::PhantomData, path::Path};

// Copy of the original factory Manifest. Keep fields/order/serde attributes
// identical: old claims and review preimages bind these exact serialized bytes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestV1 {
    pub schema: u32,
    pub id: BundleId,
    pub lab_identity: FileIdentity,
    pub bundle_identity: FileIdentity,
    pub native_identity: FileIdentity,
    pub copy_id: String,
    pub copy_uuid: String,
    pub origin_uuid: String,
    pub copy_identity: String,
    pub origin_identity: String,
    pub origin_fingerprint: String,
    pub resources: Vec<resources::ResourceRecord>,
    pub installed: Vec<resources::ResourceRecord>,
    pub provenance: Vec<resources::ResourceRecord>,
}

/// Disk-only spelling, separate from the native caller's non-serde enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StoredVoiceProfileV2 {
    Svc,
    Plasmo,
}
impl StoredVoiceProfileV2 {
    pub(super) const fn configuration_name(self) -> &'static str {
        match self {
            Self::Svc => voice_profile::SVC_CONFIGURATION_NAME,
            Self::Plasmo => voice_profile::PLASMO_CONFIGURATION_NAME,
        }
    }
    pub(super) const fn configuration(self) -> &'static [u8] {
        match self {
            Self::Svc => voice_profile::SVC_CONFIGURATION,
            Self::Plasmo => voice_profile::PLASMO_CONFIGURATION,
        }
    }
    pub(super) const fn configuration_sha256(self) -> &'static str {
        match self {
            Self::Svc => voice_profile::SVC_CONFIGURATION_SHA256,
            Self::Plasmo => voice_profile::PLASMO_CONFIGURATION_SHA256,
        }
    }
}
impl Serialize for StoredVoiceProfileV2 {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Svc => "svc",
            Self::Plasmo => "plasmo",
        })
    }
}
impl<'de> Deserialize<'de> for StoredVoiceProfileV2 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "svc" => Ok(Self::Svc),
            "plasmo" => Ok(Self::Plasmo),
            _ => Err(serde::de::Error::custom("unsupported_native_voice_profile")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ObservationPolicyV2 {
    LiteralOnlyFirstPersisted,
    LiteralLoopback35554Durable,
}
impl ObservationPolicyV2 {
    pub(super) const fn configuration(self, profile: StoredVoiceProfileV2) -> &'static [u8] {
        match (self, profile) {
            (Self::LiteralOnlyFirstPersisted, _) => profile.configuration(),
            (Self::LiteralLoopback35554Durable, StoredVoiceProfileV2::Svc) => {
                voice_profile::SVC_BRIDGE_CONFIGURATION
            }
            (Self::LiteralLoopback35554Durable, StoredVoiceProfileV2::Plasmo) => {
                voice_profile::PLASMO_BRIDGE_CONFIGURATION
            }
        }
    }
    pub(super) const fn configuration_sha256(self, profile: StoredVoiceProfileV2) -> &'static str {
        match (self, profile) {
            (Self::LiteralOnlyFirstPersisted, _) => profile.configuration_sha256(),
            (Self::LiteralLoopback35554Durable, StoredVoiceProfileV2::Svc) => {
                voice_profile::SVC_BRIDGE_CONFIGURATION_SHA256
            }
            (Self::LiteralLoopback35554Durable, StoredVoiceProfileV2::Plasmo) => {
                voice_profile::PLASMO_BRIDGE_CONFIGURATION_SHA256
            }
        }
    }
}
impl Serialize for ObservationPolicyV2 {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::LiteralOnlyFirstPersisted => serializer.serialize_u32(1),
            Self::LiteralLoopback35554Durable => serializer.serialize_u32(2),
        }
    }
}
impl<'de> Deserialize<'de> for ObservationPolicyV2 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        match u32::deserialize(deserializer)? {
            1 => Ok(Self::LiteralOnlyFirstPersisted),
            2 => Ok(Self::LiteralLoopback35554Durable),
            _ => Err(serde::de::Error::custom(
                "unsupported_voice_observation_policy",
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct VoiceRuntimeV2 {
    pub minecraft: String,
    pub loader: String,
    pub loader_version: String,
    pub protocol: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct InitialVoiceConfigurationV2 {
    pub name: String,
    #[serde(deserialize_with = "observation_v2")]
    pub observation: Observation,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(super) struct VoiceManifestV2 {
    pub schema: u32,
    pub profile: StoredVoiceProfileV2,
    pub id: BundleId,
    pub lab_identity: FileIdentity,
    pub bundle_identity: FileIdentity,
    pub native_identity: FileIdentity,
    pub copy_id: String,
    pub copy_uuid: String,
    pub origin_uuid: String,
    pub copy_identity: String,
    pub origin_identity: String,
    pub origin_fingerprint: String,
    pub resources: Vec<resources::ResourceRecord>,
    pub installed: Vec<resources::ResourceRecord>,
    pub provenance: Vec<resources::ResourceRecord>,
    pub runtime: VoiceRuntimeV2,
    pub initial_voice_configuration: InitialVoiceConfigurationV2,
    pub observation_policy: ObservationPolicyV2,
}

// Only V2 uses these object guards. They forward MapAccess directly to the
// strict derive, retaining duplicate tokens without a Value/map or re-encoding.
// The shared V1 identity/observation/resource derives remain unchanged.
fn object_only<'de, D, T>(deserializer: D) -> std::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a closed native voice object")
        }

        fn visit_map<A: MapAccess<'de>>(
            self,
            map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            T::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectVisitor(PhantomData))
}

struct ObjectOnly<T>(T);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for ObjectOnly<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        object_only(deserializer).map(Self)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationWireV2 {
    #[serde(deserialize_with = "object_only")]
    identity: FileIdentity,
    bytes: u64,
    sha256: String,
}
fn observation_v2<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Observation, D::Error> {
    let wire: ObservationWireV2 = object_only(deserializer)?;
    Ok(Observation {
        identity: wire.identity,
        bytes: wire.bytes,
        sha256: wire.sha256,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceRecordWireV2 {
    name: String,
    #[serde(deserialize_with = "observation_v2")]
    observation: Observation,
}
fn resource_records_v2<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Vec<resources::ResourceRecord>, D::Error> {
    let records = Vec::<ObjectOnly<ResourceRecordWireV2>>::deserialize(deserializer)?;
    Ok(records
        .into_iter()
        .map(|ObjectOnly(record)| resources::ResourceRecord {
            name: record.name,
            observation: record.observation,
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VoiceManifestWireV2 {
    schema: u32,
    profile: StoredVoiceProfileV2,
    id: BundleId,
    #[serde(deserialize_with = "object_only")]
    lab_identity: FileIdentity,
    #[serde(deserialize_with = "object_only")]
    bundle_identity: FileIdentity,
    #[serde(deserialize_with = "object_only")]
    native_identity: FileIdentity,
    copy_id: String,
    copy_uuid: String,
    origin_uuid: String,
    copy_identity: String,
    origin_identity: String,
    origin_fingerprint: String,
    #[serde(deserialize_with = "resource_records_v2")]
    resources: Vec<resources::ResourceRecord>,
    #[serde(deserialize_with = "resource_records_v2")]
    installed: Vec<resources::ResourceRecord>,
    #[serde(deserialize_with = "resource_records_v2")]
    provenance: Vec<resources::ResourceRecord>,
    #[serde(deserialize_with = "object_only")]
    runtime: VoiceRuntimeV2,
    #[serde(deserialize_with = "object_only")]
    initial_voice_configuration: InitialVoiceConfigurationV2,
    observation_policy: ObservationPolicyV2,
}
impl<'de> Deserialize<'de> for VoiceManifestV2 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let wire: VoiceManifestWireV2 = object_only(deserializer)?;
        Ok(Self {
            schema: wire.schema,
            profile: wire.profile,
            id: wire.id,
            lab_identity: wire.lab_identity,
            bundle_identity: wire.bundle_identity,
            native_identity: wire.native_identity,
            copy_id: wire.copy_id,
            copy_uuid: wire.copy_uuid,
            origin_uuid: wire.origin_uuid,
            copy_identity: wire.copy_identity,
            origin_identity: wire.origin_identity,
            origin_fingerprint: wire.origin_fingerprint,
            resources: wire.resources,
            installed: wire.installed,
            provenance: wire.provenance,
            runtime: wire.runtime,
            initial_voice_configuration: wire.initial_voice_configuration,
            observation_policy: wire.observation_policy,
        })
    }
}
impl VoiceManifestV2 {
    fn validate_wire(&self) -> Result<()> {
        let profile = self.profile;
        let initial = &self.initial_voice_configuration;
        if self.schema != 2
            || self.runtime.minecraft != voice_profile::MINECRAFT
            || self.runtime.loader != voice_profile::LOADER
            || self.runtime.loader_version != voice_profile::LOADER_VERSION
            || self.runtime.protocol != voice_profile::PROTOCOL
            || initial.name != profile.configuration_name()
            || initial.observation.bytes
                != self.observation_policy.configuration(profile).len() as u64
            || initial.observation.sha256 != self.observation_policy.configuration_sha256(profile)
        {
            return Err(Error::Identity);
        }
        // This checks only the persisted initial record. The mutable file is
        // never opened here, and B1 does not confer physical V2 admission.
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ManifestKind {
    StatusV1,
    VoiceV2(StoredVoiceProfileV2),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Manifest {
    StatusV1(ManifestV1),
    VoiceV2(VoiceManifestV2),
}
impl Serialize for Manifest {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::StatusV1(value) => value.serialize(serializer),
            Self::VoiceV2(value) => value.serialize(serializer),
        }
    }
}
impl Manifest {
    pub(super) fn kind(&self) -> ManifestKind {
        match self {
            Self::StatusV1(_) => ManifestKind::StatusV1,
            Self::VoiceV2(value) => ManifestKind::VoiceV2(value.profile),
        }
    }
    pub(super) fn schema(&self) -> u32 {
        match self {
            Self::StatusV1(value) => value.schema,
            Self::VoiceV2(value) => value.schema,
        }
    }
    pub(super) fn id(&self) -> BundleId {
        match self {
            Self::StatusV1(value) => value.id,
            Self::VoiceV2(value) => value.id,
        }
    }
    pub(super) fn lab_identity(&self) -> &FileIdentity {
        match self {
            Self::StatusV1(value) => &value.lab_identity,
            Self::VoiceV2(value) => &value.lab_identity,
        }
    }
    pub(super) fn bundle_identity(&self) -> &FileIdentity {
        match self {
            Self::StatusV1(value) => &value.bundle_identity,
            Self::VoiceV2(value) => &value.bundle_identity,
        }
    }
    pub(super) fn native_identity(&self) -> &FileIdentity {
        match self {
            Self::StatusV1(value) => &value.native_identity,
            Self::VoiceV2(value) => &value.native_identity,
        }
    }
    pub(super) fn copy_id(&self) -> &str {
        match self {
            Self::StatusV1(value) => &value.copy_id,
            Self::VoiceV2(value) => &value.copy_id,
        }
    }
    pub(super) fn copy_uuid(&self) -> &str {
        match self {
            Self::StatusV1(value) => &value.copy_uuid,
            Self::VoiceV2(value) => &value.copy_uuid,
        }
    }
    pub(super) fn origin_uuid(&self) -> &str {
        match self {
            Self::StatusV1(value) => &value.origin_uuid,
            Self::VoiceV2(value) => &value.origin_uuid,
        }
    }
    pub(super) fn copy_identity(&self) -> &str {
        match self {
            Self::StatusV1(value) => &value.copy_identity,
            Self::VoiceV2(value) => &value.copy_identity,
        }
    }
    pub(super) fn origin_identity(&self) -> &str {
        match self {
            Self::StatusV1(value) => &value.origin_identity,
            Self::VoiceV2(value) => &value.origin_identity,
        }
    }
    pub(super) fn origin_fingerprint(&self) -> &str {
        match self {
            Self::StatusV1(value) => &value.origin_fingerprint,
            Self::VoiceV2(value) => &value.origin_fingerprint,
        }
    }
    pub(super) fn resources(&self) -> &[resources::ResourceRecord] {
        match self {
            Self::StatusV1(value) => &value.resources,
            Self::VoiceV2(value) => &value.resources,
        }
    }
    pub(super) fn installed(&self) -> &[resources::ResourceRecord] {
        match self {
            Self::StatusV1(value) => &value.installed,
            Self::VoiceV2(value) => &value.installed,
        }
    }
    pub(super) fn provenance(&self) -> &[resources::ResourceRecord] {
        match self {
            Self::StatusV1(value) => &value.provenance,
            Self::VoiceV2(value) => &value.provenance,
        }
    }
    pub(super) fn status_v1(&self) -> Result<&ManifestV1> {
        match self {
            Self::StatusV1(value) => Ok(value),
            Self::VoiceV2(_) => Err(Error::Identity),
        }
    }
    pub(super) fn voice_v2(&self) -> Result<&VoiceManifestV2> {
        match self {
            Self::VoiceV2(value) => Ok(value),
            Self::StatusV1(_) => Err(Error::Identity),
        }
    }
}

// This pass retains only schema. The old struct derive also accepts positional
// sequences, so the probe must preserve that V1 input form instead of requiring
// a one-field sequence. The unchanged concrete derive checks its exact length
// and every field on the SAME original bytes in the second pass.
struct SchemaProbe {
    schema: u32,
}
impl<'de> Deserialize<'de> for SchemaProbe {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct SchemaVisitor;
        impl<'de> Visitor<'de> for SchemaVisitor {
            type Value = SchemaProbe;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a manifest object or legacy positional sequence")
            }

            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut schema = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "schema" {
                        if schema.is_some() {
                            return Err(de::Error::duplicate_field("schema"));
                        }
                        schema = Some(map.next_value::<u32>()?);
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                Ok(SchemaProbe {
                    schema: schema.ok_or_else(|| de::Error::missing_field("schema"))?,
                })
            }

            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let schema = sequence
                    .next_element::<u32>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                while sequence.next_element::<IgnoredAny>()?.is_some() {}
                Ok(SchemaProbe { schema })
            }
        }
        deserializer.deserialize_any(SchemaVisitor)
    }
}

pub(super) fn decode_manifest(original_bytes: &[u8]) -> Result<Manifest> {
    if original_bytes.len() as u64 > MAX_JOURNAL {
        return Err(Error::Limit);
    }
    let probe: SchemaProbe =
        serde_json::from_slice(original_bytes).map_err(|_| Error::NeedsRecovery)?;
    let decoded = match probe.schema {
        1 => Manifest::StatusV1(
            serde_json::from_slice(original_bytes).map_err(|_| Error::NeedsRecovery)?,
        ),
        2 => Manifest::VoiceV2(
            serde_json::from_slice(original_bytes).map_err(|_| Error::NeedsRecovery)?,
        ),
        _ => return Err(Error::Identity),
    };
    if let Ok(voice) = decoded.voice_v2() {
        voice.validate_wire()?;
    }
    // No new lexical/canonical/value constraints are imposed on V1. Its old
    // bundle validator still supplies physical/resource/provenance checks.
    Ok(decoded)
}

pub(super) fn read_manifest(path: &Path) -> Result<Manifest> {
    let original_bytes = recovery::read_bounded(path, MAX_JOURNAL)?;
    decode_manifest(&original_bytes)
}

#[cfg(test)]
#[path = "manifest/tests.rs"]
mod tests;
