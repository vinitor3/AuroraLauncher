use super::*;
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::UdpSocket,
    os::windows::process::CommandExt,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const CHILD_MARKER: &str = "AURORA_OWNED_UDP_BINDING_FIXTURE_CHILD";
const CHILD_TEST: &str =
    "engine::connect_discovery::udp::owned_process_tests::udp_binding_child_fixture";
const EVENT_PREFIX: &str = "UDP_BINDING_EVENT ";
const MAX_LINE: usize = 1024;
const MAX_OUTPUT: usize = 16 * 1024;
const MAX_COMMANDS: usize = 16;
const WAIT: Duration = Duration::from_secs(5);
const DROP_WAIT: Duration = Duration::from_secs(2);

#[derive(Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "camelCase", deny_unknown_fields)]
enum ChildCommand {
    Bind { endpoints: Vec<SocketAddr> },
    Release,
    Rebind,
    Stop,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase", deny_unknown_fields)]
enum ChildEvent {
    Ready { endpoints: Vec<SocketAddr> },
    Released,
    Rebound,
    Stopped,
}

fn read_line_bounded(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, &'static str> {
    let mut line = Vec::new();
    let count = reader
        .take((MAX_LINE + 1) as u64)
        .read_until(b'\n', &mut line)
        .map_err(|_| "fixture_line_read")?;
    if count == 0 {
        return Ok(None);
    }
    if count > MAX_LINE || line.last() != Some(&b'\n') {
        return Err("fixture_line_limit");
    }
    Ok(Some(line))
}

fn child_emit(event: ChildEvent) -> Result<(), &'static str> {
    let encoded = serde_json::to_vec(&event).map_err(|_| "fixture_event_encode")?;
    if encoded.len() + EVENT_PREFIX.len() + 1 > MAX_LINE {
        return Err("fixture_event_limit");
    }
    let mut output = std::io::stdout().lock();
    output
        // libtest may leave its test-name prefix on the current line.
        .write_all(b"\n")
        .and_then(|_| output.write_all(EVENT_PREFIX.as_bytes()))
        .and_then(|_| output.write_all(&encoded))
        .and_then(|_| output.write_all(b"\n"))
        .and_then(|_| output.flush())
        .map_err(|_| "fixture_event_write")
}

fn child_bind(endpoints: &[SocketAddr]) -> Result<Vec<UdpSocket>, &'static str> {
    if endpoints.is_empty() || endpoints.len() > 4 {
        return Err("fixture_endpoint_limit");
    }
    endpoints
        .iter()
        .map(|endpoint| {
            // Wildcards exist only to prove that an OS wildcard row cannot
            // satisfy the production API's exact literal-loopback request.
            let permitted = match endpoint {
                SocketAddr::V4(value) => value.ip().is_loopback() || value.ip().is_unspecified(),
                SocketAddr::V6(value) => {
                    (value.ip().is_loopback() || value.ip().is_unspecified())
                        && value.scope_id() == 0
                        && value.flowinfo() == 0
                }
            };
            if !permitted {
                return Err("fixture_endpoint_invalid");
            }
            UdpSocket::bind(endpoint).map_err(|_| "fixture_bind")
        })
        .collect()
}

fn child_loop() -> Result<(), &'static str> {
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut endpoints = Vec::new();
    let mut sockets: Option<Vec<UdpSocket>> = None;
    for _ in 0..MAX_COMMANDS {
        let Some(line) = read_line_bounded(&mut input)? else {
            return Ok(());
        };
        let command: ChildCommand =
            serde_json::from_slice(&line).map_err(|_| "fixture_command_invalid")?;
        match command {
            ChildCommand::Bind { endpoints: wanted } if endpoints.is_empty() => {
                let bound = child_bind(&wanted)?;
                endpoints = bound
                    .iter()
                    .map(|socket| socket.local_addr().map_err(|_| "fixture_local_address"))
                    .collect::<Result<_, _>>()?;
                sockets = Some(bound);
                child_emit(ChildEvent::Ready {
                    endpoints: endpoints.clone(),
                })?;
            }
            ChildCommand::Release if sockets.is_some() => {
                drop(sockets.take());
                child_emit(ChildEvent::Released)?;
            }
            ChildCommand::Rebind if !endpoints.is_empty() && sockets.is_none() => {
                sockets = Some(child_bind(&endpoints)?);
                child_emit(ChildEvent::Rebound)?;
            }
            ChildCommand::Stop => {
                drop(sockets.take());
                child_emit(ChildEvent::Stopped)?;
                return Ok(());
            }
            _ => return Err("fixture_command_state"),
        }
    }
    Err("fixture_command_limit")
}

