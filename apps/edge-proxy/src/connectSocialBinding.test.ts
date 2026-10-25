import assert from "node:assert/strict";
import test from "node:test";
import { createSocialBindingAuthority, normalizeSocialBindingSession, socialBindingSessionDigest, socialKeyProofPreimage, validateSocialKeyChallenge, SocialBindingError } from "./connectSocialBinding.ts";
import type { SocialBindingSession, SocialKeyChallenge } from "./connectSocialBinding.ts";

const sid = "00000000-0000-4000-8000-000000000001";
const operationId = "00000000-0000-4000-8000-000000000002";
const request = (uid = "guest") => new Request("http://127.0.0.1/proof", { headers: { Authorization: `Bearer fixture-${uid}` } });
const code = (expected: string) => (error: unknown) => error instanceof SocialBindingError && error.code === expected;
async function fixture() {
  let wall = 1_800_000_000_000, mono = 100;
  let reads = 0, blocked = false;
  const session: SocialBindingSession = { id: sid, hostUid: "host", title: "Mundo", modpackName: "Create", minecraftVersion: "1.21.1", loader: "Fabric", imageUrl: null,
    players: 1, maxPlayers: 2, visibility: "public", joinPolicy: "open", expiresAtMs: wall + 300_000 };
  const keys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
  const publicKey = Buffer.from(await crypto.subtle.exportKey("raw", keys.publicKey)).toString("hex");
  let onRead: (() => Promise<void>) | undefined;
  const dependencies = {
    clock: { wall: () => wall, monotonic: () => mono },
    trustedAuthenticate: async (req: Request) => {
      const value = req.headers.get("Authorization");
      if (!value?.startsWith("Bearer fixture-")) throw new SocialBindingError("unauthenticated");
      return value.slice("Bearer fixture-".length);
    },
    readSession: async () => { reads++; await onRead?.(); if (blocked) throw new SocialBindingError("session_not_authorized"); return { ...session }; },
  };
  const authority = createSocialBindingAuthority(dependencies);
  const input = { schemaVersion: 1, operationId, generation: 1, role: "guest", publicKey, sessionId: sid, preparedExpiresAtUnixMs: wall + 60_000 };
  const sign = async (challenge: SocialKeyChallenge) => ({ schemaVersion: 1, challenge,
    signature: Buffer.from(await crypto.subtle.sign("Ed25519", keys.privateKey, socialKeyProofPreimage(challenge))).toString("hex") });
  return { authority, dependencies, input, session, sign, keys,
    advance: (ms: number) => { wall += ms; mono += ms; }, wallOnly: (ms: number) => { wall += ms; },
    monoOnly: (ms: number) => { mono += ms; }, block: () => { blocked = true; }, reads: () => reads,
    onRead: (fn: () => Promise<void>) => { onRead = fn; } };
}

