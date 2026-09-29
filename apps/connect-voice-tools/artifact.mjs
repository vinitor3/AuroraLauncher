// Read-only, bounded inspection of explicit JARs. Nothing is extracted or executed.
import { createHash } from 'node:crypto';
import { open, lstat, realpath } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { crc32, inflateRawSync } from 'node:zlib';
import { rejectLinkedParents, resolveExplicitLocalPath } from './local-path.mjs';

const MAX_JAR = 64 * 1024 * 1024;
const verified = new WeakSet();
const fail = code => { throw new Error(code); };
const hash = (algorithm, bytes) => createHash(algorithm).update(bytes).digest('hex');
const utf8 = bytes => { try { return new TextDecoder('utf-8', { fatal: true }).decode(bytes); } catch { fail('artifact_utf8'); } };
const CANDIDATES = [
  { mod: 'svc', id: 'voicechat', version: '2.6.22', metadataVersion: '1.21.1-2.6.22', loader: 'fabric', minecraft: '1.21.1', java: 21,
    license: 'All Rights Reserved', bytes: 5369475, sha256: '972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36',
    publishedAlgorithm: 'sha512', publishedDigest: 'c44e69400ac7349ad6d4699263bde4d87359e03233c31a872eb02542658cfc73a185b46e4361c54cb1ac029a2991bd8eb4d4ea59b1c3e3b43f234087520b325a',
    source: 'https://api.modrinth.com/v2/version/VS5X7jZd' },
  { mod: 'plasmo', id: 'plasmovoice', version: '2.1.17', metadataVersion: '2.1.17', loader: 'fabric', minecraft: '1.21.1', java: 21,
    license: 'LGPL-3.0', bytes: 12365966, sha256: '46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b',
    publishedAlgorithm: 'sha256', publishedDigest: '46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b',
    source: 'https://api.github.com/repos/plasmoapp/plasmo-voice/releases/tags/2.1.17' },
];

export async function readArtifactFile(file) {
  const absolute = resolveExplicitLocalPath(file);
  await rejectLinkedParents(absolute);
  const before = await lstat(absolute);
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1) fail('unsafe_artifact_file');
  if ((await realpath(absolute)).toLowerCase() !== absolute.toLowerCase()) fail('linked_artifact_path');
  if (before.size < 22 || before.size > MAX_JAR) fail('artifact_size');
  const handle = await open(absolute, 'r');
  try {
    const initial = await handle.stat();
    if (initial.ino !== before.ino || initial.dev !== before.dev || initial.size !== before.size) fail('artifact_changed');
    const bytes = Buffer.alloc(before.size + 1);
    let offset = 0;
    while (offset < bytes.length) {
      const { bytesRead } = await handle.read(bytes, offset, bytes.length - offset, null);
      if (!bytesRead) break;
      offset += bytesRead;
    }
    const after = await handle.stat();
    const pathAfter = await lstat(absolute);
    if (offset !== before.size || after.size !== initial.size || after.mtimeMs !== initial.mtimeMs || after.ctimeMs !== initial.ctimeMs
      || pathAfter.ino !== initial.ino || pathAfter.dev !== initial.dev || pathAfter.isSymbolicLink() || after.nlink !== 1) fail('artifact_changed');
    return bytes.subarray(0, offset);
  } finally { await handle.close(); }
}

