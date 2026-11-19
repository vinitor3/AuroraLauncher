import { initializeApp, type FirebaseApp } from "firebase/app";
import { browserLocalPersistence, browserSessionPersistence, connectAuthEmulator, createUserWithEmailAndPassword, deleteUser, indexedDBLocalPersistence, initializeAuth, inMemoryPersistence, onAuthStateChanged, signInWithEmailAndPassword, signOut, type Auth, type User } from "firebase/auth";
import { connectFirestoreEmulator, deleteField, doc, getDoc, getFirestore, runTransaction, serverTimestamp, setDoc, updateDoc, type Firestore } from "firebase/firestore";
import { getDownloadURL, getStorage, ref, uploadBytes, type FirebaseStorage } from "firebase/storage";
import { validateLauncherAuthScope, type LauncherAuthScope } from "./launcherAuthScope";

export type FirebasePublicConfig = {
  apiKey: string;
  authDomain: string;
  projectId: string;
  storageBucket: string;
  messagingSenderId: string;
  appId: string;
  workerUrl?: string;
};

export type FirebaseServices = { config: FirebasePublicConfig; app: FirebaseApp; auth: Auth; db: Firestore; storage: FirebaseStorage; authScope: LauncherAuthScope };

const bundledWorkerUrl = import.meta.env.VITE_AURORA_API_URL?.trim();
const officialFirebaseProjectId = "auroralauncher";
const bundledFirebaseProjectId = import.meta.env.VITE_FIREBASE_PROJECT_ID?.trim() || officialFirebaseProjectId;

// This build mode is an isolated, local-only integration harness. It cannot
// enable the unpublished social rules on the real Aurora project.
export const socialLabBuild = import.meta.env.MODE === "social-lab";
// Explicit online profile used by the official distribution and its isolated
// validation builds. Other frontend modes keep Social disabled; this never
// switches online authentication to emulators or configurable relays.
export const socialFeatureBuild = socialLabBuild || import.meta.env.MODE === "connect-online";

/** Private online invitations require a paired, build-owned opt-in and the existing official services. */
export function privateSocialOnlineConfiguration(config: FirebasePublicConfig): boolean {
  return import.meta.env.MODE === "connect-online" && import.meta.env.VITE_AURORA_SOCIAL_PRIVATE_ONLINE === "1"
    && import.meta.env.VITE_AURORA_SOCIAL_ORIGIN === "https://aurora-connect-social.aurora-edge-proxy.workers.dev"
    && config.projectId === "auroralauncher" && config.authDomain === "auroralauncher.firebaseapp.com"
    && config.apiKey === "AIzaSyD4Sn9JkU0CGGz77Mj71QNwvYiFHPM-dvk"
    && config.appId === "1:246542594321:web:ae2a8206d599f2a4c45cb0" && config.messagingSenderId === "246542594321"
    && config.storageBucket === "auroralauncher.firebasestorage.app"
    && config.workerUrl === "https://aurora-api.aurora-edge-proxy.workers.dev";
}

// Estes valores identificam o aplicativo Web no Firebase e são públicos por
// definição. Nunca inclua aqui chaves de conta de serviço ou tokens privados.
export const bundledFirebaseConfig: FirebasePublicConfig = socialLabBuild ? {
  apiKey: "demo-aurora-social", authDomain: "localhost", projectId: "demo-aurora-social",
  storageBucket: "demo-aurora-social.invalid", messagingSenderId: "000000000000",
  appId: "demo-aurora-social",
} : {
  apiKey: import.meta.env.VITE_FIREBASE_API_KEY?.trim() || "AIzaSyD4Sn9JkU0CGGz77Mj71QNwvYiFHPM-dvk",
  authDomain: import.meta.env.VITE_FIREBASE_AUTH_DOMAIN?.trim() || "auroralauncher.firebaseapp.com",
  projectId: bundledFirebaseProjectId,
  storageBucket: import.meta.env.VITE_FIREBASE_STORAGE_BUCKET?.trim() || "auroralauncher.firebasestorage.app",
  messagingSenderId: import.meta.env.VITE_FIREBASE_MESSAGING_SENDER_ID?.trim() || "246542594321",
  appId: import.meta.env.VITE_FIREBASE_APP_ID?.trim() || "1:246542594321:web:ae2a8206d599f2a4c45cb0",
  workerUrl: bundledWorkerUrl || (bundledFirebaseProjectId === officialFirebaseProjectId
    ? "https://aurora-api.aurora-edge-proxy.workers.dev" : undefined),
};