#[test]
#[ignore = "invoked only as a hidden, owned child by the UDP binding tests"]
fn udp_binding_child_fixture() {
    if std::env::var(CHILD_MARKER).as_deref() != Ok("1") {
        return;
    }
    child_loop().expect("udp_fixture_child_protocol");
}

struct ReaderDone(Arc<AtomicBool>);
impl Drop for ReaderDone {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn read_events(
    output: impl Read,
    sender: SyncSender<Result<ChildEvent, &'static str>>,
    done: Arc<AtomicBool>,
) {
    let _done = ReaderDone(done);
    let mut reader = BufReader::new(output.take((MAX_OUTPUT + 1) as u64));
    let mut total = 0;
    loop {
        let line = match read_line_bounded(&mut reader) {
            Ok(Some(line)) => line,
            Ok(None) => return,
            Err(error) => {
                let _ = sender.try_send(Err(error));
                return;
            }
        };
        total += line.len();
        if total > MAX_OUTPUT {
            let _ = sender.try_send(Err("fixture_output_limit"));
            return;
        }
        // The surrounding libtest harness also writes bounded status lines.
        if let Some(encoded) = line.strip_prefix(EVENT_PREFIX.as_bytes()) {
            let event = serde_json::from_slice(encoded).map_err(|_| "fixture_event_invalid");
            let failed = event.is_err();
            if sender.try_send(event).is_err() || failed {
                return;
            }
        }
    }
}

struct Fixture {
    child: Child,
    endpoints: Vec<SocketAddr>,
    events: Receiver<Result<ChildEvent, &'static str>>,
    reader: Option<JoinHandle<()>>,
    reader_done: Arc<AtomicBool>,
    fallback_used: Arc<AtomicBool>,
}

impl Fixture {
    fn new() -> Self {
        Self::at(vec![
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            SocketAddr::from((Ipv6Addr::LOCALHOST, 0)),
        ])
    }

    fn at(wanted: Vec<SocketAddr>) -> Self {
        let (sender, events) = mpsc::sync_channel(MAX_COMMANDS);
        let child = Command::new(std::env::current_exe().expect("fixture_executable"))
            .args([
                "--ignored",
                "--exact",
                CHILD_TEST,
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_MARKER, "1")
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("fixture_spawn");
        // Install the owner immediately after spawn, before taking pipes,
        // spawning the reader, sending commands, or checking readiness.
        let mut fixture = Self {
            child,
            endpoints: Vec::new(),
            events,
            reader: None,
            reader_done: Arc::new(AtomicBool::new(false)),
            fallback_used: Arc::new(AtomicBool::new(false)),
        };
        let output = fixture.child.stdout.take().expect("fixture_stdout");
        let done = fixture.reader_done.clone();
        fixture.reader = Some(
            thread::Builder::new()
                .name("owned-udp-fixture-output".into())
                .spawn(move || read_events(output, sender, done))
                .expect("fixture_reader_spawn"),
        );
        fixture.send(&ChildCommand::Bind {
            endpoints: wanted.clone(),
        });
        let ChildEvent::Ready { endpoints } = fixture.next_event() else {
            panic!("fixture_ready_event");
        };
        fixture.endpoints = endpoints;
        assert_eq!(fixture.endpoints.len(), wanted.len(), "fixture_ready_count");
        for (actual, wanted) in fixture.endpoints.iter().zip(wanted) {
            assert_eq!(actual.ip(), wanted.ip(), "fixture_ready_address");
            assert_ne!(actual.port(), 0, "fixture_ready_nonzero_port");
            if wanted.port() != 0 {
                assert_eq!(actual.port(), wanted.port(), "fixture_ready_exact_port");
            }
        }
        fixture
    }

    fn proof(&mut self, run: &str) -> OwnedChildProof {
        OwnedChildProof::from_child(&mut self.child, "udp-fixture-instance", run)
            .expect("fixture_owned_child_proof")
    }

    fn try_send(&mut self, command: &ChildCommand) -> Result<(), &'static str> {
        let mut bytes = serde_json::to_vec(command).map_err(|_| "fixture_command_encode")?;
        bytes.push(b'\n');
        if bytes.len() > MAX_LINE {
            return Err("fixture_command_limit");
        }
        self.child
            .stdin
            .as_mut()
            .ok_or("fixture_stdin")?
            .write_all(&bytes)
            .and_then(|_| self.child.stdin.as_mut().expect("fixture_stdin").flush())
            .map_err(|_| "fixture_command_write")
    }

