//! Owned ready-game worker and final native transport gate.
use super::*;
use crate::engine::connect::ready_game_backend::ReadyGameBackend;

use std::sync::{mpsc, Weak};

pub(super) struct GuestSource {
    guest: Weak<lab::Guest>,
    pub(super) bridge: Weak<lab::TcpBridge>,
    authority: GuestAuthority,
    deadline: Instant,
    cancel: CancellationToken,
}
enum GuestAuthority {
    Invitation(Box<Authority>),
    Social(Weak<social::ready_source::LaunchAuthority>),
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    SocialHost(Weak<social::ready_source::HostPlayerLaunchAuthority>),
}
impl GuestAuthority {
    fn with_current<T>(
        &self,
        work: impl FnOnce() -> Result<T, ReadyGameLaunchError>,
    ) -> Result<T, ReadyGameLaunchError> {
        match self {
            Self::Invitation(authority) => {
                authority
                    .check_session()
                    .map_err(|_| ReadyGameLaunchError::Unavailable)?;
                work()
            }
            #[cfg(all(windows, feature = "connect-voice-lab"))]
            Self::SocialHost(authority) => authority
                .upgrade()
                .ok_or(ReadyGameLaunchError::Unavailable)?
                .with_current(work),
            Self::Social(authority) => authority
                .upgrade()
                .ok_or(ReadyGameLaunchError::Unavailable)?
                .with_current(work),
        }
    }
    fn voice_profile(
        &self,
    ) -> Result<Option<crate::engine::connect_native_voice::NativeVoiceProfile>, ReadyGameLaunchError>
    {
        match self {
            Self::Invitation(_) => Ok(None),
            Self::Social(authority) => {
                let authority = authority
                    .upgrade()
                    .ok_or(ReadyGameLaunchError::Unavailable)?;
                authority.voice_profile()
            }
            #[cfg(all(windows, feature = "connect-voice-lab"))]
            Self::SocialHost(authority) => {
                let authority = authority
                    .upgrade()
                    .ok_or(ReadyGameLaunchError::Unavailable)?;
                authority.voice_profile()
            }
        }
    }
    fn runtime_profile(
        &self,
    ) -> Result<Option<crate::engine::connect_ready_profile::ReadyProfile>, ReadyGameLaunchError>
    {
        match self {
            Self::Invitation(_) => Ok(None),
            Self::Social(authority) => authority
                .upgrade()
                .ok_or(ReadyGameLaunchError::Unavailable)?
                .runtime_profile(),
            #[cfg(all(windows, feature = "connect-voice-lab"))]
            Self::SocialHost(authority) => authority
                .upgrade()
                .ok_or(ReadyGameLaunchError::Unavailable)?
                .runtime_profile(),
        }
    }
    fn check_epoch(&self, epoch: u64) -> Result<(), ReadyGameLaunchError> {
        match self {
            #[cfg(all(windows, feature = "connect-voice-lab"))]
            Self::SocialHost(authority)
                if authority
                    .upgrade()
                    .is_none_or(|authority| authority.account_epoch() != epoch) =>
            {
                Err(ReadyGameLaunchError::Unavailable)
            }
            Self::Social(authority)
                if authority
                    .upgrade()
                    .is_none_or(|authority| authority.account_epoch() != epoch) =>
            {
                Err(ReadyGameLaunchError::Unavailable)
            }
            _ => Ok(()),
        }
    }
}
impl GuestSource {
    fn plan_expired(&self, deadline: Instant, displayed_expiry: u64) -> bool {
        Instant::now() >= deadline
            || (matches!(&self.authority, GuestAuthority::Invitation(_))
                && lab::unix_millis() >= displayed_expiry)
    }

    fn displayed_session_expiry(&self, published_expiry: u64) -> u64 {
        if matches!(&self.authority, GuestAuthority::Invitation(_)) {
            published_expiry
        } else {
            displayed_deadline(self.deadline)
        }
    }

