//! Real owned relay, online schema5 admissions, Host-player ticket and opaque
//! application bytes. This is not Minecraft login or audible mod verification.
use crate::*;
use serde::Serialize;
use std::{net::SocketAddr, time::Duration};
use tokio::{
    net::{TcpListener, UdpSocket},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

#[path = "native_voice_budget_tests.rs"]
mod native_voice_budget_tests;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TicketBody {
    schema_version: u8,
    session_id: String,
    issuer_instance: String,
    peer_id: String,
    host_peer_id: String,
    account_id: String,
    generation: u64,
    issued_at_unix_ms: u64,
    expires_at_unix_ms: u64,
    direct_addresses: Vec<String>,
}
fn ticket(
    authority: &SecretKey,
    peer: EndpointId,
    host: EndpointId,
    origin: &str,
    expiry: u64,
) -> OnlineNetworkTicket {
    let body = TicketBody {
        schema_version: 1,
        session_id: "online-voice-test".into(),
        issuer_instance: "a".repeat(64),
        peer_id: peer.to_string(),
        host_peer_id: host.to_string(),
        account_id: "native-fixture".into(),
        generation: 1,
        issued_at_unix_ms: unix_millis(),
        expires_at_unix_ms: expiry,
        direct_addresses: Vec::new(),
    };
    let mut bytes = b"aurora/connect/relay-ticket/v1\0".to_vec();
    bytes.extend(serde_json::to_vec(&body).unwrap());
    let signature = authority
        .sign(&bytes)
        .to_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let encoded = serde_json::json!({"body":body,"signature":signature}).to_string();
    OnlineNetworkTicket::for_local_test(
        &encoded,
        authority.public(),
        &"a".repeat(64),
        "native-fixture",
        "online-voice-test",
        peer,
        host,
        origin,
    )
    .unwrap()
}
async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(12), future)
        .await
        .expect("owned_online_fixture_deadline")
}
async fn chunk(
    send: &mut iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    marker: u8,
) {
    let payload = vec![marker; 4096];
    send.write_all(&payload).await.unwrap();
    let mut returned = vec![0; 4096];
    bounded(recv.read_exact(&mut returned)).await.unwrap();
    assert_eq!(returned, payload);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn online_schema5_local_player_and_remote_guest_keep_tcp_after_terminal_aux_retirement() {
    let host_key = generate_key();
    let local_key = generate_key();
    let remote_key = generate_key();
    let authority = generate_key();
    let relay = LocalRelayFixture::start(
        [host_key.public(), remote_key.public()],
        LocalRelayProfile::Normal,
    )
    .await
    .unwrap();
    let origin = relay
        .ticket()
        .url()
        .as_str()
        .trim_end_matches('/')
        .to_owned();
    let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let tcp_target = tcp.local_addr().unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let udp_target = udp.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let stopped = cancel.clone();
    let echo = tokio::spawn(async move {
        let mut sessions = JoinSet::new();
        let mut buffer = vec![0_u8; 4097];
        let mut udp_bytes = 0;
        loop {
            tokio::select! {
                _ = stopped.cancelled() => break,
                accepted = tcp.accept() => {
                    let (mut socket, _) = accepted.unwrap();
                    sessions.spawn(async move { let (mut reader, mut writer) = socket.split(); let _ = tokio::io::copy(&mut reader, &mut writer).await; });
                },
                received = udp.recv_from(&mut buffer) => { let (size, sender) = received.unwrap(); udp_bytes += size*2; assert!(udp_bytes <= 65536); assert!(size <= 4096); udp.send_to(&buffer[..size], sender).await.unwrap(); },
                _ = sessions.join_next(), if !sessions.is_empty() => (),
            }
        }
        sessions.shutdown().await;
    });
    let services = vec![
        Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: tcp_target,
            revision: 1,
            max_datagram_bytes: 1400,
            required_capability: Capability::ReliableStream,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        },
        Service {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            endpoint: udp_target,
            revision: 1,
            max_datagram_bytes: 4096,
            required_capability: Capability::FragmentedDatagram,
            udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                bytes_per_second: 16384,
                datagrams_per_second: 20,
            },
        },
    ];
    let expiry = unix_millis() + 50_000;
    let deadlines = SessionDeadlines {
        admission_expires_at_unix_ms: expiry - 10_000,
        session_expires_at_unix_ms: expiry,
    };
    let local = SessionInvitation::issue_online_fragmented(
        &host_key,
        local_key.public(),
        "online-voice-test",
        &services,
        deadlines,
    )
    .unwrap();
    let remote = SessionInvitation::issue_online_fragmented(
        &host_key,
        remote_key.public(),
        "online-voice-test",
        &services,
        deadlines,
    )
    .unwrap();
    let host_ticket = ticket(
        &authority,
        host_key.public(),
        host_key.public(),
        &origin,
        expiry,
    );
    let remote_ticket = ticket(
        &authority,
        remote_key.public(),
        host_key.public(),
        &origin,
        expiry,
    );
    let host = bounded(Host::start_invited_online_fragmented(
        HostConfig {
            secret_key: host_key.clone(),
            bind_addr: "0.0.0.0:0".parse().unwrap(),
            session_id: "online-voice-test".into(),
            services: services.clone(),
            leases: vec![local.lease()],
            limits: Limits::default(),
        },
        &local,
        &host_ticket,
    ))
    .await
    .unwrap();
    host.admit_online(&remote).unwrap();
    let local_ticket = host.local_player_ticket(&local, &local_key).unwrap();
    assert!(
        bounded(Guest::connect_local_authorized(
            remote_key.clone(),
            &local,
            Limits::default(),
            &local_ticket
        ))
        .await
        .is_err()
    );
    let player = bounded(Guest::connect_local_authorized(
        local_key.clone(),
        &local,
        Limits::default(),
        &local_ticket,
    ))
    .await
    .unwrap();
    assert!(matches!(
        bounded(Guest::connect_local_authorized(
            local_key,
            &local,
            Limits::default(),
            &local_ticket
        ))
        .await,
        Err(Error::Replay)
    ));
    let guest = bounded(Guest::connect_invited_online_fragmented(
        remote_key,
        &remote,
        Limits::default(),
        &remote_ticket,
    ))
    .await
    .unwrap();
    assert!(host.invitation_consumed(local.invitation_id()));
    assert!(host.invitation_consumed(remote.invitation_id()));
    let wrong_profile = SessionInvitation::issue_online(
        &host_key,
        generate_key().public(),
        "online-voice-test",
        &services[..1],
        deadlines,
    )
    .unwrap();
    assert!(host.admit_online(&wrong_profile).is_err());
    let mut streams = Vec::new();
    let mut bridges = Vec::new();
    for (index, peer) in [&player, &guest].into_iter().enumerate() {
        let (mut send, mut recv) = bounded(peer.open_tcp("game", 1)).await.unwrap();
        chunk(&mut send, &mut recv, index as u8 + 3).await;
        let bridge = bounded(peer.udp_bridge_fragmented("aux", 1, "127.0.0.1:0".parse().unwrap()))
            .await
            .unwrap();
        assert_eq!(bridge.max_payload(), Some(4096));
        for source in 0..2 {
            let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let payload = vec![index as u8 * 17 + source + 1; 4096];
            socket.send_to(&payload, bridge.local_addr()).await.unwrap();
            let mut returned = vec![0; 4097];
            let (count, sender) = bounded(socket.recv_from(&mut returned)).await.unwrap();
            assert_eq!(sender, bridge.local_addr());
            assert_eq!(&returned[..count], payload);
            chunk(&mut send, &mut recv, index as u8 + source + 19).await;
        }
        streams.push((send, recv));
        bridges.push(bridge);
    }
    let (first, second) = tokio::join!(
        host.disable_udp_service(local.invitation_id(), "aux", 1),
        host.disable_udp_service(remote.invitation_id(), "aux", 1)
    );
    first.unwrap();
    second.unwrap();
    for ((send, recv), peer) in streams.iter_mut().zip([&player, &guest]) {
        chunk(send, recv, 41).await;
        assert!(matches!(
            bounded(peer.open_udp_fragmented("aux", 1)).await,
            Err(Error::ServiceDenied)
        ));
        let (mut new_send, mut new_recv) = bounded(peer.open_tcp("game", 1)).await.unwrap();
        chunk(&mut new_send, &mut new_recv, 47).await;
        new_send.finish().unwrap();
    }
    for bridge in &bridges {
        bridge.shutdown().await;
    }
    let addresses: Vec<SocketAddr> = bridges.iter().map(UdpBridge::local_addr).collect();
    drop(bridges);
    drop(streams);
    player.shutdown().await;
    guest.shutdown().await;
    host.shutdown().await;
    drop(player);
    drop(guest);
    drop(host);
    for address in addresses {
        drop(std::net::UdpSocket::bind(address).expect("own_bridge_rebind"));
    }
    cancel.cancel();
    bounded(echo).await.unwrap();
    relay.shutdown().await.unwrap();
    drop(std::net::TcpListener::bind(tcp_target).expect("own_tcp_rebind"));
    drop(std::net::UdpSocket::bind(udp_target).expect("own_udp_rebind"));
}
