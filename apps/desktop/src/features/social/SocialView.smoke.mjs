// Synthetic DOM only. No Firebase, native transport, remote accounts or personal configuration.
import assert from "node:assert/strict";
import { beforeEach, afterEach, test } from "node:test";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";

const require = createRequire(import.meta.url);
const { window, document } = require("linkedom").parseHTML("<html><body><div id='root'></div></body></html>");
document.oninput = null;
Object.defineProperty(window.HTMLInputElement.prototype, "type", {
  configurable: true, get() { return this.getAttribute("type") || "text"; }, set(value) { this.setAttribute("type", value); },
});
Object.assign(globalThis, { window, document, HTMLElement: window.HTMLElement, IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Social synthetic DOM" } });
const loaded = new Map();
async function sourceModule(source) {
  if (loaded.has(source.href)) return loaded.get(source.href);
  let output = ts.transpileModule(await readFile(source, "utf8"), { fileName: fileURLToPath(source), compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX,
  } }).outputText.replace(/^import ["'][^"']+\.css["'];?$/gm, "");
  for (const { fileName: specifier } of ts.preProcessFile(output).importedFiles) {
    const dependency = specifier.startsWith(".") ? ["", ".ts", ".tsx"].map(ext => new URL(specifier + ext, source)).find(url => existsSync(fileURLToPath(url))) : undefined;
    const resolved = specifier === "@tauri-apps/api/core" ? "data:text/javascript,export const invoke=(...args)=>globalThis.__socialDiscoveryInvoke(...args)" : dependency ? await sourceModule(dependency) : pathToFileURL(require.resolve(specifier)).href;
    output = output.replaceAll(`from "${specifier}"`, `from ${JSON.stringify(resolved)}`);
  }
  const url = `data:text/javascript;base64,${Buffer.from(output).toString("base64")}`;
  loaded.set(source.href, url); return url;
}
const { act, createElement } = await import("react");
const { createRoot } = await import("react-dom/client");
const { SocialView } = await import(await sourceModule(new URL("./SocialView.tsx", import.meta.url)));
const { SocialConnectionPanel } = await import(await sourceModule(new URL("./SocialConnectionPanel.tsx", import.meta.url)));
const { SocialOrganizationPanel } = await import(await sourceModule(new URL("./SocialOrganizationPanel.tsx", import.meta.url)));
const { socialArtwork } = await import(await sourceModule(new URL("./presentation.ts", import.meta.url)));
const actualNow = Date.now;
const fixedNow = 1_800_000_000_000;
const person = { uid: "friend-uid", nickname: "Aventureiro", avatarUrl: "https://attacker.example/avatar.png", expiresAtMs: fixedNow + 7 * 86_400_000 };
const baseSession = { id: "session-one", hostUid: "friend-uid", title: "Mundo da montanha", modpackName: "Aurora Valley", minecraftVersion: "1.21.1", loader: "Fabric", imageUrl: "https://cdn.modrinth.com/data/project/icon.png", players: 2, maxPlayers: 8, visibility: "public", joinPolicy: "open", expiresAtMs: fixedNow + 300_000 };
const empty = () => ({ friends: [], incoming: [], outgoing: [], blocked: [], ownSession: null, presenceSharing: "hidden", modpackName: null });
let root, calls, handlers, state, page, service;
const container = document.getElementById("root");
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { resolve, reject, promise }; };
function makeService() {
  const next = { capabilities: { friends: true, catalog: true, publication: true, join: false } };
  for (const name of ["searchByNickname", "snapshot", "requestFriend", "respondFriend", "cancelRequest", "removeFriend", "block", "unblock", "setPresence", "publishSession", "closeSession", "listPublicSessions", "prepareJoin"]) next[name] = async (...args) => {
    calls.push({ name, args });
    if (handlers[name]) return handlers[name](...args);
    if (name === "snapshot") return structuredClone(state);
    if (name === "listPublicSessions") return structuredClone(page);
    if (name === "searchByNickname") return structuredClone(person);
    if (name === "prepareJoin") throw { code: "join_unavailable" };
  };
  return next;
}
const render = async (props = {}) => { await act(async () => root.render(createElement(SocialView, { service, accountId: "owner-uid", nickname: "Explorador", ...props }))); };
const buttons = text => [...container.querySelectorAll("button")].filter(button => button.textContent === text);
const button = text => { const result = buttons(text)[0]; assert.ok(result, `Button ${text}`); return result; };
const click = async text => { assert.equal(button(text).disabled, false, `${text} enabled`); await act(async () => button(text).dispatchEvent(new window.Event("click", { bubbles: true }))); };
const edit = async (element, value) => { assert.ok(element); await act(async () => {
  Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set.call(element, value);
  element.dispatchEvent(new window.Event("input", { bubbles: true }));
}); };
const submit = async selector => { const form = container.querySelector(selector); assert.ok(form); await act(async () => form.dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true }))); };
const count = name => calls.filter(call => call.name === name).length;
beforeEach(() => { Date.now = () => fixedNow; calls = []; handlers = {}; state = empty(); page = { items: [], nextCursor: null }; service = makeService(); root = createRoot(container);
  globalThis.__socialDiscoveryInvoke = async (name, args) => { calls.push({ name, args }); if (handlers[name]) return handlers[name](args); throw new Error("Unexpected discovery call"); };
});
afterEach(async () => { await act(async () => root.unmount()); Date.now = actualNow; });

test("unavailable service performs no reads, mutations or automatic social activation", async () => {
  await render({ service: null });
  assert.match(container.textContent, /serviço social ainda não está habilitado/);
  assert.deepEqual(calls, []);
  assert.equal(button("Buscar").disabled, true);
});

test("mount only reads projections and never searches, mutates, publishes or joins", async () => {
  await render();
  assert.deepEqual(calls.map(call => call.name), ["snapshot", "listPublicSessions"]);
  assert.match(container.textContent, /Atividade oculta/);
  assert.equal(container.querySelectorAll("img").length, 0);
});

test("catalog uses the account service clock when the computer year is ahead", async () => {
  Date.now = () => fixedNow + 365 * 86_400_000;
  service.now = () => fixedNow; service.capabilities.join = true;
  page.items = [baseSession]; handlers.prepareJoin = async () => {};
  await render(); assert.equal(button("Entrar").disabled, false);
  await click("Entrar"); assert.deepEqual(calls.find(call => call.name === "prepareJoin").args, [baseSession.id]);
});

test("service clock expiry is rechecked on Enter before the next UI timer", async () => {
  let serverNow = fixedNow;
  Date.now = () => fixedNow - 365 * 86_400_000;
  service.now = () => serverNow; service.capabilities.join = true; page.items = [baseSession];
  await render(); serverNow = baseSession.expiresAtMs;
  await click("Entrar"); assert.equal(count("prepareJoin"), 0);
});

test("unavailable bounded service clock hides stale cards and offers a clear refresh", async () => {
  service.now = () => Number.NaN; service.capabilities.join = true; page.items = [baseSession];
  await render(); assert.equal(buttons("Entrar").length, 0);
  assert.match(container.textContent, /horário do serviço está indisponível/);
  assert.equal(button("Atualizar").disabled, false);
});

