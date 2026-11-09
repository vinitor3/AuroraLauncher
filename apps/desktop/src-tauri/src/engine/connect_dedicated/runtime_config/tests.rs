//! Filesystem-only C0 fixtures. No Bundle, Recovery observation, run or Java.
use super::*;
use std::{
    collections::BTreeMap,
    os::windows::{fs::MetadataExt, process::CommandExt},
    panic::{catch_unwind, AssertUnwindSafe},
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const PROFILES: [StoredVoiceProfileV2; 2] =
    [StoredVoiceProfileV2::Svc, StoredVoiceProfileV2::Plasmo];
const WAIT: Duration = Duration::from_secs(10);

struct Fixture {
    root: PathBuf,
    canonical: PathBuf,
    parent: PathBuf,
    identity: FileIdentity,
    pins: Option<Vec<File>>,
    profile: StoredVoiceProfileV2,
}
impl Fixture {
    fn new(profile: StoredVoiceProfileV2) -> Self {
        let temporary = std::env::temp_dir();
        let parent = temporary.canonicalize().unwrap();
        let root = temporary.join(format!(
            "aurora-runtime-config-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&root).unwrap();
        let canonical = root.canonicalize().unwrap();
        let pins = recovery::pin_directory(&root).unwrap();
        let identity = super::super::file_identity(pins.first().unwrap()).unwrap();
        let fixture = Self {
            root,
            canonical,
            parent,
            identity,
            pins: Some(pins),
            profile,
        };
        fs::create_dir_all(fixture.path().parent().unwrap()).unwrap();
        fs::write(fixture.path(), profile.configuration()).unwrap();
        fixture
    }
    fn path(&self) -> PathBuf {
        self.root.join(self.profile.configuration_name())
    }
    fn reader(&self) -> PinnedRuntimeConfiguration {
        PinnedRuntimeConfiguration::pin(&self.root, &self.identity, self.profile).unwrap()
    }
    fn sample(&self) -> RuntimeConfigSample {
        self.reader().read_sample().unwrap()
    }
    fn reject(&self, bytes: &[u8], expected: RuntimeConfigError) {
        fs::write(self.path(), bytes).unwrap();
        let before = tree(&self.root);
        assert_eq!(self.reader().read_sample(), Err(expected));
        assert_eq!(tree(&self.root), before);
        assert_eq!(fs::read(self.path()).unwrap(), bytes);
    }
    fn assert_parent_released(&self) {
        let parent = self.path().parent().unwrap().to_path_buf();
        let parked = parent.with_file_name("released-runtime-parent");
        fs::rename(&parent, &parked).expect("runtime_no_directory_pin_escaped");
        fs::rename(&parked, &parent).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            drop(self.pins.take());
            return;
        }
        let metadata = fs::symlink_metadata(&self.root).unwrap();
        assert!(metadata.is_dir() && metadata.file_attributes() & REPARSE_POINT == 0);
        assert_eq!(self.root.canonicalize().unwrap(), self.canonical);
        assert_eq!(self.canonical.parent(), Some(self.parent.as_path()));
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("aurora-runtime-config-test-"));
        // The exact generated absolute target is checked while its original
        // root handle is still pinned. Never recurse through a replacement root.
        drop(self.pins.take());
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Directory,
    File(Vec<u8>),
    Reparse,
}
fn tree(root: &Path) -> BTreeMap<String, Entry> {
    fn walk(root: &Path, path: &Path, result: &mut BTreeMap<String, Entry>) {
        assert!(result.len() < 96, "runtime_fixture_tree_bound");
        let name = path
            .strip_prefix(root)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let metadata = fs::symlink_metadata(path).unwrap();
        if metadata.file_attributes() & REPARSE_POINT != 0 {
            result.insert(name, Entry::Reparse);
        } else if metadata.is_dir() {
            result.insert(name, Entry::Directory);
            for child in fs::read_dir(path).unwrap() {
                walk(root, &child.unwrap().path(), result);
            }
        } else {
            assert!(metadata.is_file() && metadata.len() <= MAX_BYTES as u64 + 1);
            result.insert(name, Entry::File(fs::read(path).unwrap()));
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}

#[derive(Default)]
struct GateState {
    entered: bool,
    released: bool,
}
#[derive(Default)]
pub(super) struct ReadGate {
    state: Mutex<GateState>,
    changed: Condvar,
}
impl ReadGate {
    pub(super) fn arrive(&self) {
        let deadline = Instant::now() + WAIT;
        let mut state = self.state.lock().unwrap();
        state.entered = true;
        self.changed.notify_all();
        while !state.released {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "runtime_fixture_reader_gate_deadline");
            state = self.changed.wait_timeout(state, left).unwrap().0;
        }
    }
    fn wait_entered(&self) {
        let deadline = Instant::now() + WAIT;
        let mut state = self.state.lock().unwrap();
        while !state.entered {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "runtime_fixture_gate_arrival_deadline");
            state = self.changed.wait_timeout(state, left).unwrap().0;
        }
    }
    fn release(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.released = true;
        self.changed.notify_all();
    }
}
struct PendingRead {
    gate: Arc<ReadGate>,
    task: Option<JoinHandle<Result<RuntimeConfigSample>>>,
}
impl PendingRead {
    fn start(reader: PinnedRuntimeConfiguration) -> Self {
        let gate = Arc::new(ReadGate::default());
        let in_reader = gate.clone();
        let task = thread::spawn(move || {
            reader.read_inner(ReadHooks {
                after_open: Some(in_reader),
                panic_after_open: false,
            })
        });
        Self {
            gate,
            task: Some(task),
        }
    }
    fn finish(mut self) -> Result<RuntimeConfigSample> {
        self.gate.release();
        self.task
            .take()
            .unwrap()
            .join()
            .expect("runtime_fixture_reader_join")
    }
}
impl Drop for PendingRead {
    fn drop(&mut self) {
        self.gate.release();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

#[derive(Clone, Copy)]
enum WriteAction {
    Rewrite,
    RenameIdentical,
    Remove,
}
struct PendingWriter {
    task: Option<JoinHandle<()>>,
    result: mpsc::Receiver<std::io::Result<()>>,
}
impl PendingWriter {
    fn start(path: PathBuf, content: Vec<u8>, action: WriteAction) -> Self {
        let (sender, result) = mpsc::sync_channel(1);
        let task = thread::spawn(move || {
            let outcome = (|| match action {
                WriteAction::Rewrite => fs::write(&path, content),
                WriteAction::RenameIdentical => {
                    let next = path.with_file_name("runtime-next.tmp");
                    let old = path.with_file_name("runtime-original.bin");
                    fs::write(&next, content)?;
                    fs::rename(&path, old)?;
                    fs::rename(next, path)
                }
                WriteAction::Remove => fs::remove_file(path),
            })();
            let _ = sender.try_send(outcome);
        });
        Self {
            task: Some(task),
            result,
        }
    }
    fn wait_done(&self) {
        self.result
            .recv_timeout(WAIT)
            .expect("runtime_writer_completed_while_reader_held")
            .expect("runtime_writer_real_filesystem_action");
    }
}
impl Drop for PendingWriter {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

#[test]
fn stable_baselines_and_writer_extensions_are_readonly_data_with_no_retained_resources() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let before = tree(&fixture.root);
        let first = fixture.sample();
        let second = fixture.sample();
        assert_eq!(first.require_same_revision(&second), Ok(()));
        assert_eq!(tree(&fixture.root), before);
        assert!(
            !fixture.root.join(".aurora").exists(),
            "runtime_no_store_or_database_initialization"
        );
        // Keep both samples alive while renaming the actual parent directory.
        fixture.assert_parent_released();
        let mut generated = profile.configuration().to_vec();
        let extra = match profile {
            StoredVoiceProfileV2::Svc => {
                b"# writer defaults\nmtu_size=1275\ncodec=VOIP\n".as_slice()
            }
            StoredVoiceProfileV2::Plasmo => {
                b"# writer defaults\nserver_id = 'opaque-local-fixture'\n[voice]\nmtu_size = 1024\n"
                    .as_slice()
            }
        };
        generated.extend_from_slice(extra);
        fs::write(fixture.path(), generated).unwrap();
        let changed = fixture.sample();
        assert_eq!(
            first.require_same_revision(&changed),
            Err(RuntimeConfigError::Changed)
        );
        assert_eq!(changed.require_same_revision(&fixture.sample()), Ok(()));
    }
}

#[test]
fn real_in_place_writer_finishes_before_reader_release_and_invalidates_the_sample() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let reader = PendingRead::start(fixture.reader());
        reader.gate.wait_entered();
        let mut rewritten = profile.configuration().to_vec();
        rewritten.extend_from_slice(b"# actual writer completed before the reader resumes\n");
        let writer = PendingWriter::start(fixture.path(), rewritten.clone(), WriteAction::Rewrite);
        writer.wait_done();
        assert_eq!(fs::read(fixture.path()).unwrap(), rewritten);
        assert_eq!(reader.finish(), Err(RuntimeConfigError::Changed));
        drop(writer);
        fixture.assert_parent_released();
        fixture.sample();
    }
}

#[test]
fn byte_identical_rename_with_original_handle_alive_is_detected_by_pathname_reopen() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let original = FileStamp::capture(&open_read(&fixture.path()).unwrap()).unwrap();
        let reader = PendingRead::start(fixture.reader());
        reader.gate.wait_entered();
        let writer = PendingWriter::start(
            fixture.path(),
            profile.configuration().to_vec(),
            WriteAction::RenameIdentical,
        );
        writer.wait_done();
        let old = fixture.path().with_file_name("runtime-original.bin");
        let retained = FileStamp::capture(&open_read(&old).unwrap()).unwrap();
        let replacement = FileStamp::capture(&open_read(&fixture.path()).unwrap()).unwrap();
        assert_eq!(
            retained, original,
            "runtime_original_handle_metadata_stays_favorable"
        );
        assert_ne!(replacement.identity, original.identity);
        assert_eq!(fs::read(&old).unwrap(), profile.configuration());
        assert_eq!(fs::read(fixture.path()).unwrap(), profile.configuration());
        assert_eq!(reader.finish(), Err(RuntimeConfigError::Changed));
        drop(writer);
        fixture.assert_parent_released();
        // A later independent stable query may accept the new physical file.
        fixture.sample();
    }
}

