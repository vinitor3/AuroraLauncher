import { initializeApp, type FirebaseApp } from "firebase/app";
import { createUserWithEmailAndPassword, deleteUser, getAuth, onAuthStateChanged, signInWithEmailAndPassword, signOut, type Auth, type User } from "firebase/auth";
import { deleteField, doc, getDoc, getFirestore, runTransaction, serverTimestamp, updateDoc, type Firestore } from "firebase/firestore";
import { getDownloadURL, getStorage, ref, uploadBytes, type FirebaseStorage } from "firebase/storage";

export type FirebasePublicConfig = {
  apiKey: string;
  authDomain: string;
  projectId: string;
  storageBucket: string;
  messagingSenderId: string;
  appId: string;
  workerUrl?: string;
};

export type FirebaseServices = { config: FirebasePublicConfig; app: FirebaseApp; auth: Auth; db: Firestore; storage: FirebaseStorage };

const bundledWorkerUrl = import.meta.env.VITE_AURORA_API_URL?.trim();

export type AuroraSkinFavorite = {
  id: string;
  name: string;
  skinUrl: string;
  skinModel: "classic" | "slim";
};

export function createFirebaseServices(config: FirebasePublicConfig): FirebaseServices {
  const resolvedConfig = { ...config, workerUrl: config.workerUrl?.trim() || bundledWorkerUrl || undefined };
  const app = initializeApp(resolvedConfig, `aurora-${resolvedConfig.projectId}`);
  return {
    config: resolvedConfig,
    app,
    auth: getAuth(app),
    db: getFirestore(app),
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

export function observeAuroraSession(services: FirebaseServices, listener: (user: User | null) => void) {
  return onAuthStateChanged(services.auth, listener);
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
  return { ...profile, ...next, updatedAt: undefined } as AuroraUserProfile;
}

export async function clearRemoteSkinLibrary(services: FirebaseServices, profile: AuroraUserProfile) {
  if (profile.skinFavorites.length === 0) return profile;
  await updateDoc(doc(services.db, "users", profile.uid), {
    skinFavorites: deleteField(),
    updatedAt: serverTimestamp(),
  });
  return { ...profile, skinFavorites: [] };
}
