// Causal controller/DTO tests, no authentication or transport runtime claims.
import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import ts from "typescript";
const modules = new Map();
async function moduleUrl(url) {
  if (modules.has(url.href)) return modules.get(url.href);
  let output = ts.transpileModule(await readFile(url, "utf8"), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  for (const { fileName } of ts.preProcessFile(output).importedFiles) {
    const target = ["", ".ts"].map(ext => new URL(fileName + ext, url)).find(candidate => existsSync(fileURLToPath(candidate)));
    output = output.replaceAll(`from "${fileName}"`, `from ${JSON.stringify(await moduleUrl(target))}`);
  }
  const result = `data:text/javascript;base64,${Buffer.from(output).toString("base64")}`; modules.set(url.href, result); return result;
}
const { createSocialConnectController } = await import(await moduleUrl(new URL("./connect-controller.ts", import.meta.url)));
const { snapshotFrom } = await import(await moduleUrl(new URL("../connect/types.ts", import.meta.url)));
const { socialConnectCapabilitiesFrom } = await import(await moduleUrl(new URL("./connect-types.ts", import.meta.url)));
const session = "11111111-2222-4333-8444-555555555555", operation = "22222222-2222-4333-8444-555555555555";
const idle = () => ({ available: true, operationId: null, generation: 0, role: null, phase: "idle", transportState: "notStarted", statusState: "notChecked", tcpAddress: null, expiresAtMs: null, statusSummary: null, errorCode: null, worldState: "notVerified", simpleVoiceChatState: "disabled", plasmoVoiceState: "disabled", relayEnabled: false, pathState: "notVerified", preparation: null, review: null, cleanupState: "notStarted" });
const active = (role = "guest", stage = "authenticating") => ({ ...idle(), operationId: operation, generation: 1, role, phase: "starting", cleanupState: "pending", social: { sessionId: session, role, stage, digest: null, gameTarget: role === "host" ? "127.0.0.1:43001" : null, visibility: role === "host" ? "public" : null, expiresAtMs: Date.now() + 60_000 } });
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const tick = async () => { await new Promise(resolve => setImmediate(resolve)); };
const owners = [];
afterEach(() => { owners.splice(0).forEach(owner => owner.invalidate()); });
async function fixture(options = {}) {
  const calls = [], published = [], handlers = {};
  let snapshot = idle(), tokenCalls = 0;
  const user = { uid: "own-account", getIdToken: () => { tokenCalls++; calls.push(["SDK token"]); return options.token ?? Promise.resolve("LOCAL_TEST_TOKEN"); } };
  let currentUser = user;
  const controller = createSocialConnectController({ accountId: user.uid, user, currentUser: () => currentUser,
    publish: value => published.push(value), invoke: async (command, args) => {
      calls.push([command, args]); if (handlers[command]) return handlers[command](args);
      if (command === "connect_social_capabilities") return { available: options.available ?? true, experimental: true, accountEpoch: 7, maxSessionSeconds: 60 };
      if (command === "connect_snapshot") return structuredClone(snapshot);
      if (command === "connect_social_reserve") { snapshot = active(args.request.role); return { operationId: operation, generation: 1 }; }
      if (command === "connect_social_authenticate") return structuredClone(snapshot);
      if (command === "connect_stop") { snapshot = { ...snapshot, phase: "closed", transportState: "closed", cleanupState: "complete", social: snapshot.social && { ...snapshot.social, stage: "closed" } }; return structuredClone(snapshot); }
      if (command === "connect_social_confirm_host") { snapshot = { ...snapshot, phase: "hostListening", social: { ...snapshot.social, stage: "waitingGuest" } }; return structuredClone(snapshot); }
      throw new Error("Unexpected command");
    } });
  owners.push(controller); await controller.start();
  return { controller, calls, published, handlers, tokenCalls: () => tokenCalls, setUser: value => { currentUser = value; }, setSnapshot: value => { snapshot = value; } };
}
test("join reserves native slot before SDK token and sends no authority URL/pin/UID override in credential command", async () => {
  const f = await fixture(); await f.controller.join(session);
  const names = f.calls.map(([command]) => command);
  assert.ok(names.indexOf("connect_social_reserve") < names.indexOf("SDK token"));
  assert.deepEqual(f.calls.find(([command]) => command === "connect_social_reserve")[1], { request: { accountEpoch: 7, accountId: "own-account", sessionId: session, role: "guest" } });
  assert.deepEqual(f.calls.find(([command]) => command === "connect_social_authenticate")[1], { operationId: operation, generation: 1, idToken: "LOCAL_TEST_TOKEN" });
  assert.ok(f.published.at(-1).snapshot.social); assert.doesNotMatch(JSON.stringify(f.published), /LOCAL_TEST_TOKEN/);
});
test("double Join during delayed SDK token cannot create a second reservation", async () => {
  const token = deferred(), f = await fixture({ token: token.promise });
  const first = f.controller.join(session); await tick(); await f.controller.join(session);
  assert.equal(f.calls.filter(([command]) => command === "connect_social_reserve").length, 1);
  token.resolve("LOCAL_TEST_TOKEN"); await first;
});
test("account change while SDK token waits never sends that token to native", async () => {
  const token = deferred(), f = await fixture({ token: token.promise });
  const work = f.controller.join(session); await tick(); f.setUser({ uid: "another-account" }); token.resolve("SECRET_LATE_TOKEN"); await work;
  assert.equal(f.calls.filter(([command]) => command === "connect_social_authenticate").length, 0);
  assert.doesNotMatch(JSON.stringify(f.published), /SECRET_LATE_TOKEN/);
});
test("account invalidation during delayed reservation prevents SDK and authenticate after late reply", async () => {
  const reserved = deferred(), f = await fixture(); f.handlers.connect_social_reserve = () => reserved.promise;
  const work = f.controller.join(session); await tick(); f.controller.invalidate(); reserved.resolve({ operationId: operation, generation: 1 }); await work;
  assert.equal(f.tokenCalls(), 0); assert.equal(f.calls.filter(([name]) => name === "connect_social_authenticate").length, 0);
});
test("Disconnect cancels pending credential handoff and stops original native reservation", async () => {
  const token = deferred(), f = await fixture({ token: token.promise });
  const work = f.controller.join(session); await tick(); await f.controller.stop(); token.resolve("LATE"); await work;
  assert.equal(f.calls.filter(([command]) => command === "connect_social_authenticate").length, 0);
  assert.deepEqual(f.calls.find(([command]) => command === "connect_stop")[1], { operationId: operation });
  assert.equal(f.published.at(-1).snapshot.cleanupState, "complete");
});
test("reservation arriving after Disconnect observed idle is still reaped by exact operation", async () => {
  const reserved = deferred(), f = await fixture(); f.handlers.connect_social_reserve = () => reserved.promise;
  const work = f.controller.join(session); await tick(); await f.controller.stop();
  assert.equal(f.published.at(-1).snapshot.phase, "idle");
  f.setSnapshot(active()); reserved.resolve({ operationId: operation, generation: 1 }); await work;
  assert.deepEqual(f.calls.filter(([name]) => name === "connect_stop").map(([, args]) => args), [{ operationId: operation }]);
  assert.equal(f.tokenCalls(), 0);
});
test("token rejection closes the original reservation and never forwards credentials", async () => {
  const token = deferred(), f = await fixture({ token: token.promise }); const work = f.controller.join(session); await tick();
  assert.equal(f.published.at(-1).snapshot.operationId, operation, "reservation visible while token waits");
  token.reject(new Error("SECRET_SDK_FAILURE")); await work;
  assert.deepEqual(f.calls.filter(([name]) => name === "connect_stop").map(([, args]) => args), [{ operationId: operation }]);
  assert.equal(f.published.at(-1).snapshot.cleanupState, "complete"); assert.doesNotMatch(JSON.stringify(f.published), /SECRET_SDK_FAILURE/);
});
test("cleanup failure after SDK rejection is visible and does not claim released resources", async () => {
  const token = deferred(), f = await fixture({ token: token.promise }); const work = f.controller.join(session); await tick();
  f.handlers.connect_stop = () => ({ ...active(), phase: "failed", cleanupState: "failed" }); token.reject(new Error("sdk")); await work;
  assert.match(f.published.at(-1).error, /ainda não foi encerrada/); assert.notEqual(f.published.at(-1).snapshot.cleanupState, "complete");
});
test("host reserves fixed local source but never confirms policy until explicit action", async () => {
  const f = await fixture(); await f.controller.prepareHost(session, "public");
  assert.deepEqual(f.calls.find(([name]) => name === "connect_social_reserve")[1].request, { accountEpoch: 7, accountId: "own-account", sessionId: session, role: "host", gameTarget: "127.0.0.1:43001", visibility: "public" });
  assert.equal(f.calls.filter(([name]) => name === "connect_social_confirm_host").length, 0);
  const reviewed = active("host", "hostReview"); reviewed.phase = "reviewed"; reviewed.social.digest = "a".repeat(64);
  f.setSnapshot(reviewed); await f.controller.refresh(); await f.controller.confirmHost();
  assert.deepEqual(f.calls.find(([name]) => name === "connect_social_confirm_host")[1], { operationId: operation, generation: 1, digest: "a".repeat(64) });
});
test("expired host review does not confirm or reuse old consent", async () => {
  const f = await fixture(); const reviewed = active("host", "hostReview"); reviewed.social.digest = "a".repeat(64); reviewed.social.expiresAtMs = Date.now() - 1;
  f.setSnapshot(reviewed); await f.controller.refresh(); await f.controller.confirmHost();
  assert.equal(f.calls.filter(([name]) => name === "connect_social_confirm_host").length, 0);
  assert.match(f.published.at(-1).error, /prazo/);
});
test("default-off capabilities do not reserve or read token", async () => {
  const f = await fixture({ available: false }); await f.controller.join(session);
  assert.equal(f.tokenCalls(), 0); assert.equal(f.calls.filter(([name]) => name === "connect_social_reserve").length, 0);
});
test("raw native failures and token failures never surface private details", async () => {
  const f = await fixture({ token: Promise.resolve("value") }); f.handlers.connect_social_authenticate = () => { throw new Error("SECRET_TOKEN_PRIVATE_PATH"); };
  await f.controller.join(session); assert.doesNotMatch(JSON.stringify(f.published), /SECRET_TOKEN_PRIVATE_PATH/);
});
test("snapshot social branch rejects manual invitation fields and untrusted endpoints", () => {
  const value = active(); snapshotFrom(value);
  for (const invalid of [ { ...value, invitationProfile: "sessionGameTcp" }, { ...value, tcpAddress: "192.168.1.1:42010" }, { ...value, role: "host" }, { ...value, social: { ...value.social, privateKey: "bad" } }, { ...value, social: { ...value.social, digest: "a".repeat(64) } } ]) assert.throws(() => snapshotFrom(invalid));
});
test("capability schema rejects authority material and invalid epochs", () => {
  const value = { available: true, experimental: true, accountEpoch: 1, maxSessionSeconds: 60 }; socialConnectCapabilitiesFrom(value);
  assert.throws(() => socialConnectCapabilitiesFrom({ ...value, authorityPin: "a".repeat(64) }));
  assert.throws(() => socialConnectCapabilitiesFrom({ ...value, accountEpoch: -1 }));
});
