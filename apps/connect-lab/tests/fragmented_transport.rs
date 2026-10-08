//! Real authenticated agents, loopback fixtures and opaque bytes only.
use aurora_connect_lab::*;
use bytes::Bytes;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, UdpSocket},
    task::{JoinHandle, JoinSet},
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;

fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn caps() -> Vec<Capability> {
    vec![
        Capability::ReliableStream,
        Capability::Datagram,
        Capability::FragmentedDatagram,
    ]
}
fn udp_service(
    name: &str,
    endpoint: SocketAddr,
    fragmented: bool,
    policy: UdpReturnPolicy,
) -> Service {
    Service {
        service_id: name.into(),
        protocol: Protocol::Udp,
        endpoint,
        revision: 1,
        max_datagram_bytes: if fragmented { 4096 } else { 1264 },
        required_capability: if fragmented {
            Capability::FragmentedDatagram
        } else {
            Capability::Datagram
        },
        udp_return_policy: policy,
    }
}

struct Lab {
    host: Option<Host>,
    guests: Vec<Guest>,
    keys: Vec<SecretKey>,
    services: Vec<Service>,
    counts: Vec<Arc<AtomicUsize>>,
    destinations: Vec<Arc<UdpSocket>>,
    sources: Vec<Arc<Mutex<Option<SocketAddr>>>>,
    tasks: Vec<JoinHandle<()>>,
    stop: CancellationToken,
    udp_addrs: Vec<SocketAddr>,
    tcp_addr: SocketAddr,
}
impl Drop for Lab {
    fn drop(&mut self) {
        self.stop.cancel();
        for task in &self.tasks {
            task.abort();
        }
    }
}
impl Lab {
    async fn new(limits: Limits, policy: UdpReturnPolicy) -> Self {
        let stop = CancellationToken::new();
        let mut tasks = Vec::new();
        let mut udp_addrs = Vec::new();
        let mut counts = Vec::new();
        let mut destinations = Vec::new();
        let mut sources = Vec::new();
        for tag in [0u8, 0x40] {
            let socket = Arc::new(UdpSocket::bind(local()).await.unwrap());
            destinations.push(socket.clone());
            let source_slot = Arc::new(Mutex::new(None));
            sources.push(source_slot.clone());
            udp_addrs.push(socket.local_addr().unwrap());
            let count = Arc::new(AtomicUsize::new(0));
            counts.push(count.clone());
            let token = stop.clone();
            tasks.push(tokio::spawn(async move {
                let mut buffer = vec![0; 65536];
                loop {
                    tokio::select! {
                        _ = token.cancelled() => break,
                        value = socket.recv_from(&mut buffer) => {
                        let Ok((size, source)) = value else { break; };
                        *source_slot.lock().unwrap() = Some(source);
                            count.fetch_add(1, Ordering::SeqCst);
                            for byte in &mut buffer[..size] { *byte ^= tag; }
                            let _ = socket.send_to(&buffer[..size], source).await;
                        }
                    }
                }
            }));
        }
        let listener = TcpListener::bind(local()).await.unwrap();
        let tcp_addr = listener.local_addr().unwrap();
        let token = stop.clone();
        tasks.push(tokio::spawn(async move {
            let mut children = JoinSet::new();
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    accepted = listener.accept() => {
                        let Ok((mut socket, _)) = accepted else { break; };
                        children.spawn(async move { let (mut read, mut write) = socket.split();
                            let _ = tokio::io::copy(&mut read, &mut write).await;
                            let _ = write.shutdown().await; });
                    }
                    _ = children.join_next(), if !children.is_empty() => (),
                }
            }
            children.shutdown().await;
        }));
        let mut services = vec![
            udp_service("voice-a", udp_addrs[0], true, policy),
            udp_service("voice-b", udp_addrs[1], true, policy),
            udp_service("plain", udp_addrs[0], false, UdpReturnPolicy::ReplyBudget),
        ];
        services.push(Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: tcp_addr,
            revision: 1,
            max_datagram_bytes: 0,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        });
        let keys = vec![generate_key(), generate_key()];
        let leases = keys
            .iter()
            .enumerate()
            .map(|(i, key)| Lease {
                lease_id: format!("peer-{i}"),
                peer: key.public(),
                expires_at_unix_ms: unix_millis() + 30_000,
                services: services.iter().map(|s| s.service_id.clone()).collect(),
            })
            .collect();
        let host = Host::start(HostConfig {
            secret_key: generate_key(),
            bind_addr: local(),
            session_id: "fragmented-lab".into(),
            services: services.clone(),
            leases,
            limits,
        })
        .await
        .unwrap();
        udp_addrs.extend(host.bound_sockets());
        Self {
            host: Some(host),
            guests: Vec::new(),
            keys,
            services,
            counts,
            destinations,
            sources,
            tasks,
            stop,
            udp_addrs,
            tcp_addr,
        }
    }
    async fn guest(&mut self, index: usize, capabilities: Vec<Capability>) -> Guest {
        let guest = Guest::connect(GuestConfig {
            secret_key: self.keys[index].clone(),
            bind_addr: local(),
            host: self.host.as_ref().unwrap().addr(),
            session_id: "fragmented-lab".into(),
            lease_id: format!("peer-{index}"),
            capabilities,
            limits: Limits::default(),
        })
        .await
        .unwrap();
        self.udp_addrs.extend(guest.bound_sockets());
        self.guests.push(guest.clone());
        guest
    }
    async fn cleanup(&mut self) {
        for guest in &self.guests {
            guest.shutdown().await;
        }
        self.guests.clear();
        if let Some(host) = self.host.take() {
            host.shutdown().await;
            drop(host);
        }
        self.stop.cancel();
        for task in self.tasks.drain(..) {
            task.await.unwrap();
        }
        self.destinations.clear();
        assert!(
            TcpListener::bind(self.tcp_addr).await.is_ok(),
            "tcp_cleanup_rebind_failed"
        );
        for address in &self.udp_addrs {
            assert!(
                UdpSocket::bind(address).await.is_ok(),
                "udp_cleanup_rebind_failed"
            );
        }
    }
}

