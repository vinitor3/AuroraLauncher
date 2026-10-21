use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

const MARKER: &str = "AURORA_OWNED_RUN_TEST_CHILD";
const CONTROL: &str = "AURORA_OWNED_RUN_TEST_CONTROL";
const ENDPOINTS: &str = "AURORA_OWNED_RUN_TEST_ENDPOINTS";
const MODE: &str = "AURORA_OWNED_RUN_TEST_MODE";
const CHILD_TEST: &str = "engine::connect_owned_run::tests::fixture::owned_run_child_fixture";
const MAX_LINE: usize = 2048;
const MAX_COMMANDS: usize = 16;
const CHILD_LIMIT: Duration = Duration::from_secs(20);
pub(super) const WAIT: Duration = Duration::from_secs(8);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Bindings {
    pub tcp4: SocketAddr,
    pub tcp6: SocketAddr,
    pub udp4: SocketAddr,
    pub udp6: SocketAddr,
}

impl Default for Bindings {
    fn default() -> Self {
        Self {
            tcp4: (Ipv4Addr::LOCALHOST, 0).into(),
            tcp6: (Ipv6Addr::LOCALHOST, 0).into(),
            udp4: (Ipv4Addr::LOCALHOST, 0).into(),
            udp6: (Ipv6Addr::LOCALHOST, 0).into(),
        }
    }
}

impl Bindings {
    pub(super) fn udp(&self) -> [SocketAddr; 2] {
        [self.udp4, self.udp6]
    }

    pub(super) fn assert_released(&self) {
        let tcp4 = TcpListener::bind(self.tcp4).expect("owned_fixture_tcp4_released");
        let tcp6 = TcpListener::bind(self.tcp6).expect("owned_fixture_tcp6_released");
        let udp4 = UdpSocket::bind(self.udp4).expect("owned_fixture_udp4_released");
        let udp6 = UdpSocket::bind(self.udp6).expect("owned_fixture_udp6_released");
        drop((tcp4, tcp6, udp4, udp6));
    }

