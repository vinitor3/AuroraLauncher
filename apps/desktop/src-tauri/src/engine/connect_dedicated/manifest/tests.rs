//! Codec oracles. Real pre-V2 persisted bytes/preimages and the thirteen old
//! physical roots remain covered by tests/v1_upgrade_capture after integration.
use super::*;
use serde::de::IgnoredAny;
use std::{fs, ops::Range, path::PathBuf};

// Deliberately structural V1 data: the old serde type did not validate hash
// spellings/counts/physical identities. B1 must not silently add those rules.
const V1_CANONICAL: &[u8] = br#"{"schema":1,"id":"00000000-0000-0000-0000-000000000001","lab_identity":{"volume":7,"index":11},"bundle_identity":{"volume":7,"index":12},"native_identity":{"volume":7,"index":13},"copy_id":"aurora-test-codec","copy_uuid":"copy","origin_uuid":"origin","copy_identity":"copy-physical","origin_identity":"origin-physical","origin_fingerprint":"legacy-fingerprint","resources":[{"name":"r","observation":{"identity":{"volume":7,"index":21},"bytes":23,"sha256":"legacy-hash"}}],"installed":[{"name":"i","observation":{"identity":{"volume":7,"index":22},"bytes":24,"sha256":"legacy-hash"}}],"provenance":[{"name":"p","observation":{"identity":{"volume":7,"index":23},"bytes":25,"sha256":"legacy-hash"}}]}"#;

fn voice(profile: StoredVoiceProfileV2) -> VoiceManifestV2 {
    let base: ManifestV1 = serde_json::from_slice(V1_CANONICAL).expect("codec_v1_fixture");
    VoiceManifestV2 {
        schema: 2,
        profile,
        id: base.id,
        lab_identity: base.lab_identity,
        bundle_identity: base.bundle_identity,
        native_identity: base.native_identity,
        copy_id: base.copy_id,
        copy_uuid: base.copy_uuid,
        origin_uuid: base.origin_uuid,
        copy_identity: base.copy_identity,
        origin_identity: base.origin_identity,
        origin_fingerprint: base.origin_fingerprint,
        resources: base.resources,
        installed: base.installed,
        provenance: base.provenance,
        runtime: VoiceRuntimeV2 {
            minecraft: voice_profile::MINECRAFT.into(),
            loader: voice_profile::LOADER.into(),
            loader_version: voice_profile::LOADER_VERSION.into(),
            protocol: voice_profile::PROTOCOL,
        },
        initial_voice_configuration: InitialVoiceConfigurationV2 {
            name: profile.configuration_name().into(),
            observation: Observation {
                identity: FileIdentity {
                    volume: 7,
                    index: 99,
                },
                bytes: profile.configuration().len() as u64,
                sha256: profile.configuration_sha256().into(),
            },
        },
        observation_policy: ObservationPolicyV2::LiteralOnlyFirstPersisted,
    }
}

fn encoded_voice(profile: StoredVoiceProfileV2) -> Vec<u8> {
    serde_json::to_vec(&voice(profile)).expect("codec_v2_fixture")
}

struct TestFile {
    root: PathBuf,
    path: PathBuf,
}
impl TestFile {
    fn new(original: &[u8]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "aurora-manifest-codec-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&root).expect("codec_own_root");
        let fixture = Self {
            path: root.join("bundle.json"),
            root,
        };
        fixture.write(original);
        assert!(
            read_manifest(&fixture.path).is_ok(),
            "codec_positive_anchor"
        );
        fixture
    }
    fn write(&self, bytes: &[u8]) {
        fs::write(&self.path, bytes).expect("codec_own_file_write");
    }
    fn reject(&self, mutated: &[u8]) {
        // A duplicate-key attack is still syntactically valid JSON. This
        // validation discards tokens without ever constructing/collapsing a map.
        serde_json::from_slice::<IgnoredAny>(mutated).expect("codec_attack_is_valid_json");
        self.write(mutated);
        assert!(
            read_manifest(&self.path).is_err(),
            "codec_dispatch_must_reject"
        );
        assert_eq!(
            fs::read(&self.path).unwrap(),
            mutated,
            "codec_never_rewrites"
        );
    }
}
impl Drop for TestFile {
    fn drop(&mut self) {
        // Only these two exact newly created paths are removed; no recursive
        // traversal or retained B0 root is involved in codec fixtures.
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_dir(&self.root);
    }
}

