import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { lstat, readdir, open, writeFile, mkdir, mkdtemp, realpath } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { EULA_URL } from './pins.mjs';
import { runtimeFiles, runtimeArtifacts } from './play-profile.mjs';
import { SVC_ARTIFACT, SVC_CONFIG_PATH, SVC_BASELINE_BYTES, validateSvcConfig } from './svc-profile.mjs';
import { PLASMO_ARTIFACT, PLASMO_CONFIG_PATH, PLASMO_BASELINE_BYTES, validatePlasmoConfig } from './plasmo-profile.mjs';
import { makePlan, planDigest } from './runtime.mjs';
import { absent, hash, readLocal } from './files.mjs';
import { withOwnedProcess } from './owned-process.mjs';
import { rejectLinkedParents, resolveExplicitLocalPath } from '../connect-voice-tools/local-path.mjs';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const PARENT = join(REPO, '.aurora', 'connect-runtime');
const CONNECT = join(REPO, 'apps/connect-lab/target/debug/aurora-connect-lab.exe');
const FALSE_EULA = Buffer.from(`# ${EULA_URL}\neula=false\n`);
const TRUE_EULA = Buffer.from(`# ${EULA_URL}\neula=true\n`);
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
export async function verifyPreparedFiles(basename, baseline, profile = 'status') {
  const root = await preparedDirectory(basename);
  await absent(join(root, 'run-used.json'));
  const files = baseline?.files;
  const fixedFiles = runtimeFiles(profile);
  if (!files || Object.keys(files).sort().join(',') !== Object.keys(fixedFiles).sort().join(',')
    || typeof files['server.properties'] !== 'string' || files['eula.txt'] !== FALSE_EULA.toString()
    || Object.values(files).some(value => typeof value !== 'string')) fail('server_baseline');
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

function powershellPath() {
  const systemRoot = process.env.SystemRoot;
  if (process.platform !== 'win32' || typeof systemRoot !== 'string'
    || !/^[a-zA-Z]:[\\/]Windows$/i.test(systemRoot)) fail('server_windows_system_directory');
  return resolveExplicitLocalPath(join(systemRoot, 'System32/WindowsPowerShell/v1.0/powershell.exe'));
}
function minimalEnvironment(temp) {
  return { SystemRoot: process.env.SystemRoot, windir: process.env.SystemRoot, TEMP: temp, TMP: temp, USERPROFILE: temp };
}
async function toolHashes() {
  const powershell = powershellPath();
  return { connectSha256: await hashReadonlyTool(CONNECT, 128 * 1024 * 1024),
    powershellSha256: await hashReadonlyTool(powershell, 4 * 1024 * 1024),
    readOnlyBuildAndSystemHardlinksAllowed: true, hashesProveSignatureOrTrust: false };
}

// The two fixed read-only executables commonly have legitimate Cargo/WinSxS
// hardlinks. This exception never applies to artifacts, configs or the EULA.
// Hashes identify reviewed bytes; they do not establish signing or trust.
async function hashReadonlyTool(file, maximum) {
  if (file !== CONNECT && file !== powershellPath()) fail('server_tool_path');
  await rejectLinkedParents(file);
  const before = await lstat(file);
  if (!before.isFile() || before.isSymbolicLink() || before.size < 2 || before.size > maximum
    || (await realpath(file)).toLowerCase() !== file.toLowerCase()) fail('server_tool_file');
  const handle = await open(file, 'r');
  try {
    const opened = await handle.stat();
    if (opened.ino !== before.ino || opened.dev !== before.dev || opened.size !== before.size) fail('server_tool_changed');
    const bytes = Buffer.alloc(before.size + 1); let count = 0;
    while (count < bytes.length) {
      const { bytesRead } = await handle.read(bytes, count, bytes.length - count, null);
      if (!bytesRead) break;
      count += bytesRead;
    }
    const after = await handle.stat(); const currentPath = await lstat(file);
    if (count !== before.size || after.size !== before.size || after.mtimeMs !== before.mtimeMs
      || after.ctimeMs !== before.ctimeMs || after.nlink !== before.nlink || currentPath.isSymbolicLink()
      || currentPath.ino !== before.ino || currentPath.dev !== before.dev) fail('server_tool_changed');
    if (bytes[0] !== 0x4d || bytes[1] !== 0x5a) fail('server_tool_file');
    return hash(bytes.subarray(0, count));
  } finally { await handle.close(); }
}
export const runPlanDigest = plan => hash(json(plan));
export async function makeRunPlan(javaHome, basename, profile = 'status') {
  const baseline = await makePlan(javaHome, profile);
  const stage = await verifyPreparedFiles(basename, baseline, profile);
  const tools = await toolHashes();
  return {
    schemaVersion: 1, purpose: 'aurora_connect_disposable_server_execution',
    preparedBasename: basename, baselinePlanSha256: stage.baselineHash, java: baseline.java,
    artifacts: stage.verified, tools,
    probeEnvironment: 'fresh_sibling_probe_directory_not_prepared_stage',
    diff: { file: 'eula.txt', beforeSha256: hash(FALSE_EULA), afterSha256: hash(TRUE_EULA),
      before: 'eula=false', after: 'eula=true', restore: 'exact_applied_bytes_only' },
    propertiesSha256: hash(baseline.files['server.properties']),
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
    limits: { lifetimeSeconds: 300, readinessSeconds: 180, bindProbeSeconds: 10, statusProbeSeconds: 6,
      stopSeconds: 5, outputBytes: 4 * 1024 * 1024,
      reuse: false, personalInstanceAccess: false, relay: false, mods: ['svc', 'plasmo'].includes(profile) },
  };
}

async function replaceExact(file, before, after) {
  same(await readLocal(file, 4096), before, 'server_eula_recovery_conflict');
  const handle = await open(file, 'r+');
  try {
    const metadata = await handle.stat();
    if (!metadata.isFile() || metadata.nlink !== 1 || metadata.size !== before.length) fail('server_eula_recovery_conflict');
    const current = Buffer.alloc(before.length + 1);
    let read = 0;
    while (read < current.length) {
      const { bytesRead } = await handle.read(current, read, current.length - read, read);
      if (!bytesRead) break;
      read += bytesRead;
    }
    same(current.subarray(0, read), before, 'server_eula_recovery_conflict');
    let offset = 0;
    while (offset < after.length) {
      const written = await handle.write(after, offset, after.length - offset, offset);
      if (!written.bytesWritten) fail('server_eula_write_failed');
      offset += written.bytesWritten;
    }
    await handle.truncate(after.length); await handle.sync();
  } finally { await handle.close(); }
  same(await readLocal(file, 4096), after, 'server_eula_recovery_conflict');
}

// Narrow transaction for a fresh confined prepared-* child only. Unit tests use
// synthetic EULA bytes and a Node operation, never actual Minecraft consent.
// The executor below performs complete fixed-baseline verification first.
export async function withPreparedEula(basename, approval, operation) {
  if (typeof approval !== 'string' || !digestPattern.test(approval) || typeof operation !== 'function') fail('server_run_approval');
  const root = await preparedDirectory(basename);
  const eula = join(root, 'eula.txt');
  same(await readLocal(eula, 4096), FALSE_EULA, 'server_eula_changed');
  const used = join(root, 'run-used.json');
  await writeFile(used, json({ runPlanSha256: approval, reusePermitted: false,
    beforeSha256: hash(FALSE_EULA), appliedSha256: hash(TRUE_EULA) }), { flag: 'wx' });
  let applyAttempted = false;
  let value;
  let operationError;
  let restored = false;
  try {
    await writeFile(join(root, 'run-eula-before.txt'), FALSE_EULA, { flag: 'wx' });
    applyAttempted = true;
    await replaceExact(eula, FALSE_EULA, TRUE_EULA);
    value = await operation(root);
  } catch (error) { operationError = error; }
  finally {
    if (applyAttempted) {
      try { await replaceExact(eula, TRUE_EULA, FALSE_EULA); restored = true; }
      catch {
        await writeFile(join(root, 'run-recovery.json'), json({ code: 'server_eula_recovery_conflict',
          snapshot: 'run-eula-before.txt', overwritePermitted: false }), { flag: 'wx' }).catch(() => {});
        fail('server_eula_recovery_conflict');
      }
    }
  }
  if (operationError) throw operationError;
  return { value, eulaRestored: restored };
}

// Queries only listening sockets on this one test port. No CommandLine, process
// name search, mutation or PID-based termination. Raw addresses/PIDs stay local.
const BIND_QUERY = "$ErrorActionPreference='Stop';try{$a=@(Get-NetTCPConnection -State Listen -LocalPort 43001 -ErrorAction Stop)}catch{if($_.CategoryInfo.Category -eq 'ObjectNotFound'){$a=@()}else{exit 3}};ConvertTo-Json -InputObject @($a|Select-Object LocalAddress,OwningProcess) -Compress";
const VOICE_BIND_QUERY = "$ErrorActionPreference='Stop';try{$a=@(Get-NetUDPEndpoint -LocalPort 43002 -ErrorAction Stop)}catch{if($_.CategoryInfo.Category -eq 'ObjectNotFound'){$a=@()}else{exit 3}};ConvertTo-Json -InputObject @($a|Select-Object LocalAddress,OwningProcess) -Compress";
const PLASMO_BIND_QUERY = "$ErrorActionPreference='Stop';try{$a=@(Get-NetUDPEndpoint -LocalPort 43003 -ErrorAction Stop)}catch{if($_.CategoryInfo.Category -eq 'ObjectNotFound'){$a=@()}else{exit 3}};ConvertTo-Json -InputObject @($a|Select-Object LocalAddress,OwningProcess) -Compress";
async function finiteCommand(executable, args, { cwd, env, signal, timeout = 5000 }) {
  return new Promise((resolvePromise, reject) => {
    execFile(executable, args, { cwd, env, windowsHide: true, shell: false, timeout, maxBuffer: 16384, signal },
      (error, stdout) => {
        if (error) reject(new Error(signal?.aborted ? 'server_cancelled'
          : error.code === 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER' ? 'server_probe_output_limit'
          : error.killed ? 'server_probe_deadline' : 'server_probe_failed'));
        else resolvePromise(stdout);
      });
  });
}
async function listeners(root, probeHome, signal, voice) {
  // Only compiled profile identifiers select compiled queries; no port, path or
  // user-controlled string is interpolated into PowerShell source.
  const query = voice === 'svc' ? VOICE_BIND_QUERY : voice === 'plasmo' ? PLASMO_BIND_QUERY : BIND_QUERY;
  const output = await finiteCommand(powershellPath(), ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', query],
    { cwd: root, env: minimalEnvironment(probeHome), signal, timeout: 10000 });
  let result;
  try { result = JSON.parse(output); } catch { fail('server_bind_probe_invalid'); }
  if (!Array.isArray(result) || result.length > 16 || result.some(row => !row || typeof row !== 'object'
    || Object.keys(row).sort().join(',') !== 'LocalAddress,OwningProcess'
    || typeof row.LocalAddress !== 'string' || !Number.isSafeInteger(row.OwningProcess) || row.OwningProcess < 1)) fail('server_bind_probe_invalid');
  return result;
}
export function requireOwnedBind(rows, ownsProcessId, { allowIpv4MappedLoopback = false } = {}) {
  // Windows reports Java's IPv4 UDP bind as an IPv4-mapped IPv6 address.
  // Accept only this exact equivalent loopback form when explicitly selected;
  // never arbitrary mapped addresses, ::1, wildcard or an unowned process.
  const address = rows?.[0]?.LocalAddress;
  const loopback = address === '127.0.0.1' || (allowIpv4MappedLoopback === true && address === '::ffff:127.0.0.1');
  if (!Array.isArray(rows) || rows.length !== 1 || !loopback
    || !Number.isSafeInteger(rows[0]?.OwningProcess) || rows[0].OwningProcess < 1
    || typeof ownsProcessId !== 'function' || !ownsProcessId(rows[0].OwningProcess)) {
    const observed = Array.isArray(rows) ? rows : [];
    throw Object.assign(new Error('server_bind_not_owned'), { bindObservation: {
      rows: observed.length,
      allOwned: observed.length > 0 && typeof ownsProcessId === 'function'
        && observed.every(row => Number.isSafeInteger(row?.OwningProcess) && ownsProcessId(row.OwningProcess)),
      ipv4Loopback: observed.filter(row => row?.LocalAddress === '127.0.0.1').length,
      ipv4MappedLoopback: observed.filter(row => row?.LocalAddress === '::ffff:127.0.0.1').length,
      wildcard: observed.filter(row => ['0.0.0.0', '::'].includes(row?.LocalAddress)).length,
    } });
  }
}
async function ready(root, probeHome, signal, ownsProcessId, profile) {
  const deadline = AbortSignal.timeout(180000);
  const parentSignal = signal;
  signal = AbortSignal.any([parentSignal, deadline]);
  try {
    while (true) {
      signal.throwIfAborted();
      const rows = await listeners(root, probeHome, signal);
      if (rows.length) {
        requireOwnedBind(rows, ownsProcessId);
        let output;
        try { output = await finiteCommand(CONNECT, ['minecraft-status', '127.0.0.1:43001', 'localhost', '43001', '767'],
          { cwd: root, env: minimalEnvironment(probeHome), signal, timeout: 6000 }); }
        catch { signal.throwIfAborted(); await delay(250, undefined, { signal }); continue; }
        let status;
        try { status = JSON.parse(output); } catch { fail('server_status_invalid'); }
        if (status.event !== 'minecraft_status_received' || status.summary?.reportedProtocol !== 767
          || status.summary?.protocolMatchesRequested !== true) fail('server_status_invalid');
        requireOwnedBind(await listeners(root, probeHome, signal), ownsProcessId);
        if (['svc', 'plasmo'].includes(profile)) {
          const voiceRows = await listeners(root, probeHome, signal, profile);
          if (!voiceRows.length) { await delay(250, undefined, { signal }); continue; }
          requireOwnedBind(voiceRows, ownsProcessId, { allowIpv4MappedLoopback: true });
        }
        signal.throwIfAborted();
        return { protocol: 767, statusAndMatchingPong: true, ownLoopbackBind: true,
          ...(profile === 'svc' ? { svcOwnUdpLoopbackBind: true, voicePort: 43002,
            voiceAuthentication: 'not_tested', voiceAudio: 'not_tested' } : {}),
          ...(profile === 'plasmo' ? { plasmoOwnUdpLoopbackBind: true, voicePort: 43003,
            voiceAuthentication: 'not_tested', voiceAudio: 'not_tested' } : {}) };
      }
      await delay(250, undefined, { signal });
    }
  } catch (error) {
    if (deadline.aborted && !parentSignal.aborted) fail('server_readiness_deadline');
    throw error;
  }
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

export async function runServer(javaHome, basename, approval, acceptedEula, { signal, onReady, profile = 'status' } = {}) {
  if (acceptedEula !== true) fail('server_eula_acceptance_required');
  if (typeof approval !== 'string' || !digestPattern.test(approval)) fail('server_run_approval');
  const plan = await makeRunPlan(javaHome, basename, profile);
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
  if (runPlanDigest(await makeRunPlan(javaHome, basename, profile)) !== approval) fail('server_run_plan_changed');
  signal?.throwIfAborted();
  let readinessFailure; let bindObservation;
  const transaction = await withPreparedEula(basename, approval, async ownedRoot => {
    const temp = join(ownedRoot, '.connect-run-temp'); await mkdir(temp);
    return withOwnedProcess({ executable: plan.launch.executable, args: plan.launch.args, cwd: ownedRoot,
      env: minimalEnvironment(temp), signal, timeoutMs: 300000, stopTimeoutMs: 5000, outputLimitBytes: 4 * 1024 * 1024 },
    async ({ signal: childSignal, ownsProcessId }) => {
      const evidence = await ready(ownedRoot, probeHome, childSignal, ownsProcessId, profile).catch(error => {
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
  if (args.length === 3 && ['run-plan', 'run-plan-play', 'run-plan-svc', 'run-plan-plasmo'].includes(args[0])) {
    const plan = await makeRunPlan(args[1], args[2], args[0] === 'run-plan' ? 'status' : args[0].slice(9));
    return { event: 'server_run_plan', runPlanSha256: runPlanDigest(plan), plan };
  }
  if (args.length === 5 && ['run', 'run-play', 'run-svc', 'run-plasmo'].includes(args[0]) && args[4] === '--user-accepted-minecraft-eula') {
    return runServer(args[1], args[2], args[3], true, { signal, profile: args[0] === 'run' ? 'status' : args[0].slice(4),
      onReady: async ({ signal: childSignal, evidence, runPlanSha256 }) => {
        emit({ event: 'server_ready', runPlanSha256, evidence, runtimeHomologated: false, worldJoined: false, voice: 'not_tested' });
        await waitForStop(input, childSignal);
      } });
  }
  if (['run', 'run-play', 'run-svc', 'run-plasmo'].includes(args[0])) fail('server_eula_acceptance_required');
  fail('server_usage');
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const controller = new AbortController();
  const interrupt = () => controller.abort(); process.on('SIGINT', interrupt); process.on('SIGTERM', interrupt);
  try { process.stdout.write(json(await main(process.argv.slice(2), { signal: controller.signal }))); }
  catch (error) {
    const code = /^(server|owned_process|runtime)_[a-z_]+$/.test(error.message) ? error.message : 'server_io_or_process_failed';
    process.stdout.write(json({ event: 'failed', code, readinessFailure: error.readinessFailure,
      bindObservation: error.bindObservation })); process.exitCode = 1;
  } finally { process.off('SIGINT', interrupt); process.off('SIGTERM', interrupt); }
}
