import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import type { ConnectSnapshot } from "../connect/types";
import { ReadyCopyPicker } from "./ReadyCopyPicker";
import type { SocialPlayRequest } from "./socialPlay";
import {
  isReadyGameError, readyGameCapabilitySupports, readyGameContextFingerprint, readyGameError, readyGameSnapshotFrom,
  sameReadyGameContext, validReadyGameContext, type ReadyGameLaunchCapability, type ReadyGameLaunchContext,
  type ReadyGameLaunchReview, type ReadyGameLaunchSnapshot,
} from "./types";

type Props = { capability?: ReadyGameLaunchCapability; session?: ConnectSnapshot; launchContext?: ReadyGameLaunchContext; observeExisting?: boolean; sessionStale?: boolean; onChooseSource?: () => void; revealRequest?: number; revealReady?: boolean; socialPlay?: SocialPlayRequest };
type Action = "prepare" | "confirm" | "cancel";
const active = (snapshot?: ReadyGameLaunchSnapshot) => !!snapshot && ["preparing", "reviewed", "starting"].includes(snapshot.phase);
const hostPlayer = (session?: ConnectSnapshot) => session?.role === "host" && session.social?.role === "host"
  && session.social.hostPlayer === true;
const launchParticipant = (session?: ConnectSnapshot) => session?.role === "guest" || hostPlayer(session);
const connected = (session?: ConnectSnapshot) => !!session?.tcpAddress
  && !["complete", "failed"].includes(session.cleanupState)
  && (session.role === "guest" && session.phase === "guestConnected" && session.transportState === "authenticated"
    || hostPlayer(session) && session.phase === "hostListening" && session.social?.readyGameLaunchAvailable
      && ["authenticated", "admitted"].includes(session.transportState));
const date = (value: number) => new Date(value).toLocaleTimeString("pt-BR");
const readClocks = () => ({ wall: Date.now(), monotonic: performance.now() });

