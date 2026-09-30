//! Explicit synthetic endpoint/probe for the existing, approved loopback bridges.
//! The control protocol belongs only to this byte fixture, never to Minecraft,
//! a voice mod, or the Connect transport. No destinations arrive over that wire.
use serde_json::json;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::{Mutex, oneshot},
    task::JoinSet,
    time::{Instant, sleep, timeout},
};

const MAGIC: &[u8; 8] = b"ACBP0001";
const CONTROL: u8 = 1;
const LOAD: u8 = 2;
const SNAPSHOT: u8 = 1;
const TCP_BYTES: usize = 1_048_576;
const CHUNK: usize = 32768;
const QUIET: Duration = Duration::from_millis(500);
const PACE: Duration = Duration::from_millis(200);
const DEADLINE: Duration = Duration::from_secs(60);
const SNAPSHOT_BYTES: usize = 12 * 8;

#[derive(Debug, thiserror::Error)]
pub(super) enum BoundaryError {
    #[error("boundary_invalid_arguments")]
    Arguments,
    #[error("boundary_fixture_protocol")]
    Protocol,
    #[error("boundary_io_failed")]
    Io,
    #[error("boundary_deadline")]
    Deadline,
    #[error("boundary_cancelled")]
    Cancelled,
    #[error("boundary_tcp_integrity")]
    Tcp,
    #[error("boundary_udp_integrity")]
    Udp,
    #[error("boundary_oversize_delivered")]
    Oversize,
    #[error("boundary_endpoint_summary")]
    Summary,
    #[error("boundary_tcp_overlap_missing")]
    Overlap,
}

impl From<std::io::Error> for BoundaryError {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}
type Result<T> = std::result::Result<T, BoundaryError>;

fn endpoint(value: &str, allow_zero: bool) -> Result<SocketAddr> {
    let address: SocketAddr = value.parse().map_err(|_| BoundaryError::Arguments)?;
    // Literal-only parser: no DNS, wildcard, LAN destinations or IPv6 scope.
    if !address.ip().is_loopback()
        || (!allow_zero && address.port() == 0)
        || matches!(address, SocketAddr::V6(addr) if addr.scope_id() != 0)
    {
        return Err(BoundaryError::Arguments);
    }
    Ok(address)
}

fn cap(value: &str) -> Result<usize> {
    let value = value
        .parse::<usize>()
        .map_err(|_| BoundaryError::Arguments)?;
    if !(1..=1400).contains(&value) {
        return Err(BoundaryError::Arguments);
    }
    Ok(value)
}

fn packet(source: usize, phase: usize, length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| ((index * 31 + source * 13 + phase * 17) % 251) as u8)
        .collect()
}

// Fixed-size response, bounded counters only. No addresses/identity/payload/logs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Stats {
    cap: u64,
    udp_valid: u64,
    udp_bytes: u64,
    oversize_requests: u64,
    unexpected_udp: u64,
    oversize_emitted: u64,
    valid_emitted: u64,
    origins: u64,
    tcp_accepts: u64,
    tcp_bytes: u64,
    tcp_completed: u64,
    protocol_errors: u64,
}
impl Stats {
    fn values(self) -> [u64; 12] {
        [
            self.cap,
            self.udp_valid,
            self.udp_bytes,
            self.oversize_requests,
            self.unexpected_udp,
            self.oversize_emitted,
            self.valid_emitted,
            self.origins,
            self.tcp_accepts,
            self.tcp_bytes,
            self.tcp_completed,
            self.protocol_errors,
        ]
    }
    fn encode(self) -> [u8; SNAPSHOT_BYTES] {
        let mut bytes = [0; SNAPSHOT_BYTES];
        for (value, target) in self.values().into_iter().zip(bytes.as_chunks_mut::<8>().0) {
            target.copy_from_slice(&value.to_be_bytes());
        }
        bytes
    }
    fn decode(bytes: [u8; SNAPSHOT_BYTES]) -> Self {
        let mut values = [0u64; 12];
        for (value, source) in values.iter_mut().zip(bytes.as_chunks::<8>().0) {
            *value = u64::from_be_bytes(*source);
        }
        Self {
            cap: values[0],
            udp_valid: values[1],
            udp_bytes: values[2],
            oversize_requests: values[3],
            unexpected_udp: values[4],
            oversize_emitted: values[5],
            valid_emitted: values[6],
            origins: values[7],
            tcp_accepts: values[8],
            tcp_bytes: values[9],
            tcp_completed: values[10],
            protocol_errors: values[11],
        }
    }
}

