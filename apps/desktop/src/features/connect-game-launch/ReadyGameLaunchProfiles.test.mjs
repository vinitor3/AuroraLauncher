// Pure DTO contracts. These tests do not create Java, transport or game files.
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import ts from "typescript";
const source = await readFile(new URL("./types.ts", import.meta.url), "utf8");
const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const types = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
const previous = { minecraftVersion: "1.21.1", loader: "fabric", loaderVersion: "0.16.14", javaMajor: 21, coreVersion: "1.1.7", skinsVersion: "0.3.3" };
const added = { ...previous, minecraftVersion: "1.20.1", javaMajor: 17 };
const legacy = { schemaVersion: 1, instanceScope: "diagnosticCopy", maxPlanSeconds: 60, ...previous };
const current = { schemaVersion: 2, instanceScope: "diagnosticCopy", maxPlanSeconds: 60, runtimes: [previous, added] };
const operation = "11111111-1111-4111-8111-111111111111", plan = "22222222-2222-4222-8222-222222222222";
function reviewed(runtime) {
  return { operationId: operation, generation: 2, planId: plan, phase: "reviewed", result: null, errorCode: null,
    review: { operationId: operation, planId: plan, digest: "a".repeat(64), expiresAtMs: 1800000030000, sessionExpiresAtMs: 1800000060000,
      instanceId: "aurora-test-33333333-3333-4333-8333-333333333333", sourceInstanceId: "source", contextKey: "context",
      javaExecutable: "C:/java/bin/java.exe", nickname: "Player93", appearance: { skinUrl: null, skinFile: null, capeUrl: null, skinModel: null },
      gameAddress: "127.0.0.1:42010", coreSha256: "b".repeat(64), skinsSha256: "c".repeat(64), ...runtime } };
}

test("ready launch schema2 advertises exactly the two explicit existing Fabric profiles", () => {
  assert.deepEqual(types.readyGameCapabilityFrom(current), current);
  assert.deepEqual(types.readyGameCapabilityFrom(legacy), legacy);
  for (const runtimes of [[], [previous], [added], [previous, previous], [previous, added, added],
    [previous, { ...added, javaMajor: 21 }], [previous, { ...added, loader: "forge" }],
    [previous, { ...added, minecraftVersion: "1.19.2" }], [previous, { ...added, loaderVersion: "0.16.15" }]]) {
    assert.throws(() => types.readyGameCapabilityFrom({ ...current, runtimes }));
  }
  assert.throws(() => types.readyGameCapabilityFrom({ ...current, autoLaunch: true }));
  assert.throws(() => types.readyGameCapabilityFrom({ ...legacy, minecraftVersion: "1.20.1", javaMajor: 17 }));
});

test("review metadata must pair the exact Minecraft version with its verified Java major", () => {
  for (const profile of [previous, added]) assert.deepEqual(types.readyGameSnapshotFrom(reviewed(profile)), reviewed(profile));
  for (const profile of [{ ...added, javaMajor: 21 }, { ...previous, javaMajor: 17 }, { ...added, loader: "forge" },
    { ...added, minecraftVersion: "1.16.5", javaMajor: 8 }, { ...added, skinsVersion: "0.3.0" }, { ...added, loaderVersion: "0.16.13" }]) {
    assert.throws(() => types.readyGameSnapshotFrom(reviewed(profile)));
  }
});

test("legacy or missing capability never authorizes the additional runtime", () => {
  assert.equal(types.readyGameCapabilitySupports(legacy, reviewed(previous).review), true);
  assert.equal(types.readyGameCapabilitySupports(legacy, reviewed(added).review), false);
  assert.equal(types.readyGameCapabilitySupports(current, reviewed(added).review), true);
  assert.equal(types.readyGameCapabilitySupports(current, reviewed(previous).review), true);
  assert.equal(types.readyGameCapabilitySupports(undefined, reviewed(added).review), false);
});

