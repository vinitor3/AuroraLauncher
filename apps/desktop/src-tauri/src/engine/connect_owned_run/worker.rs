use super::*;
use std::{
    io::{Read, Write},
    os::windows::process::CommandExt,
    panic::{catch_unwind, AssertUnwindSafe},
    process::{Child, ChildStdin, ExitStatus, Stdio},
    thread::{self, JoinHandle},
};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

const POLL: Duration = Duration::from_millis(20);

pub(super) fn start(
    mut command: Command,
    instance_id: &str,
    limits: OwnedRunLimits,
    cancel: CancellationToken,
    hooks: Hooks,
) -> Result<OwnedRun, OwnedRunError> {
    if instance_id.is_empty() || instance_id.len() > 128 || !limits.valid() {
        return Err(OwnedRunError::InvalidRequest);
    }
    if cancel.is_cancelled() {
        return Err(OwnedRunError::Cancelled);
    }
    let deadline = Instant::now() + limits.lifetime;
    let run_id = uuid::Uuid::new_v4().to_string();
    let (sender, receiver) = watch::channel(None);
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            reason: None,
            failure: None,
            proof: None,
            query_active: false,
            stdout_bytes: 0,
            stderr_bytes: 0,
            completed: false,
        }),
        changed: Condvar::new(),
        deadline,
        limits,
        cancel,
        #[cfg(test)]
        hooks: hooks.clone(),
    });
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);
    if shared.cancel.is_cancelled() {
        return Err(OwnedRunError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(OwnedRunError::Deadline);
    }
    #[cfg(test)]
    hooks.before_spawn();
    #[cfg(not(test))]
    let _ = hooks;
    // The RAII owner is installed immediately: no fallible work or pipe
    // extraction may be inserted between spawn and this struct construction.
    let child = command.spawn().map_err(|_| OwnedRunError::Spawn)?;
    let mut owner = Worker {
        child: Some(child),
        stdin: None,
        readers: Vec::new(),
        shared: shared.clone(),
        sender,
        exit: None,
        forced: false,
        reaped: false,
        finished: false,
    };
    let initialized = catch_unwind(AssertUnwindSafe(|| {
        #[cfg(test)]
        shared.hooks.init(test_support::InitStage::Spawned)?;
        let child = owner
            .child
            .as_mut()
            .ok_or(OwnedRunFailure::Initialization)?;
        let proof = OwnedChildProof::from_child(child, instance_id, &run_id)
            .map_err(|_| OwnedRunFailure::Proof)?;
        shared.lock().proof = Some(Arc::new(proof));
        #[cfg(test)]
        {
            shared.hooks.metadata(instance_id, &run_id);
            shared.hooks.init(test_support::InitStage::Proof)?;
        }
        owner.stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or(OwnedRunFailure::Initialization)?;
        let stderr = child.stderr.take().ok_or(OwnedRunFailure::Initialization)?;
        owner
            .readers
            .push(spawn_reader(stdout, shared.clone(), true)?);
        #[cfg(test)]
        shared.hooks.init(test_support::InitStage::FirstReader)?;
        owner
            .readers
            .push(spawn_reader(stderr, shared.clone(), false)?);
        #[cfg(test)]
        shared.hooks.init(test_support::InitStage::Readers)?;
        if owner.stdin.is_none() {
            return Err(OwnedRunFailure::Initialization);
        }
        #[cfg(test)]
        shared.hooks.after_init();
        Ok::<_, OwnedRunFailure>(())
    }));
    match initialized {
        Ok(Ok(())) => {}
        Ok(Err(failure)) => {
            shared.fail(failure);
            drop(owner); // Cleanup completes before the start error escapes.
            return Err(OwnedRunError::Initialization);
        }
        Err(_) => {
            shared.fail(OwnedRunFailure::Panic);
            drop(owner);
            return Err(OwnedRunError::Internal);
        }
    }
    // Spawning the worker can fail. The closure owns Worker, so dropping that
    // closure on failure invokes the same synchronous RAII cleanup.
    #[cfg(test)]
    if shared.hooks.fail_worker_spawn {
        shared.fail(OwnedRunFailure::Thread);
        drop(owner);
        return Err(OwnedRunError::Initialization);
    }
    if thread::Builder::new()
        .name("connect-owned-run".into())
        .spawn(move || owner.run())
        .is_err()
    {
        shared.fail(OwnedRunFailure::Thread);
        return Err(OwnedRunError::Initialization);
    }
    let start_error = {
        let mut state = shared.lock();
        shared.refresh(&mut state);
        match state.reason {
            None => None,
            Some(OwnedRunReason::Cancelled) => Some(OwnedRunError::Cancelled),
            Some(OwnedRunReason::Deadline) => Some(OwnedRunError::Deadline),
            Some(_) => Some(OwnedRunError::Initialization),
        }
    };
    if let Some(error) = start_error {
        // Do not hold the gate while the owner needs it to finish cleanup.
        let mut state = shared.lock();
        while !state.completed {
            state = shared.changed.wait(state).unwrap_or_else(|poison| {
                let mut state = poison.into_inner();
                state.seal(OwnedRunReason::Failure, Some(OwnedRunFailure::Internal));
                state
            });
        }
        return Err(error);
    }
    Ok(OwnedRun {
        shared,
        completion: OwnedRunCompletion { receiver },
    })
}

