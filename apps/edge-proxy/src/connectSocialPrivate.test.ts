import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { createSocialBindingAuthority, socialKeyProofPreimage, SocialBindingError, type SocialBindingSession, type SocialPrivateAuthorization } from "./connectSocialBinding.ts";
import { createSocialAdmissionAuthority, hostSocialPolicyPreimage, socialAdmissionPreimage, socialJoinContextPreimage,
  validateHostSocialPolicy, validateSocialAdmissionGrant, validateSocialJoinContext } from "./connectSocialAdmission.ts";
import { privateAuthorizationFields } from "./connectSocialPrivate.ts";
import { createSocialFirebaseDependencies } from "./connectSocialFirebase.ts";

const hex = (bytes: ArrayBuffer) => Buffer.from(bytes).toString("hex");
const request = (uid = "guest", signal?: AbortSignal) => new Request("http://127.0.0.1/fixture", { signal, headers: { Authorization: `Bearer fixture-${uid}` } });
const errorCode = (expected: string) => (error: unknown) => error instanceof SocialBindingError && error.code === expected;
test("private canonical vectors keep schema 6/7 and context 2 byte-identical across Rust and TypeScript", async () => {
  const fixture = JSON.parse(await readFile(new URL("../../connect-lab/tests/fixtures/social-private-vectors.json", import.meta.url), "utf8"));
  assert.equal(fixture.vectors.length, 2);
  for (const vector of fixture.vectors) for (const [name, preimage] of [["policy", hostSocialPolicyPreimage], ["grant", socialAdmissionPreimage], ["context", socialJoinContextPreimage]] as const) {
    const expected = vector[name], bytes = preimage(expected.body);
    assert.equal(Buffer.from(bytes).toString("hex"), expected.preimageHex);
    const key = await crypto.subtle.importKey("raw", Buffer.from(expected.signerPublicKey, "hex"), "Ed25519", false, ["verify"]);
    assert.ok(await crypto.subtle.verify("Ed25519", key, Buffer.from(expected.signature, "hex"), bytes));
  }
});
async function fixture({ schema = 6, online = true, enabled = true } = {}) {
  let wall = 1_800_000_000_000, mono = 1000, blocked = false, active = true, authReads = 0;
  let beforeAuthorization: (() => Promise<void>) | undefined;
  const session: SocialBindingSession = { id: crypto.randomUUID(), hostUid: "host", title: "Private world", modpackName: "Local", minecraftVersion: "1.21.1", loader: "Fabric",
    imageUrl: null, players: 1, maxPlayers: online ? 8 : 2, visibility: "private", joinPolicy: "open", expiresAtMs: wall + 14_395_000 };
  const authorization: SocialPrivateAuthorization = { authorizationId: "d".repeat(64), revision: 1, sessionId: session.id, hostUid: "host", recipientUid: "guest", expiresAtMs: session.expiresAtMs };
  const dependencies = {
    clock: { wall: () => wall, monotonic: () => mono },
    trustedAuthenticate: async (r: Request) => r.headers.get("authorization")!.slice(15),
    readSession: async (p: { accountId: string }, id: string) => {
      if (id !== session.id || p.accountId !== "host" && (p.accountId !== "guest" || blocked || !active)) throw new SocialBindingError("session_not_authorized");
      return { ...session };
    },
    readPrivateAuthorization: async (_p: { accountId: string }, _id: string, _recipient: string) => {
      authReads++; await beforeAuthorization?.();
      if (!active) throw new SocialBindingError("session_not_authorized");
      return { ...authorization };
    },
  };
  const signingKeys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
  const binding = createSocialBindingAuthority(dependencies);
  const authority = await createSocialAdmissionAuthority(binding, dependencies, { keys: signingKeys, online, privateInvitations: enabled });
  async function prepare(uid = "guest", role: "host" | "guest" = "guest") {
    const keys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]), publicKey = hex(await crypto.subtle.exportKey("raw", keys.publicKey));
    const challenge = await binding.challenge(request(uid), { schemaVersion: 1, operationId: crypto.randomUUID(), generation: 1, role, publicKey, sessionId: session.id, preparedExpiresAtUnixMs: wall + 55_000 });
    return { keys, proof: { schemaVersion: 1, challenge, signature: hex(await crypto.subtle.sign("Ed25519", keys.privateKey, socialKeyProofPreimage(challenge))) } };
  }
  async function host() {
    const prepared = await prepare("host", "host"), c = prepared.proof.challenge;
    const body = { schemaVersion: schema, purpose: "hostSocialSession", issuerInstance: c.issuerInstance, policyId: c.challengeId, accountId: "host", operationId: c.operationId, generation: c.generation,
      publicKey: c.publicKey, sessionId: session.id, sessionDigest: c.sessionDigest, visibility: "private", joinPolicy: "open", serviceId: "game", protocol: "tcp", sourceDigest: "a".repeat(64),
      maxGuests: online ? schema === 7 ? 7 : 8 : 1, issuedAtUnixMs: c.issuedAtUnixMs, expiresAtUnixMs: online ? session.expiresAtMs : c.expiresAtUnixMs,
      ...(schema === 7 ? { voiceProfile: "svc", voiceSourceDigest: "b".repeat(64), hostPlayer: true, runtimeProfile: "fabric-loader-0.16.14-1.21.1", voiceTransportProfile: "nativeVoice8V1" } : {}) };
    const policySigned = { body, signature: hex(await crypto.subtle.sign("Ed25519", prepared.keys.privateKey, hostSocialPolicyPreimage(body))) };
    const policy = await authority.registerHost(request("host"), { proof: prepared.proof, policySigned });
    return { ...prepared, policySigned, policy };
  }
  async function join(policyId: string, revision = authorization.revision) {
    const guest = await prepare();
    return authority.join(request(), { proof: guest.proof, policyId, authorizationId: authorization.authorizationId, authorizationRevision: revision });
  }
  return { authority, binding, signingKeys, dependencies, session, authorization, host, prepare, join,
    advance: (ms: number) => { wall += ms; mono += ms; }, wall: () => wall, reads: () => authReads,
    active: (value: boolean) => { active = value; }, block: () => { blocked = true; }, onAuthorization: (fn: () => Promise<void>) => { beforeAuthorization = fn; } };
}

