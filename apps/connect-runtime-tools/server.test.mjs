import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, writeFile, readFile, readdir, lstat, rm, unlink, symlink, link } from 'node:fs/promises';
import { dirname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { PassThrough } from 'node:stream';
import { EULA_URL } from './pins.mjs';
import { hash } from './files.mjs';
import { planDigest } from './runtime.mjs';
import { runtimeFiles, runtimeArtifacts } from './play-profile.mjs';
import { SVC_ARTIFACT, SVC_CONFIG_PATH, SVC_BASELINE_BYTES } from './svc-profile.mjs';
import { PLASMO_ARTIFACT, FABRIC_API_ARTIFACT, PLASMO_CONFIG_PATH, PLASMO_BASELINE_BYTES } from './plasmo-profile.mjs';
import { OwnedProcessError, withOwnedProcess } from './owned-process.mjs';
import { verifyPreparedFiles, withPreparedEula, buildRunPlan, runPlanDigest, runServer, requireOwnedBind, waitForStop, main, serverFailureReport } from './server.mjs';

// Synthetic files only. No real prepared runtime is read/written by this suite,
// no JAR is downloaded/executed, and the true EULA text below is a transaction
// fixture, not evidence of legal consent or Minecraft execution.
const parent = resolve(dirname(fileURLToPath(import.meta.url)), '../../.aurora/connect-runtime');
const falseEula = Buffer.from(`# ${EULA_URL}\neula=false\n`);
const trueEula = Buffer.from(`# ${EULA_URL}\neula=true\n`);
const json = value => `${JSON.stringify(value, null, 2)}\n`;
const approved = 'a'.repeat(64);
async function exists(file) { try { await lstat(file); return true; } catch (e) { if (e.code === 'ENOENT') return false; throw e; } }
async function fixture(t, profile = 'status', guiNames) {
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
  const voiceDirectories = profile === 'svc' ? ['mods', 'config', 'config/voicechat']
    : profile === 'plasmo' ? ['mods', 'config', 'config/plasmovoice', 'config/plasmovoice/server'] : [];
  for (const directory of ['libraries', ...voiceDirectories]) {
    await mkdir(join(root, directory));
  }
  const artifacts = [];
  const verified = [];
  for (const [i, original] of runtimeArtifacts(profile).entries()) {
    const bytes = Buffer.from(`SYNTHETIC_NOT_A_JAR_${i}`);
    const pin = { ...original, bytes: bytes.length, digest: createHash(original.algorithm).update(bytes).digest('hex') };
    artifacts.push(pin);
    verified.push({ file: pin.file, bytes: bytes.length, sha256: hash(bytes), algorithm: pin.algorithm, digest: pin.digest });
    await writeFile(join(root, pin.file), bytes, { flag: 'wx' });
  }
  const baseline = { schemaVersion: 1, purpose: 'synthetic_test_only', artifacts,
    ...(profile === 'gui' ? { profile: 'gui', playerNames: [...guiNames] } : {}),
    files: runtimeFiles(profile, guiNames) };
  const baselineHash = planDigest(baseline);
  const ready = { planSha256: baselineHash, verified, javaExecuted: false, runtimeHomologated: false };
  await writeFile(join(root, 'plan.json'), json(baseline), { flag: 'wx' });
  await writeFile(join(root, 'consent.json'), json({ planSha256: baselineHash, downloadEulaAccepted: true, javaExecuted: false }), { flag: 'wx' });
  await writeFile(join(root, 'ready.json'), json(ready), { flag: 'wx' });
  for (const [file, content] of Object.entries(baseline.files)) await writeFile(join(root, file), content, { flag: 'wx' });
  return { root, basename, baseline, ready };
}
async function expectCode(promise, code) { await assert.rejects(promise, error => error.message === code); }

test('interactive STATUS plan changes only its explicit purpose, profile and approved fixed lifetime', () => {
  const baseline = { java: { version: 'synthetic_java_21' }, files: runtimeFiles('status'),
    launch: { executable: 'SYNTHETIC_JAVA_NOT_EXECUTED', args: ['-Xms256M', '-Xmx1536M', 'FIXED_ENTRYPOINT'] } };
  const stage = { baselineHash: approved, verified: [] }; const tools = { synthetic: true };
  const original = buildRunPlan('prepared-Abc123', baseline, stage, tools);
  assert.deepEqual(original, buildRunPlan('prepared-Abc123', baseline, stage, tools, 'status'));
  assert.equal(original.purpose, 'aurora_connect_disposable_server_execution');
  assert.equal(Object.hasOwn(original, 'profile'), false);
  assert.deepEqual(original.limits, { lifetimeSeconds: 300, readinessSeconds: 180, bindProbeSeconds: 10,
    statusProbeSeconds: 6, stopSeconds: 5, outputBytes: 4 * 1024 * 1024,
    reuse: false, personalInstanceAccess: false, relay: false, mods: false });
  const interactive = buildRunPlan('prepared-Abc123', baseline, stage, tools, 'status-interactive');
  assert.equal(interactive.purpose, 'aurora_connect_disposable_status_interactive_execution');
  assert.equal(interactive.profile, 'status-interactive');
  assert.equal(interactive.limits.lifetimeSeconds, 900);
  const comparison = structuredClone(interactive);
  delete comparison.profile;
  comparison.purpose = original.purpose; comparison.limits.lifetimeSeconds = 300;
  assert.deepEqual(comparison, original);
  assert.notEqual(runPlanDigest(interactive), runPlanDigest(original));
  const changed = structuredClone(interactive); changed.limits.lifetimeSeconds = 901;
  assert.notEqual(runPlanDigest(changed), runPlanDigest(interactive));
  for (const profile of ['play', 'svc', 'plasmo']) {
    const plan = buildRunPlan('prepared-Abc123', { ...baseline, files: runtimeFiles(profile) }, stage, tools, profile);
    assert.equal(plan.limits.lifetimeSeconds, 300);
    assert.equal(plan.purpose, original.purpose);
  }
  for (const profile of ['interactive', 'status-interactive-901', 'svc-interactive', 900, null]) {
    assert.throws(() => buildRunPlan('prepared-Abc123', baseline, stage, tools, profile), /runtime_profile/);
  }
});

test('interactive STATUS CLI preserves consent and digest gates and refuses an arbitrary lifetime argument', async () => {
  await expectCode(main(['run-status-interactive', 'PRIVATE_JDK', 'prepared-Abc123', approved]), 'server_eula_acceptance_required');
  await expectCode(main(['run-status-interactive', 'PRIVATE_JDK', 'prepared-Abc123', 'wrong', '--user-accepted-minecraft-eula']), 'server_run_approval');
  await expectCode(main(['run-status-interactive', 'PRIVATE_JDK', 'prepared-Abc123', approved,
    '--user-accepted-minecraft-eula', '901']), 'server_eula_acceptance_required');
  await expectCode(main(['run-plan-status-interactive', 'PRIVATE_JDK', 'prepared-Abc123', '901']), 'server_usage');
  await expectCode(main(['run-status-interactive-901']), 'server_usage');
  await expectCode(main(['run-svc-interactive']), 'server_usage');
  await expectCode(runServer('PRIVATE_JDK', 'prepared-Abc123', approved, false,
    { profile: 'status-interactive' }), 'server_eula_acceptance_required');
});

test('GUI run plan contains the reviewed names and whitelist bytes, with a distinct fixed 900-second approval', () => {
  const baseline = { java: { version: 'synthetic_java_21' }, playerNames: ['AuroraGuiA'], files: runtimeFiles('gui', ['AuroraGuiA']),
    launch: { executable: 'SYNTHETIC_JAVA_NOT_EXECUTED', args: ['-Xms256M', '-Xmx1536M', 'FIXED_ENTRYPOINT'] } };
  const stage = { baselineHash: approved, verified: [] }; const tools = { synthetic: true };
  const gui = buildRunPlan('prepared-Abc123', baseline, stage, tools, 'gui');
  assert.equal(gui.purpose, 'aurora_connect_disposable_gui_execution'); assert.equal(gui.profile, 'gui');
  assert.equal(gui.limits.lifetimeSeconds, 900); assert.equal(gui.limits.mods, false);
  assert.deepEqual(gui.playerNames, ['AuroraGuiA']);
  assert.equal(gui.whitelistJson, baseline.files['whitelist.json']);
  assert.equal(gui.whitelistSha256, hash(gui.whitelistJson));
  assert.equal(gui.evidence.worldJoined, false); assert.equal(gui.evidence.runtimeHomologated, false);
  for (const profile of ['status', 'status-interactive', 'play', 'svc', 'plasmo']) {
    const other = buildRunPlan('prepared-Abc123', { ...baseline, files: runtimeFiles(profile === 'status-interactive' ? 'status' : profile) }, stage, tools, profile);
    assert.notEqual(runPlanDigest(gui), runPlanDigest(other));
    assert.equal(other.limits.lifetimeSeconds, profile === 'status-interactive' ? 900 : 300);
    const comparison = { ...gui.limits, lifetimeSeconds: other.limits.lifetimeSeconds, mods: other.limits.mods };
    assert.deepEqual(comparison, other.limits);
  }
  for (const names of [['auroraguia'], ['AuroraGuiB'], ['AuroraGuiA', 'AuroraGuiB']]) {
    const changed = buildRunPlan('prepared-Abc123', { ...baseline, playerNames: names, files: runtimeFiles('gui', names) }, stage, tools, 'gui');
    assert.notEqual(runPlanDigest(changed), runPlanDigest(gui));
  }
});

test('GUI server CLI rejects malformed names, missing consent and arbitrary lifetime before selected filesystem access', async () => {
  await expectCode(main(['run-gui', 'PRIVATE_JDK', 'prepared-Abc123', approved, 'AuroraGuiA']), 'server_eula_acceptance_required');
  await expectCode(main(['run-gui', 'PRIVATE_JDK', 'prepared-Abc123', 'wrong', 'AuroraGuiA', '--user-accepted-minecraft-eula']), 'server_run_approval');
  for (const names of [['ab'], ['AuroraGuiA', 'auroraguia'], ['abc\n']]) {
    await expectCode(main(['run-plan-gui', 'PRIVATE_JDK', 'prepared-Abc123', ...names]), 'runtime_gui_names');
    await expectCode(main(['run-gui', 'PRIVATE_JDK', 'prepared-Abc123', approved, ...names, '--user-accepted-minecraft-eula']), 'runtime_gui_names');
  }
  for (const args of [
    ['run-plan-gui', 'PRIVATE_JDK', 'prepared-Abc123'],
    ['run-plan-gui', 'PRIVATE_JDK', 'prepared-Abc123', 'AuroraGuiA', 'AuroraGuiB', '901'],
    ['run-gui', 'PRIVATE_JDK', 'prepared-Abc123', approved, '--user-accepted-minecraft-eula'],
    ['run-gui', 'PRIVATE_JDK', 'prepared-Abc123', approved, 'AuroraGuiA', 'AuroraGuiB', '901', '--user-accepted-minecraft-eula'],
  ]) await expectCode(main(args), 'server_usage');
  await expectCode(runServer('PRIVATE_JDK', 'prepared-Abc123', approved, false,
    { profile: 'gui', guiNames: ['AuroraGuiA'] }), 'server_eula_acceptance_required');
});

test('GUI stage verification reconstructs exact whitelist and properties from reviewed names, never supplied JSON', async t => {
  const names = ['AuroraGuiA', 'AuroraGuiB']; const f = await fixture(t, 'gui', names);
  assert.equal((await verifyPreparedFiles(f.basename, f.baseline, 'gui', names)).verified.length, 9);
  for (const changedNames of [['AuroraGuiA'], ['auroraguia', 'AuroraGuiB'], ['AuroraGuiB', 'AuroraGuiA']]) {
    await expectCode(verifyPreparedFiles(f.basename, f.baseline, 'gui', changedNames), 'server_gui_baseline');
  }
  for (const profile of ['status', 'play', 'svc', 'plasmo']) {
    await assert.rejects(verifyPreparedFiles(f.basename, f.baseline, profile));
  }
  for (const variation of ['uuid', 'extraPlayer', 'onlineMode', 'bind', 'whitelistOff', 'extraArtifact', 'playerNames']) {
    const changed = structuredClone(f.baseline);
    if (variation === 'uuid') changed.files['whitelist.json'] = changed.files['whitelist.json'].replace('56e1d47e', '00000000');
    if (variation === 'extraPlayer') changed.files['whitelist.json'] = runtimeFiles('play')['whitelist.json'];
    if (variation === 'onlineMode') changed.files['server.properties'] = changed.files['server.properties'].replace('online-mode=false', 'online-mode=true');
    if (variation === 'bind') changed.files['server.properties'] = changed.files['server.properties'].replace('127.0.0.1', '0.0.0.0');
    if (variation === 'whitelistOff') changed.files['server.properties'] = changed.files['server.properties'].replace('white-list=true', 'white-list=false');
    if (variation === 'extraArtifact') changed.artifacts.push({ ...changed.artifacts[0], file: 'mods/unreviewed.jar' });
    if (variation === 'playerNames') changed.playerNames = ['IndependentEdit'];
    await assert.rejects(verifyPreparedFiles(f.basename, changed, 'gui', names));
  }
  const original = await readFile(join(f.root, 'whitelist.json'));
  await writeFile(join(f.root, 'whitelist.json'), Buffer.concat([original, Buffer.from('\n')]));
  await expectCode(verifyPreparedFiles(f.basename, f.baseline, 'gui', names), 'server_config_changed');
  assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
  assert.equal(await exists(join(f.root, 'run-used.json')), false);
  assert.equal(await exists(join(f.root, 'run-eula-before.txt')), false);
});

test('server failure report copies only permitted process primitives from OwnedProcessError', () => {
  const expected = { spawned: true, closed: true, exitCode: 7, signal: null, forced: false, stdoutBytes: 21, stderrBytes: 22 };
  const source = { ...expected, pid: 'PRIVATE_PID', cwd: 'PRIVATE_PATH', args: ['PRIVATE_ARGUMENT'],
    env: { PRIVATE_ENV: 'PRIVATE_VALUE' }, stdout: 'PRIVATE_STDOUT', stderr: 'PRIVATE_STDERR',
    toJSON() { throw new Error('PRIVATE_SERIALIZER_MUST_NOT_RUN'); } };
  const error = new OwnedProcessError('owned_process_child_exit_nonzero', source);
  error.cause = new Error('PRIVATE_CAUSE');
  const report = serverFailureReport(error);
  assert.equal(report.event, 'failed'); assert.equal(report.code, error.code);
  assert.deepEqual(report.process, expected); assert.notEqual(report.process, source);
  assert.deepEqual(Object.keys(report.process).sort(), Object.keys(expected).sort());
  assert.doesNotMatch(JSON.stringify(report), /PRIVATE|pid|cwd|args|env|cause|toJSON/);
});

test('server failure report preserves errors without summaries and ignores untrusted or malformed process data', () => {
  const plain = new Error('server_usage');
  assert.deepEqual(JSON.parse(JSON.stringify(serverFailureReport(plain))), { event: 'failed', code: 'server_usage' });
  assert.deepEqual(JSON.parse(JSON.stringify(serverFailureReport(new Error('PRIVATE_PATH')))),
    { event: 'failed', code: 'server_io_or_process_failed' });
  const valid = { spawned: true, closed: true, exitCode: null, signal: 'SIGKILL', forced: true, stdoutBytes: 0, stderrBytes: 0 };
  const imitation = Object.assign(new Error('owned_process_deadline'), { name: 'OwnedProcessError', process: valid });
  assert.equal(Object.hasOwn(serverFailureReport(imitation), 'process'), false);
  assert.equal(Object.hasOwn(serverFailureReport(new OwnedProcessError('owned_process_cancelled')), 'process'), false);
  for (const patch of [{ spawned: 1 }, { closed: 'true' }, { forced: null }, { exitCode: 'PRIVATE' },
    { exitCode: Number.MAX_SAFE_INTEGER }, { signal: 'SIGPRIVATE' }, { stdoutBytes: -1 },
    { stdoutBytes: Infinity }, { stderrBytes: 0.5 }, { stderrBytes: 'PRIVATE' }]) {
    assert.equal(Object.hasOwn(serverFailureReport(new OwnedProcessError('owned_process_deadline', { ...valid, ...patch })), 'process'), false);
  }
  const getter = { ...valid };
  Object.defineProperty(getter, 'stdoutBytes', { get() { assert.fail('must not invoke a process summary accessor'); } });
  assert.equal(Object.hasOwn(serverFailureReport(new OwnedProcessError('owned_process_deadline', getter)), 'process'), false);
  assert.deepEqual(serverFailureReport(new OwnedProcessError('owned_process_deadline', valid)).process, valid);
  const readiness = Object.assign(new Error('owned_process_operation_failed'), { readinessFailure: 'server_bind_not_owned',
    bindObservation: { rows: 1, allOwned: false, ipv4Loopback: 1, ipv4MappedLoopback: 0, wildcard: 0 } });
  assert.equal(serverFailureReport(readiness).readinessFailure, readiness.readinessFailure);
  assert.deepEqual(serverFailureReport(readiness).bindObservation, readiness.bindObservation);
});

test('server failure report retains actual reaped Node exit and forced deadline without exposing child data', async () => {
  // Uses only this test's Node children and pipes, with no files, stages, Java,
  // network or EULA transaction. Waiting for abort keeps the callback cooperative.
  const wait = ({ signal }) => new Promise(resolvePromise => {
    if (signal.aborted) resolvePromise(); else signal.addEventListener('abort', resolvePromise, { once: true });
  });
  const prefix = "process.stdout.write('PRIVATE_STDOUT'); process.stderr.write('PRIVATE_STDERR');";
  for (const deadline of [false, true]) {
    const source = prefix + (deadline ? 'setInterval(() => {}, 1000);' : 'setTimeout(() => process.exit(7), 25);');
    await assert.rejects(withOwnedProcess({ executable: process.execPath, args: ['-e', source],
      cwd: resolve(dirname(fileURLToPath(import.meta.url))), env: { AURORA_TEST_PRIVATE: 'PRIVATE_ENV' },
      timeoutMs: deadline ? 1500 : 3000, stopTimeoutMs: 500, outputLimitBytes: 1024 }, wait), error => {
      assert.ok(error instanceof OwnedProcessError);
      const report = serverFailureReport(error);
      assert.equal(report.code, deadline ? 'owned_process_deadline' : 'owned_process_child_exit_nonzero');
      assert.deepEqual(report.process, error.process);
      assert.equal(report.process.spawned, true); assert.equal(report.process.closed, true);
      assert.equal(report.process.forced, deadline);
      if (!deadline) assert.equal(report.process.exitCode, 7);
      assert.equal(report.process.stdoutBytes, Buffer.byteLength('PRIVATE_STDOUT'));
      assert.equal(report.process.stderrBytes, Buffer.byteLength('PRIVATE_STDERR'));
      assert.doesNotMatch(JSON.stringify(report), /PRIVATE|pid|path|args|env|cause|stack/);
      return true;
    });
  }
});

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
  await verifyPreparedFiles(f.basename, f.baseline, 'play');
  const original = await readFile(join(f.root, 'whitelist.json'));
  await writeFile(join(f.root, 'whitelist.json'), '[]\n');
  await expectCode(verifyPreparedFiles(f.basename, f.baseline, 'play'), 'server_config_changed');
  assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
  assert.equal(await exists(join(f.root, 'run-used.json')), false);
  await writeFile(join(f.root, 'whitelist.json'), original);
  f.baseline.files['unexpected.conf'] = 'data';
  await expectCode(verifyPreparedFiles(f.basename, f.baseline, 'play'), 'server_baseline');
});

