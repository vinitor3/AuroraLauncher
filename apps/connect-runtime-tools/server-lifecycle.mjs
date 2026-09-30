// Shared only by the two fixed disposable-lab callers. No desktop command.
import { execFile } from 'node:child_process';
import { lstat, open, writeFile, realpath } from 'node:fs/promises';
import { dirname, join, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { EULA_URL } from './pins.mjs';
import { hash, readLocal } from './files.mjs';
import { rejectLinkedParents, resolveExplicitLocalPath } from '../connect-voice-tools/local-path.mjs';
const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const PARENT = join(REPO, '.aurora', 'connect-runtime');
const CONNECT = join(REPO, 'apps/connect-lab/target/debug/aurora-connect-lab.exe');
const FALSE_EULA = Buffer.from('# '+EULA_URL+'\neula=false\n');
const TRUE_EULA = Buffer.from('# '+EULA_URL+'\neula=true\n');
const json = value => JSON.stringify(value, null, 2)+'\n';
const fail = code => { throw new Error(code); };
const same = (bytes, expected, code) => { if (!bytes.equals(Buffer.from(expected))) fail(code); };
const digestPattern = /^[a-f0-9]{64}$/;

export async function requireLabRoot(root) {
  if (typeof root !== 'string' || resolveExplicitLocalPath(root) !== root) fail('server_lab_root');
  const rel = relative(PARENT, root).replaceAll('\\', '/');
  if (!/^prepared-[A-Za-z0-9]{6}$/.test(rel)
    && !/^voice-roundtrip-[A-Za-z0-9]{6}\/native\/instances\/aurora-test-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(rel)) fail('server_lab_root');
  await rejectLinkedParents(root);
  const metadata = await lstat(root);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) fail('server_unsafe_directory');
}

function powershellPath() {
  const systemRoot = process.env.SystemRoot;
  if (process.platform !== 'win32' || typeof systemRoot !== 'string'
    || !/^[a-zA-Z]:[\\/]Windows$/i.test(systemRoot)) fail('server_windows_system_directory');
  return resolveExplicitLocalPath(join(systemRoot, 'System32/WindowsPowerShell/v1.0/powershell.exe'));
}
export function minimalEnvironment(temp) {
  return { SystemRoot: process.env.SystemRoot, windir: process.env.SystemRoot, TEMP: temp, TMP: temp, USERPROFILE: temp };
}
export async function toolHashes() {
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

// Narrow transaction for a fresh confined stage or verified native lab copy.
// Both callers perform their complete fixed-baseline verification first.
// Unit tests use
// synthetic EULA bytes and a Node operation, never actual Minecraft consent.
export async function withLabEula(root, approval, operation) {
  if (typeof approval !== 'string' || !digestPattern.test(approval) || typeof operation !== 'function') fail('server_run_approval');
  await requireLabRoot(root);
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
export async function listeners(root, probeHome, signal, voice) {
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
export async function ready(root, probeHome, signal, ownsProcessId, profile) {
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
