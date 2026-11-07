import { type FormEvent, type ReactNode, useEffect, useRef, useState } from "react";
import type { SocialPerson, SocialService, SocialSession, SocialSessionDraft, SocialSnapshot } from "../../shared/social";
import { validateSocialSession } from "../../shared/social";
import { socialArtwork, socialError, socialInitials } from "./presentation";
import { SocialConnectionPanel, type SocialConnectionControls } from "./SocialConnectionPanel";
import "./social.css";

type FriendTab = "friends" | "incoming" | "outgoing" | "blocked";
type Props = { service: SocialService | null; accountId: string; nickname: string; currentModpack?: string; connection?: SocialConnectionControls; onOpenConnectedGame?: () => void };
const friendTabs: [FriendTab, string][] = [["friends", "Amigos"], ["incoming", "Recebidos"], ["outgoing", "Enviados"], ["blocked", "Bloqueados"]];
const MAX_VISIBLE_SESSIONS = 100;
const emptyDraft = (): SocialSessionDraft => ({ title: "", modpackName: "", minecraftVersion: "", loader: "Vanilla", imageUrl: null, players: 0, maxPlayers: 8, visibility: "private", joinPolicy: "open" });
const clientKeys = new WeakMap<SocialService, number>();
let nextClientKey = 0;

function Avatar({ name }: { name: string }) {
  return <span className="social-avatar" aria-hidden="true">{socialInitials(name)}</span>;
}

function Empty({ title, children }: { title: string; children: ReactNode }) {
  return <div className="social-empty"><span className="social-empty-mark" aria-hidden="true">◇</span><strong>{title}</strong><p>{children}</p></div>;
}

function SessionCard({ session, now, joinEnabled, pending, onJoin }: {
  session: SocialSession; now: number; joinEnabled: boolean; pending: boolean; onJoin: (id: string) => void;
}) {
  const image = socialArtwork(session.imageUrl);
  const [failedImage, setFailedImage] = useState<string>();
  const expired = session.expiresAtMs <= now;
  const full = session.players >= session.maxPlayers;
  return <article className="social-session" aria-label={session.title}>
    <div className="social-session-art">{image && failedImage !== image
      ? <img src={image} alt="" loading="lazy" referrerPolicy="no-referrer" onError={() => setFailedImage(image)} />
      : <span aria-hidden="true">{socialInitials(session.modpackName || session.title)}</span>}</div>
    <div className="social-session-body">
      <div className="social-session-title"><h3>{session.title}</h3><p>{session.modpackName || "Minecraft"}</p></div>
      <div className="social-session-meta"><span>{session.minecraftVersion}</span><span>{session.loader || "Vanilla"}</span>
        <span className="social-population" title="Quantidade declarada pelo anfitrião">{session.players} / {session.maxPlayers}</span></div>
      <small>{expired ? "Anúncio expirado" : full ? "Lotação declarada" : session.visibility === "friends" ? "Para amigos" : "Sessão da comunidade"}</small>
    </div>
    <div className="social-session-footer"><small>{!joinEnabled ? "Entrada automática em implementação" : session.joinPolicy === "approval" ? "O anfitrião aprova a entrada" : "Ingresso aberto"}</small>
      <button disabled={!joinEnabled || pending || expired || full || session.joinPolicy !== "open"} onClick={() => { if (session.joinPolicy === "open") onJoin(session.id); }}>{session.joinPolicy === "approval" ? "Pedir para entrar" : "Entrar"}</button></div>
  </article>;
}

/** Presentation consumes account-scoped projections; it never handles transport credentials. */
export function SocialView(props: Props) {
  if (props.service && !clientKeys.has(props.service)) clientKeys.set(props.service, ++nextClientKey);
  // A new account or client receives a new component before any previous data renders.
  return <SocialAccountView key={`${props.accountId}:${props.service ? clientKeys.get(props.service) : "none"}`} {...props} />;
}

