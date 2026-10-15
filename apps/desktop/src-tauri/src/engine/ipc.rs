//! Canal IPC local entre o Launcher e o Aurora Core.
//!
//! O servidor só escuta em `127.0.0.1` e cada execução recebe um nonce novo.
//! Esse nonce identifica o processo iniciado pelo Aurora; não é uma credencial
//! de conta e nunca deve ser persistido ou registrado em logs.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tungstenite::{
    accept_with_config, handshake::HandshakeError, protocol::WebSocketConfig, Message, WebSocket,
};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("não foi possível abrir o canal IPC local: {0}")]
    Io(#[from] io::Error),
}

/// Dados que o Core entrega à preparação de inicialização do Minecraft.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcEndpoint {
    pub port: u16,
    pub nonce: String,
}

/// Projeção pública da sessão enviada ao Core. Credenciais e tokens não fazem
/// parte deste tipo por projeto.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcSessionProfile {
    aurora_user_id: String,
    minecraft_uuid: String,
    username: String,
    state: String,
    scopes: Vec<String>,
}

impl IpcSessionProfile {
    pub fn offline(minecraft_uuid: Uuid, username: impl Into<String>) -> Self {
        Self {
            aurora_user_id: String::new(),
            minecraft_uuid: minecraft_uuid.to_string(),
            username: username.into(),
            state: "offline".to_owned(),
            scopes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum IpcEvent {
    Connected {
        loader: String,
        minecraft_version: String,
    },
    /// Evento extensível de um módulo. Campos com aparência de credencial são
    /// recusados antes de o evento alcançar a interface do Launcher.
    ModuleEvent {
        name: String,
        payload: serde_json::Value,
    },
    Disconnected,
}

/// Servidor de uma execução. Ao ser descartado, o listener é encerrado.
pub struct IpcServer {
    endpoint: IpcEndpoint,
    receiver: Receiver<IpcEvent>,
    shutdown: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    outbound: SyncSender<String>,
}

impl IpcServer {
    /// Abre uma porta efêmera exclusivamente no loopback IPv4.
    pub fn start() -> Result<Self, IpcError> {
        Self::start_with_session(None)
    }

    pub fn start_with_session(session: Option<IpcSessionProfile>) -> Result<Self, IpcError> {
        let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let endpoint = IpcEndpoint {
            port,
            nonce: Uuid::new_v4().simple().to_string(),
        };
        let (sender, receiver) = mpsc::sync_channel(64);
        let (outbound, outbound_receiver) = mpsc::sync_channel(64);
        let shutdown = Arc::new(AtomicBool::new(false));
        let connected = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let worker_connected = Arc::clone(&connected);
        let worker_endpoint = endpoint.clone();
        let worker = thread::spawn(move || {
            accept_connections(
                listener,
                worker_endpoint,
                sender,
                outbound_receiver,
                session,
                worker_shutdown,
                worker_connected,
            )
        });

        Ok(Self {
            endpoint,
            receiver,
            shutdown,
            connected,
            worker: Some(worker),
            outbound,
        })
    }

    pub fn endpoint(&self) -> &IpcEndpoint {
        &self.endpoint
    }

    /// Recebe eventos sem bloquear a interface do Launcher.
    pub fn try_recv(&self) -> Result<IpcEvent, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }

    pub fn send_json(&self, value: &serde_json::Value) -> Result<(), IpcError> {
        self.outbound
            .try_send(value.to_string())
            .map_err(|error| IpcError::Io(io::Error::new(io::ErrorKind::WouldBlock, error)))
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn accept_connections(
    listener: TcpListener,
    endpoint: IpcEndpoint,
    sender: SyncSender<IpcEvent>,
    outbound: Receiver<String>,
    session: Option<IpcSessionProfile>,
    shutdown: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, address)) if address.ip().is_loopback() => {
                let expected_nonce = endpoint.nonce.clone();
                let event_sender = sender.clone();
                handle_connection(
                    stream,
                    expected_nonce,
                    event_sender,
                    &outbound,
                    session.as_ref(),
                    &shutdown,
                    &connected,
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(_) => break,
        }
    }
}

fn handle_connection(
    stream: TcpStream,
    expected_nonce: String,
    sender: SyncSender<IpcEvent>,
    outbound: &Receiver<String>,
    session: Option<&IpcSessionProfile>,
    shutdown: &AtomicBool,
    connected: &AtomicBool,
) {
    let Some(mut socket) = upgrade_connection(stream, shutdown, Duration::from_secs(5)) else {
        return;
    };
    let mut authenticated = false;
    let handshake_started = Instant::now();

    while !shutdown.load(Ordering::Acquire) {
        if !authenticated && handshake_started.elapsed() > Duration::from_secs(5) {
            let _ = socket.close(None);
            return;
        }
        if authenticated {
            while let Ok(outbound_message) = outbound.try_recv() {
                if socket.send(Message::Text(outbound_message)).is_err() {
                    break;
                }
            }
        }
        let message = match socket.read() {
            Ok(message) => message,
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(error) => {
                eprintln!("[Aurora IPC] conexão encerrada: {error}");
                break;
            }
        };
        // The last blocking read or already-buffered frame may have crossed the
        // deadline/cancellation after the wrapper's pre-IO observation.
        if socket.get_ref().check().is_err() {
            break;
        }
        let text = match message.to_text() {
            Ok(text) => text,
            Err(_) => continue,
        };
        let parsed = match serde_json::from_str::<WireMessage>(text) {
            Ok(message) => message,
            Err(_) => continue,
        };

        if !authenticated {
            if parsed.kind != "hello"
                || parsed.nonce.as_deref() != Some(expected_nonce.as_str())
                || parsed.protocol != Some(1)
                || !parsed
                    .core_version
                    .as_deref()
                    .is_some_and(|version| version.starts_with("1."))
            {
                eprintln!("[Aurora IPC] autenticação local recusada");
                let _ = socket.close(None);
                return;
            }
            authenticated = true;
            socket.get_mut().deadline = None;
            connected.store(true, Ordering::Release);
            let _ = socket.send(Message::Text(r#"{"kind":"accepted"}"#.into()));
            if let Some(session) = session {
                if let Ok(mut value) = serde_json::to_value(session) {
                    value["kind"] = serde_json::Value::String("session".to_owned());
                    let _ = socket.send(Message::Text(value.to_string()));
                }
            }
            let _ = sender.try_send(IpcEvent::Connected {
                loader: parsed.loader.unwrap_or_else(|| "unknown".into()),
                minecraft_version: parsed.minecraft_version.unwrap_or_else(|| "unknown".into()),
            });
            continue;
        }

        if contains_sensitive_key(&parsed.payload) {
            continue;
        }

        if retired_companion_message(&parsed.kind) {
            // Legacy clients cannot reactivate the removed in-game bridge, even
            // through the generic module-event extension point.
            continue;
        } else if valid_event_name(&parsed.kind)
            && serde_json::to_vec(&parsed.payload).is_ok_and(|value| value.len() <= 65_536)
            && !contains_sensitive_key(&parsed.payload)
        {
            let _ = sender.try_send(IpcEvent::ModuleEvent {
                name: parsed.kind,
                payload: serde_json::Value::Object(parsed.payload.into_iter().collect()),
            });
        }
    }

    if authenticated {
        connected.store(false, Ordering::Release);
        let _ = sender.try_send(IpcEvent::Disconnected);
    }
}

/// Upgrade is cancellable before authentication: a silent loopback client must
/// not hold the owned server's Drop/join forever. Tungstenite 0.24 also bounds
/// the HTTP upgrade to 64 KiB/512 successful reads; the elapsed deadline below
/// is never reset by a partial request. The Core's existing 1 MiB message bound
/// is enforced here as well, without changing hello/nonce/protocol semantics.
struct GuardedStream<'a> {
    inner: TcpStream,
    shutdown: &'a AtomicBool,
    deadline: Option<Instant>,
}

impl GuardedStream<'_> {
    fn check(&self) -> io::Result<()> {
        if self.shutdown.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "IPC stopped",
            ));
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "IPC handshake deadline",
            ));
        }
        Ok(())
    }
}

