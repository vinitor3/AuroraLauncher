//! Explicit file-only opt-in with installed pinned resources. Both approvals
//! are reviewed by the local operator; neither JSON document executes a plan.
use super::*;
use std::{thread, time::Instant};

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

fn resources(stage: &Path, jdk: &Path) -> serde_json::Value {
    let mut files = Vec::new();
    for name in ARTIFACT_NAMES {
        files.push(serde_json::json!({
            "name":name,
            "observed":observe_file(&stage.join(name),64*1024*1024).unwrap().0,
        }));
    }
    for (name, path, limit) in [
        ("jdk/release", jdk.join("release"), 64 * 1024),
        ("jdk/java.exe", jdk.join("bin/java.exe"), 4 * 1024 * 1024),
    ] {
        files.push(serde_json::json!({
            "name":name,"observed":observe_file(&path,limit).unwrap().0,
        }));
    }
    serde_json::json!({
        "files":files,
        "serverPublishedSha1":"59353fb40c36d304f2035d51e7d6e6baa98dc05c",
        "serverSha256Meaning":"observed_only_not_independent_official_digest",
    })
}

fn review_file(root: &Path, name: &str, review: &Review, extra: serde_json::Value) {
    let path = root.join(name);
    let bytes = serde_json::to_vec_pretty(&serde_json::json!({
        "schema":1,
        "reviewId":review.review_id,
        "purpose":review.purpose,
        "bundleId":review.bundle_id,
        "digest":review.digest,
        "before":review.before,
        "after":review.after,
        "details":review.details,
        "javaWillExecute":false,
        "externalNetwork":false,
        "personalConfigurationChanged":false,
        "approvalTtlSeconds":60,
        "extra":extra,
    }))
    .unwrap();
    recovery::write_new(&path, &bytes).expect("actual_review_create_new");
    println!("native_eula_review_ready:{}", path.display());
}

fn approval(root: &Path, name: &str, digest: &str) -> String {
    let path = root.join(name);
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                let bytes = recovery::read_bounded(&path, 128).expect("actual_approval_read");
                let approval = std::str::from_utf8(&bytes)
                    .expect("actual_approval_utf8")
                    .trim();
                assert_eq!(approval.len(), 64, "actual_approval_length");
                assert!(approval.bytes().all(|byte| byte.is_ascii_hexdigit()));
                assert_eq!(approval, digest, "actual_approval_mismatch");
                return approval.to_owned();
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => panic!("actual_approval_metadata"),
        }
        assert!(Instant::now() < deadline, "actual_approval_deadline");
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
#[ignore = "explicit real pinned resources and two concrete local approvals; no Java or download"]
fn actual_pinned_resources_and_eula_roundtrip_without_java() {
    let stage =
        PathBuf::from(std::env::var_os("AURORA_DEDICATED_LAB_STAGE").expect("actual_stage"));
    let jdk = PathBuf::from(std::env::var_os("AURORA_DEDICATED_LAB_JDK").expect("actual_jdk"));
    let evidence = lab_root()
        .unwrap()
        .parent()
        .unwrap()
        .join("connect-validation")
        .join(format!("native-eula84a-review-{}", Uuid::new_v4()));
    fs::create_dir(&evidence).expect("actual_evidence_create_new");
    let _pins = recovery::pin_directory(&evidence).unwrap();
    let plan = plan_status_factory(&stage, &jdk, true).expect("actual_factory_plan");
    let resources_before = resources(&stage, &jdk);
    review_file(
        &evidence,
        "factory-review.json",
        plan.review(),
        resources_before.clone(),
    );
    let digest = approval(&evidence, "factory-approve.txt", &plan.review().digest);
    let prepared = plan.create(&digest).expect("actual_factory_create");
    let paths = prepared.test_paths();
    let source_before = files::tree(&paths.origin);
    let source_runtime = observe_file(&paths.origin.join("aurora-instance.json"), 1024)
        .unwrap()
        .0;
    let copy_runtime = observe_file(&paths.copy.join("aurora-instance.json"), 1024)
        .unwrap()
        .0;
    review_file(
        &evidence,
        "eula-review.json",
        prepared.review(),
        serde_json::json!({
            "sourceRuntime":source_runtime,
            "copyRuntime":copy_runtime,
            "target":target(&paths),
            "transaction":paths.transaction,
            "resources":resources_before,
            "onlyOperations":["claim","snapshot","apply_eula_true","restore_original_eula_false"],
            "processWillSpawn":false,
        }),
    );
    let digest = approval(&evidence, "eula-approve.txt", &prepared.review().digest);
    let applied = prepared.apply_eula(&digest).expect("actual_eula_apply");
    assert_eq!(bytes(&target(&paths)), EULA_TRUE);
    assert_eq!(bytes(&slot(&paths, "snapshot.bin")), EULA_FALSE);
    assert_eq!(
        applied.restore_before_spawn().unwrap(),
        EulaOutcome::Restored
    );
    assert_eq!(bytes(&target(&paths)), EULA_FALSE);
    assert_eq!(bytes(&slot(&paths, "applied.bin")), EULA_TRUE);
    assert!(slot(&paths, "claim.json").is_file());
    assert_eq!(files::tree(&paths.origin), source_before);
    assert_eq!(resources(&stage, &jdk), resources_before);
    unlocked(&paths);
    let result = serde_json::json!({
        "schema":1,"event":"native_eula_actual_resources_passed",
        "bundle":paths.bundle,"copy":paths.copy,
        "eulaRestored":true,"claimConsumed":true,"sourceUnchanged":true,
        "resourcesUnchanged":true,"lockReleased":true,"javaExecuted":false,
        "minecraftStatusTested":false,"dedicatedProducerImplemented":false,
        "personalConfigurationChanged":false,"externalNetworkTested":false,
        "voiceAudioTested":false,"publicRelayEnabled":false,
    });
    recovery::write_new(
        &evidence.join("result.json"),
        &serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    println!("{result}");
}
