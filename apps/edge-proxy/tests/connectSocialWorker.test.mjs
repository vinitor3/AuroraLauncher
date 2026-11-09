// Real Worker router, crypto and SQLite Durable Objects. Only upstream Google
// responses are controlled; this test never contacts Firebase or Cloudflare.
import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve, relative } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { once } from "node:events";
import { WebSocket } from "ws";
import { blake3 } from "@noble/hashes/blake3.js";
import { build } from "esbuild";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { socialKeyProofPreimage } from "../src/connectSocialBinding.ts";
import { hostSocialPolicyPreimage } from "../src/connectSocialAdmission.ts";

const hex = value => Buffer.from(value).toString("hex");
const encode = value => Buffer.from(JSON.stringify(value)).toString("base64url");
const fields = value => Object.fromEntries(Object.entries(value).map(([key, item]) => [key,
  item === null ? { nullValue: null } : typeof item === "number" ? { integerValue: String(item) }
    : key === "expiresAt" ? { timestampValue: item } : { stringValue: item }]));

for (const policySchema of [2, 4, 5]) test(`real social Worker HTTP schema ${policySchema} verifies RSA callers and persists exact authority, mailbox and revocation in SQLite`, { timeout: 30_000 }, async () => {
  const root = resolve(tmpdir()), persistent = await mkdtemp(join(root, "aurora-social-miniflare-"));
  assert.ok(!relative(root, persistent).startsWith(".."));
  const rsa = await crypto.subtle.generateKey({ name: "RSASSA-PKCS1-v1_5", hash: "SHA-256", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]) }, true, ["sign", "verify"]);
  const publicJwk = await crypto.subtle.exportKey("jwk", rsa.publicKey);
  const session = crypto.randomUUID(), expiry = Date.now() + 300_000;
  let blocked = false, reads = 0, jwksReads = 0;
  const tokens = new Map();
  for (const uid of ["host", "guest", "stranger"]) {
    const now = Math.floor(Date.now() / 1000), unsigned = encode({ alg: "RS256", kid: "fixture-rsa" }) + "." + encode({
      aud: "auroralauncher", iss: "https://securetoken.google.com/auroralauncher", sub: uid, user_id: uid, iat: now, auth_time: now, exp: now + 3500,
    });
    tokens.set(uid, unsigned + "." + Buffer.from(await crypto.subtle.sign("RSASSA-PKCS1-v1_5", rsa.privateKey, new TextEncoder().encode(unsigned))).toString("base64url"));
  }
  const upstream = async request => {
    const url = new URL(request.url);
    if (url.href === "https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com") { jwksReads++; return Response.json({ keys: [{ ...publicJwk, kid: "fixture-rsa", alg: "RS256" }] }); }
    assert.equal(url.origin, "https://firestore.googleapis.com", "unexpected network target is forbidden");
    const uid = [...tokens].find(([, token]) => request.headers.get("authorization") === `Bearer ${token}`)?.[0];
    assert.ok(uid, "Firestore must receive the original verified caller credential"); reads++;
    const path = url.pathname.split("/projects/auroralauncher/databases/(default)/documents/")[1];
    if (path === `users/${uid}`) return Response.json({ fields: fields({ uid, usernameLower: `${uid}nick` }) });
    if (path === `usernames/${uid}nick`) return Response.json({ fields: fields({ uid }) });
    if (path === `socialSessions/${session}`) return blocked && uid === "guest" ? new Response("", { status: 403 }) : Response.json({ fields: fields({
      schema: 1, hostUid: "host", title: "Controlled online session", modpackName: "Pack", minecraftVersion: "1.21.1", loader: "Fabric",
      imageUrl: null, players: 1, maxPlayers: 9, visibility: "public", joinPolicy: "approval", expiresAt: new Date(expiry).toISOString(),
    }) });
    if (path === `socialJoinRequests/${session}/guests/${uid}`) return Response.json({ fields: fields({
      sessionId: session, hostUid: "host", guestUid: uid, status: uid === "guest" ? "accepted" : "pending", expiresAt: new Date(expiry).toISOString(),
    }) });
    assert.fail(`unexpected upstream path ${path}`);
  };
  const bundle = await build({ entryPoints: [fileURLToPath(new URL("../src/connectSocialWorker.ts", import.meta.url))], bundle: true,
    write: false, format: "esm", external: ["cloudflare:workers"] });
  const options = convertV4MiniflareOptions({ host: "127.0.0.1", port: 0, cf: false, compatibilityDate: "2026-09-02", modules: true,
    script: bundle.outputFiles[0].text, bindings: { FIREBASE_PROJECT_ID: "auroralauncher" }, outboundService: upstream,
    durableObjects: {
      SOCIAL_SESSIONS: { className: "SocialSessionAuthority", useSQLite: true }, SOCIAL_ACCOUNTS: { className: "SocialAccountBudget", useSQLite: true },
      RELAY_SESSIONS: { className: "ConnectRelaySession", useSQLite: true },
    }, resourcePersistencePath: persistent });
  let runtime;
  let address;
  const sockets = [];
  const start = async () => { runtime = new Miniflare({ ...options, logRequests: false }); address = await runtime.ready; };
  const event = (socket, name) => once(socket, name, { signal: AbortSignal.timeout(4000) });
  const relay = async (peer, ticket) => {
    const url = new URL("/relay", address); url.protocol = "ws:";
    const socket = new WebSocket(url, "iroh-relay-v2", { headers: { authorization: "Bearer " + JSON.stringify(ticket) } }); sockets.push(socket);
    const [challenge] = await event(socket, "message"); assert.equal(challenge.length, 17);
    const digest = blake3(challenge.subarray(1), { context: new TextEncoder().encode("iroh-relay handshake v1 challenge signature") });
    const signature = new Uint8Array(await crypto.subtle.sign("Ed25519", peer.keys.privateKey, digest));
    socket.send(Uint8Array.of(1, ...Buffer.from(peer.challenge.publicKey, "hex"), 64, ...signature));
    const [accepted] = await event(socket, "message"); assert.deepEqual([...accepted], [2]); return socket;
  };
  const request = async (uid, path, body = {}, extra = {}) => {
    const response = await runtime.dispatchFetch(`https://social.fixture.invalid/v1/social/${path}`, { method: "POST",
      headers: { "Content-Type": "application/json", "X-Aurora-Session": session, Authorization: `Bearer ${tokens.get(uid)}`, ...extra }, body: JSON.stringify(body) });
    return { status: response.status, body: await response.json() };
  };
  const good = async (...args) => { const response = await request(...args); assert.equal(response.status, 200, JSON.stringify({ ...response.body, jwksReads, reads })); return response.body; };
  const prepare = async (uid, role) => {
    const keys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]), publicKey = hex(await crypto.subtle.exportKey("raw", keys.publicKey));
    const challenge = await good(uid, "key/challenge", { schemaVersion: 1, operationId: crypto.randomUUID(), generation: 1, role, publicKey, sessionId: session, preparedExpiresAtUnixMs: Date.now() + 55_000 });
    return { keys, challenge, proof: { schemaVersion: 1, challenge, signature: hex(await crypto.subtle.sign("Ed25519", keys.privateKey, socialKeyProofPreimage(challenge))) } };
  };
  try {
    await start();
    assert.equal((await request("host", "bootstrap", {}, { Origin: "https://renderer.invalid" })).status, 403);
    assert.equal((await request("host", "bootstrap", {}, { Authorization: "Bearer unsigned.token.value" })).status, 401);
    assert.equal((await good("stranger", "presence")).state, "unavailable", "visible pending advert is observable before an authority exists");
    const clockRequest = { operationId: crypto.randomUUID(), generation: 7, nonce: "ab".repeat(16) };
    const bootstrap = await good("host", "bootstrap", clockRequest); assert.equal(bootstrap.accountId, "host"); assert.match(bootstrap.admissionPublicKey, /^[a-f0-9]{64}$/);
    const pin = await crypto.subtle.importKey("raw", Buffer.from(bootstrap.admissionPublicKey, "hex"), "Ed25519", false, ["verify"]);
    assert.equal(bootstrap.clock.body.nonce, clockRequest.nonce); assert.equal(bootstrap.clock.body.operationId, clockRequest.operationId);
    assert.equal(bootstrap.clock.body.generation, 7); assert.equal(bootstrap.clock.body.sessionId, session);
    assert.equal(bootstrap.clock.body.accountId, "host"); assert.equal(bootstrap.clock.body.sessionExpiresAtUnixMs, bootstrap.expiresAtUnixMs);
    assert.ok(bootstrap.clock.body.serverReceivedAtUnixMs <= bootstrap.clock.body.serverNowUnixMs);
    assert.equal(await crypto.subtle.verify("Ed25519", pin, Buffer.from(bootstrap.clock.signature, "hex"),
      new TextEncoder().encode("aurora/connect/social-clock/v1\0" + JSON.stringify(bootstrap.clock.body))), true);
    const host = await prepare("host", "host"), c = host.challenge;
    const body = { schemaVersion: policySchema, purpose: "hostSocialSession", issuerInstance: c.issuerInstance, policyId: c.challengeId, accountId: "host",
      operationId: c.operationId, generation: c.generation, publicKey: c.publicKey, sessionId: session, sessionDigest: c.sessionDigest,
      visibility: "public", joinPolicy: "approval", serviceId: "game", protocol: "tcp", sourceDigest: "a".repeat(64), maxGuests: 8,
      issuedAtUnixMs: c.issuedAtUnixMs, expiresAtUnixMs: expiry,
      ...(policySchema >= 4 ? { voiceProfile: "svc", voiceSourceDigest: "d".repeat(64), hostPlayer: false, runtimeProfile: "fabric-loader-0.16.14-1.21.1" } : {}),
      ...(policySchema === 5 ? { voiceTransportProfile: "nativeVoice8V1" } : {}) };
    const policyId = (await good("host", "host/register", { proof: host.proof, policySigned: { body, signature: hex(await crypto.subtle.sign("Ed25519", host.keys.privateKey, hostSocialPolicyPreimage(body))) } })).policyId;
    assert.equal((await good("stranger", "presence")).state, "unknown", "registered policy alone cannot invent native transport observations");
    assert.equal((await request("stranger", "context", { sessionId: session })).status, 403);
    const context = await good("guest", "context", { sessionId: session }); assert.equal(context.body.recipientAccountId, "guest");
    const guest = await prepare("guest", "guest"), joined = await good("guest", "join", { proof: guest.proof, policyId });
    const grantId = joined.grant.body.grantId; assert.equal(joined.grant.body.guestPublicKey, guest.challenge.publicKey);
    assert.equal(joined.grant.body.schemaVersion, policySchema);
    assert.equal(joined.grant.body.runtimeProfile, body.runtimeProfile);
    assert.equal(joined.grant.body.voiceTransportProfile, body.voiceTransportProfile);
    const { socialAdmissionPreimage } = await import("../src/connectSocialAdmission.ts");
    assert.equal(await crypto.subtle.verify("Ed25519", pin, Buffer.from(joined.grant.signature, "hex"), socialAdmissionPreimage(joined.grant.body)), true);
    const polled = await good("host", "host/poll", { policyId }); assert.equal(polled.deliveries.length, 1); assert.ok(polled.checkedAtUnixMs <= Date.now());
    await good("host", "host/publish", { grantId, invitation: "opaque-native-envelope-only" });
    assert.equal((await good("guest", "guest/receive", { grantId })).invitation, "opaque-native-envelope-only");
    assert.notEqual((await request("stranger", "guest/receive", { grantId })).status, 200);
    const renewed = await good("guest", "continuity", { grantId }); assert.equal(renewed.body.sequence, 2);
    await good("host", "host/poll", { policyId, transport: { observedAtUnixMs: Date.now(), connectedGrantIds: [grantId], hostPlayerConnected: false } });
    const presence = await good("stranger", "presence");
    assert.equal(presence.accountId, "stranger"); assert.equal(presence.state, "ready");
    assert.equal(presence.admittedGuests, 1); assert.equal(presence.connectedGuests, 1); assert.equal(presence.maxGuests, 8);
    assert.equal(presence.hostPlayerConnected, false); assert.ok(presence.expiresAtUnixMs - presence.checkedAtUnixMs <= 3000);
    assert.doesNotMatch(JSON.stringify(presence), /grantId|publicKey|policyId|fixture-rsa|\.eyJ/);
    assert.equal((await request("stranger", "context", { sessionId: session })).status, 403, "observing presence never approves entry");
    const ticket = await good("guest", "relay/ticket", { role: "guest", id: grantId });
    const actor = (await runtime.getDurableObjectNamespace("SOCIAL_SESSIONS")).getByName(session);
    assert.deepEqual((await actor.relayAuthorize(ticket)).allowedPeers, [c.publicKey]);
    const hostTicket = await good("host", "relay/ticket", { role: "host", id: policyId });
    const [hostSocket, guestSocket] = await Promise.all([relay(host, hostTicket), relay(guest, ticket)]);
    const forwarded = event(hostSocket, "message"); guestSocket.send(Uint8Array.of(4, ...Buffer.from(c.publicKey, "hex"), 0, 42));
    assert.deepEqual([...(await forwarded)[0]], [6, ...Buffer.from(guest.challenge.publicKey, "hex"), 0, 42]);
    const closed = sockets.map(socket => { const done = event(socket, "close"); socket.close(); return done; }); await Promise.all(closed);
    assert.equal((await request("guest", "host/revoke", { grantId })).status, 403);
    await good("host", "host/revoke", { grantId });
    await good("host", "host/revoke", { grantId });
    const departed = await good("stranger", "presence"); assert.equal(departed.admittedGuests, 0); assert.equal(departed.connectedGuests, 0);
    await assert.rejects(actor.relayAuthorize(ticket));
    const readBeforeRestart = reads;
    await runtime.dispose(); runtime = undefined; await start();
    const restored = await good("host", "bootstrap");
    assert.equal(restored.admissionPublicKey, bootstrap.admissionPublicKey); assert.equal(restored.issuerInstance, bootstrap.issuerInstance);
    assert.notEqual((await request("guest", "key/proof", guest.proof)).status, 200);
    assert.notEqual((await request("guest", "continuity", { grantId })).status, 200);
    assert.equal((await request("guest", "context", { sessionId: session })).status, 403);
    const newKey = await prepare("guest", "guest");
    assert.equal((await request("guest", "join", { proof: newKey.proof, policyId })).status, 403);
    assert.equal((await good("host", "host/poll", { policyId })).deliveries.length, 0);
    assert.ok(reads > readBeforeRestart);
    blocked = true; assert.notEqual((await request("guest", "bootstrap")).status, 200);
    assert.equal((await request("guest", "presence")).status, 403, "live Firestore block also closes display-only visibility");
    await good("host", "host/stop", { policyId });
    assert.equal((await good("stranger", "presence")).state, "unavailable");
  } finally {
    for (const socket of sockets) if (socket.readyState !== WebSocket.CLOSED) socket.terminate();
    await runtime?.dispose();
    assert.ok(resolve(persistent).startsWith(root + "/") || resolve(persistent).startsWith(root + "\\"));
    await rm(persistent, { recursive: true, force: true });
  }
});
