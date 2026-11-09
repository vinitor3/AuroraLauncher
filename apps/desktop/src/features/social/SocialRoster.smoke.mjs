// Synthetic DOM: verifies presentation/cancellation only, never real transport.
import assert from "node:assert/strict";
import { beforeEach, afterEach, test } from "node:test";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";

const require = createRequire(import.meta.url);
const { window, document } = require("linkedom").parseHTML("<html><body><div id='root'></div></body></html>");
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Synthetic roster" } });
const dataUrl = source => `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const { act, createElement } = await import("react");
const { createRoot } = await import("react-dom/client");
const path = new URL("./SocialConnectionPanel.tsx", import.meta.url);
let code = ts.transpileModule(await readFile(path, "utf8"), { fileName: fileURLToPath(path), compilerOptions: {
  target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX,
} }).outputText;
for (const { fileName: specifier } of ts.preProcessFile(code).importedFiles) {
  const resolved = specifier === "../connect/DiscoveryPicker" ? dataUrl("export function DiscoveryPicker(){return null;}")
    : specifier === "./connect-types" ? dataUrl("export const socialRuntimeLabels={};export const socialStageText={connected:'Conectado',waitingGuest:'Aguardando'};export const socialGameTarget=()=>false;export const socialConnectError=()=>'';")
      : pathToFileURL(require.resolve(specifier)).href;
  code = code.replaceAll(`from "${specifier}"`, `from ${JSON.stringify(resolved)}`);
}
const { SocialConnectionPanel } = await import(dataUrl(code));
const container = document.getElementById("root");
let root, connection, calls;
const deferred = () => { let resolve; const promise = new Promise(yes => { resolve = yes; }); return { promise, resolve }; };
const render = async service => act(async () => root.render(createElement(SocialConnectionPanel, { connection, service, now: 1000 })));
beforeEach(() => {
  calls = []; root = createRoot(container);
  connection = { state: { available: true, pending: false, maxSessionSeconds: 14400, snapshot: {
    operationId: "original-attempt", phase: "hostListening", role: "host", cleanupState: "notStarted",
    social: { role: "host", stage: "connected", expiresAtMs: 10000, maxGuests: 8,
      participants: [{ grantId: "a".repeat(64), accountId: "private-uid-001", state: "connected" }] },
  } }, controller: { revokeParticipant: async id => { calls.push(id); } } };
});
afterEach(async () => act(async () => root.unmount()));

test("roster uses Aurora display name only, keeps removal on the original grant and never claims world admission", async () => {
  const pending = deferred(), reads = [];
  const service = { profile: uid => { reads.push(uid); return pending.promise; } };
  await render(service);
  assert.match(container.textContent, /Participante/);
  assert.doesNotMatch(container.textContent, /private-uid/);
  await act(async () => pending.resolve({ uid: "private-uid-001", nickname: "Aurora_User", avatarUrl: null }));
  assert.match(container.textContent, /Aurora_User · conexão autenticada/);
  assert.match(container.textContent, /entrada no mundo ainda não foi verificada/);
  assert.deepEqual(reads, ["private-uid-001"]);
  await render(service);
  assert.equal(reads.length, 1, "Unchanged polling projections do not reread profiles");
  const button = [...container.querySelectorAll("button")].find(value => value.textContent === "Remover desta sessão");
  await act(async () => button.dispatchEvent(new window.Event("click", { bubbles: true })));
  assert.deepEqual(calls, ["a".repeat(64)]);
});

test("late roster display from a previous account/service or attempt cannot replace the current name", async () => {
  const old = deferred(), current = deferred();
  await render({ profile: () => old.promise });
  const service = { profile: () => current.promise };
  await render(service);
  await act(async () => old.resolve({ uid: "private-uid-001", nickname: "Stale_User" }));
  assert.doesNotMatch(container.textContent, /Stale_User/);
  await act(async () => current.resolve({ uid: "private-uid-001", nickname: "Current_User" }));
  assert.match(container.textContent, /Current_User/);
  const next = deferred(); service.profile = () => next.promise;
  connection = { ...connection, state: { ...connection.state, snapshot: { ...connection.state.snapshot, operationId: "next-attempt" } } };
  await render(service);
  assert.doesNotMatch(container.textContent, /Current_User/);
  assert.match(container.textContent, /Participante/);
  await act(async () => next.resolve({ uid: "another-account", nickname: "Wrong_User" }));
  assert.doesNotMatch(container.textContent, /Wrong_User|private-uid/);
});

test("unavailable or malformed public names stay anonymous without changing transport controls", async () => {
  await render({ profile: async () => ({ uid: "private-uid-001", nickname: "<script>" }) });
  assert.match(container.textContent, /Participante/);
  assert.doesNotMatch(container.textContent, /private-uid|script/);
  assert.equal([...container.querySelectorAll("button")].find(value => value.textContent === "Remover desta sessão").disabled, false);
});
