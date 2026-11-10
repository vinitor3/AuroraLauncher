import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { createServer } from 'node:net';
import { resolve } from 'node:path';
import { childEnvironment, checkEnvironment, runOperator, parseArguments, loadProductionSdk, onlineManager,
  mutationTracker, fixedTcpRoundtrip, rebind, redactedError, SmokeError, runSmoke, withdrawalObservation } from './connect-private-online-smoke.mjs';

test('online smoke requires exact opt-in, binary hash and no destination or credential argv', () => {
  const valid = ['--check', '--binary', resolve('fixture.exe'), '--sha256', 'a'.repeat(64)];
  assert.equal(parseArguments(valid).run, false);
  assert.equal(parseArguments(['--run', ...valid.slice(1)]).run, true);
  for (const args of [[], [...valid, '--origin', 'https://example.invalid'], [...valid, '--token', 'not-a-token'],
    ['--run', '--binary', 'relative.exe', '--sha256', 'a'.repeat(64)], ['--run', ...valid.slice(1, 4), 'A'.repeat(64)]])
    assert.throws(() => parseArguments(args), /ARGUMENTS_INVALID/);
});
test('native children receive no credential, debug or emulator/environment authority overrides', () => {
  const env = childEnvironment({ PATH: 'owned-path', SystemRoot: 'windows', TEMP: 'temp', RUST_LOG: 'trace', RUST_BACKTRACE: 'full',
    FIREBASE_AUTH_EMULATOR_HOST: 'localhost:9187', AURORA_SOCIAL_LAB_PIN: 'foreign', AURORA_SOCIAL_ORIGIN: 'http://evil',
    AURORA_SOCIAL_PRIVATE_LAB: '1', SECRET_PASSWORD: 'never-forward', NODE_OPTIONS: '--inspect' });
  assert.deepEqual(Object.keys(env).sort(), ['AURORA_SOCIAL_DESKTOP_FIXTURE', 'AURORA_SOCIAL_DESKTOP_ONLINE_FIXTURE', 'PATH', 'SystemRoot', 'TEMP'].sort());
  assert.equal(env.AURORA_SOCIAL_DESKTOP_ONLINE_FIXTURE, '1');
  assert.throws(() => checkEnvironment({ FIRESTORE_EMULATOR_HOST: 'localhost:8187' }), /EMULATOR_ENVIRONMENT_FORBIDDEN/);
  assert.throws(() => checkEnvironment({ NODE_TLS_REJECT_UNAUTHORIZED: '0' }), /TLS_VERIFICATION_REQUIRED/);
  checkEnvironment({});
  assert.throws(() => runOperator({}), /OPERATOR_REQUIRED/);
  assert.throws(() => runOperator({ AURORA_CONNECT_TEST_OPERATOR: 'not an email' }), /OPERATOR_REQUIRED/);
  assert.equal(runOperator({ AURORA_CONNECT_TEST_OPERATOR: 'operator@example.invalid' }), 'operator@example.invalid');
});
test('real production SDK configuration and private native callback are preserved without network', async () => {
  const sdk = await loadProductionSdk();
  assert.equal(sdk.privateConfiguration(sdk.config), true);
  for (const change of [{ projectId: 'demo-aurora-social' }, { workerUrl: 'https://example.invalid' }, { authDomain: 'example.invalid' }])
    assert.equal(sdk.privateConfiguration({ ...sdk.config, ...change }), false);
  let capability = true;
  const services = { auth: { currentUser: { uid: 'owned-test-uid' } }, db: {}, config: sdk.config, authScope: { id: 'owned', persistent: false } };
  const service = sdk.factory(services, { privateOnlineCapability: () => capability });
  assert.equal(service.privateInvitations.environment, 'online'); capability = false;
  await assert.rejects(service.privateInvitations.snapshot(), error => error.code === 'join_unavailable');
  assert.equal(sdk.factory(services).privateInvitations, undefined);
});
function childFixture(ready) {
  const child = new EventEmitter(); child.stdin = new PassThrough(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
  const commands = []; let ended = false;
  const emit = value => child.stdout.write(`SOCIAL89_JSON:${JSON.stringify(value)}\n`);
  child.stdin.on('data', chunk => {
    const request = JSON.parse(chunk.toString()); commands.push(request);
    emit({ ok: true, value: request.command === 'quit' ? { cleanup: true } : { observed: true } });
  });
  child.stdin.on('finish', () => { if (!ended) { ended = true; child.emit('exit', 0, null); child.stdout.end(); } });
  child.kill = () => { ended = true; child.emit('exit', null, 'forced'); };
  queueMicrotask(() => emit(ready));
  return { child, commands };
}
test('native manager verifies compiled capability before accepting any credential and closes its own child', async () => {
  let created, options, args;
  const manager = await onlineManager('fixture.exe', (_binary, a, o) => { args = a; options = o;
    created = childFixture({ event: 'ready', mode: 'compiledOnline', capabilities: { privateInvitations: true } }); return created.child; });
  const token = 'RAM-only-fixture-token'; await manager.ok('authenticate', { token, operationId: 'own' });
  assert.ok(!JSON.stringify(args).includes(token)); assert.ok(!JSON.stringify(options.env).includes(token));
  assert.equal(created.commands[0].token, token);
  assert.deepEqual(await manager.close(), { cleanup: true, forced: false });
  assert.equal(created.commands.at(-1).command, 'quit');
});
test('legacy or missing private native capability is rejected and reaped before registration', async () => {
  for (const ready of [{ event: 'ready', capabilities: { privateInvitations: true } },
    { event: 'ready', mode: 'compiledOnline', capabilities: { privateInvitations: false } }]) {
    let created;
    await assert.rejects(onlineManager('fixture.exe', () => { created = childFixture(ready); return created.child; }), /NATIVE_ONLINE_GATE_REQUIRED/);
    assert.deepEqual(created.commands, [{ command: 'quit' }]);
  }
});
test('an unexpected process exit cannot be reported as confirmed native cleanup', async () => {
  let created;
  const manager = await onlineManager('fixture.exe', () => {
    created = childFixture({ event: 'ready', mode: 'compiledOnline', capabilities: { privateInvitations: true } }); return created.child;
  });
  created.child.emit('exit', 0, null);
  await assert.rejects(manager.close(), /NATIVE_CLEANUP_FAILED/);
});
test('timed-out mutation remains tracked until original completion, preventing early cleanup', async () => {
  const tracker = mutationTracker(); let finish, recorded = false;
  const pending = new Promise(resolve => { finish = resolve; });
  await assert.rejects(tracker.run(async () => { await pending; recorded = true; }, 2), /SDK_OPERATION_TIMEOUT/);
  await assert.rejects(tracker.settled(2), /SDK_MUTATIONS_UNSETTLED/); assert.equal(recorded, false);
  finish(); assert.equal(await tracker.settled(100), true); assert.equal(recorded, true);
});
test('actual local TCP probe checks one MiB exact data and EOF, then listener can rebind', async () => {
  let bytes = 0; const server = createServer({ allowHalfOpen: true }, socket => {
    socket.on('data', chunk => { assert.ok(chunk.every(value => value === 0xa5)); bytes += chunk.length; }); socket.pipe(socket);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve)); const address = `127.0.0.1:${server.address().port}`;
  try { assert.deepEqual(await fixedTcpRoundtrip(address), { tcpBytesEachDirection: 1048576 }); assert.equal(bytes, 1048576); }
  finally { await new Promise(resolve => server.close(resolve)); }
  await rebind(address);
  await assert.rejects(fixedTcpRoundtrip('192.0.2.1:1234'), /TCP_PROBE_INVALID/);
});
test('TCP probe rejects corrupted return bytes without accepting compilation as proof', async () => {
  const server = createServer(socket => { socket.on('error', () => {}); socket.once('data', () => socket.end(Buffer.from([0]))); });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try { await assert.rejects(fixedTcpRoundtrip(`127.0.0.1:${server.address().port}`), /TCP_ECHO_MISMATCH/); }
  finally { await new Promise(resolve => server.close(resolve)); }
});
test('permission preflight failure creates neither managers nor identities and exposes no raw error', async () => {
  let created = 0;
  const owner = { async preflight() { throw new Error('private token must not escape'); }, result: () => ({ created: 0 }) };
  const result = await runSmoke({ binary: 'none', sha256: 'a'.repeat(64), owner, sdk: {}, createManager: () => { created++; } });
  assert.equal(created, 0); assert.equal(result.passed, false); assert.equal(result.code, 'SMOKE_OPERATION_FAILED');
  assert.ok(!JSON.stringify(result).includes('token must'));
  assert.equal(redactedError(new SmokeError('NATIVE_CLEANUP_FAILED')), 'NATIVE_CLEANUP_FAILED');
});
test('native cleanup failure is carried into ownership gate and forbids Auth deletion', async () => {
  let calls = 0, received;
  const owner = { async preflight() {}, async snapshotDocuments() {}, async cleanupDocuments(value) { received = value; throw new Error('denied'); }, result: () => ({}) };
  const result = await runSmoke({ binary: 'none', sha256: 'a'.repeat(64), owner, sdk: {}, createManager: async () => {
    if (++calls === 2) throw new SmokeError('NATIVE_START_FAILED');
    return { close: async () => { throw new SmokeError('NATIVE_CLEANUP_FAILED'); } };
  } });
  assert.equal(result.cleanupPassed, false); assert.equal(result.code, 'NATIVE_START_FAILED');
  assert.deepEqual(received, { sessionStopped: false, pendingMutationsSettled: true });
});
test('signup with an uncertain response is journaled before I/O and never claims complete cleanup', async () => {
  const steps = []; let deletes = 0;
  const owner = { accounts: [{ role: 'host', nickname: 'owned', email: 'owned@aurora.internal' }], async preflight() {},
    async recordAccountCreateStarted(role) { steps.push(`intent:${role}`); }, async recordAccount() { assert.fail('no identity returned'); },
    async snapshotDocuments() {}, async cleanupDocuments() { throw Object.assign(new Error('unconfirmed'), { code: 'owned_cleanup_account_unconfirmed' }); },
    result: () => ({ complete: false, unconfirmedAccounts: 1 }) };
  const sdk = { config: {}, app: { initializeApp: () => ({}), deleteApp: async () => {} },
    auth: { inMemoryPersistence: {}, initializeAuth: () => ({}), async createUserWithEmailAndPassword() { steps.push('signup'); throw new Error('network response lost'); },
      async deleteUser() { deletes++; } }, firestore: { getFirestore: () => ({}), terminate: async () => {} } };
  const result = await runSmoke({ binary: 'none', sha256: 'a'.repeat(64), owner, sdk,
    createManager: async () => ({ ready: { capabilities: { privateInvitations: true } }, close: async () => {} }) });
  assert.deepEqual(steps, ['intent:host', 'signup']); assert.equal(deletes, 0);
  assert.equal(result.passed, false); assert.equal(result.cleanupPassed, false); assert.equal(result.cleanupCode, 'AUTH_CREATION_RESULT_UNCERTAIN');
});
test('closure evidence requires original healthy host and confirmed revoked server record without claiming exclusive causality', () => {
  const original = { operationId: 'same', phase: 'hostListening', errorCode: null,
    social: { sessionId: 'original', visibility: 'private', participants: [{ accountId: 'recipient', state: 'connected' }] } };
  const input = { operationId: 'same', sessionId: 'original', guestUid: 'recipient', beforeHost: original,
    afterHost: { ...original, social: { ...original.social, participants: [] } },
    closedGuest: { phase: 'failed', cleanupState: 'complete', tcpAddress: null, errorCode: 'connect_unauthorized' },
    privateView: { sessionId: 'original', outgoing: [{ person: { uid: 'recipient' }, status: 'revoked' }] } };
  const result = withdrawalObservation(input);
  assert.equal(result.causality, 'not_proven_by_terminal_reason'); assert.equal(result.originalHostStillReady, true);
  for (const afterHost of [{ ...input.afterHost, phase: 'failed', errorCode: 'connect_expired' },
    { ...input.afterHost, operationId: 'new' }, { ...input.afterHost, social: { ...input.afterHost.social, sessionId: 'new' } }, original])
    assert.throws(() => withdrawalObservation({ ...input, afterHost }), /WITHDRAWAL_EVIDENCE_INCOMPLETE/);
  assert.throws(() => withdrawalObservation({ ...input, privateView: { sessionId: 'original', outgoing: [{ person: { uid: 'recipient' }, status: 'active' }] } }), /WITHDRAWAL_EVIDENCE_INCOMPLETE/);
  assert.throws(() => withdrawalObservation({ ...input, closedGuest: { ...input.closedGuest, tcpAddress: '127.0.0.1:1234' } }), /WITHDRAWAL_EVIDENCE_INCOMPLETE/);
});
