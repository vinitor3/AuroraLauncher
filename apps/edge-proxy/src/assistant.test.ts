import assert from "node:assert/strict";
import test, { afterEach, before } from "node:test";
import worker from "./index.ts";
import { buildAiToolRequest } from "../../../shared/aiTools.ts";

const env = { FIREBASE_PROJECT_ID: "assistant-test", GEMINI_API_KEY: "test-gemini-key" } as never;
const originalFetch = globalThis.fetch;
let token: string;
let publicJwk: JsonWebKey & { kid: string };
const encode = (value: unknown) => Buffer.from(JSON.stringify(value)).toString("base64url");

before(async () => {
  const keys = await crypto.subtle.generateKey({ name: "RSASSA-PKCS1-v1_5", modulusLength: 2_048,
    publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" }, true, ["sign", "verify"]);
  publicJwk = { ...await crypto.subtle.exportKey("jwk", keys.publicKey), alg: "RS256", kid: "assistant-test-key", use: "sig" };
  const now = Math.floor(Date.now() / 1_000);
  const payload = `${encode({ alg: "RS256", kid: "assistant-test-key" })}.${encode({
    aud: "assistant-test", iss: "https://securetoken.google.com/assistant-test", sub: "assistant-user",
    user_id: "assistant-user", exp: now + 300, iat: now, auth_time: now,
  })}`;
  const signature = await crypto.subtle.sign("RSASSA-PKCS1-v1_5", keys.privateKey, new TextEncoder().encode(payload));
  token = `${payload}.${Buffer.from(signature).toString("base64url")}`;
});

afterEach(() => { globalThis.fetch = originalFetch; });

function assistantRequest(body: unknown, authenticated = true) {
  return new Request("https://aurora.test/v1/assistant", {
    method: "POST",
    headers: { "Content-Type": "application/json", ...(authenticated ? { Authorization: `Bearer ${token}` } : {}) },
    body: JSON.stringify(body),
  });
}

function mockGemini(reply: (url: URL, init?: RequestInit) => Promise<Response> | Response) {
  globalThis.fetch = async (input, init) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return Response.json({ keys: [publicJwk] });
    assert.equal(url.hostname, "generativelanguage.googleapis.com");
    return reply(url, init);
  };
}

test("a rota autenticada mantém texto, contexto desktop e modelo padrão do launcher", async () => {
  let calls = 0;
  mockGemini((url, init) => {
    calls += 1;
    assert.match(url.pathname, /gemini-flash-latest:generateContent$/);
    assert.equal(new Headers(init?.headers).get("x-goog-api-key"), "test-gemini-key");
    assert.ok(init?.signal instanceof AbortSignal);
    const body = JSON.parse(String(init?.body));
    assert.match(body.systemInstruction.parts[0].text, /Assistente do Aurora Launcher/);
    assert.doesNotMatch(body.systemInstruction.parts[0].text, /dentro do Minecraft/);
    assert.deepEqual(body.contents, [{ role: "user", parts: [{ text: "Como instalar mods?" }] }]);
    assert.equal(body.generationConfig.thinkingConfig.thinkingLevel, "low");
    assert.ok(body.generationConfig.maxOutputTokens >= 2048);
    return Response.json({ candidates: [{ finishReason: "STOP", content: { parts: [{ text: "Use o catálogo." }] } }] });
  });
  const response = await worker.fetch(assistantRequest({ message: "Como instalar mods?", mode: "launcher" }), env);
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { text: "Use o catálogo.", model: "gemini-flash-latest" });
  assert.equal(calls, 1);
});

test("consulta desktop sem mode mantém compatibilidade e fallback para diagnóstico", async () => {
  const models: string[] = [];
  mockGemini((url) => {
    models.push(url.pathname.split("/").at(-1)!);
    if (models.length === 1) return Response.json({ error: { status: "NOT_FOUND", message: "Model not found" } }, { status: 404 });
    return Response.json({ candidates: [{ finishReason: "STOP", content: { parts: [{ text: "Confira o log." }] } }] });
  });
  const response = await worker.fetch(assistantRequest({ message: "Ajude com um crash" }), env);
  assert.equal(response.status, 200);
  assert.deepEqual(models, ["gemini-flash-latest:generateContent", "gemini-3.8-flash:generateContent"]);
});

