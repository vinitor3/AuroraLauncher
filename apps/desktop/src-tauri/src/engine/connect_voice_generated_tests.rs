//! Opt-in evidence from exported bytes of the two stopped, disposable servers.
//! This does not run a JVM or simulate a mod rewriting an applied announcement.
//! The public process guard performs real Windows presence queries; run serially
//! only after all Java processes have stopped. No raw config or UUID is printed.
use super::*;
use crate::engine::{acquire_instance_mutation_lock, ContentUpdateError};

const GENERATED_SVC_SHA256: &str =
    "9ceff6b64b73ad52f4c5f843e01aa027b237cf8f44aebabe53d60ce831582fe4";
const GENERATED_PLASMO_SHA256: &str =
    "66dee32150e0dd3ea6fd999803291f5709ff52ebbc1c8d3a487dea8f443173cf";

// Avoid Debug formatting even on a failed assertion: configuration, filesystem
// paths and diagnostic-copy identifiers are not test output.
fn checked<T, E>(result: std::result::Result<T, E>, code: &'static str) -> T {
    match result {
        Ok(value) => value,
        Err(_) => panic!("{code}"),
    }
}

fn explicit_local_directory(variable: &str) -> PathBuf {
    let value = std::env::var_os(variable).expect("generated_config_explicit_directory_required");
    let path = PathBuf::from(value);
    let text = path.to_str().expect("generated_config_directory_encoding");
    // Require an ordinary drive-absolute Windows spelling before any I/O. In
    // particular, UNC, device paths, ADS and drive-relative paths are refused.
    assert!(
        cfg!(windows)
            && text.len() > 3
            && text.as_bytes()[0].is_ascii_alphabetic()
            && text.as_bytes()[1] == b':'
            && matches!(text.as_bytes()[2], b'/' | b'\\'),
        "generated_config_local_directory_required"
    );
    for component in text[3..].split(['/', '\\']) {
        checked(
            recovery::valid_relative(component),
            "generated_config_directory_component",
        );
    }
    assert!(path.is_absolute(), "generated_config_absolute_directory");
    path
}

fn config_bytes(path: &Path) -> Vec<u8> {
    checked(
        recovery::read_bounded(path, MAX_CONFIG),
        "generated_config_read_failed",
    )
}

fn assert_only_announcement_changed(voice_mod: VoiceMod, original: &[u8], applied: &[u8]) {
    match voice_mod {
        VoiceMod::SimpleVoiceChat => {
            // Independently construct the only allowed delta in the fixed
            // writer output: insertion into its one empty voice_host RHS.
            let marker = b"voice_host=";
            let offsets: Vec<_> = original
                .windows(marker.len())
                .enumerate()
                .filter(|(offset, bytes)| {
                    *bytes == marker && (*offset == 0 || original[*offset - 1] == b'\n')
                })
                .map(|(offset, _)| offset + marker.len())
                .collect();
            assert!(offsets.len() == 1, "generated_svc_announcement_count");
            let offset = offsets[0];
            assert!(
                original[offset..].starts_with(b"\n") || original[offset..].starts_with(b"\r\n"),
                "generated_svc_announcement_not_empty"
            );
            let expected = [
                &original[..offset],
                b"127.0.0.1:35554".as_slice(),
                &original[offset..],
            ]
            .concat();
            assert!(applied == expected, "generated_svc_unowned_bytes_changed");
        }
        VoiceMod::PlasmoVoice => {
            // The actual writer omitted host.public. Preserve its entire byte
            // prefix, including comments/server_id/EOL, and independently check
            // the resulting TOML tree differs only by the two owned fields.
            assert!(
                applied.starts_with(original),
                "generated_plasmo_original_prefix_changed"
            );
            let mut expected: toml::Value = checked(
                toml::from_str(checked(
                    std::str::from_utf8(original),
                    "generated_plasmo_utf8",
                )),
                "generated_plasmo_parse",
            );
            let host = expected
                .get_mut("host")
                .and_then(toml::Value::as_table_mut)
                .expect("generated_plasmo_host_missing");
            assert!(
                !host.contains_key("public"),
                "generated_plasmo_public_exists"
            );
            let mut public = toml::Table::new();
            public.insert("ip".into(), toml::Value::String("127.0.0.1".into()));
            public.insert("port".into(), toml::Value::Integer(35554));
            host.insert("public".into(), toml::Value::Table(public));
            let actual: toml::Value = checked(
                toml::from_str(checked(
                    std::str::from_utf8(applied),
                    "generated_plasmo_applied_utf8",
                )),
                "generated_plasmo_applied_parse",
            );
            assert!(
                actual == expected,
                "generated_plasmo_unowned_values_changed"
            );
        }
    }
}

