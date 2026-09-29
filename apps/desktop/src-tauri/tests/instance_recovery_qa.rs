//! Independent recovery QA. Every mutation targets a fresh temporary instance;
//! no game, fixture JAR, account, model or personal instance is used.
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use aurora_launcher_core::engine::{
    acquire_instance_mutation_lock, acquire_instance_recovery_lock, recovery, Instance, InstanceId,
    InstanceLayout,
};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

struct Fixture {
    root: PathBuf,
    instance: Instance,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "aurora-instance-recovery-independent-{}",
            uuid::Uuid::new_v4()
        ));
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("qa").unwrap());
        instance.ensure_layout().unwrap();
        fs::create_dir(instance.root().join("config")).unwrap();
        fs::write(
            instance.root().join("config/video.toml"),
            b"distance = 12\n",
        )
        .unwrap();
        fs::write(instance.root().join("options.txt"), b"gamma:0.5\n").unwrap();
        fs::write(
            instance.root().join("aurora-instance.json"),
            br#"{"source":"fabric","minecraft":"1.20.1","versionId":"fabric-loader-0.16.14-1.20.1"}"#,
        )
        .unwrap();
        Self { root, instance }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Junction-specific tests remove their link before reaching this path.
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn inventory(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else {
                result.insert(
                    entry.path().strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    result
}

fn synthetic_jar(managed: bool) -> Vec<u8> {
    synthetic_jar_version(managed, "1")
}

fn synthetic_jar_version(managed: bool, version: &str) -> Vec<u8> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    archive
        .start_file("fabric.mod.json", SimpleFileOptions::default())
        .unwrap();
    archive
        .write_all(
            &serde_json::to_vec(&serde_json::json!({
                "id": "ordinary", "name": "Synthetic QA", "version": version,
            }))
            .unwrap(),
        )
        .unwrap();
    if managed {
        archive
            .start_file(
                "com/aurora/core/Synthetic.class",
                SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(b"never executed").unwrap();
    }
    archive.finish().unwrap().into_inner()
}

#[test]
fn another_instances_guard_cannot_authorize_capture() {
    let fixture = Fixture::new();
    let other = InstanceLayout::new(&fixture.root).open(InstanceId::parse("other").unwrap());
    other.ensure_layout().unwrap();
    let foreign_guard = acquire_instance_recovery_lock(&other).unwrap();
    let before = inventory(fixture.instance.root());
    assert!(recovery::create_point(&fixture.instance, &foreign_guard, None).is_err());
    assert_eq!(before, inventory(fixture.instance.root()));
}

#[test]
fn recovery_and_existing_mutations_share_the_same_exclusion_lock() {
    let fixture = Fixture::new();
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    assert!(acquire_instance_recovery_lock(&fixture.instance).is_err());
    drop(guard);
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    assert!(acquire_instance_mutation_lock(&fixture.instance).is_err());
    drop(guard);
    assert!(acquire_instance_mutation_lock(&fixture.instance).is_ok());
}

#[test]
fn protected_bytes_round_trip_and_undo_without_touching_excluded_data() {
    let fixture = Fixture::new();
    for (relative, contents) in [
        ("saves/qa-world/level.dat", b"world before".as_slice()),
        ("screenshots/image.png", b"screenshot".as_slice()),
        ("logs/latest.log", b"log".as_slice()),
        ("resourcepacks/pack.zip", b"resource pack".as_slice()),
        ("natives/library.dll", b"native".as_slice()),
    ] {
        let path = fixture.instance.root().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    fs::write(
        fixture.instance.root().join("config/removed.toml"),
        b"saved",
    )
    .unwrap();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let point =
        recovery::create_point(&fixture.instance, &guard, Some("QA sintética".into())).unwrap();
    let saved_config = inventory(&fixture.instance.root().join("config"));

    fs::write(
        fixture.instance.root().join("config/video.toml"),
        b"distance = 24\n",
    )
    .unwrap();
    fs::remove_file(fixture.instance.root().join("config/removed.toml")).unwrap();
    fs::create_dir(fixture.instance.root().join("config/new-directory")).unwrap();
    fs::write(
        fixture
            .instance
            .root()
            .join("config/new-directory/new.toml"),
        b"new",
    )
    .unwrap();
    fs::write(fixture.instance.root().join("options.txt"), b"gamma:1.0\n").unwrap();
    fs::write(
        fixture.instance.root().join("saves/qa-world/level.dat"),
        b"world after",
    )
    .unwrap();
    let changed_config = inventory(&fixture.instance.root().join("config"));
    let excluded = ["saves", "screenshots", "logs", "resourcepacks", "natives"]
        .map(|name| (name, inventory(&fixture.instance.root().join(name))));

    let preview = recovery::preview_restore(&fixture.instance, &guard, &point.id).unwrap();
    for (path, action) in [
        ("config/removed.toml", "add"),
        ("config/video.toml", "replace"),
        ("config/new-directory/new.toml", "remove"),
    ] {
        assert!(preview
            .changes
            .iter()
            .any(|change| change.path == path && change.action == action));
    }
    recovery::restore(&fixture.instance, &guard, &preview.id).unwrap();
    assert_eq!(
        saved_config,
        inventory(&fixture.instance.root().join("config"))
    );
    assert_eq!(
        fs::read(fixture.instance.root().join("options.txt")).unwrap(),
        b"gamma:0.5\n"
    );
    for (name, bytes) in &excluded {
        assert_eq!(*bytes, inventory(&fixture.instance.root().join(name)));
    }
    let undo = recovery::preview_undo(&fixture.instance, &guard).unwrap();
    recovery::restore(&fixture.instance, &guard, &undo.id).unwrap();
    assert_eq!(
        changed_config,
        inventory(&fixture.instance.root().join("config"))
    );
    assert_eq!(
        fs::read(fixture.instance.root().join("options.txt")).unwrap(),
        b"gamma:1.0\n"
    );
    for (name, bytes) in &excluded {
        assert_eq!(*bytes, inventory(&fixture.instance.root().join(name)));
    }
}

#[test]
fn instance_rename_retains_points_but_reusing_its_old_name_does_not() {
    let fixture = Fixture::new();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();
    drop(guard);
    let renamed = InstanceLayout::new(&fixture.root).open(InstanceId::parse("renamed").unwrap());
    fs::rename(fixture.instance.root(), renamed.root()).unwrap();
    let guard = acquire_instance_recovery_lock(&renamed).unwrap();
    assert!(recovery::list_points(&renamed, &guard)
        .unwrap()
        .points
        .iter()
        .any(|item| item.id == point.id));
    fs::write(renamed.root().join("config/video.toml"), b"changed").unwrap();
    let preview = recovery::preview_restore(&renamed, &guard, &point.id).unwrap();
    recovery::restore(&renamed, &guard, &preview.id).unwrap();
    assert_eq!(
        fs::read(renamed.root().join("config/video.toml")).unwrap(),
        b"distance = 12\n"
    );
    drop(guard);

    fixture.instance.ensure_layout().unwrap();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    assert!(recovery::list_points(&fixture.instance, &guard)
        .unwrap()
        .points
        .is_empty());
    assert!(recovery::preview_restore(&fixture.instance, &guard, &point.id).is_err());
}

#[test]
fn duplicate_points_share_content_and_deleting_one_keeps_the_other_usable() {
    let fixture = Fixture::new();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let first = recovery::create_point(&fixture.instance, &guard, Some("first".into())).unwrap();
    let first_physical = recovery::list_points(&fixture.instance, &guard)
        .unwrap()
        .physical_bytes;
    let second = recovery::create_point(&fixture.instance, &guard, Some("second".into())).unwrap();
    assert_eq!(
        first_physical,
        recovery::list_points(&fixture.instance, &guard)
            .unwrap()
            .physical_bytes
    );
    recovery::delete_point(&fixture.instance, &guard, &first.id).unwrap();
    assert!(recovery::point_details(&fixture.instance, &guard, &first.id).is_err());
    fs::write(
        fixture.instance.root().join("config/video.toml"),
        b"changed",
    )
    .unwrap();
    let preview = recovery::preview_restore(&fixture.instance, &guard, &second.id).unwrap();
    recovery::restore(&fixture.instance, &guard, &preview.id).unwrap();
    assert_eq!(
        fs::read(fixture.instance.root().join("config/video.toml")).unwrap(),
        b"distance = 12\n"
    );
}

#[test]
fn renamed_managed_artifacts_are_excluded_and_never_overwritten_by_an_older_point() {
    let fixture = Fixture::new();
    let ordinary = synthetic_jar(false);
    let managed = synthetic_jar(true);
    fs::write(fixture.instance.root().join("mods/ordinary.jar"), &ordinary).unwrap();
    fs::write(
        fixture
            .instance
            .root()
            .join("mods/unrelated-name.jar.disabled"),
        &managed,
    )
    .unwrap();
    fs::write(
        fixture
            .instance
            .root()
            .join("mods/aurora-companion-old.jar"),
        b"retired artifact",
    )
    .unwrap();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();
    assert!(point
        .files
        .iter()
        .any(|file| file.path == "mods/ordinary.jar"));
    assert!(!point
        .files
        .iter()
        .any(|file| file.path == "mods/unrelated-name.jar.disabled"));
    assert!(!point
        .files
        .iter()
        .any(|file| file.path == "mods/aurora-companion-old.jar"));
    fs::write(fixture.instance.root().join("mods/ordinary.jar"), &managed).unwrap();
    let before = inventory(&fixture.instance.root().join("mods"));
    // A conflicting newly managed target may fail closed or be explicitly
    // excluded from the effective restore. It must never become an old mod.
    if let Ok(preview) = recovery::preview_restore(&fixture.instance, &guard, &point.id) {
        recovery::restore(&fixture.instance, &guard, &preview.id).unwrap();
    }
    assert_eq!(before, inventory(&fixture.instance.root().join("mods")));
}

#[test]
fn modifying_a_saved_point_source_after_preview_invalidates_confirmation() {
    let fixture = Fixture::new();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();
    fs::write(
        fixture.instance.root().join("config/video.toml"),
        b"first change",
    )
    .unwrap();
    let preview = recovery::preview_restore(&fixture.instance, &guard, &point.id).unwrap();
    fs::write(
        fixture.instance.root().join("config/video.toml"),
        b"second change",
    )
    .unwrap();
    assert!(recovery::restore(&fixture.instance, &guard, &preview.id).is_err());
    assert_eq!(
        fs::read(fixture.instance.root().join("config/video.toml")).unwrap(),
        b"second change"
    );
}

#[test]
fn copying_an_identity_marker_never_authorizes_another_directory() {
    let fixture = Fixture::new();
    let original_guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let point = recovery::create_point(&fixture.instance, &original_guard, None).unwrap();
    let copied = InstanceLayout::new(&fixture.root).open(InstanceId::parse("copy").unwrap());
    copied.ensure_layout().unwrap();
    fs::create_dir(copied.root().join(".aurora")).unwrap();
    fs::copy(
        fixture
            .instance
            .root()
            .join(".aurora/recovery-identity.json"),
        copied.root().join(".aurora/recovery-identity.json"),
    )
    .unwrap();
    let copied_guard = acquire_instance_recovery_lock(&copied).unwrap();
    assert!(recovery::point_details(&copied, &copied_guard, &point.id).is_err());
    assert!(recovery::preview_restore(&copied, &copied_guard, &point.id).is_err());
}

#[test]
fn credential_like_configs_and_unprotected_profile_links_remain_current() {
    let fixture = Fixture::new();
    let sensitive = fixture.instance.root().join("config/ordinary-name.json");
    fs::write(&sensitive, br#"{"accessToken":"SYNTHETIC_OLD"}"#).unwrap();
    let profile = fixture.instance.root().join("aurora-modpack.json");
    fs::write(&profile, br#"{"name":"Saved title","projectId":"same-project","iconUrl":"https://example.invalid/old","javaPath":"C:/synthetic/java","account":"SYNTHETIC_OLD"}"#).unwrap();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();
    assert!(!point
        .files
        .iter()
        .any(|file| file.path == "config/ordinary-name.json"));
    assert!(!point
        .files
        .iter()
        .any(|file| file.path == "aurora-modpack.json"));
    fs::write(&sensitive, br#"{"accessToken":"SYNTHETIC_CURRENT"}"#).unwrap();
    fs::write(&profile, br#"{"name":"Current title","projectId":"same-project","iconUrl":"https://example.invalid/current","javaPath":"C:/synthetic/current-java","account":"SYNTHETIC_CURRENT"}"#).unwrap();
    let preview = recovery::preview_restore(&fixture.instance, &guard, &point.id).unwrap();
    assert!(preview
        .profile_changes
        .iter()
        .all(|change| change.field == "name"));
    recovery::restore(&fixture.instance, &guard, &preview.id).unwrap();
    assert_eq!(
        fs::read(&sensitive).unwrap(),
        br#"{"accessToken":"SYNTHETIC_CURRENT"}"#
    );
    let restored: serde_json::Value = serde_json::from_slice(&fs::read(&profile).unwrap()).unwrap();
    assert_eq!(restored["name"], "Saved title");
    assert_eq!(restored["projectId"], "same-project");
    assert_eq!(restored["iconUrl"], "https://example.invalid/current");
    assert_eq!(restored["javaPath"], "C:/synthetic/current-java");
    assert_eq!(restored["account"], "SYNTHETIC_CURRENT");
}

#[test]
fn checksummed_malicious_manifests_cannot_expand_scope_or_escape_paths() {
    for malicious in [
        "../outside.txt",
        "config/../../outside.txt",
        "C:/outside.txt",
        "/outside.txt",
        "config\\outside.txt",
        "config/file.txt:stream",
        "config/NUL.txt",
        "config/trailing. ",
        "saves/qa-world/level.dat",
        ".aurora/recovery-identity.json",
    ] {
        let fixture = Fixture::new();
        let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
        let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();
        let store = fixture.root.join(".aurora/recovery");
        let manifest_path = store.join("manifests").join(format!("{}.json", point.id));
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["point"]["files"][0]["path"] = malicious.into();
        let changed = serde_json::to_vec(&manifest).unwrap();
        fs::write(&manifest_path, &changed).unwrap();
        let db = rusqlite::Connection::open(store.join("index.sqlite3")).unwrap();
        db.execute(
            "UPDATE points SET manifest_hash=?1 WHERE id=?2",
            rusqlite::params![format!("{:x}", Sha256::digest(&changed)), point.id],
        )
        .unwrap();
        drop(db);
        let before = inventory(fixture.instance.root());
        assert!(
            recovery::preview_restore(&fixture.instance, &guard, &point.id).is_err(),
            "accepted {malicious}"
        );
        assert_eq!(before, inventory(fixture.instance.root()));
        assert!(!fixture.root.join("outside.txt").exists());
    }
}

#[test]
fn changed_runtime_cannot_replay_mods_into_a_different_loader() {
    let fixture = Fixture::new();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();
    fs::write(
        fixture.instance.root().join("aurora-instance.json"),
        br#"{"source":"forge","minecraft":"1.20.1","versionId":"1.20.1-forge-47.4.23"}"#,
    )
    .unwrap();
    assert!(recovery::preview_restore(&fixture.instance, &guard, &point.id).is_err());
}

#[test]
fn correcting_a_copied_markers_physical_identity_does_not_forge_the_registry() {
    let fixture = Fixture::new();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();
    let original: serde_json::Value = serde_json::from_slice(
        &fs::read(
            fixture
                .instance
                .root()
                .join(".aurora/recovery-identity.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let copied = InstanceLayout::new(&fixture.root).open(InstanceId::parse("copy").unwrap());
    copied.ensure_layout().unwrap();
    let copied_guard = acquire_instance_recovery_lock(&copied).unwrap();
    recovery::create_point(&copied, &copied_guard, None).unwrap();
    let marker_path = copied.root().join(".aurora/recovery-identity.json");
    let mut forged: serde_json::Value =
        serde_json::from_slice(&fs::read(&marker_path).unwrap()).unwrap();
    forged["uuid"] = original["uuid"].clone();
    fs::write(marker_path, serde_json::to_vec(&forged).unwrap()).unwrap();
    assert!(recovery::point_details(&copied, &copied_guard, &point.id).is_err());
    assert!(recovery::preview_restore(&copied, &copied_guard, &point.id).is_err());
}

#[test]
fn leaf_hardlinks_cannot_mutate_or_reuse_external_backup_content() {
    let fixture = Fixture::new();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();
    let file = &point.files[0];
    let blob = fixture
        .root
        .join(".aurora/recovery/blobs/sha256")
        .join(&file.sha256[..2])
        .join(&file.sha256);
    let external = fixture.root.join("external-owned-by-someone-else.bin");
    fs::rename(&blob, &external).unwrap();
    fs::hard_link(&external, &blob).unwrap();
    let before = fs::read(&external).unwrap();
    assert!(recovery::point_details(&fixture.instance, &guard, &point.id).is_err());
    assert!(recovery::preview_restore(&fixture.instance, &guard, &point.id).is_err());
    assert_eq!(before, fs::read(&external).unwrap());
    fs::hard_link(
        &external,
        fixture.instance.root().join("config/external.toml"),
    )
    .unwrap();
    assert!(recovery::create_point(&fixture.instance, &guard, None).is_err());
}

#[cfg(windows)]
#[test]
fn a_real_windows_junction_in_scope_never_reads_or_changes_its_target() {
    use std::os::windows::process::CommandExt;
    struct Junction(PathBuf);
    impl Drop for Junction {
        fn drop(&mut self) {
            let _ = fs::remove_dir(&self.0);
        }
    }
    let fixture = Fixture::new();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();
    let target = fixture.root.join("outside-scope");
    let link = fixture.instance.root().join("config");
    fs::rename(&link, &target).unwrap();
    let output = std::process::Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(&link)
        .arg(&target)
        .creation_flags(0x0800_0000)
        .output()
        .unwrap();
    assert!(output.status.success());
    let _junction = Junction(link);
    let before = inventory(&target);
    assert!(recovery::create_point(&fixture.instance, &guard, None).is_err());
    assert!(recovery::preview_restore(&fixture.instance, &guard, &point.id).is_err());
    assert_eq!(before, inventory(&target));
}

#[test]
fn rewriting_a_manifest_after_preview_requires_another_review_even_with_valid_hash() {
    let fixture = Fixture::new();
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();
    fs::write(
        fixture.instance.root().join("config/video.toml"),
        b"changed",
    )
    .unwrap();
    let preview = recovery::preview_restore(&fixture.instance, &guard, &point.id).unwrap();
    let store = fixture.root.join(".aurora/recovery");
    let manifest_path = store.join("manifests").join(format!("{}.json", point.id));
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["point"]["description"] = "Changed after the review".into();
    let changed = serde_json::to_vec(&manifest).unwrap();
    fs::write(manifest_path, &changed).unwrap();
    let db = rusqlite::Connection::open(store.join("index.sqlite3")).unwrap();
    db.execute(
        "UPDATE points SET manifest_hash=?1 WHERE id=?2",
        rusqlite::params![format!("{:x}", Sha256::digest(&changed)), point.id],
    )
    .unwrap();
    drop(db);
    assert!(recovery::restore(&fixture.instance, &guard, &preview.id).is_err());
    assert_eq!(
        fs::read(fixture.instance.root().join("config/video.toml")).unwrap(),
        b"changed"
    );
}

#[test]
fn a_file_directory_type_conflict_is_rejected_before_a_pending_journal_can_strand_the_instance() {
    for directory_in_point in [false, true] {
        let fixture = Fixture::new();
        let root = fixture.instance.root();
        if directory_in_point {
            fs::create_dir(root.join("config/conflict")).unwrap();
            fs::write(root.join("config/conflict/item.txt"), b"saved nested").unwrap();
        } else {
            fs::write(root.join("config/conflict"), b"saved flat").unwrap();
        }
        let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
        let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();
        if directory_in_point {
            fs::remove_file(root.join("config/conflict/item.txt")).unwrap();
            fs::remove_dir(root.join("config/conflict")).unwrap();
            fs::write(root.join("config/conflict"), b"current flat").unwrap();
        } else {
            fs::remove_file(root.join("config/conflict")).unwrap();
            fs::create_dir(root.join("config/conflict")).unwrap();
            fs::write(root.join("config/conflict/item.txt"), b"current nested").unwrap();
        }
        let before = inventory(&root.join("config"));
        if let Ok(preview) = recovery::preview_restore(&fixture.instance, &guard, &point.id) {
            assert!(recovery::restore(&fixture.instance, &guard, &preview.id).is_err());
        }
        assert_eq!(before, inventory(&root.join("config")));
        assert!(!recovery::has_pending(&fixture.instance).unwrap());
    }
}

#[test]
fn jar_bytes_and_active_disabled_off_states_round_trip_through_restore_and_undo() {
    let fixture = Fixture::new();
    let mods = fixture.instance.mods_dir();
    fs::write(mods.join("active.jar"), synthetic_jar_version(false, "1")).unwrap();
    fs::write(
        mods.join("parked.jar.disabled"),
        synthetic_jar_version(false, "2"),
    )
    .unwrap();
    fs::write(
        mods.join("removed.jar.off"),
        synthetic_jar_version(false, "3"),
    )
    .unwrap();
    let saved = inventory(&mods);
    let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
    let point = recovery::create_point(&fixture.instance, &guard, None).unwrap();

    fs::rename(mods.join("active.jar"), mods.join("active.jar.disabled")).unwrap();
    fs::write(
        mods.join("active.jar.disabled"),
        synthetic_jar_version(false, "4"),
    )
    .unwrap();
    fs::rename(mods.join("parked.jar.disabled"), mods.join("parked.jar")).unwrap();
    fs::remove_file(mods.join("removed.jar.off")).unwrap();
    fs::write(mods.join("added.jar"), synthetic_jar_version(false, "5")).unwrap();
    let changed = inventory(&mods);
    let preview = recovery::preview_restore(&fixture.instance, &guard, &point.id).unwrap();
    for (path, action) in [
        ("mods/active.jar", "add"),
        ("mods/active.jar.disabled", "remove"),
        ("mods/parked.jar.disabled", "add"),
        ("mods/parked.jar", "remove"),
        ("mods/removed.jar.off", "add"),
        ("mods/added.jar", "remove"),
    ] {
        assert!(preview
            .changes
            .iter()
            .any(|change| change.path == path && change.action == action));
    }
    recovery::restore(&fixture.instance, &guard, &preview.id).unwrap();
    assert_eq!(saved, inventory(&mods));
    let undo = recovery::preview_undo(&fixture.instance, &guard).unwrap();
    recovery::restore(&fixture.instance, &guard, &undo.id).unwrap();
    assert_eq!(changed, inventory(&mods));
}
