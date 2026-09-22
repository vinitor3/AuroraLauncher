// Synthetic UI fixtures only. This does not replace a real Tauri/WebView test.
// Run: node apps/desktop/src/features/doctor/DoctorView.smoke.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";

const require = createRequire(import.meta.url);
const { parseHTML } = require("linkedom");
const { window, document } = parseHTML("<html><body><div id='root'></div></body></html>");
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
let clipboard = "";
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Synthetic React DOM smoke", clipboard: { writeText: async (value) => { clipboard = value; } } } });
const requests = [];
const listeners = new Map();
let callbackId = 0;
let latestRun = null;
window.__TAURI_INTERNALS__ = {
  transformCallback(callback) { listeners.set(++callbackId, callback); return callbackId; },
  invoke(command, args) {
    if (command === "plugin:event|listen") return Promise.resolve(args.handler);
    if (command === "plugin:event|unlisten") { listeners.delete(args.eventId); return Promise.resolve(); }
    if (command === "read_latest_doctor_run") return Promise.resolve(latestRun);
    assert.equal(command, "analyze_instance");
    return new Promise((resolve, reject) => requests.push({ args, resolve, reject }));
  },
};
window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
let exported;
let download;
URL.createObjectURL = (blob) => { exported = blob; return "blob:synthetic-report"; };
URL.revokeObjectURL = () => {};
window.HTMLAnchorElement.prototype.click = function () { download = this.download; };

const { act, createElement } = await import("react");
const { createRoot } = await import("react-dom/client");
const input = await readFile(new URL("./DoctorView.tsx", import.meta.url), "utf8");
let output = ts.transpileModule(input, {
  fileName: "DoctorView.tsx",
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX },
}).outputText.replace('import "./doctor.css";', "");
for (const specifier of ["react/jsx-runtime", "react", "@tauri-apps/api/core", "@tauri-apps/api/event"]) {
  output = output.replaceAll(`from "${specifier}"`, `from ${JSON.stringify(pathToFileURL(require.resolve(specifier)).href)}`);
}
const { DoctorView } = await import(`data:text/javascript;base64,${Buffer.from(output).toString("base64")}`);
const container = document.getElementById("root");
const root = createRoot(container);
const props = { instanceId: "synthetic-a", javaExecutable: "synthetic-java-17", busy: false, onSelectRun: () => {} };
const button = (label) => {
  const element = [...container.querySelectorAll("button")].find((item) => item.textContent === label);
  assert.ok(element, `Missing button: ${label}`);
  return element;
};
const click = async (label) => { await act(async () => button(label).dispatchEvent(new window.Event("click", { bubbles: true }))); };
const render = async (overrides = {}) => { await act(async () => root.render(createElement(DoctorView, { ...props, ...overrides }))); };
const report = (overrides = {}) => ({
  schemaVersion: 1,
  status: "no_issues",
  summary: "Fixture sintética sem problema reconhecido.",
  instance: { minecraftVersion: "1.20.1", loader: "fabric", javaMajor: 17, requiredJavaMajor: 17, maxMemoryMb: 4096 },
  lastRun: { outcome: "success", exitCode: 0, runId: "9ba91227-cf20-4417-9b5f-88b530e5af19", startedAtMs: 1700000000000, endedAtMs: 1700000001000 },
  issues: [], logs: [], limitations: ["Fixture sintética; sem execução do Minecraft."], sanitizedJson: '{"synthetic":true}',
  ...overrides,
});
const finish = async (result) => { const request = requests.shift(); assert.ok(request); await act(async () => request.resolve(result)); return request; };
const emitRun = async (run) => { await act(async () => { for (const listener of listeners.values()) listener({ payload: run }); }); };

await render();
assert.match(container.textContent, /Entenda o que aconteceu/);
await click("Analisar instância");
assert.match(container.textContent, /Analisando os dados locais/);
assert.equal(button("Analisando…").disabled, true);
assert.equal(requests[0].args.id, "synthetic-a");
assert.equal(requests[0].args.javaExecutable, "synthetic-java-17");
await render({ instanceId: "synthetic-b" });
await finish(report({ summary: "STALE_A_MUST_NOT_APPEAR" }));
assert.doesNotMatch(container.textContent, /STALE_A_MUST_NOT_APPEAR/);
await click("Analisar instância");
await finish(report({ status: "no_logs", summary: "Fixture sintética sem logs.", lastRun: null }));
assert.match(container.textContent, /Nenhum log disponível/);

