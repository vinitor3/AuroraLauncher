//! JNA's Windows cleanup leaves the extracted library and an empty `.x` marker.
//! This exception derives its payload from the exact, verified classpath JAR;
//! it grants no authority to a filename, marker, or unverified legacy profile.
use super::{verify_sha1, Error, Instance, Reader, RuntimePins};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
};

const RESOURCE: &str = "com/sun/jna/win32-x86-64/jnidispatch.dll";
const MAX_DISPATCH_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Default)]
pub(super) struct JnaTemporaryFiles {
    payloads: BTreeSet<(usize, [u8; 32])>,
    archives: usize,
}

impl JnaTemporaryFiles {
    pub(super) fn observe_classpath(
        &mut self,
        relative: &str,
        bytes: &[u8],
        expected_sha1: &str,
    ) -> Result<(), Error> {
        let Some(suffix) = relative.strip_prefix("libraries/net/java/dev/jna/jna/") else {
            return Ok(());
        };
        let (version, filename) = suffix.split_once('/').ok_or(Error::ArtifactMismatch)?;
        if version.is_empty()
            || version.len() > 32
            || !version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            || filename != format!("jna-{version}.jar")
            || self.archives >= 8
        {
            return Err(Error::ArtifactMismatch);
        }
        // Keep this check inside the witness constructor as well as the caller:
        // no caller may turn an arbitrary archive into a native-file exception.
        verify_sha1(bytes, expected_sha1)?;
        let mut archive =
            zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| Error::ArtifactMismatch)?;
        if archive.len() > 4096 {
            return Err(Error::ArtifactMismatch);
        }
        // zip's name map collapses duplicate central-directory entries. These
        // small official JNA archives use ordinary ZIP: require its original
        // entry count to match the map before trusting the single resource.
        let end = bytes
            .len()
            .checked_sub(22 + archive.comment().len())
            .ok_or(Error::ArtifactMismatch)?;
        if &bytes[end..end + 4] != b"PK\x05\x06"
            || usize::from(u16::from_le_bytes([bytes[end + 10], bytes[end + 11]])) != archive.len()
        {
            return Err(Error::ArtifactMismatch);
        }
        let mut dispatch = None;
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|_| Error::ArtifactMismatch)?;
            if entry.name() != RESOURCE {
                continue;
            }
            if dispatch.is_some() || entry.is_dir() || entry.size() > MAX_DISPATCH_BYTES {
                return Err(Error::ArtifactMismatch);
            }
            let mut payload = Vec::new();
            entry
                .take(MAX_DISPATCH_BYTES + 1)
                .read_to_end(&mut payload)
                .map_err(|_| Error::ArtifactMismatch)?;
            if payload.is_empty() || payload.len() as u64 > MAX_DISPATCH_BYTES {
                return Err(Error::ArtifactMismatch);
            }
            dispatch = Some((payload.len(), Sha256::digest(payload).into()));
        }
        self.payloads
            .insert(dispatch.ok_or(Error::ArtifactMismatch)?);
        self.archives += 1;
        Ok(())
    }

    fn verify(&self, name: &str, bytes: &[u8], observed: &[String]) -> Result<(), Error> {
        if self.payloads.is_empty() {
            return Err(Error::ArtifactMismatch);
        }
        let (dll, marker) = match name.strip_suffix(".x") {
            Some(dll) => (dll, true),
            None => (name, false),
        };
        let digits = dll
            .strip_prefix("jna")
            .and_then(|s| s.strip_suffix(".dll"))
            .ok_or(Error::ArtifactMismatch)?;
        if digits.is_empty()
            || digits.len() > 20
            || !digits.bytes().all(|b| b.is_ascii_digit())
            || digits.parse::<u64>().ok().map(|n| n.to_string()).as_deref() != Some(digits)
        {
            return Err(Error::ArtifactMismatch);
        }
        let partner = if marker {
            dll.to_owned()
        } else {
            format!("{dll}.x")
        };
        if !observed.contains(&partner)
            || (marker && !bytes.is_empty())
            || (!marker
                && !self
                    .payloads
                    .contains(&(bytes.len(), Sha256::digest(bytes).into())))
        {
            return Err(Error::ArtifactMismatch);
        }
        Ok(())
    }
}

pub(super) fn pin_natives(
    reader: &mut Reader<'_>,
    instance: &Instance,
    observed: &[String],
    pins: Option<&RuntimePins>,
    classpath_hashes: &BTreeMap<String, String>,
    jna: &JnaTemporaryFiles,
) -> Result<(), Error> {
    if pins.is_some_and(|pins| {
        pins.extracted_natives
            .keys()
            .any(|name| !observed.contains(name))
    }) {
        return Err(Error::MissingArtifact);
    }
    for name in observed {
        // Every accepted temporary and marker uses the same secure file reader,
        // retained handles, byte budget, fingerprint and final directory check.
        let bytes = reader.instance(instance, &format!("natives/{name}"), 64 * 1024 * 1024)?;
        if let Some(pins) = pins {
            let expected = if pins.extracted_natives.is_empty() {
                classpath_hashes.get(name)
            } else {
                pins.extracted_natives.get(name)
            };
            if let Some(expected) = expected {
                verify_sha1(&bytes, expected)?;
            } else {
                jna.verify(name, &bytes, observed)?;
            }
        } else if name.to_ascii_lowercase().starts_with("jna") {
            // Legacy unpinned inputs cannot authorize runtime-generated JNA.
            return Err(Error::ArtifactMismatch);
        }
    }
    Ok(())
}
