//! Trusted native implementation boundary. No constructor here accepts a transport permit.
use super::ready_game_profile::{
    ReadyGameLaunchError, ReadyGameLaunchRequest, ReadyGameLaunchResult,
};
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;

/// Implemented by the desktop's ready-only launch path, never exposed through IPC.
pub trait ReadyGameBackend: Send + Sync + 'static {
    fn prepare(
        &self,
        request: &ReadyGameLaunchRequest,
        endpoint: SocketAddr,
        cancel: &CancellationToken,
    ) -> Result<Box<dyn PreparedReadyGame>, ReadyGameLaunchError>;
}

/// Prepared immutable evidence. Implementations must reacquire the instance lock,
/// compare all evidence and acquire tracking locks before entering the spawn gate.
pub trait PreparedReadyGame: Send {
    fn fingerprint(&self) -> &str;
    fn core_sha256(&self) -> &str;
    fn skins_sha256(&self) -> &str;
    fn launch(
        self: Box<Self>,
        gate: &super::live::ready_game::ReadyGameSpawnGate,
        cancel: &CancellationToken,
    ) -> Result<ReadyGameLaunchResult, ReadyGameLaunchError>;
}
