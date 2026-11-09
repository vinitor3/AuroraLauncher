//! Original-process fixtures exercise Win32 Job ownership, without Java/game.
use super::*;
use std::{
    fs::OpenOptions,
    os::windows::fs::OpenOptionsExt,
    process::{Command, Stdio},
};

const CHILD: &str = "engine::connect_owned_process::tests::owned_job_fixture_child";
const GRANDCHILD: &str = "engine::connect_owned_process::tests::owned_job_fixture_grandchild";
fn fixture_args(name: &str) -> Vec<OsString> {
    [
        "--ignored",
        "--exact",
        name,
        "--nocapture",
        "--test-threads=1",
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

#[test]
fn owned_installer_job_reaps_descendants_on_success_cancel_deadline_and_panic() {
    for mode in [
        "success",
        "orphan",
        "cancel",
        "deadline",
        "panic",
        "beforeSpawn",
        "queryFailure",
    ] {
        let root = std::env::temp_dir().join(format!(
            "aurora-owned-installer-job-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("fixture-mode"), mode).unwrap();
        let start = Instant::now();
        let ready = root.join("descendant.ready");
        let fault_started = std::sync::atomic::AtomicBool::new(false);
        let pending_proved = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observer = if mode == "queryFailure" {
            let root = root.clone();
            let pending_proved = pending_proved.clone();
            Some(thread::spawn(move || {
                let limit = Instant::now() + Duration::from_secs(8);
                while !root.join("descendant.ready").exists() && Instant::now() < limit {
                    thread::sleep(Duration::from_millis(10));
                }
                thread::sleep(Duration::from_millis(1400));
                assert!(
                    OpenOptions::new()
                        .read(true)
                        .write(true)
                        .share_mode(0)
                        .open(root.join("descendant.lock"))
                        .is_err(),
                    "failed termination must retain the actual live descendant"
                );
                thread::sleep(Duration::from_millis(1100));
                assert!(
                    OpenOptions::new()
                        .read(true)
                        .write(true)
                        .share_mode(0)
                        .open(root.join("caller-guard"))
                        .is_err(),
                    "unknown Job status must retain the caller guard"
                );
                pending_proved.store(true, std::sync::atomic::Ordering::SeqCst);
            }))
        } else {
            None
        };
        let caller_guard = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(root.join("caller-guard"))
            .unwrap();
        let cancelled = || {
            if mode == "beforeSpawn" {
                return true;
            }
            if ready.is_file() {
                if mode == "queryFailure"
                    && !fault_started.swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    COLLECTION_FAULTS.with(|faults| {
                        *faults.borrow_mut() = CollectionFaults {
                            query_until: Some(Instant::now() + Duration::from_millis(3100)),
                            terminate_until: Some(Instant::now() + Duration::from_millis(2200)),
                        }
                    });
                }
                if mode == "panic" {
                    panic!("intentional original installer callback panic");
                }
                return matches!(mode, "cancel" | "queryFailure");
            }
            false
        };
        let timeout = if mode == "deadline" {
            Duration::from_secs(4)
        } else {
            Duration::from_secs(12)
        };
        let result = run_owned_installer(
            &std::env::current_exe().unwrap(),
            &fixture_args(CHILD),
            &root,
            &root.join("home"),
            timeout,
            &cancelled,
            false,
        );
        COLLECTION_FAULTS.with(|faults| *faults.borrow_mut() = CollectionFaults::default());
        if let Some(observer) = observer {
            observer.join().unwrap();
            assert!(pending_proved.load(std::sync::atomic::Ordering::SeqCst));
            assert!(start.elapsed() >= Duration::from_millis(3100));
        }
        drop(caller_guard);
        assert_eq!(
            result,
            match mode {
                "success" | "orphan" => Ok(()),
                "deadline" => Err(OwnedInstallerError::Deadline),
                "panic" => Err(OwnedInstallerError::Panic),
                _ => Err(OwnedInstallerError::Cancelled),
            }
        );
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "normal Win32 collection must finish promptly"
        );
        if mode == "beforeSpawn" {
            assert!(!ready.exists());
        } else {
            assert!(
                ready.exists(),
                "the actual descendant must have reached its held file"
            );
            // The grandchild holds this with no sharing while alive. A return
            // from the owner is valid only once that original handle is gone.
            drop(
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .share_mode(0)
                    .open(root.join("descendant.lock"))
                    .expect("descendant handle released before completion"),
            );
        }
        let canonical = root.canonicalize().unwrap();
        assert_eq!(
            canonical.parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
#[ignore = "only the own Job fixture parent may run this child"]
#[allow(
    clippy::zombie_processes,
    reason = "the outer Windows Job owns and collects the intentionally orphaned descendant"
)]
fn owned_job_fixture_child() {
    let root = std::env::current_dir().unwrap();
    let Ok(mode) = fs::read_to_string(root.join("fixture-mode")) else {
        return;
    };
    assert!([
        "success",
        "orphan",
        "cancel",
        "deadline",
        "panic",
        "queryFailure"
    ]
    .contains(&mode.as_str()));
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(fixture_args(GRANDCHILD))
        .current_dir(&root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    if mode != "orphan" {
        assert!(child.wait().unwrap().success());
    }
}

#[test]
#[ignore = "only the original fixture child may run this descendant"]
fn owned_job_fixture_grandchild() {
    let root = std::env::current_dir().unwrap();
    let Ok(mode) = fs::read_to_string(root.join("fixture-mode")) else {
        return;
    };
    let _held = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(root.join("descendant.lock"))
        .unwrap();
    fs::write(root.join("descendant.ready"), b"ready").unwrap();
    thread::sleep(if matches!(mode.as_str(), "success" | "orphan") {
        Duration::from_millis(100)
    } else {
        Duration::from_secs(10)
    });
}

#[test]
fn owned_installer_argv_quotes_literal_whitespace_quotes_and_backslashes() {
    for (input, expected) in [
        ("", "\"\""),
        ("a b", "\"a b\""),
        ("a\"b", "\"a\\\"b\""),
        ("tail\\", "\"tail\\\\\""),
        ("$(literal)`", "\"$(literal)`\""),
    ] {
        assert_eq!(
            argument(OsStr::new(input)).unwrap(),
            expected.encode_utf16().collect::<Vec<_>>()
        );
    }
    assert_eq!(
        wide(OsStr::new("bad\0value")),
        Err(OwnedInstallerError::Invalid)
    );
}
