import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";

// Run the real configuration and factory without initializing Firebase or reading
// local account data. import.meta.env is the only replaced build-time expression.
const source = readFileSync(new URL("../src/lib/firebase.ts", import.meta.url), "utf8");
function setup(buildEnv = {}) {
  const exports = {}, initialized = [];
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
    require: () => ({ initializeApp: (config, name) => { initialized.push({ config, name }); return {}; },
      getAuth: () => ({}), getFirestore: () => ({}), getStorage: () => ({}) }),
  });
  return { ...exports, initialized };
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
