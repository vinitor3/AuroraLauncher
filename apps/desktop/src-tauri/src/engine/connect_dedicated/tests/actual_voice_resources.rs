//! Separate opt-ins for the two production voice factories. The operator
//! reviews actual digests before file creation and EULA application. No Java.
use super::*;
use std::{fs::OpenOptions, thread};

const COMMON: [&str; 9] = [
    "server.jar",
    "libraries/asm-9.8.jar",
    "libraries/asm-analysis-9.8.jar",
    "libraries/asm-commons-9.8.jar",
    "libraries/asm-tree-9.8.jar",
    "libraries/asm-util-9.8.jar",
    "libraries/sponge-mixin-0.15.5+mixin.0.8.7.jar",
    "libraries/intermediary-1.21.1.jar",
    "libraries/fabric-loader-0.16.14.jar",
];

fn selected_resources(stage: &Path, jdk: &Path, extras: &[&str]) -> Vec<(String, Observation)> {
    let mut observed = Vec::new();
    for name in COMMON.iter().copied().chain(extras.iter().copied()) {
        observed.push((
            name.to_owned(),
            observe_file(&stage.join(name), 64 * 1024 * 1024)
                .expect("actual_voice_selected_artifact")
                .0,
        ));
    }
    for (name, path, limit) in [
        ("jdk/release", jdk.join("release"), 64 * 1024),
        ("jdk/java.exe", jdk.join("bin/java.exe"), 4 * 1024 * 1024),
    ] {
        observed.push((
            name.to_owned(),
            observe_file(&path, limit)
                .expect("actual_voice_selected_jdk")
                .0,
        ));
    }
    observed
}

fn publish_review(root: &Path, name: &str, review: &Review) {
    let document = serde_json::json!({
        "schema": 1,
        "reviewId": review.review_id,
        "bundleId": review.bundle_id,
        "purpose": review.purpose,
        "digest": review.digest,
        "before": review.before,
        "after": review.after,
        "details": review.details,
        "javaWillExecute": false,
        "externalNetwork": false,
        "personalConfigurationChanged": false,
        "approvalTtlSeconds": 60,
        "operations": ["new_private_copy", "claim", "snapshot", "apply_eula", "restore_eula"],
    });
    let path = root.join(name);
    recovery::write_new(&path, &serde_json::to_vec_pretty(&document).unwrap())
        .expect("actual_voice_review_create_new");
    println!("native_voice_files_review_ready:{}", path.display());
}

fn approval(root: &Path, name: &str, expected: &str, deadline: Instant) -> String {
    let path = root.join(name);
    loop {
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                let bytes =
                    recovery::read_bounded(&path, 128).expect("actual_voice_approval_bounded_read");
                let value = std::str::from_utf8(&bytes)
                    .expect("actual_voice_approval_utf8")
                    .trim();
                assert_eq!(value, expected, "actual_voice_approval_digest");
                assert!(Instant::now() < deadline, "actual_voice_approval_expired");
                return value.to_owned();
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => panic!("actual_voice_approval_metadata"),
        }
        assert!(Instant::now() < deadline, "actual_voice_approval_deadline");
        thread::sleep(Duration::from_millis(25));
    }
}

