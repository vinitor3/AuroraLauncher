//! Alternativas oficiais para arquivos externos, identificadas pelo conteúdo.
//!
//! A consulta nunca procura nomes ou versões semelhantes: apenas um SHA-1
//! idêntico publicado pela API Modrinth pode fornecer uma URL alternativa.
//! O download valida tamanho/hash e redirecionamentos antes de o chamador
//! publicar o arquivo temporário na instância.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use sha1::{Digest, Sha1};
use thiserror::Error;

use super::download::TransferProgress;

const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_MIRROR_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MIRROR_TRANSFER_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModrinthMirrorFile {
    pub download_url: String,
    pub sha1: String,
    pub size: u64,
    /// Informativo; o destino continua sendo o nome esperado pelo manifesto.
    pub file_name: String,
}

#[derive(Debug, Error)]
pub enum ExternalDownloadError {
    #[error("O SHA-1 do arquivo externo é inválido.")]
    InvalidSha1,
    #[error("Não foi possível acessar a alternativa oficial do Modrinth.")]
    Request(#[source] reqwest::Error),
    #[error("O Modrinth respondeu com HTTP {0}.")]
    HttpStatus(StatusCode),
    #[error("A resposta do Modrinth excedeu o limite de tamanho.")]
    ResponseTooLarge,
    #[error("Não foi possível ler a resposta do Modrinth.")]
    Read(#[source] io::Error),
    #[error("O Modrinth retornou metadados inválidos.")]
    InvalidResponse(#[source] serde_json::Error),
    #[error("O Modrinth retornou uma URL de arquivo não autorizada.")]
    InvalidDownloadUrl,
    #[error("O tamanho do arquivo alternativo é inválido ou excede 2 GiB.")]
    InvalidFileSize,
    #[error("O arquivo alternativo não corresponde ao tamanho ou SHA-1 esperado.")]
    Integrity,
    #[error("Não foi possível gravar o arquivo alternativo temporário.")]
    Write(#[source] io::Error),
    #[error("O download alternativo foi cancelado.")]
    Cancelled,
    #[error("O download alternativo excedeu o tempo limite.")]
    TransferTimeout,
}

#[derive(Deserialize)]
struct ModrinthVersion {
    files: Vec<ModrinthFile>,
}

#[derive(Deserialize)]
struct ModrinthFile {
    hashes: ModrinthHashes,
    url: String,
    filename: String,
    size: u64,
}

#[derive(Deserialize)]
struct ModrinthHashes {
    sha1: Option<String>,
}

/// Resolve somente metadados da cópia exata. Ausência (404 ou sem hash exato)
/// é `Ok(None)`; falhas de serviço são `Err`, permitindo o fluxo oficial usual.
pub fn resolve_modrinth_file_by_sha1(
    sha1: &str,
) -> Result<Option<ModrinthMirrorFile>, ExternalDownloadError> {
    let sha1 = normalized_sha1(sha1)?;
    let client = Client::builder()
        .user_agent("Aurora-Smart-Launcher/0.1")
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 3
                || !is_official_https_url(attempt.url(), "api.modrinth.com")
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(ExternalDownloadError::Request)?;
    let response = client
        .get(format!("{MODRINTH_API}/version_file/{sha1}"))
        .query(&[("algorithm", "sha1")])
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(Duration::from_secs(8))
        .send()
        .map_err(ExternalDownloadError::Request)?;
    if !metadata_status_has_file(response.status())? {
        return Ok(None);
    }
    let content_length = response.content_length();
    let body = read_bounded_metadata(response, content_length)?;
    parse_modrinth_file(&body, &sha1)
}

/// Regra compartilhada pelo resolvedor e pelo client que baixar a alternativa.
/// Redirecionamentos também devem passar por esta mesma validação.
pub fn is_modrinth_mirror_url(url: &Url) -> bool {
    is_official_https_url(url, "cdn.modrinth.com")
}

/// Baixa para um caminho temporário exclusivo fornecido pelo chamador. Nunca
/// substitui um arquivo existente; o chamador publica o resultado após sucesso.
/// Em falha, remove apenas o arquivo que esta execução criou.
/// O cancelamento é cooperativo entre blocos; uma operação de rede em andamento
/// permanece limitada pelo timeout HTTP e pode atrasar a interrupção.
pub fn download_modrinth_mirror_with_progress<F, C>(
    mirror: &ModrinthMirrorFile,
    destination: &Path,
    progress: F,
    should_cancel: C,
) -> Result<(), ExternalDownloadError>
where
    F: FnMut(TransferProgress),
    C: Fn() -> bool,
{
    ensure_not_cancelled(&should_cancel)?;
    normalized_sha1(&mirror.sha1)?;
    validate_mirror_size(mirror.size)?;
    let url =
        Url::parse(&mirror.download_url).map_err(|_| ExternalDownloadError::InvalidDownloadUrl)?;
    if !is_modrinth_mirror_url(&url) {
        return Err(ExternalDownloadError::InvalidDownloadUrl);
    }
    let client = Client::builder()
        .user_agent("Aurora-Smart-Launcher/0.1")
        .connect_timeout(Duration::from_secs(8))
        .timeout(MIRROR_TRANSFER_TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 3 || !is_modrinth_mirror_url(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(ExternalDownloadError::Request)?;
    ensure_not_cancelled(&should_cancel)?;
    let response = client.get(url).send();
    ensure_not_cancelled(&should_cancel)?;
    let response = response.map_err(ExternalDownloadError::Request)?;
    if response.status() != StatusCode::OK {
        return Err(ExternalDownloadError::HttpStatus(response.status()));
    }
    if response
        .content_length()
        .is_some_and(|size| size != mirror.size)
    {
        return Err(ExternalDownloadError::Integrity);
    }
    write_mirror_stream(response, mirror, destination, progress, should_cancel)
}

fn ensure_not_cancelled(should_cancel: &impl Fn() -> bool) -> Result<(), ExternalDownloadError> {
    if should_cancel() {
        return Err(ExternalDownloadError::Cancelled);
    }
    Ok(())
}

fn ensure_transfer_active(
    should_cancel: &impl Fn() -> bool,
    started: Instant,
) -> Result<(), ExternalDownloadError> {
    ensure_not_cancelled(should_cancel)?;
    if started.elapsed() >= MIRROR_TRANSFER_TIMEOUT {
        return Err(ExternalDownloadError::TransferTimeout);
    }
    Ok(())
}

fn validate_mirror_size(size: u64) -> Result<(), ExternalDownloadError> {
    if size == 0 || size > MAX_MIRROR_BYTES {
        return Err(ExternalDownloadError::InvalidFileSize);
    }
    Ok(())
}

fn write_mirror_stream<F, C>(
    mut reader: impl Read,
    mirror: &ModrinthMirrorFile,
    destination: &Path,
    mut progress: F,
    should_cancel: C,
) -> Result<(), ExternalDownloadError>
where
    F: FnMut(TransferProgress),
    C: Fn() -> bool,
{
    ensure_not_cancelled(&should_cancel)?;
    let expected_sha1 = normalized_sha1(&mirror.sha1)?;
    validate_mirror_size(mirror.size)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(ExternalDownloadError::Write)?;
    let result = (|| {
        let started = Instant::now();
        let mut last_progress = started;
        let mut downloaded = 0u64;
        let mut hasher = Sha1::new();
        let mut buffer = [0u8; 64 * 1024];
        progress(mirror_progress(mirror, 0, started, false));
        loop {
            ensure_transfer_active(&should_cancel, started)?;
            let count = reader.read(&mut buffer);
            ensure_transfer_active(&should_cancel, started)?;
            let count = count.map_err(ExternalDownloadError::Read)?;
            if count == 0 {
                break;
            }
            downloaded += count as u64;
            if downloaded > mirror.size {
                return Err(ExternalDownloadError::Integrity);
            }
            ensure_transfer_active(&should_cancel, started)?;
            let written = output.write_all(&buffer[..count]);
            ensure_transfer_active(&should_cancel, started)?;
            written.map_err(ExternalDownloadError::Write)?;
            hasher.update(&buffer[..count]);
            if last_progress.elapsed() >= Duration::from_millis(100) {
                progress(mirror_progress(mirror, downloaded, started, false));
                last_progress = Instant::now();
            }
        }
        if downloaded != mirror.size || format!("{:x}", hasher.finalize()) != expected_sha1 {
            return Err(ExternalDownloadError::Integrity);
        }
        ensure_transfer_active(&should_cancel, started)?;
        output.sync_all().map_err(ExternalDownloadError::Write)?;
        ensure_transfer_active(&should_cancel, started)?;
        progress(mirror_progress(mirror, downloaded, started, true));
        ensure_not_cancelled(&should_cancel)?;
        Ok(())
    })();
    drop(output);
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

fn mirror_progress(
    mirror: &ModrinthMirrorFile,
    downloaded: u64,
    started: Instant,
    complete: bool,
) -> TransferProgress {
    let percent = if complete {
        100.0
    } else {
        (downloaded as f64 / mirror.size as f64 * 100.0).min(99.0)
    };
    TransferProgress {
        label: mirror.file_name.clone(),
        total_percent: percent,
        item_percent: percent,
        item_downloaded_bytes: downloaded,
        item_total_bytes: Some(mirror.size),
        downloaded_bytes: downloaded,
        total_bytes: Some(mirror.size),
        completed_files: usize::from(complete),
        total_files: 1,
        active_downloads: usize::from(!complete),
        bytes_per_second: (downloaded as f64 / started.elapsed().as_secs_f64().max(0.001)) as u64,
    }
}

fn is_official_https_url(url: &Url, expected_host: &str) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
        && url.host_str() == Some(expected_host)
}

fn normalized_sha1(sha1: &str) -> Result<String, ExternalDownloadError> {
    if sha1.len() != 40 || !sha1.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ExternalDownloadError::InvalidSha1);
    }
    Ok(sha1.to_ascii_lowercase())
}

fn metadata_status_has_file(status: StatusCode) -> Result<bool, ExternalDownloadError> {
    if status == StatusCode::NOT_FOUND {
        Ok(false)
    } else if status == StatusCode::OK {
        Ok(true)
    } else {
        Err(ExternalDownloadError::HttpStatus(status))
    }
}

fn read_bounded_metadata(
    reader: impl Read,
    content_length: Option<u64>,
) -> Result<Vec<u8>, ExternalDownloadError> {
    if content_length.is_some_and(|length| length > MAX_METADATA_BYTES) {
        return Err(ExternalDownloadError::ResponseTooLarge);
    }
    let mut body = Vec::new();
    reader
        .take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(ExternalDownloadError::Read)?;
    if body.len() as u64 > MAX_METADATA_BYTES {
        return Err(ExternalDownloadError::ResponseTooLarge);
    }
    Ok(body)
}

fn parse_modrinth_file(
    body: &[u8],
    expected_sha1: &str,
) -> Result<Option<ModrinthMirrorFile>, ExternalDownloadError> {
    let expected_sha1 = normalized_sha1(expected_sha1)?;
    let version: ModrinthVersion =
        serde_json::from_slice(body).map_err(ExternalDownloadError::InvalidResponse)?;
    for file in version.files {
        if !file
            .hashes
            .sha1
            .as_deref()
            .is_some_and(|hash| hash.eq_ignore_ascii_case(&expected_sha1))
        {
            continue;
        }
        let url = Url::parse(&file.url).map_err(|_| ExternalDownloadError::InvalidDownloadUrl)?;
        if !is_modrinth_mirror_url(&url) {
            return Err(ExternalDownloadError::InvalidDownloadUrl);
        }
        return Ok(Some(ModrinthMirrorFile {
            download_url: url.into(),
            sha1: expected_sha1,
            size: file.size,
            file_name: file.filename,
        }));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SHA1: &str = "c84dd4b3580c02b79958a0590afd5783d80ef504";
    const URL: &str = "https://cdn.modrinth.com/data/project/versions/version/mod.jar";

    fn file(sha1: Option<&str>, url: &str) -> serde_json::Value {
        json!({
            "hashes": {"sha1": sha1},
            "url": url,
            "filename": "mod.jar",
            "size": 12345
        })
    }

    fn parse(
        files: Vec<serde_json::Value>,
    ) -> Result<Option<ModrinthMirrorFile>, ExternalDownloadError> {
        parse_modrinth_file(
            &serde_json::to_vec(&json!({ "files": files })).unwrap(),
            SHA1,
        )
    }

    #[test]
    fn selects_exact_secondary_file_instead_of_different_primary() {
        let mut primary = file(Some("0000000000000000000000000000000000000000"), URL);
        primary["primary"] = json!(true);
        let mut exact = file(Some(SHA1), URL);
        exact["primary"] = json!(false);
        let result = parse(vec![primary, exact]).unwrap().unwrap();
        assert_eq!(result.sha1, SHA1);
        assert_eq!(result.download_url, URL);
        assert_eq!(result.size, 12345);
        assert_eq!(result.file_name, "mod.jar");
    }

    #[test]
    fn never_substitutes_missing_or_different_hashes() {
        assert!(parse(vec![file(None, URL)]).unwrap().is_none());
        assert!(parse(vec![file(
            Some("0000000000000000000000000000000000000000"),
            URL
        )])
        .unwrap()
        .is_none());
        assert!(parse(vec![]).unwrap().is_none());
    }

    #[test]
    fn validates_hash_before_building_api_path() {
        assert_eq!(normalized_sha1(&SHA1.to_ascii_uppercase()).unwrap(), SHA1);
        assert!(parse(vec![file(Some(&SHA1.to_ascii_uppercase()), URL)])
            .unwrap()
            .is_some());
        for invalid in [
            "",
            "abc",
            "../version",
            "g84dd4b3580c02b79958a0590afd5783d80ef504",
        ] {
            assert!(matches!(
                normalized_sha1(invalid),
                Err(ExternalDownloadError::InvalidSha1)
            ));
        }
        assert!(normalized_sha1(&format!(" {SHA1}")).is_err());
    }

    #[test]
    fn allows_only_official_cdn_https_without_credentials_or_custom_port() {
        assert!(is_modrinth_mirror_url(&Url::parse(URL).unwrap()));
        for url in [
            "http://cdn.modrinth.com/mod.jar",
            "https://cdn.modrinth.com.evil.test/mod.jar",
            "https://evil.cdn.modrinth.com/mod.jar",
            "https://api.modrinth.com/mod.jar",
            "https://modrinth.com/mod.jar",
            "https://127.0.0.1/mod.jar",
            "https://user@cdn.modrinth.com/mod.jar",
            "https://:secret@cdn.modrinth.com/mod.jar",
            "https://cdn.modrinth.com:8443/mod.jar",
            "https://cdn.modrinth.com/mod.jar#other",
            "file:///mod.jar",
        ] {
            assert!(!is_modrinth_mirror_url(&Url::parse(url).unwrap()), "{url}");
            assert!(matches!(
                parse(vec![file(Some(SHA1), url)]),
                Err(ExternalDownloadError::InvalidDownloadUrl)
            ));
        }
    }

    #[test]
    fn limits_advertised_and_streamed_metadata_bytes() {
        assert!(matches!(
            read_bounded_metadata(io::empty(), Some(MAX_METADATA_BYTES + 1)),
            Err(ExternalDownloadError::ResponseTooLarge)
        ));
        assert!(matches!(
            read_bounded_metadata(io::repeat(b' '), None),
            Err(ExternalDownloadError::ResponseTooLarge)
        ));
        assert!(matches!(
            read_bounded_metadata(io::repeat(b' '), Some(1)),
            Err(ExternalDownloadError::ResponseTooLarge)
        ));
        assert_eq!(read_bounded_metadata(&b"{}"[..], Some(2)).unwrap(), b"{}");
    }

    #[test]
    fn not_found_is_absence_while_transient_failures_remain_errors() {
        assert!(!metadata_status_has_file(StatusCode::NOT_FOUND).unwrap());
        assert!(metadata_status_has_file(StatusCode::OK).unwrap());
        for status in [
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::FOUND,
            StatusCode::UNAUTHORIZED,
        ] {
            assert!(
                matches!(metadata_status_has_file(status), Err(ExternalDownloadError::HttpStatus(actual)) if actual == status)
            );
        }
    }

    #[test]
    fn malformed_metadata_is_not_treated_as_not_found() {
        for body in [b"not json".as_slice(), b"{}", br#"{"files":[{}]}"#] {
            assert!(matches!(
                parse_modrinth_file(body, SHA1),
                Err(ExternalDownloadError::InvalidResponse(_))
            ));
        }
    }

    fn mirror_for(bytes: &[u8]) -> ModrinthMirrorFile {
        ModrinthMirrorFile {
            download_url: URL.to_owned(),
            sha1: format!("{:x}", Sha1::digest(bytes)),
            size: bytes.len() as u64,
            file_name: "mod.jar".to_owned(),
        }
    }

    fn staging_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("aurora-mirror-test-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn streaming_download_validates_bytes_and_finishes_progress() {
        let bytes = b"exact same artifact";
        let destination = staging_path();
        let mut events = Vec::new();
        write_mirror_stream(
            &bytes[..],
            &mirror_for(bytes),
            &destination,
            |event| events.push(event),
            || false,
        )
        .unwrap();
        assert_eq!(fs::read(&destination).unwrap(), bytes);
        assert_eq!(events.last().unwrap().completed_files, 1);
        assert_eq!(events.last().unwrap().total_percent, 100.0);
        fs::remove_file(destination).unwrap();
    }

    #[test]
    fn streaming_download_never_overwrites_or_removes_existing_destination() {
        let bytes = b"new download";
        let destination = staging_path();
        let existing = b"existing user file";
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .unwrap();
        output.write_all(existing).unwrap();
        drop(output);
        assert!(write_mirror_stream(
            &bytes[..],
            &mirror_for(bytes),
            &destination,
            |_| {},
            || false
        )
        .is_err());
        assert_eq!(fs::read(&destination).unwrap(), existing);
        fs::remove_file(destination).unwrap();
    }

    #[test]
    fn streaming_download_cleans_own_file_for_wrong_hash_size_and_read_error() {
        let bytes = b"correct download";
        for received in [
            b"corrupt download".as_slice(),
            b"short",
            b"too many bytes in this download",
        ] {
            let destination = staging_path();
            assert!(matches!(
                write_mirror_stream(received, &mirror_for(bytes), &destination, |_| {}, || false),
                Err(ExternalDownloadError::Integrity)
            ));
            assert!(!destination.exists());
        }
        struct BrokenReader;
        impl Read for BrokenReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("test read failure"))
            }
        }
        let destination = staging_path();
        assert!(matches!(
            write_mirror_stream(
                BrokenReader,
                &mirror_for(bytes),
                &destination,
                |_| {},
                || false
            ),
            Err(ExternalDownloadError::Read(_))
        ));
        assert!(!destination.exists());
    }

    #[test]
    fn streaming_download_rejects_unbounded_size_before_creating_file() {
        for size in [0, MAX_MIRROR_BYTES + 1, u64::MAX] {
            let destination = staging_path();
            let mut mirror = mirror_for(b"x");
            mirror.size = size;
            assert!(matches!(
                write_mirror_stream(io::empty(), &mirror, &destination, |_| {}, || false),
                Err(ExternalDownloadError::InvalidFileSize)
            ));
            assert!(!destination.exists());
        }
    }

    #[test]
    fn cancellation_before_download_creates_no_staging_file() {
        let destination = staging_path();
        assert!(matches!(
            download_modrinth_mirror_with_progress(
                &mirror_for(b"test"),
                &destination,
                |_| {},
                || true
            ),
            Err(ExternalDownloadError::Cancelled)
        ));
        assert!(!destination.exists());
    }

    #[test]
    fn cancellation_after_read_removes_staging_without_reporting_success() {
        use std::cell::Cell;
        let cancelled = Cell::new(false);
        struct CancellingReader<'a>(&'a Cell<bool>);
        impl Read for CancellingReader<'_> {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                buffer[0] = b'x';
                self.0.set(true);
                Ok(1)
            }
        }
        let destination = staging_path();
        let mut completed = false;
        assert!(matches!(
            write_mirror_stream(
                CancellingReader(&cancelled),
                &mirror_for(b"x"),
                &destination,
                |event| completed |= event.completed_files == 1,
                || cancelled.get()
            ),
            Err(ExternalDownloadError::Cancelled)
        ));
        assert!(!destination.exists());
        assert!(!completed);
    }

    #[test]
    fn cancellation_before_publication_removes_validated_staging() {
        use std::cell::Cell;
        let cancelled = Cell::new(false);
        let destination = staging_path();
        assert!(matches!(
            write_mirror_stream(
                b"x".as_slice(),
                &mirror_for(b"x"),
                &destination,
                |event| {
                    if event.completed_files == 1 {
                        cancelled.set(true);
                    }
                },
                || cancelled.get()
            ),
            Err(ExternalDownloadError::Cancelled)
        ));
        assert!(!destination.exists());
    }

    #[test]
    fn transfer_duration_is_bounded_even_when_bytes_keep_arriving() {
        assert!(matches!(
            ensure_transfer_active(&|| false, Instant::now() - MIRROR_TRANSFER_TIMEOUT),
            Err(ExternalDownloadError::TransferTimeout)
        ));
    }

    #[test]
    #[ignore = "Requer a API e CDN oficiais do Modrinth; baixa aproximadamente 1 MiB"]
    fn live_shotsfired_exact_hash_resolves_and_downloads() {
        const SHOTSFIRE_SHA1: &str = "f5ba631f02cd9df539417b0be4393a81e00ed0e0";
        let mirror = resolve_modrinth_file_by_sha1(SHOTSFIRE_SHA1)
            .expect("API Modrinth deve responder")
            .expect("arquivo ShotsFired com hash exato deve estar publicado");
        assert_eq!(mirror.sha1, SHOTSFIRE_SHA1);
        assert_eq!(mirror.file_name, "shotsfired-1.20.1-0.2.2.jar");
        assert!(is_modrinth_mirror_url(
            &Url::parse(&mirror.download_url).unwrap()
        ));
        let destination = staging_path();
        let mut final_progress = None;
        let result = download_modrinth_mirror_with_progress(
            &mirror,
            &destination,
            |event| final_progress = Some(event),
            || false,
        );
        // Ler e remover antes das asserções mantém o cleanup restrito ao staging
        // criado por esta execução, inclusive se alguma verificação falhar.
        let bytes = match result {
            Ok(()) => {
                let bytes = fs::read(&destination);
                fs::remove_file(&destination).expect("deve limpar seu próprio staging");
                bytes.expect("deve ler o download validado")
            }
            Err(error) => {
                assert!(!destination.exists(), "falha deve limpar seu staging");
                panic!("download oficial falhou: {error}");
            }
        };
        assert_eq!(bytes.len() as u64, mirror.size);
        assert_eq!(format!("{:x}", Sha1::digest(&bytes)), SHOTSFIRE_SHA1);
        let progress = final_progress.expect("download deve emitir progresso");
        assert_eq!(progress.completed_files, 1);
        assert_eq!(progress.total_percent, 100.0);
        assert_eq!(progress.downloaded_bytes, mirror.size);
    }
}
