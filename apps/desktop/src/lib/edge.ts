import type { FirebaseServices } from "./firebase";

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
      signal: AbortSignal.timeout(timeoutMs),
    });
  } catch (cause) {
    if (cause instanceof DOMException && cause.name === "TimeoutError") {
      throw new Error("O serviço Aurora demorou para responder. Tente novamente.");
    }
    throw new Error("Não foi possível conectar ao serviço Aurora.");
  }
  const payload = await response.json().catch(() => null) as { error?: string } & T | null;
  if (!response.ok) throw new Error(payload?.error ?? "O serviço Aurora não está disponível agora.");
  return payload as T;
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
