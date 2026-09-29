import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { SVC_ARTIFACT, SVC_CONFIG_PATH, SVC_BASELINE_BYTES, validateSvcConfig } from './svc-profile.mjs';

const base = 'port=43002\nbind_address=127.0.0.1\nvoice_host=\n';
const inspect = text => validateSvcConfig(Buffer.from(text, 'utf8'));
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
function rejected(bytes, expected) {
  assert.throws(() => validateSvcConfig(bytes), error => {
    assert.equal(error.message, expected);
    assert.equal(error.cause, undefined);
    assert.match(error.message, /^runtime_svc_[a-z_]+$/);
    return true;
  });
}

test('SVC artifact is the fixed official release pin and cannot be reassigned', () => {
  assert.deepEqual(SVC_ARTIFACT, {
    file: 'mods/voicechat-fabric-1.21.1-2.6.22.jar',
    url: 'https://cdn.modrinth.com/data/9eGKb6K1/versions/VS5X7jZd/voicechat-fabric-1.21.1-2.6.22.jar',
    bytes: 5369475, algorithm: 'sha256',
    digest: '972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36',
  });
  assert.ok(Object.isFrozen(SVC_ARTIFACT));
  assert.throws(() => { SVC_ARTIFACT.url = 'https://example.invalid/'; }, TypeError);
  assert.equal(SVC_CONFIG_PATH, 'config/voicechat/voicechat-server.properties');
});

test('baseline explicitly fixes only the port, loopback bind and empty announcement', () => {
  assert.ok(Buffer.isBuffer(SVC_BASELINE_BYTES));
  const before = Buffer.from(SVC_BASELINE_BYTES);
  const report = validateSvcConfig(SVC_BASELINE_BYTES);
  assert.deepEqual(report, { sha256: sha256(before), size: before.length,
    bindLoopback: true, port: 43002, announcementEmpty: true, keyCount: 3 });
  assert.deepEqual(SVC_BASELINE_BYTES, before);
  assert.ok(Object.isFrozen(report));
  assert.deepEqual(validateSvcConfig(SVC_BASELINE_BYTES), report);
});

test('summary exposes no unknown keys, values, comments or configuration text', () => {
  const bytes = Buffer.from(`${base}private_fixture=PRIVATE_SENTINEL_ANSWER\n# PRIVATE_SENTINEL_COMMENT\n`);
  const report = validateSvcConfig(bytes);
  assert.deepEqual(Object.keys(report).sort(), ['announcementEmpty', 'bindLoopback', 'keyCount', 'port', 'sha256', 'size']);
  assert.equal(report.keyCount, 4);
  assert.equal(report.sha256, sha256(bytes));
  assert.equal(JSON.stringify(report).includes('PRIVATE'), false);
});

test('ordinary ASCII spacing, CRLF and Unicode comments are accepted with exact-byte hashes', () => {
  const text = '  # comentário\r\n\t! comentário\r\n \tport \t= \t43002\r\n'
    + '\tbind_address=127.0.0.1\r\nvoice_host= \t\r\n';
  const report = inspect(text);
  assert.equal(report.keyCount, 3);
  assert.equal(report.sha256, sha256(Buffer.from(text)));
  assert.notEqual(report.sha256, inspect(text.replaceAll('\r\n', '\n')).sha256);
});

test('only the explicit fixed bind is accepted; no inheritance or alternative loopback spelling', () => {
  for (const bind of ['', '0.0.0.0', '::', '::1', '*', 'localhost', '127.0.0.2', '127.0.0.1 ', 'PRIVATE_HOST']) {
    rejected(Buffer.from(base.replace('bind_address=127.0.0.1', `bind_address=${bind}`)), 'runtime_svc_bind_invalid');
  }
  rejected(Buffer.from(base.replace('bind_address=127.0.0.1\n', '')), 'runtime_svc_bind_invalid');
});

