//! Explicit retirement of selective authority only. No configuration writes,
//! automatic conflict handling, CAS deletion or restoration occur here.
use super::*;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetirementReview {
    pub plan_id: String,
    pub receipt_id: String,
    pub voice_mod: VoiceMod,
    pub expected_generation: u64,
    pub current_config_sha256: Option<String>,
    pub current_config_bytes: Option<u64>,
    pub digest: String,
    pub expires_at_ms: u64,
    pub warning: &'static str,
}

/// The original review, deadline and record hash stay private and are consumed
/// even on refusal. Dropping or cancelling this value has no side effects.
pub struct PreparedRetirement {
    review: RetirementReview,
    token: recovery::VoiceReceiptToken,
    instance_uuid: String,
    fingerprint: String,
    deadline: Instant,
}

fn unix_ms() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RecoveryError::Integrity)?
        .as_millis()
        .try_into()
        .map_err(|_| RecoveryError::Integrity)
}

fn check_deadline(deadline: Instant, expires: u64) -> Result<()> {
    if Instant::now() >= deadline || unix_ms()? >= expires {
        return Err(RecoveryError::StalePreview);
    }
    Ok(())
}

pub fn prepare_retirement(
    instance: &Instance,
    guard: &VoiceMutationGuard,
    receipt_id: &str,
    expected_generation: u64,
) -> Result<PreparedRetirement> {
    if expected_generation != 0 || !diagnostic_lab::is_lab_candidate(instance) {
        return Err(RecoveryError::StalePreview);
    }
    guard.recheck()?;
    let (loaded, current) =
        recovery::with_existing_observation(instance, guard.inner(), |current| {
            require_test_copy(instance)?;
            let loaded = recovery::load_voice_receipt(instance, guard.inner(), receipt_id)?;
            if loaded.instance_uuid != current.instance_uuid {
                return Err(RecoveryError::Identity);
            }
            if loaded.runtime != current.runtime {
                return Err(RecoveryError::Compatibility);
            }
            if loaded.state != recovery::VoiceReceiptState::Ready
                || loaded.token.generation != expected_generation
            {
                return Err(RecoveryError::StalePreview);
            }
            let config = public_mod(loaded.voice_mod).config();
            let file = current.files.iter().find(|file| file.path == config);
            if file.is_some_and(|file| file.size > MAX_CONFIG) {
                return Err(RecoveryError::Limit);
            }
            // A filtered/excluded file must not be mislabeled as genuinely absent.
            if file.is_none() && recovery::path(instance.root(), config)?.try_exists()? {
                return Err(RecoveryError::UnsafePath);
            }
            Ok((
                loaded,
                (
                    current.instance_uuid.clone(),
                    current.fingerprint.clone(),
                    file.cloned(),
                ),
            ))
        })?;
    guard.recheck()?;
    let prepared_at = Instant::now();
    let issued_at = unix_ms()?;
    let expires_at_ms = issued_at
        .checked_add(300_000)
        .ok_or(RecoveryError::Integrity)?;
    let mut review=RetirementReview {
        plan_id:Uuid::new_v4().to_string(),receipt_id:loaded.token.id.clone(),voice_mod:public_mod(loaded.voice_mod),expected_generation,
        current_config_sha256:current.2.as_ref().map(|file|file.sha256.clone()),
        current_config_bytes:current.2.as_ref().map(|file|file.size),digest:String::new(),expires_at_ms,
        warning:"Aposentar este recibo elimina sua restauração seletiva futura. Nenhuma configuração será restaurada ou alterada. Os snapshots existentes continuam sujeitos à Recuperação; um novo anúncio exigirá outro plano e aprovação.",
    };
    review.digest = digest(
        &serde_json::to_vec(&(
            "aurora-voice-retirement/v1",
            &review,
            &current.0,
            &current.1,
            &loaded.token.record_hash,
        ))
        .map_err(|_| RecoveryError::Integrity)?,
    );
    Ok(PreparedRetirement {
        review,
        token: loaded.token,
        instance_uuid: current.0,
        fingerprint: current.1,
        deadline: prepared_at + Duration::from_secs(300),
    })
}

impl PreparedRetirement {
    pub fn review(&self) -> &RetirementReview {
        &self.review
    }

    pub fn confirm(
        self,
        instance: &Instance,
        guard: &VoiceMutationGuard,
        approved_digest: &str,
    ) -> Result<DurableRestorationSummary> {
        self.confirm_with_executor_begin(instance, guard, approved_digest, || {})
    }

    pub(in super::super::super) fn confirm_with_executor_begin(
        self,
        instance: &Instance,
        guard: &VoiceMutationGuard,
        approved_digest: &str,
        on_executor_begin: impl FnOnce(),
    ) -> Result<DurableRestorationSummary> {
        check_deadline(self.deadline, self.review.expires_at_ms)?;
        if approved_digest != self.review.digest {
            return Err(RecoveryError::StalePreview);
        }
        guard.recheck()?;
        let loaded = checked_loaded(instance, guard, &self.token.id)?;
        if loaded.instance_uuid != self.instance_uuid
            || loaded.token.generation != self.token.generation
            || loaded.token.record_hash != self.token.record_hash
        {
            return Err(RecoveryError::StalePreview);
        }
        guard.recheck()?;
        check_deadline(self.deadline, self.review.expires_at_ms)?;
        // Return metadata is fully constructed before the SQLite publication.
        let result = DurableRestorationSummary {
            receipt_id: self.token.id.clone(),
            voice_mod: self.review.voice_mod,
            state: DurableReceiptState::Retired,
            generation: 1,
        };
        recovery::retire_voice_receipt(
            instance,
            guard.inner(),
            &self.token,
            &self.fingerprint,
            || {
                guard.recheck()?;
                check_deadline(self.deadline, self.review.expires_at_ms)
            },
            on_executor_begin,
        )?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retirement_original_civil_and_monotonic_deadlines_each_refuse_expiry() {
        let now = Instant::now();
        let civil = unix_ms().unwrap();
        assert!(check_deadline(now + Duration::from_secs(300), civil + 300_000).is_ok());
        assert!(matches!(
            check_deadline(now - Duration::from_secs(1), civil + 300_000),
            Err(RecoveryError::StalePreview)
        ));
        assert!(matches!(
            check_deadline(now + Duration::from_secs(300), civil - 1),
            Err(RecoveryError::StalePreview)
        ));
    }
}
