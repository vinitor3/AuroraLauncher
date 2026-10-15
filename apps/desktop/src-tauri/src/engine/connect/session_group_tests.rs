//! Native static-group slot with real Host + two independently authenticated
//! library Guests. Only loopback; no fixed desktop guest port, Java or internet.
use super::*;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    time::{sleep, timeout},
};

const WAIT: Duration = Duration::from_secs(8);
fn local() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    timeout(WAIT, future)
        .await
        .expect("native_group_fixture_deadline")
}
fn request(bind: SocketAddr, game: SocketAddr, udp: SocketAddr) -> GroupHostRequest {
    serde_json::from_value(json!({"mode":"loopback","bindAddress":bind.to_string(),
        "gameTarget":game.to_string(),"udpTarget":udp.to_string()}))
    .unwrap()
}
fn participant(key: &lab::SecretKey, admission: u16, session: u16) -> GroupParticipantRequest {
    serde_json::from_value(json!({"guestPublicId":key.public().to_string(),
        "admissionSeconds":admission,"sessionSeconds":session}))
    .unwrap()
}
fn value(snapshot: &ConnectSnapshot) -> Value {
    serde_json::to_value(snapshot).unwrap()
}
fn rows(snapshot: &ConnectSnapshot) -> Vec<Value> {
    value(snapshot)["hostGroup"]["participants"]
        .as_array()
        .unwrap()
        .clone()
}
fn assert_separate(snapshot: &ConnectSnapshot) {
    let s = value(snapshot);
    assert_eq!(s["role"], "host");
    for field in [
        "preparation",
        "review",
        "invitationProfile",
        "sessionTiming",
        "fragmentedUdp",
        "udpRecovery",
        "tcpAddress",
        "statusSummary",
    ] {
        assert!(
            s.get(field).is_none_or(Value::is_null),
            "group_mixed_with_single_field_{field}"
        );
    }
    for field in ["udpState", "udpAddress", "udpMaxPayload", "udpErrorCode"] {
        assert!(
            s.get(field).is_none(),
            "group_mixed_with_guest_field_{field}"
        );
    }
    assert_eq!(s["statusState"], "notChecked");
    assert_eq!(s["worldState"], "notVerified");
    assert_eq!(s["simpleVoiceChatState"], "disabled");
    assert_eq!(s["plasmoVoiceState"], "disabled");
    assert_eq!(s["relayEnabled"], false);
}
async fn until(
    manager: &ConnectManager,
    accept: impl Fn(&ConnectSnapshot) -> bool,
) -> ConnectSnapshot {
    bounded(async {
        loop {
            let snapshot = manager.snapshot("main").unwrap();
            if accept(&snapshot) {
                return snapshot;
            }
            assert_ne!(snapshot.phase, "failed", "native_group_failed");
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
}
async fn after(deadline: u64) {
    sleep(Duration::from_millis(
        deadline.saturating_sub(lab::unix_millis()) + 80,
    ))
    .await;
}
#[derive(Default)]
struct Observed {
    tcp: AtomicUsize,
    udp: AtomicUsize,
    sources: Mutex<BTreeSet<SocketAddr>>,
    tags: Mutex<BTreeSet<u8>>,
}
#[derive(Default)]
struct ConcurrentTraffic {
    enabled: AtomicBool,
    first_udp: tokio::sync::Notify,
    allow_tcp: CancellationToken,
    allow_return: CancellationToken,
    first_chunks: AtomicUsize,
    progress: tokio::sync::Notify,
    tcp_bytes: [AtomicUsize; 2],
    tcp_bytes_while_udp_pending: AtomicUsize,
}
struct Fixture {
    manager: Arc<ConnectManager>,
    prepared: GroupPreparation,
    review: GroupReview,
    keys: [lab::SecretKey; 2],
    bind: SocketAddr,
    held: Option<std::net::UdpSocket>,
    guests: Vec<lab::Guest>,
    tcp_addresses: Vec<SocketAddr>,
    udp_addresses: Vec<SocketAddr>,
    cancel: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
    observed: Arc<Observed>,
    traffic: Arc<ConcurrentTraffic>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.cancel.cancel();
        for task in &self.tasks {
            task.abort();
        }
    }
}
impl Fixture {
    async fn new(lifetimes: [(u16, u16); 2]) -> Self {
        let tcp = TcpListener::bind(local()).await.unwrap();
        let udp = UdpSocket::bind(local()).await.unwrap();
        let game = tcp.local_addr().unwrap();
        let aux = udp.local_addr().unwrap();
        let held = std::net::UdpSocket::bind(local()).unwrap();
        let bind = held.local_addr().unwrap();
        let manager = Arc::new(ConnectManager::default());
        let prepared = manager
            .prepare_session_group_host("main", request(bind, game, aux))
            .unwrap();
        let keys = [lab::generate_key(), lab::generate_key()];
        let participants = keys
            .iter()
            .zip(lifetimes)
            .map(|(key, (admission, session))| participant(key, admission, session))
            .collect();
        let review = manager
            .review_session_group_host("main", &prepared.operation_id, participants)
            .unwrap();
        let observed = Arc::new(Observed::default());
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let seen = observed.clone();
        let tcp_task = tokio::spawn(async move {
            let mut children = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _=token.cancelled()=>break,
                    accepted=tcp.accept()=>{
                        let (mut socket,_)=accepted.unwrap();
                        seen.tcp.fetch_add(1,Ordering::SeqCst);
                        children.spawn(async move {
                            let (mut reader,mut writer)=socket.split();
                            let _=tokio::io::copy(&mut reader,&mut writer).await;
                            let _=writer.shutdown().await;
                        });
                    },
                    done=children.join_next(),if !children.is_empty()=>{done.unwrap().unwrap();}
                }
            }
            children.shutdown().await;
        });
        let token = cancel.clone();
        let seen = observed.clone();
        let traffic = Arc::new(ConcurrentTraffic::default());
        let udp_traffic = traffic.clone();
        let udp_task = tokio::spawn(async move {
            let mut bytes = [0; 4098];
            loop {
                let (size, source) = tokio::select! {
                    _=token.cancelled()=>break,
                    result=udp.recv_from(&mut bytes)=>result.unwrap(),
                };
                assert!(size <= 4096, "oversize_native_group_endpoint");
                seen.udp.fetch_add(1, Ordering::SeqCst);
                seen.sources.lock().unwrap().insert(source);
                if size > 0 {
                    seen.tags.lock().unwrap().insert(bytes[0]);
                }
                if udp_traffic.enabled.load(Ordering::Acquire) {
                    udp_traffic.first_udp.notify_one();
                    tokio::select! {
                        _=token.cancelled()=>break,
                        _=udp_traffic.allow_return.cancelled()=>{},
                    }
                }
                assert_eq!(udp.send_to(&bytes[..size], source).await.unwrap(), size);
            }
        });
        Self {
            manager,
            prepared,
            review,
            keys,
            bind,
            held: Some(held),
            guests: vec![],
            tcp_addresses: vec![game],
            udp_addresses: vec![aux, bind],
            cancel,
            tasks: vec![tcp_task, udp_task],
            observed,
            traffic,
        }
    }
    fn id(&self) -> &str {
        &self.prepared.operation_id
    }
    fn participant_id(&self, index: usize) -> String {
        serde_json::to_value(&self.review).unwrap()["participants"][index]["participantId"]
            .as_str()
            .unwrap()
            .into()
    }
    fn export(&self, index: usize) -> GroupInvitationExport {
        self.manager
            .export_session_group_invitation("main", self.id(), &self.participant_id(index))
            .unwrap()
    }
    async fn start(&mut self) -> ConnectSnapshot {
        self.held.take();
        let starting = self
            .manager
            .confirm_session_group_host("main", self.id(), &self.review.digest)
            .await
            .unwrap();
        assert_eq!(starting.phase, "starting");
        assert_separate(&starting);
        let listening = until(&self.manager, |s| s.phase == "hostListening").await;
        assert!(listening.expires_at_ms.is_none());
        assert_separate(&listening);
        listening
    }
    async fn guest(&mut self, index: usize, exported: &GroupInvitationExport) -> lab::Guest {
        let invitation = lab::SessionInvitation::verify_fragmented_loopback(
            &exported.invitation,
            self.prepared.public_id.parse().unwrap(),
            self.keys[index].public(),
        )
        .unwrap();
        let guest = bounded(lab::Guest::connect_invited_fragmented_session(
            self.keys[index].clone(),
            &invitation,
            lab::Limits {
                handshake_timeout: Duration::from_secs(5),
                ..lab::Limits::default()
            },
        ))
        .await
        .unwrap();
        self.udp_addresses.extend(guest.bound_sockets());
        self.guests.push(guest.clone());
        guest
    }
    async fn bridges(&mut self, guest: &lab::Guest) -> (lab::TcpBridge, lab::UdpBridge) {
        let tcp = bounded(guest.tcp_bridge("game", 1, local())).await.unwrap();
        let udp = bounded(guest.udp_bridge_fragmented("aux", 1, local()))
            .await
            .unwrap();
        self.tcp_addresses.push(tcp.local_addr());
        self.udp_addresses.push(udp.local_addr());
        (tcp, udp)
    }
    async fn origin(&mut self) -> UdpSocket {
        let udp = UdpSocket::bind(local()).await.unwrap();
        self.udp_addresses.push(udp.local_addr().unwrap());
        udp
    }
    async fn cleanup(mut self) {
        bounded(self.manager.stop("main", self.id())).await.unwrap();
        for guest in &self.guests {
            bounded(guest.shutdown()).await;
        }
        self.guests.clear();
        self.held.take();
        self.cancel.cancel();
        for task in self.tasks.drain(..) {
            bounded(task).await.unwrap();
        }
        self.udp_addresses
            .extend(self.observed.sources.lock().unwrap().iter().copied());
        let tcp = std::mem::take(&mut self.tcp_addresses);
        let udp = std::mem::take(&mut self.udp_addresses);
        drop(self);
        let mut rebind_tcp = vec![];
        let mut rebind_udp = vec![];
        for address in tcp.into_iter().collect::<BTreeSet<_>>() {
            rebind_tcp.push(TcpListener::bind(address).await.unwrap());
        }
        for address in udp.into_iter().collect::<BTreeSet<_>>() {
            rebind_udp.push(UdpSocket::bind(address).await.unwrap());
        }
    }
}
async fn transfer(stream: &mut TcpStream, tag: u8) {
    bounded(async {
        let bytes = vec![tag; 32768];
        stream.write_all(&bytes).await.unwrap();
        let mut reply = vec![0; bytes.len()];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply, bytes);
    })
    .await;
}
async fn paced_transfer(
    stream: &mut TcpStream,
    tag: u8,
    index: usize,
    traffic: &ConcurrentTraffic,
) {
    bounded(async {
        traffic.allow_tcp.cancelled().await;
        let bytes = [tag; 1024];
        for chunk in 0..32 {
            stream.write_all(&bytes).await.unwrap();
            let mut reply = [0; 1024];
            stream.read_exact(&mut reply).await.unwrap();
            assert_eq!(reply, bytes);
            traffic.tcp_bytes[index].fetch_add(bytes.len(), Ordering::AcqRel);
            if !traffic.allow_return.is_cancelled() {
                traffic
                    .tcp_bytes_while_udp_pending
                    .fetch_add(bytes.len(), Ordering::AcqRel);
            }
            if chunk == 0 {
                traffic.first_chunks.fetch_add(1, Ordering::AcqRel);
                traffic.progress.notify_one();
            }
            if chunk < 31 {
                sleep(Duration::from_millis(5)).await;
            }
        }
    })
    .await;
}
async fn coordinate_traffic(traffic: &ConcurrentTraffic) {
    bounded(async {
        traffic.first_udp.notified().await;
        traffic.allow_tcp.cancel();
        while traffic.first_chunks.load(Ordering::Acquire) < 2 {
            let next = traffic.progress.notified();
            if traffic.first_chunks.load(Ordering::Acquire) >= 2 {
                break;
            }
            next.await;
        }
        traffic.allow_return.cancel();
    })
    .await;
}
async fn datagram(origin: &UdpSocket, target: SocketAddr, tag: u8, size: usize) {
    bounded(datagram_exchange(origin, target, tag, size)).await;
}
async fn datagram_exchange(origin: &UdpSocket, target: SocketAddr, tag: u8, size: usize) {
    let bytes: Vec<_> = (0..size)
        .map(|i| tag.wrapping_add((i % 251) as u8))
        .collect();
    origin.send_to(&bytes, target).await.unwrap();
    let mut reply = [0; 4098];
    let (n, source) = origin.recv_from(&mut reply).await.unwrap();
    assert!(
        source == target && n == size && reply[..n] == bytes,
        "native_group_cross_return"
    );
}
async fn diagnosed_datagram(
    f: &Fixture,
    origin: &UdpSocket,
    target: SocketAddr,
    tag: u8,
    size: usize,
) {
    if timeout(WAIT, datagram_exchange(origin, target, tag, size))
        .await
        .is_err()
    {
        let host = lock(&f.manager.live.state)
            .unwrap()
            .attempt
            .as_ref()
            .and_then(session_group::metrics);
        let guests: Vec<_> = f.guests.iter().map(lab::Guest::metrics).collect();
        eprintln!(
            "native_group_udp_diagnostic {}",
            serde_json::json!({
                "tag":tag,"payloadBytes":size,"endpointPackets":f.observed.udp.load(Ordering::SeqCst),
                "endpointTags":*f.observed.tags.lock().unwrap(),"host":host,"guests":guests,
            })
        );
        panic!("native_group_fixture_deadline UDP tag {tag}");
    }
}
async fn eof(stream: &mut TcpStream) {
    assert!(matches!(
        bounded(stream.read(&mut [0])).await,
        Ok(0) | Err(_)
    ));
}

