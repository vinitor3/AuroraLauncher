import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { before, beforeEach, after, test } from "node:test";
import { assertFails, assertSucceeds, initializeTestEnvironment } from "@firebase/rules-unit-testing";
import { collection, doc, getDoc, getDocs, limit, query, setDoc, Timestamp, serverTimestamp, updateDoc, deleteDoc, disableNetwork, enableNetwork, writeBatch } from "firebase/firestore";

const projectId = "demo-aurora-social-rules-v2";
let env, createService;
const desktopRequire = createRequire(new URL("../../apps/desktop/package.json", import.meta.url));
const empty = { friends: [], incoming: [], outgoing: [], blocked: [], revision: 1, kind: "block", actor: "a", peer: "b", updatedAt: Timestamp.now() };
const draft = { title: "Mundo Aurora", modpackName: "Meu Modpack", minecraftVersion: "1.21.1", loader: "Fabric", imageUrl: null,
  players: 1, maxPlayers: 8, visibility: "public", joinPolicy: "open" };

before(async () => {
  const ts = desktopRequire("typescript");
  const transpile = text => ts.transpileModule(text, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText;
  const shared = `data:text/javascript;base64,${Buffer.from(transpile(await readFile(new URL("../../apps/desktop/src/shared/social.ts", import.meta.url),"utf8"))).toString("base64")}`;
  const clock = `data:text/javascript;base64,${Buffer.from(transpile(await readFile(new URL("../../apps/desktop/src/lib/socialServerClock.ts", import.meta.url),"utf8"))).toString("base64")}`;
  const organization = `data:text/javascript;base64,${Buffer.from(transpile(await readFile(new URL("../../apps/desktop/src/lib/socialOrganization.ts", import.meta.url),"utf8")).replace('"firebase/firestore"', JSON.stringify(import.meta.resolve("firebase/firestore")))).toString("base64")}`;
  const privateInvitations = `data:text/javascript;base64,${Buffer.from(transpile(await readFile(new URL("../../apps/desktop/src/lib/socialPrivateInvitations.ts", import.meta.url),"utf8")).replace('"firebase/firestore"', JSON.stringify(import.meta.resolve("firebase/firestore")))).toString("base64")}`;
  const source = transpile(await readFile(new URL("../../apps/desktop/src/lib/socialFirebase.ts", import.meta.url),"utf8"))
    .replace('"../shared/social"', JSON.stringify(shared))
    .replace('"./socialOrganization"', JSON.stringify(organization))
    .replace('"./socialPrivateInvitations"', JSON.stringify(privateInvitations))
    .replace('"./socialServerClock"', JSON.stringify(clock))
    .replace('"./firebase"', JSON.stringify("data:text/javascript,export const socialLabBuild=true;export const privateSocialOnlineConfiguration=()=>false"))
    .replace('"firebase/firestore"', JSON.stringify(import.meta.resolve("firebase/firestore")));
  createService = (await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`)).createSocialFirebaseService;
  env = await initializeTestEnvironment({ projectId, firestore: { host: "127.0.0.1", port: 8187, rules: await readFile(new URL("../firestore.rules", import.meta.url), "utf8") } });
});
beforeEach(async () => {
  await env.clearFirestore();
  await env.withSecurityRulesDisabled(async context => {
    const db = context.firestore();
    for (const uid of ["a", "b", "c", "d"]) {
      await setDoc(doc(db, "users", uid), { uid, username: `User_${uid}`, usernameLower: `user_${uid}` });
      await setDoc(doc(db, "usernames", `user_${uid}`), { uid, usernameLower: `user_${uid}` });
      await setDoc(doc(db, "publicProfiles", uid), { uid, username: `User_${uid}`, avatarUrl: null });
    }
  });
});
after(async () => { await env?.cleanup(); });
const dbFor = uid => env.authenticatedContext(uid).firestore();
function sdkServices(uid, db = dbFor(uid)) {
  const claims = { sub: uid, user_id: uid, aud: projectId, iss: `https://securetoken.google.com/${projectId}`, iat: Math.floor(Date.now()/1000), exp: Math.floor(Date.now()/1000)+3600, firebase: { identities: {}, sign_in_provider: "custom" } };
  const token = `${Buffer.from('{"alg":"none","typ":"JWT"}').toString("base64url")}.${Buffer.from(JSON.stringify(claims)).toString("base64url")}.`;
  return { db, config: { projectId }, authScope: { id: `test-${uid}`, persistent: false }, auth: { currentUser: { uid, getIdToken: async () => token }, onAuthStateChanged: () => () => {} } };
}
const service = uid => createService(sdkServices(uid));
async function friendship() { await service("a").requestFriend("b"); await service("b").respondFriend("a", "accept"); }
async function ownId(uid) { return (await getDoc(doc(dbFor(uid), "socialOwners", uid))).data().sessionId; }

test("SDK Firestore clock permits bounded writes and reads with the computer one year ahead", async () => {
  await friendship();
  const a = service("a"), b = service("b"), originalNow = Date.now;
  const start = originalNow();
  Date.now = () => originalNow() + 365 * 86_400_000;
  try {
    await a.publishSession(draft);
    await a.setPresence({ sharing: "friends", modpackName: "Clock fixture" });
    await a.organization.createGroup("Clock group");
    await a.organization.inviteToGroup("b");
    const id = await ownId("a"), end = originalNow();
    for (const [ref, ttl] of [[doc(dbFor("a"), "socialSessions", id), 14_400_000], [doc(dbFor("a"), "socialGroups", "a"), 14_400_000],
      [doc(dbFor("a"), "socialPresence", "a"), 120_000], [doc(dbFor("b"), "socialGroupInvites", "b", "hosts", "a"), 300_000]]) {
      const expiresAt = (await getDoc(ref)).data().expiresAt.toMillis();
      assert.ok(expiresAt > start && expiresAt <= end + ttl);
    }
    assert.equal((await b.listPublicSessions()).items[0].id, id);
    assert.equal((await b.snapshot()).friends[0].presence.modpackName, "Clock fixture");
    assert.equal((await b.organization.snapshot()).invitations[0].groupId, "a");
  } finally { Date.now = originalNow; }
});

test("SDK exact Aurora nick search and read-only empty snapshot; directory LIST denied", async () => {
  assert.equal((await service("a").searchByNickname(" UsEr_B ")).nickname, "user_b");
  assert.equal(await service("a").searchByNickname("UnknownNick"), null);
  await assert.rejects(service("a").searchByNickname(""));
  const snapshot = await service("a").snapshot();
  assert.equal(snapshot.presenceSharing, "hidden"); assert.deepEqual(snapshot.friends, []);
  assert.equal((await getDoc(doc(dbFor("a"), "socialState", "a"))).exists(), false);
  for (const name of ["users", "usernames", "publicProfiles", "socialState", "socialSessions", "socialPresence"]) {
    await assertFails(getDocs(query(collection(dbFor("a"), name), limit(20))));
  }
});
test("SDK request/accept are reciprocal; peer graph never readable", async () => {
  await service("a").requestFriend("b");
  assert.equal((await service("b").snapshot()).incoming[0].nickname, "User_a");
  await assertFails(getDoc(doc(dbFor("a"), "socialState", "b")));
  await assert.rejects(service("a").respondFriend("b", "accept"));
  await service("b").respondFriend("a", "accept");
  assert.equal((await service("a").snapshot()).friends[0].uid, "b");
  assert.equal((await service("b").snapshot()).friends[0].uid, "a");
  assert.equal((await getDoc(doc(dbFor("a"), "socialRequests", "a", "peers", "b"))).exists(), false);
  await service("a").removeFriend("b");
  assert.deepEqual((await service("b").snapshot()).friends, []);
});
test("SDK decline, cancel and block clear both request containers without granting friendship", async () => {
  await service("a").requestFriend("b"); await service("b").respondFriend("a", "decline");
  assert.deepEqual((await service("a").snapshot()).outgoing, []);
  await service("c").requestFriend("b"); await service("c").cancelRequest("b");
  await service("d").requestFriend("b"); await service("b").block("d");
  assert.deepEqual((await service("d").snapshot()).outgoing, []);
  await assert.rejects(service("d").requestFriend("b"));
  await service("b").unblock("d");
  assert.deepEqual((await service("b").snapshot()).friends, []);
});
test("SDK friend-only presence and session disappear after reciprocal block; public metadata remains protected", async () => {
  await friendship();
  await service("a").setPresence({ sharing: "friends", modpackName: "Create" });
  await service("a").publishSession({ ...draft, visibility: "friends" });
  assert.equal((await service("b").snapshot()).friends[0].presence.modpackName, "Create");
  const id = await ownId("a");
  await assertFails(getDoc(doc(dbFor("c"), "socialSessions", id)));
  await service("a").publishSession(draft);
  assert.equal((await service("c").listPublicSessions()).items.length, 1);
  await service("b").block("a");
  await assertFails(getDoc(doc(dbFor("b"), "socialSessions", id)));
  await assertFails(getDoc(doc(dbFor("b"), "socialPresence", "a")));
  assert.deepEqual((await service("b").listPublicSessions()).items, []);
  assert.equal((await service("c").listPublicSessions()).items.length, 1);
});
test("SDK public/private/close keeps one owner pointer and opaque index atomic", async () => {
  await service("a").publishSession(draft); const id = await ownId("a");
  const catalog = await getDocs(query(collection(dbFor("b"), "socialCatalog"), limit(20)));
  assert.deepEqual(Object.keys(catalog.docs[0].data()).sort(), ["expiresAt", "schema"]);
  await service("a").publishSession({ ...draft, visibility: "private" });
  assert.equal(await ownId("a"), id);
  assert.equal((await service("b").listPublicSessions()).items.length, 0);
  await assertFails(getDoc(doc(dbFor("b"), "socialSessions", id)));
  await service("a").closeSession();
  assert.equal((await getDoc(doc(dbFor("a"), "socialOwners", "a"))).exists(), false);
});
test("rules reject one-sided friendship, self requests, forged actors, auth-only accounts and duplicate requests", async () => {
  const db = dbFor("a");
  await assertFails(setDoc(doc(db, "socialState", "a"), { ...empty, friends: ["b"], kind: "accept", updatedAt: serverTimestamp() }));
  await assert.rejects(service("a").requestFriend("a"));
  await assert.rejects(service("not-registered").requestFriend("a"));
  await assert.rejects(service("not-registered").publishSession(draft));
  await assert.rejects(service("not-registered").setPresence({ sharing: "friends", modpackName: "Fake" }));
  await service("a").requestFriend("b");
  await assert.rejects(service("a").requestFriend("b"));
  await assertFails(updateDoc(doc(db, "socialState", "a"), { actor: "c", revision: 2, updatedAt: serverTimestamp() }));
});
test("expired request cannot be accepted and can be declined to free bounded slot", async () => {
  await service("a").requestFriend("b");
  await env.withSecurityRulesDisabled(async c => updateDoc(doc(c.firestore(), "socialRequests", "a", "peers", "b"), { expiresAt: Timestamp.fromMillis(1) }));
  await assert.rejects(service("b").respondFriend("a", "accept"));
  await service("b").respondFriend("a", "decline");
  assert.equal((await service("a").snapshot()).outgoing.length, 0);
});
test("rate and ten pending slots are server enforced", async () => {
  await service("a").requestFriend("b");
  await assert.rejects(service("a").requestFriend("c"));
  await env.withSecurityRulesDisabled(async c => {
    await setDoc(doc(c.firestore(), "socialState", "c"), { ...empty, incoming: Array.from({ length: 10 }, (_, i) => `u${i}`) });
  });
  await assert.rejects(service("d").requestFriend("c"));
});
test("public LIST requires bound; projections reject credentials, arbitrary artwork, forged owner and standalone index", async () => {
  await service("a").publishSession(draft); const id = await ownId("a"); const db = dbFor("a");
  await assertFails(getDocs(collection(db, "socialCatalog")));
  await assertFails(getDocs(query(collection(db, "socialCatalog"), limit(21))));
  await assertFails(updateDoc(doc(db, "socialSessions", id), { endpoint: "127.0.0.1:43001" }));
  await assertFails(updateDoc(doc(db, "socialSessions", id), { imageUrl: "https://evil.test/a.png" }));
  await assertFails(updateDoc(doc(dbFor("b"), "socialSessions", id), { hostUid: "b" }));
  await assertFails(setDoc(doc(db, "socialCatalog", "00000000-0000-4000-8000-000000000009"), { schema: 1, expiresAt: Timestamp.fromMillis(Date.now()+10000) }));
  const anonymous = env.unauthenticatedContext().firestore();
  await assertFails(getDocs(query(collection(anonymous, "socialCatalog"), limit(20))));
});
test("SDK bounded catalog paginates mixed expired/blocked entries without leaking or auto filling", async () => {
  await env.withSecurityRulesDisabled(async c => {
    const db = c.firestore();
    for (let i=0;i<21;i++) {
      const id = `00000000-0000-4000-8000-${String(i).padStart(12,"0")}`, host = `host${i}`;
      const expiry = Timestamp.fromMillis(Date.now()+(i===0?-1000:100000));
      await setDoc(doc(db,"socialCatalog",id),{schema:1,expiresAt:expiry});
      await setDoc(doc(db,"socialOwners",host),{sessionId:id,visibility:"public"});
      await setDoc(doc(db,"socialSessions",id),{...draft,visibility:i===2?"private":"public",schema:1,hostUid:host,expiresAt:expiry});
      if (i===1) await setDoc(doc(db,"socialState",host),{...empty,blocked:["a"]});
    }
  });
  const first = await service("a").listPublicSessions();
  assert.equal(first.items.length,17); assert.ok(first.nextCursor);
  const second = await service("a").listPublicSessions(first.nextCursor);
  assert.equal(second.items.length,1); assert.equal(second.nextCursor,null);
});
test("auth changes and automatic join remain fail closed", async () => {
  const services = sdkServices("a"), auth = services.auth; const s = createService(services);
  assert.equal(s.capabilities.join,false);
  await assert.rejects(s.prepareJoin("anything"),/Nenhuma conexão/);
  auth.currentUser = { uid:"b" };
  await assert.rejects(s.requestFriend("c"),/conta mudou/);
});
test("accepted-friend and block quotas reject 101st entry without changing reciprocal state", async () => {
  await service("b").requestFriend("a");
  await env.withSecurityRulesDisabled(async c => {
    await updateDoc(doc(c.firestore(),"socialState","a"),{friends:Array.from({length:100},(_,i)=>`friend${i}`)});
  });
  await assert.rejects(service("a").respondFriend("b","accept"));
  assert.deepEqual((await getDoc(doc(dbFor("b"),"socialState","b"))).data().outgoing,["a"]);
  await env.withSecurityRulesDisabled(async c => {
    await updateDoc(doc(c.firestore(),"socialState","a"),{blocked:Array.from({length:100},(_,i)=>`blocked${i}`)});
  });
  await assert.rejects(service("a").block("c"));
  assert.equal((await getDoc(doc(dbFor("c"),"socialState","c"))).exists(),false);
});
test("expired presence and sessions cannot be projected fresh; owner can explicitly close", async () => {
  await friendship(); await service("a").publishSession(draft);
  await service("a").setPresence({sharing:"friends",modpackName:"Create"});
  const id=await ownId("a");
  await env.withSecurityRulesDisabled(async c => {
    for(const [name,key] of [["socialSessions",id],["socialCatalog",id],["socialPresence","a"]])
      await updateDoc(doc(c.firestore(),name,key),{expiresAt:Timestamp.fromMillis(1)});
  });
  const snapshot=await service("b").snapshot();
  assert.equal(snapshot.friends[0].presence,null);assert.equal(snapshot.friends[0].session,null);
  assert.deepEqual((await service("b").listPublicSessions()).items,[]);
  await service("a").closeSession();
});
test("raw writes cannot orphan published index, change request expiry, or inject unrenderable session text", async () => {
  await service("a").publishSession(draft); const id=await ownId("a"),db=dbFor("a");
  await assertFails(deleteDoc(doc(db,"socialOwners","a")));
  await assertFails(deleteDoc(doc(db,"socialSessions",id)));
  await assertFails(deleteDoc(doc(db,"socialCatalog",id)));
  await assertFails(updateDoc(doc(db,"socialSessions",id),{title:"\nInjected"}));
  await assertFails(updateDoc(doc(db,"socialSessions",id),{title:"   "}));
  await service("a").requestFriend("b");
  await assertFails(updateDoc(doc(db,"socialRequests","a","peers","b"),{expiresAt:Timestamp.fromMillis(Date.now()+999999999)}));
});
test("offline SDK cannot reuse previously permitted public projection after a block", async () => {
  await service("a").publishSession(draft);
  const db=dbFor("b"), reader=createService(sdkServices("b",db));
  assert.equal((await reader.listPublicSessions()).items.length,1);
  await service("a").block("b");
  await disableNetwork(db);
  try { await assert.rejects(reader.listPublicSessions()); }
  finally { await enableNetwork(db); }
  assert.deepEqual((await reader.listPublicSessions()).items,[]);
});
test("paired accept rejects extra unrelated friendship and third-party request reads", async () => {
  await service("a").requestFriend("b");
  const a=(await getDoc(doc(dbFor("a"),"socialState","a"))).data();
  const b=(await getDoc(doc(dbFor("b"),"socialState","b"))).data();
  const db=dbFor("b"),batch=writeBatch(db);
  batch.set(doc(db,"socialState","a"),{...a,friends:["b"],outgoing:[],revision:a.revision+1,kind:"accept",actor:"b",peer:"b",updatedAt:serverTimestamp()});
  batch.set(doc(db,"socialState","b"),{...b,friends:["a","c"],incoming:[],revision:b.revision+1,kind:"accept",actor:"b",peer:"a",updatedAt:serverTimestamp()});
  batch.delete(doc(db,"socialRequests","a","peers","b"));
  await assertFails(batch.commit());
  await assertFails(getDoc(doc(dbFor("c"),"socialRequests","a","peers","b")));
  await service("b").respondFriend("a","accept");
});
test("a committed block prevents a concurrent acceptance from retaining friendship", async () => {
  await service("a").requestFriend("b");
  const [blockResult,acceptResult]=await Promise.allSettled([service("a").block("b"),service("b").respondFriend("a","accept")]);
  // An aborted transaction is an explicit failed action, not a successful block.
  // A second explicit action resolves that case; the adapter itself never retries.
  if(blockResult.status==="rejected") await service("a").block("b");
  assert.ok(["fulfilled","rejected"].includes(acceptResult.status));
  assert.deepEqual((await service("a").snapshot()).friends,[]);
  assert.deepEqual((await service("b").snapshot()).friends,[]);
  assert.equal((await service("a").snapshot()).blocked[0].uid,"b");
  await assert.rejects(service("b").respondFriend("a","accept"));
});
test("Auth-only identities cannot enumerate the public index or read known public session metadata", async () => {
  await service("a").publishSession(draft);
  const id=await ownId("a"),authOnly=dbFor("auth-only-no-aurora-profile");
  await assertFails(getDocs(query(collection(authOnly,"socialCatalog"),limit(20))));
  await assertFails(getDoc(doc(authOnly,"socialCatalog",id)));
  await assertFails(getDoc(doc(authOnly,"socialSessions",id)));
  // A profile with no matching immutable nickname registry is also insufficient.
  await env.withSecurityRulesDisabled(async context => {
    await setDoc(doc(context.firestore(),"users","auth-only-no-aurora-profile"),{
      uid:"auth-only-no-aurora-profile",usernameLower:"unregistered_nick",
    });
  });
  await assertFails(getDocs(query(collection(authOnly,"socialCatalog"),limit(20))));
  await assertFails(getDoc(doc(authOnly,"socialSessions",id)));
  assert.equal((await service("b").listPublicSessions()).items[0].id,id);
});


test("groups require friendship and recipient consent; removals, leave and closure retain one membership", async () => {
  await friendship(); const a = service("a").organization, b = service("b").organization;
  await a.createGroup("Aventura");
  await assert.rejects(a.inviteToGroup("c"));
  await a.inviteToGroup("b");
  assert.equal((await b.snapshot()).invitations[0].name, "Aventura");
  await assertFails(getDoc(doc(dbFor("c"), "socialGroups", "a")));
  await b.acceptGroup("a");
  assert.equal((await a.snapshot()).group.members.length, 2);
  await assert.rejects(b.createGroup("Outro"));
  await a.removeGroupMember("b"); assert.equal((await b.snapshot()).group, null);
  await a.inviteToGroup("b"); await b.acceptGroup("a"); await b.leaveGroup();
  assert.equal((await a.snapshot()).group.members.length, 1);
  await a.leaveGroup(); assert.equal((await a.snapshot()).group, null);
});

test("join approval is host-only, bound to session/account, and removed by revocation or session closure", async () => {
  const a = service("a"), b = service("b"); await a.publishSession({ ...draft, joinPolicy: "approval" }); const session = await ownId("a");
  await b.organization.requestJoin(session);
  assert.equal(await b.organization.joinRequestStatus(session), "pending");
  await assert.rejects(b.organization.respondJoin(session, "b", "accept"));
  await assertFails(getDoc(doc(dbFor("c"), "socialJoinRequests", session, "guests", "b")));
  await a.organization.respondJoin(session, "b", "accept");
  assert.equal(await b.organization.joinRequestStatus(session), "accepted");
  await a.organization.respondJoin(session, "b", "decline");
  assert.equal(await b.organization.joinRequestStatus(session), "none");
  await b.organization.requestJoin(session); await a.closeSession();
  assert.equal(await b.organization.joinRequestStatus(session), "none");
});

test("blocked recipient cannot accept a group or request admission, even with retained projections", async () => {
  await friendship(); const a = service("a"), b = service("b");
  await a.organization.createGroup("Aventura"); await a.organization.inviteToGroup("b");
  await a.publishSession({ ...draft, joinPolicy: "approval" }); const session = await ownId("a");
  await a.block("b");
  await assert.rejects(b.organization.acceptGroup("a"));
  await assert.rejects(b.organization.requestJoin(session));
});

test("group membership cannot be self-detached while live or reused to read a replacement group", async () => {
  await friendship(); const a = service("a").organization, b = service("b").organization;
  await a.createGroup("Original"); await a.inviteToGroup("b"); await b.acceptGroup("a");
  await assertFails(deleteDoc(doc(dbFor("b"), "socialGroupMembers", "b")));
  await assertFails(updateDoc(doc(dbFor("a"), "socialGroups", "a"), { invited: ["c"], updatedAt: serverTimestamp() }));
  await assertFails(updateDoc(doc(dbFor("c"), "socialGroups", "a"), { updatedAt: serverTimestamp() }));
  await a.leaveGroup(); await a.createGroup("Replacement");
  await assertFails(getDoc(doc(dbFor("b"), "socialGroups", "a")));
  assert.equal((await b.snapshot()).group, null);
  await b.createGroup("Independent"); assert.equal((await b.snapshot()).group.name, "Independent");
});

test("expired group remains visible for explicit closure; expired invitation grants no roster access", async () => {
  await friendship(); const a = service("a").organization, b = service("b").organization;
  await a.createGroup("Expired"); await a.inviteToGroup("b");
  await env.withSecurityRulesDisabled(async context => {
    await updateDoc(doc(context.firestore(), "socialGroupInvites", "b", "hosts", "a"), { expiresAt: Timestamp.fromMillis(1) });
    await updateDoc(doc(context.firestore(), "socialGroups", "a"), { expiresAt: Timestamp.fromMillis(1) });
  });
  await assertFails(getDoc(doc(dbFor("b"), "socialGroups", "a")));
  assert.equal((await b.snapshot()).invitations.length, 0); await b.declineGroup("a");
  assert.equal((await a.snapshot()).group.name, "Expired"); await a.leaveGroup(); await a.createGroup("New");
});

test("join queue removals must delete the same request and republishing clears previous approvals", async () => {
  const a = service("a"), b = service("b"); await a.publishSession({ ...draft, joinPolicy: "approval" }); const session = await ownId("a");
  await b.organization.requestJoin(session);
  await assertFails(deleteDoc(doc(dbFor("a"), "socialJoinQueues", session)));
  await assertFails(updateDoc(doc(dbFor("a"), "socialJoinQueues", session), { guests: [] }));
  await a.organization.respondJoin(session, "b", "decline");
  assert.deepEqual((await getDoc(doc(dbFor("a"), "socialJoinQueues", session))).data().guests, []);
  await b.organization.requestJoin(session); await a.organization.respondJoin(session, "b", "accept");
  await a.publishSession({ ...draft, joinPolicy: "approval", title: "New review" });
  assert.equal(await b.organization.joinRequestStatus(session), "none"); await b.organization.requestJoin(session);
});

test("pending join cancellation SDK deletes only its request and releases its queue slot", async () => {
  const a = service("a"), b = service("b"), c = service("c");
  await a.publishSession({ ...draft, joinPolicy: "approval" }); const session = await ownId("a");
  await b.organization.requestJoin(session); await c.organization.requestJoin(session);
  const queue = doc(dbFor("a"), "socialJoinQueues", session), before = (await getDoc(queue)).data();
  await b.organization.cancelJoinRequest(session);
  assert.equal(await b.organization.joinRequestStatus(session), "none");
  assert.equal(await c.organization.joinRequestStatus(session), "pending");
  assert.deepEqual((await getDoc(queue)).data(), { ...before, guests: ["c"] });
  await assert.rejects(b.organization.cancelJoinRequest(session), /pedido mudou/);
  await b.organization.requestJoin(session);
  assert.deepEqual((await getDoc(queue)).data().guests, ["c", "b"]);
});

test("pending join cancellation rules deny unpaired removals, another guest, metadata changes and accepted grants", async () => {
  const a = service("a"), b = service("b"), c = service("c");
  await a.publishSession({ ...draft, joinPolicy: "approval" }); const session = await ownId("a");
  await b.organization.requestJoin(session); await c.organization.requestJoin(session); await service("d").organization.requestJoin(session);
  const db = dbFor("b"), request = doc(db, "socialJoinRequests", session, "guests", "b"), queue = doc(db, "socialJoinQueues", session);
  const removal = (guests, deleted = ["b"], change = {}) => {
    const batch = writeBatch(db);
    for (const guest of deleted) batch.delete(doc(db, "socialJoinRequests", session, "guests", guest));
    batch.update(queue, { guests, ...change }); return batch.commit();
  };
  await assertFails(deleteDoc(request));
  await assertFails(updateDoc(queue, { guests: ["c"] }));
  await assertFails(deleteDoc(queue));
  await assertFails(removal([], ["b", "c"]));
  await assertFails(removal(["b"], ["c"]));
  await assertFails(removal(["c", "d", "e"]));
  await assertFails(removal(["d", "c"]));
  await assertFails(removal(["c", "d"], ["b"], { hostUid: "b" }));
  await assertFails(removal(["c", "d"], ["b"], { expiresAt: Timestamp.fromMillis(1) }));
  await assertFails(removal(["c", "d"], ["b"], { unexpected: true }));
  assert.equal(await b.organization.joinRequestStatus(session), "pending");
  await a.organization.respondJoin(session, "b", "accept");
  await assert.rejects(b.organization.cancelJoinRequest(session), /pedido mudou/);
  await assertFails(removal(["c", "d"])); await assertFails(deleteDoc(request));
  assert.equal(await b.organization.joinRequestStatus(session), "accepted");
  assert.deepEqual((await getDoc(queue)).data().guests, ["b", "c", "d"]);
  await a.organization.respondJoin(session, "b", "decline");
  assert.deepEqual((await getDoc(queue)).data().guests, ["c", "d"], "host revocation remains explicit and available");
});

test("pending join cancellation racing host acceptance has one atomic winner and never removes an accepted request", async () => {
  const a = service("a"), b = service("b");
  await a.publishSession({ ...draft, joinPolicy: "approval" }); const session = await ownId("a");
  await b.organization.requestJoin(session);
  const [cancel, accept] = await Promise.allSettled([
    b.organization.cancelJoinRequest(session), a.organization.respondJoin(session, "b", "accept"),
  ]);
  assert.equal([cancel, accept].filter(result => result.status === "fulfilled").length, 1);
  const status = await b.organization.joinRequestStatus(session);
  assert.equal(status, cancel.status === "fulfilled" ? "none" : "accepted");
  assert.deepEqual((await getDoc(doc(dbFor("a"), "socialJoinQueues", session))).data().guests, status === "none" ? [] : ["b"]);
});

test("pending join cancellation refuses expired or replaced scope without modifying the retained queue", async () => {
  const a = service("a"), b = service("b");
  await a.publishSession({ ...draft, joinPolicy: "approval" }); const session = await ownId("a");
  await b.organization.requestJoin(session);
  const queue = doc(dbFor("a"), "socialJoinQueues", session), before = (await getDoc(queue)).data();
  const services = sdkServices("b"), oldService = createService(services);
  services.auth.currentUser = { ...services.auth.currentUser };
  await assert.rejects(oldService.organization.cancelJoinRequest(session), /conta mudou/);
  await assert.rejects(b.organization.cancelJoinRequest("11111111-1111-4111-8111-111111111111"));
  await env.withSecurityRulesDisabled(async context => {
    await updateDoc(doc(context.firestore(), "socialJoinRequests", session, "guests", "b"), { expiresAt: Timestamp.fromMillis(1) });
  });
  await assert.rejects(b.organization.cancelJoinRequest(session), /pedido mudou/);
  const rawDb = dbFor("b"), raw = writeBatch(rawDb);
  raw.delete(doc(rawDb, "socialJoinRequests", session, "guests", "b"));
  raw.update(doc(rawDb, "socialJoinQueues", session), { guests: [] });
  await assertFails(raw.commit());
  assert.deepEqual((await getDoc(queue)).data(), before);
});
