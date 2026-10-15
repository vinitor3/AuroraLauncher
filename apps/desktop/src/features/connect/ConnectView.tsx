import { invoke, isTauri } from "@tauri-apps/api/core";
import { type FormEvent, useEffect, useRef, useState } from "react";
import {
  capabilitiesFrom, connectDate, connectError, exportFrom, phaseText, preparationFrom, reviewFrom,
  snapshotFrom, statusText, transportText, sameSessionTiming, type ConnectCapabilities, type ConnectMode, type ConnectInvitationProfile,
  type ConnectRole, type ConnectSnapshot,
} from "./types";
import "./connect.css";
import { DiscoveryPicker, type DiscoveredTarget } from "./DiscoveryPicker";
import { FragmentedFields, FragmentedReviewFields, FragmentedStatusFields } from "./FragmentedFields";
import { FRAGMENTED_PROFILE, SESSION_FRAGMENTED_PROFILE, fragmentedGuestUdpFrom, isDirectIpv4Address, isFragmentedLifetime, isLoopbackUdpTarget } from "./fragmented-types";
import { GroupHostPanel, type GroupAction } from "./GroupHostPanel";
import { groupPreparationFrom } from "./group-types";
import { ConnectGameLaunchPanel } from "../connect-game-launch/ConnectGameLaunchPanel";
import type { ReadyGameLaunchContext } from "../connect-game-launch/types";
import { isControlledSourceRejection, provesNoSourceReservation, type HostSourceProfile } from "./host-source-types";

type Action = "prepareHost" | "prepareGuest" | "reviewHost" | "reviewGuest" | "confirm" | "export" | "probe" | "retryUdp" | "stop" | "refresh" | GroupAction;
const actionText: Record<Action, string> = {
  prepareHost: "Preparando anfitrião…", prepareGuest: "Preparando convidado…", reviewHost: "Preparando revisão…",
  reviewGuest: "Verificando convite…", confirm: "Estabelecendo conexão…", export: "Obtendo convite…",
  probe: "Consultando servidor…", retryUdp: "Solicitando nova tentativa UDP…", stop: "Encerrando recursos…", refresh: "Consultando estado…",
  reviewGroup: "Preparando revisão do grupo…", confirmGroup: "Estabelecendo o grupo…", exportGroup: "Obtendo convite do participante…", revokeGroup: "Revogando participante…",
};
const finalPhase = (snapshot: ConnectSnapshot) => ["idle", "closed", "failed"].includes(snapshot.phase);
type SourcePreparationAwaiting = {
  kind: "group" | "single"; generation: number; previousOperationId: string | null; mode: ConnectMode;
  profile?: HostSourceProfile; baseline?: ConnectSnapshot; controlledRejectionSettled?: symbol;
};

