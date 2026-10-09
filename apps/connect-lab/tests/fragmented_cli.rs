//! Four owned local processes, real invited TCP/fragmented UDP, no Java/mods.
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, UdpSocket},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const EXE: &str = env!("CARGO_BIN_EXE_aurora-connect-lab");
struct Cli {
    child: Child,
    lines: Receiver<Value>,
    reader: Option<JoinHandle<()>>,
}
impl Cli {
    fn start(args: &[&str]) -> Self {
        let mut child = Command::new(EXE)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, lines) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            for _ in 0..32 {
                let mut line = String::new();
                if stdout.by_ref().take(8193).read_line(&mut line).is_err()
                    || line.is_empty()
                    || line.len() > 8192
                {
                    break;
                }
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    break;
                };
                if send.send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            lines,
            reader: Some(reader),
        }
    }
    fn event(&self, name: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            let line = self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("owned_cli_event_deadline");
            if line["event"] == name {
                return line;
            }
            assert_ne!(
                line["event"], "failed",
                "owned_cli_failed_before_expected_event"
            );
        }
    }
    fn send(&mut self, text: &str) {
        let input = self.child.stdin.as_mut().unwrap();
        input.write_all(text.as_bytes()).unwrap();
        input.flush().unwrap();
    }
    fn close_input(&mut self) {
        self.child.stdin.take();
    }
    fn finish(&mut self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(exit) = self.child.try_wait().unwrap() {
                self.child.stdin.take();
                if let Some(reader) = self.reader.take() {
                    reader.join().unwrap();
                }
                return exit.success();
            }
            if Instant::now() >= deadline {
                self.child.kill().unwrap();
                self.child.wait().unwrap();
                panic!("owned_cli_exit_deadline")
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn stop(&mut self, event: &str) {
        self.send("stop\n");
        self.event(event);
        assert!(self.finish());
    }
}
impl Drop for Cli {
    fn drop(&mut self) {
        self.child.stdin.take();
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
struct PlanFile {
    root: PathBuf,
    path: PathBuf,
}
impl PlanFile {
    fn new(value: &Value) -> Self {
        let root = std::env::temp_dir().join(format!(
            "aurora-fragmented-cli-{}",
            aurora_connect_lab::generate_key().public()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("plan.json");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(&serde_json::to_vec(value).unwrap()).unwrap();
        Self { root, path }
    }
    fn text(&self) -> &str {
        self.path.to_str().unwrap()
    }
}
impl Drop for PlanFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_dir(&self.root);
    }
}
fn plan(bind: &str, tcp: &str, udp: &str) -> Value {
    json!({"schemaVersion":1,"bind":bind,"durationSeconds":120,"services":[
        {"serviceId":"game","protocol":"tcp","endpoint":tcp,"revision":1,"maxDatagramBytes":1400,"requiredCapability":"reliableStream"},
        {"serviceId":"aux","protocol":"udp","endpoint":udp,"revision":1,"maxDatagramBytes":4096,"requiredCapability":"fragmentedDatagram"}
    ]})
}
fn failed_before_identity(args: &[&str]) {
    let mut child = Cli::start(args);
    let event = child
        .lines
        .recv_timeout(Duration::from_secs(5))
        .expect("first_event_deadline");
    assert_eq!(
        event["event"], "failed",
        "must_refuse_before_public_identity_or_endpoint"
    );
    assert!(event["code"].is_string());
    assert!(!child.finish());
}

#[test]
fn cli_invited_fragmented_two_origins_large_tcp_oversize_and_cleanup() {
    let mut echo = Cli::start(&["echo-fragmented", "127.0.0.1:0", "127.0.0.1:0", "120"]);
    let echo_ready = echo.event("echo_fragmented_ready");
    assert_eq!(echo_ready["udpApplicationBudgetBytes"], 65536);
    let tcp_echo = echo_ready["tcp"].as_str().unwrap();
    let udp_echo = echo_ready["udp"].as_str().unwrap();
    let reservation = UdpSocket::bind("127.0.0.1:0").unwrap();
    let transport = reservation.local_addr().unwrap();
    let file = PlanFile::new(&plan(&transport.to_string(), tcp_echo, udp_echo));
    let mut preview = Cli::start(&["plan-fragmented", file.text()]);
    let reviewed = preview.event("plan_ready");
    assert_eq!(reviewed["plan"]["profile"], "fragmentedV2");
    assert!(preview.finish());
    let hash = reviewed["plan"]["hash"].as_str().unwrap();
    drop(reservation);
    let mut host = Cli::start(&["host-invite-plan-fragmented", file.text(), hash]);
    let host_identity = host.event("invite_host_identity");
    assert_eq!(host_identity["preparationSeconds"], 600);
    let pin = host_identity["peer"].as_str().unwrap();
    let mut guest = Cli::start(&[
        "guest-invite-fragmented",
        pin,
        "127.0.0.1:0",
        "127.0.0.1:0",
        "120",
    ]);
    let guest_identity = guest.event("guest_identity");
    assert_eq!(guest_identity["preparationSeconds"], 600);
    host.send(&format!("{}\n", guest_identity["peer"].as_str().unwrap()));
    let invitation = host.event("host_invitation");
    host.event("host_ready");
    guest.send(&format!("{}\n", invitation["invitation"].as_str().unwrap()));
    let review = guest.event("invitation_review");
    assert_eq!(review["profile"], "fragmentedV2");
    guest.send(&format!("approve {}\n", review["digest"].as_str().unwrap()));
    let ready = guest.event("guest_ready");
    assert_eq!(ready["approvedOriginalDatagramBytes"], 4096);
    assert_eq!(ready["associationCeilingNegotiated"], false);
    let tcp = ready["tcp"].as_str().unwrap();
    let udp = ready["udp"].as_str().unwrap();
    let mut probe = Cli::start(&["probe-fragmented", tcp, udp]);
    let result = probe.event("fragment_probe_passed");
    assert_eq!(result["udpPayloadBytesEachDirection"], 15262);
    assert_eq!(result["udpScheduledDuplexBytesIncludingOversize"], 46912);
    assert_eq!(result["udpExactReturns"], 8);
    assert_eq!(result["udpOversizeReachedEndpoint"], 0);
    assert_eq!(result["udpOversizeReturnsReachedGuest"], 0);
    assert_eq!(result["tcpConcurrentWithUdp"], true);
    assert_eq!(result["tcpBytesEachDirection"], 1048576);
    assert!(probe.finish());
    guest.send("stop\n");
    let transport_stats = guest.event("fragmented_transport_snapshot");
    assert!(
        transport_stats["datagramFramesTxSinceReady"]
            .as_u64()
            .unwrap()
            >= 18
    );
    assert!(
        transport_stats["datagramFramesRxSinceReady"]
            .as_u64()
            .unwrap()
            >= 18
    );
    guest.event("guest_stopped");
    assert!(guest.finish());
    host.stop("host_stopped");
    echo.stop("echo_fragmented_stopped");
    drop(TcpListener::bind(tcp).unwrap());
    drop(UdpSocket::bind(udp).unwrap());
    drop(TcpListener::bind(tcp_echo).unwrap());
    drop(UdpSocket::bind(udp_echo).unwrap());
    drop(UdpSocket::bind(transport).unwrap());
    println!(
        "{{\"event\":\"fragmented_cli_test_passed\",\"ownedAgents\":2,\"udpSources\":2,\"tcpBytesEachDirection\":1048576,\"udpScheduledDuplexBytes\":46912,\"fragmentFramesEachDirectionMin\":18,\"cleanupRebound\":true}}"
    );
}

#[test]
fn fragmented_commands_reject_bad_arguments_legacy_plan_and_ttl_before_identity() {
    let key = aurora_connect_lab::generate_key().public().to_string();
    for name in [
        "guest-invite-fragmented",
        "guest-invite-fragmented-external-ipv6",
    ] {
        failed_before_identity(&[name, &key, "127.0.0.1:0", "127.0.0.1:0", "601"]);
        failed_before_identity(&[name, &key, "0.0.0.0:1", "127.0.0.1:1", "120"]);
        failed_before_identity(&[name, &key, "127.0.0.1:1", "[::1%1]:1", "120"]);
    }
    failed_before_identity(&["echo-fragmented", "0.0.0.0:1", "127.0.0.1:1", "120"]);
    failed_before_identity(&["probe-fragmented", "0.0.0.0:1", "127.0.0.1:1"]);
    let mut legacy = plan("127.0.0.1:32000", "127.0.0.1:32001", "127.0.0.1:32002");
    legacy["services"][1]["requiredCapability"] = json!("datagram");
    let file = PlanFile::new(&legacy);
    failed_before_identity(&["plan-fragmented", file.text()]);
    let v2 = PlanFile::new(&plan(
        "127.0.0.1:32000",
        "127.0.0.1:32001",
        "127.0.0.1:32002",
    ));
    failed_before_identity(&["plan", v2.text()]);
    let mut preview = Cli::start(&["plan-fragmented", v2.text()]);
    let reviewed = preview.event("plan_ready");
    assert!(preview.finish());
    failed_before_identity(&[
        "host-invite-plan-fragmented-external-ipv6",
        v2.text(),
        reviewed["plan"]["hash"].as_str().unwrap(),
    ]);
}

#[test]
fn fragmented_probe_refuses_plain_udp_path_that_delivers_oversize_to_endpoint() {
    let mut echo = Cli::start(&["echo-fragmented", "127.0.0.1:0", "127.0.0.1:0", "120"]);
    let ready = echo.event("echo_fragmented_ready");
    let tcp = ready["tcp"].as_str().unwrap();
    let udp = ready["udp"].as_str().unwrap();
    let mut probe = Cli::start(&["probe-fragmented", tcp, udp]);
    let refused = probe.event("failed");
    assert_eq!(refused["code"], "fragment_probe_oversize_delivered");
    assert!(!probe.finish());
    echo.stop("echo_fragmented_stopped");
    drop(TcpListener::bind(tcp).unwrap());
    drop(UdpSocket::bind(udp).unwrap());
}

#[test]
fn fragmented_invitation_requires_enter_and_never_opens_guest_on_eof_without_line_ending() {
    use aurora_connect_lab::{
        Capability, Invitation, Protocol, Service, UdpReturnPolicy, unix_millis,
    };
    let signer = aurora_connect_lab::generate_key();
    let key = signer.public().to_string();
    let transport = UdpSocket::bind("127.0.0.1:0").unwrap();
    transport.set_nonblocking(true).unwrap();
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut guest = Cli::start(&[
        "guest-invite-fragmented",
        &key,
        "127.0.0.1:0",
        "127.0.0.1:0",
        "120",
    ]);
    let identity = guest.event("guest_identity");
    assert_eq!(identity["preparationSeconds"], 600);
    let peer = identity["peer"].as_str().unwrap().parse().unwrap();
    let services = [
        Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: target.local_addr().unwrap(),
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        },
        Service {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            endpoint: transport.local_addr().unwrap(),
            revision: 1,
            max_datagram_bytes: 4096,
            required_capability: Capability::FragmentedDatagram,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        },
    ];
    let invitation = Invitation::issue_fragmented(
        &signer,
        peer,
        "no_enter",
        transport.local_addr().unwrap(),
        &services,
        unix_millis() + 60_000,
    )
    .unwrap();
    guest.send(&format!("{}\n", invitation.encode()));
    let review = guest.event("invitation_review");
    guest.send(&format!("approve {}", review["digest"].as_str().unwrap()));
    guest.close_input();
    let refused = guest.event("failed");
    assert_eq!(refused["code"], "invalid_protocol");
    assert!(!guest.finish());
    let mut bytes = [0u8; 2048];
    assert_eq!(
        transport.recv(&mut bytes).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
