use super::*;
use crate::engine::{acquire_instance_mutation_lock, InstanceId, InstanceLayout};
use std::io::Write;

struct Fixture {
    root: std::path::PathBuf,
    instance: Instance,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("aurora-doctor-recovery-{}", uuid::Uuid::new_v4()));
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("qa").unwrap());
        instance.ensure_layout().unwrap();
        fs::create_dir(instance.root().join("config")).unwrap();
        fs::write(instance.root().join("config/video.toml"), b"distance=12").unwrap();
        fs::write(
            instance.root().join("aurora-modpack.json"),
            br#"{"name":"First"}"#,
        )
        .unwrap();
        Self { root, instance }
    }
    fn observation(&self, guard: &InstanceMutationGuard) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        record_observation(&self.instance, guard, &id, "preparing").unwrap();
        record_observation(&self.instance, guard, &id, "launch").unwrap();
        id
    }
    fn mod_file(&self, name: &str) {
        let file = File::create(self.instance.mods_dir().join(name)).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(br#"{"id":"fixture","version":"1"}"#).unwrap();
        zip.finish().unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("aurora-doctor-recovery-"));
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn changes_and_relevance_use_recorded_hashes_not_current_files_or_timestamps() {
    let fixture = Fixture::new();
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    let first = fixture.observation(&guard);
    record_outcome(&fixture.instance, &first, "success").unwrap();
    let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();
    fixture.mod_file("example.jar");
    fs::write(
        fixture.instance.root().join("config/video.toml"),
        b"distance=24",
    )
    .unwrap();
    fs::write(
        fixture.instance.root().join("aurora-modpack.json"),
        br#"{"name":"Second"}"#,
    )
    .unwrap();
    let second = fixture.observation(&guard);
    record_outcome(&fixture.instance, &second, "failed").unwrap();
    fs::write(
        fixture.instance.root().join("config/video.toml"),
        b"distance=48",
    )
    .unwrap();
    let result = overview(&fixture.instance, &guard, Some(&second)).unwrap();
    assert_eq!(result.status, "available");
    assert_eq!(result.baseline_run_id.as_deref(), Some(first.as_str()));
    assert!(result.current_state_changed);
    assert_eq!(result.changes.len(), 3);
    for category in ["mods", "config", "profile"] {
        assert!(result
            .changes
            .iter()
            .any(|change| change.category == category));
    }
    assert_eq!(result.points[0].point.id, point.id);
    assert!(result.points[0]
        .reasons
        .iter()
        .any(|reason| reason.contains("coincide com o estado observado antes")));
    assert!(result
        .limitations
        .iter()
        .any(|reason| reason.contains("não comprova")));
    assert_eq!(result.attempts.len(), 2);
    assert_eq!(result.attempts[0].outcome, "failed");
    assert!(result.attempts[0].ended_at_ms.is_some());
    assert!(result.attempts[0].has_launch_snapshot);
    let earlier = overview(&fixture.instance, &guard, Some(&first)).unwrap();
    assert!(earlier.changes.is_empty());
}

#[test]
fn history_survives_rename_but_not_deletion_recreation_or_copied_identity() {
    let fixture = Fixture::new();
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    let run = fixture.observation(&guard);
    let identity = recovery::identity_lookup(&fixture.instance)
        .unwrap()
        .unwrap();
    drop(guard);
    let renamed = InstanceLayout::new(&fixture.root).open(InstanceId::parse("renamed").unwrap());
    fs::rename(fixture.instance.root(), renamed.root()).unwrap();
    let guard = acquire_instance_mutation_lock(&renamed).unwrap();
    let result = overview(&renamed, &guard, Some(&run)).unwrap();
    assert_eq!(result.instance_uuid.as_deref(), Some(identity.as_str()));
    assert_eq!(result.attempts[0].run_id, run);
    drop(guard);
    fixture.instance.ensure_layout().unwrap();
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    let result = overview(&fixture.instance, &guard, Some(&run)).unwrap();
    assert_eq!(result.status, "insufficient");
    assert!(result.attempts.is_empty());
    assert_ne!(result.instance_uuid.as_deref(), Some(identity.as_str()));
    fs::copy(
        renamed.root().join(".aurora/recovery-identity.json"),
        fixture
            .instance
            .root()
            .join(".aurora/recovery-identity.json"),
    )
    .unwrap();
    assert!(overview(&fixture.instance, &guard, Some(&run)).is_err());
}

#[test]
fn missing_launch_observation_cannot_fall_back_to_preparation() {
    let fixture = Fixture::new();
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    let run = uuid::Uuid::new_v4().to_string();
    record_observation(&fixture.instance, &guard, &run, "preparing").unwrap();
    let identity = recovery::identity_lookup(&fixture.instance)
        .unwrap()
        .unwrap();
    let audit = Audit::open(&fixture.instance).unwrap();
    audit
        .db
        .execute(
            "UPDATE attempts SET launch_attempted=1 WHERE run_id=?1",
            [&run],
        )
        .unwrap();
    drop(audit);
    record_outcome(&fixture.instance, &run, "failed").unwrap();
    let result = overview(&fixture.instance, &guard, Some(&run)).unwrap();
    assert_eq!(result.status, "insufficient");
    assert!(result.changes.is_empty());
    assert_eq!(result.instance_uuid.as_deref(), Some(identity.as_str()));
    assert!(result
        .limitations
        .iter()
        .any(|value| value.contains("Não foi possível associar")));
}

#[test]
fn immutable_observation_duplicate_run_and_terminal_outcome_are_protected() {
    let fixture = Fixture::new();
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    let run = fixture.observation(&guard);
    record_observation(&fixture.instance, &guard, &run, "launch").unwrap();
    fs::write(
        fixture.instance.root().join("config/video.toml"),
        b"distance=24",
    )
    .unwrap();
    assert!(record_observation(&fixture.instance, &guard, &run, "launch").is_err());
    record_outcome(&fixture.instance, &run, "failed").unwrap();
    record_outcome(&fixture.instance, &run, "running").unwrap();
    assert_eq!(
        overview(&fixture.instance, &guard, Some(&run))
            .unwrap()
            .attempts[0]
            .outcome,
        "failed"
    );
    let other = Fixture::new();
    let other_guard = acquire_instance_mutation_lock(&other.instance).unwrap();
    assert!(record_outcome(&other.instance, &run, "success").is_err());
    assert!(record_observation(&other.instance, &guard, &run, "preparing").is_err());
    assert!(overview(&other.instance, &other_guard, Some("../../file")).is_err());
}

#[test]
fn unavailable_points_and_corrupt_history_are_explicit_without_repairing_data() {
    let fixture = Fixture::new();
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    let run = fixture.observation(&guard);
    let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();
    let file = &point.files[0];
    let blob = fixture.root.join(format!(
        ".aurora/recovery/blobs/sha256/{}/{}",
        &file.sha256[..2],
        file.sha256
    ));
    fs::write(blob, b"corrupt").unwrap();
    let result = overview(&fixture.instance, &guard, Some(&run)).unwrap();
    assert!(result.points.is_empty());
    assert_eq!(result.unavailable_points.len(), 1);
    let database = storage_path(&fixture.instance)
        .unwrap()
        .join("history.sqlite3");
    fs::write(&database, b"corrupt history").unwrap();
    let result = overview(&fixture.instance, &guard, Some(&run)).unwrap();
    assert_eq!(result.status, "unavailable");
    assert_eq!(fs::read(database).unwrap(), b"corrupt history");
}

#[test]
fn recovery_points_do_not_associate_by_same_run_id_in_another_instance() {
    let fixture = Fixture::new();
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    let run = fixture.observation(&guard);
    recovery::create_point(&fixture.instance, &guard, None).unwrap();
    let other = InstanceLayout::new(&fixture.root).open(InstanceId::parse("other").unwrap());
    other.ensure_layout().unwrap();
    let other_guard = acquire_instance_mutation_lock(&other).unwrap();
    assert!(record_observation(&other, &other_guard, &run, "preparing").is_err());
    let result = overview(&other, &other_guard, Some(&run)).unwrap();
    assert!(result.attempts.is_empty());
    assert!(result.points.is_empty());
}

#[test]
fn lost_observer_is_unknown_and_missing_prior_capture_does_not_claim_prior_baseline() {
    let fixture = Fixture::new();
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    let first = uuid::Uuid::new_v4().to_string();
    record_observation(&fixture.instance, &guard, &first, "preparing").unwrap();
    let audit = Audit::open(&fixture.instance).unwrap();
    audit.db.execute("UPDATE attempts SET launch_attempted=1,outcome='running',observer_owner='previous-launcher' WHERE run_id=?1", [&first]).unwrap();
    drop(audit);
    let second = fixture.observation(&guard);
    let result = overview(&fixture.instance, &guard, Some(&second)).unwrap();
    assert_eq!(result.baseline_run_id.as_deref(), Some(second.as_str()));
    assert_eq!(result.attempts[1].outcome, "unknown");
    assert!(result
        .limitations
        .iter()
        .any(|value| value.contains("apenas a preparação")));
}

#[test]
fn first_lab_attempt_uses_creation_baseline_and_keeps_tested_changes_after_later_edits() {
    let fixture = Fixture::new();
    let source_guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    let environment =
        crate::engine::diagnostic_lab::create(&fixture.instance, &source_guard, None, |_| {})
            .unwrap();
    let test = InstanceLayout::new(&fixture.root).open(InstanceId::parse(environment.id).unwrap());
    let guard = acquire_instance_mutation_lock(&test).unwrap();
    fs::write(test.root().join("config/video.toml"), b"distance=24").unwrap();
    let run = uuid::Uuid::new_v4().to_string();
    record_observation(&test, &guard, &run, "preparing").unwrap();
    record_observation(&test, &guard, &run, "launch").unwrap();
    record_outcome(&test, &run, "success").unwrap();
    fs::write(test.root().join("options.txt"), b"gamma:2").unwrap();
    let result = overview(&test, &guard, Some(&run)).unwrap();
    assert_eq!(result.status, "available");
    assert_eq!(result.baseline_run_id, None);
    assert_eq!(result.changes.len(), 1);
    assert_eq!(result.changes[0].path, "config/video.toml");
    assert!(result.current_state_changed);
    assert!(result
        .limitations
        .iter()
        .any(|value| value.contains("proveniência da cópia foi verificada")));
    assert_eq!(result.attempts[0].outcome, "success");
}
