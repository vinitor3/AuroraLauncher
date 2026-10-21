//! Negative cases operate on real factory bundles after all previous owners
//! have gone away. The production reader must reopen precisely these bytes.
use super::*;
use std::sync::{atomic::AtomicUsize, Mutex};

fn interrupted(fixture: &Fixture, step: Step) -> (BundleId, TestPaths) {
    let hooks = fixture::hook_fault(step);
    let prepared = fixture.prepared(hooks.clone());
    let id = prepared.bundle_id();
    let paths = prepared.test_paths();
    let approval = prepared.review().digest.clone();
    let result = if matches!(
        step,
        Step::RestoreIntent | Step::AppliedParked | Step::OriginalRestored | Step::RestoredMarked
    ) {
        prepared
            .apply_eula(&approval)
            .expect("hardening_apply_before_restore_fault")
            .restore_before_spawn()
            .map(|_| ())
    } else {
        prepared.apply_eula(&approval).map(|_| ())
    };
    assert_eq!(result, Err(Error::Injected));
    unlocked(&paths);
    (id, paths)
}

fn denied_unchanged(fixture: &Fixture, id: BundleId, paths: &TestPaths) {
    let before = files::tree(fixture.root.path());
    let hooks = Hooks::default();
    assert!(fixture
        .recover(id, &probe::Probe::default(), hooks.clone())
        .is_err());
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
    assert_eq!(files::tree(fixture.root.path()), before);
    unlocked(paths);
}

#[test]
fn corrupted_eula_slots_and_unknown_entries_cannot_be_repaired_by_guessing() {
    for (step, name) in [
        (Step::Intent, "stage.bin"),
        (Step::AppliedMarked, "snapshot.bin"),
        (Step::AppliedMarked, "old.bin"),
        (Step::RestoredMarked, "applied.bin"),
        (Step::AppliedMarked, "unrecognized.bin"),
    ] {
        let fixture = Fixture::new();
        let (id, paths) = interrupted(&fixture, step);
        let path = slot(&paths, name);
        fs::write(&path, b"independent bytes; never reconstruct from snapshot")
            .expect("hardening_corrupt_slot");
        denied_unchanged(&fixture, id, &paths);
        assert_eq!(
            bytes(&path),
            b"independent bytes; never reconstruct from snapshot"
        );
    }
}

#[test]
fn closed_journal_schema_uuid_and_bounds_are_checked_before_mutation() {
    // Each case starts with a different genuine transaction. Metadata bytes are
    // overwritten in place so a different file ID cannot explain rejection.
    for (step, name, mutation) in [
        (Step::AppliedMarked, "claim.json", "unknown"),
        (Step::AppliedMarked, "claim.json", "duplicate"),
        (Step::AppliedMarked, "claim.json", "bundle"),
        (Step::AppliedMarked, "claim.json", "schema"),
        (Step::AppliedMarked, "intent.json", "transaction"),
        (Step::AppliedMarked, "intent.json", "truncated"),
        (Step::AppliedMarked, "original-moved.json", "phase"),
        (Step::AppliedMarked, "applied.json", "transaction"),
        (Step::RestoreIntent, "restore-intent.json", "from"),
        (Step::RestoredMarked, "restored.json", "outcome"),
        (Step::AppliedMarked, "claim.json", "oversize"),
    ] {
        let fixture = Fixture::new();
        let (id, paths) = interrupted(&fixture, step);
        let path = slot(&paths, name);
        let original = bytes(&path);
        let mut json: serde_json::Value =
            serde_json::from_slice(&original).expect("hardening_original_json");
        let changed = match mutation {
            "unknown" => {
                json["unrecognized"] = serde_json::json!(true);
                serde_json::to_vec(&json).expect("hardening_unknown_json")
            }
            "duplicate" => {
                assert_eq!(original.first(), Some(&b'{'));
                let mut duplicate = br#"{"schema":1,"#.to_vec();
                duplicate.extend_from_slice(&original[1..]);
                duplicate
            }
            "bundle" | "transaction" => {
                json[mutation] = serde_json::json!(Uuid::new_v4().to_string());
                serde_json::to_vec(&json).expect("hardening_uuid_json")
            }
            "schema" => {
                json["schema"] = serde_json::json!("1");
                serde_json::to_vec(&json).expect("hardening_schema_json")
            }
            "phase" | "from" | "outcome" => {
                json[mutation] = serde_json::json!("Unrecognized");
                serde_json::to_vec(&json).expect("hardening_phase_json")
            }
            "truncated" => original[..original.len() / 2].to_vec(),
            "oversize" => vec![b' '; 32 * 1024 + 1],
            _ => unreachable!(),
        };
        fs::write(&path, &changed).expect("hardening_write_json");
        denied_unchanged(&fixture, id, &paths);
        assert_eq!(bytes(&path), changed);
    }
}

