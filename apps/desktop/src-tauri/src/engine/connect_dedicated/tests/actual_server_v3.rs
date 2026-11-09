//! Explicit serial opt-ins: real downloads/installer, immutable native proof,
//! factory and original server lifecycle. No reconstructed execution capability.
use super::super::super::*;
use crate::engine::connect_ready_profile::ReadyProfile;
use aurora_connect_lab::{probe_minecraft_status, MinecraftStatusRequest};
use producer::{DedicatedCompletion, DedicatedPhase, DedicatedRun, EulaDisposition};
use std::{
    net::{Ipv4Addr, TcpListener, UdpSocket},
    panic::{catch_unwind, AssertUnwindSafe},
    thread,
};
use tokio_util::sync::CancellationToken;

struct OriginalRun {
    run: DedicatedRun,
    completion: DedicatedCompletion,
    collected: bool,
}
impl OriginalRun {
    fn new(run: DedicatedRun) -> Self {
        let completion = run.completion();
        Self {
            run,
            completion,
            collected: false,
        }
    }
    fn finish(&mut self, runtime: &tokio::runtime::Runtime) -> producer::DedicatedReport {
        self.run.request_stop();
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
        self.run.request_stop();
        let completion = self.completion.clone();
        let _ = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("v3_cleanup_runtime");
            runtime.block_on(completion.wait())
        })
        .join();
    }
}
fn review(root: &Path, name: &str, value: &Review) -> String {
    // Invoking this ignored test with its explicit opt-in authorizes only these
    // compiled test scenarios. Product callers still use their own UI review.
    let document = serde_json::json!({"purpose":value.purpose,"digest":value.digest,"before":value.before,"after":value.after,"details":value.details});
    recovery::write_new(
        &root.join(name),
        &serde_json::to_vec_pretty(&document).unwrap(),
    )
    .unwrap();
    value.digest.clone()
}
fn run(profile: ReadyProfile, voice: NativeVoiceProfile) {
    assert_eq!(
        std::env::var("AURORA_CONNECT_V3_REAL_TESTS").as_deref(),
        Ok("1"),
        "v3_real_test_opt_in"
    );
    for port in [43001, 43002, 43003] {
        if port == 43001 {
            drop(TcpListener::bind((Ipv4Addr::LOCALHOST, port)).expect("v3_exclusive_game_port"));
        } else {
            drop(UdpSocket::bind((Ipv4Addr::LOCALHOST, port)).expect("v3_exclusive_voice_port"));
        }
    }
    let validation = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .join(".aurora/connect-validation");
    recovery::pin_directory(&validation).expect("v3_existing_validation_parent");
    let evidence = validation.join(format!("native-server-v3-{}", Uuid::new_v4()));
    fs::create_dir(&evidence).unwrap();
    println!("native_server_v3_evidence:{}", evidence.display());
    // The real Win32 process has a bounded current-directory length. Keep the
    // execution copy in a fresh short temp root; redacted reports stay in the
    // repository. No junction, drive mapping or personal instance is reused.
    let execution_root = std::env::temp_dir().join(format!("acv3-{}", Uuid::new_v4()));
    fs::create_dir(&execution_root).unwrap();
    recovery::pin_directory(&execution_root).unwrap();
    recovery::write_new(
        &evidence.join("execution-root.json"),
        &serde_json::to_vec(&serde_json::json!({"executionRoot":execution_root,"owned":true}))
            .unwrap(),
    )
    .unwrap();
    let lab = execution_root.join(resources::LAB_NAME);
    let cancel = || false;
    let plan = server_preparation::ServerPreparationPlan::prepare(&lab, profile, voice, &cancel)
        .expect("v3_original_prepare");
    let approved = review(&evidence, "preparation-review.json", plan.review());
    let proof = plan
        .execute(&approved, &cancel)
        .expect("v3_original_installer_completion_and_proof");
    assert_eq!(proof.runtime(), profile);
    proof.verify().unwrap();
    let source_records = proof.artifacts().to_vec();
    let source_stage = proof.stage().to_path_buf();
    let factory =
        factory::plan_voice_server_factory_at(&lab, proof).expect("v3_proof_consumed_once");
    let approved = review(&evidence, "factory-review.json", factory.review());
    let prepared = factory.create(&approved).expect("v3_new_factory_copy");
    assert_eq!(
        prepared
            .bundle
            .manifest
            .server_v3()
            .unwrap()
            .server
            .runtime()
            .unwrap(),
        profile
    );
    assert!(prepared.bundle.manifest.voice_v2().is_err());
    let paths = prepared.test_paths();
    let id = prepared.bundle_id();
    let original_eula = observe_file(&paths.copy.join("eula.txt"), 4096).unwrap();
    assert_eq!(original_eula.1, EULA_FALSE);
    let manifest = prepared.bundle.manifest.encode_bounded().unwrap();
    let execution =
        producer::plan_voice(prepared).expect("v3_production_recipe_from_original_proof");
    let recipe: serde_json::Value = serde_json::from_str(&execution.review().details[0]).unwrap();
    assert_eq!(recipe["limits"]["stop_ms"], 30_000);
    assert_eq!(recipe["limits"]["lifetime_ms"], 14_400_000);
    let approved = review(&evidence, "execution-review.json", execution.review());
    let original_hooks = execution.owned_test_hooks();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut original = OriginalRun::new(
        execution
            .start(&approved, CancellationToken::new())
            .expect("v3_start_original_server"),
    );
    let inspection = catch_unwind(AssertUnwindSafe(|| {
        runtime.block_on(async {
            let deadline = Instant::now() + Duration::from_secs(180);
            loop {
                let state = original
                    .run
                    .observer()
                    .snapshot()
                    .expect("v3_original_observer");
                if state.phase == DedicatedPhase::Ready {
                    break;
                }
                assert!(
                    state.reason.is_none() && Instant::now() < deadline,
                    "v3_readiness:{state:?}"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            original
                .run
                .source()
                .observe_tcp()
                .expect("v3_original_tcp_before");
            let protocol = server_catalog::protocol(profile);
            let status = probe_minecraft_status(MinecraftStatusRequest {
                endpoint: producer::ENDPOINT,
                handshake_host: "127.0.0.1".into(),
                handshake_port: 43001,
                protocol,
                timeout: Duration::from_secs(5),
            })
            .await
            .expect("v3_status_pong");
            assert_eq!(status.reported_protocol, protocol);
            assert!(status.protocol_matches_requested);
            let sample = original
                .run
                .voice_source()
                .unwrap()
                .observe()
                .expect("v3_original_voice_configuration_and_udp");
            let descriptor = profile.descriptor();
            assert_eq!(
                (sample.minecraft, sample.loader, sample.loader_version),
                (
                    descriptor.minecraft_version,
                    descriptor.loader,
                    descriptor.loader_version
                )
            );
            assert_eq!(
                sample.announcement,
                producer::VoiceRuntimeAnnouncement::Loopback35554Durable
            );
            let binding = original.run.native_voice_host_binding().unwrap();
            assert_eq!(binding.online_runtime_profile().unwrap(), profile);
            assert_eq!(
                binding.online_voice_policy().unwrap().0,
                match voice {
                    NativeVoiceProfile::SimpleVoiceChat => "svc",
                    NativeVoiceProfile::PlasmoVoice => "plasmo",
                }
            );
            assert_eq!(
                recovery::read_bounded(&paths.copy.join("eula.txt"), 4096).unwrap(),
                EULA_TRUE
            );
            original
                .run
                .source()
                .observe_tcp()
                .expect("v3_original_tcp_after");
        })
    }));
    let completion = original.finish(&runtime);
    let complete = completion
        .process
        .as_ref()
        .is_some_and(|process| process.reaped && process.readers_complete);
    let result = serde_json::json!({"runtime":profile.version_id(),"voice":format!("{voice:?}"),"inspectionPassed":inspection.is_ok(),"originalCompletion":format!("{completion:?}"),"javaLauncherJarUnavailable":original_hooks.java_launcher_jar_unavailable(),"processReapedAndReadersComplete":complete,"voiceAudioTested":false,"worldEntryTested":false,"personalConfigurationChanged":false});
    recovery::write_new(
        &evidence.join("result.json"),
        &serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    assert!(
        inspection.is_ok(),
        "v3_inspection_failed_evidence_preserved"
    );
    assert!(
        complete && completion.reached_ready && completion.failure.is_none(),
        "v3_original_completion:{completion:?}"
    );
    assert_eq!(completion.eula, EulaDisposition::Restored);
    assert_eq!(
        observe_file(&paths.copy.join("eula.txt"), 4096).unwrap(),
        original_eula
    );
    assert_eq!(
        recovery::read_bounded(
            &paths.bundle.join("bundle.json"),
            manifest::MAX_V3_MANIFEST_BYTES
        )
        .unwrap(),
        manifest
    );
    for record in source_records {
        assert_eq!(
            observe_file(&source_stage.join(record.name), record.observation.bytes)
                .unwrap()
                .0,
            record.observation
        );
    }
    let reopened = factory::Bundle::reopen(&lab, id).expect("v3_recovery_only_reopen");
    let guard = crate::engine::acquire_instance_recovery_lock(&reopened.copy).unwrap();
    reopened
        .validate(&guard)
        .expect("v3_physical_immutable_reopen");
}

#[test]
#[ignore = "real Fabric1165/SVC downloads, Java8 server and fixed ports; serial explicit opt-in"]
fn actual_server_v3_fabric1165_svc_original_preparation_status_and_recovery() {
    run(
        ReadyProfile::Fabric1165,
        NativeVoiceProfile::SimpleVoiceChat,
    );
}

#[test]
#[ignore = "real Forge1122/SVC official installer and Java8 server; serial explicit opt-in"]
fn actual_server_v3_forge1122_svc_original_preparation_status_and_recovery() {
    run(ReadyProfile::Forge1122, NativeVoiceProfile::SimpleVoiceChat);
}

#[test]
#[ignore = "real selected supported server/voice pair; serial explicit opt-in, downloads and owned Java"]
fn actual_server_v3_selected_original_preparation_status_and_recovery() {
    let runtime = std::env::var("AURORA_CONNECT_V3_RUNTIME").expect("explicit_runtime_required");
    let profile = ReadyProfile::from_version_id(&runtime).expect("closed_matrix_runtime_required");
    let voice = match std::env::var("AURORA_CONNECT_V3_VOICE").as_deref() {
        Ok("svc") => NativeVoiceProfile::SimpleVoiceChat,
        Ok("plasmo") => NativeVoiceProfile::PlasmoVoice,
        _ => panic!("explicit_supported_voice_required"),
    };
    assert!(
        voice_catalog::artifacts(profile, voice).is_some(),
        "official_voice_pair_required"
    );
    run(profile, voice);
}
