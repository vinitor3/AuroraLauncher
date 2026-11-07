//! Additional closed, nonce-bound transport protocol of the SAME85 own child.
//! The legacy STATUS codec remains untouched. No Java/mod protocol is emulated.
use super::producer_fixture::{self as status, Notice};
use super::*;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream, UdpSocket},
    sync::{atomic::AtomicBool, mpsc},
};

pub(super) const TCP_MAGIC: &[u8; 4] = b"A86T";
pub(super) const UDP_MAGIC: &[u8; 4] = b"A86U";
const TCP_BUDGET: usize = 1024 * 1024;
const UDP_BUDGET: usize = 64 * 1024;
type FixtureResult<T> = std::result::Result<T, &'static str>;

fn exact(
    stream: &mut TcpStream,
    data: &mut [u8],
    stop: &AtomicBool,
    deadline: Instant,
) -> FixtureResult<()> {
    let mut read = 0;
    while read < data.len() {
        if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err("voice_transport_read_deadline");
        }
        match stream.read(&mut data[read..]) {
            Ok(0) => return Err("voice_transport_read_eof"),
            Ok(count) => read += count,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Err("voice_transport_read"),
        }
    }
    Ok(())
}

pub(super) fn session(
    mut stream: TcpStream,
    nonce: Uuid,
    stop: Arc<AtomicBool>,
    notices: mpsc::SyncSender<Notice>,
    first: Arc<AtomicBool>,
) -> FixtureResult<()> {
    stream
        .set_nonblocking(false)
        .map_err(|_| "voice_transport_mode")?;
    stream
        .set_read_timeout(Some(Duration::from_millis(20)))
        .map_err(|_| "voice_transport_timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_millis(100)))
        .map_err(|_| "voice_transport_timeout")?;
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut prefix = [0; 4];
    loop {
        if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err("voice_transport_prefix_deadline");
        }
        match stream.peek(&mut prefix) {
            Ok(0) => return Ok(()),
            Ok(4) => break,
            Ok(_) => std::thread::sleep(Duration::from_millis(2)),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return Err("voice_transport_prefix"),
        }
    }
    if &prefix != TCP_MAGIC {
        return status::status_session(stream, "normal".into(), stop, notices, first);
    }
    let mut header = [0; 20];
    exact(&mut stream, &mut header, &stop, deadline)?;
    if &header[..4] != TCP_MAGIC || &header[4..] != nonce.as_bytes() {
        return Err("voice_transport_identity");
    }
    let mut transferred = 0usize;
    loop {
        let mut encoded = [0; 2];
        exact(&mut stream, &mut encoded, &stop, deadline)?;
        let length = u16::from_be_bytes(encoded) as usize;
        if length > 4096 || transferred.saturating_add(length) > TCP_BUDGET {
            return Err("voice_transport_byte_limit");
        }
        transferred += length;
        let mut bytes = vec![0; length];
        exact(&mut stream, &mut bytes, &stop, deadline)?;
        stream
            .write_all(&encoded)
            .and_then(|()| stream.write_all(&bytes))
            .map_err(|_| "voice_transport_echo")?;
        if length == 0 {
            // Guest actively closes its half; avoid server-side TIME_WAIT.
            let mut byte = [0];
            while !stop.load(Ordering::Acquire) && Instant::now() < deadline {
                match stream.read(&mut byte) {
                    Ok(0) => return Ok(()),
                    Ok(_) => return Err("voice_transport_tail"),
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(_) => return Err("voice_transport_tail_read"),
                }
            }
            return Ok(());
        }
    }
}

#[derive(Default)]
pub(super) struct UdpTraffic {
    attempted: usize,
    holding: bool,
    pending: Vec<(Vec<u8>, SocketAddr)>,
}
impl UdpTraffic {
    pub(super) fn hold(&mut self) -> FixtureResult<()> {
        if self.holding || !self.pending.is_empty() {
            return Err("voice_transport_hold_state");
        }
        self.holding = true;
        Ok(())
    }
    pub(super) fn release(&mut self, socket: &UdpSocket) -> FixtureResult<()> {
        self.holding = false;
        for (bytes, peer) in self.pending.drain(..) {
            if socket
                .send_to(&bytes, peer)
                .map_err(|_| "voice_transport_udp_release")?
                != bytes.len()
            {
                return Err("voice_transport_udp_partial");
            }
        }
        Ok(())
    }
}
/// Returns the number of actual packets retained after validation, or None
/// when no new held packet was observed. It grants no transport authority.
pub(super) fn udp(
    socket: &UdpSocket,
    nonce: Uuid,
    traffic: &mut UdpTraffic,
) -> FixtureResult<Option<usize>> {
    let mut bytes = [0; 4097];
    let (length, peer) = match socket.recv_from(&mut bytes) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
        Err(_) => return Err("voice_transport_udp_read"),
    };
    if !peer.ip().is_loopback() {
        return Err("voice_transport_udp_peer");
    }
    if length == 8 && &bytes[..length] == b"aur85c01" {
        socket
            .send_to(&bytes[..length], peer)
            .map_err(|_| "voice_transport_legacy_udp")?;
        return Ok(None);
    }
    if !(24..=4096).contains(&length)
        || &bytes[..4] != UDP_MAGIC
        || &bytes[4..20] != nonce.as_bytes()
        || bytes[20] > 1
        || bytes[21] > 1
    {
        return Err("voice_transport_udp_protocol");
    }
    traffic.attempted = traffic
        .attempted
        .checked_add(length * 2)
        .ok_or("voice_transport_udp_budget")?;
    if traffic.attempted > UDP_BUDGET {
        return Err("voice_transport_udp_budget");
    }
    if traffic.holding {
        if traffic.pending.len() >= 4 {
            return Err("voice_transport_udp_pending_limit");
        }
        traffic.pending.push((bytes[..length].to_vec(), peer));
        return Ok(Some(traffic.pending.len()));
    }
    if socket
        .send_to(&bytes[..length], peer)
        .map_err(|_| "voice_transport_udp_echo")?
        != length
    {
        return Err("voice_transport_udp_partial");
    }
    Ok(None)
}