test('explicit SVC stage accepts exactly ten synthetic pinned artifact paths and its complete known tree', async t => {
  const f = await fixture(t, 'svc');
  const before = await readFile(join(f.root, SVC_CONFIG_PATH));
  const result = await verifyPreparedFiles(f.basename, f.baseline, 'svc');
  assert.equal(result.verified.length, 10);
  assert.deepEqual(result.verified.map(pin => pin.file), runtimeArtifacts('svc').map(pin => pin.file));
  assert.equal(result.verified.at(-1).file, SVC_ARTIFACT.file);
  assert.deepEqual(before, SVC_BASELINE_BYTES);
  assert.deepEqual(await readFile(join(f.root, SVC_CONFIG_PATH)), before);
  assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
  assert.deepEqual((await readdir(join(f.root, 'mods'))).sort(), [SVC_ARTIFACT.file.split('/').at(-1)]);
  assert.deepEqual(await readdir(join(f.root, 'config')), ['voicechat']);
  assert.deepEqual(await readdir(join(f.root, 'config/voicechat')), ['voicechat-server.properties']);
  assert.equal(await exists(join(f.root, 'run-used.json')), false);
  await assert.rejects(verifyPreparedFiles(f.basename, f.baseline));
  await assert.rejects(verifyPreparedFiles(f.basename, f.baseline, 'play'));
  await assert.rejects(verifyPreparedFiles(f.basename, f.baseline, 'arbitrary'));
});

