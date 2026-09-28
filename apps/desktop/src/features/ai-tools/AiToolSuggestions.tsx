import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import type { AiToolAdvice, AiToolContext } from "../../../../../shared/aiTools";
import { publicAiError } from "../../../../../shared/aiErrors";
import type { DoctorReport } from "../doctor/types";
import { AiToolProposalList } from "./AiToolProposalList";
import { nativeToolError, type AiToolAdviceRequest } from "./types";
import "./aiTools.css";

type Props = { instanceId: string; report: DoctorReport; requestAdvice: AiToolAdviceRequest; busy: boolean };
type Result = { instanceId: string; report: DoctorReport; requestAdvice: AiToolAdviceRequest; context: AiToolContext; advice: AiToolAdvice };

export function AiToolSuggestions({ instanceId, report, requestAdvice, busy }: Props) {
  const [result, setResult] = useState<Result>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const generation = useRef(0);
  const pending = useRef<AbortController | undefined>(undefined);
  const inputs = useRef({ instanceId, report, requestAdvice });
  inputs.current = { instanceId, report, requestAdvice };
  const current = result?.instanceId === instanceId && result.report === report && result.requestAdvice === requestAdvice ? result : undefined;
  useEffect(() => {
    setResult(undefined); setLoading(false); setError(""); setMessage("");
    return () => { generation.current += 1; pending.current?.abort(); pending.current = undefined; };
  }, [instanceId, report, requestAdvice]);

  function cancel() {
    generation.current += 1;
    pending.current?.abort(); pending.current = undefined;
    setLoading(false);
    setMessage("Consulta cancelada. Nenhuma alteração foi aplicada.");
  }

  async function suggest() {
    if (busy || pending.current) return;
    const request = ++generation.current;
    const controller = new AbortController();
    pending.current = controller;
    setLoading(true); setResult(undefined); setError(""); setMessage("");
    const stillCurrent = () => generation.current === request && !controller.signal.aborted
      && inputs.current.instanceId === instanceId && inputs.current.report === report && inputs.current.requestAdvice === requestAdvice;
    let stage: "native" | "provider" = "native";
    try {
      const context = await invoke<AiToolContext>("ai_tools_context", { id: instanceId });
      if (!stillCurrent()) return;
      stage = "provider";
      const advice = await requestAdvice(context, "Sugira apenas alterações necessárias para os problemas observados. Considere o histórico: uma categoria recorrente não prova a mesma causa; evite repetir tentativas sem nova evidência.", controller.signal, report);
      if (!stillCurrent()) return;
      stage = "native";
      try { await invoke("ai_record_advice", { id: instanceId, contextId: context.contextId, advice: { summary: advice.summary, proposals: advice.proposals } }); }
      catch { if (stillCurrent()) setMessage("Sugestão recebida; não foi possível salvá-la no histórico local."); }
      if (stillCurrent()) setResult({ instanceId, report, requestAdvice, context, advice });
    } catch (cause) {
      if (stillCurrent()) setError(stage === "provider" ? publicAiError(cause) : nativeToolError(cause));
    } finally {
      if (pending.current === controller) { pending.current = undefined; setLoading(false); }
    }
  }

  return <section className="ai-tools" aria-label="Sugestões de correção com confirmação" aria-busy={loading}>
    <div className="ai-tools-heading"><div><strong>Sugestões para esta instância</strong><p>Envia ao Gemini a lista de mods, configurações permitidas e o histórico resumido. Os arquivos e logs não são enviados. Toda alteração exige prévia e confirmação.</p></div>
      <div className="ai-tools-buttons"><button className="secondary compact" disabled={busy || loading} onClick={() => void suggest()} type="button">{loading ? "Consultando…" : "Sugerir correção"}</button>{loading && <button className="secondary compact" onClick={cancel} type="button">Cancelar consulta</button>}</div>
    </div>
    {error && <p className="ai-tool-error" role="alert">{error}</p>}
    {message && <p className="ai-tool-status" role="status">{message}</p>}
    {current && <><p className="ai-tools-summary">{current.advice.summary}</p><small className="ai-tools-note">Sugestão do Gemini · {current.advice.model}</small><AiToolProposalList key={current.context.contextId} instanceId={instanceId} context={current.context} proposals={current.advice.proposals} disabled={busy || loading} /></>}
    {!current && <AiToolProposalList instanceId={instanceId} disabled={busy || loading} />}
  </section>;
}
