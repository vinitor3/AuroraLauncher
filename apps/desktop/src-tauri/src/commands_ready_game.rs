//! Ready-only desktop launch policy. It never calls the normal install/migrate
//! launcher pipeline and owns no public endpoint/argv IPC input.
use super::*;
use aurora_launcher_core::engine::{
    connect::{
        PreparedReadyGame, ReadyGameBackend, ReadyGameLaunchError as Error, ReadyGameLaunchRequest,
        ReadyGameLaunchResult, ReadyGameSpawnGate, ReadyVoiceReview,
    },
    connect_java_probe::probe_java_major,
    connect_native_voice::NativeVoiceProfile,
    connect_ready_profile::{select_ready_profile, ReadyProfile},
    connect_ready_runtime::{inspect_ready_runtime_for_voice, ReadyRuntime, ReadyVoiceInstall},
    connect_status_profile::protocol_for_ready_profile,
    launch_settings::{inspect_ready_settings, ReadyLaunchSettings},
};
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use tokio_util::sync::CancellationToken;
#[path = "commands_ready_game_progress.rs"]
mod copy_progress;

struct CopyInstall {
    id: uuid::Uuid,
    cancel: Arc<AtomicBool>,
}
static COPY_INSTALL: Mutex<Option<CopyInstall>> = Mutex::new(None);
static COPY_INSTALL_SHUTDOWN: AtomicBool = AtomicBool::new(false);
static COPY_INSTALL_ACCOUNT_BLOCKS: AtomicUsize = AtomicUsize::new(0);

struct CopyInstallOwner {
    id: uuid::Uuid,
    watchdog: tokio::task::JoinHandle<()>,
}
impl Drop for CopyInstallOwner {
    fn drop(&mut self) {
        self.watchdog.abort();
        if let Ok(mut active) = COPY_INSTALL.lock() {
            if active.as_ref().is_some_and(|job| job.id == self.id) {
                *active = None;
            }
        }
    }
}

fn copy_install_id(value: &str) -> Result<uuid::Uuid, &'static str> {
    let id = uuid::Uuid::parse_str(value).map_err(|_| "connect_ready_game_invalid_request")?;
    if id.to_string() != value || id.is_nil() {
        return Err("connect_ready_game_invalid_request");
    }
    Ok(id)
}

pub(crate) fn cancel_ready_copy_install(id: Option<&str>) -> Result<(), &'static str> {
    let id = id.map(copy_install_id).transpose()?;
    let job = COPY_INSTALL
        .lock()
        .map_err(|_| "connect_ready_game_internal")?;
    if let Some(job) = job.as_ref().filter(|job| id.is_none_or(|id| id == job.id)) {
        job.cancel.store(true, Ordering::Release);
    }
    Ok(())
}

pub(crate) async fn shutdown_ready_copy_install() {
    COPY_INSTALL_SHUTDOWN.store(true, Ordering::Release);
    let _ = cancel_ready_copy_install(None);
    wait_ready_copy_install().await;
}

