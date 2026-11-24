//! Offline, read-only diagnostics. Logs are untrusted input; reports contain only
//! fixed evidence labels, never excerpts, filenames, account data or raw errors.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::content_updates::{ensure_no_symlink_components, metadata_is_link};
use super::{required_java_major, Instance};

#[path = "doctor_rules.rs"]
mod rules;

const MAX_LOGS: usize = 12;
const MAX_DIRECTORY_ENTRIES: usize = 256;
const MAX_LOG_BYTES: u64 = 128 * 1024;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024;
const MAX_PROFILE_BYTES: u64 = 2 * 1024 * 1024;
const HEAD_BYTES: u64 = 4096;
const SAFE_READ_ERROR: &str = "Não foi possível ler com segurança os arquivos desta instância.";
const LEGACY_DEPENDENCY_FAILURE: &str = "Dependência obrigatória ausente ou não atendida.";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorLogStamp {
    pub relative_path: String,
    pub size: u64,
    pub modified_at_ms: u64,
    pub head_sha256: String,
}

/// Private launcher metadata, not a shareable report. Never return this to UI.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorRun {
    pub run_id: String,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub outcome: String,
    pub exit_code: Option<i32>,
    pub version_id: String,
    pub minecraft_version: String,
    pub java_executable: String,
    pub java_major: Option<u32>,
    pub max_memory_mb: u32,
    pub failure: Option<String>,
    pub logs_before: Vec<DoctorLogStamp>,
}

