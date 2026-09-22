import assert from "node:assert/strict";
import test from "node:test";
import { setImmediate } from "node:timers/promises";
import { PlayerSkinsClient } from "../src/lib/playerSkins.ts";

const uuid = "069a79f444e94726a5befca90e38aaf5";
const texture = `https://textures.minecraft.net/texture/${"a".repeat(64)}`;
const profile = { name: "Notch", uuid, model: "classic", skinUrl: texture, textureUrl: texture,
  contentHash: "b".repeat(64), source: "mojang-local", stale: false };

function native(t, implementation) {
  const previousWindow = globalThis.window, previousTauri = globalThis.isTauri;
  const calls = [];
  globalThis.isTauri = true;
  globalThis.window = { __TAURI_INTERNALS__: { invoke: async (command, args) => { calls.push({ command, args }); return implementation(); } } };
  t.after(() => {
    if (previousWindow === undefined) delete globalThis.window; else globalThis.window = previousWindow;
    if (previousTauri === undefined) delete globalThis.isTauri; else globalThis.isTauri = previousTauri;
  });
  return calls;
}

test("desktop recovers verified public profile natively after Worker404/503 or network failure", async (t) => {
  const calls = native(t, () => profile);
  for (const status of [404, 503, 0]) {
    t.mock.method(globalThis, "fetch", async () => {
      if (!status) throw new TypeError("network");
      return Response.json({ error: "Fonte indisponível" }, { status });
    });
    const result = await new PlayerSkinsClient("https://aurora.test").getPlayerProfile(" Notch ");
    assert.equal(result.source, "mojang-local");
    assert.equal(result.skinUrl, texture);
  }
  assert.equal(calls.length, 3);
  assert.deepEqual(calls[0], { command: "lookup_player_skin", args: { identifier: "Notch" } });
});

test("rate limits, invalid Worker assets and browser sessions never invoke native lookup", async (t) => {
  const calls = native(t, () => profile);
  t.mock.method(globalThis, "fetch", async () => Response.json({ error: "quota" }, { status: 429 }));
  await assert.rejects(new PlayerSkinsClient("https://aurora.test").getPlayerProfile("Notch"), /Muitas consultas/);
  t.mock.method(globalThis, "fetch", async () => Response.json(profile));
  await assert.rejects(new PlayerSkinsClient("https://aurora.test").getPlayerProfile("Notch"), /Perfil de skin inválido/);
  globalThis.isTauri = false;
  t.mock.method(globalThis, "fetch", async () => Response.json({ error: "unavailable" }, { status: 503 }));
  await assert.rejects(new PlayerSkinsClient("https://aurora.test").getPlayerProfile("Notch"), /unavailable/);
  assert.equal(calls.length, 0);
});

test("native response must match requested identity, verified source and official texture URL", async (t) => {
  let result;
  native(t, () => result);
  t.mock.method(globalThis, "fetch", async () => Response.json({ error: "unavailable" }, { status: 503 }));
  for (result of [
    { ...profile, name: "SomeoneElse" }, { ...profile, source: "mc-api" },
    { ...profile, skinUrl: "https://evil.invalid/skin.png", textureUrl: "https://evil.invalid/skin.png" },
    { ...profile, skinUrl: texture + "?token=x", textureUrl: texture + "?token=x" },
  ]) await assert.rejects(new PlayerSkinsClient("https://aurora.test").getPlayerProfile("Notch"), /oficial.*inválido/);
  result = { ...profile, uuid: "b".repeat(32) };
  await assert.rejects(new PlayerSkinsClient("https://aurora.test").getPlayerProfile(uuid), /oficial.*inválido/);
});

test("cancelled selection ignores a late native lookup result", async (t) => {
  let resolve;
  native(t, () => new Promise((yes) => { resolve = yes; }));
  t.mock.method(globalThis, "fetch", async () => Response.json({ error: "unavailable" }, { status: 503 }));
  const controller = new AbortController();
  const pending = new PlayerSkinsClient("https://aurora.test").getPlayerProfile("Notch", controller.signal);
  await setImmediate();
  controller.abort(); resolve(profile);
  await assert.rejects(pending, (error) => error.name === "AbortError");
});

test("cancelled selection also ignores a late native failure", async (t) => {
  let reject;
  native(t, () => new Promise((_resolve, no) => { reject = no; }));
  t.mock.method(globalThis, "fetch", async () => Response.json({ error: "unavailable" }, { status: 503 }));
  const controller = new AbortController();
  const pending = new PlayerSkinsClient("https://aurora.test").getPlayerProfile("Notch", controller.signal);
  await setImmediate();
  controller.abort(); reject("Jogador não encontrado.");
  await assert.rejects(pending, (error) => error.name === "AbortError");
});
