// Isolated control-plane probe, not a voice client or an Aurora transport codec.
// SVC 1.21.1-2.6.22, compatibility 20. Source inspected at commit
// 7d4317264596736e935cb5a5fbcf899376aa8b34 in henkelmax/simple-voice-chat:
// net/{RequestSecretPacket,SecretPacket}, voice/common/{Secret,NetworkMessage,
// AuthenticatePacket}, and voice/client/{ClientNetworkMessage,ClientVoicechatConnection}.
// Official JAR: 5,369,475 B, SHA-256
// 972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36.
// Its BuildConstants class contains the compatibility string "20" parsed by the
// upstream template. This clean control-only implementation uses node:crypto;
// no upstream classes, audio codec, capture, playback or recording are included.
import dgram from 'node:dgram';
import { createCipheriv, createDecipheriv, createHash, randomBytes } from 'node:crypto';
import { performance } from 'node:perf_hooks';

const MAGIC = 0xff;
const AUTH = 5; const AUTH_ACK = 6; const KEEP_ALIVE = 8; const CHECK = 9; const CHECK_ACK = 10;
const MAX_SECRET_PACKET = 256;
const MAX_DATAGRAM = 512;
const MAX_PACKETS = 32;
const MAX_BYTES = 8192;
const MAX_SENDS = 16;
const MAX_PENDING_SENDS = 4;
const WATCH_MS = 35_000; const MAX_GAP_MS = 5000;
const WATCH_PACKETS = 96; const WATCH_BYTES = 16 * 1024; const WATCH_SENDS = 96;
const activeClients = new WeakSet();
const allowedErrors = new Set(['arguments', 'unsafe_debug_environment', 'already_active', 'cancelled',
  'deadline', 'client_closed', 'client_failed', 'secret_packet', 'announcement', 'player_mismatch',
  'duplicate_secret', 'socket_failed', 'socket_closed', 'datagram', 'datagram_address',
  'authentication', 'unexpected_packet', 'limits', 'send_queue', 'cleanup_failed', 'failed',
  'keepalive_interval', 'keepalive_gap', 'keepalive_evidence', 'keepalive_replay', 'send_unconfirmed', 'clock']);

export class SvcControlError extends Error {
  constructor(code) {
    super(`svc_control_${allowedErrors.has(code) ? code : 'failed'}`);
    this.name = 'SvcControlError';
  }
}
const fail = code => { throw new SvcControlError(code); };
const redacted = error => error instanceof SvcControlError ? error : new SvcControlError('failed');

function varint(bytes, start, maximum, code) {
  let value = 0;
  for (let index = 0; index < 3; index++) {
    if (start + index >= bytes.length) fail(code);
    const byte = bytes[start + index];
    value += (byte & 0x7f) * (2 ** (7 * index));
    if (!(byte & 0x80)) {
      if (value > maximum || (index > 0 && value < 2 ** (7 * index))) fail(code);
      return { value, next: start + index + 1 };
    }
  }
  fail(code);
}

function lengthPrefix(length) {
  // Every outgoing control packet is smaller than 128 bytes.
  if (!Number.isSafeInteger(length) || length < 1 || length >= 128) fail('limits');
  return Buffer.from([length]);
}

function parseSecret(bytes, expectedUuid, approvedPort, keepalive) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 56 || bytes.length > MAX_SECRET_PACKET) fail('secret_packet');
  // Fields preceding the bounded UTF string have a fixed width in the pinned mod.
  if (bytes.readInt32BE(16) !== 43002 || bytes[36] > 2
    || bytes.readInt32BE(37) < 512 || bytes.readInt32BE(37) > 2048
    || !Number.isFinite(bytes.readDoubleBE(41)) || bytes.readDoubleBE(41) < 1 || bytes.readDoubleBE(41) > 1_000_000
    || bytes.readInt32BE(49) < 1000 || bytes[53] > 1) fail('secret_packet');
  const name = varint(bytes, 54, 64, 'secret_packet');
  if (name.next + name.value + 1 !== bytes.length || bytes.at(-1) > 1) fail('secret_packet');
  let announcement;
  try { announcement = new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(name.next, name.next + name.value)); }
  catch { fail('secret_packet'); }
  if (announcement !== `127.0.0.1:${approvedPort}`) fail('announcement');
  if (!bytes.subarray(20, 36).equals(expectedUuid)) fail('player_mismatch');
  if (keepalive && bytes.readInt32BE(49) !== 1000) fail('keepalive_interval');
  // Only our copies are erased on exit. The caller/library also owns its input
  // buffer; JS/native crypto cannot promise secure erasure of every transient.
  return { key: Buffer.from(bytes.subarray(0, 16)), uuid: Buffer.from(bytes.subarray(20, 36)) };
}

