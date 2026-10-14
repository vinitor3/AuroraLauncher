//! Private owner of plans and receipts. Blocking jobs retain their own authority.
use super::*;
use crate::engine::connect_voice_desktop as dto;
use dto::{VoiceAnnouncementError as Error, VoiceAnnouncementPhase as Phase};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use tauri::async_runtime::Mutex as AsyncMutex;
#[path = "connect_voice_desktop_durable.rs"]
mod durable;

type Outcome<T> = dto::VoiceAnnouncementResult<T>;
const MAX_GENERATION: u64 = 9_007_199_254_740_991;
const CLOSE_SECONDS: u64 = 60;

#[derive(Clone, Default)]
pub(crate) struct Manager {
    shared: Arc<Shared>,
}
#[derive(Default)]
struct Shared {
    authority: Arc<AsyncMutex<Core>>,
    view: Mutex<View>,
    #[cfg(test)]
    test_hooks: TestHooks,
}
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum TestStage {
    Inspect,
    Prepare,
    Confirm,
    ExecutorBegin,
    OutcomeRetained,
    CompletionPublished,
    CloseTask,
    CancelCloseTask,
    ListReceipts,
    ResumeReceipt,
}
#[cfg(test)]
type TestProbe = Arc<dyn Fn() -> Result<()> + Send + Sync>;
#[cfg(test)]
type TestStageHook = Arc<dyn Fn(TestStage) + Send + Sync>;
#[cfg(test)]
#[derive(Default)]
struct TestHooks {
    probe: Mutex<Option<TestProbe>>,
    stage: Mutex<Option<TestStageHook>>,
    civil: std::sync::atomic::AtomicU64,
}
#[derive(Default)]
struct Core {
    operation: Option<Operation>,
}
struct Operation {
    id: String,
    instance: Instance,
    voice_mod: VoiceMod,
    observed_hash: Option<String>,
    pending_apply: Option<PreparedAnnouncement>,
    pending_durable: Option<PreparedDurableAnnouncement>,
    controller: Option<RestorationController>,
    durable_controller: Option<DurableRestorationController>,
    durable_selected: bool,
    executor_uncertain: bool,
    observation: Option<dto::AnnouncementInspection>,
    review: Option<dto::VoiceAnnouncementReview>,
    last_recovery: Option<dto::RecoverySummary>,
}
struct View {
    snapshot: dto::VoiceAnnouncementSnapshot,
    base_phase: Phase,
    base_busy: bool,
    close_deadline: Option<Instant>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            snapshot: Default::default(),
            base_phase: Phase::Idle,
            base_busy: false,
            close_deadline: None,
        }
    }
}

