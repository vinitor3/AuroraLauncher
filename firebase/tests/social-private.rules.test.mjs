import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { before, beforeEach, after, test } from "node:test";
import { assertFails, assertSucceeds, initializeTestEnvironment } from "@firebase/rules-unit-testing";
import { collection, doc, documentId, getDoc, getDocs, limit, orderBy, query, setDoc, Timestamp, serverTimestamp, updateDoc, deleteDoc, where, writeBatch } from "firebase/firestore";

let env, createService;
const projectId = "demo-aurora-social-rules-v2";
before(async () => {
  const ts = createRequire(new URL("../../apps/desktop/package.json", import.meta.url))("typescript");
  const compile = source => ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  const encoded = source => `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
  const dependencies = {};
  for (const [specifier, file] of [["../shared/social", "shared/social.ts"], ["./socialServerClock", "lib/socialServerClock.ts"],
    ["./socialOrganization", "lib/socialOrganization.ts"], ["./socialPrivateInvitations", "lib/socialPrivateInvitations.ts"]]) {
    dependencies[specifier] = encoded(compile(await readFile(new URL(`../../apps/desktop/src/${file}`, import.meta.url), "utf8"))
      .replace('"firebase/firestore"', JSON.stringify(import.meta.resolve("firebase/firestore"))));
  }
  let sdk = compile(await readFile(new URL("../../apps/desktop/src/lib/socialFirebase.ts", import.meta.url), "utf8"))
    .replace('"firebase/firestore"', JSON.stringify(import.meta.resolve("firebase/firestore")))
    .replace('"./firebase"', JSON.stringify("data:text/javascript,export const socialLabBuild=true;export const privateSocialOnlineConfiguration=()=>false"));
  for (const [specifier, module] of Object.entries(dependencies)) sdk = sdk.replace(JSON.stringify(specifier), JSON.stringify(module));
  createService = (await import(encoded(sdk))).createSocialFirebaseService;
  env = await initializeTestEnvironment({ projectId, firestore: { host: "127.0.0.1", port: 8187,
    rules: await readFile(new URL("../firestore.rules", import.meta.url), "utf8") } });
});
beforeEach(async () => {
  await env.clearFirestore();
  await env.withSecurityRulesDisabled(async context => {
    const db = context.firestore();
    for (const uid of ["host", "guest", "other", "guest2", "guest3", "guest4", "guest5", "guest6", "guest7", "guest8"])
      await Promise.all([
        setDoc(doc(db, "users", uid), { uid, username: uid, usernameLower: uid }),
        setDoc(doc(db, "usernames", uid), { uid, usernameLower: uid }),
        setDoc(doc(db, "publicProfiles", uid), { uid, username: uid, avatarUrl: null }),
      ]);
  });
});
after(async () => { await env?.cleanup(); });
const dbFor = uid => env.authenticatedContext(uid).firestore();
function services(uid, project = projectId) {
  const claims = { sub: uid, user_id: uid, aud: project, iss: `https://securetoken.google.com/${project}`, iat: Math.floor(Date.now() / 1000),
    exp: Math.floor(Date.now() / 1000) + 3600, firebase: { identities: {}, sign_in_provider: "custom" } };
  const token = `${Buffer.from('{"alg":"none","typ":"JWT"}').toString("base64url")}.${Buffer.from(JSON.stringify(claims)).toString("base64url")}.`;
  return { db: dbFor(uid), config: { projectId: project }, authScope: { id: `private-test-${uid}`, persistent: false },
    auth: { currentUser: { uid, getIdToken: async () => token }, onAuthStateChanged: () => () => {} } };
}
const invitation = (db, id, uid = "guest") => doc(db, "socialPrivateInvitations", id, "guests", uid);
const inbox = (db, id, uid = "guest") => doc(db, "socialPrivateInbox", uid, "sessions", id);
const audience = (db, id) => doc(db, "socialPrivateAudiences", id);
async function publish(visibility = "private", joinPolicy = "open") {
  const id = crypto.randomUUID(), db = dbFor("host"), expiresAt = Timestamp.fromMillis(Date.now() + 90_000);
  const data = { schema: 1, hostUid: "host", title: "Private world", modpackName: "Pack", minecraftVersion: "1.21.1", loader: "Fabric",
    imageUrl: null, players: 0, maxPlayers: 8, visibility, joinPolicy, expiresAt };
  const batch = writeBatch(db);
  batch.set(doc(db, "socialOwners", "host"), { sessionId: id, visibility });
  batch.set(doc(db, "socialSessions", id), data);
  if (visibility === "public") batch.set(doc(db, "socialCatalog", id), { schema: 1, expiresAt });
  await batch.commit(); return { id, data };
}
async function cooldown() {
  // Test fixture only: independent cases do not wait two seconds between send operations.
  await env.withSecurityRulesDisabled(context => deleteDoc(doc(context.firestore(), "socialPrivateThrottle", "host")));
}
async function send(scope, uid = "guest", { actor = "host", omit = [], modify = {}, recipients } = {}) {
  const db = dbFor(actor), batch = writeBatch(db);
  let previous;
  if (!recipients) previous = (await getDoc(audience(db, scope.id))).data();
  const auth = { schema: 1, authorizationId: crypto.randomUUID().replaceAll("-", "").repeat(2), revision: 1,
    sessionId: scope.id, hostUid: "host", recipientUid: uid, status: "active", createdAt: serverTimestamp(), updatedAt: serverTimestamp(), expiresAt: scope.data.expiresAt, ...modify };
  if (!omit.includes("audience")) batch.set(audience(db, scope.id), { schema: 1, hostUid: "host", sessionId: scope.id,
    expiresAt: scope.data.expiresAt, recipients: recipients ?? [...(previous?.recipients ?? []), uid] });
  if (!omit.includes("auth")) batch.set(invitation(db, scope.id, uid), auth);
  if (!omit.includes("inbox")) batch.set(inbox(db, scope.id, uid), { schema: 1, sessionId: scope.id, hostUid: "host", expiresAt: scope.data.expiresAt });
  if (!omit.includes("throttle")) batch.set(doc(db, "socialPrivateThrottle", "host"), { at: serverTimestamp() });
  await batch.commit();
}
async function withdraw(scope, uid = "guest", actor = "host") {
  const db = dbFor(actor), old = (await getDoc(invitation(db, scope.id, uid))).data(), batch = writeBatch(db);
  batch.update(invitation(db, scope.id, uid), { status: "revoked", revision: old.revision + 1, updatedAt: serverTimestamp() });
  batch.delete(inbox(db, scope.id, uid)); await batch.commit(); return old;
}
async function reissue(scope, change = {}) {
  const db = dbFor("host"), old = (await getDoc(invitation(db, scope.id))).data(), batch = writeBatch(db);
  batch.update(invitation(db, scope.id), { status: "active", revision: old.revision + 1, updatedAt: serverTimestamp(), ...change });
  batch.set(inbox(db, scope.id), { schema: 1, sessionId: scope.id, hostUid: "host", expiresAt: scope.data.expiresAt });
  batch.set(doc(db, "socialPrivateThrottle", "host"), { at: serverTimestamp() }); await batch.commit();
}

