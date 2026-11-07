//! Fabric1.20.1 readiness fixtures use official profile metadata and embedded
//! managed mods. Java/game artifacts are inert unless the explicit probe is run.
use super::*;

#[test]
fn ready_fabric1201_uses_official_quickplay_profile_and_its_own_managed_pair() {
    let fixture = Fixture::fabric1201();
    let proof = fixture.inspect(&fixture.copy, "source-fixture").unwrap();
    assert_eq!(
        proof.profile,
        crate::engine::connect_ready_profile::ReadyProfile::Fabric1201
    );
    assert_eq!(proof.spec.version_name, "fabric-loader-0.16.14-1.20.1");
    assert_eq!(proof.profile.descriptor().java_major, 17);
    assert_eq!(
        proof
            .spec
            .game_args
            .iter()
            .filter(|arg| arg.as_str() == "--quickPlayMultiplayer")
            .count(),
        1
    );
    assert!(proof
        .spec
        .game_args
        .windows(2)
        .any(|pair| pair == ["--quickPlayMultiplayer", "127.0.0.1:42010"]));
    assert!(!proof
        .spec
        .game_args
        .iter()
        .any(|arg| arg.contains("${") || arg == "--quickPlayPath"));
    for property in [
        "-Djava.library.path",
        "-Djna.tmpdir",
        "-Dorg.lwjgl.system.SharedLibraryExtractPath",
        "-Dio.netty.native.workdir",
    ] {
        assert!(proof.spec.jvm_args.contains(&format!(
            "{property}={}",
            fixture.copy.natives_dir().display()
        )));
    }
    for relative in [
        proof.profile.core_file(),
        proof.profile.skins_file(),
        "versions/1.20.1/1.20.1.json",
    ] {
        assert!(fs::OpenOptions::new()
            .write(true)
            .open(fixture.copy.root().join(relative))
            .is_err());
    }
    let bytes = fs::read(fixture.copy.root().join(proof.profile.core_file())).unwrap();
    assert_eq!(proof.core_sha256, format!("{:x}", Sha256::digest(bytes)));
    let bytes = fs::read(fixture.copy.root().join(proof.profile.skins_file())).unwrap();
    assert_eq!(proof.skins_sha256, format!("{:x}", Sha256::digest(bytes)));
}

#[test]
fn ready_fabric1201_rejects_java21_and_a_managed_jar_from_the_other_profile() {
    let fixture = Fixture::fabric1201();
    let guard = acquire_instance_mutation_lock(&fixture.copy).unwrap();
    assert!(matches!(
        inspect_ready_runtime(
            &fixture.copy,
            &guard,
            "source-fixture",
            &fixture.java,
            None,
            "127.0.0.1:42010".parse().unwrap(),
            21,
            || false
        ),
        Err(Error::UnsupportedRuntime)
    ));
    drop(guard);
    let previous = Fixture::new();
    let wrong_core = fs::read(
        previous
            .copy
            .root()
            .join("mods/aurora-core-fabric-1.21.1.jar"),
    )
    .unwrap();
    fs::write(
        fixture
            .copy
            .root()
            .join("mods/aurora-core-fabric-1.20.1.jar"),
        wrong_core,
    )
    .unwrap();
    assert!(matches!(
        fixture.inspect(&fixture.copy, "source-fixture"),
        Err(Error::ArtifactMismatch)
    ));
}

#[test]
fn ready_fabric1201_preserves_source_and_rejects_conflicting_profile_metadata() {
    let fixture = Fixture::fabric1201();
    let original = fs::read(fixture.source.root().join("aurora-instance.json")).unwrap();
    let first = fixture.inspect(&fixture.copy, "source-fixture").unwrap();
    let fingerprint = first.fingerprint.clone();
    drop(first);
    let second = fixture.inspect(&fixture.copy, "source-fixture").unwrap();
    assert_eq!(fingerprint, second.fingerprint);
    drop(second);
    assert_eq!(
        fs::read(fixture.source.root().join("aurora-instance.json")).unwrap(),
        original
    );
    fs::write(
        fixture.copy.root().join("aurora-modpack.json"),
        br#"{"minecraft":"1.21.1","versionId":"fabric-loader-0.16.14-1.21.1"}"#,
    )
    .unwrap();
    assert!(matches!(
        fixture.inspect(&fixture.copy, "source-fixture"),
        Err(Error::UnsupportedRuntime)
    ));
}

#[test]
#[ignore = "explicit installed Java17 path; no Minecraft process or downloads"]
fn ready_fabric1201_actual_java17_probe_under_retained_runtime_pins() {
    let java = std::env::var_os("AURORA_CONNECT_READY_JAVA17")
        .expect("explicit Java17 opt-in path required");
    let mut fixture = Fixture::fabric1201();
    fixture.java = PathBuf::from(java);
    let proof = fixture.inspect(&fixture.copy, "source-fixture").unwrap();
    assert_eq!(
        crate::engine::connect_java_probe::probe_java_major(&fixture.java, || false).unwrap(),
        proof.profile.descriptor().java_major
    );
    assert_eq!(proof.profile.descriptor().java_major, 17);
    proof.recheck_before_spawn().unwrap();
}
