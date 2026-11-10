// Synthetic DOM only. Real Firestore authorization is exercised separately.
import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";
const require = createRequire(import.meta.url);
const { window, document } = require("linkedom").parseHTML("<html><body><div id='root'></div></body></html>");
document.oninput = null;
Object.defineProperty(window.HTMLInputElement.prototype, "type", { configurable: true, get() { return this.getAttribute("type") || "text"; }, set(value) { this.setAttribute("type", value); } });
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Synthetic private invitations" } });
const modules = new Map();
async function sourceModule(source) {
  if (modules.has(source.href)) return modules.get(source.href);
  let output = ts.transpileModule(await readFile(source, "utf8"), { fileName: fileURLToPath(source), compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX,
  } }).outputText.replace(/^import ["'][^"']+\.css["'];?$/gm, "");
  for (const { fileName: specifier } of ts.preProcessFile(output).importedFiles) {
    const dependency = specifier.startsWith(".") ? ["", ".ts", ".tsx"].map(ext => new URL(specifier + ext, source)).find(url => existsSync(fileURLToPath(url))) : undefined;
    const resolved = dependency ? await sourceModule(dependency) : pathToFileURL(require.resolve(specifier)).href;
    output = output.replaceAll(`from "${specifier}"`, `from ${JSON.stringify(resolved)}`);
  }
  const url = `data:text/javascript;base64,${Buffer.from(output).toString("base64")}`;
  modules.set(source.href, url); return url;
}
const { act, createElement } = await import("react");
const { createRoot } = await import("react-dom/client");
const { SocialView } = await import(await sourceModule(new URL("./SocialView.tsx", import.meta.url)));
const now = 1_800_000_000_000, host = { uid: "host", nickname: "Aurora_Host", avatarUrl: null }, peer = { uid: "guest", nickname: "Aurora_Guest", avatarUrl: null };
const session = { id: "aaaaaaaa-1111-4111-8111-111111111111", hostUid: host.uid, title: "Mundo privado", modpackName: "Meu Pack", minecraftVersion: "1.21.1", loader: "Fabric", imageUrl: null, players: 0, maxPlayers: 8, visibility: "private", joinPolicy: "open", expiresAtMs: now + 60_000 };
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { resolve, reject, promise }; };
function fixture(own = false) {
  const f = { now, calls: [], snapshot: { friends: [], incoming: [], outgoing: [], blocked: [], ownSession: own ? session : null, presenceSharing: "hidden", modpackName: null },
    privateSnapshot: { incoming: own ? [] : [{ session, host, expiresAtMs: session.expiresAtMs }], outgoing: [], sessionId: own ? session.id : null, expiresAtMs: own ? session.expiresAtMs : null, remainingRecipients: 8 } };
  const record = (name, ...args) => f.calls.push({ name, args });
  f.service = { capabilities: { friends: true, catalog: true, publication: true, join: true }, now: () => f.now,
    snapshot: async () => structuredClone(f.snapshot), listPublicSessions: async () => ({ items: [], nextCursor: null }),
    searchByNickname: async nick => { record("search", nick); return f.search ? f.search() : peer; },
    prepareJoin: async id => { record("join", id); if (f.join) await f.join(); },
    privateInvitations: {
      snapshot: async () => { record("privateSnapshot"); return f.read ? f.read() : structuredClone(f.privateSnapshot); },
      invite: async (...args) => { record("invite", ...args); if (f.invite) await f.invite(); else f.privateSnapshot.outgoing = [{ person: peer, status: "active" }]; },
      revoke: async (...args) => { record("revoke", ...args); f.privateSnapshot.outgoing = [{ person: peer, status: "revoked" }]; },
      decline: async (...args) => { record("decline", ...args); f.privateSnapshot.incoming = []; },
    } };
  f.connection = { state: { available: true, pending: false, error: "", privateInvitations: true, snapshot: { operationId: null, phase: "idle", cleanupState: "notStarted" } },
    controller: { prepareHost: async (...args) => record("prepareHost", ...args), refresh: async () => {} } };
  return f;
}
const container = document.getElementById("root");
let root;
const render = async (f, props = {}) => act(async () => root.render(createElement(SocialView, { service: f.service, connection: f.connection, accountId: "account", nickname: "Account", ...props })));
const buttons = label => [...container.querySelectorAll("button")].filter(item => item.textContent === label);
const button = label => { const found = buttons(label)[0]; assert.ok(found, label); return found; };
const click = async label => { assert.equal(button(label).disabled, false, label); await act(async () => button(label).dispatchEvent(new window.Event("click", { bubbles: true }))); };
const count = (f, name) => f.calls.filter(call => call.name === name).length;
const search = async (nick = peer.nickname) => {
  const input = container.querySelector(".social-private-search input"); assert.ok(input);
  await act(async () => { Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set.call(input, nick); input.dispatchEvent(new window.Event("input", { bubbles: true })); });
  await act(async () => container.querySelector(".social-private-search").dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true })));
};
beforeEach(() => { root = createRoot(container); });
afterEach(async () => act(async () => root.unmount()));

