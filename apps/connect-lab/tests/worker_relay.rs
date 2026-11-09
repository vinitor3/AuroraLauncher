#![cfg(feature = "local-relay-lab")]

use aurora_connect_lab::{EndpointId, SecretKey, generate_key, unix_millis};
use bytes::Bytes;
use iroh::{
    Endpoint, EndpointAddr, NetReportConfig, RelayConfig, RelayMap, RelayMode, RelayUrl,
    endpoint::{Connection, QuicTransportConfig, presets},
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    time::{Duration, Instant},
};
use tokio::time::{sleep, timeout};

const ALPN: &[u8] = b"aurora-relay-wire-fixture";
const SESSION_A: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const SESSION_B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

struct Worker {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl Worker {
    async fn start(approved: &[Value]) -> (Self, RelayUrl) {
        let worker = Path::new(env!("CARGO_MANIFEST_DIR")).join("../edge-proxy");
        let mut command = Command::new("node");
        command
            .current_dir(worker)
            .arg("tests/connectRelayMiniflare.mjs")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().expect("start local Node and Miniflare");
        let stdout = child.stdout.take().unwrap();
        let stdin = child.stdin.take();
        let mut worker = Self { child, stdin };
        let writer = worker.stdin.as_mut().unwrap();
        writeln!(writer, "{}", serde_json::to_string(approved).unwrap()).unwrap();
        writer.flush().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
            let _ = tx.send(result);
        });
        let url: RelayUrl = timeout(Duration::from_secs(15), rx)
            .await
            .expect("Miniflare start deadline")
            .unwrap()
            .unwrap()
            .trim()
            .parse()
            .expect("owned relay URL");
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert_ne!(url.port().unwrap_or_default(), 0);
        (worker, url)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn approved(
    session: &str,
    host: EndpointId,
    peer: EndpointId,
    allowed: &[EndpointId],
    now: u64,
    deadline: u64,
) -> Value {
    json!({
        "ticket": {
            "body": {
                "schemaVersion": 1, "sessionId": session, "issuerInstance": "f".repeat(64),
                "hostPeerId": host.to_string(), "peerId": peer.to_string(), "accountId": "fixture",
                "generation": 1, "issuedAtUnixMs": now, "expiresAtUnixMs": deadline,
                "directAddresses": []
            },
            "signature": "0".repeat(128)
        },
        "allowedPeers": allowed.iter().map(ToString::to_string).collect::<Vec<_>>(),
    })
}

fn bearer(approval: &Value) -> String {
    serde_json::to_string(&approval["ticket"]).unwrap()
}

async fn endpoint(key: SecretKey, url: &RelayUrl, approval: &Value) -> Endpoint {
    let mut report = NetReportConfig::minimal();
    report.https_probes = true;
    let endpoint = Endpoint::builder(presets::Minimal)
        .clear_ip_transports()
        .clear_relay_transports()
        .clear_address_lookup()
        .relay_mode(RelayMode::Custom(RelayMap::from_iter([RelayConfig::new(
            url.clone(),
            None,
        )
        .with_auth_token(bearer(approval))])))
        .net_report_config(report)
        .secret_key(key)
        .transport_config(
            QuicTransportConfig::builder()
                .max_idle_timeout(Some(Duration::from_secs(90).try_into().unwrap()))
                .datagram_receive_buffer_size(Some(16 * 1024))
                .datagram_send_buffer_size(16 * 1024)
                .build(),
        )
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .expect("bind the relay-only Iroh endpoint");
    assert!(endpoint.bound_sockets().is_empty());
    timeout(Duration::from_secs(8), endpoint.online())
        .await
        .expect("approved relay endpoint online");
    endpoint
}

async fn connect(guest: &Endpoint, host: &Endpoint, relay: &RelayUrl) -> (Connection, Connection) {
    let address = EndpointAddr::new(host.id()).with_relay_url(relay.clone());
    let (outgoing, incoming) = timeout(Duration::from_secs(5), async {
        tokio::join!(guest.connect(address, ALPN), async {
            host.accept().await.unwrap().await
        })
    })
    .await
    .expect("real QUIC handshake over relay");
    let outgoing = outgoing.unwrap();
    let incoming = incoming.unwrap();
    assert_eq!(incoming.remote_id(), guest.id());
    assert!(outgoing.paths().iter().any(|path| path.is_relay()));
    assert!(outgoing.paths().iter().all(|path| !path.is_ip()));
    (outgoing, incoming)
}

async fn relay_payload(outgoing: &Connection, incoming: &Connection) {
    timeout(Duration::from_secs(5), async {
        let bytes = (0..65_536).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let (mut send, mut recv) = outgoing.open_bi().await.unwrap();
        let ((), returned) = tokio::join!(
            async {
                send.write_all(&bytes).await.unwrap();
                send.finish().unwrap();
            },
            async {
                let (mut send, mut recv) = incoming.accept_bi().await.unwrap();
                let received = recv.read_to_end(bytes.len()).await.unwrap();
                assert_eq!(received, bytes);
                send.write_all(&received).await.unwrap();
                send.finish().unwrap();
                received
            }
        );
        assert_eq!(recv.read_to_end(bytes.len()).await.unwrap(), returned);
        let packet = Bytes::from_static(b"opaque datagram through the real Iroh relay");
        outgoing.send_datagram(packet.clone()).unwrap();
        assert_eq!(incoming.read_datagram().await.unwrap(), packet);
        incoming.send_datagram(packet.clone()).unwrap();
        assert_eq!(outgoing.read_datagram().await.unwrap(), packet);
    })
    .await
    .expect("bounded stream and datagram round trips");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit Miniflare and real Iroh 1.3 loopback interoperability"]
async fn miniflare_iroh_relay_stream_datagram_denial_session_isolation_and_expiry() {
    let host_a = generate_key();
    let guest_a = generate_key();
    let denied_guest = generate_key();
    let expiring_host_key = host_a.clone();
    let host_b = generate_key();
    let guest_b = generate_key();
    let now = unix_millis();
    let deadline_a = now + 35_000;
    let deadline_b = now + 55_000;
    let approvals = [
        approved(
            SESSION_A,
            host_a.public(),
            host_a.public(),
            &[guest_a.public(), denied_guest.public()],
            now,
            deadline_a,
        ),
        approved(
            SESSION_A,
            host_a.public(),
            guest_a.public(),
            &[host_a.public()],
            now,
            deadline_a,
        ),
        approved(
            SESSION_B,
            host_b.public(),
            host_b.public(),
            &[guest_b.public()],
            now,
            deadline_b,
        ),
        approved(
            SESSION_B,
            host_b.public(),
            guest_b.public(),
            &[host_b.public()],
            now,
            deadline_b,
        ),
        approved(
            SESSION_A,
            host_a.public(),
            denied_guest.public(),
            &[host_a.public()],
            now,
            deadline_a,
        ),
    ];
    let (_worker, url) = Worker::start(&approvals).await;
    let (host_a, guest_a, host_b, guest_b, denied_guest) = tokio::join!(
        endpoint(host_a, &url, &approvals[0]),
        endpoint(guest_a, &url, &approvals[1]),
        endpoint(host_b, &url, &approvals[2]),
        endpoint(guest_b, &url, &approvals[3]),
        endpoint(denied_guest, &url, &approvals[4]),
    );

    let client = |key, ticket: String| {
        iroh_relay::client::ClientBuilder::new(url.clone(), key, iroh::dns::DnsResolver::default())
            .tls_client_config(
                iroh_relay::tls::CaTlsConfig::default()
                    .client_config(iroh_relay::tls::default_provider())
                    .unwrap(),
            )
            .auth_token(ticket)
    };
    let stolen = client(generate_key(), bearer(&approvals[1]));
    assert!(
        timeout(Duration::from_secs(4), stolen.connect())
            .await
            .unwrap()
            .is_err()
    );

    let (a_out, a_in) = connect(&guest_a, &host_a, &url).await;
    let (b_out, b_in) = connect(&guest_b, &host_b, &url).await;
    relay_payload(&a_out, &a_in).await;
    relay_payload(&b_out, &b_in).await;

    let foreign = EndpointAddr::new(host_b.id()).with_relay_url(url.clone());
    let (cross_session, destination) = tokio::join!(
        timeout(Duration::from_secs(2), denied_guest.connect(foreign, ALPN)),
        timeout(Duration::from_secs(2), host_b.accept()),
    );
    assert!(!matches!(cross_session, Ok(Ok(_))));
    assert!(destination.is_err());
    relay_payload(&a_out, &a_in).await;
    relay_payload(&b_out, &b_in).await;

    sleep(Duration::from_millis(
        deadline_a.saturating_sub(unix_millis()) + 100,
    ))
    .await;
    let expired = client(expiring_host_key, bearer(&approvals[0]));
    assert!(
        timeout(Duration::from_secs(4), expired.connect())
            .await
            .unwrap()
            .is_err()
    );
    let _ = a_out.send_datagram(Bytes::from_static(b"expired"));
    assert!(!matches!(
        timeout(Duration::from_millis(800), a_in.read_datagram()).await,
        Ok(Ok(_))
    ));
    relay_payload(&b_out, &b_in).await;

    tokio::join!(
        host_a.close(),
        guest_a.close(),
        host_b.close(),
        guest_b.close(),
        denied_guest.close()
    );
}