for (const schema of [6, 7]) test(`private schema ${schema} signs recipient revision and preserves transport/service deadlines`, async () => {
  const f = await fixture({ schema }), { policy, policySigned } = await f.host();
  const context = await f.authority.resolveContext(request(), { sessionId: f.session.id });
  assert.equal(context.body.schemaVersion, 2); assert.equal(context.body.authorizationId, f.authorization.authorizationId); assert.equal(context.body.authorizationRevision, 1);
  const packet = await f.join(policy.policyId), grant = packet.grant.body;
  assert.equal(grant.schemaVersion, schema); assert.equal(grant.authorizationRevision, context.body.authorizationRevision); assert.equal(grant.authorizationId, context.body.authorizationId);
  assert.equal(grant.sessionExpiresAtUnixMs, policySigned.body.expiresAtUnixMs); assert.equal(packet.lease.body.expiresAtUnixMs - packet.lease.body.issuedAtUnixMs, 3000);
  assert.ok(grant.admissionExpiresAtUnixMs - grant.issuedAtUnixMs <= (schema === 7 ? 60_000 : 10_000));
  const key = await crypto.subtle.importKey("raw", Buffer.from(f.authority.admissionPublicKey, "hex"), "Ed25519", false, ["verify"]);
  assert.ok(await crypto.subtle.verify("Ed25519", key, Buffer.from(packet.grant.signature, "hex"), socialAdmissionPreimage(grant)));
  assert.ok(await crypto.subtle.verify("Ed25519", key, Buffer.from(context.signature, "hex"), socialJoinContextPreimage(context.body)));
  const poll = await f.authority.hostPoll(request("host"), { policyId: policy.policyId }); assert.equal(poll.state, "group");
  await f.authority.hostPublish(request("host"), { grantId: grant.grantId, invitation: "opaque fixture" });
  assert.equal((await f.authority.guestReceive(request(), { grantId: grant.grantId })).state, "ready");
});

test("private schemas and restored private state remain unavailable without explicit local authority opt-in", async () => {
  for (const schema of [6, 7]) { const f = await fixture({ schema, enabled: false }); await assert.rejects(f.host(), errorCode("policy_unavailable")); }
  const f = await fixture(); await f.host();
  await assert.rejects(createSocialAdmissionAuthority(f.binding, f.dependencies, { keys: f.signingKeys, online: true, state: f.authority.durableState() }), errorCode("policy_unavailable"));
});

