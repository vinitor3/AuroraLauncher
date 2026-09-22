import { JsonHttpClient, SkinApiError } from "./http.ts";
import { object, type McApiProfileDto, type McApiSearchDto, type McApiHistoryDto } from "./dtos.ts";
import { getSkinImageUrlFromProfile, normalizeUuid, skinImageUrl, validPlayerName,
  type PlayerSuggestion, type SkinResult, type SkinHistoryEntry } from "./models.ts";

export const MC_API_BASE_URL = "https://mc-api.bisai.dev";

export class McApiClient {
  private readonly http: JsonHttpClient;
  constructor(http = new JsonHttpClient(MC_API_BASE_URL)) { this.http = http; }

  async searchPlayers(query: string, deadline?: AbortSignal): Promise<PlayerSuggestion[]> {
    if (!/^[A-Za-z0-9_]{3,16}$/.test(query.trim())) return [];
    const { data } = await this.http.get<McApiSearchDto>(`/v1/players/search?q=${encodeURIComponent(query.trim())}&limit=12`, deadline);
    object(data);
    if (!Array.isArray(data.players) || data.players.length > 50) throw new SkinApiError(502, "Lista de jogadores inválida.");
    return data.players.flatMap((candidate) => {
      const row = object(candidate);
      if (!validPlayerName(row.name)) return [];
      const uuid = normalizeUuid(row.uuid);
      if (row.uuid != null && !uuid) return [];
      return [{ name: row.name, ...(uuid ? { uuid } : {}), source: "mc-api" as const, metadata: { ...row } }];
    });
  }

  async getPlayerProfile(identifier: string, includeHistory = false, deadline?: AbortSignal): Promise<SkinResult> {
    const id = identifier.trim();
    if (!validPlayerName(id) && !normalizeUuid(id)) throw new SkinApiError(400, "Informe um nome ou UUID válido.");
    const { data, lastKnown } = await this.http.get<McApiProfileDto>(`/v1/profile/${encodeURIComponent(id)}`, deadline);
    object(data);
    const uuid = normalizeUuid(data.uuid);
    const textureUrl = skinImageUrl(data.skin_url) ?? skinImageUrl(data.renders?.skin_png);
    if (!uuid || !validPlayerName(data.username) || !textureUrl || (normalizeUuid(id) && normalizeUuid(id) !== uuid)) {
      throw new SkinApiError(502, "Perfil de jogador inválido.");
    }
    const result: SkinResult = {
      name: data.username, uuid, model: data.is_slim === true ? "slim" : "classic",
      textureUrl, skinUrl: skinImageUrl(data.renders?.skin_png) ?? textureUrl, headUrl: skinImageUrl(data.renders?.head),
      bodyUrl: skinImageUrl(data.renders?.body), source: "mc-api", stale: lastKnown,
    };
    if (includeHistory) {
      try {
        const history = await this.http.get<McApiHistoryDto>(`/v1/profile/${uuid}/skins?limit=12`, deadline);
        if (Array.isArray(history.data.skins)) result.history = history.data.skins.slice(0, 12).flatMap((candidate): SkinHistoryEntry[] => {
          const row = object(candidate);
          const url = skinImageUrl(row.url);
          return url ? [{ textureUrl: url, model: row.slim === true ? "slim" : "classic",
            firstSeen: typeof row.first_seen === "string" ? row.first_seen : null,
            lastSeen: typeof row.last_seen === "string" ? row.last_seen : null }] : [];
        });
      } catch { /* Optional history must never prevent equipping the current skin. */ }
    }
    return result;
  }

  getSkinImageUrlFromProfile = getSkinImageUrlFromProfile;

  getTextureResponse(source: string, deadline?: AbortSignal): Promise<Response> {
    const safe = skinImageUrl(source);
    if (!safe || new URL(safe).origin !== MC_API_BASE_URL || !new URL(safe).pathname.startsWith("/v1/skin/")) {
      return Promise.reject(new SkinApiError(400, "Origem de textura inválida."));
    }
    return this.http.response(new URL(safe).pathname, "image/png", deadline);
  }

  async autocompleteSkinNames(query: string): Promise<PlayerSuggestion[]> {
    return rankPlayers(await this.searchPlayers(query), query);
  }
}

export function rankPlayers(players: PlayerSuggestion[], query: string): PlayerSuggestion[] {
  const needle = query.trim().toLowerCase();
  const score = (player: PlayerSuggestion) => (player.name.toLowerCase() === needle ? 0
    : player.name.toLowerCase().startsWith(needle) ? 10 : 100) + (player.metadata.current === false ? 1 : 0);
  return [...players].sort((a, b) => score(a) - score(b) || a.name.length - b.name.length || a.name.localeCompare(b.name))
    .filter((player, index, all) => all.findIndex((other) => other.uuid === player.uuid && other.name.toLowerCase() === player.name.toLowerCase()) === index)
    .slice(0, 12);
}
