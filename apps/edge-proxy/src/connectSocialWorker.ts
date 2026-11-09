import { DurableObject } from "cloudflare:workers";
import { routeRelayRequest, ConnectRelaySession } from "./connectRelayWorker.ts";
export { ConnectRelaySession };
import { createSocialBindingAuthority, SocialBindingError, type SocialBindingState } from "./connectSocialBinding.ts";
import { createSocialAdmissionAuthority, type SocialAdmissionState } from "./connectSocialAdmission.ts";
import { createSocialFirebaseDependencies, socialAuthenticate, socialBearer, socialJson, SOCIAL_UUID } from "./connectSocialFirebase.ts";
import { projectSocialPresence } from "./connectSocialPresence.ts";

interface Env {
  FIREBASE_PROJECT_ID: string;
  SOCIAL_SESSIONS: DurableObjectNamespace<SocialSessionAuthority>;
  SOCIAL_ACCOUNTS: DurableObjectNamespace<SocialAccountBudget>;
  RELAY_SESSIONS: DurableObjectNamespace<ConnectRelaySession>;
}
const paths = new Set(["bootstrap", "presence", "identity", "key/challenge", "key/proof", "context", "host/register", "join", "continuity", "host/poll", "host/publish", "host/revoke", "guest/receive", "host/stop", "guest/stop", "relay/ticket"]);
const fail = (code: string): never => { throw new SocialBindingError(code); };
const reply = (data: unknown, status = 200) => Response.json(data, { status, headers: { "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff" } });
function clockRequest(body: unknown): { operationId: string; generation: number; nonce: string } | undefined {
  if (!body || typeof body !== "object" || Array.isArray(body)) return fail("invalid_request");
  const value = body as Record<string, unknown>;
  if (Object.keys(value).length === 0) return undefined;
  if (Object.keys(value).length !== 3 || typeof value.operationId !== "string" || !SOCIAL_UUID.test(value.operationId)
    || typeof value.generation !== "number" || !Number.isSafeInteger(value.generation) || value.generation < 1
    || typeof value.nonce !== "string" || !/^[0-9a-f]{32}$/.test(value.nonce)) return fail("invalid_request");
  return value as { operationId: string; generation: number; nonce: string };
}
type Live = { stored: Stored; binding: ReturnType<typeof createSocialBindingAuthority>; admission: Awaited<ReturnType<typeof createSocialAdmissionAuthority>>; keys: CryptoKeyPair; clock: { wall(): number; monotonic(): number }; active: number; failed: boolean };
type Stored = { sessionId: string; until: number; busy: boolean; clock: { wall: number; monotonic: number }; privateJwk: JsonWebKey; publicJwk: JsonWebKey; binding: SocialBindingState; admission: SocialAdmissionState };

/** Distributed per-account budget, one bounded row and alarm per active account. */
export class SocialAccountBudget extends DurableObject<Env> {
  async admit(sessionId: string): Promise<boolean> {
    const now = Date.now();
    return this.ctx.storage.transaction(async txn => {
      const prior = await txn.get<{ until: number; count: number; sessions: string[] }>("budget");
      const value = prior && prior.until > now ? prior : { until: now + 60_000, count: 0, sessions: [] };
      if (value.count >= 300 || (!value.sessions.includes(sessionId) && value.sessions.length >= 4)) return false;
      value.count++; if (!value.sessions.includes(sessionId)) value.sessions.push(sessionId);
      await txn.put("budget", value); await txn.setAlarm(value.until); return true;
    });
  }
  async admitObservation(sessionId: string): Promise<boolean> {
    const now = Date.now();
    return this.ctx.storage.transaction(async txn => {
      const prior = await txn.get<{ until: number; count: number; sessions: string[] }>("observations");
      const value = prior && prior.until > now ? prior : { until: now + 60_000, count: 0, sessions: [] };
      if (value.count >= 240 || (!value.sessions.includes(sessionId) && value.sessions.length >= 32)) return false;
      value.count++; if (!value.sessions.includes(sessionId)) value.sessions.push(sessionId);
      await txn.put("observations", value); await txn.setAlarm(value.until); return true;
    });
  }
  async alarm() {
    const now = Date.now();
    await this.ctx.storage.transaction(async txn => {
      const remaining: number[] = [];
      for (const name of ["budget", "observations"]) {
        const value = await txn.get<{ until: number }>(name);
        if (value && value.until > now) remaining.push(value.until);
        else await txn.delete(name);
      }
      if (remaining.length) await txn.setAlarm(Math.min(...remaining));
    });
  }
}

