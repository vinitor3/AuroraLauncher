import type { FirebaseServices } from "./firebase";
import { buildDoctorAdviceInput, parseDoctorAdvice, type DoctorAdvice } from "../../../../shared/doctorAdvice.ts";
import { readDoctorJson, withDoctorAbort } from "../../../../shared/doctorTransport.ts";
import { aiErrorCode, AiServiceError } from "../../../../shared/aiErrors.ts";
import { buildAiToolRequest, parseAiToolAdvice, type AiToolAdvice, type AiToolContext } from "../../../../shared/aiTools.ts";

function endpoint(services: FirebaseServices, path: string) {
  const base = services.config.workerUrl?.trim().replace(/\/+$/, "");
  if (!base || !/^https:\/\//i.test(base)) {
    throw new Error("O serviço Aurora está temporariamente indisponível.");
  }
  return base + path;
}

async function request<T>(
  services: FirebaseServices,
  path: string,
  body: Record<string, unknown>,
  timeoutMs = 25_000,
  signal?: AbortSignal,
): Promise<T> {
  const user = services.auth.currentUser;
  if (!user) throw new Error("Entre na sua conta Aurora para continuar.");
  let response: Response;
  try {
    response = await fetch(endpoint(services, path), {
      method: "POST",
      headers: {
        Authorization: "Bearer " + await user.getIdToken(),
        "Content-Type": "application/json",
      },
      body: JSON.stringify(body),
      signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(timeoutMs)]) : AbortSignal.timeout(timeoutMs),
    });
  } catch (cause) {
    if (signal?.aborted) throw new DOMException("Tradução cancelada.", "AbortError");
    if (cause instanceof DOMException && cause.name === "TimeoutError") {
      throw new Error("O serviço Aurora demorou para responder. Tente novamente.");
    }
    throw new Error("Não foi possível conectar ao serviço Aurora.");
  }
  const payload = await response.json().catch(() => null) as { error?: string } & T | null;
  if (!response.ok) throw new Error(payload?.error ?? "O serviço Aurora não está disponível agora.");
  return payload as T;
}

export async function translateProjectSegments(services: FirebaseServices, segments: string[], signal: AbortSignal) {
  const result = await request<{ translations?: unknown }>(services, "/v1/translate", {
    targetLanguage: "pt-BR",
    segments,
  }, 50_000, signal);
  return result?.translations;
}

/** All AI surfaces share auth, bounded transport, cancellation and public errors. */
async function requestAi(services: FirebaseServices, path: string, body: unknown, cancellation: AbortSignal): Promise<unknown> {
  const timeout = AbortSignal.timeout(40_000);
  const signal = AbortSignal.any([cancellation, timeout]);
  const user = services.auth.currentUser;
  if (!user) throw new AiServiceError("session");
  try {
    signal.throwIfAborted();
    const token = await withDoctorAbort(user.getIdToken(), signal);
    signal.throwIfAborted();
    if (services.auth.currentUser?.uid !== user.uid) throw new DOMException("Sessão alterada.", "AbortError");
    const response = await withDoctorAbort(fetch(endpoint(services, path), {
      method: "POST", headers: { Authorization: "Bearer " + token, "Content-Type": "application/json" },
      body: JSON.stringify(body), signal,
    }), signal);
    let payload: unknown;
    try { payload = await readDoctorJson(response, 32_768, signal); }
    catch {
      if (response.ok) throw new AiServiceError("invalid_response");
    }
    if (!response.ok) {
      const code = payload && typeof payload === "object" && "code" in payload ? aiErrorCode(payload.code) : undefined;
      throw new AiServiceError(code ?? (response.status === 401 ? "session" : response.status === 404 ? "service_not_deployed"
        : response.status === 429 ? "quota" : response.status === 504 ? "timeout" : "provider_unavailable"));
    }
    if (!response.headers.get("Content-Type")?.toLowerCase().includes("application/json")) throw new AiServiceError("invalid_response");
    signal.throwIfAborted();
    if (services.auth.currentUser?.uid !== user.uid) throw new DOMException("Sessão alterada.", "AbortError");
    return payload;
  } catch (cause) {
    if (cancellation.aborted || services.auth.currentUser?.uid !== user.uid) throw new DOMException("Consulta cancelada.", "AbortError");
    if (timeout.aborted || (cause instanceof DOMException && cause.name === "TimeoutError")) throw new AiServiceError("timeout");
    if (cause instanceof AiServiceError) throw cause;
    throw new AiServiceError("network");
  }
}

export async function askAuroraAssistant(services: FirebaseServices, message: string, signal: AbortSignal = new AbortController().signal) {
  const trimmed = message.trim();
  if (!trimmed || trimmed.length > 2_000) throw new Error("A mensagem deve ter entre 1 e 2.000 caracteres.");
  const payload = await requestAi(services, "/v1/assistant", { message: trimmed, mode: "launcher" }, signal);
  if (!payload || typeof payload !== "object" || !("text" in payload) || typeof payload.text !== "string" || !payload.text.trim()
    || payload.text.length > 8_000) throw new AiServiceError("invalid_response");
  return payload.text;
}

