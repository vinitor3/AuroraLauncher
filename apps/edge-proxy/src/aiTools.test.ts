import assert from "node:assert/strict";
import test, { afterEach } from "node:test";
import { buildAiToolRequest, parseAiToolRequest, parseAiToolAdvice, type AiToolContext } from "../../../shared/aiTools.ts";
import { handleAiTools } from "./aiTools.ts";
import { handleAssistant } from "./assistant.ts";
const original = globalThis.fetch;
afterEach(() => { globalThis.fetch = original; });
const context: AiToolContext = { schemaVersion: 1, contextId: "private-snapshot-id", truncated: false,
  mods: [{ id: "mod-1", name: "Synthetic Mod", version: "1.0", enabled: true, managed: false }, { id: "mod-2", name: "Aurora Core", version: "1.1.5", enabled: true, managed: true }],
  settings: [{ id: "setting-1", label: "Rendering distance", value: 12, kind: "number", min: 2, max: 32 }], history: [] };
const input = () => buildAiToolRequest(context, "Ajude a investigar uma falha.");
const reply = { summary: "Ajuste sugerido para revisão.", proposals: [{ kind: "set_setting", targetId: "setting-1", value: 8, reason: "Reduzir a distância pode diminuir a carga gráfica." }] };

test("projection omits context and record IDs, times, paths and extra local fields", () => {
  const history = Array.from({ length: 15 }, (_, n) => ({ id: `private-${n}`, actionId: `secret-${n}`, createdAtMs: n,
    kind: "action" as const, status: "applied" as const, assessment: "unresolved" as const, undoAvailable: true,
    title: `Registro ${n}`, summary: "Remoção confirmada; o erro persistiu.", issueIds: ["mixin_failure"] }));
  const value = buildAiToolRequest({ ...context, history }, "Ajude");
  assert.equal(value.context.history.length, 12);
  assert.equal(value.context.history[0].title, "Registro 0");
  assert.equal(value.context.history[11].title, "Registro 11");
  assert.doesNotMatch(JSON.stringify(value), /private-|secret-|createdAtMs|contextId|undoAvailable/);
  assert.equal(value.context.history[0].assessment, "unresolved");
});

test("invalid, duplicate and unknown context fields fail before consulting Gemini", async () => {
  globalThis.fetch = async () => assert.fail("No invalid context upstream");
  for (const bad of [{ ...input(), files: ["secret"] }, { ...input(), context: { ...input().context, paths: ["private"] } },
    { ...input(), context: { ...input().context, mods: [context.mods[0], context.mods[0]] } },
    { ...input(), context: { ...input().context, settings: [{ ...context.settings[0], value: 50 }] } }]) {
    assert.equal(parseAiToolRequest(bad), null);
    const response = await handleAiTools(new Request("https://test/v1/ai/tools", { method: "POST", body: JSON.stringify(bad) }), { GEMINI_API_KEY: "synthetic" }, new AbortController().signal);
    assert.equal(response.status, 400);
  }
});

test("model cannot invent targets, modify protected mods or escape registered option constraints", () => {
  const invalid = [{ kind: "shell", targetId: "mod-1", reason: "Teste" }, { kind: "remove_mod", targetId: "mod-2", reason: "Teste" },
    { kind: "remove_mod", targetId: "missing", reason: "Teste" }, { kind: "set_setting", targetId: "setting-1", value: 80, reason: "Teste" },
    { kind: "set_setting", targetId: "setting-1", value: 12, reason: "Teste" }, { kind: "set_setting", targetId: "setting-1", value: true, reason: "Teste" },
    { kind: "remove_mod", targetId: "mod-1", reason: "Teste", path: "../mod.jar" }];
  for (const proposal of invalid) assert.equal(parseAiToolAdvice({ summary: "Teste", proposals: [proposal], model: "gemini-3.8-flash" }, input().context), null);
  assert.ok(parseAiToolAdvice({ ...reply, model: "gemini-3.8-flash" }, input().context));
});

test("tool endpoint returns validated proposals only; it cannot execute a native action", async () => {
  let calls = 0;
  globalThis.fetch = async (url, init) => {
    calls++;
    assert.match(String(url), /generativelanguage.googleapis.com/);
    const body = JSON.parse(String(init?.body));
    assert.deepEqual(JSON.parse(body.contents[0].parts[0].text), input());
    assert.match(body.systemInstruction.parts[0].text, /confirmação humana/);
    return Response.json({ modelVersion: "gemini-3.8-flash", candidates: [{ finishReason: "STOP", content: { parts: [{ text: JSON.stringify(reply) }] } }] });
  };
  const response = await handleAiTools(new Request("https://test/v1/ai/tools", { method: "POST", body: JSON.stringify(input()) }), { GEMINI_API_KEY: "synthetic" }, new AbortController().signal);
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { ...reply, model: "gemini-3.8-flash" });
  assert.equal(calls, 1);
});

test("malformed and stalled request bodies are classified before contacting the provider", async () => {
  globalThis.fetch = async () => assert.fail("Invalid request never contacts Gemini");
  for (const handler of [handleAssistant, handleAiTools]) {
    const malformed = await handler(new Request("https://test", { method: "POST", body: "{" }), {}, new AbortController().signal);
    assert.equal(malformed.status, 400);
    assert.equal((await malformed.json()).code, "invalid_request");
    const pending = new Request("https://test", { method: "POST", body: new ReadableStream(), duplex: "half" } as RequestInit);
    const timedOut = await handler(pending, {}, AbortSignal.timeout(20));
    assert.equal(timedOut.status, 504);
    assert.equal((await timedOut.json()).code, "timeout");
  }
});
