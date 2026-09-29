import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, writeFile, readFile, readdir, lstat, rm, unlink, symlink, link } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { PassThrough } from 'node:stream';
import { artifactPins, EULA_URL, SERVER_PROPERTIES } from './pins.mjs';
import { hash } from './files.mjs';
import { planDigest } from './runtime.mjs';
import { runtimeFiles } from './play-profile.mjs';
import { withOwnedProcess } from './owned-process.mjs';
import { verifyPreparedFiles, withPreparedEula, runPlanDigest, runServer, requireOwnedBind, waitForStop, main } from './server.mjs';

// Synthetic files only. No real prepared runtime is read/written by this suite,
// no JAR is downloaded/executed, and the true EULA text below is a transaction
// fixture, not evidence of legal consent or Minecraft execution.
const parent = resolve(dirname(fileURLToPath(import.meta.url)), '../../.aurora/connect-runtime');
const falseEula = Buffer.from(`# ${EULA_URL}\neula=false\n`);
const trueEula = Buffer.from(`# ${EULA_URL}\neula=true\n`);
const json = value => `${JSON.stringify(value, null, 2)}\n`;
const approved = 'a'.repeat(64);
async function exists(file) { try { await lstat(file); return true; } catch (e) { if (e.code === 'ENOENT') return false; throw e; } }
async function fixture(t, profile = 'status') {
  const metadata = await lstat(parent);
  assert.ok(metadata.isDirectory() && !metadata.isSymbolicLink());
  const root = await mkdtemp(join(parent, 'prepared-')); const basename = root.slice(parent.length + 1);
  t.after(async () => {
    assert.equal(dirname(resolve(root)), parent); assert.match(basename, /^prepared-[A-Za-z0-9]{6}$/);
    // Only the unique directory allocated by this fixture is eligible; never
    // remove the parent or any other prepared server, including real caches.
    if ((await lstat(root)).isSymbolicLink()) await unlink(root);
    else await rm(root, { recursive: true, force: true });
  });
  await mkdir(join(root, 'libraries'));
  const artifacts = [];
  const verified = [];
  for (const [i, original] of artifactPins().entries()) {
    const bytes = Buffer.from(`SYNTHETIC_NOT_A_JAR_${i}`);
    const pin = { ...original, bytes: bytes.length, digest: createHash(original.algorithm).update(bytes).digest('hex') };
    artifacts.push(pin);
    verified.push({ file: pin.file, bytes: bytes.length, sha256: hash(bytes), algorithm: pin.algorithm, digest: pin.digest });
    await writeFile(join(root, pin.file), bytes, { flag: 'wx' });
  }
  const baseline = { schemaVersion: 1, purpose: 'synthetic_test_only', artifacts,
    files: runtimeFiles(profile) };
  const baselineHash = planDigest(baseline);
  const ready = { planSha256: baselineHash, verified, javaExecuted: false, runtimeHomologated: false };
  await writeFile(join(root, 'plan.json'), json(baseline), { flag: 'wx' });
  await writeFile(join(root, 'consent.json'), json({ planSha256: baselineHash, downloadEulaAccepted: true, javaExecuted: false }), { flag: 'wx' });
  await writeFile(join(root, 'ready.json'), json(ready), { flag: 'wx' });
  for (const [file, content] of Object.entries(baseline.files)) await writeFile(join(root, file), content, { flag: 'wx' });
  return { root, basename, baseline, ready };
}
async function expectCode(promise, code) { await assert.rejects(promise, error => error.message === code); }

test('verifier accepts complete synthetic stage, both published algorithms and observed SHA256 without mutation', async t => {
  const f = await fixture(t); const before = await readdir(f.root);
  const first = await verifyPreparedFiles(f.basename, f.baseline);
  const second = await verifyPreparedFiles(f.basename, f.baseline);
  assert.deepEqual(first, second); assert.equal(first.verified.length, 9);
  assert.equal(first.verified[0].algorithm, 'sha1'); assert.equal(first.verified[1].algorithm, 'sha256');
  assert.equal(first.baselineHash, planDigest(f.baseline));
  assert.deepEqual(await readdir(f.root), before); assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
  assert.equal(runPlanDigest({ a: 1 }), runPlanDigest({ a: 1 }));
  assert.notEqual(runPlanDigest({ diff: { after: 'eula=true' } }), runPlanDigest({ diff: { after: 'eula=false' } }));
});

test('explicit play stage verifies whitelist bytes and rejects changed identities before execution', async t => {
  const f = await fixture(t, 'play');
  await verifyPreparedFiles(f.basename, f.baseline);
  const original = await readFile(join(f.root, 'whitelist.json'));
  await writeFile(join(f.root, 'whitelist.json'), '[]\n');
  await expectCode(verifyPreparedFiles(f.basename, f.baseline), 'server_config_changed');
  assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
  assert.equal(await exists(join(f.root, 'run-used.json')), false);
  await writeFile(join(f.root, 'whitelist.json'), original);
  f.baseline.files['unexpected.conf'] = 'data';
  await expectCode(verifyPreparedFiles(f.basename, f.baseline), 'server_baseline');
});