fn skip_space(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}
fn string_end(bytes: &[u8], start: usize) -> usize {
    assert_eq!(bytes[start], b'"');
    let mut at = start + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return at + 1,
            _ => at += 1,
        }
    }
    panic!("codec_fixture_unclosed_string");
}
fn value_end(bytes: &[u8], start: usize) -> usize {
    if bytes[start] == b'"' {
        return string_end(bytes, start);
    }
    if matches!(bytes[start], b'{' | b'[') {
        let mut at = start;
        let mut depth = 0;
        while at < bytes.len() {
            match bytes[at] {
                b'"' => {
                    at = string_end(bytes, at);
                    continue;
                }
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return at + 1;
                    }
                }
                _ => {}
            }
            at += 1;
        }
        panic!("codec_fixture_unclosed_value");
    }
    let mut at = start;
    while bytes
        .get(at)
        .is_some_and(|byte| !matches!(byte, b',' | b'}' | b']') && !byte.is_ascii_whitespace())
    {
        at += 1;
    }
    at
}

struct Member {
    name: String,
    start: usize,
    value: Range<usize>,
}
fn members(bytes: &[u8], start: usize) -> Vec<Member> {
    assert_eq!(bytes[start], b'{');
    let mut result = Vec::new();
    let mut at = skip_space(bytes, start + 1);
    while bytes[at] != b'}' {
        let key_end = string_end(bytes, at);
        let name: String = serde_json::from_slice(&bytes[at..key_end]).unwrap();
        let colon = skip_space(bytes, key_end);
        assert_eq!(bytes[colon], b':');
        let value_start = skip_space(bytes, colon + 1);
        let end = value_end(bytes, value_start);
        result.push(Member {
            name,
            start: at,
            value: value_start..end,
        });
        at = skip_space(bytes, end);
        if bytes[at] == b',' {
            at = skip_space(bytes, at + 1);
        } else {
            assert_eq!(bytes[at], b'}');
        }
    }
    result
}
fn path(bytes: &[u8], parts: &[&str]) -> Range<usize> {
    let start = skip_space(bytes, 0);
    let mut range = start..value_end(bytes, start);
    for part in parts {
        if let Ok(index) = part.parse::<usize>() {
            assert_eq!(bytes[range.start], b'[');
            let mut at = skip_space(bytes, range.start + 1);
            for _ in 0..index {
                at = skip_space(bytes, value_end(bytes, at));
                assert_eq!(bytes[at], b',');
                at = skip_space(bytes, at + 1);
            }
            range = at..value_end(bytes, at);
        } else {
            range = members(bytes, range.start)
                .into_iter()
                .find(|member| member.name == *part)
                .expect("codec_fixture_member")
                .value;
        }
    }
    range
}
fn replace(bytes: &[u8], parts: &[&str], replacement: &[u8]) -> Vec<u8> {
    let range = path(bytes, parts);
    [&bytes[..range.start], replacement, &bytes[range.end..]].concat()
}
fn positional_object(bytes: &[u8], parts: &[&str]) -> Vec<u8> {
    let range = path(bytes, parts);
    let entries = members(bytes, range.start);
    let mut sequence = vec![b'['];
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            sequence.push(b',');
        }
        sequence.extend_from_slice(&bytes[entry.value.clone()]);
    }
    sequence.push(b']');
    replace(bytes, parts, &sequence)
}
fn duplicate(bytes: &[u8], object: &[&str], key: &str, value: Option<&[u8]>) -> Vec<u8> {
    let range = path(bytes, object);
    let member = members(bytes, range.start)
        .into_iter()
        .find(|member| member.name == key)
        .expect("codec_duplicate_member");
    let prefix = &bytes[member.start..member.value.start];
    let value = value.unwrap_or(&bytes[member.value.clone()]);
    [
        &bytes[..member.start],
        prefix,
        value,
        b",",
        &bytes[member.start..],
    ]
    .concat()
}
fn unknown(bytes: &[u8], object: &[&str]) -> Vec<u8> {
    let range = path(bytes, object);
    [
        &bytes[..range.start + 1],
        br#""unknown":null,"#,
        &bytes[range.start + 1..],
    ]
    .concat()
}
fn remove(bytes: &[u8], object: &[&str], key: &str) -> Vec<u8> {
    let range = path(bytes, object);
    let entries = members(bytes, range.start);
    let index = entries.iter().position(|entry| entry.name == key).unwrap();
    let removed = if index + 1 < entries.len() {
        entries[index].start..entries[index + 1].start
    } else if index > 0 {
        entries[index - 1].value.end..entries[index].value.end
    } else {
        entries[index].start..entries[index].value.end
    };
    [&bytes[..removed.start], &bytes[removed.end..]].concat()
}