#[test]
fn deletion_after_positive_open_is_changed_and_all_unwind_paths_release_handles() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let reader = PendingRead::start(fixture.reader());
        reader.gate.wait_entered();
        let writer = PendingWriter::start(fixture.path(), Vec::new(), WriteAction::Remove);
        writer.wait_done();
        assert!(!fixture.path().exists());
        assert_eq!(reader.finish(), Err(RuntimeConfigError::Changed));
        drop(writer);
        fixture.assert_parent_released();
        fs::write(fixture.path(), profile.configuration()).unwrap();
        let reader = fixture.reader();
        assert!(
            catch_unwind(AssertUnwindSafe(|| reader.read_inner(ReadHooks {
                after_open: None,
                panic_after_open: true,
            })))
            .is_err()
        );
        fixture.assert_parent_released();
        drop(
            fs::OpenOptions::new()
                .write(true)
                .open(fixture.path())
                .unwrap(),
        );
        let reader = fixture.reader();
        assert!(catch_unwind(AssertUnwindSafe(|| {
            let pending = PendingRead::start(reader);
            pending.gate.wait_entered();
            panic!("runtime_fixture_parent_unwind");
        }))
        .is_err());
        fixture.assert_parent_released();
        fixture.sample();
    }
}

#[test]
fn missing_leaf_directory_hardlink_and_wrong_root_are_closed_errors_without_writes() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let other = Fixture::new(profile);
        assert_eq!(
            PinnedRuntimeConfiguration::pin(&fixture.root, &other.identity, profile).err(),
            Some(RuntimeConfigError::Changed)
        );
        fs::remove_file(fixture.path()).unwrap();
        assert_eq!(
            fixture.reader().read_sample(),
            Err(RuntimeConfigError::Missing)
        );
        fs::create_dir(fixture.path()).unwrap();
        assert_eq!(
            fixture.reader().read_sample(),
            Err(RuntimeConfigError::UnsafePath)
        );
        fs::remove_dir(fixture.path()).unwrap();
        fs::write(fixture.path(), profile.configuration()).unwrap();
        let link = fixture.root.join("second-config-link.bin");
        fs::hard_link(fixture.path(), &link).unwrap();
        let before = tree(&fixture.root);
        assert_eq!(
            fixture.reader().read_sample(),
            Err(RuntimeConfigError::UnsafePath)
        );
        assert_eq!(tree(&fixture.root), before);
        fs::remove_file(link).unwrap();
        fixture.sample();
    }
}

