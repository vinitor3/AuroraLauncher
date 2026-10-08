//! Experimental V2 codec, negotiated only by the explicit fragmented UDP service.
//! One reassembler belongs to one authorized connection/session/service association.
//! Callers must check that authority before decoding and charge every wire fragment
//! (including rejected fragments) to their transport budgets. Nothing here opens I/O.

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Weak,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

pub const HEADER_LEN: usize = 32;
pub const MAX_DATAGRAM: usize = 4096;
pub const MAX_FRAGMENTS: usize = 8;
pub const REASSEMBLY_TTL: Duration = Duration::from_millis(250);
pub const ASSOCIATION_MESSAGES: usize = 8;
pub const ASSOCIATION_BYTES: usize = 32 * 1024;
pub const PEER_BYTES: usize = 64 * 1024;
pub const GLOBAL_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentError {
    InvalidFrame,
    InvalidCapacity,
    WrongScope,
    Replay,
    Inconsistent,
    Limit,
    Allocation,
    Exhausted,
}
type Result<T> = std::result::Result<T, FragmentError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope {
    pub service_handle: u32,
    pub association_id: u64,
    pub generation: u32,
}

/// Version 2; flags must be zero. Integers are big-endian.
/// Layout: version/flags (2), service (4), association (8), generation (4),
/// message (8), total (2), index (1), count (1), fragment length (2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame<'a> {
    pub scope: Scope,
    pub message_id: u64,
    pub total: u16,
    pub index: u8,
    pub count: u8,
    pub payload: &'a [u8],
}

fn layout(total: usize, count: usize, index: usize) -> Result<(usize, usize)> {
    if total > MAX_DATAGRAM || !(1..=MAX_FRAGMENTS).contains(&count) || index >= count {
        return Err(FragmentError::InvalidFrame);
    }
    // An empty UDP datagram has exactly one empty fragment.
    if total == 0 {
        return if count == 1 {
            Ok((0, 0))
        } else {
            Err(FragmentError::InvalidFrame)
        };
    }
    let chunk = total.div_ceil(count);
    let last_offset = chunk * (count - 1);
    if last_offset >= total {
        return Err(FragmentError::InvalidFrame);
    }
    let offset = chunk * index;
    Ok((
        offset,
        if index + 1 == count {
            total - offset
        } else {
            chunk
        },
    ))
}

impl<'a> Frame<'a> {
    fn validate(&self) -> Result<(usize, usize)> {
        let shape = layout(
            self.total as usize,
            self.count as usize,
            self.index as usize,
        )?;
        if self.message_id == 0 || self.payload.len() != shape.1 {
            return Err(FragmentError::InvalidFrame);
        }
        Ok(shape)
    }

    pub fn decode(wire: &'a [u8]) -> Result<Self> {
        if wire.len() < HEADER_LEN || wire[0] != 2 || wire[1] != 0 {
            return Err(FragmentError::InvalidFrame);
        }
        let declared = u16::from_be_bytes([wire[30], wire[31]]) as usize;
        if wire.len() != HEADER_LEN + declared {
            return Err(FragmentError::InvalidFrame);
        }
        let frame = Self {
            scope: Scope {
                service_handle: u32::from_be_bytes(wire[2..6].try_into().unwrap()),
                association_id: u64::from_be_bytes(wire[6..14].try_into().unwrap()),
                generation: u32::from_be_bytes(wire[14..18].try_into().unwrap()),
            },
            message_id: u64::from_be_bytes(wire[18..26].try_into().unwrap()),
            total: u16::from_be_bytes([wire[26], wire[27]]),
            index: wire[28],
            count: wire[29],
            payload: &wire[HEADER_LEN..],
        };
        frame.validate()?;
        Ok(frame)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut wire = Vec::new();
        wire.try_reserve_exact(HEADER_LEN + self.payload.len())
            .map_err(|_| FragmentError::Allocation)?;
        wire.extend_from_slice(&[2, 0]);
        wire.extend_from_slice(&self.scope.service_handle.to_be_bytes());
        wire.extend_from_slice(&self.scope.association_id.to_be_bytes());
        wire.extend_from_slice(&self.scope.generation.to_be_bytes());
        wire.extend_from_slice(&self.message_id.to_be_bytes());
        wire.extend_from_slice(&self.total.to_be_bytes());
        wire.extend_from_slice(&[self.index, self.count]);
        wire.extend_from_slice(&(self.payload.len() as u16).to_be_bytes());
        wire.extend_from_slice(self.payload);
        Ok(wire)
    }
}

