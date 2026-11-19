// Synthetic React DOM coverage; this is not Minecraft or native WebView validation.
// Run: node apps/desktop/src/features/java-engine/LaunchSettingsPanel.smoke.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";

const require = createRequire(import.meta.url);
const { parseHTML } = require("linkedom");
const { window, document } = parseHTML("<html><body><div id='root'></div></body></html>");
// Linkedom lacks the browser's reflected textarea.defaultValue property.
Object.defineProperty(window.HTMLTextAreaElement.prototype, "defaultValue", {
  configurable: true, get() { return this.textContent; }, set(value) { this.textContent = value; },
});
// Advertise the native input event used by modern WebViews to React's detector.
Object.defineProperty(document, "oninput", { configurable: true, value: null });
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Synthetic React DOM smoke" } });
const pending = [];
const calls = [];
window.__TAURI_INTERNALS__ = {
  invoke(command, args) {
    assert.ok(["read_launch_settings", "save_launch_settings", "read_global_memory", "save_global_memory"].includes(command), `Unexpected native command ${command}`);
    calls.push({ command, args });
    return new Promise((resolve, reject) => pending.push({ command, args, resolve, reject }));
  },
};
const { act, createElement } = await import("react");
const { createRoot } = await import("react-dom/client");
const modules = new Map();
async function sourceModule(sourceUrl) {
  if (modules.has(sourceUrl.href)) return modules.get(sourceUrl.href);
  let output = ts.transpileModule(await readFile(sourceUrl, "utf8"), {
    fileName: fileURLToPath(sourceUrl),
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX },
  }).outputText.replace(/^import ["'][^"']+\.css["'];?$/gm, "");
  for (const { fileName: specifier } of ts.preProcessFile(output).importedFiles) {
    let resolved;
    if (specifier.startsWith(".")) {
      const dependency = ["", ".ts", ".tsx"].map((extension) => new URL(specifier + extension, sourceUrl)).find((url) => existsSync(fileURLToPath(url)));
      assert.ok(dependency, `Missing module ${specifier}`);
      resolved = await sourceModule(dependency);
    } else resolved = pathToFileURL(require.resolve(specifier)).href;
    output = output.replaceAll(`from "${specifier}"`, `from ${JSON.stringify(resolved)}`);
  }
  const result = `data:text/javascript;base64,${Buffer.from(output).toString("base64")}`;
  modules.set(sourceUrl.href, result);
  return result;
}
const { LaunchSettingsPanel } = await import(await sourceModule(new URL("./LaunchSettingsPanel.tsx", import.meta.url)));
const { defaultLaunchSettings } = await import(await sourceModule(new URL("./types.ts", import.meta.url)));
const container = document.getElementById("root");
const root = createRoot(container);
const props = { instanceId: "instance-a", busy: false, running: false };
const render = async (overrides = {}) => { await act(async () => root.render(createElement(LaunchSettingsPanel, { ...props, ...overrides }))); };
const overview = (settings = {}) => ({ settings: { ...defaultLaunchSettings(), memoryMode: "manual", maxMemoryMb: 6144, jvmArgs: ["-ea"], ...settings }, globalMaxMemoryMb: 4096, totalMemoryMb: 16384, modCount: 100, recommendedMemoryMb: 4800 });
const button = (text) => { const result = [...container.querySelectorAll("button")].find((item) => item.textContent === text); assert.ok(result, `Missing button ${text}`); return result; };
const click = async (text) => { assert.equal(button(text).disabled, false, `Disabled ${text}`); await act(async () => button(text).dispatchEvent(new window.Event("click", { bubbles: true }))); };
const submit = async () => { await act(async () => container.querySelector("form").dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true }))); };
const settle = async (command, result, reject = false) => {
  const index = pending.findIndex((item) => item.command === command);
  assert.ok(index >= 0, `Missing pending ${command}`);
  const request = pending.splice(index, 1)[0];
  await act(async () => (reject ? request.reject : request.resolve)(result));
  return request;
};
const field = (suffix) => { const result = container.querySelector(`[id$="-${suffix}"]`); assert.ok(result, `Missing field ${suffix}`); return result; };
const select = async (suffix, value) => {
  await act(async () => {
    field(suffix).querySelector(`option[value="${value}"]`).selected = true;
    field(suffix).dispatchEvent(new window.Event("change", { bubbles: true }));
  });
};
const input = async (suffix, value) => {
  await act(async () => {
    const element = field(suffix);
    const prototype = element.localName === "textarea" ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(prototype, "value").set.call(element, value);
    element.dispatchEvent(new window.Event("input", { bubbles: true }));
  });
};

