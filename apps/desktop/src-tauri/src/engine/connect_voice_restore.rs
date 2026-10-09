//! A receipt restores only the file/fields this session changed, never a previous
//! whole-instance snapshot. It is intentionally not persistent/serializable.
use super::*;

#[path = "connect_voice_restore_controller.rs"]
mod controller;
pub use controller::RestorationController;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestorationState {
    Ready,
    /// Executor started but did not return success. Inspect/recover through the
    /// existing durable recovery UI; never retry a potentially partial write.
    NeedsRecovery,
    /// Owned fields have been restored. Reconciliation preserves other current
    /// edits, so this does not promise the original whole-file hash.
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
    applied: Vec<u8>,
    applied_sha256: String,
    applied_size: u64,
    previous_announcement: String,
    applied_announcement: String,
    state: RestorationState,
}

impl RestorationReceipt {
    /// Moves the exclusive receipt into an owned caller without serializing or
    /// copying its restoration authority. Existing borrowed APIs stay available.
    pub fn into_controller(self) -> RestorationController {
        RestorationController::new(self)
    }

    pub(super) fn from_plan(plan: &PreparedAnnouncement) -> Self {
        Self {
            receipt_id: Uuid::new_v4().to_string(),
            voice_mod: plan.voice_mod,
            instance_uuid: plan.instance_uuid.clone(),
            runtime: plan.runtime.clone(),
            original: plan.original.clone(),
            original_sha256: plan.review.source_sha256.clone(),
            applied: plan.replacement.clone(),
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
        guard: &VoiceMutationGuard,
    ) -> Result<PreparedRestoration<'a>> {
        if self.state != RestorationState::Ready {
            return Err(RecoveryError::StalePreview);
        }
        require_test_copy(instance)?;
        guard.recheck()?;
        let current = recovery::capture_observation(instance, guard.inner())?;
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

    /// Explicit alternative when a writer/user changed other bytes in config.
    /// The current digest is a precondition, not approval: the returned review
    /// needs its own digest and expires independently. Never call as an automatic
    /// fallback for prepare_restore. Same stopped-game/lock preconditions apply.
    pub fn prepare_reconciled_restore<'a>(
        &'a mut self,
        instance: &Instance,
        guard: &VoiceMutationGuard,
        expected_current_sha256: &str,
    ) -> Result<PreparedRestoration<'a>> {
        if self.state != RestorationState::Ready {
            return Err(RecoveryError::StalePreview);
        }
        if expected_current_sha256.len() != 64
            || !expected_current_sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err(RecoveryError::Integrity);
        }
        require_test_copy(instance)?;
        guard.recheck()?;
        let current = recovery::capture_observation(instance, guard.inner())?;
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
        if !expected_current_sha256.eq_ignore_ascii_case(&file.sha256) {
            return Err(RecoveryError::StalePreview);
        }
        let bytes = recovery::read_bounded(&recovery::path(instance.root(), config)?, MAX_CONFIG)?;
        if digest(&bytes) != file.sha256
            || bytes.len() as u64 != file.size
            || digest(&self.original) != self.original_sha256
            || digest(&self.applied) != self.applied_sha256
            || self.applied.len() as u64 != self.applied_size
        {
            return Err(RecoveryError::Changed);
        }
        // Derive ownership and original/applied values from private bytes, never
        // from the presentation strings or untrusted caller-supplied values.
        let (replacement, before, after) = match self.voice_mod {
            VoiceMod::SimpleVoiceChat => reconcile_svc(&self.original, &self.applied, &bytes)?,
            VoiceMod::PlasmoVoice => plasmo::reconcile(&self.original, &self.applied, &bytes)?,
        };
        let prepared = prepare_change(
            self.voice_mod,
            current,
            bytes,
            replacement,
            before,
            after,
            AnnouncementAction::RestoreReconciled {
                receipt_id: self.receipt_id.clone(),
            },
        )?;
        Ok(PreparedRestoration {
            receipt: self,
            prepared,
        })
    }
}