test("private nonfriend recipient can discover exactly its invitation; third parties and lists remain denied", async () => {
  const scope = await publish(), guest = dbFor("guest"), other = dbFor("other"), host = dbFor("host");
  await assertFails(getDoc(doc(guest, "socialSessions", scope.id)));
  await send(scope);
  assert.equal((await getDoc(doc(guest, "socialSessions", scope.id))).data().visibility, "private");
  const pointers = await getDocs(query(collection(guest, "socialPrivateInbox", "guest", "sessions"), limit(20)));
  assert.equal(pointers.size, 1);
  assert.deepEqual(Object.keys(pointers.docs[0].data()).sort(), ["expiresAt", "hostUid", "schema", "sessionId"]);
  assert.equal((await getDocs(query(collection(host, "socialPrivateInvitations", scope.id, "guests"), limit(8)))).size, 1);
  for (const ref of [doc(other, "socialSessions", scope.id), invitation(other, scope.id), audience(other, scope.id), inbox(other, scope.id), doc(guest, "socialOwners", "host")]) await assertFails(getDoc(ref));
  for (const ref of [collection(guest, "socialSessions"), collection(guest, "socialPrivateInvitations", scope.id, "guests"), collection(other, "socialPrivateInbox", "guest", "sessions")]) await assertFails(getDocs(query(ref, limit(8))));
  await assertFails(getDocs(collection(guest, "socialPrivateInbox", "guest", "sessions")));
  await assertFails(getDocs(query(collection(guest, "socialPrivateInbox", "guest", "sessions"), limit(21))));
  assert.equal((await getDocs(query(collection(other, "socialCatalog"), limit(20)))).size, 0);
});

