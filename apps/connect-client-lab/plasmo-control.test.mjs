import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { inspect } from 'node:util';
import dgram from 'node:dgram';
import crypto from 'node:crypto';
import { probePlasmoControl, PlasmoControlError } from './plasmo-control.mjs';
import { offlineUuid, PLAY_NAMES } from '../connect-runtime-tools/play-profile.mjs';

// Pure control fixtures. Every UDP socket below is an in-memory EventEmitter;
// no TCP/UDP listener, Minecraft/mod, audio, account or Java is started.
const PORT = 35554; const CHANNEL = 'plasmo:voice/v2';
const UUID = offlineUuid(PLAY_NAMES[0]);
const { publicKey: FIXTURE_PUBLIC } = crypto.generateKeyPairSync('rsa', { modulusLength: 2048, publicExponent: 65537 });
const ticks = async () => { for (let i = 0; i < 6; i++) await new Promise(resolve => setImmediate(resolve)); };

// Independent layouts from pinned upstream Java DataOutput; no implementation
// codec or secret getter is exported or called by this fixture.
function connection(secret, changes = {}) {
  const address = Buffer.isBuffer(changes.address) ? changes.address : Buffer.from(changes.address ?? '127.0.0.1');
  const bytes = Buffer.alloc(23 + address.length);
  bytes[0] = 1; secret.copy(bytes, 1); bytes.writeUInt16BE(address.length, 17);
  address.copy(bytes, 19); bytes.writeInt32BE(changes.port ?? PORT, 19 + address.length); return bytes;
}
function serverPing(secret, changes = {}) {
  const bytes = Buffer.alloc(37); bytes.writeUInt32BE(changes.magic ?? 0x4e9004e9); bytes[4] = changes.type ?? 1;
  (changes.secret ?? secret).copy(bytes, 5); bytes.writeBigInt64BE(changes.timestamp ?? 123n, 21);
  bytes.writeBigInt64BE(changes.time ?? 456n, 29); return bytes;
}
function checkPlayerInfo(bytes) {
  assert.deepEqual(bytes.subarray(0, 3), Buffer.from([10, 1, 1]));
  let index = 3;
  for (const expected of ['1.21.1', '2.1.17']) {
    const length = bytes.readUInt16BE(index); index += 2;
    assert.equal(bytes.subarray(index, index + length).toString('ascii'), expected); index += length;
  }
  const length = bytes.readInt32BE(index); index += 4;
  assert.ok(length >= 1 && length <= 2048); assert.equal(index + length, bytes.length);
  const key = crypto.createPublicKey({ key: bytes.subarray(index), format: 'der', type: 'spki' });
  assert.equal(key.asymmetricKeyType, 'rsa'); assert.equal(key.asymmetricKeyDetails.modulusLength, 2048);
  assert.equal(key.asymmetricKeyDetails.publicExponent, 65537n);
}
function checkClientPing(secret, bytes, initial) {
  assert.equal(bytes.length, initial ? 50 : 37); assert.equal(bytes.readUInt32BE(0), 0x4e9004e9);
  assert.equal(bytes[4], 1); assert.deepEqual(bytes.subarray(5, 21), secret);
  assert.ok(bytes.readBigInt64BE(21) > 0n); assert.ok(bytes.readBigInt64BE(29) > 0n);
  if (initial) {
    assert.equal(bytes.readUInt16BE(37), 9); assert.equal(bytes.subarray(39, 48).toString('ascii'), '127.0.0.1');
    assert.equal(bytes.readUInt16BE(48), PORT);
  }
}

