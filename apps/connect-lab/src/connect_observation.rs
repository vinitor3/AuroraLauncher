//! Opt-in, bounded connection observation. No observer callback or I/O runs
//! inside the dial deadline, and no identity or network data is retained.
use serde::Serialize;
use std::{
    sync::atomic::{AtomicU8, Ordering},
    time::Instant,
};

/// Last operation entered, not a diagnosis of why it failed. `Admitted` means
/// the native handshake succeeded; it does not imply an application bridge.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum GuestConnectPhase {
    Preflight,
    EndpointBind,
    EndpointConnect,
    ControlOpen,
    HelloWrite,
    HelloAck,
    Admitted,
}

/// Returned only after the connect result, including awaited failure cleanup.
/// Elapsed time includes validation, bind and cleanup; it is not a network RTT.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuestConnectObservation {
    pub schema_version: u8,
    pub phase: GuestConnectPhase,
    pub elapsed_ms: u64,
}

pub(crate) struct ConnectObserver {
    started: Instant,
    phase: AtomicU8,
}
impl ConnectObserver {
    pub(crate) fn new() -> Self {
        Self {
            started: Instant::now(),
            phase: AtomicU8::new(GuestConnectPhase::Preflight as u8),
        }
    }
    pub(crate) fn enter(&self, phase: GuestConnectPhase) {
        self.phase.store(phase as u8, Ordering::Relaxed);
    }
    pub(crate) fn snapshot(&self) -> GuestConnectObservation {
        use GuestConnectPhase::*;
        let phase = match self.phase.load(Ordering::Relaxed) {
            0 => Preflight,
            1 => EndpointBind,
            2 => EndpointConnect,
            3 => ControlOpen,
            4 => HelloWrite,
            5 => HelloAck,
            6 => Admitted,
            _ => unreachable!("closed observation phase"),
        };
        GuestConnectObservation {
            schema_version: 1,
            phase,
            elapsed_ms: self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        }
    }
}
pub(crate) fn enter(observer: Option<&ConnectObserver>, phase: GuestConnectPhase) {
    if let Some(observer) = observer {
        observer.enter(phase);
    }
}
