import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { DoctorView } from "../doctor/DoctorView";
import type { DoctorRecoveryOverview } from "../doctor/DoctorRecovery";
import type { DoctorExplanation, DoctorRun } from "../doctor/types";
import { RecoveryReviewDialog } from "../recovery/RecoveryView";
import { recoveryBytes, recoveryDate, type RecoveryPreview, type RecoveryResult } from "../recovery/types";
import { labError, type LabDetails, type LabEnvironment, type LabProgress, type LabSetting } from "./types";
import "./diagnostic-lab.css";

type Props = {
  instanceId: string; busy: boolean; runningIds: string[]; javaExecutable: string;
  onOperationChange: (active: boolean) => void; onChanged: () => Promise<void>;
  onLaunch: (id: string) => Promise<void>; onRecovery: (id: string, pointId?: string) => void;
  onExplain?: DoctorExplanation;
};
type Operation = "list" | "create" | "details" | "mods" | "setting" | "preview" | "apply" | "delete" | "launch";
const operationText: Record<Operation, string> = { list: "Verificando cópias e operações interrompidas…", create: "Copiando e verificando os arquivos…", details: "Lendo o estado da cópia…", mods: "Alterando os mods selecionados na cópia…", setting: "Salvando a configuração na cópia…", preview: "Comparando mudanças selecionadas com a origem…", apply: "Protegendo a origem e aplicando as mudanças…", delete: "Excluindo a cópia de teste…", launch: "Preparando a execução da cópia…" };
const outcomes: Record<DoctorRun["outcome"], string> = { preparing: "Preparando", running: "Em execução", success: "Encerramento normal observado", failed: "Falha observada", cancelled: "Cancelada", unknown: "Resultado não confirmado" };

