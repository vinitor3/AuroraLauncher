//! Experimental, explicitly compiled voice configuration adapters for verified
//! diagnostic copies only. The opt-in desktop caller retains plans and receipts
//! privately; commands expose bounded presentation and explicit approval only.
//! The opaque guard checks for standard Java processes around the shared lock
//! and confirmation. Callers still must prevent external starts during mutation;
//! renamed/embedded JVMs are not detectable by this conservative lab gate.
use super::recovery::{self, RecoveryError, RecoveryResult};
use super::{diagnostic_lab, Instance};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

#[cfg(all(windows, feature = "connect-lab"))]
#[path = "connect_dedicated.rs"]
pub(crate) mod dedicated;

const CONFIG: &str = "config/voicechat/voicechat-server.properties";
const SVC_SHA256: &str = "972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36";
const SVC_BYTES: u64 = 5_369_475;
const PLASMO_CONFIG: &str = "config/plasmovoice/server/config.toml";
const PLASMO_SHA256: &str = "46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b";
const PLASMO_BYTES: u64 = 12_365_966;
const MAX_CONFIG: u64 = 256 * 1024;

type Result<T> = std::result::Result<T, RecoveryError>;

#[cfg(windows)]
#[path = "connect_voice_desktop_live.rs"]
mod desktop;
#[cfg(windows)]
pub(crate) use desktop::Manager as DesktopManager;

#[path = "connect_voice_inspection.rs"]
mod inspection;
pub use inspection::{inspect_announcement, AnnouncementInspection, ObservedAnnouncement};

#[path = "connect_voice_plasmo.rs"]
mod plasmo;
#[path = "connect_voice_process.rs"]
mod process;
pub use process::{acquire_voice_mutation_guard, VoiceMutationGuard};
#[path = "connect_voice_restore.rs"]
mod restoration;
#[cfg(test)]
pub(super) use restoration::prepare_fixture_durable_announcement;
pub use restoration::{
    list_restoration_receipts, prepare_durable_announcement, prepare_retirement,
    resume_restoration, DurableAppliedAnnouncement, DurableReceiptState,
    DurableRestorationController, DurableRestorationSummary, PreparedDurableAnnouncement,
    PreparedRestoration, PreparedRetirement, RestorationController, RestorationReceipt,
    RestorationState, RetirementReview,
};

