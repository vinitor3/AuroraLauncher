//! Detecção e troca transacional de conteúdo instalado.
//!
//! A identificação usa somente o SHA-1 calculado localmente. O arquivo do
//! jogador nunca é enviado ao catálogo: apenas o digest é consultado na API em
//! lote do Modrinth. Um plano é sempre recalculado imediatamente antes da
//! aplicação; os dados devolvidos ao frontend não são aceitos como autoridade.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::thread;
use std::time::Duration;

use reqwest::blocking::{Client, Response};
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use sha2::Sha512;
use thiserror::Error;
use uuid::Uuid;

use super::{
    install_modrinth_modpack_version_transactional_with_progress, Instance, ModpackInstallSummary,
    ModpackProgress,
};

const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const MAX_CONTENT_FILES: usize = 500;
const MAX_SELECTED_FILES: usize = 500;
const MAX_UPDATE_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_TRANSACTION_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_HTTP_ATTEMPTS: usize = 3;
const JOURNAL_VERSION: u32 = 1;
const MAX_SNAPSHOT_FILES: usize = 30_000;
const MAX_SNAPSHOT_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ContentUpdateError {
    #[error("tipo de conteúdo não suportado: {0}")]
    UnsupportedContentType(String),
    #[error("loader não suportado: {0}; use Forge ou Fabric")]
    UnsupportedLoader(String),
    #[error("nome de arquivo de conteúdo inválido: {0}")]
    UnsafeFilename(String),
    #[error("a instância possui arquivos demais nesta categoria (limite: {MAX_CONTENT_FILES})")]
    TooManyFiles,
    #[error("selecione entre 1 e {MAX_SELECTED_FILES} arquivos")]
    InvalidSelection,
    #[error("a instância já possui outra alteração de conteúdo em andamento")]
    Busy,
    #[error("o conteúdo mudou desde a verificação: {0}")]
    Stale(String),
    #[error("a atualização de {0} não é mais segura; verifique novamente")]
    NoLongerUpdateable(String),
    #[error("não há uma atualização concluída para desfazer")]
    NoRollback,
    #[error("a última atualização já foi desfeita")]
    AlreadyRolledBack,
    #[error("resposta inválida do catálogo Modrinth")]
    InvalidCatalogResponse,
    #[error("download inválido para {0}")]
    InvalidDownload(String),
    #[error("integridade inválida para {0}")]
    Integrity(String),
    #[error("falha de rede ao consultar ou baixar conteúdo: {0}")]
    Network(#[from] reqwest::Error),
    #[error("falha de E/S em {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("a troca de arquivos falhou e o estado anterior foi restaurado: {0}")]
    CommitRolledBack(String),
    #[error("a troca falhou e o rollback também precisa de atenção: {0}")]
    RollbackFailed(String),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentUpdateScan {
    pub minecraft_version: String,
    pub loader: String,
    pub content_type: String,
    pub items: Vec<ContentUpdateItem>,
    pub update_count: usize,
    pub unmanaged_count: usize,
    pub blocked_count: usize,
    pub rollback_available: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ContentUpdateStatus {
    UpdateAvailable,
    UpToDate,
    Unmanaged,
    Blocked,
    AuroraManaged,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentUpdateItem {
    pub filename: String,
    pub enabled: bool,
    pub current_sha1: String,
    pub status: ContentUpdateStatus,
    pub provider: Option<String>,
    pub title: Option<String>,
    pub icon_url: Option<String>,
    pub project_id: Option<String>,
    pub current_version_id: Option<String>,
    pub current_version_number: Option<String>,
    pub target_version_id: Option<String>,
    pub target_version_number: Option<String>,
    pub target_filename: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedContentUpdate {
    pub previous_filename: String,
    pub filename: String,
    pub project_id: String,
    pub previous_version_number: String,
    pub version_number: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentUpdateApplySummary {
    pub transaction_id: String,
    pub updated_files: Vec<AppliedContentUpdate>,
    pub rollback_available: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentUpdateRollbackSummary {
    pub transaction_id: String,
    pub restored_files: Vec<RestoredContentUpdate>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoredContentUpdate {
    pub replaced_filename: String,
    pub restored_filename: String,
    pub project_id: String,
    pub content_type: String,
}

/// Alvo já resolvido pelo Worker protegido (usado pelo CurseForge). O backend
/// não confia no nome nem na integridade declarada: caminho, contexto, bytes
/// atuais, tamanho, TLS e SHA-1 são revalidados antes do commit.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteContentUpdate {
    pub current_filename: String,
    pub current_sha1: String,
    pub target_filename: String,
    pub download_url: String,
    pub sha1: String,
    pub size: u64,
    pub project_id: String,
    pub current_version_id: Option<String>,
    pub current_version_number: Option<String>,
    pub target_version_id: String,
    pub target_version_number: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModpackUpdateStatus {
    UpdateAvailable,
    UpToDate,
    Unmanaged,
    Blocked,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModpackUpdateScan {
    pub status: ModpackUpdateStatus,
    pub source: Option<String>,
    pub project_id: Option<String>,
    /// Identificador numérico do arquivo CurseForge instalado. O ZIP original
    /// é descartado após a instalação, portanto este é o vínculo estável que
    /// o Worker usa para procurar uma versão posterior do mesmo projeto.
    pub current_file_id: Option<String>,
    pub current_version_id: Option<String>,
    pub current_version_number: Option<String>,
    pub target_version_id: Option<String>,
    pub target_version_number: Option<String>,
    pub minecraft_version: Option<String>,
    pub loader: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModpackUpdateApplySummary {
    pub transaction_id: String,
    pub previous_version_number: String,
    pub version_number: String,
    pub updated_file_count: usize,
    pub rollback_available: bool,
    pub preserved_extras: usize,
    pub modpack: ModpackInstallSummary,
}

#[derive(Clone, Copy)]
struct ContentSpec {
    folder: &'static str,
    extension: &'static str,
}

#[derive(Clone, Debug)]
struct LocalContent {
    filename: String,
    disk_name: String,
    enabled: bool,
    sha1: String,
}

#[derive(Clone, Debug)]
struct UpdateTarget {
    project_id: String,
    current_version_id: String,
    current_version_number: String,
    target_version_id: String,
    target_version_number: String,
    target_filename: String,
    download_url: String,
    sha1: String,
    sha512: Option<String>,
    size: u64,
    download_source: DownloadSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DownloadSource {
    Modrinth,
    AuroraWorker,
}

#[derive(Clone, Debug)]
struct ScannedContent {
    public: ContentUpdateItem,
    local: LocalContent,
    target: Option<UpdateTarget>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotEntry {
    relative: String,
    size: u64,
    sha1: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransactionJournal {
    schema_version: u32,
    transaction_id: String,
    state: TransactionState,
    content_type: String,
    minecraft_version: String,
    loader: String,
    committed_files: usize,
    files: Vec<TransactionFile>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum TransactionState {
    Prepared,
    Committing,
    Committed,
    RollingBack,
    RolledBack,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransactionFile {
    previous_filename: String,
    filename: String,
    enabled: bool,
    project_id: String,
    previous_version_number: String,
    version_number: String,
    current_sha1: Option<String>,
    target_sha1: String,
    target_sha512: Option<String>,
    expected_size: u64,
    #[serde(default = "default_true")]
    target_present: bool,
    source_relative: String,
    target_relative: String,
    staging_relative: String,
    backup_relative: String,
}

pub struct InstanceMutationGuard {
    _file: File,
    pub(super) root: PathBuf,
}

pub fn scan_content_updates(
    instance: &Instance,
    content_type: &str,
    minecraft_version: &str,
    loader: &str,
) -> Result<ContentUpdateScan, ContentUpdateError> {
    let scanned = scan_content_updates_internal(instance, content_type, minecraft_version, loader)?;
    let items = scanned
        .into_iter()
        .map(|item| item.public)
        .collect::<Vec<_>>();
    Ok(ContentUpdateScan {
        minecraft_version: minecraft_version.to_owned(),
        loader: loader.to_owned(),
        content_type: content_type.to_owned(),
        update_count: items
            .iter()
            .filter(|item| item.status == ContentUpdateStatus::UpdateAvailable)
            .count(),
        unmanaged_count: items
            .iter()
            .filter(|item| item.status == ContentUpdateStatus::Unmanaged)
            .count(),
        blocked_count: items
            .iter()
            .filter(|item| item.status == ContentUpdateStatus::Blocked)
            .count(),
        rollback_available: last_rollback_available(instance),
        items,
    })
}

pub fn apply_content_updates(
    instance: &Instance,
    content_type: &str,
    filenames: &[String],
    minecraft_version: &str,
    loader: &str,
) -> Result<ContentUpdateApplySummary, ContentUpdateError> {
    let spec = content_spec(content_type)?;
    validate_loader(loader)?;
    let selected = validate_selection(filenames, spec.extension)?;
    let _lock = acquire_instance_mutation_lock(instance)?;
    recover_incomplete_transactions(instance)?;

    // A confirmação da UI autoriza os nomes, não os dados remotos. O backend
    // refaz a consulta e exige que todos ainda sejam atualizáveis.
    let scan = scan_content_updates_internal(instance, content_type, minecraft_version, loader)?;
    let by_name = scan
        .into_iter()
        .map(|item| (item.local.filename.clone(), item))
        .collect::<HashMap<_, _>>();
    let mut chosen = Vec::with_capacity(selected.len());
    for filename in selected {
        let item = by_name
            .get(&filename)
            .ok_or_else(|| ContentUpdateError::NoLongerUpdateable(filename.clone()))?;
        if item.public.status != ContentUpdateStatus::UpdateAvailable {
            return Err(ContentUpdateError::NoLongerUpdateable(filename));
        }
        chosen.push(item.clone());
    }

    execute_prepared_updates(instance, content_type, minecraft_version, loader, chosen)
}

pub fn apply_remote_content_updates(
    instance: &Instance,
    content_type: &str,
    updates: &[RemoteContentUpdate],
    minecraft_version: &str,
    loader: &str,
) -> Result<ContentUpdateApplySummary, ContentUpdateError> {
    let spec = content_spec(content_type)?;
    validate_loader(loader)?;
    if updates.is_empty() || updates.len() > MAX_SELECTED_FILES {
        return Err(ContentUpdateError::InvalidSelection);
    }
    let _lock = acquire_instance_mutation_lock(instance)?;
    recover_incomplete_transactions(instance)?;
    let local = scan_local_content(instance, spec, content_type)?
        .into_iter()
        .map(|item| (item.filename.clone(), item))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut chosen = Vec::with_capacity(updates.len());
    for update in updates {
        validate_filename(&update.current_filename, spec.extension)?;
        validate_filename(&update.target_filename, spec.extension)?;
        if !seen.insert(update.current_filename.to_lowercase()) {
            return Err(ContentUpdateError::InvalidSelection);
        }
        if !valid_hex(&update.current_sha1, 40)
            || !valid_hex(&update.sha1, 40)
            || update.size == 0
            || update.size > MAX_UPDATE_FILE_BYTES
            || update.project_id.is_empty()
            || update.project_id.len() > 64
            || update.target_version_id.is_empty()
            || update.target_version_id.len() > 128
            || update.target_version_number.is_empty()
            || update.target_version_number.len() > 160
        {
            return Err(ContentUpdateError::InvalidCatalogResponse);
        }
        let parsed = Url::parse(&update.download_url)
            .map_err(|_| ContentUpdateError::InvalidDownload(update.target_filename.clone()))?;
        if !download_url_allowed(&parsed, DownloadSource::AuroraWorker) {
            return Err(ContentUpdateError::InvalidDownload(
                update.target_filename.clone(),
            ));
        }
        let current = local
            .get(&update.current_filename)
            .ok_or_else(|| ContentUpdateError::Stale(update.current_filename.clone()))?;
        if !current.sha1.eq_ignore_ascii_case(&update.current_sha1)
            || is_aurora_managed(content_type, &current.filename)
        {
            return Err(ContentUpdateError::Stale(update.current_filename.clone()));
        }
        let target = UpdateTarget {
            project_id: update.project_id.clone(),
            current_version_id: update.current_version_id.clone().unwrap_or_default(),
            current_version_number: update
                .current_version_number
                .clone()
                .unwrap_or_else(|| "versão instalada".to_owned()),
            target_version_id: update.target_version_id.clone(),
            target_version_number: update.target_version_number.clone(),
            target_filename: update.target_filename.clone(),
            download_url: parsed.to_string(),
            sha1: update.sha1.to_ascii_lowercase(),
            sha512: None,
            size: update.size,
            download_source: DownloadSource::AuroraWorker,
        };
        chosen.push(ScannedContent {
            public: ContentUpdateItem {
                filename: current.filename.clone(),
                enabled: current.enabled,
                current_sha1: current.sha1.clone(),
                status: ContentUpdateStatus::UpdateAvailable,
                provider: Some("curseforge".to_owned()),
                title: None,
                icon_url: None,
                project_id: Some(update.project_id.clone()),
                current_version_id: update.current_version_id.clone(),
                current_version_number: update.current_version_number.clone(),
                target_version_id: Some(update.target_version_id.clone()),
                target_version_number: Some(update.target_version_number.clone()),
                target_filename: Some(update.target_filename.clone()),
                reason: None,
            },
            local: current.clone(),
            target: Some(target),
        });
    }
    execute_prepared_updates(instance, content_type, minecraft_version, loader, chosen)
}

fn execute_prepared_updates(
    instance: &Instance,
    content_type: &str,
    minecraft_version: &str,
    loader: &str,
    chosen: Vec<ScannedContent>,
) -> Result<ContentUpdateApplySummary, ContentUpdateError> {
    let spec = content_spec(content_type)?;
    let transaction_id = Uuid::new_v4().simple().to_string();
    let transaction_root = transaction_root(instance).join(&transaction_id);
    let staging_root = transaction_root.join("staging");
    let backup_root = transaction_root.join("backup");
    fs::create_dir_all(&staging_root).map_err(|source| io_error(&staging_root, source))?;
    fs::create_dir_all(&backup_root).map_err(|source| io_error(&backup_root, source))?;
    ensure_no_symlink_components(
        instance.root(),
        &format!(".aurora/content-updates/{transaction_id}/staging"),
    )?;
    ensure_no_symlink_components(
        instance.root(),
        &format!(".aurora/content-updates/{transaction_id}/backup"),
    )?;

    let mut journal_files = Vec::with_capacity(chosen.len());
    let mut target_paths = HashSet::new();
    let source_paths = chosen
        .iter()
        .map(|item| item.local.disk_name.to_lowercase())
        .collect::<HashSet<_>>();
    for (index, item) in chosen.iter().enumerate() {
        let target = item
            .target
            .as_ref()
            .ok_or_else(|| ContentUpdateError::NoLongerUpdateable(item.local.filename.clone()))?;
        validate_filename(&target.target_filename, spec.extension)?;
        let proposed_target_disk_name = if item.local.enabled {
            target.target_filename.clone()
        } else {
            format!("{}.disabled", target.target_filename)
        };
        let target_disk_name =
            if proposed_target_disk_name.eq_ignore_ascii_case(&item.local.disk_name) {
                item.local.disk_name.clone()
            } else {
                proposed_target_disk_name
            };
        let normalized_target = target_disk_name.to_lowercase();
        if !target_paths.insert(normalized_target.clone()) {
            return Err(ContentUpdateError::UnsafeFilename(target_disk_name));
        }
        if !target_disk_name.eq_ignore_ascii_case(&item.local.disk_name)
            && source_paths.contains(&normalized_target)
        {
            return Err(ContentUpdateError::UnsafeFilename(target_disk_name));
        }
        let source_relative = relative_content_path(spec.folder, &item.local.disk_name)?;
        let target_relative = relative_content_path(spec.folder, &target_disk_name)?;
        let staging_relative = format!("staging/{index}.download");
        let backup_relative = format!("backup/{index}.previous");
        journal_files.push(TransactionFile {
            previous_filename: item.local.filename.clone(),
            filename: target.target_filename.clone(),
            enabled: item.local.enabled,
            project_id: target.project_id.clone(),
            previous_version_number: target.current_version_number.clone(),
            version_number: target.target_version_number.clone(),
            current_sha1: Some(item.local.sha1.clone()),
            target_sha1: target.sha1.clone(),
            target_sha512: target.sha512.clone(),
            expected_size: target.size,
            target_present: true,
            source_relative,
            target_relative,
            staging_relative,
            backup_relative,
        });
    }

    let total_download_bytes = journal_files
        .iter()
        .try_fold(0u64, |total, file| total.checked_add(file.expected_size))
        .ok_or(ContentUpdateError::InvalidSelection)?;
    if total_download_bytes > MAX_TRANSACTION_BYTES {
        return Err(ContentUpdateError::InvalidDownload(
            "a atualização selecionada ultrapassa 8 GiB".to_owned(),
        ));
    }

    preflight_transaction(instance, &journal_files)?;
    let download_source = chosen
        .first()
        .and_then(|item| item.target.as_ref())
        .map(|target| target.download_source)
        .ok_or(ContentUpdateError::InvalidSelection)?;
    if chosen.iter().any(|item| {
        item.target
            .as_ref()
            .is_none_or(|target| target.download_source != download_source)
    }) {
        return Err(ContentUpdateError::InvalidSelection);
    }
    let client = download_client(download_source)?;
    for (item, file) in chosen.iter().zip(&journal_files) {
        let target = item.target.as_ref().expect("target validado acima");
        let staging = checked_join(&transaction_root, &file.staging_relative)?;
        download_validated(
            &client,
            &target.download_url,
            &staging,
            file.expected_size,
            &file.target_sha1,
            file.target_sha512.as_deref(),
            target.download_source,
        )?;
    }

    let mut journal = TransactionJournal {
        schema_version: JOURNAL_VERSION,
        transaction_id: transaction_id.clone(),
        state: TransactionState::Prepared,
        content_type: content_type.to_owned(),
        minecraft_version: minecraft_version.to_owned(),
        loader: loader.to_owned(),
        committed_files: 0,
        files: journal_files,
    };
    write_journal(&transaction_root, &journal)?;
    preflight_transaction(instance, &journal.files)?;

    journal.state = TransactionState::Committing;
    write_journal(&transaction_root, &journal)?;
    for index in 0..journal.files.len() {
        // Grave a intenção antes do primeiro rename. Assim um encerramento
        // abrupto em qualquer janela de commit inclui este índice no recovery.
        journal.committed_files = index + 1;
        write_journal(&transaction_root, &journal)?;
        if let Err(error) = commit_one(instance, &transaction_root, &journal.files[index]) {
            journal.state = TransactionState::RollingBack;
            let _ = write_journal(&transaction_root, &journal);
            return match rollback_committed(instance, &transaction_root, &mut journal) {
                Ok(()) => Err(ContentUpdateError::CommitRolledBack(error.to_string())),
                Err(rollback) => Err(ContentUpdateError::RollbackFailed(format!(
                    "{error}; {rollback}"
                ))),
            };
        }
    }
    let publish_marker = transaction_root.join("publish-undo");
    if let Err(error) = atomic_write(&publish_marker, b"pending")
        .and_then(|()| {
            journal.state = TransactionState::Committed;
            write_journal(&transaction_root, &journal)
        })
        .and_then(|()| write_last_transaction_pointer(instance, &transaction_id))
    {
        journal.state = TransactionState::RollingBack;
        let _ = write_journal(&transaction_root, &journal);
        return match rollback_committed(instance, &transaction_root, &mut journal) {
            Ok(()) => {
                let _ = fs::remove_file(&publish_marker);
                Err(ContentUpdateError::CommitRolledBack(error.to_string()))
            }
            Err(rollback) => Err(ContentUpdateError::RollbackFailed(format!(
                "{error}; {rollback}"
            ))),
        };
    }
    let _ = fs::remove_file(&publish_marker);
    let _ = prune_old_transactions(instance, &transaction_id);

    Ok(ContentUpdateApplySummary {
        transaction_id,
        updated_files: journal
            .files
            .iter()
            .map(|file| AppliedContentUpdate {
                previous_filename: file.previous_filename.clone(),
                filename: file.filename.clone(),
                project_id: file.project_id.clone(),
                previous_version_number: file.previous_version_number.clone(),
                version_number: file.version_number.clone(),
            })
            .collect(),
        rollback_available: true,
    })
}

pub fn rollback_last_content_update(
    instance: &Instance,
) -> Result<ContentUpdateRollbackSummary, ContentUpdateError> {
    let _lock = acquire_instance_mutation_lock(instance)?;
    recover_incomplete_transactions(instance)?;
    let transaction_id = read_last_transaction_pointer(instance)?;
    let root = transaction_root(instance).join(&transaction_id);
    let mut journal = read_journal(&root)?;
    match journal.state {
        TransactionState::Committed => {}
        TransactionState::RolledBack => return Err(ContentUpdateError::AlreadyRolledBack),
        _ => return Err(ContentUpdateError::NoRollback),
    }

    // O undo também é fail-closed: nenhuma mutação começa se o usuário
    // alterou um arquivo depois da atualização.
    for file in &journal.files {
        let target = secure_instance_path(instance, &file.target_relative)?;
        let backup = checked_join(&root, &file.backup_relative)?;
        if file.target_present {
            verify_file(
                &target,
                Some(file.expected_size),
                &file.target_sha1,
                file.target_sha512.as_deref(),
            )?;
        } else if target.exists() {
            return Err(ContentUpdateError::Stale(file.filename.clone()));
        }
        if let Some(current_sha1) = file.current_sha1.as_deref() {
            verify_sha1(&backup, current_sha1)?;
        } else if backup.exists() {
            return Err(ContentUpdateError::Stale(file.previous_filename.clone()));
        }
        let source = secure_instance_path(instance, &file.source_relative)?;
        if source != target && source.exists() {
            return Err(ContentUpdateError::Stale(file.previous_filename.clone()));
        }
    }

    journal.state = TransactionState::RollingBack;
    write_journal(&root, &journal)?;
    rollback_committed(instance, &root, &mut journal)?;
    Ok(ContentUpdateRollbackSummary {
        transaction_id,
        restored_files: journal
            .files
            .iter()
            .map(|file| RestoredContentUpdate {
                replaced_filename: file.filename.clone(),
                restored_filename: file.previous_filename.clone(),
                project_id: file.project_id.clone(),
                content_type: journal.content_type.clone(),
            })
            .collect(),
    })
}

pub fn scan_modrinth_modpack_update(
    instance: &Instance,
) -> Result<ModpackUpdateScan, ContentUpdateError> {
    let marker_path = instance.root().join("aurora-modpack.json");
    let Ok(bytes) = fs::read(&marker_path) else {
        return Ok(unmanaged_modpack(
            "Esta instância não possui um modpack identificado.",
        ));
    };
    let marker: Value =
        serde_json::from_slice(&bytes).map_err(|_| ContentUpdateError::InvalidCatalogResponse)?;
    let source = marker["source"].as_str().map(str::to_owned);
    let project_id = value_id_as_string(&marker["projectId"]);
    let minecraft_version = marker["minecraft"].as_str().map(str::to_owned);
    let loader = marker["loader"].as_str().map(str::to_owned);
    if source.as_deref() != Some("modrinth") {
        let current_file_id = value_id_as_string(&marker["fileId"])
            .or_else(|| value_id_as_string(&marker["packVersionId"]));
        return Ok(ModpackUpdateScan {
            status: if source.is_some() {
                ModpackUpdateStatus::Blocked
            } else {
                ModpackUpdateStatus::Unmanaged
            },
            source,
            project_id,
            current_file_id,
            current_version_id: None,
            current_version_number: marker["versionNumber"].as_str().map(str::to_owned),
            target_version_id: None,
            target_version_number: None,
            minecraft_version,
            loader,
            reason: Some(
                "Atualização transacional de modpacks CurseForge exige o inventário protegido do Worker."
                    .to_owned(),
            ),
        });
    }
    let Some(project_id_value) = project_id.as_deref() else {
        return Ok(unmanaged_modpack(
            "O projeto do modpack não foi registrado.",
        ));
    };
    let Some(game) = minecraft_version.as_deref() else {
        return Ok(unmanaged_modpack(
            "A versão do Minecraft não foi registrada.",
        ));
    };
    let Some(loader_value) = loader.as_deref() else {
        return Ok(unmanaged_modpack("O loader do modpack não foi registrado."));
    };
    validate_loader(loader_value)?;
    let client = http_client()?;
    let versions = fetch_project_versions(&client, project_id_value)?;
    let compatible = compatible_versions(&versions, "modpack", game, loader_value);
    let current = locate_current_pack_version(&compatible, &marker);
    let Some(current) = current else {
        return Ok(ModpackUpdateScan {
            status: ModpackUpdateStatus::Blocked,
            source,
            project_id,
            current_file_id: None,
            current_version_id: marker["packVersionId"].as_str().map(str::to_owned),
            current_version_number: marker["versionNumber"]
                .as_str()
                .or_else(|| marker["versionName"].as_str())
                .map(str::to_owned),
            target_version_id: None,
            target_version_number: None,
            minecraft_version,
            loader,
            reason: Some(
                "Não foi possível provar qual versão do modpack está instalada.".to_owned(),
            ),
        });
    };
    let latest = latest_newer_release(&compatible, current);
    let current_id = current["id"].as_str().map(str::to_owned);
    let current_number = current["version_number"].as_str().map(str::to_owned);
    if let Some(target) = latest {
        if let Err(error) = pack_owned_files(&marker) {
            return Ok(ModpackUpdateScan {
                status: ModpackUpdateStatus::Blocked,
                source,
                project_id,
                current_file_id: None,
                current_version_id: current_id,
                current_version_number: current_number,
                target_version_id: target["id"].as_str().map(str::to_owned),
                target_version_number: target["version_number"].as_str().map(str::to_owned),
                minecraft_version,
                loader,
                reason: Some(format!(
                    "Esta instalação antiga não possui inventário seguro para remover arquivos substituídos: {error}"
                )),
            });
        }
        Ok(ModpackUpdateScan {
            status: ModpackUpdateStatus::UpdateAvailable,
            source,
            project_id,
            current_file_id: None,
            current_version_id: current_id,
            current_version_number: current_number,
            target_version_id: target["id"].as_str().map(str::to_owned),
            target_version_number: target["version_number"].as_str().map(str::to_owned),
            minecraft_version,
            loader,
            reason: None,
        })
    } else {
        Ok(ModpackUpdateScan {
            status: ModpackUpdateStatus::UpToDate,
            source,
            project_id,
            current_file_id: None,
            current_version_id: current_id,
            current_version_number: current_number,
            target_version_id: None,
            target_version_number: None,
            minecraft_version,
            loader,
            reason: None,
        })
    }
}

pub fn apply_modrinth_modpack_update_with_progress<F>(
    instance: &Instance,
    forge_java: Option<&Path>,
    mut progress: F,
) -> Result<ModpackUpdateApplySummary, ContentUpdateError>
where
    F: FnMut(ModpackProgress),
{
    let _lock = acquire_instance_mutation_lock(instance)?;
    recover_incomplete_transactions(instance)?;
    let scan = scan_modrinth_modpack_update(instance)?;
    if scan.status != ModpackUpdateStatus::UpdateAvailable
        || scan.source.as_deref() != Some("modrinth")
    {
        return Err(ContentUpdateError::NoLongerUpdateable("modpack".to_owned()));
    }
    let project_id = scan
        .project_id
        .as_deref()
        .ok_or(ContentUpdateError::InvalidCatalogResponse)?;
    let minecraft_version = scan
        .minecraft_version
        .as_deref()
        .ok_or(ContentUpdateError::InvalidCatalogResponse)?;
    let loader = scan
        .loader
        .as_deref()
        .ok_or(ContentUpdateError::InvalidCatalogResponse)?;
    let target_version_id = scan
        .target_version_id
        .as_deref()
        .ok_or(ContentUpdateError::InvalidCatalogResponse)?;
    let previous_version_number = scan
        .current_version_number
        .clone()
        .unwrap_or_else(|| "versão anterior".to_owned());
    let target_version_number = scan
        .target_version_number
        .clone()
        .ok_or(ContentUpdateError::InvalidCatalogResponse)?;
    let marker_path = instance.root().join("aurora-modpack.json");
    let marker_bytes = fs::read(&marker_path).map_err(|source| io_error(&marker_path, source))?;
    let marker: Value = serde_json::from_slice(&marker_bytes)
        .map_err(|_| ContentUpdateError::InvalidCatalogResponse)?;
    let current_owned = pack_owned_files(&marker)?;

    let transaction_id = Uuid::new_v4().simple().to_string();
    let root = transaction_root(instance).join(&transaction_id);
    let snapshot_root = root.join("snapshot");
    fs::create_dir_all(&snapshot_root).map_err(|source| io_error(&snapshot_root, source))?;
    ensure_no_symlink_components(
        instance.root(),
        &format!(".aurora/content-updates/{transaction_id}/snapshot"),
    )?;
    let mut before = None;
    let mut planned_paths = None;
    let mut old_only = None;
    let install_result = install_modrinth_modpack_version_transactional_with_progress(
        instance,
        project_id,
        minecraft_version,
        loader,
        target_version_id,
        forge_java,
        |paths| {
            let target_paths = normalize_planned_paths(paths)
                .map_err(|error| super::ModpackError::UpdatePreparation(error.to_string()))?;
            let target_keys = target_paths
                .iter()
                .map(|path| path.to_lowercase())
                .collect::<HashSet<_>>();
            let removed = current_owned
                .iter()
                .filter(|entry| !target_keys.contains(&entry.relative.to_lowercase()))
                .cloned()
                .collect::<Vec<_>>();
            for entry in &removed {
                let path = secure_instance_path(instance, &entry.relative)
                    .map_err(|error| super::ModpackError::UpdatePreparation(error.to_string()))?;
                verify_file(&path, Some(entry.size), &entry.sha1, None)
                    .map_err(|error| super::ModpackError::UpdatePreparation(error.to_string()))?;
            }
            let mut union = target_paths;
            union.extend(current_owned.iter().map(|entry| entry.relative.clone()));
            let union_paths = union.iter().map(PathBuf::from).collect::<Vec<_>>();
            let normalized = normalize_planned_paths(&union_paths)
                .map_err(|error| super::ModpackError::UpdatePreparation(error.to_string()))?;
            let snapshot = snapshot_instance_paths(instance, &snapshot_root, &normalized)
                .map_err(|error| super::ModpackError::UpdatePreparation(error.to_string()))?;
            let snapshot_bytes = serde_json::to_vec_pretty(&snapshot)
                .map_err(|_| super::ModpackError::InvalidResponse)?;
            let paths_bytes = serde_json::to_vec_pretty(&normalized)
                .map_err(|_| super::ModpackError::InvalidResponse)?;
            atomic_write(&root.join("pack-snapshot.json"), &snapshot_bytes)
                .map_err(|error| super::ModpackError::UpdatePreparation(error.to_string()))?;
            atomic_write(&root.join("pack-paths.json"), &paths_bytes)
                .map_err(|error| super::ModpackError::UpdatePreparation(error.to_string()))?;
            before = Some(snapshot);
            planned_paths = Some(normalized);
            old_only = Some(removed);
            atomic_write(&root.join("pack-in-progress"), b"installing")
                .map_err(|error| super::ModpackError::UpdatePreparation(error.to_string()))
        },
        &mut progress,
    );
    let modpack = match install_result {
        Ok(summary) => summary,
        Err(error) => {
            return if let (Some(before), Some(paths)) = (&before, &planned_paths) {
                match rollback_pack_snapshot(instance, &root, before, paths) {
                    Ok(()) => Err(ContentUpdateError::CommitRolledBack(error.to_string())),
                    Err(rollback) => Err(ContentUpdateError::RollbackFailed(format!(
                        "{error}; {rollback}"
                    ))),
                }
            } else {
                Err(ContentUpdateError::InvalidDownload(error.to_string()))
            };
        }
    };
    let before = before.ok_or(ContentUpdateError::InvalidCatalogResponse)?;
    let planned_paths = planned_paths.ok_or(ContentUpdateError::InvalidCatalogResponse)?;
    let old_only = old_only.ok_or(ContentUpdateError::InvalidCatalogResponse)?;

    let finalized = finalize_modpack_transaction(
        instance,
        &root,
        &snapshot_root,
        &transaction_id,
        project_id,
        &previous_version_number,
        &target_version_number,
        minecraft_version,
        loader,
        &before,
        &planned_paths,
        &old_only,
    );
    let (updated_file_count, preserved_extras) = match finalized {
        Ok(summary) => summary,
        Err(error) => {
            return match rollback_pack_snapshot(instance, &root, &before, &planned_paths) {
                Ok(()) => Err(ContentUpdateError::CommitRolledBack(error.to_string())),
                Err(rollback) => Err(ContentUpdateError::RollbackFailed(format!(
                    "{error}; {rollback}"
                ))),
            };
        }
    };
    let _ = prune_old_transactions(instance, &transaction_id);

    Ok(ModpackUpdateApplySummary {
        transaction_id,
        previous_version_number,
        version_number: target_version_number,
        updated_file_count,
        rollback_available: true,
        preserved_extras,
        modpack,
    })
}

#[allow(clippy::too_many_arguments)]
fn finalize_modpack_transaction(
    instance: &Instance,
    root: &Path,
    snapshot_root: &Path,
    transaction_id: &str,
    project_id: &str,
    previous_version_number: &str,
    target_version_number: &str,
    minecraft_version: &str,
    loader: &str,
    before: &[SnapshotEntry],
    planned_paths: &[String],
    old_only: &[SnapshotEntry],
) -> Result<(usize, usize), ContentUpdateError> {
    for entry in old_only {
        let path = secure_instance_path(instance, &entry.relative)?;
        verify_file(&path, Some(entry.size), &entry.sha1, None)?;
        fs::remove_file(&path).map_err(|source| io_error(&path, source))?;
    }

    let after = collect_instance_paths(instance, planned_paths)?;
    let before_by_path = before
        .iter()
        .map(|entry| (entry.relative.clone(), entry))
        .collect::<HashMap<_, _>>();
    let after_by_path = after
        .iter()
        .map(|entry| (entry.relative.clone(), entry))
        .collect::<HashMap<_, _>>();
    let mut files = Vec::new();
    let mut preserved_extras = 0usize;
    for relative in planned_paths {
        match (before_by_path.get(relative), after_by_path.get(relative)) {
            (Some(previous), Some(current)) if previous.sha1 == current.sha1 => {
                preserved_extras += 1;
            }
            (previous, Some(current)) => {
                files.push(TransactionFile {
                    previous_filename: relative.clone(),
                    filename: relative.clone(),
                    enabled: true,
                    project_id: project_id.to_owned(),
                    previous_version_number: previous_version_number.to_owned(),
                    version_number: target_version_number.to_owned(),
                    current_sha1: previous.map(|entry| entry.sha1.clone()),
                    target_sha1: current.sha1.clone(),
                    target_sha512: None,
                    expected_size: current.size,
                    target_present: true,
                    source_relative: relative.clone(),
                    target_relative: relative.clone(),
                    staging_relative: "unused".to_owned(),
                    backup_relative: previous
                        .map(|_| format!("snapshot/{relative}"))
                        .unwrap_or_else(|| format!("new/{relative}")),
                });
            }
            (Some(previous), None) => {
                let declared_removed = old_only
                    .iter()
                    .any(|entry| entry.relative.to_lowercase() == relative.to_lowercase());
                if !declared_removed {
                    let destination = secure_instance_path(instance, relative)?;
                    restore_snapshot_file(snapshot_root, previous, &destination)?;
                    preserved_extras += 1;
                    continue;
                }
                files.push(TransactionFile {
                    previous_filename: relative.clone(),
                    filename: relative.clone(),
                    enabled: true,
                    project_id: project_id.to_owned(),
                    previous_version_number: previous_version_number.to_owned(),
                    version_number: target_version_number.to_owned(),
                    current_sha1: Some(previous.sha1.clone()),
                    target_sha1: String::new(),
                    target_sha512: None,
                    expected_size: 0,
                    target_present: false,
                    source_relative: relative.clone(),
                    target_relative: relative.clone(),
                    staging_relative: "unused".to_owned(),
                    backup_relative: format!("snapshot/{relative}"),
                });
            }
            (None, None) => {}
        }
    }
    if files.is_empty() {
        return Err(ContentUpdateError::NoLongerUpdateable("modpack".to_owned()));
    }
    let updated_file_count = files.len();
    let mut journal = TransactionJournal {
        schema_version: JOURNAL_VERSION,
        transaction_id: transaction_id.to_owned(),
        state: TransactionState::Committing,
        content_type: "modpack".to_owned(),
        minecraft_version: minecraft_version.to_owned(),
        loader: loader.to_owned(),
        committed_files: files.len(),
        files,
    };
    write_journal(root, &journal)?;
    let publish_marker = root.join("publish-undo");
    atomic_write(&publish_marker, b"pending")?;
    let pack_marker = root.join("pack-in-progress");
    fs::remove_file(&pack_marker).map_err(|source| io_error(&pack_marker, source))?;
    journal.state = TransactionState::Committed;
    write_journal(root, &journal)?;
    write_last_transaction_pointer(instance, transaction_id)?;
    let _ = fs::remove_file(&publish_marker);
    Ok((updated_file_count, preserved_extras))
}

fn scan_content_updates_internal(
    instance: &Instance,
    content_type: &str,
    minecraft_version: &str,
    loader: &str,
) -> Result<Vec<ScannedContent>, ContentUpdateError> {
    let spec = content_spec(content_type)?;
    validate_loader(loader)?;
    if minecraft_version.trim().is_empty() || minecraft_version.len() > 32 {
        return Err(ContentUpdateError::InvalidCatalogResponse);
    }
    let local = scan_local_content(instance, spec, content_type)?;
    if local.is_empty() {
        return Ok(Vec::new());
    }
    let client = http_client()?;
    let identified = identify_versions(&client, &local)?;
    // O endpoint oficial escolhe, em lote, a versão mais recente do mesmo
    // projeto que satisfaz o contexto pedido. A resposta ainda é validada
    // abaixo; não confiamos que um alvo incompatível seja seguro só porque o
    // catálogo o devolveu.
    let updates = identify_updates(&client, &local, content_type, minecraft_version, loader)?;
    let mut installed_by_project = HashMap::new();
    let mut duplicate_projects = HashSet::new();
    for item in local.iter().filter(|item| item.enabled) {
        let Some(version) = identified.get(&item.sha1) else {
            continue;
        };
        let (Some(project_id), Some(version_id)) =
            (version["project_id"].as_str(), version["id"].as_str())
        else {
            continue;
        };
        if installed_by_project
            .insert(project_id.to_owned(), version_id.to_owned())
            .is_some()
        {
            duplicate_projects.insert(project_id.to_owned());
        }
    }
    let mut result = Vec::with_capacity(local.len());
    for item in local {
        if is_aurora_managed(content_type, &item.filename) {
            result.push(scanned_public(
                item,
                ContentUpdateStatus::AuroraManaged,
                None,
                Some("Este componente é atualizado junto com o Aurora.".to_owned()),
            ));
            continue;
        }
        let Some(current) = identified.get(&item.sha1) else {
            result.push(scanned_public(
                item,
                ContentUpdateStatus::Unmanaged,
                None,
                Some("O SHA-1 não corresponde a uma versão publicada no Modrinth.".to_owned()),
            ));
            continue;
        };
        let Some(project_id) = current["project_id"].as_str() else {
            result.push(scanned_public(
                item,
                ContentUpdateStatus::Blocked,
                None,
                Some("O catálogo não informou o projeto desta versão.".to_owned()),
            ));
            continue;
        };
        if duplicate_projects.contains(project_id) {
            result.push(scanned_public(
                item,
                ContentUpdateStatus::Blocked,
                None,
                Some(
                    "Há mais de uma versão habilitada deste projeto; desative a duplicata antes de atualizar."
                        .to_owned(),
                ),
            ));
            continue;
        }
        let current_id = current["id"].as_str().unwrap_or_default().to_owned();
        let current_number = current["version_number"]
            .as_str()
            .unwrap_or("versão instalada")
            .to_owned();
        let title = current["name"].as_str().map(str::to_owned);
        let Some(target_version) = updates.get(&item.sha1) else {
            let (status, reason) =
                if version_is_compatible(current, content_type, minecraft_version, loader) {
                    (ContentUpdateStatus::UpToDate, None)
                } else {
                    (
                        ContentUpdateStatus::Blocked,
                        Some(
                            "Não existe uma release compatível com este Minecraft e loader."
                                .to_owned(),
                        ),
                    )
                };
            result.push(ScannedContent {
                public: ContentUpdateItem {
                    filename: item.filename.clone(),
                    enabled: item.enabled,
                    current_sha1: item.sha1.clone(),
                    status,
                    provider: Some("modrinth".to_owned()),
                    title,
                    icon_url: None,
                    project_id: Some(project_id.to_owned()),
                    current_version_id: Some(current_id),
                    current_version_number: Some(current_number),
                    target_version_id: None,
                    target_version_number: None,
                    target_filename: None,
                    reason,
                },
                local: item,
                target: None,
            });
            continue;
        };
        let same_version = target_version["id"].as_str() == current["id"].as_str();
        let strictly_newer = target_version["date_published"]
            .as_str()
            .zip(current["date_published"].as_str())
            .is_some_and(|(target, installed)| target > installed);
        if same_version {
            result.push(ScannedContent {
                public: ContentUpdateItem {
                    filename: item.filename.clone(),
                    enabled: item.enabled,
                    current_sha1: item.sha1.clone(),
                    status: ContentUpdateStatus::UpToDate,
                    provider: Some("modrinth".to_owned()),
                    title,
                    icon_url: None,
                    project_id: Some(project_id.to_owned()),
                    current_version_id: Some(current_id),
                    current_version_number: Some(current_number),
                    target_version_id: None,
                    target_version_number: None,
                    target_filename: None,
                    reason: None,
                },
                local: item,
                target: None,
            });
            continue;
        }
        if !strictly_newer {
            result.push(ScannedContent {
                public: ContentUpdateItem {
                    filename: item.filename.clone(),
                    enabled: item.enabled,
                    current_sha1: item.sha1.clone(),
                    status: ContentUpdateStatus::Blocked,
                    provider: Some("modrinth".to_owned()),
                    title,
                    icon_url: None,
                    project_id: Some(project_id.to_owned()),
                    current_version_id: Some(current_id),
                    current_version_number: Some(current_number),
                    target_version_id: None,
                    target_version_number: None,
                    target_filename: None,
                    reason: Some(
                        "O catálogo devolveu outra versão sem provar que ela é posterior; nenhuma troca foi planejada."
                            .to_owned(),
                    ),
                },
                local: item,
                target: None,
            });
            continue;
        }
        if !version_is_compatible(target_version, content_type, minecraft_version, loader)
            || target_version["project_id"].as_str() != Some(project_id)
        {
            result.push(ScannedContent {
                public: ContentUpdateItem {
                    filename: item.filename.clone(),
                    enabled: item.enabled,
                    current_sha1: item.sha1.clone(),
                    status: ContentUpdateStatus::Blocked,
                    provider: Some("modrinth".to_owned()),
                    title,
                    icon_url: None,
                    project_id: Some(project_id.to_owned()),
                    current_version_id: Some(current_id),
                    current_version_number: Some(current_number),
                    target_version_id: None,
                    target_version_number: None,
                    target_filename: None,
                    reason: Some(
                        "O catálogo devolveu um alvo fora do Minecraft, loader ou projeto atual."
                            .to_owned(),
                    ),
                },
                local: item,
                target: None,
            });
            continue;
        }
        if !required_dependencies_satisfied(
            target_version,
            &installed_by_project,
            &duplicate_projects,
        ) {
            result.push(ScannedContent {
                public: ContentUpdateItem {
                    filename: item.filename.clone(),
                    enabled: item.enabled,
                    current_sha1: item.sha1.clone(),
                    status: ContentUpdateStatus::Blocked,
                    provider: Some("modrinth".to_owned()),
                    title,
                    icon_url: None,
                    project_id: Some(project_id.to_owned()),
                    current_version_id: Some(current_id),
                    current_version_number: Some(current_number),
                    target_version_id: target_version["id"].as_str().map(str::to_owned),
                    target_version_number: target_version["version_number"]
                        .as_str()
                        .map(str::to_owned),
                    target_filename: None,
                    reason: Some(
                        "A nova versão altera dependências obrigatórias; instale ou atualize essas dependências antes."
                            .to_owned(),
                    ),
                },
                local: item,
                target: None,
            });
            continue;
        }
        match update_target(project_id, current, target_version, spec.extension) {
            Ok(target) => {
                result.push(ScannedContent {
                    public: ContentUpdateItem {
                        filename: item.filename.clone(),
                        enabled: item.enabled,
                        current_sha1: item.sha1.clone(),
                        status: ContentUpdateStatus::UpdateAvailable,
                        provider: Some("modrinth".to_owned()),
                        title,
                        icon_url: None,
                        project_id: Some(project_id.to_owned()),
                        current_version_id: Some(target.current_version_id.clone()),
                        current_version_number: Some(target.current_version_number.clone()),
                        target_version_id: Some(target.target_version_id.clone()),
                        target_version_number: Some(target.target_version_number.clone()),
                        target_filename: Some(target.target_filename.clone()),
                        reason: None,
                    },
                    local: item,
                    target: Some(target),
                });
            }
            Err(error) => {
                result.push(scanned_public(
                    item,
                    ContentUpdateStatus::Blocked,
                    None,
                    Some(error.to_string()),
                ));
            }
        }
    }
    result.sort_by(|left, right| left.local.filename.cmp(&right.local.filename));
    Ok(result)
}

fn scanned_public(
    item: LocalContent,
    status: ContentUpdateStatus,
    target: Option<UpdateTarget>,
    reason: Option<String>,
) -> ScannedContent {
    ScannedContent {
        public: ContentUpdateItem {
            filename: item.filename.clone(),
            enabled: item.enabled,
            current_sha1: item.sha1.clone(),
            status,
            provider: None,
            title: None,
            icon_url: None,
            project_id: target.as_ref().map(|value| value.project_id.clone()),
            current_version_id: target
                .as_ref()
                .map(|value| value.current_version_id.clone()),
            current_version_number: target
                .as_ref()
                .map(|value| value.current_version_number.clone()),
            target_version_id: target.as_ref().map(|value| value.target_version_id.clone()),
            target_version_number: target
                .as_ref()
                .map(|value| value.target_version_number.clone()),
            target_filename: target.as_ref().map(|value| value.target_filename.clone()),
            reason,
        },
        local: item,
        target,
    }
}

fn content_spec(content_type: &str) -> Result<ContentSpec, ContentUpdateError> {
    match content_type {
        "mod" => Ok(ContentSpec {
            folder: "mods",
            extension: ".jar",
        }),
        "shader" => Ok(ContentSpec {
            folder: "shaderpacks",
            extension: ".zip",
        }),
        "resourcepack" => Ok(ContentSpec {
            folder: "resourcepacks",
            extension: ".zip",
        }),
        other => Err(ContentUpdateError::UnsupportedContentType(other.to_owned())),
    }
}

fn validate_loader(loader: &str) -> Result<(), ContentUpdateError> {
    if matches!(loader, "fabric" | "forge") {
        Ok(())
    } else {
        Err(ContentUpdateError::UnsupportedLoader(loader.to_owned()))
    }
}

fn validate_selection(
    filenames: &[String],
    extension: &str,
) -> Result<Vec<String>, ContentUpdateError> {
    if filenames.is_empty() || filenames.len() > MAX_SELECTED_FILES {
        return Err(ContentUpdateError::InvalidSelection);
    }
    let mut unique = BTreeSet::new();
    let mut insensitive = HashSet::new();
    for filename in filenames {
        validate_filename(filename, extension)?;
        if !insensitive.insert(filename.to_lowercase()) {
            return Err(ContentUpdateError::InvalidSelection);
        }
        unique.insert(filename.clone());
    }
    Ok(unique.into_iter().collect())
}

fn validate_filename(filename: &str, extension: &str) -> Result<(), ContentUpdateError> {
    if filename.is_empty()
        || filename.len() > 255
        || filename.contains(['/', '\\'])
        || !windows_component_safe(filename)
        || filename == "."
        || filename == ".."
        || !filename.to_ascii_lowercase().ends_with(extension)
        || Path::new(filename)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(filename)
    {
        return Err(ContentUpdateError::UnsafeFilename(filename.to_owned()));
    }
    Ok(())
}

fn windows_component_safe(value: &str) -> bool {
    if value.is_empty()
        || value.ends_with(['.', ' '])
        || value
            .chars()
            .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
    {
        return false;
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

fn scan_local_content(
    instance: &Instance,
    spec: ContentSpec,
    content_type: &str,
) -> Result<Vec<LocalContent>, ContentUpdateError> {
    ensure_instance_root_secure(instance)?;
    let directory = instance.root().join(spec.folder);
    ensure_no_symlink_components(instance.root(), spec.folder)?;
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&directory).map_err(|source| io_error(&directory, source))? {
        let entry = entry.map_err(|source| io_error(&directory, source))?;
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|source| io_error(&entry.path(), source))?;
        if metadata_is_link(&metadata) {
            return Err(ContentUpdateError::UnsafeFilename(
                entry.path().display().to_string(),
            ));
        }
        let disk_name = entry.file_name().to_string_lossy().into_owned();
        let content_name = disk_name.strip_suffix(".disabled").unwrap_or(&disk_name);
        if metadata.is_file() && content_name.to_ascii_lowercase().ends_with(spec.extension) {
            entries.push(entry);
        }
    }
    if entries.len() > MAX_CONTENT_FILES {
        return Err(ContentUpdateError::TooManyFiles);
    }
    entries.sort_by_key(|entry| entry.file_name());
    let mut result = Vec::new();
    for entry in entries {
        let disk_name = entry.file_name().to_string_lossy().into_owned();
        let enabled = !disk_name.ends_with(".disabled");
        let filename = disk_name
            .strip_suffix(".disabled")
            .unwrap_or(&disk_name)
            .to_owned();
        if !filename.to_ascii_lowercase().ends_with(spec.extension) {
            continue;
        }
        validate_filename(&filename, spec.extension)?;
        let sha1 = hash_sha1(&entry.path())?;
        result.push(LocalContent {
            filename,
            disk_name,
            enabled,
            sha1,
        });
    }
    // Evita silenciar um erro de chamada por um tipo futuro acidental.
    let _ = content_type;
    Ok(result)
}

fn is_aurora_managed(content_type: &str, filename: &str) -> bool {
    if content_type != "mod" {
        return false;
    }
    let normalized = filename.to_ascii_lowercase();
    normalized.starts_with("aurora-core-")
        || normalized.starts_with("aurora-skins-")
        || normalized.starts_with("aurora-companion-")
}

fn identify_versions(
    client: &Client,
    local: &[LocalContent],
) -> Result<HashMap<String, Value>, ContentUpdateError> {
    let mut result = HashMap::new();
    for batch in local.chunks(100) {
        let hashes = batch
            .iter()
            .map(|item| item.sha1.clone())
            .collect::<Vec<_>>();
        if hashes.is_empty() {
            continue;
        }
        let response: Value = client
            .post(format!("{MODRINTH_API}/version_files"))
            .json(&serde_json::json!({ "hashes": hashes, "algorithm": "sha1" }))
            .send()?
            .error_for_status()?
            .json()?;
        let versions = response
            .as_object()
            .ok_or(ContentUpdateError::InvalidCatalogResponse)?;
        for (hash, version) in versions {
            result.insert(hash.to_ascii_lowercase(), version.clone());
        }
    }
    Ok(result)
}

fn identify_updates(
    client: &Client,
    local: &[LocalContent],
    content_type: &str,
    minecraft_version: &str,
    loader: &str,
) -> Result<HashMap<String, Value>, ContentUpdateError> {
    let mut result = HashMap::new();
    for batch in local.chunks(100) {
        let hashes = batch
            .iter()
            .map(|item| item.sha1.clone())
            .collect::<Vec<_>>();
        if hashes.is_empty() {
            continue;
        }
        let mut body = serde_json::json!({
            "hashes": hashes,
            "algorithm": "sha1",
            "game_versions": [minecraft_version],
            "version_types": ["release"],
        });
        // O batch exige loaders do tipo de projeto. Mods usam exatamente o
        // loader da instância; os demais usam os tags oficiais do catálogo.
        body["loaders"] = match content_type {
            "mod" => serde_json::json!([loader]),
            "resourcepack" => serde_json::json!(["minecraft"]),
            "shader" => serde_json::json!(["canvas", "iris", "optifine", "vanilla"]),
            _ => {
                return Err(ContentUpdateError::UnsupportedContentType(
                    content_type.to_owned(),
                ))
            }
        };
        let response: Value = client
            .post(format!("{MODRINTH_API}/version_files/update"))
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?;
        let versions = response
            .as_object()
            .ok_or(ContentUpdateError::InvalidCatalogResponse)?;
        for (hash, version) in versions {
            result.insert(hash.to_ascii_lowercase(), version.clone());
        }
    }
    Ok(result)
}

fn fetch_project_versions(client: &Client, project_id: &str) -> Result<Value, ContentUpdateError> {
    if project_id.is_empty()
        || project_id.len() > 64
        || !project_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ContentUpdateError::InvalidCatalogResponse);
    }
    let response: Value = client
        .get(format!("{MODRINTH_API}/project/{project_id}/version"))
        .send()?
        .error_for_status()?
        .json()?;
    if !response.is_array() {
        return Err(ContentUpdateError::InvalidCatalogResponse);
    }
    Ok(response)
}

fn compatible_versions<'a>(
    versions: &'a Value,
    content_type: &str,
    minecraft_version: &str,
    loader: &str,
) -> Vec<&'a Value> {
    versions
        .as_array()
        .into_iter()
        .flatten()
        .filter(|version| version_is_compatible(version, content_type, minecraft_version, loader))
        .collect()
}

fn version_is_compatible(
    version: &Value,
    content_type: &str,
    minecraft_version: &str,
    loader: &str,
) -> bool {
    version["version_type"].as_str() == Some("release")
        && version["game_versions"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item.as_str() == Some(minecraft_version))
        })
        && version["loaders"]
            .as_array()
            .is_some_and(|items| match content_type {
                "mod" | "modpack" => items.iter().any(|item| item.as_str() == Some(loader)),
                "resourcepack" => items.iter().any(|item| item.as_str() == Some("minecraft")),
                "shader" => items.iter().any(|item| {
                    matches!(
                        item.as_str(),
                        Some("canvas" | "iris" | "optifine" | "vanilla")
                    )
                }),
                _ => false,
            })
}

fn latest_newer_release<'a>(compatible: &[&'a Value], current: &Value) -> Option<&'a Value> {
    let current_id = current["id"].as_str()?;
    let current_published = current["date_published"].as_str()?;
    compatible
        .iter()
        .copied()
        .filter(|version| version["id"].as_str().is_some_and(|id| id != current_id))
        .filter(|version| {
            version["date_published"]
                .as_str()
                .is_some_and(|published| published > current_published)
        })
        .max_by(|left, right| {
            left["date_published"]
                .as_str()
                .cmp(&right["date_published"].as_str())
                .then_with(|| left["id"].as_str().cmp(&right["id"].as_str()))
        })
}

fn required_dependencies_satisfied(
    target: &Value,
    installed_by_project: &HashMap<String, String>,
    duplicate_projects: &HashSet<String>,
) -> bool {
    target["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|dependency| dependency["dependency_type"].as_str() == Some("required"))
        .all(|dependency| {
            let project = dependency["project_id"].as_str();
            let version = dependency["version_id"].as_str();
            match (project, version) {
                (Some(project), _) if duplicate_projects.contains(project) => false,
                (Some(project), Some(version)) => installed_by_project
                    .get(project)
                    .is_some_and(|installed| installed == version),
                (Some(project), None) => installed_by_project.contains_key(project),
                // Alguns registros antigos trazem apenas version_id. Sem outra
                // consulta não é possível provar que a dependência está presente.
                (None, Some(version)) => installed_by_project
                    .values()
                    .any(|candidate| candidate == version),
                (None, None) => false,
            }
        })
}

fn update_target(
    project_id: &str,
    current: &Value,
    target: &Value,
    extension: &str,
) -> Result<UpdateTarget, ContentUpdateError> {
    let file = primary_file(target, extension)?;
    let target_filename = file["filename"]
        .as_str()
        .ok_or(ContentUpdateError::InvalidCatalogResponse)?;
    validate_filename(target_filename, extension)?;
    let download_url = file["url"]
        .as_str()
        .ok_or_else(|| ContentUpdateError::InvalidDownload(target_filename.to_owned()))?;
    let parsed = Url::parse(download_url)
        .map_err(|_| ContentUpdateError::InvalidDownload(target_filename.to_owned()))?;
    if !download_url_allowed(&parsed, DownloadSource::Modrinth) {
        return Err(ContentUpdateError::InvalidDownload(
            target_filename.to_owned(),
        ));
    }
    let sha1 = file
        .pointer("/hashes/sha1")
        .and_then(Value::as_str)
        .filter(|value| valid_hex(value, 40))
        .ok_or_else(|| ContentUpdateError::Integrity(target_filename.to_owned()))?;
    let sha512 = file
        .pointer("/hashes/sha512")
        .and_then(Value::as_str)
        .filter(|value| valid_hex(value, 128))
        .ok_or_else(|| ContentUpdateError::Integrity(target_filename.to_owned()))?;
    let size = file["size"]
        .as_u64()
        .filter(|size| *size > 0 && *size <= MAX_UPDATE_FILE_BYTES)
        .ok_or_else(|| ContentUpdateError::InvalidDownload(target_filename.to_owned()))?;
    Ok(UpdateTarget {
        project_id: project_id.to_owned(),
        current_version_id: current["id"]
            .as_str()
            .ok_or(ContentUpdateError::InvalidCatalogResponse)?
            .to_owned(),
        current_version_number: current["version_number"]
            .as_str()
            .ok_or(ContentUpdateError::InvalidCatalogResponse)?
            .to_owned(),
        target_version_id: target["id"]
            .as_str()
            .ok_or(ContentUpdateError::InvalidCatalogResponse)?
            .to_owned(),
        target_version_number: target["version_number"]
            .as_str()
            .ok_or(ContentUpdateError::InvalidCatalogResponse)?
            .to_owned(),
        target_filename: target_filename.to_owned(),
        download_url: parsed.to_string(),
        sha1: sha1.to_ascii_lowercase(),
        sha512: Some(sha512.to_ascii_lowercase()),
        size,
        download_source: DownloadSource::Modrinth,
    })
}

fn primary_file<'a>(version: &'a Value, extension: &str) -> Result<&'a Value, ContentUpdateError> {
    let files = version["files"]
        .as_array()
        .ok_or(ContentUpdateError::InvalidCatalogResponse)?;
    files
        .iter()
        .filter(|file| {
            file["filename"]
                .as_str()
                .is_some_and(|name| name.to_ascii_lowercase().ends_with(extension))
        })
        .find(|file| file["primary"].as_bool() == Some(true))
        .or_else(|| {
            let mut matching = files.iter().filter(|file| {
                file["filename"]
                    .as_str()
                    .is_some_and(|name| name.to_ascii_lowercase().ends_with(extension))
            });
            let first = matching.next()?;
            matching.next().is_none().then_some(first)
        })
        .ok_or(ContentUpdateError::InvalidCatalogResponse)
}

fn http_client() -> Result<Client, ContentUpdateError> {
    client_with_redirect_policy(DownloadSource::Modrinth)
}

fn download_client(source: DownloadSource) -> Result<Client, ContentUpdateError> {
    client_with_redirect_policy(source)
}

fn client_with_redirect_policy(source: DownloadSource) -> Result<Client, ContentUpdateError> {
    Client::builder()
        .user_agent("Aurora-Smart-Launcher/0.1")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(10 * 60))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 5 || !download_url_allowed(attempt.url(), source) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(ContentUpdateError::Network)
}

fn download_validated(
    client: &Client,
    url: &str,
    destination: &Path,
    expected_size: u64,
    expected_sha1: &str,
    expected_sha512: Option<&str>,
    source: DownloadSource,
) -> Result<(), ContentUpdateError> {
    if expected_size == 0 || expected_size > MAX_UPDATE_FILE_BYTES {
        return Err(ContentUpdateError::InvalidDownload(
            destination.display().to_string(),
        ));
    }
    let parsed =
        Url::parse(url).map_err(|_| ContentUpdateError::InvalidDownload(url.to_owned()))?;
    if !download_url_allowed(&parsed, source) {
        return Err(ContentUpdateError::InvalidDownload(url.to_owned()));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| ContentUpdateError::UnsafeFilename(destination.display().to_string()))?;
    fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    let mut last_error = None;
    for attempt in 0..MAX_HTTP_ATTEMPTS {
        match client.get(parsed.clone()).send() {
            Ok(response) => {
                let retryable_status = response.status() == StatusCode::TOO_MANY_REQUESTS
                    || response.status().is_server_error();
                match write_response(response, destination, expected_size, source) {
                    Ok(()) => {
                        match verify_file(
                            destination,
                            Some(expected_size),
                            expected_sha1,
                            expected_sha512,
                        ) {
                            Ok(()) => return Ok(()),
                            Err(error) => last_error = Some(error),
                        }
                    }
                    Err(error) => {
                        let is_io = matches!(error, ContentUpdateError::Io { .. });
                        last_error = Some(error);
                        if is_io || !retryable_status {
                            break;
                        }
                    }
                }
            }
            Err(error) => last_error = Some(ContentUpdateError::Network(error)),
        }
        let _ = fs::remove_file(destination);
        if attempt + 1 < MAX_HTTP_ATTEMPTS {
            thread::sleep(Duration::from_millis(250 * (1 << attempt)));
        }
    }
    let _ = fs::remove_file(destination);
    Err(last_error
        .unwrap_or_else(|| ContentUpdateError::InvalidDownload(destination.display().to_string())))
}

fn write_response(
    mut response: Response,
    destination: &Path,
    expected_size: u64,
    source: DownloadSource,
) -> Result<(), ContentUpdateError> {
    response = response
        .error_for_status()
        .map_err(ContentUpdateError::Network)?;
    // O redirect final também precisa continuar protegido por TLS.
    if !download_url_allowed(response.url(), source) {
        return Err(ContentUpdateError::InvalidDownload(
            response.url().to_string(),
        ));
    }
    let mut output = File::create(destination).map_err(|source| io_error(destination, source))?;
    let mut limited = response.take(expected_size.saturating_add(1));
    io::copy(&mut limited, &mut output).map_err(|source| io_error(destination, source))?;
    output
        .flush()
        .and_then(|_| output.sync_all())
        .map_err(|source| io_error(destination, source))?;
    Ok(())
}

fn download_url_allowed(url: &Url, source: DownloadSource) -> bool {
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };
    match source {
        DownloadSource::Modrinth => host == "cdn.modrinth.com" || host.ends_with(".modrinth.com"),
        DownloadSource::AuroraWorker => host == "aurora-api.aurora-edge-proxy.workers.dev",
    }
}

fn preflight_transaction(
    instance: &Instance,
    files: &[TransactionFile],
) -> Result<(), ContentUpdateError> {
    for file in files {
        let source = secure_instance_path(instance, &file.source_relative)?;
        let current_sha1 = file
            .current_sha1
            .as_deref()
            .ok_or_else(|| ContentUpdateError::Stale(file.previous_filename.clone()))?;
        verify_sha1(&source, current_sha1)
            .map_err(|_| ContentUpdateError::Stale(file.previous_filename.clone()))?;
        let target = secure_instance_path(instance, &file.target_relative)?;
        if target != source && target.exists() {
            return Err(ContentUpdateError::Stale(file.filename.clone()));
        }
    }
    Ok(())
}

fn commit_one(
    instance: &Instance,
    transaction_root: &Path,
    file: &TransactionFile,
) -> Result<(), ContentUpdateError> {
    let source = secure_instance_path(instance, &file.source_relative)?;
    let target = secure_instance_path(instance, &file.target_relative)?;
    let staging = checked_join(transaction_root, &file.staging_relative)?;
    let backup = checked_join(transaction_root, &file.backup_relative)?;
    fs::rename(&source, &backup).map_err(|source| io_error(&backup, source))?;
    if let Err(source_error) = fs::rename(&staging, &target) {
        let restore = fs::rename(&backup, &source);
        return if let Err(restore_error) = restore {
            Err(ContentUpdateError::RollbackFailed(format!(
                "{}; {}",
                source_error, restore_error
            )))
        } else {
            Err(io_error(&target, source_error))
        };
    }
    Ok(())
}

fn rollback_committed(
    instance: &Instance,
    transaction_root: &Path,
    journal: &mut TransactionJournal,
) -> Result<(), ContentUpdateError> {
    for index in (0..journal.committed_files).rev() {
        let file = &journal.files[index];
        let source = secure_instance_path(instance, &file.source_relative)?;
        let target = secure_instance_path(instance, &file.target_relative)?;
        let backup = checked_join(transaction_root, &file.backup_relative)?;
        let replaced = transaction_root
            .join("replaced")
            .join(format!("{index}.new"));
        if let Some(parent) = replaced.parent() {
            fs::create_dir_all(parent).map_err(|cause| io_error(parent, cause))?;
        }
        if file.current_sha1.is_some() {
            if backup.exists() {
                let current_sha1 = file.current_sha1.as_deref().expect("verificado acima");
                verify_sha1(&backup, current_sha1)?;
                if file.target_present && target.exists() {
                    verify_file(
                        &target,
                        Some(file.expected_size),
                        &file.target_sha1,
                        file.target_sha512.as_deref(),
                    )?;
                    if source != target && source.exists() {
                        return Err(ContentUpdateError::Stale(file.previous_filename.clone()));
                    }
                    if replaced.exists() {
                        return Err(ContentUpdateError::Stale(file.filename.clone()));
                    }
                    fs::rename(&target, &replaced).map_err(|cause| io_error(&target, cause))?;
                } else if !file.target_present && target.exists() {
                    return Err(ContentUpdateError::Stale(file.filename.clone()));
                } else if source.exists() {
                    return Err(ContentUpdateError::Stale(file.previous_filename.clone()));
                }
                fs::rename(&backup, &source).map_err(|cause| io_error(&source, cause))?;
            } else {
                // Intenção journalada, mas nenhum rename ocorreu; ou este
                // índice já havia sido restaurado antes de um novo crash.
                let current_sha1 = file.current_sha1.as_deref().expect("verificado acima");
                verify_sha1(&source, current_sha1)?;
                if source != target && target.exists() {
                    return Err(ContentUpdateError::Stale(file.filename.clone()));
                }
            }
        } else if target.exists() {
            verify_file(
                &target,
                Some(file.expected_size),
                &file.target_sha1,
                file.target_sha512.as_deref(),
            )?;
            if replaced.exists() {
                return Err(ContentUpdateError::Stale(file.filename.clone()));
            }
            fs::rename(&target, &replaced).map_err(|cause| io_error(&target, cause))?;
        }
        journal.committed_files = index;
        write_journal(transaction_root, journal)?;
    }
    journal.committed_files = 0;
    journal.state = TransactionState::RolledBack;
    write_journal(transaction_root, journal)
}

fn recover_incomplete_transactions(instance: &Instance) -> Result<(), ContentUpdateError> {
    ensure_instance_root_secure(instance)?;
    let root = transaction_root(instance);
    ensure_no_symlink_components(instance.root(), ".aurora/content-updates")?;
    let Ok(entries) = fs::read_dir(&root) else {
        return Ok(());
    };
    for entry in entries.filter_map(Result::ok) {
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|source| io_error(&entry.path(), source))?;
        if metadata_is_link(&metadata) {
            return Err(ContentUpdateError::UnsafeFilename(
                entry.path().display().to_string(),
            ));
        }
        if !metadata.is_dir() {
            continue;
        }
        let pack_marker = entry.path().join("pack-in-progress");
        if pack_marker.is_file() {
            let snapshot_path = entry.path().join("pack-snapshot.json");
            let paths_path = entry.path().join("pack-paths.json");
            let bytes =
                fs::read(&snapshot_path).map_err(|source| io_error(&snapshot_path, source))?;
            let paths_bytes =
                fs::read(&paths_path).map_err(|source| io_error(&paths_path, source))?;
            let snapshot: Vec<SnapshotEntry> = serde_json::from_slice(&bytes)
                .map_err(|_| ContentUpdateError::InvalidCatalogResponse)?;
            let paths: Vec<String> = serde_json::from_slice(&paths_bytes)
                .map_err(|_| ContentUpdateError::InvalidCatalogResponse)?;
            validate_normalized_paths(&paths)?;
            restore_snapshot(instance, &entry.path(), &snapshot, &paths)?;
            let journal_path = entry.path().join("journal.json");
            recover_atomic_file(&journal_path)?;
            if journal_path.is_file() {
                let mut journal = read_journal(&entry.path())?;
                journal.committed_files = 0;
                journal.state = TransactionState::RolledBack;
                write_journal(&entry.path(), &journal)?;
            }
            let _ = fs::remove_file(entry.path().join("publish-undo"));
            continue;
        }
        let journal_path = entry.path().join("journal.json");
        recover_atomic_file(&journal_path)?;
        if !journal_path.is_file() {
            safe_remove_transaction_directory(&root, &entry.path())?;
            continue;
        }
        let mut journal = read_journal(&entry.path())?;
        if journal.state == TransactionState::Committing
            || journal.state == TransactionState::RollingBack
        {
            rollback_committed(instance, &entry.path(), &mut journal)?;
            let _ = fs::remove_file(entry.path().join("publish-undo"));
        } else if journal.state == TransactionState::Committed
            && entry.path().join("publish-undo").is_file()
        {
            write_last_transaction_pointer(instance, &journal.transaction_id)?;
            let _ = fs::remove_file(entry.path().join("publish-undo"));
        }
    }
    Ok(())
}

fn prune_old_transactions(
    instance: &Instance,
    current_transaction_id: &str,
) -> Result<(), ContentUpdateError> {
    let root = transaction_root(instance);
    for entry in fs::read_dir(&root).map_err(|source| io_error(&root, source))? {
        let entry = entry.map_err(|source| io_error(&root, source))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == current_transaction_id || name.len() != 32 || !valid_hex(&name, 32) {
            continue;
        }
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|source| io_error(&entry.path(), source))?;
        if metadata_is_link(&metadata) || !metadata.is_dir() {
            continue;
        }
        let journal_path = entry.path().join("journal.json");
        recover_atomic_file(&journal_path)?;
        if !journal_path.is_file() {
            safe_remove_transaction_directory(&root, &entry.path())?;
            continue;
        }
        let journal = read_journal(&entry.path())?;
        if matches!(
            journal.state,
            TransactionState::Committed | TransactionState::RolledBack
        ) {
            safe_remove_transaction_directory(&root, &entry.path())?;
        }
    }
    Ok(())
}

fn safe_remove_transaction_directory(
    transactions_root: &Path,
    directory: &Path,
) -> Result<(), ContentUpdateError> {
    if directory.parent() != Some(transactions_root) {
        return Err(ContentUpdateError::UnsafeFilename(
            directory.display().to_string(),
        ));
    }
    let name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ContentUpdateError::UnsafeFilename(directory.display().to_string()))?;
    if name.len() != 32 || !valid_hex(name, 32) {
        return Err(ContentUpdateError::UnsafeFilename(name.to_owned()));
    }
    remove_directory_without_links(directory)
}

