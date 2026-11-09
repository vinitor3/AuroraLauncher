// Synthetic DOM/IPC only. No personal instance, Java, account or network access.
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
Object.defineProperty(window.HTMLInputElement.prototype, "type", {
  configurable: true, get() { return this.getAttribute("type") || "text"; }, set(value) { this.setAttribute("type", value); },
});
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Ready launch synthetic DOM" } });
const modules = new Map();
async function sourceModule(source) {
  if (modules.has(source.href)) return modules.get(source.href);
  let output = ts.transpileModule(await readFile(source, "utf8"), { fileName: fileURLToPath(source), compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX,
  } }).outputText.replace(/^import ["'][^"']+\.css["'];?$/gm, "");
  for (const { fileName: specifier } of ts.preProcessFile(output).importedFiles) {
    const dependency = specifier.startsWith(".") ? ["", ".ts", ".tsx"].map(ext => new URL(specifier + ext, source)).find(url => existsSync(fileURLToPath(url))) : undefined;
    const resolved = dependency ? await sourceModule(dependency) : pathToFileURL(require.resolve(specifier)).href;
    output = output.replaceAll(`from "${specifier}"`, `from ${JSON.stringify(resolved)}`);
  }
  const url = `data:text/javascript;base64,${Buffer.from(output).toString("base64")}`;
  modules.set(source.href, url); return url;
}
let calls, handlers, root, selection;
window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
  calls.push({ command, args }); const handler = handlers.get(command);
  if (!handler) throw new Error(`Unexpected command ${command}`);
  return handler(args);
} };
window.isTauri = true;
const { act, createElement } = await import("react");
const { createRoot } = await import("react-dom/client");
const { ReadyCopyPicker, readyCopyIdsFrom } = await import(await sourceModule(new URL("./ReadyCopyPicker.tsx", import.meta.url)));
const { ConnectGameLaunchPanel } = await import(await sourceModule(new URL("./ConnectGameLaunchPanel.tsx", import.meta.url)));
const { readyGameCapabilityFrom, readyGameSnapshotFrom, validReadyGameContext } = await import(await sourceModule(new URL("./types.ts", import.meta.url)));
const container = document.getElementById("root");
const copyId = "aurora-test-aaaaaaaa-1111-4111-8111-111111111111";
const copyB = "aurora-test-bbbbbbbb-2222-4222-8222-222222222222";
const copy = (id = copyId, state = "ready") => ({ id, state, instanceUuid: "dddddddd-3333-4333-8333-333333333333", originUuid: "eeeeeeee-4444-4444-8444-444444444444" });
const button = label => { const found = [...container.querySelectorAll("button")].find(element => element.textContent === label); assert.ok(found, label); return found; };
const click = async label => { assert.equal(button(label).disabled, false); await act(async () => button(label).dispatchEvent(new window.Event("click", { bubbles: true }))); };
const picker = async (props = {}) => { await act(async () => root.render(createElement(ReadyCopyPicker, {
  sourceInstanceId: "test-source", disabled: false, selectedId: "", onSelect: id => { selection.push(id); }, ...props,
}))); };
beforeEach(() => { calls = []; handlers = new Map(); selection = []; root = createRoot(container); });
afterEach(async () => { await act(async () => root.unmount()); });

test("ready copy selection reads only the existing list on explicit action and never installs or launches", async () => {
  handlers.set("diagnostic_lab_list", () => [copy(), copy(copyB, "creating")]);
  await picker(); assert.equal(calls.length, 0); await click("Listar cópias prontas");
  assert.deepEqual(calls, [{ command: "diagnostic_lab_list", args: { id: "test-source" } }]);
  const options = [...container.querySelectorAll("option")];
  assert.deepEqual(options.map(option => option.value), ["", copyId]);
  await act(async () => { options[1].selected = true; container.querySelector("select").dispatchEvent(new window.Event("change", { bubbles: true })); });
  assert.equal(selection.at(-1), copyId); assert.equal(calls.length, 1);
});

