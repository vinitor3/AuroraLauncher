// Run from apps/desktop: node src/features/themes/ThemeProvider.smoke.mjs
// Exercises the real provider and temporary browser transport without credentials.
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join, resolve, sep } from "node:path";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";
import { parseHTML } from "linkedom";
import React, { act } from "react";
import { createRoot } from "react-dom/client";

test("preview, binding context, confirmation and ten-second rollback use the actual provider", { timeout: 30_000 }, async () => {
  const desktop = fileURLToPath(new URL("../../../", import.meta.url));
  const cache = resolve(desktop, "node_modules/.cache");
  await mkdir(cache, { recursive: true });
  const output = await mkdtemp(join(cache, "aurora-theme-smoke-"));
  let root;
  const settle = async (action) => {
    await act(async () => { await action(); await new Promise((done) => setTimeout(done, 0)); });
  };
  try {
    // Transpile a fresh copy to keep this test independent of build artifacts.
    for (const file of ["model.ts", "transport.ts", "ThemeProvider.tsx"]) {
      let source = await readFile(new URL(file, import.meta.url), "utf8");
      source = source.replace(/^import ["']\.\/[^"']+\.css["'];?\r?\n/gm, "")
        .replace(/from ["']\.\/(model|transport)["']/g, (_, name) => `from './${name}.mjs'`);
      const compiled = ts.transpileModule(source, {
        compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX },
      });
      await writeFile(join(output, file.replace(/\.tsx?$/, ".mjs")), compiled.outputText);
    }
    const { window } = parseHTML('<!doctype html><html><body><div id="root"></div></body></html>');
    Object.assign(globalThis, { window, document: window.document, HTMLElement: window.HTMLElement, Event: window.Event, KeyboardEvent: window.Event, IS_REACT_ACT_ENVIRONMENT: true });
    window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
    window.requestAnimationFrame = (callback) => setTimeout(callback, 0);
    window.cancelAnimationFrame = clearTimeout;
    const { ThemeProvider, useThemes } = await import(pathToFileURL(join(output, "ThemeProvider.mjs")));
    const { THEME_EXAMPLES } = await import(pathToFileURL(join(output, "model.mjs")));
    let current;
    function Harness() {
      current = useThemes();
      return React.createElement("output", { id: "actual-theme" }, current.theme.id);
    }
    root = createRoot(document.getElementById("root"));
    await settle(async () => root.render(React.createElement(ThemeProvider, null, React.createElement(Harness))));
    const [a, b] = THEME_EXAMPLES;
    await settle(() => current.save(JSON.stringify(a)));
    await settle(() => current.save(JSON.stringify(b)));
    await settle(async () => current.setInstance("modpack-a"));
    await settle(() => current.preview(a.id, "modpack-a"));
    assert.equal(current.theme.id, a.id);
    assert.ok(document.querySelector(".theme-confirmation"));
    await settle(async () => document.querySelector(".theme-confirmation button").click());
    assert.equal(current.library.bindings["modpack-a"], a.id);
    assert.equal(current.theme.id, a.id);

    await settle(async () => current.setInstance("modpack-b"));
    assert.equal(current.theme.id, "aurora-default");
    await settle(() => current.preview(b.id, "modpack-a"));
    assert.equal(current.theme.id, b.id);
    await settle(async () => document.querySelector(".theme-confirmation button").click());
    assert.equal(current.theme.id, "aurora-default", "confirming another modpack must preserve the actual selected context");
    await settle(async () => current.setInstance("modpack-a"));
    assert.equal(current.theme.id, b.id);
    await settle(() => current.automatic(false));
    assert.equal(current.theme.id, "aurora-default");

    await settle(() => current.preview(a.id, null));
    await settle(async () => document.querySelector(".theme-confirmation button").click());
    assert.equal(current.theme.id, a.id);

    // Simulate a failed native bridge while retaining the actual provider logic.
    globalThis.isTauri = true;
    window.__TAURI_INTERNALS__ = { invoke: async () => { throw new Error("Simulated IPC outage"); } };
    await settle(async () => document.dispatchEvent(new window.Event("visibilitychange")));
    assert.equal(current.theme.id, "aurora-default");
    assert.equal(current.library.selectedThemeId, null);
    assert.equal(current.library.automatic, false);
    await settle(async () => { await new Promise((done) => setTimeout(done, 20)); });
    assert.equal(current.theme.id, "aurora-default", "supervision failure must never reapply the previous theme");
    globalThis.isTauri = false;
    delete window.__TAURI_INTERNALS__;

    await settle(() => current.preview(a.id, null));
    await settle(async () => document.querySelector(".theme-confirmation button").click());
    globalThis.isTauri = true;
    window.__TAURI_INTERNALS__ = { invoke: async () => { throw new Error("Simulated IPC outage"); } };
    await settle(() => assert.rejects(current.reset(), /Simulated IPC outage/));
    assert.equal(current.theme.id, "aurora-default");
    assert.equal(current.library.selectedThemeId, null, "failed native reset must keep the frontend on its safe state");
    globalThis.isTauri = false;
    delete window.__TAURI_INTERNALS__;

    await settle(() => current.reset());
    assert.equal(current.theme.id, "aurora-default");
    assert.equal(current.library.selectedThemeId, null);

    await settle(() => current.preview(b.id, null));
    await settle(async () => { await new Promise((done) => setTimeout(done, 10_200)); });
    assert.equal(current.theme.id, "aurora-default");
    assert.equal(current.library.selectedThemeId, null);
    assert.equal(current.library.automatic, false);
    assert.equal(document.querySelector(".theme-confirmation"), null);
    assert.equal(current.library.themes.length, 2, "rollback preserves the saved library");
  } finally {
    globalThis.isTauri = false;
    if (globalThis.window) delete window.__TAURI_INTERNALS__;
    if (root) await settle(async () => root.unmount());
    // Delete only the unique, verified cache directory created by this test.
    if (!resolve(output).startsWith(`${cache}${sep}`)) throw new Error("Unexpected smoke-test cache path.");
    await rm(output, { recursive: true, force: true });
  }
});
