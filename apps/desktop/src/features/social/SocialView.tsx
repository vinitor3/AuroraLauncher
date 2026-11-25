import { type FormEvent, type ReactNode, useEffect, useRef, useState } from "react";
import type { FriendInbox, SocialPerson, SocialService, SocialSession, SocialSessionDraft, SocialSnapshot } from "../../shared/social";
import { validateSocialSession } from "../../shared/social";
import { socialArtwork, socialError, socialInitials } from "./presentation";
import { SocialConnectionPanel, type SocialConnectionControls } from "./SocialConnectionPanel";
import { SocialOrganizationPanel } from "./SocialOrganizationPanel";
import { SocialAvailabilityBadge, useSocialAvailability } from "./SocialAvailability";
import "./social.css";
import { LanWorldShare } from "./LanWorldShare";
import { SocialPrivateInvitationsPanel } from "./SocialPrivateInvitationsPanel";
import { DiscordInvitation, DiscordSettingsPanel } from "./DiscordPanel";
import type { DiscordControls } from "./discordModel";

type FriendTab = "friends" | "incoming" | "outgoing" | "blocked";
type Props = { service: SocialService | null; accountId: string; nickname: string; discord?: DiscordControls; inbox?: FriendInbox; revealInboxRequest?: number; instanceNames?: Record<string, string>; currentModpack?: string; connection?: SocialConnectionControls; onOpenConnectedGame?: () => void; onPrepareDedicatedServer?: () => void; onOpenManualConnect?: () => void };
const friendTabs: [FriendTab, string][] = [["friends", "Amigos"], ["incoming", "Recebidos"], ["outgoing", "Enviados"], ["blocked", "Bloqueados"]];
const MAX_VISIBLE_SESSIONS = 100;
const emptyDraft = (): SocialSessionDraft => ({ title: "", modpackName: "", minecraftVersion: "", loader: "Vanilla", imageUrl: null, players: 0, maxPlayers: 8, visibility: "private", joinPolicy: "open" });
const sessionDraft = ({ title, modpackName, minecraftVersion, loader, imageUrl, players, maxPlayers, visibility, joinPolicy }: SocialSession): SocialSessionDraft =>
  ({ title, modpackName, minecraftVersion, loader, imageUrl, players, maxPlayers, visibility, joinPolicy });
const clientKeys = new WeakMap<SocialService, number>();
let nextClientKey = 0;

function Avatar({ name }: { name: string }) {
  return <span className="social-avatar" aria-hidden="true">{socialInitials(name)}</span>;
}

function Empty({ title, children }: { title: string; children: ReactNode }) {
  return <div className="social-empty"><span className="social-empty-mark" aria-hidden="true">◇</span><strong>{title}</strong><p>{children}</p></div>;
}

function SessionCard({ session, now, joinEnabled, pending, waitingForApproval, onJoin, onCancelRequest, availability }: {
  session: SocialSession; now: number; joinEnabled: boolean; pending: boolean; onJoin: (id: string) => void;
  onCancelRequest?: () => void;
  waitingForApproval: boolean;
  availability: ReturnType<typeof useSocialAvailability>;
}) {
  const image = socialArtwork(session.imageUrl);
  const [failedImage, setFailedImage] = useState<string>();
  const expired = session.expiresAtMs <= now;
  return <article className="social-session" aria-label={session.title}>
    <div className="social-session-art">{image && failedImage !== image
      ? <img src={image} alt="" loading="lazy" referrerPolicy="no-referrer" onError={() => setFailedImage(image)} />
      : <span aria-hidden="true">{socialInitials(session.modpackName || session.title)}</span>}</div>
    <div className="social-session-body">
      <div className="social-session-title"><h3>{session.title}</h3><p>{session.modpackName || "Minecraft"}</p></div>
      <div className="social-session-meta"><span>{session.minecraftVersion}</span><span>{session.loader || "Vanilla"}</span>
        <span className="social-population" title="Limite declarado; o transporte aplica sua capacidade própria">Limite do anúncio: {session.maxPlayers}</span></div>
      <SocialAvailabilityBadge sessionId={session.id} value={availability.values.get(session.id)} observe={availability.observe} />
      <small>{expired ? "Anúncio expirado" : session.visibility === "friends" ? "Para amigos" : "Sessão da comunidade"}</small>
    </div>
    <div className="social-session-footer"><small>{!joinEnabled ? "Entrada indisponível" : waitingForApproval ? "Aguardando aprovação" : session.joinPolicy === "approval" ? "O anfitrião aprova a entrada" : "Ingresso aberto"}</small>
      <button disabled={!joinEnabled || pending || expired} onClick={() => onJoin(session.id)}>{session.joinPolicy === "approval" ? waitingForApproval ? "Verificar pedido" : "Pedir para entrar" : "Entrar"}</button>
      {waitingForApproval && onCancelRequest && <button className="secondary" disabled={pending || expired} onClick={onCancelRequest}>Cancelar pedido</button>}</div>
  </article>;
}

