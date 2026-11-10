// Synthetic DOM/service projections only; no account, network or native process.
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
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { userAgent: "Synthetic group selection" } });
const modules = new Map();
async function sourceModule(source) {
  if (modules.has(source.href)) return modules.get(source.href);
  let output = ts.transpileModule(await readFile(source, "utf8"), { fileName: fileURLToPath(source), compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX,
  } }).outputText;
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
const { SocialOrganizationPanel } = await import(await sourceModule(new URL("./SocialOrganizationPanel.tsx", import.meta.url)));
const container = document.getElementById("root");
const alice = { uid: "alice", nickname: "Alice", avatarUrl: null };
const bob = { uid: "bob", nickname: "Bob", avatarUrl: null };
const now = 1_800_000_000_000;
const group = (ownerUid = "host", expiresAtMs = now + 60_000) => ({
  id: ownerUid, name: "Grupo", ownerUid, members: [{ uid: ownerUid, nickname: "Host", avatarUrl: null }], invited: [], expiresAtMs,
});
let root;
const button = () => [...container.querySelectorAll("button")].find(item => item.textContent === "Convidar ao grupo");
const select = () => container.querySelector('[aria-label="Amigo para convidar ao grupo"]');
const render = async (service, props = {}) => act(async () => root.render(createElement(SocialOrganizationPanel, { service, accountId: "host", friends: [alice, bob], ...props })));
const choose = async uid => act(async () => {
  const option = [...select().querySelectorAll("option")].find(item => item.value === uid); assert.ok(option);
  option.selected = true; select().dispatchEvent(new window.Event("change", { bubbles: true }));
});
const click = async () => act(async () => button().dispatchEvent(new window.Event("click", { bubbles: true })));
const refresh = async () => act(async () => document.dispatchEvent(new window.Event("visibilitychange")));
function fixture(value = group()) {
  const state = { group: value, calls: [], now };
  state.service = { now: () => state.now, snapshot: async () => ({ group: structuredClone(state.group), invitations: [], joinRequests: [] }),
    inviteToGroup: async uid => { state.calls.push(uid); state.group.invited.push([alice, bob].find(item => item.uid === uid)); } };
  return state;
}
beforeEach(() => { root = createRoot(container); });
afterEach(async () => act(async () => root.unmount()));

test("successful group invitation clears selection and cannot resend the removed option", async () => {
  const f = fixture(); await render(f.service); await choose("alice");
  assert.equal(button().disabled, false); await click();
  assert.deepEqual(f.calls, ["alice"]);
  assert.equal(select().value, ""); assert.equal(button().disabled, true);
  assert.equal([...select().querySelectorAll("option")].some(option => option.value === "alice"), false);
  await click(); assert.deepEqual(f.calls, ["alice"]);
  await choose("bob"); assert.equal(button().disabled, false);
});

test("current roster and friendship inventory invalidate an unavailable selection", async () => {
  const f = fixture(); await render(f.service); await choose("alice");
  f.group.invited.push(alice); await refresh();
  assert.equal(select().value, ""); assert.equal(button().disabled, true);
  await click(); assert.deepEqual(f.calls, []);
  f.group.invited = []; await refresh();
  assert.equal(select().value, ""); assert.equal(button().disabled, true);
  await choose("bob"); await render(f.service, { friends: [alice] });
  assert.equal(select().value, ""); assert.equal(button().disabled, true);
  await click(); assert.deepEqual(f.calls, []);
  await render(f.service);
  assert.equal(select().value, ""); assert.equal(button().disabled, true);
});

test("a recreated group requires a new selection even with the same owner and friends", async () => {
  const f = fixture(); await render(f.service); await choose("alice");
  f.group = group("host", now + 120_000); await refresh();
  assert.equal(select().value, ""); assert.equal(button().disabled, true);
  await click(); assert.deepEqual(f.calls, []);
  await choose("alice"); await click(); assert.deepEqual(f.calls, ["alice"]);
});

test("a late success from another account cannot clear its replacement group's selection", async () => {
  const original = fixture(); let complete;
  original.service.inviteToGroup = uid => { original.calls.push(uid); return new Promise(resolve => { complete = resolve; }); };
  await render(original.service); await choose("alice"); await click();
  const replacement = fixture(group("other-host"));
  await render(replacement.service, { accountId: "other-host" }); await choose("bob");
  await act(async () => complete());
  assert.equal(select().value, "bob"); assert.equal(button().disabled, false);
  await click(); assert.deepEqual(original.calls, ["alice"]); assert.deepEqual(replacement.calls, ["bob"]);
});

test("invitation failure retains the eligible choice and expiry is rechecked at the click", async () => {
  const f = fixture(); f.service.inviteToGroup = async uid => { f.calls.push(uid); throw new Error("private detail"); };
  await render(f.service); await choose("alice"); await click();
  assert.equal(select().value, "alice"); assert.equal(button().disabled, false);
  assert.doesNotMatch(container.textContent, /private detail/);
  f.now = f.group.expiresAtMs; await click(); assert.deepEqual(f.calls, ["alice"]);
});

test("the existing contract caps active invitations separately from current members", async () => {
  const f = fixture();
  f.group.members.push(...Array.from({ length: 7 }, (_, i) => ({ uid: `member-${i}`, nickname: `Member${i}`, avatarUrl: null })));
  f.group.invited.push(...Array.from({ length: 7 }, (_, i) => ({ uid: `invited-${i}`, nickname: `Invited${i}`, avatarUrl: null })));
  await render(f.service); await choose("alice"); assert.equal(button().disabled, false);
  f.group.invited.push(bob); await refresh(); assert.equal(button().disabled, true);
  await click(); assert.deepEqual(f.calls, []);
});
