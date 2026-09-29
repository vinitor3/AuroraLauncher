import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import type { DoctorExplanation, DoctorIssue, DoctorReport, DoctorRun } from "./types";
import { GeminiAdvice } from "./GeminiAdvice";
import { DoctorRecovery } from "./DoctorRecovery";
import { AiToolSuggestions } from "../ai-tools/AiToolSuggestions";
import { AiToolProposalList } from "../ai-tools/AiToolProposalList";
import type { AiToolAdviceRequest } from "../ai-tools/types";
import "./doctor.css";

type Props = {
  instanceId: string;
  javaExecutable: string;
  runId?: string;
  busy: boolean;
  onExplain?: DoctorExplanation;
  onSuggest?: AiToolAdviceRequest;
  onSelectRun: (runId?: string) => void;
  onRecovery?: (pointId?: string) => void;
  onDiagnosticLab?: () => void;
};

type Analysis = {
  context: string;
  phase: "loading" | "ready" | "error";
  report?: DoctorReport;
};

const outcomes: Record<DoctorRun["outcome"], string> = {
  preparing: "Preparação em andamento",
  running: "Minecraft em execução",
  success: "Encerramento normal",
  failed: "Execução com falha",
  cancelled: "Execução cancelada",
  unknown: "Resultado não confirmado",
};

const associations = {
  last_run: "Desta execução",
  historical: "Histórico — não comprova a causa desta execução",
  unassociated: "Sem vínculo confirmado com a execução",
};

function runDate(timestamp: number | null) {
  if (timestamp == null) return undefined;
  const date = new Date(timestamp);
  return Number.isNaN(date.getTime()) ? undefined : date.toLocaleString("pt-BR");
}

function IssueDetails({ issue }: { issue: DoctorIssue }) {
  return (
    <details className={`doctor-issue doctor-issue-${issue.severity}`}>
      <summary>
        <span className={`doctor-severity ${issue.severity}`}>{issue.severity === "error" ? "Erro" : issue.severity === "warning" ? "Atenção" : "Informação"}</span>
        <strong>{issue.title}</strong>
        <span className="doctor-issue-kind">{issue.kind === "fact" ? "Fato observado" : "Hipótese"}</span>
        <span aria-hidden="true" className="doctor-chevron">⌄</span>
      </summary>
      <div className="doctor-issue-body">
        <p>{issue.explanation}</p>
        <p className="doctor-confidence">Confiança {issue.confidence === "high" ? "alta" : "moderada"} · {issue.kind === "fact" ? "Sustentado pelas evidências abaixo" : "Precisa de confirmação"}</p>
        <h4>Evidências</h4>
        <ul className="doctor-evidence">
          {issue.evidence.map((evidence, index) => <li key={`${evidence.source}-${index}`}><strong>{evidence.source}</strong><span>{evidence.summary}</span></li>)}
        </ul>
        <div className="doctor-next-step"><h4>Próximo passo</h4><p>{issue.nextStep}</p></div>
      </div>
    </details>
  );
}

