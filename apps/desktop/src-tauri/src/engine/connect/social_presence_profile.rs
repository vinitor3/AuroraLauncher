//! Read-only catalog observations never reserve a Connect attempt or admit peers.
use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SocialPresenceRequest {
    pub account_epoch: u64,
    pub account_id: String,
    pub session_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SocialSessionAvailability {
    pub session_id: String,
    pub state: &'static str,
    /// Remaining original observation age after subtracting the complete HTTP RTT.
    pub valid_for_ms: u64,
    pub admitted_guests: Option<u8>,
    pub connected_guests: Option<u8>,
    pub host_player_connected: Option<bool>,
    pub max_guests: Option<u8>,
}
impl ConnectManager {
    pub async fn social_observe_sessions(
        &self,
        owner: &str,
        request: SocialPresenceRequest,
        id_token: String,
    ) -> Result<Vec<SocialSessionAvailability>, ConnectError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live
                .social_observe_sessions(owner, request, id_token)
                .await
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, request, id_token);
            Err(ConnectError::Disabled)
        }
    }
}
