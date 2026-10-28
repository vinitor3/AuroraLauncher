import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join, resolve, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";
import ts from "typescript";
import { parseHTML } from "linkedom";
import React, { act, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

test("layout moves the real DOM without remounting controls, and keeps editing reachable", async () => {
  const desktop = fileURLToPath(new URL("../../../", import.meta.url));
  const cache = resolve(desktop, "node_modules/.cache");
  await mkdir(cache, { recursive: true });
  const output = await mkdtemp(join(cache, "aurora-layout-smoke-"));
  let root;
  try {
    const source = await readFile(new URL("ThemeWorkspace.tsx", import.meta.url), "utf8");
    const compiled = ts.transpileModule(source, { compilerOptions: {
      target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX,
    } });
    await writeFile(join(output, "workspace.mjs"), compiled.outputText);
    const { ThemeWorkspace } = await import(pathToFileURL(join(output, "workspace.mjs")));
    const { window } = parseHTML('<html><body><div id="root"></div></body></html>');
    Object.assign(globalThis, { window, document: window.document, IS_REACT_ACT_ENVIRONMENT: true });
    let mounts = 0;
    function Control() {
      const [value, setValue] = useState(0);
      useEffect(() => { mounts += 1; }, []);
      return React.createElement("button", { "data-region": "content", onClick: () => setValue(value + 1) }, value);
    }
    root = createRoot(document.getElementById("root"));
    const render = async (order, editing = false) => act(async () => root.render(React.createElement(ThemeWorkspace, {
      theme: { layout: { sectionOrder: order } }, editing, className: "workspace",
      header: React.createElement("button", { "data-region": "header" }, "Personalização"),
      decorations: React.createElement("span", { "data-region": "decorations" }, "Decoração"),
      footer: React.createElement("span", { "data-region": "footer" }, "Estado"),
    }, React.createElement(Control))));
    const order = () => Array.from(document.querySelectorAll("[data-region]"), (item) => item.dataset.region);
    await render(["header", "decorations", "content"]);
    await act(async () => document.querySelector('[data-region="content"]').click());
    await render(["content", "decorations", "header"]);
    assert.deepEqual(order(), ["content", "decorations", "header", "footer"]);
    assert.equal(document.querySelector('[data-region="content"]').textContent, "1");
    await render(["content", "decorations", "header"], true);
    assert.deepEqual(order(), ["header", "content", "footer"]);
    assert.equal(document.querySelector('[data-region="content"]').textContent, "1");
    await render(["decorations", "header", "content"]);
    assert.equal(mounts, 1, "theme order and editor entry must retain the existing interactive subtree");
    assert.deepEqual(order(), ["decorations", "header", "content", "footer"]);
  } finally {
    if (root) await act(async () => root.unmount());
    if (!resolve(output).startsWith(`${cache}${sep}`)) throw new Error("Unexpected test cache path");
    await rm(output, { recursive: true, force: true });
  }
});
