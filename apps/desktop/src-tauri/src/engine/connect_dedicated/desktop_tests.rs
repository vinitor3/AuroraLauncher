//! The desktop projection cannot renew an approval or turn failed work into a
//! live source. The ignored case exercises both physical recovery transactions.
use super::*;
use crate::engine::connect_native_voice::NativeVoiceProfile as UiVoiceProfile;
use crate::engine::connect_voice::dedicated::tests::files::TestRoot;

fn actor(lab: PathBuf) -> Actor {
    Actor::new(
        ConnectManager::default(),
        lab,
        Arc::new(Mutex::new(NativeVoiceSnapshot::default())),
        Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)),
        Arc::new(Mutex::new(None)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)),
    )
}

#[test]
fn native_voice_recovery_inventory_tracks_only_confirmed_bundle_and_receipt_in_both_orders() {
    for eula_first in [true, false] {
        let mut actor = actor(PathBuf::new());
        let bundle = BundleId(Uuid::new_v4());
        let other_bundle = BundleId(Uuid::new_v4());
        let candidate = |bundle: BundleId, receipt: &str| NativeVoiceRecoveryCandidate {
            bundle_id: bundle.0.to_string(),
            receipt_id: receipt.into(),
            profile: UiVoiceProfile::SimpleVoiceChat,
            runtime_profile: NativeVoiceRuntime::Fabric1211,
            eula_recovery_pending: true,
            announcement_recovery_pending: true,
        };
        actor.view.recoverable_copies = vec![
            candidate(bundle, "selected-receipt"),
            candidate(bundle, "other-receipt"),
            candidate(other_bundle, "selected-receipt"),
        ];
        for eula in [eula_first, !eula_first] {
            if eula {
                actor.reconcile_recovery_candidates(bundle, Some(false), None);
            } else {
                actor.reconcile_recovery_candidates(
                    bundle,
                    None,
                    Some(("selected-receipt", false)),
                );
            }
            let selected = actor.view.recoverable_copies.iter().find(|entry| {
                entry.bundle_id == bundle.0.to_string() && entry.receipt_id == "selected-receipt"
            });
            if eula == eula_first {
                let selected = selected.unwrap();
                assert_eq!(selected.eula_recovery_pending, !eula);
                assert_eq!(selected.announcement_recovery_pending, eula);
            } else {
                assert!(
                    selected.is_none(),
                    "no Resume action for a fully restored receipt"
                );
            }
            let other = actor
                .view
                .recoverable_copies
                .iter()
                .find(|entry| entry.receipt_id == "other-receipt")
                .unwrap();
            assert!(
                other.announcement_recovery_pending,
                "another receipt is never silently retired"
            );
            let foreign = actor
                .view
                .recoverable_copies
                .iter()
                .find(|entry| entry.bundle_id == other_bundle.0.to_string())
                .unwrap();
            assert!(foreign.eula_recovery_pending && foreign.announcement_recovery_pending);
        }
        actor.publish();
        let dto = actor.shared.lock().unwrap().clone().projected();
        let serialized = serde_json::to_value(dto).unwrap();
        let candidates = serialized["recoverableCopies"].as_array().unwrap();
        assert_eq!(candidates.len(), 2);
        assert!(candidates
            .iter()
            .all(|entry| entry["eulaRecoveryPending"] == true
                || entry["announcementRecoveryPending"] == true));
    }
}

