//! CLI gates only. Global literals are parsed/rejected or reviewed then denied;
//! these tests never approve an external connection or bind a global endpoint.
use aurora_connect_lab::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

struct OwnedChild {
    child: Child,
    events: mpsc::Receiver<Value>,
    reader: Option<thread::JoinHandle<()>>,
    stderr: Option<thread::JoinHandle<Vec<u8>>>,
}
impl OwnedChild {
    fn start(arguments: &[String]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_aurora-connect-lab"))
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_remove("RUST_LOG")
            .env_remove("DEBUG")
            .env_remove("SSLKEYLOGFILE")
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (send, events) = mpsc::sync_channel(6);
        let reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut bytes = Vec::new();
                if !matches!(reader.by_ref().take(8193).read_until(b'\n',&mut bytes),Ok(n) if n>0&&n<=8192)
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
        let stderr = thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.take(4097).read_to_end(&mut bytes);
            bytes
        });
        Self {
            child,
            events,
            reader: Some(reader),
            stderr: Some(stderr),
        }
    }
    fn event(&self) -> Value {
        self.events
            .recv_timeout(Duration::from_secs(5))
            .expect("bounded CLI event")
    }
    fn write(&mut self, text: &str) {
        self.child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
        self.child.stdin.as_mut().unwrap().flush().unwrap();
    }
    fn finish(&mut self) -> i32 {
        self.child.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "owned CLI process deadline");
            thread::sleep(Duration::from_millis(10));
        };
        self.reader.take().unwrap().join().unwrap();
        assert!(
            self.stderr.take().unwrap().join().unwrap().is_empty(),
            "CLI emits no raw errors"
        );
        status.code().unwrap()
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        self.child.stdin.take();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Some(stderr) = self.stderr.take() {
            let _ = stderr.join();
        }
    }
}
struct Plan {
    path: PathBuf,
    hash: String,
}
impl Plan {
    fn new(bind: &str, seconds: u64) -> Self {
        let path = std::env::temp_dir().join(format!(
            "aurora-external-invite-{}.json",
            generate_key().public()
        ));
        let bytes=serde_json::to_vec(&json!({"schemaVersion":1,"bind":bind,"durationSeconds":seconds,"services":[{"serviceId":"game","protocol":"tcp","endpoint":"127.0.0.1:43001","revision":1,"requiredCapability":"reliableStream","maxDatagramBytes":1024,"udpReturnPolicy":{"type":"replyBudget"}}]})).unwrap();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(&bytes).unwrap();
        drop(file);
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
            self.path.to_str().unwrap().into(),
            self.hash.clone(),
        ]
    }
}
impl Drop for Plan {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}
fn fail_before_identity(arguments: Vec<String>, code: &str) {
    let mut child = OwnedChild::start(&arguments);
    child.child.stdin.take();
    let event = child.event();
    assert!(
        event["event"] == "failed" && event["code"] == code,
        "policy rejects before identity"
    );
    assert_eq!(child.finish(), 1);
}

#[test]
fn new_guest_commands_enforce_listener_and_lifetime_before_identity() {
    let pin = generate_key().public().to_string();
    for command in [
        "guest-invite-external-ipv6",
        "guest-invite-external-ipv6-tcp",
    ] {
        let tcp_only = command.ends_with("-tcp");
        for (listener, ttl) in [
            ("0.0.0.0:0", "30"),
            ("[2003::1]:43001", "30"),
            ("127.0.0.1:0", "601"),
        ] {
            let mut args = strings(&[command, &pin, listener]);
            if !tcp_only {
                args.push("127.0.0.1:0".into());
            }
            args.push(ttl.into());
            fail_before_identity(args, "invalid_configuration");
        }
    }
    fail_before_identity(
        strings(&[
            "guest-invite-external-ipv6",
            &pin,
            "127.0.0.1:0",
            "0.0.0.0:43002",
            "30",
        ]),
        "invalid_configuration",
    );
}

#[test]
fn host_commands_reject_wrong_policy_before_identity_and_require_reviewed_hash() {
    for bind in [
        "127.0.0.1:42000",
        "[::1]:42000",
        "[2001:db8::1]:42000",
        "[2001:1ff::1]:42000",
        "[2002::1]:42000",
        "[3fff::1]:42000",
    ] {
        let plan = Plan::new(bind, 30);
        fail_before_identity(
            plan.args("host-invite-plan-external-ipv6"),
            "invalid_configuration",
        );
    }
    let external = Plan::new("[2003::1]:42000", 30);
    fail_before_identity(external.args("host-invite-plan"), "invalid_configuration");
    let oversized = Plan::new("[2003::1]:42000", 601);
    fail_before_identity(
        oversized.args("host-invite-plan-external-ipv6"),
        "invalid_configuration",
    );
    let mut changed = external.args("host-invite-plan-external-ipv6");
    changed[2] = "0".repeat(64);
    fail_before_identity(changed, "plan_changed_since_review");
    // A valid new-mode plan reaches only the peer prompt; EOF prevents binding.
    let mut child = OwnedChild::start(&external.args("host-invite-plan-external-ipv6"));
    child.child.stdin.take();
    assert!(child.event()["event"] == "invite_host_identity");
    assert!(child.event()["event"] == "failed");
    assert_eq!(child.finish(), 1);
}

#[test]
fn default_guest_rejects_external_invitation_and_new_guest_requires_exact_approval() {
    for command in ["guest-invite-tcp", "guest-invite-external-ipv6-tcp"] {
        let host = generate_key();
        let pin = host.public().to_string();
        let mut child = OwnedChild::start(&strings(&[command, &pin, "127.0.0.1:0", "30"]));
        let identity = child.event();
        assert!(identity["event"] == "guest_identity");
        let guest: EndpointId = identity["peer"].as_str().unwrap().parse().unwrap();
        let service = Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: "127.0.0.1:43001".parse().unwrap(),
            revision: 1,
            max_datagram_bytes: 1024,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        };
        let invitation = Invitation::issue_external_ipv6(
            &host,
            guest,
            "cli-external",
            "[2003::1]:42000".parse().unwrap(),
            &[service],
            unix_millis() + 30_000,
        )
        .unwrap();
        child.write(&(invitation.encode() + "\n"));
        if command == "guest-invite-tcp" {
            let failure = child.event();
            assert!(failure["event"] == "failed" && failure["code"] == "invalid_configuration");
        } else {
            assert!(child.event()["event"] == "invitation_review");
            child.write(&("approve ".to_string() + &"0".repeat(64) + "\n"));
            let failure = child.event();
            assert!(failure["event"] == "failed" && failure["code"] == "unauthorized");
        }
        assert_eq!(child.finish(), 1);
    }
}
