// Opt-in diagnostic: the production SDK and two compiled-online ConnectManagers.
// No credentials, authority origin or peer keys are accepted in argv or written.
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { readFile, realpath, lstat, writeFile, mkdir } from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import { dirname, resolve, relative, isAbsolute } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawn } from 'node:child_process';
import { StringDecoder } from 'node:string_decoder';
import { createConnection, createServer } from 'node:net';

const root = fileURLToPath(new URL('../', import.meta.url));
const desktopRequire = createRequire(new URL('../apps/desktop/package.json', import.meta.url));
const testName = 'engine::connect::live::social::tests::social_desktop_process';
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
export class SmokeError extends Error { constructor(code) { super(code); this.code = code; } }
const fail = code => { throw new SmokeError(code); };
export const redactedError = error => error instanceof SmokeError ? error.code
  : typeof error?.code === 'string' && /^owned_cleanup_[a-z_0-9]{1,64}$/.test(error.code) ? error.code : 'SMOKE_OPERATION_FAILED';
export async function bounded(work, milliseconds, code) {
  let timer;
  try { return await Promise.race([work, new Promise((_, reject) => { timer = setTimeout(() => reject(new SmokeError(code)), milliseconds); })]); }
  finally { clearTimeout(timer); }
}
export function childEnvironment(environment = process.env) {
  const allowed = new Set(['systemroot', 'systemdrive', 'windir', 'path', 'pathext', 'temp', 'tmp', 'userprofile', 'localappdata', 'appdata', 'number_of_processors', 'processor_architecture']);
  return { ...Object.fromEntries(Object.entries(environment).filter(([key]) => allowed.has(key.toLowerCase()))),
    AURORA_SOCIAL_DESKTOP_FIXTURE: '1', AURORA_SOCIAL_DESKTOP_ONLINE_FIXTURE: '1' };
}
export function parseArguments(args) {
  if (args.length !== 5 || !['--check', '--run'].includes(args[0]) || args[1] !== '--binary' || args[3] !== '--sha256'
    || !isAbsolute(args[2]) || !/^[a-f0-9]{64}$/.test(args[4])) fail('ARGUMENTS_INVALID');
  return { run: args[0] === '--run', binary: args[2], sha256: args[4] };
}
export function checkEnvironment(environment = process.env) {
  if (environment.NODE_TLS_REJECT_UNAUTHORIZED === '0') fail('TLS_VERIFICATION_REQUIRED');
  if (['FIREBASE_AUTH_EMULATOR_HOST', 'FIRESTORE_EMULATOR_HOST', 'FIREBASE_DATABASE_EMULATOR_HOST', 'CLOUD_FIRESTORE_EMULATOR_ADDRESS']
    .some(name => environment[name])) fail('EMULATOR_ENVIRONMENT_FORBIDDEN');
}
export function runOperator(environment = process.env) {
  const value = environment.AURORA_CONNECT_TEST_OPERATOR;
  if (typeof value !== 'string' || !/^[^\s@]{1,64}@[^\s@]{1,253}$/.test(value)) fail('OPERATOR_REQUIRED');
  return value;
}
export async function verifyBinary(binary, expected) {
  const directory = await realpath(resolve(root, 'apps/desktop/src-tauri/target/debug/deps'));
  const actual = await realpath(binary), within = relative(directory, actual);
  if (within.startsWith('..') || isAbsolute(within) || dirname(actual).toLowerCase() !== directory.toLowerCase()
    || !/^aurora_launcher_core-[a-f0-9]{16}\.exe$/.test(within) || (await lstat(binary)).isSymbolicLink()) fail('BINARY_SCOPE_INVALID');
  const hash = createHash('sha256'); for await (const chunk of createReadStream(actual)) hash.update(chunk);
  if (hash.digest('hex') !== expected) fail('BINARY_HASH_MISMATCH');
  return actual;
}
export async function loadProductionSdk() {
  const env = { MODE: 'connect-online', VITE_AURORA_SOCIAL_PRIVATE_ONLINE: '1',
    VITE_AURORA_SOCIAL_ORIGIN: 'https://aurora-connect-social.aurora-edge-proxy.workers.dev' };
  const ts = desktopRequire('typescript'), cache = new Map();
  const allowed = ['lib/socialFirebase.ts', 'lib/firebase.ts', 'lib/launcherAuthScope.ts', 'lib/socialServerClock.ts',
    'lib/socialOrganization.ts', 'lib/socialPrivateInvitations.ts', 'shared/social.ts'].map(name => resolve(root, 'apps/desktop/src', name));
  async function compile(path) {
    if (!allowed.includes(path)) fail('SDK_MODULE_SCOPE'); if (cache.has(path)) return cache.get(path);
    let source = ts.transpileModule((await readFile(path, 'utf8')).replaceAll('import.meta.env', JSON.stringify(env)),
      { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
    for (const match of [...source.matchAll(/from\s+["']([^"']+)["']/g)]) {
      const name = match[1];
      const destination = name.startsWith('.') ? await compile(resolve(dirname(path), name + '.ts'))
        : /^firebase\/(app|auth|firestore|storage)$/.test(name) ? pathToFileURL(desktopRequire.resolve(name)).href : fail('SDK_IMPORT_SCOPE');
      source = source.replace(match[0], 'from ' + JSON.stringify(destination));
    }
    const url = `data:text/javascript;base64,${Buffer.from(source).toString('base64')}`; cache.set(path, url); return url;
  }
  const factory = (await import(await compile(resolve(root, 'apps/desktop/src/lib/socialFirebase.ts')))).createSocialFirebaseService;
  const configured = await import(await compile(resolve(root, 'apps/desktop/src/lib/firebase.ts')));
  const modules = await Promise.all(['firebase/app', 'firebase/auth', 'firebase/firestore'].map(name => import(pathToFileURL(desktopRequire.resolve(name)).href)));
  if (!configured.privateSocialOnlineConfiguration(configured.bundledFirebaseConfig)) fail('SDK_CONFIGURATION_GATE');
  return { factory, app: modules[0], auth: modules[1], firestore: modules[2], config: configured.bundledFirebaseConfig,
    privateConfiguration: configured.privateSocialOnlineConfiguration };
}
export async function onlineManager(binary, spawnProcess = spawn) {
  const child = spawnProcess(binary, ['--ignored', '--exact', testName, '--nocapture'], {
    windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'], env: childEnvironment() });
  const decoder = new StringDecoder('utf8'), queue = [], waiters = [];
  let buffered = '', bytes = 0, stderrBytes = 0, ended = false, forced = false, failure, busy = false;
  let resolveExit; const exited = new Promise(resolve => { resolveExit = resolve; });
  const reject = code => { failure ??= new SmokeError(code); for (const waiter of waiters.splice(0)) waiter.reject(failure); };
  child.once('error', () => { reject('NATIVE_START_FAILED'); resolveExit({ code: null }); });
  child.once('exit', (code, signal) => { ended = true; resolveExit({ code, signal }); reject('NATIVE_EXITED'); });
  child.stdin.on('error', () => reject('NATIVE_INPUT_FAILED'));
  child.stderr.on('data', chunk => { stderrBytes += chunk.length; if (stderrBytes > 32768) { reject('NATIVE_STDERR_LIMIT'); forced = true; child.kill(); } });
  child.stdout.on('data', chunk => {
    bytes += chunk.length; buffered += decoder.write(chunk);
    if (bytes > 2097152 || buffered.length > 65536) { reject('NATIVE_OUTPUT_LIMIT'); forced = true; child.kill(); return; }
    let end;
    while ((end = buffered.indexOf('\n')) >= 0) {
      const line = buffered.slice(0, end).replace(/\r$/, ''); buffered = buffered.slice(end + 1);
      const marker = line.indexOf('SOCIAL89_JSON:'); if (marker < 0) continue;
      let value; try { value = JSON.parse(line.slice(marker + 14)); } catch { reject('NATIVE_JSON_INVALID'); return; }
      if (waiters.length) waiters.shift().resolve(value); else if (queue.length < 8) queue.push(value); else reject('NATIVE_QUEUE_LIMIT');
    }
  });
  child.stdout.once('end', () => { if (waiters.length) reject('NATIVE_EOF'); });
  const next = () => new Promise((resolve, rejectWaiter) => {
    if (failure) return rejectWaiter(failure); if (queue.length) return resolve(queue.shift()); if (ended) return rejectWaiter(new SmokeError('NATIVE_EOF'));
    const timer = setTimeout(() => reject('NATIVE_RESPONSE_TIMEOUT'), 15000);
    waiters.push({ resolve: value => { clearTimeout(timer); resolve(value); }, reject: error => { clearTimeout(timer); rejectWaiter(error); } });
  });
  async function call(command, args = {}) {
    if (busy || ended) fail('NATIVE_CALL_STATE');
    const text = JSON.stringify({ command, ...args }); if (Buffer.byteLength(text) > 16383) fail('NATIVE_REQUEST_LIMIT');
    busy = true; try { child.stdin.write(text + '\n'); return await next(); } finally { busy = false; }
  }
  let closing;
  const close = () => closing ??= (async () => {
    let failed = ended;
    if (!ended) { try { const result = await call('quit'); if (!result.ok || result.value?.cleanup !== true) failed = true; } catch { failed = true; } }
    child.stdin.end();
    try { await bounded(exited, 8000, 'NATIVE_EXIT_TIMEOUT'); }
    catch { forced = true; child.kill(); await bounded(exited, 5000, 'NATIVE_REAP_TIMEOUT'); }
    if (failed || forced || (await exited).code !== 0) fail('NATIVE_CLEANUP_FAILED');
    return { cleanup: true, forced: false };
  })();
  let ready;
  try { ready = await next(); if (ready.event !== 'ready' || ready.mode !== 'compiledOnline' || ready.capabilities?.privateInvitations !== true) fail('NATIVE_ONLINE_GATE_REQUIRED'); }
  catch (error) { try { await close(); } catch {} throw error; }
  return { ready, call, close, async ok(command, args) { const result = await call(command, args); if (result.ok !== true) fail('NATIVE_COMMAND_REJECTED'); return result.value; } };
}
async function waitSnapshot(client, predicate, milliseconds = 20000, allowTerminalCleanup = false) {
  const until = performance.now() + milliseconds;
  while (performance.now() < until) { const value = await client.ok('snapshot'); if (predicate(value)) return value;
    if (!allowTerminalCleanup && ['failed', 'closed'].includes(value.phase)) fail('NATIVE_TERMINAL_BEFORE_EXPECTED'); await delay(100); }
  fail('NATIVE_STATE_TIMEOUT');
}
export async function rebind(address) {
  if (!/^127\.0\.0\.1:[1-9][0-9]{0,4}$/.test(address)) fail('BRIDGE_ADDRESS_INVALID');
  const server = createServer();
  await bounded(new Promise((resolve, reject) => { server.once('error', reject); server.listen(Number(address.split(':')[1]), '127.0.0.1', resolve); }), 3000, 'REBIND_TIMEOUT');
  await new Promise(resolve => server.close(resolve));
}
export async function fixedTcpRoundtrip(address) {
  if (!/^127\.0\.0\.1:[1-9][0-9]{0,4}$/.test(address)) fail('TCP_PROBE_INVALID');
  const payload = Buffer.alloc(1048576, 0xa5);
  const socket = createConnection({ host: '127.0.0.1', port: Number(address.split(':')[1]), allowHalfOpen: true });
  let count = 0; socket.on('error', () => {});
  try { await bounded(new Promise((resolve, reject) => {
    socket.once('connect', () => socket.end(payload));
    socket.on('data', chunk => { if (!chunk.equals(payload.subarray(count, count + chunk.length))) reject(new SmokeError('TCP_ECHO_MISMATCH')); count += chunk.length; });
    socket.once('end', resolve); socket.once('error', reject);
  }), 15000, 'TCP_ECHO_TIMEOUT'); if (count !== payload.length) fail('TCP_ECHO_LENGTH'); }
  finally { socket.destroy(); }
  return { tcpBytesEachDirection: count };
}
export function withdrawalObservation({ beforeHost, afterHost, closedGuest, privateView, operationId, sessionId, guestUid }) {
  const sameHost = value => value.operationId === operationId && value.phase === 'hostListening' && !value.errorCode
    && value.social?.sessionId === sessionId && value.social.visibility === 'private';
  if (!sameHost(beforeHost) || !beforeHost.social.participants?.some(peer => peer.accountId === guestUid && peer.state === 'connected')
    || !sameHost(afterHost) || !Array.isArray(afterHost.social.participants) || afterHost.social.participants.some(peer => peer.accountId === guestUid)
    || privateView.sessionId !== sessionId || !privateView.outgoing.some(peer => peer.person.uid === guestUid && peer.status === 'revoked')
    || !['closed', 'failed'].includes(closedGuest.phase) || closedGuest.cleanupState !== 'complete' || closedGuest.tcpAddress !== null) fail('WITHDRAWAL_EVIDENCE_INCOMPLETE');
  return { privateAuthorization: 'revoked_server_snapshot', originalHostStillReady: true, participantRemoved: true,
    guestPhase: closedGuest.phase, guestCode: /^connect_[a-z_0-9]{1,80}$/.test(closedGuest.errorCode ?? '') ? closedGuest.errorCode : null,
    causality: 'not_proven_by_terminal_reason' };
}

// Cloud mutations are confined to the two newly created identities. Every SDK
// mutation remains tracked after a deadline: uncertain writes forbid deletion.
export function mutationTracker() {
  const pending = new Set();
  return { async run(work, milliseconds = 20000) { const promise = Promise.resolve().then(work); pending.add(promise);
    promise.then(() => pending.delete(promise), () => pending.delete(promise));
    return bounded(promise, milliseconds, 'SDK_OPERATION_TIMEOUT'); },
  async settled(milliseconds = 20000) { await bounded(Promise.allSettled([...pending]), milliseconds, 'SDK_MUTATIONS_UNSETTLED'); return pending.size === 0; } };
}
async function register(sdk, owner, role, client, tracker) {
  const plan = owner.accounts.find(value => value.role === role); if (!plan) fail('OWNED_ACCOUNT_PLAN_MISSING');
  const app = sdk.app.initializeApp(sdk.config, `connect-private-smoke-${randomUUID()}`);
  const auth = sdk.auth.initializeAuth(app, { persistence: sdk.auth.inMemoryPersistence });
  const db = sdk.firestore.getFirestore(app), actor = { app, auth, db, role, user: null, service: null, creationStarted: false };
  // Retain ownership even if a later document operation fails.
  owner.runtimeActors.push(actor);
  const password = randomBytes(32).toString('base64url');
  await owner.recordAccountCreateStarted(role); actor.creationStarted = true;
  await tracker.run(async () => {
    actor.user = (await sdk.auth.createUserWithEmailAndPassword(auth, plan.email, password)).user;
    await owner.recordAccount({ role, uid: actor.user.uid, email: actor.user.email, createdAt: actor.user.metadata.creationTime });
  });
  const { doc, serverTimestamp, writeBatch } = sdk.firestore, batch = writeBatch(db), uid = actor.user.uid;
  batch.set(doc(db, 'usernames', plan.nickname), { uid, usernameLower: plan.nickname, createdAt: serverTimestamp() });
  batch.set(doc(db, 'users', uid), { uid, username: plan.nickname, usernameLower: plan.nickname, email: plan.email,
    avatarUrl: null, skinUrl: null, capeUrl: null, skinModel: 'classic', role: 'PLAYER', createdAt: serverTimestamp(),
    stats: { totalPlayTimeSeconds: 0, questsCompleted: 0, lastPlayedModpack: null } });
  batch.set(doc(db, 'publicProfiles', uid), { uid, username: plan.nickname, avatarUrl: null, skinUrl: null, skinModel: 'classic', updatedAt: serverTimestamp() });
  await tracker.run(() => batch.commit()); await owner.snapshotDocuments();
  actor.service = sdk.factory({ app, auth, db, config: sdk.config, authScope: { id: randomUUID(), persistent: false } },
    { privateOnlineCapability: () => client.ready.capabilities.privateInvitations === true });
  if (actor.service.privateInvitations?.environment !== 'online') fail('SDK_PRIVATE_GATE_REQUIRED');
  return actor;
}
export async function runSmoke({ binary, sha256, owner, sdk, createManager = onlineManager }) {
  const tracker = mutationTracker(), clients = [], metrics = { transport: 'same_pc', minecraft: 'not_tested', voice: 'not_tested' };
  let primary, cleanupError, host, guest, sessionId, bridge, invited = false, preflightPassed = false, sessionStopped = true;
  owner.runtimeActors = [];
  try {
    await owner.preflight(); preflightPassed = true;
    // Prove both native gates before creating anything online.
    clients.push(await createManager(binary)); clients.push(await createManager(binary));
    host = await register(sdk, owner, 'host', clients[0], tracker); guest = await register(sdk, owner, 'guest', clients[1], tracker);
    await tracker.run(() => host.service.publishSession(owner.sessionDraft));
    const view = await bounded(host.service.privateInvitations.snapshot(), 20000, 'SDK_READ_TIMEOUT');
    sessionId = view.sessionId; if (!sessionId) fail('SESSION_MISSING'); await owner.recordSession(sessionId);
    await tracker.run(async () => { await host.service.privateInvitations.invite(sessionId, guest.user.uid); invited = true; }); await owner.snapshotDocuments();
    const inbox = await bounded(guest.service.privateInvitations.snapshot(), 20000, 'SDK_READ_TIMEOUT');
    if (inbox.incoming.length !== 1 || inbox.incoming[0].session.id !== sessionId) fail('INBOX_SCOPE_MISMATCH');
    const source = await clients[0].ok('discoverFixture');
    const reservation = await clients[0].ok('reserveDiscovered', { selectionId: source.selectionId,
      request: { accountEpoch: clients[0].ready.capabilities.accountEpoch, accountId: host.user.uid, sessionId, visibility: 'private' } });
    await clients[0].ok('authenticate', { ...reservation, token: await bounded(host.user.getIdToken(), 20000, 'AUTH_TOKEN_TIMEOUT') });
    const review = await waitSnapshot(clients[0], value => value.social?.stage === 'hostReview');
    if (review.social.visibility !== 'private' || review.social.gameTarget !== source.target) fail('HOST_REVIEW_MISMATCH');
    await clients[0].ok('confirm', { ...reservation, generation: review.generation, digest: review.social.digest });
    await waitSnapshot(clients[0], value => value.social?.stage === 'waitingGuest');
    const pending = await clients[1].ok('reserve', { request: { accountEpoch: clients[1].ready.capabilities.accountEpoch,
      accountId: guest.user.uid, sessionId, role: 'guest', gameTarget: null, visibility: null } });
    await clients[1].ok('authenticate', { ...pending, token: await bounded(guest.user.getIdToken(), 20000, 'AUTH_TOKEN_TIMEOUT') });
    const ready = await waitSnapshot(clients[1], value => value.phase === 'guestConnected'); bridge = ready.tcpAddress;
    const status = await clients[1].ok('probeStatus'); if (status.protocolMatchesRequested !== true) fail('STATUS_MISMATCH');
    metrics.statusAndMatchingPong = true;
    Object.assign(metrics, await fixedTcpRoundtrip(bridge));
    const before = await clients[1].ok('snapshot'); if (before.phase !== 'guestConnected') fail('REVOKE_PRECONDITION_FAILED');
    const beforeHost = await waitSnapshot(clients[0], value => value.social?.participants?.some(peer => peer.accountId === guest.user.uid && peer.state === 'connected'));
    const started = performance.now(); await tracker.run(async () => { await host.service.privateInvitations.revoke(sessionId, guest.user.uid); invited = false; });
    const closedGuest = await waitSnapshot(clients[1], value => ['closed', 'failed'].includes(value.phase) && value.cleanupState === 'complete', 20000, true);
    metrics.withdrawalObservedAfterRevocationMs = Math.round(performance.now() - started);
    const privateView = await bounded(host.service.privateInvitations.snapshot(), 20000, 'SDK_READ_TIMEOUT');
    const afterHost = await waitSnapshot(clients[0], value => Array.isArray(value.social?.participants) && !value.social.participants.some(peer => peer.accountId === guest.user.uid));
    metrics.withdrawal = withdrawalObservation({ beforeHost, afterHost, closedGuest, privateView, operationId: reservation.operationId, sessionId, guestUid: guest.user.uid });
    await rebind(bridge); metrics.tcpRebind = true;
  } catch (error) { primary = error; }
  finally {
    for (const client of clients.reverse()) try { await client.close(); } catch (error) { cleanupError ??= error; sessionStopped = false; }
    try { await tracker.settled();
      if (owner.runtimeActors.some(actor => actor.creationStarted && !actor.user)) cleanupError ??= new SmokeError('AUTH_CREATION_RESULT_UNCERTAIN');
      if (invited && host?.service && sessionId && guest?.user) await tracker.run(() => host.service.privateInvitations.revoke(sessionId, guest.user.uid));
      // This is a new, exclusively owned account. Close also covers a publish
      // whose response was lost; the SDK checks its exact owner/session itself.
      if (host?.service) await tracker.run(() => host.service.closeSession());
      await tracker.settled();
      if (preflightPassed) {
        await owner.snapshotDocuments();
        const cleaned = await owner.cleanupDocuments({ sessionStopped, pendingMutationsSettled: true });
        if (cleaned?.passed !== true) fail('OWNED_DOCUMENT_CLEANUP_UNCONFIRMED');
        for (const actor of [...owner.runtimeActors].reverse()) if (actor.user) {
          await tracker.run(async () => { await sdk.auth.deleteUser(actor.user); await owner.recordAuthDeleted(actor.role); });
        }
      }
    } catch (error) { cleanupError ??= error; }
    for (const actor of owner.runtimeActors) for (const close of [() => sdk.firestore.terminate(actor.db), () => sdk.app.deleteApp(actor.app)])
      try { await bounded(close(), 5000, 'SDK_CLOSE_TIMEOUT'); } catch (error) { cleanupError ??= error; }
  }
  const ownership = owner.result();
  if (preflightPassed && ownership.complete !== true) cleanupError ??= new SmokeError('OWNED_CLEANUP_INCOMPLETE');
  return { schema: 1, event: 'private_online_smoke', binarySha256: sha256, passed: !primary && !cleanupError,
    code: primary ? redactedError(primary) : cleanupError ? redactedError(cleanupError) : 'passed', cleanupPassed: !cleanupError,
    cleanupCode: cleanupError ? redactedError(cleanupError) : null,
    ...metrics, ownership, limits: ['No Minecraft gameplay, audible voice, GUI or external-network homologation.',
      'Revocation is confirmed in the server SDK snapshot and closure observed; a generic native terminal reason does not establish exclusive causality.',
      'Production session deadline unchanged; only records belonging to these newly created diagnostic identities are cleaned.'] };
}
async function main() {
  const options = parseArguments(process.argv.slice(2)); if (process.platform !== 'win32') fail('WINDOWS_REQUIRED');
  checkEnvironment();
  const binary = await verifyBinary(options.binary, options.sha256), sdk = await loadProductionSdk();
  if (!options.run) { console.log(JSON.stringify({ event: 'private_online_smoke_checked', remoteWritesPerformed: false, binarySha256: options.sha256 })); return; }
  const expectedOperator = runOperator();
  const { createCleanupOwner } = await import('./connect-private-online-owned-cleanup.mjs');
  const runId = randomUUID(), runDirectory = resolve(root, '.aurora/connect-validation', `private-online-smoke-${runId}`);
  await mkdir(runDirectory);
  const owner = await createCleanupOwner({ runId, rootDirectory: runDirectory, expectedOperator });
  const result = await runSmoke({ ...options, binary, owner, sdk });
  await writeFile(resolve(runDirectory, 'result.json'), JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ ...result, ownership: undefined })); if (!result.passed) process.exitCode = 1;
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main().catch(error => {
  console.error(JSON.stringify({ event: 'private_online_smoke_failed', code: redactedError(error) })); process.exitCode = 1;
});