#[test]
fn v1_serialization_delegates_without_tag_fields_or_new_constraints() {
    let fixture = TestFile::new(V1_CANONICAL);
    let decoded = read_manifest(&fixture.path).unwrap();
    assert_eq!(decoded.kind(), ManifestKind::StatusV1);
    assert_eq!(serde_json::to_vec(&decoded).unwrap(), V1_CANONICAL);
    assert_eq!(
        serde_json::to_vec(decoded.status_v1().unwrap()).unwrap(),
        V1_CANONICAL
    );
    assert_eq!(decoded.voice_v2(), Err(Error::Identity));
    assert_eq!(decoded.resources()[0].observation.sha256, "legacy-hash");
    assert_eq!(fs::read(&fixture.path).unwrap(), V1_CANONICAL);
}

#[test]
fn v1_whitespace_key_order_and_equivalent_escapes_keep_legacy_acceptance() {
    let mut spaced = b" \r\n\t".to_vec();
    let mut quoted = false;
    let mut escaped = false;
    for byte in V1_CANONICAL {
        spaced.push(*byte);
        if quoted {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                quoted = false;
            }
        } else if *byte == b'"' {
            quoted = true;
        } else if matches!(byte, b'{' | b'[' | b',' | b':') {
            spaced.extend_from_slice(b" \n\t");
        }
    }
    spaced.extend_from_slice(b"\r\n ");
    let prefix = br#"{"schema":1,"#;
    assert!(V1_CANONICAL.starts_with(prefix));
    let reordered = [
        b"{".as_slice(),
        &V1_CANONICAL[prefix.len()..V1_CANONICAL.len() - 1],
        br#","schema":1}"#,
    ]
    .concat();
    let escaped = String::from_utf8(V1_CANONICAL.to_vec())
        .unwrap()
        .replacen("\"schema\"", "\"sch\\u0065ma\"", 1)
        .into_bytes();
    let fixture = TestFile::new(V1_CANONICAL);
    for payload in [spaced, reordered, escaped] {
        assert_ne!(payload, V1_CANONICAL);
        fixture.write(&payload);
        let decoded = read_manifest(&fixture.path).expect("old_v1_lexical_form_still_accepted");
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), V1_CANONICAL);
        assert_eq!(fs::read(&fixture.path).unwrap(), payload);
    }
}