test("nick search is exact, explicit, one result and has no Minecraft profile lookup", async () => {
  await render();
  await edit(container.querySelector("#social-nickname"), "Aventureiro");
  assert.equal(count("searchByNickname"), 0);
  await submit(".social-search");
  assert.deepEqual(calls.find(call => call.name === "searchByNickname").args, ["Aventureiro"]);
  assert.match(container.textContent, /Aventureiro/);
  assert.equal(container.querySelectorAll("img").length, 0, "avatar URL never loads");
  assert.equal(count("requestFriend"), 0);
  await click("Adicionar amigo");
  assert.deepEqual(calls.find(call => call.name === "requestFriend").args, ["friend-uid"]);
  assert.match(container.textContent, /Pedido de amizade enviado/);
});

test("invalid nick cannot cause backend search", async () => {
  await render();
  await edit(container.querySelector("#social-nickname"), "ab*");
  await submit(".social-search");
  assert.equal(count("searchByNickname"), 0);
  assert.match(container.textContent, /3 a 16/);
});

test("negative exact lookup does not invent a person", async () => {
  handlers.searchByNickname = () => null;
  await render(); await edit(container.querySelector("#social-nickname"), "Nobody"); await submit(".social-search");
  assert.match(container.textContent, /Nenhuma conta disponível/);
  assert.equal(buttons("Adicionar amigo").length, 0);
});

test("self and an existing relation do not expose another friend request", async () => {
  handlers.searchByNickname = () => ({ ...person, uid: "owner-uid" });
  await render(); await edit(container.querySelector("#social-nickname"), "Explorador"); await submit(".social-search");
  assert.equal(buttons("Adicionar amigo").length, 0);
  handlers.searchByNickname = () => person; state.outgoing.push(person);
  await click("Atualizar"); await submit(".social-search");
  assert.match(container.textContent, /Pedido enviado/);
  assert.equal(buttons("Adicionar amigo").length, 0);
});

test("incoming acceptance and refusal are distinct explicit operations", async () => {
  state.incoming = [person];
  await render(); await click("Recebidos 1");
  assert.equal(count("respondFriend"), 0);
  await click("Aceitar");
  assert.deepEqual(calls.find(call => call.name === "respondFriend").args, [person.uid, "accept"]);
  await click("Recusar");
  assert.deepEqual(calls.filter(call => call.name === "respondFriend")[1].args, [person.uid, "decline"]);
});

test("cancel, remove, block and unblock use the selected account uid", async () => {
  state.outgoing = [person]; state.friends = [{ ...person, presence: null, session: null }]; state.blocked = [person];
  await render(); await click("Enviados 1"); await click("Cancelar pedido");
  await click("Amigos 1"); await click("Remover amigo"); await click("Bloquear");
  await click("Bloqueados 1"); await click("Desbloquear");
  for (const action of ["cancelRequest", "removeFriend", "block", "unblock"]) assert.deepEqual(calls.find(call => call.name === action).args, [person.uid]);
  assert.match(container.textContent, /amizade não foi restaurada/);
});

test("modpack activity is shown only while its declared presence is current", async () => {
  state.friends = [{ ...person, presence: { modpackName: "Aurora Valley", updatedAtMs: fixedNow }, session: null }];
  await render(); assert.match(container.textContent, /Aurora Valley/); assert.doesNotMatch(container.textContent, /No mundo|On-line/);
  Date.now = () => fixedNow + 120_000;
  await act(async () => window.dispatchEvent(new window.Event("focus")));
  assert.doesNotMatch(container.textContent, /Aurora Valley/); assert.match(container.textContent, /Atividade não compartilhada/);
});

test("catalog uses card/list layouts and separates declared limit from unobserved population", async () => {
  page.items = [baseSession]; await render();
  assert.match(container.textContent, /Mundo da montanha/); assert.match(container.textContent, /Aurora Valley/);
  assert.match(container.textContent, /1\.21\.1/); assert.match(container.textContent, /Limite do anúncio: 8/); assert.match(container.textContent, /Disponibilidade não verificada/);
  assert.equal(container.querySelector(".social-catalog").dataset.layout, "grid");
  const image = container.querySelector(".social-session img");
  assert.equal(image.getAttribute("referrerpolicy") ?? image.getAttribute("referrerPolicy"), "no-referrer");
  await click("Lista"); assert.equal(container.querySelector(".social-catalog").dataset.layout, "list");
});

test("declared manual population cannot disable a real selected admission", async () => {
  page.items = [{ ...baseSession, players: baseSession.maxPlayers }]; service.capabilities.join = true; handlers.prepareJoin = () => undefined;
  await render({ connection: connection() }); await click("Entrar");
  assert.equal(count("prepareJoin"), 1); assert.equal(container.querySelector("input[aria-label='Pessoas na sessão']"), null);
});

test("visible catalog observes native connections and monotonic expiry removes stale counts", async t => {
  const id = "11111111-2222-4333-8444-555555555555"; page.items = [{ ...baseSession, id }];
  const native = connection(); native.state.maxSessionSeconds = 14400;
  let hold = false; native.controller.observeSessions = async ids => { calls.push({ name: "observe", args: ids });
    if (hold) return new Promise(() => {});
    return [{ sessionId: id, state: "ready", validUntil: performance.now() + 2000, admittedGuests: 2, connectedGuests: 1, hostPlayerConnected: true, maxGuests: 7 }]; };
  await render({ connection: native });
  assert.match(container.textContent, /Host disponível/); assert.match(container.textContent, /1 \/ 7 conexões remotas/);
  assert.match(container.textContent, /2 admissões ativas · anfitrião conectado/); assert.equal(count("prepareJoin"), 0);
  hold = true; const monotonic = performance.now(); t.mock.method(performance, "now", () => monotonic + 4000);
  await act(async () => window.dispatchEvent(new window.Event("focus")));
  assert.doesNotMatch(container.textContent, /1 \/ 7 conexões remotas/); assert.match(container.textContent, /Disponibilidade não verificada/);
});

test("delayed catalog observation cannot cross an account transition", async () => {
  const id = "11111111-2222-4333-8444-555555555555"; page.items = [{ ...baseSession, id }];
  let resolve; const response = new Promise(yes => { resolve = yes; }); const native = connection(); native.state.maxSessionSeconds = 14400;
  native.controller.observeSessions = () => response;
  await render({ connection: native }); await render({ accountId: "other", service: null });
  await act(async () => resolve([{ sessionId: id, state: "ready", validUntil: performance.now() + 2000, admittedGuests: 2, connectedGuests: 1, hostPlayerConnected: true, maxGuests: 7 }]));
  assert.doesNotMatch(container.textContent, /Host disponível|conexões remotas/);
});

test("catalog read quota does not claim to close a connection or expose a provider response", async () => {
  page.items = [{ ...baseSession, id: "11111111-2222-4333-8444-555555555555" }];
  const native = connection(); native.state.maxSessionSeconds = 14400;
  native.controller.observeSessions = async () => { throw { code: "connect_social_quota", message: "Bearer private-token" }; };
  await render({ connection: native });
  assert.match(container.textContent, /limite de consultas de disponibilidade/);
  assert.doesNotMatch(container.textContent, /conexão foi encerrada|private-token|Bearer/);
  assert.equal(count("prepareJoin"), 0);
});