#[derive(Clone, Debug)]
pub struct DoctorContext {
    pub version_id: String,
    pub minecraft_version: String,
    pub java_executable: Option<String>,
    pub max_memory_mb: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub schema_version: u32,
    pub status: String,
    pub summary: String,
    pub instance: DoctorInstanceSummary,
    pub last_run: Option<DoctorRunSummary>,
    pub issues: Vec<DoctorIssue>,
    pub logs: Vec<DoctorLogSummary>,
    pub limitations: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sanitized_json: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorInstanceSummary {
    pub minecraft_version: String,
    pub loader: String,
    pub java_major: Option<u32>,
    pub required_java_major: u32,
    pub max_memory_mb: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorRunSummary {
    pub run_id: Option<String>,
    pub started_at_ms: Option<u64>,
    pub ended_at_ms: Option<u64>,
    pub outcome: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorIssue {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub explanation: String,
    pub evidence: Vec<DoctorEvidence>,
    pub confidence: String,
    pub kind: String,
    pub next_step: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorEvidence {
    pub source: String,
    pub summary: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorLogSummary {
    pub source: String,
    pub association: String,
    pub bytes_read: u64,
    pub truncated: bool,
}

/// Snapshot before *any* launch preparation. Existing bytes are never treated
/// as evidence of a later attempt merely because a file was touched/appended.
pub fn snapshot_logs(instance: &Instance) -> Vec<DoctorLogStamp> {
    if secure_root(instance).is_err() {
        return Vec::new();
    }
    let (paths, _) = log_candidates(instance);
    paths
        .into_iter()
        .filter_map(|relative| stamp(instance, &relative).ok())
        .collect()
}

/// Reduce errors at the lifecycle boundary, so persistence also avoids secrets.
pub fn canonical_launch_failure(error: &str) -> String {
    let bounded: String = error.chars().take(8192).collect();
    // Alpha.11 persisted a broader category. Preserve its known, private-safe
    // wording rather than silently asserting that absence was confirmed.
    if bounded
        .trim()
        .eq_ignore_ascii_case(LEGACY_DEPENDENCY_FAILURE)
    {
        return LEGACY_DEPENDENCY_FAILURE.into();
    }
    if let Some(rule) = RULES
        .iter()
        .find(|rule| bounded.trim().eq_ignore_ascii_case(rule.canonical))
    {
        return rule.canonical.to_owned();
    }
    let findings = rules::detect(&bounded);
    if let Some(rule) = RULES
        .iter()
        .find(|rule| findings.iter().any(|finding| finding.id == rule.id))
    {
        return rule.canonical.to_owned();
    }
    "Falha de preparação sem causa identificada.".to_owned()
}

pub fn analyze_instance(
    instance: &Instance,
    context: DoctorContext,
    run: Option<DoctorRun>,
) -> Result<DoctorReport, String> {
    secure_root(instance)?;
    let valid_mc = safe_minecraft_version(&context.minecraft_version);
    let required = if valid_mc == "Não identificada" {
        0
    } else {
        required_java_major(&valid_mc)
    };
    let selected_java = context.java_executable.as_deref().filter(|p| !p.is_empty());
    let java_major = run
        .as_ref()
        .filter(|run| {
            run.version_id == context.version_id
                && run.minecraft_version == context.minecraft_version
                && selected_java == Some(run.java_executable.as_str())
        })
        .and_then(|run| run.java_major)
        .filter(|major| (8..=99).contains(major));
    let mut report = DoctorReport {
        schema_version: 1,
        status: "incomplete".into(),
        summary: String::new(),
        instance: DoctorInstanceSummary {
            minecraft_version: valid_mc,
            loader: loader_label(&context.version_id).into(),
            java_major,
            required_java_major: required,
            max_memory_mb: context.max_memory_mb.min(1024 * 1024),
        },
        last_run: run.as_ref().map(|run| DoctorRunSummary {
            run_id: uuid::Uuid::parse_str(&run.run_id).ok().map(|id| id.to_string()),
            started_at_ms: safe_timestamp(run.started_at_ms),
            ended_at_ms: run.ended_at_ms.and_then(safe_timestamp),
            outcome: safe_outcome(&run.outcome).into(),
            exit_code: run.exit_code,
        }),
        issues: Vec::new(),
        logs: Vec::new(),
        limitations: vec![
            "Análise local e somente leitura; nenhum log foi enviado a serviços externos.".into(),
            "Evidências compartilháveis usam marcadores conhecidos; nomes, caminhos e trechos brutos são omitidos.".into(),
            "A ausência de alertas não comprova integridade completa. Requisitos adicionais de mods são reconhecidos quando declarados nos erros do loader.".into(),
            "Os arquivos podem mudar durante a análise. O relatório não é uma cópia imutável dos logs e não substitui uma nova análise após encerrar o jogo.".into(),
        ],
        sanitized_json: String::new(),
    };
    if selected_java.is_some_and(|path| !Path::new(path).is_file()) {
        add_issue(
            &mut report,
            &RULES[0],
            "Configuração atual",
            "O executável Java selecionado não existe como arquivo acessível.",
        );
    } else if selected_java.is_none() {
        report.limitations.push("Nenhum Java foi selecionado; a seleção automática só é conhecida após uma tentativa de início.".into());
    }
    if java_major.is_some_and(|major| major < required) {
        add_issue(
            &mut report,
            &RULES[1],
            "Configuração da última execução",
            "A versão principal do Java registrada é inferior ao requisito base do Minecraft.",
        );
    }
    if java_major.is_none() {
        report.limitations.push("A versão do Java selecionado ainda não foi registrada pelo launcher. O Doctor não executa programas para consultá-la.".into());
    }
    if java_major.is_some_and(|major| major > required) {
        report.limitations.push("O Java registrado é superior ao requisito base. Isso não comprova compatibilidade com o loader ou os mods; considere as exigências declaradas pelo launcher na tentativa.".into());
    }
    let before_profile = report.limitations.len();
    check_profile(instance, &context.version_id, &mut report);
    let profile_limited = report.limitations.len() > before_profile;
    let (paths, scan_limited) = log_candidates(instance);
    let mut coverage_limited = scan_limited || profile_limited;
    if scan_limited {
        report.limitations.push(
            "A seleção de logs foi limitada ou há caminhos não acessíveis; a cobertura é parcial."
                .into(),
        );
    }
    let failed = run.as_ref().is_some_and(|run| run.outcome == "failed");
    let mut current_rule_found = false;
    if let Some(failure) = run
        .as_ref()
        .filter(|_| failed)
        .and_then(|run| run.failure.as_ref())
    {
        current_rule_found |=
            inspect_text(failure, "Preparação registrada pelo launcher", &mut report);
    }
    let mut remaining = MAX_TOTAL_BYTES;
    for relative in paths {
        let Ok(current) = stamp(instance, &relative) else {
            continue;
        };
        let association = associate_log(instance, &current, run.as_ref());
        let source = log_label(&relative);
        let (label, offset) = match association {
            Association::Current(offset) => ("last_run", Some(offset)),
            Association::Historical => ("historical", None),
            Association::Unknown => ("unassociated", None),
        };
        let mut summary = DoctorLogSummary {
            source: source.into(),
            association: label.into(),
            bytes_read: 0,
            truncated: false,
        };
        if let Some(offset) = offset {
            // A successful or voluntarily stopped session is not diagnosed as
            // a crash based on warnings/exceptions embedded in its log.
            if failed && remaining > 0 {
                let available = current.size.saturating_sub(offset);
                let length = available.min(MAX_LOG_BYTES).min(remaining);
                let mut start = current.size.saturating_sub(length).max(offset);
                let encoding = read_segment(instance, &relative, 0, 2).unwrap_or_default();
                let utf16 = encoding == [0xff, 0xfe] || encoding == [0xfe, 0xff];
                if utf16 && start % 2 != 0 {
                    start = start.saturating_add(1);
                }
                summary.truncated = start > offset;
                match read_segment(instance, &relative, start, length) {
                    Ok(bytes) => {
                        summary.bytes_read = bytes.len() as u64;
                        remaining = remaining.saturating_sub(summary.bytes_read);
                        let stable = stamp(instance, &relative).is_ok_and(|after| {
                            after.size == current.size
                                && after.modified_at_ms == current.modified_at_ms
                                && after.head_sha256 == current.head_sha256
                        });
                        if !stable {
                            coverage_limited = true;
                            report.limitations.push(
                                "Um log mudou durante a leitura; suas evidências foram ignoradas."
                                    .into(),
                            );
                            report.logs.push(summary);
                            continue;
                        }
                        let text = if utf16 {
                            let big_endian = encoding == [0xfe, 0xff];
                            let units = bytes
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .map(|pair| {
                                    if big_endian {
                                        u16::from_be_bytes([pair[0], pair[1]])
                                    } else {
                                        u16::from_le_bytes([pair[0], pair[1]])
                                    }
                                })
                                .collect::<Vec<_>>();
                            String::from_utf16_lossy(&units)
                        } else {
                            decode_log(&bytes)
                        };
                        // Starting inside an inherited or truncated line must
                        // never make its remaining bytes evidence of this run.
                        let boundary = start == 0 || {
                            let step = if utf16 { 2 } else { 1 };
                            let previous =
                                read_segment(instance, &relative, start.saturating_sub(step), step)
                                    .unwrap_or_default();
                            if utf16 {
                                previous == [10, 0] || previous == [0, 10]
                            } else {
                                previous == [10]
                            }
                        };
                        let text = if !boundary {
                            text.split_once('\n').map_or("", |(_, rest)| rest)
                        } else {
                            &text
                        };
                        current_rule_found |= inspect_text(text, source, &mut report);
                    }
                    Err(_) => {
                        coverage_limited = true;
                        report.limitations.push("Um log mudou ou ficou inacessível durante a leitura; ele foi ignorado.".into());
                    }
                }
            } else if failed {
                summary.truncated = true;
            }
        }
        report.logs.push(summary);
    }
    if report.logs.iter().any(|log| log.truncated) {
        report.limitations.push("Leitura limitada ao final de cada log: até 128 KiB por arquivo e 1 MiB no total. Eventos anteriores podem ter sido omitidos.".into());
    }
    if report.logs.iter().any(|log| log.association != "last_run") {
        report.limitations.push("Logs históricos ou sem associação comprovada não foram usados para atribuir a causa da última execução.".into());
    }
    if failed && !current_rule_found {
        report.issues.push(DoctorIssue {
            id: "unknown_cause".into(), title: "Causa não identificada".into(), severity: "info".into(),
            explanation: "O launcher registrou uma falha, mas não há evidência suficiente para identificar sua causa. Nomes em stack traces não identificam o responsável.".into(),
            evidence: vec![DoctorEvidence { source: "Resultado registrado pelo launcher".into(), summary: "A tentativa terminou com falha; nenhuma regra conclusiva foi encontrada nos dados associados disponíveis.".into() }],
            confidence: "medium".into(), kind: "fact".into(),
            next_step: "Tente iniciar novamente e analise em seguida. São necessários logs produzidos nessa tentativa ou um erro explícito de preparação; não remova mods por suposição.".into(),
        });
    }
    let associated = report.logs.iter().any(|log| log.association == "last_run");
    report.status = if !report.issues.is_empty() {
        "issues"
    } else if report.logs.is_empty() {
        "no_logs"
    } else if !run
        .as_ref()
        .is_some_and(|run| matches!(run.outcome.as_str(), "success" | "cancelled" | "failed"))
        || (!associated && failed)
        || coverage_limited
    {
        "incomplete"
    } else {
        "no_issues"
    }
    .into();
    report.summary = match run.as_ref().map(|run| safe_outcome(&run.outcome)) {
        Some("cancelled") => "A última execução foi encerrada por solicitação do jogador; ela não foi classificada como crash.",
        Some("success") => "A última execução terminou normalmente. Veja abaixo as verificações da configuração atual.",
        Some("running" | "preparing") => "A tentativa ainda está em andamento. O diagnóstico definitivo depende do encerramento e dos logs completos.",
        Some("failed") if current_rule_found => "Foram encontrados indicadores explícitos associados à última tentativa que falhou.",
        Some("failed") => "Causa não identificada para a última tentativa que falhou.",
        _ if report.logs.is_empty() => "Ainda não há logs disponíveis para esta instância. Veja as verificações de configuração abaixo.",
        _ => "Não há resultado conclusivo registrado para uma execução. Os logs existentes não foram usados como causa de uma tentativa atual.",
    }.into();
    report.sanitized_json = serde_json::to_string_pretty(&report)
        .map_err(|_| "Não foi possível gerar o relatório sanitizado.".to_owned())?;
    Ok(report)
}

fn safe_minecraft_version(value: &str) -> String {
    if (3..=24).contains(&value.len())
        && (value.starts_with("1.") || value.starts_with("26."))
        && value
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
    {
        value.into()
    } else {
        "Não identificada".into()
    }
}

fn safe_outcome(value: &str) -> &'static str {
    match value {
        "preparing" => "preparing",
        "running" => "running",
        "success" => "success",
        "failed" => "failed",
        "cancelled" => "cancelled",
        _ => "unknown",
    }
}

fn safe_timestamp(value: u64) -> Option<u64> {
    (value > 0 && value <= 4_102_444_800_000).then_some(value)
}

fn loader_label(version: &str) -> &'static str {
    let lower = version.to_ascii_lowercase();
    if lower.contains("fabric") {
        "Fabric"
    } else if lower.contains("forge") {
        "Forge"
    } else {
        "Vanilla ou não identificado"
    }
}

fn secure_root(instance: &Instance) -> Result<(), String> {
    for ancestor in instance.root().ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(|_| SAFE_READ_ERROR.to_owned())?;
        if metadata_is_link(&metadata) || !metadata.is_dir() {
            return Err(SAFE_READ_ERROR.into());
        }
    }
    Ok(())
}

fn secure_path(instance: &Instance, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty()
        || !Path::new(relative)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(SAFE_READ_ERROR.into());
    }
    secure_root(instance)?;
    ensure_no_symlink_components(instance.root(), relative)
        .map_err(|_| SAFE_READ_ERROR.to_owned())?;
    let path = instance.root().join(relative);
    if path.exists() {
        let canonical = path
            .canonicalize()
            .map_err(|_| SAFE_READ_ERROR.to_owned())?;
        let root = instance
            .root()
            .canonicalize()
            .map_err(|_| SAFE_READ_ERROR.to_owned())?;
        if !canonical.starts_with(root) {
            return Err(SAFE_READ_ERROR.into());
        }
    }
    Ok(path)
}

fn open_file(instance: &Instance, relative: &str) -> Result<File, String> {
    let path = secure_path(instance, relative)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(path).map_err(|_| SAFE_READ_ERROR.to_owned())?;
    let metadata = file.metadata().map_err(|_| SAFE_READ_ERROR.to_owned())?;
    if !metadata.is_file() || metadata_is_link(&metadata) {
        return Err(SAFE_READ_ERROR.into());
    }
    Ok(file)
}

fn read_segment(
    instance: &Instance,
    relative: &str,
    offset: u64,
    length: u64,
) -> Result<Vec<u8>, String> {
    let mut file = open_file(instance, relative)?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| SAFE_READ_ERROR.to_owned())?;
    let mut bytes = Vec::new();
    file.take(length)
        .read_to_end(&mut bytes)
        .map_err(|_| SAFE_READ_ERROR.to_owned())?;
    Ok(bytes)
}

