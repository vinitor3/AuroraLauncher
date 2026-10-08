// Synthetic DOM and native projections only. No transport, Java, account,
// clipboard outside this fixture, or personal configuration is accessed.
import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";

const require = createRequire(import.meta.url);
const { parseHTML } = require("linkedom");
const { window, document } = parseHTML("<html><body><div id='root'></div></body></html>");
// Linkedom omits the browser textarea.defaultValue reflection used by React 19
// on initial mount. Supply that DOM behavior only inside this synthetic fixture.
Object.defineProperty(window.HTMLTextAreaElement.prototype, "defaultValue", {
  configurable: true, get() { return this.textContent; }, set(value) { this.textContent = value; },
});
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
let copied = [];
let clipboardFails = false;
Object.defineProperty(globalThis, "navigator", { configurable: true, value: {
  userAgent: "Connect synthetic DOM", clipboard: { async writeText(value) { if (clipboardFails) throw new Error("fixture"); copied.push(value); } },
} });
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
  modules.set(source.href, url);
  return url;
}
const hostId = "a".repeat(64), guestId = "b".repeat(64), digest = "c".repeat(64);
const operationId = "synthetic-operation";
const statusContract = { statusProtocol: 767, statusHandshakeHost: "localhost", statusHandshakePort: 43001 };
const caps = { available: true, experimental: true, maxLifetimeSeconds: 600, modes: ["loopback", "externalIpv6"], services: ["game"], voiceEnabled: false, relayEnabled: false, ...statusContract };
const preparation = { operationId, generation: 1, publicId: hostId, expiresAtMs: 1800000060000 };
const review = { operationId, generation: 1, digest, invitationDigest: "d".repeat(64), role: "host", mode: "loopback", hostPublicId: hostId,
  guestPublicId: guestId, directAddress: "127.0.0.1:42000", gameTarget: "127.0.0.1:43001", localTcpBind: null,
  services: [{ serviceId: "game", protocol: "tcp", revision: 1, requiredCapability: "reliableStream" }], expiresAtMs: 1800000060000, invitationExpiresAtMs: 1800000600000, lifetimeSeconds: 600, ...statusContract };
const idle = () => ({ available: true, operationId: null, generation: 0, role: null, phase: "idle", transportState: "notStarted", statusState: "notChecked",
  tcpAddress: null, expiresAtMs: null, statusSummary: null, errorCode: null, worldState: "notVerified", simpleVoiceChatState: "disabled",
  plasmoVoiceState: "disabled", relayEnabled: false, pathState: "notVerified", preparation: null, review: null, cleanupState: "notStarted" });
let snapshot, capabilities, calls, handlers, root;
const nativeInvoke = async (command, args) => {
  calls.push({ command, args });
  if (handlers.has(command)) return handlers.get(command)(args);
  switch (command) {
    case "connect_capabilities": return structuredClone(capabilities);
    case "connect_snapshot": return structuredClone(snapshot);
    case "connect_prepare_host":
      snapshot = { ...idle(), operationId, generation: 1, role: "host", phase: "hostPrepared", preparation, expiresAtMs: preparation.expiresAtMs };
      return preparation;
    case "connect_prepare_guest":
      snapshot = { ...idle(), operationId, generation: 1, role: "guest", phase: "guestPrepared", preparation: { ...preparation, publicId: guestId }, expiresAtMs: preparation.expiresAtMs };
      return snapshot.preparation;
    case "connect_review_host": snapshot = { ...snapshot, phase: "reviewed", review }; return review;
    case "connect_review_guest":
      snapshot = { ...snapshot, phase: "reviewed", review: { ...review, role: "guest", gameTarget: null, localTcpBind: "127.0.0.1:0" } };
      return snapshot.review;
    case "connect_confirm":
      snapshot = { ...snapshot, phase: snapshot.role === "host" ? "hostListening" : "guestConnected", transportState: snapshot.role === "host" ? "listening" : "authenticated",
        preparation: null, review: null, tcpAddress: snapshot.role === "guest" ? "127.0.0.1:42123" : null };
      return structuredClone(snapshot);
    case "connect_export_invitation": return { invitation: '{"fixture":"synthetic invitation"}', hostPublicId: hostId, expiresAtMs: preparation.expiresAtMs };
    case "connect_probe_status":
      snapshot = { ...snapshot, statusState: "responded", statusSummary: { requestedProtocol: 767, reportedProtocol: 767, protocolMatchesRequested: true,
        reportedPlayersOnline: 1, reportedPlayersMax: 2, connectMicros: 10, statusMicros: 20, pingMicros: 30, totalMicros: 60 } };
      return structuredClone(snapshot);
    case "connect_stop":
      snapshot = { ...snapshot, phase: "closed", transportState: "closed", preparation: null, review: null, tcpAddress: null, cleanupState: "complete" };
      return structuredClone(snapshot);
    default: throw new Error(`Unexpected fixture command ${command}`);
  }
};
window.__TAURI_INTERNALS__ = { invoke: nativeInvoke };
window.isTauri = true;
const { act, createElement } = await import("react");
const { createRoot } = await import("react-dom/client");
const { ConnectView } = await import(await sourceModule(new URL("./ConnectView.tsx", import.meta.url)));
const types = await import(await sourceModule(new URL("./types.ts", import.meta.url)));
const container = document.getElementById("root");
const button = label => { const found = [...container.querySelectorAll("button")].find(element => element.textContent === label); assert.ok(found, `Button ${label}`); return found; };
const click = async label => { assert.equal(button(label).disabled, false); await act(async () => button(label).dispatchEvent(new window.Event("click", { bubbles: true }))); };
const submit = async () => { const form = container.querySelector("form"); assert.ok(form); await act(async () => form.dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true }))); };
const count = name => calls.filter(call => call.command === name).length;
const render = async () => { await act(async () => root.render(createElement(ConnectView))); };
const checkConsent = async () => { const checkbox = container.querySelector('.connect-consent input'); assert.ok(checkbox); await act(async () => { checkbox.checked = !checkbox.checked; checkbox.dispatchEvent(new window.Event("click", { bubbles: true })); }); };
beforeEach(() => { snapshot = idle(); capabilities = { ...caps }; calls = []; handlers = new Map(); copied = []; clipboardFails = false; root = createRoot(container); });
afterEach(async () => { await act(async () => root.unmount()); });

