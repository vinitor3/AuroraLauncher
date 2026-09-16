import assert from "node:assert/strict";
import test, { type TestContext } from "node:test";
import { McApiClient, rankPlayers } from "./skins/McApiClient.ts";
import { SkinService } from "./skins/SkinService.ts";
import { JsonHttpClient, SkinApiError } from "./skins/http.ts";
import { skinImageUrl, getSkinImageUrlFromProfile } from "./skins/models.ts";
import type { Env } from "./index.ts";
import { PlayerSkinsClient } from "../../desktop/src/lib/playerSkins.ts";

const uuid = "069a79f444e94726a5befca90e38aaf5";
const texture = "https://textures.minecraft.net/texture/" + "a".repeat(64);
const profile = { uuid, username: "Notch", skin_url: texture, is_slim: false,
  renders: { head: `https://mc-api.bisai.dev/v1/render/head/${uuid}`, body: `https://mc-api.bisai.dev/v1/render/body/${uuid}` } };
const json = (value: unknown) => Response.json(value);

test("mc-api usa header opcional, valida DTO e preserva metadados de busca", async () => {
  const client = new McApiClient(new JsonHttpClient("https://mc-api.bisai.dev", "test-only-key", 30, 3000, async (input, init) => {
    assert.equal(new Headers(init?.headers).get("X-API-Key"), "test-only-key");
    assert.equal(new URL(String(input)).searchParams.get("q"), "Not");
    return json({ players: [{ name: "Notch", uuid, current: true, additional: "metadata" }] });
  }));
  const players = await client.searchPlayers("Not");
  assert.equal(players[0].metadata.additional, "metadata");
  assert.equal(players[0].uuid, uuid);
  assert.deepEqual(await client.searchPlayers("No"), []);
});

test("perfil anônimo por UUID preserva modelo, render e last-known", async () => {
  const client = new McApiClient(new JsonHttpClient("https://mc-api.bisai.dev", "", 30, 3000, async (_, init) => {
    assert.equal(new Headers(init?.headers).has("X-API-Key"), false);
    return new Response(JSON.stringify(profile), { headers: { "Content-Type": "application/json", "X-Data-Origin": "last-known" } });
  }));
  const result = await client.getPlayerProfile(uuid);
  assert.equal(result.stale, true);
  assert.equal(result.model, "classic");
  assert.equal(getSkinImageUrlFromProfile(result), profile.renders.body);
  assert.equal(getSkinImageUrlFromProfile(result, "skin"), texture);
});

test("cache unifica buscas concorrentes e mantém perfil vencido durante falha", async () => {
  let clock = 1000000;
  let calls = 0;
  const service = new SkinService({}, () => clock, async () => {
    calls++;
    if (calls > 1) throw new Error("network");
    return json(profile);
  });
  const [a, b] = await Promise.all([service.getPlayerProfile("Notch"), service.getPlayerProfile("notch")]);
  assert.equal(a.uuid, b.uuid);
  assert.equal(calls, 1);
  clock += 300001;
  assert.equal((await service.getPlayerProfile("Notch")).stale, true);
  assert.equal(calls, 2);
});

test("fallback MCStat resolve perfil quando mc-api falha", async () => {
  const service = new SkinService({ mcstat_API_KEY: "test-mcstat" }, Date.now, async (input, init) => {
    const url = new URL(String(input));
    if (url.hostname === "mc-api.bisai.dev") return new Response(null, { status: 502 });
    assert.equal(url.pathname, "/api/v1/players/Notch");
    assert.equal(new Headers(init?.headers).get("X-API-Key"), "test-mcstat");
    return json({ success: true, data: { player: { mainNickname: "Notch", minecraftUuid: uuid, skinUrl: texture, skinModel: "slim" } } });
  });
  const result = await service.getPlayerProfile("Notch");
  assert.equal(result.source, "mcstat");
  assert.equal(result.model, "slim");
});

