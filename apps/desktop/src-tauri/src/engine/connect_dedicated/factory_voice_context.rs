//! Query-only values captured from one opaque, reviewed voice preparation.
//! No Bundle/Resources/Instance, mutation guard, Recovery inventory or resource
//! owner survives here. Policy2/V3 retain immutable read-only pins after full
//! validation; mutable voice configuration always belongs to each fresh query.
use super::super::{
    directory_identity, file_identity, manifest, observe_file, recovery, resources, Error,
    FileIdentity, Observation, Result,
};
use super::{provenance_names, Bundle, PreparedStatus};
use manifest::{StoredVoiceProfileV2, VoiceManifestV2};
use std::{
    collections::BTreeSet,
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;

#[derive(Clone)]
struct DirectoryRecord {
    path: PathBuf,
    identity: FileIdentity,
}
impl DirectoryRecord {
    fn capture(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.into(),
            identity: directory_identity(path)?,
        })
    }
    fn pin(&self, pins: &mut Vec<File>) -> Result<()> {
        // Only the existing read-only OS/path primitive is reused. This does
        // not open Store, observe Recovery, enumerate config or take a guard.
        let current = recovery::pin_directory(&self.path)?;
        if file_identity(current.first().ok_or(Error::Identity)?)? != self.identity {
            return Err(Error::Changed);
        }
        pins.extend(current);
        Ok(())
    }
}

#[derive(Clone)]
pub(in super::super) struct ImmutableVoiceContext {
    native_nonce: Uuid,
    profile: StoredVoiceProfileV2,
    lab: DirectoryRecord,
    bundle: DirectoryRecord,
    native: DirectoryRecord,
    copy: DirectoryRecord,
    origin: DirectoryRecord,
    manifest: VoiceManifestV2,
    envelope: manifest::Manifest,
    immutable_pins: Option<Arc<Vec<(PathBuf, recovery::SecureFile, Observation)>>>,
    manifest_observation: Observation,
    resource_observations: Vec<(String, PathBuf, Observation)>,
    #[cfg(test)]
    test_expected_pins: Option<Vec<resources::Pin>>,
    // Drop after the immutable pins. This keeps the original V3 preparation
    // directory alive for the same Shared owner, without granting execution.
    _server_work: Option<Arc<super::super::server_preparation::PreparationDirectory>>,
}
impl ImmutableVoiceContext {
    pub(in super::super) fn from_prepared(
        prepared: &PreparedStatus,
        native_nonce: Uuid,
    ) -> Result<Self> {
        // Kind is checked before opening any path. A V1 preparation cannot be
        // relabelled; the only other input is this execution's native nonce.
        let voice = prepared.bundle.manifest.voice()?;
        let profile = voice.profile;
        Bundle::voice_shape(&prepared.bundle.manifest, profile)?;
        prepared
            .approved_files
            .require_manifest_kind(&prepared.bundle.manifest)?;
        let (manifest_observation, bytes) = observe_file(
            &prepared.bundle.path.join("bundle.json"),
            prepared.bundle.manifest.byte_limit(),
        )?;
        if manifest::decode_manifest(&bytes)?.voice()? != voice {
            return Err(Error::Changed);
        }
        drop(bytes);
        let mut result = Self {
            native_nonce,
            profile,
            lab: DirectoryRecord {
                path: prepared.bundle.lab.clone(),
                identity: voice.lab_identity.clone(),
            },
            bundle: DirectoryRecord {
                path: prepared.bundle.path.clone(),
                identity: voice.bundle_identity.clone(),
            },
            native: DirectoryRecord {
                path: prepared.bundle.native.clone(),
                identity: voice.native_identity.clone(),
            },
            // These are physical Win32 IDs of the same trusted copy/origin,
            // never the string-valued Recovery identities in the manifest.
            copy: DirectoryRecord::capture(prepared.bundle.copy.root())?,
            origin: DirectoryRecord::capture(prepared.bundle.origin.root())?,
            manifest: voice.clone(),
            envelope: prepared.bundle.manifest.clone(),
            immutable_pins: None,
            // The getter copies only names/paths/observations from Resources'
            // own verified file records. JSON never selects stage/JDK paths.
            resource_observations: prepared.resources.immutable_observations(),
            manifest_observation,
            #[cfg(test)]
            test_expected_pins: prepared.bundle.test_expected_pins.clone(),
            _server_work: prepared.resources.server_work_lifetime(),
        };
        // Acquire every immutable input under deny-write/delete handles before
        // the full validation; later queries compare those SAME live objects.
        let pins = if result.policy() == manifest::ObservationPolicyV2::LiteralLoopback35554Durable
        {
            let mut files = result
                .resource_observations
                .iter()
                .map(|(_, path, observation)| (path.clone(), observation.clone()))
                .collect::<Vec<_>>();
            files.extend(result.manifest.installed.iter().map(|record| {
                (
                    result.copy.path.join(&record.name),
                    record.observation.clone(),
                )
            }));
            files.extend(result.manifest.provenance.iter().map(|record| {
                (
                    result.native.path.join(&record.name),
                    record.observation.clone(),
                )
            }));
            files.push((
                result.bundle.path.join("bundle.json"),
                result.manifest_observation.clone(),
            ));
            Some(
                files
                    .into_iter()
                    .map(|(path, expected)| {
                        let file = recovery::secure_open(&path)?;
                        if file_identity(&file)? != expected.identity
                            || file.metadata()?.len() != expected.bytes
                        {
                            return Err(Error::Changed);
                        }
                        Ok((path, file, expected))
                    })
                    .collect::<Result<Vec<_>>>()?,
            )
        } else {
            None
        };
        result.validate_for_query()?;
        result.immutable_pins = pins.map(Arc::new);
        Ok(result)
    }

