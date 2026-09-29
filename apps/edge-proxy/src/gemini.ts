import { AI_ERRORS, type AiErrorCode } from "../../../shared/aiErrors.ts";
import { readDoctorJson, withDoctorAbort } from "../../../shared/doctorTransport.ts";

// Google moves this alias to the latest Flash. The explicit current version is
// used only if the alias is missing or temporarily unavailable, never for quota.
export const GEMINI_MODEL = "gemini-flash-latest";
export const GEMINI_CURRENT_MODEL = "gemini-3.8-flash";
export const GEMINI_LITE_MODEL = "gemini-flash-lite-latest";
const GEMINI_LITE_CURRENT_MODEL = "gemini-3.5-flash-lite";
export const GEMINI_THINKING = { thinkingLevel: "low" };
export type GeminiEnvironment = { GEMINI_API_KEY?: string; GEMINI_API_KEY_2?: string; GEMINI_API_KEY_3?: string; GEMINI_KEY_ORDER?: string; GEMINI_FALLBACK_MODEL?: string };
export type GeminiOptions = { maximumTextLength?: number; model?: string; budget?: { remaining: number } };
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

/** Flash remains first. The explicitly enabled Lite fallback shares all limits. */
export async function generateGemini(env: GeminiEnvironment, body: unknown, signal: AbortSignal,
  maximumBytes = 32_768, options: GeminiOptions = {}): Promise<{ text: string; model: string }> {
  if (options.model !== undefined && ![GEMINI_MODEL, GEMINI_CURRENT_MODEL].includes(options.model)) throw new GeminiFailure("model_unavailable");
  if (!env.GEMINI_FALLBACK_MODEL) return generateWithKeys(env, body, signal, maximumBytes, options);
  if (env.GEMINI_FALLBACK_MODEL !== GEMINI_LITE_MODEL) throw new GeminiFailure("not_configured");
  const budget = options.budget ?? { remaining: 4 };
  if (budget.remaining <= 1) return generateWithKeys(env, body, signal, maximumBytes, { ...options, budget });
  // Reserve one provider call and roughly fifteen seconds for the lighter model.
  // The entire request still fits the alpha.13 client's existing forty seconds.
  const primaryBudget = { remaining: Math.min(3, budget.remaining - 1) };
  const initial = primaryBudget.remaining;
  const primarySignal = AbortSignal.any([signal, AbortSignal.timeout(20_000)]);
  try {
    return await generateWithKeys(env, body, primarySignal, maximumBytes, { ...options, budget: primaryBudget });
  } catch (cause) {
    if (signal.aborted) throw new GeminiFailure("timeout", 504);
    if (!(cause instanceof GeminiFailure) || !["provider_unavailable", "timeout", "model_unavailable"].includes(cause.code)) throw cause;
  } finally { budget.remaining -= initial - primaryBudget.remaining; }
  return generateWithKeys(env, body, signal, maximumBytes, { ...options, budget, model: GEMINI_LITE_MODEL });
}

