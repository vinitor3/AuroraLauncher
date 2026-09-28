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

const context = { schemaVersion: 1, contextId: "context-a", mods: [{ id: "mod_a", name: "Mod sintético", version: "1", enabled: true, managed: false }], settings: [{ id: "ram", label: "Memória", value: 4096, kind: "number", min: 1024, max: 8192 }], truncated: false, history: [] };
const proposals = [{ kind: "remove_mod", targetId: "mod_a", reason: "Proposta sintética. Não é uma confirmação." }];
const advice = { summary: "Sugestão sintética do Gemini.", proposals, model: "gemini-test" };
const historyEntry = { id: "history-a", kind: "action", createdAtMs: 1700000000000, title: "Alteração sintética", summary: "Memória ajustada na fixture.", issueIds: ["memory_failure"], actionId: "action-a", status: "applied", assessment: "unresolved", undoAvailable: true };
let history = [historyEntry];
let failRecord = false;
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
    if (command === "ai_tools_context") return Promise.resolve(context);
    if (command === "ai_record_advice") return failRecord ? Promise.reject("Histórico indisponível.") : Promise.resolve({});
    if (command === "ai_tool_history") return Promise.resolve(history);
    if (command === "revoke_ai_tool_plan") return Promise.resolve();
    if (command === "prepare_ai_tool" || command === "prepare_ai_tool_undo") return new Promise((resolve, reject) => prepares.push({ command, args, resolve, reject }));
    if (command === "confirm_ai_tool") return new Promise((resolve, reject) => confirms.push({ args, resolve, reject }));
    assert.fail(`Unexpected command ${command}`);
  },
};
window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
const { act, createElement } = await import("react");
const { createRoot } = await import("react-dom/client");
const { AiToolProposalList } = await import(await sourceModule(new URL("./AiToolProposalList.tsx", import.meta.url)));
const { AiToolSuggestions } = await import(await sourceModule(new URL("./AiToolSuggestions.tsx", import.meta.url)));
const container = document.getElementById("root");
const root = createRoot(container);
const button = (label) => { const found = [...document.querySelectorAll("button")].find((item) => item.textContent === label); assert.ok(found, `Missing ${label}`); return found; };
const click = async (label) => { await act(async () => button(label).dispatchEvent(new window.Event("click", { bubbles: true }))); };
const render = async (overrides = {}) => { await act(async () => root.render(createElement(AiToolProposalList, { instanceId: "instance-a", context, proposals, ...overrides }))); };
const plan = (id = "plan-a", overrides = {}) => ({ planId: id, digest: `digest-${id}`, title: "Prévia verificada pelo launcher", changes: [{ label: "Mod local verificado", before: "Presente na instância", after: "Movido para cópia recuperável" }], warnings: ["Alerta local da fixture."], expiresAtMs: Date.now() + 120000, ...overrides });
const prepareReady = async (value = plan()) => { const pending = prepares.shift(); assert.ok(pending); await act(async () => pending.resolve(value)); return pending; };
const confirmDone = async (entry = historyEntry) => { const pending = confirms.shift(); assert.ok(pending); await act(async () => pending.resolve(entry)); return pending; };
const emit = async (name, instanceId = "instance-a") => { await act(async () => { for (const listener of listeners.values()) if (listener.event === name) listener.callback({ payload: { instanceId } }); }); };
const confirmCalls = () => calls.filter((call) => call.command === "confirm_ai_tool").length;

await render();
assert.equal(calls.length, 0, "Rendering suggestions cannot prepare or execute");
await click("Revisar sugestão");
assert.equal(confirmCalls(), 0);
assert.equal(document.querySelector('[role="dialog"]'), null, "Only native-prepared data can open preview");
const prepared = await prepareReady();
assert.deepEqual(prepared.args, { id: "instance-a", contextId: "context-a", proposal: proposals[0] });
const modal = document.querySelector('[role="dialog"]');
assert.match(modal.textContent, /Prévia verificada pelo launcher.*instance-a.*Antes.*Presente.*Depois.*Movido/s);
assert.match(modal.textContent, /Alerta local da fixture/);
assert.doesNotMatch(modal.textContent, /Proposta sintética/);
await click("Cancelar");
assert.equal(confirmCalls(), 0);
assert.equal(document.querySelector('[role="dialog"]'), null);
assert.ok(calls.some((call) => call.command === "revoke_ai_tool_plan" && call.args.planId === "plan-a"));

await click("Revisar sugestão");
await prepareReady(plan("plan-b"));
const confirmButton = button("Confirmar alteração");
await act(async () => { confirmButton.dispatchEvent(new window.Event("click", { bubbles: true })); confirmButton.dispatchEvent(new window.Event("click", { bubbles: true })); });
assert.equal(confirmCalls(), 1, "Double click dispatches exactly one authorized operation");
assert.deepEqual(confirms[0].args, { planId: "plan-b", digest: "digest-plan-b" });
await confirmDone();
assert.match(container.textContent, /Alteração aplicada/);
assert.equal(button("Revisar sugestão").disabled, true, "Old proposals cannot be repeated after mutation");
await click("Atualizar histórico");
assert.match(container.textContent, /mesma categoria de erro reapareceu após a alteração\. A correção ainda não foi confirmada/);
await click("Revisar desfazer");
const undo = await prepareReady(plan("undo-plan", { title: "Desfazer alteração local" }));
assert.equal(undo.command, "prepare_ai_tool_undo");
assert.deepEqual(undo.args, { id: "instance-a", actionId: "action-a" });
assert.equal(confirmCalls(), 1, "Undo also requires a separate confirmation");
await click("Confirmar alteração");
history = [{ ...historyEntry, status: "undone", undoAvailable: false }];
await confirmDone(history[0]);
assert.equal(confirmCalls(), 2);
assert.match(container.textContent, /Alteração desfeita/);

