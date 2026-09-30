import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { BoundarySmokeError, decodeBoundaryEvent, runBoundarySmoke, validateBoundaryReady } from './boundary-smoke.mjs';

test('default and invalid options cannot authorize starting the local fixture', async () => {
  for (const options of [{}, { authorized: false }]) await assert.rejects(runBoundarySmoke(options), { code: 'boundary_explicit_opt_in_required' });
  for (const options of [null, [], { authorized: true, executable: 'PRIVATE_PATH' }, { authorized: true, signal: {} },
    { authorized: true, timeoutMs: 0 }, { authorized: true, timeoutMs: 60001 }, { authorized: true, timeoutMs: 1.5 }]) {
    await assert.rejects(runBoundarySmoke(options), error => {
      assert(error instanceof BoundarySmokeError); assert.equal(error.code, 'boundary_invalid_options');
      assert(!JSON.stringify(error).includes('PRIVATE_PATH')); return true;
    });
  }
});

test('CLI requires the single explicit local opt-in and redacts private arguments', () => {
  for (const args of [[], ['--config', 'PRIVATE_CONFIG'], ['--local-lab', '--extra']]) {
    let failure;
    try { execFileSync(process.execPath, [fileURLToPath(new URL('./boundary-smoke.mjs', import.meta.url)), ...args], { stdio: 'pipe' }); }
    catch (error) { failure = error; }
    assert.equal(failure.status, 1); assert.equal(failure.stdout.length, 0);
    assert.deepEqual(JSON.parse(failure.stderr), { result: 'FAIL', code: 'boundary_explicit_opt_in_required' });
  }
});

test('event decoder refuses oversized, malformed, non-UTF8 and non-record data', () => {
  for (const bytes of [Buffer.alloc(8193), Buffer.alloc(0), Buffer.from([0xff]), Buffer.from('{'), Buffer.from('null'), Buffer.from('[]')]) {
    assert.throws(() => decodeBoundaryEvent(bytes), { code: 'boundary_event_invalid' });
  }
  assert.deepEqual(decodeBoundaryEvent(Buffer.from('{"event":"host_ready"}')), { event: 'host_ready' });
});

test('guest readiness requires the exact 1264 service ceiling and loopback ports', () => {
  const ready = { event: 'guest_ready', tcp: '127.0.0.1:35000', udp: '127.0.0.1:35001', maxDatagramPayload: 1264,
    experimental: true, next: 'enter stop to close' };
  assert.deepEqual(validateBoundaryReady(ready), { tcp: 35000, udp: 35001 });
  for (const override of [{ maxDatagramPayload: 1263 }, { maxDatagramPayload: 1265 }, { maxDatagramPayload: '1264' },
    { tcp: '192.0.2.1:35000' }, { udp: '127.0.0.1:0' }, { udp: '127.0.0.1:65536' }, { extra: 'unreviewed' }]) {
    assert.throws(() => validateBoundaryReady({ ...ready, ...override }), { code: 'boundary_bridge_invalid' });
  }
});

const real = process.platform === 'win32' && process.env.AURORA_CONNECT_BOUNDARY_TEST_REAL === '1';
test('pre-cancellation and a verification deadline start no agents', { skip: !real }, async () => {
  const controller = new AbortController(); controller.abort('PRIVATE_REASON');
  await assert.rejects(runBoundarySmoke({ authorized: true, signal: controller.signal }), { code: 'boundary_cancelled' });
  await assert.rejects(runBoundarySmoke({ authorized: true, timeoutMs: 1 }), error => {
    assert.equal(error.code, 'boundary_deadline'); assert.equal(error.cleanup.agents, 0); assert.equal(error.cleanup.closed, 0);
    assert.equal(error.cleanup.forced, false); assert.equal(error.cleanup.ownEndpointsClosed, true); return true;
  });
});

test('real CLI pair proves exact echoes, endpoint absence, continuity, isolation and concurrent TCP', { skip: !real }, async () => {
  const result = await runBoundarySmoke({ authorized: true });
  assert.equal(result.result, 'PASS'); assert.equal(result.servicePayloadCeiling, 1264); assert.equal(result.negotiatedPayload, 1264);
  assert.equal(result.tcpBytesEachDirection, 1048576); assert.equal(result.tcpConcurrentWithUdp, true);
  assert(result.tcpBytesForwardedDuringUdp > 0); assert.equal(result.udpLocalSources, 2); assert.equal(result.udpHostAssociationsObserved, 2);
  assert.equal(result.udpExactEchoes, 4); assert.equal(result.udpOversizeRequests, 2); assert.equal(result.udpOversizeReachedEndpoint, 0);
  assert.equal(result.udpValidAfterRejection, 2); assert.equal(result.udpEndpointBytes, 5056);
  assert.equal(result.cleanup.agents, 2); assert.equal(result.cleanup.closed, 2); assert.equal(result.cleanup.forced, false);
  assert.equal(result.cleanup.ownEndpointsClosed, true); assert.equal(result.cleanup.guestPortsRebound, true); assert.equal(result.cleanup.stderrBytes, 0);
  assert.equal(result.externalNetwork, 'not_tested'); assert.equal(result.physicalPathMtu, 'not_measured');
  for (const privateText of ['127.0.0.1', 'peer', 'session', 'plan.json', 'invitation']) assert(!JSON.stringify(result).includes(privateText));
});

test('deadline reaps every started child and releases any opened bridges without assuming startup speed', { skip: !real }, async () => {
  await assert.rejects(runBoundarySmoke({ authorized: true, timeoutMs: 1200 }), error => {
    assert.equal(error.code, 'boundary_deadline'); assert(error.cleanup.agents >= 0 && error.cleanup.agents <= 2);
    assert.equal(error.cleanup.closed, error.cleanup.agents); assert.equal(error.cleanup.forced, false);
    assert.equal(error.cleanup.ownEndpointsClosed, true); assert.equal(error.cleanup.guestPortsRebound, error.cleanup.bridgesOpened);
    assert(!JSON.stringify(error).includes('127.0.0.1')); return true;
  });
});
