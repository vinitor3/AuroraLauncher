import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createSocket } from 'node:dgram';
import { lstat, mkdir, mkdtemp, open, realpath, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
import { rejectLinkedParents, resolveExplicitLocalPath } from '../connect-voice-tools/local-path.mjs';
import { readLocal } from '../connect-runtime-tools/files.mjs';
import { InvitationGateError, clearInvitationEvent, createInvitationGate } from './invitation-gate.mjs';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const PARENT = join(REPO, '.aurora', 'connect-validation');
const EXECUTABLE = join(REPO, 'apps/connect-lab/target/debug/aurora-connect-lab.exe');
// The already reviewed local debug artifact, not a signing/trust assertion. A
// rebuild requires reviewing its changes and updating this explicit laboratory pin.
const EXECUTABLE_SHA256 = '6be0ff8af7bfb354c69910a39f8f934acdab53fe608a6df0bc9c4e2325306d0b';
const EVENT_BYTES = 16 * 1024;
const OUTPUT_BYTES = 64 * 1024;
const EVENT_TIMEOUT_MS = 10_000;
const STOP_TIMEOUT_MS = 5_000;
const LIFETIME_SECONDS = 120;
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const deferred = () => {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
};

export class GameTunnelError extends Error {
  constructor(code, cleanup) {
    super(code);
    this.name = 'GameTunnelError';
    this.code = code;
    if (cleanup) this.cleanup = Object.freeze(cleanup);
  }
}
const invalid = code => { throw new GameTunnelError(code); };

async function checkExecutable() {
  resolveExplicitLocalPath(EXECUTABLE);
  await rejectLinkedParents(EXECUTABLE);
  const before = await lstat(EXECUTABLE);
  if (!before.isFile() || before.isSymbolicLink() || before.size < 2 || before.size > 128 * 1024 * 1024
    || (await realpath(EXECUTABLE)).toLowerCase() !== EXECUTABLE.toLowerCase()) invalid('game_tunnel_binary_invalid');
  // Cargo's build artifact can have legitimate hardlinks; this read-only
  // exception never applies to the generated plan or its containing directory.
  const file = await open(EXECUTABLE, 'r');
  try {
    const opened = await file.stat();
    if (opened.dev !== before.dev || opened.ino !== before.ino || opened.size !== before.size) invalid('game_tunnel_binary_changed');
    const digest = createHash('sha256'); const buffer = Buffer.alloc(64 * 1024);
    let count = 0;
    for (;;) {
      const { bytesRead } = await file.read(buffer, 0, buffer.length, null);
      if (!bytesRead) break;
      count += bytesRead;
      if (count > before.size) invalid('game_tunnel_binary_changed');
      digest.update(buffer.subarray(0, bytesRead));
    }
    const after = await file.stat(); const current = await lstat(EXECUTABLE);
    if (count !== before.size || after.size !== before.size || after.mtimeMs !== before.mtimeMs
      || after.ctimeMs !== before.ctimeMs || after.nlink !== before.nlink || current.isSymbolicLink()
      || current.dev !== before.dev || current.ino !== before.ino) invalid('game_tunnel_binary_changed');
    if (digest.digest('hex') !== EXECUTABLE_SHA256) invalid('game_tunnel_binary_unreviewed');
  } finally { await file.close(); }
}

async function ownDirectory() {
  resolveExplicitLocalPath(PARENT);
  // Inspect each existing ancestor before making a missing directory. Same-user
  // concurrent parent replacement is outside this local development isolation.
  for (const path of [join(REPO, '.aurora'), PARENT]) {
    await rejectLinkedParents(path);
    try { await mkdir(path); } catch (error) { if (error.code !== 'EEXIST') throw error; }
    const metadata = await lstat(path);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) invalid('game_tunnel_unsafe_directory');
  }
  const directory = await mkdtemp(join(PARENT, 'tunnel-'));
  await rejectLinkedParents(join(directory, 'plan.json'));
  return directory;
}

