import { minimalEnvironment, toolHashes, withLabEula, listeners, ready } from './server-lifecycle.mjs';
export { requireOwnedBind } from './server-lifecycle.mjs';
import { createHash } from 'node:crypto';
import { lstat, readdir, mkdir, mkdtemp } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { constants as osConstants } from 'node:os';
import { EULA_URL } from './pins.mjs';
import { runtimeFiles, runtimeArtifacts } from './play-profile.mjs';
import { validateGuiNames } from './gui-profile.mjs';
import { SVC_ARTIFACT, SVC_CONFIG_PATH, SVC_BASELINE_BYTES, validateSvcConfig } from './svc-profile.mjs';
import { PLASMO_ARTIFACT, PLASMO_CONFIG_PATH, PLASMO_BASELINE_BYTES, validatePlasmoConfig } from './plasmo-profile.mjs';
import { makePlan, planDigest } from './runtime.mjs';
import { absent, hash, readLocal } from './files.mjs';
import { OwnedProcessError, withOwnedProcess } from './owned-process.mjs';
import { rejectLinkedParents, resolveExplicitLocalPath } from '../connect-voice-tools/local-path.mjs';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const PARENT = join(REPO, '.aurora', 'connect-runtime');
const TRUE_EULA = Buffer.from(`# ${EULA_URL}\neula=true\n`);
const FALSE_EULA = Buffer.from(`# ${EULA_URL}\neula=false\n`);
const json = value => `${JSON.stringify(value, null, 2)}\n`;
const fail = code => { throw new Error(code); };
const same = (bytes, expected, code) => { if (!bytes.equals(Buffer.from(expected))) fail(code); };
const digestPattern = /^[a-f0-9]{64}$/;

async function preparedDirectory(basename) {
  if (typeof basename !== 'string' || !/^prepared-[A-Za-z0-9]{6}$/.test(basename)) fail('server_prepared_name');
  const root = resolveExplicitLocalPath(join(PARENT, basename));
  await rejectLinkedParents(root);
  const metadata = await lstat(root);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) fail('server_unsafe_directory');
  return root;
}
async function exactEntries(directory, expected) {
  const entries = (await readdir(directory)).sort();
  if (JSON.stringify(entries) !== JSON.stringify([...expected].sort())) fail('server_stage_entries');
}
async function exactTree(root, fixedPaths) {
  const directories = new Map([['', new Set()]]);
  for (const file of fixedPaths) {
    const parts = file.split('/'); let parent = '';
    for (let index = 0; index < parts.length; index++) {
      directories.get(parent).add(parts[index]);
      if (index < parts.length - 1) {
        parent = parent ? `${parent}/${parts[index]}` : parts[index];
        if (!directories.has(parent)) directories.set(parent, new Set());
      }
    }
  }
  for (const [relative, names] of directories) {
    const directory = join(root, relative);
    await rejectLinkedParents(directory);
    const metadata = await lstat(directory);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) fail('server_unsafe_directory');
    await exactEntries(directory, names);
  }
}

