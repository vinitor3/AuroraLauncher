import { useEffect, useRef, useState } from "react";
import type { SocialService, SocialSession } from "../../shared/social";
import type { DiscordControls, DiscordMode } from "./discordModel";
import { socialSessionId } from "./connect-types";
import { publicInviteId, publicInvitePrefix } from "./publicInvite";

export function DiscordRequests({ controls, now }: { controls: DiscordControls; now: number }) {
  const [busy, setBusy] = useState(false), [result, setResult] = useState("");
  const requests = controls.requests?.filter(r => r.until > now) ?? [];
  if (!requests.length && !result) return null;
  return <section className="social-notice theme-protected" aria-label="Pedidos do Discord">
    {requests.length > 0 && <><h3>Pedidos do Discord</h3><p>Enviar o convite permite abrir a sessão no Aurora. A política de entrada do mundo continua valendo.</p></>}
    {requests.map(request => <div key={request.userId} className="social-friend-row"><strong>{request.name}</strong>
      {[true, false].map(accept => <button key={String(accept)} type="button" disabled={busy} onClick={async () => {
        if (busy) return; setBusy(true); setResult("");
        try { const ok = await controls.reply(request, accept); setResult(ok ? accept ? "Convite confirmado pelo Discord." : "Pedido recusado." : "Pedido expirado ou Discord indisponível."); }
        finally { setBusy(false); }
      }}>{accept ? "Enviar convite no Discord" : "Recusar pedido do Discord"}</button>)}</div>)}
    {result && <small role="status">{result}</small>}
  </section>;
}

export function DiscordSettingsPanel({ controls, instances = {} }: { controls: DiscordControls; instances?: Record<string, string> }) {
  const { settings } = controls;
  const [selected, setSelected] = useState("");
  const instanceId = Object.hasOwn(instances, selected) ? selected : Object.keys(instances)[0];
  return <details className="social-extra"><summary>Presença no Discord</summary>
    <label><input type="checkbox" checked={settings.enabled} onChange={event => controls.configure({ ...settings, enabled: event.target.checked })} /> Mostrar minha atividade no Discord</label>
    <p>Mostra quando o launcher ou Minecraft está aberto. Seu mundo público ganha um convite enquanto estiver hospedado e com vaga. Mundos privados e de amigos ficam no Aurora.</p>
    {instanceId && <><label>Instância<select value={instanceId} onChange={event => setSelected(event.target.value)}>{Object.entries(instances).map(([id, name]) => <option key={id} value={id}>{name}</option>)}</select></label>
      <label>Presença durante o jogo<select value={settings.modes?.[instanceId] ?? "auto"} onChange={event => {
        const modes = { ...settings.modes }; const mode = event.target.value as DiscordMode;
        if (mode === "auto") delete modes[instanceId]; else modes[instanceId] = mode;
        controls.configure({ ...settings, modes });
      }}><option value="auto">Automática</option><option value="modpack">Deixar com o modpack</option><option value="aurora">Mostrar Aurora</option></select></label>
      <small>No automático, o Aurora cede a atividade ao CraftPresence e ao Simple RPC quando a instância é aberta. Para outro mod de presença, escolha Deixar com o modpack.</small></>}
    <p role="status">{controls.status}</p>
  </details>;
}

