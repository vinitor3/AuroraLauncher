import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const matrix = JSON.parse(readFileSync(join(root, "scripts/active-mod-matrix.json"), "utf8"));
const version = matrix.modules.find((module) => module.name === "skins").version;
const core = matrix.modules.find((module) => module.name === "core").version;
const release = join(root, "releases/skins", version);
const manifest = join(release, "manifest.json");
const artifacts = matrix.targets.map((target) => {
  const { minecraft, loader, javaRuntime, loaderVersion } = target;
  const path = `${minecraft}/${loader}/aurora-skins-${loader}-${minecraft}-${version}.jar`;
  const bytes = readFileSync(join(release, path));
  return {
    path, minecraft, loader, javaRuntime,
    sha256: createHash("sha256").update(bytes).digest("hex"),
    size: bytes.length, selectedLoaderVersion: loaderVersion,
    coreMinimum: target.skinsCoreMinimum ?? "1.1.3",
    buildLoaderVersion: target.buildLoaderVersion ?? loaderVersion,
  };
});

if (process.argv[2] === "--create" && process.argv.length === 3) {
  const metadata = {
    version, coreMinimum: "1.1.3", coreBundled: core, serverRequiresCore: false,
    protocol: 1, evidence: "docs/engineering/COMPATIBILITY_MATRIX.md", artifacts,
  };
  writeFileSync(manifest, `${JSON.stringify(metadata, null, 2)}\n`, { flag: "wx" });
} else {
  assert.equal(process.argv.length, 2, "Expected no arguments or --create");
  const recorded = JSON.parse(readFileSync(manifest, "utf8"));
  assert.equal(recorded.version, version);
  assert.equal(recorded.coreBundled, core);
  assert.deepEqual(recorded.artifacts, artifacts);
  console.log(`[Aurora Skins] ${artifacts.length} hashes, sizes and targets verified.`);
}