#[test]
fn v1_positional_struct_inputs_keep_the_unchanged_legacy_derive_acceptance() {
    let fixture = TestFile::new(V1_CANONICAL);
    let mut objects = vec![
        vec![],
        vec!["lab_identity"],
        vec!["bundle_identity"],
        vec!["native_identity"],
    ];
    for collection in ["resources", "installed", "provenance"] {
        for tail in [vec![], vec!["observation"], vec!["observation", "identity"]] {
            let mut object = vec![collection, "0"];
            object.extend(tail);
            objects.push(object);
        }
    }
    for object in objects {
        let raw = positional_object(V1_CANONICAL, &object);
        let legacy: ManifestV1 =
            serde_json::from_slice(&raw).expect("unchanged_legacy_derive_accepts_sequence");
        fixture.write(&raw);
        let decoded = read_manifest(&fixture.path).expect("dispatch_preserves_legacy_acceptance");
        assert_eq!(decoded.status_v1().unwrap(), &legacy);
        assert_eq!(
            serde_json::to_vec(&decoded).unwrap(),
            serde_json::to_vec(&legacy).unwrap()
        );
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), V1_CANONICAL);
        assert_eq!(fs::read(&fixture.path).unwrap(), raw);
    }
    for raw in [b"[]".to_vec(), b"[1]".to_vec(), {
        let mut too_long = positional_object(V1_CANONICAL, &[]);
        too_long.pop();
        too_long.extend_from_slice(b",0]");
        too_long
    }] {
        assert!(serde_json::from_slice::<ManifestV1>(&raw).is_err());
        fixture.reject(&raw);
    }
}

#[test]
fn every_v2_object_position_rejects_positional_arrays_without_changing_v1() {
    for profile in [StoredVoiceProfileV2::Svc, StoredVoiceProfileV2::Plasmo] {
        let original = encoded_voice(profile);
        let fixture = TestFile::new(&original);
        let mut objects = vec![
            vec![],
            vec!["lab_identity"],
            vec!["bundle_identity"],
            vec!["native_identity"],
            vec!["runtime"],
            vec!["initial_voice_configuration"],
            vec!["initial_voice_configuration", "observation"],
            vec!["initial_voice_configuration", "observation", "identity"],
        ];
        for collection in ["resources", "installed", "provenance"] {
            for tail in [vec![], vec!["observation"], vec!["observation", "identity"]] {
                let mut object = vec![collection, "0"];
                object.extend(tail);
                objects.push(object);
            }
        }
        for object in objects {
            let raw = positional_object(&original, &object);
            assert!(serde_json::from_slice::<VoiceManifestV2>(&raw).is_err());
            fixture.reject(&raw);
        }
    }
}

#[test]
fn shared_accessors_preserve_both_inner_records_without_coercion() {
    let v1 = decode_manifest(V1_CANONICAL).unwrap();
    let v2 = decode_manifest(&encoded_voice(StoredVoiceProfileV2::Svc)).unwrap();
    assert_eq!(v1.schema(), 1);
    assert_eq!(v2.schema(), 2);
    assert_eq!(v1.id(), v2.id());
    assert_eq!(v1.lab_identity(), v2.lab_identity());
    assert_eq!(v1.bundle_identity(), v2.bundle_identity());
    assert_eq!(v1.native_identity(), v2.native_identity());
    assert_eq!(v1.copy_id(), v2.copy_id());
    assert_eq!(v1.copy_uuid(), v2.copy_uuid());
    assert_eq!(v1.origin_uuid(), v2.origin_uuid());
    assert_eq!(v1.copy_identity(), v2.copy_identity());
    assert_eq!(v1.origin_identity(), v2.origin_identity());
    assert_eq!(v1.origin_fingerprint(), v2.origin_fingerprint());
    assert_eq!(v1.resources(), v2.resources());
    assert_eq!(v1.installed(), v2.installed());
    assert_eq!(v1.provenance(), v2.provenance());
    assert_eq!(v2.status_v1(), Err(Error::Identity));
    assert!(v2.voice_v2().is_ok());
}