/// A sender's counter is association-local and must not reset before that scope ends.
pub struct MessageIds(Option<u64>);
impl Default for MessageIds {
    fn default() -> Self {
        Self(Some(1))
    }
}
impl MessageIds {
    pub fn take(&mut self) -> Result<u64> {
        let id = self.0.ok_or(FragmentError::Exhausted)?;
        self.0 = id.checked_add(1);
        Ok(id)
    }
}

/// Borrows opaque bytes; no payload allocation and no network operations.
/// `wire_capacity` includes the V2 header. Recheck the transport capacity before
/// each actual send; abandon this message if it shrinks, never resize its layout.
pub struct Fragments<'a> {
    scope: Scope,
    message_id: u64,
    payload: &'a [u8],
    count: usize,
    next: usize,
}
impl<'a> Fragments<'a> {
    pub fn new(
        scope: Scope,
        message_id: u64,
        payload: &'a [u8],
        wire_capacity: usize,
    ) -> Result<Self> {
        if message_id == 0 || payload.len() > MAX_DATAGRAM {
            return Err(FragmentError::InvalidFrame);
        }
        let capacity = wire_capacity
            .checked_sub(HEADER_LEN)
            .filter(|n| *n > 0)
            .ok_or(FragmentError::InvalidCapacity)?;
        let count = payload.len().div_ceil(capacity).max(1);
        if count > MAX_FRAGMENTS {
            return Err(FragmentError::InvalidCapacity);
        }
        Ok(Self {
            scope,
            message_id,
            payload,
            count,
            next: 0,
        })
    }
}
impl<'a> Iterator for Fragments<'a> {
    type Item = Frame<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next == self.count {
            return None;
        }
        // Constructor computes a valid balanced layout, including the empty case.
        let (offset, len) = layout(self.payload.len(), self.count, self.next).unwrap();
        let frame = Frame {
            scope: self.scope,
            message_id: self.message_id,
            total: self.payload.len() as u16,
            index: self.next as u8,
            count: self.count as u8,
            payload: &self.payload[offset..offset + len],
        };
        self.next += 1;
        Some(frame)
    }
}