fn remove_directory_without_links(directory: &Path) -> Result<(), ContentUpdateError> {
    let metadata = fs::symlink_metadata(directory).map_err(|source| io_error(directory, source))?;
    if metadata_is_link(&metadata) || !metadata.is_dir() {
        return Err(ContentUpdateError::UnsafeFilename(
            directory.display().to_string(),
        ));
    }
    for entry in fs::read_dir(directory).map_err(|source| io_error(directory, source))? {
        let entry = entry.map_err(|source| io_error(directory, source))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|source| io_error(&path, source))?;
        if metadata_is_link(&metadata) {
            return Err(ContentUpdateError::UnsafeFilename(
                path.display().to_string(),
            ));
        }
        if metadata.is_dir() {
            remove_directory_without_links(&path)?;
        } else if metadata.is_file() {
            fs::remove_file(&path).map_err(|source| io_error(&path, source))?;
        }
    }
    fs::remove_dir(directory).map_err(|source| io_error(directory, source))
}

fn normalize_planned_paths(paths: &[PathBuf]) -> Result<Vec<String>, ContentUpdateError> {
    if paths.is_empty() {
        return Err(ContentUpdateError::TooManyFiles);
    }
    let mut normalized = BTreeSet::new();
    let mut insensitive = HashMap::new();
    for path in paths {
        let relative = path
            .to_str()
            .map(|value| value.replace('\\', "/"))
            .ok_or_else(|| ContentUpdateError::UnsafeFilename(path.display().to_string()))?;
        checked_join(Path::new("instance"), &relative)?;
        let key = relative.to_lowercase();
        if is_protected_pack_path(&relative)
            || insensitive
                .get(&key)
                .is_some_and(|existing: &String| existing != &relative)
        {
            return Err(ContentUpdateError::UnsafeFilename(relative));
        }
        insensitive.insert(key, relative.clone());
        normalized.insert(relative);
    }
    if normalized.len() > MAX_SNAPSHOT_FILES {
        return Err(ContentUpdateError::TooManyFiles);
    }
    Ok(normalized.into_iter().collect())
}

