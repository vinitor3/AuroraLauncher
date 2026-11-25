import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";

const source = ts.createSourceFile("App.tsx", readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
const launcher = source.statements.find((node) => ts.isFunctionDeclaration(node) && node.name?.text === "LauncherApp");
const search = launcher.body.statements.find((node) => ts.isFunctionDeclaration(node) && node.name?.text === "loadModrinthPage");
const js = ts.transpileModule(search.getText(source), { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;

function fixture() {
  const pending = [], state = { packs: [], versions: {}, loading: false, page: 0, total: 0, error: "" };
  const context = vm.createContext({
    catalogRequest: { current: 0 }, modrinthPageSize: 10, catalogSource: "modrinth", modpackCategoryFilter: "",
    modpackQuery: "", modpackVersionFilter: "1.21.11", modpackLoaderFilter: "", modpackSort: "popular", minecraftVersion: "1.20.1",
    loadCatalogCategories: () => {}, preferredPackVersion: (pack, preferred) => pack.versions.includes(preferred) ? preferred : pack.versions[0],
    invoke: (_, args) => new Promise((resolve, reject) => pending.push({ args, resolve, reject })),
    setModrinthPacks: (value) => { state.packs = value; }, setPackVersions: (fn) => { state.versions = fn(state.versions); },
    setModrinthPage: (value) => { state.page = value; }, setModrinthTotalHits: (value) => { state.total = value; },
    setCatalogLoading: (value) => { state.loading = value; }, setCatalogError: (value) => { state.error = value; },
    errorMessage: (error) => error.message,
  });
  vm.runInContext(js, context);
  const page = (id) => ({ items: [{ projectId: id, versions: ["1.20.1", "1.21.11"] }], totalHits: 53 });
  return { context, pending, state, page };
}

test("a late page or failure cannot replace a newer catalog query or clear its loading indicator", async () => {
  for (const fail of [false, true]) {
    const { context, pending, state, page } = fixture();
    const old = context.loadModrinthPage(1);
    const next = context.loadModrinthPage(0);
    if (fail) pending[0].reject(new Error("offline"));
    else pending[0].resolve(page("stale"));
    await old;
    assert.equal(state.loading, true);
    assert.equal(state.packs.length, 0);
    pending[1].resolve(page("current"));
    await next;
    assert.equal(state.loading, false);
    assert.equal(state.packs[0].projectId, "current");
    assert.equal(state.versions.current, "1.21.11");
    assert.equal(state.error, "");
  }
});

test("failed pagination removes obsolete results and counts, leaving search available", async () => {
  const { context, pending, state, page } = fixture();
  const first = context.loadModrinthPage(1);
  pending[0].resolve(page("first")); await first;
  assert.equal(state.page, 1);
  const second = context.loadModrinthPage(2);
  assert.equal(pending[1].args.offset, 20);
  pending[1].reject(new Error("offline")); await second;
  assert.equal(state.page, 0);
  assert.equal(state.total, 0);
  assert.equal(state.packs.length, 0);
  assert.equal(state.loading, false);
  assert.match(state.error, /Modrinth.*offline/);
});
test("o catálogo instala no identificador reservado pelo motor, mesmo que outro launcher tenha criado o nome inicial", async () => {
  const create = launcher.body.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === "createAndInstallModrinth");
  const compiled = ts.transpileModule(create.getText(source), { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
  const installs = [], selections = [];
  const context = vm.createContext({
    minecraftVersion: "1.20.1", packVersions: { pack: "1.21.1" }, safeInstanceId: s => s,
    targetInstance: "mundo-existente", setBusy() {}, setNotice() {}, setDownloadProgress() {},
    invoke: async (command, args) => {
      assert.equal(command, "create_instance");
      assert.equal(args.unique, true);
      assert.equal(args.id, "pack-1.21.1");
      return { id: "pack-1.21.1-2" };
    },
    installPackIntoInstance: async (pack, id) => {
      installs.push(id);
      return { name: pack.title, minecraft: { versionId: "fabric" }, minecraftVersion: "1.21.1", loader: "fabric" };
    },
    setTargetInstance: id => selections.push(id), setInstalledVersionId() {}, setInstalledMinecraftVersion() {},
    setMinecraftVersion() {}, completeInstanceInstall() {},
  });
  vm.runInContext(compiled, context);
  await context.createAndInstallModrinth({ projectId: "pack", slug: "pack", title: "Pack" });
  assert.deepEqual(installs, ["pack-1.21.1-2"]);
  assert.deepEqual(selections, installs);
});