test("429 respeita Retry-After sem repetir a requisição", async () => {
  let calls = 0;
  const http = new JsonHttpClient("https://mc-api.bisai.dev", "", 30, 3000, async () => {
    calls++;
    return new Response(null, { status: 429, headers: { "Retry-After": "120" } });
  });
  await assert.rejects(http.get("/v1/profile/Notch"), (error) => error instanceof SkinApiError && error.status === 429 && error.retryAfter === 120);
  await assert.rejects(http.get("/v1/profile/Notch"));
  assert.equal(calls, 1);
});

test("rejeita UUID trocado, JSON inválido e URLs externas; remove chave de render", async () => {
  const client = new McApiClient(new JsonHttpClient("https://mc-api.bisai.dev", "", 30, 3000, async () => json(profile)));
  await assert.rejects(client.getPlayerProfile("b".repeat(32)), /inválido/);
  const bad = new JsonHttpClient("https://mc-api.bisai.dev", "", 30, 3000, async () => new Response("oops", { headers: { "Content-Type": "application/json" } }));
  await assert.rejects(bad.get("/v1/profile/Notch"), /consultar/);
  assert.equal(skinImageUrl("https://evil.invalid/skin.png"), undefined);
  assert.equal(skinImageUrl("http://textures.minecraft.net/texture/" + "a".repeat(64)), texture);
  assert.equal(skinImageUrl(profile.renders.head + "?key=never-expose"), profile.renders.head);
});

test("relevância põe nome exato antes de prefixos e mantém nome atual em empate", () => {
  const rows = ["Notch123", "Notch1", "Notch"].map((name) => ({ name, source: "mc-api" as const, metadata: { current: true } }));
  assert.deepEqual(rankPlayers(rows, "notch").map((row) => row.name), ["Notch", "Notch1", "Notch123"]);
});

// Independent QA: exercise the public worker routes (and desktop client when relevant),
// with a fresh worker isolate and all external fetches mocked. No secrets or real API calls.
const qaOrigin = "https://aurora-qa.test";
let qaSequence = 0;
type QaCall = { url: URL; init?: RequestInit };

async function routeHarness(t: TestContext, upstream: (call: QaCall) => Response | Promise<Response>,
  overrides: Partial<Env> = {}) {
  const env: Env = { CURSEFORGE_API_KEY: "qa-only", FIREBASE_PROJECT_ID: "qa-only", GEMINI_API_KEY: "qa-only", ...overrides };
  const calls: QaCall[] = [];
  const mockErrors: string[] = [];
  let worker: typeof import("./index.ts").default;
  t.mock.method(globalThis, "fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = new URL(input instanceof Request ? input.url : String(input));
    if (url.origin === qaOrigin) return worker.fetch(input instanceof Request ? input : new Request(url, init), env);
    const call = { url, init };
    calls.push(call);
    try { return await upstream(call); }
    catch (cause) {
      // worker.fetch catches upstream errors; do not let that conceal a failed mock assertion.
      if (!(init?.signal?.aborted && cause === init.signal.reason)) {
        mockErrors.push(`${url.pathname}: ${String(cause)}`);
      }
      throw cause;
    }
  });
  t.after(() => assert.deepEqual(mockErrors, [], "Unexpected upstream call or swallowed mock assertion"));
  worker = (await import(`./index.ts?players-qa=${++qaSequence}`)).default;
  return {
    calls,
    request: (path: string) => worker.fetch(new Request(new URL(path, qaOrigin)), env),
    client: new PlayerSkinsClient(qaOrigin),
  };
}

