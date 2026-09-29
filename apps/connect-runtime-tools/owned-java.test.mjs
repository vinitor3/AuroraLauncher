import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { makePlan } from './runtime.mjs';
import { withOwnedProcess } from './owned-process.mjs';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const selectedHome = process.env.AURORA_CONNECT_JAVA21_HOME;
async function observe(expected, signal) {
  // Exact, read-only production probe; never run other opt-in desktop tests.
  const args = ['test', '--offline', '--locked', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml',
    '--features', 'connect-voice-lab', '--lib',
    'engine::connect_voice::process::windows::tests::production_java_process_probe_reports_only_status',
    '--', '--exact', '--ignored', '--nocapture'];
  const output = await new Promise((resolve, reject) => {
    execFile('cargo', args, { cwd: repo, windowsHide: true, timeout: 30000, maxBuffer: 65536, signal,
      env: { ...process.env, AURORA_CONNECT_PROCESS_PROBE: '1' } }, (error, stdout) => {
      if (error) reject(new Error('production_process_probe_command_failed'));
      else resolve(stdout);
    });
  });
  const statuses = [...output.matchAll(/^voice_process_probe=(STOPPED|RUNNING)\r?$/gm)].map(match => match[1]);
  assert.deepEqual(statuses, [expected], 'production probe must observe expected status');
  assert.match(output, /1 passed; 0 failed/);
}

test('real Java 21 owned fixture is observed RUNNING and STOPPED by the production voice gate', {
  skip: process.platform !== 'win32' || !selectedHome,
}, async t => {
  // This runs only our small source program in an explicitly selected installed
  // JDK. No Minecraft, mod, EULA, network endpoint or personal instance is used.
  const plan = await makePlan(selectedHome);
  await observe('STOPPED'); // A Java process owned by anyone else must not be killed.
  const root = await mkdtemp(join(tmpdir(), 'aurora-owned-java-'));
  t.after(async () => {
    assert.ok(isAbsolute(root) && root.startsWith(join(tmpdir(), 'aurora-owned-java-')));
    await rm(root, { recursive: true, force: true });
  });
  const source = join(root, 'AuroraConnectProbe.java');
  await writeFile(source, [
    'import java.io.BufferedReader;', 'import java.io.InputStreamReader;',
    'class AuroraConnectProbe {', '  public static void main(String[] args) throws Exception {',
    '    if (Runtime.version().feature() != 21) System.exit(2);',
    '    System.out.println("OWNED_JAVA_READY");',
    '    String line = new BufferedReader(new InputStreamReader(System.in)).readLine();',
    '    if (!"stop".equals(line)) System.exit(3);', '  }', '}', '',
  ].join('\n'), { flag: 'wx' });
  const result = await withOwnedProcess({
    executable: plan.java.executable,
    args: ['-Xms16M', '-Xmx128M', '-XX:-UsePerfData', `-Duser.home=${root}`, '--source', '21', source],
    cwd: root,
    // Minimal JVM environment: no tokens, account settings or unrelated secrets.
    env: { SystemRoot: process.env.SystemRoot, windir: process.env.SystemRoot,
      TEMP: root, TMP: root, USERPROFILE: root },
    timeoutMs: 40000, stopTimeoutMs: 5000, outputLimitBytes: 4096,
  }, async ({ signal }) => { await observe('RUNNING', signal); return 'production_gate_refuses_standard_java'; });
  assert.equal(result.value, 'production_gate_refuses_standard_java');
  assert.equal(result.process.exitCode, 0); assert.equal(result.process.forced, false);
  assert.ok(result.process.stdoutBytes >= 'OWNED_JAVA_READY\n'.length);
  await observe('STOPPED');
  t.diagnostic('owned Java 21 only: STOPPED -> RUNNING -> STOPPED; normal stop; no Minecraft or mod');
});
