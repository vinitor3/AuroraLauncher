use super::*;
use std::io::{Seek, SeekFrom};

pub(super) const PROPERTIES: &[u8] = b"# Disposable Aurora Connect STATUS laboratory; no personal world.\nserver-ip=127.0.0.1\nserver-port=43001\nonline-mode=false\nwhite-list=true\nenforce-whitelist=true\nenable-rcon=false\nenable-query=false\nenable-jmx-monitoring=false\nenable-status=true\nmax-players=2\nview-distance=2\nsimulation-distance=5\nlevel-name=connect-lab-world\nlevel-seed=0\ngenerate-structures=false\nmotd=Aurora Connect disposable status lab\n";
const ARTIFACTS: [(&str, u64, &str); 9] = [
    (
        "server.jar",
        51627615,
        "59353fb40c36d304f2035d51e7d6e6baa98dc05c",
    ),
    (
        "libraries/asm-9.8.jar",
        126113,
        "876eab6a83daecad5ca67eb9fcabb063c97b5aeb8cf1fca7a989ecde17522051",
    ),
    (
        "libraries/asm-analysis-9.8.jar",
        35151,
        "e640732fbcd3c6271925a504f125e38384688f4dfbbf92c8622dfcee0d09edb9",
    ),
    (
        "libraries/asm-commons-9.8.jar",
        73498,
        "3301a1c1cb4c59fcc5292648dac1d7c5aed4c0f067dfbe88873b8cdfe77404f4",
    ),
    (
        "libraries/asm-tree-9.8.jar",
        51934,
        "14b7880cb7c85eed101e2710432fc3ffb83275532a6a894dc4c4095d49ad59f1",
    ),
    (
        "libraries/asm-util-9.8.jar",
        94559,
        "8ba0460ecb28fd0e2980e5f3ef3433a513a457bc077f81a53bdc75b587a08d15",
    ),
    (
        "libraries/sponge-mixin-0.15.5+mixin.0.8.7.jar",
        1499194,
        "01dd2b778ed5283bce6b6b07d9690d86d956b17a7103efafad47073db1599584",
    ),
    (
        "libraries/intermediary-1.21.1.jar",
        657725,
        "6059157dfb4a536ec151697004f74f6004b7d346b289c2f89849e0e585aa36fa",
    ),
    (
        "libraries/fabric-loader-0.16.14.jar",
        1416842,
        "9ed7b7f4197153ad97d71b4b9cd9148989e8f0c600934b035ce45f3ef003efc0",
    ),
];
pub(super) struct Pin {
    name: &'static str,
    size: u64,
    expected: String,
    sha1: bool,
}
pub(super) fn pins() -> Vec<Pin> {
    ARTIFACTS
        .iter()
        .enumerate()
        .map(|(i, (name, size, hash))| Pin {
            name,
            size: *size,
            expected: (*hash).into(),
            sha1: i == 0,
        })
        .collect()
}
pub(super) fn artifact_name(name: &str) -> bool {
    ARTIFACTS.iter().any(|(candidate, _, _)| *candidate == name)
}
pub(super) fn status_classpath() -> String {
    ARTIFACTS[1..]
        .iter()
        .map(|(name, _, _)| *name)
        .collect::<Vec<_>>()
        .join(";")
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceRecord {
    pub name: String,
    pub observation: Observation,
}
struct Pinned {
    name: String,
    path: PathBuf,
    file: recovery::SecureFile,
    observation: Observation,
}
pub(super) struct Resources {
    files: Vec<Pinned>,
    pub records: Vec<ResourceRecord>,
    _directories: Vec<File>,
}
impl Resources {
    pub(super) fn java_executable(&self) -> Result<&Path> {
        self.files
            .iter()
            .find(|pin| pin.name == "jdk/java.exe")
            .map(|pin| pin.path.as_path())
            .ok_or(Error::Resources)
    }
    pub fn inspect(lab: &Path, stage: &Path, jdk: &Path, pins: Vec<Pin>) -> Result<Self> {
        explicit_path(lab)?;
        explicit_path(stage)?;
        explicit_path(jdk)?;
        let name = stage
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(Error::UnsafePath)?;
        if stage.parent() != Some(lab)
            || !name.starts_with("prepared-")
            || name.len() > 100
            || name.len() <= 9
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(Error::UnsafePath);
        }
        let mut directories = recovery::pin_directory(stage)?;
        directories.extend(recovery::pin_directory(jdk)?);
        let mut result = Self {
            files: Vec::new(),
            records: Vec::new(),
            _directories: directories,
        };
        for pin in pins {
            let bytes = result.read_pin(&stage.join(pin.name), pin.name, pin.size)?;
            let hash = if pin.sha1 {
                format!("{:x}", sha1::Sha1::digest(&bytes))
            } else {
                digest(&bytes)
            };
            if bytes.len() as u64 != pin.size || hash != pin.expected {
                return Err(Error::Resources);
            }
        }
        let release = result.read_pin(&jdk.join("release"), "jdk/release", 64 * 1024)?;
        let release = std::str::from_utf8(&release).map_err(|_| Error::Resources)?;
        let field = |key: &str| -> Result<&str> {
            let prefix = format!("{key}=");
            let values: Vec<_> = release
                .lines()
                .filter_map(|line| line.strip_prefix(&prefix))
                .take(2)
                .collect();
            if values.len() != 1 {
                return Err(Error::Resources);
            }
            values[0]
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .filter(|s| !s.is_empty() && !s.contains('"'))
                .ok_or(Error::Resources)
        };
        let version = field("JAVA_VERSION")?;
        let version_parts = version
            .split_once('+')
            .map_or(version, |(v, _)| v)
            .split('.')
            .collect::<Vec<_>>();
        if !(2..=4).contains(&version_parts.len())
            || version_parts[0] != "21"
            || version_parts
                .iter()
                .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
            || version
                .split_once('+')
                .is_some_and(|(_, n)| n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()))
            || field("OS_NAME")? != "Windows"
            || field("OS_ARCH")? != "x86_64"
        {
            return Err(Error::Resources);
        }
        let binary = result.read_pin(&jdk.join("bin/java.exe"), "jdk/java.exe", 4 * 1024 * 1024)?;
        if !binary.starts_with(b"MZ") {
            return Err(Error::Resources);
        }
        Ok(result)
    }
    fn read_pin(&mut self, path: &Path, name: &str, limit: u64) -> Result<Vec<u8>> {
        let mut file = recovery::secure_open(path)?;
        let size = file.metadata()?.len();
        if size > limit {
            return Err(Error::Resources);
        }
        let mut bytes = Vec::new();
        (&mut file).take(limit + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 != size {
            return Err(Error::Changed);
        }
        let observation = Observation {
            identity: file_identity(&file)?,
            bytes: size,
            sha256: digest(&bytes),
        };
        self.records.push(ResourceRecord {
            name: name.into(),
            observation: observation.clone(),
        });
        self.files.push(Pinned {
            name: name.into(),
            path: path.into(),
            file,
            observation,
        });
        Ok(bytes)
    }
    pub fn recheck(&self) -> Result<()> {
        for pin in &self.files {
            if observe_file(&pin.path, pin.observation.bytes)?.0 != pin.observation {
                return Err(Error::Changed);
            }
        }
        Ok(())
    }
    pub fn copy_artifacts(&mut self, destination: &Path) -> Result<()> {
        fs::create_dir(destination.join("libraries"))?;
        for pin in &mut self.files {
            if pin.name.starts_with("jdk/") {
                continue;
            }
            pin.file.seek(SeekFrom::Start(0))?;
            let mut bytes = Vec::new();
            (&mut pin.file)
                .take(pin.observation.bytes + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 != pin.observation.bytes
                || digest(&bytes) != pin.observation.sha256
            {
                return Err(Error::Changed);
            }
            recovery::write_new(&destination.join(&pin.name), &bytes)?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn fixture_artifacts() -> Vec<(&'static str, Vec<u8>)> {
    ARTIFACTS
        .iter()
        .map(|(name, _, _)| (*name, format!("fixture:{name}\n").into_bytes()))
        .collect()
}
#[cfg(test)]
pub(super) fn fixture_pins() -> Vec<Pin> {
    fixture_artifacts()
        .into_iter()
        .map(|(name, bytes)| Pin {
            name,
            size: bytes.len() as u64,
            expected: digest(&bytes),
            sha1: false,
        })
        .collect()
}
#[cfg(test)]
pub(super) fn fixture_jdk() -> (Vec<u8>, Vec<u8>) {
    (
        b"JAVA_VERSION=\"21.0.1\"\nOS_NAME=\"Windows\"\nOS_ARCH=\"x86_64\"\n".to_vec(),
        b"MZowned-fixture".to_vec(),
    )
}
