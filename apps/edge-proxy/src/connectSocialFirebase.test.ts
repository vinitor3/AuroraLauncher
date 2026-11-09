import assert from "node:assert/strict";
import { before, test } from "node:test";
import { createSocialFirebaseDependencies, socialAuthenticate, socialJson } from "./connectSocialFirebase.ts";
import { SocialBindingError } from "./connectSocialBinding.ts";

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