    pub(super) fn new(
        guest: &Arc<lab::Guest>,
        bridge: &Arc<lab::TcpBridge>,
        authority: &Authority,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Arc<Self> {
        Arc::new(Self {
            guest: Arc::downgrade(guest),
            bridge: Arc::downgrade(bridge),
            authority: GuestAuthority::Invitation(Box::new(authority.clone())),
            deadline,
            cancel: cancel.clone(),
        })
    }
    pub(super) fn social(
        guest: &Arc<lab::Guest>,
        bridge: &Arc<lab::TcpBridge>,
        authority: &Arc<social::ready_source::LaunchAuthority>,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Arc<Self> {
        Arc::new(Self {
            guest: Arc::downgrade(guest),
            bridge: Arc::downgrade(bridge),
            authority: GuestAuthority::Social(Arc::downgrade(authority)),
            deadline,
            cancel: cancel.clone(),
        })
    }
    #[cfg(all(windows, feature = "connect-voice-lab"))]
    pub(super) fn social_host(
        guest: &Arc<lab::Guest>,
        bridge: &Arc<lab::TcpBridge>,
        authority: &Arc<social::ready_source::HostPlayerLaunchAuthority>,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Arc<Self> {
        Arc::new(Self {
            guest: Arc::downgrade(guest),
            bridge: Arc::downgrade(bridge),
            authority: GuestAuthority::SocialHost(Arc::downgrade(authority)),
            deadline,
            cancel: cancel.clone(),
        })
    }
    fn allows_phase(&self, phase: &str) -> bool {
        #[cfg(all(windows, feature = "connect-voice-lab"))]
        if matches!(&self.authority, GuestAuthority::SocialHost(_)) {
            return phase == "hostListening";
        }
        phase == "guestConnected"
    }
    #[cfg(test)]
    pub(super) fn bound_sockets_for_test(&self) -> Vec<SocketAddr> {
        self.guest
            .upgrade()
            .map_or_else(Vec::new, |guest| guest.bound_sockets())
    }
    pub(super) fn path_observation(&self) -> Option<ConnectPathObservation> {
        let current = || {
            !self.cancel.is_cancelled()
                && Instant::now() < self.deadline
                && self.authority.with_current(|| Ok(())).is_ok()
        };
        if !current() {
            return None;
        }
        // Observe the original transport, independently of QuickPlay and the
        // TCP bridge. The temporary owner cannot escape this synchronous read.
        let guest = self.guest.upgrade()?;
        let observation = guest.path_observation();
        current().then(|| observation.into())
    }
    fn address(&self) -> Result<SocketAddr, ReadyGameLaunchError> {
        if self.cancel.is_cancelled()
            || Instant::now() >= self.deadline
            || self.authority.with_current(|| Ok(())).is_err()
        {
            return Err(ReadyGameLaunchError::Unavailable);
        }
        // A source or a blocked launch worker must never own an Endpoint.
        // These upgrades live only until this synchronous check returns.
        let guest = self
            .guest
            .upgrade()
            .ok_or(ReadyGameLaunchError::Unavailable)?;
        if guest.connection().close_reason().is_some() {
            return Err(ReadyGameLaunchError::Unavailable);
        }
        let bridge = self
            .bridge
            .upgrade()
            .ok_or(ReadyGameLaunchError::Unavailable)?;
        if bridge.is_closed() {
            return Err(ReadyGameLaunchError::Unavailable);
        }
        Ok(bridge.local_addr())
    }
    fn with_current<T>(
        &self,
        work: impl FnOnce() -> Result<T, ReadyGameLaunchError>,
    ) -> Result<T, ReadyGameLaunchError> {
        // The social guard must cover the actual spawn, not only an observation
        // immediately before it. Do not recursively lock it through address().
        self.authority.with_current(|| {
            if self.cancel.is_cancelled() || Instant::now() >= self.deadline {
                return Err(ReadyGameLaunchError::Unavailable);
            }
            let guest = self
                .guest
                .upgrade()
                .ok_or(ReadyGameLaunchError::Unavailable)?;
            let bridge = self
                .bridge
                .upgrade()
                .ok_or(ReadyGameLaunchError::Unavailable)?;
            if guest.connection().close_reason().is_some() || bridge.is_closed() {
                return Err(ReadyGameLaunchError::Unavailable);
            }
            work()
        })
    }
}

fn displayed_deadline(deadline: Instant) -> u64 {
    lab::unix_millis().saturating_add(
        deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64,
    )
}

pub(super) struct ReadyWork {
    snapshot: ReadyGameLaunchSnapshot,
    cancel: CancellationToken,
    confirm: mpsc::SyncSender<()>,
    worker: Option<JoinHandle<()>>,
    digest: Option<String>,
    deadline: Instant,
}
pub(super) fn finished(a: &Attempt) -> bool {
    a.ready_game
        .as_ref()
        .is_none_or(|work| work.worker.as_ref().is_none_or(JoinHandle::is_finished))
}
pub(super) fn cancel(a: &Attempt) {
    if let Some(work) = &a.ready_game {
        work.cancel.cancel();
    }
}
#[cfg(test)]
pub(super) fn expire_preparation_for_test(a: &mut Attempt) {
    let work = a.ready_game.as_mut().expect("owned test work");
    assert_eq!(work.snapshot.phase, "preparing");
    work.deadline = Instant::now();
}
pub(super) fn refresh_cleanup(a: &mut Attempt) {
    if finished(a)
        && group_source::finished(a)
        && host_source::finished(a)
        && host_review::finished(a)
        && !a.probe_active
        && a.worker.as_ref().is_none_or(JoinHandle::is_finished)
        && matches!(a.snapshot.phase, "closed" | "failed")
        && a.snapshot.cleanup_state == "pending"
    {
        a.snapshot.cleanup_state = "complete";
        a.snapshot.generation = a.snapshot.generation.saturating_add(1);
    }
}

/// Opaque source and operation identity captured only by an admitted Guest actor.
/// Tracking locks must be acquired BEFORE this gate (Running → IPC → Connect).
pub struct ReadyGameSpawnGate {
    state: Arc<Mutex<State>>,
    operation: String,
    plan: String,
    source: Arc<GuestSource>,
    cancel: CancellationToken,
    deadline: Instant,
    expires_at: u64,
    #[cfg(test)]
    epoch_rejected_before_spawn: std::sync::atomic::AtomicBool,
}
impl ReadyGameSpawnGate {
    #[cfg(test)]
    pub(super) fn epoch_rejected_for_test(&self) -> bool {
        self.epoch_rejected_before_spawn
            .load(std::sync::atomic::Ordering::SeqCst)
    }
    /// Read-only precommit gate. File IO must not execute while holding Connect;
    /// final child spawn still uses with_current and its registration barrier.
    pub fn check_current(&self) -> Result<(), ReadyGameLaunchError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ReadyGameLaunchError::Internal)?;
        self.validate(&state)?;
        self.with_live_plan(|| Ok(()))
    }
    fn validate(&self, state: &State) -> Result<(), ReadyGameLaunchError> {
        let a = state
            .attempt
            .as_ref()
            .ok_or(ReadyGameLaunchError::StaleOperation)?;
        let work = a
            .ready_game
            .as_ref()
            .ok_or(ReadyGameLaunchError::StaleOperation)?;
        if a.snapshot.operation_id.as_deref() != Some(&self.operation)
            || !self.source.allows_phase(a.snapshot.phase)
            || work.snapshot.plan_id.as_deref() != Some(&self.plan)
            || work.snapshot.phase != "starting"
            || a.guest_source
                .as_ref()
                .is_none_or(|source| !Arc::ptr_eq(source, &self.source))
            || a.cancel.is_cancelled()
            || self.cancel.is_cancelled()
        {
            return Err(ReadyGameLaunchError::Cancelled);
        }
        if self.source.plan_expired(self.deadline, self.expires_at) {
            return Err(ReadyGameLaunchError::Expired);
        }
        let epoch_check = self.source.authority.check_epoch(state.social_epoch);
        #[cfg(test)]
        let epoch_check = epoch_check.inspect_err(|_| {
            self.epoch_rejected_before_spawn
                .store(true, std::sync::atomic::Ordering::SeqCst);
        });
        epoch_check?;
        Ok(())
    }
    fn with_live_plan<T>(
        &self,
        work: impl FnOnce() -> Result<T, ReadyGameLaunchError>,
    ) -> Result<T, ReadyGameLaunchError> {
        self.source.with_current(|| {
            // Acquiring the original source guard may have consumed the rest
            // of the review window while the longer session remains current.
            if self.source.plan_expired(self.deadline, self.expires_at) {
                return Err(ReadyGameLaunchError::Expired);
            }
            work()
        })
    }
    pub fn with_current(
        &self,
        spawn_and_register: impl FnOnce() -> Result<ReadyGameLaunchResult, ReadyGameLaunchError>,
    ) -> Result<ReadyGameLaunchResult, ReadyGameLaunchError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ReadyGameLaunchError::Internal)?;
        self.validate(&state)?;
        let result = self.with_live_plan(spawn_and_register)?;
        let work = state
            .attempt
            .as_mut()
            .and_then(|a| a.ready_game.as_mut())
            .ok_or(ReadyGameLaunchError::Internal)?;
        work.snapshot.phase = "started";
        work.snapshot.result = Some(result.clone());
        work.snapshot.review = None;
        work.snapshot.error_code = None;
        work.snapshot.generation = work.snapshot.generation.saturating_add(1);
        Ok(result)
    }
}