// A prepared response for an earlier instance is revoked and never displayed.
await render({ context: { ...context, contextId: "context-b" } });
await click("Revisar sugestão");
await render({ instanceId: "instance-b", context: { ...context, contextId: "context-c" } });
await prepareReady(plan("stale-plan"));
assert.equal(document.querySelector('[role="dialog"]'), null);
assert.ok(calls.some((call) => call.command === "revoke_ai_tool_plan" && call.args.planId === "stale-plan"));

await render({ context: { ...context, contextId: "context-d" } });
await click("Revisar sugestão");
await prepareReady(plan("event-plan"));
await emit("aurora-doctor-run", "other-instance");
assert.ok(document.querySelector('[role="dialog"]'));
await emit("aurora-doctor-run");
assert.equal(document.querySelector('[role="dialog"]'), null);
assert.equal(button("Revisar sugestão").disabled, true);
assert.equal(confirmCalls(), 2);

await render({ context: { ...context, contextId: "context-e" } });
await click("Revisar sugestão");
await prepareReady(plan("expired-plan", { expiresAtMs: Date.now() - 1 }));
await act(async () => new Promise((resolve) => setTimeout(resolve, 2)));
assert.equal(document.querySelector('[role="dialog"]'), null);
assert.match(container.textContent, /prévia expirou/);
assert.equal(confirmCalls(), 2);

await render({ context: { ...context, contextId: "context-f" } });
await click("Revisar sugestão");
await prepareReady(plan("partial-plan"));
await click("Confirmar alteração");
history = [{ ...historyEntry, status: "pending", assessment: "inconclusive" }];
await confirmDone(history[0]);
assert.match(container.textContent, /Operação pendente/);
assert.doesNotMatch(container.textContent, /Alteração aplicada/);
const authorizedCalls = confirmCalls();

// Explicit cloud request persists only advice and cannot auto-prepare a plan.
const provider = [];
const requestAdvice = (receivedContext, question, signal, report) => new Promise((resolve, reject) => provider.push({ receivedContext, question, signal, report, resolve, reject }));
const report = { schemaVersion: 1, status: "incomplete", summary: "Fixture sintética", instance: { minecraftVersion: "1.20.1", loader: "fabric", javaMajor: 17, requiredJavaMajor: 17, maxMemoryMb: 4096 }, lastRun: null, issues: [], logs: [], limitations: [], sanitizedJson: "{}" };
const renderSuggestions = async (overrides = {}) => { await act(async () => root.render(createElement(AiToolSuggestions, { instanceId: "instance-a", report, requestAdvice, busy: false, ...overrides }))); };
await renderSuggestions();
const contextCallsBefore = calls.filter((call) => call.command === "ai_tools_context").length;
assert.equal(provider.length, 0);
await click("Sugerir correção");
assert.equal(calls.filter((call) => call.command === "ai_tools_context").length, contextCallsBefore + 1);
const cancelled = provider.shift();
await click("Cancelar consulta");
assert.equal(cancelled.signal.aborted, true);
const recordCount = calls.filter((call) => call.command === "ai_record_advice").length;
await act(async () => cancelled.resolve(advice));
assert.equal(calls.filter((call) => call.command === "ai_record_advice").length, recordCount);
await click("Sugerir correção");
const ready = provider.shift();
await act(async () => ready.resolve(advice));
assert.match(container.textContent, /Sugestão sintética do Gemini/);
assert.equal(calls.filter((call) => call.command === "ai_record_advice").length, recordCount + 1);
assert.equal(confirmCalls(), authorizedCalls);
assert.equal(document.querySelector('[role="dialog"]'), null);
assert.equal(prepares.length, 0);

await click("Sugerir correção");
const missingKey = provider.shift();
await act(async () => missingKey.reject({ code: "not_configured" }));
assert.match(container.textContent, /chave do Gemini não está configurada/);
assert.equal(button("Sugerir correção").disabled, false);
await click("Sugerir correção");
const oldReport = provider.shift();
await renderSuggestions({ report: { ...report } });
assert.equal(oldReport.signal.aborted, true);
await act(async () => oldReport.resolve(advice));
assert.doesNotMatch(container.textContent, /Sugestão sintética do Gemini/);
assert.equal(calls.filter((call) => call.command === "ai_record_advice").length, recordCount + 1);
await click("Sugerir correção");
const historyFailure = provider.shift();
failRecord = true;
await act(async () => historyFailure.resolve(advice));
assert.match(container.textContent, /Sugestão sintética do Gemini/);
assert.match(container.textContent, /não foi possível salvá-la no histórico/);
assert.equal(confirmCalls(), authorizedCalls);
await act(async () => root.unmount());
assert.equal(document.querySelector('[role="dialog"]'), null);
assert.equal(listeners.size, 0);

console.log(JSON.stringify({ result: "PASS", fixtures: "synthetic", groups: ["render-does-not-prepare-execute", "native-authoritative-diff-modal", "cancel-revokes-no-execute", "confirm-exact-plan-digest-once", "old-proposals-disabled-after-action", "persistent-unresolved-history", "undo-preview-confirm", "switch-during-prepare-revokes", "run-event-invalidates-only-own-instance", "expired-preview-no-execute", "pending-receipt-is-not-success", "explicit-mod-context-query", "cancel-ignores-late-provider", "advice-record-does-not-execute", "public-key-error-loading-ends", "new-report-ignores-old-advice", "history-failure-preserves-advice", "unmount-cleans-listeners"] }, null, 2));
