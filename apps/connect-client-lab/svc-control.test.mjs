import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import dgram from 'node:dgram';
import { createCipheriv, createDecipheriv, randomBytes } from 'node:crypto';
import { inspect } from 'node:util';
import { performance } from 'node:perf_hooks';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { probeSvcControl, SvcControlError } from './svc-control.mjs';

// All socket operations below are in-memory fixtures. No UDP/TCP listener, mod,
// JVM, auth account, audio device or remote connection is created by this suite.
const PORT = 35554;
const UUID = '01234567-89ab-3def-8123-456789abcdef';
const uuid = Buffer.from(UUID.replaceAll('-', ''), 'hex');

function secretPacket(key, changes = {}) {
  const address = Buffer.isBuffer(changes.address) ? changes.address : Buffer.from(changes.address ?? `127.0.0.1:${PORT}`);
  const fixed = Buffer.alloc(54);
  key.copy(fixed, 0); fixed.writeInt32BE(43002, 16); (changes.uuid ?? uuid).copy(fixed, 20);
  fixed[36] = 0; fixed.writeInt32BE(1275, 37); fixed.writeDoubleBE(48, 41);
  fixed.writeInt32BE(1000, 49); fixed[53] = 1;
  const length = changes.length ?? Buffer.from([address.length]);
  return Buffer.concat([fixed, length, address, Buffer.from([1])]);
}

// Independent wire fixture: header and crypto are assembled directly from the
// pinned Java layouts, not by calling/exporting the implementation's codec.
function reply(key, type, tail = Buffer.alloc(0)) {
  const iv = randomBytes(12); const cipher = createCipheriv('aes-128-gcm', key, iv);
  const encrypted = Buffer.concat([cipher.update(Buffer.concat([Buffer.from([type]), tail])), cipher.final()]);
  const body = Buffer.concat([iv, encrypted, cipher.getAuthTag()]);
  assert.ok(body.length < 128);
  return Buffer.concat([Buffer.from([255, body.length]), body]);
}

function decodeClient(key, bytes) {
  assert.equal(bytes[0], 255); assert.deepEqual(bytes.subarray(1, 17), uuid);
  assert.equal(bytes.length, 18 + bytes[17]);
  const iv = bytes.subarray(18, 30);
  const decipher = createDecipheriv('aes-128-gcm', key, iv);
  decipher.setAuthTag(bytes.subarray(-16));
  return Buffer.concat([decipher.update(bytes.subarray(30, -16)), decipher.final()]);
}

class FakeSocket extends EventEmitter {
  constructor(key, settings) { super(); this.key = key; this.settings = settings; this.sent = []; this.closed = false; }
  bind(port, address, callback) {
    this.bound = { port, address };
    if (this.settings.bindThrows) throw new Error('UNSAFE_BIND_DETAIL');
    if (!this.settings.holdBind) queueMicrotask(callback);
  }
  connect(port, address, callback) {
    this.remote = { port, address };
    if (this.settings.connectThrows) throw new Error('UNSAFE_CONNECT_DETAIL');
    this.pendingConnect = callback;
    if (!this.settings.holdConnect) queueMicrotask(() => callback(this.settings.connectError ? new Error('UNSAFE_CONNECT_DETAIL') : undefined));
  }
  send(bytes, callback) {
    if (this.settings.sendThrows) throw new Error('UNSAFE_SEND_DETAIL');
    const saved = Buffer.from(bytes); this.sent.push(saved);
    queueMicrotask(() => {
      const body = decodeClient(this.key, saved);
      const held = this.settings.holdSend || this.settings.holdType === body[0];
      if (held) (this.pendingCallbacks ??= []).push(callback);
      else callback(this.settings.sendError || this.settings.errorType === body[0] ? new Error('UNSAFE_SEND_DETAIL') : undefined);
      if (this.closed) { body.fill(0); return; }
      if (this.settings.onSend) this.settings.onSend(this, body);
      else if (body[0] === 5) {
        assert.equal(body.length, 33); assert.deepEqual(body.subarray(1, 17), uuid);
        assert.deepEqual(body.subarray(17), this.key); this.receive(reply(this.key, 6));
      } else if (body[0] === 9) {
        assert.equal(body.length, 1); this.receive(reply(this.key, 10));
      } else assert.fail('unexpected fixture request');
      body.fill(0);
    });
  }
  receive(bytes, remote = { family: 'IPv4', address: '127.0.0.1', port: PORT }) { this.emit('message', bytes, remote); }
  close(callback) {
    if (this.settings.closeStalls) return;
    this.closed = true;
    queueMicrotask(() => { this.settings.onClose?.(); this.emit('close'); callback(); });
  }
}

