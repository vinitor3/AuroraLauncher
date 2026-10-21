//! Private diagnostic factories and their closed EULA transactions.
//! Execution requires its own consumed review; there is no launcher/IPC entry point.
use crate::engine::{
    diagnostic_lab, recovery, Instance, InstanceId, InstanceLayout, InstanceMutationGuard,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};
use uuid::Uuid;

#[path = "connect_dedicated/eula.rs"]
mod eula;
#[path = "connect_dedicated/factory.rs"]
mod factory;
#[path = "connect_dedicated/manifest.rs"]
mod manifest;
#[path = "connect_dedicated/producer.rs"]
mod producer;
#[path = "connect_dedicated/resources.rs"]
mod resources;
#[path = "connect_dedicated/runtime_config.rs"]
mod runtime_config;
#[path = "connect_dedicated/voice_profile.rs"]
mod voice_profile;

pub(crate) use voice_profile::NativeVoiceProfile;

#[cfg(test)]
use eula::EulaOutcome;
use eula::EulaRecoveryPlan;
use factory::FactoryPlan;
#[cfg(test)]
use factory::PreparedStatus;

#[allow(dead_code)] // Closed experimental entry point; no IPC/production caller in84A.
pub(crate) fn plan_status_factory(stage: &Path, jdk: &Path, accepted: bool) -> Result<FactoryPlan> {
    factory::plan_status_factory(stage, jdk, accepted)
}
#[allow(dead_code)] // Closed file-only experimental entry point; no IPC caller.
pub(crate) fn plan_voice_factory(
    stage: &Path,
    jdk: &Path,
    profile: NativeVoiceProfile,
    accepted: bool,
) -> Result<FactoryPlan> {
    factory::plan_voice_factory(stage, jdk, profile, accepted)
}
#[allow(dead_code)] // Recovery is explicit; never invoked automatically at startup.
pub(crate) fn plan_eula_recovery(id: BundleId) -> Result<EulaRecoveryPlan> {
    eula::plan_eula_recovery(id)
}
#[allow(dead_code)] // Explicit native execution review, separate from file-only EULA.
pub(crate) fn plan_status_execution(
    prepared: factory::PreparedStatus,
) -> std::result::Result<producer::DedicatedPlan, producer::DedicatedError> {
    prepared.bundle.manifest.status_v1()?;
    producer::plan(prepared)
}
#[allow(dead_code)] // Separate native voice execution review; no IPC caller.
pub(crate) fn plan_voice_execution(
    prepared: factory::PreparedStatus,
) -> std::result::Result<producer::DedicatedPlan, producer::DedicatedError> {
    prepared.bundle.manifest.voice_v2()?;
    producer::plan_voice(prepared)
}

const REVIEW_TTL: Duration = Duration::from_secs(60);
const MAX_JOURNAL: u64 = 32 * 1024;
const EULA_FALSE: &[u8] = b"# https://www.minecraft.net/en-us/eula\neula=false\n";
const EULA_TRUE: &[u8] = b"# https://www.minecraft.net/en-us/eula\neula=true\n";
const RUNTIME: &[u8] = br#"{"minecraft":"1.21.1","loader":"fabric","versionId":"1.21.1"}"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Consent,
    UnsafePath,
    Resources,
    Changed,
    Identity,
    Busy,
    Expired,
    Digest,
    Consumed,
    NeedsRecovery,
    JavaRunning,
    ProbeUnavailable,
    Limit,
    Io,
    #[cfg(test)]
    Injected,
}
type Result<T> = std::result::Result<T, Error>;
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}
impl From<recovery::RecoveryError> for Error {
    fn from(error: recovery::RecoveryError) -> Self {
        use recovery::RecoveryError as R;
        match error {
            R::WrongLock => Self::Busy,
            R::VoiceProcessRunning => Self::JavaRunning,
            R::VoiceProcessUnavailable => Self::ProbeUnavailable,
            R::UnsafePath => Self::UnsafePath,
            R::Changed | R::OriginChanged => Self::Changed,
            R::Identity => Self::Identity,
            R::Pending => Self::NeedsRecovery,
            R::Limit => Self::Limit,
            _ => Self::Io,
        }
    }
}

/// Selection only; physical provenance and journal validation confer authority.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub(crate) struct BundleId(Uuid);

