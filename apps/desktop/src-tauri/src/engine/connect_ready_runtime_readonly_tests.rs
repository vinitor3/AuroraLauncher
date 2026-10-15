//! Additional readiness regression cases. All files live in the parent fixture
//! root; no Java, network or personal configuration is accessed.
use super::*;
use std::{cell::Cell, collections::BTreeMap};

fn tree(root: &Path) -> BTreeMap<String, (bool, u64, String)> {
    fn visit(base: &Path, current: &Path, files: &mut BTreeMap<String, (bool, u64, String)>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            assert!(!metadata.file_type().is_symlink());
            let relative = path
                .strip_prefix(base)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/");
            if metadata.is_dir() {
                files.insert(relative, (true, 0, String::new()));
                visit(base, &path, files);
            } else if relative.starts_with(".aurora/instance-locks/")
                && relative.ends_with(".lock")
                && relative.matches('/').count() == 2
            {
                // The caller deliberately holds a Windows byte-range lock.
                // Record this exact lock directory's names and lengths only;
                // its locked contents cannot be read and are not claimed as
                // part of the byte-equality proof. Every other file, including
                // recovery's index and configuration, is read and hashed.
                files.insert(
                    relative,
                    (
                        false,
                        metadata.len(),
                        "owned-os-lock-content-not-read".into(),
                    ),
                );
            } else {
                let bytes = fs::read(&path).unwrap();
                files.insert(
                    relative,
                    (
                        false,
                        bytes.len() as u64,
                        format!("{:x}", Sha256::digest(bytes)),
                    ),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn ready_runtime_inspection_preserves_entire_copy_origin_index_and_java_tree() {
    let f = Fixture::new();
    // Lock acquisition is part of the caller's existing lifecycle. Snapshot
    // after it so this case isolates the supposedly read-only inspector.
    let guard = acquire_instance_mutation_lock(&f.copy).unwrap();
    let before = tree(&f.root);
    let proof = inspect_ready_runtime(
        &f.copy,
        &guard,
        "source-fixture",
        &f.java,
        None,
        "127.0.0.1:42010".parse().unwrap(),
        21,
        || false,
    )
    .unwrap();
    assert_eq!(
        tree(&f.root),
        before,
        "inspection must preserve every fixture file and directory; OS lock contents are not inspected"
    );
    drop(proof);
    assert_eq!(
        tree(&f.root),
        before,
        "releasing proof must not repair or mutate the copy"
    );
}

#[test]
fn ready_runtime_lookalike_copy_is_refused_without_initializing_identity_or_provenance() {
    let f = Fixture::new();
    let fake = InstanceLayout::new(&f.root)
        .open(InstanceId::parse(format!("aurora-test-{}", uuid::Uuid::new_v4())).unwrap());
    fake.ensure_layout().unwrap();
    let guard = acquire_instance_mutation_lock(&fake).unwrap();
    let before = tree(&f.root);
    assert!(matches!(
        inspect_ready_runtime(
            &fake,
            &guard,
            "source-fixture",
            &f.java,
            None,
            "127.0.0.1:42010".parse().unwrap(),
            21,
            || false
        ),
        Err(Error::InstanceNotReady)
    ));
    assert_eq!(tree(&f.root), before);
    assert!(!fake.root().join(".aurora/recovery-identity.json").exists());
}

#[test]
fn ready_runtime_cancellation_during_java_hash_preserves_cancelled_and_releases_pins() {
    let f = Fixture::new();
    let modules = f.root.join("java/lib/modules");
    assert!(fs::OpenOptions::new().write(true).open(&modules).is_ok());
    let guard = acquire_instance_mutation_lock(&f.copy).unwrap();
    let before = tree(&f.root);
    let observed_java_pin = Cell::new(false);
    let result = inspect_ready_runtime(
        &f.copy,
        &guard,
        "source-fixture",
        &f.java,
        None,
        "127.0.0.1:42010".parse().unwrap(),
        21,
        || {
            // Causal Windows observation: this file is writable until the
            // inspector opens its deny-write pin. Never write or change it.
            if fs::OpenOptions::new().write(true).open(&modules).is_err() {
                observed_java_pin.set(true);
                true
            } else {
                false
            }
        },
    );
    assert!(
        observed_java_pin.get(),
        "cancellation must occur inside the Java inspection, not before it"
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(
        fs::OpenOptions::new().write(true).open(&modules).is_ok(),
        "failed inspection must release its Java pin"
    );
    assert!(
        fs::OpenOptions::new()
            .write(true)
            .open(f.copy.root().join("mods/aurora-core-fabric-1.21.1.jar"))
            .is_ok(),
        "earlier pins must also be released"
    );
    assert_eq!(tree(&f.root), before);
}

#[test]
fn ready_runtime_declared_selected_profile_cannot_silently_switch_to_another_installed_version() {
    let f = Fixture::new();
    let profile = f.copy.root().join("aurora-instance.json");
    let original = fs::read(&profile).unwrap();
    for value in [
        json!({"minecraft":"1.20.1","versionId":"fabric-loader-0.16.14-1.21.1"}),
        json!({"minecraft":"1.21.1","versionId":"1.21.1"}),
        json!({"minecraft":"1.21.1","versionId":"fabric-loader-0.16.10-1.21.1"}),
        json!({}),
        json!({"minecraft":"1.21.1"}),
    ] {
        fs::write(&profile, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            matches!(
                f.inspect(&f.copy, "source-fixture"),
                Err(Error::UnsupportedRuntime)
            ),
            "different selected metadata cannot authorize this fixed profile"
        );
    }
    fs::remove_file(&profile).unwrap();
    assert!(
        matches!(
            f.inspect(&f.copy, "source-fixture"),
            Err(Error::UnsupportedRuntime)
        ),
        "an installed matching child is insufficient without declared selected metadata"
    );
    fs::write(&profile, original).unwrap();
    assert!(f.inspect(&f.copy, "source-fixture").is_ok());
}
