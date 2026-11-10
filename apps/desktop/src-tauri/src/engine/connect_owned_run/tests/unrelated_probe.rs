use super::*;
use std::{
    fs,
    os::windows::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
};

struct Roots(std::path::PathBuf);
impl Roots {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("aurora-owned-probe-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("owned/nested")).unwrap();
        fs::create_dir_all(root.join("sibling")).unwrap();
        Self(root)
    }
}
impl Drop for Roots {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn probe(root: &Path, environment: &str, rows: serde_json::Value) -> String {
    // Exercise the actual production PowerShell predicate. Only its CIM data
    // source is replaced; StartTime still comes from the real owned OS process.
    let source = include_str!("../../../commands.rs");
    let script = source
        .split("const INSTANCE_GAME_PROBE: &str = r#\"")
        .nth(1)
        .unwrap()
        .split("\"#;")
        .next()
        .unwrap()
        .replace(
            "Get-CimInstance Win32_Process -Filter \"Name='java.exe' OR Name='javaw.exe'\"",
            "ConvertFrom-Json $env:AURORA_QA_GAMES",
        );
    let mut child = Command::new("powershell.exe")
        .creation_flags(0x0800_0000)
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .env("AURORA_MIGRATION_INSTANCE", root)
        .env("AURORA_OWNED_DEDICATED_PROBE", environment)
        .env("AURORA_QA_GAMES", rows.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let end = Instant::now() + Duration::from_secs(15);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= end {
            let _ = child.kill();
            let _ = child.wait();
            panic!("owned_probe_child_timeout");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    if output.status.success() {
        String::from_utf8(output.stdout).unwrap().trim().into()
    } else {
        "UNKNOWN".into()
    }
}

fn knot(pid: u32) -> serde_json::Value {
    serde_json::json!([{"ProcessId":pid,"CommandLine":"java -cp fixture net.fabricmc.loader.impl.launch.knot.KnotServer"}])
}

#[tokio::test]
async fn owned_dedicated_probe_excludes_only_disjoint_receipted_live_process() {
    let roots = Roots::new();
    let owned = roots.0.join("owned");
    let target = roots.0.join("sibling");
    let mut harness = Harness::new();
    let mut command = harness.command("normal");
    command.current_dir(&owned);
    let run = worker::start_receipted(
        command,
        "owned-probe",
        uuid::Uuid::new_v4(),
        OwnedRunLimits {
            lifetime: Duration::from_secs(60),
            stop_grace: Duration::from_secs(2),
            output_bytes: 65536,
        },
        CancellationToken::new(),
        Arc::new(test_support::TestHooks::default()),
    )
    .unwrap_or_else(|_| panic!("owned_probe_start"));
    let bindings = harness.ready();
    let scope = owned_dedicated_probe_scope(&target);
    let environment = scope.child_environment().unwrap();
    let rows: serde_json::Value = serde_json::from_str(&environment).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    let pid = rows[0]["pid"].as_u64().unwrap() as u32;
    // Exact original regression: without the typed native exclusion a dedicated
    // server without --gameDir makes even this unrelated empty root UNKNOWN.
    assert_eq!(probe(&target, "[]", knot(pid)), "UNKNOWN");
    assert_eq!(probe(&target, &environment, knot(pid)), "STOPPED");
    scope.recheck().unwrap();
    for root in [
        &owned,
        &roots.0,
        &owned.join("nested"),
        &std::path::PathBuf::from(owned.to_string_lossy().to_uppercase()),
    ] {
        assert_eq!(
            owned_dedicated_probe_scope(root)
                .child_environment()
                .unwrap(),
            "[]"
        );
        assert_eq!(probe(root, "[]", knot(pid)), "UNKNOWN");
    }
    let mut wrong_birth = rows.clone();
    wrong_birth[0]["birth"] = (rows[0]["birth"].as_str().unwrap().parse::<u64>().unwrap() + 1)
        .to_string()
        .into();
    assert_eq!(
        probe(&target, &wrong_birth.to_string(), knot(pid)),
        "UNKNOWN"
    );
    let unowned = serde_json::json!([
        {"ProcessId":pid,"CommandLine":"java net.fabricmc.loader.impl.launch.knot.KnotServer"},
        {"ProcessId":1,"CommandLine":"java net.fabricmc.loader.impl.launch.knot.KnotClient"},
    ]);
    assert_eq!(probe(&target, &environment, unowned), "UNKNOWN");
    assert_eq!(
        probe(
            &target,
            &environment,
            serde_json::json!([{"ProcessId":1,"CommandLine":null}])
        ),
        "UNKNOWN"
    );
    assert!(
        fs::rename(&owned, roots.0.join("replacement")).is_err(),
        "original root remains pinned"
    );
    let completion = run.request_stop();
    assert!(
        scope.recheck().is_err(),
        "terminal owner immediately invalidates the scope"
    );
    let result = report(&completion).await;
    normal(&result);
    assert!(scope.child_environment().is_err());
    assert_eq!(
        probe(&target, &environment, knot(pid)),
        "UNKNOWN",
        "stale environment cannot exempt an exited child"
    );
    bindings.assert_released();
    harness.finish();
    drop((scope, run));
}

#[tokio::test]
async fn owned_dedicated_probe_never_excludes_generic_owned_run() {
    let roots = Roots::new();
    let mut harness = Harness::new();
    let mut command = harness.command("normal");
    command.current_dir(roots.0.join("owned"));
    let run = worker::start(
        command,
        "generic-probe",
        OwnedRunLimits {
            lifetime: Duration::from_secs(20),
            stop_grace: Duration::from_secs(1),
            output_bytes: 65536,
        },
        CancellationToken::new(),
        Arc::new(test_support::TestHooks::default()),
    )
    .unwrap();
    let bindings = harness.ready();
    assert_eq!(
        owned_dedicated_probe_scope(&roots.0.join("sibling"))
            .child_environment()
            .unwrap(),
        "[]"
    );
    normal(&report(&run.request_stop()).await);
    bindings.assert_released();
    harness.finish();
}