// Strict ZIP32 subset sufficient for the two pinned artifacts. Validate the whole
// directory/local-header layout, but decompress only named, size-bounded metadata.
export function openJar(bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 22 || bytes.length > MAX_JAR) fail('artifact_size');
  let end = -1;
  for (let p = bytes.length - 22; p >= Math.max(0, bytes.length - 65557); p--) {
    if (bytes.readUInt32LE(p) === 0x06054b50 && p + 22 + bytes.readUInt16LE(p + 20) === bytes.length) { end = p; break; }
  }
  if (end < 0) fail('zip_end_missing');
  const count = bytes.readUInt16LE(end + 10);
  const size = bytes.readUInt32LE(end + 12);
  const start = bytes.readUInt32LE(end + 16);
  if (bytes.readUInt16LE(end + 4) || bytes.readUInt16LE(end + 6) || count !== bytes.readUInt16LE(end + 8)
    || count === 0xffff || size === 0xffffffff || start === 0xffffffff) fail('zip_multidisk_or_zip64');
  if (!count || count > 10000 || size > 4 * 1024 * 1024 || start + size !== end) fail('zip_directory_limits');
  const entries = new Map(); const folded = new Set(); const regions = [];
  let cursor = start; let totalExpanded = 0;
  const checkExtra = (offset, length) => {
    const limit = offset + length;
    while (offset < limit) {
      if (offset + 4 > limit) fail('zip_extra_invalid');
      const tag = bytes.readUInt16LE(offset); const length = bytes.readUInt16LE(offset + 2);
      if (tag === 1) fail('zip_multidisk_or_zip64');
      offset += 4 + length;
      if (offset > limit) fail('zip_extra_invalid');
    }
  };
  for (let i = 0; i < count; i++) {
    if (cursor + 46 > end || bytes.readUInt32LE(cursor) !== 0x02014b50) fail('zip_directory_invalid');
    const flags = bytes.readUInt16LE(cursor + 8); const method = bytes.readUInt16LE(cursor + 10);
    const crc = bytes.readUInt32LE(cursor + 16); const compressed = bytes.readUInt32LE(cursor + 20); const expanded = bytes.readUInt32LE(cursor + 24);
    const nameSize = bytes.readUInt16LE(cursor + 28); const extra = bytes.readUInt16LE(cursor + 30); const comment = bytes.readUInt16LE(cursor + 32);
    const offset = bytes.readUInt32LE(cursor + 42); const type = (bytes.readUInt32LE(cursor + 38) >>> 16) & 0xf000;
    if (!nameSize || nameSize > 1024 || cursor + 46 + nameSize + extra + comment > end || bytes.readUInt16LE(cursor + 34)) fail('zip_entry_limits');
    if ((flags & ~0x080e) || ![0, 8].includes(method) || (method === 0 && (flags & 6))) fail('zip_unsupported_encoding');
    if (![0, 0x4000, 0x8000].includes(type)) fail('zip_special_file');
    const rawName = bytes.subarray(cursor + 46, cursor + 46 + nameSize);
    const name = utf8(rawName);
    if (/[\u0000-\u001f\u007f\\:]/.test(name) || name.startsWith('/') || name.split('/').some((part, index, all) => part === '..' || part === '.' || (!part && index !== all.length - 1))) fail('zip_unsafe_name');
    if (folded.has(name.toLowerCase())) fail('zip_duplicate_name');
    folded.add(name.toLowerCase());
    checkExtra(cursor + 46 + nameSize, extra);
    if (compressed > MAX_JAR || expanded > MAX_JAR || (totalExpanded += expanded) > 256 * 1024 * 1024) fail('zip_expanded_limit');
    if (offset + 30 > start || bytes.readUInt32LE(offset) !== 0x04034b50) fail('zip_local_invalid');
    const localNameSize = bytes.readUInt16LE(offset + 26); const localExtra = bytes.readUInt16LE(offset + 28);
    const dataStart = offset + 30 + localNameSize + localExtra;
    if (dataStart + compressed > start || localNameSize !== nameSize || !rawName.equals(bytes.subarray(offset + 30, offset + 30 + localNameSize))
      || bytes.readUInt16LE(offset + 6) !== flags || bytes.readUInt16LE(offset + 8) !== method) fail('zip_local_mismatch');
    checkExtra(offset + 30 + localNameSize, localExtra);
    let dataEnd = dataStart + compressed;
    if (flags & 8) {
      if (dataEnd + 12 > start) fail('zip_descriptor_invalid');
      const descriptor = bytes.readUInt32LE(dataEnd) === 0x08074b50 ? dataEnd + 4 : dataEnd;
      if (descriptor + 12 > start || bytes.readUInt32LE(descriptor) !== crc || bytes.readUInt32LE(descriptor + 4) !== compressed || bytes.readUInt32LE(descriptor + 8) !== expanded) fail('zip_descriptor_invalid');
      dataEnd = descriptor + 12;
      for (const [field, expected] of [[14, crc], [18, compressed], [22, expanded]]) {
        const actual = bytes.readUInt32LE(offset + field);
        if (actual !== 0 && actual !== expected) fail('zip_local_mismatch');
      }
    } else if (bytes.readUInt32LE(offset + 14) !== crc || bytes.readUInt32LE(offset + 18) !== compressed || bytes.readUInt32LE(offset + 22) !== expanded) fail('zip_local_mismatch');
    if (name.endsWith('/') && (expanded || crc)) fail('zip_directory_data');
    regions.push([offset, dataEnd]); entries.set(name, { method, crc, compressed, expanded, dataStart });
    cursor += 46 + nameSize + extra + comment;
  }
  if (cursor !== end) fail('zip_directory_invalid');
  regions.sort((a, b) => a[0] - b[0]);
  let previous = 0;
  for (const [begin, finish] of regions) { if (begin !== previous) fail('zip_overlap_or_gap'); previous = finish; }
  if (previous !== start) fail('zip_overlap_or_gap');
  let inflatedTotal = 0;
  return {
    entryCount: count,
    read(name, maximum) {
      const entry = entries.get(name);
      if (!entry) fail('jar_entry_missing');
      if (!Number.isInteger(maximum) || maximum < 1 || maximum > 1024 * 1024 || entry.expanded > maximum
        || (inflatedTotal += entry.expanded) > 2 * 1024 * 1024) fail('zip_selected_entry_limit');
      const input = bytes.subarray(entry.dataStart, entry.dataStart + entry.compressed);
      let output;
      try {
        if (entry.method === 0) output = input;
        else {
          const result = inflateRawSync(input, { maxOutputLength: maximum, info: true });
          if (result.engine.bytesWritten !== input.length) fail('zip_deflate_trailing_data');
          output = result.buffer;
        }
      } catch (error) { if (error.message === 'zip_deflate_trailing_data') throw error; fail('zip_inflate_failed'); }
      if (output.length !== entry.expanded || crc32(output) !== entry.crc) fail('zip_entry_integrity');
      return output;
    },
  };
}

