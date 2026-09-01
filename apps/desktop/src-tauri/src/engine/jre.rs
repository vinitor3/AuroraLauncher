use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use zip::ZipArchive;

use super::download::{download_one_with_progress, DownloadError, DownloadRequest, ExpectedHash};
use super::TransferProgress;

#[derive(Debug, Error)]
pub enum JavaRuntimeError {
    #[error("o executável Java não existe: {0}")]
    MissingExecutable(PathBuf),
    #[error("não foi possível consultar o Java: {0}")]
    Probe(#[from] std::io::Error),
    #[error("não foi possível baixar o Java: {0}")]
    Network(#[from] reqwest::Error),
    #[error("não foi possível baixar o Java: {0}")]
    Download(#[from] DownloadError),
    #[error("resposta inválida do serviço de Java")]
    Metadata,
    #[error("o pacote Java baixado não passou na verificação de integridade")]
    Integrity,
    #[error("o pacote Java é inválido: {0}")]
    Zip(#[from] zip::result::ZipError),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaReleaseCatalog {
    pub available: Vec<u32>,
    pub lts: Vec<u32>,
}

pub fn ensure_managed_java(
    data_directory: &Path,
    major: u32,
) -> Result<JavaRuntime, JavaRuntimeError> {
    ensure_managed_java_with_progress(data_directory, major, |_| {})
}

pub fn ensure_managed_java_with_progress<F>(
    data_directory: &Path,
    major: u32,
    mut progress: F,
) -> Result<JavaRuntime, JavaRuntimeError>
where
    F: FnMut(TransferProgress),
{
    let runtime_directory = data_directory
        .join("runtimes")
        .join(format!("java-{major}"));
    if let Some(executable) = find_java_executable(&runtime_directory) {
        progress(java_stage(format!("Java {major} já está instalado"), 100.0));
        return JavaRuntime::from_executable(executable);
    }

    progress(java_stage(format!("Consultando Java {major}"), 2.0));
    fs::create_dir_all(&runtime_directory)?;
    let client = Client::builder()
        .user_agent("Aurora-Smart-Launcher/0.1")
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(10 * 60))
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .build()?;
    let (package, package_type) = match latest_java_package(&client, major, "jre")? {
        Some(package) => (package, "JRE"),
        None => (
            latest_java_package(&client, major, "jdk")?.ok_or(JavaRuntimeError::Metadata)?,
            "JDK",
        ),
    };
    let download_url = package["link"]
        .as_str()
        .filter(|url| url.starts_with("https://"))
        .ok_or(JavaRuntimeError::Metadata)?;
    let expected_sha256 = package["checksum"]
        .as_str()
        .ok_or(JavaRuntimeError::Metadata)?;

    let archive_path = data_directory
        .join("runtimes")
        .join(format!("java-{major}.zip"));
    let request = DownloadRequest {
        url: download_url.to_owned(),
        destination: archive_path.clone(),
        label: format!("Java {major} ({package_type})"),
        expected_hash: Some(ExpectedHash::Sha256(expected_sha256.to_owned())),
        expected_size: package["size"].as_u64(),
    };
    download_one_with_progress(&client, request, |item| {
        progress(item.map_total(3.0, 88.0));
    })?;

    let mut archive = ZipArchive::new(File::open(&archive_path)?)?;
    let archive_entries = archive.len().max(1);
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let destination = runtime_directory.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(destination)?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut destination_file = File::create(destination)?;
        io::copy(&mut entry, &mut destination_file)?;
        if index % 25 == 0 || index + 1 == archive_entries {
            progress(java_stage(
                format!("Extraindo Java {major}"),
                88.0 + (index + 1) as f64 * 11.0 / archive_entries as f64,
            ));
        }
    }
    let executable = find_java_executable(&runtime_directory).ok_or(JavaRuntimeError::Metadata)?;
    progress(java_stage(format!("Java {major} pronto"), 100.0));
    JavaRuntime::from_executable(executable)
}

fn latest_java_package(
    client: &Client,
    major: u32,
    image_type: &str,
) -> Result<Option<Value>, JavaRuntimeError> {
    let metadata_url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture=x64&image_type={image_type}&os=windows&vendor=eclipse"
    );
    let metadata: Value = client
        .get(metadata_url)
        .send()?
        .error_for_status()?
        .json()?;
    Ok(metadata
        .as_array()
        .and_then(|releases| releases.first())
        .and_then(|release| release.pointer("/binary/package"))
        .cloned())
}

pub fn available_java_releases() -> Result<JavaReleaseCatalog, JavaRuntimeError> {
    let client = Client::builder()
        .user_agent("Aurora-Smart-Launcher/0.1")
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let metadata: Value = client
        .get("https://api.adoptium.net/v3/info/available_releases")
        .send()?
        .error_for_status()?
        .json()?;
    let parse = |field: &str| {
        metadata[field]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
            .filter_map(|major| u32::try_from(major).ok())
            .filter(|major| supported_managed_java_major(*major))
            .collect::<Vec<_>>()
    };
    let available = parse("available_releases");
    if available.is_empty() {
        return Err(JavaRuntimeError::Metadata);
    }
    Ok(JavaReleaseCatalog {
        available,
        lts: parse("available_lts_releases"),
    })
}

fn java_stage(label: impl Into<String>, total_percent: f64) -> TransferProgress {
    TransferProgress {
        label: label.into(),
        total_percent,
        item_percent: 100.0,
        ..TransferProgress::default()
    }
}

fn find_java_executable(directory: &Path) -> Option<PathBuf> {
    if !directory.is_dir() {
        return None;
    }
    for entry in fs::read_dir(directory).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_java_executable(&path) {
                return Some(found);
            }
        } else if path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(java_filename()))
            && path.parent().and_then(Path::file_name) == Some(std::ffi::OsStr::new("bin"))
        {
            return Some(path);
        }
    }
    None
}

