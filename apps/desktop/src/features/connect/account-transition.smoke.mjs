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

test("cold WebView ready waits for retained native operation without Java or voice commands", async () => {
  const cleanup = deferred(), calls = [];
  const f = fixture(() => stopConnectForAccountTransition(async (command, args) => {
    calls.push([command, args]);
    if (command === "connect_snapshot") return active();
    assert.equal(command, "connect_stop");
    return cleanup.promise;
  }));
  f.controller.accept(ready("NEW_ACCOUNT"));
  const waiting = f.controller.retry();
  await tick();
  assert.deepEqual(f.published, [{ phase: "loading" }]);
  assert.deepEqual(calls, [["connect_snapshot", undefined], ["connect_stop", { operationId: "synthetic" }]]);
  cleanup.resolve(stopped()); await waiting;
  assert.deepEqual(f.published.at(-1), ready("NEW_ACCOUNT"));
  assert.deepEqual(f.events.slice(-2), [["publish", ready("NEW_ACCOUNT")], ["barrier", "idle"]]);
});

test("cold signed-out state also crosses one default-off idle observation", async () => {
  const calls = [];
  const f = fixture(() => stopConnectForAccountTransition(async command => { calls.push(command); return idle(); }));
  f.controller.accept({ phase: "signed-out" });
  assert.deepEqual(f.published, [{ phase: "loading" }]);
  await f.controller.retry();
  assert.deepEqual(f.published.at(-1), { phase: "signed-out" });
  f.controller.accept({ phase: "loading" }); f.controller.accept(ready("A"));
  await tick();
  assert.deepEqual(calls, ["connect_snapshot"]);
  assert.deepEqual(f.published.at(-1), ready("A"));
});

test("default-off idle snapshot needs no capability or stop request", async () => {
  const calls = [];
  await stopConnectForAccountTransition(async command => { calls.push(command); return idle(); });
  assert.deepEqual(calls, ["connect_snapshot"]);
});

test("existing operation is stopped even when availability is false", async () => {
  const calls = [];
  await stopConnectForAccountTransition(async (command, args) => {
    calls.push([command, args]);
    return command === "connect_snapshot" ? { ...active(), available: false } : stopped();
  });
  assert.deepEqual(calls, [["connect_snapshot", undefined], ["connect_stop", { operationId: "synthetic" }]]);
});

test("snapshot rejection or malformed native output cannot bypass cleanup", async () => {
  for (const invalid of [null, {}, { ...idle(), cleanupState: "pending" }, { ...idle(), phase: "failed", cleanupState: "complete" }]) {
    await assert.rejects(stopConnectForAccountTransition(async () => invalid));
  }
  await assert.rejects(stopConnectForAccountTransition(async () => { throw new Error("fixture"); }));
});

test("stop must prove matching operation, non-regressing generation and terminal complete cleanup", async () => {
  for (const invalid of [
    { ...stopped(), cleanupState: "pending" }, { ...stopped(), cleanupState: "failed" },
    { ...stopped(), operationId: "other" }, { ...stopped(), operationId: null },
    { ...stopped(), generation: 3 }, { ...stopped(), phase: "guestConnected" },
  ]) {
    await assert.rejects(stopConnectForAccountTransition(async command => command === "connect_snapshot" ? active() : invalid));
  }
  await stopConnectForAccountTransition(async command => command === "connect_snapshot" ? active() : { ...stopped(), phase: "failed" });
});

test("lost stop response keeps account blocked until an explicit retry observes completed cleanup", async () => {
  let lost = true, snapshots = 0, stops = 0;
  const f = fixture(() => stopConnectForAccountTransition(async command => {
    if (command === "connect_snapshot") { snapshots++; return lost ? active() : stopped(); }
    assert.equal(command, "connect_stop"); stops++;
    if (lost) { lost = false; throw new Error("lost response"); }
    return stopped();
  }));
  f.controller.accept(ready("A")); f.controller.accept(ready("B")); await f.controller.retry();
  assert.equal(f.published.at(-1).phase, "loading");
  assert.equal(f.barriers.at(-1), "blocked");
  await f.controller.retry();
  assert.deepEqual(f.published.at(-1), ready("B"));
  assert.equal(snapshots, 2); assert.equal(stops, 2);
});