fn validate_normalized_paths(paths: &[String]) -> Result<(), ContentUpdateError> {
    let converted = paths.iter().map(PathBuf::from).collect::<Vec<_>>();
    let normalized = normalize_planned_paths(&converted)?;
    if normalized != paths {
        return Err(ContentUpdateError::InvalidCatalogResponse);
    }
    Ok(())
}

fn is_protected_pack_path(relative: &str) -> bool {
    Path::new(relative)
        .components()
        .next()
        .and_then(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .is_some_and(|name| {
            matches!(
                name.to_ascii_lowercase().as_str(),
                ".aurora"
                    | "assets"
                    | "libraries"
                    | "versions"
                    | "natives"
                    | "saves"
                    | "screenshots"
                    | "logs"
                    | "crash-reports"
                    | "backups"
                    | "replay_recordings"
            )
        })
}

pub(super) fn ensure_no_symlink_components(
    root: &Path,
    relative: &str,
) -> Result<(), ContentUpdateError> {
    let mut current = root.to_path_buf();
    for component in Path::new(relative).components() {
        let Component::Normal(name) = component else {
            return Err(ContentUpdateError::UnsafeFilename(relative.to_owned()));
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata_is_link(&metadata) => {
                return Err(ContentUpdateError::UnsafeFilename(relative.to_owned()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => return Err(io_error(&current, error)),
        }
    }
    Ok(())
}

fn secure_instance_path(
    instance: &Instance,
    relative: &str,
) -> Result<PathBuf, ContentUpdateError> {
    ensure_no_symlink_components(instance.root(), relative)?;
    checked_join(instance.root(), relative)
}

pub(super) fn ensure_instance_root_secure(instance: &Instance) -> Result<(), ContentUpdateError> {
    let metadata = fs::symlink_metadata(instance.root())
        .map_err(|source| io_error(instance.root(), source))?;
    if !metadata.is_dir() || metadata_is_link(&metadata) {
        return Err(ContentUpdateError::UnsafeFilename(
            instance.root().display().to_string(),
        ));
    }
    Ok(())
}

pub(super) fn metadata_is_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

fn snapshot_instance_paths(
    instance: &Instance,
    snapshot_root: &Path,
    paths: &[String],
) -> Result<Vec<SnapshotEntry>, ContentUpdateError> {
    let entries = collect_instance_paths(instance, paths)?;
    let total_bytes = entries
        .iter()
        .try_fold(0u64, |total, entry| total.checked_add(entry.size))
        .ok_or_else(|| {
            ContentUpdateError::InvalidDownload("snapshot do modpack grande demais".to_owned())
        })?;
    if total_bytes > MAX_SNAPSHOT_BYTES {
        return Err(ContentUpdateError::InvalidDownload(
            "o snapshot dos arquivos alterados ultrapassa 8 GiB".to_owned(),
        ));
    }
    for entry in &entries {
        let source = secure_instance_path(instance, &entry.relative)?;
        let destination = checked_join(snapshot_root, &entry.relative)?;
        let parent = destination
            .parent()
            .ok_or_else(|| ContentUpdateError::UnsafeFilename(entry.relative.clone()))?;
        fs::create_dir_all(parent).map_err(|cause| io_error(parent, cause))?;
        fs::copy(&source, &destination).map_err(|cause| io_error(&destination, cause))?;
        let copied = File::open(&destination).map_err(|cause| io_error(&destination, cause))?;
        copied
            .sync_all()
            .map_err(|cause| io_error(&destination, cause))?;
        verify_sha1(&destination, &entry.sha1)?;
    }
    Ok(entries)
}

fn collect_instance_paths(
    instance: &Instance,
    paths: &[String],
) -> Result<Vec<SnapshotEntry>, ContentUpdateError> {
    validate_normalized_paths(paths)?;
    let mut entries = Vec::new();
    for relative in paths {
        ensure_no_symlink_components(instance.root(), relative)?;
        let path = secure_instance_path(instance, relative)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io_error(&path, error)),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ContentUpdateError::UnsafeFilename(relative.clone()));
        }
        entries.push(SnapshotEntry {
            relative: relative.clone(),
            size: metadata.len(),
            sha1: hash_sha1(&path)?,
        });
        if entries.len() > MAX_SNAPSHOT_FILES {
            return Err(ContentUpdateError::TooManyFiles);
        }
    }
    Ok(entries)
}

fn restore_snapshot(
    instance: &Instance,
    transaction_root: &Path,
    before: &[SnapshotEntry],
    paths: &[String],
) -> Result<(), ContentUpdateError> {
    let current = collect_instance_paths(instance, paths)?;
    let before_by_path = before
        .iter()
        .map(|entry| (entry.relative.clone(), entry))
        .collect::<HashMap<_, _>>();
    let failed_root = transaction_root.join(format!("failed-new-{}", Uuid::new_v4().simple()));
    for entry in &current {
        let unchanged = before_by_path
            .get(&entry.relative)
            .is_some_and(|previous| previous.sha1 == entry.sha1);
        if unchanged {
            continue;
        }
        let source = secure_instance_path(instance, &entry.relative)?;
        let destination = checked_join(&failed_root, &entry.relative)?;
        let parent = destination
            .parent()
            .ok_or_else(|| ContentUpdateError::UnsafeFilename(entry.relative.clone()))?;
        fs::create_dir_all(parent).map_err(|cause| io_error(parent, cause))?;
        fs::rename(&source, &destination).map_err(|cause| io_error(&source, cause))?;
    }
    let snapshot_root = transaction_root.join("snapshot");
    for entry in before {
        let destination = secure_instance_path(instance, &entry.relative)?;
        if destination.is_file() && hash_sha1(&destination)?.eq_ignore_ascii_case(&entry.sha1) {
            continue;
        }
        restore_snapshot_file(&snapshot_root, entry, &destination)?;
    }
    let marker = transaction_root.join("pack-in-progress");
    if marker.exists() {
        fs::remove_file(&marker).map_err(|cause| io_error(&marker, cause))?;
    }
    Ok(())
}

fn rollback_pack_snapshot(
    instance: &Instance,
    transaction_root: &Path,
    before: &[SnapshotEntry],
    paths: &[String],
) -> Result<(), ContentUpdateError> {
    restore_snapshot(instance, transaction_root, before, paths)?;
    let journal_path = transaction_root.join("journal.json");
    recover_atomic_file(&journal_path)?;
    if journal_path.is_file() {
        let mut journal = read_journal(transaction_root)?;
        journal.committed_files = 0;
        journal.state = TransactionState::RolledBack;
        write_journal(transaction_root, &journal)?;
    }
    for marker in ["pack-in-progress", "publish-undo"] {
        let path = transaction_root.join(marker);
        if path.exists() {
            fs::remove_file(&path).map_err(|source| io_error(&path, source))?;
        }
    }
    Ok(())
}

fn restore_snapshot_file(
    snapshot_root: &Path,
    entry: &SnapshotEntry,
    destination: &Path,
) -> Result<(), ContentUpdateError> {
    let source = checked_join(snapshot_root, &entry.relative)?;
    verify_sha1(&source, &entry.sha1)?;
    let parent = destination
        .parent()
        .ok_or_else(|| ContentUpdateError::UnsafeFilename(entry.relative.clone()))?;
    fs::create_dir_all(parent).map_err(|cause| io_error(parent, cause))?;
    let temporary = parent.join(format!(".aurora-restore-{}", Uuid::new_v4().simple()));
    fs::copy(&source, &temporary).map_err(|cause| io_error(&temporary, cause))?;
    let restored = File::open(&temporary).map_err(|cause| io_error(&temporary, cause))?;
    restored
        .sync_all()
        .map_err(|cause| io_error(&temporary, cause))?;
    if temporary
        .metadata()
        .map_err(|cause| io_error(&temporary, cause))?
        .len()
        != entry.size
    {
        let _ = fs::remove_file(&temporary);
        return Err(ContentUpdateError::Integrity(entry.relative.clone()));
    }
    verify_sha1(&temporary, &entry.sha1)?;
    if destination.exists() {
        return Err(ContentUpdateError::Stale(entry.relative.clone()));
    }
    fs::rename(&temporary, destination).map_err(|cause| io_error(destination, cause))
}

pub fn acquire_instance_mutation_lock(
    instance: &Instance,
) -> Result<InstanceMutationGuard, ContentUpdateError> {
    ensure_instance_root_secure(instance)?;
    let root = instance
        .root()
        .canonicalize()
        .map_err(|source| io_error(instance.root(), source))?;
    // Keep the handle outside the instance: Windows cannot rename a directory
    // containing our open lock file. All mutations use this same stable path,
    // including launch, updates, rename and delete; never unlink the lock file.
    let data_directory = root
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| ContentUpdateError::UnsafeFilename(root.display().to_string()))?;
    let identity = root.to_string_lossy();
    #[cfg(windows)]
    let identity = identity.to_lowercase();
    let relative = format!(
        ".aurora/instance-locks/{:x}.lock",
        Sha1::digest(identity.as_bytes())
    );
    let directory = data_directory.join(".aurora").join("instance-locks");
    ensure_no_symlink_components(data_directory, ".aurora/instance-locks")?;
    fs::create_dir_all(&directory).map_err(|source| io_error(&directory, source))?;
    ensure_no_symlink_components(data_directory, &relative)?;
    let path = data_directory.join(relative);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|source| io_error(&path, source))?;
    file.try_lock().map_err(|source| match source {
        fs::TryLockError::WouldBlock => ContentUpdateError::Busy,
        fs::TryLockError::Error(source) => io_error(&path, source),
    })?;
    Ok(InstanceMutationGuard { _file: file, root })
}

