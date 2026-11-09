import { useEffect, useState } from "react";
import type { SocialService, SocialSession } from "../../shared/social";
import { socialConnectError, socialFailureGateText, socialFailureStageText, socialGameTarget, socialRuntimeLabels, socialStageText } from "./connect-types";
import { DiscoveryPicker, type DiscoveredTarget } from "../connect/DiscoveryPicker";
import type { SocialConnectController, SocialConnectState } from "./connect-controller";

export type SocialConnectionControls = { state: SocialConnectState; controller: SocialConnectController };

function NativeVoiceLimits() {
  return <div className="social-voice-limits">
    <p>Voz para até 8 participantes, incluindo o anfitrião quando jogar neste computador. O limite do anúncio pode ser menor.</p>
    <details><summary>Limites técnicos da sessão</summary>
      <ul><li>Mensagens de voz de até 4.096 bytes.</li>
        <li>Retorno por participante: até 524.288 bytes/s (512 KiB/s) e 1.600 fragmentos/s.</li>
        <li>Tráfego agregado da sessão (TCP e UDP, nos dois sentidos): até 5.000.000 bytes/s, com rajada compartilhada de 128 KiB.</li></ul>
    </details>
    <small>A disponibilidade também depende das cotas gratuitas do serviço. Áudio dentro do jogo ainda não validado.</small>
  </div>;
}