pub(crate) struct CopyInstallAccountTransition;
impl CopyInstallAccountTransition {
    pub(crate) fn begin() -> Self {
        COPY_INSTALL_ACCOUNT_BLOCKS.fetch_add(1, Ordering::AcqRel);
        let _ = cancel_ready_copy_install(None);
        Self
    }
}
impl Drop for CopyInstallAccountTransition {
    fn drop(&mut self) {
        COPY_INSTALL_ACCOUNT_BLOCKS.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(crate) async fn wait_ready_copy_install() {
    while COPY_INSTALL.lock().is_ok_and(|active| active.is_some()) {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

pub(crate) async fn install_ready_copy(
    app: AppHandle,
    connect: &aurora_launcher_core::engine::connect::ConnectManager,
    owner: &str,
    request: crate::connect_commands::ReadyCopyInstallRequest,
) -> Result<diagnostic_lab::LabEnvironment, &'static str> {
    if !request.approved {
        return Err("connect_ready_game_approval_mismatch");
    }
    let operation = copy_install_id(&request.operation_id)?;
    let source = InstanceId::parse(&request.source_instance_id)
        .map_err(|_| "connect_ready_game_invalid_request")?;
    let profile = ReadyProfile::from_version_id(&request.profile_id)
        .ok_or("connect_ready_game_unsupported_runtime")?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active = COPY_INSTALL
            .lock()
            .map_err(|_| "connect_ready_game_internal")?;
        if COPY_INSTALL_SHUTDOWN.load(Ordering::Acquire)
            || COPY_INSTALL_ACCOUNT_BLOCKS.load(Ordering::Acquire) != 0
        {
            return Err("connect_ready_game_cancelled");
        }
        if active.is_some() {
            return Err("connect_ready_game_busy");
        }
        // Register against the original native connection while holding the
        // install lock also used by the account transition cancellation.
        connect
            .authorize_ready_copy_install(owner, &request.connect_operation_id, profile)
            .map_err(Error::code)?;
        *active = Some(CopyInstall {
            id: operation,
            cancel: cancel.clone(),
        });
    }
    let deadline_token = cancel.clone();
    let watchdog = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20 * 60)).await;
        deadline_token.store(true, Ordering::Release);
    });
    let owner = CopyInstallOwner {
        id: operation,
        watchdog,
    };
    let mut install_progress = copy_progress::CopyInstallProgress::new(
        operation,
        cancel.clone(),
        copy_progress::emitter(app.clone()),
    );
    tauri::async_runtime::spawn_blocking(move || {
        let _owner = owner;
        let cancelled = || cancel.load(Ordering::Acquire);
        let check = || {
            if cancelled() {
                Err("connect_ready_game_cancelled")
            } else {
                Ok(())
            }
        };
        let install_error = || {
            if cancelled() {
                "connect_ready_game_cancelled"
            } else {
                "connect_ready_copy_install_failed"
            }
        };
        check()?;
        let running = app.state::<RunningInstances>();
        if instance_is_running(&running, source.as_str())
            .map_err(|_| "connect_ready_game_instance_busy")?
        {
            return Err("connect_ready_game_instance_busy");
        }
        let source = layout(&app)
            .map_err(|_| "connect_ready_game_instance_not_ready")?
            .open(source);
        ensure_no_external_instance_game(source.root())
            .map_err(|_| "connect_ready_game_instance_busy")?;
        let guard =
            acquire_idle_instance(&source).map_err(|_| "connect_ready_game_instance_busy")?;
        ensure_no_external_instance_game(source.root())
            .map_err(|_| "connect_ready_game_instance_busy")?;
        check()?;
        let pending =
            diagnostic_lab::begin_runtime_copy(&source, &guard).map_err(|_| install_error())?;
        drop(guard);
        let progress = |transfer: TransferProgress| install_progress.transfer(transfer);
        let java =
            MinecraftInstaller::prepare_ready_copy_java(&pending, profile, cancelled, |item| {
                progress(item.map_total(0.0, 15.0))
            })
            .map_err(|_| install_error())?;
        check()?;
        if probe_java_major(&java, cancelled).map_err(|_| install_error())?
            != profile.descriptor().java_major
        {
            return Err("connect_ready_game_java_incompatible");
        }
        let installed = MinecraftInstaller::install_ready_copy(
            &pending,
            profile,
            &java,
            cancel.clone(),
            |item| progress(item.map_total(15.0, 98.0)),
        )
        .map_err(|_| install_error())?;
        check()?;
        let instance = pending.instance();
        let runtime = profile.descriptor();
        install_embedded_core(instance, runtime.minecraft_version, runtime.loader)
            .map_err(|_| install_error())?;
        install_embedded_skins(instance, runtime.minecraft_version, runtime.loader)
            .map_err(|_| install_error())?;
        write_instance_profile(instance, &installed, runtime.loader)
            .map_err(|_| install_error())?;
        let verified =
            aurora_launcher_core::engine::connect_ready_runtime::verify_installed_copy_runtime(
                instance, profile, &java, cancelled,
            )
            .map_err(Error::code)?;
        check()?;
        let copy = pending
            .publish(verified, cancelled)
            .map_err(|_| install_error())?;
        install_progress.complete();
        Ok(copy)
    })
    .await
    .map_err(|_| "connect_ready_copy_install_failed")?
}

pub(crate) struct DesktopReadyGameBackend {
    app: AppHandle,
}
impl DesktopReadyGameBackend {
    pub(crate) fn new(app: AppHandle) -> Self {
        Self { app }
    }
}
struct DesktopPrepared {
    app: AppHandle,
    request: ReadyGameLaunchRequest,
    endpoint: SocketAddr,
    fingerprint: String,
    core: String,
    skins: String,
    profile: ReadyProfile,
    voice_install: Option<ReadyVoiceInstall>,
}

