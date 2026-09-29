//! Experimental, explicitly compiled SVC configuration adapter for verified
//! diagnostic copies only. No Tauri command or UI exposes this module.
//! Callers must verify the game is stopped before/after acquiring the shared
//! mutation lock, just as for recovery. Never infer this from a transport event.
use super::recovery::{self, RecoveryError, RecoveryResult};
use super::{diagnostic_lab, Instance, InstanceMutationGuard};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const CONFIG: &str = "config/voicechat/voicechat-server.properties";
const SVC_SHA256: &str = "972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36";
const SVC_BYTES: u64 = 5_369_475;
const MAX_CONFIG: u64 = 256 * 1024;

type Result<T> = std::result::Result<T, RecoveryError>;

#[derive(Clone, Copy, Debug)]
pub enum AnnouncementMode {
    /// Global loopback advertisement: every participant, including the host's
    /// own client, needs its approved local bridge. No mixed/LAN mode is implied.
    DedicatedAuroraOnly,
}

pub struct AnnouncementRequest {
    pub mode: AnnouncementMode,
    pub guest_port: u16,
    /// Digest of the exact source configuration reviewed by the local operator.
    pub expected_source_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementReview {
    pub plan_id: String,
    pub digest: String,
    pub config_path: String,
    pub before: String,
    pub after: String,
    pub source_sha256: String,
    pub expires_at_ms: u64,
    pub warning: String,
}

/// In-memory approval capability; not deserializable and consumed by confirmation.
/// Preparation never publishes a recovery point or preview: those would remain
/// reachable through generic recovery after cancellation or expiry. Only a
/// successful confirmation may hand the approved bytes to that executor.
pub struct PreparedAnnouncement {
    review: AnnouncementReview,
    replacement: Vec<u8>,
    fingerprint: String,
    instance_uuid: String,
    deadline: Instant,
}
impl PreparedAnnouncement {
    pub fn review(&self) -> &AnnouncementReview {
        &self.review
    }

