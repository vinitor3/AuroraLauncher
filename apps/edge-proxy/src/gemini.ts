import { AI_ERRORS, type AiErrorCode } from "../../../shared/aiErrors.ts";
import { readDoctorJson, withDoctorAbort } from "../../../shared/doctorTransport.ts";

// Google moves this alias to the latest Flash. The explicit current version is
// used only if the alias is missing or temporarily unavailable, never for quota.
export const GEMINI_MODEL = "gemini-flash-latest";
export const GEMINI_CURRENT_MODEL = "gemini-3.8-flash";
export const GEMINI_THINKING = { thinkingLevel: "low" };
export type GeminiEnvironment = { GEMINI_API_KEY?: string };
export class GeminiFailure extends Error {
  code: AiErrorCode;
  status: number;
  constructor(code: AiErrorCode, status = 503) { super(AI_ERRORS[code]); this.code = code; this.status = status; }
}
export function aiFailureResponse(cause: unknown): Response {
  const error = cause instanceof GeminiFailure ? cause : new GeminiFailure("provider_unavailable");
  return Response.json({ error: error.message, code: error.code }, { status: error.status,
    headers: { "Access-Control-Allow-Origin": "*", "Cache-Control": "no-store" } });
}

/** One total deadline and at most one fallback; quota/auth failures never retry. */
export async function generateGemini(env: GeminiEnvironment, body: unknown, signal: AbortSignal,
  maximumBytes = 32_768): Promise<{ text: string; model: string }> {
  const key = env.GEMINI_API_KEY?.trim();
  if (!key) throw new GeminiFailure("not_configured");
  try {
    for (const model of [GEMINI_MODEL, GEMINI_CURRENT_MODEL]) {
      signal.throwIfAborted();
      const response = await withDoctorAbort(fetch(`https://generativelanguage.googleapis.com/v1beta/models/${model}:generateContent`, {
        method: "POST", headers: { "Content-Type": "application/json", "x-goog-api-key": key },
        body: JSON.stringify(body), signal,
      }), signal);
      if (!response.ok) {
        // Inspect only a bounded error body to classify a rejected model. Never
        // return or log this body, even if it contains a credential or prompt.
        const failure = await readDoctorJson(response, 8_192, signal).catch(() => null) as { error?: { status?: string; message?: string } } | null;
        const apiStatus = typeof failure?.error?.status === "string" ? failure.error.status : "";
        const message = typeof failure?.error?.message === "string" ? failure.error.message : "";
        if (response.status === 429 || apiStatus === "RESOURCE_EXHAUSTED") throw new GeminiFailure("quota", 429);
        if (response.status === 401 || response.status === 403 || /API_KEY_INVALID|PERMISSION_DENIED|UNAUTHENTICATED/.test(apiStatus)
          || /api.?key.*(?:invalid|expired)|key.*leaked/i.test(message)) throw new GeminiFailure("provider_unauthorized");
        const missingModel = response.status === 404 || (response.status === 400 && /model.*(?:not found|not supported|unavailable)/i.test(message));
        if (missingModel && model === GEMINI_MODEL) continue;
        if (missingModel) throw new GeminiFailure("model_unavailable");
        if (response.status === 503 && model === GEMINI_MODEL) {
          await withDoctorAbort(new Promise<void>((resolve) => setTimeout(resolve, 500)), signal);
          continue;
        }
        if (response.status === 400) throw new GeminiFailure("invalid_request", 400);
        throw new GeminiFailure("provider_unavailable");
      }
      if (!response.headers.get("Content-Type")?.toLowerCase().includes("application/json")) {
        void response.body?.cancel().catch(() => undefined);
        throw new GeminiFailure("invalid_response", 502);
      }
      const payload = await readDoctorJson(response, maximumBytes, signal) as {
        modelVersion?: unknown;
        candidates?: Array<{ finishReason?: unknown; content?: { parts?: Array<{ text?: unknown; thought?: unknown }> } }>;
      };
      const candidate = payload?.candidates?.[0];
      if (candidate?.finishReason !== "STOP" || !Array.isArray(candidate.content?.parts)) throw new GeminiFailure("invalid_response", 502);
      const parts = candidate.content.parts.filter((part) => part && !part.thought);
      if (!parts.length || parts.some((part) => typeof part.text !== "string")) throw new GeminiFailure("invalid_response", 502);
      const text = parts.map((part) => part.text).join("").trim();
      if (!text || text.length > 12_000) throw new GeminiFailure("invalid_response", 502);
      const version = typeof payload.modelVersion === "string" && /^gemini-[a-z0-9.-]{1,80}$/.test(payload.modelVersion) ? payload.modelVersion : model;
      signal.throwIfAborted();
      return { text, model: version };
    }
    throw new GeminiFailure("model_unavailable");
  } catch (cause) {
    if (signal.aborted) throw new GeminiFailure("timeout", 504);
    if (cause instanceof GeminiFailure) throw cause;
    if (cause instanceof SyntaxError) throw new GeminiFailure("invalid_response", 502);
    throw new GeminiFailure("provider_unavailable");
  }
}
