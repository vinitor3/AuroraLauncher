use super::*;
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{sleep, timeout},
};

fn host_request(bind: SocketAddr, target: SocketAddr, ttl: u16) -> HostRequest {
    HostRequest {
        mode: ConnectMode::Loopback,
        bind_address: bind.to_string(),
        game_target: target.to_string(),
        lifetime_seconds: ttl,
    }
}
async fn wait(manager: &ConnectManager, id: &str, phase: &str) -> ConnectSnapshot {
    timeout(Duration::from_secs(10), async {
        loop {
            let snapshot = manager.snapshot("main").unwrap();
            assert!(snapshot.operation_id.as_deref() == Some(id));
            if snapshot.phase == phase {
                return snapshot;
            }
            assert!(snapshot.phase != "failed", "native operation succeeded");
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("bounded manager transition")
}
#[tokio::test]
async fn policy_owner_single_slot_and_review_restore_are_enforced_without_network() {
    let manager = ConnectManager::default();
    let request = host_request(
        "127.0.0.1:42000".parse().unwrap(),
        "127.0.0.1:43001".parse().unwrap(),
        30,
    );
    let prepared = manager.prepare_host("main", request.clone()).unwrap();
    assert!(matches!(
        manager.prepare_host("main", request),
        Err(ConnectError::Busy)
    ));
    assert!(matches!(
        manager.snapshot("other"),
        Err(ConnectError::OwnerMismatch)
    ));
    assert!(matches!(
        manager.reserve_host_review("main", "stale", "bad"),
        Err(ConnectError::InvalidRequest)
    ));
    let recipient = aurora_connect_lab::generate_key().public().to_string();
    let review = manager
        .reserve_host_review("main", &prepared.operation_id, &recipient)
        .unwrap()
        .wait()
        .await
        .unwrap();
    let snapshot = manager.snapshot("main").unwrap();
    assert!(
        snapshot.review.as_ref().unwrap().digest == review.digest
            && snapshot.preparation.unwrap().public_id == prepared.public_id
    );
    assert!(matches!(
        manager.export_invitation("main", &prepared.operation_id),
        Err(ConnectError::PlanExpired)
    ));
    let fresh = ConnectManager::default();
    for (bind, target, ttl) in [
        ("0.0.0.0:42000", "127.0.0.1:43001", 30),
        ("127.0.0.1:42000", "0.0.0.0:43001", 30),
        ("127.0.0.1:0", "127.0.0.1:43001", 30),
        ("127.0.0.1:42000", "127.0.0.1:43001", 601),
    ] {
        assert!(matches!(
            fresh.prepare_host(
                "main",
                HostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: bind.into(),
                    game_target: target.into(),
                    lifetime_seconds: ttl
                }
            ),
            Err(ConnectError::InvalidRequest)
        ));
    }
}
pub(super) async fn status_fixture() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let mut peers = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted=listener.accept()=>{let Ok((mut socket,_))=accepted else{break;};peers.spawn(async move {
                    let first=socket.read_u8().await?;
                    if first==0x45 {socket.write_all(&[first]).await?;let(mut read,mut write)=socket.split();tokio::io::copy(&mut read,&mut write).await?;write.shutdown().await?;}
                    else {
                        if first>127{return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));}
                        let mut handshake=vec![0;first as usize];socket.read_exact(&mut handshake).await?;
                        let mut status=[0;2];socket.read_exact(&mut status).await?;
                        if status!=[1,0]{return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));}
                        let json=b"{\"version\":{\"name\":\"fixture\",\"protocol\":767},\"players\":{\"max\":2,\"online\":0},\"description\":{\"text\":\"fixture\"}}";
                        fn varint(mut value:usize)->Vec<u8>{let mut bytes=Vec::new();loop {let mut byte=(value&127)as u8;value>>=7;if value>0{byte|=128;}bytes.push(byte);if value==0{break;}}bytes}
                        let mut response=vec![0];response.extend(varint(json.len()));response.extend(json);let mut frame=varint(response.len());frame.extend(response);socket.write_all(&frame).await?;
                        let mut ping=[0;10];socket.read_exact(&mut ping).await?;socket.write_all(&ping).await?;
                    }
                    std::io::Result::Ok(())
                });},
                _=peers.join_next(),if !peers.is_empty()=>{}
            }
        }
    });
    (address, task)
}
async fn pair(
    ttl: u16,
    target: SocketAddr,
) -> (ConnectManager, ConnectManager, String, String, SocketAddr) {
    let reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = reservation.local_addr().unwrap();
    let host = ConnectManager::default();
    let guest = ConnectManager::default();
    let host_prepared = host
        .prepare_host("main", host_request(bind, target, ttl))
        .unwrap();
    let guest_prepared = guest
        .prepare_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: host_prepared.public_id,
            },
        )
        .unwrap();
    let review = host
        .reserve_host_review(
            "main",
            &host_prepared.operation_id,
            &guest_prepared.public_id,
        )
        .unwrap()
        .wait()
        .await
        .unwrap();
    assert!(matches!(
        host.confirm("main", &host_prepared.operation_id, "wrong")
            .await,
        Err(ConnectError::WrongDigest)
    ));
    drop(reservation);
    host.confirm("main", &host_prepared.operation_id, &review.digest)
        .await
        .unwrap();
    wait(&host, &host_prepared.operation_id, "hostListening").await;
    let export = host
        .export_invitation("main", &host_prepared.operation_id)
        .unwrap();
    let review = guest
        .review_guest("main", &guest_prepared.operation_id, &export.invitation)
        .unwrap();
    guest
        .confirm("main", &guest_prepared.operation_id, &review.digest)
        .await
        .unwrap();
    wait(&guest, &guest_prepared.operation_id, "guestConnected").await;
    (
        host,
        guest,
        host_prepared.operation_id,
        guest_prepared.operation_id,
        bind,
    )
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_managers_real_tcp_status_and_concurrent_stop_release_own_sockets() {
    let (target, fixture) = status_fixture().await;
    let (host, guest, host_id, guest_id, bind) = pair(30, target).await;
    let address: SocketAddr = guest
        .snapshot("main")
        .unwrap()
        .tcp_address
        .unwrap()
        .parse()
        .unwrap();
    let bytes = vec![0x45; 1024 * 1024];
    let io = timeout(Duration::from_secs(5), async {
        let socket = TcpStream::connect(address).await.unwrap();
        let (mut read, mut write) = socket.into_split();
        let mut reply = vec![0; bytes.len()];
        tokio::try_join!(
            async {
                write.write_all(&bytes).await?;
                write.shutdown().await
            },
            async {
                read.read_exact(&mut reply).await?;
                std::io::Result::Ok(())
            }
        )
        .unwrap();
        assert!(reply == bytes, "TCP1MiB each direction exact");
    })
    .await;
    assert!(io.is_ok());
    let summary = guest.probe_status("main", &guest_id).await.unwrap();
    assert!(
        summary.status_state == "responded"
            && summary.status_summary.unwrap().reported_protocol == 767
            && summary.world_state == "notVerified"
    );
    let (left, right) = tokio::join!(guest.stop("main", &guest_id), guest.stop("main", &guest_id));
    assert!(
        left.unwrap().cleanup_state == "complete" && right.unwrap().cleanup_state == "complete"
    );
    host.stop("main", &host_id).await.unwrap();
    fixture.abort();
    let _ = fixture.await;
    assert!(std::net::TcpListener::bind(address).is_ok());
    assert!(std::net::UdpSocket::bind(bind).is_ok());
    let next = guest
        .prepare_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: aurora_connect_lab::generate_key().public().to_string(),
            },
        )
        .unwrap();
    assert!(matches!(
        guest.stop("main", &guest_id).await,
        Err(ConnectError::StaleOperation)
    ));
    guest.stop("main", &next.operation_id).await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expiry_closes_session_and_shutdown_blocks_new_work() {
    let (target, fixture) = status_fixture().await;
    let (host, guest, host_id, guest_id, bind) = pair(2, target).await;
    wait(&guest, &guest_id, "closed").await;
    wait(&host, &host_id, "closed").await;
    assert!(host.export_invitation("main", &host_id).is_err());
    guest.shutdown_all().await;
    host.shutdown_all().await;
    assert!(matches!(
        guest.prepare_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: aurora_connect_lab::generate_key().public().to_string()
            }
        ),
        Err(ConnectError::Cancelled)
    ));
    fixture.abort();
    let _ = fixture.await;
    assert!(std::net::UdpSocket::bind(bind).is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fast_disconnect_cannot_export_consumed_invitation() {
    let reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = reservation.local_addr().unwrap();
    let target: SocketAddr = "127.0.0.1:43001".parse().unwrap();
    let host = ConnectManager::default();
    let prepared = host
        .prepare_host("main", host_request(bind, target, 30))
        .unwrap();
    let key = aurora_connect_lab::generate_key();
    let review = host
        .reserve_host_review("main", &prepared.operation_id, &key.public().to_string())
        .unwrap()
        .wait()
        .await
        .unwrap();
    drop(reservation);
    host.confirm("main", &prepared.operation_id, &review.digest)
        .await
        .unwrap();
    wait(&host, &prepared.operation_id, "hostListening").await;
    let export = host
        .export_invitation("main", &prepared.operation_id)
        .unwrap();
    let invitation = aurora_connect_lab::Invitation::verify(
        &export.invitation,
        prepared.public_id.parse().unwrap(),
        key.public(),
    )
    .unwrap();
    let guest = aurora_connect_lab::Guest::connect_invited(
        key,
        &invitation,
        aurora_connect_lab::Limits::default(),
    )
    .await
    .unwrap();
    assert!(matches!(
        host.export_invitation("main", &prepared.operation_id),
        Err(ConnectError::StaleOperation)
    ));
    guest.shutdown().await;
    drop(guest);
    wait(&host, &prepared.operation_id, "closed").await;
    assert!(host
        .export_invitation("main", &prepared.operation_id)
        .is_err());
    assert!(std::net::UdpSocket::bind(bind).is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn held_status_is_cancelled_and_released_before_stop_or_expiry_complete() {
    for expires in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = listener.local_addr().unwrap();
        let seen = std::sync::Arc::new(tokio::sync::Notify::new());
        let signal = seen.clone();
        let fixture = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 128];
            let _ = socket.read(&mut bytes).await;
            signal.notify_one();
            loop {
                if socket.read(&mut bytes).await.unwrap_or(0) == 0 {
                    break;
                }
            }
        });
        let (host, guest, host_id, guest_id, bind) =
            pair(if expires { 2 } else { 30 }, target).await;
        let address: SocketAddr = guest
            .snapshot("main")
            .unwrap()
            .tcp_address
            .unwrap()
            .parse()
            .unwrap();
        let probe = guest.probe_status("main", &guest_id);
        tokio::pin!(probe);
        tokio::select! {result=&mut probe=>panic!("STATUS must wait for controlled response, got {}",result.is_ok()),_=seen.notified()=>{}}
        if expires {
            let (result, _) = tokio::join!(&mut probe, wait(&guest, &guest_id, "closed"));
            assert!(
                result.unwrap().cleanup_state == "complete"
                    || guest.snapshot("main").unwrap().cleanup_state == "complete"
            );
        } else {
            let (result, stopped) = tokio::join!(&mut probe, guest.stop("main", &guest_id));
            assert!(result.is_ok() && stopped.unwrap().cleanup_state == "complete");
        }
        host.stop("main", &host_id).await.unwrap();
        timeout(Duration::from_secs(3), fixture)
            .await
            .unwrap()
            .unwrap();
        assert!(std::net::TcpListener::bind(address).is_ok());
        assert!(std::net::UdpSocket::bind(bind).is_ok());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_during_dial_awaits_native_cleanup_and_never_publishes_listener() {
    let blackhole = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = blackhole.local_addr().unwrap();
    let host = aurora_connect_lab::generate_key();
    let guest = ConnectManager::default();
    let prepared = guest
        .prepare_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: host.public().to_string(),
            },
        )
        .unwrap();
    let service = aurora_connect_lab::Service {
        service_id: "game".into(),
        protocol: aurora_connect_lab::Protocol::Tcp,
        endpoint: "127.0.0.1:43001".parse().unwrap(),
        revision: 1,
        required_capability: aurora_connect_lab::Capability::ReliableStream,
        max_datagram_bytes: 1400,
        udp_return_policy: aurora_connect_lab::UdpReturnPolicy::ReplyBudget,
    };
    let invitation = aurora_connect_lab::Invitation::issue(
        &host,
        prepared.public_id.parse().unwrap(),
        "owned-blackhole",
        bind,
        &[service],
        aurora_connect_lab::unix_millis() + 30_000,
    )
    .unwrap();
    let review = guest
        .review_guest("main", &prepared.operation_id, &invitation.encode())
        .unwrap();
    guest
        .confirm("main", &prepared.operation_id, &review.digest)
        .await
        .unwrap();
    sleep(Duration::from_millis(30)).await;
    timeout(Duration::from_secs(20), guest.shutdown_all())
        .await
        .unwrap();
    let snapshot = guest.snapshot("main").unwrap();
    assert!(
        snapshot.phase == "closed"
            && snapshot.cleanup_state == "complete"
            && snapshot.tcp_address.is_none()
    );
}
