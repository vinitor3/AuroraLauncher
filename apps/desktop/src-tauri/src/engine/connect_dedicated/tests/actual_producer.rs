//! Explicit opt-in for the real native producer. The two documents are reviews,
//! not executable input. Only separate, matching local approvals consume the
//! factory and execution plans; the file-only EULA plan is never executed here.
use super::*;
use crate::engine::{acquire_instance_recovery_lock, connect_owned_run::OwnedRunReason};
use aurora_connect_lab::{probe_minecraft_status, MinecraftStatusRequest, MinecraftStatusSummary};
use producer::{
    DedicatedCompletion, DedicatedError, DedicatedObserver, DedicatedPhase, DedicatedReason,
    DedicatedReport, DedicatedSource, EulaDisposition,
};
use std::{
    net::TcpListener,
    panic::{catch_unwind, AssertUnwindSafe},
    thread,
};
use tokio_util::sync::CancellationToken;

const ARTIFACT_NAMES: [&str; 9] = [
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

// These observations do not replace the factory's compiled resource pins. The
// independent before/after check covers the nine artifacts and selected Java
// executable/release, not every file in the JRE installation.
fn artifact_observations(root: &Path) -> Result<serde_json::Value> {
    let mut observations = Vec::new();
    for name in ARTIFACT_NAMES {
        observations.push(serde_json::json!({
            "name":name,
            "observed":observe_file(&root.join(name),64*1024*1024)?.0,
        }));
    }
    Ok(serde_json::Value::Array(observations))
}

fn selected_resources(stage: &Path, jdk: &Path) -> Result<serde_json::Value> {
    Ok(serde_json::json!({
        "artifacts":artifact_observations(stage)?,
        "javaRelease":observe_file(&jdk.join("release"),64*1024)?.0,
        "javaExecutable":observe_file(&jdk.join("bin/java.exe"),4*1024*1024)?.0,
        "serverPublishedSha1":"59353fb40c36d304f2035d51e7d6e6baa98dc05c",
        "serverSha256Meaning":"observed_only_not_independent_official_digest",
    }))
}

fn write_evidence(root: &Path, name: &str, value: &serde_json::Value) {
    let bytes = serde_json::to_vec_pretty(value).expect("actual_producer_evidence_encode");
    recovery::write_new(&root.join(name), &bytes).expect("actual_producer_evidence_create_new");
}

fn review_file(
    root: &Path,
    name: &str,
    review: &Review,
    deadline: Instant,
    execution: bool,
    extra: serde_json::Value,
) {
    write_evidence(
        root,
        name,
        &serde_json::json!({
            "schema":1,
            "reviewId":review.review_id,
            "purpose":review.purpose,
            "bundleId":review.bundle_id,
            "digest":review.digest,
            "before":review.before,
            "after":review.after,
            "details":review.details,
            "javaWillExecute":execution,
            "externalNetworkTested":false,
            "personalConfigurationChanged":false,
            "approvalTtlSeconds":60,
            "remainingApprovalMillis":deadline.saturating_duration_since(Instant::now()).as_millis(),
            "executionInheritsOriginalPreparationDeadline":execution,
            "approvalRenewsDeadline":false,
            "extra":extra,
        }),
    );
    println!(
        "native_dedicated_review_ready:{}",
        root.join(name).display()
    );
}

fn approval(root: &Path, name: &str, digest: &str, deadline: Instant) -> String {
    let path = root.join(name);
    loop {
        assert!(
            Instant::now() < deadline,
            "actual_producer_approval_expired"
        );
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                let bytes =
                    recovery::read_bounded(&path, 128).expect("actual_producer_approval_read");
                let approved = std::str::from_utf8(&bytes)
                    .expect("actual_producer_approval_utf8")
                    .trim();
                assert_eq!(approved.len(), 64, "actual_producer_approval_length");
                assert!(approved.bytes().all(|byte| byte.is_ascii_hexdigit()));
                assert_eq!(approved, digest, "actual_producer_approval_mismatch");
                return approved.to_owned();
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => panic!("actual_producer_approval_metadata"),
        }
        thread::sleep(Duration::from_millis(25));
    }
}