test('SVC tree refuses additional mods, config files and executable entries before EULA or execution', async t => {
  for (const name of ['mods/extra.jar', 'mods/voicechat-duplicate.jar', 'config/extra.properties',
    'config/voicechat/extra.properties', 'config/voicechat/PRIVATE.exe', 'whitelist.json']) {
    const f = await fixture(t, 'svc');
    await writeFile(join(f.root, name), 'SYNTHETIC_EXTRA_NOT_EXECUTABLE');
    await assert.rejects(verifyPreparedFiles(f.basename, f.baseline, 'svc'));
    assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
    assert.equal(await exists(join(f.root, 'run-used.json')), false);
    assert.equal(await exists(join(f.root, 'run-eula-before.txt')), false);
  }
});

test('SVC stage refuses changed bind, port, announcement and artifact bytes without touching EULA', async t => {
  for (const replacement of [
    SVC_BASELINE_BYTES.toString().replace('127.0.0.1', '0.0.0.0'),
    SVC_BASELINE_BYTES.toString().replace('port=43002', 'port=24454'),
    SVC_BASELINE_BYTES.toString().replace('voice_host=', 'voice_host=PRIVATE_HOST'),
  ]) {
    const f = await fixture(t, 'svc');
    await writeFile(join(f.root, SVC_CONFIG_PATH), replacement);
    await expectCode(verifyPreparedFiles(f.basename, f.baseline, 'svc'), 'server_config_changed');
    assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
    assert.equal(await exists(join(f.root, 'run-used.json')), false);
  }
  const f = await fixture(t, 'svc');
  const original = await readFile(join(f.root, SVC_ARTIFACT.file));
  await writeFile(join(f.root, SVC_ARTIFACT.file), Buffer.alloc(original.length, 42));
  await assert.rejects(verifyPreparedFiles(f.basename, f.baseline, 'svc'));
  assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
  assert.equal(await exists(join(f.root, 'run-used.json')), false);
});

