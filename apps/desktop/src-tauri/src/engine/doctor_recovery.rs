//! Private, local recovery evidence. This inventory is deliberately separate
//! from Doctor's sanitized report and must never be sent to an AI provider.
use super::recovery::{self, RecoveryObservation, RecoveryPoint, RecoveryUnavailablePoint};
use super::{Instance, InstanceMutationGuard};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::Path;

const ERROR: &str =
    "O histórico verificável da recuperação está indisponível. Nenhuma alteração foi aplicada.";
const MAX_SNAPSHOT: usize = 32 * 1024 * 1024;
const CAUSATION: &str = "Uma falha após uma mudança é uma associação temporal; não comprova que a mudança causou o problema.";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorRecoveryChange {
    pub path: String,
    pub category: String,
    pub action: String,
    pub field: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorRecoveryPoint {
    pub point: RecoveryPoint,
    pub reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorRecoveryOverview {
    pub status: String,
    pub instance_uuid: Option<String>,
    pub run_id: Option<String>,
    pub baseline_run_id: Option<String>,
    pub changes: Vec<DoctorRecoveryChange>,
    pub points: Vec<DoctorRecoveryPoint>,
    pub unavailable_points: Vec<RecoveryUnavailablePoint>,
    pub limitations: Vec<String>,
    pub current_state_changed: bool,
    pub attempts: Vec<DoctorRecoveryAttempt>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorRecoveryAttempt {
    pub run_id: String,
    pub outcome: String,
    pub observed_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub has_launch_snapshot: bool,
}

fn observer_owner() -> &'static str {
    static OWNER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    OWNER.get_or_init(|| uuid::Uuid::new_v4().to_string())
}
fn observed_outcome(outcome: String, owner: &str) -> String {
    if owner != observer_owner() && matches!(outcome.as_str(), "preparing" | "running") {
        "unknown".into()
    } else {
        outcome
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Snapshot {
    instance_uuid: String,
    fingerprint: String,
    files: Vec<recovery::RecoveryFile>,
    profile: BTreeMap<String, BTreeMap<String, Option<String>>>,
    runtime: BTreeMap<String, String>,
}
impl From<RecoveryObservation> for Snapshot {
    fn from(value: RecoveryObservation) -> Self {
        Self {
            instance_uuid: value.instance_uuid,
            fingerprint: value.fingerprint,
            files: value.files,
            profile: value.profile,
            runtime: value.runtime,
        }
    }
}

struct Audit {
    db: Connection,
    _directories: Vec<File>,
    _database: File,
}
struct Attempt {
    sequence: i64,
    run_id: String,
    preparing: Option<Snapshot>,
    launch: Option<Snapshot>,
    launch_attempted: bool,
    outcome: String,
}
impl Attempt {
    fn observed(&self) -> Option<&Snapshot> {
        if self.launch_attempted || matches!(self.outcome.as_str(), "running" | "success") {
            self.launch.as_ref()
        } else {
            self.preparing.as_ref()
        }
    }
}

fn valid_id(value: &str) -> Result<(), String> {
    if uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value) {
        Ok(())
    } else {
        Err(ERROR.into())
    }
}
fn storage_path(instance: &Instance) -> Result<std::path::PathBuf, String> {
    let data = instance
        .root()
        .parent()
        .and_then(Path::parent)
        .ok_or(ERROR)?;
    recovery::path(data, ".aurora/doctor-recovery").map_err(|_| ERROR.into())
}
impl Audit {
    fn open(instance: &Instance) -> Result<Self, String> {
        let root = storage_path(instance)?;
        fs::create_dir_all(&root).map_err(|_| ERROR)?;
        let directories = recovery::pin_directory(&root).map_err(|_| ERROR)?;
        for name in [
            "history.sqlite3",
            "history.sqlite3-journal",
            "history.sqlite3-wal",
            "history.sqlite3-shm",
        ] {
            let path = recovery::path(&root, name).map_err(|_| ERROR)?;
            if path.exists() {
                drop(recovery::shared_file(&path).map_err(|_| ERROR)?);
            }
        }
        let path = recovery::path(&root, "history.sqlite3").map_err(|_| ERROR)?;
        let db = Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::default() | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(|_| ERROR)?;
        let database = recovery::shared_file(&path).map_err(|_| ERROR)?;
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| ERROR)?;
        let version: u32 = db
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|_| ERROR)?;
        if version > 1 {
            return Err(ERROR.into());
        }
        db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS attempts(sequence INTEGER PRIMARY KEY AUTOINCREMENT,run_id TEXT NOT NULL UNIQUE,instance_uuid TEXT NOT NULL,preparing TEXT,launch TEXT,launch_attempted INTEGER NOT NULL DEFAULT 0,outcome TEXT NOT NULL DEFAULT 'preparing',observed_at_ms INTEGER NOT NULL,ended_at_ms INTEGER,observer_owner TEXT NOT NULL); CREATE INDEX IF NOT EXISTS instance_attempts ON attempts(instance_uuid,sequence); PRAGMA user_version=1;").map_err(|_| ERROR)?;
        Ok(Self {
            db,
            _directories: directories,
            _database: database,
        })
    }
    fn ensure_run(&self, instance_uuid: &str, run_id: &str) -> Result<(), String> {
        valid_id(instance_uuid)?;
        valid_id(run_id)?;
        self.db.execute("INSERT OR IGNORE INTO attempts(run_id,instance_uuid,observed_at_ms,observer_owner) VALUES(?1,?2,?3,?4)", params![run_id, instance_uuid, now_ms(), observer_owner()]).map_err(|_| ERROR)?;
        let owner: String = self
            .db
            .query_row(
                "SELECT instance_uuid FROM attempts WHERE run_id=?1",
                [run_id],
                |row| row.get(0),
            )
            .map_err(|_| ERROR)?;
        if owner != instance_uuid {
            return Err(ERROR.into());
        }
        Ok(())
    }
    fn attempt(
        &self,
        instance_uuid: &str,
        run_id: Option<&str>,
    ) -> Result<Option<Attempt>, String> {
        let row = self.db.query_row("SELECT sequence,run_id,substr(preparing,1,33554433),substr(launch,1,33554433),launch_attempted,outcome,observer_owner FROM attempts WHERE instance_uuid=?1 AND (?2 IS NULL OR run_id=?2) ORDER BY sequence DESC LIMIT 1", params![instance_uuid, run_id], |row| Ok((row.get::<_,i64>(0)?, row.get::<_,String>(1)?, row.get::<_,Option<String>>(2)?, row.get::<_,Option<String>>(3)?, row.get::<_,bool>(4)?, row.get::<_,String>(5)?, row.get::<_,String>(6)?))).optional().map_err(|_| ERROR)?;
        row.map(
            |(sequence, run_id, preparing, launch, launch_attempted, outcome, owner)| {
                Ok(Attempt {
                    sequence,
                    run_id,
                    preparing: decode(preparing, instance_uuid)?,
                    launch: decode(launch, instance_uuid)?,
                    launch_attempted,
                    outcome: observed_outcome(outcome, &owner),
                })
            },
        )
        .transpose()
    }
    fn history(&self, instance_uuid: &str) -> Result<Vec<DoctorRecoveryAttempt>, String> {
        let mut query = self.db.prepare("SELECT run_id,outcome,observed_at_ms,ended_at_ms,launch IS NOT NULL,observer_owner FROM attempts WHERE instance_uuid=?1 ORDER BY sequence DESC LIMIT 20").map_err(|_| ERROR)?;
        let attempts = query
            .query_map([instance_uuid], |row| {
                Ok(DoctorRecoveryAttempt {
                    run_id: row.get(0)?,
                    outcome: observed_outcome(row.get(1)?, &row.get::<_, String>(5)?),
                    observed_at_ms: row.get(2)?,
                    ended_at_ms: row.get(3)?,
                    has_launch_snapshot: row.get(4)?,
                })
            })
            .map_err(|_| ERROR)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ERROR.to_owned());
        attempts
    }
    fn previous(&self, instance_uuid: &str, sequence: i64) -> Result<Option<Attempt>, String> {
        let previous: Option<String> = self.db.query_row("SELECT run_id FROM attempts WHERE instance_uuid=?1 AND sequence<?2 ORDER BY sequence DESC LIMIT 1", params![instance_uuid, sequence], |row| row.get(0)).optional().map_err(|_| ERROR)?;
        previous
            .map(|id| self.attempt(instance_uuid, Some(&id)))
            .transpose()
            .map(Option::flatten)
    }
}
fn decode(value: Option<String>, instance_uuid: &str) -> Result<Option<Snapshot>, String> {
    value
        .map(|value| {
            if value.len() > MAX_SNAPSHOT {
                return Err(ERROR.into());
            }
            let snapshot: Snapshot = serde_json::from_str(&value).map_err(|_| ERROR)?;
            if snapshot.instance_uuid != instance_uuid || snapshot.files.len() > 30_000 {
                return Err(ERROR.into());
            }
            Ok(snapshot)
        })
        .transpose()
}

