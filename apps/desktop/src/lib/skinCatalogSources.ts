export const PUBLIC_SKIN_WORKER = "https://aurora-api.aurora-edge-proxy.workers.dev";

const publicTexture = /^\/v1\/skins\/(?:catalog\/mcstat|player-texture)\/([0-9a-f]{64})\/([A-Za-z0-9_-]{1,1800})\.png$/;

export async function skinSha256(image: Blob): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", await image.arrayBuffer());
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

/** Only an explicit selection from the public catalog stores this reference. */
export function publicCatalogSource(source: string | undefined, hash: string): string | undefined {
  if (!source) return undefined;
  try {
    const url = new URL(source);
    if (url.origin !== PUBLIC_SKIN_WORKER || url.username || url.password || url.search || url.hash) return undefined;
    const match = publicTexture.exec(url.pathname);
    if (!match || match[1] !== hash) return undefined;
    // The proxy encodes the upstream URL into its own path. A signed/private
    // query in that path would become visible on the public profile too.
    const upstream = new URL(atob(match[2].replace(/-/g, "+").replace(/_/g, "/")));
    if (upstream.username || upstream.password || upstream.hash) return undefined;
    if (!["https://mcstat.org", "https://textures.minecraft.net", "https://mc-api.bisai.dev"].includes(upstream.origin)) return undefined;
    if (upstream.search && !(upstream.hostname === "mcstat.org" && /^\?v=\d{1,20}$/.test(upstream.search))) return undefined;
    return url.href;
  } catch {
    return undefined;
  }
}

export async function verifyPublicCatalogSkin(source: string | undefined, equipped: Blob): Promise<string | undefined> {
  if (!source) return undefined;
  try {
    const hash = await skinSha256(equipped);
    const url = publicCatalogSource(source, hash);
    if (!url) return undefined;
    const response = await fetch(url, { credentials: "omit", redirect: "error", referrerPolicy: "no-referrer",
      cache: "no-store", signal: AbortSignal.timeout(6000) });
    if (!response.ok || !/^image\/png(?:;|$)/i.test(response.headers.get("content-type") || "")) return undefined;
    // These two public Worker routes serve only bounded, hash-validated PNGs.
    const remote = await response.blob();
    return remote.size === equipped.size && await skinSha256(remote) === hash ? url : undefined;
  } catch {
    return undefined;
  }
}

const retiredTexture = /^\/v1\/skins\/catalog\/(?:aurora-voyager|solar-ninja|ki-guardian|forest-ranger|cyber-violet|crimson-mage)\.png$/;

export function isRetiredSkinSource(source: string | null | undefined, workerUrl = PUBLIC_SKIN_WORKER): boolean {
  if (!source) return false;
  try {
    const worker = new URL(workerUrl || PUBLIC_SKIN_WORKER);
    const url = new URL(source, worker);
    return (url.origin === worker.origin || url.origin === PUBLIC_SKIN_WORKER) && retiredTexture.test(url.pathname);
  } catch {
    return false;
  }
}
