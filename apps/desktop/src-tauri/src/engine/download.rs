//! Downloads HTTP compartilhados pelo instalador.
//!
//! O gerenciador mantém poucas conexões simultâneas, reaproveita o pool do
//! `reqwest`, grava primeiro em arquivo temporário e só publica o destino após
//! validar sua integridade.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use reqwest::header::RANGE;
use reqwest::StatusCode;
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Sha256, Sha512};
use thiserror::Error;

pub const DEFAULT_DOWNLOAD_CONCURRENCY: usize = 8;
const COPY_BUFFER_SIZE: usize = 256 * 1024;
const MAX_ATTEMPTS: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpectedHash {
    Sha1(String),
    Sha256(String),
    Sha512(String),
}

#[derive(Clone, Debug)]
pub struct DownloadRequest {
    pub url: String,
    pub destination: PathBuf,
    pub label: String,
    pub expected_hash: Option<ExpectedHash>,
    pub expected_size: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct TransferProgress {
    pub label: String,
    pub total_percent: f64,
    pub item_percent: f64,
    pub item_downloaded_bytes: u64,
    pub item_total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub completed_files: usize,
    pub total_files: usize,
    pub active_downloads: usize,
    pub bytes_per_second: u64,
}

impl TransferProgress {
    pub fn map_total(mut self, start: f64, end: f64) -> Self {
        let span = (end - start).max(0.0);
        self.total_percent = (start + span * self.total_percent / 100.0).clamp(0.0, 100.0);
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct DownloadBatchSummary {
    pub downloaded_files: usize,
    pub cached_files: usize,
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("falha de rede em {url}: {source}")]
    Network {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("falha de E/S em {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("integridade inválida para {0}")]
    Integrity(PathBuf),
    #[error("o servidor não aceitou a retomada de {0}")]
    Resume(PathBuf),
    #[error("dois downloads diferentes tentaram gravar no mesmo destino: {0}")]
    ConflictingDestination(PathBuf),
}

enum WorkerEvent {
    Started {
        index: usize,
        downloaded: u64,
        total: Option<u64>,
    },
    Bytes {
        index: usize,
        downloaded: u64,
        delta: u64,
    },
    Finished {
        index: usize,
        cached: bool,
    },
    Failed {
        index: usize,
        error: DownloadError,
    },
}

/// Baixa um lote com concorrência limitada. A porcentagem total usa cada
/// arquivo como uma unidade de trabalho, evitando regressões quando um servidor
/// só informa o tamanho depois que a conexão começa.
pub fn download_many<F>(
    client: &Client,
    requests: Vec<DownloadRequest>,
    concurrency: usize,
    mut progress: F,
) -> Result<DownloadBatchSummary, DownloadError>
where
    F: FnMut(TransferProgress),
{
    let requests = deduplicate_requests(requests)?;
    if requests.is_empty() {
        return Ok(DownloadBatchSummary::default());
    }

    let total_files = requests.len();
    let initial_labels = requests
        .iter()
        .map(|request| request.label.clone())
        .collect::<Vec<_>>();
    let queue = Arc::new(Mutex::new(
        requests.into_iter().enumerate().collect::<VecDeque<_>>(),
    ));
    let (sender, receiver) = mpsc::channel();
    let worker_count = concurrency
        .clamp(1, DEFAULT_DOWNLOAD_CONCURRENCY)
        .min(total_files);

    let mut downloaded = vec![0u64; total_files];
    let mut totals = vec![None; total_files];
    let mut completed = vec![false; total_files];
    let mut active = HashSet::new();
    let mut completed_files = 0usize;
    let mut downloaded_files = 0usize;
    let mut cached_files = 0usize;
    let mut network_bytes = 0u64;
    let started_at = Instant::now();
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let mut first_error = None;

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let sender = sender.clone();
            let client = client.clone();
            scope.spawn(move || loop {
                let next = queue.lock().ok().and_then(|mut queue| queue.pop_front());
                let Some((index, request)) = next else {
                    break;
                };
                match download_one(&client, index, &request, &sender) {
                    Ok(cached) => {
                        let _ = sender.send(WorkerEvent::Finished { index, cached });
                    }
                    Err(error) => {
                        let _ = sender.send(WorkerEvent::Failed { index, error });
                    }
                }
            });
        }
        drop(sender);

        for event in receiver {
            let mut force_emit = false;
            let current_index = match event {
                WorkerEvent::Started {
                    index,
                    downloaded: current,
                    total,
                } => {
                    active.insert(index);
                    downloaded[index] = current;
                    totals[index] = total;
                    index
                }
                WorkerEvent::Bytes {
                    index,
                    downloaded: current,
                    delta,
                } => {
                    downloaded[index] = current;
                    network_bytes = network_bytes.saturating_add(delta);
                    index
                }
                WorkerEvent::Finished { index, cached } => {
                    active.remove(&index);
                    completed[index] = true;
                    if let Some(total) = totals[index] {
                        downloaded[index] = total;
                    }
                    completed_files += 1;
                    if cached {
                        cached_files += 1;
                    } else {
                        downloaded_files += 1;
                    }
                    force_emit = true;
                    index
                }
                WorkerEvent::Failed { index, error } => {
                    active.remove(&index);
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                    force_emit = true;
                    index
                }
            };

            if force_emit || last_emit.elapsed() >= Duration::from_millis(80) {
                let item_total = totals[current_index];
                let item_downloaded = downloaded[current_index];
                let item_percent = item_total
                    .filter(|total| *total > 0)
                    .map(|total| item_downloaded as f64 * 100.0 / total as f64)
                    .unwrap_or(if completed[current_index] { 100.0 } else { 0.0 });
                let work_done = completed
                    .iter()
                    .enumerate()
                    .map(|(index, done)| {
                        if *done {
                            1.0
                        } else {
                            totals[index]
                                .filter(|total| *total > 0)
                                .map(|total| (downloaded[index] as f64 / total as f64).min(0.999))
                                .unwrap_or(0.0)
                        }
                    })
                    .sum::<f64>();
                let known_total_bytes = totals
                    .iter()
                    .copied()
                    .collect::<Option<Vec<_>>>()
                    .map(|values| values.into_iter().fold(0u64, u64::saturating_add));
                let elapsed = started_at.elapsed().as_secs_f64().max(0.001);
                progress(TransferProgress {
                    label: initial_labels[current_index].clone(),
                    total_percent: (work_done * 100.0 / total_files as f64).clamp(0.0, 100.0),
                    item_percent: item_percent.clamp(0.0, 100.0),
                    item_downloaded_bytes: item_downloaded,
                    item_total_bytes: item_total,
                    downloaded_bytes: downloaded.iter().copied().sum(),
                    total_bytes: known_total_bytes,
                    completed_files,
                    total_files,
                    active_downloads: active.len(),
                    bytes_per_second: (network_bytes as f64 / elapsed) as u64,
                });
                last_emit = Instant::now();
            }
        }
    });

