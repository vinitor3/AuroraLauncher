import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';
import { mkdir, mkdtemp, writeFile, readFile, rm, symlink } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
import { LiveIpc, validateNativeEvent, readNativeFailure } from './live-ipc.mjs';
import { inspectAnnouncedConfig, verifyCopyTree, readApproval, runLiveWriter, makeLivePlan, main } from './live-writer.mjs';
import { withLabEula, requireLabRoot } from './server-lifecycle.mjs';
import { withOwnedProcess } from './owned-process.mjs';
import { runtimeFiles, runtimeArtifacts } from './play-profile.mjs';
import { SVC_BASELINE_BYTES } from './svc-profile.mjs';
import { PLASMO_BASELINE_BYTES } from './plasmo-profile.mjs';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const parent = join(repo, '.aurora/connect-runtime');
const id = 'aurora-test-00000000-0000-4000-8000-000000000001';
const digest = 'a'.repeat(64);
const nextDigest = 'b'.repeat(64);
const apply = { event: 'native_apply_review', profile: 'svc', instanceId: id, digest,
  sourceSha256: digest, replacementSha256: nextDigest };
const locked = { event: 'native_lock_held', profile: 'svc', instanceId: id, configSha256: nextDigest };
const restore = { event: 'native_restore_review', profile: 'svc', mode: 'reconciled', digest: nextDigest,
  sourceSha256: digest, replacementSha256: nextDigest };
const restored = { event: 'native_restored', profile: 'svc', mode: 'reconciled', sourceSha256: digest,
  appliedSha256: nextDigest, writerSha256: digest, restoredSha256: nextDigest,
  sourceUnchanged: true, unownedPreserved: true, pending: false };
const errorCode = (promise, code) => assert.rejects(promise, error => error.message === code);

async function fixture(t, lab = false) {
  const base = lab ? parent : tmpdir(); await mkdir(base, { recursive: true });
  const root = await mkdtemp(join(base, lab ? 'voice-roundtrip-' : 'aurora-live-ipc-'));
  t.after(async () => {
    assert.equal(dirname(resolve(root)), resolve(base));
    assert.ok(root.startsWith(join(base, lab ? 'voice-roundtrip-' : 'aurora-live-ipc-')));
    await rm(root, { recursive: true, force: true });
  });
  return root;
}
const publish = (root, name, value) => writeFile(join(root, name), JSON.stringify(value), { flag: 'wx' });
function announced(profile) {
  return profile === 'svc' ? Buffer.from(SVC_BASELINE_BYTES.toString().replace('voice_host=', 'voice_host=127.0.0.1:35554'))
    : Buffer.from(`${PLASMO_BASELINE_BYTES}\n[host.public]\nip="127.0.0.1"\nport=35554\n`);
}

test('native events enforce phase, profile, all keys, exact identity/hash and restoration claims', () => {
  for (const [phase, event] of [apply, locked, restore, restored].entries()) {
    assert.equal(validateNativeEvent({ ...event }, phase, 'svc').event, event.event);
    for (const invalid of [{ ...event, extra: true }, { ...event, event: 'other' }, { ...event, profile: 'plasmo' }]) {
      assert.throws(() => validateNativeEvent(invalid, phase, 'svc'));
    }
  }
  assert.throws(() => validateNativeEvent({ ...apply, instanceId: '../outside' }, 0, 'svc'), /live_ipc_instance/);
  assert.throws(() => validateNativeEvent({ ...apply, digest: 'secret' }, 0, 'svc'), /live_ipc_hash/);
  assert.throws(() => validateNativeEvent({ ...restore, mode: 'automatic' }, 2, 'svc'), /live_ipc_restore_mode/);
  assert.throws(() => validateNativeEvent({ ...restored, pending: true }, 3, 'svc'), /live_ipc_restoration/);
});