pub struct AppliedAnnouncement {
    pub recovery: RecoveryResult,
    /// Session-local selective restoration; dropping it never writes files.
    pub receipt: RestorationReceipt,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnnouncementAction {
    SetAnnouncement,
    RestoreOriginal { receipt_id: String },
    RestoreReconciled { receipt_id: String },
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMod {
    SimpleVoiceChat,
    PlasmoVoice,
}
impl VoiceMod {
    fn config(self) -> &'static str {
        match self {
            Self::SimpleVoiceChat => CONFIG,
            Self::PlasmoVoice => PLASMO_CONFIG,
        }
    }
    fn artifact(self) -> (&'static str, u64) {
        match self {
            Self::SimpleVoiceChat => (SVC_SHA256, SVC_BYTES),
            Self::PlasmoVoice => (PLASMO_SHA256, PLASMO_BYTES),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum AnnouncementMode {
    /// Global loopback advertisement: every participant, including the host's
    /// own client, needs its approved local bridge. No mixed/LAN mode is implied.
    DedicatedAuroraOnly,
}

pub struct AnnouncementRequest {
    pub voice_mod: VoiceMod,
    pub mode: AnnouncementMode,
    pub guest_port: u16,
    /// Digest of the exact source configuration reviewed by the local operator.
    pub expected_source_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementReview {
    pub action: AnnouncementAction,
    pub voice_mod: VoiceMod,
    pub plan_id: String,
    pub digest: String,
    pub config_path: String,
    pub before: String,
    pub after: String,
    pub source_sha256: String,
    pub replacement_sha256: String,
    pub expires_at_ms: u64,
    pub warning: String,
}

/// In-memory approval capability; not deserializable and consumed by confirmation.
/// Preparation never publishes a recovery point or preview: those would remain
/// reachable through generic recovery after cancellation or expiry. Only a
/// successful confirmation may hand the approved bytes to that executor.
pub struct PreparedAnnouncement {
    voice_mod: VoiceMod,
    review: AnnouncementReview,
    replacement: Vec<u8>,
    original: Vec<u8>,
    runtime: BTreeMap<String, String>,
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
        guard: &VoiceMutationGuard,
        approved_digest: &str,
    ) -> Result<AppliedAnnouncement> {
        // Construct the opaque receipt before commit. Never perform fallible I/O
        // afterwards and accidentally report failure for a completed mutation.
        let receipt = RestorationReceipt::from_plan(&self);
        let recovery = self.apply(instance, guard, approved_digest, || {})?;
        Ok(AppliedAnnouncement { recovery, receipt })
    }

    fn apply(
        self,
        instance: &Instance,
        guard: &VoiceMutationGuard,
        approved_digest: &str,
        on_executor_begin: impl FnOnce(),
    ) -> Result<RecoveryResult> {
        self.apply_using(
            instance,
            guard,
            approved_digest,
            on_executor_begin,
            |preview_id| recovery::restore(instance, guard.inner(), preview_id),
        )
    }

    fn apply_using<T>(
        self,
        instance: &Instance,
        guard: &VoiceMutationGuard,
        approved_digest: &str,
        on_executor_begin: impl FnOnce(),
        execute: impl FnOnce(&str) -> Result<T>,
    ) -> Result<T> {
        if Instant::now() >= self.deadline || approved_digest != self.review.digest {
            return Err(RecoveryError::StalePreview);
        }
        guard.recheck()?;
        require_test_copy(instance)?;
        let current = recovery::capture_observation(instance, guard.inner())?;
        if current.instance_uuid != self.instance_uuid {
            return Err(RecoveryError::Identity);
        }
        if current.fingerprint != self.fingerprint {
            return Err(RecoveryError::StalePreview);
        }
        // Inventory hashing may take time; do not reuse a pre-review or
        // pre-inventory observation of Java process absence for the write.
        guard.recheck()?;
        if Instant::now() >= self.deadline {
            return Err(RecoveryError::StalePreview);
        }
        on_executor_begin();
        let _pins = recovery::check_guard(instance, guard.inner())?;
        let stage = recovery::path(instance.root(), ".aurora/connect-voice-stage")?;
        fs::create_dir_all(&stage)?;
        let _stage_pins = recovery::pin_directory(&stage)?;
        let staged = recovery::path(&stage, &format!("{}.config", Uuid::new_v4()))?;
        recovery::write_new(&staged, &self.replacement)?;
        let result = recovery::preview_selected_files(
            instance,
            guard.inner(),
            &self.fingerprint,
            &BTreeMap::from([(
                self.voice_mod.config().to_owned(),
                Some(recovery::SelectedFile {
                    source: staged.clone(),
                    sha256: digest(&self.replacement),
                    size: self.replacement.len() as u64,
                }),
            )]),
            match &self.review.action {
                AnnouncementAction::SetAnnouncement => {
                    "Anúncio de voz experimental na cópia de teste"
                }
                AnnouncementAction::RestoreOriginal { .. } => {
                    "Restaurar anúncio de voz da cópia de teste"
                }
                AnnouncementAction::RestoreReconciled { .. } => {
                    "Restaurar somente as chaves de anúncio revisadas na cópia de teste"
                }
            },
        );
        // Only our unique staging file; approved CAS/preview remains for audit/undo.
        let _ = fs::remove_file(staged);
        let preview = result?;
        // Snapshot/CAS work may itself be slow. A process appearing during it
        // blocks the actual file transaction and leaves recovery inspection.
        guard.recheck()?;
        // Recovery revalidates the persisted preview, hashes, identity and entire
        // observed state; it captures protection before its durable transaction.
        execute(&preview.id)
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
fn svc_value_span(bytes: &[u8]) -> Result<std::ops::Range<usize>> {
    if bytes.len() > MAX_CONFIG as usize {
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
    Ok(start..end)
}

fn render_svc(bytes: &[u8], port: u16) -> Result<(Vec<u8>, String, String)> {
    if port == 0 {
        return Err(RecoveryError::Integrity);
    }
    let span = svc_value_span(bytes)?;
    let (start, end) = (span.start, span.end);
    let text = std::str::from_utf8(bytes).map_err(|_| RecoveryError::Integrity)?;
    let before = text[start..end].to_owned();
    let after = format!("127.0.0.1:{port}");
    if before == after {
        return Err(RecoveryError::Integrity);
    }
    if bytes.len() - (end - start) + after.len() > MAX_CONFIG as usize {
        return Err(RecoveryError::Limit);
    }
    let mut rendered = Vec::with_capacity(bytes.len() + after.len());
    rendered.extend_from_slice(&bytes[..start]);
    rendered.extend_from_slice(after.as_bytes());
    rendered.extend_from_slice(&bytes[end..]);
    Ok((rendered, before, after))
}

pub fn prepare_announcement(
    instance: &Instance,
    guard: &VoiceMutationGuard,
    request: AnnouncementRequest,
) -> Result<PreparedAnnouncement> {
    let artifact = request.voice_mod.artifact();
    prepare_announcement_with_artifact(instance, guard, request, artifact)
}

fn prepare_announcement_with_artifact(
    instance: &Instance,
    guard: &VoiceMutationGuard,
    request: AnnouncementRequest,
    (artifact_hash, artifact_bytes): (&str, u64),
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
    guard.recheck()?;
    let current = recovery::capture_observation(instance, guard.inner())?;
    let config = request.voice_mod.config();
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
                && file.sha256 == artifact_hash
                && file.size == artifact_bytes
        })
        .count()
        != 1
    {
        return Err(RecoveryError::Compatibility);
    }
    let source = current
        .files
        .iter()
        .find(|file| file.path == config)
        .ok_or(RecoveryError::UnsafePath)?;
    if !request
        .expected_source_sha256
        .eq_ignore_ascii_case(&source.sha256)
    {
        return Err(RecoveryError::StalePreview);
    }
    let bytes = recovery::read_bounded(&recovery::path(instance.root(), config)?, MAX_CONFIG)?;
    if digest(&bytes) != source.sha256 {
        return Err(RecoveryError::Changed);
    }
    let (replacement, before, after) = match request.voice_mod {
        VoiceMod::SimpleVoiceChat => render_svc(&bytes, request.guest_port)?,
        VoiceMod::PlasmoVoice => plasmo::render(&bytes, request.guest_port)?,
    };
    prepare_change(
        request.voice_mod,
        current,
        bytes,
        replacement,
        before,
        after,
        AnnouncementAction::SetAnnouncement,
    )
}

fn prepare_change(
    voice_mod: VoiceMod,
    current: recovery::RecoveryObservation,
    original: Vec<u8>,
    replacement: Vec<u8>,
    before: String,
    after: String,
    action: AnnouncementAction,
) -> Result<PreparedAnnouncement> {
    // Both directions must remain readable by the bounded restoration adapter.
    if original.len() > MAX_CONFIG as usize || replacement.len() > MAX_CONFIG as usize {
        return Err(RecoveryError::Limit);
    }
    let expires_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RecoveryError::Integrity)?
        .as_millis() as u64
        + 300_000;
    let warning = match &action {
        AnnouncementAction::SetAnnouncement => "Experimental, somente cópia dedicada Aurora. Todos os participantes precisam de ponte local; transporte, Java, instância e áudio não foram homologados. Não aplicar à origem por este adaptador.",
        AnnouncementAction::RestoreOriginal { .. } => "Restauração experimental de um único arquivo na cópia de teste. Exige que ele ainda tenha os bytes aplicados; outras mudanças anteriores a este plano serão preservadas. O recibo não sobrevive ao encerramento do processo.",
        AnnouncementAction::RestoreReconciled { .. } => "Restauração experimental somente das chaves de anúncio, com nova revisão e aprovação do arquivo atual. As chaves precisam continuar com os valores aplicados; os demais bytes atuais serão preservados. Não é restauração integral do arquivo original nem limpeza automática. O recibo existe somente neste processo.",
    };
    let mut review = AnnouncementReview {
        action,
        voice_mod,
        plan_id: Uuid::new_v4().to_string(),
        digest: String::new(),
        config_path: voice_mod.config().into(),
        before,
        after,
        source_sha256: digest(&original),
        replacement_sha256: digest(&replacement),
        expires_at_ms,
        warning: warning.into(),
    };
    review.digest = digest(
        &serde_json::to_vec(&(&review, &current.instance_uuid, &current.fingerprint))
            .map_err(|_| RecoveryError::Integrity)?,
    );
    Ok(PreparedAnnouncement {
        voice_mod,
        review,
        replacement,
        original,
        runtime: current.runtime,
        fingerprint: current.fingerprint,
        instance_uuid: current.instance_uuid,
        deadline: Instant::now() + Duration::from_secs(300),
    })
}

#[cfg(test)]
#[path = "connect_voice_tests.rs"]
mod tests;