async fn observe_source(source: &DedicatedSource) -> std::result::Result<(), String> {
    // Ready can be published just before the readiness worker drops its last
    // query reservation. Busy is transient and conveys no positive observation.
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match source.observe_tcp() {
            Ok(()) => return Ok(()),
            Err(DedicatedError::Busy) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Err(error) => return Err(format!("source:{error:?}")),
        }
    }
}

async fn inspect_live_run(
    observer: &DedicatedObserver,
    source: &DedicatedSource,
    readiness_deadline: Instant,
) -> std::result::Result<MinecraftStatusSummary, String> {
    loop {
        if Instant::now() >= readiness_deadline {
            return Err("helper_readiness_deadline".into());
        }
        match observer.snapshot() {
            Some(snapshot) if snapshot.phase == DedicatedPhase::Ready => break,
            Some(snapshot)
                if snapshot.reason.is_some()
                    || matches!(
                        snapshot.phase,
                        DedicatedPhase::Stopping | DedicatedPhase::Complete
                    ) =>
            {
                return Err("producer_closed_before_ready".into());
            }
            None => return Err("producer_observer_released_before_ready".into()),
            Some(_) => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }
    observe_source(source).await?;
    let status = probe_minecraft_status(MinecraftStatusRequest {
        endpoint: producer::ENDPOINT,
        handshake_host: "127.0.0.1".into(),
        handshake_port: producer::ENDPOINT.port(),
        protocol: producer::PROTOCOL,
        timeout: Duration::from_secs(5),
    })
    .await
    .map_err(|error| format!("status:{error:?}"))?;
    observe_source(source).await?;
    if status.requested_protocol != producer::PROTOCOL
        || status.reported_protocol != producer::PROTOCOL
        || !status.protocol_matches_requested
    {
        return Err("status_protocol_mismatch".into());
    }
    Ok(status)
}

fn report_value(report: &DedicatedReport) -> serde_json::Value {
    // The enums contain closed error variants, never process output, PID,
    // player identities, file contents or arbitrary exception strings.
    serde_json::json!({
        "reason":format!("{:?}",report.reason),
        "failure":report.failure.map(|value|format!("{value:?}")),
        "eula":format!("{:?}",report.eula),
        "reachedReady":report.reached_ready,
        "process":report.process.as_ref().map(|process|serde_json::json!({
            "reason":format!("{:?}",process.reason),
            "failure":process.failure.map(|value|format!("{value:?}")),
            "exitCode":process.exit_code,
            "forced":process.forced,
            "stdoutBytes":process.stdout_bytes,
            "stderrBytes":process.stderr_bytes,
            "reaped":process.reaped,
            "readersComplete":process.readers_complete,
        })),
    })
}

fn normal_original_completion(report: &DedicatedReport) -> bool {
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

fn closed_source(source: &DedicatedSource) -> bool {
    matches!(source.observe_tcp(), Err(DedicatedError::Closed))
}

#[test]
#[ignore = "explicit real Java STATUS opt-in; factory and execution reviews require separate local approvals"]
fn actual_pinned_native_dedicated_status_and_original_cleanup() {
    let stage = PathBuf::from(
        std::env::var_os("AURORA_DEDICATED_LAB_STAGE").expect("actual_producer_stage"),
    );
    let jdk =
        PathBuf::from(std::env::var_os("AURORA_DEDICATED_LAB_JDK").expect("actual_producer_jdk"));
    let evidence = lab_root()
        .expect("actual_producer_lab")
        .parent()
        .expect("actual_producer_aurora")
        .join("connect-validation")
        .join(format!("native-dedicated84c-review-{}", Uuid::new_v4()));
    fs::create_dir(&evidence).expect("actual_producer_evidence_directory");
    let _evidence_pins = recovery::pin_directory(&evidence).expect("actual_producer_evidence_pin");
    // Build the helper runtime and independent resource inventory before either
    // execution or its approval clock. It owns no supervisor task or authority.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("actual_producer_helper_runtime");
    let resources_before = selected_resources(&stage, &jdk).expect("actual_producer_resources");
    let factory_deadline = Instant::now() + REVIEW_TTL;
    let factory = plan_status_factory(&stage, &jdk, true).expect("actual_producer_factory_plan");
    review_file(
        &evidence,
        "factory-review.json",
        factory.review(),
        factory_deadline,
        false,
        resources_before.clone(),
    );
    let factory_digest = approval(
        &evidence,
        "factory-approve.txt",
        &factory.review().digest,
        factory_deadline,
    );
    let prepared = factory
        .create(&factory_digest)
        .expect("actual_producer_factory_create");
    let paths = prepared.test_paths();
    let bundle_id = prepared.bundle_id();
    // This is A's original preparation deadline. Creating or presenting C's
    // review, and waiting for its distinct approval, must not renew it.
    let execution_deadline = prepared.approval_deadline();
    let source_before = files::tree(&paths.origin);
    let copy_artifacts_before = artifact_observations(&paths.copy).expect("actual_producer_copy");
    let (original_eula, original_eula_bytes) =
        observe_file(&target(&paths), 4096).expect("actual_producer_original_eula");
    assert_eq!(original_eula_bytes, EULA_FALSE);
    let source_runtime = observe_file(&paths.origin.join("aurora-instance.json"), 1024)
        .expect("actual_producer_origin_metadata")
        .0;
    let copy_runtime = observe_file(&paths.copy.join("aurora-instance.json"), 1024)
        .expect("actual_producer_copy_metadata")
        .0;
    let plan = plan_status_execution(prepared).expect("actual_producer_execution_plan");
    review_file(
        &evidence,
        "execution-review.json",
        plan.review(),
        execution_deadline,
        true,
        serde_json::json!({
            "sourceRuntime":source_runtime,
            "copyRuntime":copy_runtime,
            "target":target(&paths),
            "transaction":paths.transaction,
            "resources":resources_before,
            "copyArtifacts":copy_artifacts_before,
            "eulaBefore":original_eula,
            "eulaBeforeText":String::from_utf8_lossy(EULA_FALSE),
            "eulaAfterText":String::from_utf8_lossy(EULA_TRUE),
            "readinessSecondsMaximum":180,
            "runSecondsMaximum":300,
            "statusSecondsMaximum":5,
            "stopSecondsMaximum":5,
            "automaticRecovery":false,
            "fileOnlyEulaReviewAuthorizesExecution":false,
        }),
    );
    let execution_digest = approval(
        &evidence,
        "execution-approve.txt",
        &plan.review().digest,
        execution_deadline,
    );
    let started = Instant::now();
    let run = match plan.start(&execution_digest, CancellationToken::new()) {
        Ok(run) => run,
        Err(error) => {
            write_evidence(
                &evidence,
                "result.json",
                &serde_json::json!({
                    "schema":1,"event":"native_dedicated_actual_start_rejected",
                    "passed":false,"bundleId":bundle_id,
                    "error":format!("{error:?}"),"completion":null,
                    "automaticRecovery":false,"evidencePreserved":true,
                }),
            );
            panic!("actual_producer_start_rejected");
        }
    };
    let observer = run.observer();
    let source = run.source();
    let original_completion: DedicatedCompletion = run.completion();

    // No assertion, propagation or post-start helper failure may bypass stop
    // and the original completion. Even an unexpected inspection panic is
    // caught here; its raw payload is not exported. The production worker owns
    // EULA/Child/queries independently of this helper's waiters/runtime.
    let inspected = catch_unwind(AssertUnwindSafe(|| {
        runtime.block_on(inspect_live_run(
            &observer,
            &source,
            started + Duration::from_secs(180),
        ))
    }))
    .unwrap_or_else(|_| Err("helper_inspection_panicked".into()));
    run.request_stop();
    // Do not convert a helper timeout into a claim of cleanup. The producer's
    // original 300s/5s limits remain in force; this waits for its actual report.
    let report = runtime.block_on(original_completion.wait());
    write_evidence(
        &evidence,
        "completion.json",
        &serde_json::json!({
            "schema":1,"bundleId":bundle_id,
            "originalCompletion":report_value(&report),
            "inspectionError":inspected.as_ref().err(),
            "needsRecovery":report.eula==EulaDisposition::NeedsRecovery,
            "automaticRecovery":false,"evidencePreserved":true,
        }),
    );
    let repeated_completion_equal = runtime.block_on(original_completion.wait()) == report;
    drop(run);
    drop(runtime);

    // Completion was already persisted before independent filesystem checks.
    // An uncertain or divergent EULA is observed only, never repaired here.
    let checks = catch_unwind(AssertUnwindSafe(|| {
        let eula_restored = observe_file(&target(&paths), 4096)
            .is_ok_and(|(observed, bytes)| observed == original_eula && bytes == EULA_FALSE);
        let snapshot_exact = recovery::read_bounded(&slot(&paths, "snapshot.bin"), 4096)
            .is_ok_and(|bytes| bytes == EULA_FALSE);
        let applied_preserved = recovery::read_bounded(&slot(&paths, "applied.bin"), 4096)
            .is_ok_and(|bytes| bytes == EULA_TRUE);
        let claim = recovery::read_bounded(&slot(&paths, "claim.json"), MAX_JOURNAL)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        let claim_bound_to_execution = claim.as_ref().is_some_and(|value| {
            value
                .get("review_digest")
                .and_then(serde_json::Value::as_str)
                == Some(execution_digest.as_str())
        });
        let spawn_intent =
            recovery::read_bounded(&slot(&paths, "may-have-spawned.json"), MAX_JOURNAL).is_ok();
        let restored_marker =
            recovery::read_bounded(&slot(&paths, "restored.json"), MAX_JOURNAL).is_ok();
        let old_absent = absent(&slot(&paths, "old.bin")).is_ok();
        let source_unchanged = files::tree(&paths.origin) == source_before;
        let resources_unchanged =
            selected_resources(&stage, &jdk).is_ok_and(|observed| observed == resources_before);
        let copy_artifacts_unchanged = artifact_observations(&paths.copy)
            .is_ok_and(|observed| observed == copy_artifacts_before);
        let lock_released = acquire_instance_recovery_lock(&fixture::copy_instance(&paths)).is_ok();
        let endpoint_rebound = TcpListener::bind(producer::ENDPOINT).is_ok();
        let source_closed = closed_source(&source);
        let all = eula_restored
            && snapshot_exact
            && applied_preserved
            && claim_bound_to_execution
            && spawn_intent
            && restored_marker
            && old_absent
            && source_unchanged
            && resources_unchanged
            && copy_artifacts_unchanged
            && lock_released
            && endpoint_rebound
            && source_closed
            && repeated_completion_equal;
        serde_json::json!({
            "all":all,"eulaExactOriginalIdentityAndBytes":eula_restored,
            "snapshotExact":snapshot_exact,"appliedPreserved":applied_preserved,
            "claimBoundToExecution":claim_bound_to_execution,"spawnIntent":spawn_intent,
            "restoredMarker":restored_marker,"oldAbsent":old_absent,
            "sourceUnchanged":source_unchanged,"selectedResourcesUnchanged":resources_unchanged,
            "copyArtifactsUnchanged":copy_artifacts_unchanged,
            "lockReleased":lock_released,"endpointRebound":endpoint_rebound,
            "sourceClosed":source_closed,"repeatedCompletionEqual":repeated_completion_equal,
        })
    }))
    .unwrap_or_else(|_| serde_json::json!({"all":false,"error":"helper_postcheck_panicked"}));
    let passed = inspected.is_ok()
        && normal_original_completion(&report)
        && checks.get("all").and_then(serde_json::Value::as_bool) == Some(true);
    let result = serde_json::json!({
        "schema":1,"event":"native_dedicated_actual_completed","passed":passed,
        "bundleId":bundle_id,"elapsedMillis":started.elapsed().as_millis(),
        "completion":report_value(&report),
        "status":inspected.as_ref().ok(),"inspectionError":inspected.as_ref().err(),
        "checks":checks,"automaticRecovery":false,"evidencePreserved":true,
        "minecraftLoginTested":false,"worldEntryTested":false,"voiceAudioTested":false,
        "personalConfigurationChanged":false,"externalNetworkTested":false,
        "publicRelayEnabled":false,
    });
    write_evidence(&evidence, "result.json", &result);
    println!("{result}");
    assert!(
        passed,
        "actual_producer_failed_after_original_completion; inspect preserved evidence"
    );
}