fn stamp(instance: &Instance, relative: &str) -> Result<DoctorLogStamp, String> {
    let mut file = open_file(instance, relative)?;
    let metadata = file.metadata().map_err(|_| SAFE_READ_ERROR.to_owned())?;
    let mut prefix = Vec::new();
    file.by_ref()
        .take(HEAD_BYTES.min(metadata.len()))
        .read_to_end(&mut prefix)
        .map_err(|_| SAFE_READ_ERROR.to_owned())?;
    Ok(DoctorLogStamp {
        relative_path: relative.into(),
        size: metadata.len(),
        modified_at_ms: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|time| time.as_millis() as u64)
            .unwrap_or(0),
        head_sha256: format!("{:x}", Sha256::digest(prefix)),
    })
}

fn log_candidates(instance: &Instance) -> (Vec<String>, bool) {
    let mut paths = Vec::new();
    let mut limited = false;
    for relative in ["logs/aurora-java.log", "logs/latest.log", "logs/debug.log"] {
        match secure_path(instance, relative) {
            Ok(path) if path.is_file() => paths.push(relative.into()),
            Ok(_) => {}
            Err(_) => limited = true,
        }
    }
    if let Ok(directory) = secure_path(instance, "crash-reports") {
        if let Ok(entries) = fs::read_dir(directory) {
            let mut crashes = Vec::new();
            for (index, entry) in entries.enumerate() {
                if index >= MAX_DIRECTORY_ENTRIES {
                    limited = true;
                    break;
                }
                let Ok(entry) = entry else {
                    limited = true;
                    continue;
                };
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if !name.starts_with("crash-") || !name.ends_with(".txt") {
                    continue;
                }
                let relative = format!("crash-reports/{name}");
                if let Ok(stamp) = stamp(instance, &relative) {
                    crashes.push(stamp)
                } else {
                    limited = true
                }
            }
            crashes.sort_by(|a, b| {
                b.modified_at_ms
                    .cmp(&a.modified_at_ms)
                    .then(a.relative_path.cmp(&b.relative_path))
            });
            limited |= crashes.len() > MAX_LOGS - paths.len();
            paths.extend(
                crashes
                    .into_iter()
                    .take(MAX_LOGS - paths.len())
                    .map(|item| item.relative_path),
            );
        }
    } else {
        limited = true
    }
    (paths, limited)
}

