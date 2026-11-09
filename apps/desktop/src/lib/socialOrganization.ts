import { collection, deleteDoc, doc, getDocFromServer, getDocsFromServer, limit, query, runTransaction, serverTimestamp, Timestamp, where, type Firestore } from "firebase/firestore";
import type { SocialOrganizationService, SocialPerson } from "../shared/social";

type Guarded = <T>(work: (check: () => void) => Promise<T>) => Promise<T>;
const uid = (value: string) => { if (!/^[A-Za-z0-9_-]{1,128}$/.test(value)) throw new Error("Conta inválida."); return value; };
const sid = (value: string) => { if (!/^[0-9a-f-]{36}$/.test(value)) throw new Error("Sessão inválida."); return value; };
const unavailable = () => new Error("O grupo ou pedido mudou. Atualize e tente novamente.");

/** Small private groups and join decisions. No method creates a transport or handles its credentials. */
export function createSocialOrganization(db: Firestore, account: () => string, guarded: Guarded,
  person: (uid: string, check: () => void) => Promise<SocialPerson>, clock: { now(): number; writeTime(): number }): SocialOrganizationService {
  const memberRef = (id = account()) => doc(db, "socialGroupMembers", uid(id));
  const groupRef = (id: string) => doc(db, "socialGroups", uid(id));
  const inviteRef = (receiver: string, owner: string) => doc(db, "socialGroupInvites", uid(receiver), "hosts", uid(owner));
  const requestRef = (session: string, guest = account()) => doc(db, "socialJoinRequests", sid(session), "guests", uid(guest));
  const live = (data: { expiresAt?: Timestamp } | undefined) => data?.expiresAt instanceof Timestamp && data.expiresAt.toMillis() > clock.now();
  async function vacantMembership(check: () => void) {
    const membership = (await getDocFromServer(memberRef())).data(); check();
    if (!membership) return;
    if (membership.groupId === account()) {
      const ownGroup = (await getDocFromServer(groupRef(account()))).data(); check();
      if (ownGroup) throw new Error("Encerre seu grupo atual antes de criar outro.");
    }
    // Rules permit this only for an absent/expired group or an already removed member.
    // A current membership always requires the explicit leave transaction.
    try { await deleteDoc(memberRef()); check(); } catch { throw new Error("Saia do grupo atual antes de participar de outro."); }
  }
  return {
    now: clock.now,
    snapshot: () => guarded(async check => {
      const membership = (await getDocFromServer(memberRef())).data(); check();
      let data;
      if (membership) {
        try { data = (await getDocFromServer(groupRef(membership.groupId))).data(); check(); if (!data) await vacantMembership(check); }
        catch (error) {
          check(); if ((error as { code?: string }).code !== "permission-denied") throw error;
          // Owner may have closed and recreated the group. A stale pointer never
          // grants access to that new roster; rules independently gate its cleanup.
          await vacantMembership(check);
        }
      }
      check();
      let group = null;
      if (data && Array.isArray(data.members) && data.members.includes(account())) {
        const members = [], invited = [];
        for (const id of data.members.slice(0, 8)) members.push(await person(id, check));
        if (data.ownerUid === account()) for (const id of (data.invited ?? []).slice(0, 8)) {
          const invitation = (await getDocFromServer(inviteRef(id, account()))).data(); check();
          if (live(invitation)) invited.push(await person(id, check));
        }
        group = { id: membership!.groupId, name: String(data.name), ownerUid: data.ownerUid, members, invited, expiresAtMs: data.expiresAt.toMillis() };
      }
      const invites = await getDocsFromServer(query(collection(db, "socialGroupInvites", account(), "hosts"), where("expiresAt", ">", Timestamp.fromMillis(clock.now())), limit(8))); check();
      const invitations = invites.docs.flatMap(item => { const v = item.data(); return live(v) ? [{ groupId: item.id, name: String(v.name), ownerUid: item.id, expiresAtMs: v.expiresAt.toMillis() }] : []; });
      const owner = (await getDocFromServer(doc(db, "socialOwners", account()))).data(); check();
      const joinRequests = [];
      if (owner?.sessionId) {
        const requests = await getDocsFromServer(query(collection(db, "socialJoinRequests", sid(owner.sessionId), "guests"), limit(8))); check();
        for (const item of requests.docs) { const v = item.data(); if (live(v) && ["pending", "accepted"].includes(v.status)) joinRequests.push({ sessionId: owner.sessionId, guest: await person(item.id, check), status: v.status as "pending" | "accepted", expiresAtMs: v.expiresAt.toMillis() }); }
      }
      return { group, invitations, joinRequests };
    }),
    createGroup: name => guarded(async check => {
      if (!name.trim() || name.length > 48 || /[\u0000-\u001f\u007f]/.test(name)) throw new Error("Use um nome de grupo com até 48 caracteres.");
      await vacantMembership(check);
      await runTransaction(db, async tx => {
        const membership = (await tx.get(memberRef())).data(); check(); if (membership) throw new Error("Saia do grupo atual antes de criar outro.");
        tx.set(groupRef(account()), { ownerUid: account(), name: name.trim(), members: [account()], invited: [], expiresAt: Timestamp.fromMillis(clock.writeTime() + 14_395_000), updatedAt: serverTimestamp() });
        tx.set(memberRef(), { groupId: account() });
      }, { maxAttempts: 1 });
    }),
    inviteToGroup: peer => guarded(async check => {
      uid(peer);
      await runTransaction(db, async tx => {
        const ref = groupRef(account()), group = (await tx.get(ref)).data(); check();
        if (!group || !live(group) || group.members.includes(peer)) throw unavailable();
        const active: string[] = [], expired: string[] = [];
        for (const id of group.invited as string[]) {
          const previous = (await tx.get(inviteRef(id, account()))).data(); check();
          (live(previous) ? active : expired).push(id);
        }
        if (active.includes(peer) || active.length >= 8) throw unavailable();
        for (const id of expired) tx.delete(inviteRef(id, account()));
        tx.update(ref, { invited: [...active, peer], updatedAt: serverTimestamp() });
        tx.set(inviteRef(peer, account()), { name: group.name, ownerUid: account(), expiresAt: Timestamp.fromMillis(Math.min(group.expiresAt.toMillis(), clock.writeTime() + 295_000)) });
      }, { maxAttempts: 1 });
    }),
    acceptGroup: owner => guarded(async check => {
      await vacantMembership(check);
      await runTransaction(db, async tx => {
        const invite = (await tx.get(inviteRef(account(), owner))).data(), group = (await tx.get(groupRef(owner))).data(), member = (await tx.get(memberRef())).data(); check();
        if (member || !live(invite) || !group || !live(group) || group.members.length >= 8 || !group.invited.includes(account())) throw unavailable();
        tx.update(groupRef(owner), { members: [...group.members, account()], invited: group.invited.filter((id: string) => id !== account()), updatedAt: serverTimestamp() });
        tx.set(memberRef(), { groupId: owner }); tx.delete(inviteRef(account(), owner));
      }, { maxAttempts: 1 });
    }),
    declineGroup: owner => guarded(async check => {
      const invitation = (await getDocFromServer(inviteRef(account(), owner))).data(); check();
      if (!live(invitation)) { await deleteDoc(inviteRef(account(), owner)); check(); return; }
      await runTransaction(db, async tx => {
        const group = (await tx.get(groupRef(owner))).data(); check();
        if (group?.invited.includes(account())) tx.update(groupRef(owner), { invited: group.invited.filter((id: string) => id !== account()), updatedAt: serverTimestamp() });
        tx.delete(inviteRef(account(), owner));
      }, { maxAttempts: 1 });
    }),
    leaveGroup: () => guarded(async check => {
      await runTransaction(db, async tx => {
        const member = (await tx.get(memberRef())).data(); check(); if (!member) return;
        const ref = groupRef(member.groupId), group = (await tx.get(ref)).data(); check();
        if (!group) { tx.delete(memberRef()); return; }
        if (group.ownerUid === account()) {
          tx.delete(memberRef()); // Others can clear the stale pointer after the group is gone.
          for (const id of group.invited) tx.delete(inviteRef(id, account()));
          tx.delete(ref);
        } else { tx.update(ref, { members: group.members.filter((id: string) => id !== account()), updatedAt: serverTimestamp() }); tx.delete(memberRef()); }
      }, { maxAttempts: 1 });
    }),
    removeGroupMember: peer => guarded(async check => {
      if (peer === account()) throw unavailable();
      await runTransaction(db, async tx => { const ref = groupRef(account()), group = (await tx.get(ref)).data(); check();
        if (!group || !group.members.includes(uid(peer))) throw unavailable();
        tx.update(ref, { members: group.members.filter((id: string) => id !== peer), updatedAt: serverTimestamp() }); tx.delete(memberRef(peer));
      }, { maxAttempts: 1 });
    }),
    requestJoin: sessionId => guarded(async check => {
      await runTransaction(db, async tx => {
        const session = (await tx.get(doc(db, "socialSessions", sid(sessionId)))).data(), queueRef = doc(db, "socialJoinQueues", sessionId);
        const queue = (await tx.get(queueRef)).data(), previous = (await tx.get(requestRef(sessionId))).data(); check();
        if (!session || !live(session) || session.hostUid === account() || session.joinPolicy !== "approval") throw unavailable();
        if (previous && live(previous)) throw new Error("Seu pedido já foi enviado.");
        const guests: string[] = queue?.guests ?? []; if (!guests.includes(account())) guests.push(account());
        if (guests.length > 8) throw new Error("A fila de pedidos desta sessão está cheia.");
        tx.set(queueRef, { hostUid: session.hostUid, guests, expiresAt: session.expiresAt });
        tx.set(requestRef(sessionId), { sessionId, hostUid: session.hostUid, guestUid: account(), status: "pending", expiresAt: session.expiresAt, updatedAt: serverTimestamp() });
      }, { maxAttempts: 1 });
    }),
    joinRequestStatus: sessionId => guarded(async check => { const value = (await getDocFromServer(requestRef(sessionId))).data(); check(); return value && live(value) && ["pending", "accepted"].includes(value.status) ? value.status : "none"; }),
    respondJoin: (sessionId, guest, decision) => guarded(async check => {
      await runTransaction(db, async tx => {
        const ref = requestRef(sessionId, guest), value = (await tx.get(ref)).data();
        const queueRef = doc(db, "socialJoinQueues", sid(sessionId)), queue = (await tx.get(queueRef)).data(); check(); if (!value || value.hostUid !== account()) throw unavailable();
        if (decision === "accept") { if (!live(value)) throw unavailable(); tx.update(ref, { status: "accepted", updatedAt: serverTimestamp() }); }
        else { tx.delete(ref); if (queue) tx.update(queueRef, { guests: queue.guests.filter((id: string) => id !== guest) }); }
      }, { maxAttempts: 1 });
    }),
  };
}