fn transaction_root(instance: &Instance) -> PathBuf {
    instance.root().join(".aurora").join("content-updates")
}

fn write_journal(root: &Path, journal: &TransactionJournal) -> Result<(), ContentUpdateError> {
    let bytes = serde_json::to_vec_pretty(journal)
        .map_err(|_| ContentUpdateError::InvalidCatalogResponse)?;
    atomic_write(&root.join("journal.json"), &bytes)
}

fn read_journal(root: &Path) -> Result<TransactionJournal, ContentUpdateError> {
    let path = root.join("journal.json");
    recover_atomic_file(&path)?;
    let bytes = fs::read(&path).map_err(|source| io_error(&path, source))?;
    let journal: TransactionJournal =
        serde_json::from_slice(&bytes).map_err(|_| ContentUpdateError::InvalidCatalogResponse)?;
    let file_limit = if journal.content_type == "modpack" {
        MAX_SNAPSHOT_FILES
    } else {
        MAX_SELECTED_FILES
    };
    if journal.schema_version != JOURNAL_VERSION
        || journal.transaction_id
            != root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
        || journal.files.len() > file_limit
        || journal.committed_files > journal.files.len()
    {
        return Err(ContentUpdateError::InvalidCatalogResponse);
    }
    for file in &journal.files {
        checked_join(root, &file.staging_relative)?;
        checked_join(root, &file.backup_relative)?;
        checked_join(Path::new("instance"), &file.source_relative)?;
        checked_join(Path::new("instance"), &file.target_relative)?;
    }
    Ok(journal)
}

