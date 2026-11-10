//! Native policy2 source + real online Host/local player + ReadyGame child.
use super::voice_fixture::{VoiceFixture, PROFILES};
use super::voice_producer_fixture::Harness;
use super::voice_runtime::{limits, ready, restored, Gate, Release, Running};
use super::*;
use crate::engine::connect::ConnectManager;
use producer::TestHooks;
use std::sync::atomic::AtomicUsize;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_voice_social_host_player_original_source_spawn_expiry_epoch_and_cleanup() {
    for profile in PROFILES {
        for mode in 0..6 {
            let fixture = VoiceFixture::new(profile);
            let plan = factory::fixture_voice_bridge_plan(
                &fixture.fixture.lab,
                &fixture.fixture.stage,
                &fixture.fixture.jdk,
                profile,
            )
            .unwrap();
            exercise(plan, profile, mode, false, false).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit real compiled JAR/JDK copy plus real synthetic child, online relay HostPlayer, ReadyGame spawn and durable recovery; no Java/audio"]
async fn native_voice_social_host_player_real_resources_and_durable_recovery() {
    for profile in PROFILES {
        let selection = match profile {
            NativeVoiceProfile::SimpleVoiceChat => {
                crate::engine::connect_native_voice::NativeVoiceProfile::SimpleVoiceChat
            }
            NativeVoiceProfile::PlasmoVoice => {
                crate::engine::connect_native_voice::NativeVoiceProfile::PlasmoVoice
            }
        };
        let plan = desktop::discover_plan(selection).expect("native_voice_real_compiled_resources");
        exercise(plan, profile, 0, true, false).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn social_private_online_native_voice_shared_slots_expiry_removal_and_recovery() {
    for profile in PROFILES {
        for mode in [4, 5] {
            let fixture = VoiceFixture::new(profile);
            let plan = factory::fixture_voice_bridge_plan(
                &fixture.fixture.lab,
                &fixture.fixture.stage,
                &fixture.fixture.jdk,
                profile,
            )
            .unwrap();
            exercise(plan, profile, mode, false, true).await;
        }
    }
}

async fn exercise(
    plan: FactoryPlan,
    profile: NativeVoiceProfile,
    mode: u8,
    real_resources: bool,
    private: bool,
) {
    eprintln!("native_voice_social_case {profile:?} mode={mode} begin");
    let approval = plan.review().digest.clone();
    let mut prepared = plan.create(&approval).unwrap();
    let (copy, receipt) = prepared.announcement_recovery.take().unwrap();
    let paths = prepared.test_paths();
    let mut harness = Harness::new(profile.stored());
    let mut runtime_limits = limits();
    runtime_limits.lifetime_ms = 24_000;
    // Real JARs exercise every immutable hashing/recovery gate. Their original
    // startup budget is explicit, bounded and never granted back after use.
    if real_resources {
        runtime_limits.lifetime_ms = 180_000;
        runtime_limits.readiness_ms = 120_000;
    }
    let worker_started = Instant::now();
    let query_gate = Arc::new(Gate::default());
    let query_count = Arc::new(AtomicUsize::new(0));
    let held_gate = query_gate.clone();
    let hooks = Arc::new(TestHooks {
        point: Some(Arc::new(move |point| {
            if real_resources
                && matches!(
                    point,
                    producer::ProducerPoint::BeforeApply
                        | producer::ProducerPoint::AfterApply
                        | producer::ProducerPoint::BeforeStart
                        | producer::ProducerPoint::AfterStart
                )
            {
                eprintln!(
                    "native_voice_real_resources {point:?} elapsed_ms={}",
                    worker_started.elapsed().as_millis()
                );
            }
            if mode >= 4
                && point == producer::ProducerPoint::VoiceAfterConfigurationA
                && query_count.fetch_add(1, Ordering::SeqCst) == 2
            {
                held_gate.reach();
            }
        })),
        ..TestHooks::default()
    });
    let plan = producer::plan_voice_for_test(
        prepared,
        harness.control_addr(),
        harness.nonce(),
        false,
        runtime_limits,
        hooks,
    )
    .unwrap();
    let running = Running::start(plan, CancellationToken::new());
    let release_query = Release(query_gate.clone());
    if real_resources {
        harness.ready_with_observation(Duration::from_secs(120), || {
            let snapshot = running
                .control()
                .observer()
                .snapshot()
                .expect("native_voice_real_original_observer");
            assert!(
                snapshot.reason.is_none(),
                "native_voice_real_before_child_ready:{snapshot:?}"
            );
        });
    } else {
        harness.ready();
    }
    ready(running.control()).await;
    let binding = running.control().native_voice_host_binding().unwrap();
    ConnectManager::exercise_social_host_player_test(binding, mode, private, || {
        if mode >= 4 {
            query_gate.reached();
            query_gate.release();
        } else {
            let _ = running.control().request_stop();
        }
    })
    .await;
    drop(release_query);
    let report = if real_resources {
        running.finish_with_timeout(Duration::from_secs(120)).await
    } else {
        running.finish().await
    };
    harness.stop_observed();
    harness.finish();
    restored(&paths, &report);
    // Recovery is reloaded from disk after the actual process and Host complete.
    let receipt_id = receipt.receipt_id().to_owned();
    drop(receipt);
    let guard = crate::engine::connect_voice::acquire_voice_mutation_guard(&copy).unwrap();
    let mut receipt =
        crate::engine::connect_voice::resume_restoration(&copy, &guard, &receipt_id).unwrap();
    let config = copy.root().join(profile.stored().configuration_name());
    let current = digest(&fs::read(&config).unwrap());
    let review = receipt.prepare_reconciled(&copy, &guard, &current).unwrap();
    receipt.confirm(&copy, &guard, &review.digest).unwrap();
    assert_eq!(
        receipt.state(),
        crate::engine::connect_voice::RestorationState::Restored
    );
    let mut expected = profile.stored().configuration().to_vec();
    if profile == NativeVoiceProfile::PlasmoVoice {
        // Reconciliation removes only the owned TOML code spans. It retains
        // every newline and the two separator spaces from each added key;
        // the fixture's later comment is unrelated and must survive verbatim.
        expected.extend_from_slice(b"\n  \n  \n");
    }
    expected.extend_from_slice(b"# generated by the own synthetic voice fixture\n");
    assert_eq!(fs::read(config).unwrap(), expected);
    eprintln!("native_voice_social_case {profile:?} mode={mode} complete");
}
