//! Downloads oficiais que não possuem URL de distribuição na API CurseForge.
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};

use aurora_launcher_core::engine::{
    download_modrinth_mirror_with_progress, resolve_modrinth_file_by_sha1, InstanceMutationGuard,
};
use serde::Serialize;
use tauri::{
    webview::{DownloadEvent, NewWindowResponse},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};

#[derive(Default)]
pub struct ManualDownloads(Mutex<Registry>);

#[derive(Default)]
struct Registry {
    active: BTreeMap<String, Arc<Session>>,
    cancelled: BTreeMap<String, Instant>,
}

impl Registry {
    fn remember_cancellation(&mut self, request_id: &str) {
        self.cancelled
            .retain(|_, time| time.elapsed() < Duration::from_secs(10 * 60));
        if self.cancelled.len() >= 1024 {
            if let Some(oldest) = self
                .cancelled
                .iter()
                .min_by_key(|(_, time)| **time)
                .map(|(id, _)| id.clone())
            {
                self.cancelled.remove(&oldest);
            }
        }
        self.cancelled.insert(request_id.to_owned(), Instant::now());
    }

    fn register(&mut self, session: Arc<Session>) -> Result<(), String> {
        let id = &session.request.request_id;
        if self.cancelled.contains_key(id) {
            return Err("download cancelado".to_owned());
        }
        if self.active.contains_key(id) {
            return Err("download já registrado".to_owned());
        }
        self.active.insert(id.clone(), session);
        Ok(())
    }
}

pub struct Request {
    pub request_id: String,
    pub instance_id: String,
    pub filename: String,
    pub content_type: String,
    pub sha1: String,
    pub page_url: reqwest::Url,
    pub root: PathBuf,
    pub destination: PathBuf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Resolving,
    WaitingForPage,
    Downloading,
    Finished,
}

struct State {
    phase: Phase,
    browser_download_url: Option<reqwest::Url>,
    rejected_browser_downloads: usize,
    // Dropped before notifying the frontend, which may start the next file immediately.
    mutation_lock: Option<InstanceMutationGuard>,
}

struct Session {
    request: Request,
    temporary: PathBuf,
    label: String,
    state: Mutex<State>,
    stop_timeout: mpsc::Sender<()>,
}

impl State {
    fn request_browser_download(&mut self, url: reqwest::Url) -> bool {
        if self.phase != Phase::WaitingForPage {
            if self.browser_download_url.as_ref() == Some(&url) {
                self.rejected_browser_downloads = self.rejected_browser_downloads.saturating_add(1);
            }
            return false;
        }
        self.phase = Phase::Downloading;
        self.browser_download_url = Some(url);
        true
    }

    fn accept_browser_completion(&mut self, url: &reqwest::Url, success: bool) -> bool {
        if self.browser_download_url.as_ref() != Some(url) {
            return false;
        }
        // WebView2 reports Interrupted even for duplicate requests we rejected.
        if self.phase != Phase::Finished && !success && self.rejected_browser_downloads > 0 {
            self.rejected_browser_downloads -= 1;
            return false;
        }
        true
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    request_id: String,
    instance_id: String,
    filename: String,
    content_type: String,
    status: String,
    error: Option<String>,
}

enum Completion {
    ExistingFile,
    Downloaded(Option<PathBuf>),
}

impl Session {
    fn complete(&self, outcome: Result<Completion, String>) -> Option<Result<(), String>> {
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        if state.phase == Phase::Finished {
            let _ = fs::remove_file(&self.temporary);
            return None;
        }
        // Cancellation and publication share the lock: late events cannot install a cancelled file.
        let result = outcome.and_then(|completion| match completion {
            Completion::ExistingFile => Ok(()),
            Completion::Downloaded(path) => crate::commands::finalize_manual_download(
                path.as_deref(),
                &self.temporary,
                &self.request.destination,
                &self.request.sha1,
            ),
        });
        state.phase = Phase::Finished;
        state.mutation_lock.take();
        drop(state);
        if result.is_err() {
            let _ = fs::remove_file(&self.temporary);
        }
        Some(result)
    }
}

fn emit(app: &AppHandle, session: &Session, status: &str, error: Option<String>) {
    let _ = app.emit(
        "aurora-manual-download",
        Event {
            request_id: session.request.request_id.clone(),
            instance_id: session.request.instance_id.clone(),
            filename: session.request.filename.clone(),
            content_type: session.request.content_type.clone(),
            status: status.to_owned(),
            error,
        },
    );
}

fn finished(session: &Session) -> bool {
    session
        .state
        .lock()
        .map_or(true, |state| state.phase == Phase::Finished)
}

fn finish(app: &AppHandle, session: &Session, outcome: Result<Completion, String>) {
    let Some(result) = session.complete(outcome) else {
        return;
    };
    let _ = session.stop_timeout.send(());
    if let Ok(mut downloads) = app.state::<ManualDownloads>().0.lock() {
        downloads.active.remove(&session.request.request_id);
    }
    if let Some(window) = app.get_webview_window(&session.label) {
        let _ = window.destroy();
    }
    emit(
        app,
        session,
        if result.is_ok() {
            "completed"
        } else {
            "failed"
        },
        result.err(),
    );
}

pub fn cancel(app: &AppHandle, request_id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(request_id).map_err(|_| "identificador de download inválido")?;
    let session = {
        let registry = app.state::<ManualDownloads>();
        let mut downloads = registry
            .0
            .lock()
            .map_err(|_| "não foi possível cancelar o download")?;
        // Cancellation can arrive before the async open command has registered.
        downloads.remember_cancellation(request_id);
        downloads.active.get(request_id).cloned()
    };
    if let Some(session) = session {
        finish(app, &session, Err("download cancelado".to_owned()));
    }
    Ok(())
}

pub fn official_navigation_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && url.host_str().is_some_and(|host| {
            host == "curseforge.com"
                || host.ends_with(".curseforge.com")
                || host == "forgecdn.net"
                || host.ends_with(".forgecdn.net")
        })
}