#[tokio::test]
async fn native_group_invalid_or_partial_review_never_publishes_authority_or_loses_draft() {
    for body in [
        json!({"mode":"loopback","bindAddress":"127.0.0.1:43000","gameTarget":"127.0.0.1:43001","udpTarget":"127.0.0.1:43002","pid":1}),
        json!({"mode":"automatic","bindAddress":"127.0.0.1:43000","gameTarget":"127.0.0.1:43001","udpTarget":"127.0.0.1:43002"}),
    ] {
        assert!(serde_json::from_value::<GroupHostRequest>(body).is_err());
    }
    for body in [
        json!({"guestPublicId":"00".repeat(32),"admissionSeconds":1.5,"sessionSeconds":5}),
        json!({"guestPublicId":"00".repeat(32),"admissionSeconds":1,"sessionSeconds":5,"destination":"127.0.0.1:1"}),
    ] {
        assert!(serde_json::from_value::<GroupParticipantRequest>(body).is_err());
    }
    let manager = ConnectManager::default();
    let held = std::net::UdpSocket::bind(local()).unwrap();
    let bind = held.local_addr().unwrap();
    let game: SocketAddr = "127.0.0.1:43001".parse().unwrap();
    let aux: SocketAddr = "127.0.0.1:43002".parse().unwrap();
    for (field, bad) in [
        ("bindAddress", "0.0.0.0:43000"),
        ("gameTarget", "192.168.1.1:43001"),
        ("udpTarget", "[::]:43002"),
    ] {
        let mut body = json!({"mode":"loopback","bindAddress":bind.to_string(),"gameTarget":game.to_string(),"udpTarget":aux.to_string()});
        body[field] = json!(bad);
        assert!(matches!(
            manager.prepare_session_group_host("main", serde_json::from_value(body).unwrap()),
            Err(ConnectError::InvalidRequest)
        ));
        assert!(manager.snapshot("main").unwrap().operation_id.is_none());
    }
    let prepared = manager
        .prepare_session_group_host("main", request(bind, game, aux))
        .unwrap();
    let before = value(&manager.snapshot("main").unwrap());
    assert_eq!(before["phase"], "hostPrepared");
    assert_eq!(before["hostGroup"]["participants"], json!([]));
    let keys = [lab::generate_key(), lab::generate_key()];
    for case in 0..10 {
        let mut participants = vec![participant(&keys[0], 10, 20), participant(&keys[1], 10, 20)];
        match case {
            0=>participants.clear(),
            1=>participants.truncate(1),
            2=>participants=(0..9).map(|_|participant(&lab::generate_key(),10,20)).collect(),
            3=>participants[1]=participant(&keys[0],10,20),
            4=>participants[1]=serde_json::from_value(json!({"guestPublicId":prepared.public_id,"admissionSeconds":10,"sessionSeconds":20})).unwrap(),
            5=>participants[1]=participant(&keys[1],0,20),
            6=>participants[1]=participant(&keys[1],601,700),
            7=>participants[1]=participant(&keys[1],10,3601),
            8=>participants[1]=participant(&keys[1],20,10),
            _=>participants[1]=serde_json::from_value(json!({"guestPublicId":"not-a-peer","admissionSeconds":10,"sessionSeconds":20})).unwrap(),
        }
        assert!(
            matches!(
                manager.review_session_group_host("main", &prepared.operation_id, participants),
                Err(ConnectError::InvalidRequest)
            ),
            "invalid_group_case_{case}"
        );
        assert_eq!(
            value(&manager.snapshot("main").unwrap()),
            before,
            "invalid_review_changed_draft_{case}"
        );
    }
    let reviewed = manager
        .review_session_group_host(
            "main",
            &prepared.operation_id,
            vec![participant(&keys[0], 10, 20), participant(&keys[1], 10, 20)],
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(reviewed).unwrap()["participants"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    drop(held);
    let canary = UdpSocket::bind(bind).await.unwrap();
    drop(canary);
    manager.stop("main", &prepared.operation_id).await.unwrap();
}

#[tokio::test]
async fn native_group_policy_slot_owner_review_digest_and_old_profiles_remain_separate() {
    let mut f = Fixture::new([(15, 30), (15, 30)]).await;
    let reviewed = f.manager.snapshot("main").unwrap();
    assert_separate(&reviewed);
    assert!(
        std::net::UdpSocket::bind(f.bind).is_err(),
        "reserved_canary_retained"
    );
    assert!(matches!(
        f.manager.snapshot("other"),
        Err(ConnectError::OwnerMismatch)
    ));
    assert!(matches!(
        f.manager.prepare_session_group_host(
            "main",
            request(
                f.bind,
                "127.0.0.1:43001".parse().unwrap(),
                "127.0.0.1:43002".parse().unwrap()
            )
        ),
        Err(ConnectError::Busy)
    ));
    assert!(matches!(
        f.manager.prepare_session_host(
            "main",
            SessionHostRequest {
                mode: ConnectMode::Loopback,
                bind_address: f.bind.to_string(),
                game_target: "127.0.0.1:43001".into(),
                admission_seconds: 5,
                session_seconds: 10
            }
        ),
        Err(ConnectError::Busy)
    ));
    assert!(matches!(
        f.manager
            .confirm_session_group_host("other", f.id(), &f.review.digest)
            .await,
        Err(ConnectError::OwnerMismatch)
    ));
    assert!(matches!(
        f.manager
            .confirm_session_group_host("main", "stale", &f.review.digest)
            .await,
        Err(ConnectError::StaleOperation)
    ));
    assert!(matches!(
        f.manager
            .confirm_session_group_host("main", f.id(), "wrong")
            .await,
        Err(ConnectError::WrongDigest)
    ));
    assert!(f
        .manager
        .confirm("main", f.id(), &f.review.digest)
        .await
        .is_err());
    assert!(f
        .manager
        .review_host("main", f.id(), &f.keys[0].public().to_string())
        .is_err());
    assert!(f.manager.export_invitation("main", f.id()).is_err());
    assert!(f
        .manager
        .export_session_group_invitation("main", f.id(), &f.participant_id(0))
        .is_err());
    assert!(
        f.manager
            .review_session_group_host(
                "main",
                f.id(),
                vec![
                    participant(&f.keys[0], 15, 30),
                    participant(&f.keys[1], 15, 30)
                ]
            )
            .is_err(),
        "review_must_not_reissue"
    );
    assert_eq!(
        value(&f.manager.snapshot("main").unwrap())["hostGroup"]["review"]["digest"],
        f.review.digest
    );
    f.held.take();
    let canary = UdpSocket::bind(f.bind).await.unwrap();
    assert_eq!(f.observed.tcp.load(Ordering::SeqCst), 0);
    assert_eq!(f.observed.udp.load(Ordering::SeqCst), 0);
    drop(canary);
    f.cleanup().await;

    for profile in 0..4 {
        let manager = ConnectManager::default();
        let bind = "127.0.0.1:43000";
        let game = "127.0.0.1:43001";
        let aux = "127.0.0.1:43002";
        let prepared = match profile {
            0 => manager.prepare_host(
                "main",
                HostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: bind.into(),
                    game_target: game.into(),
                    lifetime_seconds: 30,
                },
            ),
            1 => manager.prepare_fragmented_host(
                "main",
                FragmentedHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: bind.into(),
                    game_target: game.into(),
                    udp_target: aux.into(),
                    lifetime_seconds: 30,
                },
            ),
            2 => manager.prepare_session_host(
                "main",
                SessionHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: bind.into(),
                    game_target: game.into(),
                    admission_seconds: 30,
                    session_seconds: 60,
                },
            ),
            _ => manager.prepare_session_fragmented_host(
                "main",
                SessionFragmentedHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: bind.into(),
                    game_target: game.into(),
                    udp_target: aux.into(),
                    admission_seconds: 30,
                    session_seconds: 60,
                },
            ),
        }
        .unwrap();
        let before = value(&manager.snapshot("main").unwrap());
        assert!(before.get("hostGroup").is_none());
        assert!(manager
            .review_session_group_host(
                "main",
                &prepared.operation_id,
                vec![
                    participant(&lab::generate_key(), 10, 20),
                    participant(&lab::generate_key(), 10, 20)
                ]
            )
            .is_err());
        assert!(manager
            .confirm_session_group_host("main", &prepared.operation_id, "wrong")
            .await
            .is_err());
        assert_eq!(value(&manager.snapshot("main").unwrap()), before);
        bounded(manager.stop("main", &prepared.operation_id))
            .await
            .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_group_exports_exact_individual_invitations_and_revokes_only_one_live_guest() {
    let mut f = Fixture::new([(15, 30), (15, 30)]).await;
    f.start().await;
    let a_export = f.export(0);
    let b_export = f.export(1);
    assert_ne!(a_export.participant_id, b_export.participant_id);
    assert_ne!(a_export.invitation_digest, b_export.invitation_digest);
    assert_eq!(a_export.invitation, f.export(0).invitation);
    assert_eq!(b_export.invitation, f.export(1).invitation);
    assert!(lab::SessionInvitation::verify_fragmented_loopback(
        &a_export.invitation,
        f.prepared.public_id.parse().unwrap(),
        f.keys[1].public()
    )
    .is_err());
    for (index, exported) in [&a_export, &b_export].into_iter().enumerate() {
        let row = &serde_json::to_value(&f.review).unwrap()["participants"][index];
        assert_eq!(exported.operation_id, f.id());
        assert_eq!(exported.participant_id, row["participantId"]);
        assert_eq!(exported.invitation_digest, row["invitationDigest"]);
        assert_eq!(
            serde_json::to_value(&exported.session_timing).unwrap(),
            row["sessionTiming"]
        );
        assert_eq!(
            exported.expires_at_ms,
            row["sessionTiming"]["admissionExpiresAtMs"]
                .as_u64()
                .unwrap()
        );
        assert_eq!(
            serde_json::to_value(&exported.fragmented_udp).unwrap(),
            serde_json::to_value(FragmentedUdpContract::session()).unwrap()
        );
        assert!(
            !exported.invitation.contains("udpTarget")
                && !exported.invitation.contains("gameTarget")
        );
    }
    let a = f.guest(0, &a_export).await;
    until(&f.manager, |s| rows(s)[0]["consumed"] == true).await;
    assert!(f
        .manager
        .export_session_group_invitation("main", f.id(), &f.participant_id(0))
        .is_err());
    assert_eq!(f.export(1).invitation, b_export.invitation);
    let b = f.guest(1, &b_export).await;
    let active = until(&f.manager, |s| {
        rows(s)
            .iter()
            .all(|r| r["transportActive"] == true && r["consumed"] == true)
    })
    .await;
    assert_separate(&active);
    assert!(active.expires_at_ms.is_none());
    let (ta, ua) = f.bridges(&a).await;
    let (tb, ub) = f.bridges(&b).await;
    let origins = [
        f.origin().await,
        f.origin().await,
        f.origin().await,
        f.origin().await,
    ];
    let mut sa = bounded(TcpStream::connect(ta.local_addr())).await.unwrap();
    let mut sb = bounded(TcpStream::connect(tb.local_addr())).await.unwrap();
    // This tests isolation with concurrent traffic inside the existing budget,
    // not guaranteed UDP delivery while bulk TCP exhausts the shared bucket.
    // The endpoint holds a UDP return until both TCP streams acknowledge bytes.
    f.traffic.enabled.store(true, Ordering::Release);
    tokio::join!(
        paced_transfer(&mut sa, 1, 0, &f.traffic),
        paced_transfer(&mut sb, 2, 1, &f.traffic),
        diagnosed_datagram(&f, &origins[0], ua.local_addr(), 3, 4096),
        diagnosed_datagram(&f, &origins[1], ua.local_addr(), 4, 4096),
        diagnosed_datagram(&f, &origins[2], ub.local_addr(), 5, 4096),
        diagnosed_datagram(&f, &origins[3], ub.local_addr(), 6, 4096),
        coordinate_traffic(&f.traffic)
    );
    assert_eq!(f.traffic.first_chunks.load(Ordering::Acquire), 2);
    assert!(
        f.traffic
            .tcp_bytes_while_udp_pending
            .load(Ordering::Acquire)
            >= 2048
    );
    assert!(f
        .traffic
        .tcp_bytes
        .iter()
        .all(|bytes| bytes.load(Ordering::Acquire) == 32768));
    assert_eq!(f.observed.udp.load(Ordering::Acquire), 4);
    assert_eq!(f.observed.sources.lock().unwrap().len(), 4);
    let current = f.manager.snapshot("main").unwrap();
    assert!(matches!(
        f.manager.revoke_session_group_participant(
            "other",
            f.id(),
            &f.participant_id(0),
            current.generation
        ),
        Err(ConnectError::OwnerMismatch)
    ));
    assert!(f
        .manager
        .revoke_session_group_participant(
            "main",
            f.id(),
            &f.participant_id(0),
            current.generation.saturating_sub(1)
        )
        .is_err());
    let current = f.manager.snapshot("main").unwrap();
    // Deliberately discard the command's successful response (lost IPC). The
    // next snapshot must retain the actual individual revocation, without replay.
    drop(
        f.manager
            .revoke_session_group_participant(
                "main",
                f.id(),
                &f.participant_id(0),
                current.generation,
            )
            .unwrap(),
    );
    let revoked = until(&f.manager, |s| {
        rows(s)[0]["revoked"] == true && !rows(s)[0]["transportActive"].as_bool().unwrap()
    })
    .await;
    assert_eq!(rows(&revoked)[0]["admissionValid"], false);
    assert_eq!(rows(&revoked)[0]["sessionValid"], false);
    assert_eq!(rows(&revoked)[1]["transportActive"], true);
    bounded(a.connection().closed()).await;
    eof(&mut sa).await;
    let fresh = f.origin().await;
    tokio::join!(
        transfer(&mut sb, 0xa7),
        datagram(&fresh, ub.local_addr(), 0xb8, 4096)
    );
    assert_eq!(f.observed.sources.lock().unwrap().len(), 5);
    assert_eq!(f.observed.tcp.load(Ordering::SeqCst), 2);
    let stopped = bounded(f.manager.stop("main", f.id())).await.unwrap();
    assert_eq!(stopped.phase, "closed");
    assert!(rows(&stopped).iter().all(|r| r["transportActive"] == false));
    bounded(b.connection().closed()).await;
    eof(&mut sb).await;
    bounded(ta.shutdown()).await;
    bounded(ua.shutdown()).await;
    bounded(tb.shutdown()).await;
    bounded(ub.shutdown()).await;
    drop((a, b, ta, ua, tb, ub, sa, sb, origins, fresh));
    f.cleanup().await;
}

