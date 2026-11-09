use super::eula::AppliedEula;
#[cfg(test)]
use super::manifest::decode_manifest;
pub(super) use super::manifest::Manifest;
use super::manifest::{
    read_manifest, InitialVoiceConfigurationV2, ManifestKind, ManifestV1, ObservationPolicyV2,
    StoredVoiceProfileV2, VoiceManifestV2, VoiceRuntimeV2,
};
use super::voice_profile::{self, NativeVoiceProfile};
use super::*;
use crate::engine::{
    acquire_instance_mutation_lock, acquire_instance_recovery_lock, ContentUpdateError,
};

#[path = "factory_voice_context.rs"]
mod voice_context;
pub(super) use voice_context::ImmutableVoiceContext;

fn lock(instance: &Instance) -> Result<InstanceMutationGuard> {
    acquire_instance_mutation_lock(instance).map_err(|e| {
        if matches!(e, ContentUpdateError::Busy) {
            Error::Busy
        } else {
            Error::NeedsRecovery
        }
    })
}

// Only an already validated, pinned private directory may acquire the extended
// Windows representation needed by Recovery's native durable renames. Callers
// still supply ordinary drive paths; neither UNC nor device inputs are accepted.
fn internal_directory(path: &Path, pins: &mut Vec<File>) -> Result<PathBuf> {
    explicit_path(path)?;
    let original = recovery::pin_directory(path)?;
    let identity = file_identity(original.first().ok_or(Error::Identity)?)?;
    let canonical = path.canonicalize()?;
    if canonical.as_os_str().len() > 1024 {
        return Err(Error::Limit);
    }
    if !matches!(
        canonical.components().next(),
        Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), std::path::Prefix::VerbatimDisk(_))
    ) {
        return Err(Error::UnsafePath);
    }
    let canonical_pins = recovery::pin_directory(&canonical)?;
    if file_identity(canonical_pins.first().ok_or(Error::Identity)?)? != identity {
        return Err(Error::Changed);
    }
    pins.extend(original);
    pins.extend(canonical_pins);
    Ok(canonical)
}

pub(crate) struct FactoryPlan {
    lab: PathBuf,
    id: BundleId,
    resources: resources::Resources,
    review: Review,
    deadline: Instant,
    hooks: Hooks,
    kind: ManifestKind,
    voice_policy: ObservationPolicyV2,
    #[cfg(test)]
    test_expected_pins: Option<Vec<resources::Pin>>,
    _pins: Vec<File>,
}
pub(crate) struct PreparedStatus {
    pub(super) bundle: Bundle,
    pub(super) guard: InstanceMutationGuard,
    pub(super) resources: resources::Resources,
    review: Review,
    deadline: Instant,
    pub(super) hooks: Hooks,
    pub(super) approved_files: eula::ApprovedFiles,
    pub(crate) announcement_recovery:
        Option<(Instance, super::super::DurableRestorationController)>,
}

pub(crate) fn plan_status_factory(stage: &Path, jdk: &Path, accepted: bool) -> Result<FactoryPlan> {
    #[cfg(test)]
    let hooks = Hooks::default();
    #[cfg(not(test))]
    let hooks = Hooks;
    plan(&lab_root()?, stage, jdk, accepted, resources::pins(), hooks)
}
pub(super) fn plan(
    lab: &Path,
    stage: &Path,
    jdk: &Path,
    accepted: bool,
    pins: Vec<resources::Pin>,
    hooks: Hooks,
) -> Result<FactoryPlan> {
    if !accepted {
        return Err(Error::Consent);
    }
    explicit_path(lab)?;
    let directories = recovery::pin_directory(lab)?;
    let resources = resources::Resources::inspect(lab, stage, jdk, pins)?;
    let id = BundleId(Uuid::new_v4());
    let destination = lab.join(format!("native-status-{}", id.0));
    absent(&destination)?;
    let review_id = Uuid::new_v4();
    let data = factory_digest_input(
        review_id,
        id,
        directory_identity(lab)?,
        destination.to_str().ok_or(Error::UnsafePath)?,
        &resources.records,
    )?;
    Ok(FactoryPlan {
        lab: lab.into(),
        id,
        review: Review {
            review_id,
            digest: digest(&data),
            purpose: "native_status_factory",
            bundle_id: id,
            before: "Pasta nova ausente".into(),
            after: "Origem própria e cópia diagnóstica STATUS; EULA false; nenhum processo".into(),
            details: vec![
                destination.display().to_string(),
                stage.display().to_string(),
                jdk.display().to_string(),
                "Minecraft 1.21.1; Fabric 0.16.14; Java 21; protocolo 767; sem mods".into(),
            ],
        },
        resources,
        deadline: Instant::now() + REVIEW_TTL,
        hooks,
        kind: ManifestKind::StatusV1,
        voice_policy: ObservationPolicyV2::LiteralOnlyFirstPersisted,
        #[cfg(test)]
        test_expected_pins: None,
        _pins: directories,
    })
}

