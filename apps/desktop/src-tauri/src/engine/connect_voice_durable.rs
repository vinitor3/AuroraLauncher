//! Explicit durable alternative. No disk input is a callable restoration plan;
//! loading verified history only permits preparing a fresh guarded review.
use super::*;

#[path = "connect_voice_retirement.rs"]
mod retirement;
pub use retirement::{prepare_retirement, PreparedRetirement, RetirementReview};

/// Durable history is distinct from the existing RAM restoration lifecycle.
/// Retired never implies that any configuration bytes were restored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DurableReceiptState {
    Ready,
    Restored,
    Retired,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DurableRestorationSummary {
    pub receipt_id: String,
    pub voice_mod: VoiceMod,
    pub state: DurableReceiptState,
    pub generation: u64,
}

pub struct DurableAppliedAnnouncement {
    pub recovery: RecoveryResult,
    pub receipt: DurableRestorationController,
}

/// Opaque, neither Clone nor Deserialize. A new reader may recover the same
/// authority after restart; expected record generation/hash make only one win.
pub struct DurableRestorationController {
    receipt: RestorationReceipt,
    token: recovery::VoiceReceiptToken,
    pending: Option<PreparedAnnouncement>,
}

fn stored_mod(value: VoiceMod) -> recovery::VoiceReceiptMod {
    match value {
        VoiceMod::SimpleVoiceChat => recovery::VoiceReceiptMod::SimpleVoiceChat,
        VoiceMod::PlasmoVoice => recovery::VoiceReceiptMod::PlasmoVoice,
    }
}
fn public_mod(value: recovery::VoiceReceiptMod) -> VoiceMod {
    match value {
        recovery::VoiceReceiptMod::SimpleVoiceChat => VoiceMod::SimpleVoiceChat,
        recovery::VoiceReceiptMod::PlasmoVoice => VoiceMod::PlasmoVoice,
    }
}
fn public_state(value: recovery::VoiceReceiptState) -> DurableReceiptState {
    match value {
        recovery::VoiceReceiptState::Ready => DurableReceiptState::Ready,
        recovery::VoiceReceiptState::Restored => DurableReceiptState::Restored,
        recovery::VoiceReceiptState::Retired => DurableReceiptState::Retired,
    }
}

pub struct PreparedDurableAnnouncement {
    prepared: PreparedAnnouncement,
}

/// Persistence is selected before review; a RAM approval cannot authorize it.
pub fn prepare_durable_announcement(
    instance: &Instance,
    guard: &VoiceMutationGuard,
    request: AnnouncementRequest,
) -> Result<PreparedDurableAnnouncement> {
    durable_plan(prepare_announcement(instance, guard, request)?)
}

/// Only the two compiled synthetic fixture byte strings are accepted here. The
/// normal adapter never uses this path, including real-artifact tests.
#[cfg(all(test, windows, feature = "connect-lab"))]
pub(in super::super::super) fn prepare_fixture_durable_announcement(
    instance: &Instance,
    guard: &VoiceMutationGuard,
    request: AnnouncementRequest,
) -> Result<PreparedDurableAnnouncement> {
    let filename = match request.voice_mod {
        VoiceMod::SimpleVoiceChat => "mods/voicechat-fabric-1.21.1-2.6.22.jar",
        VoiceMod::PlasmoVoice => "mods/plasmovoice-fabric-1.21.1-2.1.17.jar",
    };
    let artifact = format!("fixture:{filename}\n");
    let hash = digest(artifact.as_bytes());
    durable_plan(prepare_announcement_with_artifact(
        instance,
        guard,
        request,
        (&hash, artifact.len() as u64),
    )?)
}