test('prepared name syntax refuses paths, UNC, aliases and unrelated names before filesystem access', async () => {
  for (const name of ['../prepared-Abc123', 'prepared-a', 'prepared-Abc123/x', 'C:\\prepared-Abc123',
    '\\\\server\\prepared-Abc123', 'prepared-Abc123:ads', 'prepared-Abc123.', 'prepared-Abc123\n']) {
    await expectCode(verifyPreparedFiles(name, {}), 'server_prepared_name');
  }
});

test('extra mods, executables, unrecognized files, missing files and reused markers fail closed', async t => {
  for (const name of ['mods', 'PRIVATE.exe', 'extra.json', 'run-used.json']) {
    const f = await fixture(t);
    if (name === 'mods') await mkdir(join(f.root, name)); else await writeFile(join(f.root, name), 'fixture');
    await assert.rejects(verifyPreparedFiles(f.basename, f.baseline));
    assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
  }
  const f = await fixture(t); await unlink(join(f.root, 'ready.json'));
  await expectCode(verifyPreparedFiles(f.basename, f.baseline), 'server_stage_entries');
});

test('artifact corruption, wrong size and ready observed digest are refused', async t => {
  for (const variation of ['bytes', 'size', 'observedSha', 'publishedSha', 'executed', 'unknownReady']) {
    const f = await fixture(t);
    if (variation === 'bytes') await writeFile(join(f.root, 'server.jar'), Buffer.alloc(f.baseline.artifacts[0].bytes, 0));
    if (variation === 'size') await writeFile(join(f.root, 'server.jar'), 'short');
    if (variation === 'observedSha') f.ready.verified[0].sha256 = '0'.repeat(64);
    if (variation === 'publishedSha') f.ready.verified[0].digest = '0'.repeat(40);
    if (variation === 'executed') f.ready.javaExecuted = true;
    if (variation === 'unknownReady') f.ready.unknown = 'never trust';
    await writeFile(join(f.root, 'ready.json'), json(f.ready));
    await assert.rejects(verifyPreparedFiles(f.basename, f.baseline));
    assert.equal(await exists(join(f.root, 'run-used.json')), false);
  }
});

test('baseline, config, consent and EULA must equal the reviewed bytes exactly', async t => {
  for (const file of ['plan.json', 'server.properties', 'consent.json', 'eula.txt']) {
    const f = await fixture(t); const original = await readFile(join(f.root, file));
    await writeFile(join(f.root, file), Buffer.concat([original, Buffer.from('\n')]));
    await assert.rejects(verifyPreparedFiles(f.basename, f.baseline));
    assert.equal(await exists(join(f.root, 'run-used.json')), false);
  }
});

test('links and hardlinks to artifacts or stage directories are refused', async t => {
  const linked = await fixture(t); const target = await fixture(t);
  await unlink(join(linked.root, 'server.jar'));
  await link(join(target.root, 'server.jar'), join(linked.root, 'server.jar'));
  await assert.rejects(verifyPreparedFiles(linked.basename, linked.baseline));
  await unlink(join(linked.root, 'server.jar'));
  const directory = await fixture(t);
  await rm(join(directory.root, 'libraries'), { recursive: true });
  await symlink(join(target.root, 'libraries'), join(directory.root, 'libraries'), 'junction');
  await assert.rejects(verifyPreparedFiles(directory.basename, directory.baseline));
  await unlink(join(directory.root, 'libraries'));
});

test('EULA transaction records snapshot before operation and restores exact bytes after real owned Node exits', async t => {
  const f = await fixture(t); await verifyPreparedFiles(f.basename, f.baseline);
  const source = "process.stdin.on('data', b => { if (b.toString() === 'stop\\n') process.exit(0); });";
  const result = await withPreparedEula(f.basename, approved, async root => {
    assert.deepEqual(await readFile(join(root, 'eula.txt')), trueEula);
    assert.deepEqual(await readFile(join(root, 'run-eula-before.txt')), falseEula);
    assert.equal(JSON.parse(await readFile(join(root, 'run-used.json'), 'utf8')).runPlanSha256, approved);
    return withOwnedProcess({ executable: process.execPath, args: ['-e', source], cwd: root,
      timeoutMs: 3000, stopTimeoutMs: 500 }, async () => 'synthetic_operation_only');
  });
  assert.equal(result.eulaRestored, true); assert.equal(result.value.process.exitCode, 0);
  assert.equal(result.value.value, 'synthetic_operation_only');
  assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
  await assert.rejects(withPreparedEula(f.basename, approved, () => assert.fail('must not run twice')));
});

test('operation failure or forced Node cleanup restores EULA yet remains failure', async t => {
  for (const forced of [false, true]) {
    const f = await fixture(t);
    await assert.rejects(withPreparedEula(f.basename, approved, async root => {
      if (!forced) throw new Error('SYNTHETIC_OPERATION_FAILED');
      return withOwnedProcess({ executable: process.execPath, args: ['-e', 'setInterval(() => {}, 1000)'], cwd: root,
        timeoutMs: 3000, stopTimeoutMs: 50 }, async () => 'must_not_pass');
    }));
    assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
    assert.equal(await exists(join(f.root, 'run-used.json')), true);
  }
});