fn selected_ready<'a>(
    state: &'a mut State,
    owner: &str,
    operation: &str,
) -> Result<&'a mut Attempt, ReadyGameLaunchError> {
    selected(state, owner, operation).map_err(|e| match e {
        ConnectError::OwnerMismatch => ReadyGameLaunchError::WrongOwner,
        _ => ReadyGameLaunchError::StaleOperation,
    })
}
fn update_ready(
    state: &Mutex<State>,
    operation: &str,
    plan: &str,
    change: impl FnOnce(&mut ReadyWork),
) {
    if let Ok(mut state) = state.lock() {
        if let Some(a) = state.attempt.as_mut() {
            if a.snapshot.operation_id.as_deref() == Some(operation) {
                if let Some(work) = &mut a.ready_game {
                    if work.snapshot.plan_id.as_deref() == Some(plan) {
                        change(work);
                        work.snapshot.generation = work.snapshot.generation.saturating_add(1);
                    }
                }
            }
        }
    }
}
fn finish_ready(
    state: &Mutex<State>,
    operation: &str,
    plan: &str,
    result: Result<ReadyGameLaunchResult, ReadyGameLaunchError>,
) {
    update_ready(state, operation, plan, |work| {
        if work.snapshot.phase == "started" {
            return;
        }
        let result = if matches!(result, Err(ReadyGameLaunchError::Cancelled))
            && Instant::now() >= work.deadline
        {
            Err(ReadyGameLaunchError::Expired)
        } else {
            result
        };
        work.snapshot.review = None;
        match result {
            Ok(result) => {
                work.snapshot.phase = "started";
                work.snapshot.result = Some(result);
                work.snapshot.error_code = None;
            }
            Err(error) => {
                work.snapshot.phase = match error {
                    ReadyGameLaunchError::Cancelled => "cancelled",
                    ReadyGameLaunchError::Expired => "expired",
                    _ => "failed",
                };
                work.snapshot.result = None;
                work.snapshot.error_code = Some(error.code());
            }
        }
    });
}

