use super::*;
use std::io::Write;

#[test]
fn native_server_classpath_bounds_only_the_main_manifest_section() {
    let root = std::env::temp_dir().join(format!("aurora-jar-main-section-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    for named_sections in [true, false] {
        let mut body = b"Manifest-Version: 1.0\r\n".to_vec();
        if named_sections {
            body.extend_from_slice(b"\r\n");
            for index in 0..16_000 {
                write!(
                    body,
                    "Name: classes/Example{index}.class\r\nSHA-256-Digest: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\r\n\r\n"
                )
                .unwrap();
            }
        } else {
            body.extend_from_slice(b"Large: x\r\n");
            body.extend_from_slice(
                &b" xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\r\n"
                    .repeat(18_000),
            );
            body.extend_from_slice(b"\r\n");
        }
        assert!(body.len() > MAX_MAIN_MANIFEST_BYTES);
        let name = if named_sections {
            "named.jar"
        } else {
            "main.jar"
        };
        let path = root.join(name);
        let mut archive = zip::ZipWriter::new(File::create(&path).unwrap());
        archive
            .start_file(
                "META-INF/MANIFEST.MF",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(&body).unwrap();
        archive.finish().unwrap();
        let records = [ResourceRecord {
            name: name.into(),
            observation: observe_file(&path, 4 * 1024 * 1024).unwrap().0,
        }];
        let result = validate_jar_classpaths(&root, &records, &[], &[], &|| false);
        if named_sections {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(Error::Limit)));
        }
    }
    assert_eq!(
        root.canonicalize().unwrap().parent(),
        Some(std::env::temp_dir().canonicalize().unwrap().as_path()),
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_server_classpath_matches_java_manifest_case_and_main_section() {
    const MANIFEST: &str = "META-INF/MANIFEST.MF";
    const ALTERNATE: &str = "META-INF/MANIFEST.XF";
    const SAFE: &str = "Manifest-Version: 1.0\r\n\r\n";
    let cases = [
        (
            vec![("meta-inf/manifest.mf", "Manifest-Version: 1.0\r\ncLaSs-PaTh: https://example.invalid/code.jar\r\n\r\n")],
            false, false,
        ),
        (vec![("meta-inf/manifest.mf", SAFE)], false, true),
        (vec![(MANIFEST, SAFE), ("meta-inf/manifest.mf", SAFE)], false, false),
        (vec![(MANIFEST, SAFE), (ALTERNATE, SAFE)], true, false),
        (
            vec![(MANIFEST, "Manifest-Version: 1.0\rClass-Path: https://example.invalid/code.jar\r\r")],
            false, false,
        ),
        (
            vec![(MANIFEST, "Manifest-Version: 1.0\r\nClass-Path: \r\ncLaSs-PaTh: \r\n\r\n")],
            false, false,
        ),
        (
            vec![(MANIFEST, "Manifest-Version: 1.0\r\n\r\nName: entry.txt\r\nClass-Path: https://example.invalid/ignored.jar\r\n\r\n")],
            false, true,
        ),
    ];
    for (entries, duplicate, allowed) in cases {
        let root = std::env::temp_dir().join(format!("aurora-jar-manifest-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("example.jar");
        let mut jar = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, contents) in entries {
            jar.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            jar.write_all(contents.as_bytes()).unwrap();
        }
        jar.finish().unwrap();
        if duplicate {
            let mut bytes = fs::read(&path).unwrap();
            let offsets: Vec<_> = bytes
                .windows(ALTERNATE.len())
                .enumerate()
                .filter_map(|(index, name)| (name == ALTERNATE.as_bytes()).then_some(index))
                .collect();
            assert_eq!(offsets.len(), 2);
            for index in offsets {
                bytes[index..index + MANIFEST.len()].copy_from_slice(MANIFEST.as_bytes());
            }
            fs::write(&path, bytes).unwrap();
        }
        let records = [ResourceRecord {
            name: "example.jar".into(),
            observation: observe_file(&path, 1024 * 1024).unwrap().0,
        }];
        assert_eq!(
            validate_jar_classpaths(&root, &records, &[], &[], &|| false).is_ok(),
            allowed
        );
        assert_eq!(
            root.canonicalize().unwrap().parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path()),
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn native_server_duplicate_root_manifest_requires_exact_recipe_identity_and_original_dependency() {
    let root = std::env::temp_dir().join(format!("aurora-duplicate-classpath-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let relative = "libraries/forge/forge.jar";
    fs::create_dir_all(root.join("libraries/forge")).unwrap();
    let mut zip = zip::ZipWriter::new(File::create(root.join("forge.jar")).unwrap());
    zip.start_file(
        "META-INF/MANIFEST.MF",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    write!(
        zip,
        "Manifest-Version: 1.0\r\nClass-Path: libraries/dependency.jar\r\n\r\n"
    )
    .unwrap();
    zip.finish().unwrap();
    fs::copy(root.join("forge.jar"), root.join(relative)).unwrap();
    let mut dependency =
        zip::ZipWriter::new(File::create(root.join("libraries/dependency.jar")).unwrap());
    dependency
        .start_file(
            "META-INF/MANIFEST.MF",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    write!(dependency, "Manifest-Version: 1.0\r\n\r\n").unwrap();
    dependency.finish().unwrap();
    let records: Vec<_> = ["forge.jar", relative, "libraries/dependency.jar"]
        .into_iter()
        .map(|name| ResourceRecord {
            name: name.into(),
            observation: observe_file(&root.join(name), 1024 * 1024).unwrap().0,
        })
        .collect();
    let required = vec!["forge.jar".into()];
    assert!(validate_jar_classpaths(&root, &records, &[], &required, &|| false).is_ok());
    assert!(validate_jar_classpaths(&root, &records, &[], &[], &|| false).is_err());
    let mut changed = records.clone();
    changed[0].observation.sha256 = "0".repeat(64);
    assert!(validate_jar_classpaths(&root, &changed, &[], &required, &|| false).is_err());
    assert!(validate_jar_classpaths(&root, &records[..2], &[], &required, &|| false).is_err());
    fs::create_dir(root.join("libraries/forge/libraries")).unwrap();
    fs::write(
        root.join("libraries/forge/libraries/dependency.jar"),
        b"unexpected",
    )
    .unwrap();
    assert!(validate_jar_classpaths(&root, &records, &[], &required, &|| false).is_err());
    assert_eq!(
        root.canonicalize().unwrap().parent(),
        Some(std::env::temp_dir().canonicalize().unwrap().as_path())
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_server_jar_path_budget_is_checked_before_acquisition() {
    use std::os::windows::ffi::OsStrExt;
    for runtime in ReadyProfile::ALL {
        let args = server_catalog::launch_args(runtime);
        let jar = args.windows(2).find(|pair| pair[0] == "-jar");
        let suffix = jar.map_or(0, |pair| pair[1].encode_utf16().count() + 1);
        // Three units for D:\ and four for the eventual internal \\?\ prefix.
        let directory = PathBuf::from(format!("D:\\{}", "x".repeat(258 - 7 - suffix)));
        assert_eq!(
            super::super::producer::validate_v3_launch_paths(&directory, runtime),
            Ok(())
        );
        assert_eq!(
            super::super::producer::validate_v3_launch_paths(
                &PathBuf::from(format!("{}x", directory.display())),
                runtime
            ),
            Err(Error::UnsafePath)
        );
        let verbatim = PathBuf::from(format!(r"\\?\{}", directory.display()));
        assert_eq!(
            super::super::producer::validate_v3_launch_paths(&verbatim, runtime),
            Ok(())
        );
    }

    let root = crate::engine::connect_voice::dedicated::tests::files::TestRoot::new();
    let relative = PathBuf::from(format!("native-status-{}", Uuid::nil()))
        .join("native/instances")
        .join(format!("aurora-test-{}", Uuid::nil()));
    let base_units = root
        .path()
        .join(&relative)
        .as_os_str()
        .encode_wide()
        .count();
    // Cwd itself fits, but cwd plus the official shim filename does not.
    let extra = 240_usize.checked_sub(base_units + 1).unwrap();
    let lab = root.path().join("x".repeat(extra));
    let cwd = lab.join(&relative);
    assert_eq!(cwd.as_os_str().encode_wide().count(), 240);
    assert_eq!(
        super::super::producer::validate_v3_process_directory(&cwd),
        Ok(())
    );
    assert!(matches!(
        ServerPreparationPlan::prepare(
            &lab,
            ReadyProfile::Forge1211,
            NativeVoiceProfile::SimpleVoiceChat,
            &|| false
        ),
        Err(Error::UnsafePath)
    ));
    assert!(
        !lab.exists(),
        "no resource acquisition precedes the JAR path gate"
    );
}

#[test]
fn native_server_long_working_directory_refused_before_download_copy_or_eula() {
    let root = std::env::temp_dir().join(format!("aurora-v3-long-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let lab = root.join("x".repeat(160));
    fs::create_dir(&lab).unwrap();
    assert!(matches!(
        ServerPreparationPlan::prepare(
            &lab,
            ReadyProfile::Fabric1165,
            NativeVoiceProfile::SimpleVoiceChat,
            &|| false
        ),
        Err(Error::UnsafePath)
    ));
    assert_eq!(
        fs::read_dir(&lab).unwrap().count(),
        0,
        "no Java acquisition, copy, EULA or installer may precede the path gate"
    );
    assert_eq!(
        root.canonicalize().unwrap().parent(),
        Some(std::env::temp_dir().canonicalize().unwrap().as_path())
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_server_classpaths_keep_pinned_mod_siblings_and_bootstrap_confinement() {
    for (index, (relative, classpath, pinned, unexpected, allowed)) in [
        ("mods/approved.jar", "missing-dev.jar", true, false, true),
        ("mods/approved.jar", "missing-dev.jar", false, false, false),
        ("mods/approved.jar", "missing-dev.jar", true, true, false),
        (
            "libraries/approved.jar",
            "missing-dev.jar",
            false,
            false,
            false,
        ),
        ("mods/approved.jar", "../../outside.jar", true, false, false),
        (
            "mods/approved.jar",
            "../missing-dev.jar",
            true,
            false,
            false,
        ),
        (
            "mods/approved.jar",
            "https://example.invalid/code.jar",
            true,
            false,
            false,
        ),
        ("mods/approved.jar", "C:/code.jar", true, false, false),
        ("mods/approved.jar", "a%2fcode.jar", true, false, false),
        ("mods/approved.jar", "a\\code.jar", true, false, false),
    ]
    .into_iter()
    .enumerate()
    {
        let root = std::env::temp_dir().join(format!(
            "aurora-server-classpaths-{}-{index}",
            Uuid::new_v4()
        ));
        fs::create_dir(&root).unwrap();
        let file = root.join(relative);
        fs::create_dir(file.parent().unwrap()).unwrap();
        let mut zip = zip::ZipWriter::new(File::create(&file).unwrap());
        zip.start_file(
            "META-INF/MANIFEST.MF",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        write!(
            zip,
            "Manifest-Version: 1.0\r\nClass-Path: {classpath}\r\n\r\n"
        )
        .unwrap();
        zip.finish().unwrap();
        if unexpected {
            fs::write(root.join("mods/missing-dev.jar"), b"unexpected").unwrap();
        }
        let (observation, _) = observe_file(&file, 1024 * 1024).unwrap();
        // Metadata used only by this validator; this cannot issue a prepared
        // capability or pass the compiled production provenance/catalog gate.
        let pins = [voice_catalog::VoiceArtifact {
            relative,
            bytes: observation.bytes,
            sha256: Box::leak(observation.sha256.clone().into_boxed_str()),
            url: "https://example.invalid/unused",
        }];
        let records = [ResourceRecord {
            name: relative.into(),
            observation,
        }];
        assert_eq!(
            validate_jar_classpaths(
                &root,
                &records,
                if pinned { &pins } else { &[] },
                &[],
                &|| false
            )
            .is_ok(),
            allowed,
            "case {index}"
        );
        assert!(matches!(
            validate_jar_classpaths(&root, &records, &pins, &[], &|| true),
            Err(Error::Cancelled)
        ));
        assert_eq!(
            root.canonicalize().unwrap().parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        fs::remove_dir_all(root).unwrap();
    }
}
