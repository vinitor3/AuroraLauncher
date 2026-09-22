import { object } from "./dtos.ts";
import { JsonHttpClient, SkinApiError } from "./http.ts";
import { normalizeUuid, skinImageUrl, validPlayerName, type PlayerSuggestion, type SkinResult } from "./models.ts";

/** Exact public Java profile lookup; never receives a third-party API key. */
export class MojangClient {
  private readonly names: JsonHttpClient;
  private readonly profiles: JsonHttpClient;

  constructor(rate: number, fetcher: typeof fetch, now: () => number) {
    this.names = new JsonHttpClient("https://api.mojang.com", "", rate, 2000, fetcher, now);
    this.profiles = new JsonHttpClient("https://sessionserver.mojang.com", "", rate, 2000, fetcher, now);
  }

  async findPlayer(name: string, deadline?: AbortSignal): Promise<PlayerSuggestion> {
    const { data } = await this.names.get<unknown>(`/users/profiles/minecraft/${encodeURIComponent(name)}`, deadline);
    const row = object(data);
    const uuid = normalizeUuid(row.id);
    if (!uuid || !validPlayerName(row.name) || row.name.toLowerCase() !== name.toLowerCase()) {
      throw new SkinApiError(502, "Perfil oficial de jogador inválido.");
    }
    return { name: row.name, uuid, source: "mojang", metadata: { current: true } };
  }

  async getPlayerProfile(identifier: string, deadline?: AbortSignal): Promise<SkinResult> {
    const requestedUuid = normalizeUuid(identifier);
    const uuid = requestedUuid ?? (await this.findPlayer(identifier, deadline)).uuid!;
    const { data } = await this.profiles.get<unknown>(`/session/minecraft/profile/${uuid}`, deadline);
    const row = object(data);
    if (normalizeUuid(row.id) !== uuid || !validPlayerName(row.name)
      || (!requestedUuid && row.name.toLowerCase() !== identifier.toLowerCase()) || !Array.isArray(row.properties)) {
      throw new SkinApiError(502, "Perfil oficial de jogador inválido.");
    }
    const property = row.properties.find((value) => value && typeof value === "object" && value.name === "textures");
    if (!property || typeof property.value !== "string" || property.value.length > 32768) {
      throw new SkinApiError(502, "O perfil oficial não retornou uma textura de skin.");
    }
    let textures: Record<string, unknown>;
    try { textures = object(JSON.parse(atob(property.value))); }
    catch { throw new SkinApiError(502, "Textura do perfil oficial inválida."); }
    if (normalizeUuid(textures.profileId) !== uuid) throw new SkinApiError(502, "A textura pertence a outro jogador.");
    const skin = object(object(textures.textures).SKIN);
    const textureUrl = skinImageUrl(skin.url);
    if (!textureUrl || new URL(textureUrl).hostname !== "textures.minecraft.net") {
      throw new SkinApiError(502, "O perfil oficial não retornou uma textura de skin válida.");
    }
    const metadata = skin.metadata && typeof skin.metadata === "object" ? object(skin.metadata) : {};
    return { name: row.name, uuid, model: metadata.model === "slim" ? "slim" : "classic",
      textureUrl, skinUrl: textureUrl, source: "mojang", stale: false };
  }
}