#[test]
fn hardlinked_slots_target_and_manifest_are_refused_without_touching_the_alias() {
    for (step, name) in [
        (Step::Intent, "stage.bin"),
        (Step::AppliedMarked, "snapshot.bin"),
        (Step::AppliedMarked, "old.bin"),
        (Step::RestoredMarked, "applied.bin"),
        (Step::AppliedMarked, "claim.json"),
        (Step::AppliedMarked, "target"),
        (Step::AppliedMarked, "manifest"),
    ] {
        let fixture = Fixture::new();
        let (id, paths) = interrupted(&fixture, step);
        let selected = match name {
            "target" => target(&paths),
            "manifest" => paths.bundle.join("bundle.json"),
            _ => slot(&paths, name),
        };
        let alias = fixture.root.path().join("external-alias.bin");
        fs::hard_link(&selected, &alias).expect("hardening_real_hardlink");
        let original = bytes(&alias);
        denied_unchanged(&fixture, id, &paths);
        assert_eq!(bytes(&alias), original);
        assert_eq!(bytes(&selected), original);
    }
}

#[test]
fn another_bundles_real_journal_never_authorizes_the_target_copy() {
    let fixture = Fixture::new();
    let (first_id, first) = interrupted(&fixture, Step::AppliedMarked);
    let (second_id, second) = interrupted(&fixture, Step::AppliedMarked);
    assert_ne!(first_id, second_id);
    let retained = fixture.root.path().join("retained-second-transaction");
    fs::rename(&second.transaction, &retained).expect("hardening_park_second_transaction");
    fs::rename(&first.transaction, &second.transaction).expect("hardening_transplant_transaction");
    denied_unchanged(&fixture, second_id, &second);
    assert_eq!(bytes(&target(&first)), EULA_TRUE);
    assert_eq!(bytes(&target(&second)), EULA_TRUE);
    assert_eq!(bytes(&retained.join("old.bin")), EULA_FALSE);
}

#[test]
fn moving_the_entire_bundle_to_another_data_root_does_not_transfer_authority() {
    let first = Fixture::new();
    let second = Fixture::new();
    let (id, paths) = interrupted(&first, Step::AppliedMarked);
    let name = paths.bundle.file_name().expect("hardening_bundle_name");
    let moved = second.lab.join(name);
    // A same-volume rename preserves the bundle and all file IDs. Rejection
    // must still bind the actual diagnostic root and source provenance.
    fs::rename(&paths.bundle, &moved).expect("hardening_move_bundle_root");
    let first_before = files::tree(first.root.path());
    let second_before = files::tree(second.root.path());
    let hooks = Hooks::default();
    assert!(second
        .recover(id, &probe::Probe::default(), hooks.clone())
        .is_err());
    assert_eq!(hooks.eula_renames.load(Ordering::SeqCst), 0);
    assert_eq!(files::tree(first.root.path()), first_before);
    assert_eq!(files::tree(second.root.path()), second_before);
}

#[test]
fn missing_or_unsafe_recovery_database_is_not_initialized_or_repaired() {
    for sidecar in [None, Some("-wal"), Some("-journal"), Some("-shm")] {
        let fixture = Fixture::new();
        let (id, paths) = interrupted(&fixture, Step::AppliedMarked);
        let native = paths
            .copy
            .parent()
            .and_then(Path::parent)
            .expect("hardening_native");
        let database = native.join(".aurora/recovery/index.sqlite3");
        assert!(database.is_file(), "hardening_existing_database");
        if let Some(suffix) = sidecar {
            fs::write(
                native.join(format!(".aurora/recovery/index.sqlite3{suffix}")),
                b"independent sidecar; do not repair",
            )
            .expect("hardening_sidecar");
        } else {
            fs::rename(
                &database,
                fixture.root.path().join("retained-index.sqlite3"),
            )
            .expect("hardening_remove_database");
        }
        denied_unchanged(&fixture, id, &paths);
        if sidecar.is_none() {
            assert!(!database.exists());
        }
    }
}

