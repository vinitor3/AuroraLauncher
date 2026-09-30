import { lstat, open, rename, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { absent, readLocal } from './files.mjs';
import { rejectLinkedParents } from '../connect-voice-tools/local-path.mjs';

const FAIL = code => { throw new Error(code); };
const HASH = /^[a-f0-9]{64}$/;
export const INSTANCE_ID = /^aurora-test-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const IN = ['apply-review.json', 'lock-held.json', 'restore-review.json', 'restored.json'];
const OUT = ['apply-approval.json', 'java-closed.json', 'restore-approval.json'];
const NAMES = [...IN, ...OUT, 'cancel.json', 'failed.json'];
const KEYS = [
  ['event', 'profile', 'instanceId', 'digest', 'sourceSha256', 'replacementSha256'],
  ['event', 'profile', 'instanceId', 'configSha256'],
  ['event', 'profile', 'mode', 'digest', 'sourceSha256', 'replacementSha256'],
  ['event', 'profile', 'mode', 'sourceSha256', 'appliedSha256', 'writerSha256', 'restoredSha256', 'sourceUnchanged', 'unownedPreserved', 'pending'],
];
const EVENTS = ['native_apply_review', 'native_lock_held', 'native_restore_review', 'native_restored'];
const sameKeys = (object, keys) => object && typeof object === 'object' && !Array.isArray(object)
  && Object.keys(object).sort().join(',') === [...keys].sort().join(',');

export async function readNativeFailure(directory) {
  let bytes;
  try { bytes = await readLocal(join(directory, 'failed.json'), 512); }
  catch (error) { if (error.code === 'ENOENT') return; throw new Error('live_ipc_failure_file'); }
  let value;
  try { value = JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes)); }
  catch { FAIL('live_ipc_failure_schema'); }
  if (!sameKeys(value, ['event', 'code']) || value.event !== 'native_failed'
    || typeof value.code !== 'string' || value.code.length > 64 || !/^live_[a-z_]+$/.test(value.code)) FAIL('live_ipc_failure_schema');
  // Canonical shape also rejects duplicate fields and escaped aliases.
  const fields = [...bytes.toString('utf8').matchAll(/"([A-Za-z][A-Za-z0-9]*)"\s*:/g)].map(match => match[1]);
  if (fields.length !== 2 || new Set(fields).size !== 2) FAIL('live_ipc_failure_schema');
  return value.code;
}

export function validateNativeEvent(value, phase, profile) {
  if (!Number.isInteger(phase) || phase < 0 || phase > 3 || !['svc', 'plasmo'].includes(profile)
    || !sameKeys(value, KEYS[phase]) || value.event !== EVENTS[phase] || value.profile !== profile) FAIL('live_ipc_event');
  for (const [key, field] of Object.entries(value)) {
    if ((key.endsWith('Sha256') || key === 'digest') && (typeof field !== 'string' || !HASH.test(field))) FAIL('live_ipc_hash');
  }
  if (phase < 2 && !INSTANCE_ID.test(value.instanceId)) FAIL('live_ipc_instance');
  if (phase >= 2 && !['strict', 'reconciled'].includes(value.mode)) FAIL('live_ipc_restore_mode');
  if (phase === 3 && (value.sourceUnchanged !== true || value.unownedPreserved !== true || value.pending !== false)) FAIL('live_ipc_restoration');
  return Object.freeze(value);
}

