import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import test from "node:test";
import { pathToFileURL } from "node:url";
import { parseHTML } from "linkedom";
import ts from "typescript";

test("painel do mundo aplica o escopo, bloqueia restauração ativa e descarta consultas antigas", async () => {
  const { window, document } = parseHTML("<html><body><div id='root'></div></body></html>");
  Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
  const calls = [];
  window.__TAURI_INTERNALS__ = {
    invoke(command, args) { return new Promise((resolve, reject) => calls.push({ command, args, resolve, reject })); },
  };
  const require = createRequire(import.meta.url);
  const source = await readFile(new URL("../src/features/world-backups/WorldBackupsPanel.tsx", import.meta.url), "utf8");
  const js = ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX },
  }).outputText.replace(/^import "[^"]+\.css";$/gm, "").replaceAll(/from "([^"]+)"/g, (_, name) => "from " + JSON.stringify(pathToFileURL(require.resolve(name)).href));
  const { WorldBackupsPanel } = await import("data:text/javascript;base64," + Buffer.from(js).toString("base64"));
  const { act, createElement } = await import("react");
  const { createRoot } = await import("react-dom/client");
  const root = createRoot(document.getElementById("root"));
  const render = async (instanceId, running) => act(async () => root.render(createElement(WorldBackupsPanel, { instanceId, running })));
  const next = (command, args) => {
    const request = calls.shift();
    assert.equal(request?.command, command);
    assert.deepEqual(request.args, args);
    return request;
  };
  const data = { pending: false, globalEnabled: true, instanceEnabled: null, lastError: null, versions: [
    { id: "v2", world: "Teste", createdAt: 123, files: 2, bytes: 44 },
    { id: "v1", world: "Teste", createdAt: 120, files: 1, bytes: 40 },
  ], storedBytes: 60 };
  const get = (id) => next("world_backup_overview", { id });
  const finish = (call, value) => act(async () => call.resolve(value));
  const click = async (element) => act(async () => element.dispatchEvent(new window.Event("click", { bubbles: true })));
  const button = text => [...document.querySelectorAll("button")].find(node => node.textContent === text);
  const change = async (select, value) => act(async () => {
    select.querySelector('option[value="' + value + '"]').selected = true;
    select.dispatchEvent(new window.Event("change", { bubbles: true }));
  });
  try {
    await render("a", true);
    await finish(get("a"), data);
    assert.equal(button("Restaurar como novo mundo").disabled, true);
    await render("a", false);
    await finish(get("a"), data);
    await change(document.querySelector("select"), "false");
    await finish(next("world_backup_configure", { id: "a", global: false, enabled: false }), null);
    await finish(get("a"), { ...data, instanceEnabled: false });
    assert.match(document.body.textContent, /Preferência da instância salva/);
    await change(document.querySelectorAll("select")[2], "v1");
    await click(button("Restaurar como novo mundo"));
    await finish(next("world_backup_restore", { id: "a", version: "v1" }), "Copia");
    const stale = get("a");
    assert.match(document.body.textContent, /Mundo restaurado na pasta Copia/);
    await render("b", false);
    await finish(get("b"), { ...data, pending: true, lastError: "Falta espaço.", versions: [] });
    await finish(stale, data);
    assert.match(document.body.textContent, /Falta espaço/);
    assert.doesNotMatch(document.body.textContent, /Mundo restaurado na pasta Copia/);
    await click(button("Tentar backup novamente"));
    await finish(next("world_backup_retry", { id: "b" }), null);
    await finish(get("b"), { ...data, versions: [] });
    assert.match(document.body.textContent, /Ainda não há versões/);
    assert.equal(calls.length, 0);
  } finally {
    await act(async () => root.unmount());
  }
});
