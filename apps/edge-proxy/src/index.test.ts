import assert from "node:assert/strict";
import test, { afterEach } from "node:test";

import worker from "./index.ts";

const apiKey = "test-curseforge-key";
const mcstatKey = "test-mcstat-key";
const env = {
  CURSEFORGE_API_KEY: apiKey,
  FIREBASE_PROJECT_ID: "test-project",
  GEMINI_API_KEY: "test-gemini-key",
} as never;
const envWithMcstat = { ...env, mcstat_API_KEY: mcstatKey } as never;
const originalFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = originalFetch;
});

function base64Url(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString("base64url");
}

async function downloadTicket(
  modId: number,
  fileId: number,
  expiresAt = Date.now() + 60_000,
  downloadUrl?: string,
) {
  const payload = base64Url(new TextEncoder().encode(JSON.stringify({
    modId,
    fileId,
    ...(downloadUrl ? { downloadUrl } : {}),
    expiresAt,
  })));
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(`aurora-curseforge-download\0${apiKey}`),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const signature = base64Url(new Uint8Array(
    await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(payload)),
  ));
  return `${payload}.${signature}`;
}

function downloadRequest(ticket: string, headers?: HeadersInit) {
  return new Request(`https://aurora.test/v1/curseforge/download?ticket=${encodeURIComponent(ticket)}`, { headers });
}

function jsonResponse(body: unknown, status = 200, headers?: HeadersInit) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json", ...Object.fromEntries(new Headers(headers)) },
  });
}

