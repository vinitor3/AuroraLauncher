import { McApiClient, MC_API_BASE_URL, rankPlayers } from "./McApiClient.ts";
import { JsonHttpClient, SkinApiError } from "./http.ts";
import { object } from "./dtos.ts";
import { normalizeUuid, skinImageUrl, validPlayerName, getSkinImageUrlFromProfile,
  type PlayerSkinEnv, type PlayerSuggestion, type SkinResult } from "./models.ts";

type CacheEntry = { value: unknown; expires: number; staleUntil: number };
export class SkinService {
  private readonly cache = new Map<string, CacheEntry>();
  private readonly pending = new Map<string, Promise<unknown>>();
  private readonly client: McApiClient;
  private readonly mcstat?: JsonHttpClient;
  private readonly now: () => number;
  constructor(env: PlayerSkinEnv, now: () => number = Date.now,
    fetcher: typeof fetch = (...args) => fetch(...args)) {
    this.now = now;
    const rate = Math.min(120, Math.max(1, Number(env.SKIN_API_REQUESTS_PER_MINUTE) || 30));
    this.client = new McApiClient(new JsonHttpClient(MC_API_BASE_URL, env.MC_API_KEY, rate, 3000, fetcher, now));
    const key = env.mcstat_API_KEY ?? env.MCSTAT_API_KEY;
    if (key?.trim()) this.mcstat = new JsonHttpClient("https://mcstat.org/api/v1", key, rate, 2500, fetcher, now);
  }

  private async cached<T>(key: string, lifetime: number, operation: () => Promise<T>): Promise<T> {
    const cached = this.cache.get(key);
    if (cached && cached.expires > this.now()) { console.info("[Skins] cache hit", { operation: key.split(":")[0] }); return cached.value as T; }
    const pending = this.pending.get(key);
    if (pending) return pending as Promise<T>;
    console.info("[Skins] cache miss", { operation: key.split(":")[0] });
    const promise = (async () => {
      try {
        const value = await operation();
        if (this.cache.size >= 256) this.cache.delete(this.cache.keys().next().value!);
        this.cache.set(key, { value, expires: this.now() + lifetime, staleUntil: this.now() + 3600000 });
        console.info("[Skins] lookup success", { operation: key.split(":")[0] });
        return value;
      } catch (cause) {
        console.warn("[Skins] lookup failed", { operation: key.split(":")[0], status: cause instanceof SkinApiError ? cause.status : 502 });
        if (cached && cached.staleUntil > this.now()) {
          return (key.startsWith("profile:") ? { ...(cached.value as SkinResult), stale: true } : cached.value) as T;
        }
        throw cause;
      } finally { this.pending.delete(key); }
    })();
    this.pending.set(key, promise);
    return promise;
  }

  searchPlayers(query: string): Promise<PlayerSuggestion[]> {
    const clean = query.trim();
    if (!/^[A-Za-z0-9_]{3,16}$/.test(clean)) return Promise.resolve([]);
    return this.cached(`search:${clean.toLowerCase()}`, 60000, async () => {
      try { return await this.client.searchPlayers(clean); }
      catch (cause) {
        if (!this.mcstat) throw cause;
        const { data } = await this.mcstat.get<unknown>(`/players?search=${encodeURIComponent(clean)}&limit=12&page=1`);
        const players = object(object(data).data).players;
        if (!Array.isArray(players)) throw new SkinApiError(502, "Lista de jogadores inválida.");
        return players.slice(0, 12).flatMap((candidate): PlayerSuggestion[] => {
          const row = object(candidate);
          const uuid = normalizeUuid(row.minecraftUuid);
          return validPlayerName(row.mainNickname) && row.mainNickname.toLowerCase().startsWith(clean.toLowerCase())
            ? [{ name: row.mainNickname, ...(uuid ? { uuid } : {}), source: "mcstat", metadata: { ...row } }] : [];
        });
      }
    });
  }

  getPlayerProfile(identifier: string, includeHistory = false): Promise<SkinResult> {
    const id = identifier.trim();
    if (!validPlayerName(id) && !normalizeUuid(id)) return Promise.reject(new SkinApiError(400, "Informe um nome ou UUID válido."));
    return this.cached(`profile:${normalizeUuid(id) ?? id.toLowerCase()}:${includeHistory}`, 300000, async () => {
      try { return await this.client.getPlayerProfile(id, includeHistory); }
      catch (cause) {
        if (!this.mcstat) throw cause;
        const { data } = await this.mcstat.get<unknown>(`/players/${encodeURIComponent(id)}`);
        const row = object(object(object(data).data).player);
        const uuid = normalizeUuid(row.minecraftUuid);
        const textureUrl = skinImageUrl(row.linkedSkinPngPath) ?? skinImageUrl(row.skinUrl);
        if (!uuid || !validPlayerName(row.mainNickname) || !textureUrl || (normalizeUuid(id) && normalizeUuid(id) !== uuid)) {
          throw new SkinApiError(502, "Perfil de jogador inválido.");
        }
        return { uuid, name: row.mainNickname, textureUrl, skinUrl: textureUrl,
          model: row.skinModel === "slim" ? "slim" : "classic", source: "mcstat", stale: false };
      }
    });
  }

  async autocompleteSkinNames(query: string): Promise<PlayerSuggestion[]> {
    return rankPlayers(await this.searchPlayers(query), query);
  }
  getSkinImageUrlFromProfile = getSkinImageUrlFromProfile;
  getTextureResponse(source: string, deadline?: AbortSignal): Promise<Response> {
    return this.client.getTextureResponse(source, deadline);
  }
}