fn check_cancel(cancel: &CancellationToken) -> Result<(), Error> {
    if cancel.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn appearance(app: &AppHandle, request: &ReadyGameLaunchRequest) -> Result<Option<PathBuf>, Error> {
    for value in [&request.appearance.skin_url, &request.appearance.cape_url]
        .into_iter()
        .flatten()
    {
        if public_profile_url(Some(value)) != Some(value.as_str()) {
            return Err(Error::InvalidRequest);
        }
    }
    let skin = request
        .appearance
        .skin_file
        .as_deref()
        .map(|path| local_appearance_path(app, Some(path), "skin").ok_or(Error::InvalidRequest))
        .transpose()?;
    if let Some(path) = &skin {
        doctor_runs::check_path(path).map_err(|_| Error::InvalidRequest)?;
    }
    Ok(skin)
}
struct ReadyGameInspection {
    instance: aurora_launcher_core::engine::Instance,
    mutation_lock: InstanceMutationGuard,
    proof: ReadyRuntime,
    java: JavaRuntime,
    skin: Option<PathBuf>,
    settings: ReadyLaunchSettings,
}

fn inspect(
    app: &AppHandle,
    request: &ReadyGameLaunchRequest,
    endpoint: SocketAddr,
    voice_profile: Option<NativeVoiceProfile>,
    runtime_profile: Option<ReadyProfile>,
    cancel: &CancellationToken,
) -> Result<ReadyGameInspection, Error> {
    check_cancel(cancel)?;
    validate_nickname(&request.nickname).map_err(|_| Error::InvalidRequest)?;
    let id = InstanceId::parse(&request.instance_id).map_err(|_| Error::InvalidRequest)?;
    let running = app.state::<RunningInstances>();
    // Preparation is read-only: normal UI polling owns pruning/Doctor updates.
    if running
        .0
        .lock()
        .map_err(|_| Error::InstanceBusy)?
        .values()
        .any(|process| process.instance_id == id.as_str())
    {
        return Err(Error::InstanceBusy);
    }
    let instance = layout(app).map_err(|_| Error::InstanceNotReady)?.open(id);
    ensure_no_external_instance_game(instance.root()).map_err(|_| Error::InstanceBusy)?;
    let guard = acquire_instance_mutation_lock(&instance).map_err(|error| match error {
        aurora_launcher_core::engine::ContentUpdateError::RecoveryPending => Error::RecoveryPending,
        _ => Error::InstanceBusy,
    })?;
    ensure_no_external_instance_game(instance.root()).map_err(|_| Error::InstanceBusy)?;
    let java_path = PathBuf::from(&request.java_executable);
    if java_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_none_or(|name| !name.eq_ignore_ascii_case("java.exe"))
    {
        return Err(Error::JavaUnavailable);
    }
    let skin = appearance(app, request)?;
    // First verify the installed runtime shape and pin the executable/runtime
    // files, then run the bounded -version probe while those pins are retained.
    let selected_profile = select_ready_profile(&instance)?;
    if runtime_profile.is_some_and(|expected| expected != selected_profile) {
        return Err(Error::UnsupportedRuntime);
    }
    let proof = inspect_ready_runtime_for_voice(
        &instance,
        &guard,
        &request.source_instance_id,
        &java_path,
        skin.as_deref(),
        endpoint,
        selected_profile.descriptor().java_major,
        voice_profile,
        || cancel.is_cancelled(),
    )?;
    check_cancel(cancel)?;
    let observed = probe_java_major(&java_path, || cancel.is_cancelled()).map_err(|_| {
        if cancel.is_cancelled() {
            Error::Cancelled
        } else {
            Error::JavaUnavailable
        }
    })?;
    if observed != proof.profile.descriptor().java_major {
        return Err(Error::JavaIncompatible);
    }
    let java = JavaRuntime::from_executable(java_path).map_err(|_| Error::JavaUnavailable)?;
    // Preferences belong to this independent copy. Never import or overwrite
    // the source instance's settings during review or confirmation.
    let additional_mods = proof.voice_review.as_ref().map_or(0, |review| {
        review
            .mods
            .iter()
            .filter(|item| {
                item.action == aurora_launcher_core::engine::connect::ReadyVoiceAction::Install
            })
            .count() as u32
    });
    let settings = inspect_ready_settings(&instance, &guard, observed, additional_mods)
        .map_err(|_| Error::InstanceNotReady)?;
    check_cancel(cancel)?;
    Ok(ReadyGameInspection {
        instance,
        mutation_lock: guard,
        proof,
        java,
        skin,
        settings,
    })
}

fn check_server_status(
    endpoint: SocketAddr,
    profile: ReadyProfile,
    cancel: &CancellationToken,
) -> Result<(), Error> {
    use aurora_connect_lab as lab;
    let protocol = protocol_for_ready_profile(profile).ok_or(Error::UnsupportedRuntime)?;
    let runtime = tokio::runtime::Handle::try_current().map_err(|_| Error::Internal)?;
    let summary = runtime.block_on(async {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(Error::Cancelled),
            result = lab::probe_minecraft_status(lab::MinecraftStatusRequest {
                endpoint,
                handshake_host: endpoint.ip().to_string(),
                handshake_port: endpoint.port(),
                protocol,
                timeout: lab::MAX_MINECRAFT_STATUS_TIMEOUT,
            }) => result.map_err(|_| Error::ServerStatusUnavailable)
        }
    })?;
    check_cancel(cancel)?;
    if !summary.protocol_matches_requested {
        return Err(Error::ServerProtocolMismatch);
    }
    Ok(())
}

