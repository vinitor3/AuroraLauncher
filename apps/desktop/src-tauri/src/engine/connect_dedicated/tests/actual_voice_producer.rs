//! Separate, explicitly reviewed opt-ins for real native SVC and Plasmo runs.
//! Only normal compiled factories/recipes are used. These observations do not
//! establish socket authorship by a mod, client audio, world entry or routing.
use super::*;
use crate::engine::{acquire_instance_recovery_lock, connect_owned_run::OwnedRunReason};
use aurora_connect_lab::{probe_minecraft_status, MinecraftStatusError, MinecraftStatusRequest};
use producer::{
    DedicatedCompletion, DedicatedError, DedicatedObserver, DedicatedPhase, DedicatedReason,
    DedicatedReport, DedicatedRun, DedicatedSource, DedicatedVoiceSource, EulaDisposition,
    VoiceRuntimeAnnouncement, VoiceRuntimeBinding, VoiceRuntimeError, VoiceRuntimeSample,
};
use std::{
    net::{Ipv4Addr, SocketAddr, TcpListener, UdpSocket},
    os::windows::fs::OpenOptionsExt,
    panic::{catch_unwind, AssertUnwindSafe},
    thread,
};
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

const COMMON: [&str; 9] = [
    "server.jar",
    "libraries/asm-9.8.jar",
    "libraries/asm-analysis-9.8.jar",
    "libraries/asm-commons-9.8.jar",
    "libraries/asm-tree-9.8.jar",
    "libraries/asm-util-9.8.jar",
    "libraries/sponge-mixin-0.15.5+mixin.0.8.7.jar",
    "libraries/intermediary-1.21.1.jar",
    "libraries/fabric-loader-0.16.14.jar",
];

