import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createConnection, createServer } from 'node:net';
import { createSocket } from 'node:dgram';
import { setTimeout as delay } from 'node:timers/promises';
import { GameTunnelError, withGameTunnel, withSvcTunnel, withPlasmoTunnel } from './tunnel-session.mjs';

const windows = process.platform === 'win32';
const deferred = () => {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
};

// Honest TCP byte fixture: no Minecraft login, world, mod or voice evidence.
// EADDRINUSE refuses the fixture before starting any agent. We never find or
// close an existing process/listener to make this fixed-port test pass.
async function fixture(t) {
  const sockets = new Set(); const state = { accepts: 0, bytes: 0 };
  const server = createServer(socket => {
    state.accepts++; sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
    socket.on('error', () => {});
    socket.on('data', bytes => { state.bytes += bytes.length; });
    socket.pipe(socket);
  });
  try {
    await new Promise((resolve, reject) => {
      server.once('error', reject);
      server.listen({ host: '127.0.0.1', port: 43001, exclusive: true }, resolve);
    });
  } catch (error) {
    if (error.code === 'EADDRINUSE') { t.skip('fixture port 43001 belongs to another listener; no agent started'); return undefined; }
    throw error;
  }
  t.after(async () => {
    for (const socket of sockets) socket.destroy();
    await new Promise(resolve => server.close(resolve));
  });
  return { server, state, sockets };
}

async function exchange(port, payload, signal) {
  const socket = createConnection({ host: '127.0.0.1', port, signal });
  const ended = deferred(); socket.once('close', ended.resolve);
  let timer;
  try {
    return await new Promise((resolve, reject) => {
      let count = 0; const output = Buffer.alloc(payload.length);
      timer = setTimeout(() => { socket.destroy(); reject(new Error('fixture_exchange_deadline')); }, 5_000);
      socket.once('error', () => reject(new Error('fixture_exchange_io')));
      socket.once('connect', () => socket.end(payload));
      socket.on('data', bytes => {
        if (count + bytes.length > output.length) { socket.destroy(); reject(new Error('fixture_exchange_overflow')); return; }
        bytes.copy(output, count); count += bytes.length;
      });
      socket.once('end', () => {
        if (count !== payload.length) reject(new Error('fixture_exchange_truncated'));
        else resolve(output);
      });
    });
  } finally { clearTimeout(timer); socket.destroy(); await ended.promise; }
}

async function bridgeClosed(port) {
  const socket = createConnection({ host: '127.0.0.1', port });
  const ended = deferred(); socket.once('close', ended.resolve);
  let timer;
  try {
    await new Promise((resolve, reject) => {
      socket.once('connect', () => reject(new Error('bridge_still_listening')));
      socket.once('error', error => error.code === 'ECONNREFUSED' ? resolve() : reject(new Error('unexpected_closed_bridge_error')));
      timer = setTimeout(() => reject(new Error('closed_bridge_deadline')), 2_000);
    });
  } finally { clearTimeout(timer); socket.destroy(); await ended.promise; }
}
function reaped(summary) {
  assert.equal(summary.agents, 2); assert.equal(summary.closed, 2);
  assert.equal(summary.forced, false); assert.ok(summary.stdoutBytes > 0);
  assert.equal(summary.stderrBytes, 0);
  assert.deepEqual(Object.keys(summary).sort(), ['agents', 'closed', 'forced', 'stderrBytes', 'stdoutBytes']);
}
function redacted(error, code) {
  assert.ok(error instanceof GameTunnelError); assert.equal(error.code, code); assert.equal(error.message, code);
  assert.ok(!JSON.stringify(error).includes('127.0.0.1'));
  assert.ok(!JSON.stringify(error).includes('PRIVATE_CALLBACK'));
  assert.ok(!JSON.stringify(error).includes('session'));
  reaped(error.cleanup);
  return true;
}

test('invalid options and pre-cancellation never enter callback', { skip: !windows }, async () => {
  let called = false;
  const operation = () => { called = true; };
  for (const options of [null, [], { timeoutMs: 0 }, { timeoutMs: 120001 }, { timeoutMs: 1.5 },
    { executable: 'foreign' }, { signal: {} }]) {
    await assert.rejects(withGameTunnel(operation, options), { code: 'game_tunnel_invalid_options' });
  }
  const controller = new AbortController(); controller.abort('PRIVATE_CALLBACK');
  await assert.rejects(withGameTunnel(operation, { signal: controller.signal }), { code: 'game_tunnel_cancelled' });
  assert.equal(called, false);
});

