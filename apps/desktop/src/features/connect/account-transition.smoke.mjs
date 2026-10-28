// Pure state-machine and native DTO fixtures. No Firebase account, native process,
// transport, clock, personal file or voice restoration state is accessed.
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const modules = new Map();
async function sourceModule(source) {
  if (modules.has(source.href)) return modules.get(source.href);
  let output = ts.transpileModule(await readFile(source, "utf8"), { fileName: fileURLToPath(source), compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext,
  } }).outputText;
  for (const { fileName: specifier } of ts.preProcessFile(output).importedFiles) {
    const dependency = ["", ".ts"].map(ext => new URL(specifier + ext, source)).find(url => existsSync(fileURLToPath(url)));
    assert.ok(dependency, `Local dependency required: ${specifier}`);
    output = output.replaceAll(`from "${specifier}"`, `from ${JSON.stringify(await sourceModule(dependency))}`);
  }
  const url = `data:text/javascript;base64,${Buffer.from(output).toString("base64")}`;
  modules.set(source.href, url);
  return url;
}
const { createAccountTransition, stopConnectForAccountTransition } = await import(await sourceModule(new URL("./account-transition.ts", import.meta.url)));
const ready = uid => ({ phase: "ready", profile: { uid, name: "fixture" } });
const tick = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function fixture(cleanup) {
  const events = [], published = [], barriers = [];
  let calls = 0;
  const controller = createAccountTransition({
    publish: state => { events.push(["publish", state]); published.push(state); },
    onBarrier: state => { events.push(["barrier", state]); barriers.push(state); },
    closeConnect: () => { calls++; return cleanup(); },
  });
  return { controller, events, published, barriers, calls: () => calls, checkpoint() {
    calls = 0; events.length = 0; barriers.length = 0;
    published.splice(0, Math.max(0, published.length - 1));
  } };
}

async function establishedFixture(cleanup) {
  let initial = true;
  const f = fixture(() => {
    if (initial) { initial = false; return Promise.resolve(); }
    return cleanup();
  });
  f.controller.accept(ready("A")); await f.controller.retry();
  assert.deepEqual(f.published.at(-1), ready("A"));
  f.checkpoint();
  return f;
}

test("first auth event closes inherited native work once and coalesces until cleanup", async () => {
  const cleanup = deferred(), f = fixture(() => cleanup.promise);
  const states = [{ phase: "loading" }, { phase: "error", error: "fixture" }, { phase: "signed-out" }, ready("A")];
  states.forEach(f.controller.accept);
  const waiting = f.controller.retry();
  await tick();
  assert.deepEqual(f.published, [{ phase: "loading" }]);
  assert.deepEqual(f.barriers, ["closing"]);
  assert.equal(f.calls(), 1);
  cleanup.resolve(); await waiting;
  assert.deepEqual(f.published, [{ phase: "loading" }, ready("A")]);
  assert.deepEqual(f.barriers, ["closing", "idle"]);
  f.controller.accept(ready("A")); await tick();
  assert.equal(f.calls(), 1);
});

test("initial cleanup failure blocks even first ready until explicit retry", async () => {
  const cleanup = deferred();
  let fail = true;
  const f = fixture(() => fail ? Promise.reject(new Error("private")) : cleanup.promise);
  f.controller.accept(ready("A")); await f.controller.retry();
  assert.deepEqual(f.published, [{ phase: "loading" }]);
  assert.equal(f.barriers.at(-1), "blocked");
  f.controller.accept({ phase: "signed-out" }); f.controller.accept(ready("B")); await tick();
  assert.equal(f.calls(), 1);
  fail = false;
  const waiting = f.controller.retry();
  cleanup.resolve(); await waiting;
  assert.deepEqual(f.published.at(-1), ready("B"));
  assert.equal(f.calls(), 2);
});

test("same-account ready update preserves context without an auth invalidation", async () => {
  const f = await establishedFixture(async () => assert.fail("same account cleanup"));
  f.controller.accept({ ...ready("A"), profile: { uid: "A", name: "new appearance" } });
  await tick();
  assert.equal(f.published.at(-1).profile.name, "new appearance");
  assert.deepEqual(f.barriers, []);
});

