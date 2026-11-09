/** Signed social admission; local v1 and explicitly selected online versions retain separate bounds. */
import { normalizeSocialBindingSession, socialBindingSessionDigest, SocialBindingError } from "./connectSocialBinding.ts";
import type { createSocialBindingAuthority, SocialBindingDependencies, SocialBindingReceipt, SocialBindingSession } from "./connectSocialBinding.ts";

const encoder = new TextEncoder();
const HEX = /^[0-9a-f]{64}$/, SIGNATURE = /^[0-9a-f]{128}$/, UID = /^[A-Za-z0-9_-]{1,128}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
export const HOST_POLICY_DOMAIN = "aurora/connect/social-host-policy/v1\0";
export const SOCIAL_ADMISSION_DOMAIN = "aurora/connect/social-admission/v1\0";
export const SOCIAL_CONTINUITY_DOMAIN = "aurora/connect/social-continuity/v1\0";
export const SOCIAL_JOIN_CONTEXT_DOMAIN = "aurora/connect/social-join-context/v1\0";
const policyKeys = ["schemaVersion", "purpose", "issuerInstance", "policyId", "accountId", "operationId", "generation", "publicKey", "sessionId", "sessionDigest", "visibility", "joinPolicy", "serviceId", "protocol", "sourceDigest", "maxGuests", "issuedAtUnixMs", "expiresAtUnixMs"] as const;
const grantKeys = ["schemaVersion", "purpose", "issuerInstance", "grantId", "policyId", "policyDigest", "sessionId", "sessionDigest", "hostAccountId", "hostPublicKey", "hostOperationId", "hostGeneration", "guestAccountId", "guestPublicKey", "guestOperationId", "guestGeneration", "issuedAtUnixMs", "admissionExpiresAtUnixMs", "sessionExpiresAtUnixMs"] as const;
const leaseKeys = ["schemaVersion", "purpose", "issuerInstance", "grantId", "policyDigest", "sequence", "issuedAtUnixMs", "expiresAtUnixMs"] as const;
const voicePolicyKeys = [...policyKeys, "voiceProfile", "voiceSourceDigest", "hostPlayer"] as const;
const voiceGrantKeys = [...grantKeys, "voiceProfile", "voiceSourceDigest", "hostPlayer"] as const;
const runtimePolicyKeys = [...voicePolicyKeys, "runtimeProfile"] as const;
const runtimeGrantKeys = [...voiceGrantKeys, "runtimeProfile"] as const;
export const SOCIAL_RUNTIME_PROFILES = {
  "fabric-loader-0.16.14-1.21.1": ["1.21.1", "fabric"],
  "fabric-loader-0.16.14-1.20.1": ["1.20.1", "fabric"],
  "fabric-loader-0.16.14-1.19.2": ["1.19.2", "fabric"],
  "fabric-loader-0.13.3-1.16.5": ["1.16.5", "fabric"],
  "1.21.1-forge-52.1.12": ["1.21.1", "forge"],
  "1.20.1-forge-47.4.23": ["1.20.1", "forge"],
  "1.19.2-forge-43.4.16": ["1.19.2", "forge"],
  "1.16.5-forge-36.2.42": ["1.16.5", "forge"],
  "1.12.2-forge-14.23.5.2860": ["1.12.2", "forge"],
} as const;
export type SocialRuntimeProfile = keyof typeof SOCIAL_RUNTIME_PROFILES;
const policyFields = (schema: unknown) => schema === 4 ? runtimePolicyKeys : schema === 3 ? voicePolicyKeys : policyKeys;
const grantFields = (schema: unknown) => schema === 4 ? runtimeGrantKeys : schema === 3 ? voiceGrantKeys : grantKeys;
const runtimeValid = (profile: unknown, voice: unknown): profile is SocialRuntimeProfile => typeof profile === "string"
  && Object.hasOwn(SOCIAL_RUNTIME_PROFILES, profile) && !(profile === "1.12.2-forge-14.23.5.2860" && voice === "plasmo");
const contextKeys = ["schemaVersion", "purpose", "issuerInstance", "recipientAccountId", "sessionId", "sessionDigest", "hostAccountId", "hostPublicKey", "hostOperationId", "hostGeneration", "policyId", "policyDigest", "issuedAtUnixMs", "expiresAtUnixMs"] as const;
export type HostSocialPolicy = {
  schemaVersion: 1 | 2 | 3 | 4; purpose: "hostSocialSession"; issuerInstance: string; policyId: string; accountId: string; operationId: string; generation: number;
  publicKey: string; sessionId: string; sessionDigest: string; visibility: "public" | "friends"; joinPolicy: "open" | "approval"; serviceId: "game"; protocol: "tcp";
  sourceDigest: string; voiceProfile?: "svc" | "plasmo"; voiceSourceDigest?: string; hostPlayer?: boolean; runtimeProfile?: SocialRuntimeProfile; maxGuests: number; issuedAtUnixMs: number; expiresAtUnixMs: number;
};
export type SocialAdmissionGrant = {
  schemaVersion: 1 | 2 | 3 | 4; purpose: "socialAdmission"; issuerInstance: string; grantId: string; policyId: string; policyDigest: string; sessionId: string; sessionDigest: string;
  hostAccountId: string; hostPublicKey: string; hostOperationId: string; hostGeneration: number; guestAccountId: string; guestPublicKey: string; guestOperationId: string; guestGeneration: number; voiceProfile?: "svc" | "plasmo"; voiceSourceDigest?: string; hostPlayer?: boolean; runtimeProfile?: SocialRuntimeProfile;
  issuedAtUnixMs: number; admissionExpiresAtUnixMs: number; sessionExpiresAtUnixMs: number;
};
export type SocialContinuityLease = { schemaVersion: 1; purpose: "socialContinuity"; issuerInstance: string; grantId: string; policyDigest: string; sequence: number; issuedAtUnixMs: number; expiresAtUnixMs: number };
export type SocialJoinContext = { schemaVersion: 1; purpose: "socialJoinContext"; issuerInstance: string; recipientAccountId: string; sessionId: string; sessionDigest: string;
  hostAccountId: string; hostPublicKey: string; hostOperationId: string; hostGeneration: number; policyId: string; policyDigest: string; issuedAtUnixMs: number; expiresAtUnixMs: number };
