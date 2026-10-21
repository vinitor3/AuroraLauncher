//! A real, bounded child process implementing only the synthetic STATUS
//! exchange used by the producer tests. This is not a Java/Minecraft fixture.
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

const NONCE: &str = "AURORA_DEDICATED_TEST_NONCE";
const CONTROL: &str = "AURORA_DEDICATED_TEST_CONTROL";
const MODE: &str = "AURORA_DEDICATED_TEST_MODE";
const MAX_LINE: usize = 1024;
const MAX_COMMANDS: usize = 16;
const MAX_SESSIONS: usize = 32;
const CHILD_LIMIT: Duration = Duration::from_secs(25);
pub(super) const WAIT: Duration = Duration::from_secs(8);
pub(super) const ENDPOINT: SocketAddr =
    SocketAddr::V4(std::net::SocketAddrV4::new(Ipv4Addr::LOCALHOST, 43001));

#[derive(Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Action {
    ReleaseStop,
    ExitZero,
    ExitSeven,
    FloodOut,
    FloodErr,
    FloodBoth,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase", deny_unknown_fields)]
enum Event {
    Ready { nonce: String, endpoint: SocketAddr },
    StatusObserved { pong_sent: bool },
    StopObserved { lines: usize, eof: bool },
}

fn line(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, &'static str> {
    let mut bytes = Vec::new();
    let count = reader
        .take((MAX_LINE + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "status_fixture_read")?;
    if count == 0 {
        return Ok(None);
    }
    if count > MAX_LINE || bytes.last() != Some(&b'\n') {
        return Err("status_fixture_line_limit");
    }
    Ok(Some(bytes))
}

fn message(stream: &mut TcpStream, value: &impl Serialize) -> Result<(), &'static str> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| "status_fixture_encode")?;
    bytes.push(b'\n');
    if bytes.len() > MAX_LINE {
        return Err("status_fixture_message_limit");
    }
    stream.write_all(&bytes).map_err(|_| "status_fixture_write")
}

/// The producer's closed cfg(test) recipe receives only these fixture values;
/// it chooses the test executable and fixed argv itself.
pub(super) struct Harness {
    listener: TcpListener,
    nonce: String,
    control: Option<BufReader<TcpStream>>,
    statuses: usize,
    pongs: usize,
    stopped: bool,
}

impl Harness {
    pub(super) fn new() -> Self {
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("status_fixture_control_bind");
        listener
            .set_nonblocking(true)
            .expect("status_fixture_listener_nonblocking");
        Self {
            listener,
            nonce: uuid::Uuid::new_v4().to_string(),
            control: None,
            statuses: 0,
            pongs: 0,
            stopped: false,
        }
    }

    pub(super) fn control_addr(&self) -> SocketAddr {
        self.listener
            .local_addr()
            .expect("status_fixture_control_address")
    }

    pub(super) fn nonce(&self) -> &str {
        &self.nonce
    }