class FakeSocket extends EventEmitter {
  constructor(f, settings) { super(); this.f = f; this.settings = settings; this.sent = []; this.callbacks = []; this.closed = false; }
  bind(port, address, callback) {
    assert.equal(port, 0); assert.equal(address, '127.0.0.1'); this.pendingBind = callback;
    if (this.settings.bindThrows) throw new Error('UNSAFE_BIND');
    if (!this.settings.holdBind) queueMicrotask(callback);
  }
  connect(port, address, callback) {
    assert.equal(port, PORT); assert.equal(address, '127.0.0.1'); this.pendingConnect = callback;
    if (this.settings.connectThrows) throw new Error('UNSAFE_CONNECT');
    if (!this.settings.holdConnect) queueMicrotask(() => callback(this.settings.connectError ? new Error('UNSAFE_CONNECT') : undefined));
  }
  send(bytes, callback) {
    if (this.settings.sendThrows) throw new Error('UNSAFE_SEND');
    const saved = Buffer.from(bytes); this.sent.push(saved); this.callbacks.push(callback);
    queueMicrotask(() => {
      if (!this.settings.holdSend) callback(this.settings.sendError ? new Error('UNSAFE_SEND') : undefined);
      if (this.closed) return;
      if (this.settings.onSend) this.settings.onSend(this, saved);
      else {
        checkClientPing(this.f.secret, saved, saved.length === 50);
        if (saved.length === 50) this.receive(serverPing(this.f.secret));
      }
    });
  }
  receive(bytes, remote = { address: '127.0.0.1', port: PORT, family: 'IPv4' }) { this.emit('message', bytes, remote); }
  close(callback) {
    if (this.settings.closeThrows) throw new Error('UNSAFE_CLOSE');
    if (this.settings.closeStalls) return;
    this.closed = true; queueMicrotask(() => { this.emit('close'); callback(); });
  }
}

function fixture(t, settings = {}) {
  const f = { secret: crypto.randomBytes(16), sockets: [], writes: [], keyCalls: 0 };
  const client = new EventEmitter(); client.state = settings.initialState ?? 'handshaking';
  client.socket = { destroyed: false, closed: false }; f.client = client;
  f.play = () => {
    client.uuid = settings.uuid ?? UUID; client.username = settings.username ?? PLAY_NAMES[0];
    client.state = 'play'; client.emit('state', 'play', 'configuration');
  };
  if (client.state === 'play') { client.uuid = settings.uuid ?? UUID; client.username = settings.username ?? PLAY_NAMES[0]; }
  f.payload = bytes => client.emit('custom_payload', { channel: CHANNEL, data: bytes });
  client.write = (name, packet) => {
    assert.equal(name, 'custom_payload');
    if (settings.writeThrows) throw new Error('UNSAFE_WRITE');
    f.writes.push({ channel: packet.channel, data: Buffer.from(packet.data) });
    if (packet.channel === 'minecraft:register') {
      assert.deepEqual(packet.data, Buffer.from(CHANNEL));
      if (!settings.noRequest) queueMicrotask(() => f.payload(settings.request ?? Buffer.from([2])));
    } else {
      assert.equal(packet.channel, CHANNEL); checkPlayerInfo(packet.data);
      if (!settings.noConnection) queueMicrotask(() => f.payload(settings.connection ? settings.connection(f.secret) : connection(f.secret)));
    }
  };
  if (!settings.realKeyGeneration) t.mock.method(crypto, 'generateKeyPair', (kind, options, callback) => {
    f.keyCalls++; assert.equal(kind, 'rsa'); assert.deepEqual(options, { modulusLength: 2048, publicExponent: 65537 });
    f.keyCallback = callback;
    if (settings.keyThrows) throw new Error('UNSAFE_KEY');
    if (!settings.holdKey) queueMicrotask(() => callback(settings.keyError ? new Error('UNSAFE_KEY') : null,
      settings.badKey ? { export: () => Buffer.alloc(2049) } : FIXTURE_PUBLIC));
  });
  t.mock.method(dgram, 'createSocket', kind => {
    assert.equal(kind, 'udp4'); if (settings.createThrows) throw new Error('UNSAFE_CREATE');
    const socket = new FakeSocket(f, settings); f.sockets.push(socket); return socket;
  });
  f.clean = () => {
    for (const event of ['state', 'custom_payload', 'error', 'end']) assert.equal(client.listenerCount(event), 0);
    for (const socket of f.sockets) if (!settings.closeStalls && !settings.closeThrows) {
      assert.equal(socket.closed, true); assert.equal(socket.eventNames().length, 0);
    }
  };
  f.start = (options = {}) => {
    const result = probePlasmoControl(client, { approvedPort: PORT, ...options });
    if (client.state !== 'play' && !settings.noPlay) f.play();
    return result;
  };
  t.after(() => { f.clean(); f.secret.fill(0); }); return f;
}
async function rejected(promise, code, secret) {
  await assert.rejects(promise, error => {
    assert.ok(error instanceof PlasmoControlError); assert.equal(error.message, `plasmo_control_${code}`);
    assert.equal(error.cause, undefined); const text = `${inspect(error)}${JSON.stringify(error)}`;
    assert.doesNotMatch(text, /UNSAFE_|127\.0\.0\.1|35554|43003|AuroraLab|plasmo:voice/);
    assert.ok(!text.includes(UUID)); if (secret) assert.ok(!text.includes(secret.toString('hex'))); return true;
  });
}