fn write_last_transaction_pointer(
    instance: &Instance,
    transaction_id: &str,
) -> Result<(), ContentUpdateError> {
    atomic_write(
        &transaction_root(instance).join("last-transaction"),
        transaction_id.as_bytes(),
    )
}

fn read_last_transaction_pointer(instance: &Instance) -> Result<String, ContentUpdateError> {
    let path = transaction_root(instance).join("last-transaction");
    recover_atomic_file(&path)?;
    let value = fs::read_to_string(&path)
        .map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                ContentUpdateError::NoRollback
            } else {
                io_error(&path, source)
            }
        })?
        .trim()
        .to_owned();
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ContentUpdateError::NoRollback);
    }
    Ok(value)
}

fn last_rollback_available(instance: &Instance) -> bool {
    let Ok(transaction_id) = read_last_transaction_pointer(instance) else {
        return false;
    };
    read_journal(&transaction_root(instance).join(transaction_id))
        .is_ok_and(|journal| journal.state == TransactionState::Committed)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ContentUpdateError> {
    let parent = path
        .parent()
        .ok_or_else(|| ContentUpdateError::UnsafeFilename(path.display().to_string()))?;
    fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    recover_atomic_file(path)?;
    let temporary = path.with_file_name(format!(
        ".{}.{}.writing",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("metadata"),
        Uuid::new_v4().simple()
    ));
    let mut file = File::create(&temporary).map_err(|source| io_error(&temporary, source))?;
    file.write_all(bytes)
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all())
        .map_err(|source| io_error(&temporary, source))?;
    if path.exists() {
        let backup = atomic_backup_path(path);
        fs::rename(path, &backup).map_err(|source| io_error(path, source))?;
        if let Err(source) = fs::rename(&temporary, path) {
            let _ = fs::rename(&backup, path);
            let _ = fs::remove_file(&temporary);
            return Err(io_error(path, source));
        }
        let _ = fs::remove_file(&backup);
    } else {
        fs::rename(&temporary, path).map_err(|source| io_error(path, source))?;
    }
    Ok(())
}

