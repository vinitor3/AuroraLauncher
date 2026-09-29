//! Experimental transport lab. No desktop integration, discovery, relay or port mapping.
mod guest;
mod host;
mod model;
mod transport;
mod wire;

pub use guest::{Guest, TcpBridge, UdpAssociation, UdpBridge};
pub use host::Host;
pub use iroh::{EndpointAddr, EndpointId, SecretKey};
pub use model::*;
pub use wire::{DATAGRAM_HEADER_LEN, DatagramFrame};

pub fn generate_key() -> SecretKey {
    SecretKey::generate()
}
