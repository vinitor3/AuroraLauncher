//! Opt-in companion to the narrow Node live-writer supervisor. Owns the native
//! receipt and launch lock, never Java, and never deletes its retained lab root.
//! IPC is private local evidence, not a public command surface or durable receipt.
use super::*;
use crate::engine::{acquire_instance_mutation_lock, ContentUpdateError};
use serde::{de::DeserializeOwned, Deserialize};
use std::{collections::BTreeSet, fs::File, ops::Range, thread};

type LiveResult<T> = std::result::Result<T, &'static str>;
const POLL: Duration = Duration::from_millis(100);
const IPC_MAX: u64 = 4096;
const GUEST_PORT: u16 = 35554;
const SVC_BASELINE: &[u8] = b"# Disposable Aurora Connect Simple Voice Chat server lab.\n# An explicit loopback bind avoids the mod fallback to a wildcard bind.\nport=43002\nbind_address=127.0.0.1\nvoice_host=\n";
const PLASMO_BASELINE: &[u8] = b"# Disposable Aurora Connect Plasmo Voice server lab.\ndebug = false\ncheck_for_updates = false\nuse_crowdin_translations = false\n\n[host]\nip = \"127.0.0.1\"\nport = 43003\n";
const SVC_BASELINE_HASH: &str = "820f42e6cc7043d23b18daf384f5cbbc98fb1c58729cd74b54ae63a425f39e9b";
const PLASMO_BASELINE_HASH: &str =
    "2dfc96bc221060b2f6cb5ebc73a465893594ea036145fc14d54549bbd2d5634e";
const FABRIC_API_FILE: &str = "fabric-api-0.116.10+1.21.1.jar";
const FABRIC_API_HASH: &str = "af0eb792940568fdb1f37bb65a2599773bc47d2010aedf5ec5ed0bd3b6fc40e6";
const FALSE_EULA: &[u8] = b"# https://www.minecraft.net/en-us/eula\neula=false\n";
const IPC_NAMES: [&str; 9] = [
    "apply-review.json",
    "apply-approval.json",
    "lock-held.json",
    "java-closed.json",
    "restore-review.json",
    "restore-approval.json",
    "restored.json",
    "cancel.json",
    "failed.json",
];

fn checked<T, E>(result: std::result::Result<T, E>, code: &'static str) -> LiveResult<T> {
    result.map_err(|_| code)
}
fn require(condition: bool, code: &'static str) -> LiveResult<()> {
    if condition {
        Ok(())
    } else {
        Err(code)
    }
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn local_directory(variable: &str) -> LiveResult<PathBuf> {
    let path = PathBuf::from(std::env::var_os(variable).ok_or("live_explicit_directory_required")?);
    let text = path.to_str().ok_or("live_directory_encoding")?;
    require(
        cfg!(windows)
            && text.len() > 3
            && text.as_bytes()[0].is_ascii_alphabetic()
            && text.as_bytes()[1] == b':'
            && matches!(text.as_bytes()[2], b'/' | b'\\'),
        "live_local_absolute_directory_required",
    )?;
    for part in text[3..].split(['/', '\\']) {
        checked(recovery::valid_relative(part), "live_directory_component")?;
    }
    require(path.is_absolute(), "live_absolute_directory_required")?;
    Ok(path)
}
fn read(path: &Path, maximum: u64) -> LiveResult<Vec<u8>> {
    checked(recovery::read_bounded(path, maximum), "live_bounded_read")
}
fn safe_path(root: &Path, relative: &str) -> LiveResult<PathBuf> {
    checked(recovery::path(root, relative), "live_unsafe_path")
}
fn absent(path: &Path) -> LiveResult<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err("live_destination_exists"),
    }
}
fn names(root: &Path) -> LiveResult<BTreeSet<String>> {
    let entries = checked(fs::read_dir(root), "live_directory_read")?;
    let mut result = BTreeSet::new();
    for entry in entries {
        let entry = checked(entry, "live_directory_entry")?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "live_entry_encoding")?;
        require(result.len() < 32, "live_entry_limit")?;
        require(result.insert(name), "live_duplicate_entry")?;
    }
    Ok(result)
}