function clientDatagram(data, type) {
  let clear; let encrypted;
  try {
    clear = type === AUTH ? Buffer.concat([Buffer.from([AUTH]), data.uuid, data.key]) : Buffer.from([type]);
    const iv = randomBytes(12);
    const cipher = createCipheriv('aes-128-gcm', data.key, iv, { authTagLength: 16 });
    encrypted = Buffer.concat([cipher.update(clear), cipher.final()]);
    const payload = Buffer.concat([iv, encrypted, cipher.getAuthTag()]);
    return Buffer.concat([Buffer.from([MAGIC]), data.uuid, lengthPrefix(payload.length), payload]);
  } finally { clear?.fill(0); encrypted?.fill(0); }
}

function serverDatagram(bytes, data) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 31 || bytes.length > MAX_DATAGRAM || bytes[0] !== MAGIC) fail('datagram');
  const size = varint(bytes, 1, MAX_DATAGRAM - 2, 'datagram');
  if (size.next + size.value !== bytes.length || size.value < 29) fail('datagram');
  const payload = bytes.subarray(size.next);
  let first; let last; let clear;
  try {
    const decipher = createDecipheriv('aes-128-gcm', data.key, payload.subarray(0, 12), { authTagLength: 16 });
    decipher.setAuthTag(payload.subarray(-16));
    try {
      first = decipher.update(payload.subarray(12, -16)); last = decipher.final();
      clear = Buffer.concat([first, last]);
    } catch { fail('authentication'); }
    if (clear.length !== 1 || ![AUTH_ACK, KEEP_ALIVE, CHECK_ACK].includes(clear[0])) fail('unexpected_packet');
    return clear[0];
  } finally { first?.fill(0); last?.fill(0); clear?.fill(0); }
}

/**
 * Requires a synthetic lab Client already in PLAY. Its enclosing worker must
 * suppress packet/debug logging and own Minecraft lifecycle/transport evidence.
 * Opens one private UDP4 socket on loopback, only after the pinned SVC custom
 * payload advertises the exact approved local bridge port. No DNS is used.
 * Resolves only after authenticated AuthAck and ConnectionCheckAck and socket
 * closure. The explicit keepalive mode samples exactly 35 seconds after the
 * first CheckAck, answering authenticated type 8 controls (server interval must
 * be 1000 ms). It does not decode audio, establish that
 * the listener is an Aurora bridge, or prove account authentication/audibility.
 * Retries each handshake phase at most three times at one-second intervals.
 * timeoutMs bounds the handshake (at most 10 s). Keepalive mode has a separate
 * total budget below 50 s including the 250 ms close allowance; event-loop/OS
 * scheduling can delay cleanup, which must not turn an expired run into PASS.
 * The scheduled sample window never expands to include late packets. GCM verifies
 * packet authentication, not freshness, anti-replay or a correlated RTT.
 * All secret-bearing state remains inside this process/module, never in the
 * return value, diagnostics, an exported codec or a launcher-facing API.
 */
