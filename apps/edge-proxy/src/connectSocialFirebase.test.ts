import assert from "node:assert/strict";
import { before, test, type TestContext } from "node:test";
import { createSocialFirebaseDependencies, socialAuthenticate, socialJson } from "./connectSocialFirebase.ts";
import { createSocialBindingAuthority, socialKeyProofPreimage, SocialBindingError } from "./connectSocialBinding.ts";
import { createSocialAdmissionAuthority, hostSocialPolicyPreimage } from "./connectSocialAdmission.ts";

let pair: CryptoKeyPair, publicJwk: JsonWebKey;
before(async () => {
  pair = await crypto.subtle.generateKey({ name: "RSASSA-PKCS1-v1_5", hash: "SHA-256", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]) }, true, ["sign", "verify"]);
  publicJwk = await crypto.subtle.exportKey("jwk", pair.publicKey);
});
const encoded = (value: unknown) => Buffer.from(JSON.stringify(value)).toString("base64url");
const denied = (code: string) => (error: unknown) => error instanceof SocialBindingError && error.code === code;
async function token(overrides: Record<string, unknown> = {}) {
  const now = Math.floor(Date.now() / 1000), header = encoded({ alg: "RS256", kid: "test-key" });
  const claims = encoded({ aud: "auroralauncher", iss: "https://securetoken.google.com/auroralauncher", sub: "guest", user_id: "guest", iat: now, auth_time: now, exp: now + 3500, ...overrides });
  const signature = await crypto.subtle.sign("RSASSA-PKCS1-v1_5", pair.privateKey, new TextEncoder().encode(`${header}.${claims}`));
  return `${header}.${claims}.${Buffer.from(signature).toString("base64url")}`;
}
const request = (value: string) => new Request("https://social.invalid/v1/social/bootstrap", { headers: { Authorization: `Bearer ${value}` } });
const firestoreFields = (value: Record<string, unknown>) => Object.fromEntries(Object.entries(value).map(([key, item]) => [key,
  item === null ? { nullValue: null } : typeof item === "number" ? { integerValue: String(item) } : key === "expiresAt" ? { timestampValue: item } : { stringValue: item }]));

function locatorFixture(t: TestContext, authenticatedUid = "host") {
  const sessionId = crypto.randomUUID(), calls: Array<{ path: string; token: string | null; signal: AbortSignal | null | undefined }> = [];
  const documents = new Map<string, Record<string, unknown> | number | null>([
    ["users/host", { uid: "host", usernameLower: "hostnick" }], ["usernames/hostnick", { uid: "host" }],
    [`socialSessions/${sessionId}`, { schema: 1, hostUid: "host", title: "World", modpackName: "Pack", minecraftVersion: "1.21.1", loader: "Fabric",
      imageUrl: null, players: 1, maxPlayers: 8, visibility: "public", joinPolicy: "open", expiresAt: new Date(Date.now() + 60_000).toISOString() }],
  ]);
  let hook: ((path: string, signal: AbortSignal | null | undefined) => Promise<void>) | undefined;
  t.mock.method(globalThis, "fetch", async (url: string | URL | Request, init: RequestInit) => {
    const prefix = "https://firestore.googleapis.com/v1/projects/auroralauncher/databases/(default)/documents/";
    assert.ok(String(url).startsWith(prefix)); assert.equal(init.redirect, "manual");
    const path = String(url).slice(prefix.length);
    calls.push({ path, token: new Headers(init.headers).get("authorization"), signal: init.signal });
    await hook?.(path, init.signal);
    const document = documents.get(path);
    if (document === undefined || typeof document === "number") return new Response(null, { status: document ?? 404 });
    return Response.json({ fields: document === null ? null : firestoreFields(document) });
  });
  const dependencies = createSocialFirebaseDependencies("auroralauncher", authenticatedUid, sessionId);
  const principal = (uid = "host", bearer = "fresh.token.signature") => ({ accountId: uid,
    request: new Request("https://social.invalid/v1/social/host/poll", { headers: { Authorization: `Bearer ${bearer}`, "x-aurora-verified-account": uid } }) });
  return { dependencies, documents, calls, sessionId, principal,
    read: (uid = "host", bearer = "fresh.token.signature", signal = new AbortController().signal) => dependencies.readSession(principal(uid, bearer), sessionId, signal),
    hold: (next?: typeof hook) => { hook = next; } };
}

