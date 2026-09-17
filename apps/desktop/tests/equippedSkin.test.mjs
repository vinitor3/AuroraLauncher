import assert from "node:assert/strict";
import test from "node:test";
import { findMatchingLocalSkin, parseEquippedSkinSelection } from "../src/lib/equippedSkin.ts";

const image = (...bytes) => new Blob([new Uint8Array(bytes)], { type: "image/png" });
const skin = (id, bytes, skinModel = "slim") => ({
  id, ownerId: "player", name: id, skinModel, createdAt: 1,
  ...(bytes ? { image: image(...bytes) } : {}),
});

test("selection metadata accepts both supported models and ignores unrelated fields", () => {
  for (const skinModel of ["classic", "slim"]) {
    assert.deepEqual(parseEquippedSkinSelection(JSON.stringify({ id: "local-skin", skinModel, sourceUrl: "https://example/skin" })), { id: "local-skin", skinModel });
  }
  assert.equal(parseEquippedSkinSelection(JSON.stringify({ id: "x".repeat(128), skinModel: "slim" })).id.length, 128);
});

test("malformed, missing or renderer-only selection metadata is rejected", () => {
  for (const raw of [null, "", "{broken", "null", "[]", '"skin"', "10", "{}",
    ...[
      { id: "", skinModel: "slim" }, { id: "   ", skinModel: "slim" },
      { id: 12, skinModel: "slim" }, { id: "x".repeat(129), skinModel: "slim" },
      { id: "blob:old-session", skinModel: "slim" }, { id: " BLOB:old-session", skinModel: "classic" },
      { id: "skin\n", skinModel: "slim" }, { id: "skin", skinModel: "unknown" },
      { id: "skin" }, { skinModel: "slim" },
    ].map((value) => JSON.stringify(value)),
  ]) assert.equal(parseEquippedSkinSelection(raw), undefined, String(raw));
});

test("same-size files and preferred IDs never substitute for exact byte comparison", async () => {
  const stale = skin("stale", [1, 2, 3, 9]);
  const matching = skin("matching", [1, 2, 3, 4]);
  assert.equal(await findMatchingLocalSkin([stale, matching], image(1, 2, 3, 4), "stale"), matching);
  assert.equal(await findMatchingLocalSkin([stale], image(1, 2, 3, 4), "stale"), undefined);
});

test("preferred identical image preserves selected model among identical PNG variants", async () => {
  const classic = skin("classic-variant", [1, 2, 3], "classic");
  const slim = skin("slim-variant", [1, 2, 3], "slim");
  assert.equal(await findMatchingLocalSkin([classic, slim], image(1, 2, 3), "slim-variant"), slim);
  assert.equal(await findMatchingLocalSkin([classic, slim], image(1, 2, 3), "classic-variant"), classic);
  assert.equal(await findMatchingLocalSkin([classic, slim], image(1, 2, 3), "removed-id"), classic);
  assert.equal(await findMatchingLocalSkin([classic, slim], image(1, 2, 3)), classic);
});

test("missing, wrong-size and unreadable images do not block later matching records", async () => {
  const missing = { ...skin("url-only"), sourceUrl: "https://example/skin.png" };
  const wrongSize = skin("wrong-size", [1, 2]);
  const broken = skin("broken", [1, 2, 3]);
  broken.image.arrayBuffer = async () => { throw new Error("unreadable stored image"); };
  const matching = skin("healthy", [1, 2, 3]);
  assert.equal(await findMatchingLocalSkin([missing, wrongSize, broken, matching], image(1, 2, 3), "url-only"), matching);
  assert.equal(await findMatchingLocalSkin([missing, wrongSize, broken], image(1, 2, 3)), undefined);
  assert.equal(await findMatchingLocalSkin([], image(1, 2, 3)), undefined);
});

test("lookup does not mutate records or infer a different model from skin bytes", async () => {
  const stored = Object.freeze(skin("saved", [9, 8, 7], "classic"));
  const records = Object.freeze([stored]);
  const found = await findMatchingLocalSkin(records, image(9, 8, 7));
  assert.equal(found, stored);
  assert.equal(found.skinModel, "classic");
});
