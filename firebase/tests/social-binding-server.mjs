/** Explicit local proof fixture. Never import this adapter in the production Worker. */
import http from "node:http";
import { pathToFileURL } from "node:url";
import { createSocialBindingAuthority, normalizeSocialBindingSession, SocialBindingError } from "../../apps/edge-proxy/src/connectSocialBinding.ts";
import { createSocialAdmissionAuthority } from "../../apps/edge-proxy/src/connectSocialAdmission.ts";

const PROJECT = "demo-aurora-social";
const AUTH = "http://127.0.0.1:9187/identitytoolkit.googleapis.com/v1/accounts:lookup?key=demo-aurora-social";
const DOCUMENTS = "http://127.0.0.1:8187/v1/projects/demo-aurora-social/databases/(default)/documents/";
const UID = /^[A-Za-z0-9_-]{1,128}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const fail = code => { throw new SocialBindingError(code); };

async function boundedJson(response, maximum = 65_536) {
  if (!response.ok) { await response.body?.cancel(); fail(response.status === 401 ? "unauthenticated" : response.status === 403 || response.status === 404 ? "session_not_authorized" : "backend_unavailable"); }
  const length = response.headers.get("content-length");
  if (length !== null && (!/^\d+$/.test(length) || Number(length) > maximum)) { await response.body?.cancel(); fail("backend_unavailable"); }
  const reader = response.body?.getReader(); if (!reader) fail("backend_unavailable");
  const chunks = []; let bytes = 0;
  try {
    while (true) { const chunk = await reader.read(); if (chunk.done) break; bytes += chunk.value.byteLength; if (bytes > maximum) fail("backend_unavailable"); chunks.push(chunk.value); }
    const all = new Uint8Array(bytes); let offset = 0; for (const chunk of chunks) { all.set(chunk, offset); offset += chunk.byteLength; }
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(all));
  } catch { fail("backend_unavailable"); }
  finally { await reader.cancel().catch(() => {}); reader.releaseLock(); }
}
function bearer(request) {
  const header = request.headers.get("authorization");
  if (!header || header.length > 16_384 || !/^Bearer [A-Za-z0-9_.-]+$/.test(header)) fail("unauthenticated");
  return header.slice(7);
}
function tokenClaims(token) {
  try {
    const segments = token.split("."); if (segments.length !== 3 || segments[2] !== "") fail("unauthenticated");
    const header = JSON.parse(Buffer.from(segments[0], "base64url").toString("utf8"));
    const claims = JSON.parse(Buffer.from(segments[1], "base64url").toString("utf8"));
    const now = Math.floor(Date.now() / 1_000);
    if (header.alg !== "none" || claims.aud !== PROJECT || claims.iss !== `https://securetoken.google.com/${PROJECT}`
      || typeof claims.sub !== "string" || !UID.test(claims.sub) || (claims.user_id !== undefined && claims.user_id !== claims.sub)
      || !Number.isSafeInteger(claims.exp) || claims.exp <= now || !Number.isSafeInteger(claims.iat) || claims.iat > now
      || !Number.isSafeInteger(claims.auth_time) || claims.auth_time > now) fail("unauthenticated");
    return claims;
  } catch { fail("unauthenticated"); }
}
/** The emulator deliberately has no signature. Trust is limited to this explicitly opted-in local fixture. */
export function createLocalEmulatorBindingDependencies() {
  return {
    async trustedAuthenticate(request, signal) {
      const token = bearer(request), claims = tokenClaims(token);
      const result = await boundedJson(await fetch(AUTH, { method: "POST", redirect: "error", signal,
        headers: { "Content-Type": "application/json" }, body: JSON.stringify({ idToken: token }) }));
      if (!Array.isArray(result.users) || result.users.length !== 1 || result.users[0].localId !== claims.sub || result.users[0].disabled === true) fail("unauthenticated");
      return claims.sub;
    },
    async readSession(principal, sessionId, signal) {
      if (!UID.test(principal.accountId) || !UUID.test(sessionId)) fail("session_not_authorized");
      const token = bearer(principal.request);
      const get = async path => {
        const result = await boundedJson(await fetch(DOCUMENTS + path, { redirect: "error", signal, headers: { Authorization: `Bearer ${token}` } }));
        if (!result.fields || typeof result.fields !== "object" || Array.isArray(result.fields)) fail("backend_unavailable");
        return result.fields;
      };
      // Only own profile, exact nick registration and an authorized session. Never query another user's graph.
      const profile = await get(`users/${principal.accountId}`);
      const nickname = profile.usernameLower?.stringValue;
      if (profile.uid?.stringValue !== principal.accountId || typeof nickname !== "string" || !/^[a-z0-9_]{3,16}$/.test(nickname)) fail("account_not_registered");
      const registration = await get(`usernames/${nickname}`);
      if (registration.uid?.stringValue !== principal.accountId) fail("account_not_registered");
      const fields = await get(`socialSessions/${sessionId}`);
      const keys = ["schema", "hostUid", "title", "modpackName", "minecraftVersion", "loader", "imageUrl", "players", "maxPlayers", "visibility", "joinPolicy", "expiresAt"];
      if (Object.keys(fields).length !== keys.length || keys.some(key => !Object.hasOwn(fields, key)) || fields.schema?.integerValue !== "1") fail("invalid_session");
      const string = key => fields[key]?.stringValue;
      const integer = key => /^\d+$/.test(fields[key]?.integerValue ?? "") ? Number(fields[key].integerValue) : NaN;
      const expiry = fields.expiresAt?.timestampValue;
      if (typeof expiry !== "string" || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{1,9})?Z$/.test(expiry)) fail("invalid_session");
      return normalizeSocialBindingSession({ id: sessionId, hostUid: string("hostUid"), title: string("title"), modpackName: string("modpackName"),
        minecraftVersion: string("minecraftVersion"), loader: string("loader"), imageUrl: fields.imageUrl?.nullValue === null ? null : string("imageUrl"),
        players: integer("players"), maxPlayers: integer("maxPlayers"), visibility: string("visibility"), joinPolicy: string("joinPolicy"), expiresAtMs: Date.parse(expiry) });
    },
  };
}

