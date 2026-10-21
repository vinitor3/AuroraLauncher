use super::eula::AppliedEula;
#[cfg(test)]
use super::manifest::decode_manifest;
pub(super) use super::manifest::Manifest;
use super::manifest::{read_manifest, ManifestKind, ManifestV1};
use super::*;
use crate::engine::{
    acquire_instance_mutation_lock, acquire_instance_recovery_lock, ContentUpdateError,
};

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
        _pins: directories,
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
        let guard = lock(&copy)?;
        copy.ensure_layout().map_err(|_| Error::Io)?;
        self.resources.copy_artifacts(copy.root())?;
        recovery::write_new(
            &copy.root().join("server.properties"),
            resources::PROPERTIES,
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
        let manifest = Manifest::StatusV1(ManifestV1 {
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
        });
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
        self.bundle.manifest.status_v1()?;
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
    _pins: Vec<File>,
    _files: Vec<recovery::SecureFile>,
}
impl Bundle {
    pub fn reopen(lab: &Path, id: BundleId) -> Result<Self> {
        explicit_path(lab)?;
        let path = lab.join(format!("native-status-{}", id.0));
        let mut pins = recovery::pin_directory(&path)?;
        let native = internal_directory(&path.join("native"), &mut pins)?;
        let manifest = read_manifest(&path.join("bundle.json"))?;
        // B1 recognizes the closed V2 wire format, but no V2 bundle can acquire
        // file pins, Recovery/EULA authority or an execution preparation yet.
        if manifest.kind() != ManifestKind::StatusV1 {
            return Err(Error::Identity);
        }
        if manifest.schema() != 1
            || manifest.id() != id
            || manifest.lab_identity() != &directory_identity(lab)?
            || manifest.bundle_identity() != &directory_identity(&path)?
            || manifest.native_identity() != &directory_identity(&native)?
            || manifest.copy_uuid() == manifest.origin_uuid()
            || Uuid::parse_str(manifest.copy_uuid()).is_err()
            || Uuid::parse_str(manifest.origin_uuid()).is_err()
            || manifest.installed().len() != 9
            || manifest.resources().len() != 11
            || manifest.provenance().len() != 6
        {
            return Err(Error::Identity);
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
            _pins: pins,
            _files: Vec::new(),
        };
        result.pin_files()?;
        Ok(result)
    }
    fn pin_files(&mut self) -> Result<()> {
        self.manifest.status_v1()?;
        self._files
            .push(recovery::secure_open(&self.path.join("bundle.json"))?);
        for record in self.manifest.installed() {
            if !resources::artifact_name(&record.name) {
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
        self.manifest.status_v1()?;
        if self.path
            != self
                .lab
                .join(format!("native-status-{}", self.manifest.id().0))
            || !diagnostic_lab::is_lab_candidate(&self.copy)
            || &directory_identity(&self.lab)? != self.manifest.lab_identity()
            || &directory_identity(&self.path)? != self.manifest.bundle_identity()
            || &directory_identity(&self.native)? != self.manifest.native_identity()
            || self.manifest.installed().len() != 9
            || self.manifest.resources().len() != 11
            || self.manifest.provenance().len() != 6
        {
            return Err(Error::Identity);
        }
        let actual = read_manifest(&self.path.join("bundle.json"))?;
        actual.status_v1()?;
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
