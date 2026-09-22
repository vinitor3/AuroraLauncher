import { McApiClient, MC_API_BASE_URL, rankPlayers } from "./McApiClient.ts";
import { MojangClient } from "./MojangClient.ts";
import { JsonHttpClient, SkinApiError } from "./http.ts";
import { object } from "./dtos.ts";
import { normalizeUuid, skinImageUrl, validPlayerName, getSkinImageUrlFromProfile,
  type PlayerSkinEnv, type PlayerSuggestion, type SkinResult } from "./models.ts";

type CacheEntry = { value: unknown; expires: number; staleUntil: number };
export class SkinService {
  private readonly cache = new Map<string, CacheEntry>();
  private readonly pending = new Map<string, Promise<unknown>>();
  private readonly client: McApiClient;
  private readonly mojang: MojangClient;
  private readonly mcstat?: JsonHttpClient;
  private readonly now: () => number;
  constructor(env: PlayerSkinEnv, now: () => number = Date.now,
    fetcher: typeof fetch = (...args) => fetch(...args)) {
    this.now = now;
    const rate = Math.min(120, Math.max(1, Number(env.SKIN_API_REQUESTS_PER_MINUTE) || 30));
    this.client = new McApiClient(new JsonHttpClient(MC_API_BASE_URL, env.MC_API_KEY, rate, 3000, fetcher, now));
    this.mojang = new MojangClient(rate, fetcher, now);
    const key = env.mcstat_API_KEY ?? env.MCSTAT_API_KEY;
    if (key?.trim()) this.mcstat = new JsonHttpClient("https://mcstat.org/api/v1", key, rate, 2500, fetcher, now);
  }

  private async fallback<T>(cause: unknown, deadline: AbortSignal,
    official: (signal: AbortSignal) => Promise<T>, secondary?: (signal: AbortSignal) => Promise<T>): Promise<T> {
    // Rate limiting and invalid provider data remain failures. A missing entry in
    // a third-party index is not evidence that a Minecraft account does not exist.
    if (!(cause instanceof SkinApiError) || ![404, 503].includes(cause.status) || deadline.aborted) throw cause;
    const controller = new AbortController();
    const signal = AbortSignal.any([deadline, controller.signal]);
    try {
      return await Promise.any([official(signal), ...(secondary ? [secondary(signal)] : [])]);
    } catch (error) {
      const failures: unknown[] = error instanceof AggregateError ? error.errors : [error];
      const authoritative = failures[0];
      if (authoritative instanceof SkinApiError && [404, 429].includes(authoritative.status)) throw authoritative;
      throw new SkinApiError(503, "Não foi possível consultar as fontes de jogadores agora. Tente novamente em instantes.");
    } finally { controller.abort(); }
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
      const deadline = AbortSignal.timeout(7500);
      try { return await this.client.searchPlayers(clean, deadline); }
      catch (cause) {
        try {
          return await this.fallback(cause, deadline, async (signal) => [await this.mojang.findPlayer(clean, signal)],
            this.mcstat ? async (signal) => {
              const { data } = await this.mcstat!.get<unknown>(`/players?search=${encodeURIComponent(clean)}&limit=12&page=1`, signal);
              const players = object(object(data).data).players;
              if (!Array.isArray(players)) throw new SkinApiError(502, "Lista de jogadores inválida.");
              const matches = players.slice(0, 12).flatMap((candidate): PlayerSuggestion[] => {
                const row = object(candidate);
                const uuid = normalizeUuid(row.minecraftUuid);
                return validPlayerName(row.mainNickname) && row.mainNickname.toLowerCase().startsWith(clean.toLowerCase())
                  ? [{ name: row.mainNickname, ...(uuid ? { uuid } : {}), source: "mcstat", metadata: { ...row } }] : [];
              });
              // An empty index cannot cancel a still-pending exact official match.
              if (!matches.length) throw new SkinApiError(404, "Nenhuma sugestão neste índice.");
              return matches;
            } : undefined);
        } catch (error) {
          // Prefixes need not be complete names. An empty autocomplete must not
          // claim the player's account is missing or disable exact profile search.
          if (error instanceof SkinApiError && error.status === 404) return [];
          throw error;
        }
      }
    });
  }

  getPlayerProfile(identifier: string, includeHistory = false): Promise<SkinResult> {
    const id = identifier.trim();
    if (!validPlayerName(id) && !normalizeUuid(id)) return Promise.reject(new SkinApiError(400, "Informe um nome ou UUID válido."));
    return this.cached(`profile:${normalizeUuid(id) ?? id.toLowerCase()}:${includeHistory}`, 300000, async () => {
      const deadline = AbortSignal.timeout(7500);
      try { return await this.client.getPlayerProfile(id, includeHistory, deadline); }
      catch (cause) {
        return this.fallback(cause, deadline, (signal) => this.mojang.getPlayerProfile(id, signal),
          this.mcstat ? async (signal) => {
            const { data } = await this.mcstat!.get<unknown>(`/players/${encodeURIComponent(id)}`, signal);
            const row = object(object(object(data).data).player);
            const uuid = normalizeUuid(row.minecraftUuid);
            const textureUrl = skinImageUrl(row.linkedSkinPngPath) ?? skinImageUrl(row.skinUrl);
            if (!uuid || !validPlayerName(row.mainNickname) || !textureUrl || (normalizeUuid(id) && normalizeUuid(id) !== uuid)) {
              throw new SkinApiError(502, "Perfil de jogador inválido.");
            }
            return { uuid, name: row.mainNickname, textureUrl, skinUrl: textureUrl,
              model: row.skinModel === "slim" ? "slim" : "classic", source: "mcstat", stale: false };
          } : undefined);
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