enum Association {
    Current(u64),
    Historical,
    Unknown,
}

fn associate_log(
    instance: &Instance,
    current: &DoctorLogStamp,
    run: Option<&DoctorRun>,
) -> Association {
    let Some(run) = run else {
        return Association::Unknown;
    };
    // No tolerance before start: a timestamp from an older run must never be
    // promoted to current merely due to coarse timestamp precision.
    if current.modified_at_ms < run.started_at_ms {
        return Association::Historical;
    }
    if run
        .ended_at_ms
        .is_some_and(|end| current.modified_at_ms > end)
    {
        return Association::Unknown;
    }
    let Some(before) = run
        .logs_before
        .iter()
        .find(|before| before.relative_path == current.relative_path)
    else {
        return Association::Current(0);
    };
    if before.size == current.size && before.head_sha256 == current.head_sha256 {
        return Association::Historical;
    }
    if current.size >= before.size && before.size > 0 {
        if let Ok(prefix) = read_segment(
            instance,
            &current.relative_path,
            0,
            HEAD_BYTES.min(before.size),
        ) {
            if format!("{:x}", Sha256::digest(prefix)) == before.head_sha256 {
                return if current.size > before.size {
                    Association::Current(before.size)
                } else {
                    Association::Unknown
                };
            }
        }
    }
    Association::Current(0)
}

