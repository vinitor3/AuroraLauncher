//! Explicitly opted-in manual transport sample on this computer's own IPv4.
//! A successful sample does not establish LAN, NAT traversal or internet reachability.
//! No address discovery, relay, router or firewall changes are performed here.
use aurora_connect_lab::*;
use std::{
    net::{Ipv4Addr, SocketAddr},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    task::{JoinHandle, JoinSet},
    time::timeout,
};
use tokio_util::sync::CancellationToken;

const OWN_IPV4_ENV: &str = "AURORA_CONNECT_OWN_IPV4_TEST_ADDR";
const TCP_BYTES: usize = 1024 * 1024;
const UDP_BYTES: usize = 512;
type TestResult<T = ()> = std::result::Result<T, &'static str>;

fn own_ipv4(value: Option<&str>) -> TestResult<Ipv4Addr> {
    let value = value.ok_or("own_ipv4_explicit_value_required")?;
    let ip: Ipv4Addr = value.parse().map_err(|_| "own_ipv4_literal_required")?;
    if ip.to_string() != value
        || ip.octets()[0] == 0
        || ip.octets()[0] >= 224
        || ip.is_loopback()
        || ip.is_link_local()
    {
        return Err("own_ipv4_non_loopback_unicast_required");
    }
    Ok(ip)
}

fn loopback() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
}

fn service(id: &str, protocol: Protocol, endpoint: SocketAddr) -> Service {
    Service {
        service_id: id.into(),
        protocol,
        endpoint,
        revision: 1,
        max_datagram_bytes: 1024,
        required_capability: if protocol == Protocol::Tcp {
            Capability::ReliableStream
        } else {
            Capability::Datagram
        },
        udp_return_policy: UdpReturnPolicy::ReplyBudget,
    }
}

#[test]
fn own_ipv4_input_is_explicit_canonical_unicast_without_io() {
    for text in ["10.23.4.5", "172.16.2.3", "192.168.2.3", "100.64.2.3"] {
        assert!(own_ipv4(Some(text)).is_ok(), "literal unicast accepted");
    }
    // Parsing does not prove assignment, routing or CGNAT; the opted-in test
    // must bind the supplied address successfully before any dial.
    assert!(own_ipv4(None).is_err(), "missing opt-in refused");
    for text in [
        "",
        "localhost",
        "127.0.0.1",
        "127.1.2.3",
        "0.0.0.0",
        "0.1.2.3",
        "169.254.2.3",
        "224.0.0.1",
        "239.1.2.3",
        "240.0.0.1",
        "255.255.255.255",
        "10.23.4.5:42000",
        "10.23.4.5/24",
        " 10.23.4.5",
        "10.23.4.5\n",
        "010.23.4.5",
        "[::1]",
        "::ffff:10.23.4.5",
    ] {
        assert!(own_ipv4(Some(text)).is_err(), "invalid input refused");
    }
}