#[tokio::test]
async fn native_group_session_expiry_closes_a_but_keeps_b_until_explicit_logout() {
    let mut f = Fixture::new([(2, 4), (15, 20)]).await;
    f.start().await;
    let ea = f.export(0);
    let eb = f.export(1);
    let a = f.guest(0, &ea).await;
    let b = f.guest(1, &eb).await;
    let (ta, ua) = f.bridges(&a).await;
    let (tb, ub) = f.bridges(&b).await;
    let oa = f.origin().await;
    let ob = f.origin().await;
    let mut sa = bounded(TcpStream::connect(ta.local_addr())).await.unwrap();
    let mut sb = bounded(TcpStream::connect(tb.local_addr())).await.unwrap();
    tokio::join!(
        transfer(&mut sa, 11),
        transfer(&mut sb, 12),
        datagram(&oa, ua.local_addr(), 13, 4096),
        datagram(&ob, ub.local_addr(), 14, 4096)
    );
    bounded(a.connection().closed()).await;
    eof(&mut sa).await;
    let expired = until(&f.manager, |s| rows(s)[0]["sessionValid"] == false).await;
    assert_eq!(expired.phase, "hostListening");
    assert!(expired.expires_at_ms.is_none());
    assert_eq!(rows(&expired)[0]["consumed"], true);
    assert_eq!(rows(&expired)[0]["transportActive"], false);
    assert_eq!(rows(&expired)[1]["transportActive"], true);
    let fresh = f.origin().await;
    tokio::join!(
        transfer(&mut sb, 15),
        datagram(&fresh, ub.local_addr(), 16, 4096)
    );
    bounded(f.manager.shutdown_all()).await;
    let closed = f.manager.snapshot("main").unwrap();
    assert_eq!(closed.phase, "closed");
    assert!(rows(&closed).iter().all(|r| r["transportActive"] == false));
    bounded(b.connection().closed()).await;
    eof(&mut sb).await;
    assert!(
        f.manager
            .prepare_session_group_host(
                "main",
                request(
                    f.bind,
                    "127.0.0.1:43001".parse().unwrap(),
                    "127.0.0.1:43002".parse().unwrap()
                )
            )
            .is_err(),
        "logout_latch_must_remain_closed"
    );
    bounded(ta.shutdown()).await;
    bounded(ua.shutdown()).await;
    bounded(tb.shutdown()).await;
    bounded(ub.shutdown()).await;
    drop((a, b, ta, ua, tb, ub, sa, sb, oa, ob, fresh));
    f.cleanup().await;
}

