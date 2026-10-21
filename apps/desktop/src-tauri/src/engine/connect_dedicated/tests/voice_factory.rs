//! B2 factories and physical admission use real diagnostic provenance with
//! explicitly injected small test pins. No test here starts Java or a child.
use super::voice_fixture::{VoiceFixture, PROFILES};
use super::*;
use crate::engine::{
    acquire_instance_recovery_lock,
    connect_voice::dedicated::{
        manifest::{decode_manifest, Manifest, StoredVoiceProfileV2},
        NativeVoiceProfile,
    },
};
use std::{collections::BTreeSet, sync::atomic::Ordering};

fn no_claim(paths: &TestPaths) {
    assert_eq!(bytes(&target(paths)), EULA_FALSE);
    assert_eq!(fs::read_dir(&paths.transaction).unwrap().count(), 0);
    assert!(!slot(paths, "claim.json").exists());
}

fn reopened_and_validated(fixture: &VoiceFixture, id: BundleId) -> factory::Bundle {
    let bundle = fixture.reopen(id).expect("voice_explicit_fixture_reopen");
    let guard = acquire_instance_recovery_lock(&bundle.copy).expect("voice_existing_copy_guard");
    bundle
        .validate(&guard)
        .expect("voice_explicit_fixture_validate");
    drop(guard);
    bundle
}

fn names(records: &[resources::ResourceRecord]) -> BTreeSet<&str> {
    records.iter().map(|record| record.name.as_str()).collect()
}

#[test]
fn both_voice_factories_create_exact_profiles_baselines_and_real_private_provenance() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let stage_before = files::tree(&fixture.fixture.stage);
        let jdk_before = files::tree(&fixture.fixture.jdk);
        let prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        let id = prepared.bundle_id();
        let source_before = files::tree(&paths.origin);
        let manifest = prepared.bundle.manifest.voice_v2().expect("voice_kind_v2");
        assert_eq!(manifest.schema, 2);
        assert_eq!(manifest.profile, fixture.stored_profile());
        assert_eq!(manifest.installed.len(), 9 + fixture.mod_names().len());
        assert_eq!(manifest.resources.len(), 11 + fixture.mod_names().len());
        assert_eq!(manifest.provenance.len(), 6);
        let mut installed: BTreeSet<String> = resources::fixture_artifacts()
            .into_iter()
            .map(|(name, _)| name.to_owned())
            .collect();
        installed.extend(fixture.mod_names().iter().map(|name| (*name).to_owned()));
        assert_eq!(
            names(&manifest.installed),
            installed
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
        );
        let mut observed = installed.clone();
        observed.insert("jdk/release".into());
        observed.insert("jdk/java.exe".into());
        assert_eq!(
            names(&manifest.resources),
            observed.iter().map(String::as_str).collect::<BTreeSet<_>>()
        );
        let actual_mods: BTreeSet<String> = fs::read_dir(paths.copy.join("mods"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(
            actual_mods,
            fixture
                .mod_names()
                .iter()
                .map(|name| name.strip_prefix("mods/").unwrap().to_owned())
                .collect::<BTreeSet<_>>()
        );
        for other in PROFILES {
            if other != profile {
                let other = match other {
                    NativeVoiceProfile::SimpleVoiceChat => StoredVoiceProfileV2::Svc,
                    NativeVoiceProfile::PlasmoVoice => StoredVoiceProfileV2::Plasmo,
                };
                assert!(!paths.copy.join(other.configuration_name()).exists());
            }
        }
        let (configuration, content) =
            observe_file(&fixture.configuration_path(&paths), 256 * 1024)
                .expect("voice_created_baseline_observation");
        assert_eq!(content, fixture.stored_profile().configuration());
        assert_eq!(
            configuration,
            manifest.initial_voice_configuration.observation
        );
        assert_eq!(
            configuration.sha256,
            fixture.stored_profile().configuration_sha256()
        );
        assert_eq!(
            manifest.initial_voice_configuration.name,
            fixture.stored_profile().configuration_name()
        );
        assert!(!manifest
            .installed
            .iter()
            .any(|record| { record.name == manifest.initial_voice_configuration.name }));
        assert_eq!(
            bytes(&paths.copy.join("server.properties")),
            resources::PROPERTIES
        );
        assert!(fs::read_dir(paths.origin.join("mods"))
            .unwrap()
            .next()
            .is_none());
        assert!(!paths
            .origin
            .join(fixture.stored_profile().configuration_name())
            .exists());
        assert!(diagnostic_lab::is_lab_candidate(&fixture::copy_instance(
            &paths
        )));
        locked(&paths);
        no_claim(&paths);
        let whole_before = files::tree(fixture.fixture.root.path());
        prepared
            .bundle
            .validate(&prepared.guard)
            .expect("voice_readonly_admission");
        files::assert_tree(fixture.fixture.root.path(), &whole_before);
        drop(prepared);
        unlocked(&paths);
        drop(reopened_and_validated(&fixture, id));
        no_claim(&paths);
        files::assert_tree(&paths.origin, &source_before);
        files::assert_tree(&fixture.fixture.stage, &stage_before);
        files::assert_tree(&fixture.fixture.jdk, &jdk_before);
    }
}

