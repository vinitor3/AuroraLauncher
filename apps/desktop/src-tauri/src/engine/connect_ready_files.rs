//! Read-only verification of the managed pair for the narrow Connect launch.
//!
//! This does not grant a launch or repair the copy. The caller must hold the
//! normal instance mutation guard and revalidate the complete result at commit.
use std::{
    collections::BTreeSet,
    fs,
    io::{Cursor, Read},
};

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use super::{core, recovery, skins, Instance};

pub const READY_CORE_FILE: &str = "mods/aurora-core-fabric-1.21.1.jar";
pub const READY_SKINS_FILE: &str = "mods/aurora-skins-fabric-1.21.1.jar";
const MAX_MODS: usize = 256;
const MAX_JAR_BYTES: u64 = 128 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 16_384;
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_NESTED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_NESTED_COUNT: usize = 64;
const MAX_NESTED_DEPTH: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadyFilesError {
    Cancelled,
    Missing,
    Unsafe,
    Limit,
    InvalidJar,
    Mismatch,
    Conflict,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReadyFileFingerprint {
    pub relative: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReadyManagedFiles {
    pub core_sha256: String,
    pub skins_sha256: String,
    pub fingerprints: Vec<ReadyFileFingerprint>,
}

fn check_cancel(cancelled: &impl Fn() -> bool) -> Result<(), ReadyFilesError> {
    if cancelled() {
        Err(ReadyFilesError::Cancelled)
    } else {
        Ok(())
    }
}

/// Verify the exact signed Core and embedded Skins pair and pin every active
/// top-level mod. No dependency/install helpers, extraction or write is used.
pub fn verify_managed_files(
    instance: &Instance,
    cancelled: impl Fn() -> bool,
) -> Result<ReadyManagedFiles, ReadyFilesError> {
    check_cancel(&cancelled)?;
    let expected_core =
        core::verified_embedded_core("1.21.1", "fabric").map_err(|_| ReadyFilesError::Mismatch)?;
    let expected_skins = skins::embedded_skins_artifact("1.21.1", "fabric")
        .map_err(|_| ReadyFilesError::Mismatch)?;
    let mods = recovery::path(instance.root(), "mods").map_err(|_| ReadyFilesError::Unsafe)?;
    let _pins = recovery::pin_directory(&mods).map_err(|_| ReadyFilesError::Unsafe)?;
    let entries = fs::read_dir(&mods).map_err(|_| ReadyFilesError::Missing)?;
    let mut paths = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, entry) in entries.enumerate() {
        check_cancel(&cancelled)?;
        if index >= MAX_MODS {
            return Err(ReadyFilesError::Limit);
        }
        let entry = entry.map_err(|_| ReadyFilesError::Unsafe)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| ReadyFilesError::Unsafe)?;
        let relative = format!("mods/{name}");
        let path =
            recovery::path(instance.root(), &relative).map_err(|_| ReadyFilesError::Unsafe)?;
        let metadata = fs::symlink_metadata(&path).map_err(|_| ReadyFilesError::Unsafe)?;
        // This action intentionally does not infer Fabric's recursive directory
        // discovery rules. A subdirectory makes the ready proof incomplete.
        if !metadata.is_file() {
            return Err(ReadyFilesError::Unsafe);
        }
        if !name.to_ascii_lowercase().ends_with(".jar") {
            continue;
        }
        if !seen.insert(relative.to_ascii_lowercase()) {
            return Err(ReadyFilesError::Conflict);
        }
        paths.push((relative, path));
    }
    paths.sort_by(|left, right| left.0.cmp(&right.0));
    let mut total = 0_u64;
    let mut nested_count = 0;
    let mut core_hash = None;
    let mut skins_hash = None;
    let mut fingerprints = Vec::new();
    for (relative, path) in paths {
        check_cancel(&cancelled)?;
        let bytes =
            recovery::read_bounded(&path, MAX_JAR_BYTES).map_err(|_| ReadyFilesError::Unsafe)?;
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or(ReadyFilesError::Limit)?;
        if total > MAX_TOTAL_BYTES {
            return Err(ReadyFilesError::Limit);
        }
        check_cancel(&cancelled)?;
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        if relative == READY_CORE_FILE {
            if bytes != expected_core {
                return Err(ReadyFilesError::Mismatch);
            }
            core_hash = Some(sha256.clone());
        } else if relative == READY_SKINS_FILE {
            if bytes != expected_skins {
                return Err(ReadyFilesError::Mismatch);
            }
            skins_hash = Some(sha256.clone());
        } else {
            inspect_extra_jar(&bytes, 0, &mut nested_count, &mut total, &cancelled)?;
        }
        fingerprints.push(ReadyFileFingerprint {
            relative,
            sha256,
            bytes: bytes.len() as u64,
        });
    }
    check_cancel(&cancelled)?;
    Ok(ReadyManagedFiles {
        core_sha256: core_hash.ok_or(ReadyFilesError::Missing)?,
        skins_sha256: skins_hash.ok_or(ReadyFilesError::Missing)?,
        fingerprints,
    })
}

