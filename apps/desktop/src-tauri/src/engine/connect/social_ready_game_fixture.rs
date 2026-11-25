//! Test-only ready-game backend reached through the real signed social actor.
//! It starts one owned native child, never Java, and receives no executable path.
use super::*;
use crate::engine::connect::{
    PreparedReadyGame, ReadyGameAppearance, ReadyGameBackend, ReadyGameSpawnGate,
};
use std::{
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Condvar,
    },
};

#[derive(Default)]
struct Gate {
    entered: AtomicBool,
    open: Mutex<bool>,
    signal: Condvar,
}
impl Gate {
    fn wait(&self) -> Result<(), ReadyGameLaunchError> {
        self.entered.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut open = self.open.lock().unwrap();
        while !*open {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(ReadyGameLaunchError::Expired);
            }
            (open, _) = self.signal.wait_timeout(open, left).unwrap();
        }
        Ok(())
    }
    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.signal.notify_all();
    }
}

pub(super) struct Fixture {
    prepare_gate: Option<Gate>,
    spawn_gate: Option<Gate>,
    spawns: AtomicUsize,
    epoch_rejected_before_spawn: AtomicBool,
    child: Mutex<Option<Child>>,
}
impl Fixture {
    pub(super) fn new(hold: &str) -> Result<Arc<Self>, ConnectError> {
        if !matches!(hold, "none" | "prepare" | "spawn") {
            return Err(ConnectError::InvalidRequest);
        }
        Ok(Arc::new(Self {
            prepare_gate: (hold == "prepare").then(Gate::default),
            spawn_gate: (hold == "spawn").then(Gate::default),
            spawns: AtomicUsize::new(0),
            epoch_rejected_before_spawn: AtomicBool::new(false),
            child: Mutex::new(None),
        }))
    }
    pub(super) fn request(operation: &str) -> ReadyGameLaunchRequest {
        ReadyGameLaunchRequest {
            operation_id: operation.into(),
            instance_id: format!("aurora-test-{}", uuid::Uuid::new_v4()),
            source_instance_id: "social-ready-fixture".into(),
            context_key: "social-ready-fixture-owner".into(),
            java_executable: "C:/fixture/java.exe".into(),
            nickname: "FixturePlayer".into(),
            appearance: ReadyGameAppearance {
                skin_url: None,
                skin_file: None,
                cape_url: None,
                cape_file: None,
                share_cape: None,
                skin_model: None,
            },
        }
    }
    pub(super) fn release(&self) {
        if let Some(gate) = &self.prepare_gate {
            gate.release();
        }
        if let Some(gate) = &self.spawn_gate {
            gate.release();
        }
    }
    pub(super) fn snapshot(&self) -> Value {
        let mut child = self.child.lock().unwrap();
        let alive = child
            .as_mut()
            .is_some_and(|child| child.try_wait().unwrap().is_none());
        json!({"spawns": self.spawns.load(Ordering::SeqCst), "childAlive": alive,
            "epochRejectedBeforeSpawn": self.epoch_rejected_before_spawn.load(Ordering::SeqCst),
            "prepareEntered": self.prepare_gate.as_ref().is_some_and(|gate| gate.entered.load(Ordering::SeqCst)),
            "spawnEntered": self.spawn_gate.as_ref().is_some_and(|gate| gate.entered.load(Ordering::SeqCst))})
    }
    pub(super) fn cleanup(&self) {
        self.release();
        if let Some(mut child) = self.child.lock().unwrap().take() {
            drop(child.stdin.take()); // Only our child: normal EOF, no kill.
            assert!(child.wait().unwrap().success(), "owned ready child failed");
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.cleanup();
    }
}
struct Prepared {
    fixture: Arc<Fixture>,
    instance: String,
    voice: Option<crate::engine::connect::ready_game_profile::ReadyVoiceReview>,
}
impl ReadyGameBackend for Arc<Fixture> {
    fn prepare(
        &self,
        request: &ReadyGameLaunchRequest,
        _: SocketAddr,
        voice_profile: Option<crate::engine::connect_native_voice::NativeVoiceProfile>,
        _runtime_profile: Option<crate::engine::connect_ready_profile::ReadyProfile>,
        _: &CancellationToken,
    ) -> Result<Box<dyn PreparedReadyGame>, ReadyGameLaunchError> {
        if let Some(gate) = &self.prepare_gate {
            gate.wait()?;
        }
        Ok(Box::new(Prepared {
            fixture: self.clone(),
            instance: request.instance_id.clone(),
            voice: voice_profile.map(|profile| {
                crate::engine::connect::ready_game_profile::ReadyVoiceReview {
                    profile,
                    mods: vec![crate::engine::connect::ready_game_profile::ReadyVoiceMod {
                        relative: "mods/test-only-voice-fixture.jar".into(),
                        bytes: 1,
                        sha256: "cc".repeat(32),
                        action:
                            crate::engine::connect::ready_game_profile::ReadyVoiceAction::Install,
                    }],
                }
            }),
        }))
    }
}
impl PreparedReadyGame for Prepared {
    fn voice_review(
        &self,
    ) -> Option<&crate::engine::connect::ready_game_profile::ReadyVoiceReview> {
        self.voice.as_ref()
    }
    fn profile(&self) -> crate::engine::connect_ready_profile::ReadyProfile {
        crate::engine::connect_ready_profile::ReadyProfile::Fabric1211
    }
    fn fingerprint(&self) -> &str {
        "signed-social-ready-owned-fixture"
    }
    fn core_sha256(&self) -> &str {
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }
    fn skins_sha256(&self) -> &str {
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    }
    fn launch(
        self: Box<Self>,
        gate: &ReadyGameSpawnGate,
        _: &CancellationToken,
    ) -> Result<ReadyGameLaunchResult, ReadyGameLaunchError> {
        if let Some(wait) = &self.fixture.spawn_gate {
            wait.wait()?;
        }
        let mut tracked = self.fixture.child.lock().unwrap();
        let result = gate.with_current(|| {
            if tracked.is_some() {
                return Err(ReadyGameLaunchError::Busy);
            }
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "engine::connect::live::social::ready_fixture::social_ready_controlled_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("AURORA_SOCIAL_READY_CHILD", "owned-fixture")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            let child = command
                .spawn()
                .map_err(|_| ReadyGameLaunchError::LaunchFailed)?;
            let process_id = child.id();
            *tracked = Some(child);
            self.fixture.spawns.fetch_add(1, Ordering::SeqCst);
            Ok(ReadyGameLaunchResult {
                run_id: uuid::Uuid::new_v4().to_string(),
                process_id,
                instance_id: self.instance.clone(),
                world_state: "notVerified",
            })
        });
        self.fixture
            .epoch_rejected_before_spawn
            .store(gate.epoch_rejected_for_test(), Ordering::SeqCst);
        result
    }
}

#[test]
#[ignore = "owned native child started only through the signed social fixture"]
fn social_ready_controlled_child() {
    use std::io::Read;
    assert_eq!(
        std::env::var("AURORA_SOCIAL_READY_CHILD").as_deref(),
        Ok("owned-fixture")
    );
    let mut bytes = Vec::new();
    std::io::stdin().take(1).read_to_end(&mut bytes).unwrap();
    assert!(bytes.is_empty());
}
