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
// Browsers reflect an omitted input type as "text"; Linkedom omits this
// default, which otherwise prevents React's text-input change event path.
Object.defineProperty(window.HTMLInputElement.prototype, "type", {
  configurable: true, get() { return this.getAttribute("type") || "text"; }, set(value) { this.setAttribute("type", value); },
});
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
const withoutPathObservation = value => { const { pathObservation: _path, ...previous } = value; return previous; };
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
      snapshot = { ...withoutPathObservation(snapshot), phase: "closed", transportState: "closed", preparation: null, review: null, tcpAddress: null, cleanupState: "complete" };
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
const hostSourceTypes = await import(await sourceModule(new URL("./host-source-types.ts", import.meta.url)));
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

const fragmentedContract = { profile: "fragmentedGameUdp", invitationSchemaVersion: 1, udpEnvelopeVersion: 2,
  approvedOriginalDatagramBytes: 4096, maxFragmentFrameBytes: 1264, localUdpBind: "127.0.0.1:35554",
  udpReturnPolicy: { type: "authorizedDownstream", bytesPerSecond: 16384, datagramsPerSecond: 20 } };
const fragmentedCapability = { ...fragmentedContract, maxLifetimeSeconds: 120 };
const fragmentedPreparation = { ...preparation, invitationProfile: "fragmentedGameUdp" };
const fragmentedReview = { ...review, lifetimeSeconds: 120, invitationExpiresAtMs: 1800000120000,
  targetSource: "manual", instanceId: null, runId: null, fragmentedUdp: { ...fragmentedContract, udpTarget: "127.0.0.1:43002" },
  services: [...review.services, { serviceId: "aux", protocol: "udp", revision: 1, requiredCapability: "fragmentedDatagram" }] };
const fragmentedExport = { invitation: '{"fixture":"fragmented invitation"}', hostPublicId: hostId,
  expiresAtMs: fragmentedReview.invitationExpiresAtMs, fragmentedUdp: fragmentedContract };
const udpNotStarted = { udpState: "notStarted", udpAddress: null, udpErrorCode: null, udpOriginalCapacity: null, udpCapacityScope: null };
const udpAvailable = { udpState: "available", udpAddress: "127.0.0.1:35554", udpErrorCode: null, udpOriginalCapacity: 4096, udpCapacityScope: "initialAssociation" };
function preparedFragmented(role = "host") {
  return { ...idle(), operationId, generation: 1, role, phase: role === "host" ? "hostPrepared" : "guestPrepared",
    invitationProfile: "fragmentedGameUdp", preparation: { ...fragmentedPreparation, publicId: role === "host" ? hostId : guestId },
    expiresAtMs: preparation.expiresAtMs, ...(role === "guest" ? udpNotStarted : {}) };
}
function reviewedFragmented(role = "host") {
  return { ...preparedFragmented(role), phase: "reviewed", fragmentedUdp: fragmentedContract,
    review: { ...fragmentedReview, role, gameTarget: role === "host" ? review.gameTarget : null, localTcpBind: role === "host" ? null : "127.0.0.1:0",
      targetSource: role === "host" ? "manual" : null, fragmentedUdp: { ...fragmentedContract, udpTarget: role === "host" ? "127.0.0.1:43002" : null } } };
}
function activeFragmented(role = "guest", udp = { ...udpNotStarted, udpState: "starting" }) {
  return { ...reviewedFragmented(role), phase: role === "host" ? "hostListening" : "guestConnected",
    transportState: role === "guest" ? "authenticated" : "listening", preparation: null, review: null,
    expiresAtMs: fragmentedReview.invitationExpiresAtMs, tcpAddress: role === "guest" ? "127.0.0.1:42123" : null,
    ...(role === "guest" ? udp : {}) };
}
function fragmentedFixtures() {
  capabilities.fragmentedUdpInvitations = fragmentedCapability;
  for (const role of ["host", "guest"]) {
    handlers.set(`connect_prepare_fragmented_${role}`, () => { snapshot = preparedFragmented(role); return snapshot.preparation; });
    handlers.set(`connect_review_${role}`, () => { snapshot = reviewedFragmented(role); return snapshot.review; });
  }
  handlers.set("connect_confirm", () => { snapshot = activeFragmented(snapshot.role); return structuredClone(snapshot); });
  handlers.set("connect_export_invitation", () => fragmentedExport);
  handlers.set("connect_stop", () => {
    snapshot = { ...snapshot, phase: "closed", transportState: "closed", preparation: null, review: null,
      tcpAddress: null, cleanupState: "complete", ...(snapshot.role === "guest" ? { ...udpNotStarted, udpState: "closed" } : {}) };
    return structuredClone(snapshot);
  });
}
async function chooseProfile(profile) {
  const select = [...container.querySelectorAll("select")].find(el => el.querySelector(`option[value="${profile}"]`));
  assert.ok(select);
  await act(async () => {
    for (const option of select.querySelectorAll("option")) option.selected = false;
    select.querySelector(`option[value="${profile}"]`).selected = true;
    assert.equal(select.value, profile);
    select.dispatchEvent(new window.Event("change", { bubbles: true }));
  });
}
const labeledInput = label => {
  const input = [...container.querySelectorAll("label")].find(el => el.textContent.startsWith(label))?.querySelector("input");
  assert.ok(input, `Input ${label}`); return input;
};

test("fragmented capability is opt-in and preserves the legacy default and separate durations", async () => {
  sessionFixtures(); fragmentedFixtures(); await render();
  assert.equal(container.querySelector('option[value="legacy"]').selected, true);
  assert.equal(labeledInput("Prazo da sessão").value, "600");
  await chooseProfile("fragmentedGameUdp");
  assert.equal(labeledInput("Prazo do convite e da conexão").value, "120");
  assert.equal(labeledInput("Serviço UDP").value, "", "A UDP service is never silently selected");
  await editInput(labeledInput("Prazo do convite e da conexão"), "45");
  await chooseProfile("sessionGameTcp");
  assert.equal(labeledInput("Prazo para entrar").value, "600");
  assert.equal(labeledInput("Duração máxima").value, "3600");
  await chooseProfile("fragmentedGameUdp");
  assert.equal(labeledInput("Prazo do convite e da conexão").value, "45");
  await chooseProfile("legacy"); await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_prepare_host").args.request,
    { mode: "loopback", bindAddress: "127.0.0.1:42000", gameTarget: "127.0.0.1:43001", lifetimeSeconds: 600 });
  assert.equal(count("connect_prepare_fragmented_host"), 0);
});

test("fragmented host explicitly selects its UDP endpoint, reviews both grants and authorizes only the digest", async () => {
  fragmentedFixtures(); await render(); await chooseProfile("fragmentedGameUdp");
  await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002"); await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_prepare_fragmented_host").args,
    { request: { mode: "loopback", bindAddress: "127.0.0.1:42000", gameTarget: "127.0.0.1:43001", udpTarget: "127.0.0.1:43002", lifetimeSeconds: 120 } });
  assert.equal(count("connect_confirm"), 0); await submit();
  assert.match(container.textContent, /Destino UDP autorizado127\.0\.0\.1:43002/);
  assert.match(container.textContent, /4096 bytes · fragmentação limitada/);
  assert.match(container.textContent, /16384 bytes por segundo, incluindo os cabeçalhos da ponte, e 20 fragmentos por segundo/);
  assert.match(container.textContent, /Não verificada · exige plano, diff, autorização e recuperação separados/);
  assert.equal(button("Autorizar conexão").disabled, true);
  await checkConsent(); await click("Autorizar conexão");
  assert.deepEqual(calls.find(call => call.command === "connect_confirm").args, { operationId, digest });
  assert.match(container.textContent, /Serviço UDP previsto/);
  assert.doesNotMatch(container.textContent, /Ponte UDP disponível/);
  await click("Copiar convite"); assert.equal(copied.at(-1), fragmentedExport.invitation);
  assert.doesNotMatch(container.querySelector("textarea").value, /43002|udpTarget/);
  await click("Encerrar Connect"); assert.match(container.textContent, /Recursos liberados/);
});

test("fragmented selection clears the discovered LAN source and never reuses its attestation", async () => {
  discoveryFixtures(); fragmentedFixtures(); await render(); await discoveryToReview();
  await click("Autorizar consulta e selecionar porta");
  await chooseProfile("fragmentedGameUdp");
  assert.equal(container.querySelector('option[value="ownedInstance"]'), null);
  assert.doesNotMatch(container.textContent, /Servidor respondeu em/);
  assert.equal(labeledInput("Servidor neste computador").value, "127.0.0.1:43001");
  await chooseProfile("legacy"); await chooseManaged();
  assert.equal(button("Preparar anfitrião").disabled, true, "A prior selection cannot survive switching profile");
  await chooseProfile("fragmentedGameUdp"); await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002"); await submit();
  const call = calls.find(c => c.command === "connect_prepare_fragmented_host");
  assert.equal(call.args.request.gameTarget, "127.0.0.1:43001");
  assert.equal(Object.hasOwn(call.args, "selectionId"), false);
  assert.equal(count("connect_prepare_discovered_host"), 0);
});

test("fragmented host form rejects fractional TTL, missing or non-loopback UDP before preparing", async () => {
  fragmentedFixtures(); await render(); await chooseProfile("fragmentedGameUdp");
  const ttl = labeledInput("Prazo do convite e da conexão"), udp = labeledInput("Serviço UDP");
  for (const [seconds, endpoint] of [["0", "127.0.0.1:43002"], ["121", "127.0.0.1:43002"], ["0.5", "127.0.0.1:43002"],
    ["", "127.0.0.1:43002"], ["120", ""], ["120", "localhost:43002"], ["120", "192.168.1.2:43002"], ["120", "127.0.0.1:0"]]) {
    await editInput(ttl, seconds); await editInput(udp, endpoint); await submit();
    assert.equal(count("connect_prepare_fragmented_host"), 0);
  }
  await editInput(ttl, "3"); await editInput(udp, "[::1]:43002"); await submit();
  assert.equal(calls.find(c => c.command === "connect_prepare_fragmented_host").args.request.lifetimeSeconds, 3);
  assert.equal(calls.find(c => c.command === "connect_prepare_fragmented_host").args.request.udpTarget, "[::1]:43002");
});

test("fragmented guest exposes TCP and STATUS while UDP starts, fails or closes without fallback", async () => {
  fragmentedFixtures(); await render(); await click("Convidado"); await chooseProfile("fragmentedGameUdp");
  assert.equal(container.querySelector('input[type="number"]'), null);
  assert.match(container.textContent, /127\.0\.0\.1:35554/); await submit();
  assert.deepEqual(calls.find(c => c.command === "connect_prepare_fragmented_guest").args,
    { request: { mode: "loopback", expectedHostPublicId: "" } });
  assert.match(container.textContent, /Ponte UDP ainda não iniciada/);
  await submit(); assert.doesNotMatch(container.textContent, /Destino UDP autorizado/);
  await checkConsent(); await click("Autorizar conexão");
  assert.match(container.textContent, /Preparando ponte UDP/);
  assert.equal(container.querySelector("#connect-local-address").value, "127.0.0.1:42123");
  await click("Consultar servidor"); assert.match(container.textContent, /Servidor respondeu/);
  for (const error of ["connect_udp_setup_failed", "connect_udp_capacity_unavailable", "connect_udp_closed"]) {
    snapshot = { ...snapshot, ...udpNotStarted, udpState: error === "connect_udp_closed" ? "closed" : "failed", udpErrorCode: error };
    await click("Atualizar estado");
    assert.equal(container.querySelector("#connect-local-address").value, "127.0.0.1:42123");
    assert.equal(button("Consultar servidor").disabled, false);
    assert.match(container.textContent, /Servidor respondeu/);
    assert.equal(count("connect_stop"), 0);
    assert.equal(count("connect_prepare_fragmented_guest"), 1);
  }
  await click("Encerrar Connect");
  assert.match(container.textContent, /Ponte UDP encerrada/);
  assert.equal(container.querySelector("#connect-local-address"), null);
});

test("fragmented available projection shows only the initial negotiated capacity and never promotes voice", async () => {
  fragmentedFixtures(); snapshot = activeFragmented("guest", udpAvailable); await render();
  assert.match(container.textContent, /Ponte UDP disponível/);
  assert.match(container.textContent, /4096 bytes na primeira associação; o caminho pode reduzir esse limite/);
  assert.match(container.textContent, /Anúncio e configuração dos modsNão verificados/);
  assert.match(container.textContent, /ÁudioNão testado/);
  for (const alteration of [{ udpAddress: "127.0.0.1:35555" }, { udpOriginalCapacity: 1264 }, { udpCapacityScope: "allAssociations" },
    { udpErrorCode: "PRIVATE_RAW_ERROR" }, { fragmentedUdp: { ...fragmentedContract, approvedOriginalDatagramBytes: 8192 } }]) {
    handlers.set("connect_snapshot", () => ({ ...snapshot, ...alteration }));
    await click("Atualizar estado");
    assert.match(container.textContent, /Não foi possível concluir/);
    assert.doesNotMatch(container.textContent, /35555|8192|PRIVATE_RAW_ERROR|allAssociations/);
    assert.equal(container.querySelector("#connect-local-address").value, "127.0.0.1:42123");
  }
  assert.equal(count("connect_confirm"), 0);
});

test("fragmented remount and altered review reset consent while invalid metadata never replaces the review", async () => {
  fragmentedFixtures(); snapshot = reviewedFragmented(); await render(); await checkConsent();
  await act(async () => root.unmount()); root = createRoot(container); await render();
  assert.equal(button("Autorizar conexão").disabled, true); await checkConsent();
  snapshot = { ...snapshot, review: { ...snapshot.review, digest: "f".repeat(64), fragmentedUdp: { ...fragmentedContract, udpTarget: "127.0.0.1:43003" } } };
  await click("Atualizar estado"); assert.equal(button("Autorizar conexão").disabled, true);
  assert.match(container.textContent, /Destino UDP autorizado127\.0\.0\.1:43003/);
  await checkConsent();
  handlers.set("connect_snapshot", () => ({ ...snapshot, review: { ...snapshot.review, fragmentedUdp: { ...snapshot.review.fragmentedUdp, udpReturnPolicy: { ...fragmentedContract.udpReturnPolicy, datagramsPerSecond: 21 } } } }));
  await click("Atualizar estado");
  assert.equal(button("Autorizar conexão").disabled, true);
  assert.doesNotMatch(container.textContent, /21 fragmentos/);
  assert.equal(count("connect_confirm"), 0);
});

test("fragmented cancellation wins a pending confirmation and retains wrong-profile preparation handles", async () => {
  fragmentedFixtures(); snapshot = reviewedFragmented("guest"); let complete;
  handlers.set("connect_confirm", () => new Promise(resolve => { complete = resolve; }));
  await render(); await checkConsent(); await click("Autorizar conexão"); await click("Encerrar Connect");
  await act(async () => complete(activeFragmented("guest", udpAvailable)));
  assert.match(container.textContent, /Recursos liberados/);
  assert.doesNotMatch(container.textContent, /Ponte UDP disponível/);
  assert.equal(container.querySelector("#connect-local-address"), null);
  await chooseProfile("fragmentedGameUdp");
  await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002");
  handlers.set("connect_prepare_fragmented_host", () => preparation); await submit();
  assert.equal(button("Encerrar Connect").disabled, false);
  await click("Encerrar Connect");
  assert.deepEqual(calls.filter(c => c.command === "connect_stop").at(-1).args, { operationId });
});

test("fragmented projections compose base validation and reject cross-profile metadata and altered policies", () => {
  for (const value of [preparedFragmented(), preparedFragmented("guest"), reviewedFragmented(), activeFragmented("host"),
    activeFragmented(), activeFragmented("guest", udpAvailable)]) assert.deepEqual(types.snapshotFrom(value), value);
  assert.deepEqual(types.reviewFrom(fragmentedReview), fragmentedReview);
  assert.deepEqual(types.exportFrom(fragmentedExport), fragmentedExport);
  for (const invalid of [
    { ...activeFragmented(), operationId: null, generation: -1 }, { ...activeFragmented(), sessionTiming },
    { ...activeFragmented(), fragmentedUdp: undefined }, { ...activeFragmented(), invitationProfile: "sessionGameTcp" },
    { ...preparedFragmented("guest"), udpState: undefined }, { ...preparedFragmented(), ...udpNotStarted },
    { ...preparedFragmented(), fragmentedUdp: fragmentedContract }, { ...reviewedFragmented(), review },
    { ...preparedFragmented("guest"), phase: "hostPrepared" }, { ...activeFragmented("host"), phase: "guestConnected" },
    { ...activeFragmented(), udpTarget: "127.0.0.1:43002" },
    { ...reviewedFragmented(), preparation }, { ...reviewedFragmented(), statusSummary: { requestedProtocol: 47 } },
    { ...activeSession(), fragmentedUdp: fragmentedContract }, { ...idle(), udpAddress: null },
  ]) assert.throws(() => types.snapshotFrom(invalid));
  for (const base of [preparation, sessionPreparation]) {
    for (const extra of [{ fragmentedUdp: fragmentedContract }, { udpState: "notStarted" }, { udpTarget: "127.0.0.1:43002" }]) {
      assert.throws(() => types.preparationFrom({ ...base, ...extra }));
    }
  }
  for (const invalid of [{ ...fragmentedReview, digest: "untrusted" }, { ...fragmentedReview, lifetimeSeconds: 0.5 },
    { ...fragmentedReview, udpTarget: "127.0.0.1:43002" },
    { ...fragmentedReview, lifetimeSeconds: 121 }, { ...fragmentedReview, sessionTiming }, { ...review, fragmentedUdp: fragmentedContract },
    { ...sessionReview, fragmentedUdp: fragmentedReview.fragmentedUdp }, { ...review, udpState: "available" },
    { ...fragmentedReview, services: [...fragmentedReview.services, review.services[0]] },
    { ...fragmentedReview, targetSource: "ownedInstance", instanceId: "x", runId: "y" }]) assert.throws(() => types.reviewFrom(invalid));
  for (const invalid of [{ ...fragmentedExport, sessionTiming }, { ...fragmentedExport, hostPublicId: "not-a-key" },
    { ...fragmentedExport, fragmentedUdp: fragmentedReview.fragmentedUdp }, { ...sessionExport, fragmentedUdp: fragmentedContract },
    { ...sessionExport, udpState: "available" }]) assert.throws(() => types.exportFrom(invalid));
  assert.throws(() => types.capabilitiesFrom({ ...caps, fragmentedUdpInvitations: { ...fragmentedCapability, maxLifetimeSeconds: 600 } }));
  assert.throws(() => types.capabilitiesFrom({ ...caps, available: false, fragmentedUdpInvitations: fragmentedCapability }));
  assert.throws(() => types.capabilitiesFrom({ ...caps, sessionInvitations: { ...sessionCapability, fragmentedUdp: fragmentedContract } }));
  assert.throws(() => types.sessionTimingFrom({ ...sessionTiming, udpState: "available" }));
  assert.throws(() => types.preparationFrom({ ...fragmentedPreparation, udpTarget: "127.0.0.1:43002" }));
});

test("absent fragmented capability never offers UDP and a mismatched export is not copied", async () => {
  sessionFixtures(); await render();
  assert.equal(container.querySelector('option[value="fragmentedGameUdp"]'), null);
  await act(async () => root.unmount()); root = createRoot(container);
  fragmentedFixtures(); snapshot = activeFragmented("host");
  handlers.set("connect_export_invitation", () => sessionExport); await render(); await click("Copiar convite");
  assert.deepEqual(copied, []);
  assert.doesNotMatch(container.textContent, /session invitation/);
});

const sessionFragmentedProfile = "sessionFragmentedGameUdp";
const sessionFragmentedContract = { ...fragmentedContract, profile: sessionFragmentedProfile, invitationSchemaVersion: 3 };
const sessionFragmentedCapability = { ...sessionFragmentedContract, maxAdmissionSeconds: 600, maxSessionSeconds: 3600 };
const sessionFragmentedTiming = { ...sessionTiming, profile: sessionFragmentedProfile, schemaVersion: 3 };
const sessionFragmentedExport = { ...sessionExport, invitation: '{"fixture":"schema3 opaque invitation"}',
  fragmentedUdp: sessionFragmentedContract, sessionTiming: sessionFragmentedTiming };
