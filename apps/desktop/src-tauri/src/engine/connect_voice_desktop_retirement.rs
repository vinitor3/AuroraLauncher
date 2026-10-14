//! Explicit retirement is a separate approved index transaction. It neither
//! restores configuration nor borrows approval from an announcement plan.
use super::*;

fn presentation(review: &RetirementReview) -> Outcome<dto::ReceiptRetirementReview> {
    if !uuid(&review.plan_id)
        || !uuid(&review.receipt_id)
        || !hex(&review.digest)
        || review.expected_generation != 0
        || review.expires_at_ms > MAX_GENERATION
        || review.warning.len() > 1024
        || review
            .current_config_sha256
            .as_deref()
            .is_some_and(|hash| !hex(hash))
        || review
            .current_config_bytes
            .is_some_and(|bytes| bytes > MAX_CONFIG)
        || review.current_config_sha256.is_some() != review.current_config_bytes.is_some()
    {
        return Err(Error::Integrity);
    }
    Ok(dto::ReceiptRetirementReview {
        plan_id: review.plan_id.clone(),
        receipt_id: review.receipt_id.clone(),
        voice_mod: to_mod(review.voice_mod),
        expected_generation: review.expected_generation,
        current_config_sha256: review.current_config_sha256.clone(),
        current_config_bytes: review.current_config_bytes,
        digest: review.digest.clone(),
        expires_at_ms: review.expires_at_ms,
        warning: review.warning.into(),
    })
}

impl Manager {
    pub(crate) async fn prepare_retirement(
        &self,
        data_dir: &Path,
        request: dto::ResumeReceiptRequest,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        let id = crate::engine::InstanceId::parse(&request.instance_id)
            .map_err(|_| Error::InvalidRequest)?;
        if !uuid(&request.receipt_id) || request.expected_generation != 0 {
            return Err(Error::InvalidRequest);
        }
        let mut core = self
            .shared
            .authority
            .clone()
            .try_lock_owned()
            .map_err(|_| Error::Busy)?;
        let instance = crate::engine::InstanceLayout::new(data_dir).open(id);
        if !diagnostic_lab::is_lab_candidate(&instance) {
            return Err(Error::UnsafePath);
        }
        {
            let mut view = self.shared.view()?;
            Shared::admission(&view)?;
            if core.operation.is_some() {
                return Err(Error::ContextActive);
            }
            Shared::advance(&mut view)?;
            view.snapshot.last_error_code = None;
            Shared::phase(&mut view, Phase::PreparingRetirement, true);
        }
        let shared = self.shared.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                #[cfg(test)]
                shared.hook(TestStage::PrepareRetirement);
                let guard = shared.guard(&instance).map_err(error)?;
                // Keeping this validated Ready controller makes cancelling the
                // retirement an explicit return to the normal restore choices.
                let controller =
                    resume_restoration(&instance, &guard, &request.receipt_id).map_err(error)?;
                if controller.state() != RestorationState::Ready
                    || controller.generation() != request.expected_generation
                {
                    return Err(Error::StaleReview);
                }
                let prepared = super::super::prepare_retirement(
                    &instance,
                    &guard,
                    &request.receipt_id,
                    request.expected_generation,
                )
                .map_err(error)?;
                let review = presentation(prepared.review())?;
                let operation_id = Uuid::new_v4().to_string();
                let voice_mod = controller.voice_mod();
                let mut view = shared.view()?;
                core.operation = Some(Operation {
                    id: operation_id.clone(),
                    instance,
                    voice_mod,
                    observed_hash: None,
                    pending_apply: None,
                    pending_durable: None,
                    controller: None,
                    durable_controller: Some(controller),
                    durable_selected: true,
                    pending_retirement: Some(prepared),
                    retirement_review: Some(review),
                    retired_receipt: None,
                    executor_uncertain: false,
                    observation: None,
                    review: None,
                    last_recovery: None,
                });
                view.snapshot.operation_id = Some(operation_id);
                view.snapshot.instance_id = Some(request.instance_id);
                view.snapshot.voice_mod = Some(to_mod(voice_mod));
                Ok::<_, Error>(())
            }))
            .unwrap_or(Err(Error::Internal));
            shared.finish(
                &core,
                if result.is_ok() {
                    Phase::ReviewRetirement
                } else {
                    Phase::Idle
                },
                result.err(),
            )
        })
        .await
        .map_err(|_| Error::Internal)?
    }

    pub(crate) async fn confirm_retirement(
        &self,
        request: dto::ConfirmRequest,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        if !uuid(&request.plan_id) || !hex(&request.digest) {
            return Err(Error::InvalidRequest);
        }
        let mut core = self
            .shared
            .authority
            .clone()
            .try_lock_owned()
            .map_err(|_| Error::Busy)?;
        {
            let view = self.shared.view()?;
            Shared::admission(&view)?;
            let review = view
                .snapshot
                .retirement_review
                .as_ref()
                .ok_or(Error::StaleReview)?;
            if review.plan_id != request.plan_id {
                return Err(Error::StaleReview);
            }
            let op = core.operation.as_ref().ok_or(Error::ContextChanged)?;
            if op.pending_retirement.is_none()
                || op.executor_uncertain
                || op
                    .durable_controller
                    .as_ref()
                    .is_none_or(|controller| controller.state() != RestorationState::Ready)
            {
                return Err(Error::StaleReview);
            }
        }
        self.shared
            .start(&core, &request.operation_id, Phase::Retiring)?;
        let shared = self.shared.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let op = core.operation.as_mut().ok_or(Error::Internal)?;
                op.retirement_review = None;
                #[cfg(test)]
                shared.hook(TestStage::Confirm);
                let prepared = op.pending_retirement.take().ok_or(Error::StaleReview)?;
                let guard = shared.guard(&op.instance).map_err(error)?;
                let retired = prepared
                    .confirm_with_executor_begin(&op.instance, &guard, &request.digest, || {
                        op.executor_uncertain = true;
                        #[cfg(test)]
                        shared.hook(TestStage::ExecutorBegin);
                    })
                    .map_err(error)?;
                // Successful commit is retained before publication or IPC can
                // finish; there is no fallible read-back or config inference.
                op.retired_receipt = Some(retired);
                op.durable_controller = None;
                op.executor_uncertain = false;
                #[cfg(test)]
                shared.hook(TestStage::OutcomeRetained);
                Ok::<_, Error>(())
            }))
            .unwrap_or(Err(Error::Internal));
            let phase = core
                .operation
                .as_ref()
                .map_or(Phase::Failed, Operation::stable_phase);
            shared.finish(&core, phase, result.err())
        })
        .await
        .map_err(|_| Error::Internal)?
    }
}
