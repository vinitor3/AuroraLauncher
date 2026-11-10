// Test-run ownership only. No CLI refresh/cache writes or rule changes; Auth deletion is recovery-only under a reviewed plan.
import { createHash } from "node:crypto";
import { lstat, mkdir, open, readFile, readdir, realpath } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const project = "auroralauncher";
const base = `https://firestore.googleapis.com/v1/projects/${project}/databases/(default)/documents/`;
const identity = `https://identitytoolkit.googleapis.com/v1/projects/${project}/accounts:lookup`;
const evidenceRoot = fileURLToPath(new URL("../.aurora/connect-validation/", import.meta.url));
const permissions = ["datastore.entities.get", "datastore.entities.delete", "firebaseauth.users.get", "firebaseauth.users.delete"];
const fail = code => { throw Object.assign(new Error(code), { code }); };
const hash = value => createHash("sha256").update(value).digest("hex");
const uuid = value => typeof value === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(value);
const uid = value => typeof value === "string" && /^[A-Za-z0-9_-]{20,128}$/.test(value);
const exact = (value, fields) => !!value && typeof value === "object" && !Array.isArray(value)
  && Object.keys(value).length === fields.length && fields.every(field => Object.hasOwn(value, field));
const stamp = value => typeof value === "string" && /^20\d\d-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d{1,9})?Z$/.test(value) && Number.isFinite(Date.parse(value));
function canonical(value) {
  return JSON.stringify(value && typeof value === "object" ? Array.isArray(value) ? value.map(item => JSON.parse(canonical(item)))
    : Object.fromEntries(Object.keys(value).sort().map(key => [key, JSON.parse(canonical(value[key]))])) : value);
}
function decode(value) {
  if (!value || Object.keys(value).length !== 1) fail("owned_cleanup_shape_invalid");
  if (Object.hasOwn(value, "nullValue") && value.nullValue === null) return null;
  if (Object.hasOwn(value, "stringValue") && typeof value.stringValue === "string") return value.stringValue;
  if (Object.hasOwn(value, "timestampValue") && stamp(value.timestampValue)) return value.timestampValue;
  if (Object.hasOwn(value, "integerValue") && /^\d{1,16}$/.test(value.integerValue) && Number.isSafeInteger(Number(value.integerValue))) return Number(value.integerValue);
  if (Object.hasOwn(value, "mapValue") && exact(value.mapValue, ["fields"])) return fields(value.mapValue.fields);
  if (Object.hasOwn(value, "arrayValue") && exact(value.arrayValue, ["values"]) && Array.isArray(value.arrayValue.values)) return value.arrayValue.values.map(decode);
  fail("owned_cleanup_shape_invalid");
}
const fields = value => value && typeof value === "object" && !Array.isArray(value)
  ? Object.fromEntries(Object.entries(value).map(([key, item]) => [key, decode(item)])) : fail("owned_cleanup_shape_invalid");

async function boundedJson(response) {
  let text = "", size = 0;
  for await (const chunk of response.body ?? []) {
    size += chunk.byteLength; if (size > 65_536) fail("owned_cleanup_response_invalid");
    text += Buffer.from(chunk).toString("utf8");
  }
  try { return text ? JSON.parse(text) : {}; } catch { fail("owned_cleanup_response_invalid"); }
}

async function safeDirectory(path) {
  if ((await realpath(path)) !== path) fail("owned_cleanup_directory_invalid");
  let at = path;
  while (true) {
    const stat = await lstat(at); if (!stat.isDirectory() || stat.isSymbolicLink()) fail("owned_cleanup_directory_invalid");
    const parent = dirname(at); if (parent === at) break; at = parent;
  }
}
function scopedRoot(rootDirectory, root) {
  const target = resolve(rootDirectory), inside = relative(resolve(root), target);
  if (!inside || isAbsolute(inside) || inside.startsWith(`..${sep}`) || inside === ".." || resolve(root, inside) !== target) fail("owned_cleanup_directory_invalid");
  return target;
}

