export type ConnectMode = "loopback" | "externalIpv6";
export type ConnectRole = "host" | "guest";
export type ConnectPhase = "idle" | "hostPrepared" | "guestPrepared" | "reviewed" | "starting" | "hostListening" | "guestConnected" | "closing" | "closed" | "failed";
export type ConnectCapabilities = {
  available: boolean; experimental: true; maxLifetimeSeconds: number;
  modes: ConnectMode[]; services: string[]; voiceEnabled: false; relayEnabled: false;
  statusProtocol: 767; statusHandshakeHost: "localhost"; statusHandshakePort: 43001;
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
  preparation: ConnectPreparation | null; review: ConnectReview | null;
  cleanupState: "notStarted" | "pending" | "complete" | "failed";
};
export type ConnectPreparation = { operationId: string; generation: number; publicId: string; expiresAtMs: number };
export type ConnectReview = {
  operationId: string; generation: number; digest: string; invitationDigest: string;
  role: ConnectRole; mode: ConnectMode; hostPublicId: string; guestPublicId: string;
  directAddress: string; gameTarget: string | null; localTcpBind: string | null;
  services: { serviceId: "game"; protocol: "tcp"; revision: 1; requiredCapability: "reliableStream" }[];
  expiresAtMs: number; invitationExpiresAtMs: number; lifetimeSeconds: number;
  statusProtocol: 767; statusHandshakeHost: "localhost"; statusHandshakePort: 43001;
};
export type ConnectExport = { invitation: string; hostPublicId: string; expiresAtMs: number };

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
};
export function connectError(cause: unknown): string {
  return typeof cause === "string" && Object.hasOwn(ERROR_TEXT, cause)
    ? ERROR_TEXT[cause] : "Não foi possível concluir a operação. Consulte o estado e tente novamente.";
}

const PHASES: ConnectPhase[] = ["idle", "hostPrepared", "guestPrepared", "reviewed", "starting", "hostListening", "guestConnected", "closing", "closed", "failed"];
const TRANSPORTS: ConnectSnapshot["transportState"][] = ["notStarted", "listening", "admitted", "authenticated", "closed"];
const STATUSES: ConnectSnapshot["statusState"][] = ["notChecked", "checking", "responded", "failed"];
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
const protocol = (value: unknown): value is number => Number.isInteger(value) && (value as number) >= -2_147_483_648 && (value as number) <= 2_147_483_647;
const text = (value: unknown, max = 256): value is string => typeof value === "string" && value.length > 0 && value.length <= max && !/[\u0000-\u001f\u007f]/u.test(value);
const hash = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const role = (value: unknown): value is ConnectRole => value === "host" || value === "guest";
const mode = (value: unknown): value is ConnectMode => value === "loopback" || value === "externalIpv6";
const statusContract = (value: Record<string, unknown>) => value.statusProtocol === 767 && value.statusHandshakeHost === "localhost" && value.statusHandshakePort === 43001;
const invalid = (): never => { throw new Error("Invalid Connect projection"); };

export function capabilitiesFrom(value: unknown): ConnectCapabilities {
  if (!record(value) || typeof value.available !== "boolean" || value.experimental !== true
    || value.maxLifetimeSeconds !== 600 || value.voiceEnabled !== false || value.relayEnabled !== false || !statusContract(value)
    || !Array.isArray(value.modes) || value.modes.length !== 2 || !value.modes.every(mode)
    || new Set(value.modes).size !== 2 || !Array.isArray(value.services)
    || value.services.length !== 1 || value.services[0] !== "game") return invalid();
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
    || !["notStarted", "pending", "complete", "failed"].includes(value.cleanupState as string)) return invalid();
  if (value.preparation !== null) preparationFrom(value.preparation);
  if (value.review !== null) reviewFrom(value.review);
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
  if (!record(value) || !text(value.operationId) || !integer(value.generation) || !hash(value.publicId) || !integer(value.expiresAtMs)) return invalid();
  return value as ConnectPreparation;
}
export function reviewFrom(value: unknown): ConnectReview {
  if (!record(value) || !text(value.operationId) || !integer(value.generation) || !hash(value.digest)
    || !hash(value.invitationDigest) || !hash(value.hostPublicId) || !hash(value.guestPublicId)
    || !role(value.role) || !mode(value.mode) || !text(value.directAddress)
    || !(value.gameTarget === null || text(value.gameTarget)) || !(value.localTcpBind === null || value.localTcpBind === "127.0.0.1:0")
    || !integer(value.expiresAtMs) || !integer(value.invitationExpiresAtMs) || value.expiresAtMs > value.invitationExpiresAtMs
    || !statusContract(value) || !integer(value.lifetimeSeconds) || value.lifetimeSeconds < 1 || value.lifetimeSeconds > 600
    || !Array.isArray(value.services) || value.services.length !== 1) return invalid();
  const service = value.services[0];
  if (!record(service) || service.serviceId !== "game" || service.protocol !== "tcp" || service.revision !== 1 || service.requiredCapability !== "reliableStream") return invalid();
  return value as ConnectReview;
}
export function exportFrom(value: unknown): ConnectExport {
  if (!record(value) || typeof value.invitation !== "string" || !value.invitation.length
    || new TextEncoder().encode(value.invitation).length > 4096 || !hash(value.hostPublicId) || !integer(value.expiresAtMs)) return invalid();
  return value as ConnectExport;
}

export const phaseText: Record<ConnectPhase, string> = {
  idle: "Sem tentativa ativa", hostPrepared: "Anfitrião preparado para revisão", guestPrepared: "Convidado preparado para revisão",
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
