import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, writeFile, mkdtemp, rm } from 'node:fs/promises';
import { createSocket } from 'node:dgram';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createPlan, parseConfig, probeGuestPort, readConfig, redactPlan, sha256 } from './inspect.mjs';

const svc = await readFile(new URL('./fixtures/svc.properties', import.meta.url));
const plasmo = await readFile(new URL('./fixtures/plasmo.toml', import.meta.url));
const base = { mod: 'svc', version: '2.6.22', mode: 'dedicated-aurora-only', guestPort: 35554,
  datagramBudget: 1400, transportPayload: 1450, installation: 'declared_present', portProbe: 'available_at_probe' };

test('SVC produces only announcement diff and immutable original digest', () => {
  const before = Buffer.from(svc);
  const plan = createPlan(base, svc);
  assert.equal(plan.sourceSha256, sha256(svc));
  assert.deepEqual(svc, before);
  assert.deepEqual(plan.changes, [{ key: 'voice_host', line: 4, before: '', after: '127.0.0.1:35554' }]);
  assert.equal(plan.host.portResolved, 24454);
  assert.equal(plan.guestBridge.port, 35554);
  assert.equal(plan.mtu.lowersModMtu, false);
  assert.equal(plan.applySupported, false);
  assert.equal(plan.runtimeHomologated, false);
  assert.deepEqual(plan.blockers, ['artifact_metadata_hash_and_runtime_not_verified']);
});

test('Plasmo keeps bind, inherited game port and public announcement separate', () => {
  const options = { ...base, mod: 'plasmo', version: '2.1.17', gamePort: 25565 };
  const plan = createPlan(options, plasmo);
  assert.equal(plan.host.bindResolved, '127.0.0.1');
  assert.equal(plan.host.portConfigured, 0);
  assert.equal(plan.host.portResolved, 25565);
  assert.equal(plan.advertisement.ip, 'example.invalid');
  assert.deepEqual(plan.changes.map(change => change.key), ['host.public.ip', 'host.public.port']);
  assert.equal(plan.mtu.configuredValue, 1024);
  assert(createPlan({ ...options, gamePort: undefined }, plasmo).blockers.includes('host_port_unresolved'));
});

test('Plasmo can draft missing public table, without materializing a file', () => {
  const bytes = Buffer.from('[host]\nip="127.0.0.1"\nport=25565\n[voice]\nmtu_size=1024\n');
  const plan = createPlan({ ...base, mod: 'plasmo', version: '2.1.17' }, bytes);
  assert(plan.changes.every(change => change.line === null && change.before === null));
});

test('unknown versions, missing installation, LAN and mixed modes never propose global override', () => {
  for (const overrides of [{ version: '9.9.9' }, { installation: 'absent' }, { mode: 'lan' }, { mode: 'mixed' }]) {
    const plan = createPlan({ ...base, ...overrides }, svc);
    assert.equal(plan.changes.length, 0);
    assert(plan.blockers.length > 1);
  }
});

test('missing effective values are unresolved; no port default inferred by mod name', () => {
  const plan = createPlan({ ...base, gamePort: undefined }, Buffer.from('port=-1\nbind_address=\nvoice_host=\n'));
  for (const code of ['host_port_unresolved', 'host_bind_unresolved', 'mod_mtu_unresolved']) assert(plan.blockers.includes(code));
  assert.equal(plan.host.portResolved, null);
  const resolved = createPlan({ ...base, gamePort: 25565, gameBind: '127.0.0.1' }, Buffer.from('port=-1\nbind_address=\nmtu_size=1275\n'));
  assert.equal(resolved.host.portResolved, 25565);
  assert.equal(resolved.host.bindResolved, '127.0.0.1');
});

test('MTU incompatibility is explicit and never reduces config value', () => {
  for (const [overrides, code] of [
    [{ transportPayload: 1200 }, 'transport_payload_too_small'],
    [{ datagramBudget: 1100 }, 'datagram_budget_below_configured_mtu'],
    [{ datagramBudget: undefined }, 'full_udp_datagram_budget_required'],
    [{ transportPayload: undefined }, 'negotiated_transport_payload_required'],
  ]) {
    const plan = createPlan({ ...base, ...overrides }, svc);
    assert(plan.blockers.includes(code));
    assert(!plan.changes.some(change => change.key.includes('mtu')));
  }
});