/** Presentation consumes account-scoped projections; it never handles transport credentials. */
export function SocialView(props: Props) {
  if (props.service && !clientKeys.has(props.service)) clientKeys.set(props.service, ++nextClientKey);
  // A new account or client receives a new component before any previous data renders.
  return <SocialAccountView key={`${props.accountId}:${props.service ? clientKeys.get(props.service) : "none"}`} {...props} />;
}

function SocialAccountView({ service, accountId, nickname, discord, inbox, revealInboxRequest, instanceNames, currentModpack, connection, onOpenConnectedGame, onPrepareDedicatedServer, onOpenManualConnect }: Props) {
  const [view, setView] = useState<"play" | "friends" | "share" | "invites">(inbox?.count ? "friends" : "play");
  const [snapshot, setSnapshot] = useState<SocialSnapshot>();
  const [sessions, setSessions] = useState<SocialSession[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [waitingJoinRequests, setWaitingJoinRequests] = useState<Record<string, number>>({});
  const [groupsExpanded, setGroupsExpanded] = useState(false);
  const [query, setQuery] = useState("");
  const [searched, setSearched] = useState(false);
  const [result, setResult] = useState<SocialPerson | null>(null);
  const [friendTab, setFriendTab] = useState<FriendTab>(inbox?.count ? "incoming" : "friends");
  const lastInboxRequest = useRef(revealInboxRequest);
  useEffect(() => {
    if (lastInboxRequest.current === revealInboxRequest) return;
    lastInboxRequest.current = revealInboxRequest;
    setView("friends");
    setFriendTab("incoming");
  }, [revealInboxRequest]);
  const [scope, setScope] = useState<"public" | "friends">("public");
  const [layout, setLayout] = useState<"grid" | "list">("grid");
  const [filter, setFilter] = useState("");
  const [draft, setDraft] = useState<SocialSessionDraft>(emptyDraft);
  const draftSession = useRef<string | null | undefined>(undefined);
  const draftEdited = useRef(false);
  const [publicationConsent, setPublicationConsent] = useState(false);
  const readNow = () => service?.now ? service.now() : Date.now();
  const [now, setNow] = useState(readNow);
  const epoch = useRef(0);
  const busy = useRef(false);
  const active = useRef(false);
  type JoinNavigation = {
    accountId: string; service: SocialService; controller: SocialConnectionControls["controller"];
    epoch: number; sessionId: string; expiresAtMs: number; previousOperationId: string | null;
    minimumGeneration: number; operationId: string | null; dispatched: boolean;
  };
  const joinNavigation = useRef<JoinNavigation | null>(null);
  const [joinNavigationRevision, setJoinNavigationRevision] = useState(0);
  const availability = useSocialAvailability(connection?.controller, connection?.state.available === true && connection.state.maxSessionSeconds === 14400);
  const available = !!service?.capabilities.friends;
  const observedModpack = currentModpack?.trim().slice(0, 64) || null;
  const current = (generation: number) => active.current && epoch.current === generation;

  async function readAll(generation: number) {
    if (!service) return;
    const [next, page] = await Promise.all([service.snapshot(), service.capabilities.catalog ? service.listPublicSessions() : Promise.resolve({ items: [], nextCursor: null })]);
    if (!current(generation)) return;
    const sessionId = next.ownSession?.id ?? null;
    if (!draftEdited.current || draftSession.current !== sessionId) {
      setDraft(next.ownSession ? sessionDraft(next.ownSession) : emptyDraft());
      draftEdited.current = false;
      setPublicationConsent(false);
    }
    draftSession.current = sessionId;
    setNow(readNow()); setSnapshot(next); setSessions(page.items.slice(0, MAX_VISIBLE_SESSIONS)); setNextCursor(page.items.length >= MAX_VISIBLE_SESSIONS ? null : page.nextCursor);
  }

  function editDraft(change: Partial<SocialSessionDraft>) {
    if (hostPublicationLocked) return;
    draftEdited.current = true;
    setDraft(previous => ({ ...previous, ...change }));
  }

  useEffect(() => {
    active.current = true; const generation = ++epoch.current; busy.current = false;
    setSnapshot(undefined); setSessions([]); setNextCursor(null); setPending(false); setLoading(available);
    setError(""); setMessage(""); setQuery(""); setSearched(false); setResult(null); setFilter("");
    setWaitingJoinRequests({});
    draftSession.current = undefined; draftEdited.current = false;
    setDraft(emptyDraft()); setPublicationConsent(false); setScope("public");
    if (available) void readAll(generation).catch((reason: unknown) => {
      if (current(generation)) setError(socialError(reason));
    }).finally(() => { if (current(generation)) setLoading(false); });
    const observeClock = () => setNow(readNow());
    const timer = window.setInterval(observeClock, 1000);
    window.addEventListener("focus", observeClock); document.addEventListener("visibilitychange", observeClock);
    return () => { active.current = false; ++epoch.current; joinNavigation.current = null; window.clearInterval(timer); window.removeEventListener("focus", observeClock); document.removeEventListener("visibilitychange", observeClock); };
  }, [service, accountId]);

  async function run(action: () => Promise<unknown>, success: string, refresh = true) {
    if (!available || busy.current || loading) return;
    busy.current = true; setPending(true); setError(""); setMessage(""); const generation = epoch.current;
    try {
      const result = await action();
      if (!current(generation)) return;
      if (refresh) {
        // A failed refresh cannot leave a previously accepted relation actionable.
        setSnapshot(undefined); setSessions([]); setNextCursor(null);
        await readAll(generation);
      }
      if (current(generation)) setMessage(typeof result === "string" ? result : success);
    } catch (reason) { if (current(generation)) setError(socialError(reason)); }
    finally { if (current(generation)) { busy.current = false; setPending(false); } }
  }

  async function enterSession(session: SocialSession) {
    const generation = epoch.current;
    const live = () => current(generation) && service?.capabilities.join && session.expiresAtMs > readNow();
    if (!service || !live()) return;
    if (session.visibility === "private" && (!service.privateInvitations || connection?.state.privateInvitations !== true || session.joinPolicy !== "open")) return;
    if (session.joinPolicy === "approval") {
      if (!service.organization) throw new Error("Os pedidos de entrada não estão disponíveis neste serviço.");
      const checkingExisting = waitingForApproval(session);
      const status = await service.organization.joinRequestStatus(session.id);
      if (!live()) return;
      if (status === "none" && checkingExisting) {
        setWaitingJoinRequests(previous => { const next = { ...previous }; delete next[session.id]; return next; });
        return "Este pedido não está mais disponível. Use Pedir para entrar para enviar outro.";
      }
      if (status !== "accepted") {
        if (status === "none") await service.organization.requestJoin(session.id);
        if (!live()) return;
        // This account-scoped hint is presentation only. Every explicit check
        // still reads the service; it never admits a guest from cached state.
        setWaitingJoinRequests(previous => ({ ...Object.fromEntries(Object.entries(previous)
          .filter(([id, expires]) => id !== session.id && expires > readNow()).slice(-(MAX_VISIBLE_SESSIONS - 1))),
          [session.id]: session.expiresAtMs }));
        return status === "none" ? "Pedido enviado. Use Verificar pedido para consultar a aprovação."
          : "Aguardando aprovação do anfitrião. Use Verificar pedido para consultar novamente.";
      }
      setWaitingJoinRequests(previous => { const next = { ...previous }; delete next[session.id]; return next; });
    }
    if (!live()) return;
    // A click requests navigation only. The native owner still authenticates
    // this session, including any separate voice review/consent.
    const intent: JoinNavigation | null = onOpenConnectedGame && connection?.state.available ? {
      accountId, service, controller: connection.controller, epoch: generation, sessionId: session.id,
      expiresAtMs: session.expiresAtMs, previousOperationId: connection.state.snapshot?.operationId ?? null,
      minimumGeneration: connection.state.snapshot?.generation ?? 0, operationId: null, dispatched: false,
    } : null;
    joinNavigation.current = intent;
    try { await service.prepareJoin(session.id); }
    catch (cause) { if (joinNavigation.current === intent) joinNavigation.current = null; throw cause; }
    finally {
      if (intent && current(generation) && joinNavigation.current === intent) {
        intent.dispatched = true;
        setJoinNavigationRevision(value => value + 1);
      }
    }
  }

  useEffect(() => {
    const intent = joinNavigation.current;
    if (!intent) return;
    const state = connection?.state, observed = state?.snapshot, social = observed?.social;
    if (!current(intent.epoch) || intent.accountId !== accountId || intent.service !== service
      || intent.controller !== connection?.controller || !state?.available || state.error || !onOpenConnectedGame
      || !(intent.expiresAtMs > readNow())) { joinNavigation.current = null; return; }
    if (!observed?.operationId || observed.operationId === intent.previousOperationId) {
      if (intent.operationId || intent.dispatched && !state.pending) joinNavigation.current = null;
      return;
    }
    if (observed.errorCode || observed.generation < intent.minimumGeneration || observed.role !== "guest" || social?.role !== "guest"
      || social.sessionId !== intent.sessionId || intent.operationId && intent.operationId !== observed.operationId
      || ["closing", "closed", "failed"].includes(observed.phase) || ["closed", "failed"].includes(social.stage)
      || ["complete", "failed"].includes(observed.cleanupState)
      || social.expiresAtMs !== null && !(social.expiresAtMs > Date.now())) { joinNavigation.current = null; return; }
    intent.operationId = observed.operationId;
    intent.minimumGeneration = observed.generation;
    if (!intent.dispatched || state.pending || observed.phase !== "guestConnected" || observed.transportState !== "authenticated"
      || social.stage !== "connected" || social.readyGameLaunchAvailable !== true || !observed.tcpAddress
      || social.expiresAtMs === null) return;
    joinNavigation.current = null;
    onOpenConnectedGame();
  }, [connection, service, accountId, onOpenConnectedGame, now, joinNavigationRevision]);

  const waitingForApproval = (session: SocialSession) => session.joinPolicy === "approval"
    && session.expiresAtMs > now && waitingJoinRequests[session.id] === session.expiresAtMs;

  async function cancelEntryRequest(session: SocialSession) {
    const organization = service?.organization;
    const generation = epoch.current;
    if (!organization?.cancelJoinRequest || !waitingForApproval(session) || !(session.expiresAtMs > readNow())) return;
    // The transaction independently requires a still-pending request. If the
    // host already accepted it, retain the hint so the user can check again.
    await organization.cancelJoinRequest(session.id);
    if (!current(generation)) return;
    setWaitingJoinRequests(previous => {
      if (previous[session.id] !== session.expiresAtMs) return previous;
      const next = { ...previous }; delete next[session.id]; return next;
    });
    return "Pedido de entrada cancelado. Você pode enviar outro pedido quando quiser.";
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
    const request = action === "accept" ? (inbox?.requests ?? snapshot?.incoming)?.find(item => item.uid === person.uid) : undefined;
    if (action === "accept" && !Number.isFinite(readNow())) { setError(socialError({ code: "social_clock_unavailable" })); return; }
    if (action === "accept" && (!request || !Number.isFinite(request.expiresAtMs) || request.expiresAtMs <= readNow())) { setError("Este pedido expirou. Você pode recusá-lo para removê-lo da lista."); return; }
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
    if ((inbox?.requests ?? snapshot?.incoming)?.some(item => item.uid === person.uid)) return "Pedido recebido";
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
  const manualConnectReady = !!onOpenManualConnect && connection?.state.available === true && !!connectSnapshot
    && !connectionBusy && !connection.state.error;
  const hostPublicationLocked = !!snapshot?.ownSession && !!connectSnapshot?.operationId
    && connectSnapshot.role === "host" && connectSnapshot.social?.role === "host"
    && connectSnapshot.social.sessionId === snapshot.ownSession.id
    && !(["closed", "failed"].includes(connectSnapshot.phase) && connectSnapshot.cleanupState === "complete");
  const publicationDisabled = disabled || hostPublicationLocked;
  const visibleSessions = (scope === "public" ? sessions : (snapshot?.friends.flatMap(friend => friend.session ? [friend.session] : []) ?? []))
    .filter(session => session.expiresAtMs > now && `${session.title} ${session.modpackName} ${session.minecraftVersion}`.toLocaleLowerCase().includes(filter.trim().toLocaleLowerCase()));
  const friends = (friendTab === "incoming" ? inbox?.requests : undefined) ?? snapshot?.[friendTab] ?? [];
  const quickShare = !!connection?.state.ownedGameTcpDiscovery && connection.state.available && !!snapshot && !snapshot.ownSession
    && !connectionBusy && !connection.state.error && !!service?.capabilities.publication;
  const receivedCount = inbox?.count ?? snapshot?.incoming.filter(item => item.expiresAtMs > now).length ?? 0;
  const liveActivity = (person: SocialSnapshot["friends"][number]) => person.presence
    && person.presence.updatedAtMs <= now && person.presence.updatedAtMs + 120_000 > now ? person.presence.modpackName : null;

  return <section className="social-view" data-panel={view} aria-label="Amigos e comunidade">
    <nav className="social-mode-nav" aria-label="Opções de multiplayer">
      <button type="button" aria-pressed={view === "play"} onClick={() => setView("play")}>Encontrar mundos</button>
      <button type="button" aria-pressed={view === "friends"} onClick={() => { setView("friends"); if (receivedCount) setFriendTab("incoming"); }}>
        Meus amigos{receivedCount > 0 && <span className="social-request-count">{receivedCount}</span>}
      </button>
      <button type="button" aria-pressed={view === "share"} onClick={() => setView("share")}>Compartilhar mundo</button>
      {(service?.privateInvitations || discord) && <button type="button" aria-pressed={view === "invites"} onClick={() => setView("invites")}>Convites</button>}
      {available && <button type="button" className="secondary social-refresh" disabled={disabled} onClick={() => void run(async () => undefined, "Lista atualizada.")}>{loading ? "Carregando…" : pending ? "Aguarde…" : "Atualizar"}</button>}
    </nav>
    {discord?.sessionId && service && <DiscordInvitation key={discord.sessionId} id={discord.sessionId} service={service} accountId={accountId} now={now}
      disabled={disabled || connectionBusy} waiting={waitingForApproval} dismiss={discord.dismiss} onJoin={candidate => {
        const generation = epoch.current;
        if (!connectionBusy && !disabled) void run(async () => {
          const fresh = await service.resolvePublicSession?.(candidate.id);
          if (!current(generation)) return;
          if (!fresh || fresh.id !== candidate.id || fresh.visibility !== "public" || fresh.hostUid === accountId || !(fresh.expiresAtMs > readNow())) return "Esta sessão pública está indisponível para sua conta.";
          const result = await enterSession(fresh);
          if (current(generation) && !result) discord.dismiss();
          return result;
        }, "", false);
      }} />}
    {!available ? <div className="social-notice" data-kind="warning"><strong>O serviço social ainda não está habilitado.</strong><p>Amigos, busca e comunidade dependem da configuração autorizada do serviço. Nenhuma solicitação foi enviada.</p></div> : <>
      {error && <div className="social-notice" data-kind="error" role="alert">{error}</div>}
      {message && <div className="social-notice" role="status">{message}</div>}
      {!loading && !Number.isFinite(now) && <p className="social-notice" role="status">O horário do serviço está indisponível. Atualize a lista para conferir anúncios e pedidos.</p>}

      {availability.error && <div className="social-notice" role="status">Disponibilidade não verificada. {availability.error}</div>}
    </>}
    {connection && (connectionBusy || connectSnapshot?.social || connection.state.error) && <SocialConnectionPanel connection={connection} ownSession={snapshot?.ownSession} now={Date.now()} catalogNow={now} onOpenConnectedGame={onOpenConnectedGame} onPrepareDedicatedServer={onPrepareDedicatedServer} service={service} instanceNames={instanceNames} />}
    <div className="social-layout social-layout-focused">
      <aside className="social-friends" aria-label="Amigos do Aurora" hidden={view !== "friends"}>
        <div className="social-self"><Avatar name={nickname} /><div><strong>{nickname}</strong><small>{snapshot?.presenceSharing === "friends" ? snapshot.modpackName || "Atividade compartilhada com amigos" : "Atividade oculta"}</small></div></div>
        <form className="social-search" onSubmit={search}><label htmlFor="social-nickname">Adicionar pelo nick do Aurora</label><div className="social-search-row"><input id="social-nickname" autoComplete="off" spellCheck={false} maxLength={16} value={query} disabled={disabled} placeholder="Nick completo" onChange={event => { setQuery(event.target.value); setResult(null); setSearched(false); }} /><button disabled={disabled || !query.trim()}>Buscar</button></div>
          {searched && <div className="social-search-result">{result ? <><div className="social-person"><Avatar name={result.nickname} /><div><strong>{result.nickname}</strong><small>{relation(result) || "Conta Aurora"}</small></div></div>{!relation(result) && <button type="button" disabled={disabled || !snapshot} onClick={() => actOn(result, "request")}>Adicionar amigo</button>}</> : <small role="status">Nenhuma conta disponível com esse nick.</small>}</div>}
        </form>
        <div className="social-tabs" aria-label="Listas de amizade">{friendTabs.map(([key, title]) => <button key={key} aria-pressed={friendTab === key} onClick={() => setFriendTab(key)}>{title}{key === "incoming" && inbox ? ` ${inbox.requests.length}` : snapshot ? ` ${snapshot[key].length}` : ""}</button>)}</div>
        {!friends.length ? <Empty title={loading ? "Carregando amigos…" : friendTab === "friends" ? "Nenhum amigo adicionado" : "Nenhum item nesta lista"}>{friendTab === "friends" ? "Busque o nick de alguém no Aurora e envie um pedido de amizade." : "Os pedidos e as alterações aparecem após atualizar."}</Empty> : <ul className="social-list">{friends.map(person => {
          const friend = snapshot?.friends.find(item => item.uid === person.uid);
          const activity = friend ? liveActivity(friend) : null;
          const request = friendTab === "incoming" || friendTab === "outgoing" ? (friendTab === "incoming" && inbox ? inbox.requests : snapshot?.[friendTab])?.find(item => item.uid === person.uid) : undefined;
          const expiredRequest = !!request && (!Number.isFinite(request.expiresAtMs) || request.expiresAtMs <= now);
          return <li key={person.uid} className="social-friend-row"><div className="social-person"><Avatar name={person.nickname} /><div><strong>{person.nickname}</strong><small>{friend ? activity || "Atividade não compartilhada" : expiredRequest ? "Pedido expirado" : friendTab === "incoming" ? "Quer ser seu amigo" : friendTab === "outgoing" ? "Aguardando resposta" : "Bloqueado"}</small></div></div>
            <div className="social-person-actions">
              {friendTab === "incoming" && <><button disabled={disabled || expiredRequest || !Number.isFinite(now)} onClick={() => actOn(person, "accept")}>Aceitar</button><button className="secondary" disabled={disabled} onClick={() => actOn(person, "decline")}>Recusar</button></>}
              {friendTab === "outgoing" && <button className="secondary" disabled={disabled} onClick={() => actOn(person, "cancel")}>Cancelar pedido</button>}
              {friendTab === "blocked" && <button className="secondary" disabled={disabled} onClick={() => actOn(person, "unblock")}>Desbloquear</button>}
              {friendTab === "friends" && friend?.session && <><SocialAvailabilityBadge sessionId={friend.session.id} value={availability.values.get(friend.session.id)} observe={availability.observe} />{waitingForApproval(friend.session) && <small>Aguardando aprovação</small>}<button disabled={disabled || connectionBusy || !service?.capabilities.join || (friend.session.joinPolicy === "approval" && !service.organization) || !(friend.session.expiresAtMs > now)} onClick={() => { if (!connectionBusy && friend.session && friend.session.expiresAtMs > readNow() && service?.capabilities.join) void run(() => enterSession(friend.session!), "", false); }}>{friend.session.joinPolicy === "approval" ? waitingForApproval(friend.session) ? "Verificar pedido" : "Pedir para entrar" : "Entrar"}</button>{waitingForApproval(friend.session) && service?.organization?.cancelJoinRequest && <button className="secondary" disabled={disabled || connectionBusy} onClick={() => void run(() => cancelEntryRequest(friend.session!), "", false)}>Cancelar pedido</button>}</>}
              {friendTab !== "blocked" && <details className="social-person-menu"><summary>Opções</summary><div>{friendTab === "friends" && <button className="secondary" disabled={disabled} onClick={() => actOn(person, "remove")}>Remover amigo</button>}<button className="danger" disabled={disabled} onClick={() => actOn(person, "block")}>Bloquear</button></div></details>}
            </div></li>;
        })}</ul>}
        {available && snapshot && <div className="social-search"><label className="social-consent"><input type="checkbox" checked={snapshot.presenceSharing === "friends"} disabled={disabled} onChange={event => void run(() => service!.setPresence({ sharing: event.target.checked ? "friends" : "hidden", modpackName: event.target.checked ? observedModpack : null }), event.target.checked ? "Atividade visível somente para amigos." : "Atividade oculta.")} />Compartilhar minha atividade com amigos</label>
          <small>{observedModpack ? `Em execução: ${observedModpack}` : "A atividade poderá mostrar o modpack quando um jogo iniciar pelo Aurora."}</small>
          {snapshot.presenceSharing === "friends" && <><button className="secondary" disabled={disabled} onClick={() => void run(() => service!.setPresence({ sharing: "friends", modpackName: observedModpack }), "Atividade atualizada para amigos.")}>Atualizar atividade</button><small>A atualização vale por 2 minutos. Nenhuma renovação é automática nesta versão.</small></>}
        </div>}
      </aside>
      <div className="social-community" hidden={view === "friends"}>
        <div className="social-play" hidden={view !== "play"}>
        <header className="social-community-heading"><div><h3>{scope === "friends" ? "Mundos dos amigos" : "Mundos da comunidade"}</h3></div><div className="social-layout-switch" aria-label="Visualização das sessões"><button aria-pressed={layout === "grid"} onClick={() => setLayout("grid")}>Grade</button><button aria-pressed={layout === "list"} onClick={() => setLayout("list")}>Lista</button></div></header>
        <div className="social-filters"><div className="social-tabs" aria-label="Origem das sessões"><button aria-pressed={scope === "public"} onClick={() => setScope("public")}>Comunidade</button><button aria-pressed={scope === "friends"} onClick={() => setScope("friends")}>Dos amigos</button></div><input aria-label="Filtrar sessões carregadas" placeholder="Filtrar modpack ou versão" value={filter} onChange={event => setFilter(event.target.value)} /></div>
        {available && !service?.capabilities.join && <p className="social-notice">O serviço online não está ativado neste executável. Você pode organizar amizades e descobrir anúncios.</p>}
        {!visibleSessions.length ? <Empty title={loading ? "Procurando sessões…" : "Nenhuma sessão disponível"}>{filter ? "Nenhum anúncio carregado corresponde ao filtro." : scope === "friends" ? "As sessões compartilhadas pelos seus amigos aparecerão aqui." : "Os mundos publicados voluntariamente pelos anfitriões aparecerão aqui."}</Empty> : <div className="social-catalog" data-layout={layout}>{visibleSessions.map(session => <SessionCard key={session.id} session={session} now={now} availability={availability} waitingForApproval={waitingForApproval(session)} pending={disabled || connectionBusy || session.hostUid === accountId} joinEnabled={service?.capabilities.join === true && (session.joinPolicy === "open" || !!service.organization)} onJoin={() => { if (!connectionBusy && session.hostUid !== accountId && session.expiresAtMs > readNow() && service?.capabilities.join) void run(() => enterSession(session), "", false); }} onCancelRequest={service?.organization?.cancelJoinRequest ? () => void run(() => cancelEntryRequest(session), "", false) : undefined} />)}</div>}
        {scope === "public" && nextCursor && <div className="social-pagination"><button className="secondary" disabled={disabled} onClick={() => void more()}>Carregar mais</button></div>}
        {sessions.length >= MAX_VISIBLE_SESSIONS && <small>Exibindo até 100 anúncios. Atualize para consultar a lista novamente.</small>}
        </div>
        <div className="social-host" hidden={view !== "share"}>
        {quickShare && connection && service && <LanWorldShare nickname={nickname} instanceNames={instanceNames ?? {}} allowApproval={!!service.organization} disabled={disabled} now={Date.now()}
          onPublish={(world, source) => {
            if (disabled || !quickShare || source.expiresAtMs <= Date.now() || (world.joinPolicy === "approval" && !service.organization)) return;
            const generation = epoch.current;
            void run(async () => {
              const id = await service.publishSession(validateSocialSession(world), true);
              if (!current(generation)) return;
              if (!id || source.expiresAtMs <= Date.now()) return "Mundo publicado. Verifique o mundo LAN novamente para preparar a conexão.";
              await connection.controller.prepareHostDiscovered(id, world.visibility, source.selectionId);
              return world.visibility === "public" ? "Mundo publicado para a comunidade." : "Mundo publicado para seus amigos.";
            }, "");
          }} />}
        {available && service?.capabilities.publication && snapshot && <details className="social-publish theme-protected" open={view === "share" && !quickShare && !connectionBusy && !connection?.state.error}><summary>{snapshot.ownSession ? "Meu mundo" : quickShare ? "Anúncio personalizado e convites privados" : "Abrir meu mundo"}</summary>
          <form onSubmit={event => { event.preventDefault(); if (publicationDisabled || (draft.visibility !== "private" && !publicationConsent)
            || (service.privateInvitations && draft.visibility === "private" && draft.joinPolicy !== "open")) return; void run(async () => { await service.publishSession(validateSocialSession(draft)); }, draft.visibility === "private"
            ? service.privateInvitations ? "Anúncio privado salvo. Use Convites privados para selecionar contas. O anúncio não inicia o compartilhamento." : "Anúncio privado salvo. Ele não envia convites nem inicia o compartilhamento."
            : connection?.state.available
            ? "Anúncio atualizado. Use Preparar meu servidor para ativar o compartilhamento."
            : "Anúncio atualizado. O compartilhamento do servidor ainda não foi ativado."); }}>
            <p>O anúncio não abre portas, inicia o jogo ou comprova que o servidor está acessível.</p>
            {draft.visibility === "private" && <div className="social-notice">
              <p>{service.privateInvitations
                ? "Use Convites privados para buscar uma conta pelo nick do Aurora. Salvar novamente este anúncio cria outra sessão e exige novos convites. Os códigos manuais continuam como alternativa."
                : "Para convidar alguém diretamente, use os convites manuais do Connect e troque os códigos pelo canal combinado. O anúncio só muda quando você salva; ainda não há entrega de convite para uma conta Aurora."}</p>
              {onOpenManualConnect ? <details className="social-extra"><summary>Convite manual avançado</summary><button type="button" className="secondary" disabled={disabled || !manualConnectReady} onClick={() => { if (!disabled && manualConnectReady) onOpenManualConnect(); }}>Abrir convites do Connect</button>
                {!manualConnectReady && <small>{connectionBusy ? "Encerre a conexão atual e aguarde a liberação dos recursos antes de preparar outro convite." : "Atualize a conexão para conferir se os recursos estão liberados."}</small>}</details>
                : <small>Os convites manuais do Connect não estão disponíveis neste executável.</small>}
            </div>}
            {hostPublicationLocked && <p className="social-notice" role="status">Este anúncio está vinculado à hospedagem em andamento. Use Desconectar e aguarde a liberação dos recursos antes de editar ou salvar.</p>}
            {snapshot.ownSession && <div className="social-notice"><strong>{snapshot.ownSession.title}</strong><small>{snapshot.ownSession.expiresAtMs <= now ? "Anúncio expirado" : "Anúncio registrado"}</small><p>Encerrar o anúncio revoga a autorização desta sessão e encerra a entrada e as conexões dos participantes.</p><div className="social-actions"><button type="button" className="secondary" disabled={disabled} onClick={() => void run(() => service.closeSession(), "Anúncio encerrado.")}>Encerrar anúncio</button></div></div>}
            <details className="social-publish-settings" open={!snapshot.ownSession}>
              <summary>Dados e acesso do mundo</summary>
              <div className="social-fields">
              <label>Título<input required maxLength={64} value={draft.title} disabled={publicationDisabled} onChange={event => editDraft({ title: event.target.value })} /></label>
              <label>Modpack<input required maxLength={64} value={draft.modpackName} disabled={publicationDisabled} onChange={event => editDraft({ modpackName: event.target.value })} /></label>
              <label>Versão do Minecraft<input required maxLength={24} value={draft.minecraftVersion} disabled={publicationDisabled} onChange={event => editDraft({ minecraftVersion: event.target.value })} placeholder="Ex.: 1.21.1" /></label>
              <label>Loader<input required maxLength={24} value={draft.loader} disabled={publicationDisabled} onChange={event => editDraft({ loader: event.target.value })} placeholder="Ex.: Fabric" /></label>
              <label className="social-field-wide">Imagem do modpack<input type="url" maxLength={512} value={draft.imageUrl ?? ""} disabled={publicationDisabled} onChange={event => editDraft({ imageUrl: event.target.value || null })} placeholder="Imagem do catálogo Modrinth ou CurseForge (opcional)" /></label>
              <label>Quem pode descobrir<select value={draft.visibility} disabled={publicationDisabled} onChange={event => { editDraft({ visibility: event.target.value as SocialSessionDraft["visibility"], ...(service.privateInvitations && event.target.value === "private" ? { joinPolicy: "open" } : {}) }); setPublicationConsent(false); }}><option value="private">Privada</option><option value="friends">Meus amigos</option><option value="public">Toda a comunidade</option></select></label>
              <label>Política de ingresso<select value={draft.joinPolicy} disabled={publicationDisabled} onChange={event => editDraft({ joinPolicy: event.target.value as SocialSessionDraft["joinPolicy"] })}><option value="open">Entrada aberta</option><option value="approval" disabled={!!service.privateInvitations && draft.visibility === "private"}>Aprovar cada pedido</option></select></label>
              <label>Limite total do anúncio<input type="number" min={1} max={100} value={draft.maxPlayers} disabled={publicationDisabled} onChange={event => editDraft({ maxPlayers: Number(event.target.value) })} /></label>
            </div>
            {service.privateInvitations && draft.visibility === "private" && <small>A entrada é aberta somente às contas convidadas; não há um segundo pedido de aprovação.</small>}
            {draft.visibility !== "private" && <label className="social-consent"><input type="checkbox" checked={publicationConsent} disabled={publicationDisabled} onChange={event => setPublicationConsent(event.target.checked)} />Quero publicar estes dados {draft.visibility === "friends" ? "para meus amigos" : "para a comunidade do Aurora"}. Posso encerrar o anúncio.</label>}
            <div className="social-actions"><button disabled={publicationDisabled || (draft.visibility !== "private" && !publicationConsent) || (!!service.privateInvitations && draft.visibility === "private" && draft.joinPolicy !== "open")}>Salvar anúncio</button></div>
            </details>
          </form>
          {connection && !connectionBusy && !connectSnapshot?.social && !connection.state.error && <SocialConnectionPanel connection={connection} ownSession={snapshot.ownSession} now={Date.now()} catalogNow={now} onOpenConnectedGame={onOpenConnectedGame} onPrepareDedicatedServer={onPrepareDedicatedServer} service={service} instanceNames={instanceNames} />}
        </details>}
        </div>
        {service?.organization && <details className="social-extra" onToggle={event => { if (event.target === event.currentTarget) setGroupsExpanded(event.currentTarget.open); }}><summary>Grupos e pedidos</summary><SocialOrganizationPanel service={service.organization} accountId={accountId} friends={snapshot?.friends ?? []} expanded={groupsExpanded} /></details>}
        <div hidden={view !== "invites"} className="social-invitations">
        {service?.privateInvitations && <details className="social-extra theme-protected" open={view === "invites"}><summary>Convites privados</summary><SocialPrivateInvitationsPanel service={service} ownSession={snapshot?.ownSession} friends={snapshot?.friends}
          joinEnabled={service.capabilities.join && connection?.state.available === true && connection.state.privateInvitations === true && !connection.state.error}
          connectionBusy={connectionBusy || disabled} onJoin={enterSession} /></details>}
        {discord && <DiscordSettingsPanel controls={discord} />}
        </div>
        {onOpenManualConnect && <details className="social-extra"><summary>Opções avançadas e recuperação</summary>
          <p>Use estes controles para convites manuais ou para conferir uma conexão, mesmo se a comunidade estiver indisponível.</p>
          <button type="button" className="secondary" onClick={onOpenManualConnect}>Conexão manual e recuperação</button>
        </details>}
      </div>
    </div>
  </section>;
}
