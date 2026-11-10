import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import ts from "typescript";

async function fixture(mode, flags = {}) {
  const calls = [];
  const key = `socialSdk${crypto.randomUUID().replaceAll("-", "")}`;
  globalThis[key] = calls;
  let source = ts.transpileModule(await readFile(new URL("./firebase.ts", import.meta.url), "utf8"), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  }).outputText.replaceAll("import.meta.env", JSON.stringify({ MODE: mode, ...flags }));
  source = source.replace(/import\s*\{([^}]+)\}\s*from\s*["']([^"']+)["'];?/g, (_, names, origin) => {
    const body = names.split(",").map(v => v.trim()).map(name => {
      if (name === "validateLauncherAuthScope") return `export const ${name}=value=>value;`;
      if (name.endsWith("Persistence")) return `export const ${name}={};`;
      return `export function ${name}(...args){globalThis[${JSON.stringify(key)}].push({name:${JSON.stringify(name)},args});return {origin:${JSON.stringify(origin)}};}`;
    }).join("\n");
    return `import {${names}} from "data:text/javascript;base64,${Buffer.from(body).toString("base64")}";`;
  });
  const moduleUrl = `data:text/javascript;base64,${Buffer.from(source).toString("base64")}#${key}`;
  const module = await import(moduleUrl);
  return { module, moduleUrl, calls, close() { delete globalThis[key]; } };
}

test("social build pins only emulator demo and strips caller remote service fields", async () => {
  const f = await fixture("social-lab");
  try {
    const config = { ...f.module.bundledFirebaseConfig, apiKey: "untrusted", workerUrl: "https://untrusted.invalid", storageBucket: "untrusted" };
    const services = f.module.createFirebaseServices(config, { id: "test", persistent: false });
    assert.equal(services.config.projectId, "demo-aurora-social");
    assert.equal(services.config.apiKey, "demo-aurora-social");
    assert.equal(services.config.workerUrl, undefined);
    assert.equal(services.config.storageBucket, "demo-aurora-social.invalid");
    assert.equal(f.calls.filter(c => c.name === "connectAuthEmulator").length, 1);
    assert.equal(f.calls.find(c => c.name === "connectAuthEmulator").args[1], "http://127.0.0.1:9187");
    assert.deepEqual(f.calls.find(c => c.name === "connectFirestoreEmulator").args.slice(1), ["127.0.0.1", 8187]);
  } finally { f.close(); }
});

test("social build rejects a real or saved different project before SDK initialization", async () => {
  const f = await fixture("social-lab");
  try {
    assert.throws(() => f.module.createFirebaseServices({ projectId: "auroralauncher" }, { id: "test", persistent: true }));
    assert.equal(f.calls.length, 0);
  } finally { f.close(); }
});

test("normal build keeps normal project and does not connect to emulators", async () => {
  const f = await fixture("production");
  try {
    assert.equal(f.module.socialLabBuild, false);
    assert.equal(f.module.socialFeatureBuild, false);
    assert.equal(f.module.bundledFirebaseConfig.projectId, "auroralauncher");
    f.module.createFirebaseServices(f.module.bundledFirebaseConfig, { id: "test", persistent: true });
    assert.equal(f.calls.some(c => c.name.includes("Emulator")), false);
  } finally { f.close(); }
});
test("online experiment exposes social UI with real account configuration and no emulator fallback", async () => {
  const f = await fixture("connect-online");
  try {
    assert.equal(f.module.socialLabBuild, false);
    assert.equal(f.module.socialFeatureBuild, true);
    assert.equal(f.module.bundledFirebaseConfig.projectId, "auroralauncher");
    f.module.createFirebaseServices(f.module.bundledFirebaseConfig, { id: "online-test", persistent: false });
    assert.equal(f.calls.some(c => c.name.includes("Emulator")), false);
  } finally { f.close(); }
});

