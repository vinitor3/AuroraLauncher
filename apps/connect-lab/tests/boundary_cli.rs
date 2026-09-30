//! Real native fixture/probe through the unchanged authenticated Host/Guest.
//! Child guards affect only processes created by this test; no user services.
use aurora_connect_lab::*;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::SocketAddr,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tokio::net::{TcpListener, UdpSocket};

const BINARY: &str = env!("CARGO_BIN_EXE_aurora-connect-lab");
fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

struct OwnedChild {
    child: Child,
    events: mpsc::Receiver<Value>,
    reader: Option<thread::JoinHandle<()>>,
}
impl OwnedChild {
    fn start(args: &[String]) -> Self {
        let mut child = Command::new(BINARY)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, events) = mpsc::sync_channel(4);
        let reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut bytes = Vec::new();
                let read = reader.by_ref().take(4097).read_until(b'\n', &mut bytes);
                if !matches!(read, Ok(n) if n > 0) {
                    break;
                }
                if bytes.len() > 4096 || !bytes.ends_with(b"\n") {
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
        Self {
            child,
            events,
            reader: Some(reader),
        }
    }
    fn event(&self) -> Value {
        self.events
            .recv_timeout(Duration::from_secs(15))
            .expect("bounded native event")
    }
    fn stop(&mut self) {
        self.child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"stop\n")
            .unwrap();
    }
    fn finish(&mut self) -> std::process::ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "owned child must exit normally");
            thread::sleep(Duration::from_millis(10));
        };
        self.reader.take().unwrap().join().unwrap();
        let mut stderr = Vec::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .take(4097)
            .read_to_end(&mut stderr)
            .unwrap();
        assert!(stderr.is_empty(), "native errors are redacted JSON events");
        status
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn invalid_cli_arguments_are_redacted_and_never_emit_ready() {
    let cases = [
        vec!["echo-boundary", "127.0.0.1:0", "0.0.0.0:1", "30"],
        vec!["echo-boundary", "127.0.0.1:0", "127.0.0.1:0", "9"],
        vec![
            "echo-boundary",
            "127.0.0.1:0",
            "127.0.0.1:0",
            "PRIVATE_SECONDS",
        ],
        vec!["probe-boundary", "localhost:1", "127.0.0.1:1", "1264"],
        vec!["probe-boundary", "127.0.0.1:1", "192.0.2.1:1", "1264"],
        vec!["probe-boundary", "127.0.0.1:0", "127.0.0.1:1", "1264"],
        vec!["probe-boundary", "127.0.0.1:1", "127.0.0.1:1", "0"],
        vec!["probe-boundary", "127.0.0.1:1", "127.0.0.1:1", "1401"],
        vec![
            "probe-boundary",
            "127.0.0.1:1",
            "127.0.0.1:1",
            "PRIVATE_CAP",
        ],
        vec![
            "probe-boundary",
            "127.0.0.1:1",
            "127.0.0.1:1",
            "1264",
            "PRIVATE_EXTRA",
        ],
    ];
    for args in cases {
        let args = args.into_iter().map(String::from).collect::<Vec<_>>();
        let mut child = OwnedChild::start(&args);
        let event = child.event();
        assert_eq!(event["event"], "failed");
        assert_eq!(event["code"], "boundary_invalid_arguments");
        assert!(!event.to_string().contains("PRIVATE"));
        assert_eq!(child.finish().code(), Some(1));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_native_probe_checks_two_boundaries_two_origins_tcp_and_normal_cleanup() {
    for cap in [1, 1264] {
        let mut echo = OwnedChild::start(&[
            "echo-boundary".into(),
            "127.0.0.1:0".into(),
            "127.0.0.1:0".into(),
            "30".into(),
        ]);
        let ready = echo.event();
        assert_eq!(ready["event"], "echo_ready");
        let endpoint_tcp: SocketAddr = ready["tcp"].as_str().unwrap().parse().unwrap();
        let endpoint_udp: SocketAddr = ready["udp"].as_str().unwrap().parse().unwrap();
        let guest_key = generate_key();
        let host = Host::start(HostConfig {
            secret_key: generate_key(),
            bind_addr: local(),
            session_id: "native-boundary".into(),
            services: vec![
                Service {
                    service_id: "game".into(),
                    protocol: Protocol::Tcp,
                    endpoint: endpoint_tcp,
                    revision: 1,
                    max_datagram_bytes: cap,
                    required_capability: Capability::ReliableStream,
                    udp_return_policy: UdpReturnPolicy::ReplyBudget,
                },
                Service {
                    service_id: "aux".into(),
                    protocol: Protocol::Udp,
                    endpoint: endpoint_udp,
                    revision: 1,
                    max_datagram_bytes: cap,
                    required_capability: Capability::Datagram,
                    udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                        bytes_per_second: 100_000,
                        datagrams_per_second: 100,
                    },
                },
            ],
            leases: vec![Lease {
                lease_id: "probe".into(),
                peer: guest_key.public(),
                expires_at_unix_ms: unix_millis() + 30_000,
                services: ["game".into(), "aux".into()].into_iter().collect(),
            }],
            limits: Limits::default(),
        })
        .await
        .unwrap();
        let guest = Guest::connect(GuestConfig {
            secret_key: guest_key,
            bind_addr: local(),
            host: host.addr(),
            session_id: "native-boundary".into(),
            lease_id: "probe".into(),
            capabilities: vec![Capability::ReliableStream, Capability::Datagram],
            limits: Limits::default(),
        })
        .await
        .unwrap();
        let tcp = guest.tcp_bridge("game", 1, local()).await.unwrap();
        let udp = guest.udp_bridge("aux", 1, local()).await.unwrap();
        assert_eq!(udp.max_payload(), Some(cap));
        let tcp_addr = tcp.local_addr();
        let udp_addr = udp.local_addr();
        let mut probe = OwnedChild::start(&[
            "probe-boundary".into(),
            tcp_addr.to_string(),
            udp_addr.to_string(),
            cap.to_string(),
        ]);
        let result = probe.event();
        // Keep cleanup independent of the assertions, including on failed probe.
        let probe_status = probe.finish();
        guest.shutdown().await;
        udp.shutdown().await;
        tcp.shutdown().await;
        host.shutdown().await;
        echo.stop();
        assert_eq!(echo.event()["event"], "echo_stopped");
        assert_eq!(echo.finish().code(), Some(0));
        assert_eq!(probe_status.code(), Some(0), "{result}");
        assert_eq!(result["event"], "boundary_probe_passed");
        assert_eq!(result["expectedPayload"], cap);
        assert_eq!(result["tcpBytesEachDirection"], 1_048_576);
        assert_eq!(result["tcpConcurrentWithUdp"], true);
        assert!(result["tcpBytesForwardedDuringUdp"].as_u64().unwrap() > 0);
        assert_eq!(result["udpExactEchoes"], 4);
        assert_eq!(result["udpHostAssociationsObserved"], 2);
        assert_eq!(result["udpOversizeRequests"], 2);
        assert_eq!(result["udpOversizeReachedEndpoint"], 0);
        assert_eq!(result["udpOversizeReturnsEmitted"], 4);
        assert_eq!(result["udpOversizeReturnsReachedGuest"], 0);
        assert_eq!(result["udpValidAfterRejection"], 2);
        assert_eq!(result["udpEndpointBytes"], cap * 4);
        assert!(!result.to_string().contains("127.0.0.1"));
        assert_eq!(host.metrics().active_udp, 0);
        let _endpoint_tcp = TcpListener::bind(endpoint_tcp).await.unwrap();
        let _endpoint_udp = UdpSocket::bind(endpoint_udp).await.unwrap();
        let _guest_tcp = TcpListener::bind(tcp_addr).await.unwrap();
        let _guest_udp = UdpSocket::bind(udp_addr).await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn direct_fixture_without_a_size_filter_cannot_falsely_pass() {
    let mut echo = OwnedChild::start(&[
        "echo-boundary".into(),
        "127.0.0.1:0".into(),
        "127.0.0.1:0".into(),
        "30".into(),
    ]);
    let ready = echo.event();
    let mut probe = OwnedChild::start(&[
        "probe-boundary".into(),
        ready["tcp"].as_str().unwrap().into(),
        ready["udp"].as_str().unwrap().into(),
        "1264".into(),
    ]);
    let failure = probe.event();
    assert_eq!(probe.finish().code(), Some(1));
    echo.stop();
    assert_eq!(echo.event()["event"], "echo_stopped");
    assert_eq!(echo.finish().code(), Some(0));
    assert_eq!(failure["event"], "failed");
    assert_eq!(failure["code"], "boundary_udp_integrity");
    assert!(!failure.to_string().contains("127.0.0.1"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stopping_while_an_owned_control_header_is_stalled_reaps_and_releases_ports() {
    let mut echo = OwnedChild::start(&[
        "echo-boundary".into(),
        "127.0.0.1:0".into(),
        "127.0.0.1:0".into(),
        "30".into(),
    ]);
    let ready = echo.event();
    let tcp: SocketAddr = ready["tcp"].as_str().unwrap().parse().unwrap();
    let udp: SocketAddr = ready["udp"].as_str().unwrap().parse().unwrap();
    let mut stalled = std::net::TcpStream::connect(tcp).unwrap();
    stalled.write_all(b"ACBP0001\x01").unwrap();
    echo.stop();
    assert_eq!(echo.event()["event"], "echo_stopped");
    assert_eq!(echo.finish().code(), Some(0));
    drop(stalled);
    let _tcp = TcpListener::bind(tcp).await.unwrap();
    let _udp = UdpSocket::bind(udp).await.unwrap();
}
