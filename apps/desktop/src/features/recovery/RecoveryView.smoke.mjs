// Synthetic DOM and native-command fixtures; never touches personal instances.
// Run: node apps/desktop/src/features/recovery/RecoveryView.smoke.mjs
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
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Synthetic recovery DOM" } });
const urls = new Map();
async function sourceModule(source) {
  if (urls.has(source.href)) return urls.get(source.href);
  let output = ts.transpileModule(await readFile(source, "utf8"), { fileName: fileURLToPath(source), compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX } }).outputText.replace(/^import ["'][^"']+\.css["'];?$/gm, "");
  for (const { fileName: specifier } of ts.preProcessFile(output).importedFiles) {
    const dependency = specifier.startsWith(".") ? ["", ".ts", ".tsx"].map((ext) => new URL(specifier + ext, source)).find((url) => existsSync(fileURLToPath(url))) : undefined;
    const resolved = dependency ? await sourceModule(dependency) : pathToFileURL(require.resolve(specifier)).href;
    output = output.replaceAll(`from "${specifier}"`, `from ${JSON.stringify(resolved)}`);
  }
  const url = `data:text/javascript;base64,${Buffer.from(output).toString("base64")}`;
  urls.set(source.href, url); return url;
}
const point = (overrides = {}) => ({ id: "point-a", createdAt: 1700000000000, description: "Antes dos mods sintéticos", scope: ["mods/**", "config/**", "options.txt"], logicalBytes: 8192, fileCount: 3, kind: "manual", excluded: ["Mundos", "Módulos Aurora administrados"], profile: { "aurora-modpack.json": { name: "Perfil sintético" } }, files: [{ path: "mods/example.jar.disabled", size: 4096, sha256: "a".repeat(64) }], ...overrides });
let overview = { points: [], unavailablePoints: [], undoPoint: null, undoUnavailable: false, physicalBytes: 4096, scope: ["mods/**", "config/**"], excluded: ["Mundos", "Screenshots"] };
const preview = (overrides = {}) => ({ id: "preview-a", point: point(), changes: [{ path: "mods/added.jar", action: "remove" }, { path: "config/changed.toml", action: "replace" }, { path: "mods/deleted.jar.disabled", action: "add" }], profileChanges: [{ file: "aurora-modpack.json", field: "name", before: "Perfil atual", after: "Perfil salvo" }], excluded: ["Mundos", "Credenciais, sessões e tokens"], currentFingerprint: "fixture", ...overrides });
const calls = [];
const pending = [];
const locks = [];
let parentRefreshes = 0;
let failList = false;
let deferList = false;
const callbacks = new Map();
const listeners = new Map();
let callbackId = 0;
window.__TAURI_INTERNALS__ = {
  transformCallback(callback) { callbacks.set(++callbackId, callback); return callbackId; },
  invoke(command, args) {
    if (command === "plugin:event|listen") { listeners.set(args.handler, { event: args.event, callback: callbacks.get(args.handler) }); return Promise.resolve(args.handler); }
    if (command === "plugin:event|unlisten") { listeners.delete(args.eventId); return Promise.resolve(); }
    calls.push({ command, args });
    if (command === "recovery_list" && !deferList) return failList ? Promise.reject("SYNTHETIC_REFRESH_ERROR_PRIVATE") : Promise.resolve(overview);
    return new Promise((resolve, reject) => pending.push({ command, args, resolve, reject }));
  },
};
window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
const { act, createElement } = await import("react");
const { createRoot } = await import("react-dom/client");
const { RecoveryView } = await import(await sourceModule(new URL("./RecoveryView.tsx", import.meta.url)));
const { recoveryError } = await import(await sourceModule(new URL("./types.ts", import.meta.url)));
const nativeSource = await readFile(new URL("../../../src-tauri/src/engine/recovery.rs", import.meta.url), "utf8");
const publicNativeErrors = [...nativeSource.matchAll(/#\[error\(\s*"([^"\n]+)"\s*\)\]/g)].map((match) => match[1]);
assert.ok(publicNativeErrors.length >= 13, "Verify actual native public errors, not guessed categories");
for (const message of publicNativeErrors) assert.equal(recoveryError(message), message, `Missing reviewed native message: ${message}`);
assert.doesNotMatch(recoveryError("SYNTHETIC_PRIVATE_EXCEPTION C:\\Secrets\\account.json password=private"), /SYNTHETIC|Secrets|password/);
const container = document.getElementById("root");
const root = createRoot(container);
const props = { instanceId: "instance-a", busy: false, running: false, onOperationChange: (value) => locks.push(value), onChanged: async () => { parentRefreshes += 1; } };
const render = async (overrides = {}) => { await act(async () => root.render(createElement(RecoveryView, { ...props, ...overrides }))); };
const button = (label) => { const found = [...container.querySelectorAll("button")].find((item) => item.textContent === label); assert.ok(found, `Missing ${label}`); return found; };
const click = async (label) => { assert.equal(button(label).disabled, false, `Disabled ${label}`); await act(async () => button(label).dispatchEvent(new window.Event("click", { bubbles: true }))); };
const resolve = async (command, value) => { const index = pending.findIndex((call) => call.command === command); assert.ok(index >= 0, `Missing pending ${command}`); const request = pending.splice(index, 1)[0]; await act(async () => request.resolve(value)); return request; };
const reject = async (command, error) => { const index = pending.findIndex((call) => call.command === command); assert.ok(index >= 0, `Missing pending ${command}`); const request = pending.splice(index, 1)[0]; await act(async () => request.reject(error)); return request; };
const acknowledge = async () => {
  const checkbox = container.querySelector('.recovery-acknowledge input');
  assert.ok(checkbox);
  await act(async () => { checkbox.checked = true; checkbox.dispatchEvent(new window.Event("click", { bubbles: true })); });
};
const count = (command) => calls.filter((call) => call.command === command).length;
const emitRecovered = async (instanceId) => { await act(async () => { for (const listener of listeners.values()) if (listener.event === "aurora-recovery-changed") listener.callback({ payload: { instanceId, recoveredInterrupted: true } }); }); };

await render();
assert.match(container.textContent, /Nenhum ponto manual/);
assert.match(container.textContent, /Este ponto não inclui mundos/);
assert.equal(count("recovery_create"), 0);
assert.equal(count("recovery_restore"), 0);
assert.equal(parentRefreshes, 1, "Initial list also refreshes projection after a native interrupted recovery");
await emitRecovered("other-instance");
assert.doesNotMatch(container.textContent, /interrompida foi recuperada/);
await emitRecovered("instance-a");
assert.match(container.textContent, /interrompida foi recuperada com segurança/);
await click("Criar ponto de restauração");
assert.equal(count("recovery_create"), 0, "Opening capture form cannot capture silently");
await click("Cancelar");
await click("Criar ponto de restauração");
await act(async () => container.querySelector("form").dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true })));
assert.deepEqual(pending[0].args, { id: "instance-a", description: null });
assert.equal(locks.at(-1), true);
assert.match(container.textContent, /Copiando e verificando/);
assert.equal(button("Criar e verificar ponto").disabled, true);
overview = { ...overview, points: [point()] };
await resolve("recovery_create", point());
assert.match(container.textContent, /Ponto de restauração criado e verificado/);
assert.match(container.textContent, /Tamanho lógico.*8 KiB/s);
assert.match(container.textContent, /Conteúdo físico compartilhado.*4 KiB/s);
assert.match(container.textContent, /mods\/example.jar.disabled/);
assert.equal(locks.at(-1), false);

