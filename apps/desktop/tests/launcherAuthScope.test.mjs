import assert from "node:assert/strict";
import test from "node:test";
import { setImmediate } from "node:timers/promises";
import { prepareLauncherServices, validateLauncherAuthScope } from "../src/lib/launcherAuthScope.ts";

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const primary = { id: "primary", persistent: true };
const secondary = { id: "d5ebf9bc-a7d7-4459-b5ef-bf71f8e93613", persistent: false };

test("authentication starts only after native ownership is known, regardless of configuration order", async () => {
  const ownership = deferred();
  const calls = [];
  const running = prepareLauncherServices({ loadScope: () => ownership.promise,
    loadConfig: async () => ({ projectId: "saved" }), bundledConfig: { projectId: "bundled" },
    create: (config, scope) => { calls.push({ config, scope }); return "ready"; } });
  await setImmediate();
  assert.deepEqual(calls, [], "Never initialize the shared account before receiving a scope");
  ownership.resolve(secondary);
  assert.equal(await running, "ready");
  assert.deepEqual(calls, [{ config: { projectId: "saved" }, scope: secondary }]);
});

test("a failed configuration read retains the existing bundled fallback and the isolated scope", async () => {
  const result = await prepareLauncherServices({ loadScope: async () => secondary,
    loadConfig: async () => { throw new Error("disk unavailable"); }, bundledConfig: { projectId: "bundled" },
    create: (config, scope) => ({ config, scope }) });
  assert.deepEqual(result, { config: { projectId: "bundled" }, scope: secondary });
});

test("configuration timeout is bounded and a late saved config cannot replace an initialized account", async () => {
  const config = deferred(), calls = [];
  const running = prepareLauncherServices({ loadScope: async () => primary, loadConfig: () => config.promise,
    bundledConfig: { projectId: "bundled" }, timeoutMs: 15,
    create: (value, scope) => { calls.push({ value, scope }); return value; } });
  assert.deepEqual(await running, { projectId: "bundled" });
  config.resolve({ projectId: "too-late" });
  await setImmediate();
  assert.deepEqual(calls, [{ value: { projectId: "bundled" }, scope: primary }]);
});

test("native scope failure and timeout never fall back to the remembered global account", async () => {
  for (const loadScope of [async () => { throw new Error("lock failure"); }, () => new Promise(() => {})]) {
    let initialized = false;
    await assert.rejects(prepareLauncherServices({ loadScope, loadConfig: async () => null,
      bundledConfig: {}, timeoutMs: 15, create: () => { initialized = true; } }));
    assert.equal(initialized, false);
  }
});

test("malformed public scopes cannot silently select persistent authentication", async () => {
  for (const value of [null, {}, { id: "primary" }, { id: "x", persistent: "true" },
    { id: "", persistent: true }, { id: "../other", persistent: false }, { id: "x".repeat(129), persistent: false }]) {
    assert.throws(() => validateLauncherAuthScope(value));
  }
  assert.deepEqual(validateLauncherAuthScope({ ...secondary, ignored: "not-copied" }), secondary);
});

test("each simultaneous startup passes its own scope without altering the other window", async () => {
  const one = deferred(), two = deferred();
  const create = (config, scope) => ({ config, scope });
  const first = prepareLauncherServices({ loadScope: () => one.promise, loadConfig: async () => null, bundledConfig: {}, create });
  const second = prepareLauncherServices({ loadScope: () => two.promise, loadConfig: async () => null, bundledConfig: {}, create });
  two.resolve(secondary);
  assert.deepEqual((await second).scope, secondary);
  one.resolve(primary);
  assert.deepEqual((await first).scope, primary);
});
