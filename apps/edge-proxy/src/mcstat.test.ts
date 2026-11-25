import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { performance } from "node:perf_hooks";
import test, { afterEach, beforeEach, type TestContext } from "node:test";
import { png, chunk } from "../tests/skinPng.ts";
import type { Env } from "./index.ts";

// Run explicitly: node --test apps/edge-proxy/src/index.test.ts apps/edge-proxy/src/mcstat.test.ts
// QA regressions assert the desired behavior; failures are intentional until the owner fixes index.ts.
const env: Env = {
  CURSEFORGE_API_KEY: "qa-placeholder-curseforge",
  FIREBASE_PROJECT_ID: "qa-placeholder-project",

  MCSTAT_API_KEY: "qa-placeholder-mcstat",
};
const origin = "https://aurora.test";
const mediaBase = "https://mcstat.org/media/skins/published/";
const byteLimit = 256 * 1024;
let worker: typeof import("./index.ts").default;
let sequence = 0;
let unexpected: string[];
const originalFetch = globalThis.fetch;

beforeEach(async () => {
  unexpected = [];
  globalThis.fetch = async (input) => {
    unexpected.push(String(input));
    throw new Error("External network disabled by independent QA");
  };
  // A fresh module isolates private caches without adding test-only exports to production.
  worker = (await import(`./index.ts?mcstat-qa=${++sequence}`)).default;
});

afterEach(() => {
  globalThis.fetch = originalFetch;
  assert.deepEqual(unexpected, [], "Unexpected upstream request or swallowed mock assertion");
});

type Call = { url: URL; init?: RequestInit };
type RemoteSkin = {
  name: string;
  model: string;
  pngPath: string;
  owner: { username: string };
  tags: string[];
};

function mockFetch(handler: (call: Call) => Response | Promise<Response>): Call[] {
  const calls: Call[] = [];
  globalThis.fetch = async (input, init) => {
    const call = { url: new URL(input instanceof Request ? input.url : String(input)), init };
    calls.push(call);
    try {
      return await handler(call);
    } catch (cause) {
      // Production catches upstream failures, so check mock errors again outside worker.fetch.
      unexpected.push(`${call.url}: ${String(cause)}`);
      throw cause;
    }
  };
  return calls;
}