pub(in crate::engine::connect_voice::dedicated) async fn exercise_source_revalidation(
    run: DedicatedRun,
    lab: PathBuf,
) -> DedicatedRun {
    let binding = run.native_voice_host_binding().unwrap();
    let old = binding.clone();
    let (connect, receipt) = ConnectManager::test_completed_native_source(binding.clone()).await;
    let mut actor = actor(lab);
    actor.connect = connect;
    actor.source = Some(run);
    *actor.shared_source.lock().unwrap() = Some(binding);
    *actor.source_receipt.lock().unwrap() = Some(receipt);
    actor.claimed.store(true, Ordering::Release);
    actor.view.phase = "socialSource";
    actor.view.eula_state = "ownedBySource";
    let cancel = CancellationToken::new();
    actor.maintain().await;
    assert!(actor.view.social_revalidation_available);
    actor
        .handle(NativeVoiceRequest::ReviewSocialSource {}, &cancel)
        .await
        .unwrap();
    assert_eq!(actor.view.review.as_ref().unwrap().kind, "socialSource");
    assert!(
        old.check_current().is_ok(),
        "review does not rotate a claim"
    );
    actor
        .handle(NativeVoiceRequest::CancelSocialSourceReview {}, &cancel)
        .await
        .unwrap();
    assert!(
        old.check_current().is_ok(),
        "cancel preserves the same source"
    );
    actor
        .handle(NativeVoiceRequest::ReviewSocialSource {}, &cancel)
        .await
        .unwrap();
    actor.view.native_review_deadline = Some(Instant::now());
    let stale = actor.view.review.as_ref().unwrap().digest.clone();
    assert!(actor
        .handle(
            NativeVoiceRequest::ApproveSocialSource { digest: stale },
            &cancel
        )
        .await
        .is_err());
    actor.settle_failure().await;
    assert_eq!(actor.view.phase, "socialSource");
    assert!(old.check_current().is_ok());
    actor
        .handle(NativeVoiceRequest::ReviewSocialSource {}, &cancel)
        .await
        .unwrap();
    let digest = actor.view.review.as_ref().unwrap().digest.clone();
    actor
        .handle(
            NativeVoiceRequest::ApproveSocialSource {
                digest: digest.clone(),
            },
            &cancel,
        )
        .await
        .unwrap();
    assert_eq!(actor.view.phase, "ready");
    assert_eq!(actor.view.eula_state, "ownedBySource");
    assert!(!actor.claimed.load(Ordering::Acquire));
    let renewed = actor.shared_source.lock().unwrap().clone().unwrap();
    assert!(old.check_current().is_err());
    assert!(old.with_current(|| ()).is_err());
    assert!(!old.same_claim(&renewed));
    assert_eq!(old.deadline(), renewed.deadline());
    assert_eq!(old.identity_preimage(), renewed.identity_preimage());
    assert!(renewed.check_current().is_ok());
    assert!(actor
        .handle(NativeVoiceRequest::ApproveSocialSource { digest }, &cancel)
        .await
        .is_err());
    assert!(
        renewed.check_current().is_ok(),
        "replay cannot rotate the new claim"
    );
    actor.source.take().unwrap()
}

pub(in crate::engine::connect_voice::dedicated) async fn assert_actor_collects_forced_stop(
    run: DedicatedRun,
    lab: PathBuf,
) {
    let mut actor = actor(lab);
    let (connect, operation) = ConnectManager::test_failed_social_cleanup();
    actor.connect = connect;
    actor.view.operation_id = Some(operation);
    actor.source = Some(run);
    actor.view.phase = "bound";
    actor.view.udp_state = "enabled";
    actor.view.eula_state = "ownedBySource";
    let cancel = CancellationToken::new();
    // Exercise the actual actor path with the fixture's original process. A
    // repeated Stop after maintenance collection must keep the warning.
    actor.stop.store(true, Ordering::Release);
    actor.maintain().await;
    assert_eq!(actor.view.error_code, Some(FORCED_STOP_CLEANUP_FAILED));
    assert_eq!(
        actor.handle(NativeVoiceRequest::Stop {}, &cancel).await,
        Err(FORCED_STOP_CLEANUP_FAILED),
    );
    actor.maintain().await;
    actor.publish();
    let snapshot = actor.shared.lock().unwrap().clone().projected();
    assert!(actor.source.is_none());
    assert_eq!(snapshot.phase, "failed");
    assert_eq!(snapshot.udp_state, "cleanupFailed");
    assert_eq!(snapshot.error_code, Some(FORCED_STOP_CLEANUP_FAILED));
    assert_eq!(snapshot.eula_state, "restored");
    assert!(!snapshot.eula_recovery_available);
    assert_eq!(snapshot.audio_state, "notVerified");
}

#[tokio::test]
async fn native_voice_actor_stop_rejects_failed_connect_cleanup_snapshot() {
    let root = TestRoot::new();
    let mut actor = actor(root.path().into());
    let (connect, operation) = ConnectManager::test_failed_social_cleanup();
    actor.connect = connect;
    actor.view.operation_id = Some(operation);
    actor.view.phase = "bound";
    actor.view.udp_state = "enabled";
    assert_eq!(
        actor
            .handle(NativeVoiceRequest::Stop {}, &CancellationToken::new())
            .await,
        Err(TRANSPORT_CLEANUP_FAILED),
    );
    actor.publish();
    let snapshot = actor.shared.lock().unwrap();
    assert_eq!(snapshot.phase, "failed");
    assert_eq!(snapshot.udp_state, "cleanupFailed");
    assert_eq!(snapshot.error_code, Some(TRANSPORT_CLEANUP_FAILED));
}

