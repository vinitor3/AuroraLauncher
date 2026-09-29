import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { isAbsolute, join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { OwnedProcessError, withOwnedProcess } from './owned-process.mjs';

// Every child in this file is created by the supervisor and runs this host's
// Node executable with synthetic source. No Java, Minecraft, network, EULA or
// subprocess tree is involved. Temporary files are private test rendezvous.
async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'aurora-owned-process-'));
  t.after(async () => {
    assert.ok(isAbsolute(root) && root.startsWith(join(tmpdir(), 'aurora-owned-process-')));
    await rm(root, { recursive: true, force: true });
  });
  return root;
}
async function exists(file) { try { await stat(file); return true; } catch (e) { if (e.code === 'ENOENT') return false; throw e; } }
async function waitFile(file, signal) {
  const deadline = Date.now() + 3000;
  while (!await exists(file)) {
    signal?.throwIfAborted();
    if (Date.now() >= deadline) throw new Error('fixture_ready_deadline');
    await delay(5, undefined, { signal });
  }
}
function configuration(root, source, extra = {}) {
  return {
    executable: process.execPath, args: ['-e', source, root], cwd: root,
    timeoutMs: 5000, stopTimeoutMs: 500, outputLimitBytes: 64 * 1024, ...extra,
  };
}
const prelude = `const fs = require('node:fs'); const path = require('node:path');
const root = process.argv[1]; const file = name => path.join(root, name);`;
const graceful = `${prelude}
process.stdout.write('private-out'); process.stderr.write('private-err');
fs.writeFileSync(file('ready'), 'ready');
let input = ''; process.stdin.setEncoding('utf8');
process.stdin.on('data', chunk => {
  input += chunk;
  if (input.length > 1024) process.exit(7);
  if (input.includes('stop\\n')) {
    fs.writeFileSync(file('stopped'), 'stop'); process.exit(0);
  }
});`;
const silent = `${prelude}
fs.writeFileSync(file('ready'), 'ready');
process.stdin.resume(); setInterval(() => {}, 1000);`;
const abortableWork = signal => new Promise(resolve => {
  if (signal.aborted) resolve();
  else signal.addEventListener('abort', resolve, { once: true });
});
function assertReaped(error, code, forced) {
  assert.ok(error instanceof OwnedProcessError);
  assert.equal(error.name, 'OwnedProcessError'); assert.equal(error.code, code);
  assert.equal(error.message, code); assert.equal(error.cause, undefined);
  assert.equal(error.process.closed, true); assert.equal(error.process.forced, forced);
  assert.ok(Object.isFrozen(error.process));
  assert.ok(!JSON.stringify(error).includes('PRIVATE'));
  return true;
}

test('import does not execute a child and invalid options never invoke work', async t => {
  const root = await fixture(t); let calls = 0;
  const base = configuration(root, graceful);
  for (const change of [
    { executable: 'node' }, { cwd: '.' }, { args: '-e code' }, { args: ['a\0b'] },
    { args: Array(257).fill('x') }, { args: ['x'.repeat(65537)] }, { timeoutMs: 0 },
    { timeoutMs: 3600001 }, { stopTimeoutMs: 5001 }, { outputLimitBytes: 0 },
    { outputLimitBytes: 16 * 1024 * 1024 + 1 }, { shell: true }, { env: { A: 'b\0c' } },
    { executable: '\\\\server\\share\\node.exe' }, { cwd: '\\\\?\\C:\\temp' },
    { executable: 'C:\\node.exe:stream' }, { cwd: '\\\\.\\C:\\temp' },
  ]) {
    await assert.rejects(withOwnedProcess({ ...base, ...change }, () => { calls++; }), error => {
      assert.equal(error.code, 'owned_process_invalid_options'); assert.equal(error.process, undefined); return true;
    });
  }
  assert.equal(calls, 0); assert.equal(await exists(join(root, 'ready')), false);
  // Importing a module with no CLI entrypoint has no invocation or side effects.
  const imported = await import('./owned-process.mjs');
  assert.equal(imported.withOwnedProcess, withOwnedProcess);
  assert.equal(await exists(join(root, 'ready')), false);
});

test('operation plus graceful stop returns only value, exit and output counts', async t => {
  const root = await fixture(t); let workSignal;
  const result = await withOwnedProcess(configuration(root, graceful, { outputLimitBytes: 22 }), async ({ signal }) => {
    workSignal = signal; await waitFile(join(root, 'ready'), signal);
    assert.equal(signal.aborted, false); return { evidence: 'synthetic_only' };
  });
  assert.deepEqual(result.value, { evidence: 'synthetic_only' });
  assert.deepEqual(result.process, { spawned: true, closed: true, exitCode: 0, signal: null,
    forced: false, stdoutBytes: 11, stderrBytes: 11 });
  assert.equal(await readFile(join(root, 'stopped'), 'utf8'), 'stop');
  assert.equal(workSignal.aborted, true);
  assert.ok(Object.isFrozen(result.process)); assert.ok(!JSON.stringify(result).includes('private-'));
});