test("ready copy projection rejects duplicates, original instances and unverified ready identities", () => {
  assert.deepEqual(readyCopyIdsFrom([copy(), copy(copyB, "failed")]), [copyId]);
  for (const value of [[copy(), copy()], [copy("personal-instance")], [{ ...copy(), instanceUuid: "" }],
    [{ ...copy(), originUuid: null }], [{ ...copy(), state: "running" }], {}, null]) {
    assert.throws(() => readyCopyIdsFrom(value));
  }
});

test("late copy inventory from the previous source cannot become selected after source changes", async () => {
  let complete; handlers.set("diagnostic_lab_list", () => new Promise(resolve => { complete = resolve; }));
  await picker(); await click("Listar cópias prontas"); await picker({ sourceInstanceId: "second-source" });
  await act(async () => complete([copy()]));
  assert.equal(container.querySelector("select"), null); assert.equal(selection.at(-1), "");
  handlers.set("diagnostic_lab_list", () => [copy(copyB)]); await click("Listar cópias prontas");
  assert.deepEqual(calls.at(-1).args, { id: "second-source" });
  assert.equal([...container.querySelectorAll("option")].some(option => option.value === copyId), false);
});

test("disabled context cannot list and disabling during inventory hides the late response", async () => {
  await picker({ disabled: true }); assert.equal(button("Listar cópias prontas").disabled, true); assert.equal(calls.length, 0);
  let complete; handlers.set("diagnostic_lab_list", () => new Promise(resolve => { complete = resolve; }));
  await picker(); await click("Listar cópias prontas"); await picker({ disabled: true });
  await act(async () => complete([copy()])); assert.equal(container.querySelector("select"), null);
  assert.equal(selection.at(-1), "");
});

test("copy listing failure exposes only a fixed message and never uses a stale selection", async () => {
  handlers.set("diagnostic_lab_list", () => { throw new Error("synthetic private path must not be rendered"); });
  await picker(); await click("Listar cópias prontas");
  assert.match(container.textContent, /Não foi possível conferir/); assert.doesNotMatch(container.textContent, /private path/);
  assert.equal(container.querySelector("select"), null); assert.equal(selection.at(-1), "");
});

const operationId = "aaaaaaaa-1111-4111-8111-111111111111";
const otherOperation = "bbbbbbbb-1111-4111-8111-111111111111";
const planId = "cccccccc-1111-4111-8111-111111111111";
const context = { sourceInstanceId: "test-source", contextKey: "synthetic-context", javaExecutable: "C:\\fixture-java\\bin\\java.exe",
  nickname: "Player_73", appearance: { skinUrl: null, skinFile: null, capeUrl: null, skinModel: "classic" } };
const capability = { schemaVersion: 1, instanceScope: "diagnosticCopy", maxPlanSeconds: 60, minecraftVersion: "1.21.1",
  loader: "fabric", loaderVersion: "0.16.14", javaMajor: 21, coreVersion: "1.1.5", skinsVersion: "0.3.1" };
const connection = { operationId, role: "guest", phase: "guestConnected", transportState: "authenticated", cleanupState: "pending", tcpAddress: "127.0.0.1:42010" };
const idle = () => ({ operationId, generation: 0, phase: "idle", planId: null, review: null, result: null, errorCode: null });
const review = () => ({ operationId, planId, ...structuredClone(context), instanceId: copyId, digest: "a".repeat(64),
  expiresAtMs: Date.now() + 50_000, sessionExpiresAtMs: Date.now() + 500_000, minecraftVersion: "1.21.1", loader: "fabric", loaderVersion: "0.16.14",
  javaMajor: 21, gameAddress: "127.0.0.1:42010", coreVersion: "1.1.5", coreSha256: "b".repeat(64), skinsVersion: "0.3.1", skinsSha256: "c".repeat(64) });
