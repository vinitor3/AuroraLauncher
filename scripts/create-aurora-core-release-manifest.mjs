import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const version = "1.1.2";
const manifestPath = join(repository, "apps", "aurora-core", `compatibility-manifest-${version}.json`);
const publicKeyPath = join(repository, "apps", "aurora-core", `trusted-public-key-${version}.txt`);

if (existsSync(manifestPath) || existsSync(publicKeyPath)) {
  throw new Error(`A release ${version} já possui manifesto/chave; releases são imutáveis.`);
}

const matrix = [
  ["1.12.2", "forge", "14.23.5.2860", "[14.23.5,15)", 8, 17],
  ["1.16.5", "fabric", "0.13.3", ">=0.13.3", 8, 17],
  ["1.16.5", "forge", "36.2.42", "[36,)", 8, 17],
  ["1.19.2", "fabric", "0.16.14", ">=0.14.0", 17, 17],
  ["1.19.2", "forge", "43.4.16", "[43,)", 17, 17],
  ["1.20.1", "fabric", "0.16.14", ">=0.14.0", 17, 17],
  ["1.20.1", "forge", "47.4.23", "[47,)", 17, 17],
  ["1.21.1", "fabric", "0.16.14", ">=0.16.10", 21, 21],
  ["1.21.1", "forge", "52.1.12", "[52,)", 21, 21],
];

const keyId = `aurora-core-${version}-release`;
const { publicKey, privateKey } = generateKeyPairSync("ed25519");
const publicDer = publicKey.export({ type: "spki", format: "der" });
const publicBase64 = publicDer.subarray(publicDer.length - 32).toString("base64");

const builds = matrix.map(([minecraft, loader, loaderVersion, loaderRange, javaRuntime, buildJdk]) => {
  const filename = `aurora-core-${loader}-${minecraft}-${version}.jar`;
  const relative = join("releases", "core", version, minecraft, loader, filename);
  const absolute = join(repository, relative);
  if (!existsSync(absolute)) throw new Error(`Artefato ausente: ${relative}`);
  const bytes = readFileSync(absolute);
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  const signature = sign(null, Buffer.from(`aurora-core:v1:${sha256}`), privateKey).toString("base64");
  return {
    minecraft,
    loader,
    loaderVersion,
    loaderVersionRange: loaderRange,
    javaRuntime,
    buildJdk,
    auroraCoreVersion: version,
    auroraCoreBuild: filename,
    download: relative.replaceAll("\\", "/"),
    size: statSync(absolute).size,
    sha256,
    signature: { algorithm: "Ed25519", keyId, value: signature },
  };
});

const manifest = {
  schemaVersion: 1,
  generatedAt: new Date().toISOString(),
  signaturePublicKeys: { [keyId]: publicBase64 },
  builds,
};

writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, { flag: "wx" });
writeFileSync(publicKeyPath, publicBase64, { flag: "wx" });
process.stdout.write(`${manifestPath}\n${publicKeyPath}\n`);
