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
    { schemaVersion: "1", state: "direct" }, { schemaVersion: 3, state: "direct" }, { schemaVersion: 1, state: 1 },
    { schemaVersion: 1, state: "mixed" }, { schemaVersion: 1, state: "DIRECT" },
    ...["ip", "url", "peer", "rtt", "timestamp", "relayEnabled"].map(key => ({ schemaVersion: 1, state: "direct", [key]: "untrusted" })),
    Object.assign(Object.create({ schemaVersion: 1 }), { state: "direct" })]) assert.throws(() => pathObservationFrom(value));
});

test("schema two carries an optional bounded transport estimate only on an observed path", () => {
  for (const [state, label] of Object.entries(labels)) {
    const withoutEstimate = { schemaVersion: 2, state };
    assert.equal(pathObservationFrom(withoutEstimate), withoutEstimate);
    assert.equal(pathObservationText(withoutEstimate), label);
    if (state === "unknown" || state === "closed") {
      assert.throws(() => pathObservationFrom({ ...withoutEstimate, rttEstimateMicros: 1000 }));
      // Even an unparsed object must not display a stale terminal estimate.
      assert.equal(pathObservationText({ ...withoutEstimate, rttEstimateMicros: 1000 }), label);
    } else {
      for (const [rttEstimateMicros, text] of [[1, "< 1"], [999, "< 1"], [1000, "1"], [24_567, "25"], [4_294_967_295, "4.294.967"]]) {
        const observation = { ...withoutEstimate, rttEstimateMicros };
        assert.equal(pathObservationFrom(observation), observation);
        assert.equal(pathObservationText(observation), `${label} · RTT estimado do transporte: ${text} ms`);
        assert.doesNotMatch(pathObservationText(observation), /Minecraft|voz|ping|medido|fresco/i);
      }
    }
  }
});

test("RTT schema rejects inherited, undefined, noninteger, nonfinite and out-of-range fields", () => {
  for (const state of ["direct", "relay"]) {
    for (const rttEstimateMicros of [undefined, null, false, true, "1000", {}, [], -1, -0, 0, 0.5, 1000.5, NaN, Infinity, -Infinity, 4_294_967_296, Number.MAX_SAFE_INTEGER, 1n]) {
      const observation = { schemaVersion: 2, state, rttEstimateMicros };
      assert.throws(() => pathObservationFrom(observation));
      assert.equal(pathObservationText(observation), labels[state]);
    }
    assert.throws(() => pathObservationFrom({ schemaVersion: 1, state, rttEstimateMicros: 1000 }));
    assert.throws(() => pathObservationFrom(Object.assign(Object.create({ rttEstimateMicros: 1000 }), { schemaVersion: 2, state })));
    for (const key of ["ip", "url", "peer", "rtt", "timestamp", "relayEnabled"]) {
      assert.throws(() => pathObservationFrom({ schemaVersion: 2, state, [key]: "untrusted" }));
      assert.throws(() => pathObservationFrom({ schemaVersion: 2, state, rttEstimateMicros: 1000, [key]: "untrusted" }));
    }
  }
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

test("an estimated RTT never relaxes the live snapshot role, ownership or lifecycle gates", () => {
  const pathObservation = { schemaVersion: 2, state: "direct", rttEstimateMicros: 150_000 };
  for (const [role, phase] of [["host", "hostListening"], ["guest", "guestConnected"]]) {
    const value = { available: true, operationId: "owned-operation", role, phase, pathObservation };
    assert.doesNotThrow(() => snapshotPathObservationFrom(value));
    for (const changed of [{ role: role === "host" ? "guest" : "host" }, { available: false }, { operationId: "" },
      { phase: "starting" }, { phase: "closing" }, { phase: "closed" }, { phase: "failed" }, { hostGroup: {} }]) {
      assert.throws(() => snapshotPathObservationFrom({ ...value, ...changed }));
    }
  }
});
