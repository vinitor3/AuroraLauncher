use super::super::jna::{pin_natives, JnaTemporaryFiles};
use super::*;
use std::io::Write;

const JAR: &str = "libraries/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar";
const RESOURCE: &str = "com/sun/jna/win32-x86-64/jnidispatch.dll";
const DLL: &str = "jna13560871320815461909.dll";
const PAYLOAD: &[u8] = b"fixture native bytes, never executed";

fn jar(resource: &str) -> Vec<u8> {
    jar_payload(resource, PAYLOAD)
}

fn jar_payload(resource: &str, payload: &[u8]) -> Vec<u8> {
    let mut jar = zip::ZipWriter::new(Cursor::new(Vec::new()));
    jar.start_file(resource, zip::write::SimpleFileOptions::default())
        .unwrap();
    jar.write_all(payload).unwrap();
    jar.finish().unwrap().into_inner()
}

fn witness() -> JnaTemporaryFiles {
    let bytes = jar(RESOURCE);
    let mut jna = JnaTemporaryFiles::default();
    jna.observe_classpath(JAR, &bytes, &format!("{:x}", sha1::Sha1::digest(&bytes)))
        .unwrap();
    jna
}

fn reader() -> Reader<'static> {
    Reader {
        cancel: &|| false,
        pins: Vec::new(),
        hashes: Vec::new(),
        bytes: 0,
    }
}

fn pins() -> RuntimePins {
    RuntimePins {
        library_sha1: BTreeMap::new(),
        additional_libraries: BTreeMap::new(),
        extracted_natives: BTreeMap::from([(
            "lwjgl.dll".into(),
            format!("{:x}", sha1::Sha1::digest(b"never executed fixture")),
        )]),
        client_sha1: String::new(),
        asset_index_sha1: String::new(),
    }
}

fn pair(fixture: &Fixture, dll: &str, payload: &[u8], marker: &[u8]) {
    Fixture::write(fixture.copy.root(), &format!("natives/{dll}"), payload);
    Fixture::write(fixture.copy.root(), &format!("natives/{dll}.x"), marker);
}

fn inspect_natives(
    fixture: &Fixture,
    reader: &mut Reader<'_>,
    jna: &JnaTemporaryFiles,
) -> Result<(), Error> {
    pin_natives(
        reader,
        &fixture.copy,
        &directory_names(fixture.copy.root(), "natives")?,
        Some(&pins()),
        &BTreeMap::new(),
        jna,
    )
}

#[test]
fn ready_jna_witness_requires_verified_exact_classpath_jar_and_architecture() {
    let bytes = jar(RESOURCE);
    let hash = format!("{:x}", sha1::Sha1::digest(&bytes));
    assert_eq!(
        JnaTemporaryFiles::default().observe_classpath(JAR, &bytes, "wrong"),
        Err(Error::ArtifactMismatch)
    );
    let wrong_arch = jar("com/sun/jna/win32-x86/jnidispatch.dll");
    assert_eq!(
        JnaTemporaryFiles::default().observe_classpath(
            JAR,
            &wrong_arch,
            &format!("{:x}", sha1::Sha1::digest(&wrong_arch))
        ),
        Err(Error::ArtifactMismatch)
    );
    for path in [
        "libraries/net/java/dev/jna/jna/5.14.0/another.jar",
        "libraries/net/java/dev/jna/jna/../jna-...jar",
    ] {
        assert_eq!(
            JnaTemporaryFiles::default().observe_classpath(path, &bytes, &hash),
            Err(Error::ArtifactMismatch)
        );
    }
    let mut jna = JnaTemporaryFiles::default();
    assert_eq!(jna.observe_classpath(JAR, &bytes, &hash), Ok(()));
    assert_eq!(jna.observe_classpath(JAR, &bytes, &hash), Ok(()));
    let fixture = Fixture::new();
    pair(&fixture, DLL, PAYLOAD, b"");
    let mut unrelated = JnaTemporaryFiles::default();
    unrelated
        .observe_classpath("libraries/unknown/jna.jar", &bytes, &hash)
        .unwrap();
    assert_eq!(
        inspect_natives(&fixture, &mut reader(), &unrelated),
        Err(Error::ArtifactMismatch)
    );
    assert_eq!(
        inspect_natives(&fixture, &mut reader(), &JnaTemporaryFiles::default()),
        Err(Error::ArtifactMismatch)
    );
}

