import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import type { AiToolContext, AiToolProposal } from "../../../../../shared/aiTools";
import { AiToolReviewDialog } from "./AiToolReviewDialog";
import { nativeToolError, type AiToolHistoryEntry, type AiToolPlan } from "./types";
import "./aiTools.css";

type Props = {
  instanceId: string;
  context?: AiToolContext;
  proposals?: AiToolProposal[];
  disabled?: boolean;
  onApplied?: () => void;
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

const EMPTY_PROPOSALS: AiToolProposal[] = [];

export function AiToolProposalList({ instanceId, context, proposals = EMPTY_PROPOSALS, disabled = false, onApplied }: Props) {
  const scope = `${instanceId}\0${context?.contextId ?? "history"}`;
  const [preview, setPreview] = useState<{ scope: string; proposals: AiToolProposal[]; plan: AiToolPlan }>();
  const [preparing, setPreparing] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [stale, setStale] = useState(false);
  const [history, setHistory] = useState<AiToolHistoryEntry[]>();
  const [historyLoading, setHistoryLoading] = useState(false);
  const historyOpen = useRef(false);
  const generation = useRef(0);
  const activePlan = useRef<AiToolPlan | undefined>(undefined);
  const confirmingRef = useRef(false);
  const preparingRef = useRef(false);
  const current = useRef({ scope, proposals });
  current.current = { scope, proposals };
  const plan = preview?.scope === scope && preview.proposals === proposals ? preview.plan : undefined;
  const revoke = (planId: string) => void invoke("revoke_ai_tool_plan", { planId }).catch(() => undefined);

  useEffect(() => {
    let disposed = false;
    const stops: Array<() => void> = [];
    generation.current += 1;
    setPreview(undefined); setPreparing(false); setConfirming(false); setError(""); setMessage(""); setStale(false); setHistory(undefined); setHistoryLoading(false);
    preparingRef.current = false;
    confirmingRef.current = false;
    const invalidate = (event: { payload: { instanceId: string } }) => {
      if (disposed || event.payload.instanceId !== instanceId) return;
      setStale(true);
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
  }, [scope, proposals, instanceId]);

  useEffect(() => {
    if (!plan) return;
    const timer = window.setTimeout(() => {
      if (confirmingRef.current) return;
      revoke(plan.planId);
      activePlan.current = undefined;
      setPreview(undefined);
      setError("A prévia expirou. Revise a sugestão novamente antes de confirmar.");
    }, Math.max(0, Math.min(2_147_483_647, plan.expiresAtMs - Date.now())));
    return () => window.clearTimeout(timer);
  }, [plan]);

  async function loadHistory() {
    const request = generation.current;
    setHistoryLoading(true);
    try {
      const entries = await invoke<AiToolHistoryEntry[]>("ai_tool_history", { id: instanceId });
      if (generation.current === request && current.current.scope === scope) setHistory(entries);
    } catch (cause) {
      if (generation.current === request) setError(nativeToolError(cause));
    } finally {
      if (generation.current === request) setHistoryLoading(false);
    }
  }

  async function prepare(proposal?: AiToolProposal, actionId?: string) {
    if (disabled || confirmingRef.current || preparingRef.current || (!context && !actionId) || (stale && !actionId)) return;
    const request = ++generation.current;
    preparingRef.current = true;
    setPreparing(true); setError(""); setMessage("");
    try {
      const next = await invoke<AiToolPlan>(actionId ? "prepare_ai_tool_undo" : "prepare_ai_tool", actionId
        ? { id: instanceId, actionId }
        : { id: instanceId, contextId: context?.contextId, proposal });
      if (generation.current !== request || current.current.scope !== scope || current.current.proposals !== proposals) { revoke(next.planId); return; }
      if (activePlan.current) revoke(activePlan.current.planId);
      activePlan.current = next;
      setPreview({ scope, proposals, plan: next });
    } catch (cause) {
      if (generation.current === request && current.current.scope === scope) setError(nativeToolError(cause));
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
      if (generation.current !== request || current.current.scope !== scope || current.current.proposals !== proposals) return;
      activePlan.current = undefined;
      setPreview(undefined);
      setStale(true);
      setMessage(entry.status === "undone" ? "Alteração desfeita. Faça uma nova análise para conferir."
        : entry.status === "applied" ? "Alteração aplicada. Faça uma nova análise para conferir o resultado."
          : "Operação pendente. Confira o histórico e revise a recuperação antes de tentar novamente.");
      await loadHistory();
      if (entry.status === "applied" || entry.status === "undone") onApplied?.();
    } catch (cause) {
      if (generation.current === request && current.current.scope === scope) setError(nativeToolError(cause));
    } finally {
      if (generation.current === request) { confirmingRef.current = false; setConfirming(false); }
    }
  }

  function targetLabel(proposal: AiToolProposal) {
    return proposal.kind === "remove_mod"
      ? context?.mods.find((mod) => mod.id === proposal.targetId)?.name ?? "Mod selecionado"
      : context?.settings.find((setting) => setting.id === proposal.targetId)?.label ?? "Configuração selecionada";
  }

  return <div className="ai-tools">
    <div className="ai-tool-proposals">{proposals.map((proposal, index) => <article className="ai-tool-card" key={`${proposal.kind}-${proposal.targetId}-${index}`}>
      <strong>{proposal.kind === "remove_mod" ? "Remover mod" : "Ajustar configuração"}: {targetLabel(proposal)}</strong>
      <p>{proposal.reason}</p>
      <button className="secondary compact" disabled={disabled || preparing || confirming || stale} onClick={() => void prepare(proposal)} type="button">Revisar sugestão</button>
      <p className="ai-tools-note">A prévia local mostrará o que muda antes de pedir sua confirmação.</p>
    </article>)}</div>
    {preparing && <p className="ai-tool-status" role="status">Preparando a prévia local…</p>}
    {stale && <p className="ai-tool-status">O estado da instância mudou. Solicite novas sugestões antes de revisar outra alteração.</p>}
    {message && <p className="ai-tool-status" role="status">{message}</p>}
    {error && !plan && <p className="ai-tool-error" role="alert">{error}</p>}
    <details className="ai-tool-history" onToggle={(event) => { historyOpen.current = event.currentTarget.open; if (event.currentTarget.open && !history && !historyLoading) void loadHistory(); }}>
      <summary>Histórico local de diagnósticos, sugestões e alterações</summary>
      <button className="text-button" disabled={historyLoading} onClick={() => void loadHistory()} type="button">Atualizar histórico</button>
      {historyLoading && <p className="ai-tool-status" role="status">Carregando histórico…</p>}
      {history && <div className="ai-tool-history-list">{history.length === 0 ? <p className="ai-tools-note">Nenhum registro disponível.</p> : history.map((entry) => <article key={entry.id}>
        <strong>{entry.title}</strong><small>{new Date(entry.createdAtMs).toLocaleString("pt-BR")}</small><p>{entry.summary}</p>
        <p className={entry.assessment === "unresolved" ? "ai-tool-unresolved" : ""}>{historyStatus(entry)}</p>
        {entry.undoAvailable && entry.actionId && <button className="secondary compact" disabled={disabled || preparing || confirming} onClick={() => void prepare(undefined, entry.actionId!)} type="button">Revisar desfazer</button>}
      </article>)}</div>}
    </details>
    {plan && <AiToolReviewDialog plan={plan} instanceId={instanceId} busy={confirming} blocked={disabled} error={error} onCancel={cancelPreview} onConfirm={() => void confirmPreview()} />}
  </div>;
}