    pub(super) fn exchange_bytes(&self) {
        let payload: Vec<u8> = (0..3072).map(|index| (index % 251) as u8).collect();
        for endpoint in [self.tcp4, self.tcp6] {
            let mut stream =
                TcpStream::connect_timeout(&endpoint, WAIT).expect("owned_fixture_tcp_connect");
            stream.set_read_timeout(Some(WAIT)).expect("tcp_timeout");
            stream.set_write_timeout(Some(WAIT)).expect("tcp_timeout");
            stream.write_all(&payload).expect("owned_fixture_tcp_write");
            let mut received = vec![0; payload.len()];
            stream
                .read_exact(&mut received)
                .expect("owned_fixture_tcp_read");
            assert_eq!(received, payload, "owned_fixture_tcp_exact_bytes");
            // The parent is the active closer, keeping server listener rebinding
            // independent of the parent's ephemeral TIME_WAIT connection.
            stream
                .shutdown(Shutdown::Both)
                .expect("owned_fixture_tcp_close");
        }
        for endpoint in self.udp() {
            let local = if endpoint.is_ipv4() {
                SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
            } else {
                SocketAddr::from((Ipv6Addr::LOCALHOST, 0))
            };
            let socket = UdpSocket::bind(local).expect("owned_fixture_udp_client");
            socket.set_read_timeout(Some(WAIT)).expect("udp_timeout");
            socket
                .send_to(&payload[..1264], endpoint)
                .expect("udp_send");
            let mut bytes = [0; 4096];
            let (length, source) = socket.recv_from(&mut bytes).expect("udp_receive");
            assert_eq!(source, endpoint, "owned_fixture_udp_source");
            assert_eq!(
                &bytes[..length],
                &payload[..1264],
                "owned_fixture_udp_exact_bytes"
            );
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Action {
    ReleaseStop,
    ExitZero,
    ExitSeven,
    FloodOut,
    FloodErr,
    FloodBoth,
    ReleaseUdp,
    RebindUdp,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase", deny_unknown_fields)]
enum Event {
    Ready { nonce: String, bindings: Bindings },
    StopObserved { lines: usize, eof: bool },
    UdpReleased,
    UdpRebound,
}

fn line(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, &'static str> {
    let mut bytes = Vec::new();
    let count = reader
        .take((MAX_LINE + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "fixture_read")?;
    if count == 0 {
        return Ok(None);
    }
    if count > MAX_LINE || bytes.last() != Some(&b'\n') {
        return Err("fixture_line_limit");
    }
    Ok(Some(bytes))
}

fn write_message(stream: &mut TcpStream, value: &impl Serialize) -> Result<(), &'static str> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| "fixture_encode")?;
    bytes.push(b'\n');
    if bytes.len() > MAX_LINE {
        return Err("fixture_message_limit");
    }
    stream.write_all(&bytes).map_err(|_| "fixture_write")
}

pub(super) struct Harness {
    listener: TcpListener,
    nonce: String,
    wanted: Bindings,
    control: Option<BufReader<TcpStream>>,
    pub bindings: Option<Bindings>,
}

impl Harness {
    pub(super) fn new() -> Self {
        Self::at(Bindings::default())
    }

    pub(super) fn at(wanted: Bindings) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("fixture_control_bind");
        listener
            .set_nonblocking(true)
            .expect("fixture_control_nonblocking");
        Self {
            listener,
            nonce: uuid::Uuid::new_v4().to_string(),
            wanted,
            control: None,
            bindings: None,
        }
    }

    pub(super) fn command(&self, mode: &str) -> Command {
        assert!(matches!(mode, "normal" | "hold-stop" | "ignore-stop"));
        let mut command = Command::new(std::env::current_exe().expect("fixture_executable"));
        command
            .args([
                "--ignored",
                "--exact",
                CHILD_TEST,
                "--nocapture",
                "--test-threads=1",
            ])
            .env(MARKER, &self.nonce)
            .env(
                CONTROL,
                self.listener
                    .local_addr()
                    .expect("control_address")
                    .to_string(),
            )
            .env(
                ENDPOINTS,
                serde_json::to_string(&self.wanted).expect("fixture_endpoints"),
            )
            .env(MODE, mode);
        command
    }

    pub(super) fn ready(&mut self) -> Bindings {
        let deadline = Instant::now() + WAIT;
        let stream = loop {
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    assert!(peer.ip().is_loopback(), "fixture_control_loopback");
                    break stream;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "fixture_ready_deadline");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => panic!("fixture_control_accept"),
            }
        };
        stream
            .set_nonblocking(false)
            .expect("control_blocking_stream");
        stream
            .set_read_timeout(Some(WAIT))
            .expect("control_read_timeout");
        stream
            .set_write_timeout(Some(WAIT))
            .expect("control_write_timeout");
        self.control = Some(BufReader::new(stream));
        let Event::Ready { nonce, bindings } = self.event() else {
            panic!("fixture_ready_event");
        };
        assert_eq!(nonce, self.nonce, "fixture_control_identity");
        for (actual, wanted) in [
            (bindings.tcp4, self.wanted.tcp4),
            (bindings.tcp6, self.wanted.tcp6),
            (bindings.udp4, self.wanted.udp4),
            (bindings.udp6, self.wanted.udp6),
        ] {
            assert_eq!(actual.ip(), wanted.ip(), "fixture_literal_address");
            assert_ne!(actual.port(), 0, "fixture_nonzero_port");
            if wanted.port() != 0 {
                assert_eq!(actual.port(), wanted.port(), "fixture_exact_rebind");
            }
        }
        self.bindings = Some(bindings.clone());
        bindings
    }

    pub(super) fn assert_no_child_connection(&self) {
        assert!(
            matches!(self.listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
            "fixture_no_child_connected"
        );
    }

    fn event(&mut self) -> Event {
        let bytes = line(self.control.as_mut().expect("fixture_control"))
            .expect("fixture_control_line")
            .expect("fixture_control_event");
        serde_json::from_slice(&bytes).expect("fixture_control_protocol")
    }

    pub(super) fn send(&mut self, action: Action) {
        write_message(
            self.control.as_mut().expect("fixture_control").get_mut(),
            &action,
        )
        .expect("fixture_action");
    }

    pub(super) fn stop_observed(&mut self) {
        let Event::StopObserved { lines, eof } = self.event() else {
            panic!("fixture_stop_event");
        };
        assert_eq!(lines, 1, "exactly_one_stop_line");
        assert!(eof, "stdin_closed_after_stop");
    }

    pub(super) fn release_udp(&mut self) {
        self.send(Action::ReleaseUdp);
        assert!(matches!(self.event(), Event::UdpReleased));
    }

    pub(super) fn rebind_udp(&mut self) {
        self.send(Action::RebindUdp);
        assert!(matches!(self.event(), Event::UdpRebound));
    }

    pub(super) fn finish(self) {
        let address = self.listener.local_addr().expect("fixture_control_address");
        drop(self);
        drop(TcpListener::bind(address).expect("fixture_control_released"));
    }
}

struct Echoes {
    stop: Arc<AtomicBool>,
    workers: Vec<thread::JoinHandle<()>>,
}
impl Echoes {
    fn new() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            workers: Vec::new(),
        }
    }
    fn accept(&mut self, listener: &TcpListener) -> Result<(), &'static str> {
        for _ in 0..4 {
            let mut stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => return Err("child_accept"),
            };
            if self.workers.len() >= 16 {
                return Err("child_connection_limit");
            }
            stream
                .set_nonblocking(false)
                .map_err(|_| "child_tcp_blocking")?;
            stream
                .set_read_timeout(Some(Duration::from_millis(20)))
                .map_err(|_| "child_timeout")?;
            stream
                .set_write_timeout(Some(Duration::from_millis(100)))
                .map_err(|_| "child_timeout")?;
            let stop = self.stop.clone();
            self.workers.push(thread::spawn(move || {
                let mut buffer = [0; 4096];
                let mut total = 0;
                while !stop.load(Ordering::Acquire) {
                    match stream.read(&mut buffer) {
                        Ok(0) => return,
                        Ok(count) => {
                            total += count;
                            if total > 64 * 1024 || stream.write_all(&buffer[..count]).is_err() {
                                return;
                            }
                        }
                        Err(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) => {}
                        Err(_) => return,
                    }
                }
            }));
        }
        Ok(())
    }
}
impl Drop for Echoes {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn echo_udp(socket: &UdpSocket) -> Result<(), &'static str> {
    let mut buffer = [0; 4096];
    for _ in 0..16 {
        match socket.recv_from(&mut buffer) {
            Ok((length, peer)) => {
                socket
                    .send_to(&buffer[..length], peer)
                    .map_err(|_| "child_udp_send")?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(_) => return Err("child_udp_receive"),
        }
    }
    Ok(())
}

fn bind_udp(bindings: &Bindings) -> Result<(UdpSocket, UdpSocket), &'static str> {
    let one = UdpSocket::bind(bindings.udp4).map_err(|_| "child_udp4_bind")?;
    let two = UdpSocket::bind(bindings.udp6).map_err(|_| "child_udp6_bind")?;
    one.set_nonblocking(true)
        .map_err(|_| "child_udp_nonblocking")?;
    two.set_nonblocking(true)
        .map_err(|_| "child_udp_nonblocking")?;
    Ok((one, two))
}