function preparedSessionFragmented(role = "host") {
  const base = preparedFragmented(role);
  return { ...base, invitationProfile: sessionFragmentedProfile,
    preparation: { ...base.preparation, invitationProfile: sessionFragmentedProfile } };
}
function reviewedSessionFragmented(role = "host") {
  const base = reviewedFragmented(role);
  return { ...base, invitationProfile: sessionFragmentedProfile,
    preparation: { ...base.preparation, invitationProfile: sessionFragmentedProfile },
    fragmentedUdp: sessionFragmentedContract, sessionTiming: sessionFragmentedTiming,
    review: { ...base.review, lifetimeSeconds: 600, invitationExpiresAtMs: sessionFragmentedTiming.admissionExpiresAtMs,
      fragmentedUdp: { ...sessionFragmentedContract, udpTarget: base.review.fragmentedUdp.udpTarget }, sessionTiming: sessionFragmentedTiming } };
}
function activeSessionFragmented(role = "guest", udp = { ...udpNotStarted, udpState: "starting" }) {
  return { ...reviewedSessionFragmented(role), phase: role === "host" ? "hostListening" : "guestConnected",
    transportState: role === "guest" ? "authenticated" : "listening", preparation: null, review: null,
    expiresAtMs: sessionFragmentedTiming.sessionExpiresAtMs, tcpAddress: role === "guest" ? "127.0.0.1:42123" : null,
    ...(role === "guest" ? udp : {}) };
}
function sessionFragmentedFixtures() {
  capabilities.sessionFragmentedUdpInvitations = sessionFragmentedCapability;
  for (const role of ["host", "guest"]) {
    handlers.set(`connect_prepare_session_fragmented_${role}`, () => { snapshot = preparedSessionFragmented(role); return snapshot.preparation; });
    handlers.set(`connect_review_${role}`, () => { snapshot = reviewedSessionFragmented(role); return snapshot.review; });
  }
  handlers.set("connect_confirm", () => { snapshot = activeSessionFragmented(snapshot.role); return structuredClone(snapshot); });
  handlers.set("connect_export_invitation", () => sessionFragmentedExport);
  handlers.set("connect_stop", () => {
    snapshot = { ...snapshot, phase: "closed", transportState: "closed", preparation: null, review: null,
      tcpAddress: null, cleanupState: "complete", ...(snapshot.role === "guest" ? { ...udpNotStarted, udpState: "closed" } : {}) };
    return structuredClone(snapshot);
  });
}

test("schema3 capability requires explicit selection and leaves every existing default unchanged", async () => {
  sessionFixtures(); fragmentedFixtures(); sessionFragmentedFixtures(); await render();
  assert.equal(container.querySelector('option[value="legacy"]').selected, true);
  assert.equal(labeledInput("Prazo da sessão").value, "600");
  await chooseProfile(sessionFragmentedProfile);
  assert.equal(labeledInput("Prazo para entrar").value, "600");
  assert.equal(labeledInput("Duração máxima").value, "3600");
  assert.equal(labeledInput("Serviço UDP").value, "");
  assert.equal(container.querySelectorAll('input[type="number"]').length, 2);
  assert.doesNotMatch(container.textContent, /Convite e conexão expiram juntos|um único vencimento/);
  await chooseProfile("fragmentedGameUdp");
  assert.equal(labeledInput("Prazo do convite e da conexão").value, "120");
  await chooseProfile("legacy"); await submit();
  assert.equal(count("connect_prepare_host"), 1);
  assert.equal(count("connect_prepare_session_fragmented_host"), 0);
});

test("schema3 host reviews both grants and deadlines before digest-only authorization", async () => {
  sessionFragmentedFixtures(); await render(); await chooseProfile(sessionFragmentedProfile);
  await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002"); await submit();
  assert.deepEqual(calls.find(c => c.command === "connect_prepare_session_fragmented_host").args,
    { request: { mode: "loopback", bindAddress: "127.0.0.1:42000", gameTarget: "127.0.0.1:43001",
      udpTarget: "127.0.0.1:43002", admissionSeconds: 600, sessionSeconds: 3600 } });
  assert.equal(count("connect_prepare_fragmented_host"), 0); await submit();
  assert.match(container.textContent, /Destino UDP autorizado127\.0\.0\.1:43002/);
  assert.match(container.textContent, /600 segundos desde a emissão/);
  assert.match(container.textContent, /3600 segundos desde a emissão, sem renovação/);
  assert.match(container.textContent, /porta local e os dois prazos/);
  assert.match(container.textContent, /20 fragmentos por segundo/);
  assert.equal(button("Autorizar conexão").disabled, true);
  await checkConsent(); await click("Autorizar conexão");
  assert.deepEqual(calls.find(c => c.command === "connect_confirm").args, { operationId, digest });
  await click("Copiar convite"); assert.equal(copied.at(-1), sessionFragmentedExport.invitation);
  assert.doesNotMatch(container.querySelector("textarea").value, /udpTarget|43002/);
  await click("Encerrar Connect"); assert.match(container.textContent, /Recursos liberados/);
});

test("schema3 host rejects fractional durations, reversed deadlines and unapproved UDP targets before IPC", async () => {
  sessionFragmentedFixtures(); await render(); await chooseProfile(sessionFragmentedProfile);
  for (const [admission, session, target] of [["0", "3600", "127.0.0.1:43002"], ["601", "3600", "127.0.0.1:43002"],
    ["600", "599", "127.0.0.1:43002"], ["600", "3601", "127.0.0.1:43002"], ["1.5", "60", "127.0.0.1:43002"],
    ["1", "60.5", "127.0.0.1:43002"], ["600", "3600", ""], ["600", "3600", "localhost:43002"],
    ["600", "3600", "192.168.1.2:43002"], ["600", "3600", "127.0.0.1:0"]]) {
    await editInput(labeledInput("Prazo para entrar"), admission);
    await editInput(labeledInput("Duração máxima"), session);
    await editInput(labeledInput("Serviço UDP"), target); await submit();
    assert.equal(count("connect_prepare_session_fragmented_host"), 0);
  }
  await editInput(labeledInput("Prazo para entrar"), "10");
  await editInput(labeledInput("Duração máxima"), "60");
  await editInput(labeledInput("Serviço UDP"), "[::1]:43003"); await submit();
  assert.deepEqual(calls.find(c => c.command === "connect_prepare_session_fragmented_host").args.request,
    { mode: "loopback", bindAddress: "127.0.0.1:42000", gameTarget: "127.0.0.1:43001", udpTarget: "[::1]:43003", admissionSeconds: 10, sessionSeconds: 60 });
});

test("schema3 selection discards a LAN selection and never reuses discovery authorization", async () => {
  discoveryFixtures(); sessionFragmentedFixtures(); await render(); await discoveryToReview();
  await click("Autorizar consulta e selecionar porta"); await chooseProfile(sessionFragmentedProfile);
  assert.equal(container.querySelector('option[value="ownedInstance"]'), null);
  assert.doesNotMatch(container.textContent, /Servidor respondeu em/);
  await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002"); await submit();
  const args = calls.find(c => c.command === "connect_prepare_session_fragmented_host").args;
  assert.equal(Object.hasOwn(args, "selectionId"), false);
  assert.equal(args.request.gameTarget, "127.0.0.1:43001");
  assert.equal(count("connect_prepare_discovered_host"), 0);
  assert.equal(count("connect_prepare_session_host_discovered"), 0);
});

test("schema3 guest uses its explicit command and keeps TCP and STATUS through separate UDP failure", async () => {
  sessionFragmentedFixtures(); await render(); await click("Convidado"); await chooseProfile(sessionFragmentedProfile);
  assert.equal(container.querySelector('input[type="number"]'), null);
  assert.equal([...container.querySelectorAll("label")].some(l => l.textContent.startsWith("Serviço UDP")), false);
  await submit();
  assert.deepEqual(calls.find(c => c.command === "connect_prepare_session_fragmented_guest").args,
    { request: { mode: "loopback", expectedHostPublicId: "" } });
  await submit(); assert.deepEqual(calls.find(c => c.command === "connect_review_guest").args, { operationId, encodedInvitation: "" });
  await checkConsent(); await click("Autorizar conexão");
  assert.match(container.textContent, /Preparando ponte UDP/);
  await click("Consultar servidor");
  snapshot = { ...snapshot, ...udpNotStarted, udpState: "failed", udpErrorCode: "connect_udp_setup_failed" };
  await click("Atualizar estado");
  assert.equal(container.querySelector("#connect-local-address").value, "127.0.0.1:42123");
  assert.match(container.textContent, /Servidor respondeu/);
  assert.match(container.textContent, /Ponte UDP indisponível/);
  assert.match(container.textContent, /Simple Voice ChatDesativado nesta integraçãoPlasmo VoiceDesativado nesta integração/);
  assert.equal(count("connect_stop"), 0);
  assert.equal(count("connect_prepare_fragmented_guest"), 0);
  await click("Encerrar Connect"); assert.match(container.textContent, /Ponte UDP encerrada/);
});

test("schema3 admission expiry hides the invitation while admitted TCP and UDP remain observed", async () => {
  sessionFragmentedFixtures(); snapshot = activeSessionFragmented("host"); await render(); await click("Copiar convite");
  const admissionExpiresAtMs = Date.now() - 1;
  snapshot = { ...snapshot, generation: 2, transportState: "admitted", expiresAtMs: admissionExpiresAtMs + 3000000,
    sessionTiming: { ...sessionFragmentedTiming, admissionExpiresAtMs, sessionExpiresAtMs: admissionExpiresAtMs + 3000000 } };
  await click("Atualizar estado");
  assert.match(container.textContent, /Convite utilizado · conexão ativa/);
  assert.doesNotMatch(container.textContent, /schema3 opaque invitation/);
  assert.equal([...container.querySelectorAll("button")].some(b => b.textContent === "Copiar convite"), false);
  assert.equal(count("connect_stop"), 0);
  assert.equal(button("Encerrar Connect").disabled, false);
});

test("schema3 remount and changed revision clear consent and stopping defeats a late confirmation", async () => {
  sessionFragmentedFixtures(); snapshot = reviewedSessionFragmented("guest"); await render(); await checkConsent();
  await act(async () => root.unmount()); root = createRoot(container); await render();
  assert.equal(button("Autorizar conexão").disabled, true);
  await checkConsent(); snapshot = { ...snapshot, review: { ...snapshot.review, digest: "f".repeat(64) } };
  await click("Atualizar estado"); assert.equal(button("Autorizar conexão").disabled, true);
  let complete;
  handlers.set("connect_confirm", () => new Promise(resolve => { complete = resolve; }));
  await checkConsent(); await click("Autorizar conexão"); await click("Encerrar Connect");
  await act(async () => complete(activeSessionFragmented("guest", udpAvailable)));
  assert.match(container.textContent, /Recursos liberados/);
  assert.doesNotMatch(container.textContent, /Ponte UDP disponível/);
  assert.equal(container.querySelector("#connect-local-address"), null);
  assert.equal(count("connect_prepare_session_fragmented_guest"), 0);
});

test("schema3 mismatched preparation stays cancellable and review from a short UDP profile is refused", async () => {
  sessionFragmentedFixtures(); await render(); await chooseProfile(sessionFragmentedProfile);
  await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002");
  handlers.set("connect_prepare_session_fragmented_host", () => fragmentedPreparation); await submit();
  assert.equal(button("Encerrar Connect").disabled, false);
  assert.equal(count("connect_confirm"), 0); await click("Encerrar Connect");
  snapshot = preparedSessionFragmented(); await click("Atualizar estado");
  handlers.set("connect_review_host", () => fragmentedReview); await submit();
  assert.match(container.textContent, /Não foi possível concluir/);
  assert.equal(container.querySelector(".connect-consent"), null);
  assert.equal(count("connect_confirm"), 0);
});

test("schema3 refuses cross-profile and changed-deadline exports before copying", async () => {
  sessionFragmentedFixtures(); snapshot = activeSessionFragmented("host"); await render();
  for (const invalid of [fragmentedExport, sessionExport, { ...sessionFragmentedExport,
    expiresAtMs: sessionFragmentedExport.expiresAtMs + 1000,
    sessionTiming: { ...sessionFragmentedTiming, admissionExpiresAtMs: sessionFragmentedTiming.admissionExpiresAtMs + 1000,
      sessionExpiresAtMs: sessionFragmentedTiming.sessionExpiresAtMs + 1000 } }]) {
    handlers.set("connect_export_invitation", () => invalid); await click("Copiar convite");
    assert.deepEqual(copied, []);
  }
});

test("closed profile metadata cannot describe the next differently selected attempt", async () => {
  sessionFixtures(); fragmentedFixtures(); sessionFragmentedFixtures();
  snapshot = { ...activeSession("host"), phase: "closed", transportState: "closed", cleanupState: "complete" };
  await render(); await chooseProfile("fragmentedGameUdp");
  assert.match(container.textContent, /Convite e conexão expiram juntos em até 120 segundos/);
  assert.doesNotMatch(container.textContent, /prazo para entrar e o encerramento da sessão são separados/);
  snapshot = { ...activeFragmented("host"), phase: "closed", transportState: "closed", cleanupState: "complete" };
  await click("Atualizar estado"); await chooseProfile("sessionGameTcp");
  assert.match(container.textContent, /prazo para entrar e o encerramento da sessão são separados/);
  assert.doesNotMatch(container.textContent, /Transporte TCP e UDP experimental\./);
  await chooseProfile(sessionFragmentedProfile);
  assert.match(container.textContent, /Transporte TCP e UDP experimental\. O prazo para entrar/);
});

test("schema3 parser rejects crossed schema, partial deadlines, unsigned destinations and altered UDP policies", () => {
  for (const value of [preparedSessionFragmented(), preparedSessionFragmented("guest"), reviewedSessionFragmented(),
    reviewedSessionFragmented("guest"), activeSessionFragmented("host"), activeSessionFragmented("guest", udpAvailable)]) {
    assert.deepEqual(types.snapshotFrom(value), value);
  }
  assert.deepEqual(types.exportFrom(sessionFragmentedExport), sessionFragmentedExport);
  assert.deepEqual(types.reviewFrom(reviewedSessionFragmented().review), reviewedSessionFragmented().review);
  for (const invalid of [
    { ...activeSessionFragmented(), sessionTiming }, { ...activeSessionFragmented(), fragmentedUdp: fragmentedContract },
    { ...activeSessionFragmented(), sessionTiming: undefined }, { ...activeSessionFragmented(), fragmentedUdp: undefined },
    { ...activeSessionFragmented(), invitationProfile: "fragmentedGameUdp" },
    { ...activeSessionFragmented(), expiresAtMs: sessionFragmentedTiming.admissionExpiresAtMs },
    { ...preparedSessionFragmented(), sessionTiming: sessionFragmentedTiming },
    { ...reviewedSessionFragmented(), preparation: fragmentedPreparation },
    { ...reviewedSessionFragmented(), review: fragmentedReview },
    { ...reviewedSessionFragmented(), review: { ...reviewedSessionFragmented().review, sessionTiming: { ...sessionFragmentedTiming, schemaVersion: 2 } } },
    { ...activeSessionFragmented(), fragmentedUdp: { ...sessionFragmentedContract, invitationSchemaVersion: 2 } },
    { ...activeSessionFragmented(), fragmentedUdp: { ...sessionFragmentedContract, maxLifetimeSeconds: 120 } },
    { ...activeSessionFragmented(), fragmentedUdp: { ...sessionFragmentedContract, udpReturnPolicy: { type: "replyBudget" } } },
    { ...activeSessionFragmented(), udpTarget: "127.0.0.1:43002" },
    { ...activeSessionFragmented(), udpOriginalCapacity: 4096 },
  ]) assert.throws(() => types.snapshotFrom(invalid));
  for (const invalid of [
    { ...reviewedSessionFragmented().review, sessionTiming: undefined },
    { ...reviewedSessionFragmented().review, fragmentedUdp: undefined },
    { ...reviewedSessionFragmented().review, targetSource: "ownedInstance", instanceId: "instance", runId: null },
    { ...reviewedSessionFragmented().review, invitationExpiresAtMs: sessionFragmentedTiming.sessionExpiresAtMs },
    { ...reviewedSessionFragmented().review, lifetimeSeconds: 120 },
  ]) assert.throws(() => types.reviewFrom(invalid));
  for (const invalid of [{ ...sessionFragmentedExport, sessionTiming: undefined }, { ...sessionFragmentedExport, fragmentedUdp: undefined },
    { ...sessionFragmentedExport, fragmentedUdp: fragmentedContract }, { ...sessionFragmentedExport, sessionTiming },
    { ...sessionFragmentedExport, expiresAtMs: sessionFragmentedTiming.sessionExpiresAtMs }]) assert.throws(() => types.exportFrom(invalid));
  for (const invalid of [{ ...sessionFragmentedTiming, schemaVersion: 2 }, { ...sessionTiming, schemaVersion: 3 },
    { ...sessionFragmentedTiming, admissionSeconds: 0.5 }, { ...sessionFragmentedTiming, sessionSeconds: 3601 },
    { ...sessionFragmentedTiming, sessionExpiresAtMs: sessionFragmentedTiming.sessionExpiresAtMs + 1 },
    { ...sessionFragmentedTiming, unknown: true }]) assert.throws(() => types.sessionTimingFrom(invalid));
});

test("schema3 capability is exact, omitted by default and never creates operations while disabled", async () => {
  assert.deepEqual(types.capabilitiesFrom({ ...caps, sessionFragmentedUdpInvitations: sessionFragmentedCapability }).sessionFragmentedUdpInvitations, sessionFragmentedCapability);
  for (const invalid of [fragmentedCapability, { ...sessionFragmentedCapability, maxLifetimeSeconds: 120 },
    { ...sessionFragmentedCapability, maxAdmissionSeconds: 601 }, { ...sessionFragmentedCapability, maxSessionSeconds: 3601 },
    { ...sessionFragmentedCapability, invitationSchemaVersion: 1 }, { ...sessionFragmentedCapability, localUdpBind: "127.0.0.1:35555" },
    { ...sessionFragmentedCapability, udpReturnPolicy: { ...fragmentedContract.udpReturnPolicy, datagramsPerSecond: 21 } }]) {
    assert.throws(() => types.capabilitiesFrom({ ...caps, sessionFragmentedUdpInvitations: invalid }));
  }
  assert.throws(() => types.capabilitiesFrom({ ...caps, available: false, sessionFragmentedUdpInvitations: sessionFragmentedCapability }));
  await render(); assert.equal(container.querySelector(`option[value="${sessionFragmentedProfile}"]`), null);
  await act(async () => root.unmount()); root = createRoot(container);
  capabilities.available = false; calls = []; await render();
  assert.deepEqual(calls.map(c => c.command), ["connect_capabilities"]);
  assert.equal(container.querySelector("form"), null);
});

const udpRecovery = { attemptsUsed: 0, maxAttempts: 3, pending: false };
function failedRecoverableUdp() {
  return { ...activeSessionFragmented("guest", { ...udpNotStarted, udpState: "failed", udpErrorCode: "connect_udp_setup_failed" }),
    udpRecovery: { ...udpRecovery } };
}
function recoveryFixtures() {
  sessionFragmentedFixtures();
  capabilities.sessionFragmentedUdpInvitations = { ...sessionFragmentedCapability, udpRecovery: { maxAttempts: 3 } };
  snapshot = failedRecoverableUdp();
  handlers.set("connect_retry_session_udp", () => {
    snapshot = { ...snapshot, generation: snapshot.generation + 1, ...udpNotStarted, udpState: "starting",
      udpRecovery: { ...snapshot.udpRecovery, attemptsUsed: snapshot.udpRecovery.attemptsUsed + 1, pending: true } };
    return structuredClone(snapshot);
  });
  handlers.set("connect_stop", () => {
    snapshot = { ...snapshot, generation: snapshot.generation + 1, phase: "closed", transportState: "closed",
      ...udpNotStarted, udpState: "closed", udpRecovery: { ...snapshot.udpRecovery, pending: false },
      preparation: null, review: null, tcpAddress: null, cleanupState: "complete" };
    return structuredClone(snapshot);
  });
}
const hasRetryButton = () => [...container.querySelectorAll("button")].some(b => b.textContent === "Tentar UDP novamente");

