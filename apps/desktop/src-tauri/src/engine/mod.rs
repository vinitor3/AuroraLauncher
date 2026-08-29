//! Serviços de instância, Java e inicialização da JVM.

mod companion;
mod download;
mod instance;
mod ipc;
mod jre;
mod launch;
mod minecraft;
mod modpacks;
mod tts;
mod version;

pub use companion::{install_embedded_companion, CompanionError};
pub use download::{TransferProgress, DEFAULT_DOWNLOAD_CONCURRENCY};
pub use instance::{Instance, InstanceError, InstanceId, InstanceLayout};
pub use ipc::{IpcEndpoint, IpcError, IpcEvent, IpcServer};
pub use jre::{
    discover_java_executables, ensure_managed_java, ensure_managed_java_with_progress,
    recommended_memory_mb, JavaRuntime, JavaRuntimeError,
};
pub use launch::{
    LaunchCommand, LaunchError, LaunchIdentity, LauncherEngine, PreparedLaunch, VersionLaunchSpec,
};
pub use minecraft::{required_java_major, InstallError, InstallSummary, MinecraftInstaller};
pub use modpacks::{
    install_modrinth_content, install_modrinth_content_with_progress, install_modrinth_modpack,
    install_modrinth_modpack_with_progress, install_remote_content,
    install_remote_content_with_progress, resolve_modrinth_content_artwork,
    search_modrinth_content, search_modrinth_modpacks, ModpackError, ModpackInstallSummary,
    ModpackProgress, ModrinthContent, ModrinthPack, ModrinthSearchPage, ResolvedContentArtwork,
};
pub use tts::{synthesize_speech, SpeechBoundary, SpeechResult, TtsError};
pub use version::{resolve_launch_spec, VersionResolveError};