test("join capability false never invokes prepareJoin, including synthetic click", async () => {
  page.items = [baseSession]; state.friends = [{ ...person, presence: null, session: baseSession }]; await render();
  assert.equal(buttons("Entrar").length, 2);
  for (const element of buttons("Entrar")) {
    assert.equal(element.disabled, true);
    await act(async () => element.dispatchEvent(new window.Event("click", { bubbles: true })));
  }
  assert.equal(count("prepareJoin"), 0); assert.match(container.textContent, /serviço online não está ativado/);
});

test("expired cards are removed on resume and no stale click can request admission", async () => {
  service.capabilities.join = true; page.items = [baseSession]; await render(); const oldButton = button("Entrar");
  Date.now = () => fixedNow + 300_000;
  await act(async () => oldButton.dispatchEvent(new window.Event("click", { bubbles: true })));
  assert.equal(count("prepareJoin"), 0);
  await act(async () => document.dispatchEvent(new window.Event("visibilitychange")));
  assert.equal(container.querySelector(".social-session"), null);
});

test("catalog filters loaded entries without running account or network searches", async () => {
  page.items = [baseSession]; await render(); const readCount = calls.length;
  await edit(container.querySelector('[aria-label="Filtrar sessões carregadas"]'), "Forge");
  assert.equal(container.querySelector(".social-session"), null); assert.equal(calls.length, readCount);
  await edit(container.querySelector('[aria-label="Filtrar sessões carregadas"]'), "1.21");
  assert.ok(container.querySelector(".social-session"));
});

test("untrusted titles remain text and arbitrary image URLs never load", async () => {
  page.items = [{ ...baseSession, title: '<img src=x onerror="alert(1)">', imageUrl: "https://attacker.example/a.png" }];
  await render(); assert.equal(container.querySelector(".social-session img"), null);
  assert.match(container.textContent, /<img src=x/); assert.equal(container.querySelector('[onerror]'), null);
  for (const value of ["https://cdn.modrinth.com/a.png?track=account", "https://cdn.modrinth.com/a.png#x", "http://cdn.modrinth.com/a.png", "https://127.0.0.1/a.png", "https://user@cdn.modrinth.com/a.png"]) assert.equal(socialArtwork(value), undefined);
});

test("backend errors are redacted instead of displaying credential-bearing details", async () => {
  handlers.snapshot = () => { throw { code: "permission-denied", message: "secret-token-and-private-path" }; };
  await render(); assert.match(container.textContent, /permissão mudou/); assert.doesNotMatch(container.textContent, /secret-token-and-private-path/);
});

test("account change discards a delayed search result and all previous relationship data", async () => {
  const delayed = deferred(); handlers.searchByNickname = () => delayed.promise;
  state.friends = [{ ...person, presence: null, session: null }];
  await render(); await edit(container.querySelector("#social-nickname"), "Aventureiro");
  await act(async () => container.querySelector(".social-search").dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true })));
  state = empty(); await render({ accountId: "second-account", nickname: "OutraConta" });
  await act(async () => delayed.resolve(person));
  assert.doesNotMatch(container.textContent, /Aventureiro/); assert.match(container.textContent, /OutraConta/);
  assert.equal(container.querySelector("#social-nickname").value, "");
});

test("service replacement under the same account discards delayed previous snapshots", async () => {
  const delayed = deferred(); handlers.snapshot = () => delayed.promise;
  await render();
  delete handlers.snapshot; const replacement = makeService(); await render({ service: replacement });
  await act(async () => delayed.resolve({ ...empty(), incoming: [person] }));
  assert.doesNotMatch(container.textContent, /Aventureiro/); assert.ok(button("Recebidos 0"));
});

test("double mutation is suppressed until first operation and refresh finish", async () => {
  const delayed = deferred(); state.incoming = [person]; handlers.respondFriend = () => delayed.promise;
  await render(); await click("Recebidos 1");
  await act(async () => { const element = button("Aceitar"); element.dispatchEvent(new window.Event("click", { bubbles: true })); element.dispatchEvent(new window.Event("click", { bubbles: true })); });
  assert.equal(count("respondFriend"), 1);
  await act(async () => delayed.resolve()); assert.equal(button("Aceitar").disabled, false);
});

test("failed refresh after a mutation removes previous actionable projections", async () => {
  state.friends = [{ ...person, presence: null, session: baseSession }]; page.items = [baseSession];
  handlers.block = () => { handlers.snapshot = () => { throw { code: "unavailable" }; }; };
  await render(); await click("Bloquear");
  assert.equal(container.querySelector(".social-session"), null); assert.equal(buttons("Remover amigo").length, 0);
  assert.match(container.textContent, /Não foi possível concluir/);
});

test("publication defaults private and does not publish on mount or disclosure", async () => {
  await render(); const selects = container.querySelectorAll(".social-publish select");
  assert.equal(selects[0].value, "private"); assert.equal(selects[1].value, "open");
  assert.equal(count("publishSession"), 0); assert.equal(count("setPresence"), 0);
  assert.match(container.textContent, /não abre portas/);
});

test("reopening Social hydrates the own advertisement without granting publication consent", async () => {
  state.ownSession = { ...baseSession, hostUid: "owner-uid", visibility: "friends", joinPolicy: "approval", maxPlayers: 12 };
  const expectedInputs = [baseSession.title, baseSession.modpackName, baseSession.minecraftVersion, "Fabric", baseSession.imageUrl, "12"];
  const values = () => [...container.querySelectorAll(".social-publish .social-fields input")].map(input => input.value);
  await render();
  assert.deepEqual(values(), expectedInputs);
  assert.deepEqual([...container.querySelectorAll(".social-publish select")].map(select => select.value), ["friends", "approval"]);
  assert.equal(button("Salvar anúncio").disabled, true);
  await submit(".social-publish form"); assert.equal(count("publishSession"), 0);
  await edit(container.querySelector(".social-publish .social-fields input"), "Rascunho não salvo");
  await act(async () => root.render(null)); await render();
  assert.deepEqual(values(), expectedInputs, "a fresh view loads the original persisted advertisement");
  const consent = container.querySelector(".social-publish .social-consent input");
  assert.equal(Boolean(consent.checked), false);
  await act(async () => { consent.checked = true; consent.dispatchEvent(new window.Event("click", { bubbles: true })); });
  await submit(".social-publish form");
  const { id: _id, hostUid: _host, expiresAtMs: _expiry, ...draft } = state.ownSession;
  assert.deepEqual(calls.find(call => call.name === "publishSession").args, [draft], "only editable draft fields are submitted");
});

test("refresh hydrates pristine fields, preserves intentional edits, and resets only for a different session", async () => {
  state.ownSession = { ...baseSession, hostUid: "owner-uid", visibility: "friends" };
  const title = () => container.querySelector(".social-publish .social-fields input");
  await render();
  state.ownSession.title = "Título atualizado no serviço"; await click("Atualizar");
  assert.equal(title().value, state.ownSession.title);
  await edit(title(), "Minha edição em andamento");
  state.ownSession.title = "Outra observação do serviço"; await click("Atualizar");
  assert.equal(title().value, "Minha edição em andamento");
  state.ownSession = { ...state.ownSession, id: "different-session", title: "Outro anúncio", visibility: "private" };
  await click("Atualizar"); assert.equal(title().value, "Outro anúncio");
  assert.equal(container.querySelector(".social-publish select").value, "private");
  handlers.closeSession = () => { state.ownSession = null; };
  await click("Encerrar anúncio");
  assert.equal(title().value, ""); assert.equal(container.querySelectorAll(".social-publish .social-fields input")[3].value, "Vanilla");
  assert.equal(count("publishSession"), 0);
});