await render();
assert.match(container.textContent, /Lendo as configurações/);
assert.deepEqual(calls[0], { command: "read_launch_settings", args: { id: "instance-a" } });
await settle("read_launch_settings", overview());
assert.equal(container.querySelector("section").dataset.themeSurface, "java-memory-settings");
assert.equal(button("Salvar configurações").disabled, true);
assert.equal(field("maximum").value, "6144");
assert.equal(field("arguments").value, "-ea");
assert.match(container.textContent, /16384 MB.*100 JARs.*4800 MB/s);
for (const element of container.querySelectorAll("input,select,textarea")) {
  assert.ok(container.querySelector(`label[for="${element.id}"]`), `Missing accessible label ${element.id}`);
}

await click("Restaurar padrões");
assert.equal(field("maximum").value, "4096");
assert.equal(calls.filter((item) => item.command === "save_launch_settings").length, 0, "Reset cannot persist silently");
await click("Cancelar alterações");
assert.equal(field("maximum").value, "6144");
assert.equal(field("arguments").value, "-ea");
await click("Restaurar padrões");
await submit();
assert.equal(button("Salvando…").disabled, true);
assert.deepEqual(pending[0].args, { id: "instance-a", settings: defaultLaunchSettings(4096) });
await settle("save_launch_settings", overview(defaultLaunchSettings(4096)));
assert.match(container.textContent, /Configurações salvas para a próxima inicialização/);
assert.equal(button("Salvar configurações").disabled, true);

assert.equal(field("maximum").disabled, true);
await select("memory-mode", "manual");
await input("maximum", "100");
await submit();
assert.match(container.textContent, /entre 512 e 262144 MB/);
assert.equal(pending.length, 0, "Invalid manual RAM must not invoke native save");
await input("maximum", "8192");
await input("minimum", "9000");
await submit();
assert.match(container.textContent, /A memória inicial deve ficar/);
assert.equal(pending.length, 0);
await input("minimum", "1024");
await input("arguments", "-XX:MaxGCPauseMillis=200\n-Xss1M\n\n-ea");
await submit();
assert.deepEqual(pending[0].args.settings, { ...defaultLaunchSettings(), memoryMode: "manual", maxMemoryMb: 8192, minMemoryMb: 1024, jvmArgs: ["-XX:MaxGCPauseMillis=200", "-Xss1M", "-ea"] });
await settle("save_launch_settings", overview(pending[0].args.settings));

await select("memory-mode", "automatic");
assert.equal(field("maximum").disabled, true);
assert.equal(field("minimum").disabled, true);
assert.match(container.textContent, /recalcula o limite ao iniciar/);
await select("collector", "recommended");
await submit();
assert.equal(pending[0].args.settings.memoryMode, "automatic");
assert.equal(pending[0].args.settings.minMemoryMb, null);
assert.equal(pending[0].args.settings.gcMode, "recommended");
await settle("save_launch_settings", "SYNTHETIC_PRIVATE_ERROR password=secret C:\\Private\\settings.json", true);
assert.match(container.textContent, /Não foi possível salvar/);
assert.doesNotMatch(container.textContent, /SYNTHETIC_PRIVATE_ERROR|password|Private/);
assert.equal(button("Salvar configurações").disabled, false, "Failed save keeps draft available");
await click("Cancelar alterações");
assert.equal(field("maximum").disabled, false);

// An invalid manual edit must not trap the user behind disabled automatic fields.
for (const invalidMaximum of ["100", ""]) {
  await input("maximum", invalidMaximum);
  await select("memory-mode", "automatic");
  assert.equal(field("maximum").disabled, true);
  await submit();
  assert.equal(pending[0].command, "save_launch_settings");
  assert.equal(pending[0].args.settings.memoryMode, "automatic");
  assert.equal(pending[0].args.settings.maxMemoryMb, 4096);
  assert.equal(pending[0].args.settings.minMemoryMb, null);
  await settle("save_launch_settings", overview(pending[0].args.settings));
  assert.match(container.textContent, /Configurações salvas/);
  await select("memory-mode", "manual");
}

await render({ busy: true });
assert.equal(container.querySelector("fieldset").hasAttribute("disabled"), true);
assert.equal(button("Restaurar padrões").disabled, true);
const saveCount = calls.filter((item) => item.command === "save_launch_settings").length;
await submit();
assert.equal(calls.filter((item) => item.command === "save_launch_settings").length, saveCount);
await render({ running: true });
assert.match(container.textContent, /Feche o Minecraft desta instância/);
assert.equal(button("Restaurar padrões").disabled, true);

