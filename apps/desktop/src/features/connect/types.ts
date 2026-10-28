import {
  FRAGMENTED_PROFILE, SESSION_FRAGMENTED_PROFILE, sessionFragmentedCapabilityFrom, fragmentedCapabilityFrom, fragmentedExportFieldsFrom, fragmentedPreparationFieldsFrom,
  fragmentedReviewFieldsFrom, fragmentedSnapshotFieldsFrom, type FragmentedGuestUdp, type FragmentedUdpCapability,
  type FragmentedUdpContract, type FragmentedUdpReview, type SessionFragmentedUdpCapability, type SessionUdpRecovery, isDirectIpv4Address,
} from "./fragmented-types";
import { groupCapabilityFrom, groupSnapshotFieldsFrom, pendingGroupSourceFieldsFrom, type GroupCapability, type HostGroup, type PendingGroupSource } from "./group-types";
import { readyGameCapabilityFrom, type ReadyGameLaunchCapability } from "../connect-game-launch/types";
import { pendingHostSourceFieldsFrom, type PendingHostSource } from "./host-source-types";
import { snapshotPathObservationFrom, type PathObservation } from "./path-observation-types";
import { socialConnectProjectionFrom, type SocialConnectProjection } from "../social/connect-types";

export type ConnectMode = "loopback" | "externalIpv6" | "directIpv4";
export type ConnectRole = "host" | "guest";
export type ConnectSessionProfile = "sessionGameTcp" | typeof SESSION_FRAGMENTED_PROFILE;
export type ConnectInvitationProfile = ConnectSessionProfile | typeof FRAGMENTED_PROFILE;
export type ConnectSessionTiming = {
  admissionSeconds: number; sessionSeconds: number;
  admissionExpiresAtMs: number; sessionExpiresAtMs: number;
} & ({ profile: "sessionGameTcp"; schemaVersion: 2 } | { profile: typeof SESSION_FRAGMENTED_PROFILE; schemaVersion: 3 });
export type ConnectPhase = "idle" | "hostPreparing" | "hostPrepared" | "guestPrepared" | "reviewed" | "starting" | "hostListening" | "guestConnected" | "closing" | "closed" | "failed";
export type ConnectCapabilities = {
  available: boolean; experimental: true; maxLifetimeSeconds: number;
  modes: ConnectMode[]; services: string[]; voiceEnabled: false; relayEnabled: false;
  statusProtocol: 767; statusHandshakeHost: "localhost"; statusHandshakePort: 43001;
  ownedInstanceDiscovery?: boolean;
  sessionInvitations?: { profile: "sessionGameTcp"; schemaVersion: 2; maxAdmissionSeconds: 600; maxSessionSeconds: 3600 };
  fragmentedUdpInvitations?: FragmentedUdpCapability;
  sessionFragmentedUdpInvitations?: SessionFragmentedUdpCapability;
  sessionFragmentedHostGroups?: GroupCapability;
  readyGameLaunch?: ReadyGameLaunchCapability;
};
export type ConnectStatusSummary = {
  requestedProtocol: number; reportedProtocol: number; protocolMatchesRequested: boolean;
  reportedPlayersOnline: number | null; reportedPlayersMax: number | null;
  connectMicros: number; statusMicros: number; pingMicros: number; totalMicros: number;
};
export type ConnectSnapshot = {
  available: boolean; operationId: string | null; generation: number; role: ConnectRole | null;
  phase: ConnectPhase; transportState: "notStarted" | "listening" | "admitted" | "authenticated" | "closed";
  statusState: "notChecked" | "checking" | "responded" | "failed";
  tcpAddress: string | null; expiresAtMs: number | null; statusSummary: ConnectStatusSummary | null;
  errorCode: string | null; worldState: "notVerified"; simpleVoiceChatState: "disabled";
  plasmoVoiceState: "disabled"; relayEnabled: false; pathState: "notVerified";
  pathObservation?: PathObservation;
  preparation: ConnectPreparation | null; review: ConnectReview | null;
  cleanupState: "notStarted" | "pending" | "complete" | "failed";
  invitationProfile?: ConnectInvitationProfile; sessionTiming?: ConnectSessionTiming;
  fragmentedUdp?: FragmentedUdpContract;
  udpRecovery?: SessionUdpRecovery;
  hostGroup?: HostGroup;
  pendingGroupSource?: PendingGroupSource;
  pendingHostSource?: PendingHostSource;
  social?: SocialConnectProjection;
} & Partial<FragmentedGuestUdp>;
export type ConnectPreparation = { operationId: string; generation: number; publicId: string; expiresAtMs: number; invitationProfile?: ConnectInvitationProfile };
export type ConnectReview = {
  operationId: string; generation: number; digest: string; invitationDigest: string;
  role: ConnectRole; mode: ConnectMode; hostPublicId: string; guestPublicId: string;
  directAddress: string; gameTarget: string | null; localTcpBind: string | null;
  services: ({ serviceId: "game"; protocol: "tcp"; revision: 1; requiredCapability: "reliableStream" }
    | { serviceId: "aux"; protocol: "udp"; revision: 1; requiredCapability: "fragmentedDatagram" })[];
  expiresAtMs: number; invitationExpiresAtMs: number; lifetimeSeconds: number;
  statusProtocol: 767; statusHandshakeHost: "localhost"; statusHandshakePort: 43001;
  targetSource?: "manual" | "ownedInstance" | null;
  instanceId?: string | null; runId?: string | null;
  sessionTiming?: ConnectSessionTiming;
  fragmentedUdp?: FragmentedUdpReview;
};
export type ConnectExport = { invitation: string; hostPublicId: string; expiresAtMs: number; sessionTiming?: ConnectSessionTiming; fragmentedUdp?: FragmentedUdpContract };

