// Optional integration check. Use a Firebase ID token for a dedicated test
// account. Uploading a new skin exercises the real appearance storage.
import assert from "node:assert/strict";
import { deflateSync } from "node:zlib";

const token = process.env.AURORA_SMOKE_FIREBASE_ID_TOKEN;
if (!token) throw new Error("Forneça AURORA_SMOKE_FIREBASE_ID_TOKEN de uma conta de teste.");
// Never forward a test identity to an endpoint selected through untrusted data.
const endpoint = "https://aurora-api.aurora-edge-proxy.workers.dev/v1/appearance";
const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

function chunk(type, data) {
  const payload = Buffer.concat([Buffer.from(type, "ascii"), data]);
  let crc = 0xffffffff;
  for (const byte of payload) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  const length = Buffer.alloc(4);
  const checksum = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  checksum.writeUInt32BE((crc ^ 0xffffffff) >>> 0);
  return Buffer.concat([length, payload, checksum]);
}

const header = Buffer.from([0, 0, 0, 64, 0, 0, 0, 64, 8, 6, 0, 0, 0]);
const pixels = Buffer.alloc(64 * 4 + 1);
for (let position = 1; position < pixels.length; position += 4) pixels.writeUInt32BE(0x4c5fd7ff, position);
const png = Buffer.concat([signature, chunk("IHDR", header), chunk("IDAT", deflateSync(Buffer.concat(Array(64).fill(pixels)))), chunk("IEND", Buffer.alloc(0))]);
const form = new FormData();
form.set("kind", "skin");
form.set("file", new Blob([png], { type: "image/png" }), "aurora-smoke.png");
const upload = await fetch(endpoint, {
  method: "POST", redirect: "error", headers: { Authorization: "Bearer " + token },
  body: form, signal: AbortSignal.timeout(30000),
});
assert.ok(upload.ok, "Falha de upload HTTP " + upload.status);
const { url } = await upload.json();
assert.equal(new URL(url).protocol, "https:");
const stored = await fetch(url, { signal: AbortSignal.timeout(30000) });
assert.ok(stored.ok, "Falha ao recuperar a imagem HTTP " + stored.status);
assert.match(stored.headers.get("content-type") ?? "", /^image\/png/i);
assert.deepEqual(Buffer.from(await stored.arrayBuffer()), png);
console.log("PASS · upload autenticado e leitura de PNG público");
