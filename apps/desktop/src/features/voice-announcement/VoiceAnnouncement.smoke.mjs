// Synthetic IPC/DOM fixtures. No Java, file mutation, account, audio or network.
import assert from "node:assert/strict";
import { test, beforeEach, afterEach } from "node:test";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";
const require = createRequire(import.meta.url);
const { parseHTML } = require("linkedom");
const { window, document } = parseHTML("<html><body><div id='root'></div></body></html>");
document.oninput = null;
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Voice synthetic DOM" } });
let time = 1800000000000;
Date.now = () => time;
let timerId = 0;
const timers = new Map();
globalThis.setInterval = callback => { timers.set(++timerId, callback); return timerId; };
globalThis.clearInterval = id => timers.delete(id);
const modules = new Map();
async function sourceModule(source) {
  if (modules.has(source.href)) return modules.get(source.href);
  let output = ts.transpileModule(await readFile(source, "utf8"), { fileName: fileURLToPath(source), compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX } }).outputText.replace(/^import ["'][^"']+\.css["'];?$/gm, "");
  for (const { fileName: specifier } of ts.preProcessFile(output).importedFiles) {
    const dependency = specifier.startsWith(".") ? ["", ".ts", ".tsx"].map(ext => new URL(specifier + ext, source)).find(url => existsSync(fileURLToPath(url))) : undefined;
    const resolved = dependency ? await sourceModule(dependency) : pathToFileURL(require.resolve(specifier)).href;
    output = output.replaceAll(`from "${specifier}"`, `from ${JSON.stringify(resolved)}`);
  }
  const url = `data:text/javascript;base64,${Buffer.from(output).toString("base64")}`;
  modules.set(source.href, url); return url;
}
const types = await import(await sourceModule(new URL("./types.ts", import.meta.url)));
const OP = "11111111-1111-4111-8111-111111111111", PLAN = "22222222-2222-4222-8222-222222222222", CLOSE = "33333333-3333-4333-8333-333333333333", POINT = "44444444-4444-4444-8444-444444444444";
const RECEIPT = "55555555-5555-4555-8555-555555555555";
const hash = "a".repeat(64), digest = "b".repeat(64);
const copy = "aurora-test-synthetic";
const caps = available => ({ available, experimental: true, diagnosticCopyOnly: true, mode: "dedicatedAuroraOnly", guestAddress: "127.0.0.1:35554", mods: available ? ["simple_voice_chat", "plasmo_voice"] : [], restoration: "strictAndExplicitReconciled", audioState: "notVerified", transportState: "notObserved" });
const idle = () => ({ generation: 0, operationId: null, instanceId: null, voiceMod: null, phase: "idle", busy: false, observation: null, review: null, restorationState: null, lastRecoveryResult: null, lastErrorCode: null, close: null });
const observation = (voiceMod = "simple_voice_chat") => ({ voiceMod, artifactVersion: types.MODS[voiceMod].version, artifactSha256: types.MODS[voiceMod].sha256, artifactBytes: types.MODS[voiceMod].bytes, configPath: types.MODS[voiceMod].path, sourceSha256: hash, sourceBytes: 60, limitations: [...types.LIMITATIONS], announcement: voiceMod === "simple_voice_chat" ? { type: "simpleVoiceChat", voiceHost: "" } : { type: "plasmoPublic", ip: "127.0.0.1", port: 0 } });
const review = (action = "setAnnouncement", voiceMod = "simple_voice_chat") => ({ action, voiceMod, planId: PLAN, digest, configPath: types.MODS[voiceMod].path, before: "voice_host=", after: "voice_host=127.0.0.1:35554", sourceSha256: hash, replacementSha256: "c".repeat(64), expiresAtMs: time + 300000, warning: "Experimental; somente cópia dedicada Aurora." });
const selected = (extra = {}) => ({ ...idle(), generation: 1, operationId: OP, instanceId: copy, voiceMod: "simple_voice_chat", phase: "observed", observation: observation(), ...extra });
const applied = () => selected({ phase: "applied", observation: null, restorationState: "ready", lastRecoveryResult: { restoredPointId: POINT, undoAvailable: true, recoveredInterrupted: false } });
const closeReview = intent => ({ ...applied(), generation: 3, phase: "closeReview", close: { closeId: CLOSE, intent, state: "reviewRequired", review: { digest, expiresAtMs: time + 60000, instanceId: copy, voiceMod: "simple_voice_chat", state: "applied", selectiveRestorationLost: true } } });
let snapshot, capabilities, calls, handlers, root, props, controller, logoutPromise, logoutSteps, externalAction, changed, navigations, harnessRenders;
const callbacks = new Map(), listeners = new Map(); let callbackId = 0;
function move(extra) { snapshot = { ...snapshot, ...extra, generation: snapshot.generation + 1 }; return structuredClone(snapshot); }
async function nativeInvoke(command, args) {
  if (command === "plugin:event|listen") { listeners.set(args.handler, { event: args.event, callback: callbacks.get(args.handler) }); return args.handler; }
  if (command === "plugin:event|unlisten") { listeners.delete(args.eventId); return; }
  calls.push({ command, args });
  if (handlers.has(command)) return handlers.get(command)(args);
  const request = args?.request;
  switch (command) {
    case "voice_announcement_capabilities": return structuredClone(capabilities);
    case "voice_announcement_snapshot": return structuredClone(snapshot);
    case "voice_announcement_inspect": return move({ ...selected(), instanceId: request.instanceId, voiceMod: request.voiceMod, observation: observation(request.voiceMod) });
    case "voice_announcement_prepare_apply": return move({ phase: "reviewApply", review: review("setAnnouncement", snapshot.voiceMod) });
    case "voice_announcement_confirm": return move({ phase: snapshot.review.action === "setAnnouncement" ? "applied" : "restored", review: null, observation: null, restorationState: snapshot.review.action === "setAnnouncement" ? "ready" : "restored", lastRecoveryResult: applied().lastRecoveryResult });
    case "voice_announcement_prepare_restore": return move({ phase: "reviewRestore", review: review("restoreOriginal", snapshot.voiceMod) });
    case "voice_announcement_prepare_restore_reconciled": return move({ phase: "reviewRestore", review: review("restoreReconciled", snapshot.voiceMod) });
    case "voice_announcement_cancel_review": return move({ phase: snapshot.restorationState === "ready" ? "applied" : "observed", review: null });
    case "voice_announcement_begin_close":
      if (snapshot.restorationState === "ready" || snapshot.phase === "needsRecovery") return move({ ...closeReview(request.intent), generation: snapshot.generation });
      return move({ phase: "closed", busy: false, review: null, observation: null, restorationState: null, close: { closeId: CLOSE, intent: request.intent, state: "ready", review: null } });
    case "voice_announcement_cancel_close": return move({ close: null, phase: snapshot.restorationState === "ready" ? "applied" : "failed", lastErrorCode: null });
    case "voice_announcement_confirm_close": return move({ phase: "closed", restorationState: null, close: { ...snapshot.close, state: "ready", review: null } });
    case "voice_announcement_finish_close": return move(idle());
    default: throw new Error("Unexpected synthetic command");
  }
}
window.__TAURI_INTERNALS__ = { invoke: nativeInvoke, transformCallback(callback) { callbacks.set(++callbackId, callback); return callbackId; } };
window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} }; window.isTauri = true;
const { act, createElement, useEffect } = await import("react");
const { createRoot } = await import("react-dom/client");
const { VoiceAnnouncementProvider, useVoiceAnnouncement, withVoiceAnnouncementLogout } = await import(await sourceModule(new URL("./VoiceAnnouncementProvider.tsx", import.meta.url)));
const { VoiceAnnouncementPanel } = await import(await sourceModule(new URL("./VoiceAnnouncementPanel.tsx", import.meta.url)));
function Harness({ panel = true, ...panelProps }) {
  harnessRenders += 1;
  controller = useVoiceAnnouncement();
  useEffect(() => controller.registerNavigator((...args) => navigations.push(args)), [controller.registerNavigator]);
  return createElement("div", null,
    createElement("button", { onClick() { logoutPromise = withVoiceAnnouncementLogout(controller, externalAction).catch(() => logoutSteps.push("failed")); } }, "Sair da conta fixture"),
    panel ? createElement(VoiceAnnouncementPanel, { ...panelProps, onChanged: async () => { changed += 1; } }) : null);
}
const container = document.getElementById("root");
const render = async (extra = {}) => { props = { ...props, ...extra }; await act(async () => root.render(createElement(VoiceAnnouncementProvider, null, createElement(Harness, props)))); };
const button = name => { const found = [...container.querySelectorAll("button")].find(value => value.textContent === name); assert.ok(found, name); return found; };
const click = async name => { assert.equal(button(name).disabled, false, name); await act(async () => button(name).dispatchEvent(new window.Event("click", { bubbles: true }))); };
const check = async name => { const element = container.querySelector(`input[aria-label="${name}"]`); assert.ok(element); assert.equal(element.disabled, false); await act(async () => { element.checked = !element.checked; element.dispatchEvent(new window.Event("click", { bubbles: true })); }); };
const count = name => calls.filter(call => call.command === `voice_announcement_${name}`).length;
const last = name => calls.filter(call => call.command === `voice_announcement_${name}`).at(-1);
const refresh = async () => { await act(async () => controller.refresh()); };
const tick = async () => { await act(async () => { for (const callback of [...timers.values()]) callback(); }); };
const emit = async (event, payload) => { await act(async () => { for (const listener of listeners.values()) if (listener.event === event) listener.callback({ payload }); }); };
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
beforeEach(() => {
  time = 1800000000000; snapshot = idle(); capabilities = caps(true); calls = []; handlers = new Map();
  window.isTauri = true; harnessRenders = 0;
  props = { instanceId: copy, sourceInstanceId: "original", busy: false, running: false };
  changed = 0; navigations = []; logoutPromise = null; logoutSteps = []; externalAction = async () => { logoutSteps.push("connect", "firebase"); };
  root = createRoot(container);
});
afterEach(async () => { await act(async () => root.unmount()); window.isTauri = true; assert.equal(timers.size, 0, "All UI timers were removed"); assert.equal(listeners.size, 0, "All event listeners were removed"); });