test('real CLI pair forwards TCP bytes and both agents close normally', { skip: !windows }, async t => {
  const f = await fixture(t); if (!f) return;
  let port; let operationSignal;
  const payload = Buffer.alloc(128 * 1024, 0x39);
  const result = await withGameTunnel(async context => {
    assert.deepEqual(Object.keys(context).sort(), ['bridgePort', 'signal']);
    assert.ok(Object.isFrozen(context)); assert.ok(context.signal instanceof AbortSignal);
    assert.equal(context.signal.aborted, false); assert.equal(f.state.accepts, 0);
    port = context.bridgePort; operationSignal = context.signal;
    assert.ok(port > 0 && port <= 65535 && port !== 43001);
    assert.deepEqual(await exchange(port, payload, context.signal), payload);
    return 'byte_fixture_only';
  });
  assert.equal(result.value, 'byte_fixture_only'); reaped(result.cleanup);
  assert.equal(operationSignal.aborted, true); await bridgeClosed(port);
  assert.equal(f.state.accepts, 1); assert.equal(f.state.bytes, payload.length);
  assert.equal(f.sockets.size, 0); assert.equal(f.server.listening, true);
  assert.ok(!JSON.stringify(result).includes('127.0.0.1'));
  assert.ok(!JSON.stringify(result).includes('session'));
});

test('two concurrent calls create two independent real CLI pairs carrying separate bytes', { skip: !windows }, async t => {
  const f = await fixture(t); if (!f) return;
  const bothReady = deferred(); let ready = 0;
  const ports = []; const controller = new AbortController();
  const runs = [0x41, 0x72].map(byte => withGameTunnel(async ({ bridgePort, signal }) => {
    ports.push(bridgePort); ready++;
    if (ready === 2) bothReady.resolve();
    await Promise.race([bothReady.promise, new Promise(resolve => signal.addEventListener('abort', resolve, { once: true }))]);
    signal.throwIfAborted();
    const payload = Buffer.alloc(96 * 1024, byte);
    assert.deepEqual(await exchange(bridgePort, payload, signal), payload);
    return payload.length;
  }, { signal: controller.signal }));
  let results;
  try { results = await Promise.all(runs); }
  finally { controller.abort(); await Promise.allSettled(runs); }
  assert.equal(ready, 2); assert.equal(new Set(ports).size, 2);
  for (const result of results) { assert.equal(result.value, 96 * 1024); reaped(result.cleanup); }
  assert.equal(results.reduce((n, item) => n + item.cleanup.agents, 0), 4);
  await Promise.all(ports.map(bridgeClosed));
  assert.equal(f.state.accepts, 2); assert.equal(f.state.bytes, 192 * 1024);
  assert.equal(f.sockets.size, 0); assert.equal(f.server.listening, true);
});

test('callback failure reaps both agents, redacts content and preserves foreign fixed-port listener', { skip: !windows }, async t => {
  const f = await fixture(t); if (!f) return;
  const other = createServer();
  await assert.rejects(new Promise((resolve, reject) => {
    other.once('error', reject); other.listen({ host: '127.0.0.1', port: 43001, exclusive: true }, resolve);
  }), { code: 'EADDRINUSE' });
  let port;
  await assert.rejects(withGameTunnel(async ({ bridgePort, signal }) => {
    port = bridgePort;
    assert.deepEqual(await exchange(port, Buffer.from('before failure'), signal), Buffer.from('before failure'));
    throw new Error('PRIVATE_CALLBACK pin session token');
  }), error => redacted(error, 'game_tunnel_operation_failed'));
  await bridgeClosed(port); assert.equal(f.server.listening, true);
  assert.deepEqual(await exchange(43001, Buffer.from('foreign survives')), Buffer.from('foreign survives'));
  assert.equal(f.sockets.size, 0);
});

test('explicit abort during operation stays cancellation after graceful cleanup', { skip: !windows }, async t => {
  const f = await fixture(t); if (!f) return;
  const controller = new AbortController(); let port; let callbackObserved = false;
  await assert.rejects(withGameTunnel(async ({ bridgePort, signal }) => {
    port = bridgePort;
    const observed = new Promise(resolve => signal.addEventListener('abort', () => { callbackObserved = true; resolve('late value'); }, { once: true }));
    controller.abort('PRIVATE_CALLBACK');
    return observed;
  }, { signal: controller.signal }), error => redacted(error, 'game_tunnel_cancelled'));
  assert.equal(callbackObserved, true); await bridgeClosed(port);
  assert.equal(f.server.listening, true); assert.equal(f.state.accepts, 0);
});

