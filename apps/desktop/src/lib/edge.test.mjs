import assert from "node:assert/strict";
import test, { afterEach } from "node:test";
import { askAuroraAssistant, translateProjectSegments } from "./edge.ts";

const originalFetch = globalThis.fetch;
afterEach(() => { globalThis.fetch = originalFetch; });
const services = {
  config: { workerUrl: "https://aurora.test/" },
  auth: { currentUser: { getIdToken: async () => "test-session-token" } },
};

test("o painel autentica a pergunta launcher e recebe o texto sem depender de IPC", async () => {
  let calls = 0;
  globalThis.fetch = async (url, init) => {
    calls += 1;
    assert.equal(url, "https://aurora.test/v1/assistant");
    assert.equal(init.headers.Authorization, "Bearer test-session-token");
    assert.deepEqual(JSON.parse(init.body), { message: "Como instalar mods?", mode: "launcher" });
    assert.ok(init.signal instanceof AbortSignal);
    return Response.json({ text: "Use o catálogo do launcher.", model: "test-model" });
  };
  assert.equal(await askAuroraAssistant(services, "  Como instalar mods?  "), "Use o catálogo do launcher.");
  assert.equal(calls, 1);
});

test("mensagem inválida ou sessão ausente não faz consulta externa", async () => {
  globalThis.fetch = async () => { throw new Error("Não deveria chamar fetch"); };
  await assert.rejects(askAuroraAssistant(services, "  "), /1 e 2.000/);
  await assert.rejects(askAuroraAssistant(services, "x".repeat(2_001)), /1 e 2.000/);
  await assert.rejects(askAuroraAssistant({ ...services, auth: { currentUser: null } }, "Olá"), { code: "session" });
});

test("erros de rede e timeout mantêm mensagens legíveis no painel", async () => {
  globalThis.fetch = async () => { throw new TypeError("network failed"); };
  await assert.rejects(askAuroraAssistant(services, "Olá"), /Não foi possível conectar/);
  globalThis.fetch = async () => { throw new DOMException("timed out", "TimeoutError"); };
  await assert.rejects(askAuroraAssistant(services, "Olá"), /demorou para responder/);
});

test("falhas de autorização e quota permanecem disponíveis para os controles desktop", async () => {
  globalThis.fetch = async () => Response.json({ error: "Sua sessão expirou." }, { status: 401 });
  await assert.rejects(askAuroraAssistant(services, "Olá"), { code: "session" });
  globalThis.fetch = async () => Response.json({ error: "O limite temporário do Gemini foi atingido." }, { status: 429 });
  await assert.rejects(askAuroraAssistant(services, "Olá"), /limite temporário/);
});

test("tradução autentica somente os textos, define idioma e aceita cancelamento", async () => {
  const controller = new AbortController();
  globalThis.fetch = async (url, init) => {
    assert.equal(url, "https://aurora.test/v1/translate");
    assert.equal(init.headers.Authorization, "Bearer test-session-token");
    assert.deepEqual(JSON.parse(init.body), { targetLanguage: "pt-BR", segments: ["Download"] });
    assert.ok(init.signal instanceof AbortSignal);
    return Response.json({ translations: [{ id: 0, text: "Baixar" }] });
  };
  assert.deepEqual(await translateProjectSegments(services, ["Download"], controller.signal), [{ id: 0, text: "Baixar" }]);
  controller.abort();
  globalThis.fetch = async () => { throw new DOMException("aborted", "AbortError"); };
  await assert.rejects(translateProjectSegments(services, ["Download"], controller.signal), { name: "AbortError" });
});
