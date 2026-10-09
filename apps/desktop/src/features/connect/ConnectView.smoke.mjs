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
// Advertise the native input event supported by the fixture so React uses its
// modern event path. Set the prototype value below to model user editing rather
// than a programmatic setter already observed by React's value tracker.
document.oninput = null;
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
const editInput = async (input, value) => { await act(async () => {
  Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set.call(input, value);
  input.dispatchEvent(new window.Event("input", { bubbles: true }));
}); };
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

const discoveryCandidate = { candidateId: "candidate-fixture", target: "127.0.0.1:45432", bindKind: "wildcard" };
const discoveryInventory = { inventoryId: "inventory-fixture", instanceId: "managed-fixture", runId: "run-fixture", expiresAtMs: 1800000030000, candidates: [discoveryCandidate] };
const discoveryReview = { reviewId: "review-fixture", digest, inventoryId: discoveryInventory.inventoryId, candidate: discoveryCandidate,
  instanceId: discoveryInventory.instanceId, runId: discoveryInventory.runId, expiresAtMs: discoveryInventory.expiresAtMs,
  statusProtocol: 767, handshakeHost: "localhost", handshakePort: 45432 };
const discoverySelected = { selectionId: "selection-fixture", target: discoveryCandidate.target, instanceId: discoveryInventory.instanceId,
  runId: discoveryInventory.runId, expiresAtMs: discoveryInventory.expiresAtMs, verifiedBeforeAfter: true, worldState: "notVerified",
  summary: { requestedProtocol: 767, reportedProtocol: 767, protocolMatchesRequested: true, reportedPlayersOnline: 1,
    reportedPlayersMax: 8, connectMicros: 1, statusMicros: 2, pingMicros: 3, totalMicros: 6 } };
function discoveryFixtures() {
  capabilities.ownedInstanceDiscovery = true;
  handlers.set("list_running_instances", () => [{ instanceId: "managed-fixture", processId: 123 }]);
  handlers.set("connect_discover_instance", () => discoveryInventory);
  handlers.set("connect_review_discovered", () => discoveryReview);
  handlers.set("connect_confirm_discovered_status", () => discoverySelected);
  handlers.set("connect_prepare_discovered_host", () => {
    snapshot = { ...idle(), operationId, generation: 1, role: "host", phase: "hostPrepared", preparation, expiresAtMs: preparation.expiresAtMs };
    return preparation;
  });
}
async function chooseManaged() {
  const select = [...container.querySelectorAll("select")].find(el => el.querySelector('option[value="ownedInstance"]'));
  assert.ok(select);
  await act(async () => {
    for (const option of select.querySelectorAll("option")) option.selected = option.value === "ownedInstance";
    assert.equal(select.value, "ownedInstance");
    select.dispatchEvent(new window.Event("change", { bubbles: true }));
  });
}
async function discoveryToReview() {
  await chooseManaged();
  await click("Buscar jogos abertos pelo Aurora");
  await click("Ver portas de managed-fixture");
  await click("Revisar 127.0.0.1:45432");
}
test("owned discovery sends no STATUS or host preparation before separate explicit actions", async () => {
  discoveryFixtures(); await render(); await chooseManaged();
  assert.equal(count("list_running_instances"), 0);
  assert.equal(button("Preparar anfitrião").disabled, true);
  await click("Buscar jogos abertos pelo Aurora");
  await click("Ver portas de managed-fixture");
  assert.deepEqual(calls.find(c => c.command === "connect_discover_instance").args, { instanceId: "managed-fixture" });
  assert.equal(count("connect_confirm_discovered_status"), 0);
  await click("Revisar 127.0.0.1:45432");
  assert.equal(count("connect_confirm_discovered_status"), 0);
  await click("Autorizar consulta e selecionar porta");
  assert.deepEqual(calls.find(c => c.command === "connect_confirm_discovered_status").args, { reviewId: "review-fixture", digest });
  assert.equal(count("connect_prepare_discovered_host"), 0);
  await submit();
  assert.deepEqual(calls.find(c => c.command === "connect_prepare_discovered_host").args, {
    request: { mode: "loopback", bindAddress: "127.0.0.1:42000", gameTarget: "127.0.0.1:45432", lifetimeSeconds: 600 }, selectionId: "selection-fixture",
  });
  assert.equal(count("connect_prepare_host"), 0);
  assert.equal(count("connect_confirm"), 0);
});
test("discovery ignores a pending response after switching to guest and requires a new selection", async () => {
  discoveryFixtures(); await render(); await discoveryToReview();
  let complete;
  handlers.set("connect_confirm_discovered_status", () => new Promise(resolve => { complete = resolve; }));
  await click("Autorizar consulta e selecionar porta");
  await click("Convidado");
  await act(async () => complete(discoverySelected));
  await click("Anfitrião");
  assert.equal(button("Preparar anfitrião").disabled, true);
  assert.doesNotMatch(container.textContent, /Servidor respondeu em/);
  assert.equal(count("connect_prepare_discovered_host"), 0);
});
test("stale discovery refuses preparation and never displays a raw native failure", async () => {
  discoveryFixtures(); await render(); await discoveryToReview();
  handlers.set("connect_confirm_discovered_status", () => Promise.reject("discovery_stale_review"));
  await click("Autorizar consulta e selecionar porta");
  assert.match(container.textContent, /seleção de porta expirou/);
  assert.equal(button("Preparar anfitrião").disabled, true);
  assert.equal(count("connect_prepare_discovered_host"), 0);
  assert.throws(() => types.reviewFrom({ ...review, targetSource: "ownedInstance", instanceId: null, runId: null }));
  assert.throws(() => types.reviewFrom({ ...review, role: "guest", targetSource: "ownedInstance", instanceId: "fixture", runId: "fixture" }));
  assert.equal(types.reviewFrom({ ...review, role: "guest", targetSource: null, instanceId: null, runId: null }).targetSource, null);
});
test("discovery rejects a native target changed between review and STATUS", async () => {
  discoveryFixtures(); await render(); await discoveryToReview();
  handlers.set("connect_confirm_discovered_status", () => ({ ...discoverySelected, target: "127.0.0.1:9999" }));
  await click("Autorizar consulta e selecionar porta");
  assert.match(container.textContent, /Não foi possível concluir/);
  assert.equal(button("Preparar anfitrião").disabled, true);
  assert.equal(count("connect_prepare_discovered_host"), 0);
});