for (const transition of ["account", "service"]) test(`late own advertisement cannot hydrate after ${transition} replacement`, async () => {
  state.ownSession = { ...baseSession, hostUid: "owner-uid", title: "Anúncio anterior", visibility: "friends" };
  await render();
  const old = structuredClone(state), delayed = deferred(); handlers.snapshot = () => delayed.promise;
  await click("Atualizar"); delete handlers.snapshot;
  const accountId = transition === "account" ? "another-owner" : "owner-uid";
  state.ownSession = { ...baseSession, id: "another-session", hostUid: accountId, title: "Anúncio atual", visibility: "private" };
  await render({ accountId, service: transition === "service" ? makeService() : service });
  await act(async () => delayed.resolve(old));
  assert.equal(container.querySelector(".social-publish .social-fields input").value, "Anúncio atual");
  assert.equal(container.querySelector(".social-publish select").value, "private");
  assert.doesNotMatch(container.textContent, /Anúncio anterior/);
  assert.equal(count("publishSession"), 0);
});

test("catalog retention is bounded to 100 records instead of unlimited pagination", async () => {
  page.items = Array.from({ length: 100 }, (_, index) => ({ ...baseSession, id: `session-${index}` })); page.nextCursor = "more";
  await render(); assert.equal(container.querySelectorAll(".social-session").length, 100);
  assert.equal(buttons("Carregar mais").length, 0); assert.match(container.textContent, /Exibindo até 100 anúncios/);
});

test("activity uses only running instance supplied by owner and requires an explicit update", async () => {
  state.presenceSharing = "friends"; state.modpackName = "Previous pack";
  await render({ currentModpack: "Current running pack" });
  assert.equal(count("setPresence"), 0);
  await click("Atualizar atividade");
  assert.deepEqual(calls.find(call => call.name === "setPresence").args, [{ sharing: "friends", modpackName: "Current running pack" }]);
  await render({ currentModpack: undefined }); assert.equal(count("setPresence"), 1);
  await click("Atualizar atividade");
  assert.deepEqual(calls.filter(call => call.name === "setPresence")[1].args, [{ sharing: "friends", modpackName: null }]);
});

test("uncertain operation response warns against replay without exposing raw details", async () => {
  handlers.snapshot = () => { throw { code: "social_uncertain", message: "private-credential-path" }; };
  await render(); assert.match(container.textContent, /alteração pode ter sido aplicada; atualize a lista antes de repetir/);
  assert.doesNotMatch(container.textContent, /private-credential-path/);
});

test("expired request cannot be accepted but can be declined or cancelled for cleanup", async () => {
  state.incoming = [{ ...person, expiresAtMs: fixedNow - 1 }]; state.outgoing = [{ ...person, expiresAtMs: fixedNow - 1 }];
  await render(); await click("Recebidos 1"); assert.match(container.textContent, /Pedido expirado/); assert.equal(button("Aceitar").disabled, true);
  await click("Recusar"); assert.deepEqual(calls.find(call => call.name === "respondFriend").args, [person.uid, "decline"]);
  await click("Enviados 1"); assert.match(container.textContent, /Pedido expirado/); await click("Cancelar pedido");
  assert.deepEqual(calls.find(call => call.name === "cancelRequest").args, [person.uid]);
});

test("acceptance checks the request deadline at click time even before a timer renders", async () => {
  state.incoming = [{ ...person, expiresAtMs: fixedNow + 1000 }]; await render(); await click("Recebidos 1");
  Date.now = () => fixedNow + 1000; await click("Aceitar");
  assert.equal(count("respondFriend"), 0); assert.match(container.textContent, /Este pedido expirou/);
});

function connection(snapshot) {
  const controller = {};
  for (const name of ["prepareHost", "prepareHostDiscovered", "confirmHost", "confirmGuest", "stop", "refresh"]) controller[name] = async (...args) => { calls.push({ name: `connect:${name}`, args }); };
  return { state: { available: true, pending: false, error: "", snapshot }, controller };
}
function connectionSnapshot(stage = "hostReview", role = "host") {
  return { operationId: "own-operation", generation: 2, phase: stage === "connected" ? "guestConnected" : "reviewed", role,
    cleanupState: "pending", tcpAddress: role === "guest" && stage === "connected" ? "127.0.0.1:42010" : null,
    social: { sessionId: baseSession.id, role, stage, digest: role === "host" ? "a".repeat(64) : null,
      gameTarget: role === "host" ? "127.0.0.1:43001" : null, visibility: role === "host" ? "public" : null, expiresAtMs: fixedNow + 60_000 } };
}
test("lost host continuity reports its relative observation separately from the long session deadline", async () => {
  const snapshot = connectionSnapshot("failed"); snapshot.phase = "failed"; snapshot.cleanupState = "complete";
  snapshot.errorCode = "connect_social_continuity_lost";
  snapshot.social.digest = null; snapshot.social.failureStage = "sessionContinuity";
  snapshot.social.failureObservation = { gate: "hostPoll", bootstrapRoundtripMs: 2800, bootstrapUncertaintyMs: 1200,
    hostPollElapsedMs: 1800, hostPollAgeMs: 3000, sessionRemainingMs: 14_000_000 };
  await render({ connection: connection(snapshot) });
  assert.match(container.querySelector(".social-connection [role=alert]").textContent, /^Manutenção da sessão\. /);
  assert.match(container.textContent, /Não foi possível confirmar a continuidade da sessão a tempo/);
  assert.doesNotMatch(container.textContent, /prazo da sessão terminou/);
  const details = [...container.querySelectorAll("details")].find(item => item.querySelector("summary")?.textContent === "Detalhes da interrupção");
  assert.ok(details); assert.equal(details.hasAttribute("open"), false);
  assert.match(details.textContent, /Atualização da autorização do anfitrião/);
  assert.match(details.textContent, /1200 ms/); assert.match(details.textContent, /3000 ms/); assert.match(details.textContent, /14000 s/);
  assert.equal(calls.filter(call => call.name.startsWith("connect:")).length, 0);
});
test("the original host attempt locks its advertisement through review, listening and unfinished cleanup", async () => {
  state.ownSession = { ...baseSession, hostUid: "owner-uid" };
  const own = connectionSnapshot();
  for (const [phase, cleanupState] of [["reviewed", "pending"], ["hostListening", "pending"], ["closing", "pending"], ["closed", "pending"], ["failed", "failed"]]) {
    await render({ connection: connection({ ...own, phase, cleanupState }) });
    const controls = [...container.querySelectorAll(".social-publish input, .social-publish select")];
    assert.ok(controls.length > 0); assert.ok(controls.every(control => control.disabled), phase);
    assert.equal(button("Salvar anúncio").disabled, true, phase);
    assert.equal(button("Encerrar anúncio").disabled, false, "explicit revocation stays available");
    assert.match(container.textContent, /Use Desconectar e aguarde a liberação dos recursos/);
    await submit(".social-publish form");
  }
  assert.equal(count("publishSession"), 0, "synthetic submit cannot bypass the lock");
  assert.equal(count("closeSession"), 0);
  assert.match(container.textContent, /Encerrar o anúncio revoga a autorização desta sessão/);
  await click("Encerrar anúncio"); assert.equal(count("closeSession"), 1);
});