test("private issuance requires host, registered nonself recipient, atomic records, throttle and exact immutable metadata", async () => {
  const scope = await publish();
  for (const field of ["auth", "inbox", "audience", "throttle"]) await assert.rejects(send(scope, "guest", { omit: [field] }));
  for (const modify of [{ revision: 2 }, { authorizationId: "f".repeat(63) }, { recipientUid: "other" }, { sessionId: crypto.randomUUID() },
    { createdAt: Timestamp.fromMillis(1) }, { updatedAt: Timestamp.fromMillis(1) }, { status: "revoked" }, { extra: true }, { expiresAt: Timestamp.fromMillis(Date.now() + 1000) }])
    await assert.rejects(send(scope, "guest", { modify }));
  await assert.rejects(send(scope, "unregistered")); await assert.rejects(send(scope, "host"));
  await assert.rejects(send(scope, "guest", { actor: "other", recipients: ["guest"] }));
  await send(scope); await assert.rejects(send(scope, "other"), "two-second sender throttle");
  await assertFails(updateDoc(audience(dbFor("host"), scope.id), { recipients: [] }));
  await assertFails(deleteDoc(audience(dbFor("host"), scope.id)));
});

test("private decline and reinvitation retain ID, advance revision and cannot delete/reset a live tombstone", async () => {
  const scope = await publish(); await send(scope); const first = await withdraw(scope, "guest", "guest");
  const guest = dbFor("guest"), host = dbFor("host");
  await assertFails(getDoc(doc(guest, "socialSessions", scope.id)));
  assert.equal((await getDoc(inbox(guest, scope.id))).exists(), false);
  assert.equal((await getDoc(invitation(host, scope.id))).data().revision, 2);
  await assertFails(deleteDoc(invitation(host, scope.id))); await assertFails(deleteDoc(invitation(guest, scope.id)));
  await cooldown();
  for (const change of [{ revision: 1 }, { revision: 2 }, { authorizationId: "e".repeat(64) }, { createdAt: serverTimestamp() }, { expiresAt: Timestamp.fromMillis(Date.now() + 1000) }])
    await assert.rejects(reissue(scope, change));
  await reissue(scope); const current = (await getDoc(invitation(host, scope.id))).data();
  assert.equal(current.authorizationId, first.authorizationId); assert.equal(current.revision, 3);
  assert.equal((await getDoc(doc(guest, "socialSessions", scope.id))).data().hostUid, "host");
  await assertFails(deleteDoc(inbox(guest, scope.id))); await assertFails(deleteDoc(invitation(host, scope.id)));
  await assertFails(updateDoc(invitation(guest, scope.id), { status: "revoked", revision: 4, updatedAt: serverTimestamp() }));
  await assert.rejects(withdraw(scope, "guest", "other"));
  await withdraw(scope); assert.equal((await getDoc(invitation(host, scope.id))).data().revision, 4);
});

test("private active authorization never overrides bilateral block, expiry, visibility policy or owner pointer", async () => {
  const scope = await publish(); await send(scope);
  for (const blocker of ["host", "guest"]) {
    await env.withSecurityRulesDisabled(context => setDoc(doc(context.firestore(), "socialState", blocker), { blocked: [blocker === "host" ? "guest" : "host"] }));
    await assertFails(getDoc(doc(dbFor("guest"), "socialSessions", scope.id)));
    await env.withSecurityRulesDisabled(context => deleteDoc(doc(context.firestore(), "socialState", blocker)));
  }
  await env.withSecurityRulesDisabled(context => updateDoc(doc(context.firestore(), "socialOwners", "host"), { sessionId: crypto.randomUUID() }));
  await assertFails(getDoc(doc(dbFor("guest"), "socialSessions", scope.id)));
  await env.withSecurityRulesDisabled(context => updateDoc(doc(context.firestore(), "socialOwners", "host"), { sessionId: scope.id }));
  await env.withSecurityRulesDisabled(context => updateDoc(invitation(context.firestore(), scope.id), { expiresAt: Timestamp.fromMillis(1) }));
  await assertFails(getDoc(doc(dbFor("guest"), "socialSessions", scope.id)));
  const host = dbFor("host"), cleanup = writeBatch(host); cleanup.delete(inbox(host, scope.id)); cleanup.delete(invitation(host, scope.id));
  // Expiring only the auth record is not enough to remove the still-live pointer independently.
  await assertFails(cleanup.commit());
});