#[test]
fn only_trusted_drive_paths_and_internal_extended_drive_paths_can_be_pinned() {
    let fixture = Fixture::new(StoredVoiceProfileV2::Svc);
    for path in [
        PathBuf::from("relative/copy"),
        PathBuf::from(r"\\fixture.invalid\share\copy"),
        PathBuf::from(r"\\?\UNC\fixture.invalid\share\copy"),
        PathBuf::from(r"\\.\C:\copy"),
        fixture.root.join("../copy"),
        fixture.root.join("copy:stream"),
        fixture.root.join("NUL"),
    ] {
        assert_eq!(
            PinnedRuntimeConfiguration::pin(&path, &fixture.identity, fixture.profile).err(),
            Some(RuntimeConfigError::UnsafePath)
        );
    }
    let normal = fixture.sample();
    let extended =
        PinnedRuntimeConfiguration::pin(&fixture.canonical, &fixture.identity, fixture.profile)
            .unwrap()
            .read_sample()
            .unwrap();
    assert_eq!(normal.require_same_revision(&extended), Ok(()));
}

#[test]
fn byte_limits_are_exact_and_read_errors_never_truncate_or_rewrite() {
    for profile in PROFILES {
        let fixture = Fixture::new(profile);
        let mut at_limit = profile.configuration().to_vec();
        at_limit.push(b'#');
        at_limit.resize(MAX_BYTES - 1, b'x');
        at_limit.push(b'\n');
        fs::write(fixture.path(), &at_limit).unwrap();
        fixture.sample();
        at_limit.push(b' ');
        fixture.reject(&at_limit, RuntimeConfigError::Limit);
        assert_eq!(
            validate_configuration(profile, &at_limit),
            Err(RuntimeConfigError::Limit)
        );
    }
}