function fixture(t, settings = {}) {
  const key = randomBytes(16); const sockets = []; const writes = [];
  t.mock.method(dgram, 'createSocket', type => {
    assert.equal(type, 'udp4');
    if (settings.createThrows) throw new Error('UNSAFE_CREATE_DETAIL');
    const socket = new FakeSocket(key, settings); sockets.push(socket); return socket;
  });
  const client = new EventEmitter(); client.state = 'play'; client.uuid = UUID;
  client.write = (name, packet) => {
    if (settings.writeThrows) throw new Error('UNSAFE_WRITE_DETAIL');
    assert.equal(name, 'custom_payload'); writes.push({ channel: packet.channel, data: Buffer.from(packet.data) });
    if (packet.channel === 'voicechat:request_secret' && !settings.noSecret) {
      queueMicrotask(() => client.emit('custom_payload', { channel: 'voicechat:secret',
        data: settings.payload ? settings.payload(key) : secretPacket(key) }));
    }
  };
  const assertClean = () => {
    assert.equal(client.listenerCount('custom_payload'), 0);
    assert.equal(client.listenerCount('error'), 0); assert.equal(client.listenerCount('end'), 0);
    for (const socket of sockets) {
      if (!settings.closeStalls) {
        assert.equal(socket.closed, true); assert.equal(socket.listenerCount('message'), 0);
        assert.equal(socket.listenerCount('error'), 0); assert.equal(socket.listenerCount('close'), 0);
      }
    }
  };
  t.after(() => { assertClean(); key.fill(0); });
  return { client, key, sockets, writes, assertClean };
}

async function rejected(promise, code, key) {
  await assert.rejects(promise, error => {
    assert.ok(error instanceof SvcControlError); assert.equal(error.message, `svc_control_${code}`);
    const text = `${inspect(error)}${JSON.stringify(error)}`;
    assert.doesNotMatch(text, /UNSAFE_|voicechat:|01234567|127\.0\.0\.1|35554|43002/);
    if (key) assert.ok(!text.includes(key.toString('hex')));
    assert.equal(error.cause, undefined); return true;
  });
}

test('bounded control handshake validates announcement and both independent AES-GCM acknowledgments', async t => {
  const f = fixture(t); const result = await probeSvcControl(f.client, { approvedPort: PORT });
  assert.deepEqual(f.writes, [
    { channel: 'minecraft:register', data: Buffer.from('voicechat:secret') },
    { channel: 'voicechat:request_secret', data: Buffer.from([0, 0, 0, 20]) },
  ]);
  assert.equal(f.sockets.length, 1); assert.deepEqual(f.sockets[0].bound, { port: 0, address: '127.0.0.1' });
  assert.deepEqual(f.sockets[0].remote, { port: PORT, address: '127.0.0.1' });
  assert.equal(f.sockets[0].sent[0].length, 79); assert.equal(f.sockets[0].sent[1].length, 47);
  assert.equal(result.authenticationAck, true); assert.equal(result.connectionCheckAck, true);
  assert.equal(result.announcementMatched, true); assert.equal(result.compatibility, 20);
  assert.equal(result.datagramsSent, 2); assert.equal(result.datagramsReceived, 2);
  assert.equal(result.bytesSent, 126); assert.equal(result.bytesReceived, 62); assert.equal(result.udpSocketClosed, true);
  assert.equal(result.audio, 'not_tested'); assert.equal(result.accountAuthentication, 'not_tested');
  assert.equal(result.runtimeHomologated, false); assert.ok(Object.isFrozen(result));
  assert.doesNotMatch(JSON.stringify(result), /secret|uuid|address|35554|43002|127\.0\.0\.1/i);
  assert.ok(!JSON.stringify(result).includes(f.key.toString('hex'))); f.assertClean();
});

test('arguments, client phase, UUID and cancellation are rejected before any request or socket', async t => {
  const f = fixture(t);
  for (const options of [null, [], {}, { approvedPort: 0 }, { approvedPort: 65536 }, { approvedPort: PORT, timeoutMs: 10001 },
    { approvedPort: PORT, timeoutMs: 0 }, { approvedPort: PORT, signal: {} }, { approvedPort: PORT, address: '127.0.0.1' }]) {
    await rejected(probeSvcControl(f.client, options), 'arguments');
  }
  f.client.state = 'login'; await rejected(probeSvcControl(f.client, { approvedPort: PORT }), 'arguments');
  f.client.state = 'play'; f.client.uuid = 'invalid'; await rejected(probeSvcControl(f.client, { approvedPort: PORT }), 'arguments');
  f.client.uuid = UUID; const cancelled = AbortSignal.abort('UNSAFE_CANCEL_DETAIL');
  await rejected(probeSvcControl(f.client, { approvedPort: PORT, signal: cancelled }), 'cancelled');
  assert.equal(f.writes.length, 0); assert.equal(f.sockets.length, 0);
});