#[test]
fn both_voice_profiles_have_exact_direct_wire_fields_order_and_roundtrip() {
    let expected = [
        "schema",
        "profile",
        "id",
        "lab_identity",
        "bundle_identity",
        "native_identity",
        "copy_id",
        "copy_uuid",
        "origin_uuid",
        "copy_identity",
        "origin_identity",
        "origin_fingerprint",
        "resources",
        "installed",
        "provenance",
        "runtime",
        "initial_voice_configuration",
        "observation_policy",
    ];
    for profile in [StoredVoiceProfileV2::Svc, StoredVoiceProfileV2::Plasmo] {
        let inner = voice(profile);
        let bytes = serde_json::to_vec(&inner).unwrap();
        let fixture = TestFile::new(&bytes);
        let decoded = read_manifest(&fixture.path).unwrap();
        assert_eq!(decoded.kind(), ManifestKind::VoiceV2(profile));
        assert_eq!(decoded.voice_v2().unwrap(), &inner);
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes);
        assert_eq!(
            members(&bytes, 0)
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        for (object, keys) in [
            (
                vec!["runtime"],
                vec!["minecraft", "loader", "loader_version", "protocol"],
            ),
            (
                vec!["initial_voice_configuration"],
                vec!["name", "observation"],
            ),
            (
                vec!["initial_voice_configuration", "observation"],
                vec!["identity", "bytes", "sha256"],
            ),
            (
                vec!["initial_voice_configuration", "observation", "identity"],
                vec!["volume", "index"],
            ),
        ] {
            assert_eq!(
                members(&bytes, path(&bytes, &object).start)
                    .iter()
                    .map(|field| field.name.as_str())
                    .collect::<Vec<_>>(),
                keys
            );
        }
        assert_eq!(&bytes[path(&bytes, &["observation_policy"])], b"1");
    }
}

#[test]
fn compiled_voice_baselines_keep_the_approved_exact_bytes_lengths_and_hashes() {
    for (profile, length) in [
        (StoredVoiceProfileV2::Svc, 176),
        (StoredVoiceProfileV2::Plasmo, 164),
    ] {
        assert_eq!(profile.configuration().len(), length);
        assert_eq!(
            super::super::digest(profile.configuration()),
            profile.configuration_sha256()
        );
        assert!(!profile.configuration().contains(&b'\r'));
    }
}

#[test]
fn equal_different_and_escaped_duplicate_schema_reach_original_byte_dispatch() {
    for original in [
        V1_CANONICAL.to_vec(),
        encoded_voice(StoredVoiceProfileV2::Svc),
        encoded_voice(StoredVoiceProfileV2::Plasmo),
    ] {
        let fixture = TestFile::new(&original);
        fixture.reject(&duplicate(&original, &[], "schema", None));
        fixture.reject(&duplicate(
            &original,
            &[],
            "schema",
            Some(if original.starts_with(br#"{"schema":1"#) {
                b"2"
            } else {
                b"1"
            }),
        ));
        let start = members(&original, 0)[0].value.clone();
        let escaped = [
            b"{".as_slice(),
            br#""sch\u0065ma":"#,
            &original[start],
            b",",
            &original[1..],
        ]
        .concat();
        fixture.reject(&escaped);
    }
}

#[test]
fn duplicates_in_every_common_nested_object_are_rejected_by_the_real_reader() {
    for original in [
        V1_CANONICAL.to_vec(),
        encoded_voice(StoredVoiceProfileV2::Svc),
        encoded_voice(StoredVoiceProfileV2::Plasmo),
    ] {
        let fixture = TestFile::new(&original);
        for (object, key) in [
            (vec![], "copy_id"),
            (vec!["lab_identity"], "volume"),
            (vec!["bundle_identity"], "index"),
            (vec!["native_identity"], "volume"),
        ] {
            fixture.reject(&duplicate(&original, &object, key, None));
        }
        for collection in ["resources", "installed", "provenance"] {
            for (tail, key) in [
                (vec![], "name"),
                (vec!["observation"], "identity"),
                (vec!["observation"], "bytes"),
                (vec!["observation"], "sha256"),
                (vec!["observation", "identity"], "volume"),
                (vec!["observation", "identity"], "index"),
            ] {
                let mut object = vec![collection, "0"];
                object.extend(tail);
                fixture.reject(&duplicate(&original, &object, key, None));
            }
        }
    }
}