function metadata(jar) {
  let value;
  try { value = JSON.parse(utf8(jar.read('fabric.mod.json', 65536))); } catch (error) {
    if (/^(zip_|jar_|artifact_)/.test(error.message)) throw error;
    fail('fabric_metadata_invalid');
  }
  if (value?.schemaVersion !== 1 || typeof value.id !== 'string' || !/^[a-z][a-z0-9_-]{1,63}$/.test(value.id)
    || typeof value.version !== 'string' || !/^[a-zA-Z0-9.+_-]{1,64}$/.test(value.version)
    || !value.depends || typeof value.depends !== 'object') fail('fabric_metadata_invalid');
  return value;
}

// Read constant-pool identifiers and method descriptors only; never execute code.
export function classMethods(bytes, expectedName) {
  let at = 0;
  const take = n => { if (!Number.isSafeInteger(n) || n < 0 || at + n > bytes.length) fail('class_truncated'); const old = at; at += n; return old; };
  const u2 = () => bytes.readUInt16BE(take(2)); const u4 = () => bytes.readUInt32BE(take(4));
  if (bytes.length > 256 * 1024 || u4() !== 0xcafebabe) fail('class_invalid');
  u2(); const major = u2(); const poolCount = u2();
  if (poolCount > 8192) fail('class_pool_limit');
  const pool = new Map();
  for (let index = 1; index < poolCount; index++) {
    const tag = bytes[take(1)];
    if (tag === 1) { const size = u2(); pool.set(index, bytes.subarray(take(size), at).toString('utf8')); }
    else if ([3, 4, 9, 10, 11, 12, 17, 18].includes(tag)) take(4);
    else if ([5, 6].includes(tag)) { take(8); index++; }
    else if ([7, 8, 16, 19, 20].includes(tag)) { const reference = u2(); if (tag === 7) pool.set(index, { reference }); }
    else if (tag === 15) take(3);
    else fail('class_pool_tag');
  }
  u2(); const thisClass = u2(); u2();
  if (pool.get(pool.get(thisClass)?.reference) !== expectedName) fail('class_name_mismatch');
  take(u2() * 2);
  const attributes = () => { const n = u2(); for (let i = 0; i < n; i++) { u2(); take(u4()); } };
  const fields = u2();
  for (let i = 0; i < fields; i++) { take(6); attributes(); }
  const methods = []; const count = u2();
  for (let i = 0; i < count; i++) {
    const access = u2(); const name = pool.get(u2()); const descriptor = pool.get(u2());
    if (typeof name !== 'string' || typeof descriptor !== 'string') fail('class_method_invalid');
    if (access & 1) methods.push(`${name}${descriptor}`);
    attributes();
  }
  attributes();
  if (at !== bytes.length) fail('class_trailing_data');
  return { major, methods };
}