test("feature off exposes no config controls or instance read; normal logout continues", async () => {
  capabilities = caps(false); await render();
  assert.equal(container.querySelector('[aria-label="Anúncio de voz experimental"]'), null);
  assert.equal(count("snapshot"), 0); await click("Sair da conta fixture"); await logoutPromise;
  assert.deepEqual(logoutSteps, ["connect", "firebase"]); assert.equal(count("begin_close"), 0);
});
for (const mode of ["browser", "featureOff"]) test(`${mode} creates no expiry timer or periodic consumer render`, async () => {
  if (mode === "browser") window.isTauri = false;
  else capabilities = caps(false);
  await render(); assert.equal(timers.size, 0); const renders = harnessRenders;
  time += 10000; await tick(); assert.equal(harnessRenders, renders); assert.equal(count("snapshot"), 0);
  assert.equal(count("capabilities"), mode === "browser" ? 0 : 1);
});
test("expiry timers exist only for visible file or close reviews without replacing polling or listeners", async () => {
  await render(); assert.equal(timers.size, 1, "Idle only retains the native snapshot poll");
  const [pollId] = timers.keys(); const listenerIds = [...listeners.keys()];
  time += 900000;
  snapshot = selected({ phase: "reviewApply", review: { ...review(), expiresAtMs: time - 1 } }); await refresh();
  assert.equal(timers.size, 2); assert.match(container.textContent, /A revisão venceu/);
  const fileTimers = [...timers.keys()]; await refresh(); assert.deepEqual([...timers.keys()], fileTimers);
  await render({ panel: false }); assert.deepEqual([...timers.keys()], [pollId]);
  await render({ panel: true }); assert.equal(timers.size, 2);
  await render({ instanceId: "other-copy", sourceInstanceId: "other-source" }); assert.deepEqual([...timers.keys()], [pollId]);
  await render({ instanceId: copy, sourceInstanceId: "original" }); assert.equal(timers.size, 2);
  await click("Cancelar revisão"); assert.deepEqual([...timers.keys()], [pollId]);
  time += 900000;
  snapshot = closeReview("exit"); snapshot.generation = 20; snapshot.close.review.expiresAtMs = time - 1; await refresh();
  assert.equal(timers.size, 2); assert.match(container.textContent, /A revisão venceu/);
  const closeTimers = [...timers.keys()]; await refresh(); assert.deepEqual([...timers.keys()], closeTimers);
  await click("Cancelar saída"); assert.deepEqual([...timers.keys()], [pollId]); assert.deepEqual([...listeners.keys()], listenerIds);
});
for (const voiceMod of ["simple_voice_chat", "plasmo_voice"]) test(`${voiceMod}: explicit inspection, review, one approval and strict restoration`, async () => {
  await render();
  if (voiceMod === "plasmo_voice") await act(async () => { const select = container.querySelector("select"); for (const option of select.querySelectorAll("option")) option.selected = false; select.querySelector(`option[value="${voiceMod}"]`).selected = true; select.dispatchEvent(new window.Event("change", { bubbles: true })); });
  await click("Inspecionar anúncio");
  assert.deepEqual(last("inspect").args, { request: { instanceId: copy, voiceMod } });
  assert.match(container.textContent, voiceMod === "simple_voice_chat" ? /voice_host vazio/ : /porta efetiva não resolvida/);
  await click("Revisar aplicação na cópia");
  assert.deepEqual(last("prepare_apply").args, { request: { operationId: OP, generation: 1 } });
  assert.equal(button("Confirmar alteração revisada").disabled, true); assert.equal(count("confirm"), 0);
  await check("Autorizo esta alteração do anúncio"); await click("Confirmar alteração revisada");
  assert.deepEqual(last("confirm").args, { request: { operationId: OP, planId: PLAN, digest } });
  assert.match(container.textContent, /Anúncio aplicado somente na cópia/); assert.equal(changed, 1);
  await click("Revisar restauração estrita"); assert.equal(button("Confirmar alteração revisada").disabled, true);
  await check("Autorizo esta alteração do anúncio"); await click("Confirmar alteração revisada");
  assert.match(container.textContent, /Anúncio restaurado/); assert.equal(changed, 2); assert.equal(count("prepare_restore_reconciled"), 0);
});
test("strict failure never selects reconciled action; explicit reconciled gets new consent", async () => {
  snapshot = applied(); await render();
  handlers.set("voice_announcement_prepare_restore", () => { move({ lastErrorCode: "voice_stale_review" }); throw "voice_stale_review"; });
  await click("Revisar restauração estrita"); assert.equal(count("prepare_restore_reconciled"), 0);
  await click("Revisar restauração preservando outras edições");
  assert.equal(snapshot.review.action, "restoreReconciled"); assert.equal(button("Confirmar alteração revisada").disabled, true);
  assert.deepEqual(last("prepare_restore_reconciled").args, { request: { operationId: OP } });
});
test("source switch hides review and blocks mutations of the previous copy", async () => {
  snapshot = selected({ phase: "reviewApply", review: review() }); await render(); await check("Autorizo esta alteração do anúncio");
  await render({ instanceId: "another-copy", sourceInstanceId: "another-source" });
  assert.equal(container.querySelector('[aria-label="Revisão do anúncio"]'), null); assert.equal(count("confirm"), 0);
  assert.equal(button("Inspecionar anúncio").disabled, true); await click("Abrir cópia desta operação"); assert.deepEqual(navigations, [[copy, "voice"]]);
});
test("delayed result retains native context and does not apply to a newly selected copy", async () => {
  const wait = deferred(); handlers.set("voice_announcement_inspect", () => wait.promise); await render(); await click("Inspecionar anúncio");
  await render({ instanceId: "different-copy", sourceInstanceId: "different-source" });
  await act(async () => { snapshot = selected(); wait.resolve(snapshot); });
  assert.match(container.textContent, /outra cópia/); assert.equal(button("Inspecionar anúncio").disabled, true); assert.equal(count("prepare_apply"), 0);
});
test("new digest and panel remount clear approval; old generation cannot replace review", async () => {
  snapshot = selected({ phase: "reviewApply", review: review() }); await render(); await check("Autorizo esta alteração do anúncio");
  move({ review: { ...snapshot.review, digest: "d".repeat(64) } }); await refresh(); assert.equal(button("Confirmar alteração revisada").disabled, true);
  await check("Autorizo esta alteração do anúncio"); await render({ panel: false }); await render({ panel: true }); assert.equal(button("Confirmar alteração revisada").disabled, true);
  snapshot = selected({ phase: "reviewApply", review: review(), generation: 0 }); await refresh();
  assert.equal(controller.snapshot.review.digest, "d".repeat(64)); assert.equal(count("confirm"), 0);
});
test("file and close consent reset permanently after observed expiry even if civil clock goes back", async () => {
  snapshot = selected({ phase: "reviewApply", review: { ...review(), expiresAtMs: time + 10 } }); await render(); await check("Autorizo esta alteração do anúncio");
  time += 20; await tick(); time -= 20; await tick(); assert.equal(button("Confirmar alteração revisada").disabled, true);
  snapshot = closeReview("exit"); snapshot.generation = 20; snapshot.close.review.expiresAtMs = time + 10; await refresh();
  await check("Aceito perder o recibo de restauração"); time += 20; await tick(); time -= 20; await tick(); assert.equal(button("Sair mantendo alteração na cópia").disabled, true);
});
test("review to waiting close serializes without review and preserves actual apply result", async () => {
  snapshot = selected({ phase: "reviewApply", review: review() }); await render();
  const wait = deferred(); handlers.set("voice_announcement_begin_close", () => {
    move({ phase: "closing", busy: true, review: null, close: { closeId: CLOSE, intent: "panel", state: "waiting", review: null } }); return wait.promise;
  });
  await click("Encerrar operação de anúncio"); await refresh(); assert.match(container.textContent, /Aguardando a operação de arquivos/);
  assert.equal(controller.snapshot.review, null); assert.equal(controller.blocking, true);
  await act(async () => { snapshot = { ...closeReview("panel"), generation: 40 }; wait.resolve(snapshot); });
  assert.match(container.textContent, /Sair mantendo a alteração na cópia/); assert.equal(controller.snapshot.restorationState, "ready"); assert.equal(count("confirm_close"), 0);
});
test("native exit attention works without panel; cancel keeps receipt and restores navigation", async () => {
  await render({ panel: false }); snapshot = closeReview("exit"); await emit("aurora-voice-close-attention", { closeId: CLOSE });
  assert.ok(container.querySelector('[role="alertdialog"]')); await click("Continuar e revisar restauração");
  assert.equal(controller.snapshot.phase, "applied"); assert.equal(controller.blocking, false); assert.deepEqual(navigations, [[copy, "voice"]]); assert.equal(count("confirm_close"), 0);
});
test("native exit approval does not invoke frontend finish_close", async () => {
  snapshot = closeReview("exit"); await render({ panel: false }); await check("Aceito perder o recibo de restauração"); await click("Sair mantendo alteração na cópia");
  assert.deepEqual(last("confirm_close").args, { request: { closeId: CLOSE, digest } }); assert.equal(count("finish_close"), 0); assert.equal(controller.blocking, true);
});
test("logout waits for approval and exterior Connect/Firebase action before finish_close", async () => {
  snapshot = applied(); const wait = deferred(); externalAction = async () => { logoutSteps.push("connect"); await wait.promise; logoutSteps.push("firebase"); };
  await render(); await click("Sair da conta fixture"); assert.deepEqual(logoutSteps, []); assert.equal(count("finish_close"), 0);
  await check("Aceito perder o recibo de restauração"); await click("Sair mantendo alteração na cópia");
  assert.deepEqual(logoutSteps, ["connect"]); assert.equal(controller.blocking, true); assert.equal(count("finish_close"), 0);
  await act(async () => { wait.resolve(); await logoutPromise; }); assert.deepEqual(logoutSteps, ["connect", "firebase"]); assert.equal(count("finish_close"), 1); assert.equal(controller.blocking, false);
});
test("cancelled logout leaves account action uncalled and receipt ready", async () => {
  snapshot = applied(); await render(); await click("Sair da conta fixture"); await click("Cancelar saída"); await logoutPromise;
  assert.deepEqual(logoutSteps, []); assert.equal(count("finish_close"), 0); assert.equal(controller.snapshot.restorationState, "ready");
});
test("lost accepted logout approval response resumes the same action and waits before finishing", async () => {
  snapshot = applied(); const wait = deferred(); externalAction = async () => { logoutSteps.push("connect"); await wait.promise; logoutSteps.push("firebase"); };
  handlers.set("voice_announcement_confirm_close", () => { move({ phase: "closed", restorationState: null, close: { ...snapshot.close, state: "ready", review: null } }); throw "voice_internal"; });
  await render(); await click("Sair da conta fixture"); await check("Aceito perder o recibo de restauração"); await click("Sair mantendo alteração na cópia");
  assert.deepEqual(logoutSteps, ["connect"]); assert.equal(count("confirm_close"), 1); assert.equal(count("finish_close"), 0); assert.equal(controller.blocking, true);
  await act(async () => { wait.resolve(); await logoutPromise; });
  assert.deepEqual(logoutSteps, ["connect", "firebase"]); assert.equal(count("finish_close"), 1); assert.equal(controller.blocking, false);
});
test("lost accepted logout cancellation response keeps receipt and allows a later logout request", async () => {
  snapshot = applied();
  handlers.set("voice_announcement_cancel_close", () => { move({ close: null, phase: "applied", lastErrorCode: null }); throw "voice_internal"; });
  await render(); await click("Sair da conta fixture"); await click("Cancelar saída"); await logoutPromise;
  assert.deepEqual(logoutSteps, []); assert.equal(controller.snapshot.restorationState, "ready"); assert.equal(controller.blocking, false); assert.equal(count("finish_close"), 0);
  handlers.delete("voice_announcement_cancel_close"); await click("Sair da conta fixture");
  assert.equal(count("begin_close"), 2); await click("Cancelar saída"); await logoutPromise; assert.deepEqual(logoutSteps, []);
});
test("refused logout approval never continues account work from an unchanged review", async () => {
  snapshot = applied(); handlers.set("voice_announcement_confirm_close", () => { throw "voice_internal"; });
  await render(); await click("Sair da conta fixture"); await check("Aceito perder o recibo de restauração"); await click("Sair mantendo alteração na cópia");
  assert.deepEqual(logoutSteps, []); assert.equal(controller.snapshot.restorationState, "ready"); assert.equal(count("finish_close"), 0);
  await click("Cancelar saída"); await logoutPromise; assert.deepEqual(logoutSteps, []);
});
for (const kind of ["confirm", "cancel"]) test(`lost logout ${kind} response and failed immediate reads reconcile once from later polling`, async () => {
  snapshot = applied(); const wait = deferred(); externalAction = async () => { logoutSteps.push("connect"); await wait.promise; logoutSteps.push("firebase"); };
  await render(); await click("Sair da conta fixture");
  const command = `${kind}_close`;
  handlers.set(`voice_announcement_${command}`, () => {
    if (kind === "confirm") move({ phase: "closed", restorationState: null, close: { ...snapshot.close, state: "ready", review: null } });
    else move({ close: null, phase: "applied", lastErrorCode: null });
    handlers.set("voice_announcement_snapshot", () => { throw "voice_internal"; });
    throw "voice_internal";
  });
  if (kind === "confirm") { await check("Aceito perder o recibo de restauração"); await click("Sair mantendo alteração na cópia"); }
  else await click("Cancelar saída");
  assert.equal(controller.valid, false); assert.deepEqual(logoutSteps, []); assert.equal(count("finish_close"), 0);
  handlers.delete("voice_announcement_snapshot"); await tick();
  assert.equal(count(command), 1);
  if (kind === "confirm") {
    assert.deepEqual(logoutSteps, ["connect"]); assert.equal(count("finish_close"), 0); assert.equal(controller.blocking, true);
    await act(async () => { wait.resolve(); await logoutPromise; });
    assert.deepEqual(logoutSteps, ["connect", "firebase"]); assert.equal(count("finish_close"), 1);
  } else {
    await logoutPromise; assert.deepEqual(logoutSteps, []); assert.equal(controller.snapshot.restorationState, "ready");
    handlers.delete(`voice_announcement_${command}`); await click("Sair da conta fixture"); await click("Cancelar saída"); await logoutPromise;
  }
  await tick(); assert.equal(controller.blocking, false); assert.equal(count("confirm_close"), kind === "confirm" ? 1 : 0);
});
test("exterior logout failure still releases approved native barrier", async () => {
  externalAction = async () => { logoutSteps.push("connect"); throw new Error("private failure"); };
  await render(); await click("Sair da conta fixture"); await logoutPromise;
  assert.deepEqual(logoutSteps, ["connect", "failed"]); assert.equal(count("finish_close"), 1); assert.equal(controller.blocking, false); assert.doesNotMatch(container.textContent, /private failure/);
});
test("orphaned logout review cannot discard receipt; ready orphan can release barrier", async () => {
  snapshot = closeReview("logout"); await render({ panel: false });
  assert.match(container.textContent, /pedido de saída da conta foi interrompido/); assert.equal(button("Sair mantendo alteração na cópia").disabled, true);
  await click("Cancelar saída"); assert.equal(controller.snapshot.restorationState, "ready"); assert.equal(count("confirm_close"), 0);
  move({ phase: "closed", restorationState: null, close: { closeId: CLOSE, intent: "logout", state: "ready", review: null } }); await refresh();
  await click("Retomar trabalho"); assert.equal(controller.blocking, false); assert.equal(count("finish_close"), 1); assert.deepEqual(logoutSteps, []);
});
test("wrong close approval refreshes consumed review while retaining receipt", async () => {
  snapshot = closeReview("exit"); await render();
  handlers.set("voice_announcement_confirm_close", () => { move({ close: { ...snapshot.close, review: null }, lastErrorCode: "voice_stale_close" }); throw "voice_stale_close"; });
  await check("Aceito perder o recibo de restauração"); await click("Sair mantendo alteração na cópia");
  assert.match(container.textContent, /recibo foi preservado/); assert.equal(button("Sair mantendo alteração na cópia").disabled, true); assert.equal(controller.snapshot.restorationState, "ready");
  await click("Cancelar saída"); assert.equal(controller.blocking, false);
});
test("lost panel-close confirmation response exposes finish recovery without a second approval", async () => {
  snapshot = closeReview("panel"); await render();
  handlers.set("voice_announcement_confirm_close", () => { move({ phase: "closed", restorationState: null, close: { ...snapshot.close, state: "ready", review: null } }); throw "voice_internal"; });
  await check("Aceito perder o recibo de restauração"); await click("Sair mantendo alteração na cópia");
  assert.equal(controller.snapshot.close.state, "ready"); assert.equal(controller.blocking, true);
  await click("Retomar trabalho"); assert.equal(controller.blocking, false); assert.equal(count("confirm_close"), 1); assert.equal(count("finish_close"), 1);
});
test("blocked exit reports only allowlisted error and no raw payload", async () => {
  await render({ panel: false }); await emit("aurora-voice-close-blocked", { code: "PRIVATE_TOKEN C:/private" });
  assert.match(container.textContent, /Não foi possível concluir/); assert.doesNotMatch(container.textContent, /PRIVATE_TOKEN|C:\/private/);
});
test("ready copy and running-game UI gates do not issue inspection", async () => {
  await render({ instanceId: null }); assert.equal(button("Inspecionar anúncio").disabled, true);
  await render({ instanceId: copy, running: true }); assert.equal(button("Inspecionar anúncio").disabled, true); assert.equal(count("inspect"), 0);
});
test("double confirmation cannot start two writes and completion remains owned after panel unmount", async () => {
  snapshot = selected({ phase: "reviewApply", review: review() }); const wait = deferred();
  handlers.set("voice_announcement_confirm", () => { move({ phase: "applying", busy: true, review: null, observation: null }); return wait.promise; });
  await render(); await check("Autorizo esta alteração do anúncio");
  const confirm = button("Confirmar alteração revisada");
  await act(async () => { confirm.dispatchEvent(new window.Event("click", { bubbles: true })); confirm.dispatchEvent(new window.Event("click", { bubbles: true })); });
  assert.equal(count("confirm"), 1); await render({ panel: false });
  await act(async () => { snapshot = { ...applied(), generation: 10 }; wait.resolve(snapshot); });
  await render({ panel: true }); assert.equal(controller.snapshot.restorationState, "ready"); assert.ok(button("Revisar restauração estrita")); assert.equal(count("confirm"), 1);
});
test("needsRecovery exposes recovery navigation without preparing either restore", async () => {
  snapshot = selected({ phase: "needsRecovery", observation: null, restorationState: "needs_recovery", lastErrorCode: "voice_needs_recovery" });
  await render(); assert.match(container.textContent, /Verificação na Recuperação necessária/);
  assert.equal([...container.querySelectorAll("button")].some(value => value.textContent === "Revisar restauração estrita"), false);
  await click("Abrir Recuperação da cópia"); assert.deepEqual(navigations, [[copy, "recovery"]]);
  assert.equal(count("prepare_restore"), 0); assert.equal(count("prepare_restore_reconciled"), 0);
});
test("busy worker and every published operation phase have a consistent projection", () => {
  const base = selected();
  for (const phase of ["inspecting", "preparingApply", "applying"]) types.voiceSnapshotFrom({ ...base, phase, busy: true });
  for (const phase of ["preparingRestore", "restoring"]) types.voiceSnapshotFrom({ ...applied(), phase, busy: true });
  types.voiceSnapshotFrom({ ...base, phase: "reviewApply", review: review() });
  types.voiceSnapshotFrom({ ...applied(), phase: "reviewRestore", review: review("restoreOriginal") });
  types.voiceSnapshotFrom({ ...applied(), phase: "restored", restorationState: "restored" });
  types.voiceSnapshotFrom({ ...base, phase: "failed", observation: null });
  types.voiceSnapshotFrom({ ...applied(), phase: "closing", busy: true, close: { closeId: CLOSE, intent: "panel", state: "waiting", review: null } });
  assert.throws(() => types.voiceSnapshotFrom({ ...base, phase: "closing", busy: true, review: review(), close: { closeId: CLOSE, intent: "panel", state: "waiting", review: null } }));
});
test("malformed later snapshot fails closed instead of accepting a changed plan", async () => {
  snapshot = selected({ phase: "reviewApply", review: review() }); await render();
  snapshot = { ...snapshot, generation: 2, receipt: { invented: true } }; await act(async () => controller.refresh().catch(() => undefined));
  assert.equal(controller.valid, false); assert.equal(button("Confirmar alteração revisada").disabled, true); assert.equal(count("confirm"), 0);
});
test("parsers preserve empty, absent and zero while rejecting arrays, cross-mod and authority metadata", () => {
  assert.equal(types.voiceObservationFrom(observation()).announcement.voiceHost, "");
  const absent = { ...observation("plasmo_voice"), announcement: { type: "plasmoAbsent" } }; assert.deepEqual(types.voiceObservationFrom(absent).announcement, { type: "plasmoAbsent" });
  assert.equal(types.voiceObservationFrom(observation("plasmo_voice")).announcement.port, 0);
  for (const field of ["action"]) assert.throws(() => types.voiceReviewFrom({ ...review(), [field]: [review()[field]] }));
  for (const field of ["intent", "state"]) { const bad = closeReview("exit"); bad.close[field] = [bad.close[field]]; assert.throws(() => types.voiceSnapshotFrom(bad)); }
  const badState = closeReview("exit"); badState.close.review.state = ["applied"]; assert.throws(() => types.voiceSnapshotFrom(badState));
  assert.throws(() => types.voiceSnapshotFrom({ ...applied(), phase: "closed" }));
  assert.throws(() => types.voiceSnapshotFrom({ ...selected(), observation: observation("plasmo_voice") }));
  assert.throws(() => types.voiceSnapshotFrom({ ...selected(), receipt: "authority" }));
  assert.throws(() => types.voiceObservationFrom({ ...observation("plasmo_voice"), announcement: { type: "plasmoPublic", ip: "", port: 0 } }));
  assert.throws(() => types.voiceCapabilitiesFrom({ ...caps(false), available: true }));
});
test("parsers enforce bounded text/pins/hashes and impossible state combinations", () => {
  assert.throws(() => types.voiceObservationFrom({ ...observation(), artifactSha256: "f".repeat(64) }));
  assert.throws(() => types.voiceObservationFrom({ ...observation(), announcement: { type: "simpleVoiceChat", voiceHost: "é".repeat(513) } }));
  assert.throws(() => types.voiceReviewFrom({ ...review(), before: "x".repeat(2049) }));
  assert.throws(() => types.voiceReviewFrom({ ...review(), expiresAtMs: 1.5 }));
  assert.throws(() => types.voiceSnapshotFrom({ ...selected(), busy: true }));
  assert.throws(() => types.voiceSnapshotFrom({ ...applied(), restorationState: null }));
  assert.throws(() => types.voiceSnapshotFrom({ ...selected(), phase: "closing", busy: true }));
  assert.throws(() => types.voiceSnapshotFrom({ ...selected({ phase: "reviewApply", review: review() }), review: { ...review(), sourceSha256: "d".repeat(64) } }));
});