test("host cleanup releases the editor without replacing an intentional draft", async () => {
  state.ownSession = { ...baseSession, hostUid: "owner-uid" };
  const title = () => container.querySelector(".social-publish .social-fields input");
  await render(); await edit(title(), "Minha alteração ainda não salva");
  const own = connectionSnapshot();
  await render({ connection: connection(own) });
  assert.equal(title().disabled, true); assert.equal(title().value, "Minha alteração ainda não salva");
  await render({ connection: connection({ ...own, phase: "closed", cleanupState: "complete" }) });
  assert.equal(title().disabled, false); assert.equal(title().value, "Minha alteração ainda não salva");
  assert.doesNotMatch(container.textContent, /Use Desconectar e aguarde a liberação dos recursos/);
  const consent = container.querySelector(".social-publish .social-consent input");
  await act(async () => { consent.checked = true; consent.dispatchEvent(new window.Event("click", { bubbles: true })); });
  await submit(".social-publish form");
  assert.equal(calls.find(call => call.name === "publishSession").args[0].title, "Minha alteração ainda não salva");
});

test("a guest or a host of another session does not lock this advertisement", async () => {
  state.ownSession = { ...baseSession, hostUid: "owner-uid", visibility: "private" };
  const otherHost = connectionSnapshot(); otherHost.social.sessionId = "other-session";
  for (const snapshot of [connectionSnapshot("connected", "guest"), otherHost, { ...connectionSnapshot(), phase: "failed", cleanupState: "complete" }]) {
    await render({ connection: connection(snapshot) });
    assert.ok([...container.querySelectorAll(".social-publish input, .social-publish select")].every(control => !control.disabled));
    assert.equal(button("Salvar anúncio").disabled, false);
    await submit(".social-publish form");
  }
  assert.equal(count("publishSession"), 3);
});

test("public Enter invokes selected session only; rendering does not create transport", async () => {
  service.capabilities.join = true; handlers.prepareJoin = () => undefined; page.items = [baseSession];
  await render({ connection: connection() }); assert.equal(count("prepareJoin"), 0);
  await click("Entrar"); assert.deepEqual(calls.find(call => call.name === "prepareJoin").args, [baseSession.id]);
  assert.equal(count("connect:confirmHost"), 0);
});
test("friend Enter invokes selected open session without a copied invitation", async () => {
  service.capabilities.join = true; handlers.prepareJoin = () => undefined;
  state.friends = [{ ...person, presence: null, session: { ...baseSession, visibility: "friends" } }];
  await render({ connection: connection() }); await click("Entrar");
  assert.deepEqual(calls.find(call => call.name === "prepareJoin").args, [baseSession.id]);
  assert.equal(container.querySelector("textarea"), null);
});
test("approval-only and self-owned cards stay disabled after join capability enables", async () => {
  service.capabilities.join = true; page.items = [{ ...baseSession, joinPolicy: "approval" }, { ...baseSession, id: "own", hostUid: "owner-uid" }];
  await render({ connection: connection() }); assert.equal(button("Pedir para entrar").disabled, true); assert.equal(button("Entrar").disabled, true);
  assert.equal(count("prepareJoin"), 0);
});
function approvalJoinFixture() {
  service.capabilities.join = true;
  page.items = [{ ...baseSession, joinPolicy: "approval" }];
  handlers.prepareJoin = () => undefined;
  service.organization = { snapshot: async () => ({ group: null, invitations: [], joinRequests: [] }) };
  for (const name of ["joinRequestStatus", "requestJoin"]) service.organization[name] = async (...args) => {
    calls.push({ name, args });
    return handlers[name] ? handlers[name](...args) : name === "joinRequestStatus" ? "none" : undefined;
  };
}

test("approval join shows a waiting request in friends and catalog without resending or auto-entering", async () => {
  approvalJoinFixture();
  state.friends = [{ ...person, presence: null, session: page.items[0] }];
  let status = "none";
  handlers.joinRequestStatus = () => status;
  await render();
  assert.equal(buttons("Pedir para entrar").length, 2);
  assert.equal(count("joinRequestStatus"), 0);
  await click("Pedir para entrar");
  assert.equal(count("requestJoin"), 1); assert.equal(count("prepareJoin"), 0);
  assert.equal(buttons("Verificar pedido").length, 2);
  assert.match(container.textContent, /Pedido enviado/);
  assert.match(container.querySelector(".social-session-footer").textContent, /Aguardando aprovação/);
  status = "pending";
  await click("Verificar pedido");
  assert.equal(count("requestJoin"), 1); assert.equal(count("prepareJoin"), 0);
  assert.match(container.textContent, /Aguardando aprovação do anfitrião/);
  assert.doesNotMatch(container.textContent, /Pedido enviado/);
  status = "accepted";
  assert.equal(count("prepareJoin"), 0, "a changed remote status does not auto-enter");
  await click("Verificar pedido");
  assert.equal(count("joinRequestStatus"), 3);
  assert.deepEqual(calls.find(call => call.name === "prepareJoin").args, [baseSession.id]);
  assert.equal(count("prepareJoin"), 1); assert.equal(count("requestJoin"), 1);
  assert.equal(buttons("Verificar pedido").length, 0);
});

test("approval join checks never silently resend a request removed by the service", async () => {
  approvalJoinFixture();
  await render(); await click("Pedir para entrar");
  assert.equal(count("requestJoin"), 1);
  await click("Verificar pedido");
  assert.equal(count("requestJoin"), 1); assert.equal(count("prepareJoin"), 0);
  assert.match(container.textContent, /Este pedido não está mais disponível/);
  assert.equal(buttons("Verificar pedido").length, 0);
  await click("Pedir para entrar");
  assert.equal(count("requestJoin"), 2, "resending needs a new explicit request action");
});

for (const transition of ["account", "service"]) for (const status of ["none", "pending", "accepted"]) {
  test(`approval join discards delayed ${status} after ${transition} replacement`, async () => {
    approvalJoinFixture();
    const reply = deferred(); handlers.joinRequestStatus = () => reply.promise;
    await render(); await click("Pedir para entrar");
    await render(transition === "account" ? { accountId: "other-account" } : { service: makeService() });
    await act(async () => reply.resolve(status));
    assert.equal(count("requestJoin"), 0); assert.equal(count("prepareJoin"), 0);
    assert.equal(buttons("Verificar pedido").length, 0);
    assert.doesNotMatch(container.textContent, /Pedido enviado|Aguardando aprovação/);
  });
}

test("approval join discards a late request acknowledgement after account replacement", async () => {
  approvalJoinFixture();
  const reply = deferred(); handlers.requestJoin = () => reply.promise;
  await render(); await click("Pedir para entrar");
  assert.equal(count("requestJoin"), 1);
  await render({ accountId: "other-account" });
  await act(async () => reply.resolve());
  assert.equal(count("prepareJoin"), 0); assert.equal(buttons("Verificar pedido").length, 0);
  assert.doesNotMatch(container.textContent, /Pedido enviado|Aguardando aprovação/);
});

