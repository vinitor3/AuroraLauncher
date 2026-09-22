import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { setImmediate } from "node:timers/promises";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";
import { isPlayerIdentifier } from "../src/lib/playerSkins.ts";

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function setup() {
  const slots = [], effects = [], timers = new Map(), requests = [], selected = [];
  let cursor = 0, timerId = 0, props = { workerUrl: "https://aurora.test", onSelect: (skin) => selected.push(skin) };
  const hooks = {
    useRef: (value) => slots[cursor++] ??= { current: value },
    useState(value) {
      const slot = cursor++;
      if (!(slot in slots)) slots[slot] = value;
      return [slots[slot], (next) => { slots[slot] = typeof next === "function" ? next(slots[slot]) : next; }];
    },
    useMemo(factory, dependencies) {
      const slot = cursor++;
      if (!slots[slot] || dependencies.some((value, index) => value !== slots[slot].dependencies[index])) slots[slot] = { value: factory(), dependencies };
      return slots[slot].value;
    },
    useEffect(effect, dependencies) {
      const slot = cursor++, previous = slots[slot];
      if (previous && dependencies.every((value, index) => Object.is(value, previous.dependencies[index]))) return;
      effects.push(() => { previous?.cleanup?.(); slots[slot] = { dependencies, cleanup: effect() }; });
    },
  };
  const request = (kind, query, signal) => { const result = deferred(); requests.push({ kind, query, signal, ...result }); return result.promise; };
  class PlayerSkinsClient {
    autocompleteSkinNames(query, signal) { return request("search", query, signal); }
    getPlayerProfile(query, signal) { return request("profile", query, signal); }
    getCatalog(query, page, signal) { return request("catalog", `${page}:${query}`, signal); }
    resolveCatalogUrl(entry) { return entry.source; }
  }
  const runtime = { jsx: (type, props) => ({ type, props }), jsxs: (type, props) => ({ type, props }) };
  const exports = {};
  const code = ts.transpileModule(readFileSync(new URL("../src/features/skins/OnlineSkinSearch.tsx", import.meta.url), "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  vm.runInNewContext(code, { exports, AbortController, Error,
    clearTimeout: (id) => timers.delete(id), window: { setTimeout: (fn) => { timers.set(++timerId, fn); return timerId; } },
    require: (name) => name === "react" ? hooks : name === "react/jsx-runtime" ? runtime
      : name.includes("playerSkins") ? { PlayerSkinsClient, isPlayerIdentifier } : {},
  });
  return { requests, selected,
    render(next = {}) { props = { ...props, ...next }; cursor = 0; const tree = exports.OnlineSkinSearch(props); while (effects.length) effects.shift()(); return tree; },
    tick() { for (const [id, callback] of timers) { timers.delete(id); callback(); } },
    unmount() { slots.forEach((slot) => slot?.cleanup?.()); },
  };
}

function find(tree, predicate) {
  if (!tree || typeof tree !== "object") return undefined;
  if (predicate(tree)) return tree;
  for (const child of [tree.props?.children].flat(Infinity)) { const found = find(child, predicate); if (found) return found; }
}
const button = (tree, text) => find(tree, (node) => node.type === "button" && node.props.children === text);
const form = (tree) => find(tree, (node) => node.type === "form");
const input = (tree) => find(tree, (node) => node.type === "input");
const status = (tree) => find(tree, (node) => node.props?.role === "status").props.children;
const player = { name: "Notch", model: "classic", skinUrl: "https://aurora.test/skin.png", contentHash: "a".repeat(64) };

test("fresh wardrobe starts with a browsable catalog without typing or importing a file", async () => {
  const run = setup();
  run.render(); run.tick();
  assert.equal(run.requests[0].kind, "catalog");
  assert.equal(run.requests[0].query, "0:");
  run.requests[0].resolve({ entries: [], page: 0, hasMore: false });
  await setImmediate();
  assert.equal(status(run.render()), "Nenhuma skin encontrada.");
  run.unmount();
});

test("profile submit cancels autocomplete so a late error cannot hide a valid player skin", async () => {
  const run = setup();
  button(run.render(), "Jogadores").props.onClick();
  input(run.render()).props.onChange({ target: { value: "Notch" } });
  run.render(); run.tick();
  assert.equal(run.requests[0].kind, "search");
  const pending = run.render();
  assert.equal(button(pending, "Buscar perfil").props.disabled, false);
  form(pending).props.onSubmit({ preventDefault() {} });
  assert.equal(run.requests[0].signal.aborted, true);
  run.requests[1].resolve(player);
  await setImmediate();
  run.requests[0].reject(new Error("Jogador não encontrado."));
  await setImmediate();
  assert.deepEqual(run.selected, [player]);
  assert.match(status(run.render()), /Notch carregada/);
  run.unmount();
});

test("disabled wardrobe aborts pending profile and ignores a late selection", async () => {
  const run = setup();
  button(run.render(), "Jogadores").props.onClick();
  input(run.render()).props.onChange({ target: { value: "Notch" } });
  form(run.render()).props.onSubmit({ preventDefault() {} });
  const disabled = run.render({ disabled: true });
  assert.equal(button(disabled, "Buscar perfil").props.disabled, true);
  assert.equal(run.requests[0].signal.aborted, true);
  run.requests[0].resolve(player);
  await setImmediate();
  assert.equal(run.selected.length, 0);
  run.unmount();
});