const sessionCapability = { profile: "sessionGameTcp", schemaVersion: 2, maxAdmissionSeconds: 600, maxSessionSeconds: 3600 };
const sessionTiming = { profile: "sessionGameTcp", schemaVersion: 2, admissionSeconds: 600, sessionSeconds: 3600,
  admissionExpiresAtMs: 1800000600000, sessionExpiresAtMs: 1800003600000 };
const sessionPreparation = { ...preparation, invitationProfile: "sessionGameTcp" };
const sessionReview = { ...review, sessionTiming };
const sessionExport = { invitation: '{"fixture":"session invitation"}', hostPublicId: hostId,
  expiresAtMs: sessionTiming.admissionExpiresAtMs, sessionTiming };
function preparedSession(role = "host") {
  return { ...idle(), operationId, generation: 1, role, phase: role === "host" ? "hostPrepared" : "guestPrepared",
    invitationProfile: "sessionGameTcp", preparation: { ...sessionPreparation, publicId: role === "host" ? hostId : guestId }, expiresAtMs: preparation.expiresAtMs };
}
function reviewedSession(role = "host") {
  return { ...preparedSession(role), phase: "reviewed", sessionTiming,
    review: { ...sessionReview, role, gameTarget: role === "host" ? review.gameTarget : null, localTcpBind: role === "host" ? null : "127.0.0.1:0" } };
}
function activeSession(role = "host", admitted = false) {
  return { ...reviewedSession(role), phase: role === "host" ? "hostListening" : "guestConnected",
    transportState: role === "guest" ? "authenticated" : admitted ? "admitted" : "listening", preparation: null, review: null,
    expiresAtMs: sessionTiming.sessionExpiresAtMs, tcpAddress: role === "guest" ? "127.0.0.1:42123" : null };
}
function sessionFixtures() {
  capabilities.sessionInvitations = sessionCapability;
  for (const command of ["connect_prepare_session_host", "connect_prepare_session_guest", "connect_prepare_session_host_discovered"]) {
    handlers.set(command, () => { snapshot = preparedSession(command === "connect_prepare_session_guest" ? "guest" : "host"); return snapshot.preparation; });
  }
  for (const role of ["host", "guest"]) {
    handlers.set(`connect_review_${role}`, () => { snapshot = reviewedSession(role); return snapshot.review; });
  }
  handlers.set("connect_confirm", () => { snapshot = activeSession(snapshot.role); return structuredClone(snapshot); });
  handlers.set("connect_export_invitation", () => sessionExport);
}
async function chooseSession() {
  const select = [...container.querySelectorAll("select")].find(el => el.querySelector('option[value="sessionGameTcp"]'));
  assert.ok(select);
  await act(async () => {
    for (const option of select.querySelectorAll("option")) option.selected = option.value === "sessionGameTcp";
    select.dispatchEvent(new window.Event("change", { bubbles: true }));
  });
}