#[derive(Default)]
struct Fixture {
    stats: Stats,
    seen: [[bool; 2]; 2],
    sources: [Option<SocketAddr>; 2],
}
impl Fixture {
    fn begin(&mut self, cap: usize) -> Result<()> {
        if !(1..=1400).contains(&cap) || self.stats.cap != 0 {
            return Err(BoundaryError::Protocol);
        }
        self.stats.cap = cap as u64;
        Ok(())
    }
    fn receive(&mut self, bytes: &[u8], origin: SocketAddr) -> Option<(usize, usize)> {
        if self.stats.cap == 0 || !origin.ip().is_loopback() {
            return None;
        }
        let cap = self.stats.cap as usize;
        if bytes.len() > cap {
            self.stats.oversize_requests = self.stats.oversize_requests.saturating_add(1);
            return None;
        }
        for source in 0..2 {
            for (slot, phase) in [0, 2].into_iter().enumerate() {
                if bytes != packet(source, phase, cap) {
                    continue;
                }
                if self.seen[source][slot]
                    || (slot == 1 && !self.seen[source][0])
                    || self.sources[source].is_some_and(|value| value != origin)
                    || self.sources[1 - source] == Some(origin)
                {
                    self.stats.unexpected_udp = self.stats.unexpected_udp.saturating_add(1);
                    return None;
                }
                self.seen[source][slot] = true;
                self.sources[source] = Some(origin);
                self.stats.origins =
                    self.sources.iter().filter(|value| value.is_some()).count() as u64;
                self.stats.udp_valid += 1;
                self.stats.udp_bytes += cap as u64;
                return Some((source, phase));
            }
        }
        self.stats.unexpected_udp = self.stats.unexpected_udp.saturating_add(1);
        None
    }
}

async fn serve_tcp(mut stream: TcpStream, fixture: Arc<Mutex<Fixture>>) -> Result<()> {
    let mut magic = [0; 8];
    stream.read_exact(&mut magic).await?;
    if &magic != MAGIC {
        return Err(BoundaryError::Protocol);
    }
    match stream.read_u8().await? {
        CONTROL => {
            let cap = stream.read_u16().await? as usize;
            fixture.lock().await.begin(cap)?;
            stream.write_all(&[CONTROL]).await?;
            // One control connection; at most32 snapshots. EOF is normal cleanup.
            for _ in 0..32 {
                let command = match stream.read_u8().await {
                    Ok(command) => command,
                    Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                        return Ok(());
                    }
                    Err(_) => return Err(BoundaryError::Io),
                };
                if command != SNAPSHOT {
                    return Err(BoundaryError::Protocol);
                }
                let snapshot = fixture.lock().await.stats.encode();
                stream.write_all(&snapshot).await?;
            }
            Err(BoundaryError::Protocol)
        }
        LOAD => {
            {
                let mut state = fixture.lock().await;
                if state.stats.cap == 0 || state.stats.tcp_accepts != 0 {
                    return Err(BoundaryError::Protocol);
                }
                state.stats.tcp_accepts = 1;
            }
            let mut count = 0;
            let mut bytes = vec![0; CHUNK];
            loop {
                let n = stream.read(&mut bytes).await?;
                if n == 0 {
                    break;
                }
                if count + n > TCP_BYTES
                    || bytes[..n]
                        .iter()
                        .enumerate()
                        .any(|(index, byte)| *byte != ((count + index) % 251) as u8)
                {
                    return Err(BoundaryError::Tcp);
                }
                stream.write_all(&bytes[..n]).await?;
                count += n;
                fixture.lock().await.stats.tcp_bytes = count as u64;
            }
            if count != TCP_BYTES {
                return Err(BoundaryError::Tcp);
            }
            stream.shutdown().await?;
            fixture.lock().await.stats.tcp_completed = 1;
            Ok(())
        }
        _ => Err(BoundaryError::Protocol),
    }
}

