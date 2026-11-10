//! One native actor owns the source, its original completion and local player
//! bridges. IPC waiters own no process, query, socket or restoration authority.
use super::super::{DurableRestorationController, RestorationState};
use super::*;
use crate::engine::{
    connect::{ConnectError, ConnectManager, GroupParticipantRequest, SocialNativeSourceReceipt},
    connect_native_voice::*,
    connect_owned_run::OwnedRunFailure,
};
use aurora_connect_lab as lab;
use producer::{
    DedicatedFailure, DedicatedPhase, DedicatedPlan, DedicatedReport, DedicatedRun, EulaDisposition,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

type Outcome = std::result::Result<NativeVoiceSnapshot, &'static str>;
type Message = (NativeVoiceRequest, oneshot::Sender<Outcome>);
struct ReservationBusy<'a>(&'a AtomicBool);
impl Drop for ReservationBusy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
const TRANSPORT_CLEANUP_FAILED: &str = "native_voice_transport_cleanup_failed";
const FORCED_STOP: &str = "native_voice_stop_forced";
const FORCED_STOP_CLEANUP_FAILED: &str = "native_voice_stop_forced_cleanup_failed";
#[path = "desktop_source_revalidation.rs"]
mod source_revalidation;
use source_revalidation::SourceRevalidation;
struct Client {
    sender: mpsc::Sender<Message>,
    snapshot: Arc<Mutex<NativeVoiceSnapshot>>,
    busy: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    cancel: CancellationToken,
    worker: Mutex<Option<JoinHandle<()>>>,
    completion: Arc<Mutex<Option<producer::DedicatedCompletion>>>,
    source: Arc<Mutex<Option<NativeVoiceHostBinding>>>,
    claimed: Arc<AtomicBool>,
    source_receipt: Arc<Mutex<Option<SocialNativeSourceReceipt>>>,
    epoch: u64,
}
impl Drop for Client {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
#[derive(Default)]
pub(crate) struct DesktopVoiceManager {
    client: Mutex<Option<Arc<Client>>>,
}
impl DesktopVoiceManager {
    pub(crate) fn snapshot(&self) -> Outcome {
        let client = self.client.lock().map_err(|_| "native_voice_internal")?;
        match client.as_ref() {
            Some(client) => Ok(client
                .snapshot
                .lock()
                .map_err(|_| "native_voice_internal")?
                .clone()
                .projected()),
            None => Ok(NativeVoiceSnapshot::default()),
        }
    }
    pub(crate) async fn execute(
        &self,
        connect: &ConnectManager,
        app_data: &Path,
        request: NativeVoiceRequest,
    ) -> Outcome {
        let client = {
            let mut current = self.client.lock().map_err(|_| "native_voice_internal")?;
            let previous_revision = current
                .as_ref()
                .and_then(|client| client.snapshot.lock().ok().map(|view| view.revision))
                .unwrap_or(0);
            if current.as_ref().is_some_and(|client| {
                client.cancel.is_cancelled()
                    && client
                        .worker
                        .lock()
                        .is_ok_and(|worker| worker.as_ref().is_none_or(JoinHandle::is_finished))
            }) {
                *current = None;
            }
            if current.is_none() {
                if !matches!(
                    request,
                    NativeVoiceRequest::Prepare { .. }
                        | NativeVoiceRequest::PrepareRuntime { .. }
                        | NativeVoiceRequest::InspectRecovery {}
                ) {
                    return Err("native_voice_no_session");
                }
                let (sender, receiver) = mpsc::channel(8);
                let snapshot = Arc::new(Mutex::new(NativeVoiceSnapshot {
                    revision: previous_revision.saturating_add(1),
                    ..NativeVoiceSnapshot::default()
                }));
                let busy = Arc::new(AtomicBool::new(false));
                let stop = Arc::new(AtomicBool::new(false));
                let cancel = CancellationToken::new();
                let completion = Arc::new(Mutex::new(None));
                let source = Arc::new(Mutex::new(None));
                let claimed = Arc::new(AtomicBool::new(false));
                let source_receipt = Arc::new(Mutex::new(None));
                let epoch = connect.social_capabilities().account_epoch;
                let actor = Actor::new(
                    connect.native_voice_shared_manager(),
                    app_data.join(resources::LAB_NAME),
                    snapshot.clone(),
                    stop.clone(),
                    completion.clone(),
                    source.clone(),
                    claimed.clone(),
                    source_receipt.clone(),
                );
                let worker = tokio::spawn(actor.run(receiver, busy.clone(), cancel.clone()));
                *current = Some(Arc::new(Client {
                    sender,
                    snapshot,
                    busy,
                    stop,
                    cancel,
                    worker: Mutex::new(Some(worker)),
                    completion,
                    source,
                    claimed,
                    source_receipt,
                    epoch,
                }));
            }
            current.as_ref().unwrap().clone()
        };
        if client.cancel.is_cancelled() {
            return Err("native_voice_closed");
        }
        let stopping = matches!(request, NativeVoiceRequest::Stop {});
        if stopping {
            client.stop.store(true, Ordering::Release);
            if let Ok(mut view) = client.snapshot.lock() {
                view.phase = "stopping";
                view.revision = view.revision.saturating_add(1);
            }
        }
        if !stopping
            && client
                .busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return Err("native_voice_busy");
        }
        let (send, receive) = oneshot::channel();
        if client.sender.send((request, send)).await.is_err() {
            client.busy.store(false, Ordering::Release);
            return Err("native_voice_closed");
        }
        receive.await.map_err(|_| "native_voice_internal")?
    }
    pub(crate) fn reserve_social(
        &self,
        connect: &ConnectManager,
        owner: &str,
        request: crate::engine::connect::SocialDiscoveredReserveRequest,
        host_player: bool,
    ) -> std::result::Result<crate::engine::connect::SocialReservation, &'static str> {
        let current = self.client.lock().map_err(|_| "native_voice_internal")?;
        let client = current.as_ref().ok_or("native_voice_no_session")?;
        client
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "native_voice_busy")?;
        let _busy = ReservationBusy(&client.busy);
        if client
            .source_receipt
            .lock()
            .map_err(|_| "native_voice_internal")?
            .as_ref()
            .is_some_and(|receipt| receipt.session_id() == request.session_id)
        {
            return Err("native_voice_new_advertisement_required");
        }
        if client.cancel.is_cancelled()
            || client.stop.load(Ordering::Acquire)
            || client.epoch != connect.social_capabilities().account_epoch
            || client
                .snapshot
                .lock()
                .map_err(|_| "native_voice_internal")?
                .phase
                != "ready"
        {
            return Err("native_voice_source_not_ready");
        }
        let binding = client
            .source
            .lock()
            .map_err(|_| "native_voice_internal")?
            .clone()
            .ok_or("native_voice_source_not_ready")?;
        binding
            .online_voice_policy()
            .map_err(|_| "native_voice_source_not_ready")?;
        if client.claimed.load(Ordering::Acquire) {
            return Err("native_voice_source_already_claimed");
        }
        match connect.social_reserve_native_voice_recorded(owner, request, binding, host_player) {
            Ok((reservation, receipt)) => {
                *client
                    .source_receipt
                    .lock()
                    .map_err(|_| "native_voice_internal")? = Some(receipt);
                client.claimed.store(true, Ordering::Release);
                Ok(reservation)
            }
            Err(error) => Err(error.code()),
        }
    }

    pub(crate) fn cancel(&self) {
        if let Ok(current) = self.client.lock() {
            if let Some(client) = current.as_ref() {
                client.stop.store(true, Ordering::Release);
                client.cancel.cancel();
            }
        }
    }
    pub(crate) async fn shutdown_all(&self) {
        self.cancel();
        let client = self.client.lock().ok().and_then(|value| value.clone());
        if let Some(client) = client {
            // Never remove a running JoinHandle merely because its waiter left.
            loop {
                let complete = client
                    .worker
                    .lock()
                    .is_ok_and(|worker| worker.as_ref().is_none_or(JoinHandle::is_finished));
                if complete {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            let worker = client.worker.lock().ok().and_then(|mut value| value.take());
            if let Some(worker) = worker {
                let _ = worker.await;
            }
            let completion = client
                .completion
                .lock()
                .ok()
                .and_then(|value| value.clone());
            if let Some(completion) = completion {
                let _ = completion.wait().await;
            }
        }
    }
}
struct Actor {
    shared_source: Arc<Mutex<Option<NativeVoiceHostBinding>>>,
    claimed: Arc<AtomicBool>,
    source_receipt: Arc<Mutex<Option<SocialNativeSourceReceipt>>>,
    source_revalidation: Option<SourceRevalidation>,
    completion: Arc<Mutex<Option<producer::DedicatedCompletion>>>,
    connect: ConnectManager,
    lab: PathBuf,
    shared: Arc<Mutex<NativeVoiceSnapshot>>,
    view: NativeVoiceSnapshot,
    stop: Arc<AtomicBool>,
    factory: Option<FactoryPlan>,
    server_preparation: Option<server_preparation::ServerPreparationPlan>,
    execution: Option<DedicatedPlan>,
    source: Option<DedicatedRun>,
    recovery: Option<(Instance, DurableRestorationController)>,
    eula_recovery: Option<EulaRecoveryPlan>,
    bundle_id: Option<BundleId>,
    host_generation: Option<u64>,
    host_bind: Option<std::net::UdpSocket>,
    local_key: Option<lab::SecretKey>,
    local_guest: Option<lab::Guest>,
    local_tcp: Option<lab::TcpBridge>,
    local_udp: Option<lab::UdpBridge>,
}
impl Actor {
    #[allow(clippy::too_many_arguments)] // Explicit shared owners; no reconstructed source or receipt.
    fn new(
        connect: ConnectManager,
        lab: PathBuf,
        shared: Arc<Mutex<NativeVoiceSnapshot>>,
        stop: Arc<AtomicBool>,
        completion: Arc<Mutex<Option<producer::DedicatedCompletion>>>,
        shared_source: Arc<Mutex<Option<NativeVoiceHostBinding>>>,
        claimed: Arc<AtomicBool>,
        source_receipt: Arc<Mutex<Option<SocialNativeSourceReceipt>>>,
    ) -> Self {
        let view = shared.lock().unwrap().clone();
        Self {
            shared_source,
            claimed,
            source_receipt,
            source_revalidation: None,
            completion,
            connect,
            lab,
            shared,
            view,
            stop,
            factory: None,
            server_preparation: None,
            execution: None,
            source: None,
            recovery: None,
            eula_recovery: None,
            bundle_id: None,
            host_generation: None,
            host_bind: None,
            local_key: None,
            local_guest: None,
            local_tcp: None,
            local_udp: None,
        }
    }
    fn publish(&mut self) {
        if let Ok(mut target) = self.shared.lock() {
            self.view.revision = self.view.revision.max(target.revision).saturating_add(1);
            *target = self.view.clone().projected();
        }
    }
    fn review(&mut self, kind: &'static str, review: &Review, deadline: Instant) {
        let runtime = self
            .view
            .runtime_profile
            .unwrap_or(NativeVoiceRuntime::Fabric1211)
            .ready()
            .descriptor();
        let mut details = vec![
            format!("Minecraft {} / {} {} / Java {} em cópia dedicada nova. Prazo original de até4h.", runtime.minecraft_version, runtime.loader, runtime.loader_version, runtime.java_major),
            "Servidor privado loopback43001; login Minecraft offline; whitelist desligada; máximo8jogadores; transporteAurora autenticado. EULA e execução têm revisão separada.".into(),
            "Anúncio127.0.0.1:35554 comRecoverydurável. Mod/porta originais verificados; áudio ainda requer teste.".into(),
        ];
        details.extend(review.details.iter().cloned());
        self.view.review = Some(NativeVoiceApproval {
            kind,
            digest: review.digest.clone(),
            before: review.before.clone(),
            after: review.after.clone(),
            details,
            remaining_ms: 0,
        });
        self.view.native_review_deadline = Some(deadline);
    }
    async fn run(
        mut self,
        mut receiver: mpsc::Receiver<Message>,
        busy: Arc<AtomicBool>,
        cancel: CancellationToken,
    ) {
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tick.tick() => { self.maintain().await; self.publish(); },
                message = receiver.recv() => {
                    let Some((request, result)) = message else { break; };
                    let stopping = matches!(request, NativeVoiceRequest::Stop {});
                    let outcome = match catch_future(self.handle(request, &cancel)).await {
                        Ok(result) => result,
                        Err(()) => { self.stop.store(true, Ordering::Release); let _ = self.stop_all().await; Err("native_voice_worker_failed") }
                    };
                    let outcome = match outcome {
                        Err(code) => {
                            self.settle_failure().await;
                            Err(*self.view.error_code.get_or_insert(code))
                        }
                        Ok(value) => Ok(value),
                    };
                    self.publish();
                    let response = outcome.map(|invitation| { let mut snapshot = self.view.clone().projected(); snapshot.exported_invitation = invitation; snapshot });
                    let _ = result.send(response);
                    if !stopping { busy.store(false, Ordering::Release); }
                }
            }
        }
        self.stop.store(true, Ordering::Release);
        if let Err(code) = self.stop_all().await {
            self.view.error_code = Some(code);
        }
        self.publish();
    }
    async fn maintain(&mut self) {
        self.view.social_revalidation_available = self.source.is_some()
            && self.view.phase == "socialSource"
            && !self.stop.load(Ordering::Acquire)
            && self
                .source_receipt
                .lock()
                .ok()
                .and_then(|receipt| receipt.clone())
                .is_some_and(|receipt| self.connect.can_reclaim_native_source(&receipt));
        if self.source.is_some()
            && self.claimed.load(Ordering::Acquire)
            && self.view.operation_id.is_none()
            && self.view.phase == "ready"
        {
            self.view.phase = "socialSource";
        }
        if self.stop.load(Ordering::Acquire)
            && (self.source.is_some()
                || self.factory.is_some()
                || self.server_preparation.is_some()
                || self.execution.is_some())
        {
            if let Err(code) = self.stop_all().await {
                self.view.error_code = Some(code);
            }
            return;
        }
        if self
            .view
            .native_review_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            if self.source_revalidation.take().is_some() {
                self.view.phase = "socialSource";
            }
            self.server_preparation.take();
            self.factory.take();
            self.execution.take();
            self.eula_recovery.take();
            if let Some((_, receipt)) = &mut self.recovery {
                receipt.cancel_review();
            }
            self.view.review = None;
            self.view.native_review_deadline = None;
            if self.source.is_none() {
                self.view.phase = "reviewExpired";
            } else if self.view.operation_id.is_some() {
                let _ = self.close_transport().await;
                self.view.phase = "transportClosed";
            }
        }
        if self
            .source
            .as_ref()
            .and_then(|run| run.observer().snapshot())
            .is_some_and(|value| value.phase == DedicatedPhase::Complete)
        {
            if let Err(code) = self.stop_all().await {
                self.view.error_code = Some(code);
            }
            return;
        }
        if self.view.phase == "bound" {
            if let Some(id) = &self.view.operation_id {
                match self.connect.native_voice_host_observation("main", id) {
                    Ok(observed) if observed.bound => {
                        if observed.udp_retired() || observed.udp_retiring() {
                            if let Some(bridge) = self.local_udp.take() {
                                bridge.shutdown().await;
                            }
                            self.view.udp_state = if observed.udp_retired() {
                                "retired"
                            } else {
                                "retiring"
                            };
                            self.view.negotiated_datagram_bytes = None;
                        }
                    }
                    _ => {
                        let _ = self.close_transport().await;
                        self.view.phase = "transportClosed";
                    }
                }
            }
            if self
                .local_udp
                .as_ref()
                .is_some_and(|bridge| bridge.max_payload().is_none_or(|capacity| capacity < 4096))
            {
                if let Some(bridge) = self.local_udp.take() {
                    bridge.shutdown().await;
                }
                self.view.udp_state = "localUnavailable";
                self.view.negotiated_datagram_bytes = None;
                self.view.error_code = Some("native_voice_fragment_capacity_insufficient");
            }
            if self
                .local_tcp
                .as_ref()
                .is_some_and(lab::TcpBridge::is_closed)
                || self
                    .local_udp
                    .as_ref()
                    .is_some_and(lab::UdpBridge::is_closed)
            {
                let _ = self.close_transport().await;
                self.view.error_code = Some("native_voice_local_bridge_closed");
                self.view.phase = "transportClosed";
            }
        }
    }
    fn approval(&self, kind: &str, digest: &str) -> std::result::Result<(), &'static str> {
        if self
            .view
            .review
            .as_ref()
            .is_none_or(|review| review.kind != kind || review.digest != digest)
            || self
                .view
                .native_review_deadline
                .is_none_or(|deadline| Instant::now() >= deadline)
        {
            return Err("native_voice_stale_review");
        }
        Ok(())
    }
    async fn settle_failure(&mut self) {
        if matches!(
            self.view.phase,
            "socialSourceReviewing" | "socialSourceReviewed" | "socialSourceApplying"
        ) {
            self.clear_source_revalidation();
        }
        if matches!(
            self.view.phase,
            "preparing" | "preparingServer" | "creating" | "starting" | "restoringEula"
        ) {
            // handle() awaits the original blocking task before reaching here.
            // A failure never frees a live installer/source or drops receipts.
            if self.source.is_some() {
                let _ = self.stop_all().await;
            }
            self.server_preparation.take();
            self.factory.take();
            self.execution.take();
            self.eula_recovery.take();
            self.view.review = None;
            self.view.native_review_deadline = None;
            self.view.phase = if self.view.eula_state == "needsRecovery" {
                "needsRecovery"
            } else {
                "failed"
            };
        }
        self.view.eula_recovery_available = self.bundle_id.is_some()
            && self.source.is_none()
            && self.view.eula_state == "needsRecovery";
    }
    async fn handle(
        &mut self,
        request: NativeVoiceRequest,
        cancel: &CancellationToken,
    ) -> std::result::Result<Option<String>, &'static str> {
        // An explicit Stop can arrive after maintain() already collected the
        // same source. Keep its terminal warning visible on repeated Stop.
        if !matches!(request, NativeVoiceRequest::Stop {}) {
            self.view.error_code = None;
        }
        match request {
            NativeVoiceRequest::ReviewSocialSource {} => self.review_social_source(cancel).await?,
            NativeVoiceRequest::ApproveSocialSource { digest } => {
                self.approve_social_source(&digest, cancel).await?
            }
            NativeVoiceRequest::CancelSocialSourceReview {} => {
                if self.view.phase != "socialSourceReviewed" {
                    return Err("native_voice_stale_review");
                }
                self.clear_source_revalidation();
            }
            NativeVoiceRequest::PrepareRuntime {
                profile,
                runtime_profile,
            } => {
                if self.source.is_some()
                    || self.eula_recovery.is_some()
                    || self.view.eula_state == "needsRecovery"
                    || self.view.recovery_pending
                    || self.factory.is_some()
                    || self.server_preparation.is_some()
                    || self.execution.is_some()
                {
                    return Err("native_voice_busy");
                }
                if voice_catalog::artifacts(runtime_profile.ready(), resource_profile(profile))
                    .is_none()
                {
                    return Err("native_voice_runtime_unsupported");
                }
                self.stop.store(false, Ordering::Release);
                self.bundle_id = None;
                self.claimed.store(false, Ordering::Release);
                self.view = NativeVoiceSnapshot {
                    revision: self.view.revision,
                    profile: Some(profile),
                    runtime_profile: Some(runtime_profile),
                    phase: "preparing",
                    ..NativeVoiceSnapshot::default()
                };
                self.publish();
                let lab = self.lab.clone();
                let stop = self.stop.clone();
                let cancel = cancel.clone();
                let plan = tokio::task::spawn_blocking(move || {
                    server_preparation::ServerPreparationPlan::prepare(
                        &lab,
                        runtime_profile.ready(),
                        resource_profile(profile),
                        &|| stop.load(Ordering::Acquire) || cancel.is_cancelled(),
                    )
                })
                .await
                .map_err(|_| "native_voice_worker_failed")?
                .map_err(file_error)?;
                if self.stop.load(Ordering::Acquire) {
                    self.view.phase = "stopped";
                    return Err("native_voice_cancelled");
                }
                self.review("preparation", plan.review(), plan.approval_deadline());
                self.server_preparation = Some(plan);
                self.view.phase = "preparationReviewed";
            }
            NativeVoiceRequest::ApprovePreparation { digest } => {
                self.approval("preparation", &digest)?;
                let plan = self
                    .server_preparation
                    .take()
                    .ok_or("native_voice_stale_review")?;
                self.view.review = None;
                self.view.phase = "preparingServer";
                self.publish();
                let lab = self.lab.clone();
                let stop = self.stop.clone();
                let cancel = cancel.clone();
                let plan = tokio::task::spawn_blocking(move || {
                    let cancelled = || stop.load(Ordering::Acquire) || cancel.is_cancelled();
                    let proof = plan.execute(&digest, &cancelled)?;
                    if cancelled() {
                        return Err(Error::Cancelled);
                    }
                    let plan = factory::plan_voice_server_factory_at(&lab, proof)?;
                    if cancelled() {
                        return Err(Error::Cancelled);
                    }
                    Ok(plan)
                })
                .await
                .map_err(|_| "native_voice_worker_failed")?
                .map_err(file_error)?;
                if self.stop.load(Ordering::Acquire) {
                    self.view.phase = "stopped";
                    return Err("native_voice_cancelled");
                }
                self.review("files", plan.review(), plan.approval_deadline());
                self.factory = Some(plan);
                self.view.phase = "filesReviewed";
            }
            NativeVoiceRequest::Prepare { profile } => {
                if self.source.is_some()
                    || self.eula_recovery.is_some()
                    || self.view.eula_state == "needsRecovery"
                    || self.view.recovery_pending
                    || self.factory.is_some()
                    || self.server_preparation.is_some()
                    || self.execution.is_some()
                {
                    return Err("native_voice_busy");
                }
                self.stop.store(false, Ordering::Release);
                self.bundle_id = None;
                self.claimed.store(false, Ordering::Release);
                self.view = NativeVoiceSnapshot {
                    revision: self.view.revision,
                    profile: Some(profile),
                    runtime_profile: Some(NativeVoiceRuntime::Fabric1211),
                    phase: "preparing",
                    ..NativeVoiceSnapshot::default()
                };
                self.publish();
                let lab = self.lab.clone();
                let stop = self.stop.clone();
                let cancellation = cancel.clone();
                let result = tokio::task::spawn_blocking(move || {
                    resources::prepare_managed_plan(&lab, resource_profile(profile), &|| {
                        stop.load(Ordering::Acquire) || cancellation.is_cancelled()
                    })
                })
                .await;
                let plan = match result {
                    Ok(Ok(plan))
                        if !self.stop.load(Ordering::Acquire) && !cancel.is_cancelled() =>
                    {
                        plan
                    }
                    result => {
                        self.view.phase = "idle";
                        return Err(match result {
                            Ok(Ok(_) | Err(Error::Cancelled)) => "native_voice_cancelled",
                            Ok(Err(Error::Resources)) => {
                                "native_voice_pinned_resources_unavailable"
                            }
                            Ok(Err(error)) => file_error(error),
                            Err(_) => "native_voice_worker_failed",
                        });
                    }
                };
                self.review("files", plan.review(), plan.approval_deadline());
                self.factory = Some(plan);
                self.view.phase = "filesReviewed";
            }
            NativeVoiceRequest::InspectRecovery {} => {
                if self.source.is_some()
                    || self.eula_recovery.is_some()
                    || self.execution.is_some()
                    || self.factory.is_some()
                    || self.server_preparation.is_some()
                    || self.view.recovery_pending
                {
                    return Err("native_voice_busy");
                }
                let lab = self.lab.clone();
                self.view.recoverable_copies =
                    tokio::task::spawn_blocking(move || inspect_recovery(&lab))
                        .await
                        .map_err(|_| "native_voice_worker_failed")??;
            }
            NativeVoiceRequest::ResumeRecovery {
                bundle_id,
                receipt_id,
            } => {
                if self.source.is_some()
                    || self.eula_recovery.is_some()
                    || self.view.eula_state == "needsRecovery"
                    || self.execution.is_some()
                    || self.factory.is_some()
                    || self.server_preparation.is_some()
                    || self.view.recovery_pending
                {
                    return Err("native_voice_busy");
                }
                let candidate = self
                    .view
                    .recoverable_copies
                    .iter()
                    .find(|candidate| {
                        candidate.bundle_id == bundle_id && candidate.receipt_id == receipt_id
                    })
                    .cloned()
                    .ok_or("native_voice_recovery_selection_stale")?;
                let lab = self.lab.clone();
                let selected_id = Uuid::parse_str(&bundle_id)
                    .map_err(|_| "native_voice_recovery_selection_stale")?;
                let recovery = tokio::task::spawn_blocking(move || {
                    resume_recovery(&lab, &bundle_id, &receipt_id)
                })
                .await
                .map_err(|_| "native_voice_worker_failed")??;
                self.view.recovery_pending = recovery
                    .announcement
                    .as_ref()
                    .is_some_and(|(_, receipt)| receipt.state() != RestorationState::Restored);
                self.recovery = recovery.announcement;
                self.bundle_id = Some(BundleId(selected_id));
                self.view.profile = Some(candidate.profile);
                self.view.runtime_profile = Some(recovery.runtime);
                self.view.eula_state = if recovery.eula_pending {
                    "needsRecovery"
                } else {
                    "unchanged"
                };
                self.view.eula_recovery_available = recovery.eula_pending;
                self.view.phase = if recovery.eula_pending {
                    "needsRecovery"
                } else {
                    "stopped"
                };
                self.view.review = None;
                self.view.native_review_deadline = None;
            }
            NativeVoiceRequest::ApproveFiles { digest } => {
                self.approval("files", &digest)?;
                if self.stop.load(Ordering::Acquire) {
                    return Err("native_voice_cancelled");
                }
                let plan = self.factory.take().ok_or("native_voice_stale_review")?;
                self.bundle_id = Some(plan.review().bundle_id);
                self.view.review = None;
                self.view.phase = "creating";
                self.publish();
                let mut prepared = tokio::task::spawn_blocking(move || plan.create(&digest))
                    .await
                    .map_err(|_| "native_voice_worker_failed")?
                    .map_err(file_error)?;
                self.recovery = prepared.announcement_recovery.take();
                self.view.recovery_pending = self.recovery.is_some();
                if self.stop.load(Ordering::Acquire) {
                    drop(prepared);
                    self.view.phase = "stopped";
                    return Err("native_voice_cancelled");
                }
                let plan = tokio::task::spawn_blocking(move || plan_voice_execution(prepared))
                    .await
                    .map_err(|_| "native_voice_worker_failed")?
                    .map_err(run_error)?;
                self.review("execution", plan.review(), plan.approval_deadline());
                self.execution = Some(plan);
                self.view.phase = "executionReviewed";
            }
            NativeVoiceRequest::ApproveExecution { digest } => {
                self.approval("execution", &digest)?;
                if self.stop.load(Ordering::Acquire) {
                    return Err("native_voice_cancelled");
                }
                let plan = self.execution.take().ok_or("native_voice_stale_review")?;
                self.view.review = None;
                self.view.native_review_deadline = None;
                self.view.phase = "starting";
                self.publish();
                let run = tokio::task::spawn_blocking(move || {
                    plan.start(&digest, CancellationToken::new())
                })
                .await
                .map_err(|_| "native_voice_worker_failed")?
                .map_err(run_error)?;
                *self
                    .completion
                    .lock()
                    .map_err(|_| "native_voice_internal")? = Some(run.completion());
                self.source = Some(run);
                self.view.phase = "starting";
                self.view.eula_state = "ownedBySource";
                self.publish();
                loop {
                    if self.stop.load(Ordering::Acquire) {
                        self.stop_all().await?;
                        return Err("native_voice_cancelled");
                    }
                    match self
                        .source
                        .as_ref()
                        .unwrap()
                        .observer()
                        .snapshot()
                        .map(|value| value.phase)
                    {
                        Some(DedicatedPhase::Ready) => break,
                        Some(DedicatedPhase::Complete) | None => {
                            self.stop_all().await?;
                            return Err("native_voice_source_failed");
                        }
                        _ => tokio::time::sleep(Duration::from_millis(100)).await,
                    }
                }
                *self
                    .shared_source
                    .lock()
                    .map_err(|_| "native_voice_internal")? = Some(
                    self.source
                        .as_ref()
                        .unwrap()
                        .native_voice_host_binding()
                        .map_err(run_error)?,
                );
                self.view.phase = "ready";
            }
            NativeVoiceRequest::ReviewHost {
                guests,
                local_player,
            } => {
                if self.view.operation_id.is_some() || self.stop.load(Ordering::Acquire) {
                    return Err("native_voice_busy");
                }
                if guests.len() + usize::from(local_player) > 8
                    || guests.len() + usize::from(local_player) < 2
                {
                    return Err("native_voice_roster_invalid");
                }
                self.claimed
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .map_err(|_| "native_voice_source_already_claimed")?;
                let binding = self
                    .source
                    .as_ref()
                    .ok_or("native_voice_source_not_ready")?
                    .native_voice_host_binding()
                    .map_err(run_error)?;
                let held = std::net::UdpSocket::bind("127.0.0.1:0")
                    .map_err(|_| "native_voice_bind_failed")?;
                let bind = held.local_addr().map_err(|_| "native_voice_bind_failed")?;
                let mut roster = guests
                    .into_iter()
                    .map(|guest_public_id| GroupParticipantRequest {
                        guest_public_id,
                        admission_seconds: 60,
                        session_seconds: 300,
                    })
                    .collect::<Vec<_>>();
                self.local_key = local_player.then(lab::generate_key);
                if let Some(key) = &self.local_key {
                    roster.push(GroupParticipantRequest {
                        guest_public_id: key.public().to_string(),
                        admission_seconds: 60,
                        session_seconds: 300,
                    });
                }
                let pending = self
                    .connect
                    .prepare_native_voice_group("main", binding, bind, roster)
                    .map_err(|error| error.code())?;
                self.view.operation_id = Some(pending.operation_id.clone());
                self.view.phase = "reviewingHost";
                self.publish();
                let prepared = pending.wait().await.map_err(|error| error.code())?;
                self.connect
                    .wait_native_voice_source_work("main", &prepared.operation_id)
                    .await
                    .map_err(|error| error.code())?;
                let review = self
                    .connect
                    .review_native_voice_group("main", &prepared.operation_id, prepared.generation)
                    .map_err(|error| error.code())?
                    .wait()
                    .await
                    .map_err(|error| error.code())?;
                self.connect
                    .wait_native_voice_source_work("main", &prepared.operation_id)
                    .await
                    .map_err(|error| error.code())?;
                if self.stop.load(Ordering::Acquire) {
                    self.stop_all().await?;
                    return Err("native_voice_cancelled");
                }
                self.host_generation = Some(review.data.generation);
                self.host_bind = Some(held);
                self.view.host_public_id = Some(prepared.host_public_id);
                let local_id = self.local_key.as_ref().map(|key| key.public().to_string());
                self.view.participants = review
                    .data
                    .participants
                    .iter()
                    .map(|participant| NativeVoiceParticipant {
                        participant_id: participant.participant_id.clone(),
                        public_id: participant.guest_public_id.clone(),
                        local_player: local_id.as_ref() == Some(&participant.guest_public_id),
                    })
                    .collect();
                self.view.review = Some(NativeVoiceApproval { kind: "host", digest: review.digest, before: "Origem dedicada própria; nenhum Host Connect".into(), after: format!("Host de {} participantes explícitos; TCP game + UDP aux; anúncio {}", self.view.participants.len(), review.data.announcement_policy), remaining_ms: 0, details: vec![format!("Origem nativa {} / {}; destinos fixados pelo mesmo processo; nenhuma porta recebida da interface.", review.profile, review.artifact_version), "Cada falha de observação retira UDP terminalmente; TCP conserva seus gates próprios. O Host jogador ocupa uma vaga e 127.0.0.1:35554.".into(), "Transporte local experimental; áudio/auth do mod precisam de validação manual.".into()] });
                self.view.native_review_deadline = Some(review.deadline);
                self.view.phase = "hostReviewed";
            }
            NativeVoiceRequest::ApproveHost { digest } => {
                self.approval("host", &digest)?;
                if self.stop.load(Ordering::Acquire) {
                    return Err("native_voice_cancelled");
                }
                let id = self
                    .view
                    .operation_id
                    .clone()
                    .ok_or("native_voice_stale_review")?;
                let generation = self.host_generation.ok_or("native_voice_stale_review")?;
                self.host_bind.take();
                self.view.review = None;
                self.view.native_review_deadline = None;
                self.view.phase = "binding";
                self.publish();
                self.connect
                    .confirm_native_voice_group("main", &id, generation, &digest)
                    .map_err(|error| error.code())?
                    .wait()
                    .await
                    .map_err(|error| error.code())?;
                if self.stop.load(Ordering::Acquire) {
                    self.stop_all().await?;
                    return Err("native_voice_cancelled");
                }
                if let Err(code) = self.open_local_player().await {
                    let _ = self.close_transport().await;
                    return Err(code);
                }
                self.view.phase = "bound";
                self.view.udp_state = "enabled";
            }
            NativeVoiceRequest::ExportInvitation { participant_id } => {
                if self.view.phase != "bound"
                    || self
                        .view
                        .participants
                        .iter()
                        .all(|p| p.participant_id != participant_id || p.local_player)
                {
                    return Err("native_voice_participant_invalid");
                }
                return self
                    .connect
                    .export_native_voice_invitation(
                        "main",
                        self.view
                            .operation_id
                            .as_deref()
                            .ok_or("native_voice_no_session")?,
                        &participant_id,
                    )
                    .map(Some)
                    .map_err(|error| error.code());
            }
            NativeVoiceRequest::Stop {} => {
                self.stop.store(true, Ordering::Release);
                self.stop_all().await?;
            }
            NativeVoiceRequest::PrepareRestore { reconciled } => {
                if self.source.is_some()
                    || self.eula_recovery.is_some()
                    || self.execution.is_some()
                    || self.factory.is_some()
                    || self.server_preparation.is_some()
                {
                    return Err("native_voice_source_running");
                }
                let (instance, mut receipt) =
                    self.recovery.take().ok_or("native_voice_no_receipt")?;
                let (instance, receipt, review) = tokio::task::spawn_blocking(move || {
                    let result = (|| {
                        let guard = super::super::acquire_voice_mutation_guard(&instance)?;
                        if reconciled {
                            let inspection = super::super::inspect_announcement(
                                &instance,
                                &guard,
                                receipt.voice_mod(),
                            )?;
                            receipt.prepare_reconciled(&instance, &guard, &inspection.source_sha256)
                        } else {
                            receipt.prepare_strict(&instance, &guard)
                        }
                    })();
                    (instance, receipt, result)
                })
                .await
                .map_err(|_| "native_voice_worker_failed")?;
                self.recovery = Some((instance, receipt));
                let review = review.map_err(|_| "native_voice_restore_review_failed")?;
                self.view.review = Some(NativeVoiceApproval {
                    kind: "restore",
                    digest: review.digest,
                    before: review.before,
                    after: review.after,
                    details: vec![review.warning],
                    remaining_ms: 0,
                });
                self.view.native_review_deadline = self
                    .recovery
                    .as_ref()
                    .and_then(|(_, receipt)| receipt.review_deadline());
                self.view.phase = "restoreReviewed";
            }
            NativeVoiceRequest::ApproveRestore { digest } => {
                self.approval("restore", &digest)?;
                let (instance, mut receipt) =
                    self.recovery.take().ok_or("native_voice_no_receipt")?;
                let (instance, receipt, result) = tokio::task::spawn_blocking(move || {
                    let result = super::super::acquire_voice_mutation_guard(&instance)
                        .and_then(|guard| receipt.confirm(&instance, &guard, &digest));
                    (instance, receipt, result)
                })
                .await
                .map_err(|_| "native_voice_worker_failed")?;
                self.recovery = Some((instance, receipt));
                result.map_err(|_| "native_voice_restore_failed")?;
                self.view.recovery_pending = false;
                self.view.review = None;
                self.view.native_review_deadline = None;
                self.view.phase = if self.view.eula_state == "needsRecovery" {
                    "needsRecovery"
                } else {
                    "restored"
                };
            }
            NativeVoiceRequest::PrepareEulaRestore {} => {
                if self.source.is_some()
                    || self.execution.is_some()
                    || self.factory.is_some()
                    || self.server_preparation.is_some()
                    || self.eula_recovery.is_some()
                {
                    return Err("native_voice_source_running");
                }
                let id = self.bundle_id.ok_or("native_voice_no_receipt")?;
                if let Some((_, receipt)) = &mut self.recovery {
                    receipt.cancel_review();
                }
                self.view.review = None;
                self.view.native_review_deadline = None;
                let lab = self.lab.clone();
                let plan = tokio::task::spawn_blocking(move || {
                    #[cfg(test)]
                    {
                        eula::prepare_recovery(&lab, id, None, Hooks::default())
                    }
                    #[cfg(not(test))]
                    {
                        eula::prepare_recovery(&lab, id, Hooks)
                    }
                })
                .await
                .map_err(|_| "native_voice_worker_failed")?
                .map_err(|_| "native_voice_eula_restore_review_failed")?;
                self.view.eula_recovery_available = true;
                self.review("eulaRestore", plan.review(), plan.approval_deadline());
                self.eula_recovery = Some(plan);
                self.view.phase = "eulaRestoreReviewed";
            }
            NativeVoiceRequest::ApproveEulaRestore { digest } => {
                self.approval("eulaRestore", &digest)?;
                let plan = self
                    .eula_recovery
                    .take()
                    .ok_or("native_voice_stale_review")?;
                self.view.review = None;
                self.view.native_review_deadline = None;
                self.view.phase = "restoringEula";
                self.publish();
                let outcome = tokio::task::spawn_blocking(move || plan.confirm(&digest))
                    .await
                    .map_err(|_| "native_voice_worker_failed")?
                    .map_err(|_| "native_voice_eula_restore_failed")?;
                self.view.eula_state = match outcome {
                    eula::EulaOutcome::Restored => "restored",
                    eula::EulaOutcome::Unchanged => "unchanged",
                };
                self.view.eula_recovery_available = false;
                self.view.phase = if self.view.recovery_pending {
                    "stopped"
                } else {
                    "restored"
                };
            }
        }
        Ok(None)
    }
    async fn open_local_player(&mut self) -> std::result::Result<(), &'static str> {
        let Some(key) = self.local_key.as_ref() else {
            return Ok(());
        };
        let participant = self
            .view
            .participants
            .iter()
            .find(|value| value.local_player)
            .ok_or("native_voice_participant_invalid")?;
        let id = self
            .view
            .operation_id
            .as_deref()
            .ok_or("native_voice_no_session")?;
        let encoded = self
            .connect
            .export_native_voice_invitation("main", id, &participant.participant_id)
            .map_err(|error| error.code())?;
        let host = self
            .view
            .host_public_id
            .as_ref()
            .ok_or("native_voice_no_session")?
            .parse()
            .map_err(|_| "native_voice_internal")?;
        let invitation =
            lab::SessionInvitation::verify_fragmented_loopback(&encoded, host, key.public())
                .map_err(|_| "native_voice_invitation_rejected")?;
        let guest = lab::Guest::connect_invited_fragmented_session(
            key.clone(),
            &invitation,
            lab::Limits::default(),
        )
        .await
        .map_err(|_| "native_voice_local_admission_failed")?;
        self.local_guest = Some(guest);
        let guest = self.local_guest.as_ref().unwrap();
        let tcp = guest
            .tcp_bridge("game", 1, "127.0.0.1:0".parse().unwrap())
            .await
            .map_err(|_| "native_voice_local_tcp_failed")?;
        self.view.tcp_address = Some(tcp.local_addr().to_string());
        self.local_tcp = Some(tcp);
        let udp = guest
            .udp_bridge_fragmented("aux", 1, "127.0.0.1:35554".parse().unwrap())
            .await
            .map_err(|_| "native_voice_local_udp_in_use")?;
        let capacity = udp.max_payload();
        self.local_udp = Some(udp);
        if capacity.is_none_or(|value| value < 4096) {
            return Err("native_voice_fragment_capacity_insufficient");
        }
        self.view.negotiated_datagram_bytes = capacity;
        Ok(())
    }
    async fn close_transport(&mut self) -> std::result::Result<(), &'static str> {
        self.host_bind.take();
        if let Some(bridge) = self.local_udp.take() {
            bridge.shutdown().await;
        }
        if let Some(bridge) = self.local_tcp.take() {
            bridge.shutdown().await;
        }
        if let Some(guest) = self.local_guest.take() {
            guest.shutdown().await;
        }
        self.view.tcp_address = None;
        self.view.negotiated_datagram_bytes = None;
        if let Some(id) = &self.view.operation_id {
            if !self
                .connect
                .stop("main", id)
                .await
                .is_ok_and(|snapshot| snapshot.cleanup_state == "complete")
            {
                self.view.udp_state = "cleanupFailed";
                return Err(TRANSPORT_CLEANUP_FAILED);
            }
        }
        self.view.udp_state = "closed";
        Ok(())
    }
    async fn stop_all(&mut self) -> std::result::Result<(), &'static str> {
        self.view.phase = "stopping";
        self.view.review = None;
        self.view.native_review_deadline = None;
        self.publish();
        self.server_preparation.take();
        self.factory.take();
        self.execution.take();
        self.eula_recovery.take();
        self.source_revalidation.take();
        self.view.social_revalidation_available = false;
        if let Some((_, receipt)) = &mut self.recovery {
            receipt.cancel_review();
        }
        if let Ok(mut source) = self.shared_source.lock() {
            *source = None;
        }
        let transport = self.close_transport().await;
        if let Some(run) = self.source.as_ref() {
            let report = run.request_stop().wait().await;
            self.view.eula_state = match report.eula {
                EulaDisposition::Restored => "restored",
                EulaDisposition::Unchanged => "unchanged",
                EulaDisposition::NeedsRecovery => "needsRecovery",
            };
            self.view.error_code = source_completion_error(&report);
            self.source.take();
        }
        let transport = match transport {
            Ok(()) => {
                self.view.error_code = match self.view.error_code {
                    Some(FORCED_STOP_CLEANUP_FAILED) => Some(FORCED_STOP),
                    Some(TRANSPORT_CLEANUP_FAILED) => None,
                    previous => previous,
                };
                Ok(())
            }
            Err(code) => {
                let code = if matches!(
                    self.view.error_code,
                    Some(FORCED_STOP | FORCED_STOP_CLEANUP_FAILED)
                ) {
                    FORCED_STOP_CLEANUP_FAILED
                } else {
                    code
                };
                self.view.error_code = Some(code);
                Err(code)
            }
        };
        self.view.recovery_pending = self
            .recovery
            .as_ref()
            .is_some_and(|(_, receipt)| receipt.state() != RestorationState::Restored);
        self.view.phase = if self.view.eula_state == "needsRecovery" {
            "needsRecovery"
        } else if transport.is_err() {
            "failed"
        } else {
            "stopped"
        };
        self.view.eula_recovery_available =
            self.bundle_id.is_some() && self.view.eula_state == "needsRecovery";
        transport
    }
}
fn source_completion_error(report: &DedicatedReport) -> Option<&'static str> {
    if report
        .process
        .as_ref()
        .is_some_and(|process| process.forced)
        || report.failure == Some(DedicatedFailure::Child(OwnedRunFailure::Forced))
    {
        return Some(FORCED_STOP);
    }
    match report.failure {
        Some(DedicatedFailure::Deadline) => Some("native_voice_source_expired"),
        Some(DedicatedFailure::Cancelled) => Some("native_voice_source_cancelled"),
        Some(DedicatedFailure::Preparation(_)) if report.eula == EulaDisposition::NeedsRecovery => {
            Some("native_voice_recovery_required")
        }
        Some(_) => Some("native_voice_source_failed"),
        None => None,
    }
}
fn resource_profile(
    profile: crate::engine::connect_native_voice::NativeVoiceProfile,
) -> super::NativeVoiceProfile {
    match profile {
        crate::engine::connect_native_voice::NativeVoiceProfile::SimpleVoiceChat => {
            super::NativeVoiceProfile::SimpleVoiceChat
        }
        crate::engine::connect_native_voice::NativeVoiceProfile::PlasmoVoice => {
            super::NativeVoiceProfile::PlasmoVoice
        }
    }
}