export type AuroraSkinFavorite = {
  id: string;
  name: string;
  skinUrl: string;
  skinModel: "classic" | "slim";
};

export function createFirebaseServices(config: FirebasePublicConfig, authScope: LauncherAuthScope): FirebaseServices {
  if (socialLabBuild && config.projectId !== "demo-aurora-social") {
    throw new Error("Este executável de teste aceita somente o emulador social local.");
  }
  const scope = validateLauncherAuthScope(authScope);
  // Old saved public configs predate workerUrl. Restore this build's matching
  // endpoint without sending another Firebase project's tokens to that Worker.
  const resolvedConfig = socialLabBuild ? { ...bundledFirebaseConfig, workerUrl: undefined }
    : { ...config, workerUrl: config.workerUrl?.trim()
      || (config.projectId === bundledFirebaseConfig.projectId ? bundledFirebaseConfig.workerUrl : undefined) };
  // Keep the primary app name and persistence hierarchy so existing remembered
  // accounts migrate normally. Additional windows never touch that storage key.
  const appName = `aurora-${resolvedConfig.projectId}${scope.persistent ? "" : `-session-${scope.id}`}`;
  const app = initializeApp(resolvedConfig, appName);
  const auth = initializeAuth(app, { persistence: scope.persistent
    ? [indexedDBLocalPersistence, browserLocalPersistence, browserSessionPersistence, inMemoryPersistence]
    : [browserSessionPersistence, inMemoryPersistence] });
  const db = getFirestore(app);
  if (socialLabBuild) {
    connectAuthEmulator(auth, "http://127.0.0.1:9187", { disableWarnings: true });
    connectFirestoreEmulator(db, "127.0.0.1", 8187);
  }
  return {
    config: resolvedConfig,
    app,
    auth,
    authScope: scope,
    db,
    storage: getStorage(app),
  };
}

export type AuroraUserProfile = {
  uid: string;
  username: string;
  usernameLower: string;
  email: string;
  avatarUrl: string | null;
  skinUrl: string | null;
  capeUrl: string | null;
  skinModel: "classic" | "slim";
  skinFavorites: AuroraSkinFavorite[];
  role: "PLAYER" | "CREATOR" | "ADMIN";
};

function normalizeNickname(nickname: string) {
  const normalized = nickname.trim();
  if (!/^[a-zA-Z0-9_]{3,16}$/.test(normalized)) throw new Error("O nick deve ter 3–16 caracteres: letras, números ou _.");
  return normalized.toLowerCase();
}

function syntheticEmail(usernameLower: string) { return `${usernameLower}@aurora.internal`; }

export async function registerAuroraUser(services: FirebaseServices, nickname: string, password: string) {
  const usernameLower = normalizeNickname(nickname);
  const username = nickname.trim();
  if (password.length < 8) throw new Error("A senha deve ter pelo menos 8 caracteres.");
  const credential = await createUserWithEmailAndPassword(services.auth, syntheticEmail(usernameLower), password);
  try {
    await runTransaction(services.db, async (transaction) => {
      const usernameRef = doc(services.db, "usernames", usernameLower);
      if ((await transaction.get(usernameRef)).exists()) throw new Error("Este nick já está em uso.");
      transaction.set(usernameRef, { uid: credential.user.uid, usernameLower, createdAt: serverTimestamp() });
      transaction.set(doc(services.db, "users", credential.user.uid), {
        uid: credential.user.uid, username, usernameLower, email: syntheticEmail(usernameLower),
        avatarUrl: null, skinUrl: null, capeUrl: null, skinModel: "classic", role: "PLAYER",
        createdAt: serverTimestamp(),
        stats: { totalPlayTimeSeconds: 0, questsCompleted: 0, lastPlayedModpack: null },
      });
    });
  } catch (error) { await deleteUser(credential.user); throw error; }
  await syncAuroraPublicProfile(services, {
    uid: credential.user.uid,
    username,
    avatarUrl: null,
    skinUrl: null,
    skinModel: "classic",
  }).catch(() => undefined);
  return credential;
}

