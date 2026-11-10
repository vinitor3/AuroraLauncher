//! Explicit review of the same owned physical source after its previous Social
//! worker has finished. It neither creates an advertisement nor starts Java.
use super::*;

pub(super) struct SourceRevalidation {
    digest: String,
    deadline: Instant,
    receipt: SocialNativeSourceReceipt,
    binding: NativeVoiceHostBinding,
}

fn gate_error(error: ConnectError) -> &'static str {
    if error == ConnectError::Busy {
        "native_voice_social_still_active"
    } else {
        "native_voice_social_source_changed"
    }
}

impl Actor {
    pub(super) fn clear_source_revalidation(&mut self) {
        self.source_revalidation = None;
        self.view.review = None;
        self.view.native_review_deadline = None;
        self.view.social_revalidation_available = false;
        self.view.phase = "socialSource";
    }

    async fn observe_revalidation_source(
        &self,
        binding: &NativeVoiceHostBinding,
        receipt: &SocialNativeSourceReceipt,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> std::result::Result<(), &'static str> {
        self.connect
            .with_reclaim_native_source(receipt, || Ok(()))
            .map_err(gate_error)?;
        if !receipt.matches_binding(binding) {
            return Err("native_voice_social_source_changed");
        }
        if cancel.is_cancelled() || self.stop.load(Ordering::Acquire) || Instant::now() >= deadline
        {
            return Err("native_voice_revalidation_expired");
        }
        let source = binding.clone();
        // Retain the original query owner through completion, even if Stop or
        // the review deadline arrives. Abandoning review never stops Java.
        let job = tokio::task::spawn_blocking(move || {
            source
                .reserve_tcp()
                .and_then(|query| query.execute())
                .map_err(|_| "native_voice_revalidation_failed")?;
            let sample = source
                .reserve_voice()
                .and_then(|query| query.execute())
                .map_err(|_| "native_voice_revalidation_failed")?;
            if !source.accepts(&sample) {
                return Err("native_voice_revalidation_failed");
            }
            source
                .check_current()
                .map_err(|_| "native_voice_revalidation_failed")
        });
        job.await
            .map_err(|_| "native_voice_revalidation_failed")??;
        if cancel.is_cancelled() || self.stop.load(Ordering::Acquire) || Instant::now() >= deadline
        {
            return Err("native_voice_revalidation_expired");
        }
        self.connect
            .with_reclaim_native_source(receipt, || Ok(()))
            .map_err(gate_error)
    }

    pub(super) async fn review_social_source(
        &mut self,
        cancel: &CancellationToken,
    ) -> std::result::Result<(), &'static str> {
        if self.view.phase != "socialSource"
            || self.stop.load(Ordering::Acquire)
            || !self.claimed.load(Ordering::Acquire)
        {
            return Err("native_voice_source_not_ready");
        }
        let receipt = self
            .source_receipt
            .lock()
            .map_err(|_| "native_voice_internal")?
            .clone()
            .ok_or("native_voice_source_not_ready")?;
        self.connect
            .join_reclaim_native_source(&receipt)
            .await
            .map_err(gate_error)?;
        let binding = self
            .shared_source
            .lock()
            .map_err(|_| "native_voice_internal")?
            .clone()
            .ok_or("native_voice_source_not_ready")?;
        let deadline = (Instant::now() + Duration::from_secs(60)).min(binding.deadline());
        self.view.phase = "socialSourceReviewing";
        self.view.social_revalidation_available = false;
        self.view.error_code = None;
        self.publish();
        self.observe_revalidation_source(&binding, &receipt, deadline, cancel)
            .await?;
        let claim = binding
            .claim_digest()
            .map_err(|_| "native_voice_revalidation_failed")?;
        let scope = receipt.review_scope().map_err(gate_error)?;
        let material = serde_json::to_vec(&(
            "aurora.connect.native-source-revalidation/v1",
            Uuid::new_v4(),
            scope,
            claim,
            deadline
                .saturating_duration_since(Instant::now())
                .as_nanos(),
        ))
        .map_err(|_| "native_voice_internal")?;
        let digest = super::super::digest(&material);
        self.view.review = Some(NativeVoiceApproval {
            kind: "socialSource", digest: digest.clone(), remaining_ms: 0,
            before: "O servidor dedicado atual continua aberto; a sessão Social anterior está encerrada e seus recursos foram recolhidos.".into(),
            after: "Revalidar este mesmo servidor para uma nova sessão Social, sem reiniciar Java nem renovar o prazo original.".into(),
            details: vec![
                "A origem, o processo, o runtime e a configuração de voz foram observados novamente pelo Aurora.".into(),
                "Os vínculos anteriores serão invalidados definitivamente. Nenhuma conexão será reaberta por esta confirmação.".into(),
                "Depois, encerre o anúncio anterior e publique um novo anúncio no Social. A nova entrada exige sua própria revisão.".into(),
                format!("Prazo restante do servidor original: {} s.", binding.deadline().saturating_duration_since(Instant::now()).as_secs()),
            ],
        });
        self.source_revalidation = Some(SourceRevalidation {
            digest,
            deadline,
            receipt,
            binding,
        });
        self.view.native_review_deadline = Some(deadline);
        self.view.phase = "socialSourceReviewed";
        Ok(())
    }

    pub(super) async fn approve_social_source(
        &mut self,
        digest: &str,
        cancel: &CancellationToken,
    ) -> std::result::Result<(), &'static str> {
        self.approval("socialSource", digest)?;
        let plan = self
            .source_revalidation
            .take()
            .ok_or("native_voice_stale_review")?;
        if plan.digest != digest {
            return Err("native_voice_stale_review");
        }
        self.view.phase = "socialSourceApplying";
        self.publish();
        self.observe_revalidation_source(&plan.binding, &plan.receipt, plan.deadline, cancel)
            .await?;
        let source = self
            .source
            .as_ref()
            .ok_or("native_voice_source_not_ready")?;
        let renewed = self
            .connect
            .with_reclaim_native_source(&plan.receipt, || {
                if cancel.is_cancelled()
                    || self.stop.load(Ordering::Acquire)
                    || Instant::now() >= plan.deadline
                {
                    return Err(ConnectError::PlanExpired);
                }
                source
                    .renew_native_voice_host_binding(&plan.binding)
                    .map_err(|_| ConnectError::SourceChanged)
            })
            .map_err(gate_error)?;
        *self
            .shared_source
            .lock()
            .map_err(|_| "native_voice_internal")? = Some(renewed);
        self.claimed.store(false, Ordering::Release);
        self.clear_source_revalidation();
        self.view.phase = "ready";
        self.view.error_code = None;
        Ok(())
    }
}
