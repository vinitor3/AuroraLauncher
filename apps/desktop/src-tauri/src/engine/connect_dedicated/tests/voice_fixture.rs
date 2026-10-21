//! Small, explicitly selected test pins over real diagnostic factory copies.
//! No persisted field, path spelling or cfg(test) default selects these pins.
use super::*;
use crate::engine::connect_voice::dedicated::{manifest::StoredVoiceProfileV2, NativeVoiceProfile};

pub(super) const PROFILES: [NativeVoiceProfile; 2] = [
    NativeVoiceProfile::SimpleVoiceChat,
    NativeVoiceProfile::PlasmoVoice,
];

pub(super) struct VoiceFixture {
    pub fixture: Fixture,
    pub profile: NativeVoiceProfile,
}

impl VoiceFixture {
    pub(super) fn new(profile: NativeVoiceProfile) -> Self {
        let fixture = Fixture::new();
        // The existing fixture writes the nine game artifacts and JDK. Only
        // compiled test extras for this profile are added to its own stage.
        for (name, bytes) in resources::fixture_voice_artifacts(profile) {
            let path = fixture.stage.join(name);
            fs::create_dir_all(path.parent().expect("voice_fixture_extra_parent"))
                .expect("voice_fixture_extra_directory");
            fs::write(path, bytes).expect("voice_fixture_extra");
        }
        Self { fixture, profile }
    }

    pub(super) fn plan(&self, hooks: Hooks) -> FactoryPlan {
        factory::fixture_voice_plan(
            &self.fixture.lab,
            &self.fixture.stage,
            &self.fixture.jdk,
            self.profile,
            true,
            hooks,
        )
        .expect("voice_fixture_plan")
    }

    pub(super) fn prepared(&self, hooks: Hooks) -> PreparedStatus {
        let plan = self.plan(hooks);
        let approval = plan.review().digest.clone();
        plan.create(&approval).expect("voice_fixture_create")
    }

    pub(super) fn reopen(&self, id: BundleId) -> Result<factory::Bundle> {
        factory::Bundle::reopen_fixture(
            &self.fixture.lab,
            id,
            resources::fixture_voice_pins(self.profile),
        )
    }

    pub(super) fn recover(
        &self,
        id: BundleId,
        probe: &probe::Probe,
        hooks: Hooks,
    ) -> Result<EulaRecoveryPlan> {
        eula::prepare_recovery_fixture_voice(
            &self.fixture.lab,
            id,
            resources::fixture_voice_pins(self.profile),
            probe.boxed(),
            hooks,
        )
    }

    pub(super) fn stored_profile(&self) -> StoredVoiceProfileV2 {
        match self.profile {
            NativeVoiceProfile::SimpleVoiceChat => StoredVoiceProfileV2::Svc,
            NativeVoiceProfile::PlasmoVoice => StoredVoiceProfileV2::Plasmo,
        }
    }

    pub(super) fn mod_names(&self) -> &'static [&'static str] {
        match self.profile {
            NativeVoiceProfile::SimpleVoiceChat => &["mods/voicechat-fabric-1.21.1-2.6.22.jar"],
            NativeVoiceProfile::PlasmoVoice => &[
                "mods/plasmovoice-fabric-1.21.1-2.1.17.jar",
                "mods/fabric-api-0.116.10+1.21.1.jar",
            ],
        }
    }

    pub(super) fn configuration_path(&self, paths: &TestPaths) -> PathBuf {
        paths.copy.join(self.stored_profile().configuration_name())
    }
}