fn uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}
fn hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn civil() -> Outcome<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|v| u64::try_from(v.as_millis()).ok())
        .filter(|v| *v <= MAX_GENERATION)
        .ok_or(Error::Internal)
}
fn from_mod(value: dto::VoiceMod) -> VoiceMod {
    match value {
        dto::VoiceMod::SimpleVoiceChat => VoiceMod::SimpleVoiceChat,
        dto::VoiceMod::PlasmoVoice => VoiceMod::PlasmoVoice,
    }
}
fn to_mod(value: VoiceMod) -> dto::VoiceMod {
    match value {
        VoiceMod::SimpleVoiceChat => dto::VoiceMod::SimpleVoiceChat,
        VoiceMod::PlasmoVoice => dto::VoiceMod::PlasmoVoice,
    }
}
fn error(value: RecoveryError) -> Error {
    match value {
        RecoveryError::Io | RecoveryError::Storage | RecoveryError::NoSpace => Error::Storage,
        RecoveryError::Integrity => Error::Integrity,
        RecoveryError::UnsafePath | RecoveryError::NotFound => Error::UnsafePath,
        RecoveryError::Changed | RecoveryError::StalePreview => Error::StaleReview,
        RecoveryError::OriginChanged | RecoveryError::Identity => Error::ContextChanged,
        RecoveryError::Compatibility => Error::Incompatible,
        RecoveryError::WrongLock => Error::InstanceBusy,
        RecoveryError::VoiceProcessRunning => Error::JavaRunning,
        RecoveryError::VoiceProcessUnavailable => Error::ProcessUnavailable,
        RecoveryError::Limit => Error::Limit,
        RecoveryError::Pending => Error::NeedsRecovery,
    }
}
fn present_observation(value: AnnouncementInspection) -> dto::AnnouncementInspection {
    dto::AnnouncementInspection {
        voice_mod: to_mod(value.voice_mod),
        artifact_version: value.artifact_version,
        artifact_sha256: value.artifact_sha256,
        artifact_bytes: value.artifact_bytes,
        config_path: value.config_path,
        source_sha256: value.source_sha256,
        source_bytes: value.source_bytes,
        announcement: match value.announcement {
            ObservedAnnouncement::SimpleVoiceChat { voice_host } => {
                dto::ObservedAnnouncement::SimpleVoiceChat { voice_host }
            }
            ObservedAnnouncement::PlasmoAbsent => dto::ObservedAnnouncement::PlasmoAbsent,
            ObservedAnnouncement::PlasmoPublic { ip, port } => {
                dto::ObservedAnnouncement::PlasmoPublic { ip, port }
            }
        },
        limitations: value.limitations,
    }
}
fn present_review(value: &AnnouncementReview) -> Outcome<dto::VoiceAnnouncementReview> {
    if value.before.len() > 2048
        || value.after.len() > 2048
        || value.warning.len() > 1024
        || value.expires_at_ms > MAX_GENERATION
    {
        return Err(Error::Limit);
    }
    Ok(dto::VoiceAnnouncementReview {
        action: match value.action {
            AnnouncementAction::SetAnnouncement => dto::ReviewAction::SetAnnouncement,
            AnnouncementAction::RestoreOriginal { .. } => dto::ReviewAction::RestoreOriginal,
            AnnouncementAction::RestoreReconciled { .. } => dto::ReviewAction::RestoreReconciled,
        },
        voice_mod: to_mod(value.voice_mod),
        plan_id: value.plan_id.clone(),
        digest: value.digest.clone(),
        config_path: value.config_path.clone(),
        before: value.before.clone(),
        after: value.after.clone(),
        source_sha256: value.source_sha256.clone(),
        replacement_sha256: value.replacement_sha256.clone(),
        expires_at_ms: value.expires_at_ms,
        warning: value.warning.clone(),
    })
}
fn present_recovery(value: RecoveryResult) -> dto::RecoverySummary {
    dto::RecoverySummary {
        restored_point_id: value.restored_point_id,
        undo_available: value.undo_available,
        recovered_interrupted: value.recovered_interrupted,
    }
}
impl Operation {
    fn restoration_state(&self) -> Option<dto::RestorationState> {
        if self.executor_uncertain {
            return Some(dto::RestorationState::NeedsRecovery);
        }
        self.controller
            .as_ref()
            .map(|c| c.state())
            .or_else(|| self.durable_controller.as_ref().map(|c| c.state()))
            .map(|state| match state {
                RestorationState::Ready => dto::RestorationState::Ready,
                RestorationState::NeedsRecovery => dto::RestorationState::NeedsRecovery,
                RestorationState::Restored => dto::RestorationState::Restored,
            })
    }
    fn stable_phase(&self) -> Phase {
        match self.restoration_state() {
            Some(dto::RestorationState::Ready) => Phase::Applied,
            Some(dto::RestorationState::NeedsRecovery) => Phase::NeedsRecovery,
            Some(dto::RestorationState::Restored) => Phase::Restored,
            None if self.observed_hash.is_some() => Phase::Observed,
            None => Phase::Failed,
        }
    }
    fn cancel_review(&mut self) {
        self.pending_apply = None;
        self.pending_durable = None;
        self.review = None;
        self.cancel_controller_reviews();
    }
}
impl Shared {
    fn guard(&self, instance: &Instance) -> Result<VoiceMutationGuard> {
        #[cfg(test)]
        if let Some(probe) = self.test_hooks.probe.lock().unwrap().clone() {
            return process::acquire_with_probe(instance, Box::new(move || probe()));
        }
        acquire_voice_mutation_guard(instance)
    }
    #[cfg(test)]
    fn hook(&self, stage: TestStage) {
        let hook = self.test_hooks.stage.lock().unwrap().clone();
        if let Some(hook) = hook {
            hook(stage);
        }
    }
    fn now(&self) -> Outcome<u64> {
        #[cfg(test)]
        {
            let now = self
                .test_hooks
                .civil
                .load(std::sync::atomic::Ordering::SeqCst);
            if now != 0 {
                return Ok(now);
            }
        }
        civil()
    }
    fn view(&self) -> Outcome<std::sync::MutexGuard<'_, View>> {
        self.view.lock().map_err(|_| Error::Internal)
    }
    fn snapshot(&self) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        Ok(self.view()?.snapshot.clone())
    }
    fn advance(view: &mut View) -> Outcome<()> {
        view.snapshot.generation = view
            .snapshot
            .generation
            .checked_add(1)
            .filter(|n| *n <= MAX_GENERATION)
            .ok_or(Error::Limit)?;
        Ok(())
    }
    fn phase(view: &mut View, phase: Phase, busy: bool) {
        view.base_phase = phase;
        view.base_busy = busy;
        if view
            .snapshot
            .close
            .as_ref()
            .is_some_and(|c| c.state == dto::CloseState::Waiting)
        {
            view.snapshot.phase = Phase::Closing;
            view.snapshot.busy = true;
        } else {
            view.snapshot.phase = phase;
            view.snapshot.busy = busy;
        }
    }
    fn admission(view: &View) -> Outcome<()> {
        if view.snapshot.close.is_some() {
            Err(Error::ClosePending)
        } else if view.snapshot.busy {
            Err(Error::Busy)
        } else if view.snapshot.generation >= MAX_GENERATION - 8 {
            Err(Error::Limit)
        } else {
            Ok(())
        }
    }
    fn start(&self, core: &Core, operation_id: &str, phase: Phase) -> Outcome<()> {
        if !uuid(operation_id) {
            return Err(Error::InvalidRequest);
        }
        let mut view = self.view()?;
        Self::admission(&view)?;
        if core
            .operation
            .as_ref()
            .is_none_or(|op| op.id != operation_id)
        {
            return Err(Error::ContextChanged);
        }
        Self::advance(&mut view)?;
        view.snapshot.last_error_code = None;
        view.snapshot.review = None;
        if phase == Phase::Applying {
            view.snapshot.observation = None;
        }
        Self::phase(&mut view, phase, true);
        Ok(())
    }
    fn finish(
        &self,
        core: &Core,
        phase: Phase,
        failure: Option<Error>,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        let mut view = self.view()?;
        Self::advance(&mut view)?;
        view.snapshot.restoration_state = core
            .operation
            .as_ref()
            .and_then(Operation::restoration_state);
        view.snapshot.durable_receipt = core
            .operation
            .as_ref()
            .and_then(Operation::durable_projection);
        if let Some(op) = core.operation.as_ref() {
            view.snapshot.observation = op.observation.clone();
            view.snapshot.last_recovery_result = op.last_recovery.clone();
            view.snapshot.review = if view.snapshot.close.is_some() {
                None
            } else {
                op.review.clone()
            };
        }
        view.snapshot.last_error_code = failure;
        Self::phase(&mut view, phase, false);
        let snapshot = view.snapshot.clone();
        drop(view);
        #[cfg(test)]
        self.hook(TestStage::CompletionPublished);
        Ok(snapshot)
    }
}

