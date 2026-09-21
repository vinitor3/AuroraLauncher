import assert from "node:assert/strict";
import { createHash, createPublicKey, verify } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const matrix = JSON.parse(readFileSync(join(repository, "scripts", "active-mod-matrix.json"), "utf8"));
const version = matrix.modules.find((module) => module.name === "core").version;
const manifest = JSON.parse(readFileSync(join(repository, "apps", "aurora-core", `compatibility-manifest-${version}.json`), "utf8"));
const trusted = readFileSync(join(repository, "apps", "aurora-core", `trusted-public-key-${version}.txt`), "utf8").trim();
assert.equal(manifest.schemaVersion, 1);
assert.equal(manifest.builds.length, matrix.targets.length);
for (const target of matrix.targets) {
  const builds = manifest.builds.filter((build) => build.minecraft === target.minecraft && build.loader === target.loader);
  assert.equal(builds.length, 1, `Core target ${target.minecraft}/${target.loader}`);
  const build = builds[0];
  assert.equal(build.auroraCoreVersion, version);
  assert.equal(build.javaRuntime, target.javaRuntime);
  assert.equal(build.buildJdk, target.buildJdk);
  const filename = `aurora-core-${target.loader}-${target.minecraft}-${version}.jar`;
  assert.equal(build.auroraCoreBuild, filename);
  assert.equal(build.download, `releases/core/${version}/${target.minecraft}/${target.loader}/${filename}`);
  const bytes = readFileSync(join(repository, build.download));
  assert.equal(bytes.length, build.size);
  const hash = createHash("sha256").update(bytes).digest("hex");
  assert.equal(hash, build.sha256);
  assert.equal(build.signature.algorithm, "Ed25519");
  assert.equal(manifest.signaturePublicKeys[build.signature.keyId], trusted);
  const publicKey = createPublicKey({
    key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), Buffer.from(trusted, "base64")]),
    format: "der", type: "spki",
  });
  assert(verify(null, Buffer.from(`aurora-core:v1:${hash}`), publicKey, Buffer.from(build.signature.value, "base64")), filename);
}
console.log(`[Aurora Core] ${manifest.builds.length} hashes, sizes, targets and Ed25519 signatures verified.`);