fn exercise_generated_roundtrip(
    voice_mod: VoiceMod,
    config_name: &str,
    config_sha256: &str,
    config_size: usize,
    jar_name: &str,
) {
    let directory = explicit_local_directory("AURORA_CONNECT_GENERATED_CONFIG_DIR");
    let _config_pins = checked(
        recovery::pin_directory(&directory),
        "generated_config_directory_unsafe",
    );
    let exported = checked(
        recovery::path(&directory, config_name),
        "generated_config_path_unsafe",
    );
    let original = config_bytes(&exported);
    assert!(
        original.len() == config_size,
        "generated_config_size_changed"
    );
    assert!(
        digest(&original) == config_sha256,
        "generated_config_hash_changed"
    );

    let artifacts = explicit_local_directory("AURORA_CONNECT_ARTIFACT_DIR");
    let _artifact_pins = checked(
        recovery::pin_directory(&artifacts),
        "generated_config_artifact_directory_unsafe",
    );
    let jar = checked(
        recovery::path(&artifacts, jar_name),
        "generated_config_artifact_path_unsafe",
    );
    let (jar_hash, jar_size) = voice_mod.artifact();
    let jar_bytes = checked(
        recovery::read_bounded(&jar, jar_size),
        "generated_config_artifact_read_failed",
    );
    assert!(
        jar_bytes.len() as u64 == jar_size && digest(&jar_bytes) == jar_hash,
        "generated_config_artifact_identity"
    );
    drop(jar_bytes);

    // This existing fixture creates a new origin plus a verified diagnostic
    // copy. The exported server file is only read, never passed as a mutation
    // target. Production preparation rechecks the pinned JAR in the new copy.
    let f = Fixture::for_mod(Some(&jar), voice_mod, &original);
    let source_config = checked(
        recovery::path(f.source.root(), voice_mod.config()),
        "generated_config_source_path",
    );
    let copy_config = checked(
        recovery::path(f.copy.root(), voice_mod.config()),
        "generated_config_copy_path",
    );
    let source_options = fs::read(f.source.root().join("options.txt")).ok();
    assert!(
        config_bytes(&copy_config) == original,
        "generated_config_copy_bytes"
    );
    let guard = checked(
        acquire_voice_mutation_guard(&f.copy),
        "generated_config_production_guard_refused",
    );
    let before = checked(
        recovery::capture_observation(&f.copy, guard.inner()),
        "generated_config_before_observation",
    );
    let plan = checked(
        prepare_announcement(&f.copy, &guard, f.request()),
        "generated_config_prepare_refused",
    );
    assert!(matches!(
        plan.review().action,
        AnnouncementAction::SetAnnouncement
    ));
    assert!(
        plan.review().source_sha256 == config_sha256,
        "generated_config_review_source"
    );
    assert!(
        config_bytes(&copy_config) == original,
        "generated_config_prepare_mutated"
    );
    let approval = plan.review().digest.clone();
    let mut applied = checked(
        plan.confirm(&f.copy, &guard, &approval),
        "generated_config_confirm_failed",
    );
    let announced = config_bytes(&copy_config);
    assert_only_announcement_changed(voice_mod, &original, &announced);
    assert!(
        applied.recovery.undo_available,
        "generated_config_undo_missing"
    );
    assert!(applied.receipt.state() == RestorationState::Ready);
    let after = checked(
        recovery::capture_observation(&f.copy, guard.inner()),
        "generated_config_after_observation",
    );
    let mut expected_files = before.files;
    let changed = expected_files
        .iter_mut()
        .find(|file| file.path == voice_mod.config())
        .expect("generated_config_inventory_missing");
    changed.sha256 = digest(&announced);
    changed.size = announced.len() as u64;
    assert!(
        after.files == expected_files,
        "generated_config_unowned_file_changed"
    );
    assert!(
        config_bytes(&source_config) == original,
        "generated_config_origin_changed"
    );
    assert!(matches!(
        acquire_instance_mutation_lock(&f.copy),
        Err(ContentUpdateError::Busy)
    ));
    drop(guard);

    // Actual launcher lock handoff, without launching any child/JVM. The receipt
    // stays in RAM. These are explicit independent fixture edits, not mod output.
    let launch_lock = checked(
        acquire_instance_mutation_lock(&f.copy),
        "generated_config_launch_lock",
    );
    let options = b"renderDistance:19\r\ngamma:0.6\r\n";
    let unrelated = br#"{"quality":7}"#;
    checked(
        fs::write(f.copy.root().join("options.txt"), options),
        "generated_config_options_write",
    );
    checked(
        fs::write(f.copy.root().join("config/unrelated.json"), unrelated),
        "generated_config_other_write",
    );
    assert!(matches!(
        acquire_voice_mutation_guard(&f.copy),
        Err(RecoveryError::WrongLock)
    ));
    assert!(
        config_bytes(&copy_config) == announced,
        "generated_config_lock_refusal_mutated"
    );
    drop(launch_lock);

    let restore_guard = checked(
        acquire_voice_mutation_guard(&f.copy),
        "generated_config_restore_guard_refused",
    );
    let restore = checked(
        applied.receipt.prepare_restore(&f.copy, &restore_guard),
        "generated_config_restore_prepare_failed",
    );
    assert!(matches!(
        restore.review().action,
        AnnouncementAction::RestoreOriginal { .. }
    ));
    assert!(
        restore.review().replacement_sha256 == config_sha256,
        "generated_config_restore_target"
    );
    let restore_approval = restore.review().digest.clone();
    assert!(
        restore_approval != approval,
        "generated_config_restore_approval_reused"
    );
    checked(
        restore.confirm(&f.copy, &restore_guard, &restore_approval),
        "generated_config_restore_failed",
    );
    assert!(applied.receipt.state() == RestorationState::Restored);
    assert!(
        config_bytes(&copy_config) == original,
        "generated_config_restore_bytes"
    );
    assert!(
        config_bytes(&source_config) == original,
        "generated_config_origin_changed"
    );
    assert!(
        config_bytes(&exported) == original,
        "generated_config_export_changed"
    );
    assert!(
        digest(&config_bytes(&exported)) == config_sha256,
        "generated_config_export_hash_changed"
    );
    assert!(
        checked(
            fs::read(f.copy.root().join("options.txt")),
            "generated_config_options_read"
        ) == options,
        "generated_config_options_lost"
    );
    assert!(
        checked(
            fs::read(f.copy.root().join("config/unrelated.json")),
            "generated_config_other_read"
        ) == unrelated,
        "generated_config_other_lost"
    );
    assert!(
        fs::read(f.source.root().join("options.txt")).ok() == source_options,
        "generated_config_origin_options_changed"
    );
    assert!(!checked(
        recovery::has_pending(&f.copy),
        "generated_config_pending_read"
    ));
    assert!(
        checked(
            fs::read_dir(f.copy.root().join(".aurora/connect-voice-stage")),
            "generated_config_stage_read"
        )
        .count()
            == 0,
        "generated_config_stage_not_empty"
    );
    assert!(matches!(
        applied.receipt.prepare_restore(&f.copy, &restore_guard),
        Err(RecoveryError::StalePreview)
    ));
    drop(restore_guard);
    drop(checked(
        acquire_instance_mutation_lock(&f.copy),
        "generated_config_final_launch_lock",
    ));
}

#[test]
#[ignore = "requires explicit generated-config/artifact directories and all Java stopped; uses real Windows process guard, never executes Java"]
fn generated_svc_config_roundtrips_with_production_process_guard() {
    exercise_generated_roundtrip(
        VoiceMod::SimpleVoiceChat,
        "svc.properties",
        GENERATED_SVC_SHA256,
        3081,
        "voicechat-fabric-1.21.1-2.6.22.jar",
    );
}

#[test]
#[ignore = "requires explicit generated-config/artifact directories and all Java stopped; uses real Windows process guard, never executes Java"]
fn generated_plasmo_config_roundtrips_with_production_process_guard() {
    exercise_generated_roundtrip(
        VoiceMod::PlasmoVoice,
        "plasmo.toml",
        GENERATED_PLASMO_SHA256,
        3807,
        "plasmovoice-fabric-1.21.1-2.1.17.jar",
    );
}
