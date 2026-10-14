import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createContext, type ReactNode, useCallback, useContext, useEffect, useRef, useState } from "react";
import { MODS, uuid, voiceCapabilitiesFrom, voiceDate, voiceError, voiceReceiptListFrom, voiceSnapshotFrom, type VoiceCapabilities, type VoiceReceiptList, type VoiceSnapshot } from "./types";

type Command = "inspect" | "prepare_apply" | "prepare_apply_durable" | "resume_receipt" | "confirm" | "prepare_restore" | "prepare_restore_reconciled" | "cancel_review" | "begin_close" | "cancel_close" | "confirm_close" | "finish_close";
type Navigate = (instanceId: string, target: "voice" | "recovery") => void;
type PendingLogoutTransition = {
  kind: "confirm" | "cancel"; closeId: string; operationId: string | null;
  instanceId: string | null; voiceMod: VoiceSnapshot["voiceMod"]; generation: number;
  navigateAfterCancel?: "voice" | "recovery";
};
export type VoiceAnnouncementController = {
  capabilities: VoiceCapabilities | null; snapshot: VoiceSnapshot | null; error: string;
  pending: boolean; blocking: boolean; valid: boolean;
  isBlocking: () => boolean;
  run: (command: Command, request: Record<string, unknown>) => Promise<VoiceSnapshot>;
  refresh: () => Promise<VoiceSnapshot>;
  listReceipts: (instanceId: string) => Promise<VoiceReceiptList>;
  beginPanelClose: () => Promise<void>;
  prepareLogout: () => Promise<string | null | false>;
  finishLogout: (closeId: string | null) => Promise<void>;
  registerNavigator: (navigate: Navigate) => () => void;
  navigate: Navigate;
};
const Context = createContext<VoiceAnnouncementController | null>(null);
export const useVoiceAnnouncement = () => useContext(Context);

/** The caller supplies its existing Connect cleanup and authentication action.
 * Its completion (including failure) must precede releasing the native barrier. */
export async function withVoiceAnnouncementLogout(voice: VoiceAnnouncementController | null, action: () => Promise<void>) {
  const closeId = await voice?.prepareLogout();
  if (closeId === false) return false;
  try { await action(); return true; }
  finally { if (closeId) await voice?.finishLogout(closeId); }
}

/** Always mounted above authentication/editor views. The native manager owns
 * work and receipts; this provider only coordinates presentation and logout. */
