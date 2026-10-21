import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { connectDate, snapshotFrom, type ConnectSnapshot } from "./types";
import { groupExportFrom, groupReviewFrom, sameGroupParticipant, sameGroupTiming, type GroupParticipant } from "./group-types";
import { FragmentedReviewFields } from "./FragmentedFields";
import { pathObservationText } from "./path-observation-types";

export type GroupAction = "reviewGroup" | "confirmGroup" | "exportGroup" | "revokeGroup";
type Props = {
  snapshot: ConnectSnapshot; supported: boolean; busy: boolean; stale: boolean;
  perform: (action: GroupAction, operation: (current: () => boolean) => Promise<void>) => Promise<void>;
  refresh: (current: () => boolean) => Promise<void>;
  acceptSnapshot: (value: unknown) => void;
  copy: (value: string, current?: () => boolean) => Promise<void>;
};
type Entry = { guestPublicId: string; admissionSeconds: string; sessionSeconds: string };
const newEntry = (): Entry => ({ guestPublicId: "", admissionSeconds: "600", sessionSeconds: "3600" });
const exportable = (p: GroupParticipant, now: number) => !p.consumed && !p.revoked && p.admissionValid && p.sessionValid
  && now < p.sessionTiming.admissionExpiresAtMs && now < p.sessionTiming.sessionExpiresAtMs;

