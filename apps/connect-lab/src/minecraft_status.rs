//! Bounded Java Edition server-list status over an explicitly supplied local TCP listener.
//! This observes a protocol response, not a player joining or a verified Minecraft runtime.
//! No DNS, login, account, discovery, JSON text, icon or player identity is exposed.
//! A loopback listener alone does not prove authenticated Aurora transport;
//! callers must obtain that evidence separately from the Host/Guest lifecycle.
//!
//! Wire reference: GeyserMC/MCProtocolLib commit
//! 19783c29ece24bc3f07f8ff08628549527e3de20, ClientIntentionPacket,
//! ClientboundStatusResponsePacket, ServerboundPingRequestPacket and
//! ClientboundPongResponsePacket. STATUS uses packet IDs 0 (response) and 1 (pong).
//! The same IDs and protocol 767 appear in the 1.21-1 codec at commit
//! e71d7df8c127b7196fd81dd392034d7d97acbb4e. The request is still explicit.
use serde::{Deserialize, Deserializer, Serialize, de::IgnoredAny};
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{Instant, timeout},
};

pub const MAX_MINECRAFT_STATUS_JSON_BYTES: usize = 64 * 1024;
/// MinecraftTypes.readString defaults to Short.MAX_VALUE Java UTF-16 units.
pub const MAX_MINECRAFT_STATUS_UTF16_UNITS: usize = 32767;
pub const MAX_MINECRAFT_STATUS_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_STATUS_FRAME_BYTES: usize = MAX_MINECRAFT_STATUS_JSON_BYTES + 6;

/// The address is the local bridge listener. The handshake address is opaque
/// protocol data for virtual hosting; it is never resolved or used for dialing.
pub struct MinecraftStatusRequest {
    pub endpoint: SocketAddr,
    pub handshake_host: String,
    pub handshake_port: u16,
    /// An explicit positive protocol. The generic discovery value -1 is unsupported.
    pub protocol: i32,
    pub timeout: Duration,
}

/// All values originate from the status exchange. Matching a reported protocol
/// does not establish mod compatibility, login success, world entry or voice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftStatusSummary {
    pub requested_protocol: i32,
    pub reported_protocol: i32,
    pub protocol_matches_requested: bool,
    pub reported_players_online: Option<u32>,
    pub reported_players_max: Option<u32>,
    pub connect_micros: u64,
    pub status_micros: u64,
    pub ping_micros: u64,
    pub total_micros: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum MinecraftStatusError {
    #[error("invalid_status_request")]
    InvalidRequest,
    #[error("status_connection_failed")]
    ConnectionFailed,
    #[error("status_io_failed")]
    Io,
    #[error("status_timed_out")]
    Timeout,
    #[error("invalid_status_varint")]
    InvalidVarInt,
    #[error("status_frame_too_large")]
    FrameTooLarge,
    #[error("truncated_status_response")]
    Truncated,
    #[error("invalid_status_packet")]
    InvalidPacket,
    #[error("invalid_status_utf8")]
    InvalidUtf8,
    #[error("status_string_too_long")]
    StringTooLong,
    #[error("invalid_status_json")]
    InvalidJson,
    #[error("invalid_status_schema")]
    InvalidSchema,
    #[error("status_metadata_incomplete")]
    MetadataIncomplete,
    #[error("status_pong_mismatch")]
    PongMismatch,
}

type Result<T> = std::result::Result<T, MinecraftStatusError>;

fn validate(request: &MinecraftStatusRequest) -> Result<()> {
    let host = &request.handshake_host;
    // Deliberately excludes NUL-separated proxy/Forge forwarding, whitespace,
    // slashes, schemes, credentials and scope IDs. ASCII DNS names and IP
    // literals are accepted as handshake data, including bracketed IPv6.
    if !request.endpoint.ip().is_loopback()
        || request.endpoint.port() == 0
        || request.handshake_port == 0
        || request.protocol <= 0
        || request.timeout.is_zero()
        || request.timeout > MAX_MINECRAFT_STATUS_TIMEOUT
        || host.is_empty()
        || host.len() > 253
        || !valid_handshake_host(host)
    {
        return Err(MinecraftStatusError::InvalidRequest);
    }
    Ok(())
}

fn valid_handshake_host(host: &str) -> bool {
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    if let Some(inner) = host.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return inner.parse::<std::net::Ipv6Addr>().is_ok();
    }
    // DNS label syntax only; no DNS query is made. A terminal dot is allowed.
    host.strip_suffix('.')
        .unwrap_or(host)
        .split('.')
        .all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn io_error(error: std::io::Error) -> MinecraftStatusError {
    if error.kind() == std::io::ErrorKind::UnexpectedEof {
        MinecraftStatusError::Truncated
    } else {
        MinecraftStatusError::Io
    }
}

