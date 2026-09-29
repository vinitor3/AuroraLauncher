import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { observeTwoPlayers } from './play.mjs';

test('invalid pair deadlines fail before dialing', async () => {
  for (const timeoutMs of [0, -1, 60001, NaN, 1.1, '1000']) {
    await assert.rejects(observeTwoPlayers([1, 2], { timeoutMs }), /client_pair_arguments/);
  }
});

test('a deadline and redacted partial evidence survive forced cleanup of nonclosing peers', { timeout: 6000 }, async t => {
  const sockets = new Set(); const servers = [];
  t.after(async () => {
    for (const socket of sockets) socket.destroy();
    await Promise.all(servers.map(server => new Promise(resolve => server.close(resolve))));
  });
  for (let index = 0; index < 2; index++) {
    const server = createServer({ allowHalfOpen: true }, socket => {
      sockets.add(socket); socket.on('error', () => {}); socket.on('data', () => {});
      socket.on('close', () => sockets.delete(socket));
    });
    servers.push(server); server.listen(0, '127.0.0.1'); await once(server, 'listening');
  }
  await assert.rejects(observeTwoPlayers(servers.map(server => server.address().port), { timeoutMs: 1000 }), error => {
    assert.equal(error.message, 'client_pair_deadline');
    assert.equal(error.observation.length, 2);
    assert.ok(error.observation.every(item => item.loginIdentity === false && item.frames === 0));
    assert.deepEqual(error.cleanup, [{ closed: true, forced: true }, { closed: true, forced: true }]);
    assert.doesNotMatch(JSON.stringify(error), /127\.0\.0\.1|AuroraLab|uuid|stack/);
    return true;
  });
});
