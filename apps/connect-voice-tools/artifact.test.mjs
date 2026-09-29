import { test } from 'node:test';
import assert from 'node:assert/strict';
import { crc32, deflateRawSync } from 'node:zlib';
import { mkdtemp, writeFile, readFile, rm, link } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { openJar, inspectArtifact, isVerifiedArtifact, readArtifactFile, classMethods } from './artifact.mjs';
import { createPlan } from './inspect.mjs';

// Minimal ZIP fixtures are generated locally, never downloaded or executed.
function zip(items) {
  const locals = []; const directory = []; let offset = 0;
  for (const item of items) {
    const name = Buffer.from(item.name); const data = Buffer.from(item.data); const method = item.deflate ? 8 : 0;
    const compressed = item.deflate ? deflateRawSync(data) : data; const crc = crc32(data); const flags = item.descriptor ? 8 : 0;
    const local = Buffer.alloc(30); local.writeUInt32LE(0x04034b50); local.writeUInt16LE(20, 4); local.writeUInt16LE(flags, 6); local.writeUInt16LE(method, 8);
    if (!item.descriptor) { local.writeUInt32LE(crc, 14); local.writeUInt32LE(compressed.length, 18); local.writeUInt32LE(data.length, 22); }
    local.writeUInt16LE(name.length, 26);
    let descriptor = Buffer.alloc(0);
    if (item.descriptor) { descriptor = Buffer.alloc(16); descriptor.writeUInt32LE(0x08074b50); descriptor.writeUInt32LE(crc, 4); descriptor.writeUInt32LE(compressed.length, 8); descriptor.writeUInt32LE(data.length, 12); }
    locals.push(local, name, compressed, descriptor);
    const central = Buffer.alloc(46); central.writeUInt32LE(0x02014b50); central.writeUInt16LE(20, 4); central.writeUInt16LE(20, 6);
    central.writeUInt16LE(flags, 8); central.writeUInt16LE(method, 10); central.writeUInt32LE(crc, 16); central.writeUInt32LE(compressed.length, 20); central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(name.length, 28); central.writeUInt32LE(offset, 42); directory.push(central, name);
    offset += local.length + name.length + compressed.length + descriptor.length;
  }
  const central = Buffer.concat(directory); const end = Buffer.alloc(22); end.writeUInt32LE(0x06054b50); end.writeUInt16LE(items.length, 8); end.writeUInt16LE(items.length, 10);
  end.writeUInt32LE(central.length, 12); end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, central, end]);
}
const meta = version => JSON.stringify({ schemaVersion: 1, id: 'voicechat', version, depends: { minecraft: ['1.21.1'], fabricloader: '*' } });
const tiny = () => zip([{ name: 'fabric.mod.json', data: meta('1.21.1-9.9.9'), deflate: true }]);
const central = bytes => bytes.readUInt32LE(bytes.length - 6);

test('bounded ZIP reader handles stored/deflated/data-descriptor entries with exact CRC', () => {
  const archive = zip([{ name: 'stored', data: 'abc' }, { name: 'deflated', data: 'abc'.repeat(100), deflate: true, descriptor: true }]);
  const reader = openJar(archive);
  assert.equal(reader.entryCount, 2);
  assert.equal(reader.read('stored', 3).toString(), 'abc');
  assert.equal(reader.read('deflated', 300).toString(), 'abc'.repeat(100));
  assert.throws(() => reader.read('absent', 64), /jar_entry_missing/);
});

test('matching mod name and self-declared version never establish release identity', () => {
  const report = inspectArtifact(zip([{ name: 'fabric.mod.json', data: meta('1.21.1-2.6.22') }]));
  assert.equal(report.status, 'unrecognized_artifact');
  assert.equal(report.recognizedArtifact, false);
  assert.equal(isVerifiedArtifact(report), false);
  assert.equal(isVerifiedArtifact({ recognizedArtifact: true, sha256: '972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36' }), false);
});

test('unknown and forged artifact evidence cannot produce an actionable announcement draft', () => {
  const config = Buffer.from('port=24454\nbind_address=127.0.0.1\nvoice_host=\nmtu_size=1275\n');
  for (const artifact of [inspectArtifact(tiny()), { recognizedArtifact: true, mod: 'svc', version: '2.6.22' }]) {
    const plan = createPlan({ mod: 'svc', version: '2.6.22', mode: 'dedicated-aurora-only', guestPort: 35554, artifact }, config);
    assert.equal(plan.artifactVerified, false); assert.deepEqual(plan.changes, []);
    assert(plan.blockers.includes('artifact_not_recognized_or_untrusted_report'));
  }
});

