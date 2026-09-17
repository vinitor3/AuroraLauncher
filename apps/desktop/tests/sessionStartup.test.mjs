import assert from "node:assert/strict";
import { setImmediate, setTimeout as wait } from "node:timers/promises";
import test from "node:test";
import { guardSessionProfile, startSessionStartup } from "../src/lib/sessionStartup.ts";

const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};

test("late appearance update from another account cannot replace active profile", () => {
  const current = { uid: "new-account", avatar: "new" };
  assert.equal(guardSessionProfile(current, { uid: "old-account" }, "new-account"), current);
  assert.equal(guardSessionProfile(undefined, { uid: "old-account" }, undefined), undefined);
  const updated = { uid: "new-account", avatar: "changed" };
  assert.equal(guardSessionProfile(current, updated, "new-account"), updated);
  assert.equal(guardSessionProfile(current, undefined, undefined), undefined);
});
function setup(loadProfile = async (user) => ({ uid: user.uid }), timeoutMs = 1_000) {
  let current = null, next, error;
  let unsubscribed = false;
  const states = [];
  const controller = startSessionStartup({
    observe: (onNext, onError) => { next = onNext; error = onError; return () => { unsubscribed = true; }; },
    currentUser: () => current,
    loadProfile,
    onState: (state) => states.push(state),
    timeoutMs,
  });
  return { controller, states, error: () => error(), unsubscribed: () => unsubscribed,
    emit: (user) => { current = user; next(user); },
    setCurrent: (user) => { current = user; },
  };
}

test("restored session shows only loading until its profile is ready, never login", async () => {
  const profile = deferred();
  const run = setup(() => profile.promise);
  assert.deepEqual(run.states.map((s) => s.phase), ["loading"]);
  run.emit({ uid: "saved" });
  await setImmediate();
  assert.ok(run.states.every((s) => s.phase === "loading"));
  profile.resolve({ uid: "saved" });
  await setImmediate();
  assert.deepEqual(run.states.at(-1), { phase: "ready", profile: { uid: "saved" } });
  assert.ok(!run.states.some((s) => s.phase === "signed-out"));
  run.controller.dispose();
});

test("confirmed signed out promptly reaches login without a fake profile", () => {
  const run = setup(() => { throw new Error("profile should not be fetched"); });
  run.emit(null);
  assert.deepEqual(run.states.at(-1), { phase: "signed-out" });
  run.controller.dispose();
});

test("logout and account switch ignore stale profile resolutions", async () => {
  const first = deferred(), second = deferred();
  const run = setup((user) => user.uid === "first" ? first.promise : second.promise);
  run.emit({ uid: "first" });
  run.emit(null);
  first.resolve({ uid: "first" });
  await setImmediate();
  assert.equal(run.states.at(-1).phase, "signed-out");
  run.emit({ uid: "second" });
  second.resolve({ uid: "second" });
  await setImmediate();
  assert.equal(run.states.at(-1).profile.uid, "second");
  run.controller.dispose();
});

test("profile errors and auth observer errors offer recovery without opening launcher", async () => {
  const run = setup(async () => { throw new Error("private backend details"); });
  run.emit({ uid: "saved" });
  await setImmediate();
  assert.equal(run.states.at(-1).phase, "error");
  assert.ok(!run.states.at(-1).error.includes("private"));
  run.error();
  assert.equal(run.states.at(-1).phase, "error");
  run.controller.dispose();
});

test("timeout ends spinner and discards a late profile", async () => {
  const profile = deferred();
  const run = setup(() => profile.promise, 15);
  run.emit({ uid: "saved" });
  await wait(35);
  assert.equal(run.states.at(-1).phase, "error");
  profile.resolve({ uid: "saved" });
  await setImmediate();
  assert.equal(run.states.at(-1).phase, "error");
  run.controller.dispose();
});

test("no auth callback times out instead of exposing the login prematurely", async () => {
  const run = setup(undefined, 15);
  await wait(35);
  assert.deepEqual(run.states.map((s) => s.phase), ["loading", "error"]);
  run.controller.dispose();
});

test("signup waits until account profile creation completes before reading it", async () => {
  const registration = deferred();
  let loads = 0;
  const run = setup(async (user) => { ++loads; return { uid: user.uid }; });
  run.emit(null);
  const submitted = run.controller.authenticate(() => registration.promise);
  run.emit({ uid: "new" });
  await setImmediate();
  assert.equal(loads, 0);
  assert.equal(run.states.at(-1).phase, "signed-out");
  registration.resolve();
  await submitted;
  assert.equal(loads, 1);
  assert.equal(run.states.at(-1).profile.uid, "new");
  run.controller.dispose();
});

test("rejected credentials stay signed out and allow another attempt", async () => {
  const run = setup();
  run.emit(null);
  await assert.rejects(run.controller.authenticate(async () => { throw new Error("wrong password"); }));
  assert.equal(run.states.at(-1).phase, "signed-out");
  await run.controller.authenticate(async () => { run.emit({ uid: "valid" }); });
  assert.equal(run.states.at(-1).profile.uid, "valid");
  run.controller.dispose();
});

test("disposed StrictMode subscription cannot publish stale results", async () => {
  const profile = deferred();
  const run = setup(() => profile.promise);
  run.emit({ uid: "old" });
  run.controller.dispose();
  const count = run.states.length;
  profile.resolve({ uid: "old" });
  run.emit(null);
  run.error();
  await setImmediate();
  assert.equal(run.states.length, count);
  assert.ok(run.unsubscribed());
});

test("profile UID must match the currently authenticated user", async () => {
  const run = setup(async () => ({ uid: "someone-else" }));
  run.emit({ uid: "saved" });
  await setImmediate();
  assert.equal(run.states.at(-1).phase, "error");
  run.controller.dispose();
});

test("current-user change before auth callback still blocks stale profile", async () => {
  const profile = deferred();
  const run = setup(() => profile.promise);
  run.emit({ uid: "saved" });
  run.setCurrent(null);
  profile.resolve({ uid: "saved" });
  await setImmediate();
  assert.equal(run.states.at(-1).phase, "signed-out");
  run.controller.dispose();
});