fn log_label(relative: &str) -> &'static str {
    match relative {
        "logs/aurora-java.log" => "Saída Java do launcher",
        "logs/latest.log" => "Log atual do Minecraft",
        "logs/debug.log" => "Log de depuração do loader",
        _ => "Relatório de crash da instância",
    }
}

fn decode_log(bytes: &[u8]) -> String {
    let le = bytes.starts_with(&[0xff, 0xfe]);
    let be = bytes.starts_with(&[0xfe, 0xff]);
    // Tails may no longer include a BOM. Detect UTF-16 ASCII structure without
    // adding an encoding dependency; all markers are ASCII or loss-tolerant.
    let sample = &bytes[..bytes.len().min(256)];
    let even_zero = sample.iter().step_by(2).filter(|&&b| b == 0).count();
    let odd_zero = sample
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|&&b| b == 0)
        .count();
    let looks_le = sample.len() >= 16 && odd_zero > sample.len() / 6;
    let looks_be = sample.len() >= 16 && even_zero > sample.len() / 6;
    if le || be || looks_le || looks_be {
        let offset = if le || be { 2 } else { 0 };
        let big_endian = be || (!le && looks_be && !looks_le);
        let units = bytes[offset..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                if big_endian {
                    u16::from_be_bytes([pair[0], pair[1]])
                } else {
                    u16::from_le_bytes([pair[0], pair[1]])
                }
            })
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn check_profile(instance: &Instance, version_id: &str, report: &mut DoctorReport) {
    if version_id.is_empty()
        || version_id.len() > 160
        || !version_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        || matches!(version_id, "." | "..")
    {
        report
            .limitations
            .push("O perfil instalado não pôde ser identificado com segurança.".into());
        return;
    }
    let relative = format!("versions/{version_id}/{version_id}.json");
    let Ok(path) = secure_path(instance, &relative) else {
        report
            .limitations
            .push("O caminho do perfil foi recusado pela verificação de segurança.".into());
        return;
    };
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => add_issue(report, &RULES[5], "Perfil instalado", "O JSON do perfil selecionado está ausente. A instalação pode ainda não ter sido concluída."),
        Ok(metadata) if metadata.len() > MAX_PROFILE_BYTES => report.limitations.push("O perfil excede 2 MiB e não foi processado.".into()),
        Ok(_) => if let Ok(bytes) = read_segment(instance, &relative, 0, MAX_PROFILE_BYTES + 1) {
            if bytes.len() as u64 > MAX_PROFILE_BYTES { report.limitations.push("O perfil mudou durante a análise e ultrapassou o limite de leitura.".into()); }
            else if serde_json::from_slice::<serde_json::Value>(&bytes).is_err() { add_issue(report, &RULES[6], "Perfil instalado", "O perfil selecionado não contém JSON válido. Um arquivo ainda em gravação também pode produzir este resultado."); }
        } else { report.limitations.push("Não foi possível ler o perfil instalado.".into()); },
        Err(_) => report.limitations.push("Não foi possível consultar o perfil instalado.".into()),
    }
}