test('matching synthetic review hashes cannot approve an unsafe SVC endpoint', async t => {
  for (const [before, after, code] of [
    ['127.0.0.1', '0.0.0.0', 'runtime_svc_bind_invalid'],
    ['port=43002', 'port=24454', 'runtime_svc_port_invalid'],
    ['voice_host=', 'voice_host=PRIVATE_HOST', 'runtime_svc_announcement_invalid'],
  ]) {
    const f = await fixture(t, 'svc');
    f.baseline.files[SVC_CONFIG_PATH] = f.baseline.files[SVC_CONFIG_PATH].replace(before, after);
    const changedHash = planDigest(f.baseline);
    f.ready.planSha256 = changedHash;
    // This verifier accepts synthetic artifact hashes for unit tests. Even with
    // internally matching test metadata, the endpoint must satisfy the fixed
    // SVC policy; no authority is granted by these fixture files.
    await writeFile(join(f.root, SVC_CONFIG_PATH), f.baseline.files[SVC_CONFIG_PATH]);
    await writeFile(join(f.root, 'plan.json'), json(f.baseline));
    await writeFile(join(f.root, 'consent.json'), json({ planSha256: changedHash, downloadEulaAccepted: true, javaExecuted: false }));
    await writeFile(join(f.root, 'ready.json'), json(f.ready));
    await expectCode(verifyPreparedFiles(f.basename, f.baseline, 'svc'), code);
    assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
    assert.equal(await exists(join(f.root, 'run-used.json')), false);
  }
});