test('arguments remain an array, cwd is explicit, and Java injection environment is removed only in child', async t => {
  const root = await fixture(t);
  const names = ['JAVA_TOOL_OPTIONS', '_JAVA_OPTIONS', 'JDK_JAVA_OPTIONS', 'CLASSPATH'];
  const sentinel = 'AURORA_OWNED_PARENT_SECRET';
  const previous = new Map([...names, sentinel].map(name => [name, process.env[name]]));
  for (const name of names) process.env[name] = 'PRIVATE_PARENT_' + name;
  process.env[sentinel] = 'PRIVATE_PARENT_SENTINEL';
  t.after(() => {
    for (const [name, value] of previous) {
      if (value === undefined) delete process.env[name]; else process.env[name] = value;
    }
  });
  const literal = 'literal with spaces & | < > " $ ;';
  const source = graceful.replace("fs.writeFileSync(file('ready'), 'ready');", `
fs.writeFileSync(file('state.json'), JSON.stringify({
  cwd: process.cwd(), arg: process.argv[2], keep: process.env.AURORA_TEST_KEEP,
  sentinel: process.env.AURORA_OWNED_PARENT_SECRET ?? null,
  injected: Object.keys(process.env).filter(key => ${JSON.stringify(names)}.includes(key.toUpperCase()))
})); fs.writeFileSync(file('ready'), 'ready');`);
  const result = await withOwnedProcess(configuration(root, source, {
    args: ['-e', source, root, literal],
    env: { java_tool_options: 'PRIVATE_OVERRIDE', ClassPath: 'PRIVATE_OVERRIDE', AURORA_TEST_KEEP: 'kept' },
  }), async ({ signal }) => { await waitFile(join(root, 'ready'), signal); });
  assert.equal(result.process.exitCode, 0);
  const state = JSON.parse(await readFile(join(root, 'state.json'), 'utf8'));
  assert.deepEqual(state, { cwd: root, arg: literal, keep: 'kept', sentinel: null, injected: [] });
  for (const name of names) assert.equal(process.env[name], 'PRIVATE_PARENT_' + name);
  assert.equal(process.env[sentinel], 'PRIVATE_PARENT_SENTINEL');
  const inheritedRoot = await fixture(t);
  await withOwnedProcess(configuration(inheritedRoot, source, { args: ['-e', source, inheritedRoot, literal] }),
    async ({ signal }) => { await waitFile(join(inheritedRoot, 'ready'), signal); });
  const inherited = JSON.parse(await readFile(join(inheritedRoot, 'state.json'), 'utf8'));
  assert.equal(inherited.sentinel, 'PRIVATE_PARENT_SENTINEL'); assert.deepEqual(inherited.injected, []);
});

test('spawn and invalid working-directory failures are redacted and close before returning', async t => {
  const root = await fixture(t); let calls = 0;
  for (const change of [{ executable: join(root, 'PRIVATE-missing.exe') }, { cwd: join(root, 'PRIVATE-missing') }]) {
    await assert.rejects(withOwnedProcess(configuration(root, graceful, change), () => { calls++; }), error => {
      assertReaped(error, 'owned_process_spawn_failed', false);
      assert.equal(error.process.spawned, false); assert.ok(!error.message.includes(root)); return true;
    });
  }
  assert.equal(calls, 0);
});

test('operation rejection stays redacted even after successful graceful cleanup', async t => {
  const root = await fixture(t); let workSignal;
  await assert.rejects(withOwnedProcess(configuration(root, graceful), async ({ signal }) => {
    workSignal = signal; await waitFile(join(root, 'ready'), signal);
    throw new Error('PRIVATE_OPERATION_EXCEPTION');
  }), error => {
    assertReaped(error, 'owned_process_operation_failed', false);
    assert.equal(error.process.exitCode, 0); return true;
  });
  assert.equal(workSignal.aborted, true); assert.equal(await exists(join(root, 'stopped')), true);
});

test('early zero exit and nonzero exit cannot turn pending operation into success', async t => {
  for (const code of [0, 17]) {
    const root = await fixture(t); let aborted = false;
    await assert.rejects(withOwnedProcess(configuration(root, `process.exit(${code});`), async ({ signal }) => {
      await abortableWork(signal); aborted = true; return 'LATE_SUCCESS';
    }), error => {
      assertReaped(error, code === 0 ? 'owned_process_child_exited' : 'owned_process_child_exit_nonzero', false);
      assert.equal(error.process.exitCode, code); return true;
    });
    assert.equal(aborted, true);
  }
});

