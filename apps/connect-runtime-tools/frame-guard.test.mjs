import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Readable, Writable } from 'node:stream';
import { pipeline, finished } from 'node:stream/promises';
import { setTimeout as delay } from 'node:timers/promises';
import { FrameGuard, FrameGuardError, MAX_FRAME_BYTES, MAX_TOTAL_BYTES } from './frame-guard.mjs';

function frame(payload) {
  let n = payload.length; const prefix = [];
  do { const byte = n & 0x7f; n >>>= 7; prefix.push(byte | (n ? 0x80 : 0)); } while (n);
  return Buffer.concat([Buffer.from(prefix), payload]);
}
async function collect(chunks, options = {}) {
  const guard = new FrameGuard(options); const output = [];
  await pipeline(Readable.from(chunks), guard, new Writable({ write(chunk, _encoding, callback) {
    output.push(Buffer.from(chunk)); callback();
  } }));
  return { guard, output, bytes: Buffer.concat(output) };
}
async function rejected(chunks, code, options = {}) {
  await assert.rejects(collect(chunks, options), error => {
    assert.ok(error instanceof FrameGuardError); assert.equal(error.code, code);
    assert.equal(error.message, code); assert.equal(error.cause, undefined); return true;
  });
}

test('maximums are explicit and options cannot enlarge the approved bounds', () => {
  assert.equal(MAX_FRAME_BYTES, 0x1fffff); assert.equal(MAX_TOTAL_BYTES, 32 * 1024 * 1024);
  for (const options of [{ maxFrameBytes: 0 }, { maxFrameBytes: 0x200000 }, { maxFrameBytes: 1.5 },
    { maxTotalBytes: 0 }, { maxTotalBytes: MAX_TOTAL_BYTES + 1 }, { signal: {} }]) {
    assert.throws(() => new FrameGuard(options), error => error.code === 'frame_guard_invalid_options');
  }
});

test('fragmented headers and bodies yield exactly one original frame each', async () => {
  for (const length of [1, 127, 128, 16383, 16384]) {
    const expected = frame(Buffer.alloc(length, 0xa5));
    const chunks = [...expected].map(byte => Buffer.from([byte]));
    const { output, guard } = await collect(chunks);
    assert.equal(output.length, 1); assert.deepEqual(output[0], expected);
    assert.equal(guard.stats.forwardedFrames, 1); assert.equal(guard.stats.receivedBytes, expected.length);
  }
});

test('coalesced frames and arbitrary splits preserve order and exact bytes', async () => {
  const expected = Array.from({ length: 50 }, (_, i) => frame(Buffer.alloc(i + 1, i)));
  const wire = Buffer.concat(expected);
  for (const split of [1, 2, 3, 31, 128, wire.length]) {
    const chunks = []; for (let i = 0; i < wire.length; i += split) chunks.push(wire.subarray(i, i + split));
    const result = await collect(chunks);
    assert.deepEqual(result.output, expected); assert.deepEqual(result.bytes, wire);
    assert.equal(result.guard.stats.forwardedFrames, expected.length);
  }
});

test('maximum legal 21-bit frame works and one byte above cannot be encoded in three bytes', async () => {
  const expected = frame(Buffer.alloc(MAX_FRAME_BYTES, 0x61));
  const { output, guard } = await collect([expected.subarray(0, 2), expected.subarray(2, 17), expected.subarray(17)]);
  assert.equal(output.length, 1); assert.deepEqual(output[0], expected);
  assert.equal(guard.stats.peakFrameAllocation, MAX_FRAME_BYTES + 3);
  await rejected([Buffer.from([0x80, 0x80, 0x80, 0x01])], 'frame_guard_varint');
});

test('invalid varints and configured oversize fail before allocating or forwarding payload', async () => {
  for (const bytes of [[0], [0x80, 0], [0x81, 0], [0xff, 0x80, 0], [0xff, 0xff, 0x80], [0x80, 0x80, 0x80]]) {
    await rejected([Buffer.from(bytes)], 'frame_guard_varint');
  }
  const guard = new FrameGuard({ maxFrameBytes: 127 }); const output = [];
  guard.on('data', chunk => output.push(chunk));
  const done = finished(guard); guard.end(Buffer.from([0x80, 0x01]));
  await assert.rejects(done, error => error.code === 'frame_guard_frame_limit');
  assert.equal(output.length, 0); assert.equal(guard.stats.peakFrameAllocation, 0);
});

test('no header or body fragment is released before a complete frame', async () => {
  const guard = new FrameGuard(); const output = []; guard.on('data', chunk => output.push(chunk));
  const done = finished(guard);
  guard.write(Buffer.from([3, 0x10])); await delay(5); assert.equal(output.length, 0);
  guard.write(Buffer.from([0x20])); await delay(5); assert.equal(output.length, 0);
  guard.end(Buffer.from([0x30])); await done;
  assert.deepEqual(output, [Buffer.from([3, 0x10, 0x20, 0x30])]);
});

