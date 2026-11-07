//! Own bounded test executable: real STATUS767, TCP/UDP sockets and a writer
//! with fixed profile paths. It reuses the legacy STATUS codec without changing
//! that codec's body, argv or control protocol. No Java/Minecraft/audio claim.
use super::producer_fixture::{self as status, Event as StatusEvent, Notice};
use super::voice_transport_fixture as transport;
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
};

pub(super) const WAIT: Duration = Duration::from_secs(8);
const CHILD_LIMIT: Duration = Duration::from_secs(25);
const MAX_LINE: usize = 1024;
const MAX_COMMANDS: u32 = 16;
const MAX_SESSIONS: usize = 32;
const NONCE: &str = "AURORA_NATIVE_VOICE_TEST_NONCE";
const CONTROL: &str = "AURORA_NATIVE_VOICE_TEST_CONTROL";
const PROFILE: &str = "AURORA_NATIVE_VOICE_TEST_PROFILE";
const HOLD_STOP: &str = "AURORA_NATIVE_VOICE_TEST_HOLD_STOP";

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Action {
    RewriteValid,
    RenameIdentical,
    RewriteInvalid,
    RemoveConfiguration,
    OversizeConfiguration,
    RestoreValid,
    DropUdp,
    BindUdp,
    DropTcp,
    MutableNoise,
    AddForeignMod,
    RemoveForeignMod,
    ReleaseStop,
    HoldUdpReturns,
    ReleaseUdpReturns,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandEnvelope {
    nonce: Uuid,
    sequence: u32,
    action: Action,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase", deny_unknown_fields)]
enum Event {
    Ready {
        nonce: Uuid,
        tcp: SocketAddr,
        udp: SocketAddr,
    },
    StatusObserved {
        pong_sent: bool,
    },
    Acknowledged {
        sequence: u32,
    },
    StopObserved {
        lines: usize,
        eof: bool,
    },
    UdpHeld {
        count: usize,
    },
}
type FixtureResult<T> = std::result::Result<T, &'static str>;

fn line(reader: &mut impl BufRead) -> FixtureResult<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    let count = reader
        .take(MAX_LINE as u64 + 1)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| "voice_fixture_read")?;
    if count == 0 {
        return Ok(None);
    }
    if count > MAX_LINE || bytes.last() != Some(&b'\n') {
        return Err("voice_fixture_line_limit");
    }
    Ok(Some(bytes))
}
fn message(stream: &mut TcpStream, value: &impl Serialize) -> FixtureResult<()> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| "voice_fixture_encode")?;
    bytes.push(b'\n');
    if bytes.len() > MAX_LINE {
        return Err("voice_fixture_message_limit");
    }
    stream.write_all(&bytes).map_err(|_| "voice_fixture_write")
}
fn udp_endpoint(profile: manifest::StoredVoiceProfileV2) -> SocketAddr {
    SocketAddr::from((
        Ipv4Addr::LOCALHOST,
        match profile {
            manifest::StoredVoiceProfileV2::Svc => 43002,
            manifest::StoredVoiceProfileV2::Plasmo => 43003,
        },
    ))
}

