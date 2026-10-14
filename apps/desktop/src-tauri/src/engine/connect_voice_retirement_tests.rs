//! Real disposable Recovery storage and pinned local JARs only; no Java or
//! personal configuration is opened by these explicitly selected opt-ins.
use super::*;

fn apply(f: &Fixture, guard: &VoiceMutationGuard) -> DurableAppliedAnnouncement {
    let plan = prepare_durable_announcement(&f.copy, guard, f.request()).unwrap();
    let approved = plan.review().digest.clone();
    plan.confirm(&f.copy, guard, &approved).unwrap()
}

#[test]
#[ignore = "requires explicit pinned local SVC and Plasmo artifacts; own fixtures, no Java"]
fn retirement_voice_both_mods_after_undo_allows_new_durable_application() {
    for (voice_mod, original) in [
        (VoiceMod::SimpleVoiceChat, ORIGINAL),
        (VoiceMod::PlasmoVoice, PLASMO_DEFAULTS),
    ] {
        let f = fixture(voice_mod, original);
        let guard = fixture_voice_guard(&f.copy).unwrap();
        let mut applied = apply(&f, &guard);
        let id = applied.receipt.receipt_id().to_owned();
        let undo = recovery::preview_undo(&f.copy, guard.inner()).unwrap();
        recovery::restore(&f.copy, guard.inner(), &undo.id).unwrap();
        assert_eq!(f.bytes(), original);
        assert!(matches!(
            applied.receipt.prepare_strict(&f.copy, &guard),
            Err(RecoveryError::StalePreview)
        ));
        let blocked = prepare_durable_announcement(&f.copy, &guard, f.request()).unwrap();
        let blocked_digest = blocked.review().digest.clone();
        assert!(matches!(
            blocked.confirm(&f.copy, &guard, &blocked_digest),
            Err(RecoveryError::Pending)
        ));
        let plan = prepare_retirement(&f.copy, &guard, &id, 0).unwrap();
        let review = plan.review().clone();
        assert_eq!(review.voice_mod.config(), voice_mod.config());
        assert_eq!(
            review.current_config_sha256.as_deref(),
            Some(digest(original).as_str())
        );
        assert!(review
            .warning
            .contains("Nenhuma configuração será restaurada"));
        let retired = plan.confirm(&f.copy, &guard, &review.digest).unwrap();
        assert_eq!(retired.state, DurableReceiptState::Retired);
        assert_eq!(retired.generation, 1);
        assert_eq!(f.bytes(), original);
        assert!(matches!(
            applied.receipt.prepare_strict(&f.copy, &guard),
            Err(RecoveryError::StalePreview)
        ));
        assert_eq!(applied.receipt.state(), RestorationState::Ready);
        assert!(matches!(
            resume_restoration(&f.copy, &guard, &id),
            Err(RecoveryError::StalePreview)
        ));
        let second = apply(&f, &guard);
        assert_ne!(second.receipt.receipt_id(), id);
        drop(second);
        drop(applied);
        drop(guard);
        let copy = InstanceLayout::new(&f.root).open(f.copy.id().clone());
        let guard = fixture_voice_guard(&copy).unwrap();
        let history = list_restoration_receipts(&copy, &guard).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(
            history
                .iter()
                .find(|row| row.receipt_id == id)
                .unwrap()
                .state,
            DurableReceiptState::Retired
        );
        assert!(history
            .iter()
            .any(|row| row.state == DurableReceiptState::Ready));
        assert_eq!(
            fs::read(f.source.root().join(voice_mod.config())).unwrap(),
            original
        );
    }
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; own fixtures, no Java"]
fn retirement_voice_reviews_are_explicit_single_use_and_bound_to_current_config() {
    let f = fixture(VoiceMod::SimpleVoiceChat, ORIGINAL);
    let guard = fixture_voice_guard(&f.copy).unwrap();
    let applied = apply(&f, &guard);
    let id = applied.receipt.receipt_id().to_owned();
    let config = f.copy.root().join(CONFIG);
    let announced = f.bytes();
    let cancelled = prepare_retirement(&f.copy, &guard, &id, 0).unwrap();
    drop(cancelled);
    let wrong = prepare_retirement(&f.copy, &guard, &id, 0).unwrap();
    assert!(matches!(
        wrong.confirm(&f.copy, &guard, &"0".repeat(64)),
        Err(RecoveryError::StalePreview)
    ));
    assert_eq!(
        list_restoration_receipts(&f.copy, &guard).unwrap()[0].state,
        DurableReceiptState::Ready
    );
    let changed = prepare_retirement(&f.copy, &guard, &id, 0).unwrap();
    let approved = changed.review().digest.clone();
    fs::write(&config, b"voice_host=changed.invalid:20\nport=24454\n").unwrap();
    assert!(matches!(
        changed.confirm(&f.copy, &guard, &approved),
        Err(RecoveryError::StalePreview)
    ));
    assert_eq!(
        list_restoration_receipts(&f.copy, &guard).unwrap()[0].state,
        DurableReceiptState::Ready
    );
    fs::write(&config, &announced).unwrap();
    assert!(matches!(
        prepare_retirement(&f.copy, &guard, &id, 1),
        Err(RecoveryError::StalePreview)
    ));
    let first = prepare_retirement(&f.copy, &guard, &id, 0).unwrap();
    let second = prepare_retirement(&f.copy, &guard, &id, 0).unwrap();
    let a = first.review().digest.clone();
    let b = second.review().digest.clone();
    first.confirm(&f.copy, &guard, &a).unwrap();
    assert!(matches!(
        second.confirm(&f.copy, &guard, &b),
        Err(RecoveryError::StalePreview)
    ));
    assert_eq!(f.bytes(), announced);
    assert_eq!(fs::read(f.source.root().join(CONFIG)).unwrap(), ORIGINAL);
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; own fixtures, no Java"]
fn retirement_voice_allows_absence_without_jar_but_not_an_excluded_oversize_file() {
    let f = fixture(VoiceMod::SimpleVoiceChat, ORIGINAL);
    let guard = fixture_voice_guard(&f.copy).unwrap();
    let applied = apply(&f, &guard);
    let id = applied.receipt.receipt_id().to_owned();
    let config = f.copy.root().join(CONFIG);
    fs::remove_file(f.copy.root().join("mods/voicechat.jar")).unwrap();
    fs::write(&config, vec![b'x'; MAX_CONFIG as usize + 1]).unwrap();
    assert!(prepare_retirement(&f.copy, &guard, &id, 0).is_err());
    fs::remove_file(&config).unwrap();
    let plan = prepare_retirement(&f.copy, &guard, &id, 0).unwrap();
    assert!(plan.review().current_config_sha256.is_none());
    assert!(plan.review().current_config_bytes.is_none());
    let approved = plan.review().digest.clone();
    plan.confirm(&f.copy, &guard, &approved).unwrap();
    assert!(!config.exists());
    assert_eq!(
        list_restoration_receipts(&f.copy, &guard).unwrap()[0].state,
        DurableReceiptState::Retired
    );
    assert_eq!(fs::read(f.source.root().join(CONFIG)).unwrap(), ORIGINAL);
}
