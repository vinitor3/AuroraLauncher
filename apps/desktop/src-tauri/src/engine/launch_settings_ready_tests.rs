use super::*;
use crate::engine::{
    acquire_instance_mutation_lock, InstanceId, InstanceLayout, JavaRuntime, LaunchIdentity,
    LauncherEngine, VersionLaunchSpec,
};
use std::collections::BTreeMap;

struct Fixture {
    root: PathBuf,
    source: Instance,
    copy: Instance,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("aurora-ready-settings-{}", uuid::Uuid::new_v4()));
        let layout = InstanceLayout::new(&root);
        let source = layout.open(InstanceId::parse("source").unwrap());
        let copy = layout
            .open(InstanceId::parse(format!("aurora-test-{}", uuid::Uuid::new_v4())).unwrap());
        source.ensure_layout().unwrap();
        copy.ensure_layout().unwrap();
        Self { root, source, copy }
    }
    fn write(instance: &Instance, settings: &LaunchSettings) {
        fs::create_dir_all(instance.root().join(".aurora")).unwrap();
        fs::write(
            instance.root().join(SETTINGS_PATH),
            serde_json::to_vec(settings).unwrap(),
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
fn ready_settings_use_only_selected_copy_and_bind_absence_before_confirmation() {
    let fixture = Fixture::new();
    Fixture::write(
        &fixture.source,
        &LaunchSettings {
            max_memory_mb: 1024,
            ..LaunchSettings::default()
        },
    );
    let guard = acquire_instance_mutation_lock(&fixture.copy).unwrap();
    let absent = inspect_with_memory(&fixture.copy, &guard, 21, 0, Some(8192)).unwrap();
    assert_eq!(absent.effective().max_memory_mb, 4096);
    let original = absent.bind_runtime_fingerprint("original-runtime");
    assert!(!fixture.copy.root().join(SETTINGS_PATH).exists());
    assert_ne!(original, absent.bind_runtime_fingerprint("changed-runtime"));
    Fixture::write(&fixture.copy, &LaunchSettings::default());
    assert!(absent.recheck_before_spawn().is_err());
    drop(absent);
    let explicit = inspect_with_memory(&fixture.copy, &guard, 21, 0, Some(8192)).unwrap();
    assert_ne!(
        original,
        explicit.bind_runtime_fingerprint("original-runtime")
    );
    assert_eq!(read(&fixture.source).unwrap().max_memory_mb, 1024);
}

#[test]
fn ready_settings_pin_exact_bytes_and_reject_changed_invalid_or_aliased_preferences() {
    let fixture = Fixture::new();
    Fixture::write(&fixture.copy, &LaunchSettings::default());
    let guard = acquire_instance_mutation_lock(&fixture.copy).unwrap();
    let observed = inspect_with_memory(&fixture.copy, &guard, 21, 0, Some(8192)).unwrap();
    let before = observed.bind_runtime_fingerprint("runtime");
    let path = fixture.copy.root().join(SETTINGS_PATH);
    assert!(fs::write(&path, b"{}").is_err());
    assert!(fs::rename(&path, path.with_extension("old")).is_err());
    observed.recheck_before_spawn().unwrap();
    drop(observed);
    let bytes = fs::read(&path).unwrap();
    fs::write(&path, [bytes.as_slice(), b"\n"].concat()).unwrap();
    let changed = inspect_with_memory(&fixture.copy, &guard, 21, 0, Some(8192)).unwrap();
    assert_ne!(before, changed.bind_runtime_fingerprint("runtime"));
    drop(changed);
    fs::write(&path, br#"{"schemaVersion":999}"#).unwrap();
    assert!(inspect_with_memory(&fixture.copy, &guard, 21, 0, Some(8192)).is_err());
    Fixture::write(&fixture.copy, &LaunchSettings::default());
    let alias = fixture.root.join("unrelated-settings.json");
    fs::hard_link(&path, &alias).unwrap();
    assert!(inspect_with_memory(&fixture.copy, &guard, 21, 0, Some(8192)).is_err());
    assert_eq!(fs::read(&path).unwrap(), fs::read(alias).unwrap());
}

#[test]
fn ready_settings_auto_counts_reviewed_voice_additions_without_mutating_preferences() {
    let fixture = Fixture::new();
    let settings = LaunchSettings {
        memory_mode: MemoryMode::Automatic,
        gc_mode: GcMode::Recommended,
        ..LaunchSettings::default()
    };
    Fixture::write(&fixture.copy, &settings);
    for index in 0..100 {
        fs::write(
            fixture.copy.mods_dir().join(format!("fixture-{index}.jar")),
            b"inert",
        )
        .unwrap();
    }
    let guard = acquire_instance_mutation_lock(&fixture.copy).unwrap();
    let before = inspect_with_memory(&fixture.copy, &guard, 17, 2, Some(8192)).unwrap();
    assert_eq!(before.effective().max_memory_mb, 102 * 48);
    assert!(before.effective().jvm_args.contains(&"-XX:+UseG1GC".into()));
    let fingerprint = before.bind_runtime_fingerprint("runtime");
    for index in 100..102 {
        fs::write(
            fixture.copy.mods_dir().join(format!("fixture-{index}.jar")),
            b"inert",
        )
        .unwrap();
    }
    let after = inspect_with_memory(&fixture.copy, &guard, 17, 0, Some(8192)).unwrap();
    assert_eq!(fingerprint, after.bind_runtime_fingerprint("runtime"));
    assert_eq!(read(&fixture.copy).unwrap(), settings);
    assert!(inspect_with_memory(&fixture.copy, &guard, 17, 0, None).is_err());
    assert!(inspect_with_memory(&fixture.copy, &guard, 17, 10_000, Some(8192)).is_err());
}

#[test]
fn ready_settings_require_the_copy_guard_and_feed_exact_heap_into_connect_command() {
    let fixture = Fixture::new();
    let source_guard = acquire_instance_mutation_lock(&fixture.source).unwrap();
    assert!(inspect_with_memory(&fixture.copy, &source_guard, 21, 0, Some(8192)).is_err());
    Fixture::write(
        &fixture.copy,
        &LaunchSettings {
            min_memory_mb: Some(1024),
            max_memory_mb: 6144,
            gc_mode: GcMode::Recommended,
            jvm_args: vec!["-Xss1M".into()],
            ..LaunchSettings::default()
        },
    );
    let guard = acquire_instance_mutation_lock(&fixture.copy).unwrap();
    let observed = inspect_with_memory(&fixture.copy, &guard, 21, 0, Some(8192)).unwrap();
    let client = fixture.copy.root().join("client.jar");
    let library = fixture.copy.root().join("library.jar");
    fs::write(&client, b"inert").unwrap();
    fs::write(&library, b"inert").unwrap();
    let spec = VersionLaunchSpec {
        version_name: "fixture".into(),
        main_class: "fixture.Main".into(),
        client_jar: client,
        libraries: vec![library],
        assets_dir: fixture.copy.root().join("assets"),
        asset_index: "fixture".into(),
        jvm_args: vec!["-Xmx256M".into(), "-XX:+UseSerialGC".into()],
        game_args: vec!["--quickPlayMultiplayer".into(), "127.0.0.1:42010".into()],
        aurora_ipc_port: Some(12345),
        aurora_session_nonce: Some("fixture-only".into()),
    };
    let java = JavaRuntime::from_executable(std::env::current_exe().unwrap()).unwrap();
    let prepared = LauncherEngine
        .prepare_launch_with_settings(
            &fixture.copy,
            &java,
            &LaunchIdentity::offline("Fixture", uuid::Uuid::nil()),
            &spec,
            observed.effective(),
        )
        .unwrap();
    assert_eq!(
        prepared
            .command
            .args
            .iter()
            .filter(|arg| arg.starts_with("-Xmx"))
            .collect::<Vec<_>>(),
        vec!["-Xmx6144M"]
    );
    for argument in [
        "-Xms1024M",
        "-XX:+UseG1GC",
        "-Xss1M",
        "-Daurora.ipc.port=12345",
    ] {
        assert!(prepared.command.args.contains(&argument.into()));
    }
    assert!(!prepared.command.args.contains(&"-XX:+UseSerialGC".into()));
    assert!(prepared
        .command
        .args
        .windows(2)
        .any(|pair| pair == ["--quickPlayMultiplayer", "127.0.0.1:42010"]));
    let command = prepared.into_connect_command().unwrap();
    let environment = command.get_envs().collect::<BTreeMap<_, _>>();
    for key in [
        "JAVA_TOOL_OPTIONS",
        "JDK_JAVA_OPTIONS",
        "_JAVA_OPTIONS",
        "CLASSPATH",
    ] {
        assert_eq!(environment.get(std::ffi::OsStr::new(key)), Some(&None));
    }
    observed.recheck_before_spawn().unwrap();
    drop(command);
}