test("session capability leaves the legacy 600-second command as the default", async () => {
  sessionFixtures(); await render();
  assert.equal(container.querySelector('option[value="legacy"]').selected, true);
  await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_prepare_host").args,
    { request: { mode: "loopback", bindAddress: "127.0.0.1:42000", gameTarget: "127.0.0.1:43001", lifetimeSeconds: 600 } });
  assert.equal(count("connect_prepare_session_host"), 0);
});

test("session host opts in before preparation and explicitly reviews both absolute deadlines", async () => {
  sessionFixtures(); await render(); await chooseSession();
  assert.match(container.textContent, /prazo para entrar e o encerramento da sessão são separados/);
  await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_prepare_session_host").args,
    { request: { mode: "loopback", bindAddress: "127.0.0.1:42000", gameTarget: "127.0.0.1:43001", admissionSeconds: 600, sessionSeconds: 3600 } });
  assert.equal(count("connect_prepare_host"), 0);
  await submit();
  const entry = label => [...container.querySelectorAll(".connect-review dt")].find(el => el.textContent === label).nextElementSibling.textContent;
  assert.equal(entry("Prazo para entrar"), "600 segundos desde a emissão");
  assert.equal(entry("Duração máxima da sessão"), "3600 segundos desde a emissão, sem renovação");
  assert.equal(entry("Autorize até"), types.connectDate(sessionReview.expiresAtMs));
  assert.equal(entry("Entrar até"), types.connectDate(sessionTiming.admissionExpiresAtMs));
  assert.equal(entry("Encerramento da sessão"), types.connectDate(sessionTiming.sessionExpiresAtMs));
  assert.doesNotMatch(container.textContent, /Convite e sessão expiram/);
  assert.equal(button("Autorizar conexão").disabled, true);
  assert.equal(count("connect_confirm"), 0);
  await checkConsent(); await click("Autorizar conexão");
  assert.deepEqual(calls.find(call => call.command === "connect_confirm").args, { operationId, digest });
  await click("Copiar convite");
  assert.equal(copied.at(-1), sessionExport.invitation);
  await click("Encerrar Connect");
  assert.match(container.textContent, /Recursos liberados/);
});

test("session guest uses its separate prepare command while the reviewed invitation remains opaque", async () => {
  sessionFixtures(); await render(); await click("Convidado"); await chooseSession(); await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_prepare_session_guest").args,
    { request: { mode: "loopback", expectedHostPublicId: "" } });
  assert.equal(count("connect_prepare_guest"), 0);
  await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_review_guest").args, { operationId, encodedInvitation: "" });
  await checkConsent(); await click("Autorizar conexão");
  assert.equal(container.querySelector('#connect-local-address').value, "127.0.0.1:42123");
  assert.match(container.textContent, /Entrada no mundoNão verificada/);
  assert.equal(count("connect_probe_status"), 0);
});

test("session discovery preserves separate STATUS approval and consumes the selected source only at prepare", async () => {
  discoveryFixtures(); sessionFixtures(); await render(); await chooseSession(); await discoveryToReview();
  assert.equal(count("connect_prepare_session_host_discovered"), 0);
  await click("Autorizar consulta e selecionar porta");
  assert.equal(count("connect_prepare_session_host_discovered"), 0);
  await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_prepare_session_host_discovered").args,
    { request: { mode: "loopback", bindAddress: "127.0.0.1:42000", gameTarget: "127.0.0.1:45432", admissionSeconds: 600, sessionSeconds: 3600 }, selectionId: "selection-fixture" });
  assert.equal(count("connect_prepare_discovered_host"), 0);
  assert.equal(count("connect_confirm"), 0);
});

test("session remount preserves backend timing without generating identity or renewing consent", async () => {
  sessionFixtures(); snapshot = reviewedSession(); await render(); await checkConsent();
  await act(async () => root.unmount()); root = createRoot(container); await render();
  assert.equal(button("Autorizar conexão").disabled, true);
  assert.match(container.textContent, /3600 segundos desde a emissão/);
  assert.equal(count("connect_prepare_session_host"), 0);
  assert.equal(count("connect_stop"), 0);
  await checkConsent();
  snapshot = { ...snapshot, review: { ...snapshot.review, digest: "f".repeat(64) } };
  await click("Atualizar estado");
  assert.equal(button("Autorizar conexão").disabled, true);
});

