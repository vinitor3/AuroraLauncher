import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { desktopReleasePlan, runDesktopRelease } from "./build-desktop-release.mjs";

const config = { identifier: "com.aurora.smartlauncher", productName: "Aurora Smart Launcher", bundle: { active: true, targets: "nsis" },
  app: { security: { csp: "connect-src 'self' https://aurora-connect-social.aurora-edge-proxy.workers.dev" } } };
test("official build keeps identity, release and paired online flags without a lab config", () => {
  const plan = desktopReleasePlan([], {}, config);
  assert.equal(plan.options.env.AURORA_SOCIAL_PRIVATE_ONLINE, "1");
  assert.equal(plan.options.env.VITE_AURORA_SOCIAL_PRIVATE_ONLINE, "1");
  assert.equal(plan.options.env.AURORA_SOCIAL_ORIGIN, "https://aurora-connect-social.aurora-edge-proxy.workers.dev");
  assert.equal(plan.options.env.VITE_AURORA_SOCIAL_ORIGIN, plan.options.env.AURORA_SOCIAL_ORIGIN);
  assert.equal(plan.args[plan.args.indexOf("--features") + 1], "connect-online");
  assert.deepEqual(JSON.parse(plan.args[plan.args.indexOf("--config") + 1]), { build: { beforeBuildCommand: "npm run connect:online:frontend" } });
  assert.ok(plan.args.includes("--locked") && plan.args.includes("--offline"));
  assert.ok(!plan.args.includes("--debug") && !plan.args.includes("--no-bundle"));
  assert.ok(!plan.args.some(value => /tauri\.(social|connect)-lab|connect-online\.conf/.test(value)));
  assert.equal(plan.options.env.CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS, "false");
});
test("inherited lab and service overrides cannot change the official package", () => {
  const env = { AURORA_SOCIAL_AUTHORITY_PIN: "untrusted", AURORA_SOCIAL_AUTHORITY_ISSUER: "untrusted",
    AURORA_SOCIAL_PRIVATE_LAB: "1", AURORA_SOCIAL_DESKTOP_FIXTURE: "1", AURORA_SOCIAL_PRIVATE_ONLINE: "0",
    AURORA_SOCIAL_ORIGIN: "http://127.0.0.1:1", VITE_FIREBASE_PROJECT_ID: "demo-aurora-social",
    FIREBASE_AUTH_EMULATOR_HOST: "127.0.0.1:9187", FIRESTORE_EMULATOR_HOST: "127.0.0.1:8187",
    aurora_social_origin: "http://other.invalid", vite_firebase_api_key: "other-app",
    Tauri_Config: "untrusted", TAURI_CONFIG: "untrusted", OTHER: "kept" };
  const result = desktopReleasePlan([], env, config).options.env;
  for (const key of ["AURORA_SOCIAL_AUTHORITY_PIN", "AURORA_SOCIAL_AUTHORITY_ISSUER", "AURORA_SOCIAL_PRIVATE_LAB",
    "AURORA_SOCIAL_DESKTOP_FIXTURE", "FIREBASE_AUTH_EMULATOR_HOST", "FIRESTORE_EMULATOR_HOST",
    "aurora_social_origin", "vite_firebase_api_key", "Tauri_Config", "TAURI_CONFIG"]) assert.equal(result[key], undefined);
  assert.equal(result.VITE_FIREBASE_PROJECT_ID, "auroralauncher");
  assert.equal(result.OTHER, "kept");
  assert.equal(env.AURORA_SOCIAL_PRIVATE_ONLINE, "0");
});
test("options and unsafe build environment are rejected before any process", () => {
  for (const args of [["--debug"], ["--config", "elsewhere"], ["--private-invitations"]]) {
    assert.throws(() => desktopReleasePlan(args, {}, config));
  }
  for (const env of [{ RUSTFLAGS: "--cfg debug_assertions" }, { RustFlags: "--cfg debug_assertions" },
    { CARGO_ENCODED_RUSTFLAGS: "--cfg\x1fdebug_assertions" }, { NODE_TLS_REJECT_UNAUTHORIZED: "0" }, { Node_TLS_Reject_Unauthorized: "0" }]) {
    assert.throws(() => desktopReleasePlan([], env, config));
  }
  for (const changed of [{ ...config, identifier: "com.aurora.smartlauncher.connectonline" }, { ...config, bundle: { active: false, targets: "nsis" } }]) {
    assert.throws(() => desktopReleasePlan([], {}, changed));
  }
});
test("official runner executes only its reviewed plan and propagates a failed build", () => {
  let calls = 0;
  assert.equal(runDesktopRelease({ config, env: {}, spawn(executable, args, options) {
    calls++;
    const plan = desktopReleasePlan([], {}, config);
    assert.equal(executable, plan.executable); assert.deepEqual(args, plan.args); assert.deepEqual(options, plan.options);
    return { status: 23 };
  } }), 23);
  assert.equal(calls, 1);
  assert.throws(() => runDesktopRelease({ config, env: {}, spawn() { return { error: new Error("private detail") }; } }), /^Error: Não foi possível iniciar o build oficial\.$/);
});
test("official npm path preserves both preflight and the complete mod matrix gate", () => {
  const root = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  assert.equal(root.scripts["desktop:build"], "npm run release:verify-mods && npm run release:preflight && node scripts/build-desktop-release.mjs");
  const wrapper = readFileSync(new URL("./package-release.ps1", import.meta.url), "utf8");
  assert.ok(wrapper.includes("@('run', 'desktop:build')"));
  assert.ok(wrapper.includes("'Preflight final'"));
});
test("official CSP permits the exact Social HTTPS origin while retaining existing endpoints", () => {
  const official = JSON.parse(readFileSync(new URL("../apps/desktop/src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  desktopReleasePlan([], {}, official);
  const directives = official.app.security.csp.split(";").map(value => value.trim().split(/\s+/));
  const connect = directives.find(parts => parts[0] === "connect-src");
  assert.deepEqual(connect, ["connect-src", "'self'", "ipc:", "http://ipc.localhost", "http://localhost:1420",
    "https://*.googleapis.com", "https://*.firebaseio.com", "wss://*.firebaseio.com", "https://api.modrinth.com",
    "https://aurora-api.aurora-edge-proxy.workers.dev", "https://aurora-connect-social.aurora-edge-proxy.workers.dev", "https://minotar.net"]);
  // The relay WebSocket belongs to the native transport, not the WebView.
  assert.ok(!connect.includes("wss://aurora-connect-social.aurora-edge-proxy.workers.dev"));
  for (const csp of ["", "connect-src 'self'", "connect-src 'self'; img-src https://aurora-connect-social.aurora-edge-proxy.workers.dev",
    "connect-src 'self'; connect-src https://aurora-connect-social.aurora-edge-proxy.workers.dev"]) {
    assert.throws(() => desktopReleasePlan([], {}, { ...official, app: { security: { csp } } }), /política de conexão/);
  }
});
