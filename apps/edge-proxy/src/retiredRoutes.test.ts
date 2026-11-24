import assert from "node:assert/strict";
import test from "node:test";
import worker from "./index.ts";

test("rotas do assistente foram retiradas antes de autenticar ou consultar provedores", async () => {
  const original = globalThis.fetch;
  globalThis.fetch = () => { throw new Error("Rota retirada não pode contatar a rede."); };
  try {
    for (const path of ["/v1/assistant", "/v1/doctor/advice", "/v1/ai/tools"]) {
      const request = new Request("https://aurora.test" + path, { method: "POST", body: "{}" });
      assert.equal((await worker.fetch(request, {} as never)).status, 404);
    }
  } finally {
    globalThis.fetch = original;
  }
});
