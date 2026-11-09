// Synthetic DOM/IPC only; no Java, account, network or file mutation.
import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";
const require = createRequire(import.meta.url);
const { window, document } = require("linkedom").parseHTML("<html><body><div id='root'></div></body></html>");
document.oninput = null;
Object.defineProperty(window.HTMLInputElement.prototype, "type", { configurable: true, get() { return this.getAttribute("type") || "text"; }, set(value) { this.setAttribute("type", value); } });
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Native voice synthetic DOM" } });
const modules = new Map();
async function sourceModule(source) {
  if (modules.has(source.href)) return modules.get(source.href);
  let code = ts.transpileModule(await readFile(source, "utf8"), { fileName: fileURLToPath(source), compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX } }).outputText;
  for (const { fileName: specifier } of ts.preProcessFile(code).importedFiles) {
    const dependency = specifier.startsWith(".") ? ["", ".ts", ".tsx"].map(ext => new URL(specifier + ext, source)).find(url => existsSync(fileURLToPath(url))) : undefined;
    const resolved = dependency ? await sourceModule(dependency) : pathToFileURL(require.resolve(specifier)).href;
    code = code.replaceAll(`from "${specifier}"`, `from ${JSON.stringify(resolved)}`);
  }
  const url = `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`;
  modules.set(source.href, url); return url;
}
let calls, root, snapshot, handle, clock, frames, nextFrame;
const realPerformance = globalThis.performance;
const originalRequestFrame = window.requestAnimationFrame, originalCancelFrame = window.cancelAnimationFrame;
window.isTauri = true;
window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
  calls.push({ command, args });
  if (command === "native_voice_snapshot") return structuredClone(snapshot);
  if (command === "native_voice_execute") return handle(args.request);
  throw new Error("Unexpected command");
} };
const { act, createElement } = await import("react");
const { createRoot } = await import("react-dom/client");
const { NativeVoicePanel } = await import(await sourceModule(new URL("./NativeVoicePanel.tsx", import.meta.url)));
const { voiceRuntimes } = await import(await sourceModule(new URL("./types.ts", import.meta.url)));
const container = document.getElementById("root");
const idle = () => ({ revision: 1, available: true, manualLabAvailable: false, phase: "idle", profile: null, runtimeProfile: null, review: null, operationId: null, hostPublicId: null, participants: [], udpState: "notStarted", tcpAddress: null, negotiatedDatagramBytes: null, recoveryPending: false, eulaRecoveryAvailable: false, recoverableCopies: [], eulaState: "unchanged", errorCode: null, exportedInvitation: null, audioState: "notVerified" });
const review = (kind = "execution", remainingMs = 30_000) => ({ kind, digest: "a".repeat(64), remainingMs, before: "Cópia com EULA false; nenhum processo", after: "Executar o dedicado próprio; restaurar EULA após término", details: ["Minecraft 1.21.1 / Fabric / Java 21", JSON.stringify({ cwd: "C:/owned-copy", args: ["compiled-only"], executable: "C:/owned-java" })] });
const button = label => { const result = [...container.querySelectorAll("button")].find(node => node.textContent === label); assert.ok(result, label); return result; };
const render = async (props = {}) => { await act(async () => root.render(createElement(NativeVoicePanel, props))); };
const paint = async () => { const pending = [...frames.values()]; frames.clear(); await act(async () => { for (const frame of pending) frame(clock); }); };
const click = async label => { assert.equal(button(label).disabled, false); await act(async () => button(label).dispatchEvent(new window.Event("click", { bubbles: true }))); };
const approve = async () => { const input = [...container.querySelectorAll("input[type=checkbox]")].find(node => node.parentElement.textContent.includes("Revisei")); assert.ok(input); await act(async () => { input.checked = true; input.dispatchEvent(new window.Event("click", { bubbles: true })); }); };
const poll = async () => { await act(async () => document.dispatchEvent(new window.Event("visibilitychange"))); };
beforeEach(() => { calls = []; snapshot = idle(); clock = 1000; frames = new Map(); nextFrame = 0; window.requestAnimationFrame = callback => { frames.set(++nextFrame, callback); return nextFrame; }; window.cancelAnimationFrame = id => frames.delete(id); Object.defineProperty(globalThis, "performance", { configurable: true, value: new Proxy(realPerformance, { get(target, key) { if (key === "now") return () => clock; const value = Reflect.get(target, key, target); return typeof value === "function" ? value.bind(target) : value; } }) }); handle = async () => snapshot; root = createRoot(container); });
afterEach(async () => { await act(async () => root.unmount()); Object.defineProperty(globalThis, "performance", { configurable: true, value: realPerformance }); window.requestAnimationFrame = originalRequestFrame; window.cancelAnimationFrame = originalCancelFrame; });

