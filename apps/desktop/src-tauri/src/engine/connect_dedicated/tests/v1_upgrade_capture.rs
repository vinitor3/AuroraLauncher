//! Capture legacy V1 bytes and identities before schema2, then verify the same
//! physical fixtures with the current codecs and guarded recovery path.
//! These opt-ins never start a Child or construct recovery authority from JSON.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const INDEX_ENV: &str = "AURORA_CONNECT_V1_CAPTURE_INDEX";
const INDEX_HASH_ENV: &str = "AURORA_CONNECT_V1_CAPTURE_SHA256";
const INDEX_LIMIT: u64 = 4 * 1024 * 1024;
const INDEX_EVENT: &str = "native_status_v1_upgrade_prefixes_before_schema2";
const GOLDEN_DIRECTORY: &str = "native-voice85-v1-golden-43bb0dca-8754-4911-a070-4cc7fcfee4f4";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Prefix {
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
const PREFIXES: [Prefix; 13] = [
    Prefix::Claim,
    Prefix::Snapshot,
    Prefix::Stage,
    Prefix::Intent,
    Prefix::OriginalMoved,
    Prefix::OriginalMarked,
    Prefix::AppliedMoved,
    Prefix::AppliedMarked,
    Prefix::RestoreIntent,
    Prefix::AppliedParked,
    Prefix::OriginalRestored,
    Prefix::RestoredMarked,
    Prefix::MayHaveSpawned,
];
impl Prefix {
    fn step(self) -> Step {
        match self {
            Self::Claim => Step::Claim,
            Self::Snapshot => Step::Snapshot,
            Self::Stage => Step::Stage,
            Self::Intent => Step::Intent,
            Self::OriginalMoved => Step::OriginalMoved,
            Self::OriginalMarked => Step::OriginalMarked,
            Self::AppliedMoved => Step::AppliedMoved,
            Self::AppliedMarked => Step::AppliedMarked,
            Self::RestoreIntent => Step::RestoreIntent,
            Self::AppliedParked => Step::AppliedParked,
            Self::OriginalRestored => Step::OriginalRestored,
            Self::RestoredMarked => Step::RestoredMarked,
            Self::MayHaveSpawned => Step::MayHaveSpawned,
        }
    }
    fn expected(self) -> (Option<EulaOutcome>, usize) {
        match self {
            Self::Snapshot | Self::Stage => (None, 0),
            Self::Claim | Self::Intent => (Some(EulaOutcome::Unchanged), 0),
            Self::OriginalMoved | Self::OriginalMarked => (Some(EulaOutcome::Unchanged), 1),
            Self::AppliedMoved
            | Self::AppliedMarked
            | Self::RestoreIntent
            | Self::MayHaveSpawned => (Some(EulaOutcome::Restored), 2),
            Self::AppliedParked => (Some(EulaOutcome::Restored), 1),
            Self::OriginalRestored | Self::RestoredMarked => (Some(EulaOutcome::Restored), 0),
        }
    }
}

/// Raw original UTF-8 bytes, not a Value or a new encoder's expected result.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginalBytes {
    utf8: String,
    sha256: String,
}
impl OriginalBytes {
    fn capture(bytes: Vec<u8>) -> Self {
        assert!(bytes.len() as u64 <= MAX_JOURNAL, "v1_capture_byte_limit");
        Self {
            sha256: digest(&bytes),
            utf8: String::from_utf8(bytes).expect("v1_capture_utf8"),
        }
    }
    fn bytes(&self) -> &[u8] {
        let bytes = self.utf8.as_bytes();
        assert!(bytes.len() as u64 <= MAX_JOURNAL, "v1_capture_byte_limit");
        assert_eq!(digest(bytes), self.sha256, "v1_original_bytes_hash");
        bytes
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Place {
    Manifest,
    Target,
    Claim,
    Intent,
    Snapshot,
    Stage,
    Old,
    Applied,
    OriginalMark,
    AppliedMark,
    SpawnMark,
    RestoreIntent,
    Restored,
}
const PLACES: [Place; 13] = [
    Place::Manifest,
    Place::Target,
    Place::Claim,
    Place::Intent,
    Place::Snapshot,
    Place::Stage,
    Place::Old,
    Place::Applied,
    Place::OriginalMark,
    Place::AppliedMark,
    Place::SpawnMark,
    Place::RestoreIntent,
    Place::Restored,
];
impl Place {
    fn path(self, paths: &TestPaths) -> PathBuf {
        match self {
            Self::Manifest => paths.bundle.join("bundle.json"),
            Self::Target => target(paths),
            Self::Claim => slot(paths, "claim.json"),
            Self::Intent => slot(paths, "intent.json"),
            Self::Snapshot => slot(paths, "snapshot.bin"),
            Self::Stage => slot(paths, "stage.bin"),
            Self::Old => slot(paths, "old.bin"),
            Self::Applied => slot(paths, "applied.bin"),
            Self::OriginalMark => slot(paths, "original-moved.json"),
            Self::AppliedMark => slot(paths, "applied.json"),
            Self::SpawnMark => slot(paths, "may-have-spawned.json"),
            Self::RestoreIntent => slot(paths, "restore-intent.json"),
            Self::Restored => slot(paths, "restored.json"),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskFile {
    place: Place,
    observed: Option<Observation>,
    original: Option<OriginalBytes>,
}
type TreeSnapshot = BTreeMap<String, String>;

fn tree_snapshot(path: &Path) -> TreeSnapshot {
    files::tree(path)
        .into_iter()
        .map(|(name, entry)| {
            let value = match entry {
                files::Entry::Directory => "directory".into(),
                files::Entry::File { bytes, sha256 } => format!("file:{bytes}:{sha256}"),
                files::Entry::Link => panic!("v1_capture_refuses_links"),
            };
            (name, value)
        })
        .collect()
}
fn disk_files(paths: &TestPaths) -> Vec<DiskFile> {
    PLACES
        .iter()
        .map(|place| {
            let path = place.path(paths);
            match fs::symlink_metadata(&path) {
                Ok(_) => {
                    let (observed, bytes) =
                        observe_file(&path, MAX_JOURNAL).expect("v1_capture_original_disk_file");
                    DiskFile {
                        place: *place,
                        observed: Some(observed),
                        original: Some(OriginalBytes::capture(bytes)),
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => DiskFile {
                    place: *place,
                    observed: None,
                    original: None,
                },
                Err(_) => panic!("v1_capture_metadata"),
            }
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preimages {
    factory: OriginalBytes,
    factory_digest: String,
    approved_files: OriginalBytes,
    eula: OriginalBytes,
    eula_digest: String,
    recipe: OriginalBytes,
    execution: OriginalBytes,
    execution_review_id: Uuid,
    execution_digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CapturedPrefix {
    prefix: Prefix,
    id: BundleId,
    root: PathBuf,
    root_identity: FileIdentity,
    canonical_root: PathBuf,
    lab: PathBuf,
    stage: PathBuf,
    jdk: PathBuf,
    bundle: PathBuf,
    copy: PathBuf,
    origin: PathBuf,
    transaction: PathBuf,
    original_eula: Observation,
    preimages: Preimages,
    disk: Vec<DiskFile>,
    root_tree: TreeSnapshot,
    origin_tree: TreeSnapshot,
    stage_tree: TreeSnapshot,
    jdk_tree: TreeSnapshot,
    expected_outcome: Option<EulaOutcome>,
    expected_renames: usize,
}
impl CapturedPrefix {
    fn paths(&self) -> TestPaths {
        TestPaths {
            bundle: self.bundle.clone(),
            copy: self.copy.clone(),
            origin: self.origin.clone(),
            transaction: self.transaction.clone(),
        }
    }
    fn original(&self, place: Place) -> &OriginalBytes {
        self.disk
            .iter()
            .find(|entry| entry.place == place)
            .and_then(|entry| entry.original.as_ref())
            .expect("v1_required_original_capture")
    }
    fn validate_root(&self) {
        explicit_path(&self.root).expect("v1_capture_root_ordinary_path");
        let basename = self
            .root
            .file_name()
            .and_then(|value| value.to_str())
            .and_then(|value| value.strip_prefix("aurora-dedicated-eula-test-"))
            .expect("v1_capture_generated_root_name");
        Uuid::parse_str(basename).expect("v1_capture_generated_root_uuid");
        assert_eq!(
            self.root.canonicalize().expect("v1_capture_same_root"),
            self.canonical_root,
            "v1_capture_same_root_path"
        );
        assert_eq!(
            self.canonical_root.parent(),
            Some(
                std::env::temp_dir()
                    .canonicalize()
                    .expect("v1_temp_parent")
                    .as_path()
            ),
            "v1_capture_confined_temp_root"
        );
        assert_eq!(
            directory_identity(&self.root).expect("v1_capture_root_identity"),
            self.root_identity,
            "v1_capture_same_root_identity"
        );
        assert_eq!(self.lab, self.root.join("lab"));
        assert_eq!(self.stage, self.lab.join("prepared-Test84"));
        assert_eq!(self.jdk, self.root.join("jdk"));
        assert_eq!(
            self.bundle,
            self.lab.join(format!("native-status-{}", self.id.0))
        );
        // The actual normal recovery path rederives extended-Disk spelling,
        // validates ancestry/provenance and acquires its own real guard.
        assert_eq!(
            self.transaction,
            self.bundle
                .join("eula-transaction")
                .canonicalize()
                .expect("v1_capture_transaction_path")
        );
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaptureIndex {
    schema: u32,
    event: String,
    prefixes: Vec<CapturedPrefix>,
}

fn validation_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("v1_workspace")
        .join(".aurora/connect-validation")
}
fn index_path() -> PathBuf {
    let path =
        PathBuf::from(std::env::var_os(INDEX_ENV).expect("explicit_v1_capture_index_required"));
    explicit_path(&path).expect("v1_capture_index_ordinary_path");
    assert_eq!(path.parent(), Some(validation_directory().as_path()));
    let uuid = path
        .file_name()
        .and_then(|value| value.to_str())
        .and_then(|value| value.strip_prefix("native-status-v1-upgrade-"))
        .and_then(|value| value.strip_suffix(".json"))
        .expect("v1_capture_index_name");
    Uuid::parse_str(uuid).expect("v1_capture_index_uuid");
    path
}

fn capture_prefix(fixture: &Fixture, prefix: Prefix) -> CapturedPrefix {
    let plan = fixture.plan(fixture::hook_fault(prefix.step()));
    let factory_digest = plan.review().digest.clone();
    let factory = OriginalBytes::capture(
        plan.v1_digest_input_for_test()
            .expect("v1_factory_original_input"),
    );
    assert_eq!(factory.sha256, factory_digest);
    let prepared = plan
        .create(&factory_digest)
        .expect("v1_capture_real_factory");
    let paths = prepared.test_paths();
    let id = prepared.bundle_id();
    let original_eula = observe_file(&target(&paths), 4096)
        .expect("v1_original_eula")
        .0;
    let approved_files = OriginalBytes::capture(encode(&prepared.approved_files).unwrap());
    let eula_digest = prepared.review().digest.clone();
    let eula = OriginalBytes::capture(
        eula::eula_input_for_test(&prepared).expect("v1_original_eula_input"),
    );
    assert_eq!(eula.sha256, eula_digest);
    // This only builds the original production recipe/review. It never calls
    // start, creates run-temp, applies EULA, or constructs a Child.
    let (prepared, recipe, execution, execution_review_id, execution_digest) =
        producer::capture_status_plan_for_test(prepared).expect("v1_original_status_plan");
    let recipe = OriginalBytes::capture(recipe);
    let execution = OriginalBytes::capture(execution);
    assert_eq!(execution.sha256, execution_digest);
    let approval = prepared.review().digest.clone();
    let interrupted = match prefix {
        Prefix::RestoreIntent
        | Prefix::AppliedParked
        | Prefix::OriginalRestored
        | Prefix::RestoredMarked => prepared
            .apply_eula(&approval)
            .expect("v1_apply_before_restore_capture")
            .restore_before_spawn()
            .map(|_| ()),
        Prefix::MayHaveSpawned => {
            let mut applied = prepared
                .apply_eula(&approval)
                .expect("v1_apply_before_spawn_marker");
            let result = applied.mark_may_have_spawned();
            drop(applied);
            result
        }
        _ => prepared.apply_eula(&approval).map(|_| ()),
    };
    assert_eq!(interrupted, Err(Error::Injected));
    // Every preparation/transaction/recipe owner above has dropped normally.
    unlocked(&paths);
    let expected = prefix.expected();
    let captured = CapturedPrefix {
        prefix,
        id,
        root: fixture.root.path().into(),
        root_identity: directory_identity(fixture.root.path()).expect("v1_capture_root_id"),
        canonical_root: fixture
            .root
            .path()
            .canonicalize()
            .expect("v1_capture_root_canonical"),
        lab: fixture.lab.clone(),
        stage: fixture.stage.clone(),
        jdk: fixture.jdk.clone(),
        bundle: paths.bundle.clone(),
        copy: paths.copy.clone(),
        origin: paths.origin.clone(),
        transaction: paths.transaction.clone(),
        original_eula,
        preimages: Preimages {
            factory,
            factory_digest,
            approved_files,
            eula,
            eula_digest,
            recipe,
            execution,
            execution_review_id,
            execution_digest,
        },
        disk: disk_files(&paths),
        root_tree: tree_snapshot(fixture.root.path()),
        origin_tree: tree_snapshot(&paths.origin),
        stage_tree: tree_snapshot(&fixture.stage),
        jdk_tree: tree_snapshot(&fixture.jdk),
        expected_outcome: expected.0,
        expected_renames: expected.1,
    };
    verify_preimages(&captured);
    captured
}

fn verify_preimages(captured: &CapturedPrefix) {
    let manifest = captured.original(Place::Manifest).bytes();
    let claim = captured.original(Place::Claim).bytes();
    assert_eq!(
        factory::roundtrip_manifest_for_test(manifest).unwrap(),
        manifest
    );
    assert_eq!(eula::roundtrip_claim_for_test(claim).unwrap(), claim);
    let old = &captured.preimages;
    assert_eq!(
        eula::roundtrip_approved_for_test(old.approved_files.bytes()).unwrap(),
        old.approved_files.bytes()
    );
    assert_eq!(
        factory::replay_factory_input_for_test(
            &captured.lab,
            &captured.stage,
            &captured.jdk,
            captured.id,
            old.factory.bytes(),
        )
        .expect("v1_factory_same_uuid_input"),
        old.factory.bytes()
    );
    assert_eq!(
        eula::replay_eula_input_for_test(manifest, old.approved_files.bytes(), old.eula.bytes())
            .expect("v1_eula_same_uuid_input"),
        old.eula.bytes()
    );
    let (recipe, execution, review_id) = producer::replay_status_input_for_test(
        &captured.copy,
        &captured.jdk,
        manifest,
        old.execution.bytes(),
    )
    .expect("v1_status_same_uuid_input");
    assert_eq!(recipe, old.recipe.bytes());
    assert_eq!(execution, old.execution.bytes());
    assert_eq!(review_id, old.execution_review_id);
    assert_eq!(digest(old.factory.bytes()), old.factory_digest);
    assert_eq!(digest(old.eula.bytes()), old.eula_digest);
    assert_eq!(digest(old.execution.bytes()), old.execution_digest);
    let decoded: GoldenClaim = serde_json::from_slice(claim).expect("v1_claim_original_schema");
    assert_eq!(decoded.schema, 1);
    assert_eq!(decoded.bundle, captured.id);
    assert_eq!(decoded.original, captured.original_eula);
    assert_eq!(decoded.manifest_sha256, digest(manifest));
    assert_eq!(decoded.review_digest, old.eula_digest);
}

#[test]
#[ignore = "explicit B0 capture before schema2; creates13 retained own roots and one CreateNew private index"]
fn capture_original_v1_prefixes_before_schema2() {
    let index_path = index_path();
    absent(&index_path).expect("v1_capture_index_must_be_new");
    let mut fixtures = Vec::with_capacity(PREFIXES.len());
    let mut prefixes = Vec::with_capacity(PREFIXES.len());
    for prefix in PREFIXES {
        let fixture = Fixture::new();
        prefixes.push(capture_prefix(&fixture, prefix));
        fixtures.push(fixture);
    }
    let index = CaptureIndex {
        schema: 1,
        event: INDEX_EVENT.into(),
        prefixes,
    };
    let bytes = serde_json::to_vec(&index).expect("v1_capture_index_encode");
    assert!(bytes.len() as u64 <= INDEX_LIMIT, "v1_capture_index_limit");
    recovery::write_new(&index_path, &bytes).expect("v1_capture_index_create_new_sync");
    assert_eq!(
        recovery::read_bounded(&index_path, INDEX_LIMIT).unwrap(),
        bytes
    );
    // A boolean cleanup disposition retains only disk data. Drop still closes
    // each root's pins; no mem::forget, leaked guard, or global temp setting.
    for mut fixture in fixtures {
        fixture.root.preserve_on_drop();
        drop(fixture);
    }
    for captured in &index.prefixes {
        unlocked(&captured.paths());
        captured.validate_root();
    }
    println!(
        "native_v1_prefix_capture: count=13 sha256={}",
        digest(&bytes)
    );
}

fn verify_unchanged(captured: &CapturedPrefix) {
    captured.validate_root();
    assert_eq!(captured.disk.len(), PLACES.len());
    let actual_files = disk_files(&captured.paths());
    for ((expected_place, expected), actual) in PLACES.iter().zip(&captured.disk).zip(actual_files)
    {
        assert_eq!(*expected_place, expected.place);
        assert_eq!(actual.place, *expected_place);
        assert_eq!(actual.observed, expected.observed, "v1_same_file_identity");
        assert_eq!(
            actual.original.as_ref().map(OriginalBytes::bytes),
            expected.original.as_ref().map(OriginalBytes::bytes),
            "v1_same_file_bytes"
        );
    }
    assert_eq!(tree_snapshot(&captured.root), captured.root_tree);
    verify_preimages(captured);
}

#[test]
#[ignore = "explicit after-change verification of the same13 captured physical roots; hash/index required"]
fn verify_original_v1_prefixes_after_schema2() {
    let path = index_path();
    let expected_hash = std::env::var(INDEX_HASH_ENV).expect("explicit_v1_capture_sha256_required");
    assert!(expected_hash.len() == 64 && expected_hash.bytes().all(|b| b.is_ascii_hexdigit()));
    let raw = recovery::read_bounded(&path, INDEX_LIMIT).expect("v1_original_capture_index");
    assert_eq!(digest(&raw), expected_hash, "v1_index_original_hash");
    let index: CaptureIndex = serde_json::from_slice(&raw).expect("v1_closed_capture_index");
    assert_eq!(index.schema, 1);
    assert_eq!(index.event, INDEX_EVENT);
    assert_eq!(index.prefixes.len(), PREFIXES.len());
    let roots: BTreeSet<_> = index.prefixes.iter().map(|case| &case.root).collect();
    assert_eq!(roots.len(), PREFIXES.len(), "v1_distinct_original_roots");
    // Validate every old fixture before mutating the first. A mismatch leaves
    // all roots intact; no old expected bytes are regenerated or rewritten.
    for (prefix, captured) in PREFIXES.iter().zip(&index.prefixes) {
        assert_eq!(*prefix, captured.prefix);
        assert_eq!(
            captured.prefix.expected(),
            (captured.expected_outcome, captured.expected_renames)
        );
        verify_unchanged(captured);
    }
    for captured in &index.prefixes {
        let paths = captured.paths();
        let probe = probe::Probe::default();
        let hooks = Hooks::default();
        let plan = fixture_recovery(&captured.lab, captured.id, probe.boxed(), hooks.clone());
        if let Some(expected) = captured.expected_outcome {
            let plan = plan.expect("v1_prefix_reopened_with_original_guard");
            locked(&paths);
            // A fresh recovery review deliberately gets a fresh UUID/digest.
            // Only the old factory/EULA/execution preimages require old UUIDs.
            let approval = plan.review().digest.clone();
            assert_eq!(
                plan.confirm(&approval).expect("v1_fresh_guarded_recovery"),
                expected
            );
            assert_eq!(
                observe_file(&target(&paths), 4096)
                    .expect("v1_original_target_restored")
                    .0,
                captured.original_eula
            );
            assert_eq!(
                bytes(&slot(&paths, "claim.json")),
                captured.original(Place::Claim).bytes()
            );
        } else {
            assert_eq!(plan.err(), Some(Error::NeedsRecovery));
            assert_eq!(tree_snapshot(&captured.root), captured.root_tree);
        }
        assert!(probe.calls() >= 2, "v1_recovery_real_guard_probe_alive");
        assert_eq!(
            hooks.eula_renames.load(Ordering::SeqCst),
            captured.expected_renames
        );
        unlocked(&paths);
        assert_eq!(tree_snapshot(&captured.origin), captured.origin_tree);
        assert_eq!(tree_snapshot(&captured.stage), captured.stage_tree);
        assert_eq!(tree_snapshot(&captured.jdk), captured.jdk_tree);
    }
    // Cleanup is deferred until every compatibility/recovery oracle succeeds.
    // Each generated root is pinned, reidentified and confined before removal.
    for captured in &index.prefixes {
        let pins = recovery::pin_directory(&captured.root).expect("v1_cleanup_root_pin");
        captured.validate_root();
        unlocked(&captured.paths());
        drop(pins);
        fs::remove_dir_all(&captured.root).expect("v1_cleanup_exact_own_root");
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenClaim {
    schema: u32,
    bundle: BundleId,
    transaction: Uuid,
    manifest_sha256: String,
    review_digest: String,
    original: Observation,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GoldenIndex {
    copied_without_reencode: bool,
    schema: u32,
    event: String,
    source_unmodified: bool,
    corrected84_c_uncommitted: bool,
    source_head: String,
    manifest_hash_matches_original_claim: bool,
    utc: String,
    source_bundle: String,
    files: Vec<GoldenFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenFile {
    bytes: u64,
    name: String,
    sha256: String,
}
const GOLDEN_FILES: [(&str, u64, &str); 10] = [
    (
        "bundle.json",
        6246,
        "518fb67722609789863001996fbb7f3efe54894e9e37789f95e94045ef9ec433",
    ),
    (
        "eula-transaction/claim.json",
        440,
        "5024346bb7f6fb537fdfcbb1878240821093eb07da00049af8e266579294538e",
    ),
    (
        "eula-transaction/intent.json",
        380,
        "47e91ef698236a5292f5226d46bc049510f6861e596e6afdedbced78c76470b7",
    ),
    (
        "eula-transaction/original-moved.json",
        89,
        "40a10c07ddb38ad5fbe3edf3cc8bab6414486e6f7d8a9ede28add169562f277f",
    ),
    (
        "eula-transaction/applied.json",
        83,
        "ae18bb5b3d5738dc0bcc054e85d81cce767dfffbb8efb12276c0178ac78aa07e",
    ),
    (
        "eula-transaction/may-have-spawned.json",
        90,
        "c96f716ba5e06a9ea50dc770e96c239600efa34090e4d201cc0c91a8ff8991ba",
    ),
    (
        "eula-transaction/restore-intent.json",
        82,
        "833f2c02147c5bbf3b77014160b2e122b0d56a64b4683243e3360fb61c29847c",
    ),
    (
        "eula-transaction/restored.json",
        86,
        "5a4dd11577deabebfd091f368a66c6b20d5bdfae2f50007739fb3f6e9987aa70",
    ),
    (
        "eula-transaction/snapshot.bin",
        50,
        "588ecf8a4195b594a7e5c95da52bc6c0ee84126c1fe87321d4bee2b0902bd8a8",
    ),
    (
        "eula-transaction/applied.bin",
        49,
        "f25bb6d91d9ced0c6cf0053db54834b1ab200ab2ce3a19b69db212949650c3b5",
    ),
];

#[test]
#[ignore = "explicit standalone old persisted golden roundtrip; run identically before and after schema2"]
fn old_persisted_v1_goldens_roundtrip_without_rewriting() {
    let directory = validation_directory().join(GOLDEN_DIRECTORY);
    let original_tree = tree_snapshot(&directory);
    let index: GoldenIndex = serde_json::from_slice(
        &recovery::read_bounded(&directory.join("capture.json"), MAX_JOURNAL).unwrap(),
    )
    .expect("v1_old_golden_capture_metadata");
    assert_eq!(index.schema, 1);
    assert_eq!(index.event, "persisted_v1_golden_captured_before_v2");
    assert!(
        index.copied_without_reencode && index.source_unmodified && index.corrected84_c_uncommitted
    );
    assert!(index.manifest_hash_matches_original_claim);
    // Historical capture chronology, not a claim about today's checkout HEAD.
    assert_eq!(
        index.source_head,
        "420494f2abd79cfc451925fbfae076247c80688f"
    );
    assert_eq!(index.source_bundle, "0fcc01c6-0e40-4053-9ab9-cb8865f9db74");
    assert_eq!(index.utc, "2026-10-21T18:33:27.7640687+00:00");
    assert_eq!(index.files.len(), GOLDEN_FILES.len());
    for ((name, count, hash), recorded) in GOLDEN_FILES.iter().zip(&index.files) {
        assert_eq!(recorded.name, *name);
        assert_eq!(recorded.bytes, *count);
        assert_eq!(recorded.sha256, *hash);
        let bytes = recovery::read_bounded(&directory.join(name), MAX_JOURNAL).unwrap();
        assert_eq!(bytes.len() as u64, *count);
        assert_eq!(digest(&bytes), *hash);
        match *name {
            "bundle.json" => {
                assert_eq!(factory::roundtrip_manifest_for_test(&bytes).unwrap(), bytes)
            }
            "eula-transaction/claim.json" => {
                assert_eq!(eula::roundtrip_claim_for_test(&bytes).unwrap(), bytes);
                let claim: GoldenClaim = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(claim.schema, 1);
                assert_eq!(claim.bundle.0.to_string(), index.source_bundle);
                assert_eq!(
                    claim.transaction.to_string(),
                    "b8ea565a-0211-470b-ba5e-05eda02c8d85"
                );
                assert_eq!(claim.manifest_sha256, GOLDEN_FILES[0].2);
                assert_eq!(
                    claim.review_digest,
                    "cb1b55a442f15581ff3b5dc12c43e3dd5b1dbfa95569a24ef406077987e46ddf"
                );
                assert_eq!(claim.original.bytes, EULA_FALSE.len() as u64);
                assert_eq!(claim.original.sha256, digest(EULA_FALSE));
            }
            _ => {}
        }
    }
    assert_eq!(
        tree_snapshot(&directory),
        original_tree,
        "v1_old_goldens_untouched"
    );
}
