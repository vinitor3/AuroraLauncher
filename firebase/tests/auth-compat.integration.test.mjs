// Executes the launcher's real Auth/Firestore functions against local emulators.
// No production account, Firebase config file or installed application is touched.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { test } from "node:test";
import { initializeTestEnvironment, assertFails } from "@firebase/rules-unit-testing";
import { initializeApp, deleteApp } from "firebase/app";
import { applyActionCode, confirmPasswordReset, connectAuthEmulator, getAuth, verifyPasswordResetCode } from "firebase/auth";
import { collection, connectFirestoreEmulator, doc, getDocFromServer, getDocs, getFirestore, setDoc, terminate } from "firebase/firestore";

const desktopRequire = createRequire(new URL("../../apps/desktop/package.json", import.meta.url));
const dataUrl = source => `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
async function realLauncherFunctions() {
  const ts = desktopRequire("typescript");
  const compile = source => ts.transpileModule(source, { compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext,
  } }).outputText;
  const scope = dataUrl(compile(await readFile(new URL("../../apps/desktop/src/lib/launcherAuthScope.ts", import.meta.url), "utf8")));
  let code = compile((await readFile(new URL("../../apps/desktop/src/lib/firebase.ts", import.meta.url), "utf8")).replaceAll("import.meta.env", "({})"))
    .replace('"./launcherAuthScope"', JSON.stringify(scope));
  for (const sdk of ["firebase/app", "firebase/auth", "firebase/firestore", "firebase/storage"])
    code = code.replace(JSON.stringify(sdk), JSON.stringify(import.meta.resolve(sdk)));
  return import(dataUrl(code));
}

async function recoveryCodes(projectId) {
  const response = await fetch(`http://127.0.0.1:9187/emulator/v1/projects/${projectId}/oobCodes`);
  assert.equal(response.status, 200);
  return (await response.json()).oobCodes ?? [];
}