test("session admission expiry hides export without claiming an admitted connection ended", async () => {
  sessionFixtures(); snapshot = activeSession(); await render(); await click("Copiar convite");
  const admissionExpiresAtMs = Date.now() - 1000;
  snapshot = { ...snapshot, generation: 2, transportState: "admitted",
    sessionTiming: { ...sessionTiming, admissionExpiresAtMs, sessionExpiresAtMs: admissionExpiresAtMs + 3000000 }, expiresAtMs: admissionExpiresAtMs + 3000000 };
  await click("Atualizar estado");
  assert.match(container.textContent, /Convite utilizado · conexão ativa/);
  assert.doesNotMatch(container.textContent, /Conexão encerrada|session invitation/);
  assert.equal([...container.querySelectorAll("button")].some(el => el.textContent === "Copiar convite"), false);
  assert.equal(count("connect_stop"), 0);
  assert.equal(button("Encerrar Connect").disabled, false);
});

test("session pending admission expiry removes the export action while host state remains authoritative", async () => {
  sessionFixtures(); const admissionExpiresAtMs = Date.now() - 1;
  snapshot = { ...activeSession(), sessionTiming: { ...sessionTiming, admissionExpiresAtMs, sessionExpiresAtMs: admissionExpiresAtMs + 3000000 }, expiresAtMs: admissionExpiresAtMs + 3000000 };
  await render();
  assert.equal([...container.querySelectorAll("button")].some(el => el.textContent === "Copiar convite"), false);
  assert.match(container.textContent, /Aguardando convidado/);
  assert.equal(count("connect_export_invitation"), 0);
  assert.equal(count("connect_stop"), 0);
});

test("session stop supersedes pending authorization and discards a late active session", async () => {
  sessionFixtures(); snapshot = reviewedSession(); let complete;
  handlers.set("connect_confirm", () => new Promise(resolve => { complete = resolve; }));
  await render(); await checkConsent(); await click("Autorizar conexão"); await click("Encerrar Connect");
  await act(async () => complete(activeSession()));
  assert.match(container.textContent, /Conexão encerrada/);
  assert.doesNotMatch(container.textContent, /Aguardando convidado/);
});

test("session source failure and unsupported projection never authorize or expose raw native data", async () => {
  discoveryFixtures(); sessionFixtures(); await render(); await chooseSession(); await discoveryToReview();
  await click("Autorizar consulta e selecionar porta");
  handlers.set("connect_prepare_session_host_discovered", () => Promise.reject("connect_source_changed"));
  await submit();
  assert.match(container.textContent, /jogo ou sua porta mudou/);
  assert.equal(count("connect_confirm"), 0);
  assert.equal(count("connect_prepare_session_host"), 0);
});

test("session preparation with a wrong profile keeps its cancellation handle and never authorizes", async () => {
  sessionFixtures(); await render(); await chooseSession();
  handlers.set("connect_prepare_session_host", () => preparation);
  await submit();
  assert.equal(button("Encerrar Connect").disabled, false);
  assert.equal(count("connect_confirm"), 0);
  await click("Encerrar Connect");
  assert.deepEqual(calls.find(call => call.command === "connect_stop").args, { operationId });
});

test("session host form preserves integer bounds without relying on native HTML validation", async () => {
  sessionFixtures(); await render(); await chooseSession();
  const numeric = [...container.querySelectorAll('input[type="number"]')];
  assert.equal(numeric.length, 2);
  assert.equal(numeric[0].getAttribute("min"), "1");
  assert.equal(numeric[0].getAttribute("max"), "600");
  assert.equal(numeric[1].getAttribute("max"), "3600");
  for (const [admission, session] of [["601", "3600"], ["0", "3600"], ["1.5", "3600"], ["600", "599"], ["600", "3601"], ["", "3600"]]) {
    await editInput(numeric[0], admission); await editInput(numeric[1], session); await submit();
    assert.equal(count("connect_prepare_session_host"), 0);
    assert.match(container.textContent, /Confira os endereços, o código público e o prazo/);
  }
  await editInput(numeric[0], "3"); await editInput(numeric[1], "12"); await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_prepare_session_host").args.request,
    { mode: "loopback", bindAddress: "127.0.0.1:42000", gameTarget: "127.0.0.1:43001", admissionSeconds: 3, sessionSeconds: 12 });
});

