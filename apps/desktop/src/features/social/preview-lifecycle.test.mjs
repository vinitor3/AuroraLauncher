// Causal preview ownership tests. Actual React roots in a synthetic DOM;
// controlled cleanup promises, no Firebase SDK or network operations.
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import ts from "typescript";

const require = createRequire(import.meta.url);
const { window, document } = require("linkedom").parseHTML("<html><body><div id='root'></div></body></html>");
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Preview lifecycle synthetic DOM" } });
const { act, createElement, useLayoutEffect } = await import("react");
const { createRoot } = await import("react-dom/client");
const source = ts.transpileModule(await readFile(new URL("./preview-lifecycle.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
}).outputText;
const { createPreviewOwner } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const tick = async () => { await Promise.resolve(); await Promise.resolve(); };

test("hot successor cannot create a second React root until old root unmount and SDK cleanup settle", async () => {
  const container = document.getElementById("root"), cleanup = deferred(), events = [], warnings = [];
  const originalError = console.error;
  console.error = (...args) => { warnings.push(args.map(String).join(" ")); };
  function Fixture({ register, name, finish }) {
    useLayoutEffect(() => { register(finish); return () => events.push(`${name}:effect-detached`); }, [register]);
    return createElement("button", null, `Entrar ${name}`);
  }
  const mount = (name, finish) => register => {
    events.push(`${name}:root-created`); const root = createRoot(container);
    root.render(createElement(Fixture, { register, name, finish }));
    return { unmount() { events.push(`${name}:unmount`); root.unmount(); } };
  };
  let first, second, stopped;
  try {
    await act(async () => { first = createPreviewOwner({ mount: mount("first", () => { events.push("first:sdk-cleanup"); return cleanup.promise; }), blocked: () => assert.fail("unexpected blocked") }); await first.ready; });
    assert.equal(container.textContent, "Entrar first");
    await act(async () => { stopped = first.dispose(); second = createPreviewOwner({ previous: stopped, mount: mount("second", async () => { events.push("second:sdk-cleanup"); }), blocked: () => assert.fail("unexpected blocked") }); await tick(); });
    assert.equal(container.textContent, "");
    assert.equal(events.includes("second:root-created"), false);
    assert.deepEqual(events.slice(0, 4), ["first:root-created", "first:sdk-cleanup", "first:unmount", "first:effect-detached"]);
    await act(async () => { cleanup.resolve(); await second.ready; });
    assert.equal(container.textContent, "Entrar second");
    assert.equal(events.filter(event => event === "first:sdk-cleanup").length, 1);
    assert.equal(warnings.some(warning => /createRoot.*already|already.*createRoot/i.test(warning)), false);
    assert.deepEqual(warnings, []);
    await act(async () => { await second.dispose(); });
  } finally {
    cleanup.resolve();
    await act(async () => { await Promise.allSettled([first?.dispose(), second?.dispose()]); });
    console.error = originalError;
  }
});

test("cleanup failure poisons successors rather than reopening SDK or clearing the failed barrier", async () => {
  let firstCleanup = 0, firstUnmount = 0, blocked = 0, successorMounts = 0;
  const failure = new Error("controlled-cleanup-failure");
  const first = createPreviewOwner({ mount(register) { register(() => { firstCleanup++; return Promise.reject(failure); }); return { unmount() { firstUnmount++; } }; }, blocked: () => assert.fail("first has no predecessor") });
  await first.ready;
  const stopped = first.dispose(); await assert.rejects(stopped, error => error === failure);
  const second = createPreviewOwner({ previous: stopped, mount() { successorMounts++; return { unmount() {} }; }, blocked() { blocked++; } });
  await assert.rejects(second.ready, error => error === failure);
  const third = createPreviewOwner({ previous: second.dispose(), mount() { successorMounts++; return { unmount() {} }; }, blocked() { blocked++; } });
  await assert.rejects(third.ready, error => error === failure);
  assert.equal(successorMounts, 0); assert.equal(firstCleanup, 1); assert.equal(firstUnmount, 1); assert.equal(blocked, 2);
  await assert.rejects(third.dispose(), error => error === failure);
});

test("a superseded generation waiting for old cleanup never mounts after that cleanup finishes", async () => {
  const previous = deferred(), events = [];
  const skipped = createPreviewOwner({ previous: previous.promise, mount() { events.push("obsolete-mount"); return { unmount() {} }; }, blocked() { events.push("obsolete-blocked"); } });
  const skippedCleanup = skipped.dispose();
  const next = createPreviewOwner({ previous: skippedCleanup, mount() { events.push("current-mount"); return { unmount() { events.push("current-unmount"); } }; }, blocked: () => assert.fail("unexpected blocked") });
  await tick(); assert.deepEqual(events, []);
  previous.resolve(); await next.ready;
  assert.deepEqual(events, ["current-mount"]);
  await next.dispose(); assert.deepEqual(events, ["current-mount", "current-unmount"]);
});

test("repeated dispose returns the original promise and performs cleanup/unmount once", async () => {
  const cleanup = deferred(); let calls = 0, unmounts = 0;
  const owner = createPreviewOwner({ mount(register) { register(() => { calls++; return cleanup.promise; }); return { unmount() { unmounts++; } }; }, blocked: () => assert.fail("unexpected blocked") });
  await owner.ready;
  const first = owner.dispose(), second = owner.dispose();
  assert.equal(first, second); assert.equal(calls, 1); assert.equal(unmounts, 1);
  cleanup.resolve(); await first; assert.equal(owner.dispose(), first);
});

test("synchronous cleanup failure still detaches the view and blocks the next owner", async () => {
  let unmounts = 0, blocked = 0;
  const owner = createPreviewOwner({ mount(register) { register(() => { throw new Error("synchronous failure"); }); return { unmount() { unmounts++; } }; }, blocked: () => assert.fail("unexpected blocked") });
  await owner.ready; const stopped = owner.dispose(); await assert.rejects(stopped, /synchronous failure/);
  const next = createPreviewOwner({ previous: stopped, mount: () => assert.fail("must not mount"), blocked() { blocked++; } });
  await assert.rejects(next.ready, /synchronous failure/); assert.equal(unmounts, 1); assert.equal(blocked, 1);
  await assert.rejects(next.dispose());
});

test("unmount failure is not reported as a successful release even if SDK cleanup succeeds", async () => {
  let cleanup = 0;
  const owner = createPreviewOwner({ mount(register) { register(async () => { cleanup++; }); return { unmount() { throw new Error("controlled unmount failure"); } }; }, blocked: () => assert.fail("unexpected blocked") });
  await owner.ready; await assert.rejects(owner.dispose(), /controlled unmount failure/); assert.equal(cleanup, 1);
});
