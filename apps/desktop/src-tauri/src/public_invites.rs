use std::collections::VecDeque;
use std::ffi::OsString;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

const TTL: Duration = Duration::from_secs(10 * 60);
const SCHEME: &str = "aurora-connect://join/";

fn canonical_id(raw: &str) -> Option<Uuid> {
    if raw.len() != 36 {
        return None;
    }
    let id = Uuid::parse_str(raw).ok()?;
    (id.get_version_num() == 4
        && id.get_variant() == uuid::Variant::RFC4122
        && id.to_string() == raw)
        .then_some(id)
}

pub fn from_arguments(mut args: impl Iterator<Item = OsString>) -> Option<Uuid> {
    let raw = args.next()?;
    if args.next().is_some() {
        return None;
    }
    canonical_id(raw.to_str()?.strip_prefix(SCHEME)?)
}

#[derive(Default)]
struct Inbox {
    pending: Option<(Uuid, Instant)>,
    recent: VecDeque<(Uuid, Instant)>,
}

impl Inbox {
    fn receive(&mut self, id: Uuid, now: Instant) -> bool {
        self.recent.retain(|(_, at)| now.duration_since(*at) < TTL);
        if self.recent.iter().any(|(seen, _)| *seen == id) {
            return false;
        }
        if self.recent.len() >= 32 {
            self.recent.pop_front();
        }
        self.recent.push_back((id, now));
        self.pending = Some((id, now));
        true
    }
    fn take(&mut self, now: Instant) -> Option<Uuid> {
        self.pending
            .take()
            .filter(|(_, at)| now.duration_since(*at) < TTL)
            .map(|(id, _)| id)
    }
}

#[derive(Default)]
pub struct PendingInvite(Mutex<Inbox>);

#[tauri::command]
pub fn take_public_invite(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, PendingInvite>,
) -> Option<Uuid> {
    if window.label() != "main" {
        return None;
    }
    state.0.lock().ok()?.take(Instant::now())
}