test("private online configuration requires the exact opt-in, mode, origin and every official Firebase field", async () => {
  const flags = { VITE_AURORA_SOCIAL_PRIVATE_ONLINE: "1", VITE_AURORA_SOCIAL_ORIGIN: "https://aurora-connect-social.aurora-edge-proxy.workers.dev" };
  for (const mode of ["production", "social-lab", "connect-online"]) {
    const f = await fixture(mode, flags);
    try {
      const config = f.module.bundledFirebaseConfig;
      assert.equal(f.module.privateSocialOnlineConfiguration(config), mode === "connect-online");
      if (mode === "connect-online") for (const key of ["projectId", "authDomain", "apiKey", "appId", "messagingSenderId", "storageBucket", "workerUrl"]) {
        assert.equal(f.module.privateSocialOnlineConfiguration({ ...config, [key]: "different" }), false, key);
      }
      assert.equal(f.calls.length, 0);
    } finally { f.close(); }
  }
  for (const overrides of [{ VITE_AURORA_SOCIAL_PRIVATE_ONLINE: undefined }, { VITE_AURORA_SOCIAL_PRIVATE_ONLINE: "true" },
    { VITE_AURORA_SOCIAL_PRIVATE_ONLINE: " 1" }, { VITE_AURORA_SOCIAL_PRIVATE_ONLINE: true }, { VITE_AURORA_SOCIAL_PRIVATE_ONLINE: "0" },
    { VITE_AURORA_SOCIAL_ORIGIN: flags.VITE_AURORA_SOCIAL_ORIGIN + "/" }, { VITE_AURORA_SOCIAL_ORIGIN: "https://other.invalid" }]) {
    const f = await fixture("connect-online", { ...flags, ...overrides });
    try { assert.equal(f.module.privateSocialOnlineConfiguration(f.module.bundledFirebaseConfig), false); assert.equal(f.calls.length, 0); }
    finally { f.close(); }
  }
});

for (const [mode, expected] of [["social-lab", "TESTE LOCAL · CONTAS DESCARTÁVEIS · SEM PUBLICAÇÃO NA COMUNIDADE REAL"],
  ["connect-online", "VERSÃO EXPERIMENTAL · SERVIÇO ONLINE"]]) {
  test(`social banner describes the actual ${mode} build`, async () => {
    const f = await fixture(mode);
    try {
      const source = ts.transpileModule(await readFile(new URL("../features/social/SocialBuildBanner.tsx", import.meta.url), "utf8"), {
        fileName: "SocialBuildBanner.tsx", compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX },
      }).outputText.replace('"../../lib/firebase"', JSON.stringify(f.moduleUrl))
        .replace('"react/jsx-runtime"', JSON.stringify(import.meta.resolve("react/jsx-runtime")));
      const { SocialBuildBanner } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
      const rendered = SocialBuildBanner();
      assert.equal(rendered.type, "p"); assert.equal(rendered.props.children, expected);
      if (mode === "connect-online") assert.doesNotMatch(rendered.props.children, /LOCAL|DESCARTÁVEIS|SEM PUBLICAÇÃO/);
      assert.equal(f.calls.length, 0, "rendering the notice initializes no Firebase client");
    } finally { f.close(); }
  });
}

test("native social lab permits catalog artwork without permitting remote social data endpoints", async () => {
  const config = JSON.parse(await readFile(new URL("../../src-tauri/tauri.social-lab.conf.json", import.meta.url), "utf8"));
  const directives = Object.fromEntries(config.app.security.csp.split(";").map(v => v.trim().split(/\s+/)).filter(v => v[0]).map(([key, ...values]) => [key, values]));
  assert.deepEqual(directives["img-src"], ["'self'", "data:", "blob:", "https://cdn.modrinth.com", "https://media.forgecdn.net", "https://mediafilez.forgecdn.net"]);
  assert.deepEqual(directives["connect-src"], ["'self'", "ipc:", "http://ipc.localhost", "http://localhost:1437", "http://127.0.0.1:8187", "http://127.0.0.1:9187"]);
  assert.equal(config.identifier, "com.aurora.smartlauncher.sociallab");
  assert.equal(config.bundle.active, false);
});