const ERROR_TEXT: Record<string, string> = {
  connect_disabled: "O Aurora Connect experimental não está habilitado nesta versão.",
  connect_busy: "Já existe uma tentativa ativa nesta janela. Encerre-a antes de preparar outra.",
  connect_invalid_request: "Confira os endereços, o código público e o prazo informados.",
  connect_stale_operation: "Esta tentativa não está mais disponível. Atualize o estado antes de continuar.",
  connect_owner_mismatch: "Esta tentativa pertence a outra janela do launcher.",
  connect_plan_expired: "A preparação ou o convite expirou. Encerre esta tentativa e prepare uma nova.",
  connect_wrong_digest: "A revisão mudou. Revise novamente antes de autorizar a conexão.",
  connect_invitation_rejected: "O convite não pôde ser validado para este anfitrião e convidado.",
  connect_transport_failed: "Não foi possível estabelecer ou manter a conexão autenticada.",
  connect_port_unavailable: "Uma porta necessária está indisponível. Nenhum outro processo foi encerrado.",
  connect_status_failed: "O servidor não respondeu à consulta pela conexão desta tentativa.",
  connect_cancelled: "A operação foi cancelada.",
  connect_internal: "Não foi possível concluir a operação. Consulte o estado e tente novamente.",
  connect_source_changed: "O jogo ou sua porta mudou. Encerre esta tentativa e selecione o servidor novamente.",
  discovery_unavailable: "A descoberta de portas não está habilitada nesta versão.",
  discovery_invalid_request: "Selecione uma instância aberta pelo Aurora e repita a busca.",
  discovery_process_changed: "O jogo selecionado foi encerrado ou mudou. Repita a busca.",
  discovery_listener_changed: "A porta selecionada mudou. Repita a busca antes de compartilhar.",
  discovery_inventory_limit: "O inventário excedeu o limite desta integração experimental.",
  discovery_stale_review: "A seleção de porta expirou. Repita a busca.",
  discovery_owner_mismatch: "Esta seleção pertence a outra janela do launcher.",
  discovery_wrong_digest: "A revisão da porta mudou. Selecione e revise novamente.",
  discovery_busy: "Aguarde a consulta de porta em andamento.",
  discovery_status_failed: "A porta selecionada não respondeu à consulta Minecraft.",
  discovery_internal: "Não foi possível conferir a porta. Repita a busca.",
};
export function connectError(cause: unknown): string {
  return typeof cause === "string" && Object.hasOwn(ERROR_TEXT, cause)
    ? ERROR_TEXT[cause] : "Não foi possível concluir a operação. Consulte o estado e tente novamente.";
}

