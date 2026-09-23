//! Synthetic, offline recognition tests. No fixture executable is ever run.
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use aurora_launcher_core::engine::{
    analyze_instance, canonical_launch_failure, DoctorContext, DoctorRun, InstanceId,
    InstanceLayout,
};

const FABRIC_MISSING: &str = include_str!("fixtures/doctor_detection/fabric-missing.txt");
const FORGE_MISSING: &str = include_str!("fixtures/doctor_detection/forge-missing.txt");
const FORGE_INCOMPATIBLE: &str = include_str!("fixtures/doctor_detection/forge-incompatible.txt");
const JAVA_VERSION: &str = include_str!("fixtures/doctor_detection/java-version.txt");
const MIXIN: &str = include_str!("fixtures/doctor_detection/mixin-observation.txt");
const AMBIGUOUS: &str = include_str!("fixtures/doctor_detection/ambiguous-negated.txt");

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn report(text: &str) -> serde_json::Value {
    report_input(Some(text), None)
}

fn report_input(text: Option<&str>, failure: Option<&str>) -> serde_json::Value {
    let fixture =
        Fixture(std::env::temp_dir().join(format!("aurora-doctor-rules-{}", uuid::Uuid::new_v4())));
    let instance = InstanceLayout::new(&fixture.0).open(InstanceId::parse("synthetic").unwrap());
    instance.ensure_layout().unwrap();
    let profile = instance.root().join("versions/1.20.1");
    fs::create_dir_all(&profile).unwrap();
    fs::write(profile.join("1.20.1.json"), br#"{"id":"1.20.1"}"#).unwrap();
    let java = fixture.0.join("synthetic-not-executable");
    fs::write(&java, "synthetic").unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    if let Some(text) = text {
        fs::write(instance.root().join("logs/latest.log"), text).unwrap();
    }
    let context = DoctorContext {
        version_id: "1.20.1".into(),
        minecraft_version: "1.20.1".into(),
        java_executable: Some(java.to_string_lossy().into_owned()),
        max_memory_mb: 4096,
    };
    let run = DoctorRun {
        run_id: uuid::Uuid::new_v4().to_string(),
        started_at_ms: now.saturating_sub(1000),
        ended_at_ms: Some(now + 1000),
        outcome: "failed".into(),
        exit_code: Some(1),
        version_id: context.version_id.clone(),
        minecraft_version: context.minecraft_version.clone(),
        java_executable: java.to_string_lossy().into_owned(),
        java_major: Some(17),
        max_memory_mb: 4096,
        failure: failure.map(str::to_owned),
        logs_before: Vec::new(),
    };
    // Exercise the serialized lifecycle record, not only an in-memory string.
    let run: DoctorRun = serde_json::from_slice(&serde_json::to_vec(&run).unwrap()).unwrap();
    serde_json::to_value(analyze_instance(&instance, context, Some(run)).unwrap()).unwrap()
}

fn ids(report: &serde_json::Value) -> Vec<&str> {
    report["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|issue| issue["id"].as_str().unwrap())
        .collect()
}

#[test]
fn fabric_missing_is_specific_and_does_not_duplicate_generic_header() {
    assert_eq!(ids(&report(FABRIC_MISSING)), ["missing_dependency"]);
}

#[test]
fn forge_absence_and_installed_wrong_version_are_distinct() {
    assert_eq!(ids(&report(FORGE_MISSING)), ["missing_dependency"]);
    assert_eq!(ids(&report(FORGE_INCOMPATIBLE)), ["incompatible_version"]);
    assert_eq!(
        ids(&report("Missing or unsupported mandatory dependencies:\n")),
        ["unknown_cause"]
    );
}

#[test]
fn fabric_explicit_old_new_plural_and_environment_mismatch_formats() {
    for text in [
        "Mod 'Synthetic' requires version 1.20.1 of Minecraft, but a non-matching version is present: 1.19.2!",
        "Mod 'Synthetic' requires version 2 of library, but only the wrong versions are present: 1, 3!",
        "Mod 'Synthetic' requires any version of library, which is disabled for this environment (client/server only)!",
        "Mod 'Synthetic' is incompatible with any version of library, yet a conflicting version is present: 1!",
        "File synthetic.jar is a Fabric mod and cannot be loaded",
    ] { assert_eq!(ids(&report(text)), ["incompatible_version"], "{text}"); }
}

#[test]
fn forge_crash_pair_is_adjacent_and_refers_to_same_dependency() {
    for (status, expected) in [
        ("not installed", "missing_dependency"),
        ("1.0", "incompatible_version"),
    ] {
        assert_eq!(ids(&report(&format!("Failure message: Mod §esynthetic§r requires §6library§r §o2.0 or above§r\n§7Currently, §6library§r§7 is §o{status}"))), [expected]);
    }
    for text in [
        "Failure message: Mod synthetic requires library 2.0\nCurrently, different is not installed",
        "Failure message: Mod synthetic requires library 2.0\nUnrelated line\nCurrently, library is not installed",
        "Mod synthetic requires library 2.0\nCurrently, library is not installed",
    ] { assert_eq!(ids(&report(text)), ["unknown_cause"]); }
}

#[test]
fn java_class_file_numbers_are_safe_and_actionable() {
    let result = report(JAVA_VERSION);
    assert_eq!(ids(&result), ["java_incompatible"]);
    let evidence = result["issues"][0]["evidence"][0]["summary"]
        .as_str()
        .unwrap();
    assert!(evidence.contains("Java 21") && evidence.contains("Java 17"));
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("PrivateAccount"));
    let asm = report("java.lang.IllegalArgumentException: Unsupported class file major version 65");
    assert_eq!(ids(&asm), ["incompatible_version"]);
    assert!(asm["issues"][0]["evidence"][0]["summary"]
        .as_str()
        .unwrap()
        .contains("leitor de bytecode"));
}

#[test]
fn mixin_is_an_observation_and_keeps_root_cause_unknown() {
    let result = report(MIXIN);
    assert_eq!(ids(&result), ["mixin_failure", "unknown_cause"]);
    assert_eq!(result["issues"][0]["confidence"], "medium");
    assert_eq!(result["issues"][0]["kind"], "fact");
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("SyntheticPrivateMixin"));
}

