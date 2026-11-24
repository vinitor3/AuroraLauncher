export type AiToolHistoryEntry = {
  id: string; kind: "diagnosis" | "advice" | "action"; createdAtMs: number; title: string; summary: string;
  issueIds: string[]; actionId: string | null; status: "recorded" | "applied" | "undone" | "pending";
  assessment: "unresolved" | "normal_exit" | "inconclusive" | null; undoAvailable: boolean;
};
export type AiToolPlan = { planId: string; digest: string; title: string; changes: Array<{ label: string; before: string; after: string }>;
  warnings: string[]; expiresAtMs: number };

// Native tool commands return fixed Portuguese messages. Unexpected exceptions
// are not suitable for display (or for sending back to a model).
export function nativeToolError(cause: unknown): string {
  return typeof cause === "string" && cause.length <= 400 && !/[\r\n]|[A-Za-z]:[\\/]|\/(?:home|Users)\/|(?:token|password|senha)\s*[:=]/i.test(cause)
    ? cause : "Não foi possível preparar ou concluir a alteração. Atualize a análise e tente novamente.";
}
