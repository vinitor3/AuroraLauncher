//! Real helper processes: pre-transport stop/EOF, original identity and explicit
//! policy consent. The separate emulator runner exercises actual TCP admission.
use aurora_connect_lab as lab;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
const SESSION: &str = "ed35a732-2839-4a18-9740-73ed5ecff11b";
const DIGEST: &str = "4444444444444444444444444444444444444444444444444444444444444444";
struct Process {
    child: Child,
    output: mpsc::Receiver<Value>,
    reader: Option<thread::JoinHandle<()>>,
}
impl Process {
    fn start(role: &str, source: Option<&str>) -> Self {
        let pin = lab::generate_key().public().to_string();
        let mut args = vec![role.to_owned(), pin];
        if let Some(source) = source {
            args.push(source.to_owned());
        }
        args.extend([SESSION.into(), format!("local_{role}")]);
        let mut child = Command::new(env!("CARGO_BIN_EXE_aurora-connect-social-client"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, output) = mpsc::sync_channel(16);
        let reader = thread::spawn(move || {
            let mut input = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                if !matches!(input.by_ref().take(8193).read_until(b'\n',&mut line),Ok(n)if n>0&&n<=8192)
                {
                    break;
                }
                let value: Value =
                    serde_json::from_slice(&line).expect("bounded redacted helper output");
                if send.try_send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            output,
            reader: Some(reader),
        }
    }
    fn line(&mut self, text: &str) {
        let input = self.child.stdin.as_mut().unwrap();
        writeln!(input, "{text}").unwrap();
        input.flush().unwrap();
    }
    fn next(&self) -> Value {
        self.output
            .recv_timeout(Duration::from_secs(5))
            .expect("bounded native helper event")
    }
    fn exit(&mut self, expected: i32) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert_eq!(status.code(), Some(expected));
                break;
            }
            assert!(Instant::now() < deadline, "native helper exit deadline");
            thread::sleep(Duration::from_millis(5));
        }
        self.child.stdin.take();
        self.reader.take().unwrap().join().unwrap();
        assert!(self.output.try_recv().is_err());
    }
    fn stopped(&mut self, role: &str) {
        let event = self.next();
        assert_eq!(event["event"], format!("social_{role}_stopped"));
        assert_eq!(event["cleanup"], true);
        assert_eq!(event["rebound"], true);
        self.exit(0);
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.child.stdin.take();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
fn challenge(identity: &Value, role: &str) -> Value {
    json!({"schemaVersion":1,"purpose":"accountKeyBinding","issuerInstance":"33".repeat(32),"challengeId":"11".repeat(32),
        "accountId":format!("local_{role}"),"operationId":identity["operationId"],"generation":identity["generation"],"role":role,
        "publicKey":identity["publicKey"],"sessionId":SESSION,"sessionDigest":DIGEST,"issuedAtUnixMs":lab::unix_millis(),
        "expiresAtUnixMs":identity["preparedExpiresAtUnixMs"].as_u64().unwrap()-1})
}
fn guest_expect() -> Value {
    json!({"expect":{"hostAccountId":"local_host","hostPublicKey":lab::generate_key().public().to_string(),
        "hostOperationId":"ed35a732-2839-4a18-9740-73ed5ecff11a","hostGeneration":1,"policyId":"11".repeat(32),
        "policyDigest":"22".repeat(32),"sessionDigest":DIGEST}})
}
#[test]
fn social_client_stop_and_eof_before_transport_finish_without_forced_cleanup() {
    let source = TcpListener::bind("127.0.0.1:0").unwrap();
    source.set_nonblocking(true).unwrap();
    let address = source.local_addr().unwrap().to_string();
    for role in ["host", "guest"] {
        for eof in [false, true] {
            let mut child = Process::start(role, (role == "host").then_some(address.as_str()));
            assert_eq!(child.next()["event"], "social_identity");
            if eof {
                child.child.stdin.take();
            } else {
                child.line("stop");
            }
            child.stopped(role);
        }
    }
    assert!(matches!(source.accept(),Err(error)if error.kind()==std::io::ErrorKind::WouldBlock));
}
#[test]
fn social_client_host_stop_at_each_preparation_stage_never_reaches_game() {
    let source = TcpListener::bind("127.0.0.1:0").unwrap();
    source.set_nonblocking(true).unwrap();
    let address = source.local_addr().unwrap().to_string();
    for stage in 0..3 {
        let mut child = Process::start("host", Some(&address));
        let identity = child.next();
        let issued = challenge(&identity, "host");
        child.line(&issued.to_string());
        let proof = child.next();
        let proof = lab::SocialKeyBindingProof::parse(&proof.to_string()).unwrap();
        assert_eq!(
            proof.challenge().public_key(),
            identity["publicKey"].as_str().unwrap()
        );
        if stage >= 1 {
            child.line("public");
            let review = child.next();
            assert_eq!(review["event"], "social_host_review");
            if stage == 2 {
                child.line(&format!("approve {}", review["digest"].as_str().unwrap()));
                let policy = child.next();
                assert_eq!(policy["event"], "social_host_policy");
                lab::SignedSocialHostPolicy::parse(&policy["policy"].to_string())
                    .unwrap()
                    .verify()
                    .unwrap();
            }
        }
        child.line("stop");
        child.stopped("host");
    }
    assert!(matches!(source.accept(),Err(error)if error.kind()==std::io::ErrorKind::WouldBlock));
}
#[test]
fn social_client_wrong_approval_and_changed_account_do_not_create_transport() {
    let source = TcpListener::bind("127.0.0.1:0").unwrap();
    source.set_nonblocking(true).unwrap();
    let address = source.local_addr().unwrap().to_string();
    let mut child = Process::start("host", Some(&address));
    let identity = child.next();
    child.line(&challenge(&identity, "host").to_string());
    child.next();
    child.line("friends");
    assert_eq!(child.next()["event"], "social_host_review");
    child.line("approve wrong");
    assert_eq!(child.next()["code"], "approval_mismatch");
    child.exit(1);
    let mut changed = Process::start("host", Some(&address));
    let identity = changed.next();
    let mut value = challenge(&identity, "host");
    value["accountId"] = json!("other");
    changed.line(&value.to_string());
    assert_eq!(changed.next()["code"], "challenge_context");
    changed.exit(1);
    assert!(matches!(source.accept(),Err(error)if error.kind()==std::io::ErrorKind::WouldBlock));
}
#[test]
fn social_client_guest_freezes_expectation_before_proof_and_can_stop_after_proof() {
    let mut child = Process::start("guest", None);
    let identity = child.next();
    child.line(&guest_expect().to_string());
    child.line(&challenge(&identity, "guest").to_string());
    lab::SocialKeyBindingProof::parse(&child.next().to_string()).unwrap();
    child.line("stop");
    child.stopped("guest");
    let mut changed = Process::start("guest", None);
    let identity = changed.next();
    changed.line(&guest_expect().to_string());
    let mut value = challenge(&identity, "guest");
    value["sessionDigest"] = json!("55".repeat(32));
    changed.line(&value.to_string());
    assert_eq!(changed.next()["code"], "challenge_context");
    changed.exit(1);
}