test('SVC baseline cannot omit the mod or config nor introduce an unrecognized config key', async t => {
  for (const mutation of ['missingConfig', 'missingMod', 'extraConfig', 'wrongPath']) {
    const f = await fixture(t, 'svc');
    if (mutation === 'missingConfig') delete f.baseline.files[SVC_CONFIG_PATH];
    if (mutation === 'missingMod') f.baseline.artifacts.pop();
    if (mutation === 'extraConfig') f.baseline.files['config/voicechat/unknown.properties'] = 'fixture';
    if (mutation === 'wrongPath') f.baseline.artifacts.at(-1).file = 'mods/other.jar';
    await assert.rejects(verifyPreparedFiles(f.basename, f.baseline, 'svc'));
    assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
    assert.equal(await exists(join(f.root, 'run-used.json')), false);
  }
});

test('SVC config hardlinks and linked config/mod directories are refused without traversing targets', async t => {
  const target = await fixture(t, 'svc');
  const linkedConfig = await fixture(t, 'svc');
  await unlink(join(linkedConfig.root, SVC_CONFIG_PATH));
  await link(join(target.root, SVC_CONFIG_PATH), join(linkedConfig.root, SVC_CONFIG_PATH));
  try {
    await assert.rejects(verifyPreparedFiles(linkedConfig.basename, linkedConfig.baseline, 'svc'));
    assert.deepEqual(await readFile(join(linkedConfig.root, 'eula.txt')), falseEula);
  } finally { await unlink(join(linkedConfig.root, SVC_CONFIG_PATH)); }
  for (const relative of ['config/voicechat', 'mods']) {
    const f = await fixture(t, 'svc');
    const directory = join(f.root, relative);
    assert.ok(resolve(directory).startsWith(resolve(f.root) + sep));
    // Only this newly allocated fixture child, checked to remain under its root.
    await rm(directory, { recursive: true });
    await symlink(join(target.root, relative), directory, process.platform === 'win32' ? 'junction' : 'dir');
    try {
      await assert.rejects(verifyPreparedFiles(f.basename, f.baseline, 'svc'));
      assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
      assert.equal(await exists(join(f.root, 'run-used.json')), false);
    } finally { await unlink(directory); }
  }
});