// Read-only verifier, also exercised with synthetic hashes by unit tests. Its
// return is never accepted as execution authority: makeRunPlan/runServer always
// reconstruct the baseline themselves from makePlan and compiled artifactPins.
export async function verifyPreparedFiles(basename, baseline, profile = 'status', guiNames) {
  const fixedFiles = runtimeFiles(profile, guiNames);
  if (profile !== 'gui' && baseline?.profile === 'gui') fail('server_gui_baseline');
  const root = await preparedDirectory(basename);
  await absent(join(root, 'run-used.json'));
  const files = baseline?.files;
  if (!files || Object.keys(files).sort().join(',') !== Object.keys(fixedFiles).sort().join(',')
    || typeof files['server.properties'] !== 'string' || files['eula.txt'] !== FALSE_EULA.toString()
    || Object.values(files).some(value => typeof value !== 'string')) fail('server_baseline');
  if (profile === 'gui' && (baseline.profile !== 'gui'
    || JSON.stringify(baseline.playerNames) !== JSON.stringify(validateGuiNames(guiNames))
    || Object.entries(fixedFiles).some(([file, content]) => files[file] !== content))) fail('server_gui_baseline');
  const names = runtimeArtifacts(profile).map(pin => pin.file);
  if (!baseline || !Array.isArray(baseline.artifacts) || baseline.artifacts.length !== names.length
    || baseline.artifacts.some((pin, i) => pin.file !== names[i]
      || !Number.isSafeInteger(pin.bytes) || pin.bytes < 1 || pin.bytes > 64 * 1024 * 1024
      || !['sha1', 'sha256'].includes(pin.algorithm)
      || !(pin.algorithm === 'sha1' ? /^[a-f0-9]{40}$/ : digestPattern).test(pin.digest))) fail('server_baseline');
  await exactTree(root, ['plan.json', 'consent.json', 'ready.json', ...Object.keys(fixedFiles), ...names]);
  const baselineHash = planDigest(baseline);
  same(await readLocal(join(root, 'plan.json'), 64 * 1024), json(baseline), 'server_baseline_changed');
  same(await readLocal(join(root, 'consent.json'), 4096), json({ planSha256: baselineHash, downloadEulaAccepted: true, javaExecuted: false }), 'server_stage_consent');
  for (const [file, content] of Object.entries(files)) {
    if (file !== 'eula.txt') same(await readLocal(join(root, file), 64 * 1024), content, 'server_config_changed');
  }
  if (profile === 'svc') validateSvcConfig(Buffer.from(files[SVC_CONFIG_PATH]));
  if (profile === 'plasmo') validatePlasmoConfig(Buffer.from(files[PLASMO_CONFIG_PATH]));
  same(await readLocal(join(root, 'eula.txt'), 4096), FALSE_EULA, 'server_eula_changed');
  const verified = [];
  for (const pin of baseline.artifacts) {
    const bytes = await readLocal(join(root, pin.file), pin.bytes);
    if (bytes.length !== pin.bytes || createHash(pin.algorithm).update(bytes).digest('hex') !== pin.digest) fail('server_artifact_changed');
    verified.push({ file: pin.file, bytes: bytes.length, sha256: hash(bytes), algorithm: pin.algorithm, digest: pin.digest });
  }
  same(await readLocal(join(root, 'ready.json'), 64 * 1024),
    json({ planSha256: baselineHash, verified, javaExecuted: false, runtimeHomologated: false }), 'server_ready_changed');
  return { root, baselineHash, verified };
}

export const runPlanDigest = plan => hash(json(plan));
export async function makeRunPlan(javaHome, basename, profile = 'status', guiNames) {
  const baselineProfile = profile === 'status-interactive' ? 'status' : profile;
  const playerNames = profile === 'gui' ? validateGuiNames(guiNames) : guiNames;
  const baseline = await makePlan(javaHome, baselineProfile, playerNames);
  const stage = await verifyPreparedFiles(basename, baseline, baselineProfile, playerNames);
  const tools = await toolHashes();
  return buildRunPlan(basename, baseline, stage, tools, profile);
}

