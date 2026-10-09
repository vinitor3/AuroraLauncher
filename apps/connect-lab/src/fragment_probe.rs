//! Explicit opaque UDP V2 byte fixture. This control protocol belongs only to
//! this lab endpoint, never to Minecraft, voice mods or the Connect wire.
use serde_json::json;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::{Mutex, oneshot},
    task::JoinSet,
    time::{Instant, sleep, timeout},
};

const MAGIC: &[u8; 8] = b"ACFP0002";
const CONTROL: u8 = 1;
const LOAD: u8 = 2;
const SNAPSHOT: u8 = 1;
const MAX_ORIGINAL: usize = 4096;
const OVERSIZE: usize = 4097;
const TCP_BYTES: usize = 1_048_576;
const UDP_BUDGET: usize = 65_536;
const UDP_VALID_EACH_DIRECTION: usize = 15_262;
const UDP_SCHEDULED_DUPLEX: usize = 46_912;
const CHUNK: usize = 32_768;
const SIZES: [usize; 4] = [1427, 0, 2108, 4096];
const XOR: u8 = 0xA5;
const QUIET: Duration = Duration::from_millis(500);
const PACE: Duration = Duration::from_millis(200);
const DEADLINE: Duration = Duration::from_secs(60);
const SNAPSHOT_BYTES: usize = 16 * 8;

#[derive(Debug, thiserror::Error)]
pub(super) enum FragmentProbeError {
    #[error("fragment_probe_invalid_arguments")]
    Arguments,
    #[error("fragment_probe_fixture_protocol")]
    Protocol,
    #[error("fragment_probe_io_failed")]
    Io,
    #[error("fragment_probe_deadline")]
    Deadline,
    #[error("fragment_probe_cancelled")]
    Cancelled,
    #[error("fragment_probe_tcp_integrity")]
    Tcp,
    #[error("fragment_probe_udp_integrity")]
    Udp,
    #[error("fragment_probe_oversize_delivered")]
    Oversize,
    #[error("fragment_probe_endpoint_summary")]
    Summary,
    #[error("fragment_probe_tcp_overlap_missing")]
    Overlap,
    #[error("fragment_probe_budget_exceeded")]
    Budget,
}
impl From<std::io::Error> for FragmentProbeError {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}
type Result<T> = std::result::Result<T, FragmentProbeError>;