impl ReadyGameBackend for DesktopReadyGameBackend {
    fn prepare(
        &self,
        request: &ReadyGameLaunchRequest,
        endpoint: SocketAddr,
        voice_profile: Option<NativeVoiceProfile>,
        runtime_profile: Option<ReadyProfile>,
        cancel: &CancellationToken,
    ) -> Result<Box<dyn PreparedReadyGame>, Error> {
        if voice_profile.is_some() && runtime_profile.is_none() {
            return Err(Error::Unavailable);
        }
        let ReadyGameInspection {
            mutation_lock: _guard,
            proof,
            settings,
            ..
        } = inspect(
            &self.app,
            request,
            endpoint,
            voice_profile,
            runtime_profile,
            cancel,
        )?;
        check_server_status(endpoint, proof.profile, cancel)?;
        let voice_install = if voice_profile.is_some() {
            let data = self
                .app
                .path()
                .app_local_data_dir()
                .map_err(|_| Error::VoiceArtifactsUnavailable)?;
            proof.prepare_voice_install(&data, || cancel.is_cancelled())?
        } else {
            None
        };
        settings
            .recheck_before_spawn()
            .map_err(|_| Error::InstanceChanged)?;
        Ok(Box::new(DesktopPrepared {
            app: self.app.clone(),
            request: request.clone(),
            endpoint,
            fingerprint: settings.bind_runtime_fingerprint(&proof.fingerprint),
            core: proof.core_sha256.clone(),
            skins: proof.skins_sha256.clone(),
            profile: proof.profile,
            voice_install,
        }))
    }
}
impl PreparedReadyGame for DesktopPrepared {
    fn profile(&self) -> ReadyProfile {
        self.profile
    }
    fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    fn core_sha256(&self) -> &str {
        &self.core
    }
    fn skins_sha256(&self) -> &str {
        &self.skins
    }
    fn voice_review(&self) -> Option<&ReadyVoiceReview> {
        self.voice_install.as_ref().map(ReadyVoiceInstall::review)
    }
    fn launch(
        self: Box<Self>,
        gate: &ReadyGameSpawnGate,
        cancel: &CancellationToken,
    ) -> Result<ReadyGameLaunchResult, Error> {
        let voice_profile = self.voice_review().map(|review| review.profile);
        let ReadyGameInspection {
            instance,
            mutation_lock,
            mut proof,
            java,
            skin,
            settings,
        } = inspect(
            &self.app,
            &self.request,
            self.endpoint,
            voice_profile,
            Some(self.profile),
            cancel,
        )?;
        if proof.profile != self.profile
            || settings.bind_runtime_fingerprint(&proof.fingerprint) != self.fingerprint
            || proof.core_sha256 != self.core
            || proof.skins_sha256 != self.skins
            || proof.voice_review.as_ref() != self.voice_review()
        {
            return Err(Error::InstanceChanged);
        }
        check_server_status(self.endpoint, proof.profile, cancel)?;
        gate.check_current()?;
        if let Some(voice) = &self.voice_install {
            ensure_no_external_instance_game(instance.root()).map_err(|_| Error::InstanceBusy)?;
            voice.commit(&mut proof, &instance, &mutation_lock, || {
                !cancel.is_cancelled() && gate.check_current().is_ok()
            })?;
        }
        let mut spec = proof.spec.clone();
        let uuid =
            offline_uuid_for_nickname(&self.request.nickname).map_err(|_| Error::InvalidRequest)?;
        let identity = LaunchIdentity::offline(self.request.nickname.clone(), uuid);
        let server = IpcServer::start_with_session(Some(IpcSessionProfile::offline(
            uuid,
            self.request.nickname.clone(),
        )))
        .map_err(|_| Error::LaunchFailed)?;
        spec.aurora_ipc_port = Some(server.endpoint().port);
        spec.aurora_session_nonce = Some(server.endpoint().nonce.clone());
        spec.jvm_args.push(format!(
            "-Daurora.loader={}",
            proof.profile.descriptor().loader
        ));
        let skins_root = data_directory(&self.app)
            .map_err(|_| Error::LaunchFailed)?
            .join("skins")
            .join(uuid.to_string());
        spec.jvm_args
            .push(format!("-Daurora.skins.root={}", skins_root.display()));
        if let Some(url) = &self.request.appearance.skin_url {
            spec.jvm_args
                .push(format!("-Daurora.profile.skinUrl={url}"));
        }
        if let Some(path) = skin {
            spec.jvm_args.push(format!(
                "-Daurora.profile.skinFile={}",
                java_property_path(&path)
            ));
        }
        if let Some(url) = &self.request.appearance.cape_url {
            spec.jvm_args
                .push(format!("-Daurora.profile.capeUrl={url}"));
        }
        if let Some(model) = &self.request.appearance.skin_model {
            spec.jvm_args
                .push(format!("-Daurora.profile.skinModel={model}"));
        }
        let prepared = LauncherEngine
            .prepare_launch_with_settings(&instance, &java, &identity, &spec, settings.effective())
            .map_err(|_| Error::LaunchFailed)?;
        check_cancel(cancel)?;
        let directory = doctor_directory(&self.app).map_err(|_| Error::LaunchFailed)?;
        let records = self.app.state::<DoctorRuns>();
        let instance_id = instance.id().as_str().to_owned();
        if records
            .read(&directory, &instance_id)
            .map_err(|_| Error::LaunchFailed)?
            .is_some_and(|run| matches!(run.outcome.as_str(), "preparing" | "running"))
        {
            return Err(Error::InstanceBusy);
        }
        let mut run = DoctorRun {
            run_id: uuid::Uuid::new_v4().to_string(),
            started_at_ms: doctor_runs::now_ms(),
            ended_at_ms: None,
            outcome: "preparing".into(),
            exit_code: None,
            version_id: spec.version_name.clone(),
            minecraft_version: proof.profile.descriptor().minecraft_version.into(),
            java_executable: self.request.java_executable.clone(),
            java_major: Some(proof.profile.descriptor().java_major),
            max_memory_mb: settings.effective().max_memory_mb,
            failure: None,
            logs_before: snapshot_logs(&instance),
        };
        records
            .start(&directory, &instance_id, run.clone())
            .map_err(|_| Error::LaunchFailed)?;
        let mut owned_server = Some(server);
        // Windows may reuse a PID whose completed game's IPC entry remains.
        // Retain that replaced server until all map and Connect guards leave:
        // its Drop joins the IPC thread and must not hold those mutexes.
        let mut replaced_server = None;
        let result = (|| {
            let _ = doctor_recovery::record_observation(
                &instance,
                &mutation_lock,
                &run.run_id,
                "launch",
            );
            check_cancel(cancel)?;
            // Log creation is outside the Connect mutex. Only OS spawn and
            // immediate insertion into already locked maps occur inside it.
            let mut command = prepared
                .into_connect_command()
                .map_err(|_| Error::LaunchFailed)?;
            let running = self.app.state::<RunningInstances>();
            let sessions = self.app.state::<IpcSessions>();
            let mut processes = running.0.lock().map_err(|_| Error::Internal)?;
            prune_finished_processes(&mut processes);
            if processes.values().any(|p| p.instance_id == instance_id) {
                return Err(Error::InstanceBusy);
            }
            let mut ipc_sessions = sessions.0.lock().map_err(|_| Error::Internal)?;
            ensure_no_external_instance_game(instance.root()).map_err(|_| Error::InstanceBusy)?;
            check_cancel(cancel)?;
            proof.recheck_before_spawn()?;
            settings
                .recheck_before_spawn()
                .map_err(|_| Error::InstanceChanged)?;
            let run_id = run.run_id.clone();
            let status_runtime =
                aurora_launcher_core::engine::connect_status_profile::inspect_launch_profile(
                    &instance,
                    &proof.spec,
                );
            let result_instance = instance_id.clone();
            let doctor_app = self.app.clone();
            let launched = gate.with_current(|| {
                let child = command.spawn().map_err(|_| Error::LaunchFailed)?;
                let process_id = child.id();
                // Failed spawn leaves the IPC server outside the Connect gate,
                // so its bounded shutdown/join never holds the Connect mutex.
                replaced_server = ipc_sessions.insert(
                    process_id,
                    owned_server.take().expect("single approved spawn"),
                );
                processes.insert(
                    process_id,
                    RunningInstance {
                        doctor: Some((doctor_app, run_id.clone())),
                        instance_id: result_instance.clone(),
                        child,
                        status_runtime,
                        _mutation_lock: mutation_lock,
                    },
                );
                Ok(ReadyGameLaunchResult {
                    run_id,
                    process_id,
                    instance_id: result_instance,
                    world_state: "notVerified",
                })
            })?;
            // Keep the existing RunningInstances guard while publishing the
            // Doctor running transition. A very short child cannot be pruned
            // and marked ended before this write, then resurrected by it.
            run.outcome = "running".into();
            let _ = records.save(&directory, &instance_id, run.clone());
            Ok(launched)
        })();
        drop(replaced_server);
        match &result {
            Ok(_) => {
                emit_doctor_run(&self.app, &instance_id, &run.run_id, "running");
            }
            Err(_) => {
                let _ = records.finish(
                    &directory,
                    &instance_id,
                    &run.run_id,
                    "failed",
                    None,
                    Some("Não foi possível iniciar a cópia pronta."),
                );
                emit_doctor_run(&self.app, &instance_id, &run.run_id, "failed");
            }
        }
        // Retain all immutable file pins through the actual spawn gate.
        drop(proof);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
    };