#[derive(Default)]
struct VarInt {
    value: u32,
    bytes: u32,
}
impl VarInt {
    fn push(&mut self, byte: u8) -> Result<Option<i32>> {
        if self.bytes >= 5 || (self.bytes == 4 && byte > 0x0f) {
            return Err(MinecraftStatusError::InvalidVarInt);
        }
        self.value |= u32::from(byte & 0x7f) << (self.bytes * 7);
        self.bytes += 1;
        if byte & 0x80 != 0 {
            return Ok(None);
        }
        // Reject redundant encodings, in addition to 32-bit overflow. Negative
        // signed values need five bytes; callers reject negative lengths/IDs.
        if self.bytes > 1 && self.value < (1_u32 << ((self.bytes - 1) * 7)) {
            return Err(MinecraftStatusError::InvalidVarInt);
        }
        Ok(Some(self.value as i32))
    }
}

async fn read_varint(socket: &mut TcpStream) -> Result<i32> {
    let mut value = VarInt::default();
    loop {
        let byte = socket.read_u8().await.map_err(io_error)?;
        if let Some(value) = value.push(byte)? {
            return Ok(value);
        }
    }
}

fn take_varint(bytes: &mut &[u8]) -> Result<i32> {
    let mut value = VarInt::default();
    loop {
        let (byte, rest) = bytes
            .split_first()
            .ok_or(MinecraftStatusError::InvalidPacket)?;
        *bytes = rest;
        if let Some(value) = value.push(*byte)? {
            return Ok(value);
        }
    }
}

fn put_varint(bytes: &mut Vec<u8>, value: i32) {
    let mut value = value as u32;
    while value >= 0x80 {
        bytes.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
}

async fn read_frame(socket: &mut TcpStream, maximum: usize) -> Result<Vec<u8>> {
    let length = read_varint(socket).await?;
    if length <= 0 {
        return Err(MinecraftStatusError::InvalidPacket);
    }
    if length as usize > maximum {
        return Err(MinecraftStatusError::FrameTooLarge);
    }
    // The peer-supplied bound has been checked before allocating or reading.
    let mut bytes = vec![0; length as usize];
    socket.read_exact(&mut bytes).await.map_err(io_error)?;
    Ok(bytes)
}

fn append_frame(target: &mut Vec<u8>, body: &[u8]) {
    put_varint(target, body.len() as i32);
    target.extend_from_slice(body);
}

// Known fields are typed and duplicate known keys are rejected by serde. Extra
// mod-specific fields are skipped. Strings and identities never reach the
// result or an error; there is no Debug implementation on these temporary data.
#[derive(Deserialize)]
struct VersionInfo {
    #[serde(rename = "name")]
    _name: String,
    protocol: i32,
}
#[derive(Deserialize)]
struct PlayerSample {
    #[serde(rename = "name")]
    _name: String,
    #[serde(rename = "id")]
    _id: String,
}
#[derive(Deserialize)]
struct PlayersInfo {
    online: u32,
    max: u32,
    #[serde(default, rename = "sample")]
    _sample: Optional<Vec<PlayerSample>>,
}

// Missing optional fields are accepted; explicit null or the wrong type is not.
struct Optional<T>(Option<T>);
impl<T> Default for Optional<T> {
    fn default() -> Self {
        Self(None)
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Optional<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        T::deserialize(deserializer).map(|value| Self(Some(value)))
    }
}

struct Description;
impl<'de> Deserialize<'de> for Description {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Description;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a component string, object or array")
            }
            fn visit_str<E: serde::de::Error>(
                self,
                _: &str,
            ) -> std::result::Result<Description, E> {
                Ok(Description)
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Description, A::Error> {
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(Description)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Description, A::Error> {
                while seq.next_element::<Description>()?.is_some() {}
                Ok(Description)
            }
        }
        // Validate the component's outer shape, without interpreting rich text
        // events, URLs, assets or the Minecraft component dialect.
        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Deserialize)]
struct StatusResponse {
    #[serde(default)]
    version: Optional<VersionInfo>,
    #[serde(default)]
    players: Optional<PlayersInfo>,
    #[serde(default)]
    description: Optional<Description>,
    #[serde(default, rename = "favicon")]
    _favicon: Optional<String>,
    #[serde(default, rename = "enforcesSecureChat")]
    _enforces_secure_chat: Optional<bool>,
    #[serde(default, rename = "previewsChat")]
    _previews_chat: Optional<bool>,
}