struct Rule {
    id: &'static str,
    title: &'static str,
    canonical: &'static str,
    explanation: &'static str,
    next_step: &'static str,
}

#[cfg(test)]
impl Rule {
    fn matches(&self, lower: &str) -> bool {
        lower.trim().eq_ignore_ascii_case(self.canonical)
            || rules::detect(lower)
                .iter()
                .any(|finding| finding.id == self.id)
    }
}

static RULES: &[Rule] = &[
    Rule { id: "java_missing", title: "Java selecionado ausente", canonical: "Java selecionado não encontrado.", explanation: "O Java selecionado não está disponível para iniciar o jogo.", next_step: "Selecione uma instalação Java disponível nas configurações e tente novamente." },
    Rule { id: "java_incompatible", title: "Java incompatível", canonical: "Java incompatível com a instância.", explanation: "O Java selecionado não atende ao requisito da instância ou ao formato de uma classe carregada.", next_step: "Selecione o Java exigido pelo perfil e pelos mods. As evidências podem indicar a versão necessária." },
    Rule { id: "missing_dependency", title: "Dependência obrigatória ausente", canonical: "Dependência obrigatória ausente.", explanation: "O loader informou que falta uma dependência necessária para carregar o conteúdo.", next_step: "Confira a dependência no erro local e instale a versão indicada para o mesmo Minecraft e loader do modpack." },
    Rule { id: "incompatible_version", title: "Versões incompatíveis declaradas", canonical: "Incompatibilidade explícita de versão.", explanation: "O loader recusou uma versão, um ambiente ou uma combinação de conteúdo incompatível.", next_step: "Compare as versões exigidas no erro local com as instaladas e use a combinação indicada pelo autor do modpack." },
    Rule { id: "duplicate_mod", title: "Identificador de mod duplicado", canonical: "Identificador de mod duplicado confirmado pelo loader.", explanation: "O loader encontrou mais de um arquivo declarando o mesmo identificador de mod.", next_step: "Confira os arquivos apontados pelo loader antes de ajustar as cópias instaladas. Nomes parecidos não bastam para decidir." },
    Rule { id: "missing_file", title: "Arquivo obrigatório ausente ou inacessível", canonical: "Arquivo obrigatório ausente ou inacessível.", explanation: "A preparação não encontrou ou não conseguiu acessar um arquivo obrigatório.", next_step: "Confira o arquivo apontado no erro local e se a instalação terminou. Reinstale o componente pela fonte oficial quando necessário." },
    Rule { id: "corrupt_file", title: "Arquivo inválido ou integridade divergente", canonical: "Arquivo inválido ou corrompido.", explanation: "Um arquivo falhou na verificação de formato ou integridade.", next_step: "Aguarde instalações em andamento. Se o erro persistir, obtenha novamente o arquivo afetado pela fonte oficial." },
    Rule { id: "memory_failure", title: "Falha explícita de memória", canonical: "Falha explícita de memória da JVM.", explanation: "A JVM informou falta de memória ou falha ao reservar memória.", next_step: "Feche outros programas e confira a RAM do perfil. Aumente-a apenas com memória livre; falhas de reserva podem exigir reduzi-la." },
    Rule { id: "network_failure", title: "Falha registrada de rede ou download", canonical: "Falha de rede ou download na inicialização.", explanation: "Uma conexão ou transferência falhou. Isso pode envolver um serviço opcional e não confirma sozinho a causa do encerramento.", next_step: "Confira a conexão e a disponibilidade da fonte indicada no log local. Tente novamente quando o acesso estiver restabelecido." },
    Rule { id: "mixin_failure", title: "Falha ao aplicar mixins", canonical: "Falha de aplicação de mixins observada.", explanation: "A transformação de classes falhou. Esse registro não identifica o mod responsável nem confirma a causa original.", next_step: "Confira se as versões de Minecraft, loader e mods correspondem ao modpack. Investigue a primeira exceção associada antes de remover conteúdo." },
    Rule { id: "graphics_failure", title: "Falha ao iniciar o OpenGL", canonical: "Falha explícita de inicialização gráfica.", explanation: "A biblioteca gráfica não conseguiu criar o contexto OpenGL exigido pelo jogo.", next_step: "Confira o driver no site do fabricante da GPU e o suporte da placa ao Minecraft escolhido. Em acesso remoto, tente uma sessão local." },
    Rule { id: "disk_space", title: "Espaço insuficiente no disco", canonical: "Falha explícita por falta de espaço no disco.", explanation: "Uma operação de arquivo foi recusada porque o volume não tem espaço suficiente.", next_step: "Confira o espaço livre no disco da instância e na pasta temporária. Libere espaço com cuidado e tente novamente." },
    Rule { id: "file_permission", title: "Acesso a arquivo recusado", canonical: "Falha explícita de permissão de arquivo.", explanation: "O sistema recusou uma operação de leitura ou gravação de arquivo.", next_step: "Confira as permissões da pasta e se outro programa bloqueou o arquivo. Use uma conta com acesso aos arquivos da instância." },
];

