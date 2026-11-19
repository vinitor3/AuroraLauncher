// Official distribution entry point, called only after the existing matrix/preflight gates.
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { connectOnlineEnvironment } from "./connect-online-build-options.mjs";

const desktop = fileURLToPath(new URL("../apps/desktop/", import.meta.url));
const officialIdentity = "com.aurora.smartlauncher";
const frontendOnlyOverride = { build: { beforeBuildCommand: "npm run connect:online:frontend" } };

export function desktopReleasePlan(args, inherited = {}, config) {
  if (args.length !== 0) throw new Error("O build oficial não aceita opções adicionais.");
  if (config?.identifier !== officialIdentity || config.productName !== "Aurora Smart Launcher"
      || config.bundle?.active !== true || config.bundle.targets !== "nsis") {
    throw new Error("Configuração oficial do instalador divergente.");
  }
  const csp = config.app?.security?.csp;
  const connectSources = typeof csp === "string"
    ? csp.split(";").map(directive => directive.trim().split(/\s+/)).find(parts => parts[0] === "connect-src") : undefined;
  if (!connectSources?.includes("https://aurora-connect-social.aurora-edge-proxy.workers.dev")) {
    throw new Error("A política de conexão do instalador não permite o serviço Social oficial.");
  }
  // Arbitrary rustc arguments could re-enable debug-only fixture paths in a release.
  if (Object.entries(inherited).some(([key, value]) => ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"].includes(key.toUpperCase()) && value?.trim())) {
    throw new Error("O build oficial exige RUSTFLAGS e CARGO_ENCODED_RUSTFLAGS vazios.");
  }
  if (Object.entries(inherited).some(([key, value]) => key.toUpperCase() === "NODE_TLS_REJECT_UNAUTHORIZED" && value === "0")) {
    throw new Error("O build oficial exige verificação TLS.");
  }
  const clean = { ...inherited };
  for (const key of Object.keys(clean)) {
    const upper = key.toUpperCase(); // Windows environment names are case-insensitive.
    if (upper.startsWith("AURORA_SOCIAL_") || upper.startsWith("VITE_AURORA_")
        || upper.startsWith("VITE_FIREBASE_") || upper.endsWith("_EMULATOR_HOST")
        || ["TAURI_CONFIG", "NODE_ENV", "CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS"].includes(upper)) delete clean[key];
  }
  const env = connectOnlineEnvironment(["--private-invitations"], clean);
  // Public application identifiers, not credentials. Pin them even if a local
  // .env file or shell has configured a different development Firebase project.
  Object.assign(env, {
    NODE_ENV: "production",
    VITE_FIREBASE_API_KEY: "AIzaSyD4Sn9JkU0CGGz77Mj71QNwvYiFHPM-dvk",
    VITE_FIREBASE_AUTH_DOMAIN: "auroralauncher.firebaseapp.com",
    VITE_FIREBASE_PROJECT_ID: "auroralauncher",
    VITE_FIREBASE_STORAGE_BUCKET: "auroralauncher.firebasestorage.app",
    VITE_FIREBASE_MESSAGING_SENDER_ID: "246542594321",
    VITE_FIREBASE_APP_ID: "1:246542594321:web:ae2a8206d599f2a4c45cb0",
    VITE_AURORA_API_URL: "https://aurora-api.aurora-edge-proxy.workers.dev",
    CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS: "false",
  });
  return {
    executable: process.execPath,
    args: [join(desktop, "node_modules/@tauri-apps/cli/tauri.js"), "build", "--features", "connect-online",
      "--bundles", "nsis", "--config", JSON.stringify(frontendOnlyOverride),
      "--", "--offline", "--locked", "--jobs", "2", "--config", "profile.release.debug-assertions=false"],
    options: { cwd: desktop, stdio: "inherit", windowsHide: true, env },
  };
}

export function runDesktopRelease({ args = [], env = process.env, config, spawn = spawnSync } = {}) {
  const plan = desktopReleasePlan(args, env, config
    ?? JSON.parse(readFileSync(join(desktop, "src-tauri/tauri.conf.json"), "utf8")));
  const result = spawn(plan.executable, plan.args, plan.options);
  if (result.error) throw new Error("Não foi possível iniciar o build oficial.");
  return result.status ?? 1;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { process.exitCode = runDesktopRelease({ args: process.argv.slice(2) }); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