export function PublicInviteCode({ hostedId, onOpen }: { hostedId?: string; onOpen(id: string): void }) {
  const [draft, setDraft] = useState(""), [notice, setNotice] = useState("");
  const link = hostedId && socialSessionId(hostedId) ? publicInvitePrefix + hostedId : null;
  const candidate = publicInviteId(draft);
  return <section className="social-extra" aria-label="Convite público Aurora"><h3>Convite público Aurora</h3>
    {link && <><label>Seu convite público<input readOnly value={link} onFocus={event => event.target.select()} /></label>
      <button type="button" onClick={() => { if (navigator.clipboard?.writeText) void navigator.clipboard.writeText(link).then(() => setNotice("Convite copiado."), () => setNotice("Selecione e copie o link acima.")); else setNotice("Selecione e copie o link acima."); }}>Copiar link de convite</button></>}
    <form onSubmit={event => { event.preventDefault(); if (candidate) onOpen(candidate); }}>
      <label>Convite recebido<input value={draft} maxLength={128} placeholder="Cole o link ou código Aurora" onChange={event => setDraft(event.target.value)} /></label>
      <button type="submit" disabled={!candidate}>Abrir convite Aurora</button></form>
    <small>Em versões anteriores sem abertura por link, encontre o mundo em Amigos e comunidade ou atualize o launcher.</small>
    {notice && <small role="status">{notice}</small>}
  </section>;
}

export function DiscordInvitation({ id, service, now, accountId, disabled, dismiss, onJoin, waiting, fromDiscord = false, claimActivation, pasted = false }: {
  id: string; service: SocialService; now: number; accountId: string; disabled: boolean; dismiss(): void;
  onJoin(session: SocialSession): void; waiting(session: SocialSession): boolean; fromDiscord?: boolean; claimActivation?(id: string): boolean; pasted?: boolean;
}) {
  const [resolved, setResolved] = useState<SocialSession | null>();
  const attempted = useRef(false);
  const checkUntil = useRef(0);
  const join = useRef(onJoin);
  join.current = onJoin;
  useEffect(() => {
    let active = true;
    void Promise.resolve(service.resolvePublicSession?.(id)).then(s => {
      if (active) setResolved(s?.id === id && s.visibility === "public" && s.hostUid !== accountId ? s : null);
    }).catch(() => { if (active) setResolved(null); });
    return () => { active = false; };
  }, [service, accountId, id]);
  const session = resolved && resolved.expiresAtMs > now ? resolved : null;
  const blocked = disabled || !service.capabilities.join || session?.joinPolicy === "approval" && !service.organization;
  useEffect(() => {
    if (fromDiscord && !attempted.current && !blocked && session && claimActivation?.(id)) {
      attempted.current = true; checkUntil.current = performance.now() + 120_000; join.current(session);
    }
  }, [fromDiscord, blocked, session, claimActivation, id]);
  const pendingApproval = !!session && session.joinPolicy === "approval" && waiting(session);
  useEffect(() => {
    if (!fromDiscord || blocked || !session || !pendingApproval || !attempted.current || performance.now() >= checkUntil.current) return;
    const timer = window.setInterval(() => {
      if (performance.now() >= checkUntil.current) { window.clearInterval(timer); return; }
      if (!document.hidden) join.current(session);
    }, 15_000);
    return () => window.clearInterval(timer);
  }, [fromDiscord, blocked, session, pendingApproval]);
  const title = pasted ? "Convite Aurora" : "Convite do Discord";
  const channel = pasted ? "Aurora" : "Discord";
  const action = session?.joinPolicy === "approval"
    ? (waiting(session) ? "Verificar pedido pelo " : "Pedir para entrar pelo ") + channel
    : pasted ? "Entrar pelo convite Aurora" : "Entrar pelo convite do Discord";
  return <section className="social-notice theme-protected" aria-label={title}>
    <h3>{title}</h3>
    {resolved === undefined ? <p>Consultando a sessão pública…</p> : session ? <>
      <strong>{session.title}</strong><p>{session.modpackName} · {session.minecraftVersion} · {session.loader}</p>
      {fromDiscord && pendingApproval && <p>Aguardando a decisão do anfitrião. O Aurora verifica a aprovação nesta tela por até dois minutos.</p>}
      <button type="button" disabled={blocked} onClick={() => onJoin(session)}>{action}</button>
    </> : <p>Esta sessão pública está indisponível para sua conta.</p>}
    <button type="button" className="secondary" onClick={dismiss}>Dispensar {pasted ? "convite Aurora" : "convite do Discord"}</button>
  </section>;
}