// Pure assembly for review/tests, never execution authority. runServer always
// reconstructs the baseline, verified stage and tool hashes via makeRunPlan.
export function buildRunPlan(basename, baseline, stage, tools, profile = 'status') {
  if (!['status', 'status-interactive', 'play', 'svc', 'plasmo', 'gui'].includes(profile)) fail('runtime_profile');
  const interactive = profile === 'status-interactive';
  const gui = profile === 'gui';
  return {
    schemaVersion: 1, purpose: gui ? 'aurora_connect_disposable_gui_execution' : interactive
      ? 'aurora_connect_disposable_status_interactive_execution' : 'aurora_connect_disposable_server_execution',
    preparedBasename: basename, baselinePlanSha256: stage.baselineHash, java: baseline.java,
    artifacts: stage.verified, tools,
    probeEnvironment: 'fresh_sibling_probe_directory_not_prepared_stage',
    diff: { file: 'eula.txt', beforeSha256: hash(FALSE_EULA), afterSha256: hash(TRUE_EULA),
      before: 'eula=false', after: 'eula=true', restore: 'exact_applied_bytes_only' },
    propertiesSha256: hash(baseline.files['server.properties']),
    ...(interactive ? { profile: 'status-interactive' } : {}),
    ...(gui ? { profile: 'gui', playerNames: validateGuiNames(baseline.playerNames),
      whitelistJson: baseline.files['whitelist.json'], whitelistSha256: hash(baseline.files['whitelist.json']),
      clientLimits: 'one_or_two_reviewed_offline_gui_players_no_compression_or_secure_profile' } : {}),
    ...(profile === 'play' ? { profile: 'play', whitelistSha256: hash(baseline.files['whitelist.json']),
      clientLimits: 'two_synthetic_offline_players_no_compression_or_secure_profile' } : {}),
    ...(profile === 'svc' ? { profile: 'svc', voiceMod: { ...baseline.voiceMod, artifactSha256: SVC_ARTIFACT.digest },
      voiceConfig: { path: SVC_CONFIG_PATH, sha256: hash(baseline.files[SVC_CONFIG_PATH]),
        beforeFirstBoot: 'explicit_loopback_bind_43002_empty_announcement', writerMayReserialize: true,
        acceptedOsBindForms: ['127.0.0.1', '::ffff:127.0.0.1'] },
      voiceEvidence: 'owned_udp_loopback_bind_and_generated_config_only_no_client_or_audio' } : {}),
    ...(profile === 'plasmo' ? { profile: 'plasmo', voiceMod: { ...baseline.voiceMod, artifactSha256: PLASMO_ARTIFACT.digest },
      voiceConfig: { path: PLASMO_CONFIG_PATH, sha256: hash(baseline.files[PLASMO_CONFIG_PATH]),
        beforeFirstBoot: 'explicit_loopback_bind_43003_no_public_announcement', writerMayReserialize: true,
        updateChecks: false, translationsDownload: false,
        acceptedOsBindForms: ['127.0.0.1', '::ffff:127.0.0.1'] },
      voiceEvidence: 'owned_udp_loopback_bind_and_generated_config_only_no_client_or_audio' } : {}),
    launch: { executable: baseline.launch.executable,
      args: ['-Djava.io.tmpdir=.connect-run-temp', '-Duser.home=.connect-run-temp', '-XX:-UsePerfData', ...baseline.launch.args],
      cwd: basename, environment: 'minimal_windows_and_owned_temp_only' },
    evidence: { endpoint: 'loopback_43001', protocol: 767, ownProcessBindRequired: true,
      statusAndPongOnly: true, twoAgentSmoke: 'separate_caller_operation', runtimeHomologated: false,
      worldJoined: false, voice: 'not_tested', accountAuthentication: 'not_tested' },
    limits: { lifetimeSeconds: interactive || gui ? 900 : 300, readinessSeconds: 180, bindProbeSeconds: 10, statusProbeSeconds: 6,
      stopSeconds: 5, outputBytes: 4 * 1024 * 1024,
      reuse: false, personalInstanceAccess: false, relay: false, mods: ['svc', 'plasmo'].includes(profile) },
  };
}

export async function withPreparedEula(basename, approval, operation) {
  if (typeof approval !== 'string' || !digestPattern.test(approval) || typeof operation !== 'function') fail('server_run_approval');
  return withLabEula(await preparedDirectory(basename), approval, operation);
}

// EOF or one exact `stop` line is enough; other input fails closed. The listener
// is removed and paused on every exit, including supervisor cancellation.
export async function waitForStop(input, signal) {
  return new Promise((resolvePromise, reject) => {
    let bytes = Buffer.alloc(0);
    const finish = error => {
      input.off('data', data); input.off('end', end); input.off('error', errorEvent);
      signal.removeEventListener('abort', abort); input.pause();
      if (error) reject(error); else resolvePromise();
    };
    const data = chunk => {
      const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
      if (bytes.length + buffer.length > 512) return finish(new Error('server_stop_input'));
      bytes = Buffer.concat([bytes, buffer]);
      if (bytes.includes(10)) {
        if (!/^stop\r?\n$/.test(bytes.toString('utf8'))) finish(new Error('server_stop_input'));
        else finish();
      }
    };
    const end = () => finish(bytes.length ? new Error('server_stop_input') : undefined);
    const errorEvent = () => finish(new Error('server_stop_input'));
    const abort = () => finish(new Error('server_cancelled'));
    input.on('data', data); input.once('end', end); input.once('error', errorEvent);
    signal.addEventListener('abort', abort, { once: true });
    if (signal.aborted) abort(); else if (input.readableEnded) end(); else input.resume();
  });
}

