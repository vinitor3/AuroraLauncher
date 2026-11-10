//! Scoped presentation events. They never grant installation authority.
use super::{AppHandle, DownloadProgressEvent, TransferProgress};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::Emitter;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ScopedProgress {
    operation_id: uuid::Uuid,
    #[serde(flatten)]
    progress: DownloadProgressEvent,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Outcome {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Finished {
    schema_version: u8,
    operation_id: uuid::Uuid,
    outcome: Outcome,
}

pub(super) enum Event {
    Progress(ScopedProgress),
    Finished(Finished),
}

pub(super) fn emitter(app: AppHandle) -> impl Fn(Event) + Send + Sync {
    move |event| match event {
        Event::Progress(progress) => {
            let _ = app.emit("aurora-download-progress", progress);
        }
        Event::Finished(finished) => {
            let _ = app.emit("aurora-ready-copy-install-finished", finished);
        }
    }
}

pub(super) struct CopyInstallProgress<F: Fn(Event)> {
    operation_id: uuid::Uuid,
    cancel: Arc<AtomicBool>,
    emit: F,
    complete: bool,
}

impl<F: Fn(Event)> CopyInstallProgress<F> {
    pub(super) fn new(operation_id: uuid::Uuid, cancel: Arc<AtomicBool>, emit: F) -> Self {
        Self {
            operation_id,
            cancel,
            emit,
            complete: false,
        }
    }

    pub(super) fn transfer(&self, mut progress: TransferProgress) {
        if self.complete {
            return;
        }
        // Only publication below may report 100%; installer stages are partial.
        progress.total_percent = progress.total_percent.clamp(0.0, 98.0);
        (self.emit)(Event::Progress(ScopedProgress {
            operation_id: self.operation_id,
            progress: progress.into(),
        }));
    }

    pub(super) fn complete(&mut self) {
        if self.complete {
            return;
        }
        self.complete = true;
        (self.emit)(Event::Progress(ScopedProgress {
            operation_id: self.operation_id,
            progress: TransferProgress {
                label: "Cópia de runtime preparada".into(),
                total_percent: 100.0,
                ..TransferProgress::default()
            }
            .into(),
        }));
        self.finish(Outcome::Completed);
    }

    fn finish(&self, outcome: Outcome) {
        (self.emit)(Event::Finished(Finished {
            schema_version: 1,
            operation_id: self.operation_id,
            outcome,
        }));
    }
}

impl<F: Fn(Event)> Drop for CopyInstallProgress<F> {
    fn drop(&mut self) {
        if !self.complete {
            self.finish(if self.cancel.load(Ordering::Acquire) {
                Outcome::Cancelled
            } else {
                Outcome::Failed
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn ready_copy_progress_failure_cancel_and_unwind_are_terminal_without_success() {
        for mode in ["error", "cancel", "panic", "not-started"] {
            let events = Arc::new(Mutex::new(Vec::new()));
            let recorded = events.clone();
            let cancel = Arc::new(AtomicBool::new(mode == "cancel"));
            let id = uuid::Uuid::new_v4();
            let job = CopyInstallProgress::new(id, cancel, move |event| {
                recorded.lock().unwrap().push(match event {
                    Event::Progress(value) => serde_json::to_value(value).unwrap(),
                    Event::Finished(value) => serde_json::to_value(value).unwrap(),
                });
            });
            let result = std::panic::catch_unwind(move || {
                let job = job;
                if mode != "not-started" {
                    job.transfer(TransferProgress {
                        label: "Fabric preparado".into(),
                        total_percent: 100.0,
                        ..TransferProgress::default()
                    });
                }
                assert_ne!(mode, "panic", "synthetic unwind, no private error emitted");
            });
            assert_eq!(result.is_err(), mode == "panic");
            let events = events.lock().unwrap();
            assert_eq!(
                events.last().unwrap(),
                &serde_json::json!({"schemaVersion":1,"operationId":id,"outcome":if mode == "cancel" {"cancelled"} else {"failed"}})
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|event| event.get("outcome").is_some())
                    .count(),
                1
            );
            assert!(events.iter().all(|event| event["totalPercent"]
                .as_f64()
                .is_none_or(|percent| percent < 100.0)));
        }
    }

    #[test]
    fn ready_copy_progress_published_completion_is_once_and_keeps_its_scope() {
        let events = Mutex::new(Vec::new());
        let id = uuid::Uuid::new_v4();
        let mut job = CopyInstallProgress::new(id, Arc::new(AtomicBool::new(false)), |event| {
            events.lock().unwrap().push(match event {
                Event::Progress(value) => serde_json::to_value(value).unwrap(),
                Event::Finished(value) => serde_json::to_value(value).unwrap(),
            });
        });
        job.complete();
        job.complete();
        job.transfer(TransferProgress::default());
        drop(job);
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["operationId"], id.to_string());
        assert_eq!(events[0]["totalPercent"], 100.0);
        assert_eq!(
            events[1],
            serde_json::json!({"schemaVersion":1,"operationId":id,"outcome":"completed"})
        );
    }
}
