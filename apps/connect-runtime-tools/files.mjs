import { createHash } from 'node:crypto';
import { lstat, open, realpath, rename } from 'node:fs/promises';
import { rejectLinkedParents, resolveExplicitLocalPath } from '../connect-voice-tools/local-path.mjs';
import { SVC_ARTIFACT } from './svc-profile.mjs';

export const hash = bytes => createHash('sha256').update(bytes).digest('hex');
export const fail = code => { throw new Error(code); };
export async function absent(file) {
  try { await lstat(file); } catch (error) { if (error.code === 'ENOENT') return; throw error; }
  fail('runtime_destination_exists');
}

// Rejects observed links/hardlinks and size changes. Local same-user concurrent
// parent replacement remains outside this development helper's isolation model.
export async function readLocal(file, maximum) {
  const absolute = resolveExplicitLocalPath(file);
  await rejectLinkedParents(absolute);
  const before = await lstat(absolute);
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1 || before.size > maximum) fail('runtime_unsafe_file');
  if ((await realpath(absolute)).toLowerCase() !== absolute.toLowerCase()) fail('runtime_unsafe_file');
  const handle = await open(absolute, 'r');
  try {
    const opened = await handle.stat();
    if (opened.ino !== before.ino || opened.dev !== before.dev || opened.size !== before.size) fail('runtime_file_changed');
    const buffer = Buffer.alloc(before.size + 1);
    let offset = 0;
    while (offset < buffer.length) {
      const { bytesRead } = await handle.read(buffer, offset, buffer.length - offset, null);
      if (!bytesRead) break;
      offset += bytesRead;
    }
    const after = await handle.stat();
    const pathAfter = await lstat(absolute);
    if (offset !== before.size || after.size !== before.size || after.mtimeMs !== before.mtimeMs
      || after.ctimeMs !== before.ctimeMs || after.nlink !== 1 || pathAfter.isSymbolicLink()
      || pathAfter.ino !== opened.ino || pathAfter.dev !== opened.dev) fail('runtime_file_changed');
    return buffer.subarray(0, offset);
  } finally { await handle.close(); }
}

async function writeAll(handle, bytes) {
  let offset = 0;
  while (offset < bytes.length) {
    const { bytesWritten } = await handle.write(bytes, offset, bytes.length - offset, null);
    if (!bytesWritten) fail('runtime_write_failed');
    offset += bytesWritten;
  }
}

// This low-level helper is used with compiled-in pins by the CLI. The injectable
// fetch in tests provides real bounded HTTP streams, never a production override.
export async function downloadVerified(pin, destination, { fetcher = fetch, timeoutMs = 90000, signal } = {}) {
  const url = new URL(pin.url);
  const svcPin = Object.keys(SVC_ARTIFACT).every(key => pin[key] === SVC_ARTIFACT[key]);
  if ((pin.file === SVC_ARTIFACT.file && !svcPin) || url.protocol !== 'https:' || url.username || url.password || url.port || url.hash
    || (!['piston-data.mojang.com', 'maven.fabricmc.net'].includes(url.hostname) && !svcPin)) fail('runtime_download_origin');
  if (!Number.isSafeInteger(pin.bytes) || pin.bytes < 1 || pin.bytes > 64 * 1024 * 1024
    || !['sha1', 'sha256'].includes(pin.algorithm)
    || !(pin.algorithm === 'sha1' ? /^[0-9a-f]{40}$/ : /^[0-9a-f]{64}$/).test(pin.digest)) fail('runtime_download_pin');
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 90000) fail('runtime_download_timeout');
  const file = resolveExplicitLocalPath(destination);
  await rejectLinkedParents(file);
  await absent(file);
  const partial = `${file}.part`;
  const controller = new AbortController();
  const abort = () => controller.abort();
  signal?.addEventListener('abort', abort, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(abort, timeoutMs);
  let handle; let reader;
  try {
    controller.signal.throwIfAborted();
    handle = await open(partial, 'wx');
    const response = await fetcher(pin.url, { redirect: 'error', credentials: 'omit', signal: controller.signal,
      headers: { 'User-Agent': 'AuroraConnect-local-lab/0.1', 'Accept-Encoding': 'identity' } });
    if (response.status !== 200 || !response.body || response.redirected) fail('runtime_download_response');
    const length = response.headers.get('content-length');
    if (length !== null && length !== String(pin.bytes)) fail('runtime_download_size');
    const encoding = response.headers.get('content-encoding');
    if (encoding && encoding !== 'identity') fail('runtime_download_encoding');
    reader = response.body.getReader();
    const published = createHash(pin.algorithm); const sha256 = createHash('sha256');
    let count = 0;
    for (;;) {
      controller.signal.throwIfAborted();
      const { done, value } = await reader.read();
      if (done) break;
      count += value.byteLength;
      if (count > pin.bytes) fail('runtime_download_size');
      published.update(value); sha256.update(value);
      await writeAll(handle, value);
    }
    controller.signal.throwIfAborted();
    if (count !== pin.bytes) fail('runtime_download_size');
    if (published.digest('hex') !== pin.digest) fail('runtime_download_hash');
    await handle.sync(); await handle.close(); handle = undefined;
    await rejectLinkedParents(file); await absent(file);
    controller.signal.throwIfAborted();
    await rename(partial, file);
    return { file: pin.file, bytes: count, sha256: sha256.digest('hex'), algorithm: pin.algorithm, digest: pin.digest };
  } finally {
    clearTimeout(timer); signal?.removeEventListener('abort', abort);
    controller.abort();
    if (reader) { try { await reader.cancel(); } catch { /* terminal stream */ } }
    if (handle) await handle.close();
    // Failed partials stay only in the freshly allocated lab directory for review.
    // They are never treated as a verified artifact or resumed automatically.
  }
}
