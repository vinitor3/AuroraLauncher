import assert from "node:assert/strict";
import test from "node:test";
import { MojangClient } from "./skins/MojangClient.ts";
import { SkinService } from "./skins/SkinService.ts";
import { SkinApiError } from "./skins/http.ts";

const uuid = "069a79f444e94726a5befca90e38aaf5";
const texture = `https://textures.minecraft.net/texture/${"a".repeat(64)}`;
const session = (changes: Record<string, unknown> = {}) => ({
  id: uuid, name: "Notch", properties: [{ name: "textures", value: btoa(JSON.stringify({
    profileId: uuid, textures: { SKIN: { url: texture.replace("https:", "http:"), metadata: { model: "slim" } } }, ...changes,
  })) }],
});

test("known Java name resolves through Mojang when both third-party sources return false 404", async () => {
  const calls: string[] = [];
  const service = new SkinService({ MC_API_KEY: "private-mcapi", MCSTAT_API_KEY: "private-mcstat" }, Date.now, async (input, init) => {
    const url = new URL(String(input));
    calls.push(url.hostname + url.pathname);
    if (url.hostname === "mc-api.bisai.dev" || url.hostname === "mcstat.org") return new Response(null, { status: 404 });
    assert.equal(new Headers(init?.headers).has("X-API-Key"), false);
    assert.ok(init?.signal);
    if (url.hostname === "api.minecraftservices.com") return Response.json({ id: uuid, name: "Notch" });
    assert.equal(url.href, `https://sessionserver.mojang.com/session/minecraft/profile/${uuid}`);
    return Response.json(session());
  });
  const [result, duplicate] = await Promise.all([service.getPlayerProfile("Notch"), service.getPlayerProfile("notch")]);
  assert.deepEqual(result, duplicate);
  assert.equal(result.source, "mojang");
  assert.equal(result.model, "slim");
  assert.equal(result.skinUrl, texture);
  assert.equal(result.uuid, uuid);
  assert.equal(calls.length, 5, "Concurrent duplicate names share one provider lookup chain");
  await service.getPlayerProfile("NOTCH");
  assert.equal(calls.length, 5, "The recovered profile is cached normally");
});

test("direct UUID fallback skips the name request and validates the textures owner", async () => {
  let calls = 0;
  const client = new MojangClient(30, async (input) => {
    calls++;
    assert.equal(new URL(String(input)).hostname, "sessionserver.mojang.com");
    return Response.json(session(calls === 1 ? {} : { profileId: "b".repeat(32) }));
  }, Date.now);
  assert.equal((await client.getPlayerProfile("069a79f4-44e9-4726-a5be-fca90e38aaf5")).uuid, uuid);
  await assert.rejects(client.getPlayerProfile(uuid), /outro jogador/);
  assert.equal(calls, 2);
});

test("official fallback rejects switched profiles, invalid texture JSON and arbitrary image hosts", async () => {
  for (const result of [
    { ...session(), id: "b".repeat(32) },
    { ...session(), properties: [{ name: "textures", value: btoa("not JSON") }] },
    session({ textures: { SKIN: { url: "https://evil.invalid/skin.png" } } }),
  ]) {
    const client = new MojangClient(30, async () => Response.json(result), Date.now);
    await assert.rejects(client.getPlayerProfile(uuid), (error) => error instanceof SkinApiError && error.status === 502);
  }
});

test("official exact-name autocomplete works without API keys when the index is down", async () => {
  const service = new SkinService({}, Date.now, async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "mc-api.bisai.dev") return new Response(null, { status: 503 });
    assert.equal(url.hostname, "api.minecraftservices.com");
    return url.pathname.endsWith("/Notch") ? Response.json({ id: uuid, name: "Notch" }) : new Response(null, { status: 404 });
  });
  assert.deepEqual((await service.searchPlayers("Notch")).map((row) => row.name), ["Notch"]);
  assert.deepEqual(await service.searchPlayers("Not"), [], "A missing prefix is an empty suggestion list, not a player error");
});

test("temporary official outage is not mislabeled as a missing player and 429 does not trigger more providers", async () => {
  let calls = 0;
  const service = new SkinService({}, Date.now, async (input) => {
    calls++;
    return new Response(null, { status: new URL(String(input)).hostname === "mc-api.bisai.dev" ? 404 : 503 });
  });
  await assert.rejects(service.getPlayerProfile("Notch"), (error) => error instanceof SkinApiError
    && error.status === 503 && !error.message.includes("não encontrado"));
  assert.equal(calls, 3);
  const limited = new SkinService({}, Date.now, async () => {
    calls++;
    return new Response(null, { status: 429, headers: { "Retry-After": "50" } });
  });
  await assert.rejects(limited.getPlayerProfile("Notch"), (error) => error instanceof SkinApiError && error.status === 429);
  assert.equal(calls, 4);
});

test("only an official missing profile confirms Jogador não encontrado", async () => {
  const service = new SkinService({ MCSTAT_API_KEY: "private-mcstat" }, Date.now,
    async () => new Response(null, { status: 404 }));
  await assert.rejects(service.getPlayerProfile("UnknownPlayer"), (error) => error instanceof SkinApiError
    && error.status === 404 && error.message === "Jogador não encontrado.");
});

