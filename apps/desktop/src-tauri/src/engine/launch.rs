use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Child;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::launch_settings::{EffectiveLaunchSettings, LaunchSettingsError};
use super::process::background_command;
use super::{Instance, InstanceError, JavaRuntime};

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("não foi possível preparar a instância: {0}")]
    Instance(#[from] InstanceError),
    #[error("arquivo obrigatório não encontrado: {0}")]
    MissingFile(PathBuf),
    #[error("o classpath está vazio")]
    EmptyClasspath,
    #[error("a propriedade JVM não pode ser vazia")]
    InvalidJvmProperty,
    #[error(transparent)]
    InvalidSettings(#[from] LaunchSettingsError),
    #[error("erro de E/S ao iniciar a JVM: {0}")]
    Io(#[from] std::io::Error),
}

/// Identidade já autenticada por outro módulo. A senha nunca chega ao core.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaunchIdentity {
    pub username: String,
    pub uuid: Uuid,
    /// Credencial de sessão opaca. Nunca a registrar em logs.
    pub access_token: String,
    pub user_type: String,
}

impl LaunchIdentity {
    /// Offline identity understood by legacy and modern clients. A fabricated
    /// bearer token can be interpreted as a valid Microsoft session with no
    /// multiplayer privileges, which disables the button on Minecraft 1.16.x.
    pub fn offline(username: impl Into<String>, uuid: Uuid) -> Self {
        Self {
            username: username.into(),
            uuid,
            access_token: "0".to_owned(),
            user_type: "legacy".to_owned(),
        }
    }
}

/// Resultado do resolvedor de versão (vanilla ou mod loader).
#[derive(Clone, Debug)]
pub struct VersionLaunchSpec {
    pub version_name: String,
    pub main_class: String,
    pub client_jar: PathBuf,
    pub libraries: Vec<PathBuf>,
    pub assets_dir: PathBuf,
    pub asset_index: String,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    /// Porta loopback do Aurora Core. O token de sessão nunca é colocado aqui.
    pub aurora_ipc_port: Option<u16>,
    /// Nonce efêmero do IPC. Não é uma credencial Firebase e só vale nesta execução.
    pub aurora_session_nonce: Option<String>,
}

impl VersionLaunchSpec {
    /// O authlib 2.1.28 do Minecraft 1.16.5 interpreta a resposta anônima atual
    /// de `/privileges` como todos os privilégios desativados. Ao tornar apenas
    /// o endpoint de serviços indisponível para a sessão offline, o próprio jogo
    /// usa `OfflineSocialInteractions`, que mantém multiplayer LAN/offline ativo.
    pub fn apply_offline_compatibility(&mut self, minecraft_version: &str) {
        if minecraft_version != "1.16.5" {
            return;
        }

        self.jvm_args.extend([
            "-Dminecraft.api.auth.host=https://authserver.mojang.com".into(),
            "-Dminecraft.api.account.host=https://api.mojang.com".into(),
            "-Dminecraft.api.session.host=https://sessionserver.mojang.com".into(),
            "-Dminecraft.api.services.host=http://127.0.0.1:0".into(),
        ]);
    }
}

#[derive(Clone, Debug)]
pub struct LaunchCommand {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
    pub environment: BTreeMap<String, String>,
}

impl LaunchCommand {
    /// Versão segura para UI/diagnóstico: não revela a credencial da sessão.
    pub fn redacted_args(&self) -> Vec<String> {
        let mut args = self.args.clone();
        for index in 0..args.len().saturating_sub(1) {
            if args[index] == "--accessToken" {
                args[index + 1] = "[redacted]".into();
            }
        }
        for arg in &mut args {
            if arg.starts_with("-Daurora.session.nonce=") {
                *arg = "-Daurora.session.nonce=[redacted]".into();
            }
        }
        args
    }
}

pub struct PreparedLaunch {
    pub command: LaunchCommand,
}

impl PreparedLaunch {
    pub fn spawn(self) -> Result<Child, LaunchError> {
        let mut command = background_command(&self.command.executable);
        command
            .args(&self.command.args)
            .current_dir(&self.command.working_dir);
        configure_java_environment(&mut command, &self.command.environment);
        #[cfg(target_os = "windows")]
        configure_windows_game_output(&mut command, &self.command.working_dir)?;
        Ok(command.spawn()?)
    }
}

fn configure_java_environment(
    command: &mut std::process::Command,
    environment: &BTreeMap<String, String>,
) {
    command.envs(environment);
    // These variables can inject unvalidated JVM arguments or a classpath even
    // when the launcher never invokes a shell. Keep this process isolated from
    // system-wide Java overrides without changing the parent's environment.
    for variable in [
        "JAVA_TOOL_OPTIONS",
        "JDK_JAVA_OPTIONS",
        "_JAVA_OPTIONS",
        "CLASSPATH",
    ] {
        command.env_remove(variable);
    }
}

#[cfg(target_os = "windows")]
fn configure_windows_game_output(
    command: &mut std::process::Command,
    working_dir: &Path,
) -> std::io::Result<()> {
    use std::fs::{self, OpenOptions};
    use std::io::{Error, ErrorKind};
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::process::Stdio;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    let logs = working_dir.join("logs");
    match fs::create_dir(&logs) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let directory = fs::symlink_metadata(&logs)?;
    if !directory.is_dir() || directory.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "pasta de logs inválida",
        ));
    }

    // Keep diagnostics from JVM/bootstrap failures even without a console.
    // Never write the command line (which can contain credentials) to this log.
    // Open without following a file reparse point and validate before truncating.
    let log = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(logs.join("aurora-java.log"))?;
    let metadata = log.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "arquivo de log inválido",
        ));
    }
    log.set_len(0)?;
    command
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    Ok(())
}

