use super::*;

#[test]
fn ready_matrix_java8_pins_standalone_and_nested_hotspot_files_without_a_process() {
    let fixture = Fixture::new();
    for prefix in ["", "jre/"] {
        let home = fixture.root.join(if prefix.is_empty() {
            "java8-jre"
        } else {
            "java8-jdk"
        });
        let executable = home.join("bin/java.exe");
        for relative in ["bin/java.exe", "release", "bin/jli.dll"] {
            Fixture::write(&home, relative, b"layout fixture, never executed");
        }
        for file in [
            "bin/java.dll",
            "bin/jli.dll",
            "bin/server/jvm.dll",
            "lib/amd64/jvm.cfg",
            "lib/rt.jar",
            "lib/resources.jar",
            "lib/charsets.jar",
            "lib/jce.jar",
            "lib/jsse.jar",
        ] {
            Fixture::write(&home, &format!("{prefix}{file}"), b"layout fixture");
        }
        let inspect = || Reader {
            cancel: &|| false,
            pins: Vec::new(),
            hashes: Vec::new(),
            bytes: 0,
        };
        let mut reader = inspect();
        pin_java_runtime(&mut reader, &executable, 8).unwrap();
        let archive = home.join(format!("{prefix}lib/rt.jar"));
        assert!(fs::OpenOptions::new().write(true).open(&archive).is_err());
        drop(reader);
        fs::remove_file(archive).unwrap();
        assert_eq!(
            pin_java_runtime(&mut inspect(), &executable, 8),
            Err(Error::JavaUnavailable)
        );
        assert_eq!(
            pin_java_runtime(&mut inspect(), &executable, 17),
            Err(Error::JavaUnavailable)
        );
    }
    let mut reader = Reader {
        cancel: &|| false,
        pins: Vec::new(),
        hashes: Vec::new(),
        bytes: 0,
    };
    assert_eq!(
        pin_java_runtime(&mut reader, &fixture.java, 8),
        Err(Error::JavaUnavailable)
    );
}

#[test]
fn ready_matrix_refuses_projected_status_metadata_for_new_ready_authority() {
    let projections: serde_json::Value =
        serde_json::from_str(include_str!("connect_status_profile_fixtures.json")).unwrap();
    for projected in projections.as_array().unwrap() {
        let child = &projected["profile"];
        let profile = ReadyProfile::from_version_id(child["id"].as_str().unwrap()).unwrap();
        let base = &projected["base"]["profile"];
        if !matches!(profile, ReadyProfile::Fabric1211 | ReadyProfile::Fabric1201) {
            assert!(matches!(
                RuntimePins::from_profiles(profile, base, child),
                Err(Error::UnsupportedRuntime)
            ));
        }
    }
    assert_eq!(
        verify_sha1(b"abc", "a9993e364706816aba3e25717850c26c9cd0d89d"),
        Ok(())
    );
    assert_eq!(
        verify_sha1(b"tampered", "a9993e364706816aba3e25717850c26c9cd0d89d"),
        Err(Error::ArtifactMismatch)
    );
}

#[test]
#[ignore = "explicit official installed profile JSON fixtures; no game or Java execution"]
fn ready_matrix_official_profile_inputs_resolve_all_nine_client_commands() {
    let root = PathBuf::from(
        std::env::var_os("AURORA_CONNECT_READY_PROFILE_FIXTURES")
            .expect("explicit official-profile fixtures required"),
    );
    for profile in ReadyProfile::ALL {
        let runtime = profile.descriptor();
        let id = InstanceId::parse(profile.version_id()).unwrap();
        let instance = InstanceLayout::new(&root).open(id);
        let document = |id: &str| {
            let path = instance.root().join(format!("versions/{id}/{id}.json"));
            serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap()).unwrap()
        };
        let base = document(runtime.minecraft_version);
        let child = document(profile.version_id());
        connect_quick_play::validate_profile_chain(&base, &child, profile).unwrap();
        let pins = RuntimePins::from_profiles(profile, &base, &child).unwrap();
        let spec = crate::engine::resolve_launch_spec(&instance, profile.version_id()).unwrap();
        if let Some(pins) = pins {
            assert!(spec.libraries.iter().all(|path| {
                let relative = path
                    .strip_prefix(instance.root())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                pins.library_sha1.contains_key(&relative)
                    && !pins.additional_libraries.contains_key(&relative)
            }));
            let mut modified = child.clone();
            modified["arguments"]["jvm"] = json!(["-javaagent:outside.jar"]);
            assert!(matches!(
                RuntimePins::from_profiles(profile, &base, &modified),
                Err(Error::UnsupportedRuntime)
            ));
        }
        let endpoint = "127.0.0.1:42010".parse().unwrap();
        let mut selected = spec.clone();
        connect_quick_play::apply_guest_entry(
            &instance,
            &mut selected,
            profile,
            runtime.java_major,
            endpoint,
        )
        .unwrap_or_else(|error| panic!("{}: {error}", profile.version_id()));
        let added = &selected.game_args[spec.game_args.len()..];
        if profile.uses_quick_play() {
            assert_eq!(added, ["--quickPlayMultiplayer", "127.0.0.1:42010"]);
        } else {
            assert_eq!(added, ["--server", "127.0.0.1", "--port", "42010"]);
        }
        assert!(selected
            .game_args
            .iter()
            .chain(&selected.jvm_args)
            .all(|arg| !arg.contains("${")));
        let mut expected = spec.clone();
        expected.apply_offline_compatibility(runtime.minecraft_version);
        assert_eq!(selected.jvm_args, expected.jvm_args);
        assert!(connect_quick_play::apply_guest_entry(
            &instance,
            &mut selected,
            profile,
            runtime.java_major,
            endpoint
        )
        .is_err());
        if profile.uses_forge_module_path() {
            let mut changed = spec.clone();
            let index = changed.jvm_args.iter().position(|arg| arg == "-p").unwrap();
            changed.jvm_args[index + 1] = root.join("external.jar").display().to_string();
            assert_eq!(
                connect_quick_play::apply_guest_entry(
                    &instance,
                    &mut changed,
                    profile,
                    runtime.java_major,
                    endpoint
                ),
                Err(connect_quick_play::QuickPlayError::ConflictingArguments),
            );
        }
    }
}