test("private local single mode keeps original preparation, one slot and admitted delivery", async () => {
  const f = await fixture({ online: false }), { policy, policySigned } = await f.host(), packet = await f.join(policy.policyId);
  assert.equal(policySigned.body.maxGuests, 1); assert.ok(policySigned.body.expiresAtUnixMs - policySigned.body.issuedAtUnixMs <= 60_000);
  assert.equal(packet.grant.body.sessionExpiresAtUnixMs, policySigned.body.expiresAtUnixMs);
  assert.equal((await f.authority.hostPoll(request("host"), { policyId: policy.policyId })).state, "admitted");
  await assert.rejects(f.join(policy.policyId), errorCode("authority_busy"));
});

test("private local reinvitation delivers the new grant without selecting a terminal predecessor", async () => {
  const f = await fixture({ online: false }), { policy } = await f.host(), first = await f.join(policy.policyId);
  await f.authority.guestStop(request(), { grantId: first.grant.body.grantId });
  f.authorization.revision = 3;
  const context = await f.authority.resolveContext(request(), { sessionId: f.session.id }); assert.equal(context.body.authorizationRevision, 3);
  const second = await f.join(policy.policyId), delivery = await f.authority.hostPoll(request("host"), { policyId: policy.policyId });
  assert.equal(delivery.state, "admitted"); if (delivery.state !== "admitted") assert.fail();
  assert.equal(delivery.grant.body.grantId, second.grant.body.grantId); assert.notEqual(delivery.grant.body.grantId, first.grant.body.grantId);
  assert.equal(delivery.grant.body.authorizationRevision, 3); assert.equal(delivery.grant.body.sessionExpiresAtUnixMs, first.grant.body.sessionExpiresAtUnixMs);
});

test("private closed codecs reject downgrade, missing/invalid revisions and private approval", async () => {
  const f = await fixture(), { policy, policySigned } = await f.host(), packet = await f.join(policy.policyId), context = await f.authority.resolveContext(request(), { sessionId: f.session.id });
  for (const revision of [0, -1, 1.1, Infinity, 0x1_0000_0000, undefined]) {
    assert.throws(() => validateSocialAdmissionGrant({ ...packet.grant.body, authorizationRevision: revision }));
    assert.throws(() => validateSocialJoinContext({ ...context.body, authorizationRevision: revision }));
  }
  for (const schemaVersion of [1, 2, 3, 4, 5]) assert.throws(() => validateHostSocialPolicy({ ...policySigned.body, schemaVersion }));
  assert.throws(() => validateHostSocialPolicy({ ...policySigned.body, joinPolicy: "approval" }));
  assert.throws(() => validateHostSocialPolicy({ ...policySigned.body, visibility: "public" }));
  assert.throws(() => validateSocialAdmissionGrant({ ...packet.grant.body, schemaVersion: 2 }));
  assert.throws(() => validateSocialJoinContext({ ...context.body, schemaVersion: 1 }));
});

test("revocation and immediate reinvitation cannot revive an old grant even without an observed revoked interval", async () => {
  const f = await fixture(), { policy } = await f.host(), packet = await f.join(policy.policyId), grantId = packet.grant.body.grantId;
  // Two atomic writes may occur between observations; their revision remains evidence of changed authority.
  f.authorization.revision += 2;
  await assert.rejects(f.authority.continuity(request(), { grantId }), errorCode("session_not_authorized"));
  f.authorization.revision = 1;
  await assert.rejects(f.authority.continuity(request(), { grantId }), errorCode("grant_terminal"));
  f.authorization.revision = 3;
  const next = await f.join(policy.policyId); assert.equal(next.grant.body.authorizationRevision, 3); assert.notEqual(next.grant.body.grantId, grantId);
  assert.equal(next.grant.body.sessionExpiresAtUnixMs, packet.grant.body.sessionExpiresAtUnixMs);
});

test("context authorization is checked again before a later join and input cannot omit it", async () => {
  const f = await fixture(), { policy } = await f.host(); await f.authority.resolveContext(request(), { sessionId: f.session.id });
  f.authorization.revision = 3; await assert.rejects(f.join(policy.policyId, 1), errorCode("session_not_authorized"));
  const guest = await f.prepare();
  await assert.rejects(f.authority.join(request(), { proof: guest.proof, policyId: policy.policyId }), errorCode("invalid_request"));
  await assert.rejects(f.authority.resolveContext(request("third"), { sessionId: f.session.id }), errorCode("context_unavailable"));
  assert.equal(f.authority.inspect().terminalGrants, 0);
});

