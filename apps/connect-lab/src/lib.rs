//! Experimental transport library. Direct-only endpoints; no relay or port mapping.
mod bridge;
mod fragment;
mod guest;
mod host;
mod invitation;
mod minecraft_status;
mod model;
mod plan;
mod transport;
mod wire;

pub use bridge::MAX_FRAGMENT_FRAME_BYTES;
pub use fragment::Completed as ReassembledDatagram;
pub use guest::{FragmentedUdpAssociation, Guest, TcpBridge, UdpAssociation, UdpBridge};
pub use host::Host;
pub use invitation::{
    Invitation, MAX_INVITATION_BYTES, MAX_INVITATION_LIFETIME, validate_external_invitation_address,
};
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
