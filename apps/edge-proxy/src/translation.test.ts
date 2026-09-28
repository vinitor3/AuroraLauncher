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
    assert.deepEqual(body.generationConfig.thinkingConfig, { thinkingLevel: "low" });
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

test("respostas truncadas, vazias e incompletas preservam original com erro legível", async () => {
  for (const result of [[], [{ id: 0, text: "Baixar" }], [{ id: 0, text: "Baixar" }, { id: 0, text: "mod" }],
    [{ id: 0, text: "" }, { id: 1, text: "mod" }], [{ id: 0, text: "x".repeat(3000) }, { id: 1, text: "mod" }]]) {
    mockGemini(() => geminiResponse(result));
    const response = await handleProjectTranslation(request(), env);
    assert.equal(response.status, 503);
    assert.match(await response.text(), /Não foi possível traduzir este trecho/);
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

test("IDs únicos fora de ordem são reorganizados para seus trechos originais", async () => {
  mockGemini(() => geminiResponse([{ id: 1, text: "este mod" }, { id: 0, text: "Baixe" }]));
  const response = await handleProjectTranslation(request(), env);
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { targetLanguage: "pt-BR", translations: [{ id: 0, text: "Baixe" }, { id: 1, text: "este mod" }] });
});

test("IDs duplicados, inválidos e omitidos nunca permitem associar texto ao nó errado", async () => {
  for (const id of [0, -1, 2, 0.5, "1", null]) {
    let calls = 0;
    mockGemini(() => { calls += 1; return geminiResponse([{ id: 0, text: "Baixe" }, { id, text: "este mod" }]); });
    assert.equal((await handleProjectTranslation(request(), env)).status, 503, `id: ${id}`);
    assert.equal(calls, 2, "resposta inválida deve usar somente um fallback");
  }
});

test("página mista mantém português e nomes enquanto traduz japonês, chinês e inglês", async () => {
  const segments = ["このMODは新しい建物を追加します。", "这个模组支持多人游戏。", "Install the mod with Fabric.", "Configurações de desempenho", "Fabric API"];
  const translations = ["Este mod adiciona novas construções.", "Este mod oferece suporte a jogos multijogador.", "Instale o mod com Fabric.", segments[3], segments[4]];
  mockGemini((_url, init) => {
    const body = JSON.parse(String(init?.body));
    assert.deepEqual(JSON.parse(body.contents[0].parts[0].text), segments.map((text, id) => ({ id, text })));
    assert.match(body.systemInstruction.parts[0].text, /idioma de cada trecho separadamente/);
    assert.match(body.systemInstruction.parts[0].text, /japonês, chinês/);
    assert.equal(body.generationConfig.maxOutputTokens, 16384);
    return geminiResponse(translations.map((text, id) => ({ id, text })).reverse());
  });
  const response = await handleProjectTranslation(request({ ...valid, segments }), env);
  assert.equal(response.status, 200);
  const result = await response.json() as { translations: Array<{ id: number; text: string }> };
  assert.deepEqual(result.translations.map(({ text }) => text), translations);
});

test("expansão de caracteres CJK para português pode superar o antigo limite de seis vezes", async () => {
  const source = "漢".repeat(100);
  const translated = "Descrição completa em português. ".repeat(30).trim();
  assert.ok(translated.length > source.length * 6 + 200);
  assert.ok(translated.length <= source.length * 12 + 200);
  mockGemini(() => geminiResponse([{ id: 0, text: translated }]));
  const response = await handleProjectTranslation(request({ ...valid, segments: [source] }), env);
  assert.equal(response.status, 200);
  mockGemini(() => geminiResponse([{ id: 0, text: "x".repeat(source.length * 12 + 201) }]));
  assert.equal((await handleProjectTranslation(request({ ...valid, segments: [source] }), env)).status, 503);
});

test("JSON quebrado, trecho ausente e limite de tokens tentam o segundo modelo sem publicar parciais", async () => {
  for (const firstResponse of [
    () => new Response("not-json", { headers: { "Content-Type": "application/json" } }),
    () => geminiResponse([{ id: 0, text: "Baixe" }]),
    () => geminiResponse([{ id: 0, text: "Baixe" }, { id: 1, text: "este mod" }], "MAX_TOKENS"),
  ]) {
    let calls = 0;
    mockGemini((url, init) => {
      calls += 1;
      if (calls === 1) return firstResponse();
      assert.match(url.pathname, /gemini-3\.8-flash:generateContent$/);
      assert.deepEqual(JSON.parse(String(init?.body)).generationConfig.thinkingConfig, { thinkingLevel: "low" });
      return geminiResponse([{ id: 0, text: "Baixe agora" }, { id: 1, text: "o mod" }]);
    });
    const response = await handleProjectTranslation(request(), env);
    assert.equal(response.status, 200);
    assert.deepEqual((await response.json() as { translations: unknown }).translations, [{ id: 0, text: "Baixe agora" }, { id: 1, text: "o mod" }]);
    assert.equal(calls, 2);
  }
});

test("prosa CJK copiada literalmente pede fallback; nomes curtos e português podem ficar iguais", async () => {
  const original = "この新しいMODをインストールすると建物が追加されます。";
  let calls = 0;
  mockGemini(() => geminiResponse([{ id: 0, text: ++calls === 1 ? original : "Instale este novo mod para adicionar construções." }]));
  assert.equal((await handleProjectTranslation(request({ ...valid, segments: [original] }), env)).status, 200);
  assert.equal(calls, 2);
  mockGemini(() => geminiResponse([{ id: 0, text: original }]));
  assert.equal((await handleProjectTranslation(request({ ...valid, segments: [original] }), env)).status, 503);
  const names = ["Fabric API", "Sodium", "東方幻想郷", "Configurações de desempenho"];
  calls = 0;
  mockGemini(() => { calls += 1; return geminiResponse(names.map((text, id) => ({ id, text }))); });
  assert.equal((await handleProjectTranslation(request({ ...valid, segments: names }), env)).status, 200);
  assert.equal(calls, 1, "nomes e português não devem gerar custo ou erro por falso no-op");
});

test("cancelamento impede iniciar ou continuar fallback e credencial negada não provoca repetição", async () => {
  let calls = 0;
  const controller = new AbortController();
  controller.abort();
  mockGemini(() => { calls += 1; throw new Error("não deveria consultar upstream"); });
  assert.equal((await handleProjectTranslation(request(valid, true, controller.signal), env)).status, 503);
  assert.equal(calls, 0);
  const running = new AbortController();
  mockGemini(() => { calls += 1; running.abort(); throw new DOMException("cancelled", "AbortError"); });
  assert.equal((await handleProjectTranslation(request(valid, true, running.signal), env)).status, 503);
  assert.equal(calls, 1);
  mockGemini(() => { calls += 1; return new Response(null, { status: 403 }); });
  assert.equal((await handleProjectTranslation(request(), env)).status, 503);
  assert.equal(calls, 2);
});

test("resposta excessiva e configuração recusada têm fallback limitado", async () => {
  for (const firstResponse of [
    () => new Response("x".repeat(256001)),
    () => new Response(null, { status: 400 }),
  ]) {
    let calls = 0;
    mockGemini(() => ++calls === 1 ? firstResponse() : geminiResponse([{ id: 0, text: "Baixe" }, { id: 1, text: "este mod" }]));
    assert.equal((await handleProjectTranslation(request(), env)).status, 200);
    assert.equal(calls, 2);
  }
});