test('debug environment is refused without exposing or sending a secret', async t => {
  const f = fixture(t); const previous = process.env.DEBUG;
  try {
    process.env.DEBUG = 'minecraft-protocol';
    await rejected(probeSvcControl(f.client, { approvedPort: PORT }), 'unsafe_debug_environment');
  } finally { if (previous === undefined) delete process.env.DEBUG; else process.env.DEBUG = previous; }
  assert.equal(f.writes.length, 0); assert.equal(f.sockets.length, 0);
});

test('foreign, empty, deceptive or mismatched announcements never open UDP', async t => {
  for (const address of ['', 'localhost:35554', '127.0.0.1:35555', '0.0.0.0:35554', '[::1]:35554',
    '127.0.0.1:35554\0foreign', '127.0.0.1:35554 ', '127.0.0.1:035554', 'example.invalid:35554']) {
    await t.test('refused announcement', async t => {
      const f = fixture(t, { payload: key => secretPacket(key, { address }) });
      await rejected(probeSvcControl(f.client, { approvedPort: PORT }), 'announcement', f.key);
      assert.equal(f.sockets.length, 0);
    });
  }
});

test('player identity, malformed secret framing, invalid configuration and UTF8 fail before UDP', async t => {
  const cases = [
    ['player_mismatch', key => secretPacket(key, { uuid: Buffer.alloc(16, 1) })],
    ['secret_packet', () => Buffer.alloc(257)], ['secret_packet', () => Buffer.alloc(55)],
    ['secret_packet', key => secretPacket(key).subarray(0, -1)],
    ['secret_packet', key => Buffer.concat([secretPacket(key), Buffer.from([0])])],
    ['secret_packet', key => secretPacket(key, { address: Buffer.from([0xff]) })],
    ['secret_packet', key => secretPacket(key, { length: Buffer.from([0xff, 0xff, 0xff, 0x7f]) })],
    ['secret_packet', key => secretPacket(key, { length: Buffer.from([0x80 | 15, 0]) })],
    ...[16, 36, 37, 49, 53].map(offset => ['secret_packet', key => { const bytes = secretPacket(key); bytes[offset] = 255; return bytes; }]),
    ['secret_packet', key => { const bytes = secretPacket(key); bytes.writeDoubleBE(NaN, 41); return bytes; }],
    ['secret_packet', key => { const bytes = secretPacket(key); bytes[bytes.length - 1] = 2; return bytes; }],
  ];
  for (const [code, payload] of cases) await t.test('invalid payload', async t => {
    const f = fixture(t, { payload });
    await rejected(probeSvcControl(f.client, { approvedPort: PORT }), code, f.key);
    assert.equal(f.sockets.length, 0);
  });
});

test('wrong AES key and altered tag never become an authentication acknowledgment', async t => {
  for (const wrongKey of [true, false]) await t.test('authentication refused', async t => {
    const f = fixture(t, { onSend(socket) {
      const packet = reply(wrongKey ? randomBytes(16) : socket.key, 6);
      if (!wrongKey) packet[packet.length - 1] ^= 1;
      socket.receive(packet);
    } });
    await rejected(probeSvcControl(f.client, { approvedPort: PORT }), 'authentication', f.key);
    assert.equal(f.sockets[0].sent.length, 1);
  });
});

test('foreign source addresses and ports fail even before datagram decryption', async t => {
  for (const remote of [undefined, { address: '127.0.0.2', port: PORT, family: 'IPv4' },
    { address: '127.0.0.1', port: PORT + 1, family: 'IPv4' }, { address: '127.0.0.1', port: PORT, family: 'IPv6' },
    { address: '::ffff:127.0.0.1', port: PORT, family: 'IPv6' }]) {
    await t.test('source refused', async t => {
      const f = fixture(t, { onSend(socket) { socket.emit('message', Buffer.alloc(0), remote); } });
      await rejected(probeSvcControl(f.client, { approvedPort: PORT }), 'datagram_address', f.key);
    });
  }
});

