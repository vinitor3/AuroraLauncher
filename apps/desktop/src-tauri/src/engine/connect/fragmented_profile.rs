//! Explicit desktop transport profile. This does not identify a voice mod,
//! authorize a configuration change, or establish that audio is available.
#[cfg(any(test, feature = "connect-lab"))]
use super::ConnectError;
use super::ConnectMode;
use serde::{Deserialize, Serialize};

pub const MAX_LIFETIME_SECONDS: u16 = 120;
pub const ORIGINAL_DATAGRAM_BYTES: u16 = 4096;
pub const MAX_FRAGMENT_FRAME_BYTES: u16 = 1264;
pub const LOCAL_UDP_BIND: &str = "127.0.0.1:35554";
pub const PROFILE: &str = "fragmentedGameUdp";
pub const SESSION_PROFILE: &str = "sessionFragmentedGameUdp";
pub const MAX_RECOVERY_ATTEMPTS: u8 = 3;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UdpRecoveryCapabilities {
    pub max_attempts: u8,
}
impl Default for UdpRecoveryCapabilities {
    fn default() -> Self {
        Self {
            max_attempts: MAX_RECOVERY_ATTEMPTS,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UdpRecoveryState {
    pub attempts_used: u8,
    pub max_attempts: u8,
    pub pending: bool,
}
impl Default for UdpRecoveryState {
    fn default() -> Self {
        Self {
            attempts_used: 0,
            max_attempts: MAX_RECOVERY_ATTEMPTS,
            pending: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FragmentedHostRequest {
    pub mode: ConnectMode,
    pub bind_address: String,
    pub game_target: String,
    pub udp_target: String,
    pub lifetime_seconds: u16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionFragmentedHostRequest {
    pub mode: ConnectMode,
    pub bind_address: String,
    pub game_target: String,
    pub udp_target: String,
    pub admission_seconds: u16,
    pub session_seconds: u16,
}

/// Only the game/TCP target comes from the single-use owned-process selection.
/// UDP remains an independently reviewed manual loopback destination.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionFragmentedDiscoveredHostRequest {
    pub mode: ConnectMode,
    pub bind_address: String,
    pub udp_target: String,
    pub admission_seconds: u16,
    pub session_seconds: u16,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FragmentedReturnPolicy {
    #[serde(rename = "type")]
    kind: &'static str,
    bytes_per_second: u32,
    datagrams_per_second: u32,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FragmentedUdpContract {
    profile: &'static str,
    invitation_schema_version: u8,
    udp_envelope_version: u8,
    approved_original_datagram_bytes: u16,
    max_fragment_frame_bytes: u16,
    local_udp_bind: &'static str,
    udp_return_policy: FragmentedReturnPolicy,
}
impl Default for FragmentedUdpContract {
    fn default() -> Self {
        Self {
            profile: PROFILE,
            invitation_schema_version: 1,
            udp_envelope_version: 2,
            approved_original_datagram_bytes: ORIGINAL_DATAGRAM_BYTES,
            max_fragment_frame_bytes: MAX_FRAGMENT_FRAME_BYTES,
            local_udp_bind: LOCAL_UDP_BIND,
            udp_return_policy: FragmentedReturnPolicy {
                kind: "authorizedDownstream",
                bytes_per_second: 16384,
                datagrams_per_second: 20,
            },
        }
    }
}
impl FragmentedUdpContract {
    pub(super) fn session() -> Self {
        Self {
            profile: SESSION_PROFILE,
            invitation_schema_version: 3,
            ..Self::default()
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFragmentedUdpCapabilities {
    #[serde(flatten)]
    pub contract: FragmentedUdpContract,
    pub max_admission_seconds: u16,
    pub max_session_seconds: u16,
    pub udp_recovery: UdpRecoveryCapabilities,
    pub owned_game_tcp_discovery: bool,
}
impl Default for SessionFragmentedUdpCapabilities {
    fn default() -> Self {
        Self {
            contract: FragmentedUdpContract::session(),
            max_admission_seconds: 600,
            max_session_seconds: 3600,
            udp_recovery: UdpRecoveryCapabilities::default(),
            owned_game_tcp_discovery: cfg!(all(windows, feature = "connect-lab")),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FragmentedUdpCapabilities {
    #[serde(flatten)]
    pub contract: FragmentedUdpContract,
    pub max_lifetime_seconds: u16,
}
impl Default for FragmentedUdpCapabilities {
    fn default() -> Self {
        Self {
            contract: FragmentedUdpContract::default(),
            max_lifetime_seconds: MAX_LIFETIME_SECONDS,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FragmentedUdpReview {
    #[serde(flatten)]
    pub contract: FragmentedUdpContract,
    pub udp_target: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UdpState {
    NotStarted,
    Starting,
    Available,
    Failed,
    Closed,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum UdpFailure {
    #[serde(rename = "connect_udp_setup_failed")]
    SetupFailed,
    #[serde(rename = "connect_udp_capacity_unavailable")]
    CapacityUnavailable,
    #[serde(rename = "connect_udp_closed")]
    Closed,
}

/// Guest-only fields flattened into its snapshot. Legacy/session profiles and
/// hosts omit the entire object: the host has no local guest bridge.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FragmentedGuestState {
    pub udp_state: UdpState,
    pub udp_address: Option<&'static str>,
    pub udp_error_code: Option<UdpFailure>,
    pub udp_original_capacity: Option<u16>,
    pub udp_capacity_scope: Option<&'static str>,
}
impl Default for FragmentedGuestState {
    fn default() -> Self {
        Self {
            udp_state: UdpState::NotStarted,
            udp_address: None,
            udp_error_code: None,
            udp_original_capacity: None,
            udp_capacity_scope: None,
        }
    }
}
impl FragmentedGuestState {
    pub fn starting() -> Self {
        Self {
            udp_state: UdpState::Starting,
            ..Self::default()
        }
    }
    pub fn available() -> Self {
        Self {
            udp_state: UdpState::Available,
            udp_address: Some(LOCAL_UDP_BIND),
            udp_original_capacity: Some(ORIGINAL_DATAGRAM_BYTES),
            udp_capacity_scope: Some("initialAssociation"),
            ..Self::default()
        }
    }
    pub fn failed(code: UdpFailure) -> Self {
        Self {
            udp_state: UdpState::Failed,
            udp_error_code: Some(code),
            ..Self::default()
        }
    }
    pub fn close(&mut self) {
        self.udp_state = UdpState::Closed;
        self.udp_address = None;
        self.udp_original_capacity = None;
        self.udp_capacity_scope = None;
    }
}

#[cfg(any(test, feature = "connect-lab"))]
pub fn validate_lifetime(value: u16) -> Result<(), ConnectError> {
    if !(1..=MAX_LIFETIME_SECONDS).contains(&value) {
        return Err(ConnectError::InvalidRequest);
    }
    Ok(())
}

#[cfg(feature = "connect-lab")]
pub(super) fn services(
    game: std::net::SocketAddr,
    udp: std::net::SocketAddr,
) -> Result<Vec<aurora_connect_lab::Service>, ConnectError> {
    use aurora_connect_lab::{Capability, Protocol, Service, UdpReturnPolicy};
    let approved = |address: std::net::SocketAddr| {
        address.ip().is_loopback()
            && address.port() != 0
            && !matches!(address, std::net::SocketAddr::V6(v6) if v6.scope_id() != 0 || v6.flowinfo() != 0)
    };
    if !approved(game) || !approved(udp) {
        return Err(ConnectError::InvalidRequest);
    }
    Ok(vec![
        Service {
            service_id: "game".into(),
            protocol: Protocol::Tcp,
            endpoint: game,
            revision: 1,
            required_capability: Capability::ReliableStream,
            max_datagram_bytes: 1400,
            udp_return_policy: UdpReturnPolicy::ReplyBudget,
        },
        Service {
            service_id: "aux".into(),
            protocol: Protocol::Udp,
            endpoint: udp,
            revision: 1,
            required_capability: Capability::FragmentedDatagram,
            max_datagram_bytes: ORIGINAL_DATAGRAM_BYTES.into(),
            udp_return_policy: UdpReturnPolicy::AuthorizedDownstream {
                bytes_per_second: 16384,
                datagrams_per_second: 20,
            },
        },
    ])
}

/// Additional desktop restriction after the explicit library verifier. Uses
/// the immutable verified object; it never reparses bytes to renew authority.
#[cfg(feature = "connect-lab")]
pub(super) fn validate_verified_invitation(
    invitation: &aurora_connect_lab::Invitation,
) -> Result<(), ConnectError> {
    use serde_json::json;
    let preview = invitation.preview();
    let issued = preview["issuedAtUnixMs"]
        .as_u64()
        .ok_or(ConnectError::InvitationRejected)?;
    let expires = invitation.expires_at_unix_ms();
    // Check the originally signed duration, not the remaining time: an older
    // long invitation must not enter this profile near its expiration.
    if issued == 0
        || expires > (1u64 << 53) - 1
        || expires <= issued
        || expires - issued < 1000
        || !(expires - issued).is_multiple_of(1000)
        || expires - issued > u64::from(MAX_LIFETIME_SECONDS) * 1000
        || preview["profile"] != "fragmentedV2"
        || preview["schemaVersion"] != 1
        || preview["udpEnvelopeVersion"] != 2
        || preview["maxFragmentFrameBytes"] != MAX_FRAGMENT_FRAME_BYTES
        || preview["services"]
            != json!([
                { "serviceId": "game", "protocol": "tcp", "revision": 1, "requiredCapability": "reliableStream", "maxDatagramBytes": 1400, "udpReturnPolicy": {"type":"replyBudget"} },
                { "serviceId": "aux", "protocol": "udp", "revision": 1, "requiredCapability": "fragmentedDatagram", "maxDatagramBytes": ORIGINAL_DATAGRAM_BYTES, "udpReturnPolicy": {"type":"authorizedDownstream", "bytesPerSecond":16384, "datagramsPerSecond":20} }
            ])
    {
        return Err(ConnectError::InvitationRejected);
    }
    Ok(())
}

/// Called only after the selected, reviewed profile has authenticated and its
/// TCP bridge is available. A failure belongs to UDP; it does not close Guest
/// or TCP. The caller must await this operation even when cancellation arrives.
#[cfg(feature = "connect-lab")]
pub(super) async fn open_guest_bridge(
    guest: &aurora_connect_lab::Guest,
) -> (Option<aurora_connect_lab::UdpBridge>, FragmentedGuestState) {
    let result = guest
        .udp_bridge_fragmented("aux", 1, LOCAL_UDP_BIND.parse().expect("fixed loopback"))
        .await;
    let Ok(bridge) = result else {
        return (None, FragmentedGuestState::failed(UdpFailure::SetupFailed));
    };
    if bridge.max_payload() != Some(usize::from(ORIGINAL_DATAGRAM_BYTES)) {
        bridge.shutdown().await;
        drop(bridge);
        return (
            None,
            FragmentedGuestState::failed(UdpFailure::CapacityUnavailable),
        );
    }
    (Some(bridge), FragmentedGuestState::available())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn discovered_session_request_never_accepts_a_game_endpoint_or_process_override() {
        let valid = json!({"mode":"loopback","bindAddress":"127.0.0.1:42000","udpTarget":"127.0.0.1:43002","admissionSeconds":600,"sessionSeconds":3600});
        assert!(
            serde_json::from_value::<SessionFragmentedDiscoveredHostRequest>(valid.clone()).is_ok()
        );
        assert!(serde_json::from_value::<SessionFragmentedHostRequest>(valid.clone()).is_err());
        for field in [
            "gameTarget",
            "pid",
            "runId",
            "instanceId",
            "selectionId",
            "services",
            "lifetimeSeconds",
            "localUdpBind",
        ] {
            let mut changed = valid.clone();
            changed[field] = json!("not_authority");
            assert!(
                serde_json::from_value::<SessionFragmentedDiscoveredHostRequest>(changed).is_err(),
                "{field}"
            );
        }
        for field in ["udpTarget", "admissionSeconds", "sessionSeconds"] {
            let mut changed = valid.clone();
            changed.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<SessionFragmentedDiscoveredHostRequest>(changed).is_err()
            );
        }
        for field in ["admissionSeconds", "sessionSeconds"] {
            for value in [json!(-1), json!(1.5), json!(65536)] {
                let mut changed = valid.clone();
                changed[field] = value;
                assert!(
                    serde_json::from_value::<SessionFragmentedDiscoveredHostRequest>(changed)
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn request_has_no_service_or_budget_override_and_requires_its_own_fields() {
        let valid = json!({"mode":"loopback","bindAddress":"127.0.0.1:42000","gameTarget":"127.0.0.1:43001","udpTarget":"127.0.0.1:43002","lifetimeSeconds":120});
        let request: FragmentedHostRequest = serde_json::from_value(valid.clone()).unwrap();
        assert!(validate_lifetime(request.lifetime_seconds).is_ok());
        for field in [
            "sessionSeconds",
            "services",
            "bytesPerSecond",
            "voiceMod",
            "localUdpBind",
        ] {
            let mut changed = valid.clone();
            changed[field] = json!(1);
            assert!(serde_json::from_value::<FragmentedHostRequest>(changed).is_err());
        }
        let mut changed = valid.clone();
        changed.as_object_mut().unwrap().remove("udpTarget");
        assert!(serde_json::from_value::<FragmentedHostRequest>(changed).is_err());
        for bad in [json!(1.5), json!(-1), json!(65536)] {
            let mut changed = valid.clone();
            changed["lifetimeSeconds"] = bad;
            assert!(serde_json::from_value::<FragmentedHostRequest>(changed).is_err());
        }
        assert!(validate_lifetime(0).is_err());
        assert!(validate_lifetime(121).is_err());
        assert!(validate_lifetime(1).is_ok());
    }

    #[test]
    fn failed_and_closed_udp_do_not_project_a_working_listener_or_claim_a_mod() {
        let mut state = FragmentedGuestState::available();
        assert_eq!(state.udp_address, Some(LOCAL_UDP_BIND));
        state.close();
        let closed = serde_json::to_value(&state).unwrap();
        assert_eq!(closed["udpState"], "closed");
        assert!(closed["udpAddress"].is_null());
        assert!(closed["udpOriginalCapacity"].is_null());
        assert!(closed["udpCapacityScope"].is_null());
        let mut failed = FragmentedGuestState::failed(UdpFailure::SetupFailed);
        failed.close();
        assert_eq!(failed.udp_error_code, Some(UdpFailure::SetupFailed));
        let capability = serde_json::to_value(FragmentedUdpCapabilities::default()).unwrap();
        assert_eq!(capability["profile"], PROFILE);
        assert_eq!(capability["maxLifetimeSeconds"], 120);
        assert!(capability.get("voiceMod").is_none());
        assert!(capability.get("sessionTiming").is_none());
    }

    #[cfg(feature = "connect-lab")]
    #[test]
    fn only_the_exact_signed_transport_scope_and_original_short_lifetime_are_accepted() {
        use aurora_connect_lab as lab;
        let key = lab::generate_key();
        let guest = lab::generate_key().public();
        let game = "127.0.0.1:43001".parse().unwrap();
        let udp = "127.0.0.1:43002".parse().unwrap();
        let exact = services(game, udp).unwrap();
        let issue = |registered: &[lab::Service], seconds: u64| {
            lab::Invitation::issue_fragmented_for_lifetime(
                &key,
                guest,
                "desktop-udp-profile",
                "127.0.0.1:42000".parse().unwrap(),
                registered,
                std::time::Duration::from_secs(seconds),
            )
            .unwrap()
        };
        assert!(validate_verified_invitation(&issue(&exact, 120)).is_ok());
        assert!(validate_verified_invitation(&issue(&exact, 180)).is_err());
        let mut changed = exact.clone();
        changed[1].udp_return_policy = lab::UdpReturnPolicy::ReplyBudget;
        assert!(validate_verified_invitation(&issue(&changed, 120)).is_err());
        changed[1].udp_return_policy = lab::UdpReturnPolicy::AuthorizedDownstream {
            bytes_per_second: 32768,
            datagrams_per_second: 20,
        };
        assert!(validate_verified_invitation(&issue(&changed, 120)).is_err());
        changed = exact.clone();
        changed[1].max_datagram_bytes = 2048;
        assert!(validate_verified_invitation(&issue(&changed, 120)).is_err());
        changed = exact.clone();
        changed[0].max_datagram_bytes = 2048;
        assert!(validate_verified_invitation(&issue(&changed, 120)).is_err());
        assert!(services("192.0.2.1:43001".parse().unwrap(), udp).is_err());
        assert!(services(game, "127.0.0.1:0".parse().unwrap()).is_err());
    }
}
