//! Ready-only desktop launch policy. It never calls the normal install/migrate
//! launcher pipeline and owns no public endpoint/argv IPC input.
use super::*;
use aurora_launcher_core::engine::{
    connect::{
        PreparedReadyGame, ReadyGameBackend, ReadyGameLaunchError as Error, ReadyGameLaunchRequest,
        ReadyGameLaunchResult, ReadyGameSpawnGate,
    },
    connect_java_probe::probe_java_major,
    connect_ready_profile::{select_ready_profile, ReadyProfile},
    connect_ready_runtime::{inspect_ready_runtime, ReadyRuntime},
};
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;

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
fn inspect(
    app: &AppHandle,
    request: &ReadyGameLaunchRequest,
    endpoint: SocketAddr,
    cancel: &CancellationToken,
) -> Result<
    (
        aurora_launcher_core::engine::Instance,
        InstanceMutationGuard,
        ReadyRuntime,
        JavaRuntime,
        Option<PathBuf>,
    ),
    Error,
> {
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
    let guard = acquire_idle_instance(&instance).map_err(|_| Error::InstanceBusy)?;
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
    let proof = inspect_ready_runtime(
        &instance,
        &guard,
        &request.source_instance_id,
        &java_path,
        skin.as_deref(),
        endpoint,
        selected_profile.descriptor().java_major,
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
    check_cancel(cancel)?;
    Ok((instance, guard, proof, java, skin))
}
impl ReadyGameBackend for DesktopReadyGameBackend {
    fn prepare(
        &self,
        request: &ReadyGameLaunchRequest,
        endpoint: SocketAddr,
        cancel: &CancellationToken,
    ) -> Result<Box<dyn PreparedReadyGame>, Error> {
        let (_, _, proof, _, _) = inspect(&self.app, request, endpoint, cancel)?;
        Ok(Box::new(DesktopPrepared {
            app: self.app.clone(),
            request: request.clone(),
            endpoint,
            fingerprint: proof.fingerprint.clone(),
            core: proof.core_sha256.clone(),
            skins: proof.skins_sha256.clone(),
            profile: proof.profile,
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
    fn launch(
        self: Box<Self>,
        gate: &ReadyGameSpawnGate,
        cancel: &CancellationToken,
    ) -> Result<ReadyGameLaunchResult, Error> {
        let (instance, mutation_lock, proof, java, skin) =
            inspect(&self.app, &self.request, self.endpoint, cancel)?;
        if proof.profile != self.profile
            || proof.fingerprint != self.fingerprint
            || proof.core_sha256 != self.core
            || proof.skins_sha256 != self.skins
        {
            return Err(Error::InstanceChanged);
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
            .prepare_launch(&instance, &java, &identity, &spec, 4096)
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
            max_memory_mb: 4096,
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
