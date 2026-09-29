//! Independent end-to-end service QA with disposable files, never a JVM or
//! personal instance. UI/native launch still requires separate runtime testing.
use aurora_launcher_core::engine::{
    acquire_instance_mutation_lock, diagnostic_lab as lab, doctor_recovery as history, recovery,
    Instance, InstanceId, InstanceLayout, InstanceMutationGuard,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

struct Fixture {
    root: PathBuf,
    source: Instance,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "aurora-doctor-lab-independent-{}",
            uuid::Uuid::new_v4()
        ));
        let source = InstanceLayout::new(&root).open(InstanceId::parse("source").unwrap());
        source.ensure_layout().unwrap();
        fs::create_dir_all(source.root().join("config")).unwrap();
        fs::create_dir_all(source.root().join("saves/private")).unwrap();
        fs::write(
            source.root().join("saves/private/level.dat"),
            b"original world",
        )
        .unwrap();
        Self { root, source }
    }
    fn config(&self, value: Value) {
        fs::write(
            self.source.root().join("config/graphics.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
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
            .starts_with("aurora-doctor-lab-independent-"));
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn observe(instance: &Instance, guard: &InstanceMutationGuard, outcome: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    history::record_observation(instance, guard, &id, "preparing").unwrap();
    history::record_observation(instance, guard, &id, "launch").unwrap();
    history::record_outcome(instance, &id, outcome).unwrap();
    id
}

#[test]
fn failure_evidence_copy_experiment_selected_apply_and_undo_share_verified_identity() {
    let fixture = Fixture::new();
    fixture.config(json!({"renderDistance":12,"maxFps":120,"unsupported":7}));
    let guard = acquire_instance_mutation_lock(&fixture.source).unwrap();
    let healthy = observe(&fixture.source, &guard, "success");
    let point =
        recovery::create_point(&fixture.source, &guard, Some("Before experiment".into())).unwrap();
    fixture.config(json!({"renderDistance":24,"maxFps":120,"unsupported":7}));
    let failed = observe(&fixture.source, &guard, "failed");
    let evidence = history::overview(&fixture.source, &guard, Some(&failed)).unwrap();
    assert_eq!(evidence.baseline_run_id.as_deref(), Some(healthy.as_str()));
    assert!(evidence
        .changes
        .iter()
        .any(|c| c.path == "config/graphics.json"));
    assert!(evidence.points.iter().any(|p| p.point.id == point.id));
    assert!(evidence
        .limitations
        .iter()
        .any(|s| s.contains("não comprova")));

    let copy = lab::create(&fixture.source, &guard, None, |_| {}).unwrap();
    let test = InstanceLayout::new(&fixture.root).open(InstanceId::parse(copy.id).unwrap());
    let test_guard = acquire_instance_mutation_lock(&test).unwrap();
    assert!(!test.root().join("saves/private/level.dat").exists());
    assert_ne!(
        recovery::identity_lookup(&test).unwrap(),
        recovery::identity_lookup(&fixture.source).unwrap()
    );
    lab::set_setting(
        &test,
        &test_guard,
        "config/graphics.json",
        "/renderDistance",
        json!(8),
    )
    .unwrap();
    lab::set_setting(
        &test,
        &test_guard,
        "config/graphics.json",
        "/maxFps",
        json!(60),
    )
    .unwrap();
    let test_run = observe(&test, &test_guard, "success");
    let original_history = history::overview(&fixture.source, &guard, None).unwrap();
    assert!(!original_history
        .attempts
        .iter()
        .any(|run| run.run_id == test_run));
    assert_eq!(original_history.run_id.as_deref(), Some(failed.as_str()));
    let test_history = history::overview(&test, &test_guard, Some(&test_run)).unwrap();
    assert!(test_history
        .limitations
        .iter()
        .any(|s| s.contains("não uma garantia")));

    let preview = lab::preview_apply(
        &fixture.source,
        &guard,
        &test,
        &test_guard,
        &["setting:config/graphics.json#/renderDistance".into()],
    )
    .unwrap();
    assert_eq!(preview.changes.len(), 1);
    lab::apply(&fixture.source, &guard, &test, &test_guard, &preview.id).unwrap();
    let current: Value = serde_json::from_slice(
        &fs::read(fixture.source.root().join("config/graphics.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        current,
        json!({"renderDistance":8,"maxFps":120,"unsupported":7})
    );
    assert_eq!(
        fs::read(fixture.source.root().join("saves/private/level.dat")).unwrap(),
        b"original world"
    );
    let undo = recovery::preview_undo(&fixture.source, &guard).unwrap();
    recovery::restore(&fixture.source, &guard, &undo.id).unwrap();
    let restored: Value = serde_json::from_slice(
        &fs::read(fixture.source.root().join("config/graphics.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        restored,
        json!({"renderDistance":24,"maxFps":120,"unsupported":7})
    );
    lab::delete(&test, &test_guard).unwrap();
    assert!(lab::list(&fixture.root).unwrap().is_empty());
    assert!(fixture
        .source
        .root()
        .join("saves/private/level.dat")
        .is_file());
}

#[test]
fn test_edits_cannot_borrow_original_lock_or_observed_run() {
    let fixture = Fixture::new();
    fixture.config(json!({"renderDistance":12}));
    let source_guard = acquire_instance_mutation_lock(&fixture.source).unwrap();
    let source_run = observe(&fixture.source, &source_guard, "failed");
    let copy = lab::create(&fixture.source, &source_guard, None, |_| {}).unwrap();
    let test = InstanceLayout::new(&fixture.root).open(InstanceId::parse(copy.id).unwrap());
    assert!(lab::set_setting(
        &test,
        &source_guard,
        "config/graphics.json",
        "/renderDistance",
        json!(8)
    )
    .is_err());
    let test_guard = acquire_instance_mutation_lock(&test).unwrap();
    assert!(history::record_observation(&test, &test_guard, &source_run, "launch").is_err());
    let unrelated = history::overview(&test, &test_guard, Some(&source_run)).unwrap();
    assert_eq!(unrelated.status, "insufficient");
    assert!(unrelated.changes.is_empty());
    assert!(history::record_outcome(&test, &source_run, "success").is_err());
    let original = history::overview(&fixture.source, &source_guard, Some(&source_run)).unwrap();
    assert_eq!(original.attempts[0].outcome, "failed");
}