test("UDP recovery is a separate manual action preserving session authority, TCP and destination", async () => {
  recoveryFixtures(); await render();
  const originalTiming = structuredClone(snapshot.sessionTiming);
  assert.equal(count("connect_retry_session_udp"), 0);
  assert.match(container.textContent, /Novas tentativas UDP utilizadas: 0 de 3/);
  await click("Atualizar estado"); assert.equal(count("connect_retry_session_udp"), 0);
  await click("Tentar UDP novamente");
  assert.deepEqual(calls.find(c => c.command === "connect_retry_session_udp").args, { operationId, expectedGeneration: 1 });
  assert.equal(container.querySelector("#connect-local-address").value, "127.0.0.1:42123");
  assert.deepEqual(snapshot.sessionTiming, originalTiming);
  assert.match(container.textContent, /Novas tentativas UDP utilizadas: 1 de 3/);
  assert.match(container.textContent, /Preparando novamente a ponte UDP/);
  assert.equal(hasRetryButton(), false);
  assert.equal(count("connect_prepare_session_fragmented_guest"), 0);
  assert.equal(count("connect_confirm"), 0);
  snapshot = { ...snapshot, generation: 3, ...udpAvailable, udpRecovery: { ...snapshot.udpRecovery, pending: false } };
  await click("Atualizar estado");
  assert.match(container.textContent, /Ponte UDP disponível/);
  assert.equal(count("connect_retry_session_udp"), 1);
  await click("Consultar servidor"); assert.match(container.textContent, /Servidor respondeu/);
  assert.match(container.textContent, /isto não confirma áudio/);
});

test("UDP recovery permits exactly three additional manual attempts without automatic repeats", async () => {
  recoveryFixtures(); await render();
  for (let attempt = 1; attempt <= 3; attempt++) {
    assert.equal(hasRetryButton(), true); await click("Tentar UDP novamente");
    assert.equal(count("connect_retry_session_udp"), attempt);
    snapshot = { ...snapshot, generation: snapshot.generation + 1, ...udpNotStarted, udpState: "failed",
      udpErrorCode: "connect_udp_setup_failed", udpRecovery: { ...snapshot.udpRecovery, pending: false } };
    await click("Atualizar estado");
    assert.equal(count("connect_retry_session_udp"), attempt);
  }
  assert.equal(hasRetryButton(), false);
  assert.match(container.textContent, /limite de novas tentativas UDP desta sessão foi atingido/);
  assert.equal(button("Consultar servidor").disabled, false);
  assert.equal(count("connect_stop"), 0);
  assert.equal(container.querySelector("#connect-local-address").value, "127.0.0.1:42123");
});

test("UDP recovery uses admitted session expiry after admission has already elapsed", async () => {
  recoveryFixtures(); const admissionExpiresAtMs = Date.now() - 1000;
  snapshot = { ...snapshot, sessionTiming: { ...sessionFragmentedTiming, admissionExpiresAtMs, sessionExpiresAtMs: admissionExpiresAtMs + 3000000 },
    expiresAtMs: admissionExpiresAtMs + 3000000 };
  await render(); assert.equal(hasRetryButton(), true); await click("Tentar UDP novamente");
  assert.equal(count("connect_retry_session_udp"), 1);
  assert.equal(snapshot.sessionTiming.admissionExpiresAtMs, admissionExpiresAtMs);
  assert.equal(snapshot.sessionTiming.sessionExpiresAtMs, admissionExpiresAtMs + 3000000);
});

test("UDP recovery cannot be requested after the observed absolute session deadline", async () => {
  recoveryFixtures(); const sessionExpiresAtMs = Date.now() - 1000;
  snapshot = { ...snapshot, expiresAtMs: sessionExpiresAtMs,
    sessionTiming: { ...sessionFragmentedTiming, admissionExpiresAtMs: sessionExpiresAtMs - 3000000, sessionExpiresAtMs } };
  await render(); assert.equal(hasRetryButton(), false); await click("Atualizar estado");
  assert.equal(count("connect_retry_session_udp"), 0);
});

test("lost UDP recovery IPC outcome waits for observation and never submits a replacement", async () => {
  recoveryFixtures();
  handlers.set("connect_retry_session_udp", () => {
    snapshot = { ...snapshot, generation: 2, ...udpNotStarted, udpState: "starting",
      udpRecovery: { attemptsUsed: 1, maxAttempts: 3, pending: true } };
    return Promise.reject("PRIVATE_LOST_IPC_REPLY");
  });
  await render(); await click("Tentar UDP novamente");
  assert.match(container.textContent, /Aguardando confirmar o estado da tentativa UDP/);
  assert.equal(button("Tentar UDP novamente").disabled, true);
  assert.doesNotMatch(container.textContent, /PRIVATE_LOST_IPC_REPLY/);
  assert.equal(count("connect_retry_session_udp"), 1);
  await click("Atualizar estado");
  assert.match(container.textContent, /Preparando novamente a ponte UDP/);
  assert.equal(hasRetryButton(), false);
  assert.doesNotMatch(container.textContent, /Aguardando confirmar o estado/);
  await act(async () => root.unmount()); root = createRoot(container); await render();
  assert.equal(count("connect_retry_session_udp"), 1, "Remount must only observe the accepted request");
  snapshot = { ...snapshot, generation: 3, ...udpAvailable, udpRecovery: { ...snapshot.udpRecovery, pending: false } };
  await click("Atualizar estado"); assert.equal(count("connect_retry_session_udp"), 1);
  assert.match(container.textContent, /Ponte UDP disponível/);
});

test("unknown UDP recovery outcome remains blocked while snapshot reads fail", async () => {
  recoveryFixtures(); await render();
  handlers.set("connect_retry_session_udp", () => Promise.reject("PRIVATE_REQUEST_FAILURE"));
  handlers.set("connect_snapshot", () => Promise.reject("PRIVATE_SNAPSHOT_FAILURE"));
  await click("Tentar UDP novamente"); await click("Atualizar estado");
  assert.equal(button("Tentar UDP novamente").disabled, true);
  assert.equal(count("connect_retry_session_udp"), 1);
  assert.doesNotMatch(container.textContent, /PRIVATE_REQUEST_FAILURE|PRIVATE_SNAPSHOT_FAILURE/);
  handlers.delete("connect_snapshot"); await click("Atualizar estado");
  assert.equal(button("Tentar UDP novamente").disabled, false, "An observed unchanged state permits another user action, not an automatic repeat");
  assert.equal(count("connect_retry_session_udp"), 1);
});

test("stale or invalid UDP recovery observations cannot resolve an unknown IPC outcome", async () => {
  recoveryFixtures(); snapshot = { ...snapshot, generation: 5 }; await render();
  handlers.set("connect_retry_session_udp", () => Promise.reject("PRIVATE_UNKNOWN_OUTCOME"));
  await click("Tentar UDP novamente");
  for (const observed of [{ ...snapshot, generation: 4 },
    { ...snapshot, generation: 6, udpRecovery: { attemptsUsed: 4, maxAttempts: 3, pending: false } }]) {
    handlers.set("connect_snapshot", () => observed); await click("Atualizar estado");
    assert.equal(button("Tentar UDP novamente").disabled, true);
    assert.equal(count("connect_retry_session_udp"), 1);
  }
  handlers.delete("connect_snapshot"); await click("Atualizar estado");
  assert.equal(button("Tentar UDP novamente").disabled, false);
  assert.equal(count("connect_retry_session_udp"), 1);
  await click("Tentar UDP novamente");
  assert.equal(count("connect_retry_session_udp"), 2, "Only this second user click resubmits the idempotent generation");
  assert.deepEqual(calls.filter(c => c.command === "connect_retry_session_udp").map(c => c.args),
    [{ operationId, expectedGeneration: 5 }, { operationId, expectedGeneration: 5 }]);
});

test("stop supersedes pending UDP recovery and rejects its late successful response", async () => {
  recoveryFixtures(); let complete;
  handlers.set("connect_retry_session_udp", () => new Promise(resolve => { complete = resolve; }));
  await render(); await click("Tentar UDP novamente");
  assert.equal(button("Tentar UDP novamente").disabled, true);
  // Even an event dispatched past native disabled-button behavior cannot cause
  // a second submission while the original command remains unresolved.
  await act(async () => button("Tentar UDP novamente").dispatchEvent(new window.Event("click", { bubbles: true })));
  assert.equal(count("connect_retry_session_udp"), 1);
  await click("Encerrar Connect");
  await act(async () => complete({ ...activeSessionFragmented("guest", udpAvailable), generation: 99,
    udpRecovery: { attemptsUsed: 1, maxAttempts: 3, pending: false } }));
  assert.match(container.textContent, /Recursos liberados/);
  assert.doesNotMatch(container.textContent, /Ponte UDP disponível/);
  assert.equal(hasRetryButton(), false);
  assert.equal(container.querySelector("#connect-local-address"), null);
});

test("mismatched UDP recovery response cannot change operation, TCP address or signed timing", async () => {
  recoveryFixtures(); await render();
  const original = structuredClone(snapshot);
  for (const delta of [{ operationId: "another-operation" }, { tcpAddress: "127.0.0.1:49999" },
    { sessionTiming: { ...sessionFragmentedTiming, admissionExpiresAtMs: sessionFragmentedTiming.admissionExpiresAtMs + 1000,
      sessionExpiresAtMs: sessionFragmentedTiming.sessionExpiresAtMs + 1000 }, expiresAtMs: sessionFragmentedTiming.sessionExpiresAtMs + 1000 }]) {
    handlers.set("connect_retry_session_udp", () => ({ ...original, generation: 2, ...udpNotStarted, udpState: "starting",
      udpRecovery: { attemptsUsed: 1, maxAttempts: 3, pending: true }, ...delta }));
    await click("Tentar UDP novamente");
    assert.equal(container.querySelector("#connect-local-address").value, "127.0.0.1:42123");
    assert.match(container.textContent, /Não foi possível concluir/);
    assert.equal(button("Tentar UDP novamente").disabled, true);
    await click("Atualizar estado");
  }
  assert.equal(count("connect_retry_session_udp"), 3);
  assert.doesNotMatch(container.textContent, /49999|another-operation/);
});

test("UDP recovery capability absent, old profiles and non-failed states never offer retry", async () => {
  sessionFragmentedFixtures(); snapshot = failedRecoverableUdp(); await render();
  assert.equal(hasRetryButton(), false, "Schema3 implementation62 has no retry capability");
  await act(async () => root.unmount()); root = createRoot(container);
  recoveryFixtures(); snapshot = activeFragmented("guest", { ...udpNotStarted, udpState: "failed", udpErrorCode: "connect_udp_setup_failed" });
  await render(); assert.equal(hasRetryButton(), false);
  for (const next of [activeSessionFragmented("host"), activeSessionFragmented("guest", udpAvailable),
    { ...activeSessionFragmented(), udpRecovery },
    { ...failedRecoverableUdp(), ...udpNotStarted, udpState: "closed", udpErrorCode: "connect_udp_closed" },
    { ...failedRecoverableUdp(), phase: "closing", udpRecovery: { ...udpRecovery, attemptsUsed: 1 }, cleanupState: "pending" }]) {
    snapshot = next; await click("Atualizar estado"); assert.equal(hasRetryButton(), false);
  }
  assert.equal(count("connect_retry_session_udp"), 0);
});

test("UDP recovery strict projection rejects malformed counts, cross-profile scope and contradictory pending states", () => {
  const active = failedRecoverableUdp();
  assert.deepEqual(types.snapshotFrom(active).udpRecovery, udpRecovery);
  for (const malformed of [null, [], {}, { ...udpRecovery, attemptsUsed: -1 }, { ...udpRecovery, attemptsUsed: 0.5 },
    { ...udpRecovery, attemptsUsed: 4 }, { ...udpRecovery, maxAttempts: 4 }, { ...udpRecovery, pending: "false" },
    { ...udpRecovery, unknown: true }, { ...udpRecovery, pending: true }]) {
    assert.throws(() => types.snapshotFrom({ ...active, udpRecovery: malformed }));
  }
  for (const misplaced of [idle(), preparedSessionFragmented("guest"), reviewedSessionFragmented("guest"),
    activeSessionFragmented("host"), activeSession(), activeFragmented("guest"),
    { ...active, role: "host" }, { ...active, tcpAddress: null }, { ...active, tcpAddress: "192.168.1.10:42123" },
    { ...active, transportState: "closed" }, { ...active, phase: "starting" }]) {
    assert.throws(() => types.snapshotFrom({ ...misplaced, udpRecovery }));
  }
  for (const impossible of [
    { ...active, udpState: "failed", udpRecovery: { attemptsUsed: 1, maxAttempts: 3, pending: true } },
    { ...active, ...udpNotStarted, udpState: "starting", udpRecovery: { attemptsUsed: 1, maxAttempts: 3, pending: false } },
    { ...active, phase: "closing", ...udpNotStarted, udpState: "starting", udpRecovery: { attemptsUsed: 1, maxAttempts: 3, pending: true } },
    { ...active, phase: "closed", tcpAddress: null, transportState: "closed", udpRecovery },
  ]) assert.throws(() => types.snapshotFrom(impossible));
  const closing = { ...active, phase: "closing", ...udpNotStarted, udpState: "starting",
    udpRecovery: { attemptsUsed: 1, maxAttempts: 3, pending: false }, cleanupState: "pending" };
  assert.deepEqual(types.snapshotFrom(closing), closing);
  const closed = { ...closing, phase: "closed", udpState: "closed", transportState: "closed", tcpAddress: null, cleanupState: "complete" };
  assert.deepEqual(types.snapshotFrom(closed), closed);
  for (const base of [review, sessionReview, fragmentedReview, reviewedSessionFragmented().review]) {
    assert.throws(() => types.reviewFrom({ ...base, udpRecovery }));
  }
  for (const base of [preparation, sessionPreparation, fragmentedPreparation, preparedSessionFragmented().preparation]) {
    assert.throws(() => types.preparationFrom({ ...base, udpRecovery }));
  }
  for (const base of [sessionExport, fragmentedExport, sessionFragmentedExport]) assert.throws(() => types.exportFrom({ ...base, udpRecovery }));
  assert.throws(() => types.sessionTimingFrom({ ...sessionFragmentedTiming, udpRecovery }));
});

test("UDP recovery capability permits only an exact optional max-three local policy", () => {
  const capable = { ...sessionFragmentedCapability, udpRecovery: { maxAttempts: 3 } };
  assert.deepEqual(types.capabilitiesFrom({ ...caps, sessionFragmentedUdpInvitations: capable }).sessionFragmentedUdpInvitations, capable);
  assert.deepEqual(types.capabilitiesFrom({ ...caps, sessionFragmentedUdpInvitations: sessionFragmentedCapability }).sessionFragmentedUdpInvitations,
    sessionFragmentedCapability, "Older schema3 capability remains readable without enabling recovery");
  for (const malformed of [null, [], {}, { maxAttempts: 0 }, { maxAttempts: 3.5 }, { maxAttempts: 4 }, { maxAttempts: 3, automatic: true }]) {
    assert.throws(() => types.capabilitiesFrom({ ...caps, sessionFragmentedUdpInvitations: { ...sessionFragmentedCapability, udpRecovery: malformed } }));
  }
  assert.throws(() => types.capabilitiesFrom({ ...caps, udpRecovery: { maxAttempts: 3 } }));
  assert.throws(() => types.capabilitiesFrom({ ...caps, sessionInvitations: { ...sessionCapability, udpRecovery: { maxAttempts: 3 } } }));
  assert.throws(() => types.capabilitiesFrom({ ...caps, fragmentedUdpInvitations: { ...fragmentedCapability, udpRecovery: { maxAttempts: 3 } } }));
});

function reviewedDiscoveredSessionFragmented() {
  const base = reviewedSessionFragmented();
  return { ...base, review: { ...base.review, targetSource: "ownedInstance", instanceId: discoverySelected.instanceId,
    runId: discoverySelected.runId, gameTarget: discoverySelected.target } };
}
function discoveredSessionFragmentedFixtures() {
  discoveryFixtures(); sessionFragmentedFixtures();
  capabilities.sessionFragmentedUdpInvitations = { ...sessionFragmentedCapability, ownedGameTcpDiscovery: true };
  handlers.set("connect_prepare_session_fragmented_host_discovered", () => {
    snapshot = preparedSessionFragmented(); return snapshot.preparation;
  });
  handlers.set("connect_review_host", () => { snapshot = reviewedDiscoveredSessionFragmented(); return snapshot.review; });
}

test("schema3 discovered game remains a separate authorized TCP selection and sends only its opaque ID", async () => {
  discoveredSessionFragmentedFixtures(); await render(); await chooseProfile(sessionFragmentedProfile);
  await discoveryToReview();
  assert.equal(button("Preparar anfitrião").disabled, true);
  assert.equal(count("connect_confirm_discovered_status"), 0);
  await click("Autorizar consulta e selecionar porta");
  assert.equal(labeledInput("Serviço UDP").value, "", "Game discovery must never infer a UDP endpoint");
  assert.match(container.textContent, /seleção confere somente a porta TCP do jogo/);
  await submit(); assert.equal(count("connect_prepare_session_fragmented_host_discovered"), 0);
  await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002"); await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_prepare_session_fragmented_host_discovered").args,
    { selectionId: "selection-fixture", request: { mode: "loopback", bindAddress: "127.0.0.1:42000",
      udpTarget: "127.0.0.1:43002", admissionSeconds: 600, sessionSeconds: 3600 } });
  assert.equal(count("connect_prepare_session_fragmented_host"), 0);
  assert.equal(count("connect_prepare_session_host_discovered"), 0);
  assert.equal(count("connect_confirm"), 0);
  await submit();
  assert.match(container.textContent, /Destino autorizado127\.0\.0\.1:45432/);
  assert.match(container.textContent, /Origem TCP conferidaInstância managed-fixture/);
  assert.match(container.textContent, /Origem UDPDestino manual autorizado separadamente/);
  assert.match(container.textContent, /Destino UDP autorizado127\.0\.0\.1:43002/);
  assert.equal(button("Autorizar conexão").disabled, true);
  await checkConsent(); await click("Autorizar conexão");
  assert.deepEqual(calls.find(call => call.command === "connect_confirm").args, { operationId, digest });
  assert.match(container.textContent, /Simple Voice ChatDesativado nesta integraçãoPlasmo VoiceDesativado nesta integração/);
  assert.equal(calls.some(call => /voice_|launch|spawn|kill/.test(call.command)), false);
});

test("schema3 discovery requires its own capability even when generic discovery and UDP recovery are available", async () => {
  discoveryFixtures(); sessionFragmentedFixtures();
  capabilities.sessionFragmentedUdpInvitations = { ...sessionFragmentedCapability, ownedGameTcpDiscovery: false, udpRecovery: { maxAttempts: 3 } };
  await render(); await chooseProfile(sessionFragmentedProfile);
  assert.equal(container.querySelector('option[value="ownedInstance"]'), null);
  await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002"); await submit();
  assert.equal(count("connect_prepare_session_fragmented_host"), 1);
  assert.equal(count("connect_prepare_session_fragmented_host_discovered"), 0);
  assert.equal(count("list_running_instances"), 0);
});

test("schema3 discovery capability never enables discovery on short UDP or a guest", async () => {
  fragmentedFixtures(); discoveredSessionFragmentedFixtures(); await render();
  await chooseProfile("fragmentedGameUdp");
  assert.equal(container.querySelector('option[value="ownedInstance"]'), null);
  await chooseProfile(sessionFragmentedProfile); await click("Convidado");
  assert.equal(container.querySelector('option[value="ownedInstance"]'), null);
  await submit(); assert.equal(count("connect_prepare_session_fragmented_guest"), 1);
  assert.equal(count("connect_prepare_session_fragmented_host_discovered"), 0);
});

test("switching profiles discards a pending source response even if both profiles support discovery", async () => {
  discoveredSessionFragmentedFixtures(); await render(); await discoveryToReview();
  let complete;
  handlers.set("connect_confirm_discovered_status", () => new Promise(resolve => { complete = resolve; }));
  await click("Autorizar consulta e selecionar porta");
  await chooseProfile(sessionFragmentedProfile); await chooseManaged();
  await act(async () => complete(discoverySelected));
  assert.equal(button("Preparar anfitrião").disabled, true);
  assert.doesNotMatch(container.textContent, /Servidor respondeu em/);
  assert.equal(count("connect_prepare_session_fragmented_host_discovered"), 0);
});

test("schema3 rejected discovered preparation consumes the local selection without automatic retry or manual fallback", async () => {
  discoveredSessionFragmentedFixtures(); await render(); await chooseProfile(sessionFragmentedProfile); await discoveryToReview();
  await click("Autorizar consulta e selecionar porta"); await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002");
  handlers.set("connect_prepare_session_fragmented_host_discovered", () => Promise.reject("connect_source_changed"));
  await submit();
  assert.equal(count("connect_prepare_session_fragmented_host_discovered"), 1);
  assert.equal(button("Preparar anfitrião").disabled, true);
  assert.doesNotMatch(container.textContent, /Servidor respondeu em/);
  await submit(); await click("Atualizar estado");
  assert.equal(count("connect_prepare_session_fragmented_host_discovered"), 1);
  assert.equal(count("connect_prepare_session_fragmented_host"), 0);
  assert.equal(labeledInput("Serviço UDP").value, "127.0.0.1:43002");
  await click("Buscar jogos abertos pelo Aurora"); assert.equal(count("list_running_instances"), 2);
});