export function GroupHostPanel({ snapshot, supported, busy, stale, perform, refresh, acceptSnapshot, copy }: Props) {
  const group = snapshot.hostGroup!;
  const operationId = snapshot.operationId!;
  const [entries, setEntries] = useState<Entry[]>([newEntry(), newEntry()]);
  const [acknowledged, setAcknowledged] = useState(false);
  const [selectedId, setSelectedId] = useState("");
  const [exported, setExported] = useState("");
  const [validation, setValidation] = useState("");
  const [awaiting, setAwaiting] = useState(false);
  const [now, setNow] = useState(Date.now);
  const live = useRef({ snapshot, selectedId });
  live.current = { snapshot, selectedId };
  const exportContext = useRef<{ operationId: string; generation: number; participantId: string } | undefined>(undefined);
  const reviewProjection = JSON.stringify(group.review);
  const reviewDeadline = group.review?.expiresAtMs;
  const blocked = busy || stale || awaiting || !supported;
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 250);
    return () => window.clearInterval(timer);
  }, []);
  useEffect(() => { setAcknowledged(false); }, [reviewProjection, operationId, snapshot.generation]);
  useEffect(() => { if (reviewDeadline !== undefined && now >= reviewDeadline) setAcknowledged(false); }, [now, reviewDeadline]);
  useEffect(() => { setAwaiting(false); }, [snapshot]);
  useEffect(() => {
    const context = exportContext.current;
    const participant = group.participants.find(item => item.participantId === selectedId);
    if (!context || context.operationId !== operationId || context.generation !== snapshot.generation || context.participantId !== selectedId
      || snapshot.phase !== "hostListening" || !participant || !exportable(participant, now) || stale) {
      setExported(""); exportContext.current = undefined;
    }
  }, [snapshot, group.participants, now, selectedId, operationId, stale]);
  function edit(index: number, field: keyof Entry, value: string) {
    setEntries(previous => previous.map((entry, i) => i === index ? { ...entry, [field]: value } : entry));
    setValidation("");
  }
  function sameContext(value: ConnectSnapshot) {
    return value.operationId === operationId && value.generation >= snapshot.generation && !!value.hostGroup
      && value.hostGroup.hostPublicId === group.hostPublicId && value.hostGroup.mode === group.mode
      && value.hostGroup.participants.length === group.participants.length
      && value.hostGroup.participants.every((item, index) => sameGroupParticipant(item, group.participants[index]));
  }
  function review(event: FormEvent) {
    event.preventDefault();
    if (blocked || snapshot.phase !== "hostPrepared" || !group.preparation || Date.now() >= group.preparation.expiresAtMs) return;
    const participants = entries.map(entry => ({ guestPublicId: entry.guestPublicId.trim(), admissionSeconds: Number(entry.admissionSeconds), sessionSeconds: Number(entry.sessionSeconds) }));
    if (participants.length < 2 || participants.length > 8 || new Set(participants.map(item => item.guestPublicId)).size !== participants.length
      || participants.some(item => !/^[a-f0-9]{64}$/.test(item.guestPublicId) || item.guestPublicId === group.hostPublicId
        || !Number.isInteger(item.admissionSeconds) || item.admissionSeconds < 1 || item.admissionSeconds > 600
        || !Number.isInteger(item.sessionSeconds) || item.sessionSeconds < item.admissionSeconds || item.sessionSeconds > 3600)) {
      setValidation("Informe de dois a oito códigos públicos diferentes e prazos inteiros válidos para cada participante."); return;
    }
    setAcknowledged(false); setAwaiting(true);
    void perform("reviewGroup", async current => {
      const value = groupReviewFrom(await invoke<unknown>("connect_review_session_group_host", { operationId, participants }));
      if (!current()) return;
      if (value.operationId !== operationId || value.hostPublicId !== group.hostPublicId || value.mode !== group.mode
        || value.participants.length !== participants.length || value.participants.some((item, index) => item.guestPublicId !== participants[index].guestPublicId
          || item.sessionTiming.admissionSeconds !== participants[index].admissionSeconds || item.sessionTiming.sessionSeconds !== participants[index].sessionSeconds)) throw new Error("Wrong group review");
      await refresh(current);
    });
  }
  function confirm() {
    const reviewed = group.review;
    if (blocked || snapshot.phase !== "reviewed" || !reviewed || !acknowledged) return;
    if (Date.now() >= reviewed.expiresAtMs) { setAcknowledged(false); setNow(Date.now()); return; }
    setAcknowledged(false); setAwaiting(true);
    void perform("confirmGroup", async current => {
      const value = snapshotFrom(await invoke<unknown>("connect_confirm_session_group_host", { operationId, digest: reviewed.digest }));
      if (!current()) return;
      if (!sameContext(value)) throw new Error("Wrong group confirmation");
      acceptSnapshot(value);
    });
  }
  function exportInvitation() {
    const participant = group.participants.find(item => item.participantId === selectedId);
    if (blocked || snapshot.phase !== "hostListening" || !participant || !exportable(participant, Date.now())) return;
    setExported(""); exportContext.current = undefined;
    const baselineGeneration = snapshot.generation;
    void perform("exportGroup", async current => {
      const value = groupExportFrom(await invoke<unknown>("connect_export_session_group_invitation", { operationId, participantId: participant.participantId }));
      const stillCurrent = () => {
        const active = live.current.snapshot;
        const row = active.hostGroup?.participants.find(item => item.participantId === participant.participantId);
        return current() && active.operationId === operationId && active.generation === baselineGeneration && active.phase === "hostListening"
          && live.current.selectedId === participant.participantId && !!row && exportable(row, Date.now());
      };
      if (!stillCurrent()) return;
      if (value.operationId !== operationId || value.participantId !== participant.participantId || value.hostPublicId !== group.hostPublicId
        || value.invitationDigest !== participant.invitationDigest || !sameGroupTiming(value.sessionTiming, participant.sessionTiming)) throw new Error("Wrong participant export");
      exportContext.current = { operationId, generation: baselineGeneration, participantId: participant.participantId };
      setExported(value.invitation);
      await copy(value.invitation, stillCurrent);
    });
  }
  function revoke(participant: GroupParticipant) {
    if (blocked || snapshot.phase !== "hostListening" || participant.revoked || !participant.sessionValid) return;
    setExported(""); exportContext.current = undefined; setAwaiting(true);
    void perform("revokeGroup", async current => {
      const value = snapshotFrom(await invoke<unknown>("connect_revoke_session_group_participant", { operationId, participantId: participant.participantId, expectedGeneration: snapshot.generation }));
      if (!current()) return;
      if (!sameContext(value)) throw new Error("Wrong group revocation");
      acceptSnapshot(value);
    });
  }
  return <section className="connect-card connect-group" aria-label="Grupo de participantes">
    <h3>Grupo TCP e UDP · até oito participantes</h3>
    <p>Cada pessoa recebe um convite próprio, de uso único. Os prazos e a revogação são individuais. Conexão autenticada não confirma entrada no Minecraft ou áudio.</p>
    {!supported && <p role="status">As ações de grupo não estão disponíveis. Você ainda pode encerrar Connect.</p>}
    {awaiting && !busy && <p role="status">Aguardando uma observação válida do grupo. Nenhuma solicitação será repetida automaticamente.</p>}
    {validation && <p className="connect-error" role="alert">{validation}</p>}
    {group.preparation && <>
      <label className="connect-output">Código público do anfitrião do grupo<textarea readOnly value={group.hostPublicId} /></label>
      <button type="button" className="secondary" disabled={blocked} onClick={() => { if (!blocked) void copy(group.hostPublicId); }}>Copiar código do grupo</button>
      <p>Envie esse código a cada convidado. Eles devem escolher Sessão TCP e UDP e devolver seu código público. Preparação válida até {connectDate(group.preparation.expiresAtMs)}.</p>
      <form className="connect-form" onSubmit={review}>
        {entries.map((entry, index) => <fieldset className="connect-fields" key={index} disabled={blocked}>
          <legend>Participante {index + 1}</legend>
          <label>Código público do participante {index + 1}<input required maxLength={64} autoComplete="off" spellCheck={false} value={entry.guestPublicId} onChange={event => edit(index, "guestPublicId", event.target.value)} /></label>
          <label>Prazo para entrar do participante {index + 1}, em segundos<input required type="number" min={1} max={600} step={1} value={entry.admissionSeconds} onChange={event => edit(index, "admissionSeconds", event.target.value)} /></label>
          <label>Duração da sessão do participante {index + 1}, em segundos<input required type="number" min={1} max={3600} step={1} value={entry.sessionSeconds} onChange={event => edit(index, "sessionSeconds", event.target.value)} /></label>
          {entries.length > 2 && <button type="button" className="secondary" onClick={() => setEntries(previous => previous.filter((_, i) => i !== index))}>Remover participante {index + 1}</button>}
        </fieldset>)}
        <p>Os dois prazos começam na emissão, durante a revisão. A duração da sessão deve ser igual ou maior que o prazo para entrar.</p>
        <div className="connect-actions"><button type="button" className="secondary" disabled={blocked || entries.length >= 8} onClick={() => setEntries(previous => [...previous, newEntry()])}>Adicionar participante</button>
          <button type="submit" disabled={blocked || now >= group.preparation.expiresAtMs}>Revisar grupo completo</button></div>
      </form>
    </>}
    {group.review && <div className="connect-review">
      <h4>Revise todos os participantes antes de autorizar</h4>
      <dl><dt>Modo</dt><dd>{group.mode === "loopback" ? "Neste computador" : group.mode === "directIpv4" ? "IPv4 direto autorizado" : "IPv6 externo autorizado"}</dd>
        <dt>Endereço inicial</dt><dd><code>{group.review.directAddress}</code></dd>
        <dt>{group.review.targetSource === "ownedInstance" ? "Jogo TCP · origem conferida" : "Jogo TCP · destino manual autorizado"}</dt><dd><code>{group.review.gameTarget}</code></dd>
        {group.review.targetSource === "ownedInstance" && <><dt>Jogo selecionado</dt><dd>Instância {group.review.instanceId}, execução {group.review.runId}. O grupo inteiro encerra se esse processo ou sua porta mudar; o jogo não será encerrado pelo Connect.</dd>
          <dt>Origem UDP</dt><dd>Destino manual, autorizado separadamente. Não foi descoberto nem identificado como mod de voz.</dd></>}
        <FragmentedReviewFields review={group.review.fragmentedUdp} />
        <dt>Autorize o grupo até</dt><dd>{connectDate(group.review.expiresAtMs)}</dd></dl>
      <p>Somente esses destinos e participantes serão autorizados. Depois de confirmar, a lista não pode ser ampliada e os prazos não serão renovados.</p>
      <details><summary>Identificador da revisão do grupo</summary><code>{group.review.digest}</code></details>
    </div>}
    {group.participants.length > 0 && <div className="connect-fields">{group.participants.map((participant, index) => <section className="connect-status" key={participant.participantId} aria-label={`Participante autorizado ${index + 1}`}>
      <h4>Participante {index + 1}</h4><code>{participant.guestPublicId}</code>
      <dl><dt>Entrar até</dt><dd>{connectDate(participant.sessionTiming.admissionExpiresAtMs)}</dd><dt>Encerramento da sessão</dt><dd>{connectDate(participant.sessionTiming.sessionExpiresAtMs)}</dd>
        <dt>Convite</dt><dd>{participant.revoked ? "Revogado" : participant.consumed ? "Ingresso consumido · não reutilizável" : participant.admissionValid && now < participant.sessionTiming.admissionExpiresAtMs ? "Aguardando ingresso" : "Ingresso indisponível"}</dd>
        <dt>Transporte individual</dt><dd>{participant.transportActive ? "Conexão autenticada observada" : "Sem conexão autenticada observada"}</dd>
        {snapshot.phase === "hostListening" && <><dt>Caminho individual</dt><dd>{pathObservationText(stale ? undefined : participant.pathObservation)}</dd></>}</dl>
      {snapshot.phase === "hostListening" && <div className="connect-actions">
        <button type="button" className="secondary" aria-pressed={selectedId === participant.participantId} disabled={blocked || !exportable(participant, now)} onClick={() => { setSelectedId(participant.participantId); setExported(""); exportContext.current = undefined; }}>Selecionar convite {index + 1}</button>
        <button type="button" className="danger" disabled={blocked || participant.revoked || !participant.sessionValid} onClick={() => revoke(participant)}>Revogar participante {index + 1}</button>
      </div>}
    </section>)}</div>}
    {group.review && <>
      <label className="connect-consent"><input type="checkbox" checked={acknowledged} disabled={blocked || now >= group.review.expiresAtMs} onChange={event => setAcknowledged(event.target.checked)} /><span>Conferi todos os participantes, os destinos TCP e UDP, os limites de retorno e os prazos individuais; autorizo este grupo experimental.</span></label>
      <button type="button" disabled={blocked || !acknowledged || now >= group.review.expiresAtMs} onClick={confirm}>Autorizar grupo</button>
    </>}
    {snapshot.phase === "hostListening" && <>
      <p>Revogar encerra somente o transporte do participante escolhido. Compartilhe cada convite somente com a pessoa correspondente.</p>
      <button type="button" disabled={blocked || !group.participants.some(item => item.participantId === selectedId && exportable(item, now))} onClick={exportInvitation}>Copiar convite selecionado</button>
      {exported && <label className="connect-output">Convite do participante selecionado<textarea readOnly value={exported} spellCheck={false} /></label>}
    </>}
  </section>;
}