fn parse_status(frame: &[u8]) -> Result<(VersionInfo, Option<PlayersInfo>)> {
    let mut bytes = frame;
    if take_varint(&mut bytes)? != 0 {
        return Err(MinecraftStatusError::InvalidPacket);
    }
    let length = take_varint(&mut bytes)?;
    if length < 0 {
        return Err(MinecraftStatusError::InvalidPacket);
    }
    if length as usize > MAX_MINECRAFT_STATUS_JSON_BYTES {
        return Err(MinecraftStatusError::FrameTooLarge);
    }
    if bytes.len() != length as usize {
        return Err(MinecraftStatusError::InvalidPacket);
    }
    let json = std::str::from_utf8(bytes).map_err(|_| MinecraftStatusError::InvalidUtf8)?;
    // Java String.length counts UTF-16 code units, not UTF-8 bytes or Unicode
    // scalar values. The transport byte cap remains independently enforced.
    if json.encode_utf16().count() > MAX_MINECRAFT_STATUS_UTF16_UNITS {
        return Err(MinecraftStatusError::StringTooLong);
    }
    let status: StatusResponse = serde_json::from_str(json).map_err(|error| {
        if error.is_data() {
            MinecraftStatusError::InvalidSchema
        } else {
            MinecraftStatusError::InvalidJson
        }
    })?;
    if !json.trim_start().starts_with('{')
        || status.players.0.as_ref().is_some_and(|players| {
            players.max > i32::MAX as u32 || players.online > i32::MAX as u32
        })
    {
        return Err(MinecraftStatusError::InvalidSchema);
    }
    // Older dedicated servers can answer on their bound listener while world
    // setup is still running. Their serializer omits fields initialized later.
    let (Some(version), Some(_)) = (status.version.0, status.description.0) else {
        return Err(MinecraftStatusError::MetadataIncomplete);
    };
    Ok((version, status.players.0))
}

fn micros(elapsed: Duration) -> u64 {
    elapsed.as_micros().min(u128::from(u64::MAX)) as u64
}

/// Validate first, then perform exactly one connection and one status/ping
/// exchange within the total deadline. Dropping this future cancels the probe
/// and closes its socket; there are no spawned tasks or automatic retries.
pub async fn probe_minecraft_status(
    request: MinecraftStatusRequest,
) -> Result<MinecraftStatusSummary> {
    validate(&request)?;
    timeout(request.timeout, async {
        let started = Instant::now();
        let mut socket = TcpStream::connect(request.endpoint)
            .await
            .map_err(|_| MinecraftStatusError::ConnectionFailed)?;
        socket.set_nodelay(true).map_err(io_error)?;
        let connect_micros = micros(started.elapsed());
        let status_started = Instant::now();
        let mut handshake = Vec::with_capacity(request.handshake_host.len() + 16);
        put_varint(&mut handshake, 0);
        put_varint(&mut handshake, request.protocol);
        put_varint(&mut handshake, request.handshake_host.len() as i32);
        handshake.extend_from_slice(request.handshake_host.as_bytes());
        handshake.extend_from_slice(&request.handshake_port.to_be_bytes());
        put_varint(&mut handshake, 1); // STATUS; never LOGIN or TRANSFER.
        let mut outbound = Vec::with_capacity(handshake.len() + 8);
        append_frame(&mut outbound, &handshake);
        append_frame(&mut outbound, &[0]); // Empty status request.
        socket.write_all(&outbound).await.map_err(io_error)?;
        let frame = read_frame(&mut socket, MAX_STATUS_FRAME_BYTES).await?;
        let (version, players) = parse_status(&frame)?;
        let status_micros = micros(status_started.elapsed());

        // Fresh opaque 64 public bits, not secret-key material or a persistent identity.
        let fresh = crate::generate_key().public();
        let nonce: [u8; 8] = fresh.as_bytes()[..8]
            .try_into()
            .expect("fixed eight-byte slice");
        let mut ping = vec![9, 1]; // Length + packet ID + signed i64 bytes.
        ping.extend_from_slice(&nonce);
        let ping_started = Instant::now();
        socket.write_all(&ping).await.map_err(io_error)?;
        let pong = read_frame(&mut socket, 9).await?;
        if pong.len() != 9 || pong[0] != 1 {
            return Err(MinecraftStatusError::InvalidPacket);
        }
        if pong[1..] != nonce {
            return Err(MinecraftStatusError::PongMismatch);
        }
        let ping_micros = micros(ping_started.elapsed());
        socket.shutdown().await.map_err(io_error)?;
        Ok(MinecraftStatusSummary {
            requested_protocol: request.protocol,
            reported_protocol: version.protocol,
            protocol_matches_requested: version.protocol == request.protocol,
            reported_players_online: players.as_ref().map(|value| value.online),
            reported_players_max: players.as_ref().map(|value| value.max),
            connect_micros,
            status_micros,
            ping_micros,
            total_micros: micros(started.elapsed()),
        })
    })
    .await
    .map_err(|_| MinecraftStatusError::Timeout)?
}
