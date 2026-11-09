use super::*;
use std::time::Instant;
use tokio::io::AsyncReadExt;

const PEERS: usize = 8;
const ROUNDS: usize = 60;
const FRAME: usize = 1400;
const PERIOD: Duration = Duration::from_millis(20);
const TCP_BYTES: usize = 2 * 1024 * 1024;

fn game(endpoint: SocketAddr) -> Service {
    Service {
        service_id: "game".into(),
        protocol: Protocol::Tcp,
        endpoint,
        revision: 1,
        max_datagram_bytes: 1400,
        required_capability: Capability::ReliableStream,
        udp_return_policy: UdpReturnPolicy::ReplyBudget,
    }
}

fn deadlines(expiry: u64) -> SessionDeadlines {
    SessionDeadlines {
        admission_expires_at_unix_ms: expiry - 10_000,
        session_expires_at_unix_ms: expiry,
    }
}

fn sample(peer: usize, round: usize) -> [u8; FRAME] {
    let mut data = [peer as u8 ^ round as u8; FRAME];
    data[0] = peer as u8;
    data[1] = round as u8;
    data
}

#[test]
fn native_voice_budget_is_signed_and_separate_from_legacy_online_voice() {
    let host = generate_key();
    let guest = generate_key().public();
    let mut services = [
        game("127.0.0.1:1".parse().unwrap()),
        NativeVoiceBudget::service("127.0.0.1:2".parse().unwrap()),
    ];
    let session = "native-voice-budget-test";
    let caps = deadlines(unix_millis() + 25_000);
    let native =
        SessionInvitation::issue_online_native_voice(&host, guest, session, &services, caps)
            .unwrap();
    assert_eq!(native.preview()["schemaVersion"], 6);
    assert!(
        SessionInvitation::verify_online_native_voice(&native.encode(), host.public(), guest)
            .unwrap()
            .is_online_native_voice()
    );
    assert!(
        SessionInvitation::verify_online_fragmented(&native.encode(), host.public(), guest)
            .is_err()
    );
    assert!(
        SessionInvitation::issue_online_fragmented(&host, guest, session, &services, caps).is_err()
    );
    services[1].udp_return_policy = UdpReturnPolicy::AuthorizedDownstream {
        bytes_per_second: 16_384,
        datagrams_per_second: 20,
    };
    assert!(
        SessionInvitation::issue_online_native_voice(&host, guest, session, &services, caps)
            .is_err()
    );
    let legacy =
        SessionInvitation::issue_online_fragmented(&host, guest, session, &services, caps).unwrap();
    assert_eq!(legacy.preview()["schemaVersion"], 5);
    assert!(
        SessionInvitation::verify_online_native_voice(&legacy.encode(), host.public(), guest)
            .is_err()
    );
    assert!(NativeVoiceBudget::check_limits(&Limits::default()).is_err());
    assert!(NativeVoiceBudget::check_limits(&NativeVoiceBudget::limits()).is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_voice_budget_carries_eight_real_peers_with_tcp_pressure() {
    let host_key = generate_key();
    let guests = (0..PEERS).map(|_| generate_key()).collect::<Vec<_>>();
    let authority = generate_key();
    // Guest 0 is the Host player and does not use the relay.
    let peers = std::array::from_fn(|index| {
        if index == 0 {
            host_key.public()
        } else {
            guests[index].public()
        }
    });
    let relay = LocalRelayFixture::start_native_voice_test(peers)
        .await
        .unwrap();
    let origin = relay
        .ticket()
        .url()
        .as_str()
        .trim_end_matches('/')
        .to_owned();
    let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let services = vec![
        game(tcp.local_addr().unwrap()),
        NativeVoiceBudget::service(udp.local_addr().unwrap()),
    ];
    let expiry = unix_millis() + 45_000;
    let host_ticket = ticket(
        &authority,
        host_key.public(),
        host_key.public(),
        &origin,
        expiry,
    );
    let invitations = guests
        .iter()
        .map(|key| {
            SessionInvitation::issue_online_native_voice(
                &host_key,
                key.public(),
                "online-voice-test",
                &services,
                deadlines(expiry),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let host = bounded(Host::start_invited_online_native_voice(
        HostConfig {
            secret_key: host_key.clone(),
            bind_addr: "0.0.0.0:0".parse().unwrap(),
            session_id: "online-voice-test".into(),
            services,
            leases: vec![invitations[0].lease()],
            limits: NativeVoiceBudget::limits(),
        },
        &invitations[0],
        &host_ticket,
    ))
    .await
    .unwrap();
    for invitation in &invitations[1..] {
        host.admit_online(invitation).unwrap();
    }
    let local_ticket = host
        .local_player_ticket(&invitations[0], &guests[0])
        .unwrap();
    let mut admitted = vec![
        bounded(Guest::connect_local_authorized(
            guests[0].clone(),
            &invitations[0],
            NativeVoiceBudget::limits(),
            &local_ticket,
        ))
        .await
        .unwrap(),
    ];
    for (key, invitation) in guests.into_iter().zip(&invitations).skip(1) {
        let remote = ticket(&authority, key.public(), host_key.public(), &origin, expiry);
        admitted.push(
            bounded(Guest::connect_invited_online_native_voice(
                key,
                invitation,
                NativeVoiceBudget::limits(),
                &remote,
            ))
            .await
            .unwrap(),
        );
    }
    assert!(
        invitations
            .iter()
            .all(|invitation| host.invitation_consumed(invitation.invitation_id()))
    );
    let mut associations = Vec::new();
    let mut routes = Vec::new();
    let mut input = [0_u8; 4097];
    for (index, guest) in admitted.iter().enumerate() {
        let association = bounded(guest.open_udp_fragmented("aux", 1)).await.unwrap();
        assert_eq!(association.max_payload(), Some(4096));
        association.send(&[index as u8]).await.unwrap();
        let (length, route) = bounded(udp.recv_from(&mut input)).await.unwrap();
        assert_eq!(&input[..length], &[index as u8]);
        assert!(!routes.contains(&route));
        routes.push(route);
        associations.push(association);
    }
    let maximum = [0xA7; 4096];
    associations[0].send(&maximum).await.unwrap();
    let (length, sender) = bounded(udp.recv_from(&mut input)).await.unwrap();
    assert_eq!(sender, routes[0]);
    assert_eq!(&input[..length], &maximum);
    udp.send_to(&input[..length], routes[PEERS - 1])
        .await
        .unwrap();
    assert_eq!(
        bounded(associations[PEERS - 1].recv())
            .await
            .unwrap()
            .as_ref(),
        &maximum
    );

    exercise_fanout(&host, &admitted, tcp, udp, &associations, routes).await;
    for (guest, association) in admitted.iter().zip(associations) {
        association.shutdown().await;
        guest.shutdown().await;
    }
    host.shutdown().await;
    drop((host, admitted));
    relay.shutdown().await.unwrap();
}

async fn exercise_fanout(
    host: &Host,
    players: &[Guest],
    tcp: TcpListener,
    udp: UdpSocket,
    associations: &[FragmentedUdpAssociation],
    routes: Vec<SocketAddr>,
) {
    let (mut sending, _receiving) = bounded(players[0].open_tcp("game", 1)).await.unwrap();
    let tcp_sink = tokio::spawn(async move {
        let (socket, _) = tcp.accept().await.unwrap();
        let mut received = Vec::new();
        socket
            .take((TCP_BYTES + 1) as u64)
            .read_to_end(&mut received)
            .await
            .unwrap();
        assert_eq!(received.len(), TCP_BYTES);
        assert!(received.iter().all(|byte| *byte == 0xB2));
    });
    let start = Instant::now() + Duration::from_millis(100);
    let end = start + PERIOD * ROUNDS as u32;
    let drain = end + Duration::from_millis(350);
    let tcp_writer = tokio::spawn(async move {
        tokio::time::sleep_until(start.into()).await;
        for _ in 0..TCP_BYTES / 16384 {
            sending.write_all(&[0xB2; 16384]).await.unwrap();
        }
        sending.finish().unwrap();
    });
    let fanout = tokio::spawn(async move {
        let mut input = [0_u8; 4097];
        let mut seen = [[false; ROUNDS]; PEERS];
        while let Ok(result) =
            tokio::time::timeout_at(drain.into(), udp.recv_from(&mut input)).await
        {
            let (length, route) = result.unwrap();
            let sender = input[0] as usize;
            let round = input[1] as usize;
            assert_eq!(route, routes[sender]);
            assert_eq!(&input[..length], &sample(sender, round));
            assert!(!std::mem::replace(&mut seen[sender][round], true));
            for (index, destination) in routes.iter().enumerate() {
                if index != sender {
                    udp.send_to(&input[..length], destination).await.unwrap();
                }
            }
        }
        seen
    });
    let mut producers = JoinSet::new();
    let mut consumers = JoinSet::new();
    for (peer, association) in associations.iter().enumerate() {
        let receiving = association.clone();
        consumers.spawn(async move {
            let mut seen = [[false; ROUNDS]; PEERS];
            while let Ok(result) = tokio::time::timeout_at(drain.into(), receiving.recv()).await {
                let datagram = result.unwrap();
                let bytes = datagram.as_ref();
                let sender = bytes[0] as usize;
                let round = bytes[1] as usize;
                assert_ne!(sender, peer);
                assert_eq!(bytes, &sample(sender, round));
                assert!(!std::mem::replace(&mut seen[sender][round], true));
            }
            (peer, seen.iter().flatten().filter(|value| **value).count())
        });
        let sending = association.clone();
        producers.spawn(async move {
            let mut pace = tokio::time::interval_at(start.into(), PERIOD);
            pace.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            for round in 0..ROUNDS {
                pace.tick().await;
                sending.send(&sample(peer, round)).await.unwrap();
            }
        });
    }
    let tcp_before = host.metrics().tcp_bytes;
    tokio::time::sleep_until(end.into()).await;
    let concurrent_tcp = host.metrics().tcp_bytes - tcp_before;
    while let Some(result) = bounded(producers.join_next()).await {
        result.unwrap();
    }
    let ingress = bounded(fanout)
        .await
        .unwrap()
        .map(|rounds| rounds.iter().filter(|seen| **seen).count());
    let mut downstream = [0; PEERS];
    while let Some(result) = bounded(consumers.join_next()).await {
        let (peer, delivered) = result.unwrap();
        downstream[peer] = delivered;
    }
    bounded(tcp_writer).await.unwrap();
    bounded(tcp_sink).await.unwrap();
    let host = host.metrics();
    let guest_drops: Vec<_> = players
        .iter()
        .map(|player| player.metrics().udp_dropped)
        .collect();
    let capacities: Vec<_> = players
        .iter()
        .map(|player| player.connection().max_datagram_size())
        .collect();
    assert!(
        concurrent_tcp > 0
            && ingress.iter().all(|&count| count >= ROUNDS * 9 / 10)
            && downstream
                .iter()
                .all(|&count| count >= (PEERS - 1) * ROUNDS * 9 / 10),
        "voice fan-out with TCP pressure: ingress {ingress:?}, downstream {downstream:?}, TCP bytes {concurrent_tcp}, host returned {}, host drops {}, guest drops {guest_drops:?}, capacities {capacities:?}",
        host.udp_returned,
        host.udp_dropped,
    );
}
