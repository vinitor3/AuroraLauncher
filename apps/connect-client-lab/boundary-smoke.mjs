// Explicit local byte fixture, never a Minecraft/mod/audio or external MTU test.
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createSocket } from 'node:dgram';
import { lstat, mkdir, mkdtemp, open, realpath, writeFile } from 'node:fs/promises';
import { createConnection, createServer } from 'node:net';
import { dirname, join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
import { rejectLinkedParents, resolveExplicitLocalPath } from '../connect-voice-tools/local-path.mjs';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const PARENT = join(REPO, '.aurora', 'connect-validation');
const EXECUTABLE = join(REPO, 'apps/connect-lab/target/debug/aurora-connect-lab.exe');
const EXECUTABLE_SHA256 = '7f8c7ed40e00826f58b5f482f80d1660941d5568d36a2983588488c84d0b9a40';
const CAP = 1264;
const TCP_BYTES = 1_048_576;
const EVENT_BYTES = 8192;
const OUTPUT_BYTES = 32768;
const SILENCE_MS = 300;
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };

export class BoundarySmokeError extends Error {
  constructor(code, cleanup) { super(code); this.name = 'BoundarySmokeError'; this.code = code; if (cleanup) this.cleanup = Object.freeze(cleanup); }
}
const invalid = code => { throw new BoundarySmokeError(code); };

// Exported for adversarial framing tests; no I/O or process authorization here.
export function decodeBoundaryEvent(bytes) {
  if (!Buffer.isBuffer(bytes) || !bytes.length || bytes.length > EVENT_BYTES) invalid('boundary_event_invalid');
  let value;
  try { value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)); }
  catch { invalid('boundary_event_invalid'); }
  if (!value || typeof value !== 'object' || Array.isArray(value)) invalid('boundary_event_invalid');
  return value;
}
const portOf = address => {
  const match = typeof address === 'string' && /^127\.0\.0\.1:([1-9][0-9]{0,4})$/.exec(address);
  if (!match || Number(match[1]) > 65535) invalid('boundary_bridge_invalid');
  return Number(match[1]);
};
export function validateBoundaryReady(value) {
  const tcp = portOf(value?.tcp); const udp = portOf(value?.udp);
  if (!isDeepStrictEqual(value, { event: 'guest_ready', tcp: value.tcp, udp: value.udp,
    maxDatagramPayload: CAP, experimental: true, next: 'enter stop to close' })) invalid('boundary_bridge_invalid');
  return { tcp, udp };
}

async function verifiedBytes(path, maximum, allowHardlinks = false) {
  resolveExplicitLocalPath(path); await rejectLinkedParents(path);
  const before = await lstat(path);
  if (!before.isFile() || before.isSymbolicLink() || (!allowHardlinks && before.nlink !== 1)
    || before.size > maximum || (await realpath(path)).toLowerCase() !== path.toLowerCase()) invalid('boundary_unsafe_file');
  const file = await open(path, 'r');
  try {
    const opened = await file.stat();
    if (opened.ino !== before.ino || opened.dev !== before.dev || opened.size !== before.size) invalid('boundary_file_changed');
    const hash = createHash('sha256'); const buffer = Buffer.alloc(65536); let count = 0;
    for (;;) { const { bytesRead } = await file.read(buffer, 0, buffer.length, null); if (!bytesRead) break;
      count += bytesRead; if (count > before.size) invalid('boundary_file_changed'); hash.update(buffer.subarray(0, bytesRead)); }
    const after = await file.stat(); const current = await lstat(path);
    if (count !== before.size || after.size !== before.size || after.mtimeMs !== before.mtimeMs || after.ctimeMs !== before.ctimeMs
      || after.nlink !== before.nlink || current.isSymbolicLink() || current.ino !== before.ino || current.dev !== before.dev) invalid('boundary_file_changed');
    return hash.digest('hex');
  } finally { await file.close(); }
}
async function ownDirectory() {
  for (const path of [join(REPO, '.aurora'), PARENT]) {
    await rejectLinkedParents(path);
    try { await mkdir(path); } catch (error) { if (error.code !== 'EEXIST') throw error; }
    const metadata = await lstat(path);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) invalid('boundary_unsafe_directory');
  }
  return mkdtemp(join(PARENT, 'boundary-'));
}