test('malformed, oversized, noncanonical and truncated datagrams are refused in full', async t => {
  const cases = [() => Buffer.alloc(513), () => Buffer.alloc(0), key => reply(key, 6).subarray(0, -1),
    key => Buffer.concat([reply(key, 6), Buffer.from([0])]), key => { const packet = reply(key, 6); packet[0] = 0; return packet; },
    key => { const packet = reply(key, 6); return Buffer.concat([Buffer.from([255, 0x9d, 0]), packet.subarray(2)]); },
    () => Buffer.concat([Buffer.from([255, 255, 255, 255, 127]), Buffer.alloc(29)])];
  for (const make of cases) await t.test('malformed wire', async t => {
    const f = fixture(t, { onSend(socket) { socket.receive(make(socket.key)); } });
    await rejected(probeSvcControl(f.client, { approvedPort: PORT }), 'datagram', f.key);
  });
});

test('authenticated audio/unknown packets, trailing plaintext, or out-of-order check acknowledgment do not pass', async t => {
  for (const [type, tail] of [[1, Buffer.alloc(0)], [2, Buffer.alloc(0)], [7, Buffer.alloc(0)],
    [10, Buffer.alloc(0)], [8, Buffer.alloc(0)], [6, Buffer.from([0])]]) {
    await t.test('unexpected plaintext', async t => {
      const f = fixture(t, { onSend(socket) { socket.receive(reply(socket.key, type, tail)); } });
      await rejected(probeSvcControl(f.client, { approvedPort: PORT }), 'unexpected_packet', f.key);
    });
  }
});

test('duplicate authentication acknowledgment cannot substitute for a connection-check acknowledgment', async t => {
  const f = fixture(t, { onSend(socket) { socket.receive(reply(socket.key, 6)); } });
  await rejected(probeSvcControl(f.client, { approvedPort: PORT, timeoutMs: 25 }), 'deadline', f.key);
  assert.equal(f.sockets[0].sent.length, 2);
});

test('keepalive exchange in the checking phase is bounded and never transmits an audio packet', async t => {
  const f = fixture(t, { onSend(socket, body) {
    if (body[0] === 5) socket.receive(reply(socket.key, 6));
    else if (body[0] === 9) socket.receive(reply(socket.key, 8));
    else if (body[0] === 8) socket.receive(reply(socket.key, 10));
    else assert.fail('audio sent');
  } });
  const result = await probeSvcControl(f.client, { approvedPort: PORT });
  assert.equal(result.datagramsSent, 3); assert.equal(result.datagramsReceived, 3);
});

test('inbound flood and pending sends are bounded independently of the deadline', async t => {
  for (const holdSend of [false, true]) await t.test('bounded flood', async t => {
    const f = fixture(t, { holdSend, onSend(socket, body) {
      if (body[0] !== 5) return;
      socket.receive(reply(socket.key, 6));
      for (let index = 0; index < 40; index++) socket.receive(reply(socket.key, holdSend ? 8 : 6));
    } });
    await rejected(probeSvcControl(f.client, { approvedPort: PORT }), holdSend ? 'send_queue' : 'limits', f.key);
    assert.ok(f.sockets[0].sent.length <= 4);
  });
});

test('duplicate secret payload fails without creating a second association', async t => {
  const f = fixture(t, { holdBind: true });
  const work = probeSvcControl(f.client, { approvedPort: PORT });
  await new Promise(resolve => queueMicrotask(resolve));
  f.client.emit('custom_payload', { channel: 'voicechat:secret', data: secretPacket(f.key) });
  await rejected(work, 'duplicate_secret', f.key); assert.equal(f.sockets.length, 1);
});

test('deadline waiting for Minecraft or UDP releases all owned resources', async t => {
  for (const noSecret of [true, false]) await t.test('deadline', async t => {
    const f = fixture(t, { noSecret, onSend() {} });
    await rejected(probeSvcControl(f.client, { approvedPort: PORT, timeoutMs: 20 }), 'deadline', f.key);
  });
});

test('handshake retransmissions stop after three sends and use fresh GCM IVs', async t => {
  const f = fixture(t, { onSend() {} });
  t.mock.timers.enable({ apis: ['setInterval', 'setTimeout'] });
  const work = probeSvcControl(f.client, { approvedPort: PORT, timeoutMs: 5000 });
  // Complete fake request, bind, connect and initial send callback microtasks.
  for (let i = 0; i < 6; i++) await Promise.resolve();
  assert.equal(f.sockets[0].sent.length, 1);
  for (let i = 0; i < 4; i++) {
    t.mock.timers.tick(1000); await Promise.resolve();
  }
  assert.equal(f.sockets[0].sent.length, 3);
  assert.equal(new Set(f.sockets[0].sent.map(bytes => bytes.subarray(18, 30).toString('hex'))).size, 3);
  t.mock.timers.tick(1000);
  await rejected(work, 'deadline', f.key);
});