await render({ instanceId: "synthetic-b", javaExecutable: "synthetic-java-21" });
assert.doesNotMatch(container.textContent, /Nenhum log disponível/);
await click("Analisar instância");
const failedRequest = requests.shift();
await act(async () => failedRequest.reject("SYNTHETIC_PRIVATE_ERROR_NOT_FOR_UI"));
assert.match(container.textContent, /Não foi possível concluir/);
assert.doesNotMatch(container.textContent, /SYNTHETIC_PRIVATE_ERROR_NOT_FOR_UI/);
await click("Analisar instância");
await finish(report());
assert.match(container.textContent, /Nenhum problema detectado/);
assert.match(container.textContent, /Encerramento normal/);
assert.match(container.textContent, /9ba91227-cf20-4417-9b5f-88b530e5af19/);
await click("Copiar relatório");
assert.equal(clipboard, '{"synthetic":true}');
await click("Exportar JSON");
assert.equal(await exported.text(), clipboard);
assert.equal(download, "aurora-doctor-relatorio.json");
await emitRun({ instanceId: "synthetic-other", runId: "different-run", outcome: "failed" });
assert.match(container.textContent, /Nenhum problema detectado/);
await emitRun({ instanceId: "synthetic-b", runId: "new-run", outcome: "preparing" });
assert.doesNotMatch(container.textContent, /Nenhum problema detectado/);
await click("Analisar instância");
await emitRun({ instanceId: "synthetic-b", runId: "new-run", outcome: "cancelled" });
await finish(report({ summary: "STALE_SAME_INSTANCE_AFTER_CANCEL" }));
assert.doesNotMatch(container.textContent, /STALE_SAME_INSTANCE_AFTER_CANCEL/);
assert.equal([...container.querySelectorAll("button")].some((item) => item.textContent === "Analisar a última execução com falha"), false);

let selectedRun;
latestRun = { instanceId: "synthetic-c", runId: "run-c", outcome: "failed" };
await render({ instanceId: "synthetic-c", onSelectRun: (runId) => { selectedRun = runId; } });
await click("Analisar a última execução com falha");
assert.equal(selectedRun, "run-c");
await render({ instanceId: "synthetic-c", runId: "run-c" });
await click("Analisar instância");
assert.equal(requests[0].args.runId, "run-c");
await finish(report({
  status: "issues",
  summary: "Problema sintético.",
  lastRun: { outcome: "failed", exitCode: 1, runId: null, startedAtMs: null, endedAtMs: null },
  issues: [{ id: "synthetic-memory", title: "Memória insuficiente", severity: "error", explanation: "Fixture sintética de falha explícita.", evidence: [{ source: "JVM", summary: "OutOfMemoryError reconhecido." }], confidence: "high", kind: "fact", nextStep: "Revise a memória configurada." }],
  logs: [{ source: "crash histórico", association: "historical", bytesRead: 128, truncated: true }],
}));
assert.equal(container.querySelectorAll("details.doctor-issue").length, 1);
assert.match(container.textContent, /Fato observado/);
assert.match(container.textContent, /Confiança alta/);
assert.match(container.textContent, /Próximo passo/);
assert.match(container.textContent, /Histórico — não comprova a causa desta execução/);
assert.match(container.textContent, /leitura parcial/);
await click("Analisar instância");
const pendingAtUnmount = requests.shift();
await act(async () => root.unmount());
await act(async () => pendingAtUnmount.resolve(report()));
assert.equal(container.textContent, "");

console.log(JSON.stringify({ result: "PASS", fixtures: "synthetic", checks: ["loading", "instance-switch-stale-response", "java-change-invalidates-report", "no-logs", "analysis-error-redacted", "no-issues-normal-exit", "copy-export-sanitized-only", "run-events-invalidate-only-own-instance", "cancelled-run-is-not-failure", "failed-run-link", "evidence-confidence-next-step", "historical-log-label", "unmounted-response"], source: fileURLToPath(import.meta.url) }, null, 2));