fn run(
    profile: NativeVoiceProfile,
    label: &str,
    extras: &[&str],
    configuration_name: &str,
    configuration_bytes: &[u8],
) {
    let stage =
        PathBuf::from(std::env::var_os("AURORA_DEDICATED_LAB_STAGE").expect("actual_voice_stage"));
    let jdk =
        PathBuf::from(std::env::var_os("AURORA_DEDICATED_LAB_JDK").expect("actual_voice_jdk"));
    let evidence = lab_root()
        .unwrap()
        .parent()
        .unwrap()
        .join("connect-validation")
        .join(format!(
            "native-voice85b2-{label}-review-{}",
            Uuid::new_v4()
        ));
    fs::create_dir(&evidence).expect("actual_voice_evidence_create_new");
    let _evidence_pins = recovery::pin_directory(&evidence).unwrap();
    let resources_before = selected_resources(&stage, &jdk, extras);
    let factory_deadline = Instant::now() + REVIEW_TTL;
    let plan = plan_voice_factory(&stage, &jdk, profile, true)
        .expect("actual_voice_production_factory_plan");
    assert_eq!(plan.review().purpose, "native_voice_factory_v2");
    publish_review(&evidence, "factory-review.json", plan.review());
    let digest = approval(
        &evidence,
        "factory-approve.txt",
        &plan.review().digest,
        factory_deadline,
    );
    let prepared = plan.create(&digest).expect("actual_voice_factory_create");
    let id = prepared.bundle_id();
    let paths = prepared.test_paths();
    let source_before = files::tree(&paths.origin);
    let recovery_database = paths.bundle.join("native/.aurora/recovery/index.sqlite3");
    let recovery_before = observe_file(&recovery_database, 4 * 1024 * 1024)
        .expect("actual_voice_existing_recovery_database");
    let manifest_before = bytes(&paths.bundle.join("bundle.json"));
    let manifest = manifest::decode_manifest(&manifest_before).unwrap();
    assert_eq!(manifest.schema(), 2);
    assert_eq!(manifest.installed().len(), COMMON.len() + extras.len());
    assert_eq!(manifest.resources().len(), resources_before.len());
    assert_eq!(manifest.provenance().len(), 6);
    let mut actual_mods: Vec<_> = fs::read_dir(paths.copy.join("mods"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    actual_mods.sort();
    let mut expected_mods: Vec<_> = extras
        .iter()
        .map(|name| name.strip_prefix("mods/").unwrap().to_string())
        .collect();
    expected_mods.sort();
    assert_eq!(
        actual_mods, expected_mods,
        "actual_voice_exact_installed_mods"
    );
    let configuration_path = paths.copy.join(configuration_name);
    let (initial_configuration, initial_bytes) =
        observe_file(&configuration_path, 256 * 1024).unwrap();
    assert_eq!(initial_bytes, configuration_bytes);
    assert_eq!(
        manifest
            .voice_v2()
            .unwrap()
            .initial_voice_configuration
            .observation,
        initial_configuration
    );
    // The immutable pins must leave the voice writer free, even while this
    // original preparation is alive. Opening here does not change its bytes.
    drop(
        OpenOptions::new()
            .write(true)
            .open(&configuration_path)
            .expect("actual_voice_configuration_not_pinned_against_writer"),
    );
    let writer_probe = configuration_path.with_file_name("aurora85-config-writer-probe.tmp");
    assert!(!writer_probe.exists(), "actual_voice_writer_probe_absent");
    fs::rename(&configuration_path, &writer_probe)
        .expect("actual_voice_preparation_does_not_deny_config_rename");
    fs::rename(&writer_probe, &configuration_path)
        .expect("actual_voice_config_rename_back_same_file");
    assert_eq!(
        observe_file(&configuration_path, 256 * 1024).unwrap(),
        (initial_configuration.clone(), configuration_bytes.to_vec())
    );
    assert_eq!(prepared.review().purpose, "native_voice_eula_v2");
    publish_review(&evidence, "eula-review.json", prepared.review());
    let digest = approval(
        &evidence,
        "eula-approve.txt",
        &prepared.review().digest,
        prepared.approval_deadline(),
    );
    let original_eula = observe_file(&target(&paths), 4096).unwrap();
    let applied = prepared
        .apply_eula(&digest)
        .expect("actual_voice_eula_apply");
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    assert_eq!(
        applied.restore_before_spawn().unwrap(),
        EulaOutcome::Restored
    );
    assert_eq!(observe_file(&target(&paths), 4096).unwrap(), original_eula);
    assert!(slot(&paths, "claim.json").is_file());
    unlocked(&paths);
    // This is the normal compiled-catalog reopen, not the small-fixture seam.
    let reopened =
        factory::Bundle::reopen(&lab_root().unwrap(), id).expect("actual_voice_compiled_reopen");
    let guard = crate::engine::acquire_instance_recovery_lock(&fixture::copy_instance(&paths))
        .expect("actual_voice_reopen_guard");
    reopened
        .validate(&guard)
        .expect("actual_voice_reopened_bundle_valid");
    drop(guard);
    drop(reopened);
    assert_eq!(bytes(&paths.bundle.join("bundle.json")), manifest_before);
    assert_eq!(
        observe_file(&configuration_path, 256 * 1024).unwrap(),
        (initial_configuration, configuration_bytes.to_vec())
    );
    files::assert_tree(&paths.origin, &source_before);
    assert_eq!(
        observe_file(&recovery_database, 4 * 1024 * 1024).unwrap(),
        recovery_before
    );
    assert_eq!(selected_resources(&stage, &jdk, extras), resources_before);
    unlocked(&paths);
    let result = serde_json::json!({
        "schema": 1,
        "event": "native_voice_actual_factory_files_passed",
        "profile": label,
        "eulaRestoredSameIdentityAndBytes": true,
        "claimConsumed": true,
        "compiledCatalogReopenPassed": true,
        "manifestUnchanged": true,
        "initialVoiceConfigurationUnchanged": true,
        "configurationWriteAndRenameUnblockedBeforeEula": true,
        "sourceUnchanged": true,
        "existingRecoveryDatabaseUnchanged": true,
        "selectedResourcesUnchanged": true,
        "lockReleased": true,
        "javaExecuted": false,
        "minecraftStatusTested": false,
        "voiceAudioTested": false,
        "personalConfigurationChanged": false,
        "externalNetworkTested": false,
        "publicRelayEnabled": false,
    });
    recovery::write_new(
        &evidence.join("result.json"),
        &serde_json::to_vec_pretty(&result).unwrap(),
    )
    .expect("actual_voice_result_create_new");
    println!("{result}");
}

#[test]
#[ignore = "explicit production SVC resources and two concrete file-only approvals; no Java"]
fn actual_pinned_svc_factory_eula_and_compiled_reopen_without_java() {
    run(
        NativeVoiceProfile::SimpleVoiceChat,
        "svc",
        &["mods/voicechat-fabric-1.21.1-2.6.22.jar"],
        voice_profile::SVC_CONFIGURATION_NAME,
        voice_profile::SVC_CONFIGURATION,
    );
}

#[test]
#[ignore = "explicit production Plasmo resources and two concrete file-only approvals; no Java"]
fn actual_pinned_plasmo_factory_eula_and_compiled_reopen_without_java() {
    run(
        NativeVoiceProfile::PlasmoVoice,
        "plasmo",
        &[
            "mods/plasmovoice-fabric-1.21.1-2.1.17.jar",
            "mods/fabric-api-0.116.10+1.21.1.jar",
        ],
        voice_profile::PLASMO_CONFIGURATION_NAME,
        voice_profile::PLASMO_CONFIGURATION,
    );
}
