// Causal client lifecycle fixtures, separate from actual Firestore Rules tests.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { before, test } from "node:test";
const requireDesktop = createRequire(new URL("../../apps/desktop/package.json", import.meta.url));
let createService, shared;
const fixtureKey = `__aurora_social_boundary_${process.pid}`;
const dataUrl = source => `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
before(async () => {
  const ts = requireDesktop("typescript");
  const transpile = source => ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext}}).outputText;
  const sharedUrl=dataUrl(transpile(await readFile(new URL("../../apps/desktop/src/shared/social.ts",import.meta.url),"utf8")));
  shared=await import(sharedUrl);
  const fakeSdk=dataUrl(`
    const fixture=()=>globalThis[${JSON.stringify(fixtureKey)}];
    export function doc(...args){return args.slice(1);}
    export async function getDocFromServer(ref){return fixture().read(ref);}
    export async function runTransaction(db, work, options){return fixture().transaction(work, options);}
    export class Timestamp { static fromMillis(ms){ return {toMillis:()=>ms}; } }
    const unused=()=>{throw Error("Unexpected SDK operation");};
    export const arrayRemove=unused,arrayUnion=unused,collection=unused,documentId=unused,getDocsFromServer=unused,
      increment=unused,limit=unused,orderBy=unused,query=unused,serverTimestamp=unused,startAfter=unused,deleteDoc=unused,where=unused;
  `);
  const organization=dataUrl(transpile(await readFile(new URL("../../apps/desktop/src/lib/socialOrganization.ts",import.meta.url),"utf8")).replace('"firebase/firestore"',JSON.stringify(fakeSdk)));
  // Clock bounds/authentication have their own real implementation fixtures; this
  // suite isolates the following SDK request/account lifecycle.
  const clock=dataUrl('export const createSocialServerClock=()=>({ensure:async check=>check(),now:Date.now,writeTime:Date.now});');
  const source=transpile(await readFile(new URL("../../apps/desktop/src/lib/socialFirebase.ts",import.meta.url),"utf8"))
    .replace('"./socialServerClock"',JSON.stringify(clock)).replace('"./firebase"',JSON.stringify('data:text/javascript,export const socialLabBuild=true'))
    .replace('"./socialOrganization"',JSON.stringify(organization)).replace('"../shared/social"',JSON.stringify(sharedUrl)).replace('"firebase/firestore"',JSON.stringify(fakeSdk));
  createService=(await import(dataUrl(source))).createSocialFirebaseService;
});
function pendingRead() {
  const calls=[]; let finish;
  globalThis[fixtureKey]={read(ref){calls.push(ref);return new Promise(resolve=>{finish=resolve;});}};
  return {calls,resolve(data){finish({data:()=>data});}};
}
test("account changes during a pending exact lookup cannot issue the second read or publish a person",async()=>{
  const pending=pendingRead(),auth={currentUser:{uid:"a"}},service=createService({db:{},auth});
  const result=service.searchByNickname("User_b");
  await new Promise(resolve=>setImmediate(resolve));
  assert.equal(pending.calls.length,1);auth.currentUser={uid:"c"};
  pending.resolve({uid:"b",usernameLower:"user_b"});
  await assert.rejects(result,/conta mudou/);
  assert.equal(pending.calls.length,1);
  delete globalThis[fixtureKey];
});
test("deadline poisons a pending operation even after wall clock rollback; late result starts no new I/O",async()=>{
  const pending=pendingRead(),service=createService({db:{},auth:{currentUser:{uid:"a"}}});
  const originalSetTimeout=globalThis.setTimeout,originalNow=Date.now;
  let timeout;
  globalThis.setTimeout=(callback,delay)=>{assert.equal(delay,15000);timeout=callback;return 0;};
  const result=service.searchByNickname("User_b");
  globalThis.setTimeout=originalSetTimeout;
  try {
    await new Promise(resolve=>setImmediate(resolve));
    timeout();
    await assert.rejects(result,cause=>cause.code==="social_uncertain");
    Date.now=()=>1;
    pending.resolve({uid:"b",usernameLower:"user_b"});
    await new Promise(resolve=>setImmediate(resolve));
    assert.equal(pending.calls.length,1);
  } finally {Date.now=originalNow;globalThis.setTimeout=originalSetTimeout;delete globalThis[fixtureKey];}
});
test("social projections accept only complete Aurora nick and bounded existing catalog images",()=>{
  for(const nick of ["", "ab", "player-name", "a".repeat(17), "<script>"]) assert.throws(()=>shared.normalizeSocialNickname(nick));
  assert.equal(shared.normalizeSocialNickname(" Aurora_User "),"aurora_user");
  for(const url of ["http://cdn.modrinth.com/a.png","https://cdn.modrinth.com@evil.test/a.png","https://localhost/a.png",
    "https://cdn.modrinth.com/a.svg","https://cdn.modrinth.com/a.png?track=1"])
    assert.equal(shared.socialImageUrl(url),null);
  assert.equal(shared.socialImageUrl("https://cdn.modrinth.com/data/project/icon.png"),"https://cdn.modrinth.com/data/project/icon.png");
});

test("roster display resolves only a matching exact Aurora public profile and registry",async()=>{
  const calls=[];
  globalThis[fixtureKey]={async read(ref){calls.push(ref);return {data:()=>ref[0]==="publicProfiles"
    ?{uid:"b",username:"Aurora_B"}:{uid:"b",usernameLower:"aurora_b"}};}};
  try {
    const service=createService({db:{},auth:{currentUser:{uid:"a"}}});
    assert.deepEqual(await service.profile("b"),{uid:"b",nickname:"Aurora_B",avatarUrl:null});
    assert.deepEqual(calls,[["publicProfiles","b"],["usernames","aurora_b"]]);
    globalThis[fixtureKey]={async read(ref){return {data:()=>ref[0]==="publicProfiles"?{uid:"b",username:"Aurora_B"}:{uid:"someone_else",usernameLower:"aurora_b"}};}};
    assert.equal(await service.profile("b"),null);
    await assert.rejects(service.profile("../users"));
  } finally {delete globalThis[fixtureKey];}
});

test("a new login of the same UID cannot publish a pending old roster profile",async()=>{
  const pending=pendingRead(),auth={currentUser:{uid:"a"}},service=createService({db:{},auth});
  const result=service.profile("b");
  await new Promise(resolve=>setImmediate(resolve));
  auth.currentUser={uid:"a"};
  pending.resolve({uid:"b",username:"Aurora_B"});
  await assert.rejects(result,/conta mudou/);
  assert.equal(pending.calls.length,1);
  delete globalThis[fixtureKey];
});

test("pending join cancellation stops before another read or write when the original account changes",async()=>{
  for (const nextUid of ["c", "b"]) {
    const pending=pendingRead(),writes=[],auth={currentUser:{uid:"b"}},service=createService({db:{},auth});
    globalThis[fixtureKey].transaction=async(work,options)=>{
      assert.equal(options.maxAttempts,1);
      await work({get:ref=>globalThis[fixtureKey].read(ref),delete:ref=>writes.push(ref),update:ref=>writes.push(ref)});
    };
    try {
      const result=service.organization.cancelJoinRequest("11111111-1111-4111-8111-111111111111");
      await new Promise(resolve=>setImmediate(resolve));
      assert.deepEqual(pending.calls,[["socialJoinRequests","11111111-1111-4111-8111-111111111111","guests","b"]]);
      auth.currentUser={uid:nextUid};
      pending.resolve({status:"pending",guestUid:"b"});
      await assert.rejects(result,/conta mudou/);
      assert.equal(pending.calls.length,1);assert.deepEqual(writes,[]);
    } finally {delete globalThis[fixtureKey];}
  }
});
