// Deterministic local filesystem races only. No native helper, Java or network.
// These reproduce the race itself, not the untraced historical GvrUSn failure.
// All files belong to fresh retained validation directories. Run serially:
// node --test --test-concurrency=1 apps/connect-runtime-tools/live-ipc.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { syncBuiltinESMExports } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { LiveIpc } from './live-ipc.mjs';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const parent = join(repo, '.aurora/connect-validation');
const names = ['apply-review.json', 'lock-held.json', 'restore-review.json', 'restored.json'];
const out = ['apply-approval.json', 'java-closed.json', 'restore-approval.json'];
const digest = 'a'.repeat(64);
const other = 'b'.repeat(64);
const id = 'aurora-test-00000000-0000-4000-8000-000000000001';
const signal = () => new AbortController().signal;

function events(profile = 'svc') {
  return [
    { event: 'native_apply_review', profile, instanceId: id, digest, sourceSha256: digest, replacementSha256: other },
    { event: 'native_lock_held', profile, instanceId: id, configSha256: other },
    { event: 'native_restore_review', profile, mode: 'reconciled', digest: other, sourceSha256: digest, replacementSha256: other },
    { event: 'native_restored', profile, mode: 'reconciled', sourceSha256: digest, appliedSha256: other,
      writerSha256: digest, restoredSha256: other, sourceUnchanged: true, unownedPreserved: true, pending: false },
  ];
}

async function fixture(profile = 'svc') {
  await fs.mkdir(parent, { recursive: true });
  const root = await fs.mkdtemp(join(parent, 'live-ipc-rename-'));
  assert.equal(dirname(resolve(root)), resolve(parent));
  const directory = join(root, 'ipc'); await fs.mkdir(directory);
  return { root, directory, profile, ipc: new LiveIpc(directory, profile), values: events(profile) };
}
const publish = (directory, name, value) => fs.writeFile(join(directory, name), JSON.stringify(value), { flag: 'wx' });

async function preparePhase(f, phase) {
  for (let index = 0; index < phase; index++) {
    await publish(f.directory, names[index], f.values[index]);
    await f.ipc.receive(index, signal(), 1000);
    await f.ipc.send(out[index], index === 1
      ? { closed: true, exitCode: 0, forced: false, eulaRestored: true, configSha256: other }
      : { digest: index === 0 ? digest : other });
  }
}

// Rebind Node's builtin ESM imports only inside this test process. Production
// receives no injectable filesystem or race hook. Always restore even on error.
async function withLstat(hook, operation) {
  const original = fs.lstat;
  fs.lstat = (path, ...args) => hook(original, path, ...args);
  syncBuiltinESMExports();
  try { return await operation(); }
  finally { fs.lstat = original; syncBuiltinESMExports(); }
}

async function renameBeforeStat(f, partName, targetName, operation) {
  const partial = join(f.directory, partName);
  const target = join(f.directory, targetName);
  let renamed = 0;
  const result = await withLstat(async (original, path, ...args) => {
    if (String(path) === partial && renamed === 0) {
      renamed++; await fs.rename(partial, target);
    }
    return original(path, ...args);
  }, operation);
  assert.equal(renamed, 1, 'rename must occur after enumeration and before partial lstat');
  return result;
}

async function fixedError(promise, expected) {
  await assert.rejects(promise, error => {
    assert.equal(error.message, expected);
    assert.equal(error.code, undefined);
    assert.equal(error.path, undefined);
    assert.equal(error.cause, undefined);
    assert.ok(!error.message.includes('PRIVATE_SENTINEL'));
    return true;
  });
}