test("Social navigation waits for the preceding Connect layout, then focuses and scrolls the existing heading once", async () => {
  const focus = window.HTMLElement.prototype.focus;
  const scroll = window.HTMLElement.prototype.scrollIntoView;
  const focused = [], scrolled = [];
  window.HTMLElement.prototype.focus = function () { focused.push(this); };
  window.HTMLElement.prototype.scrollIntoView = function (options) { scrolled.push({ target: this, options }); };
  try {
    await render({ revealRequest: 1, revealReady: false });
    await paint();
    assert.equal(focused.length, 0); assert.equal(scrolled.length, 0);
    await render({ revealRequest: 1, revealReady: true });
    assert.equal(container.querySelector("details.connect-native-voice").open, true);
    assert.equal(focused.length, 0, "focus waits until the open details and preceding view have committed");
    await paint();
    assert.equal(focused.length, 1); assert.equal(focused[0].tagName, "SUMMARY");
    assert.equal(focused[0].getAttribute("tabindex"), "0");
    assert.deepEqual(scrolled, [{ target: focused[0], options: { block: "start", behavior: "auto" } }]);
    assert.equal(calls.every(call => call.command === "native_voice_snapshot"), true);
    await render({ revealRequest: 1 }); await paint(); assert.equal(focused.length, 1);
    await render({ revealRequest: 2 }); await paint(); assert.equal(focused.length, 2);
    await render({ revealRequest: 3 });
    await render({ revealRequest: 3, revealReady: false }); await paint(); assert.equal(focused.length, 2, "navigation away cancels queued focus");
    await render({ revealRequest: 4 });
    await render({ revealRequest: 4, disabled: true }); await paint(); assert.equal(focused.length, 2, "account transition cancels queued focus");
    assert.equal(calls.some(call => call.command === "native_voice_execute"), false);
  } finally { window.HTMLElement.prototype.focus = focus; window.HTMLElement.prototype.scrollIntoView = scroll; }
});

test("base and restored snapshots render translated UDP and EULA states", async () => {
  await render();
  assert.match(container.querySelector("dl").textContent, /UDPNão iniciadoEULASem alteração/);
  snapshot = { ...idle(), revision: 2, phase: "restored", udpState: "closed", eulaState: "restored" };
  await poll();
  assert.match(container.querySelector("dl").textContent, /UDPEncerradoEULAOriginal restaurado/);
  assert.doesNotMatch(container.querySelector("dl").textContent, /notStarted|unchanged|restored|closed/);
});

test("all nine runtime selections use the reviewed V3 preparation action", async () => {
  await render();
  for (const runtimeProfile of Object.keys(voiceRuntimes)) {
    const select = container.querySelector("select");
    await act(async () => { [...select.options].find(option => option.value === runtimeProfile).selected = true; select.dispatchEvent(new window.Event("change", { bubbles: true })); });
    await click("Revisar preparo e anúncio");
    assert.deepEqual(calls.filter(call => call.command === "native_voice_execute").at(-1).args.request, { action: "prepareRuntime", profile: "simpleVoiceChat", runtimeProfile });
  }
});

test("failure permits a new reviewed attempt after native completion", async () => {
  snapshot = { ...idle(), phase: "failed", errorCode: "native_voice_preparation_failed" };
  await render();
  assert.match(container.textContent, /A operação terminou/);
  await click("Tentar novo preparo");
  assert.equal(calls.filter(call => call.command === "native_voice_execute").at(-1).args.request.action, "prepareRuntime");
});

test("needsRecovery offers distinct EULA and announcement reviews without preparing a new server", async () => {
  snapshot = { ...idle(), phase: "needsRecovery", eulaState: "needsRecovery", eulaRecoveryAvailable: true, recoveryPending: true };
  await render();
  assert.equal([...container.querySelectorAll("button")].some(node => /preparo e anúncio|Tentar novo preparo/.test(node.textContent)), false);
  await click("Revisar recuperação do EULA");
  assert.deepEqual(calls.filter(call => call.command === "native_voice_execute").at(-1).args.request, { action: "prepareEulaRestore" });
  await click("Revisar restauração estrita");
  assert.deepEqual(calls.filter(call => call.command === "native_voice_execute").at(-1).args.request, { action: "prepareRestore", reconciled: false });
});

test("EULA review confirmation cannot become an announcement approval", async () => {
  snapshot = { ...idle(), phase: "eulaRestoreReviewed", eulaRecoveryAvailable: true, review: review("eulaRestore") };
  await render(); await approve(); await click("Confirmar etapa revisada");
  assert.deepEqual(calls.filter(call => call.command === "native_voice_execute").at(-1).args.request, { action: "approveEulaRestore", digest: "a".repeat(64) });
});

