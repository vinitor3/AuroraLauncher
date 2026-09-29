//! Conservative process gate for the opt-in diagnostic-copy voice adapter.
//! A fresh observation is required at each mutation; this is not a monitor or
//! proof against a renamed JVM or an external process started after the probe.
use super::{Instance, RecoveryError, Result};
use crate::engine::{acquire_instance_recovery_lock, ContentUpdateError, InstanceMutationGuard};

type Probe = Box<dyn Fn() -> Result<()> + Send + Sync>;

/// Owns the ordinary shared mutation lock and the private process check.
/// No public constructor, Deref or exposed inner guard can bypass that check.
pub struct VoiceMutationGuard {
    inner: InstanceMutationGuard,
    probe: Probe,
}

impl VoiceMutationGuard {
    pub(super) fn inner(&self) -> &InstanceMutationGuard {
        &self.inner
    }

    pub(super) fn recheck(&self) -> Result<()> {
        (self.probe)()
    }
}

pub fn acquire_voice_mutation_guard(instance: &Instance) -> Result<VoiceMutationGuard> {
    acquire(instance, Box::new(probe))
}

fn acquire(instance: &Instance, probe: Probe) -> Result<VoiceMutationGuard> {
    probe()?;
    let inner = acquire_instance_recovery_lock(instance).map_err(|error| match error {
        ContentUpdateError::Busy => RecoveryError::WrongLock,
        _ => RecoveryError::Io,
    })?;
    // A failed second observation drops/releases the already acquired OS lock.
    probe()?;
    Ok(VoiceMutationGuard { inner, probe })
}

#[cfg(test)]
pub(super) fn acquire_with_probe(instance: &Instance, probe: Probe) -> Result<VoiceMutationGuard> {
    acquire(instance, probe)
}

#[cfg(not(windows))]
fn probe() -> Result<()> {
    Err(RecoveryError::VoiceProcessUnavailable)
}

#[cfg(windows)]
fn probe() -> Result<()> {
    windows::probe()
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        ffi::{c_void, OsString},
        io::Read,
        os::windows::{ffi::OsStringExt, io::AsRawHandle, process::CommandExt},
        path::PathBuf,
        process::{Child, ChildStdout, Command, Stdio},
        time::{Duration, Instant},
    };

    const MAX_TIME: Duration = Duration::from_secs(5);
    const MAX_OUTPUT: usize = 32;
    const POLL: Duration = Duration::from_millis(10);
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const ERROR_BROKEN_PIPE: i32 = 109;

    // Only names are requested, locally. CommandLine, paths, arguments and PIDs
    // never leave PowerShell. No profiles, Java execution or process termination.
    // https://learn.microsoft.com/powershell/module/cimcmdlets/get-ciminstance
    const SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
