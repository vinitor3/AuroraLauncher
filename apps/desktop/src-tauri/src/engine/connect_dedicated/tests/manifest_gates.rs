//! Legacy B1 negatives retained after physical V2 admission in B2. These
//! relabelled STATUS records lack the required voice artifacts and baseline.
//! Their synthetic configuration metadata grants no EULA or run authority.
use super::*;
use crate::engine::connect_voice::dedicated::{manifest, voice_profile};
use manifest::{
    decode_manifest, InitialVoiceConfigurationV2, Manifest, ObservationPolicyV2,
    StoredVoiceProfileV2, VoiceManifestV2, VoiceRuntimeV2,
};

fn decoded_voice_manifest(prepared: &PreparedStatus, profile: StoredVoiceProfileV2) -> Manifest {
    let original = prepared
        .bundle
        .manifest
        .status_v1()
        .expect("gate_fixture_real_status_v1")
        .clone();
    let value = VoiceManifestV2 {
        schema: 2,
        profile,
        id: original.id,
        lab_identity: original.lab_identity,
        bundle_identity: original.bundle_identity,
        native_identity: original.native_identity,
        copy_id: original.copy_id,
        copy_uuid: original.copy_uuid,
        origin_uuid: original.origin_uuid,
        copy_identity: original.copy_identity,
        origin_identity: original.origin_identity,
        origin_fingerprint: original.origin_fingerprint,
        // Preserve the original V1 counts and identities. B1 rejected its
        // kind; B2 rejects these incomplete voice resource sets before IO.
        initial_voice_configuration: InitialVoiceConfigurationV2 {
            name: profile.configuration_name().into(),
            observation: Observation {
                // Synthetic metadata only. No configuration file is created
                // or asserted to have this identity by these negative tests.
                identity: original.installed[0].observation.identity.clone(),
                bytes: profile.configuration().len() as u64,
                sha256: profile.configuration_sha256().into(),
            },
        },
        resources: original.resources,
        installed: original.installed,
        provenance: original.provenance,
        runtime: VoiceRuntimeV2 {
            minecraft: voice_profile::MINECRAFT.into(),
            loader: voice_profile::LOADER.into(),
            loader_version: voice_profile::LOADER_VERSION.into(),
            protocol: voice_profile::PROTOCOL,
        },
        observation_policy: ObservationPolicyV2::LiteralOnlyFirstPersisted,
    };
    let encoded = encode(&value).expect("gate_fixture_v2_encoded");
    let decoded = decode_manifest(&encoded).expect("gate_fixture_real_decoder_accepts_v2");
    assert_eq!(decoded.voice_v2().unwrap(), &value);
    assert_eq!(decoded.installed().len(), 9);
    assert_eq!(decoded.resources().len(), 11);
    assert_eq!(decoded.provenance().len(), 6);
    decoded
}

fn no_eula_claim(paths: &TestPaths) {
    assert_eq!(bytes(&target(paths)), EULA_FALSE);
    assert!(!slot(paths, "claim.json").exists());
    assert_eq!(fs::read_dir(&paths.transaction).unwrap().count(), 0);
}

#[test]
fn relabelled_status_records_are_rejected_before_disk_comparison_or_recovery_io() {
    for profile in [StoredVoiceProfileV2::Svc, StoredVoiceProfileV2::Plasmo] {
        let fixture = Fixture::new();
        let mut prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        let original = bytes(&paths.bundle.join("bundle.json"));
        let before = files::tree(fixture.root.path());
        // Positive control: the same actual bundle and guard pass validation.
        assert_eq!(prepared.bundle.validate(&prepared.guard), Ok(()));
        let decoded = decoded_voice_manifest(&prepared, profile);
        prepared.bundle.manifest = decoded;
        assert_eq!(
            prepared.bundle.validate(&prepared.guard),
            Err(Error::Identity)
        );
        // Disk still contains valid V1. Voice shape rejects the incomplete
        // resource sets before that comparison, which would return Changed.
        assert_eq!(bytes(&paths.bundle.join("bundle.json")), original);
        files::assert_tree(fixture.root.path(), &before);
        no_eula_claim(&paths);
        drop(prepared);
        unlocked(&paths);
    }
}

