//! A native installer and its descendants share one exclusive Windows Job.
//! Callers supply reviewed, pinned inputs; this helper supplies process ownership.
use super::recovery;
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    mem::{size_of, zeroed},
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr::{null, null_mut},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    },
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    },
    System::{JobObjects::*, Threading::*},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnedInstallerError {
    Invalid,
    Cancelled,
    Deadline,
    Create,
    Job,
    Resume,
    Wait,
    Exit,
    Panic,
}
type Result<T> = std::result::Result<T, OwnedInstallerError>;

struct Handle(HANDLE);
impl Handle {
    fn checked(raw: HANDLE) -> Result<Self> {
        if raw.is_null() || raw == INVALID_HANDLE_VALUE {
            Err(OwnedInstallerError::Create)
        } else {
            Ok(Self(raw))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: this type owns exactly one valid, non-inherited handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

struct ProcessJob {
    job: Handle,
    process: Handle,
    assigned: bool,
    complete: bool,
}
#[cfg(test)]
#[derive(Default)]
struct CollectionFaults {
    query_until: Option<Instant>,
    terminate_until: Option<Instant>,
}
#[cfg(test)]
thread_local! {
    static COLLECTION_FAULTS: std::cell::RefCell<CollectionFaults> = std::cell::RefCell::new(CollectionFaults::default());
}
impl ProcessJob {
    fn active(&self) -> Result<u32> {
        #[cfg(test)]
        if COLLECTION_FAULTS.with(|faults| {
            faults
                .borrow()
                .query_until
                .is_some_and(|until| Instant::now() < until)
        }) {
            return Err(OwnedInstallerError::Wait);
        }
        // SAFETY: a zeroed plain Win32 output structure has the supplied size.
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
        let ok = unsafe {
            QueryInformationJobObject(
                self.job.0,
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                null_mut(),
            )
        };
        if ok == 0 {
            Err(OwnedInstallerError::Wait)
        } else {
            Ok(info.ActiveProcesses)
        }
    }
    fn finish(&mut self) {
        if self.complete {
            return;
        }
        // The bounded termination phase seals execution. If Windows cannot
        // prove quiescence, this same blocking worker becomes the retained
        // collection tail; the asynchronous caller keeps displaying stopping.
        // No successful completion or guard release is inferred from a timeout.
        let termination_deadline = Instant::now() + Duration::from_secs(2);
        loop {
            self.terminate();
            let root_done = unsafe { WaitForSingleObject(self.process.0, 50) } == WAIT_OBJECT_0;
            let children_done = !self.assigned || self.active() == Ok(0);
            if root_done && children_done {
                self.complete = true;
                break;
            }
            thread::sleep(if Instant::now() < termination_deadline {
                Duration::from_millis(25)
            } else {
                Duration::from_millis(250)
            });
        }
    }
    fn terminate(&self) {
        #[cfg(test)]
        if COLLECTION_FAULTS.with(|faults| {
            faults
                .borrow()
                .terminate_until
                .is_some_and(|until| Instant::now() < until)
        }) {
            return;
        }
        // Only the original retained Job/process handle, never a PID lookup.
        unsafe {
            if self.assigned {
                TerminateJobObject(self.job.0, 1);
            } else {
                TerminateProcess(self.process.0, 1);
            }
        }
    }
}
impl Drop for ProcessJob {
    fn drop(&mut self) {
        // Normal errors and caught callback panics finalize explicitly while
        // caller guards remain retained. This emergency kill-on-close backstop
        // never publishes a cleanup-completed observation.
        if !self.complete {
            unsafe {
                if self.assigned {
                    TerminateJobObject(self.job.0, 1);
                } else {
                    TerminateProcess(self.process.0, 1);
                }
            }
        }
    }
}

struct Attributes {
    storage: Vec<usize>,
    initialized: bool,
}
impl Attributes {
    fn new(handles: &[HANDLE]) -> Result<Self> {
        let mut bytes = 0;
        // SAFETY: the first call obtains the bounded allocation size only.
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes);
        }
        if bytes == 0 || bytes > 64 * 1024 {
            return Err(OwnedInstallerError::Create);
        }
        let mut value = Self {
            storage: vec![0; bytes.div_ceil(size_of::<usize>())],
            initialized: false,
        };
        let ok = unsafe { InitializeProcThreadAttributeList(value.ptr(), 1, 0, &mut bytes) };
        if ok == 0 {
            return Err(OwnedInstallerError::Create);
        }
        value.initialized = true;
        let ok = unsafe {
            UpdateProcThreadAttribute(
                value.ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                std::mem::size_of_val(handles),
                null_mut(),
                null(),
            )
        };
        if ok == 0 {
            return Err(OwnedInstallerError::Create);
        }
        Ok(value)
    }
    fn ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                DeleteProcThreadAttributeList(self.ptr());
            }
        }
    }
}

