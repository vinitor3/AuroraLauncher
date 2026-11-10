// Synthetic DOM and event lifecycle only; no native process, account or network.
import assert from "node:assert/strict";
import { test, beforeEach, afterEach } from "node:test";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import ts from "typescript";
const require = createRequire(import.meta.url);
const { window, document } = require("linkedom").parseHTML("<html><body><div id='root'></div></body></html>");
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Synthetic progress lifecycle" } });
const code = ts.transpileModule(await readFile(new URL("./downloadProgress.ts", import.meta.url), "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
}).outputText;
const { observeDownloadProgress } = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
const { act, createElement, useEffect, useState } = await import("react");
const { createRoot } = await import("react-dom/client");
const container = document.getElementById("root");
const a = "aaaaaaaa-1111-4111-8111-111111111111", b = "bbbbbbbb-2222-4222-8222-222222222222";
const progress = (operationId, totalPercent = 98) => ({ operationId, label: "Fabric preparado", percent: totalPercent, totalPercent,
  itemPercent: 0, itemDownloadedBytes: 0, downloadedBytes: 0, completedFiles: 0, totalFiles: 0, activeDownloads: 0, bytesPerSecond: 0 });
const terminal = (operationId, outcome) => ({ schemaVersion: 1, operationId, outcome });
let root, callbacks, timers, nextTimer, stopped;
const clock = { schedule: callback => { timers.set(++nextTimer, callback); return nextTimer; }, cancel: id => timers.delete(id) };
const listen = async (name, callback) => { callbacks.set(name, callback); return () => { callbacks.delete(name); stopped.push(name); }; };
function Harness() {
  const [current, setCurrent] = useState();
  useEffect(() => observeDownloadProgress(listen, setCurrent, clock), []);
  return current ? createElement("aside", { "data-operation": current.operationId ?? "ordinary" }, `${current.label}: ${current.totalPercent}%`) : null;
}
const emit = async (name, payload) => { await act(async () => callbacks.get(name)?.({ payload })); };
const transfer = async value => emit("aurora-download-progress", value);
const finish = async value => emit("aurora-ready-copy-install-finished", value);
const runTimers = async () => { const pending = [...timers.values()]; timers.clear(); await act(async () => pending.forEach(callback => callback())); };
beforeEach(async () => { callbacks = new Map(); timers = new Map(); nextTimer = 0; stopped = []; root = createRoot(container); await act(async () => root.render(createElement(Harness))); });
afterEach(async () => { await act(async () => root.unmount()); });

test("failed or cancelled copy closes its98percent toast without fabricating success", async () => {
  for (const outcome of ["failed", "cancelled"]) {
    const id = outcome === "failed" ? a : b;
    await transfer(progress(id)); assert.match(container.textContent, /98%/);
    await finish(terminal(id, outcome)); assert.equal(container.textContent, "");
    await transfer(progress(id, 100)); assert.equal(container.textContent, "");
  }
});

test("completed terminal closes98percent even before or without final progress", async () => {
  await transfer(progress(a));
  await finish(terminal(a, "completed")); assert.equal(container.textContent, "");
  await transfer(progress(a, 100)); assert.equal(container.textContent, "");
  await finish(terminal(b, "completed"));
  await transfer(progress(b)); assert.equal(container.textContent, "");
});

test("old terminal and completion timer never clear another scoped or ordinary download", async () => {
  await transfer(progress(a, 100));
  await transfer(progress(b, 23));
  await finish(terminal(a, "failed"));
  await runTimers(); assert.match(container.textContent, /23%/);
  await transfer(progress(undefined, 42));
  await finish(terminal(b, "cancelled")); assert.match(container.textContent, /42%/);
  await transfer(progress(a, 98)); assert.match(container.textContent, /42%/);
  await transfer(progress(undefined, 100)); await runTimers(); assert.equal(container.textContent, "");
});

test("malformed terminal and progress cannot dismiss or replace the current toast", async () => {
  await transfer(progress(a));
  for (const value of [null, [], {}, { ...terminal(a, "failed"), schemaVersion: 2 }, terminal(a, "private error"), terminal("bad", "failed")]) {
    await finish(value); assert.match(container.textContent, /98%/);
  }
  for (const value of [null, [], {}, progress("bad"), { ...progress(b), totalPercent: NaN }]) {
    await transfer(value); assert.match(container.textContent, /98%/);
  }
});

test("unmount removes both subscriptions and pending timers", async () => {
  await transfer(progress(a, 100)); assert.equal(timers.size, 1);
  await act(async () => root.render(null));
  assert.equal(timers.size, 0); assert.equal(callbacks.size, 0);
  assert.deepEqual(stopped.sort(), ["aurora-download-progress", "aurora-ready-copy-install-finished"]);
});

test("late listener registration after disposal unsubscribes without state updates", async () => {
  const resolutions = []; let updates = 0, unlisten = 0;
  const stop = observeDownloadProgress((_name, callback) => new Promise(resolve => resolutions.push({ resolve, callback })), () => updates++, clock);
  stop(); stop();
  for (const item of resolutions) { item.callback({ payload: progress(a) }); item.resolve(() => unlisten++); }
  await Promise.resolve();
  assert.equal(updates, 0); assert.equal(unlisten, 2);
});
