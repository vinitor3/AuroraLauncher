//! Explicit development CLI. No persisted keys, cloud services or launcher hooks.
use aurora_connect_lab::{
    Capability, EndpointAddr, EndpointId, Guest, GuestConfig, Host, HostConfig, Lease, Limits,
    Protocol, Service, UdpReturnPolicy, generate_key, load_plan,
};
use serde_json::json;
use std::{
    error::Error,
    io::{BufRead, Read, Write},
    net::SocketAddr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    task::JoinSet,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

fn event(value: serde_json::Value) {
    println!("{value}");
    let _ = std::io::stdout().flush();
}

fn input() -> Result<String> {
    let mut line = String::new();
    std::io::stdin().lock().take(513).read_line(&mut line)?;
    if line.len() > 512 || line.is_empty() {
        return Err("invalid_input".into());
    }
    Ok(line.trim_start_matches('\u{feff}').trim().to_string())
}

async fn until_stop(seconds: u64) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = input();
        let _ = tx.send(());
    });
    tokio::select! {
        _ = rx => {},
        _ = tokio::signal::ctrl_c() => {},
        _ = tokio::time::sleep(Duration::from_secs(seconds)) => {},
    }
}

fn duration(arg: &str) -> Result<u64> {
    let n: u64 = arg.parse()?;
    if !(10..=3600).contains(&n) {
        return Err("duration_must_be_10_to_3600_seconds".into());
    }
    Ok(n)
}

fn service(id: &str, protocol: Protocol, endpoint: SocketAddr) -> Service {
    Service {
        udp_return_policy: UdpReturnPolicy::ReplyBudget,
        service_id: id.into(),
        required_capability: if protocol == Protocol::Tcp {
            Capability::ReliableStream
        } else {
            Capability::Datagram
        },
        protocol,
        endpoint,
        revision: 1,
        max_datagram_bytes: 1400,
    }
}

async fn host(args: &[String]) -> Result<()> {
    if args.len() != 4 {
        return Err("host_args".into());
    }
    let bind_addr: SocketAddr = args[0].parse()?;
    let tcp: SocketAddr = args[1].parse()?;
    let udp: SocketAddr = args[2].parse()?;
    let seconds = duration(&args[3])?;
    run_host(
        bind_addr,
        seconds,
        vec![
            service("game", Protocol::Tcp, tcp),
            service("aux", Protocol::Udp, udp),
        ],
        None,
    )
    .await
}

fn plan(args: &[String]) -> Result<()> {
    if args.len() != 1 {
        return Err("plan_args".into());
    }
    let plan = load_plan(&args[0], None)?;
    event(json!({"event":"plan_ready", "plan":plan.preview()}));
    Ok(())
}

async fn host_plan(args: &[String]) -> Result<()> {
    if args.len() != 2 {
        return Err("host_plan_args".into());
    }
    // Load/hash/validate before opening transport or asking for a peer. The immutable
    // in-memory services are exactly the bytes selected by the local operator.
    let plan = load_plan(&args[0], Some(&args[1]))?;
    run_host(plan.bind, plan.seconds, plan.services, Some(plan.hash)).await
}

