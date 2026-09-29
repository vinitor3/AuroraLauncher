import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer, Socket } from 'node:net';
import { once } from 'node:events';
import { createRequire } from 'node:module';
import { GuardedClient, createOfflineClient, closeOfflineClient } from './guarded-client.mjs';
import { PLAY_NAMES, offlineUuid } from '../connect-runtime-tools/play-profile.mjs';

const require = createRequire(import.meta.url);
test('synthetic whitelist UUIDs agree with the independently pinned protocol implementation', () => {
  const { nameToMcOfflineUUID } = require('minecraft-protocol/src/datatypes/uuid');
  for (const name of PLAY_NAMES) assert.equal(offlineUuid(name), nameToMcOfflineUUID(name));
});

test('fixed client target refuses arbitrary identity and invalid ports before socket creation', () => {
  for (const port of [0, -1, 65536, 1.1, '43001', NaN]) {
    assert.throws(() => createOfflineClient(port, PLAY_NAMES[0]), /client_invalid_target/);
  }
  assert.throws(() => createOfflineClient(43001, 'not_a_fixture_identity'), /client_invalid_target/);
});

test('packet gate prevents generic and internal specific handlers before compression/encryption/transfer/terms', async () => {
  for (const packet of ['compress', 'set_compression', 'encryption_begin', 'transfer',
    'resource_pack_send', 'add_resource_pack', 'code_of_conduct', 'start_configuration']) {
    const client = new GuardedClient(false, '1.21.1', undefined, true);
    const errors = []; let observed = 0;
    client.on('error', error => errors.push(error.message));
    client.on('packet', () => observed++); client.on(packet, () => observed++);
    client.emit('packet', { privateData: 'never logged' }, { name: packet });
    client.emit(packet, { threshold: 1, host: 'must not dial' });
    assert.deepEqual(errors, ['client_protocol_refused']); assert.equal(observed, 0);
    assert.equal(client.compressor, null); assert.equal(client.decompressor, null); assert.equal(client.cipher, null);
    await closeOfflineClient(client);
  }
});

test('direct compression/encryption installation is refused and errors are redacted', async () => {
  const client = new GuardedClient(false, '1.21.1', undefined, true); const errors = [];
  client.on('error', error => errors.push(error.message));
  client.compressionThreshold = 1; client.setEncryption(Buffer.alloc(16));
  client.emit('error', new Error('PRIVATE_PACKET_AND_PATH'));
  assert.deepEqual(errors, ['client_protocol_refused', 'client_protocol_failed']);
  assert.equal(client.compressor, null); assert.equal(client.cipher, null);
  await closeOfflineClient(client);
});

test('synchronous packet handler exceptions become redacted failure and cannot escape the event loop', async () => {
  const client = new GuardedClient(false, '1.21.1', undefined, true); const errors = [];
  client.on('error', error => errors.push(error.message));
  client.on('player_info', () => { throw new Error('PRIVATE_DER_AND_PAYLOAD'); });
  assert.doesNotThrow(() => client.emit('player_info', {}));
  assert.deepEqual(errors, ['client_protocol_handler_failed']);
  assert.equal(client.protocolRejected, true);
  client.on('error', () => { throw new Error('PRIVATE_ERROR_LISTENER'); });
  assert.doesNotThrow(() => client.emit('error', new Error('PRIVATE_UPSTREAM')));
  await closeOfflineClient(client);
});

async function fixture(t, bytes) {
  const sockets = new Set(); let accepted = 0;
  const server = createServer(socket => {
    accepted++; sockets.add(socket); socket.on('close', () => sockets.delete(socket));
    socket.on('error', () => {}); socket.on('data', () => {});
    socket.write(bytes);
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  t.after(async () => { const done = new Promise(resolve => server.close(resolve));
    for (const socket of sockets) socket.destroy(); await done; });
  return { port: server.address().port, accepted: () => accepted };
}

test('actual local TCP compression request is blocked before any inflater or encryption is installed', { timeout: 5000 }, async t => {
  // LOGIN clientbound compress: frame length 3, packet ID 3, threshold 256.
  const server = await fixture(t, Buffer.from([3, 3, 0x80, 2]));
  const client = createOfflineClient(server.port, PLAY_NAMES[0]);
  const error = await new Promise(resolve => client.once('error', resolve));
  assert.equal(error.message, 'client_protocol_refused');
  const result = await closeOfflineClient(client);
  assert.equal(result.closed, true); assert.equal(server.accepted(), 1);
  assert.equal(client.compressor, null); assert.equal(client.decompressor, null);
  assert.equal(client.frameGuard.stats.forwardedFrames, 1);
});

test('actual local TCP oversize is refused before the library splitter receives bytes', { timeout: 5000 }, async t => {
  const server = await fixture(t, Buffer.from([0x80, 0x80, 0x80, 1]));
  const client = createOfflineClient(server.port, PLAY_NAMES[1]);
  const error = await new Promise(resolve => client.once('error', resolve));
  assert.equal(error.message, 'client_protocol_failed');
  const result = await closeOfflineClient(client);
  assert.equal(result.closed, true); assert.equal(client.frameGuard.stats.forwardedFrames, 0);
  assert.equal(client.frameGuard.stats.peakFrameAllocation, 0);
});

test('complete wire frames containing partial packets or trailing bytes fail before internal handlers', { timeout: 5000 }, async t => {
  for (const bytes of [Buffer.from([1, 3]), Buffer.from([4, 3, 0x80, 2, 0])]) {
    const server = await fixture(t, bytes);
    const client = createOfflineClient(server.port, PLAY_NAMES[0]); let decoded = 0;
    client.on('packet', () => decoded++);
    const error = await new Promise(resolve => client.once('error', resolve));
    assert.equal(error.message, 'client_protocol_failed');
    await closeOfflineClient(client);
    assert.equal(decoded, 0); assert.equal(client.compressor, null);
    assert.equal(client.frameGuard.stats.forwardedFrames, 1);
  }
});

test('normal close flushes queued frames before FIN and disables later protocol writes', { timeout: 5000 }, async t => {
  let received = 0; let accepted;
  const server = createServer(socket => {
    accepted = socket; socket.on('error', () => {});
    socket.on('data', bytes => { received += bytes.length; });
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  t.after(async () => { accepted?.destroy(); await new Promise(resolve => server.close(resolve)); });
  const client = new GuardedClient(false, '1.21.1', undefined, true);
  const errors = []; client.on('error', error => errors.push(error.message));
  const socket = new Socket(); client.setSocket(socket);
  socket.connect(server.address().port, '127.0.0.1'); await once(socket, 'connect');
  client.framer.unpipe(socket);
  client.writeRaw(Buffer.alloc(65536, 0x42));
  const closed = closeOfflineClient(client);
  assert.equal(client.serializer.writable, false);
  client.write('set_protocol', { protocolVersion: 767, serverHost: 'ignored', serverPort: 1, nextState: 1 });
  client.framer.pipe(socket);
  assert.deepEqual(await closed, { closed: true, forced: false });
  assert.equal(received, 65539); // canonical three-byte length plus payload
  assert.deepEqual(errors, []);
});
