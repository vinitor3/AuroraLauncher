//! Two actual, owned runner processes on loopback. No external endpoint or Java.
use aurora_connect_lab::generate_key;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

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
struct Plan {
    path: PathBuf,
    hash: String,
}
impl Plan {
    fn new(bind: SocketAddr, target: SocketAddr, ttl: u64) -> Self {
        let path = std::env::temp_dir().join(format!(
            "aurora-invite-audit-{}.json",
            generate_key().public()
        ));
        let bytes=serde_json::to_vec(&json!({"schemaVersion":1,"bind":bind.to_string(),"durationSeconds":ttl,
            "services":[{"serviceId":"game","protocol":"tcp","endpoint":target.to_string(),"revision":1,"requiredCapability":"reliableStream","maxDatagramBytes":1024,"udpReturnPolicy":{"type":"replyBudget"}}]})).expect("fixture plan serialization");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .expect("new owned plan");
        file.write_all(&bytes).expect("owned plan write");
        Self {
            path,
            hash: Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        }
    }
    fn args(&self, command: &str) -> Vec<String> {
        vec![
            command.into(),
            self.path.to_str().expect("fixture path").into(),
            self.hash.clone(),
        ]
    }
}
impl Drop for Plan {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
struct Echo {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    task: Option<thread::JoinHandle<()>>,
}
impl Echo {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("owned echo listener");
        let address = listener.local_addr().expect("echo address");
        listener.set_nonblocking(true).expect("echo nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = stop.clone();
        let task = thread::spawn(move || {
            while !cancel.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        // Windows can inherit the listener's nonblocking mode.
                        // This fixture uses bounded blocking copy after accept.
                        let _ = socket.set_nonblocking(false);
                        let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
                        let _ = socket.set_write_timeout(Some(Duration::from_secs(5)));
                        let mut reply = socket.try_clone().expect("owned echo clone");
                        let _ = std::io::copy(&mut socket, &mut reply);
                        let _ = reply.shutdown(Shutdown::Write);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            address,
            stop,
            task: Some(task),
        }
    }
}
impl Drop for Echo {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
        assert!(
            TcpListener::bind(self.address).is_ok(),
            "owned echo listener released"
        );
    }
}
struct Pair {
    host: Process,
    guest: Process,
    bind: SocketAddr,
    listener: SocketAddr,
    _plan: Plan,
}
impl Pair {
    fn new(echo: SocketAddr, ttl: u64) -> Self {
        let reservation = UdpSocket::bind("127.0.0.1:0").expect("owned host reservation");
        Self::from_reservation(echo, ttl, reservation, "host", "guest", Duration::ZERO)
    }
    fn from_reservation(
        echo: SocketAddr,
        ttl: u64,
        reservation: UdpSocket,
        host_command: &str,
        guest_command: &str,
        preparation_wait: Duration,
    ) -> Self {
        let bind = reservation.local_addr().expect("host address");
        let plan = Plan::new(bind, echo, ttl);
        let mut host = Process::start(&plan.args(host_command));
        let host_identity = host.event("audit_host_identity");
        let mut guest = Process::start(&[
            guest_command.into(),
            host_identity["peer"]
                .as_str()
                .expect("public host fixture ID")
                .into(),
            "127.0.0.1:0".into(),
        ]);
        let guest_identity = guest.event("audit_guest_identity");
        let preparation_seconds = if host_command.ends_with("-interactive") {
            600
        } else {
            60
        };
        assert!(
            host_identity["preparationTimeoutSeconds"] == preparation_seconds
                && guest_identity["preparationTimeoutSeconds"] == preparation_seconds,
            "both roles expose the fixed preparation profile"
        );
        let waiting = Instant::now();
        if !preparation_wait.is_zero() {
            println!(
                "{}",
                json!({"event":"audit_preparation_wait","seconds":preparation_wait.as_secs(),"processes":2})
            );
        }
        while waiting.elapsed() < preparation_wait {
            thread::sleep(
                Duration::from_secs(1).min(preparation_wait.saturating_sub(waiting.elapsed())),
            );
            assert!(
                host.child.try_wait().expect("owned host state").is_none()
                    && guest.child.try_wait().expect("owned guest state").is_none(),
                "both original processes remain at their first prompt"
            );
        }
        assert!(
            matches!(host.output.try_recv(), Err(mpsc::TryRecvError::Empty))
                && matches!(guest.output.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "no unsolicited event or replacement identity during preparation"
        );
        host.line(
            guest_identity["peer"]
                .as_str()
                .expect("public guest fixture ID"),
        );
        let review = host.event("audit_review");
        assert!(
            review["preparationTimeoutSeconds"] == preparation_seconds
                && review["inputExpiresAtMs"]
                    .as_u64()
                    .expect("host input deadline")
                    <= review["expiresAtMs"].as_u64().expect("host lease deadline"),
            "host approval never extends the invitation deadline"
        );
        drop(reservation);
        host.line(&format!(
            "approve {}",
            review["digest"].as_str().expect("review digest")
        ));
        let invitation = host.event("audit_invitation");
        host.event("audit_host_ready");
        guest.line(
            invitation["invitation"]
                .as_str()
                .expect("fixture invitation"),
        );
        let review = guest.event("audit_review");
        assert!(
            review["preparationTimeoutSeconds"] == preparation_seconds
                && review["inputExpiresAtMs"]
                    .as_u64()
                    .expect("guest input deadline")
                    <= review["expiresAtMs"]
                        .as_u64()
                        .expect("guest lease deadline"),
            "guest approval never extends the invitation deadline"
        );
        assert!(
            review["localTcpBind"] == "127.0.0.1:0",
            "guest reviews its explicit local listener"
        );
        guest.line(&format!(
            "approve {}",
            review["digest"].as_str().expect("review digest")
        ));
        let ready = guest.event("audit_guest_ready");
        let listener: SocketAddr = ready["tcp"]
            .as_str()
            .expect("bridge endpoint")
            .parse()
            .expect("bridge literal");
        assert!(
            listener.ip().is_loopback() && listener.port() != 0,
            "owned bridge stays loopback"
        );
        Self {
            host,
            guest,
            bind,
            listener,
            _plan: plan,
        }
    }
    fn tcp(&self) {
        let mut socket = TcpStream::connect(self.listener).expect("owned bridge dial");
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read deadline");
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .expect("write deadline");
        let mut send = socket.try_clone().expect("duplex clone");
        let writer = thread::spawn(move || {
            let payload = vec![0x45; 1024 * 1024];
            send.write_all(&payload).expect("TCP send");
            send.shutdown(Shutdown::Write).expect("TCP half-close");
        });
        let mut bytes = Vec::new();
        socket.read_to_end(&mut bytes).expect("TCP reply");
        writer.join().expect("TCP sender joined");
        assert!(
            bytes == vec![0x45; 1024 * 1024],
            "exact TCP1MiB each direction; receivedBytes={}",
            bytes.len()
        );
    }
    fn disconnect(&mut self) {
        self.guest.line("disconnect");
        assert!(
            self.guest.event("audit_guest_disconnected")["rebound"] == true,
            "guest rebind"
        );
    }
    fn host_stats(&mut self) -> Value {
        self.host.line("stats");
        self.host.event("audit_host_stats")["metrics"].clone()
    }
    fn stop(&mut self) {
        self.guest.line("stop");
        let event = self.guest.event("audit_guest_stopped");
        assert!(event["cleanup"] == true, "guest cleanup");
        self.guest.exit(0);
        self.host.line("stop");
        assert!(
            self.host.event("audit_host_stopped")["rebound"] == true,
            "host rebind"
        );
        self.host.exit(0);
        assert!(
            TcpListener::bind(self.listener).is_ok() && UdpSocket::bind(self.bind).is_ok(),
            "both processes released own listeners"
        );
    }
}

#[test]
fn same_ram_identity_replay_and_revocation_are_distinct_real_host_replies() {
    let echo = Echo::new();
    let mut pair = Pair::new(echo.address, 90);
    check_replay_revocation(&mut pair);
}
fn check_replay_revocation(pair: &mut Pair) {
    pair.tcp();
    pair.disconnect();
    let baseline = pair.host_stats();
    assert!(
        baseline["denied"] == 0,
        "no preexisting denial in controlled host"
    );
    pair.guest.line("retry-replay");
    let replay = pair.guest.event("audit_retry_result");
    assert!(
        replay["case"] == "replay"
            && replay["code"] == "replayed_request"
            && replay["passed"] == true
            && replay["sameIdentity"] == true,
        "actual replay before revocation"
    );
    let before = pair.host_stats();
    assert!(
        before["destination_tcp_connects"] == 1
            && before["tcp_bytes"] == 2 * 1024 * 1024
            && before["denied"] == 1,
        "only approved TCP transfer"
    );
    pair.host.line("revoke");
    assert!(
        pair.host.event("audit_revoked")["endpointRetained"] == true,
        "host retained after revoke"
    );
    pair.guest.line("retry-revoked");
    let revoked = pair.guest.event("audit_retry_result");
    assert!(
        revoked["case"] == "revoked"
            && revoked["code"] == "expired_or_revoked"
            && revoked["passed"] == true
            && revoked["source"] == "native_guest_error"
            && revoked["requiresHostDenialCounter"] == true,
        "valid invitation refused by live revoked host"
    );
    let after = pair.host_stats();
    assert!(
        after["destination_tcp_connects"] == 1
            && after["tcp_bytes"] == before["tcp_bytes"]
            && after["denied"] == 2,
        "negative trials opened no destinations"
    );
    pair.stop();
}
#[test]
#[ignore = "requires explicit own global IPv6; binds locally before dial; not an internet test"]
fn own_ipv6_two_processes_replay_revocation_cleanup() {
    let reservation = own_ipv6_reservation();
    let echo = Echo::new();
    let mut pair = Pair::from_reservation(
        echo.address,
        90,
        reservation,
        "host-external-ipv6",
        "guest-external-ipv6",
        Duration::ZERO,
    );
    check_replay_revocation(&mut pair);
    drop(pair);
    drop(echo);
    println!(
        "{}",
        json!({"event":"audit_own_ipv6_sample","scope":"same_computer","passed":true,"tcpBytesEachDirection":1048576,"negativeTrials":2,"hostDenied":2,"cleanup":true})
    );
}
fn own_ipv6_reservation() -> UdpSocket {
    let supplied = std::env::var("AURORA_CONNECT_OWN_EXTERNAL_IPV6_TEST_ADDR")
        .expect("explicit own IPv6 required");
    let ip: std::net::Ipv6Addr = supplied
        .parse()
        .map_err(|_| "invalid own IPv6")
        .expect("literal own IPv6");
    assert!(ip.to_string() == supplied, "canonical own IPv6");
    aurora_connect_lab::validate_external_invitation_address(SocketAddr::from((ip, 1)))
        .expect("external IPv6 policy");
    UdpSocket::bind(SocketAddr::from((ip, 0)))
        .map_err(|_| "own IPv6 bind failed")
        .expect("bind before dial")
}
#[test]
#[ignore = "requires explicit own global IPv6 and 61s preparation; binds before dial; not internet"]
fn own_ipv6_interactive_two_original_processes_wait_past_sixty_seconds() {
    let reservation = own_ipv6_reservation();
    let echo = Echo::new();
    let started = Instant::now();
    let mut pair = Pair::from_reservation(
        echo.address,
        90,
        reservation,
        "host-external-ipv6-interactive",
        "guest-external-ipv6-interactive",
        Duration::from_secs(61),
    );
    check_replay_revocation(&mut pair);
    drop(pair);
    drop(echo);
    println!(
        "{}",
        json!({"event":"audit_interactive_preparation_sample","scope":"same_computer","passed":true,
        "elapsedMs":started.elapsed().as_millis(),"preparationWaitSeconds":61,"preparationTimeoutSeconds":600,
        "tcpBytesEachDirection":1048576,"negativeTrials":2,"hostDenied":2,"cleanup":true})
    );
}
#[test]
fn original_ttl_closes_active_connection_and_releases_both_processes() {
    let echo = Echo::new();
    let started = Instant::now();
    let mut pair = Pair::new(echo.address, 10);
    assert!(
        pair.guest.event("audit_guest_disconnected")["rebound"] == true,
        "expiry released guest sockets"
    );
    assert!(
        pair.guest.event("audit_connection_closed")["deadlineReached"] == true,
        "actual transport close at original deadline"
    );
    assert!(
        pair.guest.event("audit_expiration_checked")["source"]
            == "local_verification_after_observed_close",
        "expiry local gate distinguished from remote reply"
    );
    assert!(
        pair.guest.event("audit_guest_stopped")["cleanup"] == true,
        "guest expiry cleanup"
    );
    pair.guest.exit(0);
    let stopped = pair.host.event("audit_host_stopped");
    assert!(
        stopped["reason"] == "lease_expired" && stopped["rebound"] == true,
        "host original deadline and cleanup"
    );
    pair.host.exit(0);
    assert!(
        started.elapsed() >= Duration::from_secs(10),
        "expiry not early"
    );
    assert!(
        TcpListener::bind(pair.listener).is_ok() && UdpSocket::bind(pair.bind).is_ok(),
        "expired listeners released"
    );
}
#[test]
fn malformed_mode_pin_listener_and_input_are_refused_without_network() {
    let pin = generate_key().public().to_string();
    for args in [
        vec!["guest".into(), pin.clone(), "0.0.0.0:0".into()],
        vec!["guest".into(), "BAD".into(), "127.0.0.1:0".into()],
        vec!["guest-unknown".into()],
        vec![
            "guest-external-ipv6-interactive".into(),
            pin.clone(),
            "127.0.0.1:0".into(),
            "601".into(),
        ],
    ] {
        let mut child = Process::start(&args);
        let error = child.event("audit_failed");
        assert!(
            error["code"] == "invalid_configuration" || error["code"] == "invalid_arguments",
            "pre-input gate"
        );
        child.exit(1);
    }
    let mut child = Process::start(&["guest".into(), pin, "127.0.0.1:0".into()]);
    child.event("audit_guest_identity");
    child.line(&"a".repeat(4200));
    assert!(
        child.event("audit_failed")["code"] == "invalid_input",
        "bounded invitation input"
    );
    child.exit(1);
}
#[test]
fn wrong_approval_digest_never_opens_the_reviewed_host_port() {
    let reservation = UdpSocket::bind("127.0.0.1:0").expect("own reservation");
    let bind = reservation.local_addr().expect("own bind");
    let echo = Echo::new();
    let plan = Plan::new(bind, echo.address, 30);
    let mut host = Process::start(&plan.args("host"));
    host.event("audit_host_identity");
    host.line(&generate_key().public().to_string());
    host.event("audit_review");
    host.line(&format!("approve {}", "0".repeat(64)));
    assert!(
        host.event("audit_failed")["code"] == "approval_mismatch",
        "digest approval gate before bind"
    );
    host.exit(1);
    assert!(
        reservation.local_addr().expect("held reservation") == bind,
        "reservation untouched"
    );
}
#[test]
fn eof_cleans_active_guest_and_host_without_changing_identity() {
    let echo = Echo::new();
    let mut pair = Pair::new(echo.address, 30);
    pair.guest.child.stdin.take();
    assert!(
        pair.guest.event("audit_guest_disconnected")["rebound"] == true,
        "EOF guest rebind"
    );
    assert!(
        pair.guest.event("audit_guest_stopped")["reason"] == "input_closed",
        "EOF recorded"
    );
    pair.guest.exit(0);
    pair.host.child.stdin.take();
    assert!(
        pair.host.event("audit_host_stopped")["rebound"] == true,
        "EOF host rebind"
    );
    pair.host.exit(0);
}

#[test]
fn foreign_signer_and_explicit_external_mode_reject_before_connection() {
    let host_key = generate_key();
    let reservation = UdpSocket::bind("127.0.0.1:0").expect("owned no-dial reservation");
    let address = reservation.local_addr().expect("owned address");
    let service = aurora_connect_lab::Service {
        service_id: "game".into(),
        protocol: aurora_connect_lab::Protocol::Tcp,
        endpoint: "127.0.0.1:43001".parse().expect("loopback fixture literal"),
        revision: 1,
        max_datagram_bytes: 1024,
        required_capability: aurora_connect_lab::Capability::ReliableStream,
        udp_return_policy: aurora_connect_lab::UdpReturnPolicy::ReplyBudget,
    };
    for (command, wrong_signer, expected) in [
        ("guest", true, "unauthorized"),
        ("guest-external-ipv6", false, "invalid_configuration"),
        (
            "guest-external-ipv6-interactive",
            false,
            "invalid_configuration",
        ),
    ] {
        let mut guest = Process::start(&[
            command.into(),
            host_key.public().to_string(),
            "127.0.0.1:0".into(),
        ]);
        let identity = guest.event("audit_guest_identity");
        let recipient = identity["peer"]
            .as_str()
            .expect("public fixture recipient")
            .parse()
            .expect("public key format");
        let other = generate_key();
        let signer = if wrong_signer { &other } else { &host_key };
        let invitation = aurora_connect_lab::Invitation::issue(
            signer,
            recipient,
            "negative-audit",
            address,
            std::slice::from_ref(&service),
            aurora_connect_lab::unix_millis() + 30_000,
        )
        .expect("signed negative fixture");
        guest.line(&invitation.encode());
        assert!(
            guest.event("audit_failed")["code"] == expected,
            "signature/mode refused before transport"
        );
        guest.exit(1);
        reservation
            .set_nonblocking(true)
            .expect("owned negative socket");
        let mut bytes = [0; 1];
        assert!(
            matches!(reservation.recv_from(&mut bytes),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock),
            "no UDP dial reached reserved endpoint"
        );
    }
}

#[test]
fn host_command_quota_closes_its_endpoint_and_reports_static_failure() {
    let echo = Echo::new();
    let mut pair = Pair::new(echo.address, 30);
    for _ in 0..32 {
        pair.host.line("stats");
        pair.host.event("audit_host_stats");
    }
    let stopped = pair.host.event("audit_host_stopped");
    assert!(
        stopped["reason"] == "command_limit" && stopped["rebound"] == true,
        "quota cleanup"
    );
    assert!(
        pair.host.event("audit_failed")["code"] == "command_limit",
        "static quota error"
    );
    pair.host.exit(1);
    assert!(
        pair.guest.event("audit_guest_disconnected")["rebound"] == true,
        "remote close cleanup"
    );
    assert!(
        pair.guest.event("audit_connection_closed")["deadlineReached"] == false,
        "quota closure is not expiration evidence"
    );
    pair.guest.line("stop");
    pair.guest.event("audit_guest_stopped");
    pair.guest.exit(0);
    assert!(
        UdpSocket::bind(pair.bind).is_ok(),
        "quota host port released"
    );
}