async fn run_host(
    bind_addr: SocketAddr,
    seconds: u64,
    services: Vec<Service>,
    plan_hash: Option<String>,
) -> Result<()> {
    if bind_addr.port() == 0
        || services
            .iter()
            .any(|service| !service.endpoint.ip().is_loopback() || service.endpoint.port() == 0)
    {
        return Err(aurora_connect_lab::Error::Configuration.into());
    }
    let approved_ids = services
        .iter()
        .map(|service| service.service_id.clone())
        .collect();
    let approval: Vec<_> = services.iter().map(|service| {
        let policy = match service.udp_return_policy {
            UdpReturnPolicy::ReplyBudget => json!({"type":"replyBudget"}),
            UdpReturnPolicy::AuthorizedDownstream { bytes_per_second, datagrams_per_second } =>
                json!({"type":"authorizedDownstream", "bytesPerSecond":bytes_per_second, "datagramsPerSecond":datagrams_per_second}),
        };
        json!({"serviceId":service.service_id, "protocol":service.protocol,
            "endpoint":service.endpoint.to_string(), "revision":service.revision,
            "maxDatagramBytes":service.max_datagram_bytes,
            "requiredCapability":service.required_capability, "udpReturnPolicy":policy})
    }).collect();
    let secret_key = generate_key();
    // The public key is fresh each run, hence also provides a fresh public session ID.
    let session_id = secret_key.public().to_string();
    event(
        json!({"event":"host_identity", "peer":session_id, "session":session_id, "bind":bind_addr.to_string(),
            "planHash":plan_hash, "approvedServices":approval, "durationSeconds":seconds,
            "next":"review these services, then enter the guest public peer ID to authorize; Ctrl+C cancels"}),
    );
    let peer: EndpointId = input()?
        .parse()
        .map_err(|_| aurora_connect_lab::Error::Unauthorized)?;
    let expires_at_unix_ms =
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64 + seconds * 1000;
    let host = Host::start(HostConfig {
        secret_key,
        bind_addr,
        session_id,
        services,
        leases: vec![Lease {
            lease_id: "manual-lab".into(),
            peer,
            expires_at_unix_ms,
            services: approved_ids,
        }],
        limits: Limits::default(),
    })
    .await?;
    event(
        json!({"event":"host_ready", "experimental":true, "relay":false, "next":"enter stop to revoke and close"}),
    );
    until_stop(seconds).await;
    host.revoke("manual-lab");
    host.shutdown().await;
    event(json!({"event":"host_stopped"}));
    Ok(())
}

async fn guest(args: &[String]) -> Result<()> {
    if args.len() != 6 {
        return Err("guest_args".into());
    }
    let peer: EndpointId = args[0].parse()?;
    let address: SocketAddr = args[1].parse()?;
    let session_id = args[2].clone();
    let tcp: SocketAddr = args[3].parse()?;
    let udp: SocketAddr = args[4].parse()?;
    let seconds = duration(&args[5])?;
    let secret_key = generate_key();
    event(
        json!({"event":"guest_identity", "peer":secret_key.public().to_string(), "next":"authorize this public peer at host, then enter ready"}),
    );
    if input()? != "ready" {
        return Err("expected_ready".into());
    }
    let guest = Guest::connect(GuestConfig {
        secret_key,
        bind_addr: if address.is_ipv4() {
            "0.0.0.0:0".parse()?
        } else {
            "[::]:0".parse()?
        },
        host: EndpointAddr::new(peer).with_ip_addr(address),
        session_id,
        lease_id: "manual-lab".into(),
        capabilities: vec![Capability::ReliableStream, Capability::Datagram],
        limits: Limits::default(),
    })
    .await?;
    let tcp_bridge = guest.tcp_bridge("game", 1, tcp).await?;
    let udp_bridge = guest.udp_bridge("aux", 1, udp).await?;
    event(
        json!({"event":"guest_ready", "tcp":tcp_bridge.local_addr().to_string(), "udp":udp_bridge.local_addr().to_string(), "maxDatagramPayload":udp_bridge.max_payload(), "experimental":true, "next":"enter stop to close"}),
    );
    until_stop(seconds).await;
    guest.shutdown().await;
    udp_bridge.shutdown().await;
    tcp_bridge.shutdown().await;
    event(json!({"event":"guest_stopped"}));
    Ok(())
}