/// Called under the existing idle-instance mutation lock. A failed launch
/// capture is recorded before reading files, so the preparation snapshot can
/// never masquerade as the state that actually reached the JVM.
pub fn record_observation(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    run_id: &str,
    phase: &str,
) -> Result<(), String> {
    valid_id(run_id)?;
    if !matches!(phase, "preparing" | "launch") {
        return Err(ERROR.into());
    }
    let _directories = recovery::check_guard(instance, guard).map_err(|_| ERROR)?;
    if phase == "launch" {
        if let Some(identity) = recovery::identity_lookup(instance).map_err(|_| ERROR)? {
            let audit = Audit::open(instance)?;
            audit.ensure_run(&identity, run_id)?;
            audit
                .db
                .execute(
                    "UPDATE attempts SET launch_attempted=1 WHERE instance_uuid=?1 AND run_id=?2",
                    params![identity, run_id],
                )
                .map_err(|_| ERROR)?;
        }
    }
    let snapshot =
        Snapshot::from(recovery::capture_observation(instance, guard).map_err(|_| ERROR)?);
    let mut audit = Audit::open(instance)?;
    audit.ensure_run(&snapshot.instance_uuid, run_id)?;
    let serialized = serde_json::to_string(&snapshot).map_err(|_| ERROR)?;
    if serialized.len() > MAX_SNAPSHOT {
        return Err(ERROR.into());
    }
    let transaction = audit.db.transaction().map_err(|_| ERROR)?;
    let sql = if phase == "launch" {
        "SELECT launch FROM attempts WHERE run_id=?1"
    } else {
        "SELECT preparing FROM attempts WHERE run_id=?1"
    };
    let existing: Option<String> = transaction
        .query_row(sql, [run_id], |row| row.get(0))
        .map_err(|_| ERROR)?;
    if existing.as_ref().is_some_and(|value| value != &serialized) {
        return Err(
            "O estado desta tentativa já foi registrado e não pode ser substituído.".into(),
        );
    }
    let sql = if phase == "launch" {
        "UPDATE attempts SET launch=?1,launch_attempted=1 WHERE run_id=?2 AND instance_uuid=?3"
    } else {
        "UPDATE attempts SET preparing=?1 WHERE run_id=?2 AND instance_uuid=?3"
    };
    transaction
        .execute(sql, params![serialized, run_id, snapshot.instance_uuid])
        .map_err(|_| ERROR)?;
    transaction.commit().map_err(|_| ERROR)?;
    Ok(())
}

