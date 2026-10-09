// Local projections and controlled DOM only. No native invocation, transport,
// Java, clipboard, mod configuration or private data are accessed.
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
document.oninput = null;
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
const modules = new Map();
async function sourceModule(source) {
  if (modules.has(source.href)) return modules.get(source.href);
  let output = ts.transpileModule(await readFile(source, "utf8"), { fileName: fileURLToPath(source), compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX,
  } }).outputText;
  for (const { fileName: specifier } of ts.preProcessFile(output).importedFiles) {
    const dependency = specifier.startsWith(".") ? ["", ".ts", ".tsx"].map(ext => new URL(specifier + ext, source)).find(url => existsSync(fileURLToPath(url))) : undefined;
    const resolved = dependency ? await sourceModule(dependency) : pathToFileURL(require.resolve(specifier)).href;
    output = output.replaceAll(`from "${specifier}"`, `from ${JSON.stringify(resolved)}`);
  }
  const url = `data:text/javascript;base64,${Buffer.from(output).toString("base64")}`;
  modules.set(source.href, url);
  return url;
}
const types = await import(await sourceModule(new URL("./fragmented-types.ts", import.meta.url)));
const { FragmentedFields, FragmentedReviewFields, FragmentedStatusFields } = await import(await sourceModule(new URL("./FragmentedFields.tsx", import.meta.url)));
const { act, createElement, useState } = await import("react");
const { createRoot } = await import("react-dom/client");
const contract = {
  profile: "fragmentedGameUdp", invitationSchemaVersion: 1, udpEnvelopeVersion: 2,
  approvedOriginalDatagramBytes: 4096, maxFragmentFrameBytes: 1264, localUdpBind: "127.0.0.1:35554",
  udpReturnPolicy: { type: "authorizedDownstream", bytesPerSecond: 16384, datagramsPerSecond: 20 },
};
const capability = { ...contract, maxLifetimeSeconds: 120 };
const services = [
  { serviceId: "game", protocol: "tcp", revision: 1, requiredCapability: "reliableStream" },
  { serviceId: "aux", protocol: "udp", revision: 1, requiredCapability: "fragmentedDatagram" },
];
const hostReview = { role: "host", mode: "loopback", lifetimeSeconds: 120, expiresAtMs: 1800000060000,
  invitationExpiresAtMs: 1800000120000, services, fragmentedUdp: { ...contract, udpTarget: "127.0.0.1:43002" },
  gameTarget: "127.0.0.1:43001", localTcpBind: null, targetSource: "manual", instanceId: null, runId: null };
const guestReview = { ...hostReview, role: "guest", fragmentedUdp: { ...contract, udpTarget: null },
  gameTarget: null, localTcpBind: "127.0.0.1:0", targetSource: null };
const notStarted = { udpState: "notStarted", udpAddress: null, udpErrorCode: null, udpOriginalCapacity: null, udpCapacityScope: null };
const available = { udpState: "available", udpAddress: "127.0.0.1:35554", udpErrorCode: null, udpOriginalCapacity: 4096, udpCapacityScope: "initialAssociation" };
const guestSnapshot = { role: "guest", invitationProfile: "fragmentedGameUdp", phase: "guestConnected", fragmentedUdp: contract, preparation: null, review: null, ...available };
const hostSnapshot = { role: "host", invitationProfile: "fragmentedGameUdp", phase: "hostListening", fragmentedUdp: contract, preparation: null, review: null };

test("fragmented capability and transport contracts have exact immutable values and key sets", () => {
  assert.deepEqual(types.fragmentedContractFrom(contract), contract);
  assert.deepEqual(types.fragmentedCapabilityFrom(capability), capability);
  for (const value of [null, {}, [], { ...contract, profile: "sessionGameTcp" }, { ...contract, invitationSchemaVersion: 2 },
    { ...contract, udpEnvelopeVersion: 1 }, { ...contract, approvedOriginalDatagramBytes: 4095 },
    { ...contract, approvedOriginalDatagramBytes: 4097 }, { ...contract, maxFragmentFrameBytes: 1265 },
    { ...contract, localUdpBind: "127.0.0.1:0" }, { ...contract, voiceEnabled: true },
    { ...contract, udpReturnPolicy: { type: "replyBudget" } },
    { ...contract, udpReturnPolicy: { ...contract.udpReturnPolicy, bytesPerSecond: 16385 } },
    { ...contract, udpReturnPolicy: { ...contract.udpReturnPolicy, datagramsPerSecond: 21 } },
    { ...contract, udpReturnPolicy: { ...contract.udpReturnPolicy, burst: 10 } }]) {
    assert.throws(() => types.fragmentedContractFrom(value));
  }
  for (const maxLifetimeSeconds of [0, 119, 121, 600, 3600, null, undefined]) {
    assert.throws(() => types.fragmentedCapabilityFrom({ ...capability, maxLifetimeSeconds }));
  }
  assert.throws(() => types.fragmentedContractFrom(capability), "Capability is not a review/export contract");
});