await render({ instanceId: "instance-b" });
await render({ instanceId: "instance-c" });
await settle("read_launch_settings", overview({ maxMemoryMb: 12345 }));
assert.match(container.textContent, /Lendo as configurações/);
assert.doesNotMatch(container.textContent, /12345/);
await settle("read_launch_settings", "SYNTHETIC_PRIVATE_LOAD_ERROR", true);
assert.match(container.textContent, /Não foi possível ler/);
assert.doesNotMatch(container.textContent, /SYNTHETIC_PRIVATE_LOAD_ERROR/);
const beforeRecovery = calls.length;
await click("Restaurar padrões");
await settle("read_global_memory", { maxMemoryMb: 8192, totalMemoryMb: 16384, memoryRevision: "new" });
assert.equal(field("maximum").value, "8192");
assert.deepEqual(calls.slice(beforeRecovery).map(item => item.command), ["read_global_memory"], "Recovery may inspect the launcher default without replacing an unreadable file");
assert.equal(button("Salvar configurações").disabled, false);
assert.match(container.textContent, /Padrões carregados nos campos/);
await click("Cancelar alterações");
assert.match(container.textContent, /Não foi possível ler/);
await click("Tentar novamente");
assert.deepEqual(pending[0].args, { id: "instance-c" });
await settle("read_launch_settings", overview({ maxMemoryMb: 8192 }));
await click("Restaurar padrões");
await submit();
assert.equal(pending[0].args.id, "instance-c");
await render({ instanceId: "instance-d" });
await settle("save_launch_settings", overview({ maxMemoryMb: 12345 }));
assert.match(container.textContent, /Lendo as configurações/);
assert.doesNotMatch(container.textContent, /Configurações salvas|12345/);
await settle("read_launch_settings", overview({ maxMemoryMb: 2048 }));
assert.equal(field("maximum").value, "2048");
await input("maximum", "8192");
await submit();
await settle("save_launch_settings", "global_memory_changed", true);
assert.match(container.textContent, /editor estava aberto/);
await click("Recarregar valores");
await settle("read_launch_settings", overview({ memoryMode: "launcher", memoryRevision: "after-bulk", maxMemoryMb: 7168 }));
assert.equal(field("maximum").value, "7168");
await select("memory-mode", "manual");
await submit();
assert.equal(pending[0].args.settings.memoryRevision, "after-bulk");
await settle("save_launch_settings", overview(pending[0].args.settings));
await render({ instanceId: "" });
assert.match(container.textContent, /Selecione uma instância/);
assert.equal(pending.length, 0);
await act(async () => root.unmount());

// Unmount invalidates pending native responses, including StrictMode remounts.
const { StrictMode } = await import("react");
const remounted = createRoot(container);
await act(async () => remounted.render(createElement(StrictMode, null, createElement(LaunchSettingsPanel, props))));
assert.equal(pending.length, 2);
await settle("read_launch_settings", overview({ maxMemoryMb: 12345 }));
assert.match(container.textContent, /Lendo as configurações/);
await settle("read_launch_settings", overview());
assert.equal(field("maximum").value, "6144");
await click("Restaurar padrões");
await submit();
await act(async () => remounted.unmount());
await settle("save_launch_settings", overview(defaultLaunchSettings()));
assert.equal(container.textContent, "");
assert.equal(pending.length, 0);
const { GlobalMemoryPanel } = await import(await sourceModule(new URL("./GlobalMemoryPanel.tsx", import.meta.url)));
const globalRoot = createRoot(container);
await act(async () => globalRoot.render(createElement(GlobalMemoryPanel, { busy: false })));
await settle("read_global_memory", { maxMemoryMb: 4096, totalMemoryMb: 8192 });
assert.equal(button("Salvar padrão").disabled, true);
await click("6 GB");
await submit();
assert.deepEqual(pending[0].args, { maxMemoryMb: 6144, replaceOverrides: false });
await settle("save_global_memory", { maxMemoryMb: 6144, totalMemoryMb: 8192 });
assert.match(container.textContent, /Memória padrão salva/);
await input("maximum", "9000");
await submit();
assert.match(container.textContent, /entre 512 e 8192 MB/);
assert.equal(pending.length, 0);
await click("6 GB");
const checkbox = container.querySelector("input[type=checkbox]");
await act(async () => {
  checkbox.checked = true;
  checkbox.dispatchEvent(new window.Event("click", { bubbles: true }));
});
await submit();
assert.deepEqual(pending[0].args, { maxMemoryMb: 6144, replaceOverrides: true });
await settle("save_global_memory", { maxMemoryMb: 6144, totalMemoryMb: 8192 });
assert.match(container.textContent, /aplicada a todos os modpacks/);
await act(async () => globalRoot.unmount());
console.log("Memória global e por modpack: DOM, aplicação em lote, limites, modos, recuperação e respostas antigas passaram.");