export function loginAuroraUser(services: FirebaseServices, nickname: string, password: string) {
  return signInWithEmailAndPassword(services.auth, syntheticEmail(normalizeNickname(nickname)), password);
}

export async function loadAuroraProfile(services: FirebaseServices, user: User): Promise<AuroraUserProfile> {
  const snapshot = await getDoc(doc(services.db, "users", user.uid));
  if (!snapshot.exists()) throw new Error("O perfil Aurora desta conta não existe.");
  const data = snapshot.data() as Partial<AuroraUserProfile>;
  return {
    ...data,
    uid: user.uid,
    skinModel: data.skinModel === "slim" ? "slim" : "classic",
    skinFavorites: Array.isArray(data.skinFavorites) ? data.skinFavorites.slice(0, 24) : [],
  } as AuroraUserProfile;
}

export function observeAuroraSession(services: FirebaseServices, listener: (user: User | null) => void, onError?: () => void) {
  return onAuthStateChanged(services.auth, listener, onError);
}

export function logoutAuroraUser(services: FirebaseServices) { return signOut(services.auth); }

export async function uploadFirebaseAppearanceImage(
  services: FirebaseServices,
  profile: AuroraUserProfile,
  kind: "skin" | "cape",
  file: File,
) {
  const destination = ref(services.storage, `profiles/${profile.uid}/${kind}.png`);
  await uploadBytes(destination, file, { contentType: "image/png", cacheControl: "public,max-age=3600" });
  return getDownloadURL(destination);
}

export type AuroraAppearance = Pick<AuroraUserProfile, "avatarUrl" | "skinUrl" | "capeUrl" | "skinModel">;
export type AuroraPublicProfile = Pick<AuroraUserProfile, "uid" | "username" | "avatarUrl" | "skinUrl" | "skinModel">;

function publicHttpsUrl(value: string) {
  const trimmed = value.trim();
  if (!trimmed) return null;
  try {
    const url = new URL(trimmed);
    if (url.protocol !== "https:") throw new Error();
    return url.toString();
  } catch {
    throw new Error("Use uma URL pública HTTPS ou deixe o campo vazio.");
  }
}

export async function saveAuroraAppearance(
  services: FirebaseServices,
  profile: AuroraUserProfile,
  appearance: AuroraAppearance,
) {
  const next = {
    avatarUrl: publicHttpsUrl(appearance.avatarUrl ?? ""),
    skinUrl: publicHttpsUrl(appearance.skinUrl ?? ""),
    capeUrl: publicHttpsUrl(appearance.capeUrl ?? ""),
    skinModel: appearance.skinModel,
    updatedAt: serverTimestamp(),
  };
  await updateDoc(doc(services.db, "users", profile.uid), next);
  await syncAuroraPublicProfile(services, {
    uid: profile.uid,
    username: profile.username,
    avatarUrl: next.avatarUrl,
    skinUrl: next.skinUrl,
    skinModel: next.skinModel,
  }).catch(() => undefined);
  return { ...profile, ...next, updatedAt: undefined } as AuroraUserProfile;
}

export async function syncAuroraPublicProfile(services: FirebaseServices, profile: AuroraPublicProfile) {
  await setDoc(doc(services.db, "publicProfiles", profile.uid), {
    ...profile,
    updatedAt: serverTimestamp(),
  }, { merge: true });
}

export async function clearRemoteSkinLibrary(services: FirebaseServices, profile: AuroraUserProfile) {
  if (profile.skinFavorites.length === 0) return profile;
  await updateDoc(doc(services.db, "users", profile.uid), {
    skinFavorites: deleteField(),
    updatedAt: serverTimestamp(),
  });
  return { ...profile, skinFavorites: [] };
}
