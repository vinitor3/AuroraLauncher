//! Read-only owned-process listener inventory. This is separate from the Connect
//! actor: inventory sends no packets; STATUS requires an exact consumed review.
//! No paths, process arguments, remote destinations or generic PID API.
use super::connect::StatusSummary;
use serde::Serialize;
#[cfg(all(windows, feature = "connect-lab"))]
use sha2::{Digest, Sha256};
#[cfg(all(windows, feature = "connect-lab"))]
use std::net::SocketAddr;
#[cfg(all(windows, feature = "connect-lab"))]
use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, Instant},
};

#[cfg(all(windows, feature = "connect-lab"))]
mod windows;
#[cfg(all(windows, feature = "connect-lab"))]
pub use windows::OwnedChildProof;
#[cfg(all(test, windows, feature = "connect-lab"))]
mod tests;

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum DiscoveryError {
    #[error("discovery_unavailable")]
    Unavailable,
    #[error("discovery_invalid_request")]
    InvalidRequest,
    #[error("discovery_process_changed")]
    ProcessChanged,
    #[error("discovery_listener_changed")]
    ListenerChanged,
    #[error("discovery_inventory_limit")]
    Limit,
    #[error("discovery_stale_review")]
    Stale,
    #[error("discovery_owner_mismatch")]
    Owner,
    #[error("discovery_wrong_digest")]
    Digest,
    #[error("discovery_busy")]
    Busy,
    #[error("discovery_status_failed")]
    StatusFailed,
    #[error("discovery_internal")]
    Internal,
}
impl DiscoveryError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "discovery_unavailable",
            Self::InvalidRequest => "discovery_invalid_request",
            Self::ProcessChanged => "discovery_process_changed",
            Self::ListenerChanged => "discovery_listener_changed",
            Self::Limit => "discovery_inventory_limit",
            Self::Stale => "discovery_stale_review",
            Self::Owner => "discovery_owner_mismatch",
            Self::Digest => "discovery_wrong_digest",
            Self::Busy => "discovery_busy",
            Self::StatusFailed => "discovery_status_failed",
            Self::Internal => "discovery_internal",
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub candidate_id: String,
    pub target: String,
    pub bind_kind: &'static str,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub inventory_id: String,
    pub instance_id: String,
    pub run_id: String,
    pub expires_at_ms: u64,
    pub candidates: Vec<Candidate>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetReview {
    pub review_id: String,
    pub digest: String,
    pub inventory_id: String,
    pub candidate: Candidate,
    pub instance_id: String,
    pub run_id: String,
    pub expires_at_ms: u64,
    pub status_protocol: i32,
    pub handshake_host: &'static str,
    pub handshake_port: u16,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetStatus {
    pub selection_id: String,
    pub target: String,
    pub instance_id: String,
    pub run_id: String,
    pub summary: StatusSummary,
    pub verified_before_after: bool,
    pub expires_at_ms: u64,
    pub world_state: &'static str,
}

#[cfg(all(windows, feature = "connect-lab"))]
fn local_target(bind: SocketAddr) -> Option<(SocketAddr, &'static str)> {
    if bind.port() == 0 {
        return None;
    }
    match bind {
        SocketAddr::V4(v4) if v4.ip().is_loopback() => Some((bind, "loopback")),
        SocketAddr::V4(v4) if v4.ip().is_unspecified() => {
            Some((SocketAddr::from(([127, 0, 0, 1], v4.port())), "wildcard"))
        }
        SocketAddr::V6(v6) if v6.scope_id() == 0 && v6.flowinfo() == 0 && v6.ip().is_loopback() => {
            Some((bind, "loopback"))
        }
        SocketAddr::V6(v6)
            if v6.scope_id() == 0 && v6.flowinfo() == 0 && v6.ip().is_unspecified() =>
        {
            Some((
                SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, v6.port())),
                "wildcard",
            ))
        }
        _ => None,
    }
}

