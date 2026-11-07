use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

const MARKER: &str = "AURORA_OWNED_DISCOVERY_FIXTURE_CHILD";
struct CancelStream {
    socket: TcpStream,
    stop: Arc<AtomicBool>,
    deadline: Instant,
}
impl CancelStream {
    fn check(&self) -> std::io::Result<()> {
        if self.stop.load(Ordering::Acquire) {
            return Err(std::io::ErrorKind::ConnectionAborted.into());
        }
        if Instant::now() >= self.deadline {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        Ok(())
    }

    fn poll<T>(
        &mut self,
        mut operation: impl FnMut(&mut TcpStream) -> std::io::Result<T>,
    ) -> std::io::Result<T> {
        loop {
            self.check()?;
            match operation(&mut self.socket) {
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                other => return other,
            }
        }
    }
}
impl Read for CancelStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.poll(|socket| socket.read(buffer))
    }
}
impl Write for CancelStream {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.poll(|socket| socket.write(buffer))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.check()
    }
}
fn fixture_status(
    socket: &mut CancelStream,
    expected_protocol: i32,
    reported_protocol: i32,
) -> std::io::Result<()> {
    fn length(socket: &mut impl Read) -> std::io::Result<usize> {
        let mut value = 0;
        for shift in [0, 7, 14, 21, 28] {
            let mut byte = [0];
            socket.read_exact(&mut byte)?;
            value |= usize::from(byte[0] & 127) << shift;
            if byte[0] & 128 == 0 {
                return Ok(value);
            }
        }
        Err(std::io::ErrorKind::InvalidData.into())
    }
    fn varint(mut value: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        loop {
            let mut byte = (value & 127) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 128;
            }
            bytes.push(byte);
            if value == 0 {
                return bytes;
            }
        }
    }
    let size = length(socket)?;
    if size > 1024 {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut handshake = vec![0; size];
    socket.read_exact(&mut handshake)?;
    let mut input = handshake.as_slice();
    if length(&mut input)? != 0 || length(&mut input)? != expected_protocol as usize {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut request = [0; 2];
    socket.read_exact(&mut request)?;
    if request != [1, 0] {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let json = format!(
        r#"{{"version":{{"protocol":{reported_protocol},"name":"fixture"}},"players":{{"max":2,"online":0}},"description":{{"text":"fixture"}}}}"#
    );
    let mut payload = vec![0];
    payload.extend(varint(json.len()));
    payload.extend(json.as_bytes());
    let mut frame = varint(payload.len());
    frame.extend(payload);
    socket.write_all(&frame)?;
    let mut ping = [0; 10];
    socket.read_exact(&mut ping)?;
    socket.write_all(&ping)
}
#[test]
#[ignore = "internal child fixture, invoked by the parent test only"]
fn owned_listener_fixture() {
    assert!(
        std::env::var_os(MARKER).is_some(),
        "owned fixture marker required"
    );
    let expected_protocol: i32 = std::env::var("AURORA_DISCOVERY_PROTOCOL")
        .unwrap_or_else(|_| "767".into())
        .parse()
        .unwrap();
    let reported_protocol: i32 = std::env::var("AURORA_DISCOVERY_REPORTED_PROTOCOL")
        .unwrap_or_else(|_| expected_protocol.to_string())
        .parse()
        .unwrap();
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
    for (index, listener) in listeners.into_iter().enumerate() {
        listener.set_nonblocking(true).unwrap();
        let stop = stop.clone();
        threads.push(thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((socket, _)) => {
                        socket.set_nonblocking(true).unwrap();
                        let mut socket = CancelStream {
                            socket,
                            stop: stop.clone(),
                            deadline: Instant::now() + Duration::from_secs(5),
                        };
                        // Test-only acknowledgement: the parent may now
                        // exercise stop with this exact accepted socket.
                        println!("DISCOVERY_ACCEPTED {index}");
                        std::io::stdout().flush().unwrap();
                        let _ = fixture_status(&mut socket, expected_protocol, reported_protocol);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        }));
    }
    println!("DISCOVERY_READY {}", serde_json::to_string(&ports).unwrap());
    std::io::stdout().flush().unwrap();
    let mut command = String::new();
    let _ = std::io::stdin().read_line(&mut command);
    stop.store(true, Ordering::Release);
    // Accepted reads/writes also observe cancellation. They never rely on a
    // different thread interrupting a blocking Winsock operation.
    for thread in threads {
        let _ = thread.join();
    }
}
pub(crate) struct Fixture {
    child: Child,
    ports: Vec<u16>,
    accepted: mpsc::Receiver<usize>,
    reader: Option<thread::JoinHandle<()>>,
}
/// Private test clock seam: does not alter a production constructor or OS time.
/// Private selection-clock seam; does not renew authority or alter OS time.
pub(crate) fn expire_selection(manager: &DiscoveryManager) {
    manager.state.lock().unwrap().as_mut().unwrap().deadline =
        Instant::now() - Duration::from_secs(1);
}
impl Fixture {
    pub(crate) fn new() -> Self {
        Self::with_protocol(767, 767)
    }
    fn with_protocol(expected_protocol: i32, reported_protocol: i32) -> Self {
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
            .env("AURORA_DISCOVERY_PROTOCOL", expected_protocol.to_string())
            .env(
                "AURORA_DISCOVERY_REPORTED_PROTOCOL",
                reported_protocol.to_string(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let reader = BufReader::new(child.stdout.take().unwrap());
        let (send, receive) = mpsc::sync_channel(1);
        let (accepted_send, accepted) = mpsc::sync_channel(32);
        let read_task = thread::spawn(move || {
            let mut reader = reader.take(8193);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if let Some(json) = line.strip_prefix("DISCOVERY_READY ") {
                    if let Ok(ports) = serde_json::from_str::<Vec<u16>>(json.trim()) {
                        let _ = send.try_send(ports);
                    }
                } else if let Some(index) = line.strip_prefix("DISCOVERY_ACCEPTED ") {
                    if let Ok(index) = index.trim().parse() {
                        let _ = accepted_send.try_send(index);
                    }
                }
            }
        });
        let mut fixture = Self {
            child,
            ports: Vec::new(),
            accepted,
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
        self.stop_with_deadline(Duration::from_secs(7));
    }
    fn stop_with_deadline(&mut self, maximum: Duration) {
        if let Some(mut input) = self.child.stdin.take() {
            let _ = input.write_all(b"stop\n");
        }
        let deadline = Instant::now() + maximum;
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
fn owned_fixture_stop_interrupts_idle_and_partial_accepted_status_sockets() {
    let mut fixture = Fixture::new();
    let mut idle = TcpStream::connect(("127.0.0.1", fixture.ports[0])).unwrap();
    assert_eq!(
        fixture
            .accepted
            .recv_timeout(Duration::from_secs(5))
            .unwrap(),
        0
    );
    let mut partial = TcpStream::connect(("127.0.0.1", fixture.ports[1])).unwrap();
    // The first VarInt byte explicitly announces that another byte is needed.
    partial.write_all(&[0x80]).unwrap();
    assert_eq!(
        fixture
            .accepted
            .recv_timeout(Duration::from_secs(5))
            .unwrap(),
        1
    );
    // Less than the fixture's unchanged five-second socket timeout: success
    // requires cancellation of accepted sockets, not eventual read timeout.
    fixture.stop_with_deadline(Duration::from_secs(2));
    for socket in [&mut idle, &mut partial] {
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        match socket.read(&mut [0]) {
            Ok(0) => {}
            Err(error) => assert!(matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::NotConnected
            )),
            other => panic!("owned fixture socket remained open: {other:?}"),
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

#[tokio::test]
async fn native_profile_status_sends_the_reviewed_protocol_for_all_nine_active_pairs() {
    for index in 0..9 {
        let installed = super::super::connect_status_profile::tests::Fixture::new(index);
        let runtime = installed.profile();
        let expected = runtime.summary();
        let mut fixture =
            Fixture::with_protocol(expected.status_protocol, expected.status_protocol);
        let manager = DiscoveryManager::default();
        let inventory = manager
            .inventory_for_runtime("main", fixture.proof(), Some(runtime))
            .unwrap();
        assert_eq!(inventory.runtime.as_ref(), Some(&expected));
        let candidate = inventory
            .candidates
            .iter()
            .find(|c| c.bind_kind == "loopback" && c.target.starts_with("127.0.0.1:"))
            .unwrap();
        let review = manager
            .review("main", &inventory.inventory_id, &candidate.candidate_id)
            .unwrap();
        assert_eq!(review.status_protocol, expected.status_protocol);
        assert_eq!(review.runtime, expected);
        // A later mutable marker cannot change the native observation retained
        // with this inventory or the protocol included in its consumed review.
        std::fs::write(
            installed.instance.root().join("aurora-instance.json"),
            br#"{"minecraft":"1.21.1"}"#,
        )
        .unwrap();
        let status = manager
            .confirm_status("main", &review.review_id, &review.digest)
            .await
            .unwrap();
        assert_eq!(status.runtime, expected);
        assert_eq!(status.summary.requested_protocol, expected.status_protocol);
        assert_eq!(status.summary.reported_protocol, expected.status_protocol);
        assert!(status.summary.protocol_matches_requested && status.verified_before_after);
        assert_eq!(status.world_state, "notVerified");
        fixture.stop();
    }
}

#[tokio::test]
async fn unknown_runtime_keeps_inventory_without_review_or_network_query() {
    let mut fixture = Fixture::new();
    let manager = DiscoveryManager::default();
    let inventory = manager
        .inventory_for_runtime("main", fixture.proof(), None)
        .unwrap();
    assert!(inventory.runtime.is_none());
    assert!(!inventory.candidates.is_empty());
    assert!(matches!(
        manager.review(
            "main",
            &inventory.inventory_id,
            &inventory.candidates[0].candidate_id
        ),
        Err(DiscoveryError::RuntimeUnavailable)
    ));
    assert!(matches!(
        manager.confirm_status("main", "missing", "wrong").await,
        Err(DiscoveryError::Stale)
    ));
    assert!(fixture.accepted.try_recv().is_err());
    fixture.stop();
}

#[tokio::test]
async fn version_aware_status_preserves_an_independent_reported_protocol_mismatch() {
    let installed = super::super::connect_status_profile::tests::Fixture::new(0);
    let mut fixture = Fixture::with_protocol(340, 767);
    let manager = DiscoveryManager::default();
    let inventory = manager
        .inventory_for_runtime("main", fixture.proof(), Some(installed.profile()))
        .unwrap();
    let candidate = inventory
        .candidates
        .iter()
        .find(|c| c.bind_kind == "loopback" && c.target.starts_with("127.0.0.1:"))
        .unwrap();
    let review = manager
        .review("main", &inventory.inventory_id, &candidate.candidate_id)
        .unwrap();
    let status = manager
        .confirm_status("main", &review.review_id, &review.digest)
        .await
        .unwrap();
    assert_eq!(status.summary.requested_protocol, 340);
    assert_eq!(status.summary.reported_protocol, 767);
    assert!(!status.summary.protocol_matches_requested);
    assert_eq!(status.world_state, "notVerified");
    fixture.stop();
}

pub(crate) async fn selection(manager: &DiscoveryManager, fixture: &mut Fixture) -> TargetStatus {
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
    manager
        .confirm_status("main", &review.review_id, &review.digest)
        .await
        .unwrap()
}
pub(crate) async fn approved(manager: &DiscoveryManager, fixture: &mut Fixture) -> ApprovedTarget {
    let status = selection(manager, fixture).await;
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
    use super::super::connect::{
        ConnectError, ConnectManager, ConnectMode, DiscoveredPreparationError, HostRequest,
    };
    let mut fixture = Fixture::new();
    let discovery = DiscoveryManager::default();
    let actor = ConnectManager::default();
    let reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = reservation.local_addr().unwrap();
    let target = selection(&discovery, &mut fixture).await;
    let request = HostRequest {
        mode: ConnectMode::Loopback,
        bind_address: bind.to_string(),
        game_target: target.target.clone(),
        lifetime_seconds: 60,
    };
    let mut mismatch = request.clone();
    mismatch.game_target = "127.0.0.1:1".into();
    assert!(matches!(
        actor
            .reserve_host_discovered("main", mismatch, target.selection_id, discovery.clone())
            .unwrap()
            .wait()
            .await,
        Err(DiscoveredPreparationError::Connect(
            ConnectError::InvalidRequest
        ))
    ));
    let failed = actor.snapshot("main").unwrap();
    actor
        .stop("main", failed.operation_id.as_deref().unwrap())
        .await
        .unwrap();
    let expired = selection(&discovery, &mut fixture).await;
    expire_selection(&discovery);
    assert!(matches!(
        actor
            .reserve_host_discovered(
                "main",
                request.clone(),
                expired.selection_id,
                discovery.clone()
            )
            .unwrap()
            .wait()
            .await,
        Err(DiscoveredPreparationError::Discovery(DiscoveryError::Stale))
    ));
    let failed = actor.snapshot("main").unwrap();
    actor
        .stop("main", failed.operation_id.as_deref().unwrap())
        .await
        .unwrap();
    let target = selection(&discovery, &mut fixture).await;
    let prepared = actor
        .reserve_host_discovered(
            "main",
            request.clone(),
            target.selection_id,
            discovery.clone(),
        )
        .unwrap()
        .wait()
        .await
        .unwrap();
    // The consumed selection expires independently of the already activated guard.
    expire_selection(&discovery);
    let review = actor
        .reserve_host_review(
            "main",
            &prepared.operation_id,
            &aurora_connect_lab::generate_key().public().to_string(),
        )
        .unwrap()
        .wait()
        .await
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
    let target = selection(&discovery, &mut fixture).await;
    let reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = reservation.local_addr().unwrap();
    let mut request = request;
    request.bind_address = bind.to_string();
    request.game_target = target.target;
    let prepared = actor
        .reserve_host_discovered("main", request, target.selection_id, discovery.clone())
        .unwrap()
        .wait()
        .await
        .unwrap();
    let review = actor
        .reserve_host_review(
            "main",
            &prepared.operation_id,
            &aurora_connect_lab::generate_key().public().to_string(),
        )
        .unwrap()
        .wait()
        .await
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
    let target = selection(&discovery, &mut fixture).await;
    let reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let bind = reservation.local_addr().unwrap();
    let host = ConnectManager::default();
    let guest = ConnectManager::default();
    let prepared_host = host
        .reserve_host_discovered(
            "main",
            HostRequest {
                mode: ConnectMode::Loopback,
                bind_address: bind.to_string(),
                game_target: target.target,
                lifetime_seconds: 30,
            },
            target.selection_id,
            discovery.clone(),
        )
        .unwrap()
        .wait()
        .await
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
        .reserve_host_review(
            "main",
            &prepared_host.operation_id,
            &prepared_guest.public_id,
        )
        .unwrap()
        .wait()
        .await
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