test('parser refuses duplicate/escaped/continued properties and oversized/non-UTF8 input', () => {
  for (const bytes of [Buffer.from('port=1\nport=2'), Buffer.from('voice_host=a\\\nb'), Buffer.from('voice_host=\\u0031'), Buffer.alloc(256 * 1024 + 1), Buffer.from([0xff]), Buffer.from('x=\u0000')]) {
    assert.throws(() => parseConfig('svc', bytes));
  }
});

test('TOML subset rejects ambiguous and unsupported constructs rather than guessing', () => {
  for (const text of ['[host]\nport=1\nport=2', '[host]\nip="a"\n[host]\nport=2', 'host="x"\n[host.public]\nport=1', '[host]\npublic=2\n[host.public]\nport=1', '[host]\nip="""a"""', '[[host]]', 'host.port=123', '[host]\nip="a\\nb"', '[host]\na={x=1}', '[voice]\na=["x,y"]']) {
    assert.throws(() => parseConfig('plasmo', Buffer.from(text)), text);
  }
});

test('TOML quoted hash and inline comment are distinguished', () => {
  const parsed = parseConfig('plasmo', Buffer.from('[host] # section\nip = "a#b" # note\nport = 123\n'));
  assert.equal(parsed.entries.get('host.ip').value, 'a#b');
  assert.equal(parsed.entries.get('host.port').value, 123);
});

test('properties preserves significant trailing whitespace and caps parsing work', () => {
  assert.equal(parseConfig('svc', Buffer.from('voice_host=example.invalid  \n')).entries.get('voice_host').value, 'example.invalid  ');
  assert.throws(() => parseConfig('svc', Buffer.from('a=1\rb=2')));
  assert.throws(() => parseConfig('svc', Buffer.from(Array.from({ length: 513 }, (_, i) => `key${i}=1`).join('\n'))));
});

test('real file read and CLI preserve bytes; error output never prints paths', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'aurora-voice-inspect-'));
  try {
    const file = join(directory, 'voice.properties');
    await writeFile(file, svc);
    assert.deepEqual(await readConfig(file), svc);
    const output = execFileSync(process.execPath, [fileURLToPath(new URL('./inspect.mjs', import.meta.url)), '--mod', 'svc', '--version', '2.6.22', '--mode', 'dedicated-aurora-only', '--config', file, '--guest-port', '35554'], { encoding: 'utf8' });
    assert.equal(JSON.parse(output).sourceSha256, sha256(svc));
    assert.deepEqual(await readFile(file), svc);
    let error;
    try { execFileSync(process.execPath, [fileURLToPath(new URL('./inspect.mjs', import.meta.url)), '--config', 'private-missing-file'], { stdio: 'pipe' }); }
    catch (failure) { error = failure; }
    assert.equal(error.status, 1);
    assert(!error.stderr.toString().includes('private-missing-file'));
  } finally {
    assert(directory.startsWith(join(tmpdir(), 'aurora-voice-inspect-')));
    await rm(directory, { recursive: true, force: true });
  }
});

test('real loopback UDP probe observes occupancy, closes and permits rebind', async () => {
  const held = createSocket({ type: 'udp4', reuseAddr: false });
  await new Promise((resolve, reject) => { held.once('error', reject); held.bind(0, '127.0.0.1', resolve); });
  const number = held.address().port;
  assert.equal(await probeGuestPort(number), 'port_occupied');
  await new Promise(resolve => held.close(resolve));
  assert.equal(await probeGuestPort(number), 'available_at_probe');
  assert.equal(await probeGuestPort(number), 'available_at_probe');
});

test('a changed file invalidates plan digest and unknown flags do not authorize writes', () => {
  assert.notEqual(createPlan(base, svc).sourceSha256, createPlan(base, Buffer.concat([svc, Buffer.from('# edit\n')])).sourceSha256);
  assert.throws(() => createPlan({ ...base, guestPort: 0 }, svc));
  assert.throws(() => createPlan({ ...base, installation: 'homologated' }, svc));
});

test('shareable result hides existing announcement/bind and retains actionable loopback proposal', () => {
  const plan = createPlan({ ...base, mod: 'plasmo', version: '2.1.17', gamePort: 25565 }, plasmo);
  const redacted = redactPlan(plan);
  assert(!JSON.stringify(redacted).includes('example.invalid'));
  assert.equal(redacted.host.bindConfigured, '[redacted]');
  assert.equal(redacted.changes[0].after, '127.0.0.1');
  assert.equal(plan.advertisement.ip, 'example.invalid');
});
