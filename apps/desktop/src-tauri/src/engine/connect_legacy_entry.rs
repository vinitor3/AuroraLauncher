//! Direct multiplayer argv for the installed clients predating Quick Play.
use std::net::SocketAddr;

use super::{QuickPlayError, VersionLaunchSpec};

pub(super) fn append(
    spec: &mut VersionLaunchSpec,
    endpoint: SocketAddr,
) -> Result<(), QuickPlayError> {
    if spec
        .game_args
        .iter()
        .any(|argument| disables_legacy_entry(argument))
    {
        return Err(QuickPlayError::ConflictingArguments);
    }
    // Main accepts a separate host and integer port in this exact client. Do
    // not pass host:port, use the default port, or modify inherited arguments.
    spec.game_args.extend([
        "--server".into(),
        endpoint.ip().to_string(),
        "--port".into(),
        endpoint.port().to_string(),
    ]);
    Ok(())
}

fn disables_legacy_entry(argument: &str) -> bool {
    let argument = argument.trim();
    if !argument.starts_with('-') {
        return false;
    }
    let name = argument
        .trim_start_matches('-')
        .split(['=', ' ', '\t'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    !name.is_empty()
        && ["disablemultiplayer", "demo"]
            .iter()
            .any(|protected| protected.starts_with(&name) || name.starts_with(protected))
}

#[cfg(test)]
mod tests {
    use super::super::{apply_guest_entry, read_profile, validate_profile_chain, ReadyProfile};
    use super::*;
    use crate::engine::{resolve_launch_spec, Instance, InstanceId, InstanceLayout};

    const VERSION_ID: &str = "fabric-loader-0.16.14-1.19.2";
    use serde_json::Value;
    use std::{fs, path::PathBuf};

    struct Fixture {
        root: PathBuf,
        instance: Instance,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("aurora-legacy-entry-{}", uuid::Uuid::new_v4()));
            let instance =
                InstanceLayout::new(&root).open(InstanceId::parse("legacy-fixture").unwrap());
            instance.ensure_layout().unwrap();
            let samples: Value =
                serde_json::from_str(include_str!("connect_status_profile_fixtures.json")).unwrap();
            for profile in [&samples[3]["base"]["profile"], &samples[3]["profile"]] {
                let id = profile["id"].as_str().unwrap();
                let directory = instance.root().join("versions").join(id);
                fs::create_dir_all(&directory).unwrap();
                fs::write(
                    directory.join(format!("{id}.json")),
                    serde_json::to_vec(profile).unwrap(),
                )
                .unwrap();
            }
            for relative in [
                "versions/1.19.2/1.19.2.jar",
                "libraries/net/fabricmc/fabric-loader/0.16.14/fabric-loader-0.16.14.jar",
            ] {
                let path = instance.root().join(relative);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, b"inert official-profile shape; never executed").unwrap();
            }
            Self { root, instance }
        }
        fn spec(&self) -> VersionLaunchSpec {
            resolve_launch_spec(&self.instance, VERSION_ID).unwrap()
        }
        fn rejected(&self, mut spec: VersionLaunchSpec, java: u32, endpoint: &str) {
            let game = spec.game_args.clone();
            let jvm = spec.jvm_args.clone();
            assert!(apply_guest_entry(
                &self.instance,
                &mut spec,
                ReadyProfile::Fabric1192,
                java,
                endpoint.parse().unwrap()
            )
            .is_err());
            assert_eq!(spec.game_args, game);
            assert_eq!(spec.jvm_args, jvm);
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn legacy_entry_composes_official_1192_arguments_with_exact_native_host_and_port() {
        let fixture = Fixture::new();
        let mut spec = fixture.spec();
        let jvm = spec.jvm_args.clone();
        assert!(spec.game_args.is_empty());
        let base = read_profile(&fixture.instance, "1.19.2").unwrap();
        let child = read_profile(&fixture.instance, VERSION_ID).unwrap();
        validate_profile_chain(&base, &child, ReadyProfile::Fabric1192).unwrap();
        apply_guest_entry(
            &fixture.instance,
            &mut spec,
            ReadyProfile::Fabric1192,
            17,
            "127.0.0.1:42010".parse().unwrap(),
        )
        .unwrap();
        assert_eq!(spec.game_args, ["--server", "127.0.0.1", "--port", "42010"]);
        assert_eq!(spec.jvm_args, jvm);
        fixture.rejected(spec, 17, "127.0.0.1:42010");
    }

    #[test]
    fn legacy_entry_refuses_other_runtimes_java_and_destinations_without_mutation() {
        let fixture = Fixture::new();
        for version in [
            "fabric-loader-0.16.14-1.21.1",
            "fabric-loader-0.16.14-1.20.1",
            "fabric-loader-0.16.13-1.19.2",
            "1.19.2-forge-43.4.16",
            "unknown",
        ] {
            let mut spec = fixture.spec();
            spec.version_name = version.into();
            fixture.rejected(spec, 17, "127.0.0.1:42010");
        }
        for java in [8, 21] {
            fixture.rejected(fixture.spec(), java, "127.0.0.1:42010");
        }
        for endpoint in [
            "127.0.0.1:0",
            "127.0.0.2:42010",
            "192.0.2.1:42010",
            "[::1]:42010",
        ] {
            fixture.rejected(fixture.spec(), 17, endpoint);
        }
    }

    #[test]
    fn legacy_entry_refuses_conflicting_options_and_disabled_multiplayer_without_mutation() {
        let fixture = Fixture::new();
        for flag in [
            "--server",
            "--server=outside",
            "--ser",
            "--port",
            "--po=1234",
            "--quickPlayMultiplayer",
            "--quickPlaySingleplayer",
            "--quickPlayPath",
            "--disableMultiplayer",
            "--disableMultiplayer=false",
            "--disableM",
            "--demo",
            "--dem",
            "--d",
            "--",
            "@untrusted",
            "--username",
            "--gameDir",
        ] {
            let mut spec = fixture.spec();
            spec.game_args.push(flag.into());
            fixture.rejected(spec, 17, "127.0.0.1:42010");
        }
    }
}