#[tokio::test]
async fn native_voice_actor_failed_prepare_is_terminal_and_retryable_after_owner_finishes() {
    let root = TestRoot::new();
    // Native preflight must reject the eventual CreateProcess working directory
    // before downloading or creating anything. This is an actual manager call,
    // including its retained blocking task and busy flag, with no injected error.
    let app_data = root.path().join("x".repeat(180));
    let manager = DesktopVoiceManager::default();
    let connect = ConnectManager::default();
    let mut revision = 0;
    for _ in 0..2 {
        let result = manager
            .execute(
                &connect,
                &app_data,
                NativeVoiceRequest::PrepareRuntime {
                    profile: UiVoiceProfile::SimpleVoiceChat,
                    runtime_profile: NativeVoiceRuntime::Fabric1165,
                },
            )
            .await;
        assert!(matches!(result, Err("native_voice_preparation_failed")));
        let snapshot = manager.snapshot().unwrap();
        assert_eq!(snapshot.phase, "failed");
        assert_eq!(snapshot.error_code, Some("native_voice_preparation_failed"));
        assert!(snapshot.revision > revision);
        revision = snapshot.revision;
        assert!(snapshot.review.is_none());
        assert!(snapshot.native_review_deadline.is_none());
        assert!(!snapshot.recovery_pending);
        assert!(!snapshot.eula_recovery_available);
        assert!(snapshot.operation_id.is_none());
        assert!(snapshot.tcp_address.is_none());
        assert!(!app_data.exists());
        let current = manager.client.lock().unwrap().clone().unwrap();
        assert!(!current.busy.load(Ordering::Acquire));
        assert!(current.source.lock().unwrap().is_none());
        assert!(current.completion.lock().unwrap().is_none());
    }
    manager.shutdown_all().await;
    assert_eq!(manager.snapshot().unwrap().phase, "stopped");
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}

#[tokio::test]
async fn native_voice_actor_review_projection_never_renews_original_deadline() {
    let root = TestRoot::new();
    let mut actor = actor(root.path().into());
    let review = Review {
        review_id: Uuid::new_v4(),
        digest: "a".repeat(64),
        purpose: "projection_only_no_file_authority",
        bundle_id: BundleId(Uuid::new_v4()),
        before: "before".into(),
        after: "after".into(),
        details: vec![],
    };
    // This is only a presentation review. No file/launch authority is created.
    let deadline = Instant::now() + Duration::from_secs(2);
    actor.review("files", &review, deadline);
    actor.publish();
    let first = actor.shared.lock().unwrap().clone().projected();
    let first_remaining = first.review.as_ref().unwrap().remaining_ms;
    assert!(first_remaining > 0 && first_remaining <= 2_000);
    tokio::time::sleep(Duration::from_millis(25)).await;
    actor.publish();
    let second = actor.shared.lock().unwrap().clone().projected();
    assert!(second.review.as_ref().unwrap().remaining_ms + 10 < first_remaining);
    assert_eq!(second.native_review_deadline, Some(deadline));
    let wire = serde_json::to_value(&second).unwrap();
    assert!(wire.get("nativeReviewDeadline").is_none());
    assert_eq!(wire["review"]["digest"], review.digest);
    assert!(wire["review"]["remainingMs"].as_u64().unwrap() <= 2_000);

    // A review that arrives late keeps the issuer's expired deadline. Neither
    // display publication nor repeated snapshots can give it another60seconds.
    actor.review("files", &review, Instant::now() - Duration::from_secs(1));
    for _ in 0..3 {
        actor.publish();
        let late = actor.shared.lock().unwrap().clone().projected();
        assert_eq!(late.review.unwrap().remaining_ms, 0);
        assert_eq!(
            actor.approval("files", &review.digest),
            Err("native_voice_stale_review")
        );
    }
    actor.maintain().await;
    assert_eq!(actor.view.phase, "reviewExpired");
    assert!(actor.view.review.is_none());
    assert!(actor.view.native_review_deadline.is_none());
}