test("inbox discovery performs no admission and explicit Enter reuses the original session join", async () => {
  const f = fixture(); await render(f);
  assert.match(container.textContent, /Mundo privado/); assert.match(container.textContent, /Aurora_Host/);
  assert.equal(count(f, "join"), 0); assert.equal(count(f, "invite"), 0);
  await click("Entrar"); assert.deepEqual(f.calls.find(call => call.name === "join").args, [session.id]);
  assert.equal(count(f, "join"), 1); assert.equal(count(f, "prepareHost"), 0);
});
test("a remote service without the optional inbox never exposes or reads the local private feature", async () => {
  const f = fixture(); delete f.service.privateInvitations; await render(f);
  assert.equal(container.querySelector(".social-private"), null); assert.equal(count(f, "privateSnapshot"), 0);
  assert.match(container.textContent, /ainda não há entrega de convite para uma conta Aurora/);
});
test("private Enter requires native capability, a free slot and service join support", async () => {
  for (const change of [f => delete f.connection.state.privateInvitations, f => { f.service.capabilities.join = false; }, f => { f.connection.state.available = false; }, f => { f.connection.state.pending = true; },
    f => { f.connection.state.error = "Estado desatualizado"; }]) {
    const f = fixture(); change(f); await render(f); assert.equal(button("Entrar").disabled, true);
    await act(async () => button("Entrar").dispatchEvent(new window.Event("click", { bubbles: true })));
    assert.equal(count(f, "join"), 0);
  }
});
test("expiry is rechecked at click and declining never joins or grants friendship", async () => {
  const f = fixture(); await render(f); f.now = session.expiresAtMs;
  await click("Entrar"); assert.equal(count(f, "join"), 0);
  f.now = now; await click("Recusar convite");
  assert.deepEqual(f.calls.find(call => call.name === "decline").args, [session.id]);
  assert.match(container.textContent, /Convite recusado/); assert.equal(buttons("Entrar").length, 0);
});
test("host lookup is exact and inviting requires a separate explicit recipient action", async () => {
  const f = fixture(true); await render(f); await search();
  assert.deepEqual(f.calls.find(call => call.name === "search").args, [peer.nickname]);
  assert.equal(count(f, "invite"), 0);
  await click(`Convidar ${peer.nickname}`);
  assert.deepEqual(f.calls.find(call => call.name === "invite").args, [session.id, peer.uid]);
  assert.match(container.textContent, /Convite ativo/); assert.equal(count(f, "prepareHost"), 0);
  await click("Revogar convite"); assert.deepEqual(f.calls.find(call => call.name === "revoke").args, [session.id, peer.uid]);
  await click("Reenviar convite"); assert.equal(count(f, "invite"), 2);
  assert.match(container.textContent, /prazo original/); assert.equal(count(f, "join"), 0);
});
test("private hosting requires the SDK feature plus native capability and remains a separate approval flow", async () => {
  const f = fixture(true); await render(f); await click("Preparar meu servidor");
  assert.deepEqual(f.calls.find(call => call.name === "prepareHost").args, [session.id, "private"]);
  delete f.connection.state.privateInvitations; await render(f); assert.equal(button("Preparar meu servidor").disabled, true);
});
test("lifetime audience limit blocks a new recipient but preserves explicit reissue", async () => {
  const f = fixture(true); f.privateSnapshot.remainingRecipients = 0; await render(f); await search();
  assert.equal(button(`Convidar ${peer.nickname}`).disabled, true);
  f.privateSnapshot.outgoing = [{ person: peer, status: "revoked" }]; await click("Atualizar convites");
  assert.equal(button("Reenviar convite").disabled, false);
  assert.match(container.textContent, /Revogar não libera esse limite/);
});
test("one pending invitation cannot be submitted twice and failure redacts provider details", async () => {
  const f = fixture(true), reply = deferred(); f.invite = () => reply.promise;
  await render(f); await search(); await click(`Convidar ${peer.nickname}`);
  assert.equal(button(`Convidar ${peer.nickname}`).disabled, true);
  await act(async () => button(`Convidar ${peer.nickname}`).dispatchEvent(new window.Event("click", { bubbles: true })));
  assert.equal(count(f, "invite"), 1);
  await act(async () => reply.reject(new Error("provider-secret-path")));
  assert.doesNotMatch(container.textContent, /provider-secret-path|Convite enviado/);
});
for (const change of ["service", "account", "session"]) test(`late private lookup cannot select a recipient after ${change} changes`, async () => {
  const f = fixture(true), reply = deferred(); f.search = () => reply.promise;
  await render(f); await search();
  if (change === "service") await render(fixture(true));
  else if (change === "account") await render(f, { accountId: "another" });
  else { f.snapshot.ownSession = { ...session, id: "bbbbbbbb-2222-4222-8222-222222222222" }; await click("Atualizar"); }
  await act(async () => reply.resolve({ ...peer, nickname: "Old_Recipient" }));
  assert.doesNotMatch(container.textContent, /Old_Recipient/); assert.equal(count(f, "invite"), 0);
});
