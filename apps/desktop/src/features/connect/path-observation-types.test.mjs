// Parser and labels only; no transport, relay, account or personal settings.
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import ts from "typescript";

const source = await readFile(new URL("./path-observation-types.ts", import.meta.url), "utf8");
const output = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { pathObservationFrom, snapshotPathObservationFrom, pathObservationText } = await import(`data:text/javascript;base64,${Buffer.from(output).toString("base64")}`);
const labels = { unknown: "Não confirmado", direct: "Caminho direto observado", relay: "Relay observado", closed: "Conexão encerrada" };

test("path observation is an exact output schema with four redacted states", () => {
  for (const [state, label] of Object.entries(labels)) {
    const observation = { schemaVersion: 1, state };
    assert.equal(pathObservationFrom(observation), observation);
    assert.equal(pathObservationText(observation), label);
    assert.deepEqual(Object.keys(observation), ["schemaVersion", "state"]);
  }
  assert.equal(pathObservationText(), "Não confirmado");
});

test("path observation rejects unknown versions, absent fields and connectivity metadata", () => {
  for (const value of [null, undefined, false, 1, "direct", [], {}, { schemaVersion: 1 }, { state: "direct" },
    { schemaVersion: "1", state: "direct" }, { schemaVersion: 2, state: "direct" }, { schemaVersion: 1, state: 1 },
    { schemaVersion: 1, state: "mixed" }, { schemaVersion: 1, state: "DIRECT" },
    ...["ip", "url", "peer", "rtt", "timestamp", "relayEnabled"].map(key => ({ schemaVersion: 1, state: "direct", [key]: "untrusted" })),
    Object.assign(Object.create({ schemaVersion: 1 }), { state: "direct" })]) assert.throws(() => pathObservationFrom(value));
});

test("optional snapshot path is accepted only for the matching live single participant role", () => {
  for (const [role, phase] of [["host", "hostListening"], ["guest", "guestConnected"]]) {
    const value = { available: true, operationId: "owned-operation", role, phase };
    assert.doesNotThrow(() => snapshotPathObservationFrom(value));
    for (const state of Object.keys(labels)) assert.doesNotThrow(() => snapshotPathObservationFrom({ ...value, pathObservation: { schemaVersion: 1, state } }));
    for (const changed of [{ role: role === "host" ? "guest" : "host" }, { available: false }, { operationId: null },
      { phase: "starting" }, { phase: "closing" }, { phase: "closed" }, { phase: "failed" }, { phase: "reviewed" }, { hostGroup: {} }]) {
      assert.throws(() => snapshotPathObservationFrom({ ...value, ...changed, pathObservation: { schemaVersion: 1, state: "direct" } }));
    }
    assert.throws(() => snapshotPathObservationFrom({ ...value, pathObservation: undefined }));
    assert.throws(() => snapshotPathObservationFrom(Object.assign(Object.create({ pathObservation: { schemaVersion: 1, state: "direct" } }), value)));
  }
});