test("rotas QA: cliente desktop recebe hash e asset same-origin; TSV preserva o mesmo perfil", async (t) => {
  let png: Uint8Array;
  const h = await routeHarness(t, ({ url, init }) => {
    if (url.pathname === `/v1/profile/${uuid}`) {
      assert.equal(new Headers(init?.headers).get("X-API-Key"), "qa-mcapi-key");
      return json(profile);
    }
    assert.equal(url.href, texture);
    assert.equal(new Headers(init?.headers).get("X-API-Key"), null);
    return new Response(png, { headers: { "Content-Type": "image/png" } });
  }, { MC_API_KEY: "qa-mcapi-key" });
  png = new Uint8Array(await (await h.request("/v1/skins/catalog/solar-ninja.png")).arrayBuffer());
  const result = await h.client.getPlayerProfile(uuid);
  const asset = await h.request(result.skinUrl);
  assert.equal(asset.status, 200);
  const bytes = new Uint8Array(await asset.arrayBuffer());
  const digest = Buffer.from(await crypto.subtle.digest("SHA-256", bytes)).toString("hex");
  assert.equal(result.contentHash, digest);
  assert.equal(new URL(result.skinUrl).origin, qaOrigin);
  const catalogResponse = await h.request(`/v1/skins/profile/${uuid}?format=catalog`);
  const lines = (await catalogResponse.text()).trimEnd().split("\n");
  assert.equal(lines[0], "AURORA_SKINS_CATALOG_V1");
  const fields = lines[2].split("\t");
  assert.equal(fields.length, 7);
  assert.equal(fields[0], `player-${uuid}`);
  assert.equal(fields[4], digest);
  assert.equal(new URL(Buffer.from(fields[5], "base64url").toString(), qaOrigin).href, result.skinUrl);
  assert.equal(h.calls.length, 2, "Cached profile/asset must not spend more upstream calls");
});

test("rotas QA: 429 é compartilhado entre buscas, informa espera e se recupera após expirar", async (t) => {
  let now = Date.now();
  t.mock.method(Date, "now", () => now);
  let limited = true;
  const h = await routeHarness(t, () => limited
    ? new Response(null, { status: 429, headers: { "Retry-After": "60" } })
    : json({ players: [{ name: "Notch", uuid, current: true }] }));
  assert.equal((await h.request("/v1/skins/players?query=Not")).status, 429);
  const second = await h.request("/v1/skins/players?query=Notc");
  assert.equal(second.status, 429);
  assert.equal(second.headers.get("Retry-After"), "60");
  assert.equal(h.calls.length, 1);
  now += 60001;
  limited = false;
  const players = await h.client.autocompleteSkinNames("Notc");
  assert.equal(players[0].uuid, uuid);
  assert.equal(h.calls.length, 2);
});

test("QA-PLAYER-QUOTA: proxy de texturas deve respeitar SKIN_API_REQUESTS_PER_MINUTE", async (t) => {
  let png: Uint8Array;
  const h = await routeHarness(t, ({ url, init }) => {
    assert.equal(url.hostname, "mc-api.bisai.dev");
    assert.match(url.pathname, /^\/v1\/skin\/Quota[0-9]$/);
    assert.equal(new Headers(init?.headers).get("X-API-Key"), "qa-mcapi-key");
    return new Response(png, { headers: { "Content-Type": "image/png" } });
  }, { MC_API_KEY: "qa-mcapi-key", SKIN_API_REQUESTS_PER_MINUTE: "2" });
  png = new Uint8Array(await (await h.request("/v1/skins/catalog/solar-ninja.png")).arrayBuffer());
  const statuses: number[] = [];
  for (let index = 0; index < 3; index++) {
    const source = Buffer.from(`https://mc-api.bisai.dev/v1/skin/Quota${index}`).toString("base64url");
    statuses.push((await h.request(`/v1/skins/player-texture/${"0".repeat(64)}/${source}.png`)).status);
  }
  assert.deepEqual({ calls: h.calls.length, lastStatus: statuses[2] }, { calls: 2, lastStatus: 429 },
    "Requests with invented hashes must still respect the provider quota before downloading");
});

