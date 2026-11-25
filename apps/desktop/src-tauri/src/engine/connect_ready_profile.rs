//! Closed profiles for explicitly reviewed ready-copy launches. A descriptor is
//! a requirement, never a proof of files, Java, transport or permission to spawn.
use super::{connect::ReadyGameLaunchError as Error, recovery, Instance};
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadyProfile {
    Fabric1211,
    Fabric1201,
    Fabric1192,
    Fabric1165,
    Forge1211,
    Forge1201,
    Forge1192,
    Forge1165,
    Forge1122,
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
    pub const ALL: [Self; 9] = [
        Self::Fabric1211,
        Self::Fabric1201,
        Self::Fabric1192,
        Self::Fabric1165,
        Self::Forge1211,
        Self::Forge1201,
        Self::Forge1192,
        Self::Forge1165,
        Self::Forge1122,
    ];

    pub fn from_version_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|profile| profile.version_id() == id)
    }
    pub fn version_id(self) -> &'static str {
        match self {
            Self::Fabric1211 => "fabric-loader-0.16.14-1.21.1",
            Self::Fabric1201 => "fabric-loader-0.16.14-1.20.1",
            Self::Fabric1192 => "fabric-loader-0.16.14-1.19.2",
            Self::Fabric1165 => "fabric-loader-0.13.3-1.16.5",
            Self::Forge1211 => "1.21.1-forge-52.1.12",
            Self::Forge1201 => "1.20.1-forge-47.4.23",
            Self::Forge1192 => "1.19.2-forge-43.4.16",
            Self::Forge1165 => "1.16.5-forge-36.2.42",
            Self::Forge1122 => "1.12.2-forge-14.23.5.2860",
        }
    }
    pub fn descriptor(self) -> ReadyProfileDescriptor {
        let (minecraft_version, loader, loader_version, java_major) = match self {
            Self::Fabric1211 => ("1.21.1", "fabric", "0.16.14", 21),
            Self::Fabric1201 => ("1.20.1", "fabric", "0.16.14", 17),
            Self::Fabric1192 => ("1.19.2", "fabric", "0.16.14", 17),
            Self::Fabric1165 => ("1.16.5", "fabric", "0.13.3", 8),
            Self::Forge1211 => ("1.21.1", "forge", "52.1.12", 21),
            Self::Forge1201 => ("1.20.1", "forge", "47.4.23", 17),
            Self::Forge1192 => ("1.19.2", "forge", "43.4.16", 17),
            Self::Forge1165 => ("1.16.5", "forge", "36.2.42", 8),
            Self::Forge1122 => ("1.12.2", "forge", "14.23.5.2860", 8),
        };
        ReadyProfileDescriptor {
            minecraft_version,
            loader,
            loader_version,
            java_major,
            core_version: "1.1.7",
            skins_version: "0.3.4",
        }
    }
    #[cfg(feature = "connect-lab")]
    pub(super) fn main_class(self) -> &'static str {
        match self {
            Self::Fabric1211 | Self::Fabric1201 | Self::Fabric1192 | Self::Fabric1165 => {
                "net.fabricmc.loader.impl.launch.knot.KnotClient"
            }
            Self::Forge1211 => "net.minecraftforge.bootstrap.ForgeBootstrap",
            Self::Forge1201 | Self::Forge1192 => "cpw.mods.bootstraplauncher.BootstrapLauncher",
            Self::Forge1165 => "cpw.mods.modlauncher.Launcher",
            Self::Forge1122 => "net.minecraft.launchwrapper.Launch",
        }
    }
    #[cfg(feature = "connect-lab")]
    pub(super) fn uses_quick_play(self) -> bool {
        matches!(
            self,
            Self::Fabric1211 | Self::Fabric1201 | Self::Forge1211 | Self::Forge1201
        )
    }
    #[cfg(feature = "connect-lab")]
    pub(super) fn uses_forge_module_path(self) -> bool {
        matches!(self, Self::Forge1201 | Self::Forge1192)
    }
    pub fn core_file(self) -> &'static str {
        match self {
            Self::Fabric1211 => "mods/aurora-core-fabric-1.21.1.jar",
            Self::Fabric1201 => "mods/aurora-core-fabric-1.20.1.jar",
            Self::Fabric1192 => "mods/aurora-core-fabric-1.19.2.jar",
            Self::Fabric1165 => "mods/aurora-core-fabric-1.16.5.jar",
            Self::Forge1211 => "mods/aurora-core-forge-1.21.1.jar",
            Self::Forge1201 => "mods/aurora-core-forge-1.20.1.jar",
            Self::Forge1192 => "mods/aurora-core-forge-1.19.2.jar",
            Self::Forge1165 => "mods/aurora-core-forge-1.16.5.jar",
            Self::Forge1122 => "mods/aurora-core-forge-1.12.2.jar",
        }
    }
    pub fn skins_file(self) -> &'static str {
        match self {
            Self::Fabric1211 => "mods/aurora-skins-fabric-1.21.1.jar",
            Self::Fabric1201 => "mods/aurora-skins-fabric-1.20.1.jar",
            Self::Fabric1192 => "mods/aurora-skins-fabric-1.19.2.jar",
            Self::Fabric1165 => "mods/aurora-skins-fabric-1.16.5.jar",
            Self::Forge1211 => "mods/aurora-skins-forge-1.21.1.jar",
            Self::Forge1201 => "mods/aurora-skins-forge-1.20.1.jar",
            Self::Forge1192 => "mods/aurora-skins-forge-1.19.2.jar",
            Self::Forge1165 => "mods/aurora-skins-forge-1.16.5.jar",
            Self::Forge1122 => "mods/aurora-skins-forge-1.12.2.jar",
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
    let runtime = profile.descriptor();
    for document in documents {
        if document
            .get("minecraft")
            .is_some_and(|v| v.as_str() != Some(runtime.minecraft_version))
            || document
                .get("loader")
                .is_some_and(|v| v.as_str() != Some(runtime.loader))
            || document
                .get("loaderVersion")
                .is_some_and(|v| v.as_str() != Some(runtime.loader_version))
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
        assert_eq!(
            matrix["targets"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|target| target["connectReady"] != false)
                .count(),
            ReadyProfile::ALL.len()
        );
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
            "fabric-loader-0.16.13-1.20.1",
            "1.20.1-forge-47.4.22",
            "../fabric-loader-0.16.14-1.20.1",
            "1.20.1",
        ] {
            assert!(ReadyProfile::from_version_id(id).is_none());
        }
    }
    #[test]
    fn native_ready_markers_must_select_one_consistent_profile() {
        for profile in ReadyProfile::ALL {
            let runtime = profile.descriptor();
            let selected = json!({"versionId":profile.version_id(),"minecraft":runtime.minecraft_version,"loader":runtime.loader,"loaderVersion":runtime.loader_version});
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
                json!({"minecraft":"1.18.2"}),
                json!({"loader":"neoforge"}),
                json!({"loaderVersion":"unknown"}),
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