test('sequential keepalive requests cannot evade the total send budget', async t => {
  const f = fixture(t, { onSend(socket, body) {
    socket.receive(reply(socket.key, body[0] === 5 ? 6 : 8));
  } });
  await rejected(probeSvcControl(f.client, { approvedPort: PORT }), 'limits', f.key);
  assert.equal(f.sockets[0].sent.length, 16);
});

test('abort during bind or secret wait is redacted and releases resources before rejecting', async t => {
  for (const noSecret of [true, false]) await t.test('cancelled', async t => {
    const f = fixture(t, { noSecret, holdBind: true }); const controller = new AbortController();
    const work = probeSvcControl(f.client, { approvedPort: PORT, signal: controller.signal });
    await new Promise(resolve => queueMicrotask(resolve)); controller.abort('UNSAFE_CANCEL_DETAIL');
    await rejected(work, 'cancelled', f.key); f.assertClean();
  });
});

test('an asynchronous UDP connect error closes before rejection and sends no datagram', async t => {
  const f = fixture(t, { connectError: true });
  await rejected(probeSvcControl(f.client, { approvedPort: PORT }), 'socket_failed', f.key);
  assert.equal(f.sockets.length, 1); assert.equal(f.sockets[0].sent.length, 0); f.assertClean();
});

test('abort with UDP connect pending prevents late success or error callbacks from restarting work', async t => {
  const f = fixture(t, { holdConnect: true }); const controller = new AbortController();
  const work = probeSvcControl(f.client, { approvedPort: PORT, signal: controller.signal });
  for (let index = 0; index < 4; index++) await Promise.resolve();
  const socket = f.sockets[0]; assert.equal(typeof socket.pendingConnect, 'function');
  assert.equal(socket.sent.length, 0); controller.abort('UNSAFE_ABORT_DETAIL');
  await rejected(work, 'cancelled', f.key); f.assertClean();
  socket.pendingConnect(); socket.pendingConnect(new Error('UNSAFE_CONNECT_DETAIL'));
  await Promise.resolve(); assert.equal(socket.sent.length, 0); f.assertClean();
});

test('deadline while UDP connect is pending also closes before a late callback', async t => {
  const f = fixture(t, { holdConnect: true });
  await rejected(probeSvcControl(f.client, { approvedPort: PORT, timeoutMs: 20 }), 'deadline', f.key);
  const socket = f.sockets[0]; assert.equal(socket.sent.length, 0); f.assertClean();
  socket.pendingConnect(); await Promise.resolve(); assert.equal(socket.sent.length, 0);
});

test('a second probe cannot race for the same Minecraft secret', async t => {
  const f = fixture(t, { noSecret: true }); const controller = new AbortController();
  const first = probeSvcControl(f.client, { approvedPort: PORT, signal: controller.signal });
  await rejected(probeSvcControl(f.client, { approvedPort: PORT }), 'already_active', f.key);
  assert.equal(f.writes.length, 2); controller.abort(); await rejected(first, 'cancelled', f.key);
});

test('client and socket failures are redacted, and a successful check cannot hide incomplete cleanup', async t => {
  for (const [settings, code] of [[{ writeThrows: true }, 'failed'], [{ createThrows: true }, 'failed'],
    [{ bindThrows: true }, 'failed'], [{ connectThrows: true }, 'socket_failed'],
    [{ sendThrows: true }, 'failed'], [{ sendError: true }, 'socket_failed'], [{ closeStalls: true }, 'cleanup_failed']]) {
    await t.test('redacted failure', async t => {
      const f = fixture(t, settings);
      await rejected(probeSvcControl(f.client, { approvedPort: PORT }), code, f.key);
    });
  }
  for (const [event, code] of [['error', 'client_failed'], ['end', 'client_closed']]) await t.test('client failure', async t => {
    const f = fixture(t, { noSecret: true }); const work = probeSvcControl(f.client, { approvedPort: PORT });
    f.client.emit(event, new Error('UNSAFE_CLIENT_DETAIL')); await rejected(work, code, f.key);
  });
});

test('unrelated payloads are ignored without retaining or exporting their bytes', async t => {
  const f = fixture(t, { noSecret: true }); const controller = new AbortController();
  const work = probeSvcControl(f.client, { approvedPort: PORT, signal: controller.signal });
  f.client.emit('custom_payload', { channel: 'minecraft:brand', data: Buffer.from('UNSAFE_BRAND_DETAIL') });
  f.client.emit('custom_payload', undefined);
  assert.equal(f.sockets.length, 0); controller.abort(); await rejected(work, 'cancelled', f.key);
});