test("fragmented endpoint and lifetime validation refuses DNS, external destinations and ambiguous literals", () => {
  for (const value of ["127.0.0.1:43002", "127.255.255.255:65535", "127.1.2.3:1", "[::1]:43003"]) assert.equal(types.isLoopbackUdpTarget(value), true);
  for (const value of [null, "", "localhost:43002", "192.168.1.1:43002", "0.0.0.0:43002", "[::]:43002", "[::ffff:127.0.0.1]:43002",
    "127.0.0.1:0", "127.0.0.1:65536", "127.0.0.1:043002", "127.00.0.1:43002", "127.256.1.1:43002", "127.1:43002", "127.0.0.1:43002 ", "[0:0:0:0:0:0:0:1]:43002"]) {
    assert.equal(types.isLoopbackUdpTarget(value), false);
  }
  for (const value of [1, 120]) assert.equal(types.isFragmentedLifetime(value), true);
  for (const value of [0, -1, 121, 600, 1.5, NaN, Infinity, "120", null]) assert.equal(types.isFragmentedLifetime(value), false);
});

test("review separates the private UDP target from the fixed guest bind and exactly two grants", () => {
  assert.deepEqual(types.fragmentedReviewFieldsFrom(hostReview), hostReview.fragmentedUdp);
  assert.deepEqual(types.fragmentedReviewFieldsFrom(guestReview), guestReview.fragmentedUdp);
  for (const value of [
    { ...hostReview, fragmentedUdp: { ...contract, udpTarget: "192.168.1.1:43002" } },
    { ...guestReview, fragmentedUdp: hostReview.fragmentedUdp },
    { ...hostReview, services: [...services].reverse() }, { ...hostReview, services: [services[0]] },
    { ...hostReview, services: [...services, services[1]] }, { ...hostReview, services: [services[0], services[0]] },
    { ...hostReview, services: [services[0], { ...services[1], requiredCapability: "datagram" }] },
    { ...hostReview, services: [services[0], { ...services[1], maxDatagramBytes: 2048 }] },
    { ...hostReview, services: [services[0], { ...services[1], revision: 2 }] },
    { ...hostReview, lifetimeSeconds: 121 }, { ...hostReview, expiresAtMs: hostReview.invitationExpiresAtMs + 1 },
    { ...hostReview, targetSource: "ownedInstance", instanceId: "fixture", runId: "fixture" },
    { ...hostReview, targetSource: null }, { ...guestReview, targetSource: "manual" },
    { ...hostReview, gameTarget: "example.test:25565" }, { ...guestReview, localTcpBind: "127.0.0.1:12345" },
    { ...guestReview, udpState: "available" }, { ...hostReview, sessionTiming: {} },
    { ...hostReview, invitationProfile: "sessionGameTcp" },
  ]) assert.throws(() => types.fragmentedReviewFieldsFrom(value));
});

test("export keeps only the transport contract and refuses private target or session metadata", () => {
  const exported = { expiresAtMs: hostReview.invitationExpiresAtMs, fragmentedUdp: contract };
  assert.deepEqual(types.fragmentedExportFieldsFrom(exported), contract);
  assert.throws(() => types.fragmentedExportFieldsFrom({ ...exported, fragmentedUdp: hostReview.fragmentedUdp }));
  assert.throws(() => types.fragmentedExportFieldsFrom({ ...exported, udpTarget: "127.0.0.1:43002" }));
  assert.throws(() => types.fragmentedExportFieldsFrom({ ...exported, sessionTiming: {} }));
  assert.throws(() => types.fragmentedExportFieldsFrom({ ...exported, udpAddress: "127.0.0.1:35554" }));
  assert.throws(() => types.fragmentedExportFieldsFrom({ ...exported, expiresAtMs: Number.MAX_SAFE_INTEGER + 1 }));
});

