//! Experimental transport lab. No desktop integration, discovery, relay or port mapping.
mod guest;
mod host;
mod minecraft_status;
mod model;
mod plan;
mod transport;
mod wire;

pub use guest::{Guest, TcpBridge, UdpAssociation, UdpBridge};
pub use host::Host;
pub use iroh::{EndpointAddr, EndpointId, SecretKey};
pub use minecraft_status::{
    MAX_MINECRAFT_STATUS_JSON_BYTES, MAX_MINECRAFT_STATUS_TIMEOUT,
    MAX_MINECRAFT_STATUS_UTF16_UNITS, MinecraftStatusError, MinecraftStatusRequest,
    MinecraftStatusSummary, probe_minecraft_status,
};
pub use model::*;
pub use plan::{MAX_PLAN_BYTES, PlanError, ValidatedPlan, load_plan};
pub use wire::{DATAGRAM_HEADER_LEN, DatagramFrame};

pub fn generate_key() -> SecretKey {
    SecretKey::generate()
}