#[test]
fn duplicate_voice_runtime_configuration_profile_and_policy_fields_are_rejected() {
    for profile in [StoredVoiceProfileV2::Svc, StoredVoiceProfileV2::Plasmo] {
        let original = encoded_voice(profile);
        let fixture = TestFile::new(&original);
        for (object, keys) in [
            (
                vec![],
                vec![
                    "profile",
                    "runtime",
                    "initial_voice_configuration",
                    "observation_policy",
                ],
            ),
            (
                vec!["runtime"],
                vec!["minecraft", "loader", "loader_version", "protocol"],
            ),
            (
                vec!["initial_voice_configuration"],
                vec!["name", "observation"],
            ),
            (
                vec!["initial_voice_configuration", "observation"],
                vec!["identity", "bytes", "sha256"],
            ),
            (
                vec!["initial_voice_configuration", "observation", "identity"],
                vec!["volume", "index"],
            ),
        ] {
            for key in keys {
                fixture.reject(&duplicate(&original, &object, key, None));
            }
        }
    }
}

#[test]
fn unknown_fields_in_all_common_and_voice_objects_are_rejected() {
    for original in [
        V1_CANONICAL.to_vec(),
        encoded_voice(StoredVoiceProfileV2::Svc),
        encoded_voice(StoredVoiceProfileV2::Plasmo),
    ] {
        let fixture = TestFile::new(&original);
        for object in [
            vec![],
            vec!["lab_identity"],
            vec!["bundle_identity"],
            vec!["native_identity"],
        ] {
            fixture.reject(&unknown(&original, &object));
        }
        for collection in ["resources", "installed", "provenance"] {
            for tail in [vec![], vec!["observation"], vec!["observation", "identity"]] {
                let mut object = vec![collection, "0"];
                object.extend(tail);
                fixture.reject(&unknown(&original, &object));
            }
        }
        if !original.starts_with(br#"{"schema":1"#) {
            for object in [
                vec!["runtime"],
                vec!["initial_voice_configuration"],
                vec!["initial_voice_configuration", "observation"],
                vec!["initial_voice_configuration", "observation", "identity"],
            ] {
                fixture.reject(&unknown(&original, &object));
            }
        }
    }
}

#[test]
fn v1_profile_even_null_and_every_other_v2_extension_remain_unknown() {
    let fixture = TestFile::new(V1_CANONICAL);
    for extra in [
        br#""profile":null,"#.as_slice(),
        br#""profile":"svc","#,
        br#""profile":"plasmo","#,
        br#""runtime":{},"#,
        br#""initial_voice_configuration":{},"#,
        br#""observation_policy":1,"#,
    ] {
        let mutated = [&V1_CANONICAL[..1], extra, &V1_CANONICAL[1..]].concat();
        fixture.reject(&mutated);
    }
}

#[test]
fn unknown_missing_or_wrongly_typed_schema_never_selects_a_decoder() {
    let fixture = TestFile::new(V1_CANONICAL);
    for schema in [
        b"0".as_slice(),
        b"3",
        b"4294967295",
        b"-1",
        b"1.0",
        b"null",
        b"true",
        br#""1""#,
        b"{}",
        b"[]",
    ] {
        fixture.reject(&replace(V1_CANONICAL, &["schema"], schema));
    }
    fixture.reject(&remove(V1_CANONICAL, &[], "schema"));
}

