import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, writeFile, readFile, readdir, lstat, link, symlink, unlink, rm, open } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { artifactPins, EULA_URL, SERVER_PROPERTIES } from './pins.mjs';
import { hash, readLocal, downloadVerified } from './files.mjs';
import { makePlan, planDigest, stageRuntime, main } from './runtime.mjs';
import { PLAY_NAMES, offlineUuid, runtimeFiles, runtimeArtifacts } from './play-profile.mjs';
import { SVC_ARTIFACT, SVC_CONFIG_PATH, SVC_BASELINE_BYTES, validateSvcConfig } from './svc-profile.mjs';
import { PLASMO_ARTIFACT, FABRIC_API_ARTIFACT, PLASMO_CONFIG_PATH, PLASMO_BASELINE_BYTES,
  validatePlasmoConfig } from './plasmo-profile.mjs';

// Every executable/JAR below is synthetic data. No Java process, production
// fetch, Minecraft download or persistent consent is part of these tests.
const here = dirname(fileURLToPath(import.meta.url));
const stagingParent = resolve(here, '../../.aurora/connect-runtime');
const runtimeCli = join(here, 'runtime.mjs');
const payload = Buffer.from(Array.from({ length: 4096 }, (_, i) => i % 251));
const digest = (algorithm, bytes) => createHash(algorithm).update(bytes).digest('hex');
const pinFor = (route = 'ok', algorithm = 'sha256') => ({
  file: 'fixture.jar', url: `https://maven.fabricmc.net/fixture/${route}`,
  bytes: payload.length, algorithm, digest: digest(algorithm, payload),
});
async function exists(path) { try { await lstat(path); return true; } catch (e) { if (e.code === 'ENOENT') return false; throw e; } }
async function stagingSnapshot() { try { return (await readdir(stagingParent)).sort(); } catch (e) { if (e.code === 'ENOENT') return null; throw e; } }
async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'aurora-runtime-test-'));
  t.after(async () => {
    assert.ok(isAbsolute(root) && root.startsWith(join(tmpdir(), 'aurora-runtime-test-')));
    await rm(root, { recursive: true, force: true });
  });
  return root;
}
async function fakeJdk(root, release = 'JAVA_VERSION="21.0.8"\r\nOS_NAME="Windows"\r\nOS_ARCH="x86_64"\r\n') {
  const home = join(root, 'jdk fixture');
  await mkdir(join(home, 'bin'), { recursive: true });
  await writeFile(join(home, 'release'), release);
  await writeFile(join(home, 'bin/java.exe'), Buffer.from('MZ\0not-an-executable-fixture'));
  return home;
}
async function localHttp(t, handler) {
  const requests = [];
  const server = createServer((request, response) => {
    requests.push(request.url);
    Promise.resolve(handler(request, response)).catch(() => response.destroy());
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(async () => {
    const closed = new Promise(resolve => server.close(resolve));
    server.closeAllConnections();
    await closed;
  });
  const origin = `http://127.0.0.1:${server.address().port}`;
  const fetcher = (url, options) => {
    const supplied = new URL(url);
    assert.equal(supplied.origin, 'https://maven.fabricmc.net');
    assert.equal(options.redirect, 'error');
    assert.equal(options.credentials, 'omit');
    assert.equal(options.headers['Accept-Encoding'], 'identity');
    assert.ok(options.signal instanceof AbortSignal);
    return fetch(origin + supplied.pathname, options);
  };
  return { requests, fetcher, origin };
}
function childCli(args) {
  // Any accidentally reached fetch fails locally; only synthetic JDK files can
  // be read. This executes the actual CLI's argument/error handling in Node.
  const denyNetwork = `data:text/javascript,${encodeURIComponent("globalThis.fetch = () => { throw new Error('test_network_forbidden'); };")}`;
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ['--import', denyNetwork, runtimeCli, ...args], {
      windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
    });
    const timer = setTimeout(() => { child.kill(); reject(new Error('fixture_cli_timeout')); }, 10000);
    let stdout = ''; let stderr = '';
    child.stdout.on('data', value => { stdout += value; if (stdout.length > 64 * 1024) child.kill(); });
    child.stderr.on('data', value => { stderr += value; if (stderr.length > 4096) child.kill(); });
    child.on('error', error => { clearTimeout(timer); reject(error); });
    child.on('close', code => { clearTimeout(timer); resolve({ code, stdout, stderr }); });
  });
}

test('fixed pins are complete, independent copies and agree with reviewed metadata', () => {
  const pins = artifactPins();
  assert.equal(pins.length, 9);
  assert.equal(pins.reduce((sum, item) => sum + item.bytes, 0), 55_582_631);
  assert.equal(new Set(pins.map(pin => pin.file)).size, pins.length);
  for (const pin of pins) {
    const url = new URL(pin.url);
    assert.equal(url.protocol, 'https:');
    assert.ok(['piston-data.mojang.com', 'maven.fabricmc.net'].includes(url.hostname));
    assert.match(pin.file, /^(?:server\.jar|libraries\/[a-zA-Z0-9.+_-]+\.jar)$/);
    assert.equal(pin.digest.length, pin.algorithm === 'sha1' ? 40 : 64);
  }
  assert.deepEqual(pins.find(pin => pin.file === 'libraries/intermediary-1.21.1.jar'), {
    file: 'libraries/intermediary-1.21.1.jar',
    url: 'https://maven.fabricmc.net/net/fabricmc/intermediary/1.21.1/intermediary-1.21.1.jar',
    bytes: 657725, algorithm: 'sha256', digest: '6059157dfb4a536ec151697004f74f6004b7d346b289c2f89849e0e585aa36fa',
  });
  assert.equal(pins.at(-1).digest, '9ed7b7f4197153ad97d71b4b9cd9148989e8f0c600934b035ce45f3ef003efc0');
  assert.equal(pins.at(-1).bytes, 1416842);
  pins[0].digest = 'mutated fixture'; pins.pop();
  assert.equal(artifactPins().length, 9);
  assert.equal(artifactPins()[0].digest, '59353fb40c36d304f2035d51e7d6e6baa98dc05c');
});