pub(super) async fn echo(args: &[String]) -> super::Result<()> {
    if args.len() != 3 {
        return Err(BoundaryError::Arguments.into());
    }
    // Parse every argument before the first bind. Zero ports select owned,
    // ephemeral fixture ports; probe itself always requires nonzero ports.
    let tcp_addr = endpoint(&args[0], true)?;
    let udp_addr = endpoint(&args[1], true)?;
    let seconds = super::duration(&args[2]).map_err(|_| BoundaryError::Arguments)?;
    let tcp = TcpListener::bind(tcp_addr)
        .await
        .map_err(BoundaryError::from)?;
    let udp = UdpSocket::bind(udp_addr)
        .await
        .map_err(BoundaryError::from)?;
    super::event(
        json!({"event":"echo_ready", "tcp":tcp.local_addr()?.to_string(),
        "udp":udp.local_addr()?.to_string(), "experimental":true}),
    );
    let fixture = Arc::new(Mutex::new(Fixture::default()));
    let mut tasks = JoinSet::new();
    let mut accepted = 0;
    let mut buffer = vec![0; 65536];
    let stop = super::until_stop(seconds);
    tokio::pin!(stop);
    let operation = async {
        loop {
            tokio::select! {
                result = tcp.accept() => {
                    let (stream, _) = result?;
                    if tasks.len() < 8 && accepted < 32 {
                        accepted += 1;
                        let state = fixture.clone();
                        tasks.spawn(async move {
                            let result = timeout(DEADLINE, serve_tcp(stream, state.clone())).await;
                            if !matches!(result, Ok(Ok(()))) {
                                let mut fixture = state.lock().await;
                                fixture.stats.protocol_errors = fixture.stats.protocol_errors.saturating_add(1);
                            }
                        });
                    }
                },
                result = udp.recv_from(&mut buffer) => {
                    let (n, origin) = result?;
                    let reply = fixture.lock().await.receive(&buffer[..n], origin);
                    if let Some((source, phase)) = reply {
                        // Fixed owned return. Requires an approved
                        // authorizedDownstream transport service, not1:1credit.
                        let cap = fixture.lock().await.stats.cap as usize;
                        let oversized = packet(source, phase + 3, cap + 1);
                        if udp.send_to(&oversized, origin).await? != oversized.len() {
                            return Err(BoundaryError::Io);
                        }
                        fixture.lock().await.stats.oversize_emitted += 1;
                        if udp.send_to(&buffer[..n], origin).await? != n {
                            return Err(BoundaryError::Io);
                        }
                        fixture.lock().await.stats.valid_emitted += 1;
                    }
                },
                _ = tasks.join_next(), if !tasks.is_empty() => {},
            }
        }
    };
    // Stop/deadline remains polled even while a UDP send or mutex is pending.
    let result = tokio::select! {
        _ = &mut stop => Ok::<_, BoundaryError>(()),
        result = operation => result,
    };
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    drop(tcp);
    drop(udp);
    result?;
    super::event(json!({"event":"echo_stopped"}));
    Ok(())
}

async fn connect(address: SocketAddr, role: u8) -> Result<TcpStream> {
    let mut stream = TcpStream::connect(address).await?;
    stream.set_nodelay(true)?;
    stream.write_all(MAGIC).await?;
    stream.write_u8(role).await?;
    Ok(stream)
}
async fn snapshot(control: &mut TcpStream) -> Result<Stats> {
    control.write_u8(SNAPSHOT).await?;
    let mut bytes = [0; SNAPSHOT_BYTES];
    control.read_exact(&mut bytes).await?;
    Ok(Stats::decode(bytes))
}
fn endpoint_healthy(stats: Stats, cap: usize) -> Result<()> {
    if stats.oversize_requests != 0 {
        return Err(BoundaryError::Oversize);
    }
    if stats.cap != cap as u64 || stats.unexpected_udp != 0 || stats.protocol_errors != 0 {
        return Err(BoundaryError::Summary);
    }
    Ok(())
}

