use super::*;
use crate::engine::{acquire_instance_mutation_lock, acquire_instance_recovery_lock};

struct Fixture {
    root: PathBuf,
    source: Instance,
}
impl Fixture {
    fn new() -> Self {
        FAULT.with(|f| f.set(Fault::None));
        let root = std::env::temp_dir().join(format!("aurora-lab-tests-{}", Uuid::new_v4()));
        let source = InstanceLayout::new(&root).open(InstanceId::parse("source").unwrap());
        source.ensure_layout().unwrap();
        fs::create_dir_all(source.root().join("config")).unwrap();
        fs::create_dir_all(source.root().join("saves/PersonalWorld")).unwrap();
        fs::write(
            source.root().join("saves/PersonalWorld/level.dat"),
            b"world contents",
        )
        .unwrap();
        fs::write(source.root().join("mods/example.jar"), b"test jar bytes").unwrap();
        fs::write(
            source.root().join("config/graphics.json"),
            br#"{"renderDistance":16,"maxFps":120,"unsupported":17}"#,
        )
        .unwrap();
        fs::write(
            source.root().join("config/accounts.json"),
            br#"{"token":"do-not-copy"}"#,
        )
        .unwrap();
        fs::write(source.root().join("options.txt"), b"gamma:1").unwrap();
        fs::write(source.root().join("aurora-instance.json"),br#"{"minecraft":"1.20.1","versionId":"1.20.1","source":"vanilla","accountToken":"private"}"#).unwrap();
        Self { root, source }
    }
    fn create(&self, world: Option<&str>) -> Instance {
        let guard = acquire_instance_mutation_lock(&self.source).unwrap();
        let env = create(&self.source, &guard, world, |_| {}).unwrap();
        InstanceLayout::new(&self.root).open(InstanceId::parse(env.id).unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        FAULT.with(|f| f.set(Fault::None));
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn copy_has_fresh_identity_independent_files_and_excludes_worlds_credentials_history() {
    let f = Fixture::new();
    fs::write(f.source.root().join("logs/latest.log"), b"old log").unwrap();
    let test = f.create(None);
    assert_ne!(
        recovery::identity_lookup(&f.source).unwrap(),
        recovery::identity_lookup(&test).unwrap()
    );
    assert!(!test.root().join("saves").exists());
    assert!(!test.root().join("config/accounts.json").exists());
    assert!(!test.root().join("logs/latest.log").exists());
    let profile = fs::read_to_string(test.root().join("aurora-instance.json")).unwrap();
    assert!(!profile.contains("private"));
    assert!(!profile.contains("accountToken"));
    fs::write(test.root().join("mods/example.jar"), b"test changed").unwrap();
    assert_eq!(
        fs::read(f.source.root().join("mods/example.jar")).unwrap(),
        b"test jar bytes"
    );
    assert!(ensure_launchable(&test).is_ok());
}
#[test]
fn chosen_world_is_independent_and_not_part_of_applicable_changes() {
    let f = Fixture::new();
    let test = f.create(Some("PersonalWorld"));
    fs::write(
        test.root().join("saves/PersonalWorld/level.dat"),
        b"discardable changes",
    )
    .unwrap();
    assert_eq!(
        fs::read(f.source.root().join("saves/PersonalWorld/level.dat")).unwrap(),
        b"world contents"
    );
    let guard = acquire_instance_mutation_lock(&test).unwrap();
    assert!(inspect(&test, &guard).unwrap().changes.is_empty());
    assert_eq!(
        worlds(
            &f.source,
            &acquire_instance_mutation_lock(&f.source).unwrap()
        )
        .unwrap(),
        vec!["PersonalWorld"]
    );
}
#[test]
fn selected_toggle_and_setting_apply_transactionally_and_undo_preserves_world() {
    let f = Fixture::new();
    let test = f.create(None);
    let tg = acquire_instance_mutation_lock(&test).unwrap();
    toggle_mod(&test, &tg, "mods/example.jar", false).unwrap();
    set_setting(
        &test,
        &tg,
        "config/graphics.json",
        "/renderDistance",
        Value::from(8),
    )
    .unwrap();
    // Unsupported edits in the same JSON must never hitchhike to the source.
    let mut doc: Value =
        serde_json::from_slice(&fs::read(test.root().join("config/graphics.json")).unwrap())
            .unwrap();
    doc["unsupported"] = Value::from(99);
    fs::write(
        test.root().join("config/graphics.json"),
        serde_json::to_vec(&doc).unwrap(),
    )
    .unwrap();
    let sg = acquire_instance_mutation_lock(&f.source).unwrap();
    let selected = vec![
        "mod:mods/example.jar".into(),
        "setting:config/graphics.json#/renderDistance".into(),
    ];
    let preview = preview_apply(&f.source, &sg, &test, &tg, &selected).unwrap();
    assert_eq!(preview.changes.len(), 3);
    let result = apply(&f.source, &sg, &test, &tg, &preview.id).unwrap();
    assert!(result.undo_available);
    assert!(!f.source.root().join("mods/example.jar").exists());
    assert!(f.source.root().join("mods/example.jar.disabled").exists());
    let doc: Value =
        serde_json::from_slice(&fs::read(f.source.root().join("config/graphics.json")).unwrap())
            .unwrap();
    assert_eq!(doc["renderDistance"], 8);
    assert_eq!(doc["unsupported"], 17);
    let undo = recovery::preview_undo(&f.source, &sg).unwrap();
    recovery::restore(&f.source, &sg, &undo.id).unwrap();
    assert!(f.source.root().join("mods/example.jar").exists());
    assert_eq!(
        fs::read(f.source.root().join("saves/PersonalWorld/level.dat")).unwrap(),
        b"world contents"
    );
}
#[test]
fn source_changes_and_test_changes_invalidate_preview() {
    let f = Fixture::new();
    let test = f.create(None);
    let tg = acquire_instance_mutation_lock(&test).unwrap();
    let sg = acquire_instance_mutation_lock(&f.source).unwrap();
    toggle_mod(&test, &tg, "mods/example.jar", false).unwrap();
    let selected = vec!["mod:mods/example.jar".into()];
    let preview = preview_apply(&f.source, &sg, &test, &tg, &selected).unwrap();
    fs::write(test.root().join("options.txt"), b"gamma:2").unwrap();
    assert!(matches!(
        apply(&f.source, &sg, &test, &tg, &preview.id),
        Err(RecoveryError::StalePreview)
    ));
    fs::write(f.source.root().join("options.txt"), b"gamma:3").unwrap();
    assert!(matches!(
        preview_apply(&f.source, &sg, &test, &tg, &selected),
        Err(RecoveryError::OriginChanged)
    ));
    assert!(f.source.root().join("mods/example.jar").exists());
}
#[test]
fn source_rename_is_resolved_by_identity_and_recreation_is_not() {
    let f = Fixture::new();
    let test = f.create(None);
    let renamed = InstanceLayout::new(&f.root).open(InstanceId::parse("renamed").unwrap());
    fs::rename(f.source.root(), renamed.root()).unwrap();
    assert_eq!(origin_instance(&f.root, &test).unwrap().id(), renamed.id());
    fs::remove_dir_all(renamed.root()).unwrap();
    renamed.ensure_layout().unwrap();
    assert!(origin_instance(&f.root, &test).is_err());
}
#[test]
fn source_change_during_copy_never_publishes_ready() {
    let f = Fixture::new();
    let sg = acquire_instance_mutation_lock(&f.source).unwrap();
    let mut changed = false;
    let result = create(&f.source, &sg, None, |progress| {
        if progress.completed_bytes > 0 && !changed {
            fs::write(f.source.root().join("options.txt"), b"external change").unwrap();
            changed = true;
        }
    });
    assert!(result.is_err());
    let entries = list(&f.root).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].state, "failed");
    let test = InstanceLayout::new(&f.root).open(InstanceId::parse(&entries[0].id).unwrap());
    assert!(ensure_launchable(&test).is_err());
    let tg = acquire_instance_recovery_lock(&test).unwrap();
    delete(&test, &tg).unwrap();
    assert!(!test.root().exists());
}
#[test]
fn interrupted_copy_survives_restart_and_can_be_deleted() {
    let f = Fixture::new();
    let sg = acquire_instance_mutation_lock(&f.source).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        create(&f.source, &sg, None, |p| {
            if p.completed_bytes > 0 {
                panic!("simulated process interruption");
            }
        })
    }));
    assert!(result.is_err());
    let env = list(&f.root).unwrap().remove(0);
    assert_eq!(env.state, "failed");
    let test = InstanceLayout::new(&f.root).open(InstanceId::parse(&env.id).unwrap());
    let tg = acquire_instance_recovery_lock(&test).unwrap();
    delete(&test, &tg).unwrap();
    assert!(list(&f.root).unwrap().is_empty());
}
#[test]
fn deletion_is_resumable_and_does_not_touch_original() {
    let f = Fixture::new();
    let test = f.create(None);
    let tg = acquire_instance_recovery_lock(&test).unwrap();
    FAULT.with(|v| v.set(Fault::DeleteInterrupted));
    assert!(delete(&test, &tg).is_err());
    FAULT.with(|v| v.set(Fault::None));
    assert!(ensure_launchable(&test).is_err());
    delete(&test, &tg).unwrap();
    assert!(list(&f.root).unwrap().is_empty());
    assert!(f.source.root().join("mods/example.jar").exists());
}
#[test]
fn no_space_and_wrong_lock_leave_source_untouched() {
    let f = Fixture::new();
    let sg = acquire_instance_mutation_lock(&f.source).unwrap();
    FAULT.with(|v| v.set(Fault::NoSpace));
    assert!(matches!(
        create(&f.source, &sg, None, |_| {}),
        Err(RecoveryError::NoSpace)
    ));
    FAULT.with(|v| v.set(Fault::None));
    assert!(list(&f.root).unwrap().is_empty());
    assert!(acquire_instance_mutation_lock(&f.source).is_err());
    let env = create(&f.source, &sg, None, |_| {}).unwrap();
    let test = InstanceLayout::new(&f.root).open(InstanceId::parse(env.id).unwrap());
    assert!(matches!(inspect(&test, &sg), Err(RecoveryError::WrongLock)));
}
#[test]
fn malicious_paths_hardlinks_and_runtime_credentials_are_blocked() {
    let f = Fixture::new();
    let sg = acquire_instance_mutation_lock(&f.source).unwrap();
    assert!(create(&f.source, &sg, Some("../outside"), |_| {}).is_err());
    fs::hard_link(
        f.source.root().join("options.txt"),
        f.source.root().join("config/hardlink.txt"),
    )
    .unwrap();
    assert!(create(&f.source, &sg, None, |_| {}).is_err());
    fs::remove_file(f.source.root().join("config/hardlink.txt")).unwrap();
    fs::create_dir_all(f.source.root().join("versions/test")).unwrap();
    fs::write(
        f.source.root().join("versions/test/test.json"),
        br#"{"arguments":{"game":["--accessToken","persisted-secret"]}}"#,
    )
    .unwrap();
    assert!(create(&f.source, &sg, None, |_| {}).is_err());
}
#[test]
fn protected_mods_and_unsupported_settings_cannot_be_mutated_or_transferred() {
    let f = Fixture::new();
    let test = f.create(None);
    let tg = acquire_instance_mutation_lock(&test).unwrap();
    fs::write(test.root().join("mods/aurora-core.jar"), b"managed").unwrap();
    assert!(toggle_mod(&test, &tg, "mods/aurora-core.jar", false).is_err());
    assert!(set_setting(
        &test,
        &tg,
        "config/graphics.json",
        "/unsupported",
        Value::from(4)
    )
    .is_err());
    assert!(set_setting(
        &test,
        &tg,
        "config/graphics.json",
        "/renderDistance",
        Value::from(999)
    )
    .is_err());
    assert!(toggle_mod(&test, &tg, "../../source/mods/example.jar", false).is_err());
    let sg = acquire_instance_mutation_lock(&f.source).unwrap();
    assert!(preview_apply(
        &f.source,
        &sg,
        &test,
        &tg,
        &["setting:config/graphics.json#/unsupported".into()]
    )
    .is_err());
}
#[test]
fn corrupt_provenance_and_copied_identity_are_rejected() {
    let f = Fixture::new();
    let test = f.create(None);
    let marker = test.root().join(".aurora/recovery-identity.json");
    fs::copy(
        f.source.root().join(".aurora/recovery-identity.json"),
        &marker,
    )
    .unwrap();
    assert!(ensure_launchable(&test).is_err());
}

