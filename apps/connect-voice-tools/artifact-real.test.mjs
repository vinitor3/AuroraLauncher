import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { inspectArtifact, isVerifiedArtifact, readArtifactFile } from './artifact.mjs';
import { createPlan } from './inspect.mjs';

// Real third-party JARs are optional local inputs, never committed or auto-downloaded.
const directory = process.env.AURORA_CONNECT_ARTIFACT_DIR;
for (const [mod, filename, version, apiCount, fixture] of [
  ['svc', 'voicechat-fabric-1.21.1-2.6.22.jar', '2.6.22', 1, 'svc.properties'],
  ['plasmo', 'plasmovoice-fabric-1.21.1-2.1.17.jar', '2.1.17', 3, 'plasmo.toml'],
]) test(`real ${mod} bytes match pinned release, metadata and address API without executing a JAR`, { skip: !directory }, async () => {
  const bytes = await readArtifactFile(join(directory, filename));
  const artifact = inspectArtifact(bytes);
  assert.equal(isVerifiedArtifact(artifact), true);
  assert.equal(artifact.mod, mod); assert.equal(artifact.version, version); assert.equal(artifact.api.length, apiCount);
  assert.equal(artifact.runtimeHomologated, false);
  assert.equal(artifact.loader, 'fabric'); assert.equal(artifact.minecraft, '1.21.1');
  if (mod === 'svc') assert.equal(artifact.bundledApiVersion, '2.6.20');
  const config = await readFile(new URL(`./fixtures/${fixture}`, import.meta.url));
  const options = { mod, artifact, mode: 'dedicated-aurora-only', guestPort: 35554 };
  const plan = createPlan(options, config);
  assert.equal(plan.effectiveVersion, version); assert.equal(plan.artifactVerified, true); assert.equal(plan.declaredVersion, null);
  assert(plan.blockers.includes('voice_runtime_not_verified'));
  assert(!plan.blockers.includes('artifact_metadata_hash_and_runtime_not_verified'));
  assert(plan.changes.length > 0); assert.equal(plan.applySupported, false);
  for (const bad of [{ version: '9.9.9' }, { loader: 'forge' }, { minecraft: '1.20.1' }, { mod: mod === 'svc' ? 'plasmo' : 'svc' }]) {
    // Supply matching parser data when testing cross-mod artifact mismatch.
    const configForBad = bad.mod ? await readFile(new URL(`./fixtures/${bad.mod === 'svc' ? 'svc.properties' : 'plasmo.toml'}`, import.meta.url)) : config;
    const blocked = createPlan({ ...options, ...bad }, configForBad);
    assert.deepEqual(blocked.changes, []); assert(blocked.blockers.some(code => code.startsWith('artifact_')));
  }
  const forged = createPlan({ ...options, artifact: JSON.parse(JSON.stringify(artifact)) }, config);
  assert.equal(forged.artifactVerified, false); assert.deepEqual(forged.changes, []);
  // A legal ZIP timestamp change leaves metadata identical but loses release identity.
  const modified = Buffer.from(bytes);
  modified[10] ^= 1;
  const unpinned = inspectArtifact(modified);
  assert.equal(unpinned.recognizedArtifact, false);
  assert.equal(unpinned.metadataVersion, artifact.metadataVersion);
});

for (const [mod, filename, fixture, exceeds] of [
  ['svc', 'voicechat-fabric-1.21.1-2.6.22.jar', 'svc.properties', true],
  ['plasmo', 'plasmovoice-fabric-1.21.1-2.1.17.jar', 'plasmo.toml', false],
]) test(`real ${mod} identity composes with the external 1264-byte ceiling without approving audio`, { skip: !directory }, async () => {
  const artifact = inspectArtifact(await readArtifactFile(join(directory, filename)));
  const config = await readFile(new URL(`./fixtures/${fixture}`, import.meta.url));
  const before = Buffer.from(config);
  const plan = createPlan({ mod, artifact, mode: 'dedicated-aurora-only', guestPort: 35554,
    gamePort: 25565, transportPayload: 1264 }, config);
  assert.equal(plan.artifactVerified, true);
  assert.equal(plan.blockers.includes('configured_mtu_exceeds_transport_payload'), exceeds);
  assert(plan.blockers.includes('full_udp_datagram_budget_required'));
  assert(plan.blockers.includes('voice_runtime_not_verified'));
  assert.equal(plan.mtu.lowersModMtu, false);
  assert.equal(plan.runtimeHomologated, false);
  assert.equal(plan.applySupported, false);
  assert(!plan.changes.some(change => change.key.includes('mtu')));
  assert.deepEqual(config, before);
});
