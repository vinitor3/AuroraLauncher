import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";
import * as scopeModule from "../src/lib/launcherAuthScope.ts";

// Run the real configuration and factory without initializing Firebase or reading
// local account data. import.meta.env is the only replaced build-time expression.
const source = readFileSync(new URL("../src/lib/firebase.ts", import.meta.url), "utf8");
function setup(buildEnv = {}) {
  const exports = {}, initialized = [], authInitializations = [];
  const sdk = {
    initializeApp: (config, name) => { initialized.push({ config, name }); return { name }; },
    initializeAuth: (app, options) => { authInitializations.push({ app, options }); return { app }; },
    indexedDBLocalPersistence: "INDEXED_DB", browserLocalPersistence: "LOCAL",
    browserSessionPersistence: "SESSION", inMemoryPersistence: "MEMORY",
    getFirestore: () => ({}), getStorage: () => ({}),
  };
  const code = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
    transformers: { before: [(context) => {
      const visit = (node) => {
        if (ts.isPropertyAccessExpression(node) && node.name.text === "env" && ts.isMetaProperty(node.expression)) {
          return ts.factory.createIdentifier("__buildEnv");
        }
        return ts.visitEachChild(node, visit, context);
      };
      return (file) => ts.visitNode(file, visit);
    }] },
  }).outputText;
  vm.runInNewContext(code, { exports, __buildEnv: buildEnv,
    require: (name) => name === "./launcherAuthScope" ? scopeModule : sdk,
  });
  return { ...exports, initialized, authInitializations,
    createFirebaseServices: (config, scope = { id: "primary", persistent: true }) => exports.createFirebaseServices(config, scope) };
}

const officialWorker = "https://aurora-api.aurora-edge-proxy.workers.dev";

test("legacy official config without workerUrl recovers the bundled production endpoint", () => {
  const run = setup();
  const { workerUrl: _oldMissingField, ...legacy } = run.bundledFirebaseConfig;
  assert.equal(run.createFirebaseServices(legacy).config.workerUrl, officialWorker);
  assert.equal(run.createFirebaseServices({ ...legacy, workerUrl: " \t " }).config.workerUrl, officialWorker);
  assert.equal(run.initialized[0].config.workerUrl, officialWorker);
  assert.equal(legacy.workerUrl, undefined, "The caller's saved public configuration is not mutated");
});

test("an explicit saved Worker is trimmed and takes precedence over the build override", () => {
  const run = setup({ VITE_AURORA_API_URL: " https://build-worker.example/ " });
  const result = run.createFirebaseServices({ ...run.bundledFirebaseConfig, workerUrl: " https://saved-worker.example/ " });
  assert.equal(result.config.workerUrl, "https://saved-worker.example/");
});

test("the build override is recovered only for its matching Firebase project", () => {
  const run = setup({ VITE_AURORA_API_URL: " https://build-worker.example/ " });
  assert.equal(run.createFirebaseServices({ projectId: "auroralauncher" }).config.workerUrl, "https://build-worker.example/");
  assert.equal(run.createFirebaseServices({ projectId: "another-project" }).config.workerUrl, undefined);
  assert.equal(run.createFirebaseServices({ projectId: "another-project", workerUrl: " https://custom.example " }).config.workerUrl, "https://custom.example");
});

test("custom Firebase builds require an explicit Worker and never inherit Aurora production by default", () => {
  const unconfigured = setup({ VITE_FIREBASE_PROJECT_ID: "custom-project" });
  assert.equal(unconfigured.bundledFirebaseConfig.workerUrl, undefined);
  assert.equal(unconfigured.createFirebaseServices({ projectId: "custom-project" }).config.workerUrl, undefined);
  const configured = setup({ VITE_FIREBASE_PROJECT_ID: "custom-project", VITE_AURORA_API_URL: " https://custom.example " });
  assert.equal(configured.createFirebaseServices({ projectId: "custom-project", workerUrl: " " }).config.workerUrl, "https://custom.example");
  assert.equal(configured.createFirebaseServices({ projectId: "auroralauncher" }).config.workerUrl, undefined);
});

test("the primary window preserves the old Firebase app identity and persistence migration order", () => {
  const run = setup();
  const services = run.createFirebaseServices(run.bundledFirebaseConfig);
  assert.equal(services.app.name, "aurora-auroralauncher");
  assert.deepEqual(Array.from(run.authInitializations[0].options.persistence), ["INDEXED_DB", "LOCAL", "SESSION", "MEMORY"]);
  assert.equal(services.authScope.persistent, true);
});

test("independent windows never load or migrate the primary or another window's credentials", () => {
  const run = setup();
  const first = run.createFirebaseServices(run.bundledFirebaseConfig);
  const second = run.createFirebaseServices(run.bundledFirebaseConfig, { id: "window-2", persistent: false });
  const third = run.createFirebaseServices(run.bundledFirebaseConfig, { id: "window-3", persistent: false });
  assert.equal(new Set([first.app.name, second.app.name, third.app.name]).size, 3);
  assert.deepEqual(Array.from(run.authInitializations[1].options.persistence), ["SESSION", "MEMORY"]);
  assert.deepEqual(Array.from(run.authInitializations[2].options.persistence), ["SESSION", "MEMORY"]);
  const reload = run.createFirebaseServices(run.bundledFirebaseConfig, { id: "window-2", persistent: false });
  assert.equal(reload.app.name, second.app.name, "Reload within a window must find its session again");
  const reopening = run.createFirebaseServices(run.bundledFirebaseConfig, { id: "new-process", persistent: false });
  assert.notEqual(reopening.app.name, second.app.name, "A later process must start a fresh secondary login");
});

test("invalid scope is rejected before Firebase touches account persistence", () => {
  const run = setup();
  assert.throws(() => run.createFirebaseServices(run.bundledFirebaseConfig, { id: "x", persistent: "true" }));
  assert.equal(run.initialized.length, 0);
  assert.equal(run.authInitializations.length, 0);
});