#[test]
fn ready_jna_multiple_official_classpath_versions_are_bounded_and_duplicate_resources_fail() {
    // Forge 1.19.2 retains both base JNA 5.10.0 and loader JNA 5.12.1.
    let fixture = Fixture::new();
    let mut jna = JnaTemporaryFiles::default();
    for (version, payload) in [
        ("5.10.0", PAYLOAD),
        ("5.12.1", b"other verified dispatch".as_slice()),
    ] {
        let bytes = jar_payload(RESOURCE, payload);
        let path = format!("libraries/net/java/dev/jna/jna/{version}/jna-{version}.jar");
        jna.observe_classpath(&path, &bytes, &format!("{:x}", sha1::Sha1::digest(&bytes)))
            .unwrap();
        pair(&fixture, DLL, payload, b"");
        inspect_natives(&fixture, &mut reader(), &jna).unwrap();
    }
    pair(&fixture, DLL, PAYLOAD, b"");
    inspect_natives(&fixture, &mut reader(), &jna).unwrap();
    let bytes = jar(RESOURCE);
    let hash = format!("{:x}", sha1::Sha1::digest(&bytes));
    for _ in 2..8 {
        jna.observe_classpath(JAR, &bytes, &hash).unwrap();
    }
    assert_eq!(
        jna.observe_classpath(JAR, &bytes, &hash),
        Err(Error::ArtifactMismatch)
    );

    let alias = "com/sun/jna/win32-x86-64/jnidispatch.dlL";
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for name in [RESOURCE, alias] {
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(PAYLOAD).unwrap();
    }
    let mut duplicate = archive.finish().unwrap().into_inner();
    let mut replaced = 0;
    for offset in 0..=duplicate.len() - alias.len() {
        if &duplicate[offset..offset + alias.len()] == alias.as_bytes() {
            duplicate[offset..offset + alias.len()].copy_from_slice(RESOURCE.as_bytes());
            replaced += 1;
        }
    }
    assert_eq!(replaced, 2);
    assert_eq!(
        JnaTemporaryFiles::default().observe_classpath(
            JAR,
            &duplicate,
            &format!("{:x}", sha1::Sha1::digest(&duplicate))
        ),
        Err(Error::ArtifactMismatch)
    );
}

#[test]
fn ready_jna_exact_pair_is_readonly_pinned_and_fingerprinted() {
    let fixture = Fixture::new();
    pair(&fixture, DLL, PAYLOAD, b"");
    let bytes = jar(RESOURCE);
    Fixture::write(fixture.copy.root(), JAR, &bytes);
    let mut first = reader();
    let pinned_jar = first
        .instance(&fixture.copy, JAR, 256 * 1024 * 1024)
        .unwrap();
    let mut jna = JnaTemporaryFiles::default();
    jna.observe_classpath(
        JAR,
        &pinned_jar,
        &format!("{:x}", sha1::Sha1::digest(&bytes)),
    )
    .unwrap();
    inspect_natives(&fixture, &mut first, &jna).unwrap();
    for (relative, expected) in [
        (JAR.to_owned(), bytes.as_slice()),
        (format!("natives/{DLL}"), PAYLOAD),
        (format!("natives/{DLL}.x"), b"".as_slice()),
    ] {
        let path = fixture.copy.root().join(&relative);
        assert_eq!(fs::read(&path).unwrap(), expected);
        assert!(first
            .hashes
            .iter()
            .any(|(name, len, hash)| name == &relative
                && *len == expected.len() as u64
                && hash == &format!("{:x}", Sha256::digest(expected))));
        #[cfg(windows)]
        {
            assert!(fs::OpenOptions::new().write(true).open(&path).is_err());
            assert!(fs::remove_file(&path).is_err());
        }
    }
    let mut second = reader();
    inspect_natives(&fixture, &mut second, &jna).unwrap();
    assert_eq!(&first.hashes[1..], second.hashes);
    drop(first);
    drop(second);
    // The JAR and temporary are inspected anew, never trusted across reviews.
    Fixture::write(
        fixture.copy.root(),
        &format!("natives/{DLL}"),
        b"changed native",
    );
    assert_eq!(
        inspect_natives(&fixture, &mut reader(), &jna),
        Err(Error::ArtifactMismatch)
    );
}

