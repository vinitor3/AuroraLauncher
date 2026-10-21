use super::*;
use serde::{Deserialize, Serialize};
use std::{
    io,
    net::SocketAddrV6,
    os::windows::io::{AsRawSocket, FromRawSocket, OwnedSocket},
};
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

const CHILD_MARKER: &str = "AURORA_OWNED_UDP_LOOPBACK_FIXTURE_CHILD";
const CHILD_TEST: &str =
    "engine::connect_discovery::udp::loopback::owned_process_tests::udp_loopback_child_fixture";
const EVENT_PREFIX: &str = "UDP_LOOPBACK_EVENT ";
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
    Echo { index: usize },
    Stop,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase", deny_unknown_fields)]
enum ChildEvent {
    Ready { endpoints: Vec<SocketAddr> },
    Released,
    Rebound,
    Echoed,
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

// Only a fixture changes the Winsock dual-stack option, before bind. This is
// independent of Java and cannot affect the production query or policy82.
fn bind_mapped_socket(endpoint: SocketAddrV6) -> io::Result<UdpSocket> {
    use windows_sys::Win32::Networking::WinSock::{
        bind, setsockopt, socket, AF_INET6, IN6_ADDR, IN6_ADDR_0, INVALID_SOCKET, IPPROTO_IPV6,
        IPPROTO_UDP, IPV6_V6ONLY, SOCKADDR_IN6, SOCKADDR_IN6_0, SOCK_DGRAM,
    };
    // Standard library initializes Winsock for this process. Its temporary
    // socket is dropped before creating the mapped fixture socket.
    drop(UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?);
    // SAFETY: all scalar arguments select a plain UDP socket.
    let raw = unsafe { socket(AF_INET6 as i32, SOCK_DGRAM, IPPROTO_UDP) };
    if raw == INVALID_SOCKET {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: socket returned one new owned handle; install RAII immediately.
    let owned = unsafe { OwnedSocket::from_raw_socket(raw as _) };
    let v6_only = 0_u32;
    // SAFETY: owned is alive and the pointer covers exactly one DWORD.
    if unsafe {
        setsockopt(
            owned.as_raw_socket() as _,
            IPPROTO_IPV6,
            IPV6_V6ONLY,
            (&v6_only as *const u32).cast(),
            size_of::<u32>() as i32,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    let address = SOCKADDR_IN6 {
        sin6_family: AF_INET6,
        sin6_port: endpoint.port().to_be(),
        sin6_flowinfo: 0,
        sin6_addr: IN6_ADDR {
            u: IN6_ADDR_0 {
                Byte: endpoint.ip().octets(),
            },
        },
        Anonymous: SOCKADDR_IN6_0 { sin6_scope_id: 0 },
    };
    // SAFETY: initialized SDK sockaddr layout, valid for this synchronous call.
    if unsafe {
        bind(
            owned.as_raw_socket() as _,
            (&address as *const SOCKADDR_IN6).cast(),
            size_of::<SOCKADDR_IN6>() as i32,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(owned.into())
}

fn bind_fixture_socket(endpoint: SocketAddr) -> io::Result<UdpSocket> {
    match endpoint {
        SocketAddr::V6(value) if *value.ip() == Ipv4Addr::LOCALHOST.to_ipv6_mapped() => {
            bind_mapped_socket(value)
        }
        _ => UdpSocket::bind(endpoint),
    }
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
                    (value.ip().is_loopback()
                        || value.ip().is_unspecified()
                        || *value.ip() == Ipv4Addr::LOCALHOST.to_ipv6_mapped())
                        && value.scope_id() == 0
                        && value.flowinfo() == 0
                }
            };
            if !permitted {
                return Err("fixture_endpoint_invalid");
            }
            bind_fixture_socket(*endpoint).map_err(|_| "fixture_bind")
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
            ChildCommand::Echo { index } => {
                let socket = sockets
                    .as_ref()
                    .and_then(|values| values.get(index))
                    .ok_or("fixture_echo_socket")?;
                socket
                    .set_read_timeout(Some(WAIT))
                    .map_err(|_| "fixture_echo_timeout")?;
                let mut bytes = [0_u8; 2048];
                let (length, peer) = socket
                    .recv_from(&mut bytes)
                    .map_err(|_| "fixture_echo_receive")?;
                if length > 1264
                    || socket
                        .send_to(&bytes[..length], peer)
                        .map_err(|_| "fixture_echo_send")?
                        != length
                {
                    return Err("fixture_echo_length");
                }
                child_emit(ChildEvent::Echoed)?;
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
fn udp_loopback_child_fixture() {
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
            SocketAddr::from((Ipv4Addr::LOCALHOST.to_ipv6_mapped(), 0)),
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
        .map(|endpoint| bind_fixture_socket(*endpoint).expect("fixture_port_released"))
        .collect();
    drop(rebound);
}

fn requested(endpoint: SocketAddr) -> SocketAddrV4 {
    SocketAddrV4::new(Ipv4Addr::LOCALHOST, endpoint.port())
}

fn require(
    proof: &OwnedChildProof,
    endpoint: SocketAddr,
) -> Result<UdpLoopbackRepresentation, DiscoveryError> {
    crate::engine::connect_discovery::require_owned_udp_loopback_binding(proof, requested(endpoint))
}

fn exchange(fixture: &mut Fixture, index: usize) {
    let endpoint = requested(fixture.endpoints[index]);
    let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("fixture_sender_bind");
    sender
        .set_read_timeout(Some(WAIT))
        .expect("fixture_sender_timeout");
    let sent: Vec<_> = (0..1264).map(|value| (value % 251) as u8).collect();
    fixture.send(&ChildCommand::Echo { index });
    assert_eq!(
        sender
            .send_to(&sent, endpoint)
            .expect("fixture_sender_send"),
        sent.len()
    );
    let mut received = [0_u8; 2048];
    let (length, origin) = sender
        .recv_from(&mut received)
        .expect("fixture_sender_receive");
    assert_eq!(origin, SocketAddr::V4(endpoint));
    assert_eq!(&received[..length], sent);
    assert!(matches!(fixture.next_event(), ChildEvent::Echoed));
}

#[test]
fn real_literal_rows_from_v4_and_dual_stack_construction_preserve_identity_and_bytes() {
    let mut fixture = Fixture::new();
    let proof = fixture.proof("loopback-original");
    let duplicate = proof.duplicate().expect("fixture_duplicate_proof");
    let identity = proof.fingerprint();
    for index in 0..2 {
        let endpoint = fixture.endpoints[index];
        // The dual-stack socket's local_addr is mapped, while this host's
        // IPHelper row is literally AF_INET. Never infer one from the other.
        assert_eq!(
            require(&proof, endpoint),
            Ok(UdpLoopbackRepresentation::Ipv4Literal)
        );
        assert_eq!(
            require(&duplicate, endpoint),
            Ok(UdpLoopbackRepresentation::Ipv4Literal)
        );
        let strict = crate::engine::connect_discovery::require_owned_udp_binding;
        assert_eq!(strict(&proof, SocketAddr::V4(requested(endpoint))), Ok(()));
        assert_eq!(
            strict(&proof, endpoint),
            if endpoint.is_ipv4() {
                Ok(())
            } else {
                Err(DiscoveryError::InvalidRequest)
            }
        );
        assert!(
            bind_fixture_socket(endpoint).is_err(),
            "occupied_endpoint_must_refuse_second_bind"
        );
        exchange(&mut fixture, index);
        assert_eq!(
            require(&proof, endpoint),
            Ok(UdpLoopbackRepresentation::Ipv4Literal)
        );
    }
    assert_eq!(proof.instance_id(), "udp-fixture-instance");
    assert_eq!(proof.run_id(), "loopback-original");
    assert_eq!(duplicate.run_id(), proof.run_id());
    assert!(proof.fingerprint() == identity && duplicate.fingerprint() == identity);
    fixture.stop();
    for endpoint in &fixture.endpoints {
        assert_eq!(
            require(&proof, *endpoint),
            Err(DiscoveryError::ProcessChanged)
        );
    }
}

#[test]
fn real_foreign_parent_and_other_child_rows_do_not_satisfy_original_proof() {
    let mut original = Fixture::new();
    let mut other = Fixture::new();
    let proof = original.proof("loopback-foreign-original");
    let other_proof = other.proof("loopback-foreign-other");
    for index in 0..2 {
        let endpoint = other.endpoints[index];
        assert_eq!(
            require(&proof, endpoint),
            Err(DiscoveryError::ListenerChanged)
        );
        assert_eq!(
            require(&other_proof, endpoint),
            Ok(UdpLoopbackRepresentation::Ipv4Literal)
        );
        let parent = bind_fixture_socket(if index == 0 {
            "127.0.0.1:0".parse().unwrap()
        } else {
            "[::ffff:127.0.0.1]:0".parse().unwrap()
        })
        .expect("fixture_parent_bind");
        let parent_endpoint = parent.local_addr().expect("fixture_parent_address");
        assert_eq!(
            require(&proof, parent_endpoint),
            Err(DiscoveryError::ListenerChanged)
        );
        drop(parent);
        assert_ports_released(&[parent_endpoint]);
    }
    other.stop();
    original.stop();
}

#[test]
fn real_wildcard_other_loopback_and_two_same_port_rows_are_rejected() {
    let mut fixture = Fixture::new();
    let proof = fixture.proof("loopback-negatives");
    for address in ["0.0.0.0:0", "127.0.0.2:0", "[::]:0", "[::1]:0"] {
        let mut negative = Fixture::at(vec![address.parse().unwrap()]);
        let negative_proof = negative.proof("loopback-negative");
        let endpoint = negative.endpoints[0];
        assert_eq!(
            require(&negative_proof, endpoint),
            Err(DiscoveryError::ListenerChanged)
        );
        negative.stop();
    }
    let endpoint = fixture.endpoints[0];
    assert_eq!(
        require(&proof, endpoint),
        Ok(UdpLoopbackRepresentation::Ipv4Literal)
    );
    let other_address = SocketAddr::from((Ipv4Addr::new(127, 0, 0, 2), endpoint.port()));
    // Same port on another specific address, with a foreign PID, invalidates
    // uniqueness even though the original exact row remains present.
    let mut foreign = Fixture::at(vec![other_address]);
    assert_eq!(
        require(&proof, endpoint),
        Err(DiscoveryError::ListenerChanged)
    );
    foreign.stop();
    assert_eq!(
        require(&proof, endpoint),
        Ok(UdpLoopbackRepresentation::Ipv4Literal)
    );
    fixture.stop();
    let reserve = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("fixture_port_reserve");
    let port = reserve.local_addr().unwrap().port();
    drop(reserve);
    let mut duplicate = Fixture::at(vec![
        SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        SocketAddr::from((Ipv4Addr::new(127, 0, 0, 2), port)),
    ]);
    let duplicate_proof = duplicate.proof("loopback-two-own-rows");
    assert_eq!(
        require(&duplicate_proof, duplicate.endpoints[0]),
        Err(DiscoveryError::ListenerChanged)
    );
    duplicate.stop();
}

#[test]
fn real_release_rebind_is_new_observation_and_replacement_never_inherits_proof() {
    let mut original = Fixture::new();
    let proof = original.proof("loopback-release-original");
    let identity = proof.fingerprint();
    let endpoints = original.endpoints.clone();
    for endpoint in &endpoints {
        assert_eq!(
            require(&proof, *endpoint),
            Ok(UdpLoopbackRepresentation::Ipv4Literal)
        );
    }
    original.release();
    assert_eq!(proof.recheck(), Ok(()));
    for endpoint in &endpoints {
        assert_eq!(
            require(&proof, *endpoint),
            Err(DiscoveryError::ListenerChanged)
        );
    }
    assert_ports_released(&endpoints);
    original.rebind();
    for (index, endpoint) in endpoints.iter().enumerate() {
        assert_eq!(
            require(&proof, *endpoint),
            Ok(UdpLoopbackRepresentation::Ipv4Literal)
        );
        exchange(&mut original, index);
    }
    assert!(proof.fingerprint() == identity);
    original.stop();
    let mut replacement = Fixture::at(endpoints.clone());
    let replacement_proof = replacement.proof("loopback-replacement");
    for endpoint in &endpoints {
        assert_eq!(
            require(&proof, *endpoint),
            Err(DiscoveryError::ProcessChanged)
        );
        assert_eq!(
            require(&replacement_proof, *endpoint),
            Ok(UdpLoopbackRepresentation::Ipv4Literal)
        );
    }
    assert!(replacement_proof.fingerprint() != identity);
    replacement.stop();
}

#[test]
fn death_after_positive_real_two_family_snapshot_overrides_sample() {
    for index in 0..2 {
        let mut fixture = Fixture::new();
        let proof = fixture.proof("loopback-positive-death");
        let endpoint = requested(fixture.endpoints[index]);
        let endpoints = fixture.endpoints.clone();
        let reader_done = fixture.reader_done.clone();
        let reached = Arc::new(AtomicBool::new(false));
        let observed = reached.clone();
        let mut reader = TableReader {
            after_decode: Some(Box::new(move |matched| {
                assert!(matched, "actual_win32_two_family_positive_snapshot");
                observed.store(true, Ordering::Release);
                fixture.stop();
            })),
            ..TableReader::default()
        };
        assert_eq!(
            require_with_reader(&proof, endpoint, &mut reader),
            Err(DiscoveryError::ProcessChanged)
        );
        assert!(reached.load(Ordering::Acquire));
        assert!(reader_done.load(Ordering::Acquire));
        assert_ports_released(&endpoints);
    }
}

#[test]
fn death_after_absence_or_table_error_still_rechecks_original_proof() {
    use windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER;
    for failure in 0..4 {
        let mut fixture = Fixture::new();
        let proof = fixture.proof("loopback-error-death");
        let endpoint = requested(fixture.endpoints[0]);
        assert_eq!(
            require(&proof, fixture.endpoints[0]),
            Ok(UdpLoopbackRepresentation::Ipv4Literal)
        );
        let parent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("fixture_foreign_parent");
        let requested_endpoint = if failure == 0 {
            requested(parent.local_addr().unwrap())
        } else {
            endpoint
        };
        let reached = Arc::new(AtomicBool::new(false));
        let observed = reached.clone();
        let mut reader = TableReader {
            query: if failure == 0 {
                None
            } else {
                Some(Box::new(move |family, buffer, length| {
                    if failure == 3 && family == Family::V4 {
                        // Preserve the real favorable IPv4 table; fail only
                        // the second family before the post-error recheck.
                        TableReader::default().query(family, buffer, length)
                    } else if failure == 1 || failure == 3 {
                        ERROR_INVALID_PARAMETER
                    } else if let Some(buffer) = buffer {
                        assert_eq!(family, Family::V4);
                        buffer[0] = 1;
                        *length = 4; // Truncated row in a nominally successful table.
                        0
                    } else {
                        *length = 4;
                        windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER
                    }
                }))
            },
            after_decode: Some(Box::new(move |matched| {
                assert!(!matched);
                observed.store(true, Ordering::Release);
                fixture.stop();
            })),
        };
        assert_eq!(
            require_with_reader(&proof, requested_endpoint, &mut reader),
            Err(DiscoveryError::ProcessChanged)
        );
        assert!(reached.load(Ordering::Acquire));
        drop(parent);
    }
}

#[test]
fn invalid_input_precedes_io_and_fixture_unwind_reaps_original_and_reader() {
    let mut retained = None;
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut fixture = Fixture::new();
        let proof = fixture.proof("loopback-unwind");
        let mut reader = TableReader {
            query: Some(Box::new(|_, _, _| panic!("invalid_input_must_not_query"))),
            ..TableReader::default()
        };
        for endpoint in ["127.0.0.1:0", "127.0.0.2:4660", "0.0.0.0:4660"] {
            assert_eq!(
                require_with_reader(&proof, endpoint.parse().unwrap(), &mut reader),
                Err(DiscoveryError::InvalidRequest)
            );
        }
        retained = Some((
            proof,
            fixture.endpoints.clone(),
            fixture.reader_done.clone(),
            fixture.fallback_used.clone(),
        ));
        panic!("intentional_loopback_fixture_unwind");
    }));
    assert!(caught.is_err());
    let (proof, endpoints, reader_done, fallback_used) = retained.expect("fixture_started");
    assert_eq!(proof.recheck(), Err(DiscoveryError::ProcessChanged));
    assert!(reader_done.load(Ordering::Acquire));
    assert!(!fallback_used.load(Ordering::Acquire));
    assert_ports_released(&endpoints);
}

fn synthetic_mapped_reader(original_pid: u32, port: u16) -> TableReader {
    TableReader {
        query: Some(Box::new(move |family, buffer, length| {
            let offset = match family {
                Family::V4 => offset_of!(MIB_UDPTABLE_OWNER_PID, table),
                Family::V6 => offset_of!(MIB_UDP6TABLE_OWNER_PID, table),
            };
            let bytes = offset
                + if family == Family::V6 {
                    size_of::<MIB_UDP6ROW_OWNER_PID>()
                } else {
                    0
                };
            *length = bytes as u32;
            let Some(words) = buffer else {
                return windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
            };
            words.fill(0);
            if family == Family::V6 {
                words[0] = 1;
                let mut write = |position: usize, bytes: &[u8]| {
                    for (index, byte) in bytes.iter().enumerate() {
                        let position = position + index;
                        let mut word = words[position / 4].to_ne_bytes();
                        word[position % 4] = *byte;
                        words[position / 4] = u32::from_ne_bytes(word);
                    }
                };
                write(
                    offset + offset_of!(MIB_UDP6ROW_OWNER_PID, ucLocalAddr),
                    &Ipv4Addr::LOCALHOST.to_ipv6_mapped().octets(),
                );
                write(
                    offset + offset_of!(MIB_UDP6ROW_OWNER_PID, dwLocalPort),
                    &u32::from(port.to_be()).to_ne_bytes(),
                );
                write(
                    offset + offset_of!(MIB_UDP6ROW_OWNER_PID, dwOwningPid),
                    &original_pid.to_ne_bytes(),
                );
            }
            0
        })),
        ..TableReader::default()
    }
}

#[test]
fn mapped_classification_is_unavailable_at_real_boundary_after_original_proof_recheck() {
    let mut fixture = Fixture::new();
    let proof = fixture.proof("loopback-mapped-unqualified-boundary");
    let endpoint = requested(fixture.endpoints[0]);
    // Anchor this negative in the same original Child and a real positive
    // API/bytes exchange. Only Win32 table bytes use the existing test seam.
    assert_eq!(
        require(&proof, fixture.endpoints[0]),
        Ok(UdpLoopbackRepresentation::Ipv4Literal)
    );
    exchange(&mut fixture, 0);
    let mut reader = synthetic_mapped_reader(proof.fingerprint().0, endpoint.port());
    assert_eq!(
        require_with_reader(&proof, endpoint, &mut reader),
        Err(DiscoveryError::Unavailable)
    );
    assert_eq!(proof.recheck(), Ok(()));
    assert_eq!(
        require(&proof, fixture.endpoints[0]),
        Ok(UdpLoopbackRepresentation::Ipv4Literal)
    );

    let endpoints = fixture.endpoints.clone();
    let reader_done = fixture.reader_done.clone();
    let reached = Arc::new(AtomicBool::new(false));
    let observed = reached.clone();
    let mut reader = synthetic_mapped_reader(proof.fingerprint().0, endpoint.port());
    reader.after_decode = Some(Box::new(move |classified| {
        assert!(
            classified,
            "synthetic_mapped_reaches_production_restriction"
        );
        observed.store(true, Ordering::Release);
        fixture.stop();
    }));
    // Original process death takes precedence over the unqualified branch;
    // the restriction must not short-circuit the second original-handle check.
    assert_eq!(
        require_with_reader(&proof, endpoint, &mut reader),
        Err(DiscoveryError::ProcessChanged)
    );
    assert!(reached.load(Ordering::Acquire));
    assert!(reader_done.load(Ordering::Acquire));
    assert_ports_released(&endpoints);
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticCounts {
    total_rows: usize,
    original_rows: usize,
    foreign_rows: usize,
    zero_pid_rows: usize,
    original_literal_rows: usize,
    original_mapped_rows: usize,
    original_other_rows: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticPort {
    index: usize,
    fixture_form: &'static str,
    ipv4: DiagnosticCounts,
    ipv6: DiagnosticCounts,
    policy_result: &'static str,
}

fn diagnostic_family_counts(
    table: &Table,
    family: Family,
    original_pid: u32,
    endpoints: &[SocketAddr],
) -> Result<Vec<DiagnosticCounts>, DiscoveryError> {
    let (offset, row_bytes) = match family {
        Family::V4 => (
            offset_of!(MIB_UDPTABLE_OWNER_PID, table),
            size_of::<MIB_UDPROW_OWNER_PID>(),
        ),
        Family::V6 => (
            offset_of!(MIB_UDP6TABLE_OWNER_PID, table),
            size_of::<MIB_UDP6ROW_OWNER_PID>(),
        ),
    };
    let rows = table.row_count(offset, row_bytes)?;
    let mut counts = vec![DiagnosticCounts::default(); endpoints.len()];
    for index in 0..rows {
        let start = table.row_offset(offset, index, row_bytes)?;
        // SAFETY: the entire table and this SDK row have been range checked.
        let pointer = unsafe { table.storage.as_ptr().cast::<u8>().add(start) };
        let (observed, row_pid) = match family {
            Family::V4 => {
                // SAFETY: concrete integer-only SDK layout selected by family.
                let row = unsafe { pointer.cast::<MIB_UDPROW_OWNER_PID>().read_unaligned() };
                (super::super::ipv4_endpoint(row), row.dwOwningPid)
            }
            Family::V6 => {
                // SAFETY: concrete integer-only SDK layout selected by family.
                let row = unsafe { pointer.cast::<MIB_UDP6ROW_OWNER_PID>().read_unaligned() };
                (super::super::ipv6_endpoint(row), row.dwOwningPid)
            }
        };
        for (endpoint, count) in endpoints.iter().zip(&mut counts) {
            if observed.port() != endpoint.port() {
                continue;
            }
            // rows is bounded by MAX_ROWS, so these per-port counters cannot
            // overflow. No PID/address or foreign row leaves this helper.
            count.total_rows += 1;
            if row_pid == 0 {
                count.zero_pid_rows += 1;
            } else if original_pid != 0 && row_pid == original_pid {
                count.original_rows += 1;
                match observed {
                    SocketAddr::V4(value) if *value.ip() == Ipv4Addr::LOCALHOST => {
                        count.original_literal_rows += 1;
                    }
                    SocketAddr::V6(value)
                        if *value.ip() == Ipv4Addr::LOCALHOST.to_ipv6_mapped()
                            && value.scope_id() == 0 =>
                    {
                        count.original_mapped_rows += 1;
                    }
                    _ => count.original_other_rows += 1,
                }
            } else {
                count.foreign_rows += 1;
            }
        }
    }
    Ok(counts)
}

fn diagnostic_snapshot(
    proof: &OwnedChildProof,
    endpoints: &[SocketAddr],
) -> Result<Vec<DiagnosticPort>, DiscoveryError> {
    proof.recheck()?;
    let snapshot = (|| {
        let mut limited = Summary::default();
        let mut families = Vec::new();
        let mut reader = TableReader::default();
        for family in [Family::V4, Family::V6] {
            let counts = {
                let table = reader.table(family)?;
                // The existing production summary enforces complete layouts
                // and the same aggregate row/returned-byte budgets here.
                limited.merge(summarize(
                    &table,
                    family,
                    proof.fingerprint().0,
                    requested(endpoints[0]),
                )?)?;
                diagnostic_family_counts(&table, family, proof.fingerprint().0, endpoints)?
            }; // release this buffer before fetching the second family.
            families.push(counts);
        }
        Ok(endpoints
            .iter()
            .enumerate()
            .map(|(index, endpoint)| DiagnosticPort {
                index,
                fixture_form: if endpoint.is_ipv4() {
                    "Ipv4Literal"
                } else {
                    "Ipv4Mapped"
                },
                ipv4: families[0][index].clone(),
                ipv6: families[1][index].clone(),
                // This is the real API, not an inference from local_addr or
                // CIM. It is another sequential sample of the same proof.
                policy_result: match require(proof, *endpoint) {
                    Ok(UdpLoopbackRepresentation::Ipv4Literal) => "Ipv4Literal",
                    Ok(UdpLoopbackRepresentation::Ipv4Mapped) => "Ipv4Mapped",
                    Err(error) => error.code(),
                },
            })
            .collect())
    })();
    // Preserve the original proof even when table decoding/collection failed.
    proof.recheck()?;
    snapshot
}

fn diagnostic_write_new(
    path: &std::path::Path,
    value: &impl Serialize,
) -> Result<(), &'static str> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "diagnostic_encode")?;
    if bytes.len() > 16 * 1024 {
        return Err("diagnostic_write_limit");
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "diagnostic_create_new")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "diagnostic_write")
}

#[test]
#[ignore = "root-coordinated same-child IPHelper/CIM diagnosis; not mapped qualification"]
fn diagnose_same_child_iphelper_and_cim_representations() {
    #[derive(Serialize)]
    struct Metadata<'a> {
        schema: u32,
        pid: u32,
        ports: Vec<u16>,
        nonce: &'a str,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Report {
        schema: u32,
        diagnostic_error: Option<&'static str>,
        original_rechecked_before_after: bool,
        child_reaped: bool,
        readers_joined: bool,
        normal_stop: bool,
        controlled_ports_rebound: bool,
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("diagnostic_worktree_root");
    let directory = root.join(".aurora/connect-validation").join(format!(
        "native-voice-85a-backend-diagnostic-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&directory).expect("diagnostic_new_directory");
    let nonce = uuid::Uuid::new_v4().to_string();
    let mut fixture = Fixture::new();
    let proof = fixture.proof("loopback-backend-diagnostic");
    let metadata_path = directory.join("metadata.json");
    let outcome = (|| -> Result<(), &'static str> {
        let before = diagnostic_snapshot(&proof, &fixture.endpoints)
            .map_err(|_| "diagnostic_before_observation")?;
        diagnostic_write_new(&directory.join("iphelper-before.json"), &before)?;
        diagnostic_write_new(
            &metadata_path,
            &Metadata {
                schema: 1,
                pid: proof.fingerprint().0,
                ports: fixture.endpoints.iter().map(SocketAddr::port).collect(),
                nonce: &nonce,
            },
        )?;
        // Only a private path is emitted. PID/ports remain in local metadata
        // for root's fixed, read-only CIM query against this very same Child.
        println!(
            "AURORA_UDP_BACKEND_DIAGNOSTIC_METADATA {}",
            metadata_path.display()
        );
        std::io::stdout().flush().map_err(|_| "diagnostic_flush")?;
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            match std::fs::File::open(directory.join("finish.txt")) {
                Ok(file) => {
                    let mut content = Vec::new();
                    file.take(129)
                        .read_to_end(&mut content)
                        .map_err(|_| "diagnostic_finish_read")?;
                    if content.len() > 128 || content != format!("{nonce}\n").as_bytes() {
                        return Err("diagnostic_finish_invalid");
                    }
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => return Err("diagnostic_finish_open"),
            }
            if Instant::now() >= deadline {
                return Err("diagnostic_coordination_deadline");
            }
            thread::sleep(Duration::from_millis(20));
        }
        // finish coordinates timing only. No external result or file can
        // manufacture a proof, alter the API verdict, or qualify mapped.
        let after = diagnostic_snapshot(&proof, &fixture.endpoints)
            .map_err(|_| "diagnostic_after_observation")?;
        diagnostic_write_new(&directory.join("iphelper-after.json"), &after)?;
        Ok(())
    })();
    // Always stop/reap/join/rebind the same owned fixture before final verdict.
    // Its immediate RAII owner also covers a panic during the diagnostic.
    fixture.stop();
    let report = Report {
        schema: 1,
        diagnostic_error: outcome.err(),
        original_rechecked_before_after: outcome.is_ok(),
        child_reaped: matches!(fixture.child.try_wait(), Ok(Some(_))),
        readers_joined: fixture.reader_done.load(Ordering::Acquire),
        normal_stop: !fixture.fallback_used.load(Ordering::Acquire),
        controlled_ports_rebound: true, // fixture.stop just proved every rebind.
    };
    diagnostic_write_new(&directory.join("result.json"), &report).expect("diagnostic_result_saved");
    assert!(outcome.is_ok(), "same_child_backend_diagnostic_failed");
    assert_eq!(proof.recheck(), Err(DiscoveryError::ProcessChanged));
}