const reviewed = () => ({ ...idle(), generation: 2, phase: "reviewed", planId, review: review() });
const terminal = (phase, generation = 4) => ({ ...idle(), planId, generation, phase, ...(phase === "started" ? { result: {
  runId: "dddddddd-1111-4111-8111-111111111111", processId: 7310, instanceId: copyId, worldState: "notVerified",
} } : phase === "failed" ? { errorCode: "connect_ready_game_launch_failed" } : {}) });
let nativeSnapshot;
const count = command => calls.filter(call => call.command === command).length;
const panel = async (props = {}) => { await act(async () => root.render(createElement(ConnectGameLaunchPanel, {
  capability, session: connection, launchContext: context, ...props,
}))); };
function fixtures(initial = idle()) {
  nativeSnapshot = initial;
  handlers.set("connect_ready_game_launch_snapshot", () => structuredClone(nativeSnapshot));
  handlers.set("diagnostic_lab_list", () => [copy()]);
  handlers.set("connect_prepare_ready_game_launch", ({ request }) => { nativeSnapshot = reviewed(); nativeSnapshot.review = { ...nativeSnapshot.review, ...request }; return structuredClone(nativeSnapshot); });
  handlers.set("connect_confirm_ready_game_launch", () => { nativeSnapshot = terminal("started"); return structuredClone(nativeSnapshot); });
  handlers.set("connect_cancel_ready_game_launch", () => { nativeSnapshot = terminal("cancelled"); return structuredClone(nativeSnapshot); });
}
async function chooseCopy() {
  await click("Listar cópias prontas");
  await act(async () => { container.querySelectorAll("option")[1].selected = true;
    container.querySelector("select").dispatchEvent(new window.Event("change", { bubbles: true })); });
}
async function consent() {
  const input = container.querySelector(".connect-consent input"); assert.ok(input); assert.equal(input.disabled, false);
  await act(async () => { input.checked = !input.checked; input.dispatchEvent(new window.Event("click", { bubbles: true })); });
}

test("legacy ready launch capability is explicit and fixed to the original verified pair", () => {
  assert.deepEqual(readyGameCapabilityFrom(capability), capability);
  for (const invalid of [null, { ...capability, schemaVersion: 2 }, { ...capability, javaMajor: 17 }, { ...capability, instanceScope: "personal" },
    { ...capability, loader: "forge" }, { ...capability, maxPlanSeconds: 600 }, { ...capability, coreVersion: "1.2.0" }, { ...capability, relay: true }]) {
    assert.throws(() => readyGameCapabilityFrom(invalid));
  }
});

test("launch snapshot parser rejects mismatched authority projections and never accepts world claims", () => {
  const valid = reviewed(); assert.deepEqual(readyGameSnapshotFrom(valid), valid); assert.deepEqual(readyGameSnapshotFrom(terminal("started")), terminal("started"));
  const invalids = [ { ...valid, generation: -1 }, { ...valid, planId: null }, { ...valid, result: terminal("started").result },
    { ...valid, review: { ...valid.review, operationId: otherOperation } }, { ...valid, review: { ...valid.review, gameAddress: "10.0.0.1:42010" } },
    { ...valid, review: { ...valid.review, gameAddress: "127.0.0.1:65536" } }, { ...valid, review: { ...valid.review, gameAddress: "127.0.0.1:042010" } },
    { ...valid, review: { ...valid.review, expiresAtMs: valid.review.sessionExpiresAtMs + 1 } }, { ...valid, review: { ...valid.review, nickname: "x" } },
    { ...valid, review: { ...valid.review, instanceId: "personal" } }, { ...valid, review: { ...valid.review, coreSha256: "wrong" } },
    { ...valid, review: { ...valid.review, appearance: { ...context.appearance, secret: "must-not-display" } } },
    { ...terminal("started"), result: { ...terminal("started").result, worldState: "joined" } }, { ...terminal("failed"), errorCode: "raw-private-message" },
    { ...idle(), generation: 1 }, { ...valid, extra: true } ];
  for (const value of invalids) assert.throws(() => readyGameSnapshotFrom(value));
  assert.equal(validReadyGameContext(context), true); assert.equal(validReadyGameContext({ ...context, contextKey: "x".repeat(513) }), false);
});

test("disabled or absent capability starts no listing, snapshot or launch on the normal legacy panel", async () => {
  fixtures(); await panel({ capability: undefined }); assert.equal(calls.length, 0); assert.equal(container.textContent, "");
  await panel({ session: { ...connection, role: "host", phase: "hostListening" } }); assert.equal(calls.length, 0);
});