impl Manager {
    pub(in crate::engine::connect) fn authorize_ready_copy_install(
        &self,
        owner: &str,
        operation: &str,
        profile: crate::engine::connect_ready_profile::ReadyProfile,
    ) -> Result<(), ReadyGameLaunchError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ReadyGameLaunchError::Internal)?;
        if state.shutting_down {
            return Err(ReadyGameLaunchError::Cancelled);
        }
        let account_epoch = state.social_epoch;
        let attempt = selected_ready(&mut state, owner, operation)?;
        let source = attempt
            .guest_source
            .as_ref()
            .filter(|source| {
                source.allows_phase(attempt.snapshot.phase) && !attempt.cancel.is_cancelled()
            })
            .ok_or(ReadyGameLaunchError::Unavailable)?;
        source.authority.check_epoch(account_epoch)?;
        source.address()?;
        if source
            .authority
            .runtime_profile()?
            .is_some_and(|original| original != profile)
        {
            return Err(ReadyGameLaunchError::UnsupportedRuntime);
        }
        Ok(())
    }

    pub(in crate::engine::connect) fn prepare_ready_game(
        &self,
        owner: &str,
        request: ReadyGameLaunchRequest,
        backend: Arc<dyn ReadyGameBackend>,
    ) -> Result<ReadyGameLaunchSnapshot, ReadyGameLaunchError> {
        request.validate()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| ReadyGameLaunchError::Internal)?;
        if state.shutting_down {
            return Err(ReadyGameLaunchError::Cancelled);
        }
        let account_epoch = state.social_epoch;
        let a = selected_ready(&mut state, owner, &request.operation_id)?;
        if a.guest_source
            .as_ref()
            .is_none_or(|source| !source.allows_phase(a.snapshot.phase))
            || a.cancel.is_cancelled()
        {
            return Err(ReadyGameLaunchError::Unavailable);
        }
        if !finished(a) {
            return Err(ReadyGameLaunchError::Busy);
        }
        let source = a
            .guest_source
            .clone()
            .ok_or(ReadyGameLaunchError::Unavailable)?;
        source.authority.check_epoch(account_epoch)?;
        let endpoint = source.address()?;
        let plan = uuid::Uuid::new_v4().to_string();
        let cancel = a.cancel.child_token();
        let deadline = source
            .deadline
            .min(Instant::now() + Duration::from_secs(60));
        let published_expiry = a
            .snapshot
            .expires_at_ms
            .ok_or(ReadyGameLaunchError::Unavailable)?;
        let expires_at = source
            .displayed_session_expiry(published_expiry)
            .min(displayed_deadline(deadline));
        let (send, receiver) = mpsc::sync_channel(1);
        let generation = a
            .ready_game
            .as_ref()
            .map_or(1, |work| work.snapshot.generation.saturating_add(1));
        let snapshot = ReadyGameLaunchSnapshot {
            operation_id: request.operation_id.clone(),
            generation,
            plan_id: Some(plan.clone()),
            phase: "preparing",
            review: None,
            result: None,
            error_code: None,
        };
        a.ready_game = Some(ReadyWork {
            snapshot: snapshot.clone(),
            cancel: cancel.clone(),
            confirm: send,
            worker: None,
            digest: None,
            deadline,
        });
        let owned_state = self.state.clone();
        let operation = request.operation_id.clone();
        let owner = owner.to_owned();
        let monitor_state = owned_state.clone();
        let monitor_operation = operation.clone();
        let monitor_plan = plan.clone();
        let monitor_cancel = cancel.clone();
        let worker = tokio::spawn(async move {
            let mut blocking = tokio::task::spawn_blocking(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let check = || {
                        if source.plan_expired(deadline, expires_at) {
                            return Err(ReadyGameLaunchError::Expired);
                        }
                        if cancel.is_cancelled() {
                            return Err(ReadyGameLaunchError::Cancelled);
                        }
                        source.address().map(|_| ())
                    };
                    check()?;
                    let voice_profile = source.authority.voice_profile()?;
                    let runtime_profile = source.authority.runtime_profile()?;
                    let prepared = backend.prepare(
                        &request,
                        endpoint,
                        voice_profile,
                        runtime_profile,
                        &cancel,
                    )?;
                    if prepared.voice_review().map(|review| review.profile) != voice_profile
                        || (voice_profile.is_some() && runtime_profile.is_none())
                        || runtime_profile.is_some_and(|runtime| prepared.profile() != runtime)
                    {
                        return Err(ReadyGameLaunchError::Unavailable);
                    }
                    check()?;
                    // Preparation and approval are independently bounded, without
                    // replacing or extending the original session authority.
                    let deadline = source
                        .deadline
                        .min(Instant::now() + Duration::from_secs(60));
                    let session_expiry = source.displayed_session_expiry(published_expiry);
                    let expires_at = session_expiry.min(displayed_deadline(deadline));
                    let check = || {
                        if source.plan_expired(deadline, expires_at) {
                            return Err(ReadyGameLaunchError::Expired);
                        }
                        if cancel.is_cancelled() {
                            return Err(ReadyGameLaunchError::Cancelled);
                        }
                        source.address().map(|_| ())
                    };
                    check()?;
                    let runtime = prepared.profile().descriptor();
                    let material = serde_json::to_vec(&(
                        &owner,
                        &request,
                        &plan,
                        endpoint,
                        expires_at,
                        runtime,
                        prepared.fingerprint(),
                    ))
                    .map_err(|_| ReadyGameLaunchError::Internal)?;
                    let mut hash = Sha256::new();
                    if let Some(voice) = prepared.voice_review() {
                        hash.update(b"aurora-connect-ready-game-voice-v4\0");
                        hash.update(&material);
                        hash.update(
                            serde_json::to_vec(&(
                                voice_profile,
                                runtime_profile.map(|runtime| runtime.version_id()),
                                voice,
                            ))
                            .map_err(|_| ReadyGameLaunchError::Internal)?,
                        );
                    } else {
                        hash.update(b"aurora-connect-ready-game-v2\0");
                        hash.update(material);
                    }
                    let digest = format!("{:x}", hash.finalize());
                    let review = ReadyGameLaunchReview {
                        operation_id: operation.clone(),
                        plan_id: plan.clone(),
                        digest: digest.clone(),
                        expires_at_ms: expires_at,
                        session_expires_at_ms: session_expiry,
                        instance_id: request.instance_id.clone(),
                        source_instance_id: request.source_instance_id.clone(),
                        context_key: request.context_key.clone(),
                        minecraft_version: runtime.minecraft_version,
                        loader: runtime.loader,
                        loader_version: runtime.loader_version,
                        java_executable: request.java_executable.clone(),
                        java_major: runtime.java_major,
                        nickname: request.nickname.clone(),
                        appearance: request.appearance.clone(),
                        game_address: endpoint.to_string(),
                        core_version: runtime.core_version,
                        core_sha256: prepared.core_sha256().into(),
                        skins_version: runtime.skins_version,
                        skins_sha256: prepared.skins_sha256().into(),
                        voice: prepared.voice_review().cloned(),
                    };
                    update_ready(&owned_state, &operation, &plan, |work| {
                        work.deadline = deadline;
                        work.snapshot.phase = "reviewed";
                        work.snapshot.review = Some(review);
                        work.digest = Some(digest);
                    });
                    loop {
                        check()?;
                        match receiver.recv_timeout(Duration::from_millis(20)) {
                            Ok(()) => break,
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Err(_) => return Err(ReadyGameLaunchError::Cancelled),
                        }
                    }
                    check()?;
                    let gate = ReadyGameSpawnGate {
                        state: owned_state.clone(),
                        operation: operation.clone(),
                        plan: plan.clone(),
                        source: source.clone(),
                        cancel: cancel.clone(),
                        deadline,
                        expires_at,
                        #[cfg(test)]
                        epoch_rejected_before_spawn: std::sync::atomic::AtomicBool::new(false),
                    };
                    prepared.launch(&gate, &cancel)
                }))
                .unwrap_or(Err(ReadyGameLaunchError::Internal));
                finish_ready(&owned_state, &operation, &plan, result);
            });
            // The async owner never drops or aborts the blocking task. Deadline
            // and stop only signal its token, then await its registered outcome.
            loop {
                tokio::select! {
                    result = &mut blocking => {
                        if result.is_err() { finish_ready(&monitor_state, &monitor_operation, &monitor_plan, Err(ReadyGameLaunchError::Internal)); }
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {
                        let expired = monitor_state.lock().ok().and_then(|state| state.attempt.as_ref().and_then(|a| a.ready_game.as_ref().map(|work| Instant::now() >= work.deadline))).unwrap_or(true);
                        if expired { monitor_cancel.cancel(); }
                    }
                }
            }
        });
        a.ready_game.as_mut().expect("inserted").worker = Some(worker);
        Ok(snapshot)
    }
    pub(in crate::engine::connect) fn ready_game_snapshot(
        &self,
        owner: &str,
        operation: &str,
    ) -> Result<ReadyGameLaunchSnapshot, ReadyGameLaunchError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ReadyGameLaunchError::Internal)?;
        let a = selected_ready(&mut state, owner, operation)?;
        Ok(a.ready_game.as_ref().map_or_else(
            || ReadyGameLaunchSnapshot::idle(operation),
            |work| work.snapshot.clone(),
        ))
    }
    pub(in crate::engine::connect) fn confirm_ready_game(
        &self,
        owner: &str,
        operation: &str,
        plan: &str,
        digest: &str,
    ) -> Result<ReadyGameLaunchSnapshot, ReadyGameLaunchError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ReadyGameLaunchError::Internal)?;
        let account_epoch = state.social_epoch;
        let a = selected_ready(&mut state, owner, operation)?;
        let work = a
            .ready_game
            .as_mut()
            .ok_or(ReadyGameLaunchError::StaleOperation)?;
        if work.snapshot.plan_id.as_deref() != Some(plan) {
            return Err(ReadyGameLaunchError::StaleOperation);
        }
        if work.digest.as_deref() != Some(digest) {
            return Err(ReadyGameLaunchError::ApprovalMismatch);
        }
        if matches!(work.snapshot.phase, "starting" | "started") {
            return Ok(work.snapshot.clone());
        }
        if work.snapshot.phase != "reviewed" {
            return Err(ReadyGameLaunchError::Unavailable);
        }
        if work.cancel.is_cancelled() || a.cancel.is_cancelled() {
            return Err(ReadyGameLaunchError::Cancelled);
        }
        let source = a
            .guest_source
            .as_ref()
            .ok_or(ReadyGameLaunchError::Unavailable)?;
        if work
            .snapshot
            .review
            .as_ref()
            .is_none_or(|review| source.plan_expired(work.deadline, review.expires_at_ms))
        {
            work.cancel.cancel();
            return Err(ReadyGameLaunchError::Expired);
        }
        source.authority.check_epoch(account_epoch)?;
        source.address()?;
        work.snapshot.phase = "starting";
        work.snapshot.review = None;
        work.snapshot.generation = work.snapshot.generation.saturating_add(1);
        work.confirm
            .try_send(())
            .map_err(|_| ReadyGameLaunchError::Internal)?;
        Ok(work.snapshot.clone())
    }
    pub(in crate::engine::connect) async fn cancel_ready_game(
        &self,
        owner: &str,
        operation: &str,
        plan: &str,
    ) -> Result<ReadyGameLaunchSnapshot, ReadyGameLaunchError> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| ReadyGameLaunchError::Internal)?;
            let a = selected_ready(&mut state, owner, operation)?;
            let work = a
                .ready_game
                .as_ref()
                .ok_or(ReadyGameLaunchError::StaleOperation)?;
            if work.snapshot.plan_id.as_deref() != Some(plan) {
                return Err(ReadyGameLaunchError::StaleOperation);
            }
            work.cancel.cancel();
        }
        loop {
            {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| ReadyGameLaunchError::Internal)?;
                let a = selected_ready(&mut state, owner, operation)?;
                if a.ready_game
                    .as_ref()
                    .is_none_or(|work| work.snapshot.plan_id.as_deref() != Some(plan))
                {
                    return Err(ReadyGameLaunchError::StaleOperation);
                }
                if finished(a) {
                    return Ok(a.ready_game.as_ref().expect("owned work").snapshot.clone());
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