await click("Detalhes");
await resolve("recovery_details", point({ files: Array.from({ length: 251 }, (_, index) => ({ path: `mods/mod-${index}.jar`, size: 10, sha256: "b".repeat(64) })) }));
assert.match(container.textContent, /SHA-256/);
assert.equal(container.querySelectorAll(".recovery-details tbody tr").length, 101, "Only 100 files plus profile row rendered");
assert.match(container.textContent, /1–100 de 251/);
await click("Próximos arquivos");
assert.match(container.textContent, /101–200 de 251/);
await click("Próximos arquivos");
assert.match(container.textContent, /201–251 de 251/);
assert.equal(button("Próximos arquivos").disabled, true);
await click("Restaurar");
assert.equal(container.querySelector('[role="dialog"]'), null, "Only native-generated previews open confirmation");
await resolve("recovery_preview", preview());
assert.equal(count("recovery_restore"), 0);
const review = container.querySelector('[role="dialog"]');
assert.match(review.textContent, /Adicionar.*Substituir.*Remover/s);
assert.match(review.textContent, /mods\/added.jar.*config\/changed.toml.*mods\/deleted.jar.disabled/s);
assert.match(review.textContent, /Perfil atual.*Perfil salvo/s);
assert.match(review.textContent, /Mundos não serão restaurados/);
assert.match(review.textContent, /Companion não é reinstalado/);
assert.equal(button("Confirmar restauração").disabled, true, "Explicit acknowledgement required");
await click("Cancelar");
assert.equal(count("recovery_restore"), 0);
await click("Restaurar");
await resolve("recovery_preview", preview());
await acknowledge();
assert.equal(button("Confirmar restauração").disabled, false);
const confirmation = button("Confirmar restauração");
await act(async () => { confirmation.dispatchEvent(new window.Event("click", { bubbles: true })); confirmation.dispatchEvent(new window.Event("click", { bubbles: true })); });
assert.equal(count("recovery_restore"), 1, "Double confirmation sends only one native operation");
assert.deepEqual(pending[0].args, { id: "instance-a", previewId: "preview-a" });
assert.equal(button("Cancelar").disabled, true, "Unsafe in-flight cancellation is unavailable");
await reject("recovery_restore", "A prévia ficou desatualizada. Gere uma nova prévia antes de confirmar.");
assert.match(container.textContent, /Gere uma nova prévia/);
assert.doesNotMatch(container.textContent, /SYNTHETIC_PRIVATE_TOKEN/);
assert.equal(container.querySelector('[role="dialog"]'), null, "Stale preview is discarded");

