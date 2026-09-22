import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";

// Execute the real handlers with controlled network completion, including repeated
// requests for the same project. No timers are involved in these ordering checks.
const source = ts.createSourceFile("App.tsx", readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
const app = source.statements.find((node) => ts.isFunctionDeclaration(node) && node.name?.text === "LauncherApp");
const declarations = ["closeProjectDetail", "openContentDetail", "openModpackDetail"].map((name) => {
  const declaration = app.body.statements.find((node) => ts.isFunctionDeclaration(node) && node.name?.text === name);
  assert.ok(declaration, `Missing real App handler ${name}`);
  return declaration.getText(source);
});
const compiled = ts.transpileModule(declarations.join("\n"), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None } }).outputText;

function setup() {
  const state = { detail: undefined, loading: false, notice: "Aviso anterior" };
  const events = [];
  const requests = [];
  function request(source) {
    return new Promise((resolve, reject) => requests.push({ source, resolve, reject }));
  }
  const context = vm.createContext({
    services: {},
    projectDetailRequestId: { current: 0 },
    normalizeProjectMarkdown: (value) => value,
    fetch: () => request("modrinth"),
    requestCurseForgeCatalog: () => request("curseforge"),
    setProjectDetail: (value) => { state.detail = typeof value === "function" ? value(state.detail) : value; },
    setDetailLoading: (value) => { state.loading = value; events.push(["loading", value]); },
    setNotice: (value) => { state.notice = value; events.push(["notice", value]); },
  });
  vm.runInContext(compiled, context);
  return {
    state, events,
    close: () => context.closeProjectDetail(),
    open(kind, source, projectId = "project-a") {
      const before = requests.length;
      const project = { projectId, source, title: projectId, description: `${projectId} summary`, curseForgeId: before + 100 };
      const completed = kind === "content" ? context.openContentDetail(project) : context.openModpackDetail(project);
      assert.equal(requests.length, before + 1, "the real handler must request its full description");
      const pending = requests.at(-1);
      assert.equal(pending.source, source);
      return {
        async succeed(body) {
          pending.resolve(source === "modrinth" ? { ok: true, json: async () => ({ body }) } : { data: body });
          await completed;
        },
        async fail() {
          pending.reject(new Error("Falha de rede controlada"));
          await completed;
        },
      };
    },
  };
}

for (const source of ["modrinth", "curseforge"]) {
  for (const kind of ["content", "modpack"]) {
    const label = `${source} ${kind}`;

    test(`${label}: an older project cannot finish the active project's loading`, async () => {
      const run = setup();
      const first = run.open(kind, source, "project-a");
      const second = run.open(kind, source, "project-b");
      const eventCount = run.events.length;
      await first.succeed("A full description");
      assert.equal(run.state.detail.title, "project-b");
      assert.equal(run.state.detail.body, "project-b summary");
      assert.equal(run.state.loading, true);
      assert.equal(run.events.length, eventCount);
      await second.succeed("B full description");
      assert.equal(run.state.detail.body, "B full description");
      assert.equal(run.state.loading, false);
    });

    test(`${label}: reopening the same project rejects the previous body`, async () => {
      const run = setup();
      const first = run.open(kind, source);
      run.close();
      const reopened = run.open(kind, source);
      await reopened.succeed("Current full description");
      const eventCount = run.events.length;
      await first.succeed("Obsolete full description");
      assert.equal(run.state.detail.body, "Current full description");
      assert.equal(run.state.loading, false);
      assert.equal(run.events.length, eventCount);
    });

    test(`${label}: an obsolete failure cannot interrupt the active request`, async () => {
      const run = setup();
      const first = run.open(kind, source);
      const second = run.open(kind, source);
      const eventCount = run.events.length;
      await first.fail();
      assert.equal(run.state.notice, "Aviso anterior");
      assert.equal(run.state.loading, true);
      assert.equal(run.events.length, eventCount);
      await second.succeed("Current full description");
      assert.equal(run.state.detail.body, "Current full description");
      assert.equal(run.state.loading, false);
    });

    for (const outcome of ["succeed", "fail"]) {
      test(`${label}: closing invalidates a pending request that will ${outcome}`, async () => {
        const run = setup();
        const pending = run.open(kind, source);
        run.close();
        assert.equal(run.state.detail, undefined);
        assert.equal(run.state.loading, false);
        const eventCount = run.events.length;
        await pending[outcome]("Closed project description");
        assert.equal(run.state.detail, undefined);
        assert.equal(run.state.loading, false);
        assert.equal(run.state.notice, "Aviso anterior");
        assert.equal(run.events.length, eventCount);
      });
    }

    test(`${label}: a current failure still reports the error and stops loading`, async () => {
      const run = setup();
      const pending = run.open(kind, source);
      await pending.fail();
      assert.equal(run.state.detail.body, "project-a summary");
      assert.equal(run.state.loading, false);
      assert.match(run.state.notice, /A descrição completa não pôde ser carregada.*Falha de rede controlada/);
    });
  }
}

for (const [firstKind, nextKind] of [["content", "modpack"], ["modpack", "content"]]) {
  test(`switching from ${firstKind} to ${nextKind} keeps the active description loading`, async () => {
    const run = setup();
    const first = run.open(firstKind, "modrinth", "project-a");
    run.close();
    const second = run.open(nextKind, "curseforge", "project-b");
    await first.succeed("A full description");
    assert.equal(run.state.detail.kind, nextKind);
    assert.equal(run.state.detail.body, "project-b summary");
    assert.equal(run.state.loading, true);
    await second.succeed("B full description");
    assert.equal(run.state.detail.body, "B full description");
    assert.equal(run.state.loading, false);
  });
}
