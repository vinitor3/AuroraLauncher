use super::*;
use crate::engine::{
    acquire_instance_mutation_lock, install_embedded_core, install_embedded_skins, InstanceId,
    InstanceLayout,
};
use serde_json::json;
#[path = "connect_ready_runtime_fabric1201_tests.rs"]
mod fabric1201;
#[path = "connect_ready_runtime_matrix_tests.rs"]
mod matrix;
#[path = "connect_ready_runtime_readonly_tests.rs"]
mod readonly;

struct Fixture {
    root: PathBuf,
    source: Instance,
    copy: Instance,
    java: PathBuf,
    java_major: u32,
}
impl Fixture {
    fn write(root: &Path, relative: &str, bytes: &[u8]) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn new() -> Self {
        Self::with_profile(false)
    }
    fn fabric1201() -> Self {
        Self::with_profile(true)
    }
    fn with_profile(fabric1201: bool) -> Self {
        let (minecraft, version_id, java_major) = if fabric1201 {
            ("1.20.1", "fabric-loader-0.16.14-1.20.1", 17)
        } else {
            ("1.21.1", "fabric-loader-0.16.14-1.21.1", 21)
        };
        let samples: serde_json::Value =
            serde_json::from_str(include_str!("connect_status_profile_fixtures.json")).unwrap();
        let base = if fabric1201 {
            samples[5]["base"]["profile"].clone()
        } else {
            json!({"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","javaVersion":{"majorVersion":21},"assetIndex":{"id":"17"},"libraries":[{"name":"fixture:library:1.0"}],"arguments":{"game":[],"jvm":[]}})
        };
        let child = if fabric1201 {
            samples[5]["profile"].clone()
        } else {
            json!({"id":"fabric-loader-0.16.14-1.21.1","inheritsFrom":"1.21.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","libraries":[{"name":"net.fabricmc:fabric-loader:0.16.14"}],"arguments":{"game":[]}})
        };
        let asset_index = base["assetIndex"]["id"].as_str().unwrap();
        let root =
            std::env::temp_dir().join(format!("aurora-ready-runtime-{}", uuid::Uuid::new_v4()));
        let source = InstanceLayout::new(&root).open(InstanceId::parse("source-fixture").unwrap());
        source.ensure_layout().unwrap();
        Self::write(
            source.root(),
            "aurora-instance.json",
            &serde_json::to_vec(&json!({"minecraft":minecraft,"versionId":version_id})).unwrap(),
        );
        Self::write(source.root(), "config/fixture.json", br#"{"value":1}"#);
        Self::write(
            source.root(),
            &format!("versions/{minecraft}/{minecraft}.json"),
            &serde_json::to_vec(&base).unwrap(),
        );
        Self::write(
            source.root(),
            &format!("versions/{version_id}/{version_id}.json"),
            &serde_json::to_vec(&child).unwrap(),
        );
        Self::write(
            source.root(),
            &format!("versions/{minecraft}/{minecraft}.jar"),
            b"never executed fixture",
        );
        for relative in [
            "libraries/fixture/library/1.0/library-1.0.jar",
            "libraries/net/fabricmc/fabric-loader/0.16.14/fabric-loader-0.16.14.jar",
            "natives/lwjgl.dll",
        ] {
            Self::write(source.root(), relative, b"never executed fixture");
        }
        let bytes = b"fixture asset";
        let hash = format!("{:x}", sha1::Sha1::digest(bytes));
        Self::write(
            source.root(),
            &format!("assets/objects/{}/{hash}", &hash[..2]),
            bytes,
        );
        Self::write(
            source.root(),
            &format!("assets/indexes/{asset_index}.json"),
            &serde_json::to_vec(&json!({"objects":{"fixture":{"hash":hash,"size":bytes.len()}}}))
                .unwrap(),
        );
        let guard = acquire_instance_mutation_lock(&source).unwrap();
        let created = diagnostic_lab::create(&source, &guard, None, |_| {}).unwrap();
        drop(guard);
        let copy = InstanceLayout::new(&root).open(InstanceId::parse(created.id).unwrap());
        install_embedded_core(&copy, minecraft, "fabric").unwrap();
        install_embedded_skins(&copy, minecraft, "fabric").unwrap();
        let java = root.join("java/bin/java.exe");
        for relative in [
            "bin/java.exe",
            "release",
            "lib/modules",
            "bin/server/jvm.dll",
            "bin/java.dll",
            "bin/jli.dll",
        ] {
            Self::write(
                &root.join("java"),
                relative,
                b"never executed Java shape fixture",
            );
        }
        Self {
            root,
            source,
            copy,
            java,
            java_major,
        }
    }
    fn inspect(&self, instance: &Instance, source: &str) -> Result<ReadyRuntime, Error> {
        let guard = acquire_instance_mutation_lock(instance).unwrap();
        inspect_ready_runtime(
            instance,
            &guard,
            source,
            &self.java,
            None,
            "127.0.0.1:42010".parse().unwrap(),
            self.java_major,
            || false,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(all(windows, feature = "connect-lab"))]
#[test]
fn ready_runtime_install_owns_fresh_copy_and_keeps_unverified_install_unpublished() {
    let fixture = Fixture::new();
    let source_profile = fs::read(fixture.source.root().join("aurora-instance.json")).unwrap();
    let source_lock = acquire_instance_mutation_lock(&fixture.source).unwrap();
    let pending = diagnostic_lab::begin_runtime_copy(&fixture.source, &source_lock).unwrap();
    drop(source_lock);
    let copy_id = pending.instance().id().as_str().to_owned();
    let copy = InstanceLayout::new(&fixture.root).open(InstanceId::parse(&copy_id).unwrap());
    assert!(!copy.root().join("config/fixture.json").exists());
    assert!(!copy.root().join("versions/1.21.1/1.21.1.jar").exists());
    assert!(diagnostic_lab::ensure_launchable(&copy).is_err());
    assert!(
        verify_installed_copy_runtime(&copy, ReadyProfile::Fabric1211, &fixture.java, || false)
            .is_err()
    );
    drop(pending);
    let listed = diagnostic_lab::list(&fixture.root).unwrap();
    assert_eq!(
        listed.iter().find(|item| item.id == copy_id).unwrap().state,
        "failed"
    );
    let guard = crate::engine::acquire_instance_recovery_lock(&copy).unwrap();
    diagnostic_lab::delete(&copy, &guard).unwrap();
    assert!(!copy.root().exists());
    assert_eq!(
        fs::read(fixture.source.root().join("aurora-instance.json")).unwrap(),
        source_profile
    );
}

#[cfg(all(windows, feature = "connect-lab"))]
#[test]
#[ignore = "explicit official download and Java installer in an owned temporary copy; no Minecraft"]
fn ready_runtime_install_downloads_exact_official_profile() {
    use crate::engine::{connect_java_probe::probe_java_major, MinecraftInstaller};
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        },
        time::Duration,
    };
    let selected = std::env::var("AURORA_CONNECT_READY_INSTALL_PROFILE")
        .expect("explicit installed matrix version id required");
    let profile = ReadyProfile::from_version_id(&selected).expect("closed matrix profile");
    let runtime = profile.descriptor();
    let fixture = Fixture::new();
    let cancel = Arc::new(AtomicBool::new(false));
    let watchdog_cancel = cancel.clone();
    let (complete, stopped) = mpsc::channel::<()>();
    let watchdog = std::thread::spawn(move || {
        if stopped.recv_timeout(Duration::from_secs(20 * 60))
            == Err(mpsc::RecvTimeoutError::Timeout)
        {
            watchdog_cancel.store(true, Ordering::Release);
        }
    });
    let result = std::panic::catch_unwind(|| {
        let cancelled = || cancel.load(Ordering::Acquire);
        let source_lock = acquire_instance_mutation_lock(&fixture.source).unwrap();
        let pending = diagnostic_lab::begin_runtime_copy(&fixture.source, &source_lock).unwrap();
        drop(source_lock);
        println!("preparing Java {} for {selected}", runtime.java_major);
        let java =
            MinecraftInstaller::prepare_ready_copy_java(&pending, profile, cancelled, |_| {})
                .expect("official Java package and safe extraction");
        assert_eq!(
            probe_java_major(&java, cancelled).expect("bounded Java probe"),
            runtime.java_major
        );
        println!("installing {selected} inside the owned diagnostic copy");
        let installed = MinecraftInstaller::install_ready_copy(
            &pending,
            profile,
            &java,
            cancel.clone(),
            |_| {},
        )
        .expect("official exact-profile installer");
        let instance = pending.instance();
        install_embedded_core(instance, runtime.minecraft_version, runtime.loader).unwrap();
        install_embedded_skins(instance, runtime.minecraft_version, runtime.loader).unwrap();
        Fixture::write(instance.root(), "aurora-instance.json",
            &serde_json::to_vec_pretty(&json!({
                "minecraft": installed.minecraft_version, "versionId": installed.version_id, "source": runtime.loader,
            })).unwrap());
        let verified = verify_installed_copy_runtime(instance, profile, &java, cancelled)
            .expect("official runtime, managed artifacts and Java files");
        let environment = pending
            .publish(verified, cancelled)
            .expect("verified ready publication");
        assert_eq!(environment.state, "ready");
        assert_eq!(
            environment
                .runtime_java_executable
                .as_deref()
                .map(Path::new),
            Some(java.as_path())
        );
        let copy =
            InstanceLayout::new(&fixture.root).open(InstanceId::parse(&environment.id).unwrap());
        let guard = acquire_instance_mutation_lock(&copy).unwrap();
        let proof = inspect_ready_runtime(
            &copy,
            &guard,
            fixture.source.id().as_str(),
            &java,
            None,
            "127.0.0.1:42010".parse().unwrap(),
            runtime.java_major,
            cancelled,
        )
        .expect("complete ready-game inspection of the installed copy");
        assert_eq!(proof.profile, profile);
        proof.recheck_before_spawn().unwrap();
        println!("verified installed {selected} without spawning Minecraft");
    });
    drop(complete);
    watchdog.join().unwrap();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
#[test]
fn ready_voice_install_is_copy_only_and_uses_transactional_undo() {
    use std::cell::Cell;
    let f = Fixture::new();
    let additions = [
        ("mods/voice-a.jar", b"approved first fixture".as_slice()),
        ("mods/voice-b.jar", b"approved second fixture".as_slice()),
    ];
    let guard = acquire_instance_mutation_lock(&f.source).unwrap();
    assert!(super::super::content_updates::install_voice_mods(
        &f.source,
        &guard,
        &additions,
        || true,
    )
    .is_err());
    drop(guard);

    let guard = acquire_instance_mutation_lock(&f.copy).unwrap();
    let checks = Cell::new(0);
    assert!(matches!(
        super::super::content_updates::install_voice_mods(&f.copy, &guard, &additions, || {
            checks.set(checks.get() + 1);
            checks.get() <= 3
        }),
        Err(super::super::ContentUpdateError::VoiceInstallCancelled)
    ));
    assert_eq!(checks.get(), 4);
    assert!(!recovery::has_pending(&f.copy).unwrap());
    for (path, _) in additions {
        assert!(!f.copy.root().join(path).exists());
    }
    super::super::content_updates::install_voice_mods(&f.copy, &guard, &additions, || true)
        .unwrap();
    for (path, bytes) in additions {
        assert_eq!(fs::read(f.copy.root().join(path)).unwrap(), bytes);
        assert!(!f.source.root().join(path).exists());
    }
    drop(guard);
    super::super::content_updates::rollback_last_content_update(&f.copy).unwrap();
    for (path, _) in additions {
        assert!(!f.copy.root().join(path).exists());
    }
}

#[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
#[test]
fn ready_voice_install_plans_only_the_original_profile_and_rejects_conflicting_mod_ids() {
    use std::io::Write;
    let f = Fixture::new();
    let guard = acquire_instance_mutation_lock(&f.copy).unwrap();
    let inspect = || {
        inspect_ready_runtime_for_voice(
            &f.copy,
            &guard,
            "source-fixture",
            &f.java,
            None,
            "127.0.0.1:42010".parse().unwrap(),
            21,
            Some(NativeVoiceProfile::SimpleVoiceChat),
            || false,
        )
    };
    let proof = inspect().unwrap();
    let voice = proof.voice_review.as_ref().unwrap();
    assert_eq!(voice.profile, NativeVoiceProfile::SimpleVoiceChat);
    assert_eq!(voice.mods.len(), 1);
    assert_eq!(voice.mods[0].action, ReadyVoiceAction::Install);
    assert!(!f.copy.root().join(&voice.mods[0].relative).exists());
    drop(proof);
    let mut jar = zip::ZipWriter::new(Cursor::new(Vec::new()));
    jar.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    jar.write_all(br#"{"id":"another","provides":["plasmovoice"]}"#)
        .unwrap();
    Fixture::write(
        f.copy.root(),
        "mods/conflict.jar",
        &jar.finish().unwrap().into_inner(),
    );
    assert!(matches!(inspect(), Err(Error::ArtifactMismatch)));
}

#[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
#[test]
#[ignore = "fetches only the fixed client voice JARs into an isolated temp cache; no Java"]
fn ready_voice_install_uses_official_plasmo_artifacts_after_approval() {
    let f = Fixture::new();
    let guard = acquire_instance_mutation_lock(&f.copy).unwrap();
    let inspect = || {
        inspect_ready_runtime_for_voice(
            &f.copy,
            &guard,
            "source-fixture",
            &f.java,
            None,
            "127.0.0.1:42010".parse().unwrap(),
            21,
            Some(NativeVoiceProfile::PlasmoVoice),
            || false,
        )
    };
    let mut proof = inspect().unwrap();
    let staged = proof
        .prepare_voice_install(&f.root.join("app-data"), || false)
        .unwrap()
        .unwrap();
    assert_eq!(staged.review().mods.len(), 2);
    for file in &staged.review().mods {
        assert_eq!(file.action, ReadyVoiceAction::Install);
        assert!(!f.copy.root().join(&file.relative).exists());
    }
    staged.commit(&mut proof, &f.copy, &guard, || true).unwrap();
    proof.recheck_before_spawn().unwrap();
    drop(proof);
    let present = inspect().unwrap();
    assert!(present
        .voice_review
        .as_ref()
        .unwrap()
        .mods
        .iter()
        .all(|file| file.action == ReadyVoiceAction::Present));
    for file in &staged.review().mods {
        assert!(!f.source.root().join(&file.relative).exists());
    }
    drop(present);
    drop(guard);
    super::super::content_updates::rollback_last_content_update(&f.copy).unwrap();
    for file in &staged.review().mods {
        assert!(!f.copy.root().join(&file.relative).exists());
    }
}

#[test]
fn ready_runtime_verified_copy_is_readonly_and_pins_effective_inputs_through_spawn_boundary() {
    let f = Fixture::new();
    let before = fs::read(f.source.root().join("aurora-instance.json")).unwrap();
    let proof = f.inspect(&f.copy, "source-fixture").unwrap();
    assert_eq!(
        proof.spec.game_args,
        ["--quickPlayMultiplayer", "127.0.0.1:42010"]
    );
    assert_eq!(proof.core_sha256.len(), 64);
    assert_eq!(proof.skins_sha256.len(), 64);
    for path in [
        f.copy.root().join("versions/1.21.1/1.21.1.jar"),
        f.copy.root().join("config/fixture.json"),
        f.java.clone(),
    ] {
        assert!(
            fs::OpenOptions::new().write(true).open(path).is_err(),
            "ready evidence denies external writer while owned"
        );
    }
    let digest = proof.fingerprint.clone();
    drop(proof);
    let second = f.inspect(&f.copy, "source-fixture").unwrap();
    assert_eq!(digest, second.fingerprint);
    drop(second);
    assert_eq!(
        fs::read(f.source.root().join("aurora-instance.json")).unwrap(),
        before
    );
}

#[test]
fn ready_runtime_refuses_origin_wrong_owner_missing_artifacts_and_modified_assets() {
    let f = Fixture::new();
    assert!(matches!(
        f.inspect(&f.source, "source-fixture"),
        Err(Error::InstanceNotReady)
    ));
    assert!(matches!(
        f.inspect(&f.copy, "different-source"),
        Err(Error::InstanceNotReady)
    ));
    let core = f.copy.root().join("mods/aurora-core-fabric-1.21.1.jar");
    let bytes = fs::read(&core).unwrap();
    fs::remove_file(&core).unwrap();
    assert!(matches!(
        f.inspect(&f.copy, "source-fixture"),
        Err(Error::MissingArtifact)
    ));
    fs::write(&core, bytes).unwrap();
    let index: serde_json::Value =
        serde_json::from_slice(&fs::read(f.copy.root().join("assets/indexes/17.json")).unwrap())
            .unwrap();
    let hash = index["objects"]["fixture"]["hash"].as_str().unwrap();
    fs::write(
        f.copy
            .root()
            .join(format!("assets/objects/{}/{hash}", &hash[..2])),
        b"changed asset",
    )
    .unwrap();
    assert!(matches!(
        f.inspect(&f.copy, "source-fixture"),
        Err(Error::ArtifactMismatch)
    ));
}

#[test]
fn ready_runtime_revision_changes_after_java_or_configuration_change_and_links_are_refused() {
    let f = Fixture::new();
    let first = f.inspect(&f.copy, "source-fixture").unwrap();
    let digest = first.fingerprint.clone();
    drop(first);
    fs::write(&f.java, b"changed Java fixture").unwrap();
    let second = f.inspect(&f.copy, "source-fixture").unwrap();
    assert_ne!(digest, second.fingerprint);
    let second_digest = second.fingerprint.clone();
    drop(second);
    fs::write(f.copy.root().join("config/fixture.json"), br#"{"value":2}"#).unwrap();
    let third = f.inspect(&f.copy, "source-fixture").unwrap();
    assert_ne!(second_digest, third.fingerprint);
    drop(third);
    fs::hard_link(
        f.copy.root().join("versions/1.21.1/1.21.1.jar"),
        f.root.join("alias.jar"),
    )
    .unwrap();
    assert!(f.inspect(&f.copy, "source-fixture").is_err());
}

#[test]
fn ready_runtime_cancellation_stops_before_any_read_or_write() {
    let f = Fixture::new();
    let guard = acquire_instance_mutation_lock(&f.copy).unwrap();
    assert!(matches!(
        inspect_ready_runtime(
            &f.copy,
            &guard,
            "source-fixture",
            &f.java,
            None,
            "127.0.0.1:42010".parse().unwrap(),
            21,
            || true
        ),
        Err(Error::Cancelled)
    ));
}

#[test]
fn ready_runtime_final_scan_rejects_new_mod_native_and_delete_marker() {
    let f = Fixture::new();
    let proof = f.inspect(&f.copy, "source-fixture").unwrap();
    assert!(proof.recheck_before_spawn().is_ok());
    for relative in ["mods/late.jar", "natives/late.dll"] {
        let path = f.copy.root().join(relative);
        fs::write(&path, b"unreviewed entry").unwrap();
        assert!(matches!(
            proof.recheck_before_spawn(),
            Err(Error::InstanceChanged)
        ));
        fs::remove_file(path).unwrap();
        assert!(proof.recheck_before_spawn().is_ok());
    }
    let marker = f.root.join(format!(
        ".aurora/diagnostic-lab/{}/deleting.json",
        f.copy.id().as_str().strip_prefix("aurora-test-").unwrap()
    ));
    fs::write(marker, b"fixture deletion marker").unwrap();
    assert!(matches!(
        proof.recheck_before_spawn(),
        Err(Error::InstanceChanged)
    ));
}