    fn send(&mut self, command: &ChildCommand) {
        self.try_send(command).expect("fixture_command_send");
    }

    fn next_event(&self) -> ChildEvent {
        self.events
            .recv_timeout(WAIT)
            .expect("fixture_event_deadline")
            .expect("fixture_event_protocol")
    }

    fn release(&mut self) {
        self.send(&ChildCommand::Release);
        assert!(matches!(self.next_event(), ChildEvent::Released));
    }

    fn rebind(&mut self) {
        self.send(&ChildCommand::Rebind);
        assert!(matches!(self.next_event(), ChildEvent::Rebound));
    }

    fn stop(&mut self) {
        self.send(&ChildCommand::Stop);
        assert!(matches!(self.next_event(), ChildEvent::Stopped));
        drop(self.child.stdin.take());
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait().expect("fixture_wait") {
                assert!(status.success(), "fixture_child_success");
                break;
            }
            assert!(Instant::now() < deadline, "fixture_exit_deadline");
            thread::sleep(Duration::from_millis(5));
        }
        self.reader
            .take()
            .expect("fixture_reader")
            .join()
            .expect("fixture_reader_join");
        assert!(
            self.reader_done.load(Ordering::Acquire),
            "fixture_reader_finished"
        );
        assert!(
            !self.fallback_used.load(Ordering::Acquire),
            "fixture_normal_stop"
        );
        assert_ports_released(&self.endpoints);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.try_send(&ChildCommand::Stop);
            drop(self.child.stdin.take());
            let deadline = Instant::now() + DROP_WAIT;
            while !matches!(self.child.try_wait(), Ok(Some(_))) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            if !matches!(self.child.try_wait(), Ok(Some(_))) {
                self.fallback_used.store(true, Ordering::Release);
                let _ = self.child.kill();
            }
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn assert_ports_released(endpoints: &[SocketAddr]) {
    // Keep every rebound socket alive together until all ports are checked.
    let rebound: Vec<_> = endpoints
        .iter()
        .map(|endpoint| UdpSocket::bind(endpoint).expect("fixture_port_released"))
        .collect();
    drop(rebound);
}

fn require(proof: &OwnedChildProof, endpoint: SocketAddr) -> Result<(), DiscoveryError> {
    super::super::require_owned_udp_binding(proof, endpoint)
}

#[test]
fn owned_child_exact_udp_bindings_and_identity_are_preserved() {
    let mut fixture = Fixture::at(vec![
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        SocketAddr::from((Ipv6Addr::LOCALHOST, 0)),
        SocketAddr::from((Ipv4Addr::new(127, 0, 0, 2), 0)),
    ]);
    let proof = fixture.proof("udp-original-run");
    let duplicate = proof.duplicate().expect("fixture_duplicate_proof");
    let identity = proof.fingerprint();
    for endpoint in &fixture.endpoints {
        assert_eq!(require(&proof, *endpoint), Ok(()));
        assert_eq!(require(&duplicate, *endpoint), Ok(()));
    }
    assert_eq!(proof.instance_id(), "udp-fixture-instance");
    assert_eq!(proof.run_id(), "udp-original-run");
    assert_eq!(duplicate.instance_id(), proof.instance_id());
    assert_eq!(duplicate.run_id(), proof.run_id());
    assert!(proof.fingerprint() == identity && duplicate.fingerprint() == identity);
    fixture.stop();
    for endpoint in &fixture.endpoints {
        assert_eq!(
            require(&proof, *endpoint),
            Err(DiscoveryError::ProcessChanged)
        );
        assert_eq!(
            require(&duplicate, *endpoint),
            Err(DiscoveryError::ProcessChanged)
        );
    }
}

#[test]
fn owned_udp_binding_rejects_invalid_or_divergent_endpoints() {
    let mut fixture = Fixture::new();
    let proof = fixture.proof("udp-endpoints-run");
    let port = fixture.endpoints[0].port();
    for endpoint in [
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)),
        SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)),
        SocketAddr::from((Ipv4Addr::new(192, 0, 2, 1), port)),
        SocketAddr::from((Ipv4Addr::LOCALHOST.to_ipv6_mapped(), port)),
        SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 1)),
        SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 1, 0)),
    ] {
        assert_eq!(
            require(&proof, endpoint),
            Err(DiscoveryError::InvalidRequest)
        );
    }
    assert_eq!(
        require(
            &proof,
            SocketAddr::from((Ipv4Addr::new(127, 0, 0, 2), port))
        ),
        Err(DiscoveryError::ListenerChanged)
    );
    for endpoint in &fixture.endpoints {
        assert_eq!(require(&proof, *endpoint), Ok(()));
    }
    let mut wildcard = Fixture::at(vec![
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
        SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
    ]);
    let wildcard_proof = wildcard.proof("udp-wildcard-run");
    for endpoint in &wildcard.endpoints {
        let literal = if endpoint.is_ipv4() {
            SocketAddr::from((Ipv4Addr::LOCALHOST, endpoint.port()))
        } else {
            SocketAddr::from((Ipv6Addr::LOCALHOST, endpoint.port()))
        };
        assert_eq!(
            require(&wildcard_proof, *endpoint),
            Err(DiscoveryError::InvalidRequest)
        );
        assert_eq!(
            require(&wildcard_proof, literal),
            Err(DiscoveryError::ListenerChanged)
        );
    }
    wildcard.stop();
    fixture.stop();
}