test('real RSA2048 public key answers official request; independent plaintext Ping control closes cleanly', async t => {
  const f = fixture(t, { realKeyGeneration: true }); const result = await f.start();
  assert.equal(f.writes.length, 2); checkPlayerInfo(f.writes[1].data);
  assert.equal(f.sockets.length, 1); assert.equal(f.sockets[0].sent.length, 2);
  checkClientPing(f.secret, f.sockets[0].sent[0], true); checkClientPing(f.secret, f.sockets[0].sent[1], false);
  assert.equal(result.playerInfoRequested, true); assert.equal(result.playerInfoSent, true);
  assert.equal(result.announcementMatched, true); assert.equal(result.serverPingReceived, true);
  assert.equal(result.keepAliveResponseSent, true); assert.equal(result.udpSocketClosed, true);
  assert.equal(result.authentication, 'session_secret_only_no_aead');
  assert.equal(result.datagramsSent, 2); assert.equal(result.datagramsReceived, 1);
  assert.equal(result.bytesSent, 87); assert.equal(result.bytesReceived, 37);
  assert.equal(result.audio, 'not_tested'); assert.equal(result.accountAuthentication, 'not_tested');
  assert.equal(result.runtimeHomologated, false); assert.ok(Object.isFrozen(result));
  const text = JSON.stringify(result);
  assert.doesNotMatch(text, /uuid|127\.0\.0\.1|35554|43003|timestamp|rtt|publicKey|privateKey/i);
  assert.ok(!text.includes(f.secret.toString('hex'))); assert.ok(!text.includes(UUID)); f.clean();
});

test('listeners precede PLAY; no writes or sockets until identity and official request', async t => {
  const f = fixture(t, { noPlay: true, noRequest: true }); const work = f.start();
  assert.equal(f.client.listenerCount('custom_payload'), 1); assert.equal(f.writes.length, 0);
  for (const state of ['login', 'configuration']) { f.client.state = state; f.client.emit('state', state); }
  assert.equal(f.writes.length, 0); f.play(); await ticks();
  assert.equal(f.writes.length, 1); assert.equal(f.keyCalls, 0); assert.equal(f.sockets.length, 0);
  f.payload(Buffer.from([2])); const result = await work; assert.equal(result.playerInfoRequests, 1);
});

test('bounded normal Config and player-list notifications stay opaque and unretained', async t => {
  const f = fixture(t, { onSend: (socket, bytes) => {
    if (bytes.length === 50) {
      for (const id of [3, 7, 8]) f.payload(Buffer.from([id, 0xff, 0, 0xff]));
      f.client.emit('custom_payload', { channel: 'unrelated:channel', data: Buffer.alloc(100) });
      socket.receive(serverPing(f.secret));
    }
  } });
  const result = await f.start(); assert.equal(result.payloadsReceived, 5); assert.equal(result.payloadBytesReceived, 45);
});

test('arguments and cancellation fail before any write, key or UDP creation', async t => {
  const f = fixture(t);
  for (const options of [null, [], {}, { approvedPort: 35555 }, { approvedPort: PORT, timeoutMs: 0 },
    { approvedPort: PORT, timeoutMs: 10001 }, { approvedPort: PORT, signal: {} }, { approvedPort: PORT, host: '127.0.0.1' }]) {
    await rejected(probePlasmoControl(f.client, options), 'arguments');
  }
  await rejected(probePlasmoControl(f.client, { approvedPort: PORT, signal: AbortSignal.abort('UNSAFE_CANCEL') }), 'cancelled');
  assert.equal(f.writes.length, 0); assert.equal(f.keyCalls, 0); assert.equal(f.sockets.length, 0);
});

