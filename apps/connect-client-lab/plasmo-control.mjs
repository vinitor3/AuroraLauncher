// Control-only probe for the pinned Fabric 1.21.1 / Plasmo 2.1.17 JAR:
// SHA256 46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b.
// Layouts checked against its class files and plasmoapp/plasmo-voice commit
// 4c717b85c4bbfa286c7809a4018137142798986b: PacketTcpCodec, PlayerInfoPacket,
// ConnectionPacket, PacketUdpCodec, PingPacket and NettyUdpClientHandler.
// This implements no audio codec/device/client. UDP Ping carries a plaintext
// session credential: it has neither a MAC nor AEAD, and is not an echo/RTT.
import dgram from 'node:dgram';
import crypto from 'node:crypto';
import { performance } from 'node:perf_hooks';
import { offlineUuid, PLAY_NAMES } from '../connect-runtime-tools/play-profile.mjs';

const CHANNEL = 'plasmo:voice/v2';
const MAGIC = 0x4e9004e9;
const EXPECTED_UUID = offlineUuid(PLAY_NAMES[0]);
const MAX_PAYLOAD = 65536; const MAX_PAYLOAD_BYTES = 131072; const MAX_PAYLOADS = 32;
const MAX_DATAGRAM = 512; const MAX_UDP_BYTES = 8192; const MAX_DATAGRAMS = 32;
const OBSERVATION_MS = 35000; const MAX_GAP_MS = 5000; const MIN_SPAN_MS = 30000;
const TOTAL_MS = 49750; // Reserve 250ms for UDP close within the 50s worker budget.
const activeClients = new WeakSet();
const codes = new Set(['arguments', 'unsafe_debug_environment', 'already_active', 'cancelled', 'deadline',
  'client_closed', 'client_failed', 'client_changed', 'player_mismatch', 'request', 'connection',
  'announcement', 'unexpected_packet', 'limits', 'key_generation', 'socket_failed', 'socket_closed',
  'datagram', 'datagram_address', 'authentication', 'send_queue', 'cleanup_failed', 'failed',
  'clock', 'keepalive_gap', 'keepalive_evidence', 'duplicate_ping']);

export class PlasmoControlError extends Error {
  constructor(code) {
    super(`plasmo_control_${codes.has(code) ? code : 'failed'}`);
    this.name = 'PlasmoControlError';
  }
}
const fail = code => { throw new PlasmoControlError(code); };
const redact = error => error instanceof PlasmoControlError ? error : new PlasmoControlError('failed');

function javaAscii(text) {
  const bytes = Buffer.from(text, 'ascii'); const size = Buffer.alloc(2);
  size.writeUInt16BE(bytes.length); return Buffer.concat([size, bytes]);
}
function playerInfo(publicKey) {
  const der = publicKey.export({ type: 'spki', format: 'der' });
  if (!Buffer.isBuffer(der) || der.length < 1 || der.length > 2048) fail('key_generation');
  const length = Buffer.alloc(4); length.writeInt32BE(der.length);
  // Both disabled flags are true. Sending this packet does not initialize
  // upstream Config/audio/encryption handlers, an audio stream or a device.
  return Buffer.concat([Buffer.from([10, 1, 1]), javaAscii('1.21.1'), javaAscii('2.1.17'), length, der]);
}
function connection(bytes, approvedPort) {
  if (bytes.length < 23) fail('connection');
  const size = bytes.readUInt16BE(17);
  if (size > 255 || bytes.length !== 23 + size) fail('connection');
  // The sole supported Java modified-UTF string is exact ASCII loopback;
  // NUL, aliases, DNS, wildcard fallback and encoded lookalikes are refused.
  if (!bytes.subarray(19, 19 + size).equals(Buffer.from('127.0.0.1', 'ascii'))
    || bytes.readInt32BE(19 + size) !== approvedPort) fail('announcement');
  if (bytes.subarray(1, 17).equals(Buffer.alloc(16))) fail('connection');
  return Buffer.from(bytes.subarray(1, 17));
}
function ping(secret, initial, approvedPort) {
  const bytes = Buffer.alloc(initial ? 50 : 37);
  bytes.writeUInt32BE(MAGIC); bytes[4] = 1; secret.copy(bytes, 5);
  const now = BigInt(Date.now()); bytes.writeBigInt64BE(now, 21); bytes.writeBigInt64BE(now, 29);
  if (initial) { bytes.writeUInt16BE(9, 37); bytes.write('127.0.0.1', 39, 'ascii'); bytes.writeUInt16BE(approvedPort, 48); }
  return bytes;
}
function checkPing(bytes, secret) {
  // Server keepalive has no optional address. Do not accept our own initial
  // Ping, audio, trailing bytes, or call an unauthenticated timestamp an RTT.
  if (bytes.length !== 37 || bytes.readUInt32BE(0) !== MAGIC || bytes[4] !== 1) fail('datagram');
  if (!crypto.timingSafeEqual(bytes.subarray(5, 21), secret)) fail('authentication');
  if (bytes.readBigInt64BE(21) < 0n || bytes.readBigInt64BE(29) < 0n) fail('datagram');
}