export async function requestDoctorAdvice(services: FirebaseServices, report: unknown, signal: AbortSignal): Promise<DoctorAdvice> {
  const input = buildDoctorAdviceInput(report);
  const advice = parseDoctorAdvice(await requestAi(services, "/v1/doctor/advice", input, signal), input);
  if (!advice) throw new AiServiceError("invalid_response");
  return advice;
}

export async function requestAiToolAdvice(services: FirebaseServices, context: AiToolContext, question: string,
  signal: AbortSignal, doctorReport?: unknown): Promise<AiToolAdvice> {
  const input = buildAiToolRequest(context, question, doctorReport);
  const advice = parseAiToolAdvice(await requestAi(services, "/v1/ai/tools", input, signal), input.context);
  if (!advice) throw new AiServiceError("invalid_response");
  return advice;
}

export async function uploadAuroraAppearanceImage(
  services: FirebaseServices,
  kind: "skin" | "cape",
  file: File,
) {
  const user = services.auth.currentUser;
  if (!user) throw new Error("Entre na sua conta Aurora para continuar.");
  if (file.type !== "image/png" || file.size === 0 || file.size > 5 * 1024 * 1024) {
    throw new Error("A imagem precisa ser um PNG de até 5 MB.");
  }
  const form = new FormData();
  form.set("kind", kind);
  form.set("file", file, `${kind}.png`);
  const response = await fetch(endpoint(services, "/v1/appearance"), {
    method: "POST",
    headers: { Authorization: "Bearer " + await user.getIdToken() },
    body: form,
  });
  const payload = await response.json().catch(() => null) as { url?: string; error?: string } | null;
  if (!response.ok || !payload?.url) throw new Error(payload?.error ?? "Não foi possível guardar a imagem agora.");
  return payload.url;
}

export type CurseForgeCatalogRequest =
  | { action: "search"; query?: string; contentType: "mod" | "shader" | "resourcepack" | "modpack"; gameVersion?: string; loader?: "forge" | "fabric"; categoryId?: number; sort?: "relevance" | "popular" | "updated"; pageSize?: number; index?: number }
  | { action: "categories"; contentType: "mod" | "shader" | "resourcepack" | "modpack" }
  | { action: "mod"; modId: number }
  | { action: "description"; modId: number }
  | { action: "files"; modId: number; gameVersion?: string; loader?: "forge" | "fabric" }
  | { action: "file"; modId: number; fileId: number }
  | { action: "download"; modId: number; fileId: number }
  | { action: "updates"; contentType: "mod" | "shader" | "resourcepack" | "modpack"; gameVersion: string; loader?: "forge" | "fabric"; items: CurseForgeUpdateRequestItem[] }
  | { action: "resolveProjects"; projectIds: number[]; gameVersion: string; loader?: "forge" | "fabric" }
  | { action: "resolveFiles"; files: Array<{ projectId: number; fileId: number }> };

export type CurseForgeUpdateRequestItem = {
  projectId: number;
} & (
  | { currentSha1: string; currentFileId?: never }
  | { currentFileId: number; currentSha1?: never }
);

export type CurseForgeUpdateReason =
  | "loaderRequired"
  | "projectNotFound"
  | "contentTypeMismatch"
  | "installedFileNotFound"
  | "installedFileContextMismatch"
  | "installedVersionNotOlder"
  | "noCompatibleRelease"
  | "scanLimitReached"
  | "missingSha1"
  | "manualDownloadRequired";

export type CurseForgeUpdateProject = {
  id: number;
  classId?: number;
  name?: string;
  slug?: string;
  links?: { websiteUrl?: string };
  logo?: { thumbnailUrl?: string; url?: string };
};

export type CurseForgeUpdateFile = {
  id: number;
  modId: number;
  fileName: string;
  displayName?: string;
  fileDate?: string;
  fileLength?: number;
  releaseType: number;
  gameVersions: string[];
  hashes: Array<{ algo: number; value: string }>;
  dependencies?: Array<{ modId: number; relationType: number }>;
  downloadUrl?: string;
  sha1: string;
  downloadMode: "proxy" | "officialPage";
  manualDownloadRequired: boolean;
};

export type CurseForgeUpdateResult = {
  projectId: number;
  project: CurseForgeUpdateProject;
  targetFile?: CurseForgeUpdateFile;
  status: "updateAvailable" | "upToDate" | "blocked";
  reason?: CurseForgeUpdateReason;
  message: string;
} & (
  | { currentSha1: string; currentFileId?: never }
  | { currentFileId: number; currentSha1?: never }
);

export type CurseForgeUpdatesResponse = { data: CurseForgeUpdateResult[] };

export function requestCurseForgeCatalog<T>(services: FirebaseServices, body: CurseForgeCatalogRequest) {
  const timeoutMs = body.action === "resolveFiles" || body.action === "resolveProjects" || body.action === "updates"
    ? 300_000
    : 45_000;
  return request<T>(services, "/v1/curseforge", body, timeoutMs);
}