test("disabled backend exposes no host, guest, network or clipboard operation", async () => {
  capabilities.available = false;
  await render();
  assert.match(container.textContent, /desligado nesta versão/);
  assert.deepEqual(calls.map(call => call.command), ["connect_capabilities"]);
  assert.equal(container.querySelector("form"), null);
  assert.deepEqual(copied, []);
});

test("host requires separate preparation, review and explicit consent before native confirmation", async () => {
  await render();
  assert.deepEqual(calls.map(call => call.command), ["connect_capabilities", "connect_snapshot"]);
  await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_prepare_host").args, { request: { mode: "loopback", bindAddress: "127.0.0.1:42000", gameTarget: "127.0.0.1:43001", lifetimeSeconds: 600 } });
  assert.equal(count("connect_confirm"), 0);
  await click("Copiar meu código");
  assert.deepEqual(copied, [hostId]);
  await submit();
  assert.equal(button("Autorizar conexão").disabled, true);
  assert.equal(count("connect_confirm"), 0);
  await checkConsent();
  await click("Autorizar conexão");
  assert.deepEqual(calls.find(call => call.command === "connect_confirm").args, { operationId, digest });
  assert.match(container.textContent, /Aguardando convidado/);
  assert.equal(count("connect_export_invitation"), 0);
  assert.equal(count("connect_probe_status"), 0);
  assert.equal([...container.querySelectorAll("button")].some(element => element.textContent === "Consultar servidor"), false, "Host has no unauthorized STATUS action");
  await click("Copiar convite");
  assert.equal(count("connect_export_invitation"), 1);
  assert.equal(copied.length, 2);
  await click("Encerrar Connect");
  assert.match(container.textContent, /Recursos liberados/);
  assert.doesNotMatch(container.textContent, /synthetic invitation/);
});

test("guest command uses encodedInvitation and only backend listener may be copied", async () => {
  snapshot = { ...idle(), operationId, generation: 1, role: "guest", phase: "guestPrepared", preparation: { ...preparation, publicId: guestId } };
  await render();
  await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_review_guest").args, { operationId, encodedInvitation: "" });
  await checkConsent();
  await click("Autorizar conexão");
  assert.equal(container.querySelector('#connect-local-address').value, "127.0.0.1:42123");
  assert.equal(count("connect_probe_status"), 0);
  await click("Copiar endereço do jogo");
  assert.deepEqual(copied, ["127.0.0.1:42123"]);
  await click("Consultar servidor");
  assert.match(container.textContent, /Servidor respondeu/);
  assert.match(container.textContent, /Não verificada/);
  assert.match(container.textContent, /não confirma quem entrou/);
  assert.equal(container.querySelectorAll(".connect-status dd")[2].textContent, "Não confirmado · relay desligado");
});

test("host removes invitation output and export action after observed admission", async () => {
  snapshot = { ...idle(), operationId, generation: 1, role: "host", phase: "hostListening", transportState: "listening" };
  await render();
  await click("Copiar convite");
  assert.equal(container.querySelector("textarea").value, '{"fixture":"synthetic invitation"}');
  snapshot = { ...snapshot, generation: 2, transportState: "admitted" };
  await click("Atualizar estado");
  assert.match(container.textContent, /Convite utilizado · conexão ativa/);
  assert.doesNotMatch(container.textContent, /Aguardando convidado/);
  assert.equal(container.querySelector("textarea"), null);
  assert.equal([...container.querySelectorAll("button")].some(element => element.textContent === "Copiar convite"), false);
  assert.equal(count("connect_export_invitation"), 1);
});