test("QA-PLAYER-ASSET: textura primária indisponível deve usar renders.skin_png válido", async (t) => {
  let png: Uint8Array;
  const alternate = `https://mc-api.bisai.dev/v1/skin/${uuid}`;
  const h = await routeHarness(t, ({ url }) => {
    if (url.pathname === `/v1/profile/${uuid}`) return json({ ...profile, renders: { ...profile.renders, skin_png: alternate } });
    if (url.href === texture) return new Response(null, { status: 404 });
    assert.equal(url.href, alternate);
    return new Response(png, { headers: { "Content-Type": "image/png" } });
  });
  png = new Uint8Array(await (await h.request("/v1/skins/catalog/solar-ninja.png")).arrayBuffer());
  const response = await h.request(`/v1/skins/profile/${uuid}`);
  assert.equal(response.status, 200, "An unavailable primary URL must not prevent trying the supplied alternate PNG");
  const result = await response.json() as { skinUrl: string };
  assert.equal((await h.request(result.skinUrl)).status, 200);
});

test("QA-PLAYER-TSV: preserva metadata útil em doze linhas de até 12 KiB e resposta abaixo de 256 KiB", async (t) => {
  const h = await routeHarness(t, () => json({ players: Array.from({ length: 12 }, (_, index) => ({
    // Oversized fields precede the important fields to verify selective preservation.
    additional: "x".repeat(11600), overflow: "z".repeat(5400),
    name: `Notch${index}`, uuid: (index + 1).toString(16).padStart(32, "0"),
    current: false, last_seen: "2026-09-01T00:00:00Z",
  })) }));
  const response = await h.request("/v1/skins/players?query=Not&format=tsv");
  assert.equal(response.status, 200);
  const text = await response.text();
  assert.ok(Buffer.byteLength(text) < 256 * 1024, "The full encoded TSV must fit the Java response limit");
  const lines = text.trimEnd().split("\n");
  assert.equal(lines[0], "AURORA_PLAYERS_V1");
  assert.equal(lines.length, 13);
  for (const line of lines.slice(1)) {
    const fields = line.split("\t");
    assert.equal(fields.length, 4);
    const bytes = Buffer.from(fields[3], "base64url");
    assert.ok(bytes.length <= 12 * 1024, `Decoded metadata is ${bytes.length} bytes`);
    const metadata = JSON.parse(bytes.toString());
    assert.equal(metadata.current, false);
    assert.equal(metadata.last_seen, "2026-09-01T00:00:00Z");
    assert.equal(metadata.name, fields[0]);
    assert.equal(metadata.uuid, fields[1]);
    assert.equal(metadata.additional, "x".repeat(11600));
    assert.equal(metadata.overflow, undefined);
    assert.equal(metadata.truncated, true);
  }
  assert.equal(h.calls.length, 1);
  t.diagnostic(`Encoded TSV size for twelve bounded suggestions: ${Buffer.byteLength(text)} bytes`);
});

test("QA-PLAYER-URL: origem acima de 1000 caracteres sem alternativa é rejeitada antes de baixar ou publicar", async (t) => {
  const media = "https://mcstat.org/media/skins/published/" + "a".repeat(1450) + ".png";
  // Accepted design decision: reject the oversized source instead of expanding the
  // asset route. No renders.skin_png fallback is present, so rejection is mandatory.
  assert.equal(skinImageUrl(media), undefined);
  const h = await routeHarness(t, ({ url }) => {
    assert.equal(url.pathname, `/v1/profile/${uuid}`, "An invalid source must never be downloaded");
    return json({ ...profile, skin_url: media, renders: {} });
  });
  const response = await h.request(`/v1/skins/profile/${uuid}`);
  assert.equal(response.status, 502);
  const result = await response.json() as Record<string, unknown>;
  assert.match(String(result.error), /perfil.*inválido/i);
  assert.equal(result.skinUrl, undefined);
  assert.equal(result.contentHash, undefined);
  assert.equal(h.calls.length, 1, "Only the profile JSON may be fetched; no image or published asset");
});

