//! A native process probe may exclude only a live receipted dedicated child
//! whose original, pinned working directory is disjoint from the target root.
//! This exports no constructor, PID adoption, IPC DTO or process-control API.
use super::*;
use crate::engine::connect_discovery::DiscoveryError;
use std::{fs::File, path::Path};

static OWNERS: Mutex<Vec<Weak<Shared>>> = Mutex::new(Vec::new());
const MAX_OWNERS: usize = 32;

pub(super) struct RootScope {
    components: Vec<String>,
    _pins: Vec<File>,
}
impl RootScope {
    pub(super) fn capture(root: &Path) -> Option<Self> {
        if !root.is_absolute() {
            return None;
        }
        // Pin the supplied path and every ancestor before canonical resolution;
        // junctions, symlinks and directory replacement are not accepted.
        let pins = crate::engine::recovery::pin_directory(root).ok()?;
        let canonical = root.canonicalize().ok()?;
        let components = canonical
            .components()
            .map(|part| part.as_os_str().to_string_lossy().to_lowercase())
            .collect();
        Some(Self {
            components,
            _pins: pins,
        })
    }
    fn disjoint(&self, other: &Self) -> bool {
        !self.components.starts_with(&other.components)
            && !other.components.starts_with(&self.components)
    }
}

pub(super) fn register(shared: &Arc<Shared>) {
    if shared.dedicated_root.is_none() {
        return;
    }
    if let Ok(mut owners) = OWNERS.lock() {
        owners.retain(|owner| owner.strong_count() != 0);
        if owners.len() < MAX_OWNERS {
            owners.push(Arc::downgrade(shared));
        }
    }
}

struct Entry {
    owner: Weak<Shared>,
    proof: Arc<OwnedChildProof>,
    _root: Arc<RootScope>,
}
impl Entry {
    fn identity(&self) -> Result<(u32, u64), DiscoveryError> {
        let owner = self.owner.upgrade().ok_or(DiscoveryError::ProcessChanged)?;
        let mut state = owner.lock();
        owner.refresh(&mut state);
        if state.reason.is_some()
            || state.completed
            || state
                .proof
                .as_ref()
                .is_none_or(|proof| !Arc::ptr_eq(proof, &self.proof))
        {
            return Err(DiscoveryError::ProcessChanged);
        }
        self.proof.live_process_identity()
    }
}

/// Short-lived native-only snapshot. It retains process handles and root pins
/// through the external query, but never extends the child/actor's lifetime.
pub struct OwnedDedicatedProbeScope {
    entries: Vec<Entry>,
    _target: Option<RootScope>,
}
impl OwnedDedicatedProbeScope {
    pub fn recheck(&self) -> Result<(), DiscoveryError> {
        for entry in &self.entries {
            entry.identity()?;
        }
        Ok(())
    }
    /// Private child-process environment, never a renderer response or log.
    pub fn child_environment(&self) -> Result<String, DiscoveryError> {
        let rows = self
            .entries
            .iter()
            .map(|entry| {
                let (pid, birth) = entry.identity()?;
                Ok(serde_json::json!({"pid":pid,"birth":birth.to_string()}))
            })
            .collect::<Result<Vec<_>, DiscoveryError>>()?;
        serde_json::to_string(&rows).map_err(|_| DiscoveryError::Unavailable)
    }
}

pub fn owned_dedicated_probe_scope(root: &Path) -> OwnedDedicatedProbeScope {
    let empty = || OwnedDedicatedProbeScope {
        entries: vec![],
        _target: None,
    };
    let Ok(mut owners) = OWNERS.lock() else {
        return empty();
    };
    owners.retain(|owner| owner.strong_count() != 0);
    if owners.is_empty() {
        return empty();
    }
    let Some(target) = RootScope::capture(root) else {
        return empty();
    };
    let mut entries = Vec::new();
    for weak in owners.iter() {
        let Some(owner) = weak.upgrade() else {
            continue;
        };
        let Some(scope) = owner
            .dedicated_root
            .as_ref()
            .filter(|scope| scope.disjoint(&target))
        else {
            continue;
        };
        let mut state = owner.lock();
        owner.refresh(&mut state);
        if state.reason.is_some() || state.completed {
            continue;
        }
        let Some(proof) = state.proof.clone() else {
            continue;
        };
        let entry = Entry {
            owner: weak.clone(),
            proof,
            _root: scope.clone(),
        };
        drop(state);
        if entry.identity().is_ok() {
            entries.push(entry);
        }
    }
    OwnedDedicatedProbeScope {
        entries,
        _target: Some(target),
    }
}
