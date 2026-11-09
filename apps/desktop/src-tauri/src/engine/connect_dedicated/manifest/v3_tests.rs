//! Serialized codec fixtures intentionally grant no factory/process authority.
use super::super::super::{
    server_catalog, server_preparation::ServerPreparationProvenance, voice_catalog,
    NativeVoiceProfile,
};
use super::*;
use crate::engine::connect_ready_profile::ReadyProfile;

fn record(name: &str, bytes: u64, sha256: &str, index: u64) -> resources::ResourceRecord {
    resources::ResourceRecord {
        name: name.into(),
        observation: Observation {
            identity: FileIdentity { volume: 7, index },
            bytes,
            sha256: sha256.into(),
        },
    }
}
fn fixture(runtime: ReadyProfile, profile: NativeVoiceProfile) -> VoiceManifestV3 {
    let mut pins = server_catalog::fabric_artifacts(runtime)
        .unwrap_or(&[])
        .to_vec();
    pins.extend_from_slice(voice_catalog::artifacts(runtime, profile).unwrap());
    let mut records = pins
        .iter()
        .enumerate()
        .map(|(index, pin)| record(pin.relative, pin.bytes, pin.sha256, 100 + index as u64))
        .collect::<Vec<_>>();
    let recipe = server_catalog::launch_args(runtime);
    if server_catalog::forge_installer(runtime).is_some() {
        for name in server_catalog::required_recipe_artifacts(runtime) {
            records.push(record(
                &name,
                50,
                &"ab".repeat(32),
                200 + records.len() as u64,
            ));
        }
    }
    records.sort_by(|a, b| a.name.cmp(&b.name));
    let provenance =
        ServerPreparationProvenance::fixture(runtime, profile, &records, &recipe).unwrap();
    let server = ServerRuntimeV3 {
        runtime_profile: runtime.version_id().into(),
        java_major: runtime.descriptor().java_major,
        recipe_args: recipe,
        provenance,
    };
    let mut voice = voice(profile.stored());
    voice.runtime = server.runtime_metadata().unwrap();
    voice.observation_policy = ObservationPolicyV2::LiteralLoopback35554Durable;
    voice.initial_voice_configuration.observation.bytes =
        voice.observation_policy.configuration(voice.profile).len() as u64;
    voice.initial_voice_configuration.observation.sha256 = voice
        .observation_policy
        .configuration_sha256(voice.profile)
        .into();
    voice.installed = records.clone();
    for copied in &mut voice.installed {
        copied.observation.identity.index += 1000;
    }
    records.push(record("jdk/release", 100, &"cd".repeat(32), 900));
    records.push(record("jdk/java.exe", 100, &"ef".repeat(32), 901));
    voice.resources = records;
    voice.provenance = (0..6)
        .map(|index| {
            record(
                &format!("provenance-{index}"),
                32,
                &"12".repeat(32),
                950 + index,
            )
        })
        .collect();
    VoiceManifestV3 {
        schema: 3,
        voice,
        server,
    }
}

#[test]
fn v3_all_seventeen_runtime_voice_pairs_roundtrip_without_becoming_v2() {
    let mut accepted = 0;
    for runtime in ReadyProfile::ALL {
        for profile in [
            NativeVoiceProfile::SimpleVoiceChat,
            NativeVoiceProfile::PlasmoVoice,
        ] {
            if voice_catalog::artifacts(runtime, profile).is_none() {
                continue;
            }
            let value = fixture(runtime, profile);
            resources::v3_record_shape(&value).expect("v3_closed_artifact_shape");
            let original = Manifest::VoiceV3(Box::new(value));
            let bytes = original.encode_bounded().unwrap();
            let decoded = decode_manifest(&bytes).expect("v3_runtime_pair");
            assert_eq!(decoded, original);
            assert!(decoded.voice_v2().is_err());
            assert!(decoded.status_v1().is_err());
            assert_eq!(decoded.voice().unwrap().profile, profile.stored());
            assert_eq!(
                decoded.server_v3().unwrap().server.runtime().unwrap(),
                runtime
            );
            assert_eq!(decoded.encode_bounded().unwrap(), bytes);
            accepted += 1;
        }
    }
    assert_eq!(accepted, 17);
}

