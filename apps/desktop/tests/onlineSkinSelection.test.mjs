import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";
import { publicCatalogSource, skinSha256 } from "../src/lib/skinCatalogSources.ts";

const source = ts.createSourceFile("App.tsx", readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
const app = source.statements.find((node) => ts.isFunctionDeclaration(node) && node.name?.text === "LauncherApp");
const handler = app.body.statements.find((node) => ts.isFunctionDeclaration(node) && node.name?.text === "selectOnlineSkin").getText(source);
const skin = { name: "Public Player", skinUrl: "https://example.com/skin.png", model: "slim", contentHash: await skinSha256(new Blob(["PNG"])) };
function setup(overrides = {}) {
  const events = [];
  const context = vm.createContext({
    profile: { uid: "test-owner" }, busy: false, wardrobeLoading: false,
    isCurrentSkinOwner: () => true,
    selectedSkinIdRef: { current: "previous" },
    setSelectedLocalSkinId: (value) => events.push(["selected", value]),
    invoke: async (command, args) => { events.push([command, args]); return { url: skin.skinUrl, dataBase64: "PNG" }; },
    appearanceBlob: async () => new Blob(["PNG"]),
    publicCatalogSource, skinSha256,
    saveLocalSkin: async (record) => { events.push(["save", record]); return { ...record, id: "local-skin" }; },
    refreshLocalSkinLibrary: async (...args) => events.push(["refresh", args]),
    setSkinModel: (value) => events.push(["model", value]),
    setFavoriteName: (value) => events.push(["name", value]),
    setNotice: (value) => events.push(["notice", value]),
    setBusy: (value) => events.push(["busy", value]),
    errorMessage: (error) => error.message,
    ...overrides,
  });
  vm.runInContext(ts.transpileModule(handler, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None } }).outputText, context);
  return { context, events };
}

test("online selection downloads the PNG and persists its model before offering Apply", async () => {
  const run = setup();
  await run.context.selectOnlineSkin(skin);
  const saved = run.events.find(([kind]) => kind === "save")[1];
  assert.equal(saved.ownerId, "test-owner");
  assert.equal(saved.skinModel, "slim");
  assert.equal(run.context.selectedSkinIdRef.current, "local-skin");
  assert.ok(run.events.findIndex(([kind]) => kind === "selected") < run.events.findIndex(([kind]) => kind === "refresh"));
  assert.equal(await saved.image.text(), "PNG");
  assert.ok(run.events.some(([kind, value]) => kind === "model" && value === "slim"));
  assert.match(run.events.filter(([kind]) => kind === "notice").at(-1)[1], /Aplicar skin/);
  assert.equal(run.events.filter(([kind]) => kind === "load_appearance_url").length, 1);
  assert.ok(!run.events.some(([kind]) => kind === "save_local_appearance"));
});

test("a failed online download preserves the selected skin and unlocks the UI", async () => {
  const run = setup({ invoke: async () => { throw new Error("Serviço indisponível"); } });
  await run.context.selectOnlineSkin(skin);
  assert.ok(!run.events.some(([kind]) => ["save", "refresh", "model"].includes(kind)));
  assert.match(run.events.filter(([kind]) => kind === "notice").at(-1)[1], /Não foi possível baixar/);
  assert.deepEqual(run.events.at(-1), ["busy", false]);
});

test("account changes during the download cannot save a skin into the previous account", async () => {
  const run = setup({ isCurrentSkinOwner: () => false });
  await run.context.selectOnlineSkin(skin);
  assert.ok(!run.events.some(([kind]) => kind === "save"));
  assert.deepEqual(run.events.at(-1), ["busy", false]);
});
