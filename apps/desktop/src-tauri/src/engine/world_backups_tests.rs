use super::*;
use crate::engine::{acquire_instance_mutation_lock, InstanceId, InstanceLayout};
use std::io::{Seek, SeekFrom};

struct Fixture {
    root: PathBuf,
    instance: Instance,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("aurora-worlds-{}", Uuid::new_v4()));
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("fixture").unwrap());
        instance.ensure_layout().unwrap();
        Self { root, instance }
    }
    fn world(&self, name: &str) -> PathBuf {
        let dir = self.instance.root().join("saves").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("level.dat"), b"level").unwrap();
        dir
    }
    fn snapshot(&self) {
        arm(&self.instance).unwrap();
        let guard = acquire_instance_mutation_lock(&self.instance).unwrap();
        after_exit(&self.instance, &guard).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn world_backups_store_only_changed_chunks_and_restore_verified_copy() {
    let f = Fixture::new();
    let world = f.world("Mundo existente");
    fs::create_dir_all(world.join("region")).unwrap();
    fs::create_dir(world.join("vazia")).unwrap();
    fs::write(world.join("session.lock"), b"lock").unwrap();
    let path = world.join("region/r.0.0.mca");
    let mut original = Vec::new();
    for byte in 1..=8 {
        original.extend(vec![byte; CHUNK]);
    }
    fs::write(&path, &original).unwrap();
    SOURCE_BYTES.with(|n| n.set(0));
    let start = std::time::Instant::now();
    f.snapshot();
    let first = overview(&f.instance).unwrap();
    let first_time = start.elapsed();
    let first_read = SOURCE_BYTES.with(|n| n.replace(0));
    let old = first.versions[0].id.clone();
    assert_eq!(first.stored_bytes, (8 * CHUNK + 5) as u64);
    let original_mtime = fs::metadata(&path).unwrap().modified().unwrap();
    let mut file = OpenOptions::new().write(true).open(&path).unwrap();
    file.seek(SeekFrom::Start((CHUNK * 3) as u64)).unwrap();
    file.write_all(&vec![9; CHUNK]).unwrap();
    file.set_modified(original_mtime).unwrap();
    drop(file);
    let start = std::time::Instant::now();
    f.snapshot();
    let second_time = start.elapsed();
    let second_read = SOURCE_BYTES.with(|n| n.replace(0));
    let second = overview(&f.instance).unwrap();
    let logical: u64 = second.versions.iter().map(|v| v.bytes).sum();
    assert_eq!(second.stored_bytes - first.stored_bytes, CHUNK as u64);
    let db_path = f
        .instance
        .root()
        .join(".aurora/world-backups/history.sqlite3");
    let physical = fs::metadata(db_path).unwrap().len();
    assert!(physical < logical);
    let start = std::time::Instant::now();
    f.snapshot();
    let unchanged = overview(&f.instance).unwrap();
    eprintln!("synthetic MCA: first_payload={} second_added={} logical={} sqlite={} unchanged_added={} elapsed_ms={}/{}/{}",first.stored_bytes,second.stored_bytes-first.stored_bytes,logical,physical,unchanged.stored_bytes-second.stored_bytes,first_time.as_millis(),second_time.as_millis(),start.elapsed().as_millis());
    let unchanged_read = SOURCE_BYTES.with(|n| n.replace(0));
    eprintln!(
        "source bytes read from saves per capture: {first_read}/{second_read}/{unchanged_read}"
    );
    if file_stamp(&File::open(&path).unwrap()).is_some() {
        assert_eq!(first_read, (8 * CHUNK + 5) as u64);
        assert_eq!(second_read, (8 * CHUNK) as u64);
        assert_eq!(unchanged_read, 0);
    }
    assert_eq!(unchanged.versions.len(), 2);
    let guard = acquire_instance_mutation_lock(&f.instance).unwrap();
    let name = restore_copy(&f.instance, &guard, &old).unwrap();
    let restored = f.instance.root().join("saves").join(name);
    assert_eq!(
        fs::read(restored.join("region/r.0.0.mca")).unwrap(),
        original
    );
    assert!(restored.join("vazia").is_dir());
    assert!(!restored.join("session.lock").exists());
    assert_eq!(fs::read(path).unwrap()[3 * CHUNK], 9);
}

#[test]
fn world_backups_durable_marker_global_and_instance_policies() {
    let f = Fixture::new();
    let world = f.world("singleplayer");
    let guard = acquire_instance_mutation_lock(&f.instance).unwrap();
    after_exit(&f.instance, &guard).unwrap();
    assert!(overview(&f.instance).unwrap().versions.is_empty());
    arm(&f.instance).unwrap();
    drop(guard);
    let reopened = InstanceLayout::new(&f.root).open(f.instance.id().clone());
    assert!(any_pending(&f.root).unwrap());
    assert!(needs_resume(&reopened).unwrap());
    let guard = acquire_instance_mutation_lock(&reopened).unwrap();
    after_exit(&reopened, &guard).unwrap();
    drop(guard);
    assert!(!needs_resume(&reopened).unwrap());
    assert!(!any_pending(&f.root).unwrap());
    configure(&f.instance, true, Some(false)).unwrap();
    fs::write(world.join("level.dat"), b"next").unwrap();
    arm(&f.instance).unwrap();
    let disabled_guard = acquire_instance_mutation_lock(&f.instance).unwrap();
    after_exit_if_idle(&f.instance, &disabled_guard, || {
        panic!("disabled backup must skip the OS probe")
    })
    .unwrap();
    drop(disabled_guard);
    assert_eq!(overview(&f.instance).unwrap().versions.len(), 1);
    configure(&f.instance, false, Some(true)).unwrap();
    f.snapshot();
    assert_eq!(overview(&f.instance).unwrap().versions.len(), 2);
    configure(&f.instance, true, Some(true)).unwrap();
    configure(&f.instance, false, Some(false)).unwrap();
    fs::write(world.join("level.dat"), b"third").unwrap();
    f.snapshot();
    assert_eq!(overview(&f.instance).unwrap().versions.len(), 2);
    configure(&f.instance, false, None).unwrap();
    f.snapshot();
    assert_eq!(overview(&f.instance).unwrap().versions.len(), 3);
}

#[test]
fn world_backups_reject_busy_world_and_roll_back_all_partial_changes() {
    let f = Fixture::new();
    let first = f.world("a");
    let last = f.world("z");
    f.snapshot();
    fs::write(first.join("level.dat"), b"changed").unwrap();
    let guard = acquire_instance_mutation_lock(&f.instance).unwrap();
    let lock = File::create(last.join("session.lock")).unwrap();
    fs2::FileExt::try_lock_exclusive(&lock).unwrap();
    arm(&f.instance).unwrap();
    assert!(after_exit(&f.instance, &guard).is_err());
    let failed = overview(&f.instance).unwrap();
    assert_eq!(failed.versions.len(), 2);
    assert_eq!(failed.stored_bytes, 5);
    assert!(failed.pending && failed.last_error.is_some());
    drop(lock);
    fs::hard_link(last.join("level.dat"), last.join("alias")).unwrap();
    assert!(after_exit(&f.instance, &guard).is_err());
    assert_eq!(overview(&f.instance).unwrap().versions.len(), 2);
    fs::remove_file(last.join("alias")).unwrap();
    after_exit(&f.instance, &guard).unwrap();
    assert_eq!(overview(&f.instance).unwrap().versions.len(), 3);
}

#[test]
fn world_backups_remove_orphans_and_reject_corrupt_restores() {
    let f = Fixture::new();
    let world = f.world("a");
    for n in 0..8 {
        fs::write(world.join("level.dat"), [n + 1]).unwrap();
        f.snapshot();
    }
    let history = overview(&f.instance).unwrap();
    assert_eq!(history.versions.len(), KEEP);
    assert_eq!(history.stored_bytes, KEEP as u64);
    let db = archive(&f.instance).unwrap();
    let id = Uuid::new_v4().to_string();
    let orphan = f
        .instance
        .root()
        .join(".aurora/world-backup-staging")
        .join(&id);
    fs::create_dir_all(&orphan).unwrap();
    fs::write(orphan.join("interrupted"), b"partial").unwrap();
    db.connection
        .execute("INSERT INTO stages VALUES(?1)", [&id])
        .unwrap();
    db.connection
        .execute("UPDATE chunks SET data=zeroblob(length(data))", [])
        .unwrap();
    drop(db);
    let guard = acquire_instance_mutation_lock(&f.instance).unwrap();
    assert!(matches!(
        restore_copy(&f.instance, &guard, &history.versions[0].id),
        Err(BackupError::Integrity)
    ));
    assert!(restore_copy(&f.instance, &guard, "../other").is_err());
    assert!(!orphan.exists());
    assert_eq!(
        fs::read_dir(f.instance.root().join("saves"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(fs::read(world.join("level.dat")).unwrap(), [8]);
    assert_eq!(fs::read_dir(orphan.parent().unwrap()).unwrap().count(), 0);
}

#[test]
fn world_backups_retention_preserves_each_latest_and_rolls_back_if_limits_are_too_small() {
    let f = Fixture::new();
    let a = f.world("old");
    f.snapshot();
    let b = f.world("active");
    for n in 0..4 {
        fs::write(b.join("level.dat"), [n]).unwrap();
        f.snapshot();
    }
    let mut db = archive(&f.instance).unwrap();
    let tx = db.connection.transaction().unwrap();
    retain_history(&tx, 2, 1024).unwrap();
    tx.commit().unwrap();
    let latest = overview(&f.instance).unwrap();
    assert_eq!(latest.versions.len(), 2);
    assert!(latest.versions.iter().any(|v| v.world == "old"));
    assert!(latest.versions.iter().any(|v| v.world == "active"));
    for (max_count, max_bytes) in [(1, 1024), (2, 1)] {
        let tx = db.connection.transaction().unwrap();
        assert!(matches!(
            retain_history(&tx, max_count, max_bytes),
            Err(BackupError::Limit)
        ));
        drop(tx);
        assert_eq!(overview(&f.instance).unwrap().versions.len(), 2);
    }
    assert!(a.join("level.dat").is_file());
}

#[test]
fn world_backups_follow_instance_rename_and_explicit_deletion() {
    let f = Fixture::new();
    f.world("mundo");
    f.snapshot();
    configure(&f.instance, false, Some(false)).unwrap();
    arm(&f.instance).unwrap();
    let renamed = InstanceLayout::new(&f.root).open(InstanceId::parse("renamed").unwrap());
    fs::rename(f.instance.root(), renamed.root()).unwrap();
    let history = overview(&renamed).unwrap();
    assert_eq!(history.versions.len(), 1);
    assert!(history.pending);
    assert_eq!(history.instance_enabled, Some(false));
    assert!(remove_with_preferences(&renamed, || Err("simulated refusal".into())).is_err());
    assert!(needs_resume(&renamed).unwrap());
    let archive = renamed.root().join(".aurora/world-backups/history.sqlite3");
    assert!(archive.exists());
    remove_with_preferences(&renamed, || {
        fs::remove_dir_all(renamed.root()).map_err(|e| e.to_string())
    })
    .unwrap();
    assert!(!archive.exists());
    assert!(!any_pending(&f.root).unwrap());
}
