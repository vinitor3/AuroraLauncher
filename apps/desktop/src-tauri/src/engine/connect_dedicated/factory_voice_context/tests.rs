use super::super::super::*;
use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
};

// Reuse only the existing confined RAII root/tree observer. This suite creates
// fresh physical factories and never selects one of the retained B0 bundles.
use super::super::super::tests::files;

const PROFILES: [NativeVoiceProfile; 2] = [
    NativeVoiceProfile::SimpleVoiceChat,
    NativeVoiceProfile::PlasmoVoice,
];

struct Fixture {
    root: files::TestRoot,
    lab: PathBuf,
    stage: PathBuf,
    jdk: PathBuf,
    profile: NativeVoiceProfile,
}
impl Fixture {
    fn new(profile: NativeVoiceProfile) -> Self {
        let root = files::TestRoot::new();
        let lab = root.path().join("lab");
        let stage = lab.join("prepared-VoiceContextC1");
        let jdk = root.path().join("jdk");
        fs::create_dir(&lab).expect("context_lab");
        fs::create_dir(&stage).expect("context_stage");
        fs::create_dir_all(jdk.join("bin")).expect("context_jdk");
        for (name, bytes) in resources::fixture_artifacts()
            .into_iter()
            .chain(resources::fixture_voice_artifacts(profile))
        {
            let path = stage.join(name);
            fs::create_dir_all(path.parent().expect("context_artifact_parent"))
                .expect("context_artifact_directory");
            fs::write(path, bytes).expect("context_artifact");
        }
        let (release, java) = resources::fixture_jdk();
        fs::write(jdk.join("release"), release).expect("context_jdk_release");
        fs::write(jdk.join("bin/java.exe"), java).expect("context_jdk_java");
        Self {
            root,
            lab,
            stage,
            jdk,
            profile,
        }
    }
    fn prepared(&self) -> PreparedStatus {
        let plan = super::super::fixture_voice_plan(
            &self.lab,
            &self.stage,
            &self.jdk,
            self.profile,
            true,
            Hooks::default(),
        )
        .expect("context_voice_plan");
        let approved = plan.review().digest.clone();
        plan.create(&approved).expect("context_voice_create")
    }
    fn context(&self) -> ImmutableVoiceContext {
        let prepared = self.prepared();
        let context = ImmutableVoiceContext::from_prepared(&prepared, Uuid::new_v4())
            .expect("context_capture");
        drop(prepared);
        context
    }
}

/// Restores this exact fixture-owned name in unwind as well as normal paths.
/// It removes only a replacement regular file or empty directory, never a tree.
struct ParkedPath {
    original: PathBuf,
    parked: PathBuf,
    active: bool,
}
impl ParkedPath {
    fn new(original: &Path, root: &files::TestRoot) -> Self {
        let confined = root.path().canonicalize().expect("context_confined_root");
        assert!(original
            .canonicalize()
            .expect("context_original_path")
            .starts_with(&confined));
        let parked = root.path().join(format!("context-held-{}", Uuid::new_v4()));
        assert!(!parked.exists(), "context_park_absent");
        fs::rename(original, &parked).expect("context_park_path");
        Self {
            original: original.into(),
            parked,
            active: true,
        }
    }
    fn restore(&mut self) -> std::io::Result<()> {
        if !self.active {
            return Ok(());
        }
        match fs::symlink_metadata(&self.original) {
            Ok(metadata) if metadata.is_file() => fs::remove_file(&self.original)?,
            Ok(metadata) if metadata.is_dir() => fs::remove_dir(&self.original)?,
            Ok(_) => return Err(std::io::ErrorKind::InvalidData.into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(&self.parked, &self.original)?;
        self.active = false;
        Ok(())
    }
}
impl Drop for ParkedPath {
    fn drop(&mut self) {
        let restored = self.restore();
        if !std::thread::panicking() {
            restored.expect("context_restore_path");
        }
    }
}

#[test]
fn captures_both_real_voice_preparations_as_values_and_rejects_status_before_path_io() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let prepared = fixture.prepared();
        let nonce = Uuid::new_v4();
        let before = files::tree(fixture.root.path());
        let context = ImmutableVoiceContext::from_prepared(&prepared, nonce)
            .expect("context_from_real_prepared");
        assert_eq!(
            context.profile(),
            prepared.bundle.manifest.voice_v2().unwrap().profile
        );
        assert_eq!(context.native_nonce(), nonce);
        assert_eq!(context.configuration_root(), prepared.bundle.copy.root());
        assert_eq!(
            context.configuration_root_identity(),
            &directory_identity(prepared.bundle.copy.root()).unwrap()
        );
        context
            .validate_for_query()
            .expect("context_first_validation");
        files::assert_tree(fixture.root.path(), &before);
        drop(prepared);
        context
            .validate_for_query()
            .expect("context_without_prepared");
    }
    // A real V1 preparation remains a V1 negative even when its now-invalid
    // path would fail an attempted open. No voice preparation is relabelled.
    let fixture = Fixture::new(NativeVoiceProfile::SimpleVoiceChat);
    for (name, _) in resources::fixture_voice_artifacts(fixture.profile) {
        fs::remove_file(fixture.stage.join(name)).expect("context_status_stage_mod_remove");
    }
    let plan = super::super::plan(
        &fixture.lab,
        &fixture.stage,
        &fixture.jdk,
        true,
        resources::fixture_pins(),
        Hooks::default(),
    )
    .expect("context_status_plan");
    let approval = plan.review().digest.clone();
    let mut prepared = plan.create(&approval).expect("context_status_create");
    prepared.bundle.path = fixture.root.path().join("not-a-bundle");
    assert_eq!(
        ImmutableVoiceContext::from_prepared(&prepared, Uuid::new_v4()).err(),
        Some(Error::Identity)
    );
}

