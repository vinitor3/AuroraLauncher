import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createServer, createConnection } from "node:net";
import { readFile, stat } from "node:fs/promises";
import { createRequire } from "node:module";
import { isAbsolute } from "node:path";
import { StringDecoder } from "node:string_decoder";
import { test } from "node:test";
import { initializeApp, deleteApp } from "firebase/app";
import { connectAuthEmulator, createUserWithEmailAndPassword, getAuth, signOut } from "firebase/auth";
import { connectFirestoreEmulator, doc, getDocFromServer, getFirestore, serverTimestamp, terminate, writeBatch } from "firebase/firestore";
import { startSocialBindingLab } from "./social-binding-server.mjs";

const enabled = process.env.AURORA_SOCIAL_ADMISSION_INTEGRATION === "1";
const desktopRequire = createRequire(new URL("../../apps/desktop/package.json", import.meta.url));
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function deadline(work, ms, code) {
  let timer;
  try { return await Promise.race([work, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(code)), ms); })]); }
  finally { clearTimeout(timer); }
}
async function socialFactory() {
  const ts = desktopRequire("typescript");
  const compile = source => ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  const shared = `data:text/javascript;base64,${Buffer.from(compile(await readFile(new URL("../../apps/desktop/src/shared/social.ts", import.meta.url), "utf8"))).toString("base64")}`;
  const clock = `data:text/javascript;base64,${Buffer.from(compile(await readFile(new URL("../../apps/desktop/src/lib/socialServerClock.ts", import.meta.url), "utf8"))).toString("base64")}`;
  const organization = `data:text/javascript;base64,${Buffer.from(compile(await readFile(new URL("../../apps/desktop/src/lib/socialOrganization.ts", import.meta.url),"utf8")).replace('"firebase/firestore"', JSON.stringify(import.meta.resolve("firebase/firestore")))).toString("base64")}`;
  const privateInvitations = `data:text/javascript;base64,${Buffer.from(compile(await readFile(new URL("../../apps/desktop/src/lib/socialPrivateInvitations.ts", import.meta.url),"utf8")).replace('"firebase/firestore"', JSON.stringify(import.meta.resolve("firebase/firestore")))).toString("base64")}`;
  const source = compile(await readFile(new URL("../../apps/desktop/src/lib/socialFirebase.ts", import.meta.url), "utf8"))
    .replace('"../shared/social"', JSON.stringify(shared))
    .replace('"./socialServerClock"', JSON.stringify(clock))
    .replace('"./firebase"', JSON.stringify("data:text/javascript,export const socialLabBuild=true;export const privateSocialOnlineConfiguration=()=>false"))
    .replace('"./socialOrganization"', JSON.stringify(organization))
    .replace('"./socialPrivateInvitations"', JSON.stringify(privateInvitations))
    .replace('"firebase/firestore"', JSON.stringify(import.meta.resolve("firebase/firestore")));
  return (await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`)).createSocialFirebaseService;
}
async function account(factory) {
  const nickname = `join_${crypto.randomUUID().replaceAll("-", "").slice(0, 10)}`;
  const projectId = "demo-aurora-social";
  const app = initializeApp({ projectId, apiKey: projectId }, nickname);
  const auth = getAuth(app); connectAuthEmulator(auth, "http://127.0.0.1:9187", { disableWarnings: true });
  const db = getFirestore(app); connectFirestoreEmulator(db, "127.0.0.1", 8187);
  try {
    const email = `${nickname}@aurora.internal`;
    const user = (await createUserWithEmailAndPassword(auth, email, "aurora-local-admission-fixture")).user;
    const uid = user.uid, batch = writeBatch(db);
    batch.set(doc(db, "usernames", nickname), { uid, usernameLower: nickname, createdAt: serverTimestamp() });
    batch.set(doc(db, "users", uid), { uid, username: nickname, usernameLower: nickname, email, avatarUrl: null, skinUrl: null, capeUrl: null,
      skinModel: "classic", role: "PLAYER", createdAt: serverTimestamp(), stats: { totalPlayTimeSeconds: 0, questsCompleted: 0, lastPlayedModpack: null } });
    batch.set(doc(db, "publicProfiles", uid), { uid, username: nickname, avatarUrl: null, skinUrl: null, skinModel: "classic", updatedAt: serverTimestamp() });
    await batch.commit();
    return { uid, db, service: factory({ db, auth, config: { projectId }, authScope: { id: nickname, persistent: false } }), token: () => user.getIdToken(), async close() { await signOut(auth); await terminate(db); await deleteApp(app); } };
  } catch (error) { await terminate(db).catch(() => {}); await deleteApp(app); throw error; }
}

// A real, owned native child. Input/output and waits are bounded; no token enters argv or logs.
async function client(binary, args) {
  const child = spawn(binary, args, { windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
  const decoder = new StringDecoder("utf8"), queued = [], waiters = [];
  let buffered = "", total = 0, stderrBytes = 0, ended = false, outputEnded = false, closing = false, failed, forced = false, lastEvent;
  let resolveExit;
  const exited = new Promise(resolve => { resolveExit = resolve; });
  function fail(code) { failed ??= new Error(code); for (const waiter of waiters.splice(0)) waiter.reject(failed); }
  child.once("error", () => { fail("SOCIAL_NATIVE_START_FAILED"); resolveExit({ code: null, signal: "start_failed" }); });
  child.stdin.on("error", () => { if (!ended && !closing) fail("SOCIAL_NATIVE_INPUT_FAILED"); });
  child.once("exit", (code, signal) => { ended = true; resolveExit({ code, signal }); });
  child.stderr.on("data", chunk => { stderrBytes += chunk.length; if (stderrBytes > 8_192) { fail("SOCIAL_NATIVE_STDERR_LIMIT"); forced = true; child.kill(); } });
  child.stdout.on("data", chunk => {
    total += chunk.length; buffered += decoder.write(chunk);
    if (total > 262_144 || buffered.length > 16_384) { fail("SOCIAL_NATIVE_OUTPUT_LIMIT"); forced = true; child.kill(); return; }
    let newline;
    while ((newline = buffered.indexOf("\n")) >= 0) {
      const line = buffered.slice(0, newline).replace(/\r$/, ""); buffered = buffered.slice(newline + 1);
      if (!line || Buffer.byteLength(line) > 8_192) { fail("SOCIAL_NATIVE_LINE_LIMIT"); return; }
      let value; try { value = JSON.parse(line); } catch { fail("SOCIAL_NATIVE_JSON_INVALID"); return; }
      lastEvent = typeof value.event === "string" ? value.event : "proof";
      const waiter = waiters.shift();
      if (waiter) waiter.resolve(value); else if (queued.length < 32) queued.push(value); else fail("SOCIAL_NATIVE_QUEUE_LIMIT");
    }
  });
  child.stdout.once("end", () => { outputEnded = true; for (const waiter of waiters.splice(0)) waiter.reject(new Error("SOCIAL_NATIVE_EOF")); });
  const next = () => new Promise((resolve, reject) => {
    if (failed) { reject(failed); return; }
    if (queued.length) { resolve(queued.shift()); return; }
    if (ended || outputEnded) { reject(new Error("SOCIAL_NATIVE_EOF")); return; }
    const timer = setTimeout(() => { fail("SOCIAL_NATIVE_EVENT_TIMEOUT"); }, 8_000);
    waiters.push({ resolve: value => { clearTimeout(timer); resolve(value); }, reject: error => { clearTimeout(timer); reject(error); } });
  });
  const send = value => { if (ended) throw new Error("SOCIAL_NATIVE_ALREADY_EXITED"); const line = typeof value === "string" ? value : JSON.stringify(value); if (Buffer.byteLength(line) > 8_192) throw new Error("SOCIAL_NATIVE_INPUT_LIMIT"); child.stdin.write(line + "\n"); };
  return { next, send, get lastEvent() { return lastEvent; }, async closed() { return deadline(exited, 8_000, "SOCIAL_NATIVE_EXIT_TIMEOUT"); }, async close() {
    closing = true;
    if (!ended) { child.stdin.end("stop\n"); try { await deadline(exited, 5_000, "SOCIAL_NATIVE_STOP_TIMEOUT"); } catch { forced = true; child.kill(); await deadline(exited, 5_000, "SOCIAL_NATIVE_REAP_TIMEOUT"); } }
    assert.equal(forced, false, "native child required forced cleanup");
    if (failed) throw failed;
    assert.equal((await exited).code, 0, "native child failed before clean exit");
  } };
}
async function echo() {
  const sockets = new Set(); let received = 0, connections = 0;
  const server = createServer({ allowHalfOpen: true }, socket => {
    connections++; sockets.add(socket); socket.on("close", () => sockets.delete(socket)); socket.on("error", () => {});
    socket.on("data", bytes => { received += bytes.length; }); socket.pipe(socket);
  });
  await new Promise((resolve, reject) => { server.once("error", reject); server.listen(0, "127.0.0.1", resolve); });
  const port = server.address().port;
  return { target: `127.0.0.1:${port}`, get received() { return received; }, get connections() { return connections; }, async close() {
    for (const socket of sockets) socket.destroy();
    await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  } };
}
async function socketAt(address) {
  assert.ok(/^127\.0\.0\.1:\d+$/.test(address), "native bridge must be loopback");
  const socket = createConnection({ host: "127.0.0.1", port: Number(address.split(":")[1]), allowHalfOpen: true });
  socket.on("error", () => {});
  try { await deadline(new Promise((resolve, reject) => { socket.once("connect", resolve); socket.once("error", reject); }), 4_000, "TCP_CONNECT_TIMEOUT"); return socket; }
  catch (error) { socket.destroy(); throw error; }
}
async function tcpRoundtrip(address, length = 1_048_576) {
  const socket = await socketAt(address);
  const payload = Buffer.alloc(length); for (let index = 0; index < payload.length; index++) payload[index] = index % 251;
  let count = 0;
  const done = new Promise((resolve, reject) => {
    socket.on("data", bytes => { if (!bytes.equals(payload.subarray(count, count + bytes.length))) reject(new Error("TCP_ECHO_MISMATCH")); count += bytes.length; });
    socket.once("end", resolve); socket.once("error", reject);
  });
  try { socket.end(payload); await deadline(done, 8_000, "TCP_ECHO_TIMEOUT"); assert.equal(count, payload.length); }
  finally { socket.destroy(); }
}
async function rebind(address) {
  const server = createServer(); const port = Number(address.split(":")[1]);
  await new Promise((resolve, reject) => { server.once("error", reject); server.listen(port, "127.0.0.1", resolve); });
  await new Promise(resolve => server.close(resolve));
}

// The scenarios below exercise the actual HTTP authority and native transport,
// never treating an announcement or a fabricated receipt as an authorization.
test("88C resolved social context, real native TCP and active withdrawal", { skip: !enabled, timeout: 180_000 }, async t => {
  const binary = process.env.AURORA_SOCIAL_ADMISSION_RUST_CLI;
  assert.ok(binary && isAbsolute(binary), "reviewed absolute native binary path required");
  assert.ok((await stat(binary)).isFile());
  const factory = await socialFactory();
  async function scenario(visibility, withdrawal) {
    let lab, host, guest, source, hostChild, guestChild, bridgeAddress, activeSocket, pulse;
    let monitoring = false, monitor = Promise.resolve(), monitorFailure, labStopped = false, primaryError;
    const started = performance.now();
    try {
      lab = await startSocialBindingLab({ enabled: true, port: 0, admission: true });
      assert.ok(/^[0-9a-f]{64}$/.test(lab.admissionPublicKey), "authority pin comes from owned server, never a received grant");
      host = await account(factory); guest = await account(factory); source = await echo();
      if (visibility === "friends" && withdrawal !== "not_friend_context") {
        await host.service.requestFriend(guest.uid);
        await guest.service.respondFriend(host.uid, "accept");
      }
      await host.service.publishSession({ title: "Native social fixture", modpackName: "Local 88C", minecraftVersion: "1.21.1", loader: "Fabric",
        imageUrl: null, players: 1, maxPlayers: 2, visibility, joinPolicy: "open" });
      const sessionId = (await getDocFromServer(doc(host.db, "socialOwners", host.uid))).data().sessionId;
      const post = async (path, who, body) => {
        const signal = AbortSignal.timeout(8_000);
        const token = await deadline(who.token(), 5_000, "AUTH_TOKEN_TIMEOUT"); signal.throwIfAborted();
        const response = await fetch(lab.origin + path, { method: "POST", signal, headers: {
          "Content-Type": "application/json", Authorization: `Bearer ${token}` }, body: JSON.stringify(body) });
        return { status: response.status, body: await response.json() };
      };
      async function challenge(who, native, identity, role) {
        const response = await post("/v1/social/key/challenge", who, { schemaVersion: 1, operationId: identity.operationId, generation: identity.generation,
          role, publicKey: identity.publicKey, sessionId, preparedExpiresAtUnixMs: Math.min(identity.preparedExpiresAtUnixMs, Date.now() + 55_000) });
        assert.equal(response.status, 200, response.body.code ?? "challenge denied");
        assert.ok(response.body.accountId === who.uid && response.body.sessionId === sessionId, "challenge context mismatch");
        native.send(response.body);
        const proof = await native.next();
        assert.equal(proof.schemaVersion, 1, "native did not return a typed proof");
        assert.ok(proof.challenge.publicKey === identity.publicKey, "native changed identity before proof");
        return proof;
      }
      hostChild = await client(binary, ["host", lab.admissionPublicKey, source.target, sessionId, host.uid]);
      const hostIdentity = await hostChild.next(); assert.equal(hostIdentity.event, "social_identity");
      const hostProof = await challenge(host, hostChild, hostIdentity, "host");
      hostChild.send(visibility);
      const review = await hostChild.next(); assert.equal(review.event, "social_host_review");
      assert.equal(review.source, source.target, "review must show original local source");
      assert.ok(review.policy.sessionId === sessionId && review.policy.accountId === host.uid, "host policy context mismatch");
      assert.ok(review.policy.publicKey === hostIdentity.publicKey && review.policy.maxGuests === 1 && review.policy.protocol === "tcp", "host policy not confined");
      // This fixture is the explicit, preauthorized host action for its own echo source.
      hostChild.send(`approve ${review.digest}`);
      const approved = await hostChild.next(); assert.equal(approved.event, "social_host_policy");
      const registration = await post("/v1/social/host/register", host, { proof: hostProof, policySigned: approved.policy });
      assert.equal(registration.status, 200, registration.body.code ?? "host registration denied");
      assert.equal(registration.body.policyDigest, review.digest, "service registered another policy");
      if (withdrawal === "blocked_before_context") await host.service.block(guest.uid);
      // The guest knows only the selected opaque session, its own account, and
      // the authority pin from the owned local server. It must not learn its
      // expected host/policy by inspecting the host process or a later grant.
      const resolution = await post("/v1/social/context", guest, { sessionId });
      if (["blocked_before_context", "not_friend_context"].includes(withdrawal)) {
        assert.equal(resolution.status, 403);
        assert.equal(resolution.body.code, "context_unavailable");
        assert.deepEqual(Object.keys(resolution.body).sort(), ["code"]);
        assert.equal(source.received, 0); assert.equal(source.connections, 0);
        return;
      }
      assert.equal(resolution.status, 200, resolution.body.code ?? "context denied");
      assert.deepEqual(Object.keys(resolution.body).sort(), ["body", "signature"]);
      assert.deepEqual(Object.keys(resolution.body.body).sort(), ["schemaVersion", "purpose", "issuerInstance", "recipientAccountId", "sessionId", "sessionDigest", "hostAccountId", "hostPublicKey", "hostOperationId", "hostGeneration", "policyId", "policyDigest", "issuedAtUnixMs", "expiresAtUnixMs"].sort());
      assert.equal(resolution.body.body.recipientAccountId, guest.uid);
      assert.equal(resolution.body.body.sessionId, sessionId);
      guestChild = await client(binary, ["guest", lab.admissionPublicKey, sessionId, guest.uid]);
      const guestIdentity = await guestChild.next(); assert.equal(guestIdentity.event, "social_identity");
      guestChild.send({ context: resolution.body });
      const guestProof = await challenge(guest, guestChild, guestIdentity, "guest");
      if (withdrawal === "blocked_before_join") await host.service.block(guest.uid);
      const admission = await post("/v1/social/join", guest, { proof: guestProof, policyId: resolution.body.body.policyId });
      if (withdrawal === "blocked_before_join") {
        assert.equal(admission.status, 403, "current block must deny admission");
        assert.equal(source.received, 0, "denied peer reached game source");
        assert.equal(source.connections, 0, "denied peer opened a game connection");
        return;
      }
      assert.equal(admission.status, 200, admission.body.code ?? "admission denied");
      const { grant, lease } = admission.body;
      hostChild.send({ grant, lease });
      const hosting = await hostChild.next(); assert.equal(hosting.event, "social_host_ready");
      assert.equal(typeof hosting.invitation, "string");
      monitoring = true;
      monitor = (async () => {
        while (monitoring) {
          await delay(450); if (!monitoring) break;
          try {
            const current = await post("/v1/social/continuity", guest, { grantId: grant.body.grantId });
            if (current.status !== 200) { monitorFailure = current.body.code ?? "continuity_denied"; break; }
            if (monitoring) hostChild.send({ lease: current.body });
          } catch { monitorFailure = "authority_unavailable"; break; }
        }
      })();
      guestChild.send({ grant, lease, invitation: hosting.invitation });
      const connected = await guestChild.next(); assert.equal(connected.event, "social_guest_ready"); bridgeAddress = connected.tcp;
      await tcpRoundtrip(bridgeAddress);
      assert.ok(source.received >= 1_048_576, "real source did not receive complete payload");
      let postContextExpiryBytesEachDirection = 0;
      if (withdrawal === "active_block") {
        const remaining = resolution.body.body.expiresAtUnixMs - Date.now();
        assert.ok(remaining >= 0 && remaining <= 10_000, "context must have been current at admission");
        await delay(remaining + 100);
        assert.ok(Date.now() >= resolution.body.body.expiresAtUnixMs, "context must now be expired");
        assert.equal(monitorFailure, undefined, "continuity must remain healthy past context expiry");
        postContextExpiryBytesEachDirection = 65_536;
        await tcpRoundtrip(bridgeAddress, postContextExpiryBytesEachDirection);
      }
      activeSocket = await socketAt(bridgeAddress);
      let activeEcho = 0;
      activeSocket.on("data", bytes => { activeEcho += bytes.length; });
      const lostTcp = new Promise(resolve => { activeSocket.once("end", resolve); activeSocket.once("error", resolve); activeSocket.once("close", resolve); });
      const closedActive = new Promise(resolve => activeSocket.once("close", resolve));
      pulse = setInterval(() => { if (!activeSocket.destroyed && activeSocket.writable) activeSocket.write(Buffer.alloc(256, 7)); }, 50);
      await delay(220); assert.ok(activeEcho > 0, "active stream was not healthy before withdrawal");
      const withdrawalAt = performance.now();
      if (withdrawal === "active_block") await host.service.block(guest.uid);
      else if (withdrawal === "backend_lost") { await lab.close(); labStopped = true; }
      else throw new Error("UNKNOWN_WITHDRAWAL_CASE");
      await deadline(lostTcp, 7_000, "ACTIVE_TCP_NOT_WITHDRAWN");
      const withdrawalMs = Math.round(performance.now() - withdrawalAt);
      // The native side has already withdrawn the stream. Finish the fixture's
      // half-open TCP endpoint now, before asking whether its peer port can be
      // rebound; retaining it until finally creates a circular cleanup wait.
      clearInterval(pulse); pulse = undefined;
      activeSocket.destroy(); await deadline(closedActive, 2_000, "ACTIVE_CLIENT_NOT_CLOSED"); activeSocket = undefined;
      monitoring = false; await deadline(monitor, 9_000, "CONTINUITY_WORKER_NOT_STOPPED");
      assert.equal(monitorFailure, withdrawal === "active_block" ? "session_not_authorized" : "authority_unavailable", "withdrawal must have the expected cause");
      assert.ok(Date.now() < grant.body.sessionExpiresAtUnixMs - 5_000, "ordinary session expiry cannot stand in for active withdrawal");
      const hostStopped = await hostChild.next(); assert.equal(hostStopped.event, "social_host_stopped");
      assert.equal(hostStopped.reason, "continuity_expired");
      assert.equal(hostStopped.admitted, true, "host must have observed real admission");
      assert.equal(hostStopped.cleanup, true); assert.equal(hostStopped.rebound, true);
      const guestStopped = await guestChild.next(); assert.equal(guestStopped.event, "social_guest_stopped");
      assert.equal(guestStopped.reason, "connection_closed");
      assert.equal(guestStopped.cleanup, true, JSON.stringify(guestStopped)); assert.equal(guestStopped.rebound, true);
      assert.equal((await hostChild.closed()).code, 0); assert.equal((await guestChild.closed()).code, 0);
      await rebind(bridgeAddress);
      t.diagnostic(JSON.stringify({ event: "social_admission_observed", visibility, withdrawal, contextResolvedBySession: true, tcpBytesEachDirection: 1_048_576,
        postContextExpiryBytesEachDirection, withdrawalMs, elapsedMs: Math.round(performance.now() - started), realNativeProcesses: 2, relay: false, minecraft: "not_tested", voice: "not_tested" }));
    } catch (error) { primaryError = error; throw error; }
    finally {
      monitoring = false; clearInterval(pulse); activeSocket?.destroy();
      const failures = [];
      for (const result of await Promise.allSettled([deadline(monitor, 10_000, "MONITOR_CLEANUP_TIMEOUT"), hostChild?.close(), guestChild?.close()])) if (result.status === "rejected") failures.push(result.reason);
      try { await deadline(host?.service.closeSession(), 16_000, "SESSION_CLEANUP_TIMEOUT"); } catch (error) { failures.push(error); }
      for (const result of await Promise.allSettled([source?.close(), host?.close(), guest?.close(), labStopped ? undefined : lab?.close()].map(work => deadline(work, 10_000, "FIXTURE_CLEANUP_TIMEOUT")))) if (result.status === "rejected") failures.push(result.reason);
      if (failures.length) throw new AggregateError(primaryError ? [primaryError, ...failures] : failures, "SOCIAL_ADMISSION_CLEANUP_FAILED");
    }
  }
  await t.test("public session: native TCP and block withdraw active stream", () => scenario("public", "active_block"));
  await t.test("friends session: real friendship, native TCP and backend loss fails closed", () => scenario("friends", "backend_lost"));
  await t.test("block after key proof before join prevents reaching game source", () => scenario("public", "blocked_before_join"));
  await t.test("block before context hides policy and never reaches game source", () => scenario("public", "blocked_before_context"));
  await t.test("nonfriend cannot resolve a friends-only host context", () => scenario("friends", "not_friend_context"));
});
