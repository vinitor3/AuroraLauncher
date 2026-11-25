import { invoke, isTauri } from "@tauri-apps/api/core";
import { isRetiredSkinSource } from "./skinCatalogSources.ts";

export type PlayerSuggestion = { name: string; uuid?: string; source: string; metadata: Record<string, unknown> };
export type SkinResult = { name: string; uuid: string; model: "classic" | "slim"; skinUrl: string;
  textureUrl: string; contentHash: string; headUrl?: string; bodyUrl?: string; source: string; stale: boolean };
export type CatalogEntry = { id: string; name: string; author: string; model: "classic" | "slim"; hash: string; source: string; tags: string[] };
export type CatalogPage = { entries: CatalogEntry[]; page: number; hasMore: boolean; sources: string[]; stale?: boolean };
// Zero-based Worker pages are accepted through index 100000.
export const LAST_CATALOG_PAGE_NUMBER = 100001;

export function isPlayerIdentifier(value: string): boolean {
  return /^(?:[A-Za-z0-9_]{1,16}|[0-9a-f]{32}|[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12})$/i.test(value.trim());
}

class PlayerSkinRequestError extends Error {
  readonly status: number;
  constructor(message: string, status: number) { super(message); this.status = status; }
}

/** Aurora proxy first; native recovery uses public official APIs without credentials. */
export class PlayerSkinsClient {
  private base: string;
  constructor(base: string) {
    const url = new URL(base);
    if (url.protocol !== "https:" || url.username || url.password) throw new Error("O catálogo exige uma conexão HTTPS.");
    this.base = url.origin;
  }
  private async get<T>(path: string, signal?: AbortSignal): Promise<T> {
    let response: Response;
    const timeout = AbortSignal.timeout(12000);
    try {
      response = await fetch(this.base + path, { signal: signal ? AbortSignal.any([signal, timeout]) : timeout });
    } catch {
      signal?.throwIfAborted();
      if (timeout.aborted) throw new PlayerSkinRequestError("A busca de skins demorou para responder. Tente novamente.", 504);
      throw new PlayerSkinRequestError("Não foi possível conectar à busca de skins.", 0);
    }
    const data = await response.json().catch(() => null);
    if (!response.ok || !data) {
      const seconds = Number(response.headers.get("Retry-After"));
      const message = typeof data?.error === "string" ? data.error : "Não foi possível consultar as skins.";
      const wait = Number.isSafeInteger(seconds) && seconds > 0 && seconds <= 86400
        ? `Tente novamente em ${seconds < 60 ? `${seconds} s` : `${Math.ceil(seconds / 60)} min`}.` : "Aguarde alguns instantes.";
      throw new PlayerSkinRequestError(response.status === 429 ? `Muitas consultas. ${wait}` : message, response.ok ? 502 : response.status);
    }
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
    if (!isPlayerIdentifier(identifier)) throw new Error("Informe o nome ou UUID da conta Minecraft Java.");
    const id = identifier.trim();
    try {
      const result = await this.get<SkinResult>(`/v1/skins/profile/${encodeURIComponent(id)}`, signal);
      this.validateProfile(result);
      if (!this.validAsset(result.skinUrl)) throw new Error("Perfil de skin inválido.");
      return result;
    } catch (error) {
      signal?.throwIfAborted();
      if (!isTauri() || !(error instanceof PlayerSkinRequestError) || ![0, 404, 503, 504].includes(error.status)) throw error;
      // Official public APIs can reject Cloudflare egress. The narrow native
      // command uses the user's connection, validates the identity and PNG/hash,
      // and never receives or returns an upstream credential.
      let result: SkinResult;
      try { result = await invoke<SkinResult>("lookup_player_skin", { identifier: id }); }
      catch (cause) {
        signal?.throwIfAborted();
        throw new Error(typeof cause === "string" ? cause : "Não foi possível consultar o perfil oficial agora.");
      }
      signal?.throwIfAborted();
      this.validateProfile(result);
      const uuid = id.replace(/-/g, "").toLowerCase();
      const matches = /^[0-9a-f]{32}$/.test(uuid) ? result.uuid === uuid : result.name.toLowerCase() === id.toLowerCase();
      if (!matches || result.source !== "mojang-local" || result.stale !== false || result.skinUrl !== result.textureUrl
        || !/^https:\/\/textures\.minecraft\.net\/texture\/[0-9a-f]{32,64}$/.test(result.skinUrl)) {
        throw new Error("Perfil oficial de skin inválido.");
      }
      return result;
    }
  }
  private validateProfile(result: SkinResult): void {
    if (!result || typeof result !== "object" || !/^[A-Za-z0-9_]{1,16}$/.test(result.name) || !/^[0-9a-f]{32}$/.test(result.uuid)
      || !["classic", "slim"].includes(result.model) || !/^[0-9a-f]{64}$/.test(result.contentHash)) throw new Error("Perfil de skin inválido.");
  }
  async getCatalog(query: string, page: number, signal?: AbortSignal): Promise<CatalogPage> {
    const result = await this.get<CatalogPage>(`/v1/skins/catalog?format=json&query=${encodeURIComponent(query)}&page=${page}&pageSize=12`, signal);
    if (!Array.isArray(result.entries) || result.entries.length > 24 || result.page !== page
      || typeof result.hasMore !== "boolean" || !Array.isArray(result.sources)
      || result.entries.some((entry) => !entry || typeof entry.name !== "string" || typeof entry.author !== "string"
        || typeof entry.id !== "string" || !/^[0-9a-f]{64}$/.test(entry.hash) || !["classic", "slim"].includes(entry.model)
        || !this.validAsset(entry.source))) throw new Error("Catálogo de skins inválido.");
    return { ...result, entries: result.entries.filter((entry) => !isRetiredSkinSource(entry.source, this.base)) };
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
