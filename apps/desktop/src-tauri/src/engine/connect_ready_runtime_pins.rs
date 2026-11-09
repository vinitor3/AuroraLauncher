//! Exact installed metadata and artifact identities for the additional ready profiles.
//! Launch fields come from the official Mojang manifest and Fabric Meta or the
//! installer version.json. Forge supplemental files come from install_profile.json.
//! Extracted Java 8 natives come from the checked Mojang Windows classifier archives.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{Error, ReadyProfile};
use crate::engine::version;

const PROFILE_FIELDS: [&str; 10] = [
    "id",
    "inheritsFrom",
    "jar",
    "mainClass",
    "javaVersion",
    "assetIndex",
    "minecraftArguments",
    "arguments",
    "libraries",
    "downloads",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfilePins {
    base_profile_sha256: String,
    loader_profile_sha256: String,
    #[serde(default)]
    additional_libraries: BTreeMap<String, String>,
    #[serde(default)]
    fabric_library_sha1: BTreeMap<String, String>,
    #[serde(default)]
    extracted_natives: BTreeMap<String, String>,
}

pub(super) struct RuntimePins {
    pub library_sha1: BTreeMap<String, String>,
    pub additional_libraries: BTreeMap<String, String>,
    pub extracted_natives: BTreeMap<String, String>,
    pub client_sha1: String,
    pub asset_index_sha1: String,
}

fn profile_sha256(document: &Value) -> Result<String, Error> {
    let mut fields = Value::Array(
        PROFILE_FIELDS
            .iter()
            .map(|field| document[*field].clone())
            .collect(),
    );
    fields.sort_all_objects();
    let encoded = serde_json::to_vec(&fields).map_err(|_| Error::Internal)?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
}

impl RuntimePins {
    pub(super) fn from_profiles(
        profile: ReadyProfile,
        base: &Value,
        child: &Value,
    ) -> Result<Option<Self>, Error> {
        if matches!(profile, ReadyProfile::Fabric1211 | ReadyProfile::Fabric1201) {
            return Ok(None);
        }
        Self::from_exact_profiles(profile, base, child).map(Some)
    }

    pub(super) fn from_exact_profiles(
        profile: ReadyProfile,
        base: &Value,
        child: &Value,
    ) -> Result<Self, Error> {
        let mut targets: BTreeMap<String, ProfilePins> =
            serde_json::from_str(include_str!("connect_ready_runtime_pins.json"))
                .map_err(|_| Error::Internal)?;
        let target = targets
            .remove(profile.version_id())
            .ok_or(Error::Internal)?;
        if profile_sha256(base)? != target.base_profile_sha256
            || profile_sha256(child)? != target.loader_profile_sha256
        {
            return Err(Error::UnsupportedRuntime);
        }
        let mut library_sha1 = target.fabric_library_sha1;
        for document in [base, child] {
            for library in document["libraries"].as_array().ok_or(Error::Internal)? {
                let Some(path) = version::library_path(library).map_err(|_| Error::Internal)?
                else {
                    continue;
                };
                let relative = format!("libraries/{}", path.to_str().ok_or(Error::Internal)?)
                    .replace('\\', "/");
                if let Some(sha1) = library
                    .pointer("/downloads/artifact/sha1")
                    .or_else(|| library.get("sha1"))
                    .and_then(Value::as_str)
                {
                    library_sha1.insert(relative, sha1.to_owned());
                }
            }
        }
        Ok(Self {
            library_sha1,
            additional_libraries: target.additional_libraries,
            extracted_natives: target.extracted_natives,
            client_sha1: base["downloads"]["client"]["sha1"]
                .as_str()
                .ok_or(Error::Internal)?
                .to_owned(),
            asset_index_sha1: base["assetIndex"]["sha1"]
                .as_str()
                .ok_or(Error::Internal)?
                .to_owned(),
        })
    }
}

pub(super) fn verify_sha1(bytes: &[u8], expected: &str) -> Result<(), Error> {
    if format!("{:x}", sha1::Sha1::digest(bytes)) != expected {
        return Err(Error::ArtifactMismatch);
    }
    Ok(())
}
