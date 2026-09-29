import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { PLASMO_ARTIFACT, FABRIC_API_ARTIFACT, PLASMO_CONFIG_PATH, PLASMO_BASELINE_BYTES,
  validatePlasmoConfig } from './plasmo-profile.mjs';

const flags = 'debug=false\ncheck_for_updates=false\nuse_crowdin_translations=false\n';
const host = '[host]\nip="127.0.0.1"\nport=43003\n';
const base = flags + host;
const inspect = text => validatePlasmoConfig(Buffer.from(text, 'utf8'));
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
function rejected(bytes, code) {
  assert.throws(() => validatePlasmoConfig(bytes), error => {
    assert.equal(error.message, code);
    assert.match(error.message, /^runtime_plasmo_[a-z_]+$/);
    assert.equal(error.cause, undefined);
    return true;
  });
}

test('Plasmo and Fabric API pins identify the exact verified artifacts on the official CDN', () => {
  assert.deepEqual(PLASMO_ARTIFACT, {
    file: 'mods/plasmovoice-fabric-1.21.1-2.1.17.jar',
    url: 'https://cdn.modrinth.com/data/1bZhdhsH/versions/6Q2QmcUp/plasmovoice-fabric-1.21.1-2.1.17.jar',
    bytes: 12365966, algorithm: 'sha256',
    digest: '46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b',
  });
  assert.deepEqual(FABRIC_API_ARTIFACT, {
    file: 'mods/fabric-api-0.116.10+1.21.1.jar',
    url: 'https://cdn.modrinth.com/data/P7dR8mSH/versions/ID8pq1x1/fabric-api-0.116.10%2B1.21.1.jar',
    bytes: 2425988, algorithm: 'sha256',
    digest: 'af0eb792940568fdb1f37bb65a2599773bc47d2010aedf5ec5ed0bd3b6fc40e6',
  });
  for (const pin of [PLASMO_ARTIFACT, FABRIC_API_ARTIFACT]) {
    assert.ok(Object.isFrozen(pin));
    assert.throws(() => { pin.digest = '0'.repeat(64); }, TypeError);
  }
  assert.equal(PLASMO_CONFIG_PATH, 'config/plasmovoice/server/config.toml');
});

test('baseline explicitly disables external update/translation checks and fixes the private endpoint', () => {
  const before = Buffer.from(PLASMO_BASELINE_BYTES);
  const result = validatePlasmoConfig(PLASMO_BASELINE_BYTES);
  assert.deepEqual(result, { sha256: sha256(before), size: before.length, bindLoopback: true, port: 43003,
    announcementAbsent: true, keyCount: 5, updateChecks: false, translationsDownload: false });
  assert.deepEqual(PLASMO_BASELINE_BYTES, before);
  assert.deepEqual(validatePlasmoConfig(before), result);
  assert.ok(Object.isFrozen(result));
});

test('optional identifiers, comments, arrays and unrelated values appear only in the hash and key count', () => {
  const text = 'server_id="00000000-0000-4000-8000-000000000001"\n'
    + 'private_fixture="PRIVATE_SENTINEL_VALUE"\n' + base
    + '[voice]\nvalues=[8,16,32]\n# PRIVATE_SENTINEL_COMMENT\n';
  const result = inspect(text);
  assert.equal(result.keyCount, 8);
  assert.equal(result.sha256, sha256(Buffer.from(text)));
  assert.deepEqual(Object.keys(result).sort(), ['sha256', 'size', 'bindLoopback', 'port', 'announcementAbsent',
    'keyCount', 'updateChecks', 'translationsDownload'].sort());
  assert.doesNotMatch(JSON.stringify(result), /server_id|00000000|PRIVATE|127\.0\.0\.1/);
});

test('CRLF, ASCII syntax whitespace, equivalent quotes and Unicode in comments or strings stay bounded and exact', () => {
  const text = '\t# comentário\r\nlabel="Unicode \u00a0 value and # literal"\r\n'
    + '\tdebug = false # local\r\ncheck_for_updates=false\r\nuse_crowdin_translations=false\r\n'
    + '[host] # end\r\nport = 43003 # integer\r\nip = \'127.0.0.1\'\r\n';
  assert.equal(inspect(text).keyCount, 6);
  assert.equal(inspect(text).sha256, sha256(Buffer.from(text)));
  assert.notEqual(inspect(text).sha256, inspect(text.replaceAll('\r\n', '\n')).sha256);
});

test('all three root flags must be present and boolean false', () => {
  for (const key of ['debug', 'check_for_updates', 'use_crowdin_translations']) {
    for (const value of ['true', '"false"', '0', '[]']) {
      rejected(Buffer.from(base.replace(`${key}=false`, `${key}=${value}`)), 'runtime_plasmo_flags_invalid');
    }
    rejected(Buffer.from(base.replace(`${key}=false\n`, '')), 'runtime_plasmo_flags_invalid');
  }
  rejected(Buffer.from('[other]\n' + base), 'runtime_plasmo_flags_invalid');
});