test("voice review requires the matching Fabric profile and a complete closed diff", () => {
  const state = reviewed(previous);
  state.review.voice = { profile: "plasmoVoice", mods: [
    { relative: "mods/plasmovoice-fabric-1.21.1-2.1.17.jar", bytes: 12_365_966, sha256: "a".repeat(64), action: "install" },
    { relative: "mods/fabric-api-0.116.10+1.21.1.jar", bytes: 2_425_988, sha256: "b".repeat(64), action: "present" },
  ] };
  assert.deepEqual(types.readyGameSnapshotFrom(state), state);
  for (const changed of [
    { ...state.review, ...added },
    { ...state.review, voice: { ...state.review.voice, profile: "simpleVoiceChat" } },
    { ...state.review, voice: { ...state.review.voice, mods: state.review.voice.mods.slice(0, 1) } },
    { ...state.review, voice: { ...state.review.voice, mods: [{ ...state.review.voice.mods[0], relative: "../outside.jar" }, state.review.voice.mods[1]] } },
  ]) assert.throws(() => types.readyGameSnapshotFrom({ ...state, review: changed }));
});

test("voice review accepts exact legacy Forge and Fabric files for the bound runtime", () => {
  const forge = reviewed({ ...previous, minecraftVersion: "1.12.2", loader: "forge", loaderVersion: "14.23.5.2860", javaMajor: 8 });
  forge.review.voice = { profile: "simpleVoiceChat", mods: [
    { relative: "mods/voicechat-forge-1.12.2-2.6.22.jar", bytes: 5_875_010, sha256: "a".repeat(64), action: "install" },
  ] };
  assert.deepEqual(types.readyGameSnapshotFrom(forge), forge);
  assert.throws(() => types.readyGameSnapshotFrom({ ...forge, review: { ...forge.review, voice: {
    profile: "plasmoVoice", mods: [{ ...forge.review.voice.mods[0], relative: "mods/plasmovoice-forge-1.12.2-2.1.17.jar" }],
  } } }));
  const fabric = reviewed({ ...previous, minecraftVersion: "1.16.5", loaderVersion: "0.13.3", javaMajor: 8 });
  fabric.review.voice = { profile: "plasmoVoice", mods: [
    { relative: "mods/plasmovoice-fabric-1.16.5-2.1.17.jar", bytes: 12_335_242, sha256: "b".repeat(64), action: "install" },
    { relative: "mods/fabric-api-0.42.0+1.16.jar", bytes: 1_090_414, sha256: "c".repeat(64), action: "present" },
  ] };
  assert.deepEqual(types.readyGameSnapshotFrom(fabric), fabric);
  assert.throws(() => types.readyGameSnapshotFrom({ ...fabric, review: { ...fabric.review, voice: {
    ...fabric.review.voice, mods: [{ ...fabric.review.voice.mods[0], bytes: 1 }, fabric.review.voice.mods[1]],
  } } }));
});

test("schema3 requires each exact matrix target and keeps older capabilities closed", async () => {
  const matrix = JSON.parse(await readFile(new URL("../../../../../scripts/active-mod-matrix.json", import.meta.url), "utf8"));
  const runtimes = matrix.targets.filter(target => target.connectReady !== false).map(target => ({
    minecraftVersion: target.minecraft, loader: target.loader, loaderVersion: target.loaderVersion,
    javaMajor: target.javaRuntime, coreVersion: "1.1.7", skinsVersion: "0.3.3",
  }));
  const full = { ...current, schemaVersion: 3, runtimes };
  assert.deepEqual(types.readyGameCapabilityFrom(full), full);
  for (const runtime of runtimes) {
    const state = reviewed(runtime);
    assert.deepEqual(types.readyGameSnapshotFrom(state), state);
    assert.equal(types.readyGameCapabilitySupports(full, state.review), true);
    assert.equal(types.readyGameCapabilitySupports(current, state.review),
      runtime.loader === "fabric" && ["1.20.1", "1.21.1"].includes(runtime.minecraftVersion));
  }
  const [first, second, ...remaining] = runtimes;
  for (const rejected of [
    runtimes.slice(0, -1),
    [second, second, ...remaining],
    [{ ...first, loaderVersion: "14.23.5.2859" }, second, ...remaining],
    [{ ...first, javaMajor: 17 }, second, ...remaining],
  ]) assert.throws(() => types.readyGameCapabilityFrom({ ...full, runtimes: rejected }));
});
