//! Read-only, local laboratory service plans. This is not the peer wire schema,
//! a modpack manifest, a mod configuration editor or a production invitation.
use crate::{
    Capability, DATAGRAM_HEADER_LEN, Limits, Protocol, SCHEMA_VERSION, Service, UdpReturnPolicy,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::Read,
    net::SocketAddr,
    path::{Component, Path},
};

pub const MAX_PLAN_BYTES: usize = 16 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlanError {
    #[error("plan_read_failed")]
    Read,
    #[error("plan_requires_regular_file")]
    NotRegular,
    #[error("plan_linked_path_rejected")]
    LinkedPath,
    #[error("plan_too_large")]
    TooLarge,
    #[error("plan_invalid_json")]
    InvalidJson,
    #[error("plan_invalid_contract")]
    InvalidPlan,
    #[error("plan_invalid_approval_hash")]
    InvalidApproval,
    #[error("plan_changed_since_review")]
    Changed,
}
impl PlanError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Read => "plan_read_failed",
            Self::NotRegular => "plan_requires_regular_file",
            Self::LinkedPath => "plan_linked_path_rejected",
            Self::TooLarge => "plan_too_large",
            Self::InvalidJson => "plan_invalid_json",
            Self::InvalidPlan => "plan_invalid_contract",
            Self::InvalidApproval => "plan_invalid_approval_hash",
            Self::Changed => "plan_changed_since_review",
        }
    }
}

/// A reviewed byte snapshot. Consuming this structure does not read the file again.
/// The caller still creates fresh session/transport keys and authorizes a peer.
#[derive(Debug)]
pub struct ValidatedPlan {
    pub hash: String,
    pub bind: SocketAddr,
    pub seconds: u64,
    pub services: Vec<Service>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ServicePlan {
    schema_version: u8,
    bind: SocketAddr,
    duration_seconds: u64,
    services: Vec<PlanService>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlanService {
    service_id: String,
    protocol: Protocol,
    endpoint: SocketAddr,
    revision: u32,
    required_capability: Capability,
    max_datagram_bytes: usize,
    #[serde(default)]
    udp_return_policy: PlanReturnPolicy,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
enum PlanReturnPolicy {
    ReplyBudget {},
    AuthorizedDownstream {
        #[serde(rename = "bytesPerSecond")]
        bytes_per_second: u32,
        #[serde(rename = "datagramsPerSecond")]
        datagrams_per_second: u32,
    },
}

impl Default for PlanReturnPolicy {
    fn default() -> Self {
        Self::ReplyBudget {}
    }
}

impl From<PlanReturnPolicy> for UdpReturnPolicy {
    fn from(policy: PlanReturnPolicy) -> Self {
        match policy {
            PlanReturnPolicy::ReplyBudget {} => Self::ReplyBudget,
            PlanReturnPolicy::AuthorizedDownstream {
                bytes_per_second,
                datagrams_per_second,
            } => Self::AuthorizedDownstream {
                bytes_per_second,
                datagrams_per_second,
            },
        }
    }
}

impl From<UdpReturnPolicy> for PlanReturnPolicy {
    fn from(policy: UdpReturnPolicy) -> Self {
        match policy {
            UdpReturnPolicy::ReplyBudget => Self::ReplyBudget {},
            UdpReturnPolicy::AuthorizedDownstream {
                bytes_per_second,
                datagrams_per_second,
            } => Self::AuthorizedDownstream {
                bytes_per_second,
                datagrams_per_second,
            },
        }
    }
}

impl ValidatedPlan {
    /// Local review output intentionally contains the endpoints being approved.
    /// Do not use this as a shareable connectivity log: it is not redacted telemetry.
    pub fn preview(&self) -> Value {
        let services: Vec<_> = self
            .services
            .iter()
            .map(|s| PlanService {
                service_id: s.service_id.clone(),
                protocol: s.protocol,
                endpoint: s.endpoint,
                revision: s.revision,
                required_capability: s.required_capability,
                max_datagram_bytes: s.max_datagram_bytes,
                udp_return_policy: s.udp_return_policy.into(),
            })
            .collect();
        json!({
            "schemaKind": "localLabServicePlan", "schemaVersion": SCHEMA_VERSION,
            "experimental": true, "hashAlgorithm": "sha256", "hash": self.hash,
            "before": {"session": "notStarted"},
            "after": {"bind": self.bind, "durationSeconds": self.seconds, "services": services},
            "filesChanged": [], "relay": false, "modConfigurationChanges": false,
            "authorization": "requiresExplicitHashAndPinnedGuest", "rollback": "revokeAndStopLabProcess",
            "minecraft": "notVerified", "simpleVoiceChat": "notVerified", "plasmoVoice": "notVerified"
        })
    }
}

/// Load, bound, hash and validate a plan without opening any network socket or
/// changing a file. Hash approval binds the exact bytes, including whitespace/BOM.
pub fn load_plan(
    path: impl AsRef<Path>,
    expected_hash: Option<&str>,
) -> Result<ValidatedPlan, PlanError> {
    if expected_hash
        .is_some_and(|hash| hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(PlanError::InvalidApproval);
    }
    let path = path.as_ref();
    if !is_local_plain_path(path) {
        return Err(PlanError::InvalidPlan);
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| PlanError::Read)?
            .join(path)
    };
    if !is_local_plain_path(&absolute) {
        return Err(PlanError::InvalidPlan);
    }
    // Reject links/reparse points on the path, including directory junctions.
    // The file is then opened once and all validation consumes that handle.
    for ancestor in absolute.ancestors().collect::<Vec<_>>().into_iter().rev() {
        let metadata = fs::symlink_metadata(ancestor).map_err(|_| PlanError::Read)?;
        if is_link(&metadata) {
            return Err(PlanError::LinkedPath);
        }
        if ancestor == absolute && !metadata.is_file() {
            return Err(PlanError::NotRegular);
        }
    }
    let file = open_plain_file(&absolute)?;
    let metadata = file.metadata().map_err(|_| PlanError::Read)?;
    if is_link(&metadata) {
        return Err(PlanError::LinkedPath);
    }
    if !metadata.is_file() {
        return Err(PlanError::NotRegular);
    }
    if metadata.len() > MAX_PLAN_BYTES as u64 {
        return Err(PlanError::TooLarge);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_PLAN_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| PlanError::Read)?;
    if bytes.len() > MAX_PLAN_BYTES {
        return Err(PlanError::TooLarge);
    }
    let mut hash = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in Sha256::digest(&bytes) {
        hash.push(HEX[(byte >> 4) as usize] as char);
        hash.push(HEX[(byte & 0x0f) as usize] as char);
    }
    if expected_hash.is_some_and(|expected| !expected.eq_ignore_ascii_case(&hash)) {
        return Err(PlanError::Changed);
    }
    // PowerShell 5 UTF-8 exports can carry a BOM. It remains covered by the hash.
    let json_bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    let plan: ServicePlan =
        serde_json::from_slice(json_bytes).map_err(|_| PlanError::InvalidJson)?;
    validate(plan, hash)
}

fn is_local_plain_path(path: &Path) -> bool {
    if path.as_os_str().is_empty() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return false;
    }
    #[cfg(windows)]
    {
        use std::path::Prefix;
        // Reject network/device namespaces and drive-relative/root-relative paths
        // lexically: checking their metadata could already contact SMB or a device.
        if path.has_root() && !path.is_absolute() {
            return false;
        }
        for component in path.components() {
            match component {
                Component::Prefix(prefix)
                    if !path.is_absolute()
                        || !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)) =>
                {
                    return false;
                }
                Component::Normal(name) => {
                    let Some(name) = name.to_str() else {
                        return false;
                    };
                    // NTFS alternate data streams and Win32 normalized aliases
                    // must not turn an apparently regular path into another object.
                    if name.contains(':') || name.ends_with(['.', ' ']) {
                        return false;
                    }
                    let stem = name
                        .split('.')
                        .next()
                        .unwrap_or_default()
                        .to_ascii_uppercase();
                    if matches!(
                        stem.as_str(),
                        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
                    ) || ["COM", "LPT"].iter().any(|prefix| {
                        stem.strip_prefix(prefix).is_some_and(|number| {
                            matches!(
                                number,
                                "1" | "2"
                                    | "3"
                                    | "4"
                                    | "5"
                                    | "6"
                                    | "7"
                                    | "8"
                                    | "9"
                                    | "¹"
                                    | "²"
                                    | "³"
                            )
                        })
                    }) {
                        return false;
                    }
                }
                _ => {}
            }
        }
    }
    true
}

