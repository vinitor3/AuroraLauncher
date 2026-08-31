import { readFile } from "node:fs/promises";
import { after, before, beforeEach, test } from "node:test";
import assert from "node:assert/strict";

import {
  assertFails,
  assertSucceeds,
  initializeTestEnvironment,
} from "@firebase/rules-unit-testing";
import {
  doc,
  getDoc,
  serverTimestamp,
  setDoc,
  updateDoc,
  writeBatch,
} from "firebase/firestore";

const projectId = "demo-aurora-rules";
let environment;

function profile(uid, overrides = {}) {
  return {
    uid,
    username: "AuroraUser",
    usernameLower: "aurorauser",
    email: "aurorauser@aurora.internal",
    avatarUrl: null,
    skinUrl: null,
    capeUrl: null,
    skinModel: "classic",
    role: "PLAYER",
    createdAt: serverTimestamp(),
    stats: {
      totalPlayTimeSeconds: 0,
      questsCompleted: 0,
      lastPlayedModpack: null,
    },
    ...overrides,
  };
}

async function seedProfile(uid, overrides = {}) {
  await environment.withSecurityRulesDisabled(async (context) => {
    await setDoc(doc(context.firestore(), "users", uid), {
      ...profile(uid, overrides),
      createdAt: new Date("2026-08-30T00:00:00.000Z"),
    });
  });
}

before(async () => {
  const rules = await readFile(new URL("../firestore.rules", import.meta.url), "utf8");
  environment = await initializeTestEnvironment({
    projectId,
    firestore: { rules },
  });
});

beforeEach(async () => {
  await environment.clearFirestore();
});

after(async () => {
  await environment.cleanup();
});

test("permite cadastro inicial PLAYER e reserva de username no mesmo batch", async () => {
  const uid = "user-valid";
  const db = environment.authenticatedContext(uid).firestore();
  const batch = writeBatch(db);
  batch.set(doc(db, "users", uid), profile(uid));
  batch.set(doc(db, "usernames", "aurorauser"), {
    uid,
    usernameLower: "aurorauser",
    createdAt: serverTimestamp(),
  });
  await assertSucceeds(batch.commit());
});

test("nega autoatribuicao de ADMIN", async () => {
  const uid = "user-role";
  const db = environment.authenticatedContext(uid).firestore();
  await assertFails(setDoc(doc(db, "users", uid), profile(uid, { role: "ADMIN" })));
});

test("nega uid, email e usernameLower incoerentes", async () => {
  const uid = "user-identity";
  const db = environment.authenticatedContext(uid).firestore();
  await assertFails(setDoc(doc(db, "users", uid), profile("different-user")));
  await assertFails(setDoc(doc(db, "users", uid), profile(uid, { email: "other@aurora.internal" })));
  await assertFails(setDoc(doc(db, "users", uid), profile(uid, { usernameLower: "Invalid-Name" })));
});

test("nega campos extras, stats infladas e timestamp escolhido pelo cliente", async () => {
  const uid = "user-shape";
  const db = environment.authenticatedContext(uid).firestore();
  await assertFails(setDoc(doc(db, "users", uid), profile(uid, { isAdmin: true })));
  await assertFails(setDoc(doc(db, "users", uid), profile(uid, {
    stats: { totalPlayTimeSeconds: 1, questsCompleted: 0, lastPlayedModpack: null },
  })));
  await assertFails(setDoc(doc(db, "users", uid), profile(uid, {
    createdAt: new Date("2026-08-30T00:00:00.000Z"),
  })));
});

test("preserva uid, usernameLower, role e createdAt em updates", async () => {
  const uid = "user-update";
  await seedProfile(uid);
  const db = environment.authenticatedContext(uid).firestore();
  await assertFails(updateDoc(doc(db, "users", uid), { role: "ADMIN" }));
  await assertFails(updateDoc(doc(db, "users", uid), { uid: "another-user" }));
  await assertFails(updateDoc(doc(db, "users", uid), { usernameLower: "anothername" }));
  await assertFails(updateDoc(doc(db, "users", uid), { createdAt: serverTimestamp() }));
  await assertSucceeds(updateDoc(doc(db, "users", uid), {
    skinModel: "slim",
    updatedAt: serverTimestamp(),
  }));
});

test("nega reserva de username com owner, formato ou shape invalidos", async () => {
  const uid = "user-username";
  await seedProfile(uid);
  const db = environment.authenticatedContext(uid).firestore();
  await assertFails(setDoc(doc(db, "usernames", "aurorauser"), {
    uid: "another-user",
    usernameLower: "aurorauser",
    createdAt: serverTimestamp(),
  }));
  await assertFails(setDoc(doc(db, "usernames", "Invalid-Name"), {
    uid,
    usernameLower: "Invalid-Name",
    createdAt: serverTimestamp(),
  }));
  await assertFails(setDoc(doc(db, "usernames", "aurorauser"), {
    uid,
    usernameLower: "aurorauser",
    createdAt: serverTimestamp(),
    extra: true,
  }));
});

test("nega leitura anonima e leitura do perfil de outro usuario", async () => {
  await seedProfile("owner-user");
  const anonymous = environment.unauthenticatedContext().firestore();
  const other = environment.authenticatedContext("other-user").firestore();
  await assertFails(getDoc(doc(anonymous, "users", "owner-user")));
  await assertFails(getDoc(doc(other, "users", "owner-user")));
  const ownDb = environment.authenticatedContext("owner-user").firestore();
  const snapshot = await assertSucceeds(getDoc(doc(ownDb, "users", "owner-user")));
  assert.equal(snapshot.data().role, "PLAYER");
});