test("schema3 discovered preparation consumes its selection before a lost IPC response can be retried", async () => {
  discoveredSessionFragmentedFixtures(); await render(); await chooseProfile(sessionFragmentedProfile); await discoveryToReview();
  await click("Autorizar consulta e selecionar porta"); await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002");
  handlers.set("connect_prepare_session_fragmented_host_discovered", () => {
    snapshot = preparedSessionFragmented(); return Promise.reject(new Error("lost response"));
  });
  await submit(); assert.equal(container.querySelector("form"), null);
  assert.match(container.textContent, /resultado do preparo ainda não foi confirmado/);
  await click("Atualizar estado");
  assert.equal(button("Encerrar Connect").disabled, false);
  assert.equal(count("connect_prepare_session_fragmented_host_discovered"), 1);
  await click("Encerrar Connect");
  assert.equal(button("Preparar anfitrião").disabled, true);
  assert.equal(count("connect_prepare_session_fragmented_host"), 0);
});

test("schema3 discovered review invalidates consent when the approved source revision changes", async () => {
  discoveredSessionFragmentedFixtures(); snapshot = reviewedDiscoveredSessionFragmented(); await render();
  await checkConsent();
  snapshot = { ...snapshot, generation: 2, review: { ...snapshot.review, generation: 2, runId: "new-run", digest: "e".repeat(64) } };
  await click("Atualizar estado"); assert.equal(button("Autorizar conexão").disabled, true);
  assert.equal(count("connect_confirm"), 0);
  await checkConsent(); await click("Autorizar conexão");
  assert.deepEqual(calls.find(call => call.command === "connect_confirm").args, { operationId, digest: "e".repeat(64) });
  snapshot = { ...activeSessionFragmented("host"), generation: 3, phase: "failed", transportState: "closed",
    cleanupState: "complete", errorCode: "connect_source_changed" };
  await click("Atualizar estado");
  assert.match(container.textContent, /Recursos liberados/);
  assert.equal([...container.querySelectorAll("button")].some(element => element.textContent === "Copiar convite"), false);
  assert.equal(count("connect_prepare_session_fragmented_host"), 0);
});

test("only schema3 host reviews accept an owned TCP source with complete bounded identity and an independent UDP target", () => {
  const own = reviewedDiscoveredSessionFragmented();
  assert.deepEqual(types.reviewFrom(own.review), own.review);
  assert.deepEqual(types.snapshotFrom(own), own);
  for (const fields of [{ instanceId: null }, { runId: null }, { instanceId: "" }, { runId: "x".repeat(257) },
    { instanceId: "invalid\ninstance" }, { targetSource: "manual" }, { targetSource: null }, { targetSource: undefined },
    { gameTarget: "192.168.1.2:12345" }, { fragmentedUdp: { ...own.review.fragmentedUdp, udpTarget: "" } },
    { fragmentedUdp: { ...own.review.fragmentedUdp, udpTarget: null } }]) assert.throws(() => types.reviewFrom({ ...own.review, ...fields }));
  assert.throws(() => types.reviewFrom({ ...fragmentedReview, targetSource: "ownedInstance", instanceId: "instance", runId: "run" }));
  assert.throws(() => types.reviewFrom({ ...reviewedSessionFragmented("guest").review, targetSource: "ownedInstance", instanceId: "instance", runId: "run" }));
});

test("schema3 discovery capability is optional, strictly boolean and absent from the signed transport contract", () => {
  for (const ownedGameTcpDiscovery of [true, false]) {
    const capability = { ...sessionFragmentedCapability, ownedGameTcpDiscovery, udpRecovery: { maxAttempts: 3 } };
    assert.deepEqual(types.capabilitiesFrom({ ...caps, sessionFragmentedUdpInvitations: capability }).sessionFragmentedUdpInvitations, capability);
  }
  for (const malformed of [null, undefined, 1, "true", {}, []]) assert.throws(() => types.capabilitiesFrom({ ...caps,
    sessionFragmentedUdpInvitations: { ...sessionFragmentedCapability, ownedGameTcpDiscovery: malformed } }));
  assert.throws(() => types.capabilitiesFrom({ ...caps, fragmentedUdpInvitations: { ...fragmentedCapability, ownedGameTcpDiscovery: true } }));
  assert.throws(() => types.exportFrom({ ...sessionFragmentedExport, fragmentedUdp: { ...sessionFragmentedContract, ownedGameTcpDiscovery: true } }));
  assert.deepEqual(types.capabilitiesFrom({ ...caps, sessionFragmentedUdpInvitations: sessionFragmentedCapability }).sessionFragmentedUdpInvitations,
    sessionFragmentedCapability, "The older schema3 capability remains readable without the new action");
});

async function chooseMode(mode) {
  const select = [...container.querySelectorAll("select")].find(element => element.querySelector('option[value="loopback"]'));
  assert.ok(select?.querySelector(`option[value="${mode}"]`));
  await act(async () => {
    for (const option of select.querySelectorAll("option")) option.selected = false;
    select.querySelector(`option[value="${mode}"]`).selected = true;
    assert.equal(select.value, mode);
    select.dispatchEvent(new window.Event("change", { bubbles: true }));
  });
}
function directIpv4Fixtures(profile, role) {
  const suffix = profile === "legacy" ? "" : profile === "sessionGameTcp" ? "session_" : profile === "fragmentedGameUdp" ? "fragmented_" : "session_fragmented_";
  if (profile === "sessionGameTcp") sessionFixtures();
  if (profile === "fragmentedGameUdp") fragmentedFixtures();
  if (profile === sessionFragmentedProfile) sessionFragmentedFixtures();
  capabilities.modes = [...caps.modes, "directIpv4"];
  const prepared = profile === sessionFragmentedProfile ? preparedSessionFragmented(role) : profile === "fragmentedGameUdp" ? preparedFragmented(role)
    : profile === "sessionGameTcp" ? preparedSession(role) : { ...idle(), operationId, generation: 1, role,
      phase: role === "host" ? "hostPrepared" : "guestPrepared", preparation, expiresAtMs: preparation.expiresAtMs };
  const reviewed = profile === sessionFragmentedProfile ? reviewedSessionFragmented(role) : profile === "fragmentedGameUdp" ? reviewedFragmented(role)
    : profile === "sessionGameTcp" ? reviewedSession(role) : { ...prepared, phase: "reviewed", review: { ...review, role,
      gameTarget: role === "host" ? review.gameTarget : null, localTcpBind: role === "host" ? null : "127.0.0.1:0" } };
  const command = `connect_prepare_${suffix}${role}`;
  handlers.set(command, () => { snapshot = structuredClone(prepared); return snapshot.preparation; });
  handlers.set(`connect_review_${role}`, () => {
    snapshot = structuredClone(reviewed); snapshot.review.mode = "directIpv4"; snapshot.review.directAddress = "192.168.1.2:42000";
    return snapshot.review;
  });
  return command;
}

for (const profile of ["legacy", "sessionGameTcp", "fragmentedGameUdp", sessionFragmentedProfile]) {
  test(`direct IPv4 host ${profile} requires an explicit local initial address and preserves approved loopback services`, async () => {
    const command = directIpv4Fixtures(profile, "host"); await render();
    assert.equal(container.querySelector('option[value="loopback"]').selected, true);
    if (profile !== "legacy") await chooseProfile(profile);
    await chooseMode("directIpv4");
    assert.equal(labeledInput("Endereço inicial").value, "");
    assert.match(container.textContent, /não configura NAT ou roteador e não habilita relay/);
    await submit(); assert.equal(count(command), 0);
    await editInput(labeledInput("Endereço inicial"), "192.168.1.2:42000");
    if (profile.includes("Udp")) await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002");
    await submit();
    const request = calls.find(call => call.command === command).args.request;
    assert.equal(request.mode, "directIpv4"); assert.equal(request.bindAddress, "192.168.1.2:42000");
    assert.equal(request.gameTarget, "127.0.0.1:43001");
    if (profile.includes("Udp")) assert.equal(request.udpTarget, "127.0.0.1:43002");
    await submit();
    assert.match(container.textContent, /ModoIPv4 direto autorizadoEndereço inicial192\.168\.1\.2:42000/);
    assert.equal(button("Autorizar conexão").disabled, true); assert.equal(count("connect_confirm"), 0);
    await checkConsent(); await click("Autorizar conexão");
    assert.deepEqual(calls.find(call => call.command === "connect_confirm").args, { operationId, digest });
    assert.match(container.textContent, /Não confirmado · relay desligado/);
    assert.equal(calls.filter(call => call.command.startsWith("connect_prepare_")).length, 1);
  });

  test(`direct IPv4 guest ${profile} selects its mode before any invitation and has no host endpoint override`, async () => {
    const command = directIpv4Fixtures(profile, "guest"); await render(); await click("Convidado");
    if (profile !== "legacy") await chooseProfile(profile);
    await chooseMode("directIpv4"); await editInput(labeledInput("Código público do anfitrião"), hostId);
    assert.equal([...container.querySelectorAll("label")].some(element => element.textContent.startsWith("Endereço inicial")), false);
    assert.match(container.textContent, /convite deve usar o IPv4 e a porta autorizados do anfitrião/);
    assert.equal(count("connect_review_guest"), 0);
    await submit();
    assert.deepEqual(calls.find(call => call.command === command).args, { request: { mode: "directIpv4", expectedHostPublicId: hostId } });
    await submit(); assert.equal(button("Autorizar conexão").disabled, true);
    assert.match(container.textContent, /ModoIPv4 direto autorizado/);
    assert.equal(count("connect_confirm"), 0);
    assert.equal(calls.filter(call => call.command.startsWith("connect_prepare_")).length, 1);
  });
}

test("direct IPv4 capability accepts only complete old or new mode sets and remains absent from an older backend", async () => {
  for (const modes of [["loopback", "externalIpv6"], ["directIpv4", "externalIpv6", "loopback"]]) {
    assert.deepEqual(types.capabilitiesFrom({ ...caps, modes }).modes, modes);
  }
  for (const modes of [[], ["directIpv4"], ["loopback", "directIpv4"], ["externalIpv6", "directIpv4"],
    ["loopback", "externalIpv6", "directIpv4", "directIpv4"], ["loopback", "externalIpv6", "loopback"],
    ["loopback", "externalIpv6", "auto"], ["loopback", "externalIpv6", "directIpv4", "auto"]]) assert.throws(() => types.capabilitiesFrom({ ...caps, modes }));
  assert.throws(() => types.capabilitiesFrom({ ...caps, available: false, modes: [...caps.modes, "directIpv4"] }));
  await render(); assert.equal(container.querySelector('option[value="directIpv4"]'), null);
  assert.equal(container.querySelector('option[value="loopback"]').selected, true);
  await submit(); assert.equal(calls.find(call => call.command === "connect_prepare_host").args.request.mode, "loopback");
});

test("direct IPv4 selection refuses invalid initial addresses without IPC and clearing the mode never retains a personal address", async () => {
  directIpv4Fixtures("legacy", "host"); await render(); await chooseMode("directIpv4");
  for (const address of ["", "127.0.0.1:42000", "0.0.0.0:42000", "[::1]:42000", "localhost:42000", "192.168.1.2:0", "192.168.001.2:42000", "100.64.0.1:42000"]) {
    await editInput(labeledInput("Endereço inicial"), address); await submit(); assert.equal(count("connect_prepare_host"), 0);
  }
  await editInput(labeledInput("Endereço inicial"), "192.168.1.2:42000"); await chooseMode("externalIpv6");
  assert.equal(labeledInput("Endereço inicial").value, "");
  await chooseMode("loopback"); assert.equal(labeledInput("Endereço inicial").value, "127.0.0.1:42000");
  await chooseMode("directIpv4"); assert.equal(labeledInput("Endereço inicial").value, "");
});

test("direct IPv4 connection failure never selects another mode, profile or relay", async () => {
  const command = directIpv4Fixtures(sessionFragmentedProfile, "host"); await render(); await chooseProfile(sessionFragmentedProfile);
  await chooseMode("directIpv4"); await editInput(labeledInput("Endereço inicial"), "192.168.1.2:42000");
  await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002"); await submit(); await submit(); await checkConsent();
  handlers.set("connect_confirm", () => Promise.reject("connect_transport_failed")); await click("Autorizar conexão");
  assert.equal(button("Autorizar conexão").disabled, true);
  await click("Atualizar estado");
  assert.equal(count(command), 1); assert.equal(count("connect_confirm"), 1);
  assert.equal(calls.filter(call => call.command.startsWith("connect_prepare_")).length, 1);
  assert.match(container.textContent, /ModoIPv4 direto autorizado/);
  assert.match(container.textContent, /Não confirmado · relay desligado/);
});

test("direct IPv4 schema3 discovery still sends only the selected TCP authority and a manual UDP destination", async () => {
  discoveredSessionFragmentedFixtures(); capabilities.modes = [...caps.modes, "directIpv4"];
  await render(); await chooseProfile(sessionFragmentedProfile); await chooseMode("directIpv4");
  await editInput(labeledInput("Endereço inicial"), "192.168.1.2:42000"); await discoveryToReview();
  await click("Autorizar consulta e selecionar porta"); await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002"); await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_prepare_session_fragmented_host_discovered").args,
    { selectionId: discoverySelected.selectionId, request: { mode: "directIpv4", bindAddress: "192.168.1.2:42000", udpTarget: "127.0.0.1:43002", admissionSeconds: 600, sessionSeconds: 3600 } });
  assert.equal(count("connect_prepare_session_fragmented_host"), 0);
});

test("direct IPv4 review parsers reject inconsistent initial addresses across all profiles and preserve opaque invitation bytes", () => {
  for (const base of [review, sessionReview, fragmentedReview, reviewedSessionFragmented().review]) {
    const valid = { ...base, mode: "directIpv4", directAddress: "192.168.1.2:42000" };
    assert.deepEqual(types.reviewFrom(valid), valid);
    for (const directAddress of ["[::1]:42000", "127.0.0.1:42000", "0.0.0.0:42000", "localhost:42000", "192.168.1.2:0", "192.0.2.1:42000"]) {
      assert.throws(() => types.reviewFrom({ ...valid, directAddress }));
    }
    assert.throws(() => types.reviewFrom({ ...valid, mode: "automaticIpv4" }));
  }
  const opaque = { ...sessionFragmentedExport, invitation: '{"mode":"externalIpv6","directAddress":"arbitrary opaque input"}' };
  assert.equal(types.exportFrom(opaque).invitation, opaque.invitation, "Projection parsing never selects transport by inspecting encoded invitation data");
});

const groupTypes = await import(await sourceModule(new URL("./group-types.ts", import.meta.url)));
const groupOperation = "aaaaaaaa-1111-4111-8111-111111111111";
const groupKind = "staticSessionFragmentedHost";
const groupCapability = { profile: sessionFragmentedProfile, invitationSchemaVersion: 3, minParticipants: 2, maxParticipants: 8,
  maxPreparationSeconds: 600, maxReviewSeconds: 60, maxAdmissionSeconds: 600, maxSessionSeconds: 3600, ownedGameTcpDiscovery: false };
const pendingDiscoveredGroup = () => ({ ...idle(), operationId: groupOperation, generation: 1, role: "host", phase: "hostPreparing",
  expiresAtMs: 1800000600000, cleanupState: "pending", pendingGroupSource: { kind: groupKind, mode: "loopback" } });
function preparedDiscoveredGroup() {
  const value = preparedGroup();
  return { ...value, generation: 2, cleanupState: "pending", hostGroup: { ...value.hostGroup, preparation: { ...value.hostGroup.preparation, generation: 2 } } };
}
function reviewedDiscoveredGroup() {
  const value = reviewedGroup();
  return { ...value, generation: 3, cleanupState: "pending", hostGroup: { ...value.hostGroup, review: { ...value.hostGroup.review, generation: 3,
    targetSource: "ownedInstance", instanceId: discoverySelected.instanceId, runId: discoverySelected.runId, gameTarget: discoverySelected.target } } };
}
function discoveredGroupFixtures() {
  groupFixtures(); discoveryFixtures();
  capabilities.sessionFragmentedHostGroups = { ...groupCapability, ownedGameTcpDiscovery: true };
  handlers.set("connect_prepare_session_group_host_discovered", () => { snapshot = pendingDiscoveredGroup(); return structuredClone(snapshot); });
  handlers.set("connect_review_session_group_host", () => { snapshot = reviewedDiscoveredGroup(); return snapshot.hostGroup.review; });
  handlers.set("connect_confirm_session_group_host", () => {
    snapshot = { ...reviewedDiscoveredGroup(), generation: 4, phase: "hostListening", transportState: "listening", expiresAtMs: null,
      hostGroup: { ...reviewedDiscoveredGroup().hostGroup, review: null } };
    return structuredClone(snapshot);
  });
  const stopGroup = handlers.get("connect_stop");
  handlers.set("connect_stop", () => {
    if (snapshot.hostGroup) return stopGroup();
    const { pendingGroupSource: _pending, ...previous } = snapshot;
    snapshot = { ...previous, generation: previous.generation + 1, phase: "closed", transportState: "closed", expiresAtMs: null, cleanupState: "complete" };
    return structuredClone(snapshot);
  });
}
async function selectDiscoveredGroup() {
  await render(); await chooseProfile("sessionGroup"); await discoveryToReview();
  await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002");
  await click("Autorizar consulta e selecionar porta");
}
const groupParticipants = () => ["b", "e"].map((letter, index) => ({ participantId: `${letter.repeat(8)}-2222-4222-8222-222222222222`,
  guestPublicId: letter.repeat(64), invitationDigest: (index === 0 ? "d" : "f").repeat(64),
  sessionTiming: { profile: sessionFragmentedProfile, schemaVersion: 3, admissionSeconds: 600 - index * 100, sessionSeconds: 3600 - index * 500,
    admissionExpiresAtMs: 1800000000000 + (600 - index * 100) * 1000, sessionExpiresAtMs: 1800000000000 + (3600 - index * 500) * 1000 },
  consumed: false, transportActive: false, revoked: false, admissionValid: true, sessionValid: true }));
function preparedGroup() {
  return { ...idle(), operationId: groupOperation, generation: 1, role: "host", phase: "hostPrepared", expiresAtMs: 1800000600000,
    hostGroup: { kind: groupKind, profile: sessionFragmentedProfile, invitationSchemaVersion: 3, mode: "loopback", hostPublicId: hostId,
      preparation: { operationId: groupOperation, generation: 1, publicId: hostId, expiresAtMs: 1800000600000, kind: groupKind }, review: null, participants: [] } };
}
function reviewedGroup() {
  const value = preparedGroup(), participants = groupParticipants();
  return { ...value, phase: "reviewed", expiresAtMs: 1800000060000, hostGroup: { ...value.hostGroup, preparation: null, participants,
    review: { operationId: groupOperation, generation: 1, digest, kind: groupKind, mode: "loopback", hostPublicId: hostId,
      directAddress: "127.0.0.1:42000", gameTarget: "127.0.0.1:43001", targetSource: "manual",
      fragmentedUdp: { ...sessionFragmentedContract, udpTarget: "127.0.0.1:43002" }, participants: structuredClone(participants), expiresAtMs: 1800000060000 } } };
}
function activeGroup() {
  const value = reviewedGroup();
  return { ...value, phase: "hostListening", transportState: "listening", expiresAtMs: null, hostGroup: { ...value.hostGroup, review: null } };
}
function groupFixtures() {
  capabilities = { ...caps, sessionFragmentedUdpInvitations: sessionFragmentedCapability, sessionFragmentedHostGroups: groupCapability };
  handlers.set("connect_prepare_session_group_host", () => { snapshot = preparedGroup(); return snapshot.hostGroup.preparation; });
  handlers.set("connect_review_session_group_host", () => { snapshot = reviewedGroup(); return snapshot.hostGroup.review; });
  handlers.set("connect_confirm_session_group_host", () => { snapshot = activeGroup(); return structuredClone(snapshot); });
  handlers.set("connect_export_session_group_invitation", ({ participantId }) => {
    const row = snapshot.hostGroup.participants.find(item => item.participantId === participantId); assert.ok(row);
    return { operationId: groupOperation, participantId, invitationDigest: row.invitationDigest, invitation: `opaque-${participantId}`,
      hostPublicId: hostId, expiresAtMs: row.sessionTiming.admissionExpiresAtMs, sessionTiming: structuredClone(row.sessionTiming), fragmentedUdp: sessionFragmentedContract };
  });
  handlers.set("connect_revoke_session_group_participant", ({ operationId, participantId, expectedGeneration }) => {
    assert.equal(operationId, groupOperation); assert.equal(expectedGeneration, snapshot.generation);
    snapshot = { ...snapshot, generation: snapshot.generation + 1, hostGroup: { ...snapshot.hostGroup,
      participants: snapshot.hostGroup.participants.map(row => row.participantId === participantId
        ? { ...withoutPathObservation(row), revoked: true, admissionValid: false, sessionValid: false, transportActive: false } : row) } };
    return structuredClone(snapshot);
  });
  handlers.set("connect_stop", () => {
    snapshot = { ...snapshot, phase: "closed", transportState: "closed", expiresAtMs: null, cleanupState: "complete",
      hostGroup: { ...snapshot.hostGroup, preparation: null, review: null,
        participants: snapshot.hostGroup.participants.map(row => ({ ...withoutPathObservation(row), transportActive: false, admissionValid: false, sessionValid: false })) } };
    return structuredClone(snapshot);
  });
}
async function fillGroup() {
  await editInput(labeledInput("Código público do participante 1"), "b".repeat(64));
  await editInput(labeledInput("Código público do participante 2"), "e".repeat(64));
  await editInput(labeledInput("Prazo para entrar do participante 2"), "500");
  await editInput(labeledInput("Duração da sessão do participante 2"), "3100");
}