const PHASES: ConnectPhase[] = ["idle", "hostPreparing", "hostPrepared", "guestPrepared", "reviewed", "starting", "hostListening", "guestConnected", "closing", "closed", "failed"];
const TRANSPORTS: ConnectSnapshot["transportState"][] = ["notStarted", "listening", "admitted", "authenticated", "closed"];
const STATUSES: ConnectSnapshot["statusState"][] = ["notChecked", "checking", "responded", "failed"];
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
const protocol = (value: unknown): value is number => Number.isInteger(value) && (value as number) >= -2_147_483_648 && (value as number) <= 2_147_483_647;
const text = (value: unknown, max = 256): value is string => typeof value === "string" && value.length > 0 && value.length <= max && !/[\u0000-\u001f\u007f]/u.test(value);
const hash = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const role = (value: unknown): value is ConnectRole => value === "host" || value === "guest";
const mode = (value: unknown): value is ConnectMode => value === "loopback" || value === "externalIpv6" || value === "directIpv4";
const statusContract = (value: Record<string, unknown>) => value.statusProtocol === 767 && value.statusHandshakeHost === "localhost" && value.statusHandshakePort === 43001;
const invalid = (): never => { throw new Error("Invalid Connect projection"); };
const invitationProfile = (value: unknown) => value === undefined || value === "sessionGameTcp" || value === FRAGMENTED_PROFILE || value === SESSION_FRAGMENTED_PROFILE;
const fragmentedProfile = (value: unknown): value is typeof FRAGMENTED_PROFILE | typeof SESSION_FRAGMENTED_PROFILE => value === FRAGMENTED_PROFILE || value === SESSION_FRAGMENTED_PROFILE;
const noFragmentedFields = (value: Record<string, unknown>) => {
  if (["fragmentedUdp", "udpState", "udpAddress", "udpErrorCode", "udpOriginalCapacity", "udpCapacityScope", "udpTarget", "udpRecovery"]
    .some(key => value[key] !== undefined)) invalid();
};
export const sameSessionTiming = (left: ConnectSessionTiming, right: ConnectSessionTiming) =>
  left.profile === right.profile && left.schemaVersion === right.schemaVersion
  && left.admissionSeconds === right.admissionSeconds && left.sessionSeconds === right.sessionSeconds
  && left.admissionExpiresAtMs === right.admissionExpiresAtMs && left.sessionExpiresAtMs === right.sessionExpiresAtMs;

export function sessionTimingFrom(value: unknown): ConnectSessionTiming {
  if (!record(value) || !((value.profile === "sessionGameTcp" && value.schemaVersion === 2)
    || (value.profile === SESSION_FRAGMENTED_PROFILE && value.schemaVersion === 3))
    || Object.keys(value).length !== 6
    || !integer(value.admissionSeconds) || value.admissionSeconds < 1 || value.admissionSeconds > 600
    || !integer(value.sessionSeconds) || value.sessionSeconds < value.admissionSeconds || value.sessionSeconds > 3600
    || !integer(value.admissionExpiresAtMs) || !integer(value.sessionExpiresAtMs)
    || value.admissionExpiresAtMs > value.sessionExpiresAtMs
    || value.sessionExpiresAtMs - value.admissionExpiresAtMs !== (value.sessionSeconds - value.admissionSeconds) * 1000
    || value.admissionExpiresAtMs < value.admissionSeconds * 1000) return invalid();
  noFragmentedFields(value);
  return value as ConnectSessionTiming;
}