test("first successful fallback cancels the other provider request", async () => {
  let cancelled = false;
  const service = new SkinService({ MCSTAT_API_KEY: "private-mcstat" }, Date.now, async (input, init) => {
    const url = new URL(String(input));
    if (url.hostname === "mc-api.bisai.dev") return new Response(null, { status: 404 });
    if (url.hostname === "sessionserver.mojang.com") return Response.json(session());
    return new Promise<Response>((_resolve, reject) => {
      init!.signal!.addEventListener("abort", () => { cancelled = true; reject(init!.signal!.reason); }, { once: true });
    });
  });
  assert.equal((await service.getPlayerProfile(uuid)).source, "mojang");
  assert.equal(cancelled, true);
});

test("provider authorization cooldown does not block the next player's official fallback as a false 429", async () => {
  let primaryCalls = 0, nameCalls = 0;
  const otherUuid = "b".repeat(32);
  const service = new SkinService({ MC_API_KEY: "revoked-key" }, Date.now, async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "mc-api.bisai.dev") { primaryCalls++; return new Response(null, { status: 403 }); }
    const isNotch = url.pathname.endsWith("/Notch") || url.pathname.endsWith(`/${uuid}`);
    const id = isNotch ? uuid : otherUuid, name = isNotch ? "Notch" : "jeb_";
    if (url.hostname === "api.minecraftservices.com") { nameCalls++; return Response.json({ id, name }); }
    return Response.json({ id, name, properties: [{ name: "textures", value: btoa(JSON.stringify({
      profileId: id, textures: { SKIN: { url: texture } },
    })) }] });
  });
  assert.equal((await service.getPlayerProfile("Notch")).source, "mojang");
  assert.equal((await service.getPlayerProfile("jeb_")).source, "mojang");
  assert.equal(primaryCalls, 1, "An authorization failure still suppresses repeated requests to that provider");
  assert.equal(nameCalls, 2, "The official source remains available for other players during that cooldown");
});

test("an empty secondary autocomplete cannot cancel a slower valid official name", async () => {
  const service = new SkinService({ MCSTAT_API_KEY: "private-mcstat" }, Date.now, async (input, init) => {
    const url = new URL(String(input));
    if (url.hostname === "mc-api.bisai.dev") return new Response(null, { status: 404 });
    if (url.hostname === "mcstat.org") return Response.json({ data: { players: [] } });
    await new Promise<void>((resolve) => setTimeout(resolve, 5));
    assert.equal(init!.signal!.aborted, false);
    return Response.json({ id: uuid, name: "Notch" });
  });
  const result = await service.autocompleteSkinNames("Notch");
  assert.equal(result.length, 1);
  assert.equal(result[0].name, "Notch");
  assert.equal(result[0].source, "mojang");
});

test("name profile timeout and Cloudflare official 403 recover through the exact current index UUID", async () => {
  const service = new SkinService({}, Date.now, async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "api.minecraftservices.com") return new Response(null, { status: 403 });
    if (url.pathname === "/v1/profile/Notch") return new Response(null, { status: 504 });
    if (url.pathname === "/v1/players/search") return Response.json({ players: [
      { name: "Notch", uuid: "b".repeat(32), current: false },
      { name: "Notch", uuid, current: true },
    ] });
    assert.equal(url.pathname, `/v1/profile/${uuid}`);
    return Response.json({ uuid, username: "Notch", skin_url: texture, is_slim: false });
  });
  const result = await service.getPlayerProfile("Notch");
  assert.equal(result.uuid, uuid);
  assert.equal(result.source, "mc-api");
});

test("name recovery never uses previous-name matches or a renamed profile", async () => {
  for (const current of [false, true]) {
    let profileCalls = 0;
    const service = new SkinService({}, Date.now, async (input) => {
      const url = new URL(String(input));
      if (url.hostname === "api.minecraftservices.com") return new Response(null, { status: 403 });
      if (url.pathname === "/v1/profile/Notch") return new Response(null, { status: 504 });
      if (url.pathname === "/v1/players/search") return Response.json({ players: [{ name: "Notch", uuid, current }] });
      profileCalls++;
      return Response.json({ uuid, username: "RenamedPlayer", skin_url: texture });
    });
    await assert.rejects(service.getPlayerProfile("Notch"), (error) => error instanceof SkinApiError && error.status === 503);
    assert.equal(profileCalls, current ? 1 : 0);
  }
});

test("a mismatching MCStat name cannot win the official identity fallback race", async () => {
  const service = new SkinService({ MCSTAT_API_KEY: "private-mcstat" }, Date.now, async (input) => {
    const url = new URL(String(input));
    if (url.hostname === "mcstat.org") return Response.json({ data: { player: {
      mainNickname: "SomeoneElse", minecraftUuid: uuid, skinUrl: texture,
    } } });
    return new Response(null, { status: url.hostname === "api.minecraftservices.com" ? 403 : 504 });
  });
  await assert.rejects(service.getPlayerProfile("Notch"), (error) => error instanceof SkinApiError && error.status === 503);
});