test("publica catálogo de skins pesquisável com PNG íntegro", async () => {
  const catalog = await worker.fetch(new Request(
    "https://aurora.test/v1/skins/catalog?query=naruto&page=0&pageSize=12",
  ), env);
  assert.equal(catalog.status, 200);
  assert.match(catalog.headers.get("Content-Type") ?? "", /text\/tab-separated-values/);

  const lines = (await catalog.text()).trimEnd().split("\n");
  assert.equal(lines[0], "AURORA_SKINS_CATALOG_V1");
  assert.equal(lines[1], "page\t0\tmore\t0");
  assert.equal(lines.length, 3);
  const fields = lines[2].split("\t");
  assert.equal(fields.length, 7);
  assert.equal(fields[0], "solar-ninja");
  assert.equal(Buffer.from(fields[1], "base64url").toString("utf8"), "Ninja Solar");

  const assetPath = Buffer.from(fields[5], "base64url").toString("utf8");
  const asset = await worker.fetch(new Request(`https://aurora.test${assetPath}`), env);
  assert.equal(asset.status, 200);
  assert.equal(asset.headers.get("Content-Type"), "image/png");
  const png = new Uint8Array(await asset.arrayBuffer());
  assert.deepEqual(Array.from(png.slice(0, 8)), [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const digest = Buffer.from(await crypto.subtle.digest("SHA-256", png)).toString("hex");
  assert.equal(digest, fields[4]);
});

test("agrega skins do MCStat sem expor a chave e entrega PNG verificado", async () => {
  const fixtureResponse = await worker.fetch(new Request(
    "https://aurora.test/v1/skins/catalog/solar-ninja.png",
  ), env);
  const fixture = new Uint8Array(await fixtureResponse.arrayBuffer());
  const calls: Array<{ url: string; headers: Headers }> = [];
  globalThis.fetch = async (input, init) => {
    const url = new URL(String(input));
    const headers = new Headers(init?.headers);
    calls.push({ url: url.toString(), headers });
    if (url.origin === "https://mcstat.org" && url.pathname === "/api/v1/skins") {
      assert.equal(headers.get("X-API-Key"), mcstatKey);
      assert.equal(url.searchParams.get("page"), "1");
      assert.equal(url.searchParams.get("limit"), "24");
      assert.equal(url.searchParams.get("sort"), "popular");
      assert.equal(url.searchParams.get("search"), "community-knight");
      return jsonResponse({
        success: true,
        data: {
          skins: [{
            id: "skin-test-id",
            slug: "community-knight",
            name: "Community Knight",
            model: "slim",
            tags: ["medieval", "armor"],
            pngPath: "https://mcstat.org/media/skins/published/community-knight.png",
            owner: { username: "Builder" },
          }],
          pagination: { page: 1, limit: 24, total: 1, totalPages: 1 },
        },
      });
    }
    if (url.toString() === "https://mcstat.org/media/skins/published/community-knight.png") {
      assert.equal(headers.get("X-API-Key"), null);
      return new Response(fixture, {
        status: 200,
        headers: { "Content-Type": "image/png", "Content-Length": String(fixture.length) },
      });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const catalog = await worker.fetch(new Request(
    "https://aurora.test/v1/skins/catalog?query=community-knight&page=0&pageSize=12",
  ), envWithMcstat);
  assert.equal(catalog.status, 200);
  assert.equal(catalog.headers.get("X-Aurora-Catalog-Sources"), "aurora, mcstat");
  const payload = await catalog.text();
  assert.doesNotMatch(payload, new RegExp(mcstatKey));
  const lines = payload.trimEnd().split("\n");
  assert.equal(lines.length, 3);
  const fields = lines[2].split("\t");
  assert.match(fields[0], /^mcstat-[0-9a-f]{32}$/);
  assert.equal(Buffer.from(fields[1], "base64url").toString("utf8"), "Community Knight");
  assert.equal(Buffer.from(fields[2], "base64url").toString("utf8"), "Builder · MCStat");
  assert.equal(fields[3], "slim");
  assert.match(fields[4], /^[0-9a-f]{64}$/);
  const assetPath = Buffer.from(fields[5], "base64url").toString("utf8");
  assert.match(assetPath, /^\/v1\/skins\/catalog\/mcstat\/[0-9a-f]{64}\//);
  assert.doesNotMatch(assetPath, new RegExp(mcstatKey));

  const asset = await worker.fetch(new Request(`https://aurora.test${assetPath}`), envWithMcstat);
  assert.equal(asset.status, 200);
  assert.equal(asset.headers.get("Content-Type"), "image/png");
  const downloaded = new Uint8Array(await asset.arrayBuffer());
  const digest = Buffer.from(await crypto.subtle.digest("SHA-256", downloaded)).toString("hex");
  assert.equal(digest, fields[4]);
  assert.equal(calls.length, 2);
});

test("ignora origem MCStat fora da allowlist e mantém o catálogo seguro", async () => {
  let calls = 0;
  globalThis.fetch = async (input) => {
    const url = new URL(String(input));
    calls += 1;
    assert.equal(url.pathname, "/api/v1/skins");
    return jsonResponse({
      success: true,
      data: {
        skins: [{
          name: "Unsafe Skin",
          model: "classic",
          tags: [],
          pngPath: "https://example.invalid/skin.png",
        }],
        pagination: { page: 1, limit: 24, total: 1, totalPages: 1 },
      },
    });
  };

  const catalog = await worker.fetch(new Request(
    "https://aurora.test/v1/skins/catalog?query=unsafe-source&page=0&pageSize=12",
  ), envWithMcstat);
  assert.equal(catalog.status, 200);
  assert.equal((await catalog.text()).trimEnd().split("\n").length, 2);
  const encoded = base64Url(new TextEncoder().encode("https://example.invalid/skin.png"));
  const asset = await worker.fetch(new Request(
    `https://aurora.test/v1/skins/catalog/mcstat/${"0".repeat(64)}/${encoded}.png`,
  ), envWithMcstat);
  assert.equal(asset.status, 403);
  assert.equal(calls, 1);
});

test("mantém as skins Aurora quando a API MCStat está indisponível", async () => {
  globalThis.fetch = async () => jsonResponse({ error: "rate limited" }, 429);
  const catalog = await worker.fetch(new Request(
    "https://aurora.test/v1/skins/catalog?query=naruto&page=0&pageSize=12",
  ), envWithMcstat);
  assert.equal(catalog.status, 200);
  assert.equal(catalog.headers.get("X-Aurora-Catalog-Sources"), "aurora");
  const lines = (await catalog.text()).trimEnd().split("\n");
  assert.equal(lines.length, 3);
  assert.equal(lines[2].split("\t")[0], "solar-ninja");
});

async function firebaseIdentity(userId: string) {
  const keyPair = await crypto.subtle.generateKey(
    {
      name: "RSASSA-PKCS1-v1_5",
      modulusLength: 2_048,
      publicExponent: new Uint8Array([1, 0, 1]),
      hash: "SHA-256",
    },
    true,
    ["sign", "verify"],
  );
  const publicJwk = await crypto.subtle.exportKey("jwk", keyPair.publicKey);
  Object.assign(publicJwk, { alg: "RS256", kid: "test-key", use: "sig" });
  const now = Math.floor(Date.now() / 1_000);
  const header = base64Url(new TextEncoder().encode(JSON.stringify({ alg: "RS256", kid: "test-key" })));
  const claims = base64Url(new TextEncoder().encode(JSON.stringify({
    aud: "test-project",
    iss: "https://securetoken.google.com/test-project",
    sub: userId,
    user_id: userId,
    exp: now + 300,
    iat: now,
    auth_time: now,
  })));
  const signature = base64Url(new Uint8Array(await crypto.subtle.sign(
    "RSASSA-PKCS1-v1_5",
    keyPair.privateKey,
    new TextEncoder().encode(`${header}.${claims}`),
  )));
  return { publicJwk, token: `${header}.${claims}.${signature}` };
}

test("rejeita tickets de download inválidos e expirados", async () => {
  const invalid = await worker.fetch(downloadRequest("invalid"), env);
  assert.equal(invalid.status, 403);

  const expiredTicket = await downloadTicket(11, 12, Date.now() - 1_000);
  const expired = await worker.fetch(downloadRequest(expiredTicket), env);
  assert.equal(expired.status, 410);

  const unsafeUrlTicket = await downloadTicket(
    13,
    14,
    Date.now() + 60_000,
    "https://edge.forgecdn.net:444/file.jar",
  );
  const unsafeUrl = await worker.fetch(downloadRequest(unsafeUrlTicket), env);
  assert.equal(unsafeUrl.status, 403);

  const validTicket = await downloadTicket(
    15,
    16,
    Date.now() + 60_000,
    "https://edge.forgecdn.net/file.jar",
  );
  const [validPayload, validSignature] = validTicket.split(".");
  const tamperedPayload = JSON.parse(Buffer.from(validPayload, "base64url").toString("utf8"));
  tamperedPayload.downloadUrl = "https://example.invalid/file.jar";
  const tamperedTicket = `${base64Url(new TextEncoder().encode(JSON.stringify(tamperedPayload)))}.${validSignature}`;
  const tampered = await worker.fetch(downloadRequest(tamperedTicket), env);
  assert.equal(tampered.status, 403);
});

test("não entrega downloadUrl fora da allowlist do CurseForge", async () => {
  const calls: string[] = [];
  globalThis.fetch = async (input) => {
    const url = String(input);
    calls.push(url);
    if (url.endsWith("/mods/21/files/22")) {
      return jsonResponse({ data: { id: 22, modId: 21, downloadUrl: "https://example.invalid/file.jar" } });
    }
    if (url.endsWith("/mods/21/files/22/download-url")) {
      return jsonResponse({ data: "https://example.invalid/file.jar" });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(downloadRequest(await downloadTicket(21, 22)), env);
  assert.equal(response.status, 409);
  assert.equal(calls.length, 2);
  assert.ok(calls.every((url) => url.startsWith("https://api.curseforge.com/")));
});

test("converte JSON inválido do upstream em erro 503 sanitizado", async () => {
  globalThis.fetch = async () => new Response("not-json", { status: 200 });
  const response = await worker.fetch(downloadRequest(await downloadTicket(23, 24)), env);
  assert.equal(response.status, 503);
  const body = await response.text();
  assert.match(body, /resposta inválida/i);
  assert.doesNotMatch(body, /unexpected|json/i);
});

test("bloqueia redirecionamento do CDN para outro domínio", async () => {
  const calls: string[] = [];
  globalThis.fetch = async (input) => {
    const url = String(input);
    calls.push(url);
    if (url === "https://edge.forgecdn.net/files/32.jar") {
      return new Response(null, { status: 302, headers: { Location: "https://example.invalid/file.jar" } });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(downloadRequest(await downloadTicket(
    31,
    32,
    Date.now() + 60_000,
    "https://edge.forgecdn.net/files/32.jar",
  )), env);
  assert.equal(response.status, 502);
  assert.equal(calls.length, 1);
});

test("preserva 416 e Content-Range para o downloader reiniciar o arquivo parcial", async () => {
  let calls = 0;
  globalThis.fetch = async (input, init) => {
    const url = String(input);
    calls += 1;
    if (url === "https://edge.forgecdn.net/files/42.jar") {
      assert.equal(new Headers(init?.headers).get("x-api-key"), apiKey);
      assert.equal(new Headers(init?.headers).get("Range"), "bytes=99-");
      return new Response(null, { status: 416, headers: { "Content-Range": "bytes */99" } });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(
    downloadRequest(await downloadTicket(
      41,
      42,
      Date.now() + 60_000,
      "https://edge.forgecdn.net/files/42.jar",
    ), { Range: "bytes=99-" }),
    env,
  );
  assert.equal(response.status, 416);
  assert.equal(response.headers.get("Content-Range"), "bytes */99");
  assert.equal(calls, 1);
});

test("preserva a classificação 429 depois do retry final", async () => {
  let calls = 0;
  globalThis.fetch = async () => {
    calls += 1;
    return jsonResponse({ message: "rate limited" }, 429);
  };

  const response = await worker.fetch(new Request("https://aurora.test/health/curseforge"), env);
  assert.equal(response.status, 429);
  assert.equal(calls, 2);
  assert.match(await response.text(), /limite temporário/i);
});

test("normaliza e agrupa categorias reais do CurseForge", async () => {
  const { publicJwk, token } = await firebaseIdentity("category-test-user");

  globalThis.fetch = async (input) => {
    const url = String(input);
    if (url.includes("securetoken@system.gserviceaccount.com")) {
      return jsonResponse({ keys: [publicJwk] });
    }
    if (url.includes("/v1/categories?")) {
      return jsonResponse({
        data: [
          { id: 6, name: "Mods", isClass: true },
          { id: 100, name: "Gameplay", slug: "gameplay", displayIndex: 2 },
          { id: 101, name: "Adventure", slug: "adventure", parentCategoryId: 100, displayIndex: 1 },
        ],
      });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(new Request("https://aurora.test/v1/curseforge", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ action: "categories", contentType: "mod" }),
  }), env);
  assert.equal(response.status, 200);
  const payload = await response.json() as { data: Array<{ id: number; group: string; slug: string }> };
  assert.equal(payload.data.some((category) => category.id === 6), false);
  assert.deepEqual(
    payload.data.find((category) => category.id === 101),
    { id: 101, name: "Adventure", slug: "adventure", group: "Gameplay", displayIndex: 1 },
  );
});

test("envia categoryId com classe, versão e loader na busca CurseForge", async () => {
  const { publicJwk, token } = await firebaseIdentity("search-test-user");
  globalThis.fetch = async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    if (url.hostname === "api.curseforge.com") {
      assert.equal(url.pathname, "/v1/mods/search");
      assert.equal(url.searchParams.get("gameId"), "432");
      assert.equal(url.searchParams.get("classId"), "4471");
      assert.equal(url.searchParams.get("categoryId"), "101");
      assert.equal(url.searchParams.get("gameVersion"), "1.20.1");
      assert.equal(url.searchParams.get("modLoaderType"), "1");
      return jsonResponse({ data: [], pagination: { totalCount: 0 } });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(new Request("https://aurora.test/v1/curseforge", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      action: "search",
      contentType: "modpack",
      categoryId: 101,
      gameVersion: "1.20.1",
      loader: "forge",
      pageSize: 20,
      index: 0,
    }),
  }), env);
  assert.equal(response.status, 200);
});
