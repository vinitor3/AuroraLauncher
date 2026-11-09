//! Trusted native implementation boundary. No constructor here accepts a transport permit.
use super::ready_game_profile::{
    ReadyGameLaunchError, ReadyGameLaunchRequest, ReadyGameLaunchResult, ReadyVoiceReview,
};
use crate::engine::connect_native_voice::NativeVoiceProfile;
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;

/// Implemented by the desktop's ready-only launch path, never exposed through IPC.
pub trait ReadyGameBackend: Send + Sync + 'static {
    fn prepare(
        &self,
        request: &ReadyGameLaunchRequest,
        endpoint: SocketAddr,
        voice_profile: Option<NativeVoiceProfile>,
        runtime_profile: Option<crate::engine::connect_ready_profile::ReadyProfile>,
        cancel: &CancellationToken,
    ) -> Result<Box<dyn PreparedReadyGame>, ReadyGameLaunchError>;
}

/// Prepared immutable evidence. Implementations must reacquire the instance lock,
/// compare all evidence and acquire tracking locks before entering the spawn gate.
pub trait PreparedReadyGame: Send {
    fn profile(&self) -> crate::engine::connect_ready_profile::ReadyProfile;
    fn fingerprint(&self) -> &str;
    fn core_sha256(&self) -> &str;
    fn skins_sha256(&self) -> &str;
    fn voice_review(&self) -> Option<&ReadyVoiceReview> {
        None
    }
    fn launch(
        self: Box<Self>,
        gate: &super::live::ready_game::ReadyGameSpawnGate,
        cancel: &CancellationToken,
    ) -> Result<ReadyGameLaunchResult, ReadyGameLaunchError>;
}