#[test]
fn parent_and_other_child_udp_sockets_never_satisfy_original_proof() {
    let mut first = Fixture::new();
    let mut second = Fixture::new();
    let first_proof = first.proof("udp-first-run");
    let second_proof = second.proof("udp-second-run");
    let parent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("parent_bind");
    let parent_endpoint = parent.local_addr().expect("parent_endpoint");
    assert_eq!(
        require(&first_proof, parent_endpoint),
        Err(DiscoveryError::ListenerChanged)
    );
    for endpoint in &second.endpoints {
        assert_eq!(
            require(&first_proof, *endpoint),
            Err(DiscoveryError::ListenerChanged)
        );
        assert_eq!(require(&second_proof, *endpoint), Ok(()));
    }
    for endpoint in &first.endpoints {
        assert_eq!(
            require(&second_proof, *endpoint),
            Err(DiscoveryError::ListenerChanged)
        );
        assert_eq!(require(&first_proof, *endpoint), Ok(()));
    }
    drop(parent);
    assert_ports_released(&[parent_endpoint]);
    second.stop();
    first.stop();
}

#[test]
fn owned_child_release_and_rebind_is_only_a_new_observation() {
    let mut fixture = Fixture::new();
    let proof = fixture.proof("udp-rebind-run");
    let identity = proof.fingerprint();
    for endpoint in &fixture.endpoints {
        assert_eq!(require(&proof, *endpoint), Ok(()));
    }
    fixture.release();
    assert_eq!(proof.recheck(), Ok(()));
    for endpoint in &fixture.endpoints {
        assert_eq!(
            require(&proof, *endpoint),
            Err(DiscoveryError::ListenerChanged)
        );
    }
    assert_ports_released(&fixture.endpoints);
    fixture.rebind();
    for endpoint in &fixture.endpoints {
        assert_eq!(require(&proof, *endpoint), Ok(()));
    }
    assert!(proof.fingerprint() == identity);
    assert_eq!(proof.run_id(), "udp-rebind-run");
    fixture.stop();
}