export function VoiceAnnouncementProvider({ children }: { children: ReactNode }) {
  const [capabilities, setCapabilities] = useState<VoiceCapabilities | null>(null);
  const [snapshot, setSnapshot] = useState<VoiceSnapshot | null>(null);
  const [error, setError] = useState("");
  const [pendingCount, setPendingCount] = useState(0);
  const [valid, setValid] = useState(true);
  const [consent, setConsent] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const mounted = useRef(true);
  const current = useRef<VoiceSnapshot | null>(null);
  const calls = useRef(0);
  const navigation = useRef<Navigate | null>(null);
  const logout = useRef<{ closeId: string; resolve: (result: string | false) => void } | null>(null);
  const logoutActionActive = useRef(false);
  const pendingLogoutTransition = useRef<PendingLogoutTransition | null>(null);
  const previousCloseReview = useRef<string | null>(null);
  const closeExpiry = capabilities?.available ? snapshot?.close?.review?.expiresAtMs : undefined;
  useEffect(() => {
    if (closeExpiry === undefined) return;
    setNow(Date.now());
    const clock = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(clock);
  }, [closeExpiry, snapshot?.close?.closeId, snapshot?.close?.review?.digest]);
  useEffect(() => { if (snapshot?.close?.review && now >= snapshot.close.review.expiresAtMs) setConsent(false); }, [now, snapshot?.close?.review]);

  const accept = useCallback((value: unknown) => {
    const next = voiceSnapshotFrom(value);
    if (current.current && next.generation < current.current.generation) return current.current;
    current.current = next;
    if (mounted.current) { setSnapshot(next); setValid(true); }
    const identity = next.close?.review ? `${next.close.closeId}:${next.close.review.digest}` : null;
    if (previousCloseReview.current !== identity) {
      previousCloseReview.current = identity;
      if (mounted.current) setConsent(false);
    }
    return next;
  }, []);
  const refresh = useCallback(async () => {
    try { return accept(await invoke("voice_announcement_snapshot")); }
    catch { if (mounted.current) { setValid(false); setError(voiceError(null)); } throw new Error("voice_internal"); }
  }, [accept]);
  const run = useCallback(async (command: Command, request: Record<string, unknown>) => {
    if ((command === "prepare_apply_durable" || command === "resume_receipt") && !capabilities?.durableReceipts) throw new Error("voice_unavailable");
    // Close is deliberately allowed while a file worker runs: native quiesce
    // waits for it and incorporates its result. Never abort the command promise.
    if (calls.current && command !== "begin_close" && command !== "cancel_close") throw new Error("voice_busy");
    calls.current += 1;
    if (mounted.current) { setPendingCount(calls.current); setError(""); }
    try { return accept(await invoke(`voice_announcement_${command}`, { request })); }
    catch (cause) {
      if (mounted.current) setError(voiceError(cause));
      await refresh().catch(() => undefined);
      throw new Error(typeof cause === "string" ? voiceError(cause) : voiceError(null));
    } finally { calls.current -= 1; if (mounted.current) setPendingCount(calls.current); }
  }, [accept, refresh, capabilities]);
  const listReceipts = useCallback(async (instanceId: string) => {
    if (!capabilities?.durableReceipts || calls.current || current.current?.busy || current.current?.close
      || (current.current?.operationId && current.current.phase !== "closed")) throw new Error("voice_busy");
    calls.current += 1;
    if (mounted.current) { setPendingCount(calls.current); setError(""); }
    try {
      const result = voiceReceiptListFrom(await invoke("voice_announcement_list_receipts", { request: { instanceId } }));
      if (result.instanceId !== instanceId) throw new Error("voice_context_changed");
      return result;
    } catch (cause) {
      if (mounted.current) setError(voiceError(cause));
      await refresh().catch(() => undefined);
      throw new Error("voice_internal");
    } finally { calls.current -= 1; if (mounted.current) setPendingCount(calls.current); }
  }, [capabilities, refresh]);

  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    const stops: Array<() => void> = [];
    let timer: ReturnType<typeof setInterval> | undefined;
    if (isTauri()) {
      void invoke("voice_announcement_capabilities").then(voiceCapabilitiesFrom).then(async caps => {
        if (disposed) return;
        setCapabilities(caps);
        if (!caps.available) return;
        await refresh();
        if (!disposed) timer = setInterval(() => { if (!calls.current) void refresh().catch(() => undefined); }, 1000);
      }).catch(() => { if (!disposed) { setValid(false); setError(voiceError(null)); } });
      const subscribe = (event: string, handler: (payload: unknown) => void) => {
        void listen(event, ({ payload }) => { if (!disposed) handler(payload); }).then(stop => disposed ? stop() : stops.push(stop)).catch(() => {
          if (!disposed) { setValid(false); setError(voiceError(null)); }
        });
      };
      subscribe("aurora-voice-close-attention", payload => {
        if (payload && typeof payload === "object" && Object.keys(payload).length === 1 && uuid((payload as { closeId?: unknown }).closeId)) void refresh().catch(() => undefined);
        else { setValid(false); setError(voiceError(null)); }
      });
      subscribe("aurora-voice-close-blocked", payload => {
        const code = payload && typeof payload === "object" && Object.keys(payload).length === 1 ? (payload as { code?: unknown }).code : null;
        setError(voiceError(code));
        void refresh().catch(() => undefined);
      });
    }
    return () => {
      disposed = true; mounted.current = false;
      if (timer) clearInterval(timer);
      stops.forEach(stop => stop());
      // UI disappearance is not native cancellation or receipt disposal.
      logout.current?.resolve(false); logout.current = null;
      pendingLogoutTransition.current = null; logoutActionActive.current = false;
    };
  }, [refresh]);

  const beginPanelClose = useCallback(async () => {
    const selected = current.current;
    const next = await run("begin_close", { operationId: selected?.operationId ?? null, intent: "panel" });
    if (next.close?.intent === "panel" && next.close.state === "ready") await run("finish_close", { closeId: next.close.closeId });
  }, [run]);
  const prepareLogout = useCallback(async (): Promise<string | null | false> => {
    if (!isTauri()) return null;
    const caps = voiceCapabilitiesFrom(await invoke("voice_announcement_capabilities"));
    if (!caps.available) return null;
    if (logoutActionActive.current) throw new Error("voice_close_pending");
    pendingLogoutTransition.current = null;
    logoutActionActive.current = true;
    try {
      const selected = await refresh();
      const next = await run("begin_close", { operationId: selected.operationId, intent: "logout" });
      if (next.close?.intent !== "logout") throw new Error("voice_close_pending");
      if (next.close.state === "ready") return next.close.closeId;
      return await new Promise<string | false>(resolve => { logout.current = { closeId: next.close!.closeId, resolve }; });
    } catch (cause) { logoutActionActive.current = false; throw cause; }
  }, [refresh, run]);
  const finishLogout = useCallback(async (closeId: string | null) => {
    try { if (closeId !== null) await run("finish_close", { closeId }); }
    finally { logoutActionActive.current = false; pendingLogoutTransition.current = null; }
  }, [run]);
  const registerNavigator = useCallback((navigate: Navigate) => {
    navigation.current = navigate;
    return () => { if (navigation.current === navigate) navigation.current = null; };
  }, []);
  const navigate = useCallback<Navigate>((id, target) => navigation.current?.(id, target), []);
  const isBlocking = useCallback(() => calls.current > 0 || !!current.current?.busy || !!current.current?.close, []);

  useEffect(() => {
    const transition = pendingLogoutTransition.current;
    if (!valid || pendingCount || !snapshot || !transition || logout.current?.closeId !== transition.closeId) return;
    if (snapshot.generation <= transition.generation || snapshot.operationId !== transition.operationId || snapshot.instanceId !== transition.instanceId || snapshot.voiceMod !== transition.voiceMod) return;
    const confirmed = transition.kind === "confirm" && snapshot.close?.closeId === transition.closeId && snapshot.close.intent === "logout" && snapshot.close.state === "ready";
    const cancelled = transition.kind === "cancel" && !snapshot.close && !snapshot.busy;
    if (!confirmed && !cancelled) return;
    // A later validated snapshot may be the first evidence after a lost IPC
    // response and failed immediate reads. Never retry approval or infer success
    // from the error. Finish the one waiter only after command work has settled.
    pendingLogoutTransition.current = null;
    logout.current.resolve(confirmed ? transition.closeId : false); logout.current = null;
    if (cancelled) {
      logoutActionActive.current = false;
      if (transition.navigateAfterCancel && transition.instanceId) navigate(transition.instanceId, transition.navigateAfterCancel);
    }
  }, [snapshot, pendingCount, valid, navigate]);

  async function cancelClose(target?: "voice" | "recovery") {
    const selected = current.current;
    const closing = selected?.close;
    const selectedId = selected?.instanceId;
    if (!closing) return;
    if (selected && closing.intent === "logout" && logout.current?.closeId === closing.closeId) pendingLogoutTransition.current = {
      kind: "cancel", closeId: closing.closeId, operationId: selected.operationId,
      instanceId: selected.instanceId, voiceMod: selected.voiceMod, generation: selected.generation, navigateAfterCancel: target,
    };
    const finishCancellation = () => {
      if (logout.current?.closeId === closing.closeId) { logout.current.resolve(false); logout.current = null; }
      pendingLogoutTransition.current = null;
      if (closing.intent === "logout") logoutActionActive.current = false;
      if (target && selectedId) navigate(selectedId, target);
    };
    try {
      await run("cancel_close", { closeId: closing.closeId });
      finishCancellation();
    } catch { /* Reconcile only from a validated snapshot, including later polling. */ }
  }
  async function confirmClose() {
    const selected = current.current;
    const closing = selected?.close;
    if (!valid || !consent || !closing?.review || Date.now() >= closing.review.expiresAtMs || (closing.intent === "logout" && !logout.current)) return;
    setConsent(false);
    if (selected && closing.intent === "logout") pendingLogoutTransition.current = {
      kind: "confirm", closeId: closing.closeId, operationId: selected.operationId,
      instanceId: selected.instanceId, voiceMod: selected.voiceMod, generation: selected.generation,
    };
    try {
      const next = await run("confirm_close", { closeId: closing.closeId, digest: closing.review.digest });
      if (next.close?.closeId !== closing.closeId || next.close.state !== "ready") return;
      if (closing.intent === "logout" && logout.current?.closeId === closing.closeId) {
        pendingLogoutTransition.current = null;
        logout.current.resolve(closing.closeId); logout.current = null;
      } else if (closing.intent === "panel") await run("finish_close", { closeId: closing.closeId });
      // Native exit confirmation re-enters app.exit in its wrapper. Never call
      // finish_close or a frontend exit API to bypass that native barrier.
    } catch { /* Reconcile only from a validated snapshot, including later polling. */ }
  }

  const closing = snapshot?.close;
  const orphanLogout = closing?.intent === "logout" && !logoutActionActive.current;
  const resumableReady = closing?.state === "ready" && (closing.intent === "panel" || orphanLogout);
  const dialog = closing?.state === "reviewRequired" || closing?.state === "waiting" || resumableReady;
  const pending = pendingCount > 0;
  const controller: VoiceAnnouncementController = {
    capabilities, snapshot, error, valid, pending, blocking: pending || !!snapshot?.busy || !!closing,
    run, refresh, listReceipts, beginPanelClose, prepareLogout, finishLogout, registerNavigator, navigate, isBlocking,
  };
  return <Context.Provider value={controller}>
    {children}
    {dialog && <div className="recovery-review-overlay"><section role="alertdialog" aria-modal="true" aria-labelledby="voice-close-title" className="recovery-review">
      <header><h2 id="voice-close-title">{closing?.state === "waiting" ? "Aguardando a operação de arquivos" : resumableReady ? "Fechamento registrado" : "Sair mantendo a alteração na cópia?"}</h2></header>
      <div className="recovery-review-body">
        {orphanLogout && <p role="status">O pedido de saída da conta foi interrompido. Retome o trabalho e, se desejar sair, faça um novo pedido.</p>}
        {closing?.state === "waiting" ? <p>O launcher aguardará a operação iniciada e mostrará seu resultado antes de sair.</p> : closing?.state === "ready" ? <p>{snapshot?.durableReceipt?.state === "saved" ? "O fechamento já foi autorizado; o recibo salvo permanece no histórico da cópia." : snapshot?.durableReceipt?.state === "uncertain" ? "O fechamento já foi autorizado; a operação ainda exige verificação na Recuperação." : "O fechamento já foi autorizado; o recibo não está mais disponível."} Retomar libera a interface sem alterar arquivos ou sair da conta.</p> : <>
          <p>Cópia: <strong>{snapshot?.instanceId}</strong> · {snapshot?.voiceMod ? MODS[snapshot.voiceMod].name : ""}</p>
          {closing?.review?.durableReceipt?.state === "saved" ? <p>A configuração permanecerá como está. O recibo salvo fica no histórico desta cópia para uma retomada explícita, com nova verificação e aprovação. Nenhuma restauração será feita automaticamente.</p>
            : closing?.review?.durableReceipt?.state === "uncertain" ? <p>O resultado da alteração precisa ser verificado na Recuperação. A existência de um recibo utilizável ainda não está confirmada. Nenhuma restauração será feita automaticamente.</p>
            : <><p>A configuração permanecerá como está. Sair descarta a restauração seletiva que existe somente na memória deste processo. Nenhuma restauração será feita automaticamente.</p>
              <p>A Recuperação mantém seu próprio histórico e pode tratar operações interrompidas; ela não recria este recibo.</p></>}
          {closing?.review ? <><p>Revisão válida até {voiceDate(closing.review.expiresAtMs)}.</p>
            <label><input aria-label={closing.review.durableReceipt ? "Autorizo sair mantendo a configuração" : "Aceito perder o recibo de restauração"} type="checkbox" checked={consent} disabled={pending || !valid || orphanLogout || now >= closing.review.expiresAtMs} onChange={event => setConsent(event.target.checked)} />{closing.review.durableReceipt?.state === "saved" ? "Entendi que a cópia continuará alterada e que restaurar exige retomar o recibo e aprovar uma nova revisão." : closing.review.durableReceipt?.state === "uncertain" ? "Entendi que preciso conferir a Recuperação antes de tentar outra alteração." : "Entendi que a cópia continuará alterada e que perderei este recibo."}</label>
            {now >= closing.review.expiresAtMs && <p role="alert">A revisão venceu. Cancele e revise novamente.</p>}
          </> : <p role="alert">Esta revisão não pode mais ser usada. Cancele e revise novamente; o recibo foi preservado.</p>}
        </>}
        {error && <p role="alert">{error}</p>}
      </div>
      <footer><div>
        {resumableReady ? <button autoFocus disabled={pending} onClick={() => void run("finish_close", { closeId: closing.closeId }).catch(() => undefined)} type="button">Retomar trabalho</button> : <button autoFocus className="secondary" disabled={pending} onClick={() => void cancelClose()} type="button">Cancelar saída</button>}
        {closing?.state === "reviewRequired" && <>
          <button className="secondary" disabled={pending} onClick={() => void cancelClose("voice")} type="button">Continuar e revisar restauração</button>
          <button className="secondary" disabled={pending} onClick={() => void cancelClose("recovery")} type="button">Abrir Recuperação</button>
          <button disabled={pending || !valid || orphanLogout || !consent || !closing.review || now >= closing.review.expiresAtMs} onClick={() => void confirmClose()} type="button">Sair mantendo alteração na cópia</button>
        </>}
      </div></footer>
    </section></div>}
    {!dialog && error && <aside className="notice-toast" role="alert"><span>{error}</span><button disabled={pending} onClick={() => { setError(""); void refresh().catch(() => undefined); }} type="button">Atualizar estado do anúncio</button></aside>}
  </Context.Provider>;
}