#[test]
fn origin_and_copy_provenance_drift_preserves_the_pending_eula() {
    for choice in [
        "origin",
        "copy-runtime",
        "copy-identity",
        "manifest-uuid",
        "mods",
    ] {
        let fixture = Fixture::new();
        let (id, paths) = interrupted(&fixture, Step::AppliedMarked);
        match choice {
            "origin" => {
                fs::create_dir_all(paths.origin.join("config"))
                    .expect("hardening_origin_config_directory");
                fs::write(
                    paths.origin.join("config/new-origin.txt"),
                    b"new origin inventory",
                )
                .expect("hardening_origin_change");
            }
            "copy-runtime" => {
                fs::write(
                    paths.copy.join("aurora-instance.json"),
                    br#"{"minecraft":"1.20.1","loader":"fabric","versionId":"1.20.1"}"#,
                )
                .expect("hardening_runtime_change");
            }
            "copy-identity" => {
                fs::write(paths.copy.join(".aurora/recovery-identity.json"), b"{}")
                    .expect("hardening_identity_change");
            }
            "manifest-uuid" => {
                let path = paths.bundle.join("bundle.json");
                let mut json: serde_json::Value =
                    serde_json::from_slice(&bytes(&path)).expect("hardening_manifest_json");
                json["copy_uuid"] = serde_json::json!(Uuid::new_v4().to_string());
                fs::write(
                    path,
                    serde_json::to_vec(&json).expect("hardening_manifest_encode"),
                )
                .expect("hardening_manifest_uuid");
            }
            "mods" => {
                fs::write(
                    paths.copy.join("mods/foreign.jar"),
                    b"not a server resource",
                )
                .expect("hardening_foreign_mod");
            }
            _ => unreachable!(),
        }
        denied_unchanged(&fixture, id, &paths);
        assert_eq!(bytes(&target(&paths)), EULA_TRUE);
        assert_eq!(bytes(&slot(&paths, "old.bin")), EULA_FALSE);
    }
}

#[test]
fn an_independent_target_created_in_either_absent_interval_is_never_overwritten() {
    for step in [Step::OriginalMoved, Step::AppliedParked] {
        let fixture = Fixture::new();
        let selected = Arc::new(Mutex::new(None::<PathBuf>));
        let writes = Arc::new(AtomicUsize::new(0));
        let selected_hook = selected.clone();
        let writes_hook = writes.clone();
        let independent = b"# created by an independent writer\neula=false\n";
        let hooks = Hooks {
            after_step: Some(Arc::new(move |actual| {
                if actual == step {
                    let path = selected_hook
                        .lock()
                        .expect("hardening_path_lock")
                        .clone()
                        .expect("hardening_path_selected");
                    assert!(!path.exists(), "hardening_real_absent_interval");
                    let mut file = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)?;
                    std::io::Write::write_all(&mut file, independent)?;
                    file.sync_all()?;
                    writes_hook.fetch_add(1, Ordering::SeqCst);
                }
                Ok(())
            })),
            ..Hooks::default()
        };
        let prepared = fixture.prepared(hooks.clone());
        let id = prepared.bundle_id();
        let paths = prepared.test_paths();
        *selected.lock().expect("hardening_select_path") = Some(target(&paths));
        let approval = prepared.review().digest.clone();
        let result = if step == Step::OriginalMoved {
            prepared.apply_eula(&approval).map(|_| ())
        } else {
            prepared
                .apply_eula(&approval)
                .expect("hardening_apply_before_collision")
                .restore_before_spawn()
                .map(|_| ())
        };
        assert!(result.is_err());
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert_eq!(
            hooks.eula_renames.load(Ordering::SeqCst),
            if step == Step::OriginalMoved { 1 } else { 3 }
        );
        assert_eq!(bytes(&target(&paths)), independent);
        assert_eq!(bytes(&slot(&paths, "old.bin")), EULA_FALSE);
        assert_eq!(bytes(&slot(&paths, "snapshot.bin")), EULA_FALSE);
        assert_eq!(
            bytes(&slot(
                &paths,
                if step == Step::OriginalMoved {
                    "stage.bin"
                } else {
                    "applied.bin"
                }
            )),
            EULA_TRUE
        );
        denied_unchanged(&fixture, id, &paths);
    }
}

