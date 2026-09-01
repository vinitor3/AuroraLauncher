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
type HealthSnapshot = { body: string; expiresAt: number; status: number };

const windows = new Map<string, RateWindow>();
const curseForgeResolvedDownloads = new Map<string, { expiresAt: number; url: string }>();
const curseForgeTicketUses = new Map<string, { count: number; expiresAt: number }>();
let curseForgeHealthSnapshot: HealthSnapshot | undefined;
let curseForgeHealthPending: Promise<HealthSnapshot> | undefined;
let curseForgeSigningKey: { source: string; value: Promise<CryptoKey> } | undefined;
const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder();
const firebaseJwksUrl =
  "https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com";
const curseForgeApiBase = "https://api.curseforge.com/v1";
const supabaseBase = "https://pgbzkinjfwbncifurirb.supabase.co";
const appearanceBucket = "aurora-appearance";
const auroraApiRevision = "2026-09-01-curseforge-categories-v3";
const minecraftGameId = 432;
const curseForgeClassIds = { mod: 6, modpack: 4471, resourcepack: 12, shader: 6552 } as const;
const curseForgeLoaderIds = { forge: 1, fabric: 4 } as const;
const curseForgeRequestTimeoutMs = 15_000;
const curseForgeDownloadTicketLifetimeMs = 8 * 60 * 60 * 1_000;

class PublicError extends Error {
  readonly status: number;

