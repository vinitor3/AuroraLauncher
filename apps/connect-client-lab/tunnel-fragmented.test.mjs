// Real owned processes and sockets. Run only with the reviewed immutable V2
// artifact and the laboratory ports released; no Minecraft/mod/audio evidence.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer, createConnection } from 'node:net';
import { createSocket } from 'node:dgram';
import { setTimeout as delay } from 'node:timers/promises';
import { withSvcFragmentedTunnel, withPlasmoFragmentedTunnel, GameTunnelError } from './tunnel-session.mjs';

const windows = process.platform === 'win32';
const closeUdp = socket => new Promise(resolve => { try { socket.close(resolve); } catch { resolve(); } });
async function bindUdp(port) {
  const socket = createSocket('udp4');
  try {
    await new Promise((resolve, reject) => {
      socket.once('error', reject); socket.bind({ address: '127.0.0.1', port, exclusive: true }, resolve);
    });
    return socket;
  } catch { await closeUdp(socket); throw new Error('owned_udp_fixture_bind_failed'); }
}
async function fixture(t, port) {
  const state = { accepts: 0, active: 0, tcpBytes: 0, udpBytes: 0, udpRequests: 0,
    udpWhileTcp: false, tcpBytesAtFirstUdp: null, tcpBytesAtLastUdp: 0,
    oversizeRequests: 0, oversizeReturns: 0, sources: new Set(), error: false };
  const sockets = new Set();
  const server = createServer(socket => {
    state.accepts++; state.active++; sockets.add(socket);
    socket.on('close', () => { state.active--; sockets.delete(socket); });
    socket.on('error', () => {});
    socket.on('data', bytes => { state.tcpBytes += bytes.length; }); socket.pipe(socket);
  });
  await new Promise((resolve, reject) => {
    server.once('error', () => reject(new Error('owned_tcp_fixture_bind_failed')));
    server.listen({ host: '127.0.0.1', port: 43001, exclusive: true }, resolve);
  });
  t.after(async () => {
    for (const socket of sockets) socket.destroy();
    await new Promise(resolve => server.close(resolve));
  });
  const udp = await bindUdp(port); t.after(() => closeUdp(udp));
  udp.on('error', () => { state.error = true; });
  udp.on('message', (bytes, source) => {
    state.udpRequests++; state.udpBytes += bytes.length; state.sources.add(source.port);
    if (state.tcpBytesAtFirstUdp === null) state.tcpBytesAtFirstUdp = state.tcpBytes;
    state.tcpBytesAtLastUdp = state.tcpBytes;
    if (state.active) state.udpWhileTcp = true;
    if (bytes.length > 4096) state.oversizeRequests++;
    const response = bytes.length === 2 && bytes[0] === 0xdd ? Buffer.alloc(4097, bytes[1]) : bytes;
    if (response.length === 4097) state.oversizeReturns++;
    udp.send(response, source.port, source.address, error => { if (error) state.error = true; });
  });
  return { state, server, udp };
}
async function peer(signal) {
  const socket = await bindUdp(0); const received = []; let error = false;
  const abort = () => { closeUdp(socket); };
  signal.addEventListener('abort', abort, { once: true });
  socket.on('error', () => { error = true; });
  socket.on('message', (bytes, source) => {
    if (source.address !== '127.0.0.1' || source.port !== 35554 || received.length >= 20) error = true;
    else received.push(Buffer.from(bytes));
  });
  const send = bytes => new Promise((resolve, reject) => {
    socket.send(bytes, 35554, '127.0.0.1', error => error ? reject(new Error('fixture_udp_send_failed')) : resolve());
  });
  return {
    async exchange(bytes) {
      const before = received.length; await send(bytes); const deadline = performance.now() + 3000;
      while (received.length === before && performance.now() < deadline && !error && !signal.aborted) await delay(5);
      assert.equal(error, false); assert.equal(received.length, before + 1, 'one_return_to_its_origin');
      assert.deepEqual(received[before], bytes);
    },
    async reject(bytes) {
      const before = received.length; await send(bytes); await delay(500);
      assert.equal(error, false); assert.equal(received.length, before, 'oversize_must_not_return');
    },
    async close() { signal.removeEventListener('abort', abort); await closeUdp(socket); },
  };
}
async function tcpTransfer(port, signal, onConnected) {
  const socket = createConnection({ host: '127.0.0.1', port, signal });
  const payload = Buffer.alloc(1024 * 1024, 0x79); let bytes = 0; let invalid = false;
  let timer;
  const closed = new Promise(resolve => socket.once('close', resolve));
  try {
    const completed = new Promise((resolve, reject) => {
      timer = setTimeout(() => reject(new Error('fixture_tcp_deadline')), 5000);
      socket.once('error', () => reject(new Error('fixture_tcp_io')));
      socket.on('data', chunk => {
        if (bytes + chunk.length > payload.length || !chunk.equals(payload.subarray(bytes, bytes + chunk.length))) invalid = true;
        bytes += chunk.length;
      });
      socket.once('end', resolve);
    });
    completed.catch(() => {});
    await new Promise((resolve, reject) => {
      socket.once('connect', resolve); socket.once('error', () => reject(new Error('fixture_tcp_connect')));
    });
    onConnected();
    for (let offset = 0; offset < payload.length; offset += 16384) {
      if (signal.aborted) throw new Error('fixture_cancelled');
      socket.write(payload.subarray(offset, offset + 16384)); await delay(16);
    }
    socket.end(); await completed;
    assert.equal(invalid, false); assert.equal(bytes, payload.length);
  } finally { clearTimeout(timer); socket.destroy(); await closed; }
}
function reaped(cleanup) {
  assert.equal(cleanup.agents, 2); assert.equal(cleanup.closed, 2); assert.equal(cleanup.forced, false);
  assert.equal(cleanup.stderrBytes, 0); assert.ok(cleanup.stdoutBytes > 0);
  assert.equal(cleanup.transport.event, 'fragmented_transport_snapshot');
  assert.equal(cleanup.transport.profile, 'fragmentedV2');
  assert.equal(cleanup.transport.evidence, 'transport_counters_only_not_minecraft_or_voice');
}