test("UDP lifecycle projects negotiation only while available and rejects raw errors or fallback ports", () => {
  for (const value of [notStarted, { ...notStarted, udpState: "starting" }, available,
    ...["connect_udp_setup_failed", "connect_udp_capacity_unavailable", "connect_udp_closed"].map(udpErrorCode => ({ ...notStarted, udpState: "failed", udpErrorCode })),
    { ...notStarted, udpState: "closed" }, { ...notStarted, udpState: "closed", udpErrorCode: "connect_udp_closed" }]) {
    assert.deepEqual(types.fragmentedGuestUdpFrom(value), value);
  }
  for (const value of [{}, { ...notStarted, udpState: "ready" }, { ...available, udpAddress: "127.0.0.1:35555" },
    { ...available, udpOriginalCapacity: 4095 }, { ...available, udpCapacityScope: "allAssociations" },
    { ...notStarted, udpState: "failed" }, { ...notStarted, udpState: "failed", udpErrorCode: "PRIVATE_ERROR secret" },
    { ...notStarted, udpState: "starting", udpErrorCode: "connect_udp_setup_failed" },
    { ...available, udpState: "closed" }, { ...notStarted, udpAddress: undefined },
    { ...notStarted, sessionTiming: {} }, { ...notStarted, invitationProfile: "sessionGameTcp" }]) assert.throws(() => types.fragmentedGuestUdpFrom(value));
  assert.doesNotMatch(types.fragmentedUdpErrorText("PRIVATE_ERROR secret"), /PRIVATE_ERROR|secret/);
});

test("snapshot distinguishes host grants and guest bridge, while preparation and schema2 cannot mix", () => {
  const preparation = { invitationProfile: "fragmentedGameUdp" };
  types.fragmentedPreparationFieldsFrom(preparation);
  types.fragmentedSnapshotFieldsFrom(hostSnapshot);
  types.fragmentedSnapshotFieldsFrom(guestSnapshot);
  types.fragmentedSnapshotFieldsFrom({ ...hostSnapshot, phase: "hostPrepared", fragmentedUdp: undefined, preparation });
  types.fragmentedSnapshotFieldsFrom({ ...guestSnapshot, phase: "guestPrepared", fragmentedUdp: undefined, preparation, ...notStarted });
  types.fragmentedSnapshotFieldsFrom({ ...guestSnapshot, phase: "reviewed", review: guestReview, ...notStarted });
  types.fragmentedSnapshotFieldsFrom({ ...guestSnapshot, ...notStarted, udpState: "starting" });
  types.fragmentedSnapshotFieldsFrom({ ...guestSnapshot, phase: "closing" });
  types.fragmentedSnapshotFieldsFrom({ ...guestSnapshot, phase: "closed", ...notStarted, udpState: "closed" });
  for (const value of [{ ...guestSnapshot, invitationProfile: "sessionGameTcp" }, { ...guestSnapshot, sessionTiming: {} },
    { ...hostSnapshot, ...notStarted }, { ...guestSnapshot, fragmentedUdp: undefined },
    { ...guestSnapshot, phase: "guestPrepared" }, { ...guestSnapshot, phase: "closed" },
    { ...guestSnapshot, phase: "reviewed", ...available }, { ...guestSnapshot, phase: "idle" },
    { ...guestSnapshot, preparation: { invitationProfile: "sessionGameTcp" } },
    { ...guestSnapshot, review: hostReview }, { ...guestSnapshot, review: { ...guestReview, fragmentedUdp: undefined } }]) {
    assert.throws(() => types.fragmentedSnapshotFieldsFrom(value));
  }
  assert.throws(() => types.fragmentedPreparationFieldsFrom({ ...preparation, sessionTiming: {} }));
  assert.throws(() => types.fragmentedPreparationFieldsFrom({ ...preparation, fragmentedUdp: contract }));
});

let root;
const container = document.getElementById("root");
beforeEach(() => { root = createRoot(container); });
afterEach(async () => { await act(async () => root.unmount()); });
const render = async element => { await act(async () => root.render(element)); };
const edit = async (input, value) => { await act(async () => {
  Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set.call(input, value);
  input.dispatchEvent(new window.Event("input", { bubbles: true }));
}); };

