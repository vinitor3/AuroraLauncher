import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";

// Run the real component and inspect its JSX tree. CSS motion itself is checked
// separately in a browser; this suite protects accessible state and interaction.
const filename = new URL("../src/features/auth/StartupScreen.tsx", import.meta.url);
const code = ts.transpileModule(readFileSync(filename, "utf8"), {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX },
}).outputText;
const exports = {};
const runtime = { jsx: (type, props, key) => ({ type, props, key }), jsxs: (type, props, key) => ({ type, props, key }) };
vm.runInNewContext(code, {
  exports,
  require: (name) => {
    assert.equal(name, "react/jsx-runtime");
    return runtime;
  },
});
const render = (props = {}) => exports.StartupScreen(props);

function elements(node, predicate = () => true) {
  if (Array.isArray(node)) return node.flatMap((child) => elements(child, predicate));
  if (!node || typeof node !== "object") return [];
  return [...(predicate(node) ? [node] : []), ...elements(node.props?.children, predicate)];
}
const withClass = (node, name) => elements(node, (element) => element.props.className?.split(/\s+/).includes(name));
const withRole = (node, role) => elements(node, (element) => element.props.role === role);
const buttons = (node) => elements(node, (element) => element.type === "button");
function text(node) {
  if (Array.isArray(node)) return node.map(text).join("");
  if (typeof node === "string" || typeof node === "number") return String(node);
  return node && typeof node === "object" ? text(node.props?.children) : "";
}

test("loading is busy with one named status and no login form or retry actions", () => {
  const tree = render();
  assert.equal(tree.type, "main");
  assert.equal(tree.props["aria-busy"], true);
  assert.equal(withRole(tree, "status").length, 1);
  assert.ok(text(withRole(tree, "status")[0]).trim().length > 0);
  assert.equal(withRole(tree, "alert").length, 0);
  assert.equal(buttons(tree).length, 0);
  assert.equal(elements(tree, (element) => element.type === "form" || element.type === "input").length, 0);
  const section = elements(tree, (element) => element.type === "section")[0];
  const title = elements(tree, (element) => element.type === "h1")[0];
  assert.equal(section.props["aria-labelledby"], title.props.id);
});

test("loading animation and progress are decorative rather than repeated screen-reader announcements", () => {
  const tree = render();
  const emblem = withClass(tree, "startup-emblem")[0];
  const loading = withClass(tree, "startup-loading")[0];
  assert.ok(emblem);
  assert.ok(loading);
  assert.equal(String(emblem.props["aria-hidden"]), "true");
  assert.equal(String(loading.props["aria-hidden"]), "true");
  assert.ok(withClass(emblem, "startup-orbit").length >= 1);
  assert.equal(withClass(emblem, "startup-star").length, 1);
  assert.equal(withClass(loading, "startup-progress").length, 1);
  assert.ok(elements(loading, (element) => element.type === "span").length >= 3);
  assert.equal(elements(loading, (element) => element.props.role || element.props["aria-live"]).length, 0);
  assert.equal(elements(tree, (element) => element.props["aria-valuenow"] !== undefined).length, 0,
    "session restoration must not claim measured percentage progress");
});

test("error stops busy state, announces the exact error, and removes all loading indicators", () => {
  const error = "Não foi possível recuperar a sessão.";
  const tree = render({ error });
  assert.equal(tree.props["aria-busy"], false);
  assert.equal(withRole(tree, "alert").length, 1);
  assert.equal(text(withRole(tree, "alert")[0]), error);
  assert.equal(withRole(tree, "status").length, 0);
  assert.equal(withClass(tree, "startup-content-error").length, 1);
  assert.equal(withClass(tree, "startup-loading").length, 0);
  assert.equal(withClass(tree, "startup-progress").length, 0);
  assert.equal(buttons(tree).length, 1);
  assert.ok(buttons(tree).every((button) => button.props.type === "button"));
});

test("retry and sign-out buttons invoke only their respective supplied callbacks", () => {
  const calls = [];
  const tree = render({ error: "Falha temporária", onRetry: () => calls.push("retry"), onSignOut: () => calls.push("signout") });
  assert.deepEqual(calls, [], "render must never retry or sign out automatically");
  const retry = buttons(tree).find((button) => text(button) === "Tentar novamente");
  const signout = buttons(tree).find((button) => text(button) === "Voltar para entrar");
  assert.ok(retry && signout);
  retry.props.onClick();
  assert.deepEqual(calls, ["retry"]);
  signout.props.onClick();
  assert.deepEqual(calls, ["retry", "signout"]);
});

test("retry render returns to loading without stale errors, actions, or authenticated UI", () => {
  const failed = render({ error: "Sessão indisponível" });
  assert.equal(withRole(failed, "alert").length, 1);
  const retrying = render();
  assert.equal(retrying.props["aria-busy"], true);
  assert.equal(withClass(retrying, "startup-content-error").length, 0);
  assert.equal(withClass(retrying, "startup-loading").length, 1);
  assert.equal(withRole(retrying, "alert").length, 0);
  assert.equal(buttons(retrying).length, 0);
  assert.ok(!text(retrying).includes("Sessão indisponível"));
});

test("optional error callbacks do not prevent a safe render", () => {
  const tree = render({ error: "Sem conexão" });
  assert.equal(buttons(tree).length, 1);
  assert.ok(buttons(tree).every((button) => button.props.onClick === undefined));
  assert.equal(tree.props["aria-busy"], false);
});

test("pre-authentication startup errors offer retry without an unsafe sign-out action", () => {
  const tree = render({ error: "Sessão da janela indisponível", onRetry() {} });
  assert.deepEqual(buttons(tree).map(text), ["Tentar novamente"]);
});