/** Shared finite budget; secondary credentials only recover transient failures. */
async function generateWithKeys(env: GeminiEnvironment, body: unknown, signal: AbortSignal,
  maximumBytes: number, options: GeminiOptions): Promise<{ text: string; model: string }> {
  const order = env.GEMINI_KEY_ORDER ?? "1,2,3";
  const slots = order.split(",");
  if (slots.length > 3 || slots.some((slot) => !["1", "2", "3"].includes(slot)) || new Set(slots).size !== slots.length) throw new GeminiFailure("not_configured");
  const available = [env.GEMINI_API_KEY, env.GEMINI_API_KEY_2, env.GEMINI_API_KEY_3];
  const distinct = new Set<string>();
  const keys: Array<{ slot: number; key: string }> = [];
  for (const slot of slots.map(Number)) {
    const key = available[slot - 1]?.trim();
    if (key && !distinct.has(key)) { keys.push({ slot, key }); distinct.add(key); }
  }
  if (!keys.length) throw new GeminiFailure("not_configured");
  const budget = options.budget ?? { remaining: Math.max(2, keys.length + 1) };
  let model = options.model ?? GEMINI_MODEL;
  if (![GEMINI_MODEL, GEMINI_CURRENT_MODEL, GEMINI_LITE_MODEL, GEMINI_LITE_CURRENT_MODEL].includes(model)) throw new GeminiFailure("model_unavailable");
  let keyIndex = 0;
  let singleKeyRetried = false;
  let retries = 0;
  let attempts = 0;
  let failure = new GeminiFailure("provider_unavailable");
  const reportFailure = (status: number, code: AiErrorCode, started: number) => {
    // Operational metadata only. Never log headers, key values, prompts,
    // response/error bodies or raw exceptions.
    console.warn("Gemini request failure", { slot: keys[keyIndex].slot, model, attempt: attempts, status, code, elapsedMs: Date.now() - started });
  };
  const advance = () => {
    if (budget.remaining <= 0) return false;
    if (keyIndex + 1 < keys.length) { keyIndex++; return true; }
    if (keys.length === 1 && !singleKeyRetried) {
      singleKeyRetried = true;
      model = model === GEMINI_LITE_MODEL || model === GEMINI_LITE_CURRENT_MODEL ? GEMINI_LITE_CURRENT_MODEL : GEMINI_CURRENT_MODEL;
      return true;
    }
    return false;
  };
  try {
    while (budget.remaining > 0) {
      signal.throwIfAborted();
      if (retries > 0) {
        await withDoctorAbort(new Promise<void>((resolve) => setTimeout(resolve,
          Math.min(1_000 * 2 ** (retries - 1), 2_000) + Math.floor(Math.random() * 251))), signal);
      }
      signal.throwIfAborted();
      budget.remaining--;
      attempts++;
      const started = Date.now();
      // Real valid responses can exceed thirty seconds. Keep that room for the
      // preferred credential; early transient failures still permit failover.
      // The caller's global deadline always wins, including reads after headers.
      const attemptSignal = AbortSignal.any([signal, AbortSignal.timeout(34_000)]);
      let response: Response;
      try {
        response = await withDoctorAbort(fetch(`https://generativelanguage.googleapis.com/v1beta/models/${model}:generateContent`, {
          method: "POST", headers: { "Content-Type": "application/json", "x-goog-api-key": keys[keyIndex].key },
          body: JSON.stringify(body), signal: attemptSignal,
        }), attemptSignal);
      } catch {
        if (signal.aborted) throw new GeminiFailure("timeout", 504);
        failure = new GeminiFailure(attemptSignal.aborted ? "timeout" : "provider_unavailable", attemptSignal.aborted ? 504 : 503);
        reportFailure(0, failure.code, started);
        if (advance()) { retries++; continue; }
        throw failure;
      }
      if (!response.ok) {
        // A known terminal HTTP status stays terminal even if its body stalls.
        if ([401, 403, 402, 429].includes(response.status)) {
          void response.body?.cancel().catch(() => undefined);
          const code = response.status === 402 || response.status === 429 ? "quota" : "provider_unauthorized";
          reportFailure(response.status, code, started);
          throw new GeminiFailure(code, code === "quota" ? 429 : 503);
        }
        // Inspect only a bounded error body to classify a rejected model. Never
        // return or log this body, even if it contains a credential or prompt.
        const errorSignal = AbortSignal.any([attemptSignal, AbortSignal.timeout(1_000)]);
        const errorBody = await readDoctorJson(response, 8_192, errorSignal).catch(() => null) as { error?: { status?: string; message?: string } } | null;
        const apiStatus = typeof errorBody?.error?.status === "string" ? errorBody.error.status : "";
        const message = typeof errorBody?.error?.message === "string" ? errorBody.error.message : "";
        signal.throwIfAborted();
        if (apiStatus === "RESOURCE_EXHAUSTED") { reportFailure(response.status, "quota", started); throw new GeminiFailure("quota", 429); }
        if (/API_KEY_INVALID|PERMISSION_DENIED|UNAUTHENTICATED/.test(apiStatus)
          || /api.?key.*(?:invalid|expired|not valid)|key.*leaked/i.test(message)) {
          reportFailure(response.status, "provider_unauthorized", started); throw new GeminiFailure("provider_unauthorized");
        }
        const missingModel = response.status === 404 || (response.status === 400 && /model.*(?:not found|not supported|unavailable)/i.test(message));
        reportFailure(response.status, missingModel ? "model_unavailable" : response.status === 400 ? "invalid_request" : "provider_unavailable", started);
        if (missingModel && [GEMINI_MODEL, GEMINI_LITE_MODEL].includes(model) && budget.remaining > 0) {
          model = model === GEMINI_MODEL ? GEMINI_CURRENT_MODEL : GEMINI_LITE_CURRENT_MODEL; continue;
        }
        if (missingModel) throw new GeminiFailure("model_unavailable");
        if (response.status === 400) throw new GeminiFailure("invalid_request", 400);
        failure = new GeminiFailure("provider_unavailable");
        if ((response.status === 408 || response.status >= 500) && advance()) { retries++; continue; }
        throw failure;
      }
      if (!response.headers.get("Content-Type")?.toLowerCase().includes("application/json")) {
        void response.body?.cancel().catch(() => undefined);
        throw new GeminiFailure("invalid_response", 502);
      }
      let payload: {
        modelVersion?: unknown;
        candidates?: Array<{ finishReason?: unknown; content?: { parts?: Array<{ text?: unknown; thought?: unknown }> } }>;
      };
      try { payload = await readDoctorJson(response, maximumBytes, attemptSignal) as typeof payload; }
      catch {
        if (signal.aborted) throw new GeminiFailure("timeout", 504);
        if (attemptSignal.aborted) {
          failure = new GeminiFailure("timeout", 504);
          reportFailure(response.status, failure.code, started);
          if (advance()) { retries++; continue; }
          throw failure;
        }
        throw new GeminiFailure("invalid_response", 502);
      }
      const candidate = payload?.candidates?.[0];
      if (candidate?.finishReason !== "STOP" || !Array.isArray(candidate.content?.parts)) throw new GeminiFailure("invalid_response", 502);
      const parts = candidate.content.parts.filter((part) => part && !part.thought);
      if (!parts.length || parts.some((part) => typeof part.text !== "string")) throw new GeminiFailure("invalid_response", 502);
      const text = parts.map((part) => part.text).join("").trim();
      if (!text || text.length > (options.maximumTextLength ?? 12_000)) throw new GeminiFailure("invalid_response", 502);
      const version = typeof payload.modelVersion === "string" && /^gemini-[a-z0-9.-]{1,80}$/.test(payload.modelVersion) ? payload.modelVersion : model;
      signal.throwIfAborted();
      return { text, model: version };
    }
    throw failure;
  } catch (cause) {
    if (signal.aborted) throw new GeminiFailure("timeout", 504);
    if (cause instanceof GeminiFailure) throw cause;
    if (cause instanceof SyntaxError) throw new GeminiFailure("invalid_response", 502);
    throw new GeminiFailure("provider_unavailable");
  }
}