test("direct account change hides the previous profile until owned cleanup completes", async () => {
  const cleanup = deferred(), f = await establishedFixture(() => cleanup.promise);
  f.controller.accept(ready("B"));
  assert.deepEqual(f.published, [ready("A"), { phase: "loading" }]);
  assert.deepEqual(f.barriers, ["closing"]);
  const waiting = f.controller.retry();
  await tick();
  assert.equal(f.calls(), 1);
  assert.equal(f.published.at(-1).phase, "loading");
  cleanup.resolve(); await waiting;
  assert.deepEqual(f.events.slice(-2), [["publish", ready("B")], ["barrier", "idle"]]);
});

test("loading invalidates even the same UID and coalesces subsequent auth events", async () => {
  const cleanup = deferred(), f = await establishedFixture(() => cleanup.promise);
  f.controller.accept({ phase: "loading" });
  f.controller.accept(ready("A"));
  f.controller.accept({ phase: "signed-out" });
  f.controller.accept(ready("B"));
  f.controller.accept(ready("C"));
  const waiting = f.controller.retry();
  cleanup.resolve(); await waiting;
  assert.deepEqual(f.published, [ready("A"), { phase: "loading" }, ready("C")]);
  assert.equal(f.calls(), 1);
});

test("error and signed-out each invalidate an established account", async () => {
  for (const next of [{ phase: "signed-out" }, { phase: "error", error: "safe auth failure" }]) {
    const cleanup = deferred(), f = await establishedFixture(() => cleanup.promise);
    f.controller.accept(next);
    const waiting = f.controller.retry();
    assert.equal(f.published.at(-1).phase, "loading");
    cleanup.resolve(); await waiting;
    assert.deepEqual(f.published.at(-1), next);
    assert.equal(f.calls(), 1);
  }
});

test("cleanup failure blocks future auth results until manual retry and exposes no native error", async () => {
  const first = deferred(), second = deferred();
  let attempt = 0;
  const f = await establishedFixture(() => (++attempt === 1 ? first.promise : second.promise));
  f.controller.accept(ready("B"));
  const waiting = f.controller.retry();
  first.reject(new Error("SECRET_NATIVE_DETAIL")); await waiting;
  assert.deepEqual(f.barriers, ["closing", "blocked"]);
  f.controller.accept({ phase: "loading" }); f.controller.accept(ready("C"));
  await tick();
  assert.equal(f.calls(), 1);
  assert.equal(f.published.at(-1).phase, "loading");
  const retried = f.controller.retry();
  assert.equal(retried, f.controller.retry());
  second.resolve(); await retried;
  assert.deepEqual(f.published.at(-1), ready("C"));
  assert.deepEqual(f.barriers, ["closing", "blocked", "closing", "idle"]);
  assert.doesNotMatch(JSON.stringify(f.events), /SECRET_NATIVE_DETAIL/);
});

test("an auth observer retry cannot detach or duplicate the ongoing cleanup", async () => {
  const cleanup = deferred(), f = await establishedFixture(() => cleanup.promise);
  f.controller.accept({ phase: "loading" });
  const waiting = f.controller.retry();
  // Replacement observer emits its startup/loading state and final ready.
  f.controller.accept({ phase: "loading" }); f.controller.accept(ready("B"));
  assert.equal(f.controller.retry(), waiting);
  cleanup.resolve(); await waiting;
  assert.equal(f.calls(), 1);
  assert.deepEqual(f.published.at(-1), ready("B"));
});

test("each newly published account requires its own later cleanup", async () => {
  const f = await establishedFixture(async () => {});
  f.controller.accept(ready("B")); await f.controller.retry();
  f.controller.accept(ready("C")); await f.controller.retry();
  assert.equal(f.calls(), 2);
  assert.deepEqual(f.published.at(-1), ready("C"));
});

const idle = () => ({ available: false, operationId: null, generation: 0, role: null, phase: "idle", transportState: "notStarted", statusState: "notChecked",
  tcpAddress: null, expiresAtMs: null, statusSummary: null, errorCode: null, worldState: "notVerified", simpleVoiceChatState: "disabled",
  plasmoVoiceState: "disabled", relayEnabled: false, pathState: "notVerified", preparation: null, review: null, cleanupState: "notStarted" });
const active = () => ({ ...idle(), available: true, operationId: "synthetic", generation: 4, role: "guest", phase: "guestConnected", transportState: "authenticated", cleanupState: "pending" });
const stopped = () => ({ ...active(), generation: 5, phase: "closed", transportState: "closed", cleanupState: "complete" });

