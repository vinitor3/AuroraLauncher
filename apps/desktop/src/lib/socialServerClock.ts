import type { FirebaseServices } from "./firebase";

const MAX_SAMPLE_AGE_MS = 60_000;
const REFRESH_AFTER_MS = 30_000;
const REQUEST_TIMEOUT_MS = 10_000;
const MAX_RESPONSE_BYTES = 8192;
const unavailable = () => Object.assign(new Error("O horário do serviço está indisponível. Atualize e tente novamente."), { code: "social_clock_unavailable" });
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);

/** Display and Firestore TTL arithmetic only. Never Connect admission authority. */
export function createSocialServerClock(services: FirebaseServices, localEmulator: boolean) {
  const user = services.auth.currentUser;
  const project = services.config.projectId;
  const scope = services.authScope;
  const scopeId = scope.id, persistent = scope.persistent;
  if (!user || !/^[A-Za-z0-9_-]{1,128}$/.test(user.uid)
    || (localEmulator ? !/^demo-aurora-social(?:-rules-v2)?$/.test(project) : project !== "auroralauncher")) throw unavailable();
  const document = `projects/${project}/databases/(default)/documents/users/${user.uid}`;
  const url = `${localEmulator ? "http://127.0.0.1:8187" : "https://firestore.googleapis.com"}/v1/projects/${project}/databases/(default)/documents:batchGet`;
  let sample: { readTime: number; received: number; rtt: number } | undefined;
  let pending: Promise<void> | undefined;
  let generation = 0;
  const current = () => services.auth.currentUser === user && services.authScope === scope && scope.id === scopeId && scope.persistent === persistent;
  function age() { return sample ? performance.now() - sample.received : Infinity; }
  function usable() { const elapsed = age(); return current() && !!sample && elapsed >= 0 && elapsed < MAX_SAMPLE_AGE_MS; }
  function readTime(value: unknown): number {
    if (typeof value !== "string" || !/^20\d{2}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$/.test(value)) throw unavailable();
    const parsed = Date.parse(value);
    if (!Number.isSafeInteger(parsed)) throw unavailable();
    return parsed;
  }
  function tokenForDestination(token: string) {
    if (token.length > 16_384 || !/^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]*$/.test(token)) throw unavailable();
    try {
      const raw = token.split(".")[1].replaceAll("-", "+").replaceAll("_", "/");
      const payload: unknown = JSON.parse(atob(raw.padEnd(Math.ceil(raw.length / 4) * 4, "=")));
      if (!object(payload) || payload.aud !== project || payload.iss !== `https://securetoken.google.com/${project}` || payload.sub !== user!.uid) throw unavailable();
    } catch { throw unavailable(); }
  }
  async function fetchSample() {
    const requestGeneration = ++generation;
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
    const unsubscribe = services.auth.onAuthStateChanged(next => { if (next !== user) controller.abort(); });
    const check = () => { if (!current() || controller.signal.aborted || generation !== requestGeneration) throw unavailable(); };
    let rejectAbort: (() => void) | undefined;
    const aborted = new Promise<never>((_, reject) => { rejectAbort = () => reject(unavailable()); controller.signal.addEventListener("abort", rejectAbort, { once: true }); });
    try {
      check();
      let token = await Promise.race([user!.getIdToken(), aborted]);
      check(); tokenForDestination(token);
      const request = () => fetch(url, { method: "POST", headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
        body: JSON.stringify({ documents: [document], mask: { fieldPaths: ["uid"] } }), cache: "no-store", credentials: "omit", redirect: "error", signal: controller.signal });
      let sent = performance.now();
      let response = await request();
      check();
      if (response.status === 401) {
        // A clock far behind can keep an expired token in Firebase's cache.
        // Refresh once within this same original timeout/account, never loop.
        await response.body?.cancel();
        token = await Promise.race([user!.getIdToken(true), aborted]);
        check(); tokenForDestination(token); sent = performance.now();
        response = await request();
      }
      check();
      if (!response.ok || !response.body || Number(response.headers.get("Content-Length") ?? 0) > MAX_RESPONSE_BYTES) throw unavailable();
      const reader = response.body.getReader();
      const chunks: Uint8Array[] = []; let size = 0;
      try {
        while (true) {
          const part = await reader.read(); check(); if (part.done) break;
          size += part.value.byteLength;
          if (size > MAX_RESPONSE_BYTES) { controller.abort(); throw unavailable(); }
          chunks.push(part.value);
        }
      } finally { reader.releaseLock(); }
      const received = performance.now(), rtt = received - sent;
      if (!Number.isFinite(rtt) || rtt < 0 || rtt > REQUEST_TIMEOUT_MS) throw unavailable();
      const bytes = new Uint8Array(size); let offset = 0;
      for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
      const result: unknown = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
      if (!Array.isArray(result) || result.length !== 1 || !object(result[0])) throw unavailable();
      const entry = result[0];
      if (Object.keys(entry).some(key => !["found", "readTime"].includes(key)) || !object(entry.found) || entry.found.name !== document) throw unavailable();
      const serverTime = readTime(entry.readTime);
      check(); sample = { readTime: serverTime, received, rtt };
    } catch { throw unavailable(); }
    finally { clearTimeout(timer); unsubscribe(); if (rejectAbort) controller.signal.removeEventListener("abort", rejectAbort); }
  }
  return {
    async ensure(check: () => void) {
      check(); if (!current()) throw unavailable();
      if (!usable() || age() >= REFRESH_AFTER_MS) {
        pending ??= fetchSample().finally(() => { pending = undefined; });
        await pending; check();
      }
      if (!usable()) throw unavailable();
    },
    /** Upper bound for expiration checks. Missing/stale samples are not a local-clock fallback. */
    now() { return usable() ? sample!.readTime + age() + sample!.rtt + 1 : Number.NaN; },
    /** Lower bound for bounded TTL writes; preserve the caller's existing TTL. */
    writeTime() { if (!usable()) throw unavailable(); return Math.floor(sample!.readTime + age() - 1); },
  };
}