fn spawn_reader(
    mut pipe: impl Read + Send + 'static,
    shared: Arc<Shared>,
    stdout: bool,
) -> Result<JoinHandle<()>, OwnedRunFailure> {
    thread::Builder::new()
        .name(
            if stdout {
                "connect-owned-out"
            } else {
                "connect-owned-err"
            }
            .into(),
        )
        .spawn(move || {
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                #[cfg(test)]
                if shared.hooks.read_error_stdout == Some(stdout) {
                    // Fail the run, while retaining and draining the real pipe.
                    shared.hooks.before_read_error();
                    shared.fail(OwnedRunFailure::Read);
                }
                let mut buffer = [0u8; 4096];
                loop {
                    match pipe.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => {
                            let mut state = shared.lock();
                            let counter = if stdout {
                                &mut state.stdout_bytes
                            } else {
                                &mut state.stderr_bytes
                            };
                            *counter = counter.saturating_add(count as u64);
                            if state.stdout_bytes.saturating_add(state.stderr_bytes)
                                > shared.limits.output_bytes
                            {
                                state.seal(
                                    OwnedRunReason::Failure,
                                    Some(OwnedRunFailure::OutputLimit),
                                );
                                shared.changed.notify_all();
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(_) => {
                            shared.fail(OwnedRunFailure::Read);
                            break;
                        }
                    }
                }
            }));
            if outcome.is_err() {
                shared.fail(OwnedRunFailure::Panic);
            }
            // Test-only: retain the actual drained pipe/reader before its
            // thread returns. No synthetic completion or process is supplied.
            #[cfg(test)]
            shared.hooks.reader_final();
        })
        .map_err(|_| OwnedRunFailure::Thread)
}

