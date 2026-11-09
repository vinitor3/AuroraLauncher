use super::super::VoiceMutationGuard;
use super::*;
use factory::{Bundle, PreparedStatus};
use manifest::Manifest;

#[path = "producer_worker.rs"]
pub(super) mod producer_worker;

const CLAIM: &str = "claim.json";
const INTENT: &str = "intent.json";
const SNAPSHOT: &str = "snapshot.bin";
const STAGE: &str = "stage.bin";
const OLD: &str = "old.bin";
const APPLIED: &str = "applied.bin";
const ORIGINAL_MARK: &str = "original-moved.json";
const APPLIED_MARK: &str = "applied.json";
const SPAWN_MARK: &str = "may-have-spawned.json";
const RESTORE_INTENT: &str = "restore-intent.json";
const RESTORED: &str = "restored.json";
const SLOTS: [&str; 11] = [
    CLAIM,
    INTENT,
    SNAPSHOT,
    STAGE,
    OLD,
    APPLIED,
    ORIGINAL_MARK,
    APPLIED_MARK,
    SPAWN_MARK,
    RESTORE_INTENT,
    RESTORED,
];

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum EulaOutcome {
    Restored,
    Unchanged,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Physical {
    Unchanged,
    Prepared,
    OriginalParked,
    Applied,
    AppliedParked,
    Restored,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Claim {
    schema: u32,
    bundle: BundleId,
    transaction: Uuid,
    manifest_sha256: String,
    review_digest: String,
    original: Observation,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Intent {
    schema: u32,
    transaction: Uuid,
    snapshot: Observation,
    stage: Observation,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Phase {
    OriginalMoved,
    Applied,
    MayHaveSpawned,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Marker {
    schema: u32,
    transaction: Uuid,
    phase: Phase,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RestoreIntent {
    schema: u32,
    transaction: Uuid,
    from: Physical,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Restored {
    schema: u32,
    transaction: Uuid,
    outcome: EulaOutcome,
}
struct State {
    claim: Claim,
    physical: Physical,
    restore: Option<RestoreIntent>,
    terminal: Option<Restored>,
    digest: String,
    may_have_spawned: bool,
}

pub(crate) struct AppliedEula {
    prepared: PreparedStatus,
    transaction: Uuid,
    reviewed_digest: String,
    may_have_spawned: bool,
}
pub(crate) struct EulaRecoveryPlan {
    bundle: Bundle,
    guard: VoiceMutationGuard,
    review: Review,
    state_digest: String,
    deadline: Instant,
    hooks: Hooks,
}
// This inner retains the original fields, order and serde attributes. V1
// review preimages and the B0 persisted goldens bind these exact bytes.
#[derive(Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(Deserialize))]
pub(super) struct ApprovedFilesV1 {
    original: Observation,
    properties: Observation,
}
#[derive(Serialize, PartialEq, Eq)]
pub(super) struct ApprovedFilesV2 {
    original: Observation,
    properties: Observation,
    voice_configuration: Observation,
}
#[derive(PartialEq, Eq)]
pub(super) enum ApprovedFiles {
    StatusV1(ApprovedFilesV1),
    VoiceV2(ApprovedFilesV2),
}
impl Serialize for ApprovedFiles {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::StatusV1(files) => files.serialize(serializer),
            Self::VoiceV2(files) => files.serialize(serializer),
        }
    }
}
impl ApprovedFiles {
    pub(super) fn require_manifest_kind(&self, manifest: &Manifest) -> Result<()> {
        match (manifest, self) {
            (Manifest::StatusV1(_), Self::StatusV1(_))
            | (Manifest::VoiceV2(_), Self::VoiceV2(_)) => Ok(()),
            _ => Err(Error::Identity),
        }
    }

    fn original(&self) -> &Observation {
        match self {
            Self::StatusV1(files) => &files.original,
            Self::VoiceV2(files) => &files.original,
        }
    }

    fn properties(&self) -> &Observation {
        match self {
            Self::StatusV1(files) => &files.properties,
            Self::VoiceV2(files) => &files.properties,
        }
    }
}

pub(super) fn execution_digest(bundle: &Bundle, review_id: Uuid) -> Result<String> {
    digest_observations(bundle, review_id, &execution_observations(bundle)?)
}
fn execution_observations(bundle: &Bundle) -> Result<ApprovedFiles> {
    absent(&bundle.transaction.join(CLAIM))?;
    if fs::read_dir(&bundle.transaction)?.next().is_some() {
        return Err(Error::NeedsRecovery);
    }
    let (eula, bytes) = observe_file(&bundle.copy.root().join("eula.txt"), 4096)?;
    let (properties, configuration) =
        observe_file(&bundle.copy.root().join("server.properties"), 4096)?;
    if bytes != EULA_FALSE || configuration != resources::properties(&bundle.manifest) {
        return Err(Error::Changed);
    }
    match &bundle.manifest {
        Manifest::StatusV1(_) => Ok(ApprovedFiles::StatusV1(ApprovedFilesV1 {
            original: eula,
            properties,
        })),
        Manifest::VoiceV2(_) => Ok(ApprovedFiles::VoiceV2(ApprovedFilesV2 {
            original: eula,
            properties,
            voice_configuration: initial_voice_configuration(bundle)?,
        })),
    }
}

fn initial_voice_configuration(bundle: &Bundle) -> Result<Observation> {
    let voice = bundle.manifest.voice_v2()?;
    // The closed profile selects the compiled path. The persisted name never
    // chooses a join, and the persisted observation never approves a new file.
    if voice.initial_voice_configuration.name != voice.profile.configuration_name() {
        return Err(Error::Identity);
    }
    let (observed, bytes) = observe_file(
        &bundle.copy.root().join(voice.profile.configuration_name()),
        256 * 1024,
    )?;
    if observed != voice.initial_voice_configuration.observation
        || bytes != voice.observation_policy.configuration(voice.profile)
    {
        return Err(Error::Changed);
    }
    Ok(observed)
}
fn digest_observations(
    bundle: &Bundle,
    review_id: Uuid,
    observations: &ApprovedFiles,
) -> Result<String> {
    Ok(digest(&eula_digest_input(
        &bundle.manifest,
        review_id,
        observations,
    )?))
}
fn eula_digest_input(
    manifest: &factory::Manifest,
    review_id: Uuid,
    observations: &ApprovedFiles,
) -> Result<Vec<u8>> {
    match (manifest, observations) {
        (Manifest::StatusV1(_), ApprovedFiles::StatusV1(_)) => encode(&(
            1,
            "native_status_eula",
            review_id,
            manifest,
            observations,
            digest(EULA_TRUE),
        )),
        (Manifest::VoiceV2(_), ApprovedFiles::VoiceV2(_)) => encode(&(
            2,
            "native_voice_eula_v2",
            review_id,
            manifest,
            observations,
            digest(EULA_TRUE),
        )),
        _ => Err(Error::Identity),
    }
}
pub(super) fn execution_review(
    bundle: &Bundle,
    guard: &InstanceMutationGuard,
) -> Result<(Review, ApprovedFiles)> {
    bundle.validate(guard)?;
    let review_id = Uuid::new_v4();
    let observations = execution_observations(bundle)?;
    Ok((
        Review {
            review_id,
            digest: digest_observations(bundle, review_id, &observations)?,
            purpose: match &bundle.manifest {
                Manifest::StatusV1(_) => "native_status_eula",
                Manifest::VoiceV2(_) => "native_voice_eula_v2",
            },
            bundle_id: bundle.manifest.id(),
            before: String::from_utf8_lossy(EULA_FALSE).into(),
            after: format!(
                "{}Restauração byte-exata do original após o término próprio.",
                String::from_utf8_lossy(EULA_TRUE)
            ),
            details: {
                let mut details = vec![
                    bundle.copy.root().display().to_string(),
                    "Uso único; revisão EULA; nenhuma execução Java neste checkpoint".into(),
                ];
                if let Manifest::VoiceV2(voice) = &bundle.manifest {
                    details.push(match voice.profile {
                        manifest::StoredVoiceProfileV2::Svc => {
                            "Perfil: Simple Voice Chat 2.6.22".into()
                        }
                        manifest::StoredVoiceProfileV2::Plasmo => {
                            "Perfil: Plasmo Voice 2.1.17; Fabric API 0.116.10+1.21.1".into()
                        }
                    });
                    details.push(format!(
                        "Runtime aprovado: Minecraft {}; Fabric {}; protocolo {}",
                        voice_profile::MINECRAFT,
                        voice_profile::LOADER_VERSION,
                        voice_profile::PROTOCOL,
                    ));
                    details.push(format!(
                        "Configuração inicial compilada: {}; {} bytes; SHA-256 {}",
                        voice.profile.configuration_name(),
                        voice.observation_policy.configuration(voice.profile).len(),
                        voice.observation_policy.configuration_sha256(voice.profile),
                    ));
                    let initial = &voice.initial_voice_configuration.observation;
                    details.push(format!(
                        "Observação inicial: volume {}; índice {}; {} bytes; SHA-256 {}",
                        initial.identity.volume,
                        initial.identity.index,
                        initial.bytes,
                        initial.sha256,
                    ));
                }
                details
            },
        },
        observations,
    ))
}
fn names(bundle: &Bundle) -> Result<std::collections::BTreeSet<String>> {
    let mut names = std::collections::BTreeSet::new();
    for entry in fs::read_dir(&bundle.transaction)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| Error::UnsafePath)?;
        if !SLOTS.contains(&name.as_str()) || !names.insert(name) || names.len() > SLOTS.len() {
            return Err(Error::NeedsRecovery);
        }
        let _ = recovery::secure_open(&entry.path())?;
    }
    Ok(names)
}
fn optional(
    bundle: &Bundle,
    name: &str,
    names: &std::collections::BTreeSet<String>,
) -> Result<Option<Observation>> {
    if names.contains(name) {
        Ok(Some(observe_file(&bundle.transaction.join(name), 4096)?.0))
    } else {
        Ok(None)
    }
}
fn target(bundle: &Bundle) -> Result<Option<Observation>> {
    let path = bundle.copy.root().join("eula.txt");
    match fs::symlink_metadata(&path) {
        Ok(_) => Ok(Some(observe_file(&path, 4096)?.0)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(Error::Io),
    }
}
fn marker(
    bundle: &Bundle,
    names: &std::collections::BTreeSet<String>,
    name: &str,
    phase: Phase,
    transaction: Uuid,
) -> Result<bool> {
    if !names.contains(name) {
        return Ok(false);
    }
    let actual: Marker = read_json(&bundle.transaction.join(name))?;
    if actual
        != (Marker {
            schema: 1,
            transaction,
            phase,
        })
    {
        return Err(Error::NeedsRecovery);
    }
    Ok(true)
}
fn read_state(bundle: &Bundle) -> Result<State> {
    let names = names(bundle)?;
    let claim: Claim = read_json(&bundle.transaction.join(CLAIM))?;
    if claim.schema != 1
        || claim.bundle != bundle.manifest.id()
        || claim.manifest_sha256 != digest(&encode(&bundle.manifest)?)
        || claim.original.bytes != EULA_FALSE.len() as u64
        || claim.original.sha256 != digest(EULA_FALSE)
        || claim.review_digest.len() != 64
    {
        return Err(Error::NeedsRecovery);
    }
    let current = target(bundle)?;
    let intent: Option<Intent> = if names.contains(INTENT) {
        Some(read_json(&bundle.transaction.join(INTENT))?)
    } else {
        None
    };
    let physical = if let Some(intent) = &intent {
        if intent.schema != 1
            || intent.transaction != claim.transaction
            || intent.snapshot.bytes != EULA_FALSE.len() as u64
            || intent.snapshot.sha256 != digest(EULA_FALSE)
            || intent.stage.bytes != EULA_TRUE.len() as u64
            || intent.stage.sha256 != digest(EULA_TRUE)
            || intent.snapshot.identity == claim.original.identity
            || intent.stage.identity == claim.original.identity
            || intent.stage.identity == intent.snapshot.identity
        {
            return Err(Error::NeedsRecovery);
        }
        if optional(bundle, SNAPSHOT, &names)?.as_ref() != Some(&intent.snapshot) {
            return Err(Error::NeedsRecovery);
        }
        let n = optional(bundle, STAGE, &names)?;
        let o = optional(bundle, OLD, &names)?;
        let a = optional(bundle, APPLIED, &names)?;
        if n.as_ref().is_some_and(|v| v != &intent.stage)
            || o.as_ref().is_some_and(|v| v != &claim.original)
            || a.as_ref().is_some_and(|v| v != &intent.stage)
        {
            return Err(Error::NeedsRecovery);
        }
        match (n.is_some(), o.is_some(), a.is_some(), current.as_ref()) {
            (true, false, false, Some(t)) if t == &claim.original => Physical::Prepared,
            (true, true, false, None) => Physical::OriginalParked,
            (false, true, false, Some(t)) if t == &intent.stage => Physical::Applied,
            (false, true, true, None) => Physical::AppliedParked,
            (false, false, true, Some(t)) if t == &claim.original => Physical::Restored,
            _ => return Err(Error::NeedsRecovery),
        }
    } else {
        if names
            .iter()
            .any(|n| ![CLAIM, RESTORE_INTENT, RESTORED].contains(&n.as_str()))
            || current.as_ref() != Some(&claim.original)
        {
            return Err(Error::NeedsRecovery);
        }
        Physical::Unchanged
    };
    let original = marker(
        bundle,
        &names,
        ORIGINAL_MARK,
        Phase::OriginalMoved,
        claim.transaction,
    )?;
    let applied = marker(
        bundle,
        &names,
        APPLIED_MARK,
        Phase::Applied,
        claim.transaction,
    )?;
    let spawned = marker(
        bundle,
        &names,
        SPAWN_MARK,
        Phase::MayHaveSpawned,
        claim.transaction,
    )?;
    let restore: Option<RestoreIntent> = if names.contains(RESTORE_INTENT) {
        Some(read_json(&bundle.transaction.join(RESTORE_INTENT))?)
    } else {
        None
    };
    if applied && !original || spawned && !applied {
        return Err(Error::NeedsRecovery);
    }
    if matches!(
        physical,
        Physical::Applied | Physical::AppliedParked | Physical::Restored
    ) && !original
    {
        return Err(Error::NeedsRecovery);
    }
    if matches!(physical, Physical::Unchanged | Physical::OriginalParked) && applied {
        return Err(Error::NeedsRecovery);
    }
    if physical == Physical::Prepared && (applied || (original && restore.is_none())) {
        return Err(Error::NeedsRecovery);
    }
    if let Some(r) = &restore {
        let allowed = match r.from {
            Physical::Unchanged => physical == Physical::Unchanged,
            Physical::Prepared => physical == Physical::Prepared,
            Physical::OriginalParked => {
                matches!(physical, Physical::OriginalParked | Physical::Prepared)
            }
            Physical::Applied => matches!(
                physical,
                Physical::Applied | Physical::AppliedParked | Physical::Restored
            ),
            Physical::AppliedParked => {
                matches!(physical, Physical::AppliedParked | Physical::Restored)
            }
            Physical::Restored => physical == Physical::Restored,
        };
        if r.schema != 1 || r.transaction != claim.transaction || !allowed {
            return Err(Error::NeedsRecovery);
        }
    } else if matches!(physical, Physical::AppliedParked | Physical::Restored) {
        return Err(Error::NeedsRecovery);
    }
    let terminal: Option<Restored> = if names.contains(RESTORED) {
        Some(read_json(&bundle.transaction.join(RESTORED))?)
    } else {
        None
    };
    if let Some(done) = &terminal {
        if restore.is_none()
            || done.schema != 1
            || done.transaction != claim.transaction
            || match done.outcome {
                EulaOutcome::Restored => physical != Physical::Restored,
                EulaOutcome::Unchanged => {
                    !matches!(physical, Physical::Unchanged | Physical::Prepared)
                }
            }
        {
            return Err(Error::NeedsRecovery);
        }
    }
    // Review binds physical identities of metadata too, while S/N/O/A/T are
    // additionally bound to their original identities across rename/reopen.
    let mut observations = Vec::new();
    for name in &names {
        observations.push((
            name.clone(),
            observe_file(&bundle.transaction.join(name), MAX_JOURNAL)?.0,
        ));
    }
    let state_digest = digest(&encode(&(
        claim.transaction,
        physical,
        current,
        observations,
    ))?);
    Ok(State {
        claim,
        physical,
        restore,
        terminal,
        digest: state_digest,
        may_have_spawned: spawned,
    })
}
fn write_marker(bundle: &Bundle, transaction: Uuid, name: &str, phase: Phase) -> Result<()> {
    publish(
        &bundle.transaction.join(name),
        &Marker {
            schema: 1,
            transaction,
            phase,
        },
    )
}

pub(super) fn apply(prepared: PreparedStatus, expected: &str) -> Result<AppliedEula> {
    if expected != prepared.review_digest() {
        return Err(Error::Digest);
    }
    apply_reviewed(prepared, ApplyReview::Files(expected))
}
pub(super) fn apply_execution(
    prepared: PreparedStatus,
    approval: &producer::ExecutionApproval,
) -> Result<AppliedEula> {
    prepared
        .approved_files
        .require_manifest_kind(&prepared.bundle.manifest)?;
    approval.check(&prepared)?;
    apply_reviewed(prepared, ApplyReview::Execution(approval))
}
enum ApplyReview<'a> {
    Files(&'a str),
    Execution(&'a producer::ExecutionApproval),
}
impl ApplyReview<'_> {
    fn digest(&self) -> &str {
        match self {
            Self::Files(expected) => expected,
            Self::Execution(approval) => &approval.review.digest,
        }
    }
    fn check_before_claim(&self, prepared: &PreparedStatus) -> Result<()> {
        match self {
            Self::Files(expected) => prepared.confirm_fresh(expected),
            Self::Execution(approval) => approval.check(prepared),
        }
    }
}
fn apply_reviewed(prepared: PreparedStatus, review: ApplyReview<'_>) -> Result<AppliedEula> {
    let bundle = &prepared.bundle;
    prepared
        .approved_files
        .require_manifest_kind(&bundle.manifest)?;
    prepared.hooks.before_claim()?;
    absent(&bundle.transaction.join(CLAIM))?;
    if fs::read_dir(&bundle.transaction)?.next().is_some() {
        return Err(Error::NeedsRecovery);
    }
    let observed = execution_observations(bundle)?;
    if observed != prepared.approved_files {
        return Err(Error::Changed);
    }
    let original = observed.original().clone();
    let bytes = EULA_FALSE;
    let transaction = Uuid::new_v4();
    let claim = Claim {
        schema: 1,
        bundle: bundle.manifest.id(),
        transaction,
        manifest_sha256: digest(&encode(&bundle.manifest)?),
        review_digest: review.digest().into(),
        original,
    };
    // The original ApprovedFiles observation is used above and remains the
    // Claim baseline. Execution additionally rechecks its separate recipe
    // digest and the same original deadline before the one Claim publication.
    review.check_before_claim(&prepared)?;
    publish(&bundle.transaction.join(CLAIM), &claim)?;
    prepared.hooks.checkpoint(Step::Claim)?;
    recovery::write_new(&bundle.transaction.join(SNAPSHOT), bytes)?;
    prepared.hooks.checkpoint(Step::Snapshot)?;
    recovery::write_new(&bundle.transaction.join(STAGE), EULA_TRUE)?;
    prepared.hooks.checkpoint(Step::Stage)?;
    let intent = Intent {
        schema: 1,
        transaction,
        snapshot: observe_file(&bundle.transaction.join(SNAPSHOT), 4096)?.0,
        stage: observe_file(&bundle.transaction.join(STAGE), 4096)?.0,
    };
    publish(&bundle.transaction.join(INTENT), &intent)?;
    prepared.hooks.checkpoint(Step::Intent)?;
    bundle.validate(&prepared.guard)?;
    if read_state(bundle)?.physical != Physical::Prepared {
        return Err(Error::NeedsRecovery);
    }
    recovery::durable_rename(
        &bundle.copy.root().join("eula.txt"),
        &bundle.transaction.join(OLD),
    )?;
    prepared.hooks.renamed();
    prepared.hooks.checkpoint(Step::OriginalMoved)?;
    if read_state(bundle)?.physical != Physical::OriginalParked {
        return Err(Error::NeedsRecovery);
    }
    write_marker(bundle, transaction, ORIGINAL_MARK, Phase::OriginalMoved)?;
    prepared.hooks.checkpoint(Step::OriginalMarked)?;
    bundle.validate(&prepared.guard)?;
    if read_state(bundle)?.physical != Physical::OriginalParked {
        return Err(Error::NeedsRecovery);
    }
    recovery::durable_rename(
        &bundle.transaction.join(STAGE),
        &bundle.copy.root().join("eula.txt"),
    )?;
    prepared.hooks.renamed();
    prepared.hooks.checkpoint(Step::AppliedMoved)?;
    if read_state(bundle)?.physical != Physical::Applied {
        return Err(Error::NeedsRecovery);
    }
    write_marker(bundle, transaction, APPLIED_MARK, Phase::Applied)?;
    prepared.hooks.checkpoint(Step::AppliedMarked)?;
    bundle.validate(&prepared.guard)?;
    Ok(AppliedEula {
        prepared,
        transaction,
        reviewed_digest: claim.review_digest,
        may_have_spawned: false,
    })
}
#[allow(dead_code)] // File-only restore remains a private optional operation.
impl AppliedEula {
    pub(super) fn recheck_approved_launch_files(&self) -> Result<()> {
        let bundle = &self.prepared.bundle;
        self.prepared
            .approved_files
            .require_manifest_kind(&bundle.manifest)?;
        let (observed, bytes) = observe_file(&bundle.copy.root().join("server.properties"), 4096)?;
        if &observed != self.prepared.approved_files.properties()
            || bytes != resources::properties(&bundle.manifest)
        {
            return Err(Error::Changed);
        }
        if let ApprovedFiles::VoiceV2(files) = &self.prepared.approved_files {
            if initial_voice_configuration(bundle)? != files.voice_configuration {
                return Err(Error::Changed);
            }
        }
        Ok(())
    }

    pub(super) fn restore_before_spawn(self) -> Result<EulaOutcome> {
        if self.may_have_spawned {
            return Err(Error::NeedsRecovery);
        }
        let state = read_state(&self.prepared.bundle)?;
        if state.claim.transaction != self.transaction
            || state.claim.review_digest != self.reviewed_digest
            || state.may_have_spawned
        {
            return Err(Error::NeedsRecovery);
        }
        restore(
            &self.prepared.bundle,
            Authority::NeverSpawned(&self.prepared.guard),
            &self.prepared.hooks,
        )
    }
    // One-way removal of the pre-spawn authority. No caller supplies a bool,
    // report or PID to manufacture an opposite transition.
    pub(super) fn mark_may_have_spawned(&mut self) -> Result<()> {
        if self.may_have_spawned {
            return Err(Error::Consumed);
        }
        // Preserve the same reviewed physical file without retaining a
        // deny-write handle while Minecraft legitimately rewrites it at boot.
        self.recheck_approved_launch_files()?;
        self.may_have_spawned = true;
        let bundle = &self.prepared.bundle;
        bundle.validate(&self.prepared.guard)?;
        let state = read_state(bundle)?;
        if state.physical != Physical::Applied
            || state.claim.transaction != self.transaction
            || state.claim.review_digest != self.reviewed_digest
        {
            return Err(Error::NeedsRecovery);
        }
        write_marker(bundle, self.transaction, SPAWN_MARK, Phase::MayHaveSpawned)?;
        self.prepared.hooks.checkpoint(Step::MayHaveSpawned)
    }
    #[cfg(test)]
    pub(super) fn test_paths(&self) -> TestPaths {
        self.prepared.bundle.test_paths()
    }
}

enum Authority<'a> {
    NeverSpawned(&'a InstanceMutationGuard),
    // Only the private worker descendant constructs this after awaiting the
    // completion it obtained from its own supervisor83 call and all84 jobs.
    CompletedOriginalRun(&'a InstanceMutationGuard),
    Recovery(&'a VoiceMutationGuard),
}
impl Authority<'_> {
    fn check(&self, bundle: &Bundle) -> Result<()> {
        match self {
            Self::NeverSpawned(guard) | Self::CompletedOriginalRun(guard) => bundle.validate(guard),
            Self::Recovery(guard) => {
                guard.recheck()?;
                bundle.validate(guard.inner())?;
                guard.recheck()?;
                Ok(())
            }
        }
    }
}
fn restore(bundle: &Bundle, authority: Authority<'_>, hooks: &Hooks) -> Result<EulaOutcome> {
    authority.check(bundle)?;
    let mut state = read_state(bundle)?;
    if let Some(done) = state.terminal {
        authority.check(bundle)?;
        if read_state(bundle)?.digest != state.digest {
            return Err(Error::Changed);
        }
        return Ok(done.outcome);
    }
    if state.restore.is_none() {
        publish(
            &bundle.transaction.join(RESTORE_INTENT),
            &RestoreIntent {
                schema: 1,
                transaction: state.claim.transaction,
                from: state.physical,
            },
        )?;
        hooks.checkpoint(Step::RestoreIntent)?;
    }
    state = read_state(bundle)?;
    if state.physical == Physical::Applied {
        authority.check(bundle)?;
        if read_state(bundle)?.digest != state.digest {
            return Err(Error::Changed);
        }
        recovery::durable_rename(
            &bundle.copy.root().join("eula.txt"),
            &bundle.transaction.join(APPLIED),
        )?;
        hooks.renamed();
        hooks.checkpoint(Step::AppliedParked)?;
        state = read_state(bundle)?;
        if state.physical != Physical::AppliedParked {
            return Err(Error::NeedsRecovery);
        }
    }
    if matches!(
        state.physical,
        Physical::OriginalParked | Physical::AppliedParked
    ) {
        authority.check(bundle)?;
        if read_state(bundle)?.digest != state.digest {
            return Err(Error::Changed);
        }
        recovery::durable_rename(
            &bundle.transaction.join(OLD),
            &bundle.copy.root().join("eula.txt"),
        )?;
        hooks.renamed();
        hooks.checkpoint(Step::OriginalRestored)?;
        state = read_state(bundle)?;
    }
    let outcome = match state.physical {
        Physical::Restored => EulaOutcome::Restored,
        Physical::Unchanged | Physical::Prepared => EulaOutcome::Unchanged,
        _ => return Err(Error::NeedsRecovery),
    };
    authority.check(bundle)?;
    if read_state(bundle)?.digest != state.digest {
        return Err(Error::Changed);
    }
    publish(
        &bundle.transaction.join(RESTORED),
        &Restored {
            schema: 1,
            transaction: state.claim.transaction,
            outcome,
        },
    )?;
    hooks.checkpoint(Step::RestoredMarked)?;
    authority.check(bundle)?;
    let done = read_state(bundle)?.terminal.ok_or(Error::NeedsRecovery)?;
    Ok(done.outcome)
}

pub(crate) fn plan_eula_recovery(id: BundleId) -> Result<EulaRecoveryPlan> {
    #[cfg(test)]
    {
        prepare_recovery(&lab_root()?, id, None, Hooks::default())
    }
    #[cfg(not(test))]
    {
        prepare_recovery(&lab_root()?, id, Hooks)
    }
}
pub(super) fn prepare_recovery(
    lab: &Path,
    id: BundleId,
    #[cfg(test)] probe: Option<TestProbe>,
    hooks: Hooks,
) -> Result<EulaRecoveryPlan> {
    let bundle = Bundle::reopen(lab, id)?;
    #[cfg(test)]
    {
        prepare_recovery_bundle(bundle, probe, hooks)
    }
    #[cfg(not(test))]
    {
        prepare_recovery_bundle(bundle, hooks)
    }
}
#[cfg(test)]
pub(super) fn prepare_recovery_fixture_voice(
    lab: &Path,
    id: BundleId,
    pins: Vec<resources::Pin>,
    probe: TestProbe,
    hooks: Hooks,
) -> Result<EulaRecoveryPlan> {
    // Fixed test pins are selected by the explicit Rust fixture caller, never
    // inferred from a persisted profile/path. Guard and recovery are unchanged.
    let bundle = Bundle::reopen_fixture(lab, id, pins)?;
    prepare_recovery_bundle(bundle, Some(probe), hooks)
}
fn recovery_digest_input(
    manifest: &Manifest,
    review_id: Uuid,
    state_digest: &str,
) -> Result<Vec<u8>> {
    match manifest {
        Manifest::StatusV1(_) => encode(&(
            1,
            "native_status_eula_recovery",
            review_id,
            manifest.id(),
            state_digest,
        )),
        Manifest::VoiceV2(voice) => encode(&(
            2,
            "native_voice_eula_recovery_v2",
            review_id,
            voice.profile,
            manifest.id(),
            state_digest,
        )),
    }
}
fn prepare_recovery_bundle(
    bundle: Bundle,
    #[cfg(test)] probe: Option<TestProbe>,
    hooks: Hooks,
) -> Result<EulaRecoveryPlan> {
    #[cfg(test)]
    let guard = match probe {
        Some(probe) => super::super::process::acquire_with_probe(&bundle.copy, probe)?,
        None => super::super::acquire_voice_mutation_guard(&bundle.copy)?,
    };
    #[cfg(not(test))]
    let guard = super::super::acquire_voice_mutation_guard(&bundle.copy)?;
    Authority::Recovery(&guard).check(&bundle)?;
    let state = read_state(&bundle)?;
    let review_id = Uuid::new_v4();
    let review = Review {
        review_id,
        digest: digest(&recovery_digest_input(
            &bundle.manifest,
            review_id,
            &state.digest,
        )?),
        purpose: match &bundle.manifest {
            Manifest::StatusV1(_) => "native_status_eula_recovery",
            Manifest::VoiceV2(_) => "native_voice_eula_recovery_v2",
        },
        bundle_id: bundle.manifest.id(),
        before: format!("{:?}", state.physical),
        after: String::from_utf8_lossy(EULA_FALSE).into(),
        details: vec![
            bundle.copy.root().display().to_string(),
            "Restaurar somente o original reconhecido; não iniciar processo nem liberar uso único"
                .into(),
        ],
    };
    Ok(EulaRecoveryPlan {
        bundle,
        guard,
        review,
        state_digest: state.digest,
        deadline: Instant::now() + REVIEW_TTL,
        hooks,
    })
}
#[allow(dead_code)] // Explicit guarded recovery API, with no automatic caller.
impl EulaRecoveryPlan {
    pub(crate) fn review(&self) -> &Review {
        &self.review
    }
    pub(crate) fn confirm(self, expected: &str) -> Result<EulaOutcome> {
        approve(&self.review, self.deadline, expected)?;
        Authority::Recovery(&self.guard).check(&self.bundle)?;
        if read_state(&self.bundle)?.digest != self.state_digest {
            return Err(Error::Changed);
        }
        approve(&self.review, self.deadline, expected)?;
        restore(&self.bundle, Authority::Recovery(&self.guard), &self.hooks)
    }
    #[cfg(test)]
    pub(super) fn expire_for_test(&mut self) {
        self.deadline = Instant::now() - Duration::from_secs(1);
    }
    #[cfg(test)]
    pub(super) fn test_paths(&self) -> TestPaths {
        self.bundle.test_paths()
    }
}

#[cfg(test)]
pub(super) fn roundtrip_claim_for_test(bytes: &[u8]) -> Result<Vec<u8>> {
    let claim: Claim = serde_json::from_slice(bytes).map_err(|_| Error::Identity)?;
    encode(&claim)
}

#[cfg(test)]
pub(super) fn roundtrip_approved_for_test(bytes: &[u8]) -> Result<Vec<u8>> {
    // This is the original V1 golden replay, with its original exact derive.
    let observations: ApprovedFilesV1 =
        serde_json::from_slice(bytes).map_err(|_| Error::Identity)?;
    encode(&ApprovedFiles::StatusV1(observations))
}

#[cfg(test)]
pub(super) fn eula_input_for_test(prepared: &PreparedStatus) -> Result<Vec<u8>> {
    eula_digest_input(
        &prepared.bundle.manifest,
        prepared.review().review_id,
        &prepared.approved_files,
    )
}

#[cfg(test)]
pub(super) fn recovery_input_for_test(plan: &EulaRecoveryPlan) -> Result<Vec<u8>> {
    recovery_digest_input(
        &plan.bundle.manifest,
        plan.review.review_id,
        &plan.state_digest,
    )
}

#[cfg(test)]
pub(super) fn replay_eula_input_for_test(
    manifest_bytes: &[u8],
    approved_bytes: &[u8],
    original_input: &[u8],
) -> Result<Vec<u8>> {
    use serde::de::IgnoredAny;
    type OriginalContext = (u32, String, Uuid, IgnoredAny, IgnoredAny, String);
    let (schema, purpose, review_id, _, _, _): OriginalContext =
        serde_json::from_slice(original_input).map_err(|_| Error::Identity)?;
    if schema != 1 || purpose != "native_status_eula" {
        return Err(Error::Identity);
    }
    let manifest = super::manifest::decode_manifest(manifest_bytes)?;
    manifest.status_v1()?;
    let observations: ApprovedFilesV1 =
        serde_json::from_slice(approved_bytes).map_err(|_| Error::Identity)?;
    // Same original UUID/observations, current production encoder. IgnoredAny
    // only extracts tuple context; it never normalizes either typed payload.
    eula_digest_input(&manifest, review_id, &ApprovedFiles::StatusV1(observations))
}
