/** Exercises the real DO RPC class with controlled storage and outbound Firestore.
 * Cloudflare runtime deployment/transport is a separate check, never claimed here. */
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { before, test } from "node:test";
import { socialKeyProofPreimage } from "./connectSocialBinding.ts";
import { hostSocialPolicyPreimage } from "./connectSocialAdmission.ts";

let Actor: new (ctx: unknown, env: unknown) => { dispatch(uid: string, token: string, session: string, path: string, body: unknown): Promise<any>; relayAuthorize(ticket: any): Promise<any> };
let Budget: new (ctx: unknown, env: unknown) => { admit(session: string): Promise<boolean>; admitObservation(session: string): Promise<boolean>; alarm(): Promise<void> };
before(async () => {
  const require = createRequire(new URL("../../desktop/package.json", import.meta.url)), ts = require("typescript");
  const data = (value: string) => `data:text/javascript;base64,${Buffer.from(value).toString("base64")}`;
  let source = ts.transpileModule(await readFile(new URL("./connectSocialWorker.ts", import.meta.url), "utf8"), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
  source = source.replace('"cloudflare:workers"', JSON.stringify(data("export class DurableObject { constructor(ctx,env){this.ctx=ctx;this.env=env;} }")))
    .replace('"./connectRelayWorker.ts"', JSON.stringify(data("export class ConnectRelaySession {} export const routeRelayRequest=()=>{throw Error('unexpected relay route');};")));
  for (const name of ["connectSocialBinding.ts", "connectSocialAdmission.ts", "connectSocialFirebase.ts", "connectSocialPresence.ts"]) source = source.replaceAll(`"./${name}"`, JSON.stringify(new URL(name, import.meta.url).href));
  const worker = await import(data(source));
  Actor = worker.SocialSessionAuthority; Budget = worker.SocialAccountBudget;
});
const hex = (value: ArrayBuffer) => Buffer.from(value).toString("hex");
function storage() {
  const entries = new Map<string, unknown>();
  const value = { entries, async get(key: string) { return structuredClone(entries.get(key)); }, async put(key: string, item: unknown) { entries.set(key, structuredClone(item)); },
    async delete(key: string) { return entries.delete(key); }, async deleteAll() { entries.clear(); }, async setAlarm(_at: number) {},
    async transaction<T>(work: (txn: any) => Promise<T>): Promise<T> { return work(value); } };
  return value;
}
function fixture(t: Parameters<Parameters<typeof test>[1]>[0]) {
  const session = crypto.randomUUID(), expiry = Date.now() + 14_000_000, durable = storage();
  let inFlight = 0, maximum = 0, pauseReads = false;
  const fields = (value: Record<string, unknown>) => Object.fromEntries(Object.entries(value).map(([key, item]) => [key, item === null ? { nullValue: null } : typeof item === "number" ? { integerValue: String(item) } : key === "expiresAt" ? { timestampValue: item } : { stringValue: item }]));
  t.mock.method(globalThis, "fetch", async (url: string, init: RequestInit) => {
    inFlight++; maximum = Math.max(maximum, inFlight);
    try {
      if (pauseReads) await new Promise(resolve => setTimeout(resolve, 10));
      const uid = new Headers(init.headers).get("authorization")!.replace("Bearer credential-", "");
      const path = new URL(String(url)).pathname.split("/documents/")[1];
      if (path === `users/${uid}`) return Response.json({ fields: fields({ uid, usernameLower: `${uid}nick` }) });
      if (path === `usernames/${uid}nick`) return Response.json({ fields: fields({ uid }) });
      assert.equal(path, `socialSessions/${session}`);
      return Response.json({ fields: fields({ schema: 1, hostUid: "host", title: "Group", modpackName: "Pack", minecraftVersion: "1.21.1", loader: "Fabric", imageUrl: null, players: 1, maxPlayers: 9, visibility: "public", joinPolicy: "open", expiresAt: new Date(expiry).toISOString() }) });
    } finally { inFlight--; }
  });
  const env = { FIREBASE_PROJECT_ID: "auroralauncher" }, ctx = { storage: durable };
  let actor = new Actor(ctx, env);
  const call = (uid: string, path: string, body: unknown = {}) => actor.dispatch(uid, `credential-${uid}`, session, path, body);
  async function prepared(uid: string, role: "host" | "guest") {
    const keys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
    const challenge = await call(uid, "key/challenge", { schemaVersion: 1, operationId: crypto.randomUUID(), generation: 1, role, publicKey: hex(await crypto.subtle.exportKey("raw", keys.publicKey)), sessionId: session, preparedExpiresAtUnixMs: Date.now() + 55_000 });
    return { keys, challenge, proof: { schemaVersion: 1, challenge, signature: hex(await crypto.subtle.sign("Ed25519", keys.privateKey, socialKeyProofPreimage(challenge))) } };
  }
  async function host() {
    await call("host", "bootstrap"); const preparedHost = await prepared("host", "host"), c = preparedHost.challenge;
    const body = { schemaVersion: 2, purpose: "hostSocialSession", issuerInstance: c.issuerInstance, policyId: c.challengeId, accountId: "host", operationId: c.operationId, generation: c.generation, publicKey: c.publicKey, sessionId: session, sessionDigest: c.sessionDigest, visibility: "public", joinPolicy: "open", serviceId: "game", protocol: "tcp", sourceDigest: "a".repeat(64), maxGuests: 8, issuedAtUnixMs: c.issuedAtUnixMs, expiresAtUnixMs: expiry };
    await call("host", "host/register", { proof: preparedHost.proof, policySigned: { body, signature: hex(await crypto.subtle.sign("Ed25519", preparedHost.keys.privateKey, hostSocialPolicyPreimage(body))) } });
    return c.challengeId;
  }
  return { call, prepared, host, durable, relay: (ticket: any) => actor.relayAuthorize(ticket), parallel: () => { pauseReads = true; maximum = 0; }, maximum: () => maximum, restart: () => { actor = new Actor(ctx, env); } };
}

test("DO concurrent eight guests and host share canonical durable claims without serial Firestore reads or lost leases", async t => {
  const f = fixture(t), policyId = await f.host(); f.parallel();
  const preparations = await Promise.all(Array.from({ length: 8 }, (_, i) => f.prepared(`guest${i}`, "guest")));
  const joins = await Promise.all(preparations.map((guest, i) => f.call(`guest${i}`, "join", { proof: guest.proof, policyId })));
  assert.ok(f.maximum() >= 8, "independent Firestore authorizations must overlap");
  const values = await Promise.all([f.call("host", "host/poll", { policyId }), ...joins.map((joined, i) => f.call(`guest${i}`, "continuity", { grantId: joined.grant.body.grantId }))]);
  assert.equal(values[0].deliveries.length, 8);
  const persisted: any = f.durable.entries.get("authority"); assert.equal(persisted.busy, false); assert.equal(persisted.admission.grants.length, 8);
  assert.ok(!JSON.stringify(persisted).includes("credential-"));
  f.restart(); const restored = await f.call("host", "host/poll", { policyId }); assert.equal(restored.deliveries.length, 8);
  await assert.rejects(f.call("guest0", "key/proof", preparations[0].proof), /challenge_unavailable/);
});
test("DO restart after an interrupted durable claim retires the authority instead of reopening consumed slots", async t => {
  const f = fixture(t); await f.host(); const persisted: any = f.durable.entries.get("authority"); persisted.busy = true;
  f.durable.entries.set("authority", persisted); f.restart();
  await assert.rejects(f.call("host", "bootstrap"), /authority_retired/);
});

test("relay host freshness is independent of one expiring guest and each departed peer is denied", async t => {
  const f = fixture(t), policyId = await f.host();
  const a = await f.prepared("a", "guest"), b = await f.prepared("b", "guest");
  const joinedA = await f.call("a", "join", { proof: a.proof, policyId });
  const joinedB = await f.call("b", "join", { proof: b.proof, policyId });
  const hostTicket = await f.call("host", "relay/ticket", { role: "host", id: policyId });
  const guestTicket = await f.call("b", "relay/ticket", { role: "guest", id: joinedB.grant.body.grantId });
  const saved: any = f.durable.entries.get("authority");
  saved.admission.grants.find(([id]: [string]) => id === joinedA.grant.body.grantId)[1].lastLease.expiresAtUnixMs = Date.now() + 100;
  f.durable.entries.set("authority", saved);
  const host = await f.relay(hostTicket), guest = await f.relay(guestTicket);
  assert.ok(host.freshUntil > Date.now() + 1000, "one guest cannot shorten the host shared socket deadline");
  assert.equal(host.allowedPeers.length, 2); assert.ok(guest.freshUntil > Date.now());
  await f.call("b", "guest/stop", { grantId: joinedB.grant.body.grantId });
  await assert.rejects(f.relay(guestTicket), /relay_unauthorized/);
  assert.ok(!(await f.relay(hostTicket)).allowedPeers.includes(b.challenge.publicKey));
});

test("DO restoration retains the monotonic epoch and rejects a wall rollback without extending leases", async t => {
  const f = fixture(t); await f.host();
  const saved: any = f.durable.entries.get("authority"), offset = 500_000;
  saved.clock.monotonic += offset;
  for (const [, policy] of saved.admission.policies) { policy.deadline += offset; policy.checkedDeadline += offset; }
  saved.admission.entryWindow.until += offset; saved.admission.continuityWindow.until += offset;
  for (const [, rate] of saved.admission.accountRates) rate.until += offset;
  for (const [, pending] of saved.binding.pending) pending.deadline += offset;
  for (const [, receipt] of saved.binding.receipts) receipt.deadline += offset;
  for (const [, rate] of saved.binding.rates) rate.until += offset;
  saved.binding.globalRate.until += offset;
  f.durable.entries.set("authority", saved); f.restart(); await f.call("host", "bootstrap");
  const restored: any = f.durable.entries.get("authority");
  assert.ok(restored.clock.monotonic >= saved.clock.monotonic); assert.equal(restored.until, saved.until);
  restored.clock.wall = Date.now() + 60_000; f.durable.entries.set("authority", restored); f.restart();
  await assert.rejects(f.call("host", "bootstrap"), /authority_retired/);
});

test("DO eviction consumes the original Host freshness instead of reopening it after dormancy", async t => {
  const f = fixture(t), policyId = await f.host();
  const guest = await f.prepared("guest", "guest");
  const originalNow = Date.now, afterDormancy = originalNow() + 4000;
  t.mock.method(Date, "now", () => afterDormancy);
  t.mock.method(performance, "now", () => 0); // A new runtime monotonic epoch.
  f.restart();
  await assert.rejects(f.call("guest", "join", { proof: guest.proof, policyId }), /policy_expired/);
  await assert.rejects(f.call("host", "host/poll", { policyId }), /policy_expired/);
  const saved: any = f.durable.entries.get("authority");
  assert.equal(saved.admission.policies[0][1].state, "terminal");
  assert.equal(saved.admission.grants.length, 0);
});

test("bootstrap clock binds one caller operation nonce and original session deadline under the pinned key", async t => {
  const f = fixture(t), request = { operationId: crypto.randomUUID(), generation: 7, nonce: "ab".repeat(16) };
  f.parallel(); // Controlled Firestore latency must remain inside the signed interval.
  const before = Date.now(), response = await f.call("host", "bootstrap", request);
  assert.deepEqual(Object.keys(response.clock.body), ["schemaVersion", "issuerInstance", "accountId", "sessionId", "operationId", "generation", "nonce", "serverReceivedAtUnixMs", "serverNowUnixMs", "sessionExpiresAtUnixMs"]);
  assert.equal(response.clock.body.operationId, request.operationId); assert.equal(response.clock.body.generation, 7);
  assert.equal(response.clock.body.nonce, request.nonce); assert.equal(response.clock.body.accountId, "host");
  assert.equal(response.clock.body.issuerInstance, response.issuerInstance);
  assert.equal(response.clock.body.sessionId, response.sessionId);
  assert.equal(response.clock.body.sessionExpiresAtUnixMs, response.expiresAtUnixMs);
  assert.ok(response.clock.body.serverReceivedAtUnixMs >= before && response.clock.body.serverReceivedAtUnixMs <= response.clock.body.serverNowUnixMs);
  assert.ok(response.clock.body.serverNowUnixMs - response.clock.body.serverReceivedAtUnixMs >= 20, "entry timestamp precedes outbound Firestore waits");
  assert.ok(response.clock.body.serverNowUnixMs >= before && response.clock.body.serverNowUnixMs <= Date.now());
  const key = await crypto.subtle.importKey("raw", Buffer.from(response.admissionPublicKey, "hex"), "Ed25519", false, ["verify"]);
  const verify = (body: object) => crypto.subtle.verify("Ed25519", key, Buffer.from(response.clock.signature, "hex"), new TextEncoder().encode("aurora/connect/social-clock/v1\0" + JSON.stringify(body)));
  assert.equal(await verify(response.clock.body), true);
  assert.equal(await verify({ ...response.clock.body, nonce: "cd".repeat(16) }), false);
  assert.equal(await verify({ ...response.clock.body, accountId: "guest" }), false);
  assert.equal(await verify({ ...response.clock.body, serverReceivedAtUnixMs: response.clock.body.serverReceivedAtUnixMs - 1 }), false);
  // Override values without changing the field order emitted by the Worker.
  const golden = { ...response.clock.body, issuerInstance: "ab".repeat(32), accountId: "test_account",
    sessionId: "26bc2f10-2af8-4da6-88d2-4524a0061bc0", operationId: "095d4404-8ce1-4b3f-bb12-7881d724bd72",
    generation: 3, nonce: "0123456789abcdef0123456789abcdef", serverReceivedAtUnixMs: 1_799_999_999_500, serverNowUnixMs: 1_800_000_000_000,
    sessionExpiresAtUnixMs: 1_800_000_020_000 };
  const goldenKey = await crypto.subtle.importKey("raw", Buffer.from("2152f8d19b791d24453242e15f2eab6cb7cffa7b6a5ed30097960e069881db12", "hex"), "Ed25519", false, ["verify"]);
  const goldenSignature = Buffer.from("2534fb8a5a76e970d61abf4762cab7f65179df3175cf0fa26a95c290968e1f5470bff02a31bda3666e1676fe7298fc0d6402cbb8e981ff41144524770216f300", "hex");
  assert.equal(await crypto.subtle.verify("Ed25519", goldenKey, goldenSignature, new TextEncoder().encode("aurora/connect/social-clock/v1\0" + JSON.stringify(golden))), true);
  assert.equal((await f.call("host", "bootstrap")).clock, undefined, "legacy response remains compatible");
  for (const invalid of [{ ...request, generation: 0 }, { ...request, nonce: "ab" }, { ...request, extra: true }, { ...request, operationId: "not-a-uuid" }])
    await assert.rejects(f.call("host", "bootstrap", invalid), /invalid_request/);
});

test("catalog presence never creates an authority and distinguishes admitted leases from native connected peers", async t => {
  const f = fixture(t);
  assert.equal((await f.call("reader", "presence")).state, "unavailable");
  assert.equal(f.durable.entries.size, 0, "catalog does not bootstrap keys, leases or durable session state");
  const policyId = await f.host(), a = await f.prepared("a", "guest"), b = await f.prepared("b", "guest");
  const joinedA = await f.call("a", "join", { proof: a.proof, policyId });
  await f.call("b", "join", { proof: b.proof, policyId });
  assert.equal((await f.call("reader", "presence")).state, "unknown", "legacy host polling cannot invent a connected count");
  await f.call("host", "host/poll", { policyId, transport: { observedAtUnixMs: Date.now(), connectedGrantIds: [joinedA.grant.body.grantId], hostPlayerConnected: false } });
  const saved = JSON.stringify([...f.durable.entries]);
  const value = await f.call("reader", "presence");
  assert.equal(value.state, "ready"); assert.equal(value.admittedGuests, 2); assert.equal(value.connectedGuests, 1);
  assert.equal(value.hostPlayerConnected, false); assert.equal(value.maxGuests, 8); assert.equal(value.accountId, "reader");
  assert.ok(value.expiresAtUnixMs - value.checkedAtUnixMs <= 3000);
  assert.equal(JSON.stringify([...f.durable.entries]), saved, "observation cannot checkpoint, refresh or extend authority");
  assert.doesNotMatch(JSON.stringify(value), /guestPublicKey|grantId|policyId|credential-/);
  await f.call("host", "host/revoke", { grantId: joinedA.grant.body.grantId });
  const remaining = await f.call("reader", "presence"); assert.equal(remaining.admittedGuests, 1); assert.equal(remaining.connectedGuests, 0);
  await f.call("host", "host/stop", { policyId }); assert.equal((await f.call("reader", "presence")).state, "unavailable");
});

test("presence freshness expires independently and native transport reports cannot claim foreign or duplicate peers", async t => {
  const f = fixture(t), policyId = await f.host();
  await assert.rejects(f.call("reader", "host/poll", { policyId, transport: { observedAtUnixMs: Date.now(), connectedGrantIds: [], hostPlayerConnected: false } }), /broker_unavailable/);
  await assert.rejects(f.call("host", "host/poll", { policyId, transport: { observedAtUnixMs: Date.now(), connectedGrantIds: ["ab".repeat(32), "ab".repeat(32)], hostPlayerConnected: false } }), /invalid_request/);
  await f.call("host", "host/poll", { policyId, transport: { observedAtUnixMs: Date.now(), connectedGrantIds: [], hostPlayerConnected: false } });
  const original = JSON.stringify([...f.durable.entries]), saved: any = f.durable.entries.get("authority");
  const now = Date.now(); t.mock.method(Date, "now", () => now + 3100);
  assert.equal((await f.call("reader", "presence")).state, "unavailable");
  assert.equal(JSON.stringify([...f.durable.entries]), original);
  assert.equal(saved.admission.policies[0][1].body.expiresAtUnixMs > now + 3100, true, "original session remains long lived");
});

test("catalog rate budget is bounded across actor restart and cannot consume or reset admission capacity", async t => {
  let now = 1_800_000_000_000; t.mock.method(Date, "now", () => now);
  const durable = storage(), ctx = { storage: durable }, env = {};
  let budget = new Budget(ctx, env);
  for (let i = 0; i < 32; i++) assert.equal(await budget.admitObservation(`session-${i}`), true);
  assert.equal(await budget.admitObservation("session-32"), false, "at most 32 observed session keys per minute");
  for (let i = 32; i < 240; i++) assert.equal(await budget.admitObservation("session-0"), true);
  budget = new Budget(ctx, env);
  assert.equal(await budget.admitObservation("session-0"), false, "eviction cannot reset the 240-observation budget");
  now += 10_000;
  for (let i = 0; i < 300; i++) assert.equal(await budget.admit("own-session"), true);
  assert.equal(await budget.admit("own-session"), false);
  assert.equal(durable.entries.size, 2, "only two bounded rows exist per account");
  now += 50_001; await budget.alarm();
  assert.equal(durable.entries.has("observations"), false);
  assert.equal(await budget.admit("own-session"), false, "expiry of display budget cannot reset live authority budget");
  assert.equal(await budget.admitObservation("session-new"), true);
  now += 10_000; await budget.alarm();
  assert.equal(durable.entries.has("budget"), false);
  assert.equal(durable.entries.has("observations"), true);
});