test("cold WebView ready waits for native epoch fence and cleanup without Java or voice commands", async () => {
  const cleanup = deferred(), calls = [];
  const f = fixture(() => stopConnectForAccountTransition(async (command, args) => { calls.push([command, args]); return cleanup.promise; }));
  f.controller.accept(ready("NEW_ACCOUNT")); const waiting = f.controller.retry(); await tick();
  assert.deepEqual(f.published, [{ phase: "loading" }]);
  assert.deepEqual(calls, [["connect_account_transition_stop", undefined]]);
  cleanup.resolve(stopped()); await waiting;
  assert.deepEqual(f.published.at(-1), ready("NEW_ACCOUNT"));
});
test("cold signed-out and default-off idle still invalidate native account epoch once", async () => {
  const calls = [];
  const f = fixture(() => stopConnectForAccountTransition(async command => { calls.push(command); return idle(); }));
  f.controller.accept({ phase: "signed-out" }); await f.controller.retry();
  f.controller.accept({ phase: "loading" }); f.controller.accept(ready("A")); await tick();
  assert.deepEqual(calls, ["connect_account_transition_stop"]); assert.deepEqual(f.published.at(-1), ready("A"));
});
test("cleanup response is enforced even if capability availability is false", async () => {
  const calls = [];
  await stopConnectForAccountTransition(async command => { calls.push(command); return { ...stopped(), available: false }; });
  assert.deepEqual(calls, ["connect_account_transition_stop"]);
});
test("epoch fence rejection, malformed or active response cannot bypass cleanup", async () => {
  for (const invalid of [null, {}, active(), { ...idle(), cleanupState: "pending" }, { ...idle(), phase: "failed", cleanupState: "complete" }, { ...stopped(), cleanupState: "failed" }, { ...stopped(), cleanupState: "pending" }, { ...stopped(), operationId: null }]) {
    await assert.rejects(stopConnectForAccountTransition(async () => invalid));
  }
  await assert.rejects(stopConnectForAccountTransition(async () => { throw new Error("fixture"); }));
  await stopConnectForAccountTransition(async () => ({ ...stopped(), phase: "failed" }));
});
test("lost epoch-fence response blocks account until explicit retry proves completed cleanup", async () => {
  let lost = true, calls = 0;
  const f = fixture(() => stopConnectForAccountTransition(async command => {
    assert.equal(command, "connect_account_transition_stop"); calls++;
    if (lost) { lost = false; throw new Error("lost response"); }
    return stopped();
  }));
  f.controller.accept(ready("A")); f.controller.accept(ready("B")); await f.controller.retry();
  assert.equal(f.published.at(-1).phase, "loading"); assert.equal(f.barriers.at(-1), "blocked");
  await f.controller.retry(); assert.deepEqual(f.published.at(-1), ready("B")); assert.equal(calls, 2);
});
for (const profile of ["legacy", "sessionGameTcp", "sessionFragmentedGameUdp"]) {
  test("account transition waits for retained " + profile + " source/review cleanup under epoch fence", async () => {
    const cleanup = deferred(), calls = [];
    const f = await establishedFixture(() => stopConnectForAccountTransition(async command => { calls.push(command); return cleanup.promise; }));
    f.controller.accept(ready("B")); const waiting = f.controller.retry(); await tick();
    assert.deepEqual(f.published.at(-1), { phase: "loading" }); assert.deepEqual(calls, ["connect_account_transition_stop"]);
    cleanup.resolve(stopped()); await waiting; assert.deepEqual(f.published.at(-1), ready("B"));
  });
}
test("social cleanup uses the same account fence and only publishes after worker release", async () => {
  const cleanup = deferred();
  const f = await establishedFixture(() => stopConnectForAccountTransition(async command => { assert.equal(command, "connect_account_transition_stop"); return cleanup.promise; }));
  f.controller.accept(ready("B")); const waiting = f.controller.retry(); await tick();
  assert.deepEqual(f.published.at(-1), { phase: "loading" });
  cleanup.resolve({ ...stopped(), social: { sessionId: "11111111-2222-4333-8444-555555555555", role: "guest", stage: "closed", digest: null, gameTarget: null, visibility: null, expiresAtMs: null } });
  await waiting; assert.deepEqual(f.published.at(-1), ready("B"));
});