for (const [name, port, tunnel] of [['SVC', 43002, withSvcFragmentedTunnel], ['Plasmo', 43003, withPlasmoFragmentedTunnel]]) {
  test(`${name} V2 real pair: 4096-byte originals, isolated returns, 4097 rejection and TCP overlap`, { skip: !windows }, async t => {
    const f = await fixture(t, port);
    let localTcp;
    const result = await tunnel(async context => {
      localTcp = context.bridgePort;
      assert.equal(context.profile, 'fragmentedV2'); assert.equal(context.voicePort, 35554);
      assert.equal(context.approvedOriginalDatagramBytes, 4096); assert.equal(context.maxFragmentFrameBytes, 1264);
      assert.equal(context.associationCeilingNegotiated, true); assert.equal(context.capacityScope, 'initialAssociation');
      await delay(20); assert.equal(f.state.accepts, 0); assert.equal(f.state.udpRequests, 0);
      const peers = [await peer(context.signal), await peer(context.signal)];
      let tcp; let started;
      const connected = new Promise(resolve => { started = resolve; });
      try {
        tcp = tcpTransfer(context.bridgePort, context.signal, started); tcp.catch(() => started());
        await connected;
        await Promise.all(peers.map(async (client, index) => {
          for (const size of [1427, 2108, 4096]) await client.exchange(Buffer.alloc(size, index + 0x31));
          await client.reject(Buffer.alloc(4097, index + 0xa1));
          await client.reject(Buffer.from([0xdd, index + 0xb1]));
          await client.exchange(Buffer.alloc(64, index + 0x61));
        }));
        await tcp;
      } finally { await Promise.all(peers.map(client => client.close())); if (tcp) await tcp; }
      return 'opaque_bytes_only';
    }, { timeoutMs: 30000 });
    assert.equal(result.value, 'opaque_bytes_only'); reaped(result.cleanup);
    assert.equal(f.state.error, false); assert.equal(f.state.sources.size, 2); assert.equal(f.state.udpWhileTcp, true);
    assert.ok(f.state.tcpBytesAtLastUdp > f.state.tcpBytesAtFirstUdp, 'TCP progressed between first and last delivered UDP');
    assert.equal(f.state.tcpBytes, 1024 * 1024); assert.equal(f.state.udpRequests, 10);
    assert.equal(f.state.oversizeRequests, 0); assert.equal(f.state.oversizeReturns, 2);
    assert.ok(result.cleanup.transport.datagramFramesTxSinceReady >= 16);
    assert.ok(result.cleanup.transport.datagramFramesRxSinceReady >= 16);
    const udp = await bindUdp(35554); await closeUdp(udp);
    const tcp = createServer(); await new Promise((resolve, reject) => {
      tcp.once('error', reject); tcp.listen({ host: '127.0.0.1', port: localTcp, exclusive: true }, resolve);
    });
    await new Promise(resolve => tcp.close(resolve));
    assert.equal(f.server.listening, true); assert.equal(f.udp.address().port, port);
  });

  test(`${name} V2 abort after actual UDP traffic reaps both agents and preserves owned endpoint`, { skip: !windows }, async t => {
    const f = await fixture(t, port); const controller = new AbortController();
    await assert.rejects(tunnel(async context => {
      const client = await peer(context.signal);
      try {
        await client.exchange(Buffer.alloc(1427, 0x68));
        const aborted = new Promise(resolve => context.signal.addEventListener('abort', resolve, { once: true }));
        controller.abort('PRIVATE_SENTINEL'); await aborted;
      } finally { await client.close(); }
    }, { signal: controller.signal, timeoutMs: 15000 }), error => {
      assert.ok(error instanceof GameTunnelError); assert.equal(error.code, 'game_tunnel_cancelled'); reaped(error.cleanup);
      assert.doesNotMatch(JSON.stringify(error), /PRIVATE_SENTINEL|127\.0\.0\.1|peer|session/); return true;
    });
    assert.equal(f.state.udpRequests, 1); assert.equal(f.server.listening, true);
    const udp = await bindUdp(35554); await closeUdp(udp);
  });
}