#[test]
fn missing_common_fields_do_not_acquire_implicit_defaults_in_either_schema() {
    for original in [
        V1_CANONICAL.to_vec(),
        encoded_voice(StoredVoiceProfileV2::Svc),
        encoded_voice(StoredVoiceProfileV2::Plasmo),
    ] {
        let fixture = TestFile::new(&original);
        for key in [
            "id",
            "lab_identity",
            "bundle_identity",
            "native_identity",
            "copy_id",
            "copy_uuid",
            "origin_uuid",
            "copy_identity",
            "origin_identity",
            "origin_fingerprint",
            "resources",
            "installed",
            "provenance",
        ] {
            fixture.reject(&remove(&original, &[], key));
        }
    }
}

#[test]
fn voice_profile_requires_only_the_two_exact_scalar_wire_strings() {
    let original = encoded_voice(StoredVoiceProfileV2::Svc);
    let fixture = TestFile::new(&original);
    for profile in [
        br#""status""#.as_slice(),
        br#""SVC""#,
        br#""simpleVoiceChat""#,
        br#""plasmoVoice""#,
        b"null",
        b"0",
        b"[]",
        br#"{"svc":null}"#,
    ] {
        fixture.reject(&replace(&original, &["profile"], profile));
    }
    fixture.reject(&remove(&original, &[], "profile"));
}