static LEGACY_DEPENDENCY_RULE: Rule = Rule {
    id: "missing_dependency",
    title: "Dependência obrigatória não atendida",
    canonical: LEGACY_DEPENDENCY_FAILURE,
    explanation: "O registro antigo indica uma dependência não atendida, sem distinguir ausência de versão incompatível.",
    next_step: "Confira se a dependência está instalada na versão exigida. Inicie novamente para obter evidências atuais antes de alterar o conteúdo.",
};

fn inspect_text(text: &str, source: &str, report: &mut DoctorReport) -> bool {
    if text.trim().eq_ignore_ascii_case(LEGACY_DEPENDENCY_FAILURE) {
        add_issue(report, &LEGACY_DEPENDENCY_RULE, source,
            "O registro de uma versão anterior do launcher confirma uma dependência não atendida; ele não distingue ausência de incompatibilidade.");
        return true;
    }
    if let Some(rule) = RULES
        .iter()
        .find(|rule| text.trim().eq_ignore_ascii_case(rule.canonical))
    {
        add_issue(report, rule, source, rule.canonical);
        return !matches!(rule.id, "mixin_failure" | "network_failure");
    }
    let mut specific_indicator = false;
    for finding in rules::detect(text) {
        if let Some(rule) = RULES.iter().find(|rule| rule.id == finding.id) {
            add_issue(report, rule, source, &finding.evidence);
            specific_indicator |= !matches!(rule.id, "mixin_failure" | "network_failure");
        }
    }
    specific_indicator
}

