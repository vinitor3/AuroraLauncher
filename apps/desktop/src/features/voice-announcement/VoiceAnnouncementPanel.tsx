import { useEffect, useState } from "react";
import { useVoiceAnnouncement } from "./VoiceAnnouncementProvider";
import { announcementText, MODS, phaseText, voiceDate, voiceError, type VoiceMod } from "./types";

type Props = { instanceId: string | null; sourceInstanceId: string; busy: boolean; running: boolean; onChanged: () => Promise<void> };
export function VoiceAnnouncementPanel({ instanceId, sourceInstanceId, busy, running, onChanged }: Props) {
  const voice = useVoiceAnnouncement();
  const [selectedMod, setSelectedMod] = useState<VoiceMod>("simple_voice_chat");
  const [consent, setConsent] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const snapshot = voice?.snapshot;
  const review = snapshot?.review;
  const active = !!snapshot?.operationId && snapshot.phase !== "closed";
  const sameSource = !active || snapshot?.instanceId === instanceId || (!instanceId && snapshot?.instanceId === sourceInstanceId);
  const blocked = busy || running || !!voice?.pending || !!snapshot?.busy || !voice?.valid || !!snapshot?.close;
  const canInspect = !!instanceId && !blocked && (!active || (sameSource && snapshot?.voiceMod === selectedMod && ["observed", "failed"].includes(snapshot.phase)));
  const reviewExpiry = voice?.capabilities?.available && sameSource ? review?.expiresAtMs : undefined;

  useEffect(() => { setConsent(false); }, [instanceId, sourceInstanceId, review?.planId, review?.digest]);
  useEffect(() => { if (review && now >= review.expiresAtMs) setConsent(false); }, [now, review]);
  useEffect(() => { if (snapshot?.voiceMod && active) setSelectedMod(snapshot.voiceMod); }, [snapshot?.voiceMod, active]);
  useEffect(() => {
    if (reviewExpiry === undefined) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [reviewExpiry, review?.planId, review?.digest]);
  if (!voice?.capabilities?.available) return null;

  async function act(command: "inspect" | "prepare_apply" | "confirm" | "prepare_restore" | "prepare_restore_reconciled" | "cancel_review", request: Record<string, unknown>) {
    if (!voice || blocked || !sameSource) return;
    setConsent(false);
    try {
      const result = await voice.run(command, request);
      if (["applied", "restored", "needsRecovery"].includes(result.phase)) await onChanged().catch(() => undefined);
    } catch { /* The global caller presents only reviewed native codes. */ }
  }
  const operationId = snapshot?.operationId;
  const restoreReady = snapshot?.phase === "applied" && snapshot.restorationState === "ready";
  return <section className="recovery-point" aria-label="Anúncio de voz experimental" aria-busy={!!snapshot?.busy}>
    <header><h3>Anúncio de voz · experimental</h3><p>Somente esta cópia de teste. Alterar o anúncio não ativa a ponte, não comprova o mod carregado e não valida áudio.</p></header>
    <p>Modo dedicado Aurora: todos os participantes, inclusive o cliente do anfitrião, precisam da ponte local em <code>127.0.0.1:35554</code>.</p>
    {!instanceId && !active && <p>Abra uma cópia de teste pronta para inspecionar um mod.</p>}
    {active && <p>Operação na cópia <strong>{snapshot.instanceId}</strong> · {snapshot.voiceMod ? MODS[snapshot.voiceMod].name : ""}</p>}
    {!sameSource && <p role="status">A operação pertence a outra cópia. Sua seleção atual não muda o recibo.<button className="secondary" onClick={() => snapshot?.instanceId && voice.navigate(snapshot.instanceId, "voice")} type="button">Abrir cópia desta operação</button></p>}
    <label>Mod a inspecionar<select aria-label="Mod de voz a inspecionar" value={selectedMod} disabled={blocked || active} onChange={event => { setSelectedMod(event.target.value as VoiceMod); setConsent(false); }}>
      <option value="simple_voice_chat">Simple Voice Chat 2.6.22</option><option value="plasmo_voice">Plasmo Voice 2.1.17</option>
    </select></label>
    <button disabled={!canInspect} onClick={() => void act("inspect", { instanceId, voiceMod: selectedMod })} type="button">Inspecionar anúncio</button>
    <button className="secondary" disabled={voice.pending} onClick={() => void voice.refresh().catch(() => undefined)} type="button">Atualizar estado do anúncio</button>
    {snapshot && <p role="status">{phaseText[snapshot.phase]}</p>}
    {running && <p>Feche o jogo antes de inspecionar, aplicar ou restaurar a configuração.</p>}
    {snapshot?.lastErrorCode && <p role="alert">{voiceError(snapshot.lastErrorCode)}</p>}
    {sameSource && snapshot?.observation && <div>
      <p><strong>Valor configurado:</strong> {announcementText(snapshot.observation.announcement)}</p>
      <p>Arquivo: <code>{snapshot.observation.configPath}</code></p>
      <details><summary>Versão e hashes verificados</summary><p>{MODS[snapshot.observation.voiceMod].name} {snapshot.observation.artifactVersion}</p><p>Mod: <code>{snapshot.observation.artifactSha256}</code></p><p>Configuração: <code>{snapshot.observation.sourceSha256}</code> · {snapshot.observation.sourceBytes} bytes</p></details>
      {snapshot.phase === "observed" && <button disabled={blocked} onClick={() => void act("prepare_apply", { operationId, generation: snapshot.generation })} type="button">Revisar aplicação na cópia</button>}
    </div>}
    {sameSource && review && <section aria-label="Revisão do anúncio">
      <h4>{review.action === "setAnnouncement" ? "Aplicar anúncio" : review.action === "restoreOriginal" ? "Restaurar arquivo anterior" : "Restaurar anúncio preservando outras edições"}</h4>
      <p><code>{review.configPath}</code></p><p>{review.warning}</p>
      <label>Antes<pre>{review.before || "(vazio)"}</pre></label><label>Depois<pre>{review.after || "(vazio)"}</pre></label>
      <details><summary>Hashes da revisão</summary><p>Atual: <code>{review.sourceSha256}</code></p><p>Previsto: <code>{review.replacementSha256}</code></p><p>Aprovação: <code>{review.digest}</code></p></details>
      <p>Válida até {voiceDate(review.expiresAtMs)}.</p>
      {now >= review.expiresAtMs && <p role="alert">A revisão venceu. Cancele e prepare outra.</p>}
      <label><input aria-label="Autorizo esta alteração do anúncio" type="checkbox" checked={consent} disabled={blocked || now >= review.expiresAtMs} onChange={event => setConsent(event.target.checked)} />Revisei o diff e autorizo somente esta alteração na cópia.</label>
      <button disabled={blocked || !consent || now >= review.expiresAtMs} onClick={() => {
        if (Date.now() < review.expiresAtMs && consent) void act("confirm", { operationId, planId: review.planId, digest: review.digest });
      }} type="button">Confirmar alteração revisada</button>
      <button className="secondary" disabled={blocked} onClick={() => void act("cancel_review", { operationId })} type="button">Cancelar revisão</button>
    </section>}
    {sameSource && restoreReady && <div>
      <p>O recibo existe somente nesta execução do launcher. Cada restauração exige nova revisão; sair não restaura automaticamente.</p>
      <button disabled={blocked} onClick={() => void act("prepare_restore", { operationId })} type="button">Revisar restauração estrita</button>
      <button className="secondary" disabled={blocked} onClick={() => void act("prepare_restore_reconciled", { operationId })} type="button">Revisar restauração preservando outras edições</button>
    </div>}
    {active && snapshot.instanceId && <div>
      <button className="secondary" disabled={voice.pending || !!snapshot.busy || !!snapshot.close} onClick={() => voice.navigate(snapshot.instanceId!, "recovery")} type="button">Abrir Recuperação da cópia</button>
      <button className="secondary" disabled={!!snapshot.close} onClick={() => void voice.beginPanelClose().catch(() => undefined)} type="button">Encerrar operação de anúncio</button>
    </div>}
  </section>;
}