#[test]
fn voice_factory_consent_wrong_digest_and_expiry_have_real_creation_controls() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let before = files::tree(fixture.fixture.root.path());
        assert_eq!(
            factory::fixture_voice_plan(
                &fixture.fixture.lab,
                &fixture.fixture.stage,
                &fixture.fixture.jdk,
                profile,
                false,
                Hooks::default(),
            )
            .err(),
            Some(Error::Consent)
        );
        files::assert_tree(fixture.fixture.root.path(), &before);
        assert_eq!(
            fixture.plan(Hooks::default()).create(&"0".repeat(64)).err(),
            Some(Error::Digest)
        );
        files::assert_tree(fixture.fixture.root.path(), &before);
        let mut plan = fixture.plan(Hooks::default());
        let approval = plan.review().digest.clone();
        plan.expire_for_test();
        assert_eq!(plan.create(&approval).err(), Some(Error::Expired));
        files::assert_tree(fixture.fixture.root.path(), &before);
        let prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        assert_ne!(files::tree(fixture.fixture.root.path()), before);
        no_claim(&paths);
        drop(prepared);
        unlocked(&paths);
    }
}

#[test]
fn voice_stage_pins_reject_changed_bytes_and_hold_the_same_approved_resource() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let path = fixture.fixture.stage.join(fixture.mod_names()[0]);
        // An ordinary write open works before Resources has pinned this same
        // private file. The approved plan then denies that access on Windows.
        drop(fs::OpenOptions::new().write(true).open(&path).unwrap());
        let plan = fixture.plan(Hooks::default());
        assert_eq!(
            fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .err()
                .unwrap()
                .raw_os_error(),
            Some(32),
            "voice_selected_pin_denies_write_sharing_violation"
        );
        let approval = plan.review().digest.clone();
        let prepared = plan
            .create(&approval)
            .expect("voice_plan_uses_original_pinned_resource");
        no_claim(&prepared.test_paths());
        drop(prepared);

        let rejected = VoiceFixture::new(profile);
        let path = rejected.fixture.stage.join(rejected.mod_names()[0]);
        let independent = b"independent source bytes outside the fixed fixture catalogue\n";
        fs::write(&path, independent).unwrap();
        let before = files::tree(rejected.fixture.root.path());
        assert_eq!(
            factory::fixture_voice_plan(
                &rejected.fixture.lab,
                &rejected.fixture.stage,
                &rejected.fixture.jdk,
                profile,
                true,
                Hooks::default(),
            )
            .err(),
            Some(Error::Resources)
        );
        files::assert_tree(rejected.fixture.root.path(), &before);
        assert_eq!(bytes(&path), independent);
    }
}

#[test]
fn normal_reopen_rejects_the_same_small_fixture_that_explicit_fixture_pins_admit() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let prepared = fixture.prepared(Hooks::default());
        let paths = prepared.test_paths();
        let id = prepared.bundle_id();
        prepared.bundle.validate(&prepared.guard).unwrap();
        drop(prepared);
        unlocked(&paths);
        drop(reopened_and_validated(&fixture, id));
        let before = files::tree(fixture.fixture.root.path());
        // Normal reopen remains Compiled even in this cfg(test) executable.
        // The exact error distinguishes catalogue rejection from path/kind or
        // malformed-count rejection before the actual pin check.
        assert_eq!(
            factory::Bundle::reopen(&fixture.fixture.lab, id).err(),
            Some(Error::Resources)
        );
        files::assert_tree(fixture.fixture.root.path(), &before);
        no_claim(&paths);
        drop(reopened_and_validated(&fixture, id));
    }
}

