use super::*;
use crate::engine::{acquire_instance_recovery_lock, InstanceId, InstanceLayout};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

const SVC: &[u8] = b"# Inspection fixture\r\nport=24454\r\nvoice_host=old.example.invalid:24454\r\nmtu_size=1275\r\n";
const PLASMO: &[u8] = b"# Inspection fixture\r\n[host]\r\nip = '127.0.0.1'\r\nport = 25565\r\n[host.public]\r\nip = 'old.example.invalid'\r\nport = 25565\r\n[voice]\r\nmtu_size = 1024\r\n";
const PLASMO_ABSENT: &[u8] = b"# Inspection fixture\r\n[host]\r\nip = '127.0.0.1'\r\nport = 25565\r\n[voice]\r\nmtu_size = 1024\r\n";

struct Fixture {
    root: PathBuf,
    source: Instance,
    copy: Instance,
    voice_mod: VoiceMod,
}
impl Fixture {
    fn new(voice_mod: VoiceMod, original: &[u8], pinned: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("aurora-connect-inspection-{}", Uuid::new_v4()));
        let layout = InstanceLayout::new(&root);
        let source = layout.open(InstanceId::parse("fixture-source").unwrap());
        source.ensure_layout().unwrap();
        let config = source.root().join(voice_mod.config());
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(config, original).unwrap();
        fs::write(
            source.root().join("aurora-instance.json"),
            br#"{"minecraft":"1.21.1","loader":"fabric","versionId":"1.21.1"}"#,
        )
        .unwrap();
        if pinned {
            let artifacts = PathBuf::from(
                std::env::var_os("AURORA_CONNECT_ARTIFACT_DIR")
                    .expect("explicit pinned local artifacts required; never downloaded by tests"),
            );
            let jar = artifacts.join(match voice_mod {
                VoiceMod::SimpleVoiceChat => "voicechat-fabric-1.21.1-2.6.22.jar",
                VoiceMod::PlasmoVoice => "plasmovoice-fabric-1.21.1-2.1.17.jar",
            });
            let (hash, size) = voice_mod.artifact();
            assert_eq!(recovery::hash_file(&jar).unwrap(), (hash.to_owned(), size));
            fs::copy(jar, source.root().join("mods/voice.jar")).unwrap();
        } else {
            fs::write(
                source.root().join("mods/voice.jar"),
                b"unapproved fixture artifact",
            )
            .unwrap();
        }
        let guard = acquire_instance_recovery_lock(&source).unwrap();
        let copy = diagnostic_lab::create(&source, &guard, None, |_| {}).unwrap();
        drop(guard);
        Self {
            root,
            source,
            copy: layout.open(InstanceId::parse(copy.id).unwrap()),
            voice_mod,
        }
    }
    fn synthetic() -> Self {
        Self::new(VoiceMod::SimpleVoiceChat, SVC, false)
    }
    fn database(&self) -> PathBuf {
        self.root.join(".aurora/recovery/index.sqlite3")
    }
    fn config(&self) -> PathBuf {
        self.copy.root().join(self.voice_mod.config())
    }
    fn guarded_inspection(&self, guard: &VoiceMutationGuard) -> Result<AnnouncementInspection> {
        // Snapshot includes the shared DB, CAS, source, copy, markers and lock
        // files. Acquire the real OS lock first: acquisition is not inspection.
        let before = snapshot(&self.root);
        let db_before = fs::read(self.database()).ok();
        let result = inspect_announcement(&self.copy, guard, self.voice_mod);
        assert_eq!(
            snapshot(&self.root),
            before,
            "inspection changed its fixture tree"
        );
        assert_eq!(
            fs::read(self.database()).ok(),
            db_before,
            "DB bytes changed"
        );
        result
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let (Ok(root), Ok(temp)) = (
            fs::canonicalize(&self.root),
            fs::canonicalize(std::env::temp_dir()),
        ) else {
            return;
        };
        let valid_name = self
            .root
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("aurora-connect-inspection-"))
            .is_some_and(|s| Uuid::parse_str(s).is_ok());
        if root.is_absolute()
            && root.parent() == Some(temp.as_path())
            && root.file_name() == self.root.file_name()
            && valid_name
        {
            // This canonical, directly-owned UUID fixture is the only target.
            let _ = fs::remove_dir_all(root);
        }
    }
}

