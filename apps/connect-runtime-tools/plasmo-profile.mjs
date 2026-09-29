import { parseConfig } from '../connect-voice-tools/inspect.mjs';

// Official metadata and cached bytes checked 2026-09-29:
// https://api.modrinth.com/v2/version/6Q2QmcUp (Plasmo, LGPL-3.0-only)
// https://api.modrinth.com/v2/version/ID8pq1x1 (Fabric API, Apache-2.0)
// Both published SHA-512/SHA-1 pairs matched the selected local bytes. The
// Plasmo bytes equal the previously fixed GitHub artifact; no build is swapped.
// These pins do not download, execute or redistribute either dependency.
export const PLASMO_ARTIFACT = Object.freeze({
  file: 'mods/plasmovoice-fabric-1.21.1-2.1.17.jar',
  url: 'https://cdn.modrinth.com/data/1bZhdhsH/versions/6Q2QmcUp/plasmovoice-fabric-1.21.1-2.1.17.jar',
  bytes: 12365966,
  algorithm: 'sha256',
  digest: '46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b',
});
export const FABRIC_API_ARTIFACT = Object.freeze({
  file: 'mods/fabric-api-0.116.10+1.21.1.jar',
  url: 'https://cdn.modrinth.com/data/P7dR8mSH/versions/ID8pq1x1/fabric-api-0.116.10%2B1.21.1.jar',
  bytes: 2425988,
  algorithm: 'sha256',
  digest: 'af0eb792940568fdb1f37bb65a2599773bc47d2010aedf5ec5ed0bd3b6fc40e6',
});

export const PLASMO_CONFIG_PATH = 'config/plasmovoice/server/config.toml';
export const PLASMO_BASELINE_BYTES = Buffer.from([
  '# Disposable Aurora Connect Plasmo Voice server lab.',
  'debug = false',
  'check_for_updates = false',
  'use_crowdin_translations = false',
  '',
  '[host]',
  'ip = "127.0.0.1"',
  'port = 43003',
  '',
].join('\n'), 'utf8');

const fail = code => { throw new Error(code); };

// The shared parser supplies bounded semantic entries. This lexical check only
// narrows its accepted subset: TOML whitespace is ASCII space/tab, and an empty
// public table must remain visible even though it has no semantic entries.
function codeOnLine(original) {
  let quote;
  for (let i = 0; i < original.length; i++) {
    const character = original[i];
    if (quote === character) quote = undefined;
    else if (!quote && (character === '"' || character === "'")) quote = character;
    else if (!quote && character === '#') return original.slice(0, i).replace(/^[ \t]+|[ \t]+$/g, '');
    else if (!quote && /\s/u.test(character) && character !== ' ' && character !== '\t') fail('runtime_plasmo_config_invalid');
  }
  return original.replace(/^[ \t]+|[ \t]+$/g, '');
}

// Pure observation of supplied bytes; no I/O, mutation, external query or claim
// that a mod is running. Optional server_id and all unrelated values are never
// returned, retained beyond parsing, or printed. Grammar outside this bounded
// subset is refused rather than normalized into an apparently safe config.
export function validatePlasmoConfig(bytes) {
  let parsed; let text;
  try {
    parsed = parseConfig('plasmo', bytes);
    text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
  } catch { fail('runtime_plasmo_config_invalid'); }
  // DEL is forbidden by TOML in comments and literal/basic strings alike;
  // unlike C0 controls, it is not excluded by the shared inspector's grammar.
  if (text.includes('\u007f')) fail('runtime_plasmo_config_invalid');
  let section = ''; let hostTable = false; let publicTable = false; let rawPort;
  for (const original of text.split(/\r?\n/)) {
    const line = codeOnLine(original);
    if (!line) continue;
    if (line.startsWith('[')) {
      // parseConfig already checked this exact unquoted table-header subset.
      section = line.slice(1, -1);
      hostTable ||= section === 'host';
      publicTable ||= section === 'host.public' || section.startsWith('host.public.');
    } else {
      const assignment = /^([a-zA-Z0-9_-]+)[ \t]*=[ \t]*(.*)$/.exec(line);
      if (!assignment) fail('runtime_plasmo_config_invalid');
      if (section === 'host' && assignment[1] === 'port') rawPort = assignment[2];
    }
  }
  for (const key of ['debug', 'check_for_updates', 'use_crowdin_translations']) {
    if (parsed.entries.get(key)?.value !== false) fail('runtime_plasmo_flags_invalid');
  }
  if (publicTable || [...parsed.entries.keys()].some(key => key === 'host.public' || key.startsWith('host.public.'))) {
    fail('runtime_plasmo_announcement_invalid');
  }
  if (!hostTable || parsed.entries.get('host.ip')?.value !== '127.0.0.1') fail('runtime_plasmo_bind_invalid');
  // The inspector intentionally does not preserve number type information.
  // Require the fixed integer spelling too, so 43003.0 is never mistaken for an
  // integer port accepted by the mod's TOML configuration deserializer.
  if (parsed.entries.get('host.port')?.value !== 43003 || rawPort !== '43003') fail('runtime_plasmo_port_invalid');

  return Object.freeze({
    sha256: parsed.sourceSha256,
    size: parsed.sourceBytes,
    bindLoopback: true,
    port: 43003,
    announcementAbsent: true,
    keyCount: parsed.entries.size,
    updateChecks: false,
    translationsDownload: false,
  });
}
