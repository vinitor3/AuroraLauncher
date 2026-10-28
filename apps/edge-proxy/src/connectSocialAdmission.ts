/** Local ADR-031 authority. A separate signed admission, never a promotion of an 88A receipt. */
import { normalizeSocialBindingSession, socialBindingSessionDigest, SocialBindingError } from "./connectSocialBinding.ts";
import type { createSocialBindingAuthority, SocialBindingDependencies, SocialBindingReceipt } from "./connectSocialBinding.ts";

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
const contextKeys = ["schemaVersion", "purpose", "issuerInstance", "recipientAccountId", "sessionId", "sessionDigest", "hostAccountId", "hostPublicKey", "hostOperationId", "hostGeneration", "policyId", "policyDigest", "issuedAtUnixMs", "expiresAtUnixMs"] as const;
export type HostSocialPolicy = {
  schemaVersion: 1; purpose: "hostSocialSession"; issuerInstance: string; policyId: string; accountId: string; operationId: string; generation: number;
  publicKey: string; sessionId: string; sessionDigest: string; visibility: "public" | "friends"; joinPolicy: "open"; serviceId: "game"; protocol: "tcp";
  sourceDigest: string; maxGuests: 1; issuedAtUnixMs: number; expiresAtUnixMs: number;
};
export type SocialAdmissionGrant = {
  schemaVersion: 1; purpose: "socialAdmission"; issuerInstance: string; grantId: string; policyId: string; policyDigest: string; sessionId: string; sessionDigest: string;
  hostAccountId: string; hostPublicKey: string; hostOperationId: string; hostGeneration: number; guestAccountId: string; guestPublicKey: string; guestOperationId: string; guestGeneration: number;
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
  exact(input, policyKeys);
  if (input.schemaVersion !== 1 || input.purpose !== "hostSocialSession" || !matches(HEX, input.issuerInstance) || !matches(HEX, input.policyId)
    || !matches(UID, input.accountId) || !matches(UUID, input.operationId) || !positive(input.generation) || !matches(HEX, input.publicKey)
    || !matches(UUID, input.sessionId) || !matches(HEX, input.sessionDigest) || (input.visibility !== "public" && input.visibility !== "friends")
    || input.joinPolicy !== "open" || input.serviceId !== "game" || input.protocol !== "tcp" || !matches(HEX, input.sourceDigest) || input.maxGuests !== 1
    || !positive(input.issuedAtUnixMs) || !positive(input.expiresAtUnixMs) || input.expiresAtUnixMs <= input.issuedAtUnixMs
    || input.expiresAtUnixMs - input.issuedAtUnixMs > 60_000) fail("invalid_policy");
  return ordered(input, policyKeys);
}
export function validateSocialAdmissionGrant(input: unknown): SocialAdmissionGrant {
  exact(input, grantKeys);
  if (input.schemaVersion !== 1 || input.purpose !== "socialAdmission" || !matches(HEX, input.issuerInstance) || !matches(HEX, input.grantId)
    || !matches(HEX, input.policyId) || !matches(HEX, input.policyDigest) || !matches(UUID, input.sessionId) || !matches(HEX, input.sessionDigest)
    || !matches(UID, input.hostAccountId) || !matches(HEX, input.hostPublicKey) || !matches(UUID, input.hostOperationId) || !positive(input.hostGeneration)
    || !matches(UID, input.guestAccountId) || !matches(HEX, input.guestPublicKey) || !matches(UUID, input.guestOperationId) || !positive(input.guestGeneration)
    || input.hostAccountId === input.guestAccountId || input.hostPublicKey === input.guestPublicKey
    || !positive(input.issuedAtUnixMs) || !positive(input.admissionExpiresAtUnixMs) || !positive(input.sessionExpiresAtUnixMs)
    || input.admissionExpiresAtUnixMs <= input.issuedAtUnixMs || input.admissionExpiresAtUnixMs > input.sessionExpiresAtUnixMs
    || input.admissionExpiresAtUnixMs - input.issuedAtUnixMs > 10_000 || input.sessionExpiresAtUnixMs - input.issuedAtUnixMs > 60_000) fail("invalid_grant");
  return ordered(input, grantKeys);
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
export const hostSocialPolicyPreimage = (input: unknown) => canonical(HOST_POLICY_DOMAIN, validateHostSocialPolicy(input), policyKeys);
export const socialAdmissionPreimage = (input: unknown) => canonical(SOCIAL_ADMISSION_DOMAIN, validateSocialAdmissionGrant(input), grantKeys);
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
type PolicyState = { body: HostSocialPolicy; digest: string; deadline: number; state: "registering" | "open" | "reserved" | "terminal" };
type GrantState = { body: SocialAdmissionGrant; policy: PolicyState; deadline: number; admissionDeadline: number; terminal: boolean; busy: boolean; sequence: number;
  lastLease?: SocialContinuityLease; leaseDeadline: number; attempts: number[];
  signedGrant?: SocialSigned<SocialAdmissionGrant>; initialLease?: SocialSigned<SocialContinuityLease>;
  signedLease?: SocialSigned<SocialContinuityLease>; invitation?: string; publishing?: boolean };

export async function createSocialAdmissionAuthority(binding: ReturnType<typeof createSocialBindingAuthority>, dependencies: SocialBindingDependencies) {
  const clock = dependencies.clock ?? { wall: () => Date.now(), monotonic: () => performance.now() };
  const keys = await crypto.subtle.generateKey("Ed25519", false, ["sign", "verify"]);
  const admissionPublicKey = hex(new Uint8Array(await crypto.subtle.exportKey("raw", keys.publicKey)));
  const issuerInstance = binding.issuerInstance;
  const policies = new Map<string, PolicyState>(), grants = new Map<string, GrantState>();
  let entryWindow = { until: clock.monotonic() + 60_000, count: 0 }, continuityWindow = { until: clock.monotonic() + 60_000, count: 0 };
  const accountRates = new Map<string, { until: number; count: number }>();
  function prune() {
    for (const [id, value] of policies) if (value.state !== "registering" && (clock.monotonic() >= value.deadline || clock.wall() >= value.body.expiresAtUnixMs)) policies.delete(id);
    for (const [id, value] of grants) if (!value.busy && (clock.monotonic() >= value.deadline || clock.wall() >= value.body.sessionExpiresAtUnixMs)) grants.delete(id);
    for (const [id, value] of accountRates) if (clock.monotonic() >= value.until) accountRates.delete(id);
  }
  function attempt() { prune(); if (clock.monotonic() >= entryWindow.until) entryWindow = { until: clock.monotonic() + 60_000, count: 0 }; if (++entryWindow.count > 512) fail("rate_limited"); }
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
  async function fresh(request: Request, uid: string, policy: PolicyState) {
    current(policy, request);
    const observed = { wall: clock.wall(), mono: clock.monotonic() };
    const session = normalizeSocialBindingSession(await bounded(request, signal => dependencies.readSession({ accountId: uid, request }, policy.body.sessionId, signal)));
    if (session.id !== policy.body.sessionId || session.hostUid !== policy.body.accountId || session.visibility !== policy.body.visibility || session.joinPolicy !== "open"
      || session.expiresAtMs <= clock.wall() || await socialBindingSessionDigest(session) !== policy.body.sessionDigest) fail("session_changed");
    current(policy, request);
    if (clock.wall() < observed.wall) fail("policy_expired");
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
          || session.joinPolicy !== "open" || session.expiresAtMs <= clock.wall()) fail("context_unavailable");
        const digest = await socialBindingSessionDigest(session);
        const candidates = Array.from(policies.values()).filter(value => value.body.sessionId === session.id && value.body.accountId === session.hostUid && value.state === "open");
        if (candidates.length !== 1) fail("context_unavailable");
        const policy = candidates[0];
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
      if (body.issuerInstance !== issuerInstance || policies.has(body.policyId)) fail("policy_unavailable");
      if (policies.size >= 32) fail("authority_busy");
      if (Array.from(policies.values()).some(value => value.body.accountId === body.accountId && value.body.sessionId === body.sessionId)) fail("policy_unavailable");
      const state: PolicyState = { body, digest: "", deadline: clock.monotonic() + Math.max(0, body.expiresAtUnixMs - clock.wall()), state: "registering" };
      policies.set(body.policyId, state);
      try {
        current(state, request);
        const retained = await binding.proveRetained(request, input.proof), receipt = retained.receipt; accountAttempt(receipt.accountId);
        state.deadline = Math.min(state.deadline, retained.originalDeadlineMonotonic); current(state, request);
        const proof = input.proof as { challenge: { challengeId: string; issuedAtUnixMs: number } };
        if (receipt.role !== "host" || body.policyId !== proof.challenge.challengeId || body.accountId !== receipt.accountId || body.publicKey !== receipt.publicKey
          || body.operationId !== receipt.operationId || body.generation !== receipt.generation || !matching(receipt, body)
          || body.issuedAtUnixMs !== proof.challenge.issuedAtUnixMs || body.expiresAtUnixMs > receipt.expiresAtUnixMs) fail("policy_binding_mismatch");
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
      // One slot, claimed synchronously before backend IO or signing. No failure returns it to the pool.
      policy.state = "reserved";
      let state: GrantState | undefined;
      try {
        const observed = await fresh(request, receipt.accountId, policy);
        const now = observed.wall;
        const originalDeadline = Math.min(policy.deadline, retained.originalDeadlineMonotonic, startMono + receipt.expiresAtUnixMs - startWall);
        const expires = Math.min(policy.body.expiresAtUnixMs, receipt.expiresAtUnixMs, clock.wall() + Math.floor(originalDeadline - clock.monotonic()));
        const body = validateSocialAdmissionGrant({ schemaVersion: 1, purpose: "socialAdmission", issuerInstance, grantId: hex(crypto.getRandomValues(new Uint8Array(32))),
          policyId: policy.body.policyId, policyDigest: policy.digest, sessionId: policy.body.sessionId, sessionDigest: policy.body.sessionDigest,
          hostAccountId: policy.body.accountId, hostPublicKey: policy.body.publicKey, hostOperationId: policy.body.operationId, hostGeneration: policy.body.generation,
          guestAccountId: receipt.accountId, guestPublicKey: receipt.publicKey, guestOperationId: receipt.operationId, guestGeneration: receipt.generation,
          issuedAtUnixMs: now, admissionExpiresAtUnixMs: Math.min(now + 10_000, expires), sessionExpiresAtUnixMs: expires });
        state = { body, policy, deadline: Math.min(originalDeadline, startMono + expires - startWall),
          admissionDeadline: Math.min(originalDeadline, observed.mono + body.admissionExpiresAtUnixMs - now),
          terminal: false, busy: false, sequence: 0, leaseDeadline: 0, attempts: [] };
        grants.set(body.grantId, state); live(state, request, false);
        const grant = await sign(body, socialAdmissionPreimage(body)); const firstLease = await lease(state, request, observed, true); live(state, request);
        state.signedGrant = copySigned(grant); state.initialLease = copySigned(firstLease);
        return { grant, lease: firstLease };
      } catch (error) { policy.state = "terminal"; if (state) state.terminal = true; throw error; }
    },
    async continuity(request: Request, input: unknown) {
      attempt(); exact(input, ["grantId"]); if (!matches(HEX, input.grantId)) fail("invalid_grant");
      const state = grants.get(input.grantId); if (!state) fail("grant_terminal");
      try {
        live(state, request);
        const now = clock.monotonic();
        if (now >= continuityWindow.until) continuityWindow = { until: now + 60_000, count: 0 };
        if (++continuityWindow.count > 256) fail("rate_limited");
        state.attempts = state.attempts.filter(time => now - time < 1_000);
        if (state.busy || state.attempts.length >= 4) fail("rate_limited");
        state.attempts.push(now); state.busy = true;
        const uid = await bounded(request, signal => dependencies.trustedAuthenticate(request, signal));
        if (!matches(UID, uid) || uid !== state.body.guestAccountId) fail("account_mismatch"); accountAttempt(uid);
        const observed = await fresh(request, uid, state.policy); live(state, request);
        return await lease(state, request, observed);
      } catch (error) { state.terminal = true; state.policy.state = "terminal"; throw error; }
      finally { state.busy = false; }
    },
    async hostPoll(request: Request, input: unknown) {
      attempt(); exact(input, ["policyId"]); if (!matches(HEX, input.policyId)) fail("invalid_request");
      const policy = await ownedPolicy(request, input.policyId);
      try {
        current(policy, request); await fresh(request, policy.body.accountId, policy); current(policy, request);
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
      } catch (error) { stopPolicy(grant.policy); throw error; }
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
      } catch (error) { stopPolicy(grant.policy); throw error; }
    },
    async hostStop(request: Request, input: unknown) {
      attempt(); exact(input, ["policyId"]); if (!matches(HEX, input.policyId)) fail("invalid_request");
      const policy = await ownedPolicy(request, input.policyId); stopPolicy(policy); return { stopped: true as const };
    },
    async guestStop(request: Request, input: unknown) {
      attempt(); exact(input, ["grantId"]); if (!matches(HEX, input.grantId)) fail("invalid_request");
      const grant = await ownedGrant(request, input.grantId, "guest"); stopPolicy(grant.policy); return { stopped: true as const };
    },
    inspect() { prune(); return { policies: policies.size, grants: grants.size, terminalGrants: Array.from(grants.values()).filter(value => value.terminal).length }; },
  };
}
