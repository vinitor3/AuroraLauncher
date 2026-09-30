import { lstat, mkdir, mkdtemp, readdir, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { makeRunPlan, runPlanDigest } from './server.mjs';
import { makePlan } from './runtime.mjs';
import { runtimeArtifacts } from './play-profile.mjs';
import { SVC_CONFIG_PATH, validateSvcConfig } from './svc-profile.mjs';
import { PLASMO_CONFIG_PATH, validatePlasmoConfig } from './plasmo-profile.mjs';
import { readLocal, hash } from './files.mjs';
import { requireLabRoot, minimalEnvironment, withLabEula, listeners, ready } from './server-lifecycle.mjs';
import { withOwnedProcess } from './owned-process.mjs';
import { LiveIpc, INSTANCE_ID, readNativeFailure } from './live-ipc.mjs';
import { parseConfig } from '../connect-voice-tools/inspect.mjs';
import { rejectLinkedParents } from '../connect-voice-tools/local-path.mjs';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const PARENT = join(REPO, '.aurora/connect-runtime');
const ARTIFACTS = join(REPO, '.aurora/connect-voice-artifacts');
const HELPER_TEST = 'engine::connect_voice::tests::live::native_live_writer_roundtrip';
const json = value => `${JSON.stringify(value, null, 2)}\n`;
const fail = code => { throw new Error(code); };
const digestPattern = /^[a-f0-9]{64}$/;
const configPath = profile => profile === 'svc' ? SVC_CONFIG_PATH : PLASMO_CONFIG_PATH;

async function helperInfo(name) {
  if (typeof name !== 'string' || !/^aurora_launcher_core-[0-9a-f]{16}\.exe$/.test(name)) fail('live_helper_name');
  const file = join(REPO, 'apps/desktop/src-tauri/target/debug/deps', name);
  const bytes = await readLocal(file, 64 * 1024 * 1024);
  if (bytes.length < 2 || bytes.toString('ascii', 0, 2) !== 'MZ') fail('live_helper_binary');
  return { file, sha256: hash(bytes), bytes: bytes.length, test: HELPER_TEST };
}

export async function makeLivePlan(javaHome, prepared, profile, helperName) {
  if (!['svc', 'plasmo'].includes(profile)) fail('live_profile');
  const server = await makeRunPlan(javaHome, prepared, profile);
  const resources = join(PARENT, prepared);
  const launchArgs = ['-Djava.io.tmpdir=.connect-run-temp', '-Duser.home=.connect-run-temp', '-XX:-UsePerfData',
    '-Xms256M', '-Xmx1536M', `-Dfabric.gameJarPath=${join(resources, 'server.jar')}`, '-cp',
    runtimeArtifacts(profile).filter(pin => pin.file.startsWith('libraries/')).map(pin => join(resources, pin.file)).join(';'),
    'net.fabricmc.loader.impl.launch.knot.KnotServer', 'nogui'];
  return { schemaVersion: 1, purpose: 'aurora_connect_native_voice_writer_roundtrip', profile,
    resourceStage: prepared, resourceRunPlanSha256: runPlanDigest(server), java: server.java,
    artifacts: server.artifacts, tools: server.tools, helper: await helperInfo(helperName),
    helperArgs: [HELPER_TEST, '--exact', '--ignored', '--test-threads=1', '--nocapture'],
    target: 'fresh_preserved_diagnostic_copy_under_voice_roundtrip_native_instances',
    launch: { args: launchArgs, cwd: 'derived_verified_diagnostic_copy', environment: 'minimal_windows_and_owned_temp_only' },
    configuration: { sourceSha256: server.voiceConfig.sha256, guestAnnouncement: '127.0.0.1:35554',
      fields: profile === 'svc' ? ['voice_host'] : ['host.public.ip', 'host.public.port'],
      independentApplyAndRestoreApproval: true, writerMayReserialize: true },
    eula: server.diff, javaLimits: server.limits,
    limits: { nativeSeconds: 480, nativeSupervisorSeconds: 490, orchestrationSeconds: 600, ipcBytes: 4096, stopSeconds: 5,
      helperOutputBytes: 65536, javaOutputBytes: 4194304, relay: false, audio: false,
      personalInstanceAccess: false, crashRestoration: false, runtimeHomologated: false },
    sourceStageReadOnly: true, resourcesRecheckedAfterRun: true,
    rollback: 'native_receipt_in_memory_with_reviewed_restore_after_java_close_or_preserved_recovery_files' };
}
export const livePlanDigest = plan => hash(json(plan));

// Verify actual announced bytes without creating another properties/TOML writer.
// All normal boot invariants still apply after removing only the owned fields
// from an in-memory validation view. No normalized bytes are written to disk.
export function inspectAnnouncedConfig(profile, bytes) {
  if (!['svc', 'plasmo'].includes(profile)) fail('live_profile');
  let parsed;
  try { parsed = parseConfig(profile, bytes); } catch { fail('live_config_invalid'); }
  const text = bytes.toString('utf8');
  if (profile === 'svc') {
    if (parsed.entries.get('voice_host')?.value !== '127.0.0.1:35554') fail('live_announcement');
    const lines = text.split('\n'); let removed = 0;
    const view = lines.map(line => {
      const match = /^([ \t]*voice_host[ \t]*=[ \t]*)127\.0\.0\.1:35554(\r?)$/.exec(line);
      if (!match) return line; removed++; return `${match[1]}${match[2]}`;
    }).join('\n');
    if (removed !== 1) fail('live_announcement');
    validateSvcConfig(Buffer.from(view));
  } else {
    const fields = [...parsed.entries.keys()].filter(key => key === 'host.public' || key.startsWith('host.public.')).sort();
    if (fields.join(',') !== 'host.public.ip,host.public.port'
      || parsed.entries.get('host.public.ip')?.value !== '127.0.0.1'
      || parsed.entries.get('host.public.port')?.value !== 35554) fail('live_announcement');
    let section; let headers = 0; let removed = 0;
    const view = text.split('\n').map(line => {
      const header = /^[ \t]*\[([a-zA-Z0-9_.-]+)\][ \t]*(?:#.*)?\r?$/.exec(line);
      if (header) { section = header[1]; if (section === 'host.public') { headers++; return ''; } }
      if (section !== 'host.public') return line;
      if (/^[ \t]*(?:#.*)?\r?$/.test(line)) return line;
      if (/^[ \t]*(?:ip[ \t]*=[ \t]*["']127\.0\.0\.1["']|port[ \t]*=[ \t]*35554)[ \t]*(?:#.*)?\r?$/.test(line)) {
        removed++; return '';
      }
      fail('live_announcement');
    }).join('\n');
    if (headers !== 1 || removed !== 2) fail('live_announcement');
    validatePlasmoConfig(Buffer.from(view));
  }
  return { sha256: hash(bytes), size: bytes.length, announcementLoopback: true, announcementPort: 35554,
    keyCount: parsed.entries.size };
}

export async function verifyCopyTree(root, profile) {
  if (!['svc', 'plasmo'].includes(profile)) fail('live_profile');
  await requireLabRoot(root);
  const exactDirectory = async (relative, names) => {
    const path = join(root, relative); await rejectLinkedParents(path);
    const metadata = await lstat(path);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()
      || (await readdir(path)).sort().join(',') !== [...names].sort().join(',')) fail('live_copy_tree');
  };
  await exactDirectory('', ['.aurora', 'aurora-instance.json', 'config', 'mods']);
  await exactDirectory('.aurora', ['recovery-identity.json', 'connect-voice-stage', 'recovery-stage']);
  await exactDirectory('.aurora/connect-voice-stage', []);
  // Recovery retains one completed transaction's empty new/old directories.
  // Accept that exact observed shape, never arbitrary recovery-stage contents.
  const recovery = join(root, '.aurora/recovery-stage');
  await rejectLinkedParents(recovery);
  const transactions = await readdir(recovery);
  if (transactions.length !== 1 || !/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(transactions[0])) fail('live_copy_tree');
  await exactDirectory('.aurora/recovery-stage', transactions);
  const transaction = `.aurora/recovery-stage/${transactions[0]}`;
  await exactDirectory(transaction, ['new', 'old']);
  await exactDirectory(`${transaction}/new`, []); await exactDirectory(`${transaction}/old`, []);
  await readLocal(join(root, '.aurora/recovery-identity.json'), 4096);
  let metadata;
  try { metadata = JSON.parse(await readLocal(join(root, 'aurora-instance.json'), 4096)); } catch { fail('live_copy_metadata'); }
  if (!metadata || Object.keys(metadata).sort().join(',') !== 'loader,minecraft,versionId'
    || metadata.minecraft !== '1.21.1' || metadata.loader !== 'fabric' || metadata.versionId !== '1.21.1') fail('live_copy_metadata');
  if (profile === 'svc') {
    await exactDirectory('config', ['voicechat']); await exactDirectory('config/voicechat', ['voicechat-server.properties']);
  } else {
    await exactDirectory('config', ['plasmovoice']); await exactDirectory('config/plasmovoice', ['server']);
    await exactDirectory('config/plasmovoice/server', ['config.toml']);
  }
  await exactDirectory('mods', runtimeArtifacts(profile).filter(pin => pin.file.startsWith('mods/')).map(pin => pin.file.slice(5)));
}

async function checkCopy(root, profile, expectedHash) {
  await verifyCopyTree(root, profile);
  const bytes = await readLocal(join(root, configPath(profile)), 256 * 1024);
  if (hash(bytes) !== expectedHash) fail('live_copy_config_changed');
  inspectAnnouncedConfig(profile, bytes);
  const pins = runtimeArtifacts(profile).filter(pin => pin.file.startsWith('mods/'));
  for (const pin of pins) {
    const jar = await readLocal(join(root, pin.file), pin.bytes);
    if (jar.length !== pin.bytes || hash(jar) !== pin.digest) fail('live_copy_mod_hash');
  }
}

async function preparedResources(javaHome, prepared, profile, expected) {
  if (runPlanDigest(await makeRunPlan(javaHome, prepared, profile)) !== expected) fail('live_resources_changed');
}

export async function runLiveWriter(javaHome, prepared, profile, helperName, approval, acceptedEula, { signal, onReview } = {}) {
  if (acceptedEula !== true) fail('live_eula_acceptance_required');
  if (typeof approval !== 'string' || !digestPattern.test(approval) || typeof onReview !== 'function'
    || (signal !== undefined && !(signal instanceof AbortSignal))) fail('live_approval');
  const plan = await makeLivePlan(javaHome, prepared, profile, helperName);
  if (livePlanDigest(plan) !== approval) fail('live_plan_changed');
  signal?.throwIfAborted();
  await rejectLinkedParents(PARENT);
  const probeHome = await mkdtemp(join(PARENT, 'probe-'));
  if ((await listeners(PARENT, probeHome, signal)).length || (await listeners(PARENT, probeHome, signal, profile)).length) fail('live_port_in_use');
  // Confirm after the potentially slow OS queries and before creating the lab.
  if (livePlanDigest(await makeLivePlan(javaHome, prepared, profile, helperName)) !== approval) fail('live_plan_changed');
  const root = await mkdtemp(join(PARENT, 'voice-roundtrip-'));
  const ipcDirectory = join(root, 'ipc'); await mkdir(ipcDirectory);
  const ipc = new LiveIpc(ipcDirectory, profile);
  const timeout = AbortSignal.timeout(600000);
  const combined = signal ? AbortSignal.any([signal, timeout]) : timeout;
  let callbackWork; let javaWork; let javaResult; let restored; let failure;
  const approveReview = async (review, childSignal) => {
    if (await onReview(Object.freeze(review), childSignal) !== review.digest) fail('live_mutation_approval');
    childSignal.throwIfAborted();
  };
  let nativeResult;
  try {
    nativeResult = await withOwnedProcess({ executable: plan.helper.file,
      args: plan.helperArgs, cwd: REPO,
      env: { ...minimalEnvironment(probeHome), AURORA_CONNECT_LIVE_LAB_ROOT: root,
        AURORA_CONNECT_LIVE_MOD: profile, AURORA_CONNECT_ARTIFACT_DIR: ARTIFACTS },
      signal: combined, timeoutMs: 490000, stopTimeoutMs: 5000, outputLimitBytes: 65536 }, ({ signal: nativeSignal }) => {
      callbackWork = (async () => {
      const apply = await ipc.receive(0, nativeSignal);
      if (apply.sourceSha256 !== plan.configuration.sourceSha256) fail('live_source_hash');
      await approveReview(apply, nativeSignal);
      await ipc.send('apply-approval.json', { digest: apply.digest });
      const locked = await ipc.receive(1, nativeSignal);
      if (locked.instanceId !== apply.instanceId || !INSTANCE_ID.test(locked.instanceId)
        || locked.configSha256 !== apply.replacementSha256) fail('live_lock_identity');
      const copy = join(root, 'native/instances', locked.instanceId);
      await checkCopy(copy, profile, locked.configSha256);
      nativeSignal.throwIfAborted();
      await preparedResources(javaHome, prepared, profile, plan.resourceRunPlanSha256);
      nativeSignal.throwIfAborted();
      const baseline = await makePlan(javaHome, profile);
      for (const file of ['server.properties', 'eula.txt']) {
        nativeSignal.throwIfAborted(); await writeFile(join(copy, file), baseline.files[file], { flag: 'wx' });
      }
      nativeSignal.throwIfAborted();
      const temp = join(copy, '.connect-run-temp'); await mkdir(temp);
      // The sibling JVM is always reaped, including if the native helper exits
      // while its callback is running. Only its observed close permits restore.
      nativeSignal.throwIfAborted();
      javaWork = withLabEula(copy, approval, async () => withOwnedProcess({ executable: plan.java.executable,
        args: plan.launch.args, cwd: copy, env: minimalEnvironment(temp), signal: nativeSignal,
        timeoutMs: 300000, stopTimeoutMs: 5000, outputLimitBytes: 4194304 },
      async ({ signal: javaSignal, ownsProcessId }) => ready(copy, probeHome, javaSignal, ownsProcessId, profile)));
      javaResult = await javaWork;
      const writer = inspectAnnouncedConfig(profile, await readLocal(join(copy, configPath(profile)), 256 * 1024));
      await ipc.send('java-closed.json', { ...Object.fromEntries(['closed', 'exitCode', 'forced'].map(key => [key, javaResult.value.process[key]])),
        eulaRestored: javaResult.eulaRestored, configSha256: writer.sha256 });
      const restore = await ipc.receive(2, nativeSignal);
      if (restore.sourceSha256 !== writer.sha256) fail('live_writer_hash');
      await approveReview(restore, nativeSignal);
      await ipc.send('restore-approval.json', { digest: restore.digest });
      restored = await ipc.receive(3, nativeSignal);
      if (restored.sourceSha256 !== apply.sourceSha256 || restored.appliedSha256 !== apply.replacementSha256
        || restored.writerSha256 !== writer.sha256 || restored.restoredSha256 !== restore.replacementSha256
        || restored.mode !== restore.mode) fail('live_restored_identity');
      const bytes = await readLocal(join(copy, configPath(profile)), 256 * 1024);
      if (hash(bytes) !== restored.restoredSha256) fail('live_restored_changed');
      (profile === 'svc' ? validateSvcConfig : validatePlasmoConfig)(bytes);
      await preparedResources(javaHome, prepared, profile, plan.resourceRunPlanSha256);
      return { writer, mode: restored.mode };
      })();
      return callbackWork;
    });
  } catch (error) { failure = error; }
  finally {
    if (failure) await writeFile(join(ipcDirectory, 'cancel.json'), '{}\n', { flag: 'wx' }).catch(() => {});
    // Reap the entire callback, including preparation before javaWork exists.
    // No delayed writes or sibling cleanup can continue after this returns.
    if (callbackWork) await callbackWork.catch(error => {
      failure ??= error;
      // Preserve only a fixed local diagnostic, never a path or raw child log.
      if (/^(live|server|runtime)_[a-z_]+$/.test(error.message)) failure.operationCode = error.message;
    });
  }
  if (failure) {
    const nativeCode = await readNativeFailure(ipcDirectory);
    if (nativeCode) failure.operationCode = nativeCode;
    throw failure;
  }
  return { event: 'native_writer_roundtrip', result: 'PASS', profile, labBasename: root.split(/[\\/]/).at(-1),
    planSha256: approval, native: nativeResult.process, java: javaResult.value.process,
    eulaRestored: javaResult.eulaRestored, restoration: restored.mode,
    sourceUnchanged: restored.sourceUnchanged, unownedPreserved: restored.unownedPreserved,
    writer: nativeResult.value.writer, restoredSha256: restored.restoredSha256,
    runtimeHomologated: false, voiceAuthentication: 'not_tested', voiceAudio: 'not_tested', relay: false };
}

export async function readApproval(input, expected, signal) {
  return new Promise((resolveApproval, reject) => {
    let bytes = Buffer.alloc(0);
    const finish = error => { input.off('data', data); input.off('end', ended); input.off('error', ended);
      signal.removeEventListener('abort', aborted); input.pause(); error ? reject(error) : resolveApproval(expected); };
    const data = chunk => { bytes = Buffer.concat([bytes, Buffer.from(chunk)]);
      if (bytes.length > 128) return finish(new Error('live_mutation_approval'));
      if (bytes.includes(10)) finish(bytes.toString('utf8') === `approve ${expected}\n`
        || bytes.toString('utf8') === `approve ${expected}\r\n` ? undefined : new Error('live_mutation_approval')); };
    const ended = () => finish(new Error('live_mutation_approval'));
    const aborted = () => finish(new Error('live_cancelled'));
    input.on('data', data); input.once('end', ended); input.once('error', ended);
    signal.addEventListener('abort', aborted, { once: true });
    if (signal.aborted) aborted(); else if (input.readableEnded) ended(); else input.resume();
  });
}

export async function main(args, { input = process.stdin, signal, emit = value => process.stdout.write(json(value)) } = {}) {
  if (args.length === 5 && args[0] === 'plan') {
    const plan = await makeLivePlan(...args.slice(1));
    return { event: 'native_writer_plan', planSha256: livePlanDigest(plan), plan };
  }
  if (args.length === 7 && args[0] === 'run' && args[6] === '--user-accepted-minecraft-eula') {
    return runLiveWriter(...args.slice(1, 5), args[5], true, { signal,
      onReview: async (review, childSignal) => { emit({ event: 'voice_mutation_review', review }); return readApproval(input, review.digest, childSignal); } });
  }
  fail(args[0] === 'run' ? 'live_eula_acceptance_required' : 'live_usage');
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const controller = new AbortController(); const interrupt = () => controller.abort();
  process.on('SIGINT', interrupt); process.on('SIGTERM', interrupt);
  try { process.stdout.write(json(await main(process.argv.slice(2), { signal: controller.signal }))); }
  catch (error) { process.stdout.write(json({ event: 'failed', code: /^(live|server|runtime|owned_process)_[a-z_]+$/.test(error.message)
    ? error.message : 'live_io_or_process_failed',
    ...(/^(live|server|runtime)_[a-z_]+$/.test(error.operationCode) ? { operationCode: error.operationCode } : {}) })); process.exitCode = 1; }
  finally { process.off('SIGINT', interrupt); process.off('SIGTERM', interrupt); }
}