fn protected_identity(value: &str) -> bool {
    matches!(value, "aurora_core" | "aurora_skins" | "aurora_companion")
}

fn inspect_extra_jar(
    bytes: &[u8],
    depth: usize,
    nested_count: &mut usize,
    total: &mut u64,
    cancelled: &impl Fn() -> bool,
) -> Result<(), ReadyFilesError> {
    check_cancel(cancelled)?;
    if depth > MAX_NESTED_DEPTH {
        return Err(ReadyFilesError::Limit);
    }
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|_| ReadyFilesError::InvalidJar)?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(ReadyFilesError::Limit);
    }
    let mut names = BTreeSet::new();
    for index in 0..archive.len() {
        check_cancel(cancelled)?;
        let entry = archive
            .by_index(index)
            .map_err(|_| ReadyFilesError::InvalidJar)?;
        let name = entry.name().replace('\\', "/");
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(ReadyFilesError::InvalidJar);
        }
        if [
            "com/aurora/core/",
            "com/aurora/skins/",
            "com/aurora/companion/",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix) && name.ends_with(".class"))
        {
            return Err(ReadyFilesError::Conflict);
        }
    }
    let metadata = match archive.by_name("fabric.mod.json") {
        Ok(entry) => Some(read_zip_member(entry, MAX_METADATA_BYTES)?),
        Err(zip::result::ZipError::FileNotFound) => None,
        Err(_) => return Err(ReadyFilesError::InvalidJar),
    };
    let Some(metadata) = metadata else {
        return Ok(());
    };
    let metadata: Value =
        serde_json::from_slice(&metadata).map_err(|_| ReadyFilesError::InvalidJar)?;
    let id = metadata["id"].as_str().ok_or(ReadyFilesError::InvalidJar)?;
    if protected_identity(id) {
        return Err(ReadyFilesError::Conflict);
    }
    if let Some(provides) = metadata.get("provides") {
        for alias in provides.as_array().ok_or(ReadyFilesError::InvalidJar)? {
            if protected_identity(alias.as_str().ok_or(ReadyFilesError::InvalidJar)?) {
                return Err(ReadyFilesError::Conflict);
            }
        }
    }
    if let Some(jars) = metadata.get("jars") {
        for jar in jars.as_array().ok_or(ReadyFilesError::InvalidJar)? {
            check_cancel(cancelled)?;
            *nested_count += 1;
            if *nested_count > MAX_NESTED_COUNT {
                return Err(ReadyFilesError::Limit);
            }
            let name = jar["file"].as_str().ok_or(ReadyFilesError::InvalidJar)?;
            recovery::valid_relative(name).map_err(|_| ReadyFilesError::InvalidJar)?;
            let nested = read_zip_member(
                archive
                    .by_name(name)
                    .map_err(|_| ReadyFilesError::InvalidJar)?,
                MAX_NESTED_BYTES,
            )?;
            *total = total
                .checked_add(nested.len() as u64)
                .ok_or(ReadyFilesError::Limit)?;
            if *total > MAX_TOTAL_BYTES {
                return Err(ReadyFilesError::Limit);
            }
            inspect_extra_jar(&nested, depth + 1, nested_count, total, cancelled)?;
        }
    }
    Ok(())
}