const plannedReceipt = () => ({ state: "planned", receiptId: null, generation: null });
const savedReceipt = (generation = 0) => ({ state: "saved", receiptId: RECEIPT, generation });
const receiptRow = (voiceMod = "simple_voice_chat", state = "ready") => ({ receiptId: RECEIPT, voiceMod, state, generation: state === "ready" ? 0 : 1 });
const durableApplied = (voiceMod = "simple_voice_chat") => ({ ...applied(), voiceMod, durableReceipt: savedReceipt() });
function durableClosing(intent = "panel", receipt = savedReceipt(), voiceMod = "simple_voice_chat") {
  const uncertain = receipt.state === "uncertain";
  const base = { ...closeReview(intent), voiceMod, observation: null, durableReceipt: receipt,
    restorationState: uncertain ? "needs_recovery" : "ready" };
  base.close.review = { ...base.close.review, voiceMod, state: uncertain ? "needsRecovery" : "applied",
    selectiveRestorationLost: uncertain, durableReceipt: receipt };
  return base;
}
function durableFixtures(voiceMod = "simple_voice_chat") {
  capabilities.durableReceipts = { schemaVersion: 1, maxReceipts: 64 };
  handlers.set("voice_announcement_prepare_apply_durable", () => move({ phase: "reviewApply", review: { ...review("setAnnouncement", snapshot.voiceMod),
    digest: "d".repeat(64), warning: "Recibo salvo na mesma transação; nova revisão para restaurar." }, durableReceipt: plannedReceipt() }));
  handlers.set("voice_announcement_confirm", () => {
    const apply = snapshot.review.action === "setAnnouncement";
    return move({ phase: apply ? "applied" : "restored", review: null, observation: null, restorationState: apply ? "ready" : "restored",
      lastRecoveryResult: applied().lastRecoveryResult, durableReceipt: savedReceipt(apply ? 0 : 1) });
  });
  handlers.set("voice_announcement_begin_close", ({ request }) => move({ ...durableClosing(request.intent, snapshot.durableReceipt, snapshot.voiceMod), generation: snapshot.generation }));
  handlers.set("voice_announcement_list_receipts", ({ request }) => ({ instanceId: request.instanceId, receipts: [receiptRow(voiceMod)] }));
  handlers.set("voice_announcement_resume_receipt", () => move({ ...durableApplied(voiceMod), generation: snapshot.generation, lastRecoveryResult: null }));
  handlers.set("voice_announcement_finish_close", () => { const generation = snapshot.generation + 1; snapshot = { ...idle(), generation }; return snapshot; });
  handlers.set("voice_announcement_cancel_review", () => {
    if (snapshot.durableReceipt?.state === "planned") delete snapshot.durableReceipt;
    return move({ phase: snapshot.restorationState === "ready" ? "applied" : "observed", review: null });
  });
}
async function chooseVoiceMod(voiceMod) {
  await act(async () => {
    const select = container.querySelector('select[aria-label="Mod de voz a inspecionar"]');
    for (const option of select.querySelectorAll("option")) option.selected = false;
    select.querySelector(`option[value="${voiceMod}"]`).selected = true;
    select.dispatchEvent(new window.Event("change", { bubbles: true }));
  });
}