#[test]
fn ready_jna_rejects_invalid_names_markers_unpaired_and_arbitrary_files() {
    let fixture = Fixture::new();
    let jna = witness();
    for (dll, payload, marker) in [
        (DLL, b"changed".as_slice(), b"".as_slice()),
        (DLL, PAYLOAD, b"not empty"),
        ("jna.dll", PAYLOAD, b""),
        ("jna01.dll", PAYLOAD, b""),
        ("jna-1.dll", PAYLOAD, b""),
        ("jna18446744073709551616.dll", PAYLOAD, b""),
        ("jna123.DLL", PAYLOAD, b""),
        ("JNA123.dll", PAYLOAD, b""),
        ("jna123.other.dll", PAYLOAD, b""),
        ("anything.dll", PAYLOAD, b""),
    ] {
        pair(&fixture, dll, payload, marker);
        assert_eq!(
            inspect_natives(&fixture, &mut reader(), &jna),
            Err(Error::ArtifactMismatch),
            "{dll}"
        );
        fs::remove_file(fixture.copy.root().join(format!("natives/{dll}"))).unwrap();
        fs::remove_file(fixture.copy.root().join(format!("natives/{dll}.x"))).unwrap();
    }
    Fixture::write(fixture.copy.root(), &format!("natives/{DLL}"), PAYLOAD);
    assert_eq!(
        inspect_natives(&fixture, &mut reader(), &jna),
        Err(Error::ArtifactMismatch)
    );
    fs::remove_file(fixture.copy.root().join(format!("natives/{DLL}"))).unwrap();
    Fixture::write(fixture.copy.root(), &format!("natives/{DLL}.x"), b"");
    assert_eq!(
        inspect_natives(&fixture, &mut reader(), &jna),
        Err(Error::ArtifactMismatch)
    );
}

#[test]
fn ready_jna_legacy_runtime_rejects_unverified_residue_and_final_scan_sees_new_files() {
    let fixture = Fixture::new();
    let proof = fixture
        .inspect(&fixture.copy, fixture.source.id().as_str())
        .unwrap();
    pair(&fixture, DLL, PAYLOAD, b"");
    assert_eq!(proof.recheck_before_spawn(), Err(Error::InstanceChanged));
    drop(proof);
    assert!(matches!(
        fixture.inspect(&fixture.copy, fixture.source.id().as_str()),
        Err(Error::ArtifactMismatch)
    ));
    let mut cancelled = Reader {
        cancel: &|| true,
        pins: Vec::new(),
        hashes: Vec::new(),
        bytes: 0,
    };
    assert_eq!(
        inspect_natives(&fixture, &mut cancelled, &witness()),
        Err(Error::Cancelled)
    );
}

#[cfg(windows)]
#[test]
fn ready_jna_refuses_hardlinked_payload_or_marker() {
    let fixture = Fixture::new();
    let jna = witness();
    pair(&fixture, DLL, PAYLOAD, b"");
    for name in [DLL.to_owned(), format!("{DLL}.x")] {
        let original = fixture.copy.root().join(format!("natives/{name}"));
        let alias = fixture.root.join("alias");
        fs::hard_link(&original, &alias).unwrap();
        assert_eq!(
            inspect_natives(&fixture, &mut reader(), &jna),
            Err(Error::MissingArtifact)
        );
        fs::remove_file(alias).unwrap();
        inspect_natives(&fixture, &mut reader(), &jna).unwrap();
    }
}