test("controlled host fields emit only edited values and retain the reviewed bounds", async () => {
  const edits = [];
  function Controlled() {
    const [udpTarget, setTarget] = useState("127.0.0.1:43002");
    const [lifetimeSeconds, setLifetime] = useState("120");
    return createElement(FragmentedFields, { role: "host", disabled: false, udpTarget, lifetimeSeconds,
      onUdpTargetChange(value) { edits.push(["target", value]); setTarget(value); },
      onLifetimeChange(value) { edits.push(["lifetime", value]); setLifetime(value); } });
  }
  await render(createElement(Controlled));
  assert.deepEqual(edits, [], "Rendering is not approval or a native operation");
  const inputs = [...container.querySelectorAll("input")];
  assert.equal(inputs.length, 2);
  assert.equal(inputs[1].getAttribute("min"), "1");
  assert.equal(inputs[1].getAttribute("max"), "120");
  await edit(inputs[0], "127.0.0.1:43003"); await edit(inputs[1], "30");
  assert.deepEqual(edits, [["target", "127.0.0.1:43003"], ["lifetime", "30"]]);
  assert.equal(inputs[0].value, "127.0.0.1:43003");
  assert.equal(inputs[1].value, "30");
  assert.match(container.textContent, /Anúncio e configuração dos mods exigem plano e aprovação separados/);
  assert.match(container.textContent, /áudio não testado/);
  assert.equal(container.querySelectorAll("button").length, 0);
});

test("guest fields offer no destination override and disabled host fields cannot invite editing", async () => {
  const props = { disabled: true, udpTarget: "127.0.0.1:43002", lifetimeSeconds: "120",
    onUdpTargetChange() { assert.fail("Unexpected edit"); }, onLifetimeChange() { assert.fail("Unexpected edit"); } };
  await render(createElement(FragmentedFields, { ...props, role: "guest" }));
  assert.equal(container.querySelectorAll("input").length, 0);
  assert.match(container.textContent, /127\.0\.0\.1:35554/);
  await render(createElement(FragmentedFields, { ...props, role: "host" }));
  assert.equal([...container.querySelectorAll("input")].every(input => input.disabled), true);
});

test("review shows explicit downstream budget and separates transport from voice and recovery", async () => {
  await render(createElement("dl", {}, createElement(FragmentedReviewFields, { review: hostReview.fragmentedUdp })));
  assert.match(container.textContent, /127\.0\.0\.1:43002/);
  assert.match(container.textContent, /16384 bytes por segundo, incluindo os cabeçalhos da ponte, e 20 fragmentos por segundo; orçamento compartilhado pelo serviço/);
  assert.doesNotMatch(container.textContent, /20 datagramas por segundo/);
  assert.match(container.textContent, /4096 bytes/);
  assert.match(container.textContent, /plano, diff, autorização e recuperação separados/);
  assert.doesNotMatch(container.textContent, /voz disponível|áudio disponível|mod verificado/i);
  await render(createElement("dl", {}, createElement(FragmentedReviewFields, { review: guestReview.fragmentedUdp })));
  assert.doesNotMatch(container.textContent, /Destino UDP autorizado|43002/);
});

test("UDP starting/failure/closure never renders a TCP action or a false mod-ready claim", async () => {
  for (const udp of [{ ...notStarted, udpState: "starting" }, { ...notStarted, udpState: "failed", udpErrorCode: "connect_udp_setup_failed" },
    { ...notStarted, udpState: "closed", udpErrorCode: "connect_udp_closed" }]) {
    await render(createElement("dl", {}, createElement(FragmentedStatusFields, { udp })));
    assert.equal(container.querySelectorAll("button,input").length, 0);
    assert.doesNotMatch(container.textContent, /127\.0\.0\.1:35554|Ponte UDP disponível|voz disponível|jogador conectado/i);
    assert.match(container.textContent, /Anúncio e configuração dos modsNão verificadosÁudioNão testado/);
  }
  await render(createElement("dl", {}, createElement(FragmentedStatusFields, { udp: available })));
  assert.match(container.textContent, /Ponte UDP disponível/);
  assert.match(container.textContent, /primeira associação/);
  await render(createElement("dl", {}, createElement(FragmentedStatusFields)));
  assert.doesNotMatch(container.textContent, /127\.0\.0\.1:35554|4096 bytes/);
  assert.match(container.textContent, /ponte local pertence ao convidado/);
});