test("private audience allows eight lifetime recipients but cannot drop history or add a ninth after revocation", async () => {
  const scope = await publish();
  for (const uid of ["guest", "guest2", "guest3", "guest4", "guest5", "guest6", "guest7", "guest8"]) { await cooldown(); await send(scope, uid); }
  await withdraw(scope); await cooldown(); await assert.rejects(send(scope, "other"));
  await assertFails(updateDoc(audience(dbFor("host"), scope.id), { recipients: ["guest2"] }));
  assert.equal((await getDoc(audience(dbFor("host"), scope.id))).data().recipients.length, 8);
});

test("private republish rotates announcement atomically while retaining unexpired authorization tombstones", async () => {
  const scope = await publish(); await send(scope);
  const host = dbFor("host"), next = crypto.randomUUID(), batch = writeBatch(host);
  batch.delete(doc(host, "socialSessions", scope.id));
  batch.set(doc(host, "socialOwners", "host"), { sessionId: next, visibility: "private" });
  batch.set(doc(host, "socialSessions", next), { ...scope.data, title: "Replacement", expiresAt: Timestamp.fromMillis(Date.now() + 90_000) });
  await batch.commit();
  await assertFails(getDoc(doc(dbFor("guest"), "socialSessions", next)));
  await assertFails(getDoc(doc(dbFor("guest"), "socialSessions", scope.id)));
  await assertFails(deleteDoc(invitation(host, scope.id)));
  assert.equal((await getDoc(invitation(host, scope.id))).data().status, "active", "old record grants no visibility after owner change");
  await assertFails(updateDoc(doc(host, "socialOwners", "host"), { sessionId: crypto.randomUUID() }));
});

test("private scope cannot be edited, toggled public or recreated under its old ID while tombstones remain live", async () => {
  const scope = await publish(); await send(scope); const db = dbFor("host");
  await assertFails(updateDoc(doc(db, "socialSessions", scope.id), { title: "Changed in place" }));
  const expose = writeBatch(db);
  expose.update(doc(db, "socialSessions", scope.id), { visibility: "public" });
  expose.update(doc(db, "socialOwners", "host"), { visibility: "public" });
  expose.set(doc(db, "socialCatalog", scope.id), { schema: 1, expiresAt: scope.data.expiresAt });
  await assertFails(expose.commit());
  const close = writeBatch(db); close.delete(doc(db, "socialSessions", scope.id)); close.delete(doc(db, "socialOwners", "host")); await close.commit();
  const resurrect = writeBatch(db); resurrect.set(doc(db, "socialSessions", scope.id), scope.data);
  resurrect.set(doc(db, "socialOwners", "host"), { sessionId: scope.id, visibility: "private" });
  await assertFails(resurrect.commit());
  await assertFails(deleteDoc(audience(db, scope.id))); await assertFails(deleteDoc(invitation(db, scope.id)));
});

test("private authorization cannot be issued for public or approval-only announcements", async () => {
  const scope = await publish("private", "approval"); await assert.rejects(send(scope));
  const host = dbFor("host"), batch = writeBatch(host); batch.delete(doc(host, "socialSessions", scope.id)); batch.delete(doc(host, "socialOwners", "host")); await batch.commit();
  const publicScope = await publish("public"); await assert.rejects(send(publicScope));
});

test("private SDK searches Aurora accounts and supplies inbox entry without transport credentials; revoke and reissue roundtrip", async () => {
  const a = createService(services("host")), b = createService(services("guest")), scope = await publish();
  assert.equal((await a.searchByNickname("GuEsT")).uid, "guest");
  await a.privateInvitations.invite(scope.id, "guest");
  const received = await b.privateInvitations.snapshot();
  assert.equal(received.incoming.length, 1); assert.equal(received.incoming[0].session.id, scope.id);
  assert.equal(received.incoming[0].host.uid, "host");
  assert.ok(!/authorizationId|revision|publicKey|token|directAddress/.test(JSON.stringify(received)));
  assert.equal((await a.privateInvitations.snapshot()).remainingRecipients, 7);
  await assert.rejects(a.privateInvitations.invite(scope.id, "guest"));
  await b.privateInvitations.decline(scope.id);
  assert.equal((await b.privateInvitations.snapshot()).incoming.length, 0);
  assert.equal((await a.privateInvitations.snapshot()).outgoing[0].status, "revoked");
  await cooldown(); await a.privateInvitations.invite(scope.id, "guest");
  assert.equal((await b.privateInvitations.snapshot()).incoming.length, 1);
  await a.privateInvitations.revoke(scope.id, "guest");
  assert.equal((await b.privateInvitations.snapshot()).incoming.length, 0);
});