for (const voiceMod of ["simple_voice_chat", "plasmo_voice"]) test(`${voiceMod}: durable choice precedes consent and a reopened panel resumes only a fresh restore review`, async () => {
  durableFixtures(voiceMod); await render(); await chooseVoiceMod(voiceMod);
  assert.equal(count("list_receipts"), 0); assert.equal(count("resume_receipt"), 0);
  await click("Inspecionar anúncio"); await click("Revisar aplicação com recibo salvo");
  assert.deepEqual(last("prepare_apply_durable").args, { request: { operationId: OP, generation: 1 } });
  assert.equal(count("prepare_apply"), 0); assert.equal(button("Confirmar alteração revisada").disabled, true);
  assert.match(container.textContent, /revisão prevê salvar o recibo/); assert.equal(count("confirm"), 0);
  await check("Autorizo esta alteração do anúncio"); await click("Confirmar alteração revisada");
  assert.deepEqual(last("confirm").args, { request: { operationId: OP, planId: PLAN, digest: "d".repeat(64) } });
  assert.match(container.textContent, /recibo está salvo no histórico/);
  await click("Encerrar operação de anúncio");
  assert.doesNotMatch(container.textContent, /Sair descarta a restauração seletiva/);
  assert.match(container.textContent, /recibo salvo fica no histórico/);
  await check("Autorizo sair mantendo a configuração"); await click("Sair mantendo alteração na cópia");
  await act(async () => root.unmount()); root = createRoot(container); await render(); await chooseVoiceMod(voiceMod);
  await click("Consultar recibos salvos");
  assert.deepEqual(last("list_receipts").args, { request: { instanceId: copy } });
  assert.equal(count("resume_receipt"), 0); await click("Retomar restauração");
  assert.deepEqual(last("resume_receipt").args, { request: { instanceId: copy, receiptId: RECEIPT, expectedGeneration: 0 } });
  assert.equal(controller.snapshot.observation, null); assert.equal(controller.snapshot.review, null);
  assert.equal(count("prepare_restore"), 0); assert.equal(count("confirm"), 1);
  await click("Revisar restauração estrita"); assert.equal(button("Confirmar alteração revisada").disabled, true);
  await check("Autorizo esta alteração do anúncio"); await click("Confirmar alteração revisada");
  assert.equal(controller.snapshot.durableReceipt.generation, 1); assert.match(container.textContent, /Anúncio restaurado/);
  assert.equal(count("prepare_restore_reconciled"), 0);
});