#[test]
fn pre_record_interruptions_are_visible_and_safely_cleanable() {
    for fault in [Fault::BeforeClaim, Fault::BeforeRecord] {
        let f = Fixture::new();
        let sg = acquire_instance_mutation_lock(&f.source).unwrap();
        FAULT.with(|v| v.set(fault));
        assert!(create(&f.source, &sg, None, |_| {}).is_err());
        FAULT.with(|v| v.set(Fault::None));
        let entries = list(&f.root).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].state, "failed");
        let test = InstanceLayout::new(&f.root).open(InstanceId::parse(&entries[0].id).unwrap());
        let tg = acquire_instance_recovery_lock(&test).unwrap();
        delete(&test, &tg).unwrap();
        assert!(list(&f.root).unwrap().is_empty());
    }
}
#[test]
fn external_destination_edits_after_file_copy_block_ready() {
    let f = Fixture::new();
    let sg = acquire_instance_mutation_lock(&f.source).unwrap();
    let mut changed = false;
    assert!(create(&f.source, &sg, None, |p| {
        if p.completed_bytes > 0 && !changed {
            let env = list(&f.root).unwrap().remove(0);
            let test = InstanceLayout::new(&f.root).open(InstanceId::parse(env.id).unwrap());
            fs::write(test.root().join("config/graphics.json"), b"{}").unwrap();
            changed = true;
        }
    })
    .is_err());
    assert_eq!(list(&f.root).unwrap()[0].state, "failed");
}
#[test]
fn corrupt_one_record_does_not_hide_other_environments() {
    let f = Fixture::new();
    let one = f.create(None);
    let two = f.create(None);
    fs::write(
        marker(&f.root, one.id().as_str(), "record.json").unwrap(),
        b"broken",
    )
    .unwrap();
    let listed = list(&f.root).unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed
        .iter()
        .any(|e| e.id == one.id().as_str() && e.state == "failed"));
    assert!(listed
        .iter()
        .any(|e| e.id == two.id().as_str() && e.state == "ready"));
    let tg = acquire_instance_recovery_lock(&one).unwrap();
    assert!(delete(&one, &tg).is_err());
    assert!(one.root().exists());
}
#[test]
fn absent_root_after_delete_is_reconciled_without_recreation() {
    let f = Fixture::new();
    let test = f.create(None);
    let tg = acquire_instance_recovery_lock(&test).unwrap();
    delete(&test, &tg).unwrap();
    fs::remove_file(marker(&f.root, test.id().as_str(), "deleted.json").unwrap()).unwrap();
    assert!(finish_deleted(&f.root, test.id().as_str()).unwrap());
    assert!(!test.root().exists());
    assert!(list(&f.root).unwrap().is_empty());
}
#[test]
fn deleting_a_replaced_root_fails_closed() {
    let f = Fixture::new();
    let test = f.create(None);
    fs::rename(test.root(), test.root().with_extension("original-test")).unwrap();
    test.ensure_layout().unwrap();
    fs::write(test.root().join("do-not-delete"), b"replacement root").unwrap();
    let tg = acquire_instance_recovery_lock(&test).unwrap();
    assert!(matches!(delete(&test, &tg), Err(RecoveryError::Identity)));
    assert!(test.root().join("do-not-delete").exists());
}