export function DiagnosticLabView({ instanceId, busy, runningIds, javaExecutable, onOperationChange, onChanged, onLaunch, onRecovery, onExplain }: Props) {
  const [environments, setEnvironments] = useState<LabEnvironment[]>();
  const [selectedId, setSelectedId] = useState("");
  const [details, setDetails] = useState<LabDetails>();
  const [operation, setOperation] = useState<Operation>();
  const [progress, setProgress] = useState<LabProgress>();
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [warning, setWarning] = useState("");
  const [creating, setCreating] = useState(false);
  const [worlds, setWorlds] = useState<string[]>();
  const [copyWorld, setCopyWorld] = useState(false);
  const [selectedWorld, setSelectedWorld] = useState("");
  const [selectedMods, setSelectedMods] = useState<string[]>([]);
  const [selectedChanges, setSelectedChanges] = useState<string[]>([]);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [preview, setPreview] = useState<RecoveryPreview>();
  const [deleting, setDeleting] = useState<LabEnvironment>();
  const [doctorOpen, setDoctorOpen] = useState(false);
  const [runId, setRunId] = useState<string>();
  const [latestRun, setLatestRun] = useState<DoctorRun | null>();
  const [undoOrigin, setUndoOrigin] = useState<string>();
  const [selectedAttempt, setSelectedAttempt] = useState<string>();
  const [history, setHistory] = useState<{ context: string; value?: DoctorRecoveryOverview }>();
  const context = useRef(instanceId);
  const selectedContext = useRef(selectedId);
  context.current = instanceId; selectedContext.current = selectedId;
  const generation = useRef(0);
  const previewEpoch = useRef(0);
  const inFlight = useRef(false);
  const callbacks = useRef({ onOperationChange, onChanged, onLaunch });
  callbacks.current = { onOperationChange, onChanged, onLaunch };
  const disabled = busy || !!operation;
  const selected = environments?.find((environment) => environment.id === selectedId);
  const currentDetails = details?.environment.id === selectedId ? details : undefined;
  const selectedRunning = runningIds.includes(selectedId);
  const mutationDisabled = disabled || selectedRunning || selected?.state !== "ready";
  const sourceIsCopy = environments?.some((environment) => environment.id === instanceId) ?? false;
  const historyContext = JSON.stringify([selectedId, selectedAttempt ?? null, latestRun?.runId, latestRun?.outcome]);
  const currentHistory = history?.context === historyContext ? history.value : undefined;

  function invalidatePreview() { previewEpoch.current += 1; setPreview(undefined); }
  function acceptDetails(value: LabDetails) {
    setDetails(value); setDrafts({}); setSelectedMods([]); setSelectedChanges([]); invalidatePreview();
    setEnvironments((items) => items?.map((item) => item.id === value.environment.id ? value.environment : item));
  }
  async function perform(kind: Operation, action: (current: () => boolean) => Promise<void>) {
    if (inFlight.current) return;
    const request = ++generation.current;
    const origin = instanceId;
    const current = () => generation.current === request && context.current === origin;
    inFlight.current = true; setOperation(kind); setError(""); setMessage(""); setWarning(""); setProgress(undefined);
    callbacks.current.onOperationChange(true);
    try { await action(current); }
    catch (cause) { if (current()) { setError(labError(cause)); invalidatePreview(); } }
    finally { if (current()) { inFlight.current = false; setOperation(undefined); callbacks.current.onOperationChange(false); } }
  }
  async function loadList(current: () => boolean) {
    const value = await invoke<LabEnvironment[]>("diagnostic_lab_list", { id: instanceId });
    if (current()) setEnvironments(value);
  }
  async function loadDetails(id: string, current: () => boolean) {
    const [value, run] = await Promise.all([invoke<LabDetails>("diagnostic_lab_details", { id }), invoke<DoctorRun | null>("read_latest_doctor_run", { id }).catch(() => null)]);
    if (current() && selectedContext.current === id) { acceptDetails(value); setLatestRun(run); }
  }
  useEffect(() => {
    setEnvironments(undefined); setSelectedId(""); selectedContext.current = ""; setDetails(undefined);
    setCreating(false); setCopyWorld(false); setSelectedWorld(""); setWorlds(undefined); setDoctorOpen(false);
    setPreview(undefined); setDeleting(undefined); setLatestRun(undefined); setUndoOrigin(undefined);
    inFlight.current = false;
    void perform("list", loadList);
    return () => { generation.current += 1; inFlight.current = false; callbacks.current.onOperationChange(false); };
  }, [instanceId]);
  useEffect(() => {
    let disposed = false; const stops: Array<() => void> = [];
    const subscribe = <T,>(name: string, callback: (payload: T) => void) => {
      void listen<T>(name, ({ payload }) => { if (!disposed) callback(payload); }).then((stop) => { if (disposed) stop(); else stops.push(stop); }).catch(() => undefined);
    };
    subscribe<LabProgress>("aurora-diagnostic-lab-progress", (payload) => {
      if (payload.instanceId === instanceId || payload.instanceId === selectedContext.current) setProgress(payload);
    });
    subscribe<DoctorRun>("aurora-doctor-run", (run) => {
      if (run.instanceId === selectedContext.current) { setLatestRun(run); invalidatePreview(); }
    });
    for (const event of ["aurora-recovery-changed", "aurora-ai-tool-changed", "aurora-diagnostic-lab-changed"]) {
      subscribe<{ instanceId: string }>(event, (payload) => {
        if (payload.instanceId === instanceId || payload.instanceId === selectedContext.current) invalidatePreview();
      });
    }
    return () => { disposed = true; stops.forEach((stop) => stop()); };
  }, [instanceId]);
  useEffect(() => { if (busy || selectedRunning) invalidatePreview(); }, [busy, selectedRunning]);
  useEffect(() => {
    if (!selectedId) return;
    let disposed = false;
    void invoke<DoctorRecoveryOverview>("doctor_recovery_overview", { id: selectedId, runId: selectedAttempt ?? latestRun?.runId ?? null })
      .then((value) => { if (!disposed) setHistory({ context: historyContext, value }); })
      .catch(() => { if (!disposed) setHistory({ context: historyContext }); });
    return () => { disposed = true; };
  }, [historyContext, selectedId, selectedAttempt, latestRun?.runId]);

  function chooseEnvironment(id: string) {
    if (disabled) return;
    setSelectedId(id); selectedContext.current = id; setDetails(undefined); setDoctorOpen(false); setRunId(undefined);
    setSelectedChanges([]); setSelectedMods([]); setLatestRun(undefined); setSelectedAttempt(undefined); setHistory(undefined); invalidatePreview();
    void perform("details", (current) => loadDetails(id, current));
  }
  function beginCreate() {
    if (disabled || runningIds.includes(instanceId) || sourceIsCopy) return;
    setCreating(true); setCopyWorld(false); setSelectedWorld(""); setWorlds(undefined);
    void perform("details", async (current) => {
      const available = await invoke<string[]>("diagnostic_lab_worlds", { id: instanceId });
      if (current()) setWorlds(available);
    });
  }
  function createEnvironment() {
    if (disabled || runningIds.includes(instanceId) || sourceIsCopy || (copyWorld && !selectedWorld)) return;
    void perform("create", async (current) => {
      const environment = await invoke<LabEnvironment>("diagnostic_lab_create", { id: instanceId, selectedWorld: copyWorld ? selectedWorld : null });
      if (!current()) return;
      setCreating(false); setSelectedId(environment.id); selectedContext.current = environment.id;
      setEnvironments((items) => [...(items ?? []).filter((item) => item.id !== environment.id), environment]);
      try { await loadList(current); await loadDetails(environment.id, current); }
      catch { if (current()) setWarning("A cópia foi criada, mas os dados da tela não puderam ser atualizados. Use Atualizar ambientes para conferir o estado."); }
      if (current()) { setMessage("Cópia criada e verificada. Use a autenticação normal do launcher para iniciar."); await callbacks.current.onChanged().catch(() => undefined); }
    });
  }
  function toggleMods(enabled: boolean) {
    if (mutationDisabled || !selectedMods.length) return;
    const id = selectedId;
    invalidatePreview();
    void perform("mods", async (current) => {
      try { for (const path of selectedMods) await invoke("diagnostic_lab_toggle_mod", { id, path, enabled }); }
      finally { await loadDetails(id, current); }
      if (current()) setMessage(`Mods selecionados ${enabled ? "ativados" : "desativados"} somente na cópia.`);
    });
  }
  function saveSetting(setting: LabSetting) {
    if (mutationDisabled) return;
    const key = `${setting.path}#${setting.pointer}`;
    const draft = drafts[key] ?? String(setting.value);
    const value = setting.kind === "boolean" ? draft === "true" : Number(draft);
    if (typeof value === "number" && (!Number.isFinite(value) || draft.trim() === "" || (setting.min != null && value < setting.min) || (setting.max != null && value > setting.max))) {
      setError("Informe um valor dentro dos limites exibidos para esta configuração."); return;
    }
    const id = selectedId; invalidatePreview();
    void perform("setting", async (current) => {
      await invoke("diagnostic_lab_set_setting", { id, path: setting.path, pointer: setting.pointer, value });
      await loadDetails(id, current);
      if (current()) setMessage("Configuração salva somente na cópia de teste.");
    });
  }
  function reviewChanges() {
    if (mutationDisabled || !selectedChanges.length) return;
    const epoch = previewEpoch.current; const id = selectedId;
    void perform("preview", async (current) => {
      const value = await invoke<RecoveryPreview>("diagnostic_lab_preview_apply", { id, selected: selectedChanges });
      if (current() && selectedContext.current === id && previewEpoch.current === epoch) setPreview(value);
    });
  }
  function applyChanges() {
    if (!preview || mutationDisabled) return;
    const reviewed = preview; const id = selectedId;
    void perform("apply", async (current) => {
      const result = await invoke<RecoveryResult & { originInstanceId: string }>("diagnostic_lab_apply", { id, previewId: reviewed.id });
      if (!current()) return;
      invalidatePreview(); setSelectedChanges([]); setUndoOrigin(result.undoAvailable ? result.originInstanceId : undefined);
      setMessage("Mudanças selecionadas aplicadas à original com proteção do estado anterior. Mundos foram preservados.");
      await callbacks.current.onChanged().catch(() => undefined);
    });
  }
  function deleteEnvironment() {
    if (!deleting || disabled || runningIds.includes(deleting.id)) return;
    const id = deleting.id;
    void perform("delete", async (current) => {
      await invoke("diagnostic_lab_delete", { id });
      if (!current()) return;
      setDeleting(undefined);
      if (selectedContext.current === id) { selectedContext.current = ""; setSelectedId(""); setDetails(undefined); setDoctorOpen(false); invalidatePreview(); }
      try { await loadList(current); }
      catch { if (current()) setWarning("A cópia foi excluída, mas a lista não pôde ser atualizada. Use Atualizar ambientes para conferir o estado."); }
      await callbacks.current.onChanged().catch(() => undefined);
      if (current()) setMessage("Cópia excluída. A instância original e o conteúdo compartilhado foram preservados.");
    });
  }

  return <section aria-label="Ambiente de diagnóstico isolado" aria-busy={!!operation} className="diagnostic-lab">
    <header className="recovery-heading"><div><p className="eyebrow">AMBIENTE DE TESTE</p><h2>Teste sem alterar a original</h2><p>Cópia temporária com identidade, arquivos de jogo, logs e histórico próprios. Ela usa sua autenticação normal; não é um isolamento de segurança do sistema operacional.</p></div><button disabled={disabled || runningIds.includes(instanceId) || creating || sourceIsCopy} onClick={beginCreate} type="button">Criar cópia de teste</button></header>
    {sourceIsCopy && <p>Esta instância já é uma cópia de teste. Abra a instância original para criar outra cópia.</p>}
    <div className="recovery-worlds-warning"><strong>Mundos ficam de fora por padrão.</strong><span>Você pode criar mundos descartáveis dentro da cópia. Um mundo existente só será copiado se você selecioná-lo explicitamente.</span></div>
    {runningIds.includes(instanceId) && <p role="status">Encerre o Minecraft da origem antes de criar uma cópia consistente.</p>}
    {creating && <form className="lab-create" onSubmit={(event) => { event.preventDefault(); createEnvironment(); }}><h3>Preparar cópia independente</h3><label className="lab-check"><input checked={copyWorld} disabled={disabled} onChange={(event) => { setCopyWorld(event.target.checked); setSelectedWorld(""); }} type="checkbox" />Copiar explicitamente um mundo existente</label>{copyWorld && <label>Mundo a copiar<select aria-label="Mundo a copiar" disabled={disabled} value={selectedWorld} onChange={(event) => setSelectedWorld(event.target.value)}><option value="">Escolha um mundo</option>{worlds?.map((world) => <option value={world} key={world}>{world}</option>)}</select>{worlds?.length === 0 && <small>Nenhum mundo disponível para cópia.</small>}</label>}<p>{copyWorld ? "Será criada uma cópia independente desse mundo. Alterações e progressos no teste nunca serão transferidos à original." : "Nenhum mundo será copiado. Crie um mundo descartável ao entrar no Minecraft de teste."} Mantenha o jogo fechado e evite edições externas durante a cópia.</p><div className="lab-actions"><button className="secondary" disabled={disabled} onClick={() => setCreating(false)} type="button">Cancelar</button><button disabled={disabled || (copyWorld && !selectedWorld)} type="submit">Criar e verificar cópia</button></div></form>}
    {operation && <div className="recovery-progress" role="status"><span className="recovery-spinner" aria-hidden="true" /><div><strong>{operationText[operation]}</strong>{progress && progress.totalBytes > 0 ? <><progress max={progress.totalBytes} value={Math.min(progress.completedBytes, progress.totalBytes)} /><p>{recoveryBytes(progress.completedBytes)} de {recoveryBytes(progress.totalBytes)}</p></> : <p>Aguarde a verificação; o estado anterior é preservado em caso de falha.</p>}</div></div>}
    {error && <p className="recovery-message recovery-error" role="alert">{error}</p>}
    {message && <p className="recovery-message recovery-success" role="status">{message}</p>}
    {warning && <p className="recovery-message recovery-warning" role="status">{warning}</p>}
    {undoOrigin && <button className="secondary" disabled={disabled} onClick={() => onRecovery(undoOrigin)} type="button">Abrir Recuperação da original para desfazer</button>}
    <div className="recovery-list-heading"><h3>Cópias desta instância</h3><button className="secondary compact" disabled={disabled} onClick={() => { invalidatePreview(); void perform("list", async (current) => { await loadList(current); if (selectedId) await loadDetails(selectedId, current); }); }} type="button">Atualizar ambientes</button></div>
    {environments?.length === 0 && <div className="recovery-empty"><strong>Nenhuma cópia de teste</strong><p>Crie uma cópia para testar mods e configurações com o jogo fechado.</p></div>}
    <div className="lab-environments">{environments?.map((environment) => <article className={`recovery-point${environment.id === selectedId ? " selected" : ""}`} key={environment.instanceUuid}><div><h3>Cópia de teste · {recoveryDate(environment.createdAt)}</h3><p>Origem na criação: {environment.originInstanceId}</p><small>{recoveryBytes(environment.logicalBytes)} · {environment.copiedWorld ? `Mundo copiado: ${environment.copiedWorld}` : "Sem mundos copiados"} · {runningIds.includes(environment.id) ? "Em execução" : environment.state === "ready" ? "Disponível" : "Operação incompleta — verifique ou exclua a cópia"}</small><details><summary>Identidade e limites</summary><p>Identidade própria: <code>{environment.instanceUuid}</code></p><p>Origem verificada por identidade: <code>{environment.originUuid}</code></p>{environment.limitations.map((limitation, index) => <p key={index}>{limitation}</p>)}</details></div><div className="lab-actions"><button className="secondary compact" disabled={disabled || environment.state !== "ready"} onClick={() => chooseEnvironment(environment.id)} type="button">{environment.id === selectedId ? "Atualizar cópia" : "Abrir cópia"}</button><button className="danger compact" disabled={disabled || runningIds.includes(environment.id)} onClick={() => setDeleting(environment)} type="button">Excluir cópia</button></div></article>)}</div>
    {currentDetails && <div className="lab-details">
      <header><div><h3>Alterações somente na cópia selecionada</h3><small>Os controles abaixo nunca editam a instância original.</small></div><div className="lab-actions"><button disabled={mutationDisabled} onClick={() => void perform("launch", async (current) => { await callbacks.current.onLaunch(selectedId); const run = await invoke<DoctorRun | null>("read_latest_doctor_run", { id: selectedId }).catch(() => null); if (current()) setLatestRun(run); })} type="button">Iniciar cópia de teste</button><button className="secondary" disabled={disabled} onClick={() => { setRunId(latestRun?.runId); setDoctorOpen(true); }} type="button">Analisar tentativa com Doctor</button></div></header>
      {selectedRunning && <p role="status">A cópia está em execução. Encerre o Minecraft antes de editar, aplicar ou excluir.</p>}
      <details open><summary>Mods ({currentDetails.mods.length})</summary><div className="lab-actions"><button className="secondary compact" disabled={mutationDisabled || !selectedMods.length} onClick={() => toggleMods(true)} type="button">Ativar mods selecionados</button><button className="secondary compact" disabled={mutationDisabled || !selectedMods.length} onClick={() => toggleMods(false)} type="button">Desativar mods selecionados</button></div><div className="lab-mods">{currentDetails.mods.map((mod) => <label className="lab-mod" key={mod.path}><input type="checkbox" aria-label={`Selecionar mod ${mod.path}`} disabled={mutationDisabled} checked={selectedMods.includes(mod.path)} onChange={(event) => setSelectedMods((current) => event.target.checked ? [...current, mod.path] : current.filter((path) => path !== mod.path))} /><code>{mod.path}</code><span>{mod.enabled ? "Ativo" : "Desativado"}</span></label>)}</div>{!currentDetails.mods.length && <p>Nenhum mod compatível com estes controles.</p>}</details>
      <details><summary>Configurações suportadas ({currentDetails.settings.length})</summary><p>Somente os campos já suportados pelo launcher podem ser alterados e transferidos.</p>{currentDetails.settings.map((setting) => { const key = `${setting.path}#${setting.pointer}`; const value = drafts[key] ?? String(setting.value); return <div className="lab-setting" key={key}><label>{setting.label}<small>{setting.path} · {setting.pointer}</small>{setting.kind === "boolean" ? <select aria-label={setting.label} disabled={mutationDisabled} value={value} onChange={(event) => setDrafts((current) => ({ ...current, [key]: event.target.value }))}><option value="true">Ativado</option><option value="false">Desativado</option></select> : <input aria-label={setting.label} disabled={mutationDisabled} type="number" min={setting.min ?? undefined} max={setting.max ?? undefined} value={value} onChange={(event) => setDrafts((current) => ({ ...current, [key]: event.target.value }))} />}{setting.kind === "number" && <small>Limites: {setting.min ?? "—"} a {setting.max ?? "—"}</small>}</label><button className="secondary compact" disabled={mutationDisabled || value === String(setting.value)} onClick={() => saveSetting(setting)} type="button">Salvar na cópia</button></div>; })}</details>
      <section className="lab-observations"><h3>Tentativas e observações</h3>{latestRun ? <><p>{outcomes[latestRun.outcome]}</p><small>Execução: <code>{latestRun.runId}</code></small></> : <p>Nenhuma tentativa registrada nesta cópia.</p>}<p>Uma tentativa sem o erro anterior é uma observação. Ela não garante que a causa foi encontrada nem que o problema foi resolvido.</p>
        {!!currentHistory?.attempts?.length && <label>Histórico de tentativas<select aria-label="Histórico de tentativas" value={selectedAttempt ?? currentHistory.runId ?? ""} onChange={(event) => setSelectedAttempt(event.target.value || undefined)}>{currentHistory.attempts.map((attempt) => <option key={attempt.runId} value={attempt.runId}>{recoveryDate(attempt.observedAtMs)} · {outcomes[attempt.outcome as DoctorRun["outcome"]] ?? "Resultado não confirmado"}</option>)}</select></label>}
        {currentHistory && <details><summary>Mudanças registradas na tentativa selecionada</summary>{currentHistory.changes.length ? <ul>{currentHistory.changes.map((change, index) => <li key={`${change.path}:${index}`}><code>{change.path}{change.field ? ` · ${change.field}` : ""}</code> · {change.action === "added" ? "Adicionado" : change.action === "removed" ? "Removido" : "Alterado"}</li>)}</ul> : <p>Nenhuma diferença verificável no histórico disponível para esta tentativa.</p>}{currentHistory.limitations.map((limitation, index) => <p key={index}>{limitation}</p>)}</details>}
        {history?.context === historyContext && !currentHistory && <p>Histórico das tentativas indisponível. A última observação acima permanece limitada ao registro da execução.</p>}
      </section>
      <section className="lab-changes"><h3>Alterações testadas e disponíveis para aplicar</h3><p>Selecione somente o que deseja transferir. Mundos, logs, caches, credenciais e identidade ficam fora da transferência.</p>{currentDetails.changes.length ? currentDetails.changes.map((change) => <label className="lab-change" key={change.id}><input aria-label={`Aplicar mudança ${change.path}`} type="checkbox" checked={selectedChanges.includes(change.id)} disabled={mutationDisabled} onChange={(event) => { invalidatePreview(); setSelectedChanges((current) => event.target.checked ? [...current, change.id] : current.filter((id) => id !== change.id)); }} /><span><code>{change.path}</code><small>{change.before ?? "Não definido"} → {change.after ?? "Não definido"}</small></span></label>) : <p>Nenhuma mudança suportada em relação ao estado usado na criação.</p>}<button disabled={mutationDisabled || !selectedChanges.length} onClick={reviewChanges} type="button">Revisar alterações para aplicar à instância original</button></section>
      {doctorOpen && <DoctorView key={`${selectedId}:${runId ?? "current"}`} instanceId={selectedId} javaExecutable={javaExecutable} runId={runId} busy={disabled} onSelectRun={setRunId} onExplain={onExplain} onRecovery={(pointId) => onRecovery(selectedId, pointId)} />}
    </div>}
    {preview && <RecoveryReviewDialog key={preview.id} review={{ kind: "apply", preview }} blocked={busy || selectedRunning} busy={operation === "apply"} onCancel={() => setPreview(undefined)} onConfirm={applyChanges} />}
    {deleting && <div className="recovery-review-overlay"><section role="alertdialog" aria-modal="true" aria-labelledby="lab-delete-title" className="recovery-review"><header><h2 id="lab-delete-title">Excluir cópia de teste?</h2></header><div className="recovery-review-body"><p>A cópia criada em {recoveryDate(deleting.createdAt)}, seus mundos descartáveis, alterações e logs serão excluídos. A instância original permanece intacta.</p>{runningIds.includes(deleting.id) && <p>Encerre o Minecraft desta cópia antes de excluir.</p>}</div><footer><div><button autoFocus className="secondary" disabled={!!operation} onClick={() => setDeleting(undefined)} type="button">Cancelar</button><button className="danger" disabled={disabled || runningIds.includes(deleting.id)} onClick={deleteEnvironment} type="button">Confirmar exclusão da cópia</button></div></footer></section></div>}
  </section>;
}