async fn finish_task(task: Option<JoinHandle<()>>) -> bool {
    let Some(mut task) = task else {
        return true;
    };
    match timeout(Duration::from_secs(3), &mut task).await {
        Ok(result) => result.is_ok(),
        Err(_) => {
            task.abort();
            let _ = task.await;
            false
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicit own non-loopback IPv4; binds locally before dial; not a LAN/NAT/internet test"]
async fn own_ipv4_manual_tcp_udp_revocation_cleanup() {
    let started = Instant::now();
    let supplied = std::env::var(OWN_IPV4_ENV).ok();
    let ip = own_ipv4(supplied.as_deref()).expect("explicit own IPv4 input gate");
    // Only the supplied address can be used for the transport. Port zero asks
    // the OS for one local port; there is no search or dial of other addresses.
    let reservation = std::net::UdpSocket::bind(SocketAddr::from((ip, 0)))
        .map_err(|_| "own_ipv4_local_bind_failed")
        .expect("own IPv4 must bind before any dial");
    let address = reservation
        .local_addr()
        .map_err(|_| "own_ipv4_local_address_failed")
        .expect("owned reservation address");
    let cancel = CancellationToken::new();
    let mut tcp_task = None;
    let mut udp_task = None;
    let mut host: Option<Host> = None;
    let mut guest: Option<Guest> = None;
    let mut tcp_bridge: Option<TcpBridge> = None;
    let mut udp_bridge: Option<UdpBridge> = None;
    let mut tcp_rebind = Vec::new();
    let mut udp_rebind = vec![address];
    let mut revoked = false;
    let mut closed_after_revoke = false;

    // All fallible traffic work is inside this bounded future so normal error
    // paths reach the same awaited cleanup before an assertion can fail.
    let operation = timeout(Duration::from_secs(25), async {
        let tcp = TcpListener::bind(loopback())
            .await
            .map_err(|_| "echo_tcp_bind")?;
        let tcp_address = tcp.local_addr().map_err(|_| "echo_tcp_address")?;
        tcp_rebind.push(tcp_address);
        let udp = UdpSocket::bind(loopback())
            .await
            .map_err(|_| "echo_udp_bind")?;
        let udp_address = udp.local_addr().map_err(|_| "echo_udp_address")?;
        udp_rebind.push(udp_address);
        let tcp_cancel = cancel.clone();
        tcp_task = Some(tokio::spawn(async move {
            let mut children = JoinSet::new();
            loop {
                tokio::select! {
                    _ = tcp_cancel.cancelled() => break,
                    accepted = tcp.accept() => {
                        let Ok((mut socket, _)) = accepted else { break };
                        children.spawn(async move {
                            let (mut read, mut write) = socket.split();
                            let _ = tokio::io::copy(&mut read, &mut write).await;
                            let _ = write.shutdown().await;
                        });
                    },
                    _ = children.join_next(), if !children.is_empty() => {},
                }
            }
            children.shutdown().await;
        }));
        let udp_cancel = cancel.clone();
        udp_task = Some(tokio::spawn(async move {
            let mut bytes = [0; 1025];
            loop {
                tokio::select! {
                    _ = udp_cancel.cancelled() => break,
                    packet = udp.recv_from(&mut bytes) => {
                        let Ok((count, source)) = packet else { break };
                        let _ = udp.send_to(&bytes[..count], source).await;
                    },
                }
            }
        }));
        let host_key = generate_key();
        let host_id = host_key.public();
        let guest_key = generate_key();
        drop(reservation);
        host = Some(
            Host::start(HostConfig {
                secret_key: host_key,
                bind_addr: address,
                session_id: "own-ipv4-manual".into(),
                services: vec![
                    service("game", Protocol::Tcp, tcp_address),
                    service("aux", Protocol::Udp, udp_address),
                ],
                leases: vec![Lease {
                    lease_id: "own-ipv4-lease".into(),
                    peer: guest_key.public(),
                    expires_at_unix_ms: unix_millis() + 60_000,
                    services: ["game".into(), "aux".into()].into_iter().collect(),
                }],
                limits: Limits::default(),
            })
            .await
            .map_err(|_| "host_start")?,
        );
        let current_host = host.as_ref().ok_or("host_missing")?;
        if current_host.bound_sockets() != vec![address] {
            return Err("host_explicit_bind_mismatch");
        }
        guest = Some(
            Guest::connect(GuestConfig {
                secret_key: guest_key,
                bind_addr: SocketAddr::from((ip, 0)),
                host: EndpointAddr::new(host_id).with_ip_addr(address),
                session_id: "own-ipv4-manual".into(),
                lease_id: "own-ipv4-lease".into(),
                capabilities: vec![Capability::ReliableStream, Capability::Datagram],
                limits: Limits::default(),
            })
            .await
            .map_err(|_| "guest_connect")?,
        );
        let current_guest = guest.as_ref().ok_or("guest_missing")?;
        let guest_bound = current_guest.bound_sockets();
        udp_rebind.extend(guest_bound.iter().copied());
        if guest_bound.len() != 1 || guest_bound[0].ip() != ip || guest_bound[0].port() == 0 {
            return Err("guest_explicit_bind_mismatch");
        }
        tcp_bridge = Some(
            current_guest
                .tcp_bridge("game", 1, loopback())
                .await
                .map_err(|_| "tcp_bridge")?,
        );
        udp_bridge = Some(
            current_guest
                .udp_bridge("aux", 1, loopback())
                .await
                .map_err(|_| "udp_bridge")?,
        );
        let bridge_tcp = tcp_bridge
            .as_ref()
            .ok_or("tcp_bridge_missing")?
            .local_addr();
        let bridge_udp = udp_bridge
            .as_ref()
            .ok_or("udp_bridge_missing")?
            .local_addr();
        tcp_rebind.push(bridge_tcp);
        udp_rebind.push(bridge_udp);

        let socket = TcpStream::connect(bridge_tcp)
            .await
            .map_err(|_| "tcp_connect")?;
        let (mut read, mut write) = socket.into_split();
        let payload = vec![0x45; TCP_BYTES];
        let mut reply = vec![0; TCP_BYTES];
        tokio::try_join!(
            async {
                write.write_all(&payload).await.map_err(|_| "tcp_send")?;
                write.shutdown().await.map_err(|_| "tcp_half_close")
            },
            async {
                read.read_exact(&mut reply)
                    .await
                    .map_err(|_| "tcp_receive")?;
                let mut extra = [0; 1];
                if read.read(&mut extra).await.map_err(|_| "tcp_eof")? != 0 {
                    return Err("tcp_extra_bytes");
                }
                Ok(())
            }
        )?;
        if reply != payload {
            return Err("tcp_payload_mismatch");
        }
        drop(read);
        drop(write);
        let left = UdpSocket::bind(loopback())
            .await
            .map_err(|_| "left_udp_bind")?;
        let right = UdpSocket::bind(loopback())
            .await
            .map_err(|_| "right_udp_bind")?;
        let left_address = left.local_addr().map_err(|_| "left_udp_address")?;
        let right_address = right.local_addr().map_err(|_| "right_udp_address")?;
        udp_rebind.extend([left_address, right_address]);
        if left_address == right_address {
            return Err("udp_sources_not_distinct");
        }
        // Both sources are alive and send before either reply is consumed.
        left.send_to(&[0x45; UDP_BYTES], bridge_udp)
            .await
            .map_err(|_| "left_udp_send")?;
        right
            .send_to(&[0x46; UDP_BYTES], bridge_udp)
            .await
            .map_err(|_| "right_udp_send")?;
        let mut left_reply = [0; UDP_BYTES + 1];
        let mut right_reply = [0; UDP_BYTES + 1];
        let ((left_count, left_source), (right_count, right_source)) = tokio::try_join!(
            async {
                left.recv_from(&mut left_reply)
                    .await
                    .map_err(|_| "left_udp_receive")
            },
            async {
                right
                    .recv_from(&mut right_reply)
                    .await
                    .map_err(|_| "right_udp_receive")
            },
        )?;
        if left_count != UDP_BYTES
            || left_reply[..left_count] != [0x45; UDP_BYTES]
            || left_source != bridge_udp
        {
            return Err("left_udp_return_mismatch");
        }
        if right_count != UDP_BYTES
            || right_reply[..right_count] != [0x46; UDP_BYTES]
            || right_source != bridge_udp
        {
            return Err("right_udp_return_mismatch");
        }
        // Packet delivery and its relaxed diagnostic counter can be observed
        // on different worker threads. Wait briefly for that final increment.
        timeout(Duration::from_secs(1), async {
            loop {
                let metrics = current_host.metrics();
                if metrics.destination_tcp_connects == 1
                    && metrics.tcp_bytes == (TCP_BYTES * 2) as u64
                    && metrics.udp_associations_opened == 2
                    && metrics.udp_forwarded == 2
                    && metrics.udp_returned == 2
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| "authenticated_transfer_counters_mismatch")?;
        revoked = current_host.revoke("own-ipv4-lease");
        closed_after_revoke = timeout(Duration::from_secs(3), current_guest.connection().closed())
            .await
            .is_ok();
        if !revoked || !closed_after_revoke {
            return Err("revocation_did_not_close_guest");
        }
        TestResult::Ok(())
    })
    .await;

    let mut cleanup = true;
    if let Some(value) = &tcp_bridge {
        cleanup &= timeout(Duration::from_secs(5), value.shutdown())
            .await
            .is_ok();
    }
    if let Some(value) = &udp_bridge {
        cleanup &= timeout(Duration::from_secs(5), value.shutdown())
            .await
            .is_ok();
    }
    drop(tcp_bridge);
    drop(udp_bridge);
    if let Some(value) = &guest {
        cleanup &= timeout(Duration::from_secs(5), value.shutdown())
            .await
            .is_ok();
    }
    if let Some(value) = &host {
        cleanup &= timeout(Duration::from_secs(5), value.shutdown())
            .await
            .is_ok();
    }
    let active_zero = host.as_ref().is_none_or(|value| {
        let metrics = value.metrics();
        metrics.active_peers == 0 && metrics.active_tcp == 0 && metrics.active_udp == 0
    });
    drop(guest);
    drop(host);
    cancel.cancel();
    cleanup &= finish_task(tcp_task).await;
    cleanup &= finish_task(udp_task).await;
    // Reacquire every listener/transport and both UDP client ports together.
    // TCP client's ephemeral port is excluded because normal TIME_WAIT applies.
    let tcp_rebound: Vec<_> = tcp_rebind.iter().map(std::net::TcpListener::bind).collect();
    let udp_rebound: Vec<_> = udp_rebind.iter().map(std::net::UdpSocket::bind).collect();
    let rebound = tcp_rebound.iter().all(std::result::Result::is_ok)
        && udp_rebound.iter().all(std::result::Result::is_ok);
    let stage = match operation {
        Ok(Ok(())) => "completed",
        Ok(Err(stage)) => stage,
        Err(_) => "operation_deadline",
    };
    let passed = stage == "completed" && cleanup && active_zero && rebound;
    println!(
        "{}",
        serde_json::json!({
            "event": "own_ipv4_manual_sample", "passed": passed, "stage": stage,
            "scope": "same_computer_own_ipv4", "relay": false, "elapsedMs": started.elapsed().as_millis(),
            "tcpBytesEachDirection": if stage == "completed" { TCP_BYTES } else { 0 },
            "udpSources": if stage == "completed" { 2 } else { 0 },
            "udpBytesEachReply": if stage == "completed" { UDP_BYTES } else { 0 },
            "revoked": revoked, "closedAfterRevoke": closed_after_revoke,
            "cleanup": cleanup, "activeZero": active_zero, "rebound": rebound,
            "tcpListenersRebound": tcp_rebound.iter().filter(|result| result.is_ok()).count(),
            "udpSocketsRebound": udp_rebound.iter().filter(|result| result.is_ok()).count(),
        })
    );
    assert!(
        passed,
        "own IPv4 sample failed; see redacted stage and cleanup counters"
    );
}
