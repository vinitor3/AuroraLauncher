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

test("resposta excessiva permite um fallback semântico e configuração recusada não provoca retry", async () => {
  let calls = 0;
  mockGemini(() => ++calls === 1 ? new Response("x".repeat(256001), { headers: { "Content-Type": "application/json" } })
    : geminiResponse([{ id: 0, text: "Baixe" }, { id: 1, text: "este mod" }]));
  assert.equal((await handleProjectTranslation(request(), env)).status, 200);
  assert.equal(calls, 2);
  calls = 0;
  mockGemini(() => { calls++; return new Response(null, { status: 400 }); });
  assert.equal((await handleProjectTranslation(request(), env)).status, 503);
  assert.equal(calls, 1);
});

test("tradução mantém o limite próprio para respostas válidas maiores que o chat", async () => {
  const segments = ["x".repeat(3_000), "y".repeat(3_000)];
  const translations = segments.map((_text, id) => ({ id, text: "Descrição em português. ".repeat(300) }));
  assert.ok(JSON.stringify({ translations }).length > 12_000);
  mockGemini(() => geminiResponse(translations));
  const response = await handleProjectTranslation(request({ ...valid, segments }), env);
  assert.equal(response.status, 200);
  assert.deepEqual((await response.json() as { translations: unknown }).translations,
    translations.map(({ id, text }) => ({ id, text: text.trim() })));
});

test("cancelamento do corpo de entrada não deixa consulta pendente nem chama Gemini", async () => {
  let cancelled = false;
  let calls = 0;
  const controller = new AbortController();
  mockGemini(() => { calls++; return geminiResponse([]); });
  const streamed = new Request("https://aurora.test/v1/translate", {
    method: "POST", signal: controller.signal, headers: { "Content-Type": "application/json" },
    body: new ReadableStream({ cancel() { cancelled = true; } }), duplex: "half",
  } as RequestInit & { duplex: "half" });
  const timer = setTimeout(() => controller.abort(), 10);
  try {
    const response = await handleProjectTranslation(streamed, env);
    assert.equal(response.status, 503);
    assert.equal(cancelled, true);
    assert.equal(calls, 0);
  } finally { clearTimeout(timer); }
});

test("cancelamento após headers interrompe corpo Gemini travado e impede fallback", async () => {
  let cancelled = false;
  let calls = 0;
  const controller = new AbortController();
  mockGemini(() => {
    calls++;
    return new Response(new ReadableStream({ cancel() { cancelled = true; } }), { headers: { "Content-Type": "application/json" } });
  });
  const timer = setTimeout(() => controller.abort(), 10);
  try {
    assert.equal((await handleProjectTranslation(request(valid, true, controller.signal), env)).status, 503);
    assert.equal(cancelled, true);
    assert.equal(calls, 1);
  } finally { clearTimeout(timer); }
});

test("tradução usa chave secundária após indisponibilidade e aceita configuração somente secundária", async () => {
  const usedKeys: string[] = [];
  const configured = { GEMINI_API_KEY: "synthetic-primary", GEMINI_API_KEY_2: "synthetic-secondary", GEMINI_API_KEY_3: "synthetic-third" };
  mockGemini((_url, init) => {
    usedKeys.push(new Headers(init?.headers).get("x-goog-api-key") ?? "");
    return usedKeys.length === 1 ? Response.json({ error: { status: "UNAVAILABLE" } }, { status: 503 })
      : geminiResponse([{ id: 0, text: "Baixe" }, { id: 1, text: "este mod" }]);
  });
  assert.equal((await handleProjectTranslation(request(), configured)).status, 200);
  assert.deepEqual(usedKeys, ["synthetic-primary", "synthetic-secondary"]);
  usedKeys.length = 0;
  mockGemini((_url, init) => {
    usedKeys.push(new Headers(init?.headers).get("x-goog-api-key") ?? "");
    return geminiResponse([{ id: 0, text: "Baixe" }, { id: 1, text: "este mod" }]);
  });
  assert.equal((await handleProjectTranslation(request(), { GEMINI_API_KEY_2: " synthetic-secondary " })).status, 200);
  assert.deepEqual(usedKeys, ["synthetic-secondary"]);
});

test("quota e autorização ou configuração recusadas nunca trocam de chave ou modelo", async () => {
  const configured = { GEMINI_API_KEY: "synthetic-primary", GEMINI_API_KEY_2: "synthetic-secondary", GEMINI_API_KEY_3: "synthetic-third" };
  for (const status of [400, 401, 403, 429]) {
    let calls = 0;
    mockGemini(() => { calls++; return Response.json({ error: { message: "synthetic-private-provider-detail" } }, { status }); });
    const response = await handleProjectTranslation(request(), configured);
    assert.equal(response.status, status === 429 ? 429 : 503);
    assert.doesNotMatch(await response.text(), /synthetic|private-provider-detail/);
    assert.equal(calls, 1);
  }
});

test("fallback de chaves e duas respostas inválidas compartilham no máximo quatro chamadas", async () => {
  const configured = { GEMINI_API_KEY: "synthetic-primary", GEMINI_API_KEY_2: "synthetic-secondary", GEMINI_API_KEY_3: "synthetic-third" };
  const models: string[] = [];
  mockGemini((url) => {
    models.push(url.pathname);
    return models.length % 2 === 1 ? Response.json({ error: { status: "UNAVAILABLE" } }, { status: 503 }) : geminiResponse([]);
  });
  assert.equal((await handleProjectTranslation(request(), configured)).status, 503);
  assert.equal(models.length, 4);
  assert.match(models[0], /gemini-flash-latest:generateContent$/);
  assert.match(models[2], /gemini-3\.8-flash:generateContent$/);
});

test("prazo total inclui corpo de entrada antes de consultar qualquer chave", async (t) => {
  const timeout = AbortSignal.timeout.bind(AbortSignal);
  const durations: number[] = [];
  t.mock.method(AbortSignal, "timeout", (duration: number) => { durations.push(duration); return timeout(10); });
  let cancelled = false;
  mockGemini(() => { assert.fail("An expired request body must not reach the provider"); });
  const streamed = new Request("https://aurora.test/v1/translate", {
    method: "POST", headers: { "Content-Type": "application/json" },
    body: new ReadableStream({ cancel() { cancelled = true; } }), duplex: "half",
  } as RequestInit & { duplex: "half" });
  // AbortSignal.timeout uses an unreferenced timer; keep this synthetic stream alive.
  const keepAlive = setTimeout(() => undefined, 1_000);
  try {
    assert.equal((await handleProjectTranslation(streamed, env)).status, 503);
    assert.equal(cancelled, true);
    assert.deepEqual(durations, [35_000]);
  } finally { clearTimeout(keepAlive); }
});