    pub fn confirm(
        self,
        instance: &Instance,
        guard: &InstanceMutationGuard,
        approved_digest: &str,
    ) -> Result<RecoveryResult> {
        if Instant::now() >= self.deadline || approved_digest != self.review.digest {
            return Err(RecoveryError::StalePreview);
        }
        require_test_copy(instance)?;
        let current = recovery::capture_observation(instance, guard)?;
        if current.instance_uuid != self.instance_uuid {
            return Err(RecoveryError::Identity);
        }
        if current.fingerprint != self.fingerprint {
            return Err(RecoveryError::StalePreview);
        }
        let _pins = recovery::check_guard(instance, guard)?;
        let stage = recovery::path(instance.root(), ".aurora/connect-voice-stage")?;
        fs::create_dir_all(&stage)?;
        let _stage_pins = recovery::pin_directory(&stage)?;
        let staged = recovery::path(&stage, &format!("{}.properties", Uuid::new_v4()))?;
        recovery::write_new(&staged, &self.replacement)?;
        let result = recovery::preview_selected_files(
            instance,
            guard,
            &self.fingerprint,
            &BTreeMap::from([(
                CONFIG.to_owned(),
                Some(recovery::SelectedFile {
                    source: staged.clone(),
                    sha256: digest(&self.replacement),
                    size: self.replacement.len() as u64,
                }),
            )]),
            "Anúncio SVC experimental na cópia de teste",
        );
        // Only our unique staging file; approved CAS/preview remains for audit/undo.
        let _ = fs::remove_file(staged);
        let preview = result?;
        // Recovery revalidates the persisted preview, hashes, identity and entire
        // observed state; it captures protection before its durable transaction.
        recovery::restore(instance, guard, &preview.id)
    }
}

fn require_test_copy(instance: &Instance) -> Result<()> {
    if !diagnostic_lab::is_lab_candidate(instance) {
        return Err(RecoveryError::UnsafePath);
    }
    diagnostic_lab::ensure_launchable(instance)
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Render only an existing, unambiguous voice_host property. Preserve every
/// other byte, comments and line endings. This is not a general properties writer.
fn render_svc(bytes: &[u8], port: u16) -> Result<(Vec<u8>, String, String)> {
    if port == 0 || bytes.len() > MAX_CONFIG as usize {
        return Err(RecoveryError::Integrity);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| RecoveryError::Integrity)?;
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
        || text.replace("\r\n", "").contains('\r')
    {
        return Err(RecoveryError::Integrity);
    }
    let mut keys = std::collections::BTreeSet::new();
    let mut span = None;
    let mut at = 0;
    for (index, raw) in text.split_inclusive('\n').enumerate() {
        if index >= 4096 {
            return Err(RecoveryError::Limit);
        }
        let line = raw.trim_end_matches('\n').trim_end_matches('\r');
        let trimmed = line.trim_start_matches([' ', '\t']);
        if !trimmed.is_empty() && !trimmed.starts_with(['#', '!']) {
            if line.contains('\\') {
                return Err(RecoveryError::Integrity);
            }
            let equals = line.find('=').ok_or(RecoveryError::Integrity)?;
            let key = line[..equals].trim_matches([' ', '\t']);
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                || !keys.insert(key)
                || keys.len() > 512
            {
                return Err(RecoveryError::Integrity);
            }
            if key == "voice_host" {
                let value = line[equals + 1..].trim_start_matches([' ', '\t']);
                if value.starts_with(char::is_whitespace) {
                    return Err(RecoveryError::Integrity);
                }
                let value_start = equals + 1 + line[equals + 1..].len() - value.len();
                span = Some((at + value_start, at + line.len()));
            }
        }
        at += raw.len();
    }
    let (start, end) = span.ok_or(RecoveryError::Integrity)?;
    let before = text[start..end].to_owned();
    let after = format!("127.0.0.1:{port}");
    if before == after {
        return Err(RecoveryError::Integrity);
    }
    let mut rendered = Vec::with_capacity(bytes.len() + after.len());
    rendered.extend_from_slice(&bytes[..start]);
    rendered.extend_from_slice(after.as_bytes());
    rendered.extend_from_slice(&bytes[end..]);
    Ok((rendered, before, after))
}

pub fn prepare_announcement(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    request: AnnouncementRequest,
) -> Result<PreparedAnnouncement> {
    require_test_copy(instance)?;
    let AnnouncementMode::DedicatedAuroraOnly = request.mode;
    if request.expected_source_sha256.len() != 64
        || !request
            .expected_source_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return Err(RecoveryError::Integrity);
    }
    let current = recovery::capture_observation(instance, guard)?;
    // Runtime claims must agree; the adapter does not infer a loader from a filename.
    let minecraft: Vec<_> = current
        .runtime
        .iter()
        .filter(|(key, _)| key.ends_with(":minecraft"))
        .map(|(_, value)| value.as_str())
        .collect();
    let loaders: Vec<_> = current
        .runtime
        .iter()
        .filter(|(key, _)| key.ends_with(":loader"))
        .map(|(_, value)| value.as_str())
        .collect();
    if minecraft.is_empty()
        || loaders.is_empty()
        || minecraft.iter().any(|value| *value != "1.21.1")
        || loaders.iter().any(|value| *value != "fabric")
    {
        return Err(RecoveryError::Compatibility);
    }
    if current
        .files
        .iter()
        .filter(|file| {
            file.path.starts_with("mods/")
                && file.path.ends_with(".jar")
                && file.sha256 == SVC_SHA256
                && file.size == SVC_BYTES
        })
        .count()
        != 1
    {
        return Err(RecoveryError::Compatibility);
    }
    let source = current
        .files
        .iter()
        .find(|file| file.path == CONFIG)
        .ok_or(RecoveryError::UnsafePath)?;
    if !request
        .expected_source_sha256
        .eq_ignore_ascii_case(&source.sha256)
    {
        return Err(RecoveryError::StalePreview);
    }
    let bytes = recovery::read_bounded(&recovery::path(instance.root(), CONFIG)?, MAX_CONFIG)?;
    if digest(&bytes) != source.sha256 {
        return Err(RecoveryError::Changed);
    }
    let (replacement, before, after) = render_svc(&bytes, request.guest_port)?;
    let expires_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RecoveryError::Integrity)?
        .as_millis() as u64
        + 300_000;
    let mut review = AnnouncementReview {
        plan_id: Uuid::new_v4().to_string(), digest: String::new(), config_path: CONFIG.into(), before, after,
        source_sha256: source.sha256.clone(), expires_at_ms,
        warning: "Experimental, somente cópia dedicada Aurora. Todos os participantes precisam de ponte local; transporte, Java, instância e áudio não foram homologados. Não aplicar à origem por este adaptador.".into(),
    };
    review.digest = digest(
        &serde_json::to_vec(&(&review, &current.instance_uuid, &current.fingerprint))
            .map_err(|_| RecoveryError::Integrity)?,
    );
    Ok(PreparedAnnouncement {
        review,
        replacement,
        fingerprint: current.fingerprint,
        instance_uuid: current.instance_uuid,
        deadline: Instant::now() + Duration::from_secs(300),
    })
}

#[cfg(test)]
#[path = "connect_voice_tests.rs"]
mod tests;