test("absent durable capability preserves RAM and cannot invoke durable caller methods", async () => {
  await render(); assert.equal(container.querySelector('[aria-label="Recibos salvos do anúncio"]'), null);
  await act(async () => { await assert.rejects(controller.listReceipts(copy)); await assert.rejects(controller.run("prepare_apply_durable", { operationId: OP, generation: 1 })); });
  assert.equal(count("list_receipts"), 0); assert.equal(count("prepare_apply_durable"), 0);
  await click("Inspecionar anúncio"); assert.equal([...container.querySelectorAll("button")].some(b => b.textContent === "Revisar aplicação com recibo salvo"), false);
  assert.equal(Object.hasOwn(controller.snapshot, "durableReceipt"), false);
});

test("cancelled durable plan does not retain a receipt or transfer consent to the RAM alternative", async () => {
  durableFixtures(); await render(); await click("Inspecionar anúncio"); await click("Revisar aplicação com recibo salvo");
  await check("Autorizo esta alteração do anúncio"); await click("Cancelar revisão");
  assert.equal(Object.hasOwn(controller.snapshot, "durableReceipt"), false);
  await click("Revisar aplicação na cópia"); assert.equal(controller.snapshot.review.digest, digest);
  assert.equal(button("Confirmar alteração revisada").disabled, true); assert.equal(count("confirm"), 0);
});

test("durable list results from another copy or an earlier mod selection never offer resume", async () => {
  durableFixtures(); const waiting = deferred(); handlers.set("voice_announcement_list_receipts", () => waiting.promise);
  await render(); await click("Consultar recibos salvos"); await render({ instanceId: "other-copy" });
  await act(async () => waiting.resolve({ instanceId: copy, receipts: [receiptRow()] }));
  assert.equal([...container.querySelectorAll("button")].some(b => b.textContent === "Retomar restauração"), false);
  assert.equal(count("resume_receipt"), 0);
  handlers.set("voice_announcement_list_receipts", () => ({ instanceId: copy, receipts: [receiptRow()] }));
  await click("Consultar recibos salvos");
  assert.equal([...container.querySelectorAll("button")].some(b => b.textContent === "Retomar restauração"), false);
  await render({ instanceId: copy }); handlers.set("voice_announcement_list_receipts", () => ({ instanceId: copy, receipts: [receiptRow()] }));
  await click("Consultar recibos salvos"); assert.ok(button("Retomar restauração"));
  await chooseVoiceMod("plasmo_voice"); assert.equal([...container.querySelectorAll("button")].some(b => b.textContent === "Retomar restauração"), false);
});