async function requestBody(request, maximum) {
  if (request.headers["content-type"] !== "application/json" || request.headers["content-encoding"] !== undefined) fail("invalid_request");
  const length = request.headers["content-length"];
  if (length !== undefined && (!/^\d+$/.test(length) || Number(length) > maximum)) fail("request_too_large");
  const chunks = []; let bytes = 0;
  for await (const chunk of request) { bytes += chunk.length; if (bytes > maximum) fail("request_too_large"); chunks.push(chunk); }
  try { return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks))); }
  catch { fail("invalid_request"); }
}
export async function startSocialBindingLab({ enabled = false, port = 42_888, admission = false } = {}) {
  if (enabled !== true || ![0, 42_888].includes(port)) throw new Error("SOCIAL_BINDING_LAB_OPT_IN_REQUIRED");
  if (typeof admission !== "boolean") throw new Error("SOCIAL_ADMISSION_LAB_OPT_IN_REQUIRED");
  const dependencies = createLocalEmulatorBindingDependencies();
  const authority = createSocialBindingAuthority(dependencies);
  const admissionAuthority = admission ? await createSocialAdmissionAuthority(authority, dependencies) : null;
  const keyRoutes = ["/v1/social/key/challenge", "/v1/social/key/proof"];
  const admissionRoutes = ["/v1/social/host/register", "/v1/social/join", "/v1/social/continuity"];
  let origin;
  const server = http.createServer(async (req, res) => {
    const cancellation = new AbortController();
    req.once("aborted", () => cancellation.abort());
    res.once("close", () => { if (!res.writableEnded) cancellation.abort(); });
    const reply = (status, body) => { if (!res.destroyed) { res.writeHead(status, { "Content-Type": "application/json", "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff" }); res.end(JSON.stringify(body)); } };
    try {
      // Reject browser cross-origin callers/DNS rebinding; no wildcard CORS and no arbitrary upstream/project.
      if (!origin || req.headers.host !== new URL(origin).host || req.headers.origin !== undefined || req.method !== "POST"
          || !(keyRoutes.includes(req.url) || (admissionAuthority && admissionRoutes.includes(req.url)))) fail("invalid_request");
      const body = await requestBody(req, keyRoutes.includes(req.url) ? 4_096 : 8_192);
      const request = new Request(origin + req.url, { method: "POST", signal: cancellation.signal, headers: { Authorization: req.headers.authorization ?? "" } });
      let output;
      switch (req.url) {
        case "/v1/social/key/challenge": output = await authority.challenge(request, body); break;
        case "/v1/social/key/proof": output = await authority.prove(request, body); break;
        case "/v1/social/host/register": output = await admissionAuthority.registerHost(request, body); break;
        case "/v1/social/join": output = await admissionAuthority.join(request, body); break;
        case "/v1/social/continuity": output = await admissionAuthority.continuity(request, body); break;
        default: fail("invalid_request");
      }
      reply(200, output);
    } catch (error) {
      const code = error instanceof SocialBindingError ? error.code : "lab_unavailable";
      const status = code === "unauthenticated" ? 401 : code === "rate_limited" ? 429 : code === "authority_busy" || code === "backend_timeout" || code === "backend_unavailable" ? 503
        : code === "request_too_large" ? 413 : code === "session_not_authorized" || code === "account_not_registered" ? 403 : 400;
      reply(status, { code });
    }
  });
  server.requestTimeout = 10_000; server.headersTimeout = 5_000; server.setTimeout(10_000, socket => socket.destroy());
  await new Promise((resolve, reject) => { server.once("error", reject); server.listen(port, "127.0.0.1", resolve); });
  origin = `http://127.0.0.1:${server.address().port}`;
  // These pins are provisioned directly to the runner that created this object, never through a renderer-selected URL.
  return { origin, issuerInstance: authority.issuerInstance, admissionPublicKey: admissionAuthority?.admissionPublicKey,
    inspect: () => authority.inspect(), inspectAdmission: () => admissionAuthority?.inspect(),
    async close() { server.closeIdleConnections(); const stopped = new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve())); server.closeAllConnections(); await stopped; } };
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (process.argv.length !== 2 || process.env.AURORA_SOCIAL_BINDING_LAB !== "1") throw new Error("SOCIAL_BINDING_LAB_OPT_IN_REQUIRED");
  const lab = await startSocialBindingLab({ enabled: true, admission: process.env.AURORA_SOCIAL_ADMISSION_LAB === "1" });
  console.log(JSON.stringify({ event: "social_binding_lab_ready", origin: lab.origin, localAuthEmulator: true, transportAuthorized: false }));
  let stopping = false;
  const stop = async () => { if (stopping) return; stopping = true; await lab.close(); console.log(JSON.stringify({ event: "social_binding_lab_stopped" })); };
  process.once("SIGINT", stop); process.once("SIGTERM", stop);
}