test("ready launch performs explicit copy selection, matching review and fresh consent before one launch", async () => {
  fixtures(); await panel(); assert.deepEqual(calls.map(c => c.command), ["connect_ready_game_launch_snapshot"]);
  await chooseCopy(); await click("Revisar entrada no servidor");
  assert.deepEqual(calls.find(c => c.command === "connect_prepare_ready_game_launch").args, { request: { operationId, instanceId: copyId, ...context } });
  assert.equal(count("connect_confirm_ready_game_launch"), 0); assert.equal(button("Iniciar cópia e tentar entrar").disabled, true);
  assert.match(container.textContent, /Java 21/); assert.match(container.textContent, /Player_73/); assert.match(container.textContent, /gravar dados nesta cópia/);
  await consent(); await click("Iniciar cópia e tentar entrar");
  assert.deepEqual(calls.find(c => c.command === "connect_confirm_ready_game_launch").args, { operationId, planId, digest: "a".repeat(64) });
  assert.equal(count("connect_confirm_ready_game_launch"), 1); assert.match(container.textContent, /Confira a entrada no mundo no Minecraft/);
  assert.match(container.textContent, /Encerrar o Connect não fecha o jogo/);
  assert.equal(calls.some(c => ["launch_instance", "launch_instance_with_doctor_run", "install_java", "install_fabric", "save_settings"].includes(c.command)), false);
});

test("an authorized local host player can review voice files and confirm the same native operation", async () => {
  fixtures();
  const host = { ...connection, role: "host", phase: "hostListening", transportState: "admitted",
    social: { role: "host", hostPlayer: true, readyGameLaunchAvailable: true } };
  handlers.set("connect_prepare_ready_game_launch", ({ request }) => {
    nativeSnapshot = reviewed();
    nativeSnapshot.review = { ...nativeSnapshot.review, ...request, voice: { profile: "simpleVoiceChat", mods: [
      { relative: "mods/voicechat-fabric-1.21.1-2.6.22.jar", bytes: 5_369_475, sha256: "d".repeat(64), action: "install" },
    ] } };
    return structuredClone(nativeSnapshot);
  });
  await panel({ session: host }); await chooseCopy(); await click("Revisar entrada no servidor");
  assert.match(container.textContent, /Adicionar à cópia.*voicechat-fabric-1.21.1-2.6.22/);
  assert.match(container.textContent, /Autorizo instalar os mods listados nesta cópia de teste/);
  assert.deepEqual(calls.find(c => c.command === "connect_prepare_ready_game_launch").args, { request: { operationId, instanceId: copyId, ...context } });
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, true);
  await consent(); await click("Iniciar cópia e tentar entrar");
  assert.equal(count("connect_confirm_ready_game_launch"), 1);
});

test("a reviewed official runtime copy supplies its own Java on a fresh launcher", async () => {
  fixtures();
  const ownJava = `C:/fixture/instances/${copyId}/installers/aurora-managed/runtimes/java-21/jre/bin/java.exe`;
  handlers.set("connect_install_ready_copy", () => ({ ...copy(), originInstanceId: context.sourceInstanceId, runtimeJavaExecutable: ownJava }));
  await panel({ launchContext: { ...context, javaExecutable: "" } });
  const select = container.querySelector(".connect-fields details select");
  assert.ok(select);
  await act(async () => {
    [...select.querySelectorAll("option")].find(option => option.value === "fabric-loader-0.16.14-1.21.1").selected = true;
    select.dispatchEvent(new window.Event("change", { bubbles: true }));
  });
  assert.equal(button("Criar cópia com este runtime").disabled, true);
  const approval = container.querySelector(".connect-fields details input[type=checkbox]");
  await act(async () => { approval.checked = true; approval.dispatchEvent(new window.Event("click", { bubbles: true })); });
  await click("Criar cópia com este runtime");
  const install = calls.find(call => call.command === "connect_install_ready_copy").args.request;
  assert.match(install.operationId, /^[a-f0-9-]{36}$/);
  assert.deepEqual({ ...install, operationId: undefined }, {
    operationId: undefined, connectOperationId: operationId, sourceInstanceId: context.sourceInstanceId, profileId: "fabric-loader-0.16.14-1.21.1", approved: true,
  });
  assert.equal(count("connect_prepare_ready_game_launch"), 0);
  await click("Revisar entrada no servidor");
  assert.equal(calls.find(call => call.command === "connect_prepare_ready_game_launch").args.request.javaExecutable, ownJava);
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, true);
});

