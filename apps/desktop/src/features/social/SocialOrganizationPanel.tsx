import { useEffect, useRef, useState } from "react";
import type { SocialOrganizationService, SocialPerson } from "../../shared/social";
import { socialError } from "./presentation";

export function SocialOrganizationPanel({ service, accountId, friends }: { service: SocialOrganizationService; accountId: string; friends: SocialPerson[] }) {
  type Snapshot = Awaited<ReturnType<SocialOrganizationService["snapshot"]>>;
  type Scope = { service: SocialOrganizationService; accountId: string; active: boolean; loading: boolean; mutation: boolean; revision: number; refreshAgain: boolean; clockUnavailable: boolean };
  const scope = useRef<Scope | null>(null);
  const [view, setView] = useState<{ service: SocialOrganizationService; accountId: string; data: Snapshot }>();
  const data = view?.service === service && view.accountId === accountId ? view.data : undefined;
  const [name, setName] = useState("");
  const [invitee, setInvitee] = useState<{ groupKey: string; uid: string }>();
  const [pending, setPending] = useState(false); const [error, setError] = useState("");
  const current = (owner: Scope) => owner.active && scope.current === owner;
  async function refresh(owner: Scope) {
    if (!current(owner)) return;
    if (owner.loading) { owner.refreshAgain = true; return; }
    owner.loading = true;
    const revision = owner.revision;
    try { const next = await owner.service.snapshot(); if (current(owner) && revision === owner.revision) { setView({ service: owner.service, accountId: owner.accountId, data: next }); setError(""); } }
    catch (cause) { if (current(owner) && revision === owner.revision) { owner.clockUnavailable = !!cause && typeof cause === "object" && "code" in cause && cause.code === "social_clock_unavailable"; setView(undefined); setError(socialError(cause)); } }
    finally { owner.loading = false; if (owner.refreshAgain && current(owner)) { owner.refreshAgain = false; if (!owner.clockUnavailable) void refresh(owner); } }
  }
  useEffect(() => {
    const owner: Scope = { service, accountId, active: true, loading: false, mutation: false, revision: 0, refreshAgain: false, clockUnavailable: false };
    scope.current = owner; setView(undefined); setError(""); setInvitee(undefined); setName(""); setPending(false);
    void refresh(owner);
    const update = () => { if (document.visibilityState !== "hidden" && !owner.mutation && !owner.clockUnavailable) void refresh(owner); };
    const timer = setInterval(update, 5000);
    document.addEventListener("visibilitychange", update);
    return () => { owner.active = false; clearInterval(timer); document.removeEventListener("visibilitychange", update); };
  }, [service, accountId]);
  async function run(work: () => Promise<unknown>) {
    const owner = scope.current;
    if (!owner || !current(owner) || owner.service !== service || owner.accountId !== accountId || owner.mutation) return;
    owner.mutation = true; owner.revision++; owner.clockUnavailable = false; setPending(true); setError("");
    try { await work(); if (current(owner)) await refresh(owner); }
    catch (cause) { if (current(owner)) { owner.clockUnavailable = !!cause && typeof cause === "object" && "code" in cause && cause.code === "social_clock_unavailable"; setError(socialError(cause)); } }
    finally { owner.mutation = false; if (current(owner)) setPending(false); }
  }
  const group = data?.group;
  const readNow = () => service.now ? service.now() : Date.now();
  const clockReady = Number.isFinite(readNow());
  const groupExpired = !!group && clockReady && group.expiresAtMs <= readNow();
  const groupKey = group ? JSON.stringify([group.id, group.ownerUid, group.expiresAtMs]) : "";
  const eligibleFriends = group ? friends.filter(person => !group.members.some(member => member.uid === person.uid)
    && !group.invited.some(member => member.uid === person.uid)) : [];
  const selectedInvitee = invitee?.groupKey === groupKey ? eligibleFriends.find(person => person.uid === invitee.uid) : undefined;
  useEffect(() => {
    if (invitee && !selectedInvitee) setInvitee(previous => previous === invitee ? undefined : previous);
  }, [invitee, selectedInvitee]);
  async function inviteSelectedFriend() {
    const owner = scope.current;
    if (!owner || !group || group.ownerUid !== accountId || !selectedInvitee
      || group.invited.length >= 8 || !(group.expiresAtMs > readNow())) return;
    const uid = selectedInvitee.uid;
    await run(async () => {
      await service.inviteToGroup(uid);
      if (current(owner)) setInvitee(previous => previous?.groupKey === groupKey && previous.uid === uid ? undefined : previous);
    });
  }
  return <section className="social-connection" aria-label="Grupos e pedidos de entrada">
    <div className="social-community-heading"><h3>Meu grupo</h3><button className="secondary" disabled={pending} onClick={() => void run(async () => {})}>Atualizar grupos e pedidos</button></div>
    {error && <p role="alert">{error}</p>}
    {!clockReady && <p role="status">O horário do serviço está indisponível. Atualize os grupos e pedidos.</p>}
    {group ? <>
      <h4>{group.name}</h4>{groupExpired && <p role="status">Grupo expirado. Encerre ou saia deste grupo para criar ou aceitar outro.</p>}<p>{group.members.length} de 8 integrantes. O grupo expira em até 4 horas.</p>
      <ul className="social-list">{group.members.map(member => <li key={member.uid} className="social-friend-row"><span>{member.nickname}{member.uid === group.ownerUid ? " · anfitrião" : ""}</span>
        {group.ownerUid === accountId && member.uid !== accountId && <button className="secondary" disabled={pending || groupExpired || !clockReady} onClick={() => void run(() => service.removeGroupMember(member.uid))}>Remover do grupo</button>}</li>)}</ul>
      {group.ownerUid === accountId && <div className="social-actions"><select aria-label="Amigo para convidar ao grupo" value={selectedInvitee?.uid ?? ""} disabled={pending || groupExpired || !clockReady} onChange={event => setInvitee({ groupKey, uid: event.target.value })}><option value="">Selecione um amigo</option>{eligibleFriends.map(person => <option key={person.uid} value={person.uid}>{person.nickname}</option>)}</select><button disabled={pending || groupExpired || !clockReady || !selectedInvitee || group.invited.length >= 8} onClick={() => void inviteSelectedFriend()}>Convidar ao grupo</button></div>}
      {!!group.invited.length && <p>Aguardando: {group.invited.map(person => person.nickname).join(", ")}</p>}
      <button className="danger" disabled={pending} onClick={() => void run(() => service.leaveGroup())}>{group.ownerUid === accountId ? "Encerrar grupo" : "Sair do grupo"}</button>
    </> : <form className="social-actions" onSubmit={event => { event.preventDefault(); void run(() => service.createGroup(name)); }}><input aria-label="Nome do grupo" placeholder="Nome do grupo" maxLength={48} value={name} disabled={pending} onChange={event => setName(event.target.value)} /><button disabled={pending || !name.trim()}>Criar grupo</button></form>}
    {!!data?.invitations.length && <><h4>Convites de grupo</h4>{data.invitations.map(invite => <div className="social-friend-row" key={invite.groupId}><span>{invite.name}</span><div className="social-actions"><button disabled={pending || !!group} onClick={() => void run(() => service.acceptGroup(invite.ownerUid))}>Participar</button><button className="secondary" disabled={pending} onClick={() => void run(() => service.declineGroup(invite.ownerUid))}>Recusar</button></div></div>)}</>}
    <h4>Pedidos para entrar na minha sessão</h4>
    {!data?.joinRequests.length ? <p>Nenhum pedido de entrada.</p> : data.joinRequests.map(request => <div className="social-friend-row" key={request.guest.uid}><span>{request.guest.nickname} · {request.status === "accepted" ? "aprovado" : "aguardando"}</span><div className="social-actions">{request.status === "pending" && <button disabled={pending} onClick={() => void run(() => service.respondJoin(request.sessionId, request.guest.uid, "accept"))}>Aprovar entrada</button>}<button className="secondary" disabled={pending} onClick={() => void run(() => service.respondJoin(request.sessionId, request.guest.uid, "decline"))}>{request.status === "accepted" ? "Revogar entrada" : "Recusar entrada"}</button></div></div>)}
  </section>;
}