/** Presentation only: the App-owned controller and native manager survive tabs. */
export function SocialConnectionPanel({ connection, ownSession, now, catalogNow = now, onOpenConnectedGame, onPrepareDedicatedServer, service }: {
  connection: SocialConnectionControls; ownSession?: SocialSession | null; now: number; catalogNow?: number; onOpenConnectedGame?: () => void; onPrepareDedicatedServer?: () => void; service?: SocialService | null;
}) {
  const { state, controller } = connection;
  const snapshot = state.snapshot, social = snapshot?.social;
  const [consent, setConsent] = useState(false);
  const [copyMessage, setCopyMessage] = useState("");
  const [selected, setSelected] = useState<DiscoveredTarget>();
  const [selectionEpoch, setSelectionEpoch] = useState(0);
  const [useVoice, setUseVoice] = useState(false);
  const [hostPlayer, setHostPlayer] = useState(false);
  const [rosterNames, setRosterNames] = useState<{ service: SocialService; operationId: string; key: string; names: Record<string, string> }>();
  const rosterKey = social?.role === "host" && snapshot?.phase === "hostListening"
    ? (social.participants ?? []).map(person => person.accountId).sort().join(",") : "";
  const operationId = snapshot?.operationId;
  useEffect(() => {
    let live = true;
    if (!service?.profile || !rosterKey || !operationId) { setRosterNames(undefined); return; }
    const source = service;
    // At most eight exact reads, once per roster. Names never authorize transport.
    void Promise.all(rosterKey.split(",").slice(0, 8).map(async uid => {
      try {
        const person = await source.profile!(uid);
        return person?.uid === uid && /^[A-Za-z0-9_]{3,16}$/.test(person.nickname) ? [uid, person.nickname] as const : null;
      } catch { return null; }
    })).then(people => {
      if (live) setRosterNames({ service: source, operationId, key: rosterKey, names: Object.fromEntries(people.filter(person => person !== null)) });
    });
    return () => { live = false; };
  }, [service, operationId, rosterKey]);
  const participantName = (uid: string) => rosterNames && rosterNames.service === service && rosterNames.operationId === operationId && rosterNames.key === rosterKey
    ? rosterNames.names[uid] ?? "Participante" : "Participante";
  const active = !!snapshot?.operationId && !( ["closed", "failed"].includes(snapshot.phase) && snapshot.cleanupState === "complete");
  useEffect(() => { if (!state.nativeVoiceReady && !active) { setUseVoice(false); setHostPlayer(false); } }, [state.nativeVoiceReady, active]);
  useEffect(() => { if ((ownSession?.maxPlayers ?? 0) < 2) setHostPlayer(false); }, [ownSession?.maxPlayers]);
  useEffect(() => { setConsent(false); setCopyMessage(""); }, [snapshot?.operationId, social?.digest]);
  useEffect(() => { setSelected(undefined); setSelectionEpoch(value => value + 1); }, [ownSession?.id, ownSession?.visibility, ownSession?.joinPolicy]);
  const hostEligible = !!ownSession && ownSession.expiresAtMs > catalogNow
    && ["public", "friends"].includes(ownSession.visibility);
  const retiredOwnSession = state.maxSessionSeconds === 14400 && snapshot?.cleanupState === "complete"
    && ["closed", "failed"].includes(snapshot.phase) && social?.role === "host" && social.sessionId === ownSession?.id;
  const expired = social?.expiresAtMs !== null && social?.expiresAtMs !== undefined && social.expiresAtMs <= now;
  const canReview = snapshot?.phase === "reviewed" && social?.stage === "hostReview" && !!social.digest && !expired && !state.pending;
  const canReviewGuest = snapshot?.phase === "reviewed" && social?.role === "guest" && social.stage === "guestReview"
    && social.voiceTransportProfile === "nativeVoice8V1" && !!social.digest && !expired && !state.pending;
  const discovery = state.ownedGameTcpDiscovery === true;
  const voiceReady = state.maxSessionSeconds === 14400 && !!state.nativeVoiceReady;
  const sourceReady = useVoice ? voiceReady && (!hostPlayer || (ownSession?.maxPlayers ?? 0) >= 2) : !!selected && selected.expiresAtMs > now && socialGameTarget(selected.target);
  const maxGuests = social?.maxGuests ?? (state.maxSessionSeconds === 14400 ? 8 : 1);
  const reviewMinutes = social?.expiresAtMs ? Math.max(1, Math.ceil((social.expiresAtMs - now) / 60_000)) : 0;
  const readyGuest = social?.role === "guest" && snapshot?.role === "guest" && social.stage === "connected" && snapshot.phase === "guestConnected" && snapshot.transportState === "authenticated";
  const readyHostPlayer = social?.role === "host" && social.hostPlayer === true && snapshot?.role === "host" && snapshot.phase === "hostListening" && ["connected", "waitingGuest"].includes(social.stage);
  const canOpenGame = !!onOpenConnectedGame && state.available && social?.readyGameLaunchAvailable === true && (readyGuest || readyHostPlayer)
    && !!snapshot.tcpAddress && !["complete", "failed"].includes(snapshot.cleanupState) && social.expiresAtMs !== null && social.expiresAtMs > now && !state.pending && !state.error;
  return <section className="social-connection" aria-label="Sua conexão multiplayer">
    <div className="social-community-heading"><div><h3>{snapshot?.phase === "closing" ? "Desconectando…" : social ? socialStageText[social.stage] : active ? "Há outra conexão em andamento" : "Sua conexão multiplayer"}</h3>
      <p>{state.maxSessionSeconds === 14400 ? "Compartilhamento por até 4 horas, limitado ao prazo da origem e à disponibilidade das cotas gratuitas." : "Uma pessoa convidada, somente jogo, por até 60 segundos. Voz desativada."}</p></div><span className="social-tag">Experimental</span></div>
    {state.error && <p className="social-notice" role="alert">{state.error}</p>}
    {snapshot?.errorCode && <p className="social-notice" role="alert">
      {social?.failureStage && <><strong>{socialFailureStageText[social.failureStage]}.</strong>{" "}</>}
      {socialConnectError(snapshot.errorCode)}
    </p>}
    {snapshot?.errorCode && social?.failureObservation && <details className="social-notice">
      <summary>Detalhes da interrupção</summary>
      <p>Verificação interrompida: {socialFailureGateText[social.failureObservation.gate]}.</p>
      <ul>
        {social.failureObservation.bootstrapRoundtripMs !== null && <li>Abertura do serviço: {social.failureObservation.bootstrapRoundtripMs} ms.</li>}
        {social.failureObservation.bootstrapUncertaintyMs !== null && <li>Incerteza do horário confirmado: {social.failureObservation.bootstrapUncertaintyMs} ms.</li>}
        {social.failureObservation.hostPollElapsedMs !== null && <li>Última consulta de autorização: {social.failureObservation.hostPollElapsedMs} ms.</li>}
        {social.failureObservation.hostPollAgeMs !== null && <li>Idade da última autorização confirmada: {social.failureObservation.hostPollAgeMs} ms.</li>}
        <li>Prazo de sessão ainda restante: {Math.ceil(social.failureObservation.sessionRemainingMs / 1000)} s.</li>
      </ul>
    </details>}
    {active && !social && <p>Encerre a conexão anterior para compartilhar ou entrar em uma sessão.</p>}
    {!active && !voiceReady && state.available && onPrepareDedicatedServer && <div className="social-notice">
      <p>Para hospedar uma cópia dedicada, revise o preparo do servidor e depois escolha a origem pronta aqui.</p>
      <button type="button" className="secondary" disabled={state.pending} onClick={() => { if (!state.pending) onPrepareDedicatedServer(); }}>Preparar servidor dedicado</button>
    </div>}
    {!active && hostEligible && voiceReady && <div className="social-notice">
      <label className="social-consent"><input type="checkbox" checked={useVoice} disabled={state.pending} onChange={event => setUseVoice(event.target.checked)} />Compartilhar a origem de voz pronta ({state.nativeVoiceReady === "simpleVoiceChat" ? "Simple Voice Chat" : "Plasmo Voice"})</label>
      {useVoice && <label className="social-consent"><input type="checkbox" checked={hostPlayer} disabled={state.pending || (ownSession?.maxPlayers ?? 0) < 2} onChange={event => setHostPlayer(event.target.checked)} />Também jogar neste computador (até 7 convidados e 1 vaga local, dentro do limite do anúncio)</label>}
    </div>}
    {!active && hostEligible && discovery && !useVoice && <>
      <DiscoveryPicker key={selectionEpoch} disabled={!state.available || state.pending} onSelect={setSelected} />
      {selected && !socialGameTarget(selected.target) && <p role="alert">Esta sessão local exige uma porta IPv4 em 127.0.0.1. Escolha outra porta do próprio jogo.</p>}
      {selected && selected.expiresAtMs <= now && <p role="alert">A seleção expirou. Faça uma nova busca.</p>}
    </>}
    {social?.stage === "hostReview" && <div className="social-notice">
      <strong>Compartilhar o servidor deste computador</strong>
      <p>Servidor: {social.gameTarget}. Acesso {social.visibility === "friends" ? "somente para seus amigos" : "para a comunidade"}, com {maxGuests} {maxGuests === 1 ? "vaga" : "vagas para convidados"}{social.hostPlayer ? " e uma vaga local" : ""}.</p>
      <p>O Aurora encaminhará a conexão de jogo{social.voiceProfile ? ` e ${social.voiceProfile === "svc" ? "Simple Voice Chat" : "Plasmo Voice"}` : ""}. Isso não inicia Minecraft.</p>
      {social.voiceTransportProfile === "nativeVoice8V1" && <NativeVoiceLimits />}
      <label className="social-consent"><input type="checkbox" checked={consent} disabled={!canReview} onChange={event => setConsent(event.target.checked)} />Autorizo compartilhar este servidor {social.voiceProfile ? `por até ${reviewMinutes} minutos, dentro do prazo da origem` : state.maxSessionSeconds === 14400 ? "por até 4 horas" : "por até 60 segundos"}.</label>
      <button disabled={!canReview || !consent} onClick={() => { if (canReview && consent) void controller.confirmHost(); }}>Compartilhar agora</button>
      {expired && <p>Esta revisão expirou. Encerre a tentativa para preparar outra.</p>}
    </div>}
    {social?.stage === "guestReview" && <div className="social-notice">
      <strong>Entrar nesta sessão com {social.voiceProfile === "svc" ? "Simple Voice Chat" : "Plasmo Voice"}</strong>
      <NativeVoiceLimits />
      <p>O Aurora abrirá o transporte de jogo e voz após sua confirmação. Isso não inicia Minecraft.</p>
      <p>Esta revisão usa o prazo original da entrada: {Math.max(0, Math.ceil(((social.expiresAtMs ?? now) - now) / 1000))} segundos restantes.</p>
      <label className="social-consent"><input type="checkbox" checked={consent} disabled={!canReviewGuest} onChange={event => setConsent(event.target.checked)} />Autorizo a entrada com jogo e voz dentro destes limites.</label>
      <button disabled={!canReviewGuest || !consent} onClick={() => { if (canReviewGuest && consent && social.expiresAtMs !== null && social.expiresAtMs > Date.now()) void controller.confirmGuest(); }}>Confirmar entrada com voz</button>
      <button className="secondary" disabled={snapshot?.phase === "closing"} onClick={() => void controller.stop()}>Cancelar entrada</button>
      {expired && <p>Esta revisão expirou. Cancele a entrada para iniciar uma nova tentativa.</p>}
    </div>}
    {social?.stage === "waitingGuest" && snapshot?.phase === "hostListening" && <p>A sessão está disponível para entrada autorizada. Mantenha o servidor aberto.</p>}
    {social?.stage === "connected" && snapshot && ["hostListening", "guestConnected"].includes(snapshot.phase) && <p>A conexão foi autenticada. Entrada no mundo e disponibilidade do jogo ainda não verificadas.</p>}
    {social?.runtimeProfile && <p>Jogo da origem: {socialRuntimeLabels[social.runtimeProfile]}. A cópia para jogar precisa usar este mesmo perfil.</p>}
    {social?.voiceState && <p role="status">Voz: {({ notStarted: "aguardando conexão", enabled: "transporte disponível; áudio ainda não verificado", retiring: "encerrando transporte", retired: "transporte encerrado; jogo continua disponível", failed: "transporte indisponível; jogo continua disponível", closed: "transporte encerrado" })[social.voiceState]}.</p>}
    {social?.hostPlayerVoiceState && <p role="status">Voz neste computador: {social.hostPlayerVoiceState === "enabled" ? "transporte disponível; áudio ainda não verificado" : "transporte encerrado; os demais participantes mantêm seu próprio estado"}.</p>}
    {social?.role === "host" && social.participants && snapshot?.phase === "hostListening" && <div>
      <h4>Conexões da sessão ({social.participants.length}/{maxGuests})</h4>
      <p>O estado indica o transporte autenticado. A entrada no mundo ainda não foi verificada.</p>
      {!social.participants.length && <p>Nenhum convidado conectado.</p>}
      <ul className="social-list">{social.participants.map(person => <li className="social-friend-row" key={person.grantId}>
        <span>{participantName(person.accountId)} · {({ waiting: "aguardando conexão", connected: "conexão autenticada", disconnected: "conexão fechada", removing: "removendo" })[person.state]}</span>
        <button className="danger" disabled={state.pending || person.state === "removing"} onClick={() => void controller.revokeParticipant(person.grantId)}>Remover desta sessão</button>
      </li>)}</ul>
      {!!social.participants.length && <small>Remover encerra a conexão e impede nova entrada dessa conta nesta sessão. O grupo social permanece igual.</small>}
    </div>}
    {snapshot?.cleanupState === "complete" && <p role="status">Recursos da conexão liberados.</p>}
    {snapshot?.tcpAddress && (snapshot.phase === "guestConnected" || social?.hostPlayer && snapshot.phase === "hostListening") && <div className="social-connect-address">
      <label htmlFor="social-local-address">Endereço para Conexão direta no Minecraft</label><input id="social-local-address" value={snapshot.tcpAddress} readOnly />
      <button className="secondary" onClick={() => { const address = snapshot.tcpAddress;
        if (!navigator.clipboard?.writeText) { setCopyMessage("Selecione e copie o endereço acima."); return; }
        if (address) void navigator.clipboard.writeText(address).then(() => setCopyMessage("Endereço copiado."), () => setCopyMessage("Selecione e copie o endereço acima.")); }}>Copiar endereço</button>
      {copyMessage && <small role="status">{copyMessage}</small>}
    </div>}
    <div className="social-actions">
      {canOpenGame && <button onClick={() => {
        if (canOpenGame && social?.expiresAtMs && social.expiresAtMs > Date.now()) onOpenConnectedGame?.();
      }}>Abrir Minecraft</button>}
      {!active && <button disabled={!state.available || state.pending || !hostEligible || retiredOwnSession || ((discovery || useVoice || state.maxSessionSeconds === 14400) && !sourceReady)} onClick={() => {
        if (!hostEligible || retiredOwnSession || !ownSession || ownSession.visibility === "private" || !(ownSession.expiresAtMs > (service?.now ? service.now() : Date.now()))) return;
        if (useVoice) { if (voiceReady) void controller.prepareHostNativeVoice(ownSession.id, ownSession.visibility, hostPlayer); }
        else if (discovery) {
          if (!selected || selected.expiresAtMs <= Date.now() || !socialGameTarget(selected.target)) return;
          const selection = selected.selectionId;
          setSelected(undefined); setSelectionEpoch(value => value + 1);
          void controller.prepareHostDiscovered(ownSession.id, ownSession.visibility, selection);
        } else void controller.prepareHost(ownSession.id, ownSession.visibility);
      }}>Preparar meu servidor</button>}
      {active && <button className="danger" disabled={snapshot?.phase === "closing"} onClick={() => void controller.stop()}>Desconectar</button>}
      <button className="secondary" disabled={state.pending} onClick={() => void controller.refresh()}>Atualizar conexão</button>
    </div>
    {!active && !hostEligible && <small>Para hospedar, publique um anúncio para amigos ou para a comunidade e escolha entrada aberta ou aprovação de pedidos.</small>}
    {!active && retiredOwnSession && <small>Esta sessão foi encerrada. Em Meu anúncio de sessão, encerre o anúncio e publique outro para autorizar uma nova sessão.</small>}
    {!active && !discovery && hostEligible && state.maxSessionSeconds !== 14400 && <small>Este executável usa a fonte de teste fixa em 127.0.0.1:43001.</small>}
  </section>;
}