test('EOF with a partial header or body is rejected; empty stream itself is allowed', async () => {
  for (const bytes of [[0x80], [0x80, 0x80], [1], [3, 1, 2]]) {
    await rejected([Buffer.from(bytes)], 'frame_guard_truncated');
  }
  assert.equal((await collect([])).output.length, 0);
});

test('total budget includes prefix bytes and is enforced across chunks before forwarding excess', async () => {
  const small = frame(Buffer.from([0xaa]));
  const exact = await collect([small, small, small], { maxTotalBytes: 6 });
  assert.equal(exact.guard.stats.forwardedBytes, 6);
  await rejected([small, small, small, small], 'frame_guard_total_limit', { maxTotalBytes: 6 });
  await rejected([Buffer.alloc(100)], 'frame_guard_total_limit', { maxTotalBytes: 99 });
});

test('a declared frame that cannot fit remaining total budget fails before body allocation', async () => {
  const guard = new FrameGuard({ maxTotalBytes: 6 }); const output = [];
  guard.on('data', chunk => output.push(chunk));
  const done = finished(guard);
  guard.write(Buffer.from([1, 0xaa])); // Two wire bytes; only four remain.
  guard.end(Buffer.from([4])); // Header plus four payload bytes needs five.
  await assert.rejects(done, error => error.code === 'frame_guard_total_limit');
  assert.deepEqual(output, [Buffer.from([1, 0xaa])]);
  assert.equal(guard.stats.peakFrameAllocation, 2);
});

test('a valid frame followed by truncation forwards only the valid predecessor', async () => {
  const guard = new FrameGuard(); const output = []; guard.on('data', chunk => output.push(chunk));
  const done = finished(guard); guard.end(Buffer.from([1, 0xab, 3, 0xcd]));
  await assert.rejects(done, error => error.code === 'frame_guard_truncated');
  assert.deepEqual(output, [Buffer.from([1, 0xab])]);
});

test('backpressure pauses within a coalesced input chunk instead of accumulating all output', async () => {
  const each = frame(Buffer.alloc(1024, 0x45)); const wire = Buffer.concat(Array(200).fill(each));
  const guard = new FrameGuard({ maxFrameBytes: 1024 }); let writeDone = false;
  const closed = finished(guard);
  assert.equal(guard.write(wire, error => { assert.equal(error, null); writeDone = true; }), false);
  await delay(20);
  assert.equal(writeDone, false); assert.ok(guard.readableLength <= 16384 + each.length);
  assert.ok(guard.stats.forwardedFrames < 200); assert.ok(guard.stats.pendingInputBytes > 0);
  let count = 0;
  const drained = pipeline(guard, new Writable({ highWaterMark: 1024, write(chunk, _encoding, callback) {
    count += chunk.length; setImmediate(callback);
  } }));
  guard.end(); await drained; await closed;
  assert.equal(count, wire.length); assert.equal(writeDone, true); assert.equal(guard.stats.forwardedFrames, 200);
});

test('many concurrent partial streams each allocate only their configured frame', async () => {
  const controllers = Array.from({ length: 24 }, () => new AbortController());
  const guards = controllers.map(controller => new FrameGuard({ maxFrameBytes: 4096, signal: controller.signal }));
  const completions = guards.map(guard => finished(guard).catch(error => error));
  const partial = frame(Buffer.alloc(4096)).subarray(0, 1024);
  for (const guard of guards) guard.write(partial);
  await delay(5);
  for (const guard of guards) {
    assert.equal(guard.stats.frameAllocation, 4098); assert.equal(guard.stats.bufferedFrameBytes, 1024);
    assert.equal(guard.readableLength, 0); assert.equal(guard.stats.forwardedBytes, 0);
  }
  controllers.forEach(controller => controller.abort());
  for (const error of await Promise.all(completions)) assert.equal(error.code, 'frame_guard_cancelled');
  guards.forEach(guard => { assert.equal(guard.stats.frameAllocation, 0); assert.equal(guard.stats.pendingInputBytes, 0); });
});

test('abort before input and during backpressure reject promptly with no orphan write callback', async () => {
  const before = new AbortController(); before.abort();
  await rejected([], 'frame_guard_cancelled', { signal: before.signal });
  const controller = new AbortController(); const guard = new FrameGuard({ signal: controller.signal });
  const done = finished(guard); let writeError;
  guard.write(frame(Buffer.alloc(65536)), error => { writeError = error; });
  await delay(5); assert.ok(guard.readableLength > 0);
  controller.abort('PRIVATE_REASON');
  await assert.rejects(done, error => error.code === 'frame_guard_cancelled');
  assert.equal(writeError.code, 'frame_guard_cancelled'); assert.equal(guard.stats.pendingInputBytes, 0);
});

test('external stream errors are redacted rather than propagating payload/path details', async () => {
  const guard = new FrameGuard(); const done = finished(guard);
  guard.destroy(new Error('PRIVATE_PAYLOAD_AND_PATH'));
  await assert.rejects(done, error => error.code === 'frame_guard_stream_error' && !error.message.includes('PRIVATE'));
});