#[derive(Default)]
pub struct LauncherEngine;

impl LauncherEngine {
    /// Cria a estrutura da instância gerenciada.
    pub fn prepare_instance(&self, instance: &Instance) -> Result<(), LaunchError> {
        instance.ensure_layout()?;
        Ok(())
    }

    pub fn prepare_launch(
        &self,
        instance: &Instance,
        java: &JavaRuntime,
        identity: &LaunchIdentity,
        spec: &VersionLaunchSpec,
        max_memory_mb: u32,
    ) -> Result<PreparedLaunch, LaunchError> {
        self.prepare_launch_with_settings(
            instance,
            java,
            identity,
            spec,
            &EffectiveLaunchSettings {
                min_memory_mb: None,
                max_memory_mb,
                jvm_args: Vec::new(),
            },
        )
    }

    pub fn prepare_launch_with_settings(
        &self,
        instance: &Instance,
        java: &JavaRuntime,
        identity: &LaunchIdentity,
        spec: &VersionLaunchSpec,
        settings: &EffectiveLaunchSettings,
    ) -> Result<PreparedLaunch, LaunchError> {
        settings.validate()?;
        instance.ensure_layout()?;
        require_file(&spec.client_jar)?;
        let classpath = build_classpath(&spec.libraries, &spec.client_jar)?;
        let natives = instance.natives_dir();

        let mut args = Vec::new();
        let custom_collector = settings
            .jvm_args
            .iter()
            .any(|arg| arg.starts_with("-XX:+") && is_collector_argument(arg));
        // A version/loader profile must not silently override the user's RAM
        // selection. The engine owns both heap bounds and emits each once.
        args.extend(
            spec.jvm_args
                .iter()
                .filter(|arg| {
                    !(is_managed_heap_argument(arg)
                        || custom_collector && is_collector_argument(arg))
                })
                .cloned(),
        );
        args.extend(settings.jvm_args.iter().cloned());
        if let Some(min_memory_mb) = settings.min_memory_mb {
            args.push(format!("-Xms{min_memory_mb}M"));
        }
        args.push(format!("-Xmx{}M", settings.max_memory_mb));
        args.push(format!("-Djava.library.path={}", natives.display()));
        if let Some(port) = spec.aurora_ipc_port {
            args.push(format!("-Daurora.ipc.port={port}"));
            args.push(format!("-Daurora.minecraft.version={}", spec.version_name));
        }
        if let Some(nonce) = &spec.aurora_session_nonce {
            args.push(format!("-Daurora.session.nonce={nonce}"));
        }
        args.push("-cp".into());
        args.push(classpath);
        args.push(spec.main_class.clone());
        args.extend(spec.game_args.iter().cloned());
        args.extend([
            "--username".into(),
            identity.username.clone(),
            "--version".into(),
            spec.version_name.clone(),
            "--gameDir".into(),
            instance.root().display().to_string(),
            "--assetsDir".into(),
            spec.assets_dir.display().to_string(),
            "--assetIndex".into(),
            spec.asset_index.clone(),
            "--uuid".into(),
            identity.uuid.to_string(),
            "--accessToken".into(),
            identity.access_token.clone(),
            "--userType".into(),
            identity.user_type.clone(),
        ]);

        let mut environment = BTreeMap::new();
        if let Some(home) = java.home() {
            environment.insert("JAVA_HOME".into(), home.display().to_string());
        }

        Ok(PreparedLaunch {
            command: LaunchCommand {
                executable: java.executable().to_path_buf(),
                args,
                working_dir: instance.root().to_path_buf(),
                environment,
            },
        })
    }
}