for (const profile of ['svc', 'plasmo']) {
  for (let phase = 0; phase < names.length; phase++) {
    test(`${profile} phase ${phase}: known partial published between readdir and lstat is read normally`, async () => {
      const f = await fixture(profile); await preparePhase(f, phase);
      await publish(f.directory, `${names[phase]}.part`, f.values[phase]);
      const value = await renameBeforeStat(f, `${names[phase]}.part`, names[phase],
        () => f.ipc.receive(phase, signal(), 1000));
      assert.equal(value.event, f.values[phase].event); assert.equal(value.profile, profile);
      assert.ok(Object.isFrozen(value));
      assert.deepEqual(JSON.parse(await fs.readFile(join(f.directory, names[phase]))), f.values[phase]);
    });
  }
}

test('disappearing known partial without its published final fails once rather than polling', async () => {
  const f = await fixture(); const partial = join(f.directory, 'apply-review.json.part');
  await publish(f.directory, 'apply-review.json.part', f.values[0]);
  let moved = 0;
  await withLstat(async (original, path, ...args) => {
    if (String(path) === partial && moved === 0) { moved++; await fs.rename(partial, join(f.root, 'preserved-partial')); }
    return original(path, ...args);
  }, () => fixedError(f.ipc.receive(0, signal(), 1000), 'live_ipc_file'));
  assert.equal(moved, 1); assert.ok(await fs.readFile(join(f.root, 'preserved-partial')));
});

for (const [name, text, code] of [
  ['malformed', '{', 'live_ipc_json'],
  ['oversized', 'x'.repeat(4097), 'live_ipc_file'],
  ['wrong_profile', JSON.stringify(events('plasmo')[0]), 'live_ipc_event'],
  ['invalid_hash', JSON.stringify({ ...events()[0], digest: 'PRIVATE_SENTINEL' }), 'live_ipc_hash'],
  ['extra_field', JSON.stringify({ ...events()[0], extra: true }), 'live_ipc_event'],
  ['duplicate_field', JSON.stringify(events()[0]).replace('"digest":', '"digest":"ignored","digest":'), 'live_ipc_json'],
]) {
  test(`renamed final ${name} still passes through bound/schema checks and is refused`, async () => {
    const f = await fixture();
    await fs.writeFile(join(f.directory, 'apply-review.json.part'), text, { flag: 'wx' });
    await renameBeforeStat(f, 'apply-review.json.part', 'apply-review.json',
      () => fixedError(f.ipc.receive(0, signal(), 1000), code));
    await fixedError(f.ipc.send('apply-approval.json', { digest }), 'live_ipc_phase');
  });
}

test('a raced future-phase publication remains forbidden', async () => {
  const f = await fixture();
  await publish(f.directory, 'restore-review.json.part', f.values[2]);
  await renameBeforeStat(f, 'restore-review.json.part', 'restore-review.json',
    () => fixedError(f.ipc.receive(0, signal(), 1000), 'live_ipc_early_phase'));
});

test('unknown partial is refused before any lstat exception can be tolerated', async () => {
  const f = await fixture(); const unknown = join(f.directory, 'foreign.json.part');
  await fs.writeFile(unknown, '{}', { flag: 'wx' }); let touched = 0;
  await withLstat(async (original, path, ...args) => {
    if (String(path) === unknown) touched++;
    return original(path, ...args);
  }, () => fixedError(f.ipc.receive(0, signal(), 1000), 'live_ipc_unknown_file'));
  assert.equal(touched, 0);
});

for (const code of ['ENOTDIR', 'EACCES', 'EIO']) {
  test(`known partial ${code} remains a redacted failure rather than the ENOENT exception`, async () => {
    const f = await fixture(); const partial = join(f.directory, 'apply-review.json.part');
    await publish(f.directory, 'apply-review.json.part', f.values[0]); let intercepted = 0;
    await withLstat(async (original, path, ...args) => {
      if (String(path) === partial) {
        intercepted++; throw Object.assign(new Error('PRIVATE_SENTINEL filesystem path'), { code, path: partial });
      }
      return original(path, ...args);
    }, () => fixedError(f.ipc.receive(0, signal(), 1000), 'live_ipc_file'));
    assert.equal(intercepted, 1);
  });
}

