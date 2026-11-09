/** Production-only Firebase boundary. Tokens are never persisted by this adapter. */
import { normalizeSocialBindingSession, SocialBindingError, type SocialBindingDependencies } from "./connectSocialBinding.ts";

const UID = /^[A-Za-z0-9_-]{1,128}$/;
export const SOCIAL_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const fail = (code: string): never => { throw new SocialBindingError(code); };
export async function socialJson(response: Response, maximum = 65_536): Promise<Record<string, unknown>> {
  if (!response.ok) { await response.body?.cancel(); return fail(response.status === 429 ? "rate_limited" : response.status === 401 ? "unauthenticated" : [403, 404].includes(response.status) ? "session_not_authorized" : "backend_unavailable"); }
  const reader = response.body?.getReader(); if (!reader) return fail("backend_unavailable");
  const chunks: Uint8Array[] = []; let count = 0;
  try {
    for (;;) { const { done, value } = await reader.read(); if (done) break; count += value.byteLength; if (count > maximum) fail("request_too_large"); chunks.push(value); }
    const bytes = new Uint8Array(count); let offset = 0; for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
    const parsed = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) fail("invalid_request"); return parsed;
  } finally { await reader.cancel().catch(() => {}); reader.releaseLock(); }
}
export function socialBearer(request: Request): string {
  const header = request.headers.get("authorization");
  if (!header || header.length > 16_384 || !/^Bearer [A-Za-z0-9_.-]+$/.test(header)) return fail("unauthenticated");
  return header.slice(7);
}
function decoded(text: string): Uint8Array<ArrayBuffer> {
  if (!/^[A-Za-z0-9_-]+$/.test(text)) return fail("unauthenticated");
  return Uint8Array.from(atob(text.replace(/-/g, "+").replace(/_/g, "/")), char => char.charCodeAt(0));
}
/** Exact project, issuer, account, time and RS256 signature. Unsigned emulator JWTs are never accepted. */
export async function socialAuthenticate(request: Request, project: string, signal: AbortSignal): Promise<string> {
  try {
    if (!/^[a-z][a-z0-9-]{4,28}[a-z0-9]$/.test(project) || project.startsWith("demo-")) fail("backend_unavailable");
    const parts = socialBearer(request).split("."); if (parts.length !== 3) fail("unauthenticated");
    const json = (part: string) => JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(decoded(part)));
    const header = json(parts[0]), claims = json(parts[1]), now = Math.floor(Date.now() / 1000);
    if (header.alg !== "RS256" || typeof header.kid !== "string" || !/^[A-Za-z0-9_-]{1,128}$/.test(header.kid)
      || claims.aud !== project || claims.iss !== `https://securetoken.google.com/${project}` || !UID.test(claims.sub)
      || (claims.user_id !== undefined && claims.user_id !== claims.sub)
      || !Number.isSafeInteger(claims.exp) || claims.exp <= now || claims.exp > now + 3600
      || !Number.isSafeInteger(claims.iat) || claims.iat > now || claims.iat < now - 3600
      || !Number.isSafeInteger(claims.auth_time) || claims.auth_time > now) fail("unauthenticated");
    const jwks = await socialJson(await fetch("https://www.googleapis.com/service_accounts/v1/jwk/securetoken@system.gserviceaccount.com", {
      // Workers supports manual/follow, not redirect:error. socialJson rejects
      // every 3xx response, so neither credentials nor JWKS follow redirects.
      redirect: "manual", signal, cf: { cacheTtl: 3600, cacheEverything: true },
    }));
    if (!Array.isArray(jwks.keys) || jwks.keys.length > 10) fail("backend_unavailable");
    const key = jwks.keys.find((key: JsonWebKey & { kid?: string }) => key.kid === header.kid && key.kty === "RSA" && key.alg === "RS256");
    if (!key) fail("unauthenticated");
    const publicKey = await crypto.subtle.importKey("jwk", key, { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, ["verify"]);
    if (!await crypto.subtle.verify("RSASSA-PKCS1-v1_5", publicKey, decoded(parts[2]), new TextEncoder().encode(parts[0] + "." + parts[1]))) fail("unauthenticated");
    return claims.sub;
  } catch (error) { if (error instanceof SocialBindingError) throw error; return fail("unauthenticated"); }
}
type Field = { stringValue?: string; integerValue?: string; timestampValue?: string; nullValue?: null };
export function createSocialFirebaseDependencies(project: string, authenticatedUid: string, routedSession: string): SocialBindingDependencies {
  return {
    // The Worker verifies the signature before invoking the non-public DO RPC.
    trustedAuthenticate: async request => {
      const uid = authenticatedUid || request.headers.get("x-aurora-verified-account") || "";
      if (!UID.test(uid)) return fail("unauthenticated"); return uid;
    },
    clock: { wall: Date.now, monotonic: () => performance.now() }, // DO supplies its persisted virtual epoch.
    async readSession(principal, sessionId, signal) {
      const callerUid = authenticatedUid || principal.request.headers.get("x-aurora-verified-account") || "";
      if (principal.accountId !== callerUid || !UID.test(callerUid) || sessionId !== routedSession || !SOCIAL_UUID.test(sessionId)) fail("session_not_authorized");
      const get = async (path: string) => {
        const result = await socialJson(await fetch(`https://firestore.googleapis.com/v1/projects/${project}/databases/(default)/documents/${path}`, {
          headers: { Authorization: `Bearer ${socialBearer(principal.request)}` }, redirect: "manual", signal,
        }));
        if (!result.fields || typeof result.fields !== "object" || Array.isArray(result.fields)) return fail("backend_unavailable");
        return result.fields as Record<string, Field>;
      };
      const [profile, fields] = await Promise.all([get(`users/${callerUid}`), get(`socialSessions/${sessionId}`)]);
      const nick = profile.usernameLower?.stringValue;
      if (profile.uid?.stringValue !== callerUid || !nick || !/^[a-z0-9_]{3,16}$/.test(nick)) fail("account_not_registered");
      const keys = ["schema", "hostUid", "title", "modpackName", "minecraftVersion", "loader", "imageUrl", "players", "maxPlayers", "visibility", "joinPolicy", "expiresAt"];
      if (Object.keys(fields).length !== keys.length || keys.some(key => !Object.hasOwn(fields, key)) || fields.schema?.integerValue !== "1") fail("invalid_session");
      const session = normalizeSocialBindingSession({ id: sessionId, hostUid: fields.hostUid?.stringValue, title: fields.title?.stringValue,
        modpackName: fields.modpackName?.stringValue, minecraftVersion: fields.minecraftVersion?.stringValue, loader: fields.loader?.stringValue,
        imageUrl: fields.imageUrl?.nullValue === null ? null : fields.imageUrl?.stringValue,
        players: Number(fields.players?.integerValue), maxPlayers: Number(fields.maxPlayers?.integerValue), visibility: fields.visibility?.stringValue,
        joinPolicy: fields.joinPolicy?.stringValue, expiresAtMs: Date.parse(fields.expiresAt?.timestampValue ?? "") });
      const [registry, approval] = await Promise.all([get(`usernames/${nick}`), session.hostUid !== callerUid && session.joinPolicy === "approval"
        ? get(`socialJoinRequests/${sessionId}/guests/${callerUid}`) : Promise.resolve(undefined)]);
      if (registry.uid?.stringValue !== callerUid) fail("account_not_registered");
      if (approval) {
        if (approval.status?.stringValue !== "accepted" || approval.hostUid?.stringValue !== session.hostUid
          || approval.guestUid?.stringValue !== callerUid || approval.sessionId?.stringValue !== sessionId
          || Date.parse(approval.expiresAt?.timestampValue ?? "") <= Date.now()) fail("session_not_authorized");
      }
      return session;
    },
  };
}