for (const state of ['handshaking', 'login', 'configuration', 'play']) test(`probe accepts initial ${state} lifecycle`, async t => {
  const f = fixture(t, { initialState: state }); await f.start();
});
for (const setting of [{ uuid: offlineUuid(PLAY_NAMES[1]) }, { username: PLAY_NAMES[1] }, { uuid: '' }]) {
  test('foreign or missing synthetic identity cannot request a credential', async t => {
    const f = fixture(t, setting); await rejected(f.start(), 'player_mismatch');
    assert.equal(f.writes.length, 0); assert.equal(f.keyCalls, 0); assert.equal(f.sockets.length, 0);
  });
}
test('debug environment is rejected without changing it', async t => {
  const f = fixture(t); const previous = process.env.DEBUG;
  try { process.env.DEBUG = 'UNSAFE_DEBUG'; await rejected(f.start(), 'unsafe_debug_environment'); }
  finally { if (previous === undefined) delete process.env.DEBUG; else process.env.DEBUG = previous; }
});
test('a second probe cannot share the same Minecraft client', async t => {
  const f = fixture(t, { noRequest: true }); const controller = new AbortController(); const work = f.start({ signal: controller.signal });
  await rejected(probePlasmoControl(f.client, { approvedPort: PORT }), 'already_active');
  const done = rejected(work, 'cancelled'); controller.abort(); await done;
});

for (const address of ['', 'localhost', '0.0.0.0', '::1', '127.0.0.2', '127.0.0.1\0foreign', '127.0.0.1 ',
  Buffer.from([0xc0, 0x80]), Buffer.from([0xff])]) {
  test('only exact ASCII announced loopback is accepted before opening UDP', async t => {
    const f = fixture(t, { connection: key => connection(key, { address }) });
    await rejected(f.start(), 'announcement', f.secret); assert.equal(f.sockets.length, 0);
  });
}
for (const port of [-1, 0, 43003, 35555, 65536]) test('unapproved announced port never opens UDP', async t => {
  const f = fixture(t, { connection: key => connection(key, { port }) }); await rejected(f.start(), 'announcement', f.secret);
  assert.equal(f.sockets.length, 0);
});
const badConnections = [
  key => connection(key).subarray(0, -1), key => Buffer.concat([connection(key), Buffer.from([0])]),
  key => { const b = connection(key); b.writeUInt16BE(256, 17); return b; },
  () => connection(Buffer.alloc(16)), () => Buffer.from([1]),
];
for (const bad of badConnections) test('malformed Connection fails closed before UDP', async t => {
  const f = fixture(t, { connection: bad }); await rejected(f.start(), 'connection', f.secret); assert.equal(f.sockets.length, 0);
});
test('malformed Request is rejected and generates no RSA key', async t => {
  const f = fixture(t, { request: Buffer.from([2, 0]) }); await rejected(f.start(), 'request'); assert.equal(f.keyCalls, 0);
});
test('Connection before official request is not accepted', async t => {
  const f = fixture(t, { noRequest: true }); const work = f.start(); f.payload(connection(f.secret));
  await rejected(work, 'unexpected_packet', f.secret); assert.equal(f.keyCalls, 0);
});
test('scheduler retries reuse one generation and answer only once', async t => {
  const f = fixture(t, { holdKey: true }); const work = f.start(); await ticks();
  for (let i = 0; i < 7; i++) f.payload(Buffer.from([2]));
  assert.equal(f.keyCalls, 1); f.keyCallback(null, FIXTURE_PUBLIC);
  const result = await work; assert.equal(result.playerInfoRequests, 8); assert.equal(result.playerInfoResponses, 1);
});
test('request flood is bounded even while RSA is pending', async t => {
  const f = fixture(t, { holdKey: true }); const work = f.start(); const done = rejected(work, 'limits'); await ticks();
  for (let i = 0; i < 8; i++) f.payload(Buffer.from([2])); f.keyCallback(null, FIXTURE_PUBLIC); await done;
  assert.equal(f.writes.length, 1); assert.equal(f.sockets.length, 0);
});