try {
  $java = Get-CimInstance -ClassName Win32_Process -Property Name -Filter "Name='java.exe' OR Name='javaw.exe'" -ErrorAction Stop | Select-Object -First 1
  if ($null -ne $java) { [Console]::Out.Write('RUNNING') } else { [Console]::Out.Write('STOPPED') }
  exit 0
} catch { exit 1 }
"#;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
        fn PeekNamedPipe(
            pipe: *mut c_void,
            buffer: *mut c_void,
            buffer_size: u32,
            bytes_read: *mut u32,
            available: *mut u32,
            remaining_message: *mut u32,
        ) -> i32;
    }

    // Use the operating system's directory, not PATH or a caller-controlled
    // SystemRoot string. No files are written beneath this path.
    // https://learn.microsoft.com/windows/win32/api/sysinfoapi/nf-sysinfoapi-getsystemdirectoryw
    fn powershell() -> Result<PathBuf> {
        let mut buffer = vec![0_u16; 260];
        for _ in 0..2 {
            // SAFETY: the buffer is writable for exactly the passed UTF-16 units.
            let length =
                unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
            if length == 0 || length >= 32_768 {
                return Err(RecoveryError::VoiceProcessUnavailable);
            }
            if length >= buffer.len() {
                buffer.resize(length + 1, 0);
                continue;
            }
            if buffer[..length].contains(&0) {
                return Err(RecoveryError::VoiceProcessUnavailable);
            }
            let directory = PathBuf::from(OsString::from_wide(&buffer[..length]));
            if !directory.is_absolute() {
                return Err(RecoveryError::VoiceProcessUnavailable);
            }
            return Ok(directory.join("WindowsPowerShell/v1.0/powershell.exe"));
        }
        Err(RecoveryError::VoiceProcessUnavailable)
    }

    pub(super) fn probe() -> Result<()> {
        run_script(SCRIPT, MAX_TIME)
    }

    fn spawn_script(script: &str) -> Result<Child> {
        Command::new(powershell()?)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| RecoveryError::VoiceProcessUnavailable)
    }

    fn run_script(script: &str, timeout: Duration) -> Result<()> {
        if timeout.is_zero() || timeout > MAX_TIME {
            return Err(RecoveryError::VoiceProcessUnavailable);
        }
        let deadline = Instant::now() + timeout;
        let mut child = spawn_script(script)?;
        observe_child(&mut child, deadline)
    }

    struct Reap<'a>(&'a mut Child);
    impl Drop for Reap<'_> {
        fn drop(&mut self) {
            // The only process terminated here is our own short-lived probe.
            // Also runs for timeout, pipe error, unknown output and unwinding.
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn observe_child(child: &mut Child, deadline: Instant) -> Result<()> {
        let child = Reap(child);
        let mut stdout = child
            .0
            .stdout
            .take()
            .ok_or(RecoveryError::VoiceProcessUnavailable)?;
        let mut output = Vec::with_capacity(MAX_OUTPUT);
        loop {
            if Instant::now() >= deadline {
                return Err(RecoveryError::VoiceProcessUnavailable);
            }
            drain_available(&mut stdout, &mut output)?;
            if let Some(status) = child
                .0
                .try_wait()
                .map_err(|_| RecoveryError::VoiceProcessUnavailable)?
            {
                // Capture bytes written between the previous peek and exit.
                drain_available(&mut stdout, &mut output)?;
                return if status.success() {
                    parse_status(&output)
                } else {
                    Err(RecoveryError::VoiceProcessUnavailable)
                };
            }
            std::thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
        }
    }

    // Never call a blocking read on an empty pipe or accumulate unbounded data.
    // This handle has one reader and no concurrent/pending I/O. PeekNamedPipe
    // supports anonymous pipes; read at most the amount already present.
    // https://learn.microsoft.com/windows/win32/api/namedpipeapi/nf-namedpipeapi-peeknamedpipe
    fn drain_available(stdout: &mut ChildStdout, output: &mut Vec<u8>) -> Result<()> {
        // A short read is legal. Drain all remaining bytes before accepting the
        // final status, including bytes still buffered when the child exits.
        loop {
            let mut available = 0_u32;
            // SAFETY: ChildStdout owns a live read handle, the only non-null output
            // points to a valid u32; the function neither owns nor closes the handle.
            let success = unsafe {
                PeekNamedPipe(
                    stdout.as_raw_handle(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            };
            if success == 0 {
                return if std::io::Error::last_os_error().raw_os_error() == Some(ERROR_BROKEN_PIPE)
                {
                    Ok(())
                } else {
                    Err(RecoveryError::VoiceProcessUnavailable)
                };
            }
            let available = available as usize;
            if available > MAX_OUTPUT - output.len() {
                return Err(RecoveryError::VoiceProcessUnavailable);
            }
            if available == 0 {
                return Ok(());
            }
            let mut buffer = [0_u8; MAX_OUTPUT];
            let count = stdout
                .read(&mut buffer[..available])
                .map_err(|_| RecoveryError::VoiceProcessUnavailable)?;
            if count == 0 {
                return Err(RecoveryError::VoiceProcessUnavailable);
            }
            output.extend_from_slice(&buffer[..count]);
        }
    }

    fn parse_status(output: &[u8]) -> Result<()> {
        match output {
            b"STOPPED" | b"STOPPED\n" | b"STOPPED\r\n" => Ok(()),
            b"RUNNING" | b"RUNNING\n" | b"RUNNING\r\n" => Err(RecoveryError::VoiceProcessRunning),
            _ => Err(RecoveryError::VoiceProcessUnavailable),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn fixture(script: &str, timeout: Duration) -> (Result<()>, std::process::ExitStatus) {
            let deadline = Instant::now() + timeout;
            let mut child = spawn_script(script).unwrap();
            let result = observe_child(&mut child, deadline);
            let status = child.try_wait().unwrap().expect("probe must be reaped");
            (result, status)
        }

        #[test]
        fn system_executable_is_absolute_and_status_parser_is_exact() {
            let path = powershell().unwrap();
            assert!(path.is_absolute());
            assert!(path.ends_with("WindowsPowerShell/v1.0/powershell.exe"));
            assert!(path.is_file());
            for status in [b"STOPPED".as_slice(), b"STOPPED\n", b"STOPPED\r\n"] {
                assert!(parse_status(status).is_ok());
            }
            for status in [b"RUNNING".as_slice(), b"RUNNING\n", b"RUNNING\r\n"] {
                assert!(matches!(
                    parse_status(status),
                    Err(RecoveryError::VoiceProcessRunning)
                ));
            }
            for status in [
                b"".as_slice(),
                b"STOPPED ",
                b" STOPPED",
                b"STOPPED\r",
                b"STOPPED\nSTOPPED",
                b"stopped",
                b"UNKNOWN",
                b"STOPPED\0",
                b"\xff",
            ] {
                assert!(matches!(
                    parse_status(status),
                    Err(RecoveryError::VoiceProcessUnavailable)
                ));
            }
        }

        #[test]
        fn actual_executor_accepts_only_successful_fixed_status_and_reaps_each_child() {
            for (script, expected) in [
                ("[Console]::Out.Write('STOPPED'); exit 0", 0),
                ("Write-Output 'STOPPED'; exit 0", 0),
                ("[Console]::Out.Write('RUNNING'); exit 0", 1),
                ("[Console]::Out.Write('UNKNOWN'); exit 0", 2),
                ("[Console]::Out.Write('STOPPED '); exit 0", 2),
                ("[Console]::Out.Write('STOPPED'); exit 2", 2),
                ("[Console]::Out.Write(('x' * 4096)); exit 0", 2),
                ("[Console]::OpenStandardOutput().WriteByte(255); exit 0", 2),
                ("exit 0", 2),
                ("[Console]::Error.Write('fixture-sensitive-marker'); [Console]::Out.Write('STOPPED'); exit 0", 0),
            ] {
                let (result, _) = fixture(script, MAX_TIME);
                match expected {
                    0 => assert!(result.is_ok()),
                    1 => assert!(matches!(result, Err(RecoveryError::VoiceProcessRunning))),
                    _ => assert!(matches!(result, Err(RecoveryError::VoiceProcessUnavailable))),
                }
            }
        }

        #[test]
        fn actual_executor_bounds_output_and_kills_timed_out_or_flooding_probe() {
            for script in [
                "Start-Sleep -Seconds 20",
                "[Console]::Out.Write('STOPPED'); Start-Sleep -Seconds 20",
                "while ($true) { [Console]::Out.Write(('x' * 4096)) }",
            ] {
                let start = Instant::now();
                let (result, status) = fixture(script, Duration::from_millis(800));
                assert!(matches!(
                    result,
                    Err(RecoveryError::VoiceProcessUnavailable)
                ));
                assert!(!status.success());
                assert!(start.elapsed() < Duration::from_secs(5));
            }
            assert!(run_script("exit 0", Duration::ZERO).is_err());
            assert!(run_script("exit 0", MAX_TIME + Duration::from_millis(1)).is_err());
        }

        #[test]
        fn constant_script_uses_names_only_and_fixtures_do_not_execute_cim_or_java() {
            assert!(!SCRIPT.contains("CommandLine"));
            let query = "Get-CimInstance -ClassName Win32_Process -Property Name -Filter \"Name='java.exe' OR Name='javaw.exe'\" -ErrorAction Stop | Select-Object -First 1";
            assert_eq!(SCRIPT.matches(query).count(), 1);
            for (replacement, running) in [
                ("$null", false),
                ("[pscustomobject]@{ Name = 'java.exe' }", true),
                ("[pscustomobject]@{ Name = 'javaw.exe' }", true),
            ] {
                let script = SCRIPT.replace(query, replacement);
                let (result, _) = fixture(&script, MAX_TIME);
                if running {
                    assert!(matches!(result, Err(RecoveryError::VoiceProcessRunning)));
                } else {
                    assert!(result.is_ok());
                }
            }
            let script = SCRIPT.replace(query, "$(throw 'fixture-sensitive-marker')");
            assert!(matches!(
                fixture(&script, MAX_TIME).0,
                Err(RecoveryError::VoiceProcessUnavailable)
            ));
        }

        #[test]
        #[ignore = "requires explicit AURORA_CONNECT_PROCESS_PROBE=1; queries local process names only"]
        fn production_java_process_probe_reports_only_status() {
            assert_eq!(
                std::env::var("AURORA_CONNECT_PROCESS_PROBE").as_deref(),
                Ok("1")
            );
            match probe() {
                Ok(()) => println!("voice_process_probe=STOPPED"),
                Err(RecoveryError::VoiceProcessRunning) => println!("voice_process_probe=RUNNING"),
                Err(_) => panic!("voice_process_probe=UNAVAILABLE"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{InstanceId, InstanceLayout};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct Fixture {
        root: std::path::PathBuf,
        instance: Instance,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("aurora-connect-process-{}", uuid::Uuid::new_v4()));
            let instance = InstanceLayout::new(&root).open(InstanceId::parse("fixture").unwrap());
            instance.ensure_layout().unwrap();
            Self { root, instance }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            assert!(self.root.is_absolute() && self.root.starts_with(std::env::temp_dir()));
            assert!(self
                .root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("aurora-connect-process-"));
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn guard_checks_before_and_after_lock_and_recheck_never_caches() {
        let fixture = Fixture::new();
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        let instance = fixture.instance.clone();
        let guard = acquire_with_probe(
            &fixture.instance,
            Box::new(move || {
                let call = seen.fetch_add(1, Ordering::SeqCst);
                if call > 0 {
                    assert!(matches!(
                        acquire_instance_recovery_lock(&instance),
                        Err(ContentUpdateError::Busy)
                    ));
                }
                if call == 3 {
                    return Err(RecoveryError::VoiceProcessRunning);
                }
                Ok(())
            }),
        )
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
        guard.recheck().unwrap();
        assert!(matches!(
            guard.recheck(),
            Err(RecoveryError::VoiceProcessRunning)
        ));
        assert_eq!(count.load(Ordering::SeqCst), 4);
        let _ = guard.inner();
        drop(guard);
        drop(acquire_instance_recovery_lock(&fixture.instance).unwrap());
    }

    #[test]
    fn first_or_second_probe_failure_never_retains_the_shared_lock() {
        for failure in [0, 1] {
            let fixture = Fixture::new();
            let count = Arc::new(AtomicUsize::new(0));
            let seen = count.clone();
            let result = acquire_with_probe(
                &fixture.instance,
                Box::new(move || {
                    if seen.fetch_add(1, Ordering::SeqCst) == failure {
                        Err(RecoveryError::VoiceProcessUnavailable)
                    } else {
                        Ok(())
                    }
                }),
            );
            assert!(matches!(
                result,
                Err(RecoveryError::VoiceProcessUnavailable)
            ));
            assert_eq!(count.load(Ordering::SeqCst), failure + 1);
            if failure == 0 {
                assert!(!fixture.root.join(".aurora/instance-locks").exists());
            }
            assert!(!fixture
                .instance
                .root()
                .join(".aurora/recovery-identity.json")
                .exists());
            assert!(!fixture
                .instance
                .root()
                .join(".aurora/connect-voice-stage")
                .exists());
            drop(acquire_instance_recovery_lock(&fixture.instance).unwrap());
        }
    }

    #[test]
    fn an_existing_shared_lock_is_not_bypassed_or_followed_by_second_probe() {
        let fixture = Fixture::new();
        let guard = acquire_instance_recovery_lock(&fixture.instance).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        assert!(matches!(
            acquire_with_probe(
                &fixture.instance,
                Box::new(move || {
                    seen.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            ),
            Err(RecoveryError::WrongLock)
        ));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        drop(guard);
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_platform_refuses_before_creating_a_lock() {
        let fixture = Fixture::new();
        assert!(matches!(
            acquire_voice_mutation_guard(&fixture.instance),
            Err(RecoveryError::VoiceProcessUnavailable)
        ));
        assert!(!fixture.root.join(".aurora/instance-locks").exists());
    }
}