/** dependencies is a unit-test seam; the runner uses the fixed production defaults. */
async function createOwner({ runId, rootDirectory, expectedOperator }, dependencies = {}, restoration) {
  if (!uuid(runId)) fail("owned_cleanup_run_invalid");
  if (typeof expectedOperator !== "string" || expectedOperator.length > 254 || !/^[^\s@]{1,64}@[A-Za-z0-9.-]{1,190}$/.test(expectedOperator)) fail("owned_cleanup_operator_required");
  const target = scopedRoot(rootDirectory, dependencies.evidenceRoot ?? evidenceRoot);
  const fetcher = dependencies.fetch ?? globalThis.fetch, now = dependencies.now ?? Date.now;
  await safeDirectory(target);
  const directory = join(target, "owned-cleanup"); if (!restoration) await mkdir(directory); await safeDirectory(directory);
  const accounts = ["host", "guest"].map(role => {
    const nickname = `cp${runId.replaceAll("-", "").slice(0, 12)}${role[0]}`;
    return Object.freeze({ role, nickname, email: `${nickname}@aurora.internal` });
  });
  const sessionDraft = Object.freeze({ title: `Connect private smoke ${runId.slice(0, 8)}`, modpackName: "Disposable TCP validation",
    minecraftVersion: "1.21.1", loader: "Fabric", imageUrl: null, players: 0, maxPlayers: 2, visibility: "private", joinPolicy: "open" });
  const owned = new Map(), snapshots = new Map(), receipts = restoration?.receipts ?? [];
  let sequence = receipts.length, lastHash = restoration?.journalHash ?? null, access, accessExpiry = 0, refresh, ready = false, sessionId, sessionExpiry, deleted = 0, cleaned = false;
  let lastServerTime, preflightTime;
  const authDeleted = new Set(), accountReceiptsPending = new Set(), creationStarted = new Set();
  async function journal(event, detail = {}) {
    await safeDirectory(directory);
    const bytes = JSON.stringify({ schema: 1, runId, sequence, previous: lastHash, event, ...detail }) + "\n";
    const path = join(directory, `${String(sequence++).padStart(3, "0")}.json`);
    const handle = await open(path, "wx", 0o600);
    try { await handle.writeFile(bytes); await handle.sync(); } finally { await handle.close(); }
    lastHash = hash(bytes); receipts.push({ path, hash: lastHash });
  }
  async function verifyJournal() {
    await safeDirectory(directory);
    for (const item of receipts) {
      const stat = await lstat(item.path);
      if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1 || stat.size > 65_536 || hash(await readFile(item.path)) !== item.hash) fail("owned_cleanup_journal_changed");
    }
  }
  if (!restoration) await journal("plan", { project, accounts, authRecovery: "scoped_lookup_without_cas", sessionDraft });
  async function raw(url, options = {}) {
    try { return await fetcher(url, { ...options, redirect: "error", signal: AbortSignal.timeout(15_000) }); }
    catch { fail("owned_cleanup_network_failed"); }
  }
  async function token() {
    if (access && accessExpiry > now() + 15_000) return access;
    if (typeof refresh !== "string" || !refresh || refresh.length > 16_384) fail("owned_cleanup_credentials_unavailable");
    // Public installed-app OAuth client shipped by Firebase CLI; never a user credential.
    const body = new URLSearchParams({ grant_type: "refresh_token", refresh_token: refresh,
      client_id: "563584335869-fgrhgmd47bqnekij5i8b5pr03ho849e6.apps.googleusercontent.com", client_secret: "j9iVZfS8kkCEFUPaAeJV0sAi" });
    const response = await raw("https://oauth2.googleapis.com/token", { method: "POST", body });
    if (!response.ok) fail("owned_cleanup_credentials_unavailable");
    const result = await boundedJson(response);
    if (typeof result.access_token !== "string" || !result.access_token || result.access_token.length > 16_384
      || !Number.isFinite(result.expires_in) || result.expires_in <= 30 || result.expires_in > 86_400) fail("owned_cleanup_credentials_unavailable");
    access = result.access_token; accessExpiry = now() + result.expires_in * 1000; return access;
  }
  async function request(url, method = "GET", body) {
    const response = await raw(url, { method, headers: { Authorization: `Bearer ${await token()}`, ...(body ? { "Content-Type": "application/json" } : {}) }, ...(body ? { body: JSON.stringify(body) } : {}) });
    lastServerTime = Date.parse(response.headers.get("date") ?? "");
    if (!Number.isFinite(lastServerTime)) fail("owned_cleanup_server_time_unavailable");
    if (response.status === 404 && method === "GET") return null;
    if (!response.ok) fail(`owned_cleanup_http_${response.status}`);
    return boundedJson(response);
  }
  const document = path => request(base + path);
  const lookup = input => request(identity, "POST", input);
  const assertReady = () => { if (!ready || cleaned) fail("owned_cleanup_not_ready"); };
  function paths() {
    const entries = [...owned.values()].flatMap(account => [`users/${account.uid}`, `publicProfiles/${account.uid}`, `usernames/${account.nickname}`]);
    const host = owned.get("host"), guest = owned.get("guest");
    if (host) entries.push(`socialOwners/${host.uid}`, `socialPrivateThrottle/${host.uid}`);
    if (sessionId) entries.push(`socialSessions/${sessionId}`, `socialPrivateAudiences/${sessionId}`);
    if (sessionId && guest) entries.push(`socialPrivateInvitations/${sessionId}/guests/${guest.uid}`, `socialPrivateInbox/${guest.uid}/sessions/${sessionId}`);
    return entries;
  }
  function validate(path, rawDocument) {
    if (!rawDocument) return null;
    if (rawDocument.name !== `projects/${project}/databases/(default)/documents/${path}` || !stamp(rawDocument.createTime) || !stamp(rawDocument.updateTime)) fail("owned_cleanup_document_invalid");
    const value = fields(rawDocument.fields), [collection, id] = path.split("/"), host = owned.get("host"), guest = owned.get("guest");
    const account = [...owned.values()].find(item => item.uid === id || item.nickname === id);
    const matches = (expected, extras = []) => exact(value, [...Object.keys(expected), ...extras]) && Object.entries(expected).every(([key, wanted]) => canonical(value[key]) === canonical(wanted));
    let valid = false;
    if (collection === "users" && account) valid = matches({ uid: account.uid, username: account.nickname, usernameLower: account.nickname, email: account.email,
      avatarUrl: null, skinUrl: null, capeUrl: null, skinModel: "classic", role: "PLAYER", stats: { totalPlayTimeSeconds: 0, questsCompleted: 0, lastPlayedModpack: null } }, ["createdAt"]) && stamp(value.createdAt);
    if (collection === "publicProfiles" && account) valid = matches({ uid: account.uid, username: account.nickname, avatarUrl: null, skinUrl: null, skinModel: "classic" }, ["updatedAt"]) && stamp(value.updatedAt);
    if (collection === "usernames" && account) valid = matches({ uid: account.uid, usernameLower: account.nickname }, ["createdAt"]) && stamp(value.createdAt);
    if (collection === "socialOwners") valid = matches({ sessionId, visibility: "private" });
    if (collection === "socialPrivateThrottle") valid = matches({}, ["at"]) && stamp(value.at);
    if (collection === "socialSessions") valid = matches({ ...sessionDraft, schema: 1, hostUid: host?.uid }, ["expiresAt"]) && stamp(value.expiresAt);
    if (collection === "socialPrivateAudiences") valid = matches({ schema: 1, sessionId, hostUid: host?.uid, expiresAt: sessionExpiry, recipients: [guest?.uid] });
    if (collection === "socialPrivateInbox") valid = matches({ schema: 1, sessionId, hostUid: host?.uid, expiresAt: sessionExpiry });
    if (collection === "socialPrivateInvitations") valid = matches({ schema: 1, sessionId, hostUid: host?.uid, recipientUid: guest?.uid, expiresAt: sessionExpiry }, ["authorizationId", "revision", "status", "createdAt", "updatedAt"])
      && /^[0-9a-f]{64}$/.test(value.authorizationId) && stamp(value.createdAt) && stamp(value.updatedAt)
      && (value.status === "active" && value.revision === 1 || value.status === "revoked" && value.revision === 2);
    if (!valid) fail("owned_cleanup_shape_invalid");
    return { hash: hash(canonical(rawDocument.fields)), updateTime: rawDocument.updateTime, createTime: rawDocument.createTime, value };
  }
  const immutableHash = value => { const selected = { ...value }; for (const key of ["status", "revision", "updatedAt"]) delete selected[key]; return hash(canonical(selected)); };
  const priorDocuments = new Map(), previouslyDeleted = new Set();
  if (restoration) {
    const plan = restoration.entries[0];
    if (canonical(plan.accounts) !== canonical(accounts) || canonical(plan.sessionDraft) !== canonical(sessionDraft) || plan.project !== project) fail("owned_cleanup_journal_invalid");
    for (const entry of restoration.entries) {
      if (entry.event === "preflight") preflightTime ??= entry.serverTime;
      if (entry.event === "account_create_started") { creationStarted.add(entry.role); accountReceiptsPending.add(entry.role); }
      if (["account_create_reported", "account_created"].includes(entry.event)) {
        const planned = accounts.find(item => item.role === entry.role);
        if (!planned || !uid(entry.uid) || entry.email !== planned.email || !Number.isSafeInteger(entry.createdAt)) fail("owned_cleanup_journal_invalid");
        const previous = owned.get(entry.role);
        if (previous && previous.uid !== entry.uid) fail("owned_cleanup_journal_invalid");
        owned.set(entry.role, { ...planned, uid: entry.uid, createdAt: entry.createdAt, confirmed: entry.event === "account_created" });
      }
      if (entry.event === "session_created") { if (!uuid(entry.sessionId) || !stamp(entry.expiresAt)) fail("owned_cleanup_journal_invalid"); sessionId = entry.sessionId; sessionExpiry = entry.expiresAt; }
      if (entry.event === "documents_observed") for (const item of entry.observations ?? []) if (item.present) priorDocuments.set(item.path, item);
      if (entry.event === "document_deleted") previouslyDeleted.add(entry.path);
      if (["auth_deleted_by_sdk", "auth_deleted_by_recovery"].includes(entry.event)) authDeleted.add(entry.role);
    }
    if (!Number.isSafeInteger(preflightTime) || creationStarted.size !== owned.size) fail("owned_cleanup_unknown_created_account");
  }
  return {
    accounts, sessionDraft, directory,
    async preflight() {
      if (ready) fail("owned_cleanup_already_ready");
      let cache;
      try { cache = dependencies.readCache ? await dependencies.readCache() : JSON.parse(await readFile(join(homedir(), ".config/configstore/firebase-tools.json"), "utf8")); }
      catch { fail("owned_cleanup_credentials_unavailable"); }
      if (cache?.user?.email !== expectedOperator) fail("owned_cleanup_account_mismatch");
      refresh = cache.tokens?.refresh_token; access = cache.tokens?.access_token; accessExpiry = cache.tokens?.expires_at ?? 0;
      const user = await request("https://openidconnect.googleapis.com/v1/userinfo");
      if (user?.email !== expectedOperator || user.email_verified !== true) fail("owned_cleanup_account_mismatch");
      const requiredPermissions = permissions;
      const allowed = await request(`https://cloudresourcemanager.googleapis.com/v1/projects/${project}:testIamPermissions`, "POST", { permissions: requiredPermissions });
      if (!Array.isArray(allowed?.permissions) || !requiredPermissions.every(permission => allowed.permissions.includes(permission))) fail("owned_cleanup_permissions_missing");
      if (!restoration) for (const account of accounts) {
        const existing = await lookup({ email: [account.email] });
        if (existing.users?.length || await document(`usernames/${account.nickname}`)) fail("owned_cleanup_account_exists");
      }
      if (!restoration) { preflightTime = lastServerTime; await journal("preflight", { accountVerified: true, permissionsVerified: true, serverTime: preflightTime }); }
      ready = true;
    },
    async recordAccountCreateStarted(role) {
      assertReady(); if (!accounts.some(item => item.role === role) || creationStarted.has(role)) fail("owned_cleanup_account_invalid");
      creationStarted.add(role); accountReceiptsPending.add(role);
      await journal("account_create_started", { role });
    },
    async recordAccount({ role, uid: accountUid, email, createdAt }) {
      assertReady(); const planned = accounts.find(item => item.role === role);
      if (planned && !owned.has(role)) accountReceiptsPending.add(role);
      const creation = typeof createdAt === "number" ? createdAt : Date.parse(createdAt);
      if (!planned || !creationStarted.has(role) || owned.has(role) || !uid(accountUid) || [...owned.values()].some(item => item.uid === accountUid) || email !== planned.email || !Number.isSafeInteger(creation) || creation <= 0) fail("owned_cleanup_account_invalid");
      await journal("account_create_reported", { role, uid: accountUid, email, createdAt: creation });
      const found = await lookup({ localId: [accountUid] }); const account = found.users?.[0];
      const serverCreatedAt = Number(account?.createdAt);
      // Firebase UserMetadata.creationTime is second-precision UTC text. The
      // administrative timestamp is retained exactly and bounded by HTTPS time.
      if (found.users?.length !== 1 || account.localId !== accountUid || account.email !== email || !Number.isSafeInteger(serverCreatedAt)
        || Math.floor(serverCreatedAt / 1000) !== Math.floor(creation / 1000)
        || serverCreatedAt < preflightTime - 1000 || serverCreatedAt > lastServerTime + 1000) fail("owned_cleanup_account_invalid");
      // Save the positive SDK-created identity before any registration document is written.
      owned.set(role, { ...planned, uid: accountUid, createdAt: serverCreatedAt });
      await journal("account_created", { role, uid: accountUid, email, createdAt: serverCreatedAt });
      for (const path of [`users/${accountUid}`, `publicProfiles/${accountUid}`, `usernames/${planned.nickname}`,
        ...(role === "host" ? [`socialOwners/${accountUid}`, `socialPrivateThrottle/${accountUid}`] : [])]) {
        if (await document(path)) fail("owned_cleanup_document_exists"); snapshots.set(path, null);
      }
      accountReceiptsPending.delete(role);
    },
    async recordSession(id) {
      assertReady(); if (sessionId || !uuid(id) || owned.size !== 2) fail("owned_cleanup_session_invalid");
      sessionId = id;
      const rawSession = await document(`socialSessions/${id}`), observed = validate(`socialSessions/${id}`, rawSession);
      if (!observed) fail("owned_cleanup_session_invalid"); sessionExpiry = observed.value.expiresAt;
      await journal("session_created", { sessionId, expiresAt: sessionExpiry, fieldsHash: observed.hash, updateTime: observed.updateTime });
      await this.snapshotDocuments();
    },
    async snapshotDocuments() {
      assertReady(); await verifyJournal();
      const observations = [];
      for (const path of paths()) {
        const rawDocument = await document(path), value = validate(path, rawDocument), prior = snapshots.get(path);
        if (prior && value && prior.createTime !== value.createTime) fail("owned_cleanup_document_changed");
        if (prior && value && prior.hash !== value.hash) {
          const before = { ...prior.value }, after = { ...value.value };
          for (const key of ["status", "revision", "updatedAt"]) { delete before[key]; delete after[key]; }
          if (!path.startsWith("socialPrivateInvitations/") || prior.value.status !== "active" || value.value.status !== "revoked"
            || canonical(before) !== canonical(after)) fail("owned_cleanup_document_changed");
        }
        snapshots.set(path, value); observations.push({ path, present: !!value, ...(value ? { fieldsHash: value.hash, updateTime: value.updateTime, createTime: value.createTime, immutableHash: immutableHash(value.value) } : {}) });
      }
      await journal("documents_observed", { observations });
    },
    async cleanupDocuments({ sessionStopped, pendingMutationsSettled } = {}) {
      assertReady(); if (sessionStopped !== true || pendingMutationsSettled !== true) fail("owned_cleanup_not_settled");
      if (accountReceiptsPending.size) fail("owned_cleanup_account_unconfirmed");
      await verifyJournal();
      // Read-only verification of the entire immutable allowlist precedes the first delete.
      const removals = [];
      for (const path of paths()) {
        if (!snapshots.has(path)) fail("owned_cleanup_snapshot_missing");
        const current = validate(path, await document(path)), expected = snapshots.get(path);
        if (!!current !== !!expected || current && (current.hash !== expected.hash || current.updateTime !== expected.updateTime)) fail("owned_cleanup_document_changed");
        if (current) removals.push({ path, current });
      }
      for (const { path, current } of removals.reverse()) {
        await journal("delete_planned", { path, fieldsHash: current.hash, updateTime: current.updateTime });
        await request(`${base}${path}?currentDocument.updateTime=${encodeURIComponent(current.updateTime)}`, "DELETE");
        snapshots.set(path, null); deleted++; await journal("document_deleted", { path });
      }
      cleaned = true; await journal("documents_clean", { removedDocuments: deleted });
      return { passed: true, removedDocuments: deleted };
    },
    async recordAuthDeleted(role) {
      if (!cleaned || !owned.has(role) || authDeleted.has(role)) fail("owned_cleanup_auth_unconfirmed");
      authDeleted.add(role); await journal("auth_deleted_by_sdk", { role, uid: owned.get(role).uid });
    },
    async prepareRecovery() {
      if (!restoration || !ready || cleaned) fail("owned_cleanup_not_ready");
      await verifyJournal(); const identities = [];
      for (const account of owned.values()) {
        const response = await lookup({ localId: [account.uid] }), current = response.users?.[0];
        if (current && (response.users.length !== 1 || authDeleted.has(account.role) || current.localId !== account.uid || current.email !== account.email
          || !Number.isSafeInteger(Number(current.createdAt)) || Number(current.createdAt) < preflightTime - 1000 || Number(current.createdAt) > preflightTime + 600_000
          || (account.confirmed ? Number(current.createdAt) !== account.createdAt : Math.floor(Number(current.createdAt) / 1000) !== Math.floor(account.createdAt / 1000)))) fail("owned_cleanup_account_changed");
        identities.push({ role: account.role, uid: account.uid, email: account.email, createdAt: current ? Number(current.createdAt) : account.createdAt, deleteAuth: !!current });
        if (!current) authDeleted.add(account.role);
        accountReceiptsPending.delete(account.role);
      }
      if (!sessionId && owned.has("host")) {
        const rawOwner = await document(`socialOwners/${owned.get("host").uid}`);
        if (rawOwner) {
          const value = fields(rawOwner.fields);
          if (!exact(value, ["sessionId", "visibility"]) || value.visibility !== "private" || !uuid(value.sessionId)) fail("owned_cleanup_session_invalid");
          sessionId = value.sessionId;
          const observed = validate(`socialSessions/${sessionId}`, await document(`socialSessions/${sessionId}`));
          if (!observed) fail("owned_cleanup_session_invalid"); sessionExpiry = observed.value.expiresAt;
        }
      }
      const documents = [];
      for (const path of paths()) {
        const current = validate(path, await document(path)), previous = priorDocuments.get(path);
        if (current && previouslyDeleted.has(path)) fail("owned_cleanup_document_changed");
        if (current && previous && (previous.createTime !== current.createTime
          || previous.fieldsHash !== current.hash && !(path.startsWith("socialPrivateInvitations/") && current.value.status === "revoked" && previous.immutableHash === immutableHash(current.value)))) fail("owned_cleanup_document_changed");
        snapshots.set(path, current);
        documents.push({ path, present: !!current, ...(current ? { fieldsHash: current.hash, updateTime: current.updateTime } : {}) });
      }
      const plan = { schema: 1, runId, journalHash: restoration.journalHash, accounts: identities, documents, authDeleteIsConditional: false };
      return { plan, planHash: hash(canonical(plan)) };
    },
    async executeRecovery({ planHash, sessionStopped, pendingMutationsSettled }) {
      const reviewed = await this.prepareRecovery();
      if (reviewed.planHash !== planHash) fail("owned_cleanup_recovery_plan_changed");
      await this.cleanupDocuments({ sessionStopped, pendingMutationsSettled });
      for (const account of reviewed.plan.accounts.filter(item => item.deleteAuth)) {
        // Identity Platform has no compare-and-delete API. Recheck immediately;
        // this remaining race is explicit in the approved recovery plan.
        const response = await lookup({ localId: [account.uid] }), current = response.users?.[0];
        if (response.users?.length !== 1 || current.localId !== account.uid || current.email !== account.email || Number(current.createdAt) !== account.createdAt) fail("owned_cleanup_account_changed");
        await request(identity.replace("accounts:lookup", "accounts:delete"), "POST", { localId: account.uid });
        authDeleted.add(account.role); await journal("auth_deleted_by_recovery", { role: account.role, uid: account.uid });
      }
      return this.result();
    },
    result() { return { accountsCreated: owned.size, authDeleted: authDeleted.size, removedDocuments: deleted,
      documentsCleaned: cleaned, unconfirmedAccounts: accountReceiptsPending.size,
      complete: cleaned && !accountReceiptsPending.size && authDeleted.size === owned.size, journalHash: lastHash, authRecovery: "scoped_lookup_without_cas" }; },
  };
}

