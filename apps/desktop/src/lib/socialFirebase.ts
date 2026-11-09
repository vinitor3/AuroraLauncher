import { arrayRemove, arrayUnion, collection, doc, documentId, getDocFromServer, getDocsFromServer, increment, limit, orderBy, query, runTransaction, serverTimestamp, startAfter, Timestamp, type DocumentData, type Firestore } from "firebase/firestore";
import type { FirebaseServices } from "./firebase";
import { createSocialOrganization } from "./socialOrganization";
import { normalizeSocialNickname, validateSocialSession, type SocialPerson, type SocialService, type SocialSession } from "../shared/social";

type Kind = "request" | "accept" | "decline" | "cancel" | "remove" | "block";
type State = { friends: string[]; incoming: string[]; outgoing: string[]; blocked: string[] };
const blank = (): State => ({ friends: [], incoming: [], outgoing: [], blocked: [] });
const uidValid = (uid: string) => /^[A-Za-z0-9_-]{1,128}$/.test(uid);
const sessionIdValid = (id: string) => /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(id);
const denied = (cause: unknown) => typeof cause === "object" && cause !== null && "code" in cause && cause.code === "permission-denied";

function parseState(data: DocumentData | undefined): State {
  if (!data) return blank();
  const out = blank();
  for (const field of ["friends", "incoming", "outgoing", "blocked"] as const) {
    const maximum = field === "incoming" || field === "outgoing" ? 10 : 100;
    if (!Array.isArray(data[field]) || data[field].length > maximum || !data[field].every((v: unknown) => typeof v === "string" && uidValid(v))) throw new Error("Estado social inválido.");
    out[field] = data[field];
  }
  return out;
}

function sessionValue(id: string, data: DocumentData | undefined): SocialSession | null {
  if (!data || !sessionIdValid(id) || !uidValid(data.hostUid) || !(data.expiresAt instanceof Timestamp) || data.schema !== 1) return null;
  if (data.expiresAt.toMillis() <= Date.now()) return null;
  try {
    const draft = validateSocialSession({ title: data.title, modpackName: data.modpackName, minecraftVersion: data.minecraftVersion,
      loader: data.loader, imageUrl: data.imageUrl, players: data.players, maxPlayers: data.maxPlayers,
      visibility: data.visibility, joinPolicy: data.joinPolicy });
    return { ...draft, id, hostUid: data.hostUid, expiresAtMs: data.expiresAt.toMillis() };
  } catch { return null; }
}

/** Writes both bounded private containers without reading the peer's friends/requests graph. */
export async function socialRelationshipWrite(db: Firestore, actor: string, peer: string, kind: Kind, check: () => void = () => {}): Promise<void> {
  if (!uidValid(actor) || !uidValid(peer) || actor === peer) throw new Error("Escolha outra conta Aurora.");
  await runTransaction(db, async batch => {
  check();
  const previous = parseState((await batch.get(doc(db, "socialState", actor))).data());
  check();
  for (const [uid, other, own] of [[actor, peer, true], [peer, actor, false]] as const) {
    const removeRequests = ["accept", "decline", "cancel", "block"].includes(kind);
    const friends = kind === "accept" ? arrayUnion(other) : ["remove", "block"].includes(kind) ? arrayRemove(other) : arrayUnion();
    const incoming = removeRequests ? arrayRemove(other) : kind === "request" && !own ? arrayUnion(other) : arrayUnion();
    const outgoing = removeRequests ? arrayRemove(other) : kind === "request" && own ? arrayUnion(other) : arrayUnion();
    const blocked = kind === "block" && own ? arrayUnion(other) : arrayUnion();
    batch.set(doc(db, "socialState", uid), { friends, incoming, outgoing, blocked, revision: increment(1),
      kind, actor, peer: other, updatedAt: serverTimestamp() }, { merge: true });
  }
  if (kind === "request") batch.set(doc(db, "socialThrottle", actor), { at: serverTimestamp() });
  if (kind === "request") batch.set(doc(db, "socialRequests", actor, "peers", peer), {
    createdAt: serverTimestamp(), expiresAt: Timestamp.fromMillis(Date.now() + 7 * 86_400_000 - 5_000),
  });
  else {
    if (previous.incoming.includes(peer)) batch.delete(doc(db, "socialRequests", peer, "peers", actor));
    if (previous.outgoing.includes(peer)) batch.delete(doc(db, "socialRequests", actor, "peers", peer));
  }
  }, { maxAttempts: 1 });
}