async fn echo(args: &[String], downstream: bool) -> Result<()> {
    if args.len() != 3 {
        return Err("echo_args".into());
    }
    let tcp_addr: SocketAddr = args[0].parse()?;
    let udp_addr: SocketAddr = args[1].parse()?;
    if !tcp_addr.ip().is_loopback() || !udp_addr.ip().is_loopback() {
        return Err("echo_requires_loopback".into());
    }
    let seconds = duration(&args[2])?;
    let tcp = TcpListener::bind(tcp_addr).await?;
    let udp = UdpSocket::bind(udp_addr).await?;
    event(
        json!({"event":"echo_ready", "tcp":tcp.local_addr()?.to_string(), "udp":udp.local_addr()?.to_string()}),
    );
    let stop = until_stop(seconds);
    tokio::pin!(stop);
    let mut tasks = JoinSet::new();
    let mut buffer = vec![0; 65536];
    loop {
        tokio::select! {
            _ = &mut stop => break,
            result = tcp.accept() => {
                let (mut stream, _) = result?;
                if tasks.len() < 32 { tasks.spawn(async move {
                    let (mut read, mut write) = stream.split();
                    let _ = tokio::time::timeout(Duration::from_secs(30), tokio::io::copy(&mut read, &mut write)).await;
                    let _ = write.shutdown().await;
                }); }
            },
            result = udp.recv_from(&mut buffer) => {
                let (n, source) = result?;
                if source.ip().is_loopback() {
                    if downstream {
                        // Controlled synthetic service for asymmetric-return tests only.
                        if n == 1 {
                            let mut reply = [buffer[0]; 512];
                            for sequence in 0..8 {
                                reply[1] = sequence;
                                udp.send_to(&reply, source).await?;
                            }
                        }
                    } else { udp.send_to(&buffer[..n], source).await?; }
                }
            },
            _ = tasks.join_next(), if !tasks.is_empty() => {},
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    event(json!({"event":"echo_stopped"}));
    Ok(())
}

async fn probe(args: &[String], downstream: bool) -> Result<()> {
    if args.len() != 2 {
        return Err("probe_args".into());
    }
    let tcp: SocketAddr = args[0].parse()?;
    let udp: SocketAddr = args[1].parse()?;
    if !tcp.ip().is_loopback() || !udp.ip().is_loopback() {
        return Err("probe_requires_loopback".into());
    }
    let started = std::time::Instant::now();
    let payload: Vec<u8> = (0..1_048_576).map(|i| (i % 251) as u8).collect();
    let mut stream = TcpStream::connect(tcp).await?;
    let (mut read, mut write) = stream.split();
    let send = async {
        write.write_all(&payload).await?;
        write.shutdown().await
    };
    let receive = async {
        let mut bytes = Vec::new();
        (&mut read)
            .take(payload.len() as u64 + 1)
            .read_to_end(&mut bytes)
            .await?;
        Ok::<_, std::io::Error>(bytes)
    };
    let (sent, received) = tokio::join!(send, receive);
    sent?;
    if received? != payload {
        return Err("tcp_integrity_failed".into());
    }
    let mut packets = 0;
    let mut requests = 0;
    let mut udp_bytes_sent = 0;
    let mut udp_bytes_received = 0;
    let mut roundtrips_micros = Vec::new();
    for marker in [19u8, 71u8] {
        let socket = UdpSocket::bind(if udp.is_ipv4() {
            "127.0.0.1:0"
        } else {
            "[::1]:0"
        })
        .await?;
        socket.connect(udp).await?;
        if downstream {
            let packet_started = std::time::Instant::now();
            socket.send(&[marker]).await?;
            requests += 1;
            udp_bytes_sent += 1;
            let mut seen = [false; 8];
            for _ in 0..8 {
                let mut reply = vec![0; 65536];
                let n =
                    tokio::time::timeout(Duration::from_secs(5), socket.recv(&mut reply)).await??;
                if n != 512
                    || reply[0] != marker
                    || reply[1] >= 8
                    || reply[2..n].iter().any(|byte| *byte != marker)
                    || seen[reply[1] as usize]
                {
                    return Err("udp_downstream_integrity_failed".into());
                }
                seen[reply[1] as usize] = true;
                packets += 1;
                udp_bytes_received += n;
                // Request-to-each-response timings, not per-packet RTT or audio delay.
                roundtrips_micros.push(packet_started.elapsed().as_micros());
            }
            continue;
        }
        for length in [0usize, 1, 32, 512, 1024] {
            let payload = vec![marker; length];
            let packet_started = std::time::Instant::now();
            socket.send(&payload).await?;
            requests += 1;
            udp_bytes_sent += length;
            let mut reply = vec![0; 65536];
            let n = tokio::time::timeout(Duration::from_secs(5), socket.recv(&mut reply)).await??;
            if reply[..n] != payload {
                return Err("udp_integrity_failed".into());
            }
            roundtrips_micros.push(packet_started.elapsed().as_micros());
            udp_bytes_received += n;
            packets += 1;
        }
    }
    let jitter = roundtrips_micros
        .windows(2)
        .map(|pair| pair[0].abs_diff(pair[1]))
        .max()
        .unwrap_or(0);
    roundtrips_micros.sort_unstable();
    event(
        json!({"event":"probe_passed", "tcpBytesEachDirection":payload.len(), "udpPackets":packets,
            "udpRequests":requests, "udpPayloadBytesSent":udp_bytes_sent, "udpPayloadBytesReceived":udp_bytes_received,
            "udpMeasurement":if downstream { "one_request_to_each_response" } else { "request_response_roundtrip" },
            "localSources":2, "elapsedMs":started.elapsed().as_millis(), "udpApplicationRoundtripMicrosP50":roundtrips_micros[roundtrips_micros.len()/2], "udpApplicationRoundtripMicrosMax":roundtrips_micros.last(), "udpAdjacentRoundtripVariationMicrosMax":jitter, "minecraft":"not_tested", "voice":"not_tested"}),
    );
    Ok(())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("host") => host(&args[1..]).await,
        Some("plan") => plan(&args[1..]),
        Some("host-plan") => host_plan(&args[1..]).await,
        Some("guest") => guest(&args[1..]).await,
        Some("echo") => echo(&args[1..], false).await,
        Some("echo-downstream") => echo(&args[1..], true).await,
        Some("probe") | Some("probe-downstream") => {
            match tokio::time::timeout(
                Duration::from_secs(30),
                probe(&args[1..], args[0] == "probe-downstream"),
            )
            .await
            {
                Ok(v) => v,
                Err(_) => Err("probe_timeout".into()),
            }
        }
        _ => {
            println!(
                "EXPERIMENTAL - no launcher integration, keys only in memory\nhost <quic-bind> <approved-tcp-target> <approved-udp-target> <seconds>\nplan <local-service-plan.json>\nhost-plan <local-service-plan.json> <approved-sha256>\nguest <host-public-id> <host-direct-address> <session-id> <local-tcp> <local-udp> <seconds>\necho <loopback-tcp> <loopback-udp> <seconds>\nprobe <loopback-tcp> <loopback-udp>\necho-downstream / probe-downstream: same arguments, controlled asymmetric UDP test only\nUse stop on stdin or Ctrl+C to end. See docs/aurora-connect.md."
            );
            Ok(())
        }
    };
    if let Err(error) = result {
        // Avoid echoing raw parse/IO errors containing connectivity or credential data.
        let code = error
            .downcast_ref::<aurora_connect_lab::Error>()
            .map(|e| e.code())
            .or_else(|| {
                error
                    .downcast_ref::<aurora_connect_lab::PlanError>()
                    .map(ToString::to_string)
            })
            .unwrap_or_else(|| "lab_operation_failed".into());
        event(
            json!({"event":"failed", "code":code, "hint":"check arguments, authorization, local ports, capability and deadline; no NAT diagnosis inferred"}),
        );
        std::process::exit(1);
    }
}
