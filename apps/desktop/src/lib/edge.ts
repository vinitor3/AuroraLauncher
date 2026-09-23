import type { FirebaseServices } from "./firebase";
import { buildDoctorAdviceInput, parseDoctorAdvice, type DoctorAdvice } from "../../../../shared/doctorAdvice.ts";
import { readDoctorJson, withDoctorAbort } from "../../../../shared/doctorTransport.ts";

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

export async function askAuroraAssistant(
  services: FirebaseServices,
  message: string,
) {
  const trimmed = message.trim();
  if (!trimmed || trimmed.length > 2_000) throw new Error("A mensagem deve ter entre 1 e 2.000 caracteres.");
  return (await request<{ text: string; model?: string }>(
    services,
    "/v1/assistant",
    {
      message: trimmed,
      mode: "launcher",
    },
    70_000,
  )).text;
}

/** Optional Doctor call. It never sends the serialized/local report or logs. */
export async function requestDoctorAdvice(services: FirebaseServices, report: unknown, cancellation: AbortSignal): Promise<DoctorAdvice> {
  const timeout = AbortSignal.timeout(25_000);
  const signal = AbortSignal.any([cancellation, timeout]);
  const input = buildDoctorAdviceInput(report);
  const user = services.auth.currentUser;
  if (!user) throw new Error("Entre na sua conta Aurora para consultar o Gemini.");
  const unavailable = "Não foi possível obter a explicação do Gemini. O diagnóstico local continua disponível.";
  try {
    signal.throwIfAborted();
    const token = await withDoctorAbort(user.getIdToken(), signal);
    signal.throwIfAborted();
    if (services.auth.currentUser?.uid !== user.uid) throw new DOMException("Sessão alterada.", "AbortError");
    const response = await withDoctorAbort(fetch(endpoint(services, "/v1/doctor/advice"), {
      method: "POST", headers: { Authorization: "Bearer " + token, "Content-Type": "application/json" },
      body: JSON.stringify(input), signal,
    }), signal);
    if (!response.ok) {
      void response.body?.cancel().catch(() => undefined);
      if (response.status === 429) throw new Error("O limite temporário do Gemini foi atingido. Aguarde um pouco; o diagnóstico local continua disponível.");
      if (response.status === 401) throw new Error("Entre novamente na sua conta Aurora para consultar o Gemini.");
      if (response.status === 404) throw new Error("O serviço de explicação do Doctor ainda não está disponível neste servidor. O diagnóstico local continua funcionando.");
      throw new Error(unavailable);
    }
    if (!response.headers.get("Content-Type")?.toLowerCase().includes("application/json")) {
      void response.body?.cancel().catch(() => undefined);
      throw new Error(unavailable);
    }
    const advice = parseDoctorAdvice(await readDoctorJson(response, 8_192, signal), input);
    if (!advice) throw new Error(unavailable);
    signal.throwIfAborted();
    if (services.auth.currentUser?.uid !== user.uid) throw new DOMException("Sessão alterada.", "AbortError");
    return advice;
  } catch (cause) {
    if (cancellation.aborted || services.auth.currentUser?.uid !== user.uid) throw new DOMException("Consulta cancelada.", "AbortError");
    if (timeout.aborted) throw new Error("O Gemini demorou para responder. O diagnóstico local continua disponível.");
    // Only our fixed public errors may reach UI; never server bodies or raw exceptions.
    if (cause instanceof Error && [unavailable,
      "O limite temporário do Gemini foi atingido. Aguarde um pouco; o diagnóstico local continua disponível.",
      "Entre novamente na sua conta Aurora para consultar o Gemini.",
      "O serviço de explicação do Doctor ainda não está disponível neste servidor. O diagnóstico local continua funcionando.",
    ].includes(cause.message)) throw cause;
    throw new Error(unavailable);
  }
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