test('Plasmo stage requires its exact eleven artifact paths, three-level config tree and explicit profile', async t => {
  const f = await fixture(t, 'plasmo');
  const result = await verifyPreparedFiles(f.basename, f.baseline, 'plasmo');
  assert.equal(result.verified.length, 11);
  assert.deepEqual(result.verified.map(pin => pin.file), runtimeArtifacts('plasmo').map(pin => pin.file));
  assert.deepEqual(result.verified.slice(9).map(pin => pin.file), [PLASMO_ARTIFACT.file, FABRIC_API_ARTIFACT.file]);
  assert.deepEqual(await readFile(join(f.root, PLASMO_CONFIG_PATH)), PLASMO_BASELINE_BYTES);
  assert.deepEqual((await readdir(join(f.root, 'mods'))).sort(), [PLASMO_ARTIFACT.file, FABRIC_API_ARTIFACT.file].map(file => file.split('/').at(-1)).sort());
  assert.deepEqual(await readdir(join(f.root, 'config')), ['plasmovoice']);
  assert.deepEqual(await readdir(join(f.root, 'config/plasmovoice')), ['server']);
  assert.deepEqual(await readdir(join(f.root, 'config/plasmovoice/server')), ['config.toml']);
  assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
  assert.equal(await exists(join(f.root, 'run-used.json')), false);
  for (const otherProfile of ['status', 'play', 'svc']) {
    await assert.rejects(verifyPreparedFiles(f.basename, f.baseline, otherProfile));
    const other = await fixture(t, otherProfile);
    await assert.rejects(verifyPreparedFiles(other.basename, other.baseline, 'plasmo'));
    assert.deepEqual(await readFile(join(other.root, 'eula.txt')), falseEula);
  }
});

