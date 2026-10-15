//! Explicit static Host roster. Single-party and Guest DTOs keep their shape.
use super::{ConnectMode, FragmentedUdpContract, FragmentedUdpReview, SessionTiming};
use serde::{Deserialize, Serialize};

#[cfg(feature = "connect-lab")]
pub(super) const GROUP_KIND: &str = "staticSessionFragmentedHost";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFragmentedHostGroupsCapabilities {
    pub profile: &'static str,
    pub invitation_schema_version: u8,
    pub min_participants: u8,
    pub max_participants: u8,
    pub max_preparation_seconds: u16,
    pub max_review_seconds: u16,
    pub max_admission_seconds: u16,
    pub max_session_seconds: u16,
    pub owned_game_tcp_discovery: bool,
}
impl Default for SessionFragmentedHostGroupsCapabilities {
    fn default() -> Self {
        Self {
            profile: "sessionFragmentedGameUdp",
            invitation_schema_version: 3,
            min_participants: 2,
            max_participants: 8,
            max_preparation_seconds: 600,
            max_review_seconds: 60,
            max_admission_seconds: 600,
            max_session_seconds: 3600,
            owned_game_tcp_discovery: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupHostRequest {
    pub mode: ConnectMode,
    pub bind_address: String,
    pub game_target: String,
    pub udp_target: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupParticipantRequest {
    pub guest_public_id: String,
    pub admission_seconds: u16,
    pub session_seconds: u16,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupPreparation {
    pub operation_id: String,
    pub generation: u64,
    pub public_id: String,
    pub expires_at_ms: u64,
    pub kind: &'static str,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupParticipant {
    pub participant_id: String,
    pub guest_public_id: String,
    pub invitation_digest: String,
    pub session_timing: SessionTiming,
    pub consumed: bool,
    pub revoked: bool,
    pub admission_valid: bool,
    pub session_valid: bool,
    pub transport_active: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupReview {
    pub operation_id: String,
    pub generation: u64,
    pub digest: String,
    pub kind: &'static str,
    pub mode: ConnectMode,
    pub host_public_id: String,
    pub direct_address: String,
    pub game_target: String,
    pub target_source: &'static str,
    pub fragmented_udp: FragmentedUdpReview,
    pub participants: Vec<GroupParticipant>,
    pub expires_at_ms: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupInvitationExport {
    pub operation_id: String,
    pub participant_id: String,
    pub invitation_digest: String,
    pub invitation: String,
    pub host_public_id: String,
    pub expires_at_ms: u64,
    pub session_timing: SessionTiming,
    pub fragmented_udp: FragmentedUdpContract,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostGroupSnapshot {
    pub kind: &'static str,
    pub profile: &'static str,
    pub invitation_schema_version: u8,
    pub mode: ConnectMode,
    pub host_public_id: String,
    pub preparation: Option<GroupPreparation>,
    pub review: Option<GroupReview>,
    pub participants: Vec<GroupParticipant>,
}

#[cfg(all(test, not(feature = "connect-lab")))]
mod disabled_tests {
    use super::*;
    use crate::engine::connect::{ConnectError, ConnectManager};

    #[test]
    fn default_build_refuses_all_group_actions_without_publishing_identity() {
        use std::future::Future;
        use std::task::{Context, Poll, Waker};
        let manager = ConnectManager::default();
        assert!(serde_json::to_value(manager.capabilities())
            .unwrap()
            .get("sessionFragmentedHostGroups")
            .is_none());
        assert!(matches!(
            manager.prepare_session_group_host(
                "main",
                GroupHostRequest {
                    mode: ConnectMode::Loopback,
                    bind_address: "127.0.0.1:43000".into(),
                    game_target: "127.0.0.1:43001".into(),
                    udp_target: "127.0.0.1:43002".into(),
                }
            ),
            Err(ConnectError::Disabled)
        ));
        assert!(matches!(
            manager.review_session_group_host("main", "none", Vec::new()),
            Err(ConnectError::Disabled)
        ));
        let mut confirmation = Box::pin(manager.confirm_session_group_host("main", "none", "none"));
        assert!(matches!(
            confirmation
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Err(ConnectError::Disabled))
        ));
        assert!(matches!(
            manager.export_session_group_invitation("main", "none", "none"),
            Err(ConnectError::Disabled)
        ));
        assert!(matches!(
            manager.revoke_session_group_participant("main", "none", "none", 0),
            Err(ConnectError::Disabled)
        ));
        let snapshot = manager.snapshot("main").unwrap();
        assert!(snapshot.operation_id.is_none());
        assert!(serde_json::to_value(snapshot)
            .unwrap()
            .get("hostGroup")
            .is_none());
    }
}
