import { collection, doc, getDocFromServer, getDocsFromServer, limit, query, runTransaction, serverTimestamp, Timestamp, where, type DocumentData, type Firestore } from "firebase/firestore";
import type { SocialPerson, SocialPrivateInvitationService, SocialPrivateInvitationSnapshot, SocialSession } from "../shared/social";

type Guarded = <T>(work: (check: () => void) => Promise<T>) => Promise<T>;
type Authorization = { schema: 1; authorizationId: string; revision: number; sessionId: string; hostUid: string; recipientUid: string;
  status: "active" | "revoked"; createdAt: Timestamp; updatedAt: Timestamp; expiresAt: Timestamp };
type Audience = { schema: 1; hostUid: string; sessionId: string; expiresAt: Timestamp; recipients: string[] };
const validUid = (value: unknown): value is string => typeof value === "string" && /^[A-Za-z0-9_-]{1,128}$/.test(value);
const validSession = (value: unknown): value is string => typeof value === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(value);
const changed = () => new Error("O convite ou a sessão mudou. Atualize antes de tentar novamente.");
const exact = (value: DocumentData, keys: string[]) => Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const expiration = (value: unknown, now: number) => value instanceof Timestamp && Number.isFinite(now) && value.toMillis() > now;
function authorization(value: DocumentData | undefined, session: string, recipient: string, now: number): Authorization | null {
  if (!value || !exact(value, ["schema", "authorizationId", "revision", "sessionId", "hostUid", "recipientUid", "status", "createdAt", "updatedAt", "expiresAt"])
    || value.schema !== 1 || !validSession(session) || value.sessionId !== session || !validUid(value.hostUid) || !validUid(recipient)
    || value.recipientUid !== recipient || value.hostUid === recipient || typeof value.authorizationId !== "string" || !/^[0-9a-f]{64}$/.test(value.authorizationId)
    || !Number.isInteger(value.revision) || value.revision < 1 || value.revision > 0xffffffff || !["active", "revoked"].includes(value.status)
    || !(value.createdAt instanceof Timestamp) || !(value.updatedAt instanceof Timestamp) || !expiration(value.expiresAt, now)
    || value.createdAt.toMillis() > value.updatedAt.toMillis() || value.updatedAt.toMillis() >= value.expiresAt.toMillis()) return null;
  return value as Authorization;
}
function audience(value: DocumentData | undefined, session: SocialSession): Audience | null {
  if (!value || !exact(value, ["schema", "hostUid", "sessionId", "expiresAt", "recipients"]) || value.schema !== 1
    || value.hostUid !== session.hostUid || value.sessionId !== session.id || !(value.expiresAt instanceof Timestamp)
    || value.expiresAt.toMillis() !== session.expiresAtMs || !Array.isArray(value.recipients) || value.recipients.length > 8
    || !value.recipients.every(validUid) || new Set(value.recipients).size !== value.recipients.length || value.recipients.includes(session.hostUid)) return null;
  return value as Audience;
}
function pointer(value: DocumentData | undefined, id: string, now: number) {
  return !!value && exact(value, ["schema", "sessionId", "hostUid", "expiresAt"]) && value.schema === 1 && validSession(id)
    && value.sessionId === id && validUid(value.hostUid) && expiration(value.expiresAt, now);
}

