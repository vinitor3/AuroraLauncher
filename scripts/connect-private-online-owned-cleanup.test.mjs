import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtemp, mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createCleanupOwner, prepareOwnedRecovery } from "./connect-private-online-owned-cleanup.mjs";

const runId = "11111111-2222-4333-8444-555555555555", sid = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const operator = "operator@example.invalid";
const now = 1_800_000_000_000, created = new Date(now).toISOString(), updated = new Date(now + 1).toISOString(), expiry = new Date(now + 60_000).toISOString();
const accountIds = { host: "HOST_CREATED_UID_123456789", guest: "GUEST_CREATED_UID_12345678" };
const encode = value => value === null ? { nullValue: null } : typeof value === "number" ? { integerValue: String(value) }
  : typeof value === "string" ? { [/^20\d\d-/.test(value) ? "timestampValue" : "stringValue"]: value }
    : Array.isArray(value) ? { arrayValue: { values: value.map(encode) } } : { mapValue: { fields: Object.fromEntries(Object.entries(value).map(([key, item]) => [key, encode(item)])) } };
const doc = (path, value) => ({ name: `projects/auroralauncher/databases/(default)/documents/${path}`, createTime: created, updateTime: updated,
  fields: Object.fromEntries(Object.entries(value).map(([key, item]) => [key, encode(item)])) });
