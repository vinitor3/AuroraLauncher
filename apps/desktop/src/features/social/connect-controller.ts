import { snapshotFrom, type ConnectSnapshot } from "../connect/types";
import { socialConnectCapabilitiesFrom, socialConnectError, socialReservationFrom, socialSessionId, type SocialConnectCapabilities } from "./connect-types";
import { nativeVoiceSnapshotFrom, type VoiceProfile } from "../connect-native-voice/types";
import { socialAvailabilitiesFrom } from "./availability";

export type SocialConnectState = { available: boolean; maxSessionSeconds?: 60 | 14400; ownedGameTcpDiscovery?: boolean; privateInvitations?: true; nativeVoiceReady?: VoiceProfile; pending: boolean; error: string; snapshot?: ConnectSnapshot };
export const emptySocialConnectState = (): SocialConnectState => ({ available: false, pending: false, error: "" });
type AuthUser = { uid: string; getIdToken(forceRefresh?: boolean): Promise<string> };
type Options = {
  accountId: string; user: AuthUser; currentUser: () => AuthUser | null;
  invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
  publish: (value: SocialConnectState) => void;
};

/** App-owned observer/controller. Native ConnectManager owns resources; navigation
 * never starts or stops them. Tokens cross only this narrow, RAM-only handoff. */
