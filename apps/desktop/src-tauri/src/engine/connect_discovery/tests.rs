use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

const MARKER: &str = "AURORA_OWNED_DISCOVERY_FIXTURE_CHILD";
#[test]
#[ignore = "internal child fixture, invoked by the parent test only"]
fn owned_listener_fixture() {
    assert!(
        std::env::var_os(MARKER).is_some(),
        "owned fixture marker required"
    );
    let listeners = [
        TcpListener::bind("127.0.0.1:0").unwrap(),
        TcpListener::bind("0.0.0.0:0").unwrap(),
        TcpListener::bind("[::1]:0").unwrap(),
    ];
    let ports: Vec<_> = listeners
        .iter()
        .map(|listener| listener.local_addr().unwrap().port())
        .collect();
    let stop = Arc::new(AtomicBool::new(false));
    let mut threads = Vec::new();
    for listener in listeners {
        listener.set_nonblocking(true).unwrap();
        let stop = stop.clone();
        threads.push(thread::spawn(move||{while !stop.load(Ordering::Acquire){match listener.accept(){Ok((mut socket,_))=>{
            let _=socket.set_nonblocking(false);let _=socket.set_read_timeout(Some(Duration::from_secs(5)));let _=socket.set_write_timeout(Some(Duration::from_secs(5)));
            let mut result=||->std::io::Result<()> {
                fn length(socket:&mut std::net::TcpStream)->std::io::Result<usize>{let mut value=0;for shift in [0,7,14,21,28]{let mut byte=[0];socket.read_exact(&mut byte)?;value|=usize::from(byte[0]&127)<<shift;if byte[0]&128==0{return Ok(value);}}Err(std::io::ErrorKind::InvalidData.into())}
                fn varint(mut value:usize)->Vec<u8>{let mut bytes=Vec::new();loop{let mut byte=(value&127)as u8;value>>=7;if value!=0{byte|=128;}bytes.push(byte);if value==0{return bytes;}}}
                let size=length(&mut socket)?;if size>1024{return Err(std::io::ErrorKind::InvalidData.into());}let mut handshake=vec![0;size];socket.read_exact(&mut handshake)?;let mut request=[0;2];socket.read_exact(&mut request)?;if request!=[1,0]{return Err(std::io::ErrorKind::InvalidData.into());}
                let json=b"{\"version\":{\"protocol\":767,\"name\":\"fixture\"},\"players\":{\"max\":2,\"online\":0},\"description\":{\"text\":\"fixture\"}}";
                let mut payload=vec![0];payload.extend(varint(json.len()));payload.extend(json);let mut frame=varint(payload.len());frame.extend(payload);socket.write_all(&frame)?;let mut ping=[0;10];socket.read_exact(&mut ping)?;socket.write_all(&ping)?;Ok(())
            };let _=result();},Err(error)if error.kind()==std::io::ErrorKind::WouldBlock=>thread::sleep(Duration::from_millis(5)),Err(_)=>break,}}}));
    }
    println!("DISCOVERY_READY {}", serde_json::to_string(&ports).unwrap());
    std::io::stdout().flush().unwrap();
    let mut command = String::new();
    let _ = std::io::stdin().read_line(&mut command);
    stop.store(true, Ordering::Release);
    for thread in threads {
        let _ = thread.join();
    }
}
pub(crate) struct Fixture {
    child: Child,
    ports: Vec<u16>,
    reader: Option<thread::JoinHandle<()>>,
}
/// Private test clock seam: does not alter a production constructor or OS time.
pub(crate) fn expired_target(mut target: ApprovedTarget) -> ApprovedTarget {
    target.deadline = Instant::now() - Duration::from_secs(1);
    target
}
impl Fixture {
    pub(crate) fn new() -> Self {
        let mut command = Command::new(std::env::current_exe().unwrap());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .args([
                "--ignored",
                "--exact",
                "engine::connect_discovery::tests::owned_listener_fixture",
                "--nocapture",
            ])
            .env(MARKER, "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let (send, receive) = mpsc::sync_channel(1);
        let read_task = thread::spawn(move || {
            for _ in 0..8 {
                let mut line = String::new();
                if reader.by_ref().take(4097).read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if let Some(json) = line.strip_prefix("DISCOVERY_READY ") {
                    if let Ok(ports) = serde_json::from_str::<Vec<u16>>(json.trim()) {
                        let _ = send.try_send(ports);
                    }
                    break;
                }
            }
            let _ = std::io::copy(&mut reader.take(8193), &mut std::io::sink());
        });
        let mut fixture = Self {
            child,
            ports: Vec::new(),
            reader: Some(read_task),
        };
        fixture.ports = receive
            .recv_timeout(Duration::from_secs(5))
            .expect("bounded owned fixture readiness");
        fixture
    }
    pub(crate) fn proof(&mut self) -> OwnedChildProof {
        OwnedChildProof::from_child(&mut self.child, "fixture-instance", "fixture-run").unwrap()
    }
    pub(crate) fn stop(&mut self) {
        if let Some(mut input) = self.child.stdin.take() {
            let _ = input.write_all(b"stop\n");
        }
        let deadline = Instant::now() + Duration::from_secs(7);
        while self.child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "owned fixture stop deadline");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
#[test]
fn local_policy_never_turns_a_nonlocal_listener_into_a_target() {
    for bind in ["127.0.0.1:1234", "[::1]:1234", "0.0.0.0:1234", "[::]:1234"] {
        assert!(local_target(bind.parse().unwrap()).is_some());
    }
    for bind in [
        "127.0.0.1:0",
        "[fe80::1%1]:1234",
        "192.0.2.1:1234",
        "[2001:db8::1]:1234",
    ] {
        assert!(local_target(bind.parse().unwrap()).is_none());
    }
}
#[test]
fn owned_child_inventory_filters_all_other_processes_and_rejects_exit() {
    let mut fixture = Fixture::new();
    let proof = fixture.proof();
    let rows = windows::listeners(&proof).unwrap();
    assert!(
        fixture
            .ports
            .iter()
            .all(|port| rows.iter().any(|row| row.port() == *port)),
        "all owned IPv4/wildcard/IPv6 sockets discovered"
    );
    let manager = DiscoveryManager::default();
    let inventory = manager.inventory("main", fixture.proof()).unwrap();
    assert!(inventory.candidates.len() == 3);
    assert!(matches!(
        manager.review(
            "other",
            &inventory.inventory_id,
            &inventory.candidates[0].candidate_id
        ),
        Err(DiscoveryError::Owner)
    ));
    fixture.stop();
    assert!(matches!(
        proof.recheck(),
        Err(DiscoveryError::ProcessChanged)
    ));
    assert!(matches!(
        manager.review(
            "main",
            &inventory.inventory_id,
            &inventory.candidates[0].candidate_id
        ),
        Err(DiscoveryError::ProcessChanged)
    ));
}
#[tokio::test]
async fn status_requires_review_and_exact_digest_then_returns_single_use_owned_target() {
    let mut fixture = Fixture::new();
    let manager = DiscoveryManager::default();
    let inventory = manager.inventory("main", fixture.proof()).unwrap();
    assert!(matches!(
        manager.confirm_status("main", "missing", "wrong").await,
        Err(DiscoveryError::Stale)
    ));
    let candidate = inventory
        .candidates
        .iter()
        .find(|candidate| {
            candidate.target.starts_with("127.0.0.1:") && candidate.bind_kind == "loopback"
        })
        .unwrap();
    let review = manager
        .review("main", &inventory.inventory_id, &candidate.candidate_id)
        .unwrap();
    assert!(matches!(
        manager
            .confirm_status("main", &review.review_id, "wrong")
            .await,
        Err(DiscoveryError::Digest)
    ));
    let review = manager
        .review("main", &inventory.inventory_id, &candidate.candidate_id)
        .unwrap();
    let status = manager
        .confirm_status("main", &review.review_id, &review.digest)
        .await
        .unwrap();
    assert!(
        status.verified_before_after
            && status.summary.reported_protocol == 767
            && status.world_state == "notVerified"
    );
    let target = manager.take_target("main", &status.selection_id).unwrap();
    assert!(target.recheck().unwrap().to_string() == candidate.target);
    assert!(matches!(
        manager.take_target("main", &status.selection_id),
        Err(DiscoveryError::Stale)
    ));
    fixture.stop();
    assert!(matches!(
        target.recheck(),
        Err(DiscoveryError::ProcessChanged)
    ));
}

pub(crate) async fn approved(manager: &DiscoveryManager, fixture: &mut Fixture) -> ApprovedTarget {
    let inventory = manager.inventory("main", fixture.proof()).unwrap();
    let candidate = inventory
        .candidates
        .iter()
        .find(|candidate| {
            candidate.bind_kind == "loopback" && candidate.target.starts_with("127.0.0.1:")
        })
        .unwrap();
    let review = manager
        .review("main", &inventory.inventory_id, &candidate.candidate_id)
        .unwrap();
    let status = manager
        .confirm_status("main", &review.review_id, &review.digest)
        .await
        .unwrap();
    manager.take_target("main", &status.selection_id).unwrap()
}
async fn actor_phase(
    manager: &super::super::connect::ConnectManager,
    phase: &str,
) -> super::super::connect::ConnectSnapshot {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let snapshot = manager.snapshot("main").unwrap();
            if snapshot.phase == phase {
                return snapshot;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actor_owned_target_activation_match_digest_and_lost_source_are_enforced() {
    use super::super::connect::{ConnectError, ConnectManager, ConnectMode, HostRequest};
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let actor = ConnectManager::default();
    let reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = reservation.local_addr().unwrap();
    let target = approved(&discovery, &mut fixture).await;
    let request = HostRequest {
        mode: ConnectMode::Loopback,
        bind_address: bind.to_string(),
        game_target: target.recheck().unwrap().to_string(),
        lifetime_seconds: 60,
    };
    let mut mismatch = request.clone();
    mismatch.game_target = "127.0.0.1:1".into();
    assert!(matches!(
        actor.prepare_host_discovered("main", mismatch, target),
        Err(ConnectError::InvalidRequest)
    ));
    let mut expired = approved(&discovery, &mut fixture).await;
    expired.deadline = Instant::now() - Duration::from_secs(1);
    assert!(matches!(
        actor.prepare_host_discovered("main", request.clone(), expired),
        Err(ConnectError::PlanExpired)
    ));
    let mut target = approved(&discovery, &mut fixture).await;
    target.deadline = Instant::now() + Duration::from_millis(20);
    let prepared = actor
        .prepare_host_discovered("main", request.clone(), target)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    let review = actor
        .review_host(
            "main",
            &prepared.operation_id,
            &aurora_connect_lab::generate_key().public().to_string(),
        )
        .unwrap();
    assert!(
        review.target_source == Some("ownedInstance")
            && review.instance_id.as_deref() == Some("fixture-instance")
            && review.run_id.as_deref() == Some("fixture-run")
    );
    assert!(matches!(
        actor.confirm("main", &prepared.operation_id, "wrong").await,
        Err(ConnectError::WrongDigest)
    ));
    fixture.stop();
    actor
        .confirm("main", &prepared.operation_id, &review.digest)
        .await
        .unwrap();
    let failed = actor_phase(&actor, "failed").await;
    assert!(
        failed.error_code == Some("connect_source_changed") && failed.cleanup_state == "complete"
    );
    drop(reservation);
    let mut fixture = Fixture::new();
    let target = approved(&discovery, &mut fixture).await;
    let reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = reservation.local_addr().unwrap();
    let mut request = request;
    request.bind_address = bind.to_string();
    request.game_target = target.recheck().unwrap().to_string();
    let prepared = actor
        .prepare_host_discovered("main", request, target)
        .unwrap();
    let review = actor
        .review_host(
            "main",
            &prepared.operation_id,
            &aurora_connect_lab::generate_key().public().to_string(),
        )
        .unwrap();
    drop(reservation);
    actor
        .confirm("main", &prepared.operation_id, &review.digest)
        .await
        .unwrap();
    actor_phase(&actor, "hostListening").await;
    fixture.stop();
    let failed = actor_phase(&actor, "failed").await;
    assert!(
        failed.error_code == Some("connect_source_changed") && failed.cleanup_state == "complete"
    );
    assert!(std::net::UdpSocket::bind(bind).is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn discovered_owned_server_two_managers_status_and_source_loss_cleanup() {
    use super::super::connect::{ConnectManager, ConnectMode, GuestRequest, HostRequest};
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let target = approved(&discovery, &mut fixture).await;
    let reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = reservation.local_addr().unwrap();
    let host = ConnectManager::default();
    let guest = ConnectManager::default();
    let prepared_host = host
        .prepare_host_discovered(
            "main",
            HostRequest {
                mode: ConnectMode::Loopback,
                bind_address: bind.to_string(),
                game_target: target.recheck().unwrap().to_string(),
                lifetime_seconds: 30,
            },
            target,
        )
        .unwrap();
    let prepared_guest = guest
        .prepare_guest(
            "main",
            GuestRequest {
                mode: ConnectMode::Loopback,
                expected_host_public_id: prepared_host.public_id,
            },
        )
        .unwrap();
    let host_review = host
        .review_host(
            "main",
            &prepared_host.operation_id,
            &prepared_guest.public_id,
        )
        .unwrap();
    drop(reservation);
    host.confirm("main", &prepared_host.operation_id, &host_review.digest)
        .await
        .unwrap();
    actor_phase(&host, "hostListening").await;
    let export = host
        .export_invitation("main", &prepared_host.operation_id)
        .unwrap();
    let review = guest
        .review_guest("main", &prepared_guest.operation_id, &export.invitation)
        .unwrap();
    assert!(
        review.target_source.is_none() && review.instance_id.is_none() && review.run_id.is_none()
    );
    guest
        .confirm("main", &prepared_guest.operation_id, &review.digest)
        .await
        .unwrap();
    let connected = actor_phase(&guest, "guestConnected").await;
    let tcp: std::net::SocketAddr = connected.tcp_address.unwrap().parse().unwrap();
    let responded = guest
        .probe_status("main", &prepared_guest.operation_id)
        .await
        .unwrap();
    assert!(
        responded.status_state == "responded"
            && responded.status_summary.unwrap().reported_protocol == 767
            && responded.world_state == "notVerified"
    );
    fixture.stop();
    let failed = actor_phase(&host, "failed").await;
    let closed = actor_phase(&guest, "closed").await;
    assert!(
        failed.error_code == Some("connect_source_changed")
            && failed.cleanup_state == "complete"
            && closed.cleanup_state == "complete"
    );
    assert!(std::net::UdpSocket::bind(bind).is_ok());
    assert!(std::net::TcpListener::bind(tcp).is_ok());
}
