use super::*;
use crate::engine::{resolve_launch_spec, InstanceId, InstanceLayout};
use std::{fs, path::PathBuf};

pub(crate) struct Fixture {
    root: PathBuf,
    pub(crate) instance: Instance,
    pub(crate) spec: VersionLaunchSpec,
    child: Value,
}
impl Fixture {
    pub(crate) fn new(index: usize) -> Self {
        let samples: Value =
            serde_json::from_str(include_str!("connect_status_profile_fixtures.json")).unwrap();
        let sample = &samples[index];
        let root =
            std::env::temp_dir().join(format!("aurora-status-profile-{}", uuid::Uuid::new_v4()));
        let instance =
            InstanceLayout::new(&root).open(InstanceId::parse("profile-fixture").unwrap());
        instance.ensure_layout().unwrap();
        let child = sample["profile"].clone();
        for profile in [&child, &sample["base"]["profile"]] {
            let id = profile["id"].as_str().unwrap();
            let folder = instance.root().join("versions").join(id);
            fs::create_dir_all(&folder).unwrap();
            fs::write(
                folder.join(format!("{id}.json")),
                serde_json::to_vec(profile).unwrap(),
            )
            .unwrap();
        }
        let minecraft = sample["minecraft"].as_str().unwrap();
        fs::write(
            instance
                .root()
                .join(format!("versions/{minecraft}/{minecraft}.jar")),
            b"fixture, never executed",
        )
        .unwrap();
        let spec = resolve_launch_spec(&instance, child["id"].as_str().unwrap()).unwrap();
        for path in &spec.libraries {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"fixture, never executed").unwrap();
        }
        Self {
            root,
            instance,
            spec,
            child,
        }
    }
    fn write_child(&self, profile: &Value) {
        let id = &self.spec.version_name;
        fs::write(
            self.instance
                .root()
                .join(format!("versions/{id}/{id}.json")),
            serde_json::to_vec(profile).unwrap(),
        )
        .unwrap();
    }
    pub(crate) fn profile(&self) -> StatusRuntimeProfile {
        inspect_launch_profile(&self.instance, &self.spec).expect("official profile observation")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self.root.parent() == Some(std::env::temp_dir().as_path()));
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("aurora-status-profile-"));
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn exact_status_targets_match_active_matrix() {
    let matrix: Value = serde_json::from_str(include_str!(
        "../../../../../scripts/active-mod-matrix.json"
    ))
    .unwrap();
    let targets: Vec<_> = matrix["targets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|target| target["connectReady"] != false)
        .collect();
    assert_eq!(targets.len(), TARGETS.len());
    for (minecraft, loader, version, _) in TARGETS {
        assert!(targets.iter().any(|t| t["minecraft"] == minecraft
            && t["loader"] == loader
            && t["loaderVersion"] == version));
    }
}

#[test]
fn official_profile_projections_resolve_all_nine_status_versions() {
    for (index, (mc, loader, version, protocol)) in TARGETS.iter().enumerate() {
        let fixture = Fixture::new(index);
        let profile = fixture.profile();
        assert_eq!(
            (
                profile.summary.minecraft_version,
                profile.summary.loader,
                profile.summary.loader_version,
                profile.summary.status_protocol
            ),
            (*mc, *loader, *version, *protocol)
        );
    }
}

#[test]
fn retained_profile_is_immutable_and_markers_cannot_supply_runtime() {
    let fixture = Fixture::new(1);
    let captured = fixture.profile();
    fs::write(
        fixture.instance.root().join("aurora-instance.json"),
        br#"{"minecraft":"1.21.1","loader":"forge"}"#,
    )
    .unwrap();
    assert_eq!(fixture.profile().summary, captured.summary);
    let mut changed = fixture.child.clone();
    changed["inheritsFrom"] = "1.21.1".into();
    fixture.write_child(&changed);
    assert!(inspect_launch_profile(&fixture.instance, &fixture.spec).is_none());
    assert_eq!(captured.summary.status_protocol, 754);
}

#[test]
fn forged_profile_identity_and_mixed_loaders_are_refused() {
    let fixture = Fixture::new(7);
    let cases = [
        ("id", Value::String("another-profile".into())),
        ("inheritsFrom", Value::String("../outside".into())),
        (
            "inheritsFrom",
            Value::String(fixture.spec.version_name.clone()),
        ),
        ("inheritsFrom", Value::String("1.20.1".into())),
        ("mainClass", Value::String("example.OtherMain".into())),
        ("jar", Value::String("1.20.1".into())),
    ];
    for (key, value) in cases {
        let mut changed = fixture.child.clone();
        changed[key] = value;
        fixture.write_child(&changed);
        assert!(
            inspect_launch_profile(&fixture.instance, &fixture.spec).is_none(),
            "{key}"
        );
    }
    for coordinate in [
        "net.fabricmc:fabric-loader:0.16.13",
        "net.minecraftforge:fmlloader:1.21.1-52.1.12",
        "net.neoforged:neoforge:21.1.1",
    ] {
        let mut changed = fixture.child.clone();
        changed["libraries"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"name":coordinate}));
        fixture.write_child(&changed);
        assert!(inspect_launch_profile(&fixture.instance, &fixture.spec).is_none());
    }
}

#[test]
fn loader_path_missing_artifact_and_resolved_spec_changes_are_refused() {
    let mut fixture = Fixture::new(7);
    let mut changed = fixture.child.clone();
    changed["libraries"][0]["downloads"] =
        serde_json::json!({"artifact":{"path":"../external.jar"}});
    fixture.write_child(&changed);
    assert!(inspect_launch_profile(&fixture.instance, &fixture.spec).is_none());
    fixture.write_child(&fixture.child);
    let original = fixture.spec.main_class.clone();
    fixture.spec.main_class = "example.OtherMain".into();
    assert!(inspect_launch_profile(&fixture.instance, &fixture.spec).is_none());
    fixture.spec.main_class = original;
    fixture.spec.client_jar = fixture.instance.root().join("another.jar");
    assert!(inspect_launch_profile(&fixture.instance, &fixture.spec).is_none());
    let fixture = Fixture::new(7);
    fs::remove_file(&fixture.spec.libraries[0]).unwrap();
    assert!(inspect_launch_profile(&fixture.instance, &fixture.spec).is_none());
}

#[test]
fn forge_resolved_game_arguments_must_match_observed_loader() {
    for index in [0, 2, 4, 6, 8] {
        let mut fixture = Fixture::new(index);
        fixture
            .spec
            .game_args
            .extend(["--launchTarget".into(), "other".into()]);
        if index == 0 {
            fixture
                .spec
                .game_args
                .extend(["--tweakClass".into(), "Other".into()]);
        }
        assert!(inspect_launch_profile(&fixture.instance, &fixture.spec).is_none());
    }
}