test("normal launcher registration, login, existing profile and appearance remain compatible with online rules", { timeout: 30000 }, async () => {
  const projectId = "demo-aurora-auth-compat";
  const env = await initializeTestEnvironment({ projectId, firestore: { host: "127.0.0.1", port: 8187,
    rules: await readFile(new URL("../firestore.rules", import.meta.url), "utf8") } });
  const app = initializeApp({ projectId, apiKey: projectId }, `auth-compat-${crypto.randomUUID()}`);
  const auth = getAuth(app);
  connectAuthEmulator(auth, "http://127.0.0.1:9187", { disableWarnings: true });
  const db = getFirestore(app); connectFirestoreEmulator(db, "127.0.0.1", 8187);
  const services = { app, auth, db };
  const functions = await realLauncherFunctions();
  const nickname = `Aur_${crypto.randomUUID().replaceAll("-", "").slice(0, 10)}`;
  const password = "local-only-auth-compat-fixture";
  try {
    const credential = await functions.registerAuroraUser(services, ` ${nickname} `, password);
    const uid = credential.user.uid;
    const profile = await functions.loadAuroraProfile(services, credential.user);
    assert.equal(profile.username, nickname);
    assert.equal(profile.usernameLower, nickname.toLowerCase());
    assert.equal(profile.email, `${nickname.toLowerCase()}@aurora.internal`);
    assert.equal(profile.role, "PLAYER");
    assert.deepEqual(profile.skinFavorites, []);
    assert.equal((await getDocFromServer(doc(db, "usernames", nickname.toLowerCase()))).data().uid, uid);
    assert.equal((await getDocFromServer(doc(db, "publicProfiles", uid))).data().username, nickname);
    for (const collectionName of ["publicProfiles", "usernames"])
      await assertFails(getDocs(collection(db, collectionName)));

    const saved = await functions.saveAuroraAppearance(services, profile, {
      avatarUrl: "https://example.invalid/avatar.png", skinUrl: "https://example.invalid/skin.png", capeUrl: null, skinModel: "slim",
    });
    assert.equal(saved.skinModel, "slim");
    assert.equal((await getDocFromServer(doc(db, "publicProfiles", uid))).data().skinModel, "slim");
    await functions.logoutAuroraUser(services);
    assert.equal(auth.currentUser, null);
    await assertFails(getDocFromServer(doc(db, "publicProfiles", uid)));
    const returned = await functions.loginAuroraUser(services, nickname.toUpperCase(), password);
    assert.equal(returned.user.uid, uid);
    assert.equal((await functions.loadAuroraProfile(services, returned.user)).skinModel, "slim");

    // The new creation predicate must not retroactively reject older documents.
    await env.withSecurityRulesDisabled(async context => {
      await setDoc(doc(context.firestore(), "users", uid), { ...profile, role: "CREATOR",
        createdAt: new Date("2026-01-01T00:00:00Z"), legacyField: true,
        skinFavorites: [{ id: "old", name: "Old", skinUrl: "https://example.invalid/old.png", skinModel: "classic" }],
      });
    });
    const legacy = await functions.loadAuroraProfile(services, returned.user);
    assert.equal(legacy.role, "CREATOR");
    assert.equal(legacy.skinFavorites.length, 1);
    await functions.saveAuroraAppearance(services, legacy, { avatarUrl: null, skinUrl: null, capeUrl: null, skinModel: "classic" });
    await functions.clearRemoteSkinLibrary(services, legacy);
    assert.deepEqual((await functions.loadAuroraProfile(services, returned.user)).skinFavorites, []);

    const realEmail = `recovery-${crypto.randomUUID()}@example.com`;
    await assert.rejects(functions.resetAuroraPassword(services, profile.email), /e-mail pessoal/);
    await assert.rejects(functions.linkAuroraRecoveryEmail(services, "another-uid", password, realEmail));
    await assert.rejects(functions.linkAuroraRecoveryEmail(services, uid, "invalid-fixture-password", realEmail));
    assert.equal((await recoveryCodes(projectId)).some(code => code.newEmail === realEmail), false);
    await functions.linkAuroraRecoveryEmail(services, uid, password, realEmail);
    const change = (await recoveryCodes(projectId)).find(code => code.requestType === "VERIFY_AND_CHANGE_EMAIL" && (code.email === realEmail || code.newEmail === realEmail));
    assert.ok(change, "Firebase must issue a confirmation to the new address");
    assert.equal(auth.currentUser.email, profile.email, "an unconfirmed address cannot become the account login");
    await applyActionCode(auth, change.oobCode);
    await assert.rejects(applyActionCode(auth, change.oobCode), "the change-email code is single-use");
    await functions.logoutAuroraUser(services);
    const migrated = await functions.loginAuroraUser(services, realEmail, password);
    assert.equal(migrated.user.uid, uid);
    assert.equal(functions.verifiedAuroraEmail(migrated.user), realEmail);
    assert.equal((await functions.loadAuroraProfile(services, migrated.user)).role, "CREATOR");
    const publicProfile = await getDocFromServer(doc(db, "publicProfiles", uid));
    assert.equal(JSON.stringify(publicProfile.data()).includes(realEmail), false);
    assert.equal((await getDocFromServer(doc(db, "usernames", nickname.toLowerCase()))).data().uid, uid);
    await functions.logoutAuroraUser(services);
    await assert.rejects(functions.loginAuroraUser(services, nickname, password));
    await functions.resetAuroraPassword(services, `unknown-${crypto.randomUUID()}@example.com`);
    await functions.resetAuroraPassword(services, realEmail);
    const reset = (await recoveryCodes(projectId)).find(code => code.requestType === "PASSWORD_RESET" && code.email === realEmail);
    assert.ok(reset, "Firebase must issue a password reset for the verified address");
    assert.equal(await verifyPasswordResetCode(auth, reset.oobCode), realEmail);
    const newPassword = "new-unique-local-recovery-fixture";
    await confirmPasswordReset(auth, reset.oobCode, newPassword);
    await assert.rejects(confirmPasswordReset(auth, reset.oobCode, password));
    await assert.rejects(functions.loginAuroraUser(services, realEmail, password));
    const restored = await functions.loginAuroraUser(services, realEmail, newPassword);
    assert.equal(restored.user.uid, uid);
    await functions.logoutAuroraUser(services);

    const unavailable = `Taken_${crypto.randomUUID().replaceAll("-", "").slice(0, 8)}`;
    await env.withSecurityRulesDisabled(context => setDoc(doc(context.firestore(), "usernames", unavailable.toLowerCase()), {
      uid: "preexisting-owner", usernameLower: unavailable.toLowerCase(), createdAt: new Date(),
    }));
    await assert.rejects(functions.registerAuroraUser(services, unavailable, password), /já está em uso/);
    assert.equal(auth.currentUser, null, "A rejected registry reservation deletes only the just-created local Auth user");
    await assert.rejects(functions.loginAuroraUser(services, unavailable, password));
  } finally {
    await functions.logoutAuroraUser(services).catch(() => {});
    await terminate(db); await deleteApp(app); await env.cleanup();
  }
});
