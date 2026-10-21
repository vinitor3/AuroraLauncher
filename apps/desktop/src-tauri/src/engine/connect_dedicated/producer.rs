//! Explicit native STATUS execution. The file-only EULA review is not a run
//! approval. This module has no launcher or IPC caller.
use super::*;
use crate::engine::connect_owned_run::{
    OwnedRun, OwnedRunError, OwnedRunFailure, OwnedRunObserver, OwnedRunReport,
};
use aurora_connect_lab::MinecraftStatusError;
use std::{
    collections::BTreeMap,
    net::{Ipv4Addr, SocketAddr},
    process::Command,
    sync::{Arc, Condvar, Mutex, MutexGuard, Weak},
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

#[path = "producer_voice.rs"]
mod voice;
#[cfg(test)]
pub(super) use voice::plan_voice_for_test;
#[cfg_attr(not(test), allow(unused_imports))]
// Private experimental API; no product caller yet.
pub(crate) use voice::{
    DedicatedVoiceSource, VoiceRuntimeAnnouncement, VoiceRuntimeBinding, VoiceRuntimeError,
    VoiceRuntimeSample,
};

pub(super) const ENDPOINT: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST), 43001);
pub(super) const PROTOCOL: i32 = 767;
const PURPOSE: &str = "native_dedicated_status_execution";
type RunResult<T> = std::result::Result<T, DedicatedError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DedicatedError {
    Preparation(Error),
    Cancelled,
    Closed,
    NotReady,
    Busy,
    Thread,
    #[cfg(test)]
    Internal,
    Source(OwnedRunError),
}
impl From<Error> for DedicatedError {
    fn from(error: Error) -> Self {
        Self::Preparation(error)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DedicatedReason {
    Requested,
    Cancelled,
    Dropped,
    Deadline,
    Failure,
    Exited,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DedicatedFailure {
    Cancelled,
    Dropped,
    Deadline,
    Preparation(Error),
    Runtime,
    Spawn(OwnedRunError),
    Child(OwnedRunFailure),
    Source,
    Readiness,
    Protocol,
    Status(MinecraftStatusError),
    Panic,
    Internal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DedicatedPhase {
    Starting,
    Ready,
    Stopping,
    Complete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EulaDisposition {
    Unchanged,
    Restored,
    NeedsRecovery,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DedicatedSnapshot {
    pub(crate) phase: DedicatedPhase,
    pub(crate) reason: Option<DedicatedReason>,
    pub(crate) failure: Option<DedicatedFailure>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DedicatedReport {
    pub(crate) reason: DedicatedReason,
    pub(crate) failure: Option<DedicatedFailure>,
    pub(crate) process: Option<OwnedRunReport>,
    pub(crate) eula: EulaDisposition,
    pub(crate) reached_ready: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) struct Limits {
    pub lifetime_ms: u64,
    pub readiness_ms: u64,
    pub stop_ms: u64,
    pub output_bytes: u64,
    pub attempts: u32,
}
impl Limits {
    fn production() -> Self {
        Self {
            lifetime_ms: 300_000,
            readiness_ms: 180_000,
            stop_ms: 5_000,
            output_bytes: 4 * 1024 * 1024,
            attempts: 180,
        }
    }
    fn valid(self) -> bool {
        (1..=300_000).contains(&self.lifetime_ms)
            && (1..=180_000).contains(&self.readiness_ms)
            && self.readiness_ms <= self.lifetime_ms
            && (1..=5_000).contains(&self.stop_ms)
            && self.stop_ms <= self.lifetime_ms
            && (1..=4 * 1024 * 1024).contains(&self.output_bytes)
            && (1..=180).contains(&self.attempts)
    }
}

/// Compiled code builds this recipe. No JSON, argv, environment or executable
/// supplied by a caller can be decoded into it.
#[derive(Serialize)]
pub(super) struct Recipe {
    executable: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
    environment: BTreeMap<String, String>,
    temp: PathBuf,
    endpoint: SocketAddr,
    protocol: i32,
    pub limits: Limits,
    #[cfg(test)]
    synthetic: bool,
}
impl Recipe {
    fn production(prepared: &factory::PreparedStatus) -> Result<Self> {
        prepared.bundle.manifest.status_v1()?;
        Self::production_paths(
            &prepared.resources,
            prepared.bundle.copy.root().to_path_buf(),
        )
    }
    fn voice_production(prepared: &factory::PreparedStatus) -> Result<Self> {
        prepared.bundle.manifest.voice_v2()?;
        Self::production_paths(
            &prepared.resources,
            prepared.bundle.copy.root().to_path_buf(),
        )
    }
    // Kept private: executable selection still comes from verified Resources,
    // never from an argv/recipe/approval deserialized by a caller or test index.
    fn production_paths(resources: &resources::Resources, cwd: PathBuf) -> Result<Self> {
        let temp = cwd.join(".connect-run-temp");
        absent(&temp)?;
        let windows = windows_directory()?;
        let own_temp = temp.to_str().ok_or(Error::UnsafePath)?.to_owned();
        let environment = BTreeMap::from([
            ("SystemRoot".into(), windows.clone()),
            ("windir".into(), windows),
            ("TEMP".into(), own_temp.clone()),
            ("TMP".into(), own_temp.clone()),
            ("USERPROFILE".into(), own_temp),
        ]);
        Ok(Self {
            executable: resources.java_executable()?.to_path_buf(),
            args: vec![
                "-Djava.io.tmpdir=.connect-run-temp".into(),
                "-Duser.home=.connect-run-temp".into(),
                "-XX:-UsePerfData".into(),
                "-Xms256M".into(),
                "-Xmx1536M".into(),
                "-Dfabric.gameJarPath=server.jar".into(),
                "-cp".into(),
                resources::status_classpath(),
                "net.fabricmc.loader.impl.launch.knot.KnotServer".into(),
                "nogui".into(),
            ],
            cwd,
            environment,
            temp,
            endpoint: ENDPOINT,
            protocol: PROTOCOL,
            limits: Limits::production(),
            #[cfg(test)]
            synthetic: false,
        })
    }
    pub(super) fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .args(&self.args)
            .current_dir(&self.cwd)
            .env_clear()
            .envs(&self.environment);
        command
    }
    pub(super) fn prepare_temp(&self) -> Result<Vec<File>> {
        absent(&self.temp)?;
        fs::create_dir(&self.temp)?;
        Ok(recovery::pin_directory(&self.temp)?)
    }
}

fn windows_directory() -> Result<String> {
    use std::os::windows::ffi::OsStringExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetWindowsDirectoryW(buffer: *mut u16, size: u32) -> u32;
    }
    let mut buffer = vec![0_u16; 1025];
    // SAFETY: writable UTF-16 storage covers the exact size supplied to Win32.
    let length = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() || buffer[..length].contains(&0) {
        return Err(Error::Resources);
    }
    let path = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length]));
    explicit_path(&path)?;
    recovery::pin_directory(&path)?;
    path.to_str().map(str::to_owned).ok_or(Error::UnsafePath)
}

pub(crate) struct DedicatedPlan {
    prepared: factory::PreparedStatus,
    approval: ExecutionApproval,
    hooks: ProducerHooks,
}
/// Only a consumed DedicatedPlan constructs this private review authority.
/// EULA uses its same digest and checks it again immediately before Claim.
pub(super) struct ExecutionApproval {
    pub review: Review,
    pub recipe: Recipe,
    deadline: Instant,
    file_digest: String,
    file_review_id: Uuid,
    expected_kind: manifest::ManifestKind,
}
impl ExecutionApproval {
    pub(super) fn check_fresh(&self) -> Result<()> {
        approve(&self.review, self.deadline, &self.review.digest)
    }
    fn digest_for(&self, prepared: &factory::PreparedStatus) -> Result<String> {
        Ok(digest(&execution_digest_input(
            self.review.review_id,
            &prepared.bundle.manifest,
            &self.file_digest,
            &self.recipe,
        )?))
    }
    pub(super) fn check(&self, prepared: &factory::PreparedStatus) -> Result<()> {
        prepared
            .approved_files
            .require_manifest_kind(&prepared.bundle.manifest)?;
        if prepared.bundle.manifest.kind() != self.expected_kind {
            return Err(Error::Identity);
        }
        self.check_fresh()?;
        prepared.confirm_fresh(&self.file_digest)?;
        prepared.resources.recheck()?;
        prepared.bundle.validate(&prepared.guard)?;
        if eula::execution_digest(&prepared.bundle, self.file_review_id)? != self.file_digest
            || self.digest_for(prepared)? != self.review.digest
        {
            return Err(Error::Changed);
        }
        Ok(())
    }
}
fn execution_digest_input(
    review_id: Uuid,
    manifest: &factory::Manifest,
    file_digest: &str,
    recipe: &Recipe,
) -> Result<Vec<u8>> {
    match manifest.kind() {
        manifest::ManifestKind::StatusV1 => {
            encode(&(1, PURPOSE, review_id, manifest, file_digest, recipe))
        }
        manifest::ManifestKind::VoiceV2(_) => encode(&(
            2,
            "native_voice_execution_v2",
            review_id,
            manifest,
            file_digest,
            recipe,
        )),
    }
}

#[allow(dead_code)] // Explicit internal execution plan; no IPC or automatic caller.
pub(crate) fn plan(prepared: factory::PreparedStatus) -> RunResult<DedicatedPlan> {
    prepared.bundle.manifest.status_v1()?;
    let recipe = Recipe::production(&prepared)?;
    #[cfg(test)]
    let hooks = Arc::new(TestHooks::default());
    #[cfg(not(test))]
    let hooks = ProducerHooks;
    make_plan(prepared, recipe, hooks)
}
#[allow(dead_code)] // Separate closed V2 entry point, never a STATUS relabeling.
pub(crate) fn plan_voice(prepared: factory::PreparedStatus) -> RunResult<DedicatedPlan> {
    prepared.bundle.manifest.voice_v2()?;
    let recipe = Recipe::voice_production(&prepared)?;
    #[cfg(test)]
    let hooks = Arc::new(TestHooks::default());
    #[cfg(not(test))]
    let hooks = ProducerHooks;
    make_plan(prepared, recipe, hooks)
}
fn make_plan(
    prepared: factory::PreparedStatus,
    recipe: Recipe,
    hooks: ProducerHooks,
) -> RunResult<DedicatedPlan> {
    if !recipe.limits.valid() {
        return Err(DedicatedError::Preparation(Error::Limit));
    }
    let deadline = prepared.approval_deadline();
    let file_digest = prepared.review().digest.clone();
    let file_review_id = prepared.review().review_id;
    prepared
        .approved_files
        .require_manifest_kind(&prepared.bundle.manifest)?;
    let expected_kind = prepared.bundle.manifest.kind();
    let mut approval = ExecutionApproval {
        review: Review {
            review_id: Uuid::new_v4(),
            digest: String::new(),
            purpose: match expected_kind {
                manifest::ManifestKind::StatusV1 => PURPOSE,
                manifest::ManifestKind::VoiceV2(_) => "native_voice_execution_v2",
            },
            bundle_id: prepared.bundle_id(),
            before: match expected_kind {
                manifest::ManifestKind::StatusV1 => {
                    "Cópia STATUS privada; EULA false; nenhum processo".into()
                }
                manifest::ManifestKind::VoiceV2(_) => {
                    "Cópia de voz privada; EULA false; nenhum processo".into()
                }
            },
            after: match expected_kind {
                manifest::ManifestKind::StatusV1 => {
                    "Executar dedicado STATUS767 próprio e restaurar EULA após recolher o processo"
                        .into()
                }
                manifest::ManifestKind::VoiceV2(_) => {
                    #[cfg(test)]
                    if recipe.synthetic {
                        "Executar fixture própria sem Java; observar STATUS/configuração/UDP; restaurar EULA após a completion original".into()
                    } else {
                        "Executar Java dedicado com perfil de voz privado; observar STATUS/configuração/UDP; restaurar EULA após a completion original".into()
                    }
                    #[cfg(not(test))]
                    {
                        "Executar Java dedicado com perfil de voz privado; observar STATUS/configuração/UDP; restaurar EULA após a completion original".into()
                    }
                }
            },
            details: {
                let recipe_detail = serde_json::to_string(&recipe).map_err(|_| Error::Identity)?;
                match expected_kind {
                    manifest::ManifestKind::StatusV1 => vec![
                        recipe_detail,
                        "Sem mods/LOGIN/voz; observação não autoriza Host ou convites".into(),
                    ],
                    manifest::ManifestKind::VoiceV2(_) => {
                        let mut details = vec![recipe_detail, "Observação limitada; binding IPv4 literal; anúncio padrão; sem áudio/Host/convites".into()];
                        details.push(format!("Limites aprovados: lifetime {}ms; readiness {}ms; stop {}ms; stdout+stderr {} bytes; {} tentativas; heap máximo 1536MiB; prazo original da preparação sem renovação", recipe.limits.lifetime_ms, recipe.limits.readiness_ms, recipe.limits.stop_ms, recipe.limits.output_bytes, recipe.limits.attempts));
                        // Execution presents immutable profile metadata from
                        // the manifest; it must not inherit the file-only
                        // EULA review's "no Java in this checkpoint" wording.
                        let voice = prepared.bundle.manifest.voice_v2()?;
                        details.push(match voice.profile {
                            manifest::StoredVoiceProfileV2::Svc => {
                                "Perfil: Simple Voice Chat 2.6.22".into()
                            }
                            manifest::StoredVoiceProfileV2::Plasmo => {
                                "Perfil: Plasmo Voice 2.1.17; Fabric API 0.116.10+1.21.1".into()
                            }
                        });
                        details.push(format!(
                            "Runtime aprovado: Minecraft {}; Fabric {}; protocolo {}",
                            voice.runtime.minecraft,
                            voice.runtime.loader_version,
                            voice.runtime.protocol
                        ));
                        details.push(format!(
                            "Configuração inicial compilada: {}; {} bytes; SHA-256 {}",
                            voice.profile.configuration_name(),
                            voice.profile.configuration().len(),
                            voice.profile.configuration_sha256()
                        ));
                        let initial = &voice.initial_voice_configuration.observation;
                        details.push(format!(
                            "Observação inicial: volume {}; índice {}; {} bytes; SHA-256 {}",
                            initial.identity.volume,
                            initial.identity.index,
                            initial.bytes,
                            initial.sha256
                        ));
                        details
                    }
                }
            },
        },
        recipe,
        deadline,
        file_digest,
        file_review_id,
        expected_kind,
    };
    approval.review.digest = approval.digest_for(&prepared)?;
    approval.check(&prepared)?;
    Ok(DedicatedPlan {
        prepared,
        approval,
        hooks,
    })
}
#[allow(dead_code)] // Consumes only the separately reviewed execution plan.
impl DedicatedPlan {
    pub(crate) fn review(&self) -> &Review {
        &self.approval.review
    }
    pub(crate) fn start(
        self,
        expected: &str,
        cancel: CancellationToken,
    ) -> RunResult<DedicatedRun> {
        approve(&self.approval.review, self.approval.deadline, expected)?;
        self.approval.check(&self.prepared)?;
        if cancel.is_cancelled() {
            return Err(DedicatedError::Cancelled);
        }
        #[cfg(test)]
        if self.hooks.fail_thread {
            return Err(DedicatedError::Thread);
        }
        let native_nonce = Uuid::new_v4();
        let voice_context = match self.prepared.bundle.manifest.kind() {
            manifest::ManifestKind::StatusV1 => None,
            manifest::ManifestKind::VoiceV2(_) => Some(
                factory::ImmutableVoiceContext::from_prepared(&self.prepared, native_nonce)?,
            ),
        };
        self.approval.check_fresh()?;
        let started = Instant::now();
        let (sender, receiver) = watch::channel(None);
        let child_cancel = cancel.child_token();
        let shared = Arc::new(Shared {
            native_nonce,
            voice_context,
            state: Mutex::new(State {
                phase: DedicatedPhase::Starting,
                reason: None,
                failure: None,
                run: Weak::new(),
                observer: None,
                query: false,
                reached_ready: false,
            }),
            changed: Condvar::new(),
            cancel,
            child_cancel,
            status_cancel: CancellationToken::new(),
            deadline: started + Duration::from_millis(self.approval.recipe.limits.lifetime_ms),
            readiness_deadline: started
                + Duration::from_millis(self.approval.recipe.limits.readiness_ms),
            hooks: self.hooks.clone(),
        });
        let worker_shared = shared.clone();
        // The closure owns only a preparation here. Failed thread creation
        // cannot have applied EULA or launched a child.
        std::thread::Builder::new()
            .name("connect-dedicated-status".into())
            .spawn(move || eula::producer_worker::run(self, worker_shared, sender))
            .map_err(|_| DedicatedError::Thread)?;
        Ok(DedicatedRun {
            shared,
            completion: DedicatedCompletion { receiver },
        })
    }
    pub(super) fn into_parts(self) -> (factory::PreparedStatus, ExecutionApproval, ProducerHooks) {
        (self.prepared, self.approval, self.hooks)
    }
    #[cfg(test)]
    pub(super) fn expire_for_test(&mut self) {
        self.approval.deadline = Instant::now() - Duration::from_secs(1);
    }
}

pub(crate) struct DedicatedRun {
    shared: Arc<Shared>,
    completion: DedicatedCompletion,
}
#[derive(Clone)]
pub(crate) struct DedicatedObserver {
    shared: Weak<Shared>,
}
#[derive(Clone)]
pub(crate) struct DedicatedSource {
    shared: Weak<Shared>,
}
#[derive(Clone)]
pub(crate) struct DedicatedCompletion {
    receiver: watch::Receiver<Option<DedicatedReport>>,
}
#[allow(dead_code)] // No UI/caller is installed by this experimental producer.
impl DedicatedRun {
    pub(crate) fn observer(&self) -> DedicatedObserver {
        DedicatedObserver {
            shared: Arc::downgrade(&self.shared),
        }
    }
    pub(crate) fn source(&self) -> DedicatedSource {
        DedicatedSource {
            shared: Arc::downgrade(&self.shared),
        }
    }
    pub(crate) fn completion(&self) -> DedicatedCompletion {
        self.completion.clone()
    }
    pub(crate) fn request_stop(&self) -> DedicatedCompletion {
        self.shared.request(DedicatedReason::Requested, None);
        self.completion()
    }
}
impl Drop for DedicatedRun {
    fn drop(&mut self) {
        self.shared
            .request(DedicatedReason::Dropped, Some(DedicatedFailure::Dropped));
    }
}
#[allow(dead_code)]
impl DedicatedObserver {
    pub(crate) fn snapshot(&self) -> Option<DedicatedSnapshot> {
        let shared = self.shared.upgrade()?;
        let mut state = shared.lock();
        shared.refresh(&mut state);
        Some(DedicatedSnapshot {
            phase: state.phase,
            reason: state.reason,
            failure: state.failure,
        })
    }
}
#[allow(dead_code)]
impl DedicatedSource {
    /// One exact sample of this run. This supplies no Host admission authority.
    pub(crate) fn observe_tcp(&self) -> RunResult<()> {
        let shared = self.shared.upgrade().ok_or(DedicatedError::Closed)?;
        let job = shared.reserve(QueryKind::Tcp)?;
        shared.hooks.at(ProducerPoint::BeforeQuery);
        let result = job
            .observer
            .observe_tcp(ENDPOINT)
            .map_err(DedicatedError::Source);
        if result.is_ok() {
            shared.hooks.at(ProducerPoint::AfterQuery);
        }
        let mut state = shared.lock();
        shared.refresh(&mut state);
        if state.reason.is_some() {
            return Err(DedicatedError::Closed);
        }
        result
    }
}
#[allow(dead_code)]
impl DedicatedCompletion {
    pub(crate) async fn wait(&self) -> DedicatedReport {
        let mut receiver = self.receiver.clone();
        loop {
            if let Some(report) = receiver.borrow_and_update().clone() {
                return report;
            }
            if receiver.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    }
}

pub(super) struct State {
    pub phase: DedicatedPhase,
    pub reason: Option<DedicatedReason>,
    pub failure: Option<DedicatedFailure>,
    pub run: Weak<OwnedRun>,
    pub observer: Option<OwnedRunObserver>,
    pub query: bool,
    pub reached_ready: bool,
}
impl State {
    pub fn seal(&mut self, reason: DedicatedReason, failure: Option<DedicatedFailure>) {
        if self.phase == DedicatedPhase::Complete {
            return;
        }
        self.reason.get_or_insert(reason);
        if self.failure.is_none() {
            self.failure = failure;
        }
        self.phase = DedicatedPhase::Stopping;
    }
}
pub(super) struct Shared {
    pub native_nonce: Uuid,
    pub voice_context: Option<factory::ImmutableVoiceContext>,
    pub state: Mutex<State>,
    pub changed: Condvar,
    pub cancel: CancellationToken,
    pub child_cancel: CancellationToken,
    pub status_cancel: CancellationToken,
    pub deadline: Instant,
    pub readiness_deadline: Instant,
    pub hooks: ProducerHooks,
}
impl Shared {
    pub fn lock(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                let mut state = poison.into_inner();
                state.seal(DedicatedReason::Failure, Some(DedicatedFailure::Internal));
                self.status_cancel.cancel();
                self.child_cancel.cancel();
                state
            }
        }
    }
    pub fn refresh(&self, state: &mut State) {
        if state.phase == DedicatedPhase::Complete {
            return;
        }
        if self.cancel.is_cancelled() {
            state.seal(
                DedicatedReason::Cancelled,
                Some(DedicatedFailure::Cancelled),
            );
            self.child_cancel.cancel();
        }
        if Instant::now() >= self.deadline {
            state.seal(DedicatedReason::Deadline, Some(DedicatedFailure::Deadline));
            self.child_cancel.cancel();
        }
        if state.reason.is_some() {
            self.status_cancel.cancel();
            self.changed.notify_all();
        }
    }
    pub fn request(&self, reason: DedicatedReason, failure: Option<DedicatedFailure>) {
        let run = {
            let mut state = self.lock();
            self.refresh(&mut state);
            // Repeated control disposal/stop does not turn a prior normal
            // Requested stop into Dropped. Actual failures and fresh external
            // cancellation/deadlines still latch through their own paths.
            if state.reason.is_none()
                || !matches!(
                    reason,
                    DedicatedReason::Requested | DedicatedReason::Dropped
                )
            {
                state.seal(reason, failure);
            }
            self.status_cancel.cancel();
            if !matches!(state.reason, Some(DedicatedReason::Requested)) {
                self.child_cancel.cancel();
            }
            self.changed.notify_all();
            state.run.upgrade()
        };
        // No I/O or await and no ownership retained by an observer/snapshot.
        if let Some(run) = run {
            run.request_stop();
        }
    }
    pub fn fail(&self, failure: DedicatedFailure) {
        let reason = if failure == DedicatedFailure::Deadline {
            DedicatedReason::Deadline
        } else {
            DedicatedReason::Failure
        };
        self.request(reason, Some(failure));
    }
    pub fn open(&self) -> bool {
        let mut state = self.lock();
        self.refresh(&mut state);
        state.reason.is_none()
    }
    pub fn reserve(self: &Arc<Self>, kind: QueryKind) -> RunResult<QueryJob> {
        let mut state = self.lock();
        self.refresh(&mut state);
        if state.reason.is_some() {
            return Err(DedicatedError::Closed);
        }
        if kind != QueryKind::Readiness && state.phase != DedicatedPhase::Ready {
            return Err(DedicatedError::NotReady);
        }
        if kind == QueryKind::Voice
            && !self
                .voice_context
                .as_ref()
                .is_some_and(|context| context.native_nonce() == self.native_nonce)
        {
            return Err(DedicatedError::Preparation(Error::Identity));
        }
        if state.query {
            return Err(DedicatedError::Busy);
        }
        let observer = state.observer.clone().ok_or(DedicatedError::Closed)?;
        state.query = true;
        Ok(QueryJob {
            observer,
            shared: self.clone(),
        })
    }
    pub fn wait_queries(&self) {
        let mut state = self.lock();
        while state.query {
            state = match self.changed.wait(state) {
                Ok(state) => state,
                Err(poison) => {
                    let mut state = poison.into_inner();
                    state.seal(DedicatedReason::Failure, Some(DedicatedFailure::Internal));
                    state
                }
            };
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum QueryKind {
    Readiness,
    Tcp,
    Voice,
}
pub(super) struct QueryJob {
    pub observer: OwnedRunObserver,
    shared: Arc<Shared>,
}
impl Drop for QueryJob {
    fn drop(&mut self) {
        self.shared.lock().query = false;
        self.shared.changed.notify_all();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProducerPoint {
    BeforeApply,
    AfterApply,
    BeforeStart,
    AfterStart,
    BeforeReserve,
    BeforeQuery,
    AfterQuery,
    AfterStatus,
    BeforeReady,
    BeforeRestore,
    BeforePublish,
    VoiceBeforeReserve,
    VoiceAfterConfigurationA,
    VoiceAfterPositiveUdp,
    VoiceBeforeFinalGate,
}
#[cfg(not(test))]
#[derive(Clone)]
pub(super) struct ProducerHooks;
#[cfg(not(test))]
impl ProducerHooks {
    pub fn at(&self, _: ProducerPoint) {}
}
#[cfg(test)]
pub(super) type ProducerHooks = Arc<TestHooks>;
#[cfg(test)]
#[derive(Default)]
pub(super) struct TestHooks {
    pub point: Option<Arc<dyn Fn(ProducerPoint) + Send + Sync>>,
    pub owned: crate::engine::connect_owned_run::DedicatedTestHooks,
    pub fail_thread: bool,
    pub fail_runtime: bool,
}
#[cfg(test)]
impl TestHooks {
    pub fn at(&self, point: ProducerPoint) {
        if let Some(hook) = &self.point {
            hook(point);
        }
    }
}
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(super) enum FixtureMode {
    Normal,
    HoldStop,
    IgnoreStop,
    BadProtocol,
    BadPong,
    StatusTimeout,
    TruncateFirstStatus,
    TruncateEveryStatus,
}
#[cfg(test)]
pub(super) struct TestRecipe {
    pub control: SocketAddr,
    pub nonce: Uuid,
    pub mode: FixtureMode,
}
#[cfg(test)]
pub(super) type TestLimits = Limits;
#[cfg(test)]
pub(super) fn plan_for_test(
    prepared: factory::PreparedStatus,
    fixture: TestRecipe,
    limits: TestLimits,
    hooks: Arc<TestHooks>,
) -> RunResult<DedicatedPlan> {
    if fixture.control.ip() != std::net::IpAddr::V4(Ipv4Addr::LOCALHOST)
        || fixture.control.port() == 0
        || !limits.valid()
    {
        return Err(DedicatedError::Preparation(Error::Resources));
    }
    let mut recipe = Recipe::production(&prepared)?;
    recipe.executable = std::env::current_exe().map_err(|_| DedicatedError::Internal)?;
    recipe.args = vec![
        "--ignored".into(),
        "--exact".into(),
        "engine::connect_voice::dedicated::tests::producer_fixture::dedicated_status_child_fixture"
            .into(),
        "--nocapture".into(),
        "--test-threads=1".into(),
    ];
    recipe.environment.insert(
        "AURORA_DEDICATED_TEST_NONCE".into(),
        fixture.nonce.to_string(),
    );
    recipe.environment.insert(
        "AURORA_DEDICATED_TEST_CONTROL".into(),
        fixture.control.to_string(),
    );
    recipe.environment.insert(
        "AURORA_DEDICATED_TEST_MODE".into(),
        match fixture.mode {
            FixtureMode::Normal => "normal",
            FixtureMode::HoldStop => "hold-stop",
            FixtureMode::IgnoreStop => "ignore-stop",
            FixtureMode::BadProtocol => "bad-protocol",
            FixtureMode::BadPong => "bad-pong",
            FixtureMode::StatusTimeout => "status-timeout",
            FixtureMode::TruncateFirstStatus => "truncate-first-status",
            FixtureMode::TruncateEveryStatus => "truncate-every-status",
        }
        .into(),
    );
    recipe.synthetic = true;
    recipe.limits = limits;
    make_plan(prepared, recipe, hooks)
}

#[cfg(test)]
pub(super) type V1StatusPlanCapture = (factory::PreparedStatus, Vec<u8>, Vec<u8>, Uuid, String);
#[cfg(test)]
pub(super) fn capture_status_plan_for_test(
    prepared: factory::PreparedStatus,
) -> RunResult<V1StatusPlanCapture> {
    // The real plan path validates Resources, original file review/deadline,
    // manifest/provenance and the actual compiled production recipe. No start.
    let plan = plan(prepared)?;
    let recipe = encode(&plan.approval.recipe)?;
    let input = execution_digest_input(
        plan.approval.review.review_id,
        &plan.prepared.bundle.manifest,
        &plan.approval.file_digest,
        &plan.approval.recipe,
    )?;
    let review_id = plan.approval.review.review_id;
    let digest = plan.approval.review.digest.clone();
    let DedicatedPlan {
        prepared,
        approval,
        hooks,
    } = plan;
    drop((approval, hooks));
    Ok((prepared, recipe, input, review_id, digest))
}

#[cfg(test)]
pub(super) fn replay_status_input_for_test(
    copy: &Path,
    jdk: &Path,
    manifest_bytes: &[u8],
    original_input: &[u8],
) -> Result<(Vec<u8>, Vec<u8>, Uuid)> {
    use serde::de::IgnoredAny;
    type OriginalContext = (u32, String, Uuid, IgnoredAny, String, IgnoredAny);
    let (schema, purpose, review_id, _, file_digest, _): OriginalContext =
        serde_json::from_slice(original_input).map_err(|_| Error::Identity)?;
    if schema != 1 || purpose != PURPOSE {
        return Err(Error::Identity);
    }
    let manifest = super::manifest::decode_manifest(manifest_bytes)?;
    manifest.status_v1()?;
    // These are the private Fixture's fixed paths. The captured recipe/argv is
    // ignored; java still comes only from newly verified Resources, and the
    // same private compiled builder is called by Recipe::production above.
    let lab = jdk.parent().ok_or(Error::UnsafePath)?.join("lab");
    let stage = lab.join("prepared-Test84");
    let resources = resources::Resources::inspect(&lab, &stage, jdk, resources::fixture_pins())?;
    let recipe = Recipe::production_paths(&resources, copy.to_path_buf())?;
    let encoded_recipe = encode(&recipe)?;
    let input = execution_digest_input(review_id, &manifest, &file_digest, &recipe)?;
    Ok((encoded_recipe, input, review_id))
}