/// All process/pipes/reader ownership stays here, independently of control and
/// async waiters. Drop is also the partial-initialization/panic cleanup path.
struct Worker {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    readers: Vec<JoinHandle<()>>,
    shared: Arc<Shared>,
    sender: watch::Sender<Option<OwnedRunReport>>,
    exit: Option<ExitStatus>,
    forced: bool,
    reaped: bool,
    finished: bool,
}
impl Worker {
    fn run(mut self) {
        // Finalization sits outside the unwind boundary and still owns every
        // resource if the operational loop panics.
        if catch_unwind(AssertUnwindSafe(|| self.drive())).is_err() {
            self.shared.fail(OwnedRunFailure::Panic);
        }
        self.finish();
    }
    fn drive(&mut self) {
        #[cfg(test)]
        self.shared.hooks.worker_started();
        loop {
            {
                let mut state = self.shared.lock();
                self.shared.refresh(&mut state);
            }
            if self.poll_exit() {
                #[cfg(test)]
                self.shared.hooks.after_exit();
                // A concurrent stop request cannot retroactively make an exit
                // observed before our fixed stop write into a normal stop.
                self.shared
                    .lock()
                    .seal(OwnedRunReason::Exited, Some(OwnedRunFailure::EarlyExit));
                break;
            }
            if self.shared.lock().reason.is_some() {
                #[cfg(test)]
                self.shared.hooks.before_stop();
                self.stop_child();
                break;
            }
            self.shared.pause(POLL);
        }
    }
    fn poll_exit(&mut self) -> bool {
        if self.reaped {
            return true;
        }
        let Some(child) = self.child.as_mut() else {
            return false;
        };
        match child.try_wait() {
            Ok(Some(exit)) => {
                self.exit = Some(exit);
                self.reaped = true;
                #[cfg(test)]
                self.shared.hooks.reaped();
                true
            }
            Ok(None) => false,
            Err(_) => {
                self.shared.fail(OwnedRunFailure::Wait);
                false
            }
        }
    }
    fn stop_child(&mut self) {
        if self.poll_exit() {
            self.shared.fail(OwnedRunFailure::EarlyExit);
            return;
        }
        if Instant::now() < self.shared.deadline {
            if let Some(mut stdin) = self.stdin.take() {
                #[cfg(test)]
                let fail_write = self.shared.hooks.fail_stop_write;
                #[cfg(not(test))]
                let fail_write = false;
                if fail_write || stdin.write_all(b"stop\n").is_err() {
                    self.shared.fail(OwnedRunFailure::Write);
                }
                // Only this fixed write exists; stdin closes even after error.
            }
        } else {
            drop(self.stdin.take());
        }
        let grace = (Instant::now() + self.shared.limits.stop_grace).min(self.shared.deadline);
        while !self.poll_exit() {
            {
                let mut state = self.shared.lock();
                self.shared.refresh(&mut state);
            }
            if Instant::now() >= grace {
                break;
            }
            self.shared
                .pause(POLL.min(grace.saturating_duration_since(Instant::now())));
        }
        if !self.reaped {
            self.force_reap();
        }
    }
    fn force_reap(&mut self) {
        drop(self.stdin.take());
        if self.poll_exit() {
            return;
        }
        self.forced = true;
        self.shared.fail(OwnedRunFailure::Forced);
        // Failure cannot release Child or advertise completion. Retry its own
        // handle; no process-name/PID lookup, adoption or foreign cleanup.
        while !self.reaped {
            if let Some(child) = self.child.as_mut() {
                if child.kill().is_err() {
                    self.shared.fail(OwnedRunFailure::Kill);
                }
                match child.wait() {
                    Ok(exit) => {
                        self.exit = Some(exit);
                        self.reaped = true;
                        #[cfg(test)]
                        self.shared.hooks.reaped();
                    }
                    Err(_) => self.shared.fail(OwnedRunFailure::Wait),
                }
            }
            if !self.reaped {
                thread::sleep(POLL);
            }
        }
    }
    fn finish(&mut self) {
        if self.finished {
            return;
        }
        if !self.reaped {
            self.force_reap();
        }
        drop(self.stdin.take());
        drop(self.child.take());
        while let Some(reader) = self.readers.pop() {
            // Descendants retaining pipes are unsupported: this remains
            // pending instead of claiming EOF/cleanup or killing strangers.
            if reader.join().is_err() {
                self.shared.fail(OwnedRunFailure::Panic);
            }
        }
        #[cfg(test)]
        self.shared.hooks.readers_joined();
        if !self.exit.is_some_and(|status| status.success()) {
            self.shared.fail(OwnedRunFailure::NonzeroExit);
        }
        let mut state = self.shared.lock();
        if state.reason.is_none() {
            state.seal(OwnedRunReason::Exited, Some(OwnedRunFailure::EarlyExit));
        }
        while state.query_active {
            state = self.shared.changed.wait(state).unwrap_or_else(|poison| {
                let mut state = poison.into_inner();
                state.seal(OwnedRunReason::Failure, Some(OwnedRunFailure::Internal));
                state
            });
        }
        let proof = state.proof.take();
        drop(state);
        drop(proof);
        // No Child, pipes, readers or proof remain before final publication.
        #[cfg(test)]
        self.shared.hooks.before_report();
        let mut state = self.shared.lock();
        self.shared.refresh(&mut state);
        let report = OwnedRunReport {
            reason: state.reason.unwrap_or(OwnedRunReason::Failure),
            failure: state.failure,
            exit_code: self.exit.and_then(|status| status.code()),
            forced: self.forced,
            stdout_bytes: state.stdout_bytes,
            stderr_bytes: state.stderr_bytes,
            reaped: self.reaped,
            readers_complete: true,
        };
        self.sender.send_replace(Some(report));
        state.completed = true;
        self.finished = true;
        self.shared.changed.notify_all();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if !self.finished {
            // Builder::spawn drops its closure on failure; latch that failure
            // before RAII can publish cleanup. Earlier failures stay intact.
            self.shared.fail(OwnedRunFailure::Thread);
        }
        self.finish();
    }
}