#[tokio::test]
async fn native_group_unused_admission_expires_independently_and_all_terminal_members_close_host() {
    let mut f = Fixture::new([(1, 15), (15, 20)]).await;
    f.start().await;
    let a = f.export(0);
    let b = f.export(1);
    let guest = f.guest(1, &b).await;
    after(a.expires_at_ms).await;
    let snapshot = until(&f.manager, |s| rows(s)[0]["admissionValid"] == false).await;
    assert_eq!(snapshot.phase, "hostListening");
    assert_eq!(rows(&snapshot)[0]["sessionValid"], true);
    assert_eq!(rows(&snapshot)[0]["consumed"], false);
    assert_eq!(rows(&snapshot)[1]["transportActive"], true);
    assert!(f
        .manager
        .export_session_group_invitation("main", f.id(), &a.participant_id)
        .is_err());
    let (tcp, udp) = f.bridges(&guest).await;
    let origin = f.origin().await;
    let mut stream = bounded(TcpStream::connect(tcp.local_addr())).await.unwrap();
    tokio::join!(
        transfer(&mut stream, 0x42),
        datagram(&origin, udp.local_addr(), 0x53, 4096)
    );
    let current = f.manager.snapshot("main").unwrap();
    f.manager
        .revoke_session_group_participant("main", f.id(), &b.participant_id, current.generation)
        .unwrap();
    let closed = until(&f.manager, |s| s.phase == "closed").await;
    assert!(rows(&closed).iter().all(|r| r["transportActive"] == false));
    bounded(guest.connection().closed()).await;
    eof(&mut stream).await;
    bounded(tcp.shutdown()).await;
    bounded(udp.shutdown()).await;
    drop((guest, tcp, udp, origin, stream));
    f.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_group_lost_start_response_and_abandoned_stop_keep_logout_cleanup_barrier() {
    let mut f = Fixture::new([(15, 30), (15, 30)]).await;
    let gate = Arc::new(session_group::GroupTestGate::default());
    {
        let mut state = f.manager.live.state.lock().unwrap();
        selected(&mut state, "main", f.id())
            .unwrap()
            .group_start_gate = Some(gate.clone());
    }
    f.held.take();
    drop(
        f.manager
            .confirm_session_group_host("main", f.id(), &f.review.digest)
            .await
            .unwrap(),
    );
    bounded(gate.entered.notified()).await;
    assert!(
        std::net::UdpSocket::bind(f.bind).is_err(),
        "actual_owned_endpoint_bound_before_publication"
    );
    assert_eq!(f.manager.snapshot("main").unwrap().phase, "starting");
    assert!(f
        .manager
        .export_session_group_invitation("main", f.id(), &f.participant_id(0))
        .is_err());
    let manager = f.manager.clone();
    let id = f.id().to_owned();
    let stop = tokio::spawn(async move { manager.stop("main", &id).await });
    until(&f.manager, |s| s.phase == "closing").await;
    assert!(!stop.is_finished());
    stop.abort();
    let _ = stop.await;
    let manager = f.manager.clone();
    let logout = tokio::spawn(async move { manager.shutdown_all().await });
    sleep(Duration::from_millis(30)).await;
    assert!(
        !logout.is_finished(),
        "logout_cannot_report_cleanup_while_owned_bind_is_held"
    );
    assert!(std::net::UdpSocket::bind(f.bind).is_err());
    gate.release.notify_one();
    bounded(logout).await.unwrap();
    let closed = f.manager.snapshot("main").unwrap();
    assert_eq!(closed.phase, "closed");
    assert_eq!(closed.cleanup_state, "complete");
    assert!(rows(&closed).iter().all(|r| r["transportActive"] == false));
    assert_eq!(f.observed.tcp.load(Ordering::SeqCst), 0);
    assert_eq!(f.observed.udp.load(Ordering::SeqCst), 0);
    f.cleanup().await;
}

#[tokio::test]
async fn native_group_one_expired_review_member_refuses_whole_group_before_bind() {
    let mut f = Fixture::new([(1, 15), (15, 20)]).await;
    let original = serde_json::to_value(&f.review).unwrap();
    assert!(
        f.review.expires_at_ms
            <= original["participants"][0]["sessionTiming"]["admissionExpiresAtMs"]
                .as_u64()
                .unwrap()
    );
    after(f.review.expires_at_ms).await;
    assert!(matches!(
        f.manager
            .confirm_session_group_host("main", f.id(), &f.review.digest)
            .await,
        Err(ConnectError::PlanExpired)
    ));
    let closed = f.manager.snapshot("main").unwrap();
    assert_eq!(closed.phase, "closed");
    assert_eq!(closed.cleanup_state, "complete");
    assert_eq!(closed.error_code, Some(ConnectError::PlanExpired.code()));
    assert!(rows(&closed)
        .iter()
        .all(|row| row["consumed"] == false && row["transportActive"] == false));
    assert!(f
        .manager
        .export_session_group_invitation("main", f.id(), &f.participant_id(1))
        .is_err());
    f.held.take();
    let held = UdpSocket::bind(f.bind).await.unwrap();
    assert_eq!(f.observed.tcp.load(Ordering::SeqCst), 0);
    assert_eq!(f.observed.udp.load(Ordering::SeqCst), 0);
    drop(held);
    f.cleanup().await;
}

#[tokio::test]
async fn native_group_worker_panic_keeps_cleanup_failed_and_never_claims_transport_closed() {
    let f = Fixture::new([(15, 30), (15, 30)]).await;
    // An internal lifecycle fixture, deliberately WITHOUT a real Connect Host.
    // It exercises JoinError publication without leaking or aborting transport.
    {
        let mut state = f.manager.live.state.lock().unwrap();
        let attempt = selected(&mut state, "main", f.id()).unwrap();
        attempt.snapshot.phase = "hostListening";
        attempt.snapshot.transport_state = "listening";
        attempt.snapshot.cleanup_state = "pending";
        attempt.snapshot.expires_at_ms = None;
        let group = attempt.snapshot.host_group.as_mut().unwrap();
        group.preparation = None;
        group.review = None;
        group.participants[0].consumed = true;
        group.participants[0].transport_active = true;
        attempt.worker = Some(tokio::spawn(async {
            panic!("native_group_test_worker_failure")
        }));
    }
    bounded(async {
        loop {
            let finished = {
                let mut state = f.manager.live.state.lock().unwrap();
                selected(&mut state, "main", f.id())
                    .unwrap()
                    .worker
                    .as_ref()
                    .unwrap()
                    .is_finished()
            };
            if finished {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    let failed = bounded(f.manager.stop("main", f.id())).await.unwrap();
    assert_eq!(failed.phase, "failed");
    assert_eq!(failed.cleanup_state, "failed");
    assert_eq!(
        failed.transport_state, "listening",
        "cannot_infer_transport_cleanup_from_worker_panic"
    );
    assert_eq!(failed.error_code, Some(ConnectError::Internal.code()));
    assert!(failed.expires_at_ms.is_none());
    let participants = rows(&failed);
    assert_eq!(participants[0]["consumed"], true);
    assert_eq!(participants[1]["consumed"], false);
    assert!(participants
        .iter()
        .all(|row| row["transportActive"] == false
            && row["admissionValid"] == false
            && row["sessionValid"] == false));
    let group = &value(&failed)["hostGroup"];
    assert!(group["preparation"].is_null() && group["review"].is_null());
    assert!(matches!(
        f.manager.prepare_session_group_host(
            "main",
            request(
                f.bind,
                "127.0.0.1:43001".parse().unwrap(),
                "127.0.0.1:43002".parse().unwrap()
            )
        ),
        Err(ConnectError::Busy)
    ));
    assert_eq!(f.observed.tcp.load(Ordering::SeqCst), 0);
    assert_eq!(f.observed.udp.load(Ordering::SeqCst), 0);
    f.cleanup().await;
}
