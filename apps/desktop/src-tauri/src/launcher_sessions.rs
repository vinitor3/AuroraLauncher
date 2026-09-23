//! Keep the remembered account owned by one launcher while other windows use
//! independent, temporary Firebase sessions. The lock contains no account data.
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use tauri::Manager;

const SESSION_ERROR: &str = "Não foi possível preparar uma sessão independente do launcher.";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherAuthScope {
    id: String,
    persistent: bool,
}

impl LauncherAuthScope {
    fn temporary() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            persistent: false,
        }
    }
}

pub struct LauncherSessions {
    main: LauncherAuthScope,
    windows: Mutex<BTreeMap<String, LauncherAuthScope>>,
    // Never remove the lock file: replacing its inode could elect two owners.
    // Closing the process (including a crash) releases the OS lock automatically.
    _primary: Option<File>,
}

#[derive(Default)]
pub struct LauncherSessionState(Mutex<Option<LauncherSessions>>);

impl LauncherSessions {
    pub fn acquire(data_directory: &Path) -> Result<Self, String> {
        let directory = data_directory.join("launcher-sessions");
        let path = directory.join("primary.lock");
        crate::doctor_runs::check_path(&path).map_err(|_| SESSION_ERROR.to_owned())?;
        fs::create_dir_all(&directory).map_err(|_| SESSION_ERROR.to_owned())?;
        crate::doctor_runs::check_path(&path).map_err(|_| SESSION_ERROR.to_owned())?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|_| SESSION_ERROR.to_owned())?;
        let primary = match file.try_lock() {
            Ok(()) => Some(file),
            Err(fs::TryLockError::WouldBlock) => None,
            Err(fs::TryLockError::Error(_)) => return Err(SESSION_ERROR.to_owned()),
        };
        let main = if primary.is_some() {
            LauncherAuthScope {
                id: "primary".into(),
                persistent: true,
            }
        } else {
            LauncherAuthScope::temporary()
        };
        Ok(Self {
            main,
            windows: Mutex::new(BTreeMap::new()),
            _primary: primary,
        })
    }

    fn for_window(&self, label: &str) -> Result<LauncherAuthScope, String> {
        if label == "main" {
            return Ok(self.main.clone());
        }
        let mut windows = self.windows.lock().map_err(|_| SESSION_ERROR.to_owned())?;
        Ok(windows
            .entry(label.into())
            .or_insert_with(LauncherAuthScope::temporary)
            .clone())
    }
}

#[tauri::command]
pub fn launcher_auth_scope(
    window: tauri::WebviewWindow,
    state: tauri::State<LauncherSessionState>,
) -> Result<LauncherAuthScope, String> {
    // The native caller supplies the label; JavaScript cannot select another
    // window's persistence scope or promote itself to the remembered account.
    let mut sessions = state.0.lock().map_err(|_| SESSION_ERROR.to_owned())?;
    if sessions.is_none() {
        let directory = window
            .app_handle()
            .path()
            .app_data_dir()
            .map_err(|_| SESSION_ERROR.to_owned())?;
        *sessions = Some(LauncherSessions::acquire(&directory)?);
    }
    // A setup error is returned to the startup UI. A retry can acquire the
    // scope after the filesystem is repaired without falling back to shared auth.
    sessions
        .as_ref()
        .ok_or_else(|| SESSION_ERROR.to_owned())?
        .for_window(window.label())
}

#[cfg(test)]
mod tests {
    use super::LauncherSessions;
    use std::fs;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "aurora-launcher-session-test-{}",
                uuid::Uuid::new_v4()
            )))
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn additional_launchers_are_independent_and_primary_is_restored_after_exit() {
        let fixture = Fixture::new();
        let primary = LauncherSessions::acquire(&fixture.0).unwrap();
        let second = LauncherSessions::acquire(&fixture.0).unwrap();
        let third = LauncherSessions::acquire(&fixture.0).unwrap();
        assert!(primary.for_window("main").unwrap().persistent);
        let second_scope = second.for_window("main").unwrap();
        assert!(!second_scope.persistent);
        assert!(!third.for_window("main").unwrap().persistent);
        assert_ne!(second_scope.id, third.for_window("main").unwrap().id);
        assert_eq!(second_scope.id, second.for_window("main").unwrap().id);

        drop(primary);
        let replacement = LauncherSessions::acquire(&fixture.0).unwrap();
        assert!(replacement.for_window("main").unwrap().persistent);
        // Existing temporary windows never change accounts when the owner exits.
        assert_eq!(second_scope.id, second.for_window("main").unwrap().id);
        assert!(!second.for_window("main").unwrap().persistent);
    }

    #[test]
    fn extra_windows_cannot_share_the_primary_scope() {
        let fixture = Fixture::new();
        let sessions = LauncherSessions::acquire(&fixture.0).unwrap();
        let first = sessions.for_window("secondary-a").unwrap();
        let second = sessions.for_window("secondary-b").unwrap();
        assert!(!first.persistent);
        assert!(!second.persistent);
        assert_ne!(first.id, second.id);
        assert_eq!(first.id, sessions.for_window("secondary-a").unwrap().id);
        assert!(sessions.for_window("main").unwrap().persistent);
    }

    #[test]
    fn inaccessible_lock_never_falls_back_to_shared_auth() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.0.join("launcher-sessions/primary.lock")).unwrap();
        assert!(LauncherSessions::acquire(&fixture.0).is_err());
    }

    // This helper executes only when the parent supplies a fresh test directory.
    #[test]
    fn child_process_holds_session_lock() {
        let Some(directory) = std::env::var_os("AURORA_TEST_SESSION_LOCK") else {
            return;
        };
        let directory = PathBuf::from(directory);
        let sessions = LauncherSessions::acquire(&directory).unwrap();
        assert!(sessions.for_window("main").unwrap().persistent);
        fs::write(directory.join("ready"), []).unwrap();
        std::thread::sleep(Duration::from_secs(30));
        drop(sessions);
    }

    #[test]
    fn process_crash_releases_the_primary_lock() {
        let fixture = Fixture::new();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "launcher_sessions::tests::child_process_holds_session_lock",
            ])
            .env("AURORA_TEST_SESSION_LOCK", &fixture.0)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !fixture.0.join("ready").exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let ready = fixture.0.join("ready").exists();
        let secondary = ready.then(|| LauncherSessions::acquire(&fixture.0).unwrap());
        let _ = child.kill();
        child.wait().unwrap();
        assert!(ready, "child did not acquire the lock before the deadline");
        assert!(!secondary.unwrap().for_window("main").unwrap().persistent);
        assert!(
            LauncherSessions::acquire(&fixture.0)
                .unwrap()
                .for_window("main")
                .unwrap()
                .persistent
        );
    }
}