test("online Firebase boundary verifies an actual RSA signature and rejects issuer/account/time/signature changes", async t => {
  t.mock.method(globalThis, "fetch", async (url: string | URL | Request) => {
    assert.equal(String(url), "https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com");
    return Response.json({ keys: [{ ...publicJwk, kid: "test-key", alg: "RS256" }] });
  });
  const good = await token(); assert.equal(await socialAuthenticate(request(good), "auroralauncher", AbortSignal.timeout(1000)), "guest");
  for (const bad of [await token({ aud: "other-project" }), await token({ iss: "https://attacker.invalid" }), await token({ user_id: "other" }), await token({ exp: 1 }), good.slice(0, -20) + "A".repeat(20)]) {
    await assert.rejects(socialAuthenticate(request(bad), "auroralauncher", AbortSignal.timeout(1000)), denied("unauthenticated"));
  }
});
test("unsigned emulator tokens and bearer-like malformed input cannot enter production", async () => {
  for (const value of [`${encoded({ alg: "none" })}.${encoded({ sub: "guest" })}.`, "not-a-jwt", "a.b.c"]) {
    await assert.rejects(socialAuthenticate(request(value), "auroralauncher", AbortSignal.timeout(1000)), denied("unauthenticated"));
  }
});
test("authorized Firestore projection uses only caller token and exact paths; approval is required afresh", async t => {
  const sessionId = crypto.randomUUID(), bearer = "caller.token.signature";
  let accepted = true, blocked = false;
  const fields = (value: Record<string, unknown>) => Object.fromEntries(Object.entries(value).map(([key, item]) => [key,
    item === null ? { nullValue: null } : typeof item === "number" ? { integerValue: String(item) } : key === "expiresAt" ? { timestampValue: item } : { stringValue: item }]));
  t.mock.method(globalThis, "fetch", async (url: string | URL | Request, init: RequestInit) => {
    assert.equal(init.redirect, "manual"); assert.equal(new Headers(init.headers).get("authorization"), `Bearer ${bearer}`);
    const prefix = "https://firestore.googleapis.com/v1/projects/auroralauncher/databases/(default)/documents/";
    assert.ok(String(url).startsWith(prefix)); const path = String(url).slice(prefix.length);
    if (path === "users/guest") return Response.json({ fields: fields({ uid: "guest", usernameLower: "guestnick" }) });
    if (path === "usernames/guestnick") return Response.json({ fields: fields({ uid: "guest" }) });
    if (path === `socialSessions/${sessionId}`) return blocked ? new Response("", { status: 403 }) : Response.json({ fields: fields({ schema: 1, hostUid: "host", title: "World", modpackName: "Pack", minecraftVersion: "1.21.1", loader: "Fabric", imageUrl: null, players: 1, maxPlayers: 8, visibility: "public", joinPolicy: "approval", expiresAt: new Date(Date.now() + 60_000).toISOString() }) });
    assert.equal(path, `socialJoinRequests/${sessionId}/guests/guest`);
    return Response.json({ fields: fields({ status: accepted ? "accepted" : "pending", hostUid: "host", guestUid: "guest", sessionId, expiresAt: new Date(Date.now() + 60_000).toISOString() }) });
  });
  const dependencies = createSocialFirebaseDependencies("auroralauncher", "guest", sessionId), principal = { accountId: "guest", request: request(bearer) };
  assert.equal((await dependencies.readSession(principal, sessionId, AbortSignal.timeout(1000))).joinPolicy, "approval");
  accepted = false; await assert.rejects(dependencies.readSession(principal, sessionId, AbortSignal.timeout(1000)), denied("session_not_authorized"));
  const advertisement = createSocialFirebaseDependencies("auroralauncher", "guest", sessionId, true);
  assert.equal((await advertisement.readSession(principal, sessionId, AbortSignal.timeout(1000))).joinPolicy, "approval", "visible advertisement observation is separate from admission");
  accepted = true; blocked = true; await assert.rejects(dependencies.readSession(principal, sessionId, AbortSignal.timeout(1000)), denied("session_not_authorized"));
  await assert.rejects(advertisement.readSession(principal, sessionId, AbortSignal.timeout(1000)), denied("session_not_authorized"), "visibility/block rules still apply to observations");
  await assert.rejects(dependencies.readSession({ ...principal, accountId: "host" }, sessionId, AbortSignal.timeout(1000)), denied("session_not_authorized"));
});
test("body reader applies streaming limit before JSON parsing", async () => {
  await assert.rejects(socialJson(new Response("x".repeat(8193)), 8192), denied("request_too_large"));
});