pub(crate) fn plan_voice_factory(
    stage: &Path,
    jdk: &Path,
    profile: NativeVoiceProfile,
    accepted: bool,
) -> Result<FactoryPlan> {
    #[cfg(test)]
    let hooks = Hooks::default();
    #[cfg(not(test))]
    let hooks = Hooks;
    voice_plan(
        &lab_root()?,
        stage,
        jdk,
        profile,
        accepted,
        resources::voice_pins(profile),
        hooks,
    )
}
fn voice_plan(
    lab: &Path,
    stage: &Path,
    jdk: &Path,
    profile: NativeVoiceProfile,
    accepted: bool,
    pins: Vec<resources::Pin>,
    hooks: Hooks,
) -> Result<FactoryPlan> {
    let mut plan = plan(lab, stage, jdk, accepted, pins, hooks)?;
    let stored = profile.stored();
    let destination = plan.lab.join(format!("native-status-{}", plan.id.0));
    plan.kind = ManifestKind::VoiceV2(stored);
    plan.review.digest = digest(&encode(&(
        2,
        "native_voice_factory_v2",
        plan.review.review_id,
        plan.id,
        directory_identity(lab)?,
        destination.to_str().ok_or(Error::UnsafePath)?,
        stored,
        voice_runtime(),
        &plan.resources.records,
        stored.configuration_name(),
        stored.configuration().len() as u64,
        stored.configuration_sha256(),
        ObservationPolicyV2::LiteralOnlyFirstPersisted,
        digest(resources::PROPERTIES),
        digest(EULA_FALSE),
    ))?);
    plan.review.purpose = "native_voice_factory_v2";
    plan.review.after =
        format!("Origem própria e cópia diagnóstica {profile:?}; EULA false; nenhum processo");
    plan.review.details.extend([
        format!("Perfil {profile:?}; Minecraft1.21.1; Fabric0.16.14; Java21; protocolo767"),
        format!(
            "Baseline novo: {}; {}bytes; SHA256{}; política1 LiteralOnlyFirstPersisted",
            stored.configuration_name(),
            stored.configuration().len(),
            stored.configuration_sha256()
        ),
    ]);
    // The STATUS review text is replaced rather than presented beside the
    // selected voice profile. No caller can change this private kind later.
    plan.review.details.remove(3);
    plan.review
        .details
        .extend(plan.resources.records.iter().map(|record| {
            format!(
                "Recurso {}; {}bytes; SHA256{}",
                record.name, record.observation.bytes, record.observation.sha256
            )
        }));
    Ok(plan)
}
/// The combined factory review authorizes only a new diagnostic copy and the
/// exact compiled announcement. The independent execution review remains required.
#[cfg(feature = "connect-voice-lab")]
pub(super) fn plan_voice_bridge_factory_at(
    lab: &Path,
    stage: &Path,
    jdk: &Path,
    profile: NativeVoiceProfile,
) -> Result<FactoryPlan> {
    #[cfg(test)]
    let hooks = Hooks::default();
    #[cfg(not(test))]
    let hooks = Hooks;
    let plan = voice_plan(
        lab,
        stage,
        jdk,
        profile,
        true,
        resources::voice_pins(profile),
        hooks,
    )?;
    bridge_plan(plan, profile)
}

