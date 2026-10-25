import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { isAbsolute } from "node:path";
import { stat } from "node:fs/promises";
import { createInterface } from "node:readline";
import { request as httpRequest } from "node:http";
import { test } from "node:test";
import { initializeApp, deleteApp } from "firebase/app";
import { connectAuthEmulator, createUserWithEmailAndPassword, getAuth, signOut } from "firebase/auth";
import { connectFirestoreEmulator, doc, getFirestore, serverTimestamp, terminate, Timestamp, writeBatch } from "firebase/firestore";
import { startSocialBindingLab } from "./social-binding-server.mjs";

const enabled = process.env.AURORA_SOCIAL_BINDING_INTEGRATION === "1";
const projectId = "demo-aurora-social";
const fixturePassword = "aurora-binding-local-fixture"; // Disposable emulator account, never a real password.
async function account({ registered = true } = {}) {
  const nickname = `bind_${crypto.randomUUID().replaceAll("-", "").slice(0, 10)}`;
  const app = initializeApp({ projectId, apiKey: projectId }, nickname);
  const auth = getAuth(app); connectAuthEmulator(auth, "http://127.0.0.1:9187", { disableWarnings: true });
  const db = getFirestore(app); connectFirestoreEmulator(db, "127.0.0.1", 8187);
  const email = `${nickname}@aurora.internal`;
  try {
    const user = (await createUserWithEmailAndPassword(auth, email, fixturePassword)).user;
    const uid = user.uid;
    if (registered) {
      const batch = writeBatch(db);
      batch.set(doc(db, "usernames", nickname), { uid, usernameLower: nickname, createdAt: serverTimestamp() });
      batch.set(doc(db, "users", uid), { uid, username: nickname, usernameLower: nickname, email, avatarUrl: null, skinUrl: null, capeUrl: null,
        skinModel: "classic", role: "PLAYER", createdAt: serverTimestamp(), stats: { totalPlayTimeSeconds: 0, questsCompleted: 0, lastPlayedModpack: null } });
      await batch.commit();
    }
    return { uid, db, token: () => user.getIdToken(), async close() { await signOut(auth); await terminate(db); await deleteApp(app); } };
  } catch (error) { await terminate(db).catch(() => {}); await deleteApp(app); throw error; }
}
async function publication(host) {
  const id = crypto.randomUUID();
  const value = { schema: 1, hostUid: host.uid, title: "Binding local", modpackName: "Fixture 88A", minecraftVersion: "1.21.1", loader: "Fabric",
    imageUrl: null, players: 1, maxPlayers: 2, visibility: "public", joinPolicy: "open", expiresAt: Timestamp.fromMillis(Date.now() + 300_000) };
  async function write() {
    const batch = writeBatch(host.db);
    batch.set(doc(host.db, "socialOwners", host.uid), { sessionId: id, visibility: value.visibility });
    batch.set(doc(host.db, "socialSessions", id), value);
    if (value.visibility === "public") batch.set(doc(host.db, "socialCatalog", id), { schema: 1, expiresAt: value.expiresAt });
    else batch.delete(doc(host.db, "socialCatalog", id));
    await batch.commit();
  }
  await write();
  return { id, value, write, async close() { const batch = writeBatch(host.db); for (const [name, key] of [["socialOwners", host.uid], ["socialSessions", id], ["socialCatalog", id]]) batch.delete(doc(host.db, name, key)); await batch.commit(); } };
}
async function nativeSigner(binary) {
  const child = spawn(binary, [], { windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
  let resolveExit; const exit = new Promise(resolve => { resolveExit = resolve; });
  let failure, ended = false, bytes = 0, stderrBytes = 0;
  const queue = [], waiters = [];
  const lines = createInterface({ input: child.stdout, crlfDelay: Infinity });
  const rejectWaiters = error => { failure = error; for (const waiter of waiters.splice(0)) waiter.reject(error); };
  child.once("error", () => rejectWaiters(new Error("RUST_KEY_PROOF_START_FAILED")));
  child.once("exit", (code, signal) => { ended = true; resolveExit({ code, signal }); });
  child.stdout.on("data", chunk => { bytes += chunk.length; if (bytes > 8_192) { rejectWaiters(new Error("RUST_KEY_PROOF_OUTPUT_LIMIT")); child.kill(); } });
  child.stderr.on("data", chunk => { stderrBytes += chunk.length; if (stderrBytes > 4_096) { rejectWaiters(new Error("RUST_KEY_PROOF_STDERR_LIMIT")); child.kill(); } });
  lines.on("line", line => { const waiter = waiters.shift(); if (waiter) waiter.resolve(line); else queue.push(line); });
  lines.once("close", () => { if (!queue.length) rejectWaiters(new Error("RUST_KEY_PROOF_EOF")); });
  const next = () => new Promise((resolve, reject) => {
    if (queue.length) { resolve(queue.shift()); return; }
    if (failure) { reject(failure); return; }
    const timer = setTimeout(() => { reject(new Error("RUST_KEY_PROOF_TIMEOUT")); child.kill(); }, 15_000);
    waiters.push({ resolve: value => { clearTimeout(timer); resolve(value); }, reject: error => { clearTimeout(timer); reject(error); } });
  });
  const boundedExit = async () => {
    let timer;
    try { return await Promise.race([exit, new Promise((_, reject) => { timer = setTimeout(() => { if (!ended) child.kill(); reject(new Error("RUST_KEY_PROOF_REAP_TIMEOUT")); }, 5_000); })]); }
    finally { clearTimeout(timer); }
  };
  const close = async () => { child.stdin.end(); if (!ended) child.kill(); try { await boundedExit(); } finally { lines.close(); } };
  try {
    const identity = JSON.parse(await next());
    assert.equal(identity.event, "key_proof_identity"); assert.match(identity.publicKey, /^[0-9a-f]{64}$/);
    return { publicKey: identity.publicKey, close, async sign(challenge) {
      child.stdin.end(JSON.stringify(challenge) + "\n");
      const proof = JSON.parse(await next()); assert.equal(proof.schemaVersion, 1); assert.match(proof.signature, /^[0-9a-f]{128}$/);
      const result = await boundedExit(); assert.equal(result.code, 0); assert.equal(result.signal, null);
      assert.deepEqual(proof.challenge, challenge); return proof;
    } };
  } catch (error) { await close(); throw error; }
}

test("88A real Auth/Firestore REST, Rust Ed25519 CLI and one-use account/key proof; NO ingress", { skip: !enabled, timeout: 120_000 }, async t => {
  const binary = process.env.AURORA_SOCIAL_BINDING_RUST_CLI;
  assert.ok(binary && isAbsolute(binary), "AURORA_SOCIAL_BINDING_RUST_CLI must be an absolute reviewed binary path");
  assert.ok((await stat(binary)).isFile());
  let lab, host, guest, orphan, session;
  try {
    lab = await startSocialBindingLab({ enabled: true, port: 0 }); host = await account(); guest = await account(); orphan = await account({ registered: false }); session = await publication(host);
    const post = async (path, who, value) => {
      const response = await fetch(lab.origin + `/v1/social/key/${path}`, { method: "POST", signal: AbortSignal.timeout(12_000), headers: { "Content-Type": "application/json", Authorization: `Bearer ${await who.token()}` }, body: JSON.stringify(value) });
      return { status: response.status, body: await response.json() };
    };
    const withProof = async (who, role, run) => {
      const signer = await nativeSigner(binary);
      try {
        const input = { schemaVersion: 1, operationId: crypto.randomUUID(), generation: 1, role, publicKey: signer.publicKey, sessionId: session.id, preparedExpiresAtUnixMs: Date.now() + 55_000 };
        const result = await post("challenge", who, input); assert.equal(result.status, 200, result.body.code);
        assert.equal(result.body.accountId, who.uid);
        const proof = await signer.sign(result.body);
        await run(proof);
      } finally { await signer.close(); }
    };
    await t.test("real Auth account without Aurora profile and nick registration cannot obtain challenge", async () => {
      const result = await post("challenge", orphan, { schemaVersion: 1, operationId: crypto.randomUUID(), generation: 1, role: "guest", publicKey: "a".repeat(64), sessionId: session.id, preparedExpiresAtUnixMs: Date.now() + 55_000 });
      assert.equal(result.status, 403); assert.equal(result.body.code, "session_not_authorized");
    });
    await t.test("host and guest proofs use actual caller registration and native keys", async () => {
      for (const [who, role] of [[host, "host"], [guest, "guest"]]) await withProof(who, role, async proof => {
        const result = await post("proof", who, proof); assert.equal(result.status, 200, result.body.code);
        assert.equal(result.body.accountId, who.uid); assert.equal(result.body.publicKey, proof.challenge.publicKey);
        assert.equal(result.body.operationId, proof.challenge.operationId); assert.equal(result.body.generation, proof.challenge.generation);
        assert.equal(result.body.transportAuthorized, false); assert.equal(result.body.expiresAtUnixMs, proof.challenge.expiresAtUnixMs);
        const replay = await post("proof", who, proof); assert.equal(replay.body.code, "challenge_unavailable");
      });
    });
    await t.test("proof from another Firebase account is rejected", async () => {
      await withProof(guest, "guest", async proof => { const result = await post("proof", host, proof); assert.equal(result.body.code, "account_mismatch"); });
    });
    await t.test("session mutation after proof challenge rejects receipt", async () => {
      await withProof(guest, "guest", async proof => { session.value.title = "Changed"; await session.write();
        const result = await post("proof", guest, proof); assert.equal(result.body.code, "session_changed"); });
    });
    await t.test("public to private change is denied by current Firestore rules", async () => {
      await withProof(guest, "guest", async proof => { session.value.visibility = "private"; await session.write();
        const result = await post("proof", guest, proof); assert.equal(result.status, 403); assert.equal(result.body.code, "session_not_authorized"); });
      session.value.visibility = "public"; await session.write();
    });
    await t.test("block after challenge is denied without reading another user's private graph", async () => {
      await withProof(guest, "guest", async proof => {
        // The block removes reciprocal relationships atomically, as required by the existing rules.
        const batch = writeBatch(host.db);
        batch.set(doc(host.db, "socialState", host.uid), { friends: [], incoming: [], outgoing: [], blocked: [guest.uid], revision: 1,
          kind: "block", actor: host.uid, peer: guest.uid, updatedAt: serverTimestamp() });
        batch.set(doc(host.db, "socialState", guest.uid), { friends: [], incoming: [], outgoing: [], blocked: [], revision: 1,
          kind: "block", actor: host.uid, peer: host.uid, updatedAt: serverTimestamp() });
        await batch.commit();
        const result = await post("proof", guest, proof); assert.equal(result.status, 403); assert.equal(result.body.code, "session_not_authorized");
      });
    });
    await t.test("HTTP rejects missing authentication, origins, unknown paths and oversized body", async () => {
      const body = { schemaVersion: 1, operationId: crypto.randomUUID(), generation: 1, role: "guest", publicKey: "a".repeat(64), sessionId: session.id, preparedExpiresAtUnixMs: Date.now() + 55_000 };
      const missing = await fetch(lab.origin + "/v1/social/key/challenge", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
      assert.equal(missing.status, 401);
      const crossOrigin = await fetch(lab.origin + "/v1/social/key/challenge", { method: "POST", headers: { "Content-Type": "application/json", Origin: "https://evil.test" }, body: "{}" });
      assert.equal(crossOrigin.status, 400); assert.equal(crossOrigin.headers.get("access-control-allow-origin"), null);
      const excessive = await fetch(lab.origin + "/v1/social/key/challenge", { method: "POST", headers: { "Content-Type": "application/json" }, body: " ".repeat(4_097) }); assert.equal(excessive.status, 413);
      const unknown = await fetch(lab.origin + "/unknown", { method: "POST" }); assert.equal(unknown.status, 400);
      const changedHost = await new Promise((resolve, reject) => {
        const req = httpRequest(lab.origin + "/v1/social/key/challenge", { method: "POST", headers: { Host: "evil.test", "Content-Type": "application/json" }, timeout: 5_000 }, res => {
          res.resume(); res.once("end", () => resolve(res.statusCode));
        });
        req.once("error", reject); req.once("timeout", () => req.destroy(new Error("HOST_BOUNDARY_TIMEOUT"))); req.end("{}");
      });
      assert.equal(changedHost, 400);
    });
    assert.equal(lab.inspect().transportAuthorized, false);
  } finally {
    const failures = [];
    try { await session?.close(); } catch (error) { failures.push(error); }
    for (const result of await Promise.allSettled([host?.close(), guest?.close(), orphan?.close(), lab?.close()])) if (result.status === "rejected") failures.push(result.reason);
    if (failures.length) throw new AggregateError(failures, "SOCIAL_BINDING_FIXTURE_CLEANUP_FAILED");
  }
});
