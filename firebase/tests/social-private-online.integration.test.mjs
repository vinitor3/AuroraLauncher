// Real Auth/Firestore emulator boundary + actual Worker DO class. Its runtime
// and storage are local fixtures; this is not a hosted Cloudflare or TCP test.
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { account, delay, socialFactory } from "./social-desktop-fixtures.mjs";
import { createLocalEmulatorBindingDependencies } from "./social-binding-server.mjs";
import { socialKeyProofPreimage } from "../../apps/edge-proxy/src/connectSocialBinding.ts";
import { hostSocialPolicyPreimage, socialAdmissionPreimage, socialJoinContextPreimage } from "../../apps/edge-proxy/src/connectSocialAdmission.ts";

async function actorClass() {
  const require = createRequire(new URL("../../apps/desktop/package.json", import.meta.url)), ts = require("typescript");
  const data = value => `data:text/javascript;base64,${Buffer.from(value).toString("base64")}`;
  const base = new URL("../../apps/edge-proxy/src/", import.meta.url);
  let source = ts.transpileModule(await readFile(new URL("connectSocialWorker.ts", base), "utf8"), {
    compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  source = source.replace('"cloudflare:workers"', JSON.stringify(data("export class DurableObject { constructor(ctx,env){this.ctx=ctx;this.env=env;} }")))
    .replace('"./connectRelayWorker.ts"', JSON.stringify(data("export class ConnectRelaySession {} export const routeRelayRequest=()=>{throw Error('UNEXPECTED_RELAY_ROUTE');};")));
  for (const name of ["connectSocialBinding.ts", "connectSocialAdmission.ts", "connectSocialFirebase.ts", "connectSocialPresence.ts"])
    source = source.replaceAll(`"./${name}"`, JSON.stringify(new URL(name, base).href));
  return (await import(data(source))).SocialSessionAuthority;
}
function storage() {
  const entries = new Map();
  return { entries, async get(key) { return structuredClone(entries.get(key)); }, async put(key, value) { entries.set(key, structuredClone(value)); },
    async deleteAll() { entries.clear(); }, async setAlarm() {} };
}
const hex = bytes => Buffer.from(bytes).toString("hex");
test("private online Worker schema 6/7 observes real Firebase authorization, revocation and original deadlines", {
  skip: process.env.AURORA_SOCIAL_WORKER_INTEGRATION !== "1", timeout: 90_000,
}, async t => {
  const originalFetch = globalThis.fetch, Actor = await actorClass(), authenticate = createLocalEmulatorBindingDependencies().trustedAuthenticate;
  let externalFetches = 0, routedFirestoreReads = 0;
  // Production adapter paths and headers are retained, but only the explicit
  // demo project is routed into the local emulator. No network fallback exists.
  t.mock.method(globalThis, "fetch", async (input, init) => {
    assert.equal(typeof input, "string"); const url = new URL(input);
    if (url.protocol === "https:" && url.hostname === "firestore.googleapis.com"
      && url.pathname.startsWith("/v1/projects/demo-aurora-social/databases/(default)/documents/")) {
      assert.equal(init?.redirect, "manual"); assert.equal(init?.method ?? "GET", "GET"); routedFirestoreReads++;
      return originalFetch(`http://127.0.0.1:8187${url.pathname}`, { ...init, redirect: "error" });
    }
    if (url.protocol === "http:" && url.hostname === "127.0.0.1" && ["8187", "9187"].includes(url.port)) return originalFetch(input, init);
    externalFetches++; throw new Error("EXTERNAL_FETCH_DENIED");
  });
  const factory = await socialFactory();
  for (const schema of [6, 7]) await t.test(`schema ${schema}`, async () => {
    let host, guest, other, primary, cleanupError;
    const started = performance.now();
    try {
      host = await account(factory, "owhost"); guest = await account(factory, "owgest"); other = await account(factory, "owothr");
      await host.service.publishSession({ title: "Private online contract fixture", modpackName: "Worker authority", minecraftVersion: "1.21.1", loader: "Fabric",
        imageUrl: null, players: 1, maxPlayers: 8, visibility: "private", joinPolicy: "open" });
      const sessionId = await host.sessionId(); await host.service.privateInvitations.invite(sessionId, guest.uid);
      const invitedAt = performance.now(), durable = storage(), env = { FIREBASE_PROJECT_ID: "demo-aurora-social", CONNECT_SOCIAL_PRIVATE_INVITATIONS: "1" };
      let actor = new Actor({ storage: durable }, env);
      const call = async (who, path, body = {}) => {
        const token = await who.token(), request = new Request("http://127.0.0.1:42888/local-proof", { headers: { Authorization: `Bearer ${token}` } });
        const verifiedUid = await authenticate(request, AbortSignal.timeout(5000)); assert.equal(verifiedUid, who.uid);
        return actor.dispatch(verifiedUid, token, sessionId, path, body);
      };
      const bootstrap = await call(host, "bootstrap");
      async function prepare(who, role) {
        const keys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
        const challenge = await call(who, "key/challenge", { schemaVersion: 1, operationId: crypto.randomUUID(), generation: 1, role,
          publicKey: hex(await crypto.subtle.exportKey("raw", keys.publicKey)), sessionId, preparedExpiresAtUnixMs: Date.now() + 55_000 });
        return { keys, challenge, proof: { schemaVersion: 1, challenge, signature: hex(await crypto.subtle.sign("Ed25519", keys.privateKey, socialKeyProofPreimage(challenge))) } };
      }
      const h = await prepare(host, "host"), c = h.challenge;
      const body = { schemaVersion: schema, purpose: "hostSocialSession", issuerInstance: c.issuerInstance, policyId: c.challengeId, accountId: c.accountId,
        operationId: c.operationId, generation: c.generation, publicKey: c.publicKey, sessionId, sessionDigest: c.sessionDigest, visibility: "private", joinPolicy: "open",
        serviceId: "game", protocol: "tcp", sourceDigest: "a".repeat(64), maxGuests: schema === 7 ? 7 : 8, issuedAtUnixMs: c.issuedAtUnixMs, expiresAtUnixMs: bootstrap.expiresAtUnixMs,
        ...(schema === 7 ? { voiceProfile: "svc", voiceSourceDigest: "b".repeat(64), hostPlayer: true, runtimeProfile: "fabric-loader-0.16.14-1.21.1", voiceTransportProfile: "nativeVoice8V1" } : {}) };
      const signed = { body, signature: hex(await crypto.subtle.sign("Ed25519", h.keys.privateKey, hostSocialPolicyPreimage(body))) };
      await call(host, "host/register", { proof: h.proof, policySigned: signed });
      await assert.rejects(call(other, "context", { sessionId }), /context_unavailable/);
      const context = await call(guest, "context", { sessionId }), g = await prepare(guest, "guest");
      assert.equal(context.body.schemaVersion, 2); assert.equal(context.body.recipientAccountId, guest.uid);
      const joined = await call(guest, "join", { proof: g.proof, policyId: body.policyId, authorizationId: context.body.authorizationId, authorizationRevision: context.body.authorizationRevision });
      assert.equal(joined.grant.body.schemaVersion, schema); assert.equal(joined.grant.body.authorizationRevision, 1);
      const key = await crypto.subtle.importKey("raw", Buffer.from(bootstrap.admissionPublicKey, "hex"), "Ed25519", false, ["verify"]);
      for (const [packet, encoder] of [[context, socialJoinContextPreimage], [joined.grant, socialAdmissionPreimage]])
        assert.ok(await crypto.subtle.verify("Ed25519", key, Buffer.from(packet.signature, "hex"), encoder(packet.body)));
      assert.equal(joined.grant.body.sessionExpiresAtUnixMs, body.expiresAtUnixMs);
      assert.ok(joined.lease.body.expiresAtUnixMs - joined.lease.body.issuedAtUnixMs <= 3000);
      await call(host, "host/poll", { policyId: body.policyId });
      await call(host, "host/publish", { grantId: joined.grant.body.grantId, invitation: "opaque-envelope-fixture-not-a-transport" });
      assert.equal((await call(guest, "guest/receive", { grantId: joined.grant.body.grantId })).state, "ready");
      await host.service.privateInvitations.revoke(sessionId, guest.uid);
      await assert.rejects(call(guest, "continuity", { grantId: joined.grant.body.grantId }), /session_not_authorized/);
      assert.equal(durable.entries.get("authority").admission.grants[0][1].terminal, true);
      await call(host, "host/poll", { policyId: body.policyId });
      await delay(Math.max(0, 2100 - (performance.now() - invitedAt)));
      await host.service.privateInvitations.invite(sessionId, guest.uid);
      await assert.rejects(call(guest, "continuity", { grantId: joined.grant.body.grantId }), /grant_terminal/);
      const latest = await call(guest, "context", { sessionId }); assert.equal(latest.body.authorizationRevision, 3);
      assert.equal(latest.body.authorizationId, context.body.authorizationId);
      const fresh = await prepare(guest, "guest"), next = await call(guest, "join", { proof: fresh.proof, policyId: body.policyId,
        authorizationId: latest.body.authorizationId, authorizationRevision: latest.body.authorizationRevision });
      assert.equal(next.grant.body.sessionExpiresAtUnixMs, joined.grant.body.sessionExpiresAtUnixMs); assert.notEqual(next.grant.body.grantId, joined.grant.body.grantId);
      await call(host, "host/poll", { policyId: body.policyId });
      actor = new Actor({ storage: durable }, env);
      const restored = await call(host, "host/poll", { policyId: body.policyId }); assert.equal(restored.deliveries.length, 1);
      await call(host, "host/stop", { policyId: body.policyId });
      assert.equal(durable.entries.get("authority").admission.policies[0][1].state, "terminal");
      assert.ok(!JSON.stringify([...durable.entries]).includes(await guest.token()));
      t.diagnostic(JSON.stringify({ schema, realAuthEmulator: true, realFirestoreRules: true, actualWorkerClass: true, localStorageFixture: true,
        thirdAccountDenied: true, revisionBound: true, revokeTerminal: true, reissueRetainsSessionDeadline: true, restartPreservedClaims: true,
        elapsedMs: Math.round(performance.now() - started), hostedCloudflare: false, transport: "separate_native_tests", minecraft: "not_tested", voice: "not_tested" }));
    } catch (error) { primary = error; }
    finally {
      for (const who of [other, guest, host]) if (who) try { await who.close(); } catch (error) { cleanupError ??= error; }
    }
    if (primary && cleanupError) throw new AggregateError([primary, cleanupError], "PRIVATE_ONLINE_AND_CLEANUP_FAILED");
    if (cleanupError) throw cleanupError;
    if (primary) throw primary;
  });
  assert.equal(externalFetches, 0); assert.ok(routedFirestoreReads > 0);
});