test("group capability is explicit, optional and never changes older host/guest choices", async () => {
  sessionFragmentedFixtures(); await render(); assert.equal(container.querySelector('option[value="sessionGroup"]'), null);
  await chooseProfile(sessionFragmentedProfile); await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002"); await submit();
  assert.equal(count("connect_prepare_session_group_host"), 0); assert.equal(count("connect_prepare_session_fragmented_host"), 1);
  assert.throws(() => types.capabilitiesFrom({ ...caps, sessionFragmentedHostGroups: groupCapability }));
  for (const changed of [{ maxParticipants: 9 }, { minParticipants: 1 }, { ownedGameTcpDiscovery: true }, { maxReviewSeconds: 600 }, { profile: "sessionGameTcp" }, { surprise: true }]) {
    assert.throws(() => types.capabilitiesFrom({ ...caps, sessionFragmentedUdpInvitations: sessionFragmentedCapability, sessionFragmentedHostGroups: { ...groupCapability, ...changed } }));
  }
});

test("discovered group capability requires native discovery and keeps the manual capability shape", () => {
  for (const ownedGameTcpDiscovery of [false, true]) {
    const value = { ...caps, ownedInstanceDiscovery: true, sessionFragmentedUdpInvitations: sessionFragmentedCapability,
      sessionFragmentedHostGroups: { ...groupCapability, ownedGameTcpDiscovery } };
    assert.equal(types.capabilitiesFrom(value).sessionFragmentedHostGroups.ownedGameTcpDiscovery, ownedGameTcpDiscovery);
  }
  for (const ownedInstanceDiscovery of [undefined, false]) assert.throws(() => types.capabilitiesFrom({ ...caps, ownedInstanceDiscovery,
    sessionFragmentedUdpInvitations: sessionFragmentedCapability, sessionFragmentedHostGroups: { ...groupCapability, ownedGameTcpDiscovery: true } }));
  for (const ownedGameTcpDiscovery of [undefined, null, "true", 1]) assert.throws(() => groupTypes.groupCapabilityFrom({ ...groupCapability, ownedGameTcpDiscovery }));
});

test("discovered group reserves an owned pending operation before showing any peer or roster", async () => {
  discoveredGroupFixtures(); await selectDiscoveredGroup();
  assert.match(container.textContent, /destino UDP abaixo continua manual/);
  await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_prepare_session_group_host_discovered").args,
    { selectionId: discoverySelected.selectionId, request: { mode: "loopback", bindAddress: "127.0.0.1:42000", udpTarget: "127.0.0.1:43002" } });
  assert.equal(count("connect_prepare_session_group_host"), 0);
  assert.equal(count("connect_prepare_session_fragmented_host_discovered"), 0);
  assert.match(container.textContent, /Conferindo a origem do grupo/);
  assert.equal(container.querySelector("textarea"), null);
  assert.equal(container.querySelector("form"), null);
  assert.equal(button("Encerrar Connect").disabled, false);
  snapshot = preparedDiscoveredGroup(); await click("Atualizar estado");
  assert.equal(container.querySelectorAll("fieldset").length, 2);
  await fillGroup(); await submit();
  assert.match(container.textContent, /Jogo TCP · origem conferida/);
  assert.match(container.textContent, /Destino manual, autorizado separadamente/);
  assert.match(container.textContent, /grupo inteiro encerra/);
  assert.equal(button("Autorizar grupo").disabled, true);
  await checkConsent(); await click("Autorizar grupo");
  assert.deepEqual(calls.find(call => call.command === "connect_confirm_session_group_host").args, { operationId: groupOperation, digest });
  assert.equal(count("connect_probe_status"), 0);
});

test("discovered group omits an obsolete manual TCP target and still validates UDP separately", async () => {
  discoveredGroupFixtures(); await render(); await chooseProfile("sessionGroup");
  await editInput(labeledInput("Servidor neste"), "192.168.1.10:43001");
  await discoveryToReview(); await click("Autorizar consulta e selecionar porta");
  await editInput(labeledInput("Serviço UDP"), "192.168.1.10:43002"); await submit();
  assert.equal(count("connect_prepare_session_group_host_discovered"), 0);
  await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002"); await submit();
  const args = calls.find(call => call.command === "connect_prepare_session_group_host_discovered").args;
  assert.equal(Object.hasOwn(args.request, "gameTarget"), false);
  assert.equal(Object.hasOwn(args, "instanceId"), false);
  assert.equal(Object.hasOwn(args, "runId"), false);
  assert.equal(count("connect_prepare_session_group_host"), 0);
});

test("discovered group refuses a locally expired selection at the click without consuming another token", async () => {
  discoveredGroupFixtures(); await selectDiscoveredGroup();
  const now = Date.now;
  try { Date.now = () => discoverySelected.expiresAtMs; await submit(); }
  finally { Date.now = now; }
  assert.equal(count("connect_prepare_session_group_host_discovered"), 0);
  assert.match(container.textContent, /seleção de porta expirou/);
  assert.equal(button("Preparar anfitrião").disabled, true);
});

test("lost discovered group preparation recovers its owned reservation by snapshot without resending", async () => {
  discoveredGroupFixtures(); await selectDiscoveredGroup();
  handlers.set("connect_prepare_session_group_host_discovered", () => { snapshot = pendingDiscoveredGroup(); throw new Error("lost response"); });
  await submit();
  assert.match(container.textContent, /resultado do preparo ainda não foi confirmado/);
  assert.equal(container.querySelector("form"), null);
  await click("Atualizar estado");
  assert.match(container.textContent, /Conferindo a origem do grupo/);
  await click("Encerrar Connect");
  assert.match(container.textContent, /Recursos liberados/);
  assert.equal(count("connect_prepare_session_group_host_discovered"), 1);
  assert.equal(count("connect_prepare_session_group_host"), 0);
  assert.equal(button("Preparar anfitrião").disabled, true);
});

test("a pending initial IPC can be observed and stopped, and its late reply cannot restore preparation", async () => {
  discoveredGroupFixtures(); await selectDiscoveredGroup();
  let complete;
  handlers.set("connect_prepare_session_group_host_discovered", () => { snapshot = pendingDiscoveredGroup(); return new Promise(resolve => { complete = resolve; }); });
  await submit();
  assert.equal(button("Atualizar estado").disabled, false);
  await click("Atualizar estado"); await click("Encerrar Connect");
  await act(async () => complete(pendingDiscoveredGroup()));
  assert.match(container.textContent, /Conexão encerrada/);
  assert.doesNotMatch(container.textContent, /Conferindo a origem do grupo/);
  assert.equal(count("connect_prepare_session_group_host_discovered"), 1);
  assert.equal(container.querySelector("fieldset"), null);
});

test("old idle snapshots do not clear uncertainty after an unstructured discovered preparation failure", async () => {
  discoveredGroupFixtures(); await selectDiscoveredGroup();
  handlers.set("connect_prepare_session_group_host_discovered", () => { throw new Error("unknown delivery"); });
  await submit(); await click("Atualizar estado"); await click("Atualizar estado");
  assert.match(container.textContent, /resultado do preparo ainda não foi confirmado/);
  assert.equal(container.querySelector("form"), null);
  assert.equal(count("connect_prepare_session_group_host_discovered"), 1);
  snapshot = pendingDiscoveredGroup(); await click("Atualizar estado");
  assert.equal(button("Encerrar Connect").disabled, false);
});

test("controlled pre-reservation rejection requires a new discovery selection without trapping an uncreated operation", async () => {
  discoveredGroupFixtures(); await selectDiscoveredGroup();
  handlers.set("connect_prepare_session_group_host_discovered", () => { throw "connect_invalid_request"; });
  await submit();
  assert.doesNotMatch(container.textContent, /resultado do preparo ainda não foi confirmado/);
  assert.equal(button("Preparar anfitrião").disabled, true);
  await submit(); assert.equal(count("connect_prepare_session_group_host_discovered"), 1);
});

test("failed source preparation keeps cleanup pending without a fabricated host identity", async () => {
  discoveredGroupFixtures(); await selectDiscoveredGroup(); await submit();
  const { pendingGroupSource: _pending, ...reserved } = pendingDiscoveredGroup();
  snapshot = { ...reserved, generation: 2, phase: "failed", transportState: "closed", expiresAtMs: null, errorCode: "connect_source_changed" };
  await click("Atualizar estado");
  assert.match(container.textContent, /Aguardando liberação dos recursos/);
  assert.equal(container.querySelector("form"), null);
  assert.equal(container.querySelector("textarea"), null);
  await click("Encerrar Connect");
  assert.match(container.textContent, /Recursos liberados/);
  assert.equal(button("Preparar anfitrião").disabled, true);
});

test("pending discovered preparation remains observable and stoppable when capabilities cannot be read", async () => {
  discoveredGroupFixtures(); snapshot = pendingDiscoveredGroup();
  handlers.set("connect_capabilities", () => { throw new Error("fixture unavailable"); });
  await render();
  assert.match(container.textContent, /Conferindo a origem do grupo/);
  assert.equal(container.querySelector("form"), null);
  await click("Encerrar Connect");
  assert.match(container.textContent, /Recursos liberados/);
  assert.equal(count("connect_prepare_session_group_host_discovered"), 0);
});

test("pending group and source review parsers reject invented identities and contradictory phases", () => {
  const pending = pendingDiscoveredGroup();
  assert.equal(types.snapshotFrom(pending).phase, "hostPreparing");
  const { pendingGroupSource: _pending, ...without } = pending;
  assert.throws(() => types.snapshotFrom(without));
  for (const patch of [
    { generation: 0 }, { operationId: "not-uuid" }, { role: "guest" }, { available: false }, { phase: "hostPrepared" }, { phase: "closing" },
    { cleanupState: "complete" }, { errorCode: "connect_internal" }, { expiresAtMs: null }, { transportState: "listening" },
    { hostGroup: preparedGroup().hostGroup }, { preparation }, { review }, { publicId: hostId }, { hostPublicId: hostId },
    { invitationProfile: sessionFragmentedProfile }, { fragmentedUdp: sessionFragmentedContract }, { udpTarget: "127.0.0.1:43002" },
    { pendingGroupSource: { kind: groupKind, mode: "invalid" } }, { pendingGroupSource: { kind: groupKind, mode: "loopback", peer: hostId } },
  ]) assert.throws(() => types.snapshotFrom({ ...pending, ...patch }));
  const owned = reviewedDiscoveredGroup().hostGroup.review;
  assert.equal(groupTypes.groupReviewFrom(owned).runId, discoverySelected.runId);
  for (const patch of [
    { instanceId: undefined }, { runId: null }, { instanceId: "" }, { runId: "bad\nrun" }, { targetSource: "manual" },
    { targetSource: "unknown" }, { processId: 123 }, { fingerprint: "private" },
  ]) assert.throws(() => groupTypes.groupReviewFrom({ ...owned, ...patch }));
  assert.throws(() => groupTypes.groupReviewFrom({ ...reviewedGroup().hostGroup.review, instanceId: null, runId: null }));
  assert.throws(() => types.snapshotFrom({ ...reviewedGroup(), pendingGroupSource: pending.pendingGroupSource }));
  assert.throws(() => types.preparationFrom({ ...preparation, pendingGroupSource: pending.pendingGroupSource }));
});

test("a changed discovered instance or run clears group consent even if digest and generation were not changed", async () => {
  discoveredGroupFixtures(); snapshot = reviewedDiscoveredGroup(); await render();
  await checkConsent(); assert.equal(button("Autorizar grupo").disabled, false);
  snapshot = { ...snapshot, hostGroup: { ...snapshot.hostGroup, review: { ...snapshot.hostGroup.review, runId: "another-fixture-run" } } };
  await click("Atualizar estado");
  assert.equal(button("Autorizar grupo").disabled, true);
  await checkConsent();
  snapshot = { ...snapshot, hostGroup: { ...snapshot.hostGroup, review: { ...snapshot.hostGroup.review, instanceId: "another-fixture-instance" } } };
  await click("Atualizar estado");
  assert.equal(button("Autorizar grupo").disabled, true);
  assert.equal(count("connect_confirm_session_group_host"), 0);
});

test("changing profile while discovered STATUS is pending cannot populate a later group selection", async () => {
  discoveredGroupFixtures(); await render(); await chooseProfile("sessionGroup"); await discoveryToReview();
  let complete;
  handlers.set("connect_confirm_discovered_status", () => new Promise(resolve => { complete = resolve; }));
  await click("Autorizar consulta e selecionar porta");
  await chooseProfile("legacy"); await chooseProfile("sessionGroup"); await chooseManaged();
  await act(async () => complete(discoverySelected));
  assert.equal(button("Preparar anfitrião").disabled, true);
  assert.doesNotMatch(container.textContent, /Servidor respondeu em/);
  assert.equal(count("connect_prepare_session_group_host_discovered"), 0);
});

test("discovered group uses only explicitly selected IPv4 and preserves manual group preparation", async () => {
  discoveredGroupFixtures(); capabilities.modes = [...caps.modes, "directIpv4"];
  await selectDiscoveredGroup(); await chooseMode("directIpv4");
  await submit(); assert.equal(count("connect_prepare_session_group_host_discovered"), 0);
  await editInput(labeledInput("Endereço inicial"), "192.168.1.2:42000");
  handlers.set("connect_prepare_session_group_host_discovered", () => {
    snapshot = { ...pendingDiscoveredGroup(), pendingGroupSource: { kind: groupKind, mode: "directIpv4" } }; return structuredClone(snapshot);
  });
  await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_prepare_session_group_host_discovered").args,
    { selectionId: discoverySelected.selectionId, request: { mode: "directIpv4", bindAddress: "192.168.1.2:42000", udpTarget: "127.0.0.1:43002" } });
  assert.equal(count("connect_prepare_session_group_host"), 0);
});

test("group host performs separate prepare, two-person review, consent and individual invitation export", async () => {
  groupFixtures(); await render(); await chooseProfile("sessionGroup");
  assert.equal([...container.querySelectorAll("label")].some(label => label.textContent.startsWith("Selecionar servidor")), false);
  await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002"); await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_prepare_session_group_host").args,
    { request: { mode: "loopback", bindAddress: "127.0.0.1:42000", gameTarget: "127.0.0.1:43001", udpTarget: "127.0.0.1:43002" } });
  assert.equal(count("connect_review_host"), 0); assert.equal(container.querySelectorAll("fieldset").length, 2);
  await fillGroup(); await submit();
  assert.deepEqual(calls.find(call => call.command === "connect_review_session_group_host").args, { operationId: groupOperation, participants: [
    { guestPublicId: "b".repeat(64), admissionSeconds: 600, sessionSeconds: 3600 },
    { guestPublicId: "e".repeat(64), admissionSeconds: 500, sessionSeconds: 3100 } ] });
  assert.equal(button("Autorizar grupo").disabled, true); assert.equal(count("connect_confirm"), 0);
  await checkConsent(); await click("Autorizar grupo");
  assert.deepEqual(calls.find(call => call.command === "connect_confirm_session_group_host").args, { operationId: groupOperation, digest });
  assert.equal(button("Copiar convite selecionado").disabled, true);
  await click("Selecionar convite 2"); await click("Copiar convite selecionado");
  assert.deepEqual(calls.find(call => call.command === "connect_export_session_group_invitation").args, { operationId: groupOperation, participantId: groupParticipants()[1].participantId });
  assert.deepEqual(copied, [`opaque-${groupParticipants()[1].participantId}`]);
  assert.match(container.textContent, /Conexão autenticada não confirma entrada no Minecraft ou áudio/);
  assert.equal(count("connect_export_invitation"), 0); assert.equal(count("connect_probe_status"), 0);
});

test("group choice remains host-only and requires manually entered reachable IPv4 and loopback targets", async () => {
  groupFixtures(); capabilities.modes.push("directIpv4"); await render(); await chooseProfile("sessionGroup");
  await chooseMode("directIpv4"); assert.equal(labeledInput("Endereço inicial").value, "");
  await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002"); await submit(); assert.equal(count("connect_prepare_session_group_host"), 0);
  await editInput(labeledInput("Endereço inicial"), "192.168.1.2:42000"); await editInput(labeledInput("Servidor neste"), "192.168.1.2:43001");
  await submit(); assert.equal(count("connect_prepare_session_group_host"), 0);
  await click("Convidado"); assert.equal(container.querySelector('option[value="sessionGroup"]'), null);
  assert.equal(container.querySelector('option[value="legacy"]').selected, true);
});

test("group refuses duplicate peers and invalid individual durations before review IPC; list stays within eight", async () => {
  groupFixtures(); snapshot = preparedGroup(); await render(); await fillGroup();
  await editInput(labeledInput("Código público do participante 2"), "b".repeat(64)); await submit();
  assert.equal(count("connect_review_session_group_host"), 0);
  await editInput(labeledInput("Código público do participante 2"), "e".repeat(64));
  for (const value of ["0", "601", "1.5"]) { await editInput(labeledInput("Prazo para entrar do participante 2"), value); await submit(); }
  assert.equal(count("connect_review_session_group_host"), 0);
  for (let i = 0; i < 6; i++) await click("Adicionar participante");
  assert.equal(container.querySelectorAll("fieldset").length, 8); assert.equal(button("Adicionar participante").disabled, true);
  await click("Remover participante 8"); assert.equal(container.querySelectorAll("fieldset").length, 7);
});

test("group snapshots preserve cleanup without capabilities and reject mixed or inconsistent single-peer projections", async () => {
  const valid = activeGroup(); assert.deepEqual(types.snapshotFrom(valid), valid);
  for (const mutation of [s => { s.role = "guest"; }, s => { s.phase = "guestConnected"; }, s => { s.invitationProfile = sessionFragmentedProfile; },
    s => { s.sessionTiming = groupParticipants()[0].sessionTiming; }, s => { s.review = review; }, s => { s.preparation = preparation; },
    s => { s.udpRecovery = { attemptsUsed: 0, maxAttempts: 3, pending: false }; }, s => { s.tcpAddress = "127.0.0.1:42010"; },
    s => { s.expiresAtMs = groupParticipants()[0].sessionTiming.sessionExpiresAtMs; }, s => { s.hostGroup.participants[1].participantId = s.hostGroup.participants[0].participantId; },
    s => { s.hostGroup.participants[1].guestPublicId = s.hostGroup.participants[0].guestPublicId; }, s => { s.hostGroup.participants[0].sessionTiming.schemaVersion = 2; },
    s => { s.hostGroup.participants[0].transportActive = true; }, s => { s.hostGroup.participants[0].revoked = true; }]) {
    const broken = structuredClone(valid); mutation(broken); assert.throws(() => types.snapshotFrom(broken));
  }
  groupFixtures(); delete capabilities.sessionFragmentedHostGroups; snapshot = valid; await render();
  assert.match(container.textContent, /ações de grupo não estão disponíveis/);
  assert.equal(button("Selecionar convite 1").disabled, true); await click("Encerrar Connect");
  assert.equal(count("connect_stop"), 1); assert.match(container.textContent, /Recursos liberados/);
});

