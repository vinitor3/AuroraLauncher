import { invoke, isTauri } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { nativeVoiceError, nativeVoiceSnapshotFrom, voicePhase, type NativeVoiceSnapshot, type VoiceProfile } from "./types";

/** A native owned dedicated session; this panel holds presentation only. */
export function NativeVoicePanel({ disabled = false }: { disabled?: boolean }) {
  const [snapshot, setSnapshot] = useState<NativeVoiceSnapshot>();
  const observed = useRef<NativeVoiceSnapshot | undefined>(undefined);
  const [profile, setProfile] = useState<VoiceProfile>("simpleVoiceChat");
  const [guests, setGuests] = useState("");
  const [localPlayer, setLocalPlayer] = useState(true);
  const [pending, setPending] = useState(false);
  const inFlight = useRef(false);
  const [approved, setApproved] = useState(false);
  const [error, setError] = useState("");
  const [invitation, setInvitation] = useState("");
  function accept(value: unknown) {
    const next = nativeVoiceSnapshotFrom(value);
    if (observed.current && next.revision < observed.current.revision) return;
    if (observed.current?.review?.digest !== next.review?.digest) setApproved(false);
    observed.current = next; setSnapshot(next);
  }
  useEffect(() => {
    if (!isTauri()) return;
    let closed = false; let polling = false;
    async function poll() {
      if (closed || polling || document.hidden) return;
      polling = true;
      try { const value = await invoke("native_voice_snapshot"); if (!closed) accept(value); }
      catch { if (!closed) setError("Não foi possível observar a sessão nativa de voz."); }
      finally { polling = false; }
    }
    const visible = () => { if (!document.hidden) void poll(); };
    void poll(); const timer = window.setInterval(() => void poll(), 1000);
    document.addEventListener("visibilitychange", visible);
    return () => { closed = true; window.clearInterval(timer); document.removeEventListener("visibilitychange", visible); };
  }, []);
  useEffect(() => { if (disabled) { setApproved(false); setInvitation(""); } }, [disabled]);
  async function execute(request: Record<string, unknown>) {
    const stop = request.action === "stop";
    if (disabled || (inFlight.current && !stop)) return;
    if (!stop) { inFlight.current = true; setPending(true); }
    setError(""); setInvitation("");
    try {
      const value = nativeVoiceSnapshotFrom(await invoke("native_voice_execute", { request }));
      accept(value); if (value.exportedInvitation) setInvitation(value.exportedInvitation);
    } catch (cause) { setError(nativeVoiceError(cause)); }
    finally { if (!stop) { inFlight.current = false; setPending(false); } }
  }
  if (!snapshot?.available) return null;
  const review = snapshot.review;
  const canPrepare = ["idle", "stopped", "restored", "reviewExpired"].includes(snapshot.phase) && !snapshot.recoveryPending;
  const approveAction = review && { files: "approveFiles", execution: "approveExecution", host: "approveHost", restore: "approveRestore" }[review.kind];
  const guestIds = guests.split(/\s+/).filter(Boolean);
  const count = guestIds.length + Number(localPlayer);
  return <details className="connect-native-voice">
    <summary>Voz dos mods · sessão dedicada experimental</summary>
    <p>Simple Voice Chat e Plasmo Voice usam seus próprios mods. Este fluxo cria uma cópia dedicada local, anuncia a ponte 127.0.0.1:35554 e mantém a recuperação do anúncio. Áudio e entrada no mundo precisam de validação manual.</p>
    <dl><dt>Sessão</dt><dd>{voicePhase[snapshot.phase]}</dd><dt>UDP</dt><dd>{snapshot.udpState}</dd><dt>EULA</dt><dd>{snapshot.eulaState}</dd><dt>Áudio</dt><dd>Não verificado</dd></dl>
    {snapshot.tcpAddress && <p>Endereço de jogo do Host jogador: <code>{snapshot.tcpAddress}</code></p>}
    {snapshot.negotiatedDatagramBytes && <p>Capacidade negociada por datagrama original: {snapshot.negotiatedDatagramBytes} bytes. Datagramas originais aprovados: até 4096 bytes.</p>}
    {canPrepare && <fieldset disabled={disabled || pending}><legend>Perfil separado</legend>
      <label><input type="radio" name="native-voice-profile" checked={profile === "simpleVoiceChat"} onChange={() => setProfile("simpleVoiceChat")} />Simple Voice Chat 2.6.22</label>
      <label><input type="radio" name="native-voice-profile" checked={profile === "plasmoVoice"} onChange={() => setProfile("plasmoVoice")} />Plasmo Voice 2.1.17</label>
      <p>O primeiro preparo pode baixar Minecraft, Fabric, o mod escolhido e Java 21 para o cache do launcher.</p>
      <button type="button" onClick={() => void execute({ action: "prepare", profile })}>Revisar cópia e anúncio</button>
    </fieldset>}
    {snapshot.phase === "ready" && <p>Para jogar com amigos ou abrir uma sessão pública, selecione esta fonte nativa no painel Social. A revisão Social inclui a vaga do Host jogador e entrega os convites automaticamente.</p>}
    {snapshot.manualLabAvailable && snapshot.phase === "ready" && <details><summary>Laboratório local com convites manuais</summary><fieldset disabled={disabled || pending}><legend>Participantes do Host local</legend>
      <label>Identidades públicas dos convidados, uma por linha<textarea value={guests} onChange={event => setGuests(event.target.value)} maxLength={1024} /></label>
      <label><input type="checkbox" checked={localPlayer} onChange={event => setLocalPlayer(event.target.checked)} />Incluir o Host jogador como participante e abrir sua ponte local</label>
      <p>{count} de 8 vagas. Mínimo de 2 participantes neste laboratório. Cada convidado conserva sua própria identidade.</p>
      <button type="button" disabled={count < 2 || count > 8 || new Set(guestIds).size !== guestIds.length} onClick={() => void execute({ action: "reviewHost", guests: guestIds, localPlayer })}>Revisar Host e origem</button>
    </fieldset></details>}
    {canPrepare && <button type="button" disabled={disabled || pending} onClick={() => void execute({ action: "inspectRecovery" })}>Localizar recibos de sessões anteriores</button>}
    {snapshot.recoverableCopies.length > 0 && <ul>{snapshot.recoverableCopies.map(copy => <li key={copy.receiptId}>{copy.profile === "simpleVoiceChat" ? "Simple Voice Chat" : "Plasmo Voice"} · <code>{copy.bundleId}</code> <button type="button" disabled={disabled || pending || snapshot.recoveryPending} onClick={() => void execute({ action: "resumeRecovery", bundleId: copy.bundleId, receiptId: copy.receiptId })}>Retomar recuperação</button></li>)}</ul>}
    {review && <fieldset disabled={disabled || pending}><legend>Revisão nativa</legend>
      <p>Antes</p><pre>{review.before}</pre><p>Depois</p><pre>{review.after}</pre>
      <ul>{review.details.map((detail, index) => <li key={index}>{detail}</li>)}</ul>
      <label><input type="checkbox" checked={approved} onChange={event => setApproved(event.target.checked)} />Revisei esta alteração e autorizo esta etapa</label>
      <button type="button" disabled={!approved} onClick={() => void execute({ action: approveAction, digest: review.digest })}>Confirmar etapa revisada</button>
    </fieldset>}
    {snapshot.phase === "bound" && <ul>{snapshot.participants.map(participant => <li key={participant.participantId}>
      <code>{participant.publicId}</code> {participant.localPlayer ? "Host jogador · uma vaga" : <button type="button" disabled={disabled || pending} onClick={() => void execute({ action: "exportInvitation", participantId: participant.participantId })}>Exibir convite deste participante</button>}
    </li>)}</ul>}
    {invitation && <label>Convite individual<textarea readOnly value={invitation} aria-label="Convite de voz individual" /></label>}
    {snapshot.phase !== "idle" && <button type="button" disabled={disabled || snapshot.phase === "stopping"} onClick={() => void execute({ action: "stop" })}>Encerrar transporte e dedicado próprios</button>}
    {snapshot.recoveryPending && ["stopped", "restoreReviewed", "reviewExpired"].includes(snapshot.phase) && <fieldset disabled={disabled || pending}><legend>Recuperar anúncio</legend>
      <p>O recibo durável continua disponível. Java deve estar encerrado; a restauração conserva outros arquivos.</p>
      <button type="button" onClick={() => void execute({ action: "prepareRestore", reconciled: false })}>Revisar restauração estrita</button>
      <button type="button" onClick={() => void execute({ action: "prepareRestore", reconciled: true })}>Revisar somente chaves de anúncio</button>
    </fieldset>}
    {pending && <p role="status">A operação continua sob responsabilidade nativa, inclusive se esta tela for fechada.</p>}
    {error && <p role="alert">{error}</p>}
    {snapshot.errorCode && !error && <p role="alert">{nativeVoiceError(snapshot.errorCode)}</p>}
  </details>;
}