async function fixture(options = {}) {
  const root = await mkdtemp(join(tmpdir(), "aurora-owned-cleanup-")), directory = join(root, "run"); await mkdir(directory);
  const calls = [], docs = new Map(), users = new Map();
  const cache = { user: { email: options.email ?? operator }, tokens: { access_token: "ACCESS_SECRET_SENTINEL", refresh_token: "REFRESH_SECRET_SENTINEL", expires_at: options.expired ? 0 : now + 3_600_000 } };
  const fetcher = async (url, init) => {
    assert.equal(init.redirect, "error"); assert.ok(init.signal); calls.push({ url, init });
    let data;
    if (url === "https://oauth2.googleapis.com/token") {
      if (options.refreshError) return new Response('{"error":"RAW_SECRET_ERROR"}', { status: 400 });
      data = { access_token: "ACCESS_SECRET_SENTINEL", expires_in: 3600 };
    } else {
      assert.equal(init.headers.Authorization, "Bearer ACCESS_SECRET_SENTINEL");
      if (url === "https://openidconnect.googleapis.com/v1/userinfo" && options.rejectCached && !calls.some(c => c.url === "https://oauth2.googleapis.com/token"))
        return new Response("{}", { status: 401, headers: { date: new Date(now).toUTCString() } });
      if (url === "https://openidconnect.googleapis.com/v1/userinfo") data = { email: options.serverEmail ?? cache.user.email, email_verified: true };
      else if (url.endsWith(":testIamPermissions")) data = { permissions: options.noDeletePermission ? ["datastore.entities.get"]
        : JSON.parse(init.body).permissions.filter(permission => !options.noAuthDeletePermission || permission !== "firebaseauth.users.delete") };
      else if (url.endsWith("accounts:lookup")) {
        const query = JSON.parse(init.body); data = { users: [...users.values()].filter(value => query.localId?.includes(value.localId) || query.email?.includes(value.email)) };
      } else if (url.endsWith("accounts:delete")) {
        const query = JSON.parse(init.body); assert.deepEqual(Object.keys(query), ["localId"]); assert.ok(users.has(query.localId)); users.delete(query.localId); data = {};
      } else {
        const parsed = new URL(url), path = parsed.pathname.split("/documents/")[1];
        assert.ok(path); assert.equal(parsed.origin, "https://firestore.googleapis.com");
        if (init.method === "DELETE") {
          if (options.beforeDelete) options.beforeDelete(docs, path);
          if (!docs.has(path) || docs.get(path).updateTime !== parsed.searchParams.get("currentDocument.updateTime")) return new Response("{}", { status: 412, headers: { date: new Date(now).toUTCString() } });
          docs.delete(path); data = {};
        } else { if (!docs.has(path)) return new Response("{}", { status: 404, headers: { date: new Date(now).toUTCString() } }); data = docs.get(path); }
      }
    }
    return Response.json(data, { headers: { date: new Date(now).toUTCString() } });
  };
  const dependencies = { evidenceRoot: root, fetch: fetcher, readCache: async () => cache, now: () => now };
  const owner = await createCleanupOwner({ runId, rootDirectory: directory, expectedOperator: operator }, dependencies);
  async function register(role) {
    const plan = owner.accounts.find(item => item.role === role), uid = accountIds[role];
    await owner.recordAccountCreateStarted(role);
    users.set(uid, { localId: uid, email: plan.email, createdAt: String(now) });
    await owner.recordAccount({ role, uid, email: plan.email, createdAt: now });
    docs.set(`users/${uid}`, doc(`users/${uid}`, { uid, username: plan.nickname, usernameLower: plan.nickname, email: plan.email, avatarUrl: null, skinUrl: null, capeUrl: null,
      skinModel: "classic", role: "PLAYER", createdAt: created, stats: { totalPlayTimeSeconds: 0, questsCompleted: 0, lastPlayedModpack: null } }));
    docs.set(`publicProfiles/${uid}`, doc(`publicProfiles/${uid}`, { uid, username: plan.nickname, avatarUrl: null, skinUrl: null, skinModel: "classic", updatedAt: created }));
    docs.set(`usernames/${plan.nickname}`, doc(`usernames/${plan.nickname}`, { uid, usernameLower: plan.nickname, createdAt: created }));
    await owner.snapshotDocuments();
  }
  async function publish() {
    const host = accountIds.host;
    docs.set(`socialOwners/${host}`, doc(`socialOwners/${host}`, { sessionId: sid, visibility: "private" }));
    docs.set(`socialSessions/${sid}`, doc(`socialSessions/${sid}`, { ...owner.sessionDraft, schema: 1, hostUid: host, expiresAt: expiry }));
    await owner.recordSession(sid);
  }
  async function invite() {
    const host = accountIds.host, guest = accountIds.guest;
    docs.set(`socialPrivateAudiences/${sid}`, doc(`socialPrivateAudiences/${sid}`, { schema: 1, sessionId: sid, hostUid: host, expiresAt: expiry, recipients: [guest] }));
    docs.set(`socialPrivateInvitations/${sid}/guests/${guest}`, doc(`socialPrivateInvitations/${sid}/guests/${guest}`, { schema: 1, sessionId: sid, hostUid: host, recipientUid: guest,
      authorizationId: "a".repeat(64), revision: 1, status: "active", createdAt: created, updatedAt: created, expiresAt: expiry }));
    docs.set(`socialPrivateInbox/${guest}/sessions/${sid}`, doc(`socialPrivateInbox/${guest}/sessions/${sid}`, { schema: 1, sessionId: sid, hostUid: host, expiresAt: expiry }));
    docs.set(`socialPrivateThrottle/${host}`, doc(`socialPrivateThrottle/${host}`, { at: created }));
    await owner.snapshotDocuments();
  }
  return { owner, docs, users, calls, cache, directory, register, publish, invite, dependencies };
}
const settled = { sessionStopped: true, pendingMutationsSettled: true };
test("only the two new accounts and their exact private session are removed with CAS and secret-free receipts", async () => {
  const f = await fixture({ expired: true }); await f.owner.preflight();
  for (const plan of f.owner.accounts) assert.match(plan.nickname, /^cp[0-9a-f]{12}[hg]$/);
  await f.register("host"); await f.register("guest"); await f.publish(); await f.invite();
  f.docs.set("users/PERSONAL_TEST_USER", doc("users/PERSONAL_TEST_USER", { secret: "UNRELATED" }));
  const result = await f.owner.cleanupDocuments(settled);
  assert.equal(result.removedDocuments, 12); assert.equal(f.docs.size, 1); assert.ok(f.docs.has("users/PERSONAL_TEST_USER"));
  assert.equal(f.calls.filter(call => call.init.method === "DELETE").length, 12);
  assert.equal(f.calls.filter(call => call.url.includes("accounts:delete")).length, 0);
  assert.equal(f.owner.result().complete, false); await f.owner.recordAuthDeleted("host"); await f.owner.recordAuthDeleted("guest");
  assert.equal(f.owner.result().complete, true);
  for (const file of await readdir(f.owner.directory)) assert.doesNotMatch(await readFile(join(f.owner.directory, file), "utf8"), /ACCESS_SECRET|REFRESH_SECRET|UNRELATED/);
  assert.equal(f.cache.tokens.expires_at, 0, "token refresh never changes the source cache");
});
test("preflight renews rejected cached access in memory and only repeats the identity read", async () => {
  const f = await fixture({ rejectCached: true });
  await f.owner.preflight();
  assert.deepEqual(f.calls.slice(0, 3).map(c => new URL(c.url).pathname), ["/v1/userinfo", "/token", "/v1/userinfo"]);
  assert.equal(f.calls.filter(c => c.url.endsWith(":testIamPermissions")).length, 1);
  assert.equal(f.cache.tokens.expires_at, now + 3_600_000);
});
test("operator, scope permission and refresh errors fail before any destructive request", async () => {
  for (const options of [{ email: "someone@invalid.test" }, { serverEmail: "someone@invalid.test" }, { noDeletePermission: true }, { noAuthDeletePermission: true }, { expired: true, refreshError: true }]) {
    const f = await fixture(options); await assert.rejects(f.owner.preflight(), cause => /^owned_cleanup_/.test(cause.code) && !cause.message.includes("RAW_SECRET"));
    assert.equal(f.calls.filter(call => call.init.method === "DELETE").length, 0);
    await assert.rejects(f.owner.recordAuthDeleted("host"));
  }
});
test("cleanup requires settled native and SDK lifecycle and an unchanged journal", async () => {
  const f = await fixture(); await f.owner.preflight(); await f.register("host");
  await assert.rejects(f.owner.cleanupDocuments(), /not_settled/);
  await writeFile(join(f.owner.directory, "000.json"), "tampered");
  await assert.rejects(f.owner.cleanupDocuments(settled), /journal_changed/);
  assert.equal(f.calls.filter(call => call.init.method === "DELETE").length, 0);
});
test("a changed document aborts all deletion, including changes arriving after snapshot", async () => {
  const f = await fixture(); await f.owner.preflight(); await f.register("host");
  f.docs.get(`users/${accountIds.host}`).updateTime = new Date(now + 2).toISOString();
  await assert.rejects(f.owner.cleanupDocuments(settled), /document_changed/);
  assert.equal(f.calls.filter(call => call.init.method === "DELETE").length, 0);
});
test("conditional delete refuses a change between read and write", async () => {
  const f = await fixture({ beforeDelete: (docs, path) => { docs.get(path).updateTime = new Date(now + 2).toISOString(); } });
  await f.owner.preflight(); await f.register("host");
  await assert.rejects(f.owner.cleanupDocuments(settled), /http_412/);
  assert.equal(f.docs.size, 3); assert.equal(f.owner.result().documentsCleaned, false);
});
test("legal revocation and SDK close are recorded before residual cleanup, while unrelated field changes are refused", async () => {
  const f = await fixture(); await f.owner.preflight(); await f.register("host"); await f.register("guest"); await f.publish(); await f.invite();
  const invitation = f.docs.get(`socialPrivateInvitations/${sid}/guests/${accountIds.guest}`);
  invitation.fields.status = encode("revoked"); invitation.fields.revision = encode(2); invitation.fields.updatedAt = encode(updated); invitation.updateTime = new Date(now + 2).toISOString();
  f.docs.delete(`socialPrivateInbox/${accountIds.guest}/sessions/${sid}`); f.docs.delete(`socialOwners/${accountIds.host}`); f.docs.delete(`socialSessions/${sid}`);
  await f.owner.snapshotDocuments();
  const result = await f.owner.cleanupDocuments(settled); assert.equal(result.removedDocuments, 9);
  const g = await fixture(); await g.owner.preflight(); await g.register("host");
  g.docs.get(`publicProfiles/${accountIds.host}`).fields.avatarUrl = encode("https://unrelated.invalid/personal.png");
  await assert.rejects(g.owner.snapshotDocuments(), /shape_invalid/);
});
test("a personal identity or existing registration cannot be adopted into the ownership journal", async () => {
  const f = await fixture(); await f.owner.preflight();
  await f.owner.recordAccountCreateStarted("host");
  await assert.rejects(f.owner.recordAccount({ role: "host", uid: accountIds.host, email: "teste@aurora.internal", createdAt: now }), /account_invalid/);
  assert.equal(f.owner.result().accountsCreated, 0);
  await assert.rejects(f.owner.cleanupDocuments(settled), /account_unconfirmed/);
  assert.equal(f.owner.result().complete, false);
});
test("second-precision SDK metadata retains the exact newly-created server timestamp and rejects old accounts", async () => {
  for (const offset of [987, -86_400_000]) {
    const f = await fixture(); await f.owner.preflight(); const plan = f.owner.accounts[0], uid = accountIds.host, actual = now + offset;
    await f.owner.recordAccountCreateStarted("host");
    f.users.set(uid, { localId: uid, email: plan.email, createdAt: String(actual) });
    const result = f.owner.recordAccount({ role: "host", uid, email: plan.email, createdAt: new Date(actual).toUTCString() });
    if (offset < 0) { await assert.rejects(result, /account_invalid/); assert.equal(f.owner.result().accountsCreated, 0); }
    else { await result; const receipt = JSON.parse(await readFile(join(f.owner.directory, "004.json"), "utf8")); assert.equal(receipt.createdAt, actual); }
  }
});
test("crash recovery accepts only the hash-approved owned journal, reviews first and retains CAS before UID-scoped Auth deletion", async () => {
  const f = await fixture(); await f.owner.preflight(); await f.register("host"); await f.register("guest"); await f.publish(); await f.invite();
  const options = { rootDirectory: f.directory, journalHash: f.owner.result().journalHash, expectedOperator: operator };
  await writeFile(join(f.owner.directory, "result.json"), JSON.stringify({ passed: false, arbitraryUid: "UNRELATED_PERSONAL_USER" }));
  await assert.rejects(prepareOwnedRecovery({ ...options, journalHash: "0".repeat(64) }, f.dependencies), /journal_changed/);
  const recovery = await prepareOwnedRecovery(options, f.dependencies);
  assert.equal(recovery.plan.authDeleteIsConditional, false); assert.equal(recovery.plan.accounts.length, 2);
  assert.equal(f.calls.filter(call => call.init.method === "DELETE" || call.url.endsWith("accounts:delete")).length, 0);
  await assert.rejects(recovery.execute({ planHash: "0".repeat(64), ...settled }), /recovery_plan_changed/);
  const result = await recovery.execute({ planHash: recovery.planHash, ...settled });
  assert.equal(result.complete, true); assert.equal(f.users.size, 0); assert.equal(f.docs.size, 0);
});
test("an uncertain signup without a UID stays incomplete and cannot be adopted automatically by recovery", async () => {
  const f = await fixture(); await f.owner.preflight(); await f.owner.recordAccountCreateStarted("host");
  await assert.rejects(f.owner.cleanupDocuments(settled), /account_unconfirmed/); assert.equal(f.owner.result().complete, false);
  await assert.rejects(prepareOwnedRecovery({ rootDirectory: f.directory, journalHash: f.owner.result().journalHash, expectedOperator: operator }, f.dependencies), /unknown_created_account/);
});
