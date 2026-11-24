import assert from "node:assert/strict";
import test, { afterEach } from "node:test";

import worker from "./index.ts";
import { png } from "../tests/skinPng.ts";

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

test("as seis skins retiradas não são servidas pelo Worker", async () => {
  for (const id of ["aurora-voyager", "solar-ninja", "ki-guardian", "forest-ranger", "cyber-violet", "crimson-mage"]) {
    const asset = await worker.fetch(new Request("https://aurora.test/v1/skins/catalog/" + id + ".png"), env);
    assert.equal(asset.status, 404);
  }
});

test("agrega skins do MCStat sem expor a chave e entrega PNG verificado", async () => {
  const fixture = png();
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
  assert.equal(catalog.headers.get("X-Aurora-Catalog-Sources"), "mcstat");
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

test("quando o provedor está indisponível não reaparecem as skins retiradas", async () => {
  globalThis.fetch = async () => jsonResponse({ error: "rate limited" }, 429);
  for (const configuration of [env, envWithMcstat]) {
    const response = await worker.fetch(new Request("https://aurora.test/v1/skins/catalog?query=naruto"), configuration);
    assert.equal(response.status, 503);
    assert.doesNotMatch(await response.text(), /solar-ninja|Aurora Studio/i);
  }
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
    return jsonResponse({ message: "rate limited" }, 429, { "Retry-After": "0" });
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

function curseForgeRequest(token: string, body: Record<string, unknown>) {
  return new Request("https://aurora.test/v1/curseforge", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
}

test("seleciona a release CurseForge estável mais recente compatível e preserva dependências", async () => {
  const { publicJwk, token } = await firebaseIdentity("updates-compatible-user");
  const currentSha1 = "1".repeat(40);
  const targetSha1 = "2".repeat(40);
  globalThis.fetch = async (input, init) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    if (url.pathname === "/v1/mods") {
      assert.deepEqual(JSON.parse(String(init?.body)), { modIds: [101], filterPcOnly: true });
      return jsonResponse({ data: [{ id: 101, classId: 6, name: "Aurora Mod", slug: "aurora-mod" }] });
    }
    if (url.pathname === "/v1/mods/101/files") {
      assert.equal(url.searchParams.get("gameVersion"), "1.20.1");
      assert.equal(url.searchParams.get("modLoaderType"), "4");
      assert.equal(url.searchParams.get("pageSize"), "50");
      if (url.searchParams.get("index") === "5") {
        return jsonResponse({
          data: [{
            id: 106,
            modId: 101,
            fileName: "newest-compatible.jar",
            fileDate: "2026-09-01T00:00:00Z",
            releaseType: 1,
            gameVersions: ["1.20.1", "Fabric"],
            hashes: [{ algo: 1, value: targetSha1 }],
            dependencies: [{ modId: 201, relationType: 3 }],
            downloadUrl: "https://edge.forgecdn.net/files/newest-compatible.jar",
          }],
          pagination: { index: 5, pageSize: 50, resultCount: 1, totalCount: 6 },
        });
      }
      assert.equal(url.searchParams.get("index"), "0");
      return jsonResponse({
        data: [
          {
            id: 101,
            modId: 101,
            fileName: "installed.jar",
            fileDate: "2026-08-01T00:00:00Z",
            releaseType: 2,
            gameVersions: ["1.20.1", "Fabric"],
            hashes: [{ algo: 1, value: currentSha1 }],
          },
          {
            id: 105,
            modId: 101,
            fileName: "beta.jar",
            fileDate: "2026-08-31T00:00:00Z",
            releaseType: 2,
            gameVersions: ["1.20.1", "Fabric"],
            hashes: [{ algo: 1, value: "5".repeat(40) }],
          },
          {
            id: 104,
            modId: 101,
            fileName: "wrong-version.jar",
            fileDate: "2026-08-30T00:00:00Z",
            releaseType: 1,
            gameVersions: ["1.21.1", "Fabric"],
            hashes: [{ algo: 1, value: "4".repeat(40) }],
          },
          {
            id: 103,
            modId: 101,
            fileName: "wrong-loader.jar",
            fileDate: "2026-08-29T00:00:00Z",
            releaseType: 1,
            gameVersions: ["1.20.1", "Forge"],
            hashes: [{ algo: 1, value: "3".repeat(40) }],
          },
          {
            id: 102,
            modId: 101,
            fileName: "compatible.jar",
            fileDate: "2026-08-28T00:00:00Z",
            releaseType: 1,
            gameVersions: ["1.20.1", "Fabric"],
            hashes: [{ algo: 1, value: "6".repeat(40) }],
          },
        ],
        pagination: { index: 0, pageSize: 50, resultCount: 5, totalCount: 6 },
      });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(curseForgeRequest(token, {
    action: "updates",
    contentType: "mod",
    gameVersion: "1.20.1",
    loader: "fabric",
    items: [{ projectId: 101, currentSha1 }],
  }), env);
  assert.equal(response.status, 200);
  const payload = await response.json() as { data: Array<Record<string, any>> };
  assert.equal(payload.data[0].status, "updateAvailable");
  assert.equal(payload.data[0].targetFile.id, 106);
  assert.equal(payload.data[0].targetFile.sha1, targetSha1);
  assert.deepEqual(payload.data[0].targetFile.dependencies, [{ modId: 201, relationType: 3 }]);
  assert.equal(payload.data[0].targetFile.downloadMode, "proxy");
  assert.match(payload.data[0].targetFile.downloadUrl, /^https:\/\/aurora\.test\/v1\/curseforge\/download\?ticket=/);
});

test("busca o fileId instalado exato e preserva o loader ao verificar um modpack", async () => {
  const { publicJwk, token } = await firebaseIdentity("updates-modpack-file-user");
  const targetSha1 = "6".repeat(40);
  let pageCalls = 0;
  let exactCalls = 0;
  globalThis.fetch = async (input, init) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    if (url.pathname === "/v1/mods") {
      assert.deepEqual(JSON.parse(String(init?.body)), { modIds: [601], filterPcOnly: true });
      return jsonResponse({ data: [{ id: 601, classId: 4471, name: "Aurora Pack", slug: "aurora-pack" }] });
    }
    if (url.pathname === "/v1/mods/601/files") {
      pageCalls += 1;
      assert.equal(url.searchParams.get("gameVersion"), "1.20.1");
      assert.equal(url.searchParams.get("modLoaderType"), "4");
      return jsonResponse({
        data: [
          {
            id: 603,
            modId: 601,
            fileName: "aurora-pack-fabric-new.zip",
            fileDate: "2026-09-02T00:00:00Z",
            releaseType: 1,
            gameVersions: ["1.20.1", "Fabric"],
            hashes: [{ algo: 1, value: targetSha1 }],
            downloadUrl: "https://edge.forgecdn.net/files/aurora-pack-fabric-new.zip",
          },
        ],
        pagination: { index: 0, pageSize: 50, resultCount: 1, totalCount: 1 },
      });
    }
    if (url.pathname === "/v1/mods/601/files/602") {
      exactCalls += 1;
      return jsonResponse({ data: {
        id: 602,
        modId: 601,
        fileName: "aurora-pack-fabric.zip",
        fileDate: "2026-09-01T00:00:00Z",
        releaseType: 1,
        gameVersions: ["1.20.1", "Fabric"],
        hashes: [{ algo: 1, value: "5".repeat(40) }],
      } });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(curseForgeRequest(token, {
    action: "updates",
    contentType: "modpack",
    gameVersion: "1.20.1",
    loader: "fabric",
    items: [{ projectId: 601, currentFileId: 602 }],
  }), env);
  assert.equal(response.status, 200);
  const payload = await response.json() as { data: Array<Record<string, any>> };
  assert.equal(payload.data[0].status, "updateAvailable");
  assert.equal(payload.data[0].currentFileId, 602);
  assert.equal(payload.data[0].currentSha1, undefined);
  assert.equal(payload.data[0].targetFile.id, 603);
  assert.equal(payload.data[0].targetFile.sha1, targetSha1);
  assert.equal(pageCalls, 1);
  assert.equal(exactCalls, 1);

  const missingLoader = await worker.fetch(curseForgeRequest(token, {
    action: "updates",
    contentType: "modpack",
    gameVersion: "1.20.1",
    items: [{ projectId: 601, currentFileId: 602 }],
  }), env);
  assert.equal(missingLoader.status, 200);
  const missingLoaderPayload = await missingLoader.json() as { data: Array<Record<string, unknown>> };
  assert.equal(missingLoaderPayload.data[0].status, "blocked");
  assert.equal(missingLoaderPayload.data[0].reason, "loaderRequired");
  assert.equal(missingLoaderPayload.data[0].currentFileId, 602);
  assert.equal(pageCalls, 1);
  assert.equal(exactCalls, 1);
});

test("marca conteúdo CurseForge com a mesma SHA-1 como atualizado", async () => {
  const { publicJwk, token } = await firebaseIdentity("updates-current-user");
  const sha1 = "a".repeat(40);
  globalThis.fetch = async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    if (url.pathname === "/v1/mods") {
      return jsonResponse({ data: [{ id: 202, classId: 12, name: "Aurora Pack", slug: "aurora-pack" }] });
    }
    if (url.pathname === "/v1/mods/202/files") {
      return jsonResponse({
        data: [{
          id: 203,
          modId: 202,
          fileName: "aurora-pack.zip",
          fileDate: "2026-08-30T00:00:00Z",
          releaseType: 1,
          gameVersions: ["1.20.1"],
          hashes: [{ algo: 1, value: sha1.toUpperCase() }],
          downloadUrl: "https://media.forgecdn.net/files/aurora-pack.zip",
        }],
        pagination: { resultCount: 1, totalCount: 1 },
      });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(curseForgeRequest(token, {
    action: "updates",
    contentType: "resourcepack",
    gameVersion: "1.20.1",
    items: [{ projectId: 202, currentSha1: sha1 }],
  }), env);
  assert.equal(response.status, 200);
  const payload = await response.json() as { data: Array<Record<string, any>> };
  assert.equal(payload.data[0].status, "upToDate");
  assert.equal(payload.data[0].currentSha1, sha1);
  assert.equal(payload.data[0].targetFile.sha1, sha1);
});

test("bloqueia atualização quando não existe release estável compatível", async () => {
  const { publicJwk, token } = await firebaseIdentity("updates-no-release-user");
  globalThis.fetch = async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    if (url.pathname === "/v1/mods") {
      return jsonResponse({ data: [{ id: 303, classId: 6, name: "Beta only", slug: "beta-only" }] });
    }
    if (url.pathname === "/v1/mods/303/files") {
      return jsonResponse({
        data: [{
          id: 304,
          modId: 303,
          fileName: "beta-only.jar",
          fileDate: "2026-08-29T00:00:00Z",
          releaseType: 2,
          gameVersions: ["1.19.2", "Forge"],
          hashes: [{ algo: 1, value: "c".repeat(40) }],
        }],
        pagination: { resultCount: 1, totalCount: 1 },
      });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(curseForgeRequest(token, {
    action: "updates",
    contentType: "mod",
    gameVersion: "1.19.2",
    loader: "forge",
    items: [{ projectId: 303, currentSha1: "c".repeat(40) }],
  }), env);
  assert.equal(response.status, 200);
  const payload = await response.json() as { data: Array<Record<string, unknown>> };
  assert.equal(payload.data[0].status, "blocked");
  assert.equal(payload.data[0].reason, "noCompatibleRelease");
});

test("mantém indicação de página oficial quando a release não possui URL direta", async () => {
  const { publicJwk, token } = await firebaseIdentity("updates-manual-user");
  const targetSha1 = "d".repeat(40);
  globalThis.fetch = async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    if (url.pathname === "/v1/mods") {
      return jsonResponse({ data: [{ id: 404, classId: 6552, name: "Manual Shader", slug: "manual-shader" }] });
    }
    if (url.pathname === "/v1/mods/404/files") {
      return jsonResponse({
        data: [
          {
            id: 405,
            modId: 404,
            fileName: "manual-shader.zip",
            fileDate: "2026-08-30T00:00:00Z",
            releaseType: 1,
            gameVersions: ["1.21.1"],
            hashes: [{ algo: 1, value: targetSha1 }],
            dependencies: [{ modId: 406, relationType: 3 }],
          },
          {
            id: 403,
            modId: 404,
            fileName: "installed-shader.zip",
            fileDate: "2026-08-29T00:00:00Z",
            releaseType: 1,
            gameVersions: ["1.21.1"],
            hashes: [{ algo: 1, value: "e".repeat(40) }],
          },
        ],
        pagination: { resultCount: 2, totalCount: 2 },
      });
    }
    if (url.pathname === "/v1/mods/404/files/405/download-url") {
      return jsonResponse({ message: "distribution disabled" }, 404);
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(curseForgeRequest(token, {
    action: "updates",
    contentType: "shader",
    gameVersion: "1.21.1",
    items: [{ projectId: 404, currentSha1: "e".repeat(40) }],
  }), env);
  assert.equal(response.status, 200);
  const payload = await response.json() as { data: Array<Record<string, any>> };
  assert.equal(payload.data[0].status, "updateAvailable");
  assert.equal(payload.data[0].reason, "manualDownloadRequired");
  assert.equal(payload.data[0].targetFile.downloadMode, "officialPage");
  assert.equal(payload.data[0].targetFile.manualDownloadRequired, true);
  assert.equal(payload.data[0].targetFile.downloadUrl, undefined);
  assert.deepEqual(payload.data[0].targetFile.dependencies, [{ modId: 406, relationType: 3 }]);
});

test("bloqueia a sugestão quando o limite de páginas é atingido mesmo após achar candidato", async () => {
  const { publicJwk, token } = await firebaseIdentity("updates-scan-cap-user");
  const currentSha1 = "1".repeat(40);
  let pageCalls = 0;
  globalThis.fetch = async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    if (url.pathname === "/v1/mods") {
      return jsonResponse({ data: [{ id: 701, classId: 6, name: "Long history", slug: "long-history" }] });
    }
    if (url.pathname === "/v1/mods/701/files") {
      const index = Number(url.searchParams.get("index"));
      pageCalls += 1;
      return jsonResponse({
        data: [{
          id: 710 + index,
          modId: 701,
          fileName: `page-${index}.jar`,
          fileDate: `2026-08-${String(index + 10).padStart(2, "0")}T00:00:00Z`,
          releaseType: 1,
          gameVersions: ["1.20.1", "Fabric"],
          hashes: [{ algo: 1, value: index === 0 ? currentSha1 : String(index + 1).repeat(40) }],
          downloadUrl: `https://edge.forgecdn.net/files/page-${index}.jar`,
        }],
        pagination: { index, pageSize: 50, resultCount: 1, totalCount: 5 },
      });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(curseForgeRequest(token, {
    action: "updates",
    contentType: "mod",
    gameVersion: "1.20.1",
    loader: "fabric",
    items: [{ projectId: 701, currentSha1 }],
  }), env);
  assert.equal(response.status, 200);
  const payload = await response.json() as { data: Array<Record<string, unknown>> };
  assert.equal(payload.data[0].status, "blocked");
  assert.equal(payload.data[0].reason, "scanLimitReached");
  assert.equal(payload.data[0].targetFile, undefined);
  assert.equal(pageCalls, 4);
});

test("não oferece downgrade quando a versão instalada é mais nova que a release estável", async () => {
  const { publicJwk, token } = await firebaseIdentity("updates-no-downgrade-user");
  const currentSha1 = "8".repeat(40);
  globalThis.fetch = async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    if (url.pathname === "/v1/mods") {
      return jsonResponse({ data: [{ id: 801, classId: 6, name: "Preview mod", slug: "preview-mod" }] });
    }
    if (url.pathname === "/v1/mods/801/files") {
      return jsonResponse({
        data: [
          {
            id: 803,
            modId: 801,
            fileName: "installed-beta.jar",
            fileDate: "2026-09-02T00:00:00Z",
            releaseType: 2,
            gameVersions: ["1.20.1", "Forge"],
            hashes: [{ algo: 1, value: currentSha1 }],
          },
          {
            id: 802,
            modId: 801,
            fileName: "older-stable.jar",
            fileDate: "2026-09-01T00:00:00Z",
            releaseType: 1,
            gameVersions: ["1.20.1", "Forge"],
            hashes: [{ algo: 1, value: "9".repeat(40) }],
          },
        ],
        pagination: { resultCount: 2, totalCount: 2 },
      });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(curseForgeRequest(token, {
    action: "updates",
    contentType: "mod",
    gameVersion: "1.20.1",
    loader: "forge",
    items: [{ projectId: 801, currentSha1 }],
  }), env);
  const payload = await response.json() as { data: Array<Record<string, unknown>> };
  assert.equal(payload.data[0].status, "blocked");
  assert.equal(payload.data[0].reason, "installedVersionNotOlder");
  assert.equal(payload.data[0].targetFile, undefined);
});

test("rejeita fileId instalado fora do projeto e contexto solicitados", async () => {
  const { publicJwk, token } = await firebaseIdentity("updates-current-context-user");
  globalThis.fetch = async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    if (url.pathname === "/v1/mods") {
      return jsonResponse({ data: [{ id: 901, classId: 4471, name: "Context pack", slug: "context-pack" }] });
    }
    if (url.pathname === "/v1/mods/901/files") {
      return jsonResponse({
        data: [{
          id: 903,
          modId: 901,
          fileName: "target.zip",
          fileDate: "2026-09-02T00:00:00Z",
          releaseType: 1,
          gameVersions: ["1.20.1", "Fabric"],
          hashes: [{ algo: 1, value: "3".repeat(40) }],
        }],
        pagination: { resultCount: 1, totalCount: 1 },
      });
    }
    if (url.pathname === "/v1/mods/901/files/902") {
      return jsonResponse({ data: {
        id: 902,
        modId: 901,
        fileName: "wrong-loader.zip",
        fileDate: "2026-09-01T00:00:00Z",
        releaseType: 1,
        gameVersions: ["1.20.1", "Forge"],
      } });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(curseForgeRequest(token, {
    action: "updates",
    contentType: "modpack",
    gameVersion: "1.20.1",
    loader: "fabric",
    items: [{ projectId: 901, currentFileId: 902 }],
  }), env);
  const payload = await response.json() as { data: Array<Record<string, unknown>> };
  assert.equal(payload.data[0].status, "blocked");
  assert.equal(payload.data[0].reason, "installedFileContextMismatch");
});

test("aceita no máximo quatro projetos por chamada de atualização", async () => {
  const { publicJwk, token } = await firebaseIdentity("updates-four-items-user");
  const projectIds = [1001, 1002, 1003, 1004];
  globalThis.fetch = async (input, init) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    if (url.pathname === "/v1/mods") {
      assert.deepEqual(JSON.parse(String(init?.body)), { modIds: projectIds, filterPcOnly: true });
      return jsonResponse({ data: projectIds.map((id) => ({ id, classId: 12, name: `Pack ${id}` })) });
    }
    const match = url.pathname.match(/^\/v1\/mods\/(\d+)\/files$/);
    if (match) {
      const projectId = Number(match[1]);
      const sha1 = String(projectId % 10).repeat(40);
      return jsonResponse({
        data: [{
          id: projectId + 10,
          modId: projectId,
          fileName: `${projectId}.zip`,
          fileDate: "2026-09-01T00:00:00Z",
          releaseType: 1,
          gameVersions: ["1.20.1"],
          hashes: [{ algo: 1, value: sha1 }],
        }],
        pagination: { resultCount: 1, totalCount: 1 },
      });
    }
    throw new Error(`fetch inesperado: ${url}`);
  };

  const response = await worker.fetch(curseForgeRequest(token, {
    action: "updates",
    contentType: "resourcepack",
    gameVersion: "1.20.1",
    items: projectIds.map((projectId) => ({ projectId, currentSha1: String(projectId % 10).repeat(40) })),
  }), env);
  assert.equal(response.status, 200);
  const payload = await response.json() as { data: Array<Record<string, unknown>> };
  assert.equal(payload.data.length, 4);
  assert.ok(payload.data.every((result) => result.status === "upToDate"));
});

test("reserva o ciclo completo de scan, confirmação e refresh antes do limite específico", async () => {
  const { publicJwk, token } = await firebaseIdentity("updates-rate-window-user");
  globalThis.fetch = async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    throw new Error(`fetch inesperado: ${url}`);
  };
  for (let index = 0; index < 32; index += 1) {
    const response = await worker.fetch(curseForgeRequest(token, { action: "updates" }), env);
    assert.equal(response.status, 400);
  }
  const limited = await worker.fetch(curseForgeRequest(token, { action: "updates" }), env);
  assert.equal(limited.status, 429);
  assert.match(await limited.text(), /muitas solicitações/i);
});

test("rejeita schema, limite, IDs e hashes inválidos na verificação de atualizações", async () => {
  const { publicJwk, token } = await firebaseIdentity("updates-invalid-user");
  let curseForgeCalls = 0;
  globalThis.fetch = async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    curseForgeCalls += 1;
    throw new Error(`fetch inesperado: ${url}`);
  };
  const validBase = {
    action: "updates",
    contentType: "mod",
    gameVersion: "1.20.1",
    loader: "fabric",
  };
  const invalidBodies = [
    { ...validBase, items: "invalid" },
    {
      ...validBase,
      items: Array.from({ length: 5 }, (_, index) => ({ projectId: index + 1, currentSha1: "a".repeat(40) })),
    },
    { ...validBase, items: [{ projectId: 0, currentSha1: "a".repeat(40) }] },
    { ...validBase, items: [{ projectId: 1, currentSha1: "not-a-sha1" }] },
  ];
  for (const body of invalidBodies) {
    const response = await worker.fetch(curseForgeRequest(token, body), env);
    assert.equal(response.status, 400);
  }
  assert.equal(curseForgeCalls, 0);
});

test("exige exatamente um identificador atual e reserva fileId para modpacks", async () => {
  const { publicJwk, token } = await firebaseIdentity("updates-current-identity-user");
  let curseForgeCalls = 0;
  globalThis.fetch = async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    curseForgeCalls += 1;
    throw new Error(`fetch inesperado: ${url}`);
  };
  const sha1 = "a".repeat(40);
  const invalidBodies = [
    {
      action: "updates",
      contentType: "mod",
      gameVersion: "1.20.1",
      loader: "fabric",
      items: [{ projectId: 1, currentSha1: sha1, currentFileId: 2 }],
    },
    {
      action: "updates",
      contentType: "mod",
      gameVersion: "1.20.1",
      loader: "fabric",
      items: [{ projectId: 1 }],
    },
    {
      action: "updates",
      contentType: "mod",
      gameVersion: "1.20.1",
      loader: "fabric",
      items: [{ projectId: 1, currentFileId: 2 }],
    },
    {
      action: "updates",
      contentType: "modpack",
      gameVersion: "1.20.1",
      loader: "fabric",
      items: [{ projectId: 1, currentSha1: sha1 }],
    },
  ];
  for (const body of invalidBodies) {
    const response = await worker.fetch(curseForgeRequest(token, body), env);
    assert.equal(response.status, 400);
  }
  assert.equal(curseForgeCalls, 0);
});

test("rejeita tipos e loaders herdados do protótipo na verificação de atualizações", async () => {
  const { publicJwk, token } = await firebaseIdentity("updates-prototype-user");
  let curseForgeCalls = 0;
  globalThis.fetch = async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "www.googleapis.com") return jsonResponse({ keys: [publicJwk] });
    curseForgeCalls += 1;
    throw new Error(`fetch inesperado: ${url}`);
  };
  const item = { projectId: 1, currentSha1: "a".repeat(40) };
  const inheritedType = await worker.fetch(curseForgeRequest(token, {
    action: "updates",
    contentType: "toString",
    gameVersion: "1.20.1",
    items: [item],
  }), env);
  assert.equal(inheritedType.status, 400);
  const inheritedLoader = await worker.fetch(curseForgeRequest(token, {
    action: "updates",
    contentType: "mod",
    gameVersion: "1.20.1",
    loader: "constructor",
    items: [item],
  }), env);
  assert.equal(inheritedLoader.status, 400);
  assert.equal(curseForgeCalls, 0);
});
