import assert from "node:assert/strict";
import { setImmediate } from "node:timers/promises";
import test from "node:test";
import { ExternalDownloadQueue } from "../src/lib/externalDownloadQueue.ts";

const request = (requestId, overrides = {}) => ({
  requestId,
  id: "instance",
  pageUrl: "https://www.curseforge.com/minecraft/mc-mods/example/download/1",
  filename: "example.jar",
  contentType: "mod",
  sha1: "a".repeat(40),
  ...overrides,
});
const event = (input, status, overrides = {}) => ({
  requestId: input.requestId,
  instanceId: input.id,
  filename: input.filename,
  contentType: input.contentType,
  status,
  ...overrides,
});

test("opens one page at a time and moves automatically to the next item", async () => {
  const opened = [];
  const queue = new ExternalDownloadQueue({
    open: async (input) => { opened.push(input.requestId); },
    cancel: async () => {},
  });
  const first = request("first");
  const second = request("second", { filename: "other.jar" });
  const firstDone = queue.enqueue(first);
  const secondDone = queue.enqueue(second);
  await setImmediate();
  assert.deepEqual(opened, ["first"]);
  assert.equal(queue.accept(event(second, "completed")), false);
  assert.equal(queue.accept(event(first, "resolving")), true);
  assert.equal(queue.accept(event(first, "downloading")), true);
  assert.equal(queue.accept(event(first, "completed")), true);
  assert.equal(queue.accept(event(first, "failed")), false);
  await firstDone;
  await setImmediate();
  assert.deepEqual(opened, ["first", "second"]);
  queue.accept(event(second, "completed"));
  await secondDone;
});

test("deduplicates instance/filename independently of correlation IDs", async () => {
  const queue = new ExternalDownloadQueue({ open: async () => {}, cancel: async () => {} });
  const first = request("first");
  const firstDone = queue.enqueue(first);
  await assert.rejects(queue.enqueue(request("duplicate")), /já está na fila/);
  const otherInstance = request("other-instance", { id: "other" });
  const otherDone = queue.enqueue(otherInstance);
  await setImmediate();
  queue.accept(event(first, "completed"));
  await firstDone;
  await setImmediate();
  queue.accept(event(otherInstance, "completed"));
  await otherDone;
});

test("ignores stale events from a previous attempt at the same file", async () => {
  const queue = new ExternalDownloadQueue({ open: async () => {}, cancel: async () => {} });
  const previous = request("previous");
  const previousDone = queue.enqueue(previous);
  await setImmediate();
  queue.accept(event(previous, "completed"));
  await previousDone;
  const current = request("current");
  const currentDone = queue.enqueue(current);
  await setImmediate();
  assert.equal(queue.accept(event(previous, "completed")), false);
  assert.equal(queue.accept(event(previous, "failed")), false);
  assert.equal(queue.accept(event(previous, "downloading")), false);
  assert.equal(queue.accept(event(current, "completed", { instanceId: "wrong" })), false);
  assert.equal(queue.accept(event(current, "completed", { filename: "wrong.jar" })), false);
  assert.equal(queue.accept(event(current, "completed", { contentType: "shader" })), false);
  assert.equal(queue.accept(event(current, "completed")), true);
  await currentDone;
});

test("timeout cancels a pending open and waits for closure before advancing", async () => {
  const opened = [];
  const cancelled = [];
  let closePage;
  const pageClosed = new Promise((resolve) => { closePage = resolve; });
  const queue = new ExternalDownloadQueue({
    open: (input) => {
      opened.push(input.requestId);
      return input.requestId === "first" ? new Promise(() => {}) : Promise.resolve();
    },
    cancel: async (requestId) => { cancelled.push(requestId); await pageClosed; },
    timeoutMs: 15,
  });
  const first = request("first");
  const second = request("second", { filename: "other.jar" });
  const firstDone = assert.rejects(queue.enqueue(first), /10 minutos/);
  const secondDone = queue.enqueue(second);
  await new Promise((resolve) => setTimeout(resolve, 35));
  assert.deepEqual(cancelled, ["first"]);
  assert.deepEqual(opened, ["first"]);
  assert.equal(queue.accept(event(first, "completed")), false);
  closePage();
  await firstDone;
  await setImmediate();
  assert.deepEqual(opened, ["first", "second"]);
  queue.accept(event(second, "completed"));
  await secondDone;
});

test("failure events reject promptly while open is pending and observe a later rejection", async () => {
  const cancelled = [];
  let rejectOpen;
  const queue = new ExternalDownloadQueue({
    open: () => new Promise((_resolve, reject) => { rejectOpen = reject; }),
    cancel: async (requestId) => { cancelled.push(requestId); },
  });
  const input = request("pending");
  const finished = assert.rejects(queue.enqueue(input), /fechada/);
  await setImmediate();
  queue.accept(event(input, "failed", { error: "página fechada" }));
  await finished;
  assert.deepEqual(cancelled, ["pending"]);
  rejectOpen(new Error("late open failure"));
  await setImmediate();
});

test("unmount cancels the active page once and never opens queued pages", async () => {
  const opened = [];
  const cancelled = [];
  const queue = new ExternalDownloadQueue({
    open: async (input) => { opened.push(input.requestId); },
    cancel: async (requestId) => { cancelled.push(requestId); },
  });
  const first = request("first");
  const firstDone = assert.rejects(queue.enqueue(first), /interrompido/);
  const secondDone = assert.rejects(queue.enqueue(request("second", { filename: "other.jar" })), /interrompido/);
  await setImmediate();
  await queue.cancelAll();
  await Promise.all([firstDone, secondDone]);
  assert.deepEqual(opened, ["first"]);
  assert.deepEqual(cancelled, ["first"]);
  assert.equal(queue.accept(event(first, "completed")), false);
});

test("invoke failure releases duplicate protection and allows a fresh retry", async () => {
  const cancelled = [];
  let attempt = 0;
  const queue = new ExternalDownloadQueue({
    open: async () => { if (attempt++ === 0) throw new Error("abertura falhou"); },
    cancel: async (requestId) => { cancelled.push(requestId); },
  });
  await assert.rejects(queue.enqueue(request("first")), /abertura falhou/);
  assert.deepEqual(cancelled, ["first"]);
  const retry = request("retry");
  const retryDone = queue.enqueue(retry);
  await setImmediate();
  queue.accept(event(retry, "completed"));
  await retryDone;
});