test('native failure diagnostics expose only a bounded static code, including after child close', async t => {
  const root = await fixture(t);
  assert.equal(await readNativeFailure(root), undefined);
  const failed = join(root, 'failed.json');
  await writeFile(failed, '{"event":"native_failed","code":"live_unowned_inventory_changed"}');
  assert.equal(await readNativeFailure(root), 'live_unowned_inventory_changed');
  await errorCode(new LiveIpc(root, 'svc').receive(0, new AbortController().signal), 'live_unowned_inventory_changed');
  for (const bytes of ['{"event":"native_failed","code":"private contents"}',
    '{"event":"native_failed","code":"live_code","extra":"private"}',
    '{"event":"native_failed","code":"live_bad","code":"live_alias"}',
    '{"event":"native_failed","\\u0063ode":"live_code"}', 'x'.repeat(513)]) {
    await writeFile(failed, bytes); await assert.rejects(readNativeFailure(root), error => /^live_ipc_failure_(schema|file)$/.test(error.message));
  }
});

test('IPC requires each approval in order and leaves bounded published messages immutable', async t => {
  const root = await fixture(t); const ipc = new LiveIpc(root, 'svc'); const signal = new AbortController().signal;
  await publish(root, 'apply-review.json', apply);
  assert.equal((await ipc.receive(0, signal)).digest, digest);
  await errorCode(ipc.receive(1, signal), 'live_ipc_phase');
  await ipc.send('apply-approval.json', { digest });
  await errorCode(ipc.send('apply-approval.json', { digest }), 'live_ipc_phase');
  await publish(root, 'lock-held.json', locked); await ipc.receive(1, signal);
  await errorCode(ipc.send('java-closed.json', { closed: true, exitCode: 0, forced: true, eulaRestored: true, configSha256: digest }), 'live_ipc_approval');
  await ipc.send('java-closed.json', { closed: true, exitCode: 0, forced: false, eulaRestored: true, configSha256: digest });
  await publish(root, 'restore-review.json', restore); await ipc.receive(2, signal);
  await ipc.send('restore-approval.json', { digest: nextDigest });
  await publish(root, 'restored.json', restored); assert.equal((await ipc.receive(3, signal)).unownedPreserved, true);
  assert.equal(JSON.parse(await readFile(join(root, 'restore-approval.json'))).digest, nextDigest);
  await errorCode(ipc.receive(3, signal), 'live_ipc_phase');
});

test('IPC refuses early phases, changed published bytes, cancellation and unknown sidecars', async t => {
  for (const scenario of ['early', 'changed', 'cancel', 'extra', 'link']) {
    const root = await fixture(t); const ipc = new LiveIpc(root, 'svc'); const signal = new AbortController().signal;
    if (scenario === 'early') await publish(root, 'restore-review.json', restore);
    if (scenario === 'changed') {
      await publish(root, 'apply-review.json', apply); await ipc.receive(0, signal);
      await writeFile(join(root, 'apply-review.json'), JSON.stringify({ ...apply, digest: nextDigest }));
      await errorCode(ipc.send('apply-approval.json', { digest }), 'live_ipc_changed'); continue;
    }
    if (scenario === 'cancel') await publish(root, 'cancel.json', {});
    if (scenario === 'extra') await publish(root, 'unexpected.json', {});
    if (scenario === 'link') await symlink(join(root, 'absent'), join(root, 'apply-review.json'), 'junction');
    await assert.rejects(ipc.receive(0, signal, 200));
  }
});

test('IPC incomplete, malformed and oversized messages never authorize; waiting has deadline/cancel', async t => {
  for (const text of ['{', 'null', JSON.stringify({ ...apply, digest: null }), 'x'.repeat(4097),
    JSON.stringify(apply).replace('"event":', '"event":"ignored","event":'),
    JSON.stringify(apply).replace('"digest":', '"\\u0064igest":')]) {
    const root = await fixture(t); await writeFile(join(root, 'apply-review.json'), text);
    await assert.rejects(new LiveIpc(root, 'svc').receive(0, new AbortController().signal, 100));
  }
  const root = await fixture(t); const ipc = new LiveIpc(root, 'svc');
  await errorCode(ipc.receive(0, new AbortController().signal, 20), 'live_ipc_deadline');
  const controller = new AbortController(); const pending = ipc.receive(0, controller.signal); controller.abort();
  await errorCode(pending, 'live_ipc_cancelled');
});