async fn restore_eula(actor: &mut Actor, configuration: &Path, expected_voice: &[u8]) {
    let cancel = CancellationToken::new();
    actor
        .handle(NativeVoiceRequest::PrepareEulaRestore {}, &cancel)
        .await
        .unwrap();
    assert_eq!(actor.view.phase, "eulaRestoreReviewed");
    assert!(actor.view.eula_recovery_available);
    let review = actor.view.clone().projected().review.unwrap();
    assert_eq!(review.kind, "eulaRestore");
    assert!(review.remaining_ms > 0 && review.remaining_ms <= 60_000);
    let original_deadline = actor.eula_recovery.as_ref().unwrap().approval_deadline();
    let eula = actor
        .eula_recovery
        .as_ref()
        .unwrap()
        .test_paths()
        .copy
        .join("eula.txt");
    assert_eq!(actor.view.native_review_deadline, Some(original_deadline));
    assert_eq!(fs::read(&eula).unwrap(), EULA_TRUE);
    assert_eq!(
        actor
            .handle(
                NativeVoiceRequest::ApproveEulaRestore {
                    digest: "0".repeat(64),
                },
                &cancel,
            )
            .await,
        Err("native_voice_stale_review")
    );
    assert_eq!(fs::read(configuration).unwrap(), expected_voice);
    assert_eq!(fs::read(&eula).unwrap(), EULA_TRUE);

    // Expire the actual original plan, then let normal maintenance release its
    // guard. A new explicit review is required; the old digest stays unusable.
    actor.eula_recovery.as_mut().unwrap().expire_for_test();
    actor.view.native_review_deadline =
        Some(actor.eula_recovery.as_ref().unwrap().approval_deadline());
    actor.maintain().await;
    assert!(actor.eula_recovery.is_none());
    assert_eq!(actor.view.phase, "reviewExpired");
    assert!(actor.view.eula_recovery_available);
    assert_eq!(fs::read(&eula).unwrap(), EULA_TRUE);
    actor
        .handle(NativeVoiceRequest::PrepareEulaRestore {}, &cancel)
        .await
        .unwrap();
    let fresh = actor.view.review.as_ref().unwrap().digest.clone();
    assert_ne!(fresh, review.digest);
    assert_eq!(
        actor
            .handle(
                NativeVoiceRequest::ApproveEulaRestore {
                    digest: review.digest,
                },
                &cancel,
            )
            .await,
        Err("native_voice_stale_review")
    );
    actor
        .handle(
            NativeVoiceRequest::ApproveEulaRestore { digest: fresh },
            &cancel,
        )
        .await
        .unwrap();
    assert_eq!(actor.view.eula_state, "restored");
    assert!(!actor.view.eula_recovery_available);
    assert_eq!(fs::read(eula).unwrap(), EULA_FALSE);
    assert_eq!(fs::read(configuration).unwrap(), expected_voice);
}

async fn restore_announcement(actor: &mut Actor) {
    let cancel = CancellationToken::new();
    actor
        .handle(
            NativeVoiceRequest::PrepareRestore { reconciled: false },
            &cancel,
        )
        .await
        .unwrap();
    let review = actor.view.clone().projected().review.unwrap();
    assert_eq!(review.kind, "restore");
    assert!(review.remaining_ms > 0 && review.remaining_ms <= 300_000);
    actor
        .handle(
            NativeVoiceRequest::ApproveRestore {
                digest: review.digest,
            },
            &cancel,
        )
        .await
        .unwrap();
    assert!(!actor.view.recovery_pending);
}