fn extras(profile: NativeVoiceProfile) -> &'static [&'static str] {
    match profile {
        NativeVoiceProfile::SimpleVoiceChat => &["mods/voicechat-fabric-1.21.1-2.6.22.jar"],
        NativeVoiceProfile::PlasmoVoice => &[
            "mods/plasmovoice-fabric-1.21.1-2.1.17.jar",
            "mods/fabric-api-0.116.10+1.21.1.jar",
        ],
    }
}
fn endpoint(profile: NativeVoiceProfile) -> SocketAddr {
    SocketAddr::from((
        Ipv4Addr::LOCALHOST,
        match profile {
            NativeVoiceProfile::SimpleVoiceChat => 43002,
            NativeVoiceProfile::PlasmoVoice => 43003,
        },
    ))
}
fn artifacts(root: &Path, profile: NativeVoiceProfile) -> Result<Vec<(String, Observation)>> {
    COMMON
        .iter()
        .copied()
        .chain(extras(profile).iter().copied())
        .map(|name| {
            Ok((
                name.into(),
                observe_file(&root.join(name), 64 * 1024 * 1024)?.0,
            ))
        })
        .collect()
}
fn selected_resources(
    stage: &Path,
    jdk: &Path,
    profile: NativeVoiceProfile,
) -> Result<Vec<(String, Observation)>> {
    let mut selected = artifacts(stage, profile)?;
    for (name, path, maximum) in [
        ("jdk/release", jdk.join("release"), 64 * 1024),
        ("jdk/java.exe", jdk.join("bin/java.exe"), 4 * 1024 * 1024),
    ] {
        selected.push((name.into(), observe_file(&path, maximum)?.0));
    }
    Ok(selected)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExclusiveReadCheck {
    logical_name: String,
    exclusive_read: bool,
}
fn exclusive_reads(
    stage: &Path,
    copy: Option<&Path>,
    jdk: &Path,
    profile: NativeVoiceProfile,
) -> Vec<ExclusiveReadCheck> {
    let read = |path: &Path| {
        // READ needs no write permission, and share0 conflicts with retained
        // read handles as well. Each successful handle is dropped immediately.
        fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .is_ok()
    };
    let mut checks = Vec::new();
    for name in COMMON
        .iter()
        .copied()
        .chain(extras(profile).iter().copied())
    {
        checks.push(ExclusiveReadCheck {
            logical_name: format!("stage/{name}"),
            exclusive_read: read(&stage.join(name)),
        });
        if let Some(copy) = copy {
            checks.push(ExclusiveReadCheck {
                logical_name: format!("copy/{name}"),
                exclusive_read: read(&copy.join(name)),
            });
        }
    }
    for (name, path) in [
        ("jdk/release", jdk.join("release")),
        ("jdk/java.exe", jdk.join("bin/java.exe")),
    ] {
        checks.push(ExclusiveReadCheck {
            logical_name: name.into(),
            exclusive_read: read(&path),
        });
    }
    checks
}

fn write_document(root: &Path, name: &str, value: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec_pretty(value).expect("actual_voice_run_document_encode");
    recovery::write_new(&root.join(name), &bytes).expect("actual_voice_run_document_create_new");
    digest(&bytes)
}

struct PublishedReview {
    document_sha256: String,
    digest: String,
    deadline: Instant,
}
fn publish_review(
    root: &Path,
    name: &str,
    review: &Review,
    deadline: Instant,
    execution: bool,
    profile: NativeVoiceProfile,
) -> PublishedReview {
    let document = serde_json::json!({
        "schema": 1, "reviewId":review.review_id, "bundleId":review.bundle_id,
        "purpose":review.purpose, "digest":review.digest, "profile":format!("{profile:?}"),
        "before":review.before, "after":review.after, "details":review.details,
        "javaWillExecute":execution, "eulaWillBeTrueDuringRun":execution,
        "restorationRequiresOriginalCompletion":execution,
        "operations":if execution {
            vec!["claim", "snapshot_original_eula", "apply_eula_true", "mark_may_have_spawned", "spawn_reviewed_java83", "status767_and_pong", "observe_runtime_configuration_and_owned_udp85", "request_original_stop", "collect_original_completion", "restore_original_eula_if_authorized"]
        } else { vec!["create_new_private_origin", "create_new_diagnostic_copy", "install_compiled_profile_resources", "write_initial_compiled_baseline", "leave_eula_false"] },
        "approvalTtlSeconds":60,
        "remainingApprovalMillis":deadline.saturating_duration_since(Instant::now()).as_millis(),
        "executionInheritsOriginalPreparationDeadline":execution, "approvalRenewsDeadline":false,
        "fileOnlyReviewAuthorizesJava":false, "automaticRecovery":false,
        "maximumLifetimeSeconds":300, "maximumReadinessSeconds":180, "maximumStopSeconds":5,
        "maximumStdoutAndStderrBytes":4*1024*1024, "maximumHeapMiB":1536,
        "externalNetworkTested":false, "personalConfigurationChanged":false,
        "clientOrGuiTested":false, "worldEntryTested":false, "voiceAudioTested":false,
    });
    let document_sha256 = write_document(root, name, &document);
    println!(
        "native_voice_execution_review_ready:{}",
        serde_json::json!({
            "path":root.join(name), "sha256":document_sha256, "digest":review.digest,
        })
    );
    PublishedReview {
        document_sha256,
        digest: review.digest.clone(),
        deadline,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ReviewApproval {
    review_sha256: String,
    digest: String,
}
fn approval(root: &Path, name: &str, review: &PublishedReview) -> String {
    let path = root.join(name);
    loop {
        assert!(
            Instant::now() < review.deadline,
            "actual_voice_run_approval_expired"
        );
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                let bytes =
                    recovery::read_bounded(&path, 512).expect("actual_voice_run_approval_read");
                let approved: ReviewApproval = serde_json::from_slice(&bytes)
                    .expect("actual_voice_run_approval_closed_object");
                assert_eq!(
                    approved.review_sha256, review.document_sha256,
                    "actual_voice_run_review_sha"
                );
                assert_eq!(
                    approved.digest, review.digest,
                    "actual_voice_run_review_digest"
                );
                assert!(
                    Instant::now() < review.deadline,
                    "actual_voice_run_approval_expired_after_read"
                );
                return approved.digest;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => panic!("actual_voice_run_approval_metadata"),
        }
        thread::sleep(Duration::from_millis(25));
    }
}

/// Installed immediately after start. A panic anywhere in the helper still
/// requests this original control and waits for its original completion.
struct OriginalRun {
    control: DedicatedRun,
    completion: DedicatedCompletion,
    collected: bool,
}
impl OriginalRun {
    fn new(control: DedicatedRun) -> Self {
        let completion = control.completion();
        Self {
            control,
            completion,
            collected: false,
        }
    }
    fn stop_and_collect(&mut self, runtime: &Runtime) -> DedicatedReport {
        self.control.request_stop();
        let report = runtime.block_on(self.completion.wait());
        self.collected = true;
        report
    }
}
impl Drop for OriginalRun {
    fn drop(&mut self) {
        if self.collected {
            return;
        }
        self.control.request_stop();
        let completion = self.completion.clone();
        // No artificial waiter timeout becomes cleanup authority. The native
        // worker keeps its original300s/5s bounds and owns all actual cleanup.
        let collected = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("actual_voice_run_fallback_runtime");
            runtime.block_on(completion.wait())
        })
        .join();
        if !thread::panicking() {
            assert!(
                collected.is_ok(),
                "actual_voice_run_original_fallback_completion"
            );
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum InspectionError {
    ReadinessDeadline,
    ClosedBeforeReady,
    ObserverReleased,
    VoiceSourceUnavailable,
    Tcp(DedicatedError),
    Status(MinecraftStatusError),
    Protocol,
    Voice(VoiceRuntimeError),
    UnexpectedSample,
    EulaNotApplied,
    HelperPanicked,
}
fn inspection_error_value(error: InspectionError) -> String {
    match error {
        InspectionError::Tcp(error) => format!("tcp:{error:?}"),
        InspectionError::Status(error) => format!("status:{error:?}"),
        InspectionError::Voice(error) => format!("voice:{error:?}"),
        other => format!("{other:?}"),
    }
}
#[derive(Default)]
struct LiveFacts {
    ready: bool,
    tcp_before: bool,
    status767_pong: bool,
    tcp_after: bool,
    eula_true: bool,
    voice: Option<VoiceRuntimeSample>,
    error: Option<InspectionError>,
}
impl LiveFacts {
    fn fail(mut self, error: InspectionError) -> Self {
        self.error = Some(error);
        self
    }
}

async fn inspect(
    observer: &DedicatedObserver,
    tcp: &DedicatedSource,
    voice: Option<&DedicatedVoiceSource>,
    profile: NativeVoiceProfile,
    eula: &Path,
    readiness_deadline: Instant,
) -> LiveFacts {
    let mut facts = LiveFacts::default();
    loop {
        if Instant::now() >= readiness_deadline {
            return facts.fail(InspectionError::ReadinessDeadline);
        }
        match observer.snapshot() {
            Some(state) if state.phase == DedicatedPhase::Ready => {
                facts.ready = true;
                break;
            }
            Some(state)
                if state.reason.is_some()
                    || matches!(
                        state.phase,
                        DedicatedPhase::Stopping | DedicatedPhase::Complete
                    ) =>
            {
                return facts.fail(InspectionError::ClosedBeforeReady)
            }
            None => return facts.fail(InspectionError::ObserverReleased),
            Some(_) => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }
    // One observation attempt per API. No config/binding retries or timeout
    // changes can turn a failed runtime profile into a passing qualification.
    if let Err(error) = tcp.observe_tcp() {
        return facts.fail(InspectionError::Tcp(error));
    }
    facts.tcp_before = true;
    match probe_minecraft_status(MinecraftStatusRequest {
        endpoint: producer::ENDPOINT,
        handshake_host: "127.0.0.1".into(),
        handshake_port: producer::ENDPOINT.port(),
        protocol: producer::PROTOCOL,
        timeout: Duration::from_secs(5),
    })
    .await
    {
        Ok(status)
            if status.requested_protocol == 767
                && status.reported_protocol == 767
                && status.protocol_matches_requested =>
        {
            facts.status767_pong = true
        }
        Ok(_) => return facts.fail(InspectionError::Protocol),
        Err(error) => return facts.fail(InspectionError::Status(error)),
    }
    if let Err(error) = tcp.observe_tcp() {
        return facts.fail(InspectionError::Tcp(error));
    }
    facts.tcp_after = true;
    facts.eula_true = recovery::read_bounded(eula, 4096).is_ok_and(|bytes| bytes == EULA_TRUE);
    if !facts.eula_true {
        return facts.fail(InspectionError::EulaNotApplied);
    }
    let Some(voice) = voice else {
        return facts.fail(InspectionError::VoiceSourceUnavailable);
    };
    let sample = match voice.observe() {
        Ok(sample) => sample,
        Err(error) => return facts.fail(InspectionError::Voice(error)),
    };
    let artifact_version = match profile {
        NativeVoiceProfile::SimpleVoiceChat => "2.6.22",
        NativeVoiceProfile::PlasmoVoice => "2.1.17",
    };
    if sample.profile != profile
        || sample.minecraft != "1.21.1"
        || sample.loader != "fabric"
        || sample.loader_version != "0.16.14"
        || sample.artifact_version != artifact_version
        || sample.binding != VoiceRuntimeBinding::Ipv4Literal
        || sample.announcement != VoiceRuntimeAnnouncement::DefaultUnchanged
    {
        return facts.fail(InspectionError::UnexpectedSample);
    }
    facts.voice = Some(sample);
    facts
}

fn report_value(report: &DedicatedReport) -> serde_json::Value {
    serde_json::json!({
        "reason":format!("{:?}",report.reason), "failure":report.failure.map(|value|format!("{value:?}")),
        "eula":format!("{:?}",report.eula), "reachedReady":report.reached_ready,
        "process":report.process.as_ref().map(|process|serde_json::json!({
            "reason":format!("{:?}",process.reason), "failure":process.failure.map(|value|format!("{value:?}")),
            "exitCode":process.exit_code, "forced":process.forced,
            "stdoutBytes":process.stdout_bytes, "stderrBytes":process.stderr_bytes,
            "reaped":process.reaped, "readersComplete":process.readers_complete,
        })),
    })
}
fn normal_completion(report: &DedicatedReport) -> bool {
    report.reason == DedicatedReason::Requested
        && report.failure.is_none()
        && report.reached_ready
        && report.eula == EulaDisposition::Restored
        && report.process.as_ref().is_some_and(|process| {
            process.reason == OwnedRunReason::Requested
                && process.failure.is_none()
                && process.exit_code == Some(0)
                && !process.forced
                && process.reaped
                && process.readers_complete
        })
}
fn sample_value(sample: VoiceRuntimeSample) -> serde_json::Value {
    serde_json::json!({
        "profile":format!("{:?}",sample.profile), "minecraft":sample.minecraft,
        "loader":sample.loader, "loaderVersion":sample.loader_version, "artifactVersion":sample.artifact_version,
        "binding":"Ipv4Literal", "announcement":"DefaultUnchanged",
        "modSocketAuthorshipEstablished":false, "voiceAudioTested":false,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimView {
    schema: u32,
    bundle: BundleId,
    transaction: Uuid,
    manifest_sha256: String,
    review_digest: String,
    original: Observation,
}

fn run(profile: NativeVoiceProfile, label: &str) {
    let stage = PathBuf::from(
        std::env::var_os("AURORA_DEDICATED_LAB_STAGE").expect("actual_voice_run_stage"),
    );
    let jdk =
        PathBuf::from(std::env::var_os("AURORA_DEDICATED_LAB_JDK").expect("actual_voice_run_jdk"));
    let lab = lab_root().expect("actual_voice_run_lab");
    let evidence = lab
        .parent()
        .expect("actual_voice_run_aurora")
        .join("connect-validation")
        .join(format!(
            "native-voice85-{label}-execution-review-{}",
            Uuid::new_v4()
        ));
    fs::create_dir(&evidence).expect("actual_voice_run_fresh_evidence");
    let _evidence_pins = recovery::pin_directory(&evidence).expect("actual_voice_run_evidence_pin");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("actual_voice_run_helper_runtime");
    let resources_before =
        selected_resources(&stage, &jdk, profile).expect("actual_voice_run_resources");
    // The factory plan itself retains immutable pins. Establish this positive
    // access control BEFORE creating that plan, without a write or mutation.
    let baseline_access = exclusive_reads(&stage, None, &jdk, profile);
    let baseline_all = baseline_access.iter().all(|check| check.exclusive_read);
    write_document(
        &evidence,
        "baseline-access.json",
        &serde_json::json!({
            "schema":1, "event":"native_voice_exclusive_read_baseline", "profile":label,
            "allExclusiveRead":baseline_all,
            "stageFileCount":COMMON.len()+extras(profile).len(), "copyFileCount":0, "jdkFileCount":2,
            "fileCount":baseline_access.len(), "checks":baseline_access,
            "factoryPlanned":false, "fileMutation":false, "processStarted":false,
        }),
    );
    assert!(
        baseline_all,
        "actual_voice_run_exclusive_read_baseline_failed_before_factory"
    );
    let factory_deadline = Instant::now() + REVIEW_TTL;
    let factory =
        plan_voice_factory(&stage, &jdk, profile, true).expect("actual_voice_run_compiled_factory");
    assert_eq!(factory.review().purpose, "native_voice_factory_v2");
    let factory_review = publish_review(
        &evidence,
        "factory-review.json",
        factory.review(),
        factory_deadline,
        false,
        profile,
    );
    let approved = approval(&evidence, "factory-approve.json", &factory_review);
    let prepared = factory
        .create(&approved)
        .expect("actual_voice_run_new_copy");
    let paths = prepared.test_paths();
    let bundle_id = prepared.bundle_id();
    let original_deadline = prepared.approval_deadline();
    let source_before = files::tree(&paths.origin);
    let copy_before = artifacts(&paths.copy, profile).expect("actual_voice_run_copy_artifacts");
    let manifest_before = observe_file(&paths.bundle.join("bundle.json"), MAX_JOURNAL)
        .expect("actual_voice_run_manifest")
        .0;
    let (eula_before, eula_bytes) =
        observe_file(&target(&paths), 4096).expect("actual_voice_run_eula");
    assert_eq!(eula_bytes, EULA_FALSE);
    let recovery_database = paths.bundle.join("native/.aurora/recovery/index.sqlite3");
    let database_before = observe_file(&recovery_database, 4 * 1024 * 1024)
        .expect("actual_voice_run_database")
        .0;
    let plan = plan_voice_execution(prepared).expect("actual_voice_run_production_execution_plan");
    assert_eq!(plan.review().purpose, "native_voice_execution_v2");
    let execution_review = publish_review(
        &evidence,
        "execution-review.json",
        plan.review(),
        original_deadline,
        true,
        profile,
    );
    let execution_digest = approval(&evidence, "execution-approve.json", &execution_review);
    let started = Instant::now();
    let control = match plan.start(&execution_digest, CancellationToken::new()) {
        Ok(run) => run,
        Err(error) => {
            write_document(
                &evidence,
                "result.json",
                &serde_json::json!({
                    "schema":1,"event":"native_voice_actual_start_rejected","profile":label,"passed":false,
                    "error":format!("{error:?}"),"originalCompletion":null,"automaticRecovery":false,
                    "evidencePreserved":true,"javaExecuted":false,"voiceAudioTested":false,
                    "worldEntryTested":false,"clientOrGuiTested":false,"externalNetworkTested":false,
                }),
            );
            panic!("actual_voice_run_start_rejected_evidence_preserved");
        }
    };
    // RAII is installed before obtaining sources or running any helper oracle.
    let mut original = OriginalRun::new(control);
    let observer = original.control.observer();
    let tcp = original.control.source();
    let voice = original.control.voice_source();
    let facts = catch_unwind(AssertUnwindSafe(|| {
        runtime.block_on(inspect(
            &observer,
            &tcp,
            voice.as_ref(),
            profile,
            &target(&paths),
            started + Duration::from_secs(180),
        ))
    }))
    .unwrap_or_else(|_| LiveFacts::default().fail(InspectionError::HelperPanicked));
    let report = original.stop_and_collect(&runtime);
    // Persist original completion even when voice/config/binding failed. No
    // filesystem assertion or report publication can bypass original cleanup.
    write_document(
        &evidence,
        "completion.json",
        &serde_json::json!({
            "schema":1,"profile":label,"originalCompletion":report_value(&report),
            "inspectionError":facts.error.map(inspection_error_value),
            "needsRecovery":report.eula==EulaDisposition::NeedsRecovery,
            "automaticRecovery":false,"evidencePreserved":true,
        }),
    );
    let repeated = runtime.block_on(original.completion.wait()) == report;
    let checks = catch_unwind(AssertUnwindSafe(|| {
        // Control, TCP source and voice source remain alive through these
        // resource-release checks. They must not retain process or file owners.
        let eula_exact = observe_file(&target(&paths),4096).is_ok_and(|(observed,bytes)|
            observed==eula_before && bytes==EULA_FALSE);
        let snapshot = recovery::read_bounded(&slot(&paths,"snapshot.bin"),4096).is_ok_and(|bytes|bytes==EULA_FALSE);
        let applied = recovery::read_bounded(&slot(&paths,"applied.bin"),4096).is_ok_and(|bytes|bytes==EULA_TRUE);
        let claim = recovery::read_bounded(&slot(&paths,"claim.json"),MAX_JOURNAL).ok()
            .and_then(|bytes|serde_json::from_slice::<ClaimView>(&bytes).ok());
        let claim_exact = claim.is_some_and(|claim|claim.schema==1 && claim.bundle==bundle_id
            && !claim.transaction.is_nil() && claim.manifest_sha256==manifest_before.sha256
            && claim.review_digest==execution_digest && claim.original==eula_before);
        let marker = recovery::read_bounded(&slot(&paths,"may-have-spawned.json"),MAX_JOURNAL).is_ok();
        let restored = recovery::read_bounded(&slot(&paths,"restored.json"),MAX_JOURNAL).is_ok();
        let old_absent = absent(&slot(&paths,"old.bin")).is_ok();
        let origin_unchanged = files::tree(&paths.origin)==source_before;
        let resources_unchanged = selected_resources(&stage,&jdk,profile).is_ok_and(|value|value==resources_before);
        let artifacts_unchanged = artifacts(&paths.copy,profile).is_ok_and(|value|value==copy_before);
        let manifest_unchanged = observe_file(&paths.bundle.join("bundle.json"),MAX_JOURNAL)
            .is_ok_and(|value|value.0==manifest_before);
        let database_unchanged = observe_file(&recovery_database,4*1024*1024)
            .is_ok_and(|value|value.0==database_before);
        let unlocked = acquire_instance_recovery_lock(&fixture::copy_instance(&paths)).is_ok();
        let tcp_rebound = TcpListener::bind(producer::ENDPOINT).is_ok();
        let udp_rebound = UdpSocket::bind(endpoint(profile)).is_ok();
        let tcp_closed = matches!(tcp.observe_tcp(),Err(DedicatedError::Closed));
        let voice_closed = voice.as_ref().is_some_and(|source|matches!(source.observe(),Err(VoiceRuntimeError::Closed)));
        let exclusive_access = exclusive_reads(&stage,Some(&paths.copy),&jdk,profile);
        let exclusive_read = exclusive_access.iter().all(|check|check.exclusive_read);
        // Isolated normal reopen is compiled-catalog admission and never a
        // fresh execution/recovery authority or a mutable-baseline recapture.
        let compiled_reopen = factory::Bundle::reopen(&lab,bundle_id).is_ok();
        let bundler_output = ["libraries", "versions"].iter().all(|name| {
            let path = paths.copy.join(".connect-run-temp").join(name);
            recovery::pin_directory(&path).is_ok()
                && fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_some_and(|entry|entry.is_ok()))
        });
        let all = eula_exact && snapshot && applied && claim_exact && marker && restored && old_absent
            && origin_unchanged && resources_unchanged && artifacts_unchanged && manifest_unchanged
            && database_unchanged && unlocked && tcp_rebound && udp_rebound && tcp_closed && voice_closed
            && exclusive_read && compiled_reopen && bundler_output && repeated;
        serde_json::json!({
            "all":all,"eulaExactOriginalIdentityAndBytes":eula_exact,"snapshotExact":snapshot,
            "appliedPreserved":applied,"claimBoundToOriginalManifestAndExecution":claim_exact,
            "spawnIntent":marker,"restoredMarker":restored,"oldAbsent":old_absent,
            "sourceUnchanged":origin_unchanged,"selectedResourcesUnchanged":resources_unchanged,
            "copyArtifactsUnchanged":artifacts_unchanged,"manifestUnchanged":manifest_unchanged,
            "existingRecoveryDatabaseUnchanged":database_unchanged,"lockReleased":unlocked,
            "tcpPortRebound":tcp_rebound,"udpPortRebound":udp_rebound,"tcpSourceClosed":tcp_closed,
            "voiceSourceClosed":voice_closed,"controlAndSourcesHeldDuringChecks":true,
            "immutableFilesExclusiveReadAfterCompletion":exclusive_read,
            "immutableFileExclusiveReadCount":exclusive_access.len(),"immutableFileExclusiveReads":exclusive_access,
            "compiledNormalReopenPassed":compiled_reopen,
            "bundlerOutputInPinnedRunTemp":bundler_output,
            "repeatedOriginalCompletionEqual":repeated,
        })
    })).unwrap_or_else(|_|serde_json::json!({"all":false,"error":"helper_postcheck_panicked"}));
    let passed = facts.error.is_none()
        && facts.voice.is_some()
        && facts.ready
        && facts.status767_pong
        && facts.tcp_before
        && facts.tcp_after
        && facts.eula_true
        && normal_completion(&report)
        && checks.get("all").and_then(serde_json::Value::as_bool) == Some(true);
    let result = serde_json::json!({
        "schema":1,"event":"native_voice_actual_completed","profile":label,"passed":passed,
        "elapsedMillis":started.elapsed().as_millis(),"originalCompletion":report_value(&report),
        "gameReady":facts.ready,"tcpBeforeStatus":facts.tcp_before,"status767PongVerified":facts.status767_pong,
        "tcpAfterStatus":facts.tcp_after,"eulaTrueDuringRun":facts.eula_true,
        "voiceObservation":facts.voice.map(sample_value),
        "inspectionError":facts.error.map(inspection_error_value),"checks":checks,
        "needsRecovery":report.eula==EulaDisposition::NeedsRecovery,"automaticRecovery":false,
        "evidencePreserved":true,"modSocketAuthorshipEstablished":false,"voiceAudioTested":false,
        "minecraftLoginTested":false,"worldEntryTested":false,"clientOrGuiTested":false,
        "externalNetworkTested":false,"personalConfigurationChanged":false,"publicRelayEnabled":false,
    });
    write_document(&evidence, "result.json", &result);
    println!("{result}");
    drop(original);
    drop(runtime);
    assert!(
        passed,
        "actual_voice_run_failed_after_original_cleanup_evidence_preserved"
    );
}

#[test]
#[ignore = "85D real SVC Java opt-in: separate concrete factory/execution SHA+digest approvals; no GUI/audio"]
fn actual_pinned_native_svc_status_voice_observation_and_original_cleanup() {
    run(NativeVoiceProfile::SimpleVoiceChat, "svc");
}

#[test]
#[ignore = "85E real Plasmo Java opt-in: separate concrete factory/execution SHA+digest approvals; no GUI/audio"]
fn actual_pinned_native_plasmo_status_voice_observation_and_original_cleanup() {
    run(NativeVoiceProfile::PlasmoVoice, "plasmo");
}