/// Calcula o `-Xmx` do perfil, preservando no mínimo 2 GiB para o sistema.
///
/// Implementa a fórmula da especificação: `clamp(mods * 48, 4096,
/// ram_total - 2048)`. Em máquinas com pouca memória, o limite superior
/// prevalece para não reservar memória inexistente.
pub fn recommended_memory_mb(total_memory_mb: u32, mod_count: u32) -> u32 {
    let system_reserve_mb = 2_048;
    let minimum_mb = 4_096;
    let upper_bound = total_memory_mb.saturating_sub(system_reserve_mb);
    let by_mod_count = mod_count.saturating_mul(48);
    by_mod_count.max(minimum_mb).min(upper_bound)
}

/// Java selecionado para uma instância; nunca usa uma string de shell.
#[derive(Clone, Debug)]
pub struct JavaRuntime {
    executable: PathBuf,
    home: Option<PathBuf>,
}

impl JavaRuntime {
    pub fn from_executable(executable: impl Into<PathBuf>) -> Result<Self, JavaRuntimeError> {
        let executable = executable.into();
        if !executable.is_file() {
            return Err(JavaRuntimeError::MissingExecutable(executable));
        }
        Ok(Self {
            executable,
            home: None,
        })
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn with_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.home = Some(home.into());
        self
    }

    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    pub fn version(&self) -> Result<String, JavaRuntimeError> {
        let output = Command::new(&self.executable).arg("-version").output()?;
        Ok(String::from_utf8_lossy(&output.stderr)
            .lines()
            .next()
            .unwrap_or("Java detectado")
            .to_owned())
    }

    pub fn major_version(&self) -> Result<u32, JavaRuntimeError> {
        let output = Command::new(&self.executable).arg("-version").output()?;
        parse_java_major(&format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        ))
        .ok_or(JavaRuntimeError::Metadata)
    }
}

/// Localiza instalações Java usuais sem depender do `PATH` do launcher.
pub fn discover_java_executables() -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        paths.insert(PathBuf::from(home).join("bin").join(java_filename()));
    }

    #[cfg(target_os = "windows")]
    for base in [
        PathBuf::from(r"C:\Program Files\Java"),
        PathBuf::from(r"C:\Program Files\Eclipse Adoptium"),
        PathBuf::from(r"C:\Program Files\Microsoft"),
        PathBuf::from(r"C:\Program Files\Zulu"),
    ] {
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                paths.insert(entry.path().join("bin").join(java_filename()));
            }
        }
    }

    paths.into_iter().filter(|path| path.is_file()).collect()
}

pub fn discover_managed_java_executables(data_directory: &Path) -> Vec<PathBuf> {
    let runtimes = data_directory.join("runtimes");
    let Ok(entries) = fs::read_dir(runtimes) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let major = name.to_str()?.strip_prefix("java-")?.parse::<u32>().ok()?;
            supported_managed_java_major(major)
                .then(|| find_java_executable(&entry.path()))
                .flatten()
        })
        .collect()
}

pub fn supported_managed_java_major(major: u32) -> bool {
    (8..=30).contains(&major)
}

fn parse_java_major(version_text: &str) -> Option<u32> {
    let quoted = version_text.split('"').nth(1)?;
    let mut numbers = quoted.split('.');
    let first = numbers.next()?.parse::<u32>().ok()?;
    if first == 1 {
        numbers.next()?.parse::<u32>().ok()
    } else {
        Some(first)
    }
}

#[cfg(target_os = "windows")]
fn java_filename() -> &'static str {
    "java.exe"
}

#[cfg(not(target_os = "windows"))]
fn java_filename() -> &'static str {
    "java"
}

#[cfg(test)]
mod tests {
    use super::{parse_java_major, recommended_memory_mb, supported_managed_java_major};

    #[test]
    fn reserves_memory_for_the_system() {
        assert_eq!(recommended_memory_mb(16_384, 100), 4_800);
        assert_eq!(recommended_memory_mb(8_192, 10), 4_096);
        assert_eq!(recommended_memory_mb(4_096, 10), 2_048);
    }

    #[test]
    fn recognizes_installable_java_generations() {
        assert_eq!(parse_java_major("openjdk version \"16.0.2\""), Some(16));
        assert_eq!(parse_java_major("java version \"1.8.0_452\""), Some(8));
        assert!(supported_managed_java_major(16));
        assert!(supported_managed_java_major(26));
        assert!(!supported_managed_java_major(31));
    }
}