test("upstream redirects and quotas fail closed without following a credential-bearing destination", async t => {
  let calls = 0;
  t.mock.method(globalThis, "fetch", async (_url: unknown, init: RequestInit) => {
    calls++; assert.equal(init.redirect, "manual"); return new Response(null, { status: 302, headers: { location: "https://untrusted.invalid" } });
  });
  await assert.rejects(socialAuthenticate(request(await token()), "auroralauncher", AbortSignal.timeout(1000)), denied("backend_unavailable"));
  assert.equal(calls, 1);
  await assert.rejects(socialJson(new Response(null, { status: 429 })), denied("rate_limited"));
});

test("registration locator overlaps three fresh reads and retains no credential or authorization", async t => {
  const f = locatorFixture(t);
  await f.read("host", "initial.token.signature");
  f.calls.length = 0;
  const releases: Array<() => void> = [];
  f.hold(async () => new Promise<void>(resolve => releases.push(resolve)));
  const signal = new AbortController().signal;
  const pending = f.read("host", "renewed.token.signature", signal);
  assert.equal(releases.length, 3, "all documents start before any previous document completes");
  assert.deepEqual(f.calls.map(call => call.path).sort(), ["users/host", "usernames/hostnick", `socialSessions/${f.sessionId}`].sort());
  assert.ok(f.calls.every(call => call.token === "Bearer renewed.token.signature" && call.signal === signal));
  for (const release of releases) release();
  assert.equal((await pending).hostUid, "host");
  f.hold();
  f.documents.set(`socialSessions/${f.sessionId}`, 403);
  await assert.rejects(f.read(), denied("session_not_authorized"));
});

test("warm registration locator rejects changed UID, deleted or null current documents", async t => {
  const f = locatorFixture(t);
  await f.read();
  for (const [path, value, code] of [
    ["users/host", { uid: "different", usernameLower: "hostnick" }, "account_not_registered"],
    ["users/host", { uid: "host", usernameLower: null }, "account_not_registered"],
    ["usernames/hostnick", { uid: "different" }, "account_not_registered"],
    ["users/host", 404, "session_not_authorized"],
    ["usernames/hostnick", 404, "session_not_authorized"],
    ["usernames/hostnick", null, "backend_unavailable"],
    [`socialSessions/${f.sessionId}`, null, "backend_unavailable"],
  ] as const) {
    const prior = f.documents.get(path)!;
    f.documents.set(path, value);
    await assert.rejects(f.read(), denied(code));
    f.documents.set(path, prior);
  }
});

test("changed nickname requires its new registry and ignores no current registry failure", async t => {
  const f = locatorFixture(t);
  await f.read();
  f.documents.set("users/host", { uid: "host", usernameLower: "renamed" });
  f.documents.set("usernames/hostnick", 404);
  f.documents.set("usernames/renamed", { uid: "different" });
  await assert.rejects(f.read(), denied("account_not_registered"));
  f.documents.set("usernames/renamed", { uid: "host" });
  f.calls.length = 0;
  await f.read();
  assert.ok(f.calls.some(call => call.path === "usernames/renamed"));
  assert.ok(f.calls.some(call => call.path === "usernames/hostnick"), "old hint may be fetched but cannot authorize the new name");
  f.calls.length = 0;
  await f.read();
  assert.equal(f.calls.length, 3);
  assert.ok(!f.calls.some(call => call.path === "usernames/hostnick"));
  f.documents.set("usernames/renamed", 404);
  await assert.rejects(f.read(), denied("session_not_authorized"));
});

test("registration locator remains bounded and scoped to account and dependency lifetime", async t => {
  const f = locatorFixture(t, "");
  for (let index = 0; index < 129; index++) {
    f.documents.set(`users/account${index}`, { uid: `account${index}`, usernameLower: `nick${index}` });
    f.documents.set(`usernames/nick${index}`, { uid: `account${index}` });
    await f.read(`account${index}`);
  }
  for (const [uid, expected] of [["account0", 3], ["account128", 2]] as const) {
    const releases: Array<() => void> = [];
    f.hold(async () => new Promise<void>(resolve => releases.push(resolve)));
    const pending = f.read(uid);
    assert.equal(releases.length, expected, "only the first 128 successful account locators are retained");
    f.hold();
    for (const release of releases) release();
    await pending;
  }
  const fresh = createSocialFirebaseDependencies("auroralauncher", "account0", f.sessionId);
  const releases: Array<() => void> = [];
  f.hold(async () => new Promise<void>(resolve => releases.push(resolve)));
  const pending = fresh.readSession(f.principal("account0"), f.sessionId, new AbortController().signal);
  assert.equal(releases.length, 2, "a new dependency has no locator from the prior authority");
  f.hold(); for (const release of releases) release(); await pending;
});