#[derive(Clone, Copy)]
enum RecordMutation {
    MissingInstalled,
    MissingResource,
    MissingProvenance,
    DuplicateInstalled,
    DuplicateResource,
    DuplicateProvenance,
    TraversalInstalled,
    AliasJdk,
    CoherentOtherProfile,
}

#[test]
fn decoded_v2_lists_profiles_and_names_are_closed_before_record_derived_opens() {
    for profile in PROFILES {
        for mutation in [
            RecordMutation::MissingInstalled,
            RecordMutation::MissingResource,
            RecordMutation::MissingProvenance,
            RecordMutation::DuplicateInstalled,
            RecordMutation::DuplicateResource,
            RecordMutation::DuplicateProvenance,
            RecordMutation::TraversalInstalled,
            RecordMutation::AliasJdk,
            RecordMutation::CoherentOtherProfile,
        ] {
            let fixture = VoiceFixture::new(profile);
            let prepared = fixture.prepared(Hooks::default());
            let paths = prepared.test_paths();
            let id = prepared.bundle_id();
            let mut manifest = prepared.bundle.manifest.clone();
            drop(prepared);
            drop(reopened_and_validated(&fixture, id));
            let Manifest::VoiceV2(voice) = &mut manifest else {
                panic!("voice_factory_must_produce_v2");
            };
            match mutation {
                RecordMutation::MissingInstalled => {
                    voice.installed.pop();
                }
                RecordMutation::MissingResource => {
                    voice.resources.pop();
                }
                RecordMutation::MissingProvenance => {
                    voice.provenance.pop();
                }
                RecordMutation::DuplicateInstalled => {
                    *voice.installed.last_mut().unwrap() = voice.installed[0].clone();
                }
                RecordMutation::DuplicateResource => {
                    *voice.resources.last_mut().unwrap() = voice.resources[0].clone();
                }
                RecordMutation::DuplicateProvenance => {
                    *voice.provenance.last_mut().unwrap() = voice.provenance[0].clone();
                }
                RecordMutation::TraversalInstalled => {
                    voice.installed[0].name = "../must-not-be-opened.bin".into();
                }
                RecordMutation::AliasJdk => {
                    voice
                        .resources
                        .iter_mut()
                        .find(|record| record.name == "jdk/java.exe")
                        .unwrap()
                        .name = "jdk/Java.exe".into();
                }
                RecordMutation::CoherentOtherProfile => {
                    let other = match voice.profile {
                        StoredVoiceProfileV2::Svc => StoredVoiceProfileV2::Plasmo,
                        StoredVoiceProfileV2::Plasmo => StoredVoiceProfileV2::Svc,
                    };
                    voice.profile = other;
                    voice.initial_voice_configuration.name = other.configuration_name().into();
                    voice.initial_voice_configuration.observation.bytes =
                        other.configuration().len() as u64;
                    voice.initial_voice_configuration.observation.sha256 =
                        other.configuration_sha256().into();
                }
            }
            let encoded = encode(&manifest).unwrap();
            assert_eq!(
                decode_manifest(&encoded).unwrap(),
                manifest,
                "voice_attack_is_valid_wire_v2"
            );
            fs::write(paths.bundle.join("bundle.json"), &encoded).unwrap();
            let before = files::tree(fixture.fixture.root.path());
            assert_eq!(fixture.reopen(id).err(), Some(Error::Identity));
            files::assert_tree(fixture.fixture.root.path(), &before);
            // A leaked manifest SecureFile would deny this non-truncating
            // write open. No cleanup or repair is performed by rejection.
            drop(
                fs::OpenOptions::new()
                    .write(true)
                    .open(paths.bundle.join("bundle.json"))
                    .unwrap(),
            );
            no_claim(&paths);
            unlocked(&paths);
        }
    }
}

#[derive(Clone, Copy)]
enum DiskMutation {
    ExtraMod,
    ExtraLibrary,
    MissingMod,
    DifferentBytes,
    SameBytesNewIdentity,
    HardLink,
}

