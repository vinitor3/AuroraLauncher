import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';

const require = createRequire(import.meta.url);
const lock = JSON.parse(await readFile(new URL('package-lock.json', import.meta.url), 'utf8'));

test('isolated dependency lock pins protocol/data, licenses, registry integrity and patched UUID', () => {
  assert.equal(lock.packages['node_modules/minecraft-protocol'].version, '1.68.0');
  assert.equal(lock.packages['node_modules/minecraft-data'].version, '3.109.0');
  assert.equal(lock.packages['node_modules/uuid'].version, '11.1.1');
  for (const [path, entry] of Object.entries(lock.packages)) {
    if (!path) continue;
    assert.match(entry.resolved, /^https:\/\/registry\.npmjs\.org\//);
    assert.match(entry.integrity, /^sha512-/);
    assert.equal(entry.hasInstallScript, undefined);
    assert.ok(['MIT', 'BSD-2-Clause', 'BSD-3-Clause', 'Apache-2.0', 'ISC', 'CC0-1.0'].includes(entry.license));
    if (path.endsWith('/uuid')) assert.equal(entry.version, '11.1.1');
  }
});

test('UUID override retains CommonJS v4 API for both callers and rejects the advisory buffer bounds case', () => {
  for (const caller of ['yggdrasil', '@azure/msal-node']) {
    const callerRequire = createRequire(require.resolve(caller));
    const uuid = callerRequire('uuid');
    assert.equal(uuid.validate(uuid.v4()), true);
    assert.throws(() => uuid.v3('fixture', uuid.v3.DNS, new Uint8Array(8), 4), RangeError);
    assert.throws(() => uuid.v5('fixture', uuid.v5.DNS, new Uint8Array(8), 4), RangeError);
    assert.throws(() => uuid.v6({}, new Uint8Array(8), 4), RangeError);
  }
  const { CryptoProvider } = require('@azure/msal-node');
  assert.equal(require('uuid').validate(new CryptoProvider().createNewGuid()), true);
});

test('pinned protocol implementation and 1.21.1 data load without a client, socket or account', () => {
  const protocol = require('minecraft-protocol');
  const data = require('minecraft-data')('1.21.1');
  assert.equal(data.version.version, 767);
  assert.equal(typeof protocol.createClient, 'function');
  assert.equal(typeof protocol.Client, 'function');
  const client = new protocol.Client(false, '1.21.1', undefined, true);
  assert.equal(client.ended, true);
  assert.equal(client.socket, undefined);
  assert.equal(client.state, 'handshaking');
  assert.equal(client.compressor, null);
  client.serializer.destroy(); client.deserializer.destroy();
  client.splitter.destroy(); client.framer.destroy();
});