export function ConnectView({ launchContext }: { launchContext?: ReadyGameLaunchContext }) {
  const [capabilities, setCapabilities] = useState<ConnectCapabilities>();
  const [snapshot, setSnapshot] = useState<ConnectSnapshot>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [warning, setWarning] = useState("");
  const [message, setMessage] = useState("");
  const [pending, setPending] = useState<Action>();
  const [selectedRole, setSelectedRole] = useState<ConnectRole>("host");
  const [mode, setMode] = useState<ConnectMode>("loopback");
  const [bindAddress, setBindAddress] = useState("127.0.0.1:42000");
  const [gameTarget, setGameTarget] = useState("127.0.0.1:43001");
  const [targetMode, setTargetMode] = useState<"manual" | "ownedInstance">("manual");
  const [discoveredTarget, setDiscoveredTarget] = useState<DiscoveredTarget>();
  const [discoveryEpoch, setDiscoveryEpoch] = useState(0);
  const [lifetime, setLifetime] = useState("600");
  const [invitationProfile, setInvitationProfile] = useState<"legacy" | "sessionGroup" | ConnectInvitationProfile>("legacy");
  const [fragmentedLifetime, setFragmentedLifetime] = useState("120");
  const [udpTarget, setUdpTarget] = useState("");
  const [admissionSeconds, setAdmissionSeconds] = useState("600");
  const [sessionSeconds, setSessionSeconds] = useState("3600");
  const [hostPublicId, setHostPublicId] = useState("");
  const [guestPublicId, setGuestPublicId] = useState("");
  const [invitation, setInvitation] = useState("");
  const [exportedInvitation, setExportedInvitation] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  const [knownOperationId, setKnownOperationId] = useState<string>();
  const [udpRecoveryAwaitingSnapshot, setUdpRecoveryAwaitingSnapshot] = useState(false);
  const [sourcePreparationUncertain, setSourcePreparationUncertain] = useState(false);
  const epoch = useRef(0);
  const mounted = useRef(false);
  const pendingRef = useRef<Action | undefined>(undefined);
  const snapshotRef = useRef<ConnectSnapshot | undefined>(undefined);
  const observedReview = useRef<string | undefined>(undefined);
  const exportedFor = useRef<string | undefined>(undefined);
  const udpRetryContext = useRef<{ operationId: string; generation: number } | undefined>(undefined);
  const sourcePreparationAwaiting = useRef<SourcePreparationAwaiting | undefined>(undefined);

  function acceptSnapshot(value: unknown, rejectionProof?: symbol) {
    const next = snapshotFrom(value);
    if (snapshotRef.current && next.generation < snapshotRef.current.generation) return;
    const awaitingSource = sourcePreparationAwaiting.current;
    if (awaitingSource) {
      // An old idle/closed observation cannot prove that a lost preparation
      // request failed before reserving its owned native worker.
      const definitiveObservation = awaitingSource.kind === "single" && rejectionProof !== undefined
        && rejectionProof === awaitingSource.controlledRejectionSettled;
      const rejectedBeforeReservation = definitiveObservation && provesNoSourceReservation(next, awaitingSource.baseline);
      if (!rejectedBeforeReservation) {
        if (next.generation <= awaitingSource.generation || !next.operationId || next.operationId === awaitingSource.previousOperationId) return;
        // A definitive rejection followed by a fresh observation may expose
        // another owned operation. Observe/stop it without attributing it to
        // the rejected request or transferring any consent to it.
        if (definitiveObservation) {
          if (next.role === null || next.phase === "idle") throw new Error("Invalid owned operation observation");
        } else if (next.role !== "host") throw new Error("Wrong source preparation owner");
        else if (awaitingSource.kind === "group") {
          if ((next.pendingGroupSource?.mode ?? next.hostGroup?.mode ?? awaitingSource.mode) !== awaitingSource.mode
            || (!next.pendingGroupSource && !next.hostGroup && !["closing", "closed", "failed"].includes(next.phase))) throw new Error("Wrong group preparation observation");
        } else if (next.hostGroup || next.pendingGroupSource || (next.invitationProfile ?? "legacy") !== awaitingSource.profile
          || (next.pendingHostSource && (next.pendingHostSource.mode !== awaitingSource.mode || next.pendingHostSource.profile !== awaitingSource.profile))
          || !["hostPreparing", "hostPrepared", "closing", "closed", "failed"].includes(next.phase)) throw new Error("Wrong host preparation observation");
      }
      sourcePreparationAwaiting.current = undefined;
      setSourcePreparationUncertain(false);
      setAcknowledged(false); setExportedInvitation(""); exportedFor.current = undefined;
    }
    const recovering = udpRetryContext.current;
    if (recovering && (next.operationId !== recovering.operationId || next.generation >= recovering.generation)) {
      udpRetryContext.current = undefined;
      setUdpRecoveryAwaitingSnapshot(false);
    }
    snapshotRef.current = next;
    setSnapshot(next);
    setKnownOperationId(next.operationId ?? undefined);
    if (observedReview.current !== next.review?.digest) {
      observedReview.current = next.review?.digest;
      setAcknowledged(false);
    }
    if (exportedFor.current !== next.operationId || finalPhase(next) || next.transportState === "admitted"
      || (next.sessionTiming && Date.now() >= next.sessionTiming.admissionExpiresAtMs)) {
      setExportedInvitation("");
      exportedFor.current = undefined;
    }
    if (finalPhase(next)) { setGuestPublicId(""); setInvitation(""); }
    setWarning("");
  }

  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    let timer: number | undefined;
    async function poll() {
      if (disposed) return;
      if (!pendingRef.current) {
        const request = epoch.current;
        const rejectionProof = sourcePreparationAwaiting.current?.controlledRejectionSettled;
        try {
          const value = await invoke<unknown>("connect_snapshot");
          if (!disposed && request === epoch.current) acceptSnapshot(value, rejectionProof);
        } catch {
          if (!disposed && request === epoch.current) setWarning("O estado não pôde ser atualizado. A última observação pode estar desatualizada.");
        }
      }
      if (!disposed) timer = window.setTimeout(() => { void poll(); }, 1_000);
    }
    void (async () => {
      if (!isTauri()) {
        setError("O Aurora Connect experimental requer o aplicativo desktop habilitado.");
        setLoading(false);
        return;
      }
      try {
        const value = capabilitiesFrom(await invoke<unknown>("connect_capabilities"));
        if (disposed) return;
        setCapabilities(value);
        if (value.available) await poll();
      } catch (cause) {
        if (!disposed) {
          setError(connectError(cause));
          // Capabilities gate new actions, not observation or cleanup of an
          // already owned operation recovered after navigation/remount.
          await poll();
        }
      }
      finally { if (!disposed) setLoading(false); }
    })();
    return () => { disposed = true; mounted.current = false; epoch.current += 1; if (timer !== undefined) window.clearTimeout(timer); };
  }, []);

  async function perform(action: Action, operation: (current: () => boolean) => Promise<void>) {
    if (pendingRef.current && action !== "stop" && !(action === "refresh" && sourcePreparationAwaiting.current)) return;
    if (pendingRef.current === "stop") return;
    const request = ++epoch.current;
    const current = () => mounted.current && request === epoch.current;
    pendingRef.current = action;
    setPending(action); setError(""); setMessage("");
    try { await operation(current); }
    catch (cause) { if (current()) { setError(connectError(cause)); setAcknowledged(false); } }
    finally { if (current()) { pendingRef.current = undefined; setPending(undefined); } }
  }
  async function refresh(current: () => boolean) {
    const rejectionProof = sourcePreparationAwaiting.current?.controlledRejectionSettled;
    try {
      const value = await invoke<unknown>("connect_snapshot");
      if (current()) acceptSnapshot(value, rejectionProof);
    } catch (cause) {
      if (current()) setWarning("O estado não pôde ser atualizado. A última observação pode estar desatualizada.");
      throw cause;
    }
  }
  async function copy(value: string, current = () => mounted.current) {
    try {
      if (!navigator.clipboard?.writeText) throw new Error("Clipboard unavailable");
      await navigator.clipboard.writeText(value);
      if (current()) setMessage("Copiado. Compartilhe somente com o participante autorizado.");
    } catch {
      if (current()) setMessage("A cópia automática não está disponível. Selecione o texto exibido e copie manualmente.");
    }
  }
  function prepare(event: FormEvent) {
    event.preventDefault();
    if (active || !capabilities?.available) return;
    if (!capabilities.modes.includes(mode) || (selectedRole === "host" && mode === "directIpv4" && !isDirectIpv4Address(bindAddress.trim()))) {
      setError(connectError("connect_invalid_request"));
      return;
    }
    if (invitationProfile === "sessionGroup") {
      const discovery = capabilities.sessionFragmentedHostGroups?.ownedGameTcpDiscovery === true && targetMode === "ownedInstance";
      if (selectedRole !== "host" || !capabilities.sessionFragmentedHostGroups || (!discovery && !isLoopbackUdpTarget(gameTarget.trim())) || !isLoopbackUdpTarget(udpTarget.trim())) {
        setError(connectError("connect_invalid_request")); return;
      }
      if (discovery) {
        if (!discoveredTarget) return;
        if (Date.now() >= discoveredTarget.expiresAtMs) {
          setDiscoveredTarget(undefined); setDiscoveryEpoch(value => value + 1);
          setError(connectError("discovery_stale_review")); return;
        }
        const selectionId = discoveredTarget.selectionId;
        void perform("prepareHost", async current => {
          sourcePreparationAwaiting.current = { kind: "group", generation: snapshotRef.current?.generation ?? 0, previousOperationId: snapshotRef.current?.operationId ?? null, mode };
          setSourcePreparationUncertain(true);
          setDiscoveredTarget(undefined); setDiscoveryEpoch(value => value + 1);
          try {
            const value = snapshotFrom(await invoke<unknown>("connect_prepare_session_group_host_discovered", {
              selectionId, request: { mode, bindAddress: bindAddress.trim(), udpTarget: udpTarget.trim() },
            }));
            if (!current()) return;
            if (value.phase !== "hostPreparing" || value.pendingGroupSource?.mode !== mode) throw new Error("Wrong initial group preparation");
            acceptSnapshot(value);
          } catch (cause) {
            // These controlled failures happen before the native reservation.
            // An absent/malformed IPC result remains uncertain until a newer
            // operation snapshot is observed; it never triggers a resend.
            if (current() && typeof cause === "string" && ["connect_disabled", "connect_owner_mismatch", "connect_invalid_request", "connect_busy", "connect_cancelled", "connect_internal"].includes(cause)) {
              sourcePreparationAwaiting.current = undefined; setSourcePreparationUncertain(false);
            }
            throw cause;
          }
        });
        return;
      }
      void perform("prepareHost", async current => {
        const value = groupPreparationFrom(await invoke<unknown>("connect_prepare_session_group_host", {
          request: { mode, bindAddress: bindAddress.trim(), gameTarget: gameTarget.trim(), udpTarget: udpTarget.trim() },
        }));
        if (!current()) return;
        setKnownOperationId(value.operationId); setSnapshot(undefined); setDiscoveredTarget(undefined);
        await refresh(current);
      });
      return;
    }
    const sessionUdp = invitationProfile === SESSION_FRAGMENTED_PROFILE && Boolean(capabilities.sessionFragmentedUdpInvitations);
    const fragmented = sessionUdp || invitationProfile === FRAGMENTED_PROFILE && Boolean(capabilities.fragmentedUdpInvitations);
    const discoveryAvailable = sessionUdp ? capabilities.sessionFragmentedUdpInvitations?.ownedGameTcpDiscovery === true
      : !fragmented && capabilities.ownedInstanceDiscovery === true;
    const discovery = discoveryAvailable && selectedRole === "host" && targetMode === "ownedInstance";
    if (discovery && !discoveredTarget) return;
    if (discovery && Date.now() >= discoveredTarget!.expiresAtMs) {
      setDiscoveredTarget(undefined); setDiscoveryEpoch(value => value + 1);
      setError(connectError("discovery_stale_review")); return;
    }
    const session = sessionUdp || invitationProfile === "sessionGameTcp" && Boolean(capabilities.sessionInvitations);
    const admission = Number(admissionSeconds), duration = Number(sessionSeconds);
    if (fragmented && selectedRole === "host" && ((!sessionUdp && !isFragmentedLifetime(Number(fragmentedLifetime)))
      || !isLoopbackUdpTarget(udpTarget.trim()) || (!discovery && !isLoopbackUdpTarget(gameTarget.trim())))) {
      setError(connectError("connect_invalid_request"));
      return;
    }
    if (session && selectedRole === "host" && (!Number.isInteger(admission) || admission < 1 || admission > 600
      || !Number.isInteger(duration) || duration < admission || duration > 3600)) {
      setError(connectError("connect_invalid_request"));
      return;
    }
    const command = selectedRole === "host"
      ? sessionUdp ? discovery ? "connect_prepare_session_fragmented_host_discovered" : "connect_prepare_session_fragmented_host" : fragmented ? "connect_prepare_fragmented_host" : session ? discovery ? "connect_prepare_session_host_discovered" : "connect_prepare_session_host"
        : discovery ? "connect_prepare_discovered_host" : "connect_prepare_host"
      : sessionUdp ? "connect_prepare_session_fragmented_guest" : fragmented ? "connect_prepare_fragmented_guest" : session ? "connect_prepare_session_guest" : "connect_prepare_guest";
    const request = selectedRole === "host"
      ? { mode, bindAddress: bindAddress.trim(),
        ...(sessionUdp && discovery ? {} : { gameTarget: discovery ? discoveredTarget!.target : gameTarget.trim() }),
        ...(fragmented ? { udpTarget: udpTarget.trim() } : {}),
        ...(session ? { admissionSeconds: admission, sessionSeconds: duration }
          : { lifetimeSeconds: Number(fragmented ? fragmentedLifetime : lifetime) }) }
      : { mode, expectedHostPublicId: hostPublicId.trim() };
    const selectionId = discovery ? discoveredTarget!.selectionId : undefined;
    void perform(selectedRole === "host" ? "prepareHost" : "prepareGuest", async current => {
      const awaiting: SourcePreparationAwaiting | undefined = discovery ? {
        kind: "single", generation: snapshotRef.current?.generation ?? 0, previousOperationId: snapshotRef.current?.operationId ?? null,
        mode, profile: sessionUdp ? SESSION_FRAGMENTED_PROFILE : session ? "sessionGameTcp" : "legacy", baseline: snapshotRef.current,
      } : undefined;
      if (awaiting) {
        sourcePreparationAwaiting.current = awaiting; setSourcePreparationUncertain(true);
        setDiscoveredTarget(undefined); setDiscoveryEpoch(value => value + 1);
      }
      let value;
      try {
        value = preparationFrom(await invoke<unknown>(command, { request, ...(discovery ? { selectionId } : {}) }));
      } catch (cause) {
        if (awaiting && mounted.current && sourcePreparationAwaiting.current === awaiting && isControlledSourceRejection(cause)) {
          awaiting.controlledRejectionSettled = Symbol("definitive native preparation rejection");
          // This invoke begins strictly after the definitive reply. A poll
          // already in flight holds its earlier token and cannot prove it.
          if (current()) { try { await refresh(current); } catch { /* Keep the same definitive context for explicit observation. */ } }
        }
        throw cause;
      }
      if (!current()) return;
      setDiscoveredTarget(undefined);
      setKnownOperationId(value.operationId);
      // The preparation is real, but its full state has not been observed yet.
      // Do not let a previous closed snapshot hide the new cancellation handle.
      setSnapshot(undefined);
      if (value.invitationProfile !== (sessionUdp ? SESSION_FRAGMENTED_PROFILE : fragmented ? FRAGMENTED_PROFILE : session ? "sessionGameTcp" : undefined)) throw new Error("Wrong preparation profile");
      await refresh(current);
    });
  }
  function review(event: FormEvent) {
    event.preventDefault();
    if (!operationId || !snapshot?.role) return;
    setAcknowledged(false);
    void perform(snapshot.role === "host" ? "reviewHost" : "reviewGuest", async current => {
      const value = reviewFrom(await invoke<unknown>(snapshot.role === "host" ? "connect_review_host" : "connect_review_guest",
        snapshot.role === "host" ? { operationId, guestPublicId: guestPublicId.trim() } : { operationId, encodedInvitation: invitation }));
      if (!current()) return;
      if (value.operationId !== operationId) throw new Error("Wrong operation projection");
      if ((value.fragmentedUdp?.profile ?? value.sessionTiming?.profile) !== snapshot.invitationProfile) throw new Error("Wrong review profile");
      await refresh(current);
    });
  }
  function confirm() {
    const reviewed = snapshot?.review;
    if (!reviewed || !acknowledged || reviewed.operationId !== operationId) return;
    setAcknowledged(false);
    void perform("confirm", async current => {
      const value = await invoke<unknown>("connect_confirm", { operationId: reviewed.operationId, digest: reviewed.digest });
      if (current()) acceptSnapshot(value);
    });
  }
  function stop() {
    if (!operationId) return;
    setAcknowledged(false); setExportedInvitation("");
    void perform("stop", async current => {
      const value = await invoke<unknown>("connect_stop", { operationId });
      if (current()) acceptSnapshot(value);
    });
  }
  function udpRetryAllowed(value: ConnectSnapshot | undefined): value is ConnectSnapshot & { operationId: string } {
    return !!(capabilities?.sessionFragmentedUdpInvitations?.udpRecovery && value?.operationId
      && value.invitationProfile === SESSION_FRAGMENTED_PROFILE && value.role === "guest" && value.phase === "guestConnected"
      && value.transportState === "authenticated" && value.udpState === "failed" && value.udpRecovery
      && !value.udpRecovery.pending && value.udpRecovery.attemptsUsed < value.udpRecovery.maxAttempts
      && value.sessionTiming && Date.now() < value.sessionTiming.sessionExpiresAtMs);
  }
  function retryUdp() {
    const baseline = snapshotRef.current;
    if (pendingRef.current || udpRetryContext.current || !udpRetryAllowed(baseline) || baseline.operationId !== operationId) return;
    const { operationId: retryOperation, generation: expectedGeneration } = baseline;
    udpRetryContext.current = { operationId: retryOperation, generation: expectedGeneration };
    setUdpRecoveryAwaitingSnapshot(true);
    // A lost IPC reply remains uncertain until a validated observation arrives.
    // Neither this callback nor polling submits another recovery command.
    void perform("retryUdp", async current => {
      const value = snapshotFrom(await invoke<unknown>("connect_retry_session_udp", { operationId: retryOperation, expectedGeneration }));
      if (!current()) return;
      if (value.operationId !== retryOperation || value.invitationProfile !== SESSION_FRAGMENTED_PROFILE || value.role !== "guest"
        || value.generation < expectedGeneration || !value.udpRecovery || !baseline.sessionTiming || !value.sessionTiming
        || !sameSessionTiming(baseline.sessionTiming, value.sessionTiming)
        || (value.phase === "guestConnected" && value.tcpAddress !== baseline.tcpAddress)) throw new Error("Wrong UDP recovery projection");
      acceptSnapshot(value);
    });
  }
  function exportInvitation() {
    if (!operationId) return;
    void perform("export", async current => {
      const value = exportFrom(await invoke<unknown>("connect_export_invitation", { operationId }));
      if (!current()) return;
      if ((value.fragmentedUdp?.profile ?? value.sessionTiming?.profile) !== snapshot?.invitationProfile) throw new Error("Wrong export profile");
      if (snapshot?.sessionTiming && (!value.sessionTiming || !sameSessionTiming(snapshot.sessionTiming, value.sessionTiming))) throw new Error("Wrong export timing");
      exportedFor.current = operationId;
      setExportedInvitation(value.invitation);
      await copy(value.invitation, current);
    });
  }
  const operationId = knownOperationId ?? snapshot?.operationId;
  const activeOperation = Boolean(operationId && (!snapshot || !finalPhase(snapshot) || snapshot.cleanupState === "pending" || snapshot.cleanupState === "failed"));
  const active = sourcePreparationUncertain || activeOperation;
  const busy = Boolean(pending);
  const prepared = snapshot?.preparation;
  const reviewed = snapshot?.review;
  const canReview = !snapshot?.hostGroup && (snapshot?.phase === "hostPrepared" || snapshot?.phase === "guestPrepared");
  const canProbe = snapshot?.phase === "guestConnected";
  const canExport = !snapshot?.hostGroup && snapshot?.role === "host" && snapshot.phase === "hostListening" && snapshot.transportState === "listening"
    && (!snapshot.sessionTiming || Date.now() < snapshot.sessionTiming.admissionExpiresAtMs);
  const sessionUdpSelected = invitationProfile === SESSION_FRAGMENTED_PROFILE && Boolean(capabilities?.sessionFragmentedUdpInvitations);
  const sessionSelected = sessionUdpSelected || invitationProfile === "sessionGameTcp" && Boolean(capabilities?.sessionInvitations);
  const fragmentedSelected = sessionUdpSelected || invitationProfile === FRAGMENTED_PROFILE && Boolean(capabilities?.fragmentedUdpInvitations);
  const groupSelected = invitationProfile === "sessionGroup" && selectedRole === "host" && Boolean(capabilities?.sessionFragmentedHostGroups);
  const discoveryAvailable = groupSelected ? capabilities?.sessionFragmentedHostGroups?.ownedGameTcpDiscovery === true : sessionUdpSelected ? capabilities?.sessionFragmentedUdpInvitations?.ownedGameTcpDiscovery === true
    : !fragmentedSelected && capabilities?.ownedInstanceDiscovery === true;
  const displayedProfile = active ? snapshot?.hostGroup || snapshot?.pendingGroupSource || sourcePreparationAwaiting.current?.kind === "group" ? "sessionGroup"
    : snapshot?.invitationProfile ?? sourcePreparationAwaiting.current?.profile : groupSelected || sessionSelected || fragmentedSelected ? invitationProfile : undefined;
  const sessionDisplayed = displayedProfile === "sessionGameTcp" || displayedProfile === SESSION_FRAGMENTED_PROFILE;
  const fragmentedDisplayed = displayedProfile === FRAGMENTED_PROFILE || displayedProfile === SESSION_FRAGMENTED_PROFILE;

  return <section className="connect-view" aria-labelledby="connect-title">
    <header className="connect-heading"><div><h2 id="connect-title">Aurora Connect</h2><p>Compartilhe um servidor autorizado ou use um convite recebido.</p></div><span className="connect-badge">Experimental</span></header>
    {loading && <p role="status">Verificando disponibilidade…</p>}
    {error && <p className="connect-error" role="alert">{error}</p>}
    {warning && <p className="connect-warning" role="status">{warning}</p>}
    {message && <p className="connect-message" role="status">{message}</p>}
    {!loading && capabilities && !capabilities.available && <p className="connect-warning">Este recurso está desligado nesta versão do launcher.</p>}
    {(capabilities?.available || snapshot) && <>
      <p>{displayedProfile === "sessionGroup"
        ? "Um grupo fixo de dois a oito participantes, com convites e prazos individuais. O servidor deve permanecer aberto; o destino UDP é informado e autorizado separadamente."
        : fragmentedDisplayed && sessionDisplayed
        ? "Transporte TCP e UDP experimental. O prazo para entrar e o encerramento da sessão são separados, sem renovação automática. O servidor deve permanecer aberto."
        : fragmentedDisplayed
        ? "Transporte TCP e UDP experimental. Convite e conexão expiram juntos em até 120 segundos, sem renovação automática. O servidor deve permanecer aberto."
        : sessionDisplayed
        ? "Uma tentativa por janela. O prazo para entrar e o encerramento da sessão são separados, sem renovação automática. O servidor deve permanecer aberto."
        : "Uma tentativa por janela. A conexão expira em até 10 minutos e não é renovada automaticamente. O servidor deve permanecer aberto."}</p>
      <section className="connect-status" aria-label="Estado observado da conexão">
        <header><h3>{snapshot ? snapshot.pendingHostSource ? "Conferindo a origem do jogo…" : snapshot.hostGroup && snapshot.phase === "hostListening" ? "Anfitrião do grupo disponível" : snapshot.phase === "hostListening" && snapshot.transportState === "admitted" ? "Convite utilizado · conexão ativa" : phaseText[snapshot.phase] : "Estado ainda não disponível"}</h3><div className="connect-actions">
          <button className="secondary" type="button" disabled={busy && !sourcePreparationUncertain} onClick={() => { void perform("refresh", refresh); }}>Atualizar estado</button>
          {activeOperation && <button className="danger" type="button" disabled={pending === "stop" || snapshot?.phase === "closing"} onClick={stop}>Encerrar Connect</button>}
        </div></header>
        {pending && <p role="status">{actionText[pending]}</p>}
        {sourcePreparationUncertain && <p role="status">O resultado do preparo ainda não foi confirmado. Atualize o estado; a seleção não será enviada novamente.</p>}
        {snapshot?.pendingGroupSource && <p role="status">Conferindo o jogo selecionado. O código público e os convites só ficarão disponíveis depois dessa etapa; você pode encerrar Connect enquanto aguarda.</p>}
        {snapshot?.pendingHostSource && <p role="status">Conferindo o jogo selecionado. Você pode encerrar Connect enquanto aguarda. O código público só ficará disponível após a conferência, dentro do prazo original de 60 segundos.</p>}
        {snapshot && <>
          <dl><dt>Transporte</dt><dd>{transportText[snapshot.transportState]}</dd><dt>Servidor Minecraft</dt><dd>{statusText[snapshot.statusState]}</dd>
            <dt>Caminho da conexão</dt><dd>Não confirmado · relay desligado</dd><dt>Entrada no mundo</dt><dd>Não verificada</dd>
            <dt>Simple Voice Chat</dt><dd>Desativado nesta integração</dd><dt>Plasmo Voice</dt><dd>Desativado nesta integração</dd>
            {(snapshot.invitationProfile === FRAGMENTED_PROFILE || snapshot.invitationProfile === SESSION_FRAGMENTED_PROFILE) && <FragmentedStatusFields udp={snapshot.role === "guest" ? fragmentedGuestUdpFrom(snapshot, snapshot.invitationProfile) : undefined} />}
            {snapshot.sessionTiming
              ? <><dt>Entrar até</dt><dd>{connectDate(snapshot.sessionTiming.admissionExpiresAtMs)} · horário local</dd><dt>Encerramento da sessão</dt><dd>{connectDate(snapshot.sessionTiming.sessionExpiresAtMs)} · horário local</dd></>
              : snapshot.expiresAtMs !== null && <><dt>Limite desta etapa</dt><dd>{connectDate(snapshot.expiresAtMs)} · horário local</dd></>}
            {snapshot.cleanupState !== "notStarted" && <><dt>Encerramento</dt><dd>{{ pending: "Aguardando liberação dos recursos", complete: "Recursos liberados", failed: "Liberação não confirmada" }[snapshot.cleanupState]}</dd></>}
          </dl>
          {(snapshot.hostGroup || snapshot.role === "host") && snapshot.cleanupState === "failed" && <p className="connect-warning" role="status">O encerramento não foi confirmado. O transporte exibido é a última observação; não confirma recursos liberados nem participantes ativos.</p>}
          {snapshot.errorCode && <p className="connect-error" role="alert">{connectError(snapshot.errorCode)}</p>}
          {snapshot.udpRecovery && <div className="connect-hint">
            <p>Novas tentativas UDP utilizadas: {snapshot.udpRecovery.attemptsUsed} de {snapshot.udpRecovery.maxAttempts}.</p>
            <p>A nova tentativa mantém a porta, o participante, os destinos e o encerramento da sessão já aprovados. O jogo continua independente; isto não confirma áudio.</p>
            {snapshot.udpRecovery.pending && <p role="status">Preparando novamente a ponte UDP…</p>}
            {udpRecoveryAwaitingSnapshot && !pending && <p role="status">Aguardando confirmar o estado da tentativa UDP. Nenhuma solicitação será repetida automaticamente.</p>}
            {udpRetryAllowed(snapshot) && <button type="button" disabled={busy || udpRecoveryAwaitingSnapshot || snapshot.udpRecovery.pending} onClick={retryUdp}>Tentar UDP novamente</button>}
            {snapshot.udpRecovery.attemptsUsed >= snapshot.udpRecovery.maxAttempts && snapshot.udpState === "failed"
              && <p>O limite de novas tentativas UDP desta sessão foi atingido.</p>}
          </div>}
          {snapshot.statusSummary && <div className="connect-status-summary"><p>Última resposta: protocolo {snapshot.statusSummary.reportedProtocol}, {snapshot.statusSummary.protocolMatchesRequested ? "correspondente ao solicitado" : "diferente do solicitado"}.</p>
            <p>Jogadores informados pelo servidor: {snapshot.statusSummary.reportedPlayersOnline ?? "não informado"} / {snapshot.statusSummary.reportedPlayersMax ?? "não informado"}. Essa contagem não confirma quem entrou no mundo.</p></div>}
          {capabilities?.available && snapshot.tcpAddress && snapshot.phase === "guestConnected" && <div className="connect-output"><label htmlFor="connect-local-address">Endereço para Conexão direta no Minecraft</label><input id="connect-local-address" readOnly value={snapshot.tcpAddress} />
            <div><button className="secondary" type="button" onClick={() => { void copy(snapshot.tcpAddress!); }}>Copiar endereço do jogo</button></div><small>Abra o Minecraft compatível e use Multiplayer → Conexão direta. Copiar o endereço não inicia nem fecha o jogo.</small></div>}
          {capabilities?.available && canProbe && <div><button type="button" disabled={busy || snapshot.statusState === "checking"} onClick={() => { void perform("probe", async current => { const value = await invoke<unknown>("connect_probe_status", { operationId }); if (current()) acceptSnapshot(value); }); }}>Consultar servidor</button><p className="connect-hint">Consulta de Minecraft 1.21.1 (protocolo {capabilities.statusProtocol}), com endereço lógico {capabilities.statusHandshakeHost}:{capabilities.statusHandshakePort}. Não comprova compatibilidade com outras versões.</p></div>}
        </>}
      </section>
      <ConnectGameLaunchPanel capability={capabilities?.readyGameLaunch} session={snapshot} launchContext={launchContext}
        observeExisting={!capabilities && !loading} sessionStale={Boolean(warning) || loading || pending === "stop"} />
      {capabilities?.available && !active && snapshot && <section className="connect-card">
        <div className="connect-tabs" aria-label="Papel nesta tentativa"><button type="button" aria-pressed={selectedRole === "host"} disabled={busy} onClick={() => { setSelectedRole("host"); setDiscoveredTarget(undefined); }}>Anfitrião</button><button type="button" aria-pressed={selectedRole === "guest"} disabled={busy} onClick={() => { setSelectedRole("guest"); if (invitationProfile === "sessionGroup") setInvitationProfile("legacy"); setDiscoveredTarget(undefined); }}>Convidado</button></div>
        <form className="connect-form" onSubmit={prepare}>
          <h3>{selectedRole === "host" ? "Compartilhar servidor local" : "Preparar recebimento do convite"}</h3>
          {(capabilities.sessionInvitations || capabilities.fragmentedUdpInvitations || capabilities.sessionFragmentedUdpInvitations) && <label>Tipo de convite<select value={invitationProfile} disabled={busy} onChange={event => {
            const next = event.target.value;
            setInvitationProfile(next === "sessionGroup" && selectedRole === "host" && capabilities.sessionFragmentedHostGroups ? "sessionGroup" : next === SESSION_FRAGMENTED_PROFILE && capabilities.sessionFragmentedUdpInvitations ? SESSION_FRAGMENTED_PROFILE
              : next === FRAGMENTED_PROFILE && capabilities.fragmentedUdpInvitations ? FRAGMENTED_PROFILE
              : next === "sessionGameTcp" && capabilities.sessionInvitations ? "sessionGameTcp" : "legacy");
            setDiscoveredTarget(undefined);
            if (next === FRAGMENTED_PROFILE || next === SESSION_FRAGMENTED_PROFILE || next === "sessionGroup") setTargetMode("manual");
          }}><option value="legacy">Convite curto · até 10 minutos</option>{capabilities.sessionInvitations && <option value="sessionGameTcp">Sessão de jogo · até 1 hora</option>}{capabilities.fragmentedUdpInvitations && <option value={FRAGMENTED_PROFILE}>Transporte TCP e UDP · até 2 minutos</option>}{capabilities.sessionFragmentedUdpInvitations && <option value={SESSION_FRAGMENTED_PROFILE}>Sessão TCP e UDP · até 1 hora</option>}{selectedRole === "host" && capabilities.sessionFragmentedHostGroups && <option value="sessionGroup">Grupo TCP e UDP · de 2 a 8 participantes</option>}</select></label>}
          <label>Modo de conexão<select value={mode} disabled={busy} onChange={event => { const next = event.target.value as ConnectMode; if (!capabilities.modes.includes(next)) return; setMode(next); setBindAddress(next === "loopback" ? "127.0.0.1:42000" : ""); }}><option value="loopback">Ensaio neste computador</option><option value="externalIpv6">IPv6 externo autorizado</option>{capabilities.modes.includes("directIpv4") && <option value="directIpv4">IPv4 direto autorizado</option>}</select></label>
          {mode === "externalIpv6" && <p>Use o endereço inicial IPv6 autorizado. Este modo compartilha informações de rede com o participante; não configura o roteador nem garante alcance.</p>}
          {mode === "directIpv4" && <p>{selectedRole === "host" ? "Informe um IPv4 deste computador e a porta autorizada, alcançáveis pelo convidado." : "O convite deve usar o IPv4 e a porta autorizados do anfitrião, alcançáveis por este computador."} Este modo compartilha esse endereço, não configura NAT ou roteador e não habilita relay; redes diferentes podem não alcançar esse endereço.</p>}
          {selectedRole === "host" ? <div className="connect-fields">
            <label>Endereço inicial da conexão<input required maxLength={128} value={bindAddress} disabled={busy} placeholder={mode === "loopback" ? "127.0.0.1:42000" : mode === "directIpv4" ? "IPv4 local autorizado:porta" : "[IPv6 autorizado]:42000"} onChange={event => setBindAddress(event.target.value)} autoComplete="off" spellCheck={false} /></label>
            {discoveryAvailable && <label>Selecionar servidor<select disabled={busy} value={targetMode} onChange={event => { setTargetMode(event.target.value as "manual" | "ownedInstance"); setDiscoveredTarget(undefined); }}><option value="manual">Endereço manual · servidor dedicado ou local</option><option value="ownedInstance">Jogo aberto pelo Aurora · mundo LAN</option></select></label>}
            {discoveryAvailable && targetMode === "ownedInstance"
              ? <><DiscoveryPicker key={`${invitationProfile}:${discoveryEpoch}`} disabled={busy} onSelect={setDiscoveredTarget} />
                {(sessionUdpSelected || groupSelected) && <p>Esta seleção confere somente a porta TCP do jogo. O destino UDP abaixo continua manual e exige sua autorização separada; não foi conferido como parte desse processo.</p>}</>
              : <label>Servidor neste computador<input required maxLength={128} value={gameTarget} disabled={busy} onChange={event => setGameTarget(event.target.value)} autoComplete="off" spellCheck={false} /><small>Somente endereço loopback e porta do jogo que você autoriza compartilhar. A seleção manual não comprova qual processo possui essa porta.</small></label>}
            {(fragmentedSelected || groupSelected) && <FragmentedFields role="host" disabled={busy} udpTarget={udpTarget} lifetimeSeconds={fragmentedLifetime} separateSession={sessionSelected || groupSelected}
              onUdpTargetChange={setUdpTarget} onLifetimeChange={setFragmentedLifetime} />}
            {sessionSelected ? <>
              <label>Prazo para entrar, em segundos<input required type="number" min={1} max={600} step={1} value={admissionSeconds} disabled={busy} onChange={event => setAdmissionSeconds(event.target.value)} /></label>
              <label>Duração máxima da sessão, em segundos<input required type="number" min={1} max={3600} step={1} value={sessionSeconds} disabled={busy} onChange={event => setSessionSeconds(event.target.value)} /><small>Os dois prazos começam na emissão do convite. A duração da sessão deve ser igual ou maior que o prazo para entrar.</small></label>
            </> : !fragmentedSelected && !groupSelected && <label>Prazo da sessão, em segundos<input required type="number" min={1} max={capabilities.maxLifetimeSeconds} step={1} value={lifetime} disabled={busy} onChange={event => setLifetime(event.target.value)} /></label>}
            {groupSelected && <p>Após preparar, receba os códigos públicos de cada convidado e defina seus prazos na revisão do grupo. A lista será fixa depois da autorização.</p>}
          </div> : <label>Código público do anfitrião<input required maxLength={64} value={hostPublicId} disabled={busy} onChange={event => setHostPublicId(event.target.value)} autoComplete="off" spellCheck={false} /><small>Receba este código diretamente do anfitrião, por um canal autorizado.</small></label>}
          {fragmentedSelected && selectedRole === "guest" && <FragmentedFields role="guest" disabled={busy} udpTarget={udpTarget} lifetimeSeconds={fragmentedLifetime}
            onUdpTargetChange={setUdpTarget} onLifetimeChange={setFragmentedLifetime} />}
          <div><button type="submit" disabled={busy || (discoveryAvailable && selectedRole === "host" && targetMode === "ownedInstance" && !discoveredTarget)}>{selectedRole === "host" ? "Preparar anfitrião" : "Preparar convidado"}</button></div>
        </form>
      </section>}
      {snapshot?.hostGroup && <GroupHostPanel key={snapshot.operationId} snapshot={snapshot} supported={Boolean(capabilities?.available && capabilities.sessionFragmentedHostGroups && capabilities.modes.includes(snapshot.hostGroup.mode)
        && (snapshot.hostGroup.review?.targetSource !== "ownedInstance" || capabilities.sessionFragmentedHostGroups.ownedGameTcpDiscovery))} busy={busy} stale={Boolean(warning)}
        perform={perform} refresh={refresh} acceptSnapshot={acceptSnapshot} copy={copy} />}
      {capabilities?.available && prepared && <section className="connect-card"><h3>Troque os códigos públicos</h3><p>{snapshot?.role === "host" ? "Envie seu código ao convidado e receba o código gerado por ele nesta tentativa." : "Envie seu código ao anfitrião e aguarde o convite destinado a esta tentativa."}</p>
        <div className="connect-output"><label htmlFor="connect-public-code">Seu código público</label><textarea id="connect-public-code" readOnly value={prepared.publicId} /><div><button className="secondary" type="button" onClick={() => { void copy(prepared.publicId); }}>Copiar meu código</button></div></div>
        <small>Preparação válida até {connectDate(prepared.expiresAtMs)}. Nenhuma chave privada é exibida.</small>
      </section>}
      {capabilities?.available && canReview && <form className="connect-card connect-form" onSubmit={review}>
        {snapshot.role === "host" ? <label>Código público recebido do convidado<input required maxLength={64} value={guestPublicId} disabled={busy} onChange={event => { setGuestPublicId(event.target.value); setAcknowledged(false); }} autoComplete="off" spellCheck={false} /></label>
          : <label>Convite recebido<textarea required maxLength={4096} value={invitation} disabled={busy} onChange={event => { setInvitation(event.target.value); setAcknowledged(false); }} autoComplete="off" spellCheck={false} /></label>}
        <div><button type="submit" disabled={busy}>Revisar convite</button></div>
      </form>}
      {capabilities?.available && reviewed && snapshot?.phase === "reviewed" && <section className="connect-card connect-review" aria-labelledby="connect-review-title"><h3 id="connect-review-title">Revise antes de autorizar</h3>
        <dl><dt>Papel</dt><dd>{reviewed.role === "host" ? "Anfitrião" : "Convidado"}</dd><dt>Modo</dt><dd>{reviewed.mode === "loopback" ? "Neste computador" : reviewed.mode === "directIpv4" ? "IPv4 direto autorizado" : "IPv6 externo autorizado"}</dd>
          <dt>Endereço inicial</dt><dd><code>{reviewed.directAddress}</code></dd>{reviewed.gameTarget && <><dt>Destino autorizado</dt><dd><code>{reviewed.gameTarget}</code></dd></>}
          {reviewed.targetSource === "ownedInstance" && <><dt>Origem TCP conferida</dt><dd>Instância {reviewed.instanceId} iniciada por este launcher. A conexão encerra se o processo ou a porta TCP deixar de corresponder à seleção.</dd>
            {reviewed.fragmentedUdp && <><dt>Origem UDP</dt><dd>Destino manual autorizado separadamente; não foi conferido como parte do processo do jogo.</dd></>}</>}
          <dt>Serviço</dt><dd>Minecraft · TCP · somente o destino aprovado pelo anfitrião</dd>
          {reviewed.fragmentedUdp && <FragmentedReviewFields review={reviewed.fragmentedUdp} />}
          {reviewed.sessionTiming ? <><dt>Prazo para entrar</dt><dd>{reviewed.sessionTiming.admissionSeconds} segundos desde a emissão</dd><dt>Duração máxima da sessão</dt><dd>{reviewed.sessionTiming.sessionSeconds} segundos desde a emissão, sem renovação</dd><dt>Autorize até</dt><dd>{connectDate(reviewed.expiresAtMs)}</dd><dt>Entrar até</dt><dd>{connectDate(reviewed.sessionTiming.admissionExpiresAtMs)}</dd><dt>Encerramento da sessão</dt><dd>{connectDate(reviewed.sessionTiming.sessionExpiresAtMs)}</dd></>
            : <><dt>Prazo</dt><dd>{reviewed.lifetimeSeconds} segundos, sem renovação</dd><dt>Autorize até</dt><dd>{connectDate(reviewed.expiresAtMs)}</dd><dt>Convite e sessão expiram</dt><dd>{connectDate(reviewed.invitationExpiresAtMs)}</dd></>}
          <dt>Consulta do servidor</dt><dd>Minecraft 1.21.1 · protocolo {reviewed.statusProtocol} · endereço lógico {reviewed.statusHandshakeHost}:{reviewed.statusHandshakePort}</dd>
          <dt>Anfitrião</dt><dd><code>{reviewed.hostPublicId}</code></dd><dt>Convidado</dt><dd><code>{reviewed.guestPublicId}</code></dd></dl>
        <p>Uma única admissão. Encerrar ou perder esta conexão exige preparar um novo convite. Nenhum arquivo do jogo será alterado.</p>
        <details><summary>Identificadores da revisão</summary><p>Revisão: <code>{reviewed.digest}</code></p><p>Convite: <code>{reviewed.invitationDigest}</code></p></details>
        <label className="connect-consent"><input type="checkbox" checked={acknowledged} disabled={busy} onChange={event => setAcknowledged(event.target.checked)} /><span>{reviewed.fragmentedUdp
          ? `Conferi o participante, os destinos TCP e UDP, o orçamento de retorno, a porta local e ${reviewed.sessionTiming ? "os dois prazos" : "o prazo"}; autorizo somente este transporte experimental.`
          : `Conferi o participante, o destino e ${reviewed.sessionTiming ? "os dois prazos" : "o prazo"} e autorizo esta conexão experimental.`}</span></label>
        <div className="connect-actions"><button type="button" disabled={busy || !acknowledged} onClick={confirm}>Autorizar conexão</button><button className="secondary" type="button" disabled={pending === "stop"} onClick={stop}>Cancelar tentativa</button></div>
      </section>}
      {capabilities?.available && canExport && <section className="connect-card"><h3>Envie o convite ao participante</h3><p>Somente o convidado autorizado pode usar este convite. Envie também seu código público pelo canal combinado.</p><div><button type="button" disabled={busy} onClick={exportInvitation}>Copiar convite</button></div>
        {exportedInvitation && <label className="connect-output">Convite desta tentativa<textarea readOnly value={exportedInvitation} spellCheck={false} /></label>}
      </section>}
      <small>Navegar pelo launcher mantém a tentativa ativa. Use Encerrar Connect para liberar a ponte; o Minecraft continua sob seu controle. Porta revisada e resposta de servidor não comprovam entrada no mundo ou voz.</small>
    </>}
  </section>;
}
