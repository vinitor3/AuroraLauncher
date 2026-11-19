import { invoke, isTauri } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { nativeVoiceError, nativeVoiceSnapshotFrom, nativeVoiceUdpLabel, nativeVoiceEulaLabel, voicePhase, voiceReviewDeadline, voiceReviewDetails, type VoiceReviewDeadline, type NativeVoiceSnapshot, type VoiceProfile, voiceRuntimes, type VoiceRuntime } from "./types";

/** A native owned dedicated session; this panel holds presentation only. */
export function NativeVoicePanel({ disabled = false, revealRequest = 0, revealReady = true }: { disabled?: boolean; revealRequest?: number; revealReady?: boolean }) {
  const [snapshot, setSnapshot] = useState<NativeVoiceSnapshot>();
  const panel = useRef<HTMLDetailsElement>(null);
  const heading = useRef<HTMLElement>(null);
  const revealed = useRef(0);
  const observed = useRef<NativeVoiceSnapshot | undefined>(undefined);
  const [profile, setProfile] = useState<VoiceProfile>("simpleVoiceChat");
  const [runtime, setRuntime] = useState<VoiceRuntime>("fabric-loader-0.16.14-1.21.1");
  const [guests, setGuests] = useState("");
  const [localPlayer, setLocalPlayer] = useState(true);
  const [pending, setPending] = useState(false);
  const inFlight = useRef(false);
  const [approved, setApproved] = useState(false);
  const [error, setError] = useState("");
  const [invitation, setInvitation] = useState("");
  const deadline = useRef<VoiceReviewDeadline | undefined>(undefined);
  const [now, setNow] = useState(() => performance.now());
  const mounted = useRef(false);
  const scope = useRef(0);
  const disabledNow = useRef(disabled); disabledNow.current = disabled;
  useEffect(() => {
    if (disabled || !revealReady || !snapshot?.available || revealRequest <= revealed.current || !panel.current) return;
    const target = panel.current;
    target.open = true;
    // The preceding Connect view loads asynchronously. Wait for that layout,
    // then open the details before focusing its heading in the next frame.
    const frame = window.requestAnimationFrame(() => {
      if (disabledNow.current || !target.isConnected || target.closest("[hidden], [inert]")) return;
      heading.current?.focus({ preventScroll: true });
      heading.current?.scrollIntoView?.({ block: "start", behavior: "auto" });
      revealed.current = revealRequest;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [revealRequest, revealReady, disabled, snapshot?.available]);
  function accept(value: unknown, sentAt?: number) {
    const next = nativeVoiceSnapshotFrom(value);
    if (observed.current && next.revision < observed.current.revision) return false;
    if (observed.current?.review?.digest !== next.review?.digest || observed.current?.review?.kind !== next.review?.kind) {
      setApproved(false); deadline.current = undefined;
    }
    if (next.review && sentAt !== undefined) deadline.current = voiceReviewDeadline(deadline.current, next.review, sentAt);
    setNow(performance.now());
    observed.current = next; setSnapshot(next);
    return true;
  }
  useEffect(() => {
    mounted.current = true;
    if (!isTauri()) return () => { mounted.current = false; scope.current++; };
    let closed = false; let polling = false;
    async function poll() {
      if (closed || polling || document.hidden) return;
      polling = true;
      const sentAt = performance.now();
      try { const value = await invoke("native_voice_snapshot"); if (!closed) accept(value, sentAt); }
      catch { if (!closed) setError("Não foi possível observar a sessão nativa de voz."); }
      finally { polling = false; }
    }
    const visible = () => { if (!document.hidden) void poll(); };
    void poll(); const timer = window.setInterval(() => void poll(), 1000);
    document.addEventListener("visibilitychange", visible);
    return () => { mounted.current = false; scope.current++; closed = true; window.clearInterval(timer); document.removeEventListener("visibilitychange", visible); };
  }, []);
  useEffect(() => { if (disabled) { scope.current++; setApproved(false); setInvitation(""); } }, [disabled]);
  useEffect(() => {
    if (!snapshot?.review) return;
    const timer = window.setInterval(() => setNow(performance.now()), 250);
    return () => window.clearInterval(timer);
  }, [snapshot?.review?.digest]);
  async function execute(request: Record<string, unknown>) {
    const stop = request.action === "stop";
    if (disabled || (inFlight.current && !stop)) return;
    if (!stop) { inFlight.current = true; setPending(true); }
    const generation = scope.current;
    const current = () => mounted.current && !disabledNow.current && generation === scope.current;
    setError(""); setInvitation("");
    try {
      const value = nativeVoiceSnapshotFrom(await invoke("native_voice_execute", { request }));
      if (current() && accept(value) && value.exportedInvitation) setInvitation(value.exportedInvitation);
    } catch (cause) { if (current()) setError(nativeVoiceError(cause)); }
    finally {
      // A mutating command can take minutes before creating a fresh review.
      // Only a separate snapshot read supplies its conservative display clock.
      if (current()) {
        const sentAt = performance.now();
        try { const value = await invoke("native_voice_snapshot"); if (current()) accept(value, sentAt); }
        catch { if (current()) setError("Não foi possível observar a sessão nativa de voz."); }
      }
      if (!stop) { inFlight.current = false; if (mounted.current) setPending(false); }
    }
  }
  if (!snapshot?.available) return null;
  const review = snapshot.review;
  const terminal = ["idle", "stopped", "restored", "reviewExpired", "failed", "needsRecovery", "restoreReviewed", "eulaRestoreReviewed"].includes(snapshot.phase);
  const recoveryPending = snapshot.recoveryPending || snapshot.eulaRecoveryAvailable || snapshot.eulaState === "needsRecovery";
  const canPrepare = ["idle", "stopped", "restored", "reviewExpired", "failed"].includes(snapshot.phase) && !recoveryPending;
  const approveAction = review && { preparation: "approvePreparation", files: "approveFiles", execution: "approveExecution", host: "approveHost", restore: "approveRestore", eulaRestore: "approveEulaRestore", socialSource: "approveSocialSource" }[review.kind];
  const anchored = !!review && deadline.current?.digest === review.digest && deadline.current.kind === review.kind;
  const reviewRemaining = anchored ? Math.max(0, deadline.current!.at - now) : 0;
  const canApprove = !!review && anchored && reviewRemaining > 0 && !disabled && !pending && (review.kind !== "socialSource" || snapshot.phase === "socialSourceReviewed");
  const details = review ? voiceReviewDetails(review.details) : undefined;
  const guestIds = guests.split(/\s+/).filter(Boolean);
  const count = guestIds.length + Number(localPlayer);
  return <details className="connect-native-voice theme-protected" ref={panel}>
    <summary ref={heading} tabIndex={0}>Voz dos mods · sessão dedicada experimental</summary>
    <p>Simple Voice Chat e Plasmo Voice usam seus próprios mods. Este fluxo cria uma cópia dedicada local, anuncia a ponte 127.0.0.1:35554 e mantém a recuperação do anúncio. Áudio e entrada no mundo precisam de validação manual.</p>
    <dl><dt>Sessão</dt><dd>{voicePhase[snapshot.phase]}</dd><dt>UDP</dt><dd>{snapshot.phase === "socialSource" && snapshot.udpState === "notStarted" ? "Acompanhe em Amigos e comunidade" : nativeVoiceUdpLabel(snapshot.udpState)}</dd><dt>EULA</dt><dd>{nativeVoiceEulaLabel(snapshot.eulaState)}</dd><dt>Áudio</dt><dd>Não verificado</dd></dl>
    {snapshot.tcpAddress && <p>Endereço de jogo do Host jogador: <code>{snapshot.tcpAddress}</code></p>}
    {snapshot.negotiatedDatagramBytes && <p>Capacidade negociada por datagrama original: {snapshot.negotiatedDatagramBytes} bytes. Datagramas originais aprovados: até 4096 bytes.</p>}
    {snapshot.runtimeProfile && <p>Runtime original: {voiceRuntimes[snapshot.runtimeProfile]}</p>}
    {canPrepare && <fieldset disabled={disabled || pending}><legend>Perfil separado</legend>
      <label><input type="radio" name="native-voice-profile" checked={profile === "simpleVoiceChat"} onChange={() => setProfile("simpleVoiceChat")} />Simple Voice Chat 2.6.22</label>
      <label><input disabled={runtime === "1.12.2-forge-14.23.5.2860"} type="radio" name="native-voice-profile" checked={profile === "plasmoVoice"} onChange={() => setProfile("plasmoVoice")} />Plasmo Voice 2.1.17</label>
      <label>Runtime do servidor<select value={runtime} onChange={event => { const next = event.target.value as VoiceRuntime; setRuntime(next); if (next === "1.12.2-forge-14.23.5.2860") setProfile("simpleVoiceChat"); }}>{Object.entries(voiceRuntimes).map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select></label>
      {runtime === "1.12.2-forge-14.23.5.2860" && <p>Plasmo Voice indisponível neste par: não existe artefato oficial compatível.</p>}
      <p>O preparo baixa apenas os arquivos oficiais fixados deste runtime. Forge exige uma etapa própria para autorizar seu instalador; EULA e execução do servidor são revisadas depois.</p>
      <button type="button" onClick={() => void execute({ action: "prepareRuntime", profile, runtimeProfile: runtime })}>{snapshot.phase === "failed" ? "Tentar novo preparo" : "Revisar preparo e anúncio"}</button>
    </fieldset>}
    {snapshot.phase === "socialSource" && <section>
      <p>Após encerrar a tentativa Social e recolher seus recursos, é possível revisar o mesmo servidor sem reiniciar Java. O prazo original continua valendo.</p>
      <button type="button" disabled={disabled || pending || !snapshot.socialRevalidationAvailable} onClick={() => void execute({ action: "reviewSocialSource" })}>Revisar servidor atual para nova sessão</button>
      {!snapshot.socialRevalidationAvailable && <p>Aguarde o encerramento completo da tentativa Social antes de revalidar a origem.</p>}
    </section>}
    {review?.kind === "socialSource" && <button type="button" disabled={disabled || pending || snapshot.phase !== "socialSourceReviewed"} onClick={() => void execute({ action: "cancelSocialSourceReview" })}>Cancelar revalidação</button>}
    {snapshot.phase === "ready" && <p>Para jogar com amigos ou abrir uma sessão pública, selecione esta fonte nativa no painel Social. A revisão Social inclui a vaga do Host jogador e entrega os convites automaticamente. Se esta origem foi revalidada, encerre o anúncio anterior e publique um novo antes de prepará-la no Social.</p>}
    {snapshot.manualLabAvailable && snapshot.phase === "ready" && <details><summary>Laboratório local com convites manuais</summary><fieldset disabled={disabled || pending}><legend>Participantes do Host local</legend>
      <label>Identidades públicas dos convidados, uma por linha<textarea value={guests} onChange={event => setGuests(event.target.value)} maxLength={1024} /></label>
      <label><input type="checkbox" checked={localPlayer} onChange={event => setLocalPlayer(event.target.checked)} />Incluir o Host jogador como participante e abrir sua ponte local</label>
      <p>{count} de 8 vagas. Mínimo de 2 participantes neste laboratório. Cada convidado conserva sua própria identidade.</p>
      <button type="button" disabled={count < 2 || count > 8 || new Set(guestIds).size !== guestIds.length} onClick={() => void execute({ action: "reviewHost", guests: guestIds, localPlayer })}>Revisar Host e origem</button>
    </fieldset></details>}
    {canPrepare && <button type="button" disabled={disabled || pending} onClick={() => void execute({ action: "inspectRecovery" })}>Localizar recibos de sessões anteriores</button>}
    {snapshot.recoverableCopies.length > 0 && <ul>{snapshot.recoverableCopies.map(copy => <li key={copy.bundleId}>{copy.profile === "simpleVoiceChat" ? "Simple Voice Chat" : "Plasmo Voice"} · {voiceRuntimes[copy.runtimeProfile]} · <code>{copy.bundleId}</code>
      <p>Pendente: {[copy.eulaRecoveryPending && "EULA", copy.announcementRecoveryPending && "anúncio de voz"].filter(Boolean).join(" e ")}.</p>
      <button type="button" disabled={disabled || pending || recoveryPending || !terminal} onClick={() => void execute({ action: "resumeRecovery", bundleId: copy.bundleId, receiptId: copy.receiptId })}>Retomar recuperação</button></li>)}</ul>}
    {review && <fieldset disabled={disabled || pending}><legend>Revisão nativa</legend>
      <p>Antes</p><pre>{review.before}</pre><p>Depois</p><pre>{review.after}</pre>
      {details!.copies.map(copy => <p key={copy}>Cópia dedicada: <code>{copy}</code></p>)}
      <ul>{details!.summary.map((detail, index) => <li key={index}>{detail}</li>)}</ul>
      {!!details!.technical.length && <details><summary>Detalhes técnicos da execução</summary>{details!.technical.map((detail, index) => <pre key={index}>{detail}</pre>)}</details>}
      <p role="status">{!anchored ? "Conferindo o prazo original da revisão…" : reviewRemaining > 0 ? `Prazo restante desta revisão: ${Math.ceil(reviewRemaining / 1000)} s.` : "Esta revisão expirou. Encerre a etapa ou prepare uma nova revisão de recuperação."}</p>
      <label><input type="checkbox" checked={approved && canApprove} disabled={!canApprove} onChange={event => setApproved(event.target.checked)} />Revisei esta alteração e autorizo esta etapa</label>
      <button type="button" disabled={!approved || !canApprove} onClick={() => {
        if (canApprove && deadline.current && performance.now() < deadline.current.at) void execute({ action: approveAction, digest: review.digest });
      }}>Confirmar etapa revisada</button>
    </fieldset>}
    {snapshot.phase === "bound" && <ul>{snapshot.participants.map(participant => <li key={participant.participantId}>
      <code>{participant.publicId}</code> {participant.localPlayer ? "Host jogador · uma vaga" : <button type="button" disabled={disabled || pending} onClick={() => void execute({ action: "exportInvitation", participantId: participant.participantId })}>Exibir convite deste participante</button>}
    </li>)}</ul>}
    {invitation && <label>Convite individual<textarea readOnly value={invitation} aria-label="Convite de voz individual" /></label>}
    {snapshot.phase !== "idle" && <button type="button" disabled={disabled || snapshot.phase === "stopping"} onClick={() => void execute({ action: "stop" })}>Encerrar transporte e dedicado próprios</button>}
    {snapshot.eulaRecoveryAvailable && terminal && <fieldset disabled={disabled || pending}><legend>Recuperar EULA</legend>
      <p>Restaurar o EULA original desta cópia requer uma revisão própria, com Java encerrado. Isso não restaura o anúncio de voz nem autoriza outra execução.</p>
      <button type="button" onClick={() => void execute({ action: "prepareEulaRestore" })}>Revisar recuperação do EULA</button>
    </fieldset>}
    {snapshot.recoveryPending && terminal && <fieldset disabled={disabled || pending}><legend>Recuperar anúncio de voz</legend>
      <p>O recibo durável continua disponível. Java deve estar encerrado; a restauração conserva outros arquivos.</p>
      <button type="button" onClick={() => void execute({ action: "prepareRestore", reconciled: false })}>Revisar restauração estrita</button>
      <button type="button" onClick={() => void execute({ action: "prepareRestore", reconciled: true })}>Revisar somente chaves de anúncio</button>
    </fieldset>}
    {snapshot.phase === "failed" && <p role="status">A operação terminou. {recoveryPending ? "Conclua as recuperações indicadas antes de preparar outra sessão." : "Você pode revisar um novo preparo acima."}</p>}
    {pending && <p role="status">A operação continua sob responsabilidade nativa, inclusive se esta tela for fechada.</p>}
    {error && <p role="alert">{error}</p>}
    {snapshot.errorCode && !error && <p role="alert">{nativeVoiceError(snapshot.errorCode)}</p>}
  </details>;
}
