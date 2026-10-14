//! Explicit durable operations share the existing exclusive worker/exit barrier.
//! Reading history never confirms a plan or initializes Recovery storage.
use super::*;

impl Operation {
    pub(super) fn durable_projection(&self) -> Option<dto::DurableReceiptProjection> {
        if let Some(controller) = &self.durable_controller {
            Some(dto::DurableReceiptProjection {
                state: if self.executor_uncertain
                    || controller.state() == RestorationState::NeedsRecovery
                {
                    dto::DurableReceiptState::Uncertain
                } else {
                    dto::DurableReceiptState::Saved
                },
                receipt_id: Some(controller.receipt_id().into()),
                generation: Some(controller.generation()),
            })
        } else if self.durable_selected && self.executor_uncertain {
            Some(dto::DurableReceiptProjection {
                state: dto::DurableReceiptState::Uncertain,
                receipt_id: None,
                generation: None,
            })
        } else if self.pending_durable.is_some() {
            Some(dto::DurableReceiptProjection {
                state: dto::DurableReceiptState::Planned,
                receipt_id: None,
                generation: None,
            })
        } else {
            None
        }
    }
    pub(super) fn restoration_ready(&self) -> bool {
        !self.executor_uncertain
            && (self
                .controller
                .as_ref()
                .is_some_and(|c| c.state() == RestorationState::Ready && !c.has_pending_review())
                || self.durable_controller.as_ref().is_some_and(|c| {
                    c.state() == RestorationState::Ready && !c.has_pending_review()
                }))
    }
    pub(super) fn prepare_selected_restoration(
        &mut self,
        guard: &VoiceMutationGuard,
        current_hash: Option<&str>,
    ) -> Result<AnnouncementReview> {
        if let Some(controller) = &mut self.durable_controller {
            match current_hash {
                Some(hash) => controller.prepare_reconciled(&self.instance, guard, hash),
                None => controller.prepare_strict(&self.instance, guard),
            }
        } else {
            let controller = self
                .controller
                .as_mut()
                .ok_or(RecoveryError::StalePreview)?;
            match current_hash {
                Some(hash) => controller.prepare_reconciled(&self.instance, guard, hash),
                None => controller.prepare_strict(&self.instance, guard),
            }
        }
    }
    pub(super) fn confirm_selected_restoration(
        &mut self,
        guard: &VoiceMutationGuard,
        digest: &str,
    ) -> Result<RecoveryResult> {
        if let Some(controller) = &mut self.durable_controller {
            controller.confirm(&self.instance, guard, digest)
        } else {
            self.controller
                .as_mut()
                .ok_or(RecoveryError::StalePreview)?
                .confirm(&self.instance, guard, digest)
        }
    }
    pub(super) fn cancel_controller_reviews(&mut self) {
        if let Some(controller) = &mut self.controller {
            controller.cancel_review();
        }
        if let Some(controller) = &mut self.durable_controller {
            controller.cancel_review();
        }
    }
}