async function reserveBind() {
  const socket = createSocket('udp4');
  const done = deferred();
  socket.once('error', () => done.resolve(false));
  socket.bind({ address: '127.0.0.1', port: 0, exclusive: true }, () => done.resolve(true));
  const ready = await done.promise;
  if (!ready) {
    try { socket.close(); } catch { /* not bound */ }
    invalid('game_tunnel_reservation_failed');
  }
  return { socket, address: `127.0.0.1:${socket.address().port}` };
}
async function closeReservation(reservation) {
  if (reservation) await new Promise(resolve => reservation.socket.close(resolve));
}

function startAgent(args, kind, environment, fail) {
  let child;
  try {
    child = spawn(EXECUTABLE, args, { cwd: REPO, env: environment, windowsHide: true,
      shell: false, detached: false, stdio: ['pipe', 'pipe', 'pipe'] });
  } catch { invalid('game_tunnel_spawn_failed'); }
  const spawned = deferred(); const closed = deferred();
  const state = { child, spawned: false, closed: false, closing: false, forced: false,
    exitCode: null, exitSignal: null, stdoutBytes: 0, stderrBytes: 0, stopped: false,
    didSpawn: spawned.promise, didClose: closed.promise };
  const sequence = kind === 'host'
    ? ['invite_host_identity', 'host_invitation', 'host_ready', 'host_stopped']
    : ['guest_identity', 'invitation_review', 'guest_ready', 'guest_stopped'];
  const queue = [];
  const pendingInputs = new Set();
  let phase = 0; let waiter; let used = 0; let parsingFailed = false;
  const line = Buffer.alloc(EVENT_BYTES);
  const decoder = new TextDecoder('utf-8', { fatal: true });
  const clearQueue = () => { for (const event of queue) clearInvitationEvent(event); queue.length = 0; };
  const malformed = code => {
    parsingFailed = true; clearQueue(); line.fill(0); used = 0; fail(code);
  };
  const deliver = bytes => {
    let event;
    try { event = JSON.parse(decoder.decode(bytes)); }
    catch { malformed('game_tunnel_event_invalid'); return; }
    if (!event || typeof event !== 'object' || Array.isArray(event) || phase >= sequence.length || event.event !== sequence[phase]
      || (event.event === `${kind}_stopped` && (!state.closing
        || !isDeepStrictEqual(event, { event: `${kind}_stopped` })))) {
      clearInvitationEvent(event); malformed('game_tunnel_event_unexpected'); return;
    }
    phase++;
    if (event.event === `${kind}_stopped`) state.stopped = true;
    if (waiter) { const pending = waiter; waiter = undefined; pending(event); }
    else if (queue.length < 3) queue.push(event);
    else { clearInvitationEvent(event); malformed('game_tunnel_event_queue_limit'); }
  };
  child.stdout.on('data', bytes => {
    try {
      state.stdoutBytes += bytes.length;
      if (state.stdoutBytes > OUTPUT_BYTES) { malformed('game_tunnel_output_limit'); return; }
      if (parsingFailed) return;
      for (const byte of bytes) {
        if (byte === 10) {
          deliver(line.subarray(0, used)); line.fill(0); used = 0;
          if (parsingFailed) return;
        } else if (used === EVENT_BYTES) { malformed('game_tunnel_event_limit'); return; }
        else line[used++] = byte;
      }
    } finally { bytes.fill(0); }
  });
  child.stderr.on('data', bytes => {
    state.stderrBytes += bytes.length;
    if (state.stderrBytes > OUTPUT_BYTES) fail('game_tunnel_output_limit');
    bytes.fill(0);
  });
  child.stdout.on('error', () => fail('game_tunnel_io_failed'));
  child.stderr.on('error', () => fail('game_tunnel_io_failed'));
  // EPIPE is observed even if a child exits during cleanup. Exit/close and the
  // bounded stop decide success; a broken stdin cannot leave an unhandled error.
  child.stdin.on('error', () => { if (!state.closing) fail('game_tunnel_io_failed'); });
  child.once('spawn', () => { state.spawned = true; spawned.resolve(); });
  child.on('error', () => fail(state.spawned ? 'game_tunnel_io_failed' : 'game_tunnel_spawn_failed'));
  child.once('exit', (code, signal) => {
    state.exitCode = code; state.exitSignal = signal;
    if (!state.closing) fail('game_tunnel_agent_exited');
  });
  child.once('close', (code, signal) => {
    state.closed = true; state.exitCode = code; state.exitSignal = signal;
    if (used) fail('game_tunnel_event_truncated');
    line.fill(0); used = 0;
    closed.resolve();
  });
  state.event = () => queue.length ? Promise.resolve(queue.shift())
    : new Promise(resolve => { waiter = resolve; });
  state.clear = () => {
    clearQueue(); waiter = undefined; line.fill(0); used = 0;
    for (const input of pendingInputs) input.fill(0);
    pendingInputs.clear();
  };
  state.write = text => {
    if (state.closed || child.stdin.destroyed) invalid('game_tunnel_io_failed');
    if (typeof text !== 'string' || Buffer.byteLength(text) > 4096 || /[\r\n\0]/.test(text)) invalid('game_tunnel_input_invalid');
    const input = Buffer.from(`${text}\n`);
    pendingInputs.add(input);
    try {
      child.stdin.write(input, error => {
        input.fill(0); pendingInputs.delete(input);
        if (error && !state.closing) fail('game_tunnel_io_failed');
      });
    } catch { input.fill(0); pendingInputs.delete(input); invalid('game_tunnel_io_failed'); }
  };
  return state;
}