struct Ipc {
    root: PathBuf,
    deadline: Instant,
    _pins: Vec<File>,
}
impl Ipc {
    fn open(deadline: Instant) -> LiveResult<(PathBuf, Self)> {
        let lab = local_directory("AURORA_CONNECT_LIVE_LAB_ROOT")?;
        // src-tauri -> desktop -> apps -> repository. No current-directory or
        // caller-supplied repository fallback is accepted.
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .ok_or("live_manifest_root")?;
        let parent = repository.join(".aurora/connect-runtime");
        let normalize = |p: &Path| p.to_string_lossy().replace('/', "\\").to_ascii_lowercase();
        require(
            lab.parent()
                .is_some_and(|p| normalize(p) == normalize(&parent)),
            "live_lab_confinement",
        )?;
        let suffix = lab
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("voice-roundtrip-"))
            .ok_or("live_lab_name")?;
        require(
            suffix.len() == 6 && suffix.bytes().all(|b| b.is_ascii_alphanumeric()),
            "live_lab_name",
        )?;
        let pins = checked(recovery::pin_directory(&lab), "live_lab_directory_unsafe")?;
        require(
            names(&lab)? == BTreeSet::from(["ipc".to_owned()]),
            "live_lab_not_fresh",
        )?;
        let root = safe_path(&lab, "ipc")?;
        let mut pins = pins;
        pins.extend(checked(
            recovery::pin_directory(&root),
            "live_ipc_directory_unsafe",
        )?);
        require(names(&root)?.is_empty(), "live_ipc_not_empty")?;
        Ok((
            lab,
            Self {
                root,
                deadline,
                _pins: pins,
            },
        ))
    }
    fn check(&self) -> LiveResult<()> {
        require(Instant::now() < self.deadline, "live_deadline")?;
        let entries = names(&self.root)?;
        for name in entries {
            let base = name.strip_suffix(".part").unwrap_or(&name);
            require(IPC_NAMES.contains(&base), "live_ipc_extra_entry")?;
            let path = safe_path(&self.root, &name)?;
            let metadata = checked(fs::symlink_metadata(&path), "live_ipc_metadata")?;
            require(
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() <= IPC_MAX,
                "live_ipc_file_shape",
            )?;
            // A partial is never parsed. The cooperating publisher can still
            // hold it open. Completed messages use recovery's single-link read.
            if !name.ends_with(".part") {
                let _ = read(&path, IPC_MAX)?;
            }
            if base == "cancel.json" {
                return Err("live_cancelled");
            }
        }
        Ok(())
    }
    fn publish(&self, name: &str, value: serde_json::Value) -> LiveResult<()> {
        self.check()?;
        require(IPC_NAMES.contains(&name), "live_ipc_publish_name")?;
        let path = safe_path(&self.root, name)?;
        absent(&path)?;
        let partial = safe_path(&self.root, &format!("{name}.part"))?;
        absent(&partial)?;
        let mut bytes = checked(serde_json::to_vec(&value), "live_ipc_encode")?;
        bytes.push(b'\n');
        require(bytes.len() as u64 <= IPC_MAX, "live_ipc_output_limit")?;
        checked(recovery::write_new(&partial, &bytes), "live_ipc_write_new")?;
        self.check()?;
        // Windows implementation explicitly omits REPLACE_EXISTING.
        checked(
            recovery::durable_rename(&partial, &path),
            "live_ipc_publish",
        )
    }
    fn wait<T: DeserializeOwned>(&self, name: &str) -> LiveResult<T> {
        let path = safe_path(&self.root, name)?;
        loop {
            self.check()?;
            match fs::symlink_metadata(&path) {
                Ok(_) => {
                    return checked(
                        serde_json::from_slice(&read(&path, IPC_MAX)?),
                        "live_ipc_schema",
                    )
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("live_ipc_read"),
            }
            thread::sleep(POLL.min(self.deadline.saturating_duration_since(Instant::now())));
        }
    }
    fn approval(&self, name: &str, digest: &str) -> LiveResult<()> {
        let approval: Approval = self.wait(name)?;
        require(
            valid_digest(&approval.digest) && approval.digest == digest,
            "live_approval_mismatch",
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    digest: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JavaClosed {
    closed: bool,
    exit_code: i32,
    forced: bool,
    eula_restored: bool,
    config_sha256: String,
}

fn copy_artifact(
    cache: &Path,
    source: &Instance,
    filename: &str,
    expected: &str,
    size: u64,
) -> LiveResult<()> {
    let bytes = read(&safe_path(cache, filename)?, size)?;
    require(
        bytes.len() as u64 == size && digest(&bytes) == expected,
        "live_artifact_identity",
    )?;
    checked(
        recovery::write_new(
            &safe_path(source.root(), &format!("mods/{filename}"))?,
            &bytes,
        ),
        "live_artifact_copy",
    )
}
fn make_copy(
    lab: &Path,
    voice_mod: VoiceMod,
    original: &[u8],
) -> LiveResult<(Instance, Instance, String)> {
    let native = safe_path(lab, "native")?;
    absent(&native)?;
    checked(fs::create_dir(&native), "live_native_create")?;
    let _native_pins = checked(recovery::pin_directory(&native), "live_native_pin")?;
    let layout = InstanceLayout::new(&native);
    let source = layout.open(checked(
        InstanceId::parse("fixture-source"),
        "live_source_id",
    )?);
    checked(source.ensure_layout(), "live_source_layout")?;
    let config = safe_path(source.root(), voice_mod.config())?;
    checked(
        fs::create_dir_all(config.parent().ok_or("live_config_parent")?),
        "live_config_directory",
    )?;
    checked(
        recovery::write_new(&config, original),
        "live_baseline_write",
    )?;
    checked(
        recovery::write_new(
            &safe_path(source.root(), "aurora-instance.json")?,
            br#"{"minecraft":"1.21.1","loader":"fabric","versionId":"1.21.1"}"#,
        ),
        "live_runtime_identity_write",
    )?;
    let artifacts = local_directory("AURORA_CONNECT_ARTIFACT_DIR")?;
    let _artifact_pins = checked(
        recovery::pin_directory(&artifacts),
        "live_artifact_directory",
    )?;
    let filename = match voice_mod {
        VoiceMod::SimpleVoiceChat => "voicechat-fabric-1.21.1-2.6.22.jar",
        VoiceMod::PlasmoVoice => "plasmovoice-fabric-1.21.1-2.1.17.jar",
    };
    let (sha256, size) = voice_mod.artifact();
    copy_artifact(&artifacts, &source, filename, sha256, size)?;
    if matches!(voice_mod, VoiceMod::PlasmoVoice) {
        copy_artifact(
            &artifacts,
            &source,
            FABRIC_API_FILE,
            FABRIC_API_HASH,
            2_425_988,
        )?;
    }
    let source_guard = checked(
        acquire_voice_mutation_guard(&source),
        "live_source_process_guard",
    )?;
    let baseline = checked(
        recovery::capture_observation(&source, source_guard.inner()),
        "live_source_observation",
    )?;
    let environment = checked(
        diagnostic_lab::create(&source, source_guard.inner(), None, |_| {}),
        "live_diagnostic_copy",
    )?;
    let copy = layout.open(checked(InstanceId::parse(environment.id), "live_copy_id")?);
    checked(
        diagnostic_lab::ensure_launchable(&copy),
        "live_copy_identity",
    )?;
    drop(source_guard);
    Ok((source, copy, baseline.fingerprint))
}

// Independent assertion helpers: they never produce mutation bytes used by the
// application. The production receipt alone computes/restores the actual file.
fn svc_span(bytes: &[u8]) -> LiveResult<Range<usize>> {
    let text = checked(std::str::from_utf8(bytes), "live_svc_utf8")?;
    let mut found = None;
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches('\n').trim_end_matches('\r');
        if let Some((key, rhs)) = line.split_once('=') {
            if key.trim_matches([' ', '\t']) == "voice_host" {
                require(found.is_none(), "live_svc_duplicate")?;
                let value = rhs.trim_start_matches([' ', '\t']);
                found = Some(offset + line.len() - value.len()..offset + line.len());
            }
        }
        offset += raw.len();
    }
    found.ok_or("live_svc_missing")
}
fn toml_tree(bytes: &[u8]) -> LiveResult<toml::Value> {
    checked(
        toml::from_str(checked(std::str::from_utf8(bytes), "live_plasmo_utf8")?),
        "live_plasmo_parse",
    )
}
fn without_public(mut tree: toml::Value, expected: bool) -> LiveResult<toml::Value> {
    let host = tree
        .get_mut("host")
        .and_then(toml::Value::as_table_mut)
        .ok_or("live_plasmo_host")?;
    let public = host.remove("public");
    if expected {
        let public = public
            .and_then(|value| value.as_table().cloned())
            .ok_or("live_plasmo_public")?;
        require(
            public.len() == 2
                && public.get("ip").and_then(toml::Value::as_str) == Some("127.0.0.1")
                && public.get("port").and_then(toml::Value::as_integer)
                    == Some(i64::from(GUEST_PORT)),
            "live_plasmo_announcement_changed",
        )?;
    } else {
        require(public.is_none(), "live_plasmo_announcement_remains")?;
    }
    Ok(tree)
}
fn assert_applied(voice_mod: VoiceMod, original: &[u8], applied: &[u8]) -> LiveResult<()> {
    match voice_mod {
        VoiceMod::SimpleVoiceChat => {
            let before = svc_span(original)?;
            let after = svc_span(applied)?;
            require(
                original[before.clone()].is_empty()
                    && &applied[after.clone()] == b"127.0.0.1:35554",
                "live_svc_apply_value",
            )?;
            require(
                original[..before.start] == applied[..after.start]
                    && original[before.end..] == applied[after.end..],
                "live_svc_apply_unowned",
            )
        }
        VoiceMod::PlasmoVoice => {
            require(applied.starts_with(original), "live_plasmo_apply_prefix")?;
            require(
                without_public(toml_tree(original)?, false)?
                    == without_public(toml_tree(applied)?, true)?,
                "live_plasmo_apply_unowned",
            )
        }
    }
}
fn plasmo_without_public_code(bytes: &[u8]) -> LiveResult<Vec<u8>> {
    let text = checked(std::str::from_utf8(bytes), "live_plasmo_utf8")?;
    let mut ranges = Vec::new();
    let mut public = false;
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches('\n').trim_end_matches('\r');
        let mut quote = None;
        let mut end = line.len();
        for (index, ch) in line.char_indices() {
            if quote == Some(ch) {
                quote = None;
            } else if quote.is_none() && matches!(ch, '\'' | '"') {
                quote = Some(ch);
            } else if quote.is_none() && ch == '#' {
                end = index;
                break;
            }
        }
        let code = &line[..end];
        let trimmed = code.trim_matches([' ', '\t']);
        let mut owned = false;
        if trimmed.starts_with('[') {
            public = trimmed == "[host.public]";
            owned = public;
        } else if public && !trimmed.is_empty() {
            let (key, _) = trimmed.split_once('=').ok_or("live_plasmo_public_code")?;
            require(
                matches!(key.trim_matches([' ', '\t']), "ip" | "port"),
                "live_plasmo_public_extra",
            )?;
            owned = true;
        }
        if owned {
            ranges.push(
                offset + code.len() - code.trim_start_matches([' ', '\t']).len()
                    ..offset + code.trim_end_matches([' ', '\t']).len(),
            );
        }
        offset += raw.len();
    }
    require(ranges.len() == 3, "live_plasmo_public_shape")?;
    let mut expected = bytes.to_vec();
    for range in ranges.into_iter().rev() {
        // Spaces/tabs inside the removed syntax also belong to the writer.
        // Preserve them independently of the production parser's span logic.
        let retained: Vec<u8> = bytes[range.clone()]
            .iter()
            .copied()
            .filter(|byte| matches!(byte, b' ' | b'\t'))
            .collect();
        expected.splice(range, retained);
    }
    Ok(expected)
}
fn assert_restored(
    voice_mod: VoiceMod,
    mode: &str,
    original: &[u8],
    writer: &[u8],
    restored: &[u8],
) -> LiveResult<()> {
    if mode == "strict" {
        return require(restored == original, "live_strict_restore_bytes");
    }
    match voice_mod {
        VoiceMod::SimpleVoiceChat => {
            let before = svc_span(writer)?;
            let after = svc_span(restored)?;
            require(
                &writer[before.clone()] == b"127.0.0.1:35554" && restored[after.clone()].is_empty(),
                "live_svc_restore_value",
            )?;
            require(
                writer[..before.start] == restored[..after.start]
                    && writer[before.end..] == restored[after.end..],
                "live_svc_restore_unowned",
            )
        }
        VoiceMod::PlasmoVoice => {
            require(
                without_public(toml_tree(writer)?, true)?
                    == without_public(toml_tree(restored)?, false)?,
                "live_plasmo_restore_unowned_values",
            )?;
            require(
                plasmo_without_public_code(writer)? == restored,
                "live_plasmo_restore_unowned_bytes",
            )
        }
    }
}

fn run_live() -> LiveResult<()> {
    let deadline = Instant::now() + Duration::from_secs(480);
    let (profile, voice_mod, original, original_hash) =
        match std::env::var("AURORA_CONNECT_LIVE_MOD").ok().as_deref() {
            Some("svc") => (
                "svc",
                VoiceMod::SimpleVoiceChat,
                SVC_BASELINE,
                SVC_BASELINE_HASH,
            ),
            Some("plasmo") => (
                "plasmo",
                VoiceMod::PlasmoVoice,
                PLASMO_BASELINE,
                PLASMO_BASELINE_HASH,
            ),
            _ => return Err("live_profile_required"),
        };
    require(
        digest(original) == original_hash,
        "live_compiled_baseline_hash",
    )?;
    let (lab, ipc) = Ipc::open(deadline)?;
    let result = (|| {
        let (source, copy, source_fingerprint) = make_copy(&lab, voice_mod, original)?;
        ipc.check()?;
        let config = safe_path(copy.root(), voice_mod.config())?;
        require(read(&config, MAX_CONFIG)? == original, "live_copy_baseline")?;
        let guard = checked(
            acquire_voice_mutation_guard(&copy),
            "live_apply_process_guard",
        )?;
        let plan = checked(
            prepare_announcement(
                &copy,
                &guard,
                AnnouncementRequest {
                    voice_mod,
                    mode: AnnouncementMode::DedicatedAuroraOnly,
                    guest_port: GUEST_PORT,
                    expected_source_sha256: original_hash.to_owned(),
                },
            ),
            "live_apply_prepare",
        )?;
        let approval = plan.review().digest.clone();
        ipc.publish("apply-review.json", serde_json::json!({ "event": "native_apply_review", "profile": profile,
        "instanceId": copy.id().as_str(), "digest": approval, "sourceSha256": plan.review().source_sha256,
        "replacementSha256": plan.review().replacement_sha256 }))?;
        ipc.approval("apply-approval.json", &approval)?;
        ipc.check()?;
        let mut applied = checked(plan.confirm(&copy, &guard, &approval), "live_apply_confirm")?;
        let applied_bytes = read(&config, MAX_CONFIG)?;
        assert_applied(voice_mod, original, &applied_bytes)?;
        let applied_hash = digest(&applied_bytes);
        drop(guard);
        let launch_lock = checked(acquire_instance_mutation_lock(&copy), "live_launch_lock")?;
        checked(
            diagnostic_lab::ensure_launchable(&copy),
            "live_launch_copy_identity",
        )?;
        ipc.publish(
            "lock-held.json",
            serde_json::json!({ "event": "native_lock_held", "profile": profile,
        "instanceId": copy.id().as_str(), "configSha256": applied_hash }),
        )?;
        let closed: JavaClosed = ipc.wait("java-closed.json")?;
        require(
            closed.closed
                && closed.exit_code == 0
                && !closed.forced
                && closed.eula_restored
                && valid_digest(&closed.config_sha256),
            "live_java_close_rejected",
        )?;
        require(
            read(&safe_path(copy.root(), "eula.txt")?, IPC_MAX)? == FALSE_EULA,
            "live_eula_not_restored",
        )?;
        let writer = read(&config, MAX_CONFIG)?;
        let writer_hash = digest(&writer);
        require(
            writer_hash == closed.config_sha256,
            "live_writer_hash_changed",
        )?;
        drop(launch_lock);
        ipc.check()?;
        let restore_guard = checked(
            acquire_voice_mutation_guard(&copy),
            "live_restore_process_guard",
        )?;
        let before_restore = checked(
            recovery::capture_observation(&copy, restore_guard.inner()),
            "live_writer_observation",
        )?;
        // A stale strict preview is the only reason to OFFER a reconciled review.
        // Nothing is restored until Node approves the new, separate digest below.
        let mode = match applied.receipt.prepare_restore(&copy, &restore_guard) {
            Ok(preview) => {
                drop(preview);
                "strict"
            }
            Err(RecoveryError::StalePreview) => "reconciled",
            Err(_) => return Err("live_strict_restore_rejected"),
        };
        let restore = if mode == "strict" {
            checked(
                applied.receipt.prepare_restore(&copy, &restore_guard),
                "live_strict_restore_prepare",
            )?
        } else {
            checked(
                applied
                    .receipt
                    .prepare_reconciled_restore(&copy, &restore_guard, &writer_hash),
                "live_reconciled_restore_prepare",
            )?
        };
        let restore_approval = restore.review().digest.clone();
        ipc.publish(
            "restore-review.json",
            serde_json::json!({ "event": "native_restore_review", "profile": profile,
        "mode": mode, "digest": restore_approval, "sourceSha256": restore.review().source_sha256,
        "replacementSha256": restore.review().replacement_sha256 }),
        )?;
        ipc.approval("restore-approval.json", &restore_approval)?;
        ipc.check()?;
        checked(
            restore.confirm(&copy, &restore_guard, &restore_approval),
            "live_restore_confirm",
        )?;
        let restored = read(&config, MAX_CONFIG)?;
        assert_restored(voice_mod, mode, original, &writer, &restored)?;
        require(
            applied.receipt.state() == RestorationState::Restored,
            "live_receipt_state",
        )?;
        let after_restore = checked(
            recovery::capture_observation(&copy, restore_guard.inner()),
            "live_restored_observation",
        )?;
        let mut expected_files = before_restore.files;
        let target = expected_files
            .iter_mut()
            .find(|file| file.path == voice_mod.config())
            .ok_or("live_inventory_target")?;
        target.sha256 = digest(&restored);
        target.size = restored.len() as u64;
        require(
            after_restore.files == expected_files
                && after_restore.runtime == before_restore.runtime
                && after_restore.profile == before_restore.profile,
            "live_unowned_inventory_changed",
        )?;
        require(
            !checked(recovery::has_pending(&copy), "live_pending_read")?,
            "live_pending_recovery",
        )?;
        require(
            names(&safe_path(copy.root(), ".aurora/connect-voice-stage")?)?.is_empty(),
            "live_stage_not_empty",
        )?;
        require(
            matches!(
                acquire_instance_mutation_lock(&copy),
                Err(ContentUpdateError::Busy)
            ),
            "live_restore_lock_missing",
        )?;
        let source_guard = checked(acquire_instance_recovery_lock(&source), "live_origin_lock")?;
        let source_after = checked(
            recovery::capture_observation(&source, &source_guard),
            "live_origin_observation",
        )?;
        require(
            source_after.fingerprint == source_fingerprint
                && read(&safe_path(source.root(), voice_mod.config())?, MAX_CONFIG)? == original,
            "live_origin_changed",
        )?;
        drop(source_guard);
        drop(restore_guard);
        drop(checked(
            acquire_instance_mutation_lock(&copy),
            "live_final_launch_lock",
        )?);
        ipc.publish("restored.json", serde_json::json!({ "event": "native_restored", "profile": profile, "mode": mode,
        "sourceSha256": original_hash, "appliedSha256": applied_hash, "writerSha256": writer_hash,
        "restoredSha256": digest(&restored), "sourceUnchanged": true, "unownedPreserved": true, "pending": false }))?;
        wait_stop(&ipc)
    })();
    if let Err(code) = result {
        // Only our static error code is persisted, never Debug/config content.
        let _ = ipc.publish(
            "failed.json",
            serde_json::json!({"event": "native_failed", "code": code}),
        );
    }
    result
}

#[cfg(windows)]
fn wait_stop(ipc: &Ipc) -> LiveResult<()> {
    use std::{ffi::c_void, os::windows::io::AsRawHandle};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn PeekNamedPipe(
            pipe: *mut c_void,
            buffer: *mut c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            remaining: *mut u32,
        ) -> i32;
        fn ReadFile(
            file: *mut c_void,
            buffer: *mut c_void,
            count: u32,
            read: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
    }
    let stdin = std::io::stdin();
    let handle = stdin.as_raw_handle();
    let mut bytes = Vec::new();
    loop {
        ipc.check()?;
        let mut available = 0_u32;
        // SAFETY: sole stdin reader, no pending I/O; the borrowed pipe handle and
        // output pointer remain valid. Peek never consumes data or closes it.
        let ok = unsafe {
            PeekNamedPipe(
                handle,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(109) {
                return require(
                    bytes == b"stop\n" || bytes == b"stop\r\n",
                    "live_stop_input",
                );
            }
            return Err("live_stop_pipe");
        }
        require(
            available as usize <= 6 - bytes.len(),
            "live_stop_input_limit",
        )?;
        if available != 0 {
            let mut buffer = [0_u8; 6];
            let mut read = 0_u32;
            // SAFETY: read only bytes Peek observed as immediately available,
            // bounded by this six-byte buffer, without a second reader/thread.
            let ok = unsafe {
                ReadFile(
                    handle,
                    buffer.as_mut_ptr().cast(),
                    available,
                    &mut read,
                    std::ptr::null_mut(),
                )
            };
            require(ok != 0 && read > 0 && read <= available, "live_stop_read")?;
            bytes.extend_from_slice(&buffer[..read as usize]);
            require(
                b"stop\n".starts_with(&bytes) || b"stop\r\n".starts_with(&bytes),
                "live_stop_input",
            )?;
        } else {
            thread::sleep(POLL.min(ipc.deadline.saturating_duration_since(Instant::now())));
        }
    }
}
#[cfg(not(windows))]
fn wait_stop(_: &Ipc) -> LiveResult<()> {
    Err("live_windows_required")
}

#[test]
#[ignore = "explicit confined live lab and pinned artifacts; real process guard and Node-supervised JVM writer; never run in general include-ignored suites"]
fn native_live_writer_roundtrip() {
    if let Err(code) = run_live() {
        panic!("{code}");
    }
}

#[test]
fn live_compiled_baselines_match_reviewed_runtime_profiles() {
    assert!(SVC_BASELINE.len() == 176 && digest(SVC_BASELINE) == SVC_BASELINE_HASH);
    assert!(PLASMO_BASELINE.len() == 164 && digest(PLASMO_BASELINE) == PLASMO_BASELINE_HASH);
}

#[test]
fn live_ipc_messages_refuse_unknown_duplicate_and_wrongly_typed_fields() {
    for bytes in [
        br#"{"digest":"a","path":"elsewhere"}"#.as_slice(),
        br#"{"digest":"a","digest":"b"}"#,
        br#"{"digest":null}"#,
    ] {
        assert!(serde_json::from_slice::<Approval>(bytes).is_err());
    }
    for bytes in [
        br#"{"closed":true,"exitCode":0,"forced":false,"eulaRestored":true,"configSha256":"a","extra":true}"#.as_slice(),
        br#"{"closed":true,"exitCode":0.0,"forced":false,"eulaRestored":true,"configSha256":"a"}"#,
        br#"{"closed":"true","exitCode":0,"forced":false,"eulaRestored":true,"configSha256":"a"}"#,
        br#"{"closed":true,"exitCode":0,"forced":false,"configSha256":"a"}"#,
    ] {
        assert!(serde_json::from_slice::<JavaClosed>(bytes).is_err());
    }
    assert!(valid_digest(&"a".repeat(64)));
    assert!(!valid_digest(&"A".repeat(64)));
    assert!(!valid_digest(&"a".repeat(63)));
}

#[test]
fn live_independent_restoration_assertions_detect_unowned_changes() {
    // Pure assertion fixtures, not evidence that a mod or JVM wrote these bytes.
    let current_svc = b"# writer\r\nport=43002\r\nvoice_host=127.0.0.1:35554\r\nmtu_size=1200\r\n";
    let restored_svc = b"# writer\r\nport=43002\r\nvoice_host=\r\nmtu_size=1200\r\n";
    assert!(assert_restored(
        VoiceMod::SimpleVoiceChat,
        "reconciled",
        SVC_BASELINE,
        current_svc,
        restored_svc
    )
    .is_ok());
    assert!(assert_restored(
        VoiceMod::SimpleVoiceChat,
        "reconciled",
        SVC_BASELINE,
        current_svc,
        b"# writer\r\nport=43002\r\nvoice_host=\r\nmtu_size=1275\r\n"
    )
    .is_err());
    let current_plasmo = b"debug = false\r\n[host]\r\nip = '127.0.0.1'\r\nport = 43003\r\n[host.public] # announcement\r\nip = \"127.0.0.1\" # address\r\nport = 35554 # port\r\n[voice]\r\nmtu_size = 1000\r\n";
    let restored_plasmo = b"debug = false\r\n[host]\r\nip = '127.0.0.1'\r\nport = 43003\r\n # announcement\r\n   # address\r\n   # port\r\n[voice]\r\nmtu_size = 1000\r\n";
    assert!(assert_restored(
        VoiceMod::PlasmoVoice,
        "reconciled",
        PLASMO_BASELINE,
        current_plasmo,
        restored_plasmo
    )
    .is_ok());
    let dropped_comment = String::from_utf8_lossy(restored_plasmo).replace(" # address", "");
    assert!(assert_restored(
        VoiceMod::PlasmoVoice,
        "reconciled",
        PLASMO_BASELINE,
        current_plasmo,
        dropped_comment.as_bytes()
    )
    .is_err());
    let dropped_spacing =
        String::from_utf8_lossy(restored_plasmo).replace("   # address", " # address");
    assert!(assert_restored(
        VoiceMod::PlasmoVoice,
        "reconciled",
        PLASMO_BASELINE,
        current_plasmo,
        dropped_spacing.as_bytes()
    )
    .is_err());
}
