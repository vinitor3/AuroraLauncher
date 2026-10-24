//! Real dedicated fixtures through the private86A0 binding and reservations.
//! No Host, invitation, Java, transport admission or replacement completion.
use super::producer_fixture::Harness as StatusHarness;
use super::voice_fixture::{VoiceFixture, PROFILES};
use super::voice_producer_fixture::{Action, Harness, WAIT};
use super::voice_runtime::{limits, prepare, ready, restored, Gate, Release, Running};
use super::*;
use crate::engine::connect_owned_run::{DedicatedTestHooks, OwnerTestGate};
use producer::{
    DedicatedError, DedicatedReason, FixtureMode, NativeVoiceHostBinding, ProducerPoint,
    ReservedTcpQuery, ReservedVoiceQuery, TestHooks, TestRecipe, VoiceRuntimeAnnouncement,
    VoiceRuntimeBinding, VoiceRuntimeError, VoiceRuntimeSample,
};
use std::sync::atomic::AtomicBool;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy)]
enum Query {
    Tcp,
    Voice,
}
enum ReservedQuery {
    Tcp(ReservedTcpQuery),
    Voice(ReservedVoiceQuery),
}
#[derive(Debug, PartialEq, Eq)]
enum ExecutedQuery {
    Tcp(std::result::Result<(), DedicatedError>),
    Voice(std::result::Result<VoiceRuntimeSample, VoiceRuntimeError>),
}
impl ReservedQuery {
    fn reserve(binding: &NativeVoiceHostBinding, kind: Query) -> Self {
        match kind {
            Query::Tcp => Self::Tcp(binding.reserve_tcp().expect("binding_reserve_tcp")),
            Query::Voice => Self::Voice(binding.reserve_voice().expect("binding_reserve_voice")),
        }
    }
    fn execute(self) -> ExecutedQuery {
        match self {
            Self::Tcp(query) => ExecutedQuery::Tcp(query.execute()),
            Self::Voice(query) => ExecutedQuery::Voice(query.execute()),
        }
    }
}
impl ExecutedQuery {
    fn require_closed(self) {
        match self {
            Self::Tcp(result) => assert_eq!(result, Err(DedicatedError::Closed)),
            Self::Voice(result) => assert_eq!(result, Err(VoiceRuntimeError::Closed)),
        }
    }
}
struct ReleaseOriginal(OwnerTestGate);
impl Drop for ReleaseOriginal {
    fn drop(&mut self) {
        self.0.release();
    }
}
struct PendingExecution {
    gate: Arc<Gate>,
    task: Option<std::thread::JoinHandle<ExecutedQuery>>,
}
impl PendingExecution {
    fn start(query: ReservedQuery, gate: Arc<Gate>) -> Self {
        Self {
            gate,
            task: Some(std::thread::spawn(move || query.execute())),
        }
    }
    fn finish(mut self) -> ExecutedQuery {
        self.gate.release();
        self.task
            .take()
            .unwrap()
            .join()
            .expect("binding_query_join")
    }
}
impl Drop for PendingExecution {
    fn drop(&mut self) {
        self.gate.release();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

#[tokio::test]
async fn native_binding_uses_real_tcp_and_voice_of_both_original_profiles() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let mut harness = Harness::new(fixture.stored_profile());
        let (plan, paths, _) = prepare(
            &fixture,
            &harness,
            Arc::new(TestHooks::default()),
            Hooks::default(),
            limits(),
        );
        let running = Running::start(plan, CancellationToken::new());
        harness.ready();
        ready(running.control()).await;
        let binding: NativeVoiceHostBinding = running
            .control()
            .native_voice_host_binding()
            .expect("binding_ready_v2");
        assert_eq!(binding.reserve_tcp().unwrap().execute(), Ok(()));
        let sample = binding.reserve_voice().unwrap().execute().unwrap();
        assert_eq!(sample.profile, profile);
        assert_eq!(sample.binding, VoiceRuntimeBinding::Ipv4Literal);
        assert_eq!(
            sample.announcement,
            VoiceRuntimeAnnouncement::DefaultUnchanged
        );
        assert_eq!(running.control().source().observe_tcp(), Ok(()));
        assert_eq!(
            running.control().voice_source().unwrap().observe(),
            Ok(sample)
        );

        // Construction checks only immutable origin/lifecycle state. It does
        // not substitute a new IO observation or cache a favorable sample.
        harness.command(Action::RewriteInvalid);
        let changed = running.control().native_voice_host_binding().unwrap();
        assert_eq!(
            changed.reserve_voice().unwrap().execute(),
            Err(VoiceRuntimeError::ConfigurationIncompatible)
        );
        assert_eq!(changed.reserve_tcp().unwrap().execute(), Ok(()));
        harness.command(Action::RestoreValid);
        assert_eq!(binding.reserve_voice().unwrap().execute(), Ok(sample));
        let report = running.finish().await;
        restored(&paths, &report);
        harness.ports_released();
        harness.finish();
    }
}

