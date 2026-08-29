export interface Env {
  FIREBASE_PROJECT_ID: string;
  GEMINI_API_KEY: string;
  CURSEFORGE_API_KEY: string;
  SUPABASE_API_KEY?: string;
  SUPABASE_API_KEY_SERVICE_ROLE?: string;
}

type FirebaseClaims = {
  aud: string;
  iss: string;
  sub: string;
  user_id?: string;
  exp: number;
  iat: number;
  auth_time: number;
};

type JwtHeader = { alg: string; kid?: string };
type RateWindow = { startedAt: number; count: number };

const windows = new Map<string, RateWindow>();
const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder();
const firebaseJwksUrl =
  "https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com";
const curseForgeApiBase = "https://api.curseforge.com/v1";
const supabaseBase = "https://pgbzkinjfwbncifurirb.supabase.co";
const appearanceBucket = "aurora-appearance";
const minecraftGameId = 432;
const curseForgeClassIds = { mod: 6, modpack: 4471, resourcepack: 12, shader: 6552 } as const;
const curseForgeLoaderIds = { forge: 1, fabric: 4 } as const;

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json; charset=utf-8",
      "Access-Control-Allow-Origin": "*",
      "Cache-Control": "no-store",
    },
  });
}

function error(message: string, status = 400): Response {
  return json({ error: message }, status);
}

function fromBase64Url(value: string): Uint8Array {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(value.length / 4) * 4, "=");
  const binary = atob(normalized);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

function parseJsonSegment<T>(segment: string): T {
  return JSON.parse(textDecoder.decode(fromBase64Url(segment))) as T;
}