for (const status of ["none", "accepted"]) test(`approval join rechecks service expiry after a delayed ${status}`, async () => {
  approvalJoinFixture();
  let now = fixedNow; service.now = () => now;
  const reply = deferred(); handlers.joinRequestStatus = () => reply.promise;
  await render(); await click("Pedir para entrar");
  now = baseSession.expiresAtMs;
  await act(async () => reply.resolve(status));
  assert.equal(count("requestJoin"), 0); assert.equal(count("prepareJoin"), 0);
  assert.equal(buttons("Verificar pedido").length, 0);
});

test("host preparation requires an eligible own advert and never auto-approves", async () => {
  state.ownSession = { ...baseSession, hostUid: "owner-uid" };
  await render({ connection: connection() }); assert.equal(count("connect:prepareHost"), 0);
  await click("Preparar meu servidor"); assert.deepEqual(calls.find(call => call.name === "connect:prepareHost").args, [baseSession.id, "public"]);
  assert.equal(count("connect:confirmHost"), 0);
});
test("dedicated server shortcut routes once without preparing, reserving or approving", async () => {
  let routed = 0;
  const control = connection(); control.state.maxSessionSeconds = 14400;
  state.ownSession = { ...baseSession, hostUid: "owner-uid" };
  await render({ connection: control, onPrepareDedicatedServer: () => { routed++; } });
  assert.equal(routed, 0); assert.equal(button("Preparar meu servidor").disabled, true);
  await click("Abrir painel do servidor dedicado");
  assert.equal(routed, 1);
  assert.equal(calls.some(call => call.name.startsWith("connect:") || call.name === "native_voice_execute"), false);
  await render({ connection: { ...control, state: { ...control.state, pending: true } }, onPrepareDedicatedServer: () => { routed++; } });
  assert.equal(button("Abrir painel do servidor dedicado").disabled, true);
  await act(async () => button("Abrir painel do servidor dedicado").dispatchEvent(new window.Event("click", { bubbles: true })));
  assert.equal(routed, 1);
  await render({ connection: control });
  assert.equal(buttons("Abrir painel do servidor dedicado").length, 0);
});
test("host review shows fixed source scope deadline and starts with consent unchecked", async () => {
  await render({ connection: connection(connectionSnapshot()) });
  assert.match(container.textContent, /127\.0\.0\.1:43001/); assert.match(container.textContent, /60 segundos/);
  assert.match(container.textContent, /com 1 vaga/); assert.equal(button("Compartilhar agora").disabled, true);
  assert.equal(count("connect:confirmHost"), 0);
});
test("host explicit consent permits confirm and resets when the reviewed digest changes", async () => {
  const snapshot = connectionSnapshot(), control = connection(snapshot); await render({ connection: control });
  const checkbox = container.querySelector('.social-connection input[type="checkbox"]'); assert.ok(checkbox);
  await act(async () => { checkbox.checked = true; checkbox.dispatchEvent(new window.Event("click", { bubbles: true })); });
  assert.equal(button("Compartilhar agora").disabled, false); await click("Compartilhar agora"); assert.equal(count("connect:confirmHost"), 1);
  await render({ connection: connection({ ...snapshot, social: { ...snapshot.social, digest: "b".repeat(64) } }) });
  assert.equal(button("Compartilhar agora").disabled, true);
});
test("active native reservation disables new joins and retains Disconnect during credential wait", async () => {
  service.capabilities.join = true; page.items = [baseSession]; const control = connection(connectionSnapshot("reserved", "guest")); control.state.pending = true;
  await render({ connection: control }); assert.equal(button("Entrar").disabled, true); assert.equal(button("Desconectar").disabled, false);
  await click("Desconectar"); assert.equal(count("connect:stop"), 1);
});
test("connected projection shows loopback address and distinguishes game world from transport", async () => {
  await render({ connection: connection(connectionSnapshot("connected", "guest")) });
  assert.equal(container.querySelector("#social-local-address").value, "127.0.0.1:42010");
  assert.match(container.textContent, /Entrada no mundo e disponibilidade do jogo ainda não verificadas/);
  await click("Copiar endereço"); assert.match(container.textContent, /Selecione e copie/);
});
test("closed projection only claims resource release when native cleanup is complete", async () => {
  const snapshot = connectionSnapshot("closed", "guest"); snapshot.phase = "closed"; snapshot.cleanupState = "pending";
  await render({ connection: connection(snapshot) }); assert.doesNotMatch(container.textContent, /Recursos da conexão liberados/);
  await render({ connection: connection({ ...snapshot, cleanupState: "complete" }) }); assert.match(container.textContent, /Recursos da conexão liberados/);
});
test("closing native phase takes precedence over retained connected or host-review presentation", async () => {
  const snapshot = connectionSnapshot("connected", "guest"); snapshot.phase = "closing";
  await render({ connection: connection(snapshot) }); assert.match(container.textContent, /Desconectando/);
  assert.doesNotMatch(container.textContent, /A conexão foi autenticada/); assert.equal(container.querySelector("#social-local-address"), null);
  await render({ connection: connection({ ...connectionSnapshot(), phase: "closing" }) });
  assert.equal(button("Compartilhar agora").disabled, true);
});

test("social guest can navigate to explicit game review only after native source publication", async () => {
  const snapshot = { ...connectionSnapshot("connected", "guest"), transportState: "authenticated" };
  let opened = 0;
  const onOpenConnectedGame = () => { opened++; };
  await render({ connection: connection(snapshot), onOpenConnectedGame });
  assert.equal([...container.querySelectorAll("button")].some(button => button.textContent === "Abrir Minecraft"), false);
  const ready = { ...snapshot, social: { ...snapshot.social, readyGameLaunchAvailable: true } };
  await render({ connection: connection(ready), onOpenConnectedGame });
  assert.equal(opened, 0); await click("Abrir Minecraft"); assert.equal(opened, 1);
  assert.equal(calls.some(call => call.name.startsWith("connect:") || call.name === "connect_prepare_ready_game_launch" || call.name === "launch_instance"), false);
  await render({ connection: connection({ ...ready, phase: "closing" }), onOpenConnectedGame });
  assert.equal([...container.querySelectorAll("button")].some(button => button.textContent === "Abrir Minecraft"), false);
  await render({ connection: connection(snapshot), onOpenConnectedGame });
  assert.equal([...container.querySelectorAll("button")].some(button => button.textContent === "Abrir Minecraft"), false);
});
test("social game navigation refuses expiry at click before the timer renders", async () => {
  const snapshot = { ...connectionSnapshot("connected", "guest"), transportState: "authenticated" };
  snapshot.social.readyGameLaunchAvailable = true; snapshot.social.expiresAtMs = fixedNow + 1000;
  let opened = 0;
  await render({ connection: connection(snapshot), onOpenConnectedGame: () => { opened++; } });
  const savedNow = Date.now; Date.now = () => fixedNow + 1001;
  try { await click("Abrir Minecraft"); assert.equal(opened, 0); } finally { Date.now = savedNow; }
});
for (const enabled of [false, true]) {
  test(`publication result explains separate host activation with social connection ${enabled ? "available" : "unavailable"}`, async () => {
    await render({ connection: enabled ? connection() : undefined });
    const fields = container.querySelectorAll(".social-publish .social-fields input");
    await edit(fields[0], "Meu mundo"); await edit(fields[1], "Meu modpack"); await edit(fields[2], "1.21.1");
    await submit(".social-publish form");
    assert.equal(count("publishSession"), 1);
    assert.equal(count("connect:prepareHost"), 0); assert.equal(count("connect:confirmHost"), 0);
    if (enabled) assert.match(container.textContent, /Anúncio atualizado\. Use Preparar meu servidor para ativar o compartilhamento\./);
    else assert.match(container.textContent, /Anúncio atualizado\. O compartilhamento do servidor ainda não foi ativado\./);
    assert.doesNotMatch(container.textContent, /A conexão automática continua separada/);
  });
}

