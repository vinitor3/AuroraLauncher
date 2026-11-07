import { snapshotFrom, type ConnectSnapshot } from "../connect/types";
import { socialConnectCapabilitiesFrom, socialConnectError, socialReservationFrom, socialSessionId, type SocialConnectCapabilities } from "./connect-types";

export type SocialConnectState = { available: boolean; ownedGameTcpDiscovery?: boolean; pending: boolean; error: string; snapshot?: ConnectSnapshot };
export const emptySocialConnectState = (): SocialConnectState => ({ available: false, pending: false, error: "" });
type AuthUser = { uid: string; getIdToken(): Promise<string> };
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
    if (alive) timer = setTimeout(() => { void poll(); }, 500);
  }
  async function start() {
    const generation = revision;
    try {
      check(generation);
      const value = await options.invoke("connect_social_capabilities");
      check(generation); capabilities = socialConnectCapabilitiesFrom(value);
      emit({ available: capabilities.available, ownedGameTcpDiscovery: capabilities.ownedGameTcpDiscovery === true });
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
  async function reserve(role: "host" | "guest", sessionId: string, visibility?: "public" | "friends", selectionId?: string) {
    if (!socialSessionId(sessionId)) { emit({ error: socialConnectError("connect_social_failed") }); return; }
    if (selectionId !== undefined && (!socialSessionId(selectionId) || capabilities?.ownedGameTcpDiscovery !== true)) {
      emit({ error: socialConnectError("connect_social_unavailable") }); return;
    }
    await perform(async generation => {
      let reserved: { operationId: string; generation: number } | undefined;
      try {
        check(generation);
        reserved = socialReservationFrom(await options.invoke(selectionId ? "connect_social_reserve_discovered" : "connect_social_reserve", selectionId ? {
          request: { accountEpoch: capabilities!.accountEpoch, accountId: options.accountId, sessionId, visibility }, selectionId,
        } : { request: {
          accountEpoch: capabilities!.accountEpoch, accountId: options.accountId, sessionId, role,
          ...(role === "host" ? { gameTarget: "127.0.0.1:43001", visibility } : {}),
        } }));
        check(generation);
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
    join: (sessionId: string) => reserve("guest", sessionId),
    prepareHost: (sessionId: string, visibility: "public" | "friends") => reserve("host", sessionId, visibility),
    prepareHostDiscovered: (sessionId: string, visibility: "public" | "friends", selectionId: string) => reserve("host", sessionId, visibility, selectionId),
    confirmHost: () => perform(async generation => {
      const snapshot = state.snapshot, social = snapshot?.social;
      if (!snapshot?.operationId || social?.stage !== "hostReview" || !social.digest || !social.expiresAtMs || social.expiresAtMs <= Date.now()) throw "connect_plan_expired";
      const value = await options.invoke("connect_social_confirm_host", { operationId: snapshot.operationId, generation: snapshot.generation, digest: social.digest });
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