#[tokio::test]
async fn undispatched_tcp_and_voice_tokens_share_the_original_slot_and_drop_releases_it() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let mut harness = Harness::new(fixture.stored_profile());
        let (plan, paths, _) = prepare(
            &fixture,
            &harness,
            Arc::new(TestHooks::default()),
            Hooks::default(),
            limits(),
        );
        let running = Running::start(plan, CancellationToken::new());
        harness.ready();
        ready(running.control()).await;
        let binding = running.control().native_voice_host_binding().unwrap();
        let tcp = running.control().source();
        let voice = running.control().voice_source().unwrap();
        for query in [Query::Tcp, Query::Voice] {
            let reserved = ReservedQuery::reserve(&binding, query);
            // No worker has been created or polled: the real84 slot is held.
            assert_eq!(binding.reserve_tcp().err(), Some(DedicatedError::Busy));
            assert_eq!(binding.reserve_voice().err(), Some(VoiceRuntimeError::Busy));
            assert_eq!(tcp.observe_tcp(), Err(DedicatedError::Busy));
            assert_eq!(voice.observe(), Err(VoiceRuntimeError::Busy));
            assert_eq!(tcp.reserve_tcp().err(), Some(DedicatedError::Busy));
            assert_eq!(voice.reserve_voice().err(), Some(VoiceRuntimeError::Busy));
            // A caller unable to dispatch must dispose of the actual token.
            // This is not an injected OS thread-spawn failure or fake counter.
            drop(reserved);
            assert_eq!(tcp.reserve_tcp().unwrap().execute(), Ok(()));
            assert!(voice.reserve_voice().unwrap().execute().is_ok());
        }
        let report = running.finish().await;
        restored(&paths, &report);
        harness.ports_released();
        harness.finish();
    }
}

#[tokio::test]
async fn binding_constructor_refuses_real_starting_and_sealed_voice_runs() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let mut harness = Harness::new(fixture.stored_profile());
        let gate = Arc::new(Gate::default());
        let called = gate.clone();
        let hooks = Arc::new(TestHooks {
            point: Some(Arc::new(move |point| {
                if point == ProducerPoint::BeforeReady {
                    called.reach();
                }
            })),
            ..TestHooks::default()
        });
        let (plan, paths, _) = prepare(&fixture, &harness, hooks, Hooks::default(), limits());
        let running = Running::start(plan, CancellationToken::new());
        let _release = Release(gate.clone());
        harness.ready();
        gate.reached();
        assert_eq!(
            running.control().native_voice_host_binding().err(),
            Some(DedicatedError::NotReady)
        );
        gate.release();
        ready(running.control()).await;
        let binding = running.control().native_voice_host_binding().unwrap();
        assert!(binding.reserve_voice().unwrap().execute().is_ok());
        running.control().request_stop();
        assert_eq!(
            running.control().native_voice_host_binding().err(),
            Some(DedicatedError::Closed)
        );
        assert_eq!(binding.reserve_tcp().err(), Some(DedicatedError::Closed));
        assert_eq!(
            binding.reserve_voice().err(),
            Some(VoiceRuntimeError::Closed)
        );
        let report = running.finish().await;
        restored(&paths, &report);
        harness.ports_released();
        harness.finish();
    }
}

