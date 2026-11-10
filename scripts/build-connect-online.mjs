// Builds an isolated executable; never creates an installer or publishes services.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const desktop = fileURLToPath(new URL("../apps/desktop/", import.meta.url));
// Keep the diagnostic build and its assertions, but avoid spending the launch
// review window hashing the full installed runtime with unoptimized crypto.
// Every file is still read and verified again immediately before launch.
const hashBuildOptions = ["sha1", "sha2", "digest", "block-buffer", "crypto-common"]
  .flatMap(name => ["--config", `profile.dev.package.${name}.opt-level=3`]);
const result = spawnSync(process.execPath, [join(desktop, "node_modules/@tauri-apps/cli/tauri.js"),
  "build", "--debug", "--no-bundle", "--features", "connect-lab,connect-voice-lab",
  "--config", "src-tauri/tauri.connect-online.conf.json", "--", "--offline", "--locked", "--jobs", "2", ...hashBuildOptions], {
  cwd: desktop, stdio: "inherit", windowsHide: true,
  env: { ...process.env, AURORA_SOCIAL_ORIGIN: "https://aurora-connect-social.aurora-edge-proxy.workers.dev" },
});
if (result.error) { console.error("Não foi possível iniciar o build experimental."); process.exitCode = 1; }
else process.exitCode = result.status ?? 1;
