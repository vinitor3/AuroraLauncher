/** Account/key possession only. No transport acceptor consumes these receipts. */
export const SOCIAL_KEY_PROOF_DOMAIN = "aurora/connect/social-key-proof/v1\0";
export const SOCIAL_BINDING_MAX_CHALLENGE_BYTES = 2_048;
const encoder = new TextEncoder();
const HEX = /^[0-9a-f]{64}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const UID = /^[A-Za-z0-9_-]{1,128}$/;
const challengeKeys = ["schemaVersion", "purpose", "issuerInstance", "challengeId", "accountId", "operationId", "generation", "role", "publicKey", "sessionId", "sessionDigest", "issuedAtUnixMs", "expiresAtUnixMs"] as const;
export type SocialKeyChallenge = {
  schemaVersion: 1; purpose: "accountKeyBinding"; issuerInstance: string; challengeId: string;
  accountId: string; operationId: string; generation: number; role: "host" | "guest";
  publicKey: string; sessionId: string; sessionDigest: string; issuedAtUnixMs: number; expiresAtUnixMs: number;
};
export type SocialKeyProof = { schemaVersion: 1; challenge: SocialKeyChallenge; signature: string };
export type SocialBindingInput = Pick<SocialKeyChallenge, "schemaVersion" | "operationId" | "generation" | "role" | "publicKey" | "sessionId"> & { preparedExpiresAtUnixMs: number };
export type SocialBindingSession = {
  id: string; hostUid: string; title: string; modpackName: string; minecraftVersion: string; loader: string;
  imageUrl: string | null; players: number; maxPlayers: number; visibility: "private" | "friends" | "public";
  joinPolicy: "open" | "approval"; expiresAtMs: number;
};
export type SocialBindingReceipt = {
  bindingId: string; accountId: string; publicKey: string; operationId: string; generation: number; sessionId: string; sessionDigest: string;
  role: "host" | "guest"; expiresAtUnixMs: number; transportAuthorized: false;
};
export type SocialBindingPrincipal = { accountId: string; request: Request };
export type SocialPrivateAuthorization = {
  authorizationId: string; revision: number; sessionId: string; hostUid: string; recipientUid: string; expiresAtMs: number;
};
export type SocialBindingDependencies = {
  /** Production must verify a signed ID token. Emulator implementations belong in a separate local entrypoint. */
  trustedAuthenticate(request: Request, signal: AbortSignal): Promise<string>;
  /** Must freshly validate Aurora registration and authorize this exact session with the caller's token. */
  readSession(principal: SocialBindingPrincipal, sessionId: string, signal: AbortSignal): Promise<SocialBindingSession>;
  /** Current active recipient authorization; never an inbox hint or a caller-supplied claim. */
  readPrivateAuthorization?(principal: SocialBindingPrincipal, sessionId: string, recipientUid: string, signal: AbortSignal): Promise<SocialPrivateAuthorization>;
  clock?: { wall(): number; monotonic(): number };
};
export class SocialBindingError extends Error {
  readonly code: string;
  constructor(code: string) { super(code); this.name = "SocialBindingError"; this.code = code; }
}
function fail(code: string): never { throw new SocialBindingError(code); }
function exact(value: unknown, keys: readonly string[]): asserts value is Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value) || Object.getPrototypeOf(value) !== Object.prototype
      || Object.keys(value).length !== keys.length || keys.some(key => !Object.hasOwn(value, key))) fail("invalid_shape");
}
const positiveInteger = (n: unknown): n is number => typeof n === "number" && Number.isSafeInteger(n) && n > 0;
const matches = (regex: RegExp, value: unknown): value is string => typeof value === "string" && regex.test(value);
export function validateSocialKeyChallenge(value: unknown): SocialKeyChallenge {
  exact(value, challengeKeys);
  if (value.schemaVersion !== 1 || value.purpose !== "accountKeyBinding" || !matches(HEX, value.issuerInstance)
      || !matches(HEX, value.challengeId) || !matches(UID, value.accountId) || !matches(UUID, value.operationId)
      || !positiveInteger(value.generation) || (value.role !== "host" && value.role !== "guest")
      || !matches(HEX, value.publicKey) || !matches(UUID, value.sessionId) || !matches(HEX, value.sessionDigest)
      || !positiveInteger(value.issuedAtUnixMs) || !positiveInteger(value.expiresAtUnixMs)
      || value.expiresAtUnixMs - value.issuedAtUnixMs < 1 || value.expiresAtUnixMs - value.issuedAtUnixMs > 60_000
      || encoder.encode(JSON.stringify(value)).length > SOCIAL_BINDING_MAX_CHALLENGE_BYTES) fail("invalid_challenge");
  return Object.fromEntries(challengeKeys.map(key => [key, value[key]])) as SocialKeyChallenge;
}
/** Fixed order, decimal integers, UTF-8 and u16 BE lengths; shared with the native signer. */
export function socialKeyProofPreimage(input: unknown): Uint8Array<ArrayBuffer> {
  const challenge = validateSocialKeyChallenge(input);
  const fields = challengeKeys.map(key => encoder.encode(String(challenge[key])));
  const domain = encoder.encode(SOCIAL_KEY_PROOF_DOMAIN);
  const bytes = new Uint8Array(domain.length + fields.reduce((length, field) => length + field.length + 2, 0));
  bytes.set(domain); let offset = domain.length;
  for (const field of fields) { bytes[offset++] = field.length >>> 8; bytes[offset++] = field.length & 255; bytes.set(field, offset); offset += field.length; }
  return bytes;
}
function inputValue(value: unknown): SocialBindingInput {
  exact(value, ["schemaVersion", "operationId", "generation", "role", "publicKey", "sessionId", "preparedExpiresAtUnixMs"]);
  if (value.schemaVersion !== 1 || !matches(UUID, value.operationId) || !positiveInteger(value.generation)
      || (value.role !== "host" && value.role !== "guest") || !matches(HEX, value.publicKey) || !matches(UUID, value.sessionId)
      || !positiveInteger(value.preparedExpiresAtUnixMs)) fail("invalid_input");
  return { ...value } as SocialBindingInput;
}
function proofValue(value: unknown): SocialKeyProof {
  exact(value, ["schemaVersion", "challenge", "signature"]);
  if (value.schemaVersion !== 1 || !matches(/^[0-9a-f]{128}$/, value.signature)) fail("invalid_proof");
  return { schemaVersion: 1, challenge: validateSocialKeyChallenge(value.challenge), signature: value.signature };
}
export function normalizeSocialBindingSession(value: unknown): SocialBindingSession {
  const keys = ["id", "hostUid", "title", "modpackName", "minecraftVersion", "loader", "imageUrl", "players", "maxPlayers", "visibility", "joinPolicy", "expiresAtMs"];
  exact(value, keys);
  const text = (v: unknown, max: number) => typeof v === "string" && v.trim().length > 0 && v.length <= max && !/[\u0000-\u001f\u007f]/.test(v);
  const image = value.imageUrl;
  if (!matches(UUID, value.id) || !matches(UID, value.hostUid) || !text(value.title, 64) || !text(value.modpackName, 64)
      || !text(value.minecraftVersion, 24) || !text(value.loader, 24)
      || !(image === null || (typeof image === "string" && image.length <= 512 && /^https:\/\/(cdn\.modrinth\.com|media\.forgecdn\.net|mediafilez\.forgecdn\.net)\/[A-Za-z0-9_./%+-]+\.(png|jpg|jpeg|webp)$/.test(image)))
      || !Number.isSafeInteger(value.players) || !positiveInteger(value.maxPlayers) || value.maxPlayers > 100
      || (value.players as number) < 0 || (value.players as number) > value.maxPlayers
      || (value.visibility !== "private" && value.visibility !== "friends" && value.visibility !== "public")
      || (value.joinPolicy !== "open" && value.joinPolicy !== "approval")
      || !positiveInteger(value.expiresAtMs)) fail("invalid_session");
  return Object.fromEntries(keys.map(key => [key, value[key]])) as SocialBindingSession;
}
const toHex = (bytes: Uint8Array) => Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
const fromHex = (text: string) => Uint8Array.from(text.match(/../g)!, pair => Number.parseInt(pair, 16));
export async function socialBindingSessionDigest(input: unknown): Promise<string> {
  const session = normalizeSocialBindingSession(input);
  // The object has the fixed insertion order above; no credentials, graph, endpoint or mutable client projection.
  return toHex(new Uint8Array(await crypto.subtle.digest("SHA-256", encoder.encode("aurora/connect/social-session/v1\0" + JSON.stringify(session)))));
}
async function bounded<T>(work: (signal: AbortSignal) => Promise<T>, callerSignal?: AbortSignal): Promise<T> {
  const controller = new AbortController(); let timeout: ReturnType<typeof setTimeout> | undefined;
  let aborted: (() => void) | undefined;
  try {
    if (callerSignal?.aborted) fail("binding_cancelled");
    const cancellation = new Promise<never>((_, reject) => {
      aborted = () => { controller.abort(); reject(new SocialBindingError("binding_cancelled")); };
      callerSignal?.addEventListener("abort", aborted, { once: true });
    });
    return await Promise.race([cancellation, Promise.resolve().then(() => work(controller.signal)), new Promise<never>((_, reject) => {
      timeout = setTimeout(() => { controller.abort(); reject(new SocialBindingError("backend_timeout")); }, 5_000);
    })]);
  } catch (error) { if (error instanceof SocialBindingError) throw error; return fail("backend_unavailable"); }
  finally { if (timeout !== undefined) clearTimeout(timeout); if (aborted) callerSignal?.removeEventListener("abort", aborted); controller.abort(); }
}
type Pending = { accountId: string; state: "preparing" | "pending" | "verifying"; challenge?: SocialKeyChallenge; deadline: number; issuedAt: number; expiresAt: number };
export type SocialBindingState = {
  issuerInstance: string; pending: [string, Pending][];
  receipts: [string, { receipt: SocialBindingReceipt; deadline: number; issuedAt: number }][];
  rates: [string, { until: number; count: number }][]; globalRate: { until: number; count: number };
};