test('global deadline during an uncooperative callback cannot become success', { skip: !windows }, async t => {
  const f = await fixture(t); if (!f) return;
  const late = deferred(); let port; let started = false;
  const result = withGameTunnel(async ({ bridgePort }) => { port = bridgePort; started = true; return late.promise; }, { timeoutMs: 2_000 });
  await assert.rejects(result, error => redacted(error, 'game_tunnel_deadline'));
  assert.equal(started, true); await bridgeClosed(port);
  late.resolve('PRIVATE_CALLBACK late success'); await delay(0);
  assert.equal(f.server.listening, true);
});

test('deadline during binary verification starts no agents or operation', { skip: !windows }, async () => {
  let called = false;
  await assert.rejects(withGameTunnel(() => { called = true; }, { timeoutMs: 1 }), error => {
    assert.equal(error.code, 'game_tunnel_deadline'); assert.equal(error.cleanup.agents, 0);
    assert.equal(error.cleanup.closed, 0); assert.equal(error.cleanup.forced, false); return true;
  });
  assert.equal(called, false);
});

async function udpFixture(t, port) {
  const socket = createSocket('udp4');
  try {
    await new Promise((resolve, reject) => {
      socket.once('error', reject); socket.bind({ address: '127.0.0.1', port, exclusive: true }, resolve);
    });
  } catch (error) {
    try { socket.close(); } catch { /* not bound */ }
    throw error;
  }
  t.after(() => new Promise(resolve => socket.close(resolve)));
  return socket;
}

async function udpExchange(port, payload, signal) {
  const socket = createSocket('udp4'); let timer;
  const abort = () => socket.close();
  try {
    return await new Promise((resolve, reject) => {
      timer = setTimeout(() => reject(new Error('udp_fixture_deadline')), 2000);
      signal?.addEventListener('abort', abort, { once: true });
      socket.once('error', () => reject(new Error('udp_fixture_io')));
      socket.once('close', () => reject(new Error('udp_fixture_closed')));
      socket.once('message', (bytes, source) => {
        assert.equal(source.address, '127.0.0.1'); assert.equal(source.port, port); resolve(bytes);
      });
      socket.bind(0, '127.0.0.1', () => socket.send(payload, port, '127.0.0.1'));
    });
  } finally {
    clearTimeout(timer); signal?.removeEventListener('abort', abort);
    await new Promise(resolve => { try { socket.close(resolve); } catch { resolve(); } });
  }
}

for (const [profile, port, tunnel] of [['SVC', 43002, withSvcTunnel], ['Plasmo', 43003, withPlasmoTunnel]]) {
test(`${profile} control profile carries opaque UDP returns to two origins while TCP remains active`, { skip: !windows }, async t => {
  const f = await fixture(t); if (!f) return;
  const udp = await udpFixture(t, port); const origins = new Set();
  udp.on('message', (bytes, source) => { origins.add(source.port); udp.send(bytes, source.port, source.address); });
  const result = await tunnel(async context => {
    assert.equal(context.voicePort, 35554); assert.ok(context.maxDatagramPayload >= 128);
    const tcp = Buffer.alloc(96 * 1024, 0x42); const first = Buffer.alloc(83, 0x51); const second = Buffer.alloc(47, 0x72);
    const replies = await Promise.all([exchange(context.bridgePort, tcp, context.signal),
      udpExchange(context.voicePort, first, context.signal), udpExchange(context.voicePort, second, context.signal)]);
    assert.deepEqual(replies, [tcp, first, second]); return true;
  });
  assert.equal(result.value, true); reaped(result.cleanup); assert.equal(origins.size, 2);
  // Released guest UDP port can be rebound without touching the target fixture.
  const released = await udpFixture(t, 35554); assert.equal(released.address().port, 35554);
  assert.equal(f.server.listening, true); assert.equal(udp.address().port, port);
});

test(`occupied ${profile} guest port refuses before agents and preserves the foreign listener`, { skip: !windows }, async t => {
  const foreign = await udpFixture(t, 35554); let called = false;
  await assert.rejects(tunnel(() => { called = true; }), error => {
    assert.equal(error.code, 'game_tunnel_voice_port_in_use'); assert.equal(error.cleanup.agents, 0);
    assert.equal(error.cleanup.closed, 0); return true;
  });
  assert.equal(called, false); assert.equal(foreign.address().port, 35554);
});
}
