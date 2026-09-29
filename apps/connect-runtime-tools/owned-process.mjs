import { spawn } from 'node:child_process';
import { isAbsolute } from 'node:path';
import { performance } from 'node:perf_hooks';
import { resolveExplicitLocalPath } from '../connect-voice-tools/local-path.mjs';

const JAVA_INJECTION_VARIABLES = new Set([
  'JAVA_TOOL_OPTIONS', '_JAVA_OPTIONS', 'JDK_JAVA_OPTIONS', 'CLASSPATH',
]);
const OPTION_NAMES = new Set([
  'executable', 'args', 'cwd', 'env', 'signal', 'timeoutMs', 'stopTimeoutMs', 'outputLimitBytes',
]);
const MAX_ARGUMENT_BYTES = 64 * 1024;

// No child output, executable, arguments, environment, original exception or PID
// is included in this error. A process summary exists only after close/reaping.
export class OwnedProcessError extends Error {
  constructor(code, processSummary) {
    super(code);
    this.name = 'OwnedProcessError';
    this.code = code;
    if (processSummary) this.process = Object.freeze(processSummary);
  }
}

function invalid() { throw new OwnedProcessError('owned_process_invalid_options'); }
function boundedInteger(value, maximum) {
  return Number.isSafeInteger(value) && value >= 1 && value <= maximum;
}
function explicitPath(value) {
  if (typeof value !== 'string' || !isAbsolute(value)) return false;
  resolveExplicitLocalPath(value); // Lexical only: no UNC/network filesystem IO.
  return true;
}
function validate(options, operation) {
  if (!options || typeof options !== 'object' || Array.isArray(options)
    || Object.keys(options).some(key => !OPTION_NAMES.has(key)) || typeof operation !== 'function') invalid();
  const { executable, args, cwd, signal } = options;
  const timeoutMs = options.timeoutMs ?? 60_000;
  const stopTimeoutMs = options.stopTimeoutMs ?? 5_000;
  const outputLimitBytes = options.outputLimitBytes ?? 1024 * 1024;
  if (!explicitPath(executable) || !explicitPath(cwd) || !Array.isArray(args) || args.length > 256
    || args.some(arg => typeof arg !== 'string' || arg.includes('\0'))
    || args.reduce((sum, arg) => sum + Buffer.byteLength(arg), 0) > MAX_ARGUMENT_BYTES
    || !boundedInteger(timeoutMs, 3_600_000) || !boundedInteger(stopTimeoutMs, 5_000)
    || !boundedInteger(outputLimitBytes, 16 * 1024 * 1024)
    || (signal !== undefined && !(signal instanceof AbortSignal))) invalid();
  if (options.env !== undefined && (!options.env || typeof options.env !== 'object' || Array.isArray(options.env))) invalid();
  const overrides = Object.entries(options.env ?? {});
  if (overrides.some(([key, value]) => !/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)
    || typeof value !== 'string' || value.includes('\0'))) invalid();
  // Supplying env replaces inheritance, like spawn, so a caller can supply a
  // minimal environment without account secrets. Omitted env copies process.env.
  // Strip every casing (Windows names are case insensitive), without mutation.
  const env = options.env === undefined ? { ...process.env } : Object.fromEntries(overrides);
  for (const key of Object.keys(env)) {
    if (JAVA_INJECTION_VARIABLES.has(key.toUpperCase())) delete env[key];
  }
  return { executable, args: [...args], cwd, env, signal, timeoutMs, stopTimeoutMs, outputLimitBytes };
}

/**
 * Runs one explicitly selected local child while operation({signal,
 * ownsProcessId}) executes. The predicate checks an observed PID against this
 * still-running child; it never returns a PID or exposes a process handle.
 * This is lifecycle supervision, not artifact/EULA approval or application
 * readiness. The caller must check those gates and perform its own readiness
 * probe. No process is started on import, and no child handle/PID is exposed.
 * Explicit env replaces inheritance; when omitted process.env is copied. Both
 * forms strip the four Java argument/classpath injection variables above.
 * Local-path validation is lexical; file identity, links and actual artifact
 * provenance remain the caller's responsibility, checked before this helper.
 *
 * Success requires both the operation and an exit code of zero after `stop\n`.
 * Every failure stays a failure even if cleanup subsequently exits normally.
 * The global deadline covers startup, operation and graceful shutdown. On
 * deadline the owned ChildProcess is killed immediately, then close is awaited.
 * OS reaping can finish after the deadline; close is never falsely reported.
 * There is no tree/PID search, termination of descendants or foreign processes.
 *
 * A callback must honor its AbortSignal. Arbitrary JavaScript cannot be forcibly
 * cancelled: a callback which ignores it may continue its own side effects, but
 * its eventual fulfillment/rejection is observed and can never produce success
 * after a supervisor failure. It must not spawn untracked work or descendants.
 */
