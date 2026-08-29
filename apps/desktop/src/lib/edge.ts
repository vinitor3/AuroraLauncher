import type { FirebaseServices } from "./firebase";

function endpoint(services: FirebaseServices, path: string) {
  const base = services.config.workerUrl?.trim().replace(/\/+$/, "");
  if (!base || !/^https:\/\//i.test(base)) {
    throw new Error("O Assistente Aurora está temporariamente indisponível.");
  }
  return base + path;
}

async function request<T>(services: FirebaseServices, path: string, body: Record<string, unknown>): Promise<T> {
  const user = services.auth.currentUser;
  if (!user) throw new Error("Entre na sua conta Aurora para continuar.");
  const response = await fetch(endpoint(services, path), {
    method: "POST",
    headers: {
      Authorization: "Bearer " + await user.getIdToken(),
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
  const payload = await response.json().catch(() => null) as { error?: string } & T | null;
  if (!response.ok) throw new Error(payload?.error ?? "O serviço Aurora não está disponível agora.");
  return payload as T;
}

export async function askAuroraAssistant(
  services: FirebaseServices,
  message: string,
  options?: { mode?: "launcher" | "inGame"; screenshotBase64?: string },
) {
  const trimmed = message.trim();
  if (!trimmed || trimmed.length > 2_000) throw new Error("A mensagem deve ter entre 1 e 2.000 caracteres.");
  return (await request<{ text: string; model?: string }>(services, "/v1/assistant", {
    message: trimmed,
    mode: options?.mode ?? "launcher",
    ...(options?.screenshotBase64 ? { screenshotBase64: options.screenshotBase64 } : {}),
  })).text;
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
  | { action: "search"; query?: string; contentType: "mod" | "shader" | "resourcepack" | "modpack"; gameVersion?: string; loader?: "forge" | "fabric"; sort?: "relevance" | "popular" | "updated"; pageSize?: number; index?: number }
  | { action: "mod"; modId: number }
  | { action: "description"; modId: number }
  | { action: "files"; modId: number; gameVersion?: string; loader?: "forge" | "fabric" }
  | { action: "file"; modId: number; fileId: number }
  | { action: "download"; modId: number; fileId: number };

export function requestCurseForgeCatalog<T>(services: FirebaseServices, body: CurseForgeCatalogRequest) {
  return request<T>(services, "/v1/curseforge", body);
}
