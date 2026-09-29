//! Damaged points remain reviewable/deletable without hiding healthy points.
//! All destructive writes use isolated temporary instances.
use aurora_launcher_core::engine::{
    acquire_instance_recovery_lock, recovery, Instance, InstanceId, InstanceLayout,
};
use std::fs;
use std::path::PathBuf;

struct Fixture {
    root: PathBuf,
    instance: Instance,
}

impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("aurora-recovery-listing-{}", uuid::Uuid::new_v4()));
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("listing-qa").unwrap());
        instance.ensure_layout().unwrap();
        fs::write(instance.root().join("options.txt"), b"gamma:0.5\n").unwrap();
        Self { root, instance }
    }

    fn corrupt(&self, point: &recovery::RecoveryPoint) {
        let file = point
            .files
            .iter()
            .find(|file| file.path == "options.txt")
            .unwrap();
        fs::write(
            self.root
                .join(".aurora/recovery/blobs/sha256")
                .join(&file.sha256[..2])
                .join(&file.sha256),
            b"corrupt synthetic object",
        )
        .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn corrupted_and_missing_points_do_not_hide_healthy_points_and_can_be_deleted() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    let healthy = recovery::create_point(&f.instance, &guard, None).unwrap();
    fs::write(f.instance.root().join("options.txt"), b"gamma:0.8\n").unwrap();
    let corrupt = recovery::create_point(&f.instance, &guard, None).unwrap();
    f.corrupt(&corrupt);
    fs::write(f.instance.root().join("options.txt"), b"gamma:0.9\n").unwrap();
    let missing = recovery::create_point(&f.instance, &guard, None).unwrap();
    fs::remove_file(
        f.root
            .join(".aurora/recovery/manifests")
            .join(format!("{}.json", missing.id)),
    )
    .unwrap();
    let listed = recovery::list_points(&f.instance, &guard).unwrap();
    assert_eq!(listed.points.len(), 1);
    assert_eq!(listed.points[0].id, healthy.id);
    assert_eq!(listed.unavailable_points.len(), 2);
    assert!(!listed.undo_unavailable);
    assert!(listed
        .unavailable_points
        .iter()
        .all(|point| !point.reason.contains(&f.root.to_string_lossy().to_string())));
    for point in listed.unavailable_points {
        recovery::delete_point(&f.instance, &guard, &point.id).unwrap();
    }
    let listed = recovery::list_points(&f.instance, &guard).unwrap();
    assert!(listed.unavailable_points.is_empty());
    assert_eq!(listed.points[0].id, healthy.id);
    let preview = recovery::preview_restore(&f.instance, &guard, &healthy.id).unwrap();
    recovery::restore(&f.instance, &guard, &preview.id).unwrap();
    assert_eq!(
        fs::read(f.instance.root().join("options.txt")).unwrap(),
        b"gamma:0.5\n"
    );
}

#[test]
fn corrupted_undo_is_unavailable_without_hiding_manual_points() {
    let f = Fixture::new();
    let guard = acquire_instance_recovery_lock(&f.instance).unwrap();
    let manual = recovery::create_point(&f.instance, &guard, None).unwrap();
    fs::write(f.instance.root().join("options.txt"), b"gamma:0.8\n").unwrap();
    let preview = recovery::preview_restore(&f.instance, &guard, &manual.id).unwrap();
    recovery::restore(&f.instance, &guard, &preview.id).unwrap();
    let undo = recovery::list_points(&f.instance, &guard)
        .unwrap()
        .undo_point
        .unwrap();
    f.corrupt(&undo);
    let listed = recovery::list_points(&f.instance, &guard).unwrap();
    assert!(listed.undo_unavailable);
    assert!(listed.undo_point.is_none());
    assert_eq!(listed.points.len(), 1);
    assert_eq!(listed.points[0].id, manual.id);
    assert!(recovery::preview_undo(&f.instance, &guard).is_err());
}