#[tokio::test]
async fn ready_status_v1_cannot_construct_a_native_voice_binding() {
    let fixture = Fixture::new();
    let mut harness = StatusHarness::new();
    let prepared = fixture.prepared(Hooks::default());
    let paths = prepared.test_paths();
    let plan = producer::plan_for_test(
        prepared,
        TestRecipe {
            control: harness.control_addr(),
            nonce: Uuid::parse_str(harness.nonce()).unwrap(),
            mode: FixtureMode::Normal,
        },
        limits(),
        Arc::new(TestHooks::default()),
    )
    .unwrap();
    let running = Running::start(plan, CancellationToken::new());
    harness.ready();
    ready(running.control()).await;
    assert_eq!(running.control().source().observe_tcp(), Ok(()));
    assert_eq!(
        running.control().native_voice_host_binding().err(),
        Some(DedicatedError::Preparation(Error::Identity))
    );
    let report = running.finish().await;
    restored(&paths, &report);
    StatusHarness::assert_status_port_released();
    harness.finish();
}

#[tokio::test]
async fn stop_reaps_before_undispatched_tokens_finish_but_original_completion_and_eula_wait() {
    for profile in PROFILES {
        for kind in [Query::Tcp, Query::Voice] {
            for execute_after_stop in [false, true] {
                let fixture = VoiceFixture::new(profile);
                let mut harness = Harness::new(fixture.stored_profile());
                let original_gate = OwnerTestGate::new();
                let hooks = Arc::new(TestHooks {
                    owned: DedicatedTestHooks::new(false, None, Some(original_gate.clone())),
                    ..TestHooks::default()
                });
                let (plan, paths, _) =
                    prepare(&fixture, &harness, hooks, Hooks::default(), limits());
                let running = Running::start(plan, CancellationToken::new());
                let _release_original = ReleaseOriginal(original_gate.clone());
                harness.ready();
                ready(running.control()).await;
                let binding = running.control().native_voice_host_binding().unwrap();
                let reserved = ReservedQuery::reserve(&binding, kind);
                running.control().request_stop();
                harness.stop_observed();
                assert!(
                    original_gate.wait_reached(WAIT),
                    "binding_original_reap_and_readers_before_dispatch"
                );
                harness.ports_released();
                original_gate.release();

                let completion = running.completion.clone();
                let (entered, started) = tokio::sync::oneshot::channel();
                let lost = tokio::spawn(async move {
                    let _ = entered.send(());
                    completion.wait().await
                });
                tokio::time::timeout(WAIT, started).await.unwrap().unwrap();
                lost.abort();
                assert!(lost.await.unwrap_err().is_cancelled());
                assert!(
                    tokio::time::timeout(Duration::from_millis(40), running.completion.wait())
                        .await
                        .is_err()
                );
                assert_eq!(bytes(&target(&paths)), EULA_TRUE);
                locked(&paths);
                assert!(!slot(&paths, "restored.json").exists());
                assert_eq!(binding.reserve_tcp().err(), Some(DedicatedError::Closed));
                assert_eq!(
                    binding.reserve_voice().err(),
                    Some(VoiceRuntimeError::Closed)
                );
                if execute_after_stop {
                    reserved.execute().require_closed();
                } else {
                    drop(reserved);
                }
                let report = running.finish().await;
                assert_eq!(report.reason, DedicatedReason::Requested);
                restored(&paths, &report);
                harness.ports_released();
                harness.finish();
            }
        }
    }
}

