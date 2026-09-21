import assert from "node:assert/strict";
import test, { afterEach, before } from "node:test";
import worker from "./index.ts";

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
    assert.match(url.pathname, /gemini-3\.5-flash-lite:generateContent$/);
    assert.equal(new Headers(init?.headers).get("x-goog-api-key"), "test-gemini-key");
    assert.ok(init?.signal instanceof AbortSignal);
    const body = JSON.parse(String(init?.body));
    assert.match(body.systemInstruction.parts[0].text, /Assistente do Aurora Launcher/);
    assert.doesNotMatch(body.systemInstruction.parts[0].text, /dentro do Minecraft/);
    assert.deepEqual(body.contents, [{ role: "user", parts: [{ text: "Como instalar mods?" }] }]);
    return Response.json({ candidates: [{ content: { parts: [{ text: "Use o catálogo." }] } }] });
  });
  const response = await worker.fetch(assistantRequest({ message: "Como instalar mods?", mode: "launcher" }), env);
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { text: "Use o catálogo.", model: "gemini-3.5-flash-lite" });
  assert.equal(calls, 1);
});

test("consulta desktop sem mode mantém compatibilidade e fallback para diagnóstico", async () => {
  const models: string[] = [];
  mockGemini((url) => {
    models.push(url.pathname.split("/").at(-1)!);
    if (models.length === 1) return Response.json({ error: { status: "NOT_FOUND", message: "Model not found" } }, { status: 404 });
    return Response.json({ candidates: [{ content: { parts: [{ text: "Confira o log." }] } }] });
  });
  const response = await worker.fetch(assistantRequest({ message: "Ajude com um crash" }), env);
  assert.equal(response.status, 200);
  assert.deepEqual(models, ["gemini-3.7-flash:generateContent", "gemini-3.5-flash-lite:generateContent"]);
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

test("rede indisponível e timeout mantêm fallback limitado e falha desktop", async () => {
  let calls = 0;
  mockGemini(() => {
    calls += 1;
    throw new DOMException("fixture timeout", "TimeoutError");
  });
  const response = await worker.fetch(assistantRequest({ message: "Olá", mode: "launcher" }), env);
  assert.equal(response.status, 503);
  assert.match(await response.text(), /não está disponível/);
  assert.equal(calls, 3);
});

test("resposta vazia e chave sem autorização permanecem falhas tratadas", async () => {
  mockGemini(() => Response.json({ candidates: [] }));
  const empty = await worker.fetch(assistantRequest({ message: "Olá" }), env);
  assert.equal(empty.status, 503);
  assert.match(await empty.text(), /não retornou uma resposta/);
  mockGemini(() => Response.json({ error: { status: "PERMISSION_DENIED" } }, { status: 403 }));
  const denied = await worker.fetch(assistantRequest({ message: "Olá" }), env);
  assert.equal(denied.status, 503);
  assert.match(await denied.text(), /não autorizou/);
});