function hash(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

function skin(index: number): RemoteSkin {
  return {
    name: `QA remote ${index}`, model: "classic", pngPath: `${mediaBase}${index}.png`,
    owner: { username: "QA" }, tags: ["qa"],
  };
}

function list(skins: unknown[], total = skins.length, page = 1): Response {
  return Response.json({ success: true, data: {
    skins, pagination: { page, limit: 24, total, totalPages: Math.ceil(total / 24) },
  } });
}

function imageResponse(bytes: Uint8Array): Response {
  return new Response(bytes, { headers: { "Content-Type": "image/png" } });
}

function mockCatalog(skins: RemoteSkin[]): Call[] {
  return mockFetch(({ url }) => {
    if (url.pathname === "/api/v1/skins") {
      const page = Number(url.searchParams.get("page"));
      return list(skins.slice((page - 1) * 24, page * 24), skins.length, page);
    }
    const index = skins.findIndex((entry) => entry.pngPath === url.toString());
    assert.notEqual(index, -1);
    return imageResponse(png(index + 1));
  });
}

async function catalog(query: string, page = 0, pageSize = 12) {
  const url = new URL("/v1/skins/catalog", origin);
  url.search = new URLSearchParams({ query, page: String(page), pageSize: String(pageSize) }).toString();
  const response = await worker.fetch(new Request(url), env);
  assert.equal(response.status, 200);
  const text = await response.text();
  const lines = text.trimEnd().split("\n");
  assert.equal(lines[0], "AURORA_SKINS_CATALOG_V1");
  assert.match(lines[1], new RegExp(`^page\\t${page}\\tmore\\t[01]$`));
  const rows = lines.slice(2).map((line) => line.split("\t"));
  for (const row of rows) assert.equal(row.length, 7);
  return { response, text, rows, more: lines[1].endsWith("\t1") };
}

function decoded(value: string): string {
  return Buffer.from(value, "base64url").toString("utf8");
}

function assetUrl(bytes: Uint8Array, source: string): string {
  return `${origin}/v1/skins/catalog/mcstat/${hash(bytes)}/${Buffer.from(source).toString("base64url")}.png`;
}

test("pagination: crosses upstream page boundaries for sizes 7, 12 and 24", async () => {
  const skins = Array.from({ length: 50 }, (_, index) => skin(index));
  const calls = mockCatalog(skins);
  for (const pageSize of [7, 12, 24]) {
    const names: string[] = [];
    const ids: string[] = [];
    for (let page = 0; page < Math.ceil(50 / pageSize); page++) {
      const result = await catalog("", page, pageSize);
      assert.equal(result.rows.length, Math.min(pageSize, 50 - page * pageSize));
      assert.equal(result.more, (page + 1) * pageSize < 50);
      names.push(...result.rows.filter((row) => row[0].startsWith("mcstat-")).map((row) => decoded(row[1])));
      ids.push(...result.rows.map((row) => row[0]));
    }
    assert.equal(new Set(ids).size, 50);
    assert.deepEqual(names, skins.map((entry) => entry.name));
  }
  assert.deepEqual(calls.filter((call) => call.url.pathname === "/api/v1/skins")
    .map((call) => call.url.searchParams.get("page")), ["1", "2", "3"]);
});

test("QA-PAG-01: filtering an invalid upstream record must not repeat the next page's first skin", async () => {
  const skins = Array.from({ length: 48 }, (_, index) => skin(index));
  skins[0].pngPath = "https://untrusted.invalid/0.png";
  mockCatalog(skins);
  const names: string[] = [];
  for (let page = 0; page < 4; page++) {
    const result = await catalog("qa-filtered", page, 12);
    assert.equal(result.more, page < 3);
    names.push(...result.rows.map((row) => decoded(row[1])));
  }
  assert.deepEqual(names, skins.slice(1).map((entry) => entry.name),
    "Keep raw upstream offsets stable: filtering page 1 currently repeats QA remote 24");
});

test("cache: concurrent requests share both in-flight list and image downloads", async () => {
  let release!: () => void;
  const gate = new Promise<void>((resolve) => { release = resolve; });
  const calls = mockFetch(async ({ url }) => {
    await gate;
    return url.pathname === "/api/v1/skins" ? list([skin(1)]) : imageResponse(png(1));
  });
  const pending = Array.from({ length: 16 }, () => catalog("qa-concurrent"));
  release();
  const results = await Promise.all(pending);
  assert.ok(results.every((result) => result.rows.length === 1));
  assert.equal(new Set(results.map((result) => result.text)).size, 1);
  assert.equal(calls.length, 2);
});

test("cache: list TTL is five minutes and image TTL is one hour", async (t) => {
  let now = Date.now();
  t.mock.method(Date, "now", () => now);
  const calls = mockCatalog([skin(1)]);
  await catalog("qa-ttl");
  now += 300_000 - 1;
  await catalog("qa-ttl");
  assert.equal(calls.length, 2);
  now += 2;
  await catalog("qa-ttl");
  assert.equal(calls.length, 3);
  now += 3_600_000;
  await catalog("qa-ttl");
  assert.equal(calls.filter((call) => call.url.pathname === "/api/v1/skins").length, 3);
  assert.equal(calls.filter((call) => call.url.pathname.endsWith(".png")).length, 2);
});

test("QA-QUOTA-01: HTTP 429 must suppress repeated calls during Retry-After", async () => {
  const calls = mockFetch(() => new Response(null, { status: 429, headers: { "Retry-After": "60" } }));
  for (let attempt = 0; attempt < 4; attempt++) {
    assert.equal((await worker.fetch(new Request(origin + "/v1/skins/catalog?query=qa-rate-limit"), env)).status, 503);
  }
  assert.equal(calls.length, 1, "Four immediate requests currently consume four upstream calls after 429");
});

test("image integrity: expired cached URL with new content rejects an old hash", async (t) => {
  let now = Date.now();
  t.mock.method(Date, "now", () => now);
  let bytes = png(1);
  const calls = mockFetch(({ url }) => url.pathname === "/api/v1/skins"
    ? list([skin(1)]) : imageResponse(bytes));
  const first = await catalog("qa-replaced-image");
  const source = decoded(first.rows[0][5]);
  now += 3_600_001;
  bytes = png(2);
  const asset = await worker.fetch(new Request(new URL(source, origin)), env);
  assert.equal(asset.status, 409);
  assert.doesNotMatch(await asset.text(), /qa-placeholder/);
  const refreshed = await catalog("qa-replaced-image");
  assert.equal(refreshed.rows[0][4], hash(bytes));
  assert.notEqual(refreshed.rows[0][4], first.rows[0][4]);
  assert.equal(calls.filter((call) => call.url.pathname.endsWith(".png")).length, 2);
});

test("QA-PNG-01: a 24-byte PNG header must not become a catalog entry or downloadable asset", async () => {
  const truncated = png(1).subarray(0, 24);
  const calls = mockFetch(({ url }) => url.pathname === "/api/v1/skins"
    ? list([skin(1)]) : imageResponse(truncated));
  const result = await catalog("qa-truncated-png");
  const asset = await worker.fetch(new Request(assetUrl(truncated, skin(1).pngPath)), env);
  assert.ok(calls.length >= 2);
  assert.deepEqual({ catalogEntries: result.rows.length, assetStatus: asset.status },
    { catalogEntries: 0, assetStatus: 502 }, "IHDR dimensions alone do not prove PNG integrity");
});

test("QA-PNG-02: an invalid IDAT stream with a valid chunk CRC must not be published", async () => {
  const valid = png(1);
  const broken = Buffer.concat([
    valid.subarray(0, 33), chunk("IDAT", Buffer.from("not-a-zlib-stream")), chunk("IEND", Buffer.alloc(0)),
  ]);
  mockFetch(({ url }) => url.pathname === "/api/v1/skins" ? list([skin(1)]) : imageResponse(broken));
  assert.equal((await catalog("qa-broken-idat")).rows.length, 0);
});

function oversizedResponse(contentType: string, declared: boolean, onCancel: () => void): Response {
  let part = 0;
  return new Response(new ReadableStream<Uint8Array>({
    pull(controller) {
      // Leave data unread after the limit so cancellation is observable (not an already-closed stream).
      if (part++ < 8) controller.enqueue(new Uint8Array(128 * 1024));
      else controller.close();
    },
    cancel() { onCancel(); },
  }), { headers: {
    "Content-Type": contentType,
    ...(declared ? { "Content-Length": String(byteLimit + 1) } : {}),
  } });
}

for (const declared of [true, false]) {
  test(`image bounds: cancels oversized ${declared ? "declared" : "chunked"} body`, async () => {
    let cancelled = false;
    mockFetch(() => oversizedResponse("image/png", declared, () => { cancelled = true; }));
    const result = await worker.fetch(new Request(assetUrl(png(1), skin(1).pngPath)), env);
    assert.equal(result.status, 502);
    assert.equal(cancelled, true);
  });
  test(`list bounds: cancels oversized ${declared ? "declared" : "chunked"} JSON before hydration`, async () => {
    let cancelled = false;
    const calls = mockFetch(() => oversizedResponse("application/json", declared, () => { cancelled = true; }));
    const result = await worker.fetch(new Request(origin + "/v1/skins/catalog?query=qa-large-json"), env);
    assert.equal(result.status, 503);
    assert.equal(cancelled, true);
    assert.equal(calls.length, 1);
  });
}

test("image validation: redirects, HTML, wrong signature and unsupported dimensions are rejected", async () => {
  const responses = [
    new Response(null, { status: 302, headers: { Location: "https://untrusted.invalid/skin.png" } }),
    new Response("<html></html>", { headers: { "Content-Type": "text/html" } }),
    imageResponse(new Uint8Array(64)),
    imageResponse(png(1, 32, 32)),
  ];
  const calls = mockFetch(() => responses.shift()!);
  for (let index = 0; index < 4; index++) {
    const result = await worker.fetch(new Request(assetUrl(png(index), skin(index).pngPath)), env);
    assert.equal(result.status, 502);
  }
  assert.equal(calls.length, 4);
  assert.ok(calls.every((call) => call.init?.redirect === "manual"));
});

test("Java V1 contract: bounded Unicode metadata and a long media URL fit all decoded field limits", async () => {
  const remote = skin(1);
  remote.name = " 🐉\t龍\n".repeat(60);
  remote.owner.username = "𐍈\r\n".repeat(80);
  remote.tags = Array.from({ length: 30 }, (_, index) => `${index},${"火".repeat(35)}\t\n`);
  remote.pngPath = mediaBase + "a".repeat(1000 - mediaBase.length - 4) + ".png";
  mockFetch(({ url }) => url.pathname === "/api/v1/skins"
    ? list([remote]) : imageResponse(png(1, 64, 32)));
  const result = await catalog("qa-java-contract");
  assert.equal(result.rows.length, 1);
  assert.ok(Buffer.byteLength(result.text) < byteLimit);
  const fields = result.rows[0];
  assert.match(fields[0], /^[a-z0-9_-]{1,64}$/);
  assert.match(fields[4], /^[0-9a-f]{64}$/);
  assert.equal(fields[3], "classic");
  // Limits taken from AuroraCatalogProvider.parse/decode (actual Java consumer).
  for (const [index, limit] of [[1, 96], [2, 96], [5, 2048], [6, 512]]) {
    assert.match(fields[index], /^[A-Za-z0-9_-]+$/);
    const bytes = Buffer.from(fields[index], "base64url");
    assert.ok(bytes.length <= limit, `field ${index} exceeds the Java byte limit`);
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    assert.doesNotMatch(text, /[\r\n\t]/);
  }
  const source = new URL(decoded(fields[5]), origin);
  assert.equal(source.origin, origin);
  const downloaded = await worker.fetch(new Request(source), env);
  assert.equal(downloaded.status, 200);
  assert.equal(hash(new Uint8Array(await downloaded.arrayBuffer())), fields[4]);
});

test("Java V1 contract: deduplication does not exceed 24 entries or emit duplicate IDs within a page", async () => {
  const skins = Array.from({ length: 24 }, (_, index) => skin(index));
  mockFetch(({ url }) => url.pathname === "/api/v1/skins" ? list(skins) : imageResponse(png(1)));
  const result = await catalog("qa-duplicates", 0, 24);
  assert.equal(result.rows.length, 1);
  assert.equal(result.more, false);
});

test("QA-JAVA-01: two-page hydration fits the agreed ten-second worker budget", async (t: TestContext) => {
  const skins = Array.from({ length: 48 }, (_, index) => skin(index));
  const calls = mockFetch(async ({ url }) => {
    const isList = url.pathname === "/api/v1/skins";
    // This ~5 s critical path is valid under the agreed 10 s worker / 12 s Java budgets.
    await new Promise((resolve) => setTimeout(resolve, isList ? 500 : 2000));
    if (isList) {
      const page = Number(url.searchParams.get("page"));
      return list(skins.slice((page - 1) * 24, page * 24), skins.length, page);
    }
    const index = skins.findIndex((entry) => entry.pngPath === url.toString());
    assert.notEqual(index, -1);
    return imageResponse(png(index + 1));
  });
  const started = performance.now();
  const result = await catalog("qa-java-latency", 1, 13);
  const elapsed = performance.now() - started;
  assert.equal(result.rows.length, 13);
  assert.equal(calls.length, 15);
  t.diagnostic(`worker.fetch took ${Math.round(elapsed)} ms; agreed worker budget is 10000 ms`);
  assert.ok(elapsed < 10_000, `Healthy upstream calls took ${Math.round(elapsed)} ms before response headers`);
});
