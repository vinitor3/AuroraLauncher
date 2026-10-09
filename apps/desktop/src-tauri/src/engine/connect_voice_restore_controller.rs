//! Owns one receipt and, optionally, its exact unconsumed restoration review.
//! No guard, filesystem operation, or automatic restoration is retained in Drop.
use super::*;

/// Session-local authority for callers whose prepare and confirm calls occur
/// separately. Private fields cannot be reconstructed from a frontend snapshot.
/// A controller is neither clonable nor serializable and never exposes its bytes.
pub struct RestorationController {
    receipt: RestorationReceipt,
    pending: Option<PreparedAnnouncement>,
}

impl RestorationController {
    pub(super) fn new(receipt: RestorationReceipt) -> Self {
        Self {
            receipt,
            pending: None,
        }
    }

    pub fn state(&self) -> RestorationState {
        self.receipt.state()
    }

    /// Presence only; an expired review remains pending until explicitly
    /// cancelled or consumed. Reading state never extends its original deadline.
    pub fn has_pending_review(&self) -> bool {
        self.pending.is_some()
    }

    pub fn prepare_strict(
        &mut self,
        instance: &Instance,
        guard: &VoiceMutationGuard,
    ) -> Result<AnnouncementReview> {
        self.require_idle()?;
        let PreparedRestoration { prepared, .. } = self.receipt.prepare_restore(instance, guard)?;
        Ok(self.retain(prepared))
    }

    /// Explicit reconciliation only. A strict failure never selects this path.
    /// The current hash is a precondition, not an authorization to restore.
    pub fn prepare_reconciled(
        &mut self,
        instance: &Instance,
        guard: &VoiceMutationGuard,
        expected_current_sha256: &str,
    ) -> Result<AnnouncementReview> {
        self.require_idle()?;
        let PreparedRestoration { prepared, .. } =
            self.receipt
                .prepare_reconciled_restore(instance, guard, expected_current_sha256)?;
        Ok(self.retain(prepared))
    }

    /// Cancels only an unconsumed in-memory review. No file or receipt state is
    /// changed, and a later preparation requires its own review and approval.
    pub fn cancel_review(&mut self) -> bool {
        self.pending.take().is_some()
    }

    /// Consumes the original review even when approval or preconditions fail.
    /// Delegation preserves the borrowed API's Ready/NeedsRecovery/Restored
    /// transitions, fingerprint, original deadline and exact replacement bytes.
    pub fn confirm(
        &mut self,
        instance: &Instance,
        guard: &VoiceMutationGuard,
        approved_digest: &str,
    ) -> Result<RecoveryResult> {
        let prepared = self.pending.take().ok_or(RecoveryError::StalePreview)?;
        PreparedRestoration {
            receipt: &mut self.receipt,
            prepared,
        }
        .confirm(instance, guard, approved_digest)
    }

    fn require_idle(&self) -> Result<()> {
        if self.pending.is_some() || self.receipt.state() != RestorationState::Ready {
            return Err(RecoveryError::StalePreview);
        }
        Ok(())
    }

    fn retain(&mut self, prepared: PreparedAnnouncement) -> AnnouncementReview {
        let review = prepared.review().clone();
        self.pending = Some(prepared);
        review
    }
}

#[cfg(test)]
#[path = "connect_voice_restore_controller_tests.rs"]
mod tests;
