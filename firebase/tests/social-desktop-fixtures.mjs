// Owned local SDK/ConnectManager fixtures. No production account, argv credential,
// simulated transport or generic command execution is provided by this helper.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createConnection, createServer } from 'node:net';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { StringDecoder } from 'node:string_decoder';
import { initializeApp, deleteApp } from 'firebase/app';
import { connectAuthEmulator, createUserWithEmailAndPassword, getAuth, signOut } from 'firebase/auth';
import { connectFirestoreEmulator, doc, getDocFromServer, getFirestore, serverTimestamp, terminate, writeBatch } from 'firebase/firestore';

const desktopRequire = createRequire(new URL('../../apps/desktop/package.json', import.meta.url));
export const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
export async function deadline(work, ms, code) {
  let timer;
  try { return await Promise.race([work, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(code)), ms); })]); }
  finally { clearTimeout(timer); }
}
export async function socialFactory() {
  const ts = desktopRequire('typescript');
  const compile = source => ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  const shared = `data:text/javascript;base64,${Buffer.from(compile(await readFile(new URL('../../apps/desktop/src/shared/social.ts', import.meta.url), 'utf8'))).toString('base64')}`;
  const source = compile(await readFile(new URL('../../apps/desktop/src/lib/socialFirebase.ts', import.meta.url), 'utf8'))
    .replace('"../shared/social"', JSON.stringify(shared))
    .replace('"firebase/firestore"', JSON.stringify(import.meta.resolve('firebase/firestore')));
  return (await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`)).createSocialFirebaseService;
}
export async function account(factory, prefix = 'ui89') {
  const nickname = `${prefix}_${crypto.randomUUID().replaceAll('-', '').slice(0, 9)}`;
  const projectId = 'demo-aurora-social';
  const app = initializeApp({ projectId, apiKey: projectId }, nickname);
  const auth = getAuth(app); connectAuthEmulator(auth, 'http://127.0.0.1:9187', { disableWarnings: true });
  const db = getFirestore(app); connectFirestoreEmulator(db, '127.0.0.1', 8187);
  const password = 'aurora-local-desktop-fixture';
  try {
    const email = `${nickname}@aurora.internal`;
    const user = (await createUserWithEmailAndPassword(auth, email, password)).user;
    const uid = user.uid, batch = writeBatch(db);
    batch.set(doc(db, 'usernames', nickname), { uid, usernameLower: nickname, createdAt: serverTimestamp() });
    batch.set(doc(db, 'users', uid), { uid, username: nickname, usernameLower: nickname, email, avatarUrl: null, skinUrl: null, capeUrl: null,
      skinModel: 'classic', role: 'PLAYER', createdAt: serverTimestamp(), stats: { totalPlayTimeSeconds: 0, questsCompleted: 0, lastPlayedModpack: null } });
    batch.set(doc(db, 'publicProfiles', uid), { uid, username: nickname, avatarUrl: null, skinUrl: null, skinModel: 'classic', updatedAt: serverTimestamp() });
    await batch.commit();
    return { uid, nickname, password, db, service: factory({ db, auth }), token: () => user.getIdToken(),
      async sessionId() { return (await getDocFromServer(doc(db, 'socialOwners', uid))).data().sessionId; },
      async close() {
        let failure;
        for (const operation of [() => signOut(auth), () => terminate(db), () => deleteApp(app)]) {
          try { await deadline(operation(), 4_000, 'SDK_CLEANUP_TIMEOUT'); }
          catch (error) { failure ??= error; }
        }
        if (failure) throw failure;
      } };
  } catch (error) { await terminate(db).catch(() => {}); await deleteApp(app); throw error; }
}

export async function echo() {
  const sockets = new Set(); let received = 0, connections = 0;
  const server = createServer({ allowHalfOpen: true }, socket => {
    connections++; sockets.add(socket); socket.on('close', () => sockets.delete(socket)); socket.on('error', () => {});
    socket.on('data', bytes => { received += bytes.length; }); socket.pipe(socket);
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(43001, '127.0.0.1', resolve); });
  return { target: '127.0.0.1:43001', get received() { return received; }, get connections() { return connections; }, async close() {
    for (const socket of sockets) socket.destroy();
    await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  } };
}
export async function rebind(address) {
  assert.match(address, /^127\.0\.0\.1:[0-9]+$/);
  const server = createServer();
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(Number(address.split(':')[1]), '127.0.0.1', resolve); });
  await new Promise(resolve => server.close(resolve));
}
export async function tcpRoundtrip(address, length = 1_048_576) {
  assert.match(address, /^127\.0\.0\.1:[0-9]+$/);
  const socket = createConnection({ host: '127.0.0.1', port: Number(address.split(':')[1]), allowHalfOpen: true });
  socket.on('error', () => {});
  const payload = Buffer.alloc(length); for (let i = 0; i < length; i++) payload[i] = i % 251;
  let count = 0;
  const done = new Promise((resolve, reject) => {
    socket.on('data', bytes => { if (!bytes.equals(payload.subarray(count, count + bytes.length))) reject(new Error('TCP_ECHO_MISMATCH')); count += bytes.length; });
    socket.once('end', resolve); socket.once('error', reject);
  });
  try { socket.end(payload); await deadline(done, 8_000, 'TCP_ECHO_TIMEOUT'); assert.equal(count, length); }
  finally { socket.destroy(); }
}
export async function activeTcp(address) {
  assert.match(address, /^127\.0\.0\.1:[0-9]+$/);
  const socket = createConnection({ host: '127.0.0.1', port: Number(address.split(':')[1]), allowHalfOpen: true });
  let received = 0, pulse, resolveLost, resolveFirst, failure, lostObserved = false;
  const first = new Promise(resolve => { resolveFirst = resolve; });
  const lost = new Promise(resolve => { resolveLost = () => { lostObserved = true; resolve(); }; });
  socket.on('data', bytes => { if (!bytes.every(byte => byte === 71)) { failure = new Error('ACTIVE_TCP_ECHO_MISMATCH'); resolveLost(); socket.destroy(); } received += bytes.length; resolveFirst(); });
  socket.once('end', resolveLost); socket.once('close', resolveLost); socket.on('error', resolveLost);
  function close() { clearInterval(pulse); socket.destroy(); }
  try {
    await deadline(new Promise((resolve, reject) => { socket.once('connect', resolve); socket.once('error', reject); }), 4_000, 'ACTIVE_TCP_CONNECT_TIMEOUT');
    pulse = setInterval(() => { if (!socket.destroyed) socket.write(Buffer.alloc(256, 71)); }, 50);
    await deadline(first, 4_000, 'ACTIVE_TCP_FIRST_ECHO_TIMEOUT');
    if (failure) throw failure;
    return { get received() { return received; }, get isOpen() { return !lostObserved && !socket.destroyed && received > 0; },
      async withdrawn() { await deadline(lost, 6_000, 'ACTIVE_TCP_WITHDRAWAL_TIMEOUT'); const closed = new Promise(resolve => socket.once('close', resolve)); const already = socket.closed; close(); if (!already) await deadline(closed, 2_000, 'ACTIVE_TCP_PEER_CLOSE_TIMEOUT'); if (failure) throw failure; }, close };
  } catch (error) { close(); throw error; }
}

// Exact cfg(test) process in the desktop library: the same ConnectManager used
// by Tauri, with no transport objects assembled by this runner.
export async function manager(binary, lab) {
  const child = spawn(binary, ['--ignored', '--exact', 'engine::connect::live::social::tests::social_desktop_process', '--nocapture'], {
    windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'], env: { ...process.env,
      AURORA_SOCIAL_DESKTOP_FIXTURE: '1', AURORA_SOCIAL_LAB_PIN: lab.admissionPublicKey, AURORA_SOCIAL_LAB_ISSUER: lab.issuerInstance },
  });
  const decoder = new StringDecoder('utf8'), queue = [], waiters = [];
  let buffered = '', total = 0, stderrBytes = 0, ended = false, forced = false, failed;
  let resolveExit; const exited = new Promise(resolve => { resolveExit = resolve; });
  const fail = code => { failed ??= new Error(code); for (const waiter of waiters.splice(0)) waiter.reject(failed); };
  child.once('error', () => { fail('MANAGER_START_FAILED'); resolveExit({ code: null }); });
  child.stdin.on('error', () => { if (!ended) fail('MANAGER_INPUT_FAILED'); });
  child.once('exit', (code, signal) => { ended = true; resolveExit({ code, signal }); });
  child.stderr.on('data', chunk => { stderrBytes += chunk.length; if (stderrBytes > 32_768) { fail('MANAGER_STDERR_LIMIT'); forced = true; child.kill(); } });
  child.stdout.on('data', chunk => {
    total += chunk.length; buffered += decoder.write(chunk);
    if (total > 2_097_152 || buffered.length > 65_536) { fail('MANAGER_OUTPUT_LIMIT'); forced = true; child.kill(); return; }
    let end;
    while ((end = buffered.indexOf('\n')) >= 0) {
      const line = buffered.slice(0, end).replace(/\r$/, ''); buffered = buffered.slice(end + 1);
      const marker = line.indexOf('SOCIAL89_JSON:'); if (marker < 0) continue;
      let value; try { value = JSON.parse(line.slice(marker + 14)); } catch { fail('MANAGER_INVALID_JSON'); return; }
      if (waiters.length) waiters.shift().resolve(value); else if (queue.length < 8) queue.push(value); else fail('MANAGER_QUEUE_LIMIT');
    }
  });
  child.stdout.once('end', () => { for (const waiter of waiters.splice(0)) waiter.reject(new Error('MANAGER_EOF')); });
  const next = () => new Promise((resolve, reject) => {
    if (failed) return reject(failed); if (queue.length) return resolve(queue.shift()); if (ended) return reject(new Error('MANAGER_EOF'));
    const timer = setTimeout(() => fail('MANAGER_RESPONSE_TIMEOUT'), 12_000);
    waiters.push({ resolve: value => { clearTimeout(timer); resolve(value); }, reject: error => { clearTimeout(timer); reject(error); } });
  });
  async function call(command, args = {}) {
    if (ended) throw new Error('MANAGER_EXITED');
    const input = JSON.stringify({ command, ...args }); if (Buffer.byteLength(input) > 24_576) throw new Error('MANAGER_REQUEST_LIMIT');
    child.stdin.write(input + '\n'); return next();
  }
  async function reap() {
    child.stdin.end();
    try { await deadline(exited, 8_000, 'MANAGER_EXIT_TIMEOUT'); }
    catch { forced = true; child.kill(); await deadline(exited, 5_000, 'MANAGER_REAP_TIMEOUT'); }
    assert.equal(forced, false, 'manager required forced stop');
    assert.equal((await exited).code, 0, 'manager child failed');
  }
  let ready;
  try { ready = await next(); assert.equal(ready.event, 'ready'); }
  catch (error) {
    try { await reap(); } catch (cleanup) { throw new AggregateError([error, cleanup], 'MANAGER_START_AND_CLEANUP_FAILED'); }
    throw error;
  }
  let closing;
  return { ready, call, async ok(command, args) { const result = await call(command, args); assert.equal(result.ok, true, result.code ?? 'MANAGER_CALL_FAILED'); return result.value; },
    close() {
      closing ??= (async () => {
        let cleanupError;
        try {
          const result = await call('quit');
          assert.equal(result.ok, true, 'manager quit failed');
          assert.equal(result.value?.cleanup, true, 'manager cleanup not confirmed');
        } catch (error) { cleanupError = error; }
        try { await reap(); }
        catch (error) { if (cleanupError) throw new AggregateError([cleanupError, error], 'MANAGER_CLEANUP_AND_REAP_FAILED'); throw error; }
        if (cleanupError) throw cleanupError;
        if (failed) throw failed;
      })();
      return closing;
    } };
}
export async function waitSnapshot(client, predicate, milliseconds = 12_000) {
  const until = performance.now() + milliseconds; let snapshot;
  while (performance.now() < until) { snapshot = await client.ok('snapshot'); if (predicate(snapshot)) return snapshot; await delay(80); }
  // Only public static state is surfaced on failure, never identity or tokens.
  throw new Error(`MANAGER_STATE_TIMEOUT:${snapshot?.phase}:${snapshot?.social?.stage}:${snapshot?.errorCode ?? 'none'}`);
}
export async function prepareHost(client, who, sessionId, visibility) {
  const reservation = await client.ok('reserve', { request: { accountEpoch: client.ready.capabilities.accountEpoch, accountId: who.uid, sessionId,
    role: 'host', gameTarget: '127.0.0.1:43001', visibility } });
  await client.ok('authenticate', { ...reservation, token: await who.token() });
  const review = await waitSnapshot(client, value => value.social?.stage === 'hostReview');
  assert.equal(review.social.gameTarget, '127.0.0.1:43001'); assert.equal(review.social.visibility, visibility);
  await client.ok('confirm', { ...reservation, generation: review.generation, digest: review.social.digest });
  await waitSnapshot(client, value => value.social?.stage === 'waitingGuest');
  return reservation;
}
export async function prepareGuest(client, who, sessionId) {
  const reservation = await client.ok('reserve', { request: { accountEpoch: client.ready.capabilities.accountEpoch, accountId: who.uid, sessionId,
    role: 'guest', gameTarget: null, visibility: null } });
  await client.ok('authenticate', { ...reservation, token: await who.token() });
  return reservation;
}