function startAgent(args, kind, environment, fail) {
  const child = spawn(EXECUTABLE, args, { cwd: REPO, env: environment, shell: false,
    windowsHide: true, detached: false, stdio: ['pipe', 'pipe', 'pipe'] });
  const close = deferred(); const spawnDone = deferred(); const queue = [];
  const state = { child, closing: false, closed: false, spawned: false, forced: false, stopped: false,
    exitCode: null, exitSignal: null, stdoutBytes: 0, stderrBytes: 0, didClose: close.promise, didSpawn: spawnDone.promise };
  const sequence = kind === 'host' ? ['host_identity', 'host_ready', 'host_stopped'] : ['guest_identity', 'guest_ready', 'guest_stopped'];
  const line = Buffer.alloc(EVENT_BYTES); let used = 0; let phase = 0; let waiter; let malformed = false;
  const reject = code => { malformed = true; queue.length = 0; line.fill(0); used = 0; fail(code); };
  child.stdout.on('data', bytes => {
    try {
      state.stdoutBytes += bytes.length;
      if (state.stdoutBytes > OUTPUT_BYTES) { reject('boundary_output_limit'); return; }
      if (malformed) return;
      for (const byte of bytes) {
        if (byte !== 10) { if (used === line.length) { reject('boundary_event_limit'); return; } line[used++] = byte; continue; }
        let value;
        try { value = decodeBoundaryEvent(line.subarray(0, used)); } catch { reject('boundary_event_invalid'); return; }
        line.fill(0); used = 0;
        if (value.event !== sequence[phase] || (value.event === `${kind}_stopped`
          && (!state.closing || !isDeepStrictEqual(value, { event: `${kind}_stopped` })))) { reject('boundary_event_unexpected'); return; }
        phase++; if (value.event === `${kind}_stopped`) state.stopped = true;
        if (waiter) { const done = waiter; waiter = undefined; done(value); }
        else if (queue.length < 3) queue.push(value); else { reject('boundary_event_queue_limit'); return; }
      }
    } finally { bytes.fill(0); }
  });
  child.stderr.on('data', bytes => { state.stderrBytes += bytes.length; bytes.fill(0); fail('boundary_agent_stderr'); });
  for (const stream of [child.stdout, child.stderr]) stream.on('error', () => fail('boundary_agent_io'));
  child.stdin.on('error', () => { if (!state.closing) fail('boundary_agent_io'); });
  child.once('spawn', () => { state.spawned = true; spawnDone.resolve(); });
  child.on('error', () => { spawnDone.resolve(); fail('boundary_spawn_failed'); });
  child.once('exit', (code, signal) => { state.exitCode = code; state.exitSignal = signal; if (!state.closing) fail('boundary_agent_exited'); });
  child.once('close', (code, signal) => { state.closed = true; state.exitCode = code; state.exitSignal = signal;
    if (used) fail('boundary_event_truncated'); line.fill(0); used = 0; close.resolve(); });
  state.event = () => queue.length ? Promise.resolve(queue.shift()) : new Promise(resolve => { waiter = resolve; });
  state.clear = () => { for (const value of queue) for (const key of Object.keys(value)) delete value[key]; queue.length = 0; waiter = undefined; line.fill(0); };
  state.write = text => {
    if (typeof text !== 'string' || Buffer.byteLength(text) > 512 || /[\r\n\0]/.test(text) || child.stdin.destroyed) invalid('boundary_input_invalid');
    child.stdin.write(`${text}\n`, error => { if (error && !state.closing) fail('boundary_agent_io'); });
  };
  return state;
}
async function stopAgent(agent, fail) {
  agent.closing = true;
  if (!agent.closed && !agent.spawned) await Promise.race([agent.didClose, agent.didSpawn]);
  if (!agent.closed) {
    try { agent.child.stdin.end('stop\n'); } catch { /* observe close/exit instead */ }
    let timer; await Promise.race([agent.didClose, new Promise(resolve => { timer = setTimeout(resolve, 5000); })]); clearTimeout(timer);
    if (!agent.closed) { agent.forced = true; fail('boundary_cleanup_forced');
      // The only permitted kill target is this exact owned ChildProcess.
      try { agent.child.kill('SIGKILL'); } catch { fail('boundary_cleanup_failed'); }
    }
  }
  // OS reaping can exceed the graceful deadline; success requires an actual close.
  await agent.didClose;
  if (agent.exitCode !== 0 || agent.exitSignal !== null || !agent.stopped) fail('boundary_shutdown_failed');
  agent.clear();
}
async function closeUdp(socket) { if (socket) await new Promise(resolve => { try { socket.close(resolve); } catch { resolve(); } }); }