test("remount retrieves native preparation and review without regenerating identity or approving", async () => {
  snapshot = { ...idle(), operationId, generation: 1, role: "host", phase: "reviewed", preparation, review };
  await render();
  assert.equal(container.querySelector('#connect-public-code').value, hostId);
  assert.equal(button("Autorizar conexão").disabled, true);
  assert.equal(count("connect_prepare_host"), 0);
  assert.equal(count("connect_review_host"), 0);
  assert.equal(count("connect_confirm"), 0);
  const entries = [...container.querySelectorAll(".connect-review dt")];
  assert.equal(entries.find(element => element.textContent === "Autorize até").nextElementSibling.textContent, types.connectDate(review.expiresAtMs));
  assert.equal(entries.find(element => element.textContent === "Convite e sessão expiram").nextElementSibling.textContent, types.connectDate(review.invitationExpiresAtMs));
  await checkConsent();
  await act(async () => root.unmount()); root = createRoot(container);
  await render();
  assert.equal(button("Autorizar conexão").disabled, true);
  assert.equal(count("connect_stop"), 0, "View navigation never stops an owned backend session");
});

test("stop supersedes a pending confirmation and discards its late connected projection", async () => {
  snapshot = { ...idle(), operationId, generation: 1, role: "host", phase: "reviewed", preparation, review };
  let completeConfirmation;
  handlers.set("connect_confirm", () => new Promise(resolve => { completeConfirmation = resolve; }));
  await render(); await checkConsent(); await click("Autorizar conexão");
  assert.equal(typeof completeConfirmation, "function");
  await click("Encerrar Connect");
  assert.match(container.textContent, /Conexão encerrada/);
  await act(async () => completeConfirmation({ ...snapshot, phase: "hostListening", transportState: "listening", cleanupState: "notStarted" }));
  assert.match(container.textContent, /Conexão encerrada/);
  assert.doesNotMatch(container.textContent, /Aguardando convidado/);
});

test("revised native digest clears consent; raw native errors and failed clipboard remain safe", async () => {
  snapshot = { ...idle(), operationId, generation: 1, role: "host", phase: "reviewed", preparation, review };
  await render(); await checkConsent();
  snapshot = { ...snapshot, review: { ...review, digest: "e".repeat(64) } };
  await click("Atualizar estado");
  assert.equal(button("Autorizar conexão").disabled, true);
  clipboardFails = true;
  await click("Copiar meu código");
  assert.match(container.textContent, /copie manualmente/);
  handlers.set("connect_stop", () => Promise.reject("PRIVATE_FIXTURE C:\\token\\Account secret"));
  await click("Encerrar Connect");
  assert.doesNotMatch(container.textContent, /PRIVATE_FIXTURE|Account|secret/);
  assert.match(container.textContent, /Não foi possível concluir/);
});

test("invalid native projection cannot create available transport or expose unsupported service", async () => {
  snapshot = { ...idle(), transportState: "authenticated-unverified" };
  await render();
  assert.match(container.textContent, /Estado ainda não disponível/);
  assert.match(container.textContent, /não pôde ser atualizado/);
  assert.throws(() => types.reviewFrom({ ...review, services: [{ ...review.services[0], serviceId: "admin" }] }));
  assert.throws(() => types.reviewFrom({ ...review, lifetimeSeconds: 601 }));
  assert.throws(() => types.capabilitiesFrom({ ...caps, relayEnabled: true }));
  assert.throws(() => types.reviewFrom({ ...review, expiresAtMs: review.invitationExpiresAtMs + 1 }));
  assert.throws(() => types.reviewFrom({ ...review, statusProtocol: 47 }));
  assert.equal(types.connectError("connect_invitation_rejected"), "O convite não pôde ser validado para este anfitrião e convidado.");
});

test("a prepared operation retains its own stop handle when the subsequent state read fails", async () => {
  snapshot = { ...idle(), operationId: "old-operation", generation: 0, phase: "closed", transportState: "closed", cleanupState: "complete" };
  await render();
  handlers.set("connect_snapshot", () => Promise.reject("PRIVATE_READ_FAILURE"));
  await submit();
  assert.match(container.textContent, /Estado ainda não disponível/);
  assert.equal(button("Encerrar Connect").disabled, false);
  await click("Encerrar Connect");
  assert.deepEqual(calls.find(call => call.command === "connect_stop").args, { operationId });
  assert.doesNotMatch(container.textContent, /PRIVATE_READ_FAILURE/);
});

test("STATUS preserves the native signed protocol mismatch without claiming compatibility", async () => {
  snapshot = { ...idle(), operationId, generation: 1, role: "guest", phase: "guestConnected", transportState: "authenticated",
    tcpAddress: "127.0.0.1:42123", statusState: "responded", statusSummary: { requestedProtocol: 767, reportedProtocol: -1,
      protocolMatchesRequested: false, reportedPlayersOnline: null, reportedPlayersMax: null, connectMicros: 1, statusMicros: 2, pingMicros: 3, totalMicros: 6 } };
  await render();
  assert.match(container.textContent, /protocolo -1, diferente do solicitado/);
  assert.match(container.textContent, /Minecraft 1\.21\.1 \(protocolo 767\)/);
  assert.equal(count("connect_probe_status"), 0);
  assert.equal(button("Encerrar Connect").disabled, false);
  assert.throws(() => types.snapshotFrom({ ...snapshot, statusSummary: { ...snapshot.statusSummary, requestedProtocol: 47 } }));
});