#[test]
fn persisted_v2_with_status_resource_counts_cannot_reopen_or_retain_file_pins() {
    for profile in [StoredVoiceProfileV2::Svc, StoredVoiceProfileV2::Plasmo] {
        let fixture = Fixture::new();
        let prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        let id = prepared.bundle_id();
        let decoded = decoded_voice_manifest(&prepared, profile);
        let encoded = encode(&decoded).unwrap();
        let source = files::tree(&paths.origin);
        drop(prepared);
        unlocked(&paths);
        // The ordinary V1 reopen path works before the file is changed. Its
        // returned owner is dropped before writing the same fixture's manifest.
        drop(factory::Bundle::reopen(&fixture.lab, id).expect("positive_v1_bundle_reopen"));
        let manifest_path = paths.bundle.join("bundle.json");
        fs::write(&manifest_path, &encoded).expect("own_fixture_v2_manifest_write");
        assert!(decode_manifest(&bytes(&manifest_path))
            .unwrap()
            .voice_v2()
            .is_ok());
        let before = files::tree(fixture.root.path());
        assert_eq!(
            factory::Bundle::reopen(&fixture.lab, id).err(),
            Some(Error::Identity)
        );
        unlocked(&paths);
        // A leaked SecureFile would deny this write open on Windows. Opening
        // without truncate performs no repair or manifest rewrite.
        drop(
            fs::OpenOptions::new()
                .write(true)
                .open(&manifest_path)
                .expect("rejected_v2_does_not_retain_manifest_file_pin"),
        );
        files::assert_tree(fixture.root.path(), &before);
        files::assert_tree(&paths.origin, &source);
        no_eula_claim(&paths);
    }
}

#[derive(Clone, Copy)]
enum PlanApi {
    Entry,
    Producer,
}
fn status_plan(
    prepared: PreparedStatus,
    api: PlanApi,
) -> std::result::Result<producer::DedicatedPlan, producer::DedicatedError> {
    match api {
        PlanApi::Entry => plan_status_execution(prepared),
        PlanApi::Producer => producer::plan(prepared),
    }
}

#[test]
fn status_plan_kind_gate_precedes_recipe_temp_io_with_positive_no_start_controls() {
    for api in [PlanApi::Entry, PlanApi::Producer] {
        let fixture = Fixture::new();
        let prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        let before = files::tree(fixture.root.path());
        assert!(!paths.copy.join(".connect-run-temp").exists());
        let plan = status_plan(prepared, api).expect("positive_v1_status_plan_without_start");
        // This test never calls DedicatedPlan::start. Building the compiled
        // recipe/review alone must not create temp, claim, or mutate EULA.
        assert!(!paths.copy.join(".connect-run-temp").exists());
        files::assert_tree(fixture.root.path(), &before);
        no_eula_claim(&paths);
        drop(plan);
        unlocked(&paths);

        let prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        fs::create_dir(paths.copy.join(".connect-run-temp")).unwrap();
        let before = files::tree(fixture.root.path());
        assert_eq!(
            status_plan(prepared, api).err(),
            Some(producer::DedicatedError::Preparation(Error::Consumed))
        );
        files::assert_tree(fixture.root.path(), &before);
        no_eula_claim(&paths);
        unlocked(&paths);

        let mut prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        let decoded = decoded_voice_manifest(&prepared, StoredVoiceProfileV2::Svc);
        prepared.bundle.manifest = decoded;
        fs::create_dir(paths.copy.join(".connect-run-temp")).unwrap();
        let before = files::tree(fixture.root.path());
        assert_eq!(
            status_plan(prepared, api).err(),
            Some(producer::DedicatedError::Preparation(Error::Identity))
        );
        // If recipe IO won, the same existing directory would yield Consumed
        // just as in the V1 control. No counter or forged spawn report is used.
        files::assert_tree(fixture.root.path(), &before);
        no_eula_claim(&paths);
        unlocked(&paths);
    }
}