test("rotas QA: deadline global cancela hidratação antes de dez segundos", async (t) => {
  let png: Uint8Array;
  let aborted = 0;
  const h = await routeHarness(t, async ({ url, init }) => {
    const signal = init?.signal;
    assert.ok(signal, "Every list/image fetch must carry the shared budget");
    await new Promise<void>((resolve, reject) => {
      const onAbort = () => {
        clearTimeout(timer);
        aborted++;
        reject(signal.reason);
      };
      const timer = setTimeout(() => { signal.removeEventListener("abort", onAbort); resolve(); }, 2300);
      if (signal.aborted) onAbort();
      else signal.addEventListener("abort", onAbort, { once: true });
    });
    if (url.pathname === "/api/v1/skins") {
      const page = Number(url.searchParams.get("page"));
      return json({ success: true, data: {
        skins: Array.from({ length: 24 }, (_, index) => ({ name: `Budget${(page - 1) * 24 + index}`,
          pngPath: `https://mcstat.org/media/skins/published/budget-${(page - 1) * 24 + index}.png` })),
        pagination: { total: 48, totalPages: 2, page, limit: 24 },
      } });
    }
    assert.equal(url.hostname, "mcstat.org");
    return new Response(png, { headers: { "Content-Type": "image/png" } });
  }, { mcstat_API_KEY: "qa-mcstat-key" });
  png = new Uint8Array(await (await h.request("/v1/skins/catalog/solar-ninja.png")).arrayBuffer());
  const started = performance.now();
  const response = await h.request("/v1/skins/catalog?query=qa-budget&page=1&pageSize=13");
  const elapsed = performance.now() - started;
  assert.equal(response.status, 200);
  assert.equal(h.calls.length, 15, "Two list pages plus thirteen distinct image requests");
  assert.ok(aborted > 0, "The final wave must be interrupted by the global deadline");
  assert.ok(elapsed < 10000, `Catalog took ${Math.round(elapsed)} ms`);
  t.diagnostic(`Global cancellation after ${Math.round(elapsed)} ms; aborted image fetches: ${aborted}`);
});

test("rotas QA: JSON e PNG compartilham cota, cache e recuperação com chave ou em modo anônimo", async (t) => {
  let now = Date.now();
  t.mock.method(Date, "now", () => now);
  for (const key of ["", "qa-mcapi-key"]) {
    let png: Uint8Array;
    const source = `https://mc-api.bisai.dev/v1/skin/${uuid}`;
    const h = await routeHarness(t, ({ url, init }) => {
      assert.equal(new Headers(init?.headers).get("X-API-Key"), key || null);
      if (url.pathname === `/v1/profile/${uuid}`) return json({ ...profile, skin_url: source });
      if (url.pathname === "/v1/players/search") return json({ players: [{ name: "Notch", uuid, current: true }] });
      assert.equal(url.href, source, "A third distinct download must be blocked before fetch");
      return new Response(png, { headers: { "Content-Type": "image/png" } });
    }, { MC_API_KEY: key, SKIN_API_REQUESTS_PER_MINUTE: "2" });
    png = new Uint8Array(await (await h.request("/v1/skins/catalog/solar-ninja.png")).arrayBuffer());
    const response = await h.request(`/v1/skins/profile/${uuid}`);
    assert.equal(response.status, 200);
    const result = await response.json() as { skinUrl: string };
    assert.deepEqual(h.calls.map((call) => new Headers(call.init?.headers).get("Accept")),
      ["application/json", "image/png"]);
    const search = await h.request("/v1/skins/players?query=Not");
    assert.equal(search.status, 429, "One JSON plus one PNG exhaust a shared budget of two");
    assert.equal(search.headers.get("Retry-After"), "60");
    const uncached = Buffer.from("https://mc-api.bisai.dev/v1/skin/SharedQuota").toString("base64url");
    assert.equal((await h.request(`/v1/skins/player-texture/${"0".repeat(64)}/${uncached}.png`)).status, 429);
    assert.equal((await h.request(result.skinUrl)).status, 200, "A cached asset remains available without spending quota");
    assert.equal((await h.request(`/v1/skins/profile/${uuid}`)).status, 200);
    assert.equal(h.calls.length, 2);
    now += 60001;
    const recovered = await h.request("/v1/skins/players?query=Not");
    assert.equal(recovered.status, 200);
    assert.equal(h.calls.length, 3, "The next minute permits a new upstream request");
  }
});
