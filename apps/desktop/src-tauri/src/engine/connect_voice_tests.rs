use super::*;
use crate::engine::{acquire_instance_recovery_lock, InstanceId, InstanceLayout};
use std::path::{Path, PathBuf};

const ORIGINAL: &[u8] =
    b"# User comment\r\nport=24454\r\nvoice_host=old.example.invalid:24454\r\nmtu_size=1275\r\n";
const SVC_DEFAULTS: &[u8] =
    include_bytes!("../../tests/fixtures/connect_voice/svc-default-equivalent.properties");
const PLASMO_DEFAULTS: &[u8] =
    include_bytes!("../../tests/fixtures/connect_voice/plasmo-default-equivalent.toml");

struct Fixture {
    root: PathBuf,
    source: Instance,
    copy: Instance,
    voice_mod: VoiceMod,
}
impl Fixture {
    fn new(jar: Option<&Path>) -> Self {
        Self::for_mod(jar, VoiceMod::SimpleVoiceChat, ORIGINAL)
    }
    fn for_mod(jar: Option<&Path>, voice_mod: VoiceMod, config: &[u8]) -> Self {
        let root = std::env::temp_dir().join(format!("aurora-connect-config-{}", Uuid::new_v4()));
        let layout = InstanceLayout::new(&root);
        let source = layout.open(InstanceId::parse("fixture-source").unwrap());
        source.ensure_layout().unwrap();
        let config_path = source.root().join(voice_mod.config());
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(config_path, config).unwrap();
        fs::write(
            source.root().join("aurora-instance.json"),
            br#"{"minecraft":"1.21.1","loader":"fabric","versionId":"1.21.1"}"#,
        )
        .unwrap();
        if let Some(jar) = jar {
            fs::copy(jar, source.root().join("mods/voicechat.jar")).unwrap();
        } else {
            fs::write(
                source.root().join("mods/voicechat.jar"),
                b"synthetic unapproved jar",
            )
            .unwrap();
        }
        let source_guard = acquire_instance_recovery_lock(&source).unwrap();
        let environment = diagnostic_lab::create(&source, &source_guard, None, |_| {}).unwrap();
        drop(source_guard);
        let copy = layout.open(InstanceId::parse(environment.id).unwrap());
        Self {
            root,
            source,
            copy,
            voice_mod,
        }
    }
    fn request(&self) -> AnnouncementRequest {
        AnnouncementRequest {
            voice_mod: self.voice_mod,
            mode: AnnouncementMode::DedicatedAuroraOnly,
            guest_port: 35554,
            expected_source_sha256: digest(&self.bytes()),
        }
    }
    fn bytes(&self) -> Vec<u8> {
        fs::read(self.copy.root().join(self.voice_mod.config())).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.root.is_absolute()
                && self.root.starts_with(std::env::temp_dir())
                && self
                    .root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("aurora-connect-config-")
        );
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn renderer_changes_only_existing_announcement_and_preserves_every_other_byte() {
    let input = "# Comentário\r\nport=24454\r\n  voice_host = old.example.invalid:24454 \r\nmtu_size=1275\n";
    let (rendered, before, after) = render_svc(input.as_bytes(), 35554).unwrap();
    assert_eq!(before, "old.example.invalid:24454 ");
    assert_eq!(after, "127.0.0.1:35554");
    assert_eq!(
        String::from_utf8(rendered).unwrap(),
        input.replace("old.example.invalid:24454 ", "127.0.0.1:35554")
    );
    let (no_newline, _, _) = render_svc(b"voice_host=", 1).unwrap();
    assert_eq!(no_newline, b"voice_host=127.0.0.1:1");
}

#[test]
fn svc_output_must_fit_the_same_bound_as_future_selective_restoration() {
    let suffix = b"\nvoice_host=";
    let mut input = vec![b'#'; MAX_CONFIG as usize - suffix.len()];
    input.extend_from_slice(suffix);
    assert_eq!(input.len(), MAX_CONFIG as usize);
    assert!(matches!(
        render_svc(&input, 65535),
        Err(RecoveryError::Limit)
    ));
    let growth = b"127.0.0.1:65535".len();
    input.drain(..growth);
    let (output, _, _) = render_svc(&input, 65535).unwrap();
    assert_eq!(output.len(), MAX_CONFIG as usize);
}

#[test]
fn renderer_refuses_ambiguous_properties_unknown_syntax_and_missing_field() {
    for bytes in [
        b"voice_host=a\nvoice_host=b".as_slice(),
        b"voice_host=a\\\nb",
        b"voice_host:a",
        b"port=24454",
        b"voice_host=a\rport=24454",
        b"voice_host=\0",
        b"\xff",
        b"=x\nvoice_host=a",
    ] {
        assert!(render_svc(bytes, 35554).is_err());
    }
    assert!(render_svc(ORIGINAL, 0).is_err());
    assert!(render_svc(&vec![b' '; MAX_CONFIG as usize + 1], 35554).is_err());
    assert!(render_svc(b"voice_host=127.0.0.1:35554", 35554).is_err());
    for text in [
        "\u{00a0}voice_host=a",
        "voice_host\u{00a0}=a",
        "voice_host=\u{00a0}a",
        "\u{2003}voice_host=a",
        "voice_host\u{2003}=a",
        "voice_host=\u{2003}a",
    ] {
        assert!(render_svc(text.as_bytes(), 35554).is_err());
    }
}

#[test]
fn complete_default_equivalents_are_eligible_without_weakening_sensitive_filters() {
    for (voice_mod, original) in [
        (VoiceMod::SimpleVoiceChat, SVC_DEFAULTS),
        (VoiceMod::PlasmoVoice, PLASMO_DEFAULTS),
    ] {
        let f = Fixture::for_mod(None, voice_mod, original);
        let config = f.copy.root().join(voice_mod.config());
        assert!(!recovery::sensitive_config(&config, voice_mod.config()).unwrap());
        let guard = acquire_instance_recovery_lock(&f.copy).unwrap();
        let observation = recovery::capture_observation(&f.copy, &guard).unwrap();
        assert!(observation
            .files
            .iter()
            .any(|file| { file.path == voice_mod.config() && file.sha256 == digest(original) }));
        let output = match voice_mod {
            VoiceMod::SimpleVoiceChat => {
                let output = render_svc(original, 35554).unwrap().0;
                let expected = std::str::from_utf8(original)
                    .unwrap()
                    .replace("voice_host=", "voice_host=127.0.0.1:35554");
                assert_eq!(output, expected.as_bytes());
                output
            }
            VoiceMod::PlasmoVoice => {
                let output = plasmo::render(original, 35554).unwrap().0;
                assert!(output.starts_with(original));
                output
            }
        };
        assert!(output.len() < MAX_CONFIG as usize);
        assert_eq!(f.bytes(), original, "rendering must not mutate the fixture");
        // Full configs do not bypass the artifact gate.
        assert!(matches!(
            prepare_announcement(&f.copy, &guard, f.request()),
            Err(RecoveryError::Compatibility)
        ));
        fs::write(&config, [original, b"\n# token=fixture-only\n"].concat()).unwrap();
        assert!(recovery::sensitive_config(&config, voice_mod.config()).unwrap());
        assert!(!recovery::capture_observation(&f.copy, &guard)
            .unwrap()
            .files
            .iter()
            .any(|file| file.path == voice_mod.config()));
    }
}

#[test]
fn primary_instances_and_unapproved_jar_bytes_never_get_an_announcement_plan() {
    let f = Fixture::new(None);
    let primary_guard = acquire_instance_recovery_lock(&f.source).unwrap();
    assert!(matches!(
        prepare_announcement(&f.source, &primary_guard, f.request()),
        Err(RecoveryError::UnsafePath)
    ));
    drop(primary_guard);
    let guard = acquire_instance_recovery_lock(&f.copy).unwrap();

    assert!(matches!(
        prepare_announcement(&f.copy, &guard, f.request()),
        Err(RecoveryError::Compatibility)
    ));
    assert_eq!(f.bytes(), ORIGINAL);
}

#[test]
#[ignore = "requires explicit AURORA_CONNECT_ARTIFACT_DIR with the pinned public SVC JAR; never downloads or executes Java"]
fn real_pinned_artifact_copy_uses_existing_preview_confirmation_undo_and_conflict_guards() {
    let jar = PathBuf::from(
        std::env::var_os("AURORA_CONNECT_ARTIFACT_DIR")
            .expect("explicit local artifact directory required"),
    )
    .join("voicechat-fabric-1.21.1-2.6.22.jar");
    assert_eq!(
        recovery::hash_file(&jar).unwrap(),
        (SVC_SHA256.into(), SVC_BYTES)
    );
    let f = Fixture::new(Some(&jar));
    let guard = acquire_instance_recovery_lock(&f.copy).unwrap();

    let other = Fixture::new(Some(&jar));
    let other_guard = acquire_instance_recovery_lock(&other.copy).unwrap();
    let foreign = prepare_announcement(&f.copy, &guard, f.request()).unwrap();
    let foreign_digest = foreign.review().digest.clone();
    assert!(matches!(
        foreign.confirm(&other.copy, &other_guard, &foreign_digest),
        Err(RecoveryError::Identity)
    ));
    assert_eq!(other.bytes(), ORIGINAL);
    drop(other_guard);

    let mut wrong_hash = f.request();
    wrong_hash.expected_source_sha256 = "0".repeat(64);
    assert!(matches!(
        prepare_announcement(&f.copy, &guard, wrong_hash),
        Err(RecoveryError::StalePreview)
    ));

    // Preparation creates neither a target point nor a reusable recovery preview.
    // Generic recovery must not bypass cancellation, wrong approval or expiry.
    let point_ids = |f: &Fixture, guard: &InstanceMutationGuard| {
        recovery::list_points(&f.copy, guard)
            .unwrap()
            .points
            .into_iter()
            .map(|point| point.id)
            .collect::<Vec<_>>()
    };
    let initial_points = point_ids(&f, &guard);
    let cancelled = prepare_announcement(&f.copy, &guard, f.request()).unwrap();
    assert_eq!(cancelled.review().after, "127.0.0.1:35554");
    assert_eq!(f.bytes(), ORIGINAL);
    let cancelled_id = cancelled.review().plan_id.clone();
    drop(cancelled);
    assert!(matches!(
        recovery::restore(&f.copy, &guard, &cancelled_id),
        Err(RecoveryError::StalePreview)
    ));
    assert_eq!(f.bytes(), ORIGINAL);

    let refused = prepare_announcement(&f.copy, &guard, f.request()).unwrap();
    assert!(matches!(
        refused.confirm(&f.copy, &guard, "wrong"),
        Err(RecoveryError::StalePreview)
    ));
    let mut expired = prepare_announcement(&f.copy, &guard, f.request()).unwrap();
    expired.deadline = Instant::now() - Duration::from_secs(1);
    let expired_digest = expired.review().digest.clone();
    assert!(matches!(
        expired.confirm(&f.copy, &guard, &expired_digest),
        Err(RecoveryError::StalePreview)
    ));
    assert_eq!(point_ids(&f, &guard), initial_points);
    assert!(!f.copy.root().join(".aurora/connect-voice-stage").exists());

    let plan = prepare_announcement(&f.copy, &guard, f.request()).unwrap();
    let approval = plan.review().digest.clone();
    assert!(
        plan.confirm(&f.copy, &guard, &approval)
            .unwrap()
            .recovery
            .undo_available
    );
    assert_eq!(f.bytes(), render_svc(ORIGINAL, 35554).unwrap().0);
    assert_eq!(fs::read(f.source.root().join(CONFIG)).unwrap(), ORIGINAL);
    assert!(
        diagnostic_lab::inspect(&f.copy, &guard)
            .unwrap()
            .changes
            .is_empty(),
        "voice edit must not become applicable to source through existing generic settings"
    );

    let undo = recovery::preview_undo(&f.copy, &guard).unwrap();
    recovery::restore(&f.copy, &guard, &undo.id).unwrap();
    assert_eq!(f.bytes(), ORIGINAL);

    let stale = prepare_announcement(&f.copy, &guard, f.request()).unwrap();
    let approval = stale.review().digest.clone();
    let edited = [ORIGINAL, b"# independent later edit\r\n"].concat();
    fs::write(f.copy.root().join(CONFIG), &edited).unwrap();
    assert!(matches!(
        stale.confirm(&f.copy, &guard, &approval),
        Err(RecoveryError::StalePreview)
    ));
    assert_eq!(f.bytes(), edited);
    assert_eq!(fs::read(f.source.root().join(CONFIG)).unwrap(), ORIGINAL);
}

const PLASMO_ORIGINAL: &[u8] = b"# Keep this comment\r\n[host]\r\nip = \"127.0.0.1\"\r\nport = 25565\r\n\r\n[host.public]\r\nip = 'old.example.invalid' # keep\r\nport = 25565\r\n\r\n[voice]\r\nmtu_size = 1024\r\n";

#[test]
fn plasmo_cannot_borrow_unapproved_artifact_or_unknown_config_path() {
    let f = Fixture::for_mod(None, VoiceMod::PlasmoVoice, PLASMO_ORIGINAL);
    let guard = acquire_instance_recovery_lock(&f.copy).unwrap();
    assert!(matches!(
        prepare_announcement(&f.copy, &guard, f.request()),
        Err(RecoveryError::Compatibility)
    ));
    assert_eq!(f.bytes(), PLASMO_ORIGINAL);
}

#[test]
#[ignore = "requires explicit AURORA_CONNECT_ARTIFACT_DIR with the pinned public Plasmo JAR; never downloads or executes Java"]
fn real_plasmo_artifact_can_change_or_add_announcement_with_approval_undo_and_isolation() {
    let jar = PathBuf::from(
        std::env::var_os("AURORA_CONNECT_ARTIFACT_DIR")
            .expect("explicit local artifact directory required"),
    )
    .join("plasmovoice-fabric-1.21.1-2.1.17.jar");
    assert_eq!(
        recovery::hash_file(&jar).unwrap(),
        (PLASMO_SHA256.into(), PLASMO_BYTES)
    );
    let without_public = b"# [host.public] template stays commented\r\n[host]\r\nip = \"127.0.0.1\"\r\nport = 25565\r\n[voice]\r\nmtu_size = 1024";
    for original in [PLASMO_ORIGINAL, without_public.as_slice()] {
        let f = Fixture::for_mod(Some(&jar), VoiceMod::PlasmoVoice, original);
        let guard = acquire_instance_recovery_lock(&f.copy).unwrap();
        let mut wrong_mod = f.request();
        wrong_mod.voice_mod = VoiceMod::SimpleVoiceChat;
        assert!(matches!(
            prepare_announcement(&f.copy, &guard, wrong_mod),
            Err(RecoveryError::Compatibility)
        ));

        let cancelled = prepare_announcement(&f.copy, &guard, f.request()).unwrap();
        assert_eq!(cancelled.review().config_path, PLASMO_CONFIG);
        assert_eq!(f.bytes(), original);
        let id = cancelled.review().plan_id.clone();
        drop(cancelled);
        assert!(matches!(
            recovery::restore(&f.copy, &guard, &id),
            Err(RecoveryError::StalePreview)
        ));
        assert!(recovery::list_points(&f.copy, &guard)
            .unwrap()
            .points
            .is_empty());

        let rejected = prepare_announcement(&f.copy, &guard, f.request()).unwrap();
        assert!(matches!(
            rejected.confirm(&f.copy, &guard, "wrong"),
            Err(RecoveryError::StalePreview)
        ));
        assert_eq!(f.bytes(), original);
        let prepared = prepare_announcement(&f.copy, &guard, f.request()).unwrap();
        let approval = prepared.review().digest.clone();
        assert!(
            prepared
                .confirm(&f.copy, &guard, &approval)
                .unwrap()
                .recovery
                .undo_available
        );
        assert_eq!(f.bytes(), plasmo::render(original, 35554).unwrap().0);
        assert_eq!(
            fs::read(f.source.root().join(PLASMO_CONFIG)).unwrap(),
            original
        );
        assert!(diagnostic_lab::inspect(&f.copy, &guard)
            .unwrap()
            .changes
            .is_empty());

        let undo = recovery::preview_undo(&f.copy, &guard).unwrap();
        recovery::restore(&f.copy, &guard, &undo.id).unwrap();
        assert_eq!(f.bytes(), original);

        let stale = prepare_announcement(&f.copy, &guard, f.request()).unwrap();
        let approval = stale.review().digest.clone();
        let edited = [original, b"\r\n# independent later change\r\n"].concat();
        fs::write(f.copy.root().join(PLASMO_CONFIG), &edited).unwrap();
        assert!(matches!(
            stale.confirm(&f.copy, &guard, &approval),
            Err(RecoveryError::StalePreview)
        ));
        assert_eq!(f.bytes(), edited);
        assert_eq!(
            fs::read(f.source.root().join(PLASMO_CONFIG)).unwrap(),
            original
        );

        // The active path is derived from the pinned Fabric server's bytecode,
        // not guessed from nearby old documentation or a similarly named file.
        let request = f.request();
        fs::rename(
            f.copy.root().join(PLASMO_CONFIG),
            f.copy.root().join("config/plasmovoice/config.toml"),
        )
        .unwrap();
        assert!(matches!(
            prepare_announcement(&f.copy, &guard, request),
            Err(RecoveryError::UnsafePath)
        ));
    }
}

fn exercise_selective_restoration(voice_mod: VoiceMod, jar_name: &str, original: &[u8]) {
    let jar = PathBuf::from(
        std::env::var_os("AURORA_CONNECT_ARTIFACT_DIR")
            .expect("explicit local artifact directory required"),
    )
    .join(jar_name);
    assert_eq!(
        recovery::hash_file(&jar).unwrap(),
        (voice_mod.artifact().0.into(), voice_mod.artifact().1)
    );
    let f = Fixture::for_mod(Some(&jar), voice_mod, original);
    let guard = acquire_instance_recovery_lock(&f.copy).unwrap();
    let prepared = prepare_announcement(&f.copy, &guard, f.request()).unwrap();
    let approval = prepared.review().digest.clone();
    let mut applied = prepared.confirm(&f.copy, &guard, &approval).unwrap();
    let announced = f.bytes();
    assert_eq!(applied.receipt.state(), RestorationState::Ready);

    // Edits that predate the restoration review must survive. Removing the mod
    // must not prevent cleaning up the exact bytes that we ourselves wrote.
    fs::write(f.copy.root().join("options.txt"), b"renderDistance:19\n").unwrap();
    fs::write(
        f.copy.root().join("config/unrelated.json"),
        br#"{"quality":7}"#,
    )
    .unwrap();
    fs::rename(
        f.copy.root().join("mods/voicechat.jar"),
        f.copy.root().join("mods/voicechat.jar.disabled"),
    )
    .unwrap();
    let point_ids = || {
        recovery::list_points(&f.copy, &guard)
            .unwrap()
            .points
            .into_iter()
            .map(|point| point.id)
            .collect::<Vec<_>>()
    };
    let points_before_cancel = point_ids();
    let cancelled = applied.receipt.prepare_restore(&f.copy, &guard).unwrap();
    assert!(matches!(
        cancelled.review().action,
        AnnouncementAction::RestoreOriginal { .. }
    ));
    assert_eq!(cancelled.review().source_sha256, digest(&announced));
    assert_eq!(cancelled.review().replacement_sha256, digest(original));
    let cancelled_id = cancelled.review().plan_id.clone();
    drop(cancelled);
    assert_eq!(applied.receipt.state(), RestorationState::Ready);
    assert_eq!(point_ids(), points_before_cancel);
    assert!(matches!(
        recovery::restore(&f.copy, &guard, &cancelled_id),
        Err(RecoveryError::StalePreview)
    ));

    let wrong = applied.receipt.prepare_restore(&f.copy, &guard).unwrap();
    assert!(
        matches!(
            wrong.confirm(&f.copy, &guard, &approval),
            Err(RecoveryError::StalePreview)
        ),
        "apply approval cannot authorize restore"
    );
    assert_eq!(applied.receipt.state(), RestorationState::Ready);

    let other = Fixture::for_mod(Some(&jar), voice_mod, original);
    let other_guard = acquire_instance_recovery_lock(&other.copy).unwrap();
    assert!(matches!(
        applied.receipt.prepare_restore(&other.copy, &other_guard),
        Err(RecoveryError::Identity)
    ));
    assert_eq!(other.bytes(), original);
    assert_eq!(applied.receipt.state(), RestorationState::Ready);
    drop(other_guard);

    let manifest = f.copy.root().join("aurora-instance.json");
    let initial_manifest = fs::read(&manifest).unwrap();
    fs::write(
        &manifest,
        br#"{"minecraft":"1.20.1","loader":"fabric","versionId":"1.20.1"}"#,
    )
    .unwrap();
    assert!(matches!(
        applied.receipt.prepare_restore(&f.copy, &guard),
        Err(RecoveryError::Compatibility)
    ));
    assert_eq!(f.bytes(), announced);
    assert_eq!(applied.receipt.state(), RestorationState::Ready);
    fs::write(&manifest, initial_manifest).unwrap();

    // A later unrelated edit still invalidates the prepared transaction; a new
    // review must use the newer inventory instead of reverting it.
    let stale = applied.receipt.prepare_restore(&f.copy, &guard).unwrap();
    let restore_approval = stale.review().digest.clone();
    fs::write(f.copy.root().join("options.txt"), b"renderDistance:21\n").unwrap();
    assert!(matches!(
        stale.confirm(&f.copy, &guard, &restore_approval),
        Err(RecoveryError::StalePreview)
    ));
    assert_eq!(applied.receipt.state(), RestorationState::Ready);

    let restored = applied.receipt.prepare_restore(&f.copy, &guard).unwrap();
    let restore_approval = restored.review().digest.clone();
    restored
        .confirm(&f.copy, &guard, &restore_approval)
        .unwrap();
    assert_eq!(applied.receipt.state(), RestorationState::Restored);
    assert_eq!(f.bytes(), original);
    assert_eq!(
        fs::read(f.copy.root().join("options.txt")).unwrap(),
        b"renderDistance:21\n"
    );
    assert_eq!(
        fs::read(f.copy.root().join("config/unrelated.json")).unwrap(),
        br#"{"quality":7}"#
    );
    assert_eq!(
        recovery::hash_file(&f.copy.root().join("mods/voicechat.jar.disabled")).unwrap(),
        (voice_mod.artifact().0.into(), voice_mod.artifact().1)
    );
    assert!(!f.copy.root().join("mods/voicechat.jar").exists());
    assert_eq!(
        fs::read(f.source.root().join(voice_mod.config())).unwrap(),
        original
    );
    assert!(applied.receipt.prepare_restore(&f.copy, &guard).is_err());

    // A new receipt cannot erase even a comment added to its own file before
    // restore is prepared. Put the test mod back solely for this new apply.
    fs::rename(
        f.copy.root().join("mods/voicechat.jar.disabled"),
        f.copy.root().join("mods/voicechat.jar"),
    )
    .unwrap();
    let prepared = prepare_announcement(&f.copy, &guard, f.request()).unwrap();
    let approval = prepared.review().digest.clone();
    let mut second = prepared.confirm(&f.copy, &guard, &approval).unwrap();
    let second_announced = f.bytes();
    let stale_target = second.receipt.prepare_restore(&f.copy, &guard).unwrap();
    let restore_approval = stale_target.review().digest.clone();
    let edited = [
        second_announced.as_slice(),
        b"\n# later independent comment\n",
    ]
    .concat();
    fs::write(f.copy.root().join(voice_mod.config()), &edited).unwrap();
    assert!(matches!(
        stale_target.confirm(&f.copy, &guard, &restore_approval),
        Err(RecoveryError::StalePreview)
    ));
    assert!(matches!(
        second.receipt.prepare_restore(&f.copy, &guard),
        Err(RecoveryError::StalePreview)
    ));
    assert_eq!(f.bytes(), edited);
    assert_eq!(second.receipt.state(), RestorationState::Ready);

    // Fixture-only fault injection after all approval checks: a regular file
    // occupies the staging directory. Once the executor starts, an error must
    // require recovery inspection instead of allowing a blind receipt retry.
    fs::write(f.copy.root().join(voice_mod.config()), &second_announced).unwrap();
    let stage = f.copy.root().join(".aurora/connect-voice-stage");
    assert_eq!(fs::read_dir(&stage).unwrap().count(), 0);
    fs::remove_dir(&stage).unwrap(); // Empty, exact fixture directory only.
    fs::write(&stage, b"fixture staging fault").unwrap();
    let failed = second.receipt.prepare_restore(&f.copy, &guard).unwrap();
    let restore_approval = failed.review().digest.clone();
    assert!(failed.confirm(&f.copy, &guard, &restore_approval).is_err());
    assert_eq!(second.receipt.state(), RestorationState::NeedsRecovery);
    assert_eq!(f.bytes(), second_announced);
    assert!(second.receipt.prepare_restore(&f.copy, &guard).is_err());
    assert_eq!(
        fs::read(f.copy.root().join("options.txt")).unwrap(),
        b"renderDistance:21\n"
    );
}

#[test]
#[ignore = "requires explicit local pinned SVC artifact; no download or Java execution"]
fn svc_selective_restoration_preserves_later_unrelated_edits_and_refuses_target_edits() {
    exercise_selective_restoration(
        VoiceMod::SimpleVoiceChat,
        "voicechat-fabric-1.21.1-2.6.22.jar",
        SVC_DEFAULTS,
    );
}

#[test]
#[ignore = "requires explicit local pinned Plasmo artifact; no download or Java execution"]
fn plasmo_selective_restoration_removes_only_its_appended_block_and_preserves_later_edits() {
    exercise_selective_restoration(
        VoiceMod::PlasmoVoice,
        "plasmovoice-fabric-1.21.1-2.1.17.jar",
        PLASMO_DEFAULTS,
    );
}

#[test]
#[ignore = "requires explicit local pinned Plasmo artifact; no download or Java execution"]
fn equivalent_plasmo_reserialization_requires_review_instead_of_trusting_new_hash() {
    let jar = PathBuf::from(
        std::env::var_os("AURORA_CONNECT_ARTIFACT_DIR")
            .expect("explicit local artifact directory required"),
    )
    .join("plasmovoice-fabric-1.21.1-2.1.17.jar");
    let f = Fixture::for_mod(Some(&jar), VoiceMod::PlasmoVoice, PLASMO_DEFAULTS);
    let guard = acquire_instance_recovery_lock(&f.copy).unwrap();
    let prepared = prepare_announcement(&f.copy, &guard, f.request()).unwrap();
    let approval = prepared.review().digest.clone();
    let mut applied = prepared.confirm(&f.copy, &guard, &approval).unwrap();
    let announced = String::from_utf8(f.bytes()).unwrap();
    let (prefix, public) = announced.rsplit_once("[host.public]").unwrap();
    let voice_start = prefix.find("[voice]").unwrap();
    // Model the writer moving a table beside its parent. This is intentionally
    // not called a captured mod roundtrip: no Java runs in this test.
    let rewritten = format!(
        "{}[host.public]{public}\n{}",
        &prefix[..voice_start],
        &prefix[voice_start..]
    );
    assert_ne!(digest(announced.as_bytes()), digest(rewritten.as_bytes()));
    assert_eq!(
        toml::from_str::<toml::Value>(&announced).unwrap(),
        toml::from_str::<toml::Value>(&rewritten).unwrap()
    );
    fs::write(f.copy.root().join(PLASMO_CONFIG), &rewritten).unwrap();
    assert!(matches!(
        applied.receipt.prepare_restore(&f.copy, &guard),
        Err(RecoveryError::StalePreview)
    ));
    assert_eq!(applied.receipt.state(), RestorationState::Ready);
    assert_eq!(f.bytes(), rewritten.as_bytes());
    assert_eq!(
        fs::read(f.source.root().join(PLASMO_CONFIG)).unwrap(),
        PLASMO_DEFAULTS
    );
}