#[test]
fn v2_immutable_inventory_catalogue_identity_and_link_failures_preserve_the_copy() {
    for profile in PROFILES {
        for mutation in [
            DiskMutation::ExtraMod,
            DiskMutation::ExtraLibrary,
            DiskMutation::MissingMod,
            DiskMutation::DifferentBytes,
            DiskMutation::SameBytesNewIdentity,
            DiskMutation::HardLink,
        ] {
            let fixture = VoiceFixture::new(profile);
            let prepared = fixture.prepared(Hooks::default());
            let paths = prepared.test_paths();
            let id = prepared.bundle_id();
            let selected = paths.copy.join(fixture.mod_names()[0]);
            let (original, content) = observe_file(&selected, 64 * 1024 * 1024).unwrap();
            drop(prepared);
            drop(reopened_and_validated(&fixture, id));
            let expected = match mutation {
                DiskMutation::ExtraMod => {
                    fs::write(paths.copy.join("mods/foreign.jar"), b"unselected extra").unwrap();
                    Error::Identity
                }
                DiskMutation::ExtraLibrary => {
                    fs::write(
                        paths.copy.join("libraries/foreign.jar"),
                        b"unselected extra",
                    )
                    .unwrap();
                    Error::Identity
                }
                DiskMutation::MissingMod => {
                    fs::rename(
                        &selected,
                        fixture.fixture.root.path().join("retained-missing-mod.bin"),
                    )
                    .unwrap();
                    Error::Identity
                }
                DiskMutation::DifferentBytes => {
                    fs::write(
                        &selected,
                        b"bytes outside the approved resource catalogue\n",
                    )
                    .unwrap();
                    assert_eq!(
                        observe_file(&selected, 64 * 1024 * 1024)
                            .unwrap()
                            .0
                            .identity,
                        original.identity
                    );
                    Error::Resources
                }
                DiskMutation::SameBytesNewIdentity => {
                    fs::rename(
                        &selected,
                        fixture
                            .fixture
                            .root
                            .path()
                            .join("retained-approved-mod.bin"),
                    )
                    .unwrap();
                    fs::write(&selected, &content).unwrap();
                    let replacement = observe_file(&selected, 64 * 1024 * 1024).unwrap().0;
                    assert_ne!(replacement.identity, original.identity);
                    assert_eq!(replacement.bytes, original.bytes);
                    assert_eq!(replacement.sha256, original.sha256);
                    Error::Changed
                }
                DiskMutation::HardLink => {
                    fs::hard_link(
                        &selected,
                        fixture
                            .fixture
                            .root
                            .path()
                            .join("independent-hard-link.bin"),
                    )
                    .unwrap();
                    Error::UnsafePath
                }
            };
            let before = files::tree(fixture.fixture.root.path());
            assert_eq!(fixture.reopen(id).err(), Some(expected));
            files::assert_tree(fixture.fixture.root.path(), &before);
            no_claim(&paths);
            unlocked(&paths);
        }
    }
}

#[test]
fn voice_recovery_uses_existing_store_without_initializing_or_repairing_it() {
    for profile in PROFILES {
        for sidecar in [false, true] {
            let fixture = VoiceFixture::new(profile);
            let prepared = fixture.prepared(fixture::hook_fault(Step::AppliedMarked));
            let paths = prepared.test_paths();
            let id = prepared.bundle_id();
            let approval = prepared.review().digest.clone();
            assert_eq!(prepared.apply_eula(&approval).err(), Some(Error::Injected));
            unlocked(&paths);
            // A real recoverable transaction with the original database is
            // accepted first. Planning alone neither restores nor rewrites it.
            drop(
                fixture
                    .recover(id, &probe::Probe::default(), Hooks::default())
                    .unwrap(),
            );
            let native = paths.bundle.join("native");
            let database = native.join(".aurora/recovery/index.sqlite3");
            assert!(database.is_file());
            let expected = if sidecar {
                fs::write(
                    native.join(".aurora/recovery/index.sqlite3-wal"),
                    b"independent sidecar",
                )
                .unwrap();
                Error::NeedsRecovery
            } else {
                fs::rename(
                    &database,
                    fixture.fixture.root.path().join("retained-index.sqlite3"),
                )
                .unwrap();
                Error::Identity
            };
            let before = files::tree(fixture.fixture.root.path());
            let hooks = Hooks::default();
            assert_eq!(
                fixture
                    .recover(id, &probe::Probe::default(), hooks.clone())
                    .err(),
                Some(expected)
            );
            assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
            files::assert_tree(fixture.fixture.root.path(), &before);
            assert_eq!(bytes(&target(&paths)), EULA_TRUE);
            assert_eq!(bytes(&slot(&paths, "old.bin")), EULA_FALSE);
            if !sidecar {
                assert!(!database.exists());
            }
            unlocked(&paths);
        }
    }
}

#[derive(Clone, Copy)]
enum StatusApi {
    Entry,
    Producer,
}

fn status_plan(
    prepared: PreparedStatus,
    api: StatusApi,
) -> std::result::Result<producer::DedicatedPlan, producer::DedicatedError> {
    match api {
        StatusApi::Entry => plan_status_execution(prepared),
        StatusApi::Producer => producer::plan(prepared),
    }
}