export function capabilitiesFrom(value: unknown): ConnectCapabilities {
  if (!record(value) || typeof value.available !== "boolean" || value.experimental !== true
    || Object.hasOwn(value, "pathObservation")
    || value.udpRecovery !== undefined
    || !(value.ownedInstanceDiscovery === undefined || typeof value.ownedInstanceDiscovery === "boolean")
    || value.maxLifetimeSeconds !== 600 || value.voiceEnabled !== false || value.relayEnabled !== false || !statusContract(value)
    || !Array.isArray(value.modes) || ![2, 3].includes(value.modes.length) || !value.modes.every(mode)
    || new Set(value.modes).size !== value.modes.length || !value.modes.includes("loopback") || !value.modes.includes("externalIpv6")
    || (value.modes.includes("directIpv4") && !value.available) || !Array.isArray(value.services)
    || value.services.length !== 1 || value.services[0] !== "game") return invalid();
  if (value.sessionInvitations !== undefined) {
    const session = value.sessionInvitations;
    if (!record(session) || session.profile !== "sessionGameTcp" || session.schemaVersion !== 2
      || session.maxAdmissionSeconds !== 600 || session.maxSessionSeconds !== 3600) return invalid();
    noFragmentedFields(session);
  }
  if (value.fragmentedUdpInvitations !== undefined) {
    if (!value.available) return invalid();
    fragmentedCapabilityFrom(value.fragmentedUdpInvitations);
  }
  if (value.sessionFragmentedUdpInvitations !== undefined) {
    if (!value.available) return invalid();
    sessionFragmentedCapabilityFrom(value.sessionFragmentedUdpInvitations);
  }
  if (value.sessionFragmentedHostGroups !== undefined) {
    if (!value.available || value.sessionFragmentedUdpInvitations === undefined) return invalid();
    const group = groupCapabilityFrom(value.sessionFragmentedHostGroups);
    if (group.ownedGameTcpDiscovery && value.ownedInstanceDiscovery !== true) return invalid();
  }
  if (value.readyGameLaunch !== undefined) {
    if (!value.available) return invalid();
    readyGameCapabilityFrom(value.readyGameLaunch);
  }
  return value as ConnectCapabilities;
}
export function snapshotFrom(value: unknown): ConnectSnapshot {
  if (!record(value) || typeof value.available !== "boolean" || !integer(value.generation)
    || !(value.operationId === null || text(value.operationId)) || !(value.role === null || role(value.role))
    || !PHASES.includes(value.phase as ConnectPhase) || !TRANSPORTS.includes(value.transportState as ConnectSnapshot["transportState"])
    || !STATUSES.includes(value.statusState as ConnectSnapshot["statusState"])
    || !(value.tcpAddress === null || text(value.tcpAddress)) || !(value.expiresAtMs === null || integer(value.expiresAtMs))
    || !(value.errorCode === null || text(value.errorCode)) || value.worldState !== "notVerified"
    || value.simpleVoiceChatState !== "disabled" || value.plasmoVoiceState !== "disabled"
    || value.relayEnabled !== false || value.pathState !== "notVerified"
    || !invitationProfile(value.invitationProfile)
    || !["notStarted", "pending", "complete", "failed"].includes(value.cleanupState as string)) return invalid();
  snapshotPathObservationFrom(value);
  if (value.social !== undefined) {
    const social = socialConnectProjectionFrom(value.social);
    if (value.role !== social.role || value.operationId === null || value.phase === "idle"
      || value.preparation !== null || value.review !== null || value.statusSummary !== null
      || value.invitationProfile !== undefined || value.sessionTiming !== undefined
      || value.hostGroup !== undefined || value.pendingGroupSource !== undefined || value.pendingHostSource !== undefined) return invalid();
    noFragmentedFields(value);
    if (value.tcpAddress !== null && !(typeof value.tcpAddress === "string" && /^127\.0\.0\.1:[1-9][0-9]{0,4}$/.test(value.tcpAddress) && Number(value.tcpAddress.split(":")[1]) <= 65535)) return invalid();
    return value as ConnectSnapshot;
  }
  if (value.pendingHostSource !== undefined) {
    pendingHostSourceFieldsFrom(value);
    return value as ConnectSnapshot;
  }
  if (value.pendingGroupSource !== undefined) {
    pendingGroupSourceFieldsFrom(value);
    return value as ConnectSnapshot;
  }
  if (value.phase === "hostPreparing") return invalid();
  if (value.hostGroup !== undefined) {
    groupSnapshotFieldsFrom(value);
    return value as ConnectSnapshot;
  }
  if (fragmentedProfile(value.invitationProfile)) fragmentedSnapshotFieldsFrom(value, value.invitationProfile);
  else noFragmentedFields(value);
  const timing = value.sessionTiming === undefined ? undefined : sessionTimingFrom(value.sessionTiming);
  if (value.phase === "idle" && value.invitationProfile !== undefined) return invalid();
  if (timing && value.invitationProfile !== timing.profile) return invalid();
  if (timing && ["hostPrepared", "guestPrepared"].includes(value.phase as string)) return invalid();
  if ((value.invitationProfile === "sessionGameTcp" || value.invitationProfile === SESSION_FRAGMENTED_PROFILE)
    && ["reviewed", "starting", "hostListening", "guestConnected"].includes(value.phase as string) && !timing) return invalid();
  if (timing && ["hostListening", "guestConnected"].includes(value.phase as string)
    && value.expiresAtMs !== timing.sessionExpiresAtMs) return invalid();
  if (value.preparation !== null) {
    const prepared = preparationFrom(value.preparation);
    if (prepared.invitationProfile !== value.invitationProfile) return invalid();
  }
  if (value.review !== null) {
    const reviewed = reviewFrom(value.review);
    if (Boolean(reviewed.fragmentedUdp) !== fragmentedProfile(value.invitationProfile)
      || (reviewed.fragmentedUdp && reviewed.fragmentedUdp.profile !== value.invitationProfile)) return invalid();
    if (Boolean(reviewed.sessionTiming) !== Boolean(timing)
      || (timing && reviewed.sessionTiming && !sameSessionTiming(timing, reviewed.sessionTiming))) return invalid();
  }
  if (value.statusSummary !== null) {
    const summary = value.statusSummary;
    if (!record(summary) || summary.requestedProtocol !== 767 || !protocol(summary.reportedProtocol)
      || typeof summary.protocolMatchesRequested !== "boolean"
      || ![summary.connectMicros, summary.statusMicros, summary.pingMicros, summary.totalMicros].every(integer)
      || ![summary.reportedPlayersOnline, summary.reportedPlayersMax].every(item => item === null || integer(item))) return invalid();
  }
  return value as ConnectSnapshot;
}
export function preparationFrom(value: unknown): ConnectPreparation {
  if (!record(value) || Object.hasOwn(value, "pathObservation") || !text(value.operationId) || !integer(value.generation) || !hash(value.publicId) || !integer(value.expiresAtMs)
    || value.kind !== undefined || value.hostGroup !== undefined || value.pendingGroupSource !== undefined || value.pendingHostSource !== undefined || !invitationProfile(value.invitationProfile) || value.sessionTiming !== undefined) return invalid();
  if (fragmentedProfile(value.invitationProfile)) fragmentedPreparationFieldsFrom(value, value.invitationProfile);
  else noFragmentedFields(value);
  return value as ConnectPreparation;
}
export function reviewFrom(value: unknown): ConnectReview {
  if (!record(value) || Object.hasOwn(value, "pathObservation") || value.kind !== undefined || value.hostGroup !== undefined || value.pendingGroupSource !== undefined || value.pendingHostSource !== undefined || value.participants !== undefined || !text(value.operationId) || !integer(value.generation) || !hash(value.digest)
    || !hash(value.invitationDigest) || !hash(value.hostPublicId) || !hash(value.guestPublicId)
    || !role(value.role) || !mode(value.mode) || !text(value.directAddress)
    || (value.mode === "directIpv4" && !isDirectIpv4Address(value.directAddress))
    || !(value.gameTarget === null || text(value.gameTarget)) || !(value.localTcpBind === null || value.localTcpBind === "127.0.0.1:0")
    || !integer(value.expiresAtMs) || !integer(value.invitationExpiresAtMs) || value.expiresAtMs > value.invitationExpiresAtMs
    || !statusContract(value) || !integer(value.lifetimeSeconds) || value.lifetimeSeconds < 1 || value.lifetimeSeconds > 600
    || !Array.isArray(value.services) || value.invitationProfile !== undefined) return invalid();
  const timing = value.sessionTiming === undefined ? undefined : sessionTimingFrom(value.sessionTiming);
  if (value.fragmentedUdp !== undefined) fragmentedReviewFieldsFrom(value, timing?.profile === SESSION_FRAGMENTED_PROFILE ? SESSION_FRAGMENTED_PROFILE : FRAGMENTED_PROFILE);
  else {
    noFragmentedFields(value);
    if (timing?.profile === SESSION_FRAGMENTED_PROFILE) return invalid();
    if (value.services.length !== 1) return invalid();
  }
  if (timing) {
    if (value.invitationExpiresAtMs !== timing.admissionExpiresAtMs || value.lifetimeSeconds !== timing.admissionSeconds) return invalid();
  }
  if (value.targetSource !== undefined) {
    if (value.targetSource === null) {
      if (value.role !== "guest" || value.instanceId !== null || value.runId !== null) return invalid();
    } else if (!["manual", "ownedInstance"].includes(value.targetSource as string)) return invalid();
    if (value.targetSource === "ownedInstance" && (value.role !== "host" || !text(value.instanceId) || !text(value.runId))) return invalid();
    if (value.targetSource === "manual" && (value.instanceId !== null || value.runId !== null)) return invalid();
  }
  const service = value.services[0];
  if (!record(service) || service.serviceId !== "game" || service.protocol !== "tcp" || service.revision !== 1 || service.requiredCapability !== "reliableStream") return invalid();
  return value as ConnectReview;
}
export function exportFrom(value: unknown): ConnectExport {
  if (!record(value) || Object.hasOwn(value, "pathObservation") || value.participantId !== undefined || value.hostGroup !== undefined || value.pendingGroupSource !== undefined || value.pendingHostSource !== undefined || typeof value.invitation !== "string" || !value.invitation.length
    || new TextEncoder().encode(value.invitation).length > 4096 || !hash(value.hostPublicId) || !integer(value.expiresAtMs)) return invalid();
  if (value.invitationProfile !== undefined) return invalid();
  const timing = value.sessionTiming === undefined ? undefined : sessionTimingFrom(value.sessionTiming);
  if (value.fragmentedUdp !== undefined) fragmentedExportFieldsFrom(value, timing?.profile === SESSION_FRAGMENTED_PROFILE ? SESSION_FRAGMENTED_PROFILE : FRAGMENTED_PROFILE);
  else {
    noFragmentedFields(value);
    if (timing?.profile === SESSION_FRAGMENTED_PROFILE) return invalid();
  }
  if (timing && value.expiresAtMs !== timing.admissionExpiresAtMs) return invalid();
  return value as ConnectExport;
}

export const phaseText: Record<ConnectPhase, string> = {
  idle: "Sem tentativa ativa", hostPreparing: "Conferindo a origem do grupo…", hostPrepared: "Anfitrião preparado para revisão", guestPrepared: "Convidado preparado para revisão",
  reviewed: "Aguardando sua autorização", starting: "Estabelecendo conexão…", hostListening: "Aguardando convidado",
  guestConnected: "Conexão do convidado disponível", closing: "Encerrando recursos…", closed: "Conexão encerrada", failed: "Falha observada",
};
export const transportText: Record<ConnectSnapshot["transportState"], string> = {
  notStarted: "Não iniciado", listening: "Anfitrião disponível", admitted: "Convidado admitido",
  authenticated: "Conexão autenticada", closed: "Encerrado",
};
export const statusText: Record<ConnectSnapshot["statusState"], string> = {
  notChecked: "Não consultado", checking: "Consultando servidor…", responded: "Servidor respondeu", failed: "Consulta não concluída",
};
export function connectDate(value: number): string { return new Date(value).toLocaleTimeString("pt-BR"); }
