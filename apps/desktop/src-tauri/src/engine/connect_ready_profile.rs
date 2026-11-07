//! Closed profiles for explicitly reviewed ready-copy launches. A descriptor is
//! a requirement, never a proof of files, Java, transport or permission to spawn.
use super::{connect::ReadyGameLaunchError as Error, recovery, Instance};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadyProfile {
    Fabric1211,
    Fabric1201,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyProfileDescriptor {
    pub minecraft_version: &'static str,
    pub loader: &'static str,
    pub loader_version: &'static str,
    pub java_major: u32,
    pub core_version: &'static str,
    pub skins_version: &'static str,
}

impl ReadyProfile {
    pub const ALL: [Self; 2] = [Self::Fabric1211, Self::Fabric1201];
    pub fn from_version_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|profile| profile.version_id() == id)
    }
    pub fn version_id(self) -> &'static str {
        match self {
            Self::Fabric1211 => "fabric-loader-0.16.14-1.21.1",
            Self::Fabric1201 => "fabric-loader-0.16.14-1.20.1",
        }
    }
    pub fn descriptor(self) -> ReadyProfileDescriptor {
        let (minecraft_version, java_major) = match self {
            Self::Fabric1211 => ("1.21.1", 21),
            Self::Fabric1201 => ("1.20.1", 17),
        };
        ReadyProfileDescriptor {
            minecraft_version,
            loader: "fabric",
            loader_version: "0.16.14",
            java_major,
            core_version: "1.1.5",
            skins_version: "0.3.1",
        }
    }
    pub fn core_file(self) -> &'static str {
        match self {
            Self::Fabric1211 => "mods/aurora-core-fabric-1.21.1.jar",
            Self::Fabric1201 => "mods/aurora-core-fabric-1.20.1.jar",
        }
    }
    pub fn skins_file(self) -> &'static str {
        match self {
            Self::Fabric1211 => "mods/aurora-skins-fabric-1.21.1.jar",
            Self::Fabric1201 => "mods/aurora-skins-fabric-1.20.1.jar",
        }
    }
}

/// Read only known markers. The caller must independently pin and compare these
/// bytes in its readiness proof; this helper alone grants no authority.
pub fn select_ready_profile(instance: &Instance) -> Result<ReadyProfile, Error> {
    let mut documents = Vec::new();
    for relative in ["aurora-instance.json", "aurora-modpack.json"] {
        let path =
            recovery::path(instance.root(), relative).map_err(|_| Error::InstanceNotReady)?;
        if path.try_exists().map_err(|_| Error::InstanceNotReady)? {
            let bytes =
                recovery::read_bounded(&path, 1024 * 1024).map_err(|_| Error::InstanceNotReady)?;
            documents.push(serde_json::from_slice(&bytes).map_err(|_| Error::UnsupportedRuntime)?);
        }
    }
    profile_from_metadata(&documents)
}

pub(crate) fn profile_from_metadata(documents: &[Value]) -> Result<ReadyProfile, Error> {
    let mut selected = None;
    for document in documents {
        if !document.is_object() {
            return Err(Error::UnsupportedRuntime);
        }
        if let Some(id) = document.get("versionId") {
            let profile = id
                .as_str()
                .and_then(ReadyProfile::from_version_id)
                .ok_or(Error::UnsupportedRuntime)?;
            if selected.is_some_and(|previous| previous != profile) {
                return Err(Error::UnsupportedRuntime);
            }
            selected = Some(profile);
        }
    }
    let profile = selected.ok_or(Error::UnsupportedRuntime)?;
    for document in documents {
        if document
            .get("minecraft")
            .is_some_and(|v| v.as_str() != Some(profile.descriptor().minecraft_version))
            || document
                .get("loader")
                .is_some_and(|v| v.as_str() != Some("fabric"))
        {
            return Err(Error::UnsupportedRuntime);
        }
    }
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn ready_profiles_are_exact_existing_matrix_targets() {
        let matrix: Value = serde_json::from_str(include_str!(
            "../../../../../scripts/active-mod-matrix.json"
        ))
        .unwrap();
        for profile in ReadyProfile::ALL {
            let descriptor = profile.descriptor();
            assert!(matrix["targets"]
                .as_array()
                .unwrap()
                .iter()
                .any(|target| target["minecraft"] == descriptor.minecraft_version
                    && target["loader"] == descriptor.loader
                    && target["loaderVersion"] == descriptor.loader_version
                    && target["javaRuntime"] == descriptor.java_major));
            assert_eq!(
                ReadyProfile::from_version_id(profile.version_id()),
                Some(profile)
            );
        }
        for id in [
            "fabric-loader-0.16.14-1.19.2",
            "fabric-loader-0.16.13-1.20.1",
            "1.20.1-forge-47.4.23",
            "../fabric-loader-0.16.14-1.20.1",
            "1.20.1",
        ] {
            assert!(ReadyProfile::from_version_id(id).is_none());
        }
    }
    #[test]
    fn native_ready_markers_must_select_one_consistent_profile() {
        for profile in ReadyProfile::ALL {
            let selected = json!({"versionId":profile.version_id(),"minecraft":profile.descriptor().minecraft_version,"loader":"fabric"});
            assert_eq!(
                profile_from_metadata(std::slice::from_ref(&selected)),
                Ok(profile)
            );
            assert_eq!(
                profile_from_metadata(&[
                    selected.clone(),
                    json!({"minecraft":profile.descriptor().minecraft_version})
                ]),
                Ok(profile)
            );
            for other in [
                json!({"versionId":"unknown"}),
                json!({"minecraft":"1.19.2"}),
                json!({"loader":"forge"}),
                json!(null),
                json!({"versionId":7}),
            ] {
                assert_eq!(
                    profile_from_metadata(&[selected.clone(), other]),
                    Err(Error::UnsupportedRuntime)
                );
            }
        }
        assert_eq!(
            profile_from_metadata(&[json!({"minecraft":"1.20.1","loader":"fabric"})]),
            Err(Error::UnsupportedRuntime)
        );
        assert_eq!(
            profile_from_metadata(&[
                json!({"versionId":ReadyProfile::Fabric1201.version_id()}),
                json!({"versionId":ReadyProfile::Fabric1211.version_id()})
            ]),
            Err(Error::UnsupportedRuntime)
        );
    }
}
