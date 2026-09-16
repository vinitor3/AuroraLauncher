export type PlayerSuggestion = { name: string; uuid?: string; source: string; metadata: Record<string, unknown> };
export type SkinResult = { name: string; uuid: string; model: "classic" | "slim"; skinUrl: string;
  textureUrl: string; contentHash: string; headUrl?: string; bodyUrl?: string; source: string; stale: boolean };
export type CatalogEntry = { id: string; name: string; author: string; model: "classic" | "slim"; hash: string; source: string; tags: string[] };
export type CatalogPage = { entries: CatalogEntry[]; page: number; hasMore: boolean; sources: string[] };

/** Client to Aurora only. Upstream API credentials remain on the Worker. */
export class PlayerSkinsClient {
  private base: string;
  constructor(base: string) {
    const url = new URL(base);
    if (url.protocol !== "https:" || url.username || url.password) throw new Error("O catálogo exige uma conexão HTTPS.");
    this.base = url.origin;
  }
  private async get<T>(path: string, signal?: AbortSignal): Promise<T> {
    const response = await fetch(this.base + path, { signal: signal
      ? AbortSignal.any([signal, AbortSignal.timeout(12000)]) : AbortSignal.timeout(12000) });
    const data = await response.json().catch(() => null);
    if (!response.ok || !data) throw new Error(response.status === 429
      ? "Muitas consultas. Aguarde alguns instantes." : data?.error ?? "Não foi possível consultar as skins.");
    return data as T;
  }
  async autocompleteSkinNames(query: string, signal?: AbortSignal): Promise<PlayerSuggestion[]> {
    if (!/^[A-Za-z0-9_]{3,16}$/.test(query)) return [];
    const response = await this.get<{ players: PlayerSuggestion[] }>(`/v1/skins/players?query=${encodeURIComponent(query)}`, signal);
    if (!Array.isArray(response.players)) throw new Error("Lista de jogadores inválida.");
    return response.players.slice(0, 12).filter((player) => player && typeof player.name === "string"
      && /^[A-Za-z0-9_]{1,16}$/.test(player.name) && (!player.uuid || /^[0-9a-f]{32}$/.test(player.uuid))
      && typeof player.source === "string" && player.metadata && typeof player.metadata === "object" && !Array.isArray(player.metadata));
  }
  async getPlayerProfile(identifier: string, signal?: AbortSignal): Promise<SkinResult> {
    const result = await this.get<SkinResult>(`/v1/skins/profile/${encodeURIComponent(identifier)}`, signal);
    if (!/^[A-Za-z0-9_]{1,16}$/.test(result.name) || !/^[0-9a-f]{32}$/.test(result.uuid)
      || !["classic", "slim"].includes(result.model) || !/^[0-9a-f]{64}$/.test(result.contentHash)
      || !this.validAsset(result.skinUrl)) throw new Error("Perfil de skin inválido.");
    return result;
  }
  async getCatalog(query: string, page: number, signal?: AbortSignal): Promise<CatalogPage> {
    const result = await this.get<CatalogPage>(`/v1/skins/catalog?format=json&query=${encodeURIComponent(query)}&page=${page}&pageSize=12`, signal);
    if (!Array.isArray(result.entries) || result.entries.length > 24 || result.page !== page
      || typeof result.hasMore !== "boolean" || !Array.isArray(result.sources)
      || result.entries.some((entry) => !entry || typeof entry.name !== "string" || typeof entry.author !== "string"
        || typeof entry.id !== "string" || !/^[0-9a-f]{64}$/.test(entry.hash) || !["classic", "slim"].includes(entry.model)
        || !this.validAsset(entry.source))) throw new Error("Catálogo de skins inválido.");
    return result;
  }
  resolveCatalogUrl(entry: CatalogEntry): string {
    const url = new URL(entry.source, this.base);
    if (!this.validAsset(entry.source)) throw new Error("Origem de skin inválida.");
    return url.href;
  }
  private validAsset(value: unknown): boolean {
    try {
      if (typeof value !== "string" || value.length > 2048) return false;
      const url = new URL(value, this.base);
      return url.origin === this.base && !url.username && !url.password
        && /^\/v1\/skins\/(?:catalog\/|player-texture\/)/.test(url.pathname) && url.pathname.endsWith(".png");
    } catch { return false; }
  }
}
