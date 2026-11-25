import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { build } from "esbuild";
import { convertV4MiniflareOptions, Miniflare } from "miniflare";
import { png } from "./skinPng.ts";

test("workerd serves the verified community page across isolates when MCStat is rate limited", async () => {
  const script = process.env.AURORA_WORKER_TEST_BUNDLE ? await readFile(process.env.AURORA_WORKER_TEST_BUNDLE, "utf8")
    : (await build({ entryPoints: [fileURLToPath(new URL("../src/index.ts", import.meta.url))],
      bundle: true, format: "esm", write: false })).outputFiles[0].text;
  const directory = await mkdtemp(join(tmpdir(), "aurora-skin-qa-"));
  const origin = "https://skin-qa.test";
  const page = origin + "/v1/skins/catalog?format=json&query=&page=0&pageSize=12";
  const snapshot = origin + "/_cache/community-skins-v1?query=&page=0&size=12";
  let status = 200;
  let calls = 0;
  function start() {
    const options = convertV4MiniflareOptions({ host: "127.0.0.1", port: 0, cf: false, modules: true,
      compatibilityDate: "2026-08-27", script,
      bindings: { MCSTAT_API_KEY: "test-only", FIREBASE_PROJECT_ID: "demo-skins" },
      outboundService: async (req) => {
        const url = new URL(req.url);
        assert.equal(url.hostname, "mcstat.org");
        if (url.pathname.endsWith(".png")) return new Response(png(1), { headers: { "Content-Type": "image/png" } });
        calls++;
        if (status !== 200) return new Response(null, { status, headers: { "Retry-After": "90" } });
        return Response.json({ success: true, data: {
          skins: [{ name: "Community QA", model: "slim", owner: { username: "QA" }, tags: [], pngPath: "https://mcstat.org/media/skins/published/qa.png" }],
          pagination: { page: 1, total: 1, totalPages: 1, limit: 24 },
        } });
      } });
    return new Miniflare({ ...options, resourcePersistencePath: directory, logRequests: false });
  }
  let runtime = start();
  try {
    const live = await runtime.dispatchFetch(page);
    assert.equal(live.status, 200);
    const expected = await live.json();
    assert.equal(expected.entries[0].name, "Community QA");
    assert.equal(expected.stale, undefined);
    const storage = await runtime.getCaches();
    const cached = await storage.default.match(snapshot);
    assert.ok(cached, "workerd stored the validated page");
    const headers = new Headers(cached.headers);
    headers.set("X-Aurora-Captured-At", String(Date.now() - 6 * 60_000));
    await storage.default.put(snapshot, new Response(await cached.text(), { headers }));
    await runtime.dispose();
    runtime = start();
    status = 429;
    assert.ok(await (await runtime.getCaches()).default.match(snapshot), "new isolate sees the previous public page");
    const saved = await (await runtime.dispatchFetch(page)).json();
    assert.deepEqual(saved.entries, expected.entries);
    assert.equal(saved.stale, true);
    const tab = await runtime.dispatchFetch(page.replace("format=json", "format=v1"));
    assert.match(await tab.text(), /^AURORA_SKINS_CATALOG_V1\npage\t0\tmore\t0\nmcstat-/);
    const missing = await runtime.dispatchFetch(page.replace("query=", "query=other"));
    assert.equal(missing.status, 429);
    assert.match((await missing.json()).error, /limite/i);
    assert.ok(Number(missing.headers.get("Retry-After")) > 0);
    assert.equal(calls, 2);
  } finally {
    await runtime.dispose();
    await rm(directory, { recursive: true, force: true });
  }
});