/** Fixed, owned loopback fixture. Off by default. The cap is a service ceiling,
 * not a forced QUIC path MTU. Exactly two CLI agents; endpoints/probes are Node
 * sockets in this process. No Java, relay, mod config, account or audio work.
 * Only {authorized:true, timeoutMs?, signal?} is accepted; no path/executable,
 * destination or process injection. A fresh ignored plan is retained unchanged.
 */
export async function runBoundarySmoke(options = {}) {
  if (!options || typeof options !== 'object' || Array.isArray(options)
    || Object.keys(options).some(key => !['authorized', 'timeoutMs', 'signal'].includes(key))
    || (options.signal !== undefined && !(options.signal instanceof AbortSignal))) invalid('boundary_invalid_options');
  if (options.authorized !== true) invalid('boundary_explicit_opt_in_required');
  const timeoutMs = options.timeoutMs ?? 20_000;
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 60_000) invalid('boundary_invalid_options');
  if (process.platform !== 'win32' || !/^[a-zA-Z]:[\\/]Windows$/i.test(process.env.SystemRoot ?? '')) invalid('boundary_windows_required');
  if (options.signal?.aborted) invalid('boundary_cancelled');
  const started = performance.now(); const deadline = started + timeoutMs;
  const controller = new AbortController(); const failure = deferred(); let failureCode;
  const fail = code => { if (!failureCode) { failureCode = code; controller.abort(); failure.resolve(); } };
  const check = () => { if (!failureCode && performance.now() >= deadline) fail('boundary_deadline'); if (failureCode) invalid(failureCode); };
  const cancel = () => fail('boundary_cancelled'); options.signal?.addEventListener('abort', cancel, { once: true });
  if (options.signal?.aborted) cancel();
  const timer = setTimeout(() => fail('boundary_deadline'), timeoutMs);
  const wait = async (promise, limit = 5000) => {
    let localTimer;
    const remaining = Math.max(1, deadline - performance.now());
    try { const result = await Promise.race([promise, failure.promise,
      new Promise(resolve => { localTimer = setTimeout(() => { fail(remaining <= limit ? 'boundary_deadline' : 'boundary_operation_deadline'); resolve(); }, Math.min(limit, remaining)); })]);
      check(); return result;
    } finally { clearTimeout(localTimer); }
  };
  const agents = []; const tcpSockets = new Set(); const udpSockets = [];
  const endpoint = { tcpAccepts: 0, tcpBytes: 0, udpPackets: 0, udpBytes: 0, oversizedReturnsEmitted: 0, sources: new Set() };
  const allowedPackets = new Map(); let tcpServer; let udpServer; let reservation; let planFile; let planHash; let bridges;
  let tcpWork; let result; let ownEndpointsClosed = false; let portsRebound = false;
  const closingSockets = [];
  const ownTcp = socket => { tcpSockets.add(socket); socket.on('error', () => fail('boundary_tcp_io'));
    socket.once('close', () => tcpSockets.delete(socket)); return socket; };
  try {
    if (await verifiedBytes(EXECUTABLE, 128 * 1024 * 1024, true) !== EXECUTABLE_SHA256) invalid('boundary_binary_unreviewed'); check();
    const directory = await ownDirectory(); check();
    tcpServer = createServer(socket => { ownTcp(socket); endpoint.tcpAccepts++;
      if (endpoint.tcpAccepts > 1) { fail('boundary_tcp_endpoint_unexpected'); socket.destroy(); return; }
      socket.on('data', bytes => { endpoint.tcpBytes += bytes.length; if (endpoint.tcpBytes > TCP_BYTES) fail('boundary_tcp_overflow'); });
      socket.pipe(socket);
    });
    tcpServer.on('error', () => fail('boundary_tcp_bind_failed'));
    await wait(new Promise(resolve => tcpServer.listen({ host: '127.0.0.1', port: 0, exclusive: true }, resolve)));
    udpServer = createSocket('udp4'); udpServer.on('error', () => fail('boundary_udp_io'));
    udpServer.on('message', (bytes, source) => {
      endpoint.udpPackets++; endpoint.udpBytes += bytes.length;
      const approved = allowedPackets.get(digest(bytes));
      if (source.address !== '127.0.0.1' || bytes.length !== CAP || !approved || approved.seen) { fail('boundary_endpoint_unexpected_datagram'); return; }
      approved.seen = true; approved.origin = source.port; endpoint.sources.add(source.port);
      // Owned opaque fixture: exercise the host return cap independently of
      // reply credit, then send the exact valid echo on the same association.
      // A local send callback records emission, never proof of UDP delivery.
      udpServer.send(Buffer.alloc(CAP + 1, 0xa7), source.port, source.address, error => {
        if (error) { fail('boundary_udp_io'); return; }
        endpoint.oversizedReturnsEmitted++;
        udpServer.send(bytes, source.port, source.address, error => { if (error) fail('boundary_udp_io'); });
      });
    });
    await wait(new Promise(resolve => udpServer.bind({ address: '127.0.0.1', port: 0, exclusive: true }, resolve)));
    reservation = createSocket('udp4'); reservation.on('error', () => fail('boundary_udp_bind_failed'));
    await wait(new Promise(resolve => reservation.bind({ address: '127.0.0.1', port: 0, exclusive: true }, resolve)));
    const plan = { schemaVersion: 1, bind: `127.0.0.1:${reservation.address().port}`, durationSeconds: 60,
      services: [
        { serviceId: 'game', protocol: 'tcp', endpoint: `127.0.0.1:${tcpServer.address().port}`, revision: 1, requiredCapability: 'reliableStream', maxDatagramBytes: 1400, udpReturnPolicy: { type: 'replyBudget' } },
        { serviceId: 'aux', protocol: 'udp', endpoint: `127.0.0.1:${udpServer.address().port}`, revision: 1, requiredCapability: 'datagram', maxDatagramBytes: CAP,
          udpReturnPolicy: { type: 'authorizedDownstream', bytesPerSecond: 100000, datagramsPerSecond: 100 } },
      ] };
    const planBytes = Buffer.from(`${JSON.stringify(plan)}\n`); planHash = digest(planBytes); planFile = join(directory, 'plan.json');
    await rejectLinkedParents(planFile); await writeFile(planFile, planBytes, { flag: 'wx' }); check();
    if (await verifiedBytes(planFile, 16384) !== planHash) invalid('boundary_plan_changed'); check();
    const environment = { SystemRoot: process.env.SystemRoot, windir: process.env.SystemRoot, TEMP: directory, TMP: directory, USERPROFILE: directory };
    const host = startAgent(['host-plan', planFile, planHash], 'host', environment, fail); agents.push(host);
    const hostIdentity = await wait(host.event());
    if (!/^[0-9a-f]{64}$/.test(hostIdentity.peer ?? '') || !isDeepStrictEqual(hostIdentity, {
      event: 'host_identity', peer: hostIdentity.peer, session: hostIdentity.peer, bind: plan.bind, planHash,
      approvedServices: plan.services, durationSeconds: 60,
      next: 'review these services, then enter the guest public peer ID to authorize; Ctrl+C cancels',
    })) invalid('boundary_plan_approval_mismatch');
    const guest = startAgent(['guest', hostIdentity.peer, plan.bind, hostIdentity.session,
      '127.0.0.1:0', '127.0.0.1:0', '60'], 'guest', environment, fail); agents.push(guest);
    const guestIdentity = await wait(guest.event());
    if (!/^[0-9a-f]{64}$/.test(guestIdentity.peer ?? '') || guestIdentity.peer === hostIdentity.peer || !isDeepStrictEqual(guestIdentity, {
      event: 'guest_identity', peer: guestIdentity.peer, next: 'authorize this public peer at host, then enter ready',
    })) invalid('boundary_guest_identity_invalid');
    await closeUdp(reservation); reservation = undefined; check();
    host.write(guestIdentity.peer);
    for (const value of [hostIdentity, guestIdentity]) for (const key of Object.keys(value)) delete value[key];
    if (!isDeepStrictEqual(await wait(host.event()), { event: 'host_ready', experimental: true, relay: false,
      next: 'enter stop to revoke and close' })) invalid('boundary_host_ready_invalid');
    guest.write('ready'); bridges = validateBoundaryReady(await wait(guest.event())); check();

    const payload = Buffer.from(Array.from({ length: TCP_BYTES }, (_, index) => index % 251));
    const tcp = ownTcp(createConnection({ host: '127.0.0.1', port: bridges.tcp }));
    const connected = deferred(); const ended = deferred(); const progressed = deferred(); let sent = 0; let received = 0;
    tcp.once('connect', connected.resolve);
    tcp.on('data', bytes => {
      if (received + bytes.length > payload.length || !bytes.equals(payload.subarray(received, received + bytes.length))) { fail('boundary_tcp_integrity'); return; }
      received += bytes.length; if (received >= 32768) progressed.resolve();
    });
    tcp.once('end', () => { if (received !== TCP_BYTES) fail('boundary_tcp_truncated'); ended.resolve(); });
    tcp.once('close', () => { if (received !== TCP_BYTES && !controller.signal.aborted) fail('boundary_tcp_truncated'); });
    await wait(connected.promise);
    const abortTcp = () => tcp.destroy(); controller.signal.addEventListener('abort', abortTcp, { once: true });
    tcpWork = (async () => {
      try {
        for (let offset = 0; offset < payload.length; offset += 32768) {
          check(); await wait(new Promise((resolve, reject) => tcp.write(payload.subarray(offset, offset + 32768), error => error ? reject(error) : resolve())));
          sent += 32768;
          if (offset + 32768 < payload.length) await delay(50, undefined, { signal: controller.signal });
        }
        tcp.end(); await wait(ended.promise); check();
      } catch (error) { if (!failureCode) fail(error instanceof BoundarySmokeError ? error.code : 'boundary_tcp_io'); }
      finally { controller.signal.removeEventListener('abort', abortTcp); }
    })();
    await wait(progressed.promise); const overlapBefore = { sent, received, endpoint: endpoint.tcpBytes };
    let rejected = 0; let valid = 0; let oversizedReturnsReceived = 0;
    for (let source = 0; source < 2; source++) {
      const socket = createSocket('udp4'); udpSockets.push(socket); socket.on('error', () => fail('boundary_udp_io'));
      let expected; let replyDone;
      socket.on('message', bytes => {
        if (bytes.length > CAP) oversizedReturnsReceived++;
        if (!expected || !bytes.equals(expected)) { fail('boundary_udp_integrity'); return; }
        expected = undefined; replyDone?.(); replyDone = undefined;
      });
      await wait(new Promise(resolve => socket.bind({ address: '127.0.0.1', port: 0, exclusive: true }, resolve)));
      await wait(new Promise((resolve, reject) => socket.connect(bridges.udp, '127.0.0.1', error => error ? reject(error) : resolve())));
      const packet = (phase, length) => Buffer.from(Array.from({ length }, (_, index) => (index * 31 + source * 13 + phase * 17) % 251));
      const exact = async phase => {
        const bytes = packet(phase, CAP); const record = { seen: false, origin: null }; allowedPackets.set(digest(bytes), record);
        expected = bytes; const replied = new Promise(resolve => { replyDone = resolve; });
        try { await wait(new Promise((resolve, reject) => socket.send(bytes, error => error ? reject(error) : resolve()))); await wait(replied);
          if (!record.seen) invalid('boundary_endpoint_delivery_missing'); valid++; return record.origin;
        } finally { expected = undefined; replyDone = undefined; }
      };
      const origin = await exact(0);
      const before = { packets: endpoint.udpPackets, bytes: endpoint.udpBytes };
      await wait(new Promise((resolve, reject) => socket.send(packet(1, CAP + 1), error => error ? reject(error) : resolve())));
      await wait(delay(SILENCE_MS, undefined, { signal: controller.signal })); check();
      if (endpoint.udpPackets !== before.packets || endpoint.udpBytes !== before.bytes) invalid('boundary_oversize_delivered');
      if (await exact(2) !== origin) invalid('boundary_udp_association_changed'); rejected++;
    }
    // Keep every source open after the last valid echo to observe delayed
    // oversized returns, rather than letting cleanup hide a late delivery.
    await wait(delay(SILENCE_MS, undefined, { signal: controller.signal })); check();
    const overlapAfter = { sent, received, endpoint: endpoint.tcpBytes };
    if (overlapAfter.sent <= overlapBefore.sent || overlapAfter.received <= overlapBefore.received || overlapAfter.endpoint <= overlapBefore.endpoint
      || overlapBefore.sent >= TCP_BYTES || overlapAfter.sent >= TCP_BYTES) invalid('boundary_tcp_overlap_missing');
    await wait(tcpWork); check();
    if (sent !== TCP_BYTES || received !== TCP_BYTES || endpoint.tcpBytes !== TCP_BYTES || endpoint.tcpAccepts !== 1
      || endpoint.udpPackets !== 4 || endpoint.udpBytes !== 4 * CAP || endpoint.sources.size !== 2
      || endpoint.oversizedReturnsEmitted !== 4 || oversizedReturnsReceived !== 0) invalid('boundary_summary_invalid');
    result = { result: 'PASS', state: 'PROTÓTIPO', topology: 'same_machine_loopback', servicePayloadCeiling: CAP,
      negotiatedPayload: CAP, tcpBytesEachDirection: TCP_BYTES, tcpConcurrentWithUdp: true,
      tcpBytesForwardedDuringUdp: overlapAfter.endpoint - overlapBefore.endpoint,
      udpLocalSources: 2, udpHostAssociationsObserved: endpoint.sources.size, udpExactEchoes: valid,
      udpOversizeRequests: rejected, udpOversizeBytes: CAP + 1, udpOversizeReachedEndpoint: 0,
      udpOversizeReturnsEmitted: endpoint.oversizedReturnsEmitted, udpOversizeReturnsReachedGuest: oversizedReturnsReceived,
      udpAuthorizedDownstreamProfile: true,
      udpValidAfterRejection: 2, rejectionObservationMs: SILENCE_MS, udpEndpointBytes: endpoint.udpBytes,
      relay: false, minecraft: 'not_tested', voice: 'not_tested', externalNetwork: 'not_tested', physicalPathMtu: 'not_measured' };
  } catch (error) { fail(error instanceof BoundarySmokeError ? error.code : 'boundary_operation_failed'); }
  finally {
    controller.abort(); for (const agent of agents) agent.closing = true;
    for (const socket of tcpSockets) { const closed = deferred(); socket.once('close', closed.resolve); closingSockets.push(closed.promise); socket.destroy(); }
    await Promise.all(agents.map(agent => stopAgent(agent, fail)));
    if (tcpWork) await tcpWork;
    await Promise.all(closingSockets);
    await Promise.all(udpSockets.map(closeUdp)); await closeUdp(reservation); await closeUdp(udpServer);
    if (tcpServer) await new Promise(resolve => { if (!tcpServer.listening) resolve(); else tcpServer.close(resolve); });
    ownEndpointsClosed = true;
    // Bind only to the exact guest ports selected by our own agents. An occupied
    // port causes failure; we never discover/terminate its owner.
    if (bridges) {
      const tcp = createServer(); const udp = createSocket('udp4'); let tcpListening = false;
      try {
        await new Promise((resolve, reject) => { tcp.once('error', reject); tcp.listen({ host: '127.0.0.1', port: bridges.tcp, exclusive: true }, () => { tcpListening = true; resolve(); }); });
        await new Promise((resolve, reject) => { udp.once('error', reject); udp.bind({ address: '127.0.0.1', port: bridges.udp, exclusive: true }, resolve); });
        portsRebound = true;
      } catch { fail('boundary_bridge_cleanup_failed'); }
      finally { await closeUdp(udp); if (tcpListening) await new Promise(resolve => tcp.close(resolve)); }
    }
    if (planFile && planHash) { try { if (await verifiedBytes(planFile, 16384) !== planHash) fail('boundary_plan_changed'); } catch { fail('boundary_plan_changed'); } }
    clearTimeout(timer); options.signal?.removeEventListener('abort', cancel);
  }
  const cleanup = { agents: agents.length, closed: agents.filter(agent => agent.closed).length,
    forced: agents.some(agent => agent.forced), ownEndpointsClosed, bridgesOpened: Boolean(bridges), guestPortsRebound: portsRebound,
    stdoutBytes: agents.reduce((count, agent) => count + agent.stdoutBytes, 0), stderrBytes: agents.reduce((count, agent) => count + agent.stderrBytes, 0) };
  if (failureCode) throw new BoundarySmokeError(failureCode, cleanup);
  return Object.freeze({ ...result, elapsedMs: Math.round(performance.now() - started), cleanup: Object.freeze(cleanup) });
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const args = process.argv.slice(2);
  Promise.resolve().then(() => {
    if (args.length !== 1 || args[0] !== '--local-lab') invalid('boundary_explicit_opt_in_required');
    const controller = new AbortController(); process.once('SIGINT', () => controller.abort());
    return runBoundarySmoke({ authorized: true, signal: controller.signal });
  }).then(result => process.stdout.write(`${JSON.stringify(result)}\n`), error => {
    process.stderr.write(`${JSON.stringify({ result: 'FAIL', code: error instanceof BoundarySmokeError ? error.code : 'boundary_operation_failed', ...(error.cleanup ? { cleanup: error.cleanup } : {}) })}\n`);
    process.exitCode = 1;
  });
}
