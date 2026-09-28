import type { AiToolAdvice, AiToolContext } from "../../../../../shared/aiTools";
import type { DoctorReport } from "../doctor/types";
export type { AiToolPlan, AiToolHistoryEntry } from "../../../../../shared/aiTools";

export type AiToolAdviceRequest = (context: AiToolContext, question: string, signal: AbortSignal, report?: DoctorReport) => Promise<AiToolAdvice>;

// Native tool commands return fixed Portuguese messages. Unexpected exceptions
// are not suitable for display (or for sending back to a model).
export function nativeToolError(cause: unknown): string {
  return typeof cause === "string" && cause.length <= 400 && !/[\r\n]|[A-Za-z]:[\\/]|\/(?:home|Users)\/|(?:token|password|senha)\s*[:=]/i.test(cause)
    ? cause : "Não foi possível preparar ou concluir a alteração. Atualize a análise e tente novamente.";
}