/// The execution lifecycle owns outcomes. A UI/advice payload cannot create an
/// association; this only updates an already observed, identity-bound run.
pub fn record_outcome(instance: &Instance, run_id: &str, outcome: &str) -> Result<(), String> {
    valid_id(run_id)?;
    if !matches!(
        outcome,
        "preparing" | "running" | "failed" | "success" | "cancelled" | "unknown"
    ) {
        return Err(ERROR.into());
    }
    let identity = recovery::identity_lookup(instance)
        .map_err(|_| ERROR)?
        .ok_or(ERROR)?;
    let audit = Audit::open(instance)?;
    let changed = audit.db.execute("UPDATE attempts SET outcome=?1,ended_at_ms=CASE WHEN ?1 IN ('failed','success','cancelled') THEN ?4 ELSE ended_at_ms END WHERE run_id=?2 AND instance_uuid=?3 AND outcome IN ('preparing','running','unknown')", params![outcome, run_id, identity, now_ms()]).map_err(|_| ERROR)?;
    if changed == 0 && audit.attempt(&identity, Some(run_id))?.is_none() {
        return Err(ERROR.into());
    }
    Ok(())
}

fn changes(before: &Snapshot, after: &Snapshot) -> Vec<DoctorRecoveryChange> {
    let left: BTreeMap<_, _> = before.files.iter().map(|file| (&file.path, file)).collect();
    let right: BTreeMap<_, _> = after.files.iter().map(|file| (&file.path, file)).collect();
    let mut changes = Vec::new();
    for path in left
        .keys()
        .chain(right.keys())
        .copied()
        .collect::<BTreeSet<_>>()
    {
        let (previous, next) = (left.get(path), right.get(path));
        if previous == next {
            continue;
        }
        changes.push(DoctorRecoveryChange {
            path: path.clone(),
            category: if path.starts_with("mods/") {
                "mods"
            } else {
                "config"
            }
            .into(),
            action: match (previous, next) {
                (None, _) => "added",
                (_, None) => "removed",
                _ => "modified",
            }
            .into(),
            field: None,
        });
    }
    for file in before
        .profile
        .keys()
        .chain(after.profile.keys())
        .collect::<BTreeSet<_>>()
    {
        let left = before.profile.get(file).cloned().unwrap_or_default();
        let right = after.profile.get(file).cloned().unwrap_or_default();
        for field in left.keys().chain(right.keys()).collect::<BTreeSet<_>>() {
            if left.get(field) == right.get(field) {
                continue;
            }
            changes.push(DoctorRecoveryChange {
                path: file.clone(),
                category: "profile".into(),
                action: "modified".into(),
                field: Some(field.clone()),
            });
        }
    }
    changes
}
fn point_matches(point: &RecoveryPoint, snapshot: &Snapshot) -> bool {
    point.files == snapshot.files && point.profile == snapshot.profile
}
fn relevant(
    point: RecoveryPoint,
    baseline: Option<&Snapshot>,
    observed: Option<&Snapshot>,
    changes: &[DoctorRecoveryChange],
) -> DoctorRecoveryPoint {
    let mut reasons = vec![
        "Identidade desta instância e conteúdo do ponto verificados pelo sistema de recuperação."
            .into(),
    ];
    if baseline.is_some_and(|baseline| point_matches(&point, baseline)) {
        reasons.push(
            "O conteúdo protegido coincide com o estado observado antes destas mudanças.".into(),
        );
    } else if observed.is_some_and(|observed| point_matches(&point, observed)) {
        reasons.push("O conteúdo protegido coincide com esta tentativa; restaurá-lo pode não alterar os arquivos observados.".into());
    } else if let Some(observed) = observed {
        let files: BTreeMap<_, _> = point.files.iter().map(|file| (&file.path, file)).collect();
        let observed_files: BTreeMap<_, _> = observed
            .files
            .iter()
            .map(|file| (&file.path, file))
            .collect();
        let overlap = changes
            .iter()
            .filter(|change| {
                if let Some(field) = &change.field {
                    point
                        .profile
                        .get(&change.path)
                        .and_then(|fields| fields.get(field))
                        != observed
                            .profile
                            .get(&change.path)
                            .and_then(|fields| fields.get(field))
                } else {
                    files.get(&change.path) != observed_files.get(&change.path)
                }
            })
            .count();
        if overlap > 0 {
            reasons.push(format!("Oferece outro estado para {overlap} item(ns) com mudança verificada nesta tentativa."));
        } else {
            reasons.push("Não há evidência suficiente para relacionar este conteúdo às mudanças observadas; revise a prévia.".into());
        }
    } else {
        reasons.push(
            "Sem inventário verificável da tentativa para comparar a relevância deste ponto."
                .into(),
        );
    }
    DoctorRecoveryPoint { point, reasons }
}