export function DoctorView({ instanceId, javaExecutable, runId, busy, onSelectRun, onExplain, onSuggest, onRecovery, onDiagnosticLab }: Props) {
  // Reports are tied to the exact inputs as well as a request generation. Changing
  // instance, Java or run never makes an earlier response visible in a new context.
  const context = JSON.stringify([instanceId, javaExecutable, runId ?? null]);
  const [analysis, setAnalysis] = useState<Analysis>();
  const [latestRun, setLatestRun] = useState<{ context: string; run: DoctorRun | null }>();
  const [shareMessage, setShareMessage] = useState("");
  const generation = useRef(0);
  const currentContext = useRef(context);
  currentContext.current = context;
  const current = analysis?.context === context ? analysis : undefined;
  const report = current?.report;
  const leadIssue = report?.issues.find((issue) => issue.severity === "error")
    ?? report?.issues.find((issue) => issue.severity === "warning")
    ?? report?.issues[0];
  const loading = current?.phase === "loading";
  const latest = latestRun?.context === context ? latestRun.run : undefined;

  useEffect(() => {
    const request = ++generation.current;
    let disposed = false;
    const stopListening: Array<() => void> = [];
    setShareMessage("");
    const invalidate = (event: { payload: { instanceId: string } }) => {
      if (disposed || event.payload.instanceId !== instanceId || currentContext.current !== context) return;
      generation.current += 1;
      setAnalysis(undefined);
      setShareMessage("");
    };
    void listen<DoctorRun>("aurora-doctor-run", (event) => {
      invalidate(event);
      if (!disposed && event.payload.instanceId === instanceId) setLatestRun({ context, run: event.payload });
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stopListening.push(unlisten);
    }).catch(() => undefined);
    void listen<{ instanceId: string }>("aurora-ai-tool-changed", invalidate).then((unlisten) => {
      if (disposed) unlisten(); else stopListening.push(unlisten);
    }).catch(() => undefined);
    void listen<{ instanceId: string }>("aurora-recovery-changed", invalidate).then((unlisten) => {
      if (disposed) unlisten(); else stopListening.push(unlisten);
    }).catch(() => undefined);
    void listen<{ instanceId: string }>("aurora-diagnostic-lab-changed", invalidate).then((unlisten) => {
      if (disposed) unlisten(); else stopListening.push(unlisten);
    }).catch(() => undefined);
    void invoke<DoctorRun | null>("read_latest_doctor_run", { id: instanceId }).then((run) => {
      if (generation.current === request && currentContext.current === context) setLatestRun({ context, run });
    }).catch(() => {
      // A missing run does not prevent the read-only analysis from reporting its
      // own coverage and limitations. Never manufacture a failed run here.
    });
    return () => { disposed = true; generation.current += 1; stopListening.forEach((stop) => stop()); };
  }, [context, instanceId]);

  async function analyze() {
    const request = ++generation.current;
    setShareMessage("");
    setAnalysis({ context, phase: "loading" });
    try {
      const result = await invoke<DoctorReport>("analyze_instance", {
        id: instanceId,
        // The backend reads the same saved profile used by launchMinecraft.
        // In explicit run mode it resolves that run's recorded launch context.
        javaExecutable: javaExecutable.trim() || null,
        runId: runId ?? null,
      });
      if (generation.current === request && currentContext.current === context) {
        setAnalysis({ context, phase: "ready", report: result });
      }
    } catch {
      if (generation.current === request && currentContext.current === context) setAnalysis({ context, phase: "error" });
    }
  }

  async function copyReport() {
    if (!report) return;
    const sharedContext = context;
    try {
      await navigator.clipboard.writeText(report.sanitizedJson);
      if (currentContext.current === sharedContext) setShareMessage("Relatório sanitizado copiado.");
    } catch {
      if (currentContext.current === sharedContext) setShareMessage("Não foi possível copiar. Use Exportar ou selecione o relatório abaixo.");
    }
  }

  function exportReport() {
    if (!report) return;
    let url: string | undefined;
    try {
      url = URL.createObjectURL(new Blob([report.sanitizedJson], { type: "application/json;charset=utf-8" }));
      const link = document.createElement("a");
      link.href = url;
      // Never include instance names, player names or local paths in this file.
      link.download = "aurora-doctor-relatorio.json";
      document.body.appendChild(link);
      link.click();
      link.remove();
      setShareMessage("Exportação solicitada. O relatório contém somente os dados sanitizados exibidos abaixo.");
    } catch {
      setShareMessage("Não foi possível exportar. Copie o relatório ou selecione o texto abaixo.");
    } finally {
      if (url) window.setTimeout(() => URL.revokeObjectURL(url!), 1000);
    }
  }

  return (
    <section aria-label="Aurora Doctor" aria-busy={loading} className="doctor-view">
      <header className="doctor-heading">
        <div><p className="eyebrow">AURORA DOCTOR</p><h2>Diagnóstico da instância</h2><p>Análise local. Nenhum arquivo é alterado.</p></div>
        <button disabled={loading || busy} onClick={() => void analyze()} type="button">{loading ? "Analisando…" : "Analisar instância"}</button>
      </header>
      <div className="doctor-scope">
        <span>{runId ? "Execução selecionada pelo atalho de falha" : "Perfil salvo da instância e Java selecionado no launcher"}</span>
        {runId ? <button className="text-button" onClick={() => onSelectRun(undefined)} type="button">Voltar ao estado atual</button>
          : latest?.outcome === "failed" && <button className="text-button" onClick={() => onSelectRun(latest.runId)} type="button">Analisar a última execução com falha</button>}
      </div>
      {onRecovery && <DoctorRecovery instanceId={instanceId} runId={runId} busy={busy} onReview={onRecovery} />}
      {onDiagnosticLab && <div className="doctor-scope"><span>Teste mudanças numa cópia independente da instância, sem incluir mundos por padrão.</span><button className="text-button" disabled={busy} onClick={onDiagnosticLab} type="button">Abrir ambiente de teste</button></div>}
      {loading ? <div className="doctor-state" role="status"><span className="doctor-spinner" aria-hidden="true" /><strong>Analisando os dados locais…</strong><p>A leitura dos logs é limitada. Você pode continuar usando o launcher.</p></div>
        : current?.phase === "error" ? <div className="doctor-state doctor-state-error" role="alert"><strong>Não foi possível concluir a análise</strong><p>Os dados podem estar indisponíveis ou a execução selecionada já não estar acessível. Tente novamente ou analise o estado atual.</p></div>
        : !report ? <div className="doctor-state"><span aria-hidden="true" className="doctor-state-icon">⌕</span><strong>Entenda o que aconteceu</strong><p>Clique em “Analisar instância” para obter evidências, grau de confiança e próximos passos. Nenhum log será enviado a serviços externos.</p></div>
        : <>
          <div className={`doctor-result-summary doctor-result-${report.status}`} role="status">
            <span className="doctor-summary-label">DIAGNÓSTICO LOCAL</span>
            <strong>{leadIssue?.title ?? (report.status === "no_logs" ? "Nenhum log disponível" : report.status === "no_issues" ? "Nenhum problema detectado" : "Diagnóstico limitado")}</strong>
            <p>{leadIssue?.explanation ?? report.summary}</p>
            {leadIssue && <div className="doctor-essential-next"><span>O que fazer</span><p>{leadIssue.nextStep}</p></div>}
            <div className="doctor-summary-meta">
              {leadIssue && <small>{leadIssue.kind === "fact" ? "Fato observado" : "Hipótese"} · Confiança {leadIssue.confidence === "high" ? "alta" : "moderada"}</small>}
              {report.lastRun && <small>{runId ? "Execução selecionada" : "Última execução"}: {outcomes[report.lastRun.outcome]}</small>}
              {report.issues.length > 1 && <small>+{report.issues.length - 1} {report.issues.length === 2 ? "ponto nos detalhes" : "pontos nos detalhes"}</small>}
            </div>
            {report.status === "no_issues" && <small>Limitado às verificações e aos dados disponíveis.</small>}
          </div>
          {onExplain && <GeminiAdvice instanceId={instanceId} report={report} explain={onExplain} busy={busy} />}
          {onSuggest && <AiToolSuggestions instanceId={instanceId} report={report} requestAdvice={onSuggest} busy={busy} />}
          {report.issues.length > 0 && <details className="doctor-details">
            <summary>Evidências e detalhes ({report.issues.length})</summary>
            <div aria-label="Problemas identificados" className="doctor-issues">{report.issues.map((issue) => <IssueDetails issue={issue} key={issue.id} />)}</div>
          </details>}
          <details className="doctor-coverage"><summary>Dados da instância, fontes e limites</summary>
          <dl className="doctor-context">
            <div><dt>Minecraft</dt><dd>{report.instance.minecraftVersion || "Não identificado"}</dd></div>
            <div><dt>Loader</dt><dd>{report.instance.loader || "Não identificado"}</dd></div>
            <div><dt>Java</dt><dd>{report.instance.javaMajor ? `Java ${report.instance.javaMajor}` : "Não identificado"}{report.instance.requiredJavaMajor > 0 && <small>Requisito base: Java {report.instance.requiredJavaMajor}</small>}</dd></div>
            <div><dt>Memória máxima</dt><dd>{report.instance.maxMemoryMb > 0 ? `${report.instance.maxMemoryMb} MB` : "Não identificada"}</dd></div>
            <div><dt>{runId ? "Execução selecionada" : "Última execução"}</dt><dd>{report.lastRun ? outcomes[report.lastRun.outcome] : "Sem registro"}{report.lastRun?.exitCode != null && <small>Código de saída: {report.lastRun.exitCode}</small>}</dd></div>
          </dl>
          {report.lastRun && <div className="doctor-run-context">
            {report.lastRun.runId && <span>Execução analisada: <code>{report.lastRun.runId}</code></span>}
            {runDate(report.lastRun.startedAtMs) && <span>Início: {runDate(report.lastRun.startedAtMs)}</span>}
            {runDate(report.lastRun.endedAtMs) && <span>Fim: {runDate(report.lastRun.endedAtMs)}</span>}
          </div>}
            <p>{report.summary}</p>
            {report.logs.length > 0 ? <ul>{report.logs.map((log, index) => <li key={`${log.source}-${index}`}><strong>{log.source}</strong><span>{associations[log.association]} · {log.bytesRead.toLocaleString("pt-BR")} bytes lidos{log.truncated ? " · leitura parcial" : ""}</span></li>)}</ul> : <p>Não há logs legíveis disponíveis para esta análise.</p>}
            {report.limitations.length > 0 && <ul>{report.limitations.map((limitation, index) => <li key={index}>{limitation}</li>)}</ul>}
          </details>
          <details aria-label="Compartilhar relatório sanitizado" className="doctor-share">
            <summary>Relatório sanitizado · copiar ou exportar</summary>
            <div className="doctor-share-body">
            <div><strong>Relatório sanitizado</strong><p>Revise o conteúdo antes de compartilhar. A exportação e a cópia usam somente este relatório.</p></div>
            <div className="doctor-share-actions"><button className="secondary compact" onClick={() => void copyReport()} type="button">Copiar relatório</button><button className="secondary compact" onClick={exportReport} type="button">Exportar JSON</button></div>
            {shareMessage && <p className="doctor-share-message" role="status">{shareMessage}</p>}
            <details><summary>Ver conteúdo do relatório</summary><textarea aria-label="Conteúdo do relatório sanitizado" readOnly spellCheck={false} value={report.sanitizedJson} /></details>
            </div>
          </details>
        </>}
      {!report && onSuggest && <AiToolProposalList instanceId={instanceId} disabled={busy || loading} />}
    </section>
  );
}
