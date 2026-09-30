import { createHash } from 'node:crypto';
import { createSocket } from 'node:dgram';
import { open, lstat, realpath } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { inspectArtifact, isVerifiedArtifact, readArtifactFile } from './artifact.mjs';
import { rejectLinkedParents, resolveExplicitLocalPath } from './local-path.mjs';

const MAX_CONFIG_BYTES = 256 * 1024;
const RESEARCH_VERSIONS = { svc: ['2.6.22'], plasmo: ['2.1.17'] };
export const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const fail = code => { throw new Error(code); };
const port = value => Number.isInteger(value) && value > 0 && value <= 65535;

// This deliberately accepts a small, unambiguous text subset. It is not a
// Java-properties or TOML serializer. Unsupported syntax is refused, never fixed.
export function parseConfig(mod, bytes) {
  if (!['svc', 'plasmo'].includes(mod)) fail('unsupported_mod');
  if (!Buffer.isBuffer(bytes) || bytes.length > MAX_CONFIG_BYTES) fail('config_size');
  let text;
  try { text = new TextDecoder('utf-8', { fatal: true }).decode(bytes); }
  catch { fail('config_utf8'); }
  if (/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/.test(text)) fail('config_control_character');
  if (text.replaceAll('\r\n', '').includes('\r')) fail('unsupported_line_ending');
  const lines = text.split(/\r?\n/);
  if (lines.length > 4096) fail('config_line_limit');
  const entries = new Map();
  const sections = new Set();
  let section = '';
  for (const [index, original] of lines.entries()) {
    let line = mod === 'svc' ? original.trimStart() : original.trim();
    if (!line.trim() || line.startsWith('#') || (mod === 'svc' && line.startsWith('!'))) continue;
    if (line.includes('\\')) fail('unsupported_escape_or_continuation');
    if (mod === 'plasmo') {
      // Find comments outside single/double quoted values; reject multiline strings.
      let quote = null;
      for (let i = 0; i < line.length; i++) {
        const c = line[i];
        if (quote === c) quote = null;
        else if (!quote && (c === '"' || c === "'")) quote = c;
        else if (!quote && c === '#') { line = line.slice(0, i).trim(); break; }
      }
      if (quote || /"""|'''/.test(line)) fail('unsupported_multiline_string');
      if (line.startsWith('[')) {
        const match = /^\[([a-zA-Z0-9_-]+(?:\.[a-zA-Z0-9_-]+)*)\]$/.exec(line);
        if (!match || sections.has(match[1])) fail('ambiguous_table');
        section = match[1];
        sections.add(section);
        if (sections.size > 128) fail('config_table_limit');
        continue;
      }
    }
    const match = /^([a-zA-Z0-9_-]+)\s*=\s*(.*)$/.exec(line);
    if (!match) fail('unsupported_assignment');
    const key = mod === 'plasmo' && section ? `${section}.${match[1]}` : match[1];
    if (entries.has(key)) fail('duplicate_key');
    let value = match[2];
    if (mod === 'plasmo') {
      const scalar = /^(?:"[^"\\]*"|'[^'\\]*'|true|false|-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?)$/;
      if (scalar.test(value)) {
        if (value.startsWith('"') || value.startsWith("'")) value = value.slice(1, -1);
        else if (value === 'true' || value === 'false') value = value === 'true';
        else value = Number(value);
      } else if (/^\[.*\]$/.test(value)) {
        const items = value.slice(1, -1).trim();
        if (items && !items.split(',').every(item => scalar.test(item.trim()))) fail('unsupported_array');
        value = { opaqueArray: true };
      } else fail('unsupported_toml_value');
    }
    entries.set(key, { value, line: index + 1 });
    if (entries.size > 512) fail('config_key_limit');
  }
  if (mod === 'plasmo') {
    for (const key of entries.keys()) {
      if ([...entries.keys(), ...sections].some(other => other !== key && other.startsWith(`${key}.`)) || sections.has(key)) fail('scalar_table_collision');
    }
  }
  return { entries, sourceSha256: sha256(bytes), sourceBytes: bytes.length };
}

export async function readConfig(file) {
  // Explicit path only; never walk an instance, find accounts or infer config paths.
  const absolute = resolveExplicitLocalPath(file);
  await rejectLinkedParents(absolute);
  const before = await lstat(absolute);
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1) fail('unsafe_config_file');
  if ((await realpath(absolute)).toLowerCase() !== absolute.toLowerCase()) fail('linked_config_path');
  if (before.size > MAX_CONFIG_BYTES) fail('config_size');
  const handle = await open(absolute, 'r');
  try {
    const current = await handle.stat();
    if (current.ino !== before.ino || current.dev !== before.dev || current.size !== before.size) fail('config_changed');
    const bytes = Buffer.alloc(MAX_CONFIG_BYTES + 1);
    let offset = 0;
    while (offset < bytes.length) {
      const { bytesRead } = await handle.read(bytes, offset, bytes.length - offset, null);
      if (!bytesRead) break;
      offset += bytesRead;
    }
    const after = await handle.stat();
    if (after.mtimeMs !== current.mtimeMs || after.size !== current.size || after.ctimeMs !== current.ctimeMs) fail('config_changed');
    if (offset > MAX_CONFIG_BYTES) fail('config_size');
    return bytes.subarray(0, offset);
  } finally { await handle.close(); }
}

export async function probeGuestPort(guestPort) {
  if (!port(guestPort)) fail('invalid_guest_port');
  const socket = createSocket({ type: 'udp4', reuseAddr: false });
  return await new Promise(resolveProbe => {
    let settled = false;
    const finish = result => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      try { socket.close(); } catch { /* bind failed before opening a handle */ }
      resolveProbe(result);
    };
    const timer = setTimeout(() => finish('probe_timeout'), 1000);
    socket.once('error', error => finish(error.code === 'EADDRINUSE' ? 'port_occupied' : 'probe_failed'));
    socket.bind(guestPort, '127.0.0.1', () => finish('available_at_probe'));
  });
}

export function createPlan(options, bytes) {
  const { mod, mode, guestPort, gamePort, gameBind, datagramBudget, transportPayload, installation = 'not_checked', portProbe = 'not_checked' } = options;
  const artifactVerified = isVerifiedArtifact(options.artifact);
  const version = artifactVerified ? options.artifact.version : options.version;
  const targetLoader = options.loader ?? 'fabric';
  const targetMinecraft = options.minecraft ?? '1.21.1';
  const parsed = parseConfig(mod, bytes);
  const get = key => parsed.entries.get(key)?.value;
  const blockers = [];
  if (!port(guestPort)) fail('invalid_guest_port');
  if (!['dedicated-aurora-only', 'mixed', 'lan'].includes(mode)) fail('invalid_mode');
  if (!['not_checked', 'absent', 'declared_present'].includes(installation)) fail('invalid_installation_observation');
  if (!['not_checked', 'available_at_probe', 'port_occupied', 'probe_failed', 'probe_timeout'].includes(portProbe)) fail('invalid_port_observation');
  if (!RESEARCH_VERSIONS[mod].includes(version)) blockers.push('version_not_researched');
  if (mode === 'mixed') blockers.push('per_participant_adapter_required');
  if (mode === 'lan') blockers.push('lan_effective_config_not_verified');
  if (installation === 'absent') blockers.push('mod_absent');
  // A boolean or deserialized report cannot stand in for this process inspecting bytes.
  let artifactMismatch = false;
  if (artifactVerified) {
    if (options.artifact.mod !== mod) { blockers.push('artifact_mod_mismatch'); artifactMismatch = true; }
    if (options.version !== undefined && options.version !== version) { blockers.push('artifact_version_mismatch'); artifactMismatch = true; }
    if (options.artifact.loader !== targetLoader || options.artifact.minecraft !== targetMinecraft) { blockers.push('artifact_target_mismatch'); artifactMismatch = true; }
    blockers.push('voice_runtime_not_verified');
  } else {
    blockers.push('artifact_metadata_hash_and_runtime_not_verified');
    if (options.artifact !== undefined) { blockers.push('artifact_not_recognized_or_untrusted_report'); artifactMismatch = true; }
  }
  if (portProbe !== 'available_at_probe') blockers.push(portProbe === 'not_checked' ? 'guest_port_not_probed' : portProbe);

  const integer = key => {
    const value = get(key);
    const parsed = mod === 'svc' && typeof value === 'string' && /^-?(0|[1-9][0-9]*)$/.test(value)
      ? Number(value) : value;
    if (Number.isSafeInteger(parsed)) return parsed;
    return null;
  };
  const bindKey = mod === 'svc' ? 'bind_address' : 'host.ip';
  const portKey = mod === 'svc' ? 'port' : 'host.port';
  const mtuKey = mod === 'svc' ? 'mtu_size' : 'voice.mtu_size';
  const rawPort = integer(portKey);
  let hostPort = rawPort;
  if ((mod === 'svc' && rawPort === -1) || (mod === 'plasmo' && rawPort === 0)) {
    hostPort = port(gamePort) ? gamePort : null;
  }
  if (!port(hostPort)) { hostPort = null; blockers.push('host_port_unresolved'); }
  const bindValue = get(bindKey);
  let hostBind = typeof bindValue === 'string' ? bindValue : null;
  if (mod === 'svc' && hostBind === '') hostBind = typeof gameBind === 'string' && gameBind.length ? gameBind : null;
  if (hostBind === null) blockers.push('host_bind_unresolved');
  const configuredMtu = integer(mtuKey);
  if (configuredMtu === null || configuredMtu <= 0) blockers.push('mod_mtu_unresolved');
  if (!Number.isInteger(datagramBudget) || datagramBudget < 1 || datagramBudget > 65507) blockers.push('full_udp_datagram_budget_required');
  if (!Number.isInteger(transportPayload) || transportPayload < 1 || transportPayload > 65507) blockers.push('negotiated_transport_payload_required');
  // A configured audio ceiling above the entire negotiated UDP payload is
  // already a conservative incompatibility, even before a full packet bound
  // is measured. The inverse never proves that protocol/cipher overhead fits.
  if (configuredMtu !== null && configuredMtu > 0 && Number.isInteger(transportPayload)
    && transportPayload >= 1 && transportPayload <= 65507 && configuredMtu > transportPayload) {
    blockers.push('configured_mtu_exceeds_transport_payload');
  }
  if (Number.isInteger(datagramBudget) && Number.isInteger(transportPayload) && datagramBudget > transportPayload) blockers.push('transport_payload_too_small');
  if (configuredMtu !== null && Number.isInteger(datagramBudget) && configuredMtu > datagramBudget) blockers.push('datagram_budget_below_configured_mtu');

  const changes = [];
  const canDraft = mode === 'dedicated-aurora-only' && RESEARCH_VERSIONS[mod].includes(version) && installation !== 'absent' && !artifactMismatch;
  if (canDraft) {
    const wanted = mod === 'svc'
      ? [['voice_host', `127.0.0.1:${guestPort}`]]
      : [['host.public.ip', '127.0.0.1'], ['host.public.port', guestPort]];
    for (const [key, after] of wanted) {
      const before = get(key);
      if (before !== undefined && typeof before !== typeof after) fail('invalid_announcement_type');
      if (before !== after) changes.push({ key, line: parsed.entries.get(key)?.line ?? null, before: before ?? null, after });
    }
  }
  return {
    schemaVersion: 1, state: 'PROTÓTIPO', applySupported: false, runtimeHomologated: false,
    mod, declaredVersion: options.version ?? null, effectiveVersion: version ?? null, mode,
    artifact: artifactVerified ? options.artifact : null,
    artifactVerified, target: { loader: targetLoader, minecraft: targetMinecraft },
    sourceSha256: parsed.sourceSha256, sourceBytes: parsed.sourceBytes,
    host: { bindConfigured: bindValue ?? null, bindResolved: hostBind, portConfigured: rawPort, portResolved: hostPort, protocol: 'udp' },
    advertisement: mod === 'svc' ? { voiceHost: get('voice_host') ?? null } : { ip: get('host.public.ip') ?? null, port: get('host.public.port') ?? null },
    guestBridge: { address: '127.0.0.1', port: guestPort, protocol: 'udp', probe: portProbe },
    mtu: { configuredValue: configuredMtu, fullDatagramBudget: datagramBudget ?? null, negotiatedPayload: transportPayload ?? null, lowersModMtu: false },
    blockers: [...new Set(blockers)],
    changes,
    // Structured diff contains only allowlisted address fields, never the whole config.
    diff: changes.map(change => ({ remove: { [change.key]: change.before }, add: { [change.key]: change.after } })),
    warnings: [
      'Draft only; no file is written and this is not authorization to expose a service.',
      'Global loopback announcement requires an active bridge at this port on EVERY participating machine, including the host client; mixed mode stays disabled.',
      'Probe releases the socket immediately; the bridge must bind again and handle races.',
      'Mod mtu_size may omit protocol overhead; supply an observed full UDP datagram bound and the current transport payload after Aurora framing.',
      'Config addresses are local diagnostic data; redact them before sharing this JSON.',
      'Applying later requires confined native paths, instance identity, lock, explicit authorization, hash recheck, snapshot, journal and conflict-safe recovery.',
      'An explicit JAR proves those file bytes only; it does not prove installation in this instance, dependencies, mod authentication, audible voice or runtime compatibility.',
    ],
  };
}

export function redactPlan(plan) {
  const copy = structuredClone(plan);
  for (const key of ['bindConfigured', 'bindResolved']) if (copy.host[key] !== null) copy.host[key] = '[redacted]';
  for (const key of Object.keys(copy.advertisement)) if (copy.advertisement[key] !== null) copy.advertisement[key] = '[redacted]';
  for (const change of copy.changes) if (change.before !== null) change.before = '[redacted]';
  for (const change of copy.diff) for (const key of Object.keys(change.remove)) if (change.remove[key] !== null) change.remove[key] = '[redacted]';
  copy.addressesRedacted = true;
  return copy;
}

async function main(argv) {
  const allowed = new Set(['mod', 'version', 'mode', 'config', 'guest-port', 'game-port', 'game-bind', 'datagram-budget', 'transport-payload', 'installation', 'probe-port', 'show-addresses', 'jar', 'loader', 'minecraft']);
  const args = {};
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i]?.slice(2);
    if (!argv[i]?.startsWith('--') || !allowed.has(key) || !argv[i + 1] || args[key] !== undefined) fail('invalid_arguments');
    args[key] = argv[i + 1];
  }
  for (const key of ['mod', 'mode', 'config', 'guest-port']) if (!args[key]) fail('missing_argument');
  if (!args.version && !args.jar) fail('version_or_jar_required');
  if (args['probe-port'] !== undefined && args['probe-port'] !== 'true') fail('invalid_probe_flag');
  if (args['show-addresses'] !== undefined && args['show-addresses'] !== 'true') fail('invalid_address_flag');
  const guestPort = Number(args['guest-port']);
  const bytes = await readConfig(args.config);
  const artifact = args.jar ? inspectArtifact(await readArtifactFile(args.jar)) : undefined;
  const optionalNumber = key => args[key] === undefined ? undefined : Number(args[key]);
  const plan = createPlan({ mod: args.mod, version: args.version, mode: args.mode, guestPort, artifact, loader: args.loader, minecraft: args.minecraft,
    gamePort: optionalNumber('game-port'), gameBind: args['game-bind'],
    datagramBudget: optionalNumber('datagram-budget'), transportPayload: optionalNumber('transport-payload'),
    installation: args.installation,
    portProbe: args['probe-port'] === 'true' ? await probeGuestPort(guestPort) : 'not_checked',
  }, bytes);
  process.stdout.write(`${JSON.stringify(args['show-addresses'] === 'true' ? plan : redactPlan(plan), null, 2)}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main(process.argv.slice(2)).catch(error => {
    const code = /^[a-z_]+$/.test(error.message) ? error.message : 'read_or_inspection_failed';
    process.stderr.write(`${JSON.stringify({ error: code })}\n`);
    process.exitCode = 1;
  });
}
