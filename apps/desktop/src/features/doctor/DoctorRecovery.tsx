import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { recoveryDate, type RecoveryPoint, type RecoveryUnavailablePoint } from "../recovery/types";

export type DoctorRecoveryOverview = {
  status: "available" | "insufficient" | "unavailable";
  instanceUuid?: string;
  runId?: string;
  baselineRunId?: string;
  currentStateChanged: boolean;
  changes: { path: string; category: "mods" | "config" | "profile"; action: "added" | "removed" | "modified"; field?: string }[];
  points: { point: RecoveryPoint; reasons: string[] }[];
  unavailablePoints: RecoveryUnavailablePoint[];
  limitations: string[];
  attempts: { runId: string; outcome: string; observedAtMs: number; endedAtMs: number | null; hasLaunchSnapshot: boolean }[];
};

const actions = { added: "Adicionado", removed: "Removido", modified: "Alterado" };
const categories = { mods: "Mod", config: "Configuração", profile: "Perfil" };

export function DoctorRecovery({ instanceId, runId, busy, onReview }: {
  instanceId: string; runId?: string; busy: boolean; onReview: (pointId?: string) => void;
}) {
  const [revision, setRevision] = useState(0);
  const [result, setResult] = useState<{ context: string; value?: DoctorRecoveryOverview; failed?: boolean }>();
  const context = JSON.stringify([instanceId, runId ?? null, revision]);
  const current = result?.context === context ? result : undefined;
  useEffect(() => {
    let disposed = false;
    const stops: Array<() => void> = [];
    void invoke<DoctorRecoveryOverview>("doctor_recovery_overview", { id: instanceId, runId: runId ?? null })
      .then((value) => { if (!disposed) setResult({ context, value }); })
      .catch(() => { if (!disposed) setResult({ context, failed: true }); });
    for (const event of ["aurora-doctor-run", "aurora-recovery-changed", "aurora-ai-tool-changed", "aurora-diagnostic-lab-changed"]) {
      void listen<{ instanceId: string }>(event, ({ payload }) => {
        if (!disposed && payload.instanceId === instanceId) setRevision((value) => value + 1);
      }).then((stop) => { if (disposed) stop(); else stops.push(stop); }).catch(() => undefined);
    }
    return () => { disposed = true; stops.forEach((stop) => stop()); };
  }, [context, instanceId, runId]);

  const overview = current?.value;
  return <section aria-label="Mudanças recentes e recuperação" className="doctor-recovery">
    <header><div><strong>Mudanças recentes e recuperação</strong><p>Uma falha após uma mudança não comprova que ela causou o problema.</p></div><button className="secondary compact" disabled={busy} onClick={() => onReview()} type="button">Abrir Recuperação</button></header>
    {!current ? <p role="status">Verificando histórico e pontos desta instância…</p>
      : current.failed ? <div role="status"><p>Não foi possível verificar o histórico e os pontos. Nenhuma restauração foi iniciada.</p><button className="secondary compact" disabled={busy} onClick={() => setRevision((value) => value + 1)} type="button">Verificar novamente</button></div>
        : overview && <>
          {overview.status !== "available" && <p role="status">{overview.status === "insufficient" ? "Dados insuficientes para comparar as mudanças desta tentativa." : "O histórico desta tentativa não está disponível para esta instância."}</p>}
          {!!overview.changes.length && <details className="doctor-recovery-changes" open={overview.changes.length <= 6}><summary>Mudanças verificadas ({overview.changes.length})</summary><ul>{overview.changes.map((change, index) => <li key={`${change.path}:${change.field ?? ""}:${index}`}><span>{categories[change.category]} · {actions[change.action]}</span><code>{change.path}{change.field ? ` · ${change.field}` : ""}</code></li>)}</ul></details>}
          {overview.status === "available" && overview.changes.length === 0 && <p>Nenhuma diferença verificada no escopo do histórico disponível.</p>}
          {overview.currentStateChanged && <p>O estado atual mudou desde a tentativa. Uma nova prévia comparará os arquivos atuais antes de restaurar.</p>}
          {overview.points.length ? <div className="doctor-recovery-points">{overview.points.map(({ point, reasons }) => <article key={point.id}><div><strong>{point.description || "Ponto sem descrição"}</strong><small>{recoveryDate(point.createdAt)} · Mundos não incluídos</small><ul>{reasons.map((reason, index) => <li key={index}>{reason}</li>)}</ul></div><button className="secondary compact" disabled={busy} onClick={() => onReview(point.id)} type="button">Revisar restauração</button></article>)}</div> : <p>Nenhum ponto de restauração relevante disponível para esta tentativa.</p>}
          {!!overview.unavailablePoints.length && <p role="status">{overview.unavailablePoints.length} ponto(s) indisponível(is): os dados não puderam ser verificados. Consulte Recuperação.</p>}
          {!!overview.limitations.length && <details><summary>Limites deste histórico</summary><ul>{overview.limitations.map((limitation, index) => <li key={index}>{limitation}</li>)}</ul></details>}
        </>}
  </section>;
}