/** Instantiated only by the explicitly gated emulator SDK. No transport fields leave this adapter. */
export function createSocialPrivateInvitations(db: Firestore, account: () => string, guarded: Guarded,
  person: (uid: string, check: () => void) => Promise<SocialPerson>, clock: { now(): number },
  parseSession: (id: string, value: DocumentData | undefined, now: number) => SocialSession | null): SocialPrivateInvitationService {
  const authRef = (id: string, peer: string) => doc(db, "socialPrivateInvitations", id, "guests", peer);
  const audienceRef = (id: string) => doc(db, "socialPrivateAudiences", id);
  const inboxRef = (id: string, peer: string) => doc(db, "socialPrivateInbox", peer, "sessions", id);
  const privateSession = (id: string, value: DocumentData | undefined) => {
    const session = parseSession(id, value, clock.now());
    return session?.visibility === "private" && session.joinPolicy === "open" ? session : null;
  };
  const optional = async (ref: ReturnType<typeof doc>, check: () => void) => {
    check();
    try { const value = (await getDocFromServer(ref)).data(); check(); return value; }
    catch (error) { check(); if ((error as { code?: string }).code === "permission-denied") return undefined; throw error; }
  };
  async function withdraw(sessionId: string, recipient: string, isHost: boolean) {
    if (!validSession(sessionId) || !validUid(recipient)) throw changed();
    await guarded(async check => {
      const actor = account();
      await runTransaction(db, async tx => {
        check(); const value = authorization((await tx.get(authRef(sessionId, recipient))).data(), sessionId, recipient, clock.now()); check();
        if (!value || value.status !== "active" || value.revision === 0xffffffff || (isHost ? value.hostUid !== actor : recipient !== actor)) throw changed();
        tx.update(authRef(sessionId, recipient), { status: "revoked", revision: value.revision + 1, updatedAt: serverTimestamp() });
        tx.delete(inboxRef(sessionId, recipient));
      }, { maxAttempts: 1 }); check();
    });
  }
  return {
    snapshot: () => guarded(async check => {
      const actor = account();
      const output: SocialPrivateInvitationSnapshot = { incoming: [], outgoing: [], sessionId: null, expiresAtMs: null, remainingRecipients: 8 };
      const inbox = await getDocsFromServer(query(collection(db, "socialPrivateInbox", actor, "sessions"), where("expiresAt", ">", Timestamp.fromMillis(clock.now())), limit(20))); check();
      for (const entry of inbox.docs) {
        check(); const location = entry.data(); if (!pointer(location, entry.id, clock.now()) || location.hostUid === actor) continue;
        const permission = authorization(await optional(authRef(entry.id, actor), check), entry.id, actor, clock.now());
        if (!permission || permission.status !== "active" || permission.hostUid !== location.hostUid || !permission.expiresAt.isEqual(location.expiresAt)) continue;
        const session = privateSession(entry.id, await optional(doc(db, "socialSessions", entry.id), check));
        if (!session || session.hostUid !== permission.hostUid || session.expiresAtMs !== permission.expiresAt.toMillis()) continue;
        output.incoming.push({ session, host: await person(session.hostUid, check), expiresAtMs: session.expiresAtMs });
      }
      const owner = await optional(doc(db, "socialOwners", actor), check);
      if (owner && validSession(owner.sessionId) && owner.visibility === "private") {
        const session = privateSession(owner.sessionId, await optional(doc(db, "socialSessions", owner.sessionId), check));
        if (session?.hostUid === actor) {
          output.sessionId = session.id; output.expiresAtMs = session.expiresAtMs;
          const raw = await optional(audienceRef(session.id), check);
          const people = raw ? audience(raw, session) : null;
          if (raw && !people) throw changed();
          output.remainingRecipients = 8 - (people?.recipients.length ?? 0);
          for (const peer of people?.recipients ?? []) {
            const permission = authorization(await optional(authRef(session.id, peer), check), session.id, peer, clock.now());
            if (!permission || permission.hostUid !== actor || permission.expiresAt.toMillis() !== session.expiresAtMs) throw changed();
            output.outgoing.push({ person: await person(peer, check), status: permission.status });
          }
        }
      }
      check(); return output;
    }),
    invite: (sessionId, recipient) => guarded(async check => {
      const actor = account();
      if (!validSession(sessionId) || !validUid(recipient) || recipient === actor) throw changed();
      await runTransaction(db, async tx => {
        check(); const owner = (await tx.get(doc(db, "socialOwners", actor))).data(); check();
        const session = privateSession(sessionId, (await tx.get(doc(db, "socialSessions", sessionId))).data()); check();
        const rawAudience = (await tx.get(audienceRef(sessionId))).data(); check();
        const rawPermission = (await tx.get(authRef(sessionId, recipient))).data(); check();
        if (owner?.sessionId !== sessionId || owner.visibility !== "private" || session?.hostUid !== actor) throw changed();
        const people = rawAudience ? audience(rawAudience, session) : null;
        const previous = rawPermission ? authorization(rawPermission, sessionId, recipient, clock.now()) : null;
        if (rawAudience && !people || rawPermission && !previous || previous && (previous.status !== "revoked" || previous.revision === 0xffffffff
          || previous.hostUid !== actor || previous.expiresAt.toMillis() !== session.expiresAtMs || !people?.recipients.includes(recipient))) throw changed();
        const recipients = people?.recipients ?? [];
        if (!previous && recipients.includes(recipient) || !recipients.includes(recipient) && recipients.length >= 8) throw changed();
        const expiresAt = Timestamp.fromMillis(session.expiresAtMs);
        if (!previous) {
          const authorizationId = [...crypto.getRandomValues(new Uint8Array(32))].map(value => value.toString(16).padStart(2, "0")).join("");
          tx.set(authRef(sessionId, recipient), { schema: 1, authorizationId, revision: 1, sessionId, hostUid: actor, recipientUid: recipient,
            status: "active", createdAt: serverTimestamp(), updatedAt: serverTimestamp(), expiresAt });
          tx.set(audienceRef(sessionId), { schema: 1, hostUid: actor, sessionId, expiresAt, recipients: [...recipients, recipient] });
        } else tx.update(authRef(sessionId, recipient), { status: "active", revision: previous.revision + 1, updatedAt: serverTimestamp() });
        tx.set(inboxRef(sessionId, recipient), { schema: 1, sessionId, hostUid: actor, expiresAt });
        tx.set(doc(db, "socialPrivateThrottle", actor), { at: serverTimestamp() });
      }, { maxAttempts: 1 }); check();
    }),
    revoke: (sessionId, recipient) => withdraw(sessionId, recipient, true),
    decline: sessionId => withdraw(sessionId, account(), false),
  };
}
