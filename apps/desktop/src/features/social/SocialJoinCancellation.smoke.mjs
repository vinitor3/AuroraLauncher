// Synthetic DOM/service only. No Firestore, native transport, account or network.
import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";
const require = createRequire(import.meta.url);
const { window, document } = require("linkedom").parseHTML("<html><body><div id='root'></div></body></html>");
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Synthetic join cancellation" } });
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
const container = document.getElementById("root");
const fixedNow = 1_800_000_000_000;
const session = { id: "aaaaaaaa-1111-4111-8111-111111111111", hostUid: "host", title: "Mundo", modpackName: "Pack", minecraftVersion: "1.21.1", loader: "Fabric", imageUrl: null, players: 0, maxPlayers: 8, visibility: "public", joinPolicy: "approval", expiresAtMs: fixedNow + 60_000 };
const empty = () => ({ friends: [], incoming: [], outgoing: [], blocked: [], ownSession: null, presenceSharing: "hidden", modpackName: null });
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { resolve, reject, promise }; };
function fixture() {
  const f = { status: "none", now: fixedNow, calls: [], snapshot: empty(), page: [session] };
  const record = (name, id) => f.calls.push({ name, id });
  f.service = {
    capabilities: { friends: true, catalog: true, publication: false, join: true }, now: () => f.now,
    snapshot: async () => structuredClone(f.snapshot), listPublicSessions: async () => ({ items: structuredClone(f.page), nextCursor: null }),
    prepareJoin: async id => record("prepareJoin", id),
    organization: { now: () => f.now, snapshot: async () => ({ group: null, invitations: [], joinRequests: [] }),
      joinRequestStatus: async id => { record("joinRequestStatus", id); return f.status; },
      requestJoin: async id => { record("requestJoin", id); f.status = "pending"; },
      cancelJoinRequest: async id => { record("cancelJoinRequest", id); if (f.cancel) return f.cancel(); f.status = "none"; },
    },
  };
  return f;
}
let root;
const render = async (f, props = {}) => act(async () => root.render(createElement(SocialView, { service: f.service, accountId: "guest", nickname: "Guest", ...props })));
const buttons = label => [...container.querySelectorAll("button")].filter(item => item.textContent === label);
const button = label => { const found = buttons(label)[0]; assert.ok(found, label); return found; };
const click = async label => { assert.equal(button(label).disabled, false, label); await act(async () => button(label).dispatchEvent(new window.Event("click", { bubbles: true }))); };
const count = (f, name) => f.calls.filter(call => call.name === name).length;
beforeEach(() => { root = createRoot(container); });
afterEach(async () => act(async () => root.unmount()));

test("community cancellation clears only the pending hint and never resends or joins automatically", async () => {
  const f = fixture(); await render(f);
  assert.equal(buttons("Cancelar pedido").length, 0);
  await click("Pedir para entrar"); assert.equal(count(f, "cancelJoinRequest"), 0);
  await click("Cancelar pedido");
  assert.deepEqual(f.calls.find(call => call.name === "cancelJoinRequest"), { name: "cancelJoinRequest", id: session.id });
  assert.match(container.textContent, /Pedido de entrada cancelado/);
  assert.equal(buttons("Verificar pedido").length, 0); assert.equal(buttons("Cancelar pedido").length, 0);
  assert.equal(count(f, "requestJoin"), 1); assert.equal(count(f, "prepareJoin"), 0);
  await click("Pedir para entrar"); assert.equal(count(f, "requestJoin"), 2);
});

test("friend shortcut and friend catalog share cancellation without duplicate mutation", async () => {
  const f = fixture(); f.page = [];
  f.snapshot.friends = [{ uid: "host", nickname: "Host", avatarUrl: null, presence: null, session: { ...session, visibility: "friends" } }];
  await render(f); await click("Pedir para entrar"); await click("Dos amigos");
  assert.equal(buttons("Cancelar pedido").length, 2);
  await click("Cancelar pedido"); assert.equal(count(f, "cancelJoinRequest"), 1);
  assert.equal(buttons("Cancelar pedido").length, 0); assert.equal(buttons("Verificar pedido").length, 0);
  assert.equal(count(f, "prepareJoin"), 0);
});

test("an acceptance race preserves the hint and requires an explicit check before joining", async () => {
  const f = fixture(); await render(f); await click("Pedir para entrar");
  f.cancel = async () => { f.status = "accepted"; throw new Error("private transaction details"); };
  await click("Cancelar pedido");
  assert.doesNotMatch(container.textContent, /Pedido de entrada cancelado|private transaction/);
  assert.equal(button("Verificar pedido").disabled, false); assert.equal(count(f, "prepareJoin"), 0);
  await click("Verificar pedido"); assert.equal(count(f, "prepareJoin"), 1);
  assert.equal(buttons("Cancelar pedido").length, 0); assert.equal(count(f, "requestJoin"), 1);
});

test("an in-flight cancellation blocks duplicate cancel and status actions", async () => {
  const f = fixture(), reply = deferred(); f.cancel = () => reply.promise;
  await render(f); await click("Pedir para entrar"); await click("Cancelar pedido");
  assert.equal(button("Cancelar pedido").disabled, true); assert.equal(button("Verificar pedido").disabled, true);
  await act(async () => button("Cancelar pedido").dispatchEvent(new window.Event("click", { bubbles: true })));
  assert.equal(count(f, "cancelJoinRequest"), 1);
  await act(async () => reply.resolve()); assert.equal(buttons("Cancelar pedido").length, 0);
  assert.equal(count(f, "prepareJoin"), 0);
});

for (const transition of ["account", "service"]) for (const outcome of ["success", "failure"]) {
  test(`late cancellation ${outcome} cannot replace the ${transition} scope's pending request`, async () => {
    const old = fixture(), reply = deferred(); old.cancel = () => reply.promise;
    await render(old); await click("Pedir para entrar"); await click("Cancelar pedido");
    const next = transition === "service" ? fixture() : old; next.status = "pending";
    await render(next, transition === "account" ? { accountId: "new-guest" } : {});
    await click("Pedir para entrar");
    await act(async () => outcome === "success" ? reply.resolve() : reply.reject(new Error("old error")));
    assert.equal(button("Verificar pedido").disabled, false); assert.equal(button("Cancelar pedido").disabled, false);
    assert.doesNotMatch(container.textContent, /Pedido de entrada cancelado|Não foi possível concluir|old error/);
    assert.equal(count(old, "cancelJoinRequest"), 1); assert.equal(count(next, "prepareJoin"), 0);
  });
}

test("service clock expiry is rechecked before cancellation even before the next render", async () => {
  const f = fixture(); await render(f); await click("Pedir para entrar");
  f.now = session.expiresAtMs; await click("Cancelar pedido");
  assert.equal(count(f, "cancelJoinRequest"), 0); assert.doesNotMatch(container.textContent, /Pedido de entrada cancelado/);
});

test("services without cancellation keep the existing pending-check flow", async () => {
  const f = fixture(); delete f.service.organization.cancelJoinRequest;
  await render(f); await click("Pedir para entrar");
  assert.equal(buttons("Cancelar pedido").length, 0); assert.equal(button("Verificar pedido").disabled, false);
});