fn bridge_plan(mut plan: FactoryPlan, profile: NativeVoiceProfile) -> Result<FactoryPlan> {
    plan.voice_policy = ObservationPolicyV2::LiteralLoopback35554Durable;
    let stored = profile.stored();
    plan.review.digest = digest(&encode(&(
        "native_voice_bridge_factory_v2",
        &plan.review.digest,
        plan.voice_policy,
        stored.configuration_sha256(),
        plan.voice_policy.configuration_sha256(stored),
        "durable_recovery_receipt",
        "127.0.0.1:35554",
        digest(resources::BRIDGE_PROPERTIES),
    ))?);
    plan.review.purpose = "native_voice_bridge_factory_v2";
    plan.review.before =
        String::from_utf8(stored.configuration().to_vec()).map_err(|_| Error::Identity)?;
    plan.review.after = String::from_utf8(plan.voice_policy.configuration(stored).to_vec())
        .map_err(|_| Error::Identity)?;
    plan.review.details.push(format!("Política2 LiteralLoopback35554Durable; baseline aplicado {}bytes/SHA256{}; política1 permanece somente a preimagem original antes da transação.", plan.voice_policy.configuration(stored).len(), plan.voice_policy.configuration_sha256(stored)));
    plan.review.details.push("Jogo somente loopback43001; online-mode=false (login Minecraft offline), whitelist=false, max-players=8; autenticação de transporte Aurora obrigatória. Mundo novo descartável.".into());
    plan.review.details.push("Criar cópia nova e aplicar somente anúncio 127.0.0.1:35554 com recibo durável Recovery; nenhum Java. Restaurar depois exige revisão própria. Todos os jogadores, inclusive o Host jogador, precisam de ponte local aprovada.".into());
    Ok(plan)
}