test("private SDK republish rotates scope and stale account cannot invite; production adapter retains private gate", async () => {
  const config = services("host"), a = createService(config), b = createService(services("guest")), scope = await publish();
  await a.privateInvitations.invite(scope.id, "guest");
  const { schema, hostUid, expiresAt, ...draft } = scope.data;
  await a.publishSession({ ...draft, title: "Replacement from SDK" });
  const next = (await a.snapshot()).ownSession;
  assert.notEqual(next.id, scope.id); assert.equal(next.title, "Replacement from SDK");
  assert.equal((await b.privateInvitations.snapshot()).incoming.length, 0);
  await assertFails(deleteDoc(invitation(config.db, scope.id)));
  config.auth.currentUser = { ...config.auth.currentUser };
  await assert.rejects(a.privateInvitations.invite(next.id, "guest"), /conta mudou/);
  assert.throws(() => createService(services("host", "auroralauncher")), /horário do serviço/,
    "the emulator adapter rejects the production project before constructing any service");
});

test("private SDK can switch explicitly to public and friends using a new scope, without restoring old recipient authority", async () => {
  const a = createService(services("host")), b = createService(services("guest")), scope = await publish();
  await a.privateInvitations.invite(scope.id, "guest");
  const { schema, hostUid, expiresAt, ...draft } = scope.data;
  await a.publishSession({ ...draft, visibility: "public" });
  const current = (await a.snapshot()).ownSession;
  assert.notEqual(current.id, scope.id); assert.equal(current.visibility, "public");
  assert.equal((await b.listPublicSessions()).items[0].id, current.id);
  assert.equal((await b.privateInvitations.snapshot()).incoming.length, 0);
  await a.publishSession({ ...draft, visibility: "private" });
  const again = (await a.snapshot()).ownSession;
  await assertFails(getDoc(doc(dbFor("guest"), "socialSessions", again.id)));
  await a.publishSession({ ...draft, visibility: "friends" });
  const friends = (await a.snapshot()).ownSession;
  assert.notEqual(friends.id, again.id); assert.equal(friends.visibility, "friends");
  await assertFails(getDoc(doc(dbFor("guest"), "socialSessions", friends.id)));
});

test("private expired tombstones can be removed only after the original deadline and with the inbox removed", async () => {
  const scope = await publish(); await send(scope);
  const db = dbFor("host"), expiresAt = Timestamp.fromMillis(Date.now() - 10_000);
  await env.withSecurityRulesDisabled(async context => {
    const admin = context.firestore(), batch = writeBatch(admin);
    for (const ref of [doc(admin, "socialSessions", scope.id), audience(admin, scope.id), invitation(admin, scope.id), inbox(admin, scope.id)])
      batch.update(ref, { expiresAt });
    await batch.commit();
  });
  assert.equal((await createService(services("guest")).privateInvitations.snapshot()).incoming.length, 0);
  await assertFails(getDoc(doc(dbFor("guest"), "socialSessions", scope.id)));
  await assertFails(deleteDoc(invitation(db, scope.id)), "tombstone deletion still requires paired pointer cleanup");
  const cleanup = writeBatch(db);
  cleanup.delete(inbox(db, scope.id)); cleanup.delete(invitation(db, scope.id)); cleanup.delete(audience(db, scope.id));
  await assertSucceeds(cleanup.commit());
  assert.equal((await getDoc(invitation(dbFor("guest"), scope.id))).exists(), false);
});

