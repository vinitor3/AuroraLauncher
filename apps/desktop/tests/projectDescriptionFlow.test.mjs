import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import vm from "node:vm";
import { parseHTML } from "linkedom";
import ts from "typescript";
import * as React from "react";
import { createRoot } from "react-dom/client";
import * as ReactMarkdown from "react-markdown";
import * as remarkGfm from "remark-gfm";
import createDOMPurify from "dompurify";

// Real React effects, events, refs and DOM updates. Linkedom has no browser
// parser suitable for DOMPurify's security checks: fixtures here are trusted
// HTML, and these tests make no claim about sanitization or visual browser QA.
const { window } = parseHTML("<html><body></body></html>");
Object.assign(globalThis, { window, document: window.document, HTMLElement: window.HTMLElement,
  Node: window.Node, IS_REACT_ACT_ENVIRONMENT: true });
const require = createRequire(import.meta.url);
const purifier = createDOMPurify(window);
const filename = new URL("../src/features/catalog/ProjectDescription.tsx", import.meta.url);
const code = ts.transpileModule(readFileSync(filename, "utf8"), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
}).outputText;
const firstParagraph = "First sentence. ".repeat(120).trim();
const secondParagraph = "Second sentence. ".repeat(120).trim();
const fixtureBody = `<h2>Features</h2><p class="first">${firstParagraph}</p><p class="second">${secondParagraph}</p><table><tbody><tr><td>Version</td></tr></tbody></table><pre><code>java -jar launcher.jar</code></pre><a href="https://example.com/mod">Website</a>`;

async function setup(t, overrides = {}) {
  const exports = {};
  vm.runInNewContext(code, {
    exports, document: window.document, AbortController, Error,
    require(name) {
      if (name === "react") return React;
      if (name === "react/jsx-runtime") return require("react/jsx-runtime");
      if (name === "react-markdown") return ReactMarkdown;
      if (name === "remark-gfm") return remarkGfm;
      if (name === "dompurify") return { default: purifier };


      if (name.endsWith(".css")) return {};
      throw new Error(`Unexpected import: ${name}`);
    },
  });
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  let props = { summary: "Summary first.", body: fixtureBody, bodyFormat: "html", loading: false, ...overrides };
  async function render(next = {}) {
    props = { ...props, ...next };
    await React.act(async () => root.render(React.createElement(exports.ProjectDescription, props)));
  }
  await render();
  let mounted = true;
  async function unmount() {
    if (mounted) { mounted = false; await React.act(async () => root.unmount()); container.remove(); }
  }
  t.after(unmount);
  return { container, render };
}

test("exibe a descrição original do autor com links externos seguros", async t => {
  const { container, render } = await setup(t, { loading: true });
  assert.match(container.textContent, /Carregando página completa/);
  await render({ loading: false });
  assert.match(container.textContent, /Features/);
  assert.equal(container.querySelector("a").getAttribute("rel"), "noopener noreferrer");
  await render({ bodyFormat: "markdown", body: "## Outra descrição" });
  assert.equal(container.querySelector("h2").textContent, "Outra descrição");
});
