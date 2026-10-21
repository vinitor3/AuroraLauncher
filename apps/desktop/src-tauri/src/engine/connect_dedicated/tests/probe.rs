//! Counts observations made by the real guard's existing private test seam.

use crate::engine::recovery::RecoveryError;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Clone, Copy)]
pub(super) enum Refusal {
    Running,
    Unavailable,
}

#[derive(Clone, Default)]
pub(super) struct Probe {
    calls: Arc<AtomicUsize>,
    refuse_at: Arc<AtomicUsize>,
    unavailable: Arc<AtomicUsize>,
}

impl Probe {
    pub(super) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub(super) fn refuse(&self, call: usize, refusal: Refusal) {
        assert!(call > self.calls(), "fixture_future_probe");
        self.unavailable.store(
            usize::from(matches!(refusal, Refusal::Unavailable)),
            Ordering::SeqCst,
        );
        self.refuse_at.store(call, Ordering::SeqCst);
    }

    pub(super) fn boxed(
        &self,
    ) -> Box<dyn Fn() -> std::result::Result<(), RecoveryError> + Send + Sync> {
        let probe = self.clone();
        Box::new(move || {
            let call = probe.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if probe.refuse_at.load(Ordering::SeqCst) != call {
                return Ok(());
            }
            if probe.unavailable.load(Ordering::SeqCst) == 1 {
                Err(RecoveryError::VoiceProcessUnavailable)
            } else {
                Err(RecoveryError::VoiceProcessRunning)
            }
        })
    }
}
