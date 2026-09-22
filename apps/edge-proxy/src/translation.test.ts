import assert from "node:assert/strict";
import test, { afterEach, before } from "node:test";
import worker from "./index.ts";
import { handleProjectTranslation } from "./translation.ts";

const env = { FIREBASE_PROJECT_ID: "translation-test", GEMINI_API_KEY: "test-key" } as never;
const originalFetch = globalThis.fetch;
let token: string;
let publicJwk: JsonWebKey & { kid: string };
const encode = (value: unknown) => Buffer.from(JSON.stringify(value)).toString("base64url");
before(async () => {
  const keys = await crypto.subtle.generateKey({ name: "RSASSA-PKCS1-v1_5", modulusLength: 2_048,
    publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" }, true, ["sign", "verify"]);
  publicJwk = { ...await crypto.subtle.exportKey("jwk", keys.publicKey), alg: "RS256", kid: "translation-test-key", use: "sig" };
  const now = Math.floor(Date.now() / 1_000);
  const payload = `${encode({ alg: "RS256", kid: publicJwk.kid })}.${encode({
    aud: "translation-test", iss: "https://securetoken.google.com/translation-test", sub: "translation-user",
    exp: now + 300, iat: now, auth_time: now,
  })}`;
  const signature = await crypto.subtle.sign("RSASSA-PKCS1-v1_5", keys.privateKey, new TextEncoder().encode(payload));
  token = `${payload}.${Buffer.from(signature).toString("base64url")}`;
});
afterEach(() => { globalThis.fetch = originalFetch; });
const valid = { targetLanguage: "pt-BR", segments: ["Download", "this mod"] };
function request(body: unknown = valid, authenticated = true, signal?: AbortSignal) {
  return new Request("https://aurora.test/v1/translate", { method: "POST", signal,
    headers: { "Content-Type": "application/json", ...(authenticated ? { Authorization: `Bearer ${token}` } : {}) }, body: JSON.stringify(body) });
}
function geminiResponse(translations: unknown, finishReason = "STOP") {
  return Response.json({ candidates: [{ finishReason, content: { parts: [{ text: JSON.stringify({ translations }) }] } }] });
}
function mockGemini(reply: (url: URL, init?: RequestInit) => Promise<Response> | Response) {
  globalThis.fetch = async (input, init) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return Response.json({ keys: [publicJwk] });
    assert.equal(url.hostname, "generativelanguage.googleapis.com");
    return reply(url, init);
  };
}

test("rota exige sessão e não envia página anônima ao provedor", async () => {
  globalThis.fetch = async () => { throw new Error("não deveria consultar upstream"); };
  const response = await worker.fetch(request(valid, false), env);
  assert.equal(response.status, 401);
});

test("rota autenticada retorna trechos estruturados, com schema, idioma e limite de tempo", async () => {
  mockGemini((_url, init) => {
    assert.equal(new Headers(init?.headers).get("x-goog-api-key"), "test-key");
    assert.ok(init?.signal instanceof AbortSignal);
    const body = JSON.parse(String(init?.body));
    assert.equal(body.generationConfig.temperature, 0);
    assert.equal(body.generationConfig.responseMimeType, "application/json");
    assert.equal(body.generationConfig.responseSchema.properties.translations.minItems, 2);
    assert.equal(body.generationConfig.responseSchema.properties.translations.maxItems, 2);
    assert.match(body.systemInstruction.parts[0].text, /DADOS NÃO CONFIÁVEIS/);
    assert.deepEqual(JSON.parse(body.contents[0].parts[0].text), [{ id: 0, text: "Download" }, { id: 1, text: "this mod" }]);
    return geminiResponse([{ id: 0, text: "Baixe" }, { id: 1, text: "este mod" }]);
  });
  const response = await worker.fetch(request(), env);
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("Cache-Control"), "no-store");
  assert.deepEqual(await response.json(), { targetLanguage: "pt-BR", translations: [{ id: 0, text: "Baixe" }, { id: 1, text: "este mod" }] });
});

test("entrada inválida, lote grande e corpo excessivo não consultam Gemini", async () => {
  globalThis.fetch = async () => { throw new Error("não deveria consultar upstream"); };
  for (const body of [null, {}, { ...valid, targetLanguage: "en" }, { ...valid, segments: [] },
    { ...valid, segments: Array(97).fill("text") }, { ...valid, segments: ["x".repeat(3501)] },
    { ...valid, segments: Array(3).fill("x".repeat(3000)) }, { ...valid, extra: "x".repeat(50000) }]) {
    assert.equal((await handleProjectTranslation(request(body), env)).status, 400);
  }
});

test("respostas truncadas, reordenadas, vazias e incompletas preservam original com erro legível", async () => {
  for (const result of [[], [{ id: 0, text: "Baixar" }], [{ id: 1, text: "Baixar" }, { id: 0, text: "mod" }],
    [{ id: 0, text: "" }, { id: 1, text: "mod" }], [{ id: 0, text: "x".repeat(3000) }, { id: 1, text: "mod" }]]) {
    mockGemini(() => geminiResponse(result));
    const response = await handleProjectTranslation(request(), env);
    assert.equal(response.status, 503);
    assert.match(await response.text(), /original foi mantido/);
  }
  mockGemini(() => geminiResponse([{ id: 0, text: "Baixe" }, { id: 1, text: "mod" }], "MAX_TOKENS"));
  assert.equal((await handleProjectTranslation(request(), env)).status, 503);
});

test("fallback é limitado e quota não dispara retry", async () => {
  let calls = 0;
  mockGemini(() => { calls += 1; return new Response("unavailable", { status: 503 }); });
  assert.equal((await handleProjectTranslation(request(), env)).status, 503);
  assert.equal(calls, 2);
  calls = 0;
  mockGemini(() => { calls += 1; return new Response("quota", { status: 429 }); });
  assert.equal((await handleProjectTranslation(request(), env)).status, 429);
  assert.equal(calls, 1);
  calls = 0;
  mockGemini(() => { calls += 1; throw new DOMException("timeout", "TimeoutError"); });
  assert.equal((await handleProjectTranslation(request(), env)).status, 503);
  assert.equal(calls, 2);
});

test("modelo ausente pode usar fallback sem perder os trechos", async () => {
  let calls = 0;
  mockGemini(() => ++calls === 1 ? new Response("missing", { status: 404 }) : geminiResponse([{ id: 0, text: "Baixe" }, { id: 1, text: "mod" }]));
  assert.equal((await handleProjectTranslation(request(), env)).status, 200);
  assert.equal(calls, 2);
});
