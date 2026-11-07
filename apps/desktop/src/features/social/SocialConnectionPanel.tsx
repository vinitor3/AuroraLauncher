import { useEffect, useState } from "react";
import type { SocialSession } from "../../shared/social";
import { socialConnectError, socialGameTarget, socialStageText } from "./connect-types";
import { DiscoveryPicker, type DiscoveredTarget } from "../connect/DiscoveryPicker";
import type { SocialConnectController, SocialConnectState } from "./connect-controller";

export type SocialConnectionControls = { state: SocialConnectState; controller: SocialConnectController };

/** Presentation only: the App-owned controller and native manager survive tabs. */
export function SocialConnectionPanel({ connection, ownSession, now }: {
  connection: SocialConnectionControls; ownSession?: SocialSession | null; now: number;
}) {
  const { state, controller } = connection;
  const snapshot = state.snapshot, social = snapshot?.social;
  const [consent, setConsent] = useState(false);
  const [copyMessage, setCopyMessage] = useState("");
  const [selected, setSelected] = useState<DiscoveredTarget>();
  const [selectionEpoch, setSelectionEpoch] = useState(0);
  const active = !!snapshot?.operationId && !( ["closed", "failed"].includes(snapshot.phase) && snapshot.cleanupState === "complete");
  useEffect(() => { setConsent(false); setCopyMessage(""); }, [snapshot?.operationId, social?.digest]);
  useEffect(() => { setSelected(undefined); setSelectionEpoch(value => value + 1); }, [ownSession?.id, ownSession?.visibility, ownSession?.joinPolicy]);
  const hostEligible = !!ownSession && ownSession.expiresAtMs > now && ownSession.joinPolicy === "open"
    && ["public", "friends"].includes(ownSession.visibility);
  const expired = social?.expiresAtMs !== null && social?.expiresAtMs !== undefined && social.expiresAtMs <= now;
  const canReview = snapshot?.phase === "reviewed" && social?.stage === "hostReview" && !!social.digest && !expired && !state.pending;
  const discovery = state.ownedGameTcpDiscovery === true;
  const sourceReady = !!selected && selected.expiresAtMs > now && socialGameTarget(selected.target);
  return <section className="social-connection" aria-label="Sua conexão multiplayer">
    <div className="social-community-heading"><div><h3>{snapshot?.phase === "closing" ? "Desconectando…" : social ? socialStageText[social.stage] : active ? "Há outra conexão em andamento" : "Sua conexão multiplayer"}</h3>
      <p>Teste local: uma pessoa convidada, somente jogo, por até 60 segundos. Voz desativada.</p></div><span className="social-tag">Experimental</span></div>
    {state.error && <p className="social-notice" role="alert">{state.error}</p>}
    {snapshot?.errorCode && <p className="social-notice" role="alert">{socialConnectError(snapshot.errorCode)}</p>}
    {active && !social && <p>Encerre a conexão anterior para compartilhar ou entrar em uma sessão.</p>}
    {!active && hostEligible && discovery && <>
      <DiscoveryPicker key={selectionEpoch} disabled={!state.available || state.pending} onSelect={setSelected} />
      {selected && !socialGameTarget(selected.target) && <p role="alert">Esta sessão local exige uma porta IPv4 em 127.0.0.1. Escolha outra porta do próprio jogo.</p>}
      {selected && selected.expiresAtMs <= now && <p role="alert">A seleção expirou. Faça uma nova busca.</p>}
    </>}
    {social?.stage === "hostReview" && <div className="social-notice">
      <strong>Compartilhar o servidor deste computador</strong>
      <p>Servidor: {social.gameTarget}. Acesso {social.visibility === "friends" ? "somente para seus amigos" : "para a comunidade"}, com uma vaga.</p>
      <p>O Aurora encaminhará a conexão de jogo. Isso não inicia Minecraft nem altera as configurações do servidor.</p>
      <label className="social-consent"><input type="checkbox" checked={consent} disabled={!canReview} onChange={event => setConsent(event.target.checked)} />Autorizo compartilhar este servidor durante o teste.</label>
      <button disabled={!canReview || !consent} onClick={() => { if (canReview && consent) void controller.confirmHost(); }}>Compartilhar agora</button>
      {expired && <p>Esta revisão expirou. Encerre a tentativa para preparar outra.</p>}
    </div>}
    {social?.stage === "waitingGuest" && snapshot?.phase === "hostListening" && <p>A sessão está disponível para entrada autorizada. Mantenha o servidor aberto.</p>}
    {social?.stage === "connected" && snapshot && ["hostListening", "guestConnected"].includes(snapshot.phase) && <p>A conexão foi autenticada. Entrada no mundo e disponibilidade do jogo ainda não verificadas.</p>}
    {snapshot?.cleanupState === "complete" && <p role="status">Recursos da conexão liberados.</p>}
    {snapshot?.tcpAddress && snapshot.phase === "guestConnected" && <div className="social-connect-address">
      <label htmlFor="social-local-address">Endereço para Conexão direta no Minecraft</label><input id="social-local-address" value={snapshot.tcpAddress} readOnly />
      <button className="secondary" onClick={() => { const address = snapshot.tcpAddress;
        if (!navigator.clipboard?.writeText) { setCopyMessage("Selecione e copie o endereço acima."); return; }
        if (address) void navigator.clipboard.writeText(address).then(() => setCopyMessage("Endereço copiado."), () => setCopyMessage("Selecione e copie o endereço acima.")); }}>Copiar endereço</button>
      {copyMessage && <small role="status">{copyMessage}</small>}
    </div>}
    <div className="social-actions">
      {!active && <button disabled={!state.available || state.pending || !hostEligible || (discovery && !sourceReady)} onClick={() => {
        if (!hostEligible || !ownSession || ownSession.visibility === "private") return;
        if (discovery) {
          if (!selected || selected.expiresAtMs <= Date.now() || !socialGameTarget(selected.target)) return;
          const selection = selected.selectionId;
          setSelected(undefined); setSelectionEpoch(value => value + 1);
          void controller.prepareHostDiscovered(ownSession.id, ownSession.visibility, selection);
        } else void controller.prepareHost(ownSession.id, ownSession.visibility);
      }}>Preparar meu servidor</button>}
      {active && <button className="danger" disabled={snapshot?.phase === "closing"} onClick={() => void controller.stop()}>Desconectar</button>}
      <button className="secondary" disabled={state.pending} onClick={() => void controller.refresh()}>Atualizar conexão</button>
    </div>
    {!active && !hostEligible && <small>Para hospedar, publique um anúncio para amigos ou para a comunidade com entrada aberta.</small>}
    {!active && !discovery && hostEligible && <small>Este executável usa a fonte de teste fixa em 127.0.0.1:43001.</small>}
  </section>;
}