impl Read for GuardedStream<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.check()?;
        self.inner.read(buffer)
    }
}

impl Write for GuardedStream<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.check()?;
        self.inner.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.check()?;
        self.inner.flush()
    }
}

fn upgrade_connection<'a>(
    stream: TcpStream,
    shutdown: &'a AtomicBool,
    timeout: Duration,
) -> Option<WebSocket<GuardedStream<'a>>> {
    upgrade_connection_observed(stream, shutdown, timeout, || {})
}

fn upgrade_connection_observed<'a>(
    stream: TcpStream,
    shutdown: &'a AtomicBool,
    timeout: Duration,
    mut waiting: impl FnMut(),
) -> Option<WebSocket<GuardedStream<'a>>> {
    if shutdown.load(Ordering::Acquire) || stream.set_nonblocking(true).is_err() {
        return None;
    }
    let started = Instant::now();
    let config = WebSocketConfig {
        max_message_size: Some(1024 * 1024),
        max_frame_size: Some(1024 * 1024),
        ..WebSocketConfig::default()
    };
    let stream = GuardedStream {
        inner: stream,
        shutdown,
        deadline: Some(started + timeout),
    };
    let mut result = accept_with_config(stream, Some(config));
    loop {
        if shutdown.load(Ordering::Acquire) || started.elapsed() >= timeout {
            return None;
        }
        match result {
            Ok(mut socket) => {
                let stream = &socket.get_ref().inner;
                if stream.set_nonblocking(false).is_err()
                    || stream
                        .set_read_timeout(Some(Duration::from_millis(75)))
                        .is_err()
                    || stream
                        .set_write_timeout(Some(Duration::from_millis(75)))
                        .is_err()
                {
                    return None;
                }
                // TCP timeouts alone do not interrupt read() while a peer keeps
                // sending unfinished WebSocket continuations. Check the absolute
                // hello deadline and cancellation at every underlying IO instead.
                socket.get_mut().deadline = Some(Instant::now() + Duration::from_secs(5));
                return Some(socket);
            }
            Err(HandshakeError::Interrupted(mid)) => {
                waiting();
                thread::sleep(Duration::from_millis(5));
                if shutdown.load(Ordering::Acquire) || started.elapsed() >= timeout {
                    return None;
                }
                result = mid.handshake();
            }
            Err(HandshakeError::Failure(_)) => return None,
        }
    }
}

