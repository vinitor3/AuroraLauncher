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
const { socialConnectCapabilitiesFrom, socialRuntimeLabels } = await import(await moduleUrl(new URL("./connect-types.ts", import.meta.url)));
const session = "11111111-2222-4333-8444-555555555555", operation = "22222222-2222-4333-8444-555555555555";
const idle = () => ({ available: true, operationId: null, generation: 0, role: null, phase: "idle", transportState: "notStarted", statusState: "notChecked", tcpAddress: null, expiresAtMs: null, statusSummary: null, errorCode: null, worldState: "notVerified", simpleVoiceChatState: "disabled", plasmoVoiceState: "disabled", relayEnabled: false, pathState: "notVerified", preparation: null, review: null, cleanupState: "notStarted" });
const active = (role = "guest", stage = "authenticating") => ({ ...idle(), operationId: operation, generation: 1, role, phase: "starting", cleanupState: "pending", social: { sessionId: session, role, stage, digest: null, gameTarget: role === "host" ? "127.0.0.1:43001" : null, visibility: role === "host" ? "public" : null, expiresAtMs: Date.now() + 60_000 } });
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const tick = async () => { await new Promise(resolve => setImmediate(resolve)); };
const owners = [];
const readyVoice = () => ({ revision: 1, available: true, phase: "ready", profile: "simpleVoiceChat", review: null, operationId: null, hostPublicId: null, participants: [], udpState: "notStarted", tcpAddress: null, negotiatedDatagramBytes: null, recoveryPending: false, recoverableCopies: [], eulaState: "ownedBySource", errorCode: null, exportedInvitation: null, audioState: "notVerified" });
afterEach(() => { owners.splice(0).forEach(owner => owner.invalidate()); });
async function fixture(options = {}) {
  const calls = [], published = [], handlers = {};
  let snapshot = idle(), tokenCalls = 0;
  const user = { uid: "own-account", getIdToken: () => { tokenCalls++; calls.push(["SDK token"]); return options.token ?? Promise.resolve("LOCAL_TEST_TOKEN"); } };
  let currentUser = user;
  const controller = createSocialConnectController({ accountId: user.uid, user, currentUser: () => currentUser,
    publish: value => published.push(value), invoke: async (command, args) => {
      calls.push([command, args]); if (handlers[command]) return handlers[command](args);
      if (command === "connect_social_capabilities") return { available: options.available ?? true, experimental: true, accountEpoch: 7, maxSessionSeconds: options.online ? 14400 : 60, ...(options.discovery ? { ownedGameTcpDiscovery: true } : {}) };
      if (command === "native_voice_snapshot") return options.voice ?? { ...readyVoice(), phase: "idle" };
      if (command === "native_voice_social_reserve") { snapshot = active("host"); return { operationId: operation, generation: 1 }; }
      if (command === "connect_snapshot") return structuredClone(snapshot);
      if (command === "connect_social_reserve") { snapshot = active(args.request.role); return { operationId: operation, generation: 1 }; }
      if (command === "connect_social_reserve_discovered") { snapshot = active("host"); snapshot.social.gameTarget = null; return { operationId: operation, generation: 1 }; }
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

test("online native voice host uses the same reservation/authentication flow with explicit local-player choice", async () => {
  const f = await fixture({ online: true, voice: readyVoice() });
  assert.equal(f.published.at(-1).nativeVoiceReady, "simpleVoiceChat");
  await f.controller.prepareHostNativeVoice(session, "friends", true);
  assert.deepEqual(f.calls.find(([name]) => name === "native_voice_social_reserve")[1], {
    request: { accountEpoch: 7, accountId: "own-account", sessionId: session, visibility: "friends" }, hostPlayer: true,
  });
  const names = f.calls.map(([name]) => name);
  assert.ok(names.indexOf("native_voice_social_reserve") < names.indexOf("SDK token"));
  assert.equal(names.filter(name => name === "connect_social_authenticate").length, 1);
  assert.equal(names.filter(name => name === "connect_social_reserve").length, 0);
  assert.equal(names.filter(name => name === "connect_social_confirm_host").length, 0);
});

test("unready or legacy native voice never reserves or reads credentials", async () => {
  for (const options of [{ online: true }, { online: false, voice: readyVoice() }]) {
    const f = await fixture(options); await f.controller.prepareHostNativeVoice(session, "public", false);
    assert.equal(f.calls.filter(([name]) => name === "native_voice_social_reserve").length, 0); assert.equal(f.tokenCalls(), 0);
  }
});

test("native voice schema accepts only explicit signed projection fields and independent local voice state", () => {
  const value = active("host", "waitingGuest"); value.social.voiceProfile = "svc"; value.social.voiceState = "enabled";
  value.social.hostPlayer = true; value.social.hostPlayerVoiceState = "retired"; value.social.maxGuests = 7;
  assert.equal(snapshotFrom(value).social.hostPlayerVoiceState, "retired");
  for (const extra of [{ voiceProfile: "unknown" }, { voiceState: "reactivating" }, { hostPlayer: false }, { maxGuests: "7" }, { voiceSourceDigest: "a".repeat(64) }]) {
    assert.throws(() => snapshotFrom({ ...value, social: { ...value.social, ...extra } }));
  }
});

test("native runtime projection accepts the closed matrix and never implies unsupported legacy voice", () => {
  const value = active("host", "waitingGuest"); value.social.voiceProfile = "svc";
  assert.equal(Object.keys(socialRuntimeLabels).length, 9);
  for (const runtimeProfile of Object.keys(socialRuntimeLabels)) {
    assert.equal(snapshotFrom({ ...value, social: { ...value.social, runtimeProfile } }).social.runtimeProfile, runtimeProfile);
  }
  for (const runtimeProfile of [null, "fabric-loader-0.16.15-1.21.1", "minecraft-1.22", "__proto__"])
    assert.throws(() => snapshotFrom({ ...value, social: { ...value.social, runtimeProfile } }));
  assert.throws(() => snapshotFrom({ ...value, social: { ...value.social, runtimeProfile: "1.12.2-forge-14.23.5.2860", voiceProfile: "plasmo" } }));
  const plain = active(); plain.social.runtimeProfile = "fabric-loader-0.16.14-1.21.1";
  assert.throws(() => snapshotFrom(plain));
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
test("discovered host reserves only opaque selection before credentials and never sends a port", async () => {
  const f = await fixture({ discovery: true });
  await f.controller.prepareHostDiscovered(session, "friends", operation);
  assert.deepEqual(f.calls.find(([name]) => name === "connect_social_reserve_discovered")[1], {
    request: { accountEpoch: 7, accountId: "own-account", sessionId: session, visibility: "friends" }, selectionId: operation,
  });
  assert.ok(f.calls.findIndex(([name]) => name === "connect_social_reserve_discovered") < f.calls.findIndex(([name]) => name === "SDK token"));
  assert.equal(f.calls.filter(([name]) => name === "connect_social_confirm_host").length, 0);
  assert.equal(f.calls.filter(([name]) => name === "connect_social_reserve").length, 0);
});
test("older capability cannot invoke discovered reservation or read credentials", async () => {
  const f = await fixture(); await f.controller.prepareHostDiscovered(session, "public", operation);
  assert.equal(f.tokenCalls(), 0); assert.equal(f.calls.filter(([name]) => name === "connect_social_reserve_discovered").length, 0);
});
test("discovered late reservation after account invalidation is reaped without token handoff", async () => {
  const reply = deferred(), f = await fixture({ discovery: true }); f.handlers.connect_social_reserve_discovered = () => reply.promise;
  const work = f.controller.prepareHostDiscovered(session, "public", operation); await tick(); f.controller.invalidate();
  f.setSnapshot(active("host")); reply.resolve({ operationId: operation, generation: 1 }); await work;
  assert.equal(f.tokenCalls(), 0); assert.equal(f.calls.filter(([name]) => name === "connect_stop").length, 1);
});
test("derived source projections accept only exact IPv4 loopback and source-checking cannot claim a destination", () => {
  const value = active("host", "hostReview"); value.social.digest = "a".repeat(64); value.social.gameTarget = "127.0.0.1:51029";
  snapshotFrom(value);
  for (const gameTarget of ["127.0.0.2:51029", "[::1]:51029", "127.0.0.1:0", "127.0.0.1:65536", "http://127.0.0.1:1", "127.0.0.1:01"])
    assert.throws(() => snapshotFrom({ ...value, social: { ...value.social, gameTarget } }));
  assert.throws(() => snapshotFrom({ ...value, social: { ...value.social, stage: "sourceChecking" } }));
  snapshotFrom({ ...value, social: { ...value.social, stage: "sourceChecking", gameTarget: null, digest: null } });
});

test("social ready source flag requires an admitted guest and remains optional for older native builds", () => {
  const guest = { ...active("guest", "connected"), phase: "guestConnected", transportState: "authenticated", tcpAddress: "127.0.0.1:51401" };
  snapshotFrom(guest);
  const ready = { ...guest, social: { ...guest.social, readyGameLaunchAvailable: true } };
  assert.equal(snapshotFrom(ready).social.readyGameLaunchAvailable, true);
  for (const change of [{ phase: "starting" }, { transportState: "closed" }, { tcpAddress: null }, { cleanupState: "complete" }])
    assert.throws(() => snapshotFrom({ ...ready, ...change }));
  for (const change of [{ readyGameLaunchAvailable: false }, { readyGameLaunchAvailable: "true" }, { stage: "connecting" }, { role: "host" }, { extra: true }])
    assert.throws(() => snapshotFrom({ ...ready, social: { ...ready.social, ...change } }));
});

test("online host-player ReadyGame flag requires explicit native local player and a live authenticated bridge", () => {
  const value = { ...active("host", "waitingGuest"), phase: "hostListening", tcpAddress: "127.0.0.1:51401", transportState: "authenticated" };
  value.social = { ...value.social, voiceProfile: "svc", hostPlayer: true, readyGameLaunchAvailable: true };
  assert.equal(snapshotFrom(value).social.readyGameLaunchAvailable, true);
  assert.equal(snapshotFrom({ ...value, transportState: "admitted" }).social.readyGameLaunchAvailable, true);
  for (const extra of [{ phase: "starting" }, { transportState: "closed" }, { tcpAddress: null }, { cleanupState: "complete" }]) assert.throws(() => snapshotFrom({ ...value, ...extra }));
  assert.throws(() => snapshotFrom({ ...value, social: { ...value.social, hostPlayer: false } }));
});

test("host removal sends only current operation, account epoch and a listed opaque grant", async () => {
  const f = await fixture({ online: true }), grantId = "ab".repeat(32);
  const host = { ...active("host", "connected"), phase: "hostListening", transportState: "admitted" };
  host.social.participants = [{ grantId, accountId: "verifiedGuest", state: "connected" }];
  f.setSnapshot(host); await f.controller.refresh();
  f.handlers.connect_social_revoke_participant = () => host;
  await f.controller.revokeParticipant("cd".repeat(32));
  assert.equal(f.calls.filter(([name]) => name === "connect_social_revoke_participant").length, 0);
  await f.controller.revokeParticipant(grantId);
  assert.deepEqual(f.calls.find(([name]) => name === "connect_social_revoke_participant")[1], { operationId: operation, accountEpoch: 7, grantId });
  for (const participants of [[{ grantId, accountId: "email@example.test", state: "connected" }], [{ grantId, accountId: "guest", state: "worldJoined" }], [...host.social.participants, ...host.social.participants]]) {
    assert.throws(() => snapshotFrom({ ...host, social: { ...host.social, participants } }));
  }
});
