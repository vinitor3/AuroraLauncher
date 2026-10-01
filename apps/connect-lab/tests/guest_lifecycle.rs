//! Real CLI/QUIC lifecycle regressions with synthetic loopback destinations.
//! No Minecraft, Java, external network or prepared runtime is involved.
use aurora_connect_lab::*;
use std::{collections::BTreeSet, net::SocketAddr, time::Duration};
use tokio::{
    net::{TcpListener, UdpSocket},
    time::{sleep, timeout},
};

// Own only this spawned process. Keep stdin open throughout remote-close
// assertions: EOF must not accidentally provide the lifecycle fix under test.
struct CliChild {
    child: std::process::Child,
    events: std::sync::mpsc::Receiver<serde_json::Value>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl CliChild {
    fn start(args: &[String]) -> Self {
        use std::io::{BufRead, Read};
        use std::process::{Command, Stdio};
        let mut command = Command::new(env!("CARGO_BIN_EXE_aurora-connect-lab"));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, events) = std::sync::mpsc::sync_channel(8);
        let reader = std::thread::spawn(move || {
            let mut input = std::io::BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let Ok(n) = input.by_ref().take(16385).read_until(b'\n', &mut line) else {
                    break;
                };
                if n == 0 || n > 16384 || !line.ends_with(b"\n") {
                    break;
                }
                let Ok(value) = serde_json::from_slice(&line) else {
                    break;
                };
                if tx.try_send(value).is_err() {
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
    fn send(&mut self, line: &str) {
        use std::io::Write;
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
    }
    async fn event(&self, expected: &str) -> serde_json::Value {
        timeout(Duration::from_secs(15), async {
            loop {
                match self.events.try_recv() {
                    Ok(value) => {
                        assert!(value["event"] == expected, "unexpected CLI event");
                        return value;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        sleep(Duration::from_millis(5)).await
                    }
                    Err(_) => panic!("CLI event stream closed"),
                }
            }
        })
        .await
        .expect("CLI event deadline")
    }
    async fn exited(&mut self, expected_code: i32) {
        timeout(Duration::from_secs(5), async {
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    assert_eq!(status.code(), Some(expected_code));
                    return;
                }
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("CLI exit deadline");
    }
}
impl Drop for CliChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

struct Session {
    cli: CliChild,
    host: Host,
    destination: TcpListener,
    udp_destination: Option<UdpSocket>,
    bridge: SocketAddr,
    udp_bridge: Option<SocketAddr>,
}
async fn session(idle: Duration, seconds: u64) -> Session {
    session_with_udp(idle, seconds, false).await
}
async fn session_with_udp(idle: Duration, seconds: u64, with_udp: bool) -> Session {
    let destination = TcpListener::bind(local()).await.unwrap();
    let udp_destination = if with_udp {
        Some(UdpSocket::bind(local()).await.unwrap())
    } else {
        None
    };
    let key = generate_key();
    let reservation = std::net::UdpSocket::bind(local()).unwrap();
    let bind = reservation.local_addr().unwrap();
    drop(reservation);
    let mut args = vec![
        if with_udp { "guest" } else { "guest-tcp" }.into(),
        key.public().to_string(),
        bind.to_string(),
        "cli-lifecycle".into(),
        "127.0.0.1:0".into(),
    ];
    if with_udp {
        args.push("127.0.0.1:0".into());
    }
    args.push(seconds.to_string());
    let mut cli = CliChild::start(&args);
    let identity = cli.event("guest_identity").await;
    let mut services = vec![Service {
        service_id: "game".into(),
        protocol: Protocol::Tcp,
        endpoint: destination.local_addr().unwrap(),
        revision: 1,
        max_datagram_bytes: 1400,
        required_capability: Capability::ReliableStream,
        udp_return_policy: UdpReturnPolicy::ReplyBudget,
    }];
    let mut permitted = BTreeSet::from(["game".into()]);
    if let Some(socket) = &udp_destination {
        services.push(Service {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            endpoint: socket.local_addr().unwrap(),
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::Datagram,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        });
        permitted.insert("aux".into());
    }
    let host = Host::start(HostConfig {
        secret_key: key,
        bind_addr: bind,
        session_id: "cli-lifecycle".into(),
        services,
        leases: vec![Lease {
            lease_id: "manual-lab".into(),
            peer: identity["peer"].as_str().unwrap().parse().unwrap(),
            expires_at_unix_ms: unix_millis() + 30_000,
            services: permitted,
        }],
        limits: Limits {
            idle_timeout: idle,
            ..Limits::default()
        },
    })
    .await
    .unwrap();
    cli.send("ready");
    let ready = cli.event("guest_ready").await;
    let udp_bridge = if with_udp {
        Some(ready["udp"].as_str().unwrap().parse().unwrap())
    } else {
        assert_eq!(ready["mode"], "tcp");
        None
    };
    let bridge = ready["tcp"].as_str().unwrap().parse().unwrap();
    Session {
        cli,
        host,
        destination,
        udp_destination,
        bridge,
        udp_bridge,
    }
}

async fn cleanup_checks(session: &Session) {
    assert_eq!(session.host.metrics().destination_tcp_connects, 0);
    // UDP bridge initialization eagerly opens one authorized association;
    // it must not send any application datagram to the destination.
    assert_eq!(
        session.host.metrics().udp_associations_opened,
        u64::from(session.udp_destination.is_some())
    );
    assert_eq!(session.host.metrics().udp_forwarded, 0);
    assert_eq!(session.host.metrics().udp_returned, 0);
    assert!(
        timeout(Duration::from_millis(50), session.destination.accept())
            .await
            .is_err(),
        "no application destination may be opened by lifecycle observation"
    );
    let rebound = TcpListener::bind(session.bridge)
        .await
        .expect("guest listener must be released");
    drop(rebound);
    if let Some(socket) = &session.udp_destination {
        let mut buffer = [0; 1];
        assert!(
            timeout(Duration::from_millis(50), socket.recv_from(&mut buffer))
                .await
                .is_err(),
            "UDP initialization must not deliver application data"
        );
        let rebound = UdpSocket::bind(session.udp_bridge.unwrap())
            .await
            .expect("guest UDP listener must be released");
        drop(rebound);
    }
    session.host.shutdown().await;
}

async fn unexpected_close(session: &mut Session) {
    timeout(Duration::from_secs(5), async {
        session.cli.event("guest_stopped").await;
        let failure = session.cli.event("failed").await;
        assert_eq!(failure["code"], "closed");
        assert_eq!(failure.as_object().unwrap().len(), 3);
        assert_eq!(failure["hint"], "check arguments, authorization, local ports, capability and deadline; no NAT diagnosis inferred");
        session.cli.exited(1).await;
    }).await.expect("remote closure must end the CLI before its thirty-second deadline");
    assert!(
        session.cli.child.stdin.is_some(),
        "stdin must stay open until after exit"
    );
    cleanup_checks(session).await;
}

#[tokio::test]
async fn manual_tcp_cli_observes_real_idle_close_with_stdin_open() {
    // Host idle is one second; lease and CLI both remain valid for thirty.
    let mut session = session(Duration::from_secs(1), 30).await;
    unexpected_close(&mut session).await;
}

#[tokio::test]
async fn manual_tcp_cli_observes_revocation_with_stdin_open() {
    let mut session = session(Duration::from_secs(30), 30).await;
    assert!(session.host.revoke("manual-lab"));
    unexpected_close(&mut session).await;
}

#[tokio::test]
async fn manual_tcp_udp_cli_revocation_releases_both_listeners_without_application_data() {
    let mut session = session_with_udp(Duration::from_secs(30), 30, true).await;
    assert_eq!(session.host.metrics().udp_associations_opened, 1);
    assert!(session.host.revoke("manual-lab"));
    unexpected_close(&mut session).await;
}

#[tokio::test]
async fn manual_tcp_cli_explicit_stop_keeps_success_and_releases_listener() {
    let mut session = session(Duration::from_secs(30), 30).await;
    session.cli.send("stop");
    session.cli.event("guest_stopped").await;
    session.cli.exited(0).await;
    cleanup_checks(&session).await;
}

#[tokio::test]
async fn manual_tcp_cli_own_deadline_keeps_success_distinct_from_remote_close() {
    // The minimum CLI duration is ten seconds; peer idle/lease exceed it.
    let mut session = session(Duration::from_secs(30), 10).await;
    session.cli.event("guest_stopped").await;
    session.cli.exited(0).await;
    assert!(session.cli.child.stdin.is_some());
    cleanup_checks(&session).await;
}