    pub(in super::super) fn runtime(
        &self,
    ) -> Result<crate::engine::connect_ready_profile::ReadyProfile> {
        self.envelope
            .server_v3()
            .map(|value| value.server.runtime())
            .unwrap_or(Ok(
                crate::engine::connect_ready_profile::ReadyProfile::Fabric1211,
            ))
    }
    pub(in super::super) fn source_manifest_digest(&self) -> Result<String> {
        Ok(super::super::digest(&self.envelope.encode_bounded()?))
    }
    pub(in super::super) fn is_v3(&self) -> bool {
        self.envelope.server_v3().is_some()
    }
    pub(in super::super) fn profile(&self) -> StoredVoiceProfileV2 {
        self.profile
    }
    pub(in super::super) fn policy(&self) -> manifest::ObservationPolicyV2 {
        self.manifest.observation_policy
    }
    pub(in super::super) fn native_nonce(&self) -> Uuid {
        self.native_nonce
    }
    pub(in super::super) fn copy_id(&self) -> &str {
        &self.manifest.copy_id
    }
    pub(in super::super) fn configuration_root(&self) -> &Path {
        &self.copy.path
    }
    pub(in super::super) fn configuration_root_identity(&self) -> &FileIdentity {
        &self.copy.identity
    }

    pub(in super::super) fn validate_for_query(&self) -> Result<()> {
        // Validate all closed sets before any record-derived join. Resource
        // source paths below originate in Resources, not in the manifest.
        if let Some(server) = self.envelope.server_v3() {
            resources::v3_record_shape(server)?;
        } else {
            resources::voice_record_shape(
                self.profile,
                &self.manifest.resources,
                &self.manifest.installed,
            )?;
        }
        let expected_provenance = provenance_names(&self.manifest.copy_id)?;
        let actual_provenance: BTreeSet<_> = self
            .manifest
            .provenance
            .iter()
            .map(|record| record.name.as_str())
            .collect();
        if self.manifest.provenance.len() != 6
            || actual_provenance.len() != 6
            || actual_provenance != expected_provenance.iter().map(String::as_str).collect()
        {
            return Err(Error::Identity);
        }
        let mut source_names = BTreeSet::new();
        if self.resource_observations.len() != self.manifest.resources.len() {
            return Err(Error::Identity);
        }
        for (name, _, expected) in &self.resource_observations {
            if !source_names.insert(name.as_str())
                || !self
                    .manifest
                    .resources
                    .iter()
                    .any(|record| record.name == *name && record.observation == *expected)
            {
                return Err(Error::Identity);
            }
        }

        // Pins are local to this revalidation and remain alive until its last
        // read. No game/config/world/log/defaultconfigs traversal takes place.
        let mut pins = Vec::new();
        for directory in [
            &self.lab,
            &self.bundle,
            &self.native,
            &self.copy,
            &self.origin,
        ] {
            directory.pin(&mut pins)?;
        }
        let (current, bytes) = observe_file(
            &self.bundle.path.join("bundle.json"),
            self.envelope.byte_limit(),
        )?;
        let actual = manifest::decode_manifest(&bytes)?;
        let voice = actual.voice()?;
        if voice.profile != self.profile {
            return Err(Error::Identity);
        }
        if voice != &self.manifest
            || actual != self.envelope
            || current != self.manifest_observation
        {
            return Err(Error::Changed);
        }
        drop(bytes);

        if let Some(held) = &self.immutable_pins {
            for (path, file, expected) in held.iter() {
                let current = recovery::secure_open(path)?;
                if file_identity(&current)? != expected.identity
                    || current.metadata()?.len() != expected.bytes
                    || file_identity(file)? != expected.identity
                    || file.metadata()?.len() != expected.bytes
                {
                    return Err(Error::Changed);
                }
            }
            if let Some(server) = self.envelope.server_v3() {
                resources::v3_inventory(&self.copy.path, server)?;
            } else {
                resources::voice_inventory(&self.copy.path, self.profile)?;
            }
            return Ok(());
        }
        for (_, path, expected) in &self.resource_observations {
            let current = observe_file(path, expected.bytes)
                .map_err(|error| {
                    if error == Error::Limit {
                        Error::Changed
                    } else {
                        error
                    }
                })?
                .0;
            if &current != expected {
                return Err(Error::Changed);
            }
        }
        if let Some(server) = self.envelope.server_v3() {
            resources::validate_v3_artifacts(&self.copy.path, server)?;
        } else {
            let compiled = resources::compiled_voice_pins(self.profile);
            #[cfg(test)]
            let expected = self.test_expected_pins.as_deref().unwrap_or(&compiled);
            #[cfg(not(test))]
            let expected = compiled.as_slice();
            // This checks only the two compiled immutable catalogs mods/libraries,
            // including extra/missing names. It is not Recovery's tree inventory.
            resources::validate_voice_artifacts(
                &self.copy.path,
                self.profile,
                &self.manifest.resources,
                &self.manifest.installed,
                expected,
            )?;
        }
        for record in &self.manifest.provenance {
            if record.observation.bytes > 1024 * 1024 {
                return Err(Error::Identity);
            }
            if observe_file(&self.native.path.join(&record.name), 1024 * 1024)?.0
                != record.observation
            {
                return Err(Error::Changed);
            }
        }
        drop(pins);
        Ok(())
    }
}

#[cfg(test)]
#[path = "factory_voice_context/tests.rs"]
mod tests;
