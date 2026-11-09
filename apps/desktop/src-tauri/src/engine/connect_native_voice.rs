//! Reviewed native dedicated voice workflow. Public requests contain no source
//! paths, process identities, network destinations or configuration contents.
use super::connect::ConnectManager;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NativeVoiceProfile {
    SimpleVoiceChat,
    PlasmoVoice,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum NativeVoiceRuntime {
    #[serde(rename = "fabric-loader-0.16.14-1.21.1")]
    Fabric1211,
    #[serde(rename = "fabric-loader-0.16.14-1.20.1")]
    Fabric1201,
    #[serde(rename = "fabric-loader-0.16.14-1.19.2")]
    Fabric1192,
    #[serde(rename = "fabric-loader-0.13.3-1.16.5")]
    Fabric1165,
    #[serde(rename = "1.21.1-forge-52.1.12")]
    Forge1211,
    #[serde(rename = "1.20.1-forge-47.4.23")]
    Forge1201,
    #[serde(rename = "1.19.2-forge-43.4.16")]
    Forge1192,
    #[serde(rename = "1.16.5-forge-36.2.42")]
    Forge1165,
    #[serde(rename = "1.12.2-forge-14.23.5.2860")]
    Forge1122,
}
impl NativeVoiceRuntime {
    #[cfg(all(feature = "connect-voice-lab", feature = "connect-lab"))]
    pub(crate) fn from_ready(runtime: crate::engine::connect_ready_profile::ReadyProfile) -> Self {
        use crate::engine::connect_ready_profile::ReadyProfile as R;
        match runtime {
            R::Forge1122 => Self::Forge1122,
            R::Fabric1165 => Self::Fabric1165,
            R::Forge1165 => Self::Forge1165,
            R::Fabric1192 => Self::Fabric1192,
            R::Forge1192 => Self::Forge1192,
            R::Fabric1201 => Self::Fabric1201,
            R::Forge1201 => Self::Forge1201,
            R::Fabric1211 => Self::Fabric1211,
            R::Forge1211 => Self::Forge1211,
        }
    }
    #[cfg(all(feature = "connect-voice-lab", feature = "connect-lab"))]
    pub(crate) fn ready(self) -> crate::engine::connect_ready_profile::ReadyProfile {
        use crate::engine::connect_ready_profile::ReadyProfile;
        match self {
            Self::Fabric1211 => ReadyProfile::Fabric1211,
            Self::Fabric1201 => ReadyProfile::Fabric1201,
            Self::Fabric1192 => ReadyProfile::Fabric1192,
            Self::Fabric1165 => ReadyProfile::Fabric1165,
            Self::Forge1211 => ReadyProfile::Forge1211,
            Self::Forge1201 => ReadyProfile::Forge1201,
            Self::Forge1192 => ReadyProfile::Forge1192,
            Self::Forge1165 => ReadyProfile::Forge1165,
            Self::Forge1122 => ReadyProfile::Forge1122,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum NativeVoiceRequest {
    PrepareRuntime {
        profile: NativeVoiceProfile,
        runtime_profile: NativeVoiceRuntime,
    },
    ApprovePreparation {
        digest: String,
    },
    Prepare {
        profile: NativeVoiceProfile,
    },
    InspectRecovery {},
    ResumeRecovery {
        bundle_id: String,
        receipt_id: String,
    },
    ApproveFiles {
        digest: String,
    },
    ApproveExecution {
        digest: String,
    },
    ReviewHost {
        guests: Vec<String>,
        local_player: bool,
    },
    ApproveHost {
        digest: String,
    },
    ExportInvitation {
        participant_id: String,
    },
    Stop {},
    PrepareRestore {
        reconciled: bool,
    },
    ApproveRestore {
        digest: String,
    },
    PrepareEulaRestore {},
    ApproveEulaRestore {
        digest: String,
    },
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeVoiceApproval {
    pub kind: &'static str,
    pub digest: String,
    pub before: String,
    pub after: String,
    pub details: Vec<String>,
    pub remaining_ms: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeVoiceParticipant {
    pub participant_id: String,
    pub public_id: String,
    pub local_player: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeVoiceRecoveryCandidate {
    pub bundle_id: String,
    pub receipt_id: String,
    pub profile: NativeVoiceProfile,
    pub runtime_profile: NativeVoiceRuntime,
    pub eula_recovery_pending: bool,
    pub announcement_recovery_pending: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeVoiceSnapshot {
    pub revision: u64,
    pub available: bool,
    pub manual_lab_available: bool,
    pub phase: &'static str,
    pub profile: Option<NativeVoiceProfile>,
    pub runtime_profile: Option<NativeVoiceRuntime>,
    pub review: Option<NativeVoiceApproval>,
    pub operation_id: Option<String>,
    pub host_public_id: Option<String>,
    pub participants: Vec<NativeVoiceParticipant>,
    pub udp_state: &'static str,
    pub tcp_address: Option<String>,
    pub negotiated_datagram_bytes: Option<usize>,
    pub recovery_pending: bool,
    pub recoverable_copies: Vec<NativeVoiceRecoveryCandidate>,
    pub eula_state: &'static str,
    pub eula_recovery_available: bool,
    #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
    #[serde(skip)]
    pub(crate) native_review_deadline: Option<std::time::Instant>,
    pub error_code: Option<&'static str>,
    pub exported_invitation: Option<String>,
    pub audio_state: &'static str,
}
impl Default for NativeVoiceSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            available: cfg!(all(
                windows,
                feature = "connect-lab",
                feature = "connect-voice-lab"
            )),
            manual_lab_available: cfg!(all(
                windows,
                debug_assertions,
                feature = "connect-lab",
                feature = "connect-voice-lab"
            )),
            phase: "idle",
            profile: None,
            runtime_profile: None,
            review: None,
            operation_id: None,
            host_public_id: None,
            participants: Vec::new(),
            udp_state: "notStarted",
            tcp_address: None,
            negotiated_datagram_bytes: None,
            recovery_pending: false,
            recoverable_copies: Vec::new(),
            eula_state: "unchanged",
            eula_recovery_available: false,
            #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
            native_review_deadline: None,
            error_code: None,
            exported_invitation: None,
            audio_state: "notVerified",
        }
    }
}
impl NativeVoiceSnapshot {
    #[cfg(all(windows, feature = "connect-voice-lab", feature = "connect-lab"))]
    pub(crate) fn projected(mut self) -> Self {
        if let Some(review) = &mut self.review {
            review.remaining_ms = self
                .native_review_deadline
                .map(|deadline| {
                    deadline
                        .saturating_duration_since(std::time::Instant::now())
                        .as_millis()
                        .min(u64::MAX as u128) as u64
                })
                .unwrap_or(0);
        }
        self
    }
}
#[derive(Default)]
pub struct NativeVoiceSessionManager {
    #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
    live: super::connect_voice::dedicated::DesktopVoiceManager,
}
impl NativeVoiceSessionManager {
    pub fn snapshot(&self, owner: &str) -> Result<NativeVoiceSnapshot, &'static str> {
        if owner != "main" {
            return Err("connect_owner_mismatch");
        }
        #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
        {
            self.live.snapshot()
        }
        #[cfg(not(all(windows, feature = "connect-lab", feature = "connect-voice-lab")))]
        {
            Ok(NativeVoiceSnapshot::default())
        }
    }
    pub async fn execute(
        &self,
        owner: &str,
        connect: &ConnectManager,
        app_data: &std::path::Path,
        request: NativeVoiceRequest,
    ) -> Result<NativeVoiceSnapshot, &'static str> {
        if owner != "main" {
            return Err("connect_owner_mismatch");
        }
        if !cfg!(debug_assertions)
            && matches!(
                &request,
                NativeVoiceRequest::ReviewHost { .. }
                    | NativeVoiceRequest::ApproveHost { .. }
                    | NativeVoiceRequest::ExportInvitation { .. }
            )
        {
            return Err("native_voice_disabled");
        }
        #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
        {
            self.live.execute(connect, app_data, request).await
        }
        #[cfg(not(all(windows, feature = "connect-lab", feature = "connect-voice-lab")))]
        {
            let _ = (connect, app_data, request);
            Err("native_voice_disabled")
        }
    }
    /// Reserve the existing social Attempt with the original native source.
    /// The request supplies social intent only; it cannot select any endpoint.
    pub fn reserve_social(
        &self,
        owner: &str,
        connect: &ConnectManager,
        request: super::connect::SocialDiscoveredReserveRequest,
        host_player: bool,
    ) -> Result<super::connect::SocialReservation, &'static str> {
        if owner != "main" {
            return Err("connect_owner_mismatch");
        }
        #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
        {
            let binding = self.live.take_social_source(connect)?;
            connect
                .social_reserve_native_voice(owner, request, binding, host_player)
                .map_err(|error| error.code())
        }
        #[cfg(not(all(windows, feature = "connect-lab", feature = "connect-voice-lab")))]
        {
            let _ = (connect, request, host_player);
            Err("native_voice_disabled")
        }
    }

    /// Seals admission immediately; the retained actor owns the real cleanup.
    pub fn cancel(&self) {
        #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
        self.live.cancel();
    }
    pub async fn shutdown_all(&self) {
        self.cancel();
        #[cfg(all(windows, feature = "connect-lab", feature = "connect-voice-lab"))]
        self.live.shutdown_all().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_voice_requests_are_closed_and_never_select_a_source_or_destination() {
        let cases = [
            r#"{"action":"prepare","profile":"simpleVoiceChat"}"#,
            r#"{"action":"prepare","profile":"plasmoVoice"}"#,
            r#"{"action":"reviewHost","guests":[],"localPlayer":true}"#,
            r#"{"action":"stop"}"#,
            r#"{"action":"inspectRecovery"}"#,
            r#"{"action":"prepareEulaRestore"}"#,
            r#"{"action":"prepareRuntime","profile":"simpleVoiceChat","runtimeProfile":"fabric-loader-0.16.14-1.21.1"}"#,
            r#"{"action":"approvePreparation","digest":"review-digest"}"#,
        ];
        for input in cases {
            assert!(serde_json::from_str::<NativeVoiceRequest>(input).is_ok());
            for field in [
                "path",
                "pid",
                "gameTarget",
                "udpTarget",
                "javaExecutable",
                "config",
                "binding",
            ] {
                let added = format!(
                    "{},\"{}\":\"untrusted\"}}",
                    &input[..input.len() - 1],
                    field
                );
                assert!(
                    serde_json::from_str::<NativeVoiceRequest>(&added).is_err(),
                    "unexpected field accepted: {added}"
                );
            }
        }
        assert!(serde_json::from_str::<NativeVoiceRequest>(
            r#"{"action":"prepare","profile":"both"}"#
        )
        .is_err());
    }
    #[test]
    fn native_voice_capability_never_claims_audio_or_default_release_availability() {
        let value = NativeVoiceSnapshot::default();
        assert_eq!(value.audio_state, "notVerified");
        assert_eq!(
            value.available,
            cfg!(all(
                windows,
                feature = "connect-lab",
                feature = "connect-voice-lab"
            ))
        );
        assert!(value.tcp_address.is_none());
        assert!(value.review.is_none());
    }
}