test('nonzero exit during requested shutdown remains a failure', async t => {
  const root = await fixture(t);
  await assert.rejects(withOwnedProcess(configuration(root, graceful.replace('process.exit(0)', 'process.exit(19)')),
    async ({ signal }) => { await waitFile(join(root, 'ready'), signal); }), error => {
    assertReaped(error, 'owned_process_child_exit_nonzero', false);
    assert.equal(error.process.exitCode, 19); return true;
  });
});

test('successful operation with silent child forces cleanup and is never a normal PASS', async t => {
  const root = await fixture(t); const start = Date.now();
  await assert.rejects(withOwnedProcess(configuration(root, silent, { stopTimeoutMs: 80 }),
    async ({ signal }) => { await waitFile(join(root, 'ready'), signal); return 'LATE_SUCCESS'; }),
  error => assertReaped(error, 'owned_process_cleanup_forced', true));
  assert.ok(Date.now() - start < 3000);
});

test('global deadline aborts a pending operation, kills silent child, and awaits close', async t => {
  const root = await fixture(t); let workSignal; const start = Date.now();
  await assert.rejects(withOwnedProcess(configuration(root, silent, { timeoutMs: 200, stopTimeoutMs: 5000 }),
    async ({ signal }) => { workSignal = signal; await abortableWork(signal); return 'LATE_SUCCESS'; }),
  error => assertReaped(error, 'owned_process_deadline', true));
  assert.equal(workSignal.aborted, true); assert.ok(Date.now() - start < 3000);
});

test('global deadline also includes graceful stop after completed operation', async t => {
  const root = await fixture(t); const start = Date.now();
  await assert.rejects(withOwnedProcess(configuration(root, silent, { timeoutMs: 250, stopTimeoutMs: 5000 }),
    async ({ signal }) => { await waitFile(join(root, 'ready'), signal); }),
  error => assertReaped(error, 'owned_process_deadline', true));
  assert.ok(Date.now() - start < 3000);
});

test('pre-cancel starts no child; active cancellation still reaps after graceful cleanup', async t => {
  const root = await fixture(t); const before = new AbortController(); before.abort('PRIVATE_REASON'); let calls = 0;
  await assert.rejects(withOwnedProcess(configuration(root, graceful, { signal: before.signal }), () => { calls++; }),
    error => { assert.equal(error.code, 'owned_process_cancelled'); assert.equal(error.process, undefined); return true; });
  assert.equal(calls, 0); assert.equal(await exists(join(root, 'ready')), false);
  const active = new AbortController();
  await assert.rejects(withOwnedProcess(configuration(root, graceful, { signal: active.signal }), async ({ signal }) => {
    await waitFile(join(root, 'ready'), signal); active.abort('PRIVATE_REASON'); await abortableWork(signal);
  }), error => {
    assertReaped(error, 'owned_process_cancelled', false); assert.equal(error.process.exitCode, 0); return true;
  });
  assert.equal(await exists(join(root, 'stopped')), true);
});

test('synchronous cancellation immediately after spawn still stops and reaps the owned child', async t => {
  const root = await fixture(t); const controller = new AbortController(); let called = false;
  const running = withOwnedProcess(configuration(root, silent, {
    signal: controller.signal, stopTimeoutMs: 50,
  }), () => { called = true; return 'LATE_SUCCESS'; });
  controller.abort();
  await assert.rejects(running, error => {
    assertReaped(error, 'owned_process_cancelled', true);
    assert.equal(error.process.spawned, true); return true;
  });
  assert.equal(called, false);
});

test('stdout/stderr floods are drained without retention and terminate within a bound', async t => {
  for (const stream of ['stdout', 'stderr']) {
    const root = await fixture(t); const start = Date.now();
    const source = `${prelude} const chunk = Buffer.alloc(4096, 65);
      setInterval(() => { for (let n = 0; n < 16; n++) process.${stream}.write(chunk); }, 1);
      process.stdin.resume();`;
    await assert.rejects(withOwnedProcess(configuration(root, source, { outputLimitBytes: 8192, stopTimeoutMs: 50 }),
      ({ signal }) => abortableWork(signal)), error => {
      assertReaped(error, 'owned_process_output_limit', true);
      assert.ok(error.process[stream + 'Bytes'] > 8192);
      assert.ok(Object.keys(error.process).every(key => !['stdout', 'stderr', 'logs'].includes(key)));
      return true;
    });
    assert.ok(Date.now() - start < 3000);
  }
});

