// Synthetic DOM/native/provider fixtures. No instance files or cloud services are used.
// Run: node apps/desktop/src/features/ai-tools/AiTools.smoke.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";

const require = createRequire(import.meta.url);
const { parseHTML } = require("linkedom");
const { window, document } = parseHTML("<html><body><div id='root'></div></body></html>");
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Synthetic DOM smoke" } });
const moduleUrls = new Map();
async function sourceModule(sourceUrl) {
  if (moduleUrls.has(sourceUrl.href)) return moduleUrls.get(sourceUrl.href);
  let output = ts.transpileModule(await readFile(sourceUrl, "utf8"), { fileName: fileURLToPath(sourceUrl), compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX } }).outputText.replace(/^import ["'][^"']+\.css["'];?$/gm, "");
  for (const { fileName: specifier } of ts.preProcessFile(output).importedFiles) {
    const dependency = specifier.startsWith(".") ? ["", ".ts", ".tsx"].map((ext) => new URL(specifier + ext, sourceUrl)).find((url) => existsSync(fileURLToPath(url))) : undefined;
    const resolved = dependency ? await sourceModule(dependency) : pathToFileURL(require.resolve(specifier)).href;
    output = output.replaceAll(`from "${specifier}"`, `from ${JSON.stringify(resolved)}`);
  }
  const url = `data:text/javascript;base64,${Buffer.from(output).toString("base64")}`;
  moduleUrls.set(sourceUrl.href, url); return url;
}

const historyEntry = { id: "history-a", kind: "action", createdAtMs: 1700000000000, title: "Alteração sintética", summary: "Memória ajustada na fixture.", issueIds: ["memory_failure"], actionId: "action-a", status: "applied", assessment: "unresolved", undoAvailable: true };
let history = [historyEntry];
const calls = [];
const prepares = [];
const confirms = [];
const callbacks = new Map();
const listeners = new Map();
let callbackId = 0;
window.__TAURI_INTERNALS__ = {
  transformCallback(callback) { callbacks.set(++callbackId, callback); return callbackId; },
  invoke(command, args) {
    if (command === "plugin:event|listen") { listeners.set(args.handler, { event: args.event, callback: callbacks.get(args.handler) }); return Promise.resolve(args.handler); }
    if (command === "plugin:event|unlisten") { listeners.delete(args.eventId); return Promise.resolve(); }
    calls.push({ command, args });
    if (command === "ai_tool_history") return Promise.resolve(history);
    if (command === "revoke_ai_tool_plan") return Promise.resolve();
    if (command === "prepare_ai_tool_undo") return new Promise((resolve, reject) => prepares.push({ command, args, resolve, reject }));
    if (command === "confirm_ai_tool") return new Promise((resolve, reject) => confirms.push({ args, resolve, reject }));
    assert.fail(`Unexpected command ${command}`);
  },
};
window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
const { act, createElement } = await import("react");
const { createRoot } = await import("react-dom/client");
const { InstanceHistory } = await import(await sourceModule(new URL("./AiToolProposalList.tsx", import.meta.url)));
const container = document.getElementById("root");
const root = createRoot(container);
const button = (label) => { const found = [...document.querySelectorAll("button")].find((item) => item.textContent === label); assert.ok(found, `Missing ${label}`); return found; };
const click = async (label) => { await act(async () => button(label).dispatchEvent(new window.Event("click", { bubbles: true }))); };
const render = async (overrides = {}) => { await act(async () => root.render(createElement(InstanceHistory, { instanceId: "instance-a", ...overrides }))); };
const plan = (id = "plan-a", overrides = {}) => ({ planId: id, digest: `digest-${id}`, title: "Prévia verificada pelo launcher", changes: [{ label: "Mod local verificado", before: "Presente na instância", after: "Movido para cópia recuperável" }], warnings: ["Alerta local da fixture."], expiresAtMs: Date.now() + 120000, ...overrides });
const prepareReady = async (value = plan()) => { const pending = prepares.shift(); assert.ok(pending); await act(async () => pending.resolve(value)); return pending; };
const confirmDone = async (entry = historyEntry) => { const pending = confirms.shift(); assert.ok(pending); await act(async () => pending.resolve(entry)); return pending; };
const confirmCalls = () => calls.filter((call) => call.command === "confirm_ai_tool").length;

await render();
await click("Atualizar histórico");
assert.match(container.textContent, /mesma categoria de erro reapareceu após a alteração\. A correção ainda não foi confirmada/);
await click("Revisar desfazer");
const undo = await prepareReady(plan("undo-plan", { title: "Desfazer alteração local" }));
assert.equal(undo.command, "prepare_ai_tool_undo");
assert.deepEqual(undo.args, { id: "instance-a", actionId: "action-a" });
assert.equal(confirmCalls(), 0, "Undo also requires a separate confirmation");
await click("Confirmar alteração");
history = [{ ...historyEntry, status: "undone", undoAvailable: false }];
await confirmDone(history[0]);
assert.equal(confirmCalls(), 1);
assert.match(container.textContent, /Alteração desfeita/);

await act(async () => root.unmount());
assert.equal(listeners.size, 0);
console.log("PASS · histórico e restauração local com confirmação");