test("group review binds all immutable participant data and cannot enter older review/export/preparation parsers", () => {
  const value = reviewedGroup(); assert.deepEqual(groupTypes.groupReviewFrom(value.hostGroup.review), value.hostGroup.review);
  assert.throws(() => types.preparationFrom(preparedGroup().hostGroup.preparation));
  assert.throws(() => types.reviewFrom(value.hostGroup.review));
  for (const mutate of [s => { s.hostGroup.review.operationId = groupParticipants()[0].participantId; }, s => { s.hostGroup.review.participants.reverse(); },
    s => { s.hostGroup.review.mode = "directIpv4"; }, s => { s.hostGroup.review.participants[0].transportActive = true; },
    s => { s.hostGroup.review.fragmentedUdp.udpTarget = "192.168.0.2:1234"; }, s => { s.hostGroup.review.participants[0].sessionTiming.profile = "sessionGameTcp"; }]) {
    const broken = structuredClone(value); mutate(broken); assert.throws(() => types.snapshotFrom(broken));
  }
});

test("group reviewed snapshot cannot disagree with the pristine reviewed participant flags", () => {
  for (const change of [{ consumed: true }, { revoked: true, admissionValid: false, sessionValid: false },
    { admissionValid: false }, { admissionValid: false, sessionValid: false }, { consumed: true, transportActive: true }]) {
    const value = reviewedGroup(); Object.assign(value.hostGroup.participants[0], change);
    assert.throws(() => types.snapshotFrom(value), JSON.stringify(change));
  }
});

test("group external IPv6 review requires a canonical literal, valid port and native address policy", () => {
  for (const address of ["[2001:4860::1]:80", "[2001:4860::1]:65535", "[3000:1234:abcd::7]:42000"]) {
    const value = reviewedGroup().hostGroup.review; value.mode = "externalIpv6"; value.directAddress = address;
    assert.deepEqual(groupTypes.groupReviewFrom(value), value);
  }
  for (const address of ["[2]:99999", "[2001:4860::1]:65536", "[2001:4860::1]:0", "[2001:4860::1]:042000",
    "[2001:4860:::1]:42000", "[2001:4860:1:2:3:4:5:6:7]:42000", "[2001:4860::1%3]:42000", "[::1]:42000",
    "[2001:db8::1]:42000", "[2001:1::1]:42000", "[2002::1]:42000", "[2620:4f:8000::1]:42000", "[3fff::1]:42000",
    "[2001:4860::5efe:1:2]:42000", "[2001:4860:1:2:200:5efe:1:2]:42000", "[fe80::1]:42000",
    "[ff02::1]:42000", "[2001:4860:0:0:0:0:0:1]:42000", "[2001:4860::1]:42000/path", "host.example:42000"]) {
    const value = reviewedGroup().hostGroup.review; value.mode = "externalIpv6"; value.directAddress = address;
    assert.throws(() => groupTypes.groupReviewFrom(value), address);
  }
});

test("group remount still observes and awaits stop when the capability IPC fails", async () => {
  groupFixtures(); snapshot = activeGroup();
  handlers.set("connect_capabilities", () => { throw new Error("capability unavailable"); });
  const finishStop = handlers.get("connect_stop"); let completeStop;
  handlers.set("connect_stop", () => new Promise(resolve => { completeStop = () => resolve(finishStop()); }));
  await render();
  assert.equal(count("connect_snapshot"), 1); assert.match(container.textContent, /ações de grupo não estão disponíveis/);
  assert.equal(button("Selecionar convite 1").disabled, true); assert.equal(button("Revogar participante 1").disabled, true);
  assert.equal(button("Copiar convite selecionado").disabled, true);
  await click("Encerrar Connect"); assert.equal(button("Encerrar Connect").disabled, true);
  assert.doesNotMatch(container.textContent, /Recursos liberados/);
  await act(async () => completeStop()); assert.match(container.textContent, /Recursos liberados/);
  assert.equal(container.querySelector("form"), null); assert.deepEqual(copied, []);
  assert.deepEqual(calls.map(call => call.command), ["connect_capabilities", "connect_snapshot", "connect_stop"]);
});

test("group failed capability keeps prepared and reviewed actions disabled while allowing cleanup", async () => {
  groupFixtures(); snapshot = preparedGroup();
  handlers.set("connect_capabilities", () => { throw new Error("capability unavailable"); });
  await render(); assert.equal(button("Copiar código do grupo").disabled, true);
  assert.equal(button("Revisar grupo completo").disabled, true);
  snapshot = reviewedGroup(); await click("Atualizar estado");
  assert.equal(button("Autorizar grupo").disabled, true); assert.equal(container.querySelector('.connect-consent input').disabled, true);
  await click("Encerrar Connect"); assert.equal(count("connect_stop"), 1); assert.deepEqual(copied, []);
  assert.equal(count("connect_confirm_session_group_host"), 0); assert.equal(count("connect_review_session_group_host"), 0);
});

test("group failed cleanup preserves uncertain transport and stop without claiming resources released", async () => {
  groupFixtures(); const value = activeGroup();
  snapshot = { ...value, phase: "failed", cleanupState: "failed", errorCode: "connect_internal",
    hostGroup: { ...value.hostGroup, participants: value.hostGroup.participants.map(row => ({ ...row, admissionValid: false, sessionValid: false })) } };
  for (const transportState of ["notStarted", "listening", "closed"]) {
    const acceptable = { ...snapshot, transportState }; assert.deepEqual(types.snapshotFrom(acceptable), acceptable);
  }
  for (const changed of [{ transportState: "admitted" }, { transportState: "authenticated" }, { cleanupState: "notStarted" },
    { cleanupState: "pending" }, { cleanupState: "complete" }, { phase: "closed" }]) {
    assert.throws(() => types.snapshotFrom({ ...snapshot, ...changed }));
  }
  const invalidFlags = structuredClone(snapshot); invalidFlags.hostGroup.participants[0].sessionValid = true;
  assert.throws(() => types.snapshotFrom(invalidFlags));
  await render(); assert.match(container.textContent, /transporte exibido é a última observação/);
  assert.doesNotMatch(container.textContent, /Recursos liberados|Conexão autenticada observada/);
  assert.equal(container.querySelector("form"), null); await click("Encerrar Connect");
  assert.match(container.textContent, /Recursos liberados/); assert.equal(count("connect_stop"), 1);
});

test("failed discovered group remains cancellable while its source worker cleanup is pending", async () => {
  discoveredGroupFixtures(); const value = preparedDiscoveredGroup();
  snapshot = { ...value, phase: "failed", transportState: "closed", expiresAtMs: null, cleanupState: "pending", errorCode: "connect_source_changed",
    hostGroup: { ...value.hostGroup, preparation: null } };
  assert.deepEqual(types.snapshotFrom(snapshot), snapshot);
  for (const transportState of ["notStarted", "listening", "admitted"]) {
    assert.throws(() => types.snapshotFrom({ ...snapshot, transportState }));
  }
  await render();
  assert.match(container.textContent, /Aguardando liberação dos recursos/);
  assert.doesNotMatch(container.textContent, /Recursos liberados/);
  assert.equal(container.querySelector("form"), null);
  assert.equal(container.querySelector("textarea"), null);
  await click("Encerrar Connect");
  assert.match(container.textContent, /Recursos liberados/);
  assert.equal(count("connect_stop"), 1);
  assert.equal(count("connect_prepare_session_group_host_discovered"), 0);
});

test("group revocation affects selected participant only and never calls whole-group stop", async () => {
  groupFixtures(); snapshot = activeGroup(); snapshot.hostGroup.participants[0].consumed = true; snapshot.hostGroup.participants[0].transportActive = true;
  await render(); assert.match(container.textContent, /Conexão autenticada observada/);
  assert.equal(button("Selecionar convite 1").disabled, true); await click("Revogar participante 1");
  assert.equal(snapshot.hostGroup.participants[0].revoked, true); assert.equal(snapshot.hostGroup.participants[1].revoked, false);
  assert.equal(button("Selecionar convite 2").disabled, false); assert.equal(count("connect_stop"), 0);
  assert.equal(count("connect_revoke_session_group_participant"), 1);
  await click("Selecionar convite 2"); await click("Copiar convite selecionado"); assert.equal(copied.length, 1);
});

test("group individual expiry or consumption hides that invitation without stopping another participant", async () => {
  groupFixtures(); snapshot = activeGroup(); await render(); await click("Selecionar convite 1"); await click("Copiar convite selecionado");
  assert.equal([...container.querySelectorAll("textarea")].some(item => item.value.startsWith("opaque-")), true);
  snapshot = structuredClone(snapshot); snapshot.hostGroup.participants[0].consumed = true;
  await click("Atualizar estado"); assert.equal(button("Selecionar convite 1").disabled, true);
  assert.equal([...container.querySelectorAll("textarea")].some(item => item.value.startsWith("opaque-")), false);
  assert.equal(button("Selecionar convite 2").disabled, false); assert.equal(count("connect_stop"), 0);
});

test("group export refuses another participant or mismatched original timing before clipboard access", async () => {
  groupFixtures(); snapshot = activeGroup(); await render(); await click("Selecionar convite 1");
  const handler = handlers.get("connect_export_session_group_invitation");
  handlers.set("connect_export_session_group_invitation", () => handler({ participantId: groupParticipants()[1].participantId }));
  await click("Copiar convite selecionado"); assert.deepEqual(copied, []);
  assert.equal([...container.querySelectorAll("textarea")].some(item => item.value.startsWith("opaque-")), false);
  handlers.set("connect_export_session_group_invitation", args => ({ ...handler(args), expiresAtMs: 1 }));
  await click("Copiar convite selecionado"); assert.deepEqual(copied, []);
});

test("group late export after stop cannot reach clipboard or recreate its invitation", async () => {
  groupFixtures(); snapshot = activeGroup(); await render(); await click("Selecionar convite 1");
  const value = handlers.get("connect_export_session_group_invitation")({ participantId: groupParticipants()[0].participantId });
  let complete; handlers.set("connect_export_session_group_invitation", () => new Promise(resolve => { complete = resolve; }));
  await click("Copiar convite selecionado"); await click("Encerrar Connect"); await act(async () => complete(value));
  assert.deepEqual(copied, []); assert.equal(count("connect_stop"), 1); assert.match(container.textContent, /Recursos liberados/);
});

test("group lost confirmation blocks resubmission until valid observation and reconciles without a second confirm", async () => {
  groupFixtures(); snapshot = reviewedGroup(); await render(); await checkConsent();
  handlers.set("connect_confirm_session_group_host", () => { snapshot = activeGroup(); return Promise.reject(new Error("lost reply")); });
  await click("Autorizar grupo"); assert.equal(count("connect_confirm_session_group_host"), 1);
  assert.match(container.textContent, /Nenhuma solicitação será repetida automaticamente/);
  assert.equal(button("Autorizar grupo").disabled, true);
  handlers.set("connect_snapshot", () => ({ ...activeGroup(), role: "guest" })); await click("Atualizar estado");
  assert.equal(button("Autorizar grupo").disabled, true); assert.equal(count("connect_confirm_session_group_host"), 1);
  handlers.delete("connect_snapshot"); await click("Atualizar estado");
  assert.equal(button("Selecionar convite 1").disabled, false); assert.equal(count("connect_confirm_session_group_host"), 1);
});

test("group lost revocation remains blocked until observation and never repeats against another participant", async () => {
  groupFixtures(); snapshot = activeGroup(); await render(); const revoke = handlers.get("connect_revoke_session_group_participant");
  handlers.set("connect_revoke_session_group_participant", args => { revoke(args); return Promise.reject(new Error("lost reply")); });
  await click("Revogar participante 1"); assert.equal(button("Revogar participante 2").disabled, true);
  assert.equal(count("connect_revoke_session_group_participant"), 1); await click("Atualizar estado");
  assert.equal(button("Revogar participante 1").disabled, true); assert.equal(button("Revogar participante 2").disabled, false);
  assert.equal(count("connect_revoke_session_group_participant"), 1);
});

test("group review expiry at click clears consent even before timer and rollback cannot restore approval", async () => {
  groupFixtures(); snapshot = reviewedGroup(); await render(); await checkConsent();
  const oldNow = Date.now;
  try {
    Date.now = () => snapshot.hostGroup.review.expiresAtMs;
    await click("Autorizar grupo"); assert.equal(count("connect_confirm_session_group_host"), 0);
    Date.now = oldNow; await act(async () => { await new Promise(resolve => setTimeout(resolve, 270)); });
    assert.equal(button("Autorizar grupo").disabled, true);
  } finally { Date.now = oldNow; }
});

test("group late confirmation after stop and remount never restores consent or active resources", async () => {
  groupFixtures(); snapshot = reviewedGroup(); await render(); await checkConsent();
  let complete; handlers.set("connect_confirm_session_group_host", () => new Promise(resolve => { complete = resolve; }));
  await click("Autorizar grupo"); await click("Encerrar Connect"); await act(async () => complete(activeGroup()));
  assert.match(container.textContent, /Recursos liberados/); assert.equal(count("connect_confirm_session_group_host"), 1);
  snapshot = reviewedGroup(); await act(async () => root.unmount()); root = createRoot(container); await render();
  assert.equal(button("Autorizar grupo").disabled, true); assert.equal(count("connect_confirm_session_group_host"), 1);
});

test("group identical observations preserve review consent and exported invitation; changed generation clears both", async () => {
  groupFixtures(); snapshot = reviewedGroup(); await render(); await checkConsent();
  snapshot = structuredClone(snapshot); await click("Atualizar estado"); assert.equal(button("Autorizar grupo").disabled, false);
  snapshot = { ...snapshot, generation: 2, hostGroup: { ...snapshot.hostGroup, review: { ...snapshot.hostGroup.review, generation: 2 } } };
  await click("Atualizar estado"); assert.equal(button("Autorizar grupo").disabled, true);
  snapshot = activeGroup(); snapshot.generation = 3; await click("Atualizar estado");
  await click("Selecionar convite 1"); await click("Copiar convite selecionado");
  snapshot = structuredClone(snapshot); await click("Atualizar estado");
  assert.equal([...container.querySelectorAll("textarea")].some(item => item.value.startsWith("opaque-")), true);
  snapshot = { ...snapshot, generation: 4 }; await click("Atualizar estado");
  assert.equal([...container.querySelectorAll("textarea")].some(item => item.value.startsWith("opaque-")), false);
});

test("group failed snapshot refresh clears invitation and disables actions until a valid observation", async () => {
  groupFixtures(); snapshot = activeGroup(); await render(); await click("Selecionar convite 1"); await click("Copiar convite selecionado");
  handlers.set("connect_snapshot", () => Promise.reject("unavailable")); await click("Atualizar estado");
  assert.equal(button("Selecionar convite 1").disabled, true); assert.equal(button("Revogar participante 2").disabled, true);
  assert.equal([...container.querySelectorAll("textarea")].some(item => item.value.startsWith("opaque-")), false);
  handlers.delete("connect_snapshot"); await click("Atualizar estado"); assert.equal(button("Selecionar convite 1").disabled, false);
  assert.equal(count("connect_export_session_group_invitation"), 1); assert.equal(count("connect_revoke_session_group_participant"), 0);
});

test("group confirmation refuses valid-shaped response that switches another participant authority", async () => {
  groupFixtures(); snapshot = reviewedGroup(); await render(); await checkConsent();
  handlers.set("connect_confirm_session_group_host", () => { const wrong = activeGroup(); wrong.hostGroup.participants[1].guestPublicId = "7".repeat(64); return wrong; });
  await click("Autorizar grupo");
  assert.match(container.textContent, /Aguardando uma observação válida/); assert.equal(button("Autorizar grupo").disabled, true);
  assert.equal(count("connect_confirm_session_group_host"), 1); assert.equal(count("connect_export_session_group_invitation"), 0);
});

const readyGameCapability = { schemaVersion: 1, instanceScope: "diagnosticCopy", maxPlanSeconds: 60, minecraftVersion: "1.21.1",
  loader: "fabric", loaderVersion: "0.16.14", javaMajor: 21, coreVersion: "1.1.5", skinsVersion: "0.3.1" };
test("ready game integration is a separate optional capability and never silently enables launch", async () => {
  assert.deepEqual(types.capabilitiesFrom({ ...caps, readyGameLaunch: readyGameCapability }).readyGameLaunch, readyGameCapability);
  for (const value of [{ ...caps, available: false, readyGameLaunch: readyGameCapability },
    { ...caps, readyGameLaunch: { ...readyGameCapability, schemaVersion: 2 } },
    { ...caps, readyGameLaunch: { ...readyGameCapability, instanceScope: "anyInstance" } }]) assert.throws(() => types.capabilitiesFrom(value));
  snapshot = { ...idle(), operationId: "aaaaaaaa-1111-4111-8111-111111111111", generation: 2, role: "guest", phase: "guestConnected",
    transportState: "authenticated", tcpAddress: "127.0.0.1:42010", cleanupState: "pending" };
  await render(); assert.equal(count("connect_ready_game_launch_snapshot"), 0); assert.doesNotMatch(container.textContent, /Abrir o Minecraft nesta conexão/);
  assert.equal(count("connect_prepare_ready_game_launch"), 0);
});

test("ConnectView forwards ready launch context and exposes missing context instructions without auto-preparing", async () => {
  capabilities.readyGameLaunch = readyGameCapability;
  snapshot = { ...idle(), operationId: "aaaaaaaa-1111-4111-8111-111111111111", generation: 2, role: "guest", phase: "guestConnected",
    transportState: "authenticated", tcpAddress: "127.0.0.1:42010", cleanupState: "pending" };
  handlers.set("connect_ready_game_launch_snapshot", () => ({ operationId: snapshot.operationId, generation: 0, planId: null,
    phase: "idle", review: null, result: null, errorCode: null }));
  handlers.set("diagnostic_lab_list", () => []);
  await render(); assert.match(container.textContent, /Selecione uma instância, um Java disponível/);
  assert.equal(button("Revisar entrada no servidor").disabled, true); assert.equal(count("connect_prepare_ready_game_launch"), 0);
  const launchContext = { sourceInstanceId: "test-source", contextKey: "fixture-context", javaExecutable: "C:\\fixture\\java.exe",
    nickname: "FixturePlayer", appearance: { skinUrl: null, skinFile: null, capeUrl: null, skinModel: "classic" } };
  await act(async () => root.render(createElement(ConnectView, { launchContext })));
  await click("Listar cópias prontas"); assert.deepEqual(calls.find(call => call.command === "diagnostic_lab_list").args, { id: "test-source" });
  assert.equal(count("connect_prepare_ready_game_launch"), 0); assert.equal(count("connect_confirm_ready_game_launch"), 0);
  await click("Encerrar Connect"); assert.equal(count("connect_stop"), 1); assert.equal(count("launch_instance"), 0);
});

const sourceOperationId = "11111111-2222-4333-8444-555555555555";
const sourceProfiles = ["legacy", "sessionGameTcp", "sessionFragmentedGameUdp"];
const sourceCommands = { legacy: "connect_prepare_discovered_host", sessionGameTcp: "connect_prepare_session_host_discovered",
  sessionFragmentedGameUdp: "connect_prepare_session_fragmented_host_discovered" };
function pendingSingleSource(profile) {
  return { ...idle(), operationId: sourceOperationId, generation: 1, role: "host", phase: "hostPreparing", cleanupState: "pending",
    expiresAtMs: preparation.expiresAtMs, pendingHostSource: { kind: "ownedGameTcp", mode: "loopback", profile },
    ...(profile === "legacy" ? {} : { invitationProfile: profile }) };
}
function preparedSingleSource(profile) {
  const { pendingHostSource: _pending, ...base } = pendingSingleSource(profile);
  return { ...base, phase: "hostPrepared", generation: 2, expiresAtMs: null,
    preparation: { ...preparation, operationId: sourceOperationId, generation: 2, ...(profile === "legacy" ? {} : { invitationProfile: profile }) } };
}
function terminalSingleSource(profile, cleanupState = "pending") {
  const { pendingHostSource: _pending, ...base } = pendingSingleSource(profile);
  return { ...base, phase: "failed", generation: 2, transportState: "closed", expiresAtMs: null, cleanupState, errorCode: "connect_source_changed" };
}
function singleSourceFixtures(profile) {
  discoveryFixtures(); sessionFixtures(); sessionFragmentedFixtures();
  capabilities.sessionFragmentedUdpInvitations = { ...sessionFragmentedCapability, ownedGameTcpDiscovery: true };
  handlers.set(sourceCommands[profile], () => { snapshot = preparedSingleSource(profile); return snapshot.preparation; });
  handlers.set("connect_stop", () => {
    const { pendingHostSource: _pending, ...base } = snapshot;
    snapshot = { ...base, generation: snapshot.generation + 1, phase: "closed", transportState: "closed", expiresAtMs: null,
      cleanupState: "complete", preparation: null, review: null };
    return structuredClone(snapshot);
  });
}
async function selectSingleSource(profile) {
  await render();
  if (profile !== "legacy") await chooseProfile(profile);
  await discoveryToReview(); await click("Autorizar consulta e selecionar porta");
  if (profile === sessionFragmentedProfile) await editInput(labeledInput("Serviço UDP"), "127.0.0.1:43002");
}

