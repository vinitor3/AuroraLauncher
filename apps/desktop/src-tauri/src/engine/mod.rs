//! Serviços de instância, Java e inicialização da JVM.

pub mod ai_history;
pub mod ai_tools;
pub mod connect;
pub mod connect_discovery;
#[cfg(all(windows, feature = "connect-lab"))]
pub mod connect_java_probe;
pub mod connect_native_voice;
#[cfg(all(windows, feature = "connect-lab"))]
pub(crate) mod connect_owned_run;
#[cfg(feature = "connect-lab")]
pub mod connect_quick_play;
#[cfg(feature = "connect-lab")]
pub mod connect_ready_files;
pub mod connect_ready_profile;
#[cfg(all(windows, feature = "connect-lab"))]
pub mod connect_ready_runtime;
pub mod connect_status_profile;
#[cfg(feature = "connect-voice-lab")]
pub mod connect_voice;
pub mod connect_voice_desktop;
pub mod diagnostic_lab;
pub mod doctor_recovery;
pub mod recovery;

mod companion_migration;
mod content_updates;
mod core;
mod curseforge;
mod doctor;
mod download;
mod external_downloads;
mod instance;
mod ipc;
mod jre;
mod launch;
mod minecraft;
mod modpacks;
mod process;
mod runtime_requirements;
mod skins;
mod tts;
mod version;

pub use companion_migration::{companion_migration_required, migrate_managed_companion};
pub use content_updates::{
    acquire_instance_mutation_lock, acquire_instance_recovery_lock, apply_content_updates,
    apply_modrinth_modpack_update_with_progress, apply_remote_content_updates,
    rollback_last_content_update, scan_content_updates, scan_modrinth_modpack_update,
    AppliedContentUpdate, ContentUpdateApplySummary, ContentUpdateError, ContentUpdateItem,
    ContentUpdateRollbackSummary, ContentUpdateScan, ContentUpdateStatus, InstanceMutationGuard,
    ModpackUpdateApplySummary, ModpackUpdateScan, ModpackUpdateStatus, RemoteContentUpdate,
    RestoredContentUpdate,
};
pub use core::{
    aurora_core_manifest, install_embedded_core, AuroraCoreBuild, AuroraCoreError,
    AuroraCoreManifest,
};
pub use curseforge::{
    curseforge_archive_path, install_curseforge_modpack_with_progress,
    prepare_curseforge_modpack_with_progress, read_cached_curseforge_manifest,
    CurseForgeManifestFile, CurseForgeManifestSummary, CurseForgeResolvedFile,
};
pub use doctor::{
    analyze_instance, canonical_launch_failure, snapshot_logs, DoctorContext, DoctorLogStamp,
    DoctorReport, DoctorRun,
};
pub use download::{TransferProgress, DEFAULT_DOWNLOAD_CONCURRENCY};
pub use external_downloads::{
    download_modrinth_mirror_with_progress, is_modrinth_mirror_url, resolve_modrinth_file_by_sha1,
    ExternalDownloadError, ModrinthMirrorFile,
};
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
    install_modrinth_modpack_version_transactional_with_progress,
    install_modrinth_modpack_version_with_progress, install_modrinth_modpack_with_progress,
    install_remote_content, install_remote_content_with_progress, list_modrinth_categories,
    resolve_modrinth_content_artwork, search_modrinth_content, search_modrinth_modpacks,
    ModpackError, ModpackInstallSummary, ModpackProgress, ModrinthCategory, ModrinthContent,
    ModrinthPack, ModrinthSearchPage, ResolvedContentArtwork,
};
pub use runtime_requirements::required_java_major_for_instance;
pub use skins::{install_embedded_skins, SkinsError};
pub use tts::{synthesize_speech, SpeechBoundary, SpeechResult, TtsError};
pub use version::{resolve_launch_spec, VersionResolveError};