async function verifyFirebaseToken(request: Request, env: Env): Promise<FirebaseClaims> {
  const authorization = request.headers.get("Authorization");
  if (!authorization?.startsWith("Bearer ")) throw new Error("Faça login no Aurora para continuar.");
  const token = authorization.slice("Bearer ".length).trim();
  const parts = token.split(".");
  if (parts.length !== 3) throw new Error("A sessão Aurora não é válida.");

  const header = parseJsonSegment<JwtHeader>(parts[0]);
  const claims = parseJsonSegment<FirebaseClaims>(parts[1]);
  if (header.alg !== "RS256" || !header.kid) throw new Error("A sessão Aurora não é válida.");

  const now = Math.floor(Date.now() / 1000);
  const expectedIssuer = "https://securetoken.google.com/" + env.FIREBASE_PROJECT_ID;
  const userId = claims.user_id ?? claims.sub;
  if (
    claims.aud !== env.FIREBASE_PROJECT_ID ||
    claims.iss !== expectedIssuer ||
    !userId ||
    claims.sub !== userId ||
    !Number.isFinite(claims.exp) ||
    claims.exp <= now ||
    !Number.isFinite(claims.iat) ||
    claims.iat > now ||
    !Number.isFinite(claims.auth_time) ||
    claims.auth_time > now
  ) {
    throw new Error("Sua sessão Aurora expirou. Entre novamente.");
  }

  const keysResponse = await fetch(firebaseJwksUrl, {
    cf: { cacheTtl: 3600, cacheEverything: true },
  });
  if (!keysResponse.ok) throw new Error("Não foi possível validar a sessão Aurora.");
  const keySet = await keysResponse.json() as { keys?: Array<JsonWebKey & { kid?: string; alg?: string; use?: string }> };
  const verificationKey = keySet.keys?.find((key) => key.kid === header.kid && key.kty === "RSA" && key.alg === "RS256");
  if (!verificationKey) throw new Error("Sua sessão Aurora precisa ser atualizada.");
  const publicKey = await crypto.subtle.importKey(
    "jwk",
    verificationKey,
    { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" },
    false,
    ["verify"],
  );
  const valid = await crypto.subtle.verify(
    { name: "RSASSA-PKCS1-v1_5" },
    publicKey,
    fromBase64Url(parts[2]),
    textEncoder.encode(parts[0] + "." + parts[1]),
  );
  if (!valid) throw new Error("A sessão Aurora não é válida.");
  return claims;
}

function enforceRateLimit(userId: string): void {
  const now = Date.now();
  const current = windows.get(userId);
  if (!current || now - current.startedAt >= 60_000) {
    windows.set(userId, { startedAt: now, count: 1 });
    return;
  }
  if (current.count >= 20) throw new Error("Muitas solicitações. Aguarde um minuto e tente novamente.");
  current.count += 1;
}

async function readBody(request: Request): Promise<Record<string, unknown>> {
  const type = request.headers.get("Content-Type") ?? "";
  if (!type.includes("application/json")) throw new Error("Envie uma solicitação JSON válida.");
  const body = await request.json();
  if (!body || typeof body !== "object" || Array.isArray(body)) throw new Error("Dados inválidos.");
  return body as Record<string, unknown>;
}

function boundedText(value: unknown, maximum: number, label: string): string {
  if (typeof value !== "string" || !value.trim() || value.trim().length > maximum) {
    throw new Error(label + " inválido.");
  }
  return value.trim();
}

function optionalBoundedText(value: unknown, maximum: number, label: string): string {
  if (value === undefined || value === null || value === "") return "";
  if (typeof value !== "string" || value.trim().length > maximum) throw new Error(label + " inválido.");
  return value.trim();
}

function positiveInteger(value: unknown, label: string): number {
  if (typeof value !== "number" || !Number.isInteger(value) || value <= 0) throw new Error(label + " inválido.");
  return value;
}

function pngDimensions(bytes: Uint8Array): { width: number; height: number } {
  const signature = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  if (bytes.length < 24 || signature.some((value, index) => bytes[index] !== value)
    || textDecoder.decode(bytes.slice(12, 16)) !== "IHDR") {
    throw new Error("O arquivo não é uma imagem PNG válida.");
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return { width: view.getUint32(16), height: view.getUint32(20) };
}

function supabaseServiceKey(env: Env): string {
  return env.SUPABASE_API_KEY_SERVICE_ROLE?.trim() || env.SUPABASE_API_KEY?.trim() || "";
}

function supabaseHeaders(env: Env): Record<string, string> {
  const key = supabaseServiceKey(env);
  return {
    apikey: key,
    Authorization: `Bearer ${key}`,
  };
}

let appearanceBucketReady = false;

async function ensureAppearanceBucket(env: Env): Promise<void> {
  if (appearanceBucketReady) return;
  if (!supabaseServiceKey(env)) throw new Error("O armazenamento de aparências ainda não foi configurado.");
  const headers = supabaseHeaders(env);
  const current = await fetch(`${supabaseBase}/storage/v1/bucket/${appearanceBucket}`, { headers });
  const lookupFailure = !current.ok
    ? await current.json().catch(() => null) as { error?: string; message?: string } | null
    : null;
  const lookupMessage = `${lookupFailure?.error ?? ""} ${lookupFailure?.message ?? ""}`.trim();
  if (current.status === 404 || (current.status === 400 && /bucket not found/i.test(lookupMessage))) {
    const created = await fetch(`${supabaseBase}/storage/v1/bucket`, {
      method: "POST",
      headers: { ...headers, "Content-Type": "application/json" },
      body: JSON.stringify({
        id: appearanceBucket,
        name: appearanceBucket,
        public: true,
        file_size_limit: 5 * 1024 * 1024,
        allowed_mime_types: ["image/png"],
      }),
    });
    if (!created.ok && created.status !== 409) {
      const failure = await created.json().catch(() => null) as { error?: string; message?: string } | null;
      console.error("Supabase bucket creation failed", {
        status: created.status,
        message: `${failure?.error ?? ""} ${failure?.message ?? ""}`.trim().slice(0, 200),
      });
      throw new Error("Não foi possível preparar o armazenamento de aparências.");
    }
  } else if (!current.ok) {
    console.error("Supabase bucket lookup failed", {
      status: current.status,
      message: lookupMessage.slice(0, 200),
    });
    throw new Error("O armazenamento de aparências não está disponível agora.");
  }
  appearanceBucketReady = true;
}

async function appearanceStorageHealth(env: Env): Promise<Response> {
  if (!supabaseServiceKey(env)) {
    return error("A chave de serviço do armazenamento não está configurada.", 503);
  }
  const response = await fetch(`${supabaseBase}/storage/v1/bucket/${appearanceBucket}`, {
    headers: supabaseHeaders(env),
  });
  if (!response.ok) {
    const failure = await response.json().catch(() => null) as { error?: string; message?: string } | null;
    console.error("Supabase storage health failed", {
      status: response.status,
      message: `${failure?.error ?? ""} ${failure?.message ?? ""}`.trim().slice(0, 200),
    });
    return error("O armazenamento de aparências não está disponível.", 503);
  }
  const bucket = await response.json().catch(() => null) as { id?: string; public?: boolean } | null;
  return json({
    status: "ok",
    service: "aurora-appearance",
    bucket: bucket?.id ?? appearanceBucket,
    public: bucket?.public === true,
    serviceRoleConfigured: Boolean(env.SUPABASE_API_KEY_SERVICE_ROLE?.trim()),
  });
}

async function uploadAppearance(request: Request, env: Env, userId: string): Promise<Response> {
  const contentType = request.headers.get("Content-Type") ?? "";
  if (!contentType.includes("multipart/form-data")) return error("Envie uma imagem PNG válida.");
  const form = await request.formData();
  const kind = form.get("kind");
  const file = form.get("file");
  if (kind !== "skin" && kind !== "cape") return error("Tipo de aparência inválido.");
  if (!(file instanceof File) || file.type !== "image/png" || file.size === 0 || file.size > 5 * 1024 * 1024) {
    return error("A imagem precisa ser um PNG de até 5 MB.");
  }
  const bytes = new Uint8Array(await file.arrayBuffer());
  const { width, height } = pngDimensions(bytes);
  if (kind === "skin" && !(width === 64 && (height === 64 || height === 32))) {
    return error(`A skin precisa ter 64x64 ou 64x32 pixels; esta imagem tem ${width}x${height}.`);
  }
  if (kind === "cape" && (width === 0 || height === 0 || width > 1024 || height > 1024)) {
    return error(`Dimensões de capa inválidas: ${width}x${height}.`);
  }

  await ensureAppearanceBucket(env);
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  const hash = Array.from(digest, (value) => value.toString(16).padStart(2, "0")).join("");
  const safeUserId = userId.replace(/[^A-Za-z0-9_-]/g, "");
  const objectPath = `profiles/${safeUserId}/${kind}-${hash}.png`;
  const uploaded = await fetch(
    `${supabaseBase}/storage/v1/object/${appearanceBucket}/${objectPath}`,
    {
      method: "POST",
      headers: {
        ...supabaseHeaders(env),
        "Content-Type": "image/png",
        "Cache-Control": "31536000",
        "x-upsert": "false",
      },
      body: bytes,
    },
  );
  if (!uploaded.ok) {
    const failure = await uploaded.json().catch(() => null) as { error?: string; message?: string } | null;
    const duplicate = (uploaded.status === 400 || uploaded.status === 409)
      && /duplicate|already exists/i.test(`${failure?.error ?? ""} ${failure?.message ?? ""}`);
    if (!duplicate) {
      console.error("Supabase appearance upload failed", { status: uploaded.status });
      return error("Não foi possível guardar a imagem agora.", 503);
    }
  }
  const publicUrl = `${supabaseBase}/storage/v1/object/public/${appearanceBucket}/${objectPath}`;
  return json({ url: publicUrl });
}

async function askGemini(request: Request, env: Env): Promise<Response> {
  if (!env.GEMINI_API_KEY) return error("O Assistente Aurora ainda não foi configurado.", 503);
  const body = await readBody(request);
  const message = boundedText(body.message, 2_000, "Mensagem");
  const mode = optionalBoundedText(body.mode, 20, "Modo");
  const screenshot = optionalBoundedText(body.screenshotBase64, 2_500_000, "Captura de tela");
  const complexQuestion = Boolean(screenshot)
    || message.length > 320
    || /\b(crash|erro|exception|stacktrace|log|config|conflito|incompat|kubejs|script|código|diagn[oó]stico)\b/i.test(message);
  const modelCandidates = complexQuestion
    ? ["gemini-3.7-flash", "gemini-3.5-flash-lite", "gemini-2.5-flash"]
    : mode === "inGame"
      ? ["gemini-3.5-flash-lite", "gemini-2.0-flash-lite"]
      : ["gemini-3.5-flash-lite", "gemini-3.7-flash", "gemini-2.5-flash"];
  const parts: Array<Record<string, unknown>> = [{ text: message }];
  if (screenshot) {
    const match = screenshot.match(/^data:(image\/(?:png|jpeg));base64,(.+)$/s);
    if (!match) return error("A captura de tela não é uma imagem PNG ou JPEG válida.");
    parts.unshift({ inlineData: { mimeType: match[1], data: match[2] } });
  }
  const requestBody = JSON.stringify({
    systemInstruction: {
      parts: [{
        text: "Você é o Assistente Aurora dentro do Minecraft. Responda em português, de forma curta, segura e útil. Não peça senhas, tokens ou dados privados.",
      }],
    },
    contents: [{ role: "user", parts }],
    generationConfig: { maxOutputTokens: 400 },
  });
  let lastFailure: { status: number; apiStatus: string } | null = null;
  for (const model of modelCandidates) {
    let response: Response;
    try {
      response = await fetch(
        `https://generativelanguage.googleapis.com/v1beta/models/${model}:generateContent`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json", "x-goog-api-key": env.GEMINI_API_KEY },
          body: requestBody,
          signal: AbortSignal.timeout(20_000),
        },
      );
    } catch (cause) {
      console.error("Gemini upstream timeout", {
        model,
        message: cause instanceof Error ? cause.message.slice(0, 200) : "falha de rede",
      });
      lastFailure = { status: 503, apiStatus: "UNAVAILABLE" };
      continue;
    }
    if (response.ok) {
      const result = await response.json() as { candidates?: Array<{ content?: { parts?: Array<{ text?: string }> } }> };
      const text = result.candidates?.[0]?.content?.parts?.map((part) => part.text ?? "").join("").trim();
      if (!text) return error("O Assistente Aurora não retornou uma resposta.", 503);
      return json({ text: text.slice(0, 8_000), model });
    }

    const upstream = await response.json().catch(() => null) as { error?: { message?: string; status?: string } } | null;
    const apiStatus = upstream?.error?.status ?? "";
    lastFailure = { status: response.status, apiStatus };
    console.error("Gemini upstream failure", {
      model,
      httpStatus: response.status,
      apiStatus,
      message: upstream?.error?.message?.slice(0, 300) ?? "sem detalhes",
    });
    if (response.status === 429 || /quota|resource_exhausted/i.test(apiStatus)) {
      return error("O limite temporário do Gemini foi atingido. Aguarde um pouco e tente novamente.", 429);
    }
    const unsupportedModel = response.status === 404
      || (response.status === 400 && /model|not found|not supported/i.test(upstream?.error?.message ?? ""));
    const temporarilyUnavailable = response.status === 503 || response.status === 524
      || /unavailable|high demand/i.test(apiStatus + " " + (upstream?.error?.message ?? ""));
    if (!unsupportedModel && !temporarilyUnavailable) break;
  }
  if (lastFailure?.status === 400 || lastFailure?.status === 403) {
    return error("A chave do Gemini não autorizou o serviço. Verifique se a Gemini API está ativa no Google AI Studio.", 503);
  }
  return error("O Assistente Aurora não está disponível agora.", 503);
}

async function curseForgeCatalog(request: Request, env: Env): Promise<Response> {
  if (!env.CURSEFORGE_API_KEY) return error("O catálogo CurseForge ainda não foi configurado.", 503);
  const body = await readBody(request);
  const action = boundedText(body.action, 20, "Ação");
  let path = "";
  const query = new URLSearchParams();
  if (action === "search") {
    const pageSize = body.pageSize ?? 20;
    const index = body.index ?? 0;
    if (!Number.isInteger(pageSize) || (pageSize as number) < 1 || (pageSize as number) > 50 || !Number.isInteger(index) || (index as number) < 0) {
      throw new Error("Paginação inválida.");
    }
    path = "/mods/search";
    const contentType = boundedText(body.contentType ?? "modpack", 20, "Tipo") as keyof typeof curseForgeClassIds;
    const classId = curseForgeClassIds[contentType];
    if (!classId) throw new Error("Tipo de conteúdo inválido.");
    query.set("gameId", String(minecraftGameId));
    query.set("classId", String(classId));
    const search = optionalBoundedText(body.query, 120, "Busca");
    if (search) query.set("searchFilter", search);
    const gameVersion = optionalBoundedText(body.gameVersion, 30, "Versão do Minecraft");
    if (gameVersion) query.set("gameVersion", gameVersion);
    const loader = optionalBoundedText(body.loader, 10, "Loader") as keyof typeof curseForgeLoaderIds | "";
    if (contentType === "mod" && gameVersion && loader && curseForgeLoaderIds[loader]) {
      query.set("modLoaderType", String(curseForgeLoaderIds[loader]));
    }
    const sort = optionalBoundedText(body.sort, 20, "Ordenação");
    if (sort === "popular") query.set("sortField", "6");
    if (sort === "updated") query.set("sortField", "3");
    query.set("sortOrder", "desc");
    query.set("pageSize", String(pageSize));
    query.set("index", String(index));
  } else if (action === "mod") {
    path = "/mods/" + positiveInteger(body.modId, "Projeto");
  } else if (action === "description") {
    path = "/mods/" + positiveInteger(body.modId, "Projeto") + "/description";
  } else if (action === "files") {
    path = "/mods/" + positiveInteger(body.modId, "Projeto") + "/files";
    const gameVersion = optionalBoundedText(body.gameVersion, 30, "Versão do Minecraft");
    if (gameVersion) query.set("gameVersion", gameVersion);
    const loader = optionalBoundedText(body.loader, 10, "Loader") as keyof typeof curseForgeLoaderIds | "";
    if (loader && curseForgeLoaderIds[loader]) query.set("modLoaderType", String(curseForgeLoaderIds[loader]));
    query.set("pageSize", "50");
  } else if (action === "file") {
    path = "/mods/" + positiveInteger(body.modId, "Projeto") + "/files/" + positiveInteger(body.fileId, "Arquivo");
  } else if (action === "download") {
    path = "/mods/" + positiveInteger(body.modId, "Projeto") + "/files/" + positiveInteger(body.fileId, "Arquivo") + "/download-url";
  } else {
    throw new Error("Ação de catálogo não permitida.");
  }
  const response = await fetch(curseForgeApiBase + path + (query.size ? "?" + query.toString() : ""), {
    headers: { Accept: "application/json", "x-api-key": env.CURSEFORGE_API_KEY },
  });
  if (!response.ok) return error(response.status === 404 ? "Conteúdo não encontrado." : "O catálogo CurseForge não está disponível agora.", response.status === 404 ? 404 : 503);
  return json(await response.json());
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    if (request.method === "OPTIONS") {
      return new Response(null, {
        status: 204,
        headers: {
          "Access-Control-Allow-Origin": "*",
          "Access-Control-Allow-Headers": "Authorization, Content-Type",
          "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
          "Access-Control-Max-Age": "86400",
        },
      });
    }
    const url = new URL(request.url);
    if (request.method === "GET" && url.pathname === "/health") return json({ status: "ok", service: "aurora-api" });
    if (request.method === "GET" && url.pathname === "/health/storage") return await appearanceStorageHealth(env);
    if (request.method !== "POST") return error("Rota não encontrada.", 404);
    try {
      const claims = await verifyFirebaseToken(request, env);
      const userId = claims.user_id ?? claims.sub;
      enforceRateLimit(userId);
      if (url.pathname === "/v1/assistant") return await askGemini(request, env);
      if (url.pathname === "/v1/curseforge") return await curseForgeCatalog(request, env);
      if (url.pathname === "/v1/appearance") return await uploadAppearance(request, env, userId);
      return error("Rota não encontrada.", 404);
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : "Não foi possível concluir a solicitação.";
      const status = /sessão|login/i.test(message) ? 401 : /Muitas solicitações/i.test(message) ? 429 : 400;
      return error(message, status);
    }
  },
} satisfies ExportedHandler<Env>;