fn atomic_backup_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        ".{}.aurora-backup",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("metadata")
    ))
}

fn recover_atomic_file(path: &Path) -> Result<(), ContentUpdateError> {
    let backup = atomic_backup_path(path);
    if !backup.exists() {
        return Ok(());
    }
    if path.exists() {
        fs::remove_file(&backup).map_err(|source| io_error(&backup, source))?;
    } else {
        fs::rename(&backup, path).map_err(|source| io_error(path, source))?;
    }
    Ok(())
}

fn relative_content_path(folder: &str, filename: &str) -> Result<String, ContentUpdateError> {
    let path = Path::new(folder).join(filename);
    path.to_str()
        .map(|value| value.replace('\\', "/"))
        .ok_or_else(|| ContentUpdateError::UnsafeFilename(filename.to_owned()))
}

fn checked_join(root: &Path, relative: &str) -> Result<PathBuf, ContentUpdateError> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ContentUpdateError::UnsafeFilename(relative.to_owned()));
    }
    Ok(root.join(path))
}

fn verify_file(
    path: &Path,
    expected_size: Option<u64>,
    expected_sha1: &str,
    expected_sha512: Option<&str>,
) -> Result<(), ContentUpdateError> {
    let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
    if !metadata.is_file() || expected_size.is_some_and(|size| metadata.len() != size) {
        return Err(ContentUpdateError::Integrity(path.display().to_string()));
    }
    if !hash_sha1(path)?.eq_ignore_ascii_case(expected_sha1)
        || expected_sha512.is_some_and(|expected| {
            !hash_sha512(path).is_ok_and(|actual| actual.eq_ignore_ascii_case(expected))
        })
    {
        return Err(ContentUpdateError::Integrity(path.display().to_string()));
    }
    Ok(())
}