for (const profile of sourceProfiles) {
  test(`single source ${profile} exposes reservation while original command waits and preserves preparation wire shape`, async () => {
    singleSourceFixtures(profile); await selectSingleSource(profile);
    let complete;
    handlers.set(sourceCommands[profile], () => { snapshot = pendingSingleSource(profile); return new Promise(resolve => { complete = resolve; }); });
    await submit(); assert.equal(button("Atualizar estado").disabled, false);
    assert.equal(container.querySelector("form"), null);
    await click("Atualizar estado");
    assert.match(container.textContent, /Conferindo a origem do jogo/);
    assert.match(container.textContent, /prazo original de 60 segundos/);
    assert.equal(container.querySelector("textarea"), null);
    assert.equal(button("Encerrar Connect").disabled, false);
    const request = calls.find(item => item.command === sourceCommands[profile]).args;
    assert.deepEqual(request, { selectionId: discoverySelected.selectionId, request: { mode: "loopback", bindAddress: "127.0.0.1:42000",
      ...(profile === sessionFragmentedProfile ? { udpTarget: "127.0.0.1:43002" } : { gameTarget: discoverySelected.target }),
      ...(profile === "legacy" ? { lifetimeSeconds: 600 } : { admissionSeconds: 600, sessionSeconds: 3600 }) } });
    snapshot = preparedSingleSource(profile); await click("Atualizar estado");
    assert.equal(container.querySelector('#connect-public-code').value, hostId);
    assert.equal(snapshot.expiresAtMs, null);
    assert.equal(snapshot.preparation.expiresAtMs, pendingSingleSource(profile).expiresAtMs);
    await act(async () => complete(snapshot.preparation));
    assert.equal(count(sourceCommands[profile]), 1); assert.equal(count("connect_confirm"), 0);
  });

  test(`single source ${profile} stops a pending reservation and ignores late preparation`, async () => {
    singleSourceFixtures(profile); await selectSingleSource(profile);
    let complete;
    handlers.set(sourceCommands[profile], () => { snapshot = pendingSingleSource(profile); return new Promise(resolve => { complete = resolve; }); });
    await submit(); await click("Atualizar estado"); await click("Encerrar Connect");
    await act(async () => complete(preparedSingleSource(profile).preparation));
    assert.match(container.textContent, /Recursos liberados/);
    assert.equal(container.querySelector('#connect-public-code'), null);
    assert.equal(button("Preparar anfitrião").disabled, true);
    assert.equal(count(sourceCommands[profile]), 1);
  });

  test(`single source ${profile} reconciles definitive rejection with a new snapshot and requires a fresh selection`, async () => {
    singleSourceFixtures(profile); await selectSingleSource(profile);
    handlers.set(sourceCommands[profile], () => { throw "connect_invalid_request"; });
    const start = calls.length;
    await submit();
    assert.deepEqual(calls.slice(start).map(item => item.command), [sourceCommands[profile], "connect_snapshot"]);
    assert.doesNotMatch(container.textContent, /resultado do preparo ainda não foi confirmado/);
    assert.equal(button("Preparar anfitrião").disabled, true);
    await submit(); assert.equal(count(sourceCommands[profile]), 1);
  });

  test(`single source ${profile} observes post-reservation native failure and waits for cleanup`, async () => {
    singleSourceFixtures(profile); await selectSingleSource(profile);
    handlers.set(sourceCommands[profile], () => { snapshot = terminalSingleSource(profile); throw "connect_source_changed"; });
    await submit();
    assert.match(container.textContent, /Aguardando liberação dos recursos/);
    assert.equal(container.querySelector("form"), null);
    await click("Encerrar Connect"); assert.match(container.textContent, /Recursos liberados/);
    assert.equal(count(sourceCommands[profile]), 1);
  });

  for (const failure of ["unknown", "malformed"]) {
    test(`single source ${profile} does not use old idle as proof after ${failure} reply`, async () => {
      singleSourceFixtures(profile); await selectSingleSource(profile);
      handlers.set(sourceCommands[profile], () => {
        if (failure === "unknown") throw new Error("connect_invalid_request");
        return { code: "connect_invalid_request" };
      });
      await submit(); await click("Atualizar estado"); await click("Atualizar estado");
      assert.match(container.textContent, /resultado do preparo ainda não foi confirmado/);
      assert.equal(container.querySelector("form"), null);
      snapshot = pendingSingleSource(profile); await click("Atualizar estado");
      await click("Encerrar Connect"); assert.equal(count(sourceCommands[profile]), 1);
    });
  }

  test(`single source ${profile} rechecks selection expiration before invoking preparation`, async () => {
    singleSourceFixtures(profile); await selectSingleSource(profile);
    const now = Date.now;
    try { Date.now = () => discoverySelected.expiresAtMs; await submit(); }
    finally { Date.now = now; }
    assert.equal(count(sourceCommands[profile]), 0);
    assert.match(container.textContent, /seleção de porta expirou/);
    assert.equal(button("Preparar anfitrião").disabled, true);
  });
}

test("single source definitive error cannot reuse a snapshot invoke started before its reply", async () => {
  singleSourceFixtures("legacy"); await selectSingleSource("legacy");
  let rejectPrepare, completeOldSnapshot;
  handlers.set(sourceCommands.legacy, () => new Promise((_, reject) => { rejectPrepare = reject; }));
  await submit();
  handlers.set("connect_snapshot", () => new Promise(resolve => { completeOldSnapshot = resolve; }));
  await click("Atualizar estado");
  await act(async () => rejectPrepare("connect_invalid_request"));
  await act(async () => completeOldSnapshot(idle()));
  assert.match(container.textContent, /resultado do preparo ainda não foi confirmado/);
  assert.equal(container.querySelector("form"), null);
  handlers.delete("connect_snapshot");
  await click("Atualizar estado");
  assert.doesNotMatch(container.textContent, /resultado do preparo ainda não foi confirmado/);
  assert.equal(button("Preparar anfitrião").disabled, true);
  assert.equal(count(sourceCommands.legacy), 1);
});

test("single source failed proof query retains definitive context for later explicit observation", async () => {
  singleSourceFixtures("sessionGameTcp"); await selectSingleSource("sessionGameTcp");
  handlers.set(sourceCommands.sessionGameTcp, () => { throw "connect_busy"; });
  handlers.set("connect_snapshot", () => { throw new Error("snapshot delivery failed"); });
  await submit(); assert.equal(container.querySelector("form"), null);
  assert.match(container.textContent, /resultado do preparo ainda não foi confirmado/);
  handlers.delete("connect_snapshot"); await click("Atualizar estado");
  assert.equal(button("Preparar anfitrião").disabled, true);
  assert.doesNotMatch(container.textContent, /resultado do preparo ainda não foi confirmado/);
});

for (const other of ["hostProfile", "guest", "group"]) {
  test(`single source definitive busy observes another ${other} operation for cleanup without transferring consent`, async () => {
    groupFixtures(); const stopGroup = handlers.get("connect_stop");
    singleSourceFixtures("legacy"); await selectSingleSource("legacy");
    handlers.set(sourceCommands.legacy, () => {
      snapshot = other === "group" ? reviewedGroup() : other === "guest" ? activeSession("guest") : reviewedSession("host");
      throw "connect_busy";
    });
    if (other === "group") handlers.set("connect_stop", stopGroup);
    await submit();
    assert.doesNotMatch(container.textContent, /resultado do preparo ainda não foi confirmado/);
    assert.equal(container.querySelector('option[value="ownedInstance"]'), null);
    assert.equal(button("Encerrar Connect").disabled, false);
    if (other === "group") assert.equal(button("Autorizar grupo").disabled, true);
    if (other === "hostProfile") assert.equal(button("Autorizar conexão").disabled, true);
    assert.equal(count("connect_confirm"), 0); assert.equal(count("connect_confirm_session_group_host"), 0);
    assert.deepEqual(copied, []);
    await click("Encerrar Connect"); assert.match(container.textContent, /Recursos liberados/);
    assert.equal(count(sourceCommands.legacy), 1);
    assert.equal(button("Preparar anfitrião").disabled, true);
  });
}

test("single source unknown failure does not attribute an unrelated profile snapshot to its preparation", async () => {
  singleSourceFixtures("legacy"); await selectSingleSource("legacy");
  handlers.set(sourceCommands.legacy, () => { snapshot = reviewedSession("host"); throw new Error("unknown IPC outcome"); });
  await submit(); await click("Atualizar estado");
  assert.match(container.textContent, /resultado do preparo ainda não foi confirmado/);
  assert.equal(container.querySelector('.connect-consent input'), null);
  assert.equal(container.querySelector("form"), null);
  snapshot = pendingSingleSource("legacy"); await click("Atualizar estado"); await click("Encerrar Connect");
  assert.equal(count("connect_confirm"), 0); assert.equal(count(sourceCommands.legacy), 1);
});

test("single source proof accepts an unchanged completed baseline but never an active or regressing one", async () => {
  singleSourceFixtures("legacy"); snapshot = terminalSingleSource("legacy", "complete");
  await selectSingleSource("legacy");
  handlers.set(sourceCommands.legacy, () => { throw "connect_internal"; });
  await submit();
  assert.equal(button("Preparar anfitrião").disabled, true);
  assert.doesNotMatch(container.textContent, /resultado do preparo ainda não foi confirmado/);
  const baseline = terminalSingleSource("legacy", "complete");
  assert.equal(hostSourceTypes.provesNoSourceReservation(baseline, baseline), true);
  for (const patch of [{ generation: 1 }, { operationId: "another" }, { cleanupState: "pending" }, { transportState: "listening" }, { phase: "hostPreparing" }]) {
    assert.equal(hostSourceTypes.provesNoSourceReservation({ ...baseline, ...patch }, baseline), false);
  }
  const active = pendingSingleSource("legacy");
  assert.equal(hostSourceTypes.provesNoSourceReservation(active, active), false);
  assert.equal(hostSourceTypes.provesNoSourceReservation({ ...terminalSingleSource("legacy", "complete"), generation: active.generation }, active), false);
  assert.equal(hostSourceTypes.provesNoSourceReservation(idle(), undefined), false);
});

function expiredManualHost() {
  return { ...idle(), operationId, generation: 2, role: "host", phase: "closed", cleanupState: "complete", errorCode: "connect_plan_expired" };
}

test("single source definitive pre-rejection recovers after a manual host expires before transport starts", async () => {
  singleSourceFixtures("legacy"); snapshot = expiredManualHost();
  await selectSingleSource("legacy");
  handlers.set(sourceCommands.legacy, () => { throw "connect_invalid_request"; });
  const start = calls.length;
  await submit();
  assert.deepEqual(calls.slice(start).map(item => item.command), [sourceCommands.legacy, "connect_snapshot"]);
  assert.doesNotMatch(container.textContent, /resultado do preparo ainda não foi confirmado/);
  assert.equal(button("Preparar anfitrião").disabled, true);
  await submit(); assert.equal(count(sourceCommands.legacy), 1);
});

test("never-started terminal proof requires unchanged ownership, phase, generation and completed cleanup", () => {
  const baseline = expiredManualHost();
  assert.deepEqual(types.snapshotFrom(baseline), baseline);
  assert.equal(hostSourceTypes.provesNoSourceReservation(baseline, baseline), true);
  for (const patch of [{ operationId: "different" }, { generation: 1 }, { generation: 3 }, { role: "guest" },
    { phase: "failed" }, { phase: "hostPrepared" }, { transportState: "closed" }, { transportState: "listening" },
    { cleanupState: "pending" }, { cleanupState: "failed" }, { preparation }, { review }]) {
    assert.equal(hostSourceTypes.provesNoSourceReservation({ ...baseline, ...patch }, baseline), false);
    assert.equal(hostSourceTypes.provesNoSourceReservation(baseline, { ...baseline, ...patch }), false);
  }
});

test("expired manual baseline cannot settle in-flight preparation or a snapshot started before rejection", async () => {
  singleSourceFixtures("legacy"); snapshot = expiredManualHost();
  await selectSingleSource("legacy");
  let rejectPrepare, completeOldSnapshot;
  handlers.set(sourceCommands.legacy, () => new Promise((_, reject) => { rejectPrepare = reject; }));
  await submit(); await click("Atualizar estado");
  assert.match(container.textContent, /resultado do preparo ainda não foi confirmado/);
  assert.equal(container.querySelector("form"), null);
  handlers.set("connect_snapshot", () => new Promise(resolve => { completeOldSnapshot = resolve; }));
  await click("Atualizar estado");
  await act(async () => rejectPrepare("connect_invalid_request"));
  await act(async () => completeOldSnapshot(expiredManualHost()));
  assert.match(container.textContent, /resultado do preparo ainda não foi confirmado/);
  assert.equal(container.querySelector("form"), null);
  handlers.delete("connect_snapshot"); await click("Atualizar estado");
  assert.doesNotMatch(container.textContent, /resultado do preparo ainda não foi confirmado/);
  assert.equal(button("Preparar anfitrião").disabled, true);
  assert.equal(count(sourceCommands.legacy), 1);
});

for (const failure of ["unknown", "malformed"]) {
  test(`expired manual baseline is not proof after ${failure} preparation reply`, async () => {
    singleSourceFixtures("legacy"); snapshot = expiredManualHost();
    await selectSingleSource("legacy");
    handlers.set(sourceCommands.legacy, () => {
      if (failure === "unknown") throw new Error("connect_invalid_request");
      return { code: "connect_invalid_request" };
    });
    await submit(); await click("Atualizar estado");
    assert.match(container.textContent, /resultado do preparo ainda não foi confirmado/);
    assert.equal(container.querySelector("form"), null);
    assert.equal(count(sourceCommands.legacy), 1);
  });
}

test("single source remount and failed capability query still recover cancellation for every profile", async () => {
  singleSourceFixtures(sessionFragmentedProfile); snapshot = pendingSingleSource(sessionFragmentedProfile);
  handlers.set("connect_capabilities", () => { throw new Error("capability unavailable"); });
  await render(); assert.match(container.textContent, /Conferindo a origem do jogo/);
  await act(async () => root.unmount()); root = createRoot(container); await render();
  await click("Encerrar Connect"); assert.match(container.textContent, /Recursos liberados/);
  assert.equal(count(sourceCommands[sessionFragmentedProfile]), 0);
  assert.equal(count("connect_confirm"), 0);
});

test("single source preparation is unavailable until an initial native snapshot establishes a baseline", async () => {
  singleSourceFixtures("legacy");
  handlers.set("connect_snapshot", () => { throw new Error("initial observation unavailable"); });
  await render();
  assert.equal(container.querySelector("form"), null);
  assert.equal(container.querySelector('option[value="ownedInstance"]'), null);
  assert.match(container.textContent, /não pôde ser atualizado/);
  handlers.delete("connect_snapshot"); await click("Atualizar estado");
  await discoveryToReview(); await click("Autorizar consulta e selecionar porta");
  handlers.set(sourceCommands.legacy, () => { throw "connect_invalid_request"; });
  await submit();
  assert.equal(button("Preparar anfitrião").disabled, true);
  assert.doesNotMatch(container.textContent, /resultado do preparo ainda não foi confirmado/);
});

test("single source reservation parser rejects cross-profile metadata and fabricated preparation authority", () => {
  for (const profile of sourceProfiles) {
    const value = pendingSingleSource(profile);
    assert.deepEqual(types.snapshotFrom(value), value);
    for (const patch of [{ available: false }, { operationId: "not-uuid" }, { generation: 0 }, { role: "guest" }, { phase: "hostPrepared" },
      { cleanupState: "complete" }, { transportState: "listening" }, { expiresAtMs: null }, { errorCode: "connect_internal" },
      { preparation }, { review }, { publicId: hostId }, { hostGroup: preparedGroup().hostGroup }, { pendingGroupSource: pendingDiscoveredGroup().pendingGroupSource },
      { sessionTiming }, { fragmentedUdp: sessionFragmentedContract }, { udpState: "notStarted" }, { udpTarget: "127.0.0.1:43002" },
      { pendingHostSource: { ...value.pendingHostSource, mode: "dns" } }, { pendingHostSource: { ...value.pendingHostSource, profile: "fragmentedGameUdp" } },
      { pendingHostSource: { ...value.pendingHostSource, peer: hostId } },
      { invitationProfile: profile === "legacy" ? "sessionGameTcp" : "fragmentedGameUdp" },
    ]) assert.throws(() => types.snapshotFrom({ ...value, ...patch }));
    const { pendingHostSource: marker, ...without } = value;
    assert.throws(() => types.snapshotFrom(without));
    assert.throws(() => types.preparationFrom({ ...preparation, pendingHostSource: marker }));
    assert.throws(() => types.reviewFrom({ ...review, pendingHostSource: marker }));
    assert.throws(() => types.exportFrom({ ...sessionExport, pendingHostSource: marker }));
  }
  assert.equal(hostSourceTypes.isControlledSourceRejection("connect_internal"), true);
  for (const value of [new Error("connect_internal"), { code: "connect_internal" }, "private internal", "connect_transport_failed", null]) {
    assert.equal(hostSourceTypes.isControlledSourceRejection(value), false);
  }
});

function ownedSourceReview(profile) {
  const base = preparedSingleSource(profile);
  const value = profile === sessionFragmentedProfile ? reviewedDiscoveredSessionFragmented().review : profile === "sessionGameTcp" ? sessionReview : review;
  const projected = { ...value, operationId: sourceOperationId, targetSource: "ownedInstance", instanceId: discoverySelected.instanceId,
    runId: discoverySelected.runId, gameTarget: discoverySelected.target };
  return { ...base, generation: 3, phase: "reviewed", expiresAtMs: projected.expiresAtMs, review: projected,
    ...(projected.sessionTiming ? { sessionTiming: projected.sessionTiming } : {}),
    ...(projected.fragmentedUdp ? { fragmentedUdp: sessionFragmentedContract } : {}) };
}

for (const profile of sourceProfiles) {
  test(`owned review ${profile} remains observable and cancellable while late result cannot defeat retained stop`, async () => {
    singleSourceFixtures(profile); snapshot = preparedSingleSource(profile);
    let finishReview, finishStop;
    handlers.set("connect_review_host", () => new Promise(resolve => { finishReview = resolve; }));
    handlers.set("connect_stop", () => {
      snapshot = { ...terminalSingleSource(profile), generation: 4, phase: "closing", errorCode: null };
      return new Promise(resolve => { finishStop = resolve; });
    });
    await render(); await submit();
    assert.deepEqual(calls.find(item => item.command === "connect_review_host").args, { operationId: sourceOperationId, guestPublicId: "" });
    assert.equal(button("Atualizar estado").disabled, false);
    assert.equal(button("Revisar convite").disabled, true);
    await click("Atualizar estado");
    assert.match(container.textContent, /A revisão ainda não foi confirmada/);
    assert.equal(button("Revisar convite").disabled, true);
    await submit(); assert.equal(count("connect_review_host"), 1);
    await click("Encerrar Connect");
    await act(async () => finishReview(ownedSourceReview(profile).review));
    assert.match(container.textContent, /Encerrando recursos/);
    assert.doesNotMatch(container.textContent, /Recursos liberados/);
    assert.equal(container.querySelector('.connect-consent input'), null);
    await act(async () => finishStop({ ...snapshot, generation: 5, phase: "closed", cleanupState: "complete" }));
    assert.match(container.textContent, /Recursos liberados/);
    assert.equal(count("connect_review_host"), 1); assert.equal(count("connect_confirm"), 0);
    assert.equal(count("connect_export_invitation"), 0);
  });

  test(`owned review ${profile} reconciles lost reply by observation without recovering consent`, async () => {
    singleSourceFixtures(profile); snapshot = preparedSingleSource(profile);
    handlers.set("connect_review_host", () => { snapshot = ownedSourceReview(profile); throw new Error("review reply lost"); });
    await render(); await submit();
    assert.equal(button("Revisar convite").disabled, true);
    await click("Atualizar estado");
    assert.equal(button("Autorizar conexão").disabled, true);
    assert.equal(container.querySelector('.connect-consent input').checked, false);
    assert.doesNotMatch(container.textContent, /A revisão ainda não foi confirmada/);
    assert.equal(count("connect_review_host"), 1); assert.equal(count("connect_confirm"), 0);
    await click("Encerrar Connect");
  });

  test(`owned review ${profile} failure keeps cleanup pending and no new preparation is available`, async () => {
    singleSourceFixtures(profile); snapshot = preparedSingleSource(profile);
    handlers.set("connect_review_host", () => { snapshot = { ...terminalSingleSource(profile), generation: 3 }; throw "connect_source_changed"; });
    await render(); await submit(); await click("Atualizar estado");
    assert.match(container.textContent, /Aguardando liberação dos recursos/);
    assert.equal(container.querySelector("form"), null);
    assert.equal(container.querySelector('.connect-consent input'), null);
    await click("Encerrar Connect"); assert.match(container.textContent, /Recursos liberados/);
    assert.equal(count("connect_review_host"), 1); assert.equal(count("connect_confirm"), 0);
    assert.equal(count(sourceCommands[profile]), 0);
  });
}

