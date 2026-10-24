// Disposable local accounts only. Never imports a saved launcher config or credentials.
import { initializeApp, deleteApp } from "firebase/app";
import { connectAuthEmulator, createUserWithEmailAndPassword, getAuth, signInWithEmailAndPassword, signOut } from "firebase/auth";
import { connectFirestoreEmulator, doc, getDoc, getFirestore, runTransaction, serverTimestamp, setDoc, terminate } from "firebase/firestore";

export const projectId = "demo-aurora-social";
export const fixturePassword = "aurora-local-fixture-24"; // Public, emulator-only fixture; never a real account.
export async function openFixtureAccount(nickname) {
  if (!/^social_(alfa|beta|gama)$/.test(nickname)) throw new Error("LOCAL_FIXTURE_ACCOUNT_ONLY");
  const app = initializeApp({ projectId, apiKey: projectId }, `fixture-${nickname}-${crypto.randomUUID()}`);
  const auth = getAuth(app);
  connectAuthEmulator(auth, "http://127.0.0.1:9187", { disableWarnings: true });
  const db = getFirestore(app);
  connectFirestoreEmulator(db, "127.0.0.1", 8187);
  const email = `${nickname}@aurora.internal`;
  let credential;
  try { credential = await createUserWithEmailAndPassword(auth, email, fixturePassword); }
  catch (error) {
    if (error.code !== "auth/email-already-in-use") throw error;
    credential = await signInWithEmailAndPassword(auth, email, fixturePassword);
  }
  const uid = credential.user.uid;
  const profile = { uid, username: nickname, usernameLower: nickname, email,
    avatarUrl: null, skinUrl: null, capeUrl: null, skinModel: "classic", role: "PLAYER",
    createdAt: serverTimestamp(), stats: { totalPlayTimeSeconds: 0, questsCompleted: 0, lastPlayedModpack: null } };
  if (!(await getDoc(doc(db, "users", uid))).exists()) await runTransaction(db, async tx => {
    const nameRef = doc(db, "usernames", nickname);
    if ((await tx.get(nameRef)).exists()) throw new Error("FIXTURE_NICK_CONFLICT");
    tx.set(nameRef, { uid, usernameLower: nickname, createdAt: serverTimestamp() });
    tx.set(doc(db, "users", uid), profile);
  });
  await setDoc(doc(db, "publicProfiles", uid), { uid, username: nickname, avatarUrl: null, skinUrl: null, skinModel: "classic", updatedAt: serverTimestamp() });
  return { services: { auth, db }, uid, nickname, async close() { await signOut(auth); await terminate(db); await deleteApp(app); } };
}

if (process.argv[1] && new URL(import.meta.url).pathname.replace(/^\//, "").toLowerCase() === process.argv[1].replaceAll("\\", "/").toLowerCase()) {
  for (const name of ["social_alfa", "social_beta", "social_gama"]) {
    const fixture = await openFixtureAccount(name);
    await fixture.close();
  }
  console.log(JSON.stringify({ event: "social_local_accounts_ready", count: 3, project: projectId, remoteWrites: false }));
}
