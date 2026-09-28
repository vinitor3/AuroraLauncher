import assert from "node:assert/strict";
import test, { afterEach } from "node:test";
import { generateGemini, GEMINI_THINKING } from "./gemini.ts";
const original = globalThis.fetch;
afterEach(() => { globalThis.fetch = original; });
const env = { GEMINI_API_KEY: "synthetic-key" };
const reply = (parts: unknown[], finishReason = "STOP") => Response.json({ modelVersion: "gemini-3.8-flash", candidates: [{ finishReason, content: { parts } }] });

test("latest alias uses current thinking level and only visible completed text reaches client", async () => {
  globalThis.fetch = async (url, init) => {
    assert.match(String(url), /gemini-flash-latest:generateContent$/);
    assert.deepEqual(JSON.parse(String(init?.body)).generationConfig.thinkingConfig, { thinkingLevel: "low" });
    return reply([{ thought: true, text: "private reasoning" }, { text: "Resposta curta." }]);
  };
  assert.deepEqual(await generateGemini(env, { generationConfig: { thinkingConfig: GEMINI_THINKING } }, new AbortController().signal),
    { text: "Resposta curta.", model: "gemini-3.8-flash" });
});

test("empty, thought-only, malformed and truncated output are explicit failures", async () => {
  for (const response of [reply([]), reply([{ thought: true, text: "hidden" }]), reply([{ text: "partial" }], "MAX_TOKENS"),
    new Response("invalid json", { headers: { "Content-Type": "application/json" } }), reply([{ text: " " }])]) {
    globalThis.fetch = async () => response;
    await assert.rejects(generateGemini(env, {}, new AbortController().signal), { code: "invalid_response" });
  }
});

test("key, quota and unavailable failures are distinct without exposing provider bodies", async () => {
  for (const [status, code] of [[403, "provider_unauthorized"], [429, "quota"], [503, "provider_unavailable"]] as const) {
    let calls = 0;
    globalThis.fetch = async () => { calls++; return Response.json({ error: { message: "api_key=synthetic-private-value" } }, { status }); };
    await assert.rejects(generateGemini(env, {}, new AbortController().signal), (error: { code: string; message: string }) => error.code === code && !error.message.includes("synthetic"));
    assert.equal(calls, status === 503 ? 2 : 1);
  }
  globalThis.fetch = async () => assert.fail("Unconfigured key never sends a request");
  await assert.rejects(generateGemini({}, {}, new AbortController().signal), { code: "not_configured" });
});

test("missing latest alias can fall back to explicit3.8", async () => {
  const calls: string[] = [];
  globalThis.fetch = async (url) => { calls.push(String(url)); return calls.length === 1 ? Response.json({ error: { status: "NOT_FOUND" } }, { status: 404 }) : reply([{ text: "OK" }]); };
  assert.equal((await generateGemini(env, {}, new AbortController().signal)).text, "OK");
  assert.match(calls[1], /gemini-3.8-flash:generateContent$/);
  assert.equal(calls.length, 2);
});

test("temporary 503 permits one bounded fallback, while cancellation prevents it", async () => {
  const calls: string[] = [];
  globalThis.fetch = async (url) => {
    calls.push(String(url));
    return calls.length === 1 ? Response.json({ error: { status: "UNAVAILABLE" } }, { status: 503 }) : reply([{ text: "Disponível." }]);
  };
  assert.equal((await generateGemini(env, {}, new AbortController().signal)).text, "Disponível.");
  assert.equal(calls.length, 2);
  assert.match(calls[1], /gemini-3.8-flash:generateContent$/);
  calls.length = 0;
  await assert.rejects(generateGemini(env, {}, AbortSignal.timeout(20)), { code: "timeout" });
  assert.equal(calls.length, 1);
});

test("abort covers a stalled response after headers and never keeps a pending request", async () => {
  const signal = AbortSignal.timeout(20);
  let cancelled = false;
  globalThis.fetch = async () => new Response(new ReadableStream({ cancel() { cancelled = true; } }), { headers: { "Content-Type": "application/json" } });
  await assert.rejects(generateGemini(env, {}, signal), { code: "timeout" });
  assert.equal(cancelled, true);
});