#[test]
fn staged_configuration_corruption_cannot_transfer_unselected_fields() {
    let f = Fixture::new();
    let test = f.create(None);
    let tg = acquire_instance_mutation_lock(&test).unwrap();
    set_setting(
        &test,
        &tg,
        "config/graphics.json",
        "/renderDistance",
        Value::from(8),
    )
    .unwrap();
    let sg = acquire_instance_mutation_lock(&f.source).unwrap();
    FAULT.with(|v| v.set(Fault::StageCorruption));
    assert!(matches!(
        preview_apply(
            &f.source,
            &sg,
            &test,
            &tg,
            &["setting:config/graphics.json#/renderDistance".into()]
        ),
        Err(RecoveryError::Integrity)
    ));
    FAULT.with(|v| v.set(Fault::None));
    let doc: Value =
        serde_json::from_slice(&fs::read(f.source.root().join("config/graphics.json")).unwrap())
            .unwrap();
    assert_eq!(doc["renderDistance"], 16);
    assert_eq!(doc["unsupported"], 17);
}

#[test]
fn persisted_runtime_authentication_variants_are_rejected() {
    for value in [
        "--accessToken=synthetic",
        "--SESSION=synthetic",
        "--clientId=synthetic",
        "--xuid=synthetic",
        "--AccessToken synthetic",
        "--SeSsIoN synthetic",
        "-Dexample.accessToken=synthetic",
        "https://example.invalid/file?access_token=synthetic",
        "https://example.invalid/file?api_key=synthetic",
        "https://synthetic:synthetic@example.invalid/file",
    ] {
        let bytes = serde_json::to_vec(&serde_json::json!({"arguments":{"game":[value]}})).unwrap();
        assert!(
            matches!(
                validate_runtime_json(&bytes),
                Err(RecoveryError::UnsafePath)
            ),
            "accepted {value}"
        );
    }
    let valid = serde_json::json!({"arguments":{"game":["--username","${auth_player_name}","--accessToken","${auth_access_token}","--clientId=${clientid}","--xuid=${auth_xuid}"]},"minecraftArguments":"--AccessToken ${auth_access_token} --username ${auth_player_name}"});
    assert!(validate_runtime_json(&serde_json::to_vec(&valid).unwrap()).is_ok());
}

