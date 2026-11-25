import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useMemo, useRef, useState } from "react";
import { socialSessionId } from "./connect-types";
import type { SocialConnectState } from "./connect-controller";
import { discordActivity, validDiscordId, type DiscordActivity, type DiscordControls, type DiscordSettings } from "./discordModel";

const off: DiscordSettings = { enabled: false, applicationId: "" };
const key = (uid: string) => `aurora.discord.v1.${uid}`;
function read(uid: string) {
  try {
    const v = JSON.parse(localStorage.getItem(key(uid)) ?? "null");
    if (v && typeof v.enabled === "boolean" && typeof v.applicationId === "string" && (!v.applicationId || validDiscordId(v.applicationId))) return v as DiscordSettings;
  } catch { /* Storage may be unavailable. Consent defaults to disabled. */ }
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
  disconnected: "O Discord encerrou ou recusou a conexão. Confira o ID e a configuração de atividade.",
  unavailable: "Integração indisponível neste executável.",
};
export function useDiscordPresence(uid: string | undefined, playing: boolean, state: SocialConnectState | undefined, reveal: () => void): DiscordControls | undefined {
  const [edit, setEdit] = useState<{ uid: string; value: DiscordSettings }>();
  const [defaultId, setDefault] = useState("");
  const [event, setEvent] = useState<{ scope: string; status: string; sessionId: string | null }>();
  const stored = useMemo(() => uid ? read(uid) : off, [uid]);
  const settings = edit?.uid === uid && edit ? edit.value : stored;
  const id = settings.applicationId || defaultId;
  const binding = useRef<Session | undefined>(undefined);
  const activity = discordActivity(playing, state, Date.now());
  const activityKey = JSON.stringify(activity);
  const value = useRef<DiscordActivity>(activity);
  value.current = activity;
  const onReveal = useRef(reveal); onReveal.current = reveal;
  const desktop = isTauri();
  useEffect(() => {
    if (!desktop) return;
    let active = true;
    void invoke<string | null>("discord_default_application_id").then(id => { if (active && id && validDiscordId(id)) setDefault(id); }).catch(() => {});
    return () => { active = false; };
  }, [desktop]);
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
        setEvent(prev => ({ scope, status: prev?.status ?? "active", sessionId: payload.sessionId })); onReveal.current();
      }));
      if (binding.current === session) { session.ready = true; send(session); }
    })().catch(() => { if (binding.current === session) setEvent({ scope, status: "unavailable", sessionId: null }); });
    return () => {
      binding.current = undefined;
      for (const dispose of disposers) dispose();
      ordered(async () => { await invoke("discord_clear_presence", { scope }); });
    };
  }, [uid, desktop, settings.enabled, id]);
  useEffect(() => { if (binding.current?.ready) send(binding.current); }, [activityKey]);
  if (!uid || !desktop) return undefined;
  const visible = settings.enabled && binding.current?.uid === uid && binding.current.id === id && event?.scope === binding.current.scope ? event : undefined;
  return { settings, defaultId, status: settings.enabled ? statuses[visible?.status ?? "starting"] : "Atividade desativada.",
    sessionId: visible?.sessionId ?? null,
    configure(next) {
      if (next.enabled && !validDiscordId(next.applicationId || defaultId)) return;
      setEdit({ uid, value: next });
      try { localStorage.setItem(key(uid), JSON.stringify(next)); } catch { /* In-memory preference remains scoped to this account. */ }
    },
    dismiss() { setEvent(prev => prev ? { ...prev, sessionId: null } : undefined); },
  };
}