export function createSocialConnectController(options: Options) {
  let state = emptySocialConnectState();
  let alive = true;
  let revision = 0;
  let busy = false;
  let stopping = false;
  let capabilities: SocialConnectCapabilities | undefined;
  let refreshDue = performance.now() + 20 * 60_000;
  let voiceCheckDue = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const accountMatches = () => alive && options.currentUser() === options.user && options.user.uid === options.accountId;
  function emit(update: Partial<SocialConnectState>) {
    if (!alive) return;
    state = { ...state, ...update }; options.publish(state);
  }
  function check(generation: number) {
    if (!accountMatches() || generation !== revision) throw "connect_social_account_changed";
  }
  function accept(value: unknown) {
    const next = snapshotFrom(value);
    if (state.snapshot && next.generation < state.snapshot.generation) return;
    emit({ snapshot: next });
  }
  async function observe() {
    if (!accountMatches()) return;
    const generation = revision;
    try {
      const value = await options.invoke("connect_snapshot");
      check(generation); accept(value);
    } catch (cause) { if (alive && generation === revision) emit({ error: socialConnectError(cause) }); }
  }
  async function poll() {
    if (!alive) return;
    if (!busy) await observe();
    if (!busy && capabilities?.maxSessionSeconds === 14400 && performance.now() >= voiceCheckDue) {
      const generation = revision; voiceCheckDue = performance.now() + 2000;
      try {
        const voice = nativeVoiceSnapshotFrom(await options.invoke("native_voice_snapshot")); check(generation);
        emit({ nativeVoiceReady: voice.available && voice.phase === "ready" && voice.profile !== null ? voice.profile : undefined });
      } catch { if (alive && generation === revision) emit({ nativeVoiceReady: undefined }); }
    }
    if (!busy && capabilities?.maxSessionSeconds === 14400 && performance.now() >= refreshDue && state.snapshot?.operationId
      && !["closed", "failed", "closing"].includes(state.snapshot.phase)) {
      const operationId = state.snapshot.operationId, generation = revision;
      refreshDue = performance.now() + 20 * 60_000;
      try {
        let refreshTimer: ReturnType<typeof setTimeout> | undefined;
        let idToken: string;
        try { idToken = await Promise.race([options.user.getIdToken(true), new Promise<never>((_, reject) => {
          refreshTimer = setTimeout(() => reject("connect_social_auth_failed"), 10_000);
        })]); } finally { if (refreshTimer !== undefined) clearTimeout(refreshTimer); }
        check(generation);
        if (state.snapshot?.operationId !== operationId) throw "connect_stale_operation";
        await options.invoke("connect_social_refresh_authentication", { operationId, accountEpoch: capabilities.accountEpoch, idToken }); check(generation);
      } catch (cause) { if (alive && generation === revision) emit({ error: socialConnectError(cause) }); }
    }
    if (alive) timer = setTimeout(() => { void poll(); }, 500);
  }
  async function start() {
    const generation = revision;
    try {
      check(generation);
      const value = await options.invoke("connect_social_capabilities");
      check(generation); capabilities = socialConnectCapabilitiesFrom(value);
      emit({ available: capabilities.available, maxSessionSeconds: capabilities.maxSessionSeconds, ownedGameTcpDiscovery: capabilities.ownedGameTcpDiscovery === true, privateInvitations: capabilities.privateInvitations });
      await poll();
    } catch (cause) { if (alive && generation === revision) emit({ error: socialConnectError(cause), available: false }); }
  }
  async function perform(work: (generation: number) => Promise<void>, requireAvailable = true) {
    if (busy || !accountMatches()) return;
    if (requireAvailable && !capabilities?.available) { emit({ error: socialConnectError("connect_social_unavailable") }); return; }
    busy = true; const generation = ++revision; emit({ pending: true, error: "" });
    try { await work(generation); }
    catch (cause) { if (alive && generation === revision) emit({ error: socialConnectError(cause) }); }
    finally { if (alive && generation === revision) { busy = false; emit({ pending: false }); await observe(); } }
  }
  async function reserve(role: "host" | "guest", sessionId: string, visibility?: "public" | "friends" | "private", selectionId?: string, nativeVoice?: { hostPlayer: boolean }) {
    if (!socialSessionId(sessionId)) { emit({ error: socialConnectError("connect_social_failed") }); return; }
    if (visibility === "private" && capabilities?.privateInvitations !== true) { emit({ error: socialConnectError("connect_social_unavailable") }); return; }
    if (selectionId !== undefined && (!socialSessionId(selectionId) || capabilities?.ownedGameTcpDiscovery !== true)) {
      emit({ error: socialConnectError("connect_social_unavailable") }); return;
    }
    if (nativeVoice && (role !== "host" || capabilities?.maxSessionSeconds !== 14400 || !state.nativeVoiceReady)) {
      emit({ error: socialConnectError("connect_social_unavailable") }); return;
    }
    await perform(async generation => {
      let reserved: { operationId: string; generation: number } | undefined;
      try {
        check(generation);
        reserved = socialReservationFrom(await options.invoke(nativeVoice ? "native_voice_social_reserve" : selectionId ? "connect_social_reserve_discovered" : "connect_social_reserve", nativeVoice ? {
          request: { accountEpoch: capabilities!.accountEpoch, accountId: options.accountId, sessionId, visibility }, hostPlayer: nativeVoice.hostPlayer,
        } : selectionId ? {
          request: { accountEpoch: capabilities!.accountEpoch, accountId: options.accountId, sessionId, visibility }, selectionId,
        } : { request: {
          accountEpoch: capabilities!.accountEpoch, accountId: options.accountId, sessionId, role,
          ...(role === "host" ? { gameTarget: "127.0.0.1:43001", visibility } : {}),
        } }));
        check(generation);
        if (nativeVoice) emit({ nativeVoiceReady: undefined });
        const pending = snapshotFrom(await options.invoke("connect_snapshot")); check(generation);
        if (pending.operationId !== reserved.operationId || pending.generation !== reserved.generation || pending.social?.sessionId !== sessionId || pending.role !== role) throw "connect_stale_operation";
        accept(pending); // Cancellation stays visible while the SDK is pending.
        // The slot exists before asynchronous Firebase work. Logout invalidates
        // the native account epoch before cleanup, including delayed reserve IPC.
        let tokenTimer: ReturnType<typeof setTimeout> | undefined;
        let idToken: string;
        try { idToken = await Promise.race([options.user.getIdToken(), new Promise<never>((_, reject) => {
          tokenTimer = setTimeout(() => reject("connect_social_auth_failed"), 10_000);
        })]); } finally { if (tokenTimer !== undefined) clearTimeout(tokenTimer); }
        check(generation);
        const value = await options.invoke("connect_social_authenticate", { ...reserved, idToken });
        check(generation); accept(value);
      } catch (cause) {
        if (reserved) {
          try {
            const closed = snapshotFrom(await options.invoke("connect_stop", { operationId: reserved.operationId }));
            if (closed.operationId !== reserved.operationId || !["closed", "failed"].includes(closed.phase) || closed.cleanupState !== "complete") throw "connect_account_cleanup_incomplete";
          } catch (cleanupCause) {
            if (cleanupCause !== "connect_stale_operation") throw "connect_account_cleanup_incomplete";
          }
        }
        throw cause;
      }
    });
  }
  return {
    start,
    supportsPrivateInvitations: () => accountMatches() && capabilities?.available === true && capabilities.privateInvitations === true,
    observeSessions: async (sessionIds: string[]) => {
      const generation = revision;
      check(generation);
      if (capabilities?.maxSessionSeconds !== 14400 || !capabilities.available || sessionIds.length < 1 || sessionIds.length > 8
        || new Set(sessionIds).size !== sessionIds.length || sessionIds.some(id => !socialSessionId(id))) throw "connect_social_unavailable";
      let tokenTimer: ReturnType<typeof setTimeout> | undefined;
      let idToken: string;
      try { idToken = await Promise.race([options.user.getIdToken(), new Promise<never>((_, reject) => {
        tokenTimer = setTimeout(() => reject("connect_social_auth_failed"), 10_000);
      })]); } finally { if (tokenTimer !== undefined) clearTimeout(tokenTimer); }
      check(generation);
      const started = performance.now();
      const value = await options.invoke("connect_social_observe_sessions", { request: { accountEpoch: capabilities.accountEpoch, accountId: options.accountId, sessionIds }, idToken });
      check(generation);
      return socialAvailabilitiesFrom(value, sessionIds, started);
    },
    join: (sessionId: string) => reserve("guest", sessionId),
    prepareHost: (sessionId: string, visibility: "public" | "friends" | "private") => reserve("host", sessionId, visibility),
    prepareHostDiscovered: (sessionId: string, visibility: "public" | "friends" | "private", selectionId: string) => reserve("host", sessionId, visibility, selectionId),
    prepareHostNativeVoice: (sessionId: string, visibility: "public" | "friends" | "private", hostPlayer: boolean) => reserve("host", sessionId, visibility, undefined, { hostPlayer }),
    revokeParticipant: (grantId: string) => perform(async generation => {
      const snapshot = state.snapshot;
      if (capabilities?.maxSessionSeconds !== 14400 || !snapshot?.operationId || snapshot.role !== "host" || snapshot.phase !== "hostListening"
        || !snapshot.social?.participants?.some(person => person.grantId === grantId && person.state !== "removing")) throw "connect_stale_operation";
      const value = await options.invoke("connect_social_revoke_participant", { operationId: snapshot.operationId, accountEpoch: capabilities.accountEpoch, grantId });
      check(generation); accept(value);
    }),
    confirmHost: () => perform(async generation => {
      const snapshot = state.snapshot, social = snapshot?.social;
      if (!snapshot?.operationId || social?.stage !== "hostReview" || !social.digest || !social.expiresAtMs || social.expiresAtMs <= Date.now()) throw "connect_plan_expired";
      const value = await options.invoke("connect_social_confirm_host", { operationId: snapshot.operationId, generation: snapshot.generation, digest: social.digest });
      check(generation); accept(value);
    }),
    confirmGuest: () => perform(async generation => {
      const snapshot = state.snapshot, social = snapshot?.social;
      if (!snapshot?.operationId || snapshot.role !== "guest" || snapshot.phase !== "reviewed" || social?.stage !== "guestReview"
        || social.voiceTransportProfile !== "nativeVoice8V1" || !social.digest || !social.expiresAtMs || social.expiresAtMs <= Date.now()) throw "connect_plan_expired";
      const value = await options.invoke("connect_social_confirm_guest", { operationId: snapshot.operationId, generation: snapshot.generation, digest: social.digest });
      check(generation); accept(value);
    }),
    stop: async () => { if (stopping) return; stopping = true; busy = false; try { await perform(async generation => {
      const snapshot = snapshotFrom(await options.invoke("connect_snapshot")); check(generation);
      if (!snapshot.operationId) { accept(snapshot); return; }
      const value = await options.invoke("connect_stop", { operationId: snapshot.operationId });
      check(generation); accept(value);
    }, false); } finally { stopping = false; } },
    refresh: () => perform(async generation => {
      const value = await options.invoke("connect_snapshot"); check(generation); accept(value);
    }, false),
    invalidate() { alive = false; ++revision; capabilities = undefined; if (timer !== undefined) clearTimeout(timer); },
  };
}
export type SocialConnectController = ReturnType<typeof createSocialConnectController>;