// Virtual timers plus a mocked process monotonic clock. Neither clock nor
// socket injection exists in the probe API; packets use the independent codec
// above and never contact a listener or a Minecraft/voice server.
async function settle() { for (let i = 0; i < 16; i++) await Promise.resolve(); }
function virtualTime(t) {
  let time = 100_000;
  t.mock.timers.enable({ apis: ['setInterval', 'setTimeout'] });
  t.mock.method(performance, 'now', () => time);
  return {
    async advance(ms) { time += ms; t.mock.timers.tick(ms); await settle(); },
    set(value) { time = value; },
  };
}
async function watching(t, settings = {}) {
  const clock = virtualTime(t);
  const f = fixture(t, { onSend(socket, body) {
    if (body[0] === 5) socket.receive(reply(socket.key, 6));
    else if (body[0] === 9) socket.receive(reply(socket.key, 10));
    else assert.equal(body[0], 8);
  }, ...settings });
  const controller = new AbortController();
  const work = probeSvcControl(f.client, { approvedPort: PORT, keepalive: true, signal: controller.signal });
  work.catch(() => {});
  await settle();
  const socket = f.sockets[0];
  const keep = async () => { socket.receive(reply(f.key, 8)); await settle(); };
  return { ...f, clock, socket, controller, work, keep };
}

test('opt-in keepalive watches exactly 35 seconds, spans beyond UDP idle, and confirms local send callbacks', async t => {
  const f = await watching(t); let complete = false; f.work.then(() => { complete = true; });
  for (let second = 1; second <= 34; second++) {
    await f.clock.advance(1000); await f.keep();
    // Retransmission ACKs are legitimate but cannot count as activity.
    if (second === 15) { f.socket.receive(reply(f.key, 6)); f.socket.receive(reply(f.key, 10)); }
  }
  assert.equal(complete, false); await f.clock.advance(999); assert.equal(complete, false);
  await f.clock.advance(1); const result = await f.work;
  assert.equal(result.keepaliveWindowMs, 35000); assert.equal(result.elapsedMs, 35000);
  assert.equal(result.keepaliveObserved, true); assert.equal(result.keepalivesReceived, 34);
  assert.equal(result.keepaliveResponsesConfirmed, 34); assert.equal(result.keepaliveSpanMs, 33000);
  assert.equal(result.keepaliveMaxGapMs, 1000); assert.equal(result.datagramsSent, 36);
  assert.equal(result.datagramsReceived, 38); assert.equal(result.udpSocketClosed, true);
  assert.doesNotMatch(JSON.stringify(result), /secret|uuid|address|35554|43002|127\.0\.0\.1/i);
  assert.ok(!JSON.stringify(result).includes(f.key.toString('hex'))); f.assertClean();
  await f.clock.advance(60_000); assert.equal(f.socket.sent.length, 36);
});

test('observation begins at first CheckAck, while handshake keeps its own deadline', async t => {
  const f = await watching(t, { onSend(socket, body) {
    if (body[0] === 5) socket.receive(reply(socket.key, 6));
  } });
  await f.clock.advance(4500); f.socket.receive(reply(f.key, 10)); await settle();
  for (let second = 1; second <= 34; second++) { await f.clock.advance(1000); await f.keep(); }
  await f.clock.advance(1000); const result = await f.work;
  assert.equal(result.elapsedMs, 39500); assert.equal(result.keepaliveWindowMs, 35000);
});

test('keepalive interval and option are restricted before opening UDP', async t => {
  for (const interval of [1001, 2000, 2147483647]) await t.test('incompatible announced interval', async t => {
    const f = fixture(t, { payload: key => { const bytes = secretPacket(key); bytes.writeInt32BE(interval, 49); return bytes; } });
    await rejected(probeSvcControl(f.client, { approvedPort: PORT, keepalive: true }), 'keepalive_interval', f.key);
    assert.equal(f.sockets.length, 0);
  });
  const f = fixture(t);
  for (const keepalive of [null, 1, 'true', {}]) await rejected(probeSvcControl(f.client, { approvedPort: PORT, keepalive }), 'arguments');
  assert.equal(f.writes.length, 0);
});

test('silence and duplicate acknowledgments cannot renew the initial keepalive gap', async t => {
  for (const duplicateAck of [false, true]) await t.test('gap expires', async t => {
    const f = await watching(t); await f.clock.advance(4000);
    if (duplicateAck) { f.socket.receive(reply(f.key, 6)); f.socket.receive(reply(f.key, 10)); }
    await f.clock.advance(1001); await rejected(f.work, 'keepalive_gap', f.key);
    assert.equal(f.socket.sent.length, 2); f.assertClean();
  });
});