fn is_managed_heap_argument(argument: &str) -> bool {
    argument.starts_with("-Xmx")
        || argument.starts_with("-Xms")
        || [
            "MaxHeapSize",
            "InitialHeapSize",
            "MinHeapSize",
            "MaxRAM",
            "MaxRAMPercentage",
            "MinRAMPercentage",
            "InitialRAMPercentage",
            "MaxRAMFraction",
            "MinRAMFraction",
            "InitialRAMFraction",
            "ErgoHeapSizeLimit",
            "SoftMaxHeapSize",
        ]
        .iter()
        .any(|name| {
            argument
                .strip_prefix("-XX:")
                .and_then(|value| value.strip_prefix(name))
                .is_some_and(|suffix| suffix.starts_with('='))
        })
}

fn is_collector_argument(argument: &str) -> bool {
    matches!(
        argument
            .strip_prefix("-XX:+")
            .or_else(|| argument.strip_prefix("-XX:-")),
        Some(
            "UseG1GC"
                | "UseParallelGC"
                | "UseParallelOldGC"
                | "UseSerialGC"
                | "UseZGC"
                | "UseShenandoahGC"
                | "UseConcMarkSweepGC"
                | "UseParNewGC"
                | "UseEpsilonGC"
        )
    )
}

fn require_file(path: &Path) -> Result<(), LaunchError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(LaunchError::MissingFile(path.to_path_buf()))
    }
}