export async function probeSvcControl(client, options = {}) {
  if (!options || typeof options !== 'object' || Array.isArray(options)
    || Object.keys(options).some(key => !['approvedPort', 'signal', 'timeoutMs', 'keepalive'].includes(key))) fail('arguments');
  const { approvedPort, signal, timeoutMs = 5000, keepalive = false } = options;
  if (!client || typeof client !== 'object' || client.state !== 'play'
    || typeof client.on !== 'function' || typeof client.off !== 'function' || typeof client.write !== 'function'
    || typeof client.uuid !== 'string' || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(client.uuid)
    || !Number.isSafeInteger(approvedPort) || approvedPort < 1 || approvedPort > 65535
    || !Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 10000
    || typeof keepalive !== 'boolean'
    || (signal !== undefined && !(signal instanceof AbortSignal))) fail('arguments');
  if (process.env.DEBUG || process.env.NODE_OPTIONS || globalThis.debugNMP) fail('unsafe_debug_environment');
  if (signal?.aborted) fail('cancelled');
  if (activeClients.has(client)) fail('already_active');
  const started = performance.now();
  if (!Number.isFinite(started)) fail('clock');
  activeClients.add(client);
  const expectedUuid = Buffer.from(client.uuid.replaceAll('-', ''), 'hex');
  const counts = { datagramsSent: 0, datagramsReceived: 0, bytesSent: 0, bytesReceived: 0 };
  const maxPackets = keepalive ? WATCH_PACKETS : MAX_PACKETS;
  const maxBytes = keepalive ? WATCH_BYTES : MAX_BYTES;
  const maxSends = keepalive ? WATCH_SENDS : MAX_SENDS;
  let socket; let data; let phase = 'secret'; let finished = false; let socketClosed = false;
  let retry; let deadline; let totalDeadline; let watchTimer; let gapTimer;
  let authAttempts = 0; let checkAttempts = 0;
  const pendingSends = new Set();
  const observedKeepalives = new Set();
  let lastClock = started; let watchEnds; let lastKeepalive; let firstKeepalive;
  let keepalivesReceived = 0; let keepaliveResponsesConfirmed = 0; let maxGap = 0;
  const now = () => {
    const value = performance.now();
    if (!Number.isFinite(value) || value < lastClock) fail('clock');
    lastClock = value;
    // Timers alone cannot enforce a deadline when the event loop is delayed.
    if ((keepalive && value >= started + 49_750)
      || (['secret', 'binding', 'auth', 'check'].includes(phase) && value >= started + timeoutMs)) fail('deadline');
    return value;
  };
  let resolveProbe; let rejectProbe;
  const work = new Promise((resolve, reject) => { resolveProbe = resolve; rejectProbe = reject; });
  const finish = error => {
    if (finished) return;
    finished = true;
    clearInterval(retry); clearTimeout(deadline); clearTimeout(totalDeadline);
    clearTimeout(watchTimer); clearTimeout(gapTimer);
    if (error) rejectProbe(redacted(error)); else resolveProbe();
  };
  const reject = code => finish(new SvcControlError(code));
  const aborted = () => reject('cancelled');
  const clientFailed = () => reject('client_failed');
  const clientEnded = () => reject('client_closed');
  const socketFailed = () => reject('socket_failed');
  const socketEnded = () => { socketClosed = true; if (!finished) reject('socket_closed'); };
  const checkGap = time => {
    const gap = time - lastKeepalive;
    if (gap > MAX_GAP_MS) fail('keepalive_gap');
    maxGap = Math.max(maxGap, gap);
  };
  const finishWatch = () => {
    if (finished) return;
    try {
      const time = now();
      if (time < watchEnds) { watchTimer = setTimeout(finishWatch, Math.ceil(watchEnds - time)); return; }
      checkGap(time);
      if (keepalivesReceived < 3 || lastKeepalive - firstKeepalive < 30_000) fail('keepalive_evidence');
      if (pendingSends.size || keepaliveResponsesConfirmed !== keepalivesReceived) fail('send_unconfirmed');
      phase = 'complete'; finish();
    } catch (error) { finish(redacted(error)); }
  };
  const armGap = () => {
    clearTimeout(gapTimer);
    // A gap of exactly 5000 ms is permitted; no ACK can renew this timer.
    const check = () => {
      if (finished) return;
      try {
        const time = now(); checkGap(time);
        gapTimer = setTimeout(check, Math.ceil(MAX_GAP_MS - (time - lastKeepalive)) + 1);
      }
      catch (error) { finish(redacted(error)); }
    };
    gapTimer = setTimeout(check, MAX_GAP_MS + 1);
  };
  const send = type => {
    if (finished) return;
    let bytes;
    try {
      now();
      if (pendingSends.size >= MAX_PENDING_SENDS) fail('send_queue');
      bytes = clientDatagram(data, type);
      if (++counts.datagramsSent > maxSends || (counts.bytesSent += bytes.length) > maxBytes) fail('limits');
      const pending = { bytes, watched: type === KEEP_ALIVE && phase === 'watching', done: false };
      pendingSends.add(pending);
      socket.send(bytes, error => {
        if (pending.done) return;
        pending.done = true; bytes.fill(0); pendingSends.delete(pending);
        if (finished) return;
        try {
          const time = now();
          if (error) fail('socket_failed');
          if (pending.watched) {
            if (time >= watchEnds) fail('send_unconfirmed');
            checkGap(time); keepaliveResponsesConfirmed++;
          }
        } catch (error) { finish(redacted(error)); }
      });
    } catch (error) { bytes?.fill(0); finish(redacted(error)); }
  };
  const sendPhase = () => {
    if (phase === 'auth' && authAttempts < 3) { authAttempts++; send(AUTH); }
    else if (phase === 'check' && checkAttempts < 3) { checkAttempts++; send(CHECK); }
  };
  const received = (bytes, remote) => {
    if (finished) return;
    try {
      const time = now();
      if (phase === 'watching') {
        if (time >= watchEnds) { finishWatch(); return; }
        checkGap(time);
      }
      if (remote?.address !== '127.0.0.1' || remote?.port !== approvedPort || remote?.family !== 'IPv4') fail('datagram_address');
      if (!Buffer.isBuffer(bytes) || bytes.length > MAX_DATAGRAM) fail('datagram');
      if (++counts.datagramsReceived > maxPackets || (counts.bytesReceived += bytes.length) > maxBytes) fail('limits');
      const type = serverDatagram(bytes, data);
      if (type === AUTH_ACK) {
        if (phase === 'auth') { phase = 'check'; sendPhase(); }
        else if (phase !== 'check' && phase !== 'watching') fail('unexpected_packet');
      } else if (type === CHECK_ACK) {
        // Upstream also ignores repeated acknowledgments after connection.
        // Neither acknowledgment counts as a keepalive or renews the window.
        if (phase === 'watching') return;
        if (phase !== 'check') fail('unexpected_packet');
        if (!keepalive) { phase = 'complete'; finish(); }
        else {
          phase = 'watching'; clearInterval(retry); clearTimeout(deadline);
          watchEnds = time + WATCH_MS; lastKeepalive = time;
          watchTimer = setTimeout(finishWatch, WATCH_MS); armGap();
        }
      } else {
        if (phase !== 'check' && phase !== 'watching') fail('unexpected_packet');
        if (phase === 'watching') {
          // Bounded duplicate-wire rejection within this observation only.
          // Can reject legitimate network duplication. This is not protocol
          // freshness or a replay guarantee across runs; only digests remain.
          const identity = createHash('sha256').update(bytes).digest('hex');
          if (observedKeepalives.has(identity)) fail('keepalive_replay');
          observedKeepalives.add(identity);
          firstKeepalive ??= time; lastKeepalive = time; keepalivesReceived++; armGap();
        }
        send(KEEP_ALIVE);
      }
    } catch (error) { finish(redacted(error)); }
  };
  const payload = packet => {
    if (finished || packet?.channel !== 'voicechat:secret') return;
    try {
      now();
      if (phase !== 'secret') fail('duplicate_secret');
      data = parseSecret(packet.data, expectedUuid, approvedPort, keepalive);
      phase = 'binding';
      socket = dgram.createSocket('udp4');
      socket.on('error', socketFailed); socket.on('close', socketEnded); socket.on('message', received);
      now();
      socket.bind(0, '127.0.0.1', () => {
        if (finished) return;
        try {
          now();
          socket.connect(approvedPort, '127.0.0.1', error => {
            if (finished) return;
            if (error) { reject('socket_failed'); return; }
            try {
              now(); phase = 'auth';
              retry = setInterval(sendPhase, 1000);
              sendPhase();
            } catch (error) { finish(redacted(error)); }
          });
        } catch (error) { finish(error instanceof SvcControlError ? error : new SvcControlError('socket_failed')); }
      });
    } catch (error) { finish(redacted(error)); }
  };
  let failure;
  try {
    client.on('custom_payload', payload); client.on('error', clientFailed); client.on('end', clientEnded);
    signal?.addEventListener('abort', aborted, { once: true });
    deadline = setTimeout(() => reject('deadline'), timeoutMs);
    if (keepalive) totalDeadline = setTimeout(() => reject('deadline'), 49_750);
    if (signal?.aborted) aborted();
    if (!finished) {
      // Announce only the one payload understood by this probe. Do not register
      // a library parser which might expose secret fields as a separate event.
      now(); client.write('custom_payload', { channel: 'minecraft:register', data: Buffer.from('voicechat:secret', 'ascii') });
      if (!finished) { now(); client.write('custom_payload', { channel: 'voicechat:request_secret', data: Buffer.from([0, 0, 0, 20]) }); }
    }
    await work;
  } catch (error) { failure = redacted(error); finish(failure); await work.catch(() => {}); }
  finally {
    finished = true; clearInterval(retry); clearTimeout(deadline); clearTimeout(totalDeadline);
    clearTimeout(watchTimer); clearTimeout(gapTimer);
    signal?.removeEventListener('abort', aborted);
    client.off('custom_payload', payload); client.off('error', clientFailed); client.off('end', clientEnded);
    data?.key.fill(0); data?.uuid.fill(0); expectedUuid.fill(0); data = undefined;
    if (socket && !socketClosed) {
      // Closing still runs when the operation failed or the clock is invalid.
      // Do not apply handshake deadlines here: preserve the original failure
      // while always attempting to release the socket.
      const closeClock = () => {
        const value = performance.now();
        if (!Number.isFinite(value) || value < lastClock) fail('clock');
        lastClock = value; return value;
      };
      let closeStarted;
      if (keepalive) {
        try { closeStarted = closeClock(); }
        catch (error) { failure ??= redacted(error); }
      }
      try {
        await new Promise((resolve, rejectClose) => {
          const timer = setTimeout(() => rejectClose(new SvcControlError('cleanup_failed')), 250);
          try { socket.close(() => {
            clearTimeout(timer);
            try {
              if (keepalive && closeClock() - closeStarted > 250) fail('cleanup_failed');
              resolve();
            } catch (error) { rejectClose(redacted(error)); }
          }); }
          catch (error) {
            clearTimeout(timer);
            if (error?.code === 'ERR_SOCKET_DGRAM_NOT_RUNNING') resolve(); else rejectClose(new SvcControlError('cleanup_failed'));
          }
        });
      } catch (error) { failure ??= error instanceof SvcControlError ? error : new SvcControlError('cleanup_failed'); }
    }
    if (socketClosed) {
      socket.off('error', socketFailed); socket.off('close', socketEnded); socket.off('message', received);
    }
    for (const pending of pendingSends) pending.bytes.fill(0);
    pendingSends.clear(); observedKeepalives.clear();
    activeClients.delete(client);
  }
  let elapsedMs;
  if (!failure) {
    try { elapsedMs = Math.round(now() - started); }
    catch (error) { failure = redacted(error); }
  }
  if (failure) throw failure;
  return Object.freeze({ mod: 'simple_voice_chat', version: '2.6.22', compatibility: 20,
    announcementMatched: true, authenticationAck: true, connectionCheckAck: true,
    ...counts, elapsedMs, udpSocketClosed: true,
    ...(keepalive ? { keepaliveObserved: true, keepaliveWindowMs: WATCH_MS,
      keepalivesReceived, keepaliveResponsesConfirmed,
      keepaliveSpanMs: Math.floor(lastKeepalive - firstKeepalive), keepaliveMaxGapMs: Math.ceil(maxGap) } : {}),
    audio: 'not_tested', accountAuthentication: 'not_tested', runtimeHomologated: false });
}