test("real Ed25519 account/key proof returns an expiring non-transport receipt and consumes challenge", async () => {
  const f = await fixture(); const challenge = await f.authority.challenge(request(), f.input);
  assert.equal(challenge.accountId, "guest"); assert.equal(challenge.issuerInstance, f.authority.issuerInstance);
  assert.equal(challenge.expiresAtUnixMs - challenge.issuedAtUnixMs, 60_000);
  const proof = await f.sign(challenge); const receipt = await f.authority.prove(request(), proof);
  assert.equal(receipt.publicKey, f.input.publicKey); assert.equal(receipt.transportAuthorized, false);
  assert.equal(receipt.operationId, challenge.operationId); assert.equal(receipt.generation, challenge.generation);
  assert.equal(f.reads(), 2); assert.equal(f.authority.inspect().receipts, 1);
  await assert.rejects(f.authority.prove(request(), proof), code("challenge_unavailable"));
  f.advance(60_000); assert.equal(f.authority.inspect().receipts, 0);
});
test("host role requires session ownership; guest cannot impersonate host", async () => {
  const f = await fixture(); await assert.rejects(f.authority.challenge(request(), { ...f.input, role: "host" }), code("session_not_authorized"));
  await assert.rejects(f.authority.challenge(request("host"), f.input), code("session_not_authorized"));
  const challenge = await f.authority.challenge(request("host"), { ...f.input, role: "host" });
  assert.equal((await f.authority.prove(request("host"), await f.sign(challenge))).accountId, "host");
});
test("closed input rejects client account selection, URLs and unknown fields", async () => {
  const f = await fixture();
  for (const extension of [{ accountId: "host" }, { issuerInstance: "a".repeat(64) }, { project: "other" }, { backend: "https://evil.test" }]) {
    await assert.rejects(f.authority.challenge(request(), { ...f.input, ...extension }), code("invalid_shape"));
  }
  for (const extension of [{ schemaVersion: 2 }, { generation: 0 }, { generation: 1.5 }, { generation: Number.MAX_SAFE_INTEGER + 1 }, { role: "admin" }, { role: ["host"] }, { publicKey: "A".repeat(64) }, { sessionId: "../../users" }]) {
    await assert.rejects(f.authority.challenge(request(), { ...f.input, ...extension }), code("invalid_input"));
  }
});
test("proof is bound to original issuer, operation, session, role, account and key", async () => {
  for (const extension of [{ operationId: "00000000-0000-4000-8000-000000000003" }, { generation: 2 }, { accountId: "host" }, { role: "host" }, { publicKey: "a".repeat(64) }, { sessionDigest: "b".repeat(64) }]) {
    const f = await fixture(); const challenge = await f.authority.challenge(request(), f.input);
    const changed = { ...challenge, ...extension } as SocialKeyChallenge;
    await assert.rejects(f.authority.prove(request(), await f.sign(changed)), code("challenge_mismatch"));
    await assert.rejects(f.authority.prove(request(), await f.sign(challenge)), code("challenge_unavailable"));
  }
  const f = await fixture(), other = createSocialBindingAuthority(f.dependencies);
  const challenge = await f.authority.challenge(request(), f.input);
  await assert.rejects(other.prove(request(), await f.sign(challenge)), code("challenge_unavailable"));
});
test("a different authenticated account cannot consume a valid proof for itself", async () => {
  const f = await fixture(); const challenge = await f.authority.challenge(request(), f.input);
  await assert.rejects(f.authority.prove(request("host"), await f.sign(challenge)), code("account_mismatch"));
  assert.equal(f.authority.inspect().pending, 0);
});
test("invalid signature consumes challenge; caller never chooses a generic signing payload", async () => {
  const f = await fixture(); const challenge = await f.authority.challenge(request(), f.input);
  await assert.rejects(f.authority.prove(request(), { schemaVersion: 1, challenge, signature: "0".repeat(128) }), code("invalid_signature"));
  await assert.rejects(f.authority.prove(request(), await f.sign(challenge)), code("challenge_unavailable"));
});
test("concurrent duplicate proof has exactly one winner", async () => {
  const f = await fixture(); const proof = await f.sign(await f.authority.challenge(request(), f.input));
  const attempts = await Promise.allSettled([f.authority.prove(request(), proof), f.authority.prove(request(), proof)]);
  assert.equal(attempts.filter(value => value.status === "fulfilled").length, 1);
  assert.equal(attempts.filter(value => value.status === "rejected" && value.reason.code === "challenge_unavailable").length, 1);
});
test("fresh read denies block or lost authorization after issue", async () => {
  const f = await fixture(); const challenge = await f.authority.challenge(request(), f.input); f.block();
  await assert.rejects(f.authority.prove(request(), await f.sign(challenge)), code("session_not_authorized"));
  assert.equal(f.authority.inspect().receipts, 0);
});
test("all session metadata including expiry participates in digest", async () => {
  for (const extension of [{ visibility: "private" }, { joinPolicy: "approval" }, { title: "Novo" }, { players: 2 }, { expiresAtMs: 1_800_000_250_000 }]) {
    const f = await fixture(); const challenge = await f.authority.challenge(request(), f.input); Object.assign(f.session, extension);
    await assert.rejects(f.authority.prove(request(), await f.sign(challenge)), code("session_changed"));
  }
});
test("original wall and monotonic deadlines both apply, including clock rollback", async () => {
  for (const change of [(f: Awaited<ReturnType<typeof fixture>>) => f.advance(60_000),
    (f: Awaited<ReturnType<typeof fixture>>) => f.wallOnly(-1), (f: Awaited<ReturnType<typeof fixture>>) => f.monoOnly(60_001)]) {
    const f = await fixture(); const challenge = await f.authority.challenge(request(), f.input); change(f);
    await assert.rejects(f.authority.prove(request(), await f.sign(challenge)), code("challenge_unavailable"));
  }
});
test("expiry during the last backend read rejects instead of publishing receipt", async () => {
  const f = await fixture(); const challenge = await f.authority.challenge(request(), f.input);
  f.onRead(async () => { f.advance(60_001); });
  await assert.rejects(f.authority.prove(request(), await f.sign(challenge)), code("challenge_expired"));
  assert.equal(f.authority.inspect().receipts, 0);
});
test("session's shorter lifetime cannot be extended by the proof", async () => {
  const f = await fixture(); f.session.expiresAtMs = 1_800_000_001_000;
  const challenge = await f.authority.challenge(request(), f.input); assert.equal(challenge.expiresAtUnixMs - challenge.issuedAtUnixMs, 1_000);
  f.advance(100); assert.equal((await f.authority.prove(request(), await f.sign(challenge))).expiresAtUnixMs, challenge.expiresAtUnixMs);
});
test("four pending per account, 128 global, no eviction of live challenges", async () => {
  const f = await fixture();
  for (let i = 0; i < 4; i++) await f.authority.challenge(request(), f.input);
  await assert.rejects(f.authority.challenge(request(), f.input), code("authority_busy"));
  for (let i = 0; i < 124; i++) await f.authority.challenge(request(`guest${i}`), f.input);
  await assert.rejects(f.authority.challenge(request("last"), f.input), code("authority_busy"));
  assert.equal(f.authority.inspect().pending, 128);
});
test("pending limits reserve slots before concurrent backend work", async () => {
  const f = await fixture(); let unblock!: () => void; const blocked = new Promise<void>(resolve => { unblock = resolve; });
  f.onRead(() => blocked);
  const attempts = Array.from({ length: 5 }, () => f.authority.challenge(request(), f.input));
  const settled = Promise.allSettled(attempts); await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.authority.inspect().pending, 4); unblock();
  const results = await settled; assert.equal(results.filter(result => result.status === "fulfilled").length, 4);
});
test("per-account and pre-auth global attempt budgets fail closed", async () => {
  const f = await fixture();
  for (let i = 0; i < 20; i++) await assert.rejects(f.authority.challenge(request(), { ...f.input, role: "host" }), code("session_not_authorized"));
  await assert.rejects(f.authority.challenge(request(), f.input), code("rate_limited"));
  const g = await fixture(); for (let i = 0; i < 512; i++) await assert.rejects(g.authority.challenge(request(), {}), code("invalid_shape"));
  await assert.rejects(g.authority.challenge(request(), g.input), code("rate_limited"));
});
test("canonical codec has domain and u16 BE fields independent of JSON key order", async () => {
  const f = await fixture(); const c = await f.authority.challenge(request(), f.input);
  const reordered = Object.fromEntries(Object.entries(c).reverse());
  assert.deepEqual(socialKeyProofPreimage(c), socialKeyProofPreimage(reordered));
  const bytes = Buffer.from(socialKeyProofPreimage(c));
  const domain = Buffer.from("aurora/connect/social-key-proof/v1\0");
  assert.deepEqual(bytes.subarray(0, domain.length), domain);
  assert.equal(bytes.readUInt16BE(domain.length), 1); assert.equal(bytes[domain.length + 2], 49);
  assert.equal(bytes.readUInt16BE(domain.length + 3), 17);
  assert.throws(() => validateSocialKeyChallenge({ ...c, expiresAtUnixMs: c.issuedAtUnixMs + 60_001 }), code("invalid_challenge"));
  assert.throws(() => validateSocialKeyChallenge({ ...c, extra: null }), code("invalid_shape"));
  assert.throws(() => validateSocialKeyChallenge({ ...c, role: ["host"] }), code("invalid_challenge"));
});
test("session normalization is closed and deterministic; transport details rejected", async () => {
  const f = await fixture();
  assert.equal(await socialBindingSessionDigest(f.session), await socialBindingSessionDigest(Object.fromEntries(Object.entries(f.session).reverse())));
  assert.throws(() => normalizeSocialBindingSession({ ...f.session, endpoint: "127.0.0.1:43001" }), code("invalid_shape"));
  assert.throws(() => normalizeSocialBindingSession({ ...f.session, imageUrl: "https://evil.test/a.png" }), code("invalid_session"));
  assert.throws(() => normalizeSocialBindingSession({ ...f.session, visibility: ["public"] }), code("invalid_session"));
  assert.throws(() => normalizeSocialBindingSession({ ...f.session, joinPolicy: ["open"] }), code("invalid_session"));
});
test("preparation deadline is a cap, including backend delay, never a fresh 60 seconds", async () => {
  const f = await fixture(); f.onRead(async () => { f.advance(5_000); });
  const challenge = await f.authority.challenge(request(), { ...f.input, preparedExpiresAtUnixMs: 1_800_000_030_000 });
  assert.equal(challenge.expiresAtUnixMs, 1_800_000_030_000); assert.equal(challenge.expiresAtUnixMs - challenge.issuedAtUnixMs, 25_000);
  await assert.rejects(f.authority.challenge(request(), { ...f.input, preparedExpiresAtUnixMs: 1_800_000_065_001 }), code("preparation_expired"));
  await assert.rejects(f.authority.challenge(request(), { ...f.input, preparedExpiresAtUnixMs: 1_800_000_000_000 }), code("preparation_expired"));
});
test("cancellation propagates to backend and prevents proof receipt after client disconnect", async () => {
  const f = await fixture(); const challenge = await f.authority.challenge(request(), f.input);
  const controller = new AbortController(); let observedAbort = false;
  const authority = createSocialBindingAuthority({ ...f.dependencies,
    readSession: async (_who, _id, signal) => { await new Promise<void>(resolve => { signal.addEventListener("abort", () => { observedAbort = true; resolve(); }, { once: true }); controller.abort(); }); return f.session; } });
  const aborted = new Request("http://127.0.0.1/proof", { headers: { Authorization: "Bearer fixture-guest" }, signal: controller.signal });
  await assert.rejects(authority.challenge(aborted, f.input), code("binding_cancelled"));
  assert.equal(observedAbort, true); assert.equal(authority.inspect().pending, 0);
  const proofController = new AbortController(); f.onRead(async () => { proofController.abort(); });
  const proofRequest = new Request("http://127.0.0.1/proof", { headers: { Authorization: "Bearer fixture-guest" }, signal: proofController.signal });
  await assert.rejects(f.authority.prove(proofRequest, await f.sign(challenge)), code("binding_cancelled"));
  assert.equal(f.authority.inspect().receipts, 0);
});
test("internal retained verification transfers original monotonic cap without exposing it in HTTP receipt", async () => {
  const f = await fixture(); const challenge = await f.authority.challenge(request(), f.input);
  f.advance(10_000); f.wallOnly(-5_000);
  const proof = await f.sign(challenge), retained = await f.authority.proveRetained(request(), proof);
  assert.equal(retained.originalDeadlineMonotonic, 60_100);
  assert.equal(Object.hasOwn(retained.receipt, "originalDeadlineMonotonic"), false);
  await assert.rejects(f.authority.prove(request(), proof), code("challenge_unavailable"));
});
