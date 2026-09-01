//! Serviços de instância, Java e inicialização da JVM.

mod companion;
mod core;
mod curseforge;
mod download;
mod instance;
mod ipc;
mod jre;
mod launch;
mod minecraft;
mod modpacks;
mod runtime_requirements;
mod skins;
mod tts;
mod version;

pub use companion::{install_embedded_companion, CompanionError};
pub use core::{
    aurora_core_manifest, install_embedded_core, AuroraCoreBuild, AuroraCoreError,
    AuroraCoreManifest,
};
pub use curseforge::{
    curseforge_archive_path, install_curseforge_modpack_with_progress,
    prepare_curseforge_modpack_with_progress, read_cached_curseforge_manifest,
    CurseForgeManifestFile, CurseForgeManifestSummary, CurseForgeResolvedFile,
};
pub use download::{TransferProgress, DEFAULT_DOWNLOAD_CONCURRENCY};
pub use instance::{Instance, InstanceError, InstanceId, InstanceLayout};
pub use ipc::{IpcEndpoint, IpcError, IpcEvent, IpcServer, IpcSessionProfile};
pub use jre::{
    available_java_releases, discover_java_executables, discover_managed_java_executables,
    ensure_managed_java, ensure_managed_java_with_progress, recommended_memory_mb,
    supported_managed_java_major, JavaReleaseCatalog, JavaRuntime, JavaRuntimeError,
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
pub use runtime_requirements::required_java_major_for_instance;
pub use skins::{install_embedded_skins, SkinsError};
pub use tts::{synthesize_speech, SpeechBoundary, SpeechResult, TtsError};
pub use version::{resolve_launch_spec, VersionResolveError};