impl Manager {
    pub(crate) fn snapshot(&self) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        self.shared.snapshot()
    }

    // No await separates taking authority, publishing busy, and spawning. Once
    // spawned, the closure owns the mutex guard and installs the final outcome.
    pub(crate) async fn inspect(
        &self,
        data_dir: &Path,
        request: dto::InspectRequest,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        let id = crate::engine::InstanceId::parse(&request.instance_id)
            .map_err(|_| Error::InvalidRequest)?;
        let mut core = self
            .shared
            .authority
            .clone()
            .try_lock_owned()
            .map_err(|_| Error::Busy)?;
        {
            let mut view = self.shared.view()?;
            Shared::admission(&view)?;
            if let Some(op) = core.operation.as_ref() {
                if op.instance.id().as_str() != id.as_str()
                    || to_mod(op.voice_mod) != request.voice_mod
                {
                    return Err(Error::ContextActive);
                }
                if op.restoration_state().is_some()
                    || op.pending_apply.is_some()
                    || op.pending_durable.is_some()
                {
                    return Err(Error::ContextActive);
                }
            }
            let operation_id = Uuid::new_v4().to_string();
            core.operation = Some(Operation {
                id: operation_id.clone(),
                instance: crate::engine::InstanceLayout::new(data_dir).open(id),
                voice_mod: from_mod(request.voice_mod),
                observed_hash: None,
                pending_apply: None,
                pending_durable: None,
                controller: None,
                durable_controller: None,
                durable_selected: false,
                executor_uncertain: false,
                observation: None,
                review: None,
                last_recovery: None,
            });
            Shared::advance(&mut view)?;
            view.snapshot = dto::VoiceAnnouncementSnapshot {
                generation: view.snapshot.generation,
                operation_id: Some(operation_id),
                instance_id: Some(request.instance_id),
                voice_mod: Some(request.voice_mod),
                ..Default::default()
            };
            Shared::phase(&mut view, Phase::Inspecting, true);
        }
        let shared = self.shared.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let op = core.operation.as_mut().ok_or(Error::Internal)?;
                // Reject an out-of-scope selection before even acquiring the
                // guard (which may create the ordinary instance lock file).
                if !diagnostic_lab::is_lab_candidate(&op.instance) {
                    return Err(Error::UnsafePath);
                }
                #[cfg(test)]
                shared.hook(TestStage::Inspect);
                let guard = shared.guard(&op.instance).map_err(error)?;
                let observed =
                    inspect_announcement(&op.instance, &guard, op.voice_mod).map_err(error)?;
                op.observed_hash = Some(observed.source_sha256.clone());
                op.observation = Some(present_observation(observed));
                Ok::<_, Error>(())
            }))
            .unwrap_or(Err(Error::Internal));
            shared.finish(
                &core,
                if result.is_ok() {
                    Phase::Observed
                } else {
                    Phase::Failed
                },
                result.err(),
            )
        })
        .await
        .map_err(|_| Error::Internal)?
    }

    pub(crate) async fn prepare_apply(
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
            .durable_selected = false;
        let shared = self.shared.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let op = core.operation.as_mut().ok_or(Error::Internal)?;
                #[cfg(test)]
                shared.hook(TestStage::Prepare);
                let guard = shared.guard(&op.instance).map_err(error)?;
                let prepared = prepare_announcement(
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
                op.pending_apply = Some(prepared);
                op.review = Some(review);
                Ok::<_, Error>(())
            }))
            .unwrap_or(Err(Error::Internal));
            if result.is_err() {
                if let Some(op) = core.operation.as_mut() {
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

    pub(crate) async fn prepare_restore(
        &self,
        request: dto::OperationRequest,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        self.prepare_restoration(request, false).await
    }
    pub(crate) async fn prepare_restore_reconciled(
        &self,
        request: dto::OperationRequest,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        self.prepare_restoration(request, true).await
    }
    async fn prepare_restoration(
        &self,
        request: dto::OperationRequest,
        reconciled: bool,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        let mut core = self
            .shared
            .authority
            .clone()
            .try_lock_owned()
            .map_err(|_| Error::Busy)?;
        let op = core.operation.as_ref().ok_or(Error::ContextChanged)?;
        if !op.restoration_ready() {
            return Err(Error::StaleReview);
        }
        self.shared
            .start(&core, &request.operation_id, Phase::PreparingRestore)?;
        let shared = self.shared.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let op = core.operation.as_mut().ok_or(Error::Internal)?;
                #[cfg(test)]
                shared.hook(TestStage::Prepare);
                let guard = shared.guard(&op.instance).map_err(error)?;
                let current_hash = if reconciled {
                    // The strict read-only DB pin ends before the writer-based
                    // preparation. No mod/JAR gate or arbitrary path is used.
                    let hash = recovery::with_existing_observation(
                        &op.instance,
                        guard.inner(),
                        |current| {
                            guard.recheck()?;
                            current
                                .files
                                .iter()
                                .find(|file| file.path == op.voice_mod.config())
                                .map(|f| f.sha256.clone())
                                .ok_or(RecoveryError::UnsafePath)
                        },
                    )
                    .map_err(error)?;
                    guard.recheck().map_err(error)?;
                    Some(hash)
                } else {
                    None
                };
                let review = op
                    .prepare_selected_restoration(&guard, current_hash.as_deref())
                    .map_err(error)?;
                let presented = match present_review(&review) {
                    Ok(r) => r,
                    Err(e) => {
                        op.cancel_controller_reviews();
                        return Err(e);
                    }
                };
                op.review = Some(presented);
                Ok::<_, Error>(())
            }))
            .unwrap_or(Err(Error::Internal));
            let phase = if result.is_ok() {
                Phase::ReviewRestore
            } else {
                core.operation
                    .as_ref()
                    .map_or(Phase::Failed, Operation::stable_phase)
            };
            shared.finish(&core, phase, result.err())
        })
        .await
        .map_err(|_| Error::Internal)?
    }

    pub(crate) async fn confirm(
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
        let apply = {
            let view = self.shared.view()?;
            Shared::admission(&view)?;
            let review = view.snapshot.review.as_ref().ok_or(Error::StaleReview)?;
            if review.plan_id != request.plan_id {
                return Err(Error::StaleReview);
            }
            review.action == dto::ReviewAction::SetAnnouncement
        };
        self.shared.start(
            &core,
            &request.operation_id,
            if apply {
                Phase::Applying
            } else {
                Phase::Restoring
            },
        )?;
        {
            let mut view = self.shared.view()?;
            view.snapshot.review = None;
            if apply {
                view.snapshot.observation = None;
            }
        }
        let shared = self.shared.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let op = core.operation.as_mut().ok_or(Error::Internal)?;
                op.review = None;
                #[cfg(test)]
                shared.hook(TestStage::Confirm);
                if apply {
                    op.observed_hash = None;
                    op.observation = None;
                    if let Some(prepared) = op.pending_durable.take() {
                        let guard = shared.guard(&op.instance).map_err(error)?;
                        let applied = prepared
                            .confirm_with_executor_begin(
                                &op.instance,
                                &guard,
                                &request.digest,
                                || {
                                    op.executor_uncertain = true;
                                    #[cfg(test)]
                                    shared.hook(TestStage::ExecutorBegin);
                                },
                            )
                            .map_err(error)?;
                        op.durable_controller = Some(applied.receipt);
                        op.last_recovery = Some(present_recovery(applied.recovery));
                    } else {
                        let prepared = op.pending_apply.take().ok_or(Error::StaleReview)?;
                        let guard = shared.guard(&op.instance).map_err(error)?;
                        let receipt = RestorationReceipt::from_plan(&prepared);
                        let recovery = prepared
                            .apply(&op.instance, &guard, &request.digest, || {
                                op.executor_uncertain = true;
                                #[cfg(test)]
                                shared.hook(TestStage::ExecutorBegin);
                            })
                            .map_err(error)?;
                        op.controller = Some(receipt.into_controller());
                        op.last_recovery = Some(present_recovery(recovery));
                    }
                    op.executor_uncertain = false;
                } else {
                    // Even failure acquiring a fresh guard consumes the review.
                    let guard = match shared.guard(&op.instance) {
                        Ok(g) => g,
                        Err(e) => {
                            op.cancel_controller_reviews();
                            return Err(error(e));
                        }
                    };
                    let result = op
                        .confirm_selected_restoration(&guard, &request.digest)
                        .map_err(error)?;
                    op.last_recovery = Some(present_recovery(result));
                }
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

    pub(crate) async fn cancel_review(
        &self,
        request: dto::OperationRequest,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        if !uuid(&request.operation_id) {
            return Err(Error::InvalidRequest);
        }
        let mut core = self
            .shared
            .authority
            .clone()
            .try_lock_owned()
            .map_err(|_| Error::Busy)?;
        let mut view = self.shared.view()?;
        Shared::admission(&view)?;
        let op = core.operation.as_mut().ok_or(Error::ContextChanged)?;
        if op.id != request.operation_id {
            return Err(Error::ContextChanged);
        }
        op.cancel_review();
        let phase = op.stable_phase();
        view.snapshot.durable_receipt = op.durable_projection();
        view.snapshot.review = None;
        view.snapshot.last_error_code = None;
        Shared::phase(&mut view, phase, false);
        Shared::advance(&mut view)?;
        Ok(view.snapshot.clone())
    }

    pub(crate) async fn begin_close(
        &self,
        request: dto::BeginCloseRequest,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        if request.intent == dto::CloseIntent::Exit {
            return Err(Error::InvalidRequest);
        }
        self.close(request.operation_id, request.intent, false)
            .await
    }
    pub(crate) async fn begin_exit(&self) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        self.close(None, dto::CloseIntent::Exit, true).await
    }
    async fn close(
        &self,
        operation_id: Option<String>,
        intent: dto::CloseIntent,
        native: bool,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        let close_id = {
            let mut view = self.shared.view()?;
            if !native && operation_id != view.snapshot.operation_id {
                return Err(Error::ContextChanged);
            }
            if operation_id.as_deref().is_some_and(|id| !uuid(id)) {
                return Err(Error::InvalidRequest);
            }
            if let Some(close) = view.snapshot.close.as_ref() {
                if close.intent != intent {
                    return Err(Error::ClosePending);
                }
                close.close_id.clone()
            } else {
                Shared::advance(&mut view)?;
                let close_id = Uuid::new_v4().to_string();
                view.snapshot.close = Some(dto::VoiceAnnouncementClose {
                    close_id: close_id.clone(),
                    intent,
                    state: dto::CloseState::Waiting,
                    review: None,
                });
                view.snapshot.review = None;
                view.snapshot.phase = Phase::Closing;
                view.snapshot.busy = true;
                close_id
            }
        };
        let shared = self.shared.clone();
        // The quiesce task survives abandonment of the IPC future too. It never
        // aborts a running file job and only cancels an unexecuted review.
        tauri::async_runtime::spawn(async move {
            #[cfg(test)]
            shared.hook(TestStage::CloseTask);
            let mut core = shared.authority.clone().lock_owned().await;
            let mut view = shared.view()?;
            let Some(close) = view.snapshot.close.as_ref() else {
                return Ok(view.snapshot.clone());
            };
            if close.close_id != close_id || close.state != dto::CloseState::Waiting {
                return Ok(view.snapshot.clone());
            }
            if let Some(op) = core.operation.as_mut() {
                op.cancel_review();
            }
            view.snapshot.durable_receipt = core
                .operation
                .as_ref()
                .and_then(Operation::durable_projection);
            view.snapshot.review = None;
            view.base_phase = core
                .operation
                .as_ref()
                .map_or(Phase::Idle, Operation::stable_phase);
            view.base_busy = false;
            Shared::advance(&mut view)?;
            view.snapshot.busy = false;
            let needs_consent = core.operation.as_ref().is_some_and(|op| {
                matches!(
                    op.restoration_state(),
                    Some(dto::RestorationState::Ready | dto::RestorationState::NeedsRecovery)
                )
            });
            if needs_consent {
                let op = core.operation.as_ref().ok_or(Error::Internal)?;
                let close_review = (|| -> Outcome<dto::CloseReview> {
                    let expires_at_ms = shared
                        .now()?
                        .checked_add(CLOSE_SECONDS * 1000)
                        .filter(|n| *n <= MAX_GENERATION)
                        .ok_or(Error::Limit)?;
                    let phase = op.stable_phase();
                    let durable_receipt = op.durable_projection();
                    let common = (
                        "aurora.voice.close.v1",
                        "main",
                        &close_id,
                        &op.id,
                        view.snapshot.generation,
                        intent,
                        op.instance.id().as_str(),
                        to_mod(op.voice_mod),
                        phase,
                        &view.snapshot.last_recovery_result,
                        expires_at_ms,
                    );
                    let bytes = if let Some(receipt) = &durable_receipt {
                        serde_json::to_vec(&("aurora.voice.close.durable.v1", &common, receipt))
                    } else {
                        serde_json::to_vec(&common)
                    }
                    .map_err(|_| Error::Internal)?;
                    let hash = digest(&bytes);
                    Ok(dto::CloseReview {
                        digest: hash,
                        expires_at_ms,
                        instance_id: op.instance.id().as_str().into(),
                        voice_mod: to_mod(op.voice_mod),
                        state: phase,
                        selective_restoration_lost: !durable_receipt.as_ref().is_some_and(
                            |receipt| receipt.state == dto::DurableReceiptState::Saved,
                        ),
                        durable_receipt,
                    })
                })();
                view.close_deadline = close_review
                    .as_ref()
                    .ok()
                    .map(|_| Instant::now() + Duration::from_secs(CLOSE_SECONDS));
                view.snapshot.last_error_code =
                    close_review.as_ref().err().map(|_| Error::StaleClose);
                let close = view.snapshot.close.as_mut().ok_or(Error::Internal)?;
                close.state = dto::CloseState::ReviewRequired;
                close.review = close_review.ok();
                view.snapshot.phase = Phase::CloseReview;
            } else {
                core.operation = None;
                view.close_deadline = None;
                let close = view.snapshot.close.as_mut().ok_or(Error::Internal)?;
                close.state = dto::CloseState::Ready;
                view.snapshot.phase = Phase::Closed;
                view.snapshot.observation = None;
                view.snapshot.restoration_state = None;
            }
            Ok(view.snapshot.clone())
        })
        .await
        .map_err(|_| Error::Internal)?
    }

    pub(crate) async fn cancel_close(
        &self,
        request: dto::CloseRequest,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        if !uuid(&request.close_id) {
            return Err(Error::InvalidRequest);
        }
        {
            let view = self.shared.view()?;
            let close = view.snapshot.close.as_ref().ok_or(Error::StaleClose)?;
            if close.close_id != request.close_id || close.state == dto::CloseState::Ready {
                return Err(Error::StaleClose);
            }
        }
        let shared = self.shared.clone();
        tauri::async_runtime::spawn(async move {
            #[cfg(test)]
            shared.hook(TestStage::CancelCloseTask);
            let mut core = shared.authority.clone().lock_owned().await;
            let mut view = shared.view()?;
            let close = view.snapshot.close.as_ref().ok_or(Error::StaleClose)?;
            if close.close_id != request.close_id || close.state == dto::CloseState::Ready {
                return Err(Error::StaleClose);
            }
            if let Some(op) = core.operation.as_mut() {
                op.cancel_review();
            }
            view.snapshot.review = None;
            view.snapshot.close = None;
            view.snapshot.durable_receipt = core
                .operation
                .as_ref()
                .and_then(Operation::durable_projection);
            view.close_deadline = None;
            view.base_phase = core
                .operation
                .as_ref()
                .map_or(Phase::Idle, Operation::stable_phase);
            view.base_busy = false;
            view.snapshot.phase = view.base_phase;
            view.snapshot.busy = false;
            view.snapshot.last_error_code = None;
            Shared::advance(&mut view)?;
            Ok(view.snapshot.clone())
        })
        .await
        .map_err(|_| Error::Internal)?
    }
    pub(crate) async fn confirm_close(
        &self,
        request: dto::ConfirmCloseRequest,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        if !uuid(&request.close_id) || !hex(&request.digest) {
            return Err(Error::InvalidRequest);
        }
        let mut core = self
            .shared
            .authority
            .clone()
            .try_lock_owned()
            .map_err(|_| Error::Busy)?;
        let mut view = self.shared.view()?;
        let close = view.snapshot.close.as_mut().ok_or(Error::StaleClose)?;
        if close.close_id != request.close_id || close.state != dto::CloseState::ReviewRequired {
            return Err(Error::StaleClose);
        }
        let review = close.review.take();
        let deadline = view.close_deadline.take();
        Shared::advance(&mut view)?;
        let valid = review.as_ref().is_some_and(|r| {
            r.digest == request.digest && self.shared.now().is_ok_and(|now| now < r.expires_at_ms)
        }) && deadline.is_some_and(|d| Instant::now() < d);
        if !valid {
            view.snapshot.last_error_code = Some(Error::StaleClose);
            return Ok(view.snapshot.clone());
        }
        core.operation = None;
        view.snapshot.close.as_mut().ok_or(Error::Internal)?.state = dto::CloseState::Ready;
        view.snapshot.phase = Phase::Closed;
        view.snapshot.busy = false;
        view.snapshot.observation = None;
        view.snapshot.review = None;
        view.snapshot.restoration_state = None;
        view.snapshot.last_error_code = None;
        Ok(view.snapshot.clone())
    }
    pub(crate) async fn finish_close(
        &self,
        request: dto::CloseRequest,
    ) -> Outcome<dto::VoiceAnnouncementSnapshot> {
        if !uuid(&request.close_id) {
            return Err(Error::InvalidRequest);
        }
        let mut core = self
            .shared
            .authority
            .clone()
            .try_lock_owned()
            .map_err(|_| Error::Busy)?;
        let mut view = self.shared.view()?;
        let close = view.snapshot.close.as_ref().ok_or(Error::StaleClose)?;
        if close.close_id != request.close_id
            || close.state != dto::CloseState::Ready
            || close.intent == dto::CloseIntent::Exit
        {
            return Err(Error::StaleClose);
        }
        core.operation = None;
        Shared::advance(&mut view)?;
        view.snapshot = dto::VoiceAnnouncementSnapshot {
            generation: view.snapshot.generation,
            ..Default::default()
        };
        view.base_phase = Phase::Idle;
        view.base_busy = false;
        view.close_deadline = None;
        Ok(view.snapshot.clone())
    }
    pub(crate) fn exit_ready(&self) -> bool {
        self.shared.view().is_ok_and(|view| {
            view.snapshot.close.as_ref().is_some_and(|c| {
                c.intent == dto::CloseIntent::Exit && c.state == dto::CloseState::Ready
            })
        })
    }
}

#[cfg(test)]
#[path = "connect_voice_desktop_tests.rs"]
mod tests;