    pub(super) fn ready(&mut self) {
        let deadline = Instant::now() + WAIT;
        let stream = loop {
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    assert!(peer.ip().is_loopback(), "status_fixture_control_loopback");
                    break stream;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "status_fixture_ready_deadline");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => panic!("status_fixture_control_accept"),
            }
        };
        // Windows may inherit nonblocking mode from the accepting listener.
        stream
            .set_nonblocking(false)
            .expect("status_fixture_control_blocking");
        stream
            .set_read_timeout(Some(WAIT))
            .expect("status_fixture_read_timeout");
        stream
            .set_write_timeout(Some(WAIT))
            .expect("status_fixture_write_timeout");
        self.control = Some(BufReader::new(stream));
        let Event::Ready { nonce, endpoint } = self.event() else {
            panic!("status_fixture_ready_event");
        };
        assert_eq!(nonce, self.nonce, "status_fixture_nonce");
        assert_eq!(endpoint, ENDPOINT, "status_fixture_exact_literal");
    }

    fn event(&mut self) -> Event {
        let bytes = line(self.control.as_mut().expect("status_fixture_control"))
            .expect("status_fixture_event_line")
            .expect("status_fixture_event_present");
        serde_json::from_slice(&bytes).expect("status_fixture_event_schema")
    }

    fn observe(&mut self, event: Event) {
        match event {
            Event::StatusObserved { pong_sent } => {
                self.statuses += 1;
                self.pongs += usize::from(pong_sent);
                assert!(
                    self.statuses <= MAX_SESSIONS * 2,
                    "status_fixture_bounded_events"
                );
            }
            Event::StopObserved { lines, eof } => {
                assert_eq!(lines, 1, "status_fixture_one_stop_line");
                assert!(eof, "status_fixture_closed_stdin");
                assert!(!self.stopped, "status_fixture_stop_once");
                self.stopped = true;
            }
            Event::Ready { .. } => panic!("status_fixture_duplicate_ready"),
        }
    }

    pub(super) fn status_observed(&mut self, require_pong: bool) {
        while self.statuses == 0 || (require_pong && self.pongs == 0) {
            let event = self.event();
            self.observe(event);
        }
    }

    pub(super) fn status_counts(&self) -> (usize, usize) {
        // Every validated request emits false once; only a written pong emits
        // the additional true event. Do not count a pong as a second request.
        (self.statuses - self.pongs, self.pongs)
    }

    /// Call only after the original completion has reaped the Child. Consume
    /// every remaining bounded notice through EOF so an unexpected additional
    /// request cannot be hidden by returning after the first expected count.
    pub(super) fn status_counts_after_completion(&mut self) -> (usize, usize) {
        loop {
            let bytes = line(self.control.as_mut().expect("status_fixture_control"))
                .expect("status_fixture_completed_event_line");
            let Some(bytes) = bytes else {
                break;
            };
            let event =
                serde_json::from_slice(&bytes).expect("status_fixture_completed_event_schema");
            self.observe(event);
        }
        self.status_counts()
    }

    pub(super) fn stop_observed(&mut self) {
        while !self.stopped {
            let event = self.event();
            self.observe(event);
        }
    }

    pub(super) fn send(&mut self, action: Action) {
        message(
            self.control
                .as_mut()
                .expect("status_fixture_control")
                .get_mut(),
            &action,
        )
        .expect("status_fixture_send_action");
    }

    pub(super) fn assert_no_child(&self) {
        assert!(
            matches!(self.listener.accept(), Err(error)
            if error.kind() == std::io::ErrorKind::WouldBlock),
            "status_fixture_zero_child"
        );
    }

    pub(super) fn assert_status_port_released() {
        drop(TcpListener::bind(ENDPOINT).expect("status_fixture_listener_released"));
    }

    pub(super) fn finish(self) {
        let address = self.control_addr();
        drop(self);
        drop(TcpListener::bind(address).expect("status_fixture_control_released"));
    }
}

fn exact(
    stream: &mut TcpStream,
    bytes: &mut [u8],
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<(), &'static str> {
    let mut offset = 0;
    while offset < bytes.len() {
        if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err("status_fixture_session_cancelled");
        }
        match stream.read(&mut bytes[offset..]) {
            Ok(0) => return Err("status_fixture_session_eof"),
            Ok(count) => offset += count,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Err("status_fixture_session_read"),
        }
    }
    Ok(())
}

fn frame(
    stream: &mut TcpStream,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<Vec<u8>, &'static str> {
    let mut length = 0u32;
    let mut ended = false;
    for index in 0..3 {
        let mut byte = [0];
        exact(stream, &mut byte, stop, deadline)?;
        length |= u32::from(byte[0] & 127) << (index * 7);
        if byte[0] & 128 == 0 {
            ended = true;
            break;
        }
    }
    if !ended || length == 0 || length > 1024 {
        return Err("status_fixture_frame_limit");
    }
    let mut body = vec![0; length as usize];
    exact(stream, &mut body, stop, deadline)?;
    Ok(body)
}

fn take_varint(bytes: &mut &[u8]) -> Result<u32, &'static str> {
    let mut value = 0u32;
    for index in 0..5 {
        let Some((&byte, rest)) = bytes.split_first() else {
            return Err("status_fixture_varint");
        };
        *bytes = rest;
        if index == 4 && byte & 0xf0 != 0 {
            return Err("status_fixture_varint");
        }
        value |= u32::from(byte & 127) << (index * 7);
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    Err("status_fixture_varint")
}

fn put_varint(bytes: &mut Vec<u8>, mut value: usize) {
    loop {
        let byte = (value & 127) as u8;
        value >>= 7;
        bytes.push(byte | if value == 0 { 0 } else { 128 });
        if value == 0 {
            break;
        }
    }
}

fn write_frame(stream: &mut TcpStream, body: &[u8]) -> Result<(), &'static str> {
    let mut output = Vec::with_capacity(body.len() + 3);
    put_varint(&mut output, body.len());
    output.extend_from_slice(body);
    stream
        .write_all(&output)
        .map_err(|_| "status_fixture_packet_write")
}

struct Notice {
    event: Event,
    forwarded: mpsc::SyncSender<()>,
}

fn notice(
    events: &mpsc::SyncSender<Notice>,
    event: Event,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<(), &'static str> {
    let (forwarded, observed) = mpsc::sync_channel(1);
    events
        .try_send(Notice { event, forwarded })
        .map_err(|_| "status_fixture_events_limit")?;
    loop {
        if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err("status_fixture_event_cancelled");
        }
        match observed.recv_timeout(Duration::from_millis(20)) {
            Ok(()) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err("status_fixture_event_disconnected"),
        }
    }
}