struct Counter {
    used: AtomicUsize,
    limit: usize,
}
impl Counter {
    fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            used: AtomicUsize::new(0),
            limit,
        })
    }
    fn reserve(self: &Arc<Self>, bytes: usize) -> Result<Hold> {
        self.used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= self.limit)
            })
            .map_err(|_| FragmentError::Limit)?;
        Ok(Hold {
            counter: Arc::clone(self),
            bytes,
        })
    }
    #[cfg(test)]
    fn used(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }
}
struct Hold {
    counter: Arc<Counter>,
    bytes: usize,
}
impl Drop for Hold {
    fn drop(&mut self) {
        self.counter.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

/// Create one global pool per process, and one peer handle per authenticated peer.
/// Clones share counters. Creating replacement pools per fragment defeats budgets
/// and is not a valid transport integration.
#[derive(Clone)]
pub struct GlobalBudget(Arc<Counter>);
impl Default for GlobalBudget {
    fn default() -> Self {
        Self(Counter::new(GLOBAL_BYTES))
    }
}
impl GlobalBudget {
    pub fn peer(&self) -> PeerBudget {
        PeerBudget {
            global: Arc::clone(&self.0),
            peer: Counter::new(PEER_BYTES),
        }
    }
    #[cfg(test)]
    pub fn used_bytes(&self) -> usize {
        self.0.used()
    }
}
#[derive(Clone)]
pub struct PeerBudget {
    global: Arc<Counter>,
    peer: Arc<Counter>,
}
impl PeerBudget {
    #[cfg(test)]
    pub fn used_bytes(&self) -> usize {
        self.peer.used()
    }
    pub(crate) fn downgrade(&self) -> WeakPeerBudget {
        WeakPeerBudget {
            global: Arc::clone(&self.global),
            peer: Arc::downgrade(&self.peer),
        }
    }
}
pub(crate) struct WeakPeerBudget {
    global: Arc<Counter>,
    peer: Weak<Counter>,
}
impl WeakPeerBudget {
    pub(crate) fn upgrade(&self) -> Option<PeerBudget> {
        self.peer.upgrade().map(|peer| PeerBudget {
            global: Arc::clone(&self.global),
            peer,
        })
    }
}

struct Reservation {
    _holds: [Hold; 4],
}
impl Reservation {
    fn new(
        peer: &PeerBudget,
        bytes: &Arc<Counter>,
        messages: &Arc<Counter>,
        len: usize,
    ) -> Result<Self> {
        // Each failed acquisition unwinds all preceding holds before returning.
        let slot = messages.reserve(1)?;
        let local = bytes.reserve(len)?;
        let peer_hold = peer.peer.reserve(len)?;
        let global = peer.global.reserve(len)?;
        Ok(Self {
            _holds: [slot, local, peer_hold, global],
        })
    }
}

/// Keeps its byte/message reservations until forwarding has finished or it is dropped.
/// There is intentionally no into_vec() that detaches unaccounted data.
pub struct Completed {
    bytes: Vec<u8>,
    _reservation: Reservation,
}
impl AsRef<[u8]> for Completed {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

struct Pending {
    count: u8,
    received: u8,
    first: Instant,
    complete: Completed,
}

#[derive(Default)]
struct Window {
    high: Option<u64>,
    seen: u64,
}
impl Window {
    fn obsolete(&self, id: u64) -> bool {
        self.high.is_some_and(|high| high >= id && high - id >= 64)
    }
    fn start(&mut self, id: u64) -> bool {
        match self.high {
            None => {
                self.high = Some(id);
                self.seen = 1;
                true
            }
            Some(high) if id > high => {
                let shift = id - high;
                self.seen = if shift >= 64 {
                    1
                } else {
                    (self.seen << shift) | 1
                };
                self.high = Some(id);
                true
            }
            Some(high) => {
                let offset = high - id;
                if offset >= 64 || self.seen & (1 << offset) != 0 {
                    return false;
                }
                self.seen |= 1 << offset;
                true
            }
        }
    }
}

pub enum Progress {
    Pending,
    Duplicate,
    Complete(Completed),
}

pub struct Reassembler {
    scope: Scope,
    peer: PeerBudget,
    bytes: Arc<Counter>,
    messages: Arc<Counter>,
    window: Window,
    pending: BTreeMap<u64, Pending>,
}
impl Reassembler {
    pub fn new(scope: Scope, peer: PeerBudget) -> Self {
        Self {
            scope,
            peer,
            bytes: Counter::new(ASSOCIATION_BYTES),
            messages: Counter::new(ASSOCIATION_MESSAGES),
            window: Window::default(),
            pending: BTreeMap::new(),
        }
    }
    #[cfg(test)]
    pub fn pending_messages(&self) -> usize {
        self.pending.len()
    }
    #[cfg(test)]
    pub fn reserved_bytes(&self) -> usize {
        self.bytes.used()
    }
    #[cfg(test)]
    pub fn reserved_messages(&self) -> usize {
        self.messages.used()
    }

    /// Must also be called by an association timer when no packets arrive.
    pub fn expire(&mut self, now: Instant) -> usize {
        let before = self.pending.len();
        self.pending.retain(|_, p| {
            now.checked_duration_since(p.first)
                .is_none_or(|elapsed| elapsed < REASSEMBLY_TTL)
        });
        before - self.pending.len()
    }

    /// Releases pending bytes, while retaining replay fences for this scope.
    /// Already completed datagrams remain accounted to their own RAII holders.
    pub fn clear(&mut self) {
        self.pending.clear();
    }

    pub fn push(&mut self, frame: Frame<'_>, now: Instant) -> Result<Progress> {
        let (offset, len) = frame.validate()?;
        if frame.scope != self.scope {
            return Err(FragmentError::WrongScope);
        }
        self.expire(now);
        if let Some(pending) = self.pending.get(&frame.message_id) {
            let inconsistent = pending.count != frame.count
                || pending.complete.bytes.len() != frame.total as usize;
            let duplicate = pending.received & (1 << frame.index) != 0;
            if inconsistent
                || (duplicate && pending.complete.bytes[offset..offset + len] != *frame.payload)
            {
                self.pending.remove(&frame.message_id);
                return Err(FragmentError::Inconsistent);
            }
            if duplicate {
                return Ok(Progress::Duplicate);
            }
        } else {
            if !self.window.start(frame.message_id) {
                return Err(FragmentError::Replay);
            }
            self.pending.retain(|id, _| !self.window.obsolete(*id));
            let reservation = Reservation::new(
                &self.peer,
                &self.bytes,
                &self.messages,
                frame.total as usize,
            )?;
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(frame.total as usize)
                .map_err(|_| FragmentError::Allocation)?;
            bytes.resize(frame.total as usize, 0);
            self.pending.insert(
                frame.message_id,
                Pending {
                    count: frame.count,
                    received: 0,
                    first: now,
                    complete: Completed {
                        bytes,
                        _reservation: reservation,
                    },
                },
            );
        }
        let pending = self.pending.get_mut(&frame.message_id).unwrap();
        pending.complete.bytes[offset..offset + len].copy_from_slice(frame.payload);
        pending.received |= 1 << frame.index;
        if pending.received.count_ones() == frame.count as u32 {
            Ok(Progress::Complete(
                self.pending.remove(&frame.message_id).unwrap().complete,
            ))
        } else {
            Ok(Progress::Pending)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCOPE: Scope = Scope {
        service_handle: 7,
        association_id: 9,
        generation: 3,
    };
    fn setup() -> (GlobalBudget, PeerBudget, Reassembler) {
        let global = GlobalBudget::default();
        let peer = global.peer();
        let receiver = Reassembler::new(SCOPE, peer.clone());
        (global, peer, receiver)
    }
    fn frames(id: u64, data: &[u8], payload_cap: usize) -> Vec<Frame<'_>> {
        Fragments::new(SCOPE, id, data, HEADER_LEN + payload_cap)
            .unwrap()
            .collect()
    }
    fn error(result: Result<Progress>) -> FragmentError {
        match result {
            Err(e) => e,
            Ok(_) => panic!("expected rejection"),
        }
    }
    fn completed(result: Result<Progress>) -> Completed {
        match result {
            Ok(Progress::Complete(data)) => data,
            _ => panic!("expected complete datagram"),
        }
    }

    #[test]
    fn exact_big_endian_header_and_v1_is_not_accepted() {
        let frame = frames(1, b"abc", 3)[0];
        let wire = frame.encode().unwrap();
        assert_eq!(
            &wire[..HEADER_LEN],
            &[
                2, 0, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 3,
                0, 1, 0, 3,
            ]
        );
        assert_eq!(Frame::decode(&wire).unwrap(), frame);
        let mut old = wire.clone();
        old[0] = 1;
        assert_eq!(Frame::decode(&old), Err(FragmentError::InvalidFrame));
        for length in 0..wire.len() {
            assert_eq!(
                Frame::decode(&wire[..length]),
                Err(FragmentError::InvalidFrame)
            );
        }
        let mut extra = wire.clone();
        extra.push(0);
        assert_eq!(Frame::decode(&extra), Err(FragmentError::InvalidFrame));
    }

    #[test]
    fn malformed_shapes_and_lengths_are_rejected_before_reservation() {
        let wire = frames(1, &[7; 100], 50)[0].encode().unwrap();
        for (offset, value) in [
            (0, 3),
            (1, 1),
            (28, 2),
            (29, 0),
            (29, 9),
            (31, 49),
            (31, 51),
        ] {
            let mut invalid = wire.clone();
            invalid[offset] = value;
            assert_eq!(Frame::decode(&invalid), Err(FragmentError::InvalidFrame));
        }
        let (_, _, mut receiver) = setup();
        let base = frames(1, b"a", 1)[0];
        for bad in [
            Frame {
                total: 4097,
                ..base
            },
            Frame {
                message_id: 0,
                ..base
            },
            Frame { count: 2, ..base },
            Frame { total: 0, ..base },
        ] {
            assert_eq!(
                error(receiver.push(bad, Instant::now())),
                FragmentError::InvalidFrame
            );
        }
        assert_eq!(receiver.reserved_messages(), 0);
        assert_eq!(receiver.reserved_bytes(), 0);
    }

    #[test]
    fn balanced_sender_covers_every_byte_with_bounded_fragments() {
        let bytes = [17; MAX_DATAGRAM];
        for len in 0..=MAX_DATAGRAM {
            for cap in [1usize, 17, 512, 1256, 4096] {
                let result = Fragments::new(SCOPE, 1, &bytes[..len], HEADER_LEN + cap);
                let count = len.div_ceil(cap).max(1);
                if count > MAX_FRAGMENTS {
                    assert!(result.is_err());
                    continue;
                }
                let fs: Vec<_> = result.unwrap().collect();
                assert_eq!(fs.len(), count);
                let chunk = len.div_ceil(count);
                assert_eq!(fs.iter().map(|f| f.payload.len()).sum::<usize>(), len);
                for f in &fs {
                    assert!(f.payload.len() <= cap);
                    assert!(f.validate().is_ok());
                    if f.index as usize + 1 < count {
                        assert_eq!(f.payload.len(), chunk);
                    }
                }
            }
        }
        assert!(Fragments::new(SCOPE, 1, b"", HEADER_LEN).is_err());
        assert!(Fragments::new(SCOPE, 1, &[0; MAX_DATAGRAM + 1], 5000).is_err());
    }

    #[test]
    fn opaque_1427_2108_4096_roundtrip_reordered_without_partial_delivery() {
        for len in [1427usize, 2108, 4096] {
            let (global, _, mut receiver) = setup();
            let data: Vec<_> = (0..len).map(|n| (n % 251) as u8).collect();
            let fs = frames(1, &data, 1256);
            assert_eq!(fs.len(), len.div_ceil(1256));
            let now = Instant::now();
            for frame in fs[1..].iter().rev() {
                let wire = frame.encode().unwrap();
                assert!(matches!(
                    receiver.push(Frame::decode(&wire).unwrap(), now).unwrap(),
                    Progress::Pending
                ));
                assert!(matches!(
                    receiver.push(*frame, now).unwrap(),
                    Progress::Duplicate
                ));
            }
            let result = completed(receiver.push(fs[0], now));
            assert_eq!(result.as_ref(), data);
            assert_eq!(receiver.pending_messages(), 0);
            assert_eq!(global.used_bytes(), len);
            assert_eq!(error(receiver.push(fs[0], now)), FragmentError::Replay);
            drop(result);
            assert_eq!(global.used_bytes(), 0);
        }
    }

    #[test]
    fn empty_udp_uses_a_message_slot_and_replay_fence() {
        let (_, _, mut receiver) = setup();
        let frame = frames(1, b"", 1256)[0];
        let output = completed(receiver.push(frame, Instant::now()));
        assert!(output.as_ref().is_empty());
        assert_eq!(receiver.reserved_messages(), 1);
        assert_eq!(receiver.reserved_bytes(), 0);
        assert_eq!(
            error(receiver.push(frame, Instant::now())),
            FragmentError::Replay
        );
        drop(output);
        assert_eq!(receiver.reserved_messages(), 0);
    }

    #[test]
    fn changed_payload_or_metadata_discards_and_fences_whole_message() {
        for metadata in [false, true] {
            let (global, _, mut receiver) = setup();
            let data = [1; 2048];
            let fs = frames(1, &data, 1024);
            receiver.push(fs[0], Instant::now()).unwrap();
            let changed = [2; 1024];
            let bad = if metadata {
                Frame {
                    total: 2047,
                    ..fs[0]
                }
            } else {
                Frame {
                    payload: &changed,
                    ..fs[0]
                }
            };
            assert_eq!(
                error(receiver.push(bad, Instant::now())),
                FragmentError::Inconsistent
            );
            assert_eq!(global.used_bytes(), 0);
            assert_eq!(
                error(receiver.push(fs[1], Instant::now())),
                FragmentError::Replay
            );
        }
    }

    #[test]
    fn expiry_is_fixed_and_duplicates_never_extend_it() {
        let (global, _, mut receiver) = setup();
        let fs = frames(1, &[1; 100], 50);
        let start = Instant::now();
        receiver.push(fs[0], start).unwrap();
        assert!(matches!(
            receiver
                .push(fs[0], start + Duration::from_millis(249))
                .unwrap(),
            Progress::Duplicate
        ));
        assert_eq!(receiver.expire(start + Duration::from_millis(249)), 0);
        assert_eq!(
            error(receiver.push(fs[1], start + REASSEMBLY_TTL)),
            FragmentError::Replay
        );
        assert_eq!(global.used_bytes(), 0);
    }

    #[test]
    fn new_fragment_does_not_extend_deadline_and_timer_releases_silent_loss() {
        let (global, _, mut receiver) = setup();
        let fs = frames(1, &[1; 120], 40);
        let start = Instant::now();
        receiver.push(fs[0], start).unwrap();
        receiver
            .push(fs[1], start + Duration::from_millis(249))
            .unwrap();
        assert_eq!(receiver.expire(start + REASSEMBLY_TTL), 1);
        assert_eq!(receiver.expire(start + REASSEMBLY_TTL), 0);
        assert_eq!(global.used_bytes(), 0);
        assert_eq!(
            error(receiver.push(fs[2], start + REASSEMBLY_TTL)),
            FragmentError::Replay
        );
    }

    #[test]
    fn out_of_order_message_window_has_exact_64_boundary() {
        let (_, _, mut receiver) = setup();
        let now = Instant::now();
        for id in [64, 1, 63, 2] {
            drop(completed(receiver.push(frames(id, b"a", 1)[0], now)));
        }
        assert_eq!(
            error(receiver.push(frames(1, b"a", 1)[0], now)),
            FragmentError::Replay
        );
        drop(completed(receiver.push(frames(65, b"a", 1)[0], now)));
        assert_eq!(
            error(receiver.push(frames(1, b"a", 1)[0], now)),
            FragmentError::Replay
        );
        drop(completed(receiver.push(frames(3, b"a", 1)[0], now)));
    }

    #[test]
    fn advancing_window_releases_obsolete_incomplete_messages() {
        let (global, _, mut receiver) = setup();
        let fs = frames(1, &[1; 100], 50);
        let now = Instant::now();
        receiver.push(fs[0], now).unwrap();
        drop(completed(receiver.push(frames(65, b"a", 1)[0], now)));
        assert_eq!(global.used_bytes(), 0);
        assert_eq!(receiver.pending_messages(), 0);
        assert_eq!(error(receiver.push(fs[1], now)), FragmentError::Replay);
    }

    #[test]
    fn sender_and_receiver_never_wrap_message_ids() {
        let mut normal = MessageIds::default();
        assert_eq!(normal.take(), Ok(1));
        assert_eq!(normal.take(), Ok(2));
        let mut tail = MessageIds(Some(u64::MAX));
        assert_eq!(tail.take(), Ok(u64::MAX));
        assert_eq!(tail.take(), Err(FragmentError::Exhausted));
        let (_, _, mut receiver) = setup();
        let now = Instant::now();
        drop(completed(receiver.push(frames(u64::MAX, b"a", 1)[0], now)));
        assert_eq!(
            error(receiver.push(frames(1, b"a", 1)[0], now)),
            FragmentError::Replay
        );
        drop(completed(
            receiver.push(frames(u64::MAX - 1, b"a", 1)[0], now),
        ));
    }

    #[test]
    fn clear_releases_pending_but_keeps_fences_and_completed_reservations() {
        let (global, peer, mut receiver) = setup();
        let now = Instant::now();
        let partial = frames(1, &[1; 100], 50);
        receiver.push(partial[0], now).unwrap();
        let output = completed(receiver.push(frames(2, &[2; 200], 200)[0], now));
        receiver.clear();
        assert_eq!(global.used_bytes(), 200);
        assert_eq!(error(receiver.push(partial[1], now)), FragmentError::Replay);
        assert_eq!(receiver.reserved_messages(), 1);
        drop(receiver);
        assert_eq!(peer.used_bytes(), 200);
        drop(output);
        assert_eq!(global.used_bytes(), 0);
        assert_eq!(peer.used_bytes(), 0);
    }

    #[test]
    fn association_quota_counts_completed_and_failed_attempt_is_fenced() {
        let (global, _, mut receiver) = setup();
        let data = [0; MAX_DATAGRAM];
        let now = Instant::now();
        for id in 1..=8 {
            receiver.push(frames(id, &data, 2048)[0], now).unwrap();
        }
        assert_eq!(receiver.reserved_bytes(), ASSOCIATION_BYTES);
        assert_eq!(
            error(receiver.push(frames(9, &data, 2048)[0], now)),
            FragmentError::Limit
        );
        receiver.clear();
        assert_eq!(global.used_bytes(), 0);
        assert_eq!(
            error(receiver.push(frames(9, &data, 2048)[0], now)),
            FragmentError::Replay
        );
        let mut held = Vec::new();
        for id in 10..18 {
            held.push(completed(receiver.push(frames(id, b"", 1)[0], now)));
        }
        assert_eq!(
            error(receiver.push(frames(18, b"", 1)[0], now)),
            FragmentError::Limit
        );
        held.clear();
        assert_eq!(receiver.reserved_messages(), 0);
        drop(completed(receiver.push(frames(19, b"", 1)[0], now)));
    }

    fn fill(receiver: &mut Reassembler, now: Instant) {
        for id in 1..=8 {
            receiver
                .push(frames(id, &[0; MAX_DATAGRAM], 2048)[0], now)
                .unwrap();
        }
    }

    #[test]
    fn cloned_peer_pool_limits_multiple_associations_and_rolls_back_partial_acquisition() {
        let (global, peer, mut first) = setup();
        let now = Instant::now();
        let mut second = Reassembler::new(SCOPE, peer.clone());
        let mut third = Reassembler::new(SCOPE, peer.clone());
        fill(&mut first, now);
        fill(&mut second, now);
        assert_eq!(peer.used_bytes(), PEER_BYTES);
        assert_eq!(
            error(third.push(frames(1, b"a", 1)[0], now)),
            FragmentError::Limit
        );
        assert_eq!(third.reserved_bytes(), 0);
        assert_eq!(third.reserved_messages(), 0);
        assert_eq!(global.used_bytes(), PEER_BYTES);
        drop(first);
        drop(second);
        assert_eq!(global.used_bytes(), 0);
        drop(completed(third.push(frames(2, b"a", 1)[0], now)));
    }

    #[test]
    fn global_pool_limits_distinct_peers_and_failure_releases_peer_reservation() {
        let global = GlobalBudget::default();
        let now = Instant::now();
        let mut held = Vec::new();
        for _ in 0..16 {
            let peer = global.peer();
            for _ in 0..2 {
                let mut receiver = Reassembler::new(SCOPE, peer.clone());
                fill(&mut receiver, now);
                held.push(receiver);
            }
        }
        assert_eq!(global.used_bytes(), GLOBAL_BYTES);
        let peer = global.peer();
        let mut excess = Reassembler::new(SCOPE, peer.clone());
        assert_eq!(
            error(excess.push(frames(1, b"a", 1)[0], now)),
            FragmentError::Limit
        );
        assert_eq!(peer.used_bytes(), 0);
        assert_eq!(excess.reserved_messages(), 0);
        held.clear();
        assert_eq!(global.used_bytes(), 0);
    }

    #[test]
    fn concurrent_shared_peer_reservations_cannot_oversubscribe() {
        use std::{sync::Barrier, thread};
        let global = GlobalBudget::default();
        let peer = global.peer();
        let barrier = Arc::new(Barrier::new(33));
        let mut threads = Vec::new();
        for _ in 0..32 {
            let peer = peer.clone();
            let barrier = Arc::clone(&barrier);
            threads.push(thread::spawn(move || {
                let hold = Reservation::new(
                    &peer,
                    &Counter::new(ASSOCIATION_BYTES),
                    &Counter::new(ASSOCIATION_MESSAGES),
                    MAX_DATAGRAM,
                );
                let accepted = hold.is_ok();
                barrier.wait();
                barrier.wait();
                drop(hold);
                accepted
            }));
        }
        barrier.wait();
        assert_eq!(peer.used_bytes(), PEER_BYTES);
        assert_eq!(global.used_bytes(), PEER_BYTES);
        barrier.wait();
        let accepted = threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|accepted| *accepted)
            .count();
        assert_eq!(accepted, 16);
        assert_eq!(peer.used_bytes(), 0);
        assert_eq!(global.used_bytes(), 0);
    }

    #[test]
    fn foreign_scope_cannot_consume_sequence_or_memory() {
        let (global, _, mut receiver) = setup();
        let good = frames(1, b"a", 1)[0];
        for scope in [
            Scope {
                service_handle: 8,
                ..SCOPE
            },
            Scope {
                association_id: 10,
                ..SCOPE
            },
            Scope {
                generation: 4,
                ..SCOPE
            },
        ] {
            assert_eq!(
                error(receiver.push(Frame { scope, ..good }, Instant::now())),
                FragmentError::WrongScope
            );
        }
        assert_eq!(global.used_bytes(), 0);
        drop(completed(receiver.push(good, Instant::now())));
    }
}
