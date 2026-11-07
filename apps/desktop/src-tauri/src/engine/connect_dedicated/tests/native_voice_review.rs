//! Private native86A1 consumer over real producer/OS observations. No Host or
//! audio claim: the transport address stays bound by this test throughout.
use super::voice_fixture::{VoiceFixture, PROFILES};
use super::voice_producer_fixture::{Action, Harness, WAIT};
use super::voice_runtime::{limits, prepare, ready, restored, Gate, Release, Running};
use super::*;
use crate::engine::connect::{ConnectError, ConnectManager, GroupParticipantRequest};
use producer::{DedicatedError, ProducerPoint, TestHooks, VoiceRuntimeError};
use std::sync::atomic::AtomicBool;
use tokio_util::sync::CancellationToken;

fn roster() -> Vec<GroupParticipantRequest> {
    (0..2)
        .map(|_| GroupParticipantRequest {
            guest_public_id: aurora_connect_lab::generate_key().public().to_string(),
            admission_seconds: 10,
            session_seconds: 12,
        })
        .collect()
}
fn occupied_transport() -> std::net::UdpSocket {
    std::net::UdpSocket::bind("127.0.0.1:0").unwrap()
}
async fn work_finished(manager: &ConnectManager) {
    let deadline = Instant::now() + WAIT;
    while !manager.native_voice_work_finished_for_test() {
        assert!(
            Instant::now() < deadline,
            "native86_consumer_original_worker"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn both_profiles_prepare_and_review_original_roster_without_binding_transport() {
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
        let manager = ConnectManager::default();
        let binding = running.control().native_voice_host_binding().unwrap();
        let transport = occupied_transport();
        let bind = transport.local_addr().unwrap();
        let roster = roster();
        let expected: Vec<_> = roster.iter().map(|r| r.guest_public_id.clone()).collect();
        let prepared = manager
            .prepare_native_voice_group("main", binding.clone(), bind, roster)
            .unwrap()
            .wait()
            .await
            .unwrap();
        work_finished(&manager).await;
        assert_eq!(manager.snapshot("main").unwrap().phase, "nativePrepared");
        assert!(manager.native_voice_originals_for_test().is_none());
        assert_eq!(
            manager
                .review_native_voice_group("other", &prepared.operation_id, prepared.generation)
                .err()
                .unwrap()
                .code(),
            ConnectError::OwnerMismatch.code()
        );
        assert_eq!(
            manager
                .review_native_voice_group("main", &prepared.operation_id, prepared.generation + 1)
                .err()
                .unwrap()
                .code(),
            ConnectError::StaleOperation.code()
        );
        let review = manager
            .review_native_voice_group("main", &prepared.operation_id, prepared.generation)
            .unwrap()
            .wait()
            .await
            .unwrap();
        work_finished(&manager).await;
        assert_eq!(review.data.operation_id, prepared.operation_id);
        assert_eq!(review.data.host_public_id, prepared.host_public_id);
        assert_eq!(review.data.direct_address, bind.to_string());
        assert_eq!(review.data.game_target, "127.0.0.1:43001");
        assert_eq!(review.data.udp_target, binding.targets().1.to_string());
        assert_eq!(review.profile, binding.identity_preimage().1);
        assert_eq!(review.original_copy_id, binding.copy_id().unwrap());
        assert_eq!(
            (review.minecraft, review.loader, review.loader_version),
            ("1.21.1", "fabric", "0.16.14")
        );
        assert_eq!(
            review.artifact_version,
            if review.profile == "svc" {
                "2.6.22"
            } else {
                "2.1.17"
            }
        );
        assert_eq!(review.binding_policy, "ipv4Literal");
        assert_eq!(review.data.services.len(), 2);
        let game = &review.data.services[0];
        assert_eq!(
            (
                game.service_id.as_str(),
                game.protocol,
                game.revision,
                game.required_capability
            ),
            (
                "game",
                aurora_connect_lab::Protocol::Tcp,
                1,
                aurora_connect_lab::Capability::ReliableStream
            )
        );
        assert_eq!(
            (
                game.max_datagram_bytes,
                game.return_policy,
                game.return_bytes_per_second,
                game.return_frames_per_second
            ),
            (1400, "replyBudget", 0, 0)
        );
        let aux = &review.data.services[1];
        assert_eq!(
            (
                aux.service_id.as_str(),
                aux.protocol,
                aux.revision,
                aux.required_capability
            ),
            (
                "aux",
                aurora_connect_lab::Protocol::Udp,
                1,
                aurora_connect_lab::Capability::FragmentedDatagram
            )
        );
        assert_eq!(
            (
                aux.max_datagram_bytes,
                aux.return_policy,
                aux.return_bytes_per_second,
                aux.return_frames_per_second
            ),
            (4096, "authorizedDownstream", 16384, 20)
        );
        assert!(review.withdrawal.contains("audio is unverified"));
        assert!(review.data.expires_at_ms <= prepared.expires_at_ms);
        let (retained, invitations) = manager.native_voice_originals_for_test().unwrap();
        assert_eq!(retained.digest, review.digest);
        let envelopes: Vec<_> = invitations.iter().map(|i| i.encode()).collect();
        for ((participant, invitation), recipient) in review
            .data
            .participants
            .iter()
            .zip(&invitations)
            .zip(expected)
        {
            assert_eq!(participant.guest_public_id, recipient);
            assert_eq!(participant.invitation_digest, invitation.digest());
            assert_eq!(
                participant.admission_expires_at_ms,
                invitation.admission_expires_at_unix_ms()
            );
            assert_eq!(
                participant.session_expires_at_ms,
                invitation.session_expires_at_unix_ms()
            );
        }
        assert!(manager
            .review_native_voice_group("main", &prepared.operation_id, review.data.generation)
            .is_err());
        for _ in 0..3 {
            assert_eq!(manager.snapshot("main").unwrap().phase, "nativeReviewed");
        }
        assert_eq!(
            manager
                .native_voice_originals_for_test()
                .unwrap()
                .1
                .iter()
                .map(|i| i.encode())
                .collect::<Vec<_>>(),
            envelopes
        );
        // Existing manual/owned confirmation cannot consume a native review.
        assert!(manager
            .confirm_session_group_host("main", &prepared.operation_id, &review.digest)
            .await
            .is_err());
        assert!(manager
            .confirm("main", &prepared.operation_id, &review.digest)
            .await
            .is_err());
        let stopped = manager.stop("main", &prepared.operation_id).await.unwrap();
        assert_eq!(stopped.cleanup_state, "complete");
        assert!(manager.native_voice_originals_for_test().is_none());
        assert!(binding.reserve_voice().unwrap().execute().is_ok());
        assert_eq!(bytes(&target(&paths)), EULA_TRUE);
        drop(transport);
        let report = running.finish().await;
        restored(&paths, &report);
        harness.ports_released();
        harness.finish();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_configuration_change_rejects_review_and_preserves_native_tcp() {
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
        let manager = ConnectManager::default();
        let binding = running.control().native_voice_host_binding().unwrap();
        let transport = occupied_transport();
        let prepared = manager
            .prepare_native_voice_group(
                "main",
                binding.clone(),
                transport.local_addr().unwrap(),
                roster(),
            )
            .unwrap()
            .wait()
            .await
            .unwrap();
        work_finished(&manager).await;
        harness.command(Action::RewriteInvalid);
        let error = manager
            .review_native_voice_group("main", &prepared.operation_id, prepared.generation)
            .unwrap()
            .wait()
            .await
            .unwrap_err();
        assert_eq!(error.code(), "native_voice_configuration_rejected");
        assert!(manager.native_voice_originals_for_test().is_none());
        assert_eq!(binding.reserve_tcp().unwrap().execute(), Ok(()));
        assert_eq!(
            manager
                .stop("main", &prepared.operation_id)
                .await
                .unwrap()
                .cleanup_state,
            "complete"
        );
        let report = running.finish().await;
        restored(&paths, &report);
        harness.ports_released();
        harness.finish();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn held_real_query_times_out_without_releasing_job_or_late_review() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let mut harness = Harness::new(fixture.stored_profile());
        let gate = Arc::new(Gate::default());
        let armed = Arc::new(AtomicBool::new(false));
        let called = gate.clone();
        let active = armed.clone();
        let hooks = Arc::new(TestHooks {
            point: Some(Arc::new(move |point| {
                if point == ProducerPoint::VoiceBeforeFinalGate && active.load(Ordering::Acquire) {
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
        let manager = Arc::new(ConnectManager::default());
        let binding = running.control().native_voice_host_binding().unwrap();
        let transport = occupied_transport();
        let prepared = manager
            .prepare_native_voice_group(
                "main",
                binding.clone(),
                transport.local_addr().unwrap(),
                roster(),
            )
            .unwrap()
            .wait()
            .await
            .unwrap();
        work_finished(&manager).await;
        armed.store(true, Ordering::Release);
        let pending = manager
            .review_native_voice_group("main", &prepared.operation_id, prepared.generation)
            .unwrap();
        gate.reached();
        assert_eq!(binding.reserve_tcp().err(), Some(DedicatedError::Busy));
        let error = tokio::time::timeout(Duration::from_secs(3), pending.wait())
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code(), "native_voice_observation_window_elapsed");
        assert_eq!(binding.reserve_voice().err(), Some(VoiceRuntimeError::Busy));
        assert_eq!(manager.snapshot("main").unwrap().cleanup_state, "pending");
        assert!(!manager.native_voice_work_finished_for_test());
        let stopping = {
            let manager = manager.clone();
            let id = prepared.operation_id.clone();
            tokio::spawn(async move { manager.stop("main", &id).await })
        };
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(!stopping.is_finished());
        stopping.abort();
        assert!(stopping.await.unwrap_err().is_cancelled());
        assert!(manager
            .prepare_native_voice_group(
                "main",
                binding.clone(),
                transport.local_addr().unwrap(),
                roster()
            )
            .is_err());
        gate.release();
        assert_eq!(
            manager
                .stop("main", &prepared.operation_id)
                .await
                .unwrap()
                .cleanup_state,
            "complete"
        );
        assert!(manager.native_voice_originals_for_test().is_none());
        assert!(binding.reserve_voice().unwrap().execute().is_ok());
        let report = running.finish().await;
        restored(&paths, &report);
        harness.ports_released();
        harness.finish();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn losing_prepare_waiter_and_stopping_keeps_same_query_under_cleanup() {
    for profile in PROFILES {
        let fixture = VoiceFixture::new(profile);
        let mut harness = Harness::new(fixture.stored_profile());
        let gate = Arc::new(Gate::default());
        let called = gate.clone();
        let hooks = Arc::new(TestHooks {
            point: Some(Arc::new(move |point| {
                if point == ProducerPoint::VoiceBeforeFinalGate {
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
        let manager = Arc::new(ConnectManager::default());
        let binding = running.control().native_voice_host_binding().unwrap();
        let transport = occupied_transport();
        let pending = manager
            .prepare_native_voice_group(
                "main",
                binding.clone(),
                transport.local_addr().unwrap(),
                roster(),
            )
            .unwrap();
        let id = pending.operation_id.clone();
        drop(pending);
        gate.reached();
        let mut stop = {
            let manager = manager.clone();
            let id = id.clone();
            tokio::spawn(async move { manager.stop("main", &id).await })
        };
        assert!(tokio::time::timeout(Duration::from_millis(40), &mut stop)
            .await
            .is_err());
        assert_eq!(binding.reserve_tcp().err(), Some(DedicatedError::Busy));
        assert_eq!(manager.snapshot("main").unwrap().cleanup_state, "pending");
        gate.release();
        assert_eq!(stop.await.unwrap().unwrap().cleanup_state, "complete");
        assert!(manager.native_voice_originals_for_test().is_none());
        assert_eq!(binding.reserve_tcp().unwrap().execute(), Ok(()));
        let report = running.finish().await;
        restored(&paths, &report);
        harness.ports_released();
        harness.finish();
    }
}

#[test]
fn original_job_is_reserved_before_blocking_dispatch_and_holds_completion_while_queued() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
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
            let gate = Arc::new(Gate::default());
            let _release = Release(gate.clone());
            harness.ready();
            ready(running.control()).await;
            let blocker = {
                let gate = gate.clone();
                tokio::task::spawn_blocking(move || gate.reach())
            };
            gate.reached();
            let manager = Arc::new(ConnectManager::default());
            let binding = running.control().native_voice_host_binding().unwrap();
            let transport = occupied_transport();
            let pending = manager
                .prepare_native_voice_group(
                    "main",
                    binding.clone(),
                    transport.local_addr().unwrap(),
                    roster(),
                )
                .unwrap();
            let id = pending.operation_id.clone();
            // No await: neither actor scheduling nor executing the blocking
            // closure was necessary to reserve the real original84 slot.
            assert_eq!(binding.reserve_tcp().err(), Some(DedicatedError::Busy));
            let deadline = Instant::now() + WAIT;
            while !manager.native_voice_query_dispatched_for_test() {
                assert!(Instant::now() < deadline, "native86_actual_dispatch");
                tokio::task::yield_now().await;
            }
            drop(pending);
            running.control().request_stop();
            harness.stop_observed();
            assert!(
                tokio::time::timeout(Duration::from_millis(60), running.completion.wait())
                    .await
                    .is_err()
            );
            assert_eq!(bytes(&target(&paths)), EULA_TRUE);
            assert!(!slot(&paths, "restored.json").exists());
            let mut stop = {
                let manager = manager.clone();
                let id = id.clone();
                tokio::spawn(async move { manager.stop("main", &id).await })
            };
            assert!(tokio::time::timeout(Duration::from_millis(40), &mut stop)
                .await
                .is_err());
            assert_eq!(manager.snapshot("main").unwrap().cleanup_state, "pending");
            gate.release();
            blocker.await.unwrap();
            assert_eq!(stop.await.unwrap().unwrap().cleanup_state, "complete");
            assert!(manager.native_voice_originals_for_test().is_none());
            let report = running.finish().await;
            restored(&paths, &report);
            harness.ports_released();
            harness.finish();
        }
    });
}