await click("Restaurar");
await resolve("recovery_preview", preview({ id: "preview-b" }));
await acknowledge();
await click("Confirmar restauração");
overview = { ...overview, undoPoint: point({ id: "undo-a", kind: "undo" }) };
await resolve("recovery_restore", { restoredPointId: "point-a", undoAvailable: true, recoveredInterrupted: false });
assert.match(container.textContent, /Estado protegido restaurado/);
assert.equal(button("Desfazer restauração").disabled, false);
await click("Desfazer restauração");
assert.equal(count("recovery_restore"), 2, "Undo cannot execute before another preview and confirmation");
await resolve("recovery_preview_undo", preview({ id: "undo-preview", point: overview.undoPoint }));
await acknowledge();
await click("Confirmar desfazer restauração");
await resolve("recovery_restore", { restoredPointId: "undo-a", undoAvailable: true, recoveredInterrupted: false });
assert.match(container.textContent, /Restauração desfeita/);

// A refresh error after a committed mutation must preserve its success.
await click("Restaurar");
await resolve("recovery_preview", preview({ id: "preview-refresh-fails" }));
await acknowledge();
await click("Confirmar restauração");
failList = true;
await resolve("recovery_restore", { restoredPointId: "point-a", undoAvailable: true, recoveredInterrupted: false });
assert.match(container.textContent, /Estado protegido restaurado/);
assert.match(container.textContent, /operação foi concluída, mas a lista/);
assert.doesNotMatch(container.textContent, /Operação não concluída|SYNTHETIC_REFRESH_ERROR_PRIVATE/);
failList = false;
await click("Atualizar lista");