test('play profile is explicit, two fixed offline identities, no compression and a different approval', async t => {
  const root = await fixture(t); const home = await fakeJdk(root);
  const status = await makePlan(home);
  const play = await makePlan(home, 'play');
  assert.equal(status.files['whitelist.json'], undefined);
  assert.notEqual(planDigest(play), planDigest(status));
  assert.deepEqual(play.artifacts, status.artifacts);
  const whitelist = JSON.parse(play.files['whitelist.json']);
  assert.deepEqual(whitelist.map(row => row.name), [...PLAY_NAMES]);
  assert.equal(new Set(whitelist.map(row => row.uuid)).size, 2);
  for (const row of whitelist) {
    assert.equal(row.uuid, offlineUuid(row.name));
    assert.match(row.uuid, /^[a-f0-9]{8}-[a-f0-9]{4}-3[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/);
  }
  assert.ok(play.files['server.properties'].includes('network-compression-threshold=-1\n'));
  assert.ok(play.files['server.properties'].includes('white-list=true\n'));
  assert.equal(play.files['eula.txt'], status.files['eula.txt']);
  await assert.rejects(stageRuntime(home, planDigest(status), true, 'play'), /runtime_plan_approval/);
  await assert.rejects(main(['stage-play', home, planDigest(play)]), /runtime_eula_acceptance_required/);
  assert.throws(() => runtimeFiles('arbitrary'), /runtime_profile/);
  assert.throws(() => offlineUuid('arbitrary'), /runtime_play_identity/);
});

test('GUI staging plan binds one or two case-sensitive names and whitelist bytes without changing existing profiles', async t => {
  const root = await fixture(t); const home = await fakeJdk(root); const before = await stagingSnapshot();
  const play = await makePlan(home, 'play');
  const names = ['AuroraGuiA']; const gui = await makePlan(home, 'gui', names);
  names[0] = 'IndependentEdit';
  assert.equal(gui.purpose, 'aurora_connect_disposable_gui_lab'); assert.equal(gui.profile, 'gui');
  assert.deepEqual(gui.playerNames, ['AuroraGuiA']);
  assert.equal(gui.files['whitelist.json'], runtimeFiles('gui', ['AuroraGuiA'])['whitelist.json']);
  assert.deepEqual(gui.artifacts, play.artifacts); assert.equal(gui.artifacts.length, 9);
  const comparison = structuredClone(gui);
  delete comparison.profile; delete comparison.playerNames;
  comparison.purpose = play.purpose; comparison.files['whitelist.json'] = play.files['whitelist.json'];
  assert.deepEqual(comparison, play);
  const pair = await makePlan(home, 'gui', ['AuroraGuiA', 'AuroraGuiB']);
  const changedCase = await makePlan(home, 'gui', ['auroraguia']);
  assert.equal(new Set([gui, pair, changedCase, play, await makePlan(home)].map(planDigest)).size, 5);
  assert.equal(gui.limits.voiceMods, false); assert.equal(gui.limits.coreSkins, false);
  assert.deepEqual(await stagingSnapshot(), before);
});

test('GUI preparation CLI validates names, consent and name-bound approval before network or stage creation', async t => {
  const root = await fixture(t); const home = await fakeJdk(root); const before = await stagingSnapshot();
  const gui = await makePlan(home, 'gui', ['AuroraGuiA']);
  const originalFetch = globalThis.fetch; let fetched = 0;
  globalThis.fetch = () => { fetched++; throw new Error('test_network_forbidden'); };
  try {
    const result = await main(['plan-gui', home, 'AuroraGuiA']);
    assert.deepEqual(result, { event: 'runtime_plan', planSha256: planDigest(gui), plan: gui });
    assert.equal((await main(['plan-gui', home, 'AuroraGuiA', 'AuroraGuiB'])).plan.playerNames.length, 2);
    for (const names of [['auroraguia'], ['AuroraGuiB'], ['AuroraGuiA', 'AuroraGuiB']]) {
      await assert.rejects(stageRuntime(home, planDigest(gui), true, 'gui', names), /runtime_plan_approval/);
    }
    await assert.rejects(stageRuntime(home, planDigest(await makePlan(home, 'play')), true, 'gui', ['AuroraGuiA']), /runtime_plan_approval/);
    await assert.rejects(stageRuntime(home, planDigest(gui), true, 'play'), /runtime_plan_approval/);
    await assert.rejects(main(['stage-gui', home, planDigest(gui), 'AuroraGuiA']), /runtime_eula_acceptance_required/);
    await assert.rejects(main(['stage-gui', home, '0'.repeat(64), 'AuroraGuiA', '--user-accepted-minecraft-eula']), /runtime_plan_approval/);
    for (const args of [
      ['plan-gui', 'PRIVATE_ABSENT_JDK', 'abc\n'],
      ['plan-gui', 'PRIVATE_ABSENT_JDK', 'AuroraGuiA', 'auroraguia'],
      ['stage-gui', 'PRIVATE_ABSENT_JDK', '0'.repeat(64), '--other', '--user-accepted-minecraft-eula'],
    ]) await assert.rejects(main(args), /runtime_gui_names/);
    for (const args of [
      ['plan-gui', home], ['plan-gui', home, 'AuroraGuiA', 'AuroraGuiB', 'AuroraGuiC'],
      ['stage-gui', home, planDigest(gui), '--user-accepted-minecraft-eula'],
      ['stage-gui', home, planDigest(gui), 'AuroraGuiA', 'AuroraGuiB', '901', '--user-accepted-minecraft-eula'],
    ]) await assert.rejects(main(args), /runtime_usage/);
  } finally { globalThis.fetch = originalFetch; }
  const planned = await childCli(['plan-gui', home, 'AuroraGuiA']);
  assert.equal(planned.code, 0); assert.equal(planned.stderr, '');
  assert.equal(JSON.parse(planned.stdout).planSha256, planDigest(gui));
  const blocked = await childCli(['plan-gui', 'PRIVATE_ABSENT_JDK', 'PRIVATE bad name']);
  assert.equal(blocked.code, 1); assert.equal(blocked.stderr, '');
  assert.deepEqual(JSON.parse(blocked.stdout), { event: 'failed', code: 'runtime_gui_names' });
  assert.equal(fetched, 0); assert.deepEqual(await stagingSnapshot(), before);
});

test('SVC profile has one exact voice artifact and isolated files without changing status or play', async t => {
  const root = await fixture(t); const home = await fakeJdk(root);
  const before = await stagingSnapshot();
  const status = await makePlan(home, 'status');
  const play = await makePlan(home, 'play');
  const svc = await makePlan(home, 'svc');
  assert.deepEqual(await makePlan(home, 'svc'), svc);
  assert.equal(new Set([status, play, svc].map(planDigest)).size, 3);
  assert.equal(svc.purpose, 'aurora_connect_disposable_svc_lab');
  assert.deepEqual(svc.voiceMod, { id: 'voicechat', version: '2.6.22' });
  assert.equal(status.voiceMod, undefined); assert.equal(play.voiceMod, undefined);
  assert.equal(svc.limits.voiceMods, true);
  assert.equal(status.limits.voiceMods, false); assert.equal(play.limits.voiceMods, false);
  for (const key of ['personalInstanceAccess', 'relay', 'coreSkins', 'runtimeHomologated']) assert.equal(svc.limits[key], false);
  assert.deepEqual(svc.target, status.target);
  assert.deepEqual(svc.artifacts.slice(0, 9), status.artifacts);
  assert.deepEqual(svc.artifacts[9], SVC_ARTIFACT);
  assert.equal(svc.artifacts.length, 10);
  assert.equal(svc.downloadBytes, 60_952_106);
  assert.deepEqual(runtimeArtifacts('svc'), svc.artifacts);
  assert.deepEqual(runtimeArtifacts('status'), artifactPins());
  assert.deepEqual(runtimeArtifacts('play'), artifactPins());
  const mutatedPins = runtimeArtifacts('svc');
  mutatedPins[9].digest = '0'.repeat(64); mutatedPins.pop();
  assert.deepEqual(runtimeArtifacts('svc').at(-1), SVC_ARTIFACT);
  assert.throws(() => runtimeArtifacts('arbitrary'), /runtime_profile/);
  assert.deepEqual(Object.keys(svc.files).sort(), ['server.properties', 'eula.txt', SVC_CONFIG_PATH].sort());
  assert.equal(svc.files['server.properties'], SERVER_PROPERTIES);
  assert.equal(svc.files['eula.txt'], `# ${EULA_URL}\neula=false\n`);
  assert.deepEqual(Buffer.from(svc.files[SVC_CONFIG_PATH]), SVC_BASELINE_BYTES);
  assert.equal(validateSvcConfig(Buffer.from(svc.files[SVC_CONFIG_PATH])).keyCount, 3);
  assert.equal(status.files[SVC_CONFIG_PATH], undefined); assert.equal(play.files[SVC_CONFIG_PATH], undefined);
  assert.equal(svc.launch.args[svc.launch.args.indexOf('-cp') + 1].split(';').length, 8);
  assert.ok(!svc.launch.args.some(value => value.includes('voicechat')));
  assert.deepEqual(await stagingSnapshot(), before);
});

test('SVC CLI and explicit profile keep approval and EULA gates before any networking or staging', async t => {
  const root = await fixture(t); const home = await fakeJdk(root);
  const before = await stagingSnapshot(); const originalFetch = globalThis.fetch; let fetched = 0;
  const svc = await makePlan(home, 'svc'); const status = await makePlan(home);
  globalThis.fetch = () => { fetched++; throw new Error('test_network_forbidden'); };
  try {
    const result = await main(['plan-svc', home]);
    assert.equal(result.event, 'runtime_plan');
    assert.equal(result.planSha256, planDigest(svc));
    assert.deepEqual(result.plan, svc);
    await assert.rejects(stageRuntime(home, planDigest(status), true, 'svc'), /runtime_plan_approval/);
    await assert.rejects(stageRuntime(home, planDigest(svc), true, 'status'), /runtime_plan_approval/);
    await assert.rejects(stageRuntime(home, planDigest(svc), false, 'svc'), /runtime_eula_acceptance_required/);
    await assert.rejects(main(['stage-svc', home, planDigest(svc)]), /runtime_eula_acceptance_required/);
    await assert.rejects(main(['stage-svc', home, planDigest(status), '--user-accepted-minecraft-eula']), /runtime_plan_approval/);
  } finally { globalThis.fetch = originalFetch; }
  const planned = await childCli(['plan-svc', home]);
  assert.equal(planned.code, 0); assert.equal(planned.stderr, '');
  assert.equal(JSON.parse(planned.stdout).planSha256, planDigest(svc));
  const blocked = await childCli(['stage-svc', home, planDigest(svc)]);
  assert.equal(blocked.code, 1); assert.equal(blocked.stderr, '');
  assert.deepEqual(JSON.parse(blocked.stdout), { event: 'failed', code: 'runtime_eula_acceptance_required' });
  assert.equal(fetched, 0);
  assert.deepEqual(await stagingSnapshot(), before);
});

test('Plasmo profile pins eleven artifacts and isolated flags without importing another voice profile', async t => {
  const root = await fixture(t); const home = await fakeJdk(root); const before = await stagingSnapshot();
  const status = await makePlan(home, 'status'); const play = await makePlan(home, 'play');
  const svc = await makePlan(home, 'svc'); const plasmo = await makePlan(home, 'plasmo');
  assert.deepEqual(await makePlan(home, 'plasmo'), plasmo);
  assert.equal(new Set([status, play, svc, plasmo].map(planDigest)).size, 4);
  assert.equal(plasmo.purpose, 'aurora_connect_disposable_plasmo_lab');
  assert.deepEqual(plasmo.voiceMod, { id: 'plasmovoice', version: '2.1.17', fabricApi: '0.116.10+1.21.1' });
  assert.equal(plasmo.limits.voiceMods, true);
  for (const key of ['personalInstanceAccess', 'relay', 'coreSkins', 'runtimeHomologated']) assert.equal(plasmo.limits[key], false);
  assert.deepEqual(plasmo.target, status.target);
  assert.deepEqual(plasmo.artifacts.slice(0, 9), status.artifacts);
  assert.deepEqual(plasmo.artifacts.slice(9), [PLASMO_ARTIFACT, FABRIC_API_ARTIFACT]);
  assert.equal(plasmo.artifacts.length, 11); assert.equal(plasmo.downloadBytes, 70_374_585);
  assert.deepEqual(runtimeArtifacts('plasmo'), plasmo.artifacts);
  const mutated = runtimeArtifacts('plasmo');
  mutated[9].url = 'https://example.invalid/'; mutated[10].digest = '0'.repeat(64); mutated.pop();
  assert.deepEqual(runtimeArtifacts('plasmo').slice(9), [PLASMO_ARTIFACT, FABRIC_API_ARTIFACT]);
  assert.deepEqual(Object.keys(plasmo.files).sort(), ['server.properties', 'eula.txt', PLASMO_CONFIG_PATH].sort());
  assert.equal(plasmo.files['server.properties'], SERVER_PROPERTIES);
  assert.equal(plasmo.files['eula.txt'], `# ${EULA_URL}\neula=false\n`);
  assert.deepEqual(Buffer.from(plasmo.files[PLASMO_CONFIG_PATH]), PLASMO_BASELINE_BYTES);
  assert.deepEqual(validatePlasmoConfig(Buffer.from(plasmo.files[PLASMO_CONFIG_PATH])), {
    sha256: hash(PLASMO_BASELINE_BYTES), size: PLASMO_BASELINE_BYTES.length, bindLoopback: true,
    port: 43003, announcementAbsent: true, keyCount: 5, updateChecks: false, translationsDownload: false,
  });
  assert.equal(plasmo.files[SVC_CONFIG_PATH], undefined);
  for (const other of [status, play, svc]) {
    assert.equal(other.files[PLASMO_CONFIG_PATH], undefined);
    assert.equal(other.artifacts.some(pin => pin.file === PLASMO_ARTIFACT.file || pin.file === FABRIC_API_ARTIFACT.file), false);
  }
  assert.equal(plasmo.launch.args[plasmo.launch.args.indexOf('-cp') + 1].split(';').length, 8);
  assert.ok(!plasmo.launch.args.some(value => value.includes('plasmovoice') || value.includes('fabric-api')));
  assert.deepEqual(await stagingSnapshot(), before);
});

test('Plasmo approval cannot be exchanged with status, play or SVC and CLI gates precede writes', async t => {
  const root = await fixture(t); const home = await fakeJdk(root); const before = await stagingSnapshot();
  const plasmo = await makePlan(home, 'plasmo');
  const originalFetch = globalThis.fetch; let fetched = 0;
  globalThis.fetch = () => { fetched++; throw new Error('test_network_forbidden'); };
  try {
    const result = await main(['plan-plasmo', home]);
    assert.equal(result.event, 'runtime_plan'); assert.equal(result.planSha256, planDigest(plasmo));
    assert.deepEqual(result.plan, plasmo);
    for (const otherProfile of ['status', 'play', 'svc']) {
      const other = await makePlan(home, otherProfile);
      await assert.rejects(stageRuntime(home, planDigest(other), true, 'plasmo'), /runtime_plan_approval/);
      await assert.rejects(stageRuntime(home, planDigest(plasmo), true, otherProfile), /runtime_plan_approval/);
    }
    await assert.rejects(stageRuntime(home, planDigest(plasmo), false, 'plasmo'), /runtime_eula_acceptance_required/);
    await assert.rejects(main(['stage-plasmo', home, planDigest(plasmo)]), /runtime_eula_acceptance_required/);
    await assert.rejects(main(['stage-plasmo', home, '0'.repeat(64), '--user-accepted-minecraft-eula']), /runtime_plan_approval/);
  } finally { globalThis.fetch = originalFetch; }
  const planned = await childCli(['plan-plasmo', home]);
  assert.equal(planned.code, 0); assert.equal(planned.stderr, '');
  assert.equal(JSON.parse(planned.stdout).planSha256, planDigest(plasmo));
  const blocked = await childCli(['stage-plasmo', home, planDigest(plasmo)]);
  assert.equal(blocked.code, 1); assert.equal(blocked.stderr, '');
  assert.deepEqual(JSON.parse(blocked.stdout), { event: 'failed', code: 'runtime_eula_acceptance_required' });
  assert.equal(fetched, 0); assert.deepEqual(await stagingSnapshot(), before);
});

test('plan reads synthetic JDK bytes without executing and deterministically binds the review', { skip: process.platform !== 'win32' }, async t => {
  const root = await fixture(t); const home = await fakeJdk(root);
  const before = await stagingSnapshot();
  const plan = await makePlan(home);
  assert.deepEqual(await makePlan(home), plan);
  assert.equal(planDigest(await makePlan(home)), planDigest(plan));
  assert.deepEqual(plan.target, { minecraft: '1.21.1', loader: 'fabric', loaderVersion: '0.16.14', javaMajor: 21, protocol: 767 });
  assert.equal(plan.java.executableSha256, hash(await readFile(join(home, 'bin/java.exe'))));
  assert.equal(plan.java.releaseSha256, hash(await readFile(join(home, 'release'))));
  assert.equal(plan.files['server.properties'], SERVER_PROPERTIES);
  for (const entry of ['server-ip=127.0.0.1', 'server-port=43001', 'online-mode=false', 'enable-rcon=false', 'enable-query=false', 'white-list=true', 'enforce-whitelist=true']) {
    assert.ok(plan.files['server.properties'].split('\n').includes(entry));
  }
  assert.equal(plan.files['eula.txt'], `# ${EULA_URL}\neula=false\n`);
  assert.equal(plan.consent.downloadRequiresUserAcceptance, true);
  assert.equal(plan.consent.eulaFileRemainsFalse, true);
  assert.equal(plan.launch.cwd, 'fresh_lab_directory');
  assert.equal(plan.launch.executableNotRunByThisTool, true);
  assert.ok(plan.launch.args.includes('-Dfabric.gameJarPath=server.jar'));
  assert.deepEqual(plan.launch.args.slice(-2), ['net.fabricmc.loader.impl.launch.knot.KnotServer', 'nogui']);
  assert.equal(plan.launch.args[plan.launch.args.indexOf('-cp') + 1].split(';').length, 8);
  assert.deepEqual(plan.launch.unsetChildEnvironment, ['JAVA_TOOL_OPTIONS', '_JAVA_OPTIONS', 'JDK_JAVA_OPTIONS', 'CLASSPATH']);
  for (const key of ['personalInstanceAccess', 'relay', 'voiceMods', 'coreSkins', 'runtimeHomologated']) assert.equal(plan.limits[key], false);
  assert.equal(plan.limits.retries, 0); assert.equal(plan.limits.redirects, false);
  await writeFile(join(home, 'bin/java.exe'), Buffer.from('MZ\0changed-fixture'));
  assert.notEqual(planDigest(await makePlan(home)), planDigest(plan));
  assert.deepEqual(await stagingSnapshot(), before);
});

test('JDK release, platform, executable signature, duplicates and bounds fail closed', { skip: process.platform !== 'win32' }, async t => {
  const root = await fixture(t); const home = await fakeJdk(root);
  const good = await readFile(join(home, 'release'));
  for (const invalid of [
    Buffer.from([255]), Buffer.alloc(65537, 35),
    good.toString().replace('21.0.8', '17.0.8'), good.toString().replace('Windows', 'Linux'),
    good.toString().replace('x86_64', 'aarch64'), good.toString() + 'JAVA_VERSION="21.0.8"\n',
    good.toString().replace('JAVA_VERSION', 'UNKNOWN_VERSION'),
  ]) {
    await writeFile(join(home, 'release'), invalid);
    await assert.rejects(makePlan(home));
  }
  await writeFile(join(home, 'release'), good);
  for (const invalid of [Buffer.alloc(0), Buffer.from('M'), Buffer.from('ELF\0fixture'), Buffer.alloc(4 * 1024 * 1024 + 1)]) {
    await writeFile(join(home, 'bin/java.exe'), invalid);
    await assert.rejects(makePlan(home));
  }
});

test('approval and EULA gates cannot reach networking or allocate staging', { skip: process.platform !== 'win32' }, async t => {
  const root = await fixture(t); const home = await fakeJdk(root);
  const before = await stagingSnapshot(); const originalFetch = globalThis.fetch; let fetched = 0;
  globalThis.fetch = () => { fetched++; throw new Error('test_network_forbidden'); };
  try {
    for (const consent of [undefined, false, 'true', 1]) {
      await assert.rejects(stageRuntime(join(root, 'absent-jdk'), 'x', consent), { message: 'runtime_eula_acceptance_required' });
    }
    // Synthetic gate input only: approval is invalid, so no download/consent
    // record can occur; this is not an acceptance of terms for an actual stage.
    for (const approval of [undefined, '', 'x', '0'.repeat(64), 'F'.repeat(64)]) {
      await assert.rejects(stageRuntime(home, approval, true), { message: 'runtime_plan_approval' });
    }
    await assert.rejects(main(['stage', home, '0'.repeat(64)]), { message: 'runtime_eula_acceptance_required' });
    for (const args of [[], ['run'], ['stage', home, '0'.repeat(64), '--accept'], ['plan', home, 'extra']]) {
      await assert.rejects(main(args), { message: 'runtime_usage' });
    }
  } finally { globalThis.fetch = originalFetch; }
  assert.equal(fetched, 0);
  assert.deepEqual(await stagingSnapshot(), before);
});

test('actual CLI plans without Java and redacts gate failures without networking or staging', { skip: process.platform !== 'win32' }, async t => {
  const root = await fixture(t); const home = await fakeJdk(root); const before = await stagingSnapshot();
  const planned = await childCli(['plan', home]);
  assert.equal(planned.code, 0); assert.equal(planned.stderr, '');
  const output = JSON.parse(planned.stdout);
  assert.equal(output.event, 'runtime_plan'); assert.equal(output.planSha256, planDigest(await makePlan(home)));
  for (const [args, expected] of [
    [[], 'runtime_usage'], [['stage', home, '0'.repeat(64)], 'runtime_eula_acceptance_required'],
    [['stage', home, '0'.repeat(64), '--user-accepted-minecraft-eula'], 'runtime_plan_approval'],
    [['plan', join(root, 'sensitive-marker-absent')], 'runtime_io_or_network'],
  ]) {
    const result = await childCli(args);
    assert.equal(result.code, 1); assert.equal(result.stderr, '');
    assert.deepEqual(JSON.parse(result.stdout), { event: 'failed', code: expected });
    assert.ok(!result.stdout.includes(root)); assert.ok(!result.stdout.includes('sensitive-marker'));
  }
  assert.deepEqual(await stagingSnapshot(), before);
});

test('readLocal enforces exact limits and rejects hardlinks, directories and linked parents', async t => {
  const root = await fixture(t); const file = join(root, 'bytes');
  await writeFile(file, payload);
  assert.deepEqual(await readLocal(file, payload.length), payload);
  await assert.rejects(readLocal(file, payload.length - 1), { message: 'runtime_unsafe_file' });
  await assert.rejects(readLocal(root, 100), { message: 'runtime_unsafe_file' });
  await link(file, join(root, 'hardlink'));
  await assert.rejects(readLocal(file, payload.length), { message: 'runtime_unsafe_file' });
  await unlink(join(root, 'hardlink'));
  const directoryLink = join(root, 'linked-parent');
  await symlink(join(root, 'absent-target'), directoryLink, process.platform === 'win32' ? 'junction' : 'dir');
  await assert.rejects(readLocal(join(directoryLink, 'file'), 100), { message: 'linked_local_parent' });
  await unlink(directoryLink);
});

test('unsafe network, device, alternate-stream and ambiguous paths fail before fetch', async t => {
  await fixture(t); let fetched = 0;
  const fetcher = () => { fetched++; throw new Error('unreachable'); };
  for (const path of ['\\\\host.invalid\\share\\file', '//host.invalid/share/file', '\\\\?\\C:\\file', 'C:file', 'NUL', 'file:stream', 'bad. ', '\u0000']) {
    await assert.rejects(readLocal(path, 100), { message: 'unsafe_local_path_syntax' });
    await assert.rejects(downloadVerified(pinFor(), path, { fetcher }), { message: 'unsafe_local_path_syntax' });
  }
  assert.equal(fetched, 0);
});

test('download pin, origin and timeout validation precede filesystem and fetch', async t => {
  const root = await fixture(t); let fetched = 0;
  const fetcher = () => { fetched++; throw new Error('unreachable'); };
  const destination = join(root, 'new.jar');
  for (const url of ['http://maven.fabricmc.net/f', 'https://elsewhere.invalid/f', 'https://user:password@maven.fabricmc.net/f', 'https://maven.fabricmc.net:8443/f', 'https://maven.fabricmc.net/f#fragment']) {
    await assert.rejects(downloadVerified({ ...pinFor(), url }, destination, { fetcher }), { message: 'runtime_download_origin' });
  }
  for (const fields of [{ bytes: 0 }, { bytes: 1.5 }, { bytes: 64 * 1024 * 1024 + 1 }, { algorithm: 'md5' }, { digest: 'A'.repeat(64) }, { digest: 'x' }]) {
    await assert.rejects(downloadVerified({ ...pinFor(), ...fields }, destination, { fetcher }), { message: 'runtime_download_pin' });
  }
  for (const timeoutMs of [0, -1, 90001, 0.5]) {
    await assert.rejects(downloadVerified(pinFor(), destination, { fetcher, timeoutMs }), { message: 'runtime_download_timeout' });
  }
  assert.equal(fetched, 0); assert.deepEqual(await readdir(root), []);
});

test('Modrinth admits only the exact SVC pin before filesystem or fetch and never downloads in this fixture', async t => {
  const root = await fixture(t); let fetched = 0;
  const fetcher = (url, options) => {
    fetched++;
    assert.equal(url, SVC_ARTIFACT.url);
    assert.equal(options.redirect, 'error'); assert.equal(options.credentials, 'omit');
    assert.equal(options.headers['Accept-Encoding'], 'identity');
    throw new Error('fixture_modrinth_fetch_denied');
  };
  // The missing destination parent makes an accidental filesystem call fail
  // differently. Each mutated pin must be rejected before that access.
  const absentParentDestination = join(root, 'must-not-exist', 'rejected.jar');
  for (const fields of [
    { file: 'mods/extra.jar' }, { file: 'voicechat-fabric-1.21.1-2.6.22.jar' },
    { bytes: SVC_ARTIFACT.bytes + 1 }, { digest: '0'.repeat(64) },
    { algorithm: 'sha1', digest: '0'.repeat(40) },
    { url: SVC_ARTIFACT.url + '?download=1' }, { url: SVC_ARTIFACT.url + '#fragment' },
    { url: SVC_ARTIFACT.url.replace('VS5X7jZd', 'another-release') },
    { url: SVC_ARTIFACT.url.replace('cdn.modrinth.com', 'api.modrinth.com') },
    { url: 'https://maven.fabricmc.net/fixture/attempted-svc-replacement.jar' },
  ]) {
    await assert.rejects(downloadVerified({ ...SVC_ARTIFACT, ...fields }, absentParentDestination, { fetcher }),
      error => /^runtime_download_(?:origin|pin)$/.test(error.message));
  }
  assert.equal(fetched, 0); assert.deepEqual(await readdir(root), []);
  const destination = join(root, 'approved-synthetic-attempt.jar');
  await assert.rejects(downloadVerified(SVC_ARTIFACT, destination, { fetcher }), { message: 'fixture_modrinth_fetch_denied' });
  assert.equal(fetched, 1);
  assert.equal(await exists(destination), false);
  assert.equal((await lstat(`${destination}.part`)).size, 0);
});

for (const pin of [PLASMO_ARTIFACT, FABRIC_API_ARTIFACT]) test(`Modrinth restricts every field of ${pin.file} before filesystem access`, async t => {
  const root = await fixture(t); let fetched = 0;
  const fetcher = (url, options) => {
    fetched++; assert.equal(url, pin.url);
    assert.equal(options.redirect, 'error'); assert.equal(options.credentials, 'omit');
    assert.equal(options.headers['Accept-Encoding'], 'identity');
    throw new Error('fixture_modrinth_fetch_denied');
  };
  const destination = join(root, 'must-not-exist', 'rejected.jar');
  for (const fields of [{ file: 'mods/extra.jar' }, { file: pin.file.split('/').at(-1) },
    { bytes: pin.bytes + 1 }, { digest: '0'.repeat(64) }, { algorithm: 'sha1', digest: '0'.repeat(40) },
    { url: pin.url + '?download=1' }, { url: pin.url + '#fragment' },
    { url: pin.url.replace('/versions/', '/different/') },
    { url: pin.url.replace('cdn.modrinth.com', 'api.modrinth.com') },
    { url: 'https://maven.fabricmc.net/fixture/attempted-replacement.jar' }]) {
    await assert.rejects(downloadVerified({ ...pin, ...fields }, destination, { fetcher }),
      error => /^runtime_download_(?:origin|pin)$/.test(error.message));
  }
  const other = pin === PLASMO_ARTIFACT ? FABRIC_API_ARTIFACT : PLASMO_ARTIFACT;
  await assert.rejects(downloadVerified({ ...pin, url: other.url }, destination, { fetcher }), /runtime_download_origin/);
  assert.equal(fetched, 0); assert.deepEqual(await readdir(root), []);
  const admitted = join(root, 'admitted-fixture-attempt.jar');
  await assert.rejects(downloadVerified(pin, admitted, { fetcher }), { message: 'fixture_modrinth_fetch_denied' });
  assert.equal(fetched, 1); assert.equal(await exists(admitted), false);
  assert.equal((await lstat(`${admitted}.part`)).size, 0);
});

test('real local HTTP verifies SHA-1 and SHA-256 and publishes exact bytes only after completion', async t => {
  const root = await fixture(t);
  let release; const releaseBody = new Promise(resolve => { release = resolve; });
  let firstRequest; const started = new Promise(resolve => { firstRequest = resolve; });
  const http = await localHttp(t, async (request, response) => {
    response.writeHead(200, { 'Content-Length': payload.length });
    response.write(payload.subarray(0, 31)); firstRequest();
    await releaseBody; response.end(payload.subarray(31));
  });
  for (const algorithm of ['sha256', 'sha1']) {
    const destination = join(root, `${algorithm}.jar`);
    const running = downloadVerified(pinFor('ok', algorithm), destination, { fetcher: http.fetcher });
    await started;
    if (algorithm === 'sha256') {
      assert.equal(await exists(destination), false);
      assert.equal(await exists(`${destination}.part`), true);
      release();
    }
    const verified = await running;
    assert.deepEqual(verified, { file: 'fixture.jar', bytes: payload.length, sha256: hash(payload), algorithm, digest: digest(algorithm, payload) });
    assert.deepEqual(await readFile(destination), payload);
    assert.equal(await exists(`${destination}.part`), false);
  }
  assert.equal(http.requests.length, 2);
});

test('real local HTTP rejects wrong hashes, lengths, extra bytes, encoding, redirects and status', async t => {
  const root = await fixture(t);
  const http = await localHttp(t, (request, response) => {
    const route = request.url.split('/').at(-1);
    if (route === 'redirect') { response.writeHead(302, { Location: '/fixture/ok' }); response.end(); return; }
    if (route === 'status') { response.writeHead(503); response.end(); return; }
    if (route === 'encoding') { response.writeHead(200, { 'Content-Length': payload.length, 'Content-Encoding': 'gzip' }); response.end(payload); return; }
    if (route === 'length') { response.writeHead(200, { 'Content-Length': payload.length + 1 }); response.end(payload); return; }
    if (route === 'hash') { response.end(Buffer.alloc(payload.length, 42)); return; }
    if (route === 'short') { response.write(payload.subarray(0, 10)); response.end(); return; }
    if (route === 'oversize') { response.write(payload); response.end(Buffer.from('extra')); return; }
    response.end(payload);
  });
  for (const [route, code] of [['hash', 'runtime_download_hash'], ['length', 'runtime_download_size'], ['short', 'runtime_download_size'], ['oversize', 'runtime_download_size'], ['encoding', 'runtime_download_encoding'], ['status', 'runtime_download_response'], ['redirect', undefined]]) {
    const destination = join(root, `${route}.jar`);
    await assert.rejects(downloadVerified(pinFor(route), destination, { fetcher: http.fetcher, timeoutMs: 1000 }), code ? { message: code } : undefined);
    assert.equal(await exists(destination), false);
    assert.ok((await lstat(`${destination}.part`)).size <= payload.length);
  }
  assert.ok(!http.requests.includes('/fixture/ok'), 'redirect target must not be fetched');
});

test('real local HTTP rejects a transport-truncated body and never publishes partial data', async t => {
  const root = await fixture(t);
  const http = await localHttp(t, (_request, response) => {
    response.writeHead(200, { 'Content-Length': payload.length }); response.flushHeaders();
    response.write(payload.subarray(0, 12));
    setTimeout(() => response.destroy(), 20);
  });
  const destination = join(root, 'truncated.jar');
  await assert.rejects(downloadVerified(pinFor(), destination, { fetcher: http.fetcher, timeoutMs: 1000 }));
  assert.equal(await exists(destination), false);
  assert.ok((await lstat(`${destination}.part`)).size <= 12);
});

test('deadlines and external cancellation stop stalled HTTP headers/body without publication', async t => {
  const root = await fixture(t);
  let announce; let received = new Promise(resolve => { announce = resolve; });
  const http = await localHttp(t, (request, response) => {
    if (request.url.endsWith('/body')) { response.writeHead(200, { 'Content-Length': payload.length }); response.write(payload.subarray(0, 11)); }
    announce();
  });
  for (const route of ['headers', 'body']) {
    const destination = join(root, `${route}.jar`); const before = performance.now();
    await assert.rejects(downloadVerified(pinFor(route), destination, { fetcher: http.fetcher, timeoutMs: 100 }));
    assert.ok(performance.now() - before < 2000);
    assert.equal(await exists(destination), false);
  }
  received = new Promise(resolve => { announce = resolve; });
  const controller = new AbortController(); const destination = join(root, 'cancel.jar');
  const running = downloadVerified(pinFor('body'), destination, { fetcher: http.fetcher, timeoutMs: 2000, signal: controller.signal });
  const rejected = assert.rejects(running);
  await received; controller.abort(); await rejected;
  assert.equal(await exists(destination), false);
  const aborted = new AbortController(); aborted.abort(); const count = http.requests.length;
  await assert.rejects(downloadVerified(pinFor(), join(root, 'preabort.jar'), { fetcher: http.fetcher, signal: aborted.signal }));
  assert.equal(http.requests.length, count);
  assert.equal(await exists(join(root, 'preabort.jar.part')), false);
});

test('existing destinations and partials, including hardlinks, are preserved without fetch', async t => {
  const root = await fixture(t); let fetched = 0;
  const fetcher = () => { fetched++; throw new Error('unreachable'); };
  const original = Buffer.from('unrelated preserved bytes');
  const source = join(root, 'original'); await writeFile(source, original);
  for (const kind of ['file', 'hardlink', 'partial', 'partial-hardlink']) {
    const destination = join(root, `${kind}.jar`); const occupied = kind.startsWith('partial') ? `${destination}.part` : destination;
    if (kind.includes('hardlink')) await link(source, occupied); else await writeFile(occupied, original);
    await assert.rejects(downloadVerified(pinFor(), destination, { fetcher }));
    assert.deepEqual(await readFile(occupied), original);
    assert.deepEqual(await readFile(source), original);
  }
  assert.equal(fetched, 0);
});

test('destination parent junctions are rejected without touching their target or fetch', async t => {
  const root = await fixture(t); const parent = join(root, 'linked-parent');
  await symlink(join(root, 'nonexistent'), parent, process.platform === 'win32' ? 'junction' : 'dir');
  let fetched = 0;
  await assert.rejects(downloadVerified(pinFor(), join(parent, 'file.jar'), { fetcher: () => { fetched++; } }), { message: 'linked_local_parent' });
  assert.equal(fetched, 0); assert.equal(await exists(join(root, 'nonexistent')), false);
  await unlink(parent);
});

test('a destination appearing during the HTTP transfer cannot be overwritten', async t => {
  const root = await fixture(t); const destination = join(root, 'race.jar'); const marker = Buffer.from('other local writer');
  const http = await localHttp(t, async (_request, response) => { await writeFile(destination, marker); response.end(payload); });
  await assert.rejects(downloadVerified(pinFor(), destination, { fetcher: http.fetcher }), { message: 'runtime_destination_exists' });
  assert.deepEqual(await readFile(destination), marker);
  assert.deepEqual(await readFile(`${destination}.part`), payload);
});

test('cancellation while syncing verified bytes still prevents final publication', async t => {
  const root = await fixture(t); const destination = join(root, 'cancel-after-stream.jar');
  const controller = new AbortController();
  const probe = await open(join(root, 'handle-probe'), 'wx');
  const prototype = Object.getPrototypeOf(probe); const originalSync = prototype.sync;
  await probe.close();
  // Inject cancellation at an actual asynchronous durability boundary after
  // the HTTP body and digest have completed. No production method is edited.
  let synced = 0;
  t.mock.method(prototype, 'sync', async function (...args) {
    await originalSync.apply(this, args); synced++; controller.abort();
  });
  const http = await localHttp(t, (_request, response) => response.end(payload));
  await assert.rejects(downloadVerified(pinFor(), destination, { fetcher: http.fetcher, signal: controller.signal }));
  assert.equal(synced, 1);
  assert.equal(await exists(destination), false);
  assert.deepEqual(await readFile(`${destination}.part`), payload);
});

for (const profile of ['status', 'svc', 'plasmo']) test(`stage failure after synthetic approval preserves only its new unready directory (${profile})`, { skip: process.platform !== 'win32' }, async t => {
  const root = await fixture(t); const home = await fakeJdk(root);
  const plan = await makePlan(home, profile); const approval = planDigest(plan);
  const before = await stagingSnapshot() ?? [];
  let ownedDirectory;
  t.after(async () => {
    if (!ownedDirectory) return;
    assert.equal(dirname(ownedDirectory), stagingParent);
    assert.ok(isAbsolute(ownedDirectory));
    const saved = JSON.parse(await readFile(join(ownedDirectory, 'plan.json'), 'utf8'));
    assert.equal(saved.java.home, resolve(home));
    assert.equal(planDigest(saved), approval);
    // Only the uniquely allocated directory proven to belong to this fixture.
    // Never remove its parent, a previous prepared lab or a caller's directory.
    await rm(ownedDirectory, { recursive: true, force: true });
  });
  const originalFetch = globalThis.fetch; const requested = [];
  globalThis.fetch = url => { requested.push(url); throw new Error('fixture_network_refused'); };
  try {
    // Simulated consent for this fake JDK only. There is no HTTP request or
    // Minecraft download, and the fixture consent record is removed on cleanup.
    await assert.rejects(stageRuntime(home, approval, true, profile), { message: 'fixture_network_refused' });
  } finally { globalThis.fetch = originalFetch; }
  assert.deepEqual(requested, [artifactPins()[0].url]);
  const after = await stagingSnapshot();
  const added = after.filter(name => !before.includes(name));
  assert.equal(added.length, 1);
  assert.match(added[0], /^prepared-[a-zA-Z0-9]+$/);
  const candidate = join(stagingParent, added[0]);
  const savedPlan = JSON.parse(await readFile(join(candidate, 'plan.json'), 'utf8'));
  assert.equal(savedPlan.java.home, resolve(home));
  assert.equal(planDigest(savedPlan), approval);
  ownedDirectory = candidate;
  assert.deepEqual((await readdir(candidate)).sort(), ['consent.json', 'failed.json', 'libraries', 'plan.json', 'server.jar.part',
    ...(['svc', 'plasmo'].includes(profile) ? ['config', 'mods'] : [])].sort());
  assert.deepEqual(JSON.parse(await readFile(join(candidate, 'consent.json'), 'utf8')), {
    planSha256: approval, downloadEulaAccepted: true, javaExecuted: false,
  });
  assert.deepEqual(JSON.parse(await readFile(join(candidate, 'failed.json'), 'utf8')), { event: 'runtime_staging_failed', ready: false });
  assert.equal((await lstat(join(candidate, 'server.jar.part'))).size, 0);
  assert.deepEqual(await readdir(join(candidate, 'libraries')), []);
  if (profile === 'svc') {
    assert.deepEqual(await readdir(join(candidate, 'mods')), []);
    assert.deepEqual(await readdir(join(candidate, 'config')), ['voicechat']);
    assert.deepEqual(await readdir(join(candidate, 'config/voicechat')), []);
    assert.equal(await exists(join(candidate, SVC_CONFIG_PATH)), false);
    assert.equal(await exists(join(candidate, SVC_ARTIFACT.file)), false);
  }
  if (profile === 'plasmo') {
    assert.deepEqual(await readdir(join(candidate, 'mods')), []);
    assert.deepEqual(await readdir(join(candidate, 'config')), ['plasmovoice']);
    assert.deepEqual(await readdir(join(candidate, 'config/plasmovoice')), ['server']);
    assert.deepEqual(await readdir(join(candidate, 'config/plasmovoice/server')), []);
    for (const file of [PLASMO_CONFIG_PATH, PLASMO_ARTIFACT.file, FABRIC_API_ARTIFACT.file]) {
      assert.equal(await exists(join(candidate, file)), false);
    }
  }
  for (const file of ['ready.json', 'eula.txt', 'server.properties', 'server.jar']) assert.equal(await exists(join(candidate, file)), false);
  assert.ok(before.every(name => after.includes(name)), 'previous staging entries remain');
});
