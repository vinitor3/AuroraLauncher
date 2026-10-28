export type SocialConnectStage = "reserved" | "authenticating" | "hostReview" | "waitingGuest" | "connecting" | "connected" | "closed" | "failed";
export type SocialConnectProjection = {
  sessionId: string; role: "host" | "guest"; stage: SocialConnectStage;
  digest: string | null; gameTarget: string | null; visibility: "public" | "friends" | null; expiresAtMs: number | null;
};
export type SocialConnectCapabilities = { available: boolean; experimental: true; accountEpoch: number; maxSessionSeconds: 60 };
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
export const socialSessionId = (value: unknown): value is string => typeof value === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(value);
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
const invalid = (): never => { throw new Error("Invalid social Connect projection"); };
export function socialConnectCapabilitiesFrom(value: unknown): SocialConnectCapabilities {
  if (!record(value) || Object.keys(value).length !== 4 || typeof value.available !== "boolean" || value.experimental !== true
    || !integer(value.accountEpoch) || value.maxSessionSeconds !== 60) return invalid();
  return value as SocialConnectCapabilities;
}
export function socialConnectProjectionFrom(value: unknown): SocialConnectProjection {
  if (!record(value) || Object.keys(value).length !== 7 || !socialSessionId(value.sessionId)
    || !["host", "guest"].includes(value.role as string)
    || !["reserved", "authenticating", "hostReview", "waitingGuest", "connecting", "connected", "closed", "failed"].includes(value.stage as string)
    || !(value.digest === null || typeof value.digest === "string" && /^[0-9a-f]{64}$/.test(value.digest))
    || !(value.gameTarget === null || value.gameTarget === "127.0.0.1:43001")
    || ![null, "public", "friends"].includes(value.visibility as null | string)
    || !(value.expiresAtMs === null || integer(value.expiresAtMs))) return invalid();
  if (value.role === "guest" && (value.digest !== null || value.gameTarget !== null || value.visibility !== null)) return invalid();
  if (value.stage === "hostReview" && (value.role !== "host" || value.digest === null || value.gameTarget === null || value.visibility === null || value.expiresAtMs === null)) return invalid();
  return value as SocialConnectProjection;
}
export function socialReservationFrom(value: unknown): { operationId: string; generation: number } {
  if (!record(value) || Object.keys(value).length !== 2 || !socialSessionId(value.operationId) || !integer(value.generation) || value.generation < 1) return invalid();
  return value as { operationId: string; generation: number };
}
export function socialConnectError(cause: unknown): string {
  const code = typeof cause === "string" ? cause : record(cause) && typeof cause.code === "string" ? cause.code : "";
  const messages: Record<string, string> = {
    connect_busy: "Já existe uma conexão em andamento. Encerre-a antes de entrar em outra sessão.",
    connect_social_unavailable: "A conexão social local não está disponível neste executável.",
    connect_social_account_changed: "A conta mudou. Atualize a sessão antes de continuar.",
    connect_social_failed: "A sessão não está disponível ou a permissão mudou. Atualize a lista.",
    connect_social_auth_failed: "Sua conta não pôde ser confirmada. Entre novamente no Aurora.",
    connect_social_epoch_mismatch: "A sessão da conta mudou. Atualize antes de continuar.",
    connect_plan_expired: "O prazo da sessão terminou. O anfitrião pode preparar outra sessão.",
    connect_port_unavailable: "A porta de conexão está ocupada. Nenhum outro processo foi encerrado.",
    connect_invitation_rejected: "A sessão não está disponível ou sua autorização mudou. Atualize a lista.",
    connect_transport_failed: "Não foi possível estabelecer ou manter a conexão. O servidor pode ter sido encerrado.",
    connect_cancelled: "Conexão cancelada.",
    connect_disabled: "A conexão social local não está disponível neste executável.",
    connect_stale_operation: "Esta tentativa não está mais disponível. Atualize o estado.",
    connect_account_cleanup_incomplete: "A conexão anterior ainda não foi encerrada. Tente encerrar novamente.",
    connect_cleanup_failed: "Não foi possível confirmar a liberação da conexão. Tente encerrar novamente.",
  };
  return messages[code] ?? "Não foi possível concluir a conexão. Atualize o estado antes de tentar novamente.";
}
export const socialStageText: Record<SocialConnectStage, string> = {
  reserved: "Preparando conexão…", authenticating: "Confirmando sua conta…", hostReview: "Revise o compartilhamento",
  waitingGuest: "Aguardando um participante", connecting: "Conectando…", connected: "Conexão autenticada",
  closed: "Conexão encerrada", failed: "Conexão indisponível",
};