#[tokio::test]
#[ignore = "explicit physical EULA/announcement transactions with real pinned local JAR/JDK; serial process-probe guard, no Java execution"]
async fn native_voice_actor_real_eula_and_announcement_recover_separately_in_both_orders() {
    for (profile, eula_first) in [
        (UiVoiceProfile::SimpleVoiceChat, true),
        (UiVoiceProfile::PlasmoVoice, false),
    ] {
        let plan = discover_plan(profile).expect("actor_recovery_pinned_resources");
        let digest = plan.review().digest.clone();
        let mut prepared = plan.create(&digest).expect("actor_recovery_factory");
        let paths = prepared.test_paths();
        let id = prepared.bundle_id();
        let voice = prepared.bundle.manifest.voice().unwrap().clone();
        let (copy, receipt) = prepared.announcement_recovery.take().unwrap();
        let receipt_id = receipt.receipt_id().to_owned();
        drop(receipt);
        let configuration = copy.root().join(voice.profile.configuration_name());
        let announced = fs::read(&configuration).unwrap();
        let digest = prepared.review().digest.clone();
        let mut applied = prepared.apply_eula(&digest).expect("actor_recovery_apply");
        applied.mark_may_have_spawned().unwrap();
        drop(applied);
        assert_eq!(fs::read(copy.root().join("eula.txt")).unwrap(), EULA_TRUE);

        let lab = lab_root().unwrap();
        let mut actor = actor(lab.clone());
        // The inventory candidate is selection only. Resume must reopen the
        // real original bundle and receipt under the native process guard.
        actor.view.recoverable_copies = vec![NativeVoiceRecoveryCandidate {
            bundle_id: id.0.to_string(),
            receipt_id: receipt_id.clone(),
            profile,
            runtime_profile: NativeVoiceRuntime::Fabric1211,
            eula_recovery_pending: true,
            announcement_recovery_pending: true,
        }];
        actor
            .handle(
                NativeVoiceRequest::ResumeRecovery {
                    bundle_id: id.0.to_string(),
                    receipt_id: receipt_id.clone(),
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(actor.view.phase, "needsRecovery");
        assert!(actor.view.eula_recovery_available && actor.view.recovery_pending);

        if eula_first {
            restore_eula(&mut actor, &configuration, &announced).await;
            assert_eq!(actor.view.phase, "stopped");
            assert!(actor.view.recovery_pending);
            assert!(!actor.view.recoverable_copies[0].eula_recovery_pending);
            assert!(actor.view.recoverable_copies[0].announcement_recovery_pending);
            assert_eq!(fs::read(copy.root().join("eula.txt")).unwrap(), EULA_FALSE);
            restore_announcement(&mut actor).await;
        } else {
            restore_announcement(&mut actor).await;
            assert_eq!(actor.view.phase, "needsRecovery");
            assert!(actor.view.eula_recovery_available);
            assert!(actor.view.recoverable_copies[0].eula_recovery_pending);
            assert!(!actor.view.recoverable_copies[0].announcement_recovery_pending);
            assert_eq!(fs::read(copy.root().join("eula.txt")).unwrap(), EULA_TRUE);
            // A restored announcement receipt still selects an unresolved EULA;
            // reopening it must not manufacture new announcement authority.
            drop(actor);
            {
                let guard = super::super::super::acquire_voice_mutation_guard(&copy).unwrap();
                let receipts =
                    super::super::super::list_restoration_receipts(&copy, &guard).unwrap();
                assert_eq!(
                    receipts
                        .iter()
                        .find(|receipt| receipt.receipt_id == receipt_id)
                        .unwrap()
                        .state,
                    super::super::super::DurableReceiptState::Restored,
                );
            }
            let resumed = resume_recovery(&lab, &id.0.to_string(), &receipt_id).unwrap();
            assert!(resumed.eula_pending);
            assert!(resumed.announcement.is_none());
            actor = self::actor(lab.clone());
            actor.bundle_id = Some(id);
            actor.view.eula_state = "needsRecovery";
            actor.view.eula_recovery_available = true;
            restore_eula(&mut actor, &configuration, voice.profile.configuration()).await;
        }
        assert_eq!(actor.view.phase, "restored");
        assert!(!actor.view.eula_recovery_available && !actor.view.recovery_pending);
        assert!(actor.view.recoverable_copies.is_empty());
        assert_eq!(fs::read(copy.root().join("eula.txt")).unwrap(), EULA_FALSE);
        assert_eq!(
            fs::read(configuration).unwrap(),
            voice.profile.configuration()
        );
        drop(actor);
        assert!(resume_recovery(&lab, &id.0.to_string(), &receipt_id).is_err());
        drop(copy);

        // Delete only this fixture's UUID-named native bundle after all guards
        // were collected and both original recovery receipts are terminal.
        let canonical = paths.bundle.canonicalize().unwrap();
        assert_eq!(
            canonical.parent(),
            Some(lab.canonicalize().unwrap().as_path())
        );
        assert_eq!(
            paths.bundle.file_name().unwrap().to_str().unwrap(),
            format!("native-status-{}", id.0)
        );
        fs::remove_dir_all(paths.bundle).expect("actor_recovery_fixture_cleanup");
    }
}