async function stopAgent(agent, fail) {
  agent.closing = true;
  if (!agent.spawned && !agent.closed) await Promise.race([agent.didSpawn, agent.didClose]);
  if (!agent.closed) {
    try { agent.child.stdin.end('stop\n'); } catch { /* stop deadline below */ }
    let timer;
    await Promise.race([agent.didClose, new Promise(resolve => { timer = setTimeout(resolve, STOP_TIMEOUT_MS); })]);
    clearTimeout(timer);
    if (!agent.closed) {
      agent.forced = true; fail('game_tunnel_cleanup_forced');
      // Only the ChildProcess created here. Never kill by discovered PID/name.
      try { agent.child.kill('SIGKILL'); } catch { fail('game_tunnel_cleanup_failed'); }
    }
  }
  await agent.didClose;
  if (agent.exitCode !== 0 || agent.exitSignal !== null || !agent.stopped) fail('game_tunnel_shutdown_failed');
  agent.clear();
}

/**
 * Opens one direct authenticated host/guest pair with one approved game/tcp
 * service to fixed loopback port 43001. No Java, Minecraft, account, relay,
 * voice or destination ownership is inferred. Two calls mean two independent
 * hosts/sessions/leases, not one host with two participants.
 *
 * The callback receives only {bridgePort, signal}; it must cooperate with abort
 * and own/reap any sockets or work it creates. A late callback is observed but
 * cannot turn cancellation/failure into success. No process handle or identity
 * is exposed. Success requires normal zero exits and both stopped events.
 * The owned pair uses one signed invitation. Its exact local plan, public
 * bindings, grants, deadline and Rust-verified review precede digest approval.
 * Invitations stay in RAM/stdin; V8 strings cannot be guaranteed zeroized.
 *
 * Startup and operation share timeoutMs (default 60 s, maximum 120 s). Each
 * event waits at most 10 s. Cleanup always reaps both children, allowing 5 s
 * for stop before killing only the owned process; any forced close is failure.
 * OS reaping may finish later. Generated non-secret plan files are retained in
 * the ignored validation directory; personal configuration is never changed.
 */
export async function withGameTunnel(operation, options = {}) {
  return withTunnel(operation, options);
}

// Test-only SVC control profile. The mod's announcement must select this exact
// guest port. No caller can provide a destination or broaden the service set.
export async function withSvcTunnel(operation, options = {}) {
  return withTunnel(operation, options, 'svc');
}

export async function withPlasmoTunnel(operation, options = {}) {
  return withTunnel(operation, options, 'plasmo');
}