test('announced configuration validates actual fields without changing bytes or boot invariants', () => {
  for (const profile of ['svc', 'plasmo']) {
    const bytes = announced(profile); const original = Buffer.from(bytes);
    assert.equal(inspectAnnouncedConfig(profile, bytes).announcementPort, 35554);
    assert.deepEqual(bytes, original);
    for (const text of [bytes.toString().replace('35554', '35555'), bytes.toString().replace('127.0.0.1', '0.0.0.0'),
      bytes.toString().replace(profile === 'svc' ? '43002' : '43003', '12345')]) {
      assert.throws(() => inspectAnnouncedConfig(profile, Buffer.from(text)));
    }
  }
  for (const suffix of ['extra=1\n', '[host.public.extra]\na=1\n', 'port=35554\n']) {
    assert.throws(() => inspectAnnouncedConfig('plasmo', Buffer.concat([announced('plasmo'), Buffer.from(suffix)])));
  }
  assert.throws(() => inspectAnnouncedConfig('plasmo', Buffer.from(announced('plasmo').toString().replace('35554', '35554.0'))));
  assert.throws(() => inspectAnnouncedConfig('plasmo', Buffer.from(announced('plasmo').toString().replace('debug = false', 'debug = true'))));
});

async function copyFixture(t, profile) {
  const lab = await fixture(t, true); const root = join(lab, 'native/instances', id);
  await mkdir(join(root, '.aurora/connect-voice-stage'), { recursive: true });
  for (const leaf of ['new', 'old']) await mkdir(join(root, '.aurora/recovery-stage/11111111-1111-4111-8111-111111111111', leaf), { recursive: true });
  await writeFile(join(root, '.aurora/recovery-identity.json'), '{}');
  await writeFile(join(root, 'aurora-instance.json'), '{"minecraft":"1.21.1","loader":"fabric","versionId":"1.21.1"}');
  const path = profile === 'svc' ? 'config/voicechat/voicechat-server.properties' : 'config/plasmovoice/server/config.toml';
  await mkdir(dirname(join(root, path)), { recursive: true }); await writeFile(join(root, path), announced(profile));
  await mkdir(join(root, 'mods'));
  for (const pin of runtimeArtifacts(profile).filter(pin => pin.file.startsWith('mods/'))) await writeFile(join(root, pin.file), 'synthetic-tree-only');
  return root;
}

test('native launch tree only accepts exact config, metadata and artifact names, excluding caches and sidecars', async t => {
  for (const profile of ['svc', 'plasmo']) {
    const root = await copyFixture(t, profile); await verifyCopyTree(root, profile);
    for (const file of ['extra.exe', '.fabric', 'plugins', 'defaultconfigs', 'config/extra.json', 'mods/extra.jar',
      '.aurora/recovery-stage/extra', '.aurora/recovery-stage/11111111-1111-4111-8111-111111111111/new/0',
      '.aurora/recovery-stage/11111111-1111-4111-8111-111111111111/old/0']) {
      await writeFile(join(root, file), 'unapproved'); await assert.rejects(verifyCopyTree(root, profile));
      await rm(join(root, file));
    }
    await writeFile(join(root, 'aurora-instance.json'), '{"minecraft":"1.21.1","loader":"fabric","versionId":"1.21.1","jvmArgs":"unapproved"}');
    await errorCode(verifyCopyTree(root, profile), 'live_copy_metadata');
  }
});

test('shared EULA retains exact transaction in a native copy and refuses unrelated absolute roots', async t => {
  const root = await copyFixture(t, 'svc'); const original = runtimeFiles('svc')['eula.txt'];
  await writeFile(join(root, 'eula.txt'), original);
  const outcome = await withLabEula(root, digest, async own => {
    assert.match(await readFile(join(own, 'eula.txt'), 'utf8'), /eula=true/); return 7;
  });
  assert.equal(outcome.value, 7); assert.equal(outcome.eulaRestored, true);
  assert.equal(await readFile(join(root, 'eula.txt'), 'utf8'), original);
  await assert.rejects(withLabEula(root, digest, () => assert.fail('used copy')));
  await errorCode(requireLabRoot(resolve(tmpdir())), 'server_lab_root');
  await errorCode(requireLabRoot(join(parent, 'voice-roundtrip-Abc123/native/instances/fixture-source')), 'server_lab_root');
});

