export interface PlayerSuggestion {
  name: string;
  uuid?: string;
  source: "mc-api" | "mcstat" | "mojang";
  metadata: Record<string, unknown>;
}

export interface SkinHistoryEntry {
  textureUrl: string;
  model: "classic" | "slim";
  firstSeen?: string | null;
  lastSeen?: string | null;
}

export interface SkinResult {
  name: string;
  uuid: string;
  model: "classic" | "slim";
  textureUrl: string;
  skinUrl: string;
  headUrl?: string;
  bodyUrl?: string;
  history?: SkinHistoryEntry[];
  source: "mc-api" | "mcstat" | "mojang";
  stale: boolean;
}

export interface PlayerSkinEnv {
  MC_API_KEY?: string;
  mcstat_API_KEY?: string;
  MCSTAT_API_KEY?: string;
  SKIN_API_REQUESTS_PER_MINUTE?: string;
}

export function normalizeUuid(value: unknown): string | undefined {
  if (typeof value !== "string" || !/^(?:[0-9a-f]{32}|[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12})$/i.test(value)) return undefined;
  return value.replace(/-/g, "").toLowerCase();
}

export function validPlayerName(value: unknown): value is string {
  return typeof value === "string" && /^[A-Za-z0-9_]{1,16}$/.test(value);
}

/** Only public image endpoints; credentials or arbitrary URL query strings never escape. */
export function skinImageUrl(value: unknown): string | undefined {
  // Base64 transport plus the proxy prefix must fit the legacy 2048-byte URL field.
  if (typeof value !== "string" || value.length > 1000) return undefined;
  try {
    const url = new URL(value, "https://mc-api.bisai.dev");
    if (url.username || url.password || url.port || url.hash) return undefined;
    if (url.hostname === "textures.minecraft.net" && /^\/texture\/[0-9a-f]{32,64}$/i.test(url.pathname)
      && ["http:", "https:"].includes(url.protocol) && !url.search) {
      url.protocol = "https:";
      return url.href;
    }
    if (url.protocol !== "https:") return undefined;
    if (url.hostname === "mcstat.org" && /^\/media\/skins\/published\/[A-Za-z0-9_.-]+\.png$/.test(url.pathname)) {
      // MCStat may version assets with ?v=. Do not forward any other parameters.
      const version = url.searchParams.get("v");
      url.search = "";
      if (version && /^[0-9]{1,20}$/.test(version)) url.searchParams.set("v", version);
      return url.href;
    }
    if (url.hostname === "mc-api.bisai.dev"
      && /^\/v1\/(?:skin\/(?:texture\/)?[A-Za-z0-9_-]{1,64}|render\/(?:head|body|3d\/fullbody)\/[A-Za-z0-9_-]{1,36})$/.test(url.pathname)) {
      url.search = "";
      return url.href;
    }
  } catch { /* Invalid upstream URL. */ }
  return undefined;
}

export function getSkinImageUrlFromProfile(profile: SkinResult, view: "skin" | "head" | "body" = "body"): string {
  return (view === "head" ? profile.headUrl : view === "body" ? profile.bodyUrl : undefined)
    ?? profile.skinUrl ?? profile.textureUrl;
}
