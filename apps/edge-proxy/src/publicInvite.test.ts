import assert from "node:assert/strict";
import test from "node:test";
import worker from "./index.ts";

const id = "4a8b5173-8dea-4b58-a496-fb26d076074b";

test("a prévia de convite abre somente o launcher e preserva a decisão no aplicativo", async () => {
  const origin = "https://aurora.test/join/";
  const get = (path: string, method = "GET") => worker.fetch(new Request(origin + path, { method }), {} as never);
  const response = await get(id);
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("referrer-policy"), "no-referrer");
  const html = await response.text();
  assert.ok(html.includes('href="aurora-connect://join/' + id + '"'));
  assert.ok(html.includes('value="aurora-v1:' + id + '"'));
  assert.doesNotMatch(html, /<script|<iframe|<form/i);
  const head = await get(id, "HEAD");
  assert.equal(head.status, 200);
  assert.equal(await head.text(), "");
  for (const path of ["", "convite", id + "/extra", id + "?next=https://example.org", id.replace("-4b58-", "-9b58-")]) {
    assert.equal((await get(path)).status, 404);
  }
  assert.equal((await get(id, "POST")).status, 404);
});