#[test]
fn svc_additional_runtime_policy_does_not_change_the_legacy_lexer() {
    let fixture = Fixture::new(StoredVoiceProfileV2::Svc);
    for bytes in [
        b"port=0\nbind_address=127.0.0.1\nvoice_host=\n".as_slice(),
        b"port=-1\nbind_address=127.0.0.1\nvoice_host=\n",
        b"port=43002\nbind_address=0.0.0.0\nvoice_host=\n",
        b"port=43002\nbind_address=\nvoice_host=\n",
        b"port=43002\nbind_address=127.0.0.2\nvoice_host=\n",
        b"port=43002\nbind_address=127.0.0.1\nvoice_host=127.0.0.1:35554\n",
        b"port=43002 \nbind_address=127.0.0.1\nvoice_host=\n",
        b"port=43002\nvoice_host=\n",
        b"bind_address=127.0.0.1\nvoice_host=\n",
    ] {
        assert!(
            super::super::super::svc_value_span(bytes).is_ok(),
            "runtime_policy_case_is_legacy_valid"
        );
        fixture.reject(bytes, RuntimeConfigError::Incompatible);
    }
    for bytes in [
        b"# comments Unicode: a\n port \t=\t43002\nbind_address=127.0.0.1\nvoice_host= \t"
            .as_slice(),
        b"port=43002\r\nbind_address=127.0.0.1\r\nvoice_host=\r\nunknown_key=value\r\n",
    ] {
        fs::write(fixture.path(), bytes).unwrap();
        fixture.sample();
    }
}

#[test]
fn svc_legacy_malformed_inputs_and_existing_line_key_bounds_are_not_relaxed() {
    let fixture = Fixture::new(StoredVoiceProfileV2::Svc);
    for bytes in [
        b"port=43002\nbind_address=127.0.0.1\nvoice_host=\nvoice_host=\n".as_slice(),
        b"port=43002\nport=43002\nbind_address=127.0.0.1\nvoice_host=\n",
        b"port=43002\nbind_address=127.0.0.1\nbind_address=127.0.0.1\nvoice_host=\n",
        b"port=43002\nbind_address=127.0.0.1\nvoice_host=\\\ncontinued\n",
        b"port=43002\nbind_address=127.0.0.1\nvoice_host:\n",
        b"port=43002\rbind_address=127.0.0.1\nvoice_host=\n",
        b"port=43002\nbind_address=127.0.0.1\nvoice_host=\xff\n",
        b"port=43002\nbind_address=127.0.0.1\nvoice_host=\0\n",
    ] {
        assert!(super::super::super::svc_value_span(bytes).is_err());
        fixture.reject(bytes, RuntimeConfigError::Incompatible);
    }
    let base = b"port=43002\nbind_address=127.0.0.1\nvoice_host=\n";
    let mut lines = base.to_vec();
    for _ in 3..4096 {
        lines.extend_from_slice(b"#\n");
    }
    fs::write(fixture.path(), &lines).unwrap();
    fixture.sample();
    lines.extend_from_slice(b"#\n");
    fixture.reject(&lines, RuntimeConfigError::Limit);
    let mut keys = base.to_vec();
    for index in 3..512 {
        keys.extend_from_slice(format!("extra_{index}=x\n").as_bytes());
    }
    fs::write(fixture.path(), &keys).unwrap();
    fixture.sample();
    keys.extend_from_slice(b"extra_overflow=x\n");
    assert!(matches!(
        super::super::super::svc_value_span(&keys),
        Err(RecoveryError::Integrity)
    ));
    fixture.reject(&keys, RuntimeConfigError::Incompatible);
}