#[tokio::test]
async fn reserved_execution_keeps_final_gate_for_positive_tcp_voice_and_voice_errors() {
    for profile in PROFILES {
        for (kind, invalid_configuration) in [
            (Query::Tcp, false),
            (Query::Voice, false),
            (Query::Voice, true),
        ] {
            let fixture = VoiceFixture::new(profile);
            let mut harness = Harness::new(fixture.stored_profile());
            let gate = Arc::new(Gate::default());
            let called = gate.clone();
            let armed = Arc::new(AtomicBool::new(false));
            let in_hook = armed.clone();
            let point = match kind {
                Query::Tcp => ProducerPoint::AfterQuery,
                Query::Voice => ProducerPoint::VoiceBeforeFinalGate,
            };
            let hooks = Arc::new(TestHooks {
                point: Some(Arc::new(move |actual| {
                    if actual == point && in_hook.load(Ordering::Acquire) {
                        called.reach();
                    }
                })),
                ..TestHooks::default()
            });
            let (plan, paths, _) = prepare(&fixture, &harness, hooks, Hooks::default(), limits());
            let running = Running::start(plan, CancellationToken::new());
            let _release = Release(gate.clone());
            harness.ready();
            ready(running.control()).await;
            let binding = running.control().native_voice_host_binding().unwrap();
            if invalid_configuration {
                harness.command(Action::RewriteInvalid);
                // A real unsealed query establishes the error independently
                // before testing that the final seal overrides that result.
                assert_eq!(
                    binding.reserve_voice().unwrap().execute(),
                    Err(VoiceRuntimeError::ConfigurationIncompatible)
                );
            }
            let reserved = ReservedQuery::reserve(&binding, kind);
            armed.store(true, Ordering::Release);
            let pending = PendingExecution::start(reserved, gate.clone());
            gate.reached();
            running.control().request_stop();
            harness.stop_observed();
            assert!(
                tokio::time::timeout(Duration::from_millis(40), running.completion.wait())
                    .await
                    .is_err()
            );
            assert_eq!(bytes(&target(&paths)), EULA_TRUE);
            pending.finish().require_closed();
            let report = running.finish().await;
            restored(&paths, &report);
            harness.ports_released();
            harness.finish();
        }
    }
}

#[tokio::test]
async fn weak_binding_and_clones_cannot_retain_or_adopt_a_replacement_run() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let mut harness = Harness::new(fixture.stored_profile());
        let (plan, paths, _) = prepare(
            &fixture,
            &harness,
            Arc::new(TestHooks::default()),
            Hooks::default(),
            limits(),
        );
        let running = Running::start(plan, CancellationToken::new());
        harness.ready();
        ready(running.control()).await;
        let observer = running.control().observer();
        let old = running.control().native_voice_host_binding().unwrap();
        let retained = old.clone();
        assert!(old.reserve_voice().unwrap().execute().is_ok());
        let report = running.finish().await;
        restored(&paths, &report);
        harness.ports_released();
        harness.finish();
        let deadline = Instant::now() + WAIT;
        while observer.snapshot().is_some() {
            assert!(Instant::now() < deadline, "binding_must_not_retain_shared");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        for replacement_profile in PROFILES {
            let replacement = VoiceFixture::new(replacement_profile);
            let mut harness = Harness::new(replacement.stored_profile());
            let (plan, paths, _) = prepare(
                &replacement,
                &harness,
                Arc::new(TestHooks::default()),
                Hooks::default(),
                limits(),
            );
            let running = Running::start(plan, CancellationToken::new());
            harness.ready();
            ready(running.control()).await;
            let new = running.control().native_voice_host_binding().unwrap();
            assert_eq!(new.reserve_tcp().unwrap().execute(), Ok(()));
            assert_eq!(
                new.reserve_voice().unwrap().execute().unwrap().profile,
                replacement_profile
            );
            for original in [&old, &retained] {
                assert_eq!(original.reserve_tcp().err(), Some(DedicatedError::Closed));
                assert_eq!(
                    original.reserve_voice().err(),
                    Some(VoiceRuntimeError::Closed)
                );
            }
            let report = running.finish().await;
            restored(&paths, &report);
            harness.ports_released();
            harness.finish();
        }
    }
}