#[cfg(test)]
pub(super) fn discover_plan(
    profile: crate::engine::connect_native_voice::NativeVoiceProfile,
) -> std::result::Result<FactoryPlan, &'static str> {
    let profile = resource_profile(profile);
    let lab = lab_root().map_err(file_error)?;
    let mut stages = Vec::new();
    for entry in fs::read_dir(&lab)
        .map_err(|_| "native_voice_resources_unavailable")?
        .take(256)
    {
        let entry = entry.map_err(|_| "native_voice_resources_unavailable")?;
        let name = entry.file_name();
        if name
            .to_str()
            .is_some_and(|name| name.starts_with("prepared-"))
        {
            stages.push(entry.path());
        }
    }
    stages.sort();
    let jdks = crate::engine::jre::discover_java_executables();
    for stage in stages {
        let mod_name = match profile {
            super::NativeVoiceProfile::SimpleVoiceChat => "mods/voicechat-fabric-1.21.1-2.6.22.jar",
            super::NativeVoiceProfile::PlasmoVoice => "mods/plasmovoice-fabric-1.21.1-2.1.17.jar",
        };
        if !stage.join(mod_name).is_file() {
            continue;
        }
        for java in jdks.iter().take(32) {
            let Some(jdk) = java.parent().and_then(Path::parent) else {
                continue;
            };
            if let Ok(plan) = factory::plan_voice_bridge_factory_at(&lab, &stage, jdk, profile) {
                return Ok(plan);
            }
        }
    }
    Err("native_voice_pinned_resources_unavailable")
}
fn file_error(error: Error) -> &'static str {
    match error {
        Error::JavaRunning => "native_voice_java_running",
        Error::ProbeUnavailable => "native_voice_process_probe_unavailable",
        Error::NeedsRecovery => "native_voice_recovery_required",
        Error::Expired => "native_voice_review_expired",
        Error::Digest => "native_voice_wrong_digest",
        Error::Busy => "native_voice_busy",
        _ => "native_voice_preparation_failed",
    }
}
fn run_error(error: DedicatedError) -> &'static str {
    match error {
        DedicatedError::Preparation(error) => file_error(error),
        DedicatedError::NotReady => "native_voice_source_not_ready",
        DedicatedError::Busy => "native_voice_busy",
        DedicatedError::Cancelled | DedicatedError::Closed => "native_voice_source_closed",
        _ => "native_voice_source_failed",
    }
}

