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

test("catalog uses card/list layouts with modpack version and declared population", async () => {
  page.items = [baseSession]; await render();
  assert.match(container.textContent, /Mundo da montanha/); assert.match(container.textContent, /Aurora Valley/);
  assert.match(container.textContent, /1\.21\.1/); assert.match(container.textContent, /2 \/ 8/);
  assert.equal(container.querySelector(".social-catalog").dataset.layout, "grid");
  const image = container.querySelector(".social-session img");
  assert.equal(image.getAttribute("referrerpolicy") ?? image.getAttribute("referrerPolicy"), "no-referrer");
  await click("Lista"); assert.equal(container.querySelector(".social-catalog").dataset.layout, "list");
});

test("join capability false never invokes prepareJoin, including synthetic click", async () => {
  page.items = [baseSession]; state.friends = [{ ...person, presence: null, session: baseSession }]; await render();
  assert.equal(buttons("Entrar").length, 2);
  for (const element of buttons("Entrar")) {
    assert.equal(element.disabled, true);
    await act(async () => element.dispatchEvent(new window.Event("click", { bubbles: true })));
  }
  assert.equal(count("prepareJoin"), 0); assert.match(container.textContent, /conexão social segura estiver integrada/);
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
  for (const name of ["prepareHost", "prepareHostDiscovered", "confirmHost", "stop", "refresh"]) controller[name] = async (...args) => { calls.push({ name: `connect:${name}`, args }); };
  return { state: { available: true, pending: false, error: "", snapshot }, controller };
}
function connectionSnapshot(stage = "hostReview", role = "host") {
  return { operationId: "own-operation", generation: 2, phase: stage === "connected" ? "guestConnected" : "reviewed", role,
    cleanupState: "pending", tcpAddress: role === "guest" && stage === "connected" ? "127.0.0.1:42010" : null,
    social: { sessionId: baseSession.id, role, stage, digest: role === "host" ? "a".repeat(64) : null,
      gameTarget: role === "host" ? "127.0.0.1:43001" : null, visibility: role === "host" ? "public" : null, expiresAtMs: fixedNow + 60_000 } };
}
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
test("host preparation requires an eligible own advert and never auto-approves", async () => {
  state.ownSession = { ...baseSession, hostUid: "owner-uid" };
  await render({ connection: connection() }); assert.equal(count("connect:prepareHost"), 0);
  await click("Preparar meu servidor"); assert.deepEqual(calls.find(call => call.name === "connect:prepareHost").args, [baseSession.id, "public"]);
  assert.equal(count("connect:confirmHost"), 0);
});
test("host review shows fixed source scope deadline and starts with consent unchecked", async () => {
  await render({ connection: connection(connectionSnapshot()) });
  assert.match(container.textContent, /127\.0\.0\.1:43001/); assert.match(container.textContent, /60 segundos/);
  assert.match(container.textContent, /com uma vaga/); assert.equal(button("Compartilhar agora").disabled, true);
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
