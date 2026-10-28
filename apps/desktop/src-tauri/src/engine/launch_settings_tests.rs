use super::*;
use crate::engine::{acquire_instance_mutation_lock, InstanceId, InstanceLayout};

struct Fixture {
    root: PathBuf,
    instance: Instance,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "aurora-launch-settings-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("test").unwrap());
        instance.ensure_layout().unwrap();
        Self { root, instance }
    }

    fn path(&self) -> PathBuf {
        self.instance.root().join(SETTINGS_PATH)
    }

    fn write_raw(&self, contents: &[u8]) {
        fs::create_dir_all(self.path().parent().unwrap()).unwrap();
        fs::write(self.path(), contents).unwrap();
    }

    fn assert_no_staging(&self) {
        assert!(fs::read_dir(self.path().parent().unwrap())
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn automatic() -> LaunchSettings {
    LaunchSettings {
        memory_mode: MemoryMode::Automatic,
        ..LaunchSettings::default()
    }
}

fn with_args(args: &[&str]) -> LaunchSettings {
    LaunchSettings {
        jvm_args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        ..LaunchSettings::default()
    }
}

#[test]
fn invalid_launch_preferences_do_not_disable_read_only_diagnostics() {
    let fixture = Fixture::new();
    fixture.write_raw(br#"{"schemaVersion":2}"#);
    assert!(read(&fixture.instance).is_err());
    let (memory, limitation) = diagnostic_memory(&fixture.instance, Some(6144));
    assert_eq!(memory, 6144);
    assert!(limitation
        .unwrap()
        .contains("não representa a configuração atual"));
    let (memory, limitation) = diagnostic_memory(&fixture.instance, None);
    assert_eq!(memory, 4096);
    assert!(limitation.is_some());
    // No disk repair or fallback is allowed in the launch path.
    assert_eq!(fs::read(fixture.path()).unwrap(), br#"{"schemaVersion":2}"#);
    assert!(read(&fixture.instance).is_err());
}

#[test]
fn valid_launch_preferences_provide_diagnostic_memory_without_a_fallback() {
    let fixture = Fixture::new();
    fixture.write_raw(br#"{"maxMemoryMb":512}"#);
    let (memory, limitation) = diagnostic_memory(&fixture.instance, Some(8192));
    assert_eq!(memory, 512);
    assert!(limitation.is_none());
}

#[cfg(windows)]
#[test]
fn windows_physical_inventory_returns_a_plausible_nonzero_measurement() {
    let measured =
        physical_memory_mb().expect("GlobalMemoryStatusEx must succeed on the test host");
    assert!(measured >= 512);
}

#[test]
fn absent_or_partial_preferences_preserve_the_previous_launch_defaults() {
    let fixture = Fixture::new();
    assert_eq!(read(&fixture.instance).unwrap(), LaunchSettings::default());
    fixture.write_raw(b"{}");
    let settings = read(&fixture.instance).unwrap();
    assert_eq!(settings.memory_mode, MemoryMode::Manual);
    assert_eq!(settings.max_memory_mb, 4096);
    assert_eq!(settings.min_memory_mb, None);
    assert_eq!(settings.gc_mode, GcMode::Runtime);
    assert!(settings.jvm_args.is_empty());
    let effective = settings.resolve(None, 500, 8).unwrap();
    assert_eq!(effective.max_memory_mb, 4096);
    assert!(effective.jvm_args.is_empty());
}

#[test]
fn automatic_memory_uses_mod_inventory_and_reserves_two_gib_for_the_system() {
    for (total, mods, expected) in [
        (16_384, 100, 4_800),
        (8_192, 10, 4_096),
        (4_096, 10, 2_048),
        (2_560, 100, 512),
    ] {
        assert_eq!(recommendation(Some(total), mods), Some(expected));
        assert_eq!(
            automatic()
                .resolve(Some(total), mods, 17)
                .unwrap()
                .max_memory_mb,
            expected
        );
    }
}

#[test]
fn automatic_memory_does_not_invent_capacity_when_ram_is_unknown_or_too_low() {
    for total in [None, Some(0), Some(2_048), Some(2_559)] {
        assert_eq!(recommendation(total, 1), None);
        assert!(matches!(
            automatic().resolve(total, 1, 17),
            Err(LaunchSettingsError::MemoryUnavailable)
        ));
    }
}

#[test]
fn automatic_memory_handles_overflow_and_caps_very_large_heaps() {
    assert_eq!(recommendation(Some(8_192), u32::MAX), Some(6_144));
    assert_eq!(
        recommendation(Some(u32::MAX), u32::MAX),
        Some(MAX_MEMORY_MB)
    );
}

#[test]
fn manual_memory_checks_boundaries_initial_heap_and_physical_capacity() {
    for max in [MIN_MEMORY_MB, MAX_MEMORY_MB] {
        let settings = LaunchSettings {
            max_memory_mb: max,
            min_memory_mb: Some(max),
            ..LaunchSettings::default()
        };
        let effective = settings.resolve(Some(max), 0, 21).unwrap();
        assert_eq!(effective.max_memory_mb, max);
        assert_eq!(effective.min_memory_mb, Some(max));
    }
    for (min, max) in [
        (None, 0),
        (None, MIN_MEMORY_MB - 1),
        (None, MAX_MEMORY_MB + 1),
        (Some(MIN_MEMORY_MB - 1), 4096),
        (Some(8192), 4096),
    ] {
        let settings = LaunchSettings {
            min_memory_mb: min,
            max_memory_mb: max,
            ..LaunchSettings::default()
        };
        assert!(matches!(
            settings.validate(),
            Err(LaunchSettingsError::Memory)
        ));
    }
    assert!(matches!(
        LaunchSettings::default().resolve(Some(2_048), 0, 17),
        Err(LaunchSettingsError::MemoryExceedsSystem)
    ));
}

#[test]
fn automatic_mode_rejects_an_initial_heap_that_could_override_its_budget() {
    let settings = LaunchSettings {
        min_memory_mb: Some(512),
        ..automatic()
    };
    assert!(matches!(
        settings.validate(),
        Err(LaunchSettingsError::Memory)
    ));
}

#[test]
fn recommended_collector_is_conservative_for_legacy_java_and_explicit_for_modern_java() {
    let settings = LaunchSettings {
        gc_mode: GcMode::Recommended,
        ..LaunchSettings::default()
    };
    for major in [8, 11, 16] {
        assert!(settings
            .resolve(Some(8192), 0, major)
            .unwrap()
            .jvm_args
            .is_empty());
    }
    for major in [17, 21, 25] {
        assert_eq!(
            settings.resolve(Some(8192), 0, major).unwrap().jvm_args,
            ["-XX:+UseG1GC"]
        );
    }
    assert!(LaunchSettings::default()
        .resolve(Some(8192), 0, 21)
        .unwrap()
        .jvm_args
        .is_empty());
}

#[test]
fn explicit_and_recommended_collectors_cannot_silently_compete() {
    for args in [
        vec!["-XX:+UseG1GC", "-XX:+UseParallelGC"],
        vec!["-XX:+UseG1GC", "-XX:+UseG1GC"],
    ] {
        assert!(matches!(
            with_args(&args).validate(),
            Err(LaunchSettingsError::CollectorConflict)
        ));
    }
    let settings = LaunchSettings {
        gc_mode: GcMode::Recommended,
        ..with_args(&["-XX:+UseSerialGC"])
    };
    assert!(matches!(
        settings.validate(),
        Err(LaunchSettingsError::CollectorConflict)
    ));
    assert_eq!(
        with_args(&["-XX:+UseParallelGC"])
            .resolve(Some(8192), 0, 17)
            .unwrap()
            .jvm_args,
        ["-XX:+UseParallelGC"]
    );
}

#[test]
fn performance_arguments_remain_tokens_and_preserve_order() {
    let args = [
        "-ea",
        "-Xss256k",
        "-XX:MaxGCPauseMillis=200",
        "-XX:G1ReservePercent=20",
        "-XX:InitiatingHeapOccupancyPercent=35",
        "-XX:ParallelGCThreads=4",
        "-XX:ConcGCThreads=2",
        "-XX:+ParallelRefProcEnabled",
        "-XX:-UseStringDeduplication",
        "-XX:-AlwaysPreTouch",
        "-Dfile.encoding=UTF-8",
        "-Duser.language=pt",
        "-Duser.country=BR",
    ];
    let effective = with_args(&args).resolve(None, 0, 17).unwrap();
    assert_eq!(effective.jvm_args, args);
}

#[test]
fn custom_arguments_cannot_redirect_execution_memory_or_aurora_properties() {
    for arg in [
        "-javaagent:C:/agent.jar",
        "-agentlib:jdwp=transport=dt_socket,server=y,address=8000",
        "-agentpath:C:/agent.dll",
        "-Xrunjdwp:transport=dt_socket,server=y",
        "-XX:OnError=cmd.exe",
        "-XX:OnOutOfMemoryError=cmd.exe",
        "-XX:+HeapDumpOnOutOfMemoryError",
        "-Xmx4096M",
        "-Xms512M",
        "-XX:MaxRAMPercentage=80",
        "-XX:MaxHeapSize=4096",
        "-cp",
        "-classpath=other.jar",
        "--class-path=other.jar",
        "-jar",
        "--module-path=other.jar",
        "--module=other.Main",
        "@C:/args.txt",
        "-Daurora.ipc.port=1234",
        "-Daurora.session.nonce=fake",
        "-Daurora.profile.skinUrl=https://example.invalid/skin.png",
        "-Djava.library.path=C:/foreign",
        "-Dminecraft.api.services.host=https://example.invalid",
        "--gameDir=C:/other",
        "example.Main",
        "-Xss1m -javaagent:C:/agent.jar",
        "-ea\n-javaagent:C:/agent.jar",
        "-ea\t-da",
        "-ea\0",
        " -ea",
        "",
    ] {
        assert!(
            matches!(
                with_args(&[arg]).validate(),
                Err(LaunchSettingsError::Argument)
            ),
            "unexpectedly accepted {arg:?}"
        );
    }
}

#[test]
fn malformed_and_unicode_numeric_arguments_are_rejected_without_panicking() {
    for arg in [
        "-Xss",
        "-Xssé",
        "-Xss😀",
        "-Xss1😀",
        "-Xss１２m",
        "-Xss+1m",
        "-Xss0m",
        "-Xss17m",
        "-Xss255k",
        "-Xss16385k",
        "-XX:MaxGCPauseMillis=0",
        "-XX:MaxGCPauseMillis=10001",
        "-XX:ParallelGCThreads=257",
        "-XX:ConcGCThreads=+1",
        "-XX:G1ReservePercent=101",
        "-XX:InitiatingHeapOccupancyPercent=0",
        "-Duser.language=PT",
        "-Duser.language=portuguese",
        "-Duser.country=br",
        "-Duser.country=BR;run",
    ] {
        let result = std::panic::catch_unwind(|| with_args(&[arg]).validate());
        assert!(result.is_ok(), "argument validation panicked for {arg:?}");
        assert!(
            matches!(result.unwrap(), Err(LaunchSettingsError::Argument)),
            "unexpectedly accepted {arg:?}"
        );
    }
}

#[test]
fn argument_list_and_token_size_are_bounded() {
    let mut settings = with_args(&["-ea"]);
    settings.jvm_args = vec!["-ea".into(); 64];
    assert!(settings.validate().is_ok());
    settings.jvm_args.push("-da".into());
    assert!(matches!(
        settings.validate(),
        Err(LaunchSettingsError::Argument)
    ));
    settings.jvm_args = vec![format!("-Duser.language={}", "p".repeat(256))];
    assert!(matches!(
        settings.validate(),
        Err(LaunchSettingsError::Argument)
    ));
}

#[test]
fn recommended_collector_reserves_its_argument_slot_before_the_java_version_is_known() {
    let mut settings = LaunchSettings {
        gc_mode: GcMode::Recommended,
        jvm_args: vec!["-ea".into(); 63],
        ..LaunchSettings::default()
    };
    assert!(settings.validate().is_ok());
    assert_eq!(settings.resolve(None, 0, 0).unwrap().jvm_args.len(), 63);
    let modern = settings.resolve(None, 0, 17).unwrap();
    assert_eq!(modern.jvm_args.len(), 64);
    assert_eq!(modern.jvm_args.last().unwrap(), "-XX:+UseG1GC");
    settings.jvm_args.push("-da".into());
    assert!(matches!(
        settings.validate(),
        Err(LaunchSettingsError::Argument)
    ));
    for java_major in [0, 17] {
        assert!(matches!(
            settings.resolve(None, 0, java_major),
            Err(LaunchSettingsError::Argument)
        ));
    }
}

#[test]
fn unsupported_schema_unknown_fields_and_malformed_json_are_not_silently_reset() {
    let fixture = Fixture::new();
    for bytes in [
        br#"{"schemaVersion":2}"#.as_slice(),
        br#"{"maxMemoryMb":4096,"command":"run"}"#.as_slice(),
        br#"{"memoryMode":"unexpected"}"#.as_slice(),
        br#"{"maxMemoryMb":-1}"#.as_slice(),
        br#"{"maxMemoryMb":"#.as_slice(),
    ] {
        fixture.write_raw(bytes);
        assert!(matches!(
            read(&fixture.instance),
            Err(LaunchSettingsError::InvalidSettings)
        ));
        assert_eq!(fs::read(fixture.path()).unwrap(), bytes);
    }
}

#[test]
fn oversized_or_non_file_settings_are_rejected_without_modifying_them() {
    let fixture = Fixture::new();
    let bytes = vec![b' '; MAX_BYTES as usize + 1];
    fixture.write_raw(&bytes);
    assert!(matches!(
        read(&fixture.instance),
        Err(LaunchSettingsError::Storage)
    ));
    assert_eq!(fs::read(fixture.path()).unwrap(), bytes);
    fs::remove_file(fixture.path()).unwrap();
    fs::create_dir(fixture.path()).unwrap();
    let marker = fixture.path().join("preserved");
    fs::write(&marker, b"unchanged").unwrap();
    assert!(matches!(
        read(&fixture.instance),
        Err(LaunchSettingsError::Storage)
    ));
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    assert!(matches!(
        save(&fixture.instance, &guard, &LaunchSettings::default()),
        Err(LaunchSettingsError::Storage)
    ));
    assert_eq!(fs::read(marker).unwrap(), b"unchanged");
    fixture.assert_no_staging();
}

#[test]
fn save_round_trips_and_replaces_existing_preferences_repeatedly() {
    let fixture = Fixture::new();
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    for max_memory_mb in [4096, 6144, 8192] {
        let settings = LaunchSettings {
            min_memory_mb: Some(1024),
            max_memory_mb,
            ..with_args(&["-Xss1m", "-Duser.language=pt"])
        };
        save(&fixture.instance, &guard, &settings).unwrap();
        assert_eq!(read(&fixture.instance).unwrap(), settings);
        fixture.assert_no_staging();
    }
}

#[test]
fn invalid_save_preserves_previous_bytes_and_leaves_no_staging() {
    let fixture = Fixture::new();
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    save(&fixture.instance, &guard, &LaunchSettings::default()).unwrap();
    let before = fs::read(fixture.path()).unwrap();
    for settings in [
        LaunchSettings {
            max_memory_mb: 0,
            ..LaunchSettings::default()
        },
        LaunchSettings {
            schema_version: 99,
            ..LaunchSettings::default()
        },
        with_args(&["-javaagent:agent.jar"]),
    ] {
        assert!(save(&fixture.instance, &guard, &settings).is_err());
        assert_eq!(fs::read(fixture.path()).unwrap(), before);
        fixture.assert_no_staging();
    }
}

#[test]
fn saving_with_another_instances_guard_does_not_write_preferences() {
    let fixture = Fixture::new();
    let other = InstanceLayout::new(&fixture.root).open(InstanceId::parse("other").unwrap());
    other.ensure_layout().unwrap();
    let guard = acquire_instance_mutation_lock(&other).unwrap();
    assert!(matches!(
        save(&fixture.instance, &guard, &LaunchSettings::default()),
        Err(LaunchSettingsError::WrongLock)
    ));
    assert!(!fixture.path().exists());
}

#[test]
fn enabled_inventory_counts_only_top_level_regular_jar_files() {
    let fixture = Fixture::new();
    for name in [
        "active.jar",
        "uppercase.JAR",
        "disabled.jar.disabled",
        "notes.txt",
    ] {
        fs::write(fixture.instance.mods_dir().join(name), b"fixture").unwrap();
    }
    let directory = fixture.instance.mods_dir().join("directory.jar");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("nested.jar"), b"nested").unwrap();
    assert_eq!(enabled_mod_count(&fixture.instance).unwrap(), 2);
}

#[test]
fn missing_mods_directory_is_an_empty_inventory() {
    let fixture = Fixture::new();
    fs::remove_dir(fixture.instance.mods_dir()).unwrap();
    assert_eq!(enabled_mod_count(&fixture.instance).unwrap(), 0);
}

#[test]
fn inventory_enumeration_limit_includes_disabled_and_unrelated_entries() {
    let fixture = Fixture::new();
    for index in 0..10_000 {
        fs::write(
            fixture
                .instance
                .mods_dir()
                .join(format!("{index}.disabled")),
            b"",
        )
        .unwrap();
    }
    assert_eq!(enabled_mod_count(&fixture.instance).unwrap(), 0);
    fs::write(fixture.instance.mods_dir().join("one-more.txt"), b"").unwrap();
    assert!(matches!(
        enabled_mod_count(&fixture.instance),
        Err(LaunchSettingsError::Storage)
    ));
}

// A directory junction works without symlink privileges on Windows. Remove the
// link before fixture cleanup so recursive cleanup cannot reach its target.
struct DirectoryLink(PathBuf);

impl DirectoryLink {
    fn new(target: &Path, link: &Path) -> Self {
        #[cfg(windows)]
        {
            let output = crate::engine::process::background_command("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(link.to_string_lossy().replace('/', "\\"))
                .arg(target.to_string_lossy().replace('/', "\\"))
                .output()
                .unwrap();
            assert!(output.status.success(), "junction fixture failed");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        Self(link.to_owned())
    }
}

impl Drop for DirectoryLink {
    fn drop(&mut self) {
        #[cfg(windows)]
        let _ = fs::remove_dir(&self.0);
        #[cfg(unix)]
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn inventory_does_not_traverse_linked_jars_or_a_linked_mod_directory() {
    let fixture = Fixture::new();
    let outside = fixture.root.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("outside.jar"), b"unchanged").unwrap();
    let link = DirectoryLink::new(&outside, &fixture.instance.mods_dir().join("linked.jar"));
    assert_eq!(enabled_mod_count(&fixture.instance).unwrap(), 0);
    drop(link);
    fs::remove_dir(fixture.instance.mods_dir()).unwrap();
    let _link = DirectoryLink::new(&outside, &fixture.instance.mods_dir());
    assert!(matches!(
        enabled_mod_count(&fixture.instance),
        Err(LaunchSettingsError::Storage)
    ));
    assert_eq!(fs::read(outside.join("outside.jar")).unwrap(), b"unchanged");
}

#[test]
fn linked_settings_parent_is_rejected_and_the_outside_file_is_preserved() {
    let fixture = Fixture::new();
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    let outside = fixture.root.join("outside");
    fs::create_dir(&outside).unwrap();
    let marker = outside.join("launch-settings.json");
    fs::write(&marker, b"outside-settings").unwrap();
    let _link = DirectoryLink::new(&outside, &fixture.instance.root().join(".aurora"));
    assert!(matches!(
        read(&fixture.instance),
        Err(LaunchSettingsError::Storage)
    ));
    assert!(matches!(
        save(&fixture.instance, &guard, &LaunchSettings::default()),
        Err(LaunchSettingsError::Storage)
    ));
    assert_eq!(fs::read(marker).unwrap(), b"outside-settings");
    assert_eq!(fs::read_dir(outside).unwrap().count(), 1);
}

#[cfg(windows)]
#[test]
fn failed_windows_atomic_replacement_preserves_previous_settings_and_cleans_staging() {
    use std::os::windows::fs::OpenOptionsExt;

    let fixture = Fixture::new();
    let guard = acquire_instance_mutation_lock(&fixture.instance).unwrap();
    let previous = LaunchSettings::default();
    save(&fixture.instance, &guard, &previous).unwrap();
    let before = fs::read(fixture.path()).unwrap();
    // FILE_SHARE_READ only: another reader must prevent rename/delete publication.
    let handle = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(fixture.path())
        .unwrap();
    let replacement = LaunchSettings {
        max_memory_mb: 8192,
        ..previous.clone()
    };
    assert!(matches!(
        save(&fixture.instance, &guard, &replacement),
        Err(LaunchSettingsError::Storage)
    ));
    drop(handle);
    assert_eq!(fs::read(fixture.path()).unwrap(), before);
    assert_eq!(read(&fixture.instance).unwrap(), previous);
    fixture.assert_no_staging();
    save(&fixture.instance, &guard, &replacement).unwrap();
    assert_eq!(read(&fixture.instance).unwrap(), replacement);
    fixture.assert_no_staging();
}