test("private fresh reference checks isolate account/session/host/expiry and stop retained delivery", async () => {
  for (const change of [ { authorizationId: "e".repeat(64) }, { recipientUid: "other" }, { hostUid: "other" }, { sessionId: crypto.randomUUID() }, { expiresAtMs: 1_800_000_000_001 } ]) {
    const f = await fixture(), { policy } = await f.host(), packet = await f.join(policy.policyId);
    Object.assign(f.authorization, change);
    await assert.rejects(f.authority.hostPublish(request("host"), { grantId: packet.grant.body.grantId, invitation: "must not deliver" }), errorCode("session_not_authorized"));
    assert.equal(f.authority.inspect().terminalGrants, 1);
  }
});

test("private continuity retains the original cached observation and becomes terminal on block/cancel/slow read", async () => {
  for (const reason of ["block", "cancel", "slow"] as const) {
    const f = await fixture(), { policy } = await f.host(), packet = await f.join(policy.policyId), grantId = packet.grant.body.grantId;
    const abort = new AbortController();
    if (reason === "block") f.block();
    else f.onAuthorization(async () => { if (reason === "cancel") abort.abort(); else f.advance(3001); });
    await assert.rejects(f.authority.continuity(request("guest", abort.signal), { grantId }));
    assert.equal(f.authority.inspect().terminalGrants, 1);
  }
  const f = await fixture(), { policy } = await f.host(), packet = await f.join(policy.policyId), grantId = packet.grant.body.grantId;
  const lease = await f.authority.continuity(request(), { grantId }); f.authorization.revision = 3; f.advance(500);
  const cached = await f.authority.continuity(request(), { grantId }); assert.equal(cached.body.expiresAtUnixMs, lease.body.expiresAtUnixMs);
  f.advance(501); await assert.rejects(f.authority.continuity(request(), { grantId }), errorCode("session_not_authorized"));
});

function fields(now = Date.now()) {
  const text = (stringValue: string) => ({ stringValue }), timestamp = (ms: number) => ({ timestampValue: new Date(ms).toISOString() });
  return { schema: { integerValue: "1" }, authorizationId: text("d".repeat(64)), revision: { integerValue: "1" }, sessionId: text("11111111-1111-4111-8111-111111111111"),
    hostUid: text("host"), recipientUid: text("guest"), status: text("active"), createdAt: timestamp(now - 1000), updatedAt: timestamp(now - 100), expiresAt: timestamp(now + 10000) };
}
test("private Firestore projection rejects tombstones, malformed time/revisions and unknown fields", () => {
  const now = Date.now(), valid = fields(now), sessionId = valid.sessionId.stringValue;
  assert.equal(privateAuthorizationFields(valid, sessionId, "guest", "host", now).revision, 1);
  for (const change of [{ status: { stringValue: "revoked" } }, { revision: { integerValue: "01" } }, { revision: { integerValue: "4294967296" } },
    { expiresAt: { timestampValue: "invalid" } }, { createdAt: { timestampValue: new Date(now + 1).toISOString() } }, { extra: { stringValue: "no" } }]) {
    assert.throws(() => privateAuthorizationFields({ ...valid, ...change }, sessionId, "guest", "guest", now), errorCode("session_not_authorized"));
  }
  assert.throws(() => privateAuthorizationFields(valid, sessionId, "guest", "third", now), errorCode("session_not_authorized"));
});

test("production adapter authorization read uses only exact recipient path and current caller token", async t => {
  const valid = fields(), sessionId = valid.sessionId.stringValue, calls: string[] = [];
  let data: Record<string, unknown> = valid;
  t.mock.method(globalThis, "fetch", async (url: string, init: RequestInit) => {
    calls.push(url); assert.equal(init.redirect, "manual"); assert.deepEqual(init.headers, { Authorization: "Bearer fixture-guest" });
    return Response.json({ fields: data });
  });
  const dependencies = createSocialFirebaseDependencies("auroralauncher", "guest", sessionId), principal = { accountId: "guest", request: request() };
  const signal = new AbortController().signal;
  assert.equal((await dependencies.readPrivateAuthorization!(principal, sessionId, "guest", signal)).revision, 1);
  data = { ...valid, status: { stringValue: "revoked" } };
  await assert.rejects(dependencies.readPrivateAuthorization!(principal, sessionId, "guest", signal), errorCode("session_not_authorized"));
  await assert.rejects(dependencies.readPrivateAuthorization!(principal, sessionId, "../host", signal), errorCode("session_not_authorized"));
  assert.equal(calls.length, 2); assert.ok(calls.every(url => url.endsWith(`/socialPrivateInvitations/${sessionId}/guests/guest`)));
});