#[test]
fn status_apis_reject_real_admitted_voice_preparations_before_recipe_io() {
    for profile in PROFILES {
        for api in [StatusApi::Entry, StatusApi::Producer] {
            let fixture = VoiceFixture::new(profile);
            let control = fixture.fixture.prepared(Hooks::default());
            let control_paths = control.test_paths();
            let before = files::tree(fixture.fixture.root.path());
            let plan =
                status_plan(control, api).expect("voice_gate_positive_real_status_plan_no_start");
            files::assert_tree(fixture.fixture.root.path(), &before);
            no_claim(&control_paths);
            drop(plan);
            unlocked(&control_paths);

            let prepared = fixture.prepared(Hooks::default());
            let paths = prepared.test_paths();
            prepared
                .bundle
                .validate(&prepared.guard)
                .expect("voice_gate_real_v2_admitted");
            fs::create_dir(paths.copy.join(".connect-run-temp")).unwrap();
            let before = files::tree(fixture.fixture.root.path());
            assert_eq!(
                status_plan(prepared, api).err(),
                Some(producer::DedicatedError::Preparation(Error::Identity))
            );
            // The real V2 factory/physical admission succeeded above. Recipe
            // IO would instead encounter the existing temp directory.
            files::assert_tree(fixture.fixture.root.path(), &before);
            no_claim(&paths);
            unlocked(&paths);
        }
    }
}

#[test]
fn voice_bundles_cannot_transfer_authority_with_a_new_lab_or_bundle_name() {
    for profile in PROFILES {
        for another_lab in [false, true] {
            let fixture = VoiceFixture::new(profile);
            let prepared = fixture.prepared(Hooks::default());
            let paths = prepared.test_paths();
            let original_id = prepared.bundle_id();
            let original_manifest = bytes(&paths.bundle.join("bundle.json"));
            let original_bundle_identity = prepared.bundle.manifest.bundle_identity().clone();
            let original_native_identity = prepared.bundle.manifest.native_identity().clone();
            let original_lab_identity = prepared.bundle.manifest.lab_identity().clone();
            drop(prepared);
            drop(reopened_and_validated(&fixture, original_id));

            let (destination_lab, requested_id) = if another_lab {
                let lab = fixture.fixture.root.path().join("another-voice-lab");
                fs::create_dir(&lab).unwrap();
                assert_ne!(directory_identity(&lab).unwrap(), original_lab_identity);
                (lab, original_id)
            } else {
                (fixture.fixture.lab.clone(), BundleId(Uuid::new_v4()))
            };
            let destination = destination_lab.join(format!("native-status-{}", requested_id.0));
            assert_eq!(absent(&destination), Ok(()));
            // Both ends of this directory move are inside this exact generated
            // TestRoot. Resolve the existing parent before moving its own data.
            let own_root = fixture.fixture.root.path().canonicalize().unwrap();
            assert!(paths.bundle.canonicalize().unwrap().starts_with(&own_root));
            assert!(destination_lab
                .canonicalize()
                .unwrap()
                .starts_with(&own_root));
            let before_move = files::tree(fixture.fixture.root.path());
            fs::rename(&paths.bundle, &destination).unwrap();
            assert_eq!(
                directory_identity(&destination).unwrap(),
                original_bundle_identity
            );
            assert_eq!(
                directory_identity(&destination.join("native")).unwrap(),
                original_native_identity
            );
            assert_eq!(bytes(&destination.join("bundle.json")), original_manifest);
            let moved_tree = files::tree(fixture.fixture.root.path());
            assert_eq!(
                factory::Bundle::reopen_fixture(
                    &destination_lab,
                    requested_id,
                    resources::fixture_voice_pins(profile),
                )
                .err(),
                Some(Error::Identity)
            );
            files::assert_tree(fixture.fixture.root.path(), &moved_tree);

            // Return the same physical tree, then exercise the original
            // admitted path again. No replacement IDs or journal are created.
            assert_eq!(absent(&paths.bundle), Ok(()));
            fs::rename(&destination, &paths.bundle).unwrap();
            drop(reopened_and_validated(&fixture, original_id));
            assert_eq!(bytes(&paths.bundle.join("bundle.json")), original_manifest);
            files::assert_tree(fixture.fixture.root.path(), &before_move);
            no_claim(&paths);
            unlocked(&paths);
        }
    }
}
