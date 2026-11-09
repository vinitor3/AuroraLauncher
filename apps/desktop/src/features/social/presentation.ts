import { socialImageUrl } from "../../shared/social";

/** Share the wire validator: even a malformed projection cannot choose an image host. */
export function socialArtwork(value: string | null | undefined): string | undefined {
  return socialImageUrl(value) ?? undefined;
}

export function socialInitials(value: string): string {
  return value.trim().slice(0, 2).toUpperCase() || "◇";
}

export function socialError(error: unknown): string {
  const code = typeof error === "object" && error !== null && "code" in error
    ? String((error as { code: unknown }).code) : "";
  if (code.includes("permission-denied") || code === "forbidden") return "Esta ação não está disponível para sua conta ou a permissão mudou. Atualize a lista.";
  if (code.includes("unauthenticated") || code === "signed_out") return "Sua sessão terminou. Entre novamente no Aurora.";
  if (code.includes("resource-exhausted") || code === "rate_limited") return "Muitas ações em pouco tempo. Aguarde um pouco antes de tentar novamente.";
  if (code === "already_exists") return "Já existe um pedido ou uma amizade com essa pessoa. Atualize a lista.";
  if (code === "invalid_nickname") return "Use o nick completo do Aurora: de 3 a 16 letras, números ou _.";
  if (code === "join_unavailable") return "A entrada automática ainda não está habilitada nesta versão.";
  if (code === "social_uncertain") return "A resposta demorou. A alteração pode ter sido aplicada; atualize a lista antes de repetir.";
  if (code === "social_clock_unavailable") return "Não foi possível conferir o horário do serviço. Atualize a lista para tentar novamente; o relógio do computador não foi alterado.";
  return "Não foi possível concluir. Confira sua conexão e atualize a lista antes de tentar novamente.";
}
