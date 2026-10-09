import { invoke, isTauri } from "@tauri-apps/api/core";
import { type FormEvent, useEffect, useRef, useState } from "react";
import {
  capabilitiesFrom, connectDate, connectError, exportFrom, phaseText, preparationFrom, reviewFrom,
  snapshotFrom, statusText, transportText, type ConnectCapabilities, type ConnectMode,
  type ConnectRole, type ConnectSnapshot,
} from "./types";
import "./connect.css";
import { DiscoveryPicker, type DiscoveredTarget } from "./DiscoveryPicker";

type Action = "prepareHost" | "prepareGuest" | "reviewHost" | "reviewGuest" | "confirm" | "export" | "probe" | "stop" | "refresh";
const actionText: Record<Action, string> = {
  prepareHost: "Preparando anfitrião…", prepareGuest: "Preparando convidado…", reviewHost: "Preparando revisão…",
  reviewGuest: "Verificando convite…", confirm: "Estabelecendo conexão…", export: "Obtendo convite…",
  probe: "Consultando servidor…", stop: "Encerrando recursos…", refresh: "Consultando estado…",
};
const finalPhase = (snapshot: ConnectSnapshot) => ["idle", "closed", "failed"].includes(snapshot.phase);

export function ConnectView() {
  const [capabilities, setCapabilities] = useState<ConnectCapabilities>();
  const [snapshot, setSnapshot] = useState<ConnectSnapshot>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [warning, setWarning] = useState("");
  const [message, setMessage] = useState("");
  const [pending, setPending] = useState<Action>();
  const [selectedRole, setSelectedRole] = useState<ConnectRole>("host");
  const [mode, setMode] = useState<ConnectMode>("loopback");
  const [bindAddress, setBindAddress] = useState("127.0.0.1:42000");
  const [gameTarget, setGameTarget] = useState("127.0.0.1:43001");
  const [targetMode, setTargetMode] = useState<"manual" | "ownedInstance">("manual");
  const [discoveredTarget, setDiscoveredTarget] = useState<DiscoveredTarget>();
  const [lifetime, setLifetime] = useState("600");
  const [invitationProfile, setInvitationProfile] = useState<"legacy" | "sessionGameTcp">("legacy");
  const [admissionSeconds, setAdmissionSeconds] = useState("600");
  const [sessionSeconds, setSessionSeconds] = useState("3600");
  const [hostPublicId, setHostPublicId] = useState("");
  const [guestPublicId, setGuestPublicId] = useState("");
  const [invitation, setInvitation] = useState("");
  const [exportedInvitation, setExportedInvitation] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  const [knownOperationId, setKnownOperationId] = useState<string>();
  const epoch = useRef(0);
  const mounted = useRef(false);
  const pendingRef = useRef<Action | undefined>(undefined);
  const snapshotRef = useRef<ConnectSnapshot | undefined>(undefined);
  const observedReview = useRef<string | undefined>(undefined);
  const exportedFor = useRef<string | undefined>(undefined);

  function acceptSnapshot(value: unknown) {
    const next = snapshotFrom(value);
    if (snapshotRef.current && next.generation < snapshotRef.current.generation) return;
    snapshotRef.current = next;
    setSnapshot(next);
    setKnownOperationId(next.operationId ?? undefined);
    if (observedReview.current !== next.review?.digest) {
      observedReview.current = next.review?.digest;
      setAcknowledged(false);
    }
    if (exportedFor.current !== next.operationId || finalPhase(next) || next.transportState === "admitted"
      || (next.sessionTiming && Date.now() >= next.sessionTiming.admissionExpiresAtMs)) {
      setExportedInvitation("");
      exportedFor.current = undefined;
    }
    if (finalPhase(next)) { setGuestPublicId(""); setInvitation(""); }
    setWarning("");
  }

  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    let timer: number | undefined;
    async function poll() {
      if (disposed) return;
      if (!pendingRef.current) {
        const request = epoch.current;
        try {
          const value = await invoke<unknown>("connect_snapshot");
          if (!disposed && request === epoch.current) acceptSnapshot(value);
        } catch {
          if (!disposed && request === epoch.current) setWarning("O estado não pôde ser atualizado. A última observação pode estar desatualizada.");
        }
      }
      if (!disposed) timer = window.setTimeout(() => { void poll(); }, 1_000);
    }
    void (async () => {
      if (!isTauri()) {
        setError("O Aurora Connect experimental requer o aplicativo desktop habilitado.");
        setLoading(false);
        return;
      }
      try {
        const value = capabilitiesFrom(await invoke<unknown>("connect_capabilities"));
        if (disposed) return;
        setCapabilities(value);
        if (value.available) await poll();
      } catch (cause) { if (!disposed) setError(connectError(cause)); }
      finally { if (!disposed) setLoading(false); }
    })();
    return () => { disposed = true; mounted.current = false; epoch.current += 1; if (timer !== undefined) window.clearTimeout(timer); };
  }, []);

  async function perform(action: Action, operation: (current: () => boolean) => Promise<void>) {
    if (pendingRef.current && action !== "stop") return;
    if (pendingRef.current === "stop") return;
    const request = ++epoch.current;
    const current = () => mounted.current && request === epoch.current;
    pendingRef.current = action;
    setPending(action); setError(""); setMessage("");
    try { await operation(current); }
    catch (cause) { if (current()) { setError(connectError(cause)); setAcknowledged(false); } }
    finally { if (current()) { pendingRef.current = undefined; setPending(undefined); } }
  }
  async function refresh(current: () => boolean) {
    const value = await invoke<unknown>("connect_snapshot");
    if (current()) acceptSnapshot(value);
  }
  async function copy(value: string, current = () => mounted.current) {
    try {
      if (!navigator.clipboard?.writeText) throw new Error("Clipboard unavailable");
      await navigator.clipboard.writeText(value);
      if (current()) setMessage("Copiado. Compartilhe somente com o participante autorizado.");
    } catch {
      if (current()) setMessage("A cópia automática não está disponível. Selecione o texto exibido e copie manualmente.");
    }
  }
  function prepare(event: FormEvent) {
    event.preventDefault();
    if (active || !capabilities?.available) return;
    const discovery = selectedRole === "host" && targetMode === "ownedInstance";
    if (discovery && !discoveredTarget) return;
    const session = invitationProfile === "sessionGameTcp" && Boolean(capabilities.sessionInvitations);
    const admission = Number(admissionSeconds), duration = Number(sessionSeconds);
    if (session && selectedRole === "host" && (!Number.isInteger(admission) || admission < 1 || admission > 600
      || !Number.isInteger(duration) || duration < admission || duration > 3600)) {
      setError(connectError("connect_invalid_request"));
      return;
    }
    const command = selectedRole === "host"
      ? session ? discovery ? "connect_prepare_session_host_discovered" : "connect_prepare_session_host"
        : discovery ? "connect_prepare_discovered_host" : "connect_prepare_host"
      : session ? "connect_prepare_session_guest" : "connect_prepare_guest";
    const request = selectedRole === "host"
      ? { mode, bindAddress: bindAddress.trim(), gameTarget: discovery ? discoveredTarget!.target : gameTarget.trim(),
        ...(session ? { admissionSeconds: admission, sessionSeconds: duration } : { lifetimeSeconds: Number(lifetime) }) }
      : { mode, expectedHostPublicId: hostPublicId.trim() };
    void perform(selectedRole === "host" ? "prepareHost" : "prepareGuest", async current => {
      const value = preparationFrom(await invoke<unknown>(command, { request, ...(discovery ? { selectionId: discoveredTarget!.selectionId } : {}) }));
      if (!current()) return;
      setDiscoveredTarget(undefined);
      setKnownOperationId(value.operationId);
      // The preparation is real, but its full state has not been observed yet.
      // Do not let a previous closed snapshot hide the new cancellation handle.
      setSnapshot(undefined);
      if (value.invitationProfile !== (session ? "sessionGameTcp" : undefined)) throw new Error("Wrong preparation profile");
      await refresh(current);
    });
  }
  function review(event: FormEvent) {
    event.preventDefault();
    if (!operationId || !snapshot?.role) return;
    setAcknowledged(false);
    void perform(snapshot.role === "host" ? "reviewHost" : "reviewGuest", async current => {
      const value = reviewFrom(await invoke<unknown>(snapshot.role === "host" ? "connect_review_host" : "connect_review_guest",
        snapshot.role === "host" ? { operationId, guestPublicId: guestPublicId.trim() } : { operationId, encodedInvitation: invitation }));
      if (!current()) return;
      if (value.operationId !== operationId) throw new Error("Wrong operation projection");
      await refresh(current);
    });
  }
  function confirm() {
    const reviewed = snapshot?.review;
    if (!reviewed || !acknowledged || reviewed.operationId !== operationId) return;
    setAcknowledged(false);
    void perform("confirm", async current => {
      const value = await invoke<unknown>("connect_confirm", { operationId: reviewed.operationId, digest: reviewed.digest });
      if (current()) acceptSnapshot(value);
    });
  }
  function stop() {
    if (!operationId) return;
    setAcknowledged(false); setExportedInvitation("");
    void perform("stop", async current => {
      const value = await invoke<unknown>("connect_stop", { operationId });
      if (current()) acceptSnapshot(value);
    });
  }
  function exportInvitation() {
    if (!operationId) return;
    void perform("export", async current => {
      const value = exportFrom(await invoke<unknown>("connect_export_invitation", { operationId }));
      if (!current()) return;
      exportedFor.current = operationId;
      setExportedInvitation(value.invitation);
      await copy(value.invitation, current);
    });
  }
  const operationId = knownOperationId ?? snapshot?.operationId;
  const active = Boolean(operationId && (!snapshot || !finalPhase(snapshot) || snapshot.cleanupState === "pending" || snapshot.cleanupState === "failed"));
  const busy = Boolean(pending);
  const prepared = snapshot?.preparation;
  const reviewed = snapshot?.review;
  const canReview = snapshot?.phase === "hostPrepared" || snapshot?.phase === "guestPrepared";
  const canProbe = snapshot?.phase === "guestConnected";
  const canExport = snapshot?.role === "host" && snapshot.phase === "hostListening" && snapshot.transportState === "listening"
    && (!snapshot.sessionTiming || Date.now() < snapshot.sessionTiming.admissionExpiresAtMs);
  const sessionSelected = invitationProfile === "sessionGameTcp" && Boolean(capabilities?.sessionInvitations);
  const sessionDisplayed = snapshot?.invitationProfile === "sessionGameTcp" || (!active && sessionSelected);

  return <section className="connect-view" aria-labelledby="connect-title">
    <header className="connect-heading"><div><h2 id="connect-title">Aurora Connect</h2><p>Compartilhe um servidor autorizado ou use um convite recebido.</p></div><span className="connect-badge">Experimental</span></header>
    {loading && <p role="status">Verificando disponibilidade…</p>}
    {error && <p className="connect-error" role="alert">{error}</p>}
    {warning && <p className="connect-warning" role="status">{warning}</p>}
    {message && <p className="connect-message" role="status">{message}</p>}
    {!loading && capabilities && !capabilities.available && <p className="connect-warning">Este recurso está desligado nesta versão do launcher.</p>}
    {capabilities?.available && <>
      <p>{sessionDisplayed
        ? "Uma tentativa por janela. O prazo para entrar e o encerramento da sessão são separados, sem renovação automática. O servidor deve permanecer aberto."
        : "Uma tentativa por janela. A conexão expira em até 10 minutos e não é renovada automaticamente. O servidor deve permanecer aberto."}</p>
      <section className="connect-status" aria-label="Estado observado da conexão">
        <header><h3>{snapshot ? snapshot.phase === "hostListening" && snapshot.transportState === "admitted" ? "Convite utilizado · conexão ativa" : phaseText[snapshot.phase] : "Estado ainda não disponível"}</h3><div className="connect-actions">
          <button className="secondary" type="button" disabled={busy} onClick={() => { void perform("refresh", refresh); }}>Atualizar estado</button>
          {active && <button className="danger" type="button" disabled={pending === "stop" || snapshot?.phase === "closing"} onClick={stop}>Encerrar Connect</button>}
        </div></header>
        {pending && <p role="status">{actionText[pending]}</p>}
        {snapshot && <>
          <dl><dt>Transporte</dt><dd>{transportText[snapshot.transportState]}</dd><dt>Servidor Minecraft</dt><dd>{statusText[snapshot.statusState]}</dd>
            <dt>Caminho da conexão</dt><dd>Não confirmado · relay desligado</dd><dt>Entrada no mundo</dt><dd>Não verificada</dd>
            <dt>Simple Voice Chat</dt><dd>Desativado nesta integração</dd><dt>Plasmo Voice</dt><dd>Desativado nesta integração</dd>
            {snapshot.sessionTiming
              ? <><dt>Entrar até</dt><dd>{connectDate(snapshot.sessionTiming.admissionExpiresAtMs)} · horário local</dd><dt>Encerramento da sessão</dt><dd>{connectDate(snapshot.sessionTiming.sessionExpiresAtMs)} · horário local</dd></>
              : snapshot.expiresAtMs !== null && <><dt>Limite desta etapa</dt><dd>{connectDate(snapshot.expiresAtMs)} · horário local</dd></>}
            {snapshot.cleanupState !== "notStarted" && <><dt>Encerramento</dt><dd>{{ pending: "Aguardando liberação dos recursos", complete: "Recursos liberados", failed: "Liberação não confirmada" }[snapshot.cleanupState]}</dd></>}
          </dl>
          {snapshot.errorCode && <p className="connect-error" role="alert">{connectError(snapshot.errorCode)}</p>}
          {snapshot.statusSummary && <div className="connect-status-summary"><p>Última resposta: protocolo {snapshot.statusSummary.reportedProtocol}, {snapshot.statusSummary.protocolMatchesRequested ? "correspondente ao solicitado" : "diferente do solicitado"}.</p>
            <p>Jogadores informados pelo servidor: {snapshot.statusSummary.reportedPlayersOnline ?? "não informado"} / {snapshot.statusSummary.reportedPlayersMax ?? "não informado"}. Essa contagem não confirma quem entrou no mundo.</p></div>}
          {snapshot.tcpAddress && snapshot.phase === "guestConnected" && <div className="connect-output"><label htmlFor="connect-local-address">Endereço para Conexão direta no Minecraft</label><input id="connect-local-address" readOnly value={snapshot.tcpAddress} />
            <div><button className="secondary" type="button" onClick={() => { void copy(snapshot.tcpAddress!); }}>Copiar endereço do jogo</button></div><small>Abra o Minecraft compatível e use Multiplayer → Conexão direta. Este painel não inicia nem fecha o jogo.</small></div>}
          {canProbe && <div><button type="button" disabled={busy || snapshot.statusState === "checking"} onClick={() => { void perform("probe", async current => { const value = await invoke<unknown>("connect_probe_status", { operationId }); if (current()) acceptSnapshot(value); }); }}>Consultar servidor</button><p className="connect-hint">Consulta de Minecraft 1.21.1 (protocolo {capabilities.statusProtocol}), com endereço lógico {capabilities.statusHandshakeHost}:{capabilities.statusHandshakePort}. Não comprova compatibilidade com outras versões.</p></div>}
        </>}
      </section>
      {!active && snapshot && <section className="connect-card">
        <div className="connect-tabs" aria-label="Papel nesta tentativa"><button type="button" aria-pressed={selectedRole === "host"} disabled={busy} onClick={() => { setSelectedRole("host"); setDiscoveredTarget(undefined); }}>Anfitrião</button><button type="button" aria-pressed={selectedRole === "guest"} disabled={busy} onClick={() => { setSelectedRole("guest"); setDiscoveredTarget(undefined); }}>Convidado</button></div>
        <form className="connect-form" onSubmit={prepare}>
          <h3>{selectedRole === "host" ? "Compartilhar servidor local" : "Preparar recebimento do convite"}</h3>
          {capabilities.sessionInvitations && <label>Tipo de convite<select value={invitationProfile} disabled={busy} onChange={event => { setInvitationProfile(event.target.value === "sessionGameTcp" ? "sessionGameTcp" : "legacy"); setDiscoveredTarget(undefined); }}><option value="legacy">Convite curto · até 10 minutos</option><option value="sessionGameTcp">Sessão de jogo · até 1 hora</option></select></label>}
          <label>Modo de conexão<select value={mode} disabled={busy} onChange={event => { const next = event.target.value as ConnectMode; setMode(next); setBindAddress(next === "loopback" ? "127.0.0.1:42000" : ""); }}><option value="loopback">Ensaio neste computador</option><option value="externalIpv6">IPv6 externo autorizado</option></select></label>
          {mode === "externalIpv6" && <p>Use o endereço inicial IPv6 autorizado. Este modo compartilha informações de rede com o participante; não configura o roteador nem garante alcance.</p>}
          {selectedRole === "host" ? <div className="connect-fields">
            <label>Endereço inicial da conexão<input required maxLength={128} value={bindAddress} disabled={busy} placeholder={mode === "loopback" ? "127.0.0.1:42000" : "[IPv6 autorizado]:42000"} onChange={event => setBindAddress(event.target.value)} autoComplete="off" spellCheck={false} /></label>
            {capabilities.ownedInstanceDiscovery && <label>Selecionar servidor<select disabled={busy} value={targetMode} onChange={event => { setTargetMode(event.target.value as "manual" | "ownedInstance"); setDiscoveredTarget(undefined); }}><option value="manual">Endereço manual · servidor dedicado ou local</option><option value="ownedInstance">Jogo aberto pelo Aurora · mundo LAN</option></select></label>}
            {targetMode === "ownedInstance" && capabilities.ownedInstanceDiscovery
              ? <DiscoveryPicker disabled={busy} onSelect={setDiscoveredTarget} />
              : <label>Servidor neste computador<input required maxLength={128} value={gameTarget} disabled={busy} onChange={event => setGameTarget(event.target.value)} autoComplete="off" spellCheck={false} /><small>Somente endereço loopback e porta do jogo que você autoriza compartilhar. A seleção manual não comprova qual processo possui essa porta.</small></label>}
            {sessionSelected ? <>
              <label>Prazo para entrar, em segundos<input required type="number" min={1} max={capabilities.sessionInvitations!.maxAdmissionSeconds} step={1} value={admissionSeconds} disabled={busy} onChange={event => setAdmissionSeconds(event.target.value)} /></label>
              <label>Duração máxima da sessão, em segundos<input required type="number" min={1} max={capabilities.sessionInvitations!.maxSessionSeconds} step={1} value={sessionSeconds} disabled={busy} onChange={event => setSessionSeconds(event.target.value)} /><small>Os dois prazos começam na emissão do convite. A duração da sessão deve ser igual ou maior que o prazo para entrar.</small></label>
            </> : <label>Prazo da sessão, em segundos<input required type="number" min={1} max={capabilities.maxLifetimeSeconds} step={1} value={lifetime} disabled={busy} onChange={event => setLifetime(event.target.value)} /></label>}
          </div> : <label>Código público do anfitrião<input required maxLength={64} value={hostPublicId} disabled={busy} onChange={event => setHostPublicId(event.target.value)} autoComplete="off" spellCheck={false} /><small>Receba este código diretamente do anfitrião, por um canal autorizado.</small></label>}
          <div><button type="submit" disabled={busy || (selectedRole === "host" && targetMode === "ownedInstance" && !discoveredTarget)}>{selectedRole === "host" ? "Preparar anfitrião" : "Preparar convidado"}</button></div>
        </form>
      </section>}
      {prepared && <section className="connect-card"><h3>Troque os códigos públicos</h3><p>{snapshot?.role === "host" ? "Envie seu código ao convidado e receba o código gerado por ele nesta tentativa." : "Envie seu código ao anfitrião e aguarde o convite destinado a esta tentativa."}</p>
        <div className="connect-output"><label htmlFor="connect-public-code">Seu código público</label><textarea id="connect-public-code" readOnly value={prepared.publicId} /><div><button className="secondary" type="button" onClick={() => { void copy(prepared.publicId); }}>Copiar meu código</button></div></div>
        <small>Preparação válida até {connectDate(prepared.expiresAtMs)}. Nenhuma chave privada é exibida.</small>
      </section>}
      {canReview && <form className="connect-card connect-form" onSubmit={review}>
        {snapshot.role === "host" ? <label>Código público recebido do convidado<input required maxLength={64} value={guestPublicId} disabled={busy} onChange={event => { setGuestPublicId(event.target.value); setAcknowledged(false); }} autoComplete="off" spellCheck={false} /></label>
          : <label>Convite recebido<textarea required maxLength={4096} value={invitation} disabled={busy} onChange={event => { setInvitation(event.target.value); setAcknowledged(false); }} autoComplete="off" spellCheck={false} /></label>}
        <div><button type="submit" disabled={busy}>Revisar convite</button></div>
      </form>}
      {reviewed && snapshot?.phase === "reviewed" && <section className="connect-card connect-review" aria-labelledby="connect-review-title"><h3 id="connect-review-title">Revise antes de autorizar</h3>
        <dl><dt>Papel</dt><dd>{reviewed.role === "host" ? "Anfitrião" : "Convidado"}</dd><dt>Modo</dt><dd>{reviewed.mode === "loopback" ? "Neste computador" : "IPv6 externo autorizado"}</dd>
          <dt>Endereço inicial</dt><dd><code>{reviewed.directAddress}</code></dd>{reviewed.gameTarget && <><dt>Destino autorizado</dt><dd><code>{reviewed.gameTarget}</code></dd></>}
          {reviewed.targetSource === "ownedInstance" && <><dt>Origem conferida</dt><dd>Instância {reviewed.instanceId} iniciada por este launcher. A conexão encerra se o processo ou a porta deixar de corresponder à seleção.</dd></>}
          <dt>Serviço</dt><dd>Minecraft · TCP · somente o destino aprovado pelo anfitrião</dd>
          {reviewed.sessionTiming ? <><dt>Prazo para entrar</dt><dd>{reviewed.sessionTiming.admissionSeconds} segundos desde a emissão</dd><dt>Duração máxima da sessão</dt><dd>{reviewed.sessionTiming.sessionSeconds} segundos desde a emissão, sem renovação</dd><dt>Autorize até</dt><dd>{connectDate(reviewed.expiresAtMs)}</dd><dt>Entrar até</dt><dd>{connectDate(reviewed.sessionTiming.admissionExpiresAtMs)}</dd><dt>Encerramento da sessão</dt><dd>{connectDate(reviewed.sessionTiming.sessionExpiresAtMs)}</dd></>
            : <><dt>Prazo</dt><dd>{reviewed.lifetimeSeconds} segundos, sem renovação</dd><dt>Autorize até</dt><dd>{connectDate(reviewed.expiresAtMs)}</dd><dt>Convite e sessão expiram</dt><dd>{connectDate(reviewed.invitationExpiresAtMs)}</dd></>}
          <dt>Consulta do servidor</dt><dd>Minecraft 1.21.1 · protocolo {reviewed.statusProtocol} · endereço lógico {reviewed.statusHandshakeHost}:{reviewed.statusHandshakePort}</dd>
          <dt>Anfitrião</dt><dd><code>{reviewed.hostPublicId}</code></dd><dt>Convidado</dt><dd><code>{reviewed.guestPublicId}</code></dd></dl>
        <p>Uma única admissão. Encerrar ou perder esta conexão exige preparar um novo convite. Nenhum arquivo do jogo será alterado.</p>
        <details><summary>Identificadores da revisão</summary><p>Revisão: <code>{reviewed.digest}</code></p><p>Convite: <code>{reviewed.invitationDigest}</code></p></details>
        <label className="connect-consent"><input type="checkbox" checked={acknowledged} disabled={busy} onChange={event => setAcknowledged(event.target.checked)} /><span>Conferi o participante, o destino e {reviewed.sessionTiming ? "os dois prazos" : "o prazo"} e autorizo esta conexão experimental.</span></label>
        <div className="connect-actions"><button type="button" disabled={busy || !acknowledged} onClick={confirm}>Autorizar conexão</button><button className="secondary" type="button" disabled={pending === "stop"} onClick={stop}>Cancelar tentativa</button></div>
      </section>}
      {canExport && <section className="connect-card"><h3>Envie o convite ao participante</h3><p>Somente o convidado autorizado pode usar este convite. Envie também seu código público pelo canal combinado.</p><div><button type="button" disabled={busy} onClick={exportInvitation}>Copiar convite</button></div>
        {exportedInvitation && <label className="connect-output">Convite desta tentativa<textarea readOnly value={exportedInvitation} spellCheck={false} /></label>}
      </section>}
      <small>Navegar pelo launcher mantém a tentativa ativa. Use Encerrar Connect para liberar a ponte; o Minecraft continua sob seu controle. Porta revisada e resposta de servidor não comprovam entrada no mundo ou voz.</small>
    </>}
  </section>;
}