test("restored records are history only and a failed resume requires an explicit fresh list", async () => {
  durableFixtures(); handlers.set("voice_announcement_list_receipts", () => ({ instanceId: copy, receipts: [receiptRow("simple_voice_chat", "restored")] }));
  await render(); await click("Consultar recibos salvos"); assert.match(container.textContent, /Restauração já concluída/);
  assert.equal([...container.querySelectorAll("button")].some(b => b.textContent === "Retomar restauração"), false);
  handlers.set("voice_announcement_list_receipts", () => ({ instanceId: copy, receipts: [receiptRow()] }));
  await click("Consultar recibos salvos"); handlers.set("voice_announcement_resume_receipt", () => { throw "voice_stale_review"; });
  await click("Retomar restauração"); await tick();
  assert.equal(count("resume_receipt"), 1); assert.equal(count("list_receipts"), 2);
  assert.equal([...container.querySelectorAll("button")].some(b => b.textContent === "Retomar restauração"), false);
  assert.equal(count("confirm"), 0);
});

test("lost durable application response observes the committed record without repeating the mutation", async () => {
  durableFixtures(); await render(); await click("Inspecionar anúncio"); await click("Revisar aplicação com recibo salvo");
  handlers.set("voice_announcement_confirm", () => { move({ ...durableApplied(), generation: snapshot.generation }); throw "voice_internal"; });
  await check("Autorizo esta alteração do anúncio"); await click("Confirmar alteração revisada"); await tick();
  assert.equal(count("confirm"), 1); assert.equal(controller.snapshot.durableReceipt.state, "saved");
  assert.match(container.textContent, /recibo está salvo/); assert.equal(count("resume_receipt"), 0);
});

test("uncertain durable executor never promises a usable saved receipt or an automatic restore", async () => {
  durableFixtures(); const uncertain = { state: "uncertain", receiptId: null, generation: null };
  snapshot = selected({ phase: "needsRecovery", observation: null, restorationState: "needs_recovery", durableReceipt: uncertain, lastErrorCode: "voice_needs_recovery" });
  await render(); assert.equal([...container.querySelectorAll("button")].some(b => b.textContent === "Revisar restauração estrita"), false);
  await click("Encerrar operação de anúncio");
  assert.match(container.textContent, /existência de um recibo utilizável ainda não está confirmada/);
  assert.doesNotMatch(container.textContent, /recibo salvo fica no histórico|recibo está salvo/);
  await check("Autorizo sair mantendo a configuração"); await click("Sair mantendo alteração na cópia");
  assert.equal(count("prepare_restore"), 0); assert.equal(count("prepare_restore_reconciled"), 0);
});

test("lost resume response is reconciled as the native operation without another resume or restore", async () => {
  durableFixtures(); handlers.set("voice_announcement_resume_receipt", () => {
    move({ ...durableApplied(), generation: snapshot.generation, lastRecoveryResult: null }); throw "voice_storage";
  });
  await render(); await click("Consultar recibos salvos"); await click("Retomar restauração"); await tick();
  assert.equal(count("resume_receipt"), 1); assert.equal(count("list_receipts"), 1);
  assert.equal(controller.snapshot.phase, "applied"); assert.equal(controller.snapshot.observation, null);
  assert.equal(controller.snapshot.review, null); assert.equal(controller.snapshot.lastRecoveryResult, null);
  assert.equal(controller.snapshot.durableReceipt.receiptId, RECEIPT);
  assert.equal(button("Revisar restauração estrita").disabled, false);
  assert.equal(count("prepare_restore"), 0); assert.equal(count("confirm"), 0);
});

test("listing that outlives panel close cannot republish rows after the close barrier is released", async () => {
  durableFixtures(); const waiting = deferred(); handlers.set("voice_announcement_list_receipts", () => waiting.promise);
  await render(); await click("Consultar recibos salvos");
  const closing = deferred(); handlers.set("voice_announcement_begin_close", () => {
    move({ phase: "closing", busy: true, close: { closeId: CLOSE, intent: "panel", state: "waiting", review: null } }); return closing.promise;
  });
  let closed; await act(async () => { closed = controller.beginPanelClose(); }); await refresh();
  await act(async () => {
    waiting.resolve({ instanceId: copy, receipts: [receiptRow()] });
    closing.resolve(move({ phase: "closed", busy: false, close: { closeId: CLOSE, intent: "panel", state: "ready", review: null } }));
  }); await closed;
  assert.equal(controller.snapshot.phase, "idle"); assert.equal(count("finish_close"), 1);
  assert.equal([...container.querySelectorAll("button")].some(b => b.textContent === "Retomar restauração"), false);
  assert.equal(count("resume_receipt"), 0);
});

test("durable resume busy snapshot has no invented mod/context and close waits for its actual result", async () => {
  durableFixtures(); const waiting = deferred(); handlers.set("voice_announcement_resume_receipt", () => {
    move({ ...idle(), generation: snapshot.generation, phase: "resumingReceipt", busy: true }); return waiting.promise;
  });
  await render(); await click("Consultar recibos salvos"); await click("Retomar restauração"); await refresh();
  assert.equal(controller.snapshot.operationId, null); assert.equal(controller.snapshot.voiceMod, null);
  assert.match(container.textContent, /Verificando recibo salvo/); assert.equal(button("Inspecionar anúncio").disabled, true);
  const closing = deferred(); handlers.set("voice_announcement_begin_close", () => {
    move({ phase: "closing", busy: true, close: { closeId: CLOSE, intent: "exit", state: "waiting", review: null } }); return closing.promise;
  });
  let closePromise; await act(async () => { closePromise = controller.run("begin_close", { operationId: null, intent: "exit" }); }); await refresh();
  await act(async () => { snapshot = { ...durableClosing("exit"), generation: 20 }; waiting.resolve(snapshot); closing.resolve(snapshot); }); await closePromise;
  assert.match(container.textContent, /recibo salvo fica no histórico/); assert.equal(count("confirm_close"), 0); assert.equal(count("confirm"), 0);
});

test("durable logout retains the receipt, requires close consent and preserves Connect/account ordering", async () => {
  durableFixtures(); snapshot = durableApplied(); await render();
  await click("Sair da conta fixture"); assert.deepEqual(logoutSteps, []);
  await check("Autorizo sair mantendo a configuração"); await click("Sair mantendo alteração na cópia"); await logoutPromise;
  assert.deepEqual(logoutSteps, ["connect", "firebase"]); assert.equal(count("finish_close"), 1);
  assert.equal(count("confirm"), 0); assert.equal(count("prepare_restore"), 0);
});

test("durable receipt parsers enforce exact scope, state and bounded lists without altering RAM snapshots", () => {
  const capability = { ...caps(true), durableReceipts: { schemaVersion: 1, maxReceipts: 64 } };
  assert.deepEqual(types.voiceCapabilitiesFrom(capability), capability);
  for (const malformed of [null, { schemaVersion: 2, maxReceipts: 64 }, { schemaVersion: 1, maxReceipts: 65 }, { schemaVersion: 1, maxReceipts: 64, automatic: true }]) {
    assert.throws(() => types.voiceCapabilitiesFrom({ ...caps(true), durableReceipts: malformed }));
  }
  assert.throws(() => types.voiceCapabilitiesFrom({ ...caps(false), durableReceipts: capability.durableReceipts }));
  assert.deepEqual(types.voiceSnapshotFrom(applied()), applied());
  assert.deepEqual(types.voiceSnapshotFrom(durableApplied()), durableApplied());
  assert.deepEqual(types.voiceSnapshotFrom({ ...idle(), phase: "resumingReceipt", busy: true }), { ...idle(), phase: "resumingReceipt", busy: true });
  for (const bad of [{ ...idle(), durableReceipt: plannedReceipt() }, { ...durableApplied(), durableReceipt: plannedReceipt() },
    { ...durableApplied(), durableReceipt: savedReceipt(1) }, { ...durableApplied(), durableReceipt: { ...savedReceipt(), receiptId: null } },
    { ...durableApplied(), durableReceipt: { ...savedReceipt(), configPath: "injected" } },
    { ...durableApplied(), phase: "resumingReceipt", busy: true }, { ...selected(), durableReceipt: plannedReceipt() },
    { ...selected({ phase: "reviewApply", review: review() }), durableReceipt: savedReceipt() }]) assert.throws(() => types.voiceSnapshotFrom(bad));
  const closing = durableClosing(); assert.deepEqual(types.voiceSnapshotFrom(closing), closing);
  const forged = structuredClone(closing); forged.close.review.selectiveRestorationLost = true; assert.throws(() => types.voiceSnapshotFrom(forged));
  delete forged.durableReceipt; forged.close.review.selectiveRestorationLost = false; assert.throws(() => types.voiceSnapshotFrom(forged));
  for (const receipts of [[{ ...receiptRow(), generation: 1 }], [{ ...receiptRow("simple_voice_chat", "restored"), generation: 0 }],
    [{ ...receiptRow(), path: "untrusted" }], [receiptRow(), receiptRow()], [receiptRow(), { ...receiptRow(), receiptId: PLAN }],
    Array(65).fill(receiptRow()), [{ ...receiptRow(), voiceMod: "custom" }]]) assert.throws(() => types.voiceReceiptListFrom({ instanceId: copy, receipts }));
  assert.deepEqual(types.voiceReceiptListFrom({ instanceId: copy, receipts: [receiptRow()] }), { instanceId: copy, receipts: [receiptRow()] });
});