#[test]
fn does_not_initialize_recovery_or_read_mutable_configuration_or_game_tree() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let context = fixture.context();
        // Recovery's SQLite/CAS root is shared under the native data root,
        // not under either instances/<id> directory. Park the actual store.
        let recovery = context.native.path.join(".aurora/recovery");
        assert!(
            recovery.join("index.sqlite3").is_file(),
            "context_existing_recovery_database"
        );
        let parked = ParkedPath::new(&recovery, &fixture.root);
        let configuration = context
            .configuration_root()
            .join(context.profile().configuration_name());
        let mut config = ParkedPath::new(&configuration, &fixture.root);
        // These are intentionally incompatible mutable data. A Recovery tree
        // inventory or a voice-config read is not needed by context validation.
        fs::write(&configuration, [0xff, 0, b'\r']).expect("context_invalid_config");
        fs::create_dir_all(context.copy.path.join("connect-lab-world/data"))
            .expect("context_game_directory");
        fs::write(
            context.copy.path.join("connect-lab-world/data/changed.dat"),
            b"live-game-data",
        )
        .expect("context_game_data");
        let before = files::tree(fixture.root.path());
        context
            .validate_for_query()
            .expect("context_no_recovery_or_config");
        files::assert_tree(fixture.root.path(), &before);
        assert!(!recovery.exists(), "context_no_store_init");
        fs::remove_file(&configuration).expect("context_remove_invalid_config");
        context
            .validate_for_query()
            .expect("context_missing_config_is_not_observed");
        config.restore().expect("context_config_restore");
        drop(parked);
        context
            .validate_for_query()
            .expect("context_restored_fixture");
    }
}

#[test]
fn retained_context_and_clone_hold_no_directory_or_artifact_handles() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let context = fixture.context();
        let retained = context.clone();
        // The immediate successful renames are causal release checks. The
        // original preparation was dropped; neither context is dropped here.
        for path in [
            &context.copy.path,
            &context.origin.path,
            &context.native.path,
            &context.bundle.path,
            &context.lab.path,
            &fixture.stage,
            &fixture.jdk,
        ] {
            let mut parked = ParkedPath::new(path, &fixture.root);
            parked
                .restore()
                .expect("context_directory_handles_released");
            retained
                .validate_for_query()
                .expect("context_revalidate_after_directory_rename");
        }
        let artifact = context.copy.path.join(&context.manifest.installed[0].name);
        let mut parked = ParkedPath::new(&artifact, &fixture.root);
        parked.restore().expect("context_file_handles_released");
        context
            .validate_for_query()
            .expect("context_no_retained_file_handle");
    }
}

#[test]
fn physical_root_replacement_cannot_be_recaptured_as_a_new_valid_context() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let context = fixture.context();
        for directory in [
            &context.lab,
            &context.bundle,
            &context.native,
            &context.copy,
            &context.origin,
        ] {
            let mut parked = ParkedPath::new(&directory.path, &fixture.root);
            fs::create_dir(&directory.path).expect("context_replacement_directory");
            assert_ne!(
                directory_identity(&directory.path).unwrap(),
                directory.identity
            );
            assert_eq!(context.validate_for_query(), Err(Error::Changed));
            parked
                .restore()
                .expect("context_original_directory_restore");
            context
                .validate_for_query()
                .expect("context_original_directory_still_valid");
        }
    }
}