const profile1211 = { minecraftVersion: "1.21.1", loader: "fabric", loaderVersion: "0.16.14", javaMajor: 21, coreVersion: "1.1.5", skinsVersion: "0.3.1" };
const profile1201 = { ...profile1211, minecraftVersion: "1.20.1", javaMajor: 17 };
const capability93 = { schemaVersion: 2, instanceScope: "diagnosticCopy", maxPlanSeconds: 60, runtimes: [profile1211, profile1201] };

test("runtime copy creation rejects any source ID with the reserved diagnostic prefix", async () => {
  for (const sourceInstanceId of [copyId, "aurora-test-origem"]) {
    await picker({ sourceInstanceId, connectOperationId: operationId, runtimes: [profile1211] });
    assert.match(container.textContent, /Selecione a instância de origem para preparar outra cópia/);
    assert.equal(container.querySelector("details"), null);
    assert.equal(button("Listar cópias prontas").disabled, false);
  }
  assert.equal(calls.length, 0);
});
test("a signed session preselects its only compatible runtime and ignores old install completion after account change", async () => {
  fixtures();
  let complete;
  handlers.set("connect_install_ready_copy", () => new Promise(resolve => { complete = resolve; }));
  const profileId = "fabric-loader-0.16.14-1.20.1";
  const signed = { ...connection, social: { sessionId: operationId, role: "guest", stage: "connected",
    runtimeProfile: profileId } };
  await panel({ capability: capability93, session: signed });
  const select = container.querySelector(".connect-fields details select");
  assert.ok(select.disabled);
  assert.deepEqual([...select.querySelectorAll("option")].map(option => option.value), ["", profileId]);
  const approval = container.querySelector(".connect-fields details input[type=checkbox]");
  assert.ok(approval);
  await act(async () => { approval.checked = true; approval.dispatchEvent(new window.Event("click", { bubbles: true })); });
  await click("Criar cópia com este runtime");
  const request = calls.find(call => call.command === "connect_install_ready_copy").args.request;
  assert.equal(request.connectOperationId, operationId);
  assert.equal(request.profileId, profileId);
  nativeSnapshot = { ...idle(), operationId: otherOperation };
  await panel({ capability: capability93, session: { ...signed, operationId: otherOperation } });
  const java = `C:/fixture/instances/${copyId}/installers/aurora-managed/runtimes/java-17/jre/bin/java.exe`;
  await act(async () => complete({ ...copy(), originInstanceId: context.sourceInstanceId, runtimeJavaExecutable: java }));
  assert.equal(button("Revisar entrada no servidor").disabled, true);
  assert.equal(count("connect_prepare_ready_game_launch"), 0);
});

test("Fabric1201 review requires schema2 and fresh consent without adding runtime input to IPC", async () => {
  fixtures();
  handlers.set("connect_prepare_ready_game_launch", ({ request }) => {
    nativeSnapshot = reviewed(); nativeSnapshot.review = { ...nativeSnapshot.review, ...request, ...profile1201 };
    return structuredClone(nativeSnapshot);
  });
  await panel({ capability: capability93 }); await chooseCopy(); await click("Revisar entrada no servidor");
  assert.match(container.textContent, /Minecraft 1\.20\.1 \/ Fabric 0\.16\.14/);
  assert.match(container.textContent, /\(Java 17\)/);
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, true);
  assert.deepEqual(calls.find(c => c.command === "connect_prepare_ready_game_launch").args, { request: { operationId, instanceId: copyId, ...context } });
  await consent(); await click("Iniciar cópia e tentar entrar");
  assert.equal(count("connect_confirm_ready_game_launch"), 1);
  assert.deepEqual(calls.find(c => c.command === "connect_confirm_ready_game_launch").args, { operationId, planId, digest: "a".repeat(64) });
});

test("losing schema2 authorization disables a Fabric1201 review while keeping cancellation", async () => {
  const initial = reviewed(); initial.review = { ...initial.review, ...profile1201 };
  fixtures(initial); await panel({ capability: capability93 }); await consent();
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, false);
  await panel({ capability });
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, true);
  assert.equal(container.querySelector(".connect-consent input").disabled, true);
  assert.match(container.textContent, /não oferece o perfil desta cópia/);
  assert.equal(count("connect_confirm_ready_game_launch"), 0);
  await click("Cancelar preparo do jogo");
  assert.equal(count("connect_cancel_ready_game_launch"), 1);
});