async fn catch_future<T>(
    future: impl std::future::Future<Output = T>,
) -> std::result::Result<T, ()> {
    let mut future = Box::pin(future);
    std::future::poll_fn(|context| {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            future.as_mut().poll(context)
        })) {
            Ok(std::task::Poll::Ready(value)) => std::task::Poll::Ready(Ok(value)),
            Ok(std::task::Poll::Pending) => std::task::Poll::Pending,
            Err(_) => std::task::Poll::Ready(Err(())),
        }
    })
    .await
}

fn inspect_recovery(
    lab: &Path,
) -> std::result::Result<Vec<NativeVoiceRecoveryCandidate>, &'static str> {
    match fs::symlink_metadata(lab) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("native_voice_recovery_inventory_failed"),
        Ok(_) => {}
    }
    let _directories = recovery::pin_directory(lab).map_err(|error| file_error(error.into()))?;
    let mut candidates = Vec::new();
    for entry in fs::read_dir(lab)
        .map_err(|_| "native_voice_recovery_inventory_failed")?
        .take(256)
    {
        let entry = entry.map_err(|_| "native_voice_recovery_inventory_failed")?;
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| name.strip_prefix("native-status-"))
            .and_then(|id| Uuid::parse_str(id).ok())
        else {
            continue;
        };
        let Ok(bundle) = factory::Bundle::reopen(lab, BundleId(id)) else {
            continue;
        };
        let Ok(voice) = bundle.manifest.voice() else {
            continue;
        };
        if voice.observation_policy != manifest::ObservationPolicyV2::LiteralLoopback35554Durable {
            continue;
        }
        let profile = match voice.profile {
            manifest::StoredVoiceProfileV2::Svc => {
                crate::engine::connect_native_voice::NativeVoiceProfile::SimpleVoiceChat
            }
            manifest::StoredVoiceProfileV2::Plasmo => {
                crate::engine::connect_native_voice::NativeVoiceProfile::PlasmoVoice
            }
        };
        let guard = super::super::acquire_voice_mutation_guard(&bundle.copy)
            .map_err(|_| "native_voice_recovery_probe_failed")?;
        let eula_pending = eula::recovery_needed(&bundle).map_err(file_error)?;
        let runtime = NativeVoiceRuntime::from_ready(
            bundle
                .manifest
                .server_v3()
                .map(|server| server.server.runtime())
                .transpose()
                .map_err(file_error)?
                .unwrap_or(crate::engine::connect_ready_profile::ReadyProfile::Fabric1211),
        );
        let receipts = super::super::list_restoration_receipts(&bundle.copy, &guard)
            .map_err(|_| "native_voice_recovery_inventory_failed")?;
        for receipt in receipts {
            let announcement_pending = receipt.state == super::super::DurableReceiptState::Ready;
            if announcement_pending || eula_pending {
                candidates.push(NativeVoiceRecoveryCandidate {
                    bundle_id: id.to_string(),
                    receipt_id: receipt.receipt_id,
                    profile,
                    runtime_profile: runtime,
                    eula_recovery_pending: eula_pending,
                    announcement_recovery_pending: announcement_pending,
                });
                if candidates.len() >= 64 {
                    return Ok(candidates);
                }
            }
        }
    }
    Ok(candidates)
}
struct Recovered {
    announcement: Option<(Instance, DurableRestorationController)>,
    eula_pending: bool,
    runtime: NativeVoiceRuntime,
}
fn resume_recovery(
    lab: &Path,
    id: &str,
    receipt_id: &str,
) -> std::result::Result<Recovered, &'static str> {
    let id = Uuid::parse_str(id).map_err(|_| "native_voice_recovery_selection_stale")?;
    if receipt_id.len() > 128 {
        return Err("native_voice_recovery_selection_stale");
    }
    let bundle = factory::Bundle::reopen(lab, BundleId(id)).map_err(file_error)?;
    if bundle
        .manifest
        .voice()
        .map_err(file_error)?
        .observation_policy
        != manifest::ObservationPolicyV2::LiteralLoopback35554Durable
    {
        return Err("native_voice_recovery_selection_stale");
    }
    let guard = super::super::acquire_voice_mutation_guard(&bundle.copy)
        .map_err(|_| "native_voice_recovery_probe_failed")?;
    let eula_pending = eula::recovery_needed(&bundle).map_err(file_error)?;
    let runtime = NativeVoiceRuntime::from_ready(
        bundle
            .manifest
            .server_v3()
            .map(|server| server.server.runtime())
            .transpose()
            .map_err(file_error)?
            .unwrap_or(crate::engine::connect_ready_profile::ReadyProfile::Fabric1211),
    );
    let summary = super::super::list_restoration_receipts(&bundle.copy, &guard)
        .map_err(|_| "native_voice_recovery_inventory_failed")?
        .into_iter()
        .find(|receipt| receipt.receipt_id == receipt_id)
        .ok_or("native_voice_recovery_selection_stale")?;
    let announcement = match summary.state {
        super::super::DurableReceiptState::Ready => {
            let receipt = super::super::resume_restoration(&bundle.copy, &guard, receipt_id)
                .map_err(|_| "native_voice_recovery_resume_failed")?;
            Some((bundle.copy.clone(), receipt))
        }
        super::super::DurableReceiptState::Restored
        | super::super::DurableReceiptState::Retired => None,
    };
    if !eula_pending && summary.state != super::super::DurableReceiptState::Ready {
        return Err("native_voice_recovery_selection_stale");
    }
    Ok(Recovered {
        announcement,
        eula_pending,
        runtime,
    })
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
pub(super) mod actor_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "explicit own-copy file transaction over compiled local JAR/JDK pins; no Java execution"]
    fn native_voice_bridge_files_real_resource_transaction() {
        for profile in [
            crate::engine::connect_native_voice::NativeVoiceProfile::SimpleVoiceChat,
            crate::engine::connect_native_voice::NativeVoiceProfile::PlasmoVoice,
        ] {
            let plan = discover_plan(profile).expect("native_voice_compiled_resources");
            assert_eq!(plan.review().purpose, "native_voice_bridge_factory_v2");
            assert!(plan.review().after.contains("35554"));
            let digest = plan.review().digest.clone();
            let mut prepared = plan.create(&digest).expect("native_voice_guarded_factory");
            let voice = prepared.bundle.manifest.voice_v2().unwrap().clone();
            assert_eq!(
                voice.observation_policy,
                manifest::ObservationPolicyV2::LiteralLoopback35554Durable
            );
            let (copy, receipt) = prepared
                .announcement_recovery
                .take()
                .expect("native_voice_durable_receipt");
            let configuration = copy.root().join(voice.profile.configuration_name());
            assert_eq!(
                fs::read(&configuration).unwrap(),
                voice.observation_policy.configuration(voice.profile)
            );
            let plan =
                plan_voice_execution(prepared).expect("native_voice_announced_execution_plan");
            assert_eq!(plan.review().purpose, "native_voice_execution_v2");
            drop(plan); // The execution review was never consumed; no process.
            let receipt_id = receipt.receipt_id().to_owned();
            drop(receipt);
            let guard = super::super::super::acquire_voice_mutation_guard(&copy).unwrap();
            let mut receipt = super::super::super::resume_restoration(&copy, &guard, &receipt_id)
                .expect("native_voice_resume_durable_receipt");
            let review = receipt
                .prepare_strict(&copy, &guard)
                .expect("native_voice_restore_review");
            receipt
                .confirm(&copy, &guard, &review.digest)
                .expect("native_voice_restore_confirm");
            assert_eq!(
                fs::read(configuration).unwrap(),
                voice.profile.configuration()
            );
            assert_eq!(fs::read(copy.root().join("eula.txt")).unwrap(), EULA_FALSE);
            assert_eq!(receipt.state(), RestorationState::Restored);
        }
    }
}
