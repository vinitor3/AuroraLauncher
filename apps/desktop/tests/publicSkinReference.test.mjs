import assert from "node:assert/strict";
import test from "node:test";
import { PUBLIC_SKIN_WORKER, publicCatalogSource, skinSha256, verifyPublicCatalogSkin } from "../src/lib/skinCatalogSources.ts";

const image = new Blob(["fixture"], { type: "image/png" });
const hash = await skinSha256(image);
const encoded = (value) => Buffer.from(value).toString("base64url");
const asset = "https://mcstat.org/media/skins/published/fixture.png";
const path = `/v1/skins/catalog/mcstat/${hash}/${encoded(asset)}.png`;
const reference = PUBLIC_SKIN_WORKER + path;

test("verified fallback returns the public Worker URL only for the equipped bytes", async (t) => {
  t.mock.method(globalThis, "fetch", async (url, init) => {
    assert.equal(url, reference);
    assert.equal(init.credentials, "omit");
    assert.equal(init.redirect, "error");
    assert.equal(init.referrerPolicy, "no-referrer");
    return new Response(image);
  });
  assert.equal(await verifyPublicCatalogSkin(reference, image), reference);
  assert.equal(await verifyPublicCatalogSkin(reference, new Blob(["changed"])), undefined);
  assert.equal(fetch.mock.callCount(), 1);
});

test("personal URLs, query parameters, credentials, old Studio and custom workers never leave the device", async (t) => {
  t.mock.method(globalThis, "fetch", () => assert.fail("private source must stay local"));
  for (const source of [undefined, "blob:private", "https://privado.example" + path,
    reference + "?token=private", reference + "#private", reference.replace("https://", "https://user:password@"),
    reference.replace(encoded(asset), encoded(asset + "?token=private")),
    PUBLIC_SKIN_WORKER + "/v1/skins/catalog/solar-ninja.png"]) {
    assert.equal(await verifyPublicCatalogSkin(source, image), undefined);
  }
  assert.equal(fetch.mock.callCount(), 0);
  assert.ok(publicCatalogSource(reference.replace("catalog/mcstat", "player-texture"), hash));
});

test("changed, missing, or non-PNG public content keeps the profile local", async (t) => {
  const responses = [new Response(new Blob(["changed"], { type: "image/png" })), new Response(null, { status: 404 }), new Response("fixture")];
  t.mock.method(globalThis, "fetch", async () => responses.shift());
  for (let i = 0; i < 3; i++) assert.equal(await verifyPublicCatalogSkin(reference, image), undefined);
});