#[test]
fn changing_a_known_profile_cannot_relabel_the_other_initial_configuration() {
    for (profile, replacement) in [
        (StoredVoiceProfileV2::Svc, br#""plasmo""#.as_slice()),
        (StoredVoiceProfileV2::Plasmo, br#""svc""#.as_slice()),
    ] {
        let original = encoded_voice(profile);
        let fixture = TestFile::new(&original);
        let changed = replace(&original, &["profile"], replacement);
        fixture.reject(&changed);
        assert_eq!(decode_manifest(&changed), Err(Error::Identity));
    }
}

#[test]
fn first_persisted_policy_is_numeric_literal_only_and_has_no_mapped_mode() {
    for profile in [StoredVoiceProfileV2::Svc, StoredVoiceProfileV2::Plasmo] {
        let original = encoded_voice(profile);
        let fixture = TestFile::new(&original);
        for policy in [
            b"0".as_slice(),
            b"2",
            b"1.0",
            br#""1""#,
            br#""Ipv4Mapped""#,
            br#""LiteralOnlyFirstPersisted""#,
            br#"{"LiteralOnlyFirstPersisted":null}"#,
            b"null",
        ] {
            fixture.reject(&replace(&original, &["observation_policy"], policy));
        }
        fixture.reject(&remove(&original, &[], "observation_policy"));
    }
}

#[test]
fn runtime_values_and_required_fields_are_closed_for_both_profiles() {
    for profile in [StoredVoiceProfileV2::Svc, StoredVoiceProfileV2::Plasmo] {
        let original = encoded_voice(profile);
        let fixture = TestFile::new(&original);
        for (key, value) in [
            ("minecraft", br#""1.20.1""#.as_slice()),
            ("loader", br#""forge""#),
            ("loader_version", br#""0.16.15""#),
            ("protocol", b"768"),
            ("protocol", b"767.0"),
        ] {
            fixture.reject(&replace(&original, &["runtime", key], value));
        }
        for key in ["minecraft", "loader", "loader_version", "protocol"] {
            fixture.reject(&remove(&original, &["runtime"], key));
        }
        fixture.reject(&remove(&original, &[], "runtime"));
    }
}

#[test]
fn initial_configuration_must_match_the_selected_compiled_baseline_without_io() {
    for profile in [StoredVoiceProfileV2::Svc, StoredVoiceProfileV2::Plasmo] {
        let original = encoded_voice(profile);
        let fixture = TestFile::new(&original);
        let other = if profile == StoredVoiceProfileV2::Svc {
            StoredVoiceProfileV2::Plasmo
        } else {
            StoredVoiceProfileV2::Svc
        };
        for name in [
            other.configuration_name(),
            "../voicechat-server.properties",
            "C:/outside/config.toml",
            "config/plasmovoice/server.toml",
            "config\\voicechat\\voicechat-server.properties",
        ] {
            let name = serde_json::to_vec(name).unwrap();
            fixture.reject(&replace(
                &original,
                &["initial_voice_configuration", "name"],
                &name,
            ));
        }
        for size in [b"0".as_slice(), b"999", b"176.0", b"null"] {
            fixture.reject(&replace(
                &original,
                &["initial_voice_configuration", "observation", "bytes"],
                size,
            ));
        }
        let uppercase_hash = profile.configuration_sha256().to_ascii_uppercase();
        for hash in [
            "",
            other.configuration_sha256(),
            "bad",
            uppercase_hash.as_str(),
        ] {
            let hash = serde_json::to_vec(hash).unwrap();
            fixture.reject(&replace(
                &original,
                &["initial_voice_configuration", "observation", "sha256"],
                &hash,
            ));
        }
        for key in ["name", "observation"] {
            fixture.reject(&remove(&original, &["initial_voice_configuration"], key));
        }
        fixture.reject(&remove(&original, &[], "initial_voice_configuration"));
        // The codec must not try to locate/open the mutable file: its physical
        // path has never existed in this UUID test directory.
        assert!(!fixture.root.join(profile.configuration_name()).exists());
        fixture.write(&original);
        assert!(read_manifest(&fixture.path).is_ok());
    }
}

#[test]
fn merely_changing_schema_cannot_upgrade_or_downgrade_the_wire_format() {
    let fixture = TestFile::new(V1_CANONICAL);
    fixture.reject(&replace(V1_CANONICAL, &["schema"], b"2"));
    for profile in [StoredVoiceProfileV2::Svc, StoredVoiceProfileV2::Plasmo] {
        let original = encoded_voice(profile);
        fixture.reject(&replace(&original, &["schema"], b"1"));
    }
}

#[test]
fn stripped_common_data_is_structural_v1_without_claiming_physical_downgrade_admission() {
    for profile in [StoredVoiceProfileV2::Svc, StoredVoiceProfileV2::Plasmo] {
        let mut stripped = replace(&encoded_voice(profile), &["schema"], b"1");
        for key in [
            "profile",
            "runtime",
            "initial_voice_configuration",
            "observation_policy",
        ] {
            stripped = remove(&stripped, &[], key);
        }
        let fixture = TestFile::new(&stripped);
        let decoded = read_manifest(&fixture.path).unwrap();
        assert_eq!(decoded.kind(), ManifestKind::StatusV1);
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), V1_CANONICAL);
        // Bundle resources/pins/claim checks are a separate required gate;
        // imposing new V1 semantic rules in this codec would break old serde.
    }
}

#[test]
fn both_kinds_enforce_the_original_byte_limit_and_reject_trailing_documents() {
    for original in [
        V1_CANONICAL.to_vec(),
        encoded_voice(StoredVoiceProfileV2::Svc),
        encoded_voice(StoredVoiceProfileV2::Plasmo),
    ] {
        let fixture = TestFile::new(&original);
        let mut at_limit = original.clone();
        at_limit.resize(MAX_JOURNAL as usize, b' ');
        fixture.write(&at_limit);
        let decoded = read_manifest(&fixture.path).unwrap();
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), original);
        at_limit.push(b' ');
        fixture.write(&at_limit);
        assert_eq!(read_manifest(&fixture.path), Err(Error::Limit));
        assert_eq!(decode_manifest(&at_limit), Err(Error::Limit));
        let trailing = [original.as_slice(), b" {}"].concat();
        fixture.write(&trailing);
        assert_eq!(read_manifest(&fixture.path), Err(Error::NeedsRecovery));
    }
}