test("absent session capability provides only legacy preparation and disabled backend exposes neither profile", async () => {
  await render();
  assert.equal(container.querySelector('option[value="sessionGameTcp"]'), null);
  await act(async () => root.unmount()); root = createRoot(container);
  capabilities = { ...caps, available: false }; calls = [];
  await render();
  assert.equal(container.querySelector("form"), null);
  assert.deepEqual(calls.map(call => call.command), ["connect_capabilities"]);
});

test("session timing validates signed seconds, matching expiry deltas and unchanged legacy aliases", () => {
  assert.deepEqual(types.sessionTimingFrom(sessionTiming), sessionTiming);
  assert.equal(types.reviewFrom(sessionReview).sessionTiming, sessionTiming);
  assert.equal(types.exportFrom(sessionExport).sessionTiming, sessionTiming);
  for (const value of [null, {}, { ...sessionTiming, profile: "legacy" }, { ...sessionTiming, schemaVersion: 1 },
    { ...sessionTiming, admissionSeconds: 0 }, { ...sessionTiming, admissionSeconds: 601 },
    { ...sessionTiming, sessionSeconds: 3601 }, { ...sessionTiming, sessionSeconds: 599 },
    { ...sessionTiming, sessionSeconds: 3600.1 }, { ...sessionTiming, sessionExpiresAtMs: Number.MAX_SAFE_INTEGER + 1 },
    { ...sessionTiming, sessionExpiresAtMs: sessionTiming.admissionExpiresAtMs - 1 },
    { ...sessionTiming, sessionExpiresAtMs: sessionTiming.sessionExpiresAtMs + 1 },
    { ...sessionTiming, admissionExpiresAtMs: undefined }]) assert.throws(() => types.sessionTimingFrom(value));
  assert.throws(() => types.reviewFrom({ ...sessionReview, lifetimeSeconds: 599 }));
  assert.throws(() => types.reviewFrom({ ...sessionReview, invitationExpiresAtMs: sessionTiming.admissionExpiresAtMs - 1 }));
  assert.throws(() => types.exportFrom({ ...sessionExport, expiresAtMs: sessionTiming.sessionExpiresAtMs }));
  assert.throws(() => types.reviewFrom({ ...sessionReview, expiresAtMs: sessionTiming.admissionExpiresAtMs + 1 }));
  assert.throws(() => types.reviewFrom({ ...review, lifetimeSeconds: 3600 }));
});

test("session projections reject partial or cross-profile metadata while legacy stays metadata-free", () => {
  assert.deepEqual(types.preparationFrom(preparation), preparation);
  assert.deepEqual(types.snapshotFrom(idle()), idle());
  assert.deepEqual(types.snapshotFrom(preparedSession()), preparedSession());
  assert.deepEqual(types.snapshotFrom(reviewedSession()), reviewedSession());
  assert.deepEqual(types.snapshotFrom(activeSession()), activeSession());
  for (const value of [
    { ...idle(), sessionTiming }, { ...activeSession(), invitationProfile: undefined },
    { ...idle(), invitationProfile: "sessionGameTcp" }, { ...preparedSession(), sessionTiming },
    { ...activeSession(), sessionTiming: undefined }, { ...activeSession(), expiresAtMs: sessionTiming.admissionExpiresAtMs },
    { ...reviewedSession(), review }, { ...reviewedSession(), preparation },
    { ...reviewedSession(), review: { ...sessionReview, sessionTiming: { ...sessionTiming, sessionExpiresAtMs: sessionTiming.sessionExpiresAtMs + 1 } } },
    { ...preparedSession(), invitationProfile: "legacy" },
  ]) assert.throws(() => types.snapshotFrom(value));
  assert.throws(() => types.preparationFrom({ ...sessionPreparation, sessionTiming }));
  assert.throws(() => types.preparationFrom({ ...preparation, invitationProfile: null }));
  for (const session of [null, {}, { ...sessionCapability, maxAdmissionSeconds: 601 },
    { ...sessionCapability, maxSessionSeconds: 3601 }, { ...sessionCapability, schemaVersion: 1 }]) {
    assert.throws(() => types.capabilitiesFrom({ ...caps, sessionInvitations: session }));
  }
  assert.deepEqual(types.capabilitiesFrom({ ...caps, sessionInvitations: sessionCapability }).sessionInvitations, sessionCapability);
});
