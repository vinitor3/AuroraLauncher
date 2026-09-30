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
import { createCipheriv, createDecipheriv, randomBytes } from 'node:crypto';
import { performance } from 'node:perf_hooks';

const MAGIC = 0xff;
const AUTH = 5; const AUTH_ACK = 6; const KEEP_ALIVE = 8; const CHECK = 9; const CHECK_ACK = 10;
const MAX_SECRET_PACKET = 256;
const MAX_DATAGRAM = 512;
const MAX_PACKETS = 32;
const MAX_BYTES = 8192;
const MAX_SENDS = 16;
const MAX_PENDING_SENDS = 4;
const activeClients = new WeakSet();
const allowedErrors = new Set(['arguments', 'unsafe_debug_environment', 'already_active', 'cancelled',
  'deadline', 'client_closed', 'client_failed', 'secret_packet', 'announcement', 'player_mismatch',
  'duplicate_secret', 'socket_failed', 'socket_closed', 'datagram', 'datagram_address',
  'authentication', 'unexpected_packet', 'limits', 'send_queue', 'cleanup_failed', 'failed']);

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

function parseSecret(bytes, expectedUuid, approvedPort) {
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
 * closure. It does not keep a voice session alive, decode audio, establish that
 * the listener is an Aurora bridge, or prove account authentication/audibility.
 * Retries each handshake phase at most three times at one-second intervals.
 * All secret-bearing state remains inside this process/module, never in the
 * return value, diagnostics, an exported codec or a launcher-facing API.
 */
export async function probeSvcControl(client, options = {}) {
  if (!options || typeof options !== 'object' || Array.isArray(options)
    || Object.keys(options).some(key => !['approvedPort', 'signal', 'timeoutMs'].includes(key))) fail('arguments');
  const { approvedPort, signal, timeoutMs = 5000 } = options;
  if (!client || typeof client !== 'object' || client.state !== 'play'
    || typeof client.on !== 'function' || typeof client.off !== 'function' || typeof client.write !== 'function'
    || typeof client.uuid !== 'string' || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(client.uuid)
    || !Number.isSafeInteger(approvedPort) || approvedPort < 1 || approvedPort > 65535
    || !Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 10000
    || (signal !== undefined && !(signal instanceof AbortSignal))) fail('arguments');
  if (process.env.DEBUG || process.env.NODE_OPTIONS || globalThis.debugNMP) fail('unsafe_debug_environment');
  if (signal?.aborted) fail('cancelled');
  if (activeClients.has(client)) fail('already_active');
  activeClients.add(client);
  const expectedUuid = Buffer.from(client.uuid.replaceAll('-', ''), 'hex');
  const started = performance.now();
  const counts = { datagramsSent: 0, datagramsReceived: 0, bytesSent: 0, bytesReceived: 0 };
  let socket; let data; let phase = 'secret'; let finished = false; let socketClosed = false;
  let retry; let deadline; let pendingSends = 0; let authAttempts = 0; let checkAttempts = 0;
  let resolveProbe; let rejectProbe;
  const work = new Promise((resolve, reject) => { resolveProbe = resolve; rejectProbe = reject; });
  const finish = error => {
    if (finished) return;
    finished = true;
    clearInterval(retry); clearTimeout(deadline);
    if (error) rejectProbe(redacted(error)); else resolveProbe();
  };
  const reject = code => finish(new SvcControlError(code));
  const aborted = () => reject('cancelled');
  const clientFailed = () => reject('client_failed');
  const clientEnded = () => reject('client_closed');
  const socketFailed = () => reject('socket_failed');
  const socketEnded = () => { socketClosed = true; if (!finished) reject('socket_closed'); };
  const send = type => {
    if (finished) return;
    let bytes;
    try {
      if (pendingSends >= MAX_PENDING_SENDS) fail('send_queue');
      bytes = clientDatagram(data, type);
      if (++counts.datagramsSent > MAX_SENDS || (counts.bytesSent += bytes.length) > MAX_BYTES) fail('limits');
      pendingSends++;
      socket.send(bytes, error => {
        bytes.fill(0); pendingSends--;
        if (error) reject('socket_failed');
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
      if (remote?.address !== '127.0.0.1' || remote?.port !== approvedPort || remote?.family !== 'IPv4') fail('datagram_address');
      if (!Buffer.isBuffer(bytes) || bytes.length > MAX_DATAGRAM) fail('datagram');
      if (++counts.datagramsReceived > MAX_PACKETS || (counts.bytesReceived += bytes.length) > MAX_BYTES) fail('limits');
      const type = serverDatagram(bytes, data);
      if (type === AUTH_ACK) {
        if (phase === 'auth') { phase = 'check'; sendPhase(); }
        else if (phase !== 'check') fail('unexpected_packet');
      } else if (type === CHECK_ACK) {
        if (phase !== 'check') fail('unexpected_packet');
        phase = 'complete'; finish();
      } else {
        if (phase !== 'check') fail('unexpected_packet');
        send(KEEP_ALIVE);
      }
    } catch (error) { finish(redacted(error)); }
  };
  const payload = packet => {
    if (finished || packet?.channel !== 'voicechat:secret') return;
    try {
      if (phase !== 'secret') fail('duplicate_secret');
      data = parseSecret(packet.data, expectedUuid, approvedPort);
      phase = 'binding';
      socket = dgram.createSocket('udp4');
      socket.on('error', socketFailed); socket.on('close', socketEnded); socket.on('message', received);
      socket.bind(0, '127.0.0.1', () => {
        if (finished) return;
        try {
          socket.connect(approvedPort, '127.0.0.1', error => {
            if (finished) return;
            if (error) { reject('socket_failed'); return; }
            phase = 'auth';
            retry = setInterval(sendPhase, 1000);
            sendPhase();
          });
        } catch { reject('socket_failed'); }
      });
    } catch (error) { finish(redacted(error)); }
  };
  let failure;
  try {
    client.on('custom_payload', payload); client.on('error', clientFailed); client.on('end', clientEnded);
    signal?.addEventListener('abort', aborted, { once: true });
    deadline = setTimeout(() => reject('deadline'), timeoutMs);
    if (signal?.aborted) aborted();
    if (!finished) {
      // Announce only the one payload understood by this probe. Do not register
      // a library parser which might expose secret fields as a separate event.
      client.write('custom_payload', { channel: 'minecraft:register', data: Buffer.from('voicechat:secret', 'ascii') });
      if (!finished) client.write('custom_payload', { channel: 'voicechat:request_secret', data: Buffer.from([0, 0, 0, 20]) });
    }
    await work;
  } catch (error) { failure = redacted(error); finish(failure); await work.catch(() => {}); }
  finally {
    finished = true; clearInterval(retry); clearTimeout(deadline);
    signal?.removeEventListener('abort', aborted);
    client.off('custom_payload', payload); client.off('error', clientFailed); client.off('end', clientEnded);
    data?.key.fill(0); data?.uuid.fill(0); expectedUuid.fill(0); data = undefined;
    if (socket && !socketClosed) {
      try {
        await new Promise((resolve, rejectClose) => {
          const timer = setTimeout(() => rejectClose(new SvcControlError('cleanup_failed')), 250);
          try { socket.close(() => { clearTimeout(timer); resolve(); }); }
          catch (error) {
            clearTimeout(timer);
            if (error?.code === 'ERR_SOCKET_DGRAM_NOT_RUNNING') resolve(); else rejectClose(new SvcControlError('cleanup_failed'));
          }
        });
      } catch { failure ??= new SvcControlError('cleanup_failed'); }
    }
    if (socketClosed) {
      socket.off('error', socketFailed); socket.off('close', socketEnded); socket.off('message', received);
    }
    activeClients.delete(client);
  }
  if (failure) throw failure;
  return Object.freeze({ mod: 'simple_voice_chat', version: '2.6.22', compatibility: 20,
    announcementMatched: true, authenticationAck: true, connectionCheckAck: true,
    ...counts, elapsedMs: Math.round(performance.now() - started), udpSocketClosed: true,
    audio: 'not_tested', accountAuthentication: 'not_tested', runtimeHomologated: false });
}
