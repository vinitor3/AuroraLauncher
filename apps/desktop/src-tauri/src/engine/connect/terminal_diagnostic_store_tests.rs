use super::*;
use std::path::PathBuf;
fn fixture() -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("aurora-connect-terminal-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    root
}
fn cleanup(root: &Path) {
    assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
    assert!(root
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("aurora-connect-terminal-"));
    fs::remove_dir_all(root).unwrap();
}
fn bytes() -> Vec<u8> {
    serde_json::to_vec(
        &serde_json::json!({"schema":1,"observedAtMs":1,"role":"host","phase":"failed",
        "errorCode":"connect_social_continuity_lost","failureStage":"sessionContinuity",
        "observation":{"gate":"hostPoll","bootstrapRoundtripMs":702,"bootstrapUncertaintyMs":352,
        "hostPollElapsedMs":1756,"hostPollAgeMs":3009,"sessionRemainingMs":14086000}}),
    )
    .unwrap()
}
#[test]
fn connect_terminal_journal_is_bounded_and_preserves_unsafe_existing_files() {
    let root = fixture();
    let directory = root.join("doctor-runs/connect");
    let data = bytes();
    persist(&directory, &data).unwrap();
    let destination = directory.join("last-terminal.json");
    assert_eq!(fs::read(&destination).unwrap(), data);
    let mut other: serde_json::Value = serde_json::from_slice(&data).unwrap();
    other["observedAtMs"] = 2.into();
    let updated = serde_json::to_vec(&other).unwrap();
    persist(&directory, &updated).unwrap();
    assert_eq!(fs::read(&destination).unwrap(), updated);
    assert!(persist(&directory, &vec![b' '; MAX_BYTES + 1]).is_err());
    assert_eq!(fs::read(&destination).unwrap(), updated);
    other["token"] = "never-record-this".into();
    assert!(persist(&directory, &serde_json::to_vec(&other).unwrap()).is_err());
    assert_eq!(fs::read(&destination).unwrap(), updated);
    fs::write(&destination, b"unrelated original content").unwrap();
    assert!(persist(&directory, &data).is_err());
    assert_eq!(
        fs::read(&destination).unwrap(),
        b"unrelated original content"
    );
    fs::write(&destination, &data).unwrap();
    let alias = root.join("preserved-alias");
    fs::hard_link(&destination, &alias).unwrap();
    assert!(persist(&directory, &updated).is_err());
    assert_eq!(fs::read(&alias).unwrap(), data);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    cleanup(&root);
}

#[cfg(windows)]
#[test]
fn connect_terminal_journal_refuses_junction_without_touching_destination() {
    use std::os::windows::process::CommandExt;
    let root = fixture();
    let outside = root.join("outside");
    let link = root.join("connect");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("sentinel"), b"preserve").unwrap();
    let output = std::process::Command::new("powershell.exe").creation_flags(0x08000000)
        .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:AURORA_CONNECT_TEST_LINK -Target $env:AURORA_CONNECT_TEST_TARGET | Out-Null"])
        .env("AURORA_CONNECT_TEST_LINK", &link).env("AURORA_CONNECT_TEST_TARGET", &outside).output().unwrap();
    assert!(output.status.success());
    assert!(persist(&link, &bytes()).is_err());
    assert!(persist(&link.join("missing-child/connect"), &bytes()).is_err());
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
    assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"preserve");
    fs::remove_dir(link).unwrap();
    cleanup(&root);
}
