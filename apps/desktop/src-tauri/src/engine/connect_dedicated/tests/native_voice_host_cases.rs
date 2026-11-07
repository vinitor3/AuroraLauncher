//! Additional86 cases over the original producer and real Host/Guests.
use super::voice_fixture::{VoiceFixture, PROFILES};
use super::voice_producer_fixture::{Action, Harness, WAIT};
use super::voice_runtime::{limits, prepare, ready, restored, Gate, Release, Running};
use super::voice_transport_fixture::TCP_MAGIC;
use super::*;
use crate::engine::connect::{ConnectManager, GroupParticipantRequest};
use aurora_connect_lab as lab;
use producer::{NativeVoiceHostBinding, ProducerPoint, TestHooks};
use std::{
    net::SocketAddr,
    sync::atomic::{AtomicBool, AtomicUsize},
};
use tokio_util::sync::CancellationToken;

async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(WAIT, future)
        .await
        .expect("native86_case_deadline")
}
struct PendingVoice {
    gate: Arc<Gate>,
    task: Option<
        std::thread::JoinHandle<
            std::result::Result<producer::VoiceRuntimeSample, producer::VoiceRuntimeError>,
        >,
    >,
}
impl PendingVoice {
    fn start(query: producer::ReservedVoiceQuery, gate: Arc<Gate>) -> Self {
        Self {
            gate,
            task: Some(std::thread::spawn(move || query.execute())),
        }
    }
    fn finish(
        mut self,
    ) -> std::result::Result<producer::VoiceRuntimeSample, producer::VoiceRuntimeError> {
        self.gate.release();
        self.task
            .take()
            .unwrap()
            .join()
            .expect("native86_busy_worker_join")
    }
}
impl Drop for PendingVoice {
    fn drop(&mut self) {
        self.gate.release();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}
struct Fixture {
    running: Running,
    manager: Arc<ConnectManager>,
    binding: NativeVoiceHostBinding,
    id: String,
    generation: u64,
    digest: String,
    participants: Vec<String>,
    host_id: String,
    keys: [lab::SecretKey; 2],
    held: Option<std::net::UdpSocket>,
    bind: SocketAddr,
    paths: TestPaths,
    harness: Harness,
    _lab: VoiceFixture,
}
impl Fixture {
    async fn prepared(
        profile: NativeVoiceProfile,
        hooks: Arc<TestHooks>,
        lifetimes: [(u16, u16); 2],
    ) -> Self {
        let fixture = VoiceFixture::new(profile);
        let mut harness = Harness::new(fixture.stored_profile());
        let mut runtime_limits = limits();
        runtime_limits.lifetime_ms = 20_000;
        let (plan, paths, _) = prepare(&fixture, &harness, hooks, Hooks::default(), runtime_limits);
        let running = Running::start(plan, CancellationToken::new());
        harness.ready();
        ready(running.control()).await;
        let manager = Arc::new(ConnectManager::default());
        let binding = running.control().native_voice_host_binding().unwrap();
        let held = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let bind = held.local_addr().unwrap();
        let keys = [lab::generate_key(), lab::generate_key()];
        let roster = keys
            .iter()
            .zip(lifetimes)
            .map(|(key, (admission, session))| GroupParticipantRequest {
                guest_public_id: key.public().to_string(),
                admission_seconds: admission,
                session_seconds: session,
            })
            .collect();
        let preparation = manager
            .prepare_native_voice_group("main", binding.clone(), bind, roster)
            .unwrap()
            .wait()
            .await
            .unwrap();
        bounded(async {
            while !manager.native_voice_work_finished_for_test() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        let review = manager
            .review_native_voice_group("main", &preparation.operation_id, preparation.generation)
            .unwrap()
            .wait()
            .await
            .unwrap();
        bounded(async {
            while !manager.native_voice_work_finished_for_test() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        Self {
            _lab: fixture,
            paths,
            harness,
            running,
            manager,
            binding,
            id: preparation.operation_id,
            generation: review.data.generation,
            digest: review.digest,
            participants: review
                .data
                .participants
                .iter()
                .map(|p| p.participant_id.clone())
                .collect(),
            host_id: preparation.host_public_id,
            keys,
            held: Some(held),
            bind,
        }
    }
    async fn start(&mut self) {
        self.held.take();
        bounded(
            self.manager
                .confirm_native_voice_group("main", &self.id, self.generation, &self.digest)
                .unwrap()
                .wait(),
        )
        .await
        .unwrap();
    }
    fn invitation(&self, index: usize) -> lab::SessionInvitation {
        let encoded = self
            .manager
            .export_native_voice_invitation("main", &self.id, &self.participants[index])
            .unwrap();
        lab::SessionInvitation::verify_fragmented_loopback(
            &encoded,
            self.host_id.parse().unwrap(),
            self.keys[index].public(),
        )
        .unwrap()
    }
    async fn guest(&self, index: usize) -> lab::Guest {
        bounded(lab::Guest::connect_invited_fragmented_session(
            self.keys[index].clone(),
            &self.invitation(index),
            lab::Limits::default(),
        ))
        .await
        .unwrap()
    }
    async fn tcp(&self, guest: &lab::Guest, marker: u8) {
        let (mut send, mut recv) = bounded(guest.open_tcp("game", 1)).await.unwrap();
        send.write_all(&[TCP_MAGIC.as_slice(), self.harness.nonce().as_bytes()].concat())
            .await
            .unwrap();
        let data = [marker; 1024];
        send.write_all(&(data.len() as u16).to_be_bytes())
            .await
            .unwrap();
        send.write_all(&data).await.unwrap();
        let mut length = [0; 2];
        bounded(recv.read_exact(&mut length)).await.unwrap();
        assert_eq!(length, (data.len() as u16).to_be_bytes());
        let mut echo = [0; 1024];
        bounded(recv.read_exact(&mut echo)).await.unwrap();
        assert_eq!(echo, data);
        send.write_all(&[0, 0]).await.unwrap();
        bounded(recv.read_exact(&mut length)).await.unwrap();
        assert_eq!(length, [0, 0]);
        send.finish().unwrap();
    }
    async fn retired(&self) {
        bounded(async {
            loop {
                if self
                    .manager
                    .native_voice_host_observation("main", &self.id)
                    .unwrap()
                    .udp_retired()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
    }
    async fn finish(self, guests: Vec<lab::Guest>) {
        for guest in &guests {
            guest.shutdown().await;
        }
        drop(guests);
        assert_eq!(
            self.manager
                .stop("main", &self.id)
                .await
                .unwrap()
                .cleanup_state,
            "complete"
        );
        drop(std::net::UdpSocket::bind(self.bind).expect("native86_case_host_rebind"));
        let report = self.running.finish().await;
        restored(&self.paths, &report);
        self.harness.ports_released();
        self.harness.finish();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_real_voice_job_busy_withdraws_udp_and_busy_tcp_keeps_game() {
    for profile in PROFILES {
        let gate = Arc::new(Gate::default());
        let called = gate.clone();
        let armed = Arc::new(AtomicBool::new(false));
        let active = armed.clone();
        let hooks = Arc::new(TestHooks {
            point: Some(Arc::new(move |point| {
                if point == ProducerPoint::VoiceBeforeFinalGate && active.load(Ordering::Acquire) {
                    called.reach();
                }
            })),
            ..TestHooks::default()
        });
        let mut fixture = Fixture::prepared(profile, hooks, [(15, 18); 2]).await;
        let _release = Release(gate.clone());
        fixture.start().await;
        let guests = vec![fixture.guest(0).await, fixture.guest(1).await];
        for guest in &guests {
            fixture.tcp(guest, 11).await;
        }
        armed.store(true, Ordering::Release);
        let query = fixture.binding.reserve_voice().unwrap();
        let worker = PendingVoice::start(query, gate.clone());
        gate.reached();
        fixture.retired().await;
        for guest in &guests {
            fixture.tcp(guest, 23).await;
        }
        // At least two distinct TCP reservation opportunities while the same
        // external84 job, after real OS observation, remains held.
        tokio::time::sleep(Duration::from_millis(2200)).await;
        assert!(matches!(
            fixture.binding.reserve_tcp(),
            Err(producer::DedicatedError::Busy)
        ));
        for guest in &guests {
            fixture.tcp(guest, 37).await;
            assert!(guest.connection().close_reason().is_none());
        }
        gate.release();
        assert!(worker.finish().is_ok());
        assert!(fixture
            .manager
            .native_voice_host_observation("main", &fixture.id)
            .unwrap()
            .udp_retired());
        for guest in &guests {
            assert!(matches!(
                bounded(guest.open_udp_fragmented("aux", 1)).await,
                Err(lab::Error::ServiceDenied)
            ));
        }
        fixture.finish(guests).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_before_and_after_real_host_bind_releases_transport_before_held_query() {
    for profile in PROFILES {
        for hold_query in [3, 4] {
            let gate = Arc::new(Gate::default());
            let called = gate.clone();
            let calls = Arc::new(AtomicUsize::new(0));
            let count = calls.clone();
            let hooks = Arc::new(TestHooks {
                point: Some(Arc::new(move |point| {
                    if point == ProducerPoint::VoiceBeforeFinalGate
                        && count.fetch_add(1, Ordering::AcqRel) + 1 == hold_query
                    {
                        called.reach();
                    }
                })),
                ..TestHooks::default()
            });
            let mut fixture = Fixture::prepared(profile, hooks, [(15, 18); 2]).await;
            let _release = Release(gate.clone());
            assert_eq!(calls.load(Ordering::Acquire), 2);
            fixture.held.take();
            let pending = fixture
                .manager
                .confirm_native_voice_group(
                    "main",
                    &fixture.id,
                    fixture.generation,
                    &fixture.digest,
                )
                .unwrap();
            gate.reached();
            assert_eq!(calls.load(Ordering::Acquire), hold_query);
            let bind_result = std::net::UdpSocket::bind(fixture.bind);
            assert_eq!(bind_result.is_ok(), hold_query == 3);
            drop(bind_result);
            drop(pending);
            let mut stopping = {
                let manager = fixture.manager.clone();
                let id = fixture.id.clone();
                tokio::spawn(async move { manager.stop("main", &id).await })
            };
            bounded(async {
                loop {
                    if let Ok(socket) = std::net::UdpSocket::bind(fixture.bind) {
                        drop(socket);
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
            assert!(
                tokio::time::timeout(Duration::from_millis(40), &mut stopping)
                    .await
                    .is_err()
            );
            assert_eq!(
                fixture.manager.snapshot("main").unwrap().cleanup_state,
                "pending"
            );
            stopping.abort();
            assert!(stopping.await.unwrap_err().is_cancelled());
            gate.release();
            assert_eq!(
                fixture
                    .manager
                    .stop("main", &fixture.id)
                    .await
                    .unwrap()
                    .cleanup_state,
                "complete"
            );
            assert!(fixture.binding.reserve_voice().unwrap().execute().is_ok());
            fixture.finish(Vec::new()).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn consumed_ingress_expiry_preserves_game_and_unadmitted_expiry_grants_nothing() {
    for profile in PROFILES {
        let mut fixture =
            Fixture::prepared(profile, Arc::new(TestHooks::default()), [(3, 12); 2]).await;
        fixture.start().await;
        let unopened = fixture.invitation(1);
        let admitted = fixture.invitation(0);
        let guest = fixture.guest(0).await;
        fixture.tcp(&guest, 43).await;
        tokio::time::sleep(Duration::from_millis(
            admitted
                .admission_expires_at_unix_ms()
                .saturating_sub(lab::unix_millis())
                + 80,
        ))
        .await;
        assert!(admitted.check_admission_current().is_err());
        assert!(admitted.check_session_current().is_ok());
        fixture.tcp(&guest, 47).await;
        assert!(bounded(lab::Guest::connect_invited_fragmented_session(
            fixture.keys[1].clone(),
            &unopened,
            lab::Limits::default()
        ))
        .await
        .is_err());
        assert!(
            fixture
                .manager
                .native_voice_host_observation("main", &fixture.id)
                .unwrap()
                .bound
        );
        assert!(fixture
            .manager
            .export_native_voice_invitation("main", &fixture.id, &fixture.participants[1])
            .is_err());
        fixture.finish(vec![guest]).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn elapsed_window_wins_when_real_result_and_timer_are_both_ready() {
    for profile in PROFILES {
        let gate = Arc::new(Gate::default());
        let called = gate.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let hooks = Arc::new(TestHooks {
            point: Some(Arc::new(move |point| {
                if point == ProducerPoint::VoiceBeforeFinalGate
                    && count.fetch_add(1, Ordering::AcqRel) + 1 == 5
                {
                    called.reach();
                }
            })),
            ..TestHooks::default()
        });
        let mut fixture = Fixture::prepared(profile, hooks, [(15, 18); 2]).await;
        let _release = Release(gate.clone());
        fixture.start().await;
        let guests = vec![fixture.guest(0).await, fixture.guest(1).await];
        for guest in &guests {
            fixture.tcp(guest, 53).await;
        }
        let entered = gate.clone();
        bounded(tokio::task::spawn_blocking(move || entered.reached()))
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::Acquire), 5);
        // This isolated runtime deliberately does not poll the actor until the
        // actual window has elapsed AND the original84 job has been released.
        std::thread::sleep(Duration::from_millis(2100));
        gate.release();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if let Ok(reserved) = fixture.binding.reserve_tcp() {
                drop(reserved);
                break;
            }
            assert!(Instant::now() < deadline, "native86_original_job_completed");
            std::thread::sleep(Duration::from_millis(1));
        }
        // execute has returned and dropped84; leave time for the same blocking
        // task to publish its JoinHandle result, without polling the actor.
        std::thread::sleep(Duration::from_millis(20));
        tokio::task::yield_now().await;
        fixture.retired().await;
        assert!(
            fixture
                .manager
                .native_voice_late_ready_for_test("main", &fixture.id)
                .unwrap(),
            "native86_real_task_and_deadline_ready_before_select"
        );
        for guest in &guests {
            fixture.tcp(guest, 59).await;
            assert!(matches!(
                bounded(guest.open_udp_fragmented("aux", 1)).await,
                Err(lab::Error::ServiceDenied)
            ));
        }
        fixture.finish(guests).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn udp_listener_loss_withdraws_only_udp_then_distinct_tcp_loss_closes_host() {
    use crate::engine::{connect_discovery::DiscoveryError, connect_owned_run::OwnedRunError};
    for profile in PROFILES {
        let mut fixture =
            Fixture::prepared(profile, Arc::new(TestHooks::default()), [(15, 18); 2]).await;
        fixture.start().await;
        let guests = vec![fixture.guest(0).await, fixture.guest(1).await];
        for guest in &guests {
            fixture.tcp(guest, 61).await;
        }
        fixture.harness.command(Action::DropUdp);
        assert!(matches!(
            fixture.binding.reserve_voice().unwrap().execute(),
            Err(producer::VoiceRuntimeError::Source(
                OwnedRunError::Discovery(DiscoveryError::ListenerChanged)
            ))
        ));
        fixture.retired().await;
        for guest in &guests {
            fixture.tcp(guest, 67).await;
        }
        fixture.harness.command(Action::BindUdp);
        fixture.harness.udp_bytes();
        assert!(fixture.binding.reserve_voice().unwrap().execute().is_ok());
        assert!(fixture
            .manager
            .native_voice_host_observation("main", &fixture.id)
            .unwrap()
            .udp_retired());
        for guest in &guests {
            assert!(matches!(
                bounded(guest.open_udp_fragmented("aux", 1)).await,
                Err(lab::Error::ServiceDenied)
            ));
        }
        fixture.harness.command(Action::DropTcp);
        assert!(matches!(
            fixture.binding.reserve_tcp().unwrap().execute(),
            Err(producer::DedicatedError::Source(OwnedRunError::Discovery(
                DiscoveryError::ListenerChanged
            )))
        ));
        bounded(async {
            loop {
                if let Ok(socket) = std::net::UdpSocket::bind(fixture.bind) {
                    drop(socket);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        // Listener loss is a transport finding. It never becomes authority to
        // terminate the still-owned native process or its independent UDP.
        assert!(fixture.binding.check_current().is_ok());
        fixture.harness.udp_bytes();
        for guest in &guests {
            bounded(guest.connection().closed()).await;
        }
        fixture.finish(guests).await;
    }
}