fn check_regular_path(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err(
                        "o destino do download não pode ser um link ou uma junção".to_owned()
                    );
                }
            }
            if metadata.file_type().is_symlink() {
                return Err("o destino do download não pode ser um link".to_owned());
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("não foi possível verificar o destino do download".to_owned()),
    }
}

fn prepare_destination(root: &Path, destination: &Path) -> Result<(), String> {
    let relative = destination
        .strip_prefix(root)
        .map_err(|_| "destino fora da instância")?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("destino inválido".to_owned());
    }
    let mut current = root.to_path_buf();
    check_regular_path(&current)?;
    let components = relative.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        check_regular_path(&current)?;
        if index + 1 < components.len() && !current.exists() {
            fs::create_dir(&current).map_err(|_| "não foi possível preparar o download")?;
            check_regular_path(&current)?;
        }
        if index + 1 == components.len() && current.is_dir() {
            return Err("o destino do arquivo é uma pasta".to_owned());
        }
    }
    Ok(())
}

pub async fn start(
    app: AppHandle,
    request: Request,
    mutation_lock: InstanceMutationGuard,
) -> Result<(), String> {
    uuid::Uuid::parse_str(&request.request_id).map_err(|_| "identificador de download inválido")?;
    if !official_navigation_url(&request.page_url)
        || !request
            .page_url
            .host_str()
            .is_some_and(|host| host == "curseforge.com" || host.ends_with(".curseforge.com"))
    {
        return Err("página oficial do CurseForge inválida".to_owned());
    }
    prepare_destination(&request.root, &request.destination)?;
    let label = format!("curseforge-download-{}", request.request_id);
    let temporary = request.destination.with_file_name(format!(
        ".{}.aurora-manual-download",
        uuid::Uuid::new_v4().simple()
    ));
    let (stop_timeout, timeout) = mpsc::channel();
    let session = Arc::new(Session {
        request,
        temporary,
        label,
        state: Mutex::new(State {
            phase: Phase::Resolving,
            browser_download_url: None,
            rejected_browser_downloads: 0,
            mutation_lock: Some(mutation_lock),
        }),
        stop_timeout,
    });
    {
        let registry = app.state::<ManualDownloads>();
        let mut downloads = registry
            .0
            .lock()
            .map_err(|_| "não foi possível registrar o download")?;
        downloads.register(Arc::clone(&session))?;
    }
    let timer_app = app.clone();
    let timer_session = Arc::clone(&session);
    std::thread::spawn(move || {
        if timeout.recv_timeout(Duration::from_secs(10 * 60)).is_err() {
            finish(
                &timer_app,
                &timer_session,
                Err("o download excedeu o limite de 10 minutos; tente novamente".to_owned()),
            );
        }
    });
    emit(&app, &session, "resolving", None);
    let automatic_app = app.clone();
    let automatic_session = Arc::clone(&session);
    let automatic = tauri::async_runtime::spawn_blocking(move || {
        let request = &automatic_session.request;
        if crate::commands::verify_manual_download_file(&request.destination, &request.sha1).is_ok()
        {
            finish(
                &automatic_app,
                &automatic_session,
                Ok(Completion::ExistingFile),
            );
            return true;
        }
        // Never replace a CurseForge manifest ZIP with a Modrinth .mrpack.
        if request.content_type != "modpack" {
            if let Ok(Some(mirror)) = resolve_modrinth_file_by_sha1(&request.sha1) {
                if finished(&automatic_session) {
                    return true;
                }
                emit(&automatic_app, &automatic_session, "downloading", None);
                if download_modrinth_mirror_with_progress(
                    &mirror,
                    &automatic_session.temporary,
                    |progress| {
                        if !finished(&automatic_session) {
                            crate::commands::emit_transfer_progress(&automatic_app, progress);
                        }
                    },
                    || finished(&automatic_session),
                )
                .is_ok()
                {
                    finish(
                        &automatic_app,
                        &automatic_session,
                        Ok(Completion::Downloaded(None)),
                    );
                    return true;
                }
            }
        }
        finished(&automatic_session)
    })
    .await;
    match automatic {
        Ok(true) => return Ok(()),
        Err(_) => {
            finish(
                &app,
                &session,
                Err("a tentativa de download automático falhou".to_owned()),
            );
            return Ok(());
        }
        Ok(false) => {}
    }
    {
        let mut state = session
            .state
            .lock()
            .map_err(|_| "estado do download indisponível")?;
        if state.phase == Phase::Finished {
            return Ok(());
        }
        state.phase = Phase::WaitingForPage;
    }
    let download_app = app.clone();
    let download_session = Arc::clone(&session);
    let popup_app = app.clone();
    let popup_label = session.label.clone();
    let window = WebviewWindowBuilder::new(&app, &session.label, WebviewUrl::External(session.request.page_url.clone()))
        .title(format!("Aurora · Baixando {}", session.request.filename))
        .inner_size(980.0, 720.0).min_inner_size(720.0, 520.0).center()
        .on_navigation(official_navigation_url)
        .on_new_window(move |url, _| {
            if official_navigation_url(&url) {
                if let Some(window) = popup_app.get_webview_window(&popup_label) { let _ = window.navigate(url); }
            }
            NewWindowResponse::Deny
        })
        .on_download(move |_, event| match event {
            DownloadEvent::Requested { url, destination } => {
                if !official_navigation_url(&url) { return false; }
                let Ok(mut state) = download_session.state.try_lock() else { return false };
                if !state.request_browser_download(url) { return false; }
                *destination = download_session.temporary.clone();
                drop(state);
                emit(&download_app, &download_session, "downloading", None);
                true
            },
            DownloadEvent::Finished { url, path, success } => {
                let app = download_app.clone();
                let session = Arc::clone(&download_session);
                tauri::async_runtime::spawn_blocking(move || {
                    let Ok(mut state) = session.state.lock() else { return };
                    if !state.accept_browser_completion(&url, success) { return; }
                    drop(state);
                    finish(&app, &session, if success { Ok(Completion::Downloaded(path)) }
                        else { Err("o CurseForge não concluiu o download; tente novamente na página oficial".to_owned()) });
                });
                true
            },
            _ => true,
        }).build();
    match window {
        Ok(window) => {
            let close_app = app.clone();
            let close_session = Arc::clone(&session);
            window.on_window_event(move |event| {
                if matches!(event, WindowEvent::Destroyed) {
                    let app = close_app.clone();
                    let session = Arc::clone(&close_session);
                    tauri::async_runtime::spawn_blocking(move || {
                        finish(
                            &app,
                            &session,
                            Err("a janela de download foi fechada".to_owned()),
                        );
                    });
                }
            });
            if finished(&session) {
                let _ = window.destroy();
            }
        }
        Err(_) => finish(
            &app,
            &session,
            Err("não foi possível abrir a página oficial de download".to_owned()),
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_official_cdn_redirects_but_not_lookalikes_or_credentials() {
        for url in [
            "https://www.curseforge.com/minecraft/mc-mods/demo/download/1",
            "https://edge.forgecdn.net/files/1/a.jar",
            "https://mediafilez.forgecdn.net/files/1/a.jar",
        ] {
            assert!(official_navigation_url(&reqwest::Url::parse(url).unwrap()));
        }
        for url in [
            "http://edge.forgecdn.net/a.jar",
            "https://forgecdn.net.evil.test/a.jar",
            "https://evilforgecdn.net/a.jar",
            "https://edge.forgecdn.net:8443/a.jar",
            "https://user@www.curseforge.com/a",
            "https://evil.test/a.jar",
        ] {
            assert!(!official_navigation_url(&reqwest::Url::parse(url).unwrap()));
        }
    }

    #[test]
    fn confines_destination_to_instance() {
        let root =
            std::env::temp_dir().join(format!("aurora-external-path-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        assert!(prepare_destination(&root, &root.join("mods/good.jar")).is_ok());
        assert!(prepare_destination(&root, &root.join("../bad.jar")).is_err());
        assert!(prepare_destination(&root, &root.with_file_name("outside.jar")).is_err());
        assert!(prepare_destination(&root, &root).is_err());
        assert!(prepare_destination(&root, &root.join("mods")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    fn session_for_test() -> (PathBuf, aurora_launcher_core::engine::Instance, Session) {
        use aurora_launcher_core::engine::{
            acquire_instance_mutation_lock, InstanceId, InstanceLayout,
        };
        let root =
            std::env::temp_dir().join(format!("aurora-download-session-{}", uuid::Uuid::new_v4()));
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("example").unwrap());
        instance.ensure_layout().unwrap();
        let (stop_timeout, _) = mpsc::channel();
        let session = Session {
            request: Request {
                request_id: uuid::Uuid::new_v4().to_string(),
                instance_id: "example".to_owned(),
                filename: "example.jar".to_owned(),
                content_type: "mod".to_owned(),
                sha1: "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".to_owned(),
                page_url: reqwest::Url::parse(
                    "https://www.curseforge.com/minecraft/mc-mods/example/download/1",
                )
                .unwrap(),
                root: instance.root().to_path_buf(),
                destination: instance.mods_dir().join("example.jar"),
            },
            temporary: instance.mods_dir().join(".exclusive.part"),
            label: "test".to_owned(),
            state: Mutex::new(State {
                phase: Phase::WaitingForPage,
                browser_download_url: None,
                rejected_browser_downloads: 0,
                mutation_lock: Some(acquire_instance_mutation_lock(&instance).unwrap()),
            }),
            stop_timeout,
        };
        (root, instance, session)
    }

    #[test]
    fn cancellation_is_terminal_releases_lock_and_rejects_late_publication() {
        use aurora_launcher_core::engine::{acquire_instance_mutation_lock, ContentUpdateError};
        let (root, instance, session) = session_for_test();
        fs::write(&session.request.destination, b"original").unwrap();
        fs::write(&session.temporary, b"partial").unwrap();
        assert!(matches!(
            acquire_instance_mutation_lock(&instance),
            Err(ContentUpdateError::Busy)
        ));
        assert!(session
            .complete(Err("cancelled".to_owned()))
            .unwrap()
            .is_err());
        let next_lock = acquire_instance_mutation_lock(&instance).unwrap();
        assert!(!session.temporary.exists());
        fs::write(&session.temporary, b"hello").unwrap();
        assert!(session.complete(Ok(Completion::Downloaded(None))).is_none());
        assert!(!session.temporary.exists());
        assert_eq!(fs::read(&session.request.destination).unwrap(), b"original");
        drop(next_lock);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn success_releases_instance_before_next_file_and_completes_once() {
        let (root, instance, session) = session_for_test();
        fs::write(&session.temporary, b"hello").unwrap();
        session
            .complete(Ok(Completion::Downloaded(None)))
            .unwrap()
            .unwrap();
        let next = aurora_launcher_core::engine::acquire_instance_mutation_lock(&instance).unwrap();
        assert_eq!(fs::read(&session.request.destination).unwrap(), b"hello");
        assert!(session.complete(Err("late failure".to_owned())).is_none());
        drop(next);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_before_registration_is_remembered_and_bounded() {
        let (root, instance, session) = session_for_test();
        let mut registry = Registry::default();
        registry.remember_cancellation(&session.request.request_id);
        assert!(registry.register(Arc::new(session)).is_err());
        assert!(registry.active.is_empty());
        let next = aurora_launcher_core::engine::acquire_instance_mutation_lock(&instance).unwrap();
        for _ in 0..1100 {
            registry.remember_cancellation(&uuid::Uuid::new_v4().to_string());
        }
        assert!(registry.cancelled.len() <= 1024);
        drop(next);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejected_duplicate_completion_does_not_cancel_accepted_download() {
        let (root, _, session) = session_for_test();
        let url = reqwest::Url::parse("https://edge.forgecdn.net/files/1/example.jar").unwrap();
        let mut state = session.state.lock().unwrap();
        assert!(state.request_browser_download(url.clone()));
        assert!(!state.request_browser_download(url.clone()));
        assert!(!state.accept_browser_completion(&url, false));
        assert!(state.accept_browser_completion(&url, true));
        let unrelated = reqwest::Url::parse("https://www.curseforge.com/another").unwrap();
        assert!(!state.accept_browser_completion(&unrelated, true));
        drop(state);
        drop(session);
        fs::remove_dir_all(root).unwrap();
    }
}