function discoveryFixture(target = "127.0.0.1:51029") {
  const common = { instanceId: "owned-game", runId: "owned-run", expiresAtMs: fixedNow + 30_000,
    runtime: { minecraftVersion: "1.21.1", loader: "fabric", loaderVersion: "0.16.14", statusProtocol: 767, evidence: "nativeLaunchProfile" } };
  const candidate = { candidateId: "candidate", target, bindKind: "loopback" };
  handlers.list_running_instances = () => [{ instanceId: common.instanceId }];
  handlers.connect_discover_instance = () => ({ ...common, inventoryId: "inventory", candidates: [candidate] });
  handlers.connect_review_discovered = () => ({ ...common, inventoryId: "inventory", reviewId: "review", digest: "a".repeat(64), candidate,
    statusProtocol: 767, handshakeHost: "localhost", handshakePort: 51029 });
  const selected = { ...common, selectionId: "22222222-2222-4333-8444-555555555555", target, verifiedBeforeAfter: true, worldState: "notVerified",
    summary: { requestedProtocol: 767, reportedProtocol: 767, protocolMatchesRequested: true, reportedPlayersOnline: 0, reportedPlayersMax: 2,
      connectMicros: 1, statusMicros: 2, pingMicros: 3, totalMicros: 6 } };
  handlers.connect_confirm_discovered_status = () => selected;
  return selected;
}
const chooseDiscovery = async (target = "127.0.0.1:51029") => {
  await click("Buscar jogos abertos pelo Aurora"); await click("Ver portas de owned-game");
  await click(`Revisar ${target}`); await click("Autorizar consulta e selecionar porta");
};
const renderPanel = async (control, ownSession = { ...baseSession, hostUid: "owner-uid" }) => {
  await act(async () => root.render(createElement(SocialConnectionPanel, { connection: control, ownSession, now: Date.now() })));
};
test("social discovered host requires explicit owned selection and consumes it before reservation", async () => {
  const selected = discoveryFixture(), control = connection(); control.state.ownedGameTcpDiscovery = true;
  await renderPanel(control);
  assert.equal(calls.length, 0); assert.equal(button("Preparar meu servidor").disabled, true);
  await chooseDiscovery(); assert.equal(button("Preparar meu servidor").disabled, false);
  await click("Preparar meu servidor");
  assert.deepEqual(calls.find(call => call.name === "connect:prepareHostDiscovered").args, [baseSession.id, "public", selected.selectionId]);
  assert.equal(button("Preparar meu servidor").disabled, true);
  assert.equal(count("connect:prepareHost"), 0); assert.equal(count("connect:confirmHost"), 0);
});
test("social discovered selection expires at click and IPv6 cannot become a social host source", async () => {
  const selected = discoveryFixture(), control = connection(); control.state.ownedGameTcpDiscovery = true;
  await renderPanel(control); await chooseDiscovery(); Date.now = () => selected.expiresAtMs;
  await click("Preparar meu servidor"); assert.equal(count("connect:prepareHostDiscovered"), 0);
  Date.now = () => fixedNow; discoveryFixture("[::1]:51029");
  await renderPanel(control, { ...baseSession, id: "new-advert", hostUid: "owner-uid" });
  await chooseDiscovery("[::1]:51029"); assert.equal(button("Preparar meu servidor").disabled, true);
  assert.match(container.textContent, /exige uma porta IPv4/);
});
test("changing advert discards selected source and delayed STATUS cannot restore it", async () => {
  const selected = discoveryFixture(), control = connection(); control.state.ownedGameTcpDiscovery = true;
  await renderPanel(control); await chooseDiscovery();
  await renderPanel(control, { ...baseSession, id: "new-advert", hostUid: "owner-uid" });
  assert.equal(button("Preparar meu servidor").disabled, true);
  const reply = deferred(); handlers.connect_confirm_discovered_status = () => reply.promise;
  await chooseDiscovery();
  await renderPanel(control, { ...baseSession, id: "third-advert", hostUid: "owner-uid" });
  await act(async () => reply.resolve(selected));
  assert.equal(button("Preparar meu servidor").disabled, true); assert.equal(count("connect:prepareHostDiscovered"), 0);
});
test("native source checking keeps Disconnect available without exposing a fabricated source", async () => {
  const snapshot = connectionSnapshot("sourceChecking"); snapshot.phase = "starting"; snapshot.social.gameTarget = null; snapshot.social.digest = null;
  const control = connection(snapshot); control.state.ownedGameTcpDiscovery = true;
  await renderPanel(control); assert.match(container.textContent, /Conferindo o jogo selecionado/);
  assert.equal(button("Desconectar").disabled, false); assert.equal(buttons("Compartilhar agora").length, 0);
  await click("Desconectar"); assert.equal(count("connect:stop"), 1);
});

