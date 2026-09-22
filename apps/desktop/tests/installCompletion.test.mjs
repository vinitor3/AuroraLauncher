import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";

const source = ts.createSourceFile("App.tsx", readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
const app = source.statements.find((node) => ts.isFunctionDeclaration(node) && node.name?.text === "LauncherApp");
function setup(failInstall = false, failRefresh = false) {
  const events = [];
  const declarations = ["completeInstanceInstall", "createAndInstallModrinth"].map((name) => app.body.statements.find((node) => ts.isFunctionDeclaration(node) && node.name?.text === name).getText(source));
  const globals = {
    instances: [], packVersions: {}, minecraftVersion: "1.20.1",
    preferredPackVersion: () => "1.20.1", safeInstanceId: () => "test-pack",
    invoke: async () => ({ id: "test-pack" }),
    installPackIntoInstance: async () => {
      if (failInstall) throw new Error("Download interrompido");
      return { name: "Test Pack", minecraftVersion: "1.20.1", loader: "fabric", minecraft: { versionId: "fabric-test" } };
    },
    refresh: async () => { if (failRefresh) throw new Error("Listagem indisponível"); },
    closeInstanceEditor: () => events.push(["closeEditor"]),
  };
  for (const name of ["setTargetInstance", "setProjectDetail", "setShowCreateModal", "setInstanceFilter", "setActiveSection", "setDownloadProgress", "setNotice", "setBusy", "setInstalledVersionId", "setInstalledMinecraftVersion", "setMinecraftVersion"]) globals[name] = (value) => events.push([name, value]);
  const context = vm.createContext(globals);
  vm.runInContext(ts.transpileModule(declarations.join("\n"), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None } }).outputText, context);
  return { context, events };
}

test("completed modpack installation reveals the selected instance and closes overlapping views", async () => {
  const run = setup();
  await run.context.createAndInstallModrinth({ title: "Test Pack" });
  for (const expected of [["setActiveSection", "instances"], ["setTargetInstance", "test-pack"], ["setProjectDetail", undefined], ["setInstanceFilter", ""], ["closeEditor"]]) {
    assert.ok(run.events.some((event) => JSON.stringify(event) === JSON.stringify(expected)));
  }
  assert.match(run.events.filter(([name]) => name === "setNotice").at(-1)[1], /instalado!/);
});

test("a failed installation stays on the source view and never announces success", async () => {
  const run = setup(true);
  await run.context.createAndInstallModrinth({ title: "Test Pack" });
  assert.ok(!run.events.some(([name]) => name === "setActiveSection"));
  assert.match(run.events.filter(([name]) => name === "setNotice").at(-1)[1], /Não foi possível preparar/);
});

test("a refresh failure after successful installation does not claim the download failed", async () => {
  const run = setup(false, true);
  await run.context.createAndInstallModrinth({ title: "Test Pack" });
  assert.match(run.events.filter(([name]) => name === "setNotice").at(-1)[1], /instalação foi concluída.*lista não pôde/);
  assert.deepEqual(run.events.at(-1), ["setBusy", false]);
});