export const createCleanupOwner = (options, dependencies) => createOwner(options, dependencies);

/** No arbitrary UID or document path is accepted. Preparation performs reads only. */
export async function prepareOwnedRecovery({ rootDirectory, journalHash, expectedOperator }, dependencies = {}) {
  const target = scopedRoot(rootDirectory, dependencies.evidenceRoot ?? evidenceRoot), directory = join(target, "owned-cleanup");
  await safeDirectory(directory);
  if (!/^[0-9a-f]{64}$/.test(journalHash)) fail("owned_cleanup_journal_invalid");
  const files = await readdir(directory), entries = [], receipts = [];
  if (files.some(name => name !== "result.json" && !/^\d{3}\.json$/.test(name))) fail("owned_cleanup_journal_invalid");
  // The runner's redacted outcome is observational, never ownership authority.
  const names = files.filter(name => name !== "result.json").sort();
  if (!names.length || names.length > 128) fail("owned_cleanup_journal_invalid");
  let previous = null, runId;
  for (const [index, name] of names.entries()) {
    if (name !== `${String(index).padStart(3, "0")}.json`) fail("owned_cleanup_journal_invalid");
    const path = join(directory, name), stat = await lstat(path);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1 || stat.size > 65_536) fail("owned_cleanup_journal_invalid");
    const bytes = await readFile(path); let entry; try { entry = JSON.parse(bytes); } catch { fail("owned_cleanup_journal_invalid"); }
    runId ??= entry.runId;
    if (!uuid(runId) || entry.runId !== runId || entry.schema !== 1 || entry.sequence !== index || entry.previous !== previous || index === 0 && entry.event !== "plan") fail("owned_cleanup_journal_invalid");
    previous = hash(bytes); entries.push(entry); receipts.push({ path, hash: previous });
  }
  if (previous !== journalHash) fail("owned_cleanup_journal_changed");
  const owner = await createOwner({ runId, rootDirectory: target, expectedOperator }, dependencies, { entries, receipts, journalHash });
  await owner.preflight(); const prepared = await owner.prepareRecovery();
  return { ...prepared, execute: options => owner.executeRecovery(options) };
}