export async function withOwnedProcess(options, operation) {
  let config;
  try { config = validate(options, operation); }
  catch { throw new OwnedProcessError('owned_process_invalid_options'); }
  if (config.signal?.aborted) throw new OwnedProcessError('owned_process_cancelled');

  const controller = new AbortController();
  const startedAt = performance.now();
  let failureCode;
  let resolveFailure;
  const failure = new Promise(resolve => { resolveFailure = resolve; });
  const fail = code => {
    if (!failureCode) {
      failureCode = code;
      controller.abort();
      resolveFailure();
    }
  };
  let child;
  try {
    child = spawn(config.executable, config.args, {
      cwd: config.cwd, env: config.env, shell: false, windowsHide: true,
      detached: false, stdio: ['pipe', 'pipe', 'pipe'],
    });
  } catch {
    throw new OwnedProcessError('owned_process_spawn_failed');
  }

  let spawned = false;
  let closing = false;
  let closed = false;
  let forced = false;
  let stdoutBytes = 0;
  let stderrBytes = 0;
  let exitCode = null;
  let exitSignal = null;
  let resolveSpawn;
  const didSpawn = new Promise(resolve => { resolveSpawn = resolve; });
  let resolveClose;
  const didClose = new Promise(resolve => { resolveClose = resolve; });
  child.once('spawn', () => { spawned = true; resolveSpawn(); });
  child.on('error', () => fail(spawned ? 'owned_process_io_failed' : 'owned_process_spawn_failed'));
  child.once('exit', (code, signal) => {
    exitCode = code; exitSignal = signal;
    if (!closing) fail(code === 0 ? 'owned_process_child_exited' : 'owned_process_child_exit_nonzero');
  });
  child.once('close', (code, signal) => {
    exitCode = code; exitSignal = signal; closed = true; resolveClose();
  });
  const count = (stream, bytes) => {
    if (stream === 'stdout') stdoutBytes = Math.min(Number.MAX_SAFE_INTEGER, stdoutBytes + bytes.length);
    else stderrBytes = Math.min(Number.MAX_SAFE_INTEGER, stderrBytes + bytes.length);
    if (stdoutBytes + stderrBytes > config.outputLimitBytes) fail('owned_process_output_limit');
  };
  // Flowing streams are always drained, including while stopping after a flood.
  // No content is decoded, accumulated, returned or written to a log.
  child.stdout.on('data', bytes => count('stdout', bytes));
  child.stderr.on('data', bytes => count('stderr', bytes));
  child.stdout.on('error', () => fail('owned_process_io_failed'));
  child.stderr.on('error', () => fail('owned_process_io_failed'));
  // EPIPE can race a normal child exit. It is observed, never an unhandled error;
  // the final exit/deadline decides success. A still-alive child gets bounded
  // grace and then forced termination if it cannot receive the stop command.
  child.stdin.on('error', () => {});

  const onAbort = () => fail('owned_process_cancelled');
  config.signal?.addEventListener('abort', onAbort, { once: true });
  if (config.signal?.aborted) onAbort();
  let resolveDeadline;
  const expired = new Promise(resolve => { resolveDeadline = resolve; });
  const deadline = setTimeout(() => { fail('owned_process_deadline'); resolveDeadline(); },
    Math.max(1, config.timeoutMs - (performance.now() - startedAt)));
  let value;
  let graceTimer;
  const ownsProcessId = candidate => !controller.signal.aborted && spawned && !closed
    && child.exitCode === null && child.signalCode === null
    && Number.isSafeInteger(candidate) && candidate > 0 && candidate === child.pid;
  try {
    await Promise.race([didSpawn, failure]);
    if (!failureCode) {
      const work = Promise.resolve().then(() => {
        if (!failureCode) return operation({ signal: controller.signal, ownsProcessId });
      }).then(
        result => { value = result; },
        () => fail('owned_process_operation_failed'),
      );
      // Both fulfillment and rejection are observed even for an uncooperative
      // late callback. We never use a later callback value after failureCode.
      await Promise.race([work, failure]);
    }
  } finally {
    closing = true;
    controller.abort();
    // Cancellation may arrive synchronously after spawn() returns but before
    // its 'spawn' event. Still own/reap that process: wait for spawn or failed
    // spawn's close before deciding whether stop/kill is necessary.
    if (!spawned && !closed) await Promise.race([didSpawn, didClose]);
    if (!closed && spawned) {
      try { child.stdin.end('stop\n'); } catch { /* bounded stop/reap below */ }
      const remaining = config.timeoutMs - (performance.now() - startedAt);
      if (remaining > 0 && failureCode !== 'owned_process_deadline') {
        const grace = new Promise(resolve => { graceTimer = setTimeout(resolve, config.stopTimeoutMs); });
        // A failure already in progress does not suppress graceful cleanup.
        // The global deadline still bounds grace even when the operation failed.
        await Promise.race([didClose, grace, expired]);
      }
      clearTimeout(graceTimer);
      if (!closed) {
        forced = true;
        fail('owned_process_cleanup_forced');
        // Only this ChildProcess object: no numeric PID, taskkill, tree walk or
        // executable-name search. Await close even if exit races this call.
        try { child.kill('SIGKILL'); } catch { fail('owned_process_cleanup_failed'); }
      }
    }
    await didClose;
    clearTimeout(deadline);
    config.signal?.removeEventListener('abort', onAbort);
  }
  const summary = { spawned, closed, exitCode, signal: exitSignal, forced, stdoutBytes, stderrBytes };
  if (failureCode) throw new OwnedProcessError(failureCode, summary);
  if (exitCode !== 0 || exitSignal !== null) throw new OwnedProcessError('owned_process_child_exit_nonzero', summary);
  return { value, process: Object.freeze(summary) };
}