test('Plasmo known tree refuses additional JARs, alternate config locations and nested executables', async t => {
  for (const file of ['mods/extra.jar', SVC_ARTIFACT.file, 'config/extra.properties',
    'config/plasmovoice/config.toml', 'config/plasmovoice/server.toml',
    'config/plasmovoice/server/extra.toml', 'config/plasmovoice/server/PRIVATE.exe']) {
    const f = await fixture(t, 'plasmo');
    await writeFile(join(f.root, file), 'SYNTHETIC_EXTRA_NOT_EXECUTABLE');
    await assert.rejects(verifyPreparedFiles(f.basename, f.baseline, 'plasmo'));
    assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
    assert.equal(await exists(join(f.root, 'run-used.json')), false);
    assert.equal(await exists(join(f.root, 'run-eula-before.txt')), false);
  }
});

test('Plasmo config and both mod artifacts reject mutations before EULA or process use', async t => {
  for (const [before, after] of [['debug = false', 'debug = true'],
    ['check_for_updates = false', 'check_for_updates = true'],
    ['use_crowdin_translations = false', 'use_crowdin_translations = true'],
    ['127.0.0.1', '0.0.0.0'], ['port = 43003', 'port = 0']]) {
    const f = await fixture(t, 'plasmo');
    await writeFile(join(f.root, PLASMO_CONFIG_PATH), PLASMO_BASELINE_BYTES.toString().replace(before, after));
    await expectCode(verifyPreparedFiles(f.basename, f.baseline, 'plasmo'), 'server_config_changed');
    assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
    assert.equal(await exists(join(f.root, 'run-used.json')), false);
  }
  for (const pin of [PLASMO_ARTIFACT, FABRIC_API_ARTIFACT]) {
    const f = await fixture(t, 'plasmo');
    const bytes = await readFile(join(f.root, pin.file));
    await writeFile(join(f.root, pin.file), Buffer.alloc(bytes.length, 42));
    await assert.rejects(verifyPreparedFiles(f.basename, f.baseline, 'plasmo'));
    assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
    assert.equal(await exists(join(f.root, 'run-used.json')), false);
  }
});

test('matching synthetic Plasmo review hashes still cannot enable remote checks or unsafe endpoints', async t => {
  const base = PLASMO_BASELINE_BYTES.toString();
  for (const [config, code] of [
    [base.replace('debug = false', 'debug = true'), 'runtime_plasmo_flags_invalid'],
    [base.replace('check_for_updates = false', 'check_for_updates = true'), 'runtime_plasmo_flags_invalid'],
    [base.replace('use_crowdin_translations = false', 'use_crowdin_translations = true'), 'runtime_plasmo_flags_invalid'],
    [base.replace('127.0.0.1', '0.0.0.0'), 'runtime_plasmo_bind_invalid'],
    [base.replace('43003', '43003.0'), 'runtime_plasmo_port_invalid'],
    [base + '[host.public]\n', 'runtime_plasmo_announcement_invalid'],
  ]) {
    const f = await fixture(t, 'plasmo'); f.baseline.files[PLASMO_CONFIG_PATH] = config;
    const changedHash = planDigest(f.baseline); f.ready.planSha256 = changedHash;
    await writeFile(join(f.root, PLASMO_CONFIG_PATH), config);
    await writeFile(join(f.root, 'plan.json'), json(f.baseline));
    await writeFile(join(f.root, 'consent.json'), json({ planSha256: changedHash, downloadEulaAccepted: true, javaExecuted: false }));
    await writeFile(join(f.root, 'ready.json'), json(f.ready));
    await expectCode(verifyPreparedFiles(f.basename, f.baseline, 'plasmo'), code);
    assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
    assert.equal(await exists(join(f.root, 'run-used.json')), false);
  }
});

test('Plasmo baseline rejects missing dependencies, wrong artifact order and unknown config keys', async t => {
  for (const variation of ['missingApi', 'missingPlasmo', 'reordered', 'wrongApiPath', 'missingConfig', 'extraConfig']) {
    const f = await fixture(t, 'plasmo');
    if (variation === 'missingApi') f.baseline.artifacts.pop();
    if (variation === 'missingPlasmo') f.baseline.artifacts.splice(9, 1);
    if (variation === 'reordered') [f.baseline.artifacts[9], f.baseline.artifacts[10]] = [f.baseline.artifacts[10], f.baseline.artifacts[9]];
    if (variation === 'wrongApiPath') f.baseline.artifacts[10].file = 'mods/unreviewed-fabric-api.jar';
    if (variation === 'missingConfig') delete f.baseline.files[PLASMO_CONFIG_PATH];
    if (variation === 'extraConfig') f.baseline.files['config/plasmovoice/config.toml'] = 'fixture';
    await assert.rejects(verifyPreparedFiles(f.basename, f.baseline, 'plasmo'));
    assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
    assert.equal(await exists(join(f.root, 'run-used.json')), false);
  }
});