test("execution keeps the meaningful diff and copy visible with raw recipe collapsed", async () => {
  snapshot = { ...idle(), phase: "executionReviewed", runtimeProfile: "fabric-loader-0.16.14-1.21.1", review: review() };
  await render();
  const technical = [...container.querySelectorAll("details")].find(node => node.querySelector(":scope > summary")?.textContent === "Detalhes técnicos da execução");
  assert.ok(technical); assert.equal(technical.hasAttribute("open"), false);
  assert.match(technical.textContent, /compiled-only/);
  assert.match(container.textContent, /Cópia dedicada: C:\/owned-copy/);
  assert.match(container.textContent, /Cópia com EULA false/);
});

test("polling cannot renew an expired digest and click checks the original monotonic deadline", async () => {
  snapshot = { ...idle(), phase: "executionReviewed", review: review("execution", 1000) };
  await render(); await approve();
  clock += 1100;
  await act(async () => button("Confirmar etapa revisada").dispatchEvent(new window.Event("click", { bubbles: true })));
  assert.equal(calls.some(call => call.command === "native_voice_execute"), false);
  snapshot.revision++; snapshot.review.remainingMs = 60_000;
  await poll();
  assert.equal(button("Confirmar etapa revisada").disabled, true);
  assert.match(container.textContent, /Esta revisão expirou/);
});

test("a long preparation anchors its new review from a fresh snapshot read", async () => {
  handle = async () => { clock += 180_000; snapshot = { ...idle(), revision: 2, phase: "filesReviewed", review: review("files", 50_000) }; return snapshot; };
  await render(); await click("Revisar preparo e anúncio");
  assert.match(container.textContent, /Prazo restante desta revisão: 50 s/);
  await approve(); assert.equal(button("Confirmar etapa revisada").disabled, false);
});

test("disabling the account scope hides a late exported invitation", async () => {
  let complete;
  snapshot = { ...idle(), phase: "bound", manualLabAvailable: true, participants: [{ participantId: "p", publicId: "public", localPlayer: false }] };
  handle = () => new Promise(resolve => { complete = resolve; });
  await render(); await click("Exibir convite deste participante");
  await render({ disabled: true });
  await act(async () => complete({ ...snapshot, revision: 2, exportedInvitation: "stale-private-invitation" }));
  assert.equal(container.querySelector("textarea[aria-label='Convite de voz individual']"), null);
});

test("double approval stays single and Stop cannot be undone by its late result", async () => {
  let complete;
  snapshot = { ...idle(), phase: "executionReviewed", review: review() };
  handle = request => {
    if (request.action === "stop") { snapshot = { ...idle(), revision: 4, phase: "stopped" }; return snapshot; }
    return new Promise(resolve => { complete = resolve; });
  };
  await render(); await approve(); await click("Confirmar etapa revisada");
  await act(async () => button("Confirmar etapa revisada").dispatchEvent(new window.Event("click", { bubbles: true })));
  assert.equal(calls.filter(call => call.command === "native_voice_execute" && call.args.request.action === "approveExecution").length, 1);
  await click("Encerrar transporte e dedicado próprios");
  await act(async () => complete({ ...idle(), revision: 3, phase: "ready" }));
  assert.match(container.textContent, /Origem encerrada/);
  assert.doesNotMatch(container.textContent, /Origem pronta para revisão/);
});

for (const cleanupFailed of [false, true]) {
  test(`forced Stop warns about incomplete saving with ${cleanupFailed ? "failed" : "completed"} transport cleanup`, async () => {
    snapshot = { ...idle(), phase: "socialSource", eulaState: "ownedBySource" };
    handle = request => {
      assert.equal(request.action, "stop");
      snapshot = { ...idle(), revision: 2, phase: cleanupFailed ? "failed" : "stopped",
        udpState: cleanupFailed ? "cleanupFailed" : "closed", eulaState: "restored", recoveryPending: true,
        errorCode: cleanupFailed ? "native_voice_stop_forced_cleanup_failed" : "native_voice_stop_forced" };
      if (cleanupFailed) throw snapshot.errorCode;
      return snapshot;
    };
    await render();
    await click("Encerrar transporte e dedicado próprios");
    const alert = container.querySelector("[role='alert']").textContent;
    assert.match(alert, /encerrado à força.*salvamento do mundo pode ter ficado incompleto/);
    if (cleanupFailed) {
      assert.match(alert, /encerramento do transporte de voz ainda precisa ser confirmado/);
      assert.match(container.textContent, /Encerramento não confirmado/);
    }
    assert.match(container.textContent, /Original restaurado/);
    assert.equal(button("Revisar restauração estrita").disabled, false);
  });
}