fn reconcile_svc(
    original: &[u8],
    applied: &[u8],
    current: &[u8],
) -> Result<(Vec<u8>, String, String)> {
    let original_span = svc_value_span(original)?;
    let applied_span = svc_value_span(applied)?;
    let current_span = svc_value_span(current)?;
    let previous = &original[original_span];
    let expected = &applied[applied_span];
    if &current[current_span.clone()] != expected {
        return Err(RecoveryError::StalePreview);
    }
    if previous == expected {
        return Err(RecoveryError::Integrity);
    }
    if current.len() - current_span.len() + previous.len() > MAX_CONFIG as usize {
        return Err(RecoveryError::Limit);
    }
    let mut replacement = current.to_vec();
    replacement.splice(current_span, previous.iter().copied());
    svc_value_span(&replacement)?;
    let before = std::str::from_utf8(expected)
        .map_err(|_| RecoveryError::Integrity)?
        .to_owned();
    let after = std::str::from_utf8(previous)
        .map_err(|_| RecoveryError::Integrity)?
        .to_owned();
    Ok((replacement, before, after))
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
        guard: &VoiceMutationGuard,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svc_reconcile_preserves_current_comments_other_values_and_line_endings() {
        let original = b"voice_host=old.invalid:99 \nport=24454\n";
        let applied = render_svc(original, 35554).unwrap().0;
        let current = b"# new comment\r\nport = 24455\r\n\tvoice_host = \t127.0.0.1:35554\r\n";
        let (output, before, after) = reconcile_svc(original, &applied, current).unwrap();
        assert_eq!(
            output,
            b"# new comment\r\nport = 24455\r\n\tvoice_host = \told.invalid:99 \r\n"
        );
        assert_eq!(before, "127.0.0.1:35554");
        assert_eq!(after, "old.invalid:99 ");
        let empty = b"voice_host=";
        let applied = render_svc(empty, 35554).unwrap().0;
        assert_eq!(reconcile_svc(empty, &applied, &applied).unwrap().0, empty);
    }

    #[test]
    fn svc_reconcile_refuses_owned_conflicts_missing_keys_and_ambiguous_syntax() {
        let original = b"voice_host=old.invalid:99";
        let applied = render_svc(original, 35554).unwrap().0;
        for current in [
            b"voice_host=127.0.0.1:35555".as_slice(),
            b"voice_host=127.0.0.1:35554 ",
            b"port=24454",
            b"voice_host=127.0.0.1:35554\nvoice_host=other",
            b"voice_host=127.0.0.1:35554\\",
            b"voice_host=127.0.0.1:35554\rport=24454",
            b"voice_host=\xff",
        ] {
            assert!(reconcile_svc(original, &applied, current).is_err());
        }
        assert!(reconcile_svc(&applied, &applied, &applied).is_err());
        let oversized = vec![b'#'; MAX_CONFIG as usize + 1];
        assert!(reconcile_svc(original, &applied, &oversized).is_err());
    }

    #[test]
    fn svc_reconcile_checks_output_size_and_original_values_are_not_presentation_data() {
        let original = b"voice_host=long-original-host.invalid:12345";
        let applied = render_svc(original, 1).unwrap().0;
        let suffix = b"\nvoice_host=127.0.0.1:1";
        let mut current = vec![b'#'; MAX_CONFIG as usize - suffix.len()];
        current.extend_from_slice(suffix);
        assert!(matches!(
            reconcile_svc(original, &applied, &current),
            Err(RecoveryError::Limit)
        ));
        let delta = b"long-original-host.invalid:12345".len() - b"127.0.0.1:1".len();
        current.drain(..delta);
        assert_eq!(
            reconcile_svc(original, &applied, &current).unwrap().0.len(),
            MAX_CONFIG as usize
        );
    }
}