export function inspectArtifact(bytes) {
  const jar = openJar(bytes);
  const value = metadata(jar);
  const digest = hash('sha256', bytes);
  const candidate = CANDIDATES.find(item => item.sha256 === digest && item.bytes === bytes.length);
  const basic = { schemaVersion: 1, sha256: digest, bytes: bytes.length, entries: jar.entryCount, recognizedArtifact: false, runtimeHomologated: false };
  if (!candidate) return Object.freeze({ ...basic, status: 'unrecognized_artifact', metadataModId: ['voicechat', 'plasmovoice'].includes(value.id) ? value.id : 'other', metadataVersion: value.version });
  if (value.id !== candidate.id || value.version !== candidate.metadataVersion || value.license !== candidate.license
    || !Object.hasOwn(value.depends, 'fabricloader') || hash(candidate.publishedAlgorithm, bytes) !== candidate.publishedDigest) fail('pinned_artifact_metadata_mismatch');
  const expectedMinecraft = candidate.mod === 'svc' ? ['1.21', '1.21.1'] : '>=1.21.0 <=1.21.1';
  if (JSON.stringify(value.depends.minecraft) !== JSON.stringify(expectedMinecraft)) fail('pinned_artifact_metadata_mismatch');
  const checks = candidate.mod === 'svc' ? [
    ['de/maxhenkel/voicechat/api/events/VoiceHostEvent', ['getVoiceHost()Ljava/lang/String;', 'setVoiceHost(Ljava/lang/String;)V', 'getPlayer()Lde/maxhenkel/voicechat/api/ServerPlayer;']],
  ] : [
    ['su/plo/voice/api/client/event/socket/UdpClientConnectEvent', ['getClient()Lsu/plo/voice/api/client/socket/UdpClient;', 'setClient(Lsu/plo/voice/api/client/socket/UdpClient;)V']],
    ['su/plo/voice/api/client/socket/UdpClient', ['connect(Ljava/lang/String;I)V', 'getSecret()Ljava/util/UUID;']],
    ['su/plo/voice/proto/packets/tcp/clientbound/ConnectionPacket', ['getIp()Ljava/lang/String;', 'getPort()I']],
  ];
  const api = checks.map(([name, expected]) => {
    const result = classMethods(jar.read(`${name}.class`, 256 * 1024), name);
    if (!expected.every(method => result.methods.includes(method))) fail('artifact_api_mismatch');
    return Object.freeze({ class: name, major: result.major, signatures: Object.freeze(expected) });
  });
  let bundledApi = null;
  let licenseFileSha256 = null;
  if (candidate.mod === 'svc') {
    const nested = openJar(jar.read('META-INF/jars/voicechat-api-2.6.20.jar', 1024 * 1024));
    const apiMetadata = JSON.parse(utf8(nested.read('fabric.mod.json', 65536)));
    if (apiMetadata.id !== 'voicechat_api' || apiMetadata.version !== '2.6.20') fail('artifact_api_mismatch');
    bundledApi = '2.6.20';
  } else {
    const license = jar.read('META-INF/licenses/LICENSE', 65536);
    if (!utf8(license).includes('GNU LESSER GENERAL PUBLIC LICENSE')) fail('artifact_license_mismatch');
    licenseFileSha256 = hash('sha256', license);
  }
  const result = Object.freeze({ ...basic, status: 'verified_pinned_artifact', recognizedArtifact: true, mod: candidate.mod,
    version: candidate.version, metadataVersion: value.version, loader: candidate.loader, minecraft: candidate.minecraft,
    requiredTestJava: candidate.java, declaredJavaDependency: value.depends.java ?? null, declaredLoaderDependency: value.depends.fabricloader, license: value.license, licenseFileSha256,
    publishedAlgorithm: candidate.publishedAlgorithm, publishedDigest: candidate.publishedDigest, source: candidate.source,
    bundledApiVersion: bundledApi, api: Object.freeze(api), sourceQueriedAt: '2026-09-29' });
  verified.add(result);
  return result;
}

export function isVerifiedArtifact(value) { return verified.has(value); }

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const args = process.argv.slice(2);
  (async () => {
    if (args.length !== 2 || args[0] !== '--jar') fail('expected_explicit_jar');
    process.stdout.write(`${JSON.stringify(inspectArtifact(await readArtifactFile(args[1])), null, 2)}\n`);
  })().catch(error => {
    const code = /^[a-z_]+$/.test(error.message) ? error.message : 'artifact_inspection_failed';
    process.stderr.write(`${JSON.stringify({ error: code })}\n`); process.exitCode = 1;
  });
}
