/** Stable public errors. Provider bodies, keys and exceptions never become UI text. */
export const AI_ERRORS = {
  not_configured: "A chave do Gemini não está configurada no Worker do Aurora.",
  provider_unauthorized: "A chave do Gemini foi recusada. Confira a chave e a autorização da Gemini API no Worker.",
  quota: "O limite temporário do Gemini foi atingido. Confira a cota do projeto ou aguarde antes de tentar novamente.",
  model_unavailable: "O modelo Gemini atual não está disponível para este projeto. Confira o acesso ao Gemini 3.8 no Google AI Studio.",
  provider_unavailable: "O Gemini está indisponível agora. Tente novamente mais tarde.",
  invalid_response: "O Gemini não retornou uma resposta válida. Nenhuma ação foi executada.",
  timeout: "O Gemini demorou para responder. Tente novamente; nenhuma ação foi executada.",
  session: "Sua sessão Aurora expirou. Entre novamente para consultar o Gemini.",
  service_not_deployed: "Esta função ainda não está publicada no Worker do Aurora. Atualize o serviço para usar o Gemini.",
  network: "Não foi possível conectar ao serviço Aurora. Confira sua conexão.",
  invalid_request: "Os dados da consulta estão desatualizados ou inválidos. Analise a instância novamente.",
} as const;
export type AiErrorCode = keyof typeof AI_ERRORS;
export class AiServiceError extends Error {
  readonly code: AiErrorCode;
  constructor(code: AiErrorCode) { super(AI_ERRORS[code]); this.name = "AiServiceError"; this.code = code; }
}
export function aiErrorCode(value: unknown): AiErrorCode | undefined {
  return typeof value === "string" && Object.hasOwn(AI_ERRORS, value) ? value as AiErrorCode : undefined;
}
export function publicAiError(cause: unknown): string {
  if (cause && typeof cause === "object" && "code" in cause) {
    const code = aiErrorCode(cause.code); if (code) return AI_ERRORS[code];
  }
  if (cause instanceof Error && Object.values(AI_ERRORS).some((message) => message === cause.message)) return cause.message;
  return AI_ERRORS.invalid_response;
}
