import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { PlayEvidence } from './play-evidence.mjs';
import { observeTwoPlayers } from './play.mjs';
import { PLAY_NAMES, offlineUuid } from '../connect-runtime-tools/play-profile.mjs';

const flags = Object.fromEntries(['x', 'y', 'z', 'yaw', 'pitch'].map(key => [key, false]));
const position = { x: 1, y: 64, z: 2, yaw: 0, pitch: 0, flags, teleportId: 4 };
function loggedIn(index = 0) {
  const e = new PlayEvidence(index);
  e.observe('success', { username: PLAY_NAMES[index], uuid: offlineUuid(PLAY_NAMES[index]) }, () => {});
  e.observe('finish_configuration', {}, () => {});
  e.observe('login', { entityId: 1 }, () => {});
  return e;
}
test('login/configuration/play alone cannot imply world or another participant', () => {
  const e = loggedIn(); assert.equal(e.ready, false);
  assert.equal(e.summary().chunks, 0); assert.equal(e.summary().peerSpawn, false);
});
test('each participant requires position, chunk/batch and independently observed peer identity/spawn', () => {
  for (const index of [0, 1]) {
    const e = loggedIn(index); const writes = [];
    const write = (name, data) => writes.push([name, data]);
    e.observe('position', position, write);
    e.observe('map_chunk', { chunkData: Buffer.from([1]) }, write);
    e.observe('chunk_batch_finished', { batchSize: 1 }, write);
    e.observe('player_info', { action: { add_player: true }, data: [{ uuid: offlineUuid(PLAY_NAMES[1 - index]) }] }, write);
    assert.equal(e.ready, false);
    e.observe('spawn_entity', { objectUUID: offlineUuid(PLAY_NAMES[1 - index]), type: 128, x: 2, y: 64, z: 3 }, write);
    assert.equal(e.ready, true);
    assert.deepEqual(writes.map(row => row[0]), ['teleport_confirm', 'position_look', 'chunk_batch_received']);
    const json = JSON.stringify(e.summary());
    assert.ok(!json.includes(PLAY_NAMES[index]) && !json.includes('uuid') && !json.includes('chunkData') && !json.includes('teleportId'));
  }
});
test('relative teleports add only flagged fields and acknowledgements use the received ID', () => {
  const e = loggedIn(); const writes = []; const write = (name, data) => writes.push([name, data]);
  assert.throws(() => e.observe('position', { ...position, flags: { ...flags, x: true } }, write), /client_position_base_unknown/);
  assert.equal(writes.length, 0);
  e.observe('position', position, write);
  e.observe('position', { ...position, x: 3, y: 65, yaw: 90, flags: { ...flags, x: true, yaw: true }, teleportId: 5 }, write);
  assert.deepEqual(writes[2], ['teleport_confirm', { teleportId: 5 }]);
  assert.deepEqual(writes[3], ['position_look', { x: 4, y: 65, z: 2, yaw: 90, pitch: 0, onGround: false }]);
});
test('invalid identity/order/position/chunk and server disconnect refuse evidence without exposing text', () => {
  const e = new PlayEvidence(0);
  assert.throws(() => e.observe('success', { username: 'PRIVATE_NAME', uuid: 'PRIVATE_UUID' }), /client_login_identity_mismatch/);
  assert.throws(() => e.observe('login', { entityId: 1 }), /client_evidence_order/);
  const active = loggedIn();
  for (const invalid of [{ ...position, x: NaN }, { ...position, flags: {} }, { ...position, teleportId: -1 }]) {
    assert.throws(() => active.observe('position', invalid, () => assert.fail('must not send')), /client_position_invalid/);
  }
  assert.throws(() => active.observe('map_chunk', { chunkData: Buffer.alloc(0) }), /client_chunk_invalid/);
  assert.throws(() => active.observe('disconnect', { reason: 'PRIVATE_REASON' }), /client_server_disconnect/);
});
test('packet count has an explicit bound independent from wire bytes', () => {
  const e = new PlayEvidence(0);
  for (let i = 0; i < 65536; i++) e.observe('fixture', {});
  assert.throws(() => e.observe('fixture', {}), /client_packet_count_limit/);
});
test('pair refuses shared/invalid listeners and pre-cancel before creating a client', async () => {
  for (const ports of [[43001, 43001], [0, 1], ['1', 2], [1], undefined]) {
    await assert.rejects(observeTwoPlayers(ports), /client_pair_arguments/);
  }
  const controller = new AbortController(); controller.abort();
  await assert.rejects(observeTwoPlayers([43010, 43011], { signal: controller.signal }), { name: 'AbortError' });
});
test('pair failure on real local wire closes both owned connections and returns no partial success', { timeout: 5000 }, async t => {
  const servers = []; const sockets = new Set();
  t.after(async () => { for (const socket of sockets) socket.destroy();
    await Promise.all(servers.map(server => new Promise(resolve => server.close(resolve)))); });
  for (let i = 0; i < 2; i++) {
    const server = createServer(socket => { sockets.add(socket); socket.on('close', () => sockets.delete(socket));
      socket.on('error', () => {}); socket.on('data', () => {}); socket.write(Buffer.from([3, 3, 0x80, 2])); });
    servers.push(server); server.listen(0, '127.0.0.1'); await once(server, 'listening');
  }
  await assert.rejects(observeTwoPlayers(servers.map(server => server.address().port)), error => {
    assert.equal(error.message, 'client_protocol_refused');
    assert.equal(error.observation.length, 2);
    assert.ok(error.observation.every(row => row.playAndWorldPacketsObserved === false));
    assert.ok(!JSON.stringify(error.observation).includes(PLAY_NAMES[0]));
    return true;
  });
  // close in our client has completed; the fixture's TCP close notification may
  // be delivered on the next event-loop iteration.
  await new Promise(resolve => setTimeout(resolve, 20)); assert.equal(sockets.size, 0);
});