#[test]
fn explicit_fixture_pins_never_become_the_compiled_default_even_in_test_binary() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let context = fixture.context();
        context
            .validate_for_query()
            .expect("context_explicit_fixture_positive");
        let mut compiled = context.clone();
        compiled.test_expected_pins = None;
        assert_eq!(compiled.validate_for_query(), Err(Error::Resources));
        context
            .validate_for_query()
            .expect("context_explicit_fixture_unchanged");
    }
}

#[test]
fn changed_source_stage_or_jdk_snapshot_is_rejected_without_adopting_its_identity() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let context = fixture.context();
        for name in ["server.jar", "jdk/release", "jdk/java.exe"] {
            let (_, path, observation) = context
                .resource_observations
                .iter()
                .find(|record| record.0 == name)
                .expect("context_resource_record");
            let original = fs::read(path).expect("context_resource_bytes");
            let mut parked = ParkedPath::new(path, &fixture.root);
            fs::write(path, &original).expect("context_resource_identical_replacement");
            assert_ne!(
                observe_file(path, observation.bytes).unwrap().0.identity,
                observation.identity
            );
            assert_eq!(context.validate_for_query(), Err(Error::Changed));
            parked.restore().expect("context_resource_restore");
            context
                .validate_for_query()
                .expect("context_original_resource_valid");
        }
    }
}

#[test]
fn installed_catalog_rejects_wrong_bytes_new_identity_links_and_extra_or_missing_names() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let context = fixture.context();
        let name = resources::fixture_voice_artifacts(profile)[0].0;
        let artifact = context.copy.path.join(name);
        let original = fs::read(&artifact).expect("context_installed_bytes");
        let mut parked = ParkedPath::new(&artifact, &fixture.root);
        let mut wrong = original.clone();
        wrong[0] ^= 1;
        fs::write(&artifact, wrong).expect("context_installed_wrong_bytes");
        assert_eq!(context.validate_for_query(), Err(Error::Resources));
        fs::write(&artifact, &original).expect("context_installed_identical_bytes");
        assert_eq!(context.validate_for_query(), Err(Error::Changed));
        parked
            .restore()
            .expect("context_original_installed_restore");
        let linked = context.copy.path.join("context-hard-link");
        fs::hard_link(&artifact, &linked).expect("context_installed_hard_link");
        assert_eq!(context.validate_for_query(), Err(Error::UnsafePath));
        fs::remove_file(&linked).expect("context_installed_unlink");
        for directory in ["mods", "libraries"] {
            let extra = context.copy.path.join(directory).join("unapproved.jar");
            fs::write(&extra, b"extra").expect("context_extra_artifact");
            assert_eq!(context.validate_for_query(), Err(Error::Identity));
            fs::remove_file(extra).expect("context_extra_remove");
        }
        let mut missing = ParkedPath::new(&artifact, &fixture.root);
        assert_eq!(context.validate_for_query(), Err(Error::Identity));
        missing.restore().expect("context_missing_restore");
        context
            .validate_for_query()
            .expect("context_catalog_original_valid");
    }
}

#[test]
fn manifest_and_all_six_provenance_records_keep_the_original_observation() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let context = fixture.context();
        let manifest_path = context.bundle.path.join("bundle.json");
        let original = fs::read(&manifest_path).expect("context_manifest_bytes");
        let mut parked = ParkedPath::new(&manifest_path, &fixture.root);
        fs::write(&manifest_path, &original).expect("context_identical_manifest_replacement");
        assert_eq!(context.validate_for_query(), Err(Error::Changed));
        let duplicate = String::from_utf8(original.clone()).unwrap().replacen(
            "\"schema\":2",
            "\"schema\":2,\"schema\":2",
            1,
        );
        fs::write(&manifest_path, duplicate).expect("context_duplicate_schema");
        // The existing B1 codec rejects duplicate schema keys as malformed input.
        assert_eq!(context.validate_for_query(), Err(Error::NeedsRecovery));
        parked.restore().expect("context_manifest_restore");
        for record in &context.manifest.provenance {
            let path = context.native.path.join(&record.name);
            let bytes = fs::read(&path).expect("context_provenance_bytes");
            let mut parked = ParkedPath::new(&path, &fixture.root);
            fs::write(&path, bytes).expect("context_identical_provenance_replacement");
            assert_eq!(context.validate_for_query(), Err(Error::Changed));
            parked.restore().expect("context_provenance_restore");
            context
                .validate_for_query()
                .expect("context_original_provenance_valid");
        }
    }
}
