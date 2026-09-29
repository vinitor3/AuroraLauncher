import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:net';
import { spawn } from 'node:child_process';
import { access } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const script = join(repo, 'scripts/connect-minecraft-status-smoke.ps1');
function vi(number) {
  const result = [];
  do { const byte = number & 127; number >>>= 7; result.push(byte | (number ? 128 : 0)); } while (number);
  return Buffer.from(result);
}
const frame = body => Buffer.concat([vi(body.length), body]);
function int(bytes, start = 0) {
  let value = 0;
  for (let i = 0; i < 5; i++) {
    if (start + i >= bytes.length) return null;
    const byte = bytes[start + i]; value |= (byte & 127) << (7 * i);
    if (byte < 128) return { value, size: i + 1 };
  }
  throw new Error('fixture_invalid_varint');
}
async function fixture(t, protocol) {
  const sockets = new Set(); const state = { accepts: 0, status: 0, pongs: 0, failure: undefined };
  const server = createServer(socket => {
    state.accepts++; sockets.add(socket); socket.on('close', () => sockets.delete(socket));
    socket.on('error', () => {});
    let bytes = Buffer.alloc(0); let phase = 0;
    socket.on('data', data => {
      try {
        bytes = Buffer.concat([bytes, data]); assert.ok(bytes.length <= 8192);
        for (;;) {
          const length = int(bytes); if (!length) break;
          assert.ok(length.value >= 0 && length.value <= 2048);
          if (bytes.length < length.size + length.value) break;
          const body = bytes.subarray(length.size, length.size + length.value);
          bytes = bytes.subarray(length.size + length.value);
          if (phase === 0) {
            assert.equal(body[0], 0); const requested = int(body, 1); assert.equal(requested.value, 767);
            const hostLength = int(body, 1 + requested.size);
            const start = 1 + requested.size + hostLength.size;
            assert.equal(body.subarray(start, start + hostLength.value).toString(), 'localhost');
            assert.equal(body.readUInt16BE(start + hostLength.value), server.address().port);
            assert.equal(body.at(-1), 1); assert.equal(body.length, start + hostLength.value + 3);
          } else if (phase === 1) {
            assert.deepEqual(body, Buffer.from([0])); state.status++;
            const json = Buffer.from(JSON.stringify({ version: { name: 'PRIVATE_FIXTURE', protocol },
              players: { online: 0, max: 2 }, description: { text: 'PRIVATE_FIXTURE_MOTD' } }));
            socket.write(frame(Buffer.concat([Buffer.from([0]), vi(json.length), json])));
          } else if (phase === 2) {
            assert.equal(body.length, 9); assert.equal(body[0], 1); state.pongs++; socket.write(frame(body));
          } else { throw new Error('unexpected_fixture_request'); }
          phase++;
        }
      } catch (error) { state.failure = error; socket.destroy(); }
    });
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(async () => {
    for (const socket of sockets) socket.destroy();
    await new Promise(resolve => server.close(resolve));
  });
  return { port: server.address().port, state, sockets };
}
async function run(t, port) {
  await access(join(repo, 'apps/connect-lab/target/debug/aurora-connect-lab.exe'));
  const powershell = join(process.env.SystemRoot, 'System32/WindowsPowerShell/v1.0/powershell.exe');
  const child = spawn(powershell, ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', script,
    '-GamePort', String(port), '-SkipBuild'], { cwd: repo, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  const closed = new Promise((resolve, reject) => {
    let stdout = ''; let stderr = '';
    child.stdout.on('data', data => { stdout += data; if (stdout.length > 65536) child.kill(); });
    child.stderr.on('data', data => { stderr += data; if (stderr.length > 65536) child.kill(); });
    child.on('error', reject); child.on('close', code => resolve({ code, stdout, stderr }));
  });
  // The script also bounds/reaps its own agents. This test owns only PowerShell;
  // its emergency timeout is a harness failure, never a cleanup PASS.
  const timer = setTimeout(() => child.kill(), 75000);
  t.after(async () => { clearTimeout(timer); if (child.exitCode === null) child.kill(); await closed; });
  const result = await closed; clearTimeout(timer); return result;
}
test('two real CLI agents carry STATUS with only game/tcp and cleanly exit', { skip: process.platform !== 'win32' }, async t => {
  const f = await fixture(t, 767); const result = await run(t, f.port);
  assert.equal(f.state.failure, undefined);
  const failureCode = result.stderr.match(/^[a-z_]+/)?.[0] ?? 'unknown';
  assert.equal(result.code, 0, `PowerShell smoke: ${failureCode}`);
  assert.equal(result.stderr, '');
  const summary = JSON.parse(result.stdout);
  assert.equal(summary.result, 'PASS'); assert.equal(summary.agents, 2);
  assert.deepEqual(summary.applicationServices, ['game/tcp']);
  assert.equal(summary.cleanup, 'owned_agents_exited'); assert.equal(summary.relay, 'disabled');
  assert.equal(summary.runtimeHomologated, false); assert.equal(summary.worldJoined, false);
  assert.equal(summary.status.reportedProtocol, 767); assert.equal(summary.status.protocolMatchesRequested, true);
  assert.ok(!result.stdout.includes('PRIVATE_FIXTURE')); assert.ok(!result.stdout.includes('127.0.0.1'));
  assert.equal(f.state.failure, undefined); assert.equal(f.state.accepts, 1);
  assert.equal(f.state.status, 1); assert.equal(f.state.pongs, 1); assert.equal(f.sockets.size, 0);
});
test('two-agent smoke refuses mismatching game protocol after STATUS and pong', { skip: process.platform !== 'win32' }, async t => {
  const f = await fixture(t, 766); const result = await run(t, f.port);
  assert.equal(f.state.failure, undefined);
  assert.equal(result.code, 1); assert.ok(!result.stdout.includes('PASS'));
  assert.ok(result.stderr.includes('minecraft_status_failed_or_protocol_mismatch'));
  assert.ok(!result.stderr.includes('PRIVATE_FIXTURE'));
  assert.equal(f.state.failure, undefined); assert.equal(f.state.accepts, 1);
  assert.equal(f.state.status, 1); assert.equal(f.state.pongs, 1); assert.equal(f.sockets.size, 0);
});