// As in the existing filesystem fixtures, paths travel only in environment
// values to a constant hidden command. The own helper is collected on unwind.
struct Junction(PathBuf);
impl Junction {
    fn create(path: &Path, target: &Path) -> Self {
        let junction = Self(path.to_path_buf());
        let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap());
        let child = Command::new(system.join("System32/WindowsPowerShell/v1.0/powershell.exe"))
            .creation_flags(0x08000000).env_clear().env("SystemRoot", &system)
            .env("AURORA_RUNTIME_LINK", path).env("AURORA_RUNTIME_TARGET", target)
            .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:AURORA_RUNTIME_LINK -Target $env:AURORA_RUNTIME_TARGET | Out-Null"])
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        struct Process(Option<Child>);
        impl Drop for Process {
            fn drop(&mut self) {
                if let Some(mut child) = self.0.take() {
                    if !matches!(child.try_wait(), Ok(Some(_))) {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                }
            }
        }
        let mut process = Process(Some(child));
        let deadline = Instant::now() + WAIT;
        let status = loop {
            if let Some(status) = process.0.as_mut().unwrap().try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "runtime_junction_helper_deadline"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert!(status.success(), "runtime_real_junction_creation");
        assert_ne!(
            fs::symlink_metadata(path).unwrap().file_attributes() & REPARSE_POINT,
            0
        );
        drop(process);
        junction
    }
}
impl Drop for Junction {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.0)
            .is_ok_and(|metadata| metadata.file_attributes() & REPARSE_POINT != 0)
        {
            let removed = fs::remove_dir(&self.0);
            if !thread::panicking() {
                removed.expect("runtime_remove_only_junction");
            }
        }
    }
}

#[test]
fn real_reparse_leaf_and_parent_never_follow_the_other_fixture() {
    for profile in PROFILES {
        for parent in [false, true] {
            let fixture = Fixture::new(profile);
            let other = Fixture::new(profile);
            let selected = if parent {
                fixture.path().parent().unwrap().to_path_buf()
            } else {
                fixture.path()
            };
            let parked = fixture.root.join("retained-original-path");
            fs::rename(&selected, &parked).unwrap();
            let target = other.path().parent().unwrap().to_path_buf();
            let link = Junction::create(&selected, &target);
            let outside = tree(&other.root);
            let result = PinnedRuntimeConfiguration::pin(&fixture.root, &fixture.identity, profile)
                .and_then(PinnedRuntimeConfiguration::read_sample);
            assert_eq!(result, Err(RuntimeConfigError::UnsafePath));
            assert_eq!(tree(&other.root), outside);
            drop(link);
            fs::rename(&parked, &selected).unwrap();
            fixture.sample();
        }
    }
}

#[test]
fn loopback35554_policy_is_separate_and_caps_original_mod_datagrams() {
    for profile in [StoredVoiceProfileV2::Svc, StoredVoiceProfileV2::Plasmo] {
        let policy = ObservationPolicyV2::LiteralLoopback35554Durable;
        let bytes = policy.configuration(profile);
        assert!(validate_configuration_with_policy(profile, policy, bytes).is_ok());
        assert!(validate_configuration(profile, bytes).is_err());
        assert!(
            validate_configuration_with_policy(profile, policy, profile.configuration()).is_err()
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            policy.configuration_sha256(profile)
        );
        let text = std::str::from_utf8(bytes).unwrap();
        assert!(validate_configuration_with_policy(
            profile,
            policy,
            text.replace("35554", "35555").as_bytes()
        )
        .is_err());
        let with_mtu = |mtu| match profile {
            StoredVoiceProfileV2::Svc => format!("{text}mtu_size={mtu}\n"),
            StoredVoiceProfileV2::Plasmo => format!("{text}[voice]\nmtu_size = {mtu}\n"),
        };
        assert!(
            validate_configuration_with_policy(profile, policy, with_mtu(4096).as_bytes()).is_ok()
        );
        assert!(
            validate_configuration_with_policy(profile, policy, with_mtu(4097).as_bytes()).is_err()
        );
    }
}
