import assert from "node:assert/strict";
import test from "node:test";
import { createSocialBindingAuthority, socialKeyProofPreimage, SocialBindingError } from "./connectSocialBinding.ts";
import { createSocialAdmissionAuthority, hostSocialPolicyDigest, hostSocialPolicyPreimage, socialAdmissionPreimage, socialContinuityPreimage,
  socialJoinContextPreimage, validateHostSocialPolicy, validateSocialAdmissionGrant, validateSocialContinuityLease, validateSocialJoinContext } from "./connectSocialAdmission.ts";
import type { SocialBindingSession } from "./connectSocialBinding.ts";

const req = (uid = "guest", signal?: AbortSignal) => new Request("http://127.0.0.1/local", { signal, headers: { Authorization: `Bearer fixture-${uid}` } });
const code = (expected: string) => (error: unknown) => error instanceof SocialBindingError && error.code === expected;
const hex = (buffer: ArrayBuffer) => Buffer.from(buffer).toString("hex");
async function fixture(online = false) {
  let wall = 1_800_000_000_000, mono = 1_000, reads = 0;
  let hook: ((count: number) => Promise<void>) | undefined;
  const blocked = new Set<string>(); const sessions = new Map<string, SocialBindingSession>();
  const session: SocialBindingSession = { id: crypto.randomUUID(), hostUid: "host", title: "World", modpackName: "Local", minecraftVersion: "1.21.1", loader: "Fabric", imageUrl: null,
    players: 1, maxPlayers: online ? 8 : 2, visibility: "public", joinPolicy: "open", expiresAtMs: wall + (online ? 14_395_000 : 300_000) };
  sessions.set(session.id, session);
  const dependencies = { clock: { wall: () => wall, monotonic: () => mono },
    trustedAuthenticate: async (request: Request) => { const auth = request.headers.get("Authorization"); if (!auth?.startsWith("Bearer fixture-")) throw new SocialBindingError("unauthenticated"); return auth.slice(15); },
    readSession: async (principal: { accountId: string }, id: string) => { reads++; await hook?.(reads); if (blocked.has(principal.accountId)) throw new SocialBindingError("session_not_authorized");
      const value = sessions.get(id); if (!value) throw new SocialBindingError("session_not_authorized"); return { ...value }; } };
  const keys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
  const binding = createSocialBindingAuthority(dependencies), authority = await createSocialAdmissionAuthority(binding, dependencies, { keys, online });
  async function prepared(uid = "guest", role: "guest" | "host" = "guest", selected = session) {
    const keys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
    const publicKey = hex(await crypto.subtle.exportKey("raw", keys.publicKey));
    const challenge = await binding.challenge(req(uid), { schemaVersion: 1, operationId: crypto.randomUUID(), generation: 1, role, publicKey,
      sessionId: selected.id, preparedExpiresAtUnixMs: wall + 55_000 });
    const proof = { schemaVersion: 1, challenge, signature: hex(await crypto.subtle.sign("Ed25519", keys.privateKey, socialKeyProofPreimage(challenge))) };
    const body = { schemaVersion: online ? 2 : 1, purpose: "hostSocialSession", issuerInstance: challenge.issuerInstance, policyId: challenge.challengeId,
      accountId: uid, operationId: challenge.operationId, generation: challenge.generation, publicKey, sessionId: selected.id, sessionDigest: challenge.sessionDigest,
      visibility: selected.visibility, joinPolicy: selected.joinPolicy, serviceId: "game", protocol: "tcp", sourceDigest: "a".repeat(64), maxGuests: online ? 8 : 1,
      issuedAtUnixMs: challenge.issuedAtUnixMs, expiresAtUnixMs: online ? selected.expiresAtMs : challenge.expiresAtUnixMs };
    const policySigned = { body, signature: hex(await crypto.subtle.sign("Ed25519", keys.privateKey, hostSocialPolicyPreimage(body))) };
    return { uid, proof, policySigned, keys };
  }
  const register = async () => { const host = await prepared("host", "host"); const policy = await authority.registerHost(req("host"), { proof: host.proof, policySigned: host.policySigned }); return { host, policy }; };
  const pair = async () => { const { host, policy } = await register(); const guest = await prepared(); const packet = await authority.join(req(), { proof: guest.proof, policyId: policy.policyId }); return { host, guest, policy, packet }; };
  return { binding, authority, dependencies, keys, session, sessions, prepared, register, pair, block: (uid = "guest") => blocked.add(uid), unblock: (uid = "guest") => blocked.delete(uid),
    readCount: () => reads, onRead: (fn: (count: number) => Promise<void>) => { hook = fn; },
    advance: (amount: number) => { wall += amount; mono += amount; }, wallOnly: (amount: number) => { wall += amount; }, monoOnly: (amount: number) => { mono += amount; } };
}
async function verify(keyHex: string, signed: { signature: string }, bytes: Uint8Array<ArrayBuffer>) {
  const key = await crypto.subtle.importKey("raw", Buffer.from(keyHex, "hex"), "Ed25519", false, ["verify"]);
  return crypto.subtle.verify("Ed25519", key, Buffer.from(signed.signature, "hex"), bytes);
}