    if let Some(error) = first_error {
        return Err(error);
    }
    Ok(DownloadBatchSummary {
        downloaded_files,
        cached_files,
    })
}

fn deduplicate_requests(
    requests: Vec<DownloadRequest>,
) -> Result<Vec<DownloadRequest>, DownloadError> {
    let mut destinations = HashMap::<PathBuf, (String, Option<ExpectedHash>, Option<u64>)>::new();
    let mut unique = Vec::with_capacity(requests.len());
    for request in requests {
        let signature = (
            request.url.clone(),
            request.expected_hash.clone(),
            request.expected_size,
        );
        if let Some(existing) = destinations.get(&request.destination) {
            if existing != &signature {
                return Err(DownloadError::ConflictingDestination(
                    request.destination.clone(),
                ));
            }
            continue;
        }
        destinations.insert(request.destination.clone(), signature);
        unique.push(request);
    }
    Ok(unique)
}

pub fn download_one_with_progress<F>(
    client: &Client,
    request: DownloadRequest,
    progress: F,
) -> Result<DownloadBatchSummary, DownloadError>
where
    F: FnMut(TransferProgress),
{
    download_many(client, vec![request], 1, progress)
}

fn download_one(
    client: &Client,
    index: usize,
    request: &DownloadRequest,
    sender: &mpsc::Sender<WorkerEvent>,
) -> Result<bool, DownloadError> {
    if request.destination.is_file()
        && request
            .expected_hash
            .as_ref()
            .map(|hash| hash_matches(&request.destination, hash))
            .unwrap_or(true)
    {
        let size = request
            .destination
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or_default();
        let _ = sender.send(WorkerEvent::Started {
            index,
            downloaded: size,
            total: Some(request.expected_size.unwrap_or(size)),
        });
        return Ok(true);
    }

    let parent = request
        .destination
        .parent()
        .ok_or_else(|| DownloadError::Io {
            path: request.destination.clone(),
            source: io::Error::new(io::ErrorKind::InvalidInput, "destino sem diretório pai"),
        })?;
    fs::create_dir_all(parent).map_err(|source| DownloadError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let temporary = request.destination.with_file_name(format!(
        "{}.aurora-download",
        request
            .destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("download")
    ));

    let mut last_error = None;
    for attempt in 0..MAX_ATTEMPTS {
        match download_attempt(client, index, request, &temporary, sender) {
            Ok(()) => {
                if let Some(expected) = &request.expected_hash {
                    if !hash_matches(&temporary, expected) {
                        let _ = fs::remove_file(&temporary);
                        last_error = Some(DownloadError::Integrity(request.destination.clone()));
                        if attempt + 1 < MAX_ATTEMPTS {
                            thread::sleep(Duration::from_millis(250 * (1 << attempt)));
                            continue;
                        }
                        break;
                    }
                }
                if request.destination.exists() {
                    fs::remove_file(&request.destination).map_err(|source| DownloadError::Io {
                        path: request.destination.clone(),
                        source,
                    })?;
                }
                fs::rename(&temporary, &request.destination).map_err(|source| {
                    DownloadError::Io {
                        path: request.destination.clone(),
                        source,
                    }
                })?;
                return Ok(false);
            }
            Err(error) => {
                last_error = Some(error);
                if attempt + 1 < MAX_ATTEMPTS {
                    thread::sleep(Duration::from_millis(250 * (1 << attempt)));
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| DownloadError::Integrity(request.destination.clone())))
}

fn download_attempt(
    client: &Client,
    index: usize,
    request: &DownloadRequest,
    temporary: &Path,
    sender: &mpsc::Sender<WorkerEvent>,
) -> Result<(), DownloadError> {
    let resumable = request.expected_hash.is_some();
    let resume_from = if resumable {
        temporary
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or_default()
    } else {
        0
    };
    let mut builder = client.get(&request.url);
    if resume_from > 0 {
        builder = builder.header(RANGE, format!("bytes={resume_from}-"));
    }
    let response = builder.send().map_err(|source| DownloadError::Network {
        url: request.url.clone(),
        source,
    })?;
    if resume_from > 0 && response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
        fs::remove_file(temporary).map_err(|source| DownloadError::Io {
            path: temporary.to_path_buf(),
            source,
        })?;
        return Err(DownloadError::Resume(request.destination.clone()));
    }
    let append = resume_from > 0 && response.status() == StatusCode::PARTIAL_CONTENT;
    let mut response = response
        .error_for_status()
        .map_err(|source| DownloadError::Network {
            url: request.url.clone(),
            source,
        })?;
    let starting_bytes = if append { resume_from } else { 0 };
    let response_length = response.content_length();
    let total = request
        .expected_size
        .or_else(|| response_length.map(|length| length.saturating_add(starting_bytes)));
    let _ = sender.send(WorkerEvent::Started {
        index,
        downloaded: starting_bytes,
        total,
    });

    let mut output = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(temporary)
        .map_err(|source| DownloadError::Io {
            path: temporary.to_path_buf(),
            source,
        })?;
    let mut buffer = vec![0u8; COPY_BUFFER_SIZE];
    let mut current = starting_bytes;
    loop {
        let size = response
            .read(&mut buffer)
            .map_err(|source| DownloadError::Io {
                path: temporary.to_path_buf(),
                source,
            })?;
        if size == 0 {
            break;
        }
        output
            .write_all(&buffer[..size])
            .map_err(|source| DownloadError::Io {
                path: temporary.to_path_buf(),
                source,
            })?;
        current = current.saturating_add(size as u64);
        let _ = sender.send(WorkerEvent::Bytes {
            index,
            downloaded: current,
            delta: size as u64,
        });
    }
    output.flush().map_err(|source| DownloadError::Io {
        path: temporary.to_path_buf(),
        source,
    })?;
    Ok(())
}

fn hash_matches(path: &Path, expected: &ExpectedHash) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut buffer = [0u8; 128 * 1024];
    match expected {
        ExpectedHash::Sha1(expected) => {
            let mut digest = Sha1::new();
            while let Ok(size) = file.read(&mut buffer) {
                if size == 0 {
                    return format!("{:x}", digest.finalize()).eq_ignore_ascii_case(expected);
                }
                digest.update(&buffer[..size]);
            }
        }
        ExpectedHash::Sha256(expected) => {
            let mut digest = Sha256::new();
            while let Ok(size) = file.read(&mut buffer) {
                if size == 0 {
                    return format!("{:x}", digest.finalize()).eq_ignore_ascii_case(expected);
                }
                digest.update(&buffer[..size]);
            }
        }
        ExpectedHash::Sha512(expected) => {
            let mut digest = Sha512::new();
            while let Ok(size) = file.read(&mut buffer) {
                if size == 0 {
                    return format!("{:x}", digest.finalize()).eq_ignore_ascii_case(expected);
                }
                digest.update(&buffer[..size]);
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn maps_batch_progress_into_install_range() {
        let progress = TransferProgress {
            total_percent: 50.0,
            ..TransferProgress::default()
        }
        .map_total(20.0, 80.0);
        assert_eq!(progress.total_percent, 50.0);
    }

    #[test]
    fn deduplicates_identical_destinations_before_workers_start() {
        let destination = PathBuf::from("assets/objects/e9/repeated-hash");
        let request = DownloadRequest {
            url: "https://resources.download.minecraft.net/e9/repeated-hash".to_owned(),
            destination,
            label: "primeiro nome do asset".to_owned(),
            expected_hash: Some(ExpectedHash::Sha1("repeated-hash".to_owned())),
            expected_size: Some(42),
        };
        let unique = deduplicate_requests(vec![request.clone(), request])
            .expect("assets idênticos devem compartilhar o mesmo download");
        assert_eq!(unique.len(), 1);
    }

    #[test]
    fn downloads_two_files_concurrently_and_validates_hashes() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("porta local de teste");
        let address = listener.local_addr().expect("endereço local");
        let active = Arc::new(AtomicUsize::new(0));
        let maximum_active = Arc::new(AtomicUsize::new(0));
        let server_active = Arc::clone(&active);
        let server_maximum = Arc::clone(&maximum_active);
        let server = thread::spawn(move || {
            let mut handlers = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("conexão do gerenciador");
                let active = Arc::clone(&server_active);
                let maximum = Arc::clone(&server_maximum);
                handlers.push(thread::spawn(move || {
                    let mut request = [0u8; 1024];
                    let _ = stream.read(&mut request);
                    let now_active = active.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum.fetch_max(now_active, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(100));
                    let body = b"aurora-parallel-download";
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .expect("cabeçalho HTTP");
                    stream.write_all(body).expect("corpo HTTP");
                    active.fetch_sub(1, Ordering::SeqCst);
                }));
            }
            for handler in handlers {
                handler.join().expect("servidor local");
            }
        });

        let test_directory = std::env::temp_dir().join(format!(
            "aurora-download-test-{}-{}",
            std::process::id(),
            address.port()
        ));
        fs::create_dir_all(&test_directory).expect("diretório temporário");
        let body_hash = "edc622815591d36915ea0aa93a4c20aa76ba85a5";
        let requests = (0..2)
            .map(|index| DownloadRequest {
                url: format!("http://{address}/file-{index}"),
                destination: test_directory.join(format!("file-{index}.bin")),
                label: format!("file-{index}.bin"),
                expected_hash: Some(ExpectedHash::Sha1(body_hash.to_owned())),
                expected_size: Some(24),
            })
            .collect();
        let client = Client::builder().build().expect("cliente HTTP local");
        let mut snapshots = Vec::new();
        let result = download_many(&client, requests, 2, |item| snapshots.push(item));
        server.join().expect("servidor finalizado");

        assert!(result.is_ok(), "o lote deve concluir: {result:?}");
        assert_eq!(maximum_active.load(Ordering::SeqCst), 2);
        assert_eq!(snapshots.last().map(|item| item.completed_files), Some(2));
        assert_eq!(
            fs::read(test_directory.join("file-0.bin")).expect("arquivo baixado"),
            b"aurora-parallel-download"
        );
        fs::remove_dir_all(test_directory).expect("limpeza do teste");
    }
}
