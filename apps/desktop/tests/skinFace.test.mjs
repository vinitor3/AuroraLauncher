import assert from "node:assert/strict";
import test from "node:test";
import { drawSkinFace, hasVisibleLegacyHat } from "../src/lib/skinFace.ts";

test("opaque legacy hat padding never covers the base face", (t) => {
  const pixels = new Uint8ClampedArray(32 * 32 * 4).fill(255);
  const previous = globalThis.document;
  globalThis.document = { createElement: () => ({ getContext: () => ({ drawImage() {}, getImageData: () => ({ data: pixels }) }) }) };
  t.after(() => { if (previous) globalThis.document = previous; else delete globalThis.document; });
  const draws = [];
  const context = { canvas: { width: 64, height: 64 }, clearRect() {}, drawImage: (...args) => draws.push(args) };
  drawSkinFace(context, { naturalHeight: 32 });
  assert.equal(draws.length, 1);
  assert.equal(draws[0][1], 8, "The visible layer is the base face, not the black hat padding");
  assert.equal(context.imageSmoothingEnabled, false);
  pixels[3] = 0;
  draws.length = 0;
  drawSkinFace(context, { naturalHeight: 32 });
  assert.equal(draws.length, 2, "A real legacy hat with transparency remains visible");
  assert.equal(draws[1][1], 40);
});

test("modern skin hats remain intact and legacy alpha follows Minecraft's threshold", () => {
  let draws = 0;
  drawSkinFace({ canvas: { width: 8, height: 8 }, clearRect() {}, drawImage: () => { draws++; } }, { naturalHeight: 64 });
  assert.equal(draws, 2);
  assert.equal(hasVisibleLegacyHat(new Uint8ClampedArray([0, 0, 0, 255, 0, 0, 0, 128])), false);
  assert.equal(hasVisibleLegacyHat(new Uint8ClampedArray([0, 0, 0, 255, 0, 0, 0, 127])), true);
});

test("a legacy image without CORS still renders its base face", (t) => {
  const previous = globalThis.document;
  globalThis.document = { createElement: () => ({ getContext: () => ({ drawImage() {}, getImageData: () => { throw new Error("tainted canvas"); } }) }) };
  t.after(() => { if (previous) globalThis.document = previous; else delete globalThis.document; });
  let draws = 0;
  drawSkinFace({ canvas: { width: 8, height: 8 }, clearRect() {}, drawImage: () => { draws++; } }, { naturalHeight: 32 });
  assert.equal(draws, 1);
});