test('truncated ZIP, trailing bytes, ZIP64, multi-disk and excessive entries are refused', () => {
  const original = tiny();
  for (const length of [0, 5, 21, original.length - 1]) assert.throws(() => openJar(original.subarray(0, length)));
  assert.throws(() => openJar(Buffer.concat([original, Buffer.from('junk')])));
  for (const [field, value] of [[4, 1], [8, 0xffff], [10, 10001]]) {
    const bytes = Buffer.from(original); bytes.writeUInt16LE(value, bytes.length - 22 + field); assert.throws(() => openJar(bytes));
  }
});

test('duplicate, traversal, absolute and Windows-style entry names are rejected without extraction', () => {
  for (const name of ['../fabric.mod.json', '/fabric.mod.json', 'C:/fabric.mod.json', 'a\\b', 'a//b', 'a/./b']) {
    assert.throws(() => openJar(zip([{ name, data: '{}' }])));
  }
  assert.throws(() => openJar(zip([{ name: 'fabric.mod.json', data: '{}' }, { name: 'FABRIC.MOD.JSON', data: '{}' }])), /zip_duplicate_name/);
});

test('encrypted, symlink, overlapping and mismatched local headers are refused', () => {
  for (const mutate of [
    bytes => bytes.writeUInt16LE(1, central(bytes) + 8),
    bytes => bytes.writeUInt32LE(0xa0000000, central(bytes) + 38),
    bytes => bytes.writeUInt32LE(1, central(bytes) + 42),
    bytes => { bytes[30] ^= 1; },
  ]) { const bytes = tiny(); mutate(bytes); assert.throws(() => openJar(bytes)); }
});

test('CRC, actual inflate output and declared metadata size are independently bounded', () => {
  const corrupted = zip([{ name: 'fabric.mod.json', data: meta('1.0') }]); corrupted[30 + 'fabric.mod.json'.length] ^= 1;
  assert.throws(() => inspectArtifact(corrupted), /zip_entry_integrity/);
  const inflated = zip([{ name: 'fabric.mod.json', data: 'x'.repeat(65537), deflate: true }]);
  assert.throws(() => inspectArtifact(inflated), /zip_selected_entry_limit/);
  // Lie in both size fields to reach the decompressor's independent actual-output cap.
  inflated.writeUInt32LE(10, 22); inflated.writeUInt32LE(10, central(inflated) + 24);
  assert.throws(() => inspectArtifact(inflated), /zip_inflate_failed/);
});

test('selected extraction budget is cumulative and never bypassed by repeated reads', () => {
  const jar = openJar(zip([{ name: 'large', data: Buffer.alloc(1024 * 1024), deflate: true }]));
  jar.read('large', 1024 * 1024); jar.read('large', 1024 * 1024);
  assert.throws(() => jar.read('large', 1024 * 1024), /zip_selected_entry_limit/);
});

test('Fabric metadata must exist and parse; malformed archive cannot become another mod', () => {
  for (const item of [{ name: 'other.json', data: '{}' }, { name: 'fabric.mod.json', data: '{' }, { name: 'fabric.mod.json', data: JSON.stringify({ id: 'voicechat', version: '2.6.22' }) }]) {
    assert.throws(() => inspectArtifact(zip([item])));
  }
});

test('class reader refuses bad magic, truncation and oversized entries', () => {
  for (const bytes of [Buffer.alloc(0), Buffer.alloc(32), Buffer.from('cafebabe', 'hex'), Buffer.alloc(256 * 1024 + 1)]) {
    assert.throws(() => classMethods(bytes, 'example/Interface'));
  }
});

test('explicit filesystem inspection preserves bytes and refuses hardlinks and oversize input', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'aurora-jar-readonly-'));
  try {
    const file = join(directory, 'synthetic.jar'); const bytes = tiny(); await writeFile(file, bytes);
    const report = inspectArtifact(await readArtifactFile(file)); assert.equal(report.recognizedArtifact, false);
    assert.deepEqual(await readFile(file), bytes);
    const second = join(directory, 'linked.jar'); await link(file, second);
    await assert.rejects(readArtifactFile(file), /unsafe_artifact_file/);
    assert.throws(() => openJar(Buffer.alloc(64 * 1024 * 1024 + 1)), /artifact_size/);
  } finally { assert(directory.startsWith(join(tmpdir(), 'aurora-jar-readonly-'))); await rm(directory, { recursive: true, force: true }); }
});