fn verify_sha1(path: &Path, expected: &str) -> Result<(), ContentUpdateError> {
    if !valid_hex(expected, 40) || !hash_sha1(path)?.eq_ignore_ascii_case(expected) {
        return Err(ContentUpdateError::Integrity(path.display().to_string()));
    }
    Ok(())
}

fn hash_sha1(path: &Path) -> Result<String, ContentUpdateError> {
    let mut file = File::open(path).map_err(|source| io_error(path, source))?;
    let mut digest = Sha1::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let size = file
            .read(&mut buffer)
            .map_err(|source| io_error(path, source))?;
        if size == 0 {
            break;
        }
        digest.update(&buffer[..size]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn hash_sha512(path: &Path) -> Result<String, ContentUpdateError> {
    let mut file = File::open(path).map_err(|source| io_error(path, source))?;
    let mut digest = Sha512::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let size = file
            .read(&mut buffer)
            .map_err(|source| io_error(path, source))?;
        if size == 0 {
            break;
        }
        digest.update(&buffer[..size]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn valid_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn default_true() -> bool {
    true
}

fn io_error(path: &Path, source: io::Error) -> ContentUpdateError {
    ContentUpdateError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn locate_current_pack_version<'a>(compatible: &[&'a Value], marker: &Value) -> Option<&'a Value> {
    if let Some(id) = marker["packVersionId"].as_str() {
        return compatible
            .iter()
            .copied()
            .find(|version| version["id"].as_str() == Some(id));
    }
    let legacy_name = marker["versionNumber"]
        .as_str()
        .or_else(|| marker["versionName"].as_str())?;
    let matches = compatible
        .iter()
        .copied()
        .filter(|version| {
            version["version_number"].as_str() == Some(legacy_name)
                || version["name"].as_str() == Some(legacy_name)
        })
        .collect::<Vec<_>>();
    (matches.len() == 1).then_some(matches[0])
}

fn pack_owned_files(marker: &Value) -> Result<Vec<SnapshotEntry>, ContentUpdateError> {
    let files = marker["ownedFiles"]
        .as_array()
        .filter(|files| !files.is_empty() && files.len() <= MAX_SNAPSHOT_FILES)
        .ok_or(ContentUpdateError::InvalidCatalogResponse)?;
    let mut result = Vec::with_capacity(files.len());
    let mut paths = Vec::with_capacity(files.len());
    for file in files {
        let relative = file["path"]
            .as_str()
            .ok_or(ContentUpdateError::InvalidCatalogResponse)?
            .to_owned();
        let sha1 = file["sha1"]
            .as_str()
            .filter(|hash| valid_hex(hash, 40))
            .ok_or(ContentUpdateError::InvalidCatalogResponse)?
            .to_ascii_lowercase();
        let size = file["size"]
            .as_u64()
            .filter(|size| *size <= MAX_UPDATE_FILE_BYTES)
            .ok_or(ContentUpdateError::InvalidCatalogResponse)?;
        paths.push(PathBuf::from(&relative));
        result.push(SnapshotEntry {
            relative,
            size,
            sha1,
        });
    }
    let normalized = normalize_planned_paths(&paths)?;
    if normalized
        .iter()
        .zip(&result)
        .any(|(normalized, entry)| normalized != &entry.relative)
    {
        return Err(ContentUpdateError::InvalidCatalogResponse);
    }
    Ok(result)
}

fn unmanaged_modpack(reason: &str) -> ModpackUpdateScan {
    ModpackUpdateScan {
        status: ModpackUpdateStatus::Unmanaged,
        source: None,
        project_id: None,
        current_file_id: None,
        current_version_id: None,
        current_version_number: None,
        target_version_id: None,
        target_version_number: None,
        minecraft_version: None,
        loader: None,
        reason: Some(reason.to_owned()),
    }
}

fn value_id_as_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|id| id.to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;
    use crate::engine::{InstanceId, InstanceLayout};

    fn test_instance(label: &str) -> (PathBuf, Instance) {
        let root = std::env::temp_dir().join(format!(
            "aurora-content-updater-{label}-{}",
            Uuid::new_v4().simple()
        ));
        let instance = InstanceLayout::new(&root).open(InstanceId::parse("test").unwrap());
        instance.ensure_layout().unwrap();
        (root, instance)
    }

    fn digest_sha1(bytes: &[u8]) -> String {
        format!("{:x}", Sha1::digest(bytes))
    }

    fn digest_sha512(bytes: &[u8]) -> String {
        format!("{:x}", Sha512::digest(bytes))
    }

    fn transaction_file(old: &[u8], new: &[u8], same_name: bool) -> TransactionFile {
        TransactionFile {
            previous_filename: "old.jar".to_owned(),
            filename: if same_name { "old.jar" } else { "new.jar" }.to_owned(),
            enabled: true,
            project_id: "project".to_owned(),
            previous_version_number: "1.0".to_owned(),
            version_number: "2.0".to_owned(),
            current_sha1: Some(digest_sha1(old)),
            target_sha1: digest_sha1(new),
            target_sha512: Some(digest_sha512(new)),
            expected_size: new.len() as u64,
            target_present: true,
            source_relative: "mods/old.jar".to_owned(),
            target_relative: if same_name {
                "mods/old.jar"
            } else {
                "mods/new.jar"
            }
            .to_owned(),
            staging_relative: "staging/0.download".to_owned(),
            backup_relative: "backup/0.previous".to_owned(),
        }
    }

    #[test]
    fn planner_requires_exact_minecraft_and_loader_and_stable_channel() {
        let versions = json!([
            {"id":"wrong-game","version_type":"release","date_published":"2026-03-01T00:00:00Z","game_versions":["1.21.1"],"loaders":["fabric"]},
            {"id":"wrong-loader","version_type":"release","date_published":"2026-03-01T00:00:00Z","game_versions":["1.20.1"],"loaders":["forge"]},
            {"id":"beta","version_type":"beta","date_published":"2026-04-01T00:00:00Z","game_versions":["1.20.1"],"loaders":["fabric"]},
            {"id":"safe","version_type":"release","date_published":"2026-02-01T00:00:00Z","game_versions":["1.20.1"],"loaders":["fabric"]}
        ]);
        let compatible = compatible_versions(&versions, "mod", "1.20.1", "fabric");
        assert_eq!(compatible.len(), 1);
        assert_eq!(compatible[0]["id"], "safe");
    }

    #[test]
    fn newer_release_uses_publication_date_not_filename_or_semver() {
        let current = json!({"id":"current","date_published":"2026-01-01T00:00:00Z"});
        let versions = json!([
            {"id":"odd-name","version_type":"release","version_number":"banana","date_published":"2026-02-01T00:00:00Z","game_versions":["1.20.1"],"loaders":["fabric"]},
            {"id":"older-semver","version_type":"release","version_number":"999.0","date_published":"2025-12-01T00:00:00Z","game_versions":["1.20.1"],"loaders":["fabric"]}
        ]);
        let compatible = compatible_versions(&versions, "mod", "1.20.1", "fabric");
        assert_eq!(
            latest_newer_release(&compatible, &current).unwrap()["id"],
            "odd-name"
        );
    }

    #[test]
    fn rejects_path_traversal_in_transaction_paths() {
        assert!(checked_join(Path::new("instance"), "../outside.jar").is_err());
        assert!(checked_join(Path::new("instance"), "mods/../../outside.jar").is_err());
        assert!(checked_join(Path::new("instance"), "mods/safe.jar").is_ok());
    }

    #[test]
    fn commits_and_rolls_back_a_replacement_bit_for_bit() {
        let (root, instance) = test_instance("commit-rollback");
        let transaction_id = Uuid::new_v4().simple().to_string();
        let transaction_root = transaction_root(&instance).join(&transaction_id);
        fs::create_dir_all(transaction_root.join("staging")).unwrap();
        fs::create_dir_all(transaction_root.join("backup")).unwrap();
        fs::write(instance.root().join("mods/old.jar"), b"old bytes").unwrap();
        fs::write(transaction_root.join("staging/0.download"), b"new bytes").unwrap();
        let file = transaction_file(b"old bytes", b"new bytes", false);
        commit_one(&instance, &transaction_root, &file).unwrap();
        assert_eq!(
            fs::read(instance.root().join("mods/new.jar")).unwrap(),
            b"new bytes"
        );
        assert!(!instance.root().join("mods/old.jar").exists());
        let mut journal = TransactionJournal {
            schema_version: JOURNAL_VERSION,
            transaction_id,
            state: TransactionState::Committed,
            content_type: "mod".to_owned(),
            minecraft_version: "1.20.1".to_owned(),
            loader: "fabric".to_owned(),
            committed_files: 1,
            files: vec![file],
        };
        rollback_committed(&instance, &transaction_root, &mut journal).unwrap();
        assert_eq!(
            fs::read(instance.root().join("mods/old.jar")).unwrap(),
            b"old bytes"
        );
        assert!(!instance.root().join("mods/new.jar").exists());
        assert_eq!(journal.state, TransactionState::RolledBack);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_publish_restores_the_previous_file() {
        let (root, instance) = test_instance("failed-publish");
        let transaction_root =
            transaction_root(&instance).join(Uuid::new_v4().simple().to_string());
        fs::create_dir_all(transaction_root.join("staging")).unwrap();
        fs::create_dir_all(transaction_root.join("backup")).unwrap();
        fs::write(instance.root().join("mods/old.jar"), b"old bytes").unwrap();
        // O staging ausente força falha depois que o arquivo antigo foi movido.
        let file = transaction_file(b"old bytes", b"new bytes", true);
        assert!(commit_one(&instance, &transaction_root, &file).is_err());
        assert_eq!(
            fs::read(instance.root().join("mods/old.jar")).unwrap(),
            b"old bytes"
        );
        assert!(!transaction_root.join("backup/0.previous").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disabled_state_is_preserved_in_target_name() {
        let spec = content_spec("mod").unwrap();
        let target = "updated.jar";
        let disk_name = format!("{target}.disabled");
        assert_eq!(
            relative_content_path(spec.folder, &disk_name).unwrap(),
            "mods/updated.jar.disabled"
        );
    }

    #[test]
    fn rollback_recovers_when_only_intent_was_journaled() {
        let (root, instance) = test_instance("intent-only");
        let transaction_id = Uuid::new_v4().simple().to_string();
        let transaction_root = transaction_root(&instance).join(&transaction_id);
        fs::create_dir_all(transaction_root.join("backup")).unwrap();
        fs::write(instance.root().join("mods/old.jar"), b"old bytes").unwrap();
        let mut journal = TransactionJournal {
            schema_version: JOURNAL_VERSION,
            transaction_id,
            state: TransactionState::Committing,
            content_type: "mod".to_owned(),
            minecraft_version: "1.20.1".to_owned(),
            loader: "fabric".to_owned(),
            committed_files: 1,
            files: vec![transaction_file(b"old bytes", b"new bytes", false)],
        };
        write_journal(&transaction_root, &journal).unwrap();
        rollback_committed(&instance, &transaction_root, &mut journal).unwrap();
        assert_eq!(
            fs::read(instance.root().join("mods/old.jar")).unwrap(),
            b"old bytes"
        );
        assert_eq!(journal.state, TransactionState::RolledBack);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_recovers_between_backup_and_publish() {
        let (root, instance) = test_instance("backup-only");
        let transaction_id = Uuid::new_v4().simple().to_string();
        let transaction_root = transaction_root(&instance).join(&transaction_id);
        fs::create_dir_all(transaction_root.join("backup")).unwrap();
        fs::write(instance.root().join("mods/old.jar"), b"old bytes").unwrap();
        fs::rename(
            instance.root().join("mods/old.jar"),
            transaction_root.join("backup/0.previous"),
        )
        .unwrap();
        let mut journal = TransactionJournal {
            schema_version: JOURNAL_VERSION,
            transaction_id,
            state: TransactionState::Committing,
            content_type: "mod".to_owned(),
            minecraft_version: "1.20.1".to_owned(),
            loader: "fabric".to_owned(),
            committed_files: 1,
            files: vec![transaction_file(b"old bytes", b"new bytes", false)],
        };
        write_journal(&transaction_root, &journal).unwrap();
        rollback_committed(&instance, &transaction_root, &mut journal).unwrap();
        assert_eq!(
            fs::read(instance.root().join("mods/old.jar")).unwrap(),
            b"old bytes"
        );
        assert!(!instance.root().join("mods/new.jar").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn operating_system_lock_is_released_without_deleting_the_lock_file() {
        let (root, instance) = test_instance("lock");
        let first = acquire_instance_mutation_lock(&instance).unwrap();
        assert!(matches!(
            acquire_instance_mutation_lock(&instance),
            Err(ContentUpdateError::Busy)
        ));
        drop(first);
        acquire_instance_mutation_lock(&instance).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_case_alias_cannot_acquire_a_second_mutation_lock() {
        let (root, instance) = test_instance("case-lock");
        let guard = acquire_instance_mutation_lock(&instance).unwrap();
        let alias = InstanceLayout::new(&root).open(InstanceId::parse("TEST").unwrap());
        assert!(matches!(
            acquire_instance_mutation_lock(&alias),
            Err(ContentUpdateError::Busy)
        ));
        drop(guard);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn mutation_lock_storage_does_not_follow_a_junction() {
        use crate::engine::process::background_command;
        let (root, instance) = test_instance("lock-junction");
        let outside = root.join("outside-locks");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("preserved"), b"unchanged").unwrap();
        let link = root.join(".aurora/instance-locks");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        let output = background_command("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(link.to_string_lossy().replace('/', "\\"))
            .arg(outside.to_string_lossy().replace('/', "\\"))
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(acquire_instance_mutation_lock(&instance).is_err());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
        assert_eq!(fs::read(outside.join("preserved")).unwrap(), b"unchanged");
        // Remove the junction itself before the fixture; never recurse through it.
        fs::remove_dir(link).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn download_hosts_are_fail_closed() {
        assert!(download_url_allowed(
            &Url::parse("https://cdn.modrinth.com/data/a/versions/b/file.jar").unwrap(),
            DownloadSource::Modrinth
        ));
        assert!(download_url_allowed(
            &Url::parse("https://aurora-api.aurora-edge-proxy.workers.dev/ticket/abc").unwrap(),
            DownloadSource::AuroraWorker
        ));
        for url in [
            "https://example.com/file.jar",
            "https://localhost/file.jar",
            "http://cdn.modrinth.com/file.jar",
            "https://modrinth.com.evil.example/file.jar",
        ] {
            let parsed = Url::parse(url).unwrap();
            assert!(!download_url_allowed(&parsed, DownloadSource::Modrinth));
            assert!(!download_url_allowed(&parsed, DownloadSource::AuroraWorker));
        }
    }

    #[test]
    fn modpack_journal_accepts_more_than_content_batch_limit() {
        let (root, instance) = test_instance("large-pack-journal");
        let transaction_id = Uuid::new_v4().simple().to_string();
        let transaction_root = transaction_root(&instance).join(&transaction_id);
        let file = transaction_file(b"old", b"new", true);
        let journal = TransactionJournal {
            schema_version: JOURNAL_VERSION,
            transaction_id,
            state: TransactionState::Committed,
            content_type: "modpack".to_owned(),
            minecraft_version: "1.20.1".to_owned(),
            loader: "fabric".to_owned(),
            committed_files: 0,
            files: vec![file; MAX_SELECTED_FILES + 1],
        };
        write_journal(&transaction_root, &journal).unwrap();
        assert_eq!(
            read_journal(&transaction_root).unwrap().files.len(),
            MAX_SELECTED_FILES + 1
        );
        fs::remove_dir_all(root).unwrap();
    }
}