async function withTunnel(operation, options, voiceProfile) {
  const voice = voiceProfile !== undefined;
  if (typeof operation !== 'function' || !options || typeof options !== 'object' || Array.isArray(options)
    || Object.keys(options).some(key => !['signal', 'timeoutMs'].includes(key))
    || (options.signal !== undefined && !(options.signal instanceof AbortSignal))) invalid('game_tunnel_invalid_options');
  const timeoutMs = options.timeoutMs ?? 60_000;
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 120_000) invalid('game_tunnel_invalid_options');
  if (process.platform !== 'win32' || !/^[a-zA-Z]:[\\/]Windows$/i.test(process.env.SystemRoot ?? '')) invalid('game_tunnel_windows_required');
  if (options.signal?.aborted) invalid('game_tunnel_cancelled');

  const controller = new AbortController(); const failure = deferred();
  let failureCode;
  const fail = code => {
    if (!failureCode) { failureCode = code; controller.abort(); failure.resolve(); }
  };
  const check = () => { if (failureCode) invalid(failureCode); };
  const onAbort = () => fail('game_tunnel_cancelled');
  options.signal?.addEventListener('abort', onAbort, { once: true });
  if (options.signal?.aborted) onAbort();
  const timer = setTimeout(() => fail('game_tunnel_deadline'), timeoutMs);
  const agents = [];
  let reservation; let voiceReservation; let planFile; let planHash; let value; let callbackWork; let invitationGate;
  const event = async agent => {
    let timer; let result; let active = true;
    const expired = new Promise(resolve => { timer = setTimeout(() => { fail('game_tunnel_event_deadline'); resolve(); }, EVENT_TIMEOUT_MS); });
    const next = agent.event().then(value => {
      if (!active) { clearInvitationEvent(value); return undefined; }
      return value;
    });
    try { result = await Promise.race([next, failure.promise, expired]); check(); return result; }
    catch (error) { clearInvitationEvent(result); throw error; }
    finally { active = false; clearTimeout(timer); }
  };
  try {
    await checkExecutable(); check();
    const directory = await ownDirectory(); check();
    const environment = { SystemRoot: process.env.SystemRoot, windir: process.env.SystemRoot,
      TEMP: directory, TMP: directory, USERPROFILE: directory };
    reservation = await reserveBind(); check();
    if (voice) {
      const socket = createSocket('udp4');
      await new Promise((resolve, reject) => {
        socket.once('error', () => { try { socket.close(); } catch { /* not bound */ } reject(new GameTunnelError('game_tunnel_voice_port_in_use')); });
        socket.bind({ address: '127.0.0.1', port: 35554, exclusive: true }, resolve);
      });
      voiceReservation = { socket }; check();
    }
    const service = { serviceId: 'game', protocol: 'tcp', endpoint: '127.0.0.1:43001',
      revision: 1, requiredCapability: 'reliableStream', maxDatagramBytes: 1400, udpReturnPolicy: { type: 'replyBudget' } };
    const services = [service];
    if (voice) services.push({ serviceId: 'aux', protocol: 'udp', endpoint: voiceProfile === 'svc' ? '127.0.0.1:43002' : '127.0.0.1:43003', revision: 1,
      requiredCapability: 'datagram', maxDatagramBytes: 1400,
      udpReturnPolicy: { type: 'authorizedDownstream', bytesPerSecond: 16384, datagramsPerSecond: 20 } });
    const plan = { schemaVersion: 1, bind: reservation.address, durationSeconds: LIFETIME_SECONDS, services };
    const bytes = Buffer.from(`${JSON.stringify(plan)}\n`);
    planFile = join(directory, 'plan.json'); planHash = hash(bytes);
    await writeFile(planFile, bytes, { flag: 'wx' }); check();
    invitationGate = createInvitationGate({ plan, planHash });
    const host = startAgent(['host-invite-plan', planFile, planHash], 'host', environment, fail); agents.push(host);
    const hostIdentity = await event(host);
    let hostPin;
    try { hostPin = invitationGate.hostIdentity(hostIdentity); }
    finally { clearInvitationEvent(hostIdentity); }
    const guest = startAgent([voice ? 'guest-invite' : 'guest-invite-tcp', hostPin,
      '127.0.0.1:0', ...(voice ? ['127.0.0.1:35554'] : []), String(LIFETIME_SECONDS)], 'guest', environment, fail); agents.push(guest);
    hostPin = undefined;
    const guestIdentity = await event(guest);
    try { invitationGate.guestIdentity(guestIdentity); }
    finally { clearInvitationEvent(guestIdentity); }
    // The CLI binds after authorizing its guest. Keep the reservation until this
    // last moment; a foreign winner of the unavoidable release/bind race causes
    // a startup error, never permission to terminate or modify that process.
    await closeReservation(reservation); reservation = undefined; check();
    invitationGate.authorizePeer(peer => host.write(peer));
    const invitation = await event(host);
    try { invitationGate.invitation(invitation, encoded => guest.write(encoded)); }
    finally { clearInvitationEvent(invitation); }
    const hostReady = await event(host);
    try {
      if (!isDeepStrictEqual(hostReady, { event: 'host_ready', experimental: true, relay: false,
        next: 'enter stop to revoke and close; reconnect requires a fresh host/session/invitation' })) invalid('game_tunnel_mode_invalid');
    } finally { clearInvitationEvent(hostReady); }
    const review = await event(guest);
    try { invitationGate.review(review); }
    finally { clearInvitationEvent(review); }
    if (voiceReservation) { await closeReservation(voiceReservation); voiceReservation = undefined; check(); }
    check(); invitationGate.approve(approval => guest.write(approval));
    const guestReady = await event(guest);
    let bridgePort; let maxDatagramPayload;
    try {
      const match = typeof guestReady.tcp === 'string' && /^127\.0\.0\.1:([1-9][0-9]{0,4})$/.exec(guestReady.tcp);
      if (!match || Number(match[1]) > 65535 || guestReady.experimental !== true) invalid('game_tunnel_mode_invalid');
      const expected = { event: 'guest_ready', tcp: guestReady.tcp, experimental: true, next: 'enter stop to close',
        ...(voice ? { udp: '127.0.0.1:35554', maxDatagramPayload: guestReady.maxDatagramPayload } : { mode: 'tcp' }) };
      if (!isDeepStrictEqual(guestReady, expected) || (voice && (!Number.isSafeInteger(guestReady.maxDatagramPayload)
        || guestReady.maxDatagramPayload < 128 || guestReady.maxDatagramPayload > 1400))) invalid('game_tunnel_mode_invalid');
      bridgePort = Number(match[1]); maxDatagramPayload = guestReady.maxDatagramPayload;
    } finally { clearInvitationEvent(guestReady); }
    check();
    callbackWork = Promise.resolve().then(() => {
      check(); return operation(Object.freeze({ bridgePort, signal: controller.signal,
        ...(voice ? { voicePort: 35554, maxDatagramPayload } : {}) }));
    }).then(result => { value = result; }, () => fail('game_tunnel_operation_failed'));
    await Promise.race([callbackWork, failure.promise]);
  } catch (error) { fail(error instanceof GameTunnelError || error instanceof InvitationGateError ? error.code : 'game_tunnel_preparation_failed'); }
  finally {
    controller.abort();
    invitationGate?.dispose();
    // Mark both closing before stopping either, because closing a host can end
    // its guest's transport. Both stop deadlines run concurrently, never 5+5 s.
    for (const agent of agents) agent.closing = true;
    await Promise.all(agents.map(agent => stopAgent(agent, fail)));
    if (reservation) {
      try { await closeReservation(reservation); } catch { fail('game_tunnel_reservation_failed'); }
    }
    if (voiceReservation) {
      try { await closeReservation(voiceReservation); } catch { fail('game_tunnel_reservation_failed'); }
    }
    // The fixed voice probe cooperates with abort and owns its UDP socket.
    // Await that cleanup; retain the prior generic TCP callback semantics.
    if (voice && callbackWork) await callbackWork;
    clearTimeout(timer); options.signal?.removeEventListener('abort', onAbort);
    if (planFile && planHash) {
      try { if (hash(await readLocal(planFile, EVENT_BYTES)) !== planHash) fail('game_tunnel_plan_changed'); }
      catch { fail('game_tunnel_plan_changed'); }
    }
  }
  const cleanup = Object.freeze({ agents: agents.length, closed: agents.filter(agent => agent.closed).length,
    forced: agents.some(agent => agent.forced), stdoutBytes: agents.reduce((n, agent) => n + agent.stdoutBytes, 0),
    stderrBytes: agents.reduce((n, agent) => n + agent.stderrBytes, 0) });
  if (failureCode) throw new GameTunnelError(failureCode, cleanup);
  return { value, cleanup };
}