#[cfg(all(windows, feature = "connect-lab"))]
struct Stored {
    owner: String,
    proof: OwnedChildProof,
    inventory: Inventory,
    rows: BTreeMap<String, SocketAddr>,
    deadline: Instant,
    review: Option<TargetReview>,
    in_flight: bool,
    verified: Option<(TargetStatus, SocketAddr)>,
}
#[derive(Default)]
pub struct DiscoveryManager {
    #[cfg(all(windows, feature = "connect-lab"))]
    state: Mutex<Option<Stored>>,
}
#[cfg(all(windows, feature = "connect-lab"))]
impl DiscoveryManager {
    pub fn inventory(
        &self,
        owner: &str,
        proof: OwnedChildProof,
    ) -> Result<Inventory, DiscoveryError> {
        if owner.is_empty() || owner.len() > 128 {
            return Err(DiscoveryError::InvalidRequest);
        }
        let mut state = self.state.lock().map_err(|_| DiscoveryError::Internal)?;
        if state.as_ref().is_some_and(|stored| stored.in_flight) {
            return Err(DiscoveryError::Busy);
        }
        proof.recheck()?;
        let listeners = windows::listeners(&proof)?;
        proof.recheck()?;
        let mut rows = BTreeMap::new();
        let mut candidates = Vec::new();
        for bind in listeners {
            let Some((target, kind)) = local_target(bind) else {
                continue;
            };
            if rows.values().any(|existing| *existing == bind) {
                continue;
            }
            if rows.len() >= 32 {
                return Err(DiscoveryError::Limit);
            }
            let id = uuid::Uuid::new_v4().to_string();
            rows.insert(id.clone(), bind);
            candidates.push(Candidate {
                candidate_id: id,
                target: target.to_string(),
                bind_kind: kind,
            });
        }
        let inventory = Inventory {
            inventory_id: uuid::Uuid::new_v4().to_string(),
            instance_id: proof.instance_id().into(),
            run_id: proof.run_id().into(),
            expires_at_ms: aurora_connect_lab::unix_millis() + 30_000,
            candidates,
        };
        *state = Some(Stored {
            owner: owner.into(),
            proof,
            inventory: inventory.clone(),
            rows,
            deadline: Instant::now() + Duration::from_secs(30),
            review: None,
            in_flight: false,
            verified: None,
        });
        Ok(inventory)
    }
    pub fn review(
        &self,
        owner: &str,
        inventory_id: &str,
        candidate_id: &str,
    ) -> Result<TargetReview, DiscoveryError> {
        let mut state = self.state.lock().map_err(|_| DiscoveryError::Internal)?;
        let stored = state.as_mut().ok_or(DiscoveryError::Stale)?;
        stored.check(owner)?;
        if stored.in_flight {
            return Err(DiscoveryError::Busy);
        }
        if stored.inventory.inventory_id != inventory_id {
            return Err(DiscoveryError::Stale);
        }
        let bind = *stored.rows.get(candidate_id).ok_or(DiscoveryError::Stale)?;
        windows::require_listener(&stored.proof, bind)?;
        let candidate = stored
            .inventory
            .candidates
            .iter()
            .find(|candidate| candidate.candidate_id == candidate_id)
            .cloned()
            .ok_or(DiscoveryError::Stale)?;
        let mut review = TargetReview {
            review_id: uuid::Uuid::new_v4().to_string(),
            digest: String::new(),
            inventory_id: inventory_id.into(),
            candidate,
            instance_id: stored.proof.instance_id().into(),
            run_id: stored.proof.run_id().into(),
            expires_at_ms: stored.inventory.expires_at_ms,
            status_protocol: 767,
            handshake_host: "localhost",
            handshake_port: bind.port(),
        };
        // Private ownership observations bind the hash but never enter the DTO.
        let bytes = serde_json::to_vec(&(stored.proof.fingerprint(), bind.to_string(), &review))
            .map_err(|_| DiscoveryError::Internal)?;
        review.digest = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        stored.check(owner)?;
        stored.review = Some(review.clone());
        stored.verified = None;
        Ok(review)
    }
    pub async fn confirm_status(
        &self,
        owner: &str,
        review_id: &str,
        digest: &str,
    ) -> Result<TargetStatus, DiscoveryError> {
        let (proof, bind, review, deadline) = {
            let mut state = self.state.lock().map_err(|_| DiscoveryError::Internal)?;
            let stored = state.as_mut().ok_or(DiscoveryError::Stale)?;
            stored.check(owner)?;
            if stored.in_flight {
                return Err(DiscoveryError::Busy);
            }
            let review = stored.review.take().ok_or(DiscoveryError::Stale)?;
            if review.review_id != review_id {
                return Err(DiscoveryError::Stale);
            }
            if review.digest != digest {
                return Err(DiscoveryError::Digest);
            }
            let bind = *stored
                .rows
                .get(&review.candidate.candidate_id)
                .ok_or(DiscoveryError::Stale)?;
            windows::require_listener(&stored.proof, bind)?;
            let proof = stored.proof.duplicate()?;
            stored.check(owner)?;
            stored.in_flight = true;
            (proof, bind, review, stored.deadline)
        };
        let _reset = Reset {
            manager: self,
            inventory_id: review.inventory_id.clone(),
        };
        let target: SocketAddr = review
            .candidate
            .target
            .parse()
            .map_err(|_| DiscoveryError::Internal)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(DiscoveryError::Stale);
        }
        let summary = aurora_connect_lab::probe_minecraft_status(
            aurora_connect_lab::MinecraftStatusRequest {
                endpoint: target,
                handshake_host: "localhost".into(),
                handshake_port: bind.port(),
                protocol: 767,
                timeout: remaining.min(aurora_connect_lab::MAX_MINECRAFT_STATUS_TIMEOUT),
            },
        )
        .await
        .map_err(|_| DiscoveryError::StatusFailed)?;
        windows::require_listener(&proof, bind)?;
        let mut state = self.state.lock().map_err(|_| DiscoveryError::Internal)?;
        let stored = state.as_mut().ok_or(DiscoveryError::Stale)?;
        stored.check(owner)?;
        if stored.inventory.inventory_id != review.inventory_id {
            return Err(DiscoveryError::Stale);
        }
        let status = TargetStatus {
            selection_id: uuid::Uuid::new_v4().to_string(),
            target: target.to_string(),
            instance_id: review.instance_id,
            run_id: review.run_id,
            summary: StatusSummary {
                requested_protocol: summary.requested_protocol,
                reported_protocol: summary.reported_protocol,
                protocol_matches_requested: summary.protocol_matches_requested,
                reported_players_online: summary.reported_players_online,
                reported_players_max: summary.reported_players_max,
                connect_micros: summary.connect_micros,
                status_micros: summary.status_micros,
                ping_micros: summary.ping_micros,
                total_micros: summary.total_micros,
            },
            verified_before_after: true,
            expires_at_ms: stored.inventory.expires_at_ms,
            world_state: "notVerified",
        };
        stored.verified = Some((status.clone(), bind));
        Ok(status)
    }
    /// Opaque, single-use target for a future trusted caller. Frontend cannot
    /// create it, provide a PID or change its selected destination.
    pub fn take_target(
        &self,
        owner: &str,
        selection_id: &str,
    ) -> Result<ApprovedTarget, DiscoveryError> {
        let mut state = self.state.lock().map_err(|_| DiscoveryError::Internal)?;
        let stored = state.as_mut().ok_or(DiscoveryError::Stale)?;
        stored.check(owner)?;
        if stored.in_flight {
            return Err(DiscoveryError::Busy);
        }
        let (selected, review_bind) = stored.verified.take().ok_or(DiscoveryError::Stale)?;
        if selected.selection_id != selection_id {
            return Err(DiscoveryError::Stale);
        }
        windows::require_listener(&stored.proof, review_bind)?;
        let proof = stored.proof.duplicate()?;
        stored.check(owner)?;
        Ok(ApprovedTarget {
            proof,
            bind: review_bind,
            target: selected
                .target
                .parse()
                .map_err(|_| DiscoveryError::Internal)?,
            deadline: stored.deadline,
        })
    }
}
#[cfg(all(windows, feature = "connect-lab"))]
impl Stored {
    fn check(&self, owner: &str) -> Result<(), DiscoveryError> {
        if self.owner != owner {
            return Err(DiscoveryError::Owner);
        }
        if Instant::now() >= self.deadline {
            return Err(DiscoveryError::Stale);
        }
        self.proof.recheck()
    }
}
#[cfg(all(windows, feature = "connect-lab"))]
struct Reset<'a> {
    manager: &'a DiscoveryManager,
    inventory_id: String,
}
#[cfg(all(windows, feature = "connect-lab"))]
impl Drop for Reset<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.manager.state.lock() {
            if let Some(stored) = state.as_mut() {
                if stored.inventory.inventory_id == self.inventory_id {
                    stored.in_flight = false;
                }
            }
        }
    }
}
#[cfg(all(windows, feature = "connect-lab"))]
pub struct ApprovedTarget {
    proof: OwnedChildProof,
    bind: SocketAddr,
    target: SocketAddr,
    deadline: Instant,
}
#[cfg(all(windows, feature = "connect-lab"))]
impl ApprovedTarget {
    pub fn recheck(&self) -> Result<SocketAddr, DiscoveryError> {
        if Instant::now() >= self.deadline {
            return Err(DiscoveryError::Stale);
        }
        windows::require_listener(&self.proof, self.bind)?;
        if Instant::now() >= self.deadline {
            return Err(DiscoveryError::Stale);
        }
        Ok(self.target)
    }
    /// Consume the 30s selection while preparing the host. Activation does not
    /// open transport or extend an invitation; the actor has its own 60s review.
    pub fn activate(self) -> Result<ActiveOwnedTarget, DiscoveryError> {
        self.recheck()?;
        Ok(ActiveOwnedTarget {
            proof: self.proof,
            bind: self.bind,
            target: self.target,
        })
    }
}
#[cfg(all(windows, feature = "connect-lab"))]
pub struct ActiveOwnedTarget {
    proof: OwnedChildProof,
    bind: SocketAddr,
    target: SocketAddr,
}
#[cfg(all(windows, feature = "connect-lab"))]
impl ActiveOwnedTarget {
    pub fn recheck_owned(&self) -> Result<SocketAddr, DiscoveryError> {
        windows::require_listener(&self.proof, self.bind)?;
        Ok(self.target)
    }
    pub fn instance_id(&self) -> &str {
        self.proof.instance_id()
    }
    pub fn run_id(&self) -> &str {
        self.proof.run_id()
    }
    pub(crate) fn fingerprint(&self) -> (u32, u64, String) {
        let (pid, birth) = self.proof.fingerprint();
        (pid, birth, self.bind.to_string())
    }
}