#[derive(Clone, Debug)]
#[allow(dead_code)] // Presentation fields are for the future trusted review caller.
pub(crate) struct Review {
    review_id: Uuid,
    pub(crate) digest: String,
    pub(crate) purpose: &'static str,
    pub(crate) bundle_id: BundleId,
    pub(crate) before: String,
    pub(crate) after: String,
    pub(crate) details: Vec<String>,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value).map_err(|_| Error::Identity)?;
    if bytes.len() as u64 > MAX_JOURNAL {
        return Err(Error::Limit);
    }
    Ok(bytes)
}
fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T> {
    let bytes = recovery::read_bounded(path, MAX_JOURNAL)?;
    serde_json::from_slice(&bytes).map_err(|_| Error::NeedsRecovery)
}
fn publish<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    recovery::write_new(path, &encode(value)?)?;
    Ok(())
}
fn approve(review: &Review, deadline: Instant, expected: &str) -> Result<()> {
    if Instant::now() >= deadline {
        return Err(Error::Expired);
    }
    if expected != review.digest {
        return Err(Error::Digest);
    }
    Ok(())
}
fn explicit_path(path: &Path) -> Result<()> {
    let mut components = path.components();
    match components.next() {
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(), std::path::Prefix::Disk(_)) => {}
        _ => return Err(Error::UnsafePath),
    }
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(Error::UnsafePath);
    }
    for component in components {
        let Component::Normal(name) = component else {
            return Err(Error::UnsafePath);
        };
        let name = name.to_str().ok_or(Error::UnsafePath)?;
        if name.is_empty() || name.contains([':', '\0']) || name.ends_with(['.', ' ']) {
            return Err(Error::UnsafePath);
        }
    }
    if path.as_os_str().len() > 1024 {
        return Err(Error::Limit);
    }
    Ok(())
}
fn lab_root() -> Result<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or(Error::UnsafePath)?
        .join(".aurora/connect-runtime");
    explicit_path(&root)?;
    recovery::pin_directory(&root)?;
    Ok(root)
}
fn absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(Error::Consumed),
        Err(_) => Err(Error::Io),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct FileIdentity {
    volume: u32,
    index: u64,
}
fn file_identity(file: &File) -> Result<FileIdentity> {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    struct Info {
        values: [u32; 13],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut Info) -> i32;
    }
    let mut info = Info { values: [0; 13] };
    // SAFETY: DWORD-only BY_HANDLE_FILE_INFORMATION layout; the borrowed File
    // owns a live handle and the initialized output remains valid for this call.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(Error::Io);
    }
    Ok(FileIdentity {
        volume: info.values[7],
        index: (u64::from(info.values[11]) << 32) | u64::from(info.values[12]),
    })
}
fn directory_identity(path: &Path) -> Result<FileIdentity> {
    let pins = recovery::pin_directory(path)?;
    file_identity(pins.first().ok_or(Error::Identity)?)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Observation {
    identity: FileIdentity,
    bytes: u64,
    sha256: String,
}
fn observe_file(path: &Path, max: u64) -> Result<(Observation, Vec<u8>)> {
    let mut file = recovery::secure_open(path)?;
    let identity = file_identity(&file)?;
    let size = file.metadata()?.len();
    if size > max {
        return Err(Error::Limit);
    }
    let mut bytes = Vec::new();
    (&mut file).take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != size {
        return Err(Error::Changed);
    }
    Ok((
        Observation {
            identity,
            bytes: size,
            sha256: digest(&bytes),
        },
        bytes,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Claim,
    Snapshot,
    Stage,
    Intent,
    OriginalMoved,
    OriginalMarked,
    AppliedMoved,
    AppliedMarked,
    RestoreIntent,
    AppliedParked,
    OriginalRestored,
    RestoredMarked,
    MayHaveSpawned,
}
#[cfg(test)]
#[derive(Clone, Default)]
struct Hooks {
    before_claim: Option<std::sync::Arc<dyn Fn() -> Result<()> + Send + Sync>>,
    after_step: Option<std::sync::Arc<dyn Fn(Step) -> Result<()> + Send + Sync>>,
    eula_renames: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
#[cfg(not(test))]
#[derive(Clone, Default)]
struct Hooks;
impl Hooks {
    fn before_claim(&self) -> Result<()> {
        #[cfg(test)]
        if let Some(hook) = &self.before_claim {
            hook()?;
        }
        Ok(())
    }
    fn checkpoint(&self, step: Step) -> Result<()> {
        #[cfg(test)]
        if let Some(after) = &self.after_step {
            after(step)?;
        }
        #[cfg(not(test))]
        let _ = step;
        Ok(())
    }
    fn renamed(&self) {
        #[cfg(test)]
        self.eula_renames
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
#[derive(Clone)]
struct TestPaths {
    bundle: PathBuf,
    copy: PathBuf,
    origin: PathBuf,
    transaction: PathBuf,
}

#[cfg(test)]
fn fixture_plan(
    lab: &Path,
    stage: &Path,
    jdk: &Path,
    accepted: bool,
    hooks: Hooks,
) -> Result<FactoryPlan> {
    factory::plan(lab, stage, jdk, accepted, resources::fixture_pins(), hooks)
}
#[cfg(test)]
type TestProbe = Box<dyn Fn() -> std::result::Result<(), recovery::RecoveryError> + Send + Sync>;
#[cfg(test)]
fn fixture_recovery(
    lab: &Path,
    id: BundleId,
    probe: TestProbe,
    hooks: Hooks,
) -> Result<EulaRecoveryPlan> {
    eula::prepare_recovery(lab, id, Some(probe), hooks)
}

#[cfg(test)]
#[path = "connect_dedicated/tests.rs"]
mod tests;