  constructor(message: string, status: number) {
    super(message);
    this.status = status;
  }
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function curseForgeApiKey(env: Env): string {
  const value = env.CURSEFORGE_API_KEY?.trim() ?? "";
  const quoted = value.match(/^(["'])(.*)\1$/s);
  return (quoted?.[2] ?? value).trim();
}

async function mapWithConcurrency<T, R>(
  items: T[],
  concurrency: number,
  mapper: (item: T, index: number) => Promise<R>,
): Promise<R[]> {
  const results = new Array<R>(items.length);
  let nextIndex = 0;
  const workers = Array.from({ length: Math.min(concurrency, items.length) }, async () => {
    while (nextIndex < items.length) {
      const index = nextIndex;
      nextIndex += 1;
      results[index] = await mapper(items[index], index);
    }
  });
  await Promise.all(workers);
  return results;
}

function curseForgeHeaders(env: Env, jsonBody = false): Record<string, string> {
  return {
    Accept: "application/json",
    ...(jsonBody ? { "Content-Type": "application/json" } : {}),
    "x-api-key": curseForgeApiKey(env),
  };
}

async function fetchCurseForge(
  env: Env,
  path: string,
  init: RequestInit = {},
): Promise<Response> {
  let lastResponse: Response | undefined;
  for (let attempt = 0; attempt < 2; attempt += 1) {
    try {
      const response = await fetch(curseForgeApiBase + path, {
        ...init,
        headers: { ...curseForgeHeaders(env, Boolean(init.body)), ...(init.headers ?? {}) },
        signal: AbortSignal.timeout(curseForgeRequestTimeoutMs),
      });
      lastResponse = response;
      if (response.status !== 429 && response.status < 500) return response;
      if (attempt === 0) await response.body?.cancel();
    } catch (cause) {
      console.error("CurseForge upstream network failure", {
        path: path.split("?")[0],
        attempt: attempt + 1,
        message: cause instanceof Error ? cause.message.slice(0, 160) : "falha de rede",
      });
      if (attempt === 1) {
        throw new PublicError("O CurseForge demorou para responder. Tente novamente em instantes.", 503);
      }
    }
    if (attempt === 0) await delay(250);
  }
  return lastResponse!;
}

async function curseForgeFailure(response: Response, operation: string): Promise<PublicError> {
  const upstream = await response.json().catch(() => null) as {
    error?: string;
    message?: string;
  } | null;
  console.error("CurseForge upstream failure", {
    operation,
    status: response.status,
    message: `${upstream?.error ?? ""} ${upstream?.message ?? ""}`.trim().slice(0, 200),
  });
  if (response.status === 400) return new PublicError("O CurseForge recusou estes filtros. Revise a busca e tente novamente.", 400);
  if (response.status === 401 || response.status === 403) {
    return new PublicError("A autorização do catálogo CurseForge precisa ser renovada pelo Aurora.", 503);
  }
  if (response.status === 404) return new PublicError("Conteúdo não encontrado.", 404);
  if (response.status === 429) return new PublicError("O limite temporário do CurseForge foi atingido. Aguarde um pouco.", 429);
  return new PublicError("O catálogo CurseForge não está disponível agora.", 503);
}

async function requireCurseForge(
  env: Env,
  path: string,
  operation: string,
  init: RequestInit = {},
): Promise<Response> {
  const response = await fetchCurseForge(env, path, init);
  if (!response.ok) throw await curseForgeFailure(response, operation);
  return response;
}

async function readCurseForgeJson<T>(response: Response, operation: string): Promise<T> {
  try {
    const payload = await response.json();
    if (!payload || typeof payload !== "object" || Array.isArray(payload)) throw new Error("payload inválido");
    return payload as T;
  } catch {
    console.error("CurseForge invalid JSON response", { operation });
    throw new PublicError("O CurseForge retornou uma resposta inválida. Tente novamente mais tarde.", 503);
  }
}

function invalidCurseForgeResponse(): PublicError {
  return new PublicError("O CurseForge retornou uma resposta inválida. Tente novamente mais tarde.", 503);
}

function toBase64Url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

async function downloadTicketSignature(env: Env, payload: string): Promise<string> {
  const source = curseForgeApiKey(env);
  if (!curseForgeSigningKey || curseForgeSigningKey.source !== source) {
    curseForgeSigningKey = {
      source,
      value: crypto.subtle.importKey(
        "raw",
        textEncoder.encode(`aurora-curseforge-download\0${source}`),
        { name: "HMAC", hash: "SHA-256" },
        false,
        ["sign"],
      ),
    };
  }
  const key = await curseForgeSigningKey.value;
  return toBase64Url(new Uint8Array(await crypto.subtle.sign("HMAC", key, textEncoder.encode(payload))));
}

async function createCurseForgeDownloadUrl(
  request: Request,
  env: Env,
  modId: number,
  fileId: number,
  downloadUrl: string,
): Promise<string> {
  if (!validCurseForgeCdnUrl(downloadUrl)) {
    throw new PublicError("O CurseForge retornou uma URL de download inválida.", 502);
  }
  const payload = toBase64Url(textEncoder.encode(JSON.stringify({
    modId,
    fileId,
    downloadUrl,
    expiresAt: Date.now() + curseForgeDownloadTicketLifetimeMs,
  })));
  const signature = await downloadTicketSignature(env, payload);
  const url = new URL("/v1/curseforge/download", request.url);
  url.searchParams.set("ticket", `${payload}.${signature}`);
  return url.toString();
}

async function readCurseForgeDownloadTicket(
  request: Request,
  env: Env,
): Promise<{ modId: number; fileId: number; ticketId: string; expiresAt: number; downloadUrl?: string }> {
  const ticket = new URL(request.url).searchParams.get("ticket") ?? "";
  const [payload, signature, extra] = ticket.split(".");
  if (!payload || !signature || extra || ticket.length > 1_000) {
    throw new PublicError("Este link de download do CurseForge é inválido.", 403);
  }
  const expected = await downloadTicketSignature(env, payload);
  const suppliedBytes = textEncoder.encode(signature);
  const expectedBytes = textEncoder.encode(expected);
  if (suppliedBytes.length !== expectedBytes.length) {
    throw new PublicError("Este link de download do CurseForge é inválido.", 403);
  }
  let different = 0;
  for (let index = 0; index < suppliedBytes.length; index += 1) different |= suppliedBytes[index] ^ expectedBytes[index];
  if (different !== 0) throw new PublicError("Este link de download do CurseForge é inválido.", 403);
  let decoded: { modId?: unknown; fileId?: unknown; downloadUrl?: unknown; expiresAt?: unknown };
  try {
    decoded = JSON.parse(textDecoder.decode(fromBase64Url(payload))) as typeof decoded;
  } catch {
    throw new PublicError("Este link de download do CurseForge é inválido.", 403);
  }
  if (typeof decoded.modId !== "number" || !Number.isInteger(decoded.modId) || decoded.modId <= 0
    || typeof decoded.fileId !== "number" || !Number.isInteger(decoded.fileId) || decoded.fileId <= 0) {
    throw new PublicError("Este link de download do CurseForge é inválido.", 403);
  }
  const modId = decoded.modId;
  const fileId = decoded.fileId;
  if (typeof decoded.expiresAt !== "number" || !Number.isFinite(decoded.expiresAt)
    || decoded.expiresAt < Date.now() || decoded.expiresAt > Date.now() + curseForgeDownloadTicketLifetimeMs + 60_000) {
    throw new PublicError("Este link de download do CurseForge expirou. Inicie o download novamente.", 410);
  }
  if (decoded.downloadUrl !== undefined && !validCurseForgeCdnUrl(decoded.downloadUrl)) {
    throw new PublicError("Este link de download do CurseForge é inválido.", 403);
  }
  return {
    modId,
    fileId,
    ticketId: signature,
    expiresAt: decoded.expiresAt,
    downloadUrl: decoded.downloadUrl as string | undefined,
  };
}

function enforceDownloadTicketUse(ticketId: string, expiresAt: number): void {
  if (curseForgeTicketUses.size >= 4_096) {
    for (const [key, usage] of curseForgeTicketUses) {
      if (usage.expiresAt <= Date.now()) curseForgeTicketUses.delete(key);
    }
    while (curseForgeTicketUses.size >= 4_096) {
      const oldestKey = curseForgeTicketUses.keys().next().value as string | undefined;
      if (!oldestKey) break;
      curseForgeTicketUses.delete(oldestKey);
    }
  }
  const usage = curseForgeTicketUses.get(ticketId);
  if (usage && usage.expiresAt > Date.now()) {
    if (usage.count >= 16) {
      throw new PublicError("Este link de download do CurseForge atingiu o limite de tentativas.", 429);
    }
    usage.count += 1;
    return;
  }
  curseForgeTicketUses.set(ticketId, { count: 1, expiresAt });
}

function validCurseForgeCdnUrl(value: unknown): value is string {
  if (typeof value !== "string") return false;
  try {
    const url = new URL(value);
    return url.protocol === "https:"
      && !url.username
      && !url.password
      && (!url.port || url.port === "443")
      && (url.hostname === "forgecdn.net" || url.hostname.endsWith(".forgecdn.net"));
  } catch {
    return false;
  }
}

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

function enforceRateLimit(userId: string, maximum = 20): void {
  const now = Date.now();
  if (windows.size >= 4_096) {
    for (const [key, window] of windows) {
      if (now - window.startedAt >= 60_000) windows.delete(key);
    }
    while (windows.size >= 4_096) {
      const oldestKey = windows.keys().next().value as string | undefined;
      if (!oldestKey) break;
      windows.delete(oldestKey);
    }
  }
  const current = windows.get(userId);
  if (!current || now - current.startedAt >= 60_000) {
    windows.set(userId, { startedAt: now, count: 1 });
    return;
  }
  if (current.count >= maximum) {
    throw new PublicError("Muitas solicitações. Aguarde um minuto e tente novamente.", 429);
  }
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

async function protectCurseForgeFileUrl(
  request: Request,
  env: Env,
  file: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const modId = typeof file.modId === "number" ? file.modId : 0;
  const fileId = typeof file.id === "number" ? file.id : 0;
  if (!modId || !fileId || !validCurseForgeCdnUrl(file.downloadUrl)) {
    const sanitized = { ...file };
    delete sanitized.downloadUrl;
    return sanitized;
  }
  return {
    ...file,
    downloadUrl: await createCurseForgeDownloadUrl(request, env, modId, fileId, file.downloadUrl),
  };
}

async function resolveCurseForgeDownload(
  env: Env,
  modId: number,
  fileId: number,
): Promise<string> {
  const cacheKey = `${modId}:${fileId}`;
  const cached = curseForgeResolvedDownloads.get(cacheKey);
  if (cached && cached.expiresAt > Date.now()) return cached.url;
  curseForgeResolvedDownloads.delete(cacheKey);
  const fileResponse = await requireCurseForge(
    env,
    `/mods/${modId}/files/${fileId}`,
    "resolve-download-file",
  );
  const filePayload = await readCurseForgeJson<{ data?: Record<string, unknown> }>(
    fileResponse,
    "resolve-download-file",
  );
  if (!filePayload.data || typeof filePayload.data !== "object" || Array.isArray(filePayload.data)) {
    throw invalidCurseForgeResponse();
  }
  if (filePayload.data?.id !== fileId || filePayload.data?.modId !== modId) {
    throw new PublicError("O arquivo solicitado não pertence a este projeto.", 404);
  }
  let downloadUrl = filePayload.data.downloadUrl;
  if (!validCurseForgeCdnUrl(downloadUrl)) {
    const urlResponse = await requireCurseForge(
      env,
      `/mods/${modId}/files/${fileId}/download-url`,
      "resolve-download-url",
    );
    const urlPayload = await readCurseForgeJson<{ data?: unknown }>(urlResponse, "resolve-download-url");
    downloadUrl = urlPayload.data;
  }
  if (!validCurseForgeCdnUrl(downloadUrl)) {
    throw new PublicError("Este autor exige que o download seja feito pela página oficial do CurseForge.", 409);
  }
  if (curseForgeResolvedDownloads.size >= 1_000) {
    for (const [key, value] of curseForgeResolvedDownloads) {
      if (value.expiresAt <= Date.now()) curseForgeResolvedDownloads.delete(key);
    }
    if (curseForgeResolvedDownloads.size >= 1_000) {
      curseForgeResolvedDownloads.delete(curseForgeResolvedDownloads.keys().next().value as string);
    }
  }
  curseForgeResolvedDownloads.set(cacheKey, { expiresAt: Date.now() + 5 * 60_000, url: downloadUrl });
  return downloadUrl;
}

async function fetchCurseForgeCdn(url: string, headers: Record<string, string>): Promise<Response> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 30_000);
  try {
    return await fetch(url, {
      headers,
      redirect: "manual",
      signal: controller.signal,
    });
  } finally {
    clearTimeout(timeout);
  }
}

function curseForgeDownloadHeaders(response: Response): Headers {
  const headers = new Headers({
    "Access-Control-Allow-Origin": "*",
    "Cache-Control": "private, no-store",
    "Content-Type": response.headers.get("Content-Type") ?? "application/octet-stream",
    "X-Content-Type-Options": "nosniff",
  });
  for (const name of ["Accept-Ranges", "Content-Disposition", "Content-Length", "Content-Range", "ETag", "Last-Modified"]) {
    const value = response.headers.get(name);
    if (value) headers.set(name, value);
  }
  return headers;
}

async function curseForgeDownload(request: Request, env: Env): Promise<Response> {
  if (!curseForgeApiKey(env)) return error("O catálogo CurseForge ainda não foi configurado.", 503);
  try {
    const { modId, fileId, ticketId, expiresAt, downloadUrl } = await readCurseForgeDownloadTicket(request, env);
    enforceDownloadTicketUse(ticketId, expiresAt);
    enforceRateLimit(`curseforge-download-ticket:${ticketId}`, 8);
    enforceRateLimit(`curseforge-download-ip:${request.headers.get("CF-Connecting-IP") ?? "unknown"}`, 1_024);
    const requestedRange = request.headers.get("Range");
    if (requestedRange && !/^bytes=\d+-\d*$/.test(requestedRange)) {
      throw new PublicError("O intervalo solicitado para o download é inválido.", 416);
    }
    let url = downloadUrl ?? await resolveCurseForgeDownload(env, modId, fileId);
    let response: Response | undefined;
    for (let redirect = 0; redirect < 4; redirect += 1) {
      const parsed = new URL(url);
      const forgeCdn = parsed.hostname === "forgecdn.net" || parsed.hostname.endsWith(".forgecdn.net");
      response = await fetchCurseForgeCdn(url, {
        ...(forgeCdn ? { "x-api-key": curseForgeApiKey(env) } : {}),
        ...(requestedRange ? { Range: requestedRange } : {}),
      });
      if (response.status < 300 || response.status >= 400) break;
      const location = response.headers.get("Location");
      if (!location) break;
      await response.body?.cancel();
      const redirected = new URL(location, url);
      if (!validCurseForgeCdnUrl(redirected.toString())) {
        throw new PublicError("O CDN do CurseForge retornou um redirecionamento inseguro.", 502);
      }
      url = redirected.toString();
    }
    if (requestedRange && response?.status === 404) {
      await response.body?.cancel();
      for (let redirect = 0; redirect < 4; redirect += 1) {
        const parsed = new URL(url);
        const forgeCdn = parsed.hostname === "forgecdn.net" || parsed.hostname.endsWith(".forgecdn.net");
        response = await fetchCurseForgeCdn(url, forgeCdn ? { "x-api-key": curseForgeApiKey(env) } : {});
        if (response.status < 300 || response.status >= 400) break;
        const location = response.headers.get("Location");
        if (!location) break;
        await response.body?.cancel();
        const redirected = new URL(location, url);
        if (!validCurseForgeCdnUrl(redirected.toString())) {
          throw new PublicError("O CDN do CurseForge retornou um redirecionamento inseguro.", 502);
        }
        url = redirected.toString();
      }
    }
    if (response?.status === 416) {
      return new Response(response.body, {
        status: 416,
        headers: curseForgeDownloadHeaders(response),
      });
    }
    if (!response?.ok || !response.body) {
      console.error("CurseForge CDN failure", { status: response?.status ?? 0, modId, fileId });
      await response?.body?.cancel();
      if (response?.status === 401 || response?.status === 403) {
        throw new PublicError("O CurseForge não autorizou este download. Tente novamente mais tarde.", 503);
      }
      throw new PublicError("O arquivo do CurseForge não está disponível agora.", 503);
    }
    return new Response(response.body, {
      status: response.status,
      headers: curseForgeDownloadHeaders(response),
    });
  } catch (cause) {
    const publicError = cause instanceof PublicError
      ? cause
      : new PublicError("Não foi possível iniciar o download pelo CurseForge.", 503);
    return error(publicError.message, publicError.status);
  }
}

async function probeCurseForgeHealth(env: Env): Promise<Response> {
  if (!curseForgeApiKey(env)) return error("A chave do catálogo CurseForge não está configurada.", 503);
  try {
    const searchQuery = new URLSearchParams({
      gameId: String(minecraftGameId),
      classId: String(curseForgeClassIds.mod),
      gameVersion: "1.20.1",
      pageSize: "1",
      sortField: "6",
      sortOrder: "desc",
    });
    const searchResponse = await requireCurseForge(env, `/mods/search?${searchQuery}`, "health-search");
    const searchPayload = await searchResponse.json().catch(() => null) as { data?: unknown[] } | null;
    if (!Array.isArray(searchPayload?.data)) {
      throw new PublicError("O CurseForge retornou uma busca inválida.", 503);
    }
    const categoryQuery = new URLSearchParams({
      gameId: String(minecraftGameId),
      classId: String(curseForgeClassIds.mod),
    });
    const categoryResponse = await requireCurseForge(env, `/categories?${categoryQuery}`, "health-categories");
    const categoryPayload = await categoryResponse.json().catch(() => null) as { data?: unknown[] } | null;
    if (!Array.isArray(categoryPayload?.data) || categoryPayload.data.length === 0) {
      throw new PublicError("O CurseForge não retornou as categorias do Minecraft.", 503);
    }
    return json({
      status: "ok",
      service: "aurora-curseforge",
      searchResults: searchPayload.data.length,
      categories: categoryPayload.data.length,
    });
  } catch (cause) {
    const publicError = cause instanceof PublicError
      ? cause
      : new PublicError("O catálogo CurseForge não está disponível agora.", 503);
    return error(publicError.message, publicError.status);
  }
}

function healthSnapshotResponse(snapshot: HealthSnapshot): Response {
  return new Response(snapshot.body, {
    status: snapshot.status,
    headers: {
      "Access-Control-Allow-Origin": "*",
      "Cache-Control": "public, max-age=60",
      "Content-Type": "application/json; charset=utf-8",
    },
  });
}

async function curseForgeHealth(env: Env): Promise<Response> {
  if (curseForgeHealthSnapshot && curseForgeHealthSnapshot.expiresAt > Date.now()) {
    return healthSnapshotResponse(curseForgeHealthSnapshot);
  }
  if (!curseForgeHealthPending) {
    curseForgeHealthPending = (async () => {
      const response = await probeCurseForgeHealth(env);
      const snapshot = {
        body: await response.text(),
        expiresAt: Date.now() + 60_000,
        status: response.status,
      };
      curseForgeHealthSnapshot = snapshot;
      return snapshot;
    })();
  }
  try {
    return healthSnapshotResponse(await curseForgeHealthPending);
  } finally {
    curseForgeHealthPending = undefined;
  }
}

async function curseForgeCatalog(request: Request, env: Env): Promise<Response> {
  if (!curseForgeApiKey(env)) return error("O catálogo CurseForge ainda não foi configurado.", 503);
  const body = await readBody(request);
  const action = boundedText(body.action, 20, "Ação");
  let path = "";
  const query = new URLSearchParams();
  if (action === "search") {
    const pageSize = body.pageSize ?? 20;
    const index = body.index ?? 0;
    if (!Number.isInteger(pageSize) || (pageSize as number) < 1 || (pageSize as number) > 50
      || !Number.isInteger(index) || (index as number) < 0 || (index as number) + (pageSize as number) > 10_000) {
      throw new Error("Paginação inválida.");
    }
    path = "/mods/search";
    const contentType = boundedText(body.contentType ?? "modpack", 20, "Tipo") as keyof typeof curseForgeClassIds;
    const classId = curseForgeClassIds[contentType];
    if (!classId) throw new Error("Tipo de conteúdo inválido.");
    query.set("gameId", String(minecraftGameId));
    query.set("classId", String(classId));
    if (body.categoryId !== undefined && body.categoryId !== null) {
      query.set("categoryId", String(positiveInteger(body.categoryId, "Categoria")));
    }
    const search = optionalBoundedText(body.query, 120, "Busca");
    if (search) query.set("searchFilter", search);
    const gameVersion = optionalBoundedText(body.gameVersion, 30, "Versão do Minecraft");
    if (gameVersion) query.set("gameVersion", gameVersion);
    const loader = optionalBoundedText(body.loader, 10, "Loader") as keyof typeof curseForgeLoaderIds | "";
    if ((contentType === "mod" || contentType === "modpack") && gameVersion && loader && curseForgeLoaderIds[loader]) {
      query.set("modLoaderType", String(curseForgeLoaderIds[loader]));
    } else if ((contentType === "mod" || contentType === "modpack") && gameVersion) {
      query.set("modLoaderTypes", "[1,4]");
    }
    const sort = optionalBoundedText(body.sort, 20, "Ordenação");
    if (sort === "popular") query.set("sortField", "6");
    if (sort === "updated") query.set("sortField", "3");
    query.set("sortOrder", "desc");
    query.set("pageSize", String(pageSize));
    query.set("index", String(index));
  } else if (action === "categories") {
    path = "/categories";
    const contentType = boundedText(body.contentType, 20, "Tipo") as keyof typeof curseForgeClassIds;
    const classId = curseForgeClassIds[contentType];
    if (!classId) throw new Error("Tipo de conteúdo inválido.");
    query.set("gameId", String(minecraftGameId));
    query.set("classId", String(classId));
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
  } else if (action === "resolveProjects") {
    if (!Array.isArray(body.projectIds) || body.projectIds.length === 0 || body.projectIds.length > 32) {
      throw new Error("Lista de projetos inválida.");
    }
    const projectIds = body.projectIds.map((id) => positiveInteger(id, "Projeto"));
    if (new Set(projectIds).size !== projectIds.length) throw new Error("A lista contém projetos duplicados.");
    const gameVersion = boundedText(body.gameVersion, 30, "Versão do Minecraft");
    const loader = optionalBoundedText(body.loader, 10, "Loader") as keyof typeof curseForgeLoaderIds | "";
    if (loader && !curseForgeLoaderIds[loader]) throw new Error("Loader inválido.");
    const projectsResponse = await requireCurseForge(env, "/mods", "resolve-projects", {
      method: "POST",
      body: JSON.stringify({ modIds: projectIds, filterPcOnly: true }),
    });
    const projectPayload = await readCurseForgeJson<{ data?: Array<Record<string, unknown>> }>(
      projectsResponse,
      "resolve-projects",
    );
    if (!Array.isArray(projectPayload.data)) throw invalidCurseForgeResponse();
    const projects = projectPayload.data.filter((project) => typeof project.id === "number" && projectIds.includes(project.id as number));
    const resolved = await mapWithConcurrency(projects, 6, async (project) => {
      const parameters = new URLSearchParams({ gameVersion, pageSize: "50" });
      if (project.classId === curseForgeClassIds.mod && loader) {
        parameters.set("modLoaderType", String(curseForgeLoaderIds[loader]));
      }
      const response = await requireCurseForge(
        env,
        `/mods/${project.id}/files?${parameters}`,
        "resolve-project-files",
      );
      const payload = await readCurseForgeJson<{ data?: Array<Record<string, unknown>> }>(
        response,
        "resolve-project-files",
      );
      if (!Array.isArray(payload.data)) throw invalidCurseForgeResponse();
      const files = await mapWithConcurrency(
        payload.data,
        8,
        (file) => protectCurseForgeFileUrl(request, env, file),
      );
      return { project, files };
    });
    return json({ data: resolved });
  } else if (action === "resolveFiles") {
    if (!Array.isArray(body.files) || body.files.length === 0 || body.files.length > 512) {
      throw new Error("Lista de arquivos inválida.");
    }
    const references = body.files.map((entry) => {
      if (!entry || typeof entry !== "object" || Array.isArray(entry)) throw new Error("Referência de arquivo inválida.");
      const value = entry as Record<string, unknown>;
      return {
        projectId: positiveInteger(value.projectId, "Projeto"),
        fileId: positiveInteger(value.fileId, "Arquivo"),
      };
    });
    const expectedProjects = new Map(references.map((entry) => [entry.fileId, entry.projectId]));
    if (expectedProjects.size !== references.length) throw new Error("A lista contém arquivos duplicados.");
    const fileIds = references.map((entry) => entry.fileId);
    const fileChunks = Array.from({ length: Math.ceil(fileIds.length / 50) }, (_, index) => index);
    const fileResponses = await mapWithConcurrency(fileChunks, 6, async (index) => {
      const response = await requireCurseForge(env, "/mods/files", "resolve-files", {
        method: "POST",
        body: JSON.stringify({ fileIds: fileIds.slice(index * 50, index * 50 + 50) }),
      });
      const payload = await readCurseForgeJson<{ data?: Array<Record<string, unknown>> }>(response, "resolve-files");
      if (!Array.isArray(payload.data)) throw invalidCurseForgeResponse();
      return payload as { data: Array<Record<string, unknown>> };
    });
    const files = fileResponses.flatMap((response) => response.data ?? []).filter((file) => {
      const fileId = typeof file.id === "number" ? file.id : 0;
      const modId = typeof file.modId === "number" ? file.modId : 0;
      return expectedProjects.get(fileId) === modId;
    });
    if (files.length !== references.length) throw new Error("O CurseForge não retornou todos os arquivos do modpack.");

    const missingDownloadIds = new Set(files
      .filter((file) => !validCurseForgeCdnUrl(file.downloadUrl))
      .slice(0, 12)
      .map((file) => file.id as number));
    const filesWithUrls = await mapWithConcurrency(files, 6, async (file) => {
      if (validCurseForgeCdnUrl(file.downloadUrl)) return file;
      if (!missingDownloadIds.has(file.id as number)) return file;
      const modId = file.modId as number;
      const fileId = file.id as number;
      const response = await fetchCurseForge(env, `/mods/${modId}/files/${fileId}/download-url`);
      if (!response.ok) {
        if (response.status === 403 || response.status === 404) return file;
        throw await curseForgeFailure(response, "resolve-file-download-url");
      }
      const payload = await readCurseForgeJson<{ data?: string }>(response, "resolve-file-download-url");
      return validCurseForgeCdnUrl(payload.data)
        ? { ...file, downloadUrl: payload.data }
        : file;
    });
    const projectIds = [...new Set(references.map((entry) => entry.projectId))];
    const projectChunks = Array.from({ length: Math.ceil(projectIds.length / 50) }, (_, index) => index);
    const projectResponses = await mapWithConcurrency(projectChunks, 6, async (index) => {
      const response = await requireCurseForge(env, "/mods", "resolve-file-projects", {
        method: "POST",
        body: JSON.stringify({ modIds: projectIds.slice(index * 50, index * 50 + 50), filterPcOnly: true }),
      });
      const payload = await readCurseForgeJson<{ data?: Array<Record<string, unknown>> }>(response, "resolve-file-projects");
      if (!Array.isArray(payload.data)) throw invalidCurseForgeResponse();
      return payload as { data: Array<Record<string, unknown>> };
    });
    const projects = new Map(projectResponses
      .flatMap((response) => response.data ?? [])
      .filter((project) => typeof project.id === "number")
      .map((project) => [project.id as number, project]));
    const filesWithProjects = filesWithUrls.map((file) => ({
        ...file,
        project: projects.get(file.modId as number),
      }));
    return json({
      data: await mapWithConcurrency(
        filesWithProjects,
        16,
        (file) => protectCurseForgeFileUrl(request, env, file),
      ),
    });
  } else {
    throw new Error("Ação de catálogo não permitida.");
  }
  const response = await requireCurseForge(
    env,
    path + (query.size ? "?" + query.toString() : ""),
    action,
  );
  const payload = await readCurseForgeJson<{ data?: unknown; pagination?: unknown }>(response, action);
  if (action === "categories") {
    if (!Array.isArray(payload.data)) throw invalidCurseForgeResponse();
    const categories = payload.data
      .filter((entry): entry is Record<string, unknown> => Boolean(entry) && typeof entry === "object" && !Array.isArray(entry));
    const byId = new Map(categories
      .filter((category) => typeof category.id === "number")
      .map((category) => [category.id as number, category]));
    return json({
      data: categories
        .filter((category) => category.isClass !== true && typeof category.id === "number" && typeof category.name === "string")
        .map((category) => {
          const parent = typeof category.parentCategoryId === "number" ? byId.get(category.parentCategoryId) : undefined;
          return {
            id: category.id,
            name: category.name,
            slug: typeof category.slug === "string" ? category.slug : "",
            group: typeof parent?.name === "string" ? parent.name : "Categorias",
            displayIndex: typeof category.displayIndex === "number" ? category.displayIndex : 0,
          };
        })
        .sort((left, right) => left.displayIndex - right.displayIndex || left.name.localeCompare(right.name)),
    });
  }
  if (action === "files") {
    if (!Array.isArray(payload.data)) throw invalidCurseForgeResponse();
    const files = payload.data.filter((entry): entry is Record<string, unknown> => Boolean(entry) && typeof entry === "object" && !Array.isArray(entry));
    return json({
      ...payload,
      data: await mapWithConcurrency(files, 16, (file) => protectCurseForgeFileUrl(request, env, file)),
    });
  }
  if (action === "file") {
    if (!payload.data || typeof payload.data !== "object" || Array.isArray(payload.data)) {
      throw invalidCurseForgeResponse();
    }
    return json({ ...payload, data: await protectCurseForgeFileUrl(request, env, payload.data as Record<string, unknown>) });
  }
  if (action === "download") {
    if (!validCurseForgeCdnUrl(payload.data)) throw new PublicError("Este autor exige a página oficial do CurseForge.", 409);
    const modId = positiveInteger(body.modId, "Projeto");
    const fileId = positiveInteger(body.fileId, "Arquivo");
    return json({ data: await createCurseForgeDownloadUrl(request, env, modId, fileId, payload.data) });
  }
  if (action === "search" && !Array.isArray(payload.data)) throw invalidCurseForgeResponse();
  if (action === "mod" && (!payload.data || typeof payload.data !== "object" || Array.isArray(payload.data))) {
    throw invalidCurseForgeResponse();
  }
  if (action === "description" && typeof payload.data !== "string") throw invalidCurseForgeResponse();
  return json(payload);
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
    if (request.method === "GET" && url.pathname === "/health") {
      return json({ status: "ok", service: "aurora-api", revision: auroraApiRevision });
    }
    if (request.method === "GET" && url.pathname === "/health/storage") return await appearanceStorageHealth(env);
    if (request.method === "GET" && url.pathname === "/health/curseforge") {
      try {
        enforceRateLimit(`curseforge-health:${request.headers.get("CF-Connecting-IP") ?? "unknown"}`);
        return await curseForgeHealth(env);
      } catch (cause) {
        const publicError = cause instanceof PublicError
          ? cause
          : new PublicError("Não foi possível verificar o catálogo CurseForge.", 503);
        return error(publicError.message, publicError.status);
      }
    }
    if (request.method === "GET" && url.pathname === "/v1/curseforge/download") return await curseForgeDownload(request, env);
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
      const status = cause instanceof PublicError
        ? cause.status
        : /sessão|login/i.test(message)
          ? 401
          : /Muitas solicitações/i.test(message)
            ? 429
            : 400;
      return error(message, status);
    }
  },
} satisfies ExportedHandler<Env>;