for (const failure of ["connect_busy", "unknown", "malformed"]) {
  test(`owned review ${failure} does not interpret unchanged hostPrepared as a finished review`, async () => {
    singleSourceFixtures("legacy"); snapshot = preparedSingleSource("legacy");
    handlers.set("connect_review_host", () => {
      if (failure === "unknown") throw new Error("private unresolved review");
      if (failure === "malformed") return { operationId: sourceOperationId, digest };
      throw failure;
    });
    await render(); await submit(); await click("Atualizar estado");
    assert.equal(button("Revisar convite").disabled, true);
    assert.match(container.textContent, /A revisão ainda não foi confirmada/);
    await submit(); assert.equal(count("connect_review_host"), 1);
    snapshot = ownedSourceReview("legacy"); await click("Atualizar estado");
    assert.equal(button("Autorizar conexão").disabled, true);
    assert.equal(count("connect_confirm"), 0);
    await click("Encerrar Connect");
  });
}

test("pure native review input rejection permits editing and a separate explicit review", async () => {
  singleSourceFixtures("legacy"); snapshot = preparedSingleSource("legacy");
  handlers.set("connect_review_host", () => { throw "connect_invalid_request"; });
  await render(); await submit();
  assert.equal(button("Revisar convite").disabled, false);
  assert.doesNotMatch(container.textContent, /A revisão ainda não foi confirmada/);
  assert.equal(count("connect_review_host"), 1);
  await editInput(labeledInput("Código público recebido do convidado"), guestId);
  handlers.set("connect_review_host", () => { snapshot = ownedSourceReview("legacy"); return snapshot.review; });
  await submit(); assert.equal(count("connect_review_host"), 2);
  assert.equal(button("Autorizar conexão").disabled, true);
  assert.equal(count("connect_confirm"), 0);
});

test("host public code cannot be submitted as its own guest and remains editable", async () => {
  singleSourceFixtures("legacy"); snapshot = preparedSingleSource("legacy");
  await render(); await editInput(labeledInput("Código público recebido do convidado"), hostId.toUpperCase());
  await submit();
  assert.equal(count("connect_review_host"), 0);
  assert.equal(button("Revisar convite").disabled, false);
  assert.match(container.textContent, /Confira os endereços, o código público/);
  assert.doesNotMatch(container.textContent, /A revisão ainda não foi confirmada/);
  await editInput(labeledInput("Código público recebido do convidado"), guestId);
  handlers.set("connect_review_host", () => { snapshot = ownedSourceReview("legacy"); return snapshot.review; });
  await submit(); assert.equal(count("connect_review_host"), 1);
  assert.equal(button("Autorizar conexão").disabled, true);
  assert.equal(count("connect_confirm"), 0);
});

test("review observation that supersedes initial IPC does not permit its late reply to restore an older digest", async () => {
  singleSourceFixtures("legacy"); snapshot = preparedSingleSource("legacy");
  let finish;
  handlers.set("connect_review_host", () => new Promise(resolve => { finish = resolve; }));
  await render(); await submit();
  snapshot = ownedSourceReview("legacy"); snapshot.review = { ...snapshot.review, digest: "e".repeat(64) };
  await click("Atualizar estado");
  assert.equal(button("Autorizar conexão").disabled, true);
  await act(async () => finish(ownedSourceReview("legacy").review));
  assert.match(container.textContent, new RegExp("e".repeat(64)));
  assert.equal(button("Autorizar conexão").disabled, true);
  assert.equal(count("connect_confirm"), 0); assert.equal(count("connect_review_host"), 1);
});

test("remount during review recovers the native review without consent or accepting an old component reply", async () => {
  singleSourceFixtures("sessionGameTcp"); snapshot = preparedSingleSource("sessionGameTcp");
  let finish;
  handlers.set("connect_review_host", () => new Promise(resolve => { finish = resolve; }));
  await render(); await submit();
  await act(async () => root.unmount()); root = createRoot(container);
  snapshot = ownedSourceReview("sessionGameTcp"); await render();
  assert.equal(button("Autorizar conexão").disabled, true);
  await act(async () => finish(ownedSourceReview("sessionGameTcp").review));
  assert.equal(button("Autorizar conexão").disabled, true);
  assert.equal(count("connect_review_host"), 1); assert.equal(count("connect_confirm"), 0);
  await click("Encerrar Connect");
});

const pathObservation = state => ({ schemaVersion: 1, state });
function legacyPathSnapshot(role = "host") {
  return { ...idle(), operationId, generation: 1, role, phase: role === "host" ? "hostListening" : "guestConnected",
    transportState: role === "host" ? "listening" : "authenticated", tcpAddress: role === "guest" ? "127.0.0.1:42123" : null };
}

test("optional path observations preserve every single-peer profile and all legacy projections", () => {
  for (const role of ["host", "guest"]) {
    for (const value of [legacyPathSnapshot(role), activeSession(role), activeFragmented(role), activeSessionFragmented(role)]) {
      assert.deepEqual(types.snapshotFrom(value), value);
      for (const state of ["unknown", "direct", "relay", "closed"]) {
        const observed = { ...value, pathObservation: pathObservation(state) };
        assert.deepEqual(types.snapshotFrom(observed), observed);
        assert.equal(observed.pathState, "notVerified"); assert.equal(observed.relayEnabled, false);
      }
    }
  }
});

test("path observations are absent from preparation, review, capability and invitation exports", () => {
  const extra = { pathObservation: pathObservation("direct") };
  for (const value of [preparation, sessionPreparation, fragmentedPreparation, preparedSessionFragmented().preparation]) {
    assert.throws(() => types.preparationFrom({ ...value, ...extra }));
  }
  for (const value of [review, sessionReview, fragmentedReview, reviewedSessionFragmented().review]) {
    assert.throws(() => types.reviewFrom({ ...value, ...extra }));
  }
  for (const value of [{ invitation: "opaque", hostPublicId: hostId, expiresAtMs: 1800000600000 }, sessionExport, fragmentedExport, sessionFragmentedExport]) {
    assert.throws(() => types.exportFrom({ ...value, ...extra }));
  }
  assert.throws(() => types.capabilitiesFrom({ ...caps, ...extra }));
  assert.throws(() => types.reviewFrom({ ...review, pathObservation: undefined }));
});

test("path guards reject malformed, wrong-role and non-live snapshots before specialized returns", () => {
  for (const value of [undefined, null, {}, [], { schemaVersion: 2, state: "direct" }, { schemaVersion: 1, state: "mixed" },
    { ...pathObservation("direct"), ip: "untrusted" }]) {
    assert.throws(() => types.snapshotFrom({ ...activeSession(), pathObservation: value }));
  }
  for (const value of [idle(), preparedSession(), preparedSession("guest"), reviewedSession(),
    { ...activeSession(), phase: "starting" }, { ...activeSession(), phase: "closing" },
    { ...activeSession(), phase: "closed", transportState: "closed" }, { ...activeSession(), phase: "failed", transportState: "closed" },
    { ...activeSession(), role: "guest" }, { ...activeSession("guest"), role: "host" }, { ...activeSession(), available: false },
    pendingDiscoveredGroup(), ...["legacy", "sessionGameTcp", sessionFragmentedProfile].map(pendingSingleSource)]) {
    assert.throws(() => types.snapshotFrom({ ...value, pathObservation: pathObservation("direct") }));
  }
  assert.throws(() => hostSourceTypes.pendingHostSourceFieldsFrom({ ...pendingSingleSource("legacy"), pathObservation: undefined }));
});

test("group paths are individual, consumed and phase-bound; reviews never accept an observation", () => {
  for (const state of ["unknown", "direct", "relay", "closed"]) {
    const value = activeGroup();
    Object.assign(value.hostGroup.participants[0], { consumed: true, admissionValid: false,
      transportActive: state !== "closed", pathObservation: pathObservation(state) });
    assert.deepEqual(types.snapshotFrom(value), value);
    assert.deepEqual(groupTypes.groupSnapshotFieldsFrom(value), value.hostGroup);
    for (const changed of [{ consumed: false }, { sessionValid: false, admissionValid: false, transportActive: false },
      { revoked: true, sessionValid: false, admissionValid: false, transportActive: false }, { transportActive: state === "closed" },
      { pathObservation: { ...pathObservation(state), peer: "untrusted" } }]) {
      const broken = structuredClone(value); Object.assign(broken.hostGroup.participants[0], changed);
      assert.throws(() => types.snapshotFrom(broken));
    }
    for (const phase of ["starting", "closing", "closed", "failed"]) {
      const broken = structuredClone(value); broken.phase = phase;
      broken.transportState = phase === "starting" ? "notStarted" : "closed";
      if (phase !== "starting") for (const row of broken.hostGroup.participants) Object.assign(row, { admissionValid: false, sessionValid: false, transportActive: false });
      assert.throws(() => types.snapshotFrom(broken));
    }
  }
  for (const value of [preparedGroup(), reviewedGroup(), activeGroup()]) {
    assert.throws(() => types.snapshotFrom({ ...value, pathObservation: pathObservation("direct") }));
    assert.throws(() => groupTypes.groupSnapshotFieldsFrom({ ...value, pathObservation: undefined }));
  }
  const reviewed = reviewedGroup(); reviewed.hostGroup.review.participants[0].pathObservation = pathObservation("closed");
  assert.throws(() => groupTypes.groupReviewFrom(reviewed.hostGroup.review));
  assert.throws(() => types.snapshotFrom(reviewed));
  const exported = { operationId: groupOperation, participantId: groupParticipants()[0].participantId, invitationDigest: digest,
    invitation: "opaque", hostPublicId: hostId, expiresAtMs: groupParticipants()[0].sessionTiming.admissionExpiresAtMs,
    sessionTiming: groupParticipants()[0].sessionTiming, fragmentedUdp: sessionFragmentedContract };
  assert.throws(() => groupTypes.groupExportFrom({ ...exported, pathObservation: pathObservation("direct") }));
});

test("participant identity ignores only a changing observation and preserves every authority field", () => {
  const original = groupParticipants()[0];
  assert.equal(groupTypes.sameGroupParticipant({ ...original, pathObservation: pathObservation("direct") },
    { ...original, pathObservation: pathObservation("relay") }), true);
  for (const changed of [{ participantId: groupParticipants()[1].participantId }, { guestPublicId: "7".repeat(64) },
    { invitationDigest: "8".repeat(64) }, { sessionTiming: { ...original.sessionTiming, sessionExpiresAtMs: original.sessionTiming.sessionExpiresAtMs + 1 } }]) {
    assert.equal(groupTypes.sameGroupParticipant(original, { ...original, ...changed, pathObservation: pathObservation("direct") }), false);
  }
});

for (const [state, label] of Object.entries({ unknown: "Não confirmado", direct: "Caminho direto observado", relay: "Relay observado", closed: "Conexão encerrada" })) {
  test(`single ${state} path is descriptive and never grants a world, voice or new connection`, async () => {
    sessionFixtures(); snapshot = { ...activeSession("guest"), pathObservation: pathObservation(state) }; await render();
    assert.match(container.textContent, new RegExp(label));
    assert.equal(container.textContent.split("O caminho pode mudar durante a conexão.").length - 1, 1);
    assert.match(container.textContent, /Esta observação não confirma entrada no mundo/);
    assert.match(container.textContent, /Entrada no mundoNão verificada/);
    assert.match(container.textContent, /Simple Voice ChatDesativado nesta integração/);
    assert.match(container.textContent, /Plasmo VoiceDesativado nesta integração/);
    assert.equal(count("connect_confirm"), 0); assert.equal(count("connect_probe_status"), 0); assert.deepEqual(copied, []);
    assert.equal(snapshot.pathState, "notVerified"); assert.equal(snapshot.relayEnabled, false);
    await click("Encerrar Connect"); assert.match(container.textContent, /Recursos liberados/);
    assert.doesNotMatch(container.textContent, /Caminho direto observado|Relay observado|O caminho pode mudar durante a conexão/);
  });
}

test("old backends and initial addresses alone keep the path unconfirmed", async () => {
  sessionFixtures(); snapshot = activeSession("guest"); await render();
  assert.match(container.textContent, /Não confirmado · relay desligado/);
  assert.doesNotMatch(container.textContent, /Caminho direto observado|Relay observado|O caminho pode mudar/);
  assert.equal(count("connect_confirm"), 0); assert.equal(count("connect_probe_status"), 0);
});

test("group shows paths per participant and revoking one preserves the other's observation", async () => {
  groupFixtures(); snapshot = activeGroup();
  snapshot.hostGroup.participants.forEach((row, index) => Object.assign(row, { consumed: true, admissionValid: false, transportActive: true,
    pathObservation: pathObservation(index === 0 ? "direct" : "relay") }));
  await render();
  const rows = () => [...container.querySelectorAll('section[aria-label^="Participante autorizado"]')];
  assert.match(rows()[0].textContent, /Caminho individualCaminho direto observado/);
  assert.match(rows()[1].textContent, /Caminho individualRelay observado/);
  assert.doesNotMatch(container.querySelector('section[aria-label="Estado observado da conexão"]').textContent, /Caminho da conexão/);
  assert.equal(container.textContent.split("O caminho pode mudar durante a conexão.").length - 1, 1);
  await click("Revogar participante 1");
  assert.doesNotMatch(rows()[0].textContent, /Caminho direto observado/);
  assert.match(rows()[1].textContent, /Relay observado/);
  assert.equal(count("connect_revoke_session_group_participant"), 1);
  assert.equal(count("connect_confirm_session_group_host"), 0); assert.equal(count("connect_export_session_group_invitation"), 0);
  await click("Encerrar Connect");
  assert.match(container.textContent, /Recursos liberados/);
  assert.doesNotMatch(container.textContent, /Caminho individual|Caminho direto observado|Relay observado/);
});

test("path changes never renew or re-export an unrelated participant's invitation", async () => {
  groupFixtures(); snapshot = activeGroup();
  Object.assign(snapshot.hostGroup.participants[0], { consumed: true, transportActive: true, pathObservation: pathObservation("direct") });
  await render(); await click("Selecionar convite 2"); await click("Copiar convite selecionado");
  assert.equal(copied.length, 1);
  const timing = structuredClone(snapshot.hostGroup.participants.map(row => row.sessionTiming));
  snapshot = { ...snapshot, generation: snapshot.generation + 1, hostGroup: { ...snapshot.hostGroup,
    participants: snapshot.hostGroup.participants.map((row, index) => index === 0 ? { ...row, pathObservation: pathObservation("relay") } : row) } };
  await click("Atualizar estado");
  assert.deepEqual(snapshot.hostGroup.participants.map(row => row.sessionTiming), timing);
  assert.equal(count("connect_export_session_group_invitation"), 1); assert.equal(copied.length, 1);
  assert.equal(container.querySelector("textarea[readonly]")?.value.includes("opaque-"), undefined);
  assert.equal(count("connect_confirm_session_group_host"), 0);
});

test("invalid path refresh hides the stale sample while keeping trusted stop and cleanup available", async () => {
  sessionFixtures(); snapshot = { ...activeSession("guest"), pathObservation: pathObservation("direct") }; await render();
  assert.match(container.textContent, /Caminho direto observado/);
  snapshot = { ...snapshot, generation: snapshot.generation + 1, pathObservation: { ...pathObservation("direct"), url: "private fixture value" } };
  await click("Atualizar estado");
  assert.match(container.textContent, /estado não pôde ser atualizado/);
  assert.doesNotMatch(container.textContent, /Caminho direto observado|private fixture value/);
  assert.equal(button("Encerrar Connect").disabled, false);
  await click("Encerrar Connect");
  assert.equal(count("connect_stop"), 1); assert.match(container.textContent, /Recursos liberados/);
});

test("late path refresh after stop and remount cannot republish a previous connection", async () => {
  sessionFixtures(); snapshot = { ...activeSession("guest"), pathObservation: pathObservation("direct") }; await render();
  const late = { ...snapshot, generation: snapshot.generation + 1, pathObservation: pathObservation("relay") };
  let finish; handlers.set("connect_snapshot", () => new Promise(resolve => { finish = resolve; }));
  await click("Atualizar estado"); await click("Encerrar Connect");
  await act(async () => root.unmount()); root = createRoot(container);
  handlers.delete("connect_snapshot"); await render();
  await act(async () => finish(late));
  assert.match(container.textContent, /Recursos liberados/);
  assert.doesNotMatch(container.textContent, /Caminho direto observado|Relay observado|O caminho pode mudar/);
  assert.equal(count("connect_confirm"), 0); assert.equal(count("connect_stop"), 1);
});

for (const kind of ["single", "group"]) {
  test(`${kind} lost stop reply keeps path samples hidden until a fresh valid snapshot`, async () => {
    if (kind === "group") {
      groupFixtures(); snapshot = activeGroup();
      snapshot.hostGroup.participants.forEach(row => Object.assign(row, { consumed: true, transportActive: true, pathObservation: pathObservation("direct") }));
    } else { sessionFixtures(); snapshot = { ...activeSession("guest"), pathObservation: pathObservation("direct") }; }
    const original = structuredClone(snapshot);
    const originalStop = handlers.get("connect_stop");
    await render(); assert.match(container.textContent, /Caminho direto observado/);
    let finishDuringStop;
    handlers.set("connect_snapshot", () => new Promise(resolve => { finishDuringStop = resolve; }));
    await click("Atualizar estado");
    let failStop;
    handlers.set("connect_stop", () => new Promise((_resolve, reject) => { failStop = reject; }));
    await click("Encerrar Connect");
    // A prior poll may finish while Stop is awaiting its reply. The old epoch
    // cannot clear uncertainty or publish a path from that response.
    await act(async () => finishDuringStop(original));
    assert.doesNotMatch(container.textContent, /Caminho direto observado|Relay observado/);
    await act(async () => failStop(new Error("lost stop fixture")));
    assert.match(container.textContent, /encerramento ainda não foi confirmado/);
    assert.doesNotMatch(container.textContent, /Caminho direto observado|Relay observado|O caminho pode mudar/);
    assert.equal(button("Encerrar Connect").disabled, false);
    let failObservation;
    handlers.set("connect_snapshot", () => new Promise((_resolve, reject) => { failObservation = reject; }));
    await click("Atualizar estado");
    assert.doesNotMatch(container.textContent, /Caminho direto observado|Relay observado/);
    assert.equal(button("Encerrar Connect").disabled, false);
    await act(async () => failObservation(new Error("unobserved state fixture")));
    assert.doesNotMatch(container.textContent, /Caminho direto observado|Relay observado|lost stop fixture|unobserved state fixture/);
    assert.equal(button("Encerrar Connect").disabled, false);
    handlers.delete("connect_snapshot");
    snapshot = { ...original, generation: original.generation + 1 };
    if (kind === "group") snapshot.hostGroup.participants.forEach(row => { row.pathObservation = pathObservation("relay"); });
    else snapshot.pathObservation = pathObservation("relay");
    await click("Atualizar estado");
    assert.match(container.textContent, /Relay observado/);
    assert.doesNotMatch(container.textContent, /Caminho direto observado|encerramento ainda não foi confirmado/);
    assert.equal(count("connect_stop"), 1);
    assert.equal(count("connect_confirm"), 0); assert.equal(count("connect_confirm_session_group_host"), 0);
    if (originalStop) handlers.set("connect_stop", originalStop); else handlers.delete("connect_stop");
    await click("Encerrar Connect");
    assert.match(container.textContent, /Recursos liberados/);
    assert.doesNotMatch(container.textContent, /Caminho direto observado|Relay observado/);
  });
}
