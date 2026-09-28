// Synthetic transport fixtures. No Firebase account, native action or Gemini key.
import assert from "node:assert/strict";
import test, { afterEach } from "node:test";
import { requestAiToolAdvice } from "./edge.ts";
import { AI_ERRORS, publicAiError } from "../../../../shared/aiErrors.ts";

const originalFetch = globalThis.fetch;
afterEach(() => { globalThis.fetch = originalFetch; });
const context = {
  schemaVersion: 1, contextId: "private-context", truncated: false,
  instanceId: "private-instance", fileContents: "private-file-content",
  mods: [
    { id: "mod-1", name: "Synthetic Mod", version: "1.0", enabled: true, managed: false, path: "C:/Users/private-user/mod.jar" },
    { id: "mod-2", name: "Aurora Core", version: "1.1.5", enabled: true, managed: true },
  ],
  settings: [{ id: "setting-1", label: "Distância de renderização", value: 12, kind: "number", min: 2, max: 32 }],
  history: [{ id: "private-history", kind: "action", createdAtMs: 1, title: "Alteração anterior", summary: "O erro persistiu.",
    issueIds: ["mixin_failure"], actionId: "private-action", status: "applied", assessment: "unresolved", undoAvailable: true }],
};
const report = {
  schemaVersion: 1, status: "issues", instanceId: "private-instance",
  instance: { minecraftVersion: "1.20.1", loader: "Fabric", javaMajor: 17, requiredJavaMajor: 17, maxMemoryMb: 4096 },
  lastRun: { outcome: "failed", runId: "private-run" },
  issues: [{ id: "mixin_failure", confidence: "medium", kind: "hypothesis", explanation: "private-explanation", evidence: [{ summary: "private-log" }] }],
  logs: [{ association: "last_run", bytesRead: 20, truncated: false, source: "private-source" }],
};
const advice = { summary: "Uma alteração pode ser revisada.", model: "gemini-3.8-flash",
  proposals: [{ kind: "set_setting", targetId: "setting-1", value: 8, reason: "Reduzir a carga gráfica pode ajudar." }] };
const services = (getIdToken = async () => "synthetic-session") => ({
  config: { workerUrl: "https://aurora.test/" }, auth: { currentUser: { uid: "synthetic-owner", getIdToken } },
});
const request = (session = services(), signal = new AbortController().signal) =>
  requestAiToolAdvice(session, context, "  Ajude a investigar  ", signal, report);

test("tool transport sends only explicit mod/configuration/history and reduced Doctor fields", async () => {
  let calls = 0;
  globalThis.fetch = async (url, init) => {
    calls++;
    assert.equal(url, "https://aurora.test/v1/ai/tools");
    assert.equal(init.headers.Authorization, "Bearer synthetic-session");
    assert.ok(init.signal instanceof AbortSignal);
    const body = JSON.parse(init.body);
    assert.equal(body.question, "Ajude a investigar");
    assert.deepEqual(Object.keys(body).sort(), ["context", "doctor", "question", "schemaVersion"]);
    assert.deepEqual(Object.keys(body.context).sort(), ["history", "mods", "settings", "truncated"]);
    assert.deepEqual(Object.keys(body.context.mods[0]).sort(), ["enabled", "id", "managed", "name", "version"]);
    assert.equal(body.context.mods[0].name, "Synthetic Mod");
    assert.equal(body.context.history[0].assessment, "unresolved");
    assert.deepEqual(body.doctor.issues, [{ id: "mixin_failure", confidence: "medium", kind: "hypothesis" }]);
    assert.doesNotMatch(init.body, /private-|fileContents|contextId|runId|actionId|createdAtMs|undoAvailable/);
    return Response.json(advice);
  };
  assert.deepEqual(await request(), advice);
  assert.equal(calls, 1, "A provider suggestion makes one HTTP call and never invokes native actions");
});

test("invented targets, managed mods, invalid values and malformed replies are rejected", async () => {
  const before = JSON.stringify(context);
  const invalid = [
    { ...advice, proposals: [{ kind: "remove_mod", targetId: "unknown", reason: "Revisar" }] },
    { ...advice, proposals: [{ kind: "remove_mod", targetId: "mod-2", reason: "Revisar" }] },
    { ...advice, proposals: [{ ...advice.proposals[0], value: 64 }] },
    { ...advice, proposals: [{ ...advice.proposals[0], command: "arbitrary-command" }] },
    { ...advice, summary: "" }, null,
  ];
  for (const value of invalid) {
    globalThis.fetch = async () => Response.json(value);
    await assert.rejects(request(), { code: "invalid_response" });
  }
  globalThis.fetch = async () => new Response("invalid json", { headers: { "Content-Type": "application/json" } });
  await assert.rejects(request(), { code: "invalid_response" });
  globalThis.fetch = async () => new Response(JSON.stringify(advice), { headers: { "Content-Type": "text/plain" } });
  await assert.rejects(request(), { code: "invalid_response" });
  assert.equal(JSON.stringify(context), before);
});

test("known public failures stay actionable and provider prose never becomes UI text", async () => {
  for (const code of Object.keys(AI_ERRORS)) {
    globalThis.fetch = async () => Response.json({ code, error: "token=private-upstream C:/Users/private-user" }, { status: 503 });
    await assert.rejects(request(), (error) => error.code === code && publicAiError(error) === AI_ERRORS[code]);
  }
  for (const code of ["unknown", "toString", "__proto__"]) {
    globalThis.fetch = async () => Response.json({ code, error: "private-upstream" }, { status: 503 });
    await assert.rejects(request(), (error) => error.code === "provider_unavailable" && !publicAiError(error).includes("private"));
  }
});

test("cancelling token acquisition or switching accounts prevents late tool requests", async () => {
  let resolveToken;
  let calls = 0;
  globalThis.fetch = async () => { calls++; return Response.json(advice); };
  const controller = new AbortController();
  const pending = request(services(() => new Promise((resolve) => { resolveToken = resolve; })), controller.signal);
  controller.abort();
  await assert.rejects(pending, { name: "AbortError" });
  resolveToken("synthetic-session");
  await Promise.resolve();
  assert.equal(calls, 0);
  const session = services(async () => { session.auth.currentUser = { uid: "different-owner" }; return "synthetic-session"; });
  await assert.rejects(request(session), { name: "AbortError" });
  assert.equal(calls, 0);
});

test("body cancellation and account changes discard otherwise valid proposals", async () => {
  let cancelled = false;
  const controller = new AbortController();
  globalThis.fetch = async () => new Response(new ReadableStream({
    start() { setTimeout(() => controller.abort(), 10); }, cancel() { cancelled = true; },
  }), { headers: { "Content-Type": "application/json" } });
  await assert.rejects(request(services(), controller.signal), { name: "AbortError" });
  assert.equal(cancelled, true);
  const session = services();
  globalThis.fetch = async () => { session.auth.currentUser = null; return Response.json(advice); };
  await assert.rejects(request(session), { name: "AbortError" });
});
