//! Internal V2 helpers. Public V1 bridges remain in guest.rs.
use crate::{Error, Result, fragment};
use iroh::{EndpointId, endpoint::Connection};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

struct Pools {
    global: fragment::GlobalBudget,
    peers: Mutex<HashMap<EndpointId, fragment::WeakPeerBudget>>,
}
fn pools() -> &'static Pools {
    static POOLS: OnceLock<Pools> = OnceLock::new();
    POOLS.get_or_init(|| Pools {
        global: fragment::GlobalBudget::default(),
        peers: Mutex::new(HashMap::new()),
    })
}
pub(crate) fn peer_budget(id: EndpointId) -> fragment::PeerBudget {
    let pools = pools();
    let mut peers = pools.peers.lock().unwrap();
    peers.retain(|_, weak| weak.upgrade().is_some());
    if let Some(peer) = peers.get(&id).and_then(|p| p.upgrade()) {
        return peer;
    }
    let peer = pools.global.peer();
    peers.insert(id, peer.downgrade());
    peer
}
pub(crate) fn error(error: fragment::FragmentError) -> Error {
    match error {
        fragment::FragmentError::Replay => Error::Replay,
        fragment::FragmentError::Limit
        | fragment::FragmentError::Allocation
        | fragment::FragmentError::Exhausted => Error::Limit,
        fragment::FragmentError::InvalidCapacity => Error::CapabilityUnavailable,
        _ => Error::Protocol,
    }
}
// A fixed conservative ceiling makes laboratory interoperability independent of
// PMTU growth. It is a wire-byte ceiling, not a physical MTU claim.
pub const MAX_FRAGMENT_FRAME_BYTES: usize = 1264;
pub(crate) fn wire_capacity(connection: &Connection) -> Option<usize> {
    connection
        .max_datagram_size()
        .map(|n| n.min(MAX_FRAGMENT_FRAME_BYTES))
        .filter(|n| *n > fragment::HEADER_LEN)
}
pub(crate) fn payload_capacity(connection: &Connection, service_max: usize) -> Option<usize> {
    wire_capacity(connection).map(|n| {
        ((n - fragment::HEADER_LEN) * fragment::MAX_FRAGMENTS)
            .min(service_max)
            .min(fragment::MAX_DATAGRAM)
    })
}
pub(crate) fn send_fragment(
    connection: &Connection,
    frame: fragment::Frame<'_>,
    initial: usize,
) -> Result<()> {
    // A path reduction abandons the remainder. No partial message reaches UDP.
    if !capacity_allows(initial, wire_capacity(connection)) {
        return Err(Error::DatagramTooLarge);
    }
    connection
        .send_datagram(frame.encode().map_err(error)?.into())
        .map_err(|_| Error::Transport)
}

fn capacity_allows(initial: usize, current: Option<usize>) -> bool {
    initial > fragment::HEADER_LEN
        && initial <= MAX_FRAGMENT_FRAME_BYTES
        && current.is_some_and(|now| now >= initial)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_pool_survives_owner_drop_while_completed_guard_is_held() {
        let id = iroh::SecretKey::generate().public();
        let peer = peer_budget(id);
        let scope = fragment::Scope {
            service_handle: 1,
            association_id: 1,
            generation: 1,
        };
        let mut assembler = fragment::Reassembler::new(scope, peer.clone());
        let frame = fragment::Fragments::new(scope, 1, &[0; 4096], 5000)
            .unwrap()
            .next()
            .unwrap();
        let fragment::Progress::Complete(output) =
            assembler.push(frame, std::time::Instant::now()).unwrap()
        else {
            panic!("incomplete");
        };
        drop(assembler);
        drop(peer);
        let reconnected = peer_budget(id);
        assert_eq!(reconnected.used_bytes(), 4096);
        assert_eq!(
            peer_budget(iroh::SecretKey::generate().public()).used_bytes(),
            0
        );
        drop(output);
        assert_eq!(reconnected.used_bytes(), 0);
    }
    #[test]
    fn shrink_or_missing_capacity_abandons_original_layout_without_renegotiation() {
        assert!(capacity_allows(1264, Some(1264)));
        assert!(capacity_allows(1264, Some(1400)));
        assert!(!capacity_allows(1264, Some(1263)));
        assert!(!capacity_allows(1264, None));
        assert!(!capacity_allows(32, Some(1264)));
        assert!(!capacity_allows(1265, Some(1400)));
    }
}