fn durable_plan(mut prepared: PreparedAnnouncement) -> Result<PreparedDurableAnnouncement> {
    prepared.review.warning.push_str(" Recibo durável de restauração no armazenamento Recovery desta cópia. A revisão expira e a restauração posterior exige nova aprovação; nada é restaurado automaticamente.");
    prepared.review.digest.clear();
    prepared.review.digest = digest(
        &serde_json::to_vec(&(
            "aurora-voice-durable-receipt/v1",
            &prepared.review,
            &prepared.instance_uuid,
            &prepared.fingerprint,
        ))
        .map_err(|_| RecoveryError::Integrity)?,
    );
    Ok(PreparedDurableAnnouncement { prepared })
}
#[cfg(all(windows, feature = "connect-lab"))]
pub(crate) fn prepare_runtime_durable_announcement(
    instance: &Instance,
    guard: &VoiceMutationGuard,
    request: AnnouncementRequest,
    runtime: crate::engine::connect_ready_profile::ReadyProfile,
) -> Result<PreparedDurableAnnouncement> {
    durable_plan(prepare_runtime_announcement(
        instance, guard, request, runtime,
    )?)
}
impl PreparedDurableAnnouncement {
    pub fn review(&self) -> &AnnouncementReview {
        self.prepared.review()
    }
    /// Opt-in persistence in the same FULL transaction as application. The RAM
    /// confirm API and its callers keep their previous behavior and byte shape.
    pub fn confirm(
        self,
        instance: &Instance,
        guard: &VoiceMutationGuard,
        approved_digest: &str,
    ) -> Result<DurableAppliedAnnouncement> {
        self.confirm_with_executor_begin(instance, guard, approved_digest, || {})
    }
    pub(in super::super) fn confirm_with_executor_begin(
        self,
        instance: &Instance,
        guard: &VoiceMutationGuard,
        approved_digest: &str,
        on_executor_begin: impl FnOnce(),
    ) -> Result<DurableAppliedAnnouncement> {
        let prepared = self.prepared;
        if !matches!(prepared.review.action, AnnouncementAction::SetAnnouncement) {
            return Err(RecoveryError::StalePreview);
        }
        let receipt = RestorationReceipt::from_plan(&prepared);
        let intent = recovery::VoiceReceiptIntent::Apply {
            id: receipt.receipt_id.clone(),
            voice_mod: stored_mod(receipt.voice_mod),
            original_hash: receipt.original_sha256.clone(),
            applied_hash: receipt.applied_sha256.clone(),
        };
        let (recovery, token) = prepared.apply_using(
            instance,
            guard,
            approved_digest,
            on_executor_begin,
            |preview| recovery::restore_voice_receipt(instance, guard.inner(), preview, intent),
        )?;
        // All metadata/bytes were constructed before the successful commit.
        Ok(DurableAppliedAnnouncement {
            recovery,
            receipt: DurableRestorationController {
                receipt,
                token,
                pending: None,
            },
        })
    }
}

pub fn list_restoration_receipts(
    instance: &Instance,
    guard: &VoiceMutationGuard,
) -> Result<Vec<DurableRestorationSummary>> {
    guard.recheck()?;
    if !diagnostic_lab::is_lab_candidate(instance) {
        return Err(RecoveryError::UnsafePath);
    }
    let records = recovery::with_existing_observation(instance, guard.inner(), |_| {
        require_test_copy(instance)?;
        recovery::list_voice_receipts(instance, guard.inner())
    })?;
    guard.recheck()?;
    Ok(records
        .into_iter()
        .map(|record| DurableRestorationSummary {
            receipt_id: record.id,
            voice_mod: public_mod(record.voice_mod),
            state: public_state(record.state),
            generation: record.generation,
        })
        .collect())
}

pub fn resume_restoration(
    instance: &Instance,
    guard: &VoiceMutationGuard,
    receipt_id: &str,
) -> Result<DurableRestorationController> {
    guard.recheck()?;
    let loaded = checked_loaded(instance, guard, receipt_id)?;
    // Recover ownership from the authenticated historical images, not a label,
    // current values, a sidecar, or a caller-supplied before/after presentation.
    let voice_mod = public_mod(loaded.voice_mod);
    let (expected, previous_announcement, applied_announcement) = match voice_mod {
        VoiceMod::SimpleVoiceChat => {
            let span = svc_value_span(&loaded.applied)?;
            let value =
                std::str::from_utf8(&loaded.applied[span]).map_err(|_| RecoveryError::Integrity)?;
            let port = value
                .strip_prefix("127.0.0.1:")
                .ok_or(RecoveryError::Integrity)?
                .parse::<u16>()
                .map_err(|_| RecoveryError::Integrity)?;
            render_svc(&loaded.original, port)?
        }
        VoiceMod::PlasmoVoice => {
            let (ip, port) =
                plasmo::inspect_public(&loaded.applied)?.ok_or(RecoveryError::Integrity)?;
            if ip != "127.0.0.1" || port == 0 {
                return Err(RecoveryError::Integrity);
            }
            plasmo::render(&loaded.original, port)?
        }
    };
    if expected != loaded.applied {
        return Err(RecoveryError::Integrity);
    }
    guard.recheck()?;
    let receipt = RestorationReceipt {
        receipt_id: loaded.token.id.clone(),
        voice_mod,
        instance_uuid: loaded.instance_uuid,
        runtime: loaded.runtime,
        original_sha256: digest(&loaded.original),
        original: loaded.original,
        applied_sha256: digest(&loaded.applied),
        applied_size: loaded.applied.len() as u64,
        applied: loaded.applied,
        previous_announcement,
        applied_announcement,
        // checked_loaded accepts only a current Ready record; history never
        // becomes callable restoration authority merely by deserialization.
        state: RestorationState::Ready,
    };
    Ok(DurableRestorationController {
        receipt,
        token: loaded.token,
        pending: None,
    })
}

