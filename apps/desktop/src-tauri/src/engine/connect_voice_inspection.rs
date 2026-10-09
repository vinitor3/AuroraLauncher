//! Bounded local observation for a later, independently approved announcement.
//! No plan, receipt, listener, mutation or proof of loaded/runtime voice exists.
use super::*;

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ObservedAnnouncement {
    SimpleVoiceChat {
        #[serde(rename = "voiceHost")]
        voice_host: String,
    },
    PlasmoAbsent,
    PlasmoPublic {
        ip: String,
        port: u16,
    },
}

/// Local presentation only. Contains the explicitly selected configured address;
/// never include this object in shared diagnostics or a remote peer message.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementInspection {
    pub voice_mod: VoiceMod,
    pub artifact_version: &'static str,
    pub artifact_sha256: &'static str,
    pub artifact_bytes: u64,
    pub config_path: &'static str,
    pub source_sha256: String,
    pub source_bytes: u64,
    pub announcement: ObservedAnnouncement,
    pub limitations: Vec<&'static str>,
}

/// The real opaque guard must be acquired independently. This function does not
/// recover journals, create a missing file or retain the guard after returning.
/// A future prepare_announcement call must revalidate all its own preconditions.
pub fn inspect_announcement(
    instance: &Instance,
    guard: &VoiceMutationGuard,
    voice_mod: VoiceMod,
) -> Result<AnnouncementInspection> {
    // This prefix check is pure. Reject an ordinary source before any process,
    // database or inventory read; full provenance stays inside the strict pin.
    if !diagnostic_lab::is_lab_candidate(instance) {
        return Err(RecoveryError::UnsafePath);
    }
    guard.recheck()?;
    let result = recovery::with_existing_observation(instance, guard.inner(), |current| {
        // The strict read-only database pin is already held before this transitive
        // legacy identity lookup. No WAL/SHM initialization can precede that gate.
        require_test_copy(instance)?;
        let (artifact_sha256, artifact_bytes) = voice_mod.artifact();
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
            || current
                .files
                .iter()
                .filter(|file| {
                    file.path.starts_with("mods/")
                        && file.path.ends_with(".jar")
                        && file.sha256 == artifact_sha256
                        && file.size == artifact_bytes
                })
                .count()
                != 1
        {
            return Err(RecoveryError::Compatibility);
        }
        let config_path = voice_mod.config();
        let source = current
            .files
            .iter()
            .find(|file| file.path == config_path)
            .ok_or(RecoveryError::UnsafePath)?;
        let bytes =
            recovery::read_bounded(&recovery::path(instance.root(), config_path)?, MAX_CONFIG)?;
        if bytes.len() as u64 != source.size || digest(&bytes) != source.sha256 {
            return Err(RecoveryError::Changed);
        }
        let announcement = match voice_mod {
            VoiceMod::SimpleVoiceChat => {
                let span = svc_value_span(&bytes)?;
                let voice_host =
                    std::str::from_utf8(&bytes[span]).map_err(|_| RecoveryError::Integrity)?;
                // The existing file can be larger. Refuse an excessive presentation
                // field instead of truncating it and suggesting it was understood.
                if voice_host.len() > 1024 {
                    return Err(RecoveryError::Limit);
                }
                ObservedAnnouncement::SimpleVoiceChat {
                    voice_host: voice_host.to_owned(),
                }
            }
            VoiceMod::PlasmoVoice => match plasmo::inspect_public(&bytes)? {
                Some((ip, port)) => ObservedAnnouncement::PlasmoPublic { ip, port },
                None => ObservedAnnouncement::PlasmoAbsent,
            },
        };
        Ok(AnnouncementInspection {
            voice_mod,
            artifact_version: match voice_mod {
                VoiceMod::SimpleVoiceChat => "2.6.22",
                VoiceMod::PlasmoVoice => "2.1.17",
            },
            artifact_sha256,
            artifact_bytes,
            config_path,
            source_sha256: source.sha256.clone(),
            source_bytes: source.size,
            announcement,
            limitations: vec![
                "inspection_only_no_approval",
                "configured_announcement_not_effective_bind",
                "empty_absent_or_zero_values_not_resolved",
                "dedicated_aurora_only_requires_separate_review",
                "artifact_presence_does_not_prove_mod_loaded",
                "voice_audio_not_verified",
                "transport_not_observed",
            ],
        })
    })?;
    // The last inventory can be slow. Do not reuse process absence observed
    // before those reads as the final local observation.
    guard.recheck()?;
    Ok(result)
}

#[cfg(test)]
#[path = "connect_voice_inspection_tests.rs"]
mod tests;
