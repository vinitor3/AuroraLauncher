//! Read-only observation of the installed inputs used for an owned launch.
//! This is not a runtime attestation, mod compatibility claim or launch permit.
use super::{recovery, Instance, VersionLaunchSpec};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusRuntimeSummary {
    pub minecraft_version: &'static str,
    pub loader: &'static str,
    pub loader_version: &'static str,
    pub status_protocol: i32,
    pub evidence: &'static str,
}

/// No Deserialize or public field constructor: only the native installed-profile
/// inspector creates this observation. Retain it with the exact spawned Child.
#[derive(Clone)]
pub struct StatusRuntimeProfile {
    summary: StatusRuntimeSummary,
    fingerprint: String,
}
impl StatusRuntimeProfile {
    pub fn summary(&self) -> StatusRuntimeSummary {
        self.summary.clone()
    }
    /// Native review preimage only; this is never included in an IPC projection.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

// Protocols: PrismarineJS/minecraft-data protocolVersions.json at
// 9b995542ab4a17837dee785439954e43d031b33e. This closed set must match the matrix.
const TARGETS: [(&str, &str, &str, i32); 9] = [
    ("1.12.2", "forge", "14.23.5.2860", 340),
    ("1.16.5", "fabric", "0.13.3", 754),
    ("1.16.5", "forge", "36.2.42", 754),
    ("1.19.2", "fabric", "0.16.14", 760),
    ("1.19.2", "forge", "43.4.16", 760),
    ("1.20.1", "fabric", "0.16.14", 763),
    ("1.20.1", "forge", "47.4.23", 763),
    ("1.21.1", "fabric", "0.16.14", 767),
    ("1.21.1", "forge", "52.1.12", 767),
];

pub fn protocol_for_ready_profile(
    profile: super::connect_ready_profile::ReadyProfile,
) -> Option<i32> {
    let runtime = profile.descriptor();
    TARGETS
        .iter()
        .find_map(|&(minecraft, loader, version, protocol)| {
            (minecraft == runtime.minecraft_version
                && loader == runtime.loader
                && version == runtime.loader_version)
                .then_some(protocol)
        })
}

fn component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// Called under the existing instance mutation lock, before spawn. Failure only
/// disables this read-only discovery query; it does not repair or change launch.
/// Never pass the renderer's claimed Minecraft version to this function.
pub fn inspect_launch_profile(
    instance: &Instance,
    spec: &VersionLaunchSpec,
) -> Option<StatusRuntimeProfile> {
    let mut id = spec.version_name.clone();
    let mut seen = BTreeSet::new();
    let mut profiles = Vec::new();
    let mut fingerprint = Sha256::new();
    loop {
        if !component(&id) || profiles.len() >= 8 || !seen.insert(id.clone()) {
            return None;
        }
        let path = recovery::path(instance.root(), &format!("versions/{id}/{id}.json")).ok()?;
        let bytes = recovery::read_bounded(&path, 1024 * 1024).ok()?;
        let profile: Value = serde_json::from_slice(&bytes).ok()?;
        if profile["id"].as_str() != Some(id.as_str()) || profile.get("jar").is_some() {
            return None;
        }
        fingerprint.update((id.len() as u64).to_le_bytes());
        fingerprint.update(id.as_bytes());
        fingerprint.update(Sha256::digest(&bytes));
        let parent = match profile.get("inheritsFrom") {
            Some(Value::String(parent)) => Some(parent.clone()),
            Some(_) => return None,
            None => None,
        };
        profiles.push(profile);
        match parent {
            Some(parent) => id = parent,
            None => break,
        }
    }
    let base = profiles.last()?;
    if profiles.len() < 2 || base["mainClass"] != "net.minecraft.client.main.Main" {
        return None;
    }
    let resolved_main = profiles.iter().find_map(|p| p["mainClass"].as_str())?;
    if resolved_main != spec.main_class {
        return None;
    }
    let client = recovery::path(instance.root(), &format!("versions/{id}/{id}.jar")).ok()?;
    if spec.client_jar != client || !client.is_file() {
        return None;
    }
    let mut detected = None;
    for profile in &profiles {
        let libraries = match profile.get("libraries") {
            Some(Value::Array(libraries)) if libraries.len() <= 4096 => libraries,
            None => continue,
            _ => return None,
        };
        for library in libraries {
            let coordinate = library["name"].as_str()?;
            if coordinate.starts_with("net.neoforged:") {
                return None;
            }
            let (loader, version) =
                if let Some(version) = coordinate.strip_prefix("net.fabricmc:fabric-loader:") {
                    if version.contains(':') {
                        return None;
                    }
                    ("fabric", version)
                } else if let Some(version) = coordinate
                    .strip_prefix("net.minecraftforge:forge:")
                    .or_else(|| coordinate.strip_prefix("net.minecraftforge:fmlloader:"))
                {
                    let version = version.split(':').next()?;
                    ("forge", version.strip_prefix(&format!("{id}-"))?)
                } else {
                    continue;
                };
            let target = TARGETS
                .iter()
                .find(|t| t.0 == id && t.1 == loader && t.2 == version)?;
            if detected.is_some_and(|old| old != target) {
                return None;
            }
            // Require the loader artifact declared in the same observed profile
            // to be present in the actual resolved classpath, with confined path.
            let parts: Vec<_> = coordinate.split(':').collect();
            if !(3..=4).contains(&parts.len()) || parts.iter().any(|part| !component(part)) {
                return None;
            }
            let classifier = parts.get(3).map(|v| format!("-{v}")).unwrap_or_default();
            let expected = format!(
                "{}/{}/{}/{}-{}{}.jar",
                parts[0].replace('.', "/"),
                parts[1],
                parts[2],
                parts[1],
                parts[2],
                classifier
            );
            if library
                .pointer("/downloads/artifact/path")
                .is_some_and(|p| p.as_str() != Some(expected.as_str()))
            {
                return None;
            }
            let path = recovery::path(instance.root(), &format!("libraries/{expected}")).ok()?;
            if !path.is_file() || !spec.libraries.iter().any(|p| p == &path) {
                return None;
            }
            detected = Some(target);
        }
    }
    let &(minecraft, loader, loader_version, protocol) = detected?;
    let main_matches = match (loader, minecraft) {
        ("fabric", _) => matches!(
            resolved_main,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
                | "net.fabricmc.loader.launch.knot.KnotClient"
        ),
        ("forge", "1.12.2") => resolved_main == "net.minecraft.launchwrapper.Launch",
        ("forge", "1.16.5") => resolved_main == "cpw.mods.modlauncher.Launcher",
        ("forge", "1.21.1") => resolved_main == "net.minecraftforge.bootstrap.ForgeBootstrap",
        ("forge", _) => resolved_main == "cpw.mods.bootstraplauncher.BootstrapLauncher",
        _ => false,
    };
    if !main_matches {
        return None;
    }
    if loader == "forge" {
        let argument = |flag: &str, value: &str| {
            spec.game_args
                .iter()
                .filter(|arg| arg.as_str() == flag)
                .count()
                == 1
                && spec
                    .game_args
                    .windows(2)
                    .any(|pair| pair[0] == flag && pair[1] == value)
        };
        match minecraft {
            "1.12.2"
                if !argument(
                    "--tweakClass",
                    "net.minecraftforge.fml.common.launcher.FMLTweaker",
                ) =>
            {
                return None
            }
            "1.21.1" if !argument("--launchTarget", "forge_client") => return None,
            "1.16.5" | "1.19.2" | "1.20.1"
                if !argument("--fml.mcVersion", minecraft)
                    || !argument("--fml.forgeVersion", loader_version)
                    || !argument("--fml.forgeGroup", "net.minecraftforge")
                    || !argument(
                        "--launchTarget",
                        if minecraft == "1.16.5" {
                            "fmlclient"
                        } else {
                            "forgeclient"
                        },
                    ) =>
            {
                return None;
            }
            _ => {}
        }
    }
    Some(StatusRuntimeProfile {
        summary: StatusRuntimeSummary {
            minecraft_version: minecraft,
            loader,
            loader_version,
            status_protocol: protocol,
            evidence: "nativeLaunchProfile",
        },
        fingerprint: format!("{:x}", fingerprint.finalize()),
    })
}

#[cfg(all(test, windows, feature = "connect-lab"))]
pub(crate) fn fixture_profile(index: usize) -> StatusRuntimeProfile {
    let (minecraft_version, loader, loader_version, status_protocol) = TARGETS[index];
    StatusRuntimeProfile {
        summary: StatusRuntimeSummary {
            minecraft_version,
            loader,
            loader_version,
            status_protocol,
            evidence: "nativeLaunchProfile",
        },
        fingerprint: format!("fixture-{index}"),
    }
}

#[cfg(test)]
#[path = "connect_status_profile_tests.rs"]
pub(crate) mod tests;
