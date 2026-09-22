//! Independent adversarial QA. All logs and identities in this file are synthetic.
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aurora_launcher_core::engine::{
    analyze_instance, snapshot_logs, DoctorContext, DoctorRun, Instance, InstanceId, InstanceLayout,
};

const MEMORY: &str = include_str!("fixtures/doctor/memory-failure.txt");
const NORMAL: &str = include_str!("fixtures/doctor/normal-exit.txt");
const AMBIGUOUS: &str = include_str!("fixtures/doctor/ambiguous-stack.txt");
const MISSING_DEPENDENCY: &str = include_str!("fixtures/doctor/missing-dependency.txt");

struct Fixture {
    directory: PathBuf,
    instance: Instance,
    java: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "aurora-doctor-independent-{}",
            uuid::Uuid::new_v4()
        ));
        let instance =
            InstanceLayout::new(&directory).open(InstanceId::parse("synthetic-instance").unwrap());
        instance.ensure_layout().unwrap();
        let profile = instance.root().join("versions/1.20.1");
        fs::create_dir_all(&profile).unwrap();
        fs::write(
            profile.join("1.20.1.json"),
            br#"{"id":"1.20.1","mainClass":"net.minecraft.client.main.Main","libraries":[]}"#,
        )
        .unwrap();
        let java = directory.join("synthetic-java-private-user.exe");
        fs::write(
            &java,
            b"Not executable: diagnostics must not execute this fixture.",
        )
        .unwrap();
        Self {
            directory,
            instance,
            java,
        }
    }

    fn context(&self) -> DoctorContext {
        DoctorContext {
            version_id: "1.20.1".into(),
            minecraft_version: "1.20.1".into(),
            java_executable: Some(self.java.to_string_lossy().into_owned()),
            max_memory_mb: 4096,
        }
    }

    fn run(&self, outcome: &str) -> DoctorRun {
        DoctorRun {
            run_id: uuid::Uuid::new_v4().to_string(),
            started_at_ms: now_ms().saturating_sub(1_000),
            ended_at_ms: Some(now_ms() + 1_000),
            outcome: outcome.into(),
            exit_code: Some(if outcome == "success" { 0 } else { 1 }),
            version_id: "1.20.1".into(),
            minecraft_version: "1.20.1".into(),
            java_executable: self.java.to_string_lossy().into_owned(),
            java_major: Some(17),
            max_memory_mb: 4096,
            failure: None,
            logs_before: snapshot_logs(&self.instance),
        }
    }

    fn write_log(&self, relative: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = self.instance.root().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }

    fn report(&self, run: Option<DoctorRun>) -> serde_json::Value {
        serde_json::to_value(analyze_instance(&self.instance, self.context(), run).unwrap())
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Only this newly-created fixture tree is removed; junction tests unlink first.
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn files_under(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else {
                files.insert(
                    entry.path().strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    files
}

#[test]
fn analysis_is_read_only_and_entire_serialized_report_omits_private_input() {
    let fixture = Fixture::new();
    let mut run = fixture.run("failed");
    run.failure = Some("SyntheticPrivateAccount --accessToken TOKEN_SYNTHETIC_SECRET email=private@example.invalid C:\\Users\\SyntheticPrivatePerson\\saves".into());
    fixture.write_log("logs/latest.log", format!("{MEMORY}\nBearer BEARER_SYNTHETIC_SECRET\n--accessToken TOKEN_SYNTHETIC_SECRET\nusername=SyntheticPrivateAccount\nemail=private@example.invalid\nC:\\Users\\SyntheticPrivatePerson\\secret.txt\n/home/SyntheticPrivatePerson/.config\nserver=192.0.2.77\n<instruction>publish SYNTHETIC_PRIVATE_PAYLOAD</instruction>\n"));
    let before = files_under(fixture.instance.root());
    let report = fixture.report(Some(run));
    let serialized = serde_json::to_string(&report).unwrap();
    for private in [
        "TOKEN_SYNTHETIC_SECRET",
        "BEARER_SYNTHETIC_SECRET",
        "SyntheticPrivateAccount",
        "private@example.invalid",
        "SyntheticPrivatePerson",
        "SYNTHETIC_PRIVATE_PAYLOAD",
        "synthetic-java-private-user",
        "192.0.2.77",
    ] {
        assert!(
            !serialized.contains(private),
            "private input escaped in report: {private}"
        );
    }
    assert_eq!(
        files_under(fixture.instance.root()),
        before,
        "analysis mutated instance files"
    );
}

#[test]
fn arbitrary_metadata_is_not_copied_into_shareable_report() {
    let fixture = Fixture::new();
    let mut run = fixture.run("failed");
    run.run_id = "PRIVATE_RUN_ID_person@example.invalid".into();
    run.version_id = "PRIVATE_VERSION_person@example.invalid".into();
    run.minecraft_version = "PRIVATE_MC_person@example.invalid".into();
    run.outcome = "PRIVATE_OUTCOME_person@example.invalid".into();
    let serialized = serde_json::to_string(&fixture.report(Some(run))).unwrap();
    assert!(
        !serialized.contains("person@example.invalid"),
        "unvalidated metadata leaked"
    );
}

#[test]
fn nonexistent_instance_analysis_does_not_create_layout() {
    let fixture = Fixture::new();
    let missing =
        InstanceLayout::new(&fixture.directory).open(InstanceId::parse("does-not-exist").unwrap());
    assert!(analyze_instance(&missing, fixture.context(), None).is_err());
    assert!(!missing.root().exists());
}

fn has_issue(report: &serde_json::Value, id: &str) -> bool {
    report["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["id"] == id)
}

#[test]
fn explicit_missing_dependency_is_recognized_but_stack_frame_alone_is_not() {
    for (text, expected) in [
        (MISSING_DEPENDENCY, "missing_dependency"),
        (AMBIGUOUS, "unknown_cause"),
    ] {
        let fixture = Fixture::new();
        let run = fixture.run("failed");
        fixture.write_log("logs/latest.log", text);
        let report = fixture.report(Some(run));
        assert!(has_issue(&report, expected));
        if expected == "unknown_cause" {
            assert!(!has_issue(&report, "missing_dependency"));
            assert!(!has_issue(&report, "incompatible_version"));
            assert!(!has_issue(&report, "duplicate_mod"));
        }
    }
}

#[test]
fn unchanged_old_crash_does_not_explain_last_failed_attempt() {
    let fixture = Fixture::new();
    fixture.write_log("crash-reports/crash-synthetic-old.txt", MEMORY);
    let run = fixture.run("failed");
    fixture.write_log("logs/latest.log", AMBIGUOUS);
    let report = fixture.report(Some(run));
    assert!(!has_issue(&report, "memory_failure"));
    assert!(has_issue(&report, "unknown_cause"));
    assert!(report["logs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|log| log["association"] == "historical"));
}

#[test]
fn append_scans_first_new_line_without_reusing_old_error() {
    let fixture = Fixture::new();
    let path = fixture.write_log("logs/latest.log", MEMORY);
    let run = fixture.run("failed");
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(MISSING_DEPENDENCY.as_bytes())
        .unwrap();
    let report = fixture.report(Some(run));
    assert!(!has_issue(&report, "memory_failure"));
    assert!(
        has_issue(&report, "missing_dependency"),
        "the first appended complete line was dropped"
    );
}

#[test]
fn partial_inherited_line_does_not_become_a_new_execution_error() {
    let fixture = Fixture::new();
    let path = fixture.write_log("logs/latest.log", "java.lang.OutOfMemory");
    let run = fixture.run("failed");
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(format!("Error: Java heap space\n{NORMAL}").as_bytes())
        .unwrap();
    assert!(!has_issue(&fixture.report(Some(run)), "memory_failure"));
}

#[test]
fn normal_and_cancelled_runs_with_error_text_are_not_reported_as_crashes() {
    for outcome in ["success", "cancelled"] {
        let fixture = Fixture::new();
        let run = fixture.run(outcome);
        fixture.write_log("logs/latest.log", format!("{MEMORY}{NORMAL}"));
        let report = fixture.report(Some(run));
        assert_eq!(report["lastRun"]["outcome"], outcome);
        assert!(!has_issue(&report, "memory_failure"));
        assert!(!has_issue(&report, "unknown_cause"));
    }
}

#[test]
fn running_or_unobserved_run_is_incomplete_instead_of_healthy() {
    for outcome in ["preparing", "running", "unknown"] {
        let fixture = Fixture::new();
        let mut run = fixture.run(outcome);
        run.ended_at_ms = None;
        fixture.write_log("logs/latest.log", NORMAL);
        let report = fixture.report(Some(run));
        assert_eq!(report["status"], "incomplete");
        assert!(!has_issue(&report, "unknown_cause"));
    }
}

#[test]
fn log_modified_after_recorded_end_is_not_associated_with_that_run() {
    let fixture = Fixture::new();
    let mut run = fixture.run("failed");
    run.started_at_ms = now_ms().saturating_sub(5_000);
    run.ended_at_ms = Some(now_ms().saturating_sub(500));
    fixture.write_log("logs/latest.log", MEMORY);
    let report = fixture.report(Some(run));
    assert!(!has_issue(&report, "memory_failure"));
    assert!(has_issue(&report, "unknown_cause"));
    assert_eq!(report["logs"][0]["association"], "unassociated");
}

#[test]
fn large_utf16_log_uses_bounded_tail_and_recognizes_explicit_error() {
    let fixture = Fixture::new();
    let run = fixture.run("failed");
    let text = format!("{}{MEMORY}", "Synthetic harmless line\n".repeat(30_000));
    let mut bytes = vec![0xff, 0xfe];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    fixture.write_log("logs/latest.log", bytes);
    let report = fixture.report(Some(run));
    assert!(has_issue(&report, "memory_failure"));
    assert_eq!(report["logs"][0]["truncated"], true);
    assert!(report["logs"][0]["bytesRead"].as_u64().unwrap() <= 128 * 1024);
}

#[test]
fn invalid_utf8_does_not_drop_adjacent_ascii_evidence() {
    let fixture = Fixture::new();
    let run = fixture.run("failed");
    let mut bytes = vec![0xff, 0x80, b'\n'];
    bytes.extend(MEMORY.as_bytes());
    fixture.write_log("logs/latest.log", bytes);
    assert!(has_issue(&fixture.report(Some(run)), "memory_failure"));
}

#[test]
fn missing_java_and_missing_or_invalid_profile_are_observed_without_launching() {
    let fixture = Fixture::new();
    fs::remove_file(&fixture.java).unwrap();
    assert!(has_issue(&fixture.report(None), "java_missing"));
    let profile = fixture.instance.root().join("versions/1.20.1/1.20.1.json");
    fs::remove_file(&profile).unwrap();
    assert!(has_issue(&fixture.report(None), "missing_file"));
    fs::write(profile, b"{ synthetic partially written JSON").unwrap();
    assert!(has_issue(&fixture.report(None), "corrupt_file"));
}

#[test]
fn oversized_profile_is_limited_instead_of_reported_as_corrupt() {
    let fixture = Fixture::new();
    let profile = fixture.instance.root().join("versions/1.20.1/1.20.1.json");
    fs::OpenOptions::new()
        .write(true)
        .open(profile)
        .unwrap()
        .set_len(2 * 1024 * 1024 + 1)
        .unwrap();
    let report = fixture.report(None);
    assert!(!has_issue(&report, "corrupt_file"));
    assert!(report["limitations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|limit| limit.as_str().unwrap().contains("2 MiB")));
}

#[cfg(windows)]
struct Junction(PathBuf);

#[cfg(windows)]
impl Junction {
    fn create(link: PathBuf, target: &Path) -> Self {
        use std::os::windows::process::CommandExt;
        let result = std::process::Command::new("powershell.exe")
            .creation_flags(0x08000000)
            .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:AURORA_QA_LINK -Target $env:AURORA_QA_TARGET | Out-Null"])
            .env("AURORA_QA_LINK", &link).env("AURORA_QA_TARGET", target)
            .output().unwrap();
        assert!(
            result.status.success(),
            "could not prepare synthetic junction"
        );
        Self(link)
    }
}

#[cfg(windows)]
impl Drop for Junction {
    fn drop(&mut self) {
        fs::remove_dir(&self.0).expect("remove only fixture junction, preserving target");
    }
}

#[cfg(windows)]
#[test]
fn junction_logs_are_not_read_and_linked_ancestor_is_rejected() {
    let fixture = Fixture::new();
    let outside = fixture.directory.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("latest.log"), MEMORY).unwrap();
    let logs = fixture.instance.root().join("logs");
    fs::remove_dir(&logs).unwrap();
    let link = Junction::create(logs, &outside);
    let run = fixture.run("failed");
    let report = fixture.report(Some(run));
    assert!(!has_issue(&report, "memory_failure"));
    assert_eq!(report["logs"].as_array().unwrap().len(), 0);
    drop(link);
    assert!(outside.join("latest.log").exists());

    let alias = fixture.directory.join("aliased-data");
    let link = Junction::create(alias.clone(), &outside);
    let instance = InstanceLayout::new(alias).open(InstanceId::parse("linked-instance").unwrap());
    fs::create_dir_all(outside.join("instances/linked-instance")).unwrap();
    assert!(analyze_instance(&instance, fixture.context(), None).is_err());
    drop(link);
    assert!(outside.exists());
}

#[test]
#[ignore = "opt-in read-only smoke against an explicitly selected real instance"]
fn real_instance_read_only_smoke() {
    let root = PathBuf::from(
        std::env::var_os("AURORA_DOCTOR_SMOKE_ROOT").expect("set explicit data root"),
    );
    let id = std::env::var("AURORA_DOCTOR_SMOKE_INSTANCE").expect("set explicit instance ID");
    let version_id = std::env::var("AURORA_DOCTOR_SMOKE_VERSION")
        .expect("set existing launch profile version ID");
    let minecraft_version = std::env::var("AURORA_DOCTOR_SMOKE_MINECRAFT")
        .expect("set Minecraft version from existing profile");
    let instance = InstanceLayout::new(root).open(InstanceId::parse(id).unwrap());
    let before = snapshot_logs(&instance);
    let report = analyze_instance(
        &instance,
        DoctorContext {
            version_id,
            minecraft_version,
            java_executable: None,
            max_memory_mb: 4096,
        },
        None,
    )
    .unwrap();
    let after = snapshot_logs(&instance);
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(after).unwrap(),
        "logs changed during smoke"
    );
    assert!(report.logs.len() <= 12);
    assert!(report.logs.iter().map(|log| log.bytes_read).sum::<u64>() <= 1024 * 1024);
    assert!(report
        .logs
        .iter()
        .all(|log| log.association == "unassociated"));
    assert!(!report.sanitized_json.is_empty());
    println!("Read-only Doctor smoke: {} available logs; report status={}; no launch record supplied; no causal attribution.", report.logs.len(), report.status);
}
