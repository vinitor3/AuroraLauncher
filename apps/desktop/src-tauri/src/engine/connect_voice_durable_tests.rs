//! End-to-end native filesystem tests. Only explicitly supplied pinned public
//! JARs are copied; no download, Minecraft process or personal instance is used.
use super::*;
#[path = "connect_voice_retirement_tests.rs"]
mod retirement;

fn fixture(voice_mod: VoiceMod, original: &[u8]) -> Fixture {
    let name = match voice_mod {
        VoiceMod::SimpleVoiceChat => "voicechat-fabric-1.21.1-2.6.22.jar",
        VoiceMod::PlasmoVoice => "plasmovoice-fabric-1.21.1-2.1.17.jar",
    };
    let jar = PathBuf::from(
        std::env::var_os("AURORA_CONNECT_ARTIFACT_DIR")
            .expect("explicit pinned local artifact directory required"),
    )
    .join(name);
    Fixture::for_mod(Some(&jar), voice_mod, original)
}

fn reopen_cycle(voice_mod: VoiceMod, original: &[u8], reconciled: bool) {
    let f = fixture(voice_mod, original);
    let guard = fixture_voice_guard(&f.copy).unwrap();
    let plan = prepare_durable_announcement(&f.copy, &guard, f.request()).unwrap();
    let approval = plan.review().digest.clone();
    let applied = plan.confirm(&f.copy, &guard, &approval).unwrap();
    let id = applied.receipt.receipt_id().to_owned();
    let announced = f.bytes();
    assert_ne!(announced, original);
    assert_eq!(applied.receipt.state(), RestorationState::Ready);
    assert_eq!(applied.receipt.generation(), 0);
    drop(applied);
    drop(guard);
    // A fresh Instance handle and a fresh guard reconstruct solely from disk.
    let copy = InstanceLayout::new(&f.root).open(f.copy.id().clone());
    fs::remove_file(copy.root().join("mods/voicechat.jar")).unwrap();
    fs::write(copy.root().join("options.txt"), b"gamma:0.9\n").unwrap();
    if reconciled {
        let mut edited = b"# Independent retained comment\n".to_vec();
        edited.extend_from_slice(&announced);
        fs::write(copy.root().join(voice_mod.config()), edited).unwrap();
    }
    let guard = fixture_voice_guard(&copy).unwrap();
    let summaries = list_restoration_receipts(&copy, &guard).unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].receipt_id, id);
    assert_eq!(summaries[0].generation, 0);
    let encoded = serde_json::to_string(&summaries).unwrap();
    assert!(!encoded.contains("127.0.0.1"));
    assert!(!encoded.contains("sha256"));
    assert!(!encoded.contains("config/"));
    let mut resumed = resume_restoration(&copy, &guard, &id).unwrap();
    assert!(!resumed.has_pending_review());
    let review = if reconciled {
        assert!(matches!(
            resumed.prepare_strict(&copy, &guard),
            Err(RecoveryError::StalePreview)
        ));
        assert!(!resumed.has_pending_review());
        resumed
            .prepare_reconciled(&copy, &guard, &digest(&f.bytes()))
            .unwrap()
    } else {
        resumed.prepare_strict(&copy, &guard).unwrap()
    };
    let current = f.bytes();
    assert!(resumed.has_pending_review());
    assert!(resumed.cancel_review());
    assert!(!resumed.cancel_review());
    assert_eq!(f.bytes(), current);
    assert!(matches!(
        resumed.confirm(&copy, &guard, &review.digest),
        Err(RecoveryError::StalePreview)
    ));
    let review = if reconciled {
        resumed
            .prepare_reconciled(&copy, &guard, &digest(&current))
            .unwrap()
    } else {
        resumed.prepare_strict(&copy, &guard).unwrap()
    };
    resumed.confirm(&copy, &guard, &review.digest).unwrap();
    assert_eq!(resumed.state(), RestorationState::Restored);
    assert_eq!(resumed.generation(), 1);
    if reconciled {
        assert!(f.bytes().starts_with(b"# Independent retained comment\n"));
        match voice_mod {
            VoiceMod::SimpleVoiceChat => assert_eq!(
                svc_value_span(&f.bytes())
                    .map(|r| f.bytes()[r].to_vec())
                    .unwrap(),
                original[svc_value_span(original).unwrap()]
            ),
            VoiceMod::PlasmoVoice => assert_eq!(
                plasmo::inspect_public(&f.bytes()).unwrap(),
                plasmo::inspect_public(original).unwrap()
            ),
        }
    } else {
        assert_eq!(f.bytes(), original);
    }
    assert_eq!(
        fs::read(copy.root().join("options.txt")).unwrap(),
        b"gamma:0.9\n"
    );
    assert_eq!(
        fs::read(f.source.root().join(voice_mod.config())).unwrap(),
        original
    );
    drop(resumed);
    drop(guard);
    let guard = fixture_voice_guard(&copy).unwrap();
    let summaries = list_restoration_receipts(&copy, &guard).unwrap();
    assert_eq!(summaries[0].state, DurableReceiptState::Restored);
    assert_eq!(summaries[0].generation, 1);
    assert!(matches!(
        resume_restoration(&copy, &guard, &id),
        Err(RecoveryError::StalePreview)
    ));
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; own fixtures, no Java"]
fn durable_voice_svc_reopens_strict_and_reconciled_without_installed_jar() {
    reopen_cycle(VoiceMod::SimpleVoiceChat, ORIGINAL, false);
    reopen_cycle(VoiceMod::SimpleVoiceChat, ORIGINAL, true);
}
#[test]
#[ignore = "requires explicit pinned local Plasmo artifact; own fixtures, no Java"]
fn durable_voice_plasmo_reopens_strict_and_reconciled_with_absent_public() {
    reopen_cycle(VoiceMod::PlasmoVoice, PLASMO_DEFAULTS, false);
    reopen_cycle(VoiceMod::PlasmoVoice, PLASMO_DEFAULTS, true);
    let public = b"[host]\nip='0.0.0.0'\nport=24454\n[host.public]\nip='old.invalid'\nport=24454\n";
    reopen_cycle(VoiceMod::PlasmoVoice, public, false);
    reopen_cycle(VoiceMod::PlasmoVoice, public, true);
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; own fixtures, no Java"]
fn durable_voice_refuses_conflicts_identity_and_stale_generation_without_overwrite() {
    let f = fixture(VoiceMod::SimpleVoiceChat, ORIGINAL);
    let guard = fixture_voice_guard(&f.copy).unwrap();
    let plan = prepare_durable_announcement(&f.copy, &guard, f.request()).unwrap();
    let digest = plan.review().digest.clone();
    let applied = plan.confirm(&f.copy, &guard, &digest).unwrap();
    let id = applied.receipt.receipt_id().to_owned();
    let announced = f.bytes();
    drop(applied);
    let mut first = resume_restoration(&f.copy, &guard, &id).unwrap();
    let mut second = resume_restoration(&f.copy, &guard, &id).unwrap();
    let conflict = b"voice_host=someone.invalid:1234\nport=24454\n";
    fs::write(f.copy.root().join(CONFIG), conflict).unwrap();
    assert!(first.prepare_strict(&f.copy, &guard).is_err());
    assert!(first
        .prepare_reconciled(&f.copy, &guard, &super::digest(conflict))
        .is_err());
    assert_eq!(f.bytes(), conflict);
    fs::write(f.copy.root().join(CONFIG), &announced).unwrap();
    let a = first.prepare_strict(&f.copy, &guard).unwrap();
    let b = second.prepare_strict(&f.copy, &guard).unwrap();
    first.confirm(&f.copy, &guard, &a.digest).unwrap();
    assert!(matches!(
        second.confirm(&f.copy, &guard, &b.digest),
        Err(RecoveryError::StalePreview)
    ));
    assert_eq!(f.bytes(), ORIGINAL);
    drop(guard);
    let other = fixture(VoiceMod::SimpleVoiceChat, ORIGINAL);
    let other_guard = fixture_voice_guard(&other.copy).unwrap();
    assert!(resume_restoration(&other.copy, &other_guard, &id).is_err());
    assert_eq!(other.bytes(), ORIGINAL);
}

#[test]
#[ignore = "requires explicit pinned local SVC artifact; own fixtures, no Java"]
fn durable_voice_cancel_bad_approval_and_cas_tamper_never_restore() {
    let f = fixture(VoiceMod::SimpleVoiceChat, ORIGINAL);
    let guard = fixture_voice_guard(&f.copy).unwrap();
    let plan = prepare_durable_announcement(&f.copy, &guard, f.request()).unwrap();
    drop(plan);
    assert!(list_restoration_receipts(&f.copy, &guard)
        .unwrap()
        .is_empty());
    let plan = prepare_durable_announcement(&f.copy, &guard, f.request()).unwrap();
    assert!(matches!(
        plan.confirm(&f.copy, &guard, "incorrect"),
        Err(RecoveryError::StalePreview)
    ));
    assert_eq!(f.bytes(), ORIGINAL);
    assert!(list_restoration_receipts(&f.copy, &guard)
        .unwrap()
        .is_empty());
    let plan = prepare_durable_announcement(&f.copy, &guard, f.request()).unwrap();
    let approved = plan.review().digest.clone();
    let applied = plan.confirm(&f.copy, &guard, &approved).unwrap();
    let id = applied.receipt.receipt_id().to_owned();
    let announced = f.bytes();
    drop(applied);
    drop(guard);
    let hash = digest(ORIGINAL);
    let blob = f.root.join(format!(
        ".aurora/recovery/blobs/sha256/{}/{}",
        &hash[..2],
        hash
    ));
    fs::write(blob, b"untrusted altered CAS").unwrap();
    let guard = fixture_voice_guard(&f.copy).unwrap();
    assert!(list_restoration_receipts(&f.copy, &guard).is_err());
    assert!(resume_restoration(&f.copy, &guard, &id).is_err());
    assert_eq!(f.bytes(), announced);
}

#[test]
fn durable_voice_empty_config_is_outside_both_existing_renderers() {
    assert!(render_svc(b"", 35554).is_err());
    assert!(plasmo::render(b"", 35554).is_err());
}
