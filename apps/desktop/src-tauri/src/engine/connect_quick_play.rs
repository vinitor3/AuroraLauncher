//! Narrow argv composition for a native Connect guest launch permit.
//!
//! The caller owns the diagnostic-copy/mutation guards and must obtain the
//! endpoint from the original live guest, then recheck that permit at spawn.
//! This module neither grants connectivity nor launches Java. It preserves the
//! installed resolver's feature rules and adds no Quick Play log destination.
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use serde_json::Value;

use super::{recovery, Instance, VersionLaunchSpec};

pub const CONNECT_QUICK_PLAY_VERSION_ID: &str = "fabric-loader-0.16.14-1.21.1";
pub const CONNECT_QUICK_PLAY_MINECRAFT_VERSION: &str = "1.21.1";
pub const CONNECT_QUICK_PLAY_JAVA_MAJOR: u32 = 21;
const FABRIC_MAIN: &str = "net.fabricmc.loader.impl.launch.knot.KnotClient";
const VANILLA_MAIN: &str = "net.minecraft.client.main.Main";
const LOADER_COORDINATE: &str = "net.fabricmc:fabric-loader:0.16.14";
const LOADER_PATH: &str = "libraries/net/fabricmc/fabric-loader/0.16.14/fabric-loader-0.16.14.jar";
const CLIENT_PATH: &str = "versions/1.21.1/1.21.1.jar";
const MAX_PROFILE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum QuickPlayError {
    #[error("connect_quick_play_unsupported_runtime")]
    UnsupportedRuntime,
    #[error("connect_quick_play_invalid_profile")]
    InvalidProfile,
    #[error("connect_quick_play_conflicting_arguments")]
    ConflictingArguments,
    #[error("connect_quick_play_invalid_endpoint")]
    InvalidEndpoint,
}
impl QuickPlayError {
    pub fn code(self) -> &'static str {
        match self {
            Self::UnsupportedRuntime => "connect_quick_play_unsupported_runtime",
            Self::InvalidProfile => "connect_quick_play_invalid_profile",
            Self::ConflictingArguments => "connect_quick_play_conflicting_arguments",
            Self::InvalidEndpoint => "connect_quick_play_invalid_endpoint",
        }
    }
}

/// Append one option/value pair to a previously resolved installed spec.
///
/// `observed_java_major` must come from the caller's bounded native Java probe,
/// never from frontend metadata. The typed endpoint must come from the opaque
/// GuestLaunchPermit; it is not an arbitrary address accepted through IPC.
/// All checks finish before either argv is changed. No installed file is written.
pub fn apply_guest_quick_play(
    instance: &Instance,
    spec: &mut VersionLaunchSpec,
    observed_java_major: u32,
    endpoint: SocketAddr,
) -> Result<(), QuickPlayError> {
    if endpoint.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) || endpoint.port() == 0 {
        return Err(QuickPlayError::InvalidEndpoint);
    }
    if observed_java_major != CONNECT_QUICK_PLAY_JAVA_MAJOR
        || spec.version_name != CONNECT_QUICK_PLAY_VERSION_ID
        || spec.main_class != FABRIC_MAIN
    {
        return Err(QuickPlayError::UnsupportedRuntime);
    }
    if spec.game_args.iter().any(|arg| conflicting_argument(arg))
        || conflicting_jvm_arguments(&spec.jvm_args, &instance.natives_dir())
    {
        return Err(QuickPlayError::ConflictingArguments);
    }
    validate_installed_runtime(instance, spec)?;
    spec.game_args
        .extend(["--quickPlayMultiplayer".into(), endpoint.to_string()]);
    Ok(())
}

