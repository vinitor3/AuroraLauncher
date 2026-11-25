import { useEffect, useState } from "react";
import type { SocialService, SocialSession } from "../../shared/social";
import { validDiscordId, type DiscordControls } from "./discordModel";

export function DiscordSettingsPanel({ controls }: { controls: DiscordControls }) {
  const { settings, defaultId } = controls;
  const [id, setId] = useState(settings.applicationId);
  const valid = validDiscordId(id.trim() || defaultId);
  return <details className="social-extra"><summary>Presença no Discord</summary>
    <p>Mostre quando estiver no launcher ou jogando Minecraft. Enquanto hospeda um mundo público com vaga, seus amigos poderão solicitar entrada pelo Discord.</p>
    <label>ID público do aplicativo Discord<input value={id} inputMode="numeric" autoComplete="off" maxLength={20}
      placeholder={defaultId || "Application ID do Discord Developer Portal"} onChange={event => setId(event.target.value)} /></label>
    {!defaultId && <small>Esta edição precisa do Application ID de um aplicativo Discord que você administra.</small>}
    <div className="social-actions"><button type="button" disabled={!valid || settings.enabled && settings.applicationId === id.trim()} onClick={() => controls.configure({ enabled: true, applicationId: id.trim() })}>{settings.enabled ? "Salvar ID" : "Ativar no Discord"}</button>
      {settings.enabled && <button type="button" className="secondary" onClick={() => controls.configure({ ...settings, enabled: false })}>Desativar</button>}</div>
    <p role="status">{controls.status}</p>
  </details>;
}

export function DiscordInvitation({ id, service, now, accountId, disabled, dismiss, onJoin, waiting }: {
  id: string; service: SocialService; now: number; accountId: string; disabled: boolean; dismiss(): void;
  onJoin(session: SocialSession): void; waiting(session: SocialSession): boolean;
}) {
  const [resolved, setResolved] = useState<SocialSession | null>();
  useEffect(() => {
    let active = true;
    void Promise.resolve(service.resolvePublicSession?.(id)).then(s => {
      if (active) setResolved(s?.id === id && s.visibility === "public" && s.hostUid !== accountId ? s : null);
    }).catch(() => { if (active) setResolved(null); });
    return () => { active = false; };
  }, [service, accountId, id]);
  const session = resolved && resolved.expiresAtMs > now ? resolved : null;
  return <section className="social-notice theme-protected" aria-label="Convite do Discord">
    <h3>Convite do Discord</h3>
    {resolved === undefined ? <p>Consultando a sessão pública…</p> : session ? <>
      <strong>{session.title}</strong><p>{session.modpackName} · {session.minecraftVersion} · {session.loader}</p>
      <button type="button" disabled={disabled || !service.capabilities.join || session.joinPolicy === "approval" && !service.organization}
        onClick={() => onJoin(session)}>{session.joinPolicy === "approval" ? waiting(session) ? "Verificar pedido pelo Discord" : "Pedir para entrar pelo Discord" : "Entrar pelo convite do Discord"}</button>
    </> : <p>Esta sessão pública está indisponível para sua conta.</p>}
    <button type="button" className="secondary" onClick={dismiss}>Dispensar convite do Discord</button>
  </section>;
}