for (const [code, settings] of [['key_generation', { keyThrows: true }], ['key_generation', { keyError: true }],
  ['key_generation', { badKey: true }], ['failed', { createThrows: true }], ['failed', { bindThrows: true }],
  ['socket_failed', { connectThrows: true }], ['socket_failed', { connectError: true }],
  ['failed', { sendThrows: true }], ['socket_failed', { sendError: true }], ['failed', { writeThrows: true }]]) {
  test('native startup and asynchronous failures are redacted and cleaned', async t => {
    const f = fixture(t, settings); await rejected(f.start(), code, f.secret);
  });
}
for (const [code, make] of [
  ['authentication', key => serverPing(key, { secret: Buffer.alloc(16) })],
  ['datagram', key => serverPing(key, { type: 2 })], ['datagram', key => serverPing(key, { magic: 0 })],
  ['datagram', key => serverPing(key).subarray(0, -1)], ['datagram', key => Buffer.concat([serverPing(key), Buffer.from([0])])],
  ['datagram', key => serverPing(key, { timestamp: -1n })], ['datagram', key => serverPing(key, { time: -1n })],
  ['datagram', () => Buffer.alloc(513)], ['datagram', () => Buffer.alloc(0)],
]) test('invalid UDP header, credential, framing or control type fails closed', async t => {
  const f = fixture(t, { onSend: socket => socket.receive(make(f.secret)) }); await rejected(f.start(), code, f.secret);
});
test('echoing the initial Ping is not a server keepalive', async t => {
  const f = fixture(t, { onSend: (socket, bytes) => socket.receive(bytes) }); await rejected(f.start(), 'datagram', f.secret);
});
for (const remote of [{ address: '127.0.0.2', port: PORT, family: 'IPv4' },
  { address: '127.0.0.1', port: 43003, family: 'IPv4' }, { address: '127.0.0.1', port: PORT, family: 'IPv6' }, null]) {
  test('wrong UDP source never authenticates', async t => {
    const f = fixture(t, { onSend: socket => socket.receive(serverPing(f.secret), remote) });
    await rejected(f.start(), 'datagram_address', f.secret);
  });
}
test('duplicate Ping before answer send completion is rejected, not another success', async t => {
  const f = fixture(t, { holdSend: true, onSend: (socket, bytes) => {
    if (bytes.length === 50) { socket.receive(serverPing(f.secret)); socket.receive(serverPing(f.secret)); }
  } }); await rejected(f.start(), 'unexpected_packet', f.secret);
});
test('response callback plus all pending initial sends must complete before success', async t => {
  const f = fixture(t, { holdSend: true }); let complete = false;
  const work = f.start().then(value => { complete = true; return value; }); await ticks();
  assert.equal(f.sockets[0].sent.length, 2); f.sockets[0].callbacks[1](); await ticks(); assert.equal(complete, false);
  f.sockets[0].callbacks[0](); await work; assert.equal(complete, true);
});
test('a failed initial send cannot be hidden by an already successful answer callback', async t => {
  const f = fixture(t, { holdSend: true }); const work = f.start(); const done = rejected(work, 'socket_failed'); await ticks();
  f.sockets[0].callbacks[1](); f.sockets[0].callbacks[0](new Error('UNSAFE_SEND')); await done;
});

for (const change of [client => { client.socket = {}; }, client => { client.socket.destroyed = true; },
  client => { client.state = 'login'; }, client => { client.protocolRejected = true; }]) {
  test('Ping cannot complete after Minecraft connection is replaced or stopped', async t => {
    const f = fixture(t, { onSend: socket => { change(f.client); socket.receive(serverPing(f.secret)); } });
    await rejected(f.start(), 'client_changed', f.secret);
  });
}
test('identity is revalidated when receiving Ping', async t => {
  const f = fixture(t, { onSend: socket => { f.client.uuid = offlineUuid(PLAY_NAMES[1]); socket.receive(serverPing(f.secret)); } });
  await rejected(f.start(), 'player_mismatch', f.secret);
});
test('client leaving PLAY is rejected immediately while waiting for Request', async t => {
  const f = fixture(t, { noRequest: true }); const work = f.start(); f.client.state = 'configuration'; f.client.emit('state', 'configuration');
  await rejected(work, 'client_changed');
});
for (const [event, code] of [['error', 'client_failed'], ['end', 'client_closed']]) test('Minecraft lifecycle failure does not expose raw errors', async t => {
  const f = fixture(t, { noRequest: true }); const work = f.start(); f.client.emit(event, new Error('UNSAFE_CLIENT'));
  await rejected(work, code);
});
for (const [event, code] of [['error', 'socket_failed'], ['close', 'socket_closed']]) test('UDP lifecycle failure cannot produce PASS', async t => {
  const f = fixture(t, { onSend: socket => {
    if (event === 'close') socket.closed = true; socket.emit(event, new Error('UNSAFE_UDP'));
  } }); await rejected(f.start(), code);
});

