import assert from "node:assert/strict";
import test from "node:test";
import { createSocialBindingAuthority, socialKeyProofPreimage, SocialBindingError } from "./connectSocialBinding.ts";
import { createSocialAdmissionAuthority, hostSocialPolicyDigest, hostSocialPolicyPreimage, socialAdmissionPreimage, socialContinuityPreimage,
  validateHostSocialPolicy, validateSocialAdmissionGrant, validateSocialContinuityLease } from "./connectSocialAdmission.ts";
import type { SocialBindingSession } from "./connectSocialBinding.ts";

const req = (uid = "guest", signal?: AbortSignal) => new Request("http://127.0.0.1/local", { signal, headers: { Authorization: `Bearer fixture-${uid}` } });
const code = (expected: string) => (error: unknown) => error instanceof SocialBindingError && error.code === expected;
const hex = (buffer: ArrayBuffer) => Buffer.from(buffer).toString("hex");
async function fixture() {
  let wall = 1_800_000_000_000, mono = 1_000, reads = 0;
  let hook: ((count: number) => Promise<void>) | undefined;
  const blocked = new Set<string>(); const sessions = new Map<string, SocialBindingSession>();
  const session: SocialBindingSession = { id: crypto.randomUUID(), hostUid: "host", title: "World", modpackName: "Local", minecraftVersion: "1.21.1", loader: "Fabric", imageUrl: null,
    players: 1, maxPlayers: 2, visibility: "public", joinPolicy: "open", expiresAtMs: wall + 300_000 };
  sessions.set(session.id, session);
  const dependencies = { clock: { wall: () => wall, monotonic: () => mono },
    trustedAuthenticate: async (request: Request) => { const auth = request.headers.get("Authorization"); if (!auth?.startsWith("Bearer fixture-")) throw new SocialBindingError("unauthenticated"); return auth.slice(15); },
    readSession: async (principal: { accountId: string }, id: string) => { reads++; await hook?.(reads); if (blocked.has(principal.accountId)) throw new SocialBindingError("session_not_authorized");
      const value = sessions.get(id); if (!value) throw new SocialBindingError("session_not_authorized"); return { ...value }; } };
  const binding = createSocialBindingAuthority(dependencies), authority = await createSocialAdmissionAuthority(binding, dependencies);
  async function prepared(uid = "guest", role: "guest" | "host" = "guest", selected = session) {
    const keys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
    const publicKey = hex(await crypto.subtle.exportKey("raw", keys.publicKey));
    const challenge = await binding.challenge(req(uid), { schemaVersion: 1, operationId: crypto.randomUUID(), generation: 1, role, publicKey,
      sessionId: selected.id, preparedExpiresAtUnixMs: wall + 55_000 });
    const proof = { schemaVersion: 1, challenge, signature: hex(await crypto.subtle.sign("Ed25519", keys.privateKey, socialKeyProofPreimage(challenge))) };
    const body = { schemaVersion: 1, purpose: "hostSocialSession", issuerInstance: challenge.issuerInstance, policyId: challenge.challengeId,
      accountId: uid, operationId: challenge.operationId, generation: challenge.generation, publicKey, sessionId: selected.id, sessionDigest: challenge.sessionDigest,
      visibility: selected.visibility, joinPolicy: "open", serviceId: "game", protocol: "tcp", sourceDigest: "a".repeat(64), maxGuests: 1,
      issuedAtUnixMs: challenge.issuedAtUnixMs, expiresAtUnixMs: challenge.expiresAtUnixMs };
    const policySigned = { body, signature: hex(await crypto.subtle.sign("Ed25519", keys.privateKey, hostSocialPolicyPreimage(body))) };
    return { uid, proof, policySigned, keys };
  }
  const register = async () => { const host = await prepared("host", "host"); const policy = await authority.registerHost(req("host"), { proof: host.proof, policySigned: host.policySigned }); return { host, policy }; };
  const pair = async () => { const { host, policy } = await register(); const guest = await prepared(); const packet = await authority.join(req(), { proof: guest.proof, policyId: policy.policyId }); return { host, guest, policy, packet }; };
  return { binding, authority, dependencies, session, sessions, prepared, register, pair, block: (uid = "guest") => blocked.add(uid), unblock: (uid = "guest") => blocked.delete(uid),
    readCount: () => reads, onRead: (fn: (count: number) => Promise<void>) => { hook = fn; },
    advance: (amount: number) => { wall += amount; mono += amount; }, wallOnly: (amount: number) => { wall += amount; }, monoOnly: (amount: number) => { mono += amount; } };
}
async function verify(keyHex: string, signed: { signature: string }, bytes: Uint8Array<ArrayBuffer>) {
  const key = await crypto.subtle.importKey("raw", Buffer.from(keyHex, "hex"), "Ed25519", false, ["verify"]);
  return crypto.subtle.verify("Ed25519", key, Buffer.from(signed.signature, "hex"), bytes);
}

