//! A receipt restores only the file this session changed, never a previous
//! whole-instance snapshot. It is intentionally not persistent/serializable.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestorationState {
    Ready,
    /// Executor started but did not return success. Inspect/recover through the
    /// existing durable recovery UI; never retry a potentially partial write.
    NeedsRecovery,
    Restored,
}

/// Opaque, session-local proof of a successful announcement mutation. No Clone,
/// Deserialize, Debug or public byte fields; cannot be fabricated by a caller.
pub struct RestorationReceipt {
    receipt_id: String,
    voice_mod: VoiceMod,
    instance_uuid: String,
    runtime: BTreeMap<String, String>,
    original: Vec<u8>,
    original_sha256: String,
    applied_sha256: String,
    applied_size: u64,
    previous_announcement: String,
    applied_announcement: String,
    state: RestorationState,
}

impl RestorationReceipt {
    pub(super) fn from_plan(plan: &PreparedAnnouncement) -> Self {
        Self {
            receipt_id: Uuid::new_v4().to_string(),
            voice_mod: plan.voice_mod,
            instance_uuid: plan.instance_uuid.clone(),
            runtime: plan.runtime.clone(),
            original: plan.original.clone(),
            original_sha256: plan.review.source_sha256.clone(),
            applied_sha256: plan.review.replacement_sha256.clone(),
            applied_size: plan.replacement.len() as u64,
            previous_announcement: plan.review.before.clone(),
            applied_announcement: plan.review.after.clone(),
            state: RestorationState::Ready,
        }
    }

    pub fn state(&self) -> RestorationState {
        self.state
    }

    /// The mutable borrow permits one outstanding restore review per receipt.
    /// Dropping a prepared review is cancellation; it preserves the receipt.
    /// Same idle-game/lock preconditions as prepare_announcement apply here.
    pub fn prepare_restore<'a>(
        &'a mut self,
        instance: &Instance,
        guard: &InstanceMutationGuard,
    ) -> Result<PreparedRestoration<'a>> {
        if self.state != RestorationState::Ready {
            return Err(RecoveryError::StalePreview);
        }
        require_test_copy(instance)?;
        let current = recovery::capture_observation(instance, guard)?;
        if current.instance_uuid != self.instance_uuid {
            return Err(RecoveryError::Identity);
        }
        if current.runtime != self.runtime {
            return Err(RecoveryError::Compatibility);
        }
        let config = self.voice_mod.config();
        let file = current
            .files
            .iter()
            .find(|f| f.path == config)
            .ok_or(RecoveryError::UnsafePath)?;
        if file.sha256 != self.applied_sha256 || file.size != self.applied_size {
            return Err(RecoveryError::StalePreview);
        }
        let applied =
            recovery::read_bounded(&recovery::path(instance.root(), config)?, MAX_CONFIG)?;
        if digest(&applied) != self.applied_sha256
            || applied.len() as u64 != self.applied_size
            || digest(&self.original) != self.original_sha256
        {
            return Err(RecoveryError::Changed);
        }
        // Use the CURRENT inventory. Other files may legitimately have changed
        // since announcement; selecting just config preserves those changes.
        // The mod JAR need not remain installed to restore our previous bytes.
        let prepared = prepare_change(
            self.voice_mod,
            current,
            applied,
            self.original.clone(),
            self.applied_announcement.clone(),
            self.previous_announcement.clone(),
            AnnouncementAction::RestoreOriginal {
                receipt_id: self.receipt_id.clone(),
            },
        )?;
        Ok(PreparedRestoration {
            receipt: self,
            prepared,
        })
    }
}

pub struct PreparedRestoration<'a> {
    receipt: &'a mut RestorationReceipt,
    prepared: PreparedAnnouncement,
}
impl PreparedRestoration<'_> {
    pub fn review(&self) -> &AnnouncementReview {
        self.prepared.review()
    }

    pub fn confirm(
        self,
        instance: &Instance,
        guard: &InstanceMutationGuard,
        approved_digest: &str,
    ) -> Result<RecoveryResult> {
        let receipt = self.receipt;
        let result = self.prepared.apply(instance, guard, approved_digest, || {
            // Approval/expiry/identity/fingerprint failures occur before this
            // callback and leave Ready; any executor error is conservative.
            receipt.state = RestorationState::NeedsRecovery;
        })?;
        receipt.state = RestorationState::Restored;
        Ok(result)
    }
}