fn build_classpath(libraries: &[PathBuf], client_jar: &Path) -> Result<String, LaunchError> {
    if libraries.is_empty() {
        return Err(LaunchError::EmptyClasspath);
    }
    for library in libraries {
        require_file(library)?;
    }
    let paths = libraries
        .iter()
        .cloned()
        .chain(std::iter::once(client_jar.to_path_buf()));
    std::env::join_paths(paths)
        .map_err(|_| LaunchError::InvalidJvmProperty)
        .map(|classpath| classpath.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch_spec() -> VersionLaunchSpec {
        VersionLaunchSpec {
            version_name: "test".into(),
            main_class: "example.Main".into(),
            client_jar: "client.jar".into(),
            libraries: vec!["library.jar".into()],
            assets_dir: "assets".into(),
            asset_index: "index".into(),
            jvm_args: Vec::new(),
            game_args: Vec::new(),
            aurora_ipc_port: None,
            aurora_session_nonce: None,
        }
    }

    #[test]
    fn redacts_access_token() {
        let command = LaunchCommand {
            executable: "java".into(),
            args: vec![
                "-Daurora.session.nonce=ephemeral-secret".into(),
                "--accessToken".into(),
                "secret".into(),
            ],
            working_dir: PathBuf::new(),
            environment: BTreeMap::new(),
        };
        assert_eq!(
            command.redacted_args(),
            vec![
                "-Daurora.session.nonce=[redacted]",
                "--accessToken",
                "[redacted]",
            ]
        );
    }

    fn launch_fixture() -> (PathBuf, Instance, JavaRuntime, VersionLaunchSpec) {
        let root = std::env::temp_dir().join(format!("aurora launch settings {}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let instance = super::super::InstanceLayout::new(&root)
            .open(super::super::InstanceId::parse("settings-test").unwrap());
        let java = JavaRuntime::from_executable(std::env::current_exe().unwrap()).unwrap();
        let mut spec = launch_spec();
        spec.client_jar = root.join("client.jar");
        spec.libraries = vec![root.join("library.jar")];
        std::fs::write(&spec.client_jar, b"fixture").unwrap();
        std::fs::write(&spec.libraries[0], b"fixture").unwrap();
        (root, instance, java, spec)
    }

    #[test]
    fn engine_owns_heap_bounds_and_preserves_loader_and_session_arguments() {
        let (root, instance, java, mut spec) = launch_fixture();
        spec.jvm_args = vec![
            "-Xmx128M".into(),
            "-Xms128M".into(),
            "-XX:MaxHeapSize=256M".into(),
            "-XX:InitialHeapSize=256M".into(),
            "-XX:MinHeapSize=256M".into(),
            "-XX:MaxRAMPercentage=90".into(),
            "-XX:MaxRAMFraction=1".into(),
            "-XX:+UseParallelGC".into(),
            "-Dloader.fixture=true".into(),
        ];
        spec.aurora_ipc_port = Some(12345);
        spec.aurora_session_nonce = Some("test-ephemeral".into());
        let identity = LaunchIdentity::offline("FixturePlayer", Uuid::nil());
        let settings = EffectiveLaunchSettings {
            min_memory_mb: Some(1024),
            max_memory_mb: 6144,
            jvm_args: vec!["-XX:+UseG1GC".into(), "-Duser.language=pt".into()],
        };
        let command = LauncherEngine
            .prepare_launch_with_settings(&instance, &java, &identity, &spec, &settings)
            .unwrap()
            .command;
        let args = &command.args;
        assert_eq!(args.iter().filter(|arg| arg.starts_with("-Xmx")).count(), 1);
        assert_eq!(args.iter().filter(|arg| arg.starts_with("-Xms")).count(), 1);
        assert!(args.contains(&"-Xmx6144M".to_owned()));
        assert!(args.contains(&"-Xms1024M".to_owned()));
        assert!(!args.iter().any(|arg| arg.starts_with("-XX:MaxHeapSize=")));
        assert!(!args
            .iter()
            .any(|arg| arg.starts_with("-XX:InitialHeapSize=")));
        assert!(!args.iter().any(|arg| arg.starts_with("-XX:MinHeapSize=")));
        assert!(!args.iter().any(|arg| arg.starts_with("-XX:MaxRAM")));
        assert!(!args.contains(&"-XX:+UseParallelGC".to_owned()));
        for expected in [
            "-Dloader.fixture=true",
            "-XX:+UseG1GC",
            "-Duser.language=pt",
            "-Daurora.ipc.port=12345",
            "-Daurora.session.nonce=test-ephemeral",
        ] {
            assert!(args.iter().any(|arg| arg == expected));
        }
        let classpath_index = args.iter().position(|arg| arg == "-cp").unwrap();
        assert!(args[classpath_index + 1].contains("aurora launch settings "));
        assert_eq!(args[classpath_index + 2], "example.Main");
        assert_eq!(command.working_dir, instance.root());
        assert_eq!(command.executable, java.executable());
        assert_eq!(
            args.iter()
                .filter(|arg| arg.as_str() == "--gameDir")
                .count(),
            1
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--gameDir"
                    && pair[1] == instance.root().display().to_string())
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_heap_bounds_fail_before_creating_instance_layout() {
        let (root, instance, java, spec) = launch_fixture();
        let result = LauncherEngine.prepare_launch_with_settings(
            &instance,
            &java,
            &LaunchIdentity::offline("FixturePlayer", Uuid::nil()),
            &spec,
            &EffectiveLaunchSettings {
                min_memory_mb: Some(8192),
                max_memory_mb: 4096,
                jvm_args: Vec::new(),
            },
        );
        assert!(matches!(result, Err(LaunchError::InvalidSettings(_))));
        assert!(!instance.root().exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_api_keeps_unspecified_initial_heap_and_requested_maximum() {
        let (root, instance, java, mut spec) = launch_fixture();
        spec.jvm_args.push("-XX:+UseSerialGC".into());
        let command = LauncherEngine
            .prepare_launch(
                &instance,
                &java,
                &LaunchIdentity::offline("FixturePlayer", Uuid::nil()),
                &spec,
                4096,
            )
            .unwrap()
            .command;
        assert!(command.args.contains(&"-Xmx4096M".to_owned()));
        assert!(!command.args.iter().any(|arg| arg.starts_with("-Xms")));
        assert!(command.args.contains(&"-XX:+UseSerialGC".to_owned()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn isolates_java_environment_without_affecting_other_process_settings() {
        let mut command = background_command("java");
        for variable in [
            "JAVA_TOOL_OPTIONS",
            "JDK_JAVA_OPTIONS",
            "_JAVA_OPTIONS",
            "CLASSPATH",
        ] {
            command.env(variable, "unvalidated-override");
        }
        let environment = BTreeMap::from([
            ("JAVA_HOME".into(), "fixture-java-home".into()),
            ("JAVA_TOOL_OPTIONS".into(), "another-override".into()),
            ("AURORA_TEST_VALUE".into(), "retained".into()),
        ]);
        configure_java_environment(&mut command, &environment);
        let entries = command.get_envs().collect::<BTreeMap<_, _>>();
        for variable in [
            "JAVA_TOOL_OPTIONS",
            "JDK_JAVA_OPTIONS",
            "_JAVA_OPTIONS",
            "CLASSPATH",
        ] {
            assert_eq!(entries.get(std::ffi::OsStr::new(variable)), Some(&None));
        }
        assert_eq!(
            entries.get(std::ffi::OsStr::new("JAVA_HOME")),
            Some(&Some(std::ffi::OsStr::new("fixture-java-home")))
        );
        assert_eq!(
            entries.get(std::ffi::OsStr::new("AURORA_TEST_VALUE")),
            Some(&Some(std::ffi::OsStr::new("retained")))
        );
    }

    #[test]
    fn offline_identity_does_not_impersonate_a_microsoft_session() {
        let identity = LaunchIdentity::offline("AuroraPlayer", Uuid::nil());
        assert_eq!(identity.access_token, "0");
        assert_eq!(identity.user_type, "legacy");
    }

    #[test]
    fn classpath_requires_libraries() {
        assert!(matches!(
            build_classpath(&[], Path::new("client.jar")),
            Err(LaunchError::EmptyClasspath)
        ));
    }

    #[test]
    fn applies_1_16_5_offline_privileges_fallback() {
        let mut spec = launch_spec();

        spec.apply_offline_compatibility("1.16.5");

        assert_eq!(
            spec.jvm_args,
            vec![
                "-Dminecraft.api.auth.host=https://authserver.mojang.com",
                "-Dminecraft.api.account.host=https://api.mojang.com",
                "-Dminecraft.api.session.host=https://sessionserver.mojang.com",
                "-Dminecraft.api.services.host=http://127.0.0.1:0",
            ]
        );
    }

    #[test]
    fn leaves_other_versions_unchanged() {
        let mut spec = launch_spec();

        spec.apply_offline_compatibility("1.20.1");

        assert!(spec.jvm_args.is_empty());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn hidden_game_process_keeps_stdout_and_stderr_in_log() {
        const CHILD_MARKER: &str = "AURORA_GAME_OUTPUT_TEST_CHILD";
        if std::env::var_os(CHILD_MARKER).is_some() {
            println!("aurora-game-stdout");
            eprintln!("aurora-game-stderr");
            return;
        }

        let root = std::env::temp_dir().join(format!("aurora-game-output-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("logs")).unwrap();
        std::fs::write(root.join("logs/aurora-java.log"), "previous-run-output").unwrap();
        let mut command = background_command(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "engine::launch::tests::hidden_game_process_keeps_stdout_and_stderr_in_log",
                "--nocapture",
            ])
            .env(CHILD_MARKER, "1");
        configure_windows_game_output(&mut command, &root).unwrap();
        let status = command.spawn().unwrap().wait().unwrap();
        let output = std::fs::read_to_string(root.join("logs/aurora-java.log")).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        assert!(status.success(), "child failed: {output}");
        assert!(output.contains("aurora-game-stdout"));
        assert!(output.contains("aurora-game-stderr"));
        assert!(!output.contains("previous-run-output"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn rejects_a_regular_file_as_log_directory() {
        let root = std::env::temp_dir().join(format!("aurora-invalid-logs-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("logs"), "preserve-me").unwrap();
        let mut command = background_command("java");
        assert!(configure_windows_game_output(&mut command, &root).is_err());
        assert_eq!(std::fs::read(root.join("logs")).unwrap(), b"preserve-me");
        std::fs::remove_dir_all(root).unwrap();
    }
}