test('later gap and terminal gap cannot be hidden by a new packet or previous success', async t => {
  for (const terminal of [false, true]) await t.test('gap expires', async t => {
    const f = await watching(t);
    const end = terminal ? 29 : 1;
    for (let second = 1; second <= end; second++) { await f.clock.advance(1000); await f.keep(); }
    await f.clock.advance(5001); f.socket.receive(reply(f.key, 8));
    await rejected(f.work, 'keepalive_gap', f.key); f.assertClean();
  });
});

test('exact five-second gaps and thirty-second span pass, one millisecond shorter span fails', async t => {
  for (const last of [35000 - 1, 35000]) await t.test('span boundary', async t => {
    const f = await watching(t);
    // At cutoff no packet is admitted. Start at4s for span30s, at5s for29999ms.
    const first = last === 35000 ? 4000 : 5000;
    await f.clock.advance(first); await f.keep();
    let elapsed = first;
    while (elapsed + 5000 < 34000) { await f.clock.advance(5000); elapsed += 5000; await f.keep(); }
    const final = last === 35000 ? 34000 : 34999;
    await f.clock.advance(final - elapsed); await f.keep();
    await f.clock.advance(35000 - final);
    if (last === 35000) {
      const result = await f.work; assert.equal(result.keepaliveSpanMs, 30000); assert.equal(result.keepaliveMaxGapMs, 5000);
    } else await rejected(f.work, 'keepalive_evidence', f.key);
  });
});

test('authenticated exact keepalive duplication is rejected without claiming protocol freshness', async t => {
  const f = await watching(t); const bytes = reply(f.key, 8);
  await f.clock.advance(1000); f.socket.receive(bytes); await settle();
  await f.clock.advance(1000); f.socket.receive(bytes);
  await rejected(f.work, 'keepalive_replay', f.key); assert.equal(f.socket.sent.length, 3);
});

test('watching flood stays bounded despite valid GCM and callback completions', async t => {
  const f = await watching(t);
  for (let index = 0; index < 100; index++) await f.keep();
  await rejected(f.work, 'limits', f.key); assert.equal(f.socket.sent.length, 96);
});

test('watching pending-send queue and callback errors fail without a successful summary', async t => {
  for (const errorSend of [false, true]) await t.test('send failure', async t => {
    const f = await watching(t, errorSend ? { errorType: 8 } : { holdType: 8 });
    for (let index = 0; index < (errorSend ? 1 : 5); index++) await f.keep();
    await rejected(f.work, errorSend ? 'socket_failed' : 'send_queue', f.key);
    const sent = f.socket.sent.length;
    for (const callback of f.socket.pendingCallbacks ?? []) { callback(); callback(new Error('UNSAFE_LATE_SEND')); }
    await settle(); assert.equal(f.socket.sent.length, sent); f.assertClean();
  });
});

test('unconfirmed response at cutoff fails and a late callback cannot change the result', async t => {
  const f = await watching(t);
  for (let second = 1; second <= 34; second++) {
    await f.clock.advance(1000);
    if (second === 34) f.socket.settings.holdType = 8;
    await f.keep();
  }
  await f.clock.advance(1000); await rejected(f.work, 'send_unconfirmed', f.key);
  for (const callback of f.socket.pendingCallbacks) callback();
  await settle(); f.assertClean();
});

test('cancellation, client closure and invalid monotonic clocks fail during observation', async t => {
  for (const mode of ['cancel', 'end', 'backwards', 'nonfinite']) await t.test('watch interruption', async t => {
    const f = await watching(t); await f.clock.advance(1000); await f.keep();
    if (mode === 'cancel') f.controller.abort('UNSAFE_CANCEL');
    else if (mode === 'end') f.client.emit('end');
    else { f.clock.set(mode === 'backwards' ? 100_999 : NaN); f.socket.receive(reply(f.key, 8)); }
    await rejected(f.work, mode === 'cancel' ? 'cancelled' : mode === 'end' ? 'client_closed' : 'clock', f.key);
    f.assertClean();
  });
});

test('watching still rejects bad authentication and malformed control packets', async t => {
  for (const malformed of [false, true]) await t.test('invalid packet', async t => {
    const f = await watching(t); const bytes = malformed ? reply(f.key, 8, Buffer.from([0])) : reply(randomBytes(16), 8);
    f.socket.receive(bytes); await rejected(f.work, malformed ? 'unexpected_packet' : 'authentication', f.key);
  });
});