fn status_session(
    mut stream: TcpStream,
    mode: String,
    stop: Arc<AtomicBool>,
    events: mpsc::SyncSender<Notice>,
    first_status: Arc<AtomicBool>,
) -> Result<(), &'static str> {
    stream
        .set_nonblocking(false)
        .map_err(|_| "status_fixture_session_blocking")?;
    stream
        .set_read_timeout(Some(Duration::from_millis(20)))
        .map_err(|_| "status_fixture_session_timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_millis(100)))
        .map_err(|_| "status_fixture_session_timeout")?;
    let deadline = Instant::now() + Duration::from_secs(6);
    let handshake = frame(&mut stream, &stop, deadline)?;
    let mut input = handshake.as_slice();
    if take_varint(&mut input)? != 0 || take_varint(&mut input)? != 767 {
        return Err("status_fixture_handshake_protocol");
    }
    let host_length = take_varint(&mut input)? as usize;
    if host_length != 9 || input.len() < host_length + 3 || &input[..host_length] != b"127.0.0.1" {
        return Err("status_fixture_handshake_host");
    }
    input = &input[host_length..];
    if u16::from_be_bytes([input[0], input[1]]) != 43001 {
        return Err("status_fixture_handshake_port");
    }
    input = &input[2..];
    if take_varint(&mut input)? != 1
        || !input.is_empty()
        || frame(&mut stream, &stop, deadline)? != [0]
    {
        return Err("status_fixture_status_request");
    }
    // The parent receives proof of the actual request before any deliberately
    // invalid response can make the producer terminate this child. The ACK is
    // internal and bounded; it changes no production protocol or authority.
    notice(
        &events,
        Event::StatusObserved { pong_sent: false },
        &stop,
        deadline,
    )?;
    if mode == "truncate-every-status"
        || (mode == "truncate-first-status" && !first_status.swap(true, Ordering::AcqRel))
    {
        // Each selected valid request reaches the parent before EOF; no
        // response bytes are sent. In first-only mode, subsequent sessions
        // retain the ordinary protocol767/pong and same listener/process.
        stream
            .shutdown(Shutdown::Both)
            .map_err(|_| "status_fixture_truncate_first")?;
        return Ok(());
    }
    if mode == "status-timeout" {
        while !stop.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        return Ok(());
    }
    let json = serde_json::to_vec(&serde_json::json!({
        "version": {"name": "Synthetic STATUS fixture", "protocol": if mode == "bad-protocol" { 766 } else { 767 }},
        "players": {"max": 2, "online": 0},
        "description": "Synthetic test; not Minecraft"
    })).map_err(|_| "status_fixture_json")?;
    let mut response = vec![0];
    put_varint(&mut response, json.len());
    response.extend_from_slice(&json);
    write_frame(&mut stream, &response)?;
    let mut ping = frame(&mut stream, &stop, deadline)?;
    if ping.len() != 9 || ping[0] != 1 {
        return Err("status_fixture_ping");
    }
    if mode == "bad-pong" {
        ping[8] ^= 1;
    }
    write_frame(&mut stream, &ping)?;
    notice(
        &events,
        Event::StatusObserved { pong_sent: true },
        &stop,
        deadline,
    )?;
    // The producer actively closes; avoid making server port43001 depend on
    // server-side TIME_WAIT when the fixture listener is later rebound.
    let mut byte = [0];
    while !stop.load(Ordering::Acquire) && Instant::now() < deadline {
        match stream.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => return Err("status_fixture_unexpected_payload"),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
    Ok(())
}

struct Sessions {
    stop: Arc<AtomicBool>,
    workers: Vec<thread::JoinHandle<Result<(), &'static str>>>,
}
impl Drop for Sessions {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn child_loop(nonce: String) -> Result<(), &'static str> {
    let mode = std::env::var(MODE).map_err(|_| "status_fixture_mode")?;
    if !matches!(
        mode.as_str(),
        "normal"
            | "hold-stop"
            | "ignore-stop"
            | "bad-protocol"
            | "bad-pong"
            | "status-timeout"
            | "truncate-first-status"
            | "truncate-every-status"
    ) {
        return Err("status_fixture_mode");
    }
    let address: SocketAddr = std::env::var(CONTROL)
        .map_err(|_| "status_fixture_control_env")?
        .parse()
        .map_err(|_| "status_fixture_control_address")?;
    if address.ip() != Ipv4Addr::LOCALHOST || address.port() == 0 {
        return Err("status_fixture_control_literal");
    }
    let listener = TcpListener::bind(ENDPOINT).map_err(|_| "status_fixture_status_bind")?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "status_fixture_status_nonblocking")?;
    let mut control =
        TcpStream::connect_timeout(&address, WAIT).map_err(|_| "status_fixture_control_connect")?;
    control
        .set_write_timeout(Some(WAIT))
        .map_err(|_| "status_fixture_control_timeout")?;
    message(
        &mut control,
        &Event::Ready {
            nonce,
            endpoint: listener
                .local_addr()
                .map_err(|_| "status_fixture_address")?,
        },
    )?;
    control
        .set_nonblocking(true)
        .map_err(|_| "status_fixture_control_nonblocking")?;
    let (stdin_send, stdin_recv) = mpsc::sync_channel(1);
    let stdin_reader = thread::spawn(move || {
        let result = (|| {
            let mut reader = BufReader::new(std::io::stdin().lock());
            let Some(first) = line(&mut reader)? else {
                return Ok(false);
            };
            if first != b"stop\n" || line(&mut reader)?.is_some() {
                return Err("status_fixture_stop_protocol");
            }
            Ok(true)
        })();
        let _ = stdin_send.try_send(result);
    });
    let (event_send, event_recv) = mpsc::sync_channel::<Notice>(MAX_SESSIONS);
    let mut sessions = Sessions {
        stop: Arc::new(AtomicBool::new(false)),
        workers: Vec::new(),
    };
    let first_status = Arc::new(AtomicBool::new(false));
    let mut stopped = false;
    let mut stdin_ended = false;
    let mut released = false;
    let mut pending = Vec::new();
    let mut commands = 0;
    let deadline = Instant::now() + CHILD_LIMIT;
    loop {
        if Instant::now() >= deadline {
            std::process::exit(19);
        }
        if !stdin_ended {
            match stdin_recv.try_recv() {
                Ok(result) => {
                    stdin_ended = true;
                    stopped = result?;
                    if stopped {
                        message(
                            &mut control,
                            &Event::StopObserved {
                                lines: 1,
                                eof: true,
                            },
                        )?;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(_) => return Err("status_fixture_stdin_worker"),
            }
        }
        if stdin_ended && mode != "ignore-stop" && (mode != "hold-stop" || released) {
            break;
        }
        while let Ok(notice) = event_recv.try_recv() {
            message(&mut control, &notice.event)?;
            let _ = notice.forwarded.try_send(());
        }
        let mut incoming = [0; 256];
        match control.read(&mut incoming) {
            Ok(0) => std::process::exit(17),
            Ok(count) => pending.extend_from_slice(&incoming[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return Err("status_fixture_control_read"),
        }
        if pending.len() > MAX_LINE {
            return Err("status_fixture_control_limit");
        }
        while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            commands += 1;
            if commands > MAX_COMMANDS {
                return Err("status_fixture_commands_limit");
            }
            let encoded: Vec<_> = pending.drain(..=end).collect();
            let action: Action =
                serde_json::from_slice(&encoded).map_err(|_| "status_fixture_action_schema")?;
            match action {
                Action::ReleaseStop => released = true,
                Action::ExitZero => std::process::exit(0),
                Action::ExitSeven => std::process::exit(7),
                Action::FloodOut => std::io::stdout()
                    .write_all(&vec![b'x'; 4 * 1024 * 1024 + 1])
                    .map_err(|_| "status_fixture_flood")?,
                Action::FloodErr => std::io::stderr()
                    .write_all(&vec![b'y'; 4 * 1024 * 1024 + 1])
                    .map_err(|_| "status_fixture_flood")?,
                Action::FloodBoth => {
                    std::io::stdout()
                        .write_all(&vec![b'x'; 2 * 1024 * 1024 + 1])
                        .map_err(|_| "status_fixture_flood")?;
                    std::io::stderr()
                        .write_all(&vec![b'y'; 2 * 1024 * 1024 + 1])
                        .map_err(|_| "status_fixture_flood")?;
                }
            }
        }
        if !stopped {
            for _ in 0..2 {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        if !peer.ip().is_loopback() || sessions.workers.len() >= MAX_SESSIONS {
                            return Err("status_fixture_sessions_limit");
                        }
                        let stop = sessions.stop.clone();
                        let events = event_send.clone();
                        let mode = mode.clone();
                        let first_status = first_status.clone();
                        sessions.workers.push(thread::spawn(move || {
                            status_session(stream, mode, stop, events, first_status)
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => return Err("status_fixture_accept"),
                }
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    drop(listener);
    drop(sessions);
    stdin_reader
        .join()
        .map_err(|_| "status_fixture_stdin_join")?;
    Ok(())
}

#[test]
#[ignore = "private STATUS child fixture invoked by native dedicated parent tests"]
fn dedicated_status_child_fixture() {
    let Ok(nonce) = std::env::var(NONCE) else {
        return;
    };
    assert!(
        uuid::Uuid::parse_str(&nonce).is_ok(),
        "status_fixture_nonce_format"
    );
    child_loop(nonce).expect("dedicated_status_child_fixture_protocol");
}