pub(super) struct Harness {
    listener: TcpListener,
    control: Option<BufReader<TcpStream>>,
    nonce: Uuid,
    sequence: u32,
    stopped: bool,
    pub statuses: usize,
    pub pongs: usize,
    udp_held: usize,
    profile: manifest::StoredVoiceProfileV2,
}
impl Harness {
    pub(super) fn new(profile: manifest::StoredVoiceProfileV2) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        Self {
            listener,
            control: None,
            nonce: Uuid::new_v4(),
            sequence: 0,
            stopped: false,
            statuses: 0,
            pongs: 0,
            udp_held: 0,
            profile,
        }
    }
    pub(super) fn control_addr(&self) -> SocketAddr {
        self.listener.local_addr().unwrap()
    }
    pub(super) fn nonce(&self) -> Uuid {
        self.nonce
    }
    pub(super) fn ready(&mut self) {
        let deadline = Instant::now() + WAIT;
        let stream = loop {
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    assert!(peer.ip().is_loopback());
                    break stream;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "voice_fixture_ready_deadline");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => panic!("voice_fixture_accept"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream.set_read_timeout(Some(WAIT)).unwrap();
        stream.set_write_timeout(Some(WAIT)).unwrap();
        self.control = Some(BufReader::new(stream));
        match self.event() {
            Event::Ready { nonce, tcp, udp } => {
                assert_eq!(nonce, self.nonce);
                assert_eq!(tcp, status::ENDPOINT);
                assert_eq!(udp, udp_endpoint(self.profile));
            }
            _ => panic!("voice_fixture_expected_ready"),
        }
    }
    fn event(&mut self) -> Event {
        let raw = line(self.control.as_mut().expect("voice_fixture_control"))
            .expect("voice_fixture_event_line")
            .expect("voice_fixture_event_present");
        serde_json::from_slice(&raw).expect("voice_fixture_event_schema")
    }
    fn observe(&mut self, event: Event) {
        match event {
            Event::StatusObserved { pong_sent } => {
                self.statuses += usize::from(!pong_sent);
                self.pongs += usize::from(pong_sent);
                assert!(self.statuses <= MAX_SESSIONS && self.pongs <= MAX_SESSIONS);
            }
            Event::StopObserved { lines, eof } => {
                assert_eq!(lines, 1);
                assert!(eof);
                assert!(!self.stopped);
                self.stopped = true;
            }
            Event::UdpHeld { count } => {
                assert_eq!(count, self.udp_held + 1);
                assert!(count <= 4);
                self.udp_held = count;
            }
            _ => panic!("voice_fixture_unexpected_event"),
        }
    }
    pub(super) fn command(&mut self, action: Action) {
        if matches!(action, Action::HoldUdpReturns) {
            self.udp_held = 0;
        }
        self.sequence += 1;
        assert!(self.sequence <= MAX_COMMANDS);
        let command = CommandEnvelope {
            nonce: self.nonce,
            sequence: self.sequence,
            action,
        };
        message(self.control.as_mut().unwrap().get_mut(), &command).unwrap();
        loop {
            match self.event() {
                Event::Acknowledged { sequence } => {
                    assert_eq!(sequence, self.sequence);
                    break;
                }
                event => self.observe(event),
            }
        }
    }
    pub(super) fn status_observed(&mut self) {
        while self.pongs == 0 {
            let event = self.event();
            self.observe(event);
        }
    }
    pub(super) fn stop_observed(&mut self) {
        while !self.stopped {
            let event = self.event();
            self.observe(event);
        }
    }
    pub(super) fn udp_held(&mut self, count: usize) {
        assert!((1..=4).contains(&count));
        while self.udp_held < count {
            let event = self.event();
            self.observe(event);
        }
    }
    pub(super) fn udp_bytes(&self) {
        let client = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        client.set_read_timeout(Some(WAIT)).unwrap();
        let endpoint = udp_endpoint(self.profile);
        assert_eq!(client.send_to(b"aur85c01", endpoint).unwrap(), 8);
        let mut response = [0; 9];
        let (count, sender) = client.recv_from(&mut response).unwrap();
        assert_eq!((count, sender), (8, endpoint));
        assert_eq!(&response[..count], b"aur85c01");
    }
    pub(super) fn ports_released(&self) {
        drop(TcpListener::bind(status::ENDPOINT).expect("voice_fixture_tcp_rebind"));
        drop(UdpSocket::bind(udp_endpoint(self.profile)).expect("voice_fixture_udp_rebind"));
    }
    pub(super) fn assert_no_child(&self) {
        assert!(
            matches!(self.listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
    pub(super) fn finish(self) {
        let address = self.control_addr();
        drop(self);
        drop(TcpListener::bind(address).expect("voice_fixture_control_rebind"));
    }
}

struct Sessions {
    stop: Arc<AtomicBool>,
    tasks: Vec<thread::JoinHandle<FixtureResult<()>>>,
}
impl Drop for Sessions {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for task in self.tasks.drain(..) {
            let _ = task.join();
        }
    }
}
fn bind_udp(profile: manifest::StoredVoiceProfileV2) -> FixtureResult<UdpSocket> {
    let socket = UdpSocket::bind(udp_endpoint(profile)).map_err(|_| "voice_fixture_udp_bind")?;
    socket
        .set_nonblocking(true)
        .map_err(|_| "voice_fixture_udp_nonblocking")?;
    Ok(socket)
}
fn configuration(profile: manifest::StoredVoiceProfileV2) -> Vec<u8> {
    let mut bytes = profile.configuration().to_vec();
    bytes.extend_from_slice(b"# generated by the own synthetic voice fixture\n");
    bytes
}
fn apply_action(
    root: &Path,
    profile: manifest::StoredVoiceProfileV2,
    sequence: u32,
    action: Action,
    udp: &mut Option<UdpSocket>,
    released: &mut bool,
) -> FixtureResult<()> {
    let path = root.join(profile.configuration_name());
    match action {
        Action::RewriteValid => {
            let mut bytes = configuration(profile);
            bytes
                .extend_from_slice(format!("# completed rewrite sequence {sequence}\n").as_bytes());
            fs::write(path, bytes)
        }
        Action::RestoreValid => fs::write(path, configuration(profile)),
        Action::RenameIdentical => {
            let bytes = fs::read(&path).map_err(|_| "voice_fixture_read_for_rename")?;
            if bytes.len() > 256 * 1024 {
                return Err("voice_fixture_rename_byte_limit");
            }
            let parent = path.parent().ok_or("voice_fixture_configuration_parent")?;
            let next = parent.join(format!("fixture-voice-next-{sequence}.bin"));
            let old = parent.join(format!("fixture-voice-retained-{sequence}.bin"));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&next)
                .map_err(|_| "voice_fixture_new_for_rename")?;
            file.write_all(&bytes)
                .map_err(|_| "voice_fixture_write_for_rename")?;
            drop(file);
            fs::rename(&path, old).and_then(|()| fs::rename(next, path))
        }
        Action::RewriteInvalid => {
            let valid =
                String::from_utf8(configuration(profile)).map_err(|_| "voice_fixture_utf8")?;
            let changed = valid.replace("43002", "43004").replace("43003", "43004");
            fs::write(path, changed)
        }
        Action::RemoveConfiguration => fs::remove_file(path),
        Action::OversizeConfiguration => fs::write(path, vec![b'#'; 256 * 1024 + 1]),
        Action::DropUdp => {
            drop(udp.take());
            return Ok(());
        }
        Action::BindUdp => {
            if udp.is_some() {
                return Err("voice_fixture_udp_already_bound");
            }
            *udp = Some(bind_udp(profile)?);
            return Ok(());
        }
        Action::MutableNoise => {
            fs::create_dir_all(root.join("world")).map_err(|_| "voice_fixture_world_directory")?;
            fs::create_dir_all(root.join("logs")).map_err(|_| "voice_fixture_log_directory")?;
            fs::write(root.join("world/fixture-level.dat"), b"mutable world bytes")
                .and_then(|()| fs::write(root.join("logs/fixture.log"), b"mutable log bytes"))
        }
        Action::AddForeignMod => {
            fs::write(root.join("mods/fixture-foreign.jar"), b"unapproved mod")
        }
        Action::RemoveForeignMod => fs::remove_file(root.join("mods/fixture-foreign.jar")),
        Action::ReleaseStop => {
            *released = true;
            return Ok(());
        }
        Action::HoldUdpReturns | Action::ReleaseUdpReturns | Action::DropTcp => {
            return Err("voice_fixture_transport_action_dispatch")
        }
    }
    .map_err(|_| "voice_fixture_action_io")
}

fn child_loop(nonce: Uuid) -> FixtureResult<()> {
    let profile = match std::env::var(PROFILE)
        .map_err(|_| "voice_fixture_profile")?
        .as_str()
    {
        "svc" => manifest::StoredVoiceProfileV2::Svc,
        "plasmo" => manifest::StoredVoiceProfileV2::Plasmo,
        _ => return Err("voice_fixture_profile"),
    };
    let hold_stop = match std::env::var(HOLD_STOP)
        .map_err(|_| "voice_fixture_hold_stop")?
        .as_str()
    {
        "0" => false,
        "1" => true,
        _ => return Err("voice_fixture_hold_stop"),
    };
    let address: SocketAddr = std::env::var(CONTROL)
        .map_err(|_| "voice_fixture_control_env")?
        .parse()
        .map_err(|_| "voice_fixture_control_address")?;
    if address.ip() != Ipv4Addr::LOCALHOST || address.port() == 0 {
        return Err("voice_fixture_control_literal");
    }
    let root = std::env::current_dir().map_err(|_| "voice_fixture_cwd")?;
    // The reviewed baseline exists before spawn. This own writer changes it
    // after spawn, so the runtime source must accept stable compatible defaults.
    fs::write(
        root.join(profile.configuration_name()),
        configuration(profile),
    )
    .map_err(|_| "voice_fixture_boot_configuration")?;
    let listener = TcpListener::bind(status::ENDPOINT).map_err(|_| "voice_fixture_tcp_bind")?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "voice_fixture_tcp_nonblocking")?;
    let mut listener = Some(listener);
    let mut udp = Some(bind_udp(profile)?);
    let mut udp_traffic = transport::UdpTraffic::default();
    let mut control =
        TcpStream::connect_timeout(&address, WAIT).map_err(|_| "voice_fixture_control_connect")?;
    control
        .set_write_timeout(Some(WAIT))
        .map_err(|_| "voice_fixture_control_timeout")?;
    message(
        &mut control,
        &Event::Ready {
            nonce,
            tcp: status::ENDPOINT,
            udp: udp_endpoint(profile),
        },
    )?;
    control
        .set_nonblocking(true)
        .map_err(|_| "voice_fixture_control_nonblocking")?;
    let (stdin_send, stdin_recv) = mpsc::sync_channel(1);
    let stdin_reader = thread::spawn(move || {
        let result = (|| {
            let mut reader = BufReader::new(std::io::stdin().lock());
            let first = line(&mut reader)?.ok_or("voice_fixture_stop_missing")?;
            if first != b"stop\n" || line(&mut reader)?.is_some() {
                return Err("voice_fixture_stop_protocol");
            }
            Ok(())
        })();
        let _ = stdin_send.try_send(result);
    });
    let (event_send, event_recv) = mpsc::sync_channel::<Notice>(MAX_SESSIONS);
    let mut sessions = Sessions {
        stop: Arc::new(AtomicBool::new(false)),
        tasks: Vec::new(),
    };
    let first_status = Arc::new(AtomicBool::new(false));
    let mut pending = Vec::new();
    let mut sequence = 0;
    let mut stopped = false;
    let mut released = false;
    let deadline = Instant::now() + CHILD_LIMIT;
    loop {
        if Instant::now() >= deadline {
            std::process::exit(19);
        }
        if !stopped {
            match stdin_recv.try_recv() {
                Ok(result) => {
                    result?;
                    stopped = true;
                    message(
                        &mut control,
                        &Event::StopObserved {
                            lines: 1,
                            eof: true,
                        },
                    )?;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(_) => return Err("voice_fixture_stdin_worker"),
            }
        }
        if stopped && (!hold_stop || released) {
            break;
        }
        while let Ok(notice) = event_recv.try_recv() {
            match notice.event {
                StatusEvent::StatusObserved { pong_sent } => {
                    message(&mut control, &Event::StatusObserved { pong_sent })?
                }
                _ => return Err("voice_fixture_status_notice"),
            }
            let _ = notice.forwarded.try_send(());
        }
        let mut incoming = [0; 256];
        match control.read(&mut incoming) {
            Ok(0) => std::process::exit(17),
            Ok(count) => pending.extend_from_slice(&incoming[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return Err("voice_fixture_control_read"),
        }
        if pending.len() > MAX_LINE {
            return Err("voice_fixture_control_line_limit");
        }
        while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            let encoded: Vec<_> = pending.drain(..=end).collect();
            let command: CommandEnvelope =
                serde_json::from_slice(&encoded).map_err(|_| "voice_fixture_command_schema")?;
            sequence += 1;
            if sequence > MAX_COMMANDS || command.nonce != nonce || command.sequence != sequence {
                return Err("voice_fixture_command_identity");
            }
            match command.action {
                Action::DropTcp => drop(listener.take()),
                Action::HoldUdpReturns => udp_traffic.hold()?,
                Action::ReleaseUdpReturns => {
                    udp_traffic.release(udp.as_ref().ok_or("voice_fixture_udp_missing")?)?
                }
                action => apply_action(&root, profile, sequence, action, &mut udp, &mut released)?,
            }
            message(&mut control, &Event::Acknowledged { sequence })?;
        }
        if let Some(socket) = &udp {
            if let Some(count) = transport::udp(socket, nonce, &mut udp_traffic)? {
                message(&mut control, &Event::UdpHeld { count })?;
            }
        }
        if !stopped {
            for _ in 0..2 {
                let Some(listener) = listener.as_ref() else {
                    break;
                };
                match listener.accept() {
                    Ok((stream, peer)) => {
                        if !peer.ip().is_loopback() || sessions.tasks.len() >= MAX_SESSIONS {
                            return Err("voice_fixture_status_sessions");
                        }
                        let stop = sessions.stop.clone();
                        let notices = event_send.clone();
                        let first = first_status.clone();
                        sessions.tasks.push(thread::spawn(move || {
                            transport::session(stream, nonce, stop, notices, first)
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => return Err("voice_fixture_tcp_accept"),
                }
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    drop(listener);
    drop(udp);
    drop(sessions);
    stdin_reader
        .join()
        .map_err(|_| "voice_fixture_stdin_join")?;
    Ok(())
}

#[test]
#[ignore = "private own voice child fixture invoked only by native parent tests"]
fn dedicated_voice_child_fixture() {
    let nonce = std::env::var(NONCE).expect("voice_fixture_explicit_nonce");
    child_loop(Uuid::parse_str(&nonce).expect("voice_fixture_nonce_format"))
        .expect("dedicated_voice_child_fixture_protocol");
}