fn add_issue(report: &mut DoctorReport, rule: &Rule, source: &str, evidence: &str) {
    if let Some(existing) = report.issues.iter_mut().find(|issue| issue.id == rule.id) {
        if !existing.evidence.iter().any(|item| item.source == source)
            && existing.evidence.len() < 4
        {
            existing.evidence.push(DoctorEvidence {
                source: source.into(),
                summary: evidence.into(),
            });
        }
        return;
    }
    report.issues.push(DoctorIssue {
        id: rule.id.into(),
        title: rule.title.into(),
        severity: if matches!(rule.id, "network_failure" | "mixin_failure") {
            "warning"
        } else {
            "error"
        }
        .into(),
        explanation: rule.explanation.into(),
        evidence: vec![DoctorEvidence {
            source: source.into(),
            summary: evidence.into(),
        }],
        confidence: if matches!(rule.id, "network_failure" | "mixin_failure")
            || rule.canonical == LEGACY_DEPENDENCY_FAILURE
        {
            "medium"
        } else {
            "high"
        }
        .into(),
        kind: "fact".into(),
        next_step: rule.next_step.into(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // These are deliberately synthetic, sanitized loader/JVM fixtures.
    #[test]
    fn recognizes_explicit_synthetic_markers_without_copying_input() {
        for (text, id) in [
            (
                "java.lang.UnsupportedClassVersionError: example",
                "java_incompatible",
            ),
            (
                "Mod 'Synthetic' requires version 1 of example, which is missing!",
                "missing_dependency",
            ),
            (
                "Mod requires version 2 but only version 1 is present!",
                "incompatible_version",
            ),
            ("Duplicate versions for mod ID 'synthetic'", "duplicate_mod"),
            (
                "java.lang.OutOfMemoryError: Java heap space",
                "memory_failure",
            ),
            (
                "Error: Unable to access jarfile personal-secret",
                "missing_file",
            ),
            (
                "java.util.zip.ZipException: zip END header not found",
                "corrupt_file",
            ),
            (
                "java.net.UnknownHostException: token-secret.invalid",
                "network_failure",
            ),
        ] {
            let lower = text.to_lowercase();
            assert!(
                RULES
                    .iter()
                    .any(|rule| rule.id == id && rule.matches(&lower)),
                "{id}"
            );
            assert!(!canonical_launch_failure(text).contains("secret"));
        }
    }

    #[test]
    fn ambiguous_stack_frames_do_not_assign_blame() {
        for text in [
            "at example.mod.memory.SomeClass.invoke(SomeClass.java:42)",
            "ERROR example_mod startup",
            "Installed mods: duplicate synthetic names",
            "Dependency check succeeded",
            "Stopping!",
        ] {
            assert!(!RULES.iter().any(|rule| rule.matches(&text.to_lowercase())));
        }
    }

    #[test]
    fn decodes_utf16_and_invalid_utf8_lossily() {
        let fixture = "\njava.lang.OutOfMemoryError: synthetic\n";
        for big_endian in [false, true] {
            let mut bytes = if big_endian {
                vec![0xfe, 0xff]
            } else {
                vec![0xff, 0xfe]
            };
            for unit in fixture.encode_utf16() {
                bytes.extend(if big_endian {
                    unit.to_be_bytes()
                } else {
                    unit.to_le_bytes()
                });
            }
            assert!(decode_log(&bytes).contains("OutOfMemoryError"));
            assert!(decode_log(&bytes[2..]).contains("OutOfMemoryError"));
        }
        assert!(decode_log(b"\xff java.lang.OutOfMemoryError").contains("OutOfMemoryError"));
    }

    #[test]
    fn sanitizes_arbitrary_failure_and_context() {
        assert_eq!(
            canonical_launch_failure(
                "C:\\Users\\private-user\\secret token=abc user@example.invalid"
            ),
            "Falha de preparação sem causa identificada."
        );
        assert_eq!(safe_minecraft_version("1.20.1"), "1.20.1");
        assert_eq!(safe_minecraft_version("26.3"), "26.3");
        assert_eq!(
            safe_minecraft_version("user@example.invalid"),
            "Não identificada"
        );
        assert_eq!(safe_outcome("secret"), "unknown");
    }
}