fn open_plain_file(path: &Path) -> Result<File, PlanError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Open the reparse point itself instead of following a final-component link.
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path).map_err(|_| PlanError::Read)
}

fn is_link(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

fn validate(plan: ServicePlan, hash: String) -> Result<ValidatedPlan, PlanError> {
    if plan.schema_version != SCHEMA_VERSION
        || !(10..=3600).contains(&plan.duration_seconds)
        || plan.services.len() != 2
        || plan.bind.port() == 0
        || plan.bind.ip().is_multicast()
        || (plan.bind.is_ipv4() && plan.bind.ip() == std::net::Ipv4Addr::BROADCAST)
    {
        return Err(PlanError::InvalidPlan);
    }
    let limits = Limits::default();
    let mut seen_game = false;
    let mut seen_aux = false;
    let mut services = Vec::with_capacity(2);
    for planned in plan.services {
        let policy: UdpReturnPolicy = planned.udp_return_policy.into();
        if !planned.endpoint.ip().is_loopback()
            || planned.endpoint.port() == 0
            || planned.revision != 1
            || planned.max_datagram_bytes > 65507
        {
            return Err(PlanError::InvalidPlan);
        }
        match planned.service_id.as_str() {
            "game"
                if !seen_game
                    && planned.protocol == Protocol::Tcp
                    && planned.required_capability == Capability::ReliableStream
                    && policy == UdpReturnPolicy::ReplyBudget =>
            {
                seen_game = true
            }
            "aux"
                if !seen_aux
                    && planned.protocol == Protocol::Udp
                    && planned.required_capability == Capability::Datagram =>
            {
                seen_aux = true
            }
            _ => return Err(PlanError::InvalidPlan),
        }
        if let UdpReturnPolicy::AuthorizedDownstream {
            bytes_per_second,
            datagrams_per_second,
        } = policy
            && (bytes_per_second < DATAGRAM_HEADER_LEN as u32
                || bytes_per_second > limits.udp_bytes_per_second
                || datagrams_per_second == 0
                || datagrams_per_second > limits.datagrams_per_second)
        {
            return Err(PlanError::InvalidPlan);
        }
        services.push(Service {
            service_id: planned.service_id,
            protocol: planned.protocol,
            endpoint: planned.endpoint,
            revision: planned.revision,
            max_datagram_bytes: planned.max_datagram_bytes,
            required_capability: planned.required_capability,
            udp_return_policy: policy,
        });
    }
    if !seen_game || !seen_aux {
        return Err(PlanError::InvalidPlan);
    }
    Ok(ValidatedPlan {
        hash,
        bind: plan.bind,
        seconds: plan.duration_seconds,
        services,
    })
}
