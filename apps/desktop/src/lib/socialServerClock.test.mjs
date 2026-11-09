import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { readFile } from "node:fs/promises";
import ts from "typescript";
const source = ts.transpileModule(await readFile(new URL("./socialServerClock.ts", import.meta.url), "utf8"), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { createSocialServerClock } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
const originalFetch = globalThis.fetch, originalPerformance = globalThis.performance, originalDate = Date.now;
const serverTime = 1_800_000_000_000;
const token = (uid = "original", project = "auroralauncher") => `${Buffer.from('{"alg":"RS256"}').toString("base64url")}.${Buffer.from(JSON.stringify({ sub: uid, aud: project, iss: `https://securetoken.google.com/${project}` })).toString("base64url")}.signature`;
let mono, calls, listeners, services, response;
const check = () => {};
beforeEach(() => {
  mono = 1000; calls = []; listeners = new Set();
  Object.defineProperty(globalThis, "performance", { configurable: true, value: { now: () => mono } });
  const user = { uid: "original", getIdToken: async () => token() };
  services = { config: { projectId: "auroralauncher" }, authScope: { id: "test", persistent: false }, auth: { currentUser: user, onAuthStateChanged: fn => { listeners.add(fn); return () => listeners.delete(fn); } } };
  response = () => [{ found: { name: "projects/auroralauncher/databases/(default)/documents/users/original", fields: { uid: { stringValue: "original" } } }, readTime: new Date(serverTime).toISOString() }];
  globalThis.fetch = async (url, options) => { calls.push({ url, options }); mono += 200; return new Response(JSON.stringify(response())); };
});
afterEach(() => { globalThis.fetch = originalFetch; Object.defineProperty(globalThis, "performance", { configurable: true, value: originalPerformance }); Date.now = originalDate; });

test("Firestore readTime supplies bounded lower/upper times independently of local year", async () => {
  Date.now = () => serverTime + 365 * 86_400_000;
  const clock = createSocialServerClock(services, false);
  assert.equal(Number.isNaN(clock.now()), true);
  await clock.ensure(check);
  assert.equal(clock.writeTime(), serverTime - 1);
  assert.equal(clock.now(), serverTime + 201);
  mono += 1000; Date.now = () => 0;
  assert.equal(clock.writeTime(), serverTime + 999);
  assert.equal(clock.now(), serverTime + 1201);
  assert.equal(calls[0].url, "https://firestore.googleapis.com/v1/projects/auroralauncher/databases/(default)/documents:batchGet");
  assert.deepEqual(JSON.parse(calls[0].options.body), { documents: ["projects/auroralauncher/databases/(default)/documents/users/original"], mask: { fieldPaths: ["uid"] } });
  assert.equal(calls[0].options.redirect, "error"); assert.equal(calls[0].options.credentials, "omit");
  assert.equal(listeners.size, 0);
});

test("one account sample is deduplicated, refreshed once after30s and never used after60s", async () => {
  const clock = createSocialServerClock(services, false);
  await Promise.all([clock.ensure(check), clock.ensure(check), clock.ensure(check)]);
  assert.equal(calls.length, 1);
  mono += 29_000; await clock.ensure(check); assert.equal(calls.length, 1);
  mono += 1000; await clock.ensure(check); assert.equal(calls.length, 2);
  mono += 60_000; assert.equal(Number.isNaN(clock.now()), true); assert.throws(() => clock.writeTime());
});

test("wrong Firebase project or unrelated provider token never reaches the network", async () => {
  services.config.projectId = "other"; assert.throws(() => createSocialServerClock(services, false));
  services.config.projectId = "auroralauncher";
  for (const bad of [token("other"), token("original", "microsoft"), "not-a-token"]) {
    services.auth.currentUser.getIdToken = async () => bad;
    await assert.rejects(createSocialServerClock(services, false).ensure(check), { code: "social_clock_unavailable" });
  }
  assert.equal(calls.length, 0);
});

test("account replacement while token waits never forwards its delayed token", async () => {
  let release;
  services.auth.currentUser.getIdToken = () => new Promise(resolve => { release = resolve; });
  const clock = createSocialServerClock(services, false), pending = clock.ensure(check);
  services.auth.currentUser = { uid: "original", getIdToken: async () => token() };
  for (const listener of listeners) listener(services.auth.currentUser);
  release(token());
  await assert.rejects(pending, { code: "social_clock_unavailable" });
  assert.equal(calls.length, 0); assert.equal(Number.isNaN(clock.now()), true);
});

test("late response cannot populate a new account or scope", async () => {
  let release;
  globalThis.fetch = async (_url, options) => { calls.push(options); return new Promise(resolve => { release = resolve; }); };
  const clock = createSocialServerClock(services, false), pending = clock.ensure(check);
  await new Promise(resolve => setImmediate(resolve));
  services.authScope = { id: "replacement", persistent: false };
  release(new Response(JSON.stringify(response())));
  await assert.rejects(pending, { code: "social_clock_unavailable" });
  assert.equal(Number.isNaN(clock.now()), true);
});

test("wrong document, missing result, unbounded payload and invalid time cannot seed a sample", async () => {
  const original = response;
  for (const invalid of [[], [{ missing: "anything", readTime: new Date(serverTime).toISOString() }],
    [{ ...original()[0], found: { name: "projects/other/databases/(default)/documents/users/original" } }],
    [{ ...original()[0], readTime: "NaN" }], [{ ...original()[0], transaction: "unrequested" }],
    [{ ...original()[0], found: { ...original()[0].found, extra: "x".repeat(8192) } }]]) {
    response = () => invalid;
    const clock = createSocialServerClock(services, false);
    await assert.rejects(clock.ensure(check), { code: "social_clock_unavailable" });
    assert.equal(Number.isNaN(clock.now()), true);
  }
});

test("emulator destination requires explicit local build and matching demo project", async () => {
  assert.throws(() => createSocialServerClock(services, true));
  services.config.projectId = "demo-aurora-social";
  services.auth.currentUser.getIdToken = async () => token("original", "demo-aurora-social");
  response = () => [{ found: { name: "projects/demo-aurora-social/databases/(default)/documents/users/original" }, readTime: new Date(serverTime).toISOString() }];
  await createSocialServerClock(services, true).ensure(check);
  assert.match(calls[0].url, /^http:\/\/127\.0\.0\.1:8187\//);
  assert.throws(() => createSocialServerClock(services, false));
});

test("an abandoned caller cannot continue into its write after the shared sample returns", async () => {
  let abandoned = false, release;
  globalThis.fetch = async () => new Promise(resolve => { release = resolve; });
  const clock = createSocialServerClock(services, false);
  const pending = clock.ensure(() => { if (abandoned) throw new Error("abandoned"); });
  await new Promise(resolve => setImmediate(resolve)); abandoned = true;
  release(new Response(JSON.stringify(response())));
  await assert.rejects(pending, /abandoned/);
});

test("an expired cached token gets one forced refresh under the original account and timeout", async () => {
  const refreshes = [];
  services.auth.currentUser.getIdToken = async force => { refreshes.push(force === true); return token(); };
  globalThis.fetch = async (_url, options) => { calls.push(options); mono += 100; return calls.length === 1
    ? new Response("expired", { status: 401 }) : new Response(JSON.stringify(response())); };
  const clock = createSocialServerClock(services, false);
  await clock.ensure(check);
  assert.deepEqual(refreshes, [false, true]); assert.equal(calls.length, 2);
  assert.equal(clock.now(), serverTime + 101);
  globalThis.fetch = async () => { calls.push({}); return new Response("expired", { status: 401 }); };
  calls.length = 0;
  await assert.rejects(createSocialServerClock(services, false).ensure(check), { code: "social_clock_unavailable" });
  assert.equal(calls.length, 2);
});