// Single-use, phase-ordered IPC in an already owned local directory. No content
// is executed. Files are bounded, exact-schema and immutable after publication.
export class LiveIpc {
  #directory; #profile; #phase = 0; #written = new Set(); #observed = new Map();
  constructor(directory, profile) {
    if (!['svc', 'plasmo'].includes(profile)) FAIL('live_ipc_profile');
    this.#directory = directory; this.#profile = profile;
  }
  async #tree() {
    try { return await this.#inspectTree(); }
    catch (error) {
      // Preserve already-redacted protocol/native failure codes, never a raw
      // filesystem exception containing a path from this enumeration.
      if (error instanceof Error && /^live_[a-z_]+$/.test(error.message)) throw error;
      FAIL('live_ipc_file');
    }
  }
  async #inspectTree() {
    await rejectLinkedParents(this.#directory);
    const metadata = await lstat(this.#directory);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) FAIL('live_ipc_directory');
    const published = new Set();
    for (const name of await readdir(this.#directory)) {
      const finalName = NAMES.find(known => name === `${known}.part`);
      if (!NAMES.includes(name) && finalName === undefined) FAIL('live_ipc_unknown_file');
      let item;
      try { item = await lstat(join(this.#directory, name)); }
      catch (error) {
        if (error.code !== 'ENOENT' || finalName === undefined) throw error;
        // The native writer publishes by rename. readdir can see its known
        // .part immediately before lstat sees ENOENT. Accept only a final file
        // that now passes the normal bounded identity/link checks. Its content
        // still goes through the existing failure/event/schema/phase checks.
        await readLocal(join(this.#directory, finalName), 4096);
        published.add(finalName);
        continue;
      }
      if (!item.isFile() || item.isSymbolicLink() || item.nlink !== 1 || item.size > 4096) FAIL('live_ipc_file');
      if (finalName === undefined) published.add(name);
    }
    for (const [name, bytes] of this.#observed) {
      if (!(await readLocal(join(this.#directory, name), 4096)).equals(bytes)) FAIL('live_ipc_changed');
    }
    const failure = await readNativeFailure(this.#directory);
    if (published.has('failed.json') && failure === undefined) FAIL('live_ipc_file');
    if (failure) FAIL(failure);
    for (let phase = this.#phase + 1; phase < IN.length; phase++) {
      if (published.has(IN[phase])) FAIL('live_ipc_early_phase');
      try { await lstat(join(this.#directory, IN[phase])); FAIL('live_ipc_early_phase'); }
      catch (error) { if (error.code !== 'ENOENT') throw error; }
    }
    return published;
  }
  async receive(phase, signal, timeoutMs = 180000) {
    if (phase !== this.#phase || !(signal instanceof AbortSignal)
      || (phase > 0 && !this.#written.has(OUT[phase - 1]))
      || !Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 480000) FAIL('live_ipc_phase');
    const combined = AbortSignal.any([signal, AbortSignal.timeout(timeoutMs)]);
    const file = join(this.#directory, IN[phase]);
    for (;;) {
      if (combined.aborted) FAIL(signal.aborted ? 'live_ipc_cancelled' : 'live_ipc_deadline');
      const published = await this.#tree();
      if (published.has('cancel.json')) FAIL('live_ipc_cancelled');
      try { await lstat(join(this.#directory, 'cancel.json')); FAIL('live_ipc_cancelled'); }
      catch (error) { if (error.code !== 'ENOENT') throw error; }
      let bytes;
      try { bytes = await readLocal(file, 4096); }
      catch (error) {
        if (published.has(IN[phase])) FAIL('live_ipc_file');
        if (error.code !== 'ENOENT') throw error;
      }
      if (bytes) {
        let value;
        try {
          const text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
          value = JSON.parse(text);
          // This protocol is a flat object with unescaped fixed field names.
          // Reject duplicate/escaped aliases rather than let JSON.parse hide one.
          const fields = [...text.matchAll(/"([A-Za-z][A-Za-z0-9]*)"\s*:/g)].map(match => match[1]);
          if (!value || fields.length !== Object.keys(value).length || new Set(fields).size !== fields.length) FAIL('live_ipc_json');
        }
        catch { FAIL('live_ipc_json'); }
        validateNativeEvent(value, phase, this.#profile);
        this.#observed.set(IN[phase], bytes); this.#phase++;
        return value;
      }
      try { await delay(100, undefined, { signal: combined }); } catch { /* next iteration preserves fixed code */ }
    }
  }
  async send(name, value) {
    const expected = OUT[this.#phase - 1];
    if (name !== expected || this.#written.has(name)) FAIL('live_ipc_phase');
    const keys = name === 'java-closed.json' ? ['closed', 'exitCode', 'forced', 'eulaRestored', 'configSha256'] : ['digest'];
    if (!sameKeys(value, keys) || (name === 'java-closed.json'
      ? value.closed !== true || value.exitCode !== 0 || value.forced !== false || value.eulaRestored !== true || !HASH.test(value.configSha256)
      : !HASH.test(value.digest))) FAIL('live_ipc_approval');
    await this.#tree();
    const file = join(this.#directory, name); const part = `${file}.part`;
    await absent(file); await absent(part);
    const handle = await open(part, 'wx');
    try { await handle.writeFile(`${JSON.stringify(value)}\n`); await handle.sync(); } finally { await handle.close(); }
    await absent(file); await rename(part, file);
    this.#written.add(name); this.#observed.set(name, await readLocal(file, 4096));
  }
}
