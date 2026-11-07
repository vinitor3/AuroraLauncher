use super::*;
use crate::engine::{
    acquire_instance_mutation_lock, install_embedded_core, install_embedded_skins, InstanceId,
    InstanceLayout,
};
use serde_json::json;
#[path = "connect_ready_runtime_fabric1201_tests.rs"]
mod fabric1201;
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