fn endpoint(value: &str, zero: bool) -> Result<SocketAddr> {
    let address: SocketAddr = value.parse().map_err(|_| FragmentProbeError::Arguments)?;
    if !address.ip().is_loopback()
        || (!zero && address.port() == 0)
        || matches!(address, SocketAddr::V6(addr) if addr.scope_id() != 0 || addr.flowinfo() != 0)
    {
        return Err(FragmentProbeError::Arguments);
    }
    Ok(address)
}
fn packet(source: usize, slot: usize, length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| ((index * 31 + source * 13 + slot * 17) % 251) as u8)
        .collect()
}
fn response(source: usize, slot: usize, length: usize) -> Vec<u8> {
    packet(source, slot, length)
        .into_iter()
        .map(|byte| byte ^ XOR)
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Stats {
    started: u64,
    valid: [u64; 4],
    valid_bytes: u64,
    oversize_requests: u64,
    unexpected_udp: u64,
    valid_emitted: u64,
    oversize_emitted: u64,
    udp_emitted_bytes: u64,
    origins: u64,
    tcp_accepts: u64,
    tcp_bytes: u64,
    tcp_completed: u64,
    protocol_errors: u64,
}
impl Stats {
    fn encode(self) -> [u8; SNAPSHOT_BYTES] {
        let values = [
            self.started,
            self.valid[0],
            self.valid[1],
            self.valid[2],
            self.valid[3],
            self.valid_bytes,
            self.oversize_requests,
            self.unexpected_udp,
            self.valid_emitted,
            self.oversize_emitted,
            self.udp_emitted_bytes,
            self.origins,
            self.tcp_accepts,
            self.tcp_bytes,
            self.tcp_completed,
            self.protocol_errors,
        ];
        let mut bytes = [0; SNAPSHOT_BYTES];
        for (value, target) in values.into_iter().zip(bytes.as_chunks_mut::<8>().0) {
            target.copy_from_slice(&value.to_be_bytes());
        }
        bytes
    }
    fn decode(bytes: [u8; SNAPSHOT_BYTES]) -> Self {
        let mut values = [0u64; 16];
        for (value, input) in values.iter_mut().zip(bytes.as_chunks::<8>().0) {
            *value = u64::from_be_bytes(*input);
        }
        Self {
            started: values[0],
            valid: [values[1], values[2], values[3], values[4]],
            valid_bytes: values[5],
            oversize_requests: values[6],
            unexpected_udp: values[7],
            valid_emitted: values[8],
            oversize_emitted: values[9],
            udp_emitted_bytes: values[10],
            origins: values[11],
            tcp_accepts: values[12],
            tcp_bytes: values[13],
            tcp_completed: values[14],
            protocol_errors: values[15],
        }
    }
}
#[derive(Default)]
struct Fixture {
    stats: Stats,
    seen: [[bool; 4]; 2],
    sources: [Option<SocketAddr>; 2],
}
impl Fixture {
    fn begin(&mut self) -> Result<()> {
        if self.stats.started != 0 {
            return Err(FragmentProbeError::Protocol);
        }
        self.stats.started = 1;
        Ok(())
    }
    fn receive(&mut self, bytes: &[u8], origin: SocketAddr) -> Option<(usize, usize)> {
        if self.stats.started != 1 || !origin.ip().is_loopback() {
            return None;
        }
        if bytes.len() > MAX_ORIGINAL {
            self.stats.oversize_requests += 1;
            return None;
        }
        for source in 0..2 {
            for (slot, size) in SIZES.into_iter().enumerate() {
                if bytes.len() != size || bytes != packet(source, slot, size) {
                    continue;
                }
                if self.seen[source][slot]
                    || (slot > 0 && !self.seen[source][slot - 1])
                    || self.sources[source].is_some_and(|value| value != origin)
                    || self.sources[1 - source] == Some(origin)
                {
                    continue;
                }
                // The nonempty first packet identifies the origin. Empty UDP
                // is then attributed only by this learned, owned socket.
                if slot > 0 && self.sources[source] != Some(origin) {
                    continue;
                }
                self.seen[source][slot] = true;
                self.sources[source] = Some(origin);
                self.stats.origins = self
                    .sources
                    .iter()
                    .filter(|source| source.is_some())
                    .count() as u64;
                self.stats.valid[slot] += 1;
                self.stats.valid_bytes += size as u64;
                return Some((source, slot));
            }
        }
        self.stats.unexpected_udp += 1;
        None
    }
}

async fn serve_tcp(mut socket: TcpStream, fixture: Arc<Mutex<Fixture>>) -> Result<()> {
    let mut magic = [0u8; 8];
    socket.read_exact(&mut magic).await?;
    if &magic != MAGIC {
        return Err(FragmentProbeError::Protocol);
    }
    match socket.read_u8().await? {
        CONTROL => {
            if socket.read_u16().await? as usize != MAX_ORIGINAL {
                return Err(FragmentProbeError::Protocol);
            }
            fixture.lock().await.begin()?;
            socket.write_u8(CONTROL).await?;
            for _ in 0..32 {
                match socket.read_u8().await {
                    Ok(SNAPSHOT) => {
                        let snapshot = fixture.lock().await.stats.encode();
                        socket.write_all(&snapshot).await?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                        return Ok(());
                    }
                    _ => return Err(FragmentProbeError::Protocol),
                }
            }
            Err(FragmentProbeError::Protocol)
        }
        LOAD => {
            {
                let mut state = fixture.lock().await;
                if state.stats.started != 1 || state.stats.tcp_accepts != 0 {
                    return Err(FragmentProbeError::Protocol);
                }
                state.stats.tcp_accepts = 1;
            }
            let mut count = 0usize;
            let mut bytes = vec![0u8; CHUNK];
            loop {
                let n = socket.read(&mut bytes).await?;
                if n == 0 {
                    break;
                }
                if count + n > TCP_BYTES
                    || bytes[..n]
                        .iter()
                        .enumerate()
                        .any(|(i, byte)| *byte != ((count + i) % 251) as u8)
                {
                    return Err(FragmentProbeError::Tcp);
                }
                count += n;
                // Count validated bytes received at this owned endpoint before
                // echoing them. Receiving that echo can wake the probe before
                // another post-write update would be visible to its snapshot.
                fixture.lock().await.stats.tcp_bytes = count as u64;
                socket.write_all(&bytes[..n]).await?;
            }
            if count != TCP_BYTES {
                return Err(FragmentProbeError::Tcp);
            }
            fixture.lock().await.stats.tcp_completed = 1;
            // Publish validated completion before EOF lets the probe request
            // its final snapshot; otherwise it can observe stale completion.
            socket.shutdown().await?;
            Ok(())
        }
        _ => Err(FragmentProbeError::Protocol),
    }
}

pub(super) async fn echo(args: &[String]) -> super::Result<()> {
    if args.len() != 3 {
        return Err(FragmentProbeError::Arguments.into());
    }
    let tcp_address = endpoint(&args[0], true)?;
    let udp_address = endpoint(&args[1], true)?;
    let seconds = super::duration(&args[2]).map_err(|_| FragmentProbeError::Arguments)?;
    if seconds > 600 {
        return Err(FragmentProbeError::Arguments.into());
    }
    let tcp = TcpListener::bind(tcp_address).await?;
    let udp = UdpSocket::bind(udp_address).await?;
    let tcp_address = tcp.local_addr()?;
    let udp_address = udp.local_addr()?;
    super::event(
        json!({"event":"echo_fragmented_ready","experimental":true,"profile":"fragmentedV2",
        "tcp":tcp_address.to_string(),"udp":udp_address.to_string(),"maxOriginalDatagramBytes":MAX_ORIGINAL,
        "maxFragmentFrameBytes":aurora_connect_lab::MAX_FRAGMENT_FRAME_BYTES,"udpApplicationBudgetBytes":UDP_BUDGET,
        "udpScheduledDuplexBytes":UDP_SCHEDULED_DUPLEX,"udpValidPayloadBytesEachDirection":UDP_VALID_EACH_DIRECTION,
        "tcpBytesEachDirection":TCP_BYTES,"udpReturn":"opaque_xor","payloadContentsEmitted":false,"relay":false,
        "next":"approve only this owned synthetic endpoint; enter stop to close"}),
    );
    let fixture = Arc::new(Mutex::new(Fixture::default()));
    let mut tasks = JoinSet::new();
    let mut accepted = 0usize;
    let mut bytes = vec![0u8; 65536];
    let stop = super::until_stop(seconds);
    tokio::pin!(stop);
    let operation = async {
        loop {
            tokio::select! {
                accepted_socket = tcp.accept() => {
                    let (socket, _) = accepted_socket?;
                    if accepted >= 4 || tasks.len() >= 4 { fixture.lock().await.stats.protocol_errors += 1; continue }
                    accepted += 1;
                    let state = fixture.clone();
                    tasks.spawn(async move {
                        if !matches!(timeout(DEADLINE, serve_tcp(socket, state.clone())).await, Ok(Ok(()))) {
                            state.lock().await.stats.protocol_errors += 1;
                        }
                    });
                }
                received = udp.recv_from(&mut bytes) => {
                    let (n, origin) = received?;
                    let reply = fixture.lock().await.receive(&bytes[..n], origin);
                    if let Some((source, slot)) = reply {
                        // Once per origin, an over-cap return must be rejected by
                        // V2 without a prefix. It never supplies a destination.
                        if slot == 3 {
                            let oversized = response(source, 4, OVERSIZE);
                            if udp.send_to(&oversized, origin).await? != OVERSIZE { return Err(FragmentProbeError::Io) }
                            let mut state = fixture.lock().await;
                            state.stats.oversize_emitted += 1;
                            state.stats.udp_emitted_bytes += OVERSIZE as u64;
                        }
                        let reply = response(source, slot, n);
                        if udp.send_to(&reply, origin).await? != n { return Err(FragmentProbeError::Io) }
                        let mut state = fixture.lock().await;
                        state.stats.valid_emitted += 1;
                        state.stats.udp_emitted_bytes += n as u64;
                        if state.stats.udp_emitted_bytes > (UDP_BUDGET - 23_456) as u64 { return Err(FragmentProbeError::Budget) }
                    }
                }
                _ = tasks.join_next(), if !tasks.is_empty() => {}
            }
        }
    };
    let result = tokio::select! {
        _ = &mut stop => Ok::<(),FragmentProbeError>(()),
        result = operation => result,
    };
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    drop(tcp);
    drop(udp);
    let rebind = TcpListener::bind(tcp_address).await;
    let udp_rebind = UdpSocket::bind(udp_address).await;
    if rebind.is_err() || udp_rebind.is_err() {
        return Err(FragmentProbeError::Io.into());
    }
    drop((rebind, udp_rebind));
    result?;
    super::event(json!({"event":"echo_fragmented_stopped","cleanupRebound":true}));
    Ok(())
}

async fn connect(address: SocketAddr, role: u8) -> Result<TcpStream> {
    let mut socket = TcpStream::connect(address).await?;
    socket.set_nodelay(true)?;
    socket.write_all(MAGIC).await?;
    socket.write_u8(role).await?;
    Ok(socket)
}
async fn snapshot(socket: &mut TcpStream) -> Result<Stats> {
    socket.write_u8(SNAPSHOT).await?;
    let mut bytes = [0u8; SNAPSHOT_BYTES];
    socket.read_exact(&mut bytes).await?;
    Ok(Stats::decode(bytes))
}
fn healthy(stats: Stats) -> Result<()> {
    if stats.oversize_requests != 0 {
        return Err(FragmentProbeError::Oversize);
    }
    if stats.started != 1 || stats.unexpected_udp != 0 || stats.protocol_errors != 0 {
        return Err(FragmentProbeError::Summary);
    }
    Ok(())
}
async fn tcp_load(
    mut socket: TcpStream,
    progress: oneshot::Sender<()>,
    udp_done: oneshot::Receiver<()>,
) -> Result<()> {
    let (mut read, mut write) = socket.split();
    let send = async {
        let bytes: Vec<u8> = (0..TCP_BYTES).map(|n| (n % 251) as u8).collect();
        for chunk in bytes[..TCP_BYTES - 3 * CHUNK].as_chunks::<CHUNK>().0 {
            write.write_all(chunk).await?;
            sleep(PACE).await;
        }
        udp_done.await.map_err(|_| FragmentProbeError::Cancelled)?;
        write.write_all(&bytes[TCP_BYTES - 3 * CHUNK..]).await?;
        write.shutdown().await?;
        Ok::<(), FragmentProbeError>(())
    };
    let receive = async {
        let mut progress = Some(progress);
        let mut total = 0usize;
        let mut bytes = vec![0u8; CHUNK];
        loop {
            let n = read.read(&mut bytes).await?;
            if n == 0 {
                break;
            }
            if total + n > TCP_BYTES
                || bytes[..n]
                    .iter()
                    .enumerate()
                    .any(|(i, byte)| *byte != ((total + i) % 251) as u8)
            {
                return Err(FragmentProbeError::Tcp);
            }
            total += n;
            if total >= CHUNK
                && let Some(sender) = progress.take()
            {
                let _ = sender.send(());
            }
        }
        if total != TCP_BYTES {
            return Err(FragmentProbeError::Tcp);
        }
        Ok(())
    };
    tokio::try_join!(send, receive)?;
    Ok(())
}
async fn quiet(sockets: &[UdpSocket; 2]) -> Result<()> {
    let mut first = vec![0u8; 65536];
    let mut second = vec![0u8; 65536];
    tokio::select! {
        _ = sleep(QUIET) => Ok(()),
        result = sockets[0].recv(&mut first) => { result?; Err(FragmentProbeError::Udp) },
        result = sockets[1].recv(&mut second) => { result?; Err(FragmentProbeError::Udp) },
    }
}
async fn exact(sockets: &[UdpSocket; 2], source: usize, slot: usize) -> Result<()> {
    let expected = response(source, slot, SIZES[slot]);
    let payload = packet(source, slot, SIZES[slot]);
    if sockets[source].send(&payload).await? != payload.len() {
        return Err(FragmentProbeError::Io);
    }
    let mut received = vec![0u8; 65536];
    let mut other = vec![0u8; 65536];
    tokio::select! {
        result = sockets[source].recv(&mut received) => { let n=result?; if received[..n] != expected { return Err(FragmentProbeError::Udp) } },
        result = sockets[1-source].recv(&mut other) => { result?; return Err(FragmentProbeError::Udp) },
    }
    Ok(())
}
async fn run_probe(tcp: SocketAddr, udp: SocketAddr) -> Result<serde_json::Value> {
    let start = Instant::now();
    let mut control = connect(tcp, CONTROL).await?;
    control.write_u16(MAX_ORIGINAL as u16).await?;
    if control.read_u8().await? != CONTROL {
        return Err(FragmentProbeError::Protocol);
    }
    let bind = if udp.is_ipv4() {
        "127.0.0.1:0"
    } else {
        "[::1]:0"
    };
    let sockets = [UdpSocket::bind(bind).await?, UdpSocket::bind(bind).await?];
    let origins = [sockets[0].local_addr()?, sockets[1].local_addr()?];
    for socket in &sockets {
        socket.connect(udp).await?;
    }
    let load = connect(tcp, LOAD).await?;
    let (progress_tx, progress_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let tcp = tcp_load(load, progress_tx, done_rx);
    let udp = async {
        progress_rx.await.map_err(|_| FragmentProbeError::Tcp)?;
        let before = snapshot(&mut control).await?;
        healthy(before)?;
        if before.tcp_bytes < CHUNK as u64 || before.tcp_bytes >= TCP_BYTES as u64 {
            return Err(FragmentProbeError::Overlap);
        }
        for source in 0..2 {
            for slot in 0..4 {
                if slot == 3 {
                    let oversized = packet(source, 4, OVERSIZE);
                    if sockets[source].send(&oversized).await? != OVERSIZE {
                        return Err(FragmentProbeError::Io);
                    }
                    quiet(&sockets).await?;
                    let rejected = snapshot(&mut control).await?;
                    healthy(rejected)?;
                    if rejected.valid_bytes != (source * 7631 + 3535) as u64 {
                        return Err(FragmentProbeError::Summary);
                    }
                }
                exact(&sockets, source, slot).await?;
                quiet(&sockets).await?;
                let observed = snapshot(&mut control).await?;
                healthy(observed)?;
                if observed.valid[slot] != (source + 1) as u64 {
                    return Err(FragmentProbeError::Summary);
                }
            }
        }
        let after = snapshot(&mut control).await?;
        healthy(after)?;
        if after.tcp_bytes <= before.tcp_bytes || after.tcp_bytes >= TCP_BYTES as u64 {
            return Err(FragmentProbeError::Overlap);
        }
        done_tx.send(()).map_err(|_| FragmentProbeError::Tcp)?;
        Ok::<u64, FragmentProbeError>(after.tcp_bytes - before.tcp_bytes)
    };
    let (_, overlap) = tokio::try_join!(tcp, udp)?;
    quiet(&sockets).await?;
    let expected = Stats {
        started: 1,
        valid: [2; 4],
        valid_bytes: UDP_VALID_EACH_DIRECTION as u64,
        valid_emitted: 8,
        oversize_emitted: 2,
        udp_emitted_bytes: 23456,
        origins: 2,
        tcp_accepts: 1,
        tcp_bytes: TCP_BYTES as u64,
        tcp_completed: 1,
        ..Stats::default()
    };
    let final_stats = snapshot(&mut control).await?;
    healthy(final_stats)?;
    if final_stats != expected {
        return Err(FragmentProbeError::Summary);
    }
    control.shutdown().await?;
    drop((control, sockets));
    for origin in origins {
        drop(UdpSocket::bind(origin).await?);
    }
    Ok(
        json!({"event":"fragment_probe_passed","experimental":true,"profile":"fragmentedV2","udpEnvelopeVersion":2,
        "maxOriginalDatagramBytes":MAX_ORIGINAL,"maxFragmentFrameBytes":aurora_connect_lab::MAX_FRAGMENT_FRAME_BYTES,
        "udpOriginalSizes":[0,1427,2108,4096],"udpLocalSources":2,"udpEndpointOriginsObserved":2,"udpExactReturns":8,
        "udpPayloadBytesEachDirection":UDP_VALID_EACH_DIRECTION,"udpValidDuplexBytes":UDP_VALID_EACH_DIRECTION*2,
        "udpIngressOversizeAttempts":2,"udpIngressOversizeBytes":OVERSIZE,"udpOversizeReachedEndpoint":0,
        "udpOversizeReturnsEmitted":2,"udpOversizeReturnsReachedGuest":0,"udpValidAfterRejection":2,
        "udpApplicationBudgetBytes":UDP_BUDGET,"udpScheduledDuplexBytesIncludingOversize":UDP_SCHEDULED_DUPLEX,
        "tcpBytesEachDirection":TCP_BYTES,"tcpConcurrentWithUdp":true,"tcpBytesForwardedDuringUdp":overlap,
        "tcpProgressMeasurement":"validated_bytes_received_at_owned_endpoint",
        "tcpPacingMillis":PACE.as_millis(),"rejectionObservationMs":QUIET.as_millis(),"originsCleanupRebound":true,
        "elapsedMs":start.elapsed().as_millis(),"relay":false,"minecraft":"not_tested","voice":"not_tested",
        "physicalPathMtu":"not_measured","externalNetwork":"not_inferred"}),
    )
}
pub(super) async fn probe(args: &[String]) -> super::Result<()> {
    if args.len() != 2 {
        return Err(FragmentProbeError::Arguments.into());
    }
    let tcp = endpoint(&args[0], false)?;
    let udp = endpoint(&args[1], false)?;
    let deadline = Instant::now() + DEADLINE;
    let result = tokio::select! {
        result = timeout(DEADLINE,run_probe(tcp,udp)) => result.map_err(|_| FragmentProbeError::Deadline)?,
        _ = tokio::signal::ctrl_c() => Err(FragmentProbeError::Cancelled),
    }?;
    if Instant::now() >= deadline {
        return Err(FragmentProbeError::Deadline.into());
    }
    super::event(result);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_loopback_only_and_fixed_budget() {
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
        assert!(endpoint("[::1]:9", false).is_ok());
        assert_eq!(
            2 * SIZES.into_iter().sum::<usize>(),
            UDP_VALID_EACH_DIRECTION
        );
        assert_eq!(
            2 * UDP_VALID_EACH_DIRECTION + 4 * OVERSIZE,
            UDP_SCHEDULED_DUPLEX
        );
        let scheduled = 2 * UDP_VALID_EACH_DIRECTION + 4 * OVERSIZE;
        assert!(scheduled <= UDP_BUDGET);
    }
    #[test]
    fn learned_two_origins_keep_empty_udp_and_exact_sequence_isolated() {
        let mut fixture = Fixture::default();
        fixture.begin().unwrap();
        let sources: [SocketAddr; 2] = [
            "127.0.0.1:50001".parse().unwrap(),
            "127.0.0.1:50002".parse().unwrap(),
        ];
        for (source, address) in sources.into_iter().enumerate() {
            for (slot, size) in SIZES.into_iter().enumerate() {
                assert_eq!(
                    fixture.receive(&packet(source, slot, size), address),
                    Some((source, slot))
                );
            }
        }
        assert_eq!(fixture.stats.valid, [2; 4]);
        assert_eq!(fixture.stats.origins, 2);
        assert_eq!(fixture.stats.valid_bytes, UDP_VALID_EACH_DIRECTION as u64);
        assert!(fixture.receive(&[], sources[0]).is_none());
        assert!(
            fixture
                .receive(&packet(0, 4, OVERSIZE), sources[0])
                .is_none()
        );
        assert_eq!(fixture.stats.oversize_requests, 1);
    }
    #[test]
    fn fixed_snapshot_framing_and_nonempty_patterns_are_distinct() {
        let expected = Stats {
            started: 1,
            valid: [2; 4],
            valid_bytes: 15262,
            oversize_emitted: 2,
            tcp_bytes: 1048576,
            ..Stats::default()
        };
        assert_eq!(Stats::decode(expected.encode()), expected);
        for (slot, size) in SIZES.into_iter().enumerate() {
            if size > 0 {
                assert_ne!(packet(0, slot, size), packet(1, slot, size));
                assert_ne!(packet(0, slot, size), response(0, slot, size));
            }
        }
    }
}