async fn tcp_load(
    mut stream: TcpStream,
    progressed: oneshot::Sender<()>,
    udp_done: oneshot::Receiver<()>,
) -> Result<()> {
    let (mut read, mut write) = stream.split();
    let send = async {
        let bytes: Vec<u8> = (0..TCP_BYTES).map(|index| (index % 251) as u8).collect();
        for chunk in bytes[..TCP_BYTES - 3 * CHUNK].as_chunks::<CHUNK>().0 {
            write.write_all(chunk).await?;
            sleep(PACE).await;
        }
        // Retain a live, non-half-closed load during all UDP checks. This is
        // paced synthetic traffic, not a saturation/throughput measurement.
        udp_done.await.map_err(|_| BoundaryError::Cancelled)?;
        write.write_all(&bytes[TCP_BYTES - 3 * CHUNK..]).await?;
        write.shutdown().await?;
        Ok::<_, BoundaryError>(())
    };
    let receive = async {
        let mut progressed = Some(progressed);
        let mut count = 0;
        let mut bytes = vec![0; CHUNK];
        loop {
            let n = read.read(&mut bytes).await?;
            if n == 0 {
                break;
            }
            if count + n > TCP_BYTES
                || bytes[..n]
                    .iter()
                    .enumerate()
                    .any(|(index, byte)| *byte != ((count + index) % 251) as u8)
            {
                return Err(BoundaryError::Tcp);
            }
            count += n;
            if count >= CHUNK
                && let Some(progressed) = progressed.take()
            {
                let _ = progressed.send(());
            }
        }
        if count != TCP_BYTES {
            return Err(BoundaryError::Tcp);
        }
        Ok(())
    };
    tokio::try_join!(send, receive)?;
    Ok(())
}

// Drain throughout every observation window and fail on any unexpected packet,
// including a delayed/duplicate/oversized response. Sockets stay owned/open
// through the final window. Silence alone is never the rejection oracle:
// snapshots also assert that our endpoint saw no oversized request.
async fn quiet(sockets: &[UdpSocket; 2]) -> Result<()> {
    let mut first = vec![0; 65536];
    let mut second = vec![0; 65536];
    tokio::select! {
        _ = sleep(QUIET) => Ok(()),
        result = sockets[0].recv(&mut first) => { result?; Err(BoundaryError::Udp) },
        result = sockets[1].recv(&mut second) => { result?; Err(BoundaryError::Udp) },
    }
}
async fn exact(sockets: &[UdpSocket; 2], source: usize, phase: usize, cap: usize) -> Result<()> {
    let expected = packet(source, phase, cap);
    if sockets[source].send(&expected).await? != expected.len() {
        return Err(BoundaryError::Io);
    }
    let mut bytes = vec![0; 65536];
    let mut other = vec![0; 65536];
    tokio::select! {
        result = sockets[source].recv(&mut bytes) => {
            let n = result?;
            if bytes[..n] != expected { return Err(BoundaryError::Udp); }
        },
        result = sockets[1 - source].recv(&mut other) => {
            result?;
            return Err(BoundaryError::Udp);
        },
    }
    Ok(())
}