/// Does not apply a point or manufacture a preview. The existing recovery
/// commands must revalidate and confirm any selection made from this overview.
pub fn overview(
    instance: &Instance,
    guard: &InstanceMutationGuard,
    run_id: Option<&str>,
) -> Result<DoctorRecoveryOverview, String> {
    if let Some(id) = run_id {
        valid_id(id)?;
    }
    let current =
        Snapshot::from(recovery::capture_observation(instance, guard).map_err(|_| ERROR)?);
    let points = recovery::list_points(instance, guard).map_err(|_| ERROR)?;
    let mut result = DoctorRecoveryOverview {
        status: "insufficient".into(), instance_uuid: Some(current.instance_uuid.clone()), run_id: run_id.map(str::to_owned), baseline_run_id: None,
        changes: Vec::new(), points: Vec::new(), unavailable_points: points.unavailable_points,
        limitations: vec![CAUSATION.into(), "A comparação cobre somente o escopo verificável da recuperação; mundos NÃO são incluídos nem restaurados.".into(), "Sem erro em uma tentativa de teste é uma observação, não uma garantia de solução.".into()], current_state_changed: false, attempts: Vec::new(),
    };
    let audit = match Audit::open(instance) {
        Ok(audit) => Some(audit),
        Err(_) => {
            result.status = "unavailable".into();
            result.limitations.push(ERROR.into());
            None
        }
    };
    if let Some(audit) = &audit {
        match audit.history(&current.instance_uuid) {
            Ok(attempts) => result.attempts = attempts,
            Err(_) => {
                result.status = "unavailable".into();
                result.limitations.push(ERROR.into());
            }
        }
    }
    let attempt = match audit
        .as_ref()
        .map(|audit| audit.attempt(&current.instance_uuid, run_id))
        .transpose()
    {
        Ok(attempt) => attempt.flatten(),
        Err(_) => {
            result.status = "unavailable".into();
            result.limitations.push(ERROR.into());
            None
        }
    };
    let previous = match (&audit, &attempt) {
        (Some(audit), Some(attempt)) => audit.previous(&current.instance_uuid, attempt.sequence),
        _ => Ok(None),
    };
    let previous = match previous {
        Ok(previous) => previous,
        Err(_) => {
            result.status = "unavailable".into();
            result.limitations.push(ERROR.into());
            None
        }
    };
    let observed = attempt.as_ref().and_then(Attempt::observed);
    // The lab's durable provenance authorizes this cross-identity comparison:
    // content at each test launch versus the exact source state copied at creation.
    let creation_baseline = if super::diagnostic_lab::is_lab_candidate(instance) {
        Some(Snapshot::from(
            super::diagnostic_lab::baseline_observation(instance).map_err(|_| ERROR)?,
        ))
    } else {
        None
    };
    let baseline = creation_baseline.as_ref().or_else(|| {
        previous.as_ref().and_then(Attempt::observed).or_else(|| {
            attempt
                .as_ref()
                .filter(|attempt| attempt.launch.is_some())
                .and_then(|attempt| attempt.preparing.as_ref())
        })
    });
    if let Some(attempt) = &attempt {
        result.run_id = Some(attempt.run_id.clone());
    }
    if let Some(observed) = observed {
        result.current_state_changed = current.fingerprint != observed.fingerprint;
        if result.current_state_changed {
            result.limitations.push("A instância mudou depois do inventário desta tentativa. A prévia de restauração usará e revalidará o estado atual.".into());
        }
        if let Some(baseline) = baseline {
            result.changes = changes(baseline, observed);
            result.baseline_run_id = if creation_baseline.is_some() {
                None
            } else {
                previous
                    .as_ref()
                    .filter(|attempt| attempt.observed().is_some())
                    .map(|attempt| attempt.run_id.clone())
                    .or_else(|| result.run_id.clone())
            };
            if result.status != "unavailable" {
                result.status = "available".into();
            }
            if baseline.runtime != observed.runtime {
                result.limitations.push("A versão ou o loader mudou entre observações; pontos incompatíveis serão bloqueados na prévia.".into());
            }
            if creation_baseline.is_some() {
                result.limitations.push("As alterações testadas comparam o inventário desta tentativa ao estado da origem utilizado na criação da cópia; a proveniência da cópia foi verificada.".into());
            } else if previous.as_ref().and_then(Attempt::observed).is_none() {
                result.limitations.push("A comparação disponível cobre apenas a preparação desta tentativa; não há tentativa anterior verificável.".into());
            }
        } else {
            result.limitations.push("Não há inventário anterior verificável para afirmar quais mudanças ocorreram antes desta tentativa.".into());
        }
    } else {
        result.limitations.push("Não foi possível associar um inventário verificável ao estado desta tentativa. Nomes de pasta e horários não são usados como prova.".into());
    }
    result.points = points
        .points
        .into_iter()
        .map(|point| relevant(point, baseline, observed, &result.changes))
        .collect();
    if result.points.is_empty() {
        result.limitations.push(
            "Nenhum ponto de restauração disponível foi validado para esta instância.".into(),
        );
    }
    Ok(result)
}

#[cfg(test)]
#[path = "doctor_recovery_tests.rs"]
mod tests;