export async function runServer(javaHome, basename, approval, acceptedEula, { signal, onReady, profile = 'status', guiNames } = {}) {
  if (acceptedEula !== true) fail('server_eula_acceptance_required');
  if (typeof approval !== 'string' || !digestPattern.test(approval)) fail('server_run_approval');
  const playerNames = profile === 'gui' ? validateGuiNames(guiNames) : guiNames;
  const plan = await makeRunPlan(javaHome, basename, profile, playerNames);
  if (approval !== runPlanDigest(plan)) fail('server_run_approval');
  signal?.throwIfAborted();
  const root = await preparedDirectory(basename);
  // Windows PowerShell can create AppData while importing its system modules.
  // Keep its profile/temp in a fresh owned sibling, not in the exact stage whose
  // file set must remain unchanged until the EULA transaction begins.
  await rejectLinkedParents(PARENT);
  const probeHome = await mkdtemp(join(PARENT, 'probe-'));
  if ((await listeners(root, probeHome, signal)).length !== 0) fail('server_port_in_use');
  if (['svc', 'plasmo'].includes(profile) && (await listeners(root, probeHome, signal, profile)).length !== 0) fail('server_voice_port_in_use');
  // Rebuild after the preflight OS query, before marker/snapshot/EULA mutation.
  if (runPlanDigest(await makeRunPlan(javaHome, basename, profile, playerNames)) !== approval) fail('server_run_plan_changed');
  signal?.throwIfAborted();
  let readinessFailure; let bindObservation;
  const transaction = await withPreparedEula(basename, approval, async ownedRoot => {
    const temp = join(ownedRoot, '.connect-run-temp'); await mkdir(temp);
    return withOwnedProcess({ executable: plan.launch.executable, args: plan.launch.args, cwd: ownedRoot,
      env: minimalEnvironment(temp), signal, timeoutMs: plan.limits.lifetimeSeconds * 1000, stopTimeoutMs: 5000, outputLimitBytes: 4 * 1024 * 1024 },
    async ({ signal: childSignal, ownsProcessId }) => {
      const evidence = await ready(ownedRoot, probeHome, childSignal, ownsProcessId, profile === 'status-interactive' ? 'status' : profile).catch(error => {
        if (['server_probe_failed', 'server_probe_deadline', 'server_probe_output_limit', 'server_bind_probe_invalid', 'server_bind_not_owned',
          'server_status_invalid', 'server_readiness_deadline'].includes(error?.message)) readinessFailure = error.message;
        if (error?.message === 'server_bind_not_owned') bindObservation = error.bindObservation;
        throw error;
      });
      if (onReady) await onReady({ signal: childSignal, evidence, runPlanSha256: approval });
      return evidence;
    });
  }).catch(error => {
    if (readinessFailure) error.readinessFailure = readinessFailure;
    if (bindObservation) error.bindObservation = bindObservation;
    throw error;
  });
  let voiceConfig;
  if (profile === 'svc') {
    // Read the completed writer output only after the owned JVM is closed and
    // its temporary EULA transaction is restored. Never publish raw config.
    voiceConfig = validateSvcConfig(await readLocal(join(root, SVC_CONFIG_PATH), 256 * 1024));
    if (voiceConfig.sha256 === hash(SVC_BASELINE_BYTES) || voiceConfig.keyCount <= 3) fail('runtime_svc_writer_not_observed');
  }
  if (profile === 'plasmo') {
    voiceConfig = validatePlasmoConfig(await readLocal(join(root, PLASMO_CONFIG_PATH), 256 * 1024));
    if (voiceConfig.sha256 === hash(PLASMO_BASELINE_BYTES) || voiceConfig.keyCount <= 5) fail('runtime_plasmo_writer_not_observed');
  }
  return { event: 'server_stopped', runPlanSha256: approval, evidence: transaction.value.value,
    process: transaction.value.process, eulaRestored: transaction.eulaRestored, reusePermitted: false,
    runtimeHomologated: false, worldJoined: false,
    voice: ['svc', 'plasmo'].includes(profile) ? `${profile}_boot_and_writer_only` : 'not_tested',
    ...(voiceConfig ? { voiceConfig, voiceTransport: 'not_tested', voiceAudio: 'not_tested' } : {}), accountAuthentication: 'not_tested' };
}