test('Plasmo files and every new config ancestor refuse links without traversing foreign content', async t => {
  const target = await fixture(t, 'plasmo');
  for (const file of [PLASMO_CONFIG_PATH, PLASMO_ARTIFACT.file, FABRIC_API_ARTIFACT.file]) {
    const f = await fixture(t, 'plasmo'); const path = join(f.root, file);
    await unlink(path); await link(join(target.root, file), path);
    try {
      await assert.rejects(verifyPreparedFiles(f.basename, f.baseline, 'plasmo'));
      assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
    } finally { await unlink(path); }
  }
  for (const relative of ['config/plasmovoice', 'config/plasmovoice/server', 'mods']) {
    const f = await fixture(t, 'plasmo'); const directory = resolve(f.root, relative);
    assert.ok(directory.startsWith(resolve(f.root) + sep));
    await rm(directory, { recursive: true });
    await symlink(join(target.root, relative), directory, process.platform === 'win32' ? 'junction' : 'dir');
    try {
      await assert.rejects(verifyPreparedFiles(f.basename, f.baseline, 'plasmo'));
      assert.deepEqual(await readFile(join(f.root, 'eula.txt')), falseEula);
      assert.equal(await exists(join(f.root, 'run-used.json')), false);
    } finally { await unlink(directory); }
  }
  assert.deepEqual(await readFile(join(target.root, PLASMO_CONFIG_PATH)), PLASMO_BASELINE_BYTES);
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

test('only explicit UDP proof accepts the observed IPv4-mapped loopback and keeps ownership strict', () => {
  const rows = [{ LocalAddress: '::ffff:127.0.0.1', OwningProcess: 42 }];
  assert.throws(() => requireOwnedBind(rows, value => value === 42), error => {
    assert.equal(error.message, 'server_bind_not_owned');
    assert.deepEqual(error.bindObservation, { rows: 1, allOwned: true, ipv4Loopback: 0, ipv4MappedLoopback: 1, wildcard: 0 });
    assert.doesNotMatch(JSON.stringify(error), /127\.0\.0\.1|OwningProcess|42/);
    return true;
  });
  requireOwnedBind(rows, value => value === 42, { allowIpv4MappedLoopback: true });
  assert.throws(() => requireOwnedBind(rows, () => false, { allowIpv4MappedLoopback: true }), /server_bind_not_owned/);
  for (const address of ['::', '0.0.0.0', '::1', '::ffff:0.0.0.0', '::ffff:192.0.2.1', '::ffff:127.0.0.2']) {
    assert.throws(() => requireOwnedBind([{ LocalAddress: address, OwningProcess: 42 }], () => true,
      { allowIpv4MappedLoopback: true }), /server_bind_not_owned/);
  }
  assert.throws(() => requireOwnedBind([...rows, ...rows], () => true, { allowIpv4MappedLoopback: true }), /server_bind_not_owned/);
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
  await expectCode(runServer('PRIVATE_JDK_DOES_NOT_EXIST', 'prepared-Abc123', approved, false, { profile: 'svc' }), 'server_eula_acceptance_required');
  await expectCode(main(['run-svc', 'PRIVATE_JDK_DOES_NOT_EXIST', 'prepared-Abc123', approved]), 'server_eula_acceptance_required');
  await expectCode(runServer('PRIVATE_JDK_DOES_NOT_EXIST', 'prepared-Abc123', approved, false, { profile: 'plasmo' }), 'server_eula_acceptance_required');
  await expectCode(main(['run-plasmo', 'PRIVATE_JDK_DOES_NOT_EXIST', 'prepared-Abc123', approved]), 'server_eula_acceptance_required');
});

test('wrong approval or changed pre-apply EULA cannot create a marker or invoke operation', async t => {
  const f = await fixture(t); let called = false;
  await expectCode(withPreparedEula(f.basename, 'wrong', () => { called = true; }), 'server_run_approval');
  assert.equal(await exists(join(f.root, 'run-used.json')), false);
  await writeFile(join(f.root, 'eula.txt'), 'eula=false\n# changed before approval\n');
  await expectCode(withPreparedEula(f.basename, approved, () => { called = true; }), 'server_eula_changed');
  assert.equal(await exists(join(f.root, 'run-used.json')), false); assert.equal(called, false);
});