    fn read_frame(stream: &mut TcpStream) -> Vec<u8> {
        let mut length = [0];
        stream.read_exact(&mut length).unwrap();
        assert!(length[0] < 128);
        let mut body = vec![0; usize::from(length[0])];
        stream.read_exact(&mut body).unwrap();
        body
    }

    #[tokio::test]
    async fn ready_game_status_queries_the_exact_bridge_and_refuses_mismatched_protocol() {
        for reported in [767, 763] {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let endpoint = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let server = std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && std::time::Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("status fixture accept failed: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let hello = read_frame(&mut stream);
                assert_eq!(&hello[..3], &[0, 0xff, 0x05]);
                assert_eq!(&hello[4..hello.len() - 3], b"127.0.0.1");
                assert_eq!(
                    &hello[hello.len() - 3..hello.len() - 1],
                    &endpoint.port().to_be_bytes()
                );
                assert_eq!(read_frame(&mut stream), [0]);
                let body = format!(
                    r#"{{"version":{{"name":"fixture","protocol":{reported}}},"description":"fixture"}}"#
                );
                let mut status = vec![(body.len() + 2) as u8, 0, body.len() as u8];
                status.extend_from_slice(body.as_bytes());
                stream.write_all(&status).unwrap();
                let ping = read_frame(&mut stream);
                assert_eq!((ping[0], ping.len()), (1, 9));
                stream.write_all(&[9]).unwrap();
                stream.write_all(&ping).unwrap();
            });
            let observed = tokio::task::spawn_blocking(move || {
                check_server_status(
                    endpoint,
                    ReadyProfile::Fabric1211,
                    &CancellationToken::new(),
                )
            })
            .await
            .unwrap();
            server.join().unwrap();
            if reported == 767 {
                assert_eq!(observed, Ok(()));
            } else {
                assert_eq!(observed, Err(Error::ServerProtocolMismatch));
            }
        }
    }
}
