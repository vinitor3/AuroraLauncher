//! Own diagnostic aliases and legacy output regression; ephemeral loopback only.
use aurora_connect_lab::*;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, UdpSocket},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tokio::net::TcpListener;
struct Process {
    child: Child,
    output: mpsc::Receiver<Value>,
    reader: Option<thread::JoinHandle<()>>,
    errors: Option<thread::JoinHandle<Vec<u8>>>,
}
impl Process {
    fn start(args: &[String]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_aurora-connect-invite-audit"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_remove("RUST_LOG")
            .env_remove("DEBUG")
            .env_remove("SSLKEYLOGFILE")
            .spawn()
            .expect("owned audit process");
        let stdout = child.stdout.take().expect("owned stdout");
        let stderr = child.stderr.take().expect("owned stderr");
        let (send, output) = mpsc::sync_channel(64);
        let reader = thread::spawn(move || {
            let mut input = BufReader::new(stdout);
            loop {
                let mut bytes = Vec::new();
                if !matches!(input.by_ref().take(16385).read_until(b'\n',&mut bytes),Ok(n) if n>0 && n<=16384)
                {
                    break;
                }
                if !bytes.ends_with(b"\n") {
                    break;
                }
                let Ok(value) = serde_json::from_slice(&bytes) else {
                    break;
                };
                if send.try_send(value).is_err() {
                    break;
                }
            }
        });
        let errors = thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.take(4097).read_to_end(&mut bytes);
            bytes
        });
        Self {
            child,
            output,
            reader: Some(reader),
            errors: Some(errors),
        }
    }
    fn line(&mut self, line: &str) {
        let input = self.child.stdin.as_mut().expect("owned stdin");
        input.write_all(line.as_bytes()).expect("stdin write");
        input.write_all(b"\n").expect("stdin newline");
        input.flush().expect("stdin flush");
    }
    fn event(&self, name: &str) -> Value {
        let event = self
            .output
            .recv_timeout(Duration::from_secs(15))
            .expect("bounded redacted event deadline");
        assert!(
            event["event"] == name,
            "expected fixed event kind; private event body omitted"
        );
        event
    }
    fn exit(&mut self, expected: i32) {
        let deadline = Instant::now() + Duration::from_secs(8);
        let code = loop {
            if let Some(status) = self.child.try_wait().expect("owned wait") {
                break status.code();
            }
            assert!(Instant::now() < deadline, "owned exit deadline");
            thread::sleep(Duration::from_millis(10));
        };
        assert!(code == Some(expected), "exact owned exit code");
        self.child.stdin.take();
        self.reader
            .take()
            .expect("stdout reader")
            .join()
            .expect("stdout reader finished");
        let errors = self
            .errors
            .take()
            .expect("stderr reader")
            .join()
            .expect("stderr reader finished");
        assert!(errors.is_empty(), "no raw stderr diagnostics");
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.child.stdin.take();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Some(reader) = self.errors.take() {
            let _ = reader.join();
        }
    }
}

