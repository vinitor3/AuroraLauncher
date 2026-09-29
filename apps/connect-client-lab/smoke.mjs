import { execFile } from 'node:child_process';
import { mkdir, mkdtemp, lstat } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { rejectLinkedParents } from '../connect-voice-tools/local-path.mjs';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const binary = join(repo, 'apps/connect-lab/target/debug/aurora-connect-lab.exe');
const controller = new AbortController();
let clientFailure;
const failedTunnelCleanup = [];
const interrupt = () => controller.abort();
process.on('SIGINT', interrupt); process.on('SIGTERM', interrupt);

function status(port, signal) {
  return new Promise((resolveStatus, reject) => {
    execFile(binary, ['minecraft-status', `127.0.0.1:${port}`, 'localhost', '43001', '767'],
      { cwd: repo, env: process.env, windowsHide: true, shell: false, timeout: 6000, maxBuffer: 16384, signal },
      (error, stdout) => {
        if (error) return reject(new Error('client_status_probe_failed'));
        try {
          const value = JSON.parse(stdout);
          if (value.event !== 'minecraft_status_received' || value.summary?.reportedProtocol !== 767
            || value.summary.protocolMatchesRequested !== true) throw new Error();
          resolveStatus(value.summary);
        } catch { reject(new Error('client_status_probe_failed')); }
      });
  });
}

try {
  if (process.argv.length !== 3 || process.argv[2] !== '--offline-lab') throw new Error('client_smoke_usage');
  if (process.platform !== 'win32' || process.env.NODE_OPTIONS || process.env.DEBUG) throw new Error('client_smoke_environment');
  const validation = join(repo, '.aurora/connect-validation');
  await rejectLinkedParents(validation);
  await mkdir(validation, { recursive: true });
  if (!(await lstat(validation)).isDirectory()) throw new Error('client_smoke_directory');
  const temp = await mkdtemp(join(validation, 'client-run-'));
  const systemRoot = process.env.SystemRoot;
  if (!/^[a-zA-Z]:[\\/]Windows$/i.test(systemRoot ?? '')) throw new Error('client_smoke_environment');
  // Only this Node process is changed, before loading protocol/auth modules.
  // Parent/user environment and account caches are never mutated.
  for (const key of Object.keys(process.env)) delete process.env[key];
  Object.assign(process.env, { SystemRoot: systemRoot, windir: systemRoot, TEMP: temp, TMP: temp, USERPROFILE: temp });
  const { withGameTunnel, GameTunnelError } = await import('./tunnel-session.mjs');
  const trackedTunnel = async (operation, options) => {
    try { return await withGameTunnel(operation, options); }
    catch (error) { if (error instanceof GameTunnelError) failedTunnelCleanup.push(error.cleanup); throw error; }
  };
  const { observeTwoPlayers } = await import('./play.mjs');
  let onlineDuring; let onlineAfter;
  const first = await trackedTunnel(async a => {
    const second = await trackedTunnel(async b => {
      const signal = AbortSignal.any([a.signal, b.signal, controller.signal]);
      const evidence = await observeTwoPlayers([a.bridgePort, b.bridgePort], { signal,
        onReady: async () => {
          onlineDuring = await status(a.bridgePort, signal);
          if (onlineDuring.reportedPlayersOnline !== 2) throw new Error('client_online_count_mismatch');
        } }).catch(error => { clientFailure = error; throw error; });
      for (let attempt = 0; attempt < 10; attempt++) {
        onlineAfter = await status(a.bridgePort, signal);
        if (onlineAfter.reportedPlayersOnline === 0) break;
        await delay(100, undefined, { signal });
      }
      if (onlineAfter.reportedPlayersOnline !== 0) throw new Error('client_offline_count_mismatch');
      return evidence;
    }, { signal: a.signal, timeoutMs: 120000 });
    return { evidence: second.value, secondCleanup: second.cleanup };
  }, { signal: controller.signal, timeoutMs: 120000 });
  process.stdout.write(`${JSON.stringify({ result: 'PASS', ...first.value.evidence,
    agents: 4, independentHostSessions: 2, playersReportedDuring: onlineDuring.reportedPlayersOnline,
    playersReportedAfter: onlineAfter.reportedPlayersOnline,
    tunnelCleanup: [first.cleanup, first.value.secondCleanup], topology: 'same_machine_loopback', relay: false,
    endpointLifecycle: 'caller_owned' }, null, 2)}\n`);
} catch (error) {
  const reported = clientFailure ?? error;
  const code = /^(client|game_tunnel)_[a-z_]+$/.test(reported?.message ?? '') ? reported.message : 'client_smoke_failed';
  process.stdout.write(`${JSON.stringify({ result: 'FAIL', code, observation: clientFailure?.observation,
    cleanup: clientFailure?.cleanup, tunnelCleanup: failedTunnelCleanup })}\n`); process.exitCode = 1;
} finally {
  controller.abort(); process.off('SIGINT', interrupt); process.off('SIGTERM', interrupt);
}
