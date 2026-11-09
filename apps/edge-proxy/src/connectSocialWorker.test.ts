/** Exercises the real DO RPC class with controlled storage and outbound Firestore.
 * Cloudflare runtime deployment/transport is a separate check, never claimed here. */
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { before, test } from "node:test";
import { socialKeyProofPreimage } from "./connectSocialBinding.ts";
import { hostSocialPolicyPreimage } from "./connectSocialAdmission.ts";

let Actor: new (ctx: unknown, env: unknown) => { dispatch(uid: string, token: string, session: string, path: string, body: unknown): Promise<any>; relayAuthorize(ticket: any): Promise<any> };
before(async () => {
  const require = createRequire(new URL("../../desktop/package.json", import.meta.url)), ts = require("typescript");
  const data = (value: string) => `data:text/javascript;base64,${Buffer.from(value).toString("base64")}`;
  let source = ts.transpileModule(await readFile(new URL("./connectSocialWorker.ts", import.meta.url), "utf8"), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
  source = source.replace('"cloudflare:workers"', JSON.stringify(data("export class DurableObject { constructor(ctx,env){this.ctx=ctx;this.env=env;} }")))
    .replace('"./connectRelayWorker.ts"', JSON.stringify(data("export class ConnectRelaySession {} export const routeRelayRequest=()=>{throw Error('unexpected relay route');};")));
  for (const name of ["connectSocialBinding.ts", "connectSocialAdmission.ts", "connectSocialFirebase.ts"]) source = source.replaceAll(`"./${name}"`, JSON.stringify(new URL(name, import.meta.url).href));
  Actor = (await import(data(source))).SocialSessionAuthority;
});
const hex = (value: ArrayBuffer) => Buffer.from(value).toString("hex");
function storage() {
  const entries = new Map<string, unknown>();
  return { entries, async get(key: string) { return structuredClone(entries.get(key)); }, async put(key: string, value: unknown) { entries.set(key, structuredClone(value)); }, async deleteAll() { entries.clear(); }, async setAlarm(_at: number) {} };
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
  for (const [, policy] of saved.admission.policies) policy.deadline += offset;
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