test('opt-in does not lengthen the handshake or accept clock/socket injection options', async t => {
  const f = fixture(t, { noSecret: true }); const clock = virtualTime(t);
  const work = probeSvcControl(f.client, { approvedPort: PORT, timeoutMs: 10000, keepalive: true });
  work.catch(() => {}); await clock.advance(10000); await rejected(work, 'deadline', f.key);
  for (const option of ['clock', 'socket', 'windowMs']) {
    await rejected(probeSvcControl(f.client, { approvedPort: PORT, keepalive: true, [option]: 1 }), 'arguments');
  }
});

test('late messages cannot bypass handshake or total deadlines before timer callbacks run', async t => {
  for (const phase of ['secret', 'check', 'watching']) await t.test('absolute deadline', async t => {
    if (phase === 'watching') {
      const f = await watching(t);
      // Deliver enough prior evidence, then simulate a stalled event loop.
      for (let i = 0; i < 34; i++) { await f.clock.advance(1000); await f.keep(); }
      f.clock.set(149750); f.socket.receive(reply(f.key, 8));
      await rejected(f.work, 'deadline', f.key);
    } else {
      const clock = virtualTime(t); const f = fixture(t, { noSecret: phase === 'secret', onSend(socket, body) {
        if (body[0] === 5) socket.receive(reply(socket.key, 6));
      } });
      const work = probeSvcControl(f.client, { approvedPort: PORT, keepalive: true, timeoutMs: 10000 });
      work.catch(() => {}); await settle(); clock.set(110000);
      if (phase === 'secret') f.client.emit('custom_payload', { channel: 'voicechat:secret', data: secretPacket(f.key) });
      else f.sockets[0].receive(reply(f.key, 10));
      await rejected(work, 'deadline', f.key);
      if (phase === 'secret') assert.equal(f.sockets.length, 0);
    }
  });
});

test('CLI refuses unknown or repeated keepalive flags before creating directories or agents', () => {
  for (const args of [['--keepalive'], ['--offline-lab', '--keepalive', '--keepalive'], ['--offline-lab', '--keepalive=35']]) {
    const result = spawnSync(process.execPath, [fileURLToPath(new URL('./svc-smoke.mjs', import.meta.url)), ...args],
      { encoding: 'utf8', timeout: 5000, windowsHide: true, env: { SystemRoot: process.env.SystemRoot } });
    assert.equal(result.status, 1); const report = JSON.parse(result.stdout);
    assert.equal(report.result, 'FAIL'); assert.equal(report.code, 'client_svc_usage');
  }
});

test('clock changes during socket close cannot turn expired cleanup into PASS', async t => {
  for (const clockValue of [NaN, 99_999, 150_000]) await t.test('close clock failure', async t => {
    const f = await watching(t);
    for (let i = 0; i < 34; i++) { await f.clock.advance(1000); await f.keep(); }
    f.socket.settings.onClose = () => f.clock.set(clockValue);
    await f.clock.advance(1000);
    await rejected(f.work, clockValue === 150_000 ? 'cleanup_failed' : 'clock', f.key);
    f.assertClean();
  });
});

test('nonfinite initial monotonic clock refuses before I/O and does not retain the active client', async t => {
  for (const initial of [NaN, Infinity, -Infinity]) await t.test('initial clock invalid', async t => {
    const f = fixture(t); let first = true;
    t.mock.method(performance, 'now', () => { if (first) { first = false; return initial; } return 100_000; });
    await rejected(probeSvcControl(f.client, { approvedPort: PORT, keepalive: true }), 'clock', f.key);
    assert.equal(f.writes.length, 0); assert.equal(f.sockets.length, 0);
    // Reusing the same fixture client proves WeakSet admission was not retained.
    const result = await probeSvcControl(f.client, { approvedPort: PORT });
    assert.equal(result.connectionCheckAck, true); assert.equal(result.elapsedMs, 0); f.assertClean();
  });
});

test('keepalive close callback enforces the unrounded 250ms deadline even when its timer has not run', async t => {
  for (const elapsed of [250, 250.1, 251]) await t.test('close boundary', async t => {
    const f = await watching(t);
    for (let i = 0; i < 34; i++) { await f.clock.advance(1000); await f.keep(); }
    // Alter only the monotonic reading; do not tick the pending cleanup timer.
    f.socket.settings.onClose = () => f.clock.set(135_000 + elapsed);
    await f.clock.advance(1000);
    if (elapsed === 250) {
      const result = await f.work; assert.equal(result.elapsedMs, 35250); assert.equal(result.udpSocketClosed, true);
    } else await rejected(f.work, 'cleanup_failed', f.key);
    f.assertClean();
  });
});
