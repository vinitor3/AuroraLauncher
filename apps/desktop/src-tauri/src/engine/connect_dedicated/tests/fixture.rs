use super::*;
use crate::engine::{acquire_instance_recovery_lock, ContentUpdateError};

pub(super) struct Fixture {
    pub root: files::TestRoot,
    pub lab: PathBuf,
    pub stage: PathBuf,
    pub jdk: PathBuf,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let root = files::TestRoot::new();
        let lab = root.path().join("lab");
        let stage = lab.join("prepared-Test84");
        let jdk = root.path().join("jdk");
        fs::create_dir(&lab).expect("fixture_lab");
        fs::create_dir(&stage).expect("fixture_stage");
        fs::create_dir(&jdk).expect("fixture_jdk");
        fs::create_dir(jdk.join("bin")).expect("fixture_jdk_bin");
        for (relative, bytes) in resources::fixture_artifacts() {
            let path = stage.join(relative);
            fs::create_dir_all(path.parent().expect("fixture_artifact_parent"))
                .expect("fixture_artifact_directory");
            fs::write(path, bytes).expect("fixture_artifact");
        }
        let (release, executable) = resources::fixture_jdk();
        fs::write(jdk.join("release"), release).expect("fixture_release");
        fs::write(jdk.join("bin/java.exe"), executable).expect("fixture_java");
        Self {
            root,
            lab,
            stage,
            jdk,
        }
    }

    pub(super) fn plan(&self, hooks: Hooks) -> FactoryPlan {
        fixture_plan(&self.lab, &self.stage, &self.jdk, true, hooks).expect("fixture_plan")
    }

    pub(super) fn prepared(&self, hooks: Hooks) -> PreparedStatus {
        let plan = self.plan(hooks);
        let digest = plan.review().digest.clone();
        plan.create(&digest).expect("fixture_factory_create")
    }

    pub(super) fn recover(
        &self,
        id: BundleId,
        probe: &probe::Probe,
        hooks: Hooks,
    ) -> Result<EulaRecoveryPlan> {
        fixture_recovery(&self.lab, id, probe.boxed(), hooks)
    }
}

pub(super) fn target(paths: &TestPaths) -> PathBuf {
    paths.copy.join("eula.txt")
}

pub(super) fn slot(paths: &TestPaths, name: &str) -> PathBuf {
    paths.transaction.join(name)
}

pub(super) fn bytes(path: &Path) -> Vec<u8> {
    fs::read(path).expect("fixture_bytes")
}

pub(super) fn hook_fault(step: Step) -> Hooks {
    Hooks {
        after_step: Some(Arc::new(move |actual| {
            if actual == step {
                Err(Error::Injected)
            } else {
                Ok(())
            }
        })),
        ..Hooks::default()
    }
}

/// Reconstruct only a lock-test subject. This never supplies start, restore or
/// factory authority; those paths always consume the real factory's bundle.
pub(super) fn copy_instance(paths: &TestPaths) -> Instance {
    let native = paths
        .copy
        .parent()
        .and_then(Path::parent)
        .expect("fixture_native");
    let id = paths
        .copy
        .file_name()
        .and_then(|value| value.to_str())
        .expect("fixture_copy_id");
    InstanceLayout::new(native).open(InstanceId::parse(id).expect("fixture_instance_id"))
}

pub(super) fn locked(paths: &TestPaths) {
    assert!(matches!(
        acquire_instance_recovery_lock(&copy_instance(paths)),
        Err(ContentUpdateError::Busy)
    ));
}

pub(super) fn unlocked(paths: &TestPaths) {
    drop(acquire_instance_recovery_lock(&copy_instance(paths)).expect("fixture_lock_released"));
}