#[test]
fn graphics_requires_an_explicit_context_failure_not_a_driver_name() {
    for text in [
        "GLFW error 65542: WGL: The driver does not appear to support OpenGL",
        "WGL: Failed to create OpenGL context",
        "org.lwjgl.LWJGLException: Pixel format not accelerated",
    ] {
        assert_eq!(ids(&report(text)), ["graphics_failure"]);
    }
    assert_eq!(ids(&report("Problematic frame: synthetic_gpu_driver.dll\nOpenGL vendor: synthetic\nGLFW version: 3.3")), ["unknown_cause"]);
}

#[test]
fn disk_and_access_errors_are_distinct_from_memory_and_network() {
    for (text, expected) in [
        ("java.io.IOException: No space left on device", "disk_space"),
        ("falha de E/S em C:\\SyntheticPrivatePath: Não há espaço suficiente no disco. (os error 112)", "disk_space"),
        ("java.nio.file.FileSystemException: C:\\SyntheticPrivatePath: There is not enough space on the disk", "disk_space"),
        ("java.nio.file.AccessDeniedException: C:\\SyntheticPrivatePath", "file_permission"),
        ("java.io.FileNotFoundException: C:\\SyntheticPrivatePath (Access is denied)", "file_permission"),
        ("java.io.IOException: Permission denied", "file_permission"),
        ("falha de E/S em C:\\SyntheticPrivatePath: Acesso negado. (os error 5)", "file_permission"),
        ("java.nio.file.NoSuchFileException: C:\\SyntheticPrivatePath", "missing_file"),
    ] {
        let result = report(text);
        assert_eq!(ids(&result), [expected], "{text}");
        assert!(!serde_json::to_string(&result).unwrap().contains("SyntheticPrivatePath"));
    }
}

#[test]
fn unrelated_negated_optional_and_stack_mentions_do_not_assign_causes() {
    assert_eq!(ids(&report(AMBIGUOUS)), ["unknown_cause"]);
}

#[test]
fn explicit_network_error_stays_an_observation_without_causal_claim() {
    assert_eq!(
        ids(&report(
            "javax.net.ssl.SSLHandshakeException: synthetic host"
        )),
        ["network_failure", "unknown_cause"]
    );
}

#[test]
fn canonical_preparation_failures_are_private_and_idempotent() {
    for text in [
        FABRIC_MISSING,
        FORGE_MISSING,
        FORGE_INCOMPATIBLE,
        JAVA_VERSION,
        MIXIN,
        "GLFW error 65542: WGL: The driver does not appear to support OpenGL",
        "java.io.IOException: No space left on device",
        "java.nio.file.AccessDeniedException: C:\\PrivateAccount\\TOKEN_SECRET",
        "integridade inválida para C:\\PrivateAccount\\TOKEN_SECRET",
    ] {
        let canonical = canonical_launch_failure(text);
        assert_ne!(
            canonical, "Falha de preparação sem causa identificada.",
            "{text}"
        );
        assert!(!canonical.contains("PrivateAccount") && !canonical.contains("TOKEN_SECRET"));
        assert_eq!(canonical_launch_failure(&canonical), canonical);
    }
    assert_eq!(
        canonical_launch_failure(AMBIGUOUS),
        "Falha de preparação sem causa identificada."
    );
}

#[test]
fn persisted_alpha11_dependency_failure_is_preserved_without_logs() {
    let legacy = "Dependência obrigatória ausente ou não atendida.";
    assert_eq!(canonical_launch_failure(legacy), legacy);
    let result = report_input(None, Some(legacy));
    assert!(result["logs"].as_array().unwrap().is_empty());
    assert_eq!(ids(&result), ["missing_dependency"]);
    assert_eq!(
        result["issues"][0]["title"],
        "Dependência obrigatória não atendida"
    );
    assert_eq!(result["issues"][0]["confidence"], "medium");
    assert!(result["issues"][0]["explanation"]
        .as_str()
        .unwrap()
        .contains("sem distinguir"));
    assert_eq!(
        canonical_launch_failure(&format!("{legacy} PrivateAccount")),
        "Falha de preparação sem causa identificada."
    );
}
