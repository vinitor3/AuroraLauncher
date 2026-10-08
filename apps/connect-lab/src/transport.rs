use crate::{ALPN, DATAGRAM_HEADER_LEN, Error, Limits, Result, model::Metrics};
use iroh::{
    Endpoint, NetReportConfig, RelayMode, SecretKey,
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

pub(crate) enum EndpointMode {
    Direct,
    #[cfg(feature = "local-relay-lab")]
    LocalRelay(crate::LocalRelayTicket),
}

pub(crate) async fn endpoint_in_mode(
    key: SecretKey,
    bind: SocketAddr,
    limits: &Limits,
    mode: EndpointMode,
) -> Result<Endpoint> {
    match mode {
        EndpointMode::Direct => endpoint(key, bind, limits).await,
        #[cfg(feature = "local-relay-lab")]
        EndpointMode::LocalRelay(ticket) => {
            let creation = ticket.begin_endpoint()?;
            let config = quic_config(limits)?;
            let online_deadline = ticket.online_deadline(limits.handshake_timeout)?;
            let mut report = NetReportConfig::minimal();
            // With QAD disabled, this is necessary to select the home relay.
            // The sole URL is our HTTP loopback fixture; no captive-portal check,
            // public DNS endpoint, TLS/ACME service or other relay is configured.
            report.https_probes = true;
            // No bind_addr: that would re-add an IP transport. The only route
            // is the owned loopback fixture, without QAD, DNS or publication.
            let endpoint = Endpoint::builder(presets::Minimal)
                .clear_ip_transports()
                .clear_relay_transports()
                .clear_address_lookup()
                .relay_mode(RelayMode::Custom(ticket.map()))
                .net_report_config(report)
                .secret_key(key)
                .alpns(vec![ALPN.to_vec()])
                .keylog(false)
                .transport_config(config)
                .bind()
                .await
                .map_err(|_| Error::Transport)?;
            if let Err(error) = creation.register(&endpoint) {
                endpoint.close().await;
                return Err(error);
            }
            // Explicit readiness for this fixture; it is not an Aurora peer
            // admission or an application-service proof.
            let ready = tokio::select! {
                biased;
                _ = ticket.cancelled() => Err(Error::Expired),
                result = tokio::time::timeout_at(online_deadline.into(), endpoint.online()) => result.map_err(|_| Error::Timeout),
            };
            if let Err(error) = ready {
                endpoint.close().await;
                return Err(error);
            }
            if let Err(error) = ticket.check_current() {
                endpoint.close().await;
                return Err(error);
            }
            Ok(endpoint)
        }
    }
}

fn quic_config(limits: &Limits) -> Result<QuicTransportConfig> {
    limits.validate()?;
    Ok(QuicTransportConfig::builder()
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
        .receive_observed_address_reports(false)
        // Iroh 1.3 ignores a QNT address limit below 8, including zero. Its
        // defaults remain 32 remote candidates and 8 multipath paths. These
        // peer-to-peer mechanisms are separate from external lookup/relay.
        .build())
}

pub(crate) async fn endpoint(
    key: SecretKey,
    bind: SocketAddr,
    limits: &Limits,
) -> Result<Endpoint> {
    let config = quic_config(limits)?;
    Endpoint::builder(presets::Minimal)
        .clear_ip_transports()
        .clear_relay_transports()
        .relay_mode(RelayMode::Disabled)
        .clear_address_lookup()
        .net_report_config(NetReportConfig::minimal())
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
    fn effective_quic_config_keeps_qnt_defaults_and_disables_observed_reports() {
        // This pinned-version dependency contract has no public config getters.
        // Inspect Debug only in the test; never emit it in production/logs.
        let config = quic_config(&Limits::default()).unwrap();
        let inspected = format!("{config:?}");
        assert!(
            inspected.contains("address_discovery_role: Role { send: false, receive: false }"),
            "observed_report_configuration_changed"
        );
        assert!(
            inspected.contains("max_remote_nat_traversal_addresses: Some(32)"),
            "qnt_candidate_default_changed"
        );
        assert!(
            inspected.contains("max_concurrent_multipath_paths: Some(8)"),
            "multipath_default_changed"
        );
        let net_report = NetReportConfig::minimal();
        assert!(!net_report.https_probes);
        assert!(!net_report.captive_portal_check);
    }

    #[tokio::test]
    async fn production_endpoints_exchange_bytes_without_observed_address_reports() {
        use bytes::Bytes;
        use iroh::EndpointAddr;

        fn checked<T, E>(result: std::result::Result<T, E>, code: &'static str) -> T {
            match result {
                Ok(value) => value,
                Err(_) => panic!("{code}"),
            }
        }

        let limits = Limits::default();
        let bind = "127.0.0.1:0".parse().unwrap();
        let server = checked(
            endpoint(SecretKey::generate(), bind, &limits).await,
            "config_server_bind_failed",
        );
        let client = match endpoint(SecretKey::generate(), bind, &limits).await {
            Ok(client) => client,
            Err(_) => {
                server.close().await;
                panic!("config_client_bind_failed");
            }
        };
        let loopback_only = [&server, &client].into_iter().all(|endpoint| {
            let sockets = endpoint.bound_sockets();
            sockets.len() == 1 && sockets[0].ip() == bind.ip() && sockets[0].port() != 0
        });
        if !loopback_only {
            tokio::join!(server.close(), client.close());
            panic!("config_unexpected_bind");
        }
        let server_addr = EndpointAddr::new(server.id()).with_ip_addr(server.bound_sockets()[0]);
        let exchange = tokio::time::timeout(Duration::from_secs(5), async {
            let (incoming, outgoing) = tokio::join!(
                async {
                    server
                        .accept()
                        .await
                        .ok_or("config_accept_closed")?
                        .await
                        .map_err(|_| "config_accept_failed")
                },
                async {
                    client
                        .connect(server_addr, ALPN)
                        .await
                        .map_err(|_| "config_connect_failed")
                }
            );
            let incoming = incoming?;
            let outgoing = outgoing?;
            if incoming.remote_id() != client.id() || outgoing.remote_id() != server.id() {
                return Err("config_transport_identity_mismatch");
            }

            let request = vec![0x37; 4096];
            let response = vec![0xa6; 4096];
            let (mut client_send, mut client_recv) = outgoing
                .open_bi()
                .await
                .map_err(|_| "config_stream_open_failed")?;
            client_send
                .write_all(&request)
                .await
                .map_err(|_| "config_request_write_failed")?;
            client_send
                .finish()
                .map_err(|_| "config_request_finish_failed")?;
            let (mut server_send, mut server_recv) = incoming
                .accept_bi()
                .await
                .map_err(|_| "config_stream_accept_failed")?;
            let received = server_recv
                .read_to_end(request.len())
                .await
                .map_err(|_| "config_request_read_failed")?;
            if received != request {
                return Err("config_request_payload_mismatch");
            }
            server_send
                .write_all(&response)
                .await
                .map_err(|_| "config_response_write_failed")?;
            server_send
                .finish()
                .map_err(|_| "config_response_finish_failed")?;
            let received = client_recv
                .read_to_end(response.len())
                .await
                .map_err(|_| "config_response_read_failed")?;
            if received != response {
                return Err("config_response_payload_mismatch");
            }
            for (sender, receiver, payload) in [
                (&outgoing, &incoming, b"config-request".as_slice()),
                (&incoming, &outgoing, b"config-response".as_slice()),
            ] {
                sender
                    .send_datagram(Bytes::copy_from_slice(payload))
                    .map_err(|_| "config_datagram_send_failed")?;
                let received = receiver
                    .read_datagram()
                    .await
                    .map_err(|_| "config_datagram_receive_failed")?;
                if received.as_ref() != payload {
                    return Err("config_datagram_payload_mismatch");
                }
            }
            // A bounded observation window after real bidirectional traffic.
            // This is transport-level evidence, not Aurora Hello/service admission.
            tokio::time::sleep(Duration::from_millis(100)).await;
            let stats = [incoming.stats(), outgoing.stats()];
            incoming.close(0u32.into(), b"config-test-complete");
            outgoing.close(0u32.into(), b"config-test-complete");
            Ok(stats)
        })
        .await;
        // Also close both owned endpoints if the exchange errored/timed out.
        tokio::join!(server.close(), client.close());
        let stats = checked(
            checked(exchange, "config_exchange_timeout"),
            "config_exchange_failed",
        );
        for (side, stats) in ["server", "client"].into_iter().zip(stats) {
            assert_eq!(stats.frame_tx.observed_addr, 0);
            assert_eq!(stats.frame_rx.observed_addr, 0);
            assert!(stats.frame_tx.stream > 0 && stats.frame_rx.stream > 0);
            assert!(stats.frame_tx.datagram > 0 && stats.frame_rx.datagram > 0);
            // QNT counters are observations, never an assertion that QNT is off.
            // Only fixed labels and aggregate counts can reach test output.
            println!(
                "{}",
                serde_json::json!({
                    "event": "transport_config_observed", "side": side,
                    "streamBytesEachDirection": 4096,
                    "datagramsEachDirection": 1,
                    "observedAddressTx": stats.frame_tx.observed_addr,
                    "observedAddressRx": stats.frame_rx.observed_addr,
                    "qntAddAddressTx": stats.frame_tx.add_address,
                    "qntAddAddressRx": stats.frame_rx.add_address,
                    "qntReachOutTx": stats.frame_tx.reach_out,
                    "qntReachOutRx": stats.frame_rx.reach_out,
                    "pathChallengeTx": stats.frame_tx.path_challenge,
                    "pathChallengeRx": stats.frame_rx.path_challenge,
                    "pathResponseTx": stats.frame_tx.path_response,
                    "pathResponseRx": stats.frame_rx.path_response
                })
            );
        }
    }

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
        let mut bucket = Bucket::new(0, 65536);
        for _ in 0..3 {
            assert!(bucket.take_with_reserve(16384, 16384));
        }
        assert!(!bucket.take_with_reserve(16384, 16384));
        assert!(bucket.take(1024));
        assert!(bucket.take(15360));
        assert!(!bucket.take(1));
    }
}
