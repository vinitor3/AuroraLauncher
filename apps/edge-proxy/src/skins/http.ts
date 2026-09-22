export class SkinApiError extends Error {
  readonly status: number;
  readonly retryAfter: number;
  constructor(status: number, message: string, retryAfter = 0) { super(message); this.status = status; this.retryAfter = retryAfter; }
}

export async function boundedBytes(response: Response, maximum: number): Promise<Uint8Array> {
  if (Number(response.headers.get("Content-Length")) > maximum) {
    await response.body?.cancel();
    throw new SkinApiError(502, "Resposta de skins muito grande.");
  }
  const reader = response.body?.getReader();
  if (!reader) throw new SkinApiError(502, "Resposta de skins vazia.");
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    while (true) {
      const next = await reader.read();
      if (next.done) break;
      length += next.value.length;
      if (length > maximum) {
        await reader.cancel();
        throw new SkinApiError(502, "Resposta de skins muito grande.");
      }
      chunks.push(next.value);
    }
  } finally { reader.releaseLock(); }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return bytes;
}

export class JsonHttpClient {
  private cooldownUntil = 0;
  private cooldownStatus: 429 | 503 = 429;
  private windowStart = 0;
  private requests = 0;
  private readonly base: string;
  private readonly apiKey: string;
  private readonly perMinute: number;
  private readonly timeoutMs: number;
  private readonly fetcher: typeof fetch;
  private readonly now: () => number;
  constructor(base: string, apiKey = "", perMinute = 30, timeoutMs = 3000,
    fetcher: typeof fetch = (...args) => fetch(...args), now: () => number = Date.now) {
    this.base = base; this.apiKey = apiKey; this.perMinute = perMinute;
    this.timeoutMs = timeoutMs; this.fetcher = fetcher; this.now = now;
  }

  async response(path: string, accept = "application/json", deadline?: AbortSignal): Promise<Response> {
    const now = this.now();
    if (now < this.cooldownUntil) throw new SkinApiError(this.cooldownStatus,
      this.cooldownStatus === 429 ? "Aguarde para consultar skins novamente." : "A fonte de skins está temporariamente indisponível.",
      this.cooldownStatus === 429 ? Math.ceil((this.cooldownUntil - now) / 1000) : 0);
    if (now - this.windowStart >= 60000) { this.requests = 0; this.windowStart = now; }
    if (++this.requests > this.perMinute) throw new SkinApiError(429, "Limite temporário da busca de skins.", 60);
    // One attempt: retries inside an autocomplete multiply bursts. The next user query/cache refresh retries.
    let response: Response;
    try {
      response = await this.fetcher(this.base + path, {
        headers: { Accept: accept, ...(this.apiKey.trim() ? { "X-API-Key": this.apiKey.trim() } : {}) },
        signal: deadline ? AbortSignal.any([deadline, AbortSignal.timeout(this.timeoutMs)]) : AbortSignal.timeout(this.timeoutMs), redirect: "manual",
      });
      if (!response.ok) {
        const seconds = Number(response.headers.get("Retry-After"));
        const date = Date.parse(response.headers.get("Retry-After") ?? "");
        const retryAfter = Number.isFinite(seconds) && seconds > 0 ? seconds : Number.isFinite(date) ? (date - now) / 1000 : 60;
        if (response.status === 429) {
          this.cooldownUntil = now + Math.max(1, Math.min(86400, retryAfter)) * 1000;
          this.cooldownStatus = 429;
        }
        if (response.status === 401 || response.status === 403) { this.cooldownUntil = now + 60000; this.cooldownStatus = 503; }
        await response.body?.cancel();
        throw new SkinApiError(response.status === 404 ? 404 : response.status === 429 ? 429 : 503,
          response.status === 404 ? "Jogador não encontrado." : "A fonte de skins está temporariamente indisponível.",
          response.status === 429 ? Math.ceil((this.cooldownUntil - now) / 1000) : 0);
      }
      return response;
    } catch (error) {
      if (error instanceof SkinApiError) throw error;
      throw new SkinApiError(503, "Não foi possível consultar a fonte de skins agora.");
    }
  }

  async get<T>(path: string, deadline?: AbortSignal): Promise<{ data: T; lastKnown: boolean }> {
    try {
      const response = await this.response(path, "application/json", deadline);
      if (!(response.headers.get("Content-Type") ?? "").includes("application/json")) {
        await response.body?.cancel();
        throw new SkinApiError(502, "Resposta inválida da fonte de skins.");
      }
      const bytes = await boundedBytes(response, 256 * 1024);
      const data = JSON.parse(new TextDecoder().decode(bytes)) as T;
      return { data, lastKnown: response.headers.get("X-Data-Origin") === "last-known" };
    } catch (error) {
      if (error instanceof SkinApiError) throw error;
      throw new SkinApiError(503, "Não foi possível consultar a fonte de skins agora.");
    }
  }
}