fn child_loop(nonce: String) -> Result<(), &'static str> {
    let requested: Bindings =
        serde_json::from_str(&std::env::var(ENDPOINTS).map_err(|_| "child_endpoints")?)
            .map_err(|_| "child_endpoints")?;
    for (endpoint, expected) in [
        (requested.tcp4, SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
        (requested.tcp6, SocketAddr::from((Ipv6Addr::LOCALHOST, 0))),
        (requested.udp4, SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
        (requested.udp6, SocketAddr::from((Ipv6Addr::LOCALHOST, 0))),
    ] {
        if endpoint.ip() != expected.ip()
            || matches!(endpoint, SocketAddr::V6(v) if v.scope_id() != 0 || v.flowinfo() != 0)
        {
            return Err("child_literal_endpoint");
        }
    }
    let mode = std::env::var(MODE).map_err(|_| "child_mode")?;
    if !matches!(mode.as_str(), "normal" | "hold-stop" | "ignore-stop") {
        return Err("child_mode");
    }
    let tcp4 = TcpListener::bind(requested.tcp4).map_err(|_| "child_tcp4_bind")?;
    let tcp6 = TcpListener::bind(requested.tcp6).map_err(|_| "child_tcp6_bind")?;
    tcp4.set_nonblocking(true)
        .map_err(|_| "child_tcp_nonblocking")?;
    tcp6.set_nonblocking(true)
        .map_err(|_| "child_tcp_nonblocking")?;
    let mut udp = Some(bind_udp(&requested)?);
    let bindings = Bindings {
        tcp4: tcp4.local_addr().map_err(|_| "child_tcp4_address")?,
        tcp6: tcp6.local_addr().map_err(|_| "child_tcp6_address")?,
        udp4: udp
            .as_ref()
            .expect("child_udp")
            .0
            .local_addr()
            .map_err(|_| "child_udp4_address")?,
        udp6: udp
            .as_ref()
            .expect("child_udp")
            .1
            .local_addr()
            .map_err(|_| "child_udp6_address")?,
    };
    let address: SocketAddr = std::env::var(CONTROL)
        .map_err(|_| "child_control")?
        .parse()
        .map_err(|_| "child_control")?;
    if !address.ip().is_loopback() || address.port() == 0 {
        return Err("child_control");
    }
    let mut control =
        TcpStream::connect_timeout(&address, WAIT).map_err(|_| "child_control_connect")?;
    control
        .set_write_timeout(Some(WAIT))
        .map_err(|_| "child_control_timeout")?;
    write_message(
        &mut control,
        &Event::Ready {
            nonce,
            bindings: bindings.clone(),
        },
    )?;
    control
        .set_nonblocking(true)
        .map_err(|_| "child_control_nonblocking")?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let ignore_eof = mode == "ignore-stop";
    let stdin_reader = thread::spawn(move || {
        let mut input = BufReader::new(std::io::stdin().lock());
        let result = (|| {
            let Some(first) = line(&mut input)? else {
                return if ignore_eof {
                    Ok(false)
                } else {
                    Err("child_missing_stop")
                };
            };
            if first != b"stop\n" {
                return Err("child_unexpected_stdin");
            }
            if line(&mut input)?.is_some() {
                return Err("child_duplicate_stdin");
            }
            Ok(true)
        })();
        let _ = sender.try_send(result);
    });
    let mut echoes = Echoes::new();
    let mut pending = Vec::new();
    let mut commands = 0;
    let mut stopped = false;
    let mut released = false;
    let deadline = Instant::now() + CHILD_LIMIT;
    loop {
        if Instant::now() >= deadline {
            std::process::exit(19);
        }
        if !stopped {
            match receiver.try_recv() {
                Ok(result) => {
                    let observed_stop = result?;
                    stopped = true;
                    if observed_stop {
                        write_message(
                            &mut control,
                            &Event::StopObserved {
                                lines: 1,
                                eof: true,
                            },
                        )?;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(_) => return Err("child_stdin_worker"),
            }
        }
        if stopped && (mode == "normal" || (mode == "hold-stop" && released)) {
            break;
        }
        let mut incoming = [0; 256];
        match control.read(&mut incoming) {
            Ok(0) => std::process::exit(17),
            Ok(count) => {
                pending.extend_from_slice(&incoming[..count]);
                if pending.len() > MAX_LINE {
                    return Err("child_control_limit");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return Err("child_control_read"),
        }
        while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            commands += 1;
            if commands > MAX_COMMANDS {
                return Err("child_command_limit");
            }
            let encoded: Vec<_> = pending.drain(..=end).collect();
            let action: Action =
                serde_json::from_slice(&encoded).map_err(|_| "child_command_invalid")?;
            match action {
                Action::ReleaseStop => released = true,
                Action::ExitZero => std::process::exit(0),
                Action::ExitSeven => std::process::exit(7),
                Action::FloodOut => std::io::stdout()
                    .write_all(&[b'x'; 80 * 1024])
                    .map_err(|_| "child_flood")?,
                Action::FloodErr => std::io::stderr()
                    .write_all(&[b'y'; 80 * 1024])
                    .map_err(|_| "child_flood")?,
                Action::FloodBoth => {
                    std::io::stdout()
                        .write_all(&[b'x'; 40 * 1024])
                        .map_err(|_| "child_flood")?;
                    std::io::stderr()
                        .write_all(&[b'y'; 40 * 1024])
                        .map_err(|_| "child_flood")?;
                }
                Action::ReleaseUdp if udp.is_some() => {
                    drop(udp.take());
                    write_message(&mut control, &Event::UdpReleased)?;
                }
                Action::RebindUdp if udp.is_none() => {
                    udp = Some(bind_udp(&bindings)?);
                    write_message(&mut control, &Event::UdpRebound)?;
                }
                _ => return Err("child_command_state"),
            }
        }
        echoes.accept(&tcp4)?;
        echoes.accept(&tcp6)?;
        if let Some((one, two)) = &udp {
            echo_udp(one)?;
            echo_udp(two)?;
        }
        thread::sleep(Duration::from_millis(2));
    }
    drop((tcp4, tcp6, udp));
    drop(echoes);
    stdin_reader.join().map_err(|_| "child_stdin_join")?;
    Ok(())
}

#[test]
#[ignore = "private child fixture invoked by the owned-run parent tests"]
fn owned_run_child_fixture() {
    let Ok(nonce) = std::env::var(MARKER) else {
        return;
    };
    if uuid::Uuid::parse_str(&nonce).is_err() {
        panic!("child_marker_invalid");
    }
    child_loop(nonce).expect("owned_run_child_fixture_protocol");
}