#[test]
fn untrusted_stage_metadata_is_not_authority_and_only_nine_resources_are_copied() {
    let fixture = Fixture::new();
    fs::write(
        fixture.stage.join("execution-plan.json"),
        br#"{"approved":true,"argv":["foreign.exe"],"eulaAccepted":true}"#,
    )
    .expect("hardening_untrusted_plan");
    fs::write(fixture.stage.join("eula.txt"), b"eula=true\n").expect("hardening_untrusted_eula");
    fs::write(
        fixture.stage.join("server.properties"),
        b"server-ip=0.0.0.0\n",
    )
    .expect("hardening_untrusted_properties");
    let before = files::tree(&fixture.stage);
    let prepared = fixture.prepared(Hooks::default());
    let paths = prepared.test_paths();
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    assert_eq!(
        bytes(&paths.copy.join("server.properties")),
        resources::PROPERTIES
    );
    assert!(!paths.copy.join("execution-plan.json").exists());
    for (name, expected) in resources::fixture_artifacts() {
        assert_eq!(bytes(&paths.copy.join(name)), expected);
    }
    assert_eq!(files::tree(&fixture.stage), before);
    drop(prepared);
    unlocked(&paths);
}

#[test]
fn unsafe_path_spellings_are_refused_before_any_factory_effect() {
    let fixture = Fixture::new();
    let before = files::tree(fixture.root.path());
    let invalid = [
        PathBuf::from("relative/prepared-Test84"),
        PathBuf::from(r"\\fixture.invalid\share\prepared-Test84"),
        PathBuf::from(format!(r"\\?\{}", fixture.stage.display())),
        fixture.lab.join("prepared-Test84:stream"),
        fixture.lab.join("prepared-Test84."),
        fixture.lab.join("prepared-Test84 "),
        fixture.lab.join("unused/../prepared-Test84"),
    ];
    for stage in invalid {
        assert_eq!(
            fixture_plan(&fixture.lab, &stage, &fixture.jdk, true, Hooks::default()).err(),
            Some(Error::UnsafePath)
        );
        assert_eq!(files::tree(fixture.root.path()), before);
    }
    let prepared = fixture.prepared(Hooks::default());
    assert_eq!(bytes(&target(&prepared.test_paths())), EULA_FALSE);
    drop(prepared);
}

#[test]
fn invalid_resources_jdk_and_resource_hardlinks_create_no_bundle() {
    for choice in [
        "jar",
        "jar-hardlink",
        "jdk-version",
        "jdk-platform",
        "jdk-duplicate",
        "jdk-duplicate-malformed",
        "jdk-executable",
    ] {
        let fixture = Fixture::new();
        match choice {
            "jar" => {
                fs::write(fixture.stage.join("server.jar"), b"wrong server")
                    .expect("hardening_wrong_server");
            }
            "jar-hardlink" => {
                fs::hard_link(
                    fixture.stage.join("server.jar"),
                    fixture.root.path().join("server-alias.jar"),
                )
                .expect("hardening_resource_hardlink");
            }
            "jdk-executable" => {
                fs::write(fixture.jdk.join("bin/java.exe"), b"not an executable")
                    .expect("hardening_non_executable");
            }
            _ => {
                let (original, _) = resources::fixture_jdk();
                let mut text = String::from_utf8(original).expect("hardening_release_utf8");
                text = match choice {
                    "jdk-version" => text.replace("21.0.1", "17.0.1"),
                    "jdk-platform" => text.replace("x86_64", "aarch64"),
                    "jdk-duplicate" => format!("{text}\nJAVA_VERSION=\"21.0.1\"\n"),
                    "jdk-duplicate-malformed" => format!("{text}\nJAVA_VERSION=malformed\n"),
                    _ => unreachable!(),
                };
                fs::write(fixture.jdk.join("release"), text).expect("hardening_release_change");
            }
        }
        let before = files::tree(fixture.root.path());
        assert!(fixture_plan(
            &fixture.lab,
            &fixture.stage,
            &fixture.jdk,
            true,
            Hooks::default()
        )
        .is_err());
        assert_eq!(files::tree(fixture.root.path()), before);
        assert_eq!(
            fs::read_dir(&fixture.lab)
                .expect("hardening_lab_entries")
                .count(),
            1
        );
    }
}