function SocialAccountView({ service, accountId, nickname, currentModpack, connection, onOpenConnectedGame }: Props) {
  const [snapshot, setSnapshot] = useState<SocialSnapshot>();
  const [sessions, setSessions] = useState<SocialSession[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [query, setQuery] = useState("");
  const [searched, setSearched] = useState(false);
  const [result, setResult] = useState<SocialPerson | null>(null);
  const [friendTab, setFriendTab] = useState<FriendTab>("friends");
  const [scope, setScope] = useState<"public" | "friends">("public");
  const [layout, setLayout] = useState<"grid" | "list">("grid");
  const [filter, setFilter] = useState("");
  const [draft, setDraft] = useState<SocialSessionDraft>(emptyDraft);
  const [publicationConsent, setPublicationConsent] = useState(false);
  const [now, setNow] = useState(Date.now);
  const epoch = useRef(0);
  const busy = useRef(false);
  const active = useRef(false);
  const available = !!service?.capabilities.friends;
  const observedModpack = currentModpack?.trim().slice(0, 64) || null;
  const current = (generation: number) => active.current && epoch.current === generation;

  async function readAll(generation: number) {
    if (!service) return;
    const [next, page] = await Promise.all([service.snapshot(), service.capabilities.catalog ? service.listPublicSessions() : Promise.resolve({ items: [], nextCursor: null })]);
    if (!current(generation)) return;
    setSnapshot(next); setSessions(page.items.slice(0, MAX_VISIBLE_SESSIONS)); setNextCursor(page.items.length >= MAX_VISIBLE_SESSIONS ? null : page.nextCursor);
  }

  useEffect(() => {
    active.current = true; const generation = ++epoch.current; busy.current = false;
    setSnapshot(undefined); setSessions([]); setNextCursor(null); setPending(false); setLoading(available);
    setError(""); setMessage(""); setQuery(""); setSearched(false); setResult(null); setFilter("");
    setDraft(emptyDraft()); setPublicationConsent(false); setFriendTab("friends"); setScope("public");
    if (available) void readAll(generation).catch((reason: unknown) => {
      if (current(generation)) setError(socialError(reason));
    }).finally(() => { if (current(generation)) setLoading(false); });
    const observeClock = () => setNow(Date.now());
    const timer = window.setInterval(observeClock, 1000);
    window.addEventListener("focus", observeClock); document.addEventListener("visibilitychange", observeClock);
    return () => { active.current = false; ++epoch.current; window.clearInterval(timer); window.removeEventListener("focus", observeClock); document.removeEventListener("visibilitychange", observeClock); };
  }, [service, accountId]);

  async function run(action: () => Promise<unknown>, success: string, refresh = true) {
    if (!available || busy.current || loading) return;
    busy.current = true; setPending(true); setError(""); setMessage(""); const generation = epoch.current;
    try {
      await action();
      if (!current(generation)) return;
      if (refresh) {
        // A failed refresh cannot leave a previously accepted relation actionable.
        setSnapshot(undefined); setSessions([]); setNextCursor(null);
        await readAll(generation);
      }
      if (current(generation)) setMessage(success);
    } catch (reason) { if (current(generation)) setError(socialError(reason)); }
    finally { if (current(generation)) { busy.current = false; setPending(false); } }
  }

  async function search(event: FormEvent) {
    event.preventDefault();
    if (!service || busy.current || loading) return;
    const nick = query.trim(); setResult(null); setSearched(false);
    if (!/^[A-Za-z0-9_]{3,16}$/.test(nick)) { setError("Use o nick completo do Aurora: de 3 a 16 letras, números ou _."); return; }
    const generation = epoch.current;
    await run(async () => {
      const found = await service.searchByNickname(nick);
      if (current(generation)) { setResult(found); setSearched(true); }
    }, "", false);
  }

  function actOn(person: SocialPerson, action: "accept" | "decline" | "cancel" | "remove" | "block" | "unblock" | "request") {
    if (!service) return;
    const request = action === "accept" ? snapshot?.incoming.find(item => item.uid === person.uid) : undefined;
    if (action === "accept" && (!request || !Number.isFinite(request.expiresAtMs) || request.expiresAtMs <= Date.now())) { setError("Este pedido expirou. Você pode recusá-lo para removê-lo da lista."); return; }
    const work = {
      accept: () => service.respondFriend(person.uid, "accept"), decline: () => service.respondFriend(person.uid, "decline"),
      cancel: () => service.cancelRequest(person.uid), remove: () => service.removeFriend(person.uid),
      block: () => service.block(person.uid), unblock: () => service.unblock(person.uid), request: () => service.requestFriend(person.uid),
    };
    const text = { accept: "Amizade aceita.", decline: "Pedido recusado.", cancel: "Pedido cancelado.", remove: "Amizade removida.", block: "Pessoa bloqueada.", unblock: "Pessoa desbloqueada. A amizade não foi restaurada.", request: "Pedido de amizade enviado." };
    void run(work[action], text[action]);
  }

  function relation(person: SocialPerson) {
    if (person.uid === accountId) return "Você";
    if (snapshot?.blocked.some(item => item.uid === person.uid)) return "Bloqueado";
    if (snapshot?.friends.some(item => item.uid === person.uid)) return "Já é seu amigo";
    if (snapshot?.incoming.some(item => item.uid === person.uid)) return "Pedido recebido";
    if (snapshot?.outgoing.some(item => item.uid === person.uid)) return "Pedido enviado";
    return null;
  }

  async function more() {
    if (!service || !nextCursor) return;
    const cursor = nextCursor, generation = epoch.current;
    await run(async () => {
      const page = await service.listPublicSessions(cursor);
      if (current(generation)) {
        const combined = [...new Map([...sessions, ...page.items].map(item => [item.id, item])).values()].slice(0, MAX_VISIBLE_SESSIONS);
        setSessions(combined); setNextCursor(combined.length >= MAX_VISIBLE_SESSIONS ? null : page.nextCursor);
      }
    }, "", false);
  }

  const disabled = loading || pending || !available;
  const connectSnapshot = connection?.state.snapshot;
  const connectionBusy = !!connection?.state.pending || !!connectSnapshot?.operationId
    && !(["closed", "failed"].includes(connectSnapshot.phase) && connectSnapshot.cleanupState === "complete");
  const visibleSessions = (scope === "public" ? sessions : (snapshot?.friends.flatMap(friend => friend.session ? [friend.session] : []) ?? []))
    .filter(session => session.expiresAtMs > now && `${session.title} ${session.modpackName} ${session.minecraftVersion}`.toLocaleLowerCase().includes(filter.trim().toLocaleLowerCase()));
  const friends = snapshot?.[friendTab] ?? [];
  const liveActivity = (person: SocialSnapshot["friends"][number]) => person.presence
    && person.presence.updatedAtMs <= now && person.presence.updatedAtMs + 120_000 > now ? person.presence.modpackName : null;

  return <section className="social-view" aria-label="Amigos e comunidade">
    <header className="social-heading"><div><p className="eyebrow">AURORA CONNECT</p><h2>Jogue em boa companhia</h2><p>Seus amigos do Aurora e os mundos compartilhados pela comunidade.</p></div><span className="social-tag">Experimental</span></header>
    {!available ? <div className="social-notice" data-kind="warning"><strong>O serviço social ainda não está habilitado.</strong><p>Amigos, busca e comunidade dependem da configuração autorizada do serviço. Nenhuma solicitação foi enviada.</p></div> : <>
      {error && <div className="social-notice" data-kind="error" role="alert">{error}</div>}
      {message && <div className="social-notice" role="status">{message}</div>}
      <div className="social-actions"><button className="secondary" disabled={disabled} onClick={() => void run(async () => undefined, "Lista atualizada.")}>{loading ? "Carregando…" : pending ? "Aguarde…" : "Atualizar"}</button><small>Atividade e população são informadas pelo anfitrião; não comprovam entrada no mundo.</small></div>
    </>}
    <div className="social-layout">
      <aside className="social-friends" aria-label="Amigos do Aurora">
        <div className="social-self"><Avatar name={nickname} /><div><strong>{nickname}</strong><small>{snapshot?.presenceSharing === "friends" ? snapshot.modpackName || "Atividade compartilhada com amigos" : "Atividade oculta"}</small></div></div>
        <form className="social-search" onSubmit={search}><label htmlFor="social-nickname">Adicionar pelo nick do Aurora</label><div className="social-search-row"><input id="social-nickname" autoComplete="off" spellCheck={false} maxLength={16} value={query} disabled={disabled} placeholder="Nick completo" onChange={event => { setQuery(event.target.value); setResult(null); setSearched(false); }} /><button disabled={disabled || !query.trim()}>Buscar</button></div><small>Somente contas cadastradas no launcher. Sem busca de perfis Minecraft.</small>
          {searched && <div className="social-search-result">{result ? <><div className="social-person"><Avatar name={result.nickname} /><div><strong>{result.nickname}</strong><small>{relation(result) || "Conta Aurora"}</small></div></div>{!relation(result) && <button type="button" disabled={disabled || !snapshot} onClick={() => actOn(result, "request")}>Adicionar amigo</button>}</> : <small role="status">Nenhuma conta disponível com esse nick.</small>}</div>}
        </form>
        <div className="social-tabs" aria-label="Listas de amizade">{friendTabs.map(([key, title]) => <button key={key} aria-pressed={friendTab === key} onClick={() => setFriendTab(key)}>{title}{snapshot ? ` ${snapshot[key].length}` : ""}</button>)}</div>
        {!friends.length ? <Empty title={loading ? "Carregando amigos…" : friendTab === "friends" ? "Sua próxima aventura é em grupo" : "Nenhum item nesta lista"}>{friendTab === "friends" ? "Busque o nick de alguém no Aurora e envie um pedido de amizade." : "Os pedidos e as alterações aparecem após atualizar."}</Empty> : <ul className="social-list">{friends.map(person => {
          const friend = snapshot?.friends.find(item => item.uid === person.uid);
          const activity = friend ? liveActivity(friend) : null;
          const request = friendTab === "incoming" || friendTab === "outgoing" ? snapshot?.[friendTab].find(item => item.uid === person.uid) : undefined;
          const expiredRequest = !!request && (!Number.isFinite(request.expiresAtMs) || request.expiresAtMs <= now);
          return <li key={person.uid} className="social-friend-row"><div className="social-person"><Avatar name={person.nickname} /><div><strong>{person.nickname}</strong><small>{friend ? activity || "Atividade não compartilhada" : expiredRequest ? "Pedido expirado" : friendTab === "incoming" ? "Quer ser seu amigo" : friendTab === "outgoing" ? "Aguardando resposta" : "Bloqueado"}</small></div></div>
            <div className="social-person-actions">
              {friendTab === "incoming" && <><button disabled={disabled || expiredRequest} onClick={() => actOn(person, "accept")}>Aceitar</button><button className="secondary" disabled={disabled} onClick={() => actOn(person, "decline")}>Recusar</button></>}
              {friendTab === "outgoing" && <button className="secondary" disabled={disabled} onClick={() => actOn(person, "cancel")}>Cancelar pedido</button>}
              {friendTab === "blocked" && <button className="secondary" disabled={disabled} onClick={() => actOn(person, "unblock")}>Desbloquear</button>}
              {friendTab === "friends" && friend?.session && <button disabled={disabled || connectionBusy || !service?.capabilities.join || friend.session.joinPolicy !== "open" || friend.session.expiresAtMs <= now || friend.session.players >= friend.session.maxPlayers} onClick={() => { if (!connectionBusy && friend.session && friend.session.joinPolicy === "open" && friend.session.expiresAtMs > Date.now() && service?.capabilities.join) void run(() => service.prepareJoin(friend.session!.id), "", false); }}>{friend.session.joinPolicy === "approval" ? "Pedir para entrar" : "Entrar"}</button>}
              {friendTab !== "blocked" && <details className="social-person-menu"><summary>Opções</summary><div>{friendTab === "friends" && <button className="secondary" disabled={disabled} onClick={() => actOn(person, "remove")}>Remover amigo</button>}<button className="danger" disabled={disabled} onClick={() => actOn(person, "block")}>Bloquear</button></div></details>}
            </div></li>;
        })}</ul>}
        {available && snapshot && <div className="social-search"><label className="social-consent"><input type="checkbox" checked={snapshot.presenceSharing === "friends"} disabled={disabled} onChange={event => void run(() => service!.setPresence({ sharing: event.target.checked ? "friends" : "hidden", modpackName: event.target.checked ? observedModpack : null }), event.target.checked ? "Atividade visível somente para amigos." : "Atividade oculta.")} />Compartilhar minha atividade com amigos</label>
          <small>{observedModpack ? `Em execução: ${observedModpack}` : "A atividade poderá mostrar o modpack quando um jogo iniciar pelo Aurora."}</small>
          {snapshot.presenceSharing === "friends" && <><button className="secondary" disabled={disabled} onClick={() => void run(() => service!.setPresence({ sharing: "friends", modpackName: observedModpack }), "Atividade atualizada para amigos.")}>Atualizar atividade</button><small>A atualização vale por 2 minutos. Nenhuma renovação é automática nesta versão.</small></>}
        </div>}
      </aside>
      <div className="social-community">
        {connection && <SocialConnectionPanel connection={connection} ownSession={snapshot?.ownSession} now={now} onOpenConnectedGame={onOpenConnectedGame} />}
        <header className="social-community-heading"><div><h3>Mundos da comunidade</h3><p>Encontre uma sessão para a sua próxima aventura.</p></div><div className="social-layout-switch" aria-label="Visualização das sessões"><button aria-pressed={layout === "grid"} onClick={() => setLayout("grid")}>Grade</button><button aria-pressed={layout === "list"} onClick={() => setLayout("list")}>Lista</button></div></header>
        <div className="social-filters"><div className="social-tabs" aria-label="Origem das sessões"><button aria-pressed={scope === "public"} onClick={() => setScope("public")}>Comunidade</button><button aria-pressed={scope === "friends"} onClick={() => setScope("friends")}>Dos amigos</button></div><input aria-label="Filtrar sessões carregadas" placeholder="Filtrar modpack ou versão" value={filter} onChange={event => setFilter(event.target.value)} /></div>
        {available && !service?.capabilities.join && <p className="social-notice">Você já pode organizar amizades e descobrir anúncios. O botão Entrar ficará disponível quando a conexão social segura estiver integrada.</p>}
        {!visibleSessions.length ? <Empty title={loading ? "Procurando sessões…" : "Nenhuma sessão disponível"}>{filter ? "Nenhum anúncio carregado corresponde ao filtro." : scope === "friends" ? "As sessões compartilhadas pelos seus amigos aparecerão aqui." : "Os mundos publicados voluntariamente pelos anfitriões aparecerão aqui."}</Empty> : <div className="social-catalog" data-layout={layout}>{visibleSessions.map(session => <SessionCard key={session.id} session={session} now={now} pending={disabled || connectionBusy || session.hostUid === accountId} joinEnabled={service?.capabilities.join === true} onJoin={id => { if (!connectionBusy && session.hostUid !== accountId && session.joinPolicy === "open" && session.expiresAtMs > Date.now() && service?.capabilities.join) void run(() => service.prepareJoin(id), "", false); }} />)}</div>}
        {scope === "public" && nextCursor && <div className="social-pagination"><button className="secondary" disabled={disabled} onClick={() => void more()}>Carregar mais</button></div>}
        {sessions.length >= MAX_VISIBLE_SESSIONS && <small>Exibindo até 100 anúncios. Atualize para consultar a lista novamente.</small>}
        {available && service?.capabilities.publication && snapshot && <details className="social-publish"><summary>{snapshot.ownSession ? "Meu anúncio de sessão" : "Compartilhar uma sessão"}</summary>
          <form onSubmit={event => { event.preventDefault(); if (draft.visibility !== "private" && !publicationConsent) return; void run(() => service.publishSession(validateSocialSession(draft)), connection?.state.available
            ? "Anúncio atualizado. Use Preparar meu servidor para ativar o compartilhamento."
            : "Anúncio atualizado. O compartilhamento do servidor ainda não foi ativado."); }}>
            <p>O anúncio não abre portas, inicia o jogo ou comprova que o servidor está acessível.</p>
            {snapshot.ownSession && <div className="social-notice"><strong>{snapshot.ownSession.title}</strong><small>{snapshot.ownSession.expiresAtMs <= now ? "Anúncio expirado" : "Anúncio registrado"}</small><div className="social-actions"><button type="button" className="secondary" disabled={disabled} onClick={() => void run(() => service.closeSession(), "Anúncio encerrado.")}>Encerrar anúncio</button></div></div>}
            <div className="social-fields">
              <label>Título<input required maxLength={64} value={draft.title} disabled={disabled} onChange={event => setDraft({ ...draft, title: event.target.value })} /></label>
              <label>Modpack<input required maxLength={64} value={draft.modpackName} disabled={disabled} onChange={event => setDraft({ ...draft, modpackName: event.target.value })} /></label>
              <label>Versão do Minecraft<input required maxLength={24} value={draft.minecraftVersion} disabled={disabled} onChange={event => setDraft({ ...draft, minecraftVersion: event.target.value })} placeholder="Ex.: 1.21.1" /></label>
              <label>Loader<input required maxLength={24} value={draft.loader} disabled={disabled} onChange={event => setDraft({ ...draft, loader: event.target.value })} placeholder="Ex.: Fabric" /></label>
              <label className="social-field-wide">Imagem do modpack<input type="url" maxLength={512} value={draft.imageUrl ?? ""} disabled={disabled} onChange={event => setDraft({ ...draft, imageUrl: event.target.value || null })} placeholder="Imagem do catálogo Modrinth ou CurseForge (opcional)" /></label>
              <label>Quem pode descobrir<select value={draft.visibility} disabled={disabled} onChange={event => { setDraft({ ...draft, visibility: event.target.value as SocialSessionDraft["visibility"] }); setPublicationConsent(false); }}><option value="private">Privada</option><option value="friends">Meus amigos</option><option value="public">Toda a comunidade</option></select></label>
              <label>Política de ingresso<select value={draft.joinPolicy} disabled={disabled} onChange={event => setDraft({ ...draft, joinPolicy: event.target.value as SocialSessionDraft["joinPolicy"] })}><option value="open">Entrada aberta</option><option value="approval">Aprovar cada pedido</option></select></label>
              <label>Pessoas na sessão<input type="number" min={0} max={draft.maxPlayers} value={draft.players} disabled={disabled} onChange={event => setDraft({ ...draft, players: Number(event.target.value) })} /></label>
              <label>Limite de pessoas<input type="number" min={1} max={100} value={draft.maxPlayers} disabled={disabled} onChange={event => setDraft({ ...draft, maxPlayers: Number(event.target.value) })} /></label>
            </div>
            {draft.visibility !== "private" && <label className="social-consent"><input type="checkbox" checked={publicationConsent} disabled={disabled} onChange={event => setPublicationConsent(event.target.checked)} />Quero publicar estes dados {draft.visibility === "friends" ? "para meus amigos" : "para a comunidade do Aurora"}. Posso encerrar o anúncio.</label>}
            <div className="social-actions"><button disabled={disabled || (draft.visibility !== "private" && !publicationConsent)}>Salvar anúncio</button></div>
          </form>
        </details>}
      </div>
    </div>
  </section>;
}