const retirementReview = (voiceMod = "simple_voice_chat") => ({ planId: PLAN, receiptId: RECEIPT, voiceMod, expectedGeneration: 0,
  currentConfigSha256: hash, currentConfigBytes: 60, digest: "e".repeat(64), expiresAtMs: time + 300000,
  warning: "Aposentar elimina a restauração seletiva futura. A configuração não será restaurada ou alterada." });
const retirementSnapshot = (voiceMod = "simple_voice_chat") => ({ ...durableApplied(voiceMod), phase: "reviewRetirement", lastRecoveryResult: null, retirementReview: retirementReview(voiceMod) });
const retiredSnapshot = (voiceMod = "simple_voice_chat") => ({ ...durableApplied(voiceMod), phase: "retired", restorationState: null,
  lastRecoveryResult: null, durableReceipt: { state: "retired", receiptId: RECEIPT, generation: 1 } });
function retirementFixtures(voiceMod = "simple_voice_chat") {
  durableFixtures(voiceMod); capabilities.durableReceipts.retirement = { schemaVersion: 1 };
  let retired = false;
  handlers.set("voice_announcement_list_receipts", ({ request }) => ({ instanceId: request.instanceId, receipts: [receiptRow(voiceMod, retired ? "retired" : "ready")] }));
  handlers.set("voice_announcement_prepare_retirement", () => move({ ...retirementSnapshot(voiceMod), generation: snapshot.generation }));
  handlers.set("voice_announcement_confirm_retirement", () => {
    retired = true; delete snapshot.retirementReview;
    return move({ ...retiredSnapshot(voiceMod), generation: snapshot.generation });
  });
  handlers.set("voice_announcement_cancel_review", () => {
    delete snapshot.retirementReview; return move({ phase: "applied", review: null });
  });
  handlers.set("voice_announcement_begin_close", ({ request }) => {
    delete snapshot.retirementReview;
    if (snapshot.restorationState !== null) return move({ ...durableClosing(request.intent, snapshot.durableReceipt, snapshot.voiceMod), generation: snapshot.generation });
    return move({ phase: "closed", busy: false, close: { closeId: CLOSE, intent: request.intent, state: "ready", review: null } });
  });
}
async function openRetirementReview() { await click("Consultar recibos salvos"); await click("Revisar aposentadoria do recibo"); }
const hasButton = name => [...container.querySelectorAll("button")].some(button => button.textContent === name);

for (const voiceMod of ["simple_voice_chat", "plasmo_voice"]) test(`${voiceMod}: explicit receipt retirement uses its own review and never restores the configuration`, async () => {
  retirementFixtures(voiceMod); await render(); await chooseVoiceMod(voiceMod); await openRetirementReview();
  assert.deepEqual(last("prepare_retirement").args, { request: { instanceId: copy, receiptId: RECEIPT, expectedGeneration: 0 } });
  assert.equal(count("resume_receipt"), 0); assert.equal(count("confirm_retirement"), 0);
  assert.equal(controller.snapshot.review, null); assert.equal(button("Confirmar aposentadoria do recibo").disabled, true);
  assert.match(container.textContent, /configuração continuará como está/);
  await check("Autorizo aposentar somente este recibo"); await click("Confirmar aposentadoria do recibo");
  assert.deepEqual(last("confirm_retirement").args, { request: { operationId: OP, planId: PLAN, digest: "e".repeat(64) } });
  assert.equal(controller.snapshot.phase, "retired"); assert.equal(controller.snapshot.restorationState, null);
  assert.equal(count("confirm"), 0); assert.equal(count("prepare_restore"), 0); assert.equal(count("prepare_restore_reconciled"), 0);
  assert.match(container.textContent, /Nenhuma configuração foi restaurada ou alterada/); assert.equal(hasButton("Retomar restauração"), false);
  await click("Encerrar operação de anúncio"); assert.equal(count("confirm_close"), 0); await click("Consultar recibos salvos");
  assert.match(container.textContent, /Recibo aposentado; não pode ser retomado/);
  assert.equal(hasButton("Retomar restauração"), false); assert.equal(hasButton("Revisar aposentadoria do recibo"), false);
});

test("older durable capability cannot prepare or confirm retirement and retains resume", async () => {
  durableFixtures(); await render(); await click("Consultar recibos salvos");
  assert.equal(hasButton("Revisar aposentadoria do recibo"), false); assert.equal(hasButton("Retomar restauração"), true);
  await act(async () => {
    await assert.rejects(controller.run("prepare_retirement", { instanceId: copy, receiptId: RECEIPT, expectedGeneration: 0 }));
    await assert.rejects(controller.run("confirm_retirement", { operationId: OP, planId: PLAN, digest }));
  });
  assert.equal(count("prepare_retirement"), 0); assert.equal(count("confirm_retirement"), 0);
});

test("cancel retirement preserves ready receipt without automatically restoring or preparing another plan", async () => {
  retirementFixtures(); await render(); await openRetirementReview(); await check("Autorizo aposentar somente este recibo");
  await click("Cancelar revisão e manter recibo");
  assert.deepEqual(last("cancel_review").args, { request: { operationId: OP } });
  assert.equal(controller.snapshot.phase, "applied"); assert.equal(controller.snapshot.durableReceipt.generation, 0);
  assert.equal(Object.hasOwn(controller.snapshot, "retirementReview"), false); assert.equal(hasButton("Revisar restauração estrita"), true);
  assert.equal(count("confirm_retirement"), 0); assert.equal(count("prepare_restore"), 0);
  await click("Encerrar operação de anúncio"); await check("Autorizo sair mantendo a configuração"); await click("Sair mantendo alteração na cópia");
  await openRetirementReview(); assert.equal(button("Confirmar aposentadoria do recibo").disabled, true);
});

test("retirement consent expires irreversibly and resets for a new digest, remount or selected copy", async () => {
  retirementFixtures(); snapshot = retirementSnapshot(); snapshot.retirementReview.expiresAtMs = time + 20;
  await render(); await check("Autorizo aposentar somente este recibo"); time += 30; await tick(); time -= 30; await tick();
  assert.equal(button("Confirmar aposentadoria do recibo").disabled, true);
  await check("Autorizo aposentar somente este recibo"); time += 30; await click("Confirmar aposentadoria do recibo");
  time -= 30; await tick(); assert.equal(button("Confirmar aposentadoria do recibo").disabled, true);
  assert.equal(count("confirm_retirement"), 0, "Click detected expiry between periodic clock ticks");
  move({ retirementReview: { ...retirementReview(), digest: "f".repeat(64) } }); await refresh();
  await check("Autorizo aposentar somente este recibo"); await render({ panel: false }); await render({ panel: true });
  assert.equal(button("Confirmar aposentadoria do recibo").disabled, true);
  await check("Autorizo aposentar somente este recibo"); await render({ instanceId: "another-copy", sourceInstanceId: "another-source" });
  assert.equal(container.querySelector('[aria-label="Revisão da aposentadoria do recibo"]'), null);
  await render({ instanceId: copy, sourceInstanceId: "original" }); assert.equal(button("Confirmar aposentadoria do recibo").disabled, true);
  assert.equal(count("confirm_retirement"), 0);
});