test('interactive approvals require exact complete lines and clean listeners after success/refusal/cancel', async () => {
  for (const kind of ['valid', 'wrong', 'eof', 'cancel', 'oversize']) {
    const input = new PassThrough(); const controller = new AbortController();
    const promise = readApproval(input, digest, controller.signal);
    if (kind === 'valid') input.write(`approve ${digest}\r\n`);
    if (kind === 'wrong') input.write(`approve ${nextDigest}\n`);
    if (kind === 'eof') input.end();
    if (kind === 'cancel') controller.abort();
    if (kind === 'oversize') input.write('a'.repeat(129));
    if (kind === 'valid') assert.equal(await promise, digest); else await assert.rejects(promise);
    assert.equal(input.listenerCount('data'), 0); assert.equal(input.listenerCount('end'), 0);
    assert.equal(input.listenerCount('error'), 0); input.destroy();
  }
});

test('live EULA/approval gates precede JDK, helper and stage access', async () => {
  await errorCode(runLiveWriter('PRIVATE', 'bad', 'svc', 'bad', digest, false), 'live_eula_acceptance_required');
  await errorCode(runLiveWriter('PRIVATE', 'bad', 'svc', 'bad', digest, true), 'live_approval');
  await errorCode(main(['run']), 'live_eula_acceptance_required');
  await errorCode(main(['run-control']), 'live_eula_acceptance_required');
  await errorCode(makeLivePlan('PRIVATE', 'bad', 'plasmo', 'bad', 'svc-control'), 'live_exercise');
  await errorCode(makeLivePlan('PRIVATE', 'bad', 'svc', 'bad', 'plasmo-control'), 'live_exercise');
  await errorCode(runLiveWriter('PRIVATE', 'bad', 'svc', 'bad', digest, true,
    { onReview: () => digest, exercise: 'svc-control' }), 'live_control_observer');
});

test('real sibling cancellation waits for callback preparation and owned child close after helper death', async t => {
  const root = await fixture(t); let callbackWork; let siblingWork; let siblingError; let preparationDone = false;
  let helperError;
  try {
    await withOwnedProcess({ executable: process.execPath, args: ['-e', 'setTimeout(()=>process.exit(3),180)'],
      cwd: root, timeoutMs: 3000, stopTimeoutMs: 100 }, ({ signal }) => {
      callbackWork = (async () => {
        await delay(50); signal.throwIfAborted();
        siblingWork = withOwnedProcess({ executable: process.execPath,
          args: ['-e', 'process.stdin.resume();process.stdin.on("data",()=>process.exit(0))'], cwd: root,
          signal, timeoutMs: 2000, stopTimeoutMs: 100 }, async ({ signal: childSignal }) => {
          await delay(1000, undefined, { signal: childSignal });
        });
        try { await siblingWork; } catch (error) { siblingError = error; throw error; }
        finally { preparationDone = true; }
      })(); return callbackWork;
    });
  } catch (error) { helperError = error; }
  finally { if (callbackWork) await callbackWork.catch(() => {}); }
  assert.equal(helperError.code, 'owned_process_child_exit_nonzero');
  assert.equal(siblingError.process.closed, true); assert.equal(preparationDone, true);
});

test('helper death before child creation waits for preparation and prevents all later writes', async t => {
  const root = await fixture(t); let work; let completed = false; let wrote = false;
  try {
    await assert.rejects(withOwnedProcess({ executable: process.execPath,
      args: ['-e', 'setTimeout(()=>process.exit(3),30)'], cwd: root, timeoutMs: 1000, stopTimeoutMs: 100 }, ({ signal }) => {
      work = (async () => {
        try { await delay(150); signal.throwIfAborted(); wrote = true; }
        finally { completed = true; }
      })(); return work;
    }), error => error.code === 'owned_process_child_exit_nonzero');
  } finally { if (work) await work.catch(() => {}); }
  assert.equal(completed, true); assert.equal(wrote, false);
});
