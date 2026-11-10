// Test-only process selection never becomes a production IPC authority API.
use super::*;

fn compiled() -> SocialLabAuthority {
    let mut authority = SocialLabAuthority::from_local_process(
        &lab::generate_key().public().to_string(),
        &"11".repeat(32),
    )
    .unwrap();
    authority.online_origin = Some("https://aurora-connect-social.aurora-edge-proxy.workers.dev");
    authority.private_invitations = true;
    authority
}

#[test]
fn social_desktop_online_mode_requires_exact_gate_and_rejects_mixed_authority() {
    for mode in [Some(""), Some("true"), Some("2")] {
        assert!(desktop_fixture_authority(mode, None, None, None, || panic!(
            "must not construct authority"
        ))
        .is_err());
    }
    for (pin, issuer, lab) in [
        (Some(""), None, None),
        (None, Some(""), None),
        (None, None, Some("0")),
        (None, None, Some("1")),
    ] {
        assert!(
            desktop_fixture_authority(Some("1"), pin, issuer, lab, || panic!(
                "must not construct authority"
            ))
            .is_err()
        );
    }
    let key = lab::generate_key().public().to_string();
    for mode in [None, Some("0")] {
        let (online, authority) =
            desktop_fixture_authority(mode, Some(&key), Some(&"11".repeat(32)), None, || {
                panic!("local mode cannot construct online authority")
            })
            .unwrap();
        assert!(!online && authority.online_origin.is_none());
    }
    assert!(desktop_fixture_authority(None, None, None, None, || Ok(compiled())).is_err());
}

#[cfg(all(windows, debug_assertions))]
#[test]
fn social_desktop_online_mode_accepts_only_compiled_enabled_service_and_closed_commands() {
    let (online, authority) =
        desktop_fixture_authority(Some("1"), None, None, None, || Ok(compiled())).unwrap();
    assert!(online && authority.private_invitations_enabled());
    for changed in 0..3 {
        let mut wrong = compiled();
        match changed {
            0 => wrong.online_origin = Some("https://unapproved.invalid"),
            1 => wrong.online_origin = None,
            _ => wrong.private_invitations = false,
        }
        assert!(desktop_fixture_authority(Some("1"), None, None, None, || Ok(wrong)).is_err());
    }
    for command in [
        "readyAdvanceEpoch",
        "guardProbeArm",
        "readyPrepare",
        "authority",
        "",
    ] {
        assert!(!desktop_fixture_command_allowed(true, command));
    }
    for command in [
        "discoverFixture",
        "reserveDiscovered",
        "authenticate",
        "snapshot",
        "stop",
        "quit",
    ] {
        assert!(desktop_fixture_command_allowed(true, command));
    }
    assert!(desktop_fixture_command_allowed(false, "readyPrepare"));
}

#[cfg(all(windows, debug_assertions))]
#[test]
#[ignore = "requires compiled public origin and private-online flag; sends no credentials and makes no HTTP requests"]
fn social_desktop_online_process_starts_without_pin_refuses_injection_and_closes() {
    use std::{
        io::Write,
        os::windows::process::CommandExt,
        process::{Command, Stdio},
    };
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "engine::connect::live::social::tests::social_desktop_process",
            "--nocapture",
        ])
        .env("AURORA_SOCIAL_DESKTOP_FIXTURE", "1")
        .env("AURORA_SOCIAL_DESKTOP_ONLINE_FIXTURE", "1")
        .env_remove("AURORA_SOCIAL_LAB_PIN")
        .env_remove("AURORA_SOCIAL_LAB_ISSUER")
        .env_remove("AURORA_SOCIAL_PRIVATE_LAB")
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for command in ["readyAdvanceEpoch", "snapshot", "quit"] {
        writeln!(input, "{}", json!({"command":command})).unwrap();
    }
    drop(input);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut forced = false;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            forced = true;
            child.kill().unwrap();
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        !forced && output.status.success(),
        "owned native validation process must exit normally"
    );
    let values = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("SOCIAL89_JSON:"))
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 4);
    assert_eq!(values[0]["mode"], "compiledOnline");
    assert_eq!(values[0]["capabilities"]["privateInvitations"], true);
    assert_eq!(values[1]["ok"], false);
    assert_eq!(values[2]["value"]["phase"], "idle");
    assert_eq!(values[3]["value"]["cleanup"], true);
}