#[test]
fn runtime_paths_and_launch_overrides_cannot_escape_test_root() {
    for value in [
        serde_json::json!({"libraries":[{"name":"safe:library:1","downloads":{"artifact":{"path":"C:/original/private.jar"}}}]}),
        serde_json::json!({"libraries":[{"name":"safe:../library:1"}]}),
        serde_json::json!({"inheritsFrom":"../original"}),
        serde_json::json!({"arguments":{"game":["--gameDir=C:/original"]}}),
        serde_json::json!({"arguments":{"game":["--assetsDir","../original/assets"]}}),
        serde_json::json!({"arguments":{"jvm":["-Djava.library.path=C:/original/natives"]}}),
        serde_json::json!({"minecraftArguments":"--gameDir /original"}),
    ] {
        assert!(
            matches!(
                validate_runtime_json(&serde_json::to_vec(&value).unwrap()),
                Err(RecoveryError::UnsafePath)
            ),
            "accepted {value}"
        );
    }
    let valid = serde_json::json!({"id":"1.20.1-forge-47.4.0","inheritsFrom":"1.20.1","libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.4.0:universal","downloads":{"artifact":{"path":"net/minecraftforge/forge/1.20.1-47.4.0/forge.jar"}}}],"arguments":{"game":["--gameDir","${game_directory}","--assetsDir","${assets_root}"],"jvm":["-Djava.library.path=${natives_directory}","-DlibraryDirectory=${library_directory}","-p","${library_directory}/net/minecraftforge/bootstraplauncher/1.1.2/bootstraplauncher-1.1.2.jar${classpath_separator}${library_directory}/cpw/mods/securejarhandler/2.1.10/securejarhandler-2.1.10.jar","--add-opens=java.base/java.util.jar=ALL-UNNAMED"]}});
    assert!(validate_runtime_json(&serde_json::to_vec(&valid).unwrap()).is_ok());
}
#[test]
fn changed_runtime_profile_is_revalidated_before_lab_launch() {
    let f = Fixture::new();
    let test = f.create(None);
    fs::create_dir_all(test.root().join("versions/test")).unwrap();
    fs::write(
        test.root().join("versions/test/test.json"),
        br#"{"arguments":{"game":["--gameDir=../source"]}}"#,
    )
    .unwrap();
    assert!(matches!(
        ensure_launchable(&test),
        Err(RecoveryError::UnsafePath)
    ));
}
