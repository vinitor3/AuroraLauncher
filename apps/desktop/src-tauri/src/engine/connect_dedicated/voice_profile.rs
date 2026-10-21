//! Compiled runtime/configuration baselines. No profile API or serde authority.

pub(super) const MINECRAFT: &str = "1.21.1";
pub(super) const LOADER: &str = "fabric";
pub(super) const LOADER_VERSION: &str = "0.16.14";
pub(super) const PROTOCOL: i32 = 767;

pub(super) const SVC_CONFIGURATION_NAME: &str = "config/voicechat/voicechat-server.properties";
pub(super) const SVC_CONFIGURATION: &[u8] = b"# Disposable Aurora Connect Simple Voice Chat server lab.\n# An explicit loopback bind avoids the mod fallback to a wildcard bind.\nport=43002\nbind_address=127.0.0.1\nvoice_host=\n";
pub(super) const SVC_CONFIGURATION_SHA256: &str =
    "820f42e6cc7043d23b18daf384f5cbbc98fb1c58729cd74b54ae63a425f39e9b";
pub(super) const PLASMO_CONFIGURATION_NAME: &str = "config/plasmovoice/server/config.toml";
pub(super) const PLASMO_CONFIGURATION: &[u8] = b"# Disposable Aurora Connect Plasmo Voice server lab.\ndebug = false\ncheck_for_updates = false\nuse_crowdin_translations = false\n\n[host]\nip = \"127.0.0.1\"\nport = 43003\n";
pub(super) const PLASMO_CONFIGURATION_SHA256: &str =
    "2dfc96bc221060b2f6cb5ebc73a465893594ea036145fc14d54549bbd2d5634e";
