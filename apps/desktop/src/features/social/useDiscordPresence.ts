import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useMemo, useRef, useState } from "react";
import { socialSessionId } from "./connect-types";
import type { SocialConnectState } from "./connect-controller";
import { discordActivity, validDiscordId, type DiscordActivity, type DiscordControls, type DiscordGame, type DiscordRequest, type DiscordSettings } from "./discordModel";

const off: DiscordSettings = { enabled: false, modes: {} };
const key = (uid: string) => `aurora.discord.v1.${uid}`;
function read(uid: string) {
  try {
    const v = JSON.parse(localStorage.getItem(key(uid)) ?? "null");
    if (v === null) return { enabled: true, modes: {} } as DiscordSettings;
    if (typeof v?.enabled === "boolean") {
      const modes = Object.fromEntries(Object.entries(v.modes ?? {}).filter(([id, mode]) => id.length <= 128 && (mode === "aurora" || mode === "modpack")));
      return { enabled: v.enabled, modes } as DiscordSettings;
    }
  } catch { /* Retain privacy when a previous preference cannot be read. */ }
  return off;
}
// Orders clear(RETIRED), set(NEW) across React unmounts and rapid account changes.
let queue = Promise.resolve();
function ordered(action: () => Promise<void>) { queue = queue.then(action).catch(() => {}); }
type Session = { uid: string; scope: string; id: string; ready: boolean };
const statuses: Record<string, string> = {
  starting: "Conectando ao Discord…", active: "Atividade confirmada pelo Discord.",
  presenceOnly: "Atividade confirmada. Convites do Discord indisponíveis.",
  waiting: "Aguardando o aplicativo Discord neste computador.",
  disconnected: "O Discord recusou a conexão. Confira a exibição de atividade nas configurações do Discord.",
  unavailable: "Integração indisponível neste executável.",
};
export function useDiscordPresence(uid: string | undefined, games: readonly DiscordGame[], state: SocialConnectState | undefined, reveal: () => void): DiscordControls | undefined {
  const [edit, setEdit] = useState<{ uid: string; value: DiscordSettings }>();
  const [defaultId, setDefault] = useState("");
  const [event, setEvent] = useState<{ scope: string; status: string; sessionId: string | null }>();
  const [requests, setRequests] = useState<DiscordRequest[]>([]);
  const stored = useMemo(() => uid ? read(uid) : off, [uid]);
  const settings = edit?.uid === uid && edit ? edit.value : stored;
  const id = defaultId;
  const playing = games.length > 0;
  const running = JSON.stringify(games.map(game => [game.instanceId, game.processId, Object.hasOwn(settings.modes, game.instanceId) ? settings.modes[game.instanceId] : "auto"]).sort());
  const [inspection, setInspection] = useState<{ key: string; yield: boolean }>();
  const inspectKey = JSON.stringify([uid, running]);
  const yieldToPack = playing && (inspection?.key !== inspectKey || inspection.yield);
  const binding = useRef<Session | undefined>(undefined);
  const activation = useRef<{ scope: string; id: string } | undefined>(undefined);
  const activity: DiscordActivity = yieldToPack ? { playing, hidden: true, party: null } : discordActivity(playing, state, Date.now());
  const activityKey = JSON.stringify(activity);
  const value = useRef<DiscordActivity>(activity);
  value.current = activity;
  const onReveal = useRef(reveal); onReveal.current = reveal;
  const desktop = isTauri();
  useEffect(() => {
    if (!desktop || !uid || !settings.enabled) return;
    let active = true;
    void invoke<string | null>("discord_default_application_id").then(id => { if (active && id && validDiscordId(id)) setDefault(id); }).catch(() => {});
    return () => { active = false; };
  }, [desktop, uid, settings.enabled]);
  useEffect(() => {
    if (!desktop || !uid || !settings.enabled || !id || !playing) return;
    let active = true;
    const entries: [string, number, string][] = JSON.parse(running);
    void Promise.all(entries.map(async ([instanceId, , mode]) => {
      if (mode !== "auto") return mode === "modpack";
      try { return await invoke<boolean>("discord_instance_has_presence_mod", { id: instanceId }) !== false; }
      catch { return true; }
    })).then(result => { if (active) setInspection({ key: inspectKey, yield: result.some(Boolean) }); });
    return () => { active = false; };
  }, [desktop, uid, settings.enabled, id, playing, running, inspectKey]);
  function send(session: Session) {
    ordered(async () => {
      if (binding.current !== session || !session.ready) return;
      try { await invoke("discord_set_presence", { scope: session.scope, applicationId: session.id, activity: value.current }); }
      catch { if (binding.current === session) setEvent({ scope: session.scope, status: "unavailable", sessionId: null }); }
    });
  }
  useEffect(() => {
    if (!desktop || !uid || !settings.enabled || !validDiscordId(id)) return;
    const session: Session = { uid, scope: crypto.randomUUID(), id, ready: false };
    const { scope } = session;
    binding.current = session; setEvent({ scope, status: "starting", sessionId: null });
    const disposers: UnlistenFn[] = [];
    const own = (dispose: UnlistenFn) => { if (binding.current === session) disposers.push(dispose); else dispose(); };
    void (async () => {
      own(await listen<{ scope: string; status: string }>("aurora-discord-status", ({ payload }) => {
        if (binding.current === session && payload.scope === scope && Object.hasOwn(statuses, payload.status))
          setEvent(prev => ({ scope, status: payload.status, sessionId: prev?.scope === scope ? prev.sessionId : null }));
      }));
      if (binding.current !== session) return;
      own(await listen<{ scope: string; sessionId: string }>("aurora-discord-join", ({ payload }) => {
        if (binding.current !== session || payload.scope !== scope || !socialSessionId(payload.sessionId)) return;
        activation.current = { scope, id: payload.sessionId };
        setEvent(prev => ({ scope, status: prev?.status ?? "active", sessionId: payload.sessionId })); onReveal.current();
      }));
      if (binding.current !== session) return;
      own(await listen<DiscordRequest>("aurora-discord-request", ({ payload }) => {
        if (binding.current !== session || payload.scope !== scope || !validDiscordId(payload.userId) || !socialSessionId(payload.sessionId)
          || value.current.party?.sessionId !== payload.sessionId || typeof payload.name !== "string" || payload.name.length > 32) return;
        setRequests(prev => [...prev.filter(r => r.scope === scope && r.sessionId === payload.sessionId && r.until > Date.now() && r.userId !== payload.userId).slice(-7),
          { ...payload, until: Date.now() + 60_000 }]);
        onReveal.current();
      }));
      if (binding.current === session) { session.ready = true; send(session); }
    })().catch(() => { if (binding.current === session) setEvent({ scope, status: "unavailable", sessionId: null }); });
    return () => {
      binding.current = undefined;
      for (const dispose of disposers) dispose();
      ordered(async () => { await invoke("discord_clear_presence", { scope }); });
    };
  }, [uid, desktop, settings.enabled, id]);
  useEffect(() => { if (binding.current?.ready) send(binding.current); }, [activityKey, running]);
  if (!uid || !desktop) return undefined;
  const visible = settings.enabled && binding.current?.uid === uid && binding.current.id === id && event?.scope === binding.current.scope ? event : undefined;
  return { settings, defaultId, status: !settings.enabled ? "Atividade desativada." : !defaultId
    ? "Aguardando configuração oficial do Aurora. Os links de convite do Aurora estão disponíveis."
    : yieldToPack ? "O modpack controla a atividade enquanto o jogo está aberto. Você pode compartilhar o link de convite do Aurora."
    : statuses[visible?.status ?? "starting"], sessionId: visible?.sessionId ?? null, fromDiscord: true,
    requests: requests.filter(r => !!visible && r.scope === visible.scope && r.sessionId === activity.party?.sessionId && r.until > Date.now()),
    async reply(request, accept) {
      if (!visible || visible.scope !== request.scope || binding.current?.scope !== request.scope || value.current.party?.sessionId !== request.sessionId || request.until <= Date.now()) return false;
      const { scope, sessionId, userId } = request;
      try { await invoke("discord_reply_to_request", { scope, sessionId, userId, accept }); return true; }
      catch { return false; }
      finally { setRequests(prev => prev.filter(r => r !== request)); }
    },
    configure(next) {
      setEdit({ uid, value: next });
      try { localStorage.setItem(key(uid), JSON.stringify(next)); } catch { /* In-memory preference remains scoped to this account. */ }
    },
    claimActivation(sessionId) {
      if (!visible || activation.current?.scope !== visible.scope || activation.current.id !== sessionId) return false;
      activation.current = undefined; return true;
    },
    dismiss() { activation.current = undefined; setEvent(prev => prev ? { ...prev, sessionId: null } : undefined); },
  };
}