fn wide(value: &OsStr) -> Result<Vec<u16>> {
    let mut value: Vec<_> = value.encode_wide().collect();
    if value.contains(&0) || value.len() > 30_000 {
        return Err(OwnedInstallerError::Invalid);
    }
    value.push(0);
    Ok(value)
}
fn argument(value: &OsStr) -> Result<Vec<u16>> {
    let value = wide(value)?;
    let mut output = vec![b'"' as u16];
    let mut slashes = 0;
    for &unit in &value[..value.len() - 1] {
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            b'\\' as u16,
            if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        output.push(unit);
        slashes = 0;
    }
    output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    output.push(b'"' as u16);
    Ok(output)
}
fn windows_directory() -> Result<OsString> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetWindowsDirectoryW(buffer: *mut u16, size: u32) -> u32;
    }
    use std::os::windows::ffi::OsStringExt;
    let mut value = vec![0; 1025];
    let length = unsafe { GetWindowsDirectoryW(value.as_mut_ptr(), value.len() as u32) } as usize;
    if length == 0 || length >= value.len() {
        return Err(OwnedInstallerError::Invalid);
    }
    Ok(OsString::from_wide(&value[..length]))
}

pub(crate) fn run_owned_java_installer(
    executable: &Path,
    args: &[OsString],
    cwd: &Path,
    home: &Path,
    timeout: Duration,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<()> {
    run_owned_installer(executable, args, cwd, home, timeout, cancelled, true)
}
fn run_owned_installer(
    executable: &Path,
    args: &[OsString],
    cwd: &Path,
    home: &Path,
    timeout: Duration,
    cancelled: &(dyn Fn() -> bool + Sync),
    java_options: bool,
) -> Result<()> {
    if !executable.is_absolute()
        || !cwd.is_absolute()
        || !home.starts_with(cwd)
        || timeout.is_zero()
        || timeout > Duration::from_secs(15 * 60)
        || args.len() > 32
    {
        return Err(OwnedInstallerError::Invalid);
    }
    if cancelled() {
        return Err(OwnedInstallerError::Cancelled);
    }
    let _cwd = recovery::pin_directory(cwd).map_err(|_| OwnedInstallerError::Invalid)?;
    let mut current = cwd.to_path_buf();
    let mut home_pins = Vec::new();
    let relative = home
        .strip_prefix(cwd)
        .map_err(|_| OwnedInstallerError::Invalid)?
        .join("tmp");
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return Err(OwnedInstallerError::Invalid);
        };
        current.push(name);
        match fs::create_dir(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(OwnedInstallerError::Invalid),
        }
        home_pins
            .extend(recovery::pin_directory(&current).map_err(|_| OwnedInstallerError::Invalid)?);
    }
    let _executable =
        recovery::secure_open(executable).map_err(|_| OwnedInstallerError::Invalid)?;
    let program = wide(executable.as_os_str())?;
    let directory = wide(cwd.as_os_str())?;
    let mut home_arg = OsString::from("-Duser.home=");
    home_arg.push(home);
    let mut temp_arg = OsString::from("-Djava.io.tmpdir=");
    temp_arg.push(home.join("tmp"));
    let mut argv = vec![executable.as_os_str().to_owned()];
    if java_options {
        argv.extend([home_arg, temp_arg]);
    }
    argv.extend_from_slice(args);
    let mut command = Vec::new();
    for arg in argv {
        if !command.is_empty() {
            command.push(b' ' as u16);
        }
        command.extend(argument(&arg)?);
    }
    if command.len() >= 32_767 {
        return Err(OwnedInstallerError::Invalid);
    }
    command.push(0);
    let windows = windows_directory()?;
    let environment = BTreeMap::from([
        (OsString::from("SystemRoot"), windows.clone()),
        (OsString::from("windir"), windows),
        (OsString::from("TEMP"), home.join("tmp").into_os_string()),
        (OsString::from("TMP"), home.join("tmp").into_os_string()),
        (OsString::from("USERPROFILE"), home.as_os_str().to_owned()),
    ]);
    let mut block = Vec::new();
    for (key, value) in environment {
        let mut entry = key;
        entry.push("=");
        entry.push(value);
        block.extend(wide(&entry)?);
    }
    block.push(0);
    let job = Handle::checked(unsafe { CreateJobObjectW(null(), null()) })?;
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
    {
        return Err(OwnedInstallerError::Job);
    }
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let nul = wide(OsStr::new("NUL"))?;
    let sink = Handle::checked(unsafe {
        CreateFileW(
            nul.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &security,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    })?;
    let inherited = [sink.0];
    let mut attributes = Attributes::new(&inherited)?;
    let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = sink.0;
    startup.StartupInfo.hStdOutput = sink.0;
    startup.StartupInfo.hStdError = sink.0;
    startup.lpAttributeList = attributes.ptr();
    let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
    if cancelled() {
        return Err(OwnedInstallerError::Cancelled);
    }
    // SAFETY: all pointers reference initialized, live buffers with exact Win32
    // layouts. Only the explicit NUL handle is inherited; the child is suspended.
    let created = unsafe {
        CreateProcessW(
            program.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_SUSPENDED
                | CREATE_NO_WINDOW
                | CREATE_UNICODE_ENVIRONMENT
                | EXTENDED_STARTUPINFO_PRESENT,
            block.as_ptr().cast(),
            directory.as_ptr(),
            &startup.StartupInfo,
            &mut info,
        )
    };
    if created == 0 {
        return Err(OwnedInstallerError::Create);
    }
    // A successful CreateProcess returns both owned handles. No fallible step
    // occurs before the process owner is installed.
    let process = Handle(info.hProcess);
    let thread = Handle(info.hThread);
    let mut owned = ProcessJob {
        job,
        process,
        assigned: false,
        complete: false,
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if unsafe { AssignProcessToJobObject(owned.job.0, owned.process.0) } == 0 {
            return Err(OwnedInstallerError::Job);
        }
        owned.assigned = true;
        if cancelled() {
            return Err(OwnedInstallerError::Cancelled);
        }
        if unsafe { ResumeThread(thread.0) } != 1 {
            return Err(OwnedInstallerError::Resume);
        }
        drop(thread);
        let deadline = Instant::now() + timeout;
        loop {
            if cancelled() {
                return Err(OwnedInstallerError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(OwnedInstallerError::Deadline);
            }
            let wait = unsafe { WaitForSingleObject(owned.process.0, 25) };
            if !matches!(wait, WAIT_OBJECT_0 | WAIT_TIMEOUT) {
                return Err(OwnedInstallerError::Wait);
            }
            if wait == WAIT_OBJECT_0 {
                if owned.active()? != 0 {
                    // A signalled root returns immediately from Wait, even
                    // while descendants still own files in the same Job.
                    thread::sleep(Duration::from_millis(25));
                    continue;
                }
                let mut exit = 0;
                if unsafe { GetExitCodeProcess(owned.process.0, &mut exit) } == 0 {
                    return Err(OwnedInstallerError::Wait);
                }
                owned.complete = true;
                return if exit == 0 {
                    Ok(())
                } else {
                    Err(OwnedInstallerError::Exit)
                };
            }
        }
    }))
    .unwrap_or(Err(OwnedInstallerError::Panic));
    // Execution expiry seals work. A failed Win32 observation keeps this
    // worker collecting with all caller guards retained; no timeout announces
    // cleanup or releases the copy while descendants may still write.
    owned.finish();
    result
}

#[cfg(test)]
#[path = "connect_owned_process_tests.rs"]
mod tests;