async fn run_probe(tcp: SocketAddr, udp: SocketAddr, cap: usize) -> Result<serde_json::Value> {
    let started = Instant::now();
    let mut control = connect(tcp, CONTROL).await?;
    control.write_u16(cap as u16).await?;
    if control.read_u8().await? != CONTROL {
        return Err(BoundaryError::Protocol);
    }
    let loopback = if udp.is_ipv4() {
        "127.0.0.1:0"
    } else {
        "[::1]:0"
    };
    let sockets = [
        UdpSocket::bind(loopback).await?,
        UdpSocket::bind(loopback).await?,
    ];
    for socket in &sockets {
        socket.connect(udp).await?;
    }
    let load = connect(tcp, LOAD).await?;
    let (progress_tx, progress_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let tcp = tcp_load(load, progress_tx, done_rx);
    let udp = async {
        progress_rx.await.map_err(|_| BoundaryError::Tcp)?;
        let before = snapshot(&mut control).await?;
        endpoint_healthy(before, cap)?;
        if before.tcp_bytes < CHUNK as u64 || before.tcp_bytes >= TCP_BYTES as u64 {
            return Err(BoundaryError::Overlap);
        }
        for source in 0..2 {
            exact(&sockets, source, 0, cap).await?;
            let observed = snapshot(&mut control).await?;
            endpoint_healthy(observed, cap)?;
            if observed.udp_valid != (source * 2 + 1) as u64 {
                return Err(BoundaryError::Summary);
            }
            let oversized = packet(source, 1, cap + 1);
            if sockets[source].send(&oversized).await? != oversized.len() {
                return Err(BoundaryError::Io);
            }
            quiet(&sockets).await?;
            let rejected = snapshot(&mut control).await?;
            endpoint_healthy(rejected, cap)?;
            if rejected.udp_valid != observed.udp_valid || rejected.udp_bytes != observed.udp_bytes
            {
                return Err(BoundaryError::Summary);
            }
            exact(&sockets, source, 2, cap).await?;
        }
        quiet(&sockets).await?;
        let after = snapshot(&mut control).await?;
        endpoint_healthy(after, cap)?;
        if after.tcp_bytes <= before.tcp_bytes || after.tcp_bytes >= TCP_BYTES as u64 {
            return Err(BoundaryError::Overlap);
        }
        done_tx.send(()).map_err(|_| BoundaryError::Tcp)?;
        Ok::<_, BoundaryError>(after.tcp_bytes - before.tcp_bytes)
    };
    let (_, overlap) = tokio::try_join!(tcp, udp)?;
    quiet(&sockets).await?;
    let final_stats = snapshot(&mut control).await?;
    endpoint_healthy(final_stats, cap)?;
    if final_stats
        != (Stats {
            cap: cap as u64,
            udp_valid: 4,
            udp_bytes: (4 * cap) as u64,
            oversize_emitted: 4,
            valid_emitted: 4,
            origins: 2,
            tcp_accepts: 1,
            tcp_bytes: TCP_BYTES as u64,
            tcp_completed: 1,
            ..Stats::default()
        })
    {
        return Err(BoundaryError::Summary);
    }
    control.shutdown().await?;
    Ok(json!({"event":"boundary_probe_passed", "experimental":true,
        "expectedPayload":cap, "tcpBytesEachDirection":TCP_BYTES,
        "tcpConcurrentWithUdp":true, "tcpBytesForwardedDuringUdp":overlap,
        "tcpPacingMillis":PACE.as_millis(), "udpLocalSources":2,
        "udpHostAssociationsObserved":final_stats.origins, "udpExactEchoes":4,
        "udpOversizeRequests":2, "udpOversizeBytes":cap+1, "udpOversizeReachedEndpoint":0,
        "udpOversizeReturnsEmitted":final_stats.oversize_emitted, "udpOversizeReturnsReachedGuest":0,
        "udpAuthorizedDownstreamRequired":true, "udpValidAfterRejection":2,
        "rejectionObservationMs":QUIET.as_millis(), "udpEndpointBytes":final_stats.udp_bytes,
        "elapsedMs":started.elapsed().as_millis(), "relay":false,
        "minecraft":"not_tested", "voice":"not_tested", "physicalPathMtu":"not_measured",
        "externalNetwork":"not_inferred"}))
}

pub(super) async fn probe(args: &[String]) -> super::Result<()> {
    if args.len() != 3 {
        return Err(BoundaryError::Arguments.into());
    }
    let tcp = endpoint(&args[0], false)?;
    let udp = endpoint(&args[1], false)?;
    let cap = cap(&args[2])?;
    let deadline = Instant::now() + DEADLINE;
    let result = tokio::select! {
        result = timeout(DEADLINE, run_probe(tcp, udp, cap)) =>
            result.map_err(|_| BoundaryError::Deadline)?,
        _ = tokio::signal::ctrl_c() => Err(BoundaryError::Cancelled),
    }?;
    // A ready I/O future must not win a simultaneous expired timer and emit
    // PASS after the monotonic deadline.
    if Instant::now() >= deadline {
        return Err(BoundaryError::Deadline.into());
    }
    super::event(result);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_literal_loopback_and_complete_safe_cap_before_io() {
        for value in ["0", "1401", "65507", "-1", "1.5", "PRIVATE_CAP"] {
            assert!(matches!(cap(value), Err(BoundaryError::Arguments)));
        }
        for value in ["1", "64", "1264", "1400"] {
            assert!(cap(value).is_ok());
        }
        for value in [
            "localhost:1",
            "0.0.0.0:1",
            "192.0.2.1:1",
            "[::]:1",
            "[::1%1]:1",
            "127.0.0.1:0",
        ] {
            assert!(endpoint(value, false).is_err());
        }
        assert!(endpoint("127.0.0.1:0", true).is_ok());
        assert!(endpoint("[::1]:7", false).is_ok());
    }

    #[test]
    fn four_patterns_remain_distinct_even_at_one_byte_and_max_cap() {
        for cap in [1, 64, 1264, 1400] {
            let values: Vec<_> = (0..2)
                .flat_map(|source| [0, 2].map(move |phase| packet(source, phase, cap)))
                .collect();
            for (index, bytes) in values.iter().enumerate() {
                assert_eq!(bytes.len(), cap);
                assert!(values[index + 1..].iter().all(|other| bytes != other));
            }
        }
    }

    #[test]
    fn fixture_tracks_exact_two_origins_and_oversize_without_truncation() {
        for cap in [1, 1264, 1400] {
            let mut fixture = Fixture::default();
            fixture.begin(cap).unwrap();
            for source in 0..2 {
                let address = format!("127.0.0.1:{}", 35000 + source).parse().unwrap();
                assert_eq!(
                    fixture.receive(&packet(source, 0, cap), address),
                    Some((source, 0))
                );
                assert_eq!(fixture.receive(&packet(source, 1, cap + 1), address), None);
                assert_eq!(
                    fixture.receive(&packet(source, 2, cap), address),
                    Some((source, 2))
                );
            }
            assert_eq!(fixture.stats.udp_valid, 4);
            assert_eq!(fixture.stats.udp_bytes, (4 * cap) as u64);
            assert_eq!(fixture.stats.origins, 2);
            assert_eq!(fixture.stats.oversize_requests, 2);
            assert_eq!(fixture.stats.unexpected_udp, 0);
            assert!(matches!(
                endpoint_healthy(fixture.stats, cap),
                Err(BoundaryError::Oversize)
            ));
        }
    }

    #[test]
    fn fixture_refuses_repeated_phases_origin_reuse_and_unknown_bytes() {
        let mut fixture = Fixture::default();
        let first = "127.0.0.1:35000".parse().unwrap();
        let second = "127.0.0.1:35001".parse().unwrap();
        assert_eq!(fixture.receive(&packet(0, 0, 64), first), None);
        fixture.begin(64).unwrap();
        assert!(fixture.begin(64).is_err());
        assert_eq!(fixture.receive(&packet(0, 2, 64), first), None);
        assert_eq!(fixture.receive(&packet(0, 0, 64), first), Some((0, 0)));
        assert_eq!(fixture.receive(&packet(0, 0, 64), first), None);
        assert_eq!(fixture.receive(&packet(1, 0, 64), first), None);
        assert_eq!(fixture.receive(&packet(0, 2, 64), second), None);
        assert_eq!(fixture.receive(&[255; 64], first), None);
        assert_eq!(fixture.stats.unexpected_udp, 5);
        assert_eq!(fixture.stats.udp_valid, 1);
    }

    #[test]
    fn snapshot_has_fixed_framing_and_preserves_bounded_counter_values() {
        let stats = Stats {
            cap: 1264,
            udp_valid: 4,
            udp_bytes: 5056,
            oversize_requests: u64::MAX,
            unexpected_udp: 6,
            oversize_emitted: 4,
            valid_emitted: 4,
            origins: 2,
            tcp_accepts: 1,
            tcp_bytes: TCP_BYTES as u64,
            tcp_completed: 1,
            protocol_errors: 0,
        };
        assert_eq!(stats.encode().len(), 96);
        assert_eq!(Stats::decode(stats.encode()), stats);
    }
}