test("a Forge Java 8 review needs matrix capability and renewed consent after capability loss", async () => {
  const matrix = JSON.parse(await readFile(new URL("../../../../../scripts/active-mod-matrix.json", import.meta.url), "utf8"));
  const runtimes = matrix.targets.map(target => ({
    minecraftVersion: target.minecraft, loader: target.loader, loaderVersion: target.loaderVersion,
    javaMajor: target.javaRuntime, coreVersion: "1.1.5", skinsVersion: "0.3.1",
  }));
  const full = { ...capability93, schemaVersion: 3, runtimes };
  const initial = reviewed();
  initial.review = { ...initial.review, ...runtimes.find(runtime => runtime.minecraftVersion === "1.12.2") };
  fixtures(initial); await panel({ capability: full });
  assert.match(container.textContent, /Minecraft 1\.12\.2 \/ Forge 14\.23\.5\.2860/);
  assert.match(container.textContent, /\(Java 8\)/);
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, true);
  await consent();
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, false);
  await panel({ capability: capability93 });
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, true);
  await panel({ capability: full });
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, true);
  await consent(); await click("Iniciar cópia e tentar entrar");
  assert.deepEqual(calls.find(c => c.command === "connect_confirm_ready_game_launch").args, { operationId, planId, digest: "a".repeat(64) });
});

test("remount recovers a native review but never recovers consent or launches automatically", async () => {
  fixtures(reviewed()); await panel(); await consent(); assert.equal(button("Iniciar cópia e tentar entrar").disabled, false);
  await act(async () => root.unmount()); root = createRoot(container); await panel();
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, true); assert.equal(count("connect_confirm_ready_game_launch"), 0);
});

test("changing account context invalidates and cancels the previous plan without stopping transport", async () => {
  fixtures(reviewed()); await panel(); await consent();
  await panel({ launchContext: { ...context, contextKey: "another-account", nickname: "OtherPlayer" } });
  assert.equal(count("connect_cancel_ready_game_launch"), 1); assert.equal(count("connect_confirm_ready_game_launch"), 0);
  assert.equal(count("connect_stop"), 0); assert.match(container.textContent, /Preparo cancelado/); assert.doesNotMatch(container.textContent, /Player_73/);
});

test("losing ready appearance context cancels the review and shows the missing profile instructions", async () => {
  fixtures(reviewed()); await panel(); await panel({ launchContext: undefined });
  assert.equal(count("connect_cancel_ready_game_launch"), 1); assert.match(container.textContent, /aguarde o perfil e a aparência/);
  assert.equal(button("Revisar entrada no servidor").disabled, true);
});

test("context change while prepare reply is pending cancels its late plan and cannot restore old review", async () => {
  fixtures(); let complete;
  handlers.set("connect_prepare_ready_game_launch", () => new Promise(resolve => { complete = resolve; }));
  await panel(); await chooseCopy(); await click("Revisar entrada no servidor");
  await panel({ launchContext: { ...context, contextKey: "new-selection", sourceInstanceId: "another-source" } });
  nativeSnapshot = reviewed(); await act(async () => complete(structuredClone(nativeSnapshot)));
  assert.equal(count("connect_cancel_ready_game_launch"), 1); assert.equal(count("connect_confirm_ready_game_launch"), 0);
  assert.doesNotMatch(container.textContent, /Player_73/); assert.match(container.textContent, /Preparo cancelado/);
});

test("cancel racing a confirmation preserves a started result and never stops Minecraft", async () => {
  fixtures(reviewed()); let complete;
  handlers.set("connect_confirm_ready_game_launch", () => new Promise(resolve => { complete = resolve; }));
  handlers.set("connect_cancel_ready_game_launch", () => { nativeSnapshot = terminal("started", 4); return structuredClone(nativeSnapshot); });
  await panel(); await consent(); await click("Iniciar cópia e tentar entrar"); await click("Cancelar preparo do jogo");
  await act(async () => complete({ ...terminal("cancelled", 3) }));
  assert.match(container.textContent, /A cópia foi iniciada/); assert.doesNotMatch(container.textContent, /Preparo cancelado/);
  assert.equal(count("connect_confirm_ready_game_launch"), 1); assert.equal(count("stop_instance"), 0);
});