/** Explicitly selected adapter. The caller must keep unpublished social rules disabled in normal builds. */
export function createSocialFirebaseService(services: FirebaseServices): SocialService {
  const db = services.db;
  const accountUser = services.auth.currentUser;
  const accountId = accountUser?.uid;
  if (!accountId || !uidValid(accountId)) throw new Error("Entre em sua conta Aurora.");
  function account() {
    if (services.auth.currentUser !== accountUser || services.auth.currentUser?.uid !== accountId) throw new Error("A conta mudou. Abra Amigos novamente.");
    return accountId!;
  }
  async function guarded<T>(work: (check: () => void) => Promise<T>): Promise<T> {
    account();
    const deadline = Date.now() + 15_000;
    let abandoned = false;
    const check = () => { account(); if (abandoned || Date.now() >= deadline) throw Object.assign(new Error("O prazo desta operação terminou. Atualize antes de repetir."), {code:"social_uncertain"}); };
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      const timeout = new Promise<never>((_, reject) => { timer = setTimeout(() => { abandoned = true; reject(Object.assign(
        new Error("A resposta demorou. Uma alteração pode ainda estar em andamento; atualize antes de repetir."), { code: "social_uncertain" })); }, 15_000); });
      const value = await Promise.race([work(check), timeout]); check(); return value;
    }
    catch (cause) {
      account();
      if (denied(cause)) throw Object.assign(new Error("Esta ação social não está disponível. Confira o pedido, bloqueios e limites; o serviço precisa das regras sociais autorizadas."), { code: "permission-denied" });
      throw cause instanceof Error ? cause : new Error("Não foi possível atualizar Amigos agora.");
    } finally { abandoned = true; if (timer !== undefined) clearTimeout(timer); }
  }
  async function optional(collectionName: string, id: string, check: () => void) {
    check();
    try { const result = await getDocFromServer(doc(db, collectionName, id)); check(); return result.data(); }
    catch (cause) { if (denied(cause)) return undefined; throw cause; }
  }
  async function person(uid: string, check: () => void): Promise<SocialPerson> {
    const profile = await optional("publicProfiles", uid, check);
    let nickname = "Perfil indisponível";
    if (profile?.uid === uid && typeof profile.username === "string") {
      try {
        const lower = normalizeSocialNickname(profile.username);
        const registry = await optional("usernames", lower, check);
        if (registry?.uid === uid && registry.usernameLower === lower) nickname = profile.username;
      } catch { /* A malformed public display name is never an account identity. */ }
    }
    return { uid, nickname, avatarUrl: null }; // Existing skin/avatar URLs are not approved social image origins.
  }
  async function sessionForHost(uid: string, check: () => void) {
    const pointer = await optional("socialOwners", uid, check);
    if (!pointer || !sessionIdValid(pointer.sessionId)) return null;
    const value = sessionValue(pointer.sessionId, await optional("socialSessions", pointer.sessionId, check));
    return value?.hostUid === uid ? value : null;
  }
  const relationship = (peer: string, kind: Kind) => guarded(check => socialRelationshipWrite(db, account(), peer, kind, check));
  return {
    capabilities: Object.freeze({ friends: true, catalog: true, publication: true, join: false }),
    organization: createSocialOrganization(db, account, guarded, person),
    profile: uid => guarded(async check => {
      if (!uidValid(uid)) throw new Error("Conta Aurora inválida.");
      const value = await person(uid, check);
      check();
      return value.nickname === "Perfil indisponível" ? null : value;
    }),
    searchByNickname: (value) => guarded(async (check) => {
      check();
      const nickname = normalizeSocialNickname(value);
      const registry = await getDocFromServer(doc(db, "usernames", nickname));
      check();
      const data = registry.data();
      if (!data || !uidValid(data.uid) || data.usernameLower !== nickname) return null;
      if (data.uid === accountId) return null;
      const own = parseState((await getDocFromServer(doc(db, "socialState", account()))).data());
      check();
      if (own.blocked.includes(data.uid)) return null;
      // The registry, not an arbitrary public display name or Minecraft API, owns this nick.
      return { uid: data.uid, nickname, avatarUrl: null };
    }),
    snapshot: () => guarded(async (check) => {
      check();
      const state = parseState((await getDocFromServer(doc(db, "socialState", account()))).data());
      const friends = [];
      // Bounded sequential reads avoid turning 100 friends into an unbounded burst.
      for (const uid of state.friends) {
        check();
        const profile = await person(uid, check);
        const p = await optional("socialPresence", uid, check);
        const live = p?.sharing === "friends" && p.expiresAt instanceof Timestamp && p.expiresAt.toMillis() > Date.now()
          && p.updatedAt instanceof Timestamp && (p.modpackName === null || typeof p.modpackName === "string" && p.modpackName.length <= 64);
        friends.push({ ...profile, presence: live ? { modpackName: p.modpackName, updatedAtMs: p.updatedAt.toMillis() } : null,
          session: await sessionForHost(uid, check) });
      }
      const people = async (uids: string[]) => { const result = []; for (const uid of uids) result.push(await person(uid, check)); return result; };
      const requests = async (uids: string[], incoming: boolean) => {
        const result = [];
        for (const uid of uids) {
          check();
          const request = (await getDocFromServer(doc(db, "socialRequests", incoming ? uid : account(), "peers", incoming ? account() : uid))).data();
          result.push({ ...await person(uid, check), expiresAtMs: request?.expiresAt instanceof Timestamp ? request.expiresAt.toMillis() : 0 });
        }
        return result;
      };
      const ownPresence = await optional("socialPresence", account(), check);
      return { friends, incoming: await requests(state.incoming, true), outgoing: await requests(state.outgoing, false), blocked: await people(state.blocked),
        ownSession: await sessionForHost(account(), check), presenceSharing: ownPresence?.sharing === "friends" ? "friends" as const : "hidden" as const,
        modpackName: typeof ownPresence?.modpackName === "string" ? ownPresence.modpackName : null };
    }),
    requestFriend: (uid) => relationship(uid, "request"),
    respondFriend: (uid, decision) => relationship(uid, decision === "accept" ? "accept" : "decline"),
    cancelRequest: (uid) => relationship(uid, "cancel"),
    removeFriend: (uid) => relationship(uid, "remove"),
    block: (uid) => relationship(uid, "block"),
    unblock: (uid) => guarded(async (check) => {
      check();
      if (!uidValid(uid) || uid === account()) throw new Error("Escolha outra conta Aurora.");
      await runTransaction(db, async transaction => {
        check(); const ref = doc(db, "socialState", account()); await transaction.get(ref); check();
        transaction.set(ref, { blocked: arrayRemove(uid), kind: "unblock", actor: account(), peer: uid,
          revision: increment(1), updatedAt: serverTimestamp() }, { merge: true });
      }, { maxAttempts: 1 });
    }),
    setPresence: (value) => guarded(async (check) => {
      check();
      if (!["friends", "hidden"].includes(value.sharing) || value.modpackName !== null && (typeof value.modpackName !== "string" || value.modpackName.length > 64)) throw new Error("Presença inválida.");
      await runTransaction(db, async transaction => {
        check(); const ref = doc(db, "socialPresence", account()); await transaction.get(ref); check();
        transaction.set(ref, { ...value, updatedAt: serverTimestamp(), expiresAt: Timestamp.fromMillis(Date.now() + 115_000) });
      }, { maxAttempts: 1 });
    }),
    publishSession: (input) => guarded(async (check) => {
      check();
      const value = validateSocialSession(input);
      const owner = account();
      await runTransaction(db, async (transaction) => {
        check();
        const ownerRef = doc(db, "socialOwners", owner);
        const current = (await transaction.get(ownerRef)).data();
        check();
        const id = current?.sessionId ?? crypto.randomUUID();
        if (!sessionIdValid(id)) throw new Error("Sessão anterior inválida.");
        const queue = current ? (await transaction.get(doc(db, "socialJoinQueues", id))).data() : undefined; check();
        if (queue) {
          for (const guest of queue.guests ?? []) transaction.delete(doc(db, "socialJoinRequests", id, "guests", guest));
          transaction.delete(doc(db, "socialJoinQueues", id));
        }
        const expiresAt = Timestamp.fromMillis(Date.now() + 14_395_000);
        transaction.set(ownerRef, { sessionId: id, visibility: value.visibility });
        transaction.set(doc(db, "socialSessions", id), { ...value, schema: 1, hostUid: owner, expiresAt });
        if (value.visibility === "public") transaction.set(doc(db, "socialCatalog", id), { schema: 1, expiresAt });
        else if (current?.visibility === "public") transaction.delete(doc(db, "socialCatalog", id));
      }, { maxAttempts: 1 });
    }),
    closeSession: () => guarded(async (check) => {
      check();
      const owner = account();
      await runTransaction(db, async (transaction) => {
        check();
        const ownerRef = doc(db, "socialOwners", owner);
        const current = (await transaction.get(ownerRef)).data();
        check();
        if (!current) return;
        if (!sessionIdValid(current.sessionId)) throw new Error("Sessão anterior inválida.");
        const queue = (await transaction.get(doc(db, "socialJoinQueues", current.sessionId))).data(); check();
        if (queue) {
          for (const guest of queue.guests ?? []) transaction.delete(doc(db, "socialJoinRequests", current.sessionId, "guests", guest));
          transaction.delete(doc(db, "socialJoinQueues", current.sessionId));
        }
        transaction.delete(doc(db, "socialSessions", current.sessionId));
        if (current.visibility === "public") transaction.delete(doc(db, "socialCatalog", current.sessionId));
        transaction.delete(ownerRef);
      }, { maxAttempts: 1 });
    }),
    listPublicSessions: (cursor) => guarded(async (check) => {
      check();
      if (cursor !== undefined && cursor !== null && !sessionIdValid(cursor)) throw new Error("Página inválida.");
      const base = collection(db, "socialCatalog");
      const q = cursor ? query(base, orderBy(documentId()), startAfter(cursor), limit(20)) : query(base, orderBy(documentId()), limit(20));
      const index = await getDocsFromServer(q);
      const items: SocialSession[] = [];
      for (const entry of index.docs) {
        account();
        const data = entry.data();
        if (!sessionIdValid(entry.id) || data.schema !== 1 || !(data.expiresAt instanceof Timestamp) || data.expiresAt.toMillis() <= Date.now()) continue;
        const value = sessionValue(entry.id, await optional("socialSessions", entry.id, check));
        if (value?.visibility === "public") items.push(value);
      }
      return { items, nextCursor: index.size === 20 ? index.docs.at(-1)!.id : null };
    }),
    prepareJoin: (_sessionId) => guarded(async (check) => {
      check(); throw new Error("A entrada automática ainda precisa do vínculo seguro entre conta e conexão. Nenhuma conexão foi aberta."); }),
  };
}
