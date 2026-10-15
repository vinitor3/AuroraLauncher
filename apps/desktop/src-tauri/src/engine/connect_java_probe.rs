//! Windows-only, bounded inspection of the explicitly selected Java executable.
//! Output stays in memory and is never returned to the UI or written to a log.
use std::{
    ffi::c_void,
    io::Read,
    os::windows::io::AsRawHandle,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const MAX_OUTPUT: usize = 16 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);
const ENVIRONMENT_OPTIONS: [&str; 4] = [
    "JDK_JAVA_OPTIONS",
    "JAVA_TOOL_OPTIONS",
    "_JAVA_OPTIONS",
    "CLASSPATH",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JavaProbeError {
    Cancelled,
    Unavailable,
    TimedOut,
    OutputLimit,
    InvalidVersion,
    CleanupFailed,
}

#[link(name = "kernel32")]
extern "system" {
    fn PeekNamedPipe(
        pipe: *mut c_void,
        buffer: *mut c_void,
        size: u32,
        read: *mut u32,
        available: *mut u32,
        remaining: *mut u32,
    ) -> i32;
}

/// Does not install Java or invoke a shell. The caller separately pins the
/// executable and instance and must recheck them after user confirmation.
pub fn probe_java_major(
    executable: &Path,
    cancelled: impl Fn() -> bool,
) -> Result<u32, JavaProbeError> {
    if !executable.is_absolute() || !executable.is_file() {
        return Err(JavaProbeError::Unavailable);
    }
    let mut command = super::process::background_command(executable);
    command.arg("-version");
    run_probe(command, &cancelled, TIMEOUT)
}

fn configure(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for variable in ENVIRONMENT_OPTIONS {
        command.env_remove(variable);
    }
}

struct OwnedProbe(Child);
impl Drop for OwnedProbe {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn run_probe(
    mut command: Command,
    cancelled: &impl Fn() -> bool,
    timeout: Duration,
) -> Result<u32, JavaProbeError> {
    if cancelled() {
        return Err(JavaProbeError::Cancelled);
    }
    configure(&mut command);
    let mut child = OwnedProbe(command.spawn().map_err(|_| JavaProbeError::Unavailable)?);
    observe_and_reap(&mut child.0, cancelled, timeout)
}

fn observe_and_reap(
    child: &mut Child,
    cancelled: &impl Fn() -> bool,
    timeout: Duration,
) -> Result<u32, JavaProbeError> {
    let result = (|| {
        let mut stdout = child.stdout.take().ok_or(JavaProbeError::Unavailable)?;
        let mut stderr = child.stderr.take().ok_or(JavaProbeError::Unavailable)?;
        let mut out = Vec::new();
        let mut err = Vec::new();
        let started = Instant::now();
        loop {
            if cancelled() {
                return Err(JavaProbeError::Cancelled);
            }
            if started.elapsed() >= timeout {
                return Err(JavaProbeError::TimedOut);
            }
            drain_available(&mut stdout, &mut out, err.len())?;
            drain_available(&mut stderr, &mut err, out.len())?;
            if let Some(status) = child.try_wait().map_err(|_| JavaProbeError::Unavailable)? {
                // The only writer owned by this probe has exited. Drain its
                // remaining bytes without waiting for EOF from descendants.
                drain_available(&mut stdout, &mut out, err.len())?;
                drain_available(&mut stderr, &mut err, out.len())?;
                if !status.success() {
                    return Err(JavaProbeError::Unavailable);
                }
                return parse_major(&out, &err).ok_or(JavaProbeError::InvalidVersion);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    // Only the process spawned above is terminated. Never enumerate/kill Java.
    // The deadline bounds observation; OS termination/reaping is also awaited.
    if child
        .try_wait()
        .map_err(|_| JavaProbeError::CleanupFailed)?
        .is_none()
    {
        child.kill().map_err(|_| JavaProbeError::CleanupFailed)?;
        child.wait().map_err(|_| JavaProbeError::CleanupFailed)?;
    }
    result
}

fn drain_available(
    pipe: &mut (impl Read + AsRawHandle),
    output: &mut Vec<u8>,
    other_bytes: usize,
) -> Result<(), JavaProbeError> {
    let mut available = 0_u32;
    // SAFETY: this exclusive pipe read handle lives through this call. Only
    // `available` is written; all other optional output pointers are null.
    let ok = unsafe {
        PeekNamedPipe(
            pipe.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return if std::io::Error::last_os_error().raw_os_error() == Some(109) {
            Ok(())
        } else {
            Err(JavaProbeError::Unavailable)
        };
    }
    let count = available as usize;
    if count > MAX_OUTPUT.saturating_sub(output.len() + other_bytes) {
        return Err(JavaProbeError::OutputLimit);
    }
    // There is exactly one reader per handle. No other thread can consume the
    // peeked bytes; read only that snapshot instead of blocking on EOF.
    if count != 0 {
        let offset = output.len();
        output.resize(offset + count, 0);
        pipe.read_exact(&mut output[offset..])
            .map_err(|_| JavaProbeError::Unavailable)?;
    }
    Ok(())
}

fn parse_major(stdout: &[u8], stderr: &[u8]) -> Option<u32> {
    let mut found = None;
    for bytes in [stdout, stderr] {
        for line in std::str::from_utf8(bytes).ok()?.lines() {
            let line = line.trim();
            let Some(version) = line
                .strip_prefix("openjdk version \"")
                .or_else(|| line.strip_prefix("java version \""))
            else {
                continue;
            };
            let version = version.split_once('"')?.0;
            let mut numbers = version.split(['.', '_', '-', '+']);
            let first = numbers.next()?.parse::<u32>().ok()?;
            let major = if first == 1 {
                numbers.next()?.parse::<u32>().ok()?
            } else {
                first
            };
            if major == 0 || major > 255 || found.is_some_and(|old| old != major) {
                return None;
            }
            found = Some(major);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, io::Write};
    const CHILD: &str = "AURORA_READY_JAVA_PROBE_CHILD";
    const TEST: &str = "engine::connect_java_probe::tests::java_probe_child_fixture";

    #[test]
    fn java_probe_child_fixture() {
        let Ok(mode) = std::env::var(CHILD) else {
            return;
        };
        for variable in ENVIRONMENT_OPTIONS {
            assert!(std::env::var_os(variable).is_none());
        }
        match mode.as_str() {
            "version" => eprintln!("openjdk version \"21.0.6\" 2025-01-21"),
            "stdout" => println!("java version \"1.8.0_452\""),
            "hang" => std::thread::sleep(Duration::from_secs(30)),
            "noise" => {
                let _ = std::io::stdout().write_all(&vec![b'x'; MAX_OUTPUT * 2]);
                std::thread::sleep(Duration::from_secs(30));
            }
            "split-noise" => {
                let _ = std::io::stdout().write_all(&vec![b'x'; 9_000]);
                let _ = std::io::stderr().write_all(&vec![b'y'; 9_000]);
                std::thread::sleep(Duration::from_secs(30));
            }
            "bad" => eprintln!("not a version"),
            "exit" => std::process::exit(2),
            _ => panic!("unknown private fixture"),
        }
    }
    fn command(mode: &str) -> Command {
        let mut command =
            super::super::process::background_command(std::env::current_exe().unwrap());
        command
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, mode);
        for variable in ENVIRONMENT_OPTIONS {
            command.env(variable, "must-be-removed");
        }
        command
    }
    fn observe_fixture(
        mode: &str,
        timeout: Duration,
        cancelled: impl Fn() -> bool,
    ) -> Result<u32, JavaProbeError> {
        let mut command = command(mode);
        configure(&mut command);
        let mut child = OwnedProbe(command.spawn().unwrap());
        let result = observe_and_reap(&mut child.0, &cancelled, timeout);
        assert!(
            child.0.try_wait().unwrap().is_some(),
            "owned probe must be reaped"
        );
        result
    }
    #[test]
    fn java_probe_observes_both_streams_and_removes_inherited_options() {
        assert_eq!(run_probe(command("version"), &|| false, TIMEOUT), Ok(21));
        assert_eq!(run_probe(command("stdout"), &|| false, TIMEOUT), Ok(8));
    }
    #[test]
    fn java_probe_timeout_reaps_only_owned_fixture() {
        assert_eq!(
            observe_fixture("hang", Duration::from_millis(100), || false),
            Err(JavaProbeError::TimedOut)
        );
    }
    #[test]
    fn java_probe_cancellation_reaps_running_fixture() {
        let checks = Cell::new(0);
        assert_eq!(
            observe_fixture("hang", TIMEOUT, || {
                checks.set(checks.get() + 1);
                checks.get() > 3
            }),
            Err(JavaProbeError::Cancelled)
        );
    }
    #[test]
    fn java_probe_combined_output_is_bounded_and_child_reaped() {
        for mode in ["noise", "split-noise"] {
            assert_eq!(
                observe_fixture(mode, TIMEOUT, || false),
                Err(JavaProbeError::OutputLimit)
            );
        }
    }
    #[test]
    fn java_probe_failure_never_returns_raw_output() {
        assert_eq!(
            observe_fixture("bad", TIMEOUT, || false),
            Err(JavaProbeError::InvalidVersion)
        );
        assert_eq!(
            observe_fixture("exit", TIMEOUT, || false),
            Err(JavaProbeError::Unavailable)
        );
        assert_eq!(
            run_probe(command("version"), &|| true, TIMEOUT),
            Err(JavaProbeError::Cancelled)
        );
    }
    #[test]
    fn java_probe_version_parser_refuses_conflicts_and_malformed_bytes() {
        assert_eq!(
            parse_major(b"openjdk version \"21\"", b"java version \"17\""),
            None
        );
        assert_eq!(parse_major(b"openjdk version \"21\"", &[255]), None);
        assert_eq!(parse_major(b"arbitrary 21 text", b""), None);
    }
}