test('canonical disappearance after enumeration is refused, not treated like a partial', async () => {
  const f = await fixture(); const canonical = join(f.directory, names[0]);
  await publish(f.directory, names[0], f.values[0]); let moved = false;
  await withLstat(async (original, path, ...args) => {
    if (String(path) === canonical && !moved) { moved = true; await fs.rename(canonical, join(f.root, 'preserved-canonical')); }
    return original(path, ...args);
  }, () => fixedError(f.ipc.receive(0, signal(), 1000), 'live_ipc_file'));
  assert.equal(moved, true);
});

test('canonical disappearance after tree metadata also fails instead of waiting for replacement', async () => {
  const f = await fixture(); const canonical = join(f.directory, names[0]);
  await publish(f.directory, names[0], f.values[0]); let moved = false;
  await withLstat(async (original, path, ...args) => {
    if (String(path) === canonical && !moved) {
      const snapshot = await original(path, ...args); moved = true;
      await fs.rename(canonical, join(f.root, 'preserved-after-stat')); return snapshot;
    }
    return original(path, ...args);
  }, () => fixedError(f.ipc.receive(0, signal(), 1000), 'live_ipc_file'));
  assert.equal(moved, true);
});

for (const mutation of ['missing', 'changed']) {
  test(`an already observed message ${mutation} is still refused before approval`, async () => {
    const f = await fixture(); await publish(f.directory, names[0], f.values[0]);
    await f.ipc.receive(0, signal(), 1000);
    if (mutation === 'missing') await fs.rename(join(f.directory, names[0]), join(f.root, 'preserved-observed'));
    else await fs.writeFile(join(f.directory, names[0]), JSON.stringify({ ...f.values[0], digest: other }));
    await fixedError(f.ipc.send('apply-approval.json', { digest }), mutation === 'missing' ? 'live_ipc_file' : 'live_ipc_changed');
    await assert.rejects(fs.lstat(join(f.directory, 'apply-approval.json')), { code: 'ENOENT' });
  });
}

for (const kind of ['directory', 'hardlink', 'junction']) {
  test(`a raced ${kind} final is rejected by normal file identity checks`, async () => {
    const f = await fixture(); const partial = join(f.directory, 'apply-review.json.part');
    if (kind === 'directory') await fs.mkdir(partial);
    if (kind === 'hardlink') {
      const original = join(f.root, 'preserved-original'); await fs.writeFile(original, JSON.stringify(f.values[0]), { flag: 'wx' });
      await fs.link(original, partial);
    }
    if (kind === 'junction') {
      const target = join(f.root, 'preserved-directory'); await fs.mkdir(target);
      await fs.symlink(target, partial, process.platform === 'win32' ? 'junction' : 'dir');
    }
    await renameBeforeStat(f, 'apply-review.json.part', 'apply-review.json',
      () => fixedError(f.ipc.receive(0, signal(), 1000), 'live_ipc_file'));
  });
}

test('an unpublished partial waits only until the declared deadline', async () => {
  const f = await fixture(); await fs.writeFile(join(f.directory, 'apply-review.json.part'), '{', { flag: 'wx' });
  const started = performance.now();
  await fixedError(f.ipc.receive(0, signal(), 25), 'live_ipc_deadline');
  assert.ok(performance.now() - started < 1000, 'partial wait exceeded bounded fixture allowance');
  assert.equal(await fs.readFile(join(f.directory, 'apply-review.json.part'), 'utf8'), '{');
});

test('raw enumeration failure is redacted without exposing its path', async () => {
  const f = await fixture();
  await withLstat(async (original, path, ...args) => {
    if (String(path) === f.directory) throw Object.assign(new Error('PRIVATE_SENTINEL path'), { code: 'EACCES', path: f.directory });
    return original(path, ...args);
  }, () => fixedError(f.ipc.receive(0, signal(), 1000), 'live_ipc_file'));
});