impl Manager {
    pub(crate) async fn prepare_apply_durable(
        &self,
        request: dto::PrepareApplyRequest,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        let mut core = self
            .shared
            .authority
            .clone()
            .try_lock_owned()
            .map_err(|_| Error::Busy)?;
        {
            let view = self.shared.view()?;
            Shared::admission(&view)?;
            if request.generation > MAX_GENERATION {
                return Err(Error::InvalidRequest);
            }
            if view.snapshot.generation != request.generation {
                return Err(Error::StaleReview);
            }
            let op = core.operation.as_ref().ok_or(Error::ContextChanged)?;
            if op.observed_hash.is_none()
                || op.pending_apply.is_some()
                || op.pending_durable.is_some()
                || op.restoration_state().is_some()
            {
                return Err(Error::StaleReview);
            }
        }
        self.shared
            .start(&core, &request.operation_id, Phase::PreparingApply)?;
        core.operation
            .as_mut()
            .ok_or(Error::Internal)?
            .durable_selected = true;
        self.shared.view()?.snapshot.durable_receipt = Some(dto::DurableReceiptProjection {
            state: dto::DurableReceiptState::Planned,
            receipt_id: None,
            generation: None,
        });
        let shared = self.shared.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let op = core.operation.as_mut().ok_or(Error::Internal)?;
                #[cfg(test)]
                shared.hook(TestStage::Prepare);
                let guard = shared.guard(&op.instance).map_err(error)?;
                let prepared = prepare_durable_announcement(
                    &op.instance,
                    &guard,
                    AnnouncementRequest {
                        voice_mod: op.voice_mod,
                        mode: AnnouncementMode::DedicatedAuroraOnly,
                        guest_port: 35554,
                        expected_source_sha256: op
                            .observed_hash
                            .clone()
                            .ok_or(Error::StaleReview)?,
                    },
                )
                .map_err(error)?;
                let review = present_review(prepared.review())?;
                op.pending_durable = Some(prepared);
                op.review = Some(review);
                Ok::<_, Error>(())
            }))
            .unwrap_or(Err(Error::Internal));
            if result.is_err() {
                if let Some(op) = &mut core.operation {
                    op.observed_hash = None;
                    op.observation = None;
                }
            }
            shared.finish(
                &core,
                if result.is_ok() {
                    Phase::ReviewApply
                } else {
                    Phase::Failed
                },
                result.err(),
            )
        })
        .await
        .map_err(|_| Error::Internal)?
    }

    pub(crate) async fn list_receipts(
        &self,
        data_dir: &Path,
        request: dto::ListReceiptsRequest,
    ) -> Outcome<dto::ReceiptList> {
        let id = crate::engine::InstanceId::parse(&request.instance_id)
            .map_err(|_| Error::InvalidRequest)?;
        let core = self
            .shared
            .authority
            .clone()
            .try_lock_owned()
            .map_err(|_| Error::Busy)?;
        {
            let view = self.shared.view()?;
            Shared::admission(&view)?;
            if core.operation.is_some() {
                return Err(Error::ContextActive);
            }
        }
        let instance = crate::engine::InstanceLayout::new(data_dir).open(id);
        if !diagnostic_lab::is_lab_candidate(&instance) {
            return Err(Error::UnsafePath);
        }
        let shared = self.shared.clone();
        tauri::async_runtime::spawn_blocking(move || {
            // This guard survives abandonment of the request future and makes
            // exit wait until read-only handles and the instance lock are gone.
            let _authority = core;
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                #[cfg(test)]
                shared.hook(TestStage::ListReceipts);
                let guard = shared.guard(&instance).map_err(error)?;
                let records = list_restoration_receipts(&instance, &guard).map_err(error)?;
                if records.len() > 64 {
                    return Err(Error::Limit);
                }
                let receipts = records
                    .into_iter()
                    .map(|record| {
                        let state = match (record.state, record.generation) {
                            (RestorationState::Ready, 0) => dto::SavedReceiptState::Ready,
                            (RestorationState::Restored, 1) => dto::SavedReceiptState::Restored,
                            _ => return Err(Error::Integrity),
                        };
                        Ok(dto::SavedReceiptSummary {
                            receipt_id: record.receipt_id,
                            voice_mod: to_mod(record.voice_mod),
                            state,
                            generation: record.generation,
                        })
                    })
                    .collect::<Outcome<Vec<_>>>()?;
                Ok(dto::ReceiptList {
                    instance_id: request.instance_id,
                    receipts,
                })
            }))
            .unwrap_or(Err(Error::Internal))
        })
        .await
        .map_err(|_| Error::Internal)?
    }

    pub(crate) async fn resume_receipt(
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
            Shared::phase(&mut view, Phase::ResumingReceipt, true);
        }
        let shared = self.shared.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                #[cfg(test)]
                shared.hook(TestStage::ResumeReceipt);
                let guard = shared.guard(&instance).map_err(error)?;
                let controller =
                    resume_restoration(&instance, &guard, &request.receipt_id).map_err(error)?;
                if controller.state() != RestorationState::Ready
                    || controller.generation() != request.expected_generation
                {
                    return Err(Error::StaleReview);
                }
                let operation_id = Uuid::new_v4().to_string();
                let voice_mod = controller.voice_mod();
                // Acquire the presentation lock before publishing ownership;
                // no fallible operation may leave a hidden resumed controller.
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
                    Phase::Applied
                } else {
                    Phase::Idle
                },
                result.err(),
            )
        })
        .await
        .map_err(|_| Error::Internal)?
    }
}