#[test]
fn ended_child_proof_never_adopts_replacement_at_same_endpoint() {
    let mut original = Fixture::new();
    let proof = original.proof("udp-ended-run");
    let identity = proof.fingerprint();
    let endpoints = original.endpoints.clone();
    for endpoint in &endpoints {
        assert_eq!(require(&proof, *endpoint), Ok(()));
    }
    original.stop();
    let mut replacement = Fixture::at(endpoints.clone());
    let replacement_proof = replacement.proof("udp-replacement-run");
    for endpoint in &endpoints {
        assert_eq!(
            require(&proof, *endpoint),
            Err(DiscoveryError::ProcessChanged)
        );
        assert_eq!(require(&replacement_proof, *endpoint), Ok(()));
    }
    assert!(proof.fingerprint() == identity);
    assert!(replacement_proof.fingerprint() != identity);
    assert_eq!(proof.run_id(), "udp-ended-run");
    assert_eq!(replacement_proof.run_id(), "udp-replacement-run");
    replacement.stop();
}

#[test]
fn exit_after_positive_real_udp_snapshot_rejects_same_process_proof() {
    for index in 0..2 {
        let mut fixture = Fixture::new();
        let proof = fixture.proof("udp-snapshot-run");
        let endpoint = fixture.endpoints[index];
        let endpoints = fixture.endpoints.clone();
        let reader_done = fixture.reader_done.clone();
        let observed = Arc::new(AtomicBool::new(false));
        let callback_observed = observed.clone();
        // No query override: this callback runs only after decoding the real
        // Win32 table. The original Child dies before the final same-handle check.
        let mut reader = TableReader {
            after_decode: Some(Box::new(move |matched| {
                assert!(matched, "real_snapshot_contains_owned_binding");
                callback_observed.store(true, Ordering::Release);
                fixture.stop();
            })),
            ..TableReader::default()
        };
        assert_eq!(
            require_with_reader(&proof, endpoint, &mut reader),
            Err(DiscoveryError::ProcessChanged)
        );
        assert!(observed.load(Ordering::Acquire));
        assert!(reader_done.load(Ordering::Acquire));
        assert_eq!(proof.recheck(), Err(DiscoveryError::ProcessChanged));
        assert_ports_released(&endpoints);
    }
}

#[test]
fn exit_after_nonmatching_real_snapshot_has_process_changed_precedence() {
    let mut fixture = Fixture::new();
    let proof = fixture.proof("udp-missing-snapshot-run");
    let endpoints = fixture.endpoints.clone();
    let parent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("parent_bind");
    let endpoint = parent.local_addr().expect("parent_endpoint");
    let observed = Arc::new(AtomicBool::new(false));
    let callback_observed = observed.clone();
    let mut reader = TableReader {
        after_decode: Some(Box::new(move |matched| {
            assert!(!matched, "real_snapshot_does_not_adopt_parent_binding");
            callback_observed.store(true, Ordering::Release);
            fixture.stop();
        })),
        ..TableReader::default()
    };
    assert_eq!(
        require_with_reader(&proof, endpoint, &mut reader),
        Err(DiscoveryError::ProcessChanged)
    );
    assert!(observed.load(Ordering::Acquire));
    assert_ports_released(&endpoints);
    drop(parent);
    assert_ports_released(&[endpoint]);
}

#[test]
fn fixture_unwind_stops_own_child_and_joins_reader() {
    let mut retained = None;
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut fixture = Fixture::new();
        let proof = fixture.proof("udp-unwind-run");
        retained = Some((
            proof,
            fixture.endpoints.clone(),
            fixture.reader_done.clone(),
            fixture.fallback_used.clone(),
        ));
        panic!("intentional_fixture_cleanup_probe");
    }));
    assert!(caught.is_err());
    let (proof, endpoints, reader_done, fallback_used) = retained.expect("fixture_started");
    assert_eq!(proof.recheck(), Err(DiscoveryError::ProcessChanged));
    assert!(
        reader_done.load(Ordering::Acquire),
        "fixture_unwind_reader_joined"
    );
    assert!(
        !fallback_used.load(Ordering::Acquire),
        "fixture_unwind_normal_stop"
    );
    assert_ports_released(&endpoints);
}
