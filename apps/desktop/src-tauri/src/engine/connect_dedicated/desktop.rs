//! One native actor owns the source, its original completion and local player
//! bridges. IPC waiters own no process, query, socket or restoration authority.
use super::super::{DurableRestorationController, RestorationState};
use super::*;
use crate::engine::{
    connect::{ConnectManager, GroupParticipantRequest},
    connect_native_voice::*,
};
use aurora_connect_lab as lab;
use producer::{DedicatedPhase, DedicatedPlan, DedicatedRun, EulaDisposition};
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
                .clone()),
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
                    NativeVoiceRequest::Prepare { .. } | NativeVoiceRequest::InspectRecovery
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
                let epoch = connect.social_capabilities().account_epoch;
                let actor = Actor::new(
                    connect.native_voice_shared_manager(),
                    app_data.join(resources::LAB_NAME),
                    snapshot.clone(),
                    stop.clone(),
                    completion.clone(),
                    source.clone(),
                    claimed.clone(),
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
                    epoch,
                }));
            }
            current.as_ref().unwrap().clone()
        };
        if client.cancel.is_cancelled() {
            return Err("native_voice_closed");
        }
        let stopping = matches!(request, NativeVoiceRequest::Stop);
        if stopping {
            client.stop.store(true, Ordering::Release);
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
    pub(crate) fn take_social_source(
        &self,
        connect: &ConnectManager,
    ) -> std::result::Result<NativeVoiceHostBinding, &'static str> {
        let current = self.client.lock().map_err(|_| "native_voice_internal")?;
        let client = current.as_ref().ok_or("native_voice_no_session")?;
        if client.cancel.is_cancelled()
            || client.stop.load(Ordering::Acquire)
            || client.busy.load(Ordering::Acquire)
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
        client
            .claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "native_voice_source_already_claimed")?;
        Ok(binding)
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
    completion: Arc<Mutex<Option<producer::DedicatedCompletion>>>,
    connect: ConnectManager,
    lab: PathBuf,
    shared: Arc<Mutex<NativeVoiceSnapshot>>,
    view: NativeVoiceSnapshot,
    stop: Arc<AtomicBool>,
    factory: Option<FactoryPlan>,
    execution: Option<DedicatedPlan>,
    source: Option<DedicatedRun>,
    recovery: Option<(Instance, DurableRestorationController)>,
    review_deadline: Option<Instant>,
    host_generation: Option<u64>,
    host_bind: Option<std::net::UdpSocket>,
    local_key: Option<lab::SecretKey>,
    local_guest: Option<lab::Guest>,
    local_tcp: Option<lab::TcpBridge>,
    local_udp: Option<lab::UdpBridge>,
}
impl Actor {
    fn new(
        connect: ConnectManager,
        lab: PathBuf,
        shared: Arc<Mutex<NativeVoiceSnapshot>>,
        stop: Arc<AtomicBool>,
        completion: Arc<Mutex<Option<producer::DedicatedCompletion>>>,
        shared_source: Arc<Mutex<Option<NativeVoiceHostBinding>>>,
        claimed: Arc<AtomicBool>,
    ) -> Self {
        let view = shared.lock().unwrap().clone();
        Self {
            shared_source,
            claimed,
            completion,
            connect,
            lab,
            shared,
            view,
            stop,
            factory: None,
            execution: None,
            source: None,
            recovery: None,
            review_deadline: None,
            host_generation: None,
            host_bind: None,
            local_key: None,
            local_guest: None,
            local_tcp: None,
            local_udp: None,
        }
    }
    fn publish(&mut self) {
        self.view.revision = self.view.revision.saturating_add(1);
        if let Ok(mut target) = self.shared.lock() {
            *target = self.view.clone();
        }
    }
    fn review(&mut self, kind: &'static str, review: &Review) {
        self.view.review = Some(NativeVoiceApproval { kind, digest: review.digest.clone(), before: review.before.clone(), after: review.after.clone(), details: vec![
            "Minecraft 1.21.1 / Fabric 0.16.14 / Java 21 em uma cópia dedicada nova. A origem pode ficar ativa por até 4 horas. Cada sessão respeita seu prazo original aprovado.".into(),
            "Servidor privado loopback43001; login Minecraft offline; whitelist desligada; máximo de 8 jogadores; transporte Aurora autenticado. A execução aplica a EULA revisada e recolhe o mesmo processo antes de restaurá-la.".into(),
            "Endereço do anúncio 127.0.0.1:35554; mod e porta de origem verificados nativamente; áudio ainda precisa de teste.".into(),
            "Recibo Recovery durável. Encerrar o transporte não encerra a origem. Restaurar anúncio exige outra confirmação após encerrar Java.".into(),
        ] });
        self.review_deadline = Some(Instant::now() + REVIEW_TTL);
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
                    let stopping = matches!(request, NativeVoiceRequest::Stop);
                    let outcome = match catch_future(self.handle(request, &cancel)).await {
                        Ok(result) => result,
                        Err(()) => { self.stop.store(true, Ordering::Release); let _ = self.stop_all().await; Err("native_voice_worker_failed") }
                    };
                    if let Err(code) = outcome { self.view.error_code = Some(code); }
                    self.publish();
                    let response = outcome.map(|invitation| { let mut snapshot = self.view.clone(); snapshot.exported_invitation = invitation; snapshot });
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
        if self.source.is_some()
            && self.claimed.load(Ordering::Acquire)
            && self.view.operation_id.is_none()
            && self.view.phase == "ready"
        {
            self.view.phase = "socialSource";
        }
        if self.stop.load(Ordering::Acquire)
            && (self.source.is_some() || self.factory.is_some() || self.execution.is_some())
        {
            if let Err(code) = self.stop_all().await {
                self.view.error_code = Some(code);
            }
            return;
        }
        if self
            .review_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.factory.take();
            self.execution.take();
            self.view.review = None;
            self.review_deadline = None;
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
                .review_deadline
                .is_none_or(|deadline| Instant::now() >= deadline)
        {
            return Err("native_voice_stale_review");
        }
        Ok(())
    }
    async fn handle(
        &mut self,
        request: NativeVoiceRequest,
        cancel: &CancellationToken,
    ) -> std::result::Result<Option<String>, &'static str> {
        self.view.error_code = None;
        match request {
            NativeVoiceRequest::Prepare { profile } => {
                if self.source.is_some()
                    || self.view.recovery_pending
                    || self.factory.is_some()
                    || self.execution.is_some()
                {
                    return Err("native_voice_busy");
                }
                self.stop.store(false, Ordering::Release);
                self.claimed.store(false, Ordering::Release);
                self.view = NativeVoiceSnapshot {
                    revision: self.view.revision,
                    profile: Some(profile),
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
                self.review("files", plan.review());
                self.factory = Some(plan);
                self.view.phase = "filesReviewed";
            }
            NativeVoiceRequest::InspectRecovery => {
                if self.source.is_some()
                    || self.execution.is_some()
                    || self.factory.is_some()
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
                    || self.execution.is_some()
                    || self.factory.is_some()
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
                let recovery = tokio::task::spawn_blocking(move || {
                    resume_recovery(&lab, &bundle_id, &receipt_id)
                })
                .await
                .map_err(|_| "native_voice_worker_failed")??;
                self.recovery = Some(recovery);
                self.view.profile = Some(candidate.profile);
                self.view.recovery_pending = true;
                self.view.phase = "stopped";
                self.view.review = None;
                self.review_deadline = None;
            }
            NativeVoiceRequest::ApproveFiles { digest } => {
                self.approval("files", &digest)?;
                if self.stop.load(Ordering::Acquire) {
                    return Err("native_voice_cancelled");
                }
                let plan = self.factory.take().ok_or("native_voice_stale_review")?;
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
                self.review("execution", plan.review());
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
                self.review_deadline = None;
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
                self.view.review = Some(NativeVoiceApproval { kind: "host", digest: review.digest, before: "Origem dedicada própria; nenhum Host Connect".into(), after: format!("Host de {} participantes explícitos; TCP game + UDP aux; anúncio {}", self.view.participants.len(), review.data.announcement_policy), details: vec![format!("Origem nativa {} / {}; destinos fixados pelo mesmo processo; nenhuma porta recebida da interface.", review.profile, review.artifact_version), "Cada falha de observação retira UDP terminalmente; TCP conserva seus gates próprios. O Host jogador ocupa uma vaga e 127.0.0.1:35554.".into(), "Transporte local experimental; áudio/auth do mod precisam de validação manual.".into()] });
                self.review_deadline = Some(Instant::now() + REVIEW_TTL);
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
                self.review_deadline = None;
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
            NativeVoiceRequest::Stop => {
                self.stop.store(true, Ordering::Release);
                self.stop_all().await?;
            }
            NativeVoiceRequest::PrepareRestore { reconciled } => {
                if self.source.is_some() || self.execution.is_some() || self.factory.is_some() {
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
                });
                self.review_deadline = Some(Instant::now() + REVIEW_TTL);
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
                self.review_deadline = None;
                self.view.phase = "restored";
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
            self.connect
                .stop("main", id)
                .await
                .map_err(|error| error.code())?;
        }
        self.view.udp_state = "closed";
        Ok(())
    }
    async fn stop_all(&mut self) -> std::result::Result<(), &'static str> {
        self.view.phase = "stopping";
        self.view.review = None;
        self.review_deadline = None;
        self.publish();
        self.factory.take();
        self.execution.take();
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
            self.source.take();
        }
        self.view.recovery_pending = self
            .recovery
            .as_ref()
            .is_some_and(|(_, receipt)| receipt.state() != RestorationState::Restored);
        self.view.phase = if self.view.eula_state == "needsRecovery" {
            "needsRecovery"
        } else {
            "stopped"
        };
        transport
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
        let Ok(voice) = bundle.manifest.voice_v2() else {
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
        let receipts = super::super::list_restoration_receipts(&bundle.copy, &guard)
            .map_err(|_| "native_voice_recovery_inventory_failed")?;
        for receipt in receipts {
            if receipt.state == super::super::DurableReceiptState::Ready {
                candidates.push(NativeVoiceRecoveryCandidate {
                    bundle_id: id.to_string(),
                    receipt_id: receipt.receipt_id,
                    profile,
                });
                if candidates.len() >= 64 {
                    return Ok(candidates);
                }
            }
        }
    }
    Ok(candidates)
}
fn resume_recovery(
    lab: &Path,
    id: &str,
    receipt_id: &str,
) -> std::result::Result<(Instance, DurableRestorationController), &'static str> {
    let id = Uuid::parse_str(id).map_err(|_| "native_voice_recovery_selection_stale")?;
    if receipt_id.len() > 128 {
        return Err("native_voice_recovery_selection_stale");
    }
    let bundle = factory::Bundle::reopen(lab, BundleId(id)).map_err(file_error)?;
    if bundle
        .manifest
        .voice_v2()
        .map_err(file_error)?
        .observation_policy
        != manifest::ObservationPolicyV2::LiteralLoopback35554Durable
    {
        return Err("native_voice_recovery_selection_stale");
    }
    let guard = super::super::acquire_voice_mutation_guard(&bundle.copy)
        .map_err(|_| "native_voice_recovery_probe_failed")?;
    let receipt = super::super::resume_restoration(&bundle.copy, &guard, receipt_id)
        .map_err(|_| "native_voice_recovery_resume_failed")?;
    Ok((bundle.copy.clone(), receipt))
}

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