test("real host consent and both 88A keys produce separately signed bound grant and first lease", async () => {
  const f = await fixture(); const { host, guest, policy, packet } = await f.pair();
  assert.equal(packet.grant.body.hostPublicKey, host.proof.challenge.publicKey); assert.equal(packet.grant.body.guestPublicKey, guest.proof.challenge.publicKey);
  assert.equal(packet.grant.body.policyDigest, policy.policyDigest); assert.equal(packet.grant.body.hostAccountId, "host"); assert.equal(packet.grant.body.guestAccountId, "guest");
  assert.equal(packet.grant.body.hostOperationId, host.proof.challenge.operationId); assert.equal(packet.grant.body.guestOperationId, guest.proof.challenge.operationId);
  assert.equal(packet.grant.body.sessionExpiresAtUnixMs, host.proof.challenge.expiresAtUnixMs);
  assert.equal(packet.grant.body.admissionExpiresAtUnixMs - packet.grant.body.issuedAtUnixMs, 10_000);
  assert.equal(packet.lease.body.sequence, 1); assert.equal(packet.lease.body.expiresAtUnixMs - packet.lease.body.issuedAtUnixMs, 3_000);
  assert.equal(await verify(f.authority.admissionPublicKey, packet.grant, socialAdmissionPreimage(packet.grant.body)), true);
  assert.equal(await verify(f.authority.admissionPublicKey, packet.lease, socialContinuityPreimage(packet.lease.body)), true);
  assert.equal(await verify(f.authority.admissionPublicKey, packet.lease, socialAdmissionPreimage(packet.grant.body)), false);
});
test("original proof is consumed internally; client receipt and policy without correct signature rejected", async () => {
  const f = await fixture(); const host = await f.prepared("host", "host");
  const receipt = await f.binding.prove(req("host"), host.proof);
  await assert.rejects(f.authority.registerHost(req("host"), { proof: receipt, policySigned: host.policySigned }), code("invalid_shape"));
  const second = await f.prepared("host", "host"); second.policySigned.signature = "0".repeat(128);
  await assert.rejects(f.authority.registerHost(req("host"), second), code("invalid_shape"));
  // Strip fixture-only keys, keeping the exact request contract.
  await assert.rejects(f.authority.registerHost(req("host"), { proof: second.proof, policySigned: second.policySigned }), code("invalid_signature"));
  await assert.rejects(f.binding.prove(req("host"), second.proof), code("challenge_unavailable"));
});
test("changed host context, extra fields, wrong visibility or approval are never a policy", async () => {
  const f = await fixture(); const host = await f.prepared("host", "host");
  for (const extension of [{ visibility: ["public"] }, { joinPolicy: "approval" }, { maxGuests: 2 }, { protocol: "udp" }, { expiresAtUnixMs: host.proof.challenge.issuedAtUnixMs + 60_001 }]) {
    assert.throws(() => validateHostSocialPolicy({ ...host.policySigned.body, ...extension }), code("invalid_policy"));
  }
  host.policySigned.body.operationId = crypto.randomUUID();
  host.policySigned.signature = hex(await crypto.subtle.sign("Ed25519", host.keys.privateKey, hostSocialPolicyPreimage(host.policySigned.body)));
  await assert.rejects(f.authority.registerHost(req("host"), { proof: host.proof, policySigned: host.policySigned }), code("policy_binding_mismatch"));
});
test("register replay and simultaneous duplicate host/session have only one policy", async () => {
  const f = await fixture(), first = await f.prepared("host", "host"), second = await f.prepared("host", "host");
  const attempts = await Promise.allSettled([first, second].map(host => f.authority.registerHost(req("host"), { proof: host.proof, policySigned: host.policySigned })));
  assert.equal(attempts.filter(value => value.status === "fulfilled").length, 1); assert.equal(f.authority.inspect().policies, 1);
  await assert.rejects(f.authority.registerHost(req("host"), { proof: first.proof, policySigned: first.policySigned }));
});
test("join atomically reserves one slot and consumes both competing proofs", async () => {
  const f = await fixture(); const { policy } = await f.register(); const a = await f.prepared("guest"), b = await f.prepared("guest2");
  const results = await Promise.allSettled([a, b].map(guest => f.authority.join(req(guest.uid), { proof: guest.proof, policyId: policy.policyId })));
  assert.equal(results.filter(value => value.status === "fulfilled").length, 1); assert.equal(f.authority.inspect().grants, 1);
  for (const guest of [a, b]) await assert.rejects(f.binding.prove(req(guest.uid), guest.proof), code("challenge_unavailable"));
});
test("fresh-read failure after reservation burns slot and cannot be recovered by replay", async () => {
  const f = await fixture(); const { policy } = await f.register(); const guest = await f.prepared(); const before = f.readCount();
  f.onRead(async count => { if (count === before + 2) f.block(); });
  await assert.rejects(f.authority.join(req(), { proof: guest.proof, policyId: policy.policyId }), code("session_not_authorized"));
  f.unblock(); await assert.rejects(f.authority.join(req(), { proof: guest.proof, policyId: policy.policyId }), code("policy_unavailable"));
  await assert.rejects(f.binding.prove(req(), guest.proof), code("challenge_unavailable"));
});
test("changed projection before joining rejects and cannot issue a transport grant", async () => {
  const f = await fixture(); const { policy } = await f.register(); const guest = await f.prepared(); f.session.title = "Changed";
  await assert.rejects(f.authority.join(req(), { proof: guest.proof, policyId: policy.policyId }), code("session_changed"));
  assert.equal(f.authority.inspect().grants, 0);
});
test("continuity is authenticated and expires at original session, never reanchors it", async () => {
  const f = await fixture(); const { packet } = await f.pair(); const expiry = packet.grant.body.sessionExpiresAtUnixMs;
  for (let i = 0; i < 53; i++) { f.advance(1_000); const lease = await f.authority.continuity(req(), { grantId: packet.grant.body.grantId });
    assert.equal(lease.body.sequence, i + 2); assert.ok(lease.body.expiresAtUnixMs <= expiry); }
  f.advance(2_000); await assert.rejects(f.authority.continuity(req(), { grantId: packet.grant.body.grantId }));
});
test("block, private switch, backend error and wrong account terminally stop continuity", async () => {
  for (const change of ["block", "private", "backend", "account"]) {
    const f = await fixture(); const { packet } = await f.pair(); f.advance(500);
    if (change === "block") f.block(); if (change === "private") f.session.visibility = "private";
    if (change === "backend") f.onRead(async () => { throw new Error("private backend detail"); });
    await assert.rejects(f.authority.continuity(req(change === "account" ? "someone" : "guest"), { grantId: packet.grant.body.grantId }));
    f.unblock(); f.session.visibility = "public"; f.onRead(async () => {});
    await assert.rejects(f.authority.continuity(req(), { grantId: packet.grant.body.grantId })); assert.equal(f.authority.inspect().terminalGrants, 1);
  }
});
test("expired continuity or clock rollback never resumes even if session remains live", async () => {
  for (const change of [(f: Awaited<ReturnType<typeof fixture>>) => f.advance(3_000), (f: Awaited<ReturnType<typeof fixture>>) => f.monoOnly(3_000), (f: Awaited<ReturnType<typeof fixture>>) => f.wallOnly(-1)]) {
    const f = await fixture(); const { packet } = await f.pair(); change(f);
    await assert.rejects(f.authority.continuity(req(), { grantId: packet.grant.body.grantId })); assert.equal(f.authority.inspect().terminalGrants, 1);
  }
});
test("three-second freshness starts before read, digest and signing for grant and renewal", async () => {
  const f = await fixture(); const { policy } = await f.register(); const guest = await f.prepared(); const before = f.readCount();
  f.onRead(async count => { if (count === before + 2) f.advance(3_001); });
  await assert.rejects(f.authority.join(req(), { proof: guest.proof, policyId: policy.policyId }), code("continuity_expired"));
  const g = await fixture(); const { packet } = await g.pair(); g.onRead(async () => { g.advance(3_001); });
  await assert.rejects(g.authority.continuity(req(), { grantId: packet.grant.body.grantId }), code("continuity_expired"));
});
test("overlapping continuity and five requests in a rolling second fail closed", async () => {
  const f = await fixture(); const { packet } = await f.pair(); let release!: () => void;
  const hold = new Promise<void>(resolve => { release = resolve; }); f.onRead(async () => hold);
  const one = f.authority.continuity(req(), { grantId: packet.grant.body.grantId });
  const both = Promise.allSettled([one, f.authority.continuity(req(), { grantId: packet.grant.body.grantId })]); release();
  assert.equal((await both).filter(value => value.status === "fulfilled").length, 0); assert.equal(f.authority.inspect().terminalGrants, 1);
  const g = await fixture(); const { packet: other } = await g.pair(); for (let i = 0; i < 4; i++) await g.authority.continuity(req(), { grantId: other.grant.body.grantId });
  await assert.rejects(g.authority.continuity(req(), { grantId: other.grant.body.grantId }), code("rate_limited"));
});
test("cancellation during renewal cannot return a lease or recover that grant", async () => {
  const f = await fixture(); const { packet } = await f.pair(); const controller = new AbortController(); f.onRead(async () => { controller.abort(); });
  await assert.rejects(f.authority.continuity(req("guest", controller.signal), { grantId: packet.grant.body.grantId }), code("binding_cancelled"));
  assert.equal(f.authority.inspect().terminalGrants, 1);
});
test("output objects cannot mutate retained authority context", async () => {
  const f = await fixture(); const { packet } = await f.pair(); const original = packet.grant.body.grantId;
  packet.grant.body.guestAccountId = "someone"; packet.lease.body.expiresAtUnixMs += 99_999;
  const next = await f.authority.continuity(req(), { grantId: original }); assert.equal(next.body.sequence, 2);
  assert.equal(next.body.expiresAtUnixMs - next.body.issuedAtUnixMs, 3_000);
});
test("codec domain/order closed shape and per-object bounds are enforced", async () => {
  const f = await fixture(); const { host, packet } = await f.pair();
  assert.deepEqual(hostSocialPolicyPreimage(host.policySigned.body), hostSocialPolicyPreimage(Object.fromEntries(Object.entries(host.policySigned.body).reverse())));
  assert.equal(await hostSocialPolicyDigest(host.policySigned.body), packet.grant.body.policyDigest);
  assert.throws(() => validateSocialAdmissionGrant({ ...packet.grant.body, guestPublicKey: packet.grant.body.hostPublicKey }), code("invalid_grant"));
  assert.throws(() => validateSocialContinuityLease({ ...packet.lease.body, sequence: Number.MAX_SAFE_INTEGER + 1 }), code("invalid_lease"));
  assert.throws(() => validateSocialContinuityLease({ ...packet.lease.body, expiresAtUnixMs: packet.lease.body.issuedAtUnixMs + 3_001 }), code("invalid_lease"));
  await assert.rejects(f.authority.join(req(), { proof: {}, policyId: "a".repeat(9_000) }), code("request_too_large"));
});
test("32 host policies are bounded without evicting any current policy", async () => {
  const f = await fixture();
  for (let index = 0; index < 33; index++) {
    const uid = `host${index}`, session = { ...f.session, id: crypto.randomUUID(), hostUid: uid };
    f.sessions.set(session.id, session); const prepared = await f.prepared(uid, "host", session);
    const attempt = f.authority.registerHost(req(uid), { proof: prepared.proof, policySigned: prepared.policySigned });
    if (index < 32) await attempt; else await assert.rejects(attempt, code("authority_busy"));
  }
  assert.equal(f.authority.inspect().policies, 32);
});
test("host monotonic preparation survives a wall rollback inside the signed interval", async () => {
  const f = await fixture(); const host = await f.prepared("host", "host");
  f.advance(20_000); f.wallOnly(-10_000);
  const policy = await f.authority.registerHost(req("host"), { proof: host.proof, policySigned: host.policySigned });
  f.advance(35_000); // Civil time remains before policy expiry; original monotonic preparation has ended.
  const guest = await f.prepared();
  await assert.rejects(f.authority.join(req(), { proof: guest.proof, policyId: policy.policyId }), code("policy_unavailable"));
});
test("guest original monotonic deadline constrains grant after civil rollback", async () => {
  const f = await fixture(); const guest = await f.prepared(); f.advance(10_000); const { policy } = await f.register();
  f.advance(10_000); f.wallOnly(-5_000);
  const packet = await f.authority.join(req(), { proof: guest.proof, policyId: policy.policyId });
  // Reconstructed civil-only expiry would be 55s; retained guest monotonic cap limits it to 50s.
  assert.equal(packet.grant.body.sessionExpiresAtUnixMs, 1_800_000_050_000);
  for (let i = 0; i < 34; i++) { f.advance(1_000); await f.authority.continuity(req(), { grantId: packet.grant.body.grantId }); }
  f.advance(1_000); await assert.rejects(f.authority.continuity(req(), { grantId: packet.grant.body.grantId }));
});
test("expiry between retained proof verification and policy publication rejects without reopening proof", async () => {
  const f = await fixture(); const host = await f.prepared("host", "host"), before = f.readCount();
  f.onRead(async count => { if (count === before + 2) f.monoOnly(55_000); });
  await assert.rejects(f.authority.registerHost(req("host"), { proof: host.proof, policySigned: host.policySigned }), code("policy_expired"));
  assert.equal(f.authority.inspect().policies, 0);
  await assert.rejects(f.binding.prove(req("host"), host.proof), code("challenge_unavailable"));
});
test("guest cannot reuse the host native key under another authenticated account", async () => {
  const f = await fixture(); const { host, policy } = await f.register();
  const challenge = await f.binding.challenge(req(), { schemaVersion: 1, operationId: crypto.randomUUID(), generation: 1, role: "guest",
    publicKey: host.proof.challenge.publicKey, sessionId: f.session.id, preparedExpiresAtUnixMs: host.proof.challenge.expiresAtUnixMs });
  const proof = { schemaVersion: 1, challenge, signature: hex(await crypto.subtle.sign("Ed25519", host.keys.privateKey, socialKeyProofPreimage(challenge))) };
  await assert.rejects(f.authority.join(req(), { proof, policyId: policy.policyId }), code("guest_binding_mismatch"));
  assert.equal(f.authority.inspect().grants, 0);
});
test("256 continuity checks per process minute are bounded across different grants", async () => {
  const f = await fixture(); const pairs = [];
  for (const index of [1, 2]) {
    const session = { ...f.session, id: crypto.randomUUID(), hostUid: `host${index}` }; f.sessions.set(session.id, session);
    const host = await f.prepared(session.hostUid, "host", session);
    const policy = await f.authority.registerHost(req(host.uid), { proof: host.proof, policySigned: host.policySigned });
    const guest = await f.prepared(`guest${index}`, "guest", session);
    const packet = await f.authority.join(req(guest.uid), { proof: guest.proof, policyId: policy.policyId }); pairs.push({ guest, packet });
  }
  for (let i = 0; i < 128; i++) { f.advance(250); for (const pair of pairs) await f.authority.continuity(req(pair.guest.uid), { grantId: pair.packet.grant.body.grantId }); }
  f.advance(250);
  await assert.rejects(f.authority.continuity(req(pairs[0].guest.uid), { grantId: pairs[0].packet.grant.body.grantId }), code("rate_limited"));
  assert.equal(f.authority.inspect().terminalGrants, 1);
});