struct Fixture {
    guest: Process,
    invitation: Invitation,
    host_key: SecretKey,
    service: Service,
    canary: TcpListener,
    reservation: Option<UdpSocket>,
}
impl Fixture {
    async fn new(command: &str) -> Self {
        let reservation = UdpSocket::bind("127.0.0.1:0").unwrap();
        let canary = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service = Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: canary.local_addr().unwrap(),
            revision: 1,
            max_datagram_bytes: 1024,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        };
        let host_key = generate_key();
        let guest = Process::start(&[
            command.into(),
            host_key.public().to_string(),
            "127.0.0.1:0".into(),
        ]);
        let identity = guest.event("audit_guest_identity");
        assert_eq!(identity["preparationTimeoutSeconds"], 60);
        let peer = identity["peer"].as_str().unwrap().parse().unwrap();
        let invitation = Invitation::issue(
            &host_key,
            peer,
            "diagnostic-cli",
            reservation.local_addr().unwrap(),
            std::slice::from_ref(&service),
            unix_millis() + 60000,
        )
        .unwrap();
        Self {
            guest,
            invitation,
            host_key,
            service,
            canary,
            reservation: Some(reservation),
        }
    }
    async fn host(&mut self) -> Host {
        self.reservation.take();
        Host::start_invited(
            HostConfig {
                secret_key: self.host_key.clone(),
                bind_addr: self.invitation.direct_addr(),
                session_id: self.invitation.session_id().into(),
                services: vec![self.service.clone()],
                leases: vec![self.invitation.lease()],
                limits: Limits::default(),
            },
            &self.invitation,
        )
        .await
        .unwrap()
    }
    fn approve(&mut self) {
        self.guest.line("");
        self.guest.line(&self.invitation.encode());
        let review = self.guest.event("audit_review");
        assert!(review["digest"] == self.invitation.digest());
        assert!(self.guest.output.try_recv().is_err());
        self.guest.line("");
        self.guest
            .line(&format!("approve {}", self.invitation.digest()));
    }
    fn observation(&self, phase: &str, code: &str) -> Value {
        let value = self.guest.event("audit_guest_connect_observation");
        assert_eq!(value.as_object().unwrap().len(), 3);
        assert_eq!(value["code"], code);
        let observed = &value["observation"];
        assert_eq!(observed.as_object().unwrap().len(), 3);
        assert_eq!(observed["schemaVersion"], 1);
        assert_eq!(observed["phase"], phase);
        assert!(observed["elapsedMs"].is_u64());
        value
    }
    async fn no_destination(&self, host: &Host) {
        assert_eq!(host.metrics().destination_tcp_connects, 0);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), self.canary.accept())
                .await
                .is_err()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diagnostic_alias_emits_once_per_dial_and_keeps_replay_and_cleanup() {
    let mut f = Fixture::new("guest-observe").await;
    let host = f.host().await;
    f.approve();
    f.observation("admitted", "ok");
    let ready = f.guest.event("audit_guest_ready");
    let local: SocketAddr = ready["tcp"].as_str().unwrap().parse().unwrap();
    f.guest.line("disconnect");
    assert_eq!(f.guest.event("audit_guest_disconnected")["rebound"], true);
    f.guest.line("retry-replay");
    f.observation("hello_ack", "replayed_request");
    assert_eq!(f.guest.event("audit_retry_result")["passed"], true);
    f.guest.line("stop");
    assert_eq!(f.guest.event("audit_guest_stopped")["cleanup"], true);
    f.guest.exit(0);
    assert!(f.guest.output.try_recv().is_err());
    f.no_destination(&host).await;
    host.shutdown().await;
    drop(host);
    drop(UdpSocket::bind(f.invitation.direct_addr()).unwrap());
    drop(std::net::TcpListener::bind(local).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn original_alias_never_emits_diagnostic_event_including_retry() {
    let mut f = Fixture::new("guest").await;
    let host = f.host().await;
    f.approve();
    f.guest.event("audit_guest_ready");
    f.guest.line("disconnect");
    assert_eq!(f.guest.event("audit_guest_disconnected")["rebound"], true);
    f.guest.line("retry-replay");
    assert_eq!(f.guest.event("audit_retry_result")["passed"], true);
    f.guest.line("stop");
    f.guest.event("audit_guest_stopped");
    f.guest.exit(0);
    assert!(f.guest.output.try_recv().is_err());
    f.no_destination(&host).await;
    host.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diagnostic_timeout_preserves_ten_seconds_error_and_failure_cleanup() {
    let mut f = Fixture::new("guest-observe").await;
    f.approve();
    let observation = f.observation("endpoint_connect", "timed_out");
    assert!(observation["observation"]["elapsedMs"].as_u64().unwrap() >= 10000);
    assert_eq!(f.guest.event("audit_failed")["code"], "timed_out");
    f.guest.exit(1);
    assert!(f.guest.output.try_recv().is_err());
    let sink = f.reservation.as_ref().unwrap();
    sink.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let (_, source) = sink.recv_from(&mut [0; 2048]).unwrap();
    drop(UdpSocket::bind(source).unwrap());
    assert!(
        tokio::time::timeout(Duration::from_millis(20), f.canary.accept())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_during_dial_waits_for_result_without_false_readiness() {
    let mut f = Fixture::new("guest-observe").await;
    f.approve();
    // Wait for actual native UDP activity before cancelling input.
    let sink = f.reservation.as_ref().unwrap();
    sink.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let (_, source) = sink.recv_from(&mut [0; 2048]).unwrap();
    f.guest.line("stop");
    f.observation("endpoint_connect", "timed_out");
    assert_eq!(
        f.guest.event("audit_guest_stopped")["reason"],
        "cancelled_before_ready"
    );
    f.guest.exit(0);
    assert!(f.guest.output.try_recv().is_err());
    drop(UdpSocket::bind(source).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diagnostic_external_alias_keeps_explicit_mode_and_interactive_preparation() {
    let mut guest = Process::start(&[
        "guest-external-ipv6-interactive-observe".into(),
        generate_key().public().to_string(),
        "127.0.0.1:0".into(),
    ]);
    assert_eq!(
        guest.event("audit_guest_identity")["preparationTimeoutSeconds"],
        600
    );
    guest.child.stdin.take();
    assert_eq!(guest.event("audit_failed")["code"], "input_closed");
    guest.exit(1);
    assert!(guest.output.try_recv().is_err());
}
