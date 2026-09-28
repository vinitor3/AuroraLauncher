import { useEffect, useRef, useState } from "react";
import { buildDoctorAdviceInput, DOCTOR_ACTIONS, DOCTOR_ISSUE_LABELS, parseDoctorAdvice, type DoctorAdvice } from "../../../../../shared/doctorAdvice";
import type { DoctorExplanation, DoctorReport } from "./types";
import { publicAiError } from "../../../../../shared/aiErrors";
import { invoke } from "@tauri-apps/api/core";
import type { AiToolContext } from "../../../../../shared/aiTools";

type Props = { instanceId: string; report: DoctorReport; explain: DoctorExplanation; busy: boolean };
type Result = {
  instanceId: string;
  report: DoctorReport;
  explain: DoctorExplanation;
  phase: "loading" | "ready" | "error" | "cancelled";
  advice?: DoctorAdvice;
  error?: string;
  historyWarning?: boolean;
};

export function GeminiAdvice({ instanceId, report, explain, busy }: Props) {
  const [result, setResult] = useState<Result>();
  const generation = useRef(0);
  const pending = useRef<AbortController | undefined>(undefined);
  const currentInputs = useRef({ instanceId, report, explain });
  currentInputs.current = { instanceId, report, explain };
  const current = result?.instanceId === instanceId && result.report === report && result.explain === explain ? result : undefined;
  const loading = current?.phase === "loading";

  useEffect(() => {
    setResult(undefined);
    return () => {
      generation.current += 1;
      pending.current?.abort();
      pending.current = undefined;
    };
  }, [instanceId, report, explain]);

  function cancel() {
    generation.current += 1;
    pending.current?.abort();
    pending.current = undefined;
    setResult({ instanceId, report, explain, phase: "cancelled" });
  }

  async function requestExplanation() {
    if (busy || pending.current) return;
    const controller = new AbortController();
    const request = ++generation.current;
    pending.current = controller;
    setResult({ instanceId, report, explain, phase: "loading" });
    const stillCurrent = () => !controller.signal.aborted && generation.current === request
      && currentInputs.current.instanceId === instanceId && currentInputs.current.report === report && currentInputs.current.explain === explain;
    try {
      // Validate again at the display boundary. Model-authored repair text and
      // arbitrary evidence IDs are never accepted as actions or references.
      const input = buildDoctorAdviceInput(report);
      let historyContext: AiToolContext | undefined;
      try { historyContext = await invoke<AiToolContext>("ai_tools_context", { id: instanceId }); } catch { /* Explanation remains available if history storage is unavailable. */ }
      if (!stillCurrent()) return;
      const response = await explain(report, controller.signal);
      if (!stillCurrent()) return;
      const advice = parseDoctorAdvice(response, input);
      if (!advice) throw new Error("Invalid structured explanation");
      let historyWarning = !historyContext;
      if (historyContext) {
        try { await invoke("ai_record_advice", { id: instanceId, contextId: historyContext.contextId, advice: { summary: `${advice.summary} ${advice.cause}`.slice(0, 400), proposals: [] } }); }
        catch { historyWarning = true; }
      }
      if (stillCurrent()) setResult({ instanceId, report, explain, phase: "ready", advice, historyWarning });
    } catch (cause) {
      if (stillCurrent()) setResult({ instanceId, report, explain, phase: "error", error: publicAiError(cause) });
    } finally {
      if (pending.current === controller) pending.current = undefined;
    }
  }

  return (
    <section aria-label="Explicação opcional do Gemini" className="doctor-gemini" aria-busy={loading}>
      <div className="doctor-gemini-heading">
        <div><strong>Ajuda do Gemini</strong><p>Envia categorias de erros, versões e memória sanitizadas. Sem logs, nomes ou caminhos.</p></div>
        <div className="doctor-gemini-actions">
          <button className="secondary compact" disabled={busy || loading} onClick={() => void requestExplanation()} type="button">{loading ? "Consultando…" : current?.phase === "error" || current?.phase === "cancelled" ? "Tentar Gemini novamente" : current?.phase === "ready" ? "Explicar novamente" : "Explicar com Gemini"}</button>
          {loading && <button className="secondary compact" onClick={cancel} type="button">Cancelar</button>}
        </div>
      </div>
      {loading && <p className="doctor-gemini-status" role="status">Preparando uma explicação curta…</p>}
      {current?.phase === "error" && <p className="doctor-gemini-status" role="status">{current.error} O diagnóstico local continua disponível.</p>}
      {current?.phase === "cancelled" && <p className="doctor-gemini-status" role="status">Consulta cancelada. O diagnóstico local continua disponível.</p>}
      {current?.advice && <div className="doctor-gemini-result">
        <p className="doctor-gemini-disclaimer">Sugestão do Gemini · confira as evidências locais</p>
        <div><h3>O que aconteceu</h3><p>{current.advice.summary}</p></div>
        <div><h3>{current.advice.certainty === "unknown" ? "Causa não identificada" : "Possível causa"}</h3><p>{current.advice.cause}</p></div>
        <div><h3>Próximos passos</h3><ol>{current.advice.actions.map((action) => <li key={action}>{DOCTOR_ACTIONS[action]}</li>)}</ol></div>
        {current.advice.evidenceIds.length > 0 && <small>Baseado em: {current.advice.evidenceIds.map((id) => DOCTOR_ISSUE_LABELS[id]).join(" · ")}.</small>}
        {current.historyWarning && <small>Não foi possível guardar esta explicação no histórico local.</small>}
      </div>}
    </section>
  );
}
