// Run from apps/desktop: node src/features/themes/ThemeVisualEditor.smoke.mjs
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join, resolve, sep } from "node:path";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";
import { parseHTML } from "linkedom";
import React, { act, useState } from "react";
import { createRoot } from "react-dom/client";

test("visual editing and commented code stay synchronized without applying unsaved themes", { timeout: 20_000 }, async () => {
  const desktop = fileURLToPath(new URL("../../../", import.meta.url));
  const cache = resolve(desktop, "node_modules/.cache");
  await mkdir(cache, { recursive: true });
  const output = await mkdtemp(join(cache, "aurora-visual-smoke-"));
  let root;
  const settle = async (action) => { await act(async () => { await action(); await new Promise((done) => setTimeout(done, 0)); }); };
  try {
    for (const file of ["model.ts", "ThemeVisualEditor.tsx", "ThemeManager.tsx"]) {
      let source = await readFile(new URL(file, import.meta.url), "utf8");
      source = source.replace(/^import ["']\.\/[^"']+\.css["'];?\r?\n/gm, "")
        .replace(/from ["']\.\/(model|ThemeVisualEditor)["']/g, (_, name) => `from './${name}.mjs'`);
      const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX } });
      await writeFile(join(output, file.replace(/\.tsx?$/, ".mjs")), compiled.outputText);
    }
    const { window } = parseHTML('<!doctype html><html><body><div id="root"></div></body></html>');
    Object.assign(globalThis, { window, document: window.document, HTMLElement: window.HTMLElement, Event: window.Event, IS_REACT_ACT_ENVIRONMENT: true });
    globalThis.FileReader = class {
      readAsDataURL(blob) {
        void blob.arrayBuffer().then((buffer) => { this.result = `data:${blob.type};base64,${Buffer.from(buffer).toString("base64")}`; this.onload?.(); });
      }
    };
    window.HTMLElement.prototype.setPointerCapture = function (id) { this.__testPointer = id; };
    window.HTMLElement.prototype.hasPointerCapture = function (id) { return this.__testPointer === id; };
    window.HTMLElement.prototype.releasePointerCapture = function () { this.__testPointer = null; };
    const { ThemeManager } = await import(pathToFileURL(join(output, "ThemeManager.mjs")));
    const { parseTheme, formatThemeCode, THEME_EXAMPLES } = await import(pathToFileURL(join(output, "model.mjs")));
    const previews = [];
    const saves = [];
    const reverseKeys = (value) => Array.isArray(value) ? value.map(reverseKeys) : value && typeof value === "object" ? Object.fromEntries(Object.entries(value).reverse().map(([key, item]) => [key, reverseKeys(item)])) : value;
    function Harness() {
      const [library, setLibrary] = useState({ themes: [], selectedThemeId: null, bindings: {}, automatic: false, recovered: false, persistent: true });
      return React.createElement(ThemeManager, {
        library, instances: [{ id: "modpack-a", name: "Modpack A" }], selectedInstanceId: "modpack-a", busy: false, error: "",
        onSave: async (json) => { const theme = parseTheme(json); saves.push(theme); setLibrary((current) => ({ ...current, themes: [reverseKeys(theme)] })); },
        onPreview: async (id, instance) => { previews.push([id, instance]); },
        onDelete: async () => {}, onReset: async () => {}, onAutomatic: async () => {}, onUnbind: async () => {},
      });
    }
    const button = (text) => [...document.querySelectorAll("button")].find((node) => node.textContent.trim() === text);
    const click = (text) => { const node = button(text); assert.ok(node, `Button ${text} must exist`); node.click(); };
    // Linkedom has no complete controlled-input event implementation; dispatch the
    // real registered React change handler while retaining the rendered component.
    const change = (node, value) => { const props = Object.entries(node).find(([key]) => key.startsWith("__reactProps$"))?.[1]; assert.equal(typeof props?.onChange, "function"); props.onChange({ target: { value } }); };
    const code = () => document.querySelector(".theme-manager-code-panel textarea");
    const renderedValue = (node) => Object.entries(node).find(([key]) => key.startsWith("__reactProps$"))?.[1].value;
    const drag = (node, type) => { const event = new window.Event(type, { bubbles: true, cancelable: true }); event.dataTransfer = { setData() {}, effectAllowed: "move" }; node.dispatchEvent(event); };
    const pointer = (node, type, x, y) => { const event = new window.Event(type, { bubbles: true, cancelable: true }); Object.assign(event, { pointerId: 1, button: 0, clientX: x, clientY: y }); node.dispatchEvent(event); };
    root = createRoot(document.getElementById("root"));
    await settle(async () => root.render(React.createElement(Harness)));
    assert.ok(document.querySelector(".theme-visual-editor"));
    assert.equal(previews.length, 0);
    await settle(async () => click("Código comentado"));
    assert.ok(renderedValue(code()).includes("//"));
    await settle(async () => change(code(), "{ broken draft"));
    await settle(async () => click("Editor visual"));

    assert.equal(renderedValue(code()), "{ broken draft", "invalid code is preserved when switching modes fails");
    assert.ok(document.querySelector(".theme-manager-error"));
    await settle(async () => change(code(), formatThemeCode(THEME_EXAMPLES[1])));
    await settle(async () => click("Editor visual"));
    assert.equal(document.querySelectorAll(".tv-nav").length, 6);
    assert.ok([...document.querySelectorAll(".tv-nav")].some((item) => item.textContent.includes("Aurora Connect")));
    assert.ok([...document.querySelectorAll(".tv-nav")].some((item) => item.textContent.includes("Amigos e comunidade")));
    assert.equal(renderedValue(document.querySelector(".tv-identity input")), THEME_EXAMPLES[1].name);
    await settle(async () => change(document.querySelector(".tv-identity input"), "Meu jardim editado"));
    await settle(async () => click("Cursor"));
    await settle(async () => change(document.querySelector(".tv-controls select"), "ring"));
    await settle(async () => change(document.querySelector('.tv-controls input[type="range"]'), "32"));
    await settle(async () => change(document.querySelectorAll(".tv-controls select")[1], "sparkle"));
    await settle(async () => click("Salvar na biblioteca"));
    assert.equal(saves.length, 1);
    assert.equal(saves[0].name, "Meu jardim editado", "saving the visual source never reads a stale code cache");
    assert.deepEqual(saves[0].cursor, { style: "ring", color: THEME_EXAMPLES[1].cursor.color, size: 32, trail: "sparkle" });
    await settle(async () => click("Código comentado"));
    assert.equal(parseTheme(renderedValue(code())).name, "Meu jardim editado");
    assert.equal(parseTheme(renderedValue(code())).cursor.style, "ring");
    assert.equal(previews.length, 0, "local changes never apply to the real launcher");
    await settle(async () => click("Testar por 10 s"));
    assert.deepEqual(previews, [[THEME_EXAMPLES[1].id, null]], "native key order does not make a freshly saved theme look unsaved");

    const gif = Uint8Array.from([71, 73, 70, 56, 57, 97, 1, 0, 1, 0, 128, 0, 0, 0, 0, 0, 255, 255, 255, 33, 249, 4, 0, 4, 0, 0, 0, 44, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 68, 1, 0, 59]);
    const gifFile = new File([gif], "tiny-loop.gif", { type: "image/gif" });
    const chooseFile = (label) => { const input = document.querySelector(`input[aria-label="${label}"]`); const props = Object.entries(input).find(([key]) => key.startsWith("__reactProps$"))?.[1]; props.onChange({ target: { files: [gifFile], value: "" } }); };
    await settle(async () => chooseFile("Selecionar imagem, GIF ou vídeo de fundo"));
    const withGif = parseTheme(renderedValue(code()));
    assert.equal(withGif.assets[withGif.media.background].mime, "image/gif");
    assert.equal(withGif.assets[withGif.media.background].data, Buffer.from(gif).toString("base64"), "GIF import preserves the embedded animation bytes");
    const gifDraft = renderedValue(code());
    await settle(async () => chooseFile("Selecionar logotipo"));
    assert.ok(document.querySelector(".theme-manager-error").textContent.includes("somente como fundo"));
    assert.equal(renderedValue(code()), gifDraft, "rejecting an animated logo never replaces the draft");

    await settle(async () => change(code(), `${renderedValue(code())}\n// unsaved note`));
    const unsaved = renderedValue(code());
    await settle(async () => click("Lago de vidro"));
    assert.ok(document.querySelector('[role="alertdialog"]'));
    assert.equal(renderedValue(code()), unsaved);
    await settle(async () => click("Continuar editando"));
    assert.equal(renderedValue(code()), unsaved);
    await settle(async () => click("Lago de vidro"));
    await settle(async () => click("Descartar rascunho e abrir"));
    assert.equal(parseTheme(renderedValue(code())).id, "lago-de-vidro");
    await settle(async () => click("Editor visual"));

    const initialNav = [...document.querySelectorAll(".tv-nav")];
    await settle(async () => drag(initialNav[1], "dragstart"));
    await settle(async () => drag(initialNav[0], "drop"));
    assert.ok(document.querySelector(".tv-nav").textContent.includes("Guarda-roupa"));
    await settle(async () => drag(document.querySelector(".tv-sidebar"), "dragstart"));
    await settle(async () => drag(document.querySelector(".tv-side-drop-left"), "drop"));
    await settle(async () => click("Código comentado"));
    assert.equal(parseTheme(renderedValue(code())).layout.sidebar, "left");
    assert.equal(parseTheme(renderedValue(code())).layout.navOrder[0], "appearance");
    await settle(async () => click("Editor visual"));
    await settle(async () => click("Organização"));
    const downJava = document.querySelector('[aria-label="Descer Java"]');
    assert.equal(downJava.disabled, false, "Java is no longer the final navigation section");
    assert.equal(document.querySelector('[aria-label="Descer Amigos e comunidade"]').disabled, true);
    await settle(async () => downJava.click());
    await settle(async () => click("Código comentado"));
    assert.deepEqual(parseTheme(renderedValue(code())).layout.navOrder, ["appearance", "instances", "discover", "connect", "java", "social"]);
    assert.equal(previews.length, 1, "reordering a new section does not authorize or activate it");

    const dragTheme = { ...THEME_EXAMPLES[1], decorations: THEME_EXAMPLES[1].decorations.map((item) => ({ ...item, width: 35 })) };
    await settle(async () => change(code(), formatThemeCode(dragTheme)));
    await settle(async () => click("Editor visual"));
    const area = document.querySelector(".tv-decoration-area");
    area.getBoundingClientRect = () => ({ x: 0, y: 0, width: 400, height: 88 });
    let item = document.querySelector(".tv-decoration");
    await settle(async () => pointer(item, "pointerdown", 100, 100));
    await settle(async () => pointer(item, "pointermove", 152, 126));
    await settle(async () => pointer(item, "pointerup", 152, 126));
    await settle(async () => click("Código comentado"));
    const positioned = parseTheme(renderedValue(code())).decorations[0];
    assert.ok(positioned.x > 50 && positioned.y > 50, "pointer movement commits normalized coordinates on release");
    await settle(async () => click("Editor visual"));
    item = document.querySelector(".tv-decoration");
    await settle(async () => pointer(item, "pointerdown", 100, 100));
    await settle(async () => pointer(item, "pointermove", 0, 0));
    await settle(async () => pointer(item, "pointercancel", 0, 0));
    await settle(async () => click("Código comentado"));
    assert.deepEqual(parseTheme(renderedValue(code())).decorations[0], positioned, "cancelled dragging preserves the saved draft position");
    assert.equal(previews.length, 1);
  } finally {
    if (root) await settle(async () => root.unmount());
    if (!resolve(output).startsWith(`${cache}${sep}`)) throw new Error("Unexpected smoke-test cache path.");
    await rm(output, { recursive: true, force: true });
  }
});
