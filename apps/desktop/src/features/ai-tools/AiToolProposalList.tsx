import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { AiToolReviewDialog } from "./AiToolReviewDialog";
import { nativeToolError, type AiToolHistoryEntry, type AiToolPlan } from "./types";
import "./aiTools.css";

type Props = {
  instanceId: string;
  disabled?: boolean;
};

function historyStatus(entry: AiToolHistoryEntry) {
  if (entry.status === "undone") return "Alteração desfeita.";
  if (entry.status === "pending") return "Operação pendente; confira o estado antes de tentar novamente.";
  if (entry.assessment === "unresolved") return "A mesma categoria de erro reapareceu após a alteração. A correção ainda não foi confirmada.";
  if (entry.assessment === "normal_exit") return "A execução seguinte terminou normalmente.";
  if (entry.assessment === "inconclusive") return "Ainda não há dados suficientes para avaliar o resultado.";
  if (entry.status === "applied") return "Alteração aplicada; resultado ainda não avaliado.";
  return "Registrado no histórico local.";
}


export function InstanceHistory({ instanceId, disabled = false }: Props) {
  const [preview, setPreview] = useState<{ instanceId: string; plan: AiToolPlan }>();
  const [preparing, setPreparing] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [history, setHistory] = useState<AiToolHistoryEntry[]>();
  const [historyLoading, setHistoryLoading] = useState(false);
  const historyOpen = useRef(false);
  const generation = useRef(0);
  const activePlan = useRef<AiToolPlan | undefined>(undefined);
  const confirmingRef = useRef(false);
  const preparingRef = useRef(false);
  const current = useRef(instanceId);
  current.current = instanceId;
  const plan = preview?.instanceId === instanceId ? preview.plan : undefined;
  const revoke = (planId: string) => void invoke("revoke_ai_tool_plan", { planId }).catch(() => undefined);

  useEffect(() => {
    let disposed = false;
    const stops: Array<() => void> = [];
    generation.current += 1;
    setPreview(undefined); setPreparing(false); setConfirming(false); setError(""); setMessage(""); setHistory(undefined); setHistoryLoading(false);
    preparingRef.current = false;
    confirmingRef.current = false;
    const invalidate = (event: { payload: { instanceId: string } }) => {
      if (disposed || event.payload.instanceId !== instanceId) return;
      setHistory(undefined);
      if (!confirmingRef.current) {
        generation.current += 1;
        preparingRef.current = false;
        setPreparing(false);
        setHistoryLoading(false);
        if (activePlan.current) revoke(activePlan.current.planId);
        activePlan.current = undefined;
        setPreview(undefined);
      }
    };
    for (const name of ["aurora-doctor-run", "aurora-ai-tool-changed"]) {
      void listen<{ instanceId: string }>(name, invalidate).then((stop) => { if (disposed) stop(); else stops.push(stop); }).catch(() => undefined);
    }
    void listen<{ instanceId: string }>("aurora-ai-history-changed", (event) => {
      if (!disposed && event.payload.instanceId === instanceId) {
        setHistory(undefined);
        if (historyOpen.current) void loadHistory();
      }
    }).then((stop) => { if (disposed) stop(); else stops.push(stop); }).catch(() => undefined);
    return () => {
      disposed = true;
      generation.current += 1;
      for (const stop of stops) stop();
      if (activePlan.current && !confirmingRef.current) revoke(activePlan.current.planId);
      activePlan.current = undefined;
    };
  }, [instanceId]);

  useEffect(() => {
    if (!plan) return;
    const timer = window.setTimeout(() => {
      if (confirmingRef.current) return;
      revoke(plan.planId);
      activePlan.current = undefined;
      setPreview(undefined);
      setError("A prévia expirou. Revise a restauração antes de confirmar.");
    }, Math.max(0, Math.min(2_147_483_647, plan.expiresAtMs - Date.now())));
    return () => window.clearTimeout(timer);
  }, [plan]);

  async function loadHistory() {
    const request = generation.current;
    setHistoryLoading(true);
    try {
      const entries = await invoke<AiToolHistoryEntry[]>("ai_tool_history", { id: instanceId });
      if (generation.current === request && current.current === instanceId) setHistory(entries);
    } catch (cause) {
      if (generation.current === request) setError(nativeToolError(cause));
    } finally {
      if (generation.current === request) setHistoryLoading(false);
    }
  }

  async function prepareUndo(actionId: string) {
    if (disabled || confirmingRef.current || preparingRef.current) return;
    const request = ++generation.current;
    preparingRef.current = true;
    setPreparing(true); setError(""); setMessage("");
    try {
      const next = await invoke<AiToolPlan>("prepare_ai_tool_undo", { id: instanceId, actionId });
      if (generation.current !== request || current.current !== instanceId) { revoke(next.planId); return; }
      if (activePlan.current) revoke(activePlan.current.planId);
      activePlan.current = next;
      setPreview({ instanceId, plan: next });
    } catch (cause) {
      if (generation.current === request && current.current === instanceId) setError(nativeToolError(cause));
    } finally {
      if (generation.current === request) { preparingRef.current = false; setPreparing(false); }
    }
  }

  function cancelPreview() {
    if (confirmingRef.current) return;
    if (activePlan.current) revoke(activePlan.current.planId);
    activePlan.current = undefined;
    setPreview(undefined);
    setMessage("Prévia cancelada. Nenhuma alteração foi aplicada.");
  }

  async function confirmPreview() {
    if (!plan || disabled || confirmingRef.current || activePlan.current?.planId !== plan.planId) return;
    if (Date.now() >= plan.expiresAtMs) { cancelPreview(); setError("A prévia expirou. Revise novamente."); return; }
    const request = generation.current;
    confirmingRef.current = true;
    setConfirming(true); setError("");
    try {
      const entry = await invoke<AiToolHistoryEntry>("confirm_ai_tool", { planId: plan.planId, digest: plan.digest });
      if (generation.current !== request || current.current !== instanceId) return;
      activePlan.current = undefined;
      setPreview(undefined);
      setMessage(entry.status === "undone" ? "Alteração desfeita. Faça uma nova análise para conferir."
        : entry.status === "applied" ? "Alteração aplicada. Faça uma nova análise para conferir o resultado."
          : "Operação pendente. Confira o histórico e revise a recuperação antes de tentar novamente.");
      await loadHistory();
    } catch (cause) {
      if (generation.current === request && current.current === instanceId) setError(nativeToolError(cause));
    } finally {
      if (generation.current === request) { confirmingRef.current = false; setConfirming(false); }
    }
  }

  return <div className="ai-tools">
    {preparing && <p className="ai-tool-status" role="status">Preparando a prévia local…</p>}
    {message && <p className="ai-tool-status" role="status">{message}</p>}
    {error && !plan && <p className="ai-tool-error" role="alert">{error}</p>}
    <details className="ai-tool-history" onToggle={(event) => { historyOpen.current = event.currentTarget.open; if (event.currentTarget.open && !history && !historyLoading) void loadHistory(); }}>
      <summary>Histórico local e restauração</summary>
      <button className="text-button" disabled={historyLoading} onClick={() => void loadHistory()} type="button">Atualizar histórico</button>
      {historyLoading && <p className="ai-tool-status" role="status">Carregando histórico…</p>}
      {history && <div className="ai-tool-history-list">{history.length === 0 ? <p className="ai-tools-note">Nenhum registro disponível.</p> : history.map((entry) => <article key={entry.id}>
        <strong>{entry.title}</strong><small>{new Date(entry.createdAtMs).toLocaleString("pt-BR")}</small><p>{entry.summary}</p>
        <p className={entry.assessment === "unresolved" ? "ai-tool-unresolved" : ""}>{historyStatus(entry)}</p>
        {entry.undoAvailable && entry.actionId && <button className="secondary compact" disabled={disabled || preparing || confirming} onClick={() => void prepareUndo(entry.actionId!)} type="button">Revisar desfazer</button>}
      </article>)}</div>}
    </details>
    {plan && <AiToolReviewDialog plan={plan} instanceId={instanceId} busy={confirming} blocked={disabled} error={error} onCancel={cancelPreview} onConfirm={() => void confirmPreview()} />}
  </div>;
}