test('bind must be the exact loopback string in the explicit host table', () => {
  for (const value of ['"0.0.0.0"', '""', '"::"', '"::1"', '"::ffff:127.0.0.1"',
    '"127.0.0.2"', '"localhost"', '"127.0.0.1 "', '0', '[]']) {
    rejected(Buffer.from(base.replace('"127.0.0.1"', value)), 'runtime_plasmo_bind_invalid');
  }
  rejected(Buffer.from(base.replace('ip="127.0.0.1"\n', '')), 'runtime_plasmo_bind_invalid');
  rejected(Buffer.from(base.replace('[host]', '[elsewhere]')), 'runtime_plasmo_bind_invalid');
});

test('port must be the exact integer 43003 rather than a default, float or string', () => {
  for (const value of ['0', '-1', '24454', '43002', '65536', '43003.0', '"43003"', '[]']) {
    rejected(Buffer.from(base.replace('port=43003', `port=${value}`)), 'runtime_plasmo_port_invalid');
  }
  rejected(Buffer.from(base.replace('port=43003\n', '')), 'runtime_plasmo_port_invalid');
});

test('public announcement must be absent including empty tables, descendants and scalar aliases', () => {
  for (const suffix of ['[host.public]\n', '[host.public]\nip="127.0.0.1"\nport=43003\n',
    '[host.public.extra]\n', 'public="PRIVATE_ANNOUNCEMENT"\n', 'public=[]\n']) {
    rejected(Buffer.from(base + suffix), 'runtime_plasmo_announcement_invalid');
  }
  assert.equal(inspect(base + '# [host.public]\n# ip="127.0.0.1"\n').announcementAbsent, true);
  assert.equal(inspect('label="[host.public]"\n' + base).announcementAbsent, true);
});

test('ambiguous TOML aliases, duplicate keys/tables, escapes and unsupported syntax are refused', () => {
  for (const text of [base + 'port=43003\n', base + '[host]\n', base + '"public" = "x"\n',
    base + 'public.ip="127.0.0.1"\n', base + 'public={ip="127.0.0.1"}\n',
    base + '[[host.public]]\n', base.replace('127.0.0.1', '127\\u002e0.0.1'),
    base + 'other="""multiline"""\n', 'debug=false\n' + base]) {
    rejected(Buffer.from(text), 'runtime_plasmo_config_invalid');
  }
});

test('Unicode whitespace and BOM are rejected outside comments or quoted strings', () => {
  for (const text of ['\uFEFF' + base, '\u00a0' + base, base.replace('debug=', 'debug\u2003='),
    base.replace('debug=false', 'debug=\u00a0false'), base.replace('debug=false', 'debug=false\u00a0'),
    base.replace('[host]', '\u2003[host]'), base.replace('port=43003', 'port=\u00a043003'),
    base + '\u00a0# comment\n', base + '\u2003\n']) {
    rejected(Buffer.from(text), 'runtime_plasmo_config_invalid');
  }
});

test('invalid bytes, controls, bare CR and unsupported input types remain redacted', () => {
  for (const bytes of [Buffer.from([0xff]), Buffer.from(base + 'x="\u0000"\n'),
    Buffer.from(base.replace('\n', '\r')), Buffer.from(base + '\u000c'),
    Buffer.from(base + 'x="\u007f"\n'), Buffer.from(base + '# \u007f\n'),
    new Uint8Array(Buffer.from(base)), base, null]) rejected(bytes, 'runtime_plasmo_config_invalid');
});

test('input and parsing-work limits are inclusive and cannot be enlarged by comments or tables', () => {
  const exact = Buffer.from(base + '#' + 'x'.repeat(256 * 1024 - Buffer.byteLength(base) - 1));
  assert.equal(validatePlasmoConfig(exact).size, 256 * 1024);
  rejected(Buffer.concat([exact, Buffer.from('x')]), 'runtime_plasmo_config_invalid');
  const lines = base + '#\n'.repeat(4096 - base.split('\n').length);
  assert.equal(lines.split('\n').length, 4096);
  assert.equal(inspect(lines).keyCount, 5);
  rejected(Buffer.from(lines + '#\n'), 'runtime_plasmo_config_invalid');
  const keys = base + Array.from({ length: 507 }, (_, i) => `extra_${i}=0`).join('\n');
  assert.equal(inspect(keys).keyCount, 512);
  rejected(Buffer.from(keys + '\none_more=0'), 'runtime_plasmo_config_invalid');
  const tables = base + Array.from({ length: 127 }, (_, i) => `[table_${i}]`).join('\n');
  assert.equal(inspect(tables).keyCount, 5);
  rejected(Buffer.from(tables + '\n[one_more]'), 'runtime_plasmo_config_invalid');
});

test('complete equivalent defaults accept only after explicit isolation flags and endpoint changes', async () => {
  // Source-derived fixture only. No JVM or actual mod writer runs in this test.
  const original = await readFile(new URL('../desktop/src-tauri/tests/fixtures/connect_voice/plasmo-default-equivalent.toml', import.meta.url));
  rejected(original, 'runtime_plasmo_flags_invalid');
  const configured = Buffer.from(original.toString('utf8').replace('use_crowdin_translations = true', 'use_crowdin_translations = false')
    .replace('check_for_updates = true', 'check_for_updates = false').replace('ip = "0.0.0.0"', 'ip = "127.0.0.1"')
    .replace('port = 0', 'port = 43003'));
  const result = validatePlasmoConfig(configured);
  assert.equal(result.keyCount, 22);
  assert.equal(result.sha256, sha256(configured));
  assert.doesNotMatch(JSON.stringify(result), /server_id|00000000|en_us|VOIP/);
});