const organizationData = name => ({ group:name ? {id:"group", name, ownerUid:"owner-uid", members:[{uid:"owner-uid",nickname:"Host"}], invited:[], expiresAtMs:fixedNow+60_000}:null, invitations:[], joinRequests:[] });
const organizationRender = async (org,accountId="owner-uid") => act(async()=>root.render(createElement(SocialOrganizationPanel,{service:org,accountId,friends:[]})));
test("group expiration uses the service sample despite a computer year ahead", async()=>{
  Date.now=()=>fixedNow+365*86_400_000;
  await organizationRender({now:()=>fixedNow,snapshot:async()=>organizationData("Grupo atual")});
  assert.doesNotMatch(container.textContent,/Grupo expirado/);
  assert.equal(container.querySelector('select[aria-label="Amigo para convidar ao grupo"]').disabled,false);
});
test("clock failure pauses group polling until an explicit retry succeeds",async()=>{
  let reads=0, fail=true;
  const org={now:()=>fail?NaN:fixedNow,snapshot:async()=>{reads++;if(fail)throw {code:"social_clock_unavailable"};return organizationData("Recuperado");}};
  await organizationRender(org);
  assert.match(container.textContent,/horário do serviço/);
  await act(async()=>document.dispatchEvent(new window.Event("visibilitychange")));
  assert.equal(reads,1);
  fail=false; await click("Atualizar grupos e pedidos");
  assert.equal(reads,2); assert.match(container.textContent,/Recuperado/);
  await act(async()=>document.dispatchEvent(new window.Event("visibilitychange")));
  assert.equal(reads,3);
});
test("expired group keeps its exit action but cannot invite another member", async()=>{
  const value=organizationData("Grupo antigo"); value.group.expiresAtMs=fixedNow-1;
  let left=0;
  await organizationRender({snapshot:async()=>value,leaveGroup:async()=>{left++;}});
  assert.match(container.textContent,/Grupo expirado/);
  assert.equal(container.querySelector('select[aria-label="Amigo para convidar ao grupo"]').disabled,true);
  assert.equal(button("Convidar ao grupo").disabled,true);
  assert.equal(button("Encerrar grupo").disabled,false);
  await click("Encerrar grupo"); assert.equal(left,1);
});
test("group projection refreshes on visible resume and discards old account results", async()=>{
  let reads=0, value=organizationData("Original");
  const original={snapshot:async()=>{reads++;return value;}};
  await organizationRender(original); assert.match(container.textContent,/Original/);
  value=organizationData("Atualizado");
  await act(async()=>document.dispatchEvent(new window.Event("visibilitychange")));
  assert.equal(reads,2); assert.match(container.textContent,/Atualizado/);
  const delayed=deferred(); original.snapshot=()=>delayed.promise;
  await act(async()=>document.dispatchEvent(new window.Event("visibilitychange")));
  await organizationRender({snapshot:async()=>organizationData("Outra conta")},"other-uid");
  await act(async()=>delayed.resolve(organizationData("Nunca exibir")));
  assert.match(container.textContent,/Outra conta/); assert.doesNotMatch(container.textContent,/Nunca exibir|Atualizado/);
});
test("group mutation invalidates pending projection and performs one fresh read after it",async()=>{
  const old=deferred(); let reads=0, mutations=0;
  const org={snapshot:async()=>{reads++;return reads===2?old.promise:organizationData(reads===1?"Antes":"Depois");},leaveGroup:async()=>{mutations++;}};
  await organizationRender(org);
  await act(async()=>document.dispatchEvent(new window.Event("visibilitychange")));
  await click("Encerrar grupo"); assert.equal(mutations,1);
  await act(async()=>old.resolve(organizationData("Antigo")));
  assert.equal(reads,3); assert.match(container.textContent,/Depois/); assert.doesNotMatch(container.textContent,/Antigo/);
});
test("failed group refresh removes previous actionable members and redacts details",async()=>{
  let fail=false;
  const org={snapshot:async()=>{if(fail)throw new Error("token=secret");return organizationData("Ocultar");}};
  await organizationRender(org); fail=true;
  await click("Atualizar grupos e pedidos");
  assert.doesNotMatch(container.textContent,/Ocultar|token=secret/); assert.equal(buttons("Encerrar grupo").length,0);
});
test("group polling has one in-flight read and unmount prevents a delayed update",async()=>{
  const pendingRead=deferred(); let reads=0;
  const org={snapshot:async()=>{reads++;return reads===1?organizationData("Grupo"):pendingRead.promise;}};
  await organizationRender(org);
  await act(async()=>{document.dispatchEvent(new window.Event("visibilitychange"));document.dispatchEvent(new window.Event("visibilitychange"));});
  assert.equal(reads,2);
  await render({service:null});
  await act(async()=>pendingRead.resolve(organizationData("Não restaurar")));
  assert.equal(reads,2); assert.doesNotMatch(container.textContent,/Não restaurar/);
});

function guestVoiceReviewSnapshot() {
  const value = connectionSnapshot("guestReview", "guest");
  Object.assign(value.social, {digest: "b".repeat(64), voiceProfile: "svc", voiceTransportProfile: "nativeVoice8V1", hostPlayer: false, runtimeProfile: "fabric-loader-0.16.14-1.21.1", expiresAtMs: fixedNow + 9000});
  return value;
}
test("native guest review shows approved voice limits and requires its own explicit consent", async () => {
  const snapshot = guestVoiceReviewSnapshot();
  await render({connection: connection(snapshot)});
  for (const text of [/até 8 participantes/, /4\.096 bytes/, /524\.288 bytes\/s/, /1\.600 fragmentos\/s/, /5\.000\.000 bytes\/s/, /128 KiB/, /9 segundos restantes/]) assert.match(container.textContent, text);
  const limits = container.querySelector(".social-voice-limits details");
  assert.ok(limits); assert.equal(limits.hasAttribute("open"), false);
  assert.equal(limits.querySelector("summary").textContent, "Limites técnicos da sessão");
  assert.equal(button("Confirmar entrada com voz").disabled, true); assert.equal(count("connect:confirmGuest"), 0);
  const consent = container.querySelector(".social-connection .social-consent input");
  assert.equal(consent.closest("details"), null);
  await act(async () => {consent.checked = true; consent.dispatchEvent(new window.Event("click", {bubbles:true}));});
  await click("Confirmar entrada com voz"); assert.equal(count("connect:confirmGuest"), 1); assert.equal(count("connect:confirmHost"), 0);
  await render({connection: connection({...snapshot, social:{...snapshot.social,digest:"c".repeat(64)}})});
  assert.equal(button("Confirmar entrada com voz").disabled, true, "a changed original invitation cannot reuse consent");
  await click("Cancelar entrada"); assert.equal(count("connect:stop"), 1);
});
test("native guest review does not renew its original deadline on refresh or approve after expiry", async () => {
  const snapshot = guestVoiceReviewSnapshot(); await render({connection: connection(snapshot)});
  const consent = container.querySelector(".social-connection .social-consent input");
  await act(async () => {consent.checked = true; consent.dispatchEvent(new window.Event("click", {bubbles:true}));});
  Date.now = () => fixedNow + 9000;
  await click("Confirmar entrada com voz"); assert.equal(count("connect:confirmGuest"), 0, "expiry is rechecked at click");
  await render({connection: connection(snapshot)}); assert.equal(button("Confirmar entrada com voz").disabled, true);
  assert.match(container.textContent, /0 segundos restantes/); assert.match(container.textContent, /Esta revisão expirou/);
  await click("Cancelar entrada"); assert.equal(count("connect:stop"), 1);
});
test("native host review exposes the same transport budget without changing legacy reviews", async () => {
  const snapshot = connectionSnapshot(); await render({connection: connection(snapshot)});
  assert.doesNotMatch(container.textContent, /5\.000\.000 bytes\/s/);
  assert.equal(container.querySelector(".social-voice-limits details"), null);
  Object.assign(snapshot.social, {voiceProfile:"plasmo", voiceTransportProfile:"nativeVoice8V1", hostPlayer:true, runtimeProfile:"fabric-loader-0.16.14-1.21.1"});
  await render({connection: connection(snapshot)}); assert.match(container.textContent, /5\.000\.000 bytes\/s/);
  const limits = container.querySelector(".social-voice-limits details");
  assert.ok(limits); assert.equal(limits.hasAttribute("open"), false);
  assert.equal(limits.querySelector("summary").textContent, "Limites técnicos da sessão");
  for (const text of [/4\.096 bytes/, /524\.288 bytes\/s/, /1\.600 fragmentos\/s/, /5\.000\.000 bytes\/s/, /128 KiB/]) assert.match(limits.textContent, text);
  const visible = container.cloneNode(true); visible.querySelectorAll("details").forEach(details => details.remove());
  for (const text of [/até 8 participantes/, /Plasmo Voice/, /Experimental/, /para a comunidade/, /por até 1 minutos, dentro do prazo da origem/, /Áudio dentro do jogo ainda não validado/]) assert.match(visible.textContent, text);
  assert.doesNotMatch(visible.textContent, /5\.000\.000 bytes\/s|1\.600 fragmentos\/s/);
  assert.equal(button("Compartilhar agora").closest("details"), null);
  limits.setAttribute("open", "");
  await act(async () => limits.dispatchEvent(new window.Event("toggle")));
  assert.equal(button("Compartilhar agora").disabled, true);
  assert.equal(count("connect:confirmHost"), 0, "opening technical details cannot grant consent");
});