#[test]
fn v3_nested_duplicate_arrays_extensions_and_missing_runtime_fail_closed() {
    let bytes = Manifest::VoiceV3(Box::new(fixture(
        ReadyProfile::Forge1201,
        NativeVoiceProfile::SimpleVoiceChat,
    )))
    .encode_bounded()
    .unwrap();
    let file = TestFile::new(&bytes);
    for parts in [
        vec![],
        vec!["voice"],
        vec!["server"],
        vec!["server", "provenance"],
        vec!["server", "provenance", "installer"],
        vec!["voice", "resources", "0"],
        vec!["voice", "resources", "0", "observation"],
        vec!["voice", "resources", "0", "observation", "identity"],
    ] {
        file.reject(&positional_object(&bytes, &parts));
        file.reject(&unknown(&bytes, &parts));
        let first = members(&bytes, path(&bytes, &parts).start).remove(0).name;
        file.reject(&duplicate(&bytes, &parts, &first, None));
    }
    for field in ["runtime_profile", "java_major", "recipe_args", "provenance"] {
        file.reject(&remove(&bytes, &["server"], field));
        file.reject(&replace(&bytes, &["server", field], b"null"));
    }
    file.reject(&replace(&bytes, &["schema"], b"2"));
    file.reject(&replace(&bytes, &["voice", "schema"], b"3"));
    file.reject(&replace(
        &bytes,
        &["server", "runtime_profile"],
        br#""1.20.1-neoforge""#,
    ));
    file.reject(&replace(&bytes, &["server", "java_major"], b"8"));
    file.reject(&replace(&bytes, &["voice", "runtime", "protocol"], b"767"));
    file.reject(&replace(&bytes, &["voice", "observation_policy"], b"1"));
}

#[test]
fn v3_original_output_identity_recipe_and_supported_pair_are_bound() {
    let original = fixture(ReadyProfile::Forge1122, NativeVoiceProfile::SimpleVoiceChat);
    let mut changed = original.clone();
    changed.voice.profile = StoredVoiceProfileV2::Plasmo;
    assert!(changed.validate_wire().is_err());
    let mut changed = original.clone();
    changed.voice.resources[0].observation.identity.index += 1;
    assert!(changed.validate_wire().is_err());
    let mut changed = original.clone();
    changed
        .server
        .recipe_args
        .push("-javaagent:elsewhere.jar".into());
    assert!(changed.validate_wire().is_err());
    let mut changed = original.clone();
    changed.server.provenance.java_tree_sha256 = "00".repeat(32);
    assert!(changed.validate_wire().is_err());
    let mut changed = original.clone();
    changed.voice.installed[0].name = "../outside.jar".into();
    assert!(resources::v3_record_shape(&changed).is_err());
    let mut changed = original.clone();
    changed.voice.installed[0].observation.sha256 = "00".repeat(32);
    assert!(resources::v3_record_shape(&changed).is_err());
    // A copied file has a new filesystem identity. That must not replace the
    // original installer's output identity in the preparation digest.
    let mut changed = original;
    changed.voice.installed[0].observation.identity.index += 1;
    resources::v3_record_shape(&changed).unwrap();
}

#[test]
fn v3_larger_envelope_does_not_raise_v1_or_v2_limits() {
    let original = Manifest::VoiceV3(Box::new(fixture(
        ReadyProfile::Fabric1211,
        NativeVoiceProfile::SimpleVoiceChat,
    )))
    .encode_bounded()
    .unwrap();
    let mut spaced = original.clone();
    spaced.resize((MAX_JOURNAL + 1) as usize, b' ');
    assert!(decode_manifest(&spaced).is_ok());
    let mut legacy = V1_CANONICAL.to_vec();
    legacy.resize((MAX_JOURNAL + 1) as usize, b' ');
    assert_eq!(decode_manifest(&legacy), Err(Error::Limit));
    let mut legacy = encoded_voice(StoredVoiceProfileV2::Svc);
    legacy.resize((MAX_JOURNAL + 1) as usize, b' ');
    assert_eq!(decode_manifest(&legacy), Err(Error::Limit));
    spaced.resize((MAX_V3_MANIFEST_BYTES + 1) as usize, b' ');
    assert_eq!(decode_manifest(&spaced), Err(Error::Limit));
}
