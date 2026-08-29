//! Canal IPC local entre o Launcher e o Aurora Companion.
//!
//! O servidor só escuta em `127.0.0.1` e cada execução recebe um nonce novo.
//! Esse nonce identifica o processo iniciado pelo Aurora; não é uma credencial
//! de conta e nunca deve ser persistido ou registrado em logs.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tungstenite::{accept, Message};
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

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum IpcEvent {
    Connected {
        loader: String,
        minecraft_version: String,
    },
    /// Pedido emitido pelo Companion quando o atalho é pressionado no jogo.
    OverlayRequested,
    AssistantRequest {
        request_id: String,
        message: String,
        screenshot_base64: Option<String>,
    },
    AssistantListenRequested {
        request_id: String,
    },
    Telemetry {
        fps: f32,
        mspt: f32,
        used_memory_mb: u32,
        dimension: Option<String>,
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
    shutdown: &AtomicBool,
    connected: &AtomicBool,
) {
    let mut socket = match accept(stream) {
        Ok(socket) => socket,
        Err(error) => {
            eprintln!("[Aurora IPC] handshake WebSocket recusado: {error}");
            return;
        }
    };
    let _ = socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(75)));
    let mut authenticated = false;

    while !shutdown.load(Ordering::Acquire) {
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
        let text = match message.to_text() {
            Ok(text) => text,
            Err(_) => continue,
        };
        let parsed = match serde_json::from_str::<WireMessage>(text) {
            Ok(message) => message,
            Err(_) => continue,
        };

        if !authenticated {
            if parsed.kind != "hello" || parsed.nonce.as_deref() != Some(expected_nonce.as_str()) {
                eprintln!("[Aurora IPC] autenticação local recusada");
                let _ = socket.close(None);
                return;
            }
            authenticated = true;
            connected.store(true, Ordering::Release);
            let _ = socket.send(Message::Text(r#"{"kind":"accepted"}"#.into()));
            let _ = sender.try_send(IpcEvent::Connected {
                loader: parsed.loader.unwrap_or_else(|| "unknown".into()),
                minecraft_version: parsed.minecraft_version.unwrap_or_else(|| "unknown".into()),
            });
            continue;
        }

        if parsed.kind == "overlay" {
            let _ = sender.try_send(IpcEvent::OverlayRequested);
        } else if parsed.kind == "assistantListen" {
            let Some(request_id) = valid_request_id(parsed.request_id) else {
                continue;
            };
            let _ = sender.try_send(IpcEvent::AssistantListenRequested { request_id });
        } else if parsed.kind == "assistantRequest" {
            let Some(request_id) = valid_request_id(parsed.request_id) else {
                continue;
            };
            let Some(message) = parsed
                .message
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty() && value.chars().count() <= 2_000)
            else {
                continue;
            };
            let screenshot_base64 = parsed
                .screenshot_base64
                .filter(|value| value.len() <= 2_500_000);
            let _ = sender.try_send(IpcEvent::AssistantRequest {
                request_id,
                message,
                screenshot_base64,
            });
        } else if parsed.kind == "telemetry" {
            let _ = sender.try_send(IpcEvent::Telemetry {
                fps: parsed.fps.unwrap_or_default().clamp(0.0, 10_000.0),
                mspt: parsed.mspt.unwrap_or_default().clamp(0.0, 10_000.0),
                used_memory_mb: parsed.used_memory_mb.unwrap_or_default(),
                dimension: parsed.dimension,
            });
        }
    }

    if authenticated {
        connected.store(false, Ordering::Release);
        let _ = sender.try_send(IpcEvent::Disconnected);
    }
}

fn valid_request_id(value: Option<String>) -> Option<String> {
    value.filter(|value| {
        !value.is_empty()
            && value.len() <= 64
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireMessage {
    kind: String,
    nonce: Option<String>,
    loader: Option<String>,
    minecraft_version: Option<String>,
    fps: Option<f32>,
    mspt: Option<f32>,
    used_memory_mb: Option<u32>,
    dimension: Option<String>,
    request_id: Option<String>,
    message: Option<String>,
    screenshot_base64: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tungstenite::connect;

    #[test]
    fn endpoint_is_loopback_port_with_fresh_nonce() {
        let server = IpcServer::start().expect("IPC deve iniciar");
        assert_ne!(server.endpoint().port, 0);
        assert_eq!(server.endpoint().nonce.len(), 32);
    }

    #[test]
    fn authenticates_and_exchanges_assistant_messages_both_ways() {
        let server = IpcServer::start().expect("IPC deve iniciar");
        let endpoint = server.endpoint().clone();
        let (mut client, _) = connect(format!("ws://127.0.0.1:{}/aurora", endpoint.port))
            .expect("Companion deve concluir o handshake");
        client
            .send(Message::Text(
                serde_json::json!({
                    "kind": "hello",
                    "nonce": endpoint.nonce,
                    "loader": "fabric",
                    "minecraftVersion": "1.20.1",
                })
                .to_string(),
            ))
            .expect("hello deve ser enviado");
        assert_eq!(
            client
                .read()
                .expect("accepted deve chegar")
                .to_text()
                .unwrap(),
            r#"{"kind":"accepted"}"#
        );

        client
            .send(Message::Text(
                r#"{"kind":"assistantRequest","requestId":"test-1","message":"Olá"}"#.into(),
            ))
            .unwrap();
        let started = Instant::now();
        let event = loop {
            if let Ok(event) = server.try_recv() {
                if matches!(event, IpcEvent::AssistantRequest { .. }) {
                    break event;
                }
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(10));
        };
        assert!(matches!(
            event,
            IpcEvent::AssistantRequest { request_id, message, .. }
                if request_id == "test-1" && message == "Olá"
        ));
        client
            .send(Message::Text(
                r#"{"kind":"assistantListen","requestId":"voice-1"}"#.into(),
            ))
            .unwrap();
        let started = Instant::now();
        loop {
            if matches!(
                server.try_recv(),
                Ok(IpcEvent::AssistantListenRequested { request_id }) if request_id == "voice-1"
            ) {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(10));
        }
        assert!(server.is_connected());
        server
            .send_json(&serde_json::json!({
                "kind": "assistantTranscript",
                "requestId": "voice-1",
                "text": "Como faço uma espada?"
            }))
            .unwrap();
        assert!(client
            .read()
            .expect("transcrição deve chegar")
            .to_text()
            .unwrap()
            .contains("assistantTranscript"));
    }
}