fn conflicting_argument(value: &str) -> bool {
    if value.chars().any(char::is_control) {
        return true;
    }
    let value = value.trim();
    if value == "--" || value.starts_with('@') {
        // An option terminator would turn our appended pair into positionals.
        return true;
    }
    if !value.starts_with('-') {
        return false;
    }
    let name = value
        .trim_start_matches('-')
        .split(['=', ' ', '\t'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    // Also refuse abbreviations of the protected options. Parsers may accept
    // them even though the installed manifest normally uses the complete flag.
    !name.is_empty()
        && (name.starts_with("quickplay")
            || name.starts_with("remotequickplay")
            || [
                "quickplay",
                "remotequickplay",
                "server",
                "port",
                "username",
                "version",
                "gamedir",
                "assetsdir",
                "assetindex",
                "uuid",
                "accesstoken",
                "usertype",
            ]
            .iter()
            .any(|protected| protected.starts_with(&name)))
}

fn conflicting_jvm_arguments(args: &[String], natives: &std::path::Path) -> bool {
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if !argument.starts_with('-') || argument.chars().any(char::is_control) {
            return true;
        }
        let name = argument.split('=').next().unwrap_or_default();
        if name == "-Djava.library.path"
            && argument.strip_prefix("-Djava.library.path=")
                != Some(natives.to_string_lossy().as_ref())
        {
            // Mojang's option may name only the exact resolved native directory.
            return true;
        }
        if [
            "-Daurora.",
            "-javaagent:",
            "-agentlib:",
            "-agentpath:",
            "-Xrun",
            "-Xbootclasspath",
            "-XX:VMOptionsFile=",
            "-XX:Flags=",
            "-XX:OnError=",
            "-XX:OnOutOfMemoryError=",
        ]
        .iter()
        .any(|prefix| argument.starts_with(prefix))
            || matches!(
                name,
                "-Djava.system.class.loader"
                    | "-Djava.class.path"
                    | "-Dsun.java.command"
                    | "-Djdk.module.main"
                    | "-Dfabric.addMods"
                    | "-Dfabric.gameJarPath"
                    | "-Dfabric.classPathGroups"
                    | "-Dfabric.remapClasspathFile"
                    | "-Dorg.lwjgl.librarypath"
                    | "-Dorg.lwjgl.system.SharedLibraryExtractPath"
                    | "-Djava.io.tmpdir"
                    | "-Duser.dir"
                    | "-Duser.home"
                    | "--patch-module"
            )
        {
            // Profile data cannot replace the reviewed entry point or Aurora's
            // IPC properties. Trusted Aurora properties are added by the caller
            // only after this check, just as the engine supplies its own -cp.
            return true;
        }
        if matches!(
            name,
            "-jar"
                | "-m"
                | "--module"
                | "--source"
                | "--"
                | "-cp"
                | "-classpath"
                | "--class-path"
                | "-p"
                | "--module-path"
                | "--upgrade-module-path"
                | "-version"
                | "--version"
                | "-help"
                | "--help"
                | "-?"
                | "--dry-run"
                | "--list-modules"
                | "--describe-module"
                | "-d"
        ) {
            // A different entry point, classpath or exit-only mode would make
            // the main_class/profile checks above misleading for this action.
            return true;
        }
        if matches!(
            argument.as_str(),
            "--add-opens"
                | "--add-exports"
                | "--add-reads"
                | "--enable-native-access"
                | "--limit-modules"
        ) {
            index += 1;
            if args.get(index).is_none_or(|value| {
                value.is_empty()
                    || value.starts_with(['-', '@'])
                    || value.chars().any(char::is_control)
            }) {
                return true;
            }
        }
        index += 1;
    }
    false
}

fn read_profile(instance: &Instance, id: &str) -> Result<Value, QuickPlayError> {
    let path = recovery::path(instance.root(), &format!("versions/{id}/{id}.json"))
        .map_err(|_| QuickPlayError::InvalidProfile)?;
    let bytes = recovery::read_bounded(&path, MAX_PROFILE_BYTES)
        .map_err(|_| QuickPlayError::InvalidProfile)?;
    serde_json::from_slice(&bytes).map_err(|_| QuickPlayError::InvalidProfile)
}

fn validate_installed_runtime(
    instance: &Instance,
    spec: &VersionLaunchSpec,
) -> Result<(), QuickPlayError> {
    let child = read_profile(instance, CONNECT_QUICK_PLAY_VERSION_ID)?;
    let base = read_profile(instance, CONNECT_QUICK_PLAY_MINECRAFT_VERSION)?;
    if child["id"] != CONNECT_QUICK_PLAY_VERSION_ID
        || child["inheritsFrom"] != CONNECT_QUICK_PLAY_MINECRAFT_VERSION
        || child["mainClass"] != FABRIC_MAIN
        || child.get("jar").is_some()
        || child.get("javaVersion").is_some_and(|java| {
            java["majorVersion"].as_u64() != Some(CONNECT_QUICK_PLAY_JAVA_MAJOR.into())
        })
        || base["id"] != CONNECT_QUICK_PLAY_MINECRAFT_VERSION
        || base["mainClass"] != VANILLA_MAIN
        || base.get("inheritsFrom").is_some()
        || base.get("jar").is_some()
        || base
            .pointer("/javaVersion/majorVersion")
            .and_then(Value::as_u64)
            != Some(CONNECT_QUICK_PLAY_JAVA_MAJOR.into())
    {
        return Err(QuickPlayError::UnsupportedRuntime);
    }
    let libraries = child["libraries"]
        .as_array()
        .ok_or(QuickPlayError::InvalidProfile)?;
    let loaders: Vec<_> = libraries
        .iter()
        .filter(|library| {
            library["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("net.fabricmc:fabric-loader:"))
        })
        .collect();
    if loaders.len() != 1 || loaders[0]["name"] != LOADER_COORDINATE {
        return Err(QuickPlayError::UnsupportedRuntime);
    }
    if loaders[0]
        .pointer("/downloads/artifact/path")
        .is_some_and(|path| path.as_str() != Some(LOADER_PATH.trim_start_matches("libraries/")))
    {
        return Err(QuickPlayError::UnsupportedRuntime);
    }
    for profile in [&base, &child] {
        if profile["libraries"].as_array().is_some_and(|libraries| {
            libraries.iter().any(|library| {
                library["name"].as_str().is_some_and(|name| {
                    name.starts_with("net.minecraftforge:")
                        || name.starts_with("net.neoforged:")
                        || (name.starts_with("net.fabricmc:fabric-loader:")
                            && name != LOADER_COORDINATE)
                })
            })
        }) {
            return Err(QuickPlayError::UnsupportedRuntime);
        }
    }
    let client =
        recovery::path(instance.root(), CLIENT_PATH).map_err(|_| QuickPlayError::InvalidProfile)?;
    let loader =
        recovery::path(instance.root(), LOADER_PATH).map_err(|_| QuickPlayError::InvalidProfile)?;
    if spec.client_jar != client
        || spec
            .libraries
            .iter()
            .filter(|path| *path == &loader)
            .count()
            != 1
    {
        return Err(QuickPlayError::UnsupportedRuntime);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{
        resolve_launch_spec, InstanceId, InstanceLayout, JavaRuntime, LaunchCommand,
        LaunchIdentity, LauncherEngine,
    };
    use serde_json::json;
    use std::{fs, path::PathBuf};

    struct Fixture {
        root: PathBuf,
        instance: Instance,
        java: JavaRuntime,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "aurora connect quick-play {}",
                uuid::Uuid::new_v4()
            ));
            let instance =
                InstanceLayout::new(&root).open(InstanceId::parse("quick-play-fixture").unwrap());
            instance.ensure_layout().unwrap();
            let java_path = root.join("java fixture.exe");
            fs::write(&java_path, b"not executed").unwrap();
            let java = JavaRuntime::from_executable(java_path).unwrap();
            let fixture = Self {
                root,
                instance,
                java,
            };
            fixture.write_profile(
                CONNECT_QUICK_PLAY_MINECRAFT_VERSION,
                &json!({
                    "id":"1.21.1", "mainClass":VANILLA_MAIN,
                    "javaVersion":{"component":"java-runtime-delta","majorVersion":21},
                    "assetIndex":{"id":"17"},
                    "libraries":[{"name":"example:vanilla:1.0"}],
                    "arguments":{
                        "jvm":["-Dfixture.path=${natives_directory}","-cp","${classpath}"],
                        "game":["--username","${auth_player_name}","--width","960",
                            {"rules":[{"action":"allow","features":{"is_quick_play_multiplayer":true}}],"value":["--quickPlayMultiplayer","${quickPlayMultiplayer}"]},
                            {"rules":[{"action":"allow","features":{"has_quick_plays_support":true}}],"value":["--quickPlayPath","${quickPlayPath}"]},
                            {"rules":[{"action":"allow","features":{"unknown_future_feature":true}}],"value":"--futureFeature"}
                        ]
                    }
                }),
            );
            fixture.write_profile(
                CONNECT_QUICK_PLAY_VERSION_ID,
                &json!({
                    "id":CONNECT_QUICK_PLAY_VERSION_ID,"inheritsFrom":"1.21.1",
                    "mainClass":FABRIC_MAIN,
                    "libraries":[{"name":LOADER_COORDINATE}],
                    "arguments":{"game":["--height","540"]}
                }),
            );
            for path in [
                CLIENT_PATH,
                LOADER_PATH,
                "libraries/example/vanilla/1.0/vanilla-1.0.jar",
            ] {
                let file = fixture.instance.root().join(path);
                fs::create_dir_all(file.parent().unwrap()).unwrap();
                fs::write(file, b"fixture jar, never executed").unwrap();
            }
            fixture
        }
        fn profile_path(&self, id: &str) -> PathBuf {
            self.instance
                .root()
                .join(format!("versions/{id}/{id}.json"))
        }
        fn write_profile(&self, id: &str, value: &Value) {
            let path = self.profile_path(id);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
        }
        fn change_profile(&self, id: &str, change: impl FnOnce(&mut Value)) {
            let mut value: Value =
                serde_json::from_slice(&fs::read(self.profile_path(id)).unwrap()).unwrap();
            change(&mut value);
            self.write_profile(id, &value);
        }
        fn spec(&self) -> VersionLaunchSpec {
            resolve_launch_spec(&self.instance, CONNECT_QUICK_PLAY_VERSION_ID).unwrap()
        }
        fn command(&self, spec: &VersionLaunchSpec, identity: &LaunchIdentity) -> LaunchCommand {
            LauncherEngine
                .prepare_launch(&self.instance, &self.java, identity, spec, 4096)
                .unwrap()
                .command
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn endpoint() -> SocketAddr {
        "127.0.0.1:42010".parse().unwrap()
    }
    fn identity() -> LaunchIdentity {
        LaunchIdentity::offline("FixturePlayer", uuid::Uuid::nil())
    }
    fn assert_refused_unchanged(
        fixture: &Fixture,
        mut spec: VersionLaunchSpec,
        major: u32,
        endpoint: SocketAddr,
        expected: QuickPlayError,
    ) {
        let before = fixture.command(&spec, &identity()).args;
        assert_eq!(
            apply_guest_quick_play(&fixture.instance, &mut spec, major, endpoint),
            Err(expected)
        );
        assert_eq!(fixture.command(&spec, &identity()).args, before);
    }

    #[test]
    fn connect_quick_play_composes_real_resolver_and_engine_without_enabling_other_features() {
        let fixture = Fixture::new();
        let before_base = fs::read(fixture.profile_path("1.21.1")).unwrap();
        let before_child = fs::read(fixture.profile_path(CONNECT_QUICK_PLAY_VERSION_ID)).unwrap();
        let original = fixture.spec();
        let baseline = fixture.command(&original, &identity());
        assert_eq!(original.game_args, ["--width", "960", "--height", "540"]);
        let mut selected = original.clone();
        apply_guest_quick_play(&fixture.instance, &mut selected, 21, endpoint()).unwrap();
        let command = fixture.command(&selected, &identity());
        let index = command
            .args
            .iter()
            .position(|arg| arg == "--quickPlayMultiplayer")
            .unwrap();
        assert_eq!(command.args[index + 1], "127.0.0.1:42010");
        assert_eq!(
            command
                .args
                .iter()
                .filter(|arg| *arg == "--quickPlayMultiplayer")
                .count(),
            1
        );
        let mut stripped = command.args.clone();
        stripped.drain(index..index + 2);
        assert_eq!(stripped, baseline.args);
        assert_eq!(command.executable, baseline.executable);
        assert_eq!(command.working_dir, baseline.working_dir);
        assert_eq!(command.environment, baseline.environment);
        assert!(!command
            .args
            .iter()
            .any(|arg| arg == "--quickPlayPath" || arg == "--futureFeature"));
        assert_eq!(
            fixture.command(&fixture.spec(), &identity()).args,
            baseline.args
        );
        assert_eq!(
            fs::read(fixture.profile_path("1.21.1")).unwrap(),
            before_base
        );
        assert_eq!(
            fs::read(fixture.profile_path(CONNECT_QUICK_PLAY_VERSION_ID)).unwrap(),
            before_child
        );
    }

    #[test]
    fn connect_quick_play_endpoint_is_exact_ipv4_loopback_and_not_shell_input() {
        let fixture = Fixture::new();
        for value in [
            "0.0.0.0:42010",
            "127.0.0.2:42010",
            "192.168.1.1:42010",
            "8.8.8.8:42010",
            "[::1]:42010",
            "[::ffff:127.0.0.1]:42010",
            "127.0.0.1:0",
        ] {
            assert_refused_unchanged(
                &fixture,
                fixture.spec(),
                21,
                value.parse().unwrap(),
                QuickPlayError::InvalidEndpoint,
            );
        }
        for value in [
            "localhost:42010",
            "127.0.0.1:42010;whoami",
            "127.0.0.1:42010 --server example",
            "127.0.0.1:42010\nstop",
            "$(whoami)",
        ] {
            assert!(value.parse::<SocketAddr>().is_err());
        }
        for port in [1, 65535] {
            let mut spec = fixture.spec();
            apply_guest_quick_play(
                &fixture.instance,
                &mut spec,
                21,
                SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
            )
            .unwrap();
            let command = fixture.command(&spec, &identity());
            assert!(command
                .args
                .windows(2)
                .any(|args| args == ["--quickPlayMultiplayer", &format!("127.0.0.1:{port}")]));
        }
    }

    #[test]
    fn connect_quick_play_rejects_resolved_conflicts_from_either_profile_without_mutation() {
        let fixture = Fixture::new();
        for conflict in [
            "--quickPlayMultiplayer",
            "--quickPlayMultiplayer=example:25565",
            "--quickPlaySingleplayer",
            "--quickPlayRealms",
            "--quickPlayPath",
            "--remoteQuickPlay",
            "--server",
            "--server=example",
            "--port",
            "--port=25565",
            "-server",
            "--QUICKPLAYMULTIPLAYER",
            "--quickPlayFuture",
            "--quick",
            "--serv",
            "--por",
            "--",
            "--server example",
            "--width\0",
            "--gameDir",
            "--assetsDir=outside",
            "--assetIndex",
            "--username",
            "--uuid",
            "--accessToken=untrusted",
            "--userType",
            "--version",
            "@arguments.txt",
        ] {
            let mut spec = fixture.spec();
            spec.game_args
                .extend([conflict.into(), "untrusted-value".into()]);
            assert_refused_unchanged(
                &fixture,
                spec,
                21,
                endpoint(),
                QuickPlayError::ConflictingArguments,
            );
        }
        for profile in [CONNECT_QUICK_PLAY_VERSION_ID, "1.21.1"] {
            let fixture = Fixture::new();
            fixture.change_profile(profile, |json| {
                json["arguments"]["game"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("--server=example"))
            });
            assert_refused_unchanged(
                &fixture,
                fixture.spec(),
                21,
                endpoint(),
                QuickPlayError::ConflictingArguments,
            );
        }
        let mut spec = fixture.spec();
        apply_guest_quick_play(&fixture.instance, &mut spec, 21, endpoint()).unwrap();
        assert_refused_unchanged(
            &fixture,
            spec,
            21,
            endpoint(),
            QuickPlayError::ConflictingArguments,
        );
    }

    #[test]
    fn connect_quick_play_requires_exact_effective_profile_and_observed_java_21() {
        let fixture = Fixture::new();
        for major in [0, 8, 17, 20, 22, 25] {
            assert_refused_unchanged(
                &fixture,
                fixture.spec(),
                major,
                endpoint(),
                QuickPlayError::UnsupportedRuntime,
            );
        }
        for version in [
            "1.21.1",
            "fabric-loader-0.16.13-1.21.1",
            "fabric-loader-0.16.14-1.21.2",
            "../fabric-loader-0.16.14-1.21.1",
            "fabric-loader-0.16.14-1.21.1 --server example",
        ] {
            let mut spec = fixture.spec();
            spec.version_name = version.into();
            assert_refused_unchanged(
                &fixture,
                spec,
                21,
                endpoint(),
                QuickPlayError::UnsupportedRuntime,
            );
        }
        let mut spec = fixture.spec();
        spec.main_class = VANILLA_MAIN.into();
        assert_refused_unchanged(
            &fixture,
            spec,
            21,
            endpoint(),
            QuickPlayError::UnsupportedRuntime,
        );
        let mut spec = fixture.spec();
        spec.libraries
            .retain(|path| !path.ends_with("fabric-loader-0.16.14.jar"));
        assert_refused_unchanged(
            &fixture,
            spec,
            21,
            endpoint(),
            QuickPlayError::UnsupportedRuntime,
        );
        let mut spec = fixture.spec();
        spec.libraries
            .push(fixture.instance.root().join(LOADER_PATH));
        assert_refused_unchanged(
            &fixture,
            spec,
            21,
            endpoint(),
            QuickPlayError::UnsupportedRuntime,
        );
        let replacement = fixture.instance.root().join("versions/1.21.1/other.jar");
        fs::write(&replacement, b"wrong client").unwrap();
        let mut spec = fixture.spec();
        spec.client_jar = replacement;
        assert_refused_unchanged(
            &fixture,
            spec,
            21,
            endpoint(),
            QuickPlayError::UnsupportedRuntime,
        );
    }

    #[test]
    fn connect_quick_play_refuses_jvm_entry_overrides_but_keeps_safe_option_pairs() {
        let fixture = Fixture::new();
        for extra in [
            vec!["-jar", "other.jar"],
            vec!["-m", "other/Main"],
            vec!["--module=other/Main"],
            vec!["--source", "21", "Other.java"],
            vec!["example.OtherMain"],
            vec!["@arguments.txt"],
            vec!["--class-path=other.jar"],
            vec!["--module-path", "other"],
            vec!["-version"],
            vec!["--dry-run"],
            vec!["--add-opens"],
            vec!["--add-opens", "--module=other/Main"],
            vec!["-Daurora.session.nonce=untrusted"],
            vec!["-Daurora.ipc.port=12345"],
            vec!["-javaagent:other.jar"],
            vec!["-agentpath:other.dll"],
            vec!["-agentlib:jdwp=transport=dt_socket,server=y"],
            vec!["-Xrunjdwp:transport=dt_socket,server=y"],
            vec!["-Xbootclasspath/a:other.jar"],
            vec!["--patch-module=java.base=other.jar"],
            vec!["-Djava.system.class.loader=example.OtherLoader"],
            vec!["-Djava.class.path=other.jar"],
            vec!["-Dfabric.addMods=other.jar"],
            vec!["-Dfabric.gameJarPath=other.jar"],
            vec!["-XX:VMOptionsFile=options.txt"],
            vec!["-XX:Flags=options.txt"],
            vec!["-XX:OnError=untrusted command"],
            vec!["-XX:OnOutOfMemoryError=untrusted command"],
            vec!["-Djava.library.path=outside"],
            vec!["-Djava.library.path"],
            vec!["-Dorg.lwjgl.librarypath=outside"],
            vec!["-Dorg.lwjgl.system.SharedLibraryExtractPath=outside"],
            vec!["-Djava.io.tmpdir=outside"],
            vec!["-Duser.dir=outside"],
            vec!["-Duser.home=outside"],
        ] {
            let mut spec = fixture.spec();
            spec.jvm_args.extend(extra.into_iter().map(str::to_owned));
            assert_refused_unchanged(
                &fixture,
                spec,
                21,
                endpoint(),
                QuickPlayError::ConflictingArguments,
            );
        }
        let mut spec = fixture.spec();
        spec.jvm_args.extend([
            format!(
                "-Djava.library.path={}",
                fixture.instance.natives_dir().display()
            ),
            "--add-opens".into(),
            "java.base/java.lang=ALL-UNNAMED".into(),
            "--add-exports=java.base/sun.security.util=ALL-UNNAMED".into(),
            "-Dfixture.message=one argument with spaces".into(),
        ]);
        let baseline = fixture.command(&spec, &identity());
        apply_guest_quick_play(&fixture.instance, &mut spec, 21, endpoint()).unwrap();
        let command = fixture.command(&spec, &identity());
        let at = command
            .args
            .iter()
            .position(|arg| arg == "--quickPlayMultiplayer")
            .unwrap();
        let mut without = command.args;
        without.drain(at..at + 2);
        assert_eq!(without, baseline.args);
    }

    #[test]
    fn connect_quick_play_rechecks_installed_chain_instead_of_trusting_display_names() {
        for change in 0..11 {
            let fixture = Fixture::new();
            let spec = fixture.spec();
            match change {
                0 => fixture
                    .change_profile(CONNECT_QUICK_PLAY_VERSION_ID, |v| v["id"] = json!("other")),
                1 => fixture.change_profile(CONNECT_QUICK_PLAY_VERSION_ID, |v| {
                    v["inheritsFrom"] = json!("1.20.1")
                }),
                2 => fixture.change_profile(CONNECT_QUICK_PLAY_VERSION_ID, |v| {
                    v["mainClass"] = json!(VANILLA_MAIN)
                }),
                3 => fixture.change_profile(CONNECT_QUICK_PLAY_VERSION_ID, |v| {
                    v["javaVersion"] = json!({"majorVersion":17})
                }),
                4 => fixture.change_profile(CONNECT_QUICK_PLAY_VERSION_ID, |v| {
                    v["libraries"][0]["name"] = json!("net.fabricmc:fabric-loader:0.16.13")
                }),
                5 => fixture.change_profile("1.21.1", |v| v["id"] = json!("1.21.2")),
                6 => fixture
                    .change_profile("1.21.1", |v| v["javaVersion"]["majorVersion"] = json!(17)),
                7 => fixture.change_profile("1.21.1", |v| v["inheritsFrom"] = json!("other")),
                8 => fixture.change_profile("1.21.1", |v| v["jar"] = json!("other")),
                9 => fixture.change_profile(CONNECT_QUICK_PLAY_VERSION_ID, |v| {
                    v["libraries"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"name":"net.minecraftforge:forge:1.21.1"}))
                }),
                10 => fixture.change_profile(CONNECT_QUICK_PLAY_VERSION_ID, |v| {
                    v["libraries"][0]["downloads"] = json!({"artifact":{"path":"other.jar"}})
                }),
                _ => unreachable!(),
            }
            assert_refused_unchanged(
                &fixture,
                spec,
                21,
                endpoint(),
                QuickPlayError::UnsupportedRuntime,
            );
        }
    }

    #[test]
    fn connect_quick_play_profile_reads_are_bounded_and_refuse_hardlinks() {
        let fixture = Fixture::new();
        let spec = fixture.spec();
        let child = fixture.profile_path(CONNECT_QUICK_PLAY_VERSION_ID);
        let original = fs::read(&child).unwrap();
        fs::write(&child, vec![b' '; MAX_PROFILE_BYTES as usize + 1]).unwrap();
        assert_refused_unchanged(
            &fixture,
            spec.clone(),
            21,
            endpoint(),
            QuickPlayError::InvalidProfile,
        );
        fs::write(&child, b"not json").unwrap();
        assert_refused_unchanged(
            &fixture,
            spec.clone(),
            21,
            endpoint(),
            QuickPlayError::InvalidProfile,
        );
        fs::write(&child, &original).unwrap();
        let alias = fixture.root.join("profile hardlink.json");
        fs::hard_link(&child, &alias).unwrap();
        assert_refused_unchanged(
            &fixture,
            spec.clone(),
            21,
            endpoint(),
            QuickPlayError::InvalidProfile,
        );
        fs::remove_file(alias).unwrap();
        fs::remove_file(child).unwrap();
        assert_refused_unchanged(
            &fixture,
            spec,
            21,
            endpoint(),
            QuickPlayError::InvalidProfile,
        );
    }

    #[test]
    fn connect_quick_play_preserves_identity_ipc_skins_and_token_redaction() {
        let fixture = Fixture::new();
        let mut spec = fixture.spec();
        let mut with_quick_play = spec.clone();
        apply_guest_quick_play(&fixture.instance, &mut with_quick_play, 21, endpoint()).unwrap();
        spec.aurora_ipc_port = Some(41000);
        spec.aurora_session_nonce = Some("fixture-nonce".into());
        spec.jvm_args.extend([
            "-Daurora.loader=fabric".into(),
            "-Daurora.skins.root=fixture skins".into(),
        ]);
        // These are caller-owned properties, never inherited from the profile.
        with_quick_play.aurora_ipc_port = spec.aurora_ipc_port;
        with_quick_play.aurora_session_nonce = spec.aurora_session_nonce.clone();
        with_quick_play.jvm_args = spec.jvm_args.clone();
        let identity = LaunchIdentity {
            username: "FixtureOnline".into(),
            uuid: uuid::Uuid::nil(),
            access_token: "fixture-token-not-credential".into(),
            user_type: "msa".into(),
        };
        let baseline = fixture.command(&spec, &identity);
        let command = fixture.command(&with_quick_play, &identity);
        let at = command
            .args
            .iter()
            .position(|arg| arg == "--quickPlayMultiplayer")
            .unwrap();
        let mut without = command.args.clone();
        without.drain(at..at + 2);
        assert_eq!(without, baseline.args);
        assert!(!command
            .redacted_args()
            .iter()
            .any(|value| value == "fixture-token-not-credential"));
        assert!(command
            .args
            .iter()
            .any(|arg| arg == "-Daurora.skins.root=fixture skins"));
    }
}