fn read_zip_member(mut entry: impl Read, max: u64) -> Result<Vec<u8>, ReadyFilesError> {
    let mut bytes = Vec::new();
    (&mut entry)
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ReadyFilesError::InvalidJar)?;
    if bytes.len() as u64 > max {
        return Err(ReadyFilesError::Limit);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{
        install_embedded_core, install_embedded_skins, InstanceId, InstanceLayout,
    };
    use std::{cell::Cell, io::Write, path::PathBuf};
    use zip::write::SimpleFileOptions;

    struct Fixture {
        root: PathBuf,
        instance: Instance,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("aurora-ready-files-{}", uuid::Uuid::new_v4()));
            let instance =
                InstanceLayout::new(&root).open(InstanceId::parse("ready-files").unwrap());
            install_embedded_core(&instance, "1.21.1", "fabric").unwrap();
            install_embedded_skins(&instance, "1.21.1", "fabric").unwrap();
            Self { root, instance }
        }
        fn add(&self, name: &str, data: &[u8]) {
            fs::write(self.instance.root().join("mods").join(name), data).unwrap();
        }
        fn read(&self, relative: &str) -> Vec<u8> {
            fs::read(self.instance.root().join(relative)).unwrap()
        }
        fn verify(&self) -> Result<ReadyManagedFiles, ReadyFilesError> {
            verify_managed_files(&self.instance, || false)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn jar(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in files {
            output
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            output.write_all(bytes).unwrap();
        }
        output.finish().unwrap().into_inner()
    }

    #[test]
    fn connect_ready_files_verifies_exact_pair_readonly_and_pins_extra_mods() {
        let fixture = Fixture::new();
        let core = fixture.read(READY_CORE_FILE);
        let skins = fixture.read(READY_SKINS_FILE);
        let first = fixture.verify().unwrap();
        assert_eq!(first.fingerprints.len(), 2);
        let extra = jar(&[(
            "fabric.mod.json",
            br#"{"id":"voicechat","depends":{"aurora_core":"*"}}"#,
        )]);
        fixture.add("third-party.jar", &extra);
        let second = fixture.verify().unwrap();
        assert_eq!(second.fingerprints.len(), 3);
        assert_eq!(first.core_sha256, second.core_sha256);
        assert_eq!(first.skins_sha256, second.skins_sha256);
        assert_eq!(fixture.read(READY_CORE_FILE), core);
        assert_eq!(fixture.read(READY_SKINS_FILE), skins);
        assert_eq!(fixture.read("mods/third-party.jar"), extra);
        assert_eq!(fixture.verify().unwrap(), second);
    }

    #[test]
    fn connect_ready_files_refuses_missing_tampered_and_renamed_duplicates() {
        let fixture = Fixture::new();
        for relative in [READY_CORE_FILE, READY_SKINS_FILE] {
            let path = fixture.instance.root().join(relative);
            let original = fs::read(&path).unwrap();
            fs::remove_file(&path).unwrap();
            assert_eq!(fixture.verify(), Err(ReadyFilesError::Missing));
            fs::write(&path, b"tampered").unwrap();
            assert_eq!(fixture.verify(), Err(ReadyFilesError::Mismatch));
            fs::write(&path, &original).unwrap();
            fixture.add("renamed.jar", &original);
            assert_eq!(fixture.verify(), Err(ReadyFilesError::Conflict));
            fs::remove_file(fixture.instance.root().join("mods/renamed.jar")).unwrap();
        }
    }

    #[test]
    fn connect_ready_files_rejects_protected_aliases_classes_and_nested_identity() {
        let fixture = Fixture::new();
        for identity in ["aurora_core", "aurora_skins", "aurora_companion"] {
            for metadata in [
                format!(r#"{{"id":"{identity}"}}"#),
                format!(r#"{{"id":"other","provides":["{identity}"]}}"#),
            ] {
                fixture.add(
                    "extra.jar",
                    &jar(&[("fabric.mod.json", metadata.as_bytes())]),
                );
                assert_eq!(fixture.verify(), Err(ReadyFilesError::Conflict));
            }
        }
        for prefix in ["core", "skins", "companion"] {
            let name = format!("com/aurora/{prefix}/Shadow.class");
            fixture.add("extra.jar", &jar(&[(&name, b"not executed")]));
            assert_eq!(fixture.verify(), Err(ReadyFilesError::Conflict));
        }
        let child = jar(&[("fabric.mod.json", br#"{"id":"aurora_core"}"#)]);
        fixture.add(
            "extra.jar",
            &jar(&[
                (
                    "fabric.mod.json",
                    br#"{"id":"other","jars":[{"file":"META-INF/jars/child.jar"}]}"#,
                ),
                ("META-INF/jars/child.jar", &child),
            ]),
        );
        assert_eq!(fixture.verify(), Err(ReadyFilesError::Conflict));
    }

    #[test]
    fn connect_ready_files_refuses_hardlinks_directories_and_malformed_jars() {
        let fixture = Fixture::new();
        let alias = fixture.root.join("hardlink.jar");
        fs::hard_link(fixture.instance.root().join(READY_CORE_FILE), &alias).unwrap();
        assert_eq!(fixture.verify(), Err(ReadyFilesError::Unsafe));
        fs::remove_file(alias).unwrap();
        let nested = fixture.instance.root().join("mods/nested");
        fs::create_dir(&nested).unwrap();
        assert_eq!(fixture.verify(), Err(ReadyFilesError::Unsafe));
        fs::remove_dir(nested).unwrap();
        fixture.add("extra.jar", b"not a zip");
        assert_eq!(fixture.verify(), Err(ReadyFilesError::InvalidJar));
        fixture.add("extra.jar", &jar(&[("fabric.mod.json", b"not json")]));
        assert_eq!(fixture.verify(), Err(ReadyFilesError::InvalidJar));
    }

    #[test]
    fn connect_ready_files_cancels_without_deleting_or_repairing() {
        let fixture = Fixture::new();
        let before = fixture.verify().unwrap();
        let observed = Cell::new(0);
        assert_eq!(
            verify_managed_files(&fixture.instance, || {
                observed.set(observed.get() + 1);
                observed.get() > 5
            }),
            Err(ReadyFilesError::Cancelled)
        );
        assert_eq!(fixture.verify().unwrap(), before);
    }

    #[test]
    fn connect_ready_files_limits_metadata_and_nested_recursion() {
        let fixture = Fixture::new();
        fixture.add(
            "extra.jar",
            &jar(&[(
                "fabric.mod.json",
                &vec![b' '; MAX_METADATA_BYTES as usize + 1],
            )]),
        );
        assert_eq!(fixture.verify(), Err(ReadyFilesError::Limit));
        let mut nested = jar(&[("fabric.mod.json", br#"{"id":"leaf"}"#)]);
        for _ in 0..=MAX_NESTED_DEPTH {
            nested = jar(&[
                (
                    "fabric.mod.json",
                    br#"{"id":"other","jars":[{"file":"child.jar"}]}"#,
                ),
                ("child.jar", &nested),
            ]);
        }
        fixture.add("extra.jar", &nested);
        assert_eq!(fixture.verify(), Err(ReadyFilesError::Limit));
    }

    #[test]
    #[ignore = "requires explicitly provided pinned local voice artifacts; no Java or audio"]
    fn connect_ready_files_accepts_pinned_voice_archives_separately_without_execution() {
        let directory = PathBuf::from(
            std::env::var_os("AURORA_CONNECT_ARTIFACT_DIR")
                .expect("explicit local artifact directory required"),
        );
        let read_pinned = |name: &str, expected: &str| {
            let path = recovery::path(&directory, name).unwrap();
            let bytes = recovery::read_bounded(&path, MAX_JAR_BYTES).unwrap();
            assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected);
            bytes
        };
        let api = read_pinned(
            "fabric-api-0.116.10+1.21.1.jar",
            "af0eb792940568fdb1f37bb65a2599773bc47d2010aedf5ec5ed0bd3b6fc40e6",
        );
        for (name, expected) in [
            (
                "voicechat-fabric-1.21.1-2.6.22.jar",
                "972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36",
            ),
            (
                "plasmovoice-fabric-1.21.1-2.1.17.jar",
                "46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b",
            ),
        ] {
            let fixture = Fixture::new();
            let bytes = read_pinned(name, expected);
            fixture.add(name, &bytes);
            fixture.add("fabric-api.jar", &api);
            let result = fixture.verify().unwrap();
            assert_eq!(result.fingerprints.len(), 4);
            assert_eq!(fixture.read(&format!("mods/{name}")), bytes);
            assert_eq!(fixture.read("mods/fabric-api.jar"), api);
        }
    }
}