export type SocialSigned<T> = { body: T; signature: string };
function fail(code: string): never { throw new SocialBindingError(code); }
const positive = (value: unknown): value is number => typeof value === "number" && Number.isSafeInteger(value) && value > 0;
const matches = (regex: RegExp, value: unknown): value is string => typeof value === "string" && regex.test(value);
function exact(input: unknown, keys: readonly string[]): asserts input is Record<string, unknown> {
  if (!input || typeof input !== "object" || Array.isArray(input) || Object.getPrototypeOf(input) !== Object.prototype
      || Object.keys(input).length !== keys.length || keys.some(key => !Object.hasOwn(input, key))) fail("invalid_shape");
  if (encoder.encode(JSON.stringify(input)).length > 8_192) fail("request_too_large");
}
function ordered<T>(input: Record<string, unknown>, keys: readonly string[]): T { return Object.fromEntries(keys.map(key => [key, input[key]])) as T; }
function canonical(domain: string, input: object, keys: readonly string[]): Uint8Array<ArrayBuffer> {
  const fields = keys.map(key => encoder.encode(String((input as Record<string, unknown>)[key]))), prefix = encoder.encode(domain);
  const out = new Uint8Array(prefix.length + fields.reduce((length, field) => length + 2 + field.length, 0)); out.set(prefix); let offset = prefix.length;
  for (const field of fields) { if (field.length > 65_535) fail("invalid_shape"); out[offset++] = field.length >>> 8; out[offset++] = field.length & 255; out.set(field, offset); offset += field.length; }
  return out;
}
export function validateHostSocialPolicy(input: unknown): HostSocialPolicy {
  const keys = policyFields((input as {schemaVersion?: unknown})?.schemaVersion);
  exact(input, keys);
  if (![1, 2, 3, 4].includes(input.schemaVersion as number) || input.purpose !== "hostSocialSession" || !matches(HEX, input.issuerInstance) || !matches(HEX, input.policyId)
    || !matches(UID, input.accountId) || !matches(UUID, input.operationId) || !positive(input.generation) || !matches(HEX, input.publicKey)
    || !matches(UUID, input.sessionId) || !matches(HEX, input.sessionDigest) || (input.visibility !== "public" && input.visibility !== "friends")
    || !["open", "approval"].includes(input.joinPolicy as string) || input.serviceId !== "game" || input.protocol !== "tcp" || !matches(HEX, input.sourceDigest) || (input.schemaVersion === 1 ? input.maxGuests !== 1 : !positive(input.maxGuests) || input.maxGuests > 8)
    || !positive(input.issuedAtUnixMs) || !positive(input.expiresAtUnixMs) || input.expiresAtUnixMs <= input.issuedAtUnixMs
    || input.expiresAtUnixMs - input.issuedAtUnixMs > (input.schemaVersion !== 1 ? 14_400_000 : 60_000)) fail("invalid_policy");
  if ([3, 4].includes(input.schemaVersion as number) && (!["svc", "plasmo"].includes(input.voiceProfile as string) || !matches(HEX, input.voiceSourceDigest) || typeof input.hostPlayer !== "boolean" || (input.hostPlayer && (input.maxGuests as number) > 7))) fail("invalid_policy");
  if (input.schemaVersion === 4 && !runtimeValid(input.runtimeProfile, input.voiceProfile)) fail("invalid_policy");
  return ordered(input, keys);
}
export function validateSocialAdmissionGrant(input: unknown): SocialAdmissionGrant {
  const keys = grantFields((input as {schemaVersion?: unknown})?.schemaVersion);
  exact(input, keys);
  if (![1, 2, 3, 4].includes(input.schemaVersion as number) || input.purpose !== "socialAdmission" || !matches(HEX, input.issuerInstance) || !matches(HEX, input.grantId)
    || !matches(HEX, input.policyId) || !matches(HEX, input.policyDigest) || !matches(UUID, input.sessionId) || !matches(HEX, input.sessionDigest)
    || !matches(UID, input.hostAccountId) || !matches(HEX, input.hostPublicKey) || !matches(UUID, input.hostOperationId) || !positive(input.hostGeneration)
    || !matches(UID, input.guestAccountId) || !matches(HEX, input.guestPublicKey) || !matches(UUID, input.guestOperationId) || !positive(input.guestGeneration)
    || input.hostAccountId === input.guestAccountId || input.hostPublicKey === input.guestPublicKey
    || !positive(input.issuedAtUnixMs) || !positive(input.admissionExpiresAtUnixMs) || !positive(input.sessionExpiresAtUnixMs)
    || input.admissionExpiresAtUnixMs <= input.issuedAtUnixMs || input.admissionExpiresAtUnixMs > input.sessionExpiresAtUnixMs
    || input.admissionExpiresAtUnixMs - input.issuedAtUnixMs > 10_000 || input.sessionExpiresAtUnixMs - input.issuedAtUnixMs > (input.schemaVersion !== 1 ? 14_400_000 : 60_000)) fail("invalid_grant");
  if ([3, 4].includes(input.schemaVersion as number) && (!["svc", "plasmo"].includes(input.voiceProfile as string) || !matches(HEX, input.voiceSourceDigest) || typeof input.hostPlayer !== "boolean")) fail("invalid_grant");
  if (input.schemaVersion === 4 && !runtimeValid(input.runtimeProfile, input.voiceProfile)) fail("invalid_grant");
  return ordered(input, keys);
}
export function validateSocialContinuityLease(input: unknown): SocialContinuityLease {
  exact(input, leaseKeys);
  if (input.schemaVersion !== 1 || input.purpose !== "socialContinuity" || !matches(HEX, input.issuerInstance) || !matches(HEX, input.grantId)
    || !matches(HEX, input.policyDigest) || !positive(input.sequence) || !positive(input.issuedAtUnixMs) || !positive(input.expiresAtUnixMs)
    || input.expiresAtUnixMs <= input.issuedAtUnixMs || input.expiresAtUnixMs - input.issuedAtUnixMs > 3_000) fail("invalid_lease");
  return ordered(input, leaseKeys);
}
export function validateSocialJoinContext(input: unknown): SocialJoinContext {
  exact(input, contextKeys);
  if (input.schemaVersion !== 1 || input.purpose !== "socialJoinContext" || !matches(HEX, input.issuerInstance)
    || !matches(UID, input.recipientAccountId) || !matches(UUID, input.sessionId) || !matches(HEX, input.sessionDigest)
    || !matches(UID, input.hostAccountId) || input.hostAccountId === input.recipientAccountId || !matches(HEX, input.hostPublicKey)
    || !matches(UUID, input.hostOperationId) || !positive(input.hostGeneration) || !matches(HEX, input.policyId) || !matches(HEX, input.policyDigest)
    || !positive(input.issuedAtUnixMs) || !positive(input.expiresAtUnixMs) || input.expiresAtUnixMs <= input.issuedAtUnixMs
    || input.expiresAtUnixMs - input.issuedAtUnixMs > 10_000) fail("invalid_context");
  return ordered(input, contextKeys);
}
export const hostSocialPolicyPreimage = (input: unknown) => canonical(HOST_POLICY_DOMAIN, validateHostSocialPolicy(input), policyFields((input as HostSocialPolicy).schemaVersion));
export const socialAdmissionPreimage = (input: unknown) => canonical(SOCIAL_ADMISSION_DOMAIN, validateSocialAdmissionGrant(input), grantFields((input as SocialAdmissionGrant).schemaVersion));
export const socialContinuityPreimage = (input: unknown) => canonical(SOCIAL_CONTINUITY_DOMAIN, validateSocialContinuityLease(input), leaseKeys);
export const socialJoinContextPreimage = (input: unknown) => canonical(SOCIAL_JOIN_CONTEXT_DOMAIN, validateSocialJoinContext(input), contextKeys);
const hex = (bytes: Uint8Array) => Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
const unhex = (text: string) => Uint8Array.from(text.match(/../g)!, byte => Number.parseInt(byte, 16));
export const hostSocialPolicyDigest = async (input: unknown) => hex(new Uint8Array(await crypto.subtle.digest("SHA-256", hostSocialPolicyPreimage(input))));
function signedPolicy(input: unknown): SocialSigned<HostSocialPolicy> {
  exact(input, ["body", "signature"]); if (!matches(SIGNATURE, input.signature)) fail("invalid_signature");
  return { body: validateHostSocialPolicy(input.body), signature: input.signature };
}
async function bounded<T>(request: Request, work: (signal: AbortSignal) => Promise<T>): Promise<T> {
  const controller = new AbortController(); let timer: ReturnType<typeof setTimeout> | undefined, abort: (() => void) | undefined;
  try {
    if (request.signal.aborted) fail("binding_cancelled");
    return await Promise.race([Promise.resolve().then(() => work(controller.signal)), new Promise<never>((_, reject) => {
      abort = () => { controller.abort(); reject(new SocialBindingError("binding_cancelled")); };
      request.signal.addEventListener("abort", abort, { once: true });
      timer = setTimeout(() => { controller.abort(); reject(new SocialBindingError("backend_timeout")); }, 5_000);
    })]);
  } catch (error) { if (error instanceof SocialBindingError) throw error; return fail("backend_unavailable"); }
  finally { clearTimeout(timer); if (abort) request.signal.removeEventListener("abort", abort); controller.abort(); }
}
type PolicyState = { body: HostSocialPolicy; digest: string; deadline: number; checkedAt?: number; pendingGuests?: number;
  pendingIdentities?: { accountId: string; publicKey: string }[]; deniedAccounts?: string[]; state: "registering" | "open" | "reserved" | "terminal" };