export function ConnectGameLaunchPanel({ capability, session, launchContext: sourceLaunchContext, observeExisting = false, sessionStale = false, onChooseSource, revealRequest = 0, revealReady = true, socialPlay }: Props) {
  const operationId = session?.operationId ?? undefined;
  const heading = useRef<HTMLHeadingElement>(null);
  const revealed = useRef(0);
  const revealAllowed = useRef(false);
  revealAllowed.current = revealReady && !sessionStale;
  const [snapshot, setSnapshot] = useState<ReadyGameLaunchSnapshot>();
  const [selectedId, setSelectedId] = useState("");
  const [copyJava, setCopyJava] = useState<{ id: string; source: string; path: string }>();
  const suggestion = useRef<{ gesture: SocialPlayRequest; id: string } | undefined>(undefined);
  const launchContext = sourceLaunchContext && copyJava && selectedId === copyJava.id && sourceLaunchContext.sourceInstanceId === copyJava.source
    ? { ...sourceLaunchContext, javaExecutable: copyJava.path } : sourceLaunchContext;
  const fingerprint = readyGameContextFingerprint(launchContext);
  const sourceFingerprint = readyGameContextFingerprint(sourceLaunchContext);
  const [acknowledged, setAcknowledged] = useState(false);
  const [pending, setPending] = useState<Action>();
  const [uncertain, setUncertain] = useState(false);
  const [error, setError] = useState("");
  const [warning, setWarning] = useState("");
  const [contextChanged, setContextChanged] = useState(false);
  const [now, setNow] = useState(readClocks);
  const observed = useRef<ReadyGameLaunchSnapshot | undefined>(undefined);
  const reviewClock = useRef<{ operationId: string; planId: string; digest: string; deadline: number } | undefined>(undefined);
  const watchedOperation = useRef<string | undefined>(undefined);
  if (operationId && launchParticipant(session) && (capability || observeExisting)) watchedOperation.current = operationId;
  const watch = !!operationId && watchedOperation.current === operationId;
  const previousContext = useRef(sourceFingerprint);
  const current = useRef({ operationId, capability, session, launchContext, fingerprint, sessionStale });
  current.current = { operationId, capability, session, launchContext, fingerprint, sessionStale };
  const mounted = useRef(false);
  const epoch = useRef(0);
  const mutation = useRef<{ id: number; action: Action } | undefined>(undefined);
  const mutationId = useRef(0);
  const uncertainAfter = useRef<number | undefined>(undefined);
  const invalidated = useRef(false);
  const cancelling = useRef<string | undefined>(undefined);
  const automaticCancellation = useRef(new Set<string>());
  const requestedInstance = useRef<string | undefined>(undefined);

  useEffect(() => {
    if (!revealReady || sessionStale || revealRequest <= revealed.current || !snapshot
      || snapshot.operationId !== operationId || !heading.current) return;
    const target = heading.current;
    // Navigation only: wait for the current native observation and committed
    // layout. It never selects a copy or carries consent into a new review.
    const frame = window.requestAnimationFrame(() => {
      if (!revealAllowed.current || !target.isConnected || target.closest("[hidden], [inert]")) return;
      target.focus({ preventScroll: true });
      target.scrollIntoView?.({ block: "start", behavior: "auto" });
      revealed.current = revealRequest;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [revealRequest, revealReady, sessionStale, operationId, snapshot?.operationId]);

  function anchoredDeadline(review: ReadyGameLaunchReview) {
    const anchor = reviewClock.current;
    return anchor?.operationId === review.operationId && anchor.planId === review.planId && anchor.digest === review.digest
      ? anchor.deadline : undefined;
  }

  function reviewCurrent(review: ReadyGameLaunchReview, source: ConnectSnapshot | undefined, time: ReturnType<typeof readClocks>) {
    const deadline = anchoredDeadline(review);
    if (deadline !== undefined) return time.monotonic < deadline;
    return !source?.social && time.wall < review.expiresAtMs && time.wall < review.sessionExpiresAtMs;
  }

  function accept(value: unknown, expectedOperation: string) {
    const next = readyGameSnapshotFrom(value);
    if (next.operationId !== expectedOperation || current.current.operationId !== expectedOperation) throw new Error("Wrong ready launch operation");
    const previous = observed.current;
    if (previous && next.generation < previous.generation) return;
    if (previous && next.generation === previous.generation && JSON.stringify(next) !== JSON.stringify(previous)) throw new Error("Changed ready launch generation");
    if (requestedInstance.current && (next.review && next.review.instanceId !== requestedInstance.current
      || next.result && next.result.instanceId !== requestedInstance.current)) throw new Error("Changed ready launch instance");
    const time = readClocks();
    if (!next.review) reviewClock.current = undefined;
    else if (current.current.session?.social && anchoredDeadline(next.review) === undefined) {
      const remaining = Math.min(
        (current.current.capability?.maxPlanSeconds ?? 60) * 1000,
        Math.min(next.review.expiresAtMs, next.review.sessionExpiresAtMs) - time.wall,
      );
      reviewClock.current = { operationId: next.operationId, planId: next.review.planId,
        digest: next.review.digest, deadline: time.monotonic + Math.max(0, remaining) };
    }
    observed.current = next; setSnapshot(next); setNow(time); setWarning("");
    if (previous?.review?.digest !== next.review?.digest || previous?.planId !== next.planId) setAcknowledged(false);
    if (uncertainAfter.current !== undefined && (next.generation > uncertainAfter.current || !active(next) && next.phase !== "idle")) {
      uncertainAfter.current = undefined; setUncertain(false);
    }
    if (next.review && sameReadyGameContext(next.review, current.current.launchContext) && !invalidated.current) setSelectedId(next.review.instanceId);
    if (next.review && !sameReadyGameContext(next.review, current.current.launchContext)) {
      invalidated.current = true; setContextChanged(true); setAcknowledged(false);
    }
    if (!active(next)) cancelling.current = undefined;
    if (invalidated.current && active(next) && next.planId && !automaticCancellation.current.has(next.planId)) {
      automaticCancellation.current.add(next.planId); void cancel(next);
    }
  }

  async function read(expectedEpoch = epoch.current) {
    const op = current.current.operationId;
    if (!op) return;
    try {
      const value = await invoke<unknown>("connect_ready_game_launch_snapshot", { operationId: op });
      if (mounted.current && epoch.current === expectedEpoch) accept(value, op);
    } catch {
      if (mounted.current && epoch.current === expectedEpoch) {
        setWarning("O preparo do jogo não pôde ser atualizado. A última observação pode estar desatualizada.");
        setAcknowledged(false);
      }
    }
  }

  async function perform(action: Action, command: string, args: Record<string, unknown>) {
    if (mutation.current && action !== "cancel" || mutation.current?.action === "cancel") return;
    const op = current.current.operationId;
    if (!op) return;
    const requestEpoch = epoch.current;
    const id = ++mutationId.current;
    const baseline = observed.current?.generation ?? 0;
    mutation.current = { id, action }; setPending(action); setError(""); setAcknowledged(false);
    const live = () => mounted.current && requestEpoch === epoch.current && current.current.operationId === op;
    try {
      const value = await invoke<unknown>(command, args);
      if (live()) accept(value, op);
    } catch (cause) {
      if (live()) {
        setError(readyGameError(cause));
        if (!isReadyGameError(cause)) {
          uncertainAfter.current = baseline; setUncertain(true);
        }
        // A lost response may follow a consumed confirmation. Observe only;
        // never issue a second prepare/confirm from this recovery path.
        await read(requestEpoch);
      }
    } finally {
      if (live() && mutation.current?.id === id) {
        mutation.current = undefined; setPending(undefined);
      }
    }
  }

  async function cancel(target = observed.current) {
    if (!target?.planId || !active(target) || target.operationId !== current.current.operationId) return;
    cancelling.current = target.planId;
    await perform("cancel", "connect_cancel_ready_game_launch", { operationId: target.operationId, planId: target.planId });
    if (cancelling.current === target.planId && active(observed.current)) cancelling.current = undefined;
  }

  useEffect(() => {
    mounted.current = true;
    const localEpoch = ++epoch.current;
    observed.current = undefined; reviewClock.current = undefined; mutation.current = undefined; uncertainAfter.current = undefined;
    invalidated.current = false; cancelling.current = undefined;
    automaticCancellation.current.clear();
    requestedInstance.current = undefined;
    setSnapshot(undefined); setSelectedId(""); setAcknowledged(false); setPending(undefined); setUncertain(false);
    setError(""); setWarning(""); setContextChanged(false);
    let timer: number | undefined;
    let disposed = false;
    async function poll() {
      if (disposed) return;
      if (operationId) await read(localEpoch);
      if (!disposed) timer = window.setTimeout(() => { setNow(readClocks()); void poll(); }, 1_000);
    }
    // Capability gates new work. Observation/cancellation remain available
    // when recovering a native operation after navigation or a failed query.
    if (watch) void poll();
    return () => { disposed = true; mounted.current = false; epoch.current++; if (timer !== undefined) window.clearTimeout(timer); };
  }, [operationId, watch]);

  useEffect(() => {
    if (previousContext.current === sourceFingerprint) return;
    previousContext.current = sourceFingerprint;
    setSelectedId(""); setAcknowledged(false);
    invalidated.current = true;
    if (active(observed.current) || mutation.current) setContextChanged(true);
    if (active(observed.current) && observed.current?.planId) {
      automaticCancellation.current.add(observed.current.planId); void cancel();
    }
  }, [sourceFingerprint]);


  const canChooseCopy = !!capability && connected(session) && !sessionStale
    && !!snapshot && !active(snapshot) && !pending && !uncertain && !warning;
  const canStartNew = canChooseCopy && validReadyGameContext(launchContext);
  const review = snapshot?.review && sameReadyGameContext(snapshot.review, launchContext) && !contextChanged ? snapshot.review : null;
  const supportedReview = !!review && readyGameCapabilitySupports(capability, review);
  useEffect(() => {
    if (sessionStale || !capability || !connected(session) || !supportedReview) setAcknowledged(false);
  }, [sessionStale, capability, supportedReview, session?.phase, session?.transportState, session?.cleanupState, session?.tcpAddress]);
  const currentReview = review && supportedReview && review.instanceId === selectedId && reviewCurrent(review, session, now);
  const social = session?.social;
  const matchingGesture = socialPlay && revealReady && !sessionStale && connected(session) && social?.readyGameLaunchAvailable
    && socialPlay.operationId === operationId && socialPlay.sessionId === social.sessionId && socialPlay.contextKey === sourceLaunchContext?.contextKey
    && (social.stage === "connected" || hostPlayer(session) && social.stage === "waitingGuest") ? socialPlay : undefined;
  // The button itself authorizes this exact displayed review. Additional files and voice still require their explicit consent.
  const playButton = !!matchingGesture && !!review && !social?.voiceProfile && !review.voice;
  const canConfirm = !!capability && connected(session) && !sessionStale && !!currentReview && (acknowledged || playButton) && !pending && !uncertain && !warning;

  useEffect(() => {
    const next = suggestion.current;
    if (!next) return;
    suggestion.current = undefined;
    if (next.gesture === matchingGesture && next.id === selectedId && snapshot?.phase === "idle" && !social?.voiceProfile && canStartNew) prepare();
  }, [matchingGesture, selectedId, snapshot?.phase, social?.voiceProfile, canStartNew]);

  function prepare() {
    if (!canStartNew || !selectedId || !operationId || !validReadyGameContext(launchContext)) return;
    invalidated.current = false; setContextChanged(false);
    requestedInstance.current = selectedId;
    void perform("prepare", "connect_prepare_ready_game_launch", { request: {
      operationId, instanceId: selectedId, sourceInstanceId: launchContext.sourceInstanceId,
      contextKey: launchContext.contextKey, javaExecutable: launchContext.javaExecutable,
      nickname: launchContext.nickname, appearance: launchContext.appearance,
    } });
  }
  function confirm() {
    const latest = observed.current?.review;
    if (!canConfirm || !latest || latest.digest !== review?.digest || latest.instanceId !== selectedId
      || !sameReadyGameContext(latest, current.current.launchContext) || !reviewCurrent(latest, current.current.session, readClocks())
      || !connected(current.current.session) || current.current.sessionStale
      || !readyGameCapabilitySupports(current.current.capability, latest)) {
      setAcknowledged(false); return;
    }
    void perform("confirm", "connect_confirm_ready_game_launch", { operationId: latest.operationId, planId: latest.planId, digest: latest.digest });
  }

  if (!capability && !snapshot && !pending && !warning) return null;
  if (!launchParticipant(session) && !snapshot && !pending && !warning) return null;
  return <section className="connect-card" aria-label="Abrir cópia pelo Connect">
    <h3 ref={heading} tabIndex={-1}>Abrir o Minecraft nesta conexão</h3>
    <p>Revisa uma cópia de teste preparada com Aurora Core e Skins antes de tentar entrar pelo Connect. A revisão confere o Minecraft, o loader e o Java instalados.</p>
    {!capability && <p role="status">Novos lançamentos estão indisponíveis. O preparo existente ainda pode ser consultado ou cancelado.</p>}
    {!connected(session) && <p>Conecte como convidado ou anfitrião jogador antes de preparar a entrada. Uma consulta ao servidor ou conexão TCP não confirma entrada no mundo.</p>}
    {sessionStale && <p role="status">Aguarde uma observação atual do Connect antes de autorizar a entrada.</p>}
    {!launchContext && <div>
      <p>Selecione uma instância em Instâncias e aguarde o perfil e a aparência. Ela pode estar vazia: você poderá preparar uma cópia separada com o perfil desta sessão.</p>
      {onChooseSource && <button className="secondary" type="button" onClick={onChooseSource}>Selecionar ou criar instância</button>}
    </div>}
    {launchContext && <p>Instância de origem selecionada: <strong>{launchContext.sourceInstanceId}</strong>. O jogo será preparado e aberto em uma cópia separada.</p>}
    {launchContext && !validReadyGameContext(launchContext) && <p>Confira o perfil e a aparência. Escolha um Java disponível ou prepare uma cópia de runtime com Java próprio.</p>}
    {contextChanged && <p role="status">O perfil ou a seleção mudou. O preparo anterior foi invalidado; aguarde a confirmação de cancelamento antes de revisar novamente.</p>}
    {error && <p role="alert">{error}</p>}
    {warning && <p role="alert">{warning}</p>}
    {uncertain && <p role="status">Aguardando uma observação que confirme o resultado. Nenhum lançamento será repetido automaticamente.</p>}
    {pending && <p role="status">{pending === "prepare" ? "Conferindo a cópia…" : pending === "confirm" ? "Iniciando a cópia…" : "Cancelando o preparo…"}</p>}
    {launchContext && <ReadyCopyPicker sourceInstanceId={launchContext.sourceInstanceId} contextKey={launchContext.contextKey}
      socialPlay={snapshot?.phase === "idle" ? matchingGesture : undefined}
      onSuggested={(id, java, gesture) => {
        if (gesture !== matchingGesture) return;
        suggestion.current = { gesture, id };
        setSelectedId(id); setCopyJava(java ? { id, source: launchContext.sourceInstanceId, path: java } : undefined);
        setAcknowledged(false);
      }}
      connectOperationId={operationId} requiredProfile={session?.social?.runtimeProfile} voiceProfile={session?.social?.voiceProfile} disabled={!canChooseCopy} selectedId={selectedId}
      runtimes={capability && (capability.schemaVersion === 1 ? [capability] : capability.runtimes)}
      onSelect={(id, java) => {
        setSelectedId(id);
        setCopyJava(java ? { id, source: launchContext.sourceInstanceId, path: java } : undefined);
        setAcknowledged(false);
      }} />}
    <button className="secondary" type="button" disabled={!canStartNew || !selectedId} onClick={prepare}>Revisar entrada no servidor</button>
    {operationId && <button className="secondary" type="button" onClick={() => { void read(); }}>Atualizar preparo do jogo</button>}
    {snapshot?.phase === "preparing" && <p role="status">Verificando a cópia, o Java e o protocolo informado pelo servidor. Os mods fixados de uma sessão com voz podem ser baixados para revisão.</p>}
    {review && <section className="connect-review" aria-label="Revisão para abrir a cópia">
      <h4>Confira antes de iniciar</h4>
      <dl>
        <dt>Cópia de teste</dt><dd>{review.instanceId}</dd>
        <dt>Destino desta conexão</dt><dd>{review.gameAddress}</dd>
        <dt>Jogo e loader</dt><dd>Minecraft {review.minecraftVersion} / {review.loader === "forge" ? "Forge" : "Fabric"} {review.loaderVersion}</dd>
        <dt>Java verificado</dt><dd>{review.javaExecutable} (Java {review.javaMajor})</dd>
        <dt>Apelido</dt><dd>{review.nickname}</dd>
        <dt>Aparência</dt><dd>{review.appearance.skinFile ? "Skin local selecionada" : review.appearance.skinUrl ? "Skin do perfil selecionada" : "Skin padrão"}; {review.appearance.capeFile || review.appearance.capeUrl ? "capa do perfil" : "capa do Minecraft"}; modelo {review.appearance.skinModel ?? "padrão"}.</dd>
        <dt>Módulos verificados</dt><dd>Core {review.coreVersion}; Skins {review.skinsVersion}</dd>
        <dt>Autorizar até</dt><dd>{date(review.expiresAtMs)}</dd>
        <dt>Conexão termina até</dt><dd>{date(review.sessionExpiresAtMs)}</dd>
      </dl>
      {review.voice && <div>
        <h5>Voz da sessão · {review.voice.profile === "simpleVoiceChat" ? "Simple Voice Chat" : "Plasmo Voice"}</h5>
        <ul>{review.voice.mods.map(mod => <li key={mod.relative}>
          {mod.action === "install" ? "Adicionar à cópia" : "Já presente e verificado"} · <code>{mod.relative}</code> · SHA-256 <code>{mod.sha256}</code>
        </li>)}</ul>
        {review.voice.mods.some(mod => mod.action === "install") && <p>A confirmação instala os arquivos indicados somente nesta cópia de teste. A instalação pode ser desfeita nas atualizações de conteúdo da cópia, com o jogo fechado.</p>}
      </div>}
      <p>O jogo pode gravar dados nesta cópia, no cache de aparência do launcher e nos registros do Doctor e logs. Iniciar o processo não confirma entrada no mundo, áudio ou compatibilidade de contas.</p>
      {!currentReview && <p role="status">Esta revisão venceu ou mudou. Prepare uma nova revisão.</p>}
      {!supportedReview && <p role="status">Esta versão do launcher não oferece o perfil desta cópia. Atualize os dados ou cancele o preparo.</p>}
      {!playButton && <label className="connect-consent"><input type="checkbox" checked={acknowledged} disabled={!currentReview || !!pending || uncertain || !capability || !connected(session) || sessionStale || !!warning}
        onChange={event => setAcknowledged(event.target.checked)} />{review.voice?.mods.some(mod => mod.action === "install")
          ? "Autorizo instalar os mods listados nesta cópia de teste e abrir a cópia para tentar entrar nesta conexão."
          : "Autorizo abrir esta cópia com os dados acima e tentar entrar nesta conexão."}</label>}
      {playButton && <p>Ao clicar em Jogar agora, você autoriza abrir esta cópia no servidor e com o perfil mostrados acima.</p>}
      <button className="primary" type="button" disabled={!canConfirm} onClick={confirm}>{playButton ? "Jogar agora" : "Iniciar cópia e tentar entrar"}</button>
    </section>}
    {snapshot?.phase === "starting" && <p role="status">A criação do processo está em andamento. Consulte o resultado antes de tentar outro lançamento.</p>}
    {snapshot?.phase === "started" && snapshot.result && <p role="status">A cópia foi iniciada e registrada no Doctor. Confira a entrada no mundo no Minecraft. Encerrar o Connect não fecha o jogo; saia pelo menu quando terminar.</p>}
    {snapshot?.phase === "cancelled" && <p role="status">Preparo cancelado. Nenhum convite foi renovado.</p>}
    {snapshot?.phase === "expired" && <p role="status">A revisão venceu. Nenhuma autorização foi renovada.</p>}
    {snapshot?.phase === "failed" && <p role="alert">{readyGameError(snapshot.errorCode)}</p>}
    {active(snapshot) && <button className="secondary" type="button" disabled={pending === "cancel"} onClick={() => { void cancel(); }}>Cancelar preparo do jogo</button>}
  </section>;
}