for (const [setting, callback] of [['holdBind', 'pendingBind'], ['holdConnect', 'pendingConnect']]) {
  test('cancel during pending socket setup closes it and blocks late callbacks', async t => {
    const f = fixture(t, { [setting]: true }); const abort = new AbortController(); const work = f.start({ signal: abort.signal });
    const done = rejected(work, 'cancelled'); await ticks(); abort.abort('UNSAFE_CANCEL'); await done;
    f.sockets[0][callback](); await ticks(); assert.equal(f.sockets[0].sent.length, 0);
  });
}
test('cancel while native RSA is pending waits for callback but never sends late PlayerInfo', async t => {
  const f = fixture(t, { holdKey: true }); const abort = new AbortController(); let done = false;
  const work = f.start({ signal: abort.signal }); const rejection = rejected(work, 'cancelled').then(() => { done = true; });
  await ticks(); abort.abort(); await ticks(); assert.equal(done, false);
  f.keyCallback(null, FIXTURE_PUBLIC); await rejection; assert.equal(f.writes.length, 1); assert.equal(f.sockets.length, 0);
});
test('deadline bounds absent official request', async t => {
  const f = fixture(t, { noRequest: true }); await rejected(f.start({ timeoutMs: 5 }), 'deadline');
  assert.equal(f.keyCalls, 0); assert.equal(f.sockets.length, 0);
});
test('initial UDP retries stop after three and total deadline rejects silence', async t => {
  t.mock.timers.enable({ apis: ['setTimeout', 'setInterval'] });
  const f = fixture(t, { onSend: () => {} }); const work = f.start(); const done = rejected(work, 'deadline'); await ticks();
  for (let i = 0; i < 8; i++) { t.mock.timers.tick(1000); await ticks(); }
  assert.equal(f.sockets[0].sent.length, 3); t.mock.timers.tick(2000); await ticks(); await done;
});
test('late connect failure after deadline cannot create traffic or override first failure', async t => {
  const f = fixture(t, { holdConnect: true }); const work = f.start({ timeoutMs: 20 }); const done = rejected(work, 'deadline');
  await ticks(); await done; f.sockets[0].pendingConnect(new Error('UNSAFE_LATE')); await ticks();
  assert.equal(f.sockets[0].sent.length, 0);
});
for (const settings of [{ closeStalls: true }, { closeThrows: true }]) test('unconfirmed socket cleanup is never a successful probe', async t => {
  const f = fixture(t, settings); await rejected(f.start(), 'cleanup_failed');
});

for (const data of [Buffer.alloc(0), Buffer.alloc(65537), new Uint8Array([2])]) test('payload framing quota is checked before interpretation', async t => {
  const f = fixture(t, { noRequest: true }); const work = f.start(); f.payload(data); await rejected(work, 'limits');
});
test('opaque Config payload total byte budget prevents retaining or decoding arbitrary content', async t => {
  const f = fixture(t, { onSend: () => {
    const bytes = Buffer.alloc(65536, 0xaa); bytes[0] = 3;
    f.payload(bytes); f.payload(bytes);
  } }); await rejected(f.start(), 'limits', f.secret);
});
test('opaque payload count is limited separately from bytes', async t => {
  const f = fixture(t, { onSend: () => { for (let i = 0; i < 31; i++) f.payload(Buffer.from([7])); } });
  await rejected(f.start(), 'limits', f.secret);
});
for (const data of [Buffer.from([99]), Buffer.from([3]), Buffer.from([7])]) test('unsupported or premature payload cannot authorize UDP', async t => {
  const f = fixture(t, { noRequest: true }); const work = f.start(); f.payload(data); await rejected(work, 'unexpected_packet');
  assert.equal(f.sockets.length, 0);
});
test('duplicate Connection cannot replace the private socket or credential', async t => {
  const f = fixture(t, { onSend: () => f.payload(connection(f.secret)) }); await rejected(f.start(), 'unexpected_packet', f.secret);
  assert.equal(f.sockets.length, 1);
});