test("pedidos antigos do mod e capturas não consultam Gemini", async () => {
  let calls = 0;
  mockGemini(() => { calls += 1; throw new Error("O mod removido não deve consultar Gemini"); });
  for (const payload of [
    { message: "Olá", mode: "inGame" },
    { message: "Olá", mode: "launcher", screenshotBase64: "data:image/png;base64,AA==" },
    { message: "Olá", screenshotBase64: "" },
    { message: "Olá", mode: "unsupported" },
  ]) {
    const response = await worker.fetch(assistantRequest(payload), env);
    assert.equal(response.status, 400);
    assert.match(await response.text(), /Use o Assistente do Aurora Launcher/);
  }
  assert.equal(calls, 0);
});

test("a rota desktop ainda exige uma sessão válida", async () => {
  let calls = 0;
  globalThis.fetch = async () => { calls += 1; throw new Error("Não deve consultar upstream"); };
  const response = await worker.fetch(assistantRequest({ message: "Olá", mode: "launcher" }, false), env);
  assert.equal(response.status, 401);
  assert.equal(calls, 0);
});

test("a quota Gemini retorna erro legível sem repetir a consulta", async () => {
  let calls = 0;
  mockGemini(() => {
    calls += 1;
    return Response.json({ error: { status: "RESOURCE_EXHAUSTED" } }, { status: 429 });
  });
  const response = await worker.fetch(assistantRequest({ message: "Olá", mode: "launcher" }), env);
  assert.equal(response.status, 429);
  assert.match(await response.text(), /limite temporário/);
  assert.equal(calls, 1);
});

test("rede indisponível não repete a consulta e mantém erro público", async () => {
  let calls = 0;
  mockGemini(() => {
    calls += 1;
    throw new DOMException("fixture timeout", "TimeoutError");
  });
  const response = await worker.fetch(assistantRequest({ message: "Olá", mode: "launcher" }), env);
  assert.equal(response.status, 503);
  assert.equal((await response.json() as { code: string }).code, "provider_unavailable");
  assert.equal(calls, 1);
});

test("resposta vazia e chave sem autorização permanecem falhas tratadas", async () => {
  mockGemini(() => Response.json({ candidates: [] }));
  const empty = await worker.fetch(assistantRequest({ message: "Olá" }), env);
  assert.equal(empty.status, 502);
  assert.equal((await empty.json() as { code: string }).code, "invalid_response");
  mockGemini(() => Response.json({ error: { status: "PERMISSION_DENIED" } }, { status: 403 }));
  const denied = await worker.fetch(assistantRequest({ message: "Olá" }), env);
  assert.equal(denied.status, 503);
  assert.equal((await denied.json() as { code: string }).code, "provider_unauthorized");
});

test("nova rota de ferramentas exige autenticação e valida propostas dentro do contexto", async () => {
  const body = buildAiToolRequest({ schemaVersion: 1, contextId: "local-only", truncated: false,
    mods: [{ id: "mod-1", name: "Example Mod", version: "1.0", enabled: true, managed: false }], settings: [], history: [] }, "Investigar o mod.");
  const request = (authenticated: boolean) => new Request("https://aurora.test/v1/ai/tools", {
    method: "POST", headers: { "Content-Type": "application/json", ...(authenticated ? { Authorization: `Bearer ${token}` } : {}) }, body: JSON.stringify(body),
  });
  let calls = 0;
  const advice = { summary: "Uma tentativa pode ser revisada.", proposals: [{ kind: "remove_mod", targetId: "mod-1", reason: "Testar a compatibilidade." }] };
  mockGemini((_url, init) => {
    calls++;
    assert.deepEqual(JSON.parse(JSON.parse(String(init?.body)).contents[0].parts[0].text), body);
    return Response.json({ modelVersion: "gemini-3.8-flash", candidates: [{ finishReason: "STOP", content: { parts: [{ text: JSON.stringify(advice) }] } }] });
  });
  assert.equal((await worker.fetch(request(false), env)).status, 401);
  assert.equal(calls, 0);
  const response = await worker.fetch(request(true), env);
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { ...advice, model: "gemini-3.8-flash" });
  assert.equal(calls, 1);
});
