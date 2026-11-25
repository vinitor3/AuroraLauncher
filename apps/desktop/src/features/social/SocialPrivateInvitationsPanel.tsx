import { useEffect, useRef, useState, type FormEvent } from "react";
import type { SocialPerson, SocialPrivateInvitationSnapshot, SocialService, SocialSession } from "../../shared/social";
import { socialError } from "./presentation";

/** Account discovery UI only; the existing App/native owner performs every join. */
export function SocialPrivateInvitationsPanel({ service, ownSession, friends = [], joinEnabled, connectionBusy, onJoin }: {
  service: SocialService; ownSession?: SocialSession | null; joinEnabled: boolean; connectionBusy: boolean;
  friends?: SocialPerson[];
  onJoin: (session: SocialSession) => Promise<unknown>;
}) {
  const invitations = service.privateInvitations;
  const [snapshot, setSnapshot] = useState<SocialPrivateInvitationSnapshot>();
  const [pending, setPending] = useState(false), [error, setError] = useState(""), [message, setMessage] = useState("");
  const [nickname, setNickname] = useState(""), [result, setResult] = useState<SocialPerson | null>(null), [searched, setSearched] = useState(false);
  const clock = () => service.now ? service.now() : Date.now();
  const [now, setNow] = useState(clock);
  const scope = useRef(0), busy = useRef(false), active = useRef(false);
  const current = (generation: number) => active.current && generation === scope.current;
  const sessionKey = ownSession ? `${ownSession.id}:${ownSession.expiresAtMs}:${ownSession.visibility}:${ownSession.joinPolicy}` : "none";
  async function read(generation: number) {
    if (!invitations) return;
    const value = await invitations.snapshot();
    if (current(generation)) { setSnapshot(value); setNow(clock()); }
  }
  useEffect(() => {
    active.current = true; const generation = ++scope.current; busy.current = true;
    setSnapshot(undefined); setResult(null); setSearched(false); setNickname(""); setMessage(""); setError(""); setPending(!!invitations);
    if (invitations) void read(generation).catch(cause => { if (current(generation)) setError(socialError(cause)); })
      .finally(() => { if (current(generation)) { busy.current = false; setPending(false); } });
    const timer = window.setInterval(() => setNow(clock()), 1000);
    return () => { active.current = false; ++scope.current; window.clearInterval(timer); };
  }, [service, invitations, sessionKey]);
  async function run(work: (generation: number) => Promise<void | string>, success = "", refresh = true) {
    if (!invitations || busy.current) return;
    busy.current = true; setPending(true); setError(""); setMessage(""); const generation = scope.current;
    try {
      const result = await work(generation); if (!current(generation)) return;
      if (refresh) { setSnapshot(undefined); setResult(null); setSearched(false); await read(generation); }
      if (current(generation)) setMessage(typeof result === "string" ? result : success);
    } catch (cause) { if (current(generation)) setError(socialError(cause)); }
    finally { if (current(generation)) { busy.current = false; setPending(false); } }
  }
  const host = !!snapshot && !!ownSession && ownSession.visibility === "private" && ownSession.joinPolicy === "open"
    && ownSession.id === snapshot.sessionId && ownSession.expiresAtMs === snapshot.expiresAtMs && ownSession.expiresAtMs > now;
  function canInvite(person: SocialPerson) {
    if (!host || !snapshot || !ownSession || !(ownSession.expiresAtMs > clock())) return false;
    const recipient = snapshot.outgoing.find(item => item.person.uid === person.uid);
    return recipient ? recipient.status === "revoked" : snapshot.remainingRecipients > 0;
  }
  function invite(person: SocialPerson) {
    if (!invitations || !ownSession || !canInvite(person)) return;
    void run(() => invitations.invite(ownSession.id, person.uid), "Convite enviado para a conta selecionada.");
  }
  async function search(event: FormEvent) {
    event.preventDefault(); setResult(null); setSearched(false);
    const query = nickname.trim();
    if (!/^[A-Za-z0-9_]{3,16}$/.test(query)) { setError("Use o nick completo do Aurora: de 3 a 16 letras, números ou _."); return; }
    await run(async generation => {
      const found = await service.searchByNickname(query);
      if (current(generation)) { setResult(found); setSearched(true); }
    }, "", false);
  }
  if (!invitations) return null;
  return <section className="social-private social-organization" aria-label="Convites privados por conta">
    <div className="social-community-heading"><div><h3>Convites privados</h3><p>{invitations.environment === "online" ? "Serviço online do Aurora." : "Teste local por conta Aurora."} O convite não inicia uma conexão nem adiciona amizade.</p>
      {invitations.environment === "online" && <small>Online aqui é o serviço de convites do Aurora; não altera a autenticação do servidor Minecraft.</small>}</div>
      <div className="social-actions"><button className="secondary" disabled={pending} onClick={() => void run(async () => {}, "Lista atualizada.")}>Atualizar convites</button>
        <button className="secondary" disabled={pending} onClick={() => void run(async () => {
          const result = await invitations.cleanupExpired();
          return `${result.removedRecords} registros expirados removidos.${result.moreAvailable ? " Há outra parte da lista para conferir; use Limpar convites expirados novamente." : " Esta etapa da limpeza terminou."}`;
        }, "", false)}>Limpar convites expirados</button></div></div>
    {error && <p className="social-notice" role="alert">{error}</p>}{message && <p className="social-notice" role="status">{message}</p>}
    <h4>Recebidos</h4>
    {!snapshot ? <p>{pending ? "Consultando convites…" : "Atualize os convites para conferir a lista."}</p>
      : !snapshot.incoming.some(item => item.expiresAtMs > now) ? <p>Nenhum convite privado disponível.</p>
        : <ul className="social-people">{snapshot.incoming.filter(item => item.expiresAtMs > now).map(item => <li key={item.session.id}>
          <div><strong>{item.session.title}</strong><p>{item.host.nickname} · {item.session.modpackName}</p><small>{item.session.minecraftVersion} · {item.session.loader} · Limite do anúncio: {item.session.maxPlayers}</small></div>
          <div className="social-actions"><button disabled={pending || connectionBusy || !joinEnabled} onClick={() => {
            if (!joinEnabled || connectionBusy || !(item.expiresAtMs > clock())) return;
            void run(async () => { await onJoin(item.session); }, "", false);
          }}>Entrar</button><button className="secondary" disabled={pending} onClick={() => {
            if (!(item.expiresAtMs > clock())) return;
            void run(() => invitations.decline(item.session.id), "Convite recusado.");
          }}>Recusar convite</button></div></li>)}</ul>}
    {!joinEnabled && <small>A entrada privada depende do suporte nativo deste executável; nenhum convite autoriza um transporte por si só.</small>}
    <h4>Convidar para meu anúncio privado</h4>
    {!host ? <p>Salve um anúncio privado com entrada aberta e atualize esta lista antes de convidar.</p> : <>
      <p>Até 8 contas diferentes por anúncio. Revogar não libera esse limite. Um novo anúncio exige novos convites; remover um participante da conexão não é desfeito por reenviar o convite.</p>
      <small>{snapshot.remainingRecipients} novos destinatários restantes. A capacidade da conexão pode ser menor.</small>
      {friends.length > 0 && <div className="social-actions" aria-label="Convidar amigos">{friends.map(friend =>
        <button type="button" key={friend.uid} disabled={pending || !canInvite(friend)} onClick={() => invite(friend)}>Convidar {friend.nickname}</button>)}</div>}
      <form className="social-private-search social-search" onSubmit={search}><label>Nick completo do Aurora<input value={nickname} maxLength={16} autoComplete="off" spellCheck={false} disabled={pending}
        onChange={event => { setNickname(event.target.value); setResult(null); setSearched(false); }} /></label><button disabled={pending || !nickname.trim()}>Buscar destinatário</button></form>
      {searched && (result ? <div className="social-notice"><strong>{result.nickname}</strong><p>Esta conta poderá descobrir e solicitar entrada na sessão privada atual.</p>
        <button disabled={pending || !canInvite(result)} onClick={() => invite(result)}>Convidar {result.nickname}</button></div> : <p>Nenhuma conta disponível com esse nick.</p>)}
      <ul className="social-people">{snapshot.outgoing.map(item => <li key={item.person.uid}><div><strong>{item.person.nickname}</strong><small>{item.status === "active" ? "Convite ativo" : "Convite revogado ou recusado"}</small></div>
        <button className="secondary" disabled={pending} onClick={() => {
          if (!ownSession || !(ownSession.expiresAtMs > clock())) return;
          void run(() => item.status === "active" ? invitations.revoke(ownSession.id, item.person.uid) : invitations.invite(ownSession.id, item.person.uid),
            item.status === "active" ? "Convite revogado. A autorização desse destinatário foi retirada." : "Convite reenviado, com o prazo original.");
        }}>{item.status === "active" ? "Revogar convite" : "Reenviar convite"}</button></li>)}</ul>
    </>}
  </section>;
}