test('default, inherited, malformed and missing ports are refused', () => {
  for (const port of ['24454', '-1', '0', '43001', '65536', '043002', '+43002', '43002.0', '43002 ', 'PRIVATE_PORT']) {
    rejected(Buffer.from(base.replace('port=43002', `port=${port}`)), 'runtime_svc_port_invalid');
  }
  rejected(Buffer.from(base.replace('port=43002\n', '')), 'runtime_svc_port_invalid');
});

test('an absent or nonempty announcement is refused rather than supplying a default', () => {
  for (const value of ['127.0.0.1:43002', 'PRIVATE_HOST', '#not_a_comment', '""']) {
    rejected(Buffer.from(base.replace('voice_host=', `voice_host=${value}`)), 'runtime_svc_announcement_invalid');
  }
  rejected(Buffer.from(base.replace('voice_host=\n', '')), 'runtime_svc_announcement_invalid');
});

test('duplicates and unsupported Java-properties aliases cannot hide conflicting values', () => {
  for (const suffix of ['port=24454\n', 'bind_address=0.0.0.0\n', 'voice_host=PRIVATE\n',
    '  port =43002\n', 'port:43002\n', 'port 43002\n', 'bind_address=127\\u002e0.0.1\n', 'other=a\\\nb\n']) {
    rejected(Buffer.from(base + suffix), 'runtime_svc_config_invalid');
  }
});

test('Unicode whitespace and a BOM cannot change the Java interpretation of protected assignments', () => {
  for (const text of ['\uFEFF' + base, '\u00a0' + base, '\u2003' + base,
    base.replace('port=', 'port\u00a0='), base.replace('port=', 'port=\u00a0'),
    base.replace('bind_address=', 'bind_address\u2003='),
    base.replace('voice_host=', 'voice_host=\u00a0'), base.replace('voice_host=', 'voice_host=\u2003')]) {
    rejected(Buffer.from(text), 'runtime_svc_config_invalid');
  }
});

test('invalid UTF-8, control characters, bare CR and wrong input types are redacted', () => {
  for (const bytes of [Buffer.from([0xff]), Buffer.from(base + 'other=\u0000'),
    Buffer.from(base.replace('\n', '\r')), Buffer.from(base + 'other=\u000c'),
    new Uint8Array(Buffer.from(base)), base, null]) rejected(bytes, 'runtime_svc_config_invalid');
});

test('256 KiB and 4096 lines are strict inclusive input limits', () => {
  const exact = Buffer.from(base + '#' + 'x'.repeat(256 * 1024 - Buffer.byteLength(base) - 1));
  assert.equal(validateSvcConfig(exact).size, 256 * 1024);
  rejected(Buffer.concat([exact, Buffer.from('x')]), 'runtime_svc_config_invalid');
  const lines = base + '#\n'.repeat(4092);
  assert.equal(lines.split('\n').length, 4096);
  assert.equal(inspect(lines).keyCount, 3);
  rejected(Buffer.from(lines + '#\n'), 'runtime_svc_config_invalid');
});

test('512 keys is the inclusive parsing-work limit', () => {
  const text = base + Array.from({ length: 509 }, (_, i) => `extra_${i}=fixture`).join('\n');
  assert.equal(inspect(text).keyCount, 512);
  rejected(Buffer.from(`${text}\none_more=fixture`), 'runtime_svc_config_invalid');
});

test('full synthetic equivalent defaults accept the owned endpoint but actual defaults are refused', async () => {
  // This fixture is source-derived, not generated by a running mod or proof of
  // runtime parsing. Its comments and all other default fields remain intact.
  const original = await readFile(new URL('../desktop/src-tauri/tests/fixtures/connect_voice/svc-default-equivalent.properties', import.meta.url));
  rejected(original, 'runtime_svc_bind_invalid');
  const configured = Buffer.from(original.toString('utf8').replace('port=24454', 'port=43002')
    .replace('bind_address=', 'bind_address=127.0.0.1'));
  const report = validateSvcConfig(configured);
  assert.equal(report.keyCount, 19);
  assert.equal(report.sha256, sha256(configured));
});