test("retirement distinguishes an absent file, an empty file and restored versus retired history", async () => {
  retirementFixtures(); snapshot = retirementSnapshot(); snapshot.retirementReview.currentConfigSha256 = null; snapshot.retirementReview.currentConfigBytes = null;
  await render(); assert.match(container.textContent, /arquivo de configuração estava ausente/);
  move({ retirementReview: { ...retirementReview(), currentConfigBytes: 0 } }); await refresh();
  assert.match(container.textContent, /Arquivo observado: 0 bytes/); assert.doesNotMatch(container.textContent, /estava ausente/);
  await click("Cancelar revisão e manter recibo"); await click("Encerrar operação de anúncio");
  await check("Autorizo sair mantendo a configuração"); await click("Sair mantendo alteração na cópia");
  handlers.set("voice_announcement_list_receipts", () => ({ instanceId: copy, receipts: [receiptRow("simple_voice_chat", "retired"), { ...receiptRow("simple_voice_chat", "restored"), receiptId: PLAN }] }));
  await click("Consultar recibos salvos"); assert.match(container.textContent, /Restauração já concluída/); assert.match(container.textContent, /Recibo aposentado; não pode ser retomado/);
  assert.equal(hasButton("Retomar restauração"), false); assert.equal(hasButton("Revisar aposentadoria do recibo"), false);
});

test("lost retirement preparation response only presents the retained review with fresh consent", async () => {
  retirementFixtures(); handlers.set("voice_announcement_prepare_retirement", () => {
    move({ ...retirementSnapshot(), generation: snapshot.generation }); throw "voice_storage";
  });
  await render(); await openRetirementReview(); await tick();
  assert.equal(count("prepare_retirement"), 1); assert.equal(button("Confirmar aposentadoria do recibo").disabled, true);
  assert.equal(count("confirm_retirement"), 0); assert.equal(count("confirm"), 0);
});

test("lost committed retirement response reconciles retired without replay or restoration", async () => {
  retirementFixtures(); handlers.set("voice_announcement_confirm_retirement", () => {
    delete snapshot.retirementReview; move({ ...retiredSnapshot(), generation: snapshot.generation }); throw "voice_storage";
  });
  await render(); await openRetirementReview(); await check("Autorizo aposentar somente este recibo"); await click("Confirmar aposentadoria do recibo"); await tick();
  assert.equal(count("confirm_retirement"), 1); assert.equal(controller.snapshot.phase, "retired");
  assert.equal(count("prepare_restore"), 0); assert.equal(count("resume_receipt"), 0); assert.equal(count("confirm"), 0);
});

test("retired close history never claims the record was lost or the configuration restored", async () => {
  retirementFixtures(); snapshot = { ...retiredSnapshot(), phase: "closed", close: { closeId: CLOSE, intent: "panel", state: "ready", review: null } };
  await render(); assert.match(container.textContent, /registro permanece no histórico; a configuração não foi restaurada/);
  assert.doesNotMatch(container.textContent, /recibo não está mais disponível|recibo salvo permanece|Anúncio restaurado/);
  await click("Retomar trabalho"); assert.equal(controller.snapshot.phase, "idle");
  assert.equal(count("confirm_retirement"), 0); assert.equal(count("prepare_restore"), 0);
});

test("retirement executor uncertainty requires Recovery and cannot promise retired history", async () => {
  retirementFixtures(); handlers.set("voice_announcement_confirm_retirement", () => {
    delete snapshot.retirementReview; return move({ phase: "needsRecovery", restorationState: "needs_recovery", lastErrorCode: "voice_needs_recovery",
      durableReceipt: { state: "uncertain", receiptId: RECEIPT, generation: 0 } });
  });
  await render(); await openRetirementReview(); await check("Autorizo aposentar somente este recibo"); await click("Confirmar aposentadoria do recibo");
  assert.equal(hasButton("Revisar restauração estrita"), false); assert.doesNotMatch(container.textContent, /Recibo aposentado; configuração inalterada/);
  await click("Encerrar operação de anúncio"); assert.match(container.textContent, /existência de um recibo utilizável ainda não está confirmada/);
  assert.equal(count("prepare_restore"), 0); assert.equal(count("resume_receipt"), 0);
});

test("double retirement confirm and logout wait for the one worker before account cleanup", async () => {
  retirementFixtures(); snapshot = retirementSnapshot(); const waiting = deferred();
  handlers.set("voice_announcement_confirm_retirement", () => { delete snapshot.retirementReview; move({ phase: "retiring", busy: true }); return waiting.promise; });
  const closing = deferred(); handlers.set("voice_announcement_begin_close", () => {
    move({ phase: "closing", busy: true, close: { closeId: CLOSE, intent: "logout", state: "waiting", review: null } }); return closing.promise;
  });
  await render(); await check("Autorizo aposentar somente este recibo"); const confirm = button("Confirmar aposentadoria do recibo");
  await act(async () => { confirm.dispatchEvent(new window.Event("click", { bubbles: true })); confirm.dispatchEvent(new window.Event("click", { bubbles: true })); });
  assert.equal(count("confirm_retirement"), 1); await click("Sair da conta fixture"); await render({ panel: false }); assert.deepEqual(logoutSteps, []);
  await act(async () => {
    snapshot = { ...retiredSnapshot(), generation: 20, phase: "closed", close: { closeId: CLOSE, intent: "logout", state: "ready", review: null } };
    waiting.resolve(snapshot); closing.resolve(snapshot);
  }); await logoutPromise;
  assert.deepEqual(logoutSteps, ["connect", "firebase"]); assert.equal(count("finish_close"), 1); assert.equal(count("confirm_close"), 0);
  assert.equal(count("confirm_retirement"), 1);
});

test("retirement parsers reject cross-receipt reviews, wrong generations and configuration authority", () => {
  const validCaps = { ...caps(true), durableReceipts: { schemaVersion: 1, maxReceipts: 64, retirement: { schemaVersion: 1 } } };
  types.voiceCapabilitiesFrom(validCaps);
  for (const retirement of [true, null, {}, { schemaVersion: 2 }, { schemaVersion: 1, automatic: true }]) assert.throws(() => types.voiceCapabilitiesFrom({ ...validCaps, durableReceipts: { ...validCaps.durableReceipts, retirement } }));
  types.voiceSnapshotFrom(retirementSnapshot()); types.voiceSnapshotFrom(retiredSnapshot());
  types.voiceSnapshotFrom({ ...idle(), phase: "preparingRetirement", busy: true });
  for (const extra of [{ expectedGeneration: 1 }, { receiptId: POINT }, { voiceMod: "plasmo_voice" }, { currentConfigSha256: null },
    { currentConfigBytes: null }, { currentConfigBytes: -1 }, { currentConfigBytes: 262145 }, { currentConfigBytes: 1.5 }, { currentConfigBytes: [0] },
    { expiresAtMs: 1.5 }, { warning: "é".repeat(513) }, { warning: "bad\u0000text" }, { before: "arbitrary" }]) {
    assert.throws(() => types.voiceSnapshotFrom({ ...retirementSnapshot(), retirementReview: { ...retirementReview(), ...extra } }));
  }
  for (const snapshot of [{ ...retirementSnapshot(), retirementReview: null }, { ...retirementSnapshot(), review: review() },
    { ...retirementSnapshot(), durableReceipt: savedReceipt(1) }, { ...retirementSnapshot(), durableReceipt: undefined },
    { ...retiredSnapshot(), restorationState: "restored" }, { ...retiredSnapshot(), durableReceipt: { state: "retired", receiptId: RECEIPT, generation: 0 } },
    { ...retiredSnapshot(), phase: "restored" }, { ...retirementSnapshot(), phase: "retiring", busy: true },
    { ...durableApplied(), phase: "preparingRetirement", busy: true }]) assert.throws(() => types.voiceSnapshotFrom(snapshot));
  types.voiceReceiptListFrom({ instanceId: copy, receipts: [receiptRow("simple_voice_chat", "retired")] });
  assert.throws(() => types.voiceReceiptListFrom({ instanceId: copy, receipts: [{ ...receiptRow("simple_voice_chat", "retired"), generation: 0 }] }));
});