test("lost confirmation response is resolved by observation without repeating launch", async () => {
  fixtures(reviewed()); handlers.set("connect_confirm_ready_game_launch", () => { nativeSnapshot = terminal("started"); throw new Error("synthetic lost IPC with private details"); });
  await panel(); await consent(); await click("Iniciar cópia e tentar entrar");
  assert.equal(count("connect_confirm_ready_game_launch"), 1); assert.match(container.textContent, /A cópia foi iniciada/);
  assert.doesNotMatch(container.textContent, /private details/); assert.equal(count("connect_prepare_ready_game_launch"), 0);
  await click("Atualizar preparo do jogo"); assert.equal(count("connect_confirm_ready_game_launch"), 1);
});

test("lost prepare response with unchanged native observation keeps a barrier until a newer result", async () => {
  fixtures(); handlers.set("connect_prepare_ready_game_launch", () => { throw new Error("IPC gone"); });
  await panel(); await chooseCopy(); await click("Revisar entrada no servidor");
  assert.equal(button("Revisar entrada no servidor").disabled, true); assert.match(container.textContent, /Aguardando uma observação/);
  await click("Atualizar preparo do jogo"); assert.equal(button("Revisar entrada no servidor").disabled, true);
  nativeSnapshot = terminal("cancelled"); await click("Atualizar preparo do jogo");
  assert.equal(count("connect_prepare_ready_game_launch"), 1); assert.doesNotMatch(container.textContent, /Aguardando uma observação/);
});

test("stale response from an old Connect operation cannot populate the new operation", async () => {
  fixtures(); let complete;
  handlers.set("connect_prepare_ready_game_launch", () => new Promise(resolve => { complete = resolve; }));
  await panel(); await chooseCopy(); await click("Revisar entrada no servidor");
  nativeSnapshot = { ...idle(), operationId: otherOperation }; await panel({ session: { ...connection, operationId: otherOperation } });
  await act(async () => complete(reviewed())); assert.doesNotMatch(container.textContent, /Confira antes de iniciar/);
  assert.equal(count("connect_confirm_ready_game_launch"), 0);
});

test("polling the identical review keeps consent while a newer digest always clears it", async () => {
  fixtures(reviewed()); await panel(); await consent(); await click("Atualizar preparo do jogo");
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, false);
  nativeSnapshot = { ...nativeSnapshot, generation: 3, review: { ...nativeSnapshot.review, digest: "d".repeat(64) } };
  await click("Atualizar preparo do jogo"); assert.equal(button("Iniciar cópia e tentar entrar").disabled, true);
});

test("inconsistent equal generation is rejected and disables consent without displaying substituted data", async () => {
  fixtures(reviewed()); await panel(); await consent();
  nativeSnapshot = { ...nativeSnapshot, review: { ...nativeSnapshot.review, nickname: "InjectedPlayer" } };
  await click("Atualizar preparo do jogo"); assert.equal(button("Iniciar cópia e tentar entrar").disabled, true);
  assert.match(container.textContent, /não pôde ser atualizado/); assert.doesNotMatch(container.textContent, /InjectedPlayer/);
});

test("expiration is rechecked on confirm even before the UI polling clock updates", async () => {
  fixtures(reviewed()); await panel(); await consent();
  const originalNow = Date.now; Date.now = () => nativeSnapshot.review.expiresAtMs;
  try { await click("Iniciar cópia e tentar entrar"); } finally { Date.now = originalNow; }
  assert.equal(count("connect_confirm_ready_game_launch"), 0); assert.equal(button("Iniciar cópia e tentar entrar").disabled, true);
});

const socialConnection = { ...connection, social: { sessionId: "11111111-2222-4333-8444-555555555555",
  role: "guest", stage: "connected", digest: null, gameTarget: null, visibility: null, expiresAtMs: null } };

test("a forward wall-clock jump after observing a Social review preserves its original consent and confirm window", async t => {
  fixtures(reviewed()); await panel({ session: socialConnection }); await consent();
  const expiresAt = nativeSnapshot.review.expiresAtMs;
  t.mock.method(Date, "now", () => expiresAt + 120_000);
  await click("Atualizar preparo do jogo");
  await click("Iniciar cópia e tentar entrar");
  assert.equal(count("connect_confirm_ready_game_launch"), 1);
});

