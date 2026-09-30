import { mkdir, mkdtemp, lstat } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { rejectLinkedParents } from '../connect-voice-tools/local-path.mjs';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const controller = new AbortController(); const interrupt = () => controller.abort();
process.on('SIGINT', interrupt); process.on('SIGTERM', interrupt);
let clientFailure;
try {
  if (process.argv[2] !== '--offline-lab' || ![3, 4].includes(process.argv.length)
    || (process.argv.length === 4 && process.argv[3] !== '--keepalive')) throw new Error('client_svc_usage');
  const keepalive = process.argv.length === 4;
  if (process.platform !== 'win32' || process.env.NODE_OPTIONS || process.env.DEBUG || globalThis.debugNMP) throw new Error('client_svc_environment');
  const root = join(repo, '.aurora/connect-validation'); await rejectLinkedParents(root);
  await mkdir(root, { recursive: true });
  if (!(await lstat(root)).isDirectory()) throw new Error('client_svc_directory');
  const temp = await mkdtemp(join(root, 'svc-client-'));
  const systemRoot = process.env.SystemRoot;
  if (!/^[a-zA-Z]:[\\/]Windows$/i.test(systemRoot ?? '')) throw new Error('client_svc_environment');
  for (const key of Object.keys(process.env)) delete process.env[key];
  Object.assign(process.env, { SystemRoot: systemRoot, windir: systemRoot, TEMP: temp, TMP: temp, USERPROFILE: temp });
  const { withSvcTunnel } = await import('./tunnel-session.mjs');
  const { observeSvcPlayer } = await import('./svc-play.mjs');
  const result = await withSvcTunnel(async ({ bridgePort, voicePort, signal }) => {
    try { return await observeSvcPlayer(bridgePort, voicePort, { signal, keepalive }); }
    catch (error) { clientFailure = error; throw error; }
  }, { signal: controller.signal, timeoutMs: 90000 });
  process.stdout.write(`${JSON.stringify({ event: 'svc_control_probe', result: 'PASS', ...result.value,
    agents: 2, independentHostSessions: 1, tunnelCleanup: result.cleanup, topology: 'same_machine_loopback',
    voiceAudio: 'not_tested', relay: false }, null, 2)}\n`);
} catch (error) {
  const reported = clientFailure ?? error;
  const code = /^(client|svc_control|game_tunnel)_[a-z_]+$/.test(reported?.message ?? '') ? reported.message : 'client_svc_failed';
  process.stdout.write(`${JSON.stringify({ event: 'svc_control_probe', result: 'FAIL', code,
    clientCleanup: clientFailure?.cleanup, tunnelCleanup: error.cleanup })}\n`); process.exitCode = 1;
} finally { controller.abort(); process.off('SIGINT', interrupt); process.off('SIGTERM', interrupt); }