#[cfg(windows)]
pub mod windows {
    use super::*;
    use std::fs::{self, File, OpenOptions};
    use std::io;
    use std::os::windows::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use tauri::{Emitter, Manager};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};

    const EXCHANGE: Duration = Duration::from_millis(600);

    // No invitation or account data is written here. The filename advertises a
    // random local pipe only in this user's app directory. The locked file
    // prevents another launcher from removing a live registration.
    struct Registration {
        path: PathBuf,
        marker: Option<File>,
    }
    impl Drop for Registration {
        fn drop(&mut self) {
            drop(self.marker.take());
            let _ = fs::remove_file(&self.path);
        }
    }

    fn name(id: Uuid) -> String {
        format!(r"\\.\pipe\aurora-public-invite-{id}")
    }
    fn directory(data: &Path) -> PathBuf {
        data.join("public-invite-endpoints")
    }

    fn register(data: &Path, primary: bool) -> io::Result<(Registration, NamedPipeServer)> {
        let directory = directory(data);
        fs::create_dir_all(&directory)?;
        let endpoint = Uuid::new_v4();
        // Default Windows pipe DACL permits write access to its creator/system/
        // administrators. The client needs write access. Refuse network clients.
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .max_instances(1)
            .in_buffer_size(256)
            .out_buffer_size(256)
            .create(name(endpoint))?;
        let prefix = if primary { "primary" } else { "session" };
        let path = directory.join(format!("{prefix}-{endpoint}"));
        let marker = OpenOptions::new()
            .create_new(true)
            .write(true)
            .share_mode(0)
            .open(&path)?;
        Ok((
            Registration {
                path,
                marker: Some(marker),
            },
            server,
        ))
    }

    async fn send(endpoint: Uuid, id: Uuid) -> io::Result<bool> {
        let address = name(endpoint);
        let mut client = loop {
            match ClientOptions::new().open(&address) {
                Ok(client) => break client,
                Err(e) if e.raw_os_error() == Some(231) => {
                    tokio::time::sleep(Duration::from_millis(20)).await
                }
                Err(e) => return Err(e),
            }
        };
        client.write_all(id.to_string().as_bytes()).await?;
        Ok(client.read_u8().await? == 1)
    }

    pub async fn forward(data: &Path, id: Uuid) -> bool {
        let Ok(entries) = fs::read_dir(directory(data)) else {
            return false;
        };
        let mut endpoints = Vec::new();
        for entry in entries.flatten().take(256) {
            let filename = entry.file_name();
            let Some((role, endpoint)) = filename.to_str().and_then(|s| s.split_once('-')) else {
                continue;
            };
            let Some(endpoint) = canonical_id(endpoint) else {
                continue;
            };
            if role != "primary" && role != "session" {
                continue;
            }
            // Dead processes release all handles, including after a crash.
            // A live marker's share mode denies deletion.
            if fs::remove_file(entry.path()).is_ok() {
                continue;
            }
            endpoints.push((role == "primary", endpoint));
        }
        endpoints.sort_by_key(|(primary, _)| !primary);
        let sessions = endpoints.iter().filter(|(primary, _)| !primary).count();
        let attempt = async {
            for (primary, endpoint) in endpoints {
                // With several temporary accounts and no remembered account,
                // let the new primary window present its own confirmation.
                if !primary && sessions > 1 {
                    continue;
                }
                if matches!(
                    tokio::time::timeout(EXCHANGE, send(endpoint, id)).await,
                    Ok(Ok(true))
                ) {
                    return true;
                }
            }
            false
        };
        tokio::time::timeout(Duration::from_secs(2), attempt)
            .await
            .unwrap_or(false)
    }

    async fn serve(server: NamedPipeServer, registration: Registration, receive: impl Fn(Uuid)) {
        let _registration = registration;
        let mut server = server;
        while server.connect().await.is_ok() {
            let exchange = async {
                let mut bytes = [0; 36];
                server.read_exact(&mut bytes).await?;
                let id = std::str::from_utf8(&bytes).ok().and_then(canonical_id);
                if let Some(id) = id {
                    receive(id);
                }
                server.write_u8(u8::from(id.is_some())).await?;
                // Wait for the sender to close after reading the acknowledgment
                // before DisconnectNamedPipe discards unread client data.
                let mut end = [0];
                let _ = server.read(&mut end).await?;
                Ok::<_, io::Error>(())
            };
            let _ = tokio::time::timeout(EXCHANGE, exchange).await;
            if let Err(error) = server.disconnect() {
                // A sender can close its handle before DisconnectNamedPipe.
                if error.raw_os_error() != Some(233) {
                    break;
                }
            }
        }
    }

    pub fn initialize(app: &tauri::App, initial: Option<Uuid>) {
        if let Some(id) = initial {
            if let Ok(mut inbox) = app.state::<PendingInvite>().0.lock() {
                inbox.receive(id, Instant::now());
            }
        }
        let Ok(data) = app.path().app_data_dir() else {
            return;
        };
        let primary = app
            .state::<crate::launcher_sessions::LauncherSessionState>()
            .primary_for_invites(&data)
            .unwrap_or(false);
        let Ok((registration, server)) =
            tauri::async_runtime::block_on(async { register(&data, primary) })
        else {
            return;
        };
        let app = app.handle().clone();
        tauri::async_runtime::spawn(serve(server, registration, move |id| {
            let changed = app
                .state::<PendingInvite>()
                .0
                .lock()
                .is_ok_and(|mut inbox| inbox.receive(id, Instant::now()));
            if changed {
                let target = app.clone();
                let _ = app.run_on_main_thread(move || {
                    super::super::show_launcher(&target);
                    let _ = target.emit_to("main", "aurora-public-invite", ());
                });
            }
        }));
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn windows_pipe_prefers_primary_preserves_other_accounts_and_ignores_stale_markers() {
            let data = std::env::temp_dir().join(format!("aurora-invite-qa-{}", Uuid::new_v4()));
            let (main_registration, main_pipe) = register(&data, true).unwrap();
            let (secondary_registration, secondary_pipe) = register(&data, false).unwrap();
            let stale = directory(&data).join(format!("primary-{}", Uuid::new_v4()));
            fs::write(&stale, []).unwrap();
            let (main_tx, mut main_rx) = tokio::sync::mpsc::unbounded_channel();
            let (secondary_tx, mut secondary_rx) = tokio::sync::mpsc::unbounded_channel();
            let main_task = tokio::spawn(serve(main_pipe, main_registration, move |id| {
                let _ = main_tx.send(id);
            }));
            let secondary_task =
                tokio::spawn(serve(secondary_pipe, secondary_registration, move |id| {
                    let _ = secondary_tx.send(id);
                }));
            for _ in 0..2 {
                let id = Uuid::new_v4();
                assert!(forward(&data, id).await);
                assert_eq!(main_rx.recv().await, Some(id));
                assert!(secondary_rx.try_recv().is_err());
            }
            assert!(!stale.exists());
            main_task.abort();
            let _ = main_task.await;
            let id = Uuid::new_v4();
            assert!(forward(&data, id).await);
            assert_eq!(secondary_rx.recv().await, Some(id));
            let (other_registration, other_pipe) = register(&data, false).unwrap();
            assert!(!forward(&data, Uuid::new_v4()).await);
            drop(other_registration);
            drop(other_pipe);
            secondary_task.abort();
            let _ = secondary_task.await;
            assert!(!forward(&data, Uuid::new_v4()).await);
            fs::remove_dir(directory(&data)).unwrap();
            fs::remove_dir(data).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_exact_public_invite_argument_is_accepted() {
        let id = Uuid::new_v4();
        let url = format!("{SCHEME}{id}");
        assert_eq!(from_arguments([OsString::from(&url)].into_iter()), Some(id));
        for wrong in [
            format!("{url}/"),
            format!("{url}?"),
            format!("{url}#x"),
            format!("{SCHEME}../{id}"),
            url.to_uppercase(),
            format!("aurora-connect://join:80/{id}"),
            format!("aurora-connect://a@join/{id}"),
            format!("{SCHEME}00000000-0000-4000-0000-000000000000"),
            format!("{SCHEME}{}", Uuid::nil()),
            format!("{SCHEME}{{{id}}}"),
        ] {
            assert_eq!(from_arguments([wrong.into()].into_iter()), None);
        }
        assert_eq!(
            from_arguments([url.into(), "--file".into()].into_iter()),
            None
        );
    }
    #[test]
    fn latest_invitation_is_consumed_once_with_bounded_deduplication_and_expiry() {
        let now = Instant::now();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut inbox = Inbox::default();
        assert!(inbox.receive(a, now));
        assert!(inbox.receive(b, now));
        assert_eq!(inbox.take(now), Some(b));
        assert!(!inbox.receive(a, now));
        assert_eq!(inbox.take(now), None);
        assert!(inbox.receive(a, now + TTL));
        assert_eq!(inbox.take(now + TTL + TTL), None);
    }
}
