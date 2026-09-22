import assert from "node:assert/strict";
import test from "node:test";
import { existsSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import vm from "node:vm";
import { parseHTML } from "linkedom";
import ts from "typescript";
import * as React from "react";
import { createRoot } from "react-dom/client";
import * as ReactMarkdown from "react-markdown";
import * as remarkGfm from "remark-gfm";
import createDOMPurify from "dompurify";
import * as translationLibrary from "../src/lib/projectTranslation.ts";

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

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

async function setup(t, overrides = {}) {
  const calls = [];
  const exports = {};
  const translate = (_services, segments, signal) => {
    const response = deferred();
    calls.push({ segments: [...segments], signal, ...response });
    return response.promise;
  };
  vm.runInNewContext(code, {
    exports, document: window.document, AbortController, Error,
    require(name) {
      if (name === "react") return React;
      if (name === "react/jsx-runtime") return require("react/jsx-runtime");
      if (name === "react-markdown") return ReactMarkdown;
      if (name === "remark-gfm") return remarkGfm;
      if (name === "dompurify") return { default: purifier };
      if (name === "../../lib/edge") return { translateProjectSegments: translate };
      if (name === "../../lib/projectTranslation") return translationLibrary;
      if (name.endsWith(".css")) return {};
      throw new Error(`Unexpected import: ${name}`);
    },
  });
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  let props = { services: {}, summary: "Summary first.", body: fixtureBody, bodyFormat: "html", loading: false, ...overrides };
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
  async function clickLabel(label) {
    const button = container.querySelector(`button[aria-label="${label}"]`);
    assert.ok(button, `Button missing: ${label}`);
    await React.act(async () => button.dispatchEvent(new window.Event("click", { bubbles: true })));
  }
  async function resume() {
    const button = [...container.querySelectorAll("button")].find(node => node.textContent === "Continuar tradução");
    assert.ok(button, "Resume action must remain available");
    await React.act(async () => button.dispatchEvent(new window.Event("click", { bubbles: true })));
  }
  async function resolve(index, same = false) {
    assert.ok(calls[index], `Missing request ${index}`);
    await React.act(async () => calls[index].resolve(calls[index].segments.map((text, id) => ({ id, text: same ? text : `Português: ${text}` })).reverse()));
  }
  async function reject(index, message = "Falha temporária no segundo lote") {
    await React.act(async () => calls[index].reject(new Error(message)));
  }
  const translated = () => container.querySelector('[lang="pt-BR"]');
  const status = () => container.querySelector('[role="status"]').textContent;
  return { container, calls, render, unmount, clickLabel, resume, resolve, reject, translated, status };
}

test("React: first translated batch is visible while later text remains original; failure resumes only remaining batches", async (t) => {
  const run = await setup(t);
  await run.clickLabel("Traduzir página para português");
  assert.equal(run.calls.length, 1);
  assert.equal(run.translated(), null);
  await run.resolve(0);
  assert.equal(run.calls.length, 2);
  assert.equal(run.translated().querySelector(".first").textContent, `Português: ${firstParagraph}`);
  assert.equal(run.translated().querySelector(".second").textContent, secondParagraph);
  await run.reject(1);
  assert.match(run.status(), /Tradução pausada/);
  assert.match(run.container.querySelector('[role="alert"]').textContent, /tradução feita até aqui foi mantida/);
  await run.resume();
  assert.deepEqual(run.calls[2].segments, run.calls[1].segments);
  assert.ok(!run.calls[2].segments.includes(firstParagraph), "completed first batch must never be translated again");
  await run.resolve(2);
  assert.equal(run.translated().querySelector(".first").textContent, `Português: ${firstParagraph}`);
  assert.equal(run.translated().querySelector(".second").textContent, `Português: ${secondParagraph}`);
  assert.equal(run.status(), "Tradução automática · Português");
  assert.equal(run.container.querySelector('[role="alert"]'), null);
});

test("React: cancelling in-flight second batch ignores late response and resumes without changing the completed first batch", async (t) => {
  const run = await setup(t);
  await run.clickLabel("Traduzir página para português");
  await run.resolve(0);
  const partial = run.translated().innerHTML;
  await run.clickLabel("Cancelar tradução");
  assert.equal(run.calls[1].signal.aborted, true);
  await run.resume();
  assert.equal(run.calls.length, 3);
  await run.resolve(1);
  assert.equal(run.translated().innerHTML, partial, "late cancelled response cannot publish progress");
  assert.match(run.status(), /^Traduzindo/);
  await run.resolve(2);
  assert.match(run.status(), /Tradução automática/);
  assert.equal(run.translated().querySelector(".second").textContent, `Português: ${secondParagraph}`);
});

test("React: cancellation before first batch can restart and late rejection does not hide the newer request", async (t) => {
  const run = await setup(t);
  await run.clickLabel("Traduzir página para português");
  await run.clickLabel("Cancelar tradução");
  await run.clickLabel("Traduzir página para português");
  assert.equal(run.calls.length, 2);
  await run.reject(0, "old cancelled request");
  assert.equal(run.container.querySelector('[role="alert"]'), null);
  assert.match(run.status(), /^Traduzindo/);
  await run.resolve(1);
  await run.resolve(2);
  assert.match(run.status(), /Tradução automática/);
});

test("React: switching page aborts the previous job and ignores its late response", async (t) => {
  const run = await setup(t);
  await run.clickLabel("Traduzir página para português");
  await run.resolve(0);
  await run.render({ summary: "New page summary", body: "<h1>Other project</h1>" });
  assert.equal(run.calls[1].signal.aborted, true);
  assert.equal(run.translated(), null);
  assert.equal(run.status(), "");
  await run.clickLabel("Traduzir página para português");
  await run.resolve(1);
  assert.equal(run.translated(), null);
  await run.resolve(2);
  assert.match(run.translated().textContent, /Português: Other project/);
  assert.doesNotMatch(run.translated().textContent, /First sentence/);
});

test("React: original is restored byte-for-byte with formatting, code and links after showing translation", async (t) => {
  const run = await setup(t, { body: '<h1 class="heading">Welcome</h1><p>Read <strong>this guide</strong> <a href="https://example.com">online</a>.</p><pre><code>java -jar test.jar</code></pre><table><tbody><tr><td>Latest</td></tr></tbody></table>' });
  const original = run.container.querySelector(".project-description-content");
  const before = original.innerHTML;
  await run.clickLabel("Traduzir página para português");
  await run.resolve(0);
  assert.equal(original.hidden, true);
  assert.equal(original.innerHTML, before);
  assert.equal(run.translated().querySelector("code").textContent, "java -jar test.jar");
  assert.equal(run.translated().querySelector("a").getAttribute("target"), "_blank");
  assert.equal(run.translated().querySelector("a").getAttribute("rel"), "noopener noreferrer");
  assert.equal(run.translated().querySelectorAll("h1, strong, a, pre, code, table, tbody, tr, td").length,
    original.querySelectorAll("h1, strong, a, pre, code, table, tbody, tr, td").length);
  await run.clickLabel("Mostrar texto original");
  assert.equal(original.hidden, false);
  assert.equal(original.innerHTML, before);
  assert.equal(run.translated(), null);
  await run.clickLabel("Traduzir página para português");
  assert.ok(run.translated());
  assert.equal(run.calls.length, 1, "showing a completed translation uses its existing result");
});

test("React: unchanged text is reported honestly and can be retried instead of claiming a translation", async (t) => {
  const run = await setup(t, { body: "<p>Já está em português.</p>", summary: "Descrição" });
  await run.clickLabel("Traduzir página para português");
  await run.resolve(0, true);
  assert.equal(run.translated(), null);
  assert.match(run.status(), /Nenhum texto foi alterado/);
  await run.clickLabel("Traduzir página para português");
  assert.equal(run.calls.length, 2);
  await run.resolve(1);
  assert.match(run.status(), /Tradução automática/);
});

test("React: unmount aborts request and a late success is ignored", async (t) => {
  const run = await setup(t);
  await run.clickLabel("Traduzir página para português");
  await run.unmount();
  assert.equal(run.calls[0].signal.aborted, true);
  await run.resolve(0);
  assert.equal(run.container.innerHTML, "");
  assert.equal(run.calls.length, 1);
});

test("React: image-only description does not call translation or claim invisible changes", async (t) => {
  const run = await setup(t, { summary: "", body: '<img src="https://example.com/banner.png" alt="Japanese banner text">' });
  await run.clickLabel("Traduzir página para português");
  assert.equal(run.calls.length, 0);
  assert.equal(run.translated(), null);
  assert.match(run.status(), /Não há texto disponível.*imagens permanecem no original/);
});

test("CJK translated fragments receive word separation when the source had no spaces", () => {
  const element = document.createElement("div");
  element.innerHTML = `<p>${"文".repeat(900)}</p>`;
  const plan = translationLibrary.prepareProjectTranslation(element);
  assert.equal(plan.segments.length, 2);
  const restored = document.createElement("div");
  restored.innerHTML = plan.apply(["Primeira frase.", "Segunda frase."]);
  assert.equal(restored.textContent, "Primeira frase. Segunda frase.");
});

test("CJK text adjacent to inline formatting gets Portuguese word separation without changing tags", () => {
  const element = document.createElement("div");
  element.innerHTML = '<p>请先安装<strong>Fabric API</strong>然后启动游戏</p>';
  const plan = translationLibrary.prepareProjectTranslation(element);
  const restored = document.createElement("div");
  restored.innerHTML = plan.apply(["Primeiro, instale", "Fabric API", "e inicie o jogo"]);
  assert.equal(restored.textContent, "Primeiro, instale Fabric API e inicie o jogo");
  assert.equal(restored.querySelector("strong").textContent, "Fabric API");
  assert.equal(element.innerHTML, '<p>请先安装<strong>Fabric API</strong>然后启动游戏</p>');
  assert.equal(plan.apply(plan.segments), element.innerHTML, "identity translation must preserve source exactly");
});

// These public-page fixtures are optional and ignored by Git. Their full raw
// Markdown exercises this component's renderer, not App's HTML normalization.
const fixturePath = new URL("../../../.aurora/qa-project-descriptions.json", import.meta.url);
const publicFixtures = existsSync(fixturePath) ? JSON.parse(readFileSync(fixturePath, "utf8")) : [];
const zombieFixturePath = new URL("../../../.aurora/qa-zombie-invade.json", import.meta.url);
if (existsSync(zombieFixturePath)) {
  const zombie = JSON.parse(readFileSync(zombieFixturePath, "utf8"));
  const appSource = ts.createSourceFile("App.tsx", readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8"), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const normalizer = ["safeHttpsUrl", "normalizeProjectMarkdown"].map(name => appSource.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name).getText(appSource)).join("\n");
  const context = vm.createContext({ document, HTMLElement: window.HTMLElement, Node: window.Node, URL,
    // Linkedom does not synthesize html/body around an HTML fragment like a
    // browser's DOMParser does. Wrap this known public Markdown+details fixture
    // to obtain the same body traversal; this is still not browser visual QA.
    DOMParser: class {
      parseFromString(input, type) { return new window.DOMParser().parseFromString(`<html><body>${input}</body></html>`, type); }
    },
  });
  vm.runInContext(ts.transpileModule(normalizer, { compilerOptions: { module: ts.ModuleKind.None, target: ts.ScriptTarget.ES2022 } }).outputText, context);
  const normalized = context.normalizeProjectMarkdown(zombie.body);
  assert.ok(normalized.includes("## 简体中文"));
  assert.ok(normalized.includes("minecraft:overworld"), "normalization must preserve the public pack's command identifiers");
  publicFixtures.push({ slug: zombie.slug + " (App normalized)", summary: zombie.description, body: normalized });
}
for (const fixture of publicFixtures) test(`React public fixture: ${fixture.slug} preserves Markdown structure across progressive batches and restore`, async (t) => {
  const run = await setup(t, { summary: fixture.summary, body: fixture.body, bodyFormat: "markdown" });
  const original = run.container.querySelector(".project-description-content");
  const htmlBefore = original.innerHTML;
  const selector = "h1,h2,h3,h4,h5,h6,p,a,img,strong,em,ol,ul,li,table,thead,tbody,tr,th,td,pre,code";
  const originalTags = [...original.querySelectorAll(selector)].map(node => node.tagName);
  const originalCode = [...original.querySelectorAll("pre,code")].map(node => node.textContent);
  const originalLinks = [...original.querySelectorAll("a")].map(node => node.getAttribute("href"));
  await run.clickLabel("Traduzir página para português");
  let index = 0;
  while (index < run.calls.length) {
    assert.ok(index < 50, "bounded page must terminate");
    assert.ok(run.calls[index].segments.length <= 48);
    assert.ok(run.calls[index].segments.join("").length <= 2500);
    await run.resolve(index++);
  }
  assert.match(run.status(), /Tradução automática/);
  assert.deepEqual([...run.translated().querySelectorAll(selector)].map(node => node.tagName), originalTags);
  assert.deepEqual([...run.translated().querySelectorAll("pre,code")].map(node => node.textContent), originalCode);
  assert.deepEqual([...run.translated().querySelectorAll("a")].map(node => node.getAttribute("href")), originalLinks);
  assert.equal(original.innerHTML, htmlBefore);
  await run.clickLabel("Mostrar texto original");
  assert.equal(original.hidden, false);
  assert.equal(original.innerHTML, htmlBefore);
  t.diagnostic(`${fixture.slug}: ${index} bounded requests; ${originalTags.length} structural elements preserved`);
});
