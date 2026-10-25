//! Actual bounded proof processes. No transport, network, or configuration.
use aurora_connect_lab as lab;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
struct Process {
    child: Child,
    events: mpsc::Receiver<Value>,
    reader: Option<thread::JoinHandle<()>>,
}
impl Process {
    fn start(args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_aurora-connect-key-proof"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("owned key proof process");
        let stdout = child.stdout.take().unwrap();
        let (send, events) = mpsc::sync_channel(4);
        let reader = thread::spawn(move || {
            let mut input = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                if !matches!(input.by_ref().take(4097).read_until(b'\n', &mut line), Ok(n) if n>0 && n<=4096)
                {
                    break;
                }
                let value: Value = serde_json::from_slice(&line).expect("redacted JSON output");
                if send.try_send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            events,
            reader: Some(reader),
        }
    }
    fn next(&self) -> Value {
        self.events
            .recv_timeout(Duration::from_secs(5))
            .expect("bounded proof event")
    }
    fn line(&mut self, value: &str) {
        let input = self.child.stdin.as_mut().unwrap();
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
    }
    fn exit(&mut self, expected: i32) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert_eq!(status.code(), Some(expected));
                break;
            }
            assert!(Instant::now() < deadline, "bounded owned process exit");
            thread::sleep(Duration::from_millis(5));
        }
        self.child.stdin.take();
        self.reader.take().unwrap().join().unwrap();
        assert!(
            self.events.try_recv().is_err(),
            "no second proof or key event"
        );
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
#[test]
fn social_key_cli_real_key_one_typed_proof_then_exit() {
    let mut child = Process::start(&[]);
    let identity = child.next();
    assert_eq!(identity["event"], "key_proof_identity");
    let now = lab::unix_millis();
    let challenge = json!({"schemaVersion":1,"purpose":"accountKeyBinding","issuerInstance":"11".repeat(32),
        "challengeId":"22".repeat(32),"accountId":"TestAccount",
        "operationId":"8b414333-65f8-4524-a01a-d609a5168463","generation":1,"role":"guest",
        "publicKey":identity["publicKey"],"sessionId":"a9e1bccc-3768-43bf-ab1c-36fc48e78e5a",
        "sessionDigest":"33".repeat(32),"issuedAtUnixMs":now,"expiresAtUnixMs":now+30_000});
    child.line(&challenge.to_string());
    let proof = child.next();
    assert_eq!(proof["challenge"], challenge);
    lab::SocialKeyBindingProof::parse(&proof.to_string())
        .unwrap()
        .verify()
        .unwrap();
    child.exit(0);
}
#[test]
fn social_key_cli_arguments_eof_and_oversize_are_bounded() {
    let mut args = Process::start(&["not-an-input-channel"]);
    assert_eq!(args.next()["code"], "key_proof_arguments_rejected");
    args.exit(1);
    let mut eof = Process::start(&[]);
    assert_eq!(eof.next()["event"], "key_proof_identity");
    eof.child.stdin.take();
    assert_eq!(eof.next()["code"], "key_proof_invalid_challenge");
    eof.exit(1);
    let mut large = Process::start(&[]);
    assert_eq!(large.next()["event"], "key_proof_identity");
    large.line(&" ".repeat(lab::MAX_SOCIAL_KEY_CHALLENGE_BYTES + 3));
    assert_eq!(large.next()["code"], "key_proof_invalid_challenge");
    large.exit(1);
}
