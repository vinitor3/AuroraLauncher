//! Exact literal-loopback TCP observation of the original owned child. This
//! neither normalizes listeners nor grants a Connect service or game authority.
use super::{windows, DiscoveryError, OwnedChildProof};
use std::net::{Ipv6Addr, SocketAddr};

pub(super) fn require_owned_tcp_listener(
    proof: &OwnedChildProof,
    endpoint: SocketAddr,
) -> Result<(), DiscoveryError> {
    if endpoint.port() == 0 {
        return Err(DiscoveryError::InvalidRequest);
    }
    match endpoint {
        SocketAddr::V4(v4) if v4.ip().is_loopback() => {}
        SocketAddr::V6(v6)
            if *v6.ip() == Ipv6Addr::LOCALHOST && v6.scope_id() == 0 && v6.flowinfo() == 0 => {}
        _ => return Err(DiscoveryError::InvalidRequest),
    }
    windows::require_listener(proof, endpoint)
}
