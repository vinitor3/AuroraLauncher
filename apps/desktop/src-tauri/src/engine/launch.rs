use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Child;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

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
            .current_dir(&self.command.working_dir)
            .envs(&self.command.environment);
        #[cfg(target_os = "windows")]
        configure_windows_game_output(&mut command, &self.command.working_dir)?;
        Ok(command.spawn()?)
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
        instance.ensure_layout()?;
        require_file(&spec.client_jar)?;
        let classpath = build_classpath(&spec.libraries, &spec.client_jar)?;
        let natives = instance.natives_dir();

        let mut args = Vec::new();
        args.push(format!("-Xmx{max_memory_mb}M"));
        args.push(format!("-Djava.library.path={}", natives.display()));
        if let Some(port) = spec.aurora_ipc_port {
            args.push(format!("-Daurora.ipc.port={port}"));
            args.push(format!("-Daurora.minecraft.version={}", spec.version_name));
        }
        if let Some(nonce) = &spec.aurora_session_nonce {
            args.push(format!("-Daurora.session.nonce={nonce}"));
        }
        args.extend(spec.jvm_args.iter().cloned());
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
            args: vec!["--accessToken".into(), "secret".into()],
            working_dir: PathBuf::new(),
            environment: BTreeMap::new(),
        };
        assert_eq!(command.redacted_args(), vec!["--accessToken", "[redacted]"]);
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