fn snapshot(root: &Path) -> BTreeMap<String, (u64, String)> {
    fn walk(root: &Path, path: &Path, result: &mut BTreeMap<String, (u64, String)>) {
        let meta = fs::symlink_metadata(path).unwrap();
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if meta.file_type().is_symlink() {
            result.insert(
                format!("link:{rel}"),
                (
                    0,
                    fs::read_link(path).unwrap().to_string_lossy().into_owned(),
                ),
            );
        } else if meta.is_dir() {
            result.insert(format!("dir:{rel}"), (0, String::new()));
            for entry in fs::read_dir(path).unwrap() {
                walk(root, &entry.unwrap().path(), result);
            }
        } else {
            // A Windows byte-range lock can refuse even read-to-EOF on an
            // empty lock file. Its observed zero length proves empty content;
            // keep it in the tree oracle without attempting that locked read.
            let bytes = if meta.len() == 0 {
                Vec::new()
            } else {
                fs::read(path).unwrap()
            };
            result.insert(format!("file:{rel}"), (bytes.len() as u64, digest(&bytes)));
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}
fn guard(instance: &Instance) -> VoiceMutationGuard {
    process::acquire_with_probe(instance, Box::new(|| Ok(()))).unwrap()
}
fn controlled_guard(
    instance: &Instance,
) -> (VoiceMutationGuard, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let refuse_at = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let refuse = refuse_at.clone();
    let guard = process::acquire_with_probe(
        instance,
        Box::new(move || {
            let count = seen.fetch_add(1, Ordering::SeqCst) + 1;
            if count == refuse.load(Ordering::SeqCst) {
                Err(RecoveryError::VoiceProcessRunning)
            } else {
                Ok(())
            }
        }),
    )
    .unwrap();
    (guard, calls, refuse_at)
}

#[test]
fn non_lab_selection_is_refused_before_process_or_missing_database_reads() {
    let f = Fixture::synthetic();
    let (guard, calls, refuse_at) = controlled_guard(&f.source);
    let observed = calls.load(Ordering::SeqCst);
    refuse_at.store(observed + 1, Ordering::SeqCst);
    fs::remove_file(f.database()).unwrap();
    let before = snapshot(&f.root);
    assert!(matches!(
        inspect_announcement(&f.source, &guard, VoiceMod::SimpleVoiceChat),
        Err(RecoveryError::UnsafePath)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), observed);
    assert_eq!(snapshot(&f.root), before);
    assert!(!f.database().exists());
}

#[test]
fn missing_identity_or_database_is_not_initialized_by_public_inspection() {
    for remove_database in [false, true] {
        let f = Fixture::synthetic();
        let guard = guard(&f.copy);
        let removed = if remove_database {
            f.database()
        } else {
            f.copy.root().join(".aurora/recovery-identity.json")
        };
        fs::remove_file(removed).unwrap();
        assert!(matches!(
            f.guarded_inspection(&guard),
            Err(RecoveryError::Identity)
        ));
    }
}

#[test]
fn each_sqlite_sidecar_is_refused_without_cleaning_or_initializing_it() {
    for suffix in ["-journal", "-wal", "-shm"] {
        let f = Fixture::synthetic();
        let guard = guard(&f.copy);
        fs::write(
            f.root
                .join(format!(".aurora/recovery/index.sqlite3{suffix}")),
            b"fixture sidecar, do not clean",
        )
        .unwrap();
        assert!(matches!(
            f.guarded_inspection(&guard),
            Err(RecoveryError::Pending)
        ));
    }
}

#[test]
fn wal_mode_without_sidecars_is_refused_before_any_transitive_sqlite_open() {
    let f = Fixture::synthetic();
    let guard = guard(&f.copy);
    let db = rusqlite::Connection::open(f.database()).unwrap();
    let mode: String = db
        .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    drop(db);
    assert!(!f.root.join(".aurora/recovery/index.sqlite3-wal").exists());
    assert!(!f.root.join(".aurora/recovery/index.sqlite3-shm").exists());
    assert!(matches!(
        f.guarded_inspection(&guard),
        Err(RecoveryError::Storage)
    ));
}

#[test]
fn pending_schema_and_identity_fail_closed_without_recovery_writes() {
    for case in ["pending", "schema", "identity"] {
        let f = Fixture::synthetic();
        let guard = guard(&f.copy);
        let identity = recovery::identity_lookup(&f.copy).unwrap().unwrap();
        let db = rusqlite::Connection::open(f.database()).unwrap();
        match case {
            "pending" => {
                db.execute(
                    "INSERT INTO journals(instance_uuid,data) VALUES(?1,?2)",
                    rusqlite::params![identity, b"fixture pending".as_slice()],
                )
                .unwrap();
            }
            "schema" => {
                db.execute_batch("PRAGMA user_version=2").unwrap();
            }
            "identity" => {
                db.execute(
                    "UPDATE instances SET instance_uuid=?1 WHERE instance_uuid=?2",
                    rusqlite::params![Uuid::new_v4().to_string(), identity],
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        drop(db);
        let result = f.guarded_inspection(&guard);
        assert!(match case {
            "pending" => matches!(result, Err(RecoveryError::Pending)),
            "schema" => matches!(result, Err(RecoveryError::Integrity)),
            "identity" => matches!(result, Err(RecoveryError::Identity)),
            _ => unreachable!(),
        });
    }
}

#[test]
fn unapproved_artifact_wrong_guard_and_observed_java_never_produce_a_plan() {
    let f = Fixture::synthetic();
    let (copy_guard, calls, refuse_at) = controlled_guard(&f.copy);
    assert!(matches!(
        f.guarded_inspection(&copy_guard),
        Err(RecoveryError::Compatibility)
    ));
    let other = Fixture::synthetic();
    let wrong_guard = guard(&other.copy);
    assert!(matches!(
        f.guarded_inspection(&wrong_guard),
        Err(RecoveryError::WrongLock)
    ));
    refuse_at.store(calls.load(Ordering::SeqCst) + 1, Ordering::SeqCst);
    assert!(matches!(
        f.guarded_inspection(&copy_guard),
        Err(RecoveryError::VoiceProcessRunning)
    ));
}

#[test]
fn reader_detects_change_between_inventories_and_preserves_existing_identity_lookup() {
    let f = Fixture::synthetic();
    let guard = guard(&f.copy);
    let identity = recovery::identity_lookup(&f.copy).unwrap().unwrap();
    let before = snapshot(&f.root);
    let changed = b"voice_host=changed.example.invalid\r\n";
    let result = recovery::with_existing_observation(&f.copy, guard.inner(), |current| {
        assert_eq!(current.instance_uuid, identity);
        assert_eq!(
            recovery::identity_lookup(&f.copy)?.as_deref(),
            Some(identity.as_str())
        );
        // Deliberate fixture edit is the only permitted difference. This tests
        // the actual shared reader callback without adding production hooks.
        fs::write(f.config(), changed)?;
        Ok(())
    });
    assert!(matches!(result, Err(RecoveryError::Changed)));
    let mut expected = before;
    let relative = f
        .config()
        .strip_prefix(&f.root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    expected.insert(
        format!("file:{relative}"),
        (changed.len() as u64, digest(changed)),
    );
    assert_eq!(snapshot(&f.root), expected);
    assert_eq!(
        recovery::identity_lookup(&f.copy).unwrap().as_deref(),
        Some(identity.as_str())
    );
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn pinned_application_inspection_reports_only_declared_local_fields_without_writes() {
    for (voice_mod, config) in [
        (VoiceMod::SimpleVoiceChat, SVC),
        (VoiceMod::PlasmoVoice, PLASMO),
        (VoiceMod::PlasmoVoice, PLASMO_ABSENT),
    ] {
        let f = Fixture::new(voice_mod, config, true);
        let guard = guard(&f.copy);
        let observation = f.guarded_inspection(&guard).unwrap();
        assert_eq!(observation.source_sha256, digest(config));
        assert_eq!(observation.source_bytes, config.len() as u64);
        assert_eq!(observation.config_path, voice_mod.config());
        assert_eq!(
            (observation.artifact_sha256, observation.artifact_bytes),
            voice_mod.artifact()
        );
        match (&observation.announcement, voice_mod) {
            (ObservedAnnouncement::SimpleVoiceChat { voice_host }, VoiceMod::SimpleVoiceChat) => {
                assert_eq!(voice_host, "old.example.invalid:24454")
            }
            (ObservedAnnouncement::PlasmoPublic { ip, port }, VoiceMod::PlasmoVoice) => {
                assert_eq!(ip, "old.example.invalid");
                assert_eq!(*port, 25565);
            }
            (ObservedAnnouncement::PlasmoAbsent, VoiceMod::PlasmoVoice) => {
                assert_eq!(config, PLASMO_ABSENT)
            }
            _ => panic!("wrong selected-mod projection"),
        }
        let json = serde_json::to_value(&observation).unwrap();
        let mut fields: Vec<_> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            [
                "announcement",
                "artifactBytes",
                "artifactSha256",
                "artifactVersion",
                "configPath",
                "limitations",
                "sourceBytes",
                "sourceSha256",
                "voiceMod"
            ]
        );
        let encoded = serde_json::to_string(&observation).unwrap();
        for forbidden in [
            "Inspection fixture",
            "mtu_size",
            "24454\\r",
            "fixture-source",
            "recovery-identity",
            "planId",
            "expiresAt",
            "receipt",
        ] {
            assert!(!encoded.contains(forbidden));
        }
        assert!(observation
            .limitations
            .contains(&"voice_audio_not_verified"));
        assert_eq!(
            fs::read(f.source.root().join(voice_mod.config())).unwrap(),
            config
        );
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn missing_or_ambiguous_configs_are_refused_but_absent_empty_zero_and_applied_values_are_observable(
) {
    for (voice_mod, initial, accepted, expected) in [
        (
            VoiceMod::SimpleVoiceChat,
            SVC,
            b"voice_host=\r\n".as_slice(),
            serde_json::json!({"type":"simpleVoiceChat","voiceHost":""}),
        ),
        (
            VoiceMod::SimpleVoiceChat,
            SVC,
            b"voice_host=127.0.0.1:35554\r\n".as_slice(),
            serde_json::json!({"type":"simpleVoiceChat","voiceHost":"127.0.0.1:35554"}),
        ),
        (
            VoiceMod::PlasmoVoice,
            PLASMO,
            b"[host]\n[host.public]\nip='127.0.0.1'\nport=0\n".as_slice(),
            serde_json::json!({"type":"plasmoPublic","ip":"127.0.0.1","port":0}),
        ),
        (
            VoiceMod::PlasmoVoice,
            PLASMO,
            b"[host]\n[host.public]\nip='127.0.0.1'\nport=35554\n".as_slice(),
            serde_json::json!({"type":"plasmoPublic","ip":"127.0.0.1","port":35554}),
        ),
    ] {
        let f = Fixture::new(voice_mod, initial, true);
        let guard = guard(&f.copy);
        fs::write(f.config(), accepted).unwrap();
        let observation = f.guarded_inspection(&guard).unwrap();
        assert_eq!(
            serde_json::to_value(observation.announcement).unwrap(),
            expected
        );
        fs::remove_file(f.config()).unwrap();
        assert!(matches!(
            f.guarded_inspection(&guard),
            Err(RecoveryError::UnsafePath)
        ));
        let ambiguous = match voice_mod {
            VoiceMod::SimpleVoiceChat => b"voice_host=a\nvoice_host=b\n".as_slice(),
            VoiceMod::PlasmoVoice => b"[host]\n[host.public]\nip='localhost'\n".as_slice(),
        };
        fs::write(f.config(), ambiguous).unwrap();
        assert!(matches!(
            f.guarded_inspection(&guard),
            Err(RecoveryError::Integrity)
        ));
        if matches!(voice_mod, VoiceMod::PlasmoVoice) {
            fs::write(f.config(), b"[host]\n[host.public]\nip=''\nport=0\n").unwrap();
            assert!(matches!(
                f.guarded_inspection(&guard),
                Err(RecoveryError::Integrity)
            ));
        }
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn application_pin_runtime_duplicate_link_and_diagnostic_provenance_are_revalidated() {
    for (voice_mod, config) in [
        (VoiceMod::SimpleVoiceChat, SVC),
        (VoiceMod::PlasmoVoice, PLASMO),
    ] {
        for case in [
            "runtime",
            "pin",
            "duplicate",
            "config_link",
            "not_ready",
            "source",
        ] {
            let f = Fixture::new(voice_mod, config, true);
            let guard = guard(&f.copy);
            match case {
                "runtime" => fs::write(
                    f.copy.root().join("aurora-instance.json"),
                    br#"{"minecraft":"1.20.1","loader":"fabric","versionId":"1.20.1"}"#,
                )
                .unwrap(),
                "pin" => fs::write(
                    f.copy.root().join("mods/voice.jar"),
                    b"not the approved artifact",
                )
                .unwrap(),
                "duplicate" => {
                    fs::copy(
                        f.copy.root().join("mods/voice.jar"),
                        f.copy.root().join("mods/duplicate.jar"),
                    )
                    .unwrap();
                }
                "config_link" => {
                    fs::hard_link(f.config(), f.copy.root().join("fixture-config-link")).unwrap();
                }
                "not_ready" => fs::remove_file(
                    f.root
                        .join(".aurora/diagnostic-lab")
                        .join(
                            Uuid::parse_str(
                                f.copy.id().as_str().strip_prefix("aurora-test-").unwrap(),
                            )
                            .unwrap()
                            .to_string(),
                        )
                        .join("ready.json"),
                )
                .unwrap(),
                "source" => {}
                _ => unreachable!(),
            }
            if case == "source" {
                let source_guard = guard_for_source(&f);
                let before = snapshot(&f.root);
                assert!(matches!(
                    inspect_announcement(&f.source, &source_guard, voice_mod),
                    Err(RecoveryError::UnsafePath)
                ));
                assert_eq!(snapshot(&f.root), before);
            } else {
                assert!(f.guarded_inspection(&guard).is_err());
            }
        }
    }
}
fn guard_for_source(f: &Fixture) -> VoiceMutationGuard {
    guard(&f.source)
}

fn recovery_entry_counts(f: &Fixture) -> (u64, u64, u64) {
    let db = rusqlite::Connection::open_with_flags(
        f.database(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    db.query_row("SELECT (SELECT COUNT(*) FROM points), (SELECT COUNT(*) FROM previews), (SELECT COUNT(*) FROM journals)", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap()
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn process_is_rechecked_after_the_last_inventory_and_inspection_does_not_replace_future_review() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, SVC, true);
    let (guard, calls, refuse_at) = controlled_guard(&f.copy);
    refuse_at.store(calls.load(Ordering::SeqCst) + 2, Ordering::SeqCst);
    assert!(matches!(
        f.guarded_inspection(&guard),
        Err(RecoveryError::VoiceProcessRunning)
    ));
    refuse_at.store(0, Ordering::SeqCst);
    let observation = f.guarded_inspection(&guard).unwrap();
    fs::write(f.config(), b"voice_host=new.example.invalid\r\n").unwrap();
    let mut before = snapshot(&f.root);
    let before_entries = recovery_entry_counts(&f);
    assert!(matches!(
        prepare_announcement(
            &f.copy,
            &guard,
            AnnouncementRequest {
                voice_mod: VoiceMod::SimpleVoiceChat,
                mode: AnnouncementMode::DedicatedAuroraOnly,
                guest_port: 35554,
                expected_source_sha256: observation.source_sha256,
            }
        ),
        Err(RecoveryError::StalePreview)
    ));
    // This is the legacy preparation, not the read-only inspector. Store::open
    // there can change SQLite bookkeeping even for a rejected old hash. Check
    // that it created no point/preview/journal and changed no other file.
    assert_eq!(recovery_entry_counts(&f), before_entries);
    let mut after = snapshot(&f.root);
    assert!(before
        .remove("file:.aurora/recovery/index.sqlite3")
        .is_some());
    assert!(after
        .remove("file:.aurora/recovery/index.sqlite3")
        .is_some());
    assert_eq!(after, before);
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; no Java or download"]
fn actual_applied_announcements_remain_inspectable_without_recreating_authority() {
    for (voice_mod, config) in [
        (VoiceMod::SimpleVoiceChat, SVC),
        (VoiceMod::PlasmoVoice, PLASMO),
        (VoiceMod::PlasmoVoice, PLASMO_ABSENT),
    ] {
        let f = Fixture::new(voice_mod, config, true);
        let guard = guard(&f.copy);
        let prepared = prepare_announcement(
            &f.copy,
            &guard,
            AnnouncementRequest {
                voice_mod,
                mode: AnnouncementMode::DedicatedAuroraOnly,
                guest_port: 35554,
                expected_source_sha256: digest(config),
            },
        )
        .unwrap();
        let approved = prepared.review().digest.clone();
        let applied = prepared.confirm(&f.copy, &guard, &approved).unwrap();
        let observation = f.guarded_inspection(&guard).unwrap();
        match observation.announcement {
            ObservedAnnouncement::SimpleVoiceChat { voice_host } => {
                assert_eq!(voice_host, "127.0.0.1:35554")
            }
            ObservedAnnouncement::PlasmoPublic { ip, port } => {
                assert_eq!(ip, "127.0.0.1");
                assert_eq!(port, 35554);
            }
            ObservedAnnouncement::PlasmoAbsent => panic!("applied public fields absent"),
        }
        let before_drop = snapshot(&f.root);
        drop(applied); // Memory-only receipt: inspection cannot recreate it.
        assert_eq!(snapshot(&f.root), before_drop);
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; no Java or download"]
fn selected_field_and_config_are_bounded_without_truncation() {
    let f = Fixture::new(VoiceMod::SimpleVoiceChat, SVC, true);
    let guard = guard(&f.copy);
    fs::write(f.config(), format!("voice_host={}\n", "x".repeat(1024))).unwrap();
    assert!(f.guarded_inspection(&guard).is_ok());
    fs::write(f.config(), format!("voice_host={}\n", "x".repeat(1025))).unwrap();
    assert!(matches!(
        f.guarded_inspection(&guard),
        Err(RecoveryError::Limit)
    ));
    fs::write(
        f.config(),
        format!("#{}\nvoice_host=\n", "x".repeat(MAX_CONFIG as usize)),
    )
    .unwrap();
    assert!(matches!(
        f.guarded_inspection(&guard),
        Err(RecoveryError::Limit)
    ));
}