#[cfg(test)]
pub(super) fn fixture_voice_plan(
    lab: &Path,
    stage: &Path,
    jdk: &Path,
    profile: NativeVoiceProfile,
    accepted: bool,
    hooks: Hooks,
) -> Result<FactoryPlan> {
    let expected = resources::fixture_voice_pins(profile);
    let mut plan = voice_plan(lab, stage, jdk, profile, accepted, expected.clone(), hooks)?;
    plan.test_expected_pins = Some(expected);
    Ok(plan)
}
#[cfg(all(test, debug_assertions))]
pub(super) fn fixture_voice_bridge_plan(
    lab: &Path,
    stage: &Path,
    jdk: &Path,
    profile: NativeVoiceProfile,
) -> Result<FactoryPlan> {
    bridge_plan(
        fixture_voice_plan(lab, stage, jdk, profile, true, Hooks::default())?,
        profile,
    )
}
fn voice_runtime() -> VoiceRuntimeV2 {
    VoiceRuntimeV2 {
        minecraft: voice_profile::MINECRAFT.into(),
        loader: voice_profile::LOADER.into(),
        loader_version: voice_profile::LOADER_VERSION.into(),
        protocol: voice_profile::PROTOCOL,
    }
}
fn create_voice_configuration(
    root: &Path,
    profile: StoredVoiceProfileV2,
) -> Result<InitialVoiceConfigurationV2> {
    let directories: &[&str] = match profile {
        StoredVoiceProfileV2::Svc => &["config", "config/voicechat"],
        StoredVoiceProfileV2::Plasmo => {
            &["config", "config/plasmovoice", "config/plasmovoice/server"]
        }
    };
    let mut parents = Vec::new();
    for name in directories {
        let path = root.join(name);
        fs::create_dir(&path)?;
        parents.extend(recovery::pin_directory(&path)?);
    }
    let path = root.join(profile.configuration_name());
    recovery::write_new(&path, profile.configuration())?;
    let (observation, bytes) = observe_file(&path, profile.configuration().len() as u64)?;
    if bytes != profile.configuration() || observation.sha256 != profile.configuration_sha256() {
        return Err(Error::Changed);
    }
    // These short-lived handles protect creation only. The mutable file and
    // its parents are never retained by Bundle::pin_files during execution.
    drop(parents);
    Ok(InitialVoiceConfigurationV2 {
        name: profile.configuration_name().into(),
        observation,
    })
}
// Production and compatibility replay use exactly this preimage/encoder.
// Callers obtain the observations and original review UUID before entering it.
fn factory_digest_input(
    review_id: Uuid,
    id: BundleId,
    lab_identity: FileIdentity,
    destination: &str,
    records: &[resources::ResourceRecord],
) -> Result<Vec<u8>> {
    encode(&(
        1,
        "native_status_factory",
        review_id,
        id,
        lab_identity,
        destination,
        records,
        digest(resources::PROPERTIES),
        digest(EULA_FALSE),
    ))
}
#[allow(dead_code)] // Functional consumed-plan API, reserved for the native caller.
impl FactoryPlan {
    #[cfg(test)]
    pub(super) fn v1_digest_input_for_test(&self) -> Result<Vec<u8>> {
        let destination = self.lab.join(format!("native-status-{}", self.id.0));
        factory_digest_input(
            self.review.review_id,
            self.id,
            directory_identity(&self.lab)?,
            destination.to_str().ok_or(Error::UnsafePath)?,
            &self.resources.records,
        )
    }
    pub(crate) fn review(&self) -> &Review {
        &self.review
    }
    pub(crate) fn create(mut self, expected: &str) -> Result<PreparedStatus> {
        approve(&self.review, self.deadline, expected)?;
        self.resources.recheck()?;
        approve(&self.review, self.deadline, expected)?;
        let bundle_path = self.lab.join(format!("native-status-{}", self.id.0));
        absent(&bundle_path)?;
        fs::create_dir(&bundle_path)?;
        let mut pins = recovery::pin_directory(&bundle_path)?;
        let native = bundle_path.join("native");
        fs::create_dir(&native)?;
        let native = internal_directory(&native, &mut pins)?;
        let layout = InstanceLayout::new(&native);
        let origin = layout.open(InstanceId::parse("status-origin").map_err(|_| Error::Identity)?);
        origin.ensure_layout().map_err(|_| Error::Io)?;
        recovery::write_new(&origin.root().join("aurora-instance.json"), RUNTIME)?;
        let source_guard = lock(&origin)?;
        let source_observation = recovery::capture_observation(&origin, &source_guard)?;
        let environment = diagnostic_lab::create(&origin, &source_guard, None, |_| {})?;
        let copy = layout.open(InstanceId::parse(environment.id).map_err(|_| Error::Identity)?);
        let mut guard = lock(&copy)?;
        copy.ensure_layout().map_err(|_| Error::Io)?;
        self.resources.copy_artifacts(copy.root())?;
        recovery::write_new(
            &copy.root().join("server.properties"),
            if self.voice_policy == ObservationPolicyV2::LiteralLoopback35554Durable {
                resources::BRIDGE_PROPERTIES
            } else {
                resources::PROPERTIES
            },
        )?;
        recovery::write_new(&copy.root().join("eula.txt"), EULA_FALSE)?;
        let mut installed = Vec::new();
        for record in &self.resources.records {
            if record.name.starts_with("jdk/") {
                continue;
            }
            let observation =
                observe_file(&copy.root().join(&record.name), record.observation.bytes)?.0;
            if observation.bytes != record.observation.bytes
                || observation.sha256 != record.observation.sha256
            {
                return Err(Error::Changed);
            }
            installed.push(resources::ResourceRecord {
                name: record.name.clone(),
                observation,
            });
        }
        let mut provenance = Vec::new();
        for name in provenance_names(copy.id().as_str())? {
            provenance.push(resources::ResourceRecord {
                observation: observe_file(&native.join(&name), 1024 * 1024)?.0,
                name,
            });
        }
        let common = ManifestV1 {
            schema: 1,
            id: self.id,
            lab_identity: directory_identity(&self.lab)?,
            bundle_identity: directory_identity(&bundle_path)?,
            native_identity: directory_identity(&native)?,
            copy_id: copy.id().as_str().into(),
            copy_uuid: environment.instance_uuid,
            origin_uuid: source_observation.instance_uuid,
            copy_identity: recovery::filesystem_identity(&copy)?,
            origin_identity: recovery::filesystem_identity(&origin)?,
            origin_fingerprint: source_observation.fingerprint,
            resources: self.resources.records.clone(),
            installed,
            provenance,
        };
        let mut announcement_recovery = None;
        let manifest = match self.kind {
            ManifestKind::StatusV1 => Manifest::StatusV1(common),
            ManifestKind::VoiceV2(profile) => {
                // Only fresh factory data is moved here, before any manifest
                // publication. No opened V1 bundle or Prepared is converted.
                let mut initial_voice_configuration =
                    create_voice_configuration(copy.root(), profile)?;
                if self.voice_policy == ObservationPolicyV2::LiteralLoopback35554Durable {
                    // The opaque process guard is acquired normally, after releasing
                    // the factory lock. No borrowed ordinary guard bypasses its probe.
                    drop(guard);
                    let voice_guard = super::super::acquire_voice_mutation_guard(&copy)?;
                    let prepare_announcement = super::super::prepare_durable_announcement;
                    #[cfg(test)]
                    let prepare_announcement = if self.test_expected_pins.is_some() {
                        super::super::prepare_fixture_durable_announcement
                    } else {
                        prepare_announcement
                    };
                    let prepared = prepare_announcement(
                        &copy,
                        &voice_guard,
                        super::super::AnnouncementRequest {
                            voice_mod: match profile {
                                StoredVoiceProfileV2::Svc => {
                                    super::super::VoiceMod::SimpleVoiceChat
                                }
                                StoredVoiceProfileV2::Plasmo => super::super::VoiceMod::PlasmoVoice,
                            },
                            mode: super::super::AnnouncementMode::DedicatedAuroraOnly,
                            guest_port: 35554,
                            expected_source_sha256: profile.configuration_sha256().into(),
                        },
                    )?;
                    // Factory approval already bound both complete compiled files.
                    // Refuse any adapter output that differs from that approved diff.
                    if prepared.review().replacement_sha256
                        != self.voice_policy.configuration_sha256(profile)
                    {
                        return Err(Error::Changed);
                    }
                    let approval = prepared.review().digest.clone();
                    let applied = prepared.confirm(&copy, &voice_guard, &approval)?;
                    announcement_recovery = Some((copy.clone(), applied.receipt));
                    drop(voice_guard);
                    guard = lock(&copy)?;
                    let (observation, bytes) =
                        observe_file(&copy.root().join(profile.configuration_name()), 256 * 1024)?;
                    if bytes != self.voice_policy.configuration(profile) {
                        return Err(Error::Changed);
                    }
                    initial_voice_configuration.observation = observation;
                }
                Manifest::VoiceV2(VoiceManifestV2 {
                    schema: 2,
                    profile,
                    id: common.id,
                    lab_identity: common.lab_identity,
                    bundle_identity: common.bundle_identity,
                    native_identity: common.native_identity,
                    copy_id: common.copy_id,
                    copy_uuid: common.copy_uuid,
                    origin_uuid: common.origin_uuid,
                    copy_identity: common.copy_identity,
                    origin_identity: common.origin_identity,
                    origin_fingerprint: common.origin_fingerprint,
                    resources: common.resources,
                    installed: common.installed,
                    provenance: common.provenance,
                    runtime: voice_runtime(),
                    initial_voice_configuration,
                    observation_policy: self.voice_policy,
                })
            }
        };
        publish(&bundle_path.join("bundle.json"), &manifest)?;
        let tx = bundle_path.join("eula-transaction");
        fs::create_dir(&tx)?;
        let tx = internal_directory(&tx, &mut pins)?;
        drop(source_guard);
        let bundle = Bundle {
            lab: self.lab.clone(),
            path: bundle_path,
            native,
            origin,
            copy,
            transaction: tx,
            manifest,
            #[cfg(test)]
            test_expected_pins: self.test_expected_pins,
            _pins: pins,
            _files: Vec::new(),
        };
        let mut bundle = bundle;
        bundle.validate(&guard)?;
        bundle.pin_files()?;
        let (review, approved_files) = eula::execution_review(&bundle, &guard)?;
        Ok(PreparedStatus {
            bundle,
            guard,
            resources: self.resources,
            review,
            deadline: Instant::now() + REVIEW_TTL,
            hooks: self.hooks,
            approved_files,
            announcement_recovery,
        })
    }
    #[cfg(test)]
    pub(super) fn expire_for_test(&mut self) {
        self.deadline = Instant::now() - Duration::from_secs(1);
    }
}
#[allow(dead_code)] // Only factory-created, lock-owning preparations can apply EULA.
impl PreparedStatus {
    pub(crate) fn review(&self) -> &Review {
        &self.review
    }
    pub(crate) fn bundle_id(&self) -> BundleId {
        self.bundle.manifest.id()
    }
    pub(super) fn apply_eula(self, expected: &str) -> Result<AppliedEula> {
        self.approved_files
            .require_manifest_kind(&self.bundle.manifest)?;
        approve(&self.review, self.deadline, expected)?;
        self.resources.recheck()?;
        self.bundle.validate(&self.guard)?;
        if eula::execution_digest(&self.bundle, self.review.review_id)? != self.review.digest {
            return Err(Error::Changed);
        }
        eula::apply(self, expected)
    }
    pub(super) fn review_digest(&self) -> &str {
        &self.review.digest
    }
    pub(super) fn confirm_fresh(&self, expected: &str) -> Result<()> {
        approve(&self.review, self.deadline, expected)
    }
    pub(super) fn approval_deadline(&self) -> Instant {
        self.deadline
    }
    #[cfg(test)]
    pub(super) fn shorten_approval_for_test(&mut self, remaining: Duration) {
        self.deadline = self.deadline.min(Instant::now() + remaining);
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

fn provenance_names(copy_id: &str) -> Result<Vec<String>> {
    let uuid = copy_id
        .strip_prefix("aurora-test-")
        .ok_or(Error::Identity)?;
    Uuid::parse_str(uuid).map_err(|_| Error::Identity)?;
    Ok(vec![
        format!("instances/{copy_id}/aurora-instance.json"),
        "instances/status-origin/aurora-instance.json".into(),
        format!("instances/{copy_id}/.aurora/recovery-identity.json"),
        "instances/status-origin/.aurora/recovery-identity.json".into(),
        format!(".aurora/diagnostic-lab/{uuid}/record.json"),
        format!(".aurora/diagnostic-lab/{uuid}/ready.json"),
    ])
}
pub(super) struct Bundle {
    pub lab: PathBuf,
    pub path: PathBuf,
    pub native: PathBuf,
    pub origin: Instance,
    pub copy: Instance,
    pub transaction: PathBuf,
    pub manifest: Manifest,
    #[cfg(test)]
    test_expected_pins: Option<Vec<resources::Pin>>,
    _pins: Vec<File>,
    _files: Vec<recovery::SecureFile>,
}
impl Bundle {
    pub fn reopen(lab: &Path, id: BundleId) -> Result<Self> {
        #[cfg(test)]
        {
            Self::reopen_inner(lab, id, None)
        }
        #[cfg(not(test))]
        {
            Self::reopen_inner(lab, id)
        }
    }
    #[cfg(test)]
    pub(super) fn reopen_fixture(
        lab: &Path,
        id: BundleId,
        pins: Vec<resources::Pin>,
    ) -> Result<Self> {
        Self::reopen_inner(lab, id, Some(pins))
    }
    fn reopen_inner(
        lab: &Path,
        id: BundleId,
        #[cfg(test)] test_expected_pins: Option<Vec<resources::Pin>>,
    ) -> Result<Self> {
        explicit_path(lab)?;
        let path = lab.join(format!("native-status-{}", id.0));
        let mut pins = recovery::pin_directory(&path)?;
        let native = internal_directory(&path.join("native"), &mut pins)?;
        let manifest = read_manifest(&path.join("bundle.json"))?;
        let status = manifest.kind() == ManifestKind::StatusV1;
        if (status && manifest.schema() != 1)
            || manifest.id() != id
            || manifest.lab_identity() != &directory_identity(lab)?
            || manifest.bundle_identity() != &directory_identity(&path)?
            || manifest.native_identity() != &directory_identity(&native)?
            || manifest.copy_uuid() == manifest.origin_uuid()
            || Uuid::parse_str(manifest.copy_uuid()).is_err()
            || Uuid::parse_str(manifest.origin_uuid()).is_err()
            || (status && manifest.installed().len() != 9)
            || (status && manifest.resources().len() != 11)
            || manifest.provenance().len() != 6
        {
            return Err(Error::Identity);
        }
        if let ManifestKind::VoiceV2(profile) = manifest.kind() {
            Self::voice_shape(&manifest, profile)?;
        }
        let layout = InstanceLayout::new(&native);
        let copy = layout.open(InstanceId::parse(manifest.copy_id()).map_err(|_| Error::Identity)?);
        if !diagnostic_lab::is_lab_candidate(&copy) {
            return Err(Error::Identity);
        }
        let origin = layout.open(InstanceId::parse("status-origin").map_err(|_| Error::Identity)?);
        let transaction = internal_directory(&path.join("eula-transaction"), &mut pins)?;
        let mut result = Self {
            lab: lab.into(),
            path,
            native,
            origin,
            copy,
            transaction,
            manifest,
            #[cfg(test)]
            test_expected_pins,
            _pins: pins,
            _files: Vec::new(),
        };
        result.pin_files()?;
        if let ManifestKind::VoiceV2(profile) = result.manifest.kind() {
            result.validate_voice_immutable(profile)?;
        }
        Ok(result)
    }
    fn voice_shape(manifest: &Manifest, profile: StoredVoiceProfileV2) -> Result<()> {
        let voice = manifest.voice_v2()?;
        let initial = &voice.initial_voice_configuration;
        if voice.schema != 2
            || voice.profile != profile
            || voice.provenance.len() != 6
            || voice.runtime != voice_runtime()
            || initial.name != profile.configuration_name()
            || initial.observation.bytes
                != voice.observation_policy.configuration(profile).len() as u64
            || initial.observation.sha256 != voice.observation_policy.configuration_sha256(profile)
        {
            return Err(Error::Identity);
        }
        resources::voice_record_shape(profile, manifest.resources(), manifest.installed())?;
        let expected = provenance_names(manifest.copy_id())?;
        let mut names = std::collections::BTreeSet::new();
        for record in manifest.provenance() {
            if !expected.contains(&record.name) || !names.insert(record.name.as_str()) {
                return Err(Error::Identity);
            }
        }
        // The codec validated the initial record's fixed name/size/hash.
        // Isolated reopen does not read its mutable file; cleanup's existing
        // readonly Recovery inventory may still read the current contents.
        Ok(())
    }
    fn validate_voice_immutable(&self, profile: StoredVoiceProfileV2) -> Result<()> {
        Self::voice_shape(&self.manifest, profile)?;
        let compiled = resources::compiled_voice_pins(profile);
        #[cfg(test)]
        let expected = self.test_expected_pins.as_deref().unwrap_or(&compiled);
        #[cfg(not(test))]
        let expected = compiled.as_slice();
        resources::validate_voice_artifacts(
            self.copy.root(),
            profile,
            self.manifest.resources(),
            self.manifest.installed(),
            expected,
        )?;
        for record in self.manifest.provenance() {
            if record.observation.bytes > 1024 * 1024
                || observe_file(&self.native.join(&record.name), 1024 * 1024)?.0
                    != record.observation
            {
                return Err(Error::Changed);
            }
        }
        Ok(())
    }
    fn pin_files(&mut self) -> Result<()> {
        let voice = match self.manifest.kind() {
            ManifestKind::StatusV1 => None,
            ManifestKind::VoiceV2(profile) => {
                Self::voice_shape(&self.manifest, profile)?;
                resources::voice_inventory(self.copy.root(), profile)?;
                Some(profile)
            }
        };
        self._files
            .push(recovery::secure_open(&self.path.join("bundle.json"))?);
        for record in self.manifest.installed() {
            if voice.is_none() && !resources::artifact_name(&record.name) {
                return Err(Error::Identity);
            }
            self._files
                .push(recovery::secure_open(&self.copy.root().join(&record.name))?);
        }
        let expected = provenance_names(self.copy.id().as_str())?;
        let mut names = std::collections::BTreeSet::new();
        for record in self.manifest.provenance() {
            if !expected.contains(&record.name) || !names.insert(&record.name) {
                return Err(Error::Identity);
            }
            self._files
                .push(recovery::secure_open(&self.native.join(&record.name))?);
        }
        Ok(())
    }
    pub fn validate(&self, guard: &InstanceMutationGuard) -> Result<()> {
        let voice = match self.manifest.kind() {
            ManifestKind::StatusV1 => None,
            ManifestKind::VoiceV2(profile) => {
                Self::voice_shape(&self.manifest, profile)?;
                Some(profile)
            }
        };
        if self.path
            != self
                .lab
                .join(format!("native-status-{}", self.manifest.id().0))
            || !diagnostic_lab::is_lab_candidate(&self.copy)
            || &directory_identity(&self.lab)? != self.manifest.lab_identity()
            || &directory_identity(&self.path)? != self.manifest.bundle_identity()
            || &directory_identity(&self.native)? != self.manifest.native_identity()
            || (voice.is_none() && self.manifest.installed().len() != 9)
            || (voice.is_none() && self.manifest.resources().len() != 11)
            || self.manifest.provenance().len() != 6
        {
            return Err(Error::Identity);
        }
        let actual = read_manifest(&self.path.join("bundle.json"))?;
        if actual.kind() != self.manifest.kind() {
            return Err(Error::Identity);
        }
        if actual != self.manifest {
            return Err(Error::Changed);
        }
        // The ordinary mutation helper opens Recovery for writing. Inspection
        // takes only the shared OS lock; both strict observations below check
        // pending journals without initializing or changing the existing DB.
        let source_guard = acquire_instance_recovery_lock(&self.origin).map_err(|error| {
            if matches!(error, ContentUpdateError::Busy) {
                Error::Busy
            } else {
                Error::NeedsRecovery
            }
        })?;
        recovery::with_existing_observation(&self.copy, guard, |observation| {
            diagnostic_lab::ensure_launchable(&self.copy)?;
            let origin = diagnostic_lab::origin_instance(&self.native, &self.copy)?;
            if observation.instance_uuid != self.manifest.copy_uuid()
                || origin.id() != self.origin.id()
                || recovery::filesystem_identity(&self.copy)? != self.manifest.copy_identity()
                || recovery::filesystem_identity(&origin)? != self.manifest.origin_identity()
                || recovery::identity_lookup(&origin)?.as_deref()
                    != Some(self.manifest.origin_uuid())
            {
                return Err(recovery::RecoveryError::Identity);
            }
            recovery::with_existing_observation(&origin, &source_guard, |source| {
                if source.fingerprint != self.manifest.origin_fingerprint() {
                    return Err(recovery::RecoveryError::OriginChanged);
                }
                Ok(())
            })?;
            Ok(())
        })?;
        let expected: serde_json::Value =
            serde_json::from_slice(RUNTIME).map_err(|_| Error::Identity)?;
        for instance in [&self.origin, &self.copy] {
            let current: serde_json::Value = serde_json::from_slice(&recovery::read_bounded(
                &instance.root().join("aurora-instance.json"),
                1024,
            )?)
            .map_err(|_| Error::Identity)?;
            if current != expected {
                return Err(Error::Changed);
            }
        }
        if let Some(profile) = voice {
            self.validate_voice_immutable(profile)?;
        } else {
            if fs::read_dir(self.copy.root().join("mods"))?
                .next()
                .is_some()
            {
                return Err(Error::Changed);
            }
            let mut names = std::collections::BTreeSet::new();
            for record in self.manifest.installed() {
                if !resources::artifact_name(&record.name)
                    || !names.insert(record.name.clone())
                    || record.observation.bytes > 64 * 1024 * 1024
                    || observe_file(
                        &self.copy.root().join(&record.name),
                        record.observation.bytes,
                    )?
                    .0 != record.observation
                {
                    return Err(Error::Changed);
                }
            }
        }
        let expected = provenance_names(self.copy.id().as_str())?;
        let mut names = std::collections::BTreeSet::new();
        for record in self.manifest.provenance() {
            if !expected.contains(&record.name)
                || !names.insert(&record.name)
                || record.observation.bytes > 1024 * 1024
                || observe_file(&self.native.join(&record.name), 1024 * 1024)?.0
                    != record.observation
            {
                return Err(Error::Changed);
            }
        }
        Ok(())
    }
    #[cfg(test)]
    pub fn test_paths(&self) -> TestPaths {
        TestPaths {
            bundle: self.path.clone(),
            copy: self.copy.root().into(),
            origin: self.origin.root().into(),
            transaction: self.transaction.clone(),
        }
    }
}

#[cfg(test)]
pub(super) fn roundtrip_manifest_for_test(bytes: &[u8]) -> Result<Vec<u8>> {
    let manifest = decode_manifest(bytes)?;
    encode(&manifest)
}

#[cfg(test)]
pub(super) fn replay_factory_input_for_test(
    lab: &Path,
    stage: &Path,
    jdk: &Path,
    id: BundleId,
    original_input: &[u8],
) -> Result<Vec<u8>> {
    type OriginalInput = (
        u32,
        String,
        Uuid,
        BundleId,
        FileIdentity,
        String,
        Vec<resources::ResourceRecord>,
        String,
        String,
    );
    let (
        schema,
        purpose,
        review_id,
        original_id,
        original_lab,
        original_destination,
        records,
        _,
        _,
    ): OriginalInput = serde_json::from_slice(original_input).map_err(|_| Error::Identity)?;
    if schema != 1 || purpose != "native_status_factory" || original_id != id {
        return Err(Error::Identity);
    }
    let current = resources::Resources::inspect(lab, stage, jdk, resources::fixture_pins())?;
    let lab_identity = directory_identity(lab)?;
    let destination = lab.join(format!("native-status-{}", id.0));
    let destination = destination.to_str().ok_or(Error::UnsafePath)?;
    if original_lab != lab_identity
        || original_destination != destination
        || current.records != records
    {
        return Err(Error::Changed);
    }
    // Reobserve real resources and retain their normal pins until the same
    // production encoder has consumed them. No executable plan is recreated.
    factory_digest_input(review_id, id, lab_identity, destination, &current.records)
}