test('changed EULA after execution is preserved with a recovery record and snapshot', async t => {
  const f = await fixture(t); const changed = Buffer.from('eula=true\n# independent fixture edit\n');
  await expectCode(withPreparedEula(f.basename, approved, async root => {
    await writeFile(join(root, 'eula.txt'), changed); return 'must_not_pass';
  }), 'server_eula_recovery_conflict');
  assert.deepEqual(await readFile(join(f.root, 'eula.txt')), changed);
  assert.deepEqual(await readFile(join(f.root, 'run-eula-before.txt')), falseEula);
  const recovery = JSON.parse(await readFile(join(f.root, 'run-recovery.json'), 'utf8'));
  assert.equal(recovery.overwritePermitted, false); assert.equal(recovery.code, 'server_eula_recovery_conflict');
});

test('concurrent EULA transactions have one exclusive marker winner and preserve loser refusal', async t => {
  const f = await fixture(t); let release; const hold = new Promise(resolve => { release = resolve; });
  let entered; const active = new Promise(resolve => { entered = resolve; });
  const winner = withPreparedEula(f.basename, approved, async () => { entered(); await hold; return 'winner'; });
  const observed = winner.catch(error => { throw error; });
  t.after(async () => { release(); await observed; });
  await active;
  await assert.rejects(withPreparedEula(f.basename, approved, () => assert.fail('loser must not execute')));
  release(); assert.equal((await observed).value, 'winner');
  assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
});

test('bind proof only accepts one loopback listener owned by the live supervisor predicate', () => {
  requireOwnedBind([{ LocalAddress: '127.0.0.1', OwningProcess: 42 }], value => value === 42);
  for (const rows of [[], [{ LocalAddress: '0.0.0.0', OwningProcess: 42 }],
    [{ LocalAddress: '::', OwningProcess: 42 }], [{ LocalAddress: '127.0.0.1', OwningProcess: 3 }],
    [{ LocalAddress: '127.0.0.1', OwningProcess: 42 }, { LocalAddress: '127.0.0.1', OwningProcess: 42 }]]) {
    assert.throws(() => requireOwnedBind(rows, value => value === 42), /server_bind_not_owned/);
  }
  assert.throws(() => requireOwnedBind([{ LocalAddress: '127.0.0.1', OwningProcess: 42 }], () => false), /server_bind_not_owned/);
});

test('stdin stop/EOF ends cleanly; cancel, oversized or malformed input remove all listeners', async () => {
  for (const kind of ['stop', 'eof', 'cancel', 'oversize', 'malformed', 'partialStop', 'partialOther', 'preCancel']) {
    const input = new PassThrough(); const controller = new AbortController();
    if (kind === 'preCancel') controller.abort();
    const pending = waitForStop(input, controller.signal);
    if (kind === 'stop') input.write('stop\r\n');
    if (kind === 'eof') input.end();
    if (kind === 'cancel') controller.abort();
    if (kind === 'oversize') input.write(Buffer.alloc(513, 65));
    if (kind === 'malformed') input.write('start\n');
    if (kind === 'partialStop') input.end('stop');
    if (kind === 'partialOther') input.end('start');
    if (['stop', 'eof'].includes(kind)) await pending; else await assert.rejects(pending);
    assert.equal(input.listenerCount('data'), 0); assert.equal(input.listenerCount('end'), 0);
    assert.equal(input.listenerCount('error'), 0); assert.equal(input.isPaused(), true); input.destroy();
  }
});

test('CLI EULA gate and usage refuse before any selected JDK, stage or process access', async () => {
  await expectCode(runServer('PRIVATE_JDK_DOES_NOT_EXIST', 'prepared-Abc123', approved, false), 'server_eula_acceptance_required');
  await expectCode(main(['run', 'PRIVATE_JDK_DOES_NOT_EXIST', 'prepared-Abc123', approved]), 'server_eula_acceptance_required');
  await expectCode(main(['unknown']), 'server_usage');
  await expectCode(runServer('PRIVATE_JDK_DOES_NOT_EXIST', 'prepared-Abc123', 'unapproved', true), 'server_run_approval');
});

test('wrong approval or changed pre-apply EULA cannot create a marker or invoke operation', async t => {
  const f = await fixture(t); let called = false;
  await expectCode(withPreparedEula(f.basename, 'wrong', () => { called = true; }), 'server_run_approval');
  assert.equal(await exists(join(f.root, 'run-used.json')), false);
  await writeFile(join(f.root, 'eula.txt'), 'eula=false\n# changed before approval\n');
  await expectCode(withPreparedEula(f.basename, approved, () => { called = true; }), 'server_eula_changed');
  assert.equal(await exists(join(f.root, 'run-used.json')), false); assert.equal(called, false);
});