/**
 * Install immediately after creating the synthetic AuroraLabA Minecraft Client.
 * Wait for PLAY and the official PlayerInfoRequest (ID2), then answer with a
 * new RSA2048 public key. The private KeyObject is never exported or used to
 * decrypt Config/audio keys. Only Connection and server Pings are parsed.
 * Config/player-list payloads are counted and bounded, not decoded or retained.
 * The enclosing worker must disable debug/packet logs and supervise Minecraft.
 * Success means control traffic plus socket closure, not audio, accounts,
 * confidentiality, anti-replay protection, or proof of an Aurora bridge.
 * Native RSA generation cannot be cancelled: cleanup awaits its callback with
 * late writes disabled, so the caller's process deadline remains the hard cap.
 * JS, parser buffers and native KeyObjects offer no complete secure-erasure
 * guarantee; only our credential/send-buffer copies are explicitly cleared.
 * Opt-in keepalive observes a fixed 35s window from the first valid Ping.
 * Callback-confirmed replies mean local UDP sends, not server delivery. The
 * 5s gap includes initial UDP wait and final silence; timestamps in packets
 * are not used as clocks. Exact duplicate refusal is only a bounded local
 * oracle, can reject a legitimate duplicate and is not anti-replay security.
 */
export async function probePlasmoControl(client, options = {}) {
  if (!options || typeof options !== 'object' || Array.isArray(options)
    || Object.keys(options).some(key => !['approvedPort', 'signal', 'timeoutMs', 'keepalive'].includes(key))) fail('arguments');
  const { approvedPort, signal, timeoutMs = 10000, keepalive = false } = options;
  if (!client || typeof client !== 'object' || !['handshaking', 'login', 'configuration', 'play'].includes(client.state)
    || typeof client.on !== 'function' || typeof client.off !== 'function' || typeof client.write !== 'function'
    || approvedPort !== 35554 || !Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 10000
    || typeof keepalive !== 'boolean' || (signal !== undefined && !(signal instanceof AbortSignal))) fail('arguments');
  if (process.env.DEBUG || process.env.NODE_OPTIONS || globalThis.debugNMP) fail('unsafe_debug_environment');
  if (signal?.aborted) fail('cancelled');
  if (activeClients.has(client)) fail('already_active');
  activeClients.add(client);
  const started = performance.now();
  if (keepalive && !Number.isFinite(started)) { activeClients.delete(client); fail('clock'); }
  const counts = { payloadsReceived: 0, payloadBytesReceived: 0, playerInfoRequests: 0, playerInfoResponses: 0,
    datagramsSent: 0, datagramsReceived: 0, bytesSent: 0, bytesReceived: 0 };
  let phase = 'play'; let minecraftSocket; let udp; let secret; let keyWork;
  let finished = false; let socketClosed = false; let initialAttempts = 0; let pendingSends = 0;
  let responseSent = false; let deadline; let retry; let failure;
  let totalDeadline; let gapTimer; let observationTimer; let handshakeComplete = false;
  let lastClock = started; let udpStarted; let firstPing; let lastPing; let maxGap = 0;
  let responsesCompleted = 0; let observationElapsed = 0;
  const seenPings = new Set();
  const pendingBuffers = new Set();
  let resolveWork; let rejectWork;
  const work = new Promise((resolve, reject) => { resolveWork = resolve; rejectWork = reject; });
  const finish = error => {
    if (finished) return;
    finished = true; clearTimeout(deadline); clearInterval(retry);
    clearTimeout(totalDeadline); clearTimeout(gapTimer); clearTimeout(observationTimer);
    if (error) rejectWork(redact(error)); else resolveWork();
  };
  const reject = code => finish(new PlasmoControlError(code));
  const aborted = () => reject('cancelled');
  const clientFailed = () => reject('client_failed');
  const clientEnded = () => reject('client_closed');
  const socketFailed = () => reject('socket_failed');
  const socketEnded = () => { socketClosed = true; if (!finished) reject('socket_closed'); };
  const clock = () => {
    const now = performance.now();
    if (!Number.isFinite(now) || now < lastClock) fail('clock');
    lastClock = now; return now;
  };
  const time = (allowWindowEnd = false) => {
    if (!keepalive) return undefined;
    const now = clock();
    if (now - started >= TOTAL_MS || (!handshakeComplete && now - started >= timeoutMs)) fail('deadline');
    if (udpStarted !== undefined && now - (lastPing ?? udpStarted) > MAX_GAP_MS) fail('keepalive_gap');
    // Traffic at/after the fixed end cannot extend or renew the window even
    // when the timer has not yet had an event-loop turn.
    if (!allowWindowEnd && firstPing !== undefined && now >= firstPing + OBSERVATION_MS) fail('deadline');
    return now;
  };
  const currentClient = (allowWindowEnd = false) => {
    time(allowWindowEnd);
    if (client.uuid !== EXPECTED_UUID || client.username !== PLAY_NAMES[0]) fail('player_mismatch');
    if (client.state !== 'play' || !minecraftSocket || client.socket !== minecraftSocket
      || minecraftSocket.destroyed || minecraftSocket.closed || client.protocolRejected) fail('client_changed');
  };
  const write = bytes => { currentClient(); client.write('custom_payload', { channel: CHANNEL, data: bytes }); };
  const watchGap = () => {
    clearTimeout(gapTimer);
    const check = () => {
      if (finished) return;
      try {
        currentClient(true); const now = clock();
        if (firstPing !== undefined && now >= firstPing + OBSERVATION_MS) { finishObservation(); return; }
        gapTimer = setTimeout(check, Math.max(1, MAX_GAP_MS - (now - (lastPing ?? udpStarted))));
      } catch (error) { finish(error); }
    };
    gapTimer = setTimeout(check, MAX_GAP_MS);
  };
  const finishObservation = () => {
    if (finished) return;
    try {
      currentClient(true); const now = time(true);
      if (now < firstPing + OBSERVATION_MS) {
        observationTimer = setTimeout(finishObservation, firstPing + OBSERVATION_MS - now); return;
      }
      const suffix = now - lastPing;
      maxGap = Math.max(maxGap, suffix);
      if (pendingSends !== 0) fail('send_queue');
      if (!handshakeComplete || counts.datagramsReceived < 3 || lastPing - firstPing < MIN_SPAN_MS
        || responsesCompleted !== counts.datagramsReceived) fail('keepalive_evidence');
      observationElapsed = now - firstPing; finish();
    } catch (error) { finish(error); }
  };
  const maybeComplete = () => {
    if (phase === 'answering' && responseSent && pendingSends === 0) {
      try {
        currentClient();
        if (keepalive) { handshakeComplete = true; clearTimeout(deadline); phase = 'observing'; }
        else finish();
      } catch (error) { finish(error); }
    }
  };
  const send = initial => {
    if (finished) return;
    let bytes;
    try {
      currentClient();
      if (pendingSends >= 4) fail('send_queue');
      bytes = ping(secret, initial, approvedPort);
      if (++counts.datagramsSent > (keepalive ? 96 : 16)
        || (counts.bytesSent += bytes.length) > (keepalive ? 16384 : MAX_UDP_BYTES)) fail('limits');
      pendingSends++; pendingBuffers.add(bytes);
      udp.send(bytes, error => {
        bytes.fill(0); pendingBuffers.delete(bytes); pendingSends--;
        if (finished) return;
        if (error) { reject('socket_failed'); return; }
        try {
          currentClient();
          if (!initial) { responseSent = true; responsesCompleted++; }
          maybeComplete();
        } catch (error) { finish(error); }
      });
    } catch (error) { bytes?.fill(0); finish(error); }
  };
  const sendInitial = () => { if (phase === 'ping' && initialAttempts < 3) { initialAttempts++; send(true); } };
  const received = (bytes, remote) => {
    if (finished) return;
    try {
      currentClient();
      if (remote?.address !== '127.0.0.1' || remote?.port !== approvedPort || remote?.family !== 'IPv4') fail('datagram_address');
      if (!Buffer.isBuffer(bytes) || bytes.length > MAX_DATAGRAM) fail('datagram');
      if (++counts.datagramsReceived > (keepalive ? 96 : MAX_DATAGRAMS)
        || (counts.bytesReceived += bytes.length) > (keepalive ? 16384 : MAX_UDP_BYTES)) fail('limits');
      if (phase !== 'ping' && !(keepalive && ['answering', 'observing'].includes(phase))) fail('unexpected_packet');
      checkPing(bytes, secret);
      if (keepalive) {
        const digest = crypto.createHash('sha256').update(bytes).digest('hex');
        if (seenPings.has(digest)) fail('duplicate_ping');
        seenPings.add(digest);
        const now = time(); maxGap = Math.max(maxGap, now - (lastPing ?? udpStarted)); lastPing = now;
        if (firstPing === undefined) {
          firstPing = now; observationTimer = setTimeout(finishObservation, OBSERVATION_MS);
        }
        watchGap();
      }
      if (phase === 'ping') phase = 'answering';
      clearInterval(retry); send(false);
    } catch (error) { finish(error); }
  };
  const openUdp = () => {
    currentClient(); phase = 'binding'; udp = dgram.createSocket('udp4');
    udp.on('error', socketFailed); udp.on('close', socketEnded); udp.on('message', received);
    udp.bind(0, '127.0.0.1', () => {
      if (finished) return;
      try {
        currentClient();
        udp.connect(approvedPort, '127.0.0.1', error => {
          if (finished) return;
          if (error) { reject('socket_failed'); return; }
          try {
            currentClient(); phase = 'ping';
            if (keepalive) { udpStarted = clock(); watchGap(); }
            retry = setInterval(sendInitial, 1000); sendInitial();
          }
          catch (error) { finish(error); }
        });
      } catch { reject('socket_failed'); }
    });
  };
  const generate = () => {
    phase = 'key';
    // Start via a resolved promise so keyWork is assigned before a callback,
    // including synchronous mock callbacks or exceptions in native startup.
    keyWork = Promise.resolve().then(() => new Promise(resolve => {
      if (finished) { resolve(); return; }
      try {
        crypto.generateKeyPair('rsa', { modulusLength: 2048, publicExponent: 65537 }, (error, publicKey) => {
          try {
            if (finished) return;
            if (error) { reject('key_generation'); return; }
            currentClient(); const bytes = playerInfo(publicKey);
            phase = 'connection'; counts.playerInfoResponses++; write(bytes);
          } catch (error) { finish(error); }
          finally { resolve(); }
        });
      } catch { reject('key_generation'); resolve(); }
    }));
  };
  const payload = packet => {
    if (finished || packet?.channel !== CHANNEL) return;
    try {
      currentClient();
      const bytes = packet.data;
      if (!Buffer.isBuffer(bytes) || bytes.length < 1 || bytes.length > MAX_PAYLOAD) fail('limits');
      if (++counts.payloadsReceived > MAX_PAYLOADS || (counts.payloadBytesReceived += bytes.length) > MAX_PAYLOAD_BYTES) fail('limits');
      if (bytes[0] === 2) {
        if (bytes.length !== 1) fail('request');
        if (++counts.playerInfoRequests > 8) fail('limits');
        if (phase === 'request') generate();
        // Scheduler retries can overlap RSA or TCP delivery. One response/key
        // generation only; repeat requests never issue another credential.
      } else if (bytes[0] === 1) {
        if (phase !== 'connection') fail('unexpected_packet');
        secret = connection(bytes, approvedPort); openUdp();
      } else if (![3, 7, 8].includes(bytes[0]) || !secret) fail('unexpected_packet');
    } catch (error) { finish(error); }
  };
  const stateChanged = next => {
    if (finished) return;
    try {
      time();
      if (phase !== 'play') { if (next !== 'play') fail('client_changed'); currentClient(); return; }
      if (next !== 'play') return;
      minecraftSocket = client.socket; currentClient(); phase = 'request';
      client.write('custom_payload', { channel: 'minecraft:register', data: Buffer.from(CHANNEL, 'ascii') });
    } catch (error) { finish(error); }
  };
  try {
    // All listeners are installed synchronously, before the first await.
    client.on('custom_payload', payload); client.on('state', stateChanged);
    client.on('error', clientFailed); client.on('end', clientEnded);
    signal?.addEventListener('abort', aborted, { once: true });
    deadline = setTimeout(() => reject('deadline'), timeoutMs);
    if (keepalive) totalDeadline = setTimeout(() => reject('deadline'), TOTAL_MS);
    if (signal?.aborted) aborted();
    if (!finished && client.state === 'play') stateChanged('play');
    await work;
  } catch (error) { failure = redact(error); finish(failure); await work.catch(() => {}); }
  finally {
    finished = true; clearTimeout(deadline); clearInterval(retry);
    clearTimeout(totalDeadline); clearTimeout(gapTimer); clearTimeout(observationTimer);
    signal?.removeEventListener('abort', aborted);
    client.off('custom_payload', payload); client.off('state', stateChanged);
    client.off('error', clientFailed); client.off('end', clientEnded);
    // Do not leave a late crypto callback writing after the probe returns.
    await keyWork;
    if (udp && !socketClosed) {
      let closeStarted;
      if (keepalive) {
        try { closeStarted = clock(); } catch (error) { failure ??= redact(error); }
      }
      try {
        await new Promise((resolve, rejectClose) => {
          const timer = setTimeout(() => rejectClose(new PlasmoControlError('cleanup_failed')), 250);
          try {
            udp.close(() => {
              clearTimeout(timer); socketClosed = true;
              try {
                if (keepalive && clock() - closeStarted > 250) fail('cleanup_failed');
                resolve();
              } catch (error) { rejectClose(redact(error)); }
            });
          } catch (error) {
            clearTimeout(timer);
            if (error?.code === 'ERR_SOCKET_DGRAM_NOT_RUNNING') { socketClosed = true; resolve(); }
            else rejectClose(new PlasmoControlError('cleanup_failed'));
          }
        });
      } catch { failure ??= new PlasmoControlError('cleanup_failed'); }
    }
    secret?.fill(0); secret = undefined;
    for (const bytes of pendingBuffers) bytes.fill(0);
    pendingBuffers.clear();
    seenPings.clear();
    if (socketClosed) { udp.off('error', socketFailed); udp.off('close', socketEnded); udp.off('message', received); }
    activeClients.delete(client);
  }
  if (!failure && keepalive) {
    try { if (clock() - started > TOTAL_MS + 250) fail('deadline'); }
    catch (error) { failure = redact(error); }
  }
  if (failure) throw failure;
  return Object.freeze({ mod: 'plasmo_voice', version: '2.1.17', announcementMatched: true,
    playerInfoRequested: true, playerInfoSent: true, serverPingReceived: true, keepAliveResponseSent: true,
    authentication: 'session_secret_only_no_aead', ...counts, elapsedMs: Math.round(performance.now() - started),
    ...(keepalive ? { keepalive: { observationWindowMs: OBSERVATION_MS, observationElapsedMs: Math.round(observationElapsed),
      serverPingsReceived: counts.datagramsReceived, responsesCompleted, pendingSends,
      sampleSpanMs: Math.round(lastPing - firstPing), maxGapMs: Math.round(maxGap) } } : {}),
    udpSocketClosed: true, audio: 'not_tested', accountAuthentication: 'not_tested', runtimeHomologated: false });
}