/// The script is constant; generated fixture paths travel only through env.
/// There is no shell interpolation and no access to personal process state.
struct Junction(PathBuf);

impl Junction {
    fn create(link: &Path, target: &Path) -> Self {
        use std::os::windows::{fs::MetadataExt, process::CommandExt};
        use std::process::{Command, Stdio};
        let guard = Self(link.to_path_buf());
        let executable =
            PathBuf::from(std::env::var_os("SystemRoot").expect("hardening_system_root"))
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let child = Command::new(executable)
            .creation_flags(0x08000000)
            .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:AURORA_EULA_QA_LINK -Target $env:AURORA_EULA_QA_TARGET | Out-Null"])
            .env("AURORA_EULA_QA_LINK", link)
            .env("AURORA_EULA_QA_TARGET", target)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .spawn().expect("hardening_junction_spawn");
        // Own the helper immediately, including timeout and assertion unwind.
        struct Process(Option<std::process::Child>);
        impl Drop for Process {
            fn drop(&mut self) {
                if let Some(mut child) = self.0.take() {
                    if !matches!(child.try_wait(), Ok(Some(_))) {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                }
            }
        }
        let mut process = Process(Some(child));
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = process
                .0
                .as_mut()
                .expect("hardening_helper_owned")
                .try_wait()
                .expect("hardening_helper_wait")
            {
                break status;
            }
            assert!(Instant::now() < deadline, "hardening_junction_timeout");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(status.success(), "hardening_junction_creation_must_succeed");
        let metadata = fs::symlink_metadata(link).expect("hardening_junction_metadata");
        assert_ne!(
            metadata.file_attributes() & 0x400,
            0,
            "hardening_real_reparse_point"
        );
        drop(process);
        guard
    }
}

impl Drop for Junction {
    fn drop(&mut self) {
        use std::os::windows::fs::MetadataExt;
        if fs::symlink_metadata(&self.0)
            .is_ok_and(|metadata| metadata.file_attributes() & 0x400 != 0)
        {
            // Remove only the reparse point, never recurse into its target.
            let removed = fs::remove_dir(&self.0);
            if !std::thread::panicking() {
                removed.expect("hardening_unlink_only_junction");
            }
        }
    }
}

#[test]
fn junction_stage_and_transaction_never_write_through_to_the_external_tree() {
    for transaction in [false, true] {
        let fixture = Fixture::new();
        let external = fixture.root.path().join("external-directory");
        if transaction {
            let (id, paths) = interrupted(&fixture, Step::AppliedMarked);
            // Only the PowerShell fixture needs an ordinary path spelling;
            // production may retain a pinned internal VerbatimDisk path.
            let link_path = fixture
                .lab
                .join(paths.bundle.file_name().expect("hardening_bundle_name"))
                .join("eula-transaction");
            assert_eq!(
                link_path.canonicalize().expect("hardening_link_spelling"),
                paths
                    .transaction
                    .canonicalize()
                    .expect("hardening_transaction_spelling")
            );
            fs::rename(&paths.transaction, &external).expect("hardening_park_transaction");
            // Keep the recognized slot set intact: adding a foreign sentinel
            // here would let unknown-entry rejection mask a missing link gate.
            assert_eq!(bytes(&external.join("snapshot.bin")), EULA_FALSE);
            let external_before = files::tree(&external);
            let link = Junction::create(&link_path, &external);
            denied_unchanged(&fixture, id, &paths);
            assert_eq!(files::tree(&external), external_before);
            drop(link);
            assert_eq!(files::tree(&external), external_before);
            assert!(!paths.transaction.exists());
        } else {
            fs::rename(&fixture.stage, &external).expect("hardening_park_stage");
            fs::write(
                external.join("outside-sentinel"),
                b"external contents remain intact",
            )
            .expect("hardening_sentinel");
            let external_before = files::tree(&external);
            let link = Junction::create(&fixture.stage, &external);
            let before = files::tree(fixture.root.path());
            assert!(fixture_plan(
                &fixture.lab,
                &fixture.stage,
                &fixture.jdk,
                true,
                Hooks::default()
            )
            .is_err());
            assert_eq!(files::tree(fixture.root.path()), before);
            assert_eq!(files::tree(&external), external_before);
            drop(link);
            assert_eq!(files::tree(&external), external_before);
            assert!(!fixture.stage.exists());
        }
    }
}