test("private maintenance lists only the caller's bounded audience without disclosing another host", async () => {
  const scope = await publish(); await send(scope);
  const host = dbFor("host"), guest = dbFor("guest"), root = collection(host, "socialPrivateAudiences");
  assert.equal((await getDocs(query(root, where("hostUid", "==", "host"), orderBy(documentId()), limit(4)))).size, 1);
  await assertFails(getDocs(query(root, limit(4))));
  await assertFails(getDocs(query(root, where("hostUid", "==", "host"), limit(5))));
  await assertFails(getDocs(query(root, where("hostUid", "==", "host"))));
  await assertFails(getDocs(query(collection(guest, "socialPrivateAudiences"), where("hostUid", "==", "host"), limit(4))));
  assert.equal((await getDocs(query(collection(guest, "socialPrivateAudiences"), where("hostUid", "==", "guest"), limit(4)))).size, 0);
});

test("private SDK maintenance preserves live authority and cleans eight expired recipients after independent recipient cleanup", async () => {
  const scope = await publish(), peers = ["guest", "guest2", "guest3", "guest4", "guest5", "guest6", "guest7", "guest8"];
  for (const uid of peers) { await cooldown(); await send(scope, uid); }
  const host = createService(services("host")), guest = createService(services("guest"));
  assert.equal((await host.privateInvitations.cleanupExpired()).removedRecords, 0);
  assert.equal((await guest.privateInvitations.cleanupExpired()).removedRecords, 0);
  assert.equal((await getDoc(invitation(dbFor("guest"), scope.id))).data().status, "active");
  await env.withSecurityRulesDisabled(async context => {
    const db = context.firestore(), batch = writeBatch(db), expiresAt = Timestamp.fromMillis(Date.now() - 10_000);
    for (const ref of [doc(db, "socialSessions", scope.id), audience(db, scope.id),
      ...peers.flatMap(uid => [invitation(db, scope.id, uid), inbox(db, scope.id, uid)])]) batch.update(ref, { expiresAt });
    await batch.commit();
  });
  assert.equal((await guest.privateInvitations.cleanupExpired()).removedRecords, 2);
  assert.equal((await createService(services("other")).privateInvitations.cleanupExpired()).removedRecords, 0);
  assert.equal((await host.privateInvitations.cleanupExpired()).removedRecords, 15);
  await env.withSecurityRulesDisabled(async context => {
    const db = context.firestore();
    for (const ref of [audience(db, scope.id), ...peers.flatMap(uid => [invitation(db, scope.id, uid), inbox(db, scope.id, uid)])])
      assert.equal((await getDoc(ref)).exists(), false);
  });
  assert.equal((await host.privateInvitations.cleanupExpired()).removedRecords, 0);
});

test("private SDK maintenance resumes bounded own pages past live audiences and rejects a changed account", async () => {
  const ids = Array.from({ length: 9 }, (_, i) => `00000000-0000-4000-8000-${String(i + 1).padStart(12, "0")}`);
  await env.withSecurityRulesDisabled(async context => {
    const db = context.firestore(), batch = writeBatch(db);
    for (const [index, id] of ids.entries()) {
      const expiresAt = Timestamp.fromMillis(Date.now() + (index < 4 ? 90_000 : -10_000));
      batch.set(audience(db, id), { schema: 1, hostUid: "host", sessionId: id, expiresAt, recipients: ["guest"] });
      batch.set(invitation(db, id), { schema: 1, authorizationId: "a".repeat(64), revision: 1, sessionId: id, hostUid: "host", recipientUid: "guest",
        status: "active", createdAt: Timestamp.fromMillis(1), updatedAt: Timestamp.fromMillis(2), expiresAt });
      batch.set(inbox(db, id), { schema: 1, sessionId: id, hostUid: "host", expiresAt });
    }
    await batch.commit();
  });
  const config = services("host"), service = createService(config);
  const first = await service.privateInvitations.cleanupExpired();
  assert.equal(first.removedRecords, 0); assert.equal(first.moreAvailable, true);
  const second = await service.privateInvitations.cleanupExpired();
  assert.equal(second.removedRecords, 12); assert.equal(second.moreAvailable, true);
  const third = await service.privateInvitations.cleanupExpired();
  assert.equal(third.removedRecords, 3); assert.equal(third.moreAvailable, false);
  assert.equal((await service.privateInvitations.cleanupExpired()).removedRecords, 0);
  await env.withSecurityRulesDisabled(async context => {
    for (const id of ids.slice(0, 4)) assert.equal((await getDoc(audience(context.firestore(), id))).exists(), true);
  });
  config.auth.currentUser = { ...config.auth.currentUser };
  await assert.rejects(service.privateInvitations.cleanupExpired(), /conta mudou/);
});
