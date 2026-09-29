import { parseConfig } from '../connect-voice-tools/inspect.mjs';

// Metadata rechecked 2026-09-29 at the author's official Modrinth release:
// https://api.modrinth.com/v2/version/VS5X7jZd
// Its published SHA-512 was previously matched to the cached JAR, from which
// this SHA-256 was computed. This pin performs no download or redistribution.
// The mod's license remains All Rights Reserved; this is a local lab profile.
export const SVC_ARTIFACT = Object.freeze({
  file: 'mods/voicechat-fabric-1.21.1-2.6.22.jar',
  url: 'https://cdn.modrinth.com/data/9eGKb6K1/versions/VS5X7jZd/voicechat-fabric-1.21.1-2.6.22.jar',
  bytes: 5369475,
  algorithm: 'sha256',
  digest: '972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36',
});

export const SVC_CONFIG_PATH = 'config/voicechat/voicechat-server.properties';
export const SVC_BASELINE_BYTES = Buffer.from([
  '# Disposable Aurora Connect Simple Voice Chat server lab.',
  '# An explicit loopback bind avoids the mod fallback to a wildcard bind.',
  'port=43002',
  'bind_address=127.0.0.1',
  'voice_host=',
  '',
].join('\n'), 'utf8');

const fail = code => { throw new Error(code); };

// Pure, bounded observation of supplied bytes. No filesystem, network, Java,
// config mutation or assertion that the mod actually bound this UDP endpoint.
// Unknown ordinary keys are retained only in the count, never in the result.
export function validateSvcConfig(bytes) {
  let parsed; let text;
  try {
    parsed = parseConfig('svc', bytes);
    // Preserve a BOM here: Java-properties syntax does not treat it as leading
    // whitespace. The shared inspector is broader than this boot precondition.
    text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
  } catch { fail('runtime_svc_config_invalid'); }

  for (const original of text.split(/\r?\n/)) {
    const line = original.replace(/^[ \t]*/, '');
    if (line === '' || line.startsWith('#') || line.startsWith('!')) continue;
    const assignment = /^([a-zA-Z0-9_-]+)[ \t]*=[ \t]*(.*)$/.exec(line);
    // Java-properties syntactic whitespace is not JavaScript's Unicode \s.
    // Compare values too, so NBSP/EM SPACE after '=' cannot disappear through
    // the shared parser and turn a different Java value into an approved one.
    if (!assignment || parsed.entries.get(assignment[1])?.value !== assignment[2]) {
      fail('runtime_svc_config_invalid');
    }
  }
  if (parsed.entries.get('bind_address')?.value !== '127.0.0.1') fail('runtime_svc_bind_invalid');
  if (parsed.entries.get('port')?.value !== '43002') fail('runtime_svc_port_invalid');
  if (parsed.entries.get('voice_host')?.value !== '') fail('runtime_svc_announcement_invalid');

  return Object.freeze({
    sha256: parsed.sourceSha256,
    size: parsed.sourceBytes,
    bindLoopback: true,
    port: 43002,
    announcementEmpty: true,
    keyCount: parsed.entries.size,
  });
}
