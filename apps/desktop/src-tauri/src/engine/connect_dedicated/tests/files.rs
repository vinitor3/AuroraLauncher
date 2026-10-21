//! Independent filesystem observations for the disposable EULA fixtures.
//! Nothing here constructs a production bundle or substitutes provenance.

use crate::engine::recovery;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub(in super::super) struct TestRoot {
    path: PathBuf,
    parent: PathBuf,
    canonical_path: PathBuf,
    pins: Option<Vec<fs::File>>,
    retain: bool,
}

impl TestRoot {
    pub(in super::super) fn new() -> Self {
        // Production rejects device/UNC syntax before access. Keep the ordinary
        // absolute input spelling; canonical Windows paths often start with \\?\.
        let temporary = std::env::temp_dir();
        assert!(temporary.is_absolute(), "fixture_temp_absolute");
        let parent = temporary.canonicalize().expect("fixture_temp");
        let path = temporary.join(format!("aurora-dedicated-eula-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).expect("fixture_root");
        let canonical_path = path.canonicalize().expect("fixture_root_identity");
        // The existing Windows primitive denies directory replacement while
        // allowing the fixture to mutate its separately verified descendants.
        let pins = recovery::pin_directory(&path).expect("fixture_root_pin");
        Self {
            path,
            parent,
            canonical_path,
            pins: Some(pins),
            retain: false,
        }
    }

    pub(in super::super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn preserve_on_drop(&mut self) {
        // Retain disk data only. The normal Drop path still validates this
        // generated directory and closes all pins; it never leaks ownership.
        self.retain = true;
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        // Keep failed fixtures intact for diagnosis. Check the exact generated
        // directory while its original Windows deny-delete handles remain held.
        if std::thread::panicking() {
            drop(self.pins.take());
            return;
        }
        let Ok(metadata) = fs::symlink_metadata(&self.path) else {
            drop(self.pins.take());
            return;
        };
        if is_link(&metadata)
            || !metadata.is_dir()
            || self.canonical_path.parent() != Some(self.parent.as_path())
            || !self
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("aurora-dedicated-eula-test-"))
            || self.path.canonicalize().ok().as_deref() != Some(self.canonical_path.as_path())
        {
            panic!("fixture_cleanup_identity");
        }
        drop(self.pins.take());
        if !self.retain {
            fs::remove_dir_all(&self.path).expect("fixture_cleanup");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in super::super) enum Entry {
    Directory,
    File { bytes: u64, sha256: String },
    Link,
}

pub(in super::super) type Tree = BTreeMap<String, Entry>;

pub(in super::super) fn tree(root: &Path) -> Tree {
    fn walk(root: &Path, path: &Path, depth: usize, total: &mut u64, result: &mut Tree) {
        assert!(depth <= 32 && result.len() < 4096, "fixture_tree_limit");
        let metadata = fs::symlink_metadata(path).expect("fixture_metadata");
        let relative = path
            .strip_prefix(root)
            .expect("fixture_relative")
            .to_str()
            .expect("fixture_utf8")
            .replace('\\', "/");
        if is_link(&metadata) {
            result.insert(relative, Entry::Link);
        } else if metadata.is_dir() {
            result.insert(relative, Entry::Directory);
            for entry in fs::read_dir(path).expect("fixture_entries") {
                walk(
                    root,
                    &entry.expect("fixture_entry").path(),
                    depth + 1,
                    total,
                    result,
                );
            }
        } else {
            assert!(metadata.is_file(), "fixture_regular_file");
            *total = total.checked_add(metadata.len()).expect("fixture_bytes");
            assert!(*total <= 16 * 1024 * 1024, "fixture_bytes_limit");
            // Windows byte-range locks may cover the first byte beyond EOF.
            // fs::read probes that byte even for an empty lock file, yielding
            // ERROR_LOCK_VIOLATION. An observed empty file still contributes its
            // path, zero length and SHA-256; do not omit it or any database.
            let bytes = if metadata.len() == 0 {
                assert_eq!(
                    fs::metadata(path).expect("fixture_empty_metadata").len(),
                    0,
                    "fixture_empty_file_changed"
                );
                Vec::new()
            } else {
                fs::read(path).expect("fixture_read")
            };
            assert_eq!(bytes.len() as u64, metadata.len(), "fixture_read_size");
            result.insert(
                relative,
                Entry::File {
                    bytes: metadata.len(),
                    sha256: format!("{:x}", Sha256::digest(bytes)),
                },
            );
        }
    }
    let mut result = Tree::new();
    let mut total = 0;
    walk(root, root, 0, &mut total, &mut result);
    result
}

#[track_caller]
pub(in super::super) fn assert_tree(root: &Path, expected: &Tree) {
    let actual = tree(root);
    if &actual == expected {
        return;
    }
    let changed: std::collections::BTreeSet<_> = actual
        .keys()
        .chain(expected.keys())
        .filter(|path| actual.get(*path) != expected.get(*path))
        .collect();
    let details: Vec<_> = changed
        .iter()
        .take(8)
        .map(|path| (path.as_str(), expected.get(*path), actual.get(*path)))
        .collect();
    panic!(
        "fixture_tree_changed: {} differing paths; first eight (path, before, after): {details:?}",
        changed.len()
    );
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