fn checked_loaded(
    instance: &Instance,
    guard: &VoiceMutationGuard,
    id: &str,
) -> Result<recovery::VoiceReceiptLoaded> {
    if !diagnostic_lab::is_lab_candidate(instance) {
        return Err(RecoveryError::UnsafePath);
    }
    recovery::with_existing_observation(instance, guard.inner(), |current| {
        require_test_copy(instance)?;
        let loaded = recovery::load_voice_receipt(instance, guard.inner(), id)?;
        if loaded.instance_uuid != current.instance_uuid {
            return Err(RecoveryError::Identity);
        }
        if loaded.runtime != current.runtime {
            return Err(RecoveryError::Compatibility);
        }
        Ok(loaded)
    })
}

impl DurableRestorationController {
    #[cfg(all(windows, feature = "connect-lab"))]
    pub(crate) fn review_deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|prepared| prepared.deadline)
    }
    pub fn voice_mod(&self) -> VoiceMod {
        self.receipt.voice_mod
    }
    pub fn receipt_id(&self) -> &str {
        &self.token.id
    }
    pub fn state(&self) -> RestorationState {
        self.receipt.state
    }
    pub fn generation(&self) -> u64 {
        self.token.generation
    }
    pub fn has_pending_review(&self) -> bool {
        self.pending.is_some()
    }
    pub fn cancel_review(&mut self) -> bool {
        self.pending.take().is_some()
    }

    fn current(&self, instance: &Instance, guard: &VoiceMutationGuard) -> Result<()> {
        if self.pending.is_some() || self.receipt.state != RestorationState::Ready {
            return Err(RecoveryError::StalePreview);
        }
        guard.recheck()?;
        let loaded = checked_loaded(instance, guard, &self.token.id)?;
        if loaded.state != recovery::VoiceReceiptState::Ready
            || loaded.token.generation != self.token.generation
            || loaded.token.record_hash != self.token.record_hash
        {
            return Err(RecoveryError::StalePreview);
        }
        Ok(())
    }
    pub fn prepare_strict(
        &mut self,
        instance: &Instance,
        guard: &VoiceMutationGuard,
    ) -> Result<AnnouncementReview> {
        self.current(instance, guard)?;
        let PreparedRestoration { prepared, .. } = self.receipt.prepare_restore(instance, guard)?;
        self.retain(prepared)
    }
    pub fn prepare_reconciled(
        &mut self,
        instance: &Instance,
        guard: &VoiceMutationGuard,
        current_sha256: &str,
    ) -> Result<AnnouncementReview> {
        self.current(instance, guard)?;
        let PreparedRestoration { prepared, .. } =
            self.receipt
                .prepare_reconciled_restore(instance, guard, current_sha256)?;
        self.retain(prepared)
    }
    fn retain(&mut self, mut prepared: PreparedAnnouncement) -> Result<AnnouncementReview> {
        prepared.review.warning = match prepared.review.action {
            AnnouncementAction::RestoreOriginal { .. } => "Restauração experimental estrita do arquivo na cópia de teste. O recibo é durável; esta revisão é somente de memória, expira e exige confirmação nova. Outros arquivos permanecem intactos.",
            AnnouncementAction::RestoreReconciled { .. } => "Restauração experimental somente das chaves de anúncio ainda aplicadas. Preserva outras edições; não promete o hash integral original. Recibo durável, revisão somente de memória com nova confirmação.",
            _ => return Err(RecoveryError::Integrity),
        }.into();
        prepared.review.digest.clear();
        prepared.review.digest = digest(
            &serde_json::to_vec(&(
                &prepared.review,
                &prepared.instance_uuid,
                &prepared.fingerprint,
            ))
            .map_err(|_| RecoveryError::Integrity)?,
        );
        let review = prepared.review.clone();
        self.pending = Some(prepared);
        Ok(review)
    }
    pub fn confirm(
        &mut self,
        instance: &Instance,
        guard: &VoiceMutationGuard,
        approved_digest: &str,
    ) -> Result<RecoveryResult> {
        let prepared = self.pending.take().ok_or(RecoveryError::StalePreview)?;
        self.current(instance, guard)?;
        let intent = recovery::VoiceReceiptIntent::Restore(recovery::VoiceReceiptToken {
            id: self.token.id.clone(),
            generation: self.token.generation,
            record_hash: self.token.record_hash.clone(),
        });
        let (result, token) = prepared.apply_using(
            instance,
            guard,
            approved_digest,
            || self.receipt.state = RestorationState::NeedsRecovery,
            |preview| recovery::restore_voice_receipt(instance, guard.inner(), preview, intent),
        )?;
        self.receipt.state = RestorationState::Restored;
        self.token = token;
        Ok(result)
    }
}