test("cancelled parallel observation cannot publish a locator or authorize a session", async t => {
  const f = locatorFixture(t);
  await f.read();
  const abort = new AbortController();
  f.hold(async () => { abort.abort(); });
  await assert.rejects(f.read("host", "fresh.token.signature", abort.signal), denied("binding_cancelled"));
});

test("warm locators never replace fresh approval or accept missing and malformed approval expiry", async t => {
  const f = locatorFixture(t, "");
  const session = f.documents.get(`socialSessions/${f.sessionId}`) as Record<string, unknown>;
  session.joinPolicy = "approval";
  f.documents.set("users/guest", { uid: "guest", usernameLower: "guestnick" });
  f.documents.set("usernames/guestnick", { uid: "guest" });
  const path = `socialJoinRequests/${f.sessionId}/guests/guest`;
  const approved = { status: "accepted", hostUid: "host", guestUid: "guest", sessionId: f.sessionId, expiresAt: new Date(Date.now() + 60_000).toISOString() };
  f.documents.set(path, approved);
  await f.read("guest");
  for (const expiry of [null, "", "invalid", "1970-01-01T00:00:00Z"]) {
    f.documents.set(path, { ...approved, expiresAt: expiry });
    await assert.rejects(f.read("guest"), denied("session_not_authorized"));
  }
  const { expiresAt: _expiry, ...missing } = approved;
  f.documents.set(path, missing);
  await assert.rejects(f.read("guest"), denied("session_not_authorized"));
  f.documents.set(path, { ...approved, status: "rejected" });
  await assert.rejects(f.read("guest"), denied("session_not_authorized"));
  f.documents.set(path, 404);
  await assert.rejects(f.read("guest"), denied("session_not_authorized"));
});

test("parallel Firestore completion cannot renew or revive the original three-second Host deadline", async t => {
  const f = locatorFixture(t);
  let wall = Date.now(), mono = 1000;
  f.dependencies.clock = { wall: () => wall, monotonic: () => mono };
  const binding = createSocialBindingAuthority(f.dependencies);
  const admissionKeys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
  const authority = await createSocialAdmissionAuthority(binding, f.dependencies, { keys: admissionKeys, online: true });
  const keys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
  const hex = (value: ArrayBuffer) => Buffer.from(value).toString("hex");
  const challenge = await binding.challenge(f.principal().request, { schemaVersion: 1, operationId: crypto.randomUUID(), generation: 1,
    role: "host", publicKey: hex(await crypto.subtle.exportKey("raw", keys.publicKey)), sessionId: f.sessionId, preparedExpiresAtUnixMs: wall + 55_000 });
  const proof = { schemaVersion: 1, challenge, signature: hex(await crypto.subtle.sign("Ed25519", keys.privateKey, socialKeyProofPreimage(challenge))) };
  const body = { schemaVersion: 2, purpose: "hostSocialSession", issuerInstance: challenge.issuerInstance, policyId: challenge.challengeId,
    accountId: "host", operationId: challenge.operationId, generation: challenge.generation, publicKey: challenge.publicKey, sessionId: f.sessionId,
    sessionDigest: challenge.sessionDigest, visibility: "public", joinPolicy: "open", serviceId: "game", protocol: "tcp", sourceDigest: "a".repeat(64), maxGuests: 7,
    issuedAtUnixMs: challenge.issuedAtUnixMs, expiresAtUnixMs: wall + 50_000 };
  const policy = await authority.registerHost(f.principal().request, { proof, policySigned: { body,
    signature: hex(await crypto.subtle.sign("Ed25519", keys.privateKey, hostSocialPolicyPreimage(body))) } });
  const first = await authority.hostPoll(f.principal().request, { policyId: policy.policyId });
  assert.equal(first.state, "group"); if (first.state !== "group") assert.fail();
  assert.equal(first.checkedAtUnixMs, wall);
  wall += 1000; mono += 1000;
  f.calls.length = 0;
  f.hold(async path => { if (path === "users/host") { wall += 3000; mono += 3000; } });
  await assert.rejects(authority.hostPoll(f.principal().request, { policyId: policy.policyId }), denied("policy_expired"));
  assert.equal(f.calls.length, 3, "fresh parallel reads all completed; none can reanchor after the original deadline");
  f.hold(); f.calls.length = 0;
  await assert.rejects(authority.hostPoll(f.principal().request, { policyId: policy.policyId }), denied("policy_expired"));
  assert.equal(f.calls.length, 0, "terminal Host policy cannot be revived by another read");
});