async fn echo(association: &FragmentedUdpAssociation, bytes: &[u8], tag: u8) {
    association.send(bytes).await.unwrap();
    let output = timeout(Duration::from_secs(2), association.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        output.as_ref(),
        bytes.iter().map(|b| b ^ tag).collect::<Vec<_>>()
    );
}
async fn tcp_roundtrip(guest: &Guest) {
    let (mut send, mut recv) = guest.open_tcp("game", 1).await.unwrap();
    let payload = vec![0x39; 1024 * 1024];
    let ((), received) = tokio::join!(
        async {
            send.write_all(&payload).await.unwrap();
            send.finish().unwrap();
        },
        async { recv.read_to_end(payload.len() + 1).await.unwrap() }
    );
    assert_eq!(received, payload);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn opaque_large_datagrams_two_peers_services_and_tcp_with_normal_cleanup() {
    let mut lab = Lab::new(Limits::default(), UdpReturnPolicy::ReplyBudget).await;
    let first = lab.guest(0, caps()).await;
    let second = lab.guest(1, caps()).await;
    let a = first.open_udp_fragmented("voice-a", 1).await.unwrap();
    let b = second.open_udp_fragmented("voice-b", 1).await.unwrap();
    assert_eq!(a.max_payload(), Some(4096));
    assert_eq!(b.max_payload(), Some(4096));
    let before_a = first.connection().stats();
    let before_b = second.connection().stats();
    timeout(Duration::from_secs(8), async {
        tokio::join!(tcp_roundtrip(&first), async {
            for size in [0, 1427, 2108, 4096] {
                let data = vec![0x31; size];
                let other = vec![0xa7; size];
                tokio::join!(echo(&a, &data, 0), echo(&b, &other, 0x40));
                sleep(Duration::from_millis(30)).await;
            }
        });
    })
    .await
    .unwrap();
    let after_a = first.connection().stats();
    let after_b = second.connection().stats();
    for (before, after) in [(before_a, after_a), (before_b, after_b)] {
        // Empty=1, 1427=2, 2108=2, 4096=4 QUIC DATAGRAM frames in each direction.
        assert!(after.frame_tx.datagram - before.frame_tx.datagram >= 9);
        assert!(after.frame_rx.datagram - before.frame_rx.datagram >= 9);
    }
    assert_eq!(lab.counts[0].load(Ordering::SeqCst), 4);
    assert_eq!(lab.counts[1].load(Ordering::SeqCst), 4);
    assert_eq!(a.send(&[0; 4097]).await, Err(Error::DatagramTooLarge));
    sleep(Duration::from_millis(80)).await;
    assert_eq!(lab.counts[0].load(Ordering::SeqCst), 4);
    a.shutdown().await;
    b.shutdown().await;
    assert_eq!(a.send(b"closed").await, Err(Error::Closed));
    drop(a);
    drop(b);
    drop(first);
    drop(second);
    lab.cleanup().await;
}

// Deliberately construct wire bytes independently from the production encoder.
fn raw(
    a: &FragmentedUdpAssociation,
    id: u64,
    total: u16,
    index: u8,
    count: u8,
    payload: &[u8],
) -> Bytes {
    let mut wire = vec![2, 0];
    wire.extend_from_slice(&a.service_handle().to_be_bytes());
    wire.extend_from_slice(&a.association_id().to_be_bytes());
    wire.extend_from_slice(&a.generation().to_be_bytes());
    wire.extend_from_slice(&id.to_be_bytes());
    wire.extend_from_slice(&total.to_be_bytes());
    wire.extend_from_slice(&[index, count]);
    wire.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    wire.extend_from_slice(payload);
    wire.into()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_reorder_duplicate_expiry_scope_and_generation_do_not_deliver_partials() {
    let mut lab = Lab::new(Limits::default(), UdpReturnPolicy::ReplyBudget).await;
    let guest = lab.guest(0, caps()).await;
    let a = guest.open_udp_fragmented("voice-a", 1).await.unwrap();
    let p = [0x6a; 1024];
    let first = raw(&a, 1, 2048, 0, 2, &p);
    let last = raw(&a, 1, 2048, 1, 2, &p);
    guest.connection().send_datagram(last.clone()).unwrap();
    guest.connection().send_datagram(last).unwrap();
    sleep(Duration::from_millis(40)).await;
    assert_eq!(lab.counts[0].load(Ordering::SeqCst), 0);
    guest.connection().send_datagram(first.clone()).unwrap();
    let output = timeout(Duration::from_secs(2), a.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(output.as_ref(), [0x6a; 2048]);
    drop(output);
    guest.connection().send_datagram(first).unwrap();
    guest
        .connection()
        .send_datagram(raw(&a, 2, 2048, 0, 2, &p))
        .unwrap();
    sleep(Duration::from_millis(320)).await;
    guest
        .connection()
        .send_datagram(raw(&a, 2, 2048, 1, 2, &p))
        .unwrap();
    for field in [5usize, 13, 17] {
        let mut wrong = raw(&a, 3, 1, 0, 1, b"x").to_vec();
        wrong[field] ^= 0x40;
        guest.connection().send_datagram(wrong.into()).unwrap();
    }
    // A V1 envelope with this V2 association is refused as well.
    guest
        .connection()
        .send_datagram(
            DatagramFrame {
                service_handle: a.service_handle(),
                association_id: a.association_id(),
                generation: a.generation(),
                sequence: 1,
                payload: Bytes::from_static(b"v1"),
            }
            .encode()
            .unwrap(),
        )
        .unwrap();
    sleep(Duration::from_millis(100)).await;
    assert_eq!(lab.counts[0].load(Ordering::SeqCst), 1);
    assert!(lab.host.as_ref().unwrap().metrics().udp_dropped >= 5);
    a.shutdown().await;
    drop(a);
    drop(guest);
    lab.cleanup().await;
}

async fn raw_open(guest: &Guest, request: Value) -> Value {
    let (mut send, mut recv) = guest.connection().open_bi().await.unwrap();
    let bytes = serde_json::to_vec(&request).unwrap();
    send.write_all(&(bytes.len() as u32).to_be_bytes())
        .await
        .unwrap();
    send.write_all(&bytes).await.unwrap();
    send.finish().unwrap();
    let bytes = recv.read_to_end(16388).await.unwrap();
    serde_json::from_slice(&bytes[4..]).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_mode_capability_revision_and_legacy_gates_are_enforced() {
    let mut lab = Lab::new(Limits::default(), UdpReturnPolicy::ReplyBudget).await;
    let guest = lab.guest(0, caps()).await;
    assert!(matches!(
        guest.open_udp("voice-a", 1).await,
        Err(Error::CapabilityUnavailable)
    ));
    assert!(matches!(
        guest.open_udp_fragmented("plain", 1).await,
        Err(Error::CapabilityUnavailable)
    ));
    assert!(matches!(
        guest.open_udp_fragmented("voice-a", 2).await,
        Err(Error::StaleRevision)
    ));
    assert!(matches!(
        guest.open_udp_fragmented("not-approved", 1).await,
        Err(Error::ServiceDenied)
    ));
    let old = lab.guest(1, vec![Capability::Datagram]).await;
    assert!(matches!(
        old.open_udp_fragmented("voice-a", 1).await,
        Err(Error::CapabilityUnavailable)
    ));
    let denied = raw_open(&old, json!({"schemaVersion":1,"requestId":1,"serviceId":"voice-a","serviceRevision":1,"protocol":"udp","operation":"openFragmented","associationId":0})).await;
    assert_eq!(denied["error"], "capability_unavailable");
    assert_eq!(
        lab.host.as_ref().unwrap().metrics().udp_associations_opened,
        0
    );
    assert_eq!(lab.counts[0].load(Ordering::SeqCst), 0);
    // The raw request used an independent request-id sequence; exercise the
    // unchanged V1 API through the other, internally sequenced agent.
    let legacy = guest.open_udp("plain", 1).await.unwrap();
    legacy.send(b"v1 intact").await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(2), legacy.recv())
            .await
            .unwrap()
            .unwrap()
            .as_ref(),
        b"v1 intact"
    );
    // Existing invitation schema only grants ordinary game/aux services.
    let mut auxiliary = lab.services[0].clone();
    auxiliary.service_id = "aux".into();
    let game = lab.services[3].clone();
    assert!(
        Invitation::issue(
            &generate_key(),
            generate_key().public(),
            "legacy-invitation",
            lab.host.as_ref().unwrap().bound_sockets()[0],
            &[game, auxiliary],
            unix_millis() + 10_000
        )
        .is_err()
    );
    let mut plan: Value =
        serde_json::from_str(include_str!("../examples/host-downstream.json")).unwrap();
    plan["services"][1]["requiredCapability"] = json!("fragmentedDatagram");
    let path = std::env::temp_dir().join(format!(
        "aurora-fragment-plan-{}.json",
        generate_key().public()
    ));
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(&serde_json::to_vec(&plan).unwrap()).unwrap();
    }
    let denied = load_plan(&path, None).is_err();
    std::fs::remove_file(&path).unwrap();
    assert!(denied, "legacy_plan_accepted_fragmented_capability");
    legacy.shutdown().await;
    drop(legacy);
    drop(guest);
    drop(old);
    lab.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn loopback_bridge_isolates_local_sources_and_rebinds_after_revocation() {
    let mut lab = Lab::new(Limits::default(), UdpReturnPolicy::ReplyBudget).await;
    let guest = lab.guest(0, caps()).await;
    let bridge = guest
        .udp_bridge_fragmented("voice-a", 1, local())
        .await
        .unwrap();
    let address = bridge.local_addr();
    assert_eq!(bridge.max_payload(), Some(4096));
    let one = UdpSocket::bind(local()).await.unwrap();
    let two = UdpSocket::bind(local()).await.unwrap();
    for (socket, marker) in [(&one, 0x31u8), (&two, 0xab)] {
        socket.send_to(&vec![marker; 2108], address).await.unwrap();
        let mut data = vec![0; 4097];
        let (size, source) = timeout(Duration::from_secs(2), socket.recv_from(&mut data))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(source, address);
        assert_eq!(&data[..size], vec![marker; 2108]);
    }
    assert_eq!(
        lab.host.as_ref().unwrap().metrics().udp_associations_opened,
        2
    );
    assert!(lab.host.as_ref().unwrap().revoke("peer-0"));
    timeout(Duration::from_secs(2), guest.connection().closed())
        .await
        .unwrap();
    bridge.shutdown().await;
    drop(bridge);
    assert!(UdpSocket::bind(address).await.is_ok());
    let origin_addresses = [one.local_addr().unwrap(), two.local_addr().unwrap()];
    drop(one);
    drop(two);
    for address in origin_addresses {
        assert!(UdpSocket::bind(address).await.is_ok());
    }
    drop(guest);
    lab.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_fragment_is_rate_charged_and_incomplete_message_never_reaches_udp() {
    let limits = Limits {
        datagrams_per_second: 1,
        datagram_burst: 1,
        ..Limits::default()
    };
    let mut lab = Lab::new(limits, UdpReturnPolicy::ReplyBudget).await;
    let guest = lab.guest(0, caps()).await;
    let a = guest.open_udp_fragmented("voice-a", 1).await.unwrap();
    guest
        .connection()
        .send_datagram(raw(&a, 1, 2048, 0, 2, &[0; 1024]))
        .unwrap();
    guest
        .connection()
        .send_datagram(raw(&a, 1, 2048, 1, 2, &[0; 1024]))
        .unwrap();
    sleep(Duration::from_millis(320)).await;
    assert_eq!(lab.counts[0].load(Ordering::SeqCst), 0);
    assert!(lab.host.as_ref().unwrap().metrics().udp_dropped >= 1);
    a.shutdown().await;
    drop(a);
    drop(guest);
    lab.cleanup().await;
}

#[tokio::test]
async fn oversized_fragmented_service_is_rejected_before_bind() {
    let endpoint = UdpSocket::bind(local()).await.unwrap();
    let mut service = udp_service(
        "aux",
        endpoint.local_addr().unwrap(),
        true,
        UdpReturnPolicy::ReplyBudget,
    );
    service.max_datagram_bytes = 4097;
    let key = generate_key();
    assert!(matches!(
        Host::start(HostConfig {
            secret_key: generate_key(),
            bind_addr: local(),
            session_id: "oversize-service".into(),
            services: vec![service],
            leases: vec![Lease {
                lease_id: "peer".into(),
                peer: key.public(),
                expires_at_unix_ms: unix_millis() + 10_000,
                services: BTreeSet::from(["aux".into()])
            }],
            limits: Limits::default()
        })
        .await,
        Err(Error::Configuration)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn downstream_expansion_requires_explicit_policy_and_oversize_has_no_partial_return() {
    for authorized in [false, true] {
        let policy = if authorized {
            UdpReturnPolicy::AuthorizedDownstream {
                bytes_per_second: 100_000,
                datagrams_per_second: 200,
            }
        } else {
            UdpReturnPolicy::ReplyBudget
        };
        let mut lab = Lab::new(Limits::default(), policy).await;
        let guest = lab.guest(0, caps()).await;
        let a = guest.open_udp_fragmented("voice-a", 1).await.unwrap();
        echo(&a, b"x", 0).await;
        let source = lab.sources[0].lock().unwrap().unwrap();
        lab.destinations[0]
            .send_to(&[0x45; 4097], source)
            .await
            .unwrap();
        sleep(Duration::from_millis(60)).await;
        assert!(lab.host.as_ref().unwrap().metrics().udp_oversize >= 1);
        lab.destinations[0]
            .send_to(&[0x61; 2108], source)
            .await
            .unwrap();
        let received = timeout(Duration::from_millis(400), a.recv()).await;
        if authorized {
            assert_eq!(received.unwrap().unwrap().as_ref(), [0x61; 2108]);
        } else {
            assert!(received.is_err(), "reply_budget_amplified");
        }
        a.shutdown().await;
        drop(a);
        drop(guest);
        lab.cleanup().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn revocation_before_last_fragment_refuses_real_delivery() {
    let mut lab = Lab::new(Limits::default(), UdpReturnPolicy::ReplyBudget).await;
    let guest = lab.guest(0, caps()).await;
    let a = guest.open_udp_fragmented("voice-a", 1).await.unwrap();
    guest
        .connection()
        .send_datagram(raw(&a, 1, 2048, 0, 2, &[0; 1024]))
        .unwrap();
    sleep(Duration::from_millis(30)).await;
    assert_eq!(lab.counts[0].load(Ordering::SeqCst), 0);
    assert!(lab.host.as_ref().unwrap().revoke("peer-0"));
    // No await between revoke and enqueue: the watcher cannot run on this
    // runtime thread first. This proves real refusal after revocation; the pure
    // authority test separately proves independence from watcher propagation.
    let _ = guest
        .connection()
        .send_datagram(raw(&a, 1, 2048, 1, 2, &[0; 1024]));
    sleep(Duration::from_millis(80)).await;
    assert_eq!(lab.counts[0].load(Ordering::SeqCst), 0);
    a.shutdown().await;
    drop(a);
    drop(guest);
    lab.cleanup().await;
}
