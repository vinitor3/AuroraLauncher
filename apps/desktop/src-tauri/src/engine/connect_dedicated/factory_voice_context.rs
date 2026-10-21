//! Query-only values captured from one opaque, reviewed voice preparation.
//! No Bundle/Resources/Instance, mutation guard, Recovery inventory or resource
//! owner survives in this context. Every revalidation owns only local handles.
use super::super::{
    directory_identity, file_identity, manifest, observe_file, recovery, resources, Error,
    FileIdentity, Observation, Result, MAX_JOURNAL,
};
use super::{provenance_names, Bundle, PreparedStatus};
use manifest::{StoredVoiceProfileV2, VoiceManifestV2};
use std::{
    collections::BTreeSet,
    fs::File,
    path::{Path, PathBuf},
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
    manifest_observation: Observation,
    resource_observations: Vec<(String, PathBuf, Observation)>,
    #[cfg(test)]
    test_expected_pins: Option<Vec<resources::Pin>>,
}
impl ImmutableVoiceContext {
    pub(in super::super) fn from_prepared(
        prepared: &PreparedStatus,
        native_nonce: Uuid,
    ) -> Result<Self> {
        // Kind is checked before opening any path. A V1 preparation cannot be
        // relabelled; the only other input is this execution's native nonce.
        let voice = prepared.bundle.manifest.voice_v2()?;
        let profile = voice.profile;
        Bundle::voice_shape(&prepared.bundle.manifest, profile)?;
        prepared
            .approved_files
            .require_manifest_kind(&prepared.bundle.manifest)?;
        let (manifest_observation, bytes) =
            observe_file(&prepared.bundle.path.join("bundle.json"), MAX_JOURNAL)?;
        if manifest::decode_manifest(&bytes)?.voice_v2()? != voice {
            return Err(Error::Changed);
        }
        drop(bytes);
        let result = Self {
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
            // The getter copies only names/paths/observations from Resources'
            // own verified file records. JSON never selects stage/JDK paths.
            resource_observations: prepared.resources.immutable_observations(),
            manifest_observation,
            #[cfg(test)]
            test_expected_pins: prepared.bundle.test_expected_pins.clone(),
        };
        result.validate_for_query()?;
        Ok(result)
    }

    pub(in super::super) fn profile(&self) -> StoredVoiceProfileV2 {
        self.profile
    }
    pub(in super::super) fn native_nonce(&self) -> Uuid {
        self.native_nonce
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
        resources::voice_record_shape(
            self.profile,
            &self.manifest.resources,
            &self.manifest.installed,
        )?;
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
        let (current, bytes) = observe_file(&self.bundle.path.join("bundle.json"), MAX_JOURNAL)?;
        let actual = manifest::decode_manifest(&bytes)?;
        let voice = actual.voice_v2()?;
        if voice.profile != self.profile {
            return Err(Error::Identity);
        }
        if voice != &self.manifest || current != self.manifest_observation {
            return Err(Error::Changed);
        }
        drop(bytes);

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