fn valid_event_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().enumerate().all(|(index, character)| {
            (index == 0 && character.is_ascii_alphabetic())
                || (index > 0
                    && (character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')))
        })
}

fn contains_sensitive_key(fields: &BTreeMap<String, serde_json::Value>) -> bool {
    fields.iter().any(|(key, value)| {
        is_sensitive_key(key)
            || match value {
                serde_json::Value::Object(nested) => {
                    nested.iter().any(|(nested_key, nested_value)| {
                        is_sensitive_key(nested_key) || contains_sensitive_value(nested_value)
                    })
                }
                _ => contains_sensitive_value(value),
            }
    })
}

fn contains_sensitive_value(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(nested) => nested
            .iter()
            .any(|(key, value)| is_sensitive_key(key) || contains_sensitive_value(value)),
        serde_json::Value::Array(values) => values.iter().any(contains_sensitive_value),
        _ => false,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "token"
            | "accesstoken"
            | "refreshtoken"
            | "password"
            | "secret"
            | "authorization"
            | "cookie"
    )
}

fn retired_companion_message(kind: &str) -> bool {
    matches!(
        kind,
        "overlay"
            | "toggleAssistant"
            | "assistantRequest"
            | "assistantListen"
            | "assistantResponse"
            | "assistantCaption"
            | "assistantTranscript"
            | "telemetry"
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireMessage {
    kind: String,
    nonce: Option<String>,
    loader: Option<String>,
    minecraft_version: Option<String>,
    core_version: Option<String>,
    protocol: Option<u32>,
    #[serde(flatten)]
    payload: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tungstenite::connect;

    fn tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    fn unfinished_fragments_end(cancel: bool) -> (io::ErrorKind, usize) {
        let (client, stream) = tcp_pair();
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = shutdown.clone();
        let (finished, result) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let mut socket =
                upgrade_connection(stream, &worker_shutdown, Duration::from_secs(1)).unwrap();
            // Use a short private deadline instead of waiting five seconds or
            // altering the process/Windows clock. Cancellation also applies to
            // authenticated streams, whose hello deadline has been removed.
            socket.get_mut().deadline = if cancel {
                None
            } else {
                Some(Instant::now() + Duration::from_millis(120))
            };
            let kind = match socket.read() {
                Err(tungstenite::Error::Io(error)) => error.kind(),
                _ => panic!("unfinished fragments must end through the IO guard"),
            };
            finished.send(kind).unwrap();
        });
        let (mut client, _) = tungstenite::client("ws://localhost/aurora", client).unwrap();
        client
            .get_mut()
            .set_write_timeout(Some(Duration::from_millis(75)))
            .unwrap();
        // Masked empty TEXT without FIN, then masked empty continuations. No
        // complete WebSocket message can return control to the caller loop.
        client
            .get_mut()
            .write_all(&[0x01, 0x80, 1, 2, 3, 4])
            .unwrap();
        let stop_writer = Arc::new(AtomicBool::new(false));
        let writer_stop = stop_writer.clone();
        let (progress, observed_progress) = mpsc::sync_channel(1);
        let writer = thread::spawn(move || {
            let chunk = [0x00, 0x80, 1, 2, 3, 4].repeat(128);
            let mut bytes = 6;
            for index in 0..1024 {
                if writer_stop.load(Ordering::Acquire)
                    || client.get_mut().write_all(&chunk).is_err()
                {
                    break;
                }
                bytes += chunk.len();
                if index == 31 {
                    let _ = progress.send(());
                }
                thread::sleep(Duration::from_millis(1));
            }
            // Retain the still-open peer in the returned value until the server
            // result is observed; EOF must not be the reason for completion.
            (client, bytes)
        });
        if cancel {
            observed_progress
                .recv_timeout(Duration::from_secs(1))
                .unwrap();
            shutdown.store(true, Ordering::Release);
        }
        let outcome = result.recv_timeout(Duration::from_millis(500));
        stop_writer.store(true, Ordering::Release);
        let (client, bytes) = writer.join().unwrap();
        worker.join().unwrap();
        drop(client);
        (
            outcome.expect("continuous empty fragments must not bypass cancellation/deadline"),
            bytes,
        )
    }

    #[test]
    fn ipc_empty_continuations_cannot_hold_hello_past_its_absolute_deadline() {
        let (kind, bytes) = unfinished_fragments_end(false);
        assert_eq!(kind, io::ErrorKind::TimedOut);
        assert!(bytes <= 6 + 1024 * 128 * 6);
    }

    #[test]
    fn ipc_empty_continuations_remain_cancellable_after_hello_deadline_is_removed() {
        let (kind, bytes) = unfinished_fragments_end(true);
        assert_eq!(kind, io::ErrorKind::ConnectionAborted);
        assert!(bytes >= 6 + 32 * 128 * 6);
        assert!(bytes <= 6 + 1024 * 128 * 6);
    }

    #[test]
    fn ipc_guard_checks_cancel_and_deadline_even_with_readable_tcp_bytes() {
        let (mut client, stream) = tcp_pair();
        let shutdown = AtomicBool::new(false);
        let mut guarded = GuardedStream {
            inner: stream,
            shutdown: &shutdown,
            deadline: None,
        };
        client.write_all(b"pending").unwrap();
        let mut byte = [0];
        guarded.read_exact(&mut byte).unwrap();
        assert_eq!(byte[0], b'p');
        shutdown.store(true, Ordering::Release);
        assert_eq!(
            guarded.read(&mut byte).unwrap_err().kind(),
            io::ErrorKind::ConnectionAborted
        );
        shutdown.store(false, Ordering::Release);
        guarded.deadline = Some(Instant::now());
        assert_eq!(
            guarded.read(&mut byte).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
    }

    #[test]
    fn ipc_upgrade_silent_peer_cancels_without_waiting_for_peer_eof() {
        let (mut client, stream) = tcp_pair();
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = shutdown.clone();
        let (started, observed) = mpsc::sync_channel(1);
        let (finished, result) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            finished
                .send(
                    upgrade_connection_observed(
                        stream,
                        &worker_shutdown,
                        Duration::from_secs(5),
                        || {
                            let _ = started.try_send(());
                        },
                    )
                    .is_none(),
                )
                .unwrap();
        });
        observed.recv_timeout(Duration::from_secs(1)).unwrap();
        // The peer deliberately remains open while cancellation completes.
        std::io::Write::write_all(&mut client, b"G").unwrap();
        shutdown.store(true, Ordering::Release);
        assert!(result.recv_timeout(Duration::from_secs(1)).unwrap());
        worker.join().unwrap();
        drop(client);
    }

    #[test]
    fn ipc_upgrade_partial_progress_does_not_renew_deadline() {
        let (mut client, stream) = tcp_pair();
        let (finished, result) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let shutdown = AtomicBool::new(false);
            finished
                .send(upgrade_connection(stream, &shutdown, Duration::from_millis(120)).is_none())
                .unwrap();
        });
        for byte in b"GET /aurora" {
            if std::io::Write::write_all(&mut client, &[*byte]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(15));
        }
        assert!(result.recv_timeout(Duration::from_secs(1)).unwrap());
        worker.join().unwrap();
        drop(client);
    }

    #[test]
    fn ipc_upgrade_large_unfinished_header_is_bounded_and_authenticated_wire_still_works() {
        let (mut client, stream) = tcp_pair();
        client
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let (finished, result) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let shutdown = AtomicBool::new(false);
            finished
                .send(upgrade_connection(stream, &shutdown, Duration::from_secs(2)).is_none())
                .unwrap();
        });
        let mut header = b"GET /aurora HTTP/1.1\r\nX-Long: ".to_vec();
        header.extend(vec![b'x'; 70 * 1024]);
        let _ = std::io::Write::write_all(&mut client, &header);
        assert!(result.recv_timeout(Duration::from_secs(3)).unwrap());
        worker.join().unwrap();
        drop(client);
        let server = IpcServer::start().unwrap();
        let (mut socket, _) =
            connect(format!("ws://127.0.0.1:{}/aurora", server.endpoint().port)).unwrap();
        socket.send(Message::Text(serde_json::json!({"kind":"hello", "nonce":server.endpoint().nonce, "loader":"fabric", "minecraftVersion":"1.21.1", "coreVersion":"1.1.5", "protocol":1}).to_string())).unwrap();
        assert!(socket
            .read()
            .unwrap()
            .to_text()
            .unwrap()
            .contains("accepted"));
    }

    #[test]
    fn endpoint_is_loopback_port_with_fresh_nonce() {
        let server = IpcServer::start().expect("IPC deve iniciar");
        assert_ne!(server.endpoint().port, 0);
        assert_eq!(server.endpoint().nonce.len(), 32);
    }

    #[test]
    fn preserves_core_event_wire_contract() {
        assert_eq!(
            serde_json::to_value(IpcEvent::Connected {
                loader: "fabric".into(),
                minecraft_version: "1.20.1".into()
            })
            .unwrap(),
            serde_json::json!({
                "kind": "connected", "loader": "fabric", "minecraft_version": "1.20.1"
            })
        );
        assert_eq!(
            serde_json::to_value(IpcEvent::Disconnected).unwrap(),
            serde_json::json!({"kind":"disconnected"})
        );
    }

    #[test]
    fn core_events_survive_and_retired_companion_messages_are_ignored() {
        let server = IpcServer::start().unwrap();
        let endpoint = server.endpoint().clone();
        let (mut client, _) = connect(format!("ws://127.0.0.1:{}/aurora", endpoint.port)).unwrap();
        client
            .send(Message::Text(
                serde_json::json!({
                    "kind": "hello", "nonce": endpoint.nonce, "loader": "fabric",
                    "minecraftVersion": "1.20.1", "coreVersion": "1.1.5", "protocol": 1
                })
                .to_string(),
            ))
            .unwrap();
        assert!(client
            .read()
            .unwrap()
            .to_text()
            .unwrap()
            .contains("accepted"));
        for kind in [
            "overlay",
            "assistantRequest",
            "assistantListen",
            "telemetry",
            "assistantResponse",
            "assistantCaption",
            "assistantTranscript",
            "toggleAssistant",
        ] {
            client
                .send(Message::Text(
                    serde_json::json!({
                        "kind": kind, "requestId": "legacy-1", "message": "must be ignored"
                    })
                    .to_string(),
                ))
                .unwrap();
        }
        client
            .send(Message::Text(
                serde_json::json!({
                    "kind": "skinChanged", "profile": "test"
                })
                .to_string(),
            ))
            .unwrap();
        let started = Instant::now();
        loop {
            match server.try_recv() {
                Ok(IpcEvent::Connected { .. }) => {}
                Ok(IpcEvent::ModuleEvent { name, payload }) => {
                    assert_eq!(name, "skinChanged");
                    assert_eq!(payload["profile"], "test");
                    break;
                }
                Ok(event) => panic!("unexpected event: {event:?}"),
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
            assert!(started.elapsed() < Duration::from_secs(2));
        }
        assert!(server.is_connected());
        server
            .send_json(&serde_json::json!({"kind": "profileUpdated"}))
            .unwrap();
        assert!(client
            .read()
            .unwrap()
            .to_text()
            .unwrap()
            .contains("profileUpdated"));
    }

    #[test]
    fn sends_public_session_without_credentials() {
        let profile = IpcSessionProfile::offline(Uuid::nil(), "AuroraPlayer");
        let server = IpcServer::start_with_session(Some(profile)).expect("IPC deve iniciar");
        let endpoint = server.endpoint().clone();
        let (mut client, _) = connect(format!("ws://127.0.0.1:{}/aurora", endpoint.port))
            .expect("Core deve concluir o handshake");
        client
            .send(Message::Text(
                serde_json::json!({
                    "kind": "hello",
                    "nonce": endpoint.nonce,
                    "loader": "forge",
                    "minecraftVersion": "1.21.1",
                    "coreVersion": "1.0.0",
                    "protocol": 1,
                })
                .to_string(),
            ))
            .unwrap();
        assert!(client
            .read()
            .unwrap()
            .to_text()
            .unwrap()
            .contains("accepted"));
        let session = client.read().unwrap().to_text().unwrap().to_owned();
        assert!(session.contains("AuroraPlayer"));
        assert!(session.contains("offline"));
        assert!(!session.to_ascii_lowercase().contains("token"));
    }

    #[test]
    fn rejects_invalid_nonce_or_protocol() {
        let server = IpcServer::start().expect("IPC deve iniciar");
        let endpoint = server.endpoint().clone();
        let (mut client, _) = connect(format!("ws://127.0.0.1:{}/aurora", endpoint.port))
            .expect("handshake WebSocket local");
        client
            .send(Message::Text(
                serde_json::json!({
                    "kind": "hello",
                    "nonce": "00000000000000000000000000000000",
                    "loader": "fabric",
                    "minecraftVersion": "1.20.1",
                    "coreVersion": "1.0.0",
                    "protocol": 2,
                })
                .to_string(),
            ))
            .unwrap();

        let _ = client.read();
        assert!(!server.is_connected());
        assert!(!matches!(server.try_recv(), Ok(IpcEvent::Connected { .. })));
    }

    #[test]
    fn validates_extensible_event_names_and_sensitive_payloads() {
        assert!(valid_event_name("skinChanged"));
        assert!(valid_event_name("module.event-1"));
        assert!(!valid_event_name("1invalid"));
        assert!(!valid_event_name("invalid event"));

        let safe = BTreeMap::from([(
            "profile".to_owned(),
            serde_json::json!({"username": "AuroraPlayer"}),
        )]);
        let sensitive = BTreeMap::from([(
            "profile".to_owned(),
            serde_json::json!({"accessToken": "must-not-cross-ipc"}),
        )]);
        assert!(!contains_sensitive_key(&safe));
        assert!(contains_sensitive_key(&sensitive));
    }
}