test("89 identity comes only from authentication and carries no transport capability", async () => {
  const f = await fixture();
  assert.deepEqual(await f.authority.identity(req(), {}), { accountId: "guest", issuerInstance: f.authority.issuerInstance });
  assert.equal(f.readCount(), 0);
  await assert.rejects(f.authority.identity(req(), { accountId: "host" }), code("invalid_shape"));
  await assert.rejects(f.authority.identity(new Request("http://127.0.0.1/local"), {}), code("unauthenticated"));
});
test("89 authenticated mailbox transports original grant and latest lease without minting authority", async () => {
  const f = await fixture(); const { policy } = await f.register();
  assert.deepEqual(await f.authority.hostPoll(req("host"), { policyId: policy.policyId }), { state: "waiting" });
  const guest = await f.prepared(); const packet = await f.authority.join(req(), { proof: guest.proof, policyId: policy.policyId });
  const admission = await f.authority.hostPoll(req("host"), { policyId: policy.policyId });
  assert.equal(admission.state, "admitted"); if (admission.state !== "admitted") assert.fail();
  assert.deepEqual(admission.grant, packet.grant); assert.deepEqual(admission.lease, packet.lease);
  const grantId = packet.grant.body.grantId;
  assert.deepEqual(await f.authority.guestReceive(req(), { grantId }), { state: "waiting" });
  // The broker treats this bounded string as opaque; only native verification can make it a transport invitation.
  assert.deepEqual(await f.authority.hostPublish(req("host"), { grantId, invitation: "untrusted-test-envelope" }), { accepted: true });
  assert.deepEqual(await f.authority.guestReceive(req(), { grantId }), { state: "ready", invitation: "untrusted-test-envelope", lease: packet.lease });
  f.advance(750); const lease = await f.authority.continuity(req(), { grantId });
  const next = await f.authority.hostPoll(req("host"), { policyId: policy.policyId });
  assert.equal(next.state, "admitted"); if (next.state !== "admitted") assert.fail();
  assert.deepEqual(next.lease, lease); assert.deepEqual(next.grant, packet.grant);
  // Polls preserve the original lease and its deadline, even with repeated successful host reads.
  f.advance(2_000); await f.authority.hostPoll(req("host"), { policyId: policy.policyId });
  f.advance(1_000); await assert.rejects(f.authority.hostPoll(req("host"), { policyId: policy.policyId }), code("continuity_expired"));
  await assert.rejects(f.authority.guestReceive(req(), { grantId }), code("policy_expired"));
});
test("89 another account cannot discover, publish, read or stop another pair", async () => {
  const f = await fixture(); const { policy, packet } = await f.pair(); const grantId = packet.grant.body.grantId;
  for (const [method, args] of [
    ["hostPoll", { policyId: policy.policyId }], ["hostPublish", { grantId, invitation: "opaque" }],
    ["guestReceive", { grantId }], ["hostStop", { policyId: policy.policyId }], ["guestStop", { grantId }],
  ] as const) await assert.rejects(f.authority[method](req("third"), args), code("broker_unavailable"));
  await assert.rejects(f.authority.hostPoll(req("third"), { policyId: "0".repeat(64) }), code("broker_unavailable"));
  await assert.rejects(f.authority.guestReceive(new Request("http://127.0.0.1/local"), { grantId }), code("unauthenticated"));
  assert.equal(f.authority.inspect().terminalGrants, 0);
  assert.equal((await f.authority.hostPoll(req("host"), { policyId: policy.policyId })).state, "admitted");
});
test("89 late first host poll retains original sequence-one lease independently of newer continuity", async () => {
  const f = await fixture(); const { policy, packet } = await f.pair();
  f.advance(750); const renewed = await f.authority.continuity(req(), { grantId: packet.grant.body.grantId });
  const first = await f.authority.hostPoll(req("host"), { policyId: policy.policyId }); if (first.state !== "admitted") assert.fail();
  assert.deepEqual(first.initialLease, packet.lease); assert.equal(first.initialLease.body.sequence, 1);
  assert.deepEqual(first.lease, renewed); assert.equal(first.lease.body.sequence, 2);
  const expiry = packet.lease.body.expiresAtUnixMs;
  first.initialLease.body.expiresAtUnixMs += 5_000;
  const second = await f.authority.hostPoll(req("host"), { policyId: policy.policyId }); if (second.state !== "admitted") assert.fail();
  assert.equal(second.initialLease.body.expiresAtUnixMs, expiry);
});
test("89 publishing is single use under concurrent sends and does not allow replacement", async () => {
  const f = await fixture(); const { packet } = await f.pair(); const grantId = packet.grant.body.grantId;
  const results = await Promise.allSettled(["first", "second"].map(invitation => f.authority.hostPublish(req("host"), { grantId, invitation })));
  assert.equal(results.filter(result => result.status === "fulfilled").length, 1);
  assert.equal(results.filter(result => result.status === "rejected" && code("invitation_already_published")(result.reason)).length, 1);
  const received = await f.authority.guestReceive(req(), { grantId }); assert.equal(received.state, "ready");
  if (received.state !== "ready") assert.fail(); assert.ok(["first", "second"].includes(received.invitation));
  await assert.rejects(f.authority.hostPublish(req("host"), { grantId, invitation: received.invitation }), code("invitation_already_published"));
});
test("89 mailbox publication and reception recheck permissions after asynchronous reads", async () => {
  for (const method of ["hostPublish", "guestReceive", "hostPoll"] as const) {
    const f = await fixture(); const { policy, packet } = await f.pair(); const grantId = packet.grant.body.grantId;
    if (method === "guestReceive") await f.authority.hostPublish(req("host"), { grantId, invitation: "opaque" });
    f.onRead(async () => { f.session.title = "changed while read was pending"; });
    const args = method === "hostPoll" ? { policyId: policy.policyId } : method === "hostPublish" ? { grantId, invitation: "opaque" } : { grantId };
    await assert.rejects(f.authority[method](req(method === "guestReceive" ? "guest" : "host"), args), code("session_changed"));
    assert.equal(f.authority.inspect().terminalGrants, 1);
  }
});
test("89 stopping either participant is idempotent, terminal and races out pending delivery", async () => {
  for (const role of ["host", "guest"] as const) {
    const f = await fixture(); const { policy, packet } = await f.pair(); const grantId = packet.grant.body.grantId;
    const stop = () => role === "host" ? f.authority.hostStop(req("host"), { policyId: policy.policyId }) : f.authority.guestStop(req(), { grantId });
    f.onRead(async () => { await stop(); });
    await assert.rejects(f.authority.hostPublish(req("host"), { grantId, invitation: "opaque" }), code("policy_expired"));
    assert.deepEqual(await stop(), { stopped: true }); assert.deepEqual(await stop(), { stopped: true });
    await assert.rejects(f.authority.continuity(req(), { grantId }));
  }
});
test("89 blocked guest or removed host registration cannot retrieve retained envelopes", async () => {
  for (const role of ["host", "guest"] as const) {
    const f = await fixture(); const { policy, packet } = await f.pair(); const grantId = packet.grant.body.grantId;
    await f.authority.hostPublish(req("host"), { grantId, invitation: "opaque" }); f.block(role);
    await assert.rejects(role === "host" ? f.authority.hostPoll(req("host"), { policyId: policy.policyId }) : f.authority.guestReceive(req(), { grantId }), code("session_not_authorized"));
    assert.equal(f.authority.inspect().terminalGrants, 1);
  }
});
test("89 mailbox respects admission monotonic deadline without shortening admitted continuity", async () => {
  const f = await fixture(); const { policy, packet } = await f.pair(); const grantId = packet.grant.body.grantId;
  await f.authority.hostPublish(req("host"), { grantId, invitation: "opaque" });
  for (let i = 0; i < 10; i++) { f.advance(750); f.monoOnly(250); await f.authority.continuity(req(), { grantId }); }
  // Civil time advances more slowly; admitted continuity remains valid but admission cannot gain those 2.5 seconds.
  assert.equal((await f.authority.hostPoll(req("host"), { policyId: policy.policyId })).state, "admitted");
  await assert.rejects(f.authority.guestReceive(req(), { grantId }), code("admission_expired"));
  assert.equal(f.authority.inspect().terminalGrants, 1);
  await assert.rejects(f.authority.hostPoll(req("host"), { policyId: policy.policyId }));
});
test("89 mailbox copies cannot mutate retained grant or lease and input stays bounded", async () => {
  const f = await fixture(); const { policy, packet } = await f.pair(); const grantId = packet.grant.body.grantId;
  const poll = await f.authority.hostPoll(req("host"), { policyId: policy.policyId }); if (poll.state !== "admitted") assert.fail();
  poll.grant.body.guestAccountId = "other"; poll.lease.body.expiresAtUnixMs += 60_000;
  const again = await f.authority.hostPoll(req("host"), { policyId: policy.policyId }); if (again.state !== "admitted") assert.fail();
  assert.deepEqual(again.grant, packet.grant); assert.deepEqual(again.lease, packet.lease);
  await assert.rejects(f.authority.hostPublish(req("host"), { grantId, invitation: "x".repeat(8_193) }), code("request_too_large"));
  await assert.rejects(f.authority.guestReceive(req(), { grantId, accountId: "guest" }), code("invalid_shape"));
  assert.equal(f.authority.inspect().terminalGrants, 0);
});
test("89 published input is frozen before authentication and a delayed read cannot publish after expiry", async () => {
  const f = await fixture(); const { packet } = await f.pair(); const grantId = packet.grant.body.grantId;
  const input = { grantId, invitation: "original" };
  const publishing = f.authority.hostPublish(req("host"), input); input.invitation = "x".repeat(10_000);
  await publishing;
  const received = await f.authority.guestReceive(req(), { grantId }); if (received.state !== "ready") assert.fail();
  assert.equal(received.invitation, "original");
  const g = await fixture(); const { packet: other } = await g.pair();
  g.onRead(async () => { g.advance(3_001); });
  await assert.rejects(g.authority.hostPublish(req("host"), { grantId: other.grant.body.grantId, invitation: "stale" }), code("continuity_expired"));
  assert.equal(g.authority.inspect().terminalGrants, 1);
});

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
test("changed host context, extra fields, wrong visibility or unknown approval mode are never a policy", async () => {
  const f = await fixture(); const host = await f.prepared("host", "host");
  for (const extension of [{ visibility: ["public"] }, { joinPolicy: "automatic" }, { maxGuests: 2 }, { protocol: "udp" }, { expiresAtUnixMs: host.proof.challenge.issuedAtUnixMs + 60_001 }]) {
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
test("context signs current public/friends policy for the authenticated recipient, never an ingress grant", async () => {
  for (const visibility of ["public", "friends"] as const) {
    const f = await fixture(); f.session.visibility = visibility;
    const { host, policy } = await f.register(); const before = f.authority.inspect(); const guest = await f.prepared(); const pending = f.binding.inspect().pending;
    const context = await f.authority.resolveContext(req(), { sessionId: f.session.id });
    assert.equal(context.body.recipientAccountId, "guest"); assert.equal(context.body.hostAccountId, "host");
    assert.equal(context.body.hostPublicKey, host.proof.challenge.publicKey); assert.equal(context.body.hostOperationId, host.proof.challenge.operationId);
    assert.equal(context.body.policyDigest, policy.policyDigest); assert.equal(context.body.policyId, policy.policyId);
    assert.equal(context.body.expiresAtUnixMs - context.body.issuedAtUnixMs, 10_000);
    assert.equal(await verify(f.authority.admissionPublicKey, context, socialJoinContextPreimage(context.body)), true);
    assert.deepEqual(f.authority.inspect(), before); assert.equal(f.binding.inspect().pending, pending);
    assert.equal("sourceDigest" in context.body || "token" in context.body || "invitation" in context.body, false);
    assert.equal((await f.authority.join(req(), { proof: guest.proof, policyId: policy.policyId })).grant.body.guestAccountId, "guest");
  }
});
test("context authenticates and reads authorized session even when policy is absent", async () => {
  const f = await fixture(); const before = f.readCount();
  await assert.rejects(f.authority.resolveContext(new Request("http://127.0.0.1"), { sessionId: f.session.id }), code("unauthenticated"));
  assert.equal(f.readCount(), before);
  await assert.rejects(f.authority.resolveContext(req(), { sessionId: f.session.id }), code("context_unavailable"));
  assert.equal(f.readCount(), before + 1);
  await assert.rejects(f.authority.resolveContext(req(), { sessionId: crypto.randomUUID() }), code("context_unavailable"));
  assert.equal(f.readCount(), before + 2);
});
test("context hides blocked/private/changed/own/missing/reserved policy differences", async () => {
  for (const kind of ["blocked", "private", "changed", "own", "missing", "reserved"]) {
    const f = await fixture(); const { policy } = await f.register();
    if (kind === "blocked") f.block(); if (kind === "private") f.session.visibility = "private";
    if (kind === "changed") f.session.title = "Changed"; if (kind === "missing") f.sessions.delete(f.session.id);
    if (kind === "reserved") { const guest = await f.prepared(); await f.authority.join(req(), { proof: guest.proof, policyId: policy.policyId }); }
    await assert.rejects(f.authority.resolveContext(req(kind === "own" ? "host" : "guest"), { sessionId: f.session.id }), code("context_unavailable"));
  }
});
test("returned context cannot be altered or forwarded to another account without invalidating signature", async () => {
  const f = await fixture(); await f.register(); const context = await f.authority.resolveContext(req(), { sessionId: f.session.id });
  for (const extension of [{ recipientAccountId: "other" }, { hostPublicKey: "b".repeat(64) }, { sessionId: crypto.randomUUID() }, { policyId: "c".repeat(64) }]) {
    assert.equal(await verify(f.authority.admissionPublicKey, context, socialJoinContextPreimage({ ...context.body, ...extension })), false);
  }
  const original = context.body.hostPublicKey; context.body.hostPublicKey = "d".repeat(64);
  const next = await f.authority.resolveContext(req(), { sessionId: f.session.id }); assert.equal(next.body.hostPublicKey, original);
});
test("context input chooses only session ID, never recipient, issuer, URL or backend pin", async () => {
  const f = await fixture();
  for (const extension of [{ recipientAccountId: "host" }, { issuerInstance: "a".repeat(64) }, { backend: "http://evil.test" }, { publicKey: "b".repeat(64) }]) {
    await assert.rejects(f.authority.resolveContext(req(), { sessionId: f.session.id, ...extension }), code("invalid_shape"));
  }
  await assert.rejects(f.authority.resolveContext(req(), { sessionId: "../../users" }), code("invalid_request"));
});
test("context codec rejects extra fields, wrong recipient, unsafe generation and excessive validity", async () => {
  const f = await fixture(); await f.register(); const { body } = await f.authority.resolveContext(req(), { sessionId: f.session.id });
  assert.deepEqual(socialJoinContextPreimage(body), socialJoinContextPreimage(Object.fromEntries(Object.entries(body).reverse())));
  assert.throws(() => validateSocialJoinContext({ ...body, token: "not-allowed" }), code("invalid_shape"));
  for (const extension of [{ recipientAccountId: body.hostAccountId }, { purpose: ["socialJoinContext"] }, { hostGeneration: Number.MAX_SAFE_INTEGER + 1 },
    { expiresAtUnixMs: body.issuedAtUnixMs + 10_001 }, { expiresAtUnixMs: body.issuedAtUnixMs }]) {
    assert.throws(() => validateSocialJoinContext({ ...body, ...extension }), code("invalid_context"));
  }
});
test("context freshness starts before read and is never reanchored by backend latency", async () => {
  const f = await fixture(); await f.register(); f.onRead(async () => { f.advance(10_001); });
  await assert.rejects(f.authority.resolveContext(req(), { sessionId: f.session.id }), code("context_unavailable"));
  const g = await fixture(); await g.register(); g.onRead(async () => { g.advance(4_000); });
  const { body } = await g.authority.resolveContext(req(), { sessionId: g.session.id });
  assert.equal(body.issuedAtUnixMs, 1_800_000_000_000); assert.equal(body.expiresAtUnixMs, 1_800_000_010_000);
});
test("context expiry honors original monotonic policy after an in-range civil rollback", async () => {
  const f = await fixture(); const host = await f.prepared("host", "host"); f.advance(20_000); f.wallOnly(-10_000);
  await f.authority.registerHost(req("host"), { proof: host.proof, policySigned: host.policySigned }); f.advance(34_000);
  const { body } = await f.authority.resolveContext(req(), { sessionId: f.session.id });
  assert.equal(body.expiresAtUnixMs - body.issuedAtUnixMs, 1_000); // Only one original monotonic second remains.
  f.advance(1_000); await assert.rejects(f.authority.resolveContext(req(), { sessionId: f.session.id }), code("context_unavailable"));
});
test("context read racing an admitted guest cannot publish the now-reserved policy", async () => {
  const f = await fixture(); const { policy } = await f.register(); const guest = await f.prepared(); const nextRead = f.readCount() + 1;
  let release!: () => void, entered!: () => void;
  const hold = new Promise<void>(resolve => { release = resolve; }), waiting = new Promise<void>(resolve => { entered = resolve; });
  f.onRead(async count => { if (count === nextRead) { entered(); await hold; } });
  const result = f.authority.resolveContext(req("other"), { sessionId: f.session.id });
  const denied = assert.rejects(result, code("context_unavailable")); await waiting;
  await f.authority.join(req(), { proof: guest.proof, policyId: policy.policyId }); release(); await denied;
});
test("context cancellation prevents publication and leaves host policy open", async () => {
  const f = await fixture(); await f.register(); const controller = new AbortController(); f.onRead(async () => { controller.abort(); });
  await assert.rejects(f.authority.resolveContext(req("guest", controller.signal), { sessionId: f.session.id }), code("binding_cancelled"));
  f.onRead(async () => {}); assert.ok((await f.authority.resolveContext(req(), { sessionId: f.session.id })).signature);
  assert.equal(f.authority.inspect().grants, 0);
});
test("an earlier context cannot bypass a later block when joining", async () => {
  const f = await fixture(); await f.register(); const context = await f.authority.resolveContext(req(), { sessionId: f.session.id }); const guest = await f.prepared(); f.block();
  await assert.rejects(f.authority.join(req(), { proof: guest.proof, policyId: context.body.policyId }), code("session_not_authorized"));
  assert.equal(f.authority.inspect().grants, 0);
});
test("context resolver uses existing account and pre-auth global entry limits", async () => {
  const f = await fixture(); await f.register(); for (let i = 0; i < 256; i++) await f.authority.resolveContext(req(), { sessionId: f.session.id });
  await assert.rejects(f.authority.resolveContext(req(), { sessionId: f.session.id }), code("rate_limited"));
  const g = await fixture(); for (let i = 0; i < 512; i++) await assert.rejects(g.authority.resolveContext(req(), {}), code("invalid_shape"));
  await assert.rejects(g.authority.resolveContext(req(), { sessionId: g.session.id }), code("rate_limited"));
});
test("context signing racing a slot reservation rechecks the original policy before publication", async () => {
  const f = await fixture(); const { policy } = await f.register(); const guest = await f.prepared();
  const original = crypto.subtle.sign; let release!: () => void, entered!: () => void;
  const held = new Promise<void>(resolve => { release = resolve; }), reached = new Promise<void>(resolve => { entered = resolve; });
  crypto.subtle.sign = (async (algorithm, key, data) => {
    const bytes = data instanceof ArrayBuffer ? new Uint8Array(data) : new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
    if (new TextDecoder().decode(bytes).startsWith("aurora/connect/social-join-context/v1\0")) { entered(); await held; }
    return original.call(crypto.subtle, algorithm, key, data);
  }) as typeof crypto.subtle.sign;
  try {
    const denied = assert.rejects(f.authority.resolveContext(req("other"), { sessionId: f.session.id }), code("context_unavailable"));
    await reached; await f.authority.join(req(), { proof: guest.proof, policyId: policy.policyId }); release(); await denied;
  } finally { release(); crypto.subtle.sign = original; }
});


test("durable restoration preserves signing identity, consumed challenge, original lease and terminal revocation", async () => {
  const f = await fixture(), { guest, policy, packet } = await f.pair();
  const binding = createSocialBindingAuthority(f.dependencies, f.binding.durableState());
  const authority = await createSocialAdmissionAuthority(binding, f.dependencies, { keys: f.keys, state: f.authority.durableState() });
  assert.equal(authority.admissionPublicKey, f.authority.admissionPublicKey);
  assert.equal(authority.issuerInstance, f.authority.issuerInstance);
  await assert.rejects(binding.prove(req(), guest.proof), code("challenge_unavailable"));
  const polled = await authority.hostPoll(req("host"), { policyId: policy.policyId });
  assert.equal(polled.state, "admitted"); if (polled.state !== "admitted") assert.fail();
  assert.deepEqual(polled.lease, packet.lease);
  await authority.guestStop(req(), { grantId: packet.grant.body.grantId });
  const restarted = await createSocialAdmissionAuthority(binding, f.dependencies, { keys: f.keys, state: authority.durableState() });
  await assert.rejects(restarted.continuity(req(), { grantId: packet.grant.body.grantId }), code("policy_expired"));
});

test("approval is bound into signed host policy and cannot replace the published session policy", async () => {
  const f = await fixture(), host = await f.prepared("host", "host");
  host.policySigned.body.joinPolicy = "approval";
  host.policySigned.signature = hex(await crypto.subtle.sign("Ed25519", host.keys.privateKey, hostSocialPolicyPreimage(host.policySigned.body)));
  await assert.rejects(f.authority.registerHost(req("host"), { proof: host.proof, policySigned: host.policySigned }), code("session_changed"));
});


test("online v2 permits eight concurrent guests; departure frees capacity without reusing proof or moving deadline", async () => {
  const f = await fixture(true), { policy } = await f.register(); const joined = [];
  for (let i = 0; i < 8; i++) { const guest = await f.prepared(`guest${i}`); joined.push(await f.authority.join(req(`guest${i}`), { proof: guest.proof, policyId: policy.policyId })); }
  const ninth = await f.prepared("guest8");
  await assert.rejects(f.authority.join(req("guest8"), { proof: ninth.proof, policyId: policy.policyId }), code("authority_busy"));
  await assert.rejects(f.binding.prove(req("guest8"), ninth.proof), code("challenge_unavailable"));
  const before = await f.authority.hostPoll(req("host"), { policyId: policy.policyId }); assert.equal(before.state, "group");
  if (before.state !== "group") assert.fail(); assert.equal(before.deliveries.length, 8);
  await f.authority.guestStop(req("guest0"), { grantId: joined[0].grant.body.grantId });
  const replacement = await f.prepared("replacement"), packet = await f.authority.join(req("replacement"), { proof: replacement.proof, policyId: policy.policyId });
  assert.equal(packet.grant.body.sessionExpiresAtUnixMs, joined[0].grant.body.sessionExpiresAtUnixMs);
  await assert.rejects(f.authority.continuity(req("guest0"), { grantId: joined[0].grant.body.grantId }), code("grant_terminal"));
  assert.equal((await f.authority.continuity(req("guest1"), { grantId: joined[1].grant.body.grantId })).body.sequence, 2);
  await f.authority.hostStop(req("host"), { policyId: policy.policyId });
  await assert.rejects(f.authority.continuity(req("replacement"), { grantId: packet.grant.body.grantId }), code("policy_expired"));
});

test("online original session survives preparation expiry; short continuity and admission stay independent", async () => {
  const f = await fixture(true), { packet, policy } = await f.pair(), expiry = packet.grant.body.sessionExpiresAtUnixMs;
  for (let i = 0; i < 65; i++) { f.advance(1000); await f.authority.continuity(req(), { grantId: packet.grant.body.grantId }); await f.authority.hostPoll(req("host"), { policyId: policy.policyId }); }
  const guest = await f.prepared("late"), late = await f.authority.join(req("late"), { proof: guest.proof, policyId: policy.policyId });
  assert.equal(late.grant.body.schemaVersion, 2); assert.equal(late.grant.body.sessionExpiresAtUnixMs, expiry);
  assert.equal(late.grant.body.admissionExpiresAtUnixMs - late.grant.body.issuedAtUnixMs, 10_000);
  f.block("late"); await assert.rejects(f.authority.continuity(req("late"), { grantId: late.grant.body.grantId }), code("session_not_authorized"));
  await f.authority.continuity(req(), { grantId: packet.grant.body.grantId });
  const roster = await f.authority.hostPoll(req("host"), { policyId: policy.policyId }); assert.equal(roster.state, "group");
  if (roster.state !== "group") assert.fail(); assert.equal(roster.deliveries.length, 1);
});

test("online relay identity is derived from exact policy/grant and cannot select another peer or account", async () => {
  const f = await fixture(true), { packet, policy } = await f.pair();
  const guest = await f.authority.relayIdentity(req(), { role: "guest", id: packet.grant.body.grantId });
  assert.equal(guest.peerId, packet.grant.body.guestPublicKey); assert.equal(guest.hostPeerId, packet.grant.body.hostPublicKey);
  assert.equal(guest.expiresAtUnixMs, packet.grant.body.sessionExpiresAtUnixMs);
  await assert.rejects(f.authority.relayIdentity(req("other"), { role: "guest", id: packet.grant.body.grantId }), code("broker_unavailable"));
  await assert.rejects(f.authority.relayIdentity(req(), { role: "guest", id: packet.grant.body.grantId, peerId: "00".repeat(32) }), code("invalid_shape"));
  await f.authority.hostStop(req("host"), { policyId: policy.policyId });
  await assert.rejects(f.authority.relayIdentity(req(), { role: "guest", id: packet.grant.body.grantId }), code("policy_expired"));
});

test("online voice policy binds profile, source and host-player consent into every grant with seven remote slots", async () => {
  const f = await fixture(true), host = await f.prepared("host", "host");
  const body = { ...host.policySigned.body, schemaVersion: 3, maxGuests: 7, voiceProfile: "svc", voiceSourceDigest: "b".repeat(64), hostPlayer: true };
  const signature = hex(await crypto.subtle.sign("Ed25519", host.keys.privateKey, hostSocialPolicyPreimage(body)));
  assert.deepEqual(hostSocialPolicyPreimage(body), hostSocialPolicyPreimage(Object.fromEntries(Object.entries(body).reverse())));
  const { policyId } = await f.authority.registerHost(req("host"), { proof: host.proof, policySigned: { body, signature } });
  for (let i = 0; i < 7; i++) {
    const guest = await f.prepared(`voice${i}`), joined = await f.authority.join(req(`voice${i}`), { proof: guest.proof, policyId });
    assert.equal(joined.grant.body.schemaVersion, 3); assert.equal(joined.grant.body.voiceProfile, "svc");
    assert.equal(joined.grant.body.voiceSourceDigest, body.voiceSourceDigest); assert.equal(joined.grant.body.hostPlayer, true);
    for (const extension of [{ voiceProfile: "plasmo" }, { voiceSourceDigest: "c".repeat(64) }, { hostPlayer: false }]) {
      assert.equal(await verify(f.authority.admissionPublicKey, joined.grant, socialAdmissionPreimage({ ...joined.grant.body, ...extension })), false);
    }
  }
  const eighth = await f.prepared("voice7"); await assert.rejects(f.authority.join(req("voice7"), { proof: eighth.proof, policyId }), code("authority_busy"));
  for (const extension of [{ schemaVersion: 2 }, { voiceProfile: "other" }, { voiceSourceDigest: "invalid" }, { hostPlayer: null }, { maxGuests: 8 }]) {
    assert.throws(() => validateHostSocialPolicy({ ...body, ...extension }));
  }
  const { hostPlayer: _omitted, ...missing } = body; assert.throws(() => validateHostSocialPolicy(missing));
  assert.notEqual(await hostSocialPolicyDigest(body), await hostSocialPolicyDigest({ ...body, hostPlayer: false }));
});

test("online observation sharing saves reads without moving original host freshness or guest lease expiry", async () => {
  const f = await fixture(true), { packet, policy } = await f.pair();
  const first = await f.authority.continuity(req(), { grantId: packet.grant.body.grantId });
  const polled = await f.authority.hostPoll(req("host"), { policyId: policy.policyId });
  assert.equal(polled.state, "group"); if (polled.state !== "group") assert.fail();
  const reads = f.readCount(); f.advance(500); f.block();
  const shared = await f.authority.continuity(req(), { grantId: packet.grant.body.grantId });
  const host = await f.authority.hostPoll(req("host"), { policyId: policy.policyId });
  assert.equal(f.readCount(), reads); assert.equal(host.state, "group"); if (host.state !== "group") assert.fail();
  assert.equal(host.checkedAtUnixMs, polled.checkedAtUnixMs);
  assert.equal(shared.body.issuedAtUnixMs, first.body.issuedAtUnixMs);
  assert.equal(shared.body.expiresAtUnixMs, first.body.expiresAtUnixMs);
  assert.equal(shared.body.sequence, first.body.sequence + 1);
  f.advance(500); await assert.rejects(f.authority.continuity(req(), { grantId: packet.grant.body.grantId }), code("session_not_authorized"));
  f.unblock(); await assert.rejects(f.authority.continuity(req(), { grantId: packet.grant.body.grantId }), code("grant_terminal"));
});

test("original host removal is idempotent, durable for that account in this policy and leaves the other guest live", async () => {
  const f = await fixture(true), { packet, policy } = await f.pair();
  const other = await f.prepared("other"), second = await f.authority.join(req("other"), { proof: other.proof, policyId: policy.policyId });
  const grantId = packet.grant.body.grantId;
  await assert.rejects(f.authority.hostRevoke(req(), { grantId }), code("broker_unavailable"));
  await assert.rejects(f.authority.hostRevoke(req("other"), { grantId }), code("broker_unavailable"));
  assert.deepEqual(await f.authority.hostRevoke(req("host"), { grantId }), { revoked: true });
  assert.deepEqual(await f.authority.hostRevoke(req("host"), { grantId }), { revoked: true });
  await assert.rejects(f.authority.continuity(req(), { grantId }), code("grant_terminal"));
  assert.equal((await f.authority.continuity(req("other"), { grantId: second.grant.body.grantId })).body.sequence, 2);
  const restored = await createSocialAdmissionAuthority(f.binding, f.dependencies, { keys: f.keys, state: f.authority.durableState(), online: true });
  const replay = await f.prepared(); await assert.rejects(restored.join(req(), { proof: replay.proof, policyId: policy.policyId }), code("session_not_authorized"));
  await assert.rejects(f.binding.prove(req(), replay.proof), code("challenge_unavailable"));
  const roster = await restored.hostPoll(req("host"), { policyId: policy.policyId });
  assert.equal(roster.state, "group"); if (roster.state !== "group") assert.fail(); assert.equal(roster.deliveries.length, 1);
  assert.equal(roster.deliveries[0].grant.body.guestAccountId, "other");
});
test("online host capacity cannot exceed its advertisement and includes the local player", async () => {
  for (const hostPlayer of [false, true]) {
    const f = await fixture(true); f.session.maxPlayers = 2;
    const host = await f.prepared("host", "host");
    const body = { ...host.policySigned.body, schemaVersion: 3, maxGuests: 3 - Number(hostPlayer), voiceProfile: "svc", voiceSourceDigest: "b".repeat(64), hostPlayer };
    const signature = hex(await crypto.subtle.sign("Ed25519", host.keys.privateKey, hostSocialPolicyPreimage(body)));
    await assert.rejects(f.authority.registerHost(req("host"), { proof: host.proof, policySigned: { body, signature } }), code("session_changed"));
  }
  const f = await fixture(true); f.session.maxPlayers = 2;
  const host = await f.prepared("host", "host");
  const body = { ...host.policySigned.body, maxGuests: 2 };
  const signature = hex(await crypto.subtle.sign("Ed25519", host.keys.privateKey, hostSocialPolicyPreimage(body)));
  const { policyId } = await f.authority.registerHost(req("host"), { proof: host.proof, policySigned: { body, signature } });
  for (const uid of ["one", "two"]) {
    const guest = await f.prepared(uid); await f.authority.join(req(uid), { proof: guest.proof, policyId });
  }
  const guest = await f.prepared("three");
  await assert.rejects(f.authority.join(req("three"), { proof: guest.proof, policyId }), code("authority_busy"));
});