export async function main(args, { input = process.stdin, emit = value => process.stdout.write(json(value)), signal } = {}) {
  if (args[0] === 'run-plan-gui' && [4, 5].includes(args.length)) {
    const plan = await makeRunPlan(args[1], args[2], 'gui', validateGuiNames(args.slice(3)));
    return { event: 'server_run_plan', runPlanSha256: runPlanDigest(plan), plan };
  }
  if (args[0] === 'run-gui') {
    if (args.at(-1) !== '--user-accepted-minecraft-eula') fail('server_eula_acceptance_required');
    if (![6, 7].includes(args.length)) fail('server_usage');
    return runServer(args[1], args[2], args[3], true, { signal, profile: 'gui', guiNames: validateGuiNames(args.slice(4, -1)),
      onReady: async ({ signal: childSignal, evidence, runPlanSha256 }) => {
        emit({ event: 'server_ready', runPlanSha256, evidence, runtimeHomologated: false, worldJoined: false, voice: 'not_tested' });
        await waitForStop(input, childSignal);
      } });
  }
  if (args.length === 3 && ['run-plan', 'run-plan-play', 'run-plan-svc', 'run-plan-plasmo', 'run-plan-status-interactive'].includes(args[0])) {
    const plan = await makeRunPlan(args[1], args[2], args[0] === 'run-plan' ? 'status' : args[0].slice(9));
    return { event: 'server_run_plan', runPlanSha256: runPlanDigest(plan), plan };
  }
  if (args.length === 5 && ['run', 'run-play', 'run-svc', 'run-plasmo', 'run-status-interactive'].includes(args[0]) && args[4] === '--user-accepted-minecraft-eula') {
    return runServer(args[1], args[2], args[3], true, { signal, profile: args[0] === 'run' ? 'status' : args[0].slice(4),
      onReady: async ({ signal: childSignal, evidence, runPlanSha256 }) => {
        emit({ event: 'server_ready', runPlanSha256, evidence, runtimeHomologated: false, worldJoined: false, voice: 'not_tested' });
        await waitForStop(input, childSignal);
      } });
  }
  if (['run', 'run-play', 'run-svc', 'run-plasmo', 'run-status-interactive'].includes(args[0])) fail('server_eula_acceptance_required');
  fail('server_usage');
}

const processFields = ['spawned', 'closed', 'exitCode', 'signal', 'forced', 'stdoutBytes', 'stderrBytes'];
const processSignals = new Set(Object.keys(osConstants.signals));
function permittedProcessSummary(error) {
  if (!(error instanceof OwnedProcessError)) return undefined;
  const source = Object.getOwnPropertyDescriptor(error, 'process')?.value;
  if (!source || typeof source !== 'object' || Array.isArray(source)) return undefined;
  const fields = Object.getOwnPropertyDescriptors(source);
  const summary = Object.fromEntries(processFields.map(key => [key, fields[key]?.value]));
  if (['spawned', 'closed', 'forced'].some(key => typeof summary[key] !== 'boolean')
    || !(summary.exitCode === null || (Number.isSafeInteger(summary.exitCode)
      && summary.exitCode >= -2147483648 && summary.exitCode <= 4294967295))
    || !(summary.signal === null || processSignals.has(summary.signal))
    || ['stdoutBytes', 'stderrBytes'].some(key => !Number.isSafeInteger(summary[key]) || summary[key] < 0)) return undefined;
  return summary;
}

export function serverFailureReport(error) {
  const code = /^(server|owned_process|runtime)_[a-z_]+$/.test(error?.message) ? error.message : 'server_io_or_process_failed';
  const processSummary = permittedProcessSummary(error);
  return { event: 'failed', code, readinessFailure: error?.readinessFailure,
    bindObservation: error?.bindObservation, ...(processSummary ? { process: processSummary } : {}) };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const controller = new AbortController();
  const interrupt = () => controller.abort(); process.on('SIGINT', interrupt); process.on('SIGTERM', interrupt);
  try { process.stdout.write(json(await main(process.argv.slice(2), { signal: controller.signal }))); }
  catch (error) {
    process.stdout.write(json(serverFailureReport(error))); process.exitCode = 1;
  } finally { process.off('SIGINT', interrupt); process.off('SIGTERM', interrupt); }
}
