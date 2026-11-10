//! One bounded, redacted Connect journal. It is not a Minecraft DoctorRun.
use super::*;
use aurora_launcher_core::engine::connect::ConnectTerminalDiagnostic;
use std::sync::Arc;

#[derive(Default)]
struct Queue {
    latest: u64,
    pending: Option<ConnectTerminalDiagnostic>,
    running: bool,
}
pub struct ConnectTerminalRecorder {
    directory: PathBuf,
    queue: Mutex<Queue>,
}
impl ConnectTerminalRecorder {
    pub fn new(directory: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            directory,
            queue: Mutex::new(Queue::default()),
        })
    }
    pub fn submit(self: &Arc<Self>, diagnostic: ConnectTerminalDiagnostic) {
        let start = {
            let Ok(mut queue) = self.queue.lock() else {
                return;
            };
            if diagnostic.sequence() <= queue.latest {
                return;
            }
            queue.latest = diagnostic.sequence();
            queue.pending = Some(diagnostic);
            if queue.running {
                false
            } else {
                queue.running = true;
                true
            }
        };
        if start {
            let recorder = self.clone();
            // One blocking job, at most one pending latest value.
            // This best-effort journal owns no Connect authority or resources.
            tauri::async_runtime::spawn_blocking(move || recorder.drain());
        }
    }
    fn drain(&self) {
        loop {
            let diagnostic = {
                let Ok(mut queue) = self.queue.lock() else {
                    return;
                };
                match queue.pending.take() {
                    Some(value) => value,
                    None => {
                        queue.running = false;
                        return;
                    }
                }
            };
            // Neither the Connect mutex nor the queue mutex covers IO.
            let _ = diagnostic.persist_latest(&self.directory);
        }
    }
}
#[cfg(test)]
#[path = "doctor_connect_terminal_tests.rs"]
mod tests;