type GrantState = { body: SocialAdmissionGrant; policy: PolicyState; deadline: number; admissionDeadline: number; terminal: boolean; busy: boolean; sequence: number;
  lastLease?: SocialContinuityLease; leaseDeadline: number; attempts: number[];
  signedGrant?: SocialSigned<SocialAdmissionGrant>; initialLease?: SocialSigned<SocialContinuityLease>;
  signedLease?: SocialSigned<SocialContinuityLease>; invitation?: string; publishing?: boolean };
export type SocialAdmissionState = {
  policies: [string, PolicyState][]; grants: [string, Omit<GrantState, "policy"> & { policyId: string }][];
  entryWindow: { until: number; count: number }; continuityWindow: { until: number; count: number };
  accountRates: [string, { until: number; count: number }][];
};

export async function createSocialAdmissionAuthority(binding: ReturnType<typeof createSocialBindingAuthority>, dependencies: SocialBindingDependencies,
  durable?: { keys: CryptoKeyPair; state?: SocialAdmissionState; online?: boolean }) {
  const clock = dependencies.clock ?? { wall: () => Date.now(), monotonic: () => performance.now() };
  const keys = durable?.keys ?? await crypto.subtle.generateKey("Ed25519", false, ["sign", "verify"]);
  const admissionPublicKey = hex(new Uint8Array(await crypto.subtle.exportKey("raw", keys.publicKey)));
  const issuerInstance = binding.issuerInstance;
  const policies = new Map<string, PolicyState>(durable?.state?.policies), grants = new Map<string, GrantState>();
  for (const [id, stored] of durable?.state?.grants ?? []) {
    const { policyId, ...value } = stored, policy = policies.get(policyId);
    if (!policy) fail("backend_unavailable"); grants.set(id, { ...value, policy });
  }
  let entryWindow = durable?.state?.entryWindow ?? { until: clock.monotonic() + 60_000, count: 0 },
    continuityWindow = durable?.state?.continuityWindow ?? { until: clock.monotonic() + 60_000, count: 0 };
  const accountRates = new Map<string, { until: number; count: number }>(durable?.state?.accountRates);
  type Observation = { session: SocialBindingSession; wall: number; mono: number };
  const observations = new Map<string, { credential: string; wall: number; mono: number; value: Promise<Observation> }>();
  function prune() {
    for (const [id, value] of policies) if (value.state !== "registering" && (clock.monotonic() >= value.deadline || clock.wall() >= value.body.expiresAtUnixMs)) policies.delete(id);
    for (const [id, value] of grants) if (!value.busy && (clock.monotonic() >= value.deadline || clock.wall() >= value.body.sessionExpiresAtUnixMs)) grants.delete(id);
    for (const [id, value] of accountRates) if (clock.monotonic() >= value.until) accountRates.delete(id);
  }
  function attempt() { prune(); if (clock.monotonic() >= entryWindow.until) entryWindow = { until: clock.monotonic() + 60_000, count: 0 }; if (++entryWindow.count > (durable?.online ? 4096 : 512)) fail("rate_limited"); }
  function accountAttempt(uid: string) {
    let rate = accountRates.get(uid); if (!rate) { if (accountRates.size >= 128) fail("authority_busy"); rate = { until: clock.monotonic() + 60_000, count: 0 }; accountRates.set(uid, rate); }
    if (++rate.count > 256) fail("rate_limited");
  }
  function current(policy: PolicyState, request: Request) {
    if (request.signal.aborted) fail("binding_cancelled");
    if (policy.state === "terminal" || clock.monotonic() >= policy.deadline || clock.wall() < policy.body.issuedAtUnixMs || clock.wall() >= policy.body.expiresAtUnixMs) fail("policy_expired");
  }
  function live(grant: GrantState, request: Request, requireLease = true) {
    current(grant.policy, request);
    if (grant.terminal || clock.monotonic() >= grant.deadline || clock.wall() < grant.body.issuedAtUnixMs || clock.wall() >= grant.body.sessionExpiresAtUnixMs) fail("grant_terminal");
    if (requireLease && (!grant.lastLease || clock.monotonic() >= grant.leaseDeadline || clock.wall() < grant.lastLease.issuedAtUnixMs || clock.wall() >= grant.lastLease.expiresAtUnixMs)) fail("continuity_expired");
  }
  async function fresh(request: Request, uid: string, policy: PolicyState, reuseKey?: string) {
    current(policy, request);
    for (const [key, value] of observations) if (clock.monotonic() - value.mono >= 1000 || clock.wall() < value.wall) observations.delete(key);
    const credential = request.headers.get("authorization") ?? "";
    const cached = durable?.online && reuseKey ? observations.get(reuseKey) : undefined;
    let observation: Observation;
    if (cached && cached.credential === credential) observation = await cached.value;
    else {
      const observed = { wall: clock.wall(), mono: clock.monotonic() };
      const value = (async () => ({ ...observed, session: normalizeSocialBindingSession(await bounded(request,
        signal => dependencies.readSession({ accountId: uid, request }, policy.body.sessionId, signal))) }))();
      // RAM only, one original observation per participant for at most 1s.
      // In-flight sharing never advances the observation or the resulting lease.
      if (durable?.online && reuseKey && observations.size < 128) observations.set(reuseKey, { ...observed, credential, value });
      try { observation = await value; } catch (error) { if (reuseKey) observations.delete(reuseKey); throw error; }
    }
    const { session } = observation, observed = { wall: observation.wall, mono: observation.mono };
    if (session.id !== policy.body.sessionId || session.hostUid !== policy.body.accountId || session.visibility !== policy.body.visibility || session.joinPolicy !== policy.body.joinPolicy
      || session.expiresAtMs <= clock.wall() || policy.body.expiresAtUnixMs > session.expiresAtMs
      || (policy.body.schemaVersion !== 1 && policy.body.maxGuests + Number(policy.body.hostPlayer === true) > session.maxPlayers)
      || (policy.body.schemaVersion === 4 && (SOCIAL_RUNTIME_PROFILES[policy.body.runtimeProfile!][0] !== session.minecraftVersion
        || SOCIAL_RUNTIME_PROFILES[policy.body.runtimeProfile!][1] !== session.loader.toLowerCase()))
      || await socialBindingSessionDigest(session) !== policy.body.sessionDigest) fail("session_changed");
    current(policy, request);
    if (clock.wall() < observed.wall) fail("policy_expired");
    if (uid === policy.body.accountId) policy.checkedAt = observed.wall;
    return observed;
  }
  async function sign<T extends object>(body: T, preimage: Uint8Array<ArrayBuffer>): Promise<SocialSigned<T>> {
    return { body: { ...body }, signature: hex(new Uint8Array(await crypto.subtle.sign("Ed25519", keys.privateKey, preimage))) };
  }
  const copySigned = <T extends object>(value: SocialSigned<T>): SocialSigned<T> => ({ body: { ...value.body }, signature: value.signature });
  async function authenticated(request: Request): Promise<string> {
    const uid = await bounded(request, signal => dependencies.trustedAuthenticate(request, signal));
    if (!matches(UID, uid)) fail("unauthenticated"); accountAttempt(uid); return uid;
  }
  function stopPolicy(policy: PolicyState) {
    policy.state = "terminal";
    for (const grant of grants.values()) if (grant.policy === policy) {
      grant.terminal = true; grant.invitation = undefined; grant.signedLease = undefined; grant.initialLease = undefined;
    }
  }
  async function ownedPolicy(request: Request, id: string): Promise<PolicyState> {
    const uid = await authenticated(request);
    const policy = policies.get(id);
    if (!policy || policy.body.accountId !== uid) fail("broker_unavailable");
    return policy;
  }
  async function ownedGrant(request: Request, id: string, role: "host" | "guest"): Promise<GrantState> {
    const uid = await authenticated(request);
    const grant = grants.get(id);
    if (!grant || (role === "host" ? grant.body.hostAccountId : grant.body.guestAccountId) !== uid) fail("broker_unavailable");
    return grant;
  }
  async function lease(grant: GrantState, request: Request, observed: { wall: number; mono: number }, first = false) {
    live(grant, request, !first);
    const previous = grant.lastLease;
    const now = observed.wall, mono = observed.mono;
    const recent = () => { if (clock.wall() < now || clock.wall() >= now + 3_000 || clock.monotonic() >= mono + 3_000) fail("continuity_expired"); };
    recent();
    const body = validateSocialContinuityLease({ schemaVersion: 1, purpose: "socialContinuity", issuerInstance, grantId: grant.body.grantId,
      policyDigest: grant.body.policyDigest, sequence: grant.sequence + 1, issuedAtUnixMs: now, expiresAtUnixMs: Math.min(now + 3_000, grant.body.sessionExpiresAtUnixMs) });
    const result = await sign(body, socialContinuityPreimage(body));
    live(grant, request, !first); recent();
    if (grant.lastLease !== previous) fail("grant_terminal");
    grant.sequence = body.sequence; grant.lastLease = body; grant.leaseDeadline = Math.min(grant.deadline, mono + body.expiresAtUnixMs - now);
    grant.signedLease = copySigned(result);
    return result;
  }
  function matching(receipt: SocialBindingReceipt, policy: HostSocialPolicy) {
    return receipt.sessionId === policy.sessionId && receipt.sessionDigest === policy.sessionDigest;
  }
  return {
    admissionPublicKey, issuerInstance,
    async relayIdentity(request: Request, input: unknown) {
      attempt(); exact(input, ["role", "id"]); if (!matches(HEX, input.id)) fail("invalid_request");
      if (input.role === "host") {
        const policy = await ownedPolicy(request, input.id); await fresh(request, policy.body.accountId, policy); current(policy, request);
        if (policy.body.schemaVersion === 1) fail("invalid_request");
        return { sessionId: policy.body.sessionId, issuerInstance, peerId: policy.body.publicKey, hostPeerId: policy.body.publicKey,
          accountId: policy.body.accountId, generation: policy.body.generation, issuedAtUnixMs: policy.body.issuedAtUnixMs, expiresAtUnixMs: policy.body.expiresAtUnixMs };
      }
      if (input.role !== "guest") fail("invalid_request");
      const grant = await ownedGrant(request, input.id, "guest"); await fresh(request, grant.body.guestAccountId, grant.policy); live(grant, request);
      if (grant.body.schemaVersion === 1) fail("invalid_request");
      return { sessionId: grant.body.sessionId, issuerInstance, peerId: grant.body.guestPublicKey, hostPeerId: grant.body.hostPublicKey,
        accountId: grant.body.guestAccountId, generation: grant.body.guestGeneration, issuedAtUnixMs: grant.body.issuedAtUnixMs, expiresAtUnixMs: grant.body.sessionExpiresAtUnixMs };
    },
    /** Private durable adapter state, including consumed slots and revocations. */
    durableState(): SocialAdmissionState { prune(); return structuredClone({ policies: [...policies],
      grants: [...grants].map(([id, { policy, ...value }]) => [id, { ...value, policyId: policy.body.policyId }] as [string, Omit<GrantState, "policy"> & { policyId: string }]),
      entryWindow, continuityWindow, accountRates: [...accountRates] }); },
    async identity(request: Request, input: unknown) {
      attempt(); exact(input, []);
      // Identity is not a registration receipt or authorization. Challenge/context freshly check registration.
      return { accountId: await authenticated(request), issuerInstance };
    },
    async resolveContext(request: Request, input: unknown): Promise<SocialSigned<SocialJoinContext>> {
      attempt(); exact(input, ["sessionId"]); if (!matches(UUID, input.sessionId)) fail("invalid_request");
      // Authentication and the caller-authorized projection precede any lookup of this session's policy.
      const uid = await bounded(request, signal => dependencies.trustedAuthenticate(request, signal));
      if (!matches(UID, uid)) fail("unauthenticated"); accountAttempt(uid);
      const observed = { wall: clock.wall(), mono: clock.monotonic() };
      try {
        const session = normalizeSocialBindingSession(await bounded(request, signal => dependencies.readSession({ accountId: uid, request }, input.sessionId as string, signal)));
        if (session.id !== input.sessionId || session.hostUid === uid || (session.visibility !== "public" && session.visibility !== "friends")
          || !["open", "approval"].includes(session.joinPolicy) || session.expiresAtMs <= clock.wall()) fail("context_unavailable");
        const digest = await socialBindingSessionDigest(session);
        const candidates = Array.from(policies.values()).filter(value => value.body.sessionId === session.id && value.body.accountId === session.hostUid && value.state === "open");
        if (candidates.length !== 1) fail("context_unavailable");
        const policy = candidates[0];
        if (policy.deniedAccounts?.includes(uid)) fail("context_unavailable");
        if (policy.body.sessionDigest !== digest || policy.body.visibility !== session.visibility) fail("context_unavailable");
        const deadline = Math.min(policy.deadline, observed.mono + 10_000);
        const expires = Math.min(policy.body.expiresAtUnixMs, session.expiresAtMs, observed.wall + Math.floor(deadline - observed.mono));
        const currentContext = () => {
          current(policy, request);
          if (policies.get(policy.body.policyId) !== policy || policy.state !== "open" || clock.monotonic() >= deadline
            || clock.wall() < observed.wall || clock.wall() >= expires) fail("context_unavailable");
        };
        currentContext();
        const body = validateSocialJoinContext({ schemaVersion: 1, purpose: "socialJoinContext", issuerInstance, recipientAccountId: uid,
          sessionId: session.id, sessionDigest: digest, hostAccountId: policy.body.accountId, hostPublicKey: policy.body.publicKey,
          hostOperationId: policy.body.operationId, hostGeneration: policy.body.generation, policyId: policy.body.policyId, policyDigest: policy.digest,
          issuedAtUnixMs: observed.wall, expiresAtUnixMs: expires });
        const result = await sign(body, socialJoinContextPreimage(body)); currentContext(); return result;
      } catch (error) {
        if (error instanceof SocialBindingError && ["unauthenticated", "rate_limited", "authority_busy", "binding_cancelled", "backend_timeout", "backend_unavailable"].includes(error.code)) throw error;
        return fail("context_unavailable");
      }
    },
    async registerHost(request: Request, input: unknown) {
      attempt(); exact(input, ["proof", "policySigned"]); const signed = signedPolicy(input.policySigned), body = signed.body;
      if ((body.schemaVersion !== 1 && durable?.online !== true) || body.issuerInstance !== issuerInstance || policies.has(body.policyId)) fail("policy_unavailable");
      if (policies.size >= 32) fail("authority_busy");
      if (Array.from(policies.values()).some(value => value.body.accountId === body.accountId && value.body.sessionId === body.sessionId)) fail("policy_unavailable");
      const state: PolicyState = { body, digest: "", deadline: clock.monotonic() + Math.max(0, body.expiresAtUnixMs - clock.wall()), state: "registering" };
      policies.set(body.policyId, state);
      try {
        current(state, request);
        const retained = await binding.proveRetained(request, input.proof), receipt = retained.receipt; accountAttempt(receipt.accountId);
        if (body.schemaVersion === 1) state.deadline = Math.min(state.deadline, retained.originalDeadlineMonotonic); current(state, request);
        const proof = input.proof as { challenge: { challengeId: string; issuedAtUnixMs: number } };
        if (receipt.role !== "host" || body.policyId !== proof.challenge.challengeId || body.accountId !== receipt.accountId || body.publicKey !== receipt.publicKey
          || body.operationId !== receipt.operationId || body.generation !== receipt.generation || !matching(receipt, body)
          || body.issuedAtUnixMs !== proof.challenge.issuedAtUnixMs || (body.schemaVersion === 1 && body.expiresAtUnixMs > receipt.expiresAtUnixMs)) fail("policy_binding_mismatch");
        for (const other of policies.values()) if (other !== state && other.state !== "registering" && other.body.accountId === body.accountId && other.body.sessionId === body.sessionId) fail("policy_unavailable");
        const key = await crypto.subtle.importKey("raw", unhex(body.publicKey), "Ed25519", false, ["verify"]);
        if (!await crypto.subtle.verify("Ed25519", key, unhex(signed.signature), hostSocialPolicyPreimage(body))) fail("invalid_signature");
        await fresh(request, receipt.accountId, state); state.digest = await hostSocialPolicyDigest(body); current(state, request);
        // Registration is serialized at publication too; neither async contender can publish a duplicate host/session.
        for (const other of policies.values()) if (other !== state && other.state !== "registering" && other.body.accountId === body.accountId && other.body.sessionId === body.sessionId) fail("policy_unavailable");
        state.state = "open"; return { policyId: body.policyId, policyDigest: state.digest, expiresAtUnixMs: body.expiresAtUnixMs };
      } catch (error) { policies.delete(body.policyId); throw error; }
    },
    async join(request: Request, input: unknown) {
      attempt(); exact(input, ["proof", "policyId"]); if (!matches(HEX, input.policyId)) fail("invalid_policy");
      const policy = policies.get(input.policyId); if (!policy || policy.state !== "open") fail("policy_unavailable");
      current(policy, request);
      const startWall = clock.wall(), startMono = clock.monotonic();
      const retained = await binding.proveRetained(request, input.proof), receipt = retained.receipt; accountAttempt(receipt.accountId);
      current(policy, request);
      if (receipt.role !== "guest" || !matching(receipt, policy.body) || receipt.accountId === policy.body.accountId || receipt.publicKey === policy.body.publicKey) fail("guest_binding_mismatch");
      if (policy.state !== "open") fail("policy_unavailable");
      const online = policy.body.schemaVersion !== 1;
      if (online && policy.deniedAccounts?.includes(receipt.accountId)) fail("session_not_authorized");
      const activeGuests = [...grants.values()].filter(value => value.policy === policy && !value.terminal && value.body.sessionExpiresAtUnixMs > clock.wall());
      const pendingIdentity = { accountId: receipt.accountId, publicKey: receipt.publicKey };
      // Reserve identity and lifetime capacity before the first asynchronous fresh
      // read. A consumed proof alone does not serialize different keys of one UID.
      const pendingCount = [...policies.values()].reduce((count, value) => count + (value.pendingIdentities?.length ?? 0), 0);
      if (online && (grants.size + pendingCount >= (policy.body.hostPlayer ? 127 : 128) || activeGuests.length + (policy.pendingGuests ?? 0) >= policy.body.maxGuests
        || activeGuests.some(value => value.body.guestAccountId === receipt.accountId || value.body.guestPublicKey === receipt.publicKey)
        || policy.pendingIdentities?.some(value => value.accountId === receipt.accountId || value.publicKey === receipt.publicKey))) fail("authority_busy");
      // Each proof is consumed; only explicitly departed v2 participants free capacity.
      if (online) { policy.pendingGuests = (policy.pendingGuests ?? 0) + 1; (policy.pendingIdentities ??= []).push(pendingIdentity); } else policy.state = "reserved";
      let state: GrantState | undefined;
      try {
        const observed = await fresh(request, receipt.accountId, policy);
        if (policy.deniedAccounts?.includes(receipt.accountId)) fail("session_not_authorized");
        const now = observed.wall;
        const originalDeadline = policy.body.schemaVersion !== 1 ? policy.deadline : Math.min(policy.deadline, retained.originalDeadlineMonotonic, startMono + receipt.expiresAtUnixMs - startWall);
        const expires = online ? policy.body.expiresAtUnixMs : Math.min(policy.body.expiresAtUnixMs, receipt.expiresAtUnixMs, clock.wall() + Math.floor(originalDeadline - clock.monotonic()));
        const body = validateSocialAdmissionGrant({ schemaVersion: policy.body.schemaVersion, purpose: "socialAdmission", issuerInstance, grantId: hex(crypto.getRandomValues(new Uint8Array(32))),
          policyId: policy.body.policyId, policyDigest: policy.digest, sessionId: policy.body.sessionId, sessionDigest: policy.body.sessionDigest,
          hostAccountId: policy.body.accountId, hostPublicKey: policy.body.publicKey, hostOperationId: policy.body.operationId, hostGeneration: policy.body.generation,
          guestAccountId: receipt.accountId, guestPublicKey: receipt.publicKey, guestOperationId: receipt.operationId, guestGeneration: receipt.generation,
          issuedAtUnixMs: now, admissionExpiresAtUnixMs: Math.min(now + 10_000, expires, receipt.expiresAtUnixMs), sessionExpiresAtUnixMs: expires,
          ...([3, 4].includes(policy.body.schemaVersion) ? { voiceProfile: policy.body.voiceProfile, voiceSourceDigest: policy.body.voiceSourceDigest, hostPlayer: policy.body.hostPlayer } : {}),
          ...(policy.body.schemaVersion === 4 ? { runtimeProfile: policy.body.runtimeProfile } : {}) });
        state = { body, policy, deadline: Math.min(originalDeadline, startMono + expires - startWall),
          admissionDeadline: Math.min(originalDeadline, observed.mono + body.admissionExpiresAtUnixMs - now),
          terminal: false, busy: false, sequence: 0, leaseDeadline: 0, attempts: [] };
        grants.set(body.grantId, state); live(state, request, false);
        const grant = await sign(body, socialAdmissionPreimage(body)); const firstLease = await lease(state, request, observed, true); live(state, request);
        if (policy.deniedAccounts?.includes(receipt.accountId)) fail("session_not_authorized");
        state.signedGrant = copySigned(grant); state.initialLease = copySigned(firstLease);
        return { grant, lease: firstLease };
      } catch (error) { if (!online) policy.state = "terminal"; if (state) state.terminal = true; throw error; }
      finally { if (online) {
        policy.pendingGuests = Math.max(0, (policy.pendingGuests ?? 1) - 1);
        policy.pendingIdentities = policy.pendingIdentities?.filter(value => value !== pendingIdentity);
      } }
    },
    async continuity(request: Request, input: unknown) {
      attempt(); exact(input, ["grantId"]); if (!matches(HEX, input.grantId)) fail("invalid_grant");
      const state = grants.get(input.grantId); if (!state) fail("grant_terminal");
      try {
        live(state, request);
        const now = clock.monotonic();
        if (now >= continuityWindow.until) continuityWindow = { until: now + 60_000, count: 0 };
        if (++continuityWindow.count > (durable?.online ? 2048 : 256)) fail("rate_limited");
        state.attempts = state.attempts.filter(time => now - time < 1_000);
        if (state.busy || state.attempts.length >= 4) fail("rate_limited");
        state.attempts.push(now); state.busy = true;
        const uid = await bounded(request, signal => dependencies.trustedAuthenticate(request, signal));
        if (!matches(UID, uid) || uid !== state.body.guestAccountId) fail("account_mismatch"); accountAttempt(uid);
        const observed = await fresh(request, uid, state.policy, state.body.grantId); live(state, request);
        return await lease(state, request, observed);
      } catch (error) { state.terminal = true; if (state.policy.body.schemaVersion === 1) state.policy.state = "terminal"; throw error; }
      finally { state.busy = false; }
    },
    async hostPoll(request: Request, input: unknown) {
      attempt(); exact(input, ["policyId"]); if (!matches(HEX, input.policyId)) fail("invalid_request");
      const policy = await ownedPolicy(request, input.policyId);
      try {
        current(policy, request); await fresh(request, policy.body.accountId, policy, `host:${policy.body.policyId}`); current(policy, request);
        if (policy.body.schemaVersion !== 1) {
          const deliveries = [];
          for (const grant of grants.values()) if (grant.policy === policy && !grant.terminal) {
            try { live(grant, request); } catch { grant.terminal = true; continue; }
            if (grant.signedGrant && grant.initialLease && grant.signedLease) deliveries.push({ grant: copySigned(grant.signedGrant), initialLease: copySigned(grant.initialLease), lease: copySigned(grant.signedLease) });
          }
          return { state: "group" as const, checkedAtUnixMs: policy.checkedAt!, deliveries };
        }
        const grant = Array.from(grants.values()).find(value => value.policy === policy);
        if (!grant?.signedGrant || !grant.initialLease || !grant.signedLease) return { state: "waiting" as const };
        live(grant, request);
        // Reading a mailbox never creates/renews a lease. Only the guest's fresh continuity request does.
        return { state: "admitted" as const, grant: copySigned(grant.signedGrant), initialLease: copySigned(grant.initialLease), lease: copySigned(grant.signedLease) };
      } catch (error) { stopPolicy(policy); throw error; }
    },
    async hostPublish(request: Request, input: unknown) {
      attempt(); exact(input, ["grantId", "invitation"]);
      if (!matches(HEX, input.grantId) || typeof input.invitation !== "string" || !input.invitation.length
        || encoder.encode(input.invitation).length > 8_192) fail("invalid_request");
      const invitation = input.invitation;
      const grant = await ownedGrant(request, input.grantId, "host");
      if (grant.invitation !== undefined || grant.publishing) fail("invitation_already_published");
      grant.publishing = true;
      try {
        live(grant, request);
        if (clock.monotonic() >= grant.admissionDeadline || clock.wall() >= grant.body.admissionExpiresAtUnixMs) fail("admission_expired");
        await fresh(request, grant.body.hostAccountId, grant.policy); live(grant, request);
        if (clock.monotonic() >= grant.admissionDeadline || clock.wall() >= grant.body.admissionExpiresAtUnixMs) fail("admission_expired");
        // This is an opaque, untrusted transport envelope. Native verifies its signature, exact grant,
        // game/TCP service, endpoint policy and original deadlines before opening any connection.
        grant.invitation = invitation;
        return { accepted: true as const };
      } catch (error) { if (grant.policy.body.schemaVersion !== 1) grant.terminal = true; else stopPolicy(grant.policy); throw error; }
      finally { grant.publishing = false; }
    },
    async guestReceive(request: Request, input: unknown) {
      attempt(); exact(input, ["grantId"]); if (!matches(HEX, input.grantId)) fail("invalid_request");
      const grant = await ownedGrant(request, input.grantId, "guest");
      try {
        live(grant, request);
        if (clock.monotonic() >= grant.admissionDeadline || clock.wall() >= grant.body.admissionExpiresAtUnixMs) fail("admission_expired");
        await fresh(request, grant.body.guestAccountId, grant.policy); live(grant, request);
        if (clock.monotonic() >= grant.admissionDeadline || clock.wall() >= grant.body.admissionExpiresAtUnixMs) fail("admission_expired");
        if (grant.invitation === undefined) return { state: "waiting" as const };
        if (!grant.signedLease) fail("continuity_expired");
        return { state: "ready" as const, invitation: grant.invitation, lease: copySigned(grant.signedLease) };
      } catch (error) { if (grant.policy.body.schemaVersion !== 1) grant.terminal = true; else stopPolicy(grant.policy); throw error; }
    },
    async hostStop(request: Request, input: unknown) {
      attempt(); exact(input, ["policyId"]); if (!matches(HEX, input.policyId)) fail("invalid_request");
      const policy = await ownedPolicy(request, input.policyId); stopPolicy(policy); return { stopped: true as const };
    },
    async hostRevoke(request: Request, input: unknown) {
      attempt(); exact(input, ["grantId"]); if (!matches(HEX, input.grantId)) fail("invalid_request");
      const grant = await ownedGrant(request, input.grantId, "host"), policy = grant.policy;
      if (policy.body.schemaVersion === 1) fail("invalid_request");
      current(policy, request);
      // Claim synchronously before any further I/O. Revocation can never be
      // undone by a delayed join/signing result, a new key or a refreshed token.
      const denied = policy.deniedAccounts ??= [];
      if (!denied.includes(grant.body.guestAccountId)) {
        if (denied.length >= 128) fail("authority_busy"); denied.push(grant.body.guestAccountId);
      }
      for (const participant of grants.values()) if (participant.policy === policy && participant.body.guestAccountId === grant.body.guestAccountId) {
        participant.terminal = true; participant.invitation = undefined; participant.signedLease = undefined; participant.initialLease = undefined;
      }
      observations.delete(grant.body.grantId);
      return { revoked: true as const };
    },
    async guestStop(request: Request, input: unknown) {
      attempt(); exact(input, ["grantId"]); if (!matches(HEX, input.grantId)) fail("invalid_request");
      const grant = await ownedGrant(request, input.grantId, "guest");
      if (grant.policy.body.schemaVersion !== 1) { grant.terminal = true; grant.invitation = undefined; } else stopPolicy(grant.policy); return { stopped: true as const };
    },
    inspect() { prune(); return { policies: policies.size, grants: grants.size, terminalGrants: Array.from(grants.values()).filter(value => value.terminal).length }; },
  };
}
