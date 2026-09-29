use crate::{ALPN, DATAGRAM_HEADER_LEN, Error, Limits, Result, model::Metrics};
use iroh::{
    Endpoint, RelayMode, SecretKey,
    endpoint::{Connection, QuicTransportConfig, RecvStream, SendStream, presets},
};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex, atomic::Ordering},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};
use tokio_util::sync::CancellationToken;

pub(crate) async fn endpoint(
    key: SecretKey,
    bind: SocketAddr,
    limits: &Limits,
) -> Result<Endpoint> {
    limits.validate()?;
    let config = QuicTransportConfig::builder()
        .max_concurrent_bidi_streams(32u32.into())
        .max_concurrent_uni_streams(0u32.into())
        .stream_receive_window((64u32 * 1024).into())
        .receive_window((1024u32 * 1024).into())
        .send_window(1024 * 1024)
        .send_fairness(true)
        .max_idle_timeout(Some(
            limits
                .idle_timeout
                .try_into()
                .map_err(|_| Error::Configuration)?,
        ))
        .datagram_receive_buffer_size(limits.datagram_receive_buffer)
        .datagram_send_buffer_size(limits.datagram_send_buffer)
        .send_observed_address_reports(false)
        .max_remote_nat_traversal_addresses(0)
        .build();
    Endpoint::builder(presets::Minimal)
        .clear_ip_transports()
        .clear_relay_transports()
        .relay_mode(RelayMode::Disabled)
        .clear_address_lookup()
        .secret_key(key)
        .alpns(vec![ALPN.to_vec()])
        .keylog(false)
        .transport_config(config)
        .bind_addr(bind)
        .map_err(|_| Error::Configuration)?
        .bind()
        .await
        .map_err(|_| Error::Transport)
}

pub(crate) fn payload_limit(connection: &Connection, service_max: usize) -> Option<usize> {
    payload_budget(connection.max_datagram_size(), service_max)
}

fn payload_budget(transport_max: Option<usize>, service_max: usize) -> Option<usize> {
    transport_max?
        .checked_sub(DATAGRAM_HEADER_LEN)
        .map(|n| n.min(service_max).min(65507))
}

pub(crate) struct Bucket {
    tokens: f64,
    rate: f64,
    burst: f64,
    at: Instant,
}
impl Bucket {
    pub fn new(rate: u32, burst: u32) -> Self {
        Self {
            tokens: burst as f64,
            rate: rate as f64,
            burst: burst as f64,
            at: Instant::now(),
        }
    }
    pub fn take(&mut self, count: usize) -> bool {
        self.take_with_reserve(count, 0)
    }
    fn take_with_reserve(&mut self, count: usize, reserve: usize) -> bool {
        let now = Instant::now();
        self.tokens =
            (self.tokens + now.duration_since(self.at).as_secs_f64() * self.rate).min(self.burst);
        self.at = now;
        if self.tokens >= count as f64 + reserve as f64 {
            self.tokens -= count as f64;
            true
        } else {
            false
        }
    }
}
pub(crate) type Budget = Arc<Mutex<Bucket>>;

pub(crate) async fn bridge_tcp(
    tcp: TcpStream,
    send: SendStream,
    recv: RecvStream,
    idle: Duration,
    cancel: CancellationToken,
    budget: Budget,
    metrics: Arc<Metrics>,
) -> Result<()> {
    let (read, write) = tcp.into_split();
    tokio::select! {
        _ = cancel.cancelled() => Err(Error::Closed),
        result = async {
            tokio::try_join!(pump(read, send, idle, budget.clone(), metrics.clone()), pump(recv, write, idle, budget, metrics))?;
            Ok(())
        } => result,
    }
}

async fn pump<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut read: R,
    mut write: W,
    idle: Duration,
    budget: Budget,
    metrics: Arc<Metrics>,
) -> Result<()> {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let size = tokio::time::timeout(idle, read.read(&mut buf))
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::Io)?;
        if size == 0 {
            write.shutdown().await.map_err(|_| Error::Io)?;
            return Ok(());
        }
        tokio::time::timeout(idle, async {
            loop {
                // TCP waits instead of consuming the last datagram burst credit.
                // Both protocols still charge the same aggregate bucket, so this
                // reservation cannot exceed the configured session bandwidth.
                if budget.lock().unwrap().take_with_reserve(size, 16 * 1024) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            write.write_all(&buf[..size]).await.map_err(|_| Error::Io)
        })
        .await
        .map_err(|_| Error::Timeout)??;
        metrics.tcp_bytes.fetch_add(size as u64, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn datagram_budget_tracks_current_transport_without_underflow_or_double_overhead() {
        assert_eq!(payload_budget(None, 1400), None);
        assert_eq!(payload_budget(Some(23), 1400), None);
        assert_eq!(payload_budget(Some(24), 1400), Some(0));
        assert_eq!(payload_budget(Some(1300), 1400), Some(1276));
        assert_eq!(payload_budget(Some(1500), 1400), Some(1400));
        assert_eq!(payload_budget(Some(1100), 1400), Some(1076));
        assert_eq!(payload_budget(Some(usize::MAX), usize::MAX), Some(65507));
    }

    #[test]
    fn tcp_backpressure_preserves_datagram_burst_without_bypassing_aggregate_budget() {
        let mut bucket = Bucket::new(1, 65536);
        for _ in 0..3 {
            assert!(bucket.take_with_reserve(16384, 16384));
        }
        assert!(!bucket.take_with_reserve(16384, 16384));
        assert!(bucket.take(1024));
        assert!(bucket.take(15360));
        assert!(!bucket.take(1));
    }
}