test("polling the same Social review after a wall rollback never resets its monotonic deadline", async t => {
  let monotonic = 10_000;
  t.mock.method(performance, "now", () => monotonic);
  fixtures(reviewed()); await panel({ session: socialConnection }); await consent();
  const wall = Date.now();
  t.mock.method(Date, "now", () => wall - 120_000);
  monotonic += 49_000;
  await click("Atualizar preparo do jogo");
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, false);
  monotonic += 2_000;
  await click("Atualizar preparo do jogo");
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, true);
  assert.equal(count("connect_confirm_ready_game_launch"), 0);
});

test("TCP remains sufficient when UDP failed but closed TCP cannot authorize launch", async () => {
  fixtures(reviewed()); await panel({ session: { ...connection, udpState: "failed" } }); await consent();
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, false);
  await panel({ session: { ...connection, phase: "closed", transportState: "closed", cleanupState: "complete", tcpAddress: null } });
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, true); assert.equal(count("connect_confirm_ready_game_launch"), 0);
});

test("missing capability after observation preserves cancel and querying but blocks new launch", async () => {
  fixtures(reviewed()); await panel(); await panel({ capability: undefined });
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, true); await click("Cancelar preparo do jogo");
  assert.equal(count("connect_cancel_ready_game_launch"), 1); assert.match(container.textContent, /Preparo cancelado/);
});

test("capability-query failure can recover and cancel an existing plan without enabling launch", async () => {
  fixtures(reviewed()); await panel({ capability: undefined, observeExisting: true });
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, true); await click("Cancelar preparo do jogo");
  assert.equal(count("connect_cancel_ready_game_launch"), 1); assert.equal(count("connect_confirm_ready_game_launch"), 0);
});

test("review from an old account found after remount is cancelled without exposing its nickname", async () => {
  fixtures(reviewed()); await panel({ launchContext: { ...context, contextKey: "different-context" } });
  assert.equal(count("connect_cancel_ready_game_launch"), 1); assert.doesNotMatch(container.textContent, /Player_73/);
});

test("failed cancellation after context change retains manual cleanup without an automatic retry loop", async () => {
  fixtures(reviewed()); handlers.set("connect_cancel_ready_game_launch", () => { throw "connect_ready_game_internal"; });
  await panel(); await panel({ launchContext: { ...context, contextKey: "another-selection" } });
  assert.equal(count("connect_cancel_ready_game_launch"), 1); assert.equal(button("Revisar entrada no servidor").disabled, true);
  await click("Atualizar preparo do jogo"); await click("Atualizar preparo do jogo");
  assert.equal(count("connect_cancel_ready_game_launch"), 1); await click("Cancelar preparo do jogo");
  assert.equal(count("connect_cancel_ready_game_launch"), 2); assert.equal(count("connect_confirm_ready_game_launch"), 0);
});

test("stale parent connection observation disables prepare and confirmation while preserving cancellation", async () => {
  fixtures(reviewed()); await panel(); await consent(); await panel({ sessionStale: true });
  assert.equal(button("Iniciar cópia e tentar entrar").disabled, true);
  assert.match(container.textContent, /Aguarde uma observação atual do Connect/); await click("Cancelar preparo do jogo");
  assert.equal(count("connect_cancel_ready_game_launch"), 1); assert.equal(count("connect_confirm_ready_game_launch"), 0);
});

test("native projection cannot substitute a different copy after the user selected one for preparation", async () => {
  fixtures(); handlers.set("connect_prepare_ready_game_launch", () => {
    nativeSnapshot = reviewed(); nativeSnapshot.review.instanceId = copyB; return structuredClone(nativeSnapshot);
  });
  await panel(); await chooseCopy(); await click("Revisar entrada no servidor");
  assert.doesNotMatch(container.textContent, /Confira antes de iniciar/); assert.equal(count("connect_confirm_ready_game_launch"), 0);
  assert.equal(button("Revisar entrada no servidor").disabled, true); assert.match(container.textContent, /Aguardando uma observação/);
});