/** One process, bounded RAM, fail closed on overload. Not a distributed authority or an ingress grant. */
export function createSocialBindingAuthority(dependencies: SocialBindingDependencies, restored?: SocialBindingState) {
  const clock = dependencies.clock ?? { wall: () => Date.now(), monotonic: () => performance.now() };
  const issuerInstance = restored?.issuerInstance ?? toHex(crypto.getRandomValues(new Uint8Array(32)));
  const pending = new Map<string, Pending>(restored?.pending);
  const receipts = new Map<string, { receipt: SocialBindingReceipt; deadline: number; issuedAt: number }>(restored?.receipts);
  const rates = new Map<string, { until: number; count: number }>(restored?.rates);
  let globalRate = restored?.globalRate ?? { until: clock.monotonic() + 60_000, count: 0 };
  function prune() {
    const mono = clock.monotonic(), wall = clock.wall();
    for (const [id, entry] of pending) if (entry.state !== "preparing" && entry.state !== "verifying" && (mono >= entry.deadline || wall >= entry.expiresAt || wall < entry.issuedAt)) pending.delete(id);
    for (const [id, entry] of receipts) if (mono >= entry.deadline || wall >= entry.receipt.expiresAtUnixMs || wall < entry.issuedAt) receipts.delete(id);
    for (const [id, rate] of rates) if (mono >= rate.until) rates.delete(id);
  }
  function globalAttempt() {
    prune(); const now = clock.monotonic();
    if (now >= globalRate.until) globalRate = { until: now + 60_000, count: 0 };
    if (++globalRate.count > 512) fail("rate_limited");
  }
  function accountAttempt(uid: string) {
    let rate = rates.get(uid);
    if (!rate) { if (rates.size >= 128) fail("authority_busy"); rate = { until: clock.monotonic() + 60_000, count: 0 }; rates.set(uid, rate); }
    if (++rate.count > 20) fail("rate_limited");
  }
  async function principal(request: Request): Promise<SocialBindingPrincipal> {
    const accountId = await bounded(signal => dependencies.trustedAuthenticate(request, signal), request.signal);
    if (!matches(UID, accountId)) fail("unauthenticated");
    accountAttempt(accountId); return { accountId, request };
  }
  async function sessionFor(who: SocialBindingPrincipal, sessionId: string, role: "host" | "guest") {
    const session = normalizeSocialBindingSession(await bounded(signal => dependencies.readSession(who, sessionId, signal), who.request.signal));
    if (session.id !== sessionId || session.expiresAtMs <= clock.wall() || (role === "host") !== (session.hostUid === who.accountId)) fail("session_not_authorized");
    return session;
  }
  function current(record: Pending, request: Request) {
    if (request.signal.aborted) fail("binding_cancelled");
    const wall = clock.wall();
    if (clock.monotonic() >= record.deadline || wall < record.issuedAt || wall >= record.expiresAt) fail("challenge_expired");
  }
  async function verifyProofInternal(request: Request, raw: unknown) {
    globalAttempt(); const proof = proofValue(raw);
    const record = pending.get(proof.challenge.challengeId);
    if (!record || record.state !== "pending" || proof.challenge.issuerInstance !== issuerInstance) fail("challenge_unavailable");
    // Claim before the first await: concurrent attempts can never use the same challenge twice.
    record.state = "verifying";
    try {
      current(record, request);
      if (JSON.stringify(record.challenge) !== JSON.stringify(proof.challenge)) fail("challenge_mismatch");
      const who = await principal(request);
      if (who.accountId !== record.accountId) fail("account_mismatch");
      current(record, request);
      const key = await crypto.subtle.importKey("raw", fromHex(proof.challenge.publicKey), "Ed25519", false, ["verify"]);
      if (!await crypto.subtle.verify("Ed25519", key, fromHex(proof.signature), socialKeyProofPreimage(proof.challenge))) fail("invalid_signature");
      current(record, request);
      const session = await sessionFor(who, proof.challenge.sessionId, proof.challenge.role);
      if (await socialBindingSessionDigest(session) !== proof.challenge.sessionDigest) fail("session_changed");
      current(record, request);
      if (receipts.size >= 128) fail("authority_busy");
      const receipt: SocialBindingReceipt = { bindingId: crypto.randomUUID(), accountId: who.accountId, publicKey: proof.challenge.publicKey,
        operationId: proof.challenge.operationId, generation: proof.challenge.generation,
        sessionId: proof.challenge.sessionId, sessionDigest: proof.challenge.sessionDigest, role: proof.challenge.role,
        expiresAtUnixMs: proof.challenge.expiresAtUnixMs, transportAuthorized: false };
      receipts.set(receipt.bindingId, { receipt, deadline: record.deadline, issuedAt: record.issuedAt });
      return { receipt: { ...receipt }, originalDeadlineMonotonic: record.deadline };
    } catch (error) { if (error instanceof SocialBindingError) throw error; return fail("invalid_proof"); }
    finally { pending.delete(proof.challenge.challengeId); }
  }
  return {
    issuerInstance,
    /** Private durable adapter state. Never return this through an HTTP route. */
    durableState(): SocialBindingState { prune(); return structuredClone({ issuerInstance, pending: [...pending], receipts: [...receipts], rates: [...rates], globalRate }); },
    async challenge(request: Request, raw: unknown): Promise<SocialKeyChallenge> {
      globalAttempt(); const input = inputValue(raw);
      const initialWall = clock.wall();
      const remaining = input.preparedExpiresAtUnixMs - initialWall;
      if (remaining < 1 || remaining > 60_000) fail("preparation_expired");
      const initialDeadline = clock.monotonic() + remaining;
      const who = await principal(request);
      if (pending.size >= 128 || receipts.size >= 128 || Array.from(pending.values()).filter(value => value.accountId === who.accountId).length >= 4) fail("authority_busy");
      const id = toHex(crypto.getRandomValues(new Uint8Array(32)));
      const record: Pending = { accountId: who.accountId, state: "preparing", deadline: initialDeadline, issuedAt: initialWall, expiresAt: input.preparedExpiresAtUnixMs };
      pending.set(id, record);
      try {
        current(record, request);
        const session = await sessionFor(who, input.sessionId, input.role);
        const sessionDigest = await socialBindingSessionDigest(session);
        current(record, request);
        const now = clock.wall();
        if (!positiveInteger(now) || now < record.issuedAt || now >= session.expiresAtMs || clock.monotonic() >= record.deadline) fail("challenge_expired");
        record.issuedAt = now; record.expiresAt = Math.min(now + 60_000, input.preparedExpiresAtUnixMs, session.expiresAtMs);
        record.deadline = Math.min(record.deadline, clock.monotonic() + record.expiresAt - now);
        const { preparedExpiresAtUnixMs: _preparedExpiresAtUnixMs, ...identity } = input;
        const challenge = validateSocialKeyChallenge({ ...identity, purpose: "accountKeyBinding", issuerInstance, challengeId: id,
          accountId: who.accountId, sessionDigest, issuedAtUnixMs: now, expiresAtUnixMs: record.expiresAt });
        record.challenge = challenge; record.state = "pending"; return { ...challenge };
      } catch (error) { pending.delete(id); throw error; }
    },
    async prove(request: Request, raw: unknown): Promise<SocialBindingReceipt> {
      return (await verifyProofInternal(request, raw)).receipt;
    },
    /** Internal same-process handoff only: never serialize this monotonic clock to HTTP or accept it from a caller. */
    proveRetained(request: Request, raw: unknown) { return verifyProofInternal(request, raw); },
    /** Aggregate diagnostics only; never exposes challenges, account ids, tokens or receipt contents. */
    inspect() { prune(); return { pending: pending.size, receipts: receipts.size, accountRateWindows: rates.size, transportAuthorized: false as const }; },
  };
}
