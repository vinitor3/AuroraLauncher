import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync, spawn } from 'node:child_process';
import { generateKeyPairSync } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const binary = resolve(repo, `apps/connect-lab/target/debug/aurora-connect-lab${process.platform === 'win32' ? '.exe' : ''}`);
const host = generateKeyPairSync('ed25519').publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('hex');
const args = ['guest-invite', host, '127.0.0.1:0', '127.0.0.1:0', '30'];
const environment = { SystemRoot: process.env.SystemRoot, windir: process.env.windir,
  TEMP: process.env.TEMP, TMP: process.env.TMP };

for (const [label, input] of [['invalid signed envelope', '{}\n'], ['oversized input', `${'x'.repeat(5000)}\n`],
  ['EOF before invitation', ''], ['partial line at EOF', '{}'], ['trailing control', '{}\0\n']]) {
  test(`invitation CLI rejects ${label} before admitting or opening a bridge`, () => {
    const result = spawnSync(binary, args, { cwd: repo, env: environment, input, windowsHide: true,
      shell: false, encoding: 'utf8', maxBuffer: 16384, timeout: 5000 });
    assert.equal(result.error, undefined); assert.equal(result.status, 1); assert.equal(result.signal, null);
    const events = result.stdout.trim().split('\n').map(line => JSON.parse(line));
    assert.deepEqual(events.map(e => e.event), ['guest_identity', 'failed']);
    assert.ok(['invalid_protocol', 'unauthorized'].includes(events[1].code));
    assert.equal(result.stderr, '');
    assert.doesNotMatch(events[1].code, /PRIVATE|127|5000/);
  });
}

test('invitation CLI refuses external listeners and lifetime over ten minutes before identity', () => {
  for (const params of [['guest-invite', host, '0.0.0.0:0', '127.0.0.1:0', '30'],
    ['guest-invite-tcp', host, '127.0.0.1:0', '601']]) {
    const result = spawnSync(binary, params, { cwd: repo, env: environment, windowsHide: true,
      shell: false, encoding: 'utf8', maxBuffer: 16384, timeout: 5000 });
    assert.equal(result.error, undefined); assert.equal(result.status, 1);
    assert.equal(JSON.parse(result.stdout).event, 'failed'); assert.equal(result.stderr, '');
  }
});

// A held stdin must not become a bridge just because the process is waiting.
// EOF is a normal way to cancel a pending prompt, without killing the process.
for (const mode of ['eof', 'deadline']) test(`pending invitation ${mode} closes its own process without a bridge`, async () => {
  const child = spawn(binary, args, { cwd: repo, env: environment, windowsHide: true, shell: false,
    stdio: ['pipe', 'pipe', 'pipe'] });
  let output = ''; let errorOutput = ''; let forced = false;
  const timer = setTimeout(() => { forced = true; child.kill(); }, mode === 'deadline' ? 65000 : 5000);
  child.stdout.setEncoding('utf8'); child.stderr.setEncoding('utf8');
  child.stdout.on('data', bytes => {
    output += bytes;
    if (output.length > 16384) { forced = true; child.kill(); return; }
    if (mode === 'eof' && output.includes('\n') && !child.stdin.writableEnded) child.stdin.end();
  });
  child.stderr.on('data', bytes => {
    errorOutput += bytes;
    if (errorOutput.length > 16384) { forced = true; child.kill(); }
  });
  child.stdin.on('error', () => {});
  try {
    const [code, signal] = await new Promise((resolve, reject) => {
      child.once('close', (code, signal) => resolve([code, signal])); child.once('error', reject);
    });
    assert.equal(forced, false); assert.equal(code, 1); assert.equal(signal, null); assert.equal(errorOutput, '');
    assert.deepEqual(output.trim().split('\n').map(line => JSON.parse(line).event), ['guest_identity', 'failed']);
    if (mode === 'deadline') assert.equal(JSON.parse(output.trim().split('\n')[1]).code, 'timed_out');
  } finally { clearTimeout(timer); if (child.exitCode === null && child.signalCode === null) child.kill(); }
});
