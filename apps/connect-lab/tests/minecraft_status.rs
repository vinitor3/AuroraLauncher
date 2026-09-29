//! Independent status-protocol fixtures; these tests never run Minecraft or Java.
use aurora_connect_lab::*;
use std::{
    collections::BTreeSet,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
    time::{Instant, sleep, timeout},
};

const HOST: &str = "mc.fixture.invalid";
const PORT: u16 = 25565;
// Explicit fixture input, not a detected runtime or a compatibility declaration.
const PROTOCOL: i32 = 767;
const STATUS: &[u8] = br#"{"version":{"name":"PRIVATE_VERSION","protocol":767},"players":{"max":20,"online":2,"sample":[{"name":"PRIVATE_PLAYER","id":"ffffffff-eeee-dddd-cccc-bbbbbbbbbbbb"}]},"description":{"text":"PRIVATE_MOTD","clickEvent":{"action":"open_url","value":"https://private.fixture.invalid"}},"favicon":"data:image/png;base64,PRIVATE_ICON","enforcesSecureChat":true}"#;

// Owns only the child created here. Bounded event reads never print peer IDs or
// raw child output on assertion failures; Drop reaps the child even on panic.
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
                let Ok(event) = serde_json::from_slice(&line) else {
                    break;
                };
                // No blocking send can outlive the process teardown.
                if tx.try_send(event).is_err() {
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
    fn send(&mut self, text: &str) {
        use std::io::Write;
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{text}").unwrap();
        stdin.flush().unwrap();
    }
    async fn event(&self, expected: &str) -> serde_json::Value {
        timeout(Duration::from_secs(10), async {
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
    async fn exited(&mut self, success: bool) {
        timeout(Duration::from_secs(5), async {
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    assert_eq!(status.success(), success);
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
fn request(endpoint: SocketAddr) -> MinecraftStatusRequest {
    MinecraftStatusRequest {
        endpoint,
        handshake_host: HOST.into(),
        handshake_port: PORT,
        protocol: PROTOCOL,
        timeout: Duration::from_secs(2),
    }
}
fn varint(value: i32) -> Vec<u8> {
    let mut result = Vec::new();
    let mut remaining = value as u32;
    loop {
        let part = (remaining & 127) as u8;
        remaining >>= 7;
        result.push(part | if remaining == 0 { 0 } else { 128 });
        if remaining == 0 {
            return result;
        }
    }
}
fn packet(body: &[u8]) -> Vec<u8> {
    [varint(body.len() as i32), body.to_vec()].concat()
}
fn status_packet(json: &[u8]) -> Vec<u8> {
    packet(&[vec![0], varint(json.len() as i32), json.to_vec()].concat())
}

// These readers exist only in the independent fixture, not in the probe module.
async fn fixture_frame(socket: &mut TcpStream) -> Vec<u8> {
    let mut length = 0_usize;
    for shift in (0..35).step_by(7) {
        let byte = socket.read_u8().await.unwrap();
        length |= usize::from(byte & 127) << shift;
        if byte < 128 {
            assert!(length <= 1024, "fixture receives only small requests");
            let mut body = vec![0; length];
            socket.read_exact(&mut body).await.unwrap();
            return body;
        }
    }
    panic!("invalid fixture request length");
}
async fn assert_socket_closed(socket: &mut TcpStream) {
    let mut byte = [0];
    match socket.read(&mut byte).await {
        Ok(0) => {}
        // Windows can reset/abort a connection closed with unread peer bytes.
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
            ) => {}
        result => panic!("expected socket closure, received {result:?}"),
    }
}
fn fixture_int(cursor: &mut &[u8]) -> i32 {
    let mut value = 0_u32;
    for shift in (0..35).step_by(7) {
        let (&byte, rest) = cursor.split_first().unwrap();
        *cursor = rest;
        value |= u32::from(byte & 127) << shift;
        if byte < 128 {
            return value as i32;
        }
    }
    panic!("invalid fixture request varint");
}
#[derive(Clone, Copy)]
enum Pong {
    Echo,
    WrongNonce,
    WrongId,
    ExtraByte,
    Truncated,
    Stall,
}
enum Script {
    Raw(Vec<u8>),
    Exchange {
        json: Vec<u8>,
        pong: Pong,
        status_delay: Duration,
        pong_delay: Duration,
    },
    PartialLengthAndStall,
}
fn exchange(json: &[u8], pong: Pong) -> Script {
    Script::Exchange {
        json: json.to_vec(),
        pong,
        status_delay: Duration::ZERO,
        pong_delay: Duration::ZERO,
    }
}
struct Fixture {
    address: SocketAddr,
    accepts: Arc<AtomicUsize>,
    task: Option<JoinHandle<()>>,
}
impl Fixture {
    async fn finish(mut self) {
        timeout(Duration::from_secs(3), self.task.take().unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(self.accepts.load(Ordering::SeqCst), 1);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
async fn fixture(script: Script) -> Fixture {
    let listener = TcpListener::bind(local()).await.unwrap();
    let address = listener.local_addr().unwrap();
    let accepts = Arc::new(AtomicUsize::new(0));
    let count = accepts.clone();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        count.fetch_add(1, Ordering::SeqCst);
        let hello = fixture_frame(&mut socket).await;
        let mut cursor = hello.as_slice();
        assert_eq!(fixture_int(&mut cursor), 0);
        assert_eq!(fixture_int(&mut cursor), PROTOCOL);
        let host_length = fixture_int(&mut cursor) as usize;
        assert_eq!(&cursor[..host_length], HOST.as_bytes());
        cursor = &cursor[host_length..];
        assert_eq!(&cursor[..2], &PORT.to_be_bytes());
        cursor = &cursor[2..];
        assert_eq!(fixture_int(&mut cursor), 1); // STATUS, never login.
        assert!(cursor.is_empty());
        assert_eq!(fixture_frame(&mut socket).await, [0]);
        match script {
            Script::Raw(bytes) => {
                let _ = socket.write_all(&bytes).await;
                let _ = socket.shutdown().await;
                return;
            }
            Script::PartialLengthAndStall => {
                socket.write_all(&[0x81]).await.unwrap();
            }
            Script::Exchange {
                json,
                pong,
                status_delay,
                pong_delay,
            } => {
                sleep(status_delay).await;
                // Deliberately cross packet boundaries with small writes.
                for bytes in status_packet(&json).chunks(7) {
                    if socket.write_all(bytes).await.is_err() {
                        return;
                    }
                }
                let mut ping = fixture_frame(&mut socket).await;
                assert_eq!(ping.len(), 9);
                assert_eq!(ping[0], 1);
                sleep(pong_delay).await;
                match pong {
                    Pong::Echo => {}
                    Pong::WrongNonce => ping[8] ^= 1,
                    Pong::WrongId => ping[0] = 0,
                    Pong::ExtraByte => ping.push(0),
                    Pong::Truncated => {
                        let _ = socket.write_all(&[9, 1]).await;
                        let _ = socket.shutdown().await;
                        return;
                    }
                    Pong::Stall => {
                        assert_socket_closed(&mut socket).await;
                        return;
                    }
                }
                let _ = socket.write_all(&packet(&ping)).await;
            }
        }
        // Success, failure, timeout and dropped probes must close their socket.
        assert_socket_closed(&mut socket).await;
    });
    Fixture {
        address,
        accepts,
        task: Some(task),
    }
}

#[tokio::test]
async fn invalid_request_is_rejected_before_contacting_listener() {
    let listener = TcpListener::bind(local()).await.unwrap();
    let address = listener.local_addr().unwrap();
    for host in [
        "",
        "bad\0forwarded",
        "a\n",
        "a\r",
        "a\t",
        "two words",
        "https://a",
        "a/path",
        "a\\path",
        "name@host",
        "é.example",
        "host:25565",
        "[::1%scope]",
        "[]",
        ":",
        "-bad.example",
        "bad-.example",
        "a..b",
    ] {
        let mut value = request(address);
        value.handshake_host = host.into();
        assert_eq!(
            probe_minecraft_status(value).await.unwrap_err(),
            MinecraftStatusError::InvalidRequest
        );
    }
    for long_host in ["a".repeat(64), ["a.".repeat(127), "a".into()].concat()] {
        let mut value = request(address);
        value.handshake_host = long_host;
        assert_eq!(
            probe_minecraft_status(value).await.unwrap_err(),
            MinecraftStatusError::InvalidRequest
        );
    }
    for duration in [Duration::ZERO, Duration::from_millis(5001)] {
        let mut value = request(address);
        value.timeout = duration;
        assert_eq!(
            probe_minecraft_status(value).await.unwrap_err(),
            MinecraftStatusError::InvalidRequest
        );
    }
    for protocol in [0, -1] {
        let mut value = request(address);
        value.protocol = protocol;
        assert_eq!(
            probe_minecraft_status(value).await.unwrap_err(),
            MinecraftStatusError::InvalidRequest
        );
    }
    let mut value = request(address);
    value.handshake_port = 0;
    assert_eq!(
        probe_minecraft_status(value).await.unwrap_err(),
        MinecraftStatusError::InvalidRequest
    );
    for endpoint in [
        "127.0.0.1:0",
        "0.0.0.0:25565",
        "192.0.2.1:25565",
        "[::]:25565",
        "[2001:db8::1]:25565",
    ] {
        let mut value = request(address);
        value.endpoint = endpoint.parse().unwrap();
        assert_eq!(
            probe_minecraft_status(value).await.unwrap_err(),
            MinecraftStatusError::InvalidRequest
        );
    }
    assert!(
        timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn malformed_varints_lengths_ids_and_truncation_are_bounded() {
    use MinecraftStatusError::*;
    let mut extra = vec![0];
    extra.extend(varint(STATUS.len() as i32));
    extra.extend_from_slice(STATUS);
    extra.push(99);
    let cases = [
        (vec![0x80], Truncated),
        (vec![0x81, 0], InvalidVarInt),
        (vec![0xff, 0xff, 0xff, 0xff, 0x10], InvalidVarInt),
        (vec![0x80, 0x80, 0x80, 0x80, 0x80, 0], InvalidVarInt),
        (varint(-1), InvalidPacket),
        (vec![0], InvalidPacket),
        (
            varint((MAX_MINECRAFT_STATUS_JSON_BYTES + 7) as i32),
            FrameTooLarge,
        ),
        (vec![10, 0], Truncated),
        (packet(&[1, 0]), InvalidPacket),
        (packet(&[0, 3, b'{', b'}']), InvalidPacket),
        (packet(&[vec![0], varint(-1)].concat()), InvalidPacket),
        (
            packet(
                &[
                    vec![0],
                    varint((MAX_MINECRAFT_STATUS_JSON_BYTES + 1) as i32),
                ]
                .concat(),
            ),
            FrameTooLarge,
        ),
        (packet(&[0, 0xff, 0xff, 0xff, 0xff, 0x10]), InvalidVarInt),
        (packet(&extra), InvalidPacket),
    ];
    for (bytes, expected) in cases {
        let f = fixture(Script::Raw(bytes)).await;
        assert_eq!(
            probe_minecraft_status(request(f.address))
                .await
                .unwrap_err(),
            expected
        );
        f.finish().await;
    }
}

#[tokio::test]
async fn json_utf8_types_duplicates_and_trailing_content_are_rejected() {
    use MinecraftStatusError::*;
    let cases: Vec<(&[u8], MinecraftStatusError)> = vec![
        (&[0xff], InvalidUtf8),
        (b"{", InvalidJson),
        (br#"[]"#, InvalidSchema),
        (br#"{"version":{"name":"x","protocol":"767"},"description":"x"}"#, InvalidSchema),
        (br#"{"version":{"name":"x","protocol":767.5},"description":"x"}"#, InvalidSchema),
        (br#"{"version":{"name":"x","protocol":2147483648},"description":"x"}"#, InvalidSchema),
        (br#"{"version":{"name":1,"protocol":767},"description":"x"}"#, InvalidSchema),
        (br#"{"version":{"name":"x","protocol":767,"protocol":766},"description":"x"}"#, InvalidSchema),
        (br#"{"version":{"name":"x","protocol":767},"description":false}"#, InvalidSchema),
        (br#"{"version":{"name":"x","protocol":767},"description":"x","players":[]}"#, InvalidSchema),
        (br#"{"version":{"name":"x","protocol":767},"description":"x","players":null}"#, InvalidSchema),
        (br#"{"version":{"name":"x","protocol":767},"description":"x","players":{"max":20,"online":-1}}"#, InvalidSchema),
        (br#"{"version":{"name":"x","protocol":767},"description":"x","players":{"max":4294967295,"online":1}}"#, InvalidSchema),
        (br#"{"version":{"name":"x","protocol":767},"description":"x","players":{"max":20,"online":1,"sample":[{"name":7,"id":"x"}]}}"#, InvalidSchema),
        (br#"{"version":{"name":"x","protocol":767},"description":"x","favicon":{}}"#, InvalidSchema),
        (br#"{"version":{"name":"x","protocol":767},"description":"x","enforcesSecureChat":"true"}"#, InvalidSchema),
    ];
    for (json, expected) in cases {
        let f = fixture(Script::Raw(status_packet(json))).await;
        assert_eq!(
            probe_minecraft_status(request(f.address))
                .await
                .unwrap_err(),
            expected
        );
        f.finish().await;
    }
    let trailing = [STATUS, b"{}"].concat();
    let f = fixture(Script::Raw(status_packet(&trailing))).await;
    assert_eq!(
        probe_minecraft_status(request(f.address))
            .await
            .unwrap_err(),
        InvalidJson
    );
    f.finish().await;
    let nested = [
        br#"{"version":{"name":"x","protocol":767},"description":"#.as_slice(),
        "[".repeat(150).as_bytes(),
        br#""x""#,
        "]".repeat(150).as_bytes(),
        b"}",
    ]
    .concat();
    let f = fixture(Script::Raw(status_packet(&nested))).await;
    assert_eq!(
        probe_minecraft_status(request(f.address))
            .await
            .unwrap_err(),
        InvalidJson
    );
    f.finish().await;
}

#[tokio::test]
async fn exact_json_limit_is_accepted_without_returning_private_text() {
    let mut status: serde_json::Value = serde_json::from_slice(STATUS).unwrap();
    status["description"]["text"] = "".into();
    let remaining = MAX_MINECRAFT_STATUS_JSON_BYTES - serde_json::to_vec(&status).unwrap().len();
    status["description"]["text"] = ["界".repeat(remaining / 3), " ".repeat(remaining % 3)]
        .concat()
        .into();
    let json = serde_json::to_vec(&status).unwrap();
    assert_eq!(json.len(), MAX_MINECRAFT_STATUS_JSON_BYTES);
    assert!(
        std::str::from_utf8(&json).unwrap().encode_utf16().count()
            < MAX_MINECRAFT_STATUS_UTF16_UNITS
    );
    let f = fixture(exchange(&json, Pong::Echo)).await;
    let summary = probe_minecraft_status(request(f.address)).await.unwrap();
    assert_eq!(summary.reported_players_online, Some(2));
    let serialized = serde_json::to_string(&summary).unwrap();
    for private in [
        "PRIVATE",
        "ffffffff",
        "fixture.invalid",
        "description",
        "favicon",
        "sample",
        "name",
    ] {
        assert!(!serialized.contains(private));
    }
    assert!(summary.total_micros >= summary.ping_micros);
    f.finish().await;
}

#[tokio::test]
async fn java_utf16_limit_is_independent_of_utf8_bytes_and_unicode_scalar_count() {
    let mut exact = STATUS.to_vec();
    exact.resize(MAX_MINECRAFT_STATUS_UTF16_UNITS, b' ');
    let f = fixture(exchange(&exact, Pong::Echo)).await;
    assert!(probe_minecraft_status(request(f.address)).await.is_ok());
    f.finish().await;

    let mut too_many_ascii = exact;
    too_many_ascii.push(b' ');
    let mut status: serde_json::Value = serde_json::from_slice(STATUS).unwrap();
    status["description"]["text"] = "".into();
    let base = serde_json::to_string(&status)
        .unwrap()
        .encode_utf16()
        .count();
    status["description"]["text"] = "😀"
        .repeat((MAX_MINECRAFT_STATUS_UTF16_UNITS + 1 - base).div_ceil(2))
        .into();
    let astral = serde_json::to_string(&status).unwrap();
    assert!(astral.len() <= MAX_MINECRAFT_STATUS_JSON_BYTES);
    assert!(astral.chars().count() <= MAX_MINECRAFT_STATUS_UTF16_UNITS);
    assert!(astral.encode_utf16().count() > MAX_MINECRAFT_STATUS_UTF16_UNITS);
    for bytes in [too_many_ascii, astral.into_bytes()] {
        let f = fixture(Script::Raw(status_packet(&bytes))).await;
        assert_eq!(
            probe_minecraft_status(request(f.address))
                .await
                .unwrap_err(),
            MinecraftStatusError::StringTooLong
        );
        f.finish().await;
    }
}

#[tokio::test]
async fn absent_player_counts_and_different_protocol_are_reported_honestly() {
    let f = fixture(exchange(br#"{"version":{"name":"other","protocol":766},"description":[{"text":"hidden"}],"modSpecific":{"ignored":true}}"#, Pong::Echo)).await;
    let summary = probe_minecraft_status(request(f.address)).await.unwrap();
    assert!(!summary.protocol_matches_requested);
    assert_eq!(summary.reported_protocol, 766);
    assert_eq!(summary.requested_protocol, PROTOCOL);
    assert_eq!(summary.reported_players_online, None);
    assert_eq!(summary.reported_players_max, None);
    f.finish().await;
}

#[tokio::test]
async fn pong_must_have_exact_id_length_and_nonce() {
    for (pong, expected) in [
        (Pong::WrongNonce, MinecraftStatusError::PongMismatch),
        (Pong::WrongId, MinecraftStatusError::InvalidPacket),
        (Pong::ExtraByte, MinecraftStatusError::FrameTooLarge),
        (Pong::Truncated, MinecraftStatusError::Truncated),
    ] {
        let f = fixture(exchange(STATUS, pong)).await;
        assert_eq!(
            probe_minecraft_status(request(f.address))
                .await
                .unwrap_err(),
            expected
        );
        f.finish().await;
    }
}

#[tokio::test]
async fn total_deadline_covers_partial_length_and_pong_and_closes_socket() {
    for script in [
        Script::PartialLengthAndStall,
        exchange(STATUS, Pong::Stall),
        Script::Exchange {
            json: STATUS.to_vec(),
            pong: Pong::Echo,
            status_delay: Duration::from_millis(200),
            pong_delay: Duration::from_millis(200),
        },
    ] {
        let f = fixture(script).await;
        let mut value = request(f.address);
        value.timeout = Duration::from_millis(300);
        let started = Instant::now();
        assert_eq!(
            probe_minecraft_status(value).await.unwrap_err(),
            MinecraftStatusError::Timeout
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        f.finish().await;
    }
}

#[tokio::test]
async fn cancelling_a_probe_closes_its_socket_without_background_work() {
    let f = fixture(Script::PartialLengthAndStall).await;
    let task = tokio::spawn(probe_minecraft_status(request(f.address)));
    timeout(Duration::from_secs(1), async {
        while f.accepts.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        sleep(Duration::from_millis(20)).await;
    })
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    f.finish().await;
}

#[tokio::test]
async fn real_authenticated_tcp_bridge_carries_status_to_independent_fixture_and_shuts_down() {
    let f = fixture(exchange(STATUS, Pong::Echo)).await;
    let guest_key = generate_key();
    let host = Host::start(HostConfig {
        secret_key: generate_key(),
        bind_addr: local(),
        session_id: "mc-status".into(),
        services: vec![Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: f.address,
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        }],
        leases: vec![Lease {
            lease_id: "status-reader".into(),
            peer: guest_key.public(),
            expires_at_unix_ms: unix_millis() + 60_000,
            services: BTreeSet::from(["game".into()]),
        }],
        limits: Limits::default(),
    })
    .await
    .unwrap();
    let guest = Guest::connect(GuestConfig {
        secret_key: guest_key,
        bind_addr: local(),
        host: host.addr(),
        session_id: "mc-status".into(),
        lease_id: "status-reader".into(),
        capabilities: vec![Capability::ReliableStream],
        limits: Limits::default(),
    })
    .await
    .unwrap();
    let bridge = guest.tcp_bridge("game", 1, local()).await.unwrap();
    let summary = probe_minecraft_status(request(bridge.local_addr()))
        .await
        .unwrap();
    assert_eq!(summary.reported_protocol, PROTOCOL);
    assert!(summary.protocol_matches_requested);
    assert_eq!(summary.reported_players_online, Some(2));
    assert_eq!(host.metrics().destination_tcp_connects, 1);
    f.finish().await;
    let address = bridge.local_addr();
    bridge.shutdown().await;
    guest.shutdown().await;
    host.shutdown().await;
    assert!(TcpStream::connect(address).await.is_err());
}

#[tokio::test]
async fn actual_cli_prints_only_redacted_protocol_evidence() {
    let f = fixture(exchange(STATUS, Pong::Echo)).await;
    let address = f.address;
    let output = timeout(
        Duration::from_secs(8),
        tokio::task::spawn_blocking(move || {
            let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_aurora-connect-lab"));
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            command
                .args([
                    "minecraft-status",
                    &address.to_string(),
                    HOST,
                    &PORT.to_string(),
                    &PROTOCOL.to_string(),
                ])
                .output()
                .unwrap()
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["event"], "minecraft_status_received");
    assert_eq!(value["runtimeHomologated"], false);
    assert_eq!(value["worldJoined"], false);
    assert_eq!(value["summary"]["reportedProtocol"], PROTOCOL);
    let text = String::from_utf8(output.stdout).unwrap();
    for private in ["PRIVATE", "ffffffff", "fixture.invalid", "127.0.0.1"] {
        assert!(!text.contains(private));
    }
    f.finish().await;
}

#[tokio::test]
async fn tcp_only_guest_cli_carries_status_without_authorizing_or_opening_udp() {
    let f = fixture(exchange(STATUS, Pong::Echo)).await;
    let host_key = generate_key();
    let reservation = std::net::UdpSocket::bind(local()).unwrap();
    let bind = reservation.local_addr().unwrap();
    drop(reservation);
    let mut cli = CliChild::start(&[
        "guest-tcp".into(),
        host_key.public().to_string(),
        bind.to_string(),
        "cli-tcp".into(),
        "127.0.0.1:0".into(),
        "30".into(),
    ]);
    let identity = cli.event("guest_identity").await;
    let host = Host::start(HostConfig {
        secret_key: host_key,
        bind_addr: bind,
        session_id: "cli-tcp".into(),
        services: vec![Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: f.address,
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        }],
        leases: vec![Lease {
            lease_id: "manual-lab".into(),
            peer: identity["peer"].as_str().unwrap().parse().unwrap(),
            expires_at_unix_ms: unix_millis() + 30_000,
            services: BTreeSet::from(["game".into()]),
        }],
        limits: Limits::default(),
    })
    .await
    .unwrap();
    cli.send("ready");
    let ready = cli.event("guest_ready").await;
    assert_eq!(ready["mode"], "tcp");
    assert!(ready.get("udp").is_none());
    assert!(ready.get("maxDatagramPayload").is_none());
    let address: SocketAddr = ready["tcp"].as_str().unwrap().parse().unwrap();
    let result = probe_minecraft_status(request(address)).await.unwrap();
    assert!(result.protocol_matches_requested);
    assert_eq!(host.metrics().destination_tcp_connects, 1);
    assert_eq!(host.metrics().udp_associations_opened, 0);
    assert_eq!(host.metrics().denied, 0);
    f.finish().await;
    cli.send("stop");
    cli.event("guest_stopped").await;
    cli.exited(true).await;
    assert!(TcpStream::connect(address).await.is_err());
    host.shutdown().await;
}

#[tokio::test]
async fn tcp_only_guest_cli_rejects_bad_arity_public_bind_and_lifetime_before_identity() {
    let peer = generate_key().public().to_string();
    for args in [
        vec!["guest-tcp".into()],
        vec![
            "guest-tcp".into(),
            peer.clone(),
            "127.0.0.1:42000".into(),
            "cli-tcp".into(),
            "0.0.0.0:0".into(),
            "30".into(),
        ],
        vec![
            "guest-tcp".into(),
            peer,
            "127.0.0.1:42000".into(),
            "cli-tcp".into(),
            "127.0.0.1:0".into(),
            "3601".into(),
        ],
    ] {
        let mut cli = CliChild::start(&args);
        cli.event("failed").await;
        cli.exited(false).await;
    }
}
