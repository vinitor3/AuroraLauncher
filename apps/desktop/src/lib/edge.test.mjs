import assert from "node:assert/strict";
import test, { afterEach } from "node:test";
import { translateProjectSegments } from "./edge.ts";

const originalFetch = globalThis.fetch;
afterEach(() => { globalThis.fetch = originalFetch; });
const services = {
  config: { workerUrl: "https://aurora.test/" },
  auth: { currentUser: { getIdToken: async () => "test-session-token" } },
};

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