// A damaged point must not hide healthy points or pretend its size is zero.
overview = { ...overview, unavailablePoints: [{ id: "corrupt-point", createdAt: 1700000000000, reason: "Os dados não puderam ser validados." }], undoPoint: null, undoUnavailable: true };
await click("Atualizar lista");
assert.match(container.textContent, /Pontos indisponíveis \(1\)/);
assert.match(container.textContent, /Desfazer está indisponível/);
assert.equal(button("Restaurar").disabled, false);
await click("Excluir ponto indisponível");
const corruptDialog = container.querySelector('[role="dialog"]');
assert.match(corruptDialog.textContent, /tamanho e o inventário não puderam ser verificados/);
assert.doesNotMatch(corruptDialog.textContent, /0 B|Tamanho lógico/);
await click("Confirmar exclusão");
overview = { ...overview, unavailablePoints: [], undoPoint: point({ id: "undo-a", kind: "undo" }), undoUnavailable: false };
const corruptDelete = await resolve("recovery_delete");
assert.equal(corruptDelete.args.pointId, "corrupt-point");

await click("Excluir");
assert.equal(count("recovery_delete"), 1);
await click("Cancelar");
await click("Excluir");
await click("Confirmar exclusão");
overview = { ...overview, points: [] };
await resolve("recovery_delete");
assert.match(container.textContent, /Ponto excluído/);
assert.match(container.textContent, /Nenhum ponto manual/);

await render({ running: true });
assert.equal(button("Criar ponto de restauração").disabled, true);
assert.equal(button("Desfazer restauração").disabled, true);
assert.match(container.textContent, /Encerre o Minecraft/);
await render({ busy: true });
assert.equal(button("Criar ponto de restauração").disabled, true);
await render();
await click("Desfazer restauração");
await reject("recovery_preview_undo", "O ponto contém dados ausentes, corrompidos ou um manifesto inválido. A restauração foi bloqueada.");
assert.match(container.textContent, /dados ausentes, corrompidos/);
await click("Desfazer restauração");
await reject("recovery_preview_undo", "Ponto corrompido. C:\\SyntheticPrivate\\Account");
assert.match(container.textContent, /Não foi possível concluir a operação com segurança/);
assert.doesNotMatch(container.textContent, /SyntheticPrivate/);

// Responses from an earlier instance or unmounted component never leak across.
deferList = true;
await click("Atualizar lista");
await render({ instanceId: "instance-b" });
const oldList = pending.find((item) => item.command === "recovery_list" && item.args.id === "instance-a");
pending.splice(pending.indexOf(oldList), 1);
await act(async () => oldList.resolve({ ...overview, points: [point({ description: "STALE_INSTANCE_PRIVATE_POINT" })] }));
assert.doesNotMatch(container.textContent, /STALE_INSTANCE_PRIVATE_POINT/);
await resolve("recovery_list", overview);
await click("Atualizar lista");
await act(async () => root.unmount());
await resolve("recovery_list", { ...overview, points: [point()] });
assert.equal(container.textContent, "");
assert.equal(locks.at(-1), false);

console.log(JSON.stringify({ result: "PASS", fixtures: "synthetic DOM/native only", checks: ["empty-worlds-scope", "interrupted-recovery-event-own-instance", "explicit-capture", "logical-shared-physical-sizes", "details-inventory-pagination", "preview-add-replace-remove-profile", "explicit-confirm-acknowledgement", "cancel-before-mutation", "double-confirm-single-native-call", "stale-preview-invalidated", "restore-and-undo-confirmed", "postcommit-refresh-error-preserves-success", "corrupt-point-delete-healthy-points-accessible", "delete-confirmed", "busy-running-disabled", "errors-redacted", "instance-switch-stale-response", "unmount-response"] }, null, 2));