test('closed stdin/EPIPE is observed, then a live child is forcibly reaped', async t => {
  const root = await fixture(t);
  const source = `${prelude} fs.closeSync(0); fs.writeFileSync(file('ready'), 'ready'); setInterval(() => {}, 1000);`;
  await assert.rejects(withOwnedProcess(configuration(root, source, { stopTimeoutMs: 80 }),
    async ({ signal }) => { await waitFile(join(root, 'ready'), signal); }),
  error => assertReaped(error, 'owned_process_cleanup_forced', true));
});

test('stdout plus stderr share one byte limit even when each stream is individually below it', async t => {
  const root = await fixture(t);
  await assert.rejects(withOwnedProcess(configuration(root, graceful, { outputLimitBytes: 21 }),
    ({ signal }) => abortableWork(signal)), error => {
    assertReaped(error, 'owned_process_output_limit', false);
    assert.equal(error.process.stdoutBytes, 11); assert.equal(error.process.stderrBytes, 11);
    assert.equal(error.process.exitCode, 0); return true;
  });
});

test('uncooperative late operation never changes failure to PASS or causes unhandled rejection', async t => {
  for (const rejects of [false, true]) {
    const root = await fixture(t); let settle; let invoked;
    const called = new Promise(resolve => { invoked = resolve; });
    const operation = new Promise((resolve, reject) => { settle = rejects ? reject : resolve; });
    const running = withOwnedProcess(configuration(root, silent, { timeoutMs: 150 }), () => { invoked(); return operation; });
    const rejection = assert.rejects(running, error => assertReaped(error, 'owned_process_deadline', true));
    await called; await rejection;
    settle(rejects ? new Error('PRIVATE_LATE_FAILURE') : 'LATE_SUCCESS');
    await delay(10);
    await assert.rejects(running, error => error.code === 'owned_process_deadline');
  }
});

test('stopping one owned child leaves a concurrent separately owned child intact', async t => {
  const otherRoot = await fixture(t); const failedRoot = await fixture(t);
  let release; const hold = new Promise(resolve => { release = resolve; });
  let otherReady; const ready = new Promise(resolve => { otherReady = resolve; });
  let otherSettled = false;
  const other = withOwnedProcess(configuration(otherRoot, graceful), async ({ signal }) => {
    await waitFile(join(otherRoot, 'ready'), signal); otherReady(); await hold; return 'still_owned';
  });
  // Observe both paths immediately so a test assertion cannot leak a rejection.
  const observed = other.then(result => { otherSettled = true; return result; }, error => { otherSettled = true; throw error; });
  t.after(async () => { release(); await observed; });
  await ready;
  await assert.rejects(withOwnedProcess(configuration(failedRoot, silent, { stopTimeoutMs: 50 }),
    async ({ signal }) => { await waitFile(join(failedRoot, 'ready'), signal); }),
  error => assertReaped(error, 'owned_process_cleanup_forced', true));
  assert.equal(otherSettled, false); assert.equal(await exists(join(otherRoot, 'stopped')), false);
  release(); const result = await observed;
  assert.equal(result.value, 'still_owned'); assert.equal(result.process.forced, false);
  assert.equal(await exists(join(otherRoot, 'stopped')), true);
});

test('import in a supervised Node fixture has no implicit execution', async t => {
  const root = await fixture(t);
  const modulePath = new URL('./owned-process.mjs', import.meta.url).href;
  const source = `${prelude}
    import(${JSON.stringify(modulePath)}).then(module => {
      if (Object.keys(module).sort().join(',') !== 'OwnedProcessError,withOwnedProcess') process.exit(4);
      fs.writeFileSync(file('ready'), 'ready');
      process.stdin.on('data', bytes => { if (bytes.toString() === 'stop\\n') process.exit(0); });
    });`;
  const result = await withOwnedProcess(configuration(root, source), async ({ signal }) => {
    await waitFile(join(root, 'ready'), signal);
  });
  assert.equal(result.process.stdoutBytes, 0); assert.equal(result.process.stderrBytes, 0);
  assert.equal(result.process.exitCode, 0);
});

test('ownership predicate accepts only the live owned PID and expires on shutdown', async t => {
  const root = await fixture(t);
  const source = graceful.replace("fs.writeFileSync(file('ready'), 'ready');",
    "fs.writeFileSync(file('pid'), String(process.pid)); fs.writeFileSync(file('ready'), 'ready');");
  let owns; let pid;
  await withOwnedProcess(configuration(root, source), async ({ signal, ownsProcessId }) => {
    await waitFile(join(root, 'ready'), signal);
    owns = ownsProcessId; pid = Number(await readFile(join(root, 'pid'), 'utf8'));
    assert.equal(owns(pid), true);
    for (const other of [process.pid, -1, 0, 1.5, NaN, Infinity, String(pid), null, undefined]) {
      assert.equal(owns(other), false);
    }
  });
  assert.equal(owns(pid), false);
});