/** One session is a coordination atom. No token/request is written to durable storage. */
export class SocialSessionAuthority extends DurableObject<Env> {
  private live?: Live;
  private loading?: Promise<Live>;
  private async load(sessionId: string, request: Request, bootstrap: boolean): Promise<Live> {
    if (this.live && this.live.stored.until > Date.now()) {
      if (this.live.failed || this.live.stored.sessionId !== sessionId) fail("authority_retired");
      return this.live;
    }
    if (this.loading) return this.loading;
    this.loading = (async () => {
      const now = Date.now(); let stored = await this.ctx.storage.get<Stored>("authority");
      if (stored && stored.until > now && stored.busy) fail("authority_retired");
      if (stored && stored.until <= now) { await this.ctx.storage.deleteAll(); stored = undefined; }
      if (stored && stored.sessionId !== sessionId) fail("invalid_request");
      if (!stored && !bootstrap) fail("authority_retired");
      if (stored && (!stored.clock || now < stored.clock.wall)) fail("authority_retired");
      // performance.now has a new epoch after eviction. Restore the original
      // virtual monotonic epoch, consuming all elapsed wall time during dormancy.
      // Signed wall deadlines remain independent caps and rollback fails closed.
      const started = performance.now(), anchor = stored ? stored.clock.monotonic + now - stored.clock.wall : started;
      const clock = { wall: () => Date.now(), monotonic: () => anchor + performance.now() - started };
      // This dependency reads only an internal header created after Worker RS256 verification.
      // No incoming HTTP header is forwarded as an authenticated principal.
      const dependencies = createSocialFirebaseDependencies(this.env.FIREBASE_PROJECT_ID, "", sessionId);
      dependencies.clock = clock;
      let session;
      if (!stored) session = await dependencies.readSession({ accountId: request.headers.get("x-aurora-verified-account")!, request }, sessionId, request.signal);
      const binding = createSocialBindingAuthority(dependencies, stored?.binding);
      const keys = stored ? {
        privateKey: await crypto.subtle.importKey("jwk", stored.privateJwk, "Ed25519", true, ["sign"]),
        publicKey: await crypto.subtle.importKey("jwk", stored.publicJwk, "Ed25519", true, ["verify"]),
      } : await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
      const admission = await createSocialAdmissionAuthority(binding, dependencies, { keys, state: stored?.admission, online: true });
      stored ??= { sessionId, until: Math.min(session!.expiresAtMs, now + 14_400_000), busy: false,
        clock: { wall: clock.wall(), monotonic: clock.monotonic() },
        privateJwk: await crypto.subtle.exportKey("jwk", keys.privateKey), publicJwk: await crypto.subtle.exportKey("jwk", keys.publicKey),
        binding: binding.durableState(), admission: admission.durableState() };
      if (stored.until <= now) fail("session_not_authorized");
      await this.ctx.storage.put("authority", stored); await this.ctx.storage.setAlarm(stored.until);
      return this.live = { stored, binding, admission, keys, clock, active: 0, failed: false };
    })();
    try { return await this.loading; } finally { this.loading = undefined; }
  }
  private async checkpoint(live: Live) {
    if (live.failed) fail("authority_retired");
    const wall = live.clock.wall();
    if (wall < live.stored.clock.wall) { live.failed = true; fail("authority_retired"); }
    const snapshot: Stored = { ...live.stored, clock: { wall, monotonic: live.clock.monotonic() }, binding: live.binding.durableState(), admission: live.admission.durableState(), busy: live.active > 0 };
    live.stored.clock = snapshot.clock;
    try { await this.ctx.storage.put("authority", snapshot); }
    catch { live.failed = true; fail("backend_unavailable"); }
  }
  async dispatch(uid: string, token: string, sessionId: string, path: string, body: unknown): Promise<unknown> {
    const receivedWall = Date.now();
    const request = new Request(`https://social.internal/v1/social/${path}`, { method: "POST", headers: { Authorization: `Bearer ${token}`, "x-aurora-verified-account": uid }, signal: AbortSignal.timeout(5000) });
    if (path === "presence") {
      if (!body || typeof body !== "object" || Array.isArray(body) || Object.keys(body).length) fail("invalid_request");
      // Firestore's real caller rules still decide whether this advert is visible.
      // Approval to join is a separate capability and is never granted here.
      const session = await createSocialFirebaseDependencies(this.env.FIREBASE_PROJECT_ID, uid, sessionId, true)
        .readSession({ accountId: uid, request }, sessionId, request.signal);
      const stored = await this.ctx.storage.get<Stored>("authority"), now = Date.now();
      const usable = session.expiresAtMs > now && !!stored && stored.sessionId === sessionId && stored.until > now
        && stored.clock.wall <= now && !this.live?.failed && (!stored.busy || !!this.live?.active);
      return { accountId: uid, ...projectSocialPresence(sessionId, stored?.admission, now, usable) };
    }
    const live = await this.load(sessionId, request, path === "bootstrap");
    if (live.active >= 16) fail("authority_busy");
    live.active++;
    const { stored, binding, admission, keys } = live;
    try {
      // Durable crash barrier before any proof/slot can be consumed. Methods share
      // one canonical authority and preserve its synchronous claims across awaits.
      await this.checkpoint(live);
      const session = path === "bootstrap" ? await createSocialFirebaseDependencies(this.env.FIREBASE_PROJECT_ID, uid, sessionId)
        .readSession({ accountId: uid, request }, sessionId, request.signal) : undefined;
        switch (path) {
          case "relay/ticket": {
            const identity = await admission.relayIdentity(request, body);
            const ticket = { schemaVersion: 1, ...identity, directAddresses: [] };
            const signature = await crypto.subtle.sign("Ed25519", keys.privateKey, new TextEncoder().encode("aurora/connect/relay-ticket/v1\0" + JSON.stringify(ticket)));
            return { body: ticket, signature: [...new Uint8Array(signature)].map(byte => byte.toString(16).padStart(2, "0")).join("") };
          }
          case "bootstrap": {
            const requested = clockRequest(body);
            const result = { schemaVersion: 1, sessionId, accountId: uid, issuerInstance: binding.issuerInstance,
              admissionPublicKey: admission.admissionPublicKey, joinPolicy: session!.joinPolicy, maxPlayers: session!.maxPlayers, expiresAtUnixMs: stored.until };
            if (!requested) return result;
            const serverNow = Date.now();
            // Workers' performance.now is also based on the last I/O time, so
            // it cannot independently attest elapsed time. The native caller
            // bounds this signed interval by its original monotonic HTTP RTT.
            if (serverNow < receivedWall) fail("backend_unavailable");
            const clockBody = { schemaVersion: 1, issuerInstance: binding.issuerInstance, accountId: uid, sessionId,
              operationId: requested.operationId, generation: requested.generation, nonce: requested.nonce,
              serverReceivedAtUnixMs: receivedWall, serverNowUnixMs: serverNow, sessionExpiresAtUnixMs: stored.until };
            if (clockBody.serverNowUnixMs >= stored.until) fail("session_not_authorized");
            const signed = await crypto.subtle.sign("Ed25519", keys.privateKey,
              new TextEncoder().encode("aurora/connect/social-clock/v1\0" + JSON.stringify(clockBody)));
            return { ...result, clock: { body: clockBody, signature: [...new Uint8Array(signed)].map(byte => byte.toString(16).padStart(2, "0")).join("") } };
          }
          case "identity": return await admission.identity(request, body);
          case "key/challenge": return await binding.challenge(request, body);
          case "key/proof": return await binding.prove(request, body);
          case "context": return await admission.resolveContext(request, body);
          case "host/register": return await admission.registerHost(request, body);
          case "join": return await admission.join(request, body);
          case "continuity": return await admission.continuity(request, body);
          case "host/poll": return await admission.hostPoll(request, body);
          case "host/publish": return await admission.hostPublish(request, body);
          case "guest/receive": return await admission.guestReceive(request, body);
          case "host/stop": return await admission.hostStop(request, body);
          case "host/revoke": return await admission.hostRevoke(request, body);
          case "guest/stop": return await admission.guestStop(request, body);
          default: return fail("invalid_request");
        }
    } finally {
      live.active--;
      // All responses pass the storage output gate. A restart while any operation
      // is pending retires the authority; a complete snapshot preserves tombstones.
      await this.checkpoint(live);
    }
  }
  /** RPC exceptions lose their custom prototype across isolates. Carry only our
   * closed error code across the boundary, never an upstream message or token. */
  async dispatchHttp(uid: string, token: string, sessionId: string, path: string, body: unknown): Promise<{ ok: true; value: unknown } | { ok: false; code: string }> {
    try { return { ok: true, value: await this.dispatch(uid, token, sessionId, path, body) }; }
    catch (error) { return { ok: false, code: error instanceof SocialBindingError ? error.code : "backend_unavailable" }; }
  }
  /** Internal relay RPC. Never accepts an account or destination outside the signed original authority. */
  async relayAuthorize(ticket: { body: Record<string, unknown>; signature: string }): Promise<{ body: Record<string, unknown>; allowedPeers: string[]; freshUntil: number }> {
    const stored = await this.ctx.storage.get<Stored>("authority"), now = Date.now(), body = ticket?.body;
    if (!stored || (stored.busy && !(this.live && this.live.active > 0 && !this.live.failed)) || stored.until <= now || !body || typeof ticket.signature !== "string" || !/^[0-9a-f]{128}$/.test(ticket.signature)) fail("relay_unauthorized");
    const keys = ["schemaVersion", "sessionId", "issuerInstance", "peerId", "hostPeerId", "accountId", "generation", "issuedAtUnixMs", "expiresAtUnixMs", "directAddresses"];
    if (Object.keys(body).length !== keys.length || keys.some(key => !Object.hasOwn(body, key)) || body.schemaVersion !== 1
      || body.sessionId !== stored.sessionId || body.issuerInstance !== stored.binding.issuerInstance || !Array.isArray(body.directAddresses) || body.directAddresses.length !== 0
      || typeof body.issuedAtUnixMs !== "number" || typeof body.expiresAtUnixMs !== "number" || body.issuedAtUnixMs > now || body.expiresAtUnixMs <= now || body.expiresAtUnixMs - body.issuedAtUnixMs > 14_400_000) fail("relay_unauthorized");
    const ordered = Object.fromEntries(keys.map(key => [key, body[key]]));
    const key = await crypto.subtle.importKey("jwk", stored.publicJwk, "Ed25519", false, ["verify"]);
    const signature = Uint8Array.from(ticket.signature.match(/../g)!, pair => parseInt(pair, 16));
    if (!await crypto.subtle.verify("Ed25519", key, signature, new TextEncoder().encode("aurora/connect/relay-ticket/v1\0" + JSON.stringify(ordered)))) fail("relay_unauthorized");
    const current = await this.ctx.storage.get<Stored>("authority");
    const checkedNow = Date.now();
    if (!current || (current.busy && !(this.live && this.live.active > 0 && !this.live.failed)) || current.binding.issuerInstance !== stored.binding.issuerInstance || current.until <= checkedNow) fail("relay_unauthorized");
    const policy = current.admission.policies.find(([, entry]) => entry.body.publicKey === body.hostPeerId)?.[1];
    if (!policy || policy.state === "terminal" || policy.body.expiresAtUnixMs <= checkedNow || !policy.checkedAt || policy.checkedAt > checkedNow || policy.checkedAt + 3000 <= checkedNow) fail("relay_unauthorized");
    const live = current.admission.grants.filter(([, grant]) => grant.policyId === policy.body.policyId && !grant.terminal
      && grant.body.sessionExpiresAtUnixMs > checkedNow && grant.lastLease && grant.lastLease.issuedAtUnixMs <= checkedNow && grant.lastLease.expiresAtUnixMs > checkedNow);
    if (body.peerId === policy.body.publicKey && body.accountId === policy.body.accountId && body.generation === policy.body.generation
      && body.expiresAtUnixMs === policy.body.expiresAtUnixMs) return { body: ordered, allowedPeers: live.map(([, grant]) => grant.body.guestPublicKey),
        // Each target has its own independent lease check in the relay. A guest
        // departure must not expire the host socket shared with other guests.
        freshUntil: Math.min(policy.checkedAt + 3000, policy.body.expiresAtUnixMs) };
    const guest = live.find(([, grant]) => grant.body.guestPublicKey === body.peerId && grant.body.guestAccountId === body.accountId
      && grant.body.guestGeneration === body.generation && grant.body.sessionExpiresAtUnixMs === body.expiresAtUnixMs);
    if (!guest) fail("relay_unauthorized"); return { body: ordered, allowedPeers: [policy.body.publicKey], freshUntil: Math.min(policy.checkedAt + 3000, policy.body.expiresAtUnixMs, guest![1].lastLease!.expiresAtUnixMs) };
  }
  async alarm() { if (this.live) this.live.failed = true; this.live = undefined; await this.ctx.storage.deleteAll(); }
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    try {
      const relayPath = new URL(request.url).pathname;
      if (relayPath === "/relay" || relayPath === "/ping") return routeRelayRequest(request, env);
      const url = new URL(request.url), path = url.pathname.replace(/^\/v1\/social\//, ""), sessionId = request.headers.get("x-aurora-session") ?? "";
      // This is a native-only service. Browsers and renderer-provided origins have no CORS authority.
      if (request.method !== "POST" || url.protocol !== "https:" || url.search || request.headers.has("origin")
        || !url.pathname.startsWith("/v1/social/") || !paths.has(path) || !SOCIAL_UUID.test(sessionId)
        || request.headers.get("content-type") !== "application/json" || request.headers.has("content-encoding")) fail("invalid_request");
      const body = await socialJson(new Response(request.body), 8192);
      if (path === "bootstrap") clockRequest(body);
      const uid = await socialAuthenticate(request, env.FIREBASE_PROJECT_ID, AbortSignal.timeout(5000));
      const budget = env.SOCIAL_ACCOUNTS.getByName(uid);
      if (!await (path === "presence" ? budget.admitObservation(sessionId) : budget.admit(sessionId))) fail("rate_limited");
      const result = await env.SOCIAL_SESSIONS.getByName(sessionId).dispatchHttp(uid, socialBearer(request), sessionId, path, body);
      if (!result.ok) fail(result.code);
      return reply(result.value);
    } catch (error) {
      const code = error instanceof SocialBindingError ? error.code : "backend_unavailable";
      const status = code === "unauthenticated" ? 401 : code === "rate_limited" ? 429 : code === "request_too_large" ? 413
        : ["backend_unavailable", "backend_timeout", "authority_busy"].includes(code) ? 503 : 403;
      return reply({ code }, status);
    }
  },
};
